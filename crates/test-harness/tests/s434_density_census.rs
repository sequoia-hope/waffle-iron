//! I5-0 — §4.3.4 seam-polyline density census runner
//! (spec `specs/yang_441_trim_cdt_construction.md` §4-I5).
//!
//! Replays corpus cases IN-PROCESS with `YANG_434_CENSUS=1` so the
//! construct-pass census lines (`[s434-census] …`, printed per conic seam
//! group on pass 0) reach visible stderr — the assay subprocess runner nulls
//! child stderr (recorded trap), so a census cannot run through it.
//!
//! These are measurement vehicles, not oracles: they replay, print the
//! census plus the case's boolean failures for context, and pass. The
//! measured distribution is recorded in the spec (§4-I5), which is where the
//! I5-1 scoping decision lives.
//!
//! Run (single-threaded so per-case banners don't interleave):
//!
//! ```text
//! cargo test -p test-harness --test s434_density_census --release \
//!   -- --ignored --nocapture --test-threads=1
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

/// Replay one corpus case in-process with the census env set, printing a
/// banner so the `[s434-census]` lines that follow are attributable.
fn census_case(case_id: &str) {
    std::env::set_var("YANG_434_CENSUS", "1");
    eprintln!("[s434-census] ==== case {case_id} ====");
    let dir = assay_dir();
    let waffle_json = match fs::read_to_string(dir.join(format!("{case_id}.waffle"))) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[s434-census] {case_id}: cannot read case: {e}");
            return;
        }
    };
    let mut builder = ModelBuilder::kernel_v2();
    if let Err(e) = builder.load(&waffle_json) {
        eprintln!("[s434-census] {case_id}: LoadProject failed: {e}");
        return;
    }
    for (id, msg) in builder.engine_errors() {
        eprintln!("[s434-census] {case_id} engine error {id}: {msg}");
    }
    for w in builder
        .engine_warnings()
        .iter()
        .filter(|w| w.contains("Auto-union failed"))
    {
        eprintln!("[s434-census] {case_id} warning: {w}");
    }
    eprintln!("[s434-census] ==== case {case_id} done ====");
}

/// Hang guard (heavy exact arithmetic; a wedged case must not stall the
/// sweep). Mirrors the m8_earclip pattern: the orphaned worker keeps
/// running, the census moves on and says so.
fn census_case_with_timeout(case_id: &str, timeout: Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = case_id.to_string();
    let worker = id.clone();
    let handle = std::thread::spawn(move || {
        census_case(&worker);
        let _ = tx.send(());
    });
    match rx.recv_timeout(timeout) {
        Ok(()) => {
            let _ = handle.join();
        }
        Err(_) => eprintln!(
            "[s434-census] {id}: TIMEOUT after {}s (census incomplete)",
            timeout.as_secs()
        ),
    }
}

macro_rules! census {
    ($name:ident, $case:literal, $secs:literal, $why:literal) => {
        #[test]
        #[ignore = $why]
        fn $name() {
            census_case_with_timeout($case, Duration::from_secs($secs));
        }
    };
}

// CORRECT curved representatives — the at-scale density picture.
census!(
    census_f0059,
    "F0059",
    300,
    "I5-0 census vehicle (spec yang_441 §4-I5): F0059 runs 764 conic reorders — the largest \
     conic-seam population in the corpus"
);
census!(
    census_r0021,
    "R0021",
    120,
    "I5-0 census vehicle: R0021 plane-cylinder ring (the original N2 Mode-2 fixture, CORRECT)"
);
census!(
    census_r0072,
    "R0072",
    120,
    "I5-0 census vehicle: R0072 same-normal boss cut on a small-radius cylinder"
);
census!(
    census_c0053,
    "C0053",
    300,
    "I5-0 census vehicle: C0053 heavy curved CORRECT case"
);

// ERROR-family cases — measures whether they reach the construct pass at
// all, and their seam density where they do.
census!(
    census_f0045,
    "F0045",
    300,
    "I5-0 census vehicle: F0045 CDT-family ERROR (TessellationFailed)"
);
census!(
    census_r0011,
    "R0011",
    300,
    "I5-0 census vehicle: R0011 CDT-family ERROR (auto-union TessellationFailed)"
);
census!(
    census_r0028,
    "R0028",
    300,
    "I5-0 census vehicle: R0028 CDT-family ERROR (developable-cap overshoot anchor)"
);
census!(
    census_r0085,
    "R0085",
    300,
    "I5-0 census vehicle: R0085 CDT-family ERROR"
);
census!(
    census_c0067,
    "C0067",
    300,
    "I5-0 census vehicle: C0067 LocalRefinementRequired ERROR (grazing family)"
);
