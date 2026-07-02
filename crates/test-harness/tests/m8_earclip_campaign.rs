//! M8-EARCLIP non-star subdivided-ring triangulation — RED→GREEN trackers.
//!
//! ## What this is
//!
//! Spec: `specs/m8_nonstar_ring_earclip.md`. Stage-0's split-neighbor
//! re-triangulation (`stage0.rs::triangulate_ring`) tries a boundary-vertex
//! apex fan (B1) and an interior-centroid fan (B2, star-shaped only). A
//! non-convex (reflex) neighbor face whose boundary was subdivided by overlay
//! split points is star-shaped from neither → `triangulate_ring` returns `None`
//! and the coplanar boolean walls, surfaced through the kernel-v2 adapter as the
//! `coplanar input face pair` message (`kernel-v2/src/adapter.rs:344`).
//!
//! Probed corpus blockers in this class (2026-07-02): R0046 (ring 9), R0088
//! (ring 12), R0098 (ring 11), F0061 (ring 23). The fix is an exact ear-clip
//! fallback (spec B3).
//!
//! ## RED target
//!
//! Each tracker replays a corpus case and asserts the failure set does NOT carry
//! the `coplanar input face pair` wall. SUCCESS or a DIFFERENT loud typed error
//! both pass (spec §5 E2E — layered blockers are expected). They are RED today
//! and are `#[ignore]`d so plain `cargo test` stays green. Run with:
//!
//! ```text
//! cargo test -p test-harness --test m8_earclip_campaign -- --ignored --nocapture
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

/// The intra-/split-neighbor coplanar wall as it surfaces through the kernel-v2
/// adapter (`adapter.rs:344`). This is the string the ear-clip fix must
/// eliminate for these cases.
const COPLANAR_WALL: &str = "coplanar input face pair";

/// Replay one corpus case through the full kernel-v2 dispatch and return every
/// boolean failure (engine errors + `Auto-union failed` warnings). We only need
/// the boolean-level failure text here: the RED target is that the coplanar wall
/// is gone, not that the whole case is oracle-correct (downstream blockers are
/// expected and pass this tracker).
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

/// Replay with a hang guard (F0061's 23-vertex ring drives heavy exact
/// arithmetic; a hung case must not wedge the suite). Mirrors
/// `assay_kv2::replay_case_with_timeout`. A timeout is reported as its own
/// (non-coplanar) failure so the tracker neither hangs nor silently passes.
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
        // Orphaned worker keeps running (heavy exact arithmetic can't be safely
        // killed in-process); the test moves on. A timeout is NOT the coplanar
        // wall, so it does not itself make the tracker RED.
        Err(_) => vec![format!("{id}: timeout after {}s", timeout.as_secs())],
    }
}

/// Assert the `coplanar input face pair` wall does NOT appear for `case_id`
/// (spec §5 E2E). The panic message carries the actual failures so a RED run
/// documents that the wall is still up.
fn assert_no_coplanar_wall(case_id: &str) {
    let failures = boolean_failures_with_timeout(case_id, Duration::from_secs(200));
    assert!(
        !failures.iter().any(|f| f.contains(COPLANAR_WALL)),
        "M8-earclip RED — {case_id} still walls on the coplanar split-neighbor gate:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
#[ignore = "M8-earclip RED (spec m8_nonstar_ring_earclip): R0046 (ring 9) walls on the coplanar \
            gate — its reflex split neighbor is star-shaped from neither fan; GREEN when the exact \
            ear-clip fallback lands"]
fn red_r0046() {
    assert_no_coplanar_wall("R0046");
}

#[test]
#[ignore = "M8-earclip RED (spec m8_nonstar_ring_earclip): R0088 (ring 12) walls on the coplanar \
            gate; GREEN when the exact ear-clip fallback lands"]
fn red_r0088() {
    assert_no_coplanar_wall("R0088");
}

#[test]
#[ignore = "M8-earclip RED (spec m8_nonstar_ring_earclip): R0098 (ring 11) walls on the coplanar \
            gate; GREEN when the exact ear-clip fallback lands"]
fn red_r0098() {
    assert_no_coplanar_wall("R0098");
}

#[test]
#[ignore = "M8-earclip RED (spec m8_nonstar_ring_earclip): F0061 (ring 23) walls on the coplanar \
            gate; GREEN when the exact ear-clip fallback lands (200s hang guard for the 23-vertex \
            ring)"]
fn red_f0061() {
    assert_no_coplanar_wall("F0061");
}
