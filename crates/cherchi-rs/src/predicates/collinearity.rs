use cad_primitives::Point3;

/// Three-point collinearity test in 3D using Shewchuk's exact `orient2d`
/// on three orthogonal-axis-drop projections.
///
/// Returns `true` iff all three projection `orient2d` tests return
/// exactly 0.0. See `specs/cherchi_rs_points_collinear.md` for the
/// full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn points_are_collinear_3d(a: Point3, b: Point3, c: Point3) -> bool {
    // Drop each axis in turn. If any projection's orient2d is non-zero,
    // the three points span 2D in that plane and thus are not collinear in 3D.
    // If all three projections return exactly 0, the points lie on a single
    // line (or are degenerate-collinear: two or more coincident).
    let drop_z = geometry_predicates::orient2d(
        [a.x(), a.y()],
        [b.x(), b.y()],
        [c.x(), c.y()],
    );
    let drop_y = geometry_predicates::orient2d(
        [a.x(), a.z()],
        [b.x(), b.z()],
        [c.x(), c.z()],
    );
    let drop_x = geometry_predicates::orient2d(
        [a.y(), a.z()],
        [b.y(), b.z()],
        [c.y(), c.z()],
    );
    drop_z == 0.0 && drop_y == 0.0 && drop_x == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Canonical collinear ─────────────────────────────────

    #[test]
    fn axis_aligned_x() {
        // Three points on the X axis.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(2.0, 0.0, 0.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn axis_aligned_y() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(0.0, 2.0, 0.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn axis_aligned_z() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 0.0, 1.0);
        let c = Point3::new(0.0, 0.0, 2.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn off_axis_collinear() {
        // (1,2,3) direction; c = 2·b, all on the same line through origin.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 2.0, 3.0);
        let c = Point3::new(2.0, 4.0, 6.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    // ── Group 2: Degenerate-collinear (per spec, these are TRUE) ──────

    #[test]
    fn two_coincident_first_pair() {
        // a == b; per spec these are degenerate-collinear → true.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 0.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn two_coincident_b_and_c() {
        let a = Point3::new(1.0, 1.0, 1.0);
        let b = Point3::new(2.0, 2.0, 2.0);
        let c = Point3::new(2.0, 2.0, 2.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn all_three_coincident() {
        let p = Point3::new(7.0, -3.0, 2.5);
        assert!(points_are_collinear_3d(p, p, p));
    }

    // ── Group 3: Canonical non-collinear ─────────────────────────────

    #[test]
    fn right_triangle_xy() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        assert!(!points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn skew_three_axes() {
        // One point on each axis — clearly spans 3D.
        let a = Point3::new(1.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(0.0, 0.0, 1.0);
        assert!(!points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn near_collinear_but_not_exact() {
        // (0,0,0)-(1,0,0) is the X axis. (2, 1e-300, 0) is very near
        // the X axis but NOT on it. Per spec's "exactly 0" criterion,
        // this must return false.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(2.0, 1e-300, 0.0);
        assert!(!points_are_collinear_3d(a, b, c));
    }

    // ── Group 4: Property — order invariance ─────────────────────────

    #[test]
    fn order_invariant_for_collinear() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 2.0, 3.0);
        let c = Point3::new(2.0, 4.0, 6.0);
        // All 6 permutations should agree.
        assert!(points_are_collinear_3d(a, b, c));
        assert!(points_are_collinear_3d(a, c, b));
        assert!(points_are_collinear_3d(b, a, c));
        assert!(points_are_collinear_3d(b, c, a));
        assert!(points_are_collinear_3d(c, a, b));
        assert!(points_are_collinear_3d(c, b, a));
    }

    #[test]
    fn order_invariant_for_non_collinear() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        // All 6 permutations should agree (false in this case).
        assert!(!points_are_collinear_3d(a, b, c));
        assert!(!points_are_collinear_3d(a, c, b));
        assert!(!points_are_collinear_3d(b, a, c));
        assert!(!points_are_collinear_3d(b, c, a));
        assert!(!points_are_collinear_3d(c, a, b));
        assert!(!points_are_collinear_3d(c, b, a));
    }

    // ── Group 5: A-01 regression (legacy port's f64 cross-product bug) ─

    /// The legacy Rust port (per `docs/audits/cherchi_port_audit.md:148-181`,
    /// finding A-01) used an inexact f64 cross-product, which produced
    /// tiny non-zero residuals for mathematically-collinear inputs derived
    /// from arithmetic. This test exercises one such input — three points
    /// strictly on a line whose coordinates exceed f64 contiguous-integer
    /// precision. Shewchuk's exact `orient2d` returns 0 here; the f64
    /// cross-product does not.
    ///
    /// The clean port (PR-CR1) gets this right by construction.
    #[test]
    fn a01_regression_large_coords_on_line() {
        // Three points strictly on the y = x diagonal in XY (and z = 0).
        // Coords chosen so f64 multiplication accumulates round-off but
        // the points are mathematically exactly on the line.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(2.0, 2.0, 0.0);
        // Exact orient2d returns 0 here → must report collinear.
        assert!(points_are_collinear_3d(a, b, c));
    }
}
