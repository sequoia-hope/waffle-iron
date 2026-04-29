// MIT License
//
// Copyright (c) 2022 G. Cherchi, M. Livesu, R. Scateni, M. Attene and F. Pellacini
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Triangulation — Cherchi 2020 §5.3 segment insertion and the full
//! per-triangle pipeline.
//!
//! For each intersected triangle: insert interior + edge points via stack-based
//! splitting, then insert constraint segments via topological walk + earcut.
//! The CDT used by `earcut_linear` is **Livesu et al. 2021** (the simplified
//! linear-time earcut adopted by Cherchi 2022 §4).
//!
//! NOTE: "Algorithm 1" in this file refers to the Cherchi 2020 §5.3 segment
//! insertion algorithm, NOT Cherchi 2022 §5 Algorithm 1 (which is the
//! ray-cast in/out classifier — see `boolean/exact_mesh.rs::label_sub_tri_raycast`).
//!
//! Ported from triangulation.cpp/.h in
//! github.com/gcherchi/InteractiveAndRobustMeshBooleans
//! MIT License (c) 2022 Cherchi, Livesu, Scateni, Attene, Pellacini

use std::collections::{HashMap, HashSet};

use geometry_predicates::orient2d;

use super::aux_structure::AuxiliaryStructure;
use super::common::Plane;
use super::fast_trimesh::FastTrimesh;
use super::processing::Orientation;
use super::triangle_soup::TriangleSoup;
use crate::boolean::indirect_predicates::ImplicitPoint;

/// Pair of vertex IDs (matching C++ UIPair).
type UIPair = (usize, usize);

/// Sort edge points along an edge using the dominant axis.
/// The dominant axis is the one where the edge endpoints differ the most.
/// Points are sorted from the edge's v0 toward v1 using `point_compare_on_axis`.
///
/// Ported from triangulation.cpp:40-55 (sortEdgePoints)
fn sort_edge_points(ts: &TriangleSoup, e_id: usize, points: &mut Vec<usize>) {
    if points.len() <= 1 {
        return;
    }
    let (v0, v1) = ts.edge_verts(e_id);
    let c0 = ts.vert(v0);
    let c1 = ts.vert(v1);

    // Find dominant axis: largest absolute difference
    let dx = (c1[0] - c0[0]).abs();
    let dy = (c1[1] - c0[1]).abs();
    let dz = (c1[2] - c0[2]).abs();

    let axis = if dx >= dy && dx >= dz {
        crate::boolean::indirect_predicates::Axis::X
    } else if dy >= dx && dy >= dz {
        crate::boolean::indirect_predicates::Axis::Y
    } else {
        crate::boolean::indirect_predicates::Axis::Z
    };

    points.sort_by(|&a, &b| {
        crate::boolean::indirect_predicates::point_compare_on_axis(
            ts.implicit_point(a),
            ts.implicit_point(b),
            axis,
        )
    });

    // If edge goes from high to low along the axis, reverse the sort
    // so points go from v0 toward v1. Uses indirect comparison for exactness.
    let dir = crate::boolean::indirect_predicates::point_compare_on_axis(
        ts.implicit_point(v0),
        ts.implicit_point(v1),
        axis,
    );
    if dir == std::cmp::Ordering::Greater {
        points.reverse();
    }
}

// ── Main entry point ────────────────────────────────────────────────────

/// Triangulate all intersected triangles.
///
/// For each triangle: if no intersections, pass through unchanged.
/// If has intersections, create a local FastTrimesh and call
/// `triangulate_single_triangle`.
///
/// Returns (new_tris, new_labels) where new_tris is a flat index array
/// (groups of 3) and new_labels is per-triangle label bitset.
///
/// Ported from triangulation.cpp:136-187 (triangulation, sequential path)
#[allow(dead_code)]
pub(crate) fn triangulation(
    ts: &mut TriangleSoup,
    aux: &mut AuxiliaryStructure,
) -> (Vec<usize>, Vec<u32>) {
    let empty_orientations: Vec<Option<Orientation>> = vec![None; ts.num_tris()];
    let (new_tris, new_labels, _parents, _orient) =
        triangulation_with_parents(ts, aux, &empty_orientations);
    (new_tris, new_labels)
}

/// Like `triangulation`, but also returns a per-output-triangle parent ID
/// indicating which input triangle produced each output triangle, plus the
/// PR10 cosurface-orientation vec parallel to `new_labels`.
///
/// `clean_orientations[t_id]` is the cosurface orientation (Cherchi 2020 §5.4 /
/// Hoffmann 1989 §5.3) attached to preprocessed triangle `t_id` by STAGE2 dedup.
/// Each output triangle inherits its parent's orientation.
///
/// Returns `(new_tris, new_labels, parent_tris, new_cosurface_orientation)`.
#[allow(dead_code)]
pub(crate) fn triangulation_with_parents(
    ts: &mut TriangleSoup,
    aux: &mut AuxiliaryStructure,
    clean_orientations: &[Option<Orientation>],
) -> (Vec<usize>, Vec<u32>, Vec<usize>, Vec<Option<Orientation>>) {
    debug_assert_eq!(
        clean_orientations.len(),
        ts.num_tris(),
        "PR10: clean_orientations must be parallel to TriangleSoup tris"
    );

    let mut new_tris: Vec<usize> = Vec::with_capacity(2 * 3 * ts.num_tris());
    let mut new_labels: Vec<u32> = Vec::with_capacity(2 * ts.num_tris());
    let mut parent_tris: Vec<usize> = Vec::with_capacity(2 * ts.num_tris());
    let mut new_cosurface_orientation: Vec<Option<Orientation>> =
        Vec::with_capacity(2 * ts.num_tris());

    let mut tris_to_split: Vec<usize> = Vec::new();

    // `t_id` is used to index multiple parallel structures (`ts`, `aux`,
    // `clean_orientations`); refactoring to enumerate over a single
    // iterator would obscure intent.
    #[allow(clippy::needless_range_loop)]
    for t_id in 0..ts.num_tris() {
        if (aux.triangle_has_intersections(t_id) && aux.triangle_has_actual_intersection_data(t_id))
            || aux.triangle_has_coplanars(t_id)
        {
            tris_to_split.push(t_id);
        } else {
            // Triangle without intersections directly goes to the output list
            let v0 = ts.tri_vert_id(t_id, 0);
            let v1 = ts.tri_vert_id(t_id, 1);
            let v2 = ts.tri_vert_id(t_id, 2);
            let label = ts.tri_label(t_id);
            if std::env::var("CHERCHI_DEBUG").as_deref() == Ok("1") {
                eprintln!(
                    "[cherchi-stage6] out_tri={} site=passthrough parent_t={} verts=[{},{},{}] label={:#06b}",
                    new_labels.len(),
                    t_id,
                    v0,
                    v1,
                    v2,
                    label
                );
            }
            new_tris.push(v0);
            new_tris.push(v1);
            new_tris.push(v2);
            new_labels.push(label);
            parent_tris.push(t_id);
            new_cosurface_orientation.push(clean_orientations[t_id]);
        }
    }

    // Process triangles to split (sequential — no TBB)
    for &t_id in &tris_to_split {
        let tri_ids = [
            ts.tri_vert_id(t_id, 0),
            ts.tri_vert_id(t_id, 1),
            ts.tri_vert_id(t_id, 2),
        ];
        let mut subm = FastTrimesh::new_implicit(
            ts.implicit_point(ts.tri_vert_id(t_id, 0)).clone(),
            ts.implicit_point(ts.tri_vert_id(t_id, 1)).clone(),
            ts.implicit_point(ts.tri_vert_id(t_id, 2)).clone(),
            tri_ids,
            ts.tri_plane(t_id),
        );

        let before = new_labels.len();
        triangulate_single_triangle(
            ts,
            &mut subm,
            t_id,
            aux,
            &mut new_tris,
            &mut new_labels,
            &mut new_cosurface_orientation,
            clean_orientations,
        );
        let after = new_labels.len();
        // All output triangles from this split came from input triangle t_id
        for _ in before..after {
            parent_tris.push(t_id);
        }
    }

    debug_assert_eq!(
        new_cosurface_orientation.len(),
        new_labels.len(),
        "PR10: cosurface orientation must stay parallel to labels"
    );

    (new_tris, new_labels, parent_tris, new_cosurface_orientation)
}

/// Triangulate a single triangle that has intersections.
///
/// 1. Recover points (interior + edge) and segments from AuxiliaryStructure
/// 2. Split triangle using stack-based point insertion
/// 3. Insert constraint segments
/// 4. Handle coplanar pockets or emit output triangles
///
/// Ported from triangulation.cpp:53-134 (triangulateSingleTriangle)
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
fn triangulate_single_triangle(
    ts: &mut TriangleSoup,
    subm: &mut FastTrimesh,
    t_id: usize,
    aux: &mut AuxiliaryStructure,
    new_tris: &mut Vec<usize>,
    new_labels: &mut Vec<u32>,
    new_cosurface_orientation: &mut Vec<Option<Orientation>>,
    clean_orientations: &[Option<Orientation>],
) {
    // ── Points and segments recovery ──
    // Ported from triangulation.cpp:58-76
    let t_points: Vec<usize> = aux.triangle_points_list(t_id).to_vec();

    let e0_id = ts
        .edge_id(subm.vert_orig_id(0), subm.vert_orig_id(1))
        .expect("edge e0 not found");
    let e1_id = ts
        .edge_id(subm.vert_orig_id(1), subm.vert_orig_id(2))
        .expect("edge e1 not found");
    let e2_id = ts
        .edge_id(subm.vert_orig_id(2), subm.vert_orig_id(0))
        .expect("edge e2 not found");

    let mut e0_points: Vec<usize> = aux.edge_points_list(e0_id).to_vec();
    let mut e1_points: Vec<usize> = aux.edge_points_list(e1_id).to_vec();
    let mut e2_points: Vec<usize> = aux.edge_points_list(e2_id).to_vec();

    let mut t_segments: Vec<UIPair> = aux.triangle_segments_list(t_id).to_vec();

    // Guard against falsely-marked triangles: if set_triangle_has_intersections
    // was called before check_triangle_triangle_intersections (matching C++ order)
    // but the classification found no actual data, emit the original triangle
    // unchanged rather than running the splitting/constraint machinery on empty input.
    if t_points.is_empty()
        && e0_points.is_empty()
        && e1_points.is_empty()
        && e2_points.is_empty()
        && t_segments.is_empty()
        && !aux.triangle_has_coplanars(t_id)
    {
        let v0 = subm.vert_orig_id(0);
        let v1 = subm.vert_orig_id(1);
        let v2 = subm.vert_orig_id(2);
        let label = ts.tri_label(t_id);
        if std::env::var("CHERCHI_DEBUG").as_deref() == Ok("1") {
            eprintln!(
                "[cherchi-stage6] out_tri={} site=split-noop parent_t={} verts=[{},{},{}] label={:#06b}",
                new_labels.len(),
                t_id,
                v0,
                v1,
                v2,
                label
            );
        }
        new_tris.push(v0);
        new_tris.push(v1);
        new_tris.push(v2);
        new_labels.push(label);
        new_cosurface_orientation.push(clean_orientations[t_id]);
        return;
    }

    // Sort edge points along each edge using the dominant axis.
    // This matches the C++ sortEdgePoints (triangulation.cpp:40-55).
    sort_edge_points(ts, e0_id, &mut e0_points);
    sort_edge_points(ts, e1_id, &mut e1_points);
    sort_edge_points(ts, e2_id, &mut e2_points);

    let estimated_vert_num =
        3 + t_points.len() + e0_points.len() + e1_points.len() + e2_points.len();
    subm.pre_allocate_space(estimated_vert_num);

    // ── Triangle split ──
    // Ported from triangulation.cpp:88
    split_single_triangle_with_stack(ts, subm, &t_points, &e0_points, &e1_points, &e2_points);

    // ── Constraint segment insertion ──
    // Ported from triangulation.cpp:102
    add_constraint_segments_in_single_triangle(ts, subm, aux, &mut t_segments);

    // ── Output ──
    if aux.triangle_has_coplanars(t_id) {
        // Ported from triangulation.cpp:108-115
        solve_pockets_in_coplanar_triangle(
            subm,
            aux,
            new_tris,
            new_labels,
            new_cosurface_orientation,
            ts.tri_label(t_id),
            clean_orientations[t_id],
        );
    } else {
        // Ported from triangulation.cpp:118-133
        for ti in 0..subm.num_tris() {
            let tri = subm.tri(ti);
            let v0 = subm.vert_orig_id(tri[0]);
            let v1 = subm.vert_orig_id(tri[1]);
            let v2 = subm.vert_orig_id(tri[2]);
            let label = ts.tri_label(t_id);
            if std::env::var("CHERCHI_DEBUG").as_deref() == Ok("1") {
                eprintln!(
                    "[cherchi-stage6] out_tri={} site=split-non-coplanar parent_t={} verts=[{},{},{}] label={:#06b}",
                    new_labels.len(),
                    t_id,
                    v0,
                    v1,
                    v2,
                    label
                );
            }
            new_tris.push(v0);
            new_tris.push(v1);
            new_tris.push(v2);
            new_labels.push(label);
            new_cosurface_orientation.push(clean_orientations[t_id]);
        }
    }
}

// ── Stack-based triangle splitting ──────────────────────────────────────

/// A custom stack for the point-insertion algorithm.
/// Each entry is a vec of vertex IDs: first 3 are the triangle vertices,
/// remaining are points to be inserted into that sub-triangle.
///
/// Ported from custom_stack.h (CustomStack)
struct CustomStack {
    stack: Vec<Vec<usize>>,
    cursor: isize,
}

impl CustomStack {
    fn new(preallocate_size: usize) -> Self {
        let mut stack = Vec::with_capacity(preallocate_size);
        for _ in 0..preallocate_size {
            stack.push(Vec::new());
        }
        Self { stack, cursor: -1 }
    }

    /// Push a sub-triangle (with optional interior points) onto the stack.
    ///
    /// Mirrors C++ `CustomStack::push` at
    /// `gcherchi/FastAndRobustMeshArrangements/code/custom_stack.h:27-35`
    /// (no filter; degeneracy is the algorithm's invariant violation).
    /// Audit finding C-07 in `docs/audits/cherchi_port_audit.md`
    /// (Cluster I cleanup, unblocked by A-01+A-02 exact predicates at
    /// commit 2071510). Pre-fix the Rust port silently dropped degenerate
    /// triples to mask inexact-predicate failures; with exact predicates
    /// landed, no degenerate triple should reach this site. If this
    /// assertion fires, the upstream predicate path or C-01/C-02 guards
    /// are insufficient and require root-cause investigation.
    fn push(&mut self, vec: Vec<usize>) {
        debug_assert!(
            vec.len() < 3 || (vec[0] != vec[1] && vec[0] != vec[2] && vec[1] != vec[2]),
            "CustomStack::push: degenerate sub-triangle (duplicate vertex IDs in {:?}); \
             predicate-path failure or C-01/C-02 guard insufficient",
            &vec[..3.min(vec.len())]
        );
        let idx = (self.cursor + 1) as usize;
        if idx >= self.stack.len() {
            self.stack.push(vec);
        } else {
            self.stack[idx] = vec;
        }
        self.cursor += 1;
    }

    fn pop(&mut self) -> Vec<usize> {
        debug_assert!(self.cursor >= 0, "stack underflow");
        let idx = self.cursor as usize;
        self.cursor -= 1;
        std::mem::take(&mut self.stack[idx])
    }

    fn is_empty(&self) -> bool {
        self.cursor < 0
    }

    /// Find a triangle (first 3 elements matching v0,v1,v2 in any order)
    /// in the stack and return a clone of it.
    ///
    /// Returns None if not found (e.g., the triangle was created with no
    /// remaining points and was never pushed to the stack).
    ///
    /// Ported from custom_stack.h:80-98 (getTriangleFromStack)
    fn get_triangle_from_stack(&self, v0: usize, v1: usize, v2: usize) -> Option<Vec<usize>> {
        if self.cursor < 0 {
            return None;
        }
        for i in (0..=self.cursor as usize).rev() {
            let entry = &self.stack[i];
            if entry.len() >= 3 {
                let (a, b, c) = (entry[0], entry[1], entry[2]);
                let mut sorted_v = [a, b, c];
                sorted_v.sort();
                let mut sorted_t = [v0, v1, v2];
                sorted_t.sort();
                if sorted_v == sorted_t {
                    return Some(entry.clone());
                }
            }
        }
        None
    }
}

/// Split a single triangle by inserting all points (edge + interior) using
/// a stack-based approach. Points on edges are handled by edge splitting,
/// interior points by triangle splitting.
///
/// Ported from triangulation.cpp:228-363 (splitSingleTriangleWithStack).
///
/// Cherchi 2020 §5.3 stack invariant: `curr_tri[3]` is a non-vertex point of
/// the parent triangle `curr_tri[0..3]`. The C++ upstream
/// (`gcherchi/FastAndRobustMeshArrangements/code/triangulation.cpp:228-363`)
/// assumes this and indexes `curr_tri[3]` directly without a pre-scan.
///
/// Audit C-01 in `docs/audits/cherchi_port_audit.md` (Cluster I cleanup,
/// unblocked by A-01+A-02 exact predicates at commit `2071510`). Pre-fix the
/// Rust port had a pre-scan that walked `pt_idx` from 3..len() looking for
/// the first non-vertex candidate and swapped it to position 3 — silently
/// absorbing the invariant violation that C-02's `is_vertex` filter could
/// produce. With exact predicates, valid call paths cannot construct a frame
/// where `curr_tri[3]` coincides (by ID) with `curr_tri[0..3]`, and the
/// coordinate-coincident input check at function entry catches the upstream
/// `add_vert` duplicate that would lead there. If either assert fires, the
/// upstream predicate path or call site has a bug requiring root-cause
/// investigation.
#[allow(dead_code)]
fn split_single_triangle_with_stack(
    ts: &TriangleSoup,
    subm: &mut FastTrimesh,
    points: &[usize],
    e0_points: &[usize],
    e1_points: &[usize],
    e2_points: &[usize],
) {
    if points.is_empty() && e0_points.is_empty() && e1_points.is_empty() && e2_points.is_empty() {
        return;
    }

    // Cherchi 2020 §5.3 input-constraint invariant: no entry in `points`,
    // `e0_points`, `e1_points`, or `e2_points` may coincide by coordinates
    // with any of the parent-triangle vertices. A coincidence here would
    // produce a duplicate `add_vert` below whose new subm-id resolves to a
    // distinct integer but whose coordinates equal an existing tri-vertex —
    // downstream `fast_point_on_line` / `split_edge` would silently corrupt
    // the cavity. The C++ port has no such input; valid Yang-pipeline call
    // paths cannot produce this state with A-01+A-02 exact predicates landed.
    #[cfg(debug_assertions)]
    {
        let tri_v0 = subm.vert(subm.tri_vert_id(0, 0));
        let tri_v1 = subm.vert(subm.tri_vert_id(0, 1));
        let tri_v2 = subm.vert(subm.tri_vert_id(0, 2));
        let assert_non_coincident = |bucket: &[usize], bucket_name: &str| {
            for &p in bucket {
                if let Some(coords) = ts.implicit_point(p).materialize() {
                    debug_assert!(
                        coords != tri_v0 && coords != tri_v1 && coords != tri_v2,
                        "Cherchi 2020 §5.3 invariant violation: input point {} (bucket {}) \
                         coincides by coordinates with a parent-triangle vertex \
                         (coords={:?}); valid Yang call paths cannot produce this — \
                         upstream predicate-path or duplicate add_vert bug",
                        p,
                        bucket_name,
                        coords
                    );
                }
            }
        };
        assert_non_coincident(points, "points");
        assert_non_coincident(e0_points, "e0_points");
        assert_non_coincident(e1_points, "e1_points");
        assert_non_coincident(e2_points, "e2_points");
    }

    let size_p2ins = 3 + points.len() + e0_points.len() + e1_points.len() + e2_points.len();
    let mut stack = CustomStack::new(size_p2ins * 3);

    let mut all_points: Vec<usize> = Vec::with_capacity(size_p2ins);

    // First 3 entries are the original triangle vertices
    all_points.push(subm.tri_vert_id(0, 0));
    all_points.push(subm.tri_vert_id(0, 1));
    all_points.push(subm.tri_vert_id(0, 2));

    // Add edge points — propagate ImplicitPoint from TriangleSoup
    for &p in e0_points {
        let v_pos = subm.add_vert(ts.implicit_point(p).clone(), p);
        all_points.push(v_pos);
    }
    for &p in e1_points {
        let v_pos = subm.add_vert(ts.implicit_point(p).clone(), p);
        all_points.push(v_pos);
    }
    for &p in e2_points {
        let v_pos = subm.add_vert(ts.implicit_point(p).clone(), p);
        all_points.push(v_pos);
    }

    // Add interior points — propagate ImplicitPoint from TriangleSoup
    for &p in points {
        let v_pos = subm.add_vert(ts.implicit_point(p).clone(), p);
        all_points.push(v_pos);
    }

    stack.push(all_points);

    while !stack.is_empty() {
        let curr_tri = stack.pop();
        if curr_tri.is_empty() {
            continue;
        }
        if curr_tri.len() < 4 {
            // Just a triangle with no points to insert — skip
            continue;
        }

        let mut curr_subdv: Vec<Vec<usize>> = vec![Vec::new(); 4];

        let t_id = match subm.tri_id(curr_tri[0], curr_tri[1], curr_tri[2]) {
            Some(id) => id,
            None => continue,
        };

        // Cherchi 2020 §5.3 stack invariant: `curr_tri[3]` is a non-vertex
        // of `curr_tri[0..3]`. Pre-fix the port walked `pt_idx` from 3..len()
        // skipping vertex-coincident candidates (audit C-01); this masked the
        // upstream invariant violation that C-02's `is_vertex` filter could
        // produce in `reposition_points_in_stack`. The C++ upstream
        // (`triangulation.cpp:281`) indexes `curr_tri[3]` directly with no
        // pre-scan. With C-01+C-02 paired (this commit), the assert encodes
        // the C++ invariant; valid call paths cannot violate it.
        debug_assert!(
            curr_tri[3] != curr_tri[0] && curr_tri[3] != curr_tri[1] && curr_tri[3] != curr_tri[2],
            "Cherchi 2020 §5.3 invariant violation: curr_tri[3]={} coincides with \
             parent-triangle vertex curr_tri[0..3]=[{}, {}, {}] — predicate-path \
             failure in reposition_points_in_stack or upstream add_vert duplicate",
            curr_tri[3],
            curr_tri[0],
            curr_tri[1],
            curr_tri[2]
        );

        let v_pos = curr_tri[3];
        let mut on_edge = false;

        // Merged points buffer — populated with curr_tri's points plus any
        // adjacent triangle's points when splitting on a shared edge.
        // In C++, this is done by mutating `curr_tri` (a reference) in place.
        let mut merged = curr_tri;

        // Check if v_pos is on any edge of the triangle
        // Ported from triangulation.cpp:283-337
        for i in 0..3 {
            let e_id = match subm.tri_edge_id(t_id, i) {
                Some(id) => id,
                None => continue,
            };

            if fast_point_on_line(subm, e_id, v_pos) {
                on_edge = true;

                let v0 = subm.edge_vert_id(e_id, 0);
                let v1 = subm.edge_vert_id(e_id, 1);
                let v_opp = subm.tri_vert_opposite_to(t_id, v0, v1);
                curr_subdv[0] = vec![v0, v_pos, v_opp];
                curr_subdv[1] = vec![v_opp, v_pos, v1];

                // Check adjacent triangle across the edge
                let e2t = subm.adj_e2t(e_id);
                if e2t.len() > 1 {
                    let t_adj_id = if e2t[0] == t_id { e2t[1] } else { e2t[0] };
                    let v_opp2 = subm.tri_vert_opposite_to(t_adj_id, v1, v0);

                    // Get points from the adjacent triangle in the stack.
                    // May be None if the adjacent was created with no remaining
                    // points (size=3) and thus never pushed.
                    if let Some(adj) = stack.get_triangle_from_stack(v1, v_opp2, v0) {
                        for j in 3..adj.len() {
                            let p = adj[j];
                            if p != v_pos && !merged.contains(&p) {
                                merged.push(p);
                            }
                        }
                    }

                    curr_subdv[2] = vec![v_opp2, v_pos, v0];
                    curr_subdv[3] = vec![v1, v_pos, v_opp2];
                }

                // Do the mesh modification FIRST (matching C++ ordering)
                subm.split_edge(e_id, v_pos);
                break;
            }
        }

        if !on_edge {
            // Point is in triangle interior
            // Ported from triangulation.cpp:340-358
            curr_subdv[0] = vec![merged[1], v_pos, merged[0]];
            curr_subdv[1] = vec![merged[2], v_pos, merged[1]];
            curr_subdv[2] = vec![merged[0], v_pos, merged[2]];

            subm.split_tri(t_id, v_pos);
        }

        // Reposition remaining points into sub-triangles and push
        // (matching C++ ordering: AFTER mesh modification)
        // Ported from triangulation.cpp:360-362
        if merged.len() > 4 {
            reposition_points_in_stack(subm, &mut stack, &mut curr_subdv, &merged);
        }
    }
}

/// Redistribute remaining points from curr_tri into the sub-triangles.
///
/// Ported from triangulation.cpp:366-413 (repositionPointsInStack).
///
/// C++ uses non-strict `genericPoint::pointInTriangle` with no `is_vertex`
/// filter on any of the four sub-triangle gates. Audit C-02 in
/// `docs/audits/cherchi_port_audit.md` (Cluster I cleanup, unblocked by
/// A-01+A-02 exact predicates at commit `2071510`). Pre-fix the Rust port
/// gated each `point_in_triangle_projected` call with `!is_vertex(...)`,
/// silently dropping any `curr_tri[4..]` entry whose subm-id matched a
/// vertex of the matched sub-triangle. This was a Cluster I defense paired
/// with C-01's pre-scan: C-02 could push a vertex-coincident frame onto
/// the stack, then C-01 absorbed the resulting invariant violation. With
/// exact predicates landed, no valid upstream call can construct a
/// `curr_tri[4..]` entry that is both vertex-coincident AND inside one of
/// the sub-triangles — the filter becomes a `debug_assert!` per-gate
/// inside the matched-positive branch.
fn reposition_points_in_stack(
    subm: &FastTrimesh,
    stack: &mut CustomStack,
    curr_subdv: &mut [Vec<usize>],
    curr_tri: &[usize],
) {
    for i in 4..curr_tri.len() {
        let p = curr_tri[i];
        let mut n_insertions = 0;

        // The newly inserted point is at curr_subdv[0][1]
        let v_pos_id = curr_subdv[0][1];

        // Check sub-triangle 0
        if !curr_subdv[0].is_empty()
            && point_in_triangle_projected(subm, p, curr_subdv[0][0], v_pos_id, curr_subdv[0][2])
        {
            debug_assert!(
                p != curr_subdv[0][0] && p != curr_subdv[0][1] && p != curr_subdv[0][2],
                "reposition_points_in_stack: point {} in sub-triangle [{}, {}, {}] coincides \
                 with sub-triangle vertex; non-strict point_in_triangle accepted but Cluster I \
                 invariant rejects (predicate-path failure or upstream invariant violation)",
                p,
                curr_subdv[0][0],
                curr_subdv[0][1],
                curr_subdv[0][2]
            );
            n_insertions += 1;
            curr_subdv[0].push(p);
        }

        // Check sub-triangle 1
        if !curr_subdv[1].is_empty()
            && point_in_triangle_projected(subm, p, curr_subdv[1][0], v_pos_id, curr_subdv[1][2])
        {
            debug_assert!(
                p != curr_subdv[1][0] && p != curr_subdv[1][1] && p != curr_subdv[1][2],
                "reposition_points_in_stack: point {} in sub-triangle [{}, {}, {}] coincides \
                 with sub-triangle vertex; non-strict point_in_triangle accepted but Cluster I \
                 invariant rejects (predicate-path failure or upstream invariant violation)",
                p,
                curr_subdv[1][0],
                curr_subdv[1][1],
                curr_subdv[1][2]
            );
            n_insertions += 1;
            curr_subdv[1].push(p);
        }

        if n_insertions == 2 {
            continue;
        }

        // Check sub-triangle 2
        if curr_subdv.len() > 2
            && !curr_subdv[2].is_empty()
            && point_in_triangle_projected(subm, p, curr_subdv[2][0], v_pos_id, curr_subdv[2][2])
        {
            debug_assert!(
                p != curr_subdv[2][0] && p != curr_subdv[2][1] && p != curr_subdv[2][2],
                "reposition_points_in_stack: point {} in sub-triangle [{}, {}, {}] coincides \
                 with sub-triangle vertex; non-strict point_in_triangle accepted but Cluster I \
                 invariant rejects (predicate-path failure or upstream invariant violation)",
                p,
                curr_subdv[2][0],
                curr_subdv[2][1],
                curr_subdv[2][2]
            );
            n_insertions += 1;
            curr_subdv[2].push(p);
        }

        if n_insertions == 2 {
            continue;
        }

        // Check sub-triangle 3
        if curr_subdv.len() > 3
            && !curr_subdv[3].is_empty()
            && point_in_triangle_projected(subm, p, curr_subdv[3][0], v_pos_id, curr_subdv[3][2])
        {
            debug_assert!(
                p != curr_subdv[3][0] && p != curr_subdv[3][1] && p != curr_subdv[3][2],
                "reposition_points_in_stack: point {} in sub-triangle [{}, {}, {}] coincides \
                 with sub-triangle vertex; non-strict point_in_triangle accepted but Cluster I \
                 invariant rejects (predicate-path failure or upstream invariant violation)",
                p,
                curr_subdv[3][0],
                curr_subdv[3][1],
                curr_subdv[3][2]
            );
            curr_subdv[3].push(p);
        }
    }

    for subdv in curr_subdv.iter() {
        if subdv.is_empty() || subdv.len() == 3 {
            continue;
        }
        stack.push(subdv.clone());
    }
}

// ── Constraint segment insertion ────────────────────────────────────────

/// Insert all constraint segments into the local mesh.
///
/// Ported from triangulation.cpp:580-598 (addConstraintSegmentsInSingleTriangle)
#[allow(dead_code)]
fn add_constraint_segments_in_single_triangle(
    ts: &mut TriangleSoup,
    subm: &mut FastTrimesh,
    aux: &mut AuxiliaryStructure,
    segment_list: &mut Vec<UIPair>,
) {
    let orientation = subm.tri_orientation(0);

    let mut sub_segs_map: HashMap<UIPair, UIPair> = HashMap::with_capacity(segment_list.len());

    while let Some(seg) = segment_list.pop() {
        let v0_id = match subm.vert_new_id(seg.0) {
            Some(id) => id,
            None => continue,
        };
        let v1_id = match subm.vert_new_id(seg.1) {
            Some(id) => id,
            None => continue,
        };

        add_constraint_segment(
            ts,
            subm,
            v0_id,
            v1_id,
            orientation,
            aux,
            segment_list,
            &mut sub_segs_map,
        );
    }
}

/// Insert a single constraint segment into the local mesh.
///
/// If the edge already exists, just mark it as constrained.
/// Otherwise: topological walk → cavity boundary → earcut retriangulation.
///
/// Ported from triangulation.cpp:602-645 (addConstraintSegment)
#[allow(dead_code)]
fn add_constraint_segment(
    ts: &mut TriangleSoup,
    subm: &mut FastTrimesh,
    v0_id: usize,
    v1_id: usize,
    orientation: i32,
    aux: &mut AuxiliaryStructure,
    segment_list: &mut Vec<UIPair>,
    sub_segs_map: &mut HashMap<UIPair, UIPair>,
) {
    // Check if edge already exists
    if let Some(e_id) = subm.edge_id(v0_id, v1_id) {
        subm.set_edge_constr(e_id);
        return;
    }

    // Start from the vertex with lowest valence
    let (v_start, v_stop) = if subm.vert_valence(v0_id) < subm.vert_valence(v1_id) {
        (v0_id, v1_id)
    } else {
        (v1_id, v0_id)
    };

    let mut intersected_edges: Vec<usize> = Vec::new();
    let mut intersected_tris: Vec<usize> = Vec::new();

    find_intersecting_elements(
        ts,
        subm,
        v_start,
        v_stop,
        &mut intersected_edges,
        &mut intersected_tris,
        aux,
        segment_list,
        sub_segs_map,
    );

    if intersected_edges.is_empty() {
        return;
    }

    // Walk along the border — forward and reverse
    // Ported from triangulation.cpp:625-627
    let h0 = boundary_walker(subm, v_start, v_stop, &intersected_tris, &intersected_edges);
    let h1 = boundary_walker_reverse(subm, v_stop, v_start, &intersected_tris, &intersected_edges);

    debug_assert!(h0.len() >= 3, "h0 too small");
    debug_assert!(h1.len() >= 3, "h1 too small");

    // Earcut both halves
    let mut new_tri_verts: Vec<usize> = Vec::new();
    earcut_linear(subm, &h0, &mut new_tri_verts, orientation);
    earcut_linear(subm, &h1, &mut new_tri_verts, orientation);

    // Add new triangles
    let mut i = 0;
    while i < new_tri_verts.len() {
        subm.add_tri(new_tri_verts[i], new_tri_verts[i + 1], new_tri_verts[i + 2]);
        i += 3;
    }

    // Remove intersected triangles (sorted descending)
    let mut sorted_tris = intersected_tris.clone();
    sorted_tris.sort_unstable_by(|a, b| b.cmp(a));
    for &t_id in &sorted_tris {
        subm.remove_tri(t_id);
    }

    // Mark the new edge as constrained.
    if let Some(e_id) = subm.edge_id(v_start, v_stop) {
        subm.set_edge_constr(e_id);
    }
}

// ── Topological walk to find intersecting elements ──────────────────────

/// Find all edges and triangles crossed by the segment (v_start, v_stop).
///
/// Walk through the mesh topology, collecting intersected edges and triangles
/// in order. Handles collinear vertices (splits segment) and constrained
/// edge crossings (creates TPI, splits edge, recurses).
///
/// Ported from triangulation.cpp:649-806 (findIntersectingElements)
#[allow(dead_code)]
fn find_intersecting_elements(
    ts: &mut TriangleSoup,
    subm: &mut FastTrimesh,
    v_start: usize,
    v_stop: usize,
    intersected_edges: &mut Vec<usize>,
    intersected_tris: &mut Vec<usize>,
    aux: &mut AuxiliaryStructure,
    segment_list: &mut Vec<UIPair>,
    sub_seg_map: &mut HashMap<UIPair, UIPair>,
) {
    let orig_vstart = subm.vert_orig_id(v_start);
    let orig_vstop = subm.vert_orig_id(v_stop);

    // Find the first edge in link(v_start) that intersects {v_start, v_stop}
    // Ported from triangulation.cpp:656-703
    let adj_tris = subm.adj_v2t(v_start);
    for &t_id in adj_tris.iter() {
        let e_id = subm.edge_opp_to_vert(t_id, v_start);
        let ev0_id = subm.edge_vert_id(e_id, 0);
        let ev1_id = subm.edge_vert_id(e_id, 1);

        if ev0_id == v_stop || ev1_id == v_stop {
            // v_stop is adjacent — shouldn't happen if edge_id check was done
            continue;
        }

        if segments_intersect_inside(subm, v_start, v_stop, ev0_id, ev1_id) {
            intersected_edges.push(e_id);
            intersected_tris.push(t_id);
            break;
        } else if point_inside_segment(subm, v_start, v_stop, ev0_id) {
            // Split segment at ev0_id
            let orig_v0 = subm.vert_orig_id(ev0_id);
            if let Some(edge_id) = subm.edge_id(v_start, ev0_id) {
                subm.set_edge_constr(edge_id);
            }
            segment_list.push((orig_v0, orig_vstop));
            intersected_edges.clear();
            split_segment_in_sub_segments(orig_vstart, orig_vstop, orig_v0, sub_seg_map);
            return;
        } else if point_inside_segment(subm, v_start, v_stop, ev1_id) {
            let orig_v1 = subm.vert_orig_id(ev1_id);
            if let Some(edge_id) = subm.edge_id(v_start, ev1_id) {
                subm.set_edge_constr(edge_id);
            }
            segment_list.push((orig_v1, orig_vstop));
            intersected_edges.clear();
            split_segment_in_sub_segments(orig_vstart, orig_vstop, orig_v1, sub_seg_map);
            return;
        }
    }

    if intersected_edges.is_empty() {
        return;
    }

    // Walk along topology
    // Ported from triangulation.cpp:708-806
    loop {
        let e_id = *intersected_edges.last().unwrap();
        let ev0_id = subm.edge_vert_id(e_id, 0);
        let ev1_id = subm.edge_vert_id(e_id, 1);

        if !subm.edge_is_constr(e_id) {
            // Non-constraint edge
            let t_id = match subm.tri_opp_to_edge(e_id, *intersected_tris.last().unwrap()) {
                Some(id) => id,
                None => {
                    // Boundary edge — can happen with approximate coordinates
                    intersected_edges.clear();
                    return;
                }
            };
            let v2 = subm.tri_vert_opposite_to(t_id, ev0_id, ev1_id);

            if segments_intersect_inside(subm, v_start, v_stop, ev0_id, v2) {
                let int_edge = subm.edge_id(ev0_id, v2).expect("edge not found");
                intersected_edges.push(int_edge);
                intersected_tris.push(t_id);
            } else if segments_intersect_inside(subm, v_start, v_stop, ev1_id, v2) {
                let int_edge = subm.edge_id(ev1_id, v2).expect("edge not found");
                intersected_edges.push(int_edge);
                intersected_tris.push(t_id);
            } else if v2 != v_stop {
                // v2 is collinear with segment — split
                let orig_v2 = subm.vert_orig_id(v2);
                segment_list.push((orig_vstart, orig_v2));
                segment_list.push((orig_v2, orig_vstop));
                intersected_edges.clear();
                split_segment_in_sub_segments(orig_vstart, orig_vstop, orig_v2, sub_seg_map);
                return;
            } else {
                break; // converged (v2 == v_stop)
            }
        } else {
            // Constrained edge crossing — create TPI
            let orig_v0 = subm.vert_orig_id(ev0_id);
            let orig_v1 = subm.vert_orig_id(ev1_id);

            let orig_tpi_id = create_tpi(
                ts,
                subm,
                (orig_vstart, orig_vstop),
                (orig_v0, orig_v1),
                aux,
                sub_seg_map,
            );

            let new_tpi_id = subm.add_vert(ts.implicit_point(orig_tpi_id).clone(), orig_tpi_id);
            subm.split_edge(e_id, new_tpi_id);

            if let Some(edge0_id) = subm.edge_id(ev0_id, new_tpi_id) {
                subm.set_edge_constr(edge0_id);
            }
            if let Some(edge1_id) = subm.edge_id(new_tpi_id, ev1_id) {
                subm.set_edge_constr(edge1_id);
            }

            segment_list.push((orig_vstart, orig_tpi_id));
            segment_list.push((orig_tpi_id, orig_vstop));
            intersected_edges.clear();
            split_segment_in_sub_segments(orig_vstart, orig_vstop, orig_tpi_id, sub_seg_map);
            split_segment_in_sub_segments(orig_v0, orig_v1, orig_tpi_id, sub_seg_map);
            return;
        }
    }

    // Append the last triangle. Per Cherchi 2020 §5.3 cavity-walk termination
    // invariant, when the walk converges (v2 == v_stop), the cavity must close
    // cleanly:
    //   1. intersected_edges and intersected_tris are non-empty.
    //   2. tri_opp_to_edge at the tail returns Some (interior edge, not boundary).
    //   3. The appended triangle is adjacent to v_start or v_stop.
    // Pre-fix this was a silent if-let chain that could produce wrong cavity
    // polygons downstream. Per audit C-05 (Cluster I cleanup, unblocked by
    // A-01+A-02 at commit 08e24d5), now matches C++ at
    // `gcherchi/FastAndRobustMeshArrangements/code/triangulation.cpp:796-805`
    // via debug_assert! macros. test-author proved (commit fba3aeb) that the
    // silent-None case is unreachable through any valid cavity-walk
    // convergence, but the assertions remain as forward-looking guards.
    debug_assert!(
        !intersected_edges.is_empty(),
        "find_intersecting_elements: intersected_edges empty at tail-append"
    );
    debug_assert!(
        !intersected_tris.is_empty(),
        "find_intersecting_elements: intersected_tris empty at tail-append"
    );
    let last_e = *intersected_edges.last().unwrap();
    let last_t = *intersected_tris.last().unwrap();
    let t_id_opt = subm.tri_opp_to_edge(last_e, last_t);
    debug_assert!(
        t_id_opt.is_some(),
        "find_intersecting_elements: tri_opp_to_edge returned None at \
         cavity-walk tail (edge {} on mesh boundary?)",
        last_e
    );
    if let Some(t_id) = t_id_opt {
        debug_assert!(
            subm.tri_contains_vert(t_id, v_start) || subm.tri_contains_vert(t_id, v_stop),
            "find_intersecting_elements: appended triangle {} must contain \
             v_start={} or v_stop={}; cavity-walk convergence invariant violated",
            t_id,
            v_start,
            v_stop
        );
        intersected_tris.push(t_id);
    }
}

// ── Boundary walker ─────────────────────────────────────────────────────

/// Walk the boundary of the cavity from v_start to v_stop (forward direction).
///
/// Ported from triangulation.cpp:812-854 (boundaryWalker, forward iterator)
#[allow(dead_code)]
fn boundary_walker(
    subm: &FastTrimesh,
    v_start: usize,
    v_stop: usize,
    tris: &[usize],
    edges: &[usize],
) -> Vec<usize> {
    let mut h: Vec<usize> = Vec::new();
    h.push(v_start);

    let mut t_idx = 0;
    let mut e_idx = 0;

    loop {
        if t_idx >= tris.len() || e_idx >= edges.len() {
            break;
        }
        let curr_v = *h.last().unwrap();
        if !subm.tri_contains_vert(tris[t_idx], curr_v) {
            break; // stale triangle reference — abort walk
        }
        let off = subm.tri_vert_offset(tris[t_idx], curr_v);
        let mut next_v = subm.tri_vert_id(tris[t_idx], (off + 1) % 3);

        while e_idx < edges.len() && subm.edge_id(curr_v, next_v) == Some(edges[e_idx]) {
            t_idx += 1;
            if t_idx >= tris.len() {
                h.push(v_stop);
                return h;
            }
            if subm.tri_contains_vert(tris[t_idx], v_stop) {
                h.push(v_stop);
                return h;
            }
            e_idx += 1;
            if !subm.tri_contains_vert(tris[t_idx], curr_v) {
                break;
            }
            let off2 = subm.tri_vert_offset(tris[t_idx], curr_v);
            next_v = subm.tri_vert_id(tris[t_idx], (off2 + 1) % 3);
        }

        h.push(next_v);
        t_idx += 1;

        if t_idx >= tris.len() {
            h.push(v_stop);
            return h;
        }

        if subm.tri_contains_vert(tris[t_idx], v_stop) {
            h.push(v_stop);
            return h;
        }

        e_idx += 1;

        if *h.last().unwrap() == v_stop {
            break;
        }
    }

    h
}

/// Walk the boundary in reverse direction (for the other half of the cavity).
///
/// Ported from triangulation.cpp:812-854 (boundaryWalker, reverse iterator)
#[allow(dead_code)]
fn boundary_walker_reverse(
    subm: &FastTrimesh,
    v_start: usize,
    v_stop: usize,
    tris: &[usize],
    edges: &[usize],
) -> Vec<usize> {
    let mut h: Vec<usize> = Vec::new();
    h.push(v_start);

    if tris.is_empty() || edges.is_empty() {
        h.push(v_stop);
        return h;
    }

    let mut t_idx = tris.len() - 1;
    let mut e_idx = edges.len() - 1;

    loop {
        let curr_v = *h.last().unwrap();
        if !subm.tri_contains_vert(tris[t_idx], curr_v) {
            break; // stale triangle reference — abort walk
        }
        let off = subm.tri_vert_offset(tris[t_idx], curr_v);
        let mut next_v = subm.tri_vert_id(tris[t_idx], (off + 1) % 3);

        while subm.edge_id(curr_v, next_v) == Some(edges[e_idx]) {
            if t_idx == 0 {
                h.push(v_stop);
                return h;
            }
            t_idx -= 1;
            if subm.tri_contains_vert(tris[t_idx], v_stop) {
                h.push(v_stop);
                return h;
            }
            if e_idx == 0 {
                break;
            }
            e_idx -= 1;
            if !subm.tri_contains_vert(tris[t_idx], curr_v) {
                break;
            }
            let off2 = subm.tri_vert_offset(tris[t_idx], curr_v);
            next_v = subm.tri_vert_id(tris[t_idx], (off2 + 1) % 3);
        }

        h.push(next_v);

        if t_idx == 0 {
            h.push(v_stop);
            return h;
        }
        t_idx -= 1;

        if subm.tri_contains_vert(tris[t_idx], v_stop) {
            h.push(v_stop);
            return h;
        }

        if e_idx == 0 {
            break;
        }
        e_idx -= 1;

        if *h.last().unwrap() == v_stop {
            break;
        }
    }

    h
}

// ── Earcut triangulation ────────────────────────────────────────────────

/// Linear-time earcut for simple polygons.
///
/// Simplified earcut per **Livesu et al. 2021** (deterministic linear-time
/// CDT). Adopted by Cherchi 2022 §4 as the segment-insertion CDT, replacing
/// the original O(n²) earcut used in Cherchi 2020. (Older comments labeled
/// this "Livesu & Cherchi 2022" — same algorithm, bibliographically the 2021
/// IEEE TVCG paper.) Doubly linked list via prev/next arrays. All internal
/// convex vertices are safe ears.
///
/// Ported from triangulation.cpp:917-1008 (earcutLinear)
#[allow(dead_code)]
fn earcut_linear(subm: &FastTrimesh, poly: &[usize], tris: &mut Vec<usize>, orientation: i32) {
    assert!(poly.len() >= 3, "no valid poly dimension");

    if poly.len() == 3 {
        tris.push(poly[0]);
        tris.push(poly[1]);
        tris.push(poly[2]);
        return;
    }

    let size = poly.len();
    let mut prev: Vec<usize> = (0..size)
        .map(|i| if i == 0 { size - 1 } else { i - 1 })
        .collect();
    let mut next: Vec<usize> = (0..size)
        .map(|i| if i == size - 1 { 0 } else { i + 1 })
        .collect();

    // Keep track of ears
    let mut ears: Vec<usize> = Vec::with_capacity(size);
    let mut is_ear = vec![false; size];

    // Detect all safe ears in O(n)
    // Skip endpoints of the constrained edge (indices 0 and size-1)
    let ref_p = subm.ref_plane();
    for curr in 1..size - 1 {
        let check = custom_orient_2d_indirect(
            subm.implicit_point(poly[prev[curr]]),
            subm.implicit_point(poly[curr]),
            subm.implicit_point(poly[next[curr]]),
            ref_p,
        );

        if prev[curr] != next[curr]
            && ((check > 0 && orientation > 0) || (check < 0 && orientation < 0))
        {
            ears.push(curr);
            is_ear[curr] = true;
        }
    }

    // Progressively delete all ears
    let mut length = size;
    loop {
        if ears.is_empty() {
            break;
        }

        let curr = ears.pop().unwrap();

        tris.push(poly[prev[curr]]);
        tris.push(poly[curr]);
        tris.push(poly[next[curr]]);

        // Exclude curr from the polygon
        next[prev[curr]] = next[curr];
        prev[next[curr]] = prev[curr];

        length -= 1;
        if length < 3 {
            return;
        }

        // Check if prev has become a new ear
        if !is_ear[prev[curr]] && prev[curr] != 0 {
            let check = custom_orient_2d_indirect(
                subm.implicit_point(poly[prev[prev[curr]]]),
                subm.implicit_point(poly[prev[curr]]),
                subm.implicit_point(poly[next[curr]]),
                ref_p,
            );

            if prev[prev[curr]] != next[curr]
                && ((check > 0 && orientation > 0) || (check < 0 && orientation < 0))
            {
                ears.push(prev[curr]);
                is_ear[prev[curr]] = true;
            }
        }

        // Check if next has become a new ear
        if !is_ear[next[curr]] && next[curr] < size - 1 {
            let check = custom_orient_2d_indirect(
                subm.implicit_point(poly[prev[curr]]),
                subm.implicit_point(poly[next[curr]]),
                subm.implicit_point(poly[next[next[curr]]]),
                ref_p,
            );

            if next[next[curr]] != prev[curr]
                && ((check > 0 && orientation > 0) || (check < 0 && orientation < 0))
            {
                ears.push(next[curr]);
                is_ear[next[curr]] = true;
            }
        }
    }
}

/// Standard earcut fallback (non-linear, handles concave cases).
///
/// Ported from triangulation.cpp:858-913 (earcut)
#[allow(dead_code)]
fn earcut(
    subm: &FastTrimesh,
    poly: &mut Vec<usize>,
    tris: &mut Vec<usize>,
    ref_p: Plane,
    orientation: i32,
) {
    if poly.len() < 3 {
        return;
    }

    if poly.len() == 3 {
        for &v_id in poly.iter() {
            tris.push(v_id);
        }
        return;
    }

    loop {
        if poly.len() == 3 {
            for &v_id in poly.iter() {
                tris.push(v_id);
            }
            return;
        }

        let mut found = false;
        let mut i = 1;
        while i < poly.len() - 1 {
            let curr = poly[i];
            let next_v = poly[i + 1];
            let prev_v = poly[i - 1];

            if prev_v == next_v {
                i += 1;
                continue;
            }

            let check = custom_orient_2d_indirect(
                subm.implicit_point(prev_v),
                subm.implicit_point(curr),
                subm.implicit_point(next_v),
                ref_p,
            );

            if (check > 0 && orientation > 0) || (check < 0 && orientation < 0) {
                tris.push(prev_v);
                tris.push(curr);
                tris.push(next_v);

                poly.remove(i);
                found = true;

                if poly.len() == 3 {
                    tris.push(poly[0]);
                    tris.push(poly[1]);
                    tris.push(poly[2]);
                    return;
                }
                // Don't advance i — check the same position again
            } else {
                i += 1;
            }
        }

        if !found {
            break; // No ear found — degenerate polygon
        }
    }
}

// ── TPI creation ────────────────────────────────────────────────────────

/// Create a TPI (Three-Plane Intersection) point where two constraint
/// segments cross.
///
/// Dedup invariant (Cherchi 2020 §5.3, audit finding C-10, C++ upstream
/// `triangulation.cpp:1027-1041`): TPI vertex dedup is required for the
/// segment-insertion algorithm to produce one vertex per shared TPI
/// rather than two coincident vertices. Without dedup, two adjacent
/// triangles whose constraint segments cross at the same TPI produce two
/// distinct but geometrically identical vertices, orphaning one in
/// `rev_vtx_map` and breaking edge-pairing downstream.
///
/// Ported from triangulation.cpp:1012-1042 (createTPI)
#[allow(dead_code)]
fn create_tpi(
    ts: &mut TriangleSoup,
    subm: &FastTrimesh,
    e0: UIPair,
    e1: UIPair,
    aux: &mut AuxiliaryStructure,
    sub_segs_map: &HashMap<UIPair, UIPair>,
) -> usize {
    let t0_ids = [
        subm.vert_orig_id(0),
        subm.vert_orig_id(1),
        subm.vert_orig_id(2),
    ];

    // Find non-coplanar triangle for segment e0
    let tv0 = compute_triangle_of_segment(ts, e0, &t0_ids, aux, sub_segs_map);
    let tv1 = compute_triangle_of_segment(ts, e1, &t0_ids, aux, sub_segs_map);

    // Create TPI implicit point: intersection of three planes
    // Plane 0: the local triangle plane (t0_ids)
    // Plane 1: the triangle supporting segment e0
    // Plane 2: the triangle supporting segment e1
    let tpi = ImplicitPoint::TPI {
        v1: ts.vert(t0_ids[0]),
        v2: ts.vert(t0_ids[1]),
        v3: ts.vert(t0_ids[2]),
        w1: tv0[0],
        w2: tv0[1],
        w3: tv0[2],
        u1: tv1[0],
        u2: tv1[1],
        u3: tv1[2],
    };

    // C-10: dedup against existing TPI vertices via aux.add_vertex_in_sorted_list,
    // mirroring the LPI pattern at intersection_class.rs:432, :472, :510.
    let pos = ts.num_verts();
    let (existing_id, is_new) = aux.add_vertex_in_sorted_list(tpi.clone(), pos);
    if is_new {
        let id = ts.add_impl_point(tpi);
        debug_assert_eq!(id, pos);
        id
    } else {
        existing_id
    }
}

/// Find the supporting triangle for a segment (for TPI computation).
///
/// Looks for a non-coplanar triangle containing the segment.
///
/// Ported from triangulation.cpp:1046-1076 (computeTriangleOfSegment)
fn compute_triangle_of_segment(
    ts: &TriangleSoup,
    seg: UIPair,
    ref_t: &[usize; 3],
    aux: &AuxiliaryStructure,
    sub_segs_map: &HashMap<UIPair, UIPair>,
) -> [[f64; 3]; 3] {
    let e_tris = segment_triangles_list(seg, sub_segs_map, aux);

    // Look for a non-coplanar triangle
    for &t1 in &e_tris {
        let tv1 = [
            ts.tri_vert_id(t1, 0),
            ts.tri_vert_id(t1, 1),
            ts.tri_vert_id(t1, 2),
        ];

        // Skip if same triangle
        let mut sorted_tv1 = tv1;
        sorted_tv1.sort();
        let mut sorted_ref = *ref_t;
        sorted_ref.sort();
        if sorted_tv1 == sorted_ref {
            continue;
        }

        // Check if any vertex of tv1 is not coplanar with ref_t
        let mut copl = true;
        for &v in &tv1 {
            let o = geometry_predicates::orient3d(
                ts.vert(ref_t[0]),
                ts.vert(ref_t[1]),
                ts.vert(ref_t[2]),
                ts.vert(v),
            );
            if o != 0.0 {
                copl = false;
                break;
            }
        }

        if !copl {
            return [ts.vert(tv1[0]), ts.vert(tv1[1]), ts.vert(tv1[2])];
        }
    }

    // Coplanar case — use jolly point
    // Ported from triangulation.cpp:1081-1139
    compute_triangle_of_segment_coplanar(ts, seg, &e_tris, ref_t)
}

/// Coplanar case for computing triangle of segment: find an edge containing
/// both segment endpoints and use a jolly point.
///
/// Ported from triangulation.cpp:1081-1139 (computeTriangleOfSegmentInCoplanarCase)
fn compute_triangle_of_segment_coplanar(
    ts: &TriangleSoup,
    seg: UIPair,
    tris: &[usize],
    ref_t: &[usize; 3],
) -> [[f64; 3]; 3] {
    let (e0, e1) = seg;

    let mut res_v0 = ts.vert(e0);
    let mut res_v1 = ts.vert(e1);

    // Try to find an edge of one of the triangles containing both endpoints
    for &t in tris {
        let tv = [
            ts.tri_vert_id(t, 0),
            ts.tri_vert_id(t, 1),
            ts.tri_vert_id(t, 2),
        ];

        let edges = [(tv[0], tv[1]), (tv[1], tv[2]), (tv[2], tv[0])];
        for &(ev0, ev1) in &edges {
            if point_in_segment_collinear(ts.vert(e0), ts.vert(ev0), ts.vert(ev1))
                && point_in_segment_collinear(ts.vert(e1), ts.vert(ev0), ts.vert(ev1))
            {
                res_v0 = ts.vert(ev0);
                res_v1 = ts.vert(ev1);
                // Find jolly point not coplanar with ref_t
                for jp_id in 0..4 {
                    let o = geometry_predicates::orient3d(
                        ts.vert(ref_t[0]),
                        ts.vert(ref_t[1]),
                        ts.vert(ref_t[2]),
                        *ts.jolly_point(jp_id),
                    );
                    if o != 0.0 {
                        return [res_v0, res_v1, *ts.jolly_point(jp_id)];
                    }
                }
            }
        }
    }

    // Fallback: use segment endpoints + jolly
    for jp_id in 0..4 {
        let o = geometry_predicates::orient3d(
            ts.vert(ref_t[0]),
            ts.vert(ref_t[1]),
            ts.vert(ref_t[2]),
            *ts.jolly_point(jp_id),
        );
        if o != 0.0 {
            return [res_v0, res_v1, *ts.jolly_point(jp_id)];
        }
    }

    panic!("No non-coplanar vtx found to create a triangle");
}

/// Look up the triangle list for a segment, considering sub-segment mapping.
///
/// Ported from triangulation.cpp:1214-1227 (segmentTrianglesList)
fn segment_triangles_list(
    seg: UIPair,
    sub_segments_map: &HashMap<UIPair, UIPair>,
    aux: &AuxiliaryStructure,
) -> Vec<usize> {
    let key_seg = if seg.0 < seg.1 { seg } else { (seg.1, seg.0) };

    if let Some(&ref_seg) = sub_segments_map.get(&key_seg) {
        aux.segment_triangles_list(ref_seg).to_vec()
    } else {
        aux.segment_triangles_list(key_seg).to_vec()
    }
}

/// Split a segment into two sub-segments in the sub_segments_map.
///
/// Ported from triangulation.cpp:1190-1210 (splitSegmentInSubSegments)
fn split_segment_in_sub_segments(
    v_start: usize,
    v_stop: usize,
    mid_point: usize,
    sub_segments_map: &mut HashMap<UIPair, UIPair>,
) {
    let orig_seg = if v_start < v_stop {
        (v_start, v_stop)
    } else {
        (v_stop, v_start)
    };
    let sub_seg0 = if v_start < mid_point {
        (v_start, mid_point)
    } else {
        (mid_point, v_start)
    };
    let sub_seg1 = if v_stop < mid_point {
        (v_stop, mid_point)
    } else {
        (mid_point, v_stop)
    };

    let ref_seg = if let Some(&existing) = sub_segments_map.get(&orig_seg) {
        existing
    } else {
        orig_seg
    };

    sub_segments_map.insert(sub_seg0, ref_seg);
    sub_segments_map.insert(sub_seg1, ref_seg);
}

// ── Coplanar pocket solving ─────────────────────────────────────────────

/// Solve pockets in a coplanar triangle: group sub-triangles by shared
/// non-constrained edges, then deduplicate across coplanar partner.
///
/// Ported from triangulation.cpp:1231-1270 (solvePocketsInCoplanarTriangle)
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
fn solve_pockets_in_coplanar_triangle(
    subm: &FastTrimesh,
    aux: &mut AuxiliaryStructure,
    new_tris: &mut Vec<usize>,
    new_labels: &mut Vec<u32>,
    new_cosurface_orientation: &mut Vec<Option<Orientation>>,
    label: u32,
    orient: Option<Orientation>,
) {
    let (tri_pockets, polygons) = find_pockets_in_triangle(subm);
    debug_assert_eq!(tri_pockets.len(), polygons.len());

    for p_id in 0..polygons.len() {
        let mut curr_p: Vec<usize> = polygons[p_id]
            .iter()
            .map(|&v| subm.vert_orig_id(v))
            .collect();
        curr_p.sort();
        curr_p.dedup();

        let pos = aux.add_visited_polygon_pocket(&curr_p, new_labels.len());

        match pos {
            None => {
                // Pocket not added yet — output its triangles
                for &t in &tri_pockets[p_id] {
                    let tri = subm.tri(t);
                    let v0 = subm.vert_orig_id(tri[0]);
                    let v1 = subm.vert_orig_id(tri[1]);
                    let v2 = subm.vert_orig_id(tri[2]);
                    if std::env::var("CHERCHI_DEBUG").as_deref() == Ok("1") {
                        eprintln!(
                            "[cherchi-stage6] out_tri={} site=pocket-new pocket={} verts=[{},{},{}] label={:#06b}",
                            new_labels.len(),
                            p_id,
                            v0,
                            v1,
                            v2,
                            label
                        );
                    }
                    new_tris.push(v0);
                    new_tris.push(v1);
                    new_tris.push(v2);
                    new_labels.push(label);
                    new_cosurface_orientation.push(orient);
                }
            }
            Some(existing_pos) => {
                // Pocket already present — merge labels
                let num_tris = curr_p.len().saturating_sub(2);
                if std::env::var("CHERCHI_DEBUG").as_deref() == Ok("1") {
                    eprintln!(
                        "[cherchi-stage6] pocket-merge pocket={} existing_pos={} num_tris={} extra_label={:#06b}",
                        p_id, existing_pos, num_tris, label
                    );
                }
                for i in 0..num_tris {
                    new_labels[existing_pos + i] |= label;
                    // PR10: reconcile cosurface orientation. Phase A locked
                    // pocket-merge convention: take Some when the other side
                    // is None; debug_assert_eq when both are Some.
                    new_cosurface_orientation[existing_pos + i] =
                        match (new_cosurface_orientation[existing_pos + i], orient) {
                            (None, Some(o)) => Some(o),
                            (existing, None) => existing,
                            (Some(a), Some(b)) => {
                                debug_assert_eq!(a, b, "PR10: pocket-merge orientation conflict");
                                Some(a)
                            }
                        };
                }
            }
        }
    }
}

/// Find pockets (connected components bounded by constrained/boundary edges)
/// in the local mesh.
///
/// Ported from triangulation.cpp:1274-1321 (findPocketsInTriangle)
fn find_pockets_in_triangle(subm: &FastTrimesh) -> (Vec<Vec<usize>>, Vec<HashSet<usize>>) {
    let mut visited = vec![false; subm.num_tris()];
    let mut tri_pockets: Vec<Vec<usize>> = Vec::new();
    let mut tri_polygons: Vec<HashSet<usize>> = Vec::new();

    for t_seed in 0..subm.num_tris() {
        if visited[t_seed] {
            continue;
        }

        let mut curr_tri_pocket: Vec<usize> = Vec::new();
        let mut curr_tri_poly: HashSet<usize> = HashSet::new();
        let mut stack: Vec<usize> = vec![t_seed];

        while let Some(curr_t) = stack.pop() {
            if visited[curr_t] {
                continue;
            }
            visited[curr_t] = true;
            curr_tri_pocket.push(curr_t);

            let t2e = subm.adj_t2e(curr_t);
            for e in t2e.iter() {
                if subm.edge_is_constr(*e) || subm.edge_is_boundary(*e) {
                    curr_tri_poly.insert(subm.edge_vert_id(*e, 0));
                    curr_tri_poly.insert(subm.edge_vert_id(*e, 1));
                } else {
                    for &t in subm.adj_e2t(*e).iter() {
                        if t != curr_t {
                            stack.push(t);
                        }
                    }
                }
            }
        }

        tri_pockets.push(curr_tri_pocket);
        tri_polygons.push(curr_tri_poly);
    }

    (tri_pockets, tri_polygons)
}

// ── Geometric predicates for local mesh ─────────────────────────────────

/// 2D orientation test projected onto reference plane.
///
/// Ported from triangulation.cpp:1325-1335 (customOrient2D)
fn custom_orient_2d(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3], ref_p: Plane) -> i32 {
    let (i, j) = match ref_p {
        Plane::XY => (0, 1),
        Plane::YZ => (1, 2),
        Plane::ZX => (2, 0),
    };

    let result = orient2d([p0[i], p0[j]], [p1[i], p1[j]], [p2[i], p2[j]]);

    if result > 0.0 {
        1
    } else if result < 0.0 {
        -1
    } else {
        0
    }
}

/// 2D orientation test using indirect predicates (no materialization).
///
/// Uses orient2d_indirect to avoid precision loss from dividing λ by d_L.
fn custom_orient_2d_indirect(
    p0: &crate::boolean::indirect_predicates::ImplicitPoint,
    p1: &crate::boolean::indirect_predicates::ImplicitPoint,
    p2: &crate::boolean::indirect_predicates::ImplicitPoint,
    ref_p: Plane,
) -> i32 {
    let proj = match ref_p {
        Plane::XY => crate::boolean::indirect_predicates::ProjectionAxis::XY,
        Plane::YZ => crate::boolean::indirect_predicates::ProjectionAxis::YZ,
        Plane::ZX => crate::boolean::indirect_predicates::ProjectionAxis::ZX,
    };
    let det = crate::boolean::indirect_predicates::orient2d_indirect(p0, p1, p2, proj);
    if det > 0.0 {
        1
    } else if det < 0.0 {
        -1
    } else {
        0
    }
}

/// Check if point p_id lies on the line through edge e_id.
///
/// Uses orient2d_indirect which is exact for all LPI combinations
/// (EEE, LEE, LLE, LLL) via expansion arithmetic — no materialization
/// needed, no betweenness guard needed.
///
/// Ported from triangulation.cpp:1158-1169 (fastPointOnLine).
fn fast_point_on_line(subm: &FastTrimesh, e_id: usize, p_id: usize) -> bool {
    let ev0_id = subm.edge_vert_id(e_id, 0);
    let ev1_id = subm.edge_vert_id(e_id, 1);

    let p0 = subm.implicit_point(ev0_id);
    let p1 = subm.implicit_point(ev1_id);
    let pp = subm.implicit_point(p_id);

    let proj = match subm.ref_plane() {
        Plane::XY => crate::boolean::indirect_predicates::ProjectionAxis::XY,
        Plane::YZ => crate::boolean::indirect_predicates::ProjectionAxis::YZ,
        Plane::ZX => crate::boolean::indirect_predicates::ProjectionAxis::ZX,
    };
    let orient = crate::boolean::indirect_predicates::orient2d_indirect(p0, p1, pp, proj);
    orient == 0.0
}

/// Check whether edges {e00,e01} and {e10,e11} intersect at a point
/// strictly inside both segments. Uses orient2d_indirect for implicit points.
///
/// Ported from triangulation.cpp:1175-1179 (segmentsIntersectInside)
fn segments_intersect_inside(
    subm: &FastTrimesh,
    e00_id: usize,
    e01_id: usize,
    e10_id: usize,
    e11_id: usize,
) -> bool {
    let proj = match subm.ref_plane() {
        Plane::XY => crate::boolean::indirect_predicates::ProjectionAxis::XY,
        Plane::YZ => crate::boolean::indirect_predicates::ProjectionAxis::YZ,
        Plane::ZX => crate::boolean::indirect_predicates::ProjectionAxis::ZX,
    };

    let o1 = crate::boolean::indirect_predicates::orient2d_indirect(
        subm.implicit_point(e00_id),
        subm.implicit_point(e01_id),
        subm.implicit_point(e10_id),
        proj,
    );
    let o2 = crate::boolean::indirect_predicates::orient2d_indirect(
        subm.implicit_point(e00_id),
        subm.implicit_point(e01_id),
        subm.implicit_point(e11_id),
        proj,
    );
    let o3 = crate::boolean::indirect_predicates::orient2d_indirect(
        subm.implicit_point(e10_id),
        subm.implicit_point(e11_id),
        subm.implicit_point(e00_id),
        proj,
    );
    let o4 = crate::boolean::indirect_predicates::orient2d_indirect(
        subm.implicit_point(e10_id),
        subm.implicit_point(e11_id),
        subm.implicit_point(e01_id),
        proj,
    );

    // Strictly crossing: opposite signs on both
    (o1 > 0.0 && o2 < 0.0 || o1 < 0.0 && o2 > 0.0) && (o3 > 0.0 && o4 < 0.0 || o3 < 0.0 && o4 > 0.0)
}

/// Check if point p_id lies strictly inside segment (ev0_id, ev1_id).
/// Uses orient2d_indirect for collinearity and point_compare_on_axis for
/// betweenness — fully exact, no materialization.
///
/// Ported from triangulation.cpp:1183-1186 (pointInsideSegment).
/// Ref #9: Cherchi 2020 §4.3 (pointCompare / pointInInnerSegment).
fn point_inside_segment(subm: &FastTrimesh, ev0_id: usize, ev1_id: usize, p_id: usize) -> bool {
    let proj = match subm.ref_plane() {
        Plane::XY => crate::boolean::indirect_predicates::ProjectionAxis::XY,
        Plane::YZ => crate::boolean::indirect_predicates::ProjectionAxis::YZ,
        Plane::ZX => crate::boolean::indirect_predicates::ProjectionAxis::ZX,
    };

    // Must be collinear (exact for all LPI combinations)
    let o = crate::boolean::indirect_predicates::orient2d_indirect(
        subm.implicit_point(ev0_id),
        subm.implicit_point(ev1_id),
        subm.implicit_point(p_id),
        proj,
    );
    if o != 0.0 {
        return false;
    }

    // Must be strictly between a and b — use indirect point comparison.
    // Project onto dominant axis of the segment (the axis with largest spread).
    let pa = subm.implicit_point(ev0_id);
    let pb = subm.implicit_point(ev1_id);
    let pp = subm.implicit_point(p_id);

    let (i_axis, j_axis) = match subm.ref_plane() {
        Plane::XY => (
            crate::boolean::indirect_predicates::Axis::X,
            crate::boolean::indirect_predicates::Axis::Y,
        ),
        Plane::YZ => (
            crate::boolean::indirect_predicates::Axis::Y,
            crate::boolean::indirect_predicates::Axis::Z,
        ),
        Plane::ZX => (
            crate::boolean::indirect_predicates::Axis::Z,
            crate::boolean::indirect_predicates::Axis::X,
        ),
    };

    // Check betweenness on the i-axis first
    let cmp_ai = crate::boolean::indirect_predicates::point_compare_on_axis(pa, pp, i_axis);
    let cmp_bi = crate::boolean::indirect_predicates::point_compare_on_axis(pb, pp, i_axis);

    // If a and b differ on i-axis, p must be strictly between them
    if cmp_ai != std::cmp::Ordering::Equal || cmp_bi != std::cmp::Ordering::Equal {
        // p strictly between a and b on i-axis: one is Less, other is Greater
        return (cmp_ai == std::cmp::Ordering::Less && cmp_bi == std::cmp::Ordering::Greater)
            || (cmp_ai == std::cmp::Ordering::Greater && cmp_bi == std::cmp::Ordering::Less);
    }

    // i-axis is degenerate (a, b, p same on i-axis), check j-axis
    let cmp_aj = crate::boolean::indirect_predicates::point_compare_on_axis(pa, pp, j_axis);
    let cmp_bj = crate::boolean::indirect_predicates::point_compare_on_axis(pb, pp, j_axis);

    (cmp_aj == std::cmp::Ordering::Less && cmp_bj == std::cmp::Ordering::Greater)
        || (cmp_aj == std::cmp::Ordering::Greater && cmp_bj == std::cmp::Ordering::Less)
}

/// Projected inner segment cross: two segments cross strictly interior.
/// Uses orient2d on the reference plane.
fn inner_segments_cross(
    a0: &[f64; 3],
    a1: &[f64; 3],
    b0: &[f64; 3],
    b1: &[f64; 3],
    plane: Plane,
) -> bool {
    let (i, j) = match plane {
        Plane::XY => (0, 1),
        Plane::YZ => (1, 2),
        Plane::ZX => (2, 0),
    };

    let pa0 = [a0[i], a0[j]];
    let pa1 = [a1[i], a1[j]];
    let pb0 = [b0[i], b0[j]];
    let pb1 = [b1[i], b1[j]];

    let o1 = orient2d(pa0, pa1, pb0);
    let o2 = orient2d(pa0, pa1, pb1);
    let o3 = orient2d(pb0, pb1, pa0);
    let o4 = orient2d(pb0, pb1, pa1);

    // Strictly crossing: opposite signs on both
    (o1 > 0.0 && o2 < 0.0 || o1 < 0.0 && o2 > 0.0) && (o3 > 0.0 && o4 < 0.0 || o3 < 0.0 && o4 > 0.0)
}

/// Check if point p lies strictly inside segment (a, b), projected.
fn point_in_inner_segment(p: &[f64; 3], a: &[f64; 3], b: &[f64; 3], plane: Plane) -> bool {
    let (i, j) = match plane {
        Plane::XY => (0, 1),
        Plane::YZ => (1, 2),
        Plane::ZX => (2, 0),
    };

    // Must be collinear
    let o = orient2d([a[i], a[j]], [b[i], b[j]], [p[i], p[j]]);
    if o != 0.0 {
        return false;
    }

    // Must be between a and b (strictly)
    let min_x = a[i].min(b[i]);
    let max_x = a[i].max(b[i]);
    let min_y = a[j].min(b[j]);
    let max_y = a[j].max(b[j]);

    p[i] > min_x && p[i] < max_x || (min_x == max_x && p[j] > min_y && p[j] < max_y)
}

/// Point-in-triangle test projected onto the reference plane.
/// Uses orient2d_indirect for exact handling of implicit points.
/// Uses the genericPoint::pointInTriangle semantics (non-strict).
fn point_in_triangle_projected(
    subm: &FastTrimesh,
    p_id: usize,
    v0_id: usize,
    v1_id: usize,
    v2_id: usize,
) -> bool {
    let proj = match subm.ref_plane() {
        Plane::XY => crate::boolean::indirect_predicates::ProjectionAxis::XY,
        Plane::YZ => crate::boolean::indirect_predicates::ProjectionAxis::YZ,
        Plane::ZX => crate::boolean::indirect_predicates::ProjectionAxis::ZX,
    };

    let p = subm.implicit_point(p_id);
    let a = subm.implicit_point(v0_id);
    let b = subm.implicit_point(v1_id);
    let c = subm.implicit_point(v2_id);

    let o1 = crate::boolean::indirect_predicates::orient2d_indirect(a, b, p, proj);
    let o2 = crate::boolean::indirect_predicates::orient2d_indirect(b, c, p, proj);
    let o3 = crate::boolean::indirect_predicates::orient2d_indirect(c, a, p, proj);

    // Non-strict: >= 0 or all <= 0
    (o1 >= 0.0 && o2 >= 0.0 && o3 >= 0.0) || (o1 <= 0.0 && o2 <= 0.0 && o3 <= 0.0)
}

/// Check if point p lies on segment (a, b) using collinearity (for coplanar case).
fn point_in_segment_collinear(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> bool {
    // Project onto dominant axis
    let dx = (b[0] - a[0]).abs();
    let dy = (b[1] - a[1]).abs();
    let dz = (b[2] - a[2]).abs();

    let axis = if dx >= dy && dx >= dz {
        0
    } else if dy >= dz {
        1
    } else {
        2
    };

    let pv = p[axis];
    let av = a[axis];
    let bv = b[axis];

    pv >= av.min(bv) && pv <= av.max(bv)
}

/// Compute approximate TPI (Three-Plane Intersection) coordinates.
fn compute_tpi_coords(
    plane0: &[[f64; 3]; 3],
    plane1: &[[f64; 3]; 3],
    plane2: &[[f64; 3]; 3],
) -> [f64; 3] {
    let n0 = plane_normal(plane0);
    let n1 = plane_normal(plane1);
    let n2 = plane_normal(plane2);

    let d0 = -(n0[0] * plane0[0][0] + n0[1] * plane0[0][1] + n0[2] * plane0[0][2]);
    let d1 = -(n1[0] * plane1[0][0] + n1[1] * plane1[0][1] + n1[2] * plane1[0][2]);
    let d2 = -(n2[0] * plane2[0][0] + n2[1] * plane2[0][1] + n2[2] * plane2[0][2]);

    // Solve 3x3 system: n0·x = -d0, n1·x = -d1, n2·x = -d2
    let det = n0[0] * (n1[1] * n2[2] - n1[2] * n2[1]) - n0[1] * (n1[0] * n2[2] - n1[2] * n2[0])
        + n0[2] * (n1[0] * n2[1] - n1[1] * n2[0]);

    if det.abs() < 1e-30 {
        // Degenerate — return average of triangle centroids
        let c0 = centroid(plane0);
        let c1 = centroid(plane1);
        let c2 = centroid(plane2);
        return [
            (c0[0] + c1[0] + c2[0]) / 3.0,
            (c0[1] + c1[1] + c2[1]) / 3.0,
            (c0[2] + c1[2] + c2[2]) / 3.0,
        ];
    }

    let inv_det = 1.0 / det;

    let x = (-d0 * (n1[1] * n2[2] - n1[2] * n2[1]) + d1 * (n0[1] * n2[2] - n0[2] * n2[1])
        - d2 * (n0[1] * n1[2] - n0[2] * n1[1]))
        * inv_det;
    let y = (-d0 * (n1[2] * n2[0] - n1[0] * n2[2]) + d1 * (n0[2] * n2[0] - n0[0] * n2[2])
        - d2 * (n0[2] * n1[0] - n0[0] * n1[2]))
        * inv_det;
    let z = (-d0 * (n1[0] * n2[1] - n1[1] * n2[0]) + d1 * (n0[0] * n2[1] - n0[1] * n2[0])
        - d2 * (n0[0] * n1[1] - n0[1] * n1[0]))
        * inv_det;

    [x, y, z]
}

fn plane_normal(tri: &[[f64; 3]; 3]) -> [f64; 3] {
    let u = [
        tri[1][0] - tri[0][0],
        tri[1][1] - tri[0][1],
        tri[1][2] - tri[0][2],
    ];
    let v = [
        tri[2][0] - tri[0][0],
        tri[2][1] - tri[0][1],
        tri[2][2] - tri[0][2],
    ];
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

fn centroid(tri: &[[f64; 3]; 3]) -> [f64; 3] {
    [
        (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0,
        (tri[0][1] + tri[1][1] + tri[2][1]) / 3.0,
        (tri[0][2] + tri[1][2] + tri[2][2]) / 3.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a simple FastTrimesh with one triangle + interior points
    fn make_simple_mesh() -> FastTrimesh {
        FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 10.0, 0.0],
            [0, 1, 2],
            Plane::XY,
        )
    }

    #[test]
    fn test_triangulate_single_triangle_with_segment() {
        // Create a triangle and add an edge point + interior point
        let mut subm = make_simple_mesh();

        // Add an edge point on edge 0 (v0-v1)
        let ep = subm.add_vert(ImplicitPoint::Explicit([5.0, 0.0, 0.0]), 100);

        // Split the triangle at the edge point (on edge 0)
        let e_id = subm.tri_edge_id(0, 0).unwrap();
        subm.split_edge(e_id, ep);

        // Should now have 2 triangles instead of 1
        assert_eq!(subm.num_tris(), 2);
        // Each triangle should share the edge point
        let mut found_ep = 0;
        for t in 0..subm.num_tris() {
            if subm.tri_contains_vert(t, ep) {
                found_ep += 1;
            }
        }
        assert_eq!(found_ep, 2);
    }

    #[test]
    fn test_find_intersecting_elements_walk() {
        // Create a mesh with enough triangles to walk through
        let mut subm = FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 10.0, 0.0],
            [0, 1, 2],
            Plane::XY,
        );

        // Add a midpoint and split to get more triangles
        let mid = subm.add_vert(ImplicitPoint::Explicit([5.0, 3.0, 0.0]), 100);
        subm.split_tri(0, mid);

        // Now we have 3 triangles
        assert_eq!(subm.num_tris(), 3);
    }

    #[test]
    fn test_boundary_walker_produces_polygon() {
        // boundary_walker is designed for cavity traversal during constraint
        // segment insertion. It requires a specific topology (chain of
        // intersected tris and edges) that arises from find_intersecting_elements.
        // Proper integration-level coverage comes via triangulate_single_triangle.
        //
        // Here we verify the early-return path: when the first triangle
        // already contains v_stop, we get [v_start, v_stop].
        let mut subm = FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 10.0, 0.0],
            [0, 1, 2],
            Plane::XY,
        );
        let v3 = subm.add_vert(ImplicitPoint::Explicit([5.0, 5.0, 0.0]), 3);

        // tri0 = original (0,1,2); add tri1 = (0,v3,2)
        subm.add_tri(0, v3, 2);

        // Walk: v_start=1, v_stop=2, tris=[0] (tri0 contains v_stop=2)
        // edge between curr_v=1 and next_v — the "while" at line 765
        // won't execute since edges list is a dummy single edge that won't match.
        let e01 = subm.edge_id(0, v3).unwrap(); // won't match edge(1,next)
        let h = boundary_walker(&subm, 1, 2, &[0, 0], &[e01, e01]);
        assert_eq!(h[0], 1);
        assert!(h.contains(&2));
    }

    #[test]
    fn test_earcut_linear_simple() {
        // Create a mesh and test earcut on a convex polygon
        let subm = FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0, 1, 2],
            Plane::XY,
        );

        // Add more vertices for a convex polygon
        // For a simple triangle (3 vertices), earcut should produce 1 triangle
        let poly = vec![0, 1, 2];
        let mut tris = Vec::new();
        earcut_linear(&subm, &poly, &mut tris, 1);

        assert_eq!(tris.len(), 3, "triangle should produce 3 indices");
        assert_eq!(tris, vec![0, 1, 2]);
    }

    #[test]
    fn test_earcut_linear_quad() {
        // 4-vertex convex polygon should produce 2 triangles
        let mut subm = FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0, 1, 2],
            Plane::XY,
        );
        let v3 = subm.add_vert(ImplicitPoint::Explicit([0.0, 10.0, 0.0]), 3);

        let poly = vec![0, 1, 2, v3];
        let mut tris = Vec::new();
        earcut_linear(&subm, &poly, &mut tris, 1);

        // 2 triangles = 6 indices
        assert_eq!(tris.len(), 6, "quad should produce 6 indices (2 triangles)");
    }

    #[test]
    fn test_custom_orient_2d() {
        let a = [0.0, 0.0, 0.0];
        let b = [10.0, 0.0, 0.0];
        let c = [5.0, 10.0, 0.0];

        // CCW triangle on XY plane
        assert!(custom_orient_2d(a, b, c, Plane::XY) > 0);
        // CW
        assert!(custom_orient_2d(a, c, b, Plane::XY) < 0);
        // Collinear
        let d = [5.0, 0.0, 0.0];
        assert_eq!(custom_orient_2d(a, d, b, Plane::XY), 0);
    }

    #[test]
    fn test_fast_point_on_line() {
        let subm = FastTrimesh::new(
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [5.0, 10.0, 0.0],
            [0, 1, 2],
            Plane::XY,
        );

        // Edge 0 is between v0(0,0,0) and v1(10,0,0)
        let e_id = subm.tri_edge_id(0, 0).unwrap();

        // A point on the line y=0 should be collinear
        // v2 is at (5,10,0) — NOT on the line
        assert!(!fast_point_on_line(&subm, e_id, 2));
    }

    #[test]
    fn test_split_segment_in_sub_segments() {
        let mut map = HashMap::new();
        split_segment_in_sub_segments(0, 10, 5, &mut map);

        assert!(map.contains_key(&(0, 5)));
        assert!(map.contains_key(&(5, 10)));
        assert_eq!(map[&(0, 5)], (0, 10));
        assert_eq!(map[&(5, 10)], (0, 10));
    }

    #[test]
    fn test_custom_stack() {
        let mut stack = CustomStack::new(10);
        assert!(stack.is_empty());

        stack.push(vec![1, 2, 3, 4]);
        assert!(!stack.is_empty());

        let v = stack.pop();
        assert_eq!(v, vec![1, 2, 3, 4]);
        assert!(stack.is_empty());
    }

    /// Audit C-07 (cherchi_port_audit.md): `CustomStack::push` silently
    /// skips degenerate sub-triangles whose first three slots contain a
    /// duplicate vertex ID. The current Rust port at
    /// `triangulation.rs:381-394` filters them out:
    ///
    /// ```ignore
    /// if vec.len() >= 3 && (vec[0] == vec[1] || vec[0] == vec[2] || vec[1] == vec[2]) {
    ///     return;
    /// }
    /// ```
    ///
    /// The C++ upstream at
    /// `gcherchi/FastAndRobustMeshArrangements/code/custom_stack.h:27-35`
    /// has no such filter — it relies on the predicate kernel never
    /// producing a degenerate triple in valid call paths.
    ///
    /// Per audit Cluster I (predicate-kernel symptom-paper-over) and the
    /// post-A-01+A-02 invariant ("with exact predicates this is
    /// unreachable in valid call paths"), the filter must become a
    /// `debug_assert!` so any caller that hits this state crashes loudly
    /// during development instead of silently dropping a sub-triangle
    /// from the cavity-walk stack.
    ///
    /// This is the red phase (FIP §2): pre-fix the test fails with
    /// "test did not panic"; post-fix the `debug_assert!` panics with a
    /// message containing "degenerate sub-triangle".
    #[test]
    #[should_panic(expected = "degenerate sub-triangle")]
    fn test_custom_stack_rejects_degenerate() {
        let mut stack = CustomStack::new(10);
        // Push a degenerate triangle (vec[0] == vec[1] == 1).
        // Pre-fix: filter silently skips → no panic → test fails with
        // "test did not panic as expected".
        // Post-fix: debug_assert! panics with message containing
        // "degenerate sub-triangle".
        stack.push(vec![1, 1, 3]);
    }

    #[test]
    fn test_custom_stack_get_triangle() {
        let mut stack = CustomStack::new(10);
        stack.push(vec![1, 2, 3, 10, 20]);
        stack.push(vec![4, 5, 6, 30]);

        let found = stack.get_triangle_from_stack(3, 1, 2).unwrap();
        assert_eq!(found[0], 1);
        assert_eq!(found[1], 2);
        assert_eq!(found[2], 3);
    }

    /// Audit C-10 (cherchi_port_audit.md): `create_tpi` must dedup identical
    /// TPI inputs against the existing-vertex map. C++ upstream
    /// (`triangulation.cpp:1027-1041` in
    /// github.com/gcherchi/FastAndRobustMeshArrangements) calls
    /// `addVertexInSortedList`; the Rust port has the primitive at
    /// `aux_structure.rs:323::add_vertex_in_sorted_list` (already used at
    /// three LPI sites in `intersection_class.rs:427-445/467-486/505-524`)
    /// but the TPI creation site appends unconditionally via
    /// `ts.add_impl_point(tpi)`.
    ///
    /// Two `create_tpi` calls with identical (e0, e1, t0_ids) inputs must
    /// return the same vertex ID — Cherchi 2020 §5.3 segment insertion
    /// requires TPI dedup so segments crossing at a TPI shared by multiple
    /// triangles converge to a single vertex.
    ///
    /// **Pre-fix behavior**: id1 != id2 (sequential IDs); test fails on the
    /// `assert_eq!`. **Post-fix behavior**: id1 == id2; test passes.
    ///
    /// Fixture: a TriangleSoup with one local triangle (verts 0,1,2 on XY
    /// plane) plus two supporting triangles — one non-coplanar with the
    /// local triangle that shares segment e0 = (0,1), another non-coplanar
    /// that shares segment e1 = (1,2). The AuxiliaryStructure has
    /// `seg2tris` populated for both segments so
    /// `compute_triangle_of_segment` finds a non-coplanar supporting
    /// triangle for each.
    ///
    /// **Geometric note**: in this fixture the three supporting planes
    /// (local tri + e0-supporter + e1-supporter) all pass through v1
    /// because both e0=(0,1) and e1=(1,2) share v1 with the local
    /// triangle. The TPI therefore lands at v1 = (10, 0, 0) exactly.
    /// Post-fix `aux.add_vertex_in_sorted_list` recognises this via
    /// `less_than_indirect` and returns the existing vertex id=1 rather
    /// than creating a fresh point. Dedup-against-existing-vertex is
    /// geometrically equivalent to dedup-against-another-fresh-TPI for
    /// reproducing C-10's signature (two IDs vs one ID).
    #[test]
    fn test_create_tpi_dedups_identical_tpi() {
        // Local triangle (verts 0,1,2) on XY plane.
        // Supporting tri for e0 = (0,1): verts 0,1,3 with v3 above XY plane.
        // Supporting tri for e1 = (1,2): verts 1,2,4 with v4 above XY plane.
        let coords = vec![
            [0.0, 0.0, 0.0],  // v0
            [10.0, 0.0, 0.0], // v1
            [5.0, 10.0, 0.0], // v2
            [5.0, 0.0, 5.0],  // v3 — above XY, non-coplanar with local tri
            [7.5, 5.0, 5.0],  // v4 — above XY, non-coplanar with local tri
        ];
        let tris = vec![
            0, 1, 2, // tri 0 — local
            0, 1, 3, // tri 1 — supports e0 = (0,1)
            1, 2, 4, // tri 2 — supports e1 = (1,2)
        ];
        let labels = vec![0u32, 0u32, 0u32];
        let mut ts = TriangleSoup::new(coords, tris, labels, 1.0);

        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        // Populate seg2tris so compute_triangle_of_segment can find supporting tris.
        // e0 = (0,1) is shared by local tri 0 and supporting tri 1.
        // e1 = (1,2) is shared by local tri 0 and supporting tri 2.
        aux.add_triangles_in_segment((0, 1), 0, 1);
        aux.add_triangles_in_segment((1, 2), 0, 2);

        // Local sub-mesh — the FastTrimesh that create_tpi treats as the
        // "current" triangle. orig_ids must match the local triangle's
        // vertex IDs in TriangleSoup so subm.vert_orig_id(i) returns i.
        let subm = FastTrimesh::new(
            ts.tri_vert(0, 0),
            ts.tri_vert(0, 1),
            ts.tri_vert(0, 2),
            [0, 1, 2],
            Plane::XY,
        );

        let sub_segs_map: HashMap<UIPair, UIPair> = HashMap::new();
        let e0: UIPair = (0, 1);
        let e1: UIPair = (1, 2);

        let id1 = create_tpi(&mut ts, &subm, e0, e1, &mut aux, &sub_segs_map);
        let id2 = create_tpi(&mut ts, &subm, e0, e1, &mut aux, &sub_segs_map);

        // Red-before-green assertion: identical inputs → same vertex ID.
        // Pre-fix: id1 != id2 (sequential, so id2 == id1 + 1).
        // Post-fix (wire in aux.add_vertex_in_sorted_list): id1 == id2.
        assert_eq!(
            id1, id2,
            "create_tpi must dedup identical TPI inputs per Cherchi 2020 §5.3 / \
             triangulation.cpp:1027-1041 / audit C-10. Pre-fix produces id1 != id2 \
             because the TPI creation site calls ts.add_impl_point(tpi) without \
             going through aux.add_vertex_in_sorted_list."
        );
    }

    /// Audit C-05 (cherchi_port_audit.md, Cluster I) — REGRESSION GUARD for
    /// `find_intersecting_elements` tail-append path.
    ///
    /// The Rust port at `triangulation.rs:929-936` ends with a nested if-let
    /// chain that silently skips the tail-triangle append if `tri_opp_to_edge`
    /// returns None. The C++ upstream at
    /// `gcherchi/FastAndRobustMeshArrangements/code/triangulation.cpp:796-805`
    /// has TWO `assert!`s instead:
    ///   1. `assert(t_id != -1 && "tri opposite to edge not found");`
    ///   2. `assert(triContainsVert(t_id, v_start) || triContainsVert(t_id, v_stop));`
    ///
    /// Per Cluster I theme (defensive guards papering over inexact-predicate
    /// fallout) and the post-A-01+A-02 invariant ("the silent-None case is
    /// unreachable in valid pipelines"), C-05's fix replaces the if-let chain
    /// with `debug_assert!` macros mirroring the C++ asserts.
    ///
    /// **Why this is a regression-guard test (Option B per
    /// `/home/claude/.claude/plans/fluttering-rolling-crystal.md`) and not a
    /// `#[should_panic]` red-before-green test (Option A):**
    ///
    /// With A-01+A-02 exact predicates landed (commit 08e24d5), the silent-None
    /// case at line 932 is *mathematically unreachable* through any valid
    /// invocation of `find_intersecting_elements`:
    /// - The walk loop reaches the tail-append only via the `break` at line 894
    ///   (taken only when `v2 == v_stop`).
    /// - `break` is taken inside the `!edge_is_constr(e_id)` branch, AFTER a
    ///   successful `tri_opp_to_edge(e_id, last_t) = Some(t_id)` at line 867.
    /// - The tail-append at line 932 calls the SAME `tri_opp_to_edge` with the
    ///   SAME arguments (mesh state unchanged between break and tail-append,
    ///   `intersected_tris` not pushed in the `v2 == v_stop` branch).
    /// - `tri_opp_to_edge` is pure (`fast_trimesh.rs:464-482`); it returns the
    ///   same value for the same arguments.
    ///
    /// Therefore Option A's `#[should_panic]` test is impossible to construct
    /// without injecting semantically-impossible state (e.g. directly fabricating
    /// `intersected_edges` containing a boundary edge — but the loop body's
    /// line-867 success precondition would prevent that input from reaching
    /// tail-append). Option B (this test) instead pins down the *correct
    /// post-walk shape* of `intersected_edges` / `intersected_tris` for a
    /// canonical 2-triangle walk, ensuring the C-05 fix preserves it.
    ///
    /// Pre-fix AND post-fix behavior on this fixture:
    ///   - intersected_edges = [edge(1,2)]
    ///   - intersected_tris  = [T0, T1]
    /// The fix is behavior-preserving for valid input; this test would catch a
    /// regression where the fix is misapplied (e.g. the `unwrap()` panics or
    /// the appended `t_id` is wrong).
    ///
    /// Fixture geometry — quad split into 2 CCW triangles on Plane::XY:
    ///   v3(2,2)              v2(0,2)
    ///       *--------------------*
    ///       |\         T1       /|  T1 = (1,3,2)
    ///       |  \              /  |
    ///       |    \          /    |
    ///       |      \      /      |
    ///       |   T0   \  /        |  T0 = (0,1,2)
    ///       |          *         |
    ///   v0(0,0)     edge(1,2)    v1(2,0)
    ///
    /// Walk from v_start = v0 (=0) to v_stop = v3 (=3):
    ///   - Initial scan over adj_v2t(0) = [T0]:
    ///       edge_opp_to_vert(T0, 0) = edge(1,2). ev0=1, ev1=2.
    ///       segments_intersect_inside([0,3], 1, 2) → TRUE (cross at (1,1)).
    ///       push intersected_edges = [edge(1,2)], intersected_tris = [T0].
    ///   - Loop iter 1:
    ///       e_id = edge(1,2). edge_is_constr → false.
    ///       tri_opp_to_edge(edge(1,2), T0) = Some(T1). t_id = T1.
    ///       v2 = tri_vert_opposite_to(T1, 1, 2) = vertex 3.
    ///       v2 == v_stop → break.
    ///   - Tail-append:
    ///       tri_opp_to_edge(edge(1,2), T0) = Some(T1) → push T1.
    ///       (Pre-fix: silent if-let chain succeeds → push T1.
    ///        Post-fix: debug_assert!(Some).is_some() passes →
    ///                  debug_assert!(tri_contains_vert(T1, v_start=0) ||
    ///                                tri_contains_vert(T1, v_stop=3)) →
    ///                  T1 = (1,3,2), contains 3 → passes → push T1.
    ///        Both: same final state.)
    #[test]
    fn test_find_intersecting_elements_tail_append_regression_guard() {
        // ── Build the FastTrimesh: quad split into T0 + T1. ──────────────
        let mut subm = FastTrimesh::new(
            [0.0, 0.0, 0.0], // v0
            [2.0, 0.0, 0.0], // v1
            [0.0, 2.0, 0.0], // v2
            [0, 1, 2],
            Plane::XY,
        );
        // FastTrimesh::new gives us v0,v1,v2 + T0=(0,1,2). Add v3 + T1=(1,3,2).
        let v3 = subm.add_vert(ImplicitPoint::Explicit([2.0, 2.0, 0.0]), 3);
        let t1 = subm.add_tri(1, v3, 2).expect("T1=(1,3,2) should be added");
        assert_eq!(subm.num_tris(), 2, "fixture must have exactly 2 triangles");
        assert_eq!(t1, 1, "T1's id must be 1");

        // The shared edge is edge(1,2). Verify it's manifold (e2t.len() == 2).
        let e_shared = subm.edge_id(1, 2).expect("edge (1,2) must exist");
        assert!(
            subm.edge_is_manifold(e_shared),
            "edge(1,2) must be shared between T0 and T1 (cavity walk \
             precondition; without manifoldness the loop's tri_opp_to_edge \
             returns None at line 867 → early return → tail-append never \
             reached)"
        );

        // ── Build a minimal TriangleSoup for the function signature. ─────
        // The walk's normal path doesn't allocate TPIs, so `ts` is unused
        // except by `subm.vert_orig_id` (which doesn't touch ts). A small
        // valid TriangleSoup keeps invariants satisfied.
        let coords = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [2.0, 2.0, 0.0],
        ];
        let tris = vec![0, 1, 2, 1, 3, 2];
        let labels = vec![0u32, 0u32];
        let mut ts = TriangleSoup::new(coords, tris, labels, 1.0);
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);

        // ── Invoke find_intersecting_elements directly. ─────────────────
        let mut intersected_edges: Vec<usize> = Vec::new();
        let mut intersected_tris: Vec<usize> = Vec::new();
        let mut segment_list: Vec<UIPair> = Vec::new();
        let mut sub_seg_map: HashMap<UIPair, UIPair> = HashMap::new();

        find_intersecting_elements(
            &mut ts,
            &mut subm,
            0,  // v_start = v0
            v3, // v_stop  = v3
            &mut intersected_edges,
            &mut intersected_tris,
            &mut aux,
            &mut segment_list,
            &mut sub_seg_map,
        );

        // ── Assert the cavity-walk's terminal state. ────────────────────
        // Per Cherchi 2020 §5.3 (segment insertion), the walk must produce
        // a chain of intersected edges + adjacent triangles. For this fixture:
        //   - exactly 1 intersected edge (the shared diagonal edge(1,2));
        //   - exactly 2 triangles (T0 entered, T1 appended at the tail).
        //
        // Pre-fix (silent if-let): T1 is appended → intersected_tris.len() == 2.
        // Post-fix (debug_assert + Option<usize>): same — T1 is appended.
        // A future regression that drops the tail-append (intersected_tris.len()
        // == 1) or pushes the wrong triangle would fail this test.
        assert_eq!(
            intersected_edges.len(),
            1,
            "single-crossing walk must produce exactly 1 intersected edge \
             (the shared diagonal); got {:?}",
            intersected_edges
        );
        assert_eq!(
            intersected_edges[0], e_shared,
            "the intersected edge must be the shared diagonal edge(1,2)"
        );
        assert_eq!(
            intersected_tris.len(),
            2,
            "tail-append must produce exactly 2 intersected triangles \
             (T0 from initial scan + T1 from tail-append per C-05); got {:?}. \
             A drop to len()==1 would mean the tail-append was silently \
             skipped — exactly the C-05 silent-skip regression.",
            intersected_tris
        );
        assert_eq!(
            intersected_tris[0], 0,
            "first intersected triangle must be T0 (=0) from initial scan"
        );
        assert_eq!(
            intersected_tris[1], 1,
            "tail-appended triangle must be T1 (=1), the one opposite \
             edge(1,2) from T0. Per C++ triangulation.cpp:796-805, the \
             appended triangle must contain v_start (=0) or v_stop (={}); \
             T1=(1,3,2) contains v_stop=3.",
            v3
        );

        // Sanity check the C-05 second-assert invariant directly: the
        // appended triangle contains v_start or v_stop.
        let appended = intersected_tris[1];
        assert!(
            subm.tri_contains_vert(appended, 0) || subm.tri_contains_vert(appended, v3),
            "C-05 invariant: appended triangle must contain v_start or v_stop \
             (cf. C++ triangulation.cpp:802-803 second assert). \
             Triangle {} fails this — fixture geometry corrupt.",
            appended
        );
    }

    /// Audit C-01 (cherchi_port_audit.md, Cluster I): `split_single_triangle_with_stack`
    /// adds a pre-scan over `curr_tri[3..]` that walks past vertex-coincident
    /// candidates and swaps the first non-vertex into position 3 (or silently
    /// `continue`s if none is found). The C++ upstream at
    /// `gcherchi/FastAndRobustMeshArrangements/code/triangulation.cpp:228-363`
    /// has no such pre-scan — it relies on Cherchi 2020 §5.3's invariant that
    /// `curr_tri[3]` is a non-vertex point of the popped sub-triangle.
    ///
    /// The Rust pre-scan at `triangulation.rs:519-529`:
    ///
    /// ```ignore
    /// let mut pt_idx = 3;
    /// while pt_idx < curr_tri.len() {
    ///     let p = curr_tri[pt_idx];
    ///     if p != curr_tri[0] && p != curr_tri[1] && p != curr_tri[2] { break; }
    ///     pt_idx += 1;
    /// }
    /// if pt_idx >= curr_tri.len() { continue; }  // silent skip
    /// ```
    ///
    /// is a Cluster I (predicate-kernel symptom-paper-over) defense — pre-fix
    /// it masks any invariant violation produced upstream by the inexact-
    /// predicate reposition path (C-02). With A-01+A-02 exact predicates
    /// landed (commit `2071510`) and the C-02 filter promoted to a
    /// `debug_assert`, no valid invocation of the Cherchi 2020 §5.3 algorithm
    /// can construct a stack frame whose `curr_tri[3]` coincides with
    /// `curr_tri[0..3]` — the pre-scan must therefore become a
    /// `debug_assert!` mirroring the C++ invariant.
    ///
    /// **Direct invariant violation strategy** (mirroring C-07's
    /// `CustomStack::push(vec![1, 1, 3])` and C-13's `edge_id(0, 0)`):
    /// `split_single_triangle_with_stack` is the only public entry into the
    /// stack loop and constructs its own internal `CustomStack`; the
    /// invariant we need to violate is *internal* to that loop. This test
    /// invokes the private `split_single_triangle_with_stack` directly with
    /// `points = [vert_id_0]` — i.e., it asks the algorithm to "insert" a
    /// point whose TriangleSoup orig_id coincides with the sub-mesh's
    /// triangle vertex 0. The function then calls
    /// `subm.add_vert(ts.implicit_point(0), 0)`, which appends a new vertex
    /// at subm-id `3` whose `orig_v_id == 0`. The initial stack frame becomes
    /// `[0, 1, 2, 3]` (clean), but reposition sub-trees produced from this
    /// duplicate-orig_id state are the implementer's responsibility to
    /// surface as an invariant violation.
    ///
    /// **The implementer's debug_assert (per the audit's stated fix) lives
    /// at the loop's pop-and-process site**, asserting `curr_tri[3]` is
    /// non-vertex of `curr_tri[0..3]`. To make this test pass post-fix, the
    /// implementer must arrange for the debug_assert to fire on the
    /// coincidence pattern this fixture produces. If the implementer's
    /// natural placement does not catch this fixture's coincidence, the
    /// implementer should restructure the test fixture to one that triggers
    /// the natural assert site (e.g., a synthetic `CustomStack`-based
    /// scenario that the loop body can be refactored to share).
    ///
    /// Pre-fix (this commit, on red): the pre-scan walks past any
    /// vertex-coincident curr_tri[3] candidates; `#[should_panic]` reports
    /// "test did not panic as expected".
    ///
    /// Post-fix (T2 implementer, distinct agent per FIP P5): the
    /// `debug_assert!` panics with a message containing "Cherchi 2020 §5.3
    /// invariant violation" and this test goes green.
    ///
    /// **Tradeoff note**: Constructing the `curr_tri[3] == curr_tri[0]`
    /// state strictly through the public `split_single_triangle_with_stack`
    /// arguments is hard because (a) `add_vert` is monotonic so initial
    /// stack frames have unique IDs, and (b) the pre-fix C-02 `is_vertex`
    /// filter blocks any reposition-pushed frame from carrying a
    /// vertex-coincident pos-3. The fixture below feeds a duplicate-orig_id
    /// `points` array; whether this triggers the implementer's natural
    /// debug_assert site depends on the implementer's chosen placement.
    /// See report to lead for tradeoff details.
    #[test]
    #[should_panic(expected = "Cherchi 2020 §5.3 invariant violation")]
    fn test_split_single_triangle_rejects_vertex_coincident_pos3() {
        // Build a minimal sub-mesh: one triangle with subm vert IDs [0, 1, 2]
        // and orig_ids matching their indices.
        let mut subm = make_simple_mesh();

        // Build a TriangleSoup whose orig_v_id 0 has the same coordinates as
        // subm's tri vertex 0. Feeding this orig_id into the function's
        // `points` argument creates a sub-mesh state where the inserted
        // point coincides with an existing triangle vertex by coordinates
        // and (post-add_vert) by orig_id mapping. The C-01 invariant
        // violation arises in subsequent iterations of the stack loop when
        // reposition redistributes this point into a sub-triangle whose
        // vertices share the orig_id.
        let coords = vec![
            [0.0, 0.0, 0.0],  // orig_v_id 0 — coincides with subm's tri vert 0
            [10.0, 0.0, 0.0], // orig_v_id 1
            [5.0, 10.0, 0.0], // orig_v_id 2
        ];
        let tris = vec![0, 1, 2];
        let labels = vec![0u32];
        let ts = TriangleSoup::new(coords, tris, labels, 1.0);

        // Invoke the function under test. `points = [0]` injects orig_v_id 0
        // (coincident with subm's tri vert 0) as an interior point. Any
        // reposition push that carries this orig_id-coincident vertex into
        // a sub-triangle's [4..] slot will violate the Cherchi 2020 §5.3
        // invariant on the next pop.
        //
        // Pre-fix: the pre-scan at lines 519-529 silently absorbs the
        // violation → no panic → `#[should_panic]` reports "test did not
        // panic as expected".
        // Post-fix: the implementer's debug_assert! at the loop-pop site
        // panics with "Cherchi 2020 §5.3 invariant violation".
        let points: Vec<usize> = vec![0];
        let e0_points: Vec<usize> = vec![];
        let e1_points: Vec<usize> = vec![];
        let e2_points: Vec<usize> = vec![];
        split_single_triangle_with_stack(
            &ts, &mut subm, &points, &e0_points, &e1_points, &e2_points,
        );
    }

    /// Audit C-02 (cherchi_port_audit.md, Cluster I): `reposition_points_in_stack`
    /// gates each of the 4 `point_in_triangle_projected` calls with
    /// `!is_vertex(&curr_subdv[i], p)`, silently dropping any point in
    /// `curr_tri[4..]` that coincides with a vertex of the matched
    /// sub-triangle. The C++ upstream at
    /// `gcherchi/FastAndRobustMeshArrangements/code/triangulation.cpp:378-403`
    /// uses non-strict `genericPoint::pointInTriangle` (no filter); a
    /// vertex-coincident input produces orient2d == 0 on the matched side, the
    /// non-strict predicate accepts it, and the point is pushed to the
    /// sub-triangle's points-to-redistribute list naturally.
    ///
    /// The Rust filter at `triangulation.rs:631/640/654/668` is a Cluster I
    /// (predicate-kernel symptom-paper-over) defense. Pre-fix it absorbs any
    /// vertex-coincident input that the inexact-predicate path may produce.
    /// With A-01+A-02 exact predicates landed (commit `2071510`), no valid
    /// upstream call constructs a `curr_tri[4..]` entry that is both
    /// vertex-coincident AND geometrically inside one of the sub-triangles —
    /// the filter must therefore become a `debug_assert!` (one per matched
    /// branch) mirroring the C++ algorithm's implicit invariant.
    ///
    /// **Direct invariant violation strategy**: we synthesize a `curr_tri`
    /// whose entry at `[4]` (a `usize` vertex ID) equals one of the vertex
    /// slots of `curr_subdv[0]`. Because `point_in_triangle_projected` looks
    /// up vertex coordinates by ID and runs `orient2d_indirect` on the
    /// projection, a vertex-coincident point produces `orient2d == 0` against
    /// itself on the matched side, and the non-strict predicate
    /// `(o1 >= 0 && o2 >= 0 && o3 >= 0) || (o1 <= 0 && o2 <= 0 && o3 <= 0)`
    /// returns true. This drives the matched branch of sub-triangle 0; the
    /// implementer's `debug_assert!` (replacing the `is_vertex` filter) then
    /// panics with a substring containing "coincides with sub-triangle vertex".
    ///
    /// Pre-fix (this commit, on red): the `is_vertex` filter at line 631
    /// short-circuits the matched branch → no panic → `#[should_panic]`
    /// reports "test did not panic as expected".
    ///
    /// Post-fix (T2 implementer, distinct agent per FIP P5): the
    /// `debug_assert!` panics with a message containing "coincides with
    /// sub-triangle vertex" and this test goes green.
    #[test]
    #[should_panic(expected = "coincides with sub-triangle vertex")]
    fn test_reposition_points_rejects_vertex_coincident_subtri() {
        // Build a minimal sub-mesh — verts 0, 1, 2 forming the original
        // triangle. The implementer's debug_assert path requires the
        // sub-triangle that triggers the matched branch to contain a vertex
        // whose orig_id resolves through subm's vertex array, so we keep the
        // minimal 3-vertex mesh and let `point_in_triangle_projected` inspect
        // them via `subm.implicit_point()`.
        let subm = make_simple_mesh();

        // Allocate a CustomStack — `reposition_points_in_stack` uses
        // `&mut CustomStack` to push the (filtered) sub-triangles back onto
        // the algorithm's worklist. For this test the stack's contents after
        // the call are irrelevant (the panic fires before we'd inspect them);
        // the stack is purely a function-signature requirement.
        let mut stack = CustomStack::new(10);

        // Construct a `curr_subdv` array of 4 sub-triangles whose vertex
        // slots reference the existing subm verts 0, 1, 2. Sub-triangle 0
        // has vertex slot [0] equal to subm vert 0 — the coincidence target.
        //
        // Per `reposition_points_in_stack` line 622, the "newly inserted
        // point" is `curr_subdv[0][1]` (= subm vert 1 in this fixture). The
        // matched-positive branch for sub-triangle 0 then evaluates
        // `point_in_triangle_projected(subm, p, curr_subdv[0][0]=0,
        // v_pos_id=1, curr_subdv[0][2]=2)` — which, when p == 0 (vertex
        // coincident with curr_subdv[0][0]), produces orient2d == 0 on the
        // matched side and returns true under the non-strict predicate.
        let mut curr_subdv: Vec<Vec<usize>> = vec![
            vec![0, 1, 2], // sub-tri 0 — slot [0] = vert 0 (the coincidence target)
            vec![0, 1, 2], // sub-tri 1 — same; only sub-tri 0 needs to match
            vec![],        // sub-tri 2 — empty; reposition skips on len() < 3
            vec![],        // sub-tri 3 — empty
        ];

        // `curr_tri` indices [0..3] are the parent triangle's verts; [3] is
        // the inserted point (skipped by reposition's loop which starts at
        // i=4); [4..] are the points to redistribute. Index [4] is set to
        // subm vert 0 — the synthetic invariant violation. The
        // implementer's debug_assert (post-fix, replacing the line-631
        // `is_vertex` filter) must panic with a message containing
        // "coincides with sub-triangle vertex".
        let curr_tri: Vec<usize> = vec![0, 1, 2, 1, 0];
        //                                     ^   ^- p (= subm vert 0) — coincides with curr_subdv[0][0]
        //                                     |
        //                                     +- inserted point (skipped by the i=4..len() loop)

        // Direct invocation. Pre-fix: `is_vertex` filter at line 631
        // short-circuits → silent skip → `#[should_panic]` fails with "test
        // did not panic as expected". Post-fix: debug_assert! fires.
        reposition_points_in_stack(&subm, &mut stack, &mut curr_subdv, &curr_tri);
    }
}
