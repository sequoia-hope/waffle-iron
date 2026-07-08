//! §4.5.3 straight-run reversal sweep — corpus trackers for R0072 / F0045.
//!
//! Spec: `specs/yang_453_junction_protected_collapse.md` §3c. The Stage-4
//! §4.5.3 sweep only processes all-conic loops, so a reversed point sequence
//! on a STRAIGHT intersection run (LineSegment seam) is never corrected:
//! Stage-4 relocates a seam × ruling-line triple point exactly onto its
//! junction, landing it BEHIND stale Stage-0 chord mints on the same seam —
//! the output loop doubles back along the exact line and kernel-v2's exact
//! CDT correctly refuses the self-intersecting ring:
//!
//! ```text
//! TessellationFailed { reason: "ring rejected by CDT (degenerate/self-intersecting)" }
//! ```
//!
//! RED today on both cases; GREEN when the line-run sweep lands (spec §3c
//! branch 6). The trackers also forbid the sibling render walls so the fix
//! cannot merely relocate the failure.
//!
//! Run: `cargo test -p test-harness --test s453_line_run_reversal`

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

/// The campaign wall: kernel-v2's exact planar CDT refusing a self-
/// intersecting output ring built from an uncorrected reversed seam run.
const CDT_REJECT_WALL: &str = "ring rejected by CDT";
/// Failure-moved signatures (spec §3c oracles) — forbidden as GREEN.
const RENDER_COLLAPSE_WALL: &str = "collapsed at render precision";
const INVERTED_TRI_WALL: &str = "inverted final triangle";

/// Boolean failures + final tessellation error of one corpus replay.
struct Replay {
    failures: Vec<String>,
    tess_err: Option<String>,
}

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

fn assert_no_cdt_reject(case_id: &str) {
    let r = replay_with_timeout(case_id, Duration::from_secs(200));
    let joined = r.failures.join("\n  ");
    let tess = r.tess_err.as_deref().unwrap_or("<ok>");

    assert!(
        !tess.contains(CDT_REJECT_WALL),
        "§4.5.3-line-run RED — {case_id} tessellation still refuses the \
         self-intersecting reversed-seam ring:\n  tess: {tess}\n  booleans:\n  {joined}"
    );
    assert!(
        !tess.contains(RENDER_COLLAPSE_WALL) && !tess.contains(INVERTED_TRI_WALL),
        "§4.5.3-line-run RED — {case_id} moved the failure into a sibling \
         render wall (spec §3c — not a valid GREEN):\n  tess: {tess}\n  booleans:\n  {joined}"
    );
}

#[test]
#[ignore = "PARTIAL (spec §3c final scope): the FaceId(9) straight-run spur is repaired \
            (4 line-site collapses fire; the wall MOVED to FaceId(11)), but face 11's \
            reversal sits on conic sites inside a MIXED cycle — a class DISPROVEN twice \
            (coarse-chord 45°-band false positives on the corner_in_band 7-gon; \
            overlay-adjacent repair of unsupported crossings on the hole-rim pin). \
            RED until a stable conic-site reversal criterion at coarse N exists \
            (spec §3c P10 records)"]
fn r0072_no_cdt_reject_wall() {
    assert_no_cdt_reject("R0072");
}

#[test]
#[ignore = "DIFFERENT MECHANISM (measured 2026-07-08, spec §3c status note): F0045's \
            FaceId(9) ring self-intersects at MACRO scale — segment 10→11 crosses \
            12→13 with a ~5e-2 excursion at model scale ~0.4 (12%), not a \
            MIN_FEATURE_SIZE-scale §4.5.3 reversal; the line-run sweep correctly \
            does not touch it. Stays RED as the pin for its own future campaign \
            (output-loop macro ordering / wrong-vertex-kept class)"]
fn f0045_no_cdt_reject_wall() {
    assert_no_cdt_reject("F0045");
}
