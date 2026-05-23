// Function bodies are `unimplemented!()` during the RED phase (Test Author
// commit). The Implementer commits replace the bodies. The per-file MIT
// attribution + "Filtered+exact cascade" + "Conservative error bound deviation"
// headers land in a separate commit after GREEN per PR-CR4 sequencing.

use cad_primitives::Point3;

/// Cartesian axis enum used to indicate which axis a triangle normal is
/// most aligned with.
///
/// Returned by [`max_component_in_triangle_normal`]; consumers use the axis
/// to pick the 2D projection plane for downstream `orient2d` predicates.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// Public predicate: returns the axis (X / Y / Z) along which the triangle's
/// normal has the largest absolute component.
///
/// Implements Cherchi 2020's filtered+exact cascade: tries the f64
/// fast-path first, falls back to dashu-rational arithmetic when the
/// filtered version can't decide confidently.
///
/// See `specs/cherchi_rs_max_component_normal.md` for the full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn max_component_in_triangle_normal(_a: Point3, _b: Point3, _c: Point3) -> Axis {
    unimplemented!("PR-CR4 RED phase — Implementer fills body in step 4a/4b")
}

/// Filtered (f64) cascade primitive — fast path.
///
/// Returns `Some(axis)` when the f64 cross-product cleanly identifies the
/// max-magnitude axis (max |n_i| exceeds the other two by more than the
/// conservative Shewchuk-style error bound). Returns `None` when any two
/// |n_i| are too close to distinguish in f64, signaling that the caller
/// must fall back to the exact path.
///
/// Soundness: if `Some(axis)` is returned, `axis` is provably correct.
pub(crate) fn max_component_filtered(
    _a: Point3,
    _b: Point3,
    _c: Point3,
) -> Option<Axis> {
    unimplemented!("PR-CR4 RED phase — Implementer fills body in step 4a")
}

/// Exact (dashu rational) cascade primitive — slow but definitive path.
///
/// Converts each f64 coordinate to an arbitrary-precision rational (RBig),
/// computes the cross product in exact arithmetic, and returns the axis
/// with largest |n_i|. On exact ties, deterministic tiebreak: X > Y > Z.
pub(crate) fn max_component_exact(_a: Point3, _b: Point3, _c: Point3) -> Axis {
    unimplemented!("PR-CR4 RED phase — Implementer fills body in step 4b")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Canonical axis-aligned (filtered path) ───────────────

    #[test]
    fn axis_aligned_xy_plane_picks_z() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::Z);
    }

    #[test]
    fn axis_aligned_xz_plane_picks_y() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::Y);
    }

    #[test]
    fn axis_aligned_yz_plane_picks_x() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(0.0, 0.0, 1.0);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::X);
    }

    #[test]
    fn axis_aligned_reversed_winding_same_axis() {
        // Swap b and c → normal flips sign, but |n_z| is unchanged
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(1.0, 0.0, 0.0);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::Z);
    }

    // ── Group 2: Canonical off-axis (still filtered path) ─────────────

    #[test]
    fn tilted_normal_mostly_along_z() {
        // Triangle has small z-component but normal is dominated by Z
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.1);
        let c = Point3::new(0.0, 1.0, 0.1);
        // n = (1,0,0.1) × (0,1,0.1) = (0·0.1 − 0.1·1, 0.1·0 − 1·0.1, 1·1 − 0·0)
        //   = (-0.1, -0.1, 1.0) → max |n_i| is z
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::Z);
    }

    // ── Group 3: Properties ───────────────────────────────────────────

    #[test]
    fn permutation_invariance() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        // All 6 permutations should yield Axis::Z (sign of n may flip, |n| doesn't)
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::Z);
        assert_eq!(max_component_in_triangle_normal(a, c, b), Axis::Z);
        assert_eq!(max_component_in_triangle_normal(b, a, c), Axis::Z);
        assert_eq!(max_component_in_triangle_normal(b, c, a), Axis::Z);
        assert_eq!(max_component_in_triangle_normal(c, a, b), Axis::Z);
        assert_eq!(max_component_in_triangle_normal(c, b, a), Axis::Z);
    }

    #[test]
    fn translation_invariance() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let baseline = max_component_in_triangle_normal(a, b, c);

        // Shift by (100, -50, 7)
        let shift = |p: Point3| Point3::new(p.x() + 100.0, p.y() - 50.0, p.z() + 7.0);
        let result = max_component_in_triangle_normal(shift(a), shift(b), shift(c));
        assert_eq!(result, baseline);
    }

    #[test]
    fn scale_invariance_large() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let baseline = max_component_in_triangle_normal(a, b, c);

        let scale = |p: Point3| Point3::new(p.x() * 1e6, p.y() * 1e6, p.z() * 1e6);
        let result = max_component_in_triangle_normal(scale(a), scale(b), scale(c));
        assert_eq!(result, baseline);
    }

    #[test]
    fn scale_invariance_small() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        let baseline = max_component_in_triangle_normal(a, b, c);

        let scale = |p: Point3| Point3::new(p.x() * 1e-6, p.y() * 1e-6, p.z() * 1e-6);
        let result = max_component_in_triangle_normal(scale(a), scale(b), scale(c));
        assert_eq!(result, baseline);
    }

    // ── Group 4: Cascade-coverage ─────────────────────────────────────

    #[test]
    fn filtered_returns_some_on_clear_input() {
        // Axis-aligned XY: |n_z| = 1.0, |n_x| = |n_y| = 0.0 → cleanly Z
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        assert_eq!(max_component_filtered(a, b, c), Some(Axis::Z));
    }

    #[test]
    fn filtered_returns_none_on_tied_input() {
        // Construct a triangle where |n_x| ≈ |n_y| (cross product
        // components close to equal). A triangle in a 45° plane:
        //   a = (0,0,0); b = (1,1,0); c = (1,1,1)
        //   bx-ax = 1, by-ay = 1, bz-az = 0
        //   cx-ax = 1, cy-ay = 1, cz-az = 1
        //   n_x = (by-ay)*(cz-az) - (bz-az)*(cy-ay) = 1*1 - 0*1 = 1
        //   n_y = (bz-az)*(cx-ax) - (bx-ax)*(cz-az) = 0*1 - 1*1 = -1
        //   n_z = (bx-ax)*(cy-ay) - (by-ay)*(cx-ax) = 1*1 - 1*1 = 0
        //   |n_x| = |n_y| = 1 (exact tie); |n_z| = 0
        // f64 computes this exactly (no round-off) so |n_x| == |n_y| → tied
        // The filtered version's "strict greater than" must return None.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        assert_eq!(max_component_filtered(a, b, c), None);
    }

    #[test]
    fn exact_resolves_tie_with_deterministic_tiebreak() {
        // Same input as `filtered_returns_none_on_tied_input`:
        // |n_x| = |n_y| = 1 exactly; tiebreak X > Y > Z → expect X
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        assert_eq!(max_component_exact(a, b, c), Axis::X);
    }

    #[test]
    fn public_fn_returns_exact_result_on_tied_input() {
        // Public function: filtered returns None → exact runs → X
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::X);
    }

    // ── Group 5: Determinism ──────────────────────────────────────────

    #[test]
    fn deterministic_under_repeated_runs() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        let first = max_component_in_triangle_normal(a, b, c);
        for _ in 0..100 {
            assert_eq!(max_component_in_triangle_normal(a, b, c), first);
        }
    }

    // ── Group 6: A-02 regression ─────────────────────────────────────

    /// The legacy port (per audit A-02 in `docs/audits/cherchi_port_audit.md:185-198`)
    /// used a single inexact f64 cross product without a filtered+exact
    /// cascade. On inputs where the |n_i| comparison is close, f64
    /// round-off can flip the answer to the wrong axis. The clean
    /// cascade port detects the ambiguous case (filtered → None) and
    /// falls back to exact arithmetic for the definitive answer.
    ///
    /// This test exercises the cascade end-to-end on the tie input
    /// (`|n_x| = |n_y|` exactly): without exact arithmetic + the
    /// X > Y > Z tiebreak, the answer would be Y or Z depending on
    /// iteration order. The cascade returns X deterministically.
    #[test]
    fn a02_regression_cascade_resolves_tie_deterministically() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        // |n_x| = |n_y| = 1 (exact tie). Without the cascade + tiebreak,
        // a naïve "first max in iteration" could return X, Y, or Z
        // depending on comparison order.
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::X);
        // And it's deterministic across multiple invocations
        // (also covered by `deterministic_under_repeated_runs`)
    }
}
