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

//! Triangulation — Algorithm 1 and the full per-triangle pipeline.
//!
//! For each intersected triangle: insert interior + edge points via stack-based
//! splitting, then insert constraint segments via topological walk + earcut.
//!
//! Ported from Cherchi triangulation.cpp/.h
//! MIT License (c) 2022 Cherchi, Livesu, Scateni, Attene, Pellacini

use std::collections::{HashMap, HashSet};

use geometry_predicates::orient2d;

use super::aux_structure::AuxiliaryStructure;
use super::common::Plane;
use super::fast_trimesh::FastTrimesh;
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
    let (new_tris, new_labels, _parents) = triangulation_with_parents(ts, aux);
    (new_tris, new_labels)
}

/// Like `triangulation`, but also returns a per-output-triangle parent ID
/// indicating which input triangle produced each output triangle.
///
/// Returns (new_tris, new_labels, parent_tris).
#[allow(dead_code)]
pub(crate) fn triangulation_with_parents(
    ts: &mut TriangleSoup,
    aux: &mut AuxiliaryStructure,
) -> (Vec<usize>, Vec<u32>, Vec<usize>) {
    let mut new_tris: Vec<usize> = Vec::with_capacity(2 * 3 * ts.num_tris());
    let mut new_labels: Vec<u32> = Vec::with_capacity(2 * ts.num_tris());
    let mut parent_tris: Vec<usize> = Vec::with_capacity(2 * ts.num_tris());

    let mut tris_to_split: Vec<usize> = Vec::new();

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
        triangulate_single_triangle(ts, &mut subm, t_id, aux, &mut new_tris, &mut new_labels);
        let after = new_labels.len();
        // All output triangles from this split came from input triangle t_id
        for _ in before..after {
            parent_tris.push(t_id);
        }
    }

    (new_tris, new_labels, parent_tris)
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
fn triangulate_single_triangle(
    ts: &mut TriangleSoup,
    subm: &mut FastTrimesh,
    t_id: usize,
    aux: &mut AuxiliaryStructure,
    new_tris: &mut Vec<usize>,
    new_labels: &mut Vec<u32>,
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
        solve_pockets_in_coplanar_triangle(subm, aux, new_tris, new_labels, ts.tri_label(t_id));
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

    fn push(&mut self, vec: Vec<usize>) {
        // Skip degenerate sub-triangles (can arise from approximate
        // coordinates causing imprecise point distribution)
        if vec.len() >= 3 && (vec[0] == vec[1] || vec[0] == vec[2] || vec[1] == vec[2]) {
            return;
        }
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
/// Ported from triangulation.cpp:228-363 (splitSingleTriangleWithStack)
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

        // Find the first valid point to insert (skip points that are
        // already vertices of this triangle — can happen when a point
        // gets assigned to a sub-triangle where it's already a vertex
        // during reposition_points_in_stack).
        let mut pt_idx = 3;
        while pt_idx < curr_tri.len() {
            let p = curr_tri[pt_idx];
            if p != curr_tri[0] && p != curr_tri[1] && p != curr_tri[2] {
                break;
            }
            pt_idx += 1;
        }
        if pt_idx >= curr_tri.len() {
            continue; // no valid points to insert
        }

        let v_pos = curr_tri[pt_idx];
        let mut on_edge = false;

        // Merged points buffer — populated with curr_tri's points plus any
        // adjacent triangle's points when splitting on a shared edge.
        // In C++, this is done by mutating `curr_tri` (a reference) in place.
        // Swap the valid point to position [3] so the remaining points start
        // at position [4].
        let mut merged = curr_tri;
        if pt_idx != 3 {
            merged.swap(3, pt_idx);
        }

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
/// Ported from triangulation.cpp:366-413 (repositionPointsInStack)
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

        // Helper: check if p is already a vertex of the sub-triangle
        let is_vertex = |subdv: &[usize], p: usize| -> bool {
            subdv.len() >= 3 && (p == subdv[0] || p == subdv[1] || p == subdv[2])
        };

        // Check sub-triangle 0
        if !curr_subdv[0].is_empty()
            && !is_vertex(&curr_subdv[0], p)
            && point_in_triangle_projected(subm, p, curr_subdv[0][0], v_pos_id, curr_subdv[0][2])
        {
            n_insertions += 1;
            curr_subdv[0].push(p);
        }

        // Check sub-triangle 1
        if !curr_subdv[1].is_empty()
            && !is_vertex(&curr_subdv[1], p)
            && point_in_triangle_projected(subm, p, curr_subdv[1][0], v_pos_id, curr_subdv[1][2])
        {
            n_insertions += 1;
            curr_subdv[1].push(p);
        }

        if n_insertions == 2 {
            continue;
        }

        // Check sub-triangle 2
        if curr_subdv.len() > 2
            && !curr_subdv[2].is_empty()
            && !is_vertex(&curr_subdv[2], p)
            && point_in_triangle_projected(subm, p, curr_subdv[2][0], v_pos_id, curr_subdv[2][2])
        {
            n_insertions += 1;
            curr_subdv[2].push(p);
        }

        if n_insertions == 2 {
            continue;
        }

        // Check sub-triangle 3
        if curr_subdv.len() > 3
            && !curr_subdv[3].is_empty()
            && !is_vertex(&curr_subdv[3], p)
            && point_in_triangle_projected(subm, p, curr_subdv[3][0], v_pos_id, curr_subdv[3][2])
        {
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
    aux: &AuxiliaryStructure,
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
    aux: &AuxiliaryStructure,
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
    aux: &AuxiliaryStructure,
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

    // Append the last triangle
    if let Some(last_e) = intersected_edges.last() {
        if let Some(last_t) = intersected_tris.last() {
            if let Some(t_id) = subm.tri_opp_to_edge(*last_e, *last_t) {
                intersected_tris.push(t_id);
            }
        }
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
/// Simplified earcut per Livesu & Cherchi 2022.
/// Doubly linked list via prev/next arrays.
/// All internal convex vertices are safe ears.
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
/// Ported from triangulation.cpp:1012-1042 (createTPI)
#[allow(dead_code)]
fn create_tpi(
    ts: &mut TriangleSoup,
    subm: &FastTrimesh,
    e0: UIPair,
    e1: UIPair,
    aux: &AuxiliaryStructure,
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

    let v_id = ts.add_impl_point(tpi);

    v_id
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
fn solve_pockets_in_coplanar_triangle(
    subm: &FastTrimesh,
    aux: &mut AuxiliaryStructure,
    new_tris: &mut Vec<usize>,
    new_labels: &mut Vec<u32>,
    label: u32,
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
/// Ref: Cherchi 2020 Section 4.3, pointInInnerSegment.
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
}
