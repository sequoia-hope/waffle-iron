//! N2-3a campaign delta — RED replay oracle for the Stage-4 Fig-11(b)
//! junction-cluster merge (spec: `specs/n2_stage4_junction_cluster_merge.md`).
//!
//! R0072 currently fails Extrude 2's auto-union with kernel-v2
//! `VertexOffSurface { FaceId(11) }`: the boolean COMPLETES in yang-rs but
//! its output cylinder face carries boundary-loop vertices off the analytic
//! cylinder (12 of them measured — 11 coplanar-overlay rim vertices at
//! chord-sagitta residuals 5.4e-6..7.3e-6 plus the diagnostic's v7
//! tangency-cluster member at 1.607e-6, all ≫ the import band ≈ 1e-9), and
//! kernel-v2's debug-tier vertex-on-surface tripwire rejects the import.
//! The mechanism-level fixture lives in
//! `crates/yang-rs/tests/n2_junction_cluster.rs` (see its header for the
//! measured population and the spec-scope note).
//!
//! This test pins the CAMPAIGN acceptance (spec §5): after the fix, R0072's
//! replay must no longer contain a `VertexOffSurface` failure. It does NOT
//! require full oracle-correctness — if a different downstream wall then
//! surfaces, this test stays green and the still-RED
//! `red_r0072_stage3_ambiguous_parallel_lines` in
//! `m8_samenormal_campaign.rs` gets its `#[ignore]` reason repointed
//! (honest-harness rule). Un-`#[ignore]` this test when the fix lands.
//!
//! The replay helper is a copy of `m8_samenormal_campaign.rs`'s
//! `replay_failures` boolean-failure arm (that file is not modified per the
//! Test-Author ground rules); the full oracle gauntlet is intentionally NOT
//! copied — the assertion here is only about the `VertexOffSurface` mode.
//!
//! ## R0096 probe verdict (spec §5, Test-Author phase 2026-07-02)
//!
//! R0096's assay error — "Stage-4 relocation region around vertex 7 is
//! invalid: LocalRefinementRequired" — is a **DIFFERENT mode**, NOT the
//! junction-cluster class, so it gets NO red test in this increment.
//! Instrumented evidence: R0096 is `revolve(circle,boss) + revolve(circle,
//! cut)` — a TORUS × TORUS boolean — and its error fires at the KV6d Tier-B
//! deliberate v1-scope STOP for a torus∩torus intersection edge
//! (`yang-rs/src/lib.rs` (2t) block, "torus∩torus (degree-4 with no single
//! base surface) — out of v1 scope": probed `site=torus-torus-edge s=7
//! e=204 tori=2 others=0`; vertex 7 is in NO conic relocation map). That is
//! a correct, deliberate capability wall (torus×torus SSI), untouched by
//! N2-3a.

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

/// Replay one corpus case through the full kernel-v2 dispatch and return
/// every boolean failure — engine errors (cut/subtract) plus "Auto-union
/// failed" warnings (the merge=true boss path downgrades a boolean error to
/// a warning). Copied from `m8_samenormal_campaign.rs::replay_failures`
/// (boolean-failure arm; the same-normal dev env is set for parity with that
/// harness even though the production wall no longer consults it).
fn replay_boolean_failures(case_id: &str) -> Vec<String> {
    std::env::set_var("YANG_M8_SAMENORMAL_DEV", "1");

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

/// **RED today.** R0072's replay must not fail with `VertexOffSurface` —
/// the Stage-4 output's cylinder-face loop vertices must all lie on the
/// analytic cylinder within kernel-v2's import band (Yang §4.4.1 Fig 11(b):
/// junction-cluster members merged onto the relocated on-curve point q, and
/// coplanar-overlay rim vertices on the exact rim).
#[test]
#[ignore = "N2-3a RED (Stage-4 junction-cluster / overlay-rim off-surface vertices): \
            R0072's union output is rejected by kernel-v2 VertexOffSurface — GREEN \
            when the Fig-11(b) merge onto the relocated point q (spec \
            n2_stage4_junction_cluster_merge) lands"]
fn red_r0072_vertex_off_surface() {
    let failures = replay_boolean_failures("R0072");
    assert!(
        !failures.iter().any(|f| f.contains("VertexOffSurface")),
        "N2-3a: R0072 still fails with kernel-v2 VertexOffSurface — the Stage-4 \
         output carries off-surface cylinder loop vertices (measured: 12, \
         residuals 1.6e-6..7.3e-6 vs band ~1e-9):\n  {}",
        failures.join("\n  ")
    );
}
