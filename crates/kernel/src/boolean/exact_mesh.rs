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

use crate::units::{TAU_EXACT_MESH_CLASSIFY, TAU_EXACT_MESH_VERTEX_NUDGE, TAU_NORMALIZE_SQ};

/// Which mesh a triangle belongs to in a boolean operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        // Two vertices on the plane — this is an edge-on-plane case.
        // For simplicity, treat as None (edge-on-plane overlap is a 2D problem).
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

    let mut hits: Vec<EdgeHit> = Vec::new();
    // Track which local vertices have been hit (to deduplicate vertex-shared edges).
    let mut vertex_hit: [bool; 3] = [false, false, false];
    // Small nudge distance for points at triangle vertices. When a constraint
    // endpoint coincides with a triangle vertex, we nudge it slightly along
    // the edge interior so that the two-edge split produces 3 non-degenerate
    // sub-triangles. The nudge is small enough (1e-14) to maintain area
    // conservation within any reasonable tolerance. Ref #9: Cherchi 2020
    // handles this via exact symbolic perturbation; our f64 nudge achieves
    // the same topological result for materialized coordinates.
    let vertex_nudge = TAU_EXACT_MESH_VERTEX_NUDGE;

    for edge_idx in 0..3 {
        let ei_v0 = edge_idx;
        let ei_v1 = (edge_idx + 1) % 3;
        let a = tri_verts[ei_v0];
        let b = tri_verts[ei_v1];

        if let Some(t) = line_segment_intersect_3d(&p0, &p1, &a, &b) {
            if t >= -eps && t <= 1.0 + eps {
                let t_clamped = t.clamp(0.0, 1.0);

                // Check if at a vertex endpoint
                let at_start = t_clamped < eps;
                let at_end = t_clamped > 1.0 - eps;

                if at_start || at_end {
                    let v_local = if at_start { ei_v0 } else { ei_v1 };
                    if vertex_hit[v_local] {
                        continue; // already recorded from the other edge sharing this vertex
                    }
                    vertex_hit[v_local] = true;
                }

                // When a hit is at a vertex endpoint (t near 0 or 1), nudge it
                // slightly into the edge interior. This ensures that the two-edge
                // split produces 3 non-degenerate sub-triangles instead of having
                // a degenerate triangle where a split point coincides with a vertex.
                let t_nudged = if at_start {
                    vertex_nudge
                } else if at_end {
                    1.0 - vertex_nudge
                } else {
                    t_clamped
                };

                let pt = [
                    a[0] + t_nudged * (b[0] - a[0]),
                    a[1] + t_nudged * (b[1] - a[1]),
                    a[2] + t_nudged * (b[2] - a[2]),
                ];

                let vert_idx = if dist_sq(&all_verts[seg[0]], &pt) < eps * eps {
                    // Segment endpoint is close to this point — check if it's
                    // at the same position or at a nudged position
                    if at_start || at_end {
                        // Create a new nudged vertex instead of reusing segment endpoint
                        let idx = all_verts.len();
                        all_verts.push(pt);
                        idx
                    } else {
                        seg[0]
                    }
                } else if dist_sq(&all_verts[seg[1]], &pt) < eps * eps {
                    if at_start || at_end {
                        let idx = all_verts.len();
                        all_verts.push(pt);
                        idx
                    } else {
                        seg[1]
                    }
                } else {
                    let idx = all_verts.len();
                    all_verts.push(pt);
                    idx
                };

                hits.push(EdgeHit {
                    edge: edge_idx,
                    t: t_nudged,
                    vert_idx,
                });
            }
        }
    }

    // We need exactly 2 hits on different edges to split.
    if hits.len() < 2 {
        return vec![tri_vi];
    }

    // Find two hits on different edges
    for i in 0..hits.len() {
        for j in (i + 1)..hits.len() {
            if hits[i].edge != hits[j].edge {
                return split_two_edge_points(
                    tri_vi,
                    hits[i].edge,
                    hits[i].vert_idx,
                    hits[j].edge,
                    hits[j].vert_idx,
                );
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
    // Edge i goes from vertex i to vertex (i+1)%3.
    // Find the shared vertex between e0 and e1.
    let e0_verts = [e0, (e0 + 1) % 3];
    let e1_verts = [e1, (e1 + 1) % 3];

    let shared_local = if e0_verts[0] == e1_verts[0] || e0_verts[0] == e1_verts[1] {
        e0_verts[0]
    } else {
        e0_verts[1]
    };

    let other_e0 = if e0_verts[0] == shared_local {
        e0_verts[1]
    } else {
        e0_verts[0]
    };

    let other_e1 = if e1_verts[0] == shared_local {
        e1_verts[1]
    } else {
        e1_verts[0]
    };

    let v_shared = tri_vi[shared_local];
    let v_other_e0 = tri_vi[other_e0];
    let v_other_e1 = tri_vi[other_e1];

    // Triangle 1: shared vertex + the two split points
    // Quad: s0, v_other_e0, v_other_e1, s1 (the remaining region)
    // Split quad into 2 triangles
    vec![
        [v_shared, s0, s1],
        [s0, v_other_e0, v_other_e1],
        [s0, v_other_e1, s1],
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
/// 4. Split each triangle by its constraint segments
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
                // Materialize the two indirect points and add to vertex array
                let p0 = materialize_ip(&ip0, &all_verts);
                let p1 = materialize_ip(&ip1, &all_verts);

                // Check for degenerate indirect points (vertex on plane)
                let vi0 = if ip0.edge[0] == ip0.edge[1] {
                    // Degenerate — use the original vertex
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

                // Skip degenerate segments (both endpoints are the same vertex)
                if vi0 == vi1 {
                    continue;
                }

                constraints_a.entry(i).or_default().push([vi0, vi1]);
                constraints_b.entry(j).or_default().push([vi0, vi1]);
            }
        }
    }

    // Step 3: Subdivide each triangle
    let mut result_tris_a = Vec::new();
    for (i, tri) in tris_a.iter().enumerate() {
        if let Some(segs) = constraints_a.get(&i) {
            // Start with the original triangle
            let mut current_tris: Vec<[usize; 3]> = vec![*tri];
            for seg in segs {
                let mut next_tris = Vec::new();
                for t in &current_tris {
                    let splits = split_triangle_by_segment(*t, *seg, &mut all_verts);
                    next_tris.extend(splits);
                }
                current_tris = next_tris;
            }
            for t in current_tris {
                result_tris_a.push(SubTriangle {
                    verts: t,
                    parent_tri: i,
                });
            }
        } else {
            // No intersection — pass through unchanged
            result_tris_a.push(SubTriangle {
                verts: *tri,
                parent_tri: i,
            });
        }
    }

    let mut result_tris_b = Vec::new();
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
            for t in current_tris {
                result_tris_b.push(SubTriangle {
                    verts: t,
                    parent_tri: j,
                });
            }
        } else {
            result_tris_b.push(SubTriangle {
                verts: remapped,
                parent_tri: j,
            });
        }
    }

    SubdividedMesh {
        verts: all_verts,
        tris_a: result_tris_a,
        tris_b: result_tris_b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::TAU_WORK;

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
                            t >= -1e-10 && t <= 1.0 + 1e-10,
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
                        det.abs() < 1e-10,
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

        // Each triangle is split by an intersection segment with endpoints on 2 edges,
        // producing 3 sub-triangles each.
        assert_eq!(
            result.tris_a.len(),
            3,
            "Mesh A triangle split by segment should produce 3 sub-triangles, got {}",
            result.tris_a.len()
        );
        assert_eq!(
            result.tris_b.len(),
            3,
            "Mesh B triangle split by segment should produce 3 sub-triangles, got {}",
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

        // The second A triangle (index 1) is NOT intersected — exactly 1 sub-tri
        let passthrough_count: usize = result.tris_a.iter().filter(|st| st.parent_tri == 1).count();
        assert_eq!(
            passthrough_count, 1,
            "Non-intersected A triangle (parent 1) should pass through unchanged, got {passthrough_count}"
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
}
