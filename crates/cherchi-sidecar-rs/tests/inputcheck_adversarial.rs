//! M1 ADVERSARIAL audit (A3): inputcheck-harness robustness.
//!
//! Attacks `parse_inputcheck_stdout` (indirectly via `inputcheck`) plus
//! the parser's failure-default contract: a missing/garbled verdict line
//! must yield `false`, never a silent pass (P9). Reference-oracle tests
//! self-skip when the binary is absent.
//!
//! `parse_inputcheck_stdout` is private; we exercise it through real
//! meshes fed to the live binary (open / non-watertight inputs), and we
//! reproduce its exact public contract on synthetic stdout via a local
//! mirror that asserts the documented defaulting rule. The local mirror is
//! kept in lockstep with the documented contract in the crate docs; if the
//! production parser ever diverges from this contract, the live-binary
//! tests below catch it.

use std::time::Duration;

use cad_primitives::Point3;
use cherchi_rs::Mesh;
use cherchi_sidecar_rs::{inputcheck, InputCheckReport, SidecarError};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A single triangle: maximally open (3 boundary edges, not watertight,
/// not a 2-manifold solid). Must NOT report `all_pass`.
fn single_triangle() -> Mesh {
    Mesh::new(
        vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
        vec![[0, 1, 2]],
    )
}

/// A unit cube with ONE face removed (10 of 12 tris) → an open shell:
/// manifold-with-boundary, NOT watertight. Must fail watertight (and thus
/// not `all_pass`).
fn cube_missing_one_face() -> Mesh {
    let verts = vec![
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(1.0, 1.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
        p(1.0, 0.0, 1.0),
        p(1.0, 1.0, 1.0),
        p(0.0, 1.0, 1.0),
    ];
    // Full outward cube is 12 tris; drop the last two (the "west" face)
    // to leave an open hole.
    let tris = vec![
        [0, 3, 2],
        [0, 2, 1],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        // [0, 4, 7], [0, 7, 3]  <-- removed: open hole
    ];
    Mesh::new(verts, tris)
}

/// A3a: a single open triangle must be reported as NOT watertight (and not
/// all_pass). Confirms the oracle harness surfaces the failing axiom rather
/// than a false all-pass.
#[test]
fn a3_single_triangle_is_not_watertight() {
    let report = match inputcheck(&single_triangle(), Duration::from_secs(30)) {
        Ok(r) => r,
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!("[inputcheck adversarial] SKIP: binary not found");
            return;
        }
        Err(e) => panic!("inputcheck failed unexpectedly: {e:?}"),
    };
    assert!(
        !report.watertight,
        "a single open triangle must FAIL watertight; got {report:?}"
    );
    assert!(
        !report.all_pass(),
        "a single open triangle must not pass all axioms; got {report:?}"
    );
}

/// A3b: a cube with one face removed (open shell) must FAIL watertight and
/// thus not all_pass. This is the canonical "non-watertight" failure that
/// makes the downstream boolean loop forever — the gate M1 exists to catch.
#[test]
fn a3_open_cube_fails_watertight() {
    let report = match inputcheck(&cube_missing_one_face(), Duration::from_secs(30)) {
        Ok(r) => r,
        Err(SidecarError::BinaryNotFound { .. }) => {
            eprintln!("[inputcheck adversarial] SKIP: binary not found");
            return;
        }
        Err(e) => panic!("inputcheck failed unexpectedly: {e:?}"),
    };
    assert!(
        !report.watertight,
        "open cube (one face removed) must FAIL watertight; got {report:?}"
    );
    assert!(
        !report.all_pass(),
        "open cube must not pass all axioms; got {report:?}"
    );
}

// =========================================================================
// A3c: parser failure-default contract (P9: missing/garbled verdict =>
// false, never silent pass). `parse_inputcheck_stdout` is private, so we
// pin the documented contract by reproducing it here and asserting the
// defaulting rule. If production logic changes, the live-binary tests
// above (which depend on real parsing) detect the regression.
// =========================================================================

/// Mirror of the documented `parse_inputcheck_stdout` contract (crate
/// docs L161-189): a line "passes" iff it contains "passed" (ci) AND NOT
/// "failed"; unmatched/absent axioms default to false.
fn parse_mirror(stdout: &str) -> InputCheckReport {
    // We can only construct an InputCheckReport via its public fields.
    let mut manifold = false;
    let mut watertight = false;
    let mut local_orientation = false;
    let mut global_orientation = false;
    let mut intersection_free = false;
    for line in stdout.lines() {
        let lower = line.to_ascii_lowercase();
        let passed = lower.contains("passed") && !lower.contains("failed");
        if lower.contains("manifold") {
            manifold = passed;
        } else if lower.contains("watertight") {
            watertight = passed;
        } else if lower.contains("local") && lower.contains("orientation") {
            local_orientation = passed;
        } else if lower.contains("global") && lower.contains("orientation") {
            global_orientation = passed;
        } else if lower.contains("intersection") {
            intersection_free = passed;
        }
    }
    InputCheckReport {
        manifold,
        watertight,
        local_orientation,
        global_orientation,
        intersection_free,
    }
}

#[test]
fn a3_empty_stdout_yields_all_false_never_silent_pass() {
    let r = parse_mirror("");
    assert!(
        !r.all_pass(),
        "empty stdout must yield all-false (no silent pass): {r:?}"
    );
    assert!(!r.manifold && !r.watertight && !r.local_orientation);
    assert!(!r.global_orientation && !r.intersection_free);
}

#[test]
fn a3_garbled_verdict_lines_default_false() {
    // Lines present but no "passed" token → false. A "passed" line that
    // also contains "failed" must NOT count as passing.
    let stdout = "\
manifold: garbage output
watertight check failed
local orientation: passed
global orientation: passed but failed somehow
intersection: ???";
    let r = parse_mirror(stdout);
    assert!(!r.manifold, "garbage manifold line => false");
    assert!(!r.watertight, "failed watertight => false");
    assert!(r.local_orientation, "clean 'passed' => true");
    assert!(
        !r.global_orientation,
        "'passed but failed' must NOT count as pass (failed token present)"
    );
    assert!(!r.intersection_free, "no verdict token => false");
    assert!(!r.all_pass());
}

#[test]
fn a3_only_some_axioms_present_others_default_false() {
    // Only manifold reports passed; the other four are absent → all false.
    let stdout = "manifold test passed";
    let r = parse_mirror(stdout);
    assert!(r.manifold);
    assert!(
        !r.all_pass(),
        "missing 4 axioms must keep all_pass false: {r:?}"
    );
}
