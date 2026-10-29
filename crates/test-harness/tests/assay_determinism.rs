//! Determinism tests — the same boolean scenario, rebuilt in a fresh
//! adapter three times, must produce a BIT-IDENTICAL result B-Rep
//! (remediation plan O7).
//!
//! The check was (V, E, F) counts until 2026-10-29; it now compares the
//! canonical geometry hash (`test_harness::assay::geometry_hash`): every
//! vertex position, edge polyline, face signature and the exact volume, as
//! `f64` bits, sorted so allocation order and `KernelId`s do not enter.
//!
//! The proptest stream is SEEDED (`RngSeed::Fixed`), so every run — and
//! both `cherchi-rs/parallel` configurations — draws the same ten
//! scenarios. With `WAFFLE_GEOMETRY_HASH_OUT` set, each scenario's hash is
//! appended to that file; `scripts/determinism_parallel_parity.sh` diffs
//! the files of a serial and a parallel build (the in-process check cannot
//! toggle a cargo feature).

use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use test_harness::assay::determinism::*;
use test_harness::assay::geometry_hash::{fnv1a_words, record_hash_for_parity};
use test_harness::assay::strategies::strats::*;
use test_harness::assay::strategies::{
    BoolOp, BooleanScenario, RectProfile, SketchProfile, SolidBodySpec,
};

/// A stable label for a scenario: the hash of its `Debug` text (which
/// prints every `f64` round-trip exact).
fn scenario_label(s: &BooleanScenario) -> String {
    let words: Vec<u64> = format!("{s:?}").bytes().map(u64::from).collect();
    format!("assay_determinism/{:016x}", fnv1a_words(&words))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10,
        rng_seed: RngSeed::Fixed(0x0007_D373_4815),
        ..ProptestConfig::default()
    })]

    #[test]
    fn boolean_deterministic(scenario in boolean_scenario_any()) {
        let result = check_determinism(&scenario, 3);
        // Identical failures are deterministic too; anything else — a hash
        // that moves, counts that move, a run that fails where another
        // succeeded, two different error texts — is not.
        prop_assert!(
            result.is_deterministic,
            "Non-deterministic: {} (scenario {:?})",
            result.detail,
            scenario
        );
        // A scenario every run of which failed (identically) is recorded by
        // its error text, so the cross-configuration diff compares that too.
        let hash = match result.run_hashes.first() {
            Some(h) => format!("{h:016x}"),
            None => {
                eprintln!("[determinism] {}: {}", scenario_label(&scenario), result.detail);
                let words: Vec<u64> = result.detail.bytes().map(u64::from).collect();
                format!("error-{:016x}", fnv1a_words(&words))
            }
        };
        record_hash_for_parity(&scenario_label(&scenario), &hash);
    }
}

fn rect_body(origin: [f64; 3], x: f64, y: f64, w: f64, h: f64, depth: f64) -> SolidBodySpec {
    SolidBodySpec {
        profile: SketchProfile::Rect(RectProfile { x, y, w, h }),
        origin,
        normal: [0.0, 0.0, 1.0],
        depth,
    }
}

/// Red-first evidence that the hash can see a difference: two scenarios
/// that differ only in a tool offset of 1e-9 (the counts are identical —
/// the old check could not tell them apart) hash differently, while the
/// same scenario rebuilt in a fresh adapter hashes identically.
#[test]
fn hash_separates_documents_the_counts_cannot() {
    let a = BooleanScenario {
        body_a: rect_body([0.0; 3], 0.0, 0.0, 10.0, 10.0, 10.0),
        body_b: rect_body([0.0, 0.0, 2.0], 3.0, 3.0, 4.0, 4.0, 12.0),
        op: BoolOp::Subtract,
    };
    let mut b = a.clone();
    b.body_b = rect_body([0.0, 0.0, 2.0], 3.0 + 1e-9, 3.0, 4.0, 4.0, 12.0);

    let (topo_a, hash_a) = run_scenario(&a).expect("scenario a builds");
    let (topo_a2, hash_a2) = run_scenario(&a).expect("scenario a rebuilds");
    let (topo_b, hash_b) = run_scenario(&b).expect("scenario b builds");

    assert_eq!(topo_a, topo_a2);
    assert_eq!(
        hash_a, hash_a2,
        "the same document in a fresh adapter must hash identically"
    );
    assert_eq!(
        topo_a, topo_b,
        "precondition: the two results have the same (V, E, F) counts"
    );
    assert_ne!(
        hash_a, hash_b,
        "a 1e-9 tool offset must move the geometry hash (counts {topo_a:?} both)"
    );
}
