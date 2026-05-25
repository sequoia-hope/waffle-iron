// Function bodies are `unimplemented!()` during the RED phase (Test Author
// commit). The Implementer commit replaces the bodies. The per-file MIT
// attribution header lands in a separate commit after GREEN per PR-CR6
// sequencing.

use cad_primitives::Point3;

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
}
