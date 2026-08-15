//! I5-0 — §4.3.4 seam-density census unit oracles
//! (spec `specs/yang_441_trim_cdt_construction.md` §4-I5).
//!
//! Pins: (a) `conic_eval` is the exact frame-consistent inverse of
//! `conic_param` for Circle and Ellipse; (b) `paper_chain_metrics` and the
//! N58 predicate agree (the factoring changed no decision); (c) the census
//! measures a sparse circle chain as failing the paper's criterion with the
//! analytically expected implied insertion count, and accepts a dense one;
//! (d) the atan2 branch-cut pair is censused as its SHORT arc.

use crate::geom::conic_eval;
use crate::stage4_construct::census_conic_seam_density;
use crate::stage4_correct::{conic_param, paper_chain_metrics, paper_chain_sample_redundant};
use crate::{Curve, Point3, Vector3};

fn unit_circle() -> Curve {
    Curve::Circle {
        center: Point3::new(0.0, 0.0, 0.0),
        normal: Vector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    }
}

fn tilted_ellipse() -> Curve {
    // Deliberately non-axis-aligned so the frame convention is exercised.
    Curve::Ellipse {
        center: Point3::new(0.3, -0.2, 0.7),
        normal: Vector3::new(0.0, 1.0, 1.0),
        major_axis: Vector3::new(2.0, 0.0, 0.0),
        major_radius: 0.5,
        minor_radius: 0.2,
    }
}

#[test]
fn conic_eval_round_trips_conic_param_on_the_circle() {
    let c = unit_circle();
    for k in 0..17 {
        let t = -3.1 + 0.39 * f64::from(k);
        let p = conic_eval(&c, t).expect("circle evals");
        let t_back = conic_param(&c, p).expect("on-curve point has a parameter");
        // Wrapped comparison: atan2 returns (−π, π].
        let mut d = t_back - t;
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d <= -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        assert!(d.abs() < 1e-12, "circle param round-trip: t={t} d={d:.3e}");
    }
}

#[test]
fn conic_eval_round_trips_conic_param_on_a_tilted_ellipse() {
    let e = tilted_ellipse();
    for k in 0..17 {
        let t = -3.1 + 0.39 * f64::from(k);
        let p = conic_eval(&e, t).expect("ellipse evals");
        let t_back = conic_param(&e, p).expect("on-curve point has a parameter");
        let mut d = t_back - t;
        while d > std::f64::consts::PI {
            d -= 2.0 * std::f64::consts::PI;
        }
        while d <= -std::f64::consts::PI {
            d += 2.0 * std::f64::consts::PI;
        }
        assert!(d.abs() < 1e-12, "ellipse param round-trip: t={t} d={d:.3e}");
    }
}

#[test]
fn conic_eval_declines_non_conics() {
    assert!(conic_eval(&Curve::LineSegment, 0.5).is_none());
}

#[test]
fn metrics_agree_with_the_n58_predicate() {
    // Across redundant, non-redundant, and degenerate-leg triples the
    // predicate must equal the threshold conjunction applied to the metrics.
    let triples: &[([f64; 3], [f64; 3], [f64; 3])] = &[
        // Tiny straight step at the origin: redundant.
        ([0.0; 3], [1e-9, 0.0, 0.0], [2e-9, 0.0, 0.0]),
        // Long chord: l fails.
        ([0.0; 3], [0.5, 0.0, 0.0], [1.0, 0.0, 0.0]),
        // Short but bent 90°: alpha fails.
        ([0.0; 3], [5e-5, 0.0, 0.0], [5e-5, 5e-5, 0.0]),
        // Sagitta above the h bound at scale ~1.
        ([1.0, 0.0, 0.0], [1.0, 5e-5, 1e-4], [1.0, 1e-4, 0.0]),
        // Degenerate leg: m coincident with a.
        ([1.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1e-9]),
    ];
    for &(a, m, b) in triples {
        let mt = paper_chain_metrics(a, m, b);
        let expect = mt.l < mt.dp * 1e3
            && mt.h < mt.dp * 1e2
            && (mt.degenerate || mt.alpha < std::f64::consts::PI / 18.0);
        assert_eq!(
            paper_chain_sample_redundant(a, m, b),
            expect,
            "predicate/metrics divergence on {a:?} {m:?} {b:?}: {mt:?}"
        );
    }
}

#[test]
fn census_measures_a_sparse_quarter_arc_chain_as_failing() {
    // Four samples a quarter turn apart on the unit circle: every pair is
    // far outside h/l/α (quarter-arc sagitta ≈ 0.29, chord ≈ 1.41, turn 45°).
    let c = unit_circle();
    let ts = [0.0, 0.5, 1.0, 1.5].map(|k| k * std::f64::consts::FRAC_PI_2);
    let verts: Vec<Point3> = ts
        .iter()
        .map(|&t| conic_eval(&c, t).expect("on-circle"))
        .collect();
    let chain: Vec<u32> = (0..4).collect();
    let census =
        census_conic_seam_density(&verts, &chain, &c, false).expect("circle chain censuses");
    assert_eq!(census.pairs, 3);
    assert_eq!(
        census.fail_any, 3,
        "every sparse pair must fail: {census:?}"
    );
    assert_eq!(census.fail_h, 3);
    assert_eq!(census.fail_l, 3);
    assert_eq!(census.fail_alpha, 3);
    // Analytic expectation: subdivision halves each quarter arc until the
    // l-term dominates (h and α admit much earlier). An interval of arc θ_d
    // = (π/2)/2^d tests legs ≈ θ_d/2; redundant when θ_d/2 < d_p·10³
    // (≈ 2–4e-4 at scale ≲1) → terminal depth ≈ 12 → ~2^12−1 ≈ 4k inserts
    // per pair upper bound (measured 9084 over 3 pairs — the varying
    // scale-relative d_p along the arc admits some leaves a level early).
    // Pin the order-of-magnitude band, not the exact count.
    assert!(
        !census.capped,
        "unit-circle simulation must not hit guards: {census:?}"
    );
    assert!(
        (3_000..40_000).contains(&census.implied_inserts),
        "implied inserts far from the analytic band: {census:?}"
    );
}

#[test]
fn census_accepts_a_paper_dense_chain() {
    // Samples 1e-4 rad apart on the unit circle: leg ≈ 1e-4 < d_p·10³
    // (≈2e-4), sagitta ≈ 1.2e-9 ≪ d_p·10², turn ≈ 0.006° ≪ 10°.
    let c = unit_circle();
    let verts: Vec<Point3> = (0..5)
        .map(|k| conic_eval(&c, 1e-4 * f64::from(k)).expect("on-circle"))
        .collect();
    let chain: Vec<u32> = (0..5).collect();
    let census =
        census_conic_seam_density(&verts, &chain, &c, false).expect("circle chain censuses");
    assert_eq!(census.pairs, 4);
    assert_eq!(census.fail_any, 0, "dense chain must pass: {census:?}");
    assert_eq!(census.implied_inserts, 0);
    assert!(!census.capped);
}

#[test]
fn census_takes_the_short_arc_across_the_branch_cut() {
    // Two samples straddling atan2's ±π cut, 0.2 rad apart the SHORT way.
    // A long-way census would report a ~2π−0.2 arc (millions of implied
    // inserts); the short way is a small bounded count.
    let c = unit_circle();
    let (t0, t1) = (std::f64::consts::PI - 0.1, -std::f64::consts::PI + 0.1);
    let verts: Vec<Point3> = [t0, t1]
        .iter()
        .map(|&t| conic_eval(&c, t).expect("on-circle"))
        .collect();
    let chain: Vec<u32> = vec![0, 1];
    let census =
        census_conic_seam_density(&verts, &chain, &c, false).expect("circle chain censuses");
    assert_eq!(census.pairs, 1);
    assert_eq!(
        census.fail_any, 1,
        "0.2 rad arc still fails l/h: {census:?}"
    );
    // Short-arc leg ≈ 0.2/2^d ≤ ~2.6e-4 → d = 10 → 1023 inserts; the long
    // arc would need ≥ 2^14. Pin the discriminating band.
    assert!(
        census.implied_inserts < 5_000,
        "census took the long arc: {census:?}"
    );
    assert!(!census.capped);
}

// ---- I5-1: the refine + splice primitives -------------------------------

use crate::stage4_construct::refine_conic_chain;
use crate::stage4_splice::{splice_refined_run_into_cycles, Side};

#[test]
fn refine_densifies_a_short_arc_to_the_paper_criterion() {
    // One 0.1-rad pair on the unit circle: sparse today, refinable well
    // under the 4096 cap.
    let c = unit_circle();
    let ts = [0.0, 0.1];
    let verts: Vec<Point3> = ts
        .iter()
        .map(|&t| conic_eval(&c, t).expect("on-circle"))
        .collect();
    let chain: Vec<u32> = vec![0, 1];
    let (pts, refined) =
        refine_conic_chain(&verts, &chain, &c, 2, 4096).expect("short arc refines");
    assert!(!pts.is_empty(), "0.1 rad must demand inserts");
    assert_eq!(refined[0], 0);
    assert_eq!(*refined.last().unwrap(), 1);
    assert_eq!(refined.len(), 2 + pts.len());
    // Every insert lies exactly on the circle and the refined chain is
    // parameter-monotone.
    let mut pool = verts.clone();
    pool.extend_from_slice(&pts);
    for p in &pts {
        let r = (p.x() * p.x() + p.y() * p.y()).sqrt();
        assert!(
            (r - 1.0).abs() < 1e-12 && p.z().abs() < 1e-12,
            "off-circle insert {p:?}"
        );
    }
    let params: Vec<f64> = refined
        .iter()
        .map(|&v| conic_param(&c, pool[v as usize]).expect("on-curve"))
        .collect();
    assert!(
        params.windows(2).all(|w| w[1] > w[0]),
        "refined chain not parameter-monotone: {params:?}"
    );
    // The refined chain passes the paper's own acceptance.
    let census =
        census_conic_seam_density(&pool, &refined, &c, false).expect("refined chain censuses");
    assert_eq!(census.fail_any, 0, "refined chain still fails: {census:?}");
    assert_eq!(census.implied_inserts, 0);
}

#[test]
fn refine_declines_over_budget_and_skips_a_dense_chain() {
    let c = unit_circle();
    // Quarter arc: needs ~3k inserts — a budget of 10 must decline.
    let sparse: Vec<Point3> = [0.0, std::f64::consts::FRAC_PI_2]
        .iter()
        .map(|&t| conic_eval(&c, t).expect("on-circle"))
        .collect();
    assert!(refine_conic_chain(&sparse, &[0, 1], &c, 2, 10).is_none());
    // Paper-dense chain: refines to ZERO inserts, chain unchanged.
    let dense: Vec<Point3> = (0..3)
        .map(|k| conic_eval(&c, 1e-4 * f64::from(k)).expect("on-circle"))
        .collect();
    let (pts, refined) = refine_conic_chain(&dense, &[0, 1, 2], &c, 3, 4096).expect("dense ok");
    assert!(pts.is_empty());
    assert_eq!(refined, vec![0, 1, 2]);
}

#[test]
fn splice_replaces_the_run_forward_and_reversed() {
    // Forward: cycle traverses the run as [1,2,3].
    let cycles = vec![vec![0, 1, 2, 3, 4, 5]];
    let ordered = vec![1, 2, 3];
    let refined = vec![1, 10, 2, 11, 3];
    let out = splice_refined_run_into_cycles(&cycles, &ordered, &refined, Side::A)
        .expect("forward splice");
    assert_eq!(out, vec![vec![1, 10, 2, 11, 3, 4, 5, 0]]);
    // Reversed: cycle traverses the run as [3,2,1] — the splice must insert
    // the refined chain in the cycle's own direction.
    let cycles_r = vec![vec![0, 3, 2, 1, 5]];
    let out_r = splice_refined_run_into_cycles(&cycles_r, &ordered, &refined, Side::A)
        .expect("reversed splice");
    assert_eq!(out_r, vec![vec![3, 11, 2, 10, 1, 5, 0]]);
}

#[test]
fn splice_leaves_non_carriers_and_refuses_scattered_runs() {
    // Non-carrier cycle (shares one junction vertex only) passes through.
    let cycles = vec![vec![0, 1, 2, 3, 4, 5], vec![1, 8, 9]];
    let out = splice_refined_run_into_cycles(&cycles, &[1, 2, 3], &[1, 10, 3], Side::A)
        .expect("splice with bystander");
    assert_eq!(out[1], vec![1, 8, 9]);
    // Scattered membership (run not contiguous in the cycle) refuses.
    let scattered = vec![vec![1, 8, 2, 9, 3]];
    assert!(splice_refined_run_into_cycles(&scattered, &[1, 2, 3], &[1, 10, 3], Side::A).is_err());
}
