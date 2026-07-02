//! M8-INTRA OPPOSITE-NORMAL coplanar step pairs — RED→GREEN campaign trackers.
//!
//! ## What this is
//!
//! Spec: `specs/m8_intra_opposite_plane_canonicalization.md`. A chained boolean
//! output can carry two coplanar faces with OPPOSITE outward normals (a stepped
//! solid: the top of a lower step and the bottom of an upper overhang lie on ONE
//! geometric plane). Today re-entering such a solid into a further boolean walls
//! with the intra-solid coplanar gate — surfaced through the kernel-v2 adapter as
//! the `coplanar input face pair` message (`kernel-v2/src/adapter.rs:344`) —
//! whenever the other operand's AABB touches either fragment. These are the
//! dominant intra-solid M8 residue (probed 2026-07-02: R0022, R0025, R0031).
//!
//! The producer-side fix is sign-aware plane canonicalization (kernel-v2
//! `canonicalize_sibling_planes`, the opposite-orientation completion of
//! PR-KV10) plus an exactly-negated intra-exclusion in yang-rs
//! `scan_near_coplanar` (spec B2/B6).
//!
//! ## RED target
//!
//! Each tracker replays a corpus case and asserts the failure set does NOT carry
//! the intra `coplanar input face pair` wall. SUCCESS or a DIFFERENT loud typed
//! error both pass (layered blockers are expected and honest — spec §5 E2E).
//! They are RED today (the wall fires) and are `#[ignore]`d so plain
//! `cargo test` stays green. Run the checklist with:
//!
//! ```text
//! cargo test -p test-harness --test m8_intra_opposite_campaign -- --ignored --nocapture
//! ```

use std::fs;
use std::path::PathBuf;

use test_harness::ModelBuilder;

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// The intra-solid coplanar wall as it surfaces through the kernel-v2 adapter
/// (`adapter.rs:344`). This is the string the fix must eliminate for these
/// cases.
const INTRA_WALL: &str = "coplanar input face pair";

/// Replay one corpus case through the full kernel-v2 dispatch and return every
/// boolean failure (engine errors + `Auto-union failed` warnings). We only need
/// the boolean-level failure text here: the RED target is that the intra wall is
/// gone, not that the whole case is oracle-correct (downstream blockers are
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

/// Assert the intra `coplanar input face pair` wall does NOT appear for `case_id`
/// (spec §5 E2E). The panic message carries the actual failures so a RED run
/// documents that the wall is still up.
fn assert_no_intra_wall(case_id: &str) {
    let failures = boolean_failures(case_id);
    assert!(
        !failures.iter().any(|f| f.contains(INTRA_WALL)),
        "M8-intra RED — {case_id} still walls on the intra coplanar gate:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
#[ignore = "M8-intra RED (spec m8_intra_opposite_plane_canonicalization): R0022 walls on the \
            intra-solid `coplanar input face pair` gate; GREEN when sign-aware canonicalization + \
            exactly-negated intra-exclusion land"]
fn red_r0022_intra_opposite_wall() {
    assert_no_intra_wall("R0022");
}

#[test]
#[ignore = "M8-intra RED (spec m8_intra_opposite_plane_canonicalization): R0025 walls on the \
            intra-solid `coplanar input face pair` gate; GREEN when sign-aware canonicalization + \
            exactly-negated intra-exclusion land"]
fn red_r0025() {
    assert_no_intra_wall("R0025");
}

#[test]
#[ignore = "M8-intra RED (spec m8_intra_opposite_plane_canonicalization): R0031 walls on the \
            intra-solid `coplanar input face pair` gate; GREEN when sign-aware canonicalization + \
            exactly-negated intra-exclusion land"]
fn red_r0031() {
    assert_no_intra_wall("R0031");
}
