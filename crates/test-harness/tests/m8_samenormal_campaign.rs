//! M8 SAME-NORMAL DISC-RIM CROSSING — campaign harness + RED tests.
//!
//! ## What this is
//!
//! A development scaffold for the same-normal coplanar disc-rim crossing
//! campaign (roadmap M8). Same-normal crossings (two solids whose coplanar
//! faces share an outward normal — e.g. two bosses unioned on one base plane,
//! or a disc drilled by coaxial-sketch-plane cuts) are walled in production by
//! a loud `CoplanarFacesUnsupported` gate (`yang-rs/src/stage0.rs`), because
//! lifting it exposes a *fan* of distinct downstream failures — NOT one bug.
//!
//! These tests turn that fan into a tracked RED→GREEN checklist. Each is
//! `#[ignore]`d (so plain `cargo test` stays green) and asserts the END GOAL:
//! the corpus case replays through the full kernel-v2 dispatch with NO boolean
//! failure. They are RED today; as each downstream mode is fixed, its test
//! goes GREEN and its `#[ignore]` is removed.
//!
//! ## The dev harness
//!
//! The production wall stays up by default (P9 — a loud `NotSupported` is never
//! a wrong result). Setting the env var **`YANG_M8_SAMENORMAL_DEV=1`** lifts the
//! wall in `stage0.rs` so the pipeline runs past Stage-0 and the real
//! downstream failure (or success) surfaces. Each test sets it itself. Run the
//! checklist with:
//!
//! ```text
//! cargo test -p test-harness --test m8_samenormal_campaign -- --ignored --nocapture
//! ```
//!
//! Production behaviour (env unset) is byte-identical to the pre-harness wall;
//! the assay's `coplanar-boolean` count is unchanged.
//!
//! ## The modes (per the gate-lift diagnostic, 2026-06-24)
//!
//! With the wall lifted, the 9 same-normal cases fail across 6 modes. The
//! `#[ignore]` reason on each test names its mode. The multi-solid §4.5.5
//! overlap mechanism itself ALREADY works (182/182 overlap triangles resolve);
//! these are the surrounding gaps:
//!
//! | Mode | Cases | Fix |
//! |------|-------|-----|
//! | Stage-6 scale-relative planar attribution tol (non-pair planar face of a curved input; centroid ~1e-12 off plane vs absolute TAU_WORK at model scale) | R0013, R0024 | extend the 707aad7f pair-plane scale-relative band to non-pair planar faces |
//! | Stage-4 relocation `DegenerateTriangle` | R0021 | §4.5.3 region repair |
//! | Stage-3 SSI `AmbiguousCurve` (cyl∩plane near-tangency → 2 near-coincident parallel lines; tangent discriminator can't separate parallels) | R0072 | add a POSITION tie-break for parallel-line candidates |
//! | kernel-v2 azimuth-merge rims disagree (reassembly) | R0078 | rim-merge robustness |
//! | cherchi TIMEOUT (loops on the coincident same-winding overlap input) | R0063 | cherchi guard / single-shared-sheet Stage-0 |
//! | residual 2nd coplanar pair (the env lifts the first pair; a second pair hits a different gate) | R0076, R0088, F0061 | once the above land, re-scope the remaining pair |
//!
//! When ALL in-scope modes are GREEN, the `stage0.rs` env gate is replaced by a
//! per-pair safety predicate (or removed). See the `kernel_v2_m8_coplanar_landscape`
//! memo for the full campaign plan.

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

/// Replay one corpus case through the full kernel-v2 dispatch WITH the
/// same-normal dev wall lifted, and return any boolean-failure messages.
///
/// A boolean failure surfaces either as an engine error (a `cut`/subtract
/// rebuild failure) or as an `"Auto-union failed: …"` warning (the merge=true
/// boss path downgrades a boolean error to a warning). An empty result means
/// every boolean in the case succeeded — the per-case campaign goal.
fn boolean_failures(case_id: &str) -> Vec<String> {
    // Lift the production wall for this process (every test in this binary
    // wants it lifted; separate test binaries are separate processes, so this
    // never leaks to the assay or other suites).
    std::env::set_var("YANG_M8_SAMENORMAL_DEV", "1");

    let waffle_path = assay_dir().join(format!("{case_id}.waffle"));
    let waffle_json = match fs::read_to_string(&waffle_path) {
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

/// Assert a case replays with no boolean failure (the GREEN target). The panic
/// message carries the actual downstream failure so the RED run documents which
/// mode still blocks the case.
fn assert_builds(case_id: &str) {
    let failures = boolean_failures(case_id);
    assert!(
        failures.is_empty(),
        "M8 same-normal RED — {case_id} still fails its boolean(s):\n  {}",
        failures.join("\n  ")
    );
}

// ── Mode 1: Stage-6 scale-relative planar attribution tolerance ────────────

#[test]
#[ignore = "M8 same-normal RED (Stage-6 scale-relative planar tol): GREEN when non-pair planar faces of a curved input use a scale-relative membership band"]
fn red_r0013_stage6_planar_tol() {
    assert_builds("R0013");
}

#[test]
#[ignore = "M8 same-normal RED (Stage-6 scale-relative planar tol): GREEN when non-pair planar faces of a curved input use a scale-relative membership band"]
fn red_r0024_stage6_planar_tol() {
    assert_builds("R0024");
}

// ── Mode 2: Stage-4 relocation DegenerateTriangle ──────────────────────────

#[test]
#[ignore = "M8 same-normal RED (Stage-4 relocation DegenerateTriangle): GREEN when the §4.5.3 region repair handles the same-normal overlap boundary"]
fn red_r0021_stage4_relocation() {
    assert_builds("R0021");
}

// ── Mode 3: Stage-3 SSI AmbiguousCurve (cyl∩plane near-tangency) ────────────

#[test]
#[ignore = "M8 same-normal RED (Stage-3 SSI AmbiguousCurve): GREEN when the curve selector adds a POSITION tie-break for near-coincident parallel-line candidates"]
fn red_r0072_stage3_ambiguous_parallel_lines() {
    assert_builds("R0072");
}

// ── Mode 4: kernel-v2 azimuth-merge rims disagree (reassembly) ──────────────

#[test]
#[ignore = "M8 same-normal RED (kernel-v2 azimuth-merge rims disagree): GREEN when reassembly rim-merge tolerates the same-normal rim split"]
fn red_r0078_kernel_azimuth_merge() {
    assert_builds("R0078");
}

// ── Mode 5: cherchi TIMEOUT (coincident same-winding overlap) ──────────────

// NOTE: this case currently HANGS cherchi (loops on the coincident
// same-winding overlap sheets). It is left WITHOUT a body until the timeout
// guard / single-shared-sheet Stage-0 lands — running it would wedge the
// suite. Documented here so the mode is not lost; add the body with the fix.
//
// fn red_r0063_cherchi_timeout() { assert_builds("R0063"); }

// ── Mode 6: residual 2nd coplanar pair ─────────────────────────────────────

#[test]
#[ignore = "M8 same-normal RED (residual 2nd coplanar pair): GREEN when the second coplanar pair's gate (not lifted by the same-normal env) is also resolved"]
fn red_r0076_residual_pair() {
    assert_builds("R0076");
}

#[test]
#[ignore = "M8 same-normal RED (residual 2nd coplanar pair): GREEN when the second coplanar pair's gate (not lifted by the same-normal env) is also resolved"]
fn red_r0088_residual_pair() {
    assert_builds("R0088");
}

#[test]
#[ignore = "M8 same-normal RED (residual 2nd coplanar pair): GREEN when the second coplanar pair's gate (not lifted by the same-normal env) is also resolved"]
fn red_f0061_residual_pair() {
    assert_builds("F0061");
}

// ── Harness self-check (always on) ─────────────────────────────────────────

/// The harness itself must work: with the dev env lifted, a same-normal case
/// must get PAST the Stage-0 coplanar wall (i.e. its failure is now a
/// downstream mode, NOT the `coplanar input face pair` wall). This guards
/// against the env toggle silently breaking. It does NOT require the case to
/// succeed — only that the wall is actually lifted.
#[test]
fn harness_lifts_the_wall() {
    let failures = boolean_failures("R0013");
    assert!(
        !failures.is_empty(),
        "expected R0013 to still fail downstream (RED); if it now builds, \
         remove this self-check and un-ignore red_r0013_stage6_planar_tol"
    );
    assert!(
        !failures
            .iter()
            .any(|f| f.contains("coplanar input face pair")),
        "harness BROKEN: the same-normal wall was NOT lifted by \
         YANG_M8_SAMENORMAL_DEV — R0013 still hit the Stage-0 coplanar wall:\n  {}",
        failures.join("\n  ")
    );
}
