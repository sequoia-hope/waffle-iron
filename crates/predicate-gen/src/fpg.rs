//! Forward error analysis for semi-static filters, after Meyer & Pion
//! 2008, "FPG: A code generator for fast and certified geometric
//! predicates" (`refs/text/meyer_pion2008_fpg.txt`).
//!
//! The scheme (FPG §2, lines 120-131): precompute a bound `δ(1)` on the
//! absolute roundoff error of an expression `e`, assuming every variable
//! `v` satisfies `|v| ≤ 1` and carries zero initial error. The
//! precomputation is a forward propagation of `(magnitude bound, error
//! bound)` pairs along the expression tree (FPG Appendix B, lines
//! 656-705). At runtime, `δ(1)` is scaled by the actual input magnitude
//! — in our consumer (Attene 2025 Appendix A) by `β^k`, where `β` is the
//! max absolute factor and `k` the polynomial degree.
//!
//! Every constant computed here is rounded conservatively UP: we emulate
//! FPG's `FPU_round_to_plus_infty` by following each f64 operation with
//! `next_up` (`next_up(RN(x)) ≥ x` because round-to-nearest is off by at
//! most half an ulp).

/// Round a (round-to-nearest) intermediate conservatively up, emulating
/// FPG's round-toward-+∞ mode. `next_up(RN(x)) ≥ RU(x) ≥ x` since
/// `RN(x) ≥ x − ulp/2`.
pub fn up(x: f64) -> f64 {
    x.next_up()
}

/// `ulp(d)` per FPG Appendix B (lines 664-687): `u = d·ulp(1)`, plus the
/// "extra bonus, because of Intel's extended precision feature"
/// (`u + u/2^11`). The bonus is unnecessary on SSE2/wasm32 hardware but
/// keeping it only makes the filter more conservative.
pub fn ulp(d: f64) -> f64 {
    // ulp(1) = (1 + min_double) − 1 rounded up = 2⁻⁵² = f64::EPSILON.
    let u = up(d * f64::EPSILON);
    up(u + u / 2048.0)
}

/// "Static filter error": a `(bound, error)` pair propagated through the
/// expression — FPG Appendix B's `Sfe`.
///
/// Invariant (for inputs scaled into `[-1, 1]`): the exact value of the
/// subexpression has magnitude ≤ `bound`, and the f64-evaluated value
/// differs from the exact value by ≤ `error`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sfe {
    pub bound: f64,
    pub error: f64,
}

impl Sfe {
    /// A plain input variable: `|v| ≤ 1`, exactly represented.
    /// "Error propagation starts with an error of 0 and an absolute
    /// bound of 1" (FPG Appendix B, line 662).
    pub const EXACT_INPUT: Sfe = Sfe {
        bound: 1.0,
        error: 0.0,
    };

    /// A translation difference `a − b` of two fresh input values,
    /// treated as a single variable (FPG "Translation Filter", lines
    /// 167-189): "the error is set to ulp(1)/2" and the bound resets
    /// to 1.
    pub fn translation_input() -> Sfe {
        Sfe {
            bound: 1.0,
            error: up(ulp(1.0) / 2.0),
        }
    }

    /// FPG Appendix B `operator+` (lines 689-696). Also used for
    /// subtraction: `|a − b| ≤ |a| + |b|` and the roundoff bound for
    /// f64 `−` equals that of `+`.
    // Domain-specific error propagation, not std::ops::Add semantics
    // (this is the bound algebra, deliberately method-named like FPG's
    // `Sfe operator+`).
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, o: Sfe) -> Sfe {
        let mut bound = up(self.bound + o.bound);
        let u = up(ulp(bound) / 2.0);
        bound = up(bound + u);
        let error = up(up(u + self.error) + o.error);
        Sfe { bound, error }
    }

    /// FPG Appendix B `operator*` (lines 698-705):
    /// `error = u + e1·e2 + e1·b2 + b1·e2`.
    #[allow(clippy::should_implement_trait)] // see `add`
    pub fn mul(self, o: Sfe) -> Sfe {
        let mut bound = up(self.bound * o.bound);
        let u = up(ulp(bound) / 2.0);
        bound = up(bound + u);
        let error = up(
            up(up(u + up(self.error * o.error)) + up(self.error * o.bound))
                + up(self.bound * o.error),
        );
        Sfe { bound, error }
    }

    /// The runtime-ready filter constant `δ(1)` for a degree-`k`
    /// homogeneous expression with this final `Sfe`.
    ///
    /// On top of the propagated `error` we fold in `(1 + ε)^(k+2)` to
    /// absorb the round-to-nearest error of the runtime threshold
    /// computation `δ·β^k` itself — FPG §2 (lines 161-163): "δ(1) has
    /// to be multiplied with a constant (1 + ǫ)^d, ǫ being the machine
    /// epsilon and rounding the constant towards +∞". `k` covers the
    /// `powi` multiplications plus the final `δ·β^k` product; the `+2`
    /// is slack for the additive subnormal guard.
    pub fn delta(self, degree: u32) -> f64 {
        let mut d = self.error;
        for _ in 0..(degree + 2) {
            d = up(d * (1.0 + f64::EPSILON));
        }
        assert!(
            d.is_finite() && d > 0.0 && d < 1.0,
            "delta(1) out of sane range: {d:e} — generator bug or \
             pathologically deep expression"
        );
        d
    }
}
