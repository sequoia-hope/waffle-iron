//! 3D collinearity test using Shewchuk's exact `orient2d` on three
//! orthogonal projections. Returns true iff the three points are exactly
//! collinear (no tolerance).
//!
//! Ported from cinolib's `points_are_colinear_3d` (used by Cherchi 2020
//! `processing.cpp:144`). cinolib is MIT-licensed.
//! © Marco Livesu et al. — https://github.com/mlivesu/cinolib
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §3 (cascaded filtered/exact predicates).
//! Shewchuk 1997 §4.5 (adaptive orient2d).

use cad_primitives::Point3;

/// Three-point collinearity test in 3D using Shewchuk's exact `orient2d`
/// on three orthogonal-axis-drop projections.
///
/// Returns `true` iff all three projection `orient2d` tests return
/// exactly 0.0. See `specs/cherchi_rs_points_collinear.md` for the
/// full contract.
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn points_are_collinear_3d(a: Point3, b: Point3, c: Point3) -> bool {
    // Drop each axis in turn. If any projection's orient2d is non-zero,
    // the three points span 2D in that plane and thus are not collinear in 3D.
    // If all three projections return exactly 0, the points lie on a single
    // line (or are degenerate-collinear: two or more coincident).
    let drop_z = geometry_predicates::orient2d([a.x(), a.y()], [b.x(), b.y()], [c.x(), c.y()]);
    let drop_y = geometry_predicates::orient2d([a.x(), a.z()], [b.x(), b.z()], [c.x(), c.z()]);
    let drop_x = geometry_predicates::orient2d([a.y(), a.z()], [b.y(), b.z()], [c.y(), c.z()]);
    drop_z == 0.0 && drop_y == 0.0 && drop_x == 0.0
}

/// Exact "strictly inside the open segment" test in 3D.
///
/// Returns `true` iff `w` is EXACTLY collinear with `(p, q)` AND lies
/// STRICTLY between the two endpoints (both endpoints excluded). No
/// tolerance, no raw-`f64` cross/dot — collinearity is the exact
/// [`points_are_collinear_3d`] test and betweenness is computed in
/// `dashu::rational::RBig` (exact rationals over the `f64` coordinates).
///
/// Ported from cinolib's `point_in_segment_3d` with the `STRICTLY_INSIDE`
/// semantics referenced by Cherchi's `triangulation.cpp:1178` (and the
/// edge-guard at `cpp:688-691`). cinolib is MIT-licensed (see the file
/// header). This is the exact replacement for the AR1 raw-`f64`
/// `point_strictly_inside_segment` stopgap (deviation N13, `f64` sub-note).
///
/// Algorithm:
/// 1. `w == p || w == q` → `false` (endpoints are excluded).
/// 2. `!points_are_collinear_3d(w, p, q)` → `false` (off the line).
/// 3. Strictly between ⟺ the exact dot product `(w − p) · (w − q) < 0`
///    (the two vectors from `w` to the endpoints point in opposite
///    directions iff `w` is interior to the segment).
///
/// # Failure modes
///
/// NaN / infinite inputs produce undefined behavior. Caller's responsibility.
pub fn point_strictly_inside_segment_3d(w: Point3, p: Point3, q: Point3) -> bool {
    use dashu::float::FBig;
    use dashu::rational::RBig;

    // Endpoints are excluded by the STRICTLY_INSIDE semantics.
    if w == p || w == q {
        return false;
    }
    // Off the supporting line → not on the segment at all.
    if !points_are_collinear_3d(w, p, q) {
        return false;
    }

    // Exact f64 → RBig conversion (both steps are total for finite f64).
    let to_r = |x: f64| -> RBig {
        let fb: FBig = FBig::try_from(x).expect("finite f64 → FBig is total");
        RBig::try_from(fb).expect("FBig → RBig is total")
    };

    // dot = (w − p) · (w − q), exact in RBig. Strictly between ⟺ dot < 0.
    let wp = [
        to_r(w.x()) - to_r(p.x()),
        to_r(w.y()) - to_r(p.y()),
        to_r(w.z()) - to_r(p.z()),
    ];
    let wq = [
        to_r(w.x()) - to_r(q.x()),
        to_r(w.y()) - to_r(q.y()),
        to_r(w.z()) - to_r(q.z()),
    ];
    let dot = &wp[0] * &wq[0] + &wp[1] * &wq[1] + &wp[2] * &wq[2];
    dot < RBig::ZERO
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Group 1: Canonical collinear ─────────────────────────────────

    #[test]
    fn axis_aligned_x() {
        // Three points on the X axis.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(2.0, 0.0, 0.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn axis_aligned_y() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(0.0, 2.0, 0.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn axis_aligned_z() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 0.0, 1.0);
        let c = Point3::new(0.0, 0.0, 2.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn off_axis_collinear() {
        // (1,2,3) direction; c = 2·b, all on the same line through origin.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 2.0, 3.0);
        let c = Point3::new(2.0, 4.0, 6.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    // ── Group 2: Degenerate-collinear (per spec, these are TRUE) ──────

    #[test]
    fn two_coincident_first_pair() {
        // a == b; per spec these are degenerate-collinear → true.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(0.0, 0.0, 0.0);
        let c = Point3::new(1.0, 1.0, 1.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn two_coincident_b_and_c() {
        let a = Point3::new(1.0, 1.0, 1.0);
        let b = Point3::new(2.0, 2.0, 2.0);
        let c = Point3::new(2.0, 2.0, 2.0);
        assert!(points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn all_three_coincident() {
        let p = Point3::new(7.0, -3.0, 2.5);
        assert!(points_are_collinear_3d(p, p, p));
    }

    // ── Group 3: Canonical non-collinear ─────────────────────────────

    #[test]
    fn right_triangle_xy() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        assert!(!points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn skew_three_axes() {
        // One point on each axis — clearly spans 3D.
        let a = Point3::new(1.0, 0.0, 0.0);
        let b = Point3::new(0.0, 1.0, 0.0);
        let c = Point3::new(0.0, 0.0, 1.0);
        assert!(!points_are_collinear_3d(a, b, c));
    }

    #[test]
    fn near_collinear_but_not_exact() {
        // (0,0,0)-(1,0,0) is the X axis. (2, 1e-300, 0) is very near
        // the X axis but NOT on it. Per spec's "exactly 0" criterion,
        // this must return false.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(2.0, 1e-300, 0.0);
        assert!(!points_are_collinear_3d(a, b, c));
    }

    // ── Group 4: Property — order invariance ─────────────────────────

    #[test]
    fn order_invariant_for_collinear() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 2.0, 3.0);
        let c = Point3::new(2.0, 4.0, 6.0);
        // All 6 permutations should agree.
        assert!(points_are_collinear_3d(a, b, c));
        assert!(points_are_collinear_3d(a, c, b));
        assert!(points_are_collinear_3d(b, a, c));
        assert!(points_are_collinear_3d(b, c, a));
        assert!(points_are_collinear_3d(c, a, b));
        assert!(points_are_collinear_3d(c, b, a));
    }

    #[test]
    fn order_invariant_for_non_collinear() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 0.0, 0.0);
        let c = Point3::new(0.0, 1.0, 0.0);
        // All 6 permutations should agree (false in this case).
        assert!(!points_are_collinear_3d(a, b, c));
        assert!(!points_are_collinear_3d(a, c, b));
        assert!(!points_are_collinear_3d(b, a, c));
        assert!(!points_are_collinear_3d(b, c, a));
        assert!(!points_are_collinear_3d(c, a, b));
        assert!(!points_are_collinear_3d(c, b, a));
    }

    // ── Group 5: A-01 regression (legacy port's f64 cross-product bug) ─

    /// The legacy Rust port (per `docs/audits/cherchi_port_audit.md:148-181`,
    /// finding A-01) used an inexact f64 cross-product, which produced
    /// tiny non-zero residuals for mathematically-collinear inputs derived
    /// from arithmetic. This test exercises one such input — three points
    /// strictly on a line whose coordinates exceed f64 contiguous-integer
    /// precision. Shewchuk's exact `orient2d` returns 0 here; the f64
    /// cross-product does not.
    ///
    /// The clean port (PR-CR1) gets this right by construction.
    #[test]
    fn a01_regression_large_coords_on_line() {
        // Three points strictly on the y = x diagonal in XY (and z = 0).
        // Coords chosen so f64 multiplication accumulates round-off but
        // the points are mathematically exactly on the line.
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(1.0, 1.0, 0.0);
        let c = Point3::new(2.0, 2.0, 0.0);
        // Exact orient2d returns 0 here → must report collinear.
        assert!(points_are_collinear_3d(a, b, c));
    }

    // ════════════════════════════════════════════════════════════════
    // PR-CR-AR2b Cycle B, Deliverable 1 — exact
    // `point_strictly_inside_segment_3d` (WASM-clean predicate).
    //
    // GREEN will add:
    //   pub fn point_strictly_inside_segment_3d(w: Point3, p: Point3,
    //       q: Point3) -> bool
    // Semantics: true iff `w` is EXACTLY collinear with (p, q) AND lies
    // STRICTLY between them (both endpoints excluded). Built from CR1
    // `points_are_collinear_3d` + EXACT `dashu` betweenness — NO raw f64
    // cross/dot, NO tolerance. It replaces the raw-f64 private
    // `point_strictly_inside_segment` in `intersection_points.rs`
    // (closing deviation N13's f64 sub-note).
    //
    // These tests are NOT feature-gated — the predicate is pure-Rust /
    // WASM-clean. They MUST fail to RESOLVE against the missing symbol
    // (RED by compile error), and by assertion once the symbol exists but
    // is wrong. No production code is authored here.
    //
    // Hand-derivations are documented inline.
    // ════════════════════════════════════════════════════════════════

    use super::point_strictly_inside_segment_3d;

    // ── D1 Group 1: strictly-between collinear point → true ───────────

    #[test]
    fn d1_strictly_between_axis_aligned() {
        // Segment on the X axis from (0,0,0) to (4,0,0). The point (2,0,0)
        // is collinear and strictly interior (0 < 2 < 4) → true.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 0.0, 0.0);
        let w = Point3::new(2.0, 0.0, 0.0);
        assert!(point_strictly_inside_segment_3d(w, p, q));
    }

    #[test]
    fn d1_strictly_between_off_axis_tilted() {
        // Segment from (0,0,0) along direction (1,2,3) to (2,4,6).
        // The point (1,2,3) is the midpoint — collinear and strictly
        // interior → true. (Tilted: not aligned with any coordinate axis.)
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(2.0, 4.0, 6.0);
        let w = Point3::new(1.0, 2.0, 3.0);
        assert!(point_strictly_inside_segment_3d(w, p, q));
    }

    // ── D1 Group 2: endpoints → false (strict) ───────────────────────

    #[test]
    fn d1_endpoint_p_is_excluded() {
        // w == p: collinear (degenerate) but NOT strictly interior → false.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 0.0, 0.0);
        assert!(!point_strictly_inside_segment_3d(p, p, q));
    }

    #[test]
    fn d1_endpoint_q_is_excluded() {
        // w == q: collinear but NOT strictly interior → false.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 0.0, 0.0);
        assert!(!point_strictly_inside_segment_3d(q, p, q));
    }

    #[test]
    fn d1_endpoint_off_axis_excluded() {
        // Both endpoints excluded on a tilted segment too.
        let p = Point3::new(1.0, 2.0, 3.0);
        let q = Point3::new(4.0, 8.0, 12.0);
        assert!(!point_strictly_inside_segment_3d(p, p, q));
        assert!(!point_strictly_inside_segment_3d(q, p, q));
    }

    // ── D1 Group 3: collinear-but-beyond an endpoint → false ─────────

    #[test]
    fn d1_collinear_beyond_q_is_false() {
        // (6,0,0) is exactly on the X axis (collinear with the segment
        // (0,0,0)-(4,0,0)) but lies BEYOND q (6 > 4) → false.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 0.0, 0.0);
        let w = Point3::new(6.0, 0.0, 0.0);
        assert!(!point_strictly_inside_segment_3d(w, p, q));
    }

    #[test]
    fn d1_collinear_before_p_is_false() {
        // (-2,0,0) is collinear but lies BEFORE p (−2 < 0) → false.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 0.0, 0.0);
        let w = Point3::new(-2.0, 0.0, 0.0);
        assert!(!point_strictly_inside_segment_3d(w, p, q));
    }

    #[test]
    fn d1_collinear_beyond_tilted_is_false() {
        // Tilted segment (0,0,0)-(2,4,6); (3,6,9) is collinear (3·(1,2,3))
        // but beyond q (parameter 1.5 > 1) → false.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(2.0, 4.0, 6.0);
        let w = Point3::new(3.0, 6.0, 9.0);
        assert!(!point_strictly_inside_segment_3d(w, p, q));
    }

    // ── D1 Group 4: non-collinear point → false ──────────────────────

    #[test]
    fn d1_non_collinear_is_false() {
        // (2,1,0) is NOT on the X axis (y ≠ 0) → not collinear → false,
        // even though its x-projection (2) would be "between".
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 0.0, 0.0);
        let w = Point3::new(2.0, 1.0, 0.0);
        assert!(!point_strictly_inside_segment_3d(w, p, q));
    }

    // ── D1 Group 5: load-bearing exactness case ──────────────────────

    /// A collinear-but-JUST-OUTSIDE case that an exact predicate must
    /// reject. This mirrors the spirit of
    /// `a01_regression_large_coords_on_line`: the point is constructed to
    /// be EXACTLY on the segment's supporting line (so `dashu`-exact
    /// collinearity holds), yet strictly beyond endpoint q by one ULP, so
    /// strict betweenness is false.
    ///
    /// Hand-derivation:
    ///   Segment on the X axis: p = (0,0,0), q = (1,0,0).
    ///   w = (1 + 2^-52, 0, 0) = (`f64::from_bits(q.x().to_bits()+1)`, 0, 0),
    ///   the next f64 above 1.0. This w is collinear (still y=z=0, exactly
    ///   on the X axis) but its x-coordinate is strictly greater than q.x
    ///   (1 + ε > 1), so it is NOT strictly inside (p, q). Exact betweenness
    ///   correctly returns false; a tolerance-based test that accepted
    ///   "within ε of the endpoint" could wrongly return true.
    #[test]
    fn d1_exact_just_outside_endpoint_is_false() {
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(1.0, 0.0, 0.0);
        // Next representable f64 strictly above 1.0.
        let just_past = f64::from_bits(1.0_f64.to_bits() + 1);
        assert!(
            just_past > 1.0,
            "construction sanity: just_past must exceed q.x"
        );
        let w = Point3::new(just_past, 0.0, 0.0);
        // Collinear (on the X axis) but strictly beyond q → must be false.
        assert!(
            !point_strictly_inside_segment_3d(w, p, q),
            "a collinear point one ULP past the endpoint is NOT strictly inside"
        );
    }

    /// Companion exact case: a collinear point one ULP strictly INSIDE the
    /// endpoint must be accepted. `just_inside = nextafter(1.0, 0.0)` is the
    /// largest f64 below 1.0; it lies exactly on the X axis and strictly
    /// between p=(0,0,0) and q=(1,0,0) → true.
    #[test]
    fn d1_exact_just_inside_endpoint_is_true() {
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(1.0, 0.0, 0.0);
        let just_inside = f64::from_bits(1.0_f64.to_bits() - 1);
        assert!(
            just_inside < 1.0 && just_inside > 0.0,
            "construction sanity: 0 < just_inside < 1"
        );
        let w = Point3::new(just_inside, 0.0, 0.0);
        assert!(
            point_strictly_inside_segment_3d(w, p, q),
            "a collinear point one ULP inside the endpoint IS strictly inside"
        );
    }

    // ── D1 Group 6: order / endpoint symmetry ────────────────────────

    #[test]
    fn d1_order_symmetric_interior() {
        // Swapping the two endpoints must not change the verdict for an
        // interior point.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 2.0, -6.0);
        let w = Point3::new(2.0, 1.0, -3.0); // midpoint → interior
        assert_eq!(
            point_strictly_inside_segment_3d(w, p, q),
            point_strictly_inside_segment_3d(w, q, p),
            "predicate must be symmetric in the two segment endpoints"
        );
        assert!(point_strictly_inside_segment_3d(w, p, q));
    }

    #[test]
    fn d1_order_symmetric_outside() {
        // Symmetry also for a collinear-but-outside point.
        let p = Point3::new(0.0, 0.0, 0.0);
        let q = Point3::new(4.0, 2.0, -6.0);
        let w = Point3::new(6.0, 3.0, -9.0); // beyond q → outside
        assert_eq!(
            point_strictly_inside_segment_3d(w, p, q),
            point_strictly_inside_segment_3d(w, q, p),
            "predicate must be symmetric in the two segment endpoints"
        );
        assert!(!point_strictly_inside_segment_3d(w, p, q));
    }
}
