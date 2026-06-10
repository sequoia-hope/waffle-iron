//! Conservative interval arithmetic for the dynamic-filter tier
//! (Attene 2025 §5.2, `refs/text/attene-predicates.txt:290-302`).
//!
//! The paper's implementation sets the FPU rounding mode to
//! toward-+∞ once per predicate; portable (and WASM-clean) Rust cannot
//! touch the rounding mode, so we emulate outward rounding by widening
//! every round-to-nearest result with `next_down` / `next_up`. RN is off
//! by at most half an ulp, so one ulp of widening per endpoint per
//! operation strictly contains the directed-rounding interval — slightly
//! wider (≈1 extra ulp/op), unconditionally sound.
//!
//! Why this tier exists: the semi-static filter compares against
//! `δ(1)·β^k`, where `δ(1)` is a WORST-CASE bound over all inputs of
//! magnitude `β`. For the deep TPI-heavy `orient3d` instances (degree up
//! to 39) the worst-case bound exceeds typical generic values by many
//! orders of magnitude, so that filter almost never certifies them —
//! which is precisely why the paper cascades into interval arithmetic
//! ("It is often advantageous to use all these approaches in a cascaded
//! multi-stage evaluation", §2.1) before falling back to exact
//! arithmetic. Without this tier, every TPI-heavy call would pay exact
//! rational evaluation.

use super::Sign;

/// A closed interval `[lo, hi]` guaranteed to contain the exact real
/// value of the computation. Endpoints may be infinite (overflow); any
/// NaN poisons the interval to the whole real line.
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct Iv {
    pub lo: f64,
    pub hi: f64,
}

const WHOLE_LINE: Iv = Iv {
    lo: f64::NEG_INFINITY,
    hi: f64::INFINITY,
};

impl Iv {
    /// A point interval around an exactly-known f64 (input coordinates
    /// are exact by definition — Attene §4: explicit values "can be
    /// assumed to be exact").
    pub fn point(x: f64) -> Iv {
        if x.is_nan() {
            return WHOLE_LINE;
        }
        Iv { lo: x, hi: x }
    }

    /// Sign certification:
    /// - `Some(Positive)` / `Some(Negative)` when the interval excludes 0;
    /// - `Some(Zero)` when the interval is exactly `[0, 0]` (every
    ///   operation is outward-rounded, so a degenerate zero interval
    ///   witnesses an exact zero);
    /// - `None` when the sign is ambiguous (including NaN poisoning).
    pub fn sign(self) -> Option<Sign> {
        if self.lo > 0.0 {
            Some(Sign::Positive)
        } else if self.hi < 0.0 {
            Some(Sign::Negative)
        } else if self.lo == 0.0 && self.hi == 0.0 {
            Some(Sign::Zero)
        } else {
            None
        }
    }

    fn widen(lo: f64, hi: f64) -> Iv {
        if lo.is_nan() || hi.is_nan() {
            return WHOLE_LINE;
        }
        // next_down(-inf) = -inf and next_up(+inf) = +inf, so infinite
        // endpoints stay put; finite endpoints absorb the ≤ ulp/2 RN
        // error.
        Iv {
            lo: lo.next_down(),
            hi: hi.next_up(),
        }
    }
}

impl core::ops::Add for Iv {
    type Output = Iv;
    fn add(self, o: Iv) -> Iv {
        Iv::widen(self.lo + o.lo, self.hi + o.hi)
    }
}

impl core::ops::Sub for Iv {
    type Output = Iv;
    fn sub(self, o: Iv) -> Iv {
        Iv::widen(self.lo - o.hi, self.hi - o.lo)
    }
}

impl core::ops::Mul for Iv {
    type Output = Iv;
    fn mul(self, o: Iv) -> Iv {
        // min/max over all endpoint products. f64::min/max IGNORE NaN
        // (they return the other operand), which would silently shrink
        // the interval — e.g. 0·∞ — so NaN products poison via widen().
        let p = [
            self.lo * o.lo,
            self.lo * o.hi,
            self.hi * o.lo,
            self.hi * o.hi,
        ];
        if p.iter().any(|v| v.is_nan()) {
            return WHOLE_LINE;
        }
        let lo = p.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let hi = p.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        Iv::widen(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_ops_contain_exact_results() {
        let a = Iv::point(0.1); // 0.1 is not exactly representable; the
                                // POINT is the f64 nearest 0.1 — exact.
        let b = Iv::point(0.2);
        let s = a + b;
        // True sum of the two f64 values lies within one widened ulp.
        assert!(s.lo <= 0.1 + 0.2 && 0.1 + 0.2 <= s.hi);
        assert!(s.sign() == Some(Sign::Positive));

        let d = a - a;
        // a − a is exactly zero and outward rounding of an exact result
        // keeps... the widening makes it [-ulp, +ulp]; sign ambiguous
        // unless both endpoints collapse. Soundness: must CONTAIN 0.
        assert!(d.lo <= 0.0 && 0.0 <= d.hi);
    }

    #[test]
    fn mul_signs() {
        let neg = Iv::point(-3.0);
        let pos = Iv::point(2.0);
        assert_eq!((neg * pos).sign(), Some(Sign::Negative));
        assert_eq!((neg * neg).sign(), Some(Sign::Positive));
        let straddle = Iv { lo: -1.0, hi: 1.0 };
        assert_eq!((straddle * pos).sign(), None);
    }

    #[test]
    fn nan_and_overflow_poison_conservatively() {
        let huge = Iv::point(f64::MAX);
        let two = Iv::point(2.0);
        let over = huge * two;
        assert_eq!(over.hi, f64::INFINITY);
        assert_eq!(over.sign(), Some(Sign::Positive)); // lo still > 0

        let zero = Iv::point(0.0);
        let inf = Iv {
            lo: 0.0,
            hi: f64::INFINITY,
        };
        let poisoned = zero * inf; // 0·∞ = NaN product
        assert_eq!(poisoned.sign(), None);
        assert_eq!(poisoned.lo, f64::NEG_INFINITY);
    }

    #[test]
    fn exact_zero_interval_witnesses_zero() {
        let z = Iv::point(0.0);
        assert_eq!(z.sign(), Some(Sign::Zero));
        // 1 − 1 is exact but widened: NOT a zero witness (sound, just
        // imprecise).
        let one = Iv::point(1.0);
        assert_eq!((one - one).sign(), None);
    }
}
