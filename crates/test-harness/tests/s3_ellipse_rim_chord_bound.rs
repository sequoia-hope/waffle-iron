//! Stage-3 ellipse-rim chord bound: corpus trackers for the
//! `AmbiguousCurve { candidates: 0, matched: 0 }` producer-fault class.
//!
//! Spec: `specs/yang_s3_ellipse_rim_chord_bound.md`. A re-entering body
//! whose single cylinder face is bounded by ellipse rims only (oblique
//! plane∩cylinder sections from a prior boolean — KV14 ellipse-arc
//! vocabulary) has NO `Curve::Circle` edge, so Stage-3's
//! `chord_tol_for_curved_owner` finds no Circle-rim AABB and raises the
//! producer fault:
//!
//! ```text
//! AmbiguousCurve { candidates: 0, matched: 0 }
//! ```
//!
//! RED today on all three replayed cases (measured 2026-07-10, probe
//! `[s3-ambig-probe] PRODUCER FAULT … cylinder-owning input A has NO
//! Circle rim`); GREEN when the ellipse-chain fallback bound (spec §3)
//! lands. The cases have KNOWN downstream walls in later ops
//! (edge-not-2-directed sites, CDT re-entry), so the assertion is
//! narrowly "this fault signature is gone", not "the case is green".
//!
//! Run: `cargo test -p test-harness --test s3_ellipse_rim_chord_bound`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

const PRODUCER_FAULT: &str = "AmbiguousCurve { candidates: 0, matched: 0 }";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn replay_failures(case_id: &str) -> Vec<String> {
    let dir = assay_dir();
    let waffle_json = match fs::read_to_string(dir.join(format!("{case_id}.waffle"))) {
        Ok(s) => s,
        Err(e) => return vec![format!("cannot read {case_id}.waffle: {e}")],
    };
    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        return vec![format!("LoadProject failed: {e}")];
    }
    let mut failures: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect();
    failures.extend(
        builder
            .engine_warnings()
            .iter()
            .filter(|w| w.contains("Auto-union"))
            .cloned(),
    );
    failures
}

fn replay_failures_with_timeout(case_id: &str, timeout: Duration) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(replay_failures(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => vec![format!("{id}: timeout after {}s", timeout.as_secs())],
    }
}

fn assert_no_producer_fault(case_id: &str) {
    let failures = replay_failures_with_timeout(case_id, Duration::from_secs(200));
    let joined = failures.join("\n  ");
    assert!(
        !failures.iter().any(|f| f.contains(PRODUCER_FAULT)),
        "S3 ellipse-rim RED — {case_id} still hits the candidates-0 producer fault \
         (ellipse-rim cylinder owner has no Stage-3 chord bound):\n  {joined}"
    );
}

#[test]
fn f0083_no_candidates_zero_fault() {
    assert_no_producer_fault("F0083");
}

#[test]
fn f0082_no_candidates_zero_fault() {
    assert_no_producer_fault("F0082");
}

#[test]
fn f0085_no_candidates_zero_fault() {
    assert_no_producer_fault("F0085");
}
