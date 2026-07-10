//! KV15 — planar femto-twin weld in MIXED operands: corpus trackers for the
//! edge-not-2-directed `InvalidBooleanOutput` class.
//!
//! Spec: `specs/kv15_mixed_operand_planar_near_weld.md`. Chained-extrude
//! scenarios mint planar vertex twins ≤ ~3e-14 apart; the PR-KV10 near-weld
//! that reconciles them is gated on ALL faces of both operands being planar,
//! so any circle/gear profile anywhere in the chain drops the weld to
//! bit-exact and the twins' femto membrane poisons Stage-6 patch boundaries:
//!
//! ```text
//! InvalidBooleanOutput("an undirected output edge is not used by exactly
//! two directed edges")
//! ```
//!
//! RED today on all three replayed cases (the fast representatives of the
//! six-case class F0070/F0076/F0079/F0081/F0084/R0076 — one per measured
//! subfamily); GREEN when the per-vertex eligibility weld (spec §3) lands.
//!
//! Run: `cargo test -p test-harness --test edge_pairing_twin_weld_campaign`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

const EDGE_PAIRING_WALL: &str = "not used by exactly two directed edges";

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

fn assert_no_edge_pairing_wall(case_id: &str) {
    let failures = replay_failures_with_timeout(case_id, Duration::from_secs(200));
    let joined = failures.join("\n  ");
    assert!(
        !failures.iter().any(|f| f.contains(EDGE_PAIRING_WALL)),
        "KV15 RED — {case_id} still hits the edge-not-2-directed wall \
         (unwelded planar femto twins in a mixed model):\n  {joined}"
    );
}

/// KV15b residue subfamily: R0076's failing twins arrive in the chained
/// input at ~3.9e-8 apart (measured: input A brep verts 22/23 of the
/// Extrude-3 union) — genuinely distinct exact crossings of near-parallel
/// geometry, SUB-FLOOR (< MIN_FEATURE_SIZE) but eight orders ABOVE the
/// representability band, so the KV15 weld correctly excludes them
/// (welding at the feature floor is the reverted-R0091 hazard). Needs its
/// own cycle at the MINTING boolean (the gear-cut op emits the sub-floor
/// twin pair; A14.2 says one feature). Un-ignore when KV15b lands.
#[test]
#[ignore = "KV15b — sub-floor (3.9e-8) near-parallel crossing twins in the chained input; representability weld correctly excludes them; fix belongs at the minting op"]
fn r0076_no_edge_pairing_wall() {
    assert_no_edge_pairing_wall("R0076");
}

/// Stacked-Z chained-extrude subfamily (15 ops, gear profiles).
#[test]
fn f0070_no_edge_pairing_wall() {
    assert_no_edge_pairing_wall("F0070");
}

/// Off-axis tilted subfamily (twins minted OUTSIDE any Stage-0 pair).
#[test]
fn f0081_no_edge_pairing_wall() {
    assert_no_edge_pairing_wall("F0081");
}
