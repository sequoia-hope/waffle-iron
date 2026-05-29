//! 3D triangle-triangle intersection classification — the algorithmic
//! payoff of PR-CR1 through PR-CR8's foundations.
//!
//! `triangle_intersects_triangle_3d` is the central algorithm of
//! Cherchi 2022 §3: mesh arrangement processes pairs of triangles via
//! this primitive. Branches on coplanarity (PR-CR7) and dispatches to
//! 6 edge-triangle tests (PR-CR8) in the non-coplanar case.
//!
//! Cherchi 2022 §3 (triangle-triangle intersection; full algorithm).
//! Shewchuk 1997 §2.1 (orient3d as the foundational predicate).
//!
//! The 3-state enum (Disjoint / Intersects / Coplanar) collapses
//! intersection types per YAGNI: callers needing granular info
//! (interior vs edge vs vertex) can probe via direct PR-CR8 calls.
//!
//! **Discovery during implementation**: shared-edge cases return
//! `Intersects` (not `Coplanar` as originally specified) — the
//! algorithm's secondary line-test propagation via vertex coincidence
//! correctly detects the shared edge as a true intersection. The
//! `Coplanar` return is reserved for cases requiring caller's 2D
//! refinement: full coplanar OR edge-in-other-plane without vertex
//! coincidence. See `specs/cherchi_rs_triangle_intersect_3d.md`
//! §"Why Coplanar covers both cases" for the full discussion.

use cad_primitives::Point3;

/// Classification of two 3D triangles' spatial relationship.
///
/// Returned by [`triangle_intersects_triangle_3d`].
///
/// The `Coplanar` variant covers BOTH "full coplanar" (both triangles in
/// the same plane) AND "partial coplanar" (an edge of one triangle lies
/// in the other's plane). Both require the caller to run a 2D handler
/// to refine. See `specs/cherchi_rs_triangle_intersect_3d.md` §"Why
/// Coplanar covers both cases".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TriangleIntersection {
    Disjoint,
    Intersects,
    Coplanar,
}

/// Classify whether two 3D triangles `(a, b, c)` and `(d, e, f)`
/// intersect, are disjoint, or require 2D refinement.
///
/// The central algorithm of Cherchi 2022 §3 — composes
/// [`triangles_are_coplanar`] (PR-CR7) for the coplanarity branch and
/// [`segment_intersects_triangle_3d`] (PR-CR8) for the non-coplanar
/// edge-triangle tests.
///
/// See `specs/cherchi_rs_triangle_intersect_3d.md` for the full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs → undefined. Degenerate (collinear-vertex)
/// triangles → deterministic but may misclassify; caller's
/// responsibility to filter via [`points_are_collinear_3d`].
///
/// [`triangles_are_coplanar`]: super::triangle_pair::triangles_are_coplanar
/// [`segment_intersects_triangle_3d`]: super::segment_triangle::segment_intersects_triangle_3d
/// [`points_are_collinear_3d`]: super::collinearity::points_are_collinear_3d
pub fn triangle_intersects_triangle_3d(
    a: Point3,
    b: Point3,
    c: Point3,
    d: Point3,
    e: Point3,
    f: Point3,
) -> TriangleIntersection {
    use super::segment_triangle::{
        segment_intersects_triangle_3d, SegmentTriangleIntersection as STI,
    };
    use super::triangle_pair::triangles_are_coplanar;
    use TriangleIntersection::*;

    // 1. Full coplanarity branch: PR-CR7.
    if triangles_are_coplanar(a, b, c, d, e, f) {
        return Coplanar;
    }

    // 2. Non-coplanar branch: iterate 6 edge-triangle pairs (PR-CR8).
    let mut any_intersects = false;
    let mut any_coplanar = false;

    let t1_edges: [(Point3, Point3); 3] = [(a, b), (b, c), (c, a)];
    let t2_edges: [(Point3, Point3); 3] = [(d, e), (e, f), (f, d)];

    // Edges of T1 against T2
    for (p, q) in t1_edges {
        match segment_intersects_triangle_3d(p, q, d, e, f) {
            STI::Intersects => any_intersects = true,
            STI::Coplanar => any_coplanar = true,
            STI::Disjoint => {}
        }
    }

    // Edges of T2 against T1
    for (p, q) in t2_edges {
        match segment_intersects_triangle_3d(p, q, a, b, c) {
            STI::Intersects => any_intersects = true,
            STI::Coplanar => any_coplanar = true,
            STI::Disjoint => {}
        }
    }

    // 3. Aggregate per priority: Intersects > Coplanar > Disjoint.
    if any_intersects {
        Intersects
    } else if any_coplanar {
        Coplanar
    } else {
        Disjoint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard unit triangle in XY plane.
    fn xy_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        )
    }

    /// Standard unit triangle in XZ plane (y = 0).
    fn xz_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
        )
    }

    // ── Group 1: Disjoint cases ───────────────────────────────────────

    #[test]
    fn far_apart_triangles_disjoint() {
        let (a, b, c) = xy_triangle();
        // T2 shifted by (100, 100, 100)
        let d = Point3::new(100.0, 100.0, 100.0);
        let e = Point3::new(101.0, 100.0, 100.0);
        let f = Point3::new(100.0, 101.0, 100.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint
        );
    }

    #[test]
    fn parallel_planes_disjoint() {
        let (a, b, c) = xy_triangle();
        // Same XY shape but at z=1
        let d = Point3::new(0.0, 0.0, 1.0);
        let e = Point3::new(1.0, 0.0, 1.0);
        let f = Point3::new(0.0, 1.0, 1.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint
        );
    }

    #[test]
    fn t1_entirely_on_one_side_of_t2_plane_disjoint() {
        // T1 in z=0 plane, T2 in y=2 plane.
        // CRITICAL: T2's vertices are all at z > 0 to ensure no edge of
        // T2 lies in T1's plane (z=0). Otherwise segment-triangle would
        // return Coplanar for that edge, escalating to overall Coplanar.
        // With T2 strictly above z=0, all 6 edge tests return Disjoint.
        let (a, b, c) = xy_triangle();
        let d = Point3::new(0.0, 2.0, 1.0);
        let e = Point3::new(1.0, 2.0, 1.0);
        let f = Point3::new(0.0, 2.0, 2.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Disjoint
        );
    }

    // ── Group 2: Intersects cases ─────────────────────────────────────

    /// T1 in z=0 (XY), T2 perpendicular in y=0.5 plane crossing through
    /// T1's interior.
    #[test]
    fn perpendicular_triangles_crossing_interior() {
        let (a, b, c) = xy_triangle();
        // T2: triangle perpendicular to XY, in y=0.25 plane, spans z=-1..1
        let d = Point3::new(0.25, 0.25, -1.0);
        let e = Point3::new(0.5, 0.25, 1.0);
        let f = Point3::new(0.25, 0.25, 1.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    /// T1 and T2 share vertex `a` with non-coplanar planes; T2 has
    /// edges crossing T1's plane on the other side.
    #[test]
    fn shared_vertex_non_coplanar_with_crossing_edges() {
        let (a, b, c) = xy_triangle();
        // T2 shares vertex a=(0,0,0), points upward, edges cross XY plane
        let d = Point3::new(0.0, 0.0, 0.0); // shared with a
        let e = Point3::new(0.5, 0.5, 1.0);
        let f = Point3::new(0.3, 0.3, -1.0);
        // T2's edge (e, f) crosses XY plane between (0.5,0.5,1) and (0.3,0.3,-1)
        // → the crossing point is (~0.4, ~0.4, 0) which is inside T1's interior
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    /// Edge of T1 crosses T2's interior — vertical X-axis triangle
    /// through unit XY triangle.
    #[test]
    fn edge_of_t1_crosses_t2_interior() {
        // T1 is a vertical triangle that crosses XY plane through (0.25, 0.25, 0)
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        // T2 is unit XY triangle
        let (d, e, f) = xy_triangle();
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    // ── Group 3: Coplanar cases ───────────────────────────────────────

    /// Two distinct triangles, both in z=0 plane → full coplanar.
    #[test]
    fn full_coplanar_two_distinct_triangles() {
        let (a, b, c) = xy_triangle();
        // Another XY-plane triangle, far away
        let d = Point3::new(5.0, 5.0, 0.0);
        let e = Point3::new(6.0, 5.0, 0.0);
        let f = Point3::new(5.0, 6.0, 0.0);
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Coplanar
        );
    }

    /// Same triangle compared to itself → full coplanar.
    #[test]
    fn full_coplanar_identical_triangle() {
        let (a, b, c) = xy_triangle();
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, a, b, c),
            TriangleIntersection::Coplanar
        );
    }

    /// Non-coplanar triangles where an edge of T1 lies in T2's plane
    /// but is far from T2's region — pure `Coplanar` return case.
    ///
    /// Geometry: T2 is the unit XY triangle; T1 has an edge (a, b) on
    /// the z=0 plane (so in T2's plane) at (5,5,0)-(6,5,0) — far from T2.
    /// T1's third vertex c=(5,5,1) is above z=0, so T1 isn't coplanar
    /// with T2.
    ///
    /// Why this isn't the shared-edge case: when triangles share an edge
    /// (vertex coincidence), the line tests on the OTHER edges of T1 fire
    /// via the degenerate vertex coincidence, propagating Intersects.
    /// The geometrically true answer for shared edges IS Intersects.
    /// This test uses the "edge in plane but no vertex coincidence" case
    /// to exercise the pure Coplanar return.
    #[test]
    fn edge_in_other_plane_far_from_triangle_returns_coplanar() {
        let (d, e, f) = xy_triangle(); // T2 = unit XY triangle near origin
        let a = Point3::new(5.0, 5.0, 0.0); // in z=0 plane (T2's), far away
        let b = Point3::new(6.0, 5.0, 0.0); // in z=0 plane, far away
        let c = Point3::new(5.0, 5.0, 1.0); // above plane
                                            // Edge (a, b) lies in T2's plane but is far from T2 itself.
                                            // segment-triangle returns Coplanar for that edge.
                                            // Other T1 edges: one endpoint in T2's plane, other above; line
                                            // tests show line passes far from T2 → Disjoint.
                                            // T2 edges: all in z=0 plane, T1 has vertices on both sides of
                                            // T1's plane (which is y=5), but T2's vertices are all at y<5
                                            // (same side) → Disjoint for all T2 edges.
                                            // Aggregation: any_intersects=false, any_coplanar=true → Coplanar.
                                            // Caller's 2D refinement would correctly identify Disjoint.
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Coplanar
        );
    }

    /// Non-coplanar triangles sharing an edge — returns `Intersects`
    /// via secondary line-test propagation (vertex coincidence causes
    /// degenerate orient3d Zero → all-same-sign branch fires → Intersects
    /// for the touching edge).
    ///
    /// This is GEOMETRICALLY CORRECT: a shared edge IS an intersection
    /// (the triangles share that segment). The Intersects return is the
    /// correct classification; no caller refinement needed.
    #[test]
    fn non_coplanar_shared_edge_returns_intersects() {
        let (a, b, c) = xy_triangle(); // (0,0,0),(1,0,0),(0,1,0)
        let (d, e, f) = xz_triangle(); // (0,0,0),(1,0,0),(0,0,1)
                                       // Triangles share edge (0,0,0)-(1,0,0). The geometric truth is
                                       // Intersects (shared edge = intersection segment).
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    // ── Group 4: Properties ───────────────────────────────────────────

    #[test]
    fn symmetry_under_swap_disjoint() {
        let (a, b, c) = xy_triangle();
        let d = Point3::new(100.0, 100.0, 100.0);
        let e = Point3::new(101.0, 100.0, 100.0);
        let f = Point3::new(100.0, 101.0, 100.0);
        let forward = triangle_intersects_triangle_3d(a, b, c, d, e, f);
        let swapped = triangle_intersects_triangle_3d(d, e, f, a, b, c);
        assert_eq!(forward, swapped);
        assert_eq!(forward, TriangleIntersection::Disjoint);
    }

    #[test]
    fn symmetry_under_swap_intersects() {
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        let (d, e, f) = xy_triangle();
        let forward = triangle_intersects_triangle_3d(a, b, c, d, e, f);
        let swapped = triangle_intersects_triangle_3d(d, e, f, a, b, c);
        assert_eq!(forward, swapped);
        assert_eq!(forward, TriangleIntersection::Intersects);
    }

    #[test]
    fn vertex_permutation_invariance_intersects() {
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        let (d, e, f) = xy_triangle();
        // All 6 permutations of T1's vertices yield Intersects
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(a, c, b, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(b, a, c, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(b, c, a, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(c, a, b, d, e, f),
            TriangleIntersection::Intersects
        );
        assert_eq!(
            triangle_intersects_triangle_3d(c, b, a, d, e, f),
            TriangleIntersection::Intersects
        );
    }

    // ── Group 5: Determinism ──────────────────────────────────────────

    #[test]
    fn deterministic_under_repeated_runs() {
        let a = Point3::new(0.25, 0.25, 1.0);
        let b = Point3::new(0.25, 0.25, -1.0);
        let c = Point3::new(0.5, 0.5, 1.0);
        let (d, e, f) = xy_triangle();
        let first = triangle_intersects_triangle_3d(a, b, c, d, e, f);
        for _ in 0..100 {
            assert_eq!(triangle_intersects_triangle_3d(a, b, c, d, e, f), first);
        }
    }
}
