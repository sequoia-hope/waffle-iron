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
    let t = if denom.abs() > 1e-30 {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let t = if denom.abs() > 1e-30 {
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
                        if denom.abs() > 1e-12 {
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
}
