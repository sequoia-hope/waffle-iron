// Function bodies are `unimplemented!()` during the RED phase (Test Author
// commit). The Implementer commit replaces the bodies. The per-file MIT
// attribution + "Deliberate deviation from cinolib" + "B-07 correctness
// improvement" headers land in a separate commit after GREEN per PR-CR5
// sequencing.

use cad_primitives::Point3;

/// Classification of a point's location relative to a triangle.
///
/// Returned by [`point_in_triangle_3d`] and [`point_in_triangle_2d`].
///
/// **Deliberate simplification from cinolib**: cinolib distinguishes which
/// edge / vertex was hit. We collapse to `OnBoundary` (YAGNI). See
/// `specs/cherchi_rs_point_in_triangle.md` §"Deliberate deviation from cinolib".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PointLocation {
    StrictlyInside,
    OnBoundary,
    StrictlyOutside,
}

/// Classify a 3D point's location relative to a 3D triangle using
/// cinolib's robust all-three-projections approach.
///
/// Tests `(p, a, b, c)` in each of the 3 cardinal 2D projections
/// (XY, XZ, YZ) and combines results:
/// - ANY projection `StrictlyOutside` → `StrictlyOutside`
/// - ALL projections `StrictlyInside` → `StrictlyInside`
/// - Otherwise → `OnBoundary`
///
/// Non-coplanar points typically fail at least one projection's
/// strictly-inside test (the cinolib robustness property — catches
/// what dominant-axis-only would miss).
///
/// See `specs/cherchi_rs_point_in_triangle.md` for the full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Degenerate
/// (collinear) triangles produce deterministic but unspecified
/// results — caller's responsibility to filter.
pub fn point_in_triangle_3d(
    _p: Point3,
    _a: Point3,
    _b: Point3,
    _c: Point3,
) -> PointLocation {
    unimplemented!("PR-CR5 RED phase — Implementer fills body in next commit")
}

/// 2D point-in-triangle primitive using Shewchuk's exact `orient2d`.
///
/// Used by [`point_in_triangle_3d`] in each cardinal projection;
/// also exposed for direct testing.
///
/// Sign-based classification:
/// - All 3 `orient2d` results same non-zero sign → `StrictlyInside`
///   (handles both CCW and CW triangles)
/// - Some zero with no mixed signs → `OnBoundary` (vertex or edge)
/// - Some zero with mixed signs → `StrictlyOutside` (on edge extension)
/// - All non-zero with mixed signs → `StrictlyOutside`
pub(crate) fn point_in_triangle_2d(
    _p: [f64; 2],
    _a: [f64; 2],
    _b: [f64; 2],
    _c: [f64; 2],
) -> PointLocation {
    unimplemented!("PR-CR5 RED phase — Implementer fills body in next commit")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: 3D classification on axis-aligned XY-plane triangle ──

    fn xy_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        )
    }

    #[test]
    fn interior_coplanar_xy() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.0);
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::StrictlyInside);
    }

    #[test]
    fn vertex_hit_a() {
        let (a, b, c) = xy_triangle();
        assert_eq!(point_in_triangle_3d(a, a, b, c), PointLocation::OnBoundary);
    }

    #[test]
    fn edge_midpoint_ab() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.5, 0.0, 0.0);
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::OnBoundary);
    }

    #[test]
    fn coplanar_but_outside_xy() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(2.0, 0.0, 0.0);
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::StrictlyOutside);
    }

    #[test]
    fn far_away_outside() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(10.0, 10.0, 10.0);
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::StrictlyOutside);
    }

    // ── Group 2: Non-axis-aligned triangle ────────────────────────────

    /// Triangle in plane x + y + z = 1 with vertices at the axis intercepts.
    fn tilted_triangle() -> (Point3, Point3, Point3) {
        (
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(0.0, 0.0, 1.0),
        )
    }

    #[test]
    fn interior_coplanar_tilted() {
        let (a, b, c) = tilted_triangle();
        // Centroid: (1/3, 1/3, 1/3). Sums to 1.0 → coplanar.
        let p = Point3::new(1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::StrictlyInside);
    }

    #[test]
    fn coplanar_outside_tilted() {
        let (a, b, c) = tilted_triangle();
        // (1, 1, -1) is on plane x+y+z=1 but far outside the simplex.
        let p = Point3::new(1.0, 1.0, -1.0);
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::StrictlyOutside);
    }

    // ── Group 3: B-07 regression — non-coplanar point over interior ──

    /// Audit B-07: legacy port projects only to the dominant-axis plane,
    /// missing that non-coplanar points may still project to triangle
    /// interior in that plane. cinolib variant catches this by testing
    /// all 3 projections.
    ///
    /// Construction: triangle in XY plane, point at z != 0 that projects
    /// to triangle interior in XY. Result must NOT be StrictlyInside.
    #[test]
    fn b07_regression_non_coplanar_over_interior() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.5); // projects to (0.25, 0.25) in XY
        let result = point_in_triangle_3d(p, a, b, c);
        // The exact return is OnBoundary or StrictlyOutside depending on
        // how degenerate 2D triangles are classified. Either is acceptable;
        // load-bearing assertion is "NOT StrictlyInside".
        assert_ne!(
            result,
            PointLocation::StrictlyInside,
            "B-07: non-coplanar point over interior must NOT classify as StrictlyInside"
        );
    }

    // ── Group 4: Properties ───────────────────────────────────────────

    #[test]
    fn permutation_invariance_interior() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.0);
        // All 6 permutations of (a, b, c) yield StrictlyInside
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::StrictlyInside);
        assert_eq!(point_in_triangle_3d(p, a, c, b), PointLocation::StrictlyInside);
        assert_eq!(point_in_triangle_3d(p, b, a, c), PointLocation::StrictlyInside);
        assert_eq!(point_in_triangle_3d(p, b, c, a), PointLocation::StrictlyInside);
        assert_eq!(point_in_triangle_3d(p, c, a, b), PointLocation::StrictlyInside);
        assert_eq!(point_in_triangle_3d(p, c, b, a), PointLocation::StrictlyInside);
    }

    #[test]
    fn permutation_invariance_boundary() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.5, 0.0, 0.0); // edge midpoint
        assert_eq!(point_in_triangle_3d(p, a, b, c), PointLocation::OnBoundary);
        assert_eq!(point_in_triangle_3d(p, a, c, b), PointLocation::OnBoundary);
        assert_eq!(point_in_triangle_3d(p, b, a, c), PointLocation::OnBoundary);
        assert_eq!(point_in_triangle_3d(p, b, c, a), PointLocation::OnBoundary);
        assert_eq!(point_in_triangle_3d(p, c, a, b), PointLocation::OnBoundary);
        assert_eq!(point_in_triangle_3d(p, c, b, a), PointLocation::OnBoundary);
    }

    // ── Group 5: 2D primitive direct testing ──────────────────────────

    #[test]
    fn primitive_2d_interior() {
        // Unit triangle (CCW): (0,0), (1,0), (0,1); point (0.25, 0.25) inside
        let result = point_in_triangle_2d(
            [0.25, 0.25],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
        );
        assert_eq!(result, PointLocation::StrictlyInside);
    }

    #[test]
    fn primitive_2d_interior_cw_winding() {
        // CW winding — same classification (sign convention agnostic)
        let result = point_in_triangle_2d(
            [0.25, 0.25],
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 0.0],
        );
        assert_eq!(result, PointLocation::StrictlyInside);
    }

    #[test]
    fn primitive_2d_vertex_and_edge() {
        // Vertex of triangle
        let r_v = point_in_triangle_2d(
            [0.0, 0.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
        );
        assert_eq!(r_v, PointLocation::OnBoundary);

        // Edge midpoint
        let r_e = point_in_triangle_2d(
            [0.5, 0.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
        );
        assert_eq!(r_e, PointLocation::OnBoundary);
    }

    #[test]
    fn primitive_2d_outside() {
        // Far outside
        let result = point_in_triangle_2d(
            [10.0, 10.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
        );
        assert_eq!(result, PointLocation::StrictlyOutside);
    }

    #[test]
    fn primitive_2d_on_edge_extension() {
        // Point on the LINE through (0,0)-(1,0) but beyond the edge
        // (i.e., at x=2, y=0). orient2d on (a,b,p) = 0 (collinear);
        // but the other two orient2ds have mixed signs → StrictlyOutside.
        let result = point_in_triangle_2d(
            [2.0, 0.0],
            [0.0, 0.0],
            [1.0, 0.0],
            [0.0, 1.0],
        );
        assert_eq!(result, PointLocation::StrictlyOutside);
    }

    // ── Group 6: Determinism ──────────────────────────────────────────

    #[test]
    fn deterministic_under_repeated_runs() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.5);
        let first = point_in_triangle_3d(p, a, b, c);
        for _ in 0..100 {
            assert_eq!(point_in_triangle_3d(p, a, b, c), first);
        }
    }
}
