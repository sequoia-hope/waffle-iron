//! Integration tests for cascade instrumentation counters (Phase E).
//!
//! These tests verify that the global cascade outcome counters in
//! `kernel_fork::healing` are correctly incremented by real boolean operations.
//!
//! Because the counters are global atomics, these tests MUST run serially
//! (--test-threads=1) or as a single consolidated test to avoid races.
//! We consolidate into a single test to guarantee serial execution.

use kernel_fork::healing::{cascade_stats, reset_cascade_stats, CascadeStats};
use test_harness::ModelBuilder;

// ── Helpers ─────────────────────────────────────────────────────────────

/// Run a simple boolean union (non-coplanar cube + offset boss).
fn run_simple_union() {
    let mut m = ModelBuilder::truck();
    m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
        .unwrap();
    m.extrude("cube", "base_sk", 10.0).unwrap();

    // Boss well inside top face to avoid edge coincidence
    m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 7., 7.)
        .unwrap();
    m.extrude("boss", "boss_sk", 5.0).unwrap();

    m.boolean_union("merged", "cube", "boss").unwrap();
    m.assert_has_solid("merged").unwrap();
}

// ── Tests ───────────────────────────────────────────────────────────────
// All cascade_metrics tests run in a single test to avoid global counter races.

#[test]
fn cascade_metrics_all() {
    // ── CM4: Reset zeroes all counters ──────────────────────────────────
    reset_cascade_stats();
    let s0 = cascade_stats();
    assert_eq!(s0.total, 0, "CM4: Reset should zero total");
    assert_eq!(
        s0.direct_success, 0,
        "CM4: Reset should zero direct_success"
    );
    assert_eq!(
        s0.perturbation_success, 0,
        "CM4: Reset should zero perturbation_success"
    );
    assert_eq!(
        s0.euler_fallback, 0,
        "CM4: Reset should zero euler_fallback"
    );
    assert_eq!(s0.exhausted, 0, "CM4: Reset should zero exhausted");

    // ── CM1: Direct success rate ───────────────────────────────────────
    // Run several simple boolean unions. The cascade should fire for each,
    // and most should succeed directly (no perturbation).
    reset_cascade_stats();
    let n_ops = 3;
    for _ in 0..n_ops {
        run_simple_union();
    }

    let stats = cascade_stats();
    assert!(
        stats.total >= n_ops,
        "CM1: total ({}) should be >= number of boolean_union calls ({})",
        stats.total,
        n_ops,
    );
    assert_eq!(
        stats.exhausted, 0,
        "CM1: No simple booleans should exhaust the cascade"
    );

    // ── CM3: Counter consistency invariant ──────────────────────────────
    let sum =
        stats.direct_success + stats.perturbation_success + stats.euler_fallback + stats.exhausted;
    assert_eq!(
        sum,
        stats.total,
        "CM3: Invariant d({}) + p({}) + e({}) + x({}) = {} != total({})",
        stats.direct_success,
        stats.perturbation_success,
        stats.euler_fallback,
        stats.exhausted,
        sum,
        stats.total,
    );

    // ── CM2: Coplanar boss should not exhaust ──────────────────────────
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();

        // Boss exactly on top face (coplanar z=10 interface)
        m.rect_sketch("boss_sk", [0., 0., 10.], [0., 0., 1.], 2., 2., 8., 8.)
            .unwrap();
        m.extrude("boss", "boss_sk", 5.0).unwrap();
        m.boolean_union("result", "cube", "boss").unwrap();
        m.assert_has_solid("result").unwrap();
    }

    let stats2 = cascade_stats();
    assert!(stats2.total >= 1, "CM2: At least 1 cascade invocation");
    assert_eq!(
        stats2.exhausted, 0,
        "CM2: Coplanar boss should not exhaust cascade"
    );
    // Consistency check on CM2 as well
    let sum2 = stats2.direct_success
        + stats2.perturbation_success
        + stats2.euler_fallback
        + stats2.exhausted;
    assert_eq!(
        sum2, stats2.total,
        "CM2: Consistency: d+p+e+x={} != total={}",
        sum2, stats2.total,
    );

    // ── CM4 continued: Reset actually works after real operations ───────
    reset_cascade_stats();
    let s_after = cascade_stats();
    assert_eq!(s_after.total, 0, "CM4: Reset after ops should zero total");

    // One more operation should bump total
    run_simple_union();
    let s_final = cascade_stats();
    assert!(
        s_final.total >= 1,
        "CM4: After reset + 1 op, total ({}) should be >= 1",
        s_final.total,
    );
}

// ── Strategy data collection ────────────────────────────────────────────

fn print_stats(label: &str, stats: &CascadeStats) {
    eprintln!("=== {} ===", label);
    eprintln!(
        "  total={} direct={} perturb={} euler_fb={} exhausted={}",
        stats.total,
        stats.direct_success,
        stats.perturbation_success,
        stats.euler_fallback,
        stats.exhausted,
    );
    for s in &stats.strategies {
        if s.attempts > 0 || s.successes > 0 {
            eprintln!(
                "  {:20} attempts={:3} successes={:3}",
                s.strategy, s.attempts, s.successes,
            );
        }
    }
}

/// CM5: Collect per-strategy usage data across representative boolean scenarios.
/// This test exercises various geometry patterns and reports which strategies
/// actually produce successful results. Run with --nocapture to see output.
#[test]
fn cascade_strategy_collection() {
    // ── Scenario 1: Simple non-coplanar union ───────────────────────────
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "sk", 10.0).unwrap();
        m.rect_sketch("sk2", [0., 0., 10.], [0., 0., 1.], 3., 3., 7., 7.)
            .unwrap();
        m.extrude("boss", "sk2", 5.0).unwrap();
        m.boolean_union("result", "cube", "boss").unwrap();
    }
    let s1 = cascade_stats();
    print_stats("S1: non-coplanar union", &s1);
    assert!(
        s1.direct_success >= 1,
        "S1: simple union should succeed directly"
    );

    // ── Scenario 2: Coplanar union (boss base exactly on cube top) ──────
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "sk", 10.0).unwrap();
        m.rect_sketch("sk2", [0., 0., 10.], [0., 0., 1.], 2., 2., 8., 8.)
            .unwrap();
        m.extrude("boss", "sk2", 5.0).unwrap();
        m.boolean_union("result", "cube", "boss").unwrap();
    }
    let s2 = cascade_stats();
    print_stats("S2: coplanar union", &s2);

    // ── Scenario 3: Subtract (pocket cut) ───────────────────────────────
    // Sketch placed ABOVE cube, normal pointing DOWN, so tool extends into cube.
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "sk", 10.0).unwrap();
        m.rect_sketch("sk2", [0., 0., 11.], [0., 0., -1.], 2., 2., 8., 8.)
            .unwrap();
        m.extrude_cut("result", "sk2", 6.0).unwrap();
    }
    let s3 = cascade_stats();
    print_stats("S3: pocket cut", &s3);

    // ── Scenario 4: Edge-coincident cut ─────────────────────────────────
    // Tool shares edges with cube at the corner — sketch placed above, normal down.
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "sk", 10.0).unwrap();
        m.rect_sketch("sk2", [0., 0., 11.], [0., 0., -1.], 0., 0., 5., 5.)
            .unwrap();
        m.extrude_cut("result", "sk2", 6.0).unwrap();
    }
    let s4 = cascade_stats();
    print_stats("S4: edge-coincident cut", &s4);

    // ── Scenario 5: Chained booleans (2 sequential unions) ──────────────
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "sk", 10.0).unwrap();
        m.rect_sketch("b1sk", [0., 0., 10.], [0., 0., 1.], 1., 1., 4., 4.)
            .unwrap();
        m.extrude("b1", "b1sk", 3.0).unwrap();
        m.boolean_union("m1", "cube", "b1").unwrap();
        m.rect_sketch("b2sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 9., 9.)
            .unwrap();
        m.extrude("b2", "b2sk", 3.0).unwrap();
        m.boolean_union("m2", "m1", "b2").unwrap();
    }
    let s5 = cascade_stats();
    print_stats("S5: chained unions", &s5);

    // ── Scenario 6: Opposite-direction extrude (through-cut) ────────────
    reset_cascade_stats();
    {
        let mut m = ModelBuilder::truck();
        m.rect_sketch("sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "sk", 10.0).unwrap();
        // Cut from above, going through
        m.rect_sketch("sk2", [0., 0., 15.], [0., 0., -1.], 3., 3., 7., 7.)
            .unwrap();
        m.extrude_cut("result", "sk2", 20.0).unwrap();
    }
    let s6 = cascade_stats();
    print_stats("S6: through-cut", &s6);

    // ── Summary ─────────────────────────────────────────────────────────
    eprintln!("\n=== SUMMARY ===");
    for (label, stats) in [
        ("S1-noncoplanar", &s1),
        ("S2-coplanar", &s2),
        ("S3-pocket", &s3),
        ("S4-edge-coin", &s4),
        ("S5-chained", &s5),
        ("S6-through", &s6),
    ] {
        let winning = if stats.direct_success > 0 {
            "direct"
        } else if stats.perturbation_success > 0 {
            let winner = stats
                .strategies
                .iter()
                .filter(|s| s.successes > 0 && s.strategy != "direct")
                .map(|s| s.strategy)
                .next()
                .unwrap_or("unknown");
            winner
        } else if stats.euler_fallback > 0 {
            "euler-fallback"
        } else {
            "EXHAUSTED"
        };
        eprintln!("  {:<16} winning={}", label, winning);
    }
}
