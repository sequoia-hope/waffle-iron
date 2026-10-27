//! Triangle-normal max-component selection for 2D projection axis choice.
//!
//! Ported from Cherchi 2020's `maxComponentInTriangleNormal` family
//! (`implicit_point.hpp:937-1029` per audit A-02).
//! Cherchi 2020 is MIT-licensed.
//! © 2020 Gianmarco Cherchi, Marco Livesu, Riccardo Scateni, Marco Attene
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §3 (cascaded filtered/exact predicates).
//! Shewchuk 1997 §2.1, §4.5 (adaptive predicates with error bounds).
//!
//! **Filtered+exact cascade**: this file establishes the pattern for
//! subsequent indirect predicates. `max_component_filtered` is the f64
//! fast-path returning `Option` (`None` = uncertain → fall back to
//! exact); `max_component_exact` uses dashu rationals for the
//! definitive answer. Soundness: if the filtered version returns
//! `Some(axis)`, that axis is provably correct and its exact normal
//! component is strictly nonzero.
//!
//! **Certified semi-static error bound (ledger N99; Attene 2020 App. A /
//! Meyer & Pion 2008 FPG)**:
//! Each component of `n = (b - a) × (c - a)` has the form `d0 * d1 - d2 * d3`
//! where `d_k = fl(v1_k - v0_k)` are six coordinate differences. Per Meyer &
//! Pion 2008 §2 ("Translation Filter", `refs/text/meyer_pion2008_fpg.txt:167-189`)
//! and Attene 2020 App. A (`refs/text/attene-predicates.txt:579-585`, also
//! applied in Cherchi 2020 App. A `refs/text/mesh_arrangement.txt:1020-1022`;
//! note Cherchi §4.2.1's prose at `mesh_arrangement.txt:298-299` misstated `δ`
//! as the max over raw coordinates, which under-bounds opposite-sign edge
//! differences by up to `4×`), the scale factor `δ` MUST be taken over the
//! six coordinate differences `max(|b - a|, |c - a|)`:
//! - For finite IEEE-754 binary64 subtraction without overflow, `|d_k_exact - d_k|
//!   <= u * |d_k| <= u * δ` (`u = 2^-53`, zero subnormal underflow loss on
//!   subtraction).
//! - Propagating `Sfe::translation_input()` through `d0 * d1 - d2 * d3`
//!   (degree 2) via FPG's outward-rounded `Sfe` algebra (`predicate-gen/src/fpg.rs:69-126`,
//!   `meyer_pion2008_fpg.txt:161-163, 658-705`, multiplying by `(1 + 2^-52)^4`
//!   toward `+∞` to absorb threshold evaluation roundoff) gives
//!   `DELTA_CROSS = 8.886121005691226e-16` (strictly bounding both pure IEEE
//!   `(8u + 4u^2)/(1 - u)^4 = 8.88178419700126e-16` and Cherchi §4.2.1's
//!   `8.88395e-16`).
//! - Adding `SUBNORMAL_GUARD = 1e-300` (`meyer_pion2008_fpg.txt:265-299`,
//!   matching `predicates/indirect/mod.rs:265`) covers `δ^2` underflow into
//!   subnormals (`<= (3 + 2u) * 2^-1075 < 2e-323`), so for every finite `n_k`:
//!   `|fl(n_k) - n_k_exact| <= eps_n = fl(DELTA_CROSS * fl(δ * δ) + SUBNORMAL_GUARD)`.
//! - Because BOTH `|fl(n_i)|` and `|fl(n_j)|` carry error `<= eps_n`, certifying
//!   `|n_i|_exact > |n_j|_exact` requires the doubled margin `margin = 2 * eps_n`:
//!   in binary64 RN, for finite `A, B, M >= 0` with `S = fl(B + M)` finite,
//!   `A > S` implies `A >= S.next_up() > B + M` in ℝ, so `|n_i|_exact >=
//!   A - eps_n > B + eps_n >= |n_j|_exact` AND (since `B >= 0, eps_n > 0`)
//!   `|n_i|_exact > eps_n > 0`.

use cad_primitives::Point3;

/// Outward-rounded FPG semi-static filter constant `Sfe::delta(2)` for a 2×2
/// cross-product component `(b_u - a_u)(c_v - a_v) - (b_v - a_v)(c_u - a_u)` on
/// translation-difference inputs (`predicate-gen/src/fpg.rs:69-126`, Meyer &
/// Pion 2008 §2 & App. B `refs/text/meyer_pion2008_fpg.txt:161-189, 658-705`,
/// Attene 2020 App. A `refs/text/attene-predicates.txt:579-585`).
const DELTA_CROSS: f64 = 8.886121005691226e-16;

/// Additive underflow guard for `δ^2` in the IEEE-754 subnormal range
/// (`refs/text/meyer_pion2008_fpg.txt:285-299`, matching
/// `crate::predicates::indirect::support::SUBNORMAL_GUARD`).
const SUBNORMAL_GUARD: f64 = 1e-300;

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
/// Implements Cherchi 2020 §4.2.1's filtered+exact cascade: tries the
/// certified f64 fast-path first, falls back to dashu-rational arithmetic when
/// the filtered version cannot decide confidently. For any non-collinear
/// triangle, the selected axis always has a strictly nonzero exact normal
/// component (checked via `debug_assert!`).
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Callers must pass
/// non-collinear triangles (filter degenerate triangles via
/// [`super::points_are_collinear_3d`] first).
pub fn max_component_in_triangle_normal(a: Point3, b: Point3, c: Point3) -> Axis {
    let axis = match max_component_filtered(a, b, c) {
        Some(axis) => axis,
        None => max_component_exact(a, b, c),
    };
    #[cfg(debug_assertions)]
    debug_assert!(
        exact_normal_component_is_nonzero(a, b, c, axis),
        "max_component_in_triangle_normal selected {axis:?} with zero exact normal component \
         (collinear triangle or filter bug): a={a:?}, b={b:?}, c={c:?}"
    );
    axis
}

/// Exact rational check (debug-assertions only) that the selected `axis`
/// component of `(b - a) × (c - a)` is strictly nonzero.
#[cfg(debug_assertions)]
fn exact_normal_component_is_nonzero(a: Point3, b: Point3, c: Point3, axis: Axis) -> bool {
    use dashu::float::FBig;
    use dashu::rational::RBig;

    let to_r = |x: f64| -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    };
    let (u0, v0, u1, v1, u2, v2) = match axis {
        Axis::X => (a.y(), a.z(), b.y(), b.z(), c.y(), c.z()),
        Axis::Y => (a.z(), a.x(), b.z(), b.x(), c.z(), c.x()),
        Axis::Z => (a.x(), a.y(), b.x(), b.y(), c.x(), c.y()),
    };
    let comp = (to_r(u1) - to_r(u0)) * (to_r(v2) - to_r(v0))
        - (to_r(v1) - to_r(v0)) * (to_r(u2) - to_r(u0));
    comp != RBig::ZERO
}

/// Filtered (f64) cascade primitive — certified semi-static fast path.
///
/// Returns `Some(axis)` when the f64 cross-product provably identifies the
/// strict max-magnitude axis: `|fl(n_i)| > |fl(n_j)| + 2 * eps_n` for both
/// other axes `j != i`, where `eps_n` bounds per-component roundoff from the
/// six edge-coordinate differences `(b - a)` and `(c - a)`. Returns `None` on
/// near-ties, collinear/zero-normal inputs, subnormal ambiguity, or overflow,
/// signaling that the caller must fall back to [`max_component_exact`].
///
/// Soundness: if `Some(axis)` is returned, `axis` is provably the unique
/// maximum-magnitude normal axis and its exact normal component is nonzero.
pub(crate) fn max_component_filtered(a: Point3, b: Point3, c: Point3) -> Option<Axis> {
    // Edge difference vectors d_b = b - a, d_c = c - a; cross product n = d_b × d_c.
    let (bx_ax, by_ay, bz_az) = (b.x() - a.x(), b.y() - a.y(), b.z() - a.z());
    let (cx_ax, cy_ay, cz_az) = (c.x() - a.x(), c.y() - a.y(), c.z() - a.z());
    let nx = by_ay * cz_az - bz_az * cy_ay;
    let ny = bz_az * cx_ax - bx_ax * cz_az;
    let nz = bx_ax * cy_ay - by_ay * cx_ax;
    let (ax, ay, az) = (nx.abs(), ny.abs(), nz.abs());

    // Semi-static filter scale δ over the SIX coordinate differences (Attene
    // 2020 App. A `attene-predicates.txt:579-585`; FPG translation filter
    // `meyer_pion2008_fpg.txt:167-189`; Cherchi 2020 App. A
    // `mesh_arrangement.txt:1020-1022`).
    let max_diff = [bx_ax, by_ay, bz_az, cx_ax, cy_ay, cz_az]
        .into_iter()
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    let eps_n = DELTA_CROSS * (max_diff * max_diff) + SUBNORMAL_GUARD;
    // Doubled margin bounds roundoff on BOTH compared components (|n_i| and |n_j|)
    // and guarantees |n_i|_exact > eps_n > 0.
    let margin = 2.0 * eps_n;
    if !max_diff.is_finite()
        || !margin.is_finite()
        || !ax.is_finite()
        || !ay.is_finite()
        || !az.is_finite()
    {
        return None;
    }

    if ax > ay + margin && ax > az + margin {
        return Some(Axis::X);
    }
    if ay > ax + margin && ay > az + margin {
        return Some(Axis::Y);
    }
    if az > ax + margin && az > ay + margin {
        return Some(Axis::Z);
    }
    None
}

/// Exact (dashu rational) cascade primitive — slow but definitive path.
///
/// Converts each f64 coordinate to an arbitrary-precision rational (RBig),
/// computes the cross product in exact arithmetic, and returns the axis
/// with largest |n_i|. On exact ties, deterministic tiebreak: X > Y > Z.
pub(crate) fn max_component_exact(a: Point3, b: Point3, c: Point3) -> Axis {
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // Convert each f64 coord to exact rational. Path: f64 → FBig (exact for
    // finite f64 via dashu_float::convert) → RBig (exact via TryFrom<FBig>).
    let to_r = |x: f64| -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    };
    let ax = to_r(a.x());
    let ay = to_r(a.y());
    let az = to_r(a.z());
    let bx = to_r(b.x());
    let by = to_r(b.y());
    let bz = to_r(b.z());
    let cx = to_r(c.x());
    let cy = to_r(c.y());
    let cz = to_r(c.z());

    // n = (b - a) × (c - a), computed in exact rationals.
    let bx_ax = &bx - &ax;
    let by_ay = &by - &ay;
    let bz_az = &bz - &az;
    let cx_ax = &cx - &ax;
    let cy_ay = &cy - &ay;
    let cz_az = &cz - &az;

    let nx = &by_ay * &cz_az - &bz_az * &cy_ay;
    let ny = &bz_az * &cx_ax - &bx_ax * &cz_az;
    let nz = &bx_ax * &cy_ay - &by_ay * &cx_ax;

    // Compare |n_i|² to avoid abs() on RBig (squares are non-negative + exact).
    let nx_sq = &nx * &nx;
    let ny_sq = &ny * &ny;
    let nz_sq = &nz * &nz;

    // Deterministic tiebreak on exact equality: X > Y > Z.
    if nx_sq >= ny_sq && nx_sq >= nz_sq {
        Axis::X
    } else if ny_sq >= nz_sq {
        Axis::Y
    } else {
        Axis::Z
    }
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

    // ── Group 7: N99 / TC-1 soundness pins (edge-diff filter + subnormal) ─

    /// Ledger N99 / audit TC-1 (`docs/audits/cherchi2020_lit_check.md` C4.2-02):
    /// O(1) vertical triangle with opposite-sign vertices where `(b - a)` and
    /// `(c - a)` both have exact XY slope `9/7`, so exact `n_z == 0` identically
    /// while exact `n = (27/64 u + 135/4 u^2, -21/64 u - 105/4 u^2, 0)` has
    /// dominant axis `Axis::X` (`|n_x| > |n_y| > |n_z| = 0`).
    ///
    /// A filter that scales `max_var` from raw coordinates instead of edge
    /// differences and uses a single-`eps` margin mis-certifies `Some(Axis::Z)`,
    /// projecting the triangle onto XY where its exact 2D area is identically 0.
    #[test]
    fn n99_tc1_o1_vertical_triangle_never_selects_zero_area_xy_plane() {
        use cad_primitives::Point2;

        let u = 0.5 * f64::EPSILON; // 2^-53
        let a = Point3::new(-0.65625, -0.84375, 0.0);
        let b = Point3::new(0.65625 + 105.0 * u, 0.84375 + 135.0 * u, 0.0);
        let c = Point3::new(0.65625 + 63.0 * u, 0.84375 + 81.0 * u, 0.25 * u);

        // Confirm the XY projection (Axis::Z) has exact zero 2D area, while YZ
        // (Axis::X) has nonzero exact 2D area.
        assert_eq!(
            crate::predicates::orient2d(
                Point2::new(a.x(), a.y()),
                Point2::new(b.x(), b.y()),
                Point2::new(c.x(), c.y()),
            ),
            crate::predicates::Sign::Zero,
            "XY projection has exact zero area on TC-1"
        );
        assert_ne!(
            crate::predicates::orient2d(
                Point2::new(a.y(), a.z()),
                Point2::new(b.y(), b.z()),
                Point2::new(c.y(), c.z()),
            ),
            crate::predicates::Sign::Zero,
            "YZ projection has nonzero exact area on TC-1"
        );

        assert_eq!(max_component_exact(a, b, c), Axis::X);
        assert_eq!(max_component_filtered(a, b, c), None);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::X);
    }

    /// Ledger N99 / audit TC-1 subnormal witness: edge differences of scale
    /// `2^-536` place products in the IEEE-754 subnormal range (`~2^-1072`),
    /// where `max_diff * max_diff` loses precision and an unguarded filter
    /// threshold underflows to `0.0`. Exact `|n_x| = |n_y| = 1.375 * 2^-1074 >
    /// |n_z| = 1.0 * 2^-1074` (tiebreak `X > Y > Z` → `Axis::X`), whereas f64
    /// subnormal rounding rounds `|n_z|` up to `2 * 2^-1074` and `|n_x|, |n_y|`
    /// down to `1 * 2^-1074`.
    #[test]
    fn n99_tc1_subnormal_cross_product_matches_exact_axis() {
        let s = f64::from_bits((1023u64 - 536) << 52); // 2^-536
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(s, s, 0.0);
        let c = Point3::new(1.125 * s, 1.375 * s, (11.0 / 32.0) * s);
        assert_eq!(max_component_exact(a, b, c), Axis::X);
        assert_eq!(max_component_filtered(a, b, c), None);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::X);
    }

    #[test]
    fn n99_overflow_edge_difference_falls_back_to_exact() {
        // Coordinates near ±1e308 overflow edge differences / products in f64;
        // the finiteness guard must reject the filtered path (`None`) and defer
        // to exact rationals (`Axis::Z`).
        let a = Point3::new(-1e308, -1e308, 0.0);
        let b = Point3::new(1e308, -1e308, 0.0);
        let c = Point3::new(-1e308, 1e308, 0.0);
        assert_eq!(max_component_filtered(a, b, c), None);
        assert_eq!(max_component_in_triangle_normal(a, b, c), Axis::Z);
    }

    #[test]
    fn n99_collinear_triangle_filtered_returns_none() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 2.0, 3.0);
        let c = Point3::new(2.0, 4.0, 6.0);
        assert_eq!(max_component_filtered(a, b, c), None);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "zero exact normal component")]
    fn n99_debug_assert_catches_zero_exact_normal_component() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 2.0, 3.0);
        let c = Point3::new(2.0, 4.0, 6.0);
        let _ = max_component_in_triangle_normal(a, b, c);
    }
}
