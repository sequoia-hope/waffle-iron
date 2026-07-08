//! D-face bigon vocabulary — corpus trackers for R0046 / F0064 / R0088.
//!
//! Spec: `specs/kv9_f3_output_vertex_identity.md` §4 row E-V6 (ERROR-census
//! campaign 4). A boolean output face bounded by exactly a CHORD and its
//! CONIC ARC (a circular/elliptic segment — the "D-face") is legitimate
//! B-Rep geometry, but `from_yang_brep`'s loop vocabulary rejected every
//! 2-edge loop containing a `Seg`:
//!
//! ```text
//! InvalidBooleanOutput("output loop with fewer than 3 edges and no full-circle edge")
//! ```
//!
//! RED today on all three cases; GREEN when the E-V6 acceptance lands.
//!
//! Run: `cargo test -p test-harness --test dface_bigon_campaign`

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

const SHORT_LOOP_WALL: &str = "output loop with fewer than 3 edges";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

fn replay_failures(case_id: &str) -> Vec<String> {
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

fn replay_failures_with_timeout(case_id: &str, timeout: Duration) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(replay_failures(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => vec![format!("{id}: timeout after {}s", timeout.as_secs())],
    }
}

fn assert_no_short_loop_wall(case_id: &str) {
    let failures = replay_failures_with_timeout(case_id, Duration::from_secs(200));
    let joined = failures.join("\n  ");
    assert!(
        !failures.iter().any(|f| f.contains(SHORT_LOOP_WALL)),
        "D-face-bigon RED — {case_id} still rejects the chord+arc segment \
         face:\n  {joined}"
    );
}

#[test]
fn r0046_no_short_loop_wall() {
    assert_no_short_loop_wall("R0046");
}

#[test]
fn f0064_no_short_loop_wall() {
    assert_no_short_loop_wall("F0064");
}

#[test]
fn r0088_no_short_loop_wall() {
    assert_no_short_loop_wall("R0088");
}
