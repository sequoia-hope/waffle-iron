//! 3D point-in-triangle classification via cinolib's robust
//! all-three-projections approach.
//!
//! Ported from cinolib's `point_in_triangle_3d` (`predicates.cpp:447-481`
//! per audit B-07). cinolib is MIT-licensed.
//! © Marco Livesu et al. — https://github.com/mlivesu/cinolib
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2022 §3 (point-in-triangle as primitive for triangle-triangle
//! intersection).
//!
//! **Deliberate deviation from cinolib (simplification)**: cinolib's
//! function returns granular boundary info (which edge / vertex was hit).
//! Our `PointLocation` enum collapses these to `OnBoundary` because no
//! current cherchi-rs caller needs the granular info (YAGNI). Easy to
//! expand later if a future port needs it. See
//! `specs/cherchi_rs_point_in_triangle.md` §"Deliberate deviation from
//! cinolib".
//!
//! **B-07 correctness improvement**: legacy Rust port (in old kernel)
//! tested only the dominant-axis projection, misclassifying non-coplanar
//! points projected over the triangle interior as `StrictlyInside`.
//! cinolib variant tests ALL THREE projections and AND-combines (skipping
//! degenerate ones), which catches this case for tilted triangles where
//! multiple projections are non-degenerate.
//!
//! **Skip-degenerate refinement**: for axis-aligned triangles, 2 of 3
//! cardinal projections are degenerate (collinear). Without skipping
//! degenerate projections, an interior coplanar point would never
//! return `StrictlyInside`. The skip-degenerate logic matches cinolib's
//! intent (the strict AND-combine only applies to projections that
//! carry discrimination info).

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
pub fn point_in_triangle_3d(p: Point3, a: Point3, b: Point3, c: Point3) -> PointLocation {
    // Project to each of the 3 cardinal 2D planes (axis-drop projections).
    let drop_z = |q: Point3| [q.x(), q.y()];
    let drop_y = |q: Point3| [q.x(), q.z()];
    let drop_x = |q: Point3| [q.y(), q.z()];

    let projections = [
        (drop_z(p), drop_z(a), drop_z(b), drop_z(c)),
        (drop_y(p), drop_y(a), drop_y(b), drop_y(c)),
        (drop_x(p), drop_x(a), drop_x(b), drop_x(c)),
    ];

    // Combine per cinolib semantics:
    //   - Skip projections where the 2D triangle is degenerate (axis-aligned
    //     triangles are degenerate in 2 of 3 projections; those don't carry
    //     discrimination info).
    //   - Among non-degenerate projections:
    //       ANY StrictlyOutside  → StrictlyOutside
    //       ALL StrictlyInside   → StrictlyInside
    //       else                 → OnBoundary
    //   - If ALL projections are degenerate, the 3D triangle itself is
    //     degenerate (collinear); return StrictlyOutside deterministically.
    let mut any_outside = false;
    let mut all_inside = true;
    let mut any_non_degenerate = false;
    for (p2, a2, b2, c2) in projections {
        // 2× signed triangle area; 0.0 iff a2/b2/c2 are collinear in 2D.
        let area2 = geometry_predicates::orient2d(a2, b2, c2);
        if area2 == 0.0 {
            continue;
        }
        any_non_degenerate = true;
        let loc = point_in_triangle_2d(p2, a2, b2, c2);
        if loc == PointLocation::StrictlyOutside {
            any_outside = true;
        }
        if loc != PointLocation::StrictlyInside {
            all_inside = false;
        }
    }

    if any_outside {
        return PointLocation::StrictlyOutside;
    }
    if !any_non_degenerate {
        return PointLocation::StrictlyOutside;
    }
    if all_inside {
        return PointLocation::StrictlyInside;
    }
    PointLocation::OnBoundary
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
    p: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
) -> PointLocation {
    // Three orient2d sign tests using Shewchuk's exact predicate.
    let s_ab = geometry_predicates::orient2d(a, b, p);
    let s_bc = geometry_predicates::orient2d(b, c, p);
    let s_ca = geometry_predicates::orient2d(c, a, p);

    let any_zero = s_ab == 0.0 || s_bc == 0.0 || s_ca == 0.0;
    let any_pos = s_ab > 0.0 || s_bc > 0.0 || s_ca > 0.0;
    let any_neg = s_ab < 0.0 || s_bc < 0.0 || s_ca < 0.0;

    if any_zero {
        // On an edge or vertex — UNLESS mixed non-zero signs coexist,
        // which means the point is on the extension of an edge (outside).
        if any_pos && any_neg {
            return PointLocation::StrictlyOutside;
        }
        return PointLocation::OnBoundary;
    }

    // All three signs are non-zero. Strictly inside iff all same sign.
    if any_pos && any_neg {
        PointLocation::StrictlyOutside
    } else {
        PointLocation::StrictlyInside
    }
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
        assert_eq!(
            point_in_triangle_3d(p, a, b, c),
            PointLocation::StrictlyInside
        );
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
        assert_eq!(
            point_in_triangle_3d(p, a, b, c),
            PointLocation::StrictlyOutside
        );
    }

    #[test]
    fn far_away_outside() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(10.0, 10.0, 10.0);
        assert_eq!(
            point_in_triangle_3d(p, a, b, c),
            PointLocation::StrictlyOutside
        );
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
        assert_eq!(
            point_in_triangle_3d(p, a, b, c),
            PointLocation::StrictlyInside
        );
    }

    #[test]
    fn coplanar_outside_tilted() {
        let (a, b, c) = tilted_triangle();
        // (1, 1, -1) is on plane x+y+z=1 but far outside the simplex.
        let p = Point3::new(1.0, 1.0, -1.0);
        assert_eq!(
            point_in_triangle_3d(p, a, b, c),
            PointLocation::StrictlyOutside
        );
    }

    // ── Group 3: B-07 regression — non-coplanar point over interior ──

    /// Audit B-07: legacy port projects only to the dominant-axis plane,
    /// missing that non-coplanar points may still project to triangle
    /// interior in that plane. cinolib variant catches this by testing
    /// all 3 projections — the off-plane point fails at least one
    /// projection's interior test.
    ///
    /// **Why a tilted triangle**: for axis-aligned triangles, 2 of 3
    /// projections are degenerate (collinear), so they're skipped per
    /// cinolib semantics. Both the dominant-axis-only legacy and the
    /// cinolib variant give the same answer on axis-aligned triangles.
    /// The cinolib robustness ONLY differentiates for triangles where
    /// multiple projections are non-degenerate.
    ///
    /// Construction:
    /// - Triangle in plane `z = 0.3·x + 0.7·y` (all 3 cardinal projections
    ///   non-degenerate; normal = (-0.3, -0.7, 1.0))
    /// - Point `(0.25, 0.25, 0.5)`. On-plane z = 0.3·0.25 + 0.7·0.25 = 0.25;
    ///   actual z = 0.5, so the point is OFF the plane.
    /// - XY projection: triangle (0,0)(1,0)(0,1), point (0.25, 0.25) → inside
    /// - XZ projection: triangle (0,0)(1,0.3)(0,0.7), point (0.25, 0.5) → inside
    /// - YZ projection: triangle (0,0)(0,0.3)(1,0.7), point (0.25, 0.5)
    ///   → MIXED orient2d signs → StrictlyOutside (catches the off-plane!)
    /// - Combined: any_outside → StrictlyOutside ✓
    ///
    /// Legacy dominant-axis-only would have picked XY (dominant axis Z)
    /// and returned StrictlyInside — wrong.
    #[test]
    fn b07_regression_non_coplanar_over_interior() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.3);
        let c = Point3::new(0.0, 1.0, 0.7);
        let p = Point3::new(0.25, 0.25, 0.5); // off the z=0.3x+0.7y plane
        let result = point_in_triangle_3d(p, a, b, c);
        // Load-bearing: cinolib variant must NOT return StrictlyInside.
        // (Specific return depends on combined projection results; here YZ
        // gives StrictlyOutside which dominates → StrictlyOutside.)
        assert_ne!(
            result,
            PointLocation::StrictlyInside,
            "B-07: non-coplanar point over tilted triangle must NOT classify as StrictlyInside"
        );
    }

    // ── Group 4: Properties ───────────────────────────────────────────

    #[test]
    fn permutation_invariance_interior() {
        let (a, b, c) = xy_triangle();
        let p = Point3::new(0.25, 0.25, 0.0);
        // All 6 permutations of (a, b, c) yield StrictlyInside
        assert_eq!(
            point_in_triangle_3d(p, a, b, c),
            PointLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d(p, a, c, b),
            PointLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d(p, b, a, c),
            PointLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d(p, b, c, a),
            PointLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d(p, c, a, b),
            PointLocation::StrictlyInside
        );
        assert_eq!(
            point_in_triangle_3d(p, c, b, a),
            PointLocation::StrictlyInside
        );
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
        let result = point_in_triangle_2d([0.25, 0.25], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
        assert_eq!(result, PointLocation::StrictlyInside);
    }

    #[test]
    fn primitive_2d_interior_cw_winding() {
        // CW winding — same classification (sign convention agnostic)
        let result = point_in_triangle_2d([0.25, 0.25], [0.0, 0.0], [0.0, 1.0], [1.0, 0.0]);
        assert_eq!(result, PointLocation::StrictlyInside);
    }

    #[test]
    fn primitive_2d_vertex_and_edge() {
        // Vertex of triangle
        let r_v = point_in_triangle_2d([0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
        assert_eq!(r_v, PointLocation::OnBoundary);

        // Edge midpoint
        let r_e = point_in_triangle_2d([0.5, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
        assert_eq!(r_e, PointLocation::OnBoundary);
    }

    #[test]
    fn primitive_2d_outside() {
        // Far outside
        let result = point_in_triangle_2d([10.0, 10.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
        assert_eq!(result, PointLocation::StrictlyOutside);
    }

    #[test]
    fn primitive_2d_on_edge_extension() {
        // Point on the LINE through (0,0)-(1,0) but beyond the edge
        // (i.e., at x=2, y=0). orient2d on (a,b,p) = 0 (collinear);
        // but the other two orient2ds have mixed signs → StrictlyOutside.
        let result = point_in_triangle_2d([2.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
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
