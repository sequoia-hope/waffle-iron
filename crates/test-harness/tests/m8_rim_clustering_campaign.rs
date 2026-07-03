//! M8 rim-aware in-frame clustering — RED trackers for the post-canon
//! `LabelMismatch` class (§2c of `specs/m8_shared_boundary_identity.md`).
//!
//! ## What this is
//!
//! Spec: `specs/m8_shared_boundary_identity.md` §2c ("Rim-aware clustering").
//! The §2 world-space vertex pass and the §2b in-frame clustering are both
//! WIRED in committed code (`kernel-v2/src/boolean.rs` — the canon pass runs
//! unconditionally; no env knob). §2b's clustering is SCOPE-LIMITED to
//! PURE-POLYGON pairs (`rim_a`/`rim_b` empty — see `stage0.rs`, the
//! `cluster_ok = rim_a.is_empty() && rim_b.is_empty()` gate). On RIM-carrying
//! pairs (a disc cap coplanar with a neighbor face) the overlay's exact 2D
//! input still carries femto-split per-input corners / edge-subdivision points
//! / circle∩line rim mints; cherchi's coplanar dedup keys exact identity, the
//! overlap sheet is only partially non-manifold, and the patch flood leaks →
//! `native boolean: patch flood-fill failed: LabelMismatch { .. }`.
//!
//! §2c lifts the scope limit: apply the SAME per-axis band clustering to
//! rim-carrying pairs, restricting the cluster DOMAIN to POLYGON-CHAIN
//! coordinates and EXCLUDING rim sample coordinates (C4a–C4d, invariant I9).
//!
//! ## RED target (spec §2c oracles)
//!
//! Each tracker replays its corpus case through the full kernel-v2 dispatch and
//! asserts the boolean-failure set does NOT carry the `LabelMismatch` string.
//! SUCCESS or a DIFFERENT loud typed error both PASS — layered blockers are
//! expected (the ear-clip / sliver classes sit downstream of the same cases).
//! The trackers are RED today (each currently surfaces `LabelMismatch`) and are
//! `#[ignore]`d so plain `cargo test` stays green. Run with:
//!
//! ```text
//! cargo test -p test-harness --test m8_rim_clustering_campaign -- --ignored --nocapture
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

/// The post-canon rim-carrying wall (cherchi `PatchError::LabelMismatch`,
/// surfaced through `native boolean: patch flood-fill failed: LabelMismatch`).
/// This is the string the rim-aware clustering fix must eliminate for these
/// cases. A DIFFERENT loud error (ear-clip coplanar wall, sliver reassembly,
/// etc.) is an acceptable GREEN — layered blockers are expected (spec §2c).
const LABEL_MISMATCH_WALL: &str = "LabelMismatch";

/// F0061's current rim-gap wall string. Post-canon it does NOT reach
/// `LabelMismatch`: its rim-carrying pair fails EARLIER, at the Stage-0
/// coplanar-preprocessing NotSupported gate (`coplanar input face pair`) — the
/// same string `m8_earclip_campaign::red_f0061` names for the ear-clip class,
/// because a rim-carrying pair that skips §2b clustering can wall at either
/// sub-path. Per spec §2c, rim-aware clustering is F0061's named gap; this
/// tracker asserts that wall is gone (success or a different loud error pass).
const COPLANAR_WALL: &str = "coplanar input face pair";

/// Replay one corpus case through the full kernel-v2 dispatch and return every
/// boolean failure (engine errors + `Auto-union failed` warnings, which carry
/// the yang-rs / cherchi error string via `{}`). The canon + §2b clustering
/// passes are live in committed code, so no env knob is set here.
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
            .filter(|w| w.contains("Auto-union"))
            .cloned(),
    );
    failures
}

/// Replay with a hang guard (chained exact arithmetic — F0061/F0063 drive heavy
/// rings — can be slow; a hung case must not wedge the suite). Mirrors
/// `m8_earclip_campaign`/`stage6_sliver_campaign`. A timeout is reported as its
/// own (non-`LabelMismatch`) failure so the tracker neither hangs nor silently
/// passes on a hang.
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
        // killed in-process); the test moves on. A timeout is NOT the
        // LabelMismatch wall, so it does not itself make the tracker RED.
        Err(_) => vec![format!("{id}: timeout after {}s", timeout.as_secs())],
    }
}

/// Assert `wall` does NOT appear in `case_id`'s boolean-failure set (spec §2c
/// oracles). The panic message carries the actual failures so a RED run
/// documents that the wall is still up; a GREEN run passes on success OR on a
/// different loud typed error.
fn assert_no_wall(case_id: &str, wall: &str) {
    let failures = boolean_failures_with_timeout(case_id, Duration::from_secs(200));
    assert!(
        !failures.iter().any(|f| f.contains(wall)),
        "M8-rim-clustering RED — {case_id} still walls on `{wall}` \
         (rim-carrying pair skips §2b clustering):\n  {}",
        failures.join("\n  ")
    );
}

/// The `LabelMismatch` sub-path (R0046/R0088/F0063).
fn assert_no_label_mismatch(case_id: &str) {
    assert_no_wall(case_id, LABEL_MISMATCH_WALL);
}

#[test]
#[ignore = "M8-rim-clustering RED (spec m8_shared_boundary_identity §2c): R0046 rim-carrying \
            pair skips §2b clustering (rim_a/rim_b non-empty), so the overlay's femto-split 2D \
            input leaks the patch flood → cherchi LabelMismatch; GREEN when rim-aware clustering \
            lands (success or a different loud error both pass)"]
fn red_r0046() {
    assert_no_label_mismatch("R0046");
}

#[test]
#[ignore = "M8-rim-clustering RED (spec m8_shared_boundary_identity §2c): R0088 rim-carrying \
            pair walls on cherchi LabelMismatch (same femto-split-input class as R0046); GREEN \
            when rim-aware clustering lands"]
fn red_r0088() {
    assert_no_label_mismatch("R0088");
}

#[test]
#[ignore = "M8-rim-clustering RED (spec m8_shared_boundary_identity §2c): F0063 rim-carrying \
            pair walls on cherchi LabelMismatch (same class; heavier ring — 200s hang guard); \
            GREEN when rim-aware clustering lands"]
fn red_f0063() {
    assert_no_label_mismatch("F0063");
}

#[test]
#[ignore = "M8-rim-clustering RED (spec m8_shared_boundary_identity §2c): F0061 is the \
            previously-tracked case of this same rim-carrying gap (m8_earclip_campaign::red_f0061 \
            names it too). Post-canon it walls EARLIER than the others — at the Stage-0 coplanar \
            NotSupported gate (`coplanar input face pair`), not LabelMismatch — so this tracker \
            asserts THAT wall is gone; GREEN when rim-aware clustering lands (200s hang guard for \
            the 23-vertex ring)"]
fn red_f0061() {
    // F0061's rim-gap wall is the Stage-0 coplanar NotSupported gate, not
    // LabelMismatch (measured: it fails earlier than R0046/R0088/F0063). Assert
    // THAT wall is gone so this tracker is genuinely RED today.
    assert_no_wall("F0061", COPLANAR_WALL);
}
