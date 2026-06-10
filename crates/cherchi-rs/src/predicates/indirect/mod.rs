//! Clean-room pure-Rust indirect predicates (M7, slice 1: orient3d).
//!
//! Implements Attene's *indirect geometric predicates* framework
//! [Attene 2025, "Indirect predicates for geometric constructions"]
//! for points of intersection of linear elements, exactly as used by the
//! Cherchi 2020 mesh arrangement:
//!
//! - An **implicit point** is represented by the primitive geometric
//!   elements that define it (a line + a plane for LPI, three planes for
//!   TPI), never by rounded coordinates. Its Cartesian coordinates are
//!   polynomial fractions `(λx/d, λy/d, λz/d)` over the defining
//!   coordinates (Attene §4, `refs/text/attene-predicates.txt:149-166`).
//! - A predicate over implicit points substitutes the fractions into its
//!   polynomial Λ, producing `Λ′/D′` where `D′` is a product of the `d`s;
//!   the sign of `D′` is resolved by counting negative `d`s (sign-parity
//!   rule, Attene §5.1, `attene-predicates.txt:281-286`). No division is
//!   ever performed.
//! - Evaluation is tiered: a **semi-statically filtered f64** tier
//!   (Attene §5.1 + Appendix A: `ε = δ(1)·β^k`) falls back to an **exact
//!   rational** tier (`dashu::rational::RBig`; Attene §5.3 — we use
//!   rationals instead of floating-point expansions, which is exact a
//!   fortiori and WASM-clean).
//!
//! ## CLEAN-ROOM PROVENANCE
//!
//! This module and its code generator (`crates/predicate-gen`) derive
//! ONLY from the published papers:
//!
//! - Attene 2025 (`refs/text/attene-predicates.txt`) — framework, LPI
//!   lambdas (§4.2), orient3d rewriting (§4.6), tiered evaluation (§5),
//!   instance reduction (§6), semi-static filter `δ(1)·β^k` (Appendix A).
//! - Cherchi et al. 2020 §4.2.2 (`refs/text/mesh_arrangement.txt:340-394`)
//!   — the same LPI lambdas (cross-checked against Attene §4.2 — they
//!   agree) AND the TPI lambdas (`λT`, `dT` via determinants over the
//!   `n`/`p` vectors of the three defining triangles).
//! - Meyer & Pion 2008 (FPG, `refs/text/meyer_pion2008_fpg.txt`) — the
//!   forward error analysis that computes the compile-time constant
//!   `δ(1)` (Appendix B rules, lines 656-705).
//!
//! The LGPL C++ `Indirect_Predicates` implementation was NOT consulted.
//! The FFI sidecar (`indirect-predicates-sidecar-rs`) is used strictly as
//! a black-box differential test oracle (`tests/indirect_ffi_parity.rs`).
//!
//! ## Design notes (ours, not the C++ architecture)
//!
//! - [`GenericPoint3D`] is an owned, generator-based enum — no lifetimes,
//!   no handle/pointer architecture.
//! - Per Attene §5.4 (caching), the f64 lambda values, the cached filter
//!   factor `β`, and the d-sign filter verdict are computed lazily once
//!   per point (`OnceLock`). Exact lambdas are recomputed on demand: the
//!   paper measured that caching the exact tier is not advantageous
//!   (Attene §5.4, Table 3 discussion).
//! - We have **two** tiers, not three: the paper's middle interval tier
//!   is an optimization, not a correctness requirement (§5: "evaluated
//!   with the fastest model which guarantees exactness"). Our exact tier
//!   is total.
//!
//! ## Sign convention
//!
//! Identical to [`crate::predicates::orient3d`] (CR6 / Shewchuk):
//! `orient3d_indirect(a, b, c, d)` returns the sign of
//! `det[a − d; b − d; c − d]` — Positive when `d` lies BELOW the plane
//! through CCW `(a, b, c)`. Attene §4.6 uses the same determinant with
//! `(p1, p2, p3, p4) = (a, b, c, d)`.
//!
//! [`Sign::Undefined`] is returned when any implicit argument is
//! undefined (`d == 0` exactly: line parallel to plane, degenerate
//! generators — Attene §4.2 note + §5.3). The 4-variant enum mirrors the
//! FFI sidecar's `Sign` convention.

use std::sync::OnceLock;

use cad_primitives::Point3;
use dashu::rational::RBig;

mod generated;

// =========================================================================
// Sign
// =========================================================================

/// Predicate result sign, 4-valued.
///
/// Same shape as the FFI sidecar's `Sign` (Negative / Zero / Positive /
/// Undefined). `Undefined` means at least one implicit input point does
/// not exist (its denominator `d` is exactly zero — Attene §5.3).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Sign {
    Negative,
    Zero,
    Positive,
    Undefined,
}

impl Sign {
    /// Flip Positive ↔ Negative; Zero and Undefined are fixed points.
    /// Used for odd argument permutations (Attene §6) and negative
    /// denominator parity (Attene §5.1).
    pub fn flipped(self) -> Sign {
        match self {
            Sign::Positive => Sign::Negative,
            Sign::Negative => Sign::Positive,
            other => other,
        }
    }

    /// Exact sign of a rational.
    pub(crate) fn of_rbig(x: &RBig) -> Sign {
        use core::cmp::Ordering;
        match x.cmp(&RBig::ZERO) {
            Ordering::Greater => Sign::Positive,
            Ordering::Less => Sign::Negative,
            Ordering::Equal => Sign::Zero,
        }
    }
}

impl From<crate::predicates::Sign> for Sign {
    fn from(s: crate::predicates::Sign) -> Sign {
        match s {
            crate::predicates::Sign::Positive => Sign::Positive,
            crate::predicates::Sign::Negative => Sign::Negative,
            crate::predicates::Sign::Zero => Sign::Zero,
        }
    }
}

// =========================================================================
// Cached lambda representations (Attene §5.4)
// =========================================================================

/// Floating-point lambda cache for one implicit point.
///
/// Computed once per point (first predicate touching it) and reused by
/// every later filtered evaluation — Attene §5.4: "upon the first
/// occurrence it is worth storing their values for future reuse. For the
/// sake of implementing filters ... this amounts to store an additional
/// FP number" (the `beta` factor, Appendix A).
#[derive(Clone, Debug)]
pub(crate) struct LambdaF64 {
    /// `(λx, λy, λz)` evaluated in f64.
    pub l: [f64; 3],
    /// Denominator `d` evaluated in f64.
    pub d: f64,
    /// Cached semi-static filter factor: the max `|b_j|` over this
    /// point's defining factors (raw inputs and input differences) —
    /// Attene Appendix A ("computed only once for each implicit point
    /// and cached for later use").
    pub beta: f64,
    /// True iff the semi-static filter certifies `sign(d) != 0`
    /// (`|d| > δ_d·β^k_d`). When false the f64 tier must not be used at
    /// all for predicates involving this point (Attene §5.1: "it makes
    /// no sense to proceed with this arithmetic model").
    pub d_reliable: bool,
}

/// Exact rational lambda for one implicit point. Recomputed on demand
/// (not cached — Attene §5.4 found exact-tier caching not advantageous).
#[derive(Clone, Debug)]
pub(crate) struct LambdaExact {
    pub l: [RBig; 3],
    pub d: RBig,
}

impl LambdaExact {
    /// The implicit point is undefined iff `d == 0` (Attene §4.2 / §5.3).
    pub fn is_undefined(&self) -> bool {
        self.d == RBig::ZERO
    }
}

// =========================================================================
// GenericPoint3D
// =========================================================================

/// Line-plane intersection point (Attene §4.2, Fig. 1-right): the
/// intersection of the line through `(p, q)` with the plane through
/// `(r, s, t)`. Undefined when the line is parallel to the plane or any
/// generator is degenerate (`d == 0`).
#[derive(Debug)]
pub struct LpiPoint {
    pub p: Point3,
    pub q: Point3,
    pub r: Point3,
    pub s: Point3,
    pub t: Point3,
    cache: OnceLock<LambdaF64>,
}

/// Three-plane intersection point (Cherchi 2020 §4.2.2): the common
/// point of the supporting planes of triangles `v`, `w`, `u`. Undefined
/// when the three planes are not in general position (`dT == 0`).
#[derive(Debug)]
pub struct TpiPoint {
    pub v: [Point3; 3],
    pub w: [Point3; 3],
    pub u: [Point3; 3],
    cache: OnceLock<LambdaF64>,
}

/// A 3D point that is either explicit (plain coordinates, assumed exact)
/// or implicit (an unevaluated intersection construction over explicit
/// points) — Attene §4 terminology. Owned and generator-based.
#[derive(Debug)]
pub enum GenericPoint3D {
    Explicit(Point3),
    Lpi(LpiPoint),
    Tpi(TpiPoint),
}

impl Clone for GenericPoint3D {
    fn clone(&self) -> Self {
        // Fresh (empty) caches: OnceLock is not Clone; the lambda cache
        // is a pure function of the generators so dropping it is safe.
        match self {
            GenericPoint3D::Explicit(p) => GenericPoint3D::Explicit(*p),
            GenericPoint3D::Lpi(l) => GenericPoint3D::lpi(l.p, l.q, l.r, l.s, l.t),
            GenericPoint3D::Tpi(t) => GenericPoint3D::tpi(t.v, t.w, t.u),
        }
    }
}

impl GenericPoint3D {
    /// An explicit point (exact coordinates).
    pub fn explicit(p: Point3) -> Self {
        GenericPoint3D::Explicit(p)
    }

    /// Implicit line-plane intersection: line through `(p, q)`, plane
    /// through `(r, s, t)`.
    pub fn lpi(p: Point3, q: Point3, r: Point3, s: Point3, t: Point3) -> Self {
        GenericPoint3D::Lpi(LpiPoint {
            p,
            q,
            r,
            s,
            t,
            cache: OnceLock::new(),
        })
    }

    /// Implicit three-plane intersection: supporting planes of triangles
    /// `v`, `w`, `u`.
    pub fn tpi(v: [Point3; 3], w: [Point3; 3], u: [Point3; 3]) -> Self {
        GenericPoint3D::Tpi(TpiPoint {
            v,
            w,
            u,
            cache: OnceLock::new(),
        })
    }

    /// True for the `Explicit` variant.
    pub fn is_explicit(&self) -> bool {
        matches!(self, GenericPoint3D::Explicit(_))
    }

    /// Canonicalization rank: most-implicit first (TPI < LPI < Explicit)
    /// so that, after a stable sort, explicit arguments occupy the
    /// trailing predicate slots (the `p4` slot of Attene §4.6 stays
    /// explicit whenever any argument is explicit — the tightest filter
    /// form, keeping explicit−explicit translation differences).
    fn rank(&self) -> u8 {
        match self {
            GenericPoint3D::Tpi(_) => 0,
            GenericPoint3D::Lpi(_) => 1,
            GenericPoint3D::Explicit(_) => 2,
        }
    }

    /// Cached f64 lambda (Attene §5.4). Panics on `Explicit` — explicit
    /// points have no lambda; dispatchers never call this on them.
    pub(crate) fn lambda_f64(&self) -> &LambdaF64 {
        match self {
            GenericPoint3D::Explicit(_) => {
                unreachable!("lambda_f64 on an explicit point (dispatch bug)")
            }
            GenericPoint3D::Lpi(l) => l
                .cache
                .get_or_init(|| generated::lpi_lambda_f64(&l.p, &l.q, &l.r, &l.s, &l.t)),
            GenericPoint3D::Tpi(t) => t
                .cache
                .get_or_init(|| generated::tpi_lambda_f64(&t.v, &t.w, &t.u)),
        }
    }

    /// Exact rational lambda (recomputed on demand; Attene §5.3 / §5.4).
    /// Panics on `Explicit`.
    pub(crate) fn lambda_exact(&self) -> LambdaExact {
        match self {
            GenericPoint3D::Explicit(_) => {
                unreachable!("lambda_exact on an explicit point (dispatch bug)")
            }
            GenericPoint3D::Lpi(l) => generated::lpi_lambda_exact(&l.p, &l.q, &l.r, &l.s, &l.t),
            GenericPoint3D::Tpi(t) => generated::tpi_lambda_exact(&t.v, &t.w, &t.u),
        }
    }

    /// Coordinates of an `Explicit` point. Panics on implicit variants.
    pub(crate) fn expect_explicit(&self) -> Point3 {
        match self {
            GenericPoint3D::Explicit(p) => *p,
            _ => unreachable!("expect_explicit on an implicit point (dispatch bug)"),
        }
    }
}

// =========================================================================
// Support helpers shared with generated code
// =========================================================================

pub(crate) mod support {
    use dashu::float::FBig;
    use dashu::rational::RBig;

    /// Exact f64 → RBig conversion. Total on finite inputs; non-finite
    /// coordinates map to 0 (generator coordinates are finite by
    /// construction; predicates on non-finite input are undefined per
    /// crate convention).
    pub fn rb(x: f64) -> RBig {
        let fb: Option<FBig> = FBig::try_from(x).ok();
        fb.and_then(|fb| RBig::try_from(fb).ok())
            .unwrap_or(RBig::ZERO)
    }

    /// Additive guard on the runtime filter threshold `ε = δ(1)·β^k`.
    ///
    /// Soundness of `δ(1)·β^k` relies on relative-error reasoning, which
    /// breaks if any of the (up to two) multiplications in the threshold
    /// computation underflows, or if intermediate steps of the predicate
    /// polynomial evaluation underflow (FPG §2 "Under/Overflow
    /// Protection", `meyer_pion2008_fpg.txt:265-299`). Instead of FPG's
    /// per-predicate `λmin` bound we add a constant `1e-300`:
    ///
    /// - any underflow-induced absolute error in the polynomial
    ///   evaluation is at most (#ops)·2⁻¹⁰⁷⁴ < 2⁻¹⁰²⁴ ≪ 1e-300, and
    /// - if `β^k` itself underflows (computed value < 2.3e-308), then
    ///   since every generated `δ(1) < 1` the true threshold is
    ///   < 2.3e-307 ≪ 1e-300, so the inflated `ε` stays an upper bound.
    ///
    /// Inflating `ε` only ever turns definite answers into `Uncertain` —
    /// never an incorrect sign. Overflow needs no guard: `ε = +inf`
    /// fails `is_finite()` and NaN polynomial values compare `false`
    /// against both `> ε` and `< -ε`, also yielding `Uncertain`.
    pub const SUBNORMAL_GUARD: f64 = 1e-300;
}

// =========================================================================
// Canonicalization (Attene §6 instance reduction)
// =========================================================================

/// Stable-sort the four arguments most-implicit-first (TPI < LPI < E),
/// returning the canonical order plus whether the permutation is odd.
/// `orient3d` is alternating in all four arguments (it is the 4×4
/// homogeneous determinant), so an odd permutation flips the sign —
/// Attene §6: "many of these instances can be reduced to each other by
/// transposing their input parameters and possibly inverting the
/// resulting sign".
fn canonicalize<'a>(
    args: [&'a GenericPoint3D; 4],
) -> ([&'a GenericPoint3D; 4], bool) {
    let mut idx = [0usize, 1, 2, 3];
    // Stable insertion sort on rank; count swaps for permutation parity.
    let mut odd = false;
    for i in 1..4 {
        let mut j = i;
        while j > 0 && args[idx[j - 1]].rank() > args[idx[j]].rank() {
            idx.swap(j - 1, j);
            odd = !odd;
            j -= 1;
        }
    }
    ([args[idx[0]], args[idx[1]], args[idx[2]], args[idx[3]]], odd)
}

// =========================================================================
// Public predicate API
// =========================================================================

/// Indirect 3D orientation predicate over explicit / LPI / TPI points.
///
/// Sign convention identical to [`crate::predicates::orient3d`]
/// (Shewchuk: Positive = `d` BELOW the CCW plane through `(a, b, c)`).
/// Returns [`Sign::Undefined`] iff any implicit argument's denominator
/// is exactly zero (the point does not exist).
///
/// Evaluation: semi-statically filtered f64 (Attene §5.1, App. A), then
/// exact rationals (§5.3). The result is always exact.
pub fn orient3d_indirect(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
    d: &GenericPoint3D,
) -> Sign {
    // All-explicit fast path: delegate to the existing pure adaptive
    // predicate (CR6) — Attene §6: "If the three input points ... are
    // all explicit, the predicate reduces to the standard [predicate]".
    if let (
        GenericPoint3D::Explicit(pa),
        GenericPoint3D::Explicit(pb),
        GenericPoint3D::Explicit(pc),
        GenericPoint3D::Explicit(pd),
    ) = (a, b, c, d)
    {
        return crate::predicates::orient3d(*pa, *pb, *pc, *pd).into();
    }
    let (args, odd) = canonicalize([a, b, c, d]);
    let s = generated::dispatch_canonical(args[0], args[1], args[2], args[3]);
    if odd {
        s.flipped()
    } else {
        s
    }
}

/// Filtered tier only: `Some(sign)` iff the semi-static f64 filter
/// certifies the sign without exact arithmetic; `None` when the filter
/// is uncertain. Exposed (in addition to [`orient3d_indirect`]) so the
/// filter-soundness oracle can compare the tiers independently.
pub fn orient3d_indirect_filtered(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
    d: &GenericPoint3D,
) -> Option<Sign> {
    let (args, odd) = canonicalize([a, b, c, d]);
    let s = generated::dispatch_filtered_canonical(args[0], args[1], args[2], args[3])?;
    Some(if odd { s.flipped() } else { s })
}

/// Exact tier only (rational arithmetic, no filter). Ground truth for
/// the soundness oracle.
pub fn orient3d_indirect_exact(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
    d: &GenericPoint3D,
) -> Sign {
    let (args, odd) = canonicalize([a, b, c, d]);
    let s = generated::dispatch_exact_canonical(args[0], args[1], args[2], args[3]);
    if odd {
        s.flipped()
    } else {
        s
    }
}

#[cfg(test)]
mod tests;
