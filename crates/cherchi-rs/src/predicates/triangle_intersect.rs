// Function body is `unimplemented!()` during the RED phase (Test Author
// commit). The Implementer commit replaces the body. The per-file MIT
// attribution header lands in a separate commit after GREEN per PR-CR9
// sequencing.

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
    _a: Point3,
    _b: Point3,
    _c: Point3,
    _d: Point3,
    _e: Point3,
    _f: Point3,
) -> TriangleIntersection {
    unimplemented!("PR-CR9 RED phase — Implementer fills body in next commit")
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
        // T1 in z=0 plane, T2 in y=2 plane (parallel-ish, perpendicular triangles)
        let (a, b, c) = xy_triangle();
        let d = Point3::new(0.0, 2.0, 0.0);
        let e = Point3::new(1.0, 2.0, 0.0);
        let f = Point3::new(0.0, 2.0, 1.0);
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

    /// Non-coplanar triangles sharing an edge — returns `Coplanar` per
    /// the spec's §"Why Coplanar covers both cases" (caller's 2D handler
    /// refines to Intersects).
    #[test]
    fn non_coplanar_shared_edge_returns_coplanar() {
        // T1 in XY plane, T2 in XZ plane; both contain the edge
        // (0,0,0)-(1,0,0) on the x-axis.
        let (a, b, c) = xy_triangle(); // (0,0,0),(1,0,0),(0,1,0)
        let (d, e, f) = xz_triangle(); // (0,0,0),(1,0,0),(0,0,1)
        // The shared edge (0,0,0)-(1,0,0) lies in both planes.
        // segment_intersects_triangle_3d returns Coplanar for that edge
        // (both endpoints on the other's plane).
        // No edge returns Intersects, so result is Coplanar.
        assert_eq!(
            triangle_intersects_triangle_3d(a, b, c, d, e, f),
            TriangleIntersection::Coplanar
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
