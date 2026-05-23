// Function body is `unimplemented!()` during the RED phase (Test Author
// commit). The Implementer commit replaces the body. The attribution +
// "Deliberate deviation" header lands in a separate commit after GREEN
// per PR-CR2 sequencing.

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
pub fn compute_multiplier(_coords: &[f64]) -> f64 {
    unimplemented!("PR-CR2 RED phase — Implementer fills body in next commit")
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
}
