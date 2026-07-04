//! M8 Stage-0 inputcheck-clean emission — E2E RED trackers
//! (spec `specs/m8_stage0_inputcheck_clean_emission.md` §5 I5 / §6).
//!
//! The acceptance cases' CURRENT walls, measured by the Increment-0
//! diagnosis (spec §2):
//!
//! - R0046 / R0088 — the defective Stage-0 operands (M-A split drop + M-B
//!   resolution collapse) drive the native boolean to a defective kept set;
//!   kernel-v2 rejects it with `InvalidBooleanOutput("an undirected output
//!   edge is not used by exactly two directed edges")`.
//! - F0063 — the defective union output is accepted silently and the NEXT
//!   feature's auto-union walls on re-entry:
//!   `BooleanFailed("yang-rs: input B-Rep is not 2-manifold")`.
//!
//! GREEN semantics (the campaign convention): the named wall string absent —
//! SUCCESS or a DIFFERENT loud typed error both pass (layered blockers are
//! expected). Caveat recorded in spec §2: R0088's second failing subtract
//! emits five-axiom-CLEAN operands, so its instance of the same wall string
//! is a separate residual — `red_r0088` therefore only demands the count of
//! that wall DROPS below two (the defective-operand op's instance falls);
//! full absence is the follow-up's target.
//!
//! `#[ignore]`d; run with:
//!
//! ```text
//! cargo test -p test-harness --test m8_stage0_inputcheck_campaign -- --ignored --nocapture
//! ```

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use test_harness::ModelBuilder;

/// kernel-v2's post-subdivision edge-pairing rejection — the R0046/R0088
/// wall (kernel-v2/src/boolean.rs `InvalidBooleanOutput`).
const EDGE_PAIRING_WALL: &str =
    "an undirected output edge is not used by exactly two directed edges";

/// yang-rs's input re-entry rejection — F0063's wall (the defective op-0/1
/// union output walls the NEXT op's input).
const NONMANIFOLD_INPUT_WALL: &str = "input B-Rep is not 2-manifold";

fn assay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("app/tests/cases/assay")
}

/// Replay one corpus case through the full kernel-v2 dispatch and return every
/// boolean failure (engine errors + `Auto-union failed` warnings). Mirrors
/// `m8_rim_clustering_campaign.rs`.
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

/// Replay with a hang guard (heavy exact arithmetic cannot be killed
/// in-process; a timeout is reported as its own non-wall failure).
fn boolean_failures_with_timeout(case_id: &str, timeout: Duration) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = case_id.to_string();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(boolean_failures(&worker));
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => {
            let _ = handle.join();
            r
        }
        Err(_) => vec![format!(
            "{case_id}: timeout after {}s (worker orphaned)",
            timeout.as_secs()
        )],
    }
}

fn wall_count(case_id: &str, wall: &str) -> (usize, Vec<String>) {
    let failures = boolean_failures_with_timeout(case_id, Duration::from_secs(200));
    let n = failures.iter().filter(|f| f.contains(wall)).count();
    (n, failures)
}

#[test]
#[ignore = "M8 Stage-0 emission RED (spec m8_stage0_inputcheck_clean_emission I5): R0046's \
            defective-operand subtract yields a defective kept set → kernel-v2 edge-pairing \
            wall; GREEN when the emission fix lands (success or a different loud error pass)"]
fn red_r0046() {
    let (n, failures) = wall_count("R0046", EDGE_PAIRING_WALL);
    assert_eq!(
        n,
        0,
        "M8 Stage-0 RED — R0046 still walls on kernel-v2 edge pairing:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
#[ignore = "M-C band-scale RED (spec m8_stage0_band_scale_crossing_verts I4): the parent \
            cycle's fix dropped R0088 to ONE edge-pairing wall instance (the band-scale \
            operand op); this cycle's exact-dedup rim insertion must clear it — GREEN when \
            the wall string is fully absent (success or a different loud error pass)"]
fn red_r0088() {
    let (n, failures) = wall_count("R0088", EDGE_PAIRING_WALL);
    assert_eq!(
        n,
        0,
        "M-C RED — R0088 still carries {n} instance(s) of the kernel-v2 edge-pairing wall \
         (band-scale rim-override drop unfixed):\n  {}",
        failures.join("\n  ")
    );
}

#[test]
#[ignore = "M8 Stage-0 emission RED (spec m8_stage0_inputcheck_clean_emission I5): F0063's \
            defective union output re-enters the next op and walls as non-2-manifold input; \
            GREEN when the emission fix lands (success or a different loud error pass)"]
fn red_f0063() {
    let (n, failures) = wall_count("F0063", NONMANIFOLD_INPUT_WALL);
    assert_eq!(
        n,
        0,
        "M8 Stage-0 RED — F0063 still walls on non-2-manifold re-entry:\n  {}",
        failures.join("\n  ")
    );
}
