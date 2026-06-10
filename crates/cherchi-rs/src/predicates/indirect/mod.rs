//! Clean-room pure-Rust indirect predicates (M7).
//!
//! Slice 1 (PR-CR-M7a): `orient3d`. Slice 2 (PR-CR-M7b): the full
//! catalog cherchi-rs consumes — `orient2d_{xy,yz,zx}`,
//! `less_than_on_{x,y,z}`, the composites (`point_in_triangle`,
//! `inner_segments_cross`, `point_in_{inner_,}segment`) and
//! `approx_lpi`.
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
//! - Evaluation cascades through the paper's three computation models
//!   (Attene §5: "the predicate is evaluated with the fastest model
//!   which guarantees exactness"):
//!   1. **semi-statically filtered f64** (Attene §5.1 + Appendix A:
//!      `ε = δ(1)·β^k`),
//!   2. **interval arithmetic** (Attene §5.2; see [`interval`] — needed
//!      because the worst-case `δ(1)·β^k` bound almost never certifies
//!      the deep TPI-heavy instances, degree up to 39),
//!   3. **exact rational** (`dashu::rational::RBig`; Attene §5.3 — we
//!      use rationals instead of floating-point expansions, which is
//!      exact a fortiori and WASM-clean).
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
//! - Per Attene §5.4 (caching), the f64 lambda values (+ the cached
//!   filter factor `β` and the d-sign filter verdict) and the interval
//!   lambdas are computed lazily once per point (`OnceLock`) — the paper
//!   found exactly this caching combination optimal (Table 3, "Interval"
//!   column). Exact lambdas are recomputed on demand: caching the exact
//!   tier is not advantageous (§5.4).
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

// rustfmt must not touch the generated module: predicate-gen's
// `checked_in_file_is_fresh` test diffs it byte-for-byte against a fresh
// generation.
#[rustfmt::skip]
mod generated;
mod interval;

pub(crate) use interval::Iv;

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

/// Interval lambda cache for one implicit point (dynamic-filter tier,
/// Attene §5.2 + §5.4: "For dynamic filters, this amounts to store the
/// λ's and d using intervals").
#[derive(Clone, Debug)]
pub(crate) struct LambdaIv {
    pub l: [Iv; 3],
    pub d: Iv,
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
    iv_cache: OnceLock<LambdaIv>,
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
    iv_cache: OnceLock<LambdaIv>,
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
            iv_cache: OnceLock::new(),
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
            iv_cache: OnceLock::new(),
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

    /// Cached interval lambda (dynamic-filter tier, Attene §5.2/§5.4).
    /// Panics on `Explicit`.
    pub(crate) fn lambda_iv(&self) -> &LambdaIv {
        match self {
            GenericPoint3D::Explicit(_) => {
                unreachable!("lambda_iv on an explicit point (dispatch bug)")
            }
            GenericPoint3D::Lpi(l) => l
                .iv_cache
                .get_or_init(|| generated::lpi_lambda_iv(&l.p, &l.q, &l.r, &l.s, &l.t)),
            GenericPoint3D::Tpi(t) => t
                .iv_cache
                .get_or_init(|| generated::tpi_lambda_iv(&t.v, &t.w, &t.u)),
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
    /// - an intermediate underflow injects an absolute error of at most
    ///   `2⁻¹⁰⁷⁵` at the operation where it occurs, AMPLIFIED by the
    ///   magnitude bounds of every subsequent multiplication. For
    ///   `β ≥ 1` the amplified contribution is bounded by
    ///   `(#ops)·2⁻¹⁰⁷⁵·Λ′(1)·β^k ≪ δ(1)·β^k`: the generated `δ(1)`
    ///   collects at least one `2⁻⁵³` rounding term against the same
    ///   `Λ′` magnitude bound, so it dominates by a factor ~`2¹⁰²²`.
    ///   For `β < 1` every factor bound is at most its `β = 1` value,
    ///   so the amplified contribution is bounded by
    ///   `(#ops)·2⁻¹⁰⁷⁵ × (product of the β = 1 factor bounds)
    ///   = (#ops)·2⁻¹⁰⁷⁵·Λ′(1) ≪ 1e-300`, absorbed by the guard.
    /// - if `β^k` itself underflows (computed value < 2.3e-308), then
    ///   since every generated `δ(1) < 1` the true threshold is
    ///   < 2.3e-307 ≪ 1e-300, so the inflated `ε` stays an upper bound.
    ///
    /// Inflating `ε` only ever turns definite answers into `Uncertain` —
    /// never an incorrect sign.
    ///
    /// OVERFLOW is handled by explicit finiteness guards in the emitted
    /// code (PR-CR-M7b-fix F1), NOT by this constant: `ε = +inf` fails
    /// its `is_finite()` check; an overflowed polynomial value fails the
    /// emitted `lam.is_finite()` check (`lam = ±inf` CAN carry the wrong
    /// sign — a later term of the opposite sign that would have brought
    /// the true sum back across zero is absorbed by `±inf + finite =
    /// ±inf`); an overflowed denominator fails the `d.is_finite()`
    /// requirement inside the emitted `d_reliable` gates. Together these
    /// are the moral equivalent of FPG's λmax upper-bound test
    /// (`meyer_pion2008_fpg.txt:265-285`). NaN polynomial values fail
    /// `is_finite()` too (previously they fell through the sign
    /// comparisons; now they exit one step earlier).
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
fn canonicalize(args: [&GenericPoint3D; 4]) -> ([&GenericPoint3D; 4], bool) {
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
    (
        [args[idx[0]], args[idx[1]], args[idx[2]], args[idx[3]]],
        odd,
    )
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
/// Evaluation cascade: semi-statically filtered f64 (Attene §5.1,
/// App. A) → interval arithmetic (§5.2) → exact rationals (§5.3). The
/// result is always exact.
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

/// Filtered (inexact) tiers only: `Some(sign)` iff the semi-static f64
/// filter (Attene §5.1) or the interval dynamic filter (§5.2) certifies
/// the sign without exact arithmetic; `None` when both are uncertain.
/// Exposed (in addition to [`orient3d_indirect`]) so the
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

// =========================================================================
// PR-CR-M7b public surface — orient2d projections, per-axis comparators,
// composite predicates, LPI approximation
// =========================================================================
//
// Clean-room formulations (papers only):
//
// - `orient2d_{xy,yz,zx}_indirect` — Attene 2025 §4.3 (indirect orient2d
//   rewriting) + §4.5 (orient2d3d drop-coordinate projections); Cherchi
//   2020 Appendix A gives the per-instance ORIENT2D_XY polynomials and
//   published filter constants (YZ/ZX by subscript replacement). The
//   generated instances canonicalize with rank L < T < E (the appendix
//   instance set's order — the determinant pivots on the first argument,
//   and the lower-degree implicit pivot gives the tighter polynomial).
// - `less_than_on_{x,y,z}_indirect` — Cherchi 2020 Appendix B
//   (POINTCOMPARE_ON_X): sign(a.c − b.c) = sign(λ_ac·d_b − λ_bc·d_a)
//   resolved with the d-sign parity rule (Attene §5.1).
// - Composites are pure compositions of the two primitive families — no
//   new polynomials (see their doc comments for the geometric
//   arguments).
// - `approx_lpi` — interval-midpoint readback of the LPI lambdas (the
//   native equivalent of the FFI `lambda3d_lpi_interval` consumer use in
//   `arrangements/intersection_points.rs::lpi_approx`).

/// Canonicalization rank for the 2D/comparator families: L < T < E (the
/// order of Cherchi 2020 Appendix A/B's instance sets; distinct from
/// orient3d's T < L < E, whose pivot sits in the LAST slot).
fn rank_lte(p: &GenericPoint3D) -> u8 {
    match p {
        GenericPoint3D::Lpi(_) => 0,
        GenericPoint3D::Tpi(_) => 1,
        GenericPoint3D::Explicit(_) => 2,
    }
}

/// Stable-sort three arguments by L < T < E, tracking permutation
/// parity. `orient2d` is alternating in its three arguments (it is a
/// 2×2 difference determinant), so an odd permutation flips the sign —
/// Attene §6 / Cherchi 2020 §4.2.2 ("any transposition of input points
/// has a predictable outcome").
fn canonicalize3_lte(args: [&GenericPoint3D; 3]) -> ([&GenericPoint3D; 3], bool) {
    let mut idx = [0usize, 1, 2];
    let mut odd = false;
    for i in 1..3 {
        let mut j = i;
        while j > 0 && rank_lte(args[idx[j - 1]]) > rank_lte(args[idx[j]]) {
            idx.swap(j - 1, j);
            odd = !odd;
            j -= 1;
        }
    }
    ([args[idx[0]], args[idx[1]], args[idx[2]]], odd)
}

/// Axis-aligned projection selector for the orient2d family.
#[derive(Clone, Copy)]
enum Proj {
    Xy,
    Yz,
    Zx,
}

const PROJECTIONS: [Proj; 3] = [Proj::Xy, Proj::Yz, Proj::Zx];

fn orient2d_proj(proj: Proj, a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Sign {
    let (args, odd) = canonicalize3_lte([a, b, c]);
    let s = match proj {
        Proj::Xy => generated::dispatch_orient2d_xy_canonical(args[0], args[1], args[2]),
        Proj::Yz => generated::dispatch_orient2d_yz_canonical(args[0], args[1], args[2]),
        Proj::Zx => generated::dispatch_orient2d_zx_canonical(args[0], args[1], args[2]),
    };
    if odd {
        s.flipped()
    } else {
        s
    }
}

fn orient2d_proj_filtered(
    proj: Proj,
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Option<Sign> {
    let (args, odd) = canonicalize3_lte([a, b, c]);
    let s = match proj {
        Proj::Xy => generated::dispatch_orient2d_xy_filtered_canonical(args[0], args[1], args[2]),
        Proj::Yz => generated::dispatch_orient2d_yz_filtered_canonical(args[0], args[1], args[2]),
        Proj::Zx => generated::dispatch_orient2d_zx_filtered_canonical(args[0], args[1], args[2]),
    }?;
    Some(if odd { s.flipped() } else { s })
}

fn orient2d_proj_exact(
    proj: Proj,
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Sign {
    let (args, odd) = canonicalize3_lte([a, b, c]);
    let s = match proj {
        Proj::Xy => generated::dispatch_orient2d_xy_exact_canonical(args[0], args[1], args[2]),
        Proj::Yz => generated::dispatch_orient2d_yz_exact_canonical(args[0], args[1], args[2]),
        Proj::Zx => generated::dispatch_orient2d_zx_exact_canonical(args[0], args[1], args[2]),
    };
    if odd {
        s.flipped()
    } else {
        s
    }
}

/// Indirect 2D orientation of `(a, b, c)` projected on the XY plane
/// (drop z): the sign of `det[Γ(b) − Γ(a); Γ(c) − Γ(a)]` with
/// `Γ(p) = (p_x, p_y)`. Positive = CCW (c strictly left of a→b in the
/// projection), Zero = collinear, Undefined iff any implicit argument's
/// `d == 0`. Same convention as [`crate::predicates::orient2d`] (and the
/// FFI reference — pinned by the parity calibration anchor).
pub fn orient2d_xy_indirect(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Sign {
    orient2d_proj(Proj::Xy, a, b, c)
}

/// Filtered (inexact) tiers only for [`orient2d_xy_indirect`].
pub fn orient2d_xy_indirect_filtered(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Option<Sign> {
    orient2d_proj_filtered(Proj::Xy, a, b, c)
}

/// Exact tier only for [`orient2d_xy_indirect`] (soundness ground truth).
pub fn orient2d_xy_indirect_exact(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Sign {
    orient2d_proj_exact(Proj::Xy, a, b, c)
}

/// As [`orient2d_xy_indirect`] but projected on YZ (drop x;
/// `Γ(p) = (p_y, p_z)`).
pub fn orient2d_yz_indirect(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Sign {
    orient2d_proj(Proj::Yz, a, b, c)
}

/// Filtered (inexact) tiers only for [`orient2d_yz_indirect`].
pub fn orient2d_yz_indirect_filtered(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Option<Sign> {
    orient2d_proj_filtered(Proj::Yz, a, b, c)
}

/// Exact tier only for [`orient2d_yz_indirect`].
pub fn orient2d_yz_indirect_exact(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Sign {
    orient2d_proj_exact(Proj::Yz, a, b, c)
}

/// As [`orient2d_xy_indirect`] but projected on ZX (drop y;
/// `Γ(p) = (p_z, p_x)`).
pub fn orient2d_zx_indirect(a: &GenericPoint3D, b: &GenericPoint3D, c: &GenericPoint3D) -> Sign {
    orient2d_proj(Proj::Zx, a, b, c)
}

/// Filtered (inexact) tiers only for [`orient2d_zx_indirect`].
pub fn orient2d_zx_indirect_filtered(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Option<Sign> {
    orient2d_proj_filtered(Proj::Zx, a, b, c)
}

/// Exact tier only for [`orient2d_zx_indirect`].
pub fn orient2d_zx_indirect_exact(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> Sign {
    orient2d_proj_exact(Proj::Zx, a, b, c)
}

/// Axis selector for the comparator family.
#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
    Z,
}

const AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

/// Canonicalize a comparator pair (L < T < E). The comparator is
/// antisymmetric — `sign(a.c − b.c) = −sign(b.c − a.c)` — so a swap
/// flips the result.
fn lt_dispatch(axis: Axis, a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    let swap = rank_lte(a) > rank_lte(b);
    let (x, y) = if swap { (b, a) } else { (a, b) };
    let s = match axis {
        Axis::X => generated::dispatch_less_than_on_x_canonical(x, y),
        Axis::Y => generated::dispatch_less_than_on_y_canonical(x, y),
        Axis::Z => generated::dispatch_less_than_on_z_canonical(x, y),
    };
    if swap {
        s.flipped()
    } else {
        s
    }
}

fn lt_dispatch_filtered(axis: Axis, a: &GenericPoint3D, b: &GenericPoint3D) -> Option<Sign> {
    let swap = rank_lte(a) > rank_lte(b);
    let (x, y) = if swap { (b, a) } else { (a, b) };
    let s = match axis {
        Axis::X => generated::dispatch_less_than_on_x_filtered_canonical(x, y),
        Axis::Y => generated::dispatch_less_than_on_y_filtered_canonical(x, y),
        Axis::Z => generated::dispatch_less_than_on_z_filtered_canonical(x, y),
    }?;
    Some(if swap { s.flipped() } else { s })
}

fn lt_dispatch_exact(axis: Axis, a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    let swap = rank_lte(a) > rank_lte(b);
    let (x, y) = if swap { (b, a) } else { (a, b) };
    let s = match axis {
        Axis::X => generated::dispatch_less_than_on_x_exact_canonical(x, y),
        Axis::Y => generated::dispatch_less_than_on_y_exact_canonical(x, y),
        Axis::Z => generated::dispatch_less_than_on_z_exact_canonical(x, y),
    };
    if swap {
        s.flipped()
    } else {
        s
    }
}

/// Indirect per-axis comparator: the sign of `a.x − b.x` over generic
/// points (`Negative` ⟺ `a.x < b.x`, `Zero` ⟺ equal, `Undefined` iff an
/// implicit argument's `d == 0`). Cherchi 2020 Appendix B
/// (POINTCOMPARE_ON_X): `sign(λ_ax·d_b − λ_bx·d_a)·sign(d_a)·sign(d_b)`.
pub fn less_than_on_x_indirect(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    lt_dispatch(Axis::X, a, b)
}

/// Filtered (inexact) tiers only for [`less_than_on_x_indirect`].
pub fn less_than_on_x_indirect_filtered(a: &GenericPoint3D, b: &GenericPoint3D) -> Option<Sign> {
    lt_dispatch_filtered(Axis::X, a, b)
}

/// Exact tier only for [`less_than_on_x_indirect`].
pub fn less_than_on_x_indirect_exact(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    lt_dispatch_exact(Axis::X, a, b)
}

/// As [`less_than_on_x_indirect`] for the y coordinate.
pub fn less_than_on_y_indirect(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    lt_dispatch(Axis::Y, a, b)
}

/// Filtered (inexact) tiers only for [`less_than_on_y_indirect`].
pub fn less_than_on_y_indirect_filtered(a: &GenericPoint3D, b: &GenericPoint3D) -> Option<Sign> {
    lt_dispatch_filtered(Axis::Y, a, b)
}

/// Exact tier only for [`less_than_on_y_indirect`].
pub fn less_than_on_y_indirect_exact(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    lt_dispatch_exact(Axis::Y, a, b)
}

/// As [`less_than_on_x_indirect`] for the z coordinate.
pub fn less_than_on_z_indirect(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    lt_dispatch(Axis::Z, a, b)
}

/// Filtered (inexact) tiers only for [`less_than_on_z_indirect`].
pub fn less_than_on_z_indirect_filtered(a: &GenericPoint3D, b: &GenericPoint3D) -> Option<Sign> {
    lt_dispatch_filtered(Axis::Z, a, b)
}

/// Exact tier only for [`less_than_on_z_indirect`].
pub fn less_than_on_z_indirect_exact(a: &GenericPoint3D, b: &GenericPoint3D) -> Sign {
    lt_dispatch_exact(Axis::Z, a, b)
}

// =========================================================================
// Composite predicates (compositions over the generated primitives —
// no new polynomials)
// =========================================================================

/// Closed (boundary-inclusive) point-in-triangle test for `p` against
/// triangle `(a, b, c)`, all generic points, with `p` coplanar with the
/// triangle (the consumer contract — `retriangulate.rs` /
/// `aux_structure.rs` only query points lying on the triangle's plane).
///
/// Composite over the orient2d projections (no new polynomial): pick the
/// first axis-aligned projection where `orient2d(a, b, c) ≠ 0` (Attene
/// §4.5: such a projection exists iff the triangle is non-degenerate,
/// and for coplanar `p` the containment verdict is projection-
/// independent because all four orientation signs scale by the same
/// nonzero projection factor); `p` is inside iff each edge orientation
/// `orient2d(a,b,p)`, `orient2d(b,c,p)`, `orient2d(c,a,p)` is Zero
/// (boundary) or matches the triangle's own orientation sign. Returns
/// `false` for degenerate triangles and undefined implicit points.
pub fn point_in_triangle_indirect(
    p: &GenericPoint3D,
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    c: &GenericPoint3D,
) -> bool {
    for proj in PROJECTIONS {
        let st = orient2d_proj(proj, a, b, c);
        match st {
            Sign::Undefined => return false,
            Sign::Zero => continue, // collapsed projection (or degenerate triangle)
            _ => {}
        }
        for (e0, e1) in [(a, b), (b, c), (c, a)] {
            match orient2d_proj(proj, e0, e1, p) {
                Sign::Undefined => return false,
                Sign::Zero => {}
                s if s == st => {}
                _ => return false,
            }
        }
        return true;
    }
    false // triangle degenerate in every projection (collinear in 3D)
}

/// True iff the open segments `(a, b)` and `(p, q)` (coplanar — the
/// consumer contract in `enforce.rs`: all four are vertices of one
/// submesh plane) cross at a single point strictly interior to BOTH.
///
/// Composite (classical proper-crossing test, four orientations): in the
/// first projection where any of the four orientation signs is nonzero
/// (a collapsed projection of the common plane projects all four points
/// onto a line, making every sign Zero — so a projection with a nonzero
/// sign never lies, and for coplanar inputs the verdict is projection-
/// independent because all signs scale by the same nonzero factor), the
/// segments properly cross iff `p` and `q` lie strictly on opposite
/// sides of line `(a, b)` AND `a` and `b` lie strictly on opposite sides
/// of line `(p, q)`. Touching configurations (any sign Zero) and fully
/// collinear overlaps return `false`.
pub fn inner_segments_cross_indirect(
    a: &GenericPoint3D,
    b: &GenericPoint3D,
    p: &GenericPoint3D,
    q: &GenericPoint3D,
) -> bool {
    for proj in PROJECTIONS {
        let s1 = orient2d_proj(proj, a, b, p);
        let s2 = orient2d_proj(proj, a, b, q);
        let s3 = orient2d_proj(proj, p, q, a);
        let s4 = orient2d_proj(proj, p, q, b);
        if [s1, s2, s3, s4].contains(&Sign::Undefined) {
            return false;
        }
        if [s1, s2, s3, s4].iter().all(|s| *s == Sign::Zero) {
            continue; // collapsed projection (or fully collinear input)
        }
        return s1 != Sign::Zero
            && s2 != Sign::Zero
            && s3 != Sign::Zero
            && s4 != Sign::Zero
            && s1 == s2.flipped()
            && s3 == s4.flipped();
    }
    false // all four points collinear in 3D — no proper crossing
}

/// Shared core of the open/closed on-segment tests: collinearity gate
/// (all three projected orientations of `(p, v1, v2)` are Zero — the
/// three components of the cross product `(v1 − p) × (v2 − p)`), then
/// betweenness on the first separating axis (`less_than(v1, v2) ≠ 0`):
/// on a line, betweenness along any axis with a nonzero direction
/// component is equivalent to betweenness on the line.
fn point_on_segment_core(
    p: &GenericPoint3D,
    v1: &GenericPoint3D,
    v2: &GenericPoint3D,
    closed: bool,
) -> bool {
    // Collinearity gate.
    for proj in PROJECTIONS {
        match orient2d_proj(proj, p, v1, v2) {
            Sign::Zero => {}
            _ => return false, // non-collinear or Undefined
        }
    }
    // Betweenness on the first separating axis.
    for axis in AXES {
        let s = lt_dispatch(axis, v1, v2);
        match s {
            Sign::Undefined => return false,
            Sign::Zero => continue,
            _ => {}
        }
        let lo = lt_dispatch(axis, v1, p);
        let hi = lt_dispatch(axis, p, v2);
        if lo == Sign::Undefined || hi == Sign::Undefined {
            return false;
        }
        return if closed {
            (lo == s || lo == Sign::Zero) && (hi == s || hi == Sign::Zero)
        } else {
            lo == s && hi == s
        };
    }
    // Degenerate segment (v1 == v2): the closed segment contains exactly
    // that point; the open segment is empty.
    closed
        && AXES
            .iter()
            .all(|&axis| lt_dispatch(axis, p, v1) == Sign::Zero)
}

/// True iff `p` lies on the OPEN segment `(v1, v2)`: collinear with and
/// strictly between the endpoints (endpoints excluded). Symmetric in
/// `v1 ↔ v2` by construction (unlike the FFI reference, whose
/// explicit-explicit comparator branch is order-sensitive — the
/// documented sidecar EE limitation; consumers OR both orders to recover
/// exactly these semantics).
pub fn point_in_inner_segment_indirect(
    p: &GenericPoint3D,
    v1: &GenericPoint3D,
    v2: &GenericPoint3D,
) -> bool {
    point_on_segment_core(p, v1, v2, false)
}

/// True iff `p` lies on the CLOSED segment `[v1, v2]` (endpoints
/// included). Same collinearity gate and separating axis as
/// [`point_in_inner_segment_indirect`] with inclusive betweenness; a
/// degenerate segment (`v1 == v2`) contains exactly the point `p == v1`.
pub fn point_in_segment_indirect(
    p: &GenericPoint3D,
    v1: &GenericPoint3D,
    v2: &GenericPoint3D,
) -> bool {
    point_on_segment_core(p, v1, v2, true)
}

/// Approximate explicit coordinates of the LPI point of line `(p, q)`
/// and plane `(r, s, t)`, read back as interval-lambda midpoints
/// (`λ_mid / d_mid`). `None` when the interval midpoint of `d` is zero
/// (degenerate / parallel configuration — caller picks its own
/// fallback). Native equivalent of the FFI `lambda3d_lpi_interval`
/// consumer use in `arrangements/intersection_points.rs::lpi_approx`
/// (M7c swap target). Bookkeeping-quality output: NOT exact, never used
/// in predicates.
pub fn approx_lpi(p: Point3, q: Point3, r: Point3, s: Point3, t: Point3) -> Option<Point3> {
    let li = generated::lpi_lambda_iv(&p, &q, &r, &s, &t);
    let mid = |iv: Iv| (iv.lo + iv.hi) / 2.0;
    let d = mid(li.d);
    if d == 0.0 {
        return None;
    }
    Some(Point3::new(
        mid(li.l[0]) / d,
        mid(li.l[1]) / d,
        mid(li.l[2]) / d,
    ))
}

#[cfg(test)]
mod tests;
