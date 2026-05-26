//! Foundational `Sign` enum + `Point3`-typed orientation predicates.
//!
//! `orient3d` wraps Shewchuk's adaptive predicate from the
//! `geometry-predicates` crate (MIT-licensed; itself a Shewchuk port)
//! and converts the f64 result to a 3-valued `Sign`.
//!
//! Shewchuk 1997 §2.1 (adaptive orient3d).
//! Cherchi 2020 §3 (predicates as foundation for arrangement).
//!
//! **Sign convention note**: per Shewchuk's `orient3d`, positive means
//! `d` lies BELOW the plane through `(a, b, c)` where `(a, b, c)`
//! appears CCW viewed from above (counter-intuitive — "below" gives
//! positive, NOT above). Cherchi 2020 and downstream consumers expect
//! this convention. See `specs/cherchi_rs_orient3d_sign.md` for details.
//!
//! No deviation from upstream behavior — this is a type-shape wrapper.

use cad_primitives::{Point2, Point3};

/// Foundational sign classification for predicate results.
///
/// Returned by [`orient3d`] (and future orient2d / indirect-predicate
/// wrappers). Subsequent cherchi-rs predicates use `Sign` instead of
/// ad-hoc `f64` sign comparisons.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Sign {
    Negative,
    Zero,
    Positive,
}

impl Sign {
    /// Classify an `f64` value by its sign. Total function: defined for
    /// every input including NaN, ±0.0, and ±infinity.
    ///
    /// - `x > 0.0` → `Positive`
    /// - `x < 0.0` → `Negative`
    /// - `x == 0.0` (including `-0.0`) → `Zero`
    /// - `x.is_nan()` → `Zero` (totality fall-through; NaN inputs are
    ///   undefined per project convention but classifying as Zero avoids
    ///   panics in production paths)
    pub fn from_f64(x: f64) -> Sign {
        if x > 0.0 {
            Sign::Positive
        } else if x < 0.0 {
            Sign::Negative
        } else {
            // x == 0.0 (including -0.0) OR x.is_nan() — NaN compares
            // false against both > 0.0 and < 0.0, so it falls through here.
            Sign::Zero
        }
    }
}

/// 3D orientation predicate: returns the sign of the orientation
/// determinant of `(a, b, c, d)`, per Shewchuk's convention.
///
/// - `Sign::Positive` — `d` lies BELOW the plane through `(a, b, c)`
///   where `(a, b, c)` appears CCW viewed from above. (Counter-
///   intuitive: "below" gives positive, NOT above.)
/// - `Sign::Negative` — `d` lies ABOVE the plane (same CCW viewpoint).
/// - `Sign::Zero` — all 4 points are coplanar.
///
/// Wraps [`geometry_predicates::orient3d`] (Shewchuk-style adaptive
/// precision). See `specs/cherchi_rs_orient3d_sign.md` for the full
/// contract, including the sign-convention note (Cherchi 2020 and
/// downstream consumers expect Shewchuk's convention).
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn orient3d(a: Point3, b: Point3, c: Point3, d: Point3) -> Sign {
    let det = geometry_predicates::orient3d(
        a.as_array(),
        b.as_array(),
        c.as_array(),
        d.as_array(),
    );
    Sign::from_f64(det)
}

/// 2D orientation predicate: returns the sign of the orientation
/// determinant of `(a, b, c)`.
///
/// - `Sign::Positive` — `(a, b, c)` is in CCW order (c is to the LEFT
///   of the directed line a→b).
/// - `Sign::Negative` — `(a, b, c)` is in CW order (c is to the RIGHT).
/// - `Sign::Zero` — `a, b, c` are collinear.
///
/// Wraps [`geometry_predicates::orient2d`] (Shewchuk-style adaptive
/// precision). Uses the natural geometric convention — NO sign
/// inversion vs `orient3d` (which uses Shewchuk's "d below the CCW
/// plane → Positive" convention). See `specs/cherchi_rs_orient2d_sign.md`.
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn orient2d(a: Point2, b: Point2, c: Point2) -> Sign {
    let det = geometry_predicates::orient2d(
        a.as_array(),
        b.as_array(),
        c.as_array(),
    );
    Sign::from_f64(det)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Sign::from_f64 truth table ───────────────────────────

    #[test]
    fn sign_from_f64_positive_one() {
        assert_eq!(Sign::from_f64(1.0), Sign::Positive);
    }

    #[test]
    fn sign_from_f64_negative_one() {
        assert_eq!(Sign::from_f64(-1.0), Sign::Negative);
    }

    #[test]
    fn sign_from_f64_zero() {
        assert_eq!(Sign::from_f64(0.0), Sign::Zero);
    }

    #[test]
    fn sign_from_f64_negative_zero() {
        // IEEE 754: -0.0 == 0.0 in comparisons, so -0.0 falls into the
        // Zero branch.
        assert_eq!(Sign::from_f64(-0.0), Sign::Zero);
    }

    #[test]
    fn sign_from_f64_epsilon() {
        // Smallest positive normal f64 → Positive
        assert_eq!(Sign::from_f64(f64::EPSILON), Sign::Positive);
    }

    #[test]
    fn sign_from_f64_positive_infinity() {
        assert_eq!(Sign::from_f64(f64::INFINITY), Sign::Positive);
    }

    #[test]
    fn sign_from_f64_negative_infinity() {
        assert_eq!(Sign::from_f64(f64::NEG_INFINITY), Sign::Negative);
    }

    #[test]
    fn sign_from_f64_nan_classifies_as_zero() {
        // NaN compares false against everything (> 0.0, < 0.0 both false),
        // so it lands in the else branch → Zero. Documents the totality
        // contract.
        assert_eq!(Sign::from_f64(f64::NAN), Sign::Zero);
    }

    // ── Group 2: orient3d canonical orientations ──────────────────────

    /// Standard tetrahedron with apex above the XY plane.
    /// (a, b, c) = unit triangle in XY plane (CCW from +Z view).
    /// d = (0, 0, 1) is ABOVE the plane.
    /// Per Shewchuk's convention: above → Negative (counter-intuitive).
    #[test]
    fn orient3d_standard_ccw_tetra_apex_above_is_negative() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(orient3d(a, b, c, d), Sign::Negative);
    }

    #[test]
    fn orient3d_swapped_two_args_is_positive() {
        // Same tetra with last two args swapped → sign flips → Positive.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(orient3d(a, b, d, c), Sign::Positive);
    }

    #[test]
    fn orient3d_coplanar_four_points_is_zero() {
        // All four points in z=0 plane → degenerate volume → Zero.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(1.0, 1.0, 0.0);
        assert_eq!(orient3d(a, b, c, d), Sign::Zero);
    }

    // ── Group 3: orient3d antisymmetry property ───────────────────────

    fn flip(s: Sign) -> Sign {
        match s {
            Sign::Positive => Sign::Negative,
            Sign::Negative => Sign::Positive,
            Sign::Zero => Sign::Zero,
        }
    }

    #[test]
    fn orient3d_swap_ab_flips_sign() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(orient3d(b, a, c, d), flip(orient3d(a, b, c, d)));
    }

    #[test]
    fn orient3d_swap_cd_flips_sign() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(orient3d(a, b, d, c), flip(orient3d(a, b, c, d)));
    }

    #[test]
    fn orient3d_double_swap_preserves_sign() {
        // Two flips cancel.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(orient3d(b, a, d, c), orient3d(a, b, c, d));
    }

    // ── Group 4: Determinism ──────────────────────────────────────────

    #[test]
    fn orient3d_deterministic_under_repeated_runs() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let d = Point3::new(0.0, 0.0, 1.0);
        let first = orient3d(a, b, c, d);
        for _ in 0..100 {
            assert_eq!(orient3d(a, b, c, d), first);
        }
    }

    // ── Group 5: orient2d canonical orientations ──────────────────────

    /// Standard unit triangle in CCW order: c is LEFT of a→b → Positive.
    /// orient2d uses the natural geometric convention (no sign inversion).
    #[test]
    fn orient2d_standard_ccw_unit_triangle_is_positive() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        assert_eq!(orient2d(a, b, c), Sign::Positive);
    }

    #[test]
    fn orient2d_standard_cw_unit_triangle_is_negative() {
        // Same triangle, last two args swapped → CW → Negative.
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(0.0, 1.0);
        let c = Point2::new(1.0, 0.0);
        assert_eq!(orient2d(a, b, c), Sign::Negative);
    }

    #[test]
    fn orient2d_collinear_x_axis_is_zero() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(2.0, 0.0);
        assert_eq!(orient2d(a, b, c), Sign::Zero);
    }

    // ── Group 6: orient2d antisymmetry ────────────────────────────────

    #[test]
    fn orient2d_swap_ab_flips_sign() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        assert_eq!(orient2d(b, a, c), flip(orient2d(a, b, c)));
    }

    #[test]
    fn orient2d_swap_bc_flips_sign() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        assert_eq!(orient2d(a, c, b), flip(orient2d(a, b, c)));
    }

    #[test]
    fn orient2d_swap_ac_flips_sign() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        assert_eq!(orient2d(c, b, a), flip(orient2d(a, b, c)));
    }

    #[test]
    fn orient2d_double_swap_preserves_sign() {
        // Two flips cancel.
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        assert_eq!(orient2d(b, a, c), flip(orient2d(a, b, c)));
        assert_eq!(orient2d(b, c, a), orient2d(a, b, c));
    }

    // ── Group 7: orient2d edge cases ──────────────────────────────────

    #[test]
    fn orient2d_collinear_diagonal_y_equals_x_is_zero() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 1.0);
        let c = Point2::new(2.0, 2.0);
        assert_eq!(orient2d(a, b, c), Sign::Zero);
    }

    #[test]
    fn orient2d_coincident_a_b_is_zero() {
        // a == b → degenerate line → Zero (not an error).
        let a = Point2::new(1.0, 2.0);
        let b = Point2::new(1.0, 2.0);
        let c = Point2::new(3.0, 4.0);
        assert_eq!(orient2d(a, b, c), Sign::Zero);
    }

    // ── Group 8: orient2d determinism ─────────────────────────────────

    #[test]
    fn orient2d_deterministic_under_repeated_runs() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(1.0, 0.0);
        let c = Point2::new(0.0, 1.0);
        let first = orient2d(a, b, c);
        for _ in 0..100 {
            assert_eq!(orient2d(a, b, c), first);
        }
    }
}
