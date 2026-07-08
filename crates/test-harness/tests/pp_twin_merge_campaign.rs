//! §4.4.1(b) merge eligibility for pp-triple-point twins — R0012 tracker.
//!
//! Spec: `specs/kv9_f3_output_vertex_identity.md` §4 row E-V7 (ERROR-census
//! campaign 5). R0012's gear×gear subtract emits two output-loop vertices
//! 7.1e-7 apart (below the A14.2 feature floor, definitionally the same
//! point) that survive into a face loop; the always-on G1 render-precision
//! gate then fails the face loudly:
//!
//! ```text
//! TessellationFailed { reason: "planar triangle collapsed at render precision" }
//! ```
//!
//! The §4.4.1(b) sub-feature merge missed them: its scan eligibility (E-V2)
//! covers the conic-endpoint maps, but pure plane∩plane junction vertices
//! (gear-flank seams) only appear in `vert_pp_planes`. RED today; GREEN when
//! E-V7 extends eligibility to pp-triple-point vertices (the merge CRITERION
//! — degenerate triangle + sub-floor shortest edge — is unchanged).
//!
//! Run: `cargo test -p test-harness --test pp_twin_merge_campaign`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

const RENDER_COLLAPSE_WALL: &str = "collapsed at render precision";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

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
    let failures: Vec<String> = builder
        .engine_errors()
        .iter()
        .map(|(id, msg)| format!("error {id}: {msg}"))
        .collect();
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

fn assert_no_render_collapse(case_id: &str) {
    let r = replay_with_timeout(case_id, Duration::from_secs(200));
    let joined = r.failures.join("\n  ");
    let tess = r.tess_err.as_deref().unwrap_or("<ok>");
    assert!(
        !tess.contains(RENDER_COLLAPSE_WALL),
        "pp-twin-merge RED — {case_id} still carries sub-floor output twins \
         into a face loop (G1 gate):\n  tess: {tess}\n  booleans:\n  {joined}"
    );
}

#[test]
#[ignore = "MEASURED, NOT SHIPPED (spec kv9_f3 §4 row E-V7): the blocker is the merge \
            CRITERION, not eligibility — the 7.1e-7 twins' incident triangles are long \
            slivers (~1e-5 area ≫ floor²) so the degenerate-triangle precondition never \
            fires; a vertex-distance criterion is the P10-reverted micro-scale hazard. \
            RED until a scale-aware sub-feature floor is derived or the twins' minting \
            is fixed at its source (measure which stage mints them first)"]
fn r0012_no_render_collapse_wall() {
    assert_no_render_collapse("R0012");
}
