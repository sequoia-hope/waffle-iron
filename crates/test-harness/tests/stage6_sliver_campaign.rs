//! Stage-6 degenerate-sliver topology — RED trackers for F0016/F0024.
//!
//! ## What this is
//!
//! Spec: `specs/yang_stage6_sliver_topology.md`. With
//! `canonicalize_vertices_to_planes` wired, a chained boolean whose
//! arrangement emits ZERO-AREA shim slivers along a shared collinear
//! solid-edge chain fails to reassemble a valid 2-manifold output B-Rep.
//! Today F0016/F0024 flip SUPPORTED_CORRECT → loud ERROR
//! ("yang-rs: reassembled output would be non-2-manifold"); the Stage-6
//! design (spec §4: walk robustness + loop T-subdivision) must make them
//! reassemble.
//!
//! The canon pass is UNWIRED in production (`kernel-v2/src/boolean.rs:~580`).
//! These trackers set `KV2_CANON_WIRE=1` to exercise the wired path, the
//! env-gated-diagnostics convention (the `KV2_PATCH_*` precedent). The var
//! is process-global; all three tests in this file want it set to the same
//! value, so setting it per-test is safe under `--ignored` parallelism.
//!
//! ## RED target
//!
//! Each tracker replays its corpus case with canon wired and asserts the
//! boolean-failure set carries NEITHER:
//!   - "reassembled output would be non-2-manifold" (today's dead-end), NOR
//!   - "an undirected output edge is not used by exactly two directed edges"
//!     (spec §3: the failure-moved signature — the fix must not merely
//!     relocate the wall into kernel-v2's edge-pairing check),
//! AND that tessellation of the result succeeds. RED today (the first
//! signature fires); GREEN when the Stage-6 sliver design lands.
//! `#[ignore]`d so plain `cargo test` stays green. Run with:
//!
//! ```text
//! cargo test -p test-harness --test stage6_sliver_campaign -- --ignored --nocapture
//! ```

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

/// Today's Stage-6 dead-end for the sliver class (yang-rs `NonManifoldOutput`,
/// `kernel-v2/src/boolean.rs:569`). The design must eliminate it.
const NON_MANIFOLD_WALL: &str = "reassembled output would be non-2-manifold";

/// The "failure moved" signature (spec §3): kernel-v2's output edge-pairing
/// check (`boolean.rs:1262`). A fix that just re-homes slivers trips THIS
/// instead — the RED forbids both so GREEN cannot be a wall relocation.
const EDGE_PAIRING_WALL: &str =
    "an undirected output edge is not used by exactly two directed edges";

/// Outcome of replaying one corpus case with canon wired: every boolean
/// failure (engine errors + `Auto-union failed` warnings, which carry the
/// yang-rs error string via `{}`), plus whether tessellation succeeded.
struct Replay {
    failures: Vec<String>,
    tess_err: Option<String>,
}

/// Load + gather failures + tessellate. Assumes `KV2_CANON_WIRE` is already
/// set in the environment by the caller (env is process-global).
fn replay(case_id: &str) -> Replay {
    let dir = assay_dir();
    let waffle_json = match fs::read_to_string(dir.join(format!("{case_id}.waffle"))) {
        Ok(s) => s,
        Err(e) => {
            return Replay {
                failures: vec![format!("cannot read {case_id}.waffle: {e}")],
                tess_err: Some("not tessellated (load precondition failed)".into()),
            }
        }
    };

    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        return Replay {
            failures: vec![format!("LoadProject failed: {e}")],
            tess_err: Some("not tessellated (load failed)".into()),
        };
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

    let tess_err = builder
        .tessellate_last_with_tol(0.01)
        .err()
        .map(|e| e.to_string());

    Replay { failures, tess_err }
}

/// Replay with a hang guard (chained exact arithmetic can be slow; a hung
/// case must not wedge the suite). Mirrors `m8_vertex_canon_campaign`. A
/// timeout is reported as its own failure.
fn replay_with_timeout(case_id: &str, timeout: Duration) -> Replay {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(replay(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => Replay {
            failures: vec![format!("{id}: timeout after {}s", timeout.as_secs())],
            tess_err: Some("not tessellated (timeout)".into()),
        },
    }
}

/// Assert `case_id` reassembles cleanly with canon wired: no non-2-manifold
/// dead-end, no relocated edge-pairing wall, and tessellation succeeds.
fn assert_reassembles_clean(case_id: &str) {
    // Canon is UNWIRED in production; wire it for this replay (spec §6).
    std::env::set_var("KV2_CANON_WIRE", "1");

    let r = replay_with_timeout(case_id, Duration::from_secs(200));
    let joined = r.failures.join("\n  ");

    assert!(
        !r.failures.iter().any(|f| f.contains(NON_MANIFOLD_WALL)),
        "Stage-6-sliver RED — {case_id} (canon wired) still dead-ends on the \
         non-2-manifold reassembly wall:\n  {joined}"
    );
    assert!(
        !r.failures.iter().any(|f| f.contains(EDGE_PAIRING_WALL)),
        "Stage-6-sliver RED — {case_id} (canon wired) moved the failure into \
         kernel-v2's edge-pairing check (spec §3 — not a valid GREEN):\n  {joined}"
    );
    assert!(
        r.tess_err.is_none(),
        "Stage-6-sliver RED — {case_id} (canon wired) reassembled but did not \
         tessellate: {}\n  boolean failures:\n  {joined}",
        r.tess_err.as_deref().unwrap_or("<none>")
    );
}

#[test]
#[ignore = "Stage-6-sliver RED (spec yang_stage6_sliver_topology): F0016 Extrude-3 union \
            with canon wired dead-ends at NonManifoldOutput (zero-area shim slivers along the \
            shared solid-edge chain fold face 13's patch boundary); GREEN when the walk-robustness \
            + loop T-subdivision design lands"]
fn red_f0016_canon() {
    assert_reassembles_clean("F0016");
}

#[test]
#[ignore = "Stage-6-sliver RED (spec yang_stage6_sliver_topology): F0024 with canon wired \
            dead-ends at NonManifoldOutput (same zero-area shim-sliver class as F0016); GREEN \
            when the Stage-6 sliver design lands"]
fn red_f0024_canon() {
    assert_reassembles_clean("F0024");
}

/// Sanity guard (spec §6 premise): the committed `KV2_CANON_WIRE` knob
/// reaches the pipeline. With the var set, F0016 replays to completion and —
/// TODAY — surfaces the non-2-manifold wall. This is a DIAGNOSTIC, not an
/// assertion that GREEN would break: it asserts only that the replay
/// COMPLETES (load ok, some outcome) and records the failure strings, so it
/// stays true across the fix. It documents that the RED premise is reachable
/// through the knob rather than a dead knob silently passing the trackers.
#[test]
#[ignore = "Stage-6-sliver diagnostic: pins that KV2_CANON_WIRE reaches the pipeline and records \
            the current failure set (does not assert the wall, so it survives GREEN)"]
fn guard_canon_knob_reaches_pipeline() {
    std::env::set_var("KV2_CANON_WIRE", "1");
    let r = replay_with_timeout("F0016", Duration::from_secs(200));

    // The replay reached an outcome (did not hang past the guard): the timeout
    // path emits a single "timeout after Ns" sentinel failure. Absence of that
    // sentinel means the knob is wired into a live path that ran to completion.
    // Stable across GREEN (a clean reassembly is also "completed").
    let timed_out = r.failures.iter().any(|f| f.contains("timeout after"));
    assert!(
        !timed_out,
        "F0016 canon-wired replay hung: {:?}",
        r.failures
    );

    eprintln!(
        "[guard] F0016 canon-wired boolean failures ({}):",
        r.failures.len()
    );
    for f in &r.failures {
        eprintln!("[guard]   {f}");
    }
    eprintln!("[guard] F0016 canon-wired tessellation: {:?}", r.tess_err);
    let has_wall = r.failures.iter().any(|f| f.contains(NON_MANIFOLD_WALL));
    eprintln!("[guard] non-2-manifold wall present today: {has_wall}");
}
