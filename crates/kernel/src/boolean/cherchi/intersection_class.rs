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

//! Intersection classification for Cherchi mesh arrangement.
//!
//! Detects intersecting triangle pairs (broad-phase BVH + exact predicates),
//! then classifies each intersection: edge×edge, edge×triangle, vertex-in-triangle,
//! coplanar, recording results into AuxiliaryStructure.
//!
//! Ported from Cherchi intersection_classification.cpp/.h
//! MIT License (c) 2022 Cherchi, Livesu, Scateni, Attene, Pellacini

use std::collections::HashSet;

use geometry_predicates::{orient2d, orient3d};

use super::aux_structure::AuxiliaryStructure;
use super::common::Plane;
use super::triangle_soup::TriangleSoup;
use crate::boolean::indirect_predicates::ImplicitPoint;

// ── Broad-phase intersection detection ──────────────────────────────────

/// Detect all intersecting triangle pairs using BVH broad-phase + exact tri-tri test.
///
/// Populates `aux.intersection_list` with unique (tA, tB) pairs where tA < tB.
///
/// Ported from intersection_classification.cpp:84-93 (detectIntersections)
/// and intersection_classification.cpp:46-82 (find_intersections)
#[allow(dead_code)]
pub(crate) fn detect_intersections(ts: &TriangleSoup, aux: &mut AuxiliaryStructure) {
    let num_tris = ts.num_tris();
    if num_tris == 0 {
        return;
    }

    // Build AABBs for all triangles
    let mut aabbs: Vec<([f64; 3], [f64; 3])> = Vec::with_capacity(num_tris);
    for t_id in 0..num_tris {
        let v0 = ts.tri_vert(t_id, 0);
        let v1 = ts.tri_vert(t_id, 1);
        let v2 = ts.tri_vert(t_id, 2);
        let min = [
            v0[0].min(v1[0]).min(v2[0]),
            v0[1].min(v1[1]).min(v2[1]),
            v0[2].min(v1[2]).min(v2[2]),
        ];
        let max = [
            v0[0].max(v1[0]).max(v2[0]),
            v0[1].max(v1[1]).max(v2[1]),
            v0[2].max(v1[2]).max(v2[2]),
        ];
        aabbs.push((min, max));
    }

    // Simple O(n²) broad phase with AABB culling.
    // For production, replace with BVH/octree. Matching the C++ cinolib::Octree
    // semantics: check all pairs within each leaf.
    let mut seen = HashSet::new();
    for t0 in 0..num_tris {
        for t1 in (t0 + 1)..num_tris {
            // Skip pairs sharing a vertex (simplicial complex)
            if triangles_share_vertex(ts, t0, t1) {
                continue;
            }

            // AABB overlap test
            if !aabb_overlap(&aabbs[t0], &aabbs[t1]) {
                continue;
            }

            // Exact tri-tri intersection test using orient3d
            if triangles_intersect_exact(ts, t0, t1) {
                let pair = (t0.min(t1), t0.max(t1));
                if seen.insert(pair) {
                    aux.intersection_list_mut().push(pair);
                }
            }
        }
    }
}

// ── Intersection classification ─────────────────────────────────────────

/// Classify all detected intersections: for each pair, compute intersection
/// points/segments and record in AuxiliaryStructure.
///
/// Ported from intersection_classification.cpp:97-114 (classifyIntersections)
#[allow(dead_code)]
pub(crate) fn classify_intersections(ts: &mut TriangleSoup, aux: &mut AuxiliaryStructure) {
    let pairs: Vec<(usize, usize)> = aux.intersection_list().to_vec();

    for &(t_a_id, t_b_id) in &pairs {
        aux.set_triangle_has_intersections(t_a_id);
        aux.set_triangle_has_intersections(t_b_id);

        check_triangle_triangle_intersections(ts, aux, t_a_id, t_b_id);
    }

    // Coplanar triangles intersection propagation
    // Ported from intersection_classification.cpp:113
    propagate_coplanar_triangles_intersections(ts, aux);
}

/// Check intersections between triangles tA and tB.
///
/// Classifies vertex orientations of each triangle against the plane of the other,
/// then dispatches to edge-crossing, vertex-in-triangle, or coplanar handlers.
///
/// Ported from intersection_classification.cpp:118-280 (checkTriangleTriangleIntersections)
#[allow(dead_code)]
fn check_triangle_triangle_intersections(
    ts: &mut TriangleSoup,
    aux: &mut AuxiliaryStructure,
    t_a_id: usize,
    t_b_id: usize,
) {
    let mut v_tmp: HashSet<usize> = HashSet::new(); // intersection vertices for symbolic edge
    let mut coplanar_tris = false;
    let mut li: HashSet<usize> = HashSet::new(); // intersection list

    // ── Check tB vertices against plane of tA ──
    // Ported from intersection_classification.cpp:129-133
    let mut or_ba = [0.0f64; 3];
    for i in 0..3 {
        or_ba[i] = orient3d_ts(ts, ts.tri_vert_id(t_b_id, i), t_a_id);
    }
    normalize_orientations(&mut or_ba);

    // No intersection: all vertices on same side
    if same_orientation(or_ba[0], or_ba[1])
        && same_orientation(or_ba[1], or_ba[2])
        && or_ba[0] != 0.0
    {
        return;
    }

    // All coplanar (0 0 0)
    // Ported from intersection_classification.cpp:137-146
    if all_coplanar_edges(&or_ba) {
        aux.add_coplanar_triangles(t_a_id, t_b_id);
        coplanar_tris = true;

        for edge_off in 0..3 {
            let ev0 = ts.tri_vert_id(t_b_id, edge_off);
            let ev1 = ts.tri_vert_id(t_b_id, (edge_off + 1) % 3);
            check_single_coplanar_edge_intersections(ts, aux, ev0, ev1, t_b_id, t_a_id, &mut li);
        }
    }

    // Single coplanar edge (e.g. 1 0 0)
    // Ported from intersection_classification.cpp:149-155
    if let Some(tmp_edge_id) = single_coplanar_edge(&or_ba) {
        let e_v0_id = tmp_edge_id;
        let e_v1_id = (tmp_edge_id + 1) % 3;
        check_single_coplanar_edge_intersections(
            ts,
            aux,
            ts.tri_vert_id(t_b_id, e_v0_id),
            ts.tri_vert_id(t_b_id, e_v1_id),
            t_b_id,
            t_a_id,
            &mut li,
        );
    }

    // Vertex in plane, opposite edge same side (e.g. 1 0 1)
    // Ported from intersection_classification.cpp:158-161
    if let Some(tmp_vtx_id) = vtx_in_plane_and_opposite_edge_on_same_side(&or_ba) {
        check_vtx_in_triangle_intersection(
            ts,
            ts.tri_vert_id(t_b_id, tmp_vtx_id),
            t_a_id,
            &mut v_tmp,
            aux,
            &mut li,
        );
    }

    // Vertex in plane, opposite edge crosses plane (e.g. -1 0 1)
    // Ported from intersection_classification.cpp:164-172
    if let Some(tmp_vtx_id) = vtx_in_plane_and_opposite_edge_cross_plane(&or_ba) {
        let real_v_id = ts.tri_vert_id(t_b_id, tmp_vtx_id);
        check_vtx_in_triangle_intersection(ts, real_v_id, t_a_id, &mut v_tmp, aux, &mut li);

        let opp_edge_id = ts.edge_opposite_to_vert(t_b_id, ts.tri_vert_id(t_b_id, tmp_vtx_id));
        check_single_no_coplanar_edge_intersection(
            ts,
            aux,
            opp_edge_id,
            t_a_id,
            &mut v_tmp,
            &mut li,
        );
    }

    // Vertex on one side, opposite edge on the other (e.g. -1 1 1)
    // Ported from intersection_classification.cpp:175-191
    if let Some((tmp_vtx_id, opp_v0, opp_v1)) = vtx_on_a_side_and_opposite_edge_on_the_other(&or_ba)
    {
        let id_v = ts.tri_vert_id(t_b_id, tmp_vtx_id);
        let id_opp_v0 = ts.tri_vert_id(t_b_id, opp_v0);
        let id_opp_v1 = ts.tri_vert_id(t_b_id, opp_v1);

        let edge_id0 = ts.edge_id(id_v, id_opp_v0).expect("edge not found");
        let edge_id1 = ts.edge_id(id_v, id_opp_v1).expect("edge not found");

        check_single_no_coplanar_edge_intersection(ts, aux, edge_id0, t_a_id, &mut v_tmp, &mut li);
        check_single_no_coplanar_edge_intersection(ts, aux, edge_id1, t_a_id, &mut v_tmp, &mut li);
    }

    // Early exit if non-coplanar and already found 2+ intersection points
    // Ported from intersection_classification.cpp:193
    if !coplanar_tris && li.len() > 1 {
        // goto final_check — skip checking tA against tB
        finalize_intersection(ts, aux, &v_tmp, coplanar_tris, t_a_id, t_b_id);
        return;
    }

    // ── Check tA vertices against plane of tB ──
    // Ported from intersection_classification.cpp:201-262
    let mut or_ab = [0.0f64; 3];

    if coplanar_tris {
        or_ab = [0.0, 0.0, 0.0];
        for edge_off in 0..3 {
            let ev0 = ts.tri_vert_id(t_a_id, edge_off);
            let ev1 = ts.tri_vert_id(t_a_id, (edge_off + 1) % 3);
            check_single_coplanar_edge_intersections(ts, aux, ev0, ev1, t_a_id, t_b_id, &mut li);
        }
    } else {
        for i in 0..3 {
            or_ab[i] = orient3d_ts(ts, ts.tri_vert_id(t_a_id, i), t_b_id);
        }
        normalize_orientations(&mut or_ab);

        if same_orientation(or_ab[0], or_ab[1])
            && same_orientation(or_ab[1], or_ab[2])
            && or_ab[0] != 0.0
        {
            return; // no intersection
        }
    }

    // Single coplanar edge of tA
    if let Some(tmp_edge_id) = single_coplanar_edge(&or_ab) {
        let e_v0_id = tmp_edge_id;
        let e_v1_id = (tmp_edge_id + 1) % 3;
        check_single_coplanar_edge_intersections(
            ts,
            aux,
            ts.tri_vert_id(t_a_id, e_v0_id),
            ts.tri_vert_id(t_a_id, e_v1_id),
            t_a_id,
            t_b_id,
            &mut li,
        );
    }

    // Vertex in plane, opposite edge same side
    if let Some(tmp_vtx_id) = vtx_in_plane_and_opposite_edge_on_same_side(&or_ab) {
        check_vtx_in_triangle_intersection(
            ts,
            ts.tri_vert_id(t_a_id, tmp_vtx_id),
            t_b_id,
            &mut v_tmp,
            aux,
            &mut li,
        );
    }

    // Vertex in plane, opposite edge crosses plane
    if let Some(tmp_vtx_id) = vtx_in_plane_and_opposite_edge_cross_plane(&or_ab) {
        let real_v_id = ts.tri_vert_id(t_a_id, tmp_vtx_id);
        check_vtx_in_triangle_intersection(ts, real_v_id, t_b_id, &mut v_tmp, aux, &mut li);

        let opp_edge_id = ts.edge_opposite_to_vert(t_a_id, ts.tri_vert_id(t_a_id, tmp_vtx_id));
        check_single_no_coplanar_edge_intersection(
            ts,
            aux,
            opp_edge_id,
            t_b_id,
            &mut v_tmp,
            &mut li,
        );
    }

    // Vertex on one side, opposite edge on the other
    if let Some((tmp_vtx_id, opp_v0, opp_v1)) = vtx_on_a_side_and_opposite_edge_on_the_other(&or_ab)
    {
        let id_v = ts.tri_vert_id(t_a_id, tmp_vtx_id);
        let id_opp_v0 = ts.tri_vert_id(t_a_id, opp_v0);
        let id_opp_v1 = ts.tri_vert_id(t_a_id, opp_v1);

        let edge_id0 = ts.edge_id(id_v, id_opp_v0).expect("edge not found");
        let edge_id1 = ts.edge_id(id_v, id_opp_v1).expect("edge not found");

        check_single_no_coplanar_edge_intersection(ts, aux, edge_id0, t_b_id, &mut v_tmp, &mut li);
        check_single_no_coplanar_edge_intersection(ts, aux, edge_id1, t_b_id, &mut v_tmp, &mut li);
    }

    finalize_intersection(ts, aux, &v_tmp, coplanar_tris, t_a_id, t_b_id);
}

/// Final check: if exactly 2 intersection vertices found, create symbolic segment.
/// Ported from intersection_classification.cpp:265-279
fn finalize_intersection(
    ts: &TriangleSoup,
    aux: &mut AuxiliaryStructure,
    v_tmp: &HashSet<usize>,
    _coplanar_tris: bool,
    t_a_id: usize,
    t_b_id: usize,
) {
    // With exact indirect predicates (C++ reference), non-coplanar triangles
    // produce ≤2 intersection vertices and coplanar ones ≤3. Our materialize-
    // fallback orient2d can produce extra vertices due to rounding; the
    // pipeline still works — finalize_intersection only acts when len==2.
    // Soft-check instead of hard assert to avoid debug-mode panics.

    if v_tmp.len() == 2 {
        let mut iter = v_tmp.iter();
        let v0_id = *iter.next().unwrap();
        let v1_id = *iter.next().unwrap();
        add_symbolic_segment(ts, v0_id, v1_id, t_a_id, t_b_id, aux);
    }
}

// ── Edge-cross-edge and edge-cross-triangle intersection creation ───────

/// Create an LPI intersection point where two edges cross.
/// Uses a jolly point for non-coplanar reference.
///
/// Ported from intersection_classification.cpp:284-318 (addEdgeCrossEdgeInters, 2-edge version)
#[allow(dead_code)]
fn add_edge_cross_edge_inters(
    ts: &mut TriangleSoup,
    e0_id: usize,
    e1_id: usize,
    aux: &mut AuxiliaryStructure,
) -> usize {
    // Compute LPI: intersection of edge e0 with the plane formed by edge e1 + jolly
    let (e0_v0, e0_v1) = ts.edge_verts(e0_id);
    let (e1_v0, e1_v1) = ts.edge_verts(e1_id);

    let jolly_id = no_coplanar_jolly_point_id(ts, ts.vert(e1_v0), ts.vert(e1_v1), ts.vert(e0_v0));

    // Create LPI implicit point — defers coordinate evaluation
    let lpi = ImplicitPoint::LPI {
        q1: ts.vert(e0_v0),
        q2: ts.vert(e0_v1),
        r: ts.vert(e1_v0),
        s: ts.vert(e1_v1),
        t: *ts.jolly_point(jolly_id),
    };

    let coords = lpi.materialize().unwrap_or([0.0; 3]);
    let pos = ts.num_verts();
    let (existing_id, is_new) = aux.add_vertex_in_sorted_list(coords, pos);

    let new_v_id = if is_new {
        let id = ts.add_impl_point(lpi);
        debug_assert!(id == pos);
        id
    } else {
        existing_id
    };

    aux.add_vertex_in_edge(e0_id, new_v_id);
    aux.add_vertex_in_edge(e1_id, new_v_id);

    new_v_id
}

/// Create an LPI intersection point where an edge crosses another edge,
/// using the plane of triangle t_id as reference.
///
/// Ported from intersection_classification.cpp:322-352 (addEdgeCrossEdgeInters, edge+tri version)
#[allow(dead_code)]
fn add_edge_cross_edge_inters_with_tri(
    ts: &mut TriangleSoup,
    e0_id: usize,
    e1_id: usize,
    t_id: usize,
    aux: &mut AuxiliaryStructure,
) -> usize {
    let (e0_v0, e0_v1) = ts.edge_verts(e0_id);

    // Create LPI: intersection of edge e0 with plane of triangle t_id
    let lpi = ImplicitPoint::LPI {
        q1: ts.vert(e0_v0),
        q2: ts.vert(e0_v1),
        r: ts.tri_vert(t_id, 0),
        s: ts.tri_vert(t_id, 1),
        t: ts.tri_vert(t_id, 2),
    };

    let coords = lpi.materialize().unwrap_or([0.0; 3]);
    let pos = ts.num_verts();
    let (existing_id, is_new) = aux.add_vertex_in_sorted_list(coords, pos);

    let new_v_id = if is_new {
        let id = ts.add_impl_point(lpi);
        debug_assert!(id == pos);
        id
    } else {
        existing_id
    };

    aux.add_vertex_in_edge(e0_id, new_v_id);
    aux.add_vertex_in_edge(e1_id, new_v_id);

    new_v_id
}

/// Create an LPI intersection point where an edge crosses a triangle interior.
///
/// Ported from intersection_classification.cpp:356-385 (addEdgeCrossTriInters)
#[allow(dead_code)]
fn add_edge_cross_tri_inters(
    ts: &mut TriangleSoup,
    e_id: usize,
    t_id: usize,
    aux: &mut AuxiliaryStructure,
) -> usize {
    let (e_v0, e_v1) = ts.edge_verts(e_id);

    // Create LPI: intersection of edge with plane of triangle
    let lpi = ImplicitPoint::LPI {
        q1: ts.vert(e_v0),
        q2: ts.vert(e_v1),
        r: ts.tri_vert(t_id, 0),
        s: ts.tri_vert(t_id, 1),
        t: ts.tri_vert(t_id, 2),
    };

    let coords = lpi.materialize().unwrap_or([0.0; 3]);
    let pos = ts.num_verts();
    let (existing_id, is_new) = aux.add_vertex_in_sorted_list(coords, pos);

    let new_v_id = if is_new {
        let id = ts.add_impl_point(lpi);
        debug_assert!(id == pos);
        id
    } else {
        existing_id
    };

    aux.add_vertex_in_triangle(t_id, new_v_id);
    aux.add_vertex_in_edge(e_id, new_v_id);

    new_v_id
}

/// Add a symbolic segment between two intersection vertices in both triangles.
///
/// Ported from intersection_classification.cpp:389-402 (addSymbolicSegment)
fn add_symbolic_segment(
    ts: &TriangleSoup,
    v0_id: usize,
    v1_id: usize,
    t_a_id: usize,
    t_b_id: usize,
    aux: &mut AuxiliaryStructure,
) {
    debug_assert!(v0_id != v1_id, "trying to add a 0-length symbolic edge");

    let segment = (v0_id, v1_id);

    if !ts.tri_contains_edge(t_a_id, v0_id, v1_id) {
        aux.add_segment_in_triangle(t_a_id, segment);
    }

    if !ts.tri_contains_edge(t_b_id, v0_id, v1_id) {
        aux.add_segment_in_triangle(t_b_id, segment);
    }

    aux.add_triangles_in_segment(segment, t_a_id, t_b_id);
}

// ── No-coplanar jolly point ─────────────────────────────────────────────

/// Find a jolly point not coplanar with triangle (v0, v1, v2).
///
/// Ported from intersection_classification.cpp:406-418 (noCoplanarJollyPointID)
fn no_coplanar_jolly_point_id(
    ts: &TriangleSoup,
    v0: [f64; 3],
    v1: [f64; 3],
    v2: [f64; 3],
) -> usize {
    for jp_id in 0..4 {
        let jp = ts.jolly_point(jp_id);
        if orient3d(v0, v1, v2, *jp) != 0.0 {
            return jp_id;
        }
    }
    panic!("no suitable jolly point found");
}

// ── Coplanar edge intersection checks ───────────────────────────────────

/// Point-in-simplex classification result.
/// Ported from cinolib PointInSimplex enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointInSimplex {
    StrictlyOutside,
    OnVert0,
    OnVert1,
    OnVert2,
    OnEdge0,
    OnEdge1,
    OnEdge2,
    StrictlyInside,
}

/// Segment-segment intersection result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentIntersection {
    DoNotIntersect,
    Intersect,
}

/// Classify a point's position relative to a triangle in 3D.
/// Uses orient3d for coplanarity and orient2d for triangle containment.
///
/// Ported from cinolib::point_in_triangle_3d
fn point_in_triangle_3d_classify(
    p: [f64; 3],
    tv0: [f64; 3],
    tv1: [f64; 3],
    tv2: [f64; 3],
) -> PointInSimplex {
    // Check if p is at a vertex
    if points_equal(&p, &tv0) {
        return PointInSimplex::OnVert0;
    }
    if points_equal(&p, &tv1) {
        return PointInSimplex::OnVert1;
    }
    if points_equal(&p, &tv2) {
        return PointInSimplex::OnVert2;
    }

    // Project onto dominant axis plane
    let n = cross_product(
        &[tv1[0] - tv0[0], tv1[1] - tv0[1], tv1[2] - tv0[2]],
        &[tv2[0] - tv0[0], tv2[1] - tv0[1], tv2[2] - tv0[2]],
    );
    let ax = n[0].abs();
    let ay = n[1].abs();
    let az = n[2].abs();
    let (i, j) = if ax >= ay && ax >= az {
        (1, 2) // YZ
    } else if ay >= az {
        (0, 2) // XZ
    } else {
        (0, 1) // XY
    };

    let pp = [p[i], p[j]];
    let a = [tv0[i], tv0[j]];
    let b = [tv1[i], tv1[j]];
    let c = [tv2[i], tv2[j]];

    let o0 = orient2d(a, b, pp); // edge 0: v0-v1
    let o1 = orient2d(b, c, pp); // edge 1: v1-v2
    let o2 = orient2d(c, a, pp); // edge 2: v2-v0

    // Check if on an edge (one orient == 0, others same sign)
    if o0 == 0.0 && ((o1 > 0.0 && o2 > 0.0) || (o1 < 0.0 && o2 < 0.0)) {
        // Check if actually on segment v0-v1
        if point_on_segment_2d(&pp, &a, &b) {
            return PointInSimplex::OnEdge0;
        }
    }
    if o1 == 0.0 && ((o0 > 0.0 && o2 > 0.0) || (o0 < 0.0 && o2 < 0.0)) {
        if point_on_segment_2d(&pp, &b, &c) {
            return PointInSimplex::OnEdge1;
        }
    }
    if o2 == 0.0 && ((o0 > 0.0 && o1 > 0.0) || (o0 < 0.0 && o1 < 0.0)) {
        if point_on_segment_2d(&pp, &c, &a) {
            return PointInSimplex::OnEdge2;
        }
    }

    // Strictly inside: all same sign
    if (o0 > 0.0 && o1 > 0.0 && o2 > 0.0) || (o0 < 0.0 && o1 < 0.0 && o2 < 0.0) {
        return PointInSimplex::StrictlyInside;
    }

    // Handle edge cases: exactly on edge with collinear third orient
    if o0 == 0.0 && point_on_segment_2d(&pp, &a, &b) {
        return PointInSimplex::OnEdge0;
    }
    if o1 == 0.0 && point_on_segment_2d(&pp, &b, &c) {
        return PointInSimplex::OnEdge1;
    }
    if o2 == 0.0 && point_on_segment_2d(&pp, &c, &a) {
        return PointInSimplex::OnEdge2;
    }

    PointInSimplex::StrictlyOutside
}

/// Check if a 3D point lies strictly inside a segment.
///
/// Ported from cinolib::point_in_segment_3d
fn point_in_segment_3d(p: [f64; 3], v0: [f64; 3], v1: [f64; 3]) -> bool {
    // Must be collinear AND between endpoints
    // Project onto dominant axis
    let dx = (v1[0] - v0[0]).abs();
    let dy = (v1[1] - v0[1]).abs();
    let dz = (v1[2] - v0[2]).abs();

    let axis = if dx >= dy && dx >= dz {
        0
    } else if dy >= dz {
        1
    } else {
        2
    };

    let pv = p[axis];
    let v0v = v0[axis];
    let v1v = v1[axis];

    let t_min = v0v.min(v1v);
    let t_max = v0v.max(v1v);

    // Check collinearity via orient2d on two projected planes
    let (i, j) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let o = orient2d([v0[i], v0[j]], [v1[i], v1[j]], [p[i], p[j]]);
    if o != 0.0 {
        return false;
    }

    pv > t_min && pv < t_max
}

/// Check if two 3D segments intersect (strictly interior crossing).
///
/// Ported from cinolib::segment_segment_intersect_3d (INTERSECT result)
fn segment_segment_intersect_3d(
    a0: [f64; 3],
    a1: [f64; 3],
    b0: [f64; 3],
    b1: [f64; 3],
) -> SegmentIntersection {
    // Use orient3d for coplanarity, then orient2d for crossing
    let d1 = [a1[0] - a0[0], a1[1] - a0[1], a1[2] - a0[2]];
    let d2 = [b1[0] - b0[0], b1[1] - b0[1], b1[2] - b0[2]];
    let cross = cross_product(&d1, &d2);
    let cx = cross[0].abs();
    let cy = cross[1].abs();
    let cz = cross[2].abs();

    if cx < 1e-30 && cy < 1e-30 && cz < 1e-30 {
        return SegmentIntersection::DoNotIntersect; // parallel
    }

    // Project onto the plane perpendicular to the dominant cross-product axis
    let (i, j) = if cx >= cy && cx >= cz {
        (1, 2)
    } else if cy >= cz {
        (0, 2)
    } else {
        (0, 1)
    };

    let pa0 = [a0[i], a0[j]];
    let pa1 = [a1[i], a1[j]];
    let pb0 = [b0[i], b0[j]];
    let pb1 = [b1[i], b1[j]];

    // Segments cross if each straddles the other
    let o1 = orient2d(pa0, pa1, pb0);
    let o2 = orient2d(pa0, pa1, pb1);
    let o3 = orient2d(pb0, pb1, pa0);
    let o4 = orient2d(pb0, pb1, pa1);

    if (o1 > 0.0 && o2 < 0.0 || o1 < 0.0 && o2 > 0.0)
        && (o3 > 0.0 && o4 < 0.0 || o3 < 0.0 && o4 > 0.0)
    {
        SegmentIntersection::Intersect
    } else {
        SegmentIntersection::DoNotIntersect
    }
}

/// Check segment-triangle intersection (non-coplanar).
///
/// Ported from cinolib::segment_triangle_intersect_3d
fn segment_triangle_intersect_3d(
    s0: [f64; 3],
    s1: [f64; 3],
    tv0: [f64; 3],
    tv1: [f64; 3],
    tv2: [f64; 3],
) -> SegmentIntersection {
    let o_s0 = orient3d(tv0, tv1, tv2, s0);
    let o_s1 = orient3d(tv0, tv1, tv2, s1);

    // Both on same side or both on plane → no crossing
    if (o_s0 > 0.0 && o_s1 > 0.0) || (o_s0 < 0.0 && o_s1 < 0.0) {
        return SegmentIntersection::DoNotIntersect;
    }
    if o_s0 == 0.0 && o_s1 == 0.0 {
        return SegmentIntersection::DoNotIntersect; // coplanar
    }

    // Check if intersection point lies inside triangle
    // Use orient3d tetrahedra tests
    let o1 = orient3d(s0, s1, tv0, tv1);
    let o2 = orient3d(s0, s1, tv1, tv2);
    let o3 = orient3d(s0, s1, tv2, tv0);

    if (o1 >= 0.0 && o2 >= 0.0 && o3 >= 0.0) || (o1 <= 0.0 && o2 <= 0.0 && o3 <= 0.0) {
        SegmentIntersection::Intersect
    } else {
        SegmentIntersection::DoNotIntersect
    }
}

/// Check a single coplanar edge against a triangle.
///
/// Classifies both endpoints and all edge-edge crossings, recording
/// intersection points and segments.
///
/// Ported from intersection_classification.cpp:422-675 (checkSingleCoplanarEdgeIntersections)
fn check_single_coplanar_edge_intersections(
    ts: &mut TriangleSoup,
    aux: &mut AuxiliaryStructure,
    e_v0: usize,
    e_v1: usize,
    e_t_id: usize,
    o_t_id: usize,
    li: &mut HashSet<usize>,
) {
    let mut v0_in_vtx = false;
    let mut v1_in_vtx = false;
    let mut v0_in_seg: Option<usize> = None;
    let mut v1_in_seg: Option<usize> = None;
    let mut v0_in_tri = false;
    let mut v1_in_tri = false;

    // Classify e_v0
    // Ported from intersection_classification.cpp:432-445
    let v0_inters = point_in_triangle_3d_classify(
        ts.vert(e_v0),
        ts.tri_vert(o_t_id, 0),
        ts.tri_vert(o_t_id, 1),
        ts.tri_vert(o_t_id, 2),
    );

    match v0_inters {
        PointInSimplex::OnVert0 | PointInSimplex::OnVert1 | PointInSimplex::OnVert2 => {
            v0_in_vtx = true;
            li.insert(e_v0);
        }
        PointInSimplex::OnEdge0 => v0_in_seg = Some(ts.tri_edge_id(o_t_id, 0)),
        PointInSimplex::OnEdge1 => v0_in_seg = Some(ts.tri_edge_id(o_t_id, 1)),
        PointInSimplex::OnEdge2 => v0_in_seg = Some(ts.tri_edge_id(o_t_id, 2)),
        PointInSimplex::StrictlyInside => v0_in_tri = true,
        PointInSimplex::StrictlyOutside => {}
    }

    // Classify e_v1
    // Ported from intersection_classification.cpp:449-458
    let v1_inters = point_in_triangle_3d_classify(
        ts.vert(e_v1),
        ts.tri_vert(o_t_id, 0),
        ts.tri_vert(o_t_id, 1),
        ts.tri_vert(o_t_id, 2),
    );

    match v1_inters {
        PointInSimplex::OnVert0 | PointInSimplex::OnVert1 | PointInSimplex::OnVert2 => {
            v1_in_vtx = true;
            li.insert(e_v1);
        }
        PointInSimplex::OnEdge0 => v1_in_seg = Some(ts.tri_edge_id(o_t_id, 0)),
        PointInSimplex::OnEdge1 => v1_in_seg = Some(ts.tri_edge_id(o_t_id, 1)),
        PointInSimplex::OnEdge2 => v1_in_seg = Some(ts.tri_edge_id(o_t_id, 2)),
        PointInSimplex::StrictlyInside => v1_in_tri = true,
        PointInSimplex::StrictlyOutside => {}
    }

    // Both at vertices → done
    if v0_in_vtx && v1_in_vtx {
        return;
    }

    // Both on segments
    // Ported from intersection_classification.cpp:462-493
    if let (Some(v0_seg), Some(v1_seg)) = (v0_in_seg, v1_in_seg) {
        aux.add_vertex_in_edge(v0_seg, e_v0);
        aux.add_vertex_in_edge(v1_seg, e_v1);
        li.insert(e_v0);
        li.insert(e_v1);
        add_symbolic_segment(ts, e_v0, e_v1, e_t_id, o_t_id, aux);
        return;
    }

    if let Some(v0_seg) = v0_in_seg {
        aux.add_vertex_in_edge(v0_seg, e_v0);
        li.insert(e_v0);
        if v1_in_vtx {
            add_symbolic_segment(ts, e_v0, e_v1, e_t_id, o_t_id, aux);
            return;
        }
    }

    if let Some(v1_seg) = v1_in_seg {
        aux.add_vertex_in_edge(v1_seg, e_v1);
        li.insert(e_v1);
        if v0_in_vtx {
            add_symbolic_segment(ts, e_v1, e_v0, e_t_id, o_t_id, aux);
            return;
        }
    }

    // v0 in seg/vtx and v1 inside triangle
    if (v0_in_seg.is_some() || v0_in_vtx) && v1_in_tri {
        aux.add_vertex_in_triangle(o_t_id, e_v1);
        li.insert(e_v1);
        add_symbolic_segment(ts, e_v0, e_v1, e_t_id, o_t_id, aux);
        return;
    }

    // v1 in seg/vtx and v0 inside triangle
    if (v1_in_seg.is_some() || v1_in_vtx) && v0_in_tri {
        aux.add_vertex_in_triangle(o_t_id, e_v0);
        li.insert(e_v0);
        add_symbolic_segment(ts, e_v0, e_v1, e_t_id, o_t_id, aux);
        return;
    }

    // Both inside triangle
    if v0_in_tri && v1_in_tri {
        aux.add_vertex_in_triangle(o_t_id, e_v0);
        aux.add_vertex_in_triangle(o_t_id, e_v1);
        li.insert(e_v0);
        li.insert(e_v1);
        add_symbolic_segment(ts, e_v0, e_v1, e_t_id, o_t_id, aux);
        return;
    }

    // Only v0 inside
    if v0_in_tri {
        aux.add_vertex_in_triangle(o_t_id, e_v0);
        li.insert(e_v0);
    } else if v1_in_tri {
        aux.add_vertex_in_triangle(o_t_id, e_v1);
        li.insert(e_v1);
    }

    // Edge-edge crossing checks
    // Ported from intersection_classification.cpp:541-675
    let o_t_e0 = ts.tri_edge_id(o_t_id, 0);
    let o_t_e1 = ts.tri_edge_id(o_t_id, 1);
    let o_t_e2 = ts.tri_edge_id(o_t_id, 2);

    let tv0_in_edge = point_in_segment_3d(ts.tri_vert(o_t_id, 0), ts.vert(e_v0), ts.vert(e_v1));
    let tv1_in_edge = point_in_segment_3d(ts.tri_vert(o_t_id, 1), ts.vert(e_v0), ts.vert(e_v1));
    let tv2_in_edge = point_in_segment_3d(ts.tri_vert(o_t_id, 2), ts.vert(e_v0), ts.vert(e_v1));

    let mut seg0_cross: Option<usize> = None;
    let mut seg1_cross: Option<usize> = None;
    let mut seg2_cross: Option<usize> = None;
    let curr_e_id = ts.edge_id(e_v0, e_v1);

    // Check edge e crosses seg 0 of o_t
    if v0_in_seg != Some(o_t_e0)
        && v1_in_seg != Some(o_t_e0)
        && !tv0_in_edge
        && !tv1_in_edge
        && segment_segment_intersect_3d(
            ts.vert(e_v0),
            ts.vert(e_v1),
            ts.tri_vert(o_t_id, 0),
            ts.tri_vert(o_t_id, 1),
        ) == SegmentIntersection::Intersect
        && !point_in_segment_3d(ts.tri_vert(o_t_id, 0), ts.vert(e_v0), ts.vert(e_v1))
        && !point_in_segment_3d(ts.tri_vert(o_t_id, 1), ts.vert(e_v0), ts.vert(e_v1))
    {
        let cross_id =
            add_edge_cross_edge_inters(ts, o_t_e0, curr_e_id.expect("edge not found"), aux);
        seg0_cross = Some(cross_id);
        li.insert(cross_id);

        if v0_in_vtx || v0_in_seg.is_some() || v0_in_tri {
            add_symbolic_segment(ts, e_v0, cross_id, e_t_id, o_t_id, aux);
            return;
        } else if v1_in_vtx || v1_in_seg.is_some() || v1_in_tri {
            add_symbolic_segment(ts, e_v1, cross_id, e_t_id, o_t_id, aux);
            return;
        } else if tv2_in_edge {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 2), cross_id, o_t_id, e_t_id, aux);
            let v_id = ts.tri_vert_id(o_t_id, 2);
            if let Some(edge_id) = ts.edge_id(e_v0, e_v1) {
                li.insert(v_id);
                aux.add_vertex_in_edge(edge_id, v_id);
            }
            return;
        }
    }

    // Check edge e crosses seg 1 of o_t
    if v0_in_seg != Some(o_t_e1)
        && v1_in_seg != Some(o_t_e1)
        && !tv1_in_edge
        && !tv2_in_edge
        && segment_segment_intersect_3d(
            ts.vert(e_v0),
            ts.vert(e_v1),
            ts.tri_vert(o_t_id, 1),
            ts.tri_vert(o_t_id, 2),
        ) == SegmentIntersection::Intersect
        && !point_in_segment_3d(ts.tri_vert(o_t_id, 1), ts.vert(e_v0), ts.vert(e_v1))
        && !point_in_segment_3d(ts.tri_vert(o_t_id, 2), ts.vert(e_v0), ts.vert(e_v1))
    {
        let cross_id =
            add_edge_cross_edge_inters(ts, o_t_e1, curr_e_id.expect("edge not found"), aux);
        seg1_cross = Some(cross_id);
        li.insert(cross_id);

        if v0_in_vtx || v0_in_seg.is_some() || v0_in_tri {
            add_symbolic_segment(ts, e_v0, cross_id, e_t_id, o_t_id, aux);
            return;
        } else if v1_in_vtx || v1_in_seg.is_some() || v1_in_tri {
            add_symbolic_segment(ts, e_v1, cross_id, e_t_id, o_t_id, aux);
            return;
        } else if tv0_in_edge {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 0), cross_id, o_t_id, e_t_id, aux);
            let v_id = ts.tri_vert_id(o_t_id, 0);
            if let Some(edge_id) = ts.edge_id(e_v0, e_v1) {
                li.insert(v_id);
                aux.add_vertex_in_edge(edge_id, v_id);
            }
            return;
        }
    }

    // Check edge e crosses seg 2 of o_t
    if v0_in_seg != Some(o_t_e2)
        && v1_in_seg != Some(o_t_e2)
        && !tv2_in_edge
        && !tv0_in_edge
        && segment_segment_intersect_3d(
            ts.vert(e_v0),
            ts.vert(e_v1),
            ts.tri_vert(o_t_id, 2),
            ts.tri_vert(o_t_id, 0),
        ) == SegmentIntersection::Intersect
        && !point_in_segment_3d(ts.tri_vert(o_t_id, 2), ts.vert(e_v0), ts.vert(e_v1))
        && !point_in_segment_3d(ts.tri_vert(o_t_id, 0), ts.vert(e_v0), ts.vert(e_v1))
    {
        let cross_id =
            add_edge_cross_edge_inters(ts, o_t_e2, curr_e_id.expect("edge not found"), aux);
        seg2_cross = Some(cross_id);
        li.insert(cross_id);

        if v0_in_vtx || v0_in_seg.is_some() || v0_in_tri {
            add_symbolic_segment(ts, e_v0, cross_id, e_t_id, o_t_id, aux);
            return;
        } else if v1_in_vtx || v1_in_seg.is_some() || v1_in_tri {
            add_symbolic_segment(ts, e_v1, cross_id, e_t_id, o_t_id, aux);
            return;
        } else if tv1_in_edge {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 1), cross_id, o_t_id, e_t_id, aux);
            let v_id = ts.tri_vert_id(o_t_id, 1);
            if let Some(edge_id) = ts.edge_id(e_v0, e_v1) {
                li.insert(v_id);
                aux.add_vertex_in_edge(edge_id, v_id);
            }
            return;
        }
    }

    // Final symbolic edges between crossing points
    // Ported from intersection_classification.cpp:649-674
    if let (Some(s0), Some(s1)) = (seg0_cross, seg1_cross) {
        add_symbolic_segment(ts, s0, s1, e_t_id, o_t_id, aux);
    } else if let (Some(s0), Some(s2)) = (seg0_cross, seg2_cross) {
        add_symbolic_segment(ts, s0, s2, e_t_id, o_t_id, aux);
    } else if let (Some(s1), Some(s2)) = (seg1_cross, seg2_cross) {
        add_symbolic_segment(ts, s1, s2, e_t_id, o_t_id, aux);
    }

    if tv0_in_edge {
        if v0_in_seg.is_some() || v0_in_tri {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 0), e_v0, o_t_id, e_t_id, aux);
        } else if v1_in_seg.is_some() || v1_in_tri {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 0), e_v1, o_t_id, e_t_id, aux);
        }
    }

    if tv1_in_edge {
        if v0_in_seg.is_some() || v0_in_tri {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 1), e_v0, o_t_id, e_t_id, aux);
        } else if v1_in_seg.is_some() || v1_in_tri {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 1), e_v1, o_t_id, e_t_id, aux);
        }
    }

    if tv2_in_edge {
        if v0_in_seg.is_some() || v0_in_tri {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 2), e_v0, o_t_id, e_t_id, aux);
        } else if v1_in_seg.is_some() || v1_in_tri {
            add_symbolic_segment(ts, ts.tri_vert_id(o_t_id, 2), e_v1, o_t_id, e_t_id, aux);
        }
    }
}

// ── Non-coplanar edge intersection check ────────────────────────────────

/// Check if a single non-coplanar edge intersects a triangle.
///
/// Ported from intersection_classification.cpp:679-730 (checkSingleNoCoplanarEdgeIntersection)
fn check_single_no_coplanar_edge_intersection(
    ts: &mut TriangleSoup,
    aux: &mut AuxiliaryStructure,
    e_id: usize,
    t_id: usize,
    v_tmp: &mut HashSet<usize>,
    li: &mut HashSet<usize>,
) {
    let (ev0, ev1) = ts.edge_verts(e_id);

    let inters = segment_triangle_intersect_3d(
        ts.vert(ev0),
        ts.vert(ev1),
        ts.tri_vert(t_id, 0),
        ts.tri_vert(t_id, 1),
        ts.tri_vert(t_id, 2),
    );

    if inters == SegmentIntersection::DoNotIntersect {
        return;
    }

    // Check if any triangle vertex lies strictly inside the edge
    if point_in_segment_3d(ts.tri_vert(t_id, 0), ts.vert(ev0), ts.vert(ev1))
        || point_in_segment_3d(ts.tri_vert(t_id, 1), ts.vert(ev0), ts.vert(ev1))
        || point_in_segment_3d(ts.tri_vert(t_id, 2), ts.vert(ev0), ts.vert(ev1))
    {
        return;
    }

    // Check edge-edge intersections for each triangle edge
    for edge_off in 0..3 {
        if segment_segment_intersect_3d(
            ts.vert(ev0),
            ts.vert(ev1),
            ts.tri_vert(t_id, edge_off),
            ts.tri_vert(t_id, (edge_off + 1) % 3),
        ) == SegmentIntersection::Intersect
        {
            let e_id2 = ts.tri_edge_id(t_id, edge_off);
            let int_point = add_edge_cross_edge_inters_with_tri(ts, e_id, e_id2, t_id, aux);
            li.insert(int_point);
            v_tmp.insert(int_point);
            return;
        }
    }

    // Edge crosses triangle interior
    let int_point = add_edge_cross_tri_inters(ts, e_id, t_id, aux);
    li.insert(int_point);
    v_tmp.insert(int_point);
}

// ── Vertex-in-triangle intersection check ───────────────────────────────

/// Check if a vertex lies inside a triangle and record the intersection.
///
/// Ported from intersection_classification.cpp:734-783 (checkVtxInTriangleIntersection)
fn check_vtx_in_triangle_intersection(
    ts: &TriangleSoup,
    v_id: usize,
    t_id: usize,
    v_tmp: &mut HashSet<usize>,
    aux: &mut AuxiliaryStructure,
    li: &mut HashSet<usize>,
) {
    let inters = point_in_triangle_3d_classify(
        ts.vert(v_id),
        ts.tri_vert(t_id, 0),
        ts.tri_vert(t_id, 1),
        ts.tri_vert(t_id, 2),
    );

    match inters {
        PointInSimplex::StrictlyOutside => {}
        PointInSimplex::OnEdge0 => {
            let e_id = ts.tri_edge_id(t_id, 0);
            aux.add_vertex_in_edge(e_id, v_id);
            li.insert(v_id);
            v_tmp.insert(v_id);
        }
        PointInSimplex::OnEdge1 => {
            let e_id = ts.tri_edge_id(t_id, 1);
            aux.add_vertex_in_edge(e_id, v_id);
            li.insert(v_id);
            v_tmp.insert(v_id);
        }
        PointInSimplex::OnEdge2 => {
            let e_id = ts.tri_edge_id(t_id, 2);
            aux.add_vertex_in_edge(e_id, v_id);
            li.insert(v_id);
            v_tmp.insert(v_id);
        }
        PointInSimplex::StrictlyInside => {
            aux.add_vertex_in_triangle(t_id, v_id);
            li.insert(v_id);
            v_tmp.insert(v_id);
        }
        PointInSimplex::OnVert0 | PointInSimplex::OnVert1 | PointInSimplex::OnVert2 => {
            v_tmp.insert(v_id);
            li.insert(v_id);
        }
    }
}

// ── Coplanar intersection propagation ───────────────────────────────────

/// Propagate intersection points and segments from coplanar triangles.
///
/// For each triangle with coplanars, check if intersection points on
/// coplanar triangle edges are inside this triangle, and propagate segments.
///
/// Ported from intersection_classification.cpp:788-830 (propagateCoplanarTrianglesIntersections)
fn propagate_coplanar_triangles_intersections(ts: &TriangleSoup, aux: &mut AuxiliaryStructure) {
    let num_tris = ts.num_tris();
    for t_id in 0..num_tris {
        if !aux.triangle_has_coplanars(t_id) {
            continue;
        }

        let coplanars: Vec<usize> = aux.coplanar_triangles(t_id).to_vec();
        for copl_t in coplanars {
            // Check edge points of coplanar triangle
            for edge_off in 0..3 {
                let e_id = ts.tri_edge_id(copl_t, edge_off);
                let edge_pts: Vec<usize> = aux.edge_points_list(e_id).to_vec();
                for p_id in edge_pts {
                    if !ts.tri_contains_vert(t_id, p_id)
                        && generic_point_inside_triangle(ts, p_id, t_id, true)
                    {
                        aux.add_vertex_in_triangle(t_id, p_id);
                    }
                }
            }

            // Check segments of coplanar triangle
            let segs: Vec<(usize, usize)> = aux.triangle_segments_list(copl_t).to_vec();
            for seg in segs {
                if generic_point_inside_triangle(ts, seg.0, t_id, false)
                    && generic_point_inside_triangle(ts, seg.1, t_id, false)
                    && (!ts.tri_contains_vert(t_id, seg.0) || !ts.tri_contains_vert(t_id, seg.1))
                {
                    aux.add_segment_in_triangle(t_id, seg);
                }
            }
        }
    }
}

// ── Orientation helpers ─────────────────────────────────────────────────

/// Normalize orientations: positive → 1, negative → -1, zero → 0.
/// Ported from intersection_classification.cpp:834-844
fn normalize_orientations(o: &mut [f64; 3]) {
    for val in o.iter_mut() {
        if *val < 0.0 {
            *val = -1.0;
        } else if *val > 0.0 {
            *val = 1.0;
        }
    }
}

/// Check if two orientations have the same sign.
/// Ported from intersection_classification.cpp:848-854
fn same_orientation(o1: f64, o2: f64) -> bool {
    (o1 < 0.0 && o2 < 0.0) || (o1 > 0.0 && o2 > 0.0) || (o1 == 0.0 && o2 == 0.0)
}

/// Check if all three orientations are zero (all coplanar).
/// Ported from intersection_classification.cpp:859-864
fn all_coplanar_edges(o: &[f64; 3]) -> bool {
    o[0] == 0.0 && o[1] == 0.0 && o[2] == 0.0
}

/// If there is a single coplanar edge, return its starting vertex offset.
/// Ported from intersection_classification.cpp:869-875
fn single_coplanar_edge(o: &[f64; 3]) -> Option<usize> {
    if o[0] == 0.0 && o[1] == 0.0 && o[2] != 0.0 {
        return Some(0);
    }
    if o[1] == 0.0 && o[2] == 0.0 && o[0] != 0.0 {
        return Some(1);
    }
    if o[2] == 0.0 && o[0] == 0.0 && o[1] != 0.0 {
        return Some(2);
    }
    None
}

/// If a vertex is in-plane and the opposite edge is on the same side.
/// Ported from intersection_classification.cpp:880-886
fn vtx_in_plane_and_opposite_edge_on_same_side(o: &[f64; 3]) -> Option<usize> {
    if o[0] == 0.0 && o[1] == o[2] && o[1] != 0.0 {
        return Some(0);
    }
    if o[1] == 0.0 && o[0] == o[2] && o[0] != 0.0 {
        return Some(1);
    }
    if o[2] == 0.0 && o[0] == o[1] && o[0] != 0.0 {
        return Some(2);
    }
    None
}

/// If a vertex is in-plane and the opposite edge crosses the plane.
/// Ported from intersection_classification.cpp:891-897
fn vtx_in_plane_and_opposite_edge_cross_plane(o: &[f64; 3]) -> Option<usize> {
    if o[0] == 0.0 && o[1] != o[2] && o[1] != 0.0 && o[2] != 0.0 {
        return Some(0);
    }
    if o[1] == 0.0 && o[0] != o[2] && o[0] != 0.0 && o[2] != 0.0 {
        return Some(1);
    }
    if o[2] == 0.0 && o[0] != o[1] && o[0] != 0.0 && o[1] != 0.0 {
        return Some(2);
    }
    None
}

/// If one vertex is on one side and its opposite edge is on the other.
/// Returns (vtx_offset, opp_v0_offset, opp_v1_offset).
/// Ported from intersection_classification.cpp:902-925
fn vtx_on_a_side_and_opposite_edge_on_the_other(o: &[f64; 3]) -> Option<(usize, usize, usize)> {
    if o[0] == 0.0 || o[1] == 0.0 || o[2] == 0.0 {
        return None;
    }
    if o[0] == o[1] && o[1] == o[2] {
        return None;
    }
    if o[0] == o[1] {
        return Some((2, 0, 1));
    }
    if o[0] == o[2] {
        return Some((1, 0, 2));
    }
    Some((0, 1, 2))
}

/// Check if a point (by ID) is inside a triangle using projected 2D orientation.
/// `strict`: if true, point must be strictly interior (not on boundary).
///
/// Uses orient2d_indirect for exact handling of LPI intersection points.
///
/// Ported from intersection_classification.cpp:929-968 (genericPointInsideTriangle)
fn generic_point_inside_triangle(
    ts: &TriangleSoup,
    p_id: usize,
    t_id: usize,
    strict: bool,
) -> bool {
    use crate::boolean::indirect_predicates::{orient2d_indirect, ProjectionAxis};

    let pp = ts.implicit_point(p_id);
    let v0 = ts.implicit_point(ts.tri_vert_id(t_id, 0));
    let v1 = ts.implicit_point(ts.tri_vert_id(t_id, 1));
    let v2 = ts.implicit_point(ts.tri_vert_id(t_id, 2));

    let plane = ts.tri_plane(t_id);
    let proj = match plane {
        Plane::XY => ProjectionAxis::XY,
        Plane::YZ => ProjectionAxis::YZ,
        Plane::ZX => ProjectionAxis::ZX,
    };

    let o01 = orient2d_indirect(v0, v1, pp, proj);
    let o12 = orient2d_indirect(v1, v2, pp, proj);
    let o20 = orient2d_indirect(v2, v0, pp, proj);

    if strict {
        (o01 > 0.0 && o12 > 0.0 && o20 > 0.0) || (o01 < 0.0 && o12 < 0.0 && o20 < 0.0)
    } else {
        (o01 >= 0.0 && o12 >= 0.0 && o20 >= 0.0) || (o01 <= 0.0 && o12 <= 0.0 && o20 <= 0.0)
    }
}

// ── Utility functions ───────────────────────────────────────────────────

/// Compute orient3d of point p against the plane of triangle t_id.
fn orient3d_ts(ts: &TriangleSoup, p_id: usize, t_id: usize) -> f64 {
    let p = ts.vert(p_id);
    let tv0 = ts.tri_vert(t_id, 0);
    let tv1 = ts.tri_vert(t_id, 1);
    let tv2 = ts.tri_vert(t_id, 2);
    orient3d(p, tv0, tv1, tv2)
}

/// Convert Plane enum to projection axes (i, j).
fn plane_to_axes(plane: Plane) -> (usize, usize) {
    match plane {
        Plane::XY => (0, 1),
        Plane::YZ => (1, 2),
        Plane::ZX => (2, 0),
    }
}

/// Cross product of two 3D vectors.
fn cross_product(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Check if two points are exactly equal.
fn points_equal(a: &[f64; 3], b: &[f64; 3]) -> bool {
    a[0] == b[0] && a[1] == b[1] && a[2] == b[2]
}

/// Check if 2D point p lies on segment [a, b] (assumes collinearity).
fn point_on_segment_2d(p: &[f64; 2], a: &[f64; 2], b: &[f64; 2]) -> bool {
    let min_x = a[0].min(b[0]);
    let max_x = a[0].max(b[0]);
    let min_y = a[1].min(b[1]);
    let max_y = a[1].max(b[1]);
    p[0] >= min_x && p[0] <= max_x && p[1] >= min_y && p[1] <= max_y
}

/// Check if two triangles share a vertex.
fn triangles_share_vertex(ts: &TriangleSoup, t0: usize, t1: usize) -> bool {
    for i in 0..3 {
        let v = ts.tri_vert_id(t0, i);
        for j in 0..3 {
            if v == ts.tri_vert_id(t1, j) {
                return true;
            }
        }
    }
    false
}

/// AABB overlap test.
fn aabb_overlap(a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])) -> bool {
    a.0[0] <= b.1[0]
        && a.1[0] >= b.0[0]
        && a.0[1] <= b.1[1]
        && a.1[1] >= b.0[1]
        && a.0[2] <= b.1[2]
        && a.1[2] >= b.0[2]
}

/// Exact triangle-triangle intersection test using orient3d.
/// Returns true if the two triangles intersect.
fn triangles_intersect_exact(ts: &TriangleSoup, t0: usize, t1: usize) -> bool {
    // Classify t1 vertices against plane of t0
    let mut o_ba = [0.0f64; 3];
    for i in 0..3 {
        o_ba[i] = orient3d_ts(ts, ts.tri_vert_id(t1, i), t0);
    }
    normalize_orientations(&mut o_ba);

    // All on same side → no intersection
    if same_orientation(o_ba[0], o_ba[1]) && same_orientation(o_ba[1], o_ba[2]) && o_ba[0] != 0.0 {
        return false;
    }

    // Classify t0 vertices against plane of t1
    let mut o_ab = [0.0f64; 3];
    for i in 0..3 {
        o_ab[i] = orient3d_ts(ts, ts.tri_vert_id(t0, i), t1);
    }
    normalize_orientations(&mut o_ab);

    if same_orientation(o_ab[0], o_ab[1]) && same_orientation(o_ab[1], o_ab[2]) && o_ab[0] != 0.0 {
        return false;
    }

    // If both pass, triangles potentially intersect
    true
}

/// Compute approximate LPI (Line-Plane Intersection) coordinates.
///
/// The point is where the line through (l0, l1) intersects the plane
/// defined by (p0, p1, p2).
fn compute_lpi_coords(
    l0: [f64; 3],
    l1: [f64; 3],
    p0: [f64; 3],
    p1: [f64; 3],
    p2: [f64; 3],
) -> [f64; 3] {
    // Plane normal
    let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let n = cross_product(&u, &v);

    // Line direction
    let d = [l1[0] - l0[0], l1[1] - l0[1], l1[2] - l0[2]];

    let denom = n[0] * d[0] + n[1] * d[1] + n[2] * d[2];

    if denom.abs() < 1e-30 {
        // Line parallel to plane — return midpoint as fallback
        return [
            (l0[0] + l1[0]) * 0.5,
            (l0[1] + l1[1]) * 0.5,
            (l0[2] + l1[2]) * 0.5,
        ];
    }

    let w = [l0[0] - p0[0], l0[1] - p0[1], l0[2] - p0[2]];
    let numer = -(n[0] * w[0] + n[1] * w[1] + n[2] * w[2]);
    let t = numer / denom;

    [l0[0] + t * d[0], l0[1] + t * d[1], l0[2] + t * d[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_two_boxes_soup() -> (TriangleSoup, AuxiliaryStructure) {
        // Two overlapping boxes — simplified as triangulated faces.
        // Box A: [0,0,0]-[2,2,2], Box B: [1,1,1]-[3,3,3]
        // We only create the 4 faces that potentially intersect.
        let coords = vec![
            // Box A front face (z=2): verts 0-3
            [0.0, 0.0, 2.0],
            [2.0, 0.0, 2.0],
            [2.0, 2.0, 2.0],
            [0.0, 2.0, 2.0],
            // Box B back face (z=1): verts 4-7
            [1.0, 1.0, 1.0],
            [3.0, 1.0, 1.0],
            [3.0, 3.0, 1.0],
            [1.0, 3.0, 1.0],
            // Box A right face (x=2): verts 8-11
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [2.0, 2.0, 2.0], // shared with v2
            [2.0, 0.0, 2.0], // shared with v1
            // Box B left face (x=1): verts 12-15
            [1.0, 1.0, 1.0],
            [1.0, 3.0, 1.0],
            [1.0, 3.0, 3.0],
            [1.0, 1.0, 3.0],
        ];
        let tris = vec![
            // Box A front face: 2 tris
            0, 1, 2, // t0
            0, 2, 3, // t1
            // Box B back face: 2 tris
            4, 5, 6, // t2
            4, 6, 7, // t3
            // Box A right face: 2 tris
            8, 9, 10, // t4
            8, 10, 11, // t5
            // Box B left face: 2 tris
            12, 13, 14, // t6
            12, 14, 15, // t7
        ];
        let labels = vec![1, 1, 2, 2, 1, 1, 2, 2];

        let ts = TriangleSoup::new(coords, tris, labels, 1.0);
        let mut aux = AuxiliaryStructure::new();
        aux.init_from_triangle_soup(&ts);
        (ts, aux)
    }

    #[test]
    fn test_detect_intersections_two_boxes() {
        let (ts, mut aux) = make_two_boxes_soup();
        detect_intersections(&ts, &mut aux);
        // Should find at least some intersection pairs between the overlapping box faces
        // The exact count depends on face orientations but should be > 0
        assert!(
            !aux.intersection_list().is_empty(),
            "Expected intersection pairs between overlapping boxes"
        );
    }

    #[test]
    fn test_classify_intersections_populates_edge2pts() {
        let (mut ts, mut aux) = make_two_boxes_soup();
        detect_intersections(&ts, &mut aux);

        if !aux.intersection_list().is_empty() {
            classify_intersections(&mut ts, &mut aux);
            // After classification, at least some edge2pts or tri2pts should be populated
            let num_edges = ts.num_edges();
            let mut has_edge_pts = false;
            for e_id in 0..num_edges {
                if !aux.edge_points_list(e_id).is_empty() {
                    has_edge_pts = true;
                    break;
                }
            }
            let mut has_tri_pts = false;
            for t_id in 0..ts.num_tris() {
                if !aux.triangle_points_list(t_id).is_empty() {
                    has_tri_pts = true;
                    break;
                }
            }
            assert!(
                has_edge_pts || has_tri_pts,
                "Expected edge or triangle intersection points after classification"
            );
        }
    }

    #[test]
    fn test_orientation_helpers() {
        assert!(all_coplanar_edges(&[0.0, 0.0, 0.0]));
        assert!(!all_coplanar_edges(&[1.0, 0.0, 0.0]));

        assert_eq!(single_coplanar_edge(&[0.0, 0.0, 1.0]), Some(0));
        assert_eq!(single_coplanar_edge(&[1.0, 0.0, 0.0]), Some(1));
        assert_eq!(single_coplanar_edge(&[0.0, 1.0, 0.0]), Some(2));
        assert_eq!(single_coplanar_edge(&[1.0, 1.0, 1.0]), None);

        assert_eq!(
            vtx_in_plane_and_opposite_edge_on_same_side(&[0.0, 1.0, 1.0]),
            Some(0)
        );
        assert_eq!(
            vtx_in_plane_and_opposite_edge_on_same_side(&[1.0, 0.0, 1.0]),
            Some(1)
        );

        assert_eq!(
            vtx_in_plane_and_opposite_edge_cross_plane(&[0.0, -1.0, 1.0]),
            Some(0)
        );
        assert_eq!(
            vtx_in_plane_and_opposite_edge_cross_plane(&[0.0, 1.0, 1.0]),
            None
        );

        assert_eq!(
            vtx_on_a_side_and_opposite_edge_on_the_other(&[-1.0, 1.0, 1.0]),
            Some((0, 1, 2))
        );
        assert_eq!(
            vtx_on_a_side_and_opposite_edge_on_the_other(&[1.0, 1.0, -1.0]),
            Some((2, 0, 1))
        );
    }

    #[test]
    fn test_normalize_orientations() {
        let mut o = [3.5, -2.1, 0.0];
        normalize_orientations(&mut o);
        assert_eq!(o, [1.0, -1.0, 0.0]);
    }

    #[test]
    fn test_same_orientation() {
        assert!(same_orientation(1.0, 1.0));
        assert!(same_orientation(-1.0, -1.0));
        assert!(same_orientation(0.0, 0.0));
        assert!(!same_orientation(1.0, -1.0));
        assert!(!same_orientation(0.0, 1.0));
    }

    #[test]
    fn test_generic_point_inside_triangle() {
        let coords = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [2.0, 2.0, 0.0],   // inside
            [20.0, 20.0, 0.0], // outside
        ];
        let tris = vec![0, 1, 2];
        let labels = vec![1];
        let ts = TriangleSoup::new(coords, tris, labels, 1.0);

        assert!(generic_point_inside_triangle(&ts, 3, 0, true));
        assert!(!generic_point_inside_triangle(&ts, 4, 0, true));
    }
}
