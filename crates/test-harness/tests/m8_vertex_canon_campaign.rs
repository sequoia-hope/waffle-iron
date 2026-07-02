//! M8-VERTEX-CANON chained-output vertex canonicalization — RED→GREEN trackers.
//!
//! ## What this is
//!
//! Spec: `specs/m8_shared_boundary_identity.md`. A chained boolean output
//! re-enters the next boolean with vertices carrying independent ~1e-16 rounding
//! that is inconsistent with its canonicalized face planes (femto-crooked loops).
//! The Stage-0 exact overlay faithfully arranges that crookedness into femto
//! slivers / needle cells / femto-twin ring vertices, which wall the coplanar
//! boolean — surfaced through the kernel-v2 adapter as the `coplanar input face
//! pair` message (`kernel-v2/src/adapter.rs:344`). A vertex-hygiene pass in
//! `to_yang_brep` (after `canonicalize_sibling_planes`) re-derives each vertex
//! from its incident canonical planes, at the root.
//!
//! Probed corpus blockers (2026-07-02): R0076, R0081 (RoundingCollapse today),
//! R0070 (LabelMismatch + coplanar wall). All three surface the coplanar wall
//! string today (verified).
//!
//! ## RED target
//!
//! Each tracker replays a corpus case and asserts the failure set does NOT carry
//! the `coplanar input face pair` wall. SUCCESS or a DIFFERENT loud typed error
//! both pass (spec §5 E2E — layered blockers are expected). RED today and
//! `#[ignore]`d so plain `cargo test` stays green. Run with:
//!
//! ```text
//! cargo test -p test-harness --test m8_vertex_canon_campaign -- --ignored --nocapture
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

/// The coplanar wall as it surfaces through the kernel-v2 adapter
/// (`adapter.rs:344`) — the string the vertex-canon fix must eliminate for
/// these cases.
const COPLANAR_WALL: &str = "coplanar input face pair";

/// Replay one corpus case through the full kernel-v2 dispatch and return every
/// boolean failure (engine errors + `Auto-union failed` warnings). The RED
/// target is that the coplanar wall is gone, not that the whole case is
/// oracle-correct (downstream blockers are expected and pass this tracker).
fn boolean_failures(case_id: &str) -> Vec<String> {
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
            .filter(|w| w.contains("Auto-union failed"))
            .cloned(),
    );
    failures
}

/// Replay with a hang guard (chained exact arithmetic can be slow; a hung case
/// must not wedge the suite). Mirrors `assay_kv2::replay_case_with_timeout`. A
/// timeout is reported as its own (non-coplanar) failure.
fn boolean_failures_with_timeout(case_id: &str, timeout: Duration) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(boolean_failures(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => vec![format!("{id}: timeout after {}s", timeout.as_secs())],
    }
}

/// Assert the `coplanar input face pair` wall does NOT appear for `case_id`.
fn assert_no_coplanar_wall(case_id: &str) {
    let failures = boolean_failures_with_timeout(case_id, Duration::from_secs(200));
    assert!(
        !failures.iter().any(|f| f.contains(COPLANAR_WALL)),
        "M8-vertex-canon RED — {case_id} still walls on the coplanar gate:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
#[ignore = "M8-vertex-canon RED (spec m8_shared_boundary_identity): R0076 walls on the coplanar \
            gate — its chained-output operand quad is femto-crooked (RoundingCollapse slivers); \
            GREEN when chained-output vertex canonicalization lands"]
fn red_r0076() {
    assert_no_coplanar_wall("R0076");
}

#[test]
#[ignore = "M8-vertex-canon RED (spec m8_shared_boundary_identity): R0081 walls on the coplanar \
            gate (femto sliver RoundingCollapse); GREEN when chained-output vertex canonicalization \
            lands"]
fn red_r0081() {
    assert_no_coplanar_wall("R0081");
}

#[test]
#[ignore = "M8-vertex-canon RED (spec m8_shared_boundary_identity): R0070 walls on the coplanar \
            gate (femto-twin LabelMismatch neighbor); GREEN when chained-output vertex \
            canonicalization lands"]
fn red_r0070() {
    assert_no_coplanar_wall("R0070");
}
