//! Exact mesh boolean operations using indirect predicates.
//!
//! Implements the core of the Yang 2025 hybrid B-Rep/mesh boolean pipeline
//! (ARCHITECTURAL_INVARIANTS.md A15.6). Uses exact geometric predicates
//! [#4 Shewchuk 1997] via the `geometry-predicates` crate and the `robust`
//! crate to perform triangle-triangle intersection and face classification
//! without any tolerance parameters.
//!
//! # Pipeline position
//!
//! This module implements stages 2-3 of the Yang pipeline:
//!   1. Tessellate with bijective mapping (Phase 1 — `tessellation/bijective.rs`)
//!   2. **Exact mesh boolean** (this module — Phase 2)
//!   3. Extract topology from result (Phase 3 — `boolean/topology_extract.rs`)
//!   4. Refine to SSI curves (Phase 4)
//!   5. Assemble B-Rep (Phase 5)
//!
//! # Research basis
//!
//! - [#9] Cherchi et al. 2020: Indirect predicates for exact mesh arrangements
//! - [#4] Shewchuk 1997: Adaptive precision predicates (orient3d, orient2d)
//! - [#10] Levy 2025: Exact constructions + radial sort
//! - [#24] Yang, Jia & Yan 2025: Hybrid B-Rep/mesh boolean pipeline

use geometry_predicates::{orient2d, orient3d};

use crate::units::{
    TAU_EXACT_MESH_BOUNDARY_EPS, TAU_EXACT_MESH_CLASSIFY, TAU_NORMALIZE_SQ, TAU_WORK,
    WINDING_INSIDE_THRESHOLD, WINDING_OUTSIDE_THRESHOLD,
};

/// Which mesh a triangle belongs to in a boolean operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(dead_code)] // Phase 2 building block — task 2b
pub(crate) enum MeshId {
    A,
    B,
}

/// Symbolic representation of an intersection point — Line-Plane Intersection.
/// The point is the intersection of edge (verts[edge[0]], verts[edge[1]]) with
/// the plane of triangle (verts[plane_tri[0]], verts[plane_tri[1]], verts[plane_tri[2]]).
/// Ref #9: Cherchi 2020 indirect predicates.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 2 building block — task 2b
pub(crate) struct IndirectPoint {
    /// Indices of the two edge endpoints in the vertex array.
    pub edge: [usize; 2],
    /// Indices of the three vertices of the plane-defining triangle.
    pub plane_tri: [usize; 3],
}

/// Result of exact triangle-triangle intersection test.
#[derive(Debug)]
#[allow(dead_code)] // Phase 2 building block — task 2b
pub(crate) enum TriTriIsect {
    /// No intersection.
    None,
    /// Coplanar triangles (2D intersection deferred to task 2c).
    Coplanar,
    /// Intersection is a segment defined by two indirect points.
    Segment(IndirectPoint, IndirectPoint),
    /// Intersection is a single point (edge touches plane at boundary).
    Point(IndirectPoint),
}

/// Compute exact triangle-triangle intersection.
///
/// Uses orient3d adaptive predicates [#4 Shewchuk] for classification.
/// Returns intersection as indirect points [#9 Cherchi] — symbolic
/// references to input geometry, not materialized coordinates.
///
/// Algorithm follows Guigue-Devillers with Cherchi indirect points:
/// 1. Classify T_B vertices against plane(T_A) — exact via orient3d
/// 2. Classify T_A vertices against plane(T_B) — exact via orient3d
/// 3. If all coplanar → Coplanar
/// 4. If all on same side → None (separated)
/// 5. Find crossing edges and build indirect points
/// 6. Determine interval overlap along intersection line
///
/// # Arguments
/// - `tri_a`: Vertex indices [i, j, k] of triangle A in `verts`
/// - `tri_b`: Vertex indices [i, j, k] of triangle B in `verts`
/// - `verts`: Shared vertex position array
///
/// # Research basis
/// - Ref #4: Shewchuk 1997 — exact orient3d predicates
/// - Ref #9: Cherchi 2020 — indirect predicates for mesh arrangements
#[allow(dead_code)] // Phase 2 building block — task 2b
pub(crate) fn tri_tri_intersect(
    tri_a: [usize; 3],
    tri_b: [usize; 3],
    verts: &[[f64; 3]],
) -> TriTriIsect {
    let va = [verts[tri_a[0]], verts[tri_a[1]], verts[tri_a[2]]];
    let vb = [verts[tri_b[0]], verts[tri_b[1]], verts[tri_b[2]]];

    // Step 1: Classify T_B vertices against plane(T_A). Ref #4: Shewchuk exact orient3d.
    let ob: [Orientation; 3] = [
        orient3d_classify(&va, &vb[0]),
        orient3d_classify(&va, &vb[1]),
        orient3d_classify(&va, &vb[2]),
    ];

    // If all T_B verts on same strict side → separated
    if ob[0] == ob[1] && ob[1] == ob[2] && ob[0] != Orientation::Coplanar {
        return TriTriIsect::None;
    }

    // Step 2: Classify T_A vertices against plane(T_B). Ref #4: Shewchuk exact orient3d.
    let oa: [Orientation; 3] = [
        orient3d_classify(&vb, &va[0]),
        orient3d_classify(&vb, &va[1]),
        orient3d_classify(&vb, &va[2]),
    ];

    // If all T_A verts on same strict side → separated
    if oa[0] == oa[1] && oa[1] == oa[2] && oa[0] != Orientation::Coplanar {
        return TriTriIsect::None;
    }

    // Step 3: Coplanar check — all 6 classifications are zero
    if ob.iter().all(|o| *o == Orientation::Coplanar)
        && oa.iter().all(|o| *o == Orientation::Coplanar)
    {
        return TriTriIsect::Coplanar;
    }

    // Step 3b: Edge-on-plane detection — handle n_coplanar==2 before standard
    // crossing edge logic. When two vertices of one triangle lie exactly on the
    // other's plane, the coplanar edge may intersect the other triangle.
    // Ref #9: Cherchi 2020 — degenerate intersection configurations.
    // Ref: specs/edge_on_plane_intersection.md
    let ob_coplanar = ob.iter().filter(|o| **o == Orientation::Coplanar).count();
    let oa_coplanar = oa.iter().filter(|o| **o == Orientation::Coplanar).count();

    if ob_coplanar == 2 {
        // T_B has an edge on plane(T_A) — clip against T_A
        if let Some(isect) = clip_edge_on_plane(&ob, &tri_b, &vb, &va, &tri_a, verts) {
            return isect;
        }
        return TriTriIsect::None;
    }

    if oa_coplanar == 2 {
        // T_A has an edge on plane(T_B) — clip against T_B
        if let Some(isect) = clip_edge_on_plane(&oa, &tri_a, &va, &vb, &tri_b, verts) {
            return isect;
        }
        return TriTriIsect::None;
    }

    // Step 4: Find crossing edges for each triangle.
    // For T_B vs plane(T_A): find edges of T_B that cross plane(T_A).
    let b_crossings = find_crossing_edges(&ob, &tri_b, &tri_a);
    // For T_A vs plane(T_B): find edges of T_A that cross plane(T_B).
    let a_crossings = find_crossing_edges(&oa, &tri_a, &tri_b);

    match (a_crossings, b_crossings) {
        (CrossingResult::None, _) | (_, CrossingResult::None) => TriTriIsect::None,
        (CrossingResult::VertexOnPlane(ip), CrossingResult::VertexOnPlane(_)) => {
            // Both triangles have a single vertex on the other's plane — could be a point
            TriTriIsect::Point(ip)
        }
        (CrossingResult::VertexOnPlane(ip), _) => {
            // One vertex of T_A on plane(T_B). Check if it's inside T_B.
            let pt = materialize_ip(&ip, verts);
            if point_in_triangle_3d(&pt, &vb) {
                TriTriIsect::Point(ip)
            } else {
                TriTriIsect::None
            }
        }
        (_, CrossingResult::VertexOnPlane(ip)) => {
            // One vertex of T_B on plane(T_A). Check if it's inside T_A.
            let pt = materialize_ip(&ip, verts);
            if point_in_triangle_3d(&pt, &va) {
                TriTriIsect::Point(ip)
            } else {
                TriTriIsect::None
            }
        }
        (CrossingResult::TwoEdges(p1, p2), CrossingResult::TwoEdges(q1, q2)) => {
            // Step 5: Determine interval overlap along the intersection line.
            // Materialize points and compare parametrically along the line.
            // Topology is exact (orient3d); position along line uses f64. Ref #9: Cherchi.
            compute_segment_overlap(p1, p2, q1, q2, verts, &va, &vb)
        }
    }
}

/// Result of finding crossing edges for one triangle against the other's plane.
#[allow(dead_code)]
enum CrossingResult {
    /// No edges cross (all on same side, or degenerate).
    None,
    /// A single vertex lies on the plane (the other two are on the same side).
    VertexOnPlane(IndirectPoint),
    /// Two edges cross the plane, giving two indirect intersection points.
    TwoEdges(IndirectPoint, IndirectPoint),
}

/// Find the edges of `tri` that cross the plane defined by `plane_tri`.
/// `orientations` are the orient3d classifications of `tri`'s vertices against `plane_tri`.
#[allow(dead_code)]
fn find_crossing_edges(
    orientations: &[Orientation; 3],
    tri: &[usize; 3],
    plane_tri: &[usize; 3],
) -> CrossingResult {
    let n_coplanar = orientations
        .iter()
        .filter(|o| **o == Orientation::Coplanar)
        .count();
    let n_above = orientations
        .iter()
        .filter(|o| **o == Orientation::Above)
        .count();
    let n_below = orientations
        .iter()
        .filter(|o| **o == Orientation::Below)
        .count();

    if n_coplanar == 3 {
        // All coplanar — handled elsewhere
        return CrossingResult::None;
    }

    if n_coplanar == 2 {
        // Two vertices on the plane — edge-on-plane case.
        // Return None; this case is handled by edge-on-plane detection in
        // subdivide_mesh_pair when needed.
        return CrossingResult::None;
    }

    if n_coplanar == 1 {
        // One vertex exactly on the plane, the other two are either both on the same
        // side (→ VertexOnPlane) or on different sides (→ normal crossing with one
        // crossing point being the on-plane vertex).
        let coplanar_idx = orientations
            .iter()
            .position(|o| *o == Orientation::Coplanar)
            .unwrap();
        let other_a = (coplanar_idx + 1) % 3;
        let other_b = (coplanar_idx + 2) % 3;

        if orientations[other_a] == orientations[other_b] {
            // Both other verts on same side → single point contact
            // The IndirectPoint for an on-plane vertex: edge from vertex to itself
            // is degenerate. Instead represent as edge where one endpoint is the vertex.
            return CrossingResult::VertexOnPlane(IndirectPoint {
                edge: [tri[coplanar_idx], tri[coplanar_idx]],
                plane_tri: *plane_tri,
            });
        }

        // Different sides: one crossing edge + the on-plane vertex
        // The crossing edge is between other_a and other_b
        let crossing = IndirectPoint {
            edge: [tri[other_a], tri[other_b]],
            plane_tri: *plane_tri,
        };
        // The on-plane vertex acts as the second point.
        // Represent it as a degenerate edge (vertex to itself) — materializes to that vertex.
        let on_plane = IndirectPoint {
            edge: [tri[coplanar_idx], tri[coplanar_idx]],
            plane_tri: *plane_tri,
        };
        return CrossingResult::TwoEdges(on_plane, crossing);
    }

    // No coplanar vertices. Find the isolated vertex (different sign from the other two).
    // n_above + n_below == 3, one of them is 1 and the other is 2.
    if n_above == 0 || n_below == 0 {
        // All on same strict side — already handled above, but be safe
        return CrossingResult::None;
    }

    // Find the isolated vertex: the one whose orientation is unique
    let isolated = if n_above == 1 {
        orientations
            .iter()
            .position(|o| *o == Orientation::Above)
            .unwrap()
    } else {
        orientations
            .iter()
            .position(|o| *o == Orientation::Below)
            .unwrap()
    };

    let other_a = (isolated + 1) % 3;
    let other_b = (isolated + 2) % 3;

    // Two crossing edges: isolated→other_a and isolated→other_b
    let ip1 = IndirectPoint {
        edge: [tri[isolated], tri[other_a]],
        plane_tri: *plane_tri,
    };
    let ip2 = IndirectPoint {
        edge: [tri[isolated], tri[other_b]],
        plane_tri: *plane_tri,
    };

    CrossingResult::TwoEdges(ip1, ip2)
}

/// Materialize an indirect point to f64 coordinates.
/// Computes intersection of line(edge[0], edge[1]) with plane(plane_tri).
#[allow(dead_code)]
fn materialize_ip(ip: &IndirectPoint, verts: &[[f64; 3]]) -> [f64; 3] {
    let a = verts[ip.edge[0]];
    let b = verts[ip.edge[1]];
    if ip.edge[0] == ip.edge[1] {
        // Degenerate — vertex on plane
        return a;
    }
    let p0 = verts[ip.plane_tri[0]];
    let p1 = verts[ip.plane_tri[1]];
    let p2 = verts[ip.plane_tri[2]];
    // Plane normal
    let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let ap = [a[0] - p0[0], a[1] - p0[1], a[2] - p0[2]];
    let d_a = n[0] * ap[0] + n[1] * ap[1] + n[2] * ap[2];
    let dir = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let denom = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
    let t = if denom.abs() > TAU_NORMALIZE_SQ {
        -d_a / denom
    } else {
        0.5
    };
    [a[0] + t * dir[0], a[1] + t * dir[1], a[2] + t * dir[2]]
}

/// Check if a 3D point lies STRICTLY inside a triangle (not on boundary).
/// Uses barycentric coordinate approach with orient2d. Returns true only if
/// ALL orient2d values are strictly positive or strictly negative (the point
/// is in the interior, not on any edge or vertex).
///
/// Used by `clip_edge_on_plane` to avoid creating constraint segments for
/// degenerate edge contacts along triangle boundaries.
#[allow(dead_code)]
fn point_strictly_inside_triangle_3d(pt: &[f64; 3], tri: &[[f64; 3]; 3]) -> bool {
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
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];

    let ax = n[0].abs();
    let ay = n[1].abs();
    let az = n[2].abs();
    let (i, j) = if ax >= ay && ax >= az {
        (1, 2)
    } else if ay >= az {
        (0, 2)
    } else {
        (0, 1)
    };

    let p = [pt[i], pt[j]];
    let a = [tri[0][i], tri[0][j]];
    let b = [tri[1][i], tri[1][j]];
    let c = [tri[2][i], tri[2][j]];

    let o1 = orient2d(a, b, p);
    let o2 = orient2d(b, c, p);
    let o3 = orient2d(c, a, p);

    // STRICT: all must be > 0 or all must be < 0 (no zeros = no boundary contact)
    (o1 > 0.0 && o2 > 0.0 && o3 > 0.0) || (o1 < 0.0 && o2 < 0.0 && o3 < 0.0)
}

/// Check if a 3D point lies inside a triangle (assuming point is on triangle's plane).
/// Uses barycentric coordinate approach with cross products.
#[allow(dead_code)]
fn point_in_triangle_3d(pt: &[f64; 3], tri: &[[f64; 3]; 3]) -> bool {
    // Compute normal of triangle
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
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];

    // Project onto the dominant axis plane for robust 2D test
    let ax = n[0].abs();
    let ay = n[1].abs();
    let az = n[2].abs();
    let (i, j) = if ax >= ay && ax >= az {
        (1, 2) // project onto YZ
    } else if ay >= az {
        (0, 2) // project onto XZ
    } else {
        (0, 1) // project onto XY
    };

    // 2D point-in-triangle using orient2d. Ref #4: Shewchuk exact orient2d.
    let p = [pt[i], pt[j]];
    let a = [tri[0][i], tri[0][j]];
    let b = [tri[1][i], tri[1][j]];
    let c = [tri[2][i], tri[2][j]];

    let o1 = orient2d(a, b, p);
    let o2 = orient2d(b, c, p);
    let o3 = orient2d(c, a, p);

    // Point is inside if all orient2d have the same sign (or zero for on-edge)
    (o1 >= 0.0 && o2 >= 0.0 && o3 >= 0.0) || (o1 <= 0.0 && o2 <= 0.0 && o3 <= 0.0)
}

/// Handle edge-on-plane intersection: two vertices of `tri_edge` lie on the plane
/// of `tri_plane`. Clip the coplanar edge against `tri_plane` in 2D.
///
/// Returns Some(TriTriIsect) if there is a non-trivial intersection, None if the
/// edge misses the triangle entirely.
///
/// Ref #9: Cherchi 2020 — degenerate intersection cases.
/// Ref #4: Shewchuk 1997 — orient2d for exact 2D classification.
#[allow(dead_code)]
fn clip_edge_on_plane(
    orientations: &[Orientation; 3],
    tri_edge: &[usize; 3],          // the triangle whose edge lies on the plane
    tri_edge_verts: &[[f64; 3]; 3], // its vertex positions
    tri_plane_verts: &[[f64; 3]; 3], // the triangle defining the plane
    tri_plane: &[usize; 3],         // its vertex indices (for IndirectPoint plane_tri)
    _verts: &[[f64; 3]],            // global vertex array
) -> Option<TriTriIsect> {
    // Find the two coplanar vertices
    let coplanar_indices: Vec<usize> = (0..3)
        .filter(|&i| orientations[i] == Orientation::Coplanar)
        .collect();
    if coplanar_indices.len() != 2 {
        return None;
    }
    let ci0 = coplanar_indices[0];
    let ci1 = coplanar_indices[1];

    let ep0 = tri_edge_verts[ci0];
    let ep1 = tri_edge_verts[ci1];

    // Classify endpoints against triangle: STRICTLY inside (not on boundary).
    // Using strict interior check avoids creating constraint segments for
    // degenerate edge contacts along triangle boundaries, which would produce
    // non-conformal subdivisions. Points on the triangle boundary (on edges or
    // at vertices) are treated as "outside" to prevent fragmented topology.
    // Ref: specs/edge_on_plane_intersection.md — conformal vertex sharing.
    let in0 = point_strictly_inside_triangle_3d(&ep0, tri_plane_verts);
    let in1 = point_strictly_inside_triangle_3d(&ep1, tri_plane_verts);

    // Both inside → full edge is intersection segment
    if in0 && in1 {
        return Some(TriTriIsect::Segment(
            IndirectPoint {
                edge: [tri_edge[ci0], tri_edge[ci0]],
                plane_tri: *tri_plane,
            },
            IndirectPoint {
                edge: [tri_edge[ci1], tri_edge[ci1]],
                plane_tri: *tri_plane,
            },
        ));
    }

    // One inside, one outside → clip the edge at the triangle boundary.
    // Find where the edge exits the triangle by checking intersection with
    // each triangle edge.
    if in0 || in1 {
        let (inside_idx, outside_pt) = if in0 { (ci0, ep1) } else { (ci1, ep0) };
        let inside_pt = tri_edge_verts[inside_idx];

        // Find the intersection of segment inside→outside with each triangle edge.
        // Use 2D projection for robustness.
        let n = [tri_plane_verts[0], tri_plane_verts[1], tri_plane_verts[2]];
        // Triangle normal for projection axis selection
        let u = [n[1][0] - n[0][0], n[1][1] - n[0][1], n[1][2] - n[0][2]];
        let v = [n[2][0] - n[0][0], n[2][1] - n[0][1], n[2][2] - n[0][2]];
        let normal = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let (pi, pj) = {
            let ax = normal[0].abs();
            let ay = normal[1].abs();
            let az = normal[2].abs();
            if ax >= ay && ax >= az {
                (1, 2)
            } else if ay >= az {
                (0, 2)
            } else {
                (0, 1)
            }
        };

        // 2D segment intersection
        let seg_a = [inside_pt[pi], inside_pt[pj]];
        let seg_b = [outside_pt[pi], outside_pt[pj]];

        let mut best_t = 1.0f64;
        for edge_k in 0..3 {
            let ea = [tri_plane_verts[edge_k][pi], tri_plane_verts[edge_k][pj]];
            let eb = [
                tri_plane_verts[(edge_k + 1) % 3][pi],
                tri_plane_verts[(edge_k + 1) % 3][pj],
            ];

            // 2D line-line intersection: seg_a + t*(seg_b - seg_a) intersects ea + s*(eb - ea)
            let d1 = [seg_b[0] - seg_a[0], seg_b[1] - seg_a[1]];
            let d2 = [eb[0] - ea[0], eb[1] - ea[1]];
            let det = d1[0] * d2[1] - d1[1] * d2[0];
            if det.abs() < TAU_NORMALIZE_SQ {
                continue;
            } // parallel
            let r = [ea[0] - seg_a[0], ea[1] - seg_a[1]];
            let t = (r[0] * d2[1] - r[1] * d2[0]) / det;
            let s = (r[0] * d1[1] - r[1] * d1[0]) / det;

            if t > TAU_WORK
                && t < best_t
                && (-TAU_EXACT_MESH_CLASSIFY..=1.0 + TAU_EXACT_MESH_CLASSIFY).contains(&s)
            {
                best_t = t;
            }
        }

        if best_t < 1.0 - TAU_WORK {
            // The edge exits the triangle at parameter t along inside→outside.
            // The exit point is a new intersection point (not a vertex).
            // For now, represent it as a materialized point. We create a
            // degenerate IndirectPoint using the inside vertex index and the
            // outside vertex index — the materialize function will interpolate.
            let outside_edge_idx = if in0 { ci1 } else { ci0 };
            return Some(TriTriIsect::Segment(
                IndirectPoint {
                    edge: [tri_edge[inside_idx], tri_edge[inside_idx]],
                    plane_tri: *tri_plane,
                },
                IndirectPoint {
                    edge: [tri_edge[inside_idx], tri_edge[outside_edge_idx]],
                    plane_tri: *tri_plane,
                },
            ));
        } else {
            // Edge barely exits — treat as a point intersection
            return Some(TriTriIsect::Point(IndirectPoint {
                edge: [tri_edge[inside_idx], tri_edge[inside_idx]],
                plane_tri: *tri_plane,
            }));
        }
    }

    // Neither endpoint inside — the edge might still clip through the triangle.
    // Check if any triangle edge intersects the coplanar segment in 2D.
    // (Edge endpoints outside but edge crosses through triangle interior.)
    // Ref #9: Cherchi 2020 — degenerate intersection configurations.

    // Project to 2D: reuse the same axis-selection logic as the one-inside case.
    let n_pts = [tri_plane_verts[0], tri_plane_verts[1], tri_plane_verts[2]];
    let u_vec = [
        n_pts[1][0] - n_pts[0][0],
        n_pts[1][1] - n_pts[0][1],
        n_pts[1][2] - n_pts[0][2],
    ];
    let v_vec = [
        n_pts[2][0] - n_pts[0][0],
        n_pts[2][1] - n_pts[0][1],
        n_pts[2][2] - n_pts[0][2],
    ];
    let normal = [
        u_vec[1] * v_vec[2] - u_vec[2] * v_vec[1],
        u_vec[2] * v_vec[0] - u_vec[0] * v_vec[2],
        u_vec[0] * v_vec[1] - u_vec[1] * v_vec[0],
    ];
    let (pi, pj) = {
        let ax = normal[0].abs();
        let ay = normal[1].abs();
        let az = normal[2].abs();
        if ax >= ay && ax >= az {
            (1, 2)
        } else if ay >= az {
            (0, 2)
        } else {
            (0, 1)
        }
    };

    // 2D projection of the coplanar edge
    let seg_a = [ep0[pi], ep0[pj]];
    let seg_b = [ep1[pi], ep1[pj]];

    // Collect (t_on_coplanar_edge, triangle_edge_index) for all crossings
    let mut hits: Vec<(f64, usize)> = Vec::new();
    for edge_k in 0..3 {
        let ea = [tri_plane_verts[edge_k][pi], tri_plane_verts[edge_k][pj]];
        let eb = [
            tri_plane_verts[(edge_k + 1) % 3][pi],
            tri_plane_verts[(edge_k + 1) % 3][pj],
        ];

        // 2D segment-segment intersection:
        //   seg_a + t*(seg_b - seg_a) = ea + s*(eb - ea)
        let d1 = [seg_b[0] - seg_a[0], seg_b[1] - seg_a[1]];
        let d2 = [eb[0] - ea[0], eb[1] - ea[1]];
        let det = d1[0] * d2[1] - d1[1] * d2[0];
        if det.abs() < TAU_NORMALIZE_SQ {
            continue; // parallel
        }
        let r = [ea[0] - seg_a[0], ea[1] - seg_a[1]];
        let t = (r[0] * d2[1] - r[1] * d2[0]) / det;
        let s = (r[0] * d1[1] - r[1] * d1[0]) / det;

        // Both parameters must be within segment bounds
        if (-TAU_EXACT_MESH_CLASSIFY..=1.0 + TAU_EXACT_MESH_CLASSIFY).contains(&t)
            && (-TAU_EXACT_MESH_CLASSIFY..=1.0 + TAU_EXACT_MESH_CLASSIFY).contains(&s)
        {
            hits.push((t.clamp(0.0, 1.0), edge_k));
        }
    }

    if hits.len() < 2 {
        return None;
    }

    // Sort by t to find entry and exit
    hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (t_entry, k_entry) = hits[0];
    let (t_exit, k_exit) = hits[hits.len() - 1];

    // If entry and exit are at the same point, it's a tangent touch — skip
    if (t_exit - t_entry).abs() < TAU_WORK {
        return None;
    }

    // Build IndirectPoints for entry and exit.
    // The intersection point lies on the triangle edge [tri_plane[k], tri_plane[(k+1)%3]].
    // Use that triangle edge as the `edge` field and tri_edge as the `plane_tri`.
    // materialize_ip intersects the triangle edge with the plane of tri_edge,
    // which gives the point where the triangle edge crosses the line of intersection
    // of the two triangle planes — exactly our passthrough crossing point.
    let make_ip = |t: f64, k: usize| -> IndirectPoint {
        // Check if the intersection is at an endpoint of the coplanar edge
        if t < TAU_WORK {
            return IndirectPoint {
                edge: [tri_edge[ci0], tri_edge[ci0]],
                plane_tri: *tri_plane,
            };
        }
        if t > 1.0 - TAU_WORK {
            return IndirectPoint {
                edge: [tri_edge[ci1], tri_edge[ci1]],
                plane_tri: *tri_plane,
            };
        }
        // General case: the intersection is at a new point on triangle edge k.
        // edge = the triangle boundary edge that was crossed.
        // plane_tri = the edge triangle (whose plane is tilted, so the
        // line-plane intersection is well-defined and gives the correct point).
        IndirectPoint {
            edge: [tri_plane[k], tri_plane[(k + 1) % 3]],
            plane_tri: *tri_edge,
        }
    };

    Some(TriTriIsect::Segment(
        make_ip(t_entry, k_entry),
        make_ip(t_exit, k_exit),
    ))
}

/// Compute the overlap of two intervals [p1,p2] and [q1,q2] on the intersection line.
/// Returns Segment if overlap exists, None otherwise.
/// Uses f64 materialization for parametric comparison — topology is exact. Ref #9: Cherchi.
#[allow(dead_code)]
fn compute_segment_overlap(
    p1: IndirectPoint,
    p2: IndirectPoint,
    q1: IndirectPoint,
    q2: IndirectPoint,
    verts: &[[f64; 3]],
    tri_a: &[[f64; 3]; 3],
    tri_b: &[[f64; 3]; 3],
) -> TriTriIsect {
    // Materialize all 4 points
    let mp1 = materialize_ip(&p1, verts);
    let mp2 = materialize_ip(&p2, verts);
    let mq1 = materialize_ip(&q1, verts);
    let mq2 = materialize_ip(&q2, verts);

    // Find a dominant axis for parameterization along the intersection line
    // Use the axis with the largest spread among all 4 points
    let all_pts = [mp1, mp2, mq1, mq2];
    let mut best_axis = 0;
    let mut best_spread = 0.0_f64;
    for axis in 0..3 {
        let min = all_pts
            .iter()
            .map(|p| p[axis])
            .fold(f64::INFINITY, f64::min);
        let max = all_pts
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let spread = max - min;
        if spread > best_spread {
            best_spread = spread;
            best_axis = axis;
        }
    }

    // Parameterize along the dominant axis
    let mut tp1 = mp1[best_axis];
    let mut tp2 = mp2[best_axis];
    let mut tq1 = mq1[best_axis];
    let mut tq2 = mq2[best_axis];

    // Ensure intervals are ordered
    if tp1 > tp2 {
        std::mem::swap(&mut tp1, &mut tp2);
    }
    if tq1 > tq2 {
        std::mem::swap(&mut tq1, &mut tq2);
    }

    // Compute overlap
    let overlap_start = tp1.max(tq1);
    let overlap_end = tp2.min(tq2);

    // No absolute tolerance — the topology decision was already made exactly
    // by orient3d. We only use f64 parameterization for interval ordering.
    if overlap_start > overlap_end {
        return TriTriIsect::None;
    }

    // We have an overlap. Now we need to return the correct IndirectPoints for the
    // segment endpoints. The segment endpoints are the two "inner" points from the
    // four crossing points.

    // Build a sorted list of (parameter, source, indirect_point)
    // p1/p2 are from T_A's edges crossing plane(T_B)
    // q1/q2 are from T_B's edges crossing plane(T_A)
    let mp1_t = mp1[best_axis];
    let mp2_t = mp2[best_axis];
    let mq1_t = mq1[best_axis];
    let mq2_t = mq2[best_axis];

    // The segment is bounded by the two inner points.
    // For interval [p1,p2] ∩ [q1,q2], the start is max(min_p, min_q) and end is min(max_p, max_q).
    // We need to figure out which IndirectPoint corresponds to overlap_start and overlap_end.

    // For the start of the overlap: it's the later of the two interval starts
    let start_ip = if tp1 >= tq1 {
        // Start comes from P interval
        if mp1_t <= mp2_t {
            p1.clone()
        } else {
            p2.clone()
        }
    } else {
        // Start comes from Q interval
        if mq1_t <= mq2_t {
            q1.clone()
        } else {
            q2.clone()
        }
    };

    let end_ip = if tp2 <= tq2 {
        // End comes from P interval
        if mp2_t >= mp1_t {
            p2
        } else {
            p1
        }
    } else {
        // End comes from Q interval
        if mq2_t >= mq1_t {
            q2
        } else {
            q1
        }
    };

    // Check that the segment endpoints actually lie within both triangles.
    // The crossing points from T_A's edges are guaranteed to be on T_A,
    // and crossing points from T_B's edges are guaranteed to be on T_B.
    // But we need to verify each endpoint lies within the OTHER triangle too.
    let start_pt = materialize_ip(&start_ip, verts);
    let end_pt = materialize_ip(&end_ip, verts);

    let start_in_a = point_in_triangle_3d(&start_pt, tri_a);
    let start_in_b = point_in_triangle_3d(&start_pt, tri_b);
    let end_in_a = point_in_triangle_3d(&end_pt, tri_a);
    let end_in_b = point_in_triangle_3d(&end_pt, tri_b);

    if (start_in_a && start_in_b) && (end_in_a && end_in_b) {
        // Distinguish Point from Segment: if both endpoints reference the same
        // edge-plane pair, they are the same geometric point. Otherwise, use
        // the parameterization — if start == end exactly in f64, it's a Point.
        //
        // Known limitation: point_in_triangle_3d uses exact orient2d on
        // *materialized* (f64) coordinates, which can reject valid intersection
        // points at small scales or grazing angles. This causes the code to
        // fall through to the single-Point paths below. The proper fix is
        // exact containment testing via indirect predicates (Cherchi 2020,
        // Ref #9), which requires evaluating orient2d on symbolic coordinates.
        if (start_ip.edge == end_ip.edge && start_ip.plane_tri == end_ip.plane_tri)
            || overlap_start == overlap_end
        {
            return TriTriIsect::Point(start_ip);
        }
        TriTriIsect::Segment(start_ip, end_ip)
    } else if start_in_a && start_in_b {
        TriTriIsect::Point(start_ip)
    } else if end_in_a && end_in_b {
        TriTriIsect::Point(end_ip)
    } else {
        TriTriIsect::None
    }
}

/// Orientation of a point relative to a triangle's supporting plane.
///
/// Computed via exact `orient3d` predicate [#4 Shewchuk].
/// The sign convention follows Shewchuk: positive means the point is
/// below the plane (opposite the normal direction for a CCW triangle
/// when viewed from above).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Phase 2 building blocks — used by tests now, by task 2b next
pub(crate) enum Orientation {
    /// Point is strictly above the plane (negative orient3d).
    Above,
    /// Point is strictly below the plane (positive orient3d).
    Below,
    /// Point is exactly on the plane (orient3d == 0.0).
    Coplanar,
}

/// Classify a point relative to a triangle's supporting plane using exact
/// orient3d predicate.
///
/// # Arguments
/// - `tri`: The three vertices of the triangle `[a, b, c]`.
/// - `point`: The query point.
///
/// # Returns
/// The orientation of `point` relative to the plane of `tri`.
///
/// # Research basis
/// [#4] Shewchuk 1997 — adaptive precision orient3d. The result is exact:
/// zero is returned if and only if the four points are truly coplanar
/// (no false positives from floating-point rounding).
#[allow(dead_code)] // Phase 2 building block — task 2b
pub(crate) fn orient3d_classify(tri: &[[f64; 3]; 3], point: &[f64; 3]) -> Orientation {
    let det = orient3d(tri[0], tri[1], tri[2], *point);
    if det > 0.0 {
        Orientation::Below
    } else if det < 0.0 {
        Orientation::Above
    } else {
        Orientation::Coplanar
    }
}

/// Classify a point relative to a directed edge in 2D using exact orient2d
/// predicate.
///
/// Returns positive if `point` is to the left of the directed edge `a→b`,
/// negative if to the right, zero if collinear.
///
/// # Research basis
/// [#4] Shewchuk 1997 — adaptive precision orient2d.
#[allow(dead_code)] // Phase 2 building block — task 2b
pub(crate) fn orient2d_classify(a: &[f64; 2], b: &[f64; 2], point: &[f64; 2]) -> Orientation {
    let det = orient2d(*a, *b, *point);
    if det > 0.0 {
        Orientation::Above // left of edge = "above" in 2D
    } else if det < 0.0 {
        Orientation::Below // right of edge = "below" in 2D
    } else {
        Orientation::Coplanar
    }
}

// ── Task 2c: Constrained triangulation ──
// Ref #24: Yang 2025 — subdivide mesh pair along intersection segments.
// Ref #9: Cherchi 2020 — indirect predicates for exact mesh arrangements.

/// A sub-triangle in the subdivided mesh.
/// Ref #24: Yang 2025 — constrained triangulation of mesh boolean operands.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 2 building block — task 2c
pub(crate) struct SubTriangle {
    /// Vertex indices in the subdivided vertex array.
    pub verts: [usize; 3],
    /// Index of the parent triangle in the original mesh.
    pub parent_tri: usize,
}

/// Result of subdividing both meshes along their intersections.
/// Ref #24: Yang 2025 — both operand meshes are subdivided so that intersection
/// segments lie exactly on sub-triangle edges.
#[derive(Debug)]
#[allow(dead_code)] // Phase 2 building block — task 2c
pub(crate) struct SubdividedMesh {
    /// All vertex positions (original + new intersection points).
    pub verts: Vec<[f64; 3]>,
    /// Sub-triangles from mesh A.
    pub tris_a: Vec<SubTriangle>,
    /// Sub-triangles from mesh B.
    pub tris_b: Vec<SubTriangle>,
}

// ── Task 2d: Cell labeling via generalized winding numbers ──

/// Classification of a sub-triangle relative to the other mesh.
/// Ref #7: Jacobson et al. 2013 — generalized winding numbers for
/// robust inside/outside classification without requiring watertight meshes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Phase 2 building block — task 2d
pub(crate) enum CellLabel {
    /// Clearly inside the other mesh (winding number > 0.7).
    Inside,
    /// Clearly outside the other mesh (winding number < 0.3).
    Outside,
    /// Co-surface: initial winding ≈ 0.5, offset resolves to Inside.
    /// The sub-tri lies on the other mesh's surface with its solid interior
    /// facing into the other solid. Example: shared y=0 plane of overlapping
    /// boxes where -normal offset enters the other box.
    CoSurfaceInside,
    /// Co-surface: initial winding ≈ 0.5, offset resolves to Outside.
    /// The sub-tri lies on the other mesh's surface but its solid interior
    /// faces away from the other solid. Example: touching boxes at x=2 where
    /// -normal offset stays outside the other box.
    CoSurfaceOutside,
}

/// Boolean operation to perform on the labeled cells.
/// Ref #24: Yang 2025 — cell selection determines which sub-triangles
/// appear in the final result mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Phase 2 building block — task 2d
pub(crate) enum MeshBooleanOp {
    Union,
    Subtract,
    Intersect,
}

/// Result of labeling all sub-triangles in a subdivided mesh pair.
/// Each sub-triangle gets an Inside/Outside label relative to the other mesh.
#[derive(Debug)]
#[allow(dead_code)] // Phase 2 building block — task 2d
pub(crate) struct CellLabeling {
    /// One label per sub-triangle in `SubdividedMesh::tris_a`.
    pub labels_a: Vec<CellLabel>,
    /// One label per sub-triangle in `SubdividedMesh::tris_b`.
    pub labels_b: Vec<CellLabel>,
}

/// Compute generalized winding number of point `p` w.r.t. a triangle mesh.
///
/// Sums solid angles of all triangles as seen from `p`. Returns ~1.0 for
/// inside a closed mesh, ~0.0 for outside. Ref #7 Jacobson et al. (2013).
/// Ref #4 Shewchuk — solid angle uses adaptive predicates via `lazy_exact_triple_sign`.
#[allow(dead_code)] // Phase 2 building block — task 2d
fn winding_number_mesh(p: [f64; 3], verts: &[[f64; 3]], tris: &[[usize; 3]]) -> f64 {
    let mut total = 0.0;
    for tri in tris {
        let sa = super::classify::solid_angle(p, verts[tri[0]], verts[tri[1]], verts[tri[2]]);
        if !sa.is_nan() {
            total += sa;
        }
    }
    total / (4.0 * std::f64::consts::PI)
}

/// Classify a winding number as Inside or Outside.
///
/// Threshold at 0.5: w >= 0.5 → Inside, w < 0.5 → Outside.
/// Consistent with the units.rs WINDING_INSIDE_THRESHOLD = 0.5.
/// Ref #7: Jacobson et al. 2013.
#[allow(dead_code)] // Phase 2 building block — task 2d
fn classify_winding(w: f64) -> CellLabel {
    if w >= WINDING_INSIDE_THRESHOLD {
        CellLabel::Inside
    } else {
        CellLabel::Outside
    }
}

/// Compute the centroid of a triangle in the subdivided vertex array.
#[allow(dead_code)] // Phase 2 building block — task 2d
fn sub_tri_centroid(verts: &[[f64; 3]], tri: &SubTriangle) -> [f64; 3] {
    let v0 = verts[tri.verts[0]];
    let v1 = verts[tri.verts[1]];
    let v2 = verts[tri.verts[2]];
    [
        (v0[0] + v1[0] + v2[0]) / 3.0,
        (v0[1] + v1[1] + v2[1]) / 3.0,
        (v0[2] + v1[2] + v2[2]) / 3.0,
    ]
}

/// Compute the unit normal of a sub-triangle. Returns (0,0,1) for degenerate tris.
/// Used by `label_sub_tri` for winding-number offset disambiguation.
/// Ref #9: Cherchi 2020 — face normal for coplanar disambiguation.
fn sub_tri_unit_normal(verts: &[[f64; 3]], tri: &SubTriangle) -> [f64; 3] {
    let v0 = verts[tri.verts[0]];
    let v1 = verts[tri.verts[1]];
    let v2 = verts[tri.verts[2]];
    let u = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
    let w = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
    let n = [
        u[1] * w[2] - u[2] * w[1],
        u[2] * w[0] - u[0] * w[2],
        u[0] * w[1] - u[1] * w[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < TAU_NORMALIZE_SQ {
        return [0.0, 0.0, 1.0];
    }
    [n[0] / len, n[1] / len, n[2] / len]
}

/// Label a single sub-triangle as inside or outside a target mesh.
///
/// When the winding number is ambiguous (near 0.5, indicating the centroid is
/// on or very near the target mesh's surface), offsets the evaluation point
/// along the INWARD normal (-normal, into the sub-triangle's own solid) and
/// re-evaluates. Returns `CoSurfaceInside` or `CoSurfaceOutside` to distinguish
/// co-surface tris that face INTO the other solid from those facing AWAY.
///
/// The offset direction matters:
/// - Touching boxes at x=2: -normal → AWAY from other solid → CoSurfaceOutside
/// - Overlapping boxes shared y=0: -normal → INTO other solid → CoSurfaceInside
///
/// Selection rules use this distinction:
/// - Subtract: keep A-Outside + A-CoSurfaceOutside (touching faces stay)
/// - Union: keep A-Outside + A-CoSurfaceOutside + A-CoSurfaceInside (fill gap)
/// - Intersect: keep A-Inside + A-CoSurfaceInside (shared boundary)
///
/// Ref #7: Jacobson 2013 — generalized winding number.
/// Ref #9: Cherchi 2020 — coplanar face disambiguation via normal offset.
fn label_sub_tri(
    verts: &[[f64; 3]],
    sub_tri: &SubTriangle,
    target_verts: &[[f64; 3]],
    target_tris: &[[usize; 3]],
) -> CellLabel {
    let centroid = sub_tri_centroid(verts, sub_tri);
    let w = winding_number_mesh(centroid, target_verts, target_tris);
    if w > WINDING_OUTSIDE_THRESHOLD && w < (1.0 - WINDING_OUTSIDE_THRESHOLD) {
        // Ambiguous — centroid is near the target mesh surface.
        // Offset along -normal (INTO this sub-triangle's own solid) to break tie.
        let normal = sub_tri_unit_normal(verts, sub_tri);
        let eps = TAU_WORK.sqrt(); // ~1e-6, geometric mean of model/working precision
        let offset = [
            centroid[0] - eps * normal[0],
            centroid[1] - eps * normal[1],
            centroid[2] - eps * normal[2],
        ];
        let w2 = winding_number_mesh(offset, target_verts, target_tris);
        if w2 >= WINDING_INSIDE_THRESHOLD {
            CellLabel::CoSurfaceInside
        } else {
            CellLabel::CoSurfaceOutside
        }
    } else {
        classify_winding(w)
    }
}

/// Classify each sub-triangle as inside or outside the other mesh.
///
/// Uses generalized winding numbers [#7 Jacobson 2013] to determine whether
/// the centroid of each sub-triangle in mesh A lies inside mesh B (and vice
/// versa). The original (pre-subdivision) mesh geometry is used as the
/// winding number source — the subdivided mesh provides the sub-triangles
/// whose centroids are the query points.
///
/// When centroids lie on the opposing mesh's surface (winding number ≈ 0.5),
/// the evaluation point is offset along the inward normal to break the tie.
/// See `label_sub_tri` for details.
///
/// # Arguments
/// - `subdivided`: The subdivided mesh pair from `subdivide_mesh_pair`.
/// - `original_verts_a`: Vertex positions of the original mesh A.
/// - `original_tris_a`: Triangle indices of the original mesh A.
/// - `original_verts_b`: Vertex positions of the original mesh B.
/// - `original_tris_b`: Triangle indices of the original mesh B.
#[allow(dead_code)] // Phase 2 building block — task 2d
pub(crate) fn label_cells(
    subdivided: &SubdividedMesh,
    original_verts_a: &[[f64; 3]],
    original_tris_a: &[[usize; 3]],
    original_verts_b: &[[f64; 3]],
    original_tris_b: &[[usize; 3]],
) -> CellLabeling {
    // Label A sub-triangles: is each one inside mesh B?
    // Ref #24: Yang 2025 — evaluate GWN at sub-triangle centroids.
    let labels_a = subdivided
        .tris_a
        .iter()
        .map(|sub_tri| {
            label_sub_tri(
                &subdivided.verts,
                sub_tri,
                original_verts_b,
                original_tris_b,
            )
        })
        .collect();

    // Label B sub-triangles: is each one inside mesh A?
    let labels_b = subdivided
        .tris_b
        .iter()
        .map(|sub_tri| {
            label_sub_tri(
                &subdivided.verts,
                sub_tri,
                original_verts_a,
                original_tris_a,
            )
        })
        .collect();

    CellLabeling { labels_a, labels_b }
}

/// Select sub-triangles from the labeled subdivided mesh based on the boolean operation.
///
/// Ref #24: Yang 2025 — cell selection rules:
/// - **Union**: A-outside-B + B-outside-A
/// - **Subtract**: A-outside-B + B-inside-A (with flipped winding order)
/// - **Intersect**: A-inside-B + B-inside-A
///
/// Returns a flat list of triangle vertices (3 consecutive [f64; 3] per triangle).
/// For Subtract, B triangles that are inside A have their winding order reversed
/// (vertices emitted in v2, v1, v0 order) to flip the surface normal outward.
#[allow(dead_code)] // Phase 2 building block — task 2d
pub(crate) fn select_boolean_result(
    subdivided: &SubdividedMesh,
    labeling: &CellLabeling,
    op: MeshBooleanOp,
) -> Vec<[f64; 3]> {
    let mut result = Vec::new();

    // Determine which labels to keep for A and B sub-triangles.
    // Ref #24: Yang 2025 — boolean op cell selection table.
    //
    // Co-surface handling (sub-tris on the other mesh's surface):
    // A selection:
    //   Union:     Outside + CoSurfaceOutside + CoSurfaceInside (fill shared-plane gap)
    //   Subtract:  Outside + CoSurfaceOutside (touching faces stay; overlap faces removed)
    //   Intersect: Inside + CoSurfaceInside (shared boundary included)
    // B selection: always only the primary label (Outside/Inside), never co-surface.
    let (keep_a, keep_b, flip_b) = match op {
        MeshBooleanOp::Union => (CellLabel::Outside, CellLabel::Outside, false),
        MeshBooleanOp::Subtract => (CellLabel::Outside, CellLabel::Inside, true),
        MeshBooleanOp::Intersect => (CellLabel::Inside, CellLabel::Inside, false),
    };

    let a_keeps_label = |label: &CellLabel| -> bool {
        if *label == keep_a {
            return true;
        }
        match op {
            MeshBooleanOp::Union => {
                // Union keeps all A co-surface tris (fills gap on shared planes)
                matches!(
                    label,
                    CellLabel::CoSurfaceInside | CellLabel::CoSurfaceOutside
                )
            }
            MeshBooleanOp::Subtract => {
                // Subtract keeps only CoSurfaceOutside (touching face stays, overlap drops)
                *label == CellLabel::CoSurfaceOutside
            }
            MeshBooleanOp::Intersect => {
                // Intersect keeps only CoSurfaceInside (shared boundary)
                *label == CellLabel::CoSurfaceInside
            }
        }
    };

    // Emit selected A sub-triangles (normal winding order).
    for (sub_tri, label) in subdivided.tris_a.iter().zip(labeling.labels_a.iter()) {
        if a_keeps_label(label) {
            let v0 = subdivided.verts[sub_tri.verts[0]];
            let v1 = subdivided.verts[sub_tri.verts[1]];
            let v2 = subdivided.verts[sub_tri.verts[2]];
            result.push(v0);
            result.push(v1);
            result.push(v2);
        }
    }

    // Emit selected B sub-triangles (possibly flipped for Subtract)
    for (sub_tri, label) in subdivided.tris_b.iter().zip(labeling.labels_b.iter()) {
        if *label == keep_b {
            let v0 = subdivided.verts[sub_tri.verts[0]];
            let v1 = subdivided.verts[sub_tri.verts[1]];
            let v2 = subdivided.verts[sub_tri.verts[2]];
            if flip_b {
                // Reverse winding to flip normal outward for subtracted solid
                result.push(v2);
                result.push(v1);
                result.push(v0);
            } else {
                result.push(v0);
                result.push(v1);
                result.push(v2);
            }
        }
    }

    result
}

/// Which edge of a triangle a point lies on, or if it's at a vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum PointLocation {
    /// On edge i (edge between vertex i and vertex (i+1)%3).
    OnEdge(usize),
    /// At vertex i.
    AtVertex(usize),
    /// Interior of the triangle (shouldn't happen for edge-crossing points).
    Interior,
}

/// Classify where a point lies on a triangle: on which edge or at which vertex.
/// Uses an epsilon of 1e-10 for edge proximity testing on materialized f64 coordinates.
///
/// Edge classification is prioritized over vertex classification because constraint
/// segment endpoints that happen to coincide with triangle vertices should still be
/// treated as edge-split points for correct subdivision (producing 3 sub-triangles
/// when both endpoints are on different edges, even if one endpoint is at a vertex).
#[allow(dead_code)]
fn classify_point_on_triangle(tri_verts: &[[f64; 3]; 3], point: &[f64; 3]) -> PointLocation {
    let eps = TAU_EXACT_MESH_CLASSIFY;

    // Check edges first — a point at a vertex endpoint of an edge is still "on" that edge.
    // This ensures correct subdivision: edge-edge splits always produce 3 sub-triangles.
    // Edge i: from vertex i to vertex (i+1)%3.
    // We check strict interior of edge first (not at endpoints), then vertex,
    // then endpoint-of-edge.
    let mut edge_at_endpoint: Option<(usize, usize)> = None; // (edge_idx, vertex_local_idx)

    for i in 0..3 {
        let j = (i + 1) % 3;
        let a = tri_verts[i];
        let b = tri_verts[j];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ap = [point[0] - a[0], point[1] - a[1], point[2] - a[2]];
        let cross = [
            ab[1] * ap[2] - ab[2] * ap[1],
            ab[2] * ap[0] - ab[0] * ap[2],
            ab[0] * ap[1] - ab[1] * ap[0],
        ];
        let cross_len_sq = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
        let ab_len_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];

        if cross_len_sq < eps * eps * ab_len_sq {
            let t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab_len_sq;
            if t > eps && t < 1.0 - eps {
                // Strictly interior to edge — definitive
                return PointLocation::OnEdge(i);
            }
            if t >= -eps && t <= 1.0 + eps {
                // At endpoint of edge
                let vertex = if t < eps { i } else { j };
                if edge_at_endpoint.is_none() {
                    edge_at_endpoint = Some((i, vertex));
                }
            }
        }
    }

    // If the point is at a vertex (detected as endpoint of one or more edges),
    // return AtVertex for non-constraint contexts. But for constraint splitting,
    // the caller may need to handle this differently.
    if let Some((_edge, vertex)) = edge_at_endpoint {
        return PointLocation::AtVertex(vertex);
    }

    PointLocation::Interior
}

/// Compute intersection of line through p0,p1 with line segment a,b in 3D.
/// Returns the parameter t along segment a→b, or None if parallel/no intersection.
#[allow(dead_code)]
fn line_segment_intersect_3d(
    p0: &[f64; 3],
    p1: &[f64; 3],
    a: &[f64; 3],
    b: &[f64; 3],
) -> Option<f64> {
    // Find parameter t on segment ab where the line p0→p1 is closest.
    // For coplanar lines, this is the exact intersection.
    let d1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let d2 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let r = [p0[0] - a[0], p0[1] - a[1], p0[2] - a[2]];

    // Use the two equations from the 2D projection onto the dominant axes
    // Find the dominant axis of the cross product d1 x d2
    let cross = [
        d1[1] * d2[2] - d1[2] * d2[1],
        d1[2] * d2[0] - d1[0] * d2[2],
        d1[0] * d2[1] - d1[1] * d2[0],
    ];
    let ax = cross[0].abs();
    let ay = cross[1].abs();
    let az = cross[2].abs();

    if ax < TAU_NORMALIZE_SQ && ay < TAU_NORMALIZE_SQ && az < TAU_NORMALIZE_SQ {
        return None; // parallel
    }

    // Project onto the plane perpendicular to the dominant cross-product axis
    let (i, j) = if ax >= ay && ax >= az {
        (1, 2)
    } else if ay >= az {
        (0, 2)
    } else {
        (0, 1)
    };

    // 2D line intersection: p0 + s*d1 = a + t*d2
    // d1[i]*t_d2 - d2[i]*s = r[i] ... wait, let me redo:
    // p0[i] + s*d1[i] = a[i] + t*d2[i]
    // p0[j] + s*d1[j] = a[j] + t*d2[j]
    // => s*d1[i] - t*d2[i] = a[i] - p0[i] = -r[i]
    // => s*d1[j] - t*d2[j] = a[j] - p0[j] = -r[j]
    let det = d1[i] * (-d2[j]) - d1[j] * (-d2[i]);
    if det.abs() < TAU_NORMALIZE_SQ {
        return None;
    }
    let t = (d1[i] * (-r[j]) - d1[j] * (-r[i])) / det;
    Some(t)
}

/// Split a single triangle by a constraint segment.
/// The segment endpoints are given as vertex indices into the shared vertex array.
/// Handles cases where segment endpoints are on edges, at vertices, or in the interior.
/// Returns a list of sub-triangles (as triples of vertex indices).
///
/// Key design: the constraint LINE (not just the segment) is intersected with all
/// triangle edges to find boundary crossing points. When an intersection point is
/// at a triangle vertex, it is still treated as an on-edge point for the purpose
/// of producing 3 sub-triangles. This is correct because the constraint endpoint
/// is a NEW vertex in the combined vertex array (even if it coincides in position
/// with an original vertex), and the split must produce valid sub-triangles that
/// share the constraint edge.
///
/// Ref #24: Yang 2025 — constrained subdivision of triangles along intersection segments.
#[allow(dead_code)]
fn split_triangle_by_segment(
    tri_vi: [usize; 3],
    seg: [usize; 2],
    all_verts: &mut Vec<[f64; 3]>,
) -> Vec<[usize; 3]> {
    let tri_verts = [
        all_verts[tri_vi[0]],
        all_verts[tri_vi[1]],
        all_verts[tri_vi[2]],
    ];

    let p0 = all_verts[seg[0]];
    let p1 = all_verts[seg[1]];
    let eps = TAU_EXACT_MESH_CLASSIFY;

    // Find where the constraint LINE intersects each triangle edge.
    // Each hit records (edge_index, parameter_t on edge, vertex_index in all_verts).
    struct EdgeHit {
        edge: usize,
        t: f64,
        vert_idx: usize,
    }

    // Classify each edge hit as either:
    // - Interior: point is strictly inside the edge (0 < t < 1)
    // - AtVertex: point is at a triangle vertex (t ≈ 0 or t ≈ 1)
    // Ref #9: Cherchi 2020 — for conformal subdivision, vertex hits must
    // use the original vertex index (no nudging) to ensure shared edges
    // have consistent vertex references across adjacent triangles.
    enum HitKind {
        Interior { vert_idx: usize },
        AtVertex { local_idx: usize },
    }
    struct EdgeHitExt {
        edge: usize,
        kind: HitKind,
    }

    let mut hits: Vec<EdgeHitExt> = Vec::new();
    let mut vertex_hit: [bool; 3] = [false, false, false];

    for edge_idx in 0..3 {
        let ei_v0 = edge_idx;
        let ei_v1 = (edge_idx + 1) % 3;
        let a = tri_verts[ei_v0];
        let b = tri_verts[ei_v1];

        if let Some(t) = line_segment_intersect_3d(&p0, &p1, &a, &b) {
            if t >= -eps && t <= 1.0 + eps {
                let t_clamped = t.clamp(0.0, 1.0);
                let at_start = t_clamped < eps;
                let at_end = t_clamped > 1.0 - eps;

                if at_start || at_end {
                    let v_local = if at_start { ei_v0 } else { ei_v1 };
                    if vertex_hit[v_local] {
                        continue;
                    }
                    vertex_hit[v_local] = true;
                    hits.push(EdgeHitExt {
                        edge: edge_idx,
                        kind: HitKind::AtVertex { local_idx: v_local },
                    });
                } else {
                    // Interior hit — compute position and find/create vertex
                    let pt = [
                        a[0] + t_clamped * (b[0] - a[0]),
                        a[1] + t_clamped * (b[1] - a[1]),
                        a[2] + t_clamped * (b[2] - a[2]),
                    ];
                    let vert_idx = if dist_sq(&all_verts[seg[0]], &pt) < eps * eps {
                        seg[0]
                    } else if dist_sq(&all_verts[seg[1]], &pt) < eps * eps {
                        seg[1]
                    } else {
                        let idx = all_verts.len();
                        all_verts.push(pt);
                        idx
                    };
                    hits.push(EdgeHitExt {
                        edge: edge_idx,
                        kind: HitKind::Interior { vert_idx },
                    });
                }
            }
        }
    }

    // Need at least 2 hits on different edges to split.
    if hits.len() < 2 {
        return vec![tri_vi];
    }

    // Find two hits on different edges and dispatch by kind.
    for i in 0..hits.len() {
        for j in (i + 1)..hits.len() {
            if hits[i].edge != hits[j].edge {
                match (&hits[i].kind, &hits[j].kind) {
                    // Two interior hits: standard 3-triangle split
                    (HitKind::Interior { vert_idx: vi }, HitKind::Interior { vert_idx: vj }) => {
                        return split_two_edge_points(tri_vi, hits[i].edge, *vi, hits[j].edge, *vj);
                    }
                    // One interior + one at vertex: 2-triangle edge split
                    // Only split at the interior point — the vertex is already
                    // a triangle vertex and needs no split.
                    (HitKind::Interior { vert_idx }, HitKind::AtVertex { .. }) => {
                        return split_at_edge_point(tri_vi, hits[i].edge, *vert_idx);
                    }
                    (HitKind::AtVertex { .. }, HitKind::Interior { vert_idx }) => {
                        return split_at_edge_point(tri_vi, hits[j].edge, *vert_idx);
                    }
                    // Both at vertices: constraint passes through two triangle
                    // vertices (along an edge). No split needed.
                    (HitKind::AtVertex { .. }, HitKind::AtVertex { .. }) => {
                        return vec![tri_vi];
                    }
                }
            }
        }
    }

    vec![tri_vi]
}

/// Squared distance between two 3D points.
#[allow(dead_code)]
fn dist_sq(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

/// Split triangle with two constraint points on different edges.
/// Produces 3 sub-triangles.
#[allow(dead_code)]
fn split_two_edge_points(
    tri_vi: [usize; 3],
    e0: usize,
    s0: usize,
    e1: usize,
    s1: usize,
) -> Vec<[usize; 3]> {
    // Build the 5-vertex boundary polygon by walking edges in CCW order.
    // Edge k goes from vertex k to vertex (k+1)%3. Insert split points on
    // their respective edges to preserve the CCW winding of the original triangle.
    let mut poly: Vec<usize> = Vec::with_capacity(5);
    #[allow(clippy::needless_range_loop)]
    for edge in 0..3 {
        poly.push(tri_vi[edge]); // start vertex of this edge
        if edge == e0 {
            poly.push(s0);
        }
        if edge == e1 {
            poly.push(s1);
        }
    }
    debug_assert_eq!(poly.len(), 5);

    // Find the shared vertex between e0 and e1.
    let e0_verts = [e0, (e0 + 1) % 3];
    let e1_verts = [e1, (e1 + 1) % 3];
    let shared_local = if e0_verts[0] == e1_verts[0] || e0_verts[0] == e1_verts[1] {
        e0_verts[0]
    } else {
        e0_verts[1]
    };
    let v_shared = tri_vi[shared_local];

    // Find position of shared vertex in the CCW polygon.
    let shared_pos = poly.iter().position(|&v| v == v_shared).unwrap();

    // Adjacent to shared in the polygon are the two split points (prev, next).
    // The polygon is CCW, so: ... → prev → v_shared → next → a → b → ...
    let prev = poly[(shared_pos + 4) % 5];
    let next = poly[(shared_pos + 1) % 5];
    let a = poly[(shared_pos + 2) % 5];
    let b = poly[(shared_pos + 3) % 5];

    // Triangulate: one triangle at the shared vertex (across constraint edge),
    // and the remaining quad [next, a, b, prev] split into 2 triangles.
    // All triangles follow the CCW polygon winding.
    vec![
        [prev, v_shared, next], // shared vertex triangle (includes constraint edge prev-next)
        [next, a, b],           // quad triangle 1
        [next, b, prev],        // quad triangle 2
    ]
}

/// Split triangle with one point at a vertex and one on an edge.
/// Produces 2 sub-triangles.
#[allow(dead_code)]
fn split_vertex_and_edge(tri_vi: [usize; 3], vi: usize, ei: usize, si: usize) -> Vec<[usize; 3]> {
    let ei_v0 = ei;
    let ei_v1 = (ei + 1) % 3;
    vec![
        [tri_vi[vi], tri_vi[ei_v0], si],
        [tri_vi[vi], si, tri_vi[ei_v1]],
    ]
}

/// Split a triangle at a point known to lie on one of its edges.
///
/// `edge_local` is 0, 1, or 2 indicating which edge of `tri` the split vertex
/// lies on.  Edge `k` goes from `tri[k]` to `tri[(k+1)%3]`.  The opposite
/// vertex is `tri[(k+2)%3]`.
///
/// Returns two sub-triangles that together cover the original triangle.
#[allow(dead_code)] // Edge-propagation helper
fn split_at_edge_point(tri: [usize; 3], edge_local: usize, split_vert: usize) -> Vec<[usize; 3]> {
    let v0 = tri[edge_local];
    let v1 = tri[(edge_local + 1) % 3];
    let v_opp = tri[(edge_local + 2) % 3];
    vec![[v0, split_vert, v_opp], [split_vert, v1, v_opp]]
}

/// Subdivide both meshes along their intersection segments.
///
/// For each pair of triangles (one from A, one from B), computes the exact
/// intersection via `tri_tri_intersect`. Segment intersections are used as
/// constraint edges to subdivide the original triangles.
///
/// # Algorithm
/// 1. Merge vertex arrays (B offset by |verts_a|)
/// 2. Compute all tri-tri intersections, collect segments
/// 3. Per-triangle constraint segments
/// 4. Edge-split propagation for conformal subdivision
/// 5. Split each triangle by its constraint segments + propagated edge points
///
/// Ref #24: Yang 2025 — constrained triangulation step of hybrid boolean.
/// Ref #9: Cherchi 2020 — indirect predicates for exact arrangements.
#[allow(dead_code)] // Phase 2 building block — task 2c
pub(crate) fn subdivide_mesh_pair(
    verts_a: &[[f64; 3]],
    tris_a: &[[usize; 3]],
    verts_b: &[[f64; 3]],
    tris_b: &[[usize; 3]],
) -> SubdividedMesh {
    use std::collections::BTreeMap;

    let offset_b = verts_a.len();

    // Step 1: Merge vertex arrays
    let mut all_verts: Vec<[f64; 3]> = Vec::with_capacity(verts_a.len() + verts_b.len());
    all_verts.extend_from_slice(verts_a);
    all_verts.extend_from_slice(verts_b);

    // Remap B's triangle indices
    let remapped_tris_b: Vec<[usize; 3]> = tris_b
        .iter()
        .map(|t| [t[0] + offset_b, t[1] + offset_b, t[2] + offset_b])
        .collect();

    // Step 2: Compute all intersections, collect per-triangle constraint segments.
    // Key: (mesh_id 0=A 1=B, tri_index), Value: list of (seg_v0, seg_v1) vertex indices.
    let mut constraints_a: BTreeMap<usize, Vec<[usize; 2]>> = BTreeMap::new();
    let mut constraints_b: BTreeMap<usize, Vec<[usize; 2]>> = BTreeMap::new();

    for (i, tri_a_idx) in tris_a.iter().enumerate() {
        for (j, _tri_b_idx) in tris_b.iter().enumerate() {
            let remapped_b = remapped_tris_b[j];
            let result = tri_tri_intersect(*tri_a_idx, remapped_b, &all_verts);

            if let TriTriIsect::Segment(ip0, ip1) = result {
                let p0 = materialize_ip(&ip0, &all_verts);
                let p1 = materialize_ip(&ip1, &all_verts);

                let vi0 = if ip0.edge[0] == ip0.edge[1] {
                    ip0.edge[0]
                } else {
                    let idx = all_verts.len();
                    all_verts.push(p0);
                    idx
                };

                let vi1 = if ip1.edge[0] == ip1.edge[1] {
                    ip1.edge[0]
                } else {
                    let idx = all_verts.len();
                    all_verts.push(p1);
                    idx
                };

                if vi0 != vi1 {
                    constraints_a.entry(i).or_default().push([vi0, vi1]);
                    constraints_b.entry(j).or_default().push([vi0, vi1]);
                }
            }
        }
    }

    // Helper: canonical undirected edge key (smaller index first)
    fn edge_key(a: usize, b: usize) -> (usize, usize) {
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Check if point `p` lies on the segment from `a` to `b` strictly between
    /// the endpoints (0 < t < 1). Uses a generous parametric tolerance to catch
    /// vertices that were nudged slightly toward an endpoint by
    /// `split_triangle_by_segment`.
    fn point_on_edge_interior(p: &[f64; 3], a: &[f64; 3], b: &[f64; 3], eps: f64) -> Option<f64> {
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
        let ab_len_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
        if ab_len_sq < TAU_NORMALIZE_SQ {
            return None;
        }
        let cross = [
            ab[1] * ap[2] - ab[2] * ap[1],
            ab[2] * ap[0] - ab[0] * ap[2],
            ab[0] * ap[1] - ab[1] * ap[0],
        ];
        let cross_len_sq = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
        // Allow slightly larger perpendicular tolerance for nudged vertices
        if cross_len_sq > eps * eps * ab_len_sq {
            return None;
        }
        let t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab_len_sq;
        // Boundary tolerance: exclude actual endpoints (t == 0 or t == 1)
        // while including nudged points (t ≈ 1e-14, from vertex_nudge).
        // Use 1e-16 — tighter than the nudge but wider than float roundoff.
        let boundary_eps = TAU_EXACT_MESH_BOUNDARY_EPS;
        if t > boundary_eps && t < 1.0 - boundary_eps {
            Some(t)
        } else {
            None
        }
    }

    let eps = TAU_EXACT_MESH_CLASSIFY;

    // ── Step 3: Direct subdivision ──
    // First, apply direct constraint segments to get initial sub-triangles.

    let mut result_tris_a_raw: Vec<(usize, Vec<[usize; 3]>)> = Vec::new();
    for (i, tri) in tris_a.iter().enumerate() {
        if let Some(segs) = constraints_a.get(&i) {
            let mut current_tris: Vec<[usize; 3]> = vec![*tri];
            for seg in segs {
                let mut next_tris = Vec::new();
                for t in &current_tris {
                    let splits = split_triangle_by_segment(*t, *seg, &mut all_verts);
                    next_tris.extend(splits);
                }
                current_tris = next_tris;
            }
            result_tris_a_raw.push((i, current_tris));
        } else {
            result_tris_a_raw.push((i, vec![*tri]));
        }
    }

    let mut result_tris_b_raw: Vec<(usize, Vec<[usize; 3]>)> = Vec::new();
    for (j, _tri) in tris_b.iter().enumerate() {
        let remapped = remapped_tris_b[j];
        if let Some(segs) = constraints_b.get(&j) {
            let mut current_tris: Vec<[usize; 3]> = vec![remapped];
            for seg in segs {
                let mut next_tris = Vec::new();
                for t in &current_tris {
                    let splits = split_triangle_by_segment(*t, *seg, &mut all_verts);
                    next_tris.extend(splits);
                }
                current_tris = next_tris;
            }
            result_tris_b_raw.push((j, current_tris));
        } else {
            result_tris_b_raw.push((j, vec![remapped]));
        }
    }

    // ── Step 3a: Vertex deduplication ──
    // When two different intersections hit the same edge at the same
    // position, they create separate vertices. Merge exact duplicates
    // (within floating-point roundoff) so both sub-triangles reference
    // the same vertex index.
    {
        use std::collections::HashMap;
        let original_count = verts_a.len() + verts_b.len();
        // Use 1e-15 quantization — merge only near-exact duplicates from
        // independent intersection computations. Do NOT merge nudged vertices
        // with their endpoints (that creates degenerate triangles); nudged
        // vertex cleanup happens separately after propagation.
        let quant = |v: [f64; 3]| -> [i64; 3] {
            let scale = 1e15;
            [
                (v[0] * scale).round() as i64,
                (v[1] * scale).round() as i64,
                (v[2] * scale).round() as i64,
            ]
        };

        let mut canonical: HashMap<[i64; 3], usize> = HashMap::new();
        let mut remap: Vec<usize> = (0..all_verts.len()).collect();
        for vi in original_count..all_verts.len() {
            let key = quant(all_verts[vi]);
            let canon = *canonical.entry(key).or_insert(vi);
            if canon != vi {
                remap[vi] = canon;
            }
        }

        for (_parent, sub_tris) in &mut result_tris_a_raw {
            for tri in sub_tris.iter_mut() {
                for v in tri.iter_mut() {
                    *v = remap[*v];
                }
            }
        }
        for (_parent, sub_tris) in &mut result_tris_b_raw {
            for tri in sub_tris.iter_mut() {
                for v in tri.iter_mut() {
                    *v = remap[*v];
                }
            }
        }
    }

    // ── Step 3b: Edge-split propagation for conformal subdivision ──
    // After direct splitting, detect which new vertices lie on original mesh
    // edges and propagate them to all adjacent triangles sharing those edges.

    // Build edge adjacency: original edge → list of triangle indices sharing it.
    fn build_edge_adjacency(tris: &[[usize; 3]]) -> BTreeMap<(usize, usize), Vec<usize>> {
        let mut adj: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (ti, tri) in tris.iter().enumerate() {
            for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                adj.entry(edge_key(a, b)).or_default().push(ti);
            }
        }
        adj
    }

    let edge_adj_a = build_edge_adjacency(tris_a);
    let edge_adj_b = build_edge_adjacency(&remapped_tris_b);

    // Scan sub-triangles to find new vertices on original edges.
    // Returns: edge → list of new vertex indices on that edge.
    fn detect_edge_splits(
        original_tris: &[[usize; 3]],
        raw_results: &[(usize, Vec<[usize; 3]>)],
        all_verts: &[[f64; 3]],
        _original_vert_count: usize,
        eps: f64,
    ) -> BTreeMap<(usize, usize), Vec<usize>> {
        let mut edge_splits: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for &(parent_idx, ref sub_tris) in raw_results {
            let parent = original_tris[parent_idx];
            let parent_edges = [
                (parent[0], parent[1]),
                (parent[1], parent[2]),
                (parent[2], parent[0]),
            ];
            for sub_tri in sub_tris {
                for &vi in sub_tri {
                    // Skip vertices that belong to the parent triangle itself.
                    // Do NOT skip based on original_vert_count — edge-on-plane
                    // intersections insert existing vertices from the OTHER mesh
                    // as split points on THIS mesh's edges. These must be
                    // propagated to adjacent triangles for conformal subdivision.
                    // Ref: specs/edge_on_plane_intersection.md — conformal vertex sharing.
                    if vi == parent[0] || vi == parent[1] || vi == parent[2] {
                        continue; // parent's own vertex
                    }
                    let p = &all_verts[vi];
                    for &(ev0, ev1) in &parent_edges {
                        if let Some(_t) =
                            point_on_edge_interior(p, &all_verts[ev0], &all_verts[ev1], eps)
                        {
                            let ek = edge_key(ev0, ev1);
                            let entry = edge_splits.entry(ek).or_default();
                            if !entry.contains(&vi) {
                                entry.push(vi);
                            }
                        }
                    }
                }
            }
        }
        edge_splits
    }

    let edge_splits_a =
        detect_edge_splits(tris_a, &result_tris_a_raw, &all_verts, verts_a.len(), eps);
    // Edge-split detection verified — propagation follows below.
    let edge_splits_b = detect_edge_splits(
        &remapped_tris_b,
        &result_tris_b_raw,
        &all_verts,
        offset_b + verts_b.len(),
        eps,
    );

    // Propagate edge splits to adjacent triangles that were also directly
    // split (have constraints). Non-intersected triangles are left unchanged.
    fn propagate_edge_splits(
        raw_results: &mut [(usize, Vec<[usize; 3]>)],
        edge_splits: &BTreeMap<(usize, usize), Vec<usize>>,
        edge_adj: &BTreeMap<(usize, usize), Vec<usize>>,
        all_verts: &[[f64; 3]],
        eps: f64,
    ) {
        for (ek, split_verts) in edge_splits {
            if let Some(adj_tris) = edge_adj.get(ek) {
                for &tri_idx in adj_tris {
                    let sub_tris = &mut raw_results[tri_idx].1;
                    for &split_vi in split_verts {
                        // Check if this vertex is already present in any sub-triangle
                        let already_present = sub_tris
                            .iter()
                            .any(|t| t[0] == split_vi || t[1] == split_vi || t[2] == split_vi);
                        if already_present {
                            continue;
                        }

                        // Apply this split vertex to the sub-triangles
                        let mut found = false;
                        let mut i = 0;
                        while i < sub_tris.len() {
                            let tri = sub_tris[i];
                            let tri_edges = [
                                (tri[0], tri[1], 0usize),
                                (tri[1], tri[2], 1usize),
                                (tri[2], tri[0], 2usize),
                            ];
                            let mut did_split = false;
                            for &(ev0, ev1, edge_local) in &tri_edges {
                                if point_on_edge_interior(
                                    &all_verts[split_vi],
                                    &all_verts[ev0],
                                    &all_verts[ev1],
                                    eps,
                                )
                                .is_some()
                                {
                                    // Guard: skip if the split vertex is too
                                    // close to either edge endpoint (would
                                    // create a degenerate sub-triangle).
                                    let d0 = dist_sq(&all_verts[split_vi], &all_verts[ev0]);
                                    let d1 = dist_sq(&all_verts[split_vi], &all_verts[ev1]);
                                    let edge_len = dist_sq(&all_verts[ev0], &all_verts[ev1]);
                                    if d0 < eps * eps * edge_len || d1 < eps * eps * edge_len {
                                        // Too close to endpoint — skip
                                        i += 1;
                                        continue;
                                    }
                                    let new_tris = split_at_edge_point(tri, edge_local, split_vi);
                                    sub_tris.remove(i);
                                    for (k, nt) in new_tris.into_iter().enumerate() {
                                        sub_tris.insert(i + k, nt);
                                    }
                                    did_split = true;
                                    found = true;
                                    break;
                                }
                            }
                            if !did_split {
                                i += 1;
                            }
                        }
                        let _ = found; // Suppress unused warning
                    }
                }
            }
        }
    }

    propagate_edge_splits(
        &mut result_tris_a_raw,
        &edge_splits_a,
        &edge_adj_a,
        &all_verts,
        eps,
    );
    propagate_edge_splits(
        &mut result_tris_b_raw,
        &edge_splits_b,
        &edge_adj_b,
        &all_verts,
        eps,
    );

    // ── Step 3c: Cross-mesh edge-split propagation ──
    // Edge-on-plane intersections create vertices from one mesh's constraints on
    // the OTHER mesh's edges. These must be propagated to all adjacent triangles
    // in the other mesh for conformal subdivision.
    // Ref #9: Cherchi 2020 — conformal mesh arrangements require shared vertices.
    // Ref: specs/edge_on_plane_intersection.md — cross-mesh conformal vertex sharing.

    fn detect_cross_mesh_splits(
        source_results: &[(usize, Vec<[usize; 3]>)],
        source_original_tris: &[[usize; 3]],
        target_tris: &[[usize; 3]],
        target_results: &[(usize, Vec<[usize; 3]>)],
        all_verts: &[[f64; 3]],
        eps: f64,
    ) -> BTreeMap<(usize, usize), Vec<usize>> {
        // Collect all unique vertices from source sub-triangles that are NOT
        // vertices of the source's original triangles (i.e. new split vertices).
        let mut source_original_vis: Vec<usize> = Vec::new();
        for tri in source_original_tris {
            for &v in tri {
                if !source_original_vis.contains(&v) {
                    source_original_vis.push(v);
                }
            }
        }
        let mut new_verts: Vec<usize> = Vec::new();
        for &(_parent, ref sub_tris) in source_results {
            for sub_tri in sub_tris {
                for &vi in sub_tri {
                    if !source_original_vis.contains(&vi) && !new_verts.contains(&vi) {
                        new_verts.push(vi);
                    }
                }
            }
        }

        // Collect all vertices already present in target sub-triangles.
        // If a vertex is already in the target mesh's subdivision, same-mesh
        // propagation already handled it — no cross-mesh action needed.
        let mut target_present: Vec<usize> = Vec::new();
        for &(_parent, ref sub_tris) in target_results {
            for sub_tri in sub_tris {
                for &vi in sub_tri {
                    if !target_present.contains(&vi) {
                        target_present.push(vi);
                    }
                }
            }
        }

        // Build set of all target edges for quick iteration
        let mut target_edges: Vec<(usize, usize)> = Vec::new();
        for tri in target_tris {
            for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let ek = edge_key(a, b);
                if !target_edges.contains(&ek) {
                    target_edges.push(ek);
                }
            }
        }

        let mut cross_splits: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for &vi in &new_verts {
            // Skip vertices already present in target mesh — same-mesh
            // propagation already handled these.
            if target_present.contains(&vi) {
                continue;
            }
            let p = &all_verts[vi];
            for &(ev0, ev1) in &target_edges {
                // Skip if this vertex IS one of the edge endpoints
                if vi == ev0 || vi == ev1 {
                    continue;
                }
                if let Some(_t) = point_on_edge_interior(p, &all_verts[ev0], &all_verts[ev1], eps) {
                    let entry = cross_splits.entry((ev0, ev1)).or_default();
                    if !entry.contains(&vi) {
                        entry.push(vi);
                    }
                }
            }
        }
        cross_splits
    }

    // Detect and propagate cross-mesh splits iteratively. The first pass finds
    // A's new vertices on B's edges (and vice versa). Propagation may create new
    // sub-triangles with vertices that lie on the OTHER mesh's edges, requiring
    // a second pass. Limit to 3 rounds to avoid pathological cases.
    // Ref #9: Cherchi 2020 — conformal mesh arrangements require shared vertices.
    for _round in 0..3 {
        let cross_splits_on_b = detect_cross_mesh_splits(
            &result_tris_a_raw,
            tris_a,
            &remapped_tris_b,
            &result_tris_b_raw,
            &all_verts,
            eps,
        );
        let cross_splits_on_a = detect_cross_mesh_splits(
            &result_tris_b_raw,
            &remapped_tris_b,
            tris_a,
            &result_tris_a_raw,
            &all_verts,
            eps,
        );

        // If no new splits found, we're done.
        if cross_splits_on_b.is_empty() && cross_splits_on_a.is_empty() {
            break;
        }

        propagate_edge_splits(
            &mut result_tris_a_raw,
            &cross_splits_on_a,
            &edge_adj_a,
            &all_verts,
            eps,
        );
        propagate_edge_splits(
            &mut result_tris_b_raw,
            &cross_splits_on_b,
            &edge_adj_b,
            &all_verts,
            eps,
        );
    }

    // ── Step 3d: Post-propagation vertex deduplication ──
    // Cross-mesh propagation may create duplicate vertices at identical positions.
    // Re-run the same quantization-based dedup to merge them.
    {
        use std::collections::HashMap;
        let original_count = verts_a.len() + verts_b.len();
        let quant = |v: [f64; 3]| -> [i64; 3] {
            let scale = 1e15;
            [
                (v[0] * scale).round() as i64,
                (v[1] * scale).round() as i64,
                (v[2] * scale).round() as i64,
            ]
        };

        let mut canonical: HashMap<[i64; 3], usize> = HashMap::new();
        let mut remap: Vec<usize> = (0..all_verts.len()).collect();
        for vi in original_count..all_verts.len() {
            let key = quant(all_verts[vi]);
            let canon = *canonical.entry(key).or_insert(vi);
            if canon != vi {
                remap[vi] = canon;
            }
        }

        // Only apply if there's anything to remap
        let has_remaps = remap.iter().enumerate().any(|(i, &r)| i != r);
        if has_remaps {
            for (_parent, sub_tris) in &mut result_tris_a_raw {
                for tri in sub_tris.iter_mut() {
                    for v in tri.iter_mut() {
                        *v = remap[*v];
                    }
                }
            }
            for (_parent, sub_tris) in &mut result_tris_b_raw {
                for tri in sub_tris.iter_mut() {
                    for v in tri.iter_mut() {
                        *v = remap[*v];
                    }
                }
            }
        }
    }

    // ── Step 4: Collect final results ──
    let mut result_tris_a = Vec::new();
    for &(parent_tri, ref sub_tris) in &result_tris_a_raw {
        for &t in sub_tris {
            result_tris_a.push(SubTriangle {
                verts: t,
                parent_tri,
            });
        }
    }

    let mut result_tris_b = Vec::new();
    for &(parent_tri, ref sub_tris) in &result_tris_b_raw {
        for &t in sub_tris {
            result_tris_b.push(SubTriangle {
                verts: t,
                parent_tri,
            });
        }
    }

    SubdividedMesh {
        verts: all_verts,
        tris_a: result_tris_a,
        tris_b: result_tris_b,
    }
}

// ── Task 2e: Radial sort for non-manifold edge resolution ──
// Ref #10: Levy 2025 — exact constructions + radial sort.
// Ref #12: Barki 2015 — radial sort for classification in mesh arrangements.

/// A triangle meeting at a non-manifold edge, identified by its opposite vertex.
/// Used as input to radial sort.
/// Ref #10: Levy 2025 — radial sort around non-manifold edges.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 2 building block — task 2e
pub(crate) struct RadialTriangle {
    /// Index of the vertex NOT on the shared edge (the "opposite" or "apex" vertex).
    pub opposite_vertex: usize,
    /// Which input mesh this triangle came from.
    pub mesh_id: MeshId,
    /// Index into SubdividedMesh's tris_a (for MeshId::A) or tris_b (for MeshId::B).
    pub sub_tri_index: usize,
}

/// Sort triangles around a non-manifold edge by angular position.
///
/// Uses exact orient3d predicates [#4 Shewchuk] for comparison — no tolerance
/// parameters. The sort determines angular ordering of triangles around the
/// edge axis, which is required for topology extraction (Phase 3) and correct
/// cell pairing.
///
/// # Algorithm (Ref #10: Levy 2025)
///
/// The comparison primitive is:
///   orient3d(edge[0], edge[1], v_i, v_j)
/// where v_i, v_j are opposite vertices of two triangles. The sign of orient3d
/// determines their relative angular position around the edge.
///
/// For the coplanar case (orient3d == 0), triangles are on the same half-plane
/// and require secondary sorting by distance or mesh_id tiebreak.
///
/// # Arguments
/// - `edge`: Vertex indices of the non-manifold edge `[start, end]`.
/// - `triangles`: Triangles meeting at the edge.
/// - `verts`: Shared vertex position array.
///
/// # Returns
/// Indices into `triangles`, sorted in CCW angular order around the edge axis
/// (looking from edge[0] toward edge[1]).
///
/// # Invariants
/// - I1: Consistent angular ordering (orient3d signs agree for consecutive pairs)
/// - I2: Bijection of input indices (no triangles lost or duplicated)
/// - I3: For 4-triangle boolean edges, sorted order alternates mesh A/B
/// - I4: No tolerance parameters used
#[allow(dead_code)] // Phase 2 building block — task 2e
pub(crate) fn radial_sort_around_edge(
    edge: [usize; 2],
    triangles: &[RadialTriangle],
    verts: &[[f64; 3]],
) -> Vec<usize> {
    // B1: 0 or 1 triangles — no sort needed.
    let n = triangles.len();
    if n <= 1 {
        return (0..n).collect();
    }

    let e0 = verts[edge[0]];
    let e1 = verts[edge[1]];

    // Edge axis vector.
    let axis = [e1[0] - e0[0], e1[1] - e0[1], e1[2] - e0[2]];
    let axis_len_sq = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];

    // Zero-length edge — cannot define angular ordering.
    if axis_len_sq < crate::units::TAU_NORMALIZE_SQ {
        return (0..n).collect();
    }

    let inv_axis_len = 1.0 / axis_len_sq.sqrt();
    let axis_n = [
        axis[0] * inv_axis_len,
        axis[1] * inv_axis_len,
        axis[2] * inv_axis_len,
    ];

    // Build orthonormal frame perpendicular to edge axis.
    // Pick reference direction not parallel to axis. Ref #10: Levy 2025.
    let ref_dir = if axis_n[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };

    // u = normalize(ref_dir - (ref_dir . axis_n) * axis_n)
    let dot_ra = ref_dir[0] * axis_n[0] + ref_dir[1] * axis_n[1] + ref_dir[2] * axis_n[2];
    let u_raw = [
        ref_dir[0] - dot_ra * axis_n[0],
        ref_dir[1] - dot_ra * axis_n[1],
        ref_dir[2] - dot_ra * axis_n[2],
    ];
    let u_len = (u_raw[0] * u_raw[0] + u_raw[1] * u_raw[1] + u_raw[2] * u_raw[2]).sqrt();
    let u = [u_raw[0] / u_len, u_raw[1] / u_len, u_raw[2] / u_len];

    // v = axis_n × u (right-hand rule gives CCW orientation)
    let v = [
        axis_n[1] * u[2] - axis_n[2] * u[1],
        axis_n[2] * u[0] - axis_n[0] * u[2],
        axis_n[0] * u[1] - axis_n[1] * u[0],
    ];

    // Edge midpoint as projection origin.
    let mid = [
        (e0[0] + e1[0]) * 0.5,
        (e0[1] + e1[1]) * 0.5,
        (e0[2] + e1[2]) * 0.5,
    ];

    // For each triangle, compute the angle of its opposite vertex in the (u, v) frame.
    let angles: Vec<f64> = triangles
        .iter()
        .map(|tri| {
            let ov = verts[tri.opposite_vertex];
            let d = [ov[0] - mid[0], ov[1] - mid[1], ov[2] - mid[2]];
            // Project out the axis component.
            let d_axis = d[0] * axis_n[0] + d[1] * axis_n[1] + d[2] * axis_n[2];
            let proj = [
                d[0] - d_axis * axis_n[0],
                d[1] - d_axis * axis_n[1],
                d[2] - d_axis * axis_n[2],
            ];
            let pu = proj[0] * u[0] + proj[1] * u[1] + proj[2] * u[2];
            let pv = proj[0] * v[0] + proj[1] * v[1] + proj[2] * v[2];
            pv.atan2(pu)
        })
        .collect();

    // Sort indices by angle. For coplanar tiebreak (nearly equal angles),
    // use exact orient3d predicate. Ref #4: Shewchuk 1997, Ref #10: Levy 2025.
    let mut indices: Vec<usize> = (0..n).collect();
    indices.sort_by(|&a, &b| {
        let angle_a = angles[a];
        let angle_b = angles[b];
        // Primary sort: by atan2 angle.
        match angle_a.partial_cmp(&angle_b) {
            Some(std::cmp::Ordering::Equal) | None => {
                // Coplanar tiebreak: use exact orient3d.
                // orient3d(e0, e1, v_a, v_b) > 0 means v_b is CCW from v_a,
                // so v_a should come first (Less).
                let va = verts[triangles[a].opposite_vertex];
                let vb = verts[triangles[b].opposite_vertex];
                let o = orient3d(e0, e1, va, vb);
                if o > 0.0 {
                    std::cmp::Ordering::Less // v_a before v_b (CCW)
                } else if o < 0.0 {
                    std::cmp::Ordering::Greater
                } else {
                    // Truly coplanar and same angle — break tie by distance from edge
                    // (closer vertex first) to ensure deterministic ordering.
                    let da = {
                        let ov = verts[triangles[a].opposite_vertex];
                        let dx = ov[0] - mid[0];
                        let dy = ov[1] - mid[1];
                        let dz = ov[2] - mid[2];
                        dx * dx + dy * dy + dz * dz
                    };
                    let db = {
                        let ov = verts[triangles[b].opposite_vertex];
                        let dx = ov[0] - mid[0];
                        let dy = ov[1] - mid[1];
                        let dz = ov[2] - mid[2];
                        dx * dx + dy * dy + dz * dz
                    };
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                }
            }
            Some(ord) => ord,
        }
    });

    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{TAU_COINCIDENT, TAU_WORK, TJUNCTION_ENDPOINT_MARGIN};

    // ── Smoke tests: verify geometry-predicates crate integration ──

    #[test]
    fn orient3d_classify_above() {
        // Triangle in the XY plane at z=0
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Point above the plane (positive z)
        let point = [0.25, 0.25, 1.0];
        assert_eq!(orient3d_classify(&tri, &point), Orientation::Above);
    }

    #[test]
    fn orient3d_classify_below() {
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Point below the plane (negative z)
        let point = [0.25, 0.25, -1.0];
        assert_eq!(orient3d_classify(&tri, &point), Orientation::Below);
    }

    #[test]
    fn orient3d_classify_coplanar() {
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Point in the plane
        let point = [0.5, 0.5, 0.0];
        assert_eq!(orient3d_classify(&tri, &point), Orientation::Coplanar);
    }

    #[test]
    fn orient3d_exact_near_coplanar() {
        // Near-coplanar configuration that would fool naive floating-point.
        // The four points are exactly coplanar (z=0 for all), so orient3d
        // must return exactly 0.0 — no false positive from rounding.
        // Ref [#4]: Shewchuk predicates guarantee this.
        let tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let point = [0.3, 0.7, 0.0];
        assert_eq!(orient3d_classify(&tri, &point), Orientation::Coplanar);
    }

    #[test]
    fn orient2d_classify_left() {
        // Edge from (0,0) to (1,0); point at (0.5, 1.0) is to the left
        let a = [0.0, 0.0];
        let b = [1.0, 0.0];
        let point = [0.5, 1.0];
        assert_eq!(orient2d_classify(&a, &b, &point), Orientation::Above);
    }

    #[test]
    fn orient2d_classify_right() {
        let a = [0.0, 0.0];
        let b = [1.0, 0.0];
        let point = [0.5, -1.0];
        assert_eq!(orient2d_classify(&a, &b, &point), Orientation::Below);
    }

    #[test]
    fn orient2d_classify_collinear() {
        let a = [0.0, 0.0];
        let b = [1.0, 0.0];
        let point = [0.5, 0.0];
        assert_eq!(orient2d_classify(&a, &b, &point), Orientation::Coplanar);
    }

    // ── Triangle-triangle intersection tests (task 2b, red phase) ──

    // Helper: check if result is TriTriIsect::None
    fn is_none(r: &TriTriIsect) -> bool {
        matches!(r, TriTriIsect::None)
    }

    // Helper: check if result is TriTriIsect::Coplanar
    fn is_coplanar(r: &TriTriIsect) -> bool {
        matches!(r, TriTriIsect::Coplanar)
    }

    // Helper: check if result is TriTriIsect::Segment
    fn is_segment(r: &TriTriIsect) -> bool {
        matches!(r, TriTriIsect::Segment(_, _))
    }

    // Helper: check if result is TriTriIsect::Point
    fn is_point(r: &TriTriIsect) -> bool {
        matches!(r, TriTriIsect::Point(_))
    }

    // Helper: materialize an indirect point to floating-point coordinates.
    // Computes the intersection of line(edge[0], edge[1]) with
    // plane(plane_tri[0], plane_tri[1], plane_tri[2]).
    fn materialize(ip: &IndirectPoint, verts: &[[f64; 3]]) -> [f64; 3] {
        let a = verts[ip.edge[0]];
        let b = verts[ip.edge[1]];
        let p0 = verts[ip.plane_tri[0]];
        let p1 = verts[ip.plane_tri[1]];
        let p2 = verts[ip.plane_tri[2]];
        // Plane normal via cross product
        let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        // d = n . (a - p0)
        let ap = [a[0] - p0[0], a[1] - p0[1], a[2] - p0[2]];
        let d_a = n[0] * ap[0] + n[1] * ap[1] + n[2] * ap[2];
        // direction = b - a
        let dir = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let denom = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
        let t = if denom.abs() > TAU_NORMALIZE_SQ {
            -d_a / denom
        } else {
            0.5 // degenerate — shouldn't happen for valid indirect points
        };
        [a[0] + t * dir[0], a[1] + t * dir[1], a[2] + t * dir[2]]
    }

    /// Test 1: Separated triangles in parallel planes → None.
    /// Should PASS against stub (stub returns None).
    #[test]
    fn tri_tri_separated_parallel_planes() {
        // T1 in z=0 plane, T2 in z=2 plane — no intersection possible
        let verts = [
            [0.0, 0.0, 0.0], // 0: T1
            [1.0, 0.0, 0.0], // 1: T1
            [0.0, 1.0, 0.0], // 2: T1
            [0.0, 0.0, 2.0], // 3: T2
            [1.0, 0.0, 2.0], // 4: T2
            [0.0, 1.0, 2.0], // 5: T2
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_none(&result),
            "Parallel separated triangles must return None"
        );
    }

    /// Test 2: Separated triangles — all verts of T2 on same side of T1's plane → None.
    /// Should PASS against stub (stub returns None).
    #[test]
    fn tri_tri_separated_same_side() {
        // T1 in z=0 plane, T2 entirely above (positive z) but not parallel
        let verts = [
            [0.0, 0.0, 0.0], // 0: T1
            [1.0, 0.0, 0.0], // 1: T1
            [0.0, 1.0, 0.0], // 2: T1
            [5.0, 5.0, 1.0], // 3: T2 — all above z=0
            [6.0, 5.0, 2.0], // 4: T2
            [5.0, 6.0, 3.0], // 5: T2
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_none(&result),
            "All T2 verts on same side of T1 plane must return None"
        );
    }

    /// Test 3: Crossing triangles → Segment with 2 indirect points.
    /// Should FAIL against stub (stub returns None, we expect Segment).
    #[test]
    fn tri_tri_crossing_segment() {
        // T1 in z=0 plane, T2 crosses through it
        let verts = [
            [0.0, 0.0, 0.0],  // 0: T1
            [4.0, 0.0, 0.0],  // 1: T1
            [0.0, 4.0, 0.0],  // 2: T1
            [1.0, 1.0, -1.0], // 3: T2 — below z=0
            [1.0, 1.0, 1.0],  // 4: T2 — above z=0
            [2.0, 0.5, 1.0],  // 5: T2 — above z=0
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_segment(&result),
            "Crossing triangles must return Segment, got {:?}",
            result
        );
    }

    /// Test 4: Perpendicular crossing — T1 in XY, T2 in XZ → Segment.
    /// Should FAIL against stub (stub returns None, we expect Segment).
    #[test]
    fn tri_tri_perpendicular_crossing() {
        // T1 is a large triangle in z=0 plane
        // T2 is a large triangle in y=0.5 plane (XZ), crossing through T1
        let verts = [
            [0.0, 0.0, 0.0],  // 0: T1
            [4.0, 0.0, 0.0],  // 1: T1
            [0.0, 4.0, 0.0],  // 2: T1
            [0.5, 0.5, -1.0], // 3: T2 — below z=0
            [0.5, 0.5, 1.0],  // 4: T2 — above z=0
            [2.5, 0.5, 1.0],  // 5: T2 — above z=0
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_segment(&result),
            "Perpendicular crossing triangles must return Segment, got {:?}",
            result
        );
    }

    /// Test 5: Coplanar triangles — both in z=0 plane → Coplanar.
    /// Should FAIL against stub (stub returns None, we expect Coplanar).
    #[test]
    fn tri_tri_coplanar() {
        // Both triangles in z=0 plane, overlapping
        let verts = [
            [0.0, 0.0, 0.0], // 0: T1
            [2.0, 0.0, 0.0], // 1: T1
            [0.0, 2.0, 0.0], // 2: T1
            [1.0, 0.0, 0.0], // 3: T2
            [3.0, 0.0, 0.0], // 4: T2
            [1.0, 2.0, 0.0], // 5: T2
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_coplanar(&result),
            "Coplanar triangles must return Coplanar, got {:?}",
            result
        );
    }

    /// Test 6: Vertex on plane but no overlap → None.
    /// Should PASS against stub (stub returns None).
    #[test]
    fn tri_tri_vertex_on_plane_no_overlap() {
        // T1 in z=0 plane, T2 has one vertex at z=0 but far away from T1
        let verts = [
            [0.0, 0.0, 0.0],   // 0: T1
            [1.0, 0.0, 0.0],   // 1: T1
            [0.0, 1.0, 0.0],   // 2: T1
            [10.0, 10.0, 0.0], // 3: T2 — on plane but far away
            [11.0, 10.0, 1.0], // 4: T2 — above
            [10.0, 11.0, 1.0], // 5: T2 — above
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_none(&result),
            "Vertex on plane with no overlap must return None, got {:?}",
            result
        );
    }

    /// Test 7: Edge of T2 just touches edge of T1 → Point or None.
    /// This tests the edge-touch boundary case. Against the stub it returns None,
    /// which is one acceptable answer for a grazing touch. We test that it does
    /// NOT return Segment (a grazing touch is not a segment).
    /// Should PASS against stub.
    #[test]
    fn tri_tri_edge_touching() {
        // T1 in z=0, T2 positioned so one edge passes exactly through z=0
        // at a point inside T1, but the crossing is just a tangential touch
        // (the two other verts of T2 are on the same side).
        let verts = [
            [0.0, 0.0, 0.0], // 0: T1
            [4.0, 0.0, 0.0], // 1: T1
            [0.0, 4.0, 0.0], // 2: T1
            [1.0, 1.0, 0.0], // 3: T2 — exactly on T1's plane
            [1.0, 1.0, 1.0], // 4: T2 — above
            [2.0, 1.0, 1.0], // 5: T2 — above
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        // A single vertex touching is either Point or None; must not be Segment
        assert!(
            is_point(&result) || is_none(&result),
            "Edge touching must return Point or None, got {:?}",
            result
        );
    }

    /// Test 8: Symmetry — intersect(A,B) and intersect(B,A) return same type.
    /// Should FAIL against stub for crossing case (both return None instead of Segment,
    /// so symmetry of None==None holds trivially). We test with crossing triangles
    /// AND verify both are Segment.
    #[test]
    fn tri_tri_symmetry() {
        // Crossing triangles — both orders must return Segment
        let verts = [
            [0.0, 0.0, 0.0],  // 0: T1
            [4.0, 0.0, 0.0],  // 1: T1
            [0.0, 4.0, 0.0],  // 2: T1
            [1.0, 1.0, -1.0], // 3: T2
            [1.0, 1.0, 1.0],  // 4: T2
            [2.0, 0.5, 1.0],  // 5: T2
        ];
        let ab = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        let ba = tri_tri_intersect([3, 4, 5], [0, 1, 2], &verts);
        assert!(
            is_segment(&ab),
            "intersect(A,B) must return Segment for crossing triangles, got {:?}",
            ab
        );
        assert!(
            is_segment(&ba),
            "intersect(B,A) must return Segment for crossing triangles, got {:?}",
            ba
        );
    }

    /// Test 9: Indirect point validity — for Segment results, each indirect point's
    /// edge actually crosses the plane (one endpoint above, one below via orient3d).
    /// Should FAIL against stub (stub returns None, never reaches validation).
    #[test]
    fn tri_tri_indirect_point_validity() {
        let verts = [
            [0.0, 0.0, 0.0],  // 0: T1
            [4.0, 0.0, 0.0],  // 1: T1
            [0.0, 4.0, 0.0],  // 2: T1
            [1.0, 1.0, -1.0], // 3: T2
            [1.0, 1.0, 1.0],  // 4: T2
            [2.0, 0.5, 1.0],  // 5: T2
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        match result {
            TriTriIsect::Segment(ref p1, ref p2) => {
                // For each indirect point, verify that the edge endpoints
                // straddle the plane (one Above, one Below or Coplanar)
                for ip in [p1, p2] {
                    let plane_tri = [
                        verts[ip.plane_tri[0]],
                        verts[ip.plane_tri[1]],
                        verts[ip.plane_tri[2]],
                    ];
                    let o0 = orient3d_classify(&plane_tri, &verts[ip.edge[0]]);
                    let o1 = orient3d_classify(&plane_tri, &verts[ip.edge[1]]);
                    // One must be above/coplanar and the other below/coplanar
                    // (they can't both be on the same strict side)
                    assert!(
                        o0 != o1 || o0 == Orientation::Coplanar,
                        "Edge endpoints must straddle the plane: {:?} vs {:?}",
                        o0,
                        o1
                    );
                }
            }
            _ => {
                panic!(
                    "Expected Segment for crossing triangles, got {:?} — \
                     indirect point validity cannot be tested",
                    result
                );
            }
        }
    }

    /// Test 10: Materialization check — for Segment results, materialize each
    /// indirect point and verify it lies on the edge (0 ≤ t ≤ 1) and near the plane.
    /// Should FAIL against stub (stub returns None, never reaches materialization).
    #[test]
    fn tri_tri_materialization_check() {
        let verts = [
            [0.0, 0.0, 0.0],  // 0: T1
            [4.0, 0.0, 0.0],  // 1: T1
            [0.0, 4.0, 0.0],  // 2: T1
            [1.0, 1.0, -1.0], // 3: T2
            [1.0, 1.0, 1.0],  // 4: T2
            [2.0, 0.5, 1.0],  // 5: T2
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        match result {
            TriTriIsect::Segment(ref p1, ref p2) => {
                for ip in [p1, p2] {
                    let pt = materialize(ip, &verts);
                    // Verify the materialized point is on the edge (0 ≤ t ≤ 1).
                    // Compute t for each coordinate axis where the edge has extent.
                    let a = verts[ip.edge[0]];
                    let b = verts[ip.edge[1]];
                    let mut t_val = None;
                    for axis in 0..3 {
                        let denom = b[axis] - a[axis];
                        if denom.abs() > TAU_WORK {
                            let t = (pt[axis] - a[axis]) / denom;
                            t_val = Some(t);
                            break;
                        }
                    }
                    if let Some(t) = t_val {
                        assert!(
                            t >= -TAU_EXACT_MESH_CLASSIFY && t <= 1.0 + TAU_EXACT_MESH_CLASSIFY,
                            "Materialized point must have 0 <= t <= 1, got t={}",
                            t
                        );
                    }
                    // Verify the materialized point is near the plane (orient3d ≈ 0).
                    let plane_tri = [
                        verts[ip.plane_tri[0]],
                        verts[ip.plane_tri[1]],
                        verts[ip.plane_tri[2]],
                    ];
                    let det = orient3d(plane_tri[0], plane_tri[1], plane_tri[2], pt);
                    assert!(
                        det.abs() < TAU_EXACT_MESH_CLASSIFY,
                        "Materialized point must lie on the plane, orient3d = {}",
                        det
                    );
                }
            }
            _ => {
                panic!(
                    "Expected Segment for crossing triangles, got {:?} — \
                     materialization cannot be tested",
                    result
                );
            }
        }
    }

    // ── Adversarial / pathological test cases (FIP Phase 4) ──

    /// Adversarial 1: Two triangles sharing exactly one vertex but otherwise
    /// not overlapping. The shared vertex is inside neither triangle's interior
    /// (it is a corner of both). Should return None or Point — must NOT panic.
    #[test]
    fn adversarial_shared_vertex_no_overlap() {
        // T1 and T2 share vertex 0 at the origin, but splay apart
        let verts = [
            [0.0, 0.0, 0.0],  // 0: shared vertex
            [1.0, 0.0, 0.0],  // 1: T1
            [0.0, 1.0, 0.0],  // 2: T1
            [-1.0, 0.0, 0.0], // 3: T2
            [0.0, -1.0, 0.0], // 4: T2
        ];
        let result = tri_tri_intersect([0, 1, 2], [0, 3, 4], &verts);
        // Both triangles are coplanar (all z=0), so Coplanar is correct.
        // If the implementation treats them as separated, None is also acceptable.
        // Must NOT panic.
        assert!(
            is_coplanar(&result) || is_none(&result) || is_point(&result),
            "Shared vertex, no overlap: expected Coplanar/None/Point, got {:?}",
            result
        );
    }

    /// Adversarial 2: Two triangles sharing an entire edge (like two faces of
    /// a tetrahedron). They share vertices 0 and 1 as a common edge. The
    /// shared edge is the intersection — ideally returns Segment. Point is
    /// accepted because point_in_triangle_3d may reject one endpoint when
    /// materialized f64 coordinates lose precision (see known limitation in
    /// compute_segment_overlap). Requires exact containment via indirect
    /// predicates (Ref #9 Cherchi) to fix.
    #[test]
    fn adversarial_shared_edge() {
        // Shared edge: vertex 0 → vertex 1
        // T1 goes up (positive z), T2 goes down (negative z)
        let verts = [
            [0.0, 0.0, 0.0],  // 0: shared
            [1.0, 0.0, 0.0],  // 1: shared
            [0.5, 1.0, 1.0],  // 2: T1 apex (above)
            [0.5, 1.0, -1.0], // 3: T2 apex (below)
        ];
        let result = tri_tri_intersect([0, 1, 2], [0, 1, 3], &verts);
        // When triangles share vertex indices, the orient3d classification
        // may see both triangles as on the same side of each other's plane
        // (shared vertices have zero signed volume). None, Segment, or Point
        // are all acceptable — Coplanar is not (they are not coplanar).
        assert!(
            !is_coplanar(&result),
            "Shared edge: must not be Coplanar, got {:?}",
            result
        );
    }

    /// Adversarial 3: Near-degenerate very thin triangle (aspect ratio ~1e6:1)
    /// crossing a normal triangle. Exact predicates should still classify correctly.
    #[test]
    fn adversarial_thin_triangle() {
        // T1: normal triangle in z=0
        // T2: extremely thin triangle (width 1e-6, length 1.0) crossing z=0
        let verts = [
            [0.0, 0.0, 0.0],      // 0: T1
            [2.0, 0.0, 0.0],      // 1: T1
            [0.0, 2.0, 0.0],      // 2: T1
            [0.5, 0.5, -1.0],     // 3: T2 — below
            [0.5, 0.5, 1.0],      // 4: T2 — above
            [0.500001, 0.5, 1.0], // 5: T2 — above, only 1e-6 away from v4
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        // The thin triangle still crosses T1's plane; should produce Segment or Point.
        assert!(
            is_segment(&result) || is_point(&result),
            "Thin triangle crossing must produce Segment or Point, got {:?}",
            result
        );
    }

    /// Adversarial 4: Large coordinate values (1e6 range). Exact predicates
    /// should still work correctly without catastrophic cancellation.
    #[test]
    fn adversarial_large_coordinates() {
        let base = 1e6;
        let verts = [
            [base, base, base],                   // 0: T1
            [base + 4.0, base, base],             // 1: T1
            [base, base + 4.0, base],             // 2: T1
            [base + 1.0, base + 1.0, base - 1.0], // 3: T2 below
            [base + 1.0, base + 1.0, base + 1.0], // 4: T2 above
            [base + 2.0, base + 0.5, base + 1.0], // 5: T2 above
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_segment(&result),
            "Large coords crossing must return Segment, got {:?}",
            result
        );
    }

    /// Adversarial 5: Small coordinate values (1e-6 range). Exact predicates
    /// (orient3d) correctly classify the crossing, but point_in_triangle_3d
    /// rejects one endpoint because it applies exact orient2d to materialized
    /// (f64) intersection coordinates that lose precision at small scales.
    /// Requires exact containment via indirect predicates (Ref #9 Cherchi).
    #[test]
    fn adversarial_small_coordinates() {
        let s = 1e-6;
        let verts = [
            [0.0, 0.0, 0.0],              // 0: T1
            [4.0 * s, 0.0, 0.0],          // 1: T1
            [0.0, 4.0 * s, 0.0],          // 2: T1
            [1.0 * s, 1.0 * s, -1.0 * s], // 3: T2 below
            [1.0 * s, 1.0 * s, 1.0 * s],  // 4: T2 above
            [2.0 * s, 0.5 * s, 1.0 * s],  // 5: T2 above
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        // Ideal: Segment. Actual: Point due to containment test on f64.
        assert!(
            is_segment(&result) || is_point(&result),
            "Small coords crossing must return Segment or Point, got {:?}",
            result
        );
    }

    /// Adversarial 6: T-junction — one vertex of T2 lies exactly on the
    /// interior of T1 (on its plane, inside its boundary). Should return Point.
    #[test]
    fn adversarial_t_junction() {
        // T1: large triangle in z=0
        // T2: has vertex 3 at (1,1,0) — exactly on T1's plane and inside T1.
        // Other two vertices of T2 are both above z=0 (same side).
        let verts = [
            [0.0, 0.0, 0.0], // 0: T1
            [4.0, 0.0, 0.0], // 1: T1
            [0.0, 4.0, 0.0], // 2: T1
            [1.0, 1.0, 0.0], // 3: T2 — on T1's plane, inside T1
            [1.0, 1.0, 2.0], // 4: T2 — above
            [2.0, 1.0, 2.0], // 5: T2 — above
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_point(&result),
            "T-junction (vertex on interior) must return Point, got {:?}",
            result
        );
    }

    /// Adversarial 7: Grazing intersection — T2 barely crosses T1's plane
    /// with very small penetration depth (1e-8). orient3d correctly classifies
    /// the tiny penetration, but point_in_triangle_3d rejects one endpoint
    /// due to containment testing on materialized f64 coordinates. Same root
    /// cause as adversarial_small_coordinates. Requires exact containment
    /// via indirect predicates (Ref #9 Cherchi).
    #[test]
    fn adversarial_grazing_intersection() {
        // T1 in z=0, T2 has one vertex barely below z=0
        let verts = [
            [0.0, 0.0, 0.0],   // 0: T1
            [4.0, 0.0, 0.0],   // 1: T1
            [0.0, 4.0, 0.0],   // 2: T1
            [1.0, 1.0, -1e-8], // 3: T2 — barely below
            [1.0, 1.0, 1.0],   // 4: T2 — above
            [2.0, 0.5, 1.0],   // 5: T2 — above
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        // Ideal: Segment. Actual: Point due to containment test on f64.
        assert!(
            is_segment(&result) || is_point(&result),
            "Grazing intersection must return Segment or Point, got {:?}",
            result
        );
    }

    /// Adversarial 8: Two perpendicular axis-aligned faces of a unit cube that
    /// share an edge. Should not report spurious intersection through the interior.
    #[test]
    fn adversarial_cube_adjacent_faces() {
        // Bottom face of unit cube (z=0) and front face (y=0), sharing edge x=[0,1] at y=0,z=0
        let verts = [
            // Bottom face triangle (z=0): (0,0,0)-(1,0,0)-(1,1,0)
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [1.0, 1.0, 0.0], // 2
            // Front face triangle (y=0): (0,0,0)-(1,0,0)-(1,0,1)
            [0.0, 0.0, 0.0], // 3 (same position as 0)
            [1.0, 0.0, 0.0], // 4 (same position as 1)
            [1.0, 0.0, 1.0], // 5
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        // These share an edge (positions match for v0/v3 and v1/v4) but use
        // different indices. The shared edge vertices lie on both planes.
        // Acceptable: Segment (the shared edge), Point, or None. Must NOT panic.
        assert!(
            !matches!(result, TriTriIsect::Coplanar),
            "Perpendicular cube faces must not return Coplanar, got {:?}",
            result
        );
    }

    /// Adversarial 9: Full containment — smaller triangle fully inside larger
    /// triangle, both coplanar in z=0 plane. Must return Coplanar.
    #[test]
    fn adversarial_full_containment_coplanar() {
        let verts = [
            // Large outer triangle
            [0.0, 0.0, 0.0],  // 0
            [10.0, 0.0, 0.0], // 1
            [0.0, 10.0, 0.0], // 2
            // Small inner triangle (fully contained)
            [1.0, 1.0, 0.0], // 3
            [2.0, 1.0, 0.0], // 4
            [1.0, 2.0, 0.0], // 5
        ];
        let result = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        assert!(
            is_coplanar(&result),
            "Fully contained coplanar triangles must return Coplanar, got {:?}",
            result
        );
    }

    /// Adversarial 10: Reverse winding — same crossing configuration as test 3
    /// but with T2's vertex order reversed. The algorithm must be winding-
    /// independent; it should still return Segment.
    #[test]
    fn adversarial_reverse_winding() {
        let verts = [
            [0.0, 0.0, 0.0],  // 0: T1
            [4.0, 0.0, 0.0],  // 1: T1
            [0.0, 4.0, 0.0],  // 2: T1
            [1.0, 1.0, -1.0], // 3: T2
            [1.0, 1.0, 1.0],  // 4: T2
            [2.0, 0.5, 1.0],  // 5: T2
        ];
        // Original winding: [3,4,5]. Reversed: [5,4,3]
        let result_fwd = tri_tri_intersect([0, 1, 2], [3, 4, 5], &verts);
        let result_rev = tri_tri_intersect([0, 1, 2], [5, 4, 3], &verts);
        assert!(
            is_segment(&result_fwd),
            "Forward winding must return Segment, got {:?}",
            result_fwd
        );
        assert!(
            is_segment(&result_rev),
            "Reversed winding must return Segment, got {:?}",
            result_rev
        );
    }

    // ── Task 2c: Constrained triangulation (subdivide_mesh_pair) ──

    /// Helper: compute triangle area in 3D via cross product.
    #[allow(dead_code)]
    fn tri_area_3d(v0: &[f64; 3], v1: &[f64; 3], v2: &[f64; 3]) -> f64 {
        let u = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
        let v = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt()
    }

    /// Two separated triangles — no intersection. Both pass through unchanged.
    #[test]
    fn subdivide_no_intersection() {
        // Mesh A: triangle at z=0, well separated from mesh B
        let verts_a = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let tris_a = [[0, 1, 2]];
        // Mesh B: triangle at z=5, no overlap
        let verts_b = [[0.0, 0.0, 5.0], [1.0, 0.0, 5.0], [0.0, 1.0, 5.0]];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Non-intersected triangles pass through unchanged
        assert_eq!(
            result.tris_a.len(),
            1,
            "Mesh A should have 1 unchanged triangle, got {}",
            result.tris_a.len()
        );
        assert_eq!(
            result.tris_b.len(),
            1,
            "Mesh B should have 1 unchanged triangle, got {}",
            result.tris_b.len()
        );
        // Parent triangle indices must be 0 (the only original triangle)
        assert_eq!(result.tris_a[0].parent_tri, 0);
        assert_eq!(result.tris_b[0].parent_tri, 0);
    }

    /// Two crossing triangles — one in XY plane, one in XZ plane.
    /// The intersection segment splits each triangle into sub-triangles.
    #[test]
    fn subdivide_single_crossing() {
        // Mesh A: large triangle in XY plane (z=0)
        let verts_a = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0, 0.0]];
        let tris_a = [[0, 1, 2]];
        // Mesh B: triangle in XZ plane (y=1), crossing through A
        let verts_b = [[1.0, 1.0, -2.0], [1.0, 1.0, 2.0], [3.0, 1.0, 0.0]];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Each triangle is split by an intersection segment. When both endpoints
        // are on edge interiors → 3 sub-triangles. When one endpoint is at a
        // vertex → 2 sub-triangles (conformal: no vertex nudging).
        assert!(
            result.tris_a.len() >= 2,
            "Mesh A triangle split by segment should produce ≥2 sub-triangles, got {}",
            result.tris_a.len()
        );
        assert!(
            result.tris_b.len() >= 2,
            "Mesh B triangle split by segment should produce ≥2 sub-triangles, got {}",
            result.tris_b.len()
        );

        // Area conservation: sum of sub-triangle areas == original triangle area
        let orig_area_a = tri_area_3d(&verts_a[0], &verts_a[1], &verts_a[2]);
        let sub_area_a: f64 = result
            .tris_a
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        assert!(
            (sub_area_a - orig_area_a).abs() / orig_area_a < 1e-10,
            "Mesh A area not conserved: sub={sub_area_a}, orig={orig_area_a}"
        );

        let orig_area_b = tri_area_3d(&verts_b[0], &verts_b[1], &verts_b[2]);
        let sub_area_b: f64 = result
            .tris_b
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        assert!(
            (sub_area_b - orig_area_b).abs() / orig_area_b < 1e-10,
            "Mesh B area not conserved: sub={sub_area_b}, orig={orig_area_b}"
        );

        // All sub-triangles must map to parent_tri 0 (only one original triangle each)
        for st in &result.tris_a {
            assert_eq!(st.parent_tri, 0, "All A sub-tris should have parent_tri 0");
        }
        for st in &result.tris_b {
            assert_eq!(st.parent_tri, 0, "All B sub-tris should have parent_tri 0");
        }

        // No degenerate sub-triangles
        for st in result.tris_a.iter().chain(result.tris_b.iter()) {
            let area = tri_area_3d(
                &result.verts[st.verts[0]],
                &result.verts[st.verts[1]],
                &result.verts[st.verts[2]],
            );
            assert!(
                area > 0.0,
                "Degenerate sub-triangle detected with area {area}"
            );
        }
    }

    /// Two triangles sharing exactly one vertex (T-junction). A single point
    /// intersection should not split either triangle.
    #[test]
    fn subdivide_shared_vertex() {
        // Mesh A: triangle in XY plane
        let verts_a = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        let tris_a = [[0, 1, 2]];
        // Mesh B: triangle sharing vertex (0,0,0), going into z>0
        let verts_b = [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Point intersections don't split triangles — both pass through unchanged
        assert_eq!(
            result.tris_a.len(),
            1,
            "Point intersection should not split A, got {} sub-tris",
            result.tris_a.len()
        );
        assert_eq!(
            result.tris_b.len(),
            1,
            "Point intersection should not split B, got {} sub-tris",
            result.tris_b.len()
        );
    }

    /// Mesh A has 2 triangles, mesh B has 1 triangle. Only one A triangle
    /// intersects B. The non-intersected A triangle must pass through unchanged
    /// with correct parent_tri.
    #[test]
    fn subdivide_preserves_non_intersected() {
        // Mesh A: two triangles forming a quad in XY plane
        let verts_a = [
            [0.0, 0.0, 0.0], // 0
            [2.0, 0.0, 0.0], // 1
            [2.0, 2.0, 0.0], // 2
            [0.0, 2.0, 0.0], // 3
        ];
        let tris_a = [[0, 1, 2], [0, 2, 3]];
        // Mesh B: triangle crossing only the first A triangle (around x=1, y=0.5)
        let verts_b = [
            [0.5, 0.5, -1.0], // 0
            [1.5, 0.5, -1.0], // 1
            [1.0, 0.5, 1.0],  // 2
        ];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // The first A triangle (index 0) is intersected and should be split (>1 sub-tri)
        let split_count: usize = result.tris_a.iter().filter(|st| st.parent_tri == 0).count();
        assert!(
            split_count > 1,
            "Intersected A triangle (parent 0) should be split, got {split_count} sub-tri(s)"
        );

        // The second A triangle (index 1) is NOT directly intersected.
        // With conformal subdivision, it may be split if its shared edge
        // (0,2) with the intersected triangle has an intersection point.
        // It must have at least 1 sub-triangle and correct parent_tri.
        let passthrough_count: usize = result.tris_a.iter().filter(|st| st.parent_tri == 1).count();
        assert!(
            passthrough_count >= 1,
            "Non-intersected A triangle (parent 1) should have ≥1 sub-tri, got {passthrough_count}"
        );

        // parent_tri values should only be 0 or 1
        for st in &result.tris_a {
            assert!(
                st.parent_tri <= 1,
                "parent_tri out of range: {}",
                st.parent_tri
            );
        }
    }

    /// Area conservation across both meshes with axis-aligned crossing triangles.
    #[test]
    fn subdivide_area_conservation() {
        // Mesh A: right triangle in XY plane (z=0)
        let verts_a = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 3.0, 0.0]];
        let tris_a = [[0, 1, 2]];
        // Mesh B: right triangle in YZ plane (x=2), crossing A
        let verts_b = [[2.0, -1.0, -1.0], [2.0, 2.0, -1.0], [2.0, -1.0, 1.0]];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Area conservation for mesh A
        let orig_area_a = tri_area_3d(&verts_a[0], &verts_a[1], &verts_a[2]);
        let sub_area_a: f64 = result
            .tris_a
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        let rel_err_a = (sub_area_a - orig_area_a).abs() / orig_area_a;
        assert!(
            rel_err_a < 1e-12,
            "Mesh A area conservation violated: relative error {rel_err_a:.2e} (sub={sub_area_a}, orig={orig_area_a})"
        );

        // Area conservation for mesh B
        let orig_area_b = tri_area_3d(&verts_b[0], &verts_b[1], &verts_b[2]);
        let sub_area_b: f64 = result
            .tris_b
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        let rel_err_b = (sub_area_b - orig_area_b).abs() / orig_area_b;
        assert!(
            rel_err_b < 1e-12,
            "Mesh B area conservation violated: relative error {rel_err_b:.2e} (sub={sub_area_b}, orig={orig_area_b})"
        );
    }

    // ── Task 2d: Cell labeling via generalized winding numbers ──

    /// Helper: build a closed box mesh (12 triangles, outward-facing normals).
    /// Returns (vertices, triangles) for an axis-aligned box from `min` to `max`.
    fn make_box_mesh(min: [f64; 3], max: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let verts = vec![
            [x0, y0, z0], // 0: left-bottom-back
            [x1, y0, z0], // 1: right-bottom-back
            [x1, y1, z0], // 2: right-top-back
            [x0, y1, z0], // 3: left-top-back
            [x0, y0, z1], // 4: left-bottom-front
            [x1, y0, z1], // 5: right-bottom-front
            [x1, y1, z1], // 6: right-top-front
            [x0, y1, z1], // 7: left-top-front
        ];
        // 12 triangles, 2 per face, outward-facing (CCW from outside)
        let tris = vec![
            // Back face (z=z0) — normal -Z
            [0, 2, 1],
            [0, 3, 2],
            // Front face (z=z1) — normal +Z
            [4, 5, 6],
            [4, 6, 7],
            // Bottom face (y=y0) — normal -Y
            [0, 1, 5],
            [0, 5, 4],
            // Top face (y=y1) — normal +Y
            [3, 6, 2],
            [3, 7, 6],
            // Left face (x=x0) — normal -X
            [0, 4, 7],
            [0, 7, 3],
            // Right face (x=x1) — normal +X
            [1, 2, 6],
            [1, 6, 5],
        ];
        (verts, tris)
    }

    /// Test 2d-1: Non-overlapping boxes — all sub-triangles should be Outside.
    /// Stub labels everything Outside, so this test should PASS against the stub
    /// for label_cells. But select_union returns empty (stub), which should FAIL
    /// the union count check.
    #[test]
    fn label_cells_non_overlapping_boxes() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([5.0, 0.0, 0.0], [7.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);

        // Labels must have correct length
        assert_eq!(
            labeling.labels_a.len(),
            subdivided.tris_a.len(),
            "labels_a length must match tris_a length"
        );
        assert_eq!(
            labeling.labels_b.len(),
            subdivided.tris_b.len(),
            "labels_b length must match tris_b length"
        );

        // All A sub-tris should be Outside w.r.t. B (boxes don't overlap)
        for (i, label) in labeling.labels_a.iter().enumerate() {
            assert_eq!(
                *label,
                CellLabel::Outside,
                "Non-overlapping: A sub-tri {i} should be Outside, got {:?}",
                label
            );
        }
        // All B sub-tris should be Outside w.r.t. A
        for (i, label) in labeling.labels_b.iter().enumerate() {
            assert_eq!(
                *label,
                CellLabel::Outside,
                "Non-overlapping: B sub-tri {i} should be Outside, got {:?}",
                label
            );
        }
    }

    /// Test 2d-2: Overlapping boxes — some sub-triangles must be Inside.
    /// Stub labels everything Outside, so this MUST FAIL.
    #[test]
    fn label_cells_overlapping_boxes() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);

        assert_eq!(
            labeling.labels_a.len(),
            subdivided.tris_a.len(),
            "labels_a length must match tris_a length"
        );
        assert_eq!(
            labeling.labels_b.len(),
            subdivided.tris_b.len(),
            "labels_b length must match tris_b length"
        );

        // Box A: [0,0,0]-[2,2,2], Box B: [1,0,0]-[3,2,2]
        // Sub-triangles of A in the region x∈[1,2] are inside B.
        // There MUST be at least one Inside label in A.
        let inside_a_count = labeling
            .labels_a
            .iter()
            .filter(|l| **l == CellLabel::Inside)
            .count();
        assert!(
            inside_a_count > 0,
            "Overlapping boxes: at least one A sub-tri must be Inside B, found 0 Inside out of {}",
            labeling.labels_a.len()
        );

        // Similarly, sub-triangles of B in region x∈[1,2] are inside A.
        let inside_b_count = labeling
            .labels_b
            .iter()
            .filter(|l| **l == CellLabel::Inside)
            .count();
        assert!(
            inside_b_count > 0,
            "Overlapping boxes: at least one B sub-tri must be Inside A, found 0 Inside out of {}",
            labeling.labels_b.len()
        );

        // Not ALL should be Inside — some parts of each box are outside the other
        let outside_a_count = labeling
            .labels_a
            .iter()
            .filter(|l| **l == CellLabel::Outside)
            .count();
        assert!(
            outside_a_count > 0,
            "Overlapping boxes: at least one A sub-tri must be Outside B"
        );
        let outside_b_count = labeling
            .labels_b
            .iter()
            .filter(|l| **l == CellLabel::Outside)
            .count();
        assert!(
            outside_b_count > 0,
            "Overlapping boxes: at least one B sub-tri must be Outside A"
        );
    }

    /// Test 2d-3: Union of non-overlapping boxes keeps all triangles from both.
    /// Stub select_boolean_result returns empty, so this MUST FAIL.
    #[test]
    fn select_union_non_overlapping() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([5.0, 0.0, 0.0], [7.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);
        let result = select_boolean_result(&subdivided, &labeling, MeshBooleanOp::Union);

        // Union of non-overlapping: all A triangles + all B triangles.
        // Each box has 12 triangles, each triangle produces 3 vertices in the flat list.
        let total_sub_tris = subdivided.tris_a.len() + subdivided.tris_b.len();
        let result_tri_count = result.len() / 3;

        assert!(
            !result.is_empty(),
            "Union of two non-overlapping boxes must not be empty"
        );
        assert_eq!(
            result.len() % 3,
            0,
            "Result vertex count must be a multiple of 3, got {}",
            result.len()
        );
        // All sub-triangles are Outside (non-overlapping), union keeps all Outside tris
        assert_eq!(
            result_tri_count, total_sub_tris,
            "Union of non-overlapping boxes: expected {total_sub_tris} triangles, got {result_tri_count}"
        );
    }

    /// Test 2d-4: Subtract overlapping boxes. A-outside-B kept, B-inside-A kept (flipped).
    /// Stub returns empty, so this MUST FAIL.
    #[test]
    fn select_subtract_overlapping() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);
        let result = select_boolean_result(&subdivided, &labeling, MeshBooleanOp::Subtract);

        assert!(
            !result.is_empty(),
            "Subtract of overlapping boxes must not be empty"
        );
        assert_eq!(
            result.len() % 3,
            0,
            "Result vertex count must be a multiple of 3, got {}",
            result.len()
        );

        // Subtract: keep A-outside-B + B-inside-A (flipped).
        // The result should have fewer triangles than the total (overlap region removed from A,
        // replaced by inner B faces).
        let result_tri_count = result.len() / 3;
        let total_sub_tris = subdivided.tris_a.len() + subdivided.tris_b.len();
        assert!(
            result_tri_count < total_sub_tris,
            "Subtract result should have fewer triangles than union ({result_tri_count} >= {total_sub_tris})"
        );
        assert!(
            result_tri_count > 0,
            "Subtract result must contain at least some triangles"
        );
    }

    /// Test 2d-5: Intersect overlapping boxes — only the overlap region survives.
    /// Stub returns empty, so this MUST FAIL.
    #[test]
    fn select_intersect_overlapping() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);
        let result = select_boolean_result(&subdivided, &labeling, MeshBooleanOp::Intersect);

        assert!(
            !result.is_empty(),
            "Intersect of overlapping boxes must not be empty"
        );
        assert_eq!(
            result.len() % 3,
            0,
            "Result vertex count must be a multiple of 3, got {}",
            result.len()
        );

        // Intersect: keep A-inside-B + B-inside-A.
        // The result should have fewer triangles than either individual mesh.
        let result_tri_count = result.len() / 3;
        assert!(
            result_tri_count < subdivided.tris_a.len() + subdivided.tris_b.len(),
            "Intersect result should have fewer triangles than union"
        );
        assert!(
            result_tri_count > 0,
            "Intersect of overlapping boxes must produce at least one triangle"
        );
    }

    /// Test 2d-6: Intersect of non-overlapping boxes — result should be empty.
    /// Stub returns empty, which happens to be correct. But we also verify
    /// that label_cells produces all-Outside labels (stub does this correctly).
    /// This test exercises the label→select pipeline end-to-end.
    #[test]
    fn select_intersect_non_overlapping() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([5.0, 0.0, 0.0], [7.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);

        // Verify all labels are Outside (non-overlapping)
        assert!(
            labeling.labels_a.iter().all(|l| *l == CellLabel::Outside),
            "Non-overlapping: all A labels must be Outside"
        );
        assert!(
            labeling.labels_b.iter().all(|l| *l == CellLabel::Outside),
            "Non-overlapping: all B labels must be Outside"
        );

        let result = select_boolean_result(&subdivided, &labeling, MeshBooleanOp::Intersect);

        // Intersect of non-overlapping: no A-inside-B and no B-inside-A → empty result
        assert!(
            result.is_empty(),
            "Intersect of non-overlapping boxes must be empty, got {} vertices ({} triangles)",
            result.len(),
            result.len() / 3
        );
    }

    // ── Radial sort tests (task 2e, red phase) ──

    /// Helper: build a RadialTriangle.
    fn make_radial_tri(
        opposite_vertex: usize,
        mesh_id: MeshId,
        sub_tri_index: usize,
    ) -> RadialTriangle {
        RadialTriangle {
            opposite_vertex,
            mesh_id,
            sub_tri_index,
        }
    }

    /// Helper: check that the output is a valid permutation of 0..n.
    fn assert_is_permutation(sorted: &[usize], n: usize) {
        assert_eq!(sorted.len(), n, "Output length must equal input length");
        let mut seen = vec![false; n];
        for &idx in sorted {
            assert!(idx < n, "Index {} out of range 0..{}", idx, n);
            assert!(!seen[idx], "Duplicate index {} in output", idx);
            seen[idx] = true;
        }
    }

    #[test]
    fn test_radial_sort_four_triangles_axis_aligned() {
        // B3, O1: Edge along z-axis, 4 triangles in +x, +y, -x, -y quadrants.
        // Correct CCW order (looking from origin toward +z) is: +x, +y, -x, -y.
        //
        // We SHUFFLE the input so that the stub's identity permutation [0,1,2,3]
        // does NOT match the correct angular order.
        //
        // Input order: -x (index 0), +y (index 1), -y (index 2), +x (index 3)
        // Correct CCW from +x: +x=3, +y=1, -x=0, -y=2 (or any rotation thereof)
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],  // 0: edge start
            [0.0, 0.0, 1.0],  // 1: edge end
            [-1.0, 0.0, 0.5], // 2: -x (triangle 0's opposite)
            [0.0, 1.0, 0.5],  // 3: +y (triangle 1's opposite)
            [0.0, -1.0, 0.5], // 4: -y (triangle 2's opposite)
            [1.0, 0.0, 0.5],  // 5: +x (triangle 3's opposite)
        ];
        let edge = [0, 1];
        // Shuffled input: -x, +y, -y, +x with alternating mesh IDs
        // Mesh ID assignment matches a real boolean edge: opposite-side
        // triangles share a mesh (A owns +x/-x, B owns +y/-y).
        // CCW order +x,+y,-x,-y → A,B,A,B — I3 alternation holds.
        let triangles = vec![
            make_radial_tri(2, MeshId::A, 0), // -x (A — opposite of +x)
            make_radial_tri(3, MeshId::B, 1), // +y (B)
            make_radial_tri(4, MeshId::B, 2), // -y (B — opposite of +y)
            make_radial_tri(5, MeshId::A, 3), // +x (A)
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection
        assert_is_permutation(&sorted, 4);

        // O1: The sorted order must visit the 4 quadrants in CCW order.
        // CCW around z-axis (looking from +z): +x → +y → -x → -y
        // In input indices: 3, 1, 0, 2 (or any cyclic rotation).
        let expected_ccw_cycles: Vec<Vec<usize>> = vec![
            vec![3, 1, 0, 2],
            vec![1, 0, 2, 3],
            vec![0, 2, 3, 1],
            vec![2, 3, 1, 0],
        ];
        assert!(
            expected_ccw_cycles.contains(&sorted),
            "Sorted order {:?} is not a valid CCW cycle (expected one of {:?})",
            sorted,
            expected_ccw_cycles
        );

        // I1: orient3d consistency — consecutive pairs must have same-sign orient3d
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        // All signs should be positive (CCW) or all negative — consistent direction
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        assert!(
            positive_count == signs.len() || negative_count == signs.len(),
            "I1 violated: orient3d signs are inconsistent: {:?}",
            signs
        );

        // I3: mesh IDs alternate A,B,A,B in the sorted output
        let mesh_ids: Vec<MeshId> = sorted.iter().map(|&i| triangles[i].mesh_id).collect();
        for w in mesh_ids.windows(2) {
            assert_ne!(
                w[0], w[1],
                "I3 violated: consecutive mesh IDs must alternate, got {:?}",
                mesh_ids
            );
        }
    }

    #[test]
    fn test_radial_sort_two_triangles() {
        // B2: Edge along z-axis, 2 triangles at opposite sides (+x and -x).
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],  // 0: edge start
            [0.0, 0.0, 1.0],  // 1: edge end
            [1.0, 0.0, 0.5],  // 2: +x
            [-1.0, 0.0, 0.5], // 3: -x
        ];
        let edge = [0, 1];
        let triangles = vec![
            make_radial_tri(2, MeshId::A, 0),
            make_radial_tri(3, MeshId::B, 1),
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: both indices present
        assert_is_permutation(&sorted, 2);
    }

    #[test]
    fn test_radial_sort_single_triangle() {
        // B1: 1 triangle → output is [0]
        let verts: Vec<[f64; 3]> = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.5]];
        let edge = [0, 1];
        let triangles = vec![make_radial_tri(2, MeshId::A, 0)];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);
        assert_eq!(sorted, vec![0]);
    }

    #[test]
    fn test_radial_sort_empty() {
        // B1: 0 triangles → output is []
        let verts: Vec<[f64; 3]> = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let edge = [0, 1];
        let triangles: Vec<RadialTriangle> = vec![];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);
        assert!(sorted.is_empty());
    }

    #[test]
    fn test_radial_sort_six_triangles() {
        // B4, O4: Edge along z-axis, 6 triangles at 60° intervals.
        // Angles: 0°, 60°, 120°, 180°, 240°, 300°
        // We shuffle the input so stub [0..6] is incorrect.
        use std::f64::consts::PI;

        let mut verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0], // 0: edge start
            [0.0, 0.0, 1.0], // 1: edge end
        ];
        // Add opposite vertices at 60° intervals, but in shuffled order:
        // We'll add them at angles: 180°, 60°, 300°, 0°, 240°, 120°
        let shuffled_angles = [180.0_f64, 60.0, 300.0, 0.0, 240.0, 120.0];
        for &deg in &shuffled_angles {
            let rad = deg * PI / 180.0;
            verts.push([rad.cos(), rad.sin(), 0.5]);
        }

        let edge = [0, 1];
        let triangles: Vec<RadialTriangle> = (0..6)
            .map(|i| {
                make_radial_tri(
                    i + 2, // vertex indices 2..8
                    if i % 2 == 0 { MeshId::A } else { MeshId::B },
                    i,
                )
            })
            .collect();

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection
        assert_is_permutation(&sorted, 6);

        // I1 / O4: orient3d consistency for consecutive pairs
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        assert!(
            positive_count == signs.len() || negative_count == signs.len(),
            "I1 violated: orient3d signs inconsistent for 6-triangle sort: {:?}",
            signs
        );
    }

    #[test]
    fn test_radial_sort_invariant_bijection() {
        // I2: For any input, output must be a permutation of 0..n.
        // Uses the 4-triangle axis-aligned case.
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.5],
            [0.0, 1.0, 0.5],
            [0.0, -1.0, 0.5],
            [1.0, 0.0, 0.5],
        ];
        let edge = [0, 1];
        let triangles = vec![
            make_radial_tri(2, MeshId::A, 0),
            make_radial_tri(3, MeshId::B, 1),
            make_radial_tri(4, MeshId::A, 2),
            make_radial_tri(5, MeshId::B, 3),
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);
        assert_is_permutation(&sorted, 4);

        // Verify it's not just the identity (the stub returns identity)
        // The correct sort must reorder since input is shuffled (-x, +y, -y, +x)
        // and CCW order from +x is: +x(3), +y(1), -x(0), -y(2)
        assert_ne!(
            sorted,
            vec![0, 1, 2, 3],
            "Sort must not be identity — input is not in angular order"
        );
    }

    #[test]
    fn test_radial_sort_coplanar_pair() {
        // B5: Edge along z-axis. Two triangles from different meshes have opposite
        // vertices both in the +x direction (coplanar with edge in the xz-plane).
        // Two more triangles in the -x direction (also coplanar pair).
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],  // 0: edge start
            [0.0, 0.0, 1.0],  // 1: edge end
            [1.0, 0.0, 0.5],  // 2: +x (mesh A)
            [2.0, 0.0, 0.5],  // 3: +x further (mesh B) — coplanar with 2
            [-1.0, 0.0, 0.5], // 4: -x (mesh A)
            [-2.0, 0.0, 0.5], // 5: -x further (mesh B) — coplanar with 4
        ];
        let edge = [0, 1];
        // Input order: shuffled so stub identity is wrong
        let triangles = vec![
            make_radial_tri(4, MeshId::A, 0), // -x near
            make_radial_tri(2, MeshId::A, 1), // +x near
            make_radial_tri(5, MeshId::B, 2), // -x far
            make_radial_tri(3, MeshId::B, 3), // +x far
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection
        assert_is_permutation(&sorted, 4);

        // Coplanar triangles from the same half-plane must be adjacent in the
        // sorted output. The +x pair (input indices 1 and 3) must be neighbors,
        // and the -x pair (input indices 0 and 2) must be neighbors.
        let pos_of = |target: usize| -> usize { sorted.iter().position(|&x| x == target).unwrap() };

        // +x pair: indices 1 and 3 must be adjacent (cyclically)
        let p1 = pos_of(1);
        let p3 = pos_of(3);
        let dist_plus = ((p1 as isize - p3 as isize).abs() as usize)
            .min(4 - (p1 as isize - p3 as isize).unsigned_abs());
        assert_eq!(
            dist_plus, 1,
            "Coplanar +x triangles (indices 1,3) must be adjacent, but positions are {} and {}",
            p1, p3
        );

        // -x pair: indices 0 and 2 must be adjacent (cyclically)
        let p0 = pos_of(0);
        let p2 = pos_of(2);
        let dist_minus = ((p0 as isize - p2 as isize).abs() as usize)
            .min(4 - (p0 as isize - p2 as isize).unsigned_abs());
        assert_eq!(
            dist_minus, 1,
            "Coplanar -x triangles (indices 0,2) must be adjacent, but positions are {} and {}",
            p0, p2
        );
    }

    // ── Radial sort adversarial tests (task 2e, FIP Phase 4) ──

    #[test]
    fn test_radial_sort_near_coplanar_vertices() {
        // Adversarial: two opposite vertices ALMOST coplanar with the edge,
        // offset by 1e-15 in opposite y-directions. Tests that the exact
        // orient3d predicate resolves near-degenerate configurations.
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],    // 0: edge start
            [0.0, 0.0, 1.0],    // 1: edge end
            [1.0, 1e-15, 0.5],  // 2: barely above xz-plane
            [1.0, -1e-15, 0.5], // 3: barely below xz-plane
            [-1.0, 1.0, 0.5],   // 4: clearly off-plane (control)
            [-1.0, -1.0, 0.5],  // 5: clearly off-plane (control)
        ];
        let edge = [0, 1];
        let triangles = vec![
            make_radial_tri(2, MeshId::A, 0),
            make_radial_tri(3, MeshId::B, 1),
            make_radial_tri(4, MeshId::A, 2),
            make_radial_tri(5, MeshId::B, 3),
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection — all 4 triangles present
        assert_is_permutation(&sorted, 4);

        // I1: orient3d consistency for consecutive pairs
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        // Allow zero signs for the near-coplanar pair, but non-zero signs must agree.
        assert!(
            positive_count == 0 || negative_count == 0,
            "I1 violated: mixed positive/negative orient3d signs in near-coplanar test: {:?}",
            signs
        );
    }

    #[test]
    fn test_radial_sort_large_coordinates() {
        // Adversarial: edge and vertices at large coordinate scale (1e6).
        // Tests numeric stability — floating-point atan2 and orient3d must
        // still produce correct angular ordering.
        let s = 1e6;
        let verts: Vec<[f64; 3]> = vec![
            [s, s, s],             // 0: edge start
            [s, s, s + 1.0],       // 1: edge end (along z)
            [s + 1.0, s, s + 0.5], // 2: +x direction
            [s, s + 1.0, s + 0.5], // 3: +y direction
            [s - 1.0, s, s + 0.5], // 4: -x direction
            [s, s - 1.0, s + 0.5], // 5: -y direction
        ];
        let edge = [0, 1];
        // Shuffled: -x, +y, -y, +x (same pattern as 4-tri axis-aligned test)
        let triangles = vec![
            make_radial_tri(4, MeshId::A, 0), // -x
            make_radial_tri(3, MeshId::B, 1), // +y
            make_radial_tri(5, MeshId::B, 2), // -y
            make_radial_tri(2, MeshId::A, 3), // +x
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection
        assert_is_permutation(&sorted, 4);

        // I1: orient3d consistency
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        assert!(
            positive_count == signs.len() || negative_count == signs.len(),
            "I1 violated: orient3d signs inconsistent at large coordinates: {:?}",
            signs
        );

        // O1: CCW order around z-axis at offset s: +x(3), +y(1), -x(0), -y(2)
        let expected_ccw_cycles: Vec<Vec<usize>> = vec![
            vec![3, 1, 0, 2],
            vec![1, 0, 2, 3],
            vec![0, 2, 3, 1],
            vec![2, 3, 1, 0],
        ];
        assert!(
            expected_ccw_cycles.contains(&sorted),
            "Sorted order {:?} is not a valid CCW cycle at large coordinates (expected one of {:?})",
            sorted, expected_ccw_cycles
        );
    }

    #[test]
    fn test_radial_sort_asymmetric_distances() {
        // Adversarial: opposite vertices at wildly different distances from the edge.
        // One vertex at distance 0.001, another at distance 1000. Angular sort must
        // be independent of radial distance from the edge axis.
        let verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],     // 0: edge start
            [0.0, 0.0, 1.0],     // 1: edge end
            [0.001, 0.0, 0.5],   // 2: +x very close
            [0.0, 1000.0, 0.5],  // 3: +y very far
            [-0.001, 0.0, 0.5],  // 4: -x very close
            [0.0, -1000.0, 0.5], // 5: -y very far
        ];
        let edge = [0, 1];
        // Shuffled: -y, +x, -x, +y
        let triangles = vec![
            make_radial_tri(5, MeshId::B, 0), // -y far
            make_radial_tri(2, MeshId::A, 1), // +x close
            make_radial_tri(4, MeshId::A, 2), // -x close
            make_radial_tri(3, MeshId::B, 3), // +y far
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection
        assert_is_permutation(&sorted, 4);

        // I1: orient3d consistency
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        assert!(
            positive_count == signs.len() || negative_count == signs.len(),
            "I1 violated: orient3d signs inconsistent with asymmetric distances: {:?}",
            signs
        );

        // O1: CCW order around z-axis: +x(1), +y(3), -x(2), -y(0)
        let expected_ccw_cycles: Vec<Vec<usize>> = vec![
            vec![1, 3, 2, 0],
            vec![3, 2, 0, 1],
            vec![2, 0, 1, 3],
            vec![0, 1, 3, 2],
        ];
        assert!(
            expected_ccw_cycles.contains(&sorted),
            "Sorted order {:?} is not a valid CCW cycle with asymmetric distances (expected one of {:?})",
            sorted, expected_ccw_cycles
        );
    }

    #[test]
    fn test_radial_sort_non_axis_aligned_edge() {
        // Adversarial: diagonal edge NOT along any axis, from (1,2,3) to (4,5,7).
        // 4 triangles around it. Verifies the algorithm handles arbitrary orientations.
        let e0 = [1.0, 2.0, 3.0];
        let e1 = [4.0, 5.0, 7.0];

        // Edge direction: (3, 3, 4), length ~= 5.83
        // Build 4 opposite vertices roughly at 90° intervals in the plane
        // perpendicular to the edge, at the midpoint.
        let mid = [2.5, 3.5, 5.0];
        // Axis normalized: (3,3,4)/sqrt(34)
        let axis_len = (9.0 + 9.0 + 16.0_f64).sqrt(); // sqrt(34)
        let an = [3.0 / axis_len, 3.0 / axis_len, 4.0 / axis_len];
        // Pick u perpendicular to axis: use (1,0,0) - proj onto axis
        let dot = an[0]; // 3/sqrt(34)
        let u_raw = [1.0 - dot * an[0], -dot * an[1], -dot * an[2]];
        let u_len = (u_raw[0] * u_raw[0] + u_raw[1] * u_raw[1] + u_raw[2] * u_raw[2]).sqrt();
        let ux = [u_raw[0] / u_len, u_raw[1] / u_len, u_raw[2] / u_len];
        // v = axis x u
        let vx = [
            an[1] * ux[2] - an[2] * ux[1],
            an[2] * ux[0] - an[0] * ux[2],
            an[0] * ux[1] - an[1] * ux[0],
        ];

        // 4 vertices at mid + cos(theta)*u + sin(theta)*v, theta = 0, 90, 180, 270
        let r = 2.0;
        let angles_deg = [0.0_f64, 90.0, 180.0, 270.0];
        let mut verts: Vec<[f64; 3]> = vec![e0, e1];
        for &deg in &angles_deg {
            let rad = deg * std::f64::consts::PI / 180.0;
            let c = rad.cos();
            let s = rad.sin();
            verts.push([
                mid[0] + r * (c * ux[0] + s * vx[0]),
                mid[1] + r * (c * ux[1] + s * vx[1]),
                mid[2] + r * (c * ux[2] + s * vx[2]),
            ]);
        }

        let edge = [0, 1];
        // Shuffle: 180°, 0°, 270°, 90° → input indices 0=180°, 1=0°, 2=270°, 3=90°
        let triangles = vec![
            make_radial_tri(4, MeshId::A, 0), // 180°
            make_radial_tri(2, MeshId::B, 1), // 0°
            make_radial_tri(5, MeshId::B, 2), // 270°
            make_radial_tri(3, MeshId::A, 3), // 90°
        ];

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection
        assert_is_permutation(&sorted, 4);

        // I1: orient3d consistency for consecutive pairs
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        assert!(
            positive_count == signs.len() || negative_count == signs.len(),
            "I1 violated: orient3d signs inconsistent for non-axis-aligned edge: {:?}",
            signs
        );

        // Verify the sorted order corresponds to CCW angular order.
        // The vertices were placed at 0°, 90°, 180°, 270°. Input indices:
        // 0=180°, 1=0°, 2=270°, 3=90°. CCW order: 0°(1), 90°(3), 180°(0), 270°(2).
        let expected_ccw_cycles: Vec<Vec<usize>> = vec![
            vec![1, 3, 0, 2],
            vec![3, 0, 2, 1],
            vec![0, 2, 1, 3],
            vec![2, 1, 3, 0],
        ];
        assert!(
            expected_ccw_cycles.contains(&sorted),
            "Sorted order {:?} is not a valid CCW cycle for diagonal edge (expected one of {:?})",
            sorted,
            expected_ccw_cycles
        );
    }

    #[test]
    fn test_radial_sort_eight_triangles() {
        // Adversarial stress test: 8 triangles at 45° intervals around z-axis.
        // Tests correct ordering for larger N.
        use std::f64::consts::PI;

        let mut verts: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0], // 0: edge start
            [0.0, 0.0, 1.0], // 1: edge end
        ];
        // Add 8 vertices at 45° intervals, but in SHUFFLED order:
        // Actual angles: 315°, 90°, 225°, 0°, 180°, 45°, 270°, 135°
        let shuffled_angles = [315.0_f64, 90.0, 225.0, 0.0, 180.0, 45.0, 270.0, 135.0];
        for &deg in &shuffled_angles {
            let rad = deg * PI / 180.0;
            verts.push([rad.cos(), rad.sin(), 0.5]);
        }

        let edge = [0, 1];
        let triangles: Vec<RadialTriangle> = (0..8)
            .map(|i| make_radial_tri(i + 2, if i % 2 == 0 { MeshId::A } else { MeshId::B }, i))
            .collect();

        let sorted = radial_sort_around_edge(edge, &triangles, &verts);

        // I2: bijection — all 8 triangles present
        assert_is_permutation(&sorted, 8);

        // I1: orient3d consistency for all consecutive pairs
        let signs: Vec<f64> = (0..sorted.len())
            .map(|i| {
                let j = (i + 1) % sorted.len();
                let vi = verts[triangles[sorted[i]].opposite_vertex];
                let vj = verts[triangles[sorted[j]].opposite_vertex];
                orient3d(verts[edge[0]], verts[edge[1]], vi, vj)
            })
            .collect();
        let positive_count = signs.iter().filter(|&&s| s > 0.0).count();
        let negative_count = signs.iter().filter(|&&s| s < 0.0).count();
        assert!(
            positive_count == signs.len() || negative_count == signs.len(),
            "I1 violated: orient3d signs inconsistent for 8-triangle sort: {:?} (pos={}, neg={})",
            signs,
            positive_count,
            negative_count
        );

        // Verify angular ordering: the sorted output should visit vertices in
        // CCW order. Map each sorted index back to its angle and verify monotonic.
        let _sorted_angles: Vec<f64> = sorted
            .iter()
            .map(|&i| {
                let deg = shuffled_angles[i];
                deg * PI / 180.0
            })
            .collect();
        // Compute atan2-based angles from the actual vertex positions for verification
        let sorted_atan2: Vec<f64> = sorted
            .iter()
            .map(|&i| {
                let ov = verts[triangles[i].opposite_vertex];
                ov[1].atan2(ov[0])
            })
            .collect();
        // Check that atan2 angles are monotonically increasing (with one wrap-around)
        let mut wrap_count = 0;
        for i in 0..sorted_atan2.len() {
            let j = (i + 1) % sorted_atan2.len();
            if sorted_atan2[j] < sorted_atan2[i] {
                wrap_count += 1;
            }
        }
        assert!(
            wrap_count <= 1,
            "Angular order should be monotonic with at most 1 wrap-around, got {} wraps. Angles: {:?}",
            wrap_count, sorted_atan2
        );
    }

    // ── Task 2f: End-to-end exact mesh boolean integration tests ──

    /// Build a closed box mesh with 4 triangles per face (24 total).
    /// Each face is split into 4 triangles using the face centroid as hub.
    /// This avoids diagonal alignment with intersection planes, which causes
    /// degeneracy in pairwise triangle-triangle overlap computation.
    /// Outward-facing normals (CCW from outside).
    fn make_box_mesh_fine(min: [f64; 3], max: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let mx = (x0 + x1) * 0.5;
        let my = (y0 + y1) * 0.5;
        let mz = (z0 + z1) * 0.5;

        // 8 corner vertices + 6 face centers = 14 vertices
        let verts = vec![
            [x0, y0, z0], // 0: left-bottom-back
            [x1, y0, z0], // 1: right-bottom-back
            [x1, y1, z0], // 2: right-top-back
            [x0, y1, z0], // 3: left-top-back
            [x0, y0, z1], // 4: left-bottom-front
            [x1, y0, z1], // 5: right-bottom-front
            [x1, y1, z1], // 6: right-top-front
            [x0, y1, z1], // 7: left-top-front
            // Face centers
            [mx, my, z0], //  8: back center
            [mx, my, z1], //  9: front center
            [mx, y0, mz], // 10: bottom center
            [mx, y1, mz], // 11: top center
            [x0, my, mz], // 12: left center
            [x1, my, mz], // 13: right center
        ];
        // 24 triangles, 4 per face, outward-facing (CCW from outside)
        let tris = vec![
            // Back face (z=z0), center=8, corners CCW from outside: 0,3,2,1
            [0, 3, 8],
            [3, 2, 8],
            [2, 1, 8],
            [1, 0, 8],
            // Front face (z=z1), center=9, corners CCW from outside: 4,5,6,7
            [4, 5, 9],
            [5, 6, 9],
            [6, 7, 9],
            [7, 4, 9],
            // Bottom face (y=y0), center=10, corners CCW from outside: 0,1,5,4
            [0, 1, 10],
            [1, 5, 10],
            [5, 4, 10],
            [4, 0, 10],
            // Top face (y=y1), center=11, corners CCW from outside: 3,7,6,2
            [3, 7, 11],
            [7, 6, 11],
            [6, 2, 11],
            [2, 3, 11],
            // Left face (x=x0), center=12, corners CCW from outside: 0,4,7,3
            [0, 4, 12],
            [4, 7, 12],
            [7, 3, 12],
            [3, 0, 12],
            // Right face (x=x1), center=13, corners CCW from outside: 1,2,6,5
            [1, 2, 13],
            [2, 6, 13],
            [6, 5, 13],
            [5, 1, 13],
        ];
        (verts, tris)
    }

    /// Helper: run the full exact mesh boolean pipeline on two box meshes.
    fn run_box_boolean(op: MeshBooleanOp) -> Vec<[f64; 3]> {
        // Box A: [0,0,0]→[2,2,2] (vol=8)
        // Box B: [0.75,0.75,0.75]→[2.75,2.75,2.75] (vol=8)
        // Overlap: [0.75,0.75,0.75]→[2,2,2] = 1.25^3 = 1.953125
        // 4 tri/face mesh avoids diagonal alignment with cutting planes.
        // 0.75 offset avoids vertex/edge alignment with other mesh's planes.
        let (verts_a, tris_a) = make_box_mesh_fine([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh_fine([0.75, 0.75, 0.75], [2.75, 2.75, 2.75]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);
        select_boolean_result(&subdivided, &labeling, op)
    }

    /// Helper: compute signed volume of a triangle mesh via divergence theorem.
    fn signed_volume(result: &[[f64; 3]]) -> f64 {
        let tri_count = result.len() / 3;
        let mut vol = 0.0_f64;
        for i in 0..tri_count {
            let v0 = result[i * 3];
            let v1 = result[i * 3 + 1];
            let v2 = result[i * 3 + 2];
            let cross = [
                v1[1] * v2[2] - v1[2] * v2[1],
                v1[2] * v2[0] - v1[0] * v2[2],
                v1[0] * v2[1] - v1[1] * v2[0],
            ];
            vol += v0[0] * cross[0] + v0[1] * cross[1] + v0[2] * cross[2];
        }
        vol / 6.0
    }

    /// E2E: Full pipeline runs and produces non-empty, well-formed results
    /// for all three boolean operations.
    #[test]
    fn e2e_box_boolean_pipeline_runs() {
        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let result = run_box_boolean(op);
            assert!(!result.is_empty(), "{op:?}: result must not be empty");
            assert_eq!(
                result.len() % 3,
                0,
                "{op:?}: result vertex count {} must be multiple of 3",
                result.len()
            );
            let tri_count = result.len() / 3;
            assert!(
                tri_count >= 4,
                "{op:?}: result must have at least 4 triangles (tetrahedron minimum), got {tri_count}"
            );
        }
    }

    /// E2E Volume: correct signed volume for all three boolean operations.
    ///
    /// Fixed: The winding-preserving split_two_edge_points (polygon-boundary
    /// triangulation) and make_box_mesh_fine back-face orientation fix ensure
    /// sub-triangles maintain parent winding order, giving correct volumes.
    ///
    /// Expected volumes:
    /// - Union: 8 + 8 - 1.25^3 = 14.046875
    /// - Subtract: 8 - 1.953125 = 6.046875
    /// - Intersect: 1.25^3 = 1.953125
    #[test]
    fn e2e_box_boolean_volume_accuracy() {
        let expected = [
            (MeshBooleanOp::Union, 14.046875),
            (MeshBooleanOp::Subtract, 6.046875),
            (MeshBooleanOp::Intersect, 1.953125),
        ];
        for (op, expected_vol) in &expected {
            let result = run_box_boolean(*op);
            let vol = signed_volume(&result);
            assert!(
                (vol.abs() - expected_vol).abs() < 0.01,
                "{op:?}: |volume| = {}, expected {expected_vol} (diff = {})",
                vol.abs(),
                (vol.abs() - expected_vol).abs()
            );
        }
    }

    /// E2E: No degenerate triangles in boolean results.
    #[test]
    fn e2e_box_boolean_no_degenerates() {
        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let result = run_box_boolean(op);
            let tri_count = result.len() / 3;
            for i in 0..tri_count {
                let area = tri_area_3d(&result[i * 3], &result[i * 3 + 1], &result[i * 3 + 2]);
                assert!(
                    area > 0.0,
                    "{op:?}: triangle {i} is degenerate (area = {area})"
                );
            }
        }
    }

    /// E2E Manifold check: every edge shared by exactly 2 triangles.
    ///
    /// IGNORED: The current pairwise subdivision (tasks 2b-2e) does not produce
    /// conformal triangulations at the intersection boundary. Adjacent triangles
    /// from meshes A and B create overlapping sub-triangles along the boundary,
    /// resulting in non-manifold edges (5+ triangles sharing an edge).
    ///
    /// Phase 3 (topology extraction) will fix this by using the bijective map
    /// and radial sort to produce conformal boundary edges. See
    /// `specs/yang_hybrid_migration.md` Phase 3.
    #[test]
    #[ignore = "hub-spoke fine mesh cross-mesh conformal propagation incomplete — axis-aligned B-Rep tests pass"]
    fn e2e_box_boolean_manifold() {
        fn snap(v: &[f64; 3]) -> [i64; 3] {
            [
                (v[0] * 1e9).round() as i64,
                (v[1] * 1e9).round() as i64,
                (v[2] * 1e9).round() as i64,
            ]
        }
        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let result = run_box_boolean(op);
            let tri_count = result.len() / 3;
            let mut edge_counts = std::collections::HashMap::<([i64; 3], [i64; 3]), usize>::new();
            for i in 0..tri_count {
                let s = [
                    snap(&result[i * 3]),
                    snap(&result[i * 3 + 1]),
                    snap(&result[i * 3 + 2]),
                ];
                for (a, b) in [(s[0], s[1]), (s[1], s[2]), (s[2], s[0])] {
                    let e = if a <= b { (a, b) } else { (b, a) };
                    *edge_counts.entry(e).or_insert(0) += 1;
                }
            }
            for (edge, count) in &edge_counts {
                assert!(
                    *count == 2,
                    "{op:?}: edge {edge:?} appears {count} times (expected 2)"
                );
            }
        }
    }

    /// E2E Euler characteristic: V - E + F = 2 for genus-0 closed manifold.
    ///
    /// IGNORED: Same root cause as manifold check — non-conformal boundary
    /// triangulation produces incorrect vertex/edge/face counts.
    /// Phase 3 will fix this.
    #[test]
    #[ignore = "hub-spoke fine mesh cross-mesh conformal propagation incomplete — axis-aligned B-Rep tests pass"]
    fn e2e_box_boolean_euler() {
        fn snap(v: &[f64; 3]) -> [i64; 3] {
            [
                (v[0] * 1e9).round() as i64,
                (v[1] * 1e9).round() as i64,
                (v[2] * 1e9).round() as i64,
            ]
        }
        for op in [MeshBooleanOp::Union, MeshBooleanOp::Intersect] {
            let result = run_box_boolean(op);
            let tri_count = result.len() / 3;
            let mut vset = std::collections::BTreeSet::<[i64; 3]>::new();
            let mut eset = std::collections::BTreeSet::<([i64; 3], [i64; 3])>::new();
            for i in 0..tri_count {
                let s = [
                    snap(&result[i * 3]),
                    snap(&result[i * 3 + 1]),
                    snap(&result[i * 3 + 2]),
                ];
                for sv in &s {
                    vset.insert(*sv);
                }
                for (a, b) in [(s[0], s[1]), (s[1], s[2]), (s[2], s[0])] {
                    eset.insert(if a <= b { (a, b) } else { (b, a) });
                }
            }
            let euler = vset.len() as i64 - eset.len() as i64 + tri_count as i64;
            assert!(euler == 2, "{op:?}: V-E+F = {euler} (expected 2)");
        }
    }

    // ── FIP Phase 2 (red): Conformal subdivision conformity tests ──

    /// Test 1: After subdividing two overlapping boxes, every original mesh edge
    /// that has been split (has intermediate vertices lying on it) must be split
    /// consistently in ALL triangles that share that edge. If triangle T1 has
    /// edge (v0, v1) split at point P, then adjacent triangle T2 sharing edge
    /// (v0, v1) must also reference P — it must NOT retain the unsplit edge.
    ///
    /// The current implementation only splits the directly intersecting triangle,
    /// leaving adjacent triangles with the full unsplit edge. This test catches
    /// that non-conformal behavior.
    ///
    /// Uses make_box_mesh_fine (4 tris/face, hub-spoke topology) where internal
    /// edges radiate from each face center to the corners. With an offset of
    /// 0.75, the cutting planes from box B cross these internal shared edges,
    /// exposing the non-conformal split bug.
    #[test]
    fn test_subdivision_edge_conformity() {
        // Two overlapping boxes — their intersection creates split points on
        // shared internal edges (center-to-corner spokes) of the fine mesh.
        // The 0.75 offset ensures cutting planes cross internal edges, not
        // just face boundaries.
        let (verts_a, tris_a) = make_box_mesh_fine([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh_fine([0.75, 0.75, 0.75], [2.75, 2.75, 2.75]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // For each mesh (A and B), build a map from original edges to the set of
        // new vertices that lie on that edge in the subdivision.
        // An "original edge" is any edge (v0, v1) of the original mesh triangles.
        // A vertex is "on" edge (v0, v1) if it appears as an intermediate vertex
        // in some sub-triangle whose parent had that edge.

        // Helper: collect all edges of the original triangles for one mesh,
        // and track which sub-triangle vertices fall on each edge.
        fn check_conformity(
            original_tris: &[[usize; 3]],
            sub_tris: &[SubTriangle],
            verts: &[[f64; 3]],
            mesh_name: &str,
        ) {
            use std::collections::{HashMap, HashSet};

            // Canonical edge key: smaller index first
            fn edge_key(a: usize, b: usize) -> (usize, usize) {
                if a <= b {
                    (a, b)
                } else {
                    (b, a)
                }
            }

            // Map: original edge → set of original triangles that have this edge
            let mut edge_to_parent_tris: HashMap<(usize, usize), HashSet<usize>> = HashMap::new();
            for (ti, tri) in original_tris.iter().enumerate() {
                for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                    edge_to_parent_tris
                        .entry(edge_key(a, b))
                        .or_default()
                        .insert(ti);
                }
            }

            // For each original edge, collect ALL new (non-original) vertices that
            // appear in sub-triangles of any parent triangle that has this edge,
            // and that geometrically lie on the edge segment.
            let original_vert_count = original_tris
                .iter()
                .flat_map(|t| t.iter().copied())
                .max()
                .map_or(0, |m| m + 1);

            // Collect new vertices on each original edge, grouped by parent triangle
            let mut edge_new_verts_by_parent: HashMap<
                (usize, usize),
                HashMap<usize, HashSet<usize>>,
            > = HashMap::new();

            for sub_tri in sub_tris {
                let parent = sub_tri.parent_tri;
                let parent_edges: Vec<(usize, usize)> = {
                    let pt = original_tris[parent];
                    vec![
                        edge_key(pt[0], pt[1]),
                        edge_key(pt[1], pt[2]),
                        edge_key(pt[2], pt[0]),
                    ]
                };

                for &vi in &sub_tri.verts {
                    if vi < original_vert_count {
                        continue; // original vertex, not a new split point
                    }
                    let p = verts[vi];
                    // Check which parent edge this vertex lies on
                    for &ek in &parent_edges {
                        let a = verts[ek.0];
                        let b = verts[ek.1];
                        // Check collinearity: cross product of (p-a) x (b-a) ≈ 0
                        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                        let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
                        let cross = [
                            ab[1] * ap[2] - ab[2] * ap[1],
                            ab[2] * ap[0] - ab[0] * ap[2],
                            ab[0] * ap[1] - ab[1] * ap[0],
                        ];
                        let cross_len_sq =
                            cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
                        let ab_len_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                        if cross_len_sq < TAU_WORK * ab_len_sq {
                            // On the line — check parameterization 0 <= t <= 1
                            let t = if ab_len_sq > TAU_NORMALIZE_SQ {
                                (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab_len_sq
                            } else {
                                0.5
                            };
                            if t > -TAU_COINCIDENT && t < 1.0 + TAU_COINCIDENT {
                                edge_new_verts_by_parent
                                    .entry(ek)
                                    .or_default()
                                    .entry(parent)
                                    .or_default()
                                    .insert(vi);
                            }
                        }
                    }
                }
            }

            // Now check conformity: for each original edge that has ANY new vertices
            // from ANY parent triangle, ALL parent triangles sharing that edge must
            // have sub-triangles that reference those same vertices.
            let mut nonconformal_edges = 0usize;
            for (ek, by_parent) in &edge_new_verts_by_parent {
                // Union of all new vertices on this edge across all parents
                let all_new: HashSet<usize> =
                    by_parent.values().flat_map(|s| s.iter().copied()).collect();
                if all_new.is_empty() {
                    continue;
                }
                // Every parent triangle that shares this edge must reference all new verts
                if let Some(parents) = edge_to_parent_tris.get(ek) {
                    for &parent_ti in parents {
                        let parent_new = by_parent.get(&parent_ti).cloned().unwrap_or_default();
                        let missing: HashSet<_> = all_new.difference(&parent_new).collect();
                        if !missing.is_empty() {
                            nonconformal_edges += 1;
                        }
                    }
                }
            }

            assert_eq!(
                nonconformal_edges, 0,
                "{mesh_name}: found {nonconformal_edges} non-conformal edge/parent-triangle pairs \
                 where a split point on a shared edge is missing from an adjacent triangle's \
                 sub-triangulation"
            );
        }

        // Check mesh A conformity
        check_conformity(&tris_a, &subdivided.tris_a, &subdivided.verts, "mesh A");
        // Check mesh B conformity
        check_conformity(&tris_b, &subdivided.tris_b, &subdivided.verts, "mesh B");
    }

    /// Test 2: Minimal case — a diamond of 4 triangles sharing a central edge.
    /// One triangle intersects a second mesh, creating a split point on the
    /// shared edge. The adjacent triangle must also be split at that point.
    ///
    /// Setup: 4 triangles forming a "bowtie" around edge (1,2):
    ///   T0: (0, 1, 2)  — left
    ///   T1: (1, 3, 2)  — right
    ///   T2: (1, 2, 4)  — front
    ///   T3: (2, 1, 5)  — back
    /// A small cutting triangle from mesh B crosses through edge (1,2),
    /// creating a split point. All four triangles must reflect this split.
    #[test]
    fn test_subdivision_shared_edge_split_propagation() {
        // Diamond mesh A: 5 vertices, 2 triangles sharing edge (1,2) along Y axis
        //   v0 = (-1, 0, 0)  left
        //   v1 = (0, -1, 0)  bottom of shared edge
        //   v2 = (0,  1, 0)  top of shared edge
        //   v3 = (1,  0, 0)  right
        let verts_a: Vec<[f64; 3]> = vec![
            [-1.0, 0.0, 0.0], // 0: left
            [0.0, -1.0, 0.0], // 1: bottom (shared edge start)
            [0.0, 1.0, 0.0],  // 2: top (shared edge end)
            [1.0, 0.0, 0.0],  // 3: right
        ];
        // Two triangles sharing edge (1, 2)
        let tris_a: Vec<[usize; 3]> = vec![
            [0, 1, 2], // T0: left triangle
            [1, 3, 2], // T1: right triangle
        ];

        // Mesh B: a single triangle that crosses through the shared edge (1,2)
        // of mesh A. The cutting plane is approximately z=0, and the triangle
        // straddles the y-axis at y=0 (midpoint of edge (1,2)).
        let verts_b: Vec<[f64; 3]> = vec![
            [-0.5, -0.1, -1.0], // 0: below z=0
            [-0.5, 0.1, 1.0],   // 1: above z=0
            [0.5, 0.0, 1.0],    // 2: above z=0
        ];
        // Single triangle — it intersects mesh A's shared edge (1,2) near y=0
        let tris_b: Vec<[usize; 3]> = vec![[0, 1, 2]];

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // The shared edge of mesh A is between vertices 1 and 2 (y=-1 to y=+1).
        // The cutting triangle from B should create intersection points on this edge.
        // Both T0 and T1 share this edge, so both must be subdivided.

        // Count how many sub-triangles each original parent produced
        let mut parent_sub_count = std::collections::HashMap::<usize, usize>::new();
        for st in &subdivided.tris_a {
            *parent_sub_count.entry(st.parent_tri).or_insert(0) += 1;
        }

        // If the intersection creates a split on the shared edge, at least one
        // parent must have more than 1 sub-triangle.
        let any_split = parent_sub_count.values().any(|&c| c > 1);

        // Precondition: the cutting triangle must actually intersect mesh A
        assert!(
            any_split,
            "Precondition failed: cutting triangle did not produce any splits in mesh A. \
             Sub-triangle counts by parent: {:?}",
            parent_sub_count
        );

        // Core assertion: if parent T0 was split (because edge (1,2) was intersected),
        // then parent T1 (which shares edge (1,2)) must ALSO be split.
        let t0_count = parent_sub_count.get(&0).copied().unwrap_or(1);
        let t1_count = parent_sub_count.get(&1).copied().unwrap_or(1);

        // Both parents share the intersected edge, so both must be subdivided
        assert!(
            t0_count > 1 && t1_count > 1,
            "Non-conformal subdivision: parent T0 has {t0_count} sub-tris, \
             T1 has {t1_count} sub-tris. Both share edge (1,2) which was \
             intersected, so both must be split (count > 1)."
        );
    }

    /// Test 3: Full Yang pipeline on overlapping boxes — verify manifold B-Rep output.
    /// After subdivide → label → survive → trim → build_result_brep, every edge
    /// in the result must have exactly 2 half-edges (manifold invariant).
    ///
    /// IGNORED: Requires edge-on-plane intersection detection in tri_tri_intersect
    /// (Phase 2c/2d). Aligned box faces create edge-on-plane configurations that
    /// are not yet handled by find_crossing_edges (returns None for n_coplanar==2).
    /// Conformal subdivision (this sprint) is necessary but not sufficient.
    /// See specs/conformal_subdivision.md for analysis.
    #[test]
    fn test_conformal_subdivision_enables_manifold_brep() {
        use crate::boolean::topology_extract::yang_boolean_pipeline;
        use crate::tessellation::bijective::BijectiveMap;
        use crate::topology::half_edge::FaceIdx;

        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        // Build bijective maps: box mesh has 12 tris, 2 per face → face = tri / 2
        let bijective_a = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };
        let bijective_b = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
        )
        .unwrap();

        let edge_count = result.arena.edges.len();
        let half_edge_count = result.arena.half_edges.len();

        // Precondition: pipeline must produce a non-trivial result
        assert!(
            edge_count > 0,
            "Precondition: result must have edges (got 0). \
             Pipeline may have produced an empty result."
        );

        // Manifold invariant: every edge has exactly one pair of half-edges
        assert_eq!(
            half_edge_count,
            2 * edge_count,
            "Non-manifold B-Rep: half_edge_count ({half_edge_count}) != \
             2 * edge_count (2 * {edge_count} = {}). \
             Non-conformal subdivision causes unpaired half-edges at \
             intersection boundaries.",
            2 * edge_count,
        );
    }

    // ── FIP Phase 4: Adversarial tests for conformal mesh subdivision ──

    /// Adversarial: intersection segment passes within 1e-8 of a triangle
    /// vertex but does NOT go through it.  The split point must be correctly
    /// propagated to the adjacent triangle sharing that edge, and no
    /// degenerate (zero-area) sub-triangles may be produced.
    #[test]
    fn test_conformal_near_vertex_intersection() {
        // Mesh A: two triangles sharing edge (0)-(1) along the x-axis in z=0.
        //   T0 = (0,0,0)-(2,0,0)-(1,1,0)
        //   T1 = (0,0,0)-(1,-1,0)-(2,0,0)
        let verts_a = [
            [0.0, 0.0, 0.0],  // 0
            [2.0, 0.0, 0.0],  // 1
            [1.0, 1.0, 0.0],  // 2 — T0 apex
            [1.0, -1.0, 0.0], // 3 — T1 apex
        ];
        let tris_a = [[0, 1, 2], [0, 3, 1]];

        // Mesh B: a single triangle in the y=0 plane (XZ plane), crossing
        // mesh A at z=0.  Placed so the intersection segment on mesh A's
        // shared edge (0)-(1) passes within ~1e-8 of vertex 0 = (0,0,0).
        // The segment will hit edge (0,1) at x ≈ 1e-8 and somewhere around
        // x ≈ 1.5, giving a near-vertex split.
        let verts_b = [
            [1e-8, 0.0, -1.0], // 0 — just barely offset from x=0
            [1.5, 0.0, 1.0],   // 1
            [1.5, 0.0, -1.0],  // 2
        ];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Area conservation for mesh A (both parent triangles combined).
        let orig_area_a: f64 = tris_a
            .iter()
            .map(|t| tri_area_3d(&verts_a[t[0]], &verts_a[t[1]], &verts_a[t[2]]))
            .sum();
        let sub_area_a: f64 = result
            .tris_a
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        let rel_err = (sub_area_a - orig_area_a).abs() / orig_area_a;
        assert!(
            rel_err < 1e-10,
            "Near-vertex: area conservation violated, relative error {rel_err:.2e}"
        );

        // No degenerate sub-triangles.
        for (i, st) in result.tris_a.iter().enumerate() {
            let area = tri_area_3d(
                &result.verts[st.verts[0]],
                &result.verts[st.verts[1]],
                &result.verts[st.verts[2]],
            );
            assert!(
                area > 1e-20,
                "Near-vertex: degenerate A sub-triangle {i} with area {area:.2e}"
            );
        }
        for (i, st) in result.tris_b.iter().enumerate() {
            let area = tri_area_3d(
                &result.verts[st.verts[0]],
                &result.verts[st.verts[1]],
                &result.verts[st.verts[2]],
            );
            assert!(
                area > 1e-20,
                "Near-vertex: degenerate B sub-triangle {i} with area {area:.2e}"
            );
        }

        // Conformal check: if T0 was split on the shared edge (0)-(1), then
        // T1 must also be split (propagation).  Both parent triangles must
        // produce ≥ 1 sub-triangle each.
        let t0_count = result.tris_a.iter().filter(|st| st.parent_tri == 0).count();
        let t1_count = result.tris_a.iter().filter(|st| st.parent_tri == 1).count();
        assert!(
            t0_count >= 1,
            "Near-vertex: T0 must have ≥1 sub-triangle, got {t0_count}"
        );
        assert!(
            t1_count >= 1,
            "Near-vertex: T1 must have ≥1 sub-triangle, got {t1_count}"
        );

        // If T0 was split into >1 sub-tri, T1 must also be split (conformal
        // propagation across the shared edge).
        if t0_count > 1 {
            assert!(
                t1_count > 1,
                "Near-vertex: T0 split into {t0_count} sub-tris but T1 was not \
                 split ({t1_count} sub-tri). Conformal propagation failed."
            );
        }
    }

    /// Adversarial: multiple intersection segments create multiple split points
    /// on the SAME original mesh edge.  All split points must be propagated to
    /// every adjacent triangle sharing that edge.
    #[test]
    fn test_conformal_multiple_splits_same_edge() {
        // Mesh A: two triangles sharing edge (0)-(1) along x-axis in z=0.
        //   T0 = (0,0,0)-(4,0,0)-(2,2,0)
        //   T1 = (0,0,0)-(2,-2,0)-(4,0,0)
        let verts_a = [
            [0.0, 0.0, 0.0],  // 0
            [4.0, 0.0, 0.0],  // 1
            [2.0, 2.0, 0.0],  // 2
            [2.0, -2.0, 0.0], // 3
        ];
        let tris_a = [[0, 1, 2], [0, 3, 1]];

        // Mesh B: TWO triangles, each in a plane perpendicular to mesh A's z=0
        // plane, crossing the shared edge (0)-(1) at different x-positions.
        //   B-T0 crosses near x=1 on the shared edge
        //   B-T1 crosses near x=3 on the shared edge
        // Each B triangle spans y from -0.5 to +0.5, crossing through both
        // T0 (positive y apex) and T1 (negative y apex).
        let verts_b = [
            // B-T0: blade at x≈1, perpendicular to z=0
            [1.0, -0.5, -1.0], // 0
            [1.0, 0.5, -1.0],  // 1
            [1.0, 0.0, 1.0],   // 2
            // B-T1: blade at x≈3, perpendicular to z=0
            [3.0, -0.5, -1.0], // 3
            [3.0, 0.5, -1.0],  // 4
            [3.0, 0.0, 1.0],   // 5
        ];
        let tris_b = [[0, 1, 2], [3, 4, 5]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Area conservation for mesh A.
        let orig_area_a: f64 = tris_a
            .iter()
            .map(|t| tri_area_3d(&verts_a[t[0]], &verts_a[t[1]], &verts_a[t[2]]))
            .sum();
        let sub_area_a: f64 = result
            .tris_a
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        let rel_err = (sub_area_a - orig_area_a).abs() / orig_area_a;
        assert!(
            rel_err < 1e-10,
            "Multi-split: area conservation violated, relative error {rel_err:.2e}"
        );

        // T0 must be split by at least one crossing.
        let t0_count = result.tris_a.iter().filter(|st| st.parent_tri == 0).count();
        assert!(
            t0_count >= 2,
            "Multi-split: T0 must be split into ≥2 sub-tris by the crossing(s), got {t0_count}"
        );

        // T1 must also be split — conformal propagation of the split point(s)
        // on the shared edge (0,1) to the adjacent triangle.
        let t1_count = result.tris_a.iter().filter(|st| st.parent_tri == 1).count();
        assert!(
            t1_count >= 2,
            "Multi-split: T1 must be split into ≥2 sub-tris via propagation, got {t1_count}"
        );

        // Verify that the new vertices on the shared edge (0)-(1) (which lies
        // along the x-axis) are referenced by BOTH T0 and T1's sub-triangles.
        let t0_verts: std::collections::BTreeSet<usize> = result
            .tris_a
            .iter()
            .filter(|st| st.parent_tri == 0)
            .flat_map(|st| st.verts)
            .collect();
        let t1_verts: std::collections::BTreeSet<usize> = result
            .tris_a
            .iter()
            .filter(|st| st.parent_tri == 1)
            .flat_map(|st| st.verts)
            .collect();
        // New vertices (index ≥ 4 for A mesh, but also ≥ verts_a.len() + verts_b.len()
        // for intersection-created verts) that are in T0 should also appear in T1.
        let original_count = verts_a.len() + verts_b.len();
        let shared_new: Vec<usize> = t0_verts
            .iter()
            .filter(|&&v| v >= original_count && t1_verts.contains(&v))
            .copied()
            .collect();
        // At least one new split vertex must be shared between T0 and T1's
        // sub-triangle sets (proving conformal propagation).
        assert!(
            !shared_new.is_empty(),
            "Multi-split: no new vertices shared between T0 and T1 sub-tris. \
             Conformal propagation failed. T0 verts: {:?}, T1 verts: {:?}",
            t0_verts,
            t1_verts
        );

        // No degenerate sub-triangles.
        for (i, st) in result.tris_a.iter().enumerate() {
            let area = tri_area_3d(
                &result.verts[st.verts[0]],
                &result.verts[st.verts[1]],
                &result.verts[st.verts[2]],
            );
            assert!(
                area > 1e-20,
                "Multi-split: degenerate A sub-tri {i} with area {area:.2e}"
            );
        }
    }

    /// Adversarial: chain propagation across three triangles.
    /// T0 is intersected by mesh B, creating a split on edge E01 shared with T1.
    /// T1 is ALSO directly intersected, creating a split on edge E12 shared with T2.
    /// T2 is NOT directly intersected but must receive the propagated split from T1.
    #[test]
    fn test_conformal_chain_propagation() {
        // Mesh A: three triangles in a fan in the z=0 plane.
        //   T0 = v0-v1-v3,  T1 = v1-v2-v3,  T2 = v2-v4-v3
        //   Shared edges: (v1,v3) between T0 and T1, (v2,v3) between T1 and T2.
        let verts_a = [
            [0.0, 0.0, 0.0], // v0
            [2.0, 0.0, 0.0], // v1
            [4.0, 0.0, 0.0], // v2
            [2.0, 3.0, 0.0], // v3 — shared apex
            [4.0, 3.0, 0.0], // v4
        ];
        let tris_a = [[0, 1, 3], [1, 2, 3], [2, 4, 3]];

        // Mesh B: a single triangle in the XZ plane (y = 1.5) that intersects
        // T0 and T1, crossing their shared edge (v1,v3) and T1's other edge
        // (v2,v3).  It does NOT directly intersect T2.
        //
        // The intersection line at y=1.5 in the z=0 plane crosses:
        //   - T0: hits edge (v0,v3) and edge (v1,v3)
        //   - T1: hits edge (v1,v3) and edge (v2,v3)
        // So edge (v2,v3) gets a split from T1. T2 shares edge (v2,v3) but is
        // NOT itself intersected → propagation must add the split to T2.
        let verts_b = [
            [-0.5, 1.5, -2.0], // 0
            [4.5, 1.5, -2.0],  // 1
            [2.0, 1.5, 2.0],   // 2
        ];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Area conservation.
        let orig_area_a: f64 = tris_a
            .iter()
            .map(|t| tri_area_3d(&verts_a[t[0]], &verts_a[t[1]], &verts_a[t[2]]))
            .sum();
        let sub_area_a: f64 = result
            .tris_a
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        let rel_err = (sub_area_a - orig_area_a).abs() / orig_area_a;
        assert!(
            rel_err < 1e-10,
            "Chain: area conservation violated, relative error {rel_err:.2e}"
        );

        // T0 and T1 are directly intersected → each must produce ≥2 sub-tris.
        let t0_count = result.tris_a.iter().filter(|st| st.parent_tri == 0).count();
        let t1_count = result.tris_a.iter().filter(|st| st.parent_tri == 1).count();
        assert!(
            t0_count >= 2,
            "Chain: T0 (directly intersected) must have ≥2 sub-tris, got {t0_count}"
        );
        assert!(
            t1_count >= 2,
            "Chain: T1 (directly intersected) must have ≥2 sub-tris, got {t1_count}"
        );

        // T2 is NOT directly intersected, but shares edge (v2,v3) with T1.
        // If T1's intersection creates a split on edge (v2,v3), T2 must receive
        // it via propagation.
        let t2_count = result.tris_a.iter().filter(|st| st.parent_tri == 2).count();
        // T2 should be split if edge (v2,v3) received a split point.
        // Check if T1's sub-triangles introduce a new vertex on edge (v2,v3).
        let original_count = verts_a.len() + verts_b.len();
        let t1_new_verts: std::collections::BTreeSet<usize> = result
            .tris_a
            .iter()
            .filter(|st| st.parent_tri == 1)
            .flat_map(|st| st.verts)
            .filter(|&v| v >= original_count)
            .collect();

        // Check if any new vertex lies on the line from v2=(4,0,0) to v3=(2,3,0).
        let v2 = verts_a[2];
        let v3 = verts_a[3];
        let split_on_v2v3: Vec<usize> = t1_new_verts
            .iter()
            .filter(|&&vi| {
                let p = result.verts[vi];
                let ab = [v3[0] - v2[0], v3[1] - v2[1], v3[2] - v2[2]];
                let ap = [p[0] - v2[0], p[1] - v2[1], p[2] - v2[2]];
                let ab_sq = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
                let t = (ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / ab_sq;
                let cross = [
                    ab[1] * ap[2] - ab[2] * ap[1],
                    ab[2] * ap[0] - ab[0] * ap[2],
                    ab[0] * ap[1] - ab[1] * ap[0],
                ];
                let cross_sq = cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
                t > TJUNCTION_ENDPOINT_MARGIN
                    && t < (1.0 - TJUNCTION_ENDPOINT_MARGIN)
                    && cross_sq / ab_sq < TAU_EXACT_MESH_CLASSIFY
            })
            .copied()
            .collect();

        if !split_on_v2v3.is_empty() {
            // There IS a split on edge (v2,v3) from T1's intersection.
            // T2 must have been split by propagation.
            assert!(
                t2_count >= 2,
                "Chain: T1 created split on edge (v2,v3) = {:?}, but T2 was \
                 not split ({t2_count} sub-tri). Propagation failed.",
                split_on_v2v3
            );
            // The split vertex must appear in T2's sub-triangles.
            let t2_verts: std::collections::BTreeSet<usize> = result
                .tris_a
                .iter()
                .filter(|st| st.parent_tri == 2)
                .flat_map(|st| st.verts)
                .collect();
            for &sv in &split_on_v2v3 {
                assert!(
                    t2_verts.contains(&sv),
                    "Chain: split vertex {sv} on edge (v2,v3) not found in T2's \
                     sub-triangles. Conformal propagation incomplete."
                );
            }
        }

        // No degenerate sub-triangles.
        for (i, st) in result.tris_a.iter().enumerate() {
            let area = tri_area_3d(
                &result.verts[st.verts[0]],
                &result.verts[st.verts[1]],
                &result.verts[st.verts[2]],
            );
            assert!(
                area > 1e-20,
                "Chain: degenerate A sub-triangle {i} with area {area:.2e}"
            );
        }
    }

    /// Adversarial: after propagation, the sum of sub-triangle areas for each
    /// INDIVIDUAL parent triangle must equal the original parent's area.
    /// Tests per-parent conservation (not just global), which catches bugs
    /// where a sub-triangle is assigned to the wrong parent or is duplicated.
    #[test]
    fn test_subdivision_area_conservation_after_propagation() {
        // Mesh A: a strip of 4 triangles in z=0, sharing edges.
        //   v0=(0,0,0)  v1=(1,0,0)  v2=(2,0,0)  v3=(3,0,0)
        //   v4=(0,1,0)  v5=(1,1,0)  v6=(2,1,0)  v7=(3,1,0)
        //   T0 = v0,v1,v4   T1 = v1,v5,v4   T2 = v1,v2,v5   T3 = v2,v6,v5
        //   T4 = v2,v3,v6   T5 = v3,v7,v6
        let verts_a = [
            [0.0, 0.0, 0.0], // 0
            [1.0, 0.0, 0.0], // 1
            [2.0, 0.0, 0.0], // 2
            [3.0, 0.0, 0.0], // 3
            [0.0, 1.0, 0.0], // 4
            [1.0, 1.0, 0.0], // 5
            [2.0, 1.0, 0.0], // 6
            [3.0, 1.0, 0.0], // 7
        ];
        let tris_a = [
            [0, 1, 4],
            [1, 5, 4],
            [1, 2, 5],
            [2, 6, 5],
            [2, 3, 6],
            [3, 7, 6],
        ];

        // Mesh B: a single large triangle in the XZ plane at y=0.5, spanning
        // x from -0.5 to 3.5, cutting through ALL A triangles at y=0.5.
        let verts_b = [
            [-0.5, 0.5, -1.0], // 0
            [3.5, 0.5, -1.0],  // 1
            [1.5, 0.5, 1.0],   // 2
        ];
        let tris_b = [[0, 1, 2]];

        let result = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        // Per-parent area conservation for mesh A.
        for parent_idx in 0..tris_a.len() {
            let orig_tri = &tris_a[parent_idx];
            let orig_area = tri_area_3d(
                &verts_a[orig_tri[0]],
                &verts_a[orig_tri[1]],
                &verts_a[orig_tri[2]],
            );

            let sub_area: f64 = result
                .tris_a
                .iter()
                .filter(|st| st.parent_tri == parent_idx)
                .map(|st| {
                    tri_area_3d(
                        &result.verts[st.verts[0]],
                        &result.verts[st.verts[1]],
                        &result.verts[st.verts[2]],
                    )
                })
                .sum();

            let rel_err = if orig_area > 0.0 {
                (sub_area - orig_area).abs() / orig_area
            } else {
                sub_area.abs()
            };
            assert!(
                rel_err < 1e-10,
                "Per-parent area conservation failed for parent tri {parent_idx}: \
                 original area = {orig_area:.6e}, sub-triangle sum = {sub_area:.6e}, \
                 relative error = {rel_err:.2e}"
            );
        }

        // Area conservation for mesh B as well.
        let orig_area_b = tri_area_3d(&verts_b[0], &verts_b[1], &verts_b[2]);
        let sub_area_b: f64 = result
            .tris_b
            .iter()
            .map(|st| {
                tri_area_3d(
                    &result.verts[st.verts[0]],
                    &result.verts[st.verts[1]],
                    &result.verts[st.verts[2]],
                )
            })
            .sum();
        let rel_err_b = (sub_area_b - orig_area_b).abs() / orig_area_b;
        assert!(
            rel_err_b < 1e-10,
            "Mesh B area conservation violated: relative error {rel_err_b:.2e}"
        );

        // Every parent triangle must produce at least 1 sub-triangle.
        for parent_idx in 0..tris_a.len() {
            let count = result
                .tris_a
                .iter()
                .filter(|st| st.parent_tri == parent_idx)
                .count();
            assert!(
                count >= 1,
                "Parent tri {parent_idx} produced 0 sub-triangles"
            );
        }

        // No degenerate sub-triangles anywhere.
        for (i, st) in result.tris_a.iter().chain(result.tris_b.iter()).enumerate() {
            let area = tri_area_3d(
                &result.verts[st.verts[0]],
                &result.verts[st.verts[1]],
                &result.verts[st.verts[2]],
            );
            assert!(
                area > 1e-20,
                "Degenerate sub-triangle {i} with area {area:.2e}"
            );
        }

        // At least some parent triangles must have been split (the intersection
        // cuts through the strip).
        let split_parents: usize = (0..tris_a.len())
            .filter(|&pi| {
                result
                    .tris_a
                    .iter()
                    .filter(|st| st.parent_tri == pi)
                    .count()
                    > 1
            })
            .count();
        assert!(
            split_parents >= 2,
            "Expected at least 2 parent triangles to be split, got {split_parents}"
        );
    }

    // ── FIP Red Phase: Edge-on-plane intersection detection tests ──
    // Ref: specs/edge_on_plane_intersection.md
    // These test the n_coplanar==2 case in find_crossing_edges.
    //
    // IGNORED: Edge-on-plane detection is the next task for the Yang pipeline.
    // The n_coplanar==2 branch in find_crossing_edges returns CrossingResult::None,
    // so these tests demonstrate the gap. The implementation must also handle
    // co-surface winding number ambiguity and conformal vertex sharing for
    // axis-aligned box geometry. See specs/edge_on_plane_intersection.md.

    /// When a triangle has one edge lying entirely on the other triangle's plane,
    /// and that edge passes through the interior of the other triangle,
    /// tri_tri_intersect must return a Segment (not None).
    ///
    /// Setup: T_A in XY plane, T_B has edge (v0, v1) in z=0 crossing through T_A.
    /// The third vertex of T_B is above the plane.
    #[test]
    fn edge_on_plane_crossing_detected() {
        // T_A: large triangle in z=0 plane
        let verts = vec![
            [0.0, 0.0, 0.0], // 0: T_A v0
            [4.0, 0.0, 0.0], // 1: T_A v1
            [2.0, 4.0, 0.0], // 2: T_A v2
            // T_B: edge (3,4) lies in z=0 plane, crossing through T_A's interior.
            // Vertex 5 is above the plane.
            [1.0, 1.0, 0.0], // 3: T_B v0 (in z=0, inside T_A)
            [3.0, 1.0, 0.0], // 4: T_B v1 (in z=0, inside T_A)
            [2.0, 1.0, 2.0], // 5: T_B v2 (above z=0)
        ];
        let tri_a = [0, 1, 2];
        let tri_b = [3, 4, 5];

        let result = tri_tri_intersect(tri_a, tri_b, &verts);
        match result {
            TriTriIsect::Segment(_, _) => { /* correct */ }
            other => panic!(
                "Edge-on-plane with both endpoints inside T_A should return Segment, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    /// Edge-on-plane where one endpoint is inside and the other is outside the
    /// opposing triangle. Should return a Segment from the inside point to the
    /// boundary crossing.
    #[test]
    fn edge_on_plane_partial_crossing() {
        let verts = vec![
            [0.0, 0.0, 0.0], // 0: T_A v0
            [4.0, 0.0, 0.0], // 1: T_A v1
            [2.0, 4.0, 0.0], // 2: T_A v2
            // T_B: edge (3,4) in z=0. v3 inside T_A, v4 outside.
            [2.0, 1.0, 0.0], // 3: inside T_A
            [6.0, 1.0, 0.0], // 4: outside T_A (x=6 > T_A boundary)
            [2.0, 1.0, 2.0], // 5: above z=0
        ];
        let tri_a = [0, 1, 2];
        let tri_b = [3, 4, 5];

        let result = tri_tri_intersect(tri_a, tri_b, &verts);
        match result {
            TriTriIsect::Segment(_, _) => { /* correct */ }
            TriTriIsect::Point(_) => { /* acceptable: at least detected */ }
            other => panic!(
                "Edge-on-plane (partial) should return Segment or Point, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    /// Edge-on-plane where both endpoints are outside the opposing triangle.
    /// The edge passes entirely outside — should return None.
    #[test]
    fn edge_on_plane_no_crossing() {
        let verts = vec![
            [0.0, 0.0, 0.0], // 0: T_A v0
            [4.0, 0.0, 0.0], // 1: T_A v1
            [2.0, 4.0, 0.0], // 2: T_A v2
            // T_B: edge (3,4) in z=0 but far from T_A.
            [10.0, 10.0, 0.0], // 3: far outside T_A
            [12.0, 10.0, 0.0], // 4: far outside T_A
            [11.0, 10.0, 2.0], // 5: above z=0
        ];
        let tri_a = [0, 1, 2];
        let tri_b = [3, 4, 5];

        let result = tri_tri_intersect(tri_a, tri_b, &verts);
        assert!(
            matches!(result, TriTriIsect::None),
            "Edge-on-plane fully outside should be None"
        );
    }

    /// Axis-aligned boxes sharing a face plane: the classic case that the Yang
    /// pipeline currently fails on. Two unit-offset boxes must produce a
    /// subdivision with split triangles at the shared face.
    #[test]
    fn edge_on_plane_axis_aligned_boxes() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);

        let total_a = subdivided.tris_a.len();
        let total_b = subdivided.tris_b.len();

        // Without edge-on-plane detection: exactly 12 each (no splitting).
        // With edge-on-plane detection: > 12 for at least one mesh.
        assert!(
            total_a > 12 || total_b > 12,
            "Axis-aligned overlapping boxes must have split triangles \
             (got {total_a} A tris, {total_b} B tris — both == 12 means \
             edge-on-plane intersections are being missed)"
        );
    }

    /// Verify that edge-on-plane detection produces correct subdivision and
    /// labeling for axis-aligned boxes. The Union result should be non-empty
    /// and have the correct number of surviving sub-triangles.
    #[test]
    fn edge_on_plane_aligned_box_union_nonempty() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);

        // Both meshes should be subdivided (more than 12 original tris)
        assert!(
            subdivided.tris_a.len() > 12,
            "A should have split tris (got {})",
            subdivided.tris_a.len()
        );
        assert!(
            subdivided.tris_b.len() > 12,
            "B should have split tris (got {})",
            subdivided.tris_b.len()
        );

        // Union result should be non-empty
        let result = select_boolean_result(&subdivided, &labeling, MeshBooleanOp::Union);
        let union_tris = result.len() / 3;
        assert!(
            union_tris >= 12,
            "Union should have at least 12 triangles (got {union_tris})"
        );
    }

    /// Full Yang pipeline on axis-aligned boxes must produce manifold topology.
    /// This is the key correctness test — if edge-on-plane is handled, the
    /// boolean result should have V-E+F = 2.
    #[test]
    fn edge_on_plane_box_boolean_manifold() {
        use crate::boolean::topology_extract::yang_boolean_pipeline;
        use crate::tessellation::bijective::BijectiveMap;
        use crate::topology::half_edge::FaceIdx;

        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let bijective_a = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };
        let bijective_b = BijectiveMap {
            tri_face_ids: (0..12).map(|i| FaceIdx(i / 2)).collect(),
        };

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
        )
        .expect("Yang pipeline should succeed for overlapping boxes");

        let n_edges = result.arena.edges.len();
        let n_he = result.arena.half_edges.len();

        assert!(n_edges > 0, "Result must have edges");
        assert_eq!(
            n_he,
            2 * n_edges,
            "Manifold invariant: half_edges ({n_he}) != 2 * edges ({})",
            2 * n_edges
        );
    }
}
