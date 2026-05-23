//! Power-of-2 coordinate scaling factor for exact-arithmetic preprocessing.
//!
//! Ported from Cherchi 2020's `compute_multiplier` (`processing.cpp:47-64`).
//! Cherchi 2020 is MIT-licensed.
//! © 2020 Gianmarco Cherchi, Marco Livesu, Riccardo Scateni, Marco Attene
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §3 (preprocessing for exact predicates).
//!
//! **Deliberate deviation from upstream**: C++'s `1 << e` is signed-int
//! UB for `e ≥ 31` (typical CAD inputs trigger this); upstream papers
//! over with `if(multiplier < 0) multiplier = 1.0; // temporary fix`.
//! Our impl uses `(1u64 << e.min(62)) as f64` which is well-defined and
//! matches the function's stated intent (power-of-2 scaling factor). See
//! `docs/audits/cherchi_port_audit.md:228-241` (A-05) and
//! `specs/cherchi_rs_compute_multiplier.md` §"Discipline question".

/// Compute a power-of-2 scaling factor for the given coordinate array.
///
/// Returns the smallest `2^k` (for `k ∈ [0, 62]`) such that the maximum
/// absolute coordinate, when multiplied by the result, stays within
/// `2^52 * result` (f64 mantissa precision). If `max|c| < 1.0`, returns
/// `1.0` (no upscaling needed). If `⌈log₂(max|c|)⌉ > 62`, clamps to `2^62`.
///
/// See `specs/cherchi_rs_compute_multiplier.md` for the full contract,
/// including the deliberate deviation from C++ upstream's UB-induced
/// fallback behavior.
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn compute_multiplier(coords: &[f64]) -> f64 {
    let max_abs = coords
        .iter()
        .copied()
        .map(f64::abs)
        .fold(0.0_f64, f64::max);

    // Sub-unit (or zero / empty) inputs need no upscaling.
    if max_abs < 1.0 {
        return 1.0;
    }

    // e = ⌈log₂(max_abs)⌉ — the smallest exponent such that 2^e ≥ max_abs.
    // For max_abs = 1.0 exactly, log2 = 0, ceil = 0, returns 2^0 = 1.0.
    let e = max_abs.log2().ceil() as u32;

    // (1u64 << e.min(62)) is well-defined for all u32 e; 2^62 fits exactly
    // in f64 mantissa. See file header "Deliberate deviation" comment.
    (1u64 << e.min(62)) as f64
}

/// Multiply each element of `coords` by `multiplier`, in place.
///
/// Pair-mate of [`compute_multiplier`]: typical usage is
/// `multiply_coordinates(&mut coords, compute_multiplier(&coords))` to
/// scale up to f64-mantissa-exact integer range.
///
/// Ported from Cherchi 2020's `multiply_coordinates` (`processing.cpp`).
/// MIT-licensed; see file header for full attribution.
///
/// # Failure modes
///
/// NaN / infinite inputs propagate per IEEE 754 multiplication semantics.
/// No validation.
pub fn multiply_coordinates(_coords: &mut [f64], _multiplier: f64) {
    unimplemented!("PR-CR3 RED phase — Implementer fills body in next commit")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Canonical small / sub-unit (output: 1.0) ────────────

    #[test]
    fn empty_slice() {
        assert_eq!(compute_multiplier(&[]), 1.0);
    }

    #[test]
    fn all_zero() {
        assert_eq!(compute_multiplier(&[0.0, 0.0, 0.0]), 1.0);
    }

    #[test]
    fn sub_unit_max() {
        // max|c| = 0.5 < 1.0 → no upscaling
        assert_eq!(compute_multiplier(&[0.5]), 1.0);
    }

    #[test]
    fn max_equals_one() {
        // max|c| = 1.0; ceil(log2(1.0)) = 0; 2^0 = 1.0
        assert_eq!(compute_multiplier(&[1.0]), 1.0);
    }

    // ── Group 2: Canonical mid-range ─────────────────────────────────

    #[test]
    fn max_equals_three() {
        // log2(3) ≈ 1.585, ceil = 2, 2^2 = 4
        assert_eq!(compute_multiplier(&[3.0]), 4.0);
    }

    #[test]
    fn max_equals_hundred() {
        // log2(100) ≈ 6.644, ceil = 7, 2^7 = 128
        assert_eq!(compute_multiplier(&[100.0]), 128.0);
    }

    #[test]
    fn uses_absolute_value() {
        // max|c| = |−100| = 100 → 128
        assert_eq!(compute_multiplier(&[-100.0, 50.0]), 128.0);
    }

    // ── Group 3: Canonical CAD-scale (the e ≥ 31 case — A-05) ───────

    #[test]
    fn cad_scale_1e10() {
        // log2(1e10) ≈ 33.22, ceil = 34, 2^34 (would be UB in C++)
        assert_eq!(compute_multiplier(&[1e10]), 2.0_f64.powi(34));
    }

    #[test]
    fn cad_scale_max_wins() {
        // max|c| = 1e10 dominates the small values; result = 2^34
        assert_eq!(
            compute_multiplier(&[1.0, 1.0, 1e10]),
            2.0_f64.powi(34)
        );
    }

    // ── Group 4: Edge cases (clamp boundary) ──────────────────────────

    #[test]
    fn clamp_boundary() {
        // max|c| = 2^62; ceil(log2(2^62)) = 62; 2^62 (right at the limit)
        let m = 2.0_f64.powi(62);
        assert_eq!(compute_multiplier(&[m]), m);
    }

    #[test]
    fn clamp_overflow() {
        // max|c| = 2^70; ceil = 70; clamped to 2^62
        assert_eq!(
            compute_multiplier(&[2.0_f64.powi(70)]),
            2.0_f64.powi(62)
        );
    }

    #[test]
    fn negative_cad_scale() {
        // max|c| = |−1e10| = 1e10 → 2^34
        assert_eq!(compute_multiplier(&[-1e10]), 2.0_f64.powi(34));
    }

    // ── Group 5: Property — order independence ────────────────────────

    #[test]
    fn order_independence() {
        let forward = [1.0, 1e6, 0.5, -100.0, 3.14, 1e10];
        let reversed: Vec<f64> = forward.iter().rev().copied().collect();
        assert_eq!(
            compute_multiplier(&forward),
            compute_multiplier(&reversed)
        );
    }

    // ── Group 6: A-05 deviation regression ────────────────────────────

    /// The C++ upstream `compute_multiplier` uses `int multiplier = 1 << e`
    /// which is signed-int UB for `e ≥ 31`. The upstream papers over with
    /// `if(multiplier < 0) multiplier = 1.0; // temporary fix` — so on
    /// inputs that trigger the UB, the actual returned value is `1.0`,
    /// completely defeating the function's stated purpose.
    ///
    /// Per `docs/audits/cherchi_port_audit.md:228-241` (A-05) and the
    /// project's C++-deviation policy (see
    /// `specs/cherchi_rs_compute_multiplier.md` §"Discipline question"),
    /// the Rust port deliberately matches the function's stated intent
    /// (well-defined `2^e`), not the UB-induced fallback.
    ///
    /// This test asserts the deliberate-deviation behavior. If it fails,
    /// either the impl regressed to "match C++ UB" (wrong choice — see spec)
    /// or the spec was changed without consensus.
    #[test]
    fn a05_deviation_strict_correct_on_cad_scale() {
        let result = compute_multiplier(&[1e10]);
        assert_eq!(result, 2.0_f64.powi(34), "strict-correct = 2^34");
        assert_ne!(result, 1.0, "must NOT match C++'s UB-induced 1.0");
    }

    // ════════════════════════════════════════════════════════════════
    //  PR-CR3 — multiply_coordinates
    // ════════════════════════════════════════════════════════════════

    // ── Group 7: multiply_coordinates canonical ──────────────────────

    #[test]
    fn multiply_empty_slice_is_noop() {
        let mut coords: [f64; 0] = [];
        multiply_coordinates(&mut coords, 42.0);
        assert_eq!(coords, [] as [f64; 0]);
    }

    #[test]
    fn multiply_by_two() {
        let mut coords = [1.0, 2.0, 3.0];
        multiply_coordinates(&mut coords, 2.0);
        assert_eq!(coords, [2.0, 4.0, 6.0]);
    }

    #[test]
    fn multiply_by_one_is_identity() {
        let mut coords = [1.0, 2.0, 3.0];
        let original = coords;
        multiply_coordinates(&mut coords, 1.0);
        assert_eq!(coords, original);
    }

    #[test]
    fn multiply_by_zero() {
        let mut coords = [1.0, -2.0, 3.0];
        multiply_coordinates(&mut coords, 0.0);
        assert_eq!(coords, [0.0, 0.0, 0.0]);
    }

    // ── Group 8: multiply_coordinates negative coords / multipliers ──

    #[test]
    fn multiply_negative_coord_positive_multiplier() {
        let mut coords = [-1.0, 2.0];
        multiply_coordinates(&mut coords, 3.0);
        assert_eq!(coords, [-3.0, 6.0]);
    }

    #[test]
    fn multiply_positive_coords_negative_multiplier() {
        let mut coords = [1.0, 2.0];
        multiply_coordinates(&mut coords, -1.0);
        assert_eq!(coords, [-1.0, -2.0]);
    }

    // ── Group 9: Properties ───────────────────────────────────────────

    /// Power-of-2 multipliers preserve f64 mantissa exactly, so
    /// applying then dividing recovers the original bit pattern.
    #[test]
    fn multiply_power_of_two_round_trip() {
        let original = [1.5, 2.5, 3.5];
        let mut coords = original;
        multiply_coordinates(&mut coords, 8.0);
        multiply_coordinates(&mut coords, 1.0 / 8.0);
        assert_eq!(coords, original);
    }

    /// Identity property for arbitrary non-empty slice.
    #[test]
    fn multiply_identity_preserves_bits() {
        let original = [1.5, -2.5, 0.0, 3.14, -1e100, 1e-100];
        let mut coords = original;
        multiply_coordinates(&mut coords, 1.0);
        assert_eq!(coords, original);
    }

    /// Length and ordering preserved under arbitrary multiplication.
    #[test]
    fn multiply_preserves_length_and_order() {
        let mut coords: Vec<f64> = (0..100).map(|i| (i as f64) * 1.5 - 50.0).collect();
        let expected_len = coords.len();
        multiply_coordinates(&mut coords, 7.0);
        assert_eq!(coords.len(), expected_len);
        // Order check: each entry is original * 7.0
        for (i, c) in coords.iter().enumerate() {
            let original = (i as f64) * 1.5 - 50.0;
            assert_eq!(*c, original * 7.0, "index {i}");
        }
    }

    // ── Group 10: Integration with compute_multiplier ─────────────────

    /// The PAIR's documented purpose: after scale-up, max abs coord
    /// has been pushed past `2^33` (well into f64-mantissa-exact
    /// integer range). For `coords = [1e10, 1.0, 0.5]`,
    /// `compute_multiplier` returns `2^34`, and after multiplying,
    /// max coord = 1e10 * 2^34 ≈ 1.7e20 — comfortably ≥ 2^33.
    #[test]
    fn integration_pair_scales_max_into_exact_range() {
        let mut coords = [1e10, 1.0, 0.5];
        let m = compute_multiplier(&coords);
        assert_eq!(m, 2.0_f64.powi(34));
        multiply_coordinates(&mut coords, m);
        let max_abs = coords.iter().map(|c| c.abs()).fold(0.0_f64, f64::max);
        assert!(
            max_abs >= 2.0_f64.powi(33),
            "max abs coord {} should be >= 2^33 after scale-up",
            max_abs
        );
    }

    /// For sub-unit inputs, `compute_multiplier` returns `1.0` and
    /// `multiply_coordinates` is a no-op.
    #[test]
    fn integration_pair_unit_max_is_noop() {
        let mut coords = [1.0];
        let original = coords;
        let m = compute_multiplier(&coords);
        assert_eq!(m, 1.0);
        multiply_coordinates(&mut coords, m);
        assert_eq!(coords, original);
    }
}
