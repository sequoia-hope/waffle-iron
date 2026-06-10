//! PR-CR-M7a oracle — semi-static filter soundness + non-vacuity for the
//! pure-Rust indirect `orient3d` (Attene §5.1 / Appendix A).
//!
//! **Soundness** (the load-bearing invariant): whenever the filtered f64
//! tier returns a definite sign, the exact rational tier must agree. A
//! single violation means the generated `δ(1)` constant under-estimates
//! the roundoff error — the fix is a MORE conservative generator, never
//! a looser test.
//!
//! **Non-vacuity**: on a generic (random-free, grid-derived) corpus the
//! filtered tier must resolve ≥ 90% of cases. An over-conservative δ
//! would silently destroy performance: every predicate call would fall
//! through to exact rational arithmetic, which is orders of magnitude
//! slower — the whole point of Attene's framework (paper §1, §7) is that
//! the FP tier almost always succeeds. A vacuous filter is "sound" but
//! useless, so we gate on the hit rate too.
//!
//! Runs with DEFAULT features (the indirect module is ungated, pure Rust).

mod indirect_common;

use cad_primitives::Point3;
use cherchi_rs::predicates::indirect::{
    orient3d_indirect, orient3d_indirect_exact, orient3d_indirect_filtered, GenericPoint3D, Sign,
};
use indirect_common::*;

/// Assert filter soundness over all 4-tuples drawn from `pool`;
/// returns (definite, total) counts for hit-rate reporting.
fn check_soundness(pool: &[GenericPoint3D], tuples: &[[usize; 4]], label: &str) -> (usize, usize) {
    let mut definite = 0usize;
    let mut total = 0usize;
    for &[a, b, c, d] in tuples {
        let args = (&pool[a], &pool[b], &pool[c], &pool[d]);
        let filtered = orient3d_indirect_filtered(args.0, args.1, args.2, args.3);
        let exact = orient3d_indirect_exact(args.0, args.1, args.2, args.3);
        total += 1;
        if let Some(s) = filtered {
            definite += 1;
            assert_eq!(
                s, exact,
                "{label}: FILTER SOUNDNESS VIOLATION on tuple [{a}, {b}, {c}, {d}]: \
                 filtered tier certified {s:?} but exact tier says {exact:?}"
            );
            assert_ne!(
                exact,
                Sign::Undefined,
                "{label}: filtered tier certified {s:?} on an UNDEFINED point \
                 (d == 0 must always defeat the d-sign filter) on [{a}, {b}, {c}, {d}]"
            );
        }
        // The public dispatcher must agree with the tier composition.
        let dispatched = orient3d_indirect(args.0, args.1, args.2, args.3);
        assert_eq!(
            dispatched, exact,
            "{label}: orient3d_indirect disagrees with the exact tier on [{a}, {b}, {c}, {d}]"
        );
    }
    (definite, total)
}

// ---------------------------------------------------------------------
// 1. Generic grid corpus: soundness + the ≥ 90% hit-rate gate
// ---------------------------------------------------------------------

#[test]
fn generic_corpus_soundness_and_hit_rate() {
    // 8 explicit + 8 LPI + 8 TPI generic points; ~600 mixed 4-tuples.
    let pool = mixed_pool(8, 8, 8, 1.0);
    let tuples = tuple_stream(pool.len(), 640);
    assert!(tuples.len() > 500, "corpus too small: {}", tuples.len());

    let (definite, total) = check_soundness(&pool, &tuples, "generic");

    // Non-vacuity gate. Measured over tuples with ≥ 1 implicit argument
    // ONLY (all-explicit tuples delegate to the adaptive CR6 predicate,
    // which always resolves — counting them would inflate the rate).
    let mut implicit_definite = 0usize;
    let mut implicit_total = 0usize;
    for &[a, b, c, d] in &tuples {
        let args = [&pool[a], &pool[b], &pool[c], &pool[d]];
        if args.iter().all(|p| p.is_explicit()) {
            continue;
        }
        implicit_total += 1;
        if orient3d_indirect_filtered(args[0], args[1], args[2], args[3]).is_some() {
            implicit_definite += 1;
        }
    }
    assert!(implicit_total > 300, "implicit subset too small");
    let rate = implicit_definite as f64 / implicit_total as f64;
    assert!(
        rate >= 0.90,
        "filtered tier resolved only {implicit_definite}/{implicit_total} = {rate:.3} \
         of generic implicit cases (gate: >= 0.90). An over-conservative δ silently \
         destroys performance — every call would pay exact rational arithmetic."
    );
    // Overall (incl. explicit) reported for the record via test output.
    println!("generic corpus: definite {definite}/{total}, implicit-only rate {rate:.3}");
}

// ---------------------------------------------------------------------
// 2. Magnitude sweeps: scale invariance of soundness, 1e-30 .. 1e30
// ---------------------------------------------------------------------

#[test]
fn magnitude_sweep_soundness() {
    // Powers of two (exact scaling — the shape is identical, so this
    // isolates the filter's β^k scaling) plus decimal scales (inexact —
    // adds rounding perturbation into the generators themselves).
    let scales: [f64; 8] = [
        2.0f64.powi(-100), // ~7.9e-31
        1e-30,
        1e-10,
        1.0,
        1e10,
        2.0f64.powi(50), // ~1.1e15
        1e20,
        1e30,
    ];
    for s in scales {
        let pool = mixed_pool(4, 4, 4, s);
        let tuples = tuple_stream(pool.len(), 200);
        let (definite, total) = check_soundness(&pool, &tuples, &format!("scale {s:e}"));
        println!("scale {s:e}: definite {definite}/{total}");
    }
}

// ---------------------------------------------------------------------
// 3. Near-degenerate families: coplanar ± 1..4 ulps
// ---------------------------------------------------------------------

/// Step `x` by `n` ulps (positive `n` → next_up).
fn ulps(x: f64, n: i32) -> f64 {
    let mut v = x;
    for _ in 0..n.abs() {
        v = if n > 0 { v.next_up() } else { v.next_down() };
    }
    v
}

#[test]
fn near_coplanar_explicit_perturbations() {
    // Four coplanar points in z = 1 (away from zero so 1 ulp is a normal-
    // range perturbation), then the query point's z stepped by 1..4 ulps
    // in each direction. Expected exact sign: z > 1 → d ABOVE the CCW
    // plane → Negative (Shewchuk convention); z < 1 → Positive; 0 ulps →
    // Zero. The filtered tier must either agree or return None — and the
    // exact tier must nail every case.
    let a = GenericPoint3D::explicit(Point3::new(0.0, 0.0, 1.0));
    let b = GenericPoint3D::explicit(Point3::new(1.0, 0.0, 1.0));
    let c = GenericPoint3D::explicit(Point3::new(0.0, 1.0, 1.0));
    for n in -4i32..=4 {
        let d = GenericPoint3D::explicit(Point3::new(0.3, 0.4, ulps(1.0, n)));
        let exact = orient3d_indirect_exact(&a, &b, &c, &d);
        let expected = match n.cmp(&0) {
            core::cmp::Ordering::Greater => Sign::Negative,
            core::cmp::Ordering::Less => Sign::Positive,
            core::cmp::Ordering::Equal => Sign::Zero,
        };
        assert_eq!(exact, expected, "exact tier wrong at {n} ulps");
        if let Some(s) = orient3d_indirect_filtered(&a, &b, &c, &d) {
            assert_eq!(s, expected, "FILTER SOUNDNESS VIOLATION at {n} ulps");
        }
        assert_eq!(orient3d_indirect(&a, &b, &c, &d), expected);
    }
}

#[test]
fn near_coplanar_implicit_perturbations() {
    // The same family with the perturbed query point replaced by an LPI
    // construction: the vertical line through (0.3, 0.4) intersected with
    // the (exactly representable) plane z = 1 ± k ulps.
    let a = GenericPoint3D::explicit(Point3::new(0.0, 0.0, 1.0));
    let b = GenericPoint3D::explicit(Point3::new(1.0, 0.0, 1.0));
    let c = GenericPoint3D::explicit(Point3::new(0.0, 1.0, 1.0));
    for n in -4i32..=4 {
        let z = ulps(1.0, n);
        let d = GenericPoint3D::lpi(
            Point3::new(0.3, 0.4, 0.0),
            Point3::new(0.3, 0.4, 2.0),
            // Generic triangle spanning the plane z = z.
            Point3::new(5.0, 0.5, z),
            Point3::new(6.0, 1.5, z),
            Point3::new(4.0, 3.0, z),
        );
        let exact = orient3d_indirect_exact(&a, &b, &c, &d);
        let expected = match n.cmp(&0) {
            core::cmp::Ordering::Greater => Sign::Negative,
            core::cmp::Ordering::Less => Sign::Positive,
            core::cmp::Ordering::Equal => Sign::Zero,
        };
        assert_eq!(exact, expected, "exact tier wrong at {n} ulps (LPI query)");
        if let Some(s) = orient3d_indirect_filtered(&a, &b, &c, &d) {
            assert_eq!(
                s, expected,
                "FILTER SOUNDNESS VIOLATION at {n} ulps (LPI query)"
            );
        }
    }
}

// ---------------------------------------------------------------------
// 4. Overflow window (PR-CR-M7b-fix F1): polynomial overflow must never
//    certify a sign (orient3d shares the generated filtered-tier shape
//    with the catalog families — see indirect_catalog_soundness.rs §5)
// ---------------------------------------------------------------------

#[test]
fn overflow_window_magnitude_sweep_soundness() {
    // Scales near the per-degree `β^k`-finite boundaries (the sweep
    // above stops at 1e30): ~8e7 for the degree-39 TTTT window, ~5e10
    // for the deep-TPI mid degrees, 1e40/1e52 mid-window, ~4e61 for the
    // shallow LPI/explicit instances. Generic pools (no engineered
    // cancellation) — a soundness backstop over the overflow windows.
    let scales: [f64; 6] = [1e7, 5e10, 1e40, 1e52, 2.0f64.powi(200), 5e60];
    for s in scales {
        let pool = mixed_pool(4, 4, 4, s);
        let tuples = tuple_stream(pool.len(), 150);
        let (definite, total) = check_soundness(&pool, &tuples, &format!("overflow scale {s:e}"));
        println!("overflow scale {s:e}: definite {definite}/{total}");
    }
}

#[test]
fn overflow_window_huge_lpi_family_soundness() {
    // LPI exactly at (p, p, 0) with `λ = d·(p, p, 0)`, `d = -2·p·s²`
    // (its `n` determinant vanishes), against explicit points at ~4e61 —
    // the same engineered family that produces the deterministic
    // orient2d wrong-sign reproduction in indirect_catalog_soundness.rs.
    for (kp, ks) in [(14.0, 43.0), (18.0, 43.0), (22.0, 40.0), (18.0, 44.0)] {
        let p = kp * 1e60;
        let s = ks * 1e60;
        let lpi = GenericPoint3D::lpi(
            Point3::new(p, p, -p),
            Point3::new(p, p, p),
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(s, -s, 0.0),
            Point3::new(s, s, 0.0),
        );
        for w in [4e61, -4e61, 2e61] {
            let b = GenericPoint3D::explicit(Point3::new(-1e61, 4.4e61, 0.0));
            let c = GenericPoint3D::explicit(Point3::new(-1e61 + w, 4.4e61 - w, 0.0));
            let d = GenericPoint3D::explicit(Point3::new(2e61, -3e61, 4e61));
            let exact = orient3d_indirect_exact(&lpi, &b, &c, &d);
            if let Some(f) = orient3d_indirect_filtered(&lpi, &b, &c, &d) {
                assert_eq!(
                    f, exact,
                    "F1 OVERFLOW SOUNDNESS VIOLATION (orient3d): p={p:e} s={s:e} w={w:e}"
                );
            }
            assert_eq!(
                orient3d_indirect(&lpi, &b, &c, &d),
                exact,
                "orient3d dispatcher wrong on overflow case p={p:e} s={s:e} w={w:e}"
            );
        }
    }
}

// ---------------------------------------------------------------------
// 5. Undefined family: d == 0 must never produce a definite filtered sign
// ---------------------------------------------------------------------

#[test]
fn undefined_points_never_pass_the_filter() {
    // LPI with the line exactly parallel to the plane, embedded in
    // otherwise-generic queries: exact must say Undefined and the filter
    // must never claim a sign.
    for k in 0..8u64 {
        let dz = coord(900 + k).abs() + 0.5;
        let bad = GenericPoint3D::lpi(
            Point3::new(coord(910 + k), coord(920 + k), dz),
            Point3::new(coord(930 + k), coord(940 + k), dz),
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        );
        let b = GenericPoint3D::explicit(point(700 + k));
        let c = GenericPoint3D::explicit(point(710 + k));
        let d = GenericPoint3D::explicit(point(720 + k));
        assert_eq!(
            orient3d_indirect(&bad, &b, &c, &d),
            Sign::Undefined,
            "undefined LPI (seed {k}) must yield Undefined"
        );
        assert_eq!(
            orient3d_indirect_filtered(&bad, &b, &c, &d),
            None,
            "filtered tier must not certify a sign for an undefined point (seed {k})"
        );
    }
}
