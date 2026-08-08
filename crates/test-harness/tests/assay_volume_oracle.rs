//! Increment 1 of `specs/assay_independent_volume_oracle.md` — calibration and
//! the corpus-wide sweep of the independent volume oracle.
//!
//! The engine lives in the lib (`test_harness::assay::volume_oracle_doc`) so
//! the categorized assay runner applies the SAME composition check in-line
//! (2026-08-08: volume composition, not body count, is the discriminator for
//! multi-body outputs — see `docs/audits/volume_oracle_flags_anchored.md`).
//!
//! Scope: all-BOSS cases only; cut cases are reported NOT-COVERED, never
//! silently passed (spec §5).
//!
//! Run the sweep:
//! ```text
//! cargo test -p test-harness --test assay_volume_oracle --release \
//!     -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use test_harness::assay::volume_oracle_doc::{evaluate_composition, CompositionVerdict};

const CORPUS: &str = "../../app/tests/cases/assay";

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS)
}

/// Read a case: (waffle document, per-op is_cut flags, scale).
fn read_case(id: &str) -> Option<(serde_json::Value, Vec<bool>, f64)> {
    let d = corpus_dir();
    let waffle: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.waffle"))).ok()?).ok()?;
    let meta: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.join(format!("{id}.meta.json"))).ok()?).ok()?;
    let cuts = meta
        .get("operations")?
        .as_array()?
        .iter()
        .map(|o| o.get("is_cut").and_then(serde_json::Value::as_bool) == Some(true))
        .collect();
    let scale = meta.get("scale").and_then(serde_json::Value::as_f64)?;
    Some((waffle, cuts, scale))
}

fn evaluate(id: &str, grid: usize) -> CompositionVerdict {
    let Some((waffle, cuts, scale)) = read_case(id) else {
        return CompositionVerdict::NotCovered("unreadable case");
    };
    evaluate_composition(id, &waffle, &cuts, scale, grid)
}

/// CALIBRATION (spec §3.4). The 14 all-axis-aligned-rectangle F cases have an
/// EXACT expected volume from the corpus generator's own unit-tested box-CSG
/// sweep. The oracle must reproduce it before any verdict it emits elsewhere is
/// trusted. These cases are deliberately NOT the deliverable — all 14 are
/// two-op box unions, and proving them correct proves nothing about the kernel.
/// Their job is to prove the ORACLE.
#[test]
#[ignore = "slow (builds 3 solids per case); run with --ignored"]
fn calibration_against_exact_box_csg() {
    const CASES: [&str; 14] = [
        "F0001", "F0002", "F0003", "F0004", "F0005", "F0006", "F0007", "F0008", "F0009", "F0010",
        "F0051", "F0053", "F0091", "F0093",
    ];
    let mut failures = Vec::new();
    let mut covered = 0;
    for id in CASES {
        match evaluate(id, 96) {
            CompositionVerdict::Agree { rel, band } => {
                covered += 1;
                println!("{id}: AGREE rel={rel:.3e} band={band:.3e}");
            }
            CompositionVerdict::Flag { rel, band } => {
                covered += 1;
                failures.push(format!("{id}: rel={rel:.3e} > band={band:.3e}"));
            }
            CompositionVerdict::NotCovered(why) => println!("{id}: NOT-COVERED ({why})"),
        }
    }
    println!(
        "calibration: {covered}/14 covered, {} flagged",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "the ORACLE is wrong on its calibration set — no sweep verdict is \
         trustworthy until this is green:\n{}",
        failures.join("\n")
    );
    assert!(covered >= 10, "calibration coverage too thin: {covered}/14");
}

/// INCREMENT 2 — sweep every all-boss SUPPORTED_CORRECT case and report the
/// discrepancy distribution. Read-only: the assay's own verdicts do not change
/// here (the categorized runner applies the same check itself).
#[test]
#[ignore = "full sweep; run with --ignored --nocapture"]
fn sweep_all_boss_correct_cases() {
    let dir = corpus_dir();
    let results: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("results.json")).expect("results.json"))
            .expect("parse results.json");
    let mut status: BTreeMap<String, String> = BTreeMap::new();
    for r in results["results"].as_array().expect("results array") {
        status.insert(
            r["id"].as_str().unwrap_or_default().to_string(),
            r["category"].as_str().unwrap_or_default().to_string(),
        );
    }
    let only: Option<String> = std::env::var("ORACLE_CASE").ok();
    let grid: usize = std::env::var("ORACLE_GRID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);

    let (mut agree, mut flagged, mut skipped) = (0usize, Vec::new(), 0usize);
    for (id, cat) in &status {
        if cat != "SUPPORTED_CORRECT" && cat != "SUPPORTED_WRONG" {
            continue;
        }
        if let Some(want) = &only {
            if id != want {
                continue;
            }
        }
        match evaluate(id, grid) {
            CompositionVerdict::Agree { rel, band } => {
                agree += 1;
                println!("{id}: AGREE rel={rel:.3e} band={band:.3e}");
            }
            CompositionVerdict::Flag { rel, band } => {
                println!("{id}: **FLAG** rel={rel:.3e} band={band:.3e}");
                flagged.push(format!("{id} (rel={rel:.3e}, band={band:.3e})"));
            }
            CompositionVerdict::NotCovered(why) => {
                skipped += 1;
                println!("{id}: not-covered ({why})");
            }
        }
    }
    // Coverage is REPORTED, never implied (spec §5).
    println!(
        "\nORACLE SWEEP: agree={agree} flagged={} not-covered={skipped}",
        flagged.len()
    );
    if !flagged.is_empty() {
        println!("FLAGGED:\n  {}", flagged.join("\n  "));
    }
}
