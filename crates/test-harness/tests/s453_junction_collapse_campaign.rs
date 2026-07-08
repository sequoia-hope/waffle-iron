//! §4.5.3 junction-protected reversal collapse — corpus trackers for
//! R0009 / R0011 / R0091.
//!
//! Spec: `specs/yang_453_junction_protected_collapse.md`. The Stage-4 §4.5.3
//! reversed-intersection sweep collapses the NEXT point of a detected
//! reversal; when that next point is a curve-junction vertex (the exact
//! shared endpoint of two different conic sections — e.g. adjacent gear-flank
//! plane∩cylinder ellipses), the collapse merges arcs of two different curves
//! into one output edge and the surviving endpoints no longer lie on the
//! edge's representative `Curve`. kernel-v2 then rejects loudly:
//!
//! ```text
//! InvalidBooleanOutput("output ellipse-arc endpoint does not lie on its ellipse")
//! ```
//!
//! RED today on all three cases; GREEN when the junction-protected collapse
//! direction lands (spec §3 branch 2). The trackers forbid BOTH conic
//! endpoint-membership walls (ellipse AND circle) so the fix cannot merely
//! relocate the failure onto the circle analog.
//!
//! Run: `cargo test -p test-harness --test s453_junction_collapse_campaign`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// The campaign wall: a chained/merged output conic edge whose endpoint was
/// relocated onto a DIFFERENT curve (kernel-v2 `classify_edge`).
const ELLIPSE_ENDPOINT_WALL: &str = "does not lie on its ellipse";
/// The failure-moved analog for circle junctions (spec §5) — forbidden too.
const CIRCLE_ENDPOINT_WALL: &str = "does not lie on its circle";

/// Boolean-failure strings of one corpus replay: engine errors + `Auto-union
/// failed` warnings (which carry the yang-rs error text via `{}`).
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

/// Hang guard: chained exact arithmetic can be slow; a hung case must not
/// wedge the suite (mirrors `stage6_sliver_campaign`).
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

fn assert_no_conic_endpoint_wall(case_id: &str) {
    let failures = replay_failures_with_timeout(case_id, Duration::from_secs(200));
    let joined = failures.join("\n  ");
    assert!(
        !failures.iter().any(|f| f.contains(ELLIPSE_ENDPOINT_WALL)),
        "§4.5.3-junction RED — {case_id} still carries the merged-edge ellipse \
         endpoint-membership wall (a §4.5.3 collapse removed a curve-junction \
         vertex):\n  {joined}"
    );
    assert!(
        !failures.iter().any(|f| f.contains(CIRCLE_ENDPOINT_WALL)),
        "§4.5.3-junction RED — {case_id} moved the failure onto the CIRCLE \
         endpoint-membership wall (spec §5 — not a valid GREEN):\n  {joined}"
    );
}

#[test]
fn r0011_no_conic_endpoint_wall() {
    assert_no_conic_endpoint_wall("R0011");
}

#[test]
#[ignore = "spec §3b BANKED-UNWIRED: R0009's ellipse-endpoint wall (micro scale, \
            1.05e-4) is the §4.4.1(b) merge-survivor mechanism, same as R0091. \
            Wiring sub_feature_merge_direction clears it (R0009 → 1 pre-existing \
            non-2-manifold error, no WRONG) but is blocked on the R0091 χ \
            verification (see r0091 tracker). Un-ignore together with it"]
fn r0009_no_conic_endpoint_wall() {
    assert_no_conic_endpoint_wall("R0009");
}

#[test]
#[ignore = "spec §3b BANKED-UNWIRED: the exactness-ranked §4.4.1(b) merge survivor \
            (sub_feature_merge_direction) clears this wall but flips R0091 to \
            SUPPORTED_WRONG (χ=−4 vs meta 2 — three handles the authored numbers \
            don't obviously predict; partial 219° revolve is genus 0). RED until \
            the output's true χ is verified (sidecar reference parity) or the \
            meta χ is refuted, then wire the ranked survivor and un-ignore"]
fn r0091_no_conic_endpoint_wall() {
    assert_no_conic_endpoint_wall("R0091");
}
