//! PR-Y15b Phase 1 — Red-phase parity tests for the F0002-class
//! `combined_failures` cohort.
//!
//! Per the PR-Y15b spec (`specs/yang_pr_y15b_pre_cherchi_input_validation.md`)
//! and the Phase-0 diagnostic memo (`docs/audits/pr_y15b_phase0_diagnostic.md`):
//!
//! - F0002 and F0004 produce pre-Cherchi A/B meshes that
//!   `mesh_booleans_inputcheck` rejects with mask `(M=1,W=1,LO=0,GO=0,I=1)`
//!   ("M+W+I" — F0002 reference signature) on BOTH sides.
//! - The cluster birth is at `inject_face_with_shared_first` step 2
//!   (`crates/kernel/src/boolean/coplanar_preprocess.rs:1741-1747`) per
//!   Phase 0's verdict (verbatim probe output in the diagnostic memo).
//! - F0003 is a pass-boss-only control: its Waffle status is `Passed`
//!   today and MUST stay `Passed` post-fix (spec §1 + amendment 3 +
//!   §6.5). Catches accidental tessellation regressions on
//!   pass-boss-only paths.
//! - R0014 / R0017 are second-tier reproducers (spec §1.46) — they're in
//!   the same `combined_failures` bucket per PR-S2's sweep. Pinned in a
//!   single test (saves harness setup cost) so a regression on either
//!   fires its own sub-report.
//!
//! All four tests are `#[ignore]` per the standing pattern for tests that
//! require `YANG_BOOLEAN=1` and the Cherchi sidecar binary.
//!
//! Each test wraps the Waffle invocation in a 60-second WAFFLE_TIMEOUT
//! thread per `cherchi_inputcheck_corpus_sweep.rs`'s pattern (PR-S3
//! backport) so a kernel hang doesn't stall the test runner. Subprocess
//! timeout for `mesh_booleans_inputcheck` is 10 s per the PR-S2 pattern
//! — the validator is fast on well-formed input (<1 s) and 10 s is
//! generous enough that any case exceeding it is itself diagnostic.
//!
//! FIP §4 / spec §10 binding: this file is RED today (3 of 4 tests
//! fail) and turns GREEN post-Phase-2-fix. The F0003 control is GREEN
//! today and MUST stay GREEN.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use test_harness::assay::randomized_runner::run_single_case;
use test_harness::assay::scoring::{AssayResult, AssayStatus};
use test_harness::cherchi_sidecar::{cherchi_bin, run_with_timeout, TimedRun};

/// Per-case Waffle timeout. Mirrors the PR-S3 backport in
/// `cherchi_inputcheck_corpus_sweep.rs:63` (60 s). R0071 (gear+revolve at
/// scale 1.86e-4) hangs `run_single_case` indefinitely; without the
/// timeout, R-case tests would stall the runner. 60 s is generous for
/// normal cases (~3 s).
const WAFFLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Per-invocation `mesh_booleans_inputcheck` timeout. Matches the PR-S2
/// pattern (`cherchi_inputcheck_corpus_sweep.rs:39`). The validator is
/// fast on well-formed input (<1 s); cases exceeding 10 s are
/// pathological and the timeout signal itself is informative.
const INPUTCHECK_TIMEOUT: Duration = Duration::from_secs(10);

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Resolve the `mesh_booleans_inputcheck` binary path. Same shape as the
/// helper in `cherchi_inputcheck_corpus_sweep.rs:77` (inlined here per
/// FIP role boundary — test-author cannot modify
/// `crates/test-harness/src/cherchi_sidecar.rs`). The validator lives
/// next to `mesh_booleans` in the upstream Cherchi build dir; both are
/// built by the same upstream Makefile.
fn cherchi_inputcheck_bin() -> Option<PathBuf> {
    let base = cherchi_bin()?;
    let parent = base.parent()?;
    let candidate = parent.join("mesh_booleans_inputcheck");
    if !candidate.exists() {
        eprintln!(
            "[pr-y15b-parity] SKIP: `mesh_booleans_inputcheck` not found at `{}`. \
             Build it per the upstream README (it's built alongside `mesh_booleans`).",
            candidate.display()
        );
        return None;
    }
    Some(candidate)
}

/// Per-process temp dir for OBJ dumps. PID-stamped to avoid cross-test
/// contamination when multiple test binaries run on the same host.
fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("waffle_pr_y15b_{}", std::process::id()))
}

/// Run a single assay case with `YANG_BOOLEAN=1` (and optionally
/// `YANG_DUMP_OBJ_BASE`) on a worker thread, returning the
/// `AssayResult` or `None` on timeout / case-not-found. Matches the
/// PR-S3 thread-wrap pattern in `cherchi_inputcheck_corpus_sweep.rs:340`.
///
/// `dump_base` is forwarded to the `YANG_DUMP_OBJ_BASE` env var for the
/// duration of the run; pass `None` to skip the dump (saves disk for
/// status-only assertions).
fn run_case_with_timeout(
    dir: &Path,
    case_id: &str,
    dump_base: Option<&str>,
) -> (Option<AssayResult>, bool) {
    std::env::set_var("YANG_BOOLEAN", "1");
    if let Some(b) = dump_base {
        std::env::set_var("YANG_DUMP_OBJ_BASE", b);
    } else {
        std::env::remove_var("YANG_DUMP_OBJ_BASE");
    }

    let dir_owned = dir.to_path_buf();
    let id_owned = case_id.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let r = run_single_case(&dir_owned, &id_owned, true);
        let _ = tx.send(r);
    });
    let outcome = match rx.recv_timeout(WAFFLE_TIMEOUT) {
        Ok(r) => {
            let _ = handle.join();
            (r, false)
        }
        Err(_) => {
            eprintln!(
                "[pr-y15b-parity] WAFFLE_TIMEOUT on {} after {}s — leaking thread",
                case_id,
                WAFFLE_TIMEOUT.as_secs()
            );
            (None, true)
        }
    };

    std::env::remove_var("YANG_DUMP_OBJ_BASE");
    outcome
}

/// Result of running `mesh_booleans_inputcheck` on one OBJ. The 5
/// booleans are TRUE on `passed`, FALSE on `failed`/`FAILED`. `None`
/// means the validator did not produce a parseable line for that check
/// (treated as a hard test failure — the validator is supposed to print
/// all 5).
#[derive(Debug, Clone)]
struct InputcheckOutcome {
    manifold: Option<bool>,
    watertight: Option<bool>,
    local_orientation: Option<bool>,
    global_orientation: Option<bool>,
    intersection: Option<bool>,
    raw: String,
}

impl InputcheckOutcome {
    fn all_passed(&self) -> bool {
        self.manifold == Some(true)
            && self.watertight == Some(true)
            && self.local_orientation == Some(true)
            && self.global_orientation == Some(true)
            && self.intersection == Some(true)
    }
}

/// Parse `mesh_booleans_inputcheck` output. Case-insensitive on the
/// `passed`/`failed` keyword per the PR-S2 spec (the build guide uses
/// lowercase, F0002 captures use uppercase). Mirrors
/// `parse_inputcheck_output` in `cherchi_inputcheck_corpus_sweep.rs:183`.
fn parse_inputcheck(text: &str) -> InputcheckOutcome {
    let prefixes: [(&str, fn(&mut InputcheckOutcome, bool)); 5] = [
        ("Manifold check", |o, p| o.manifold = Some(p)),
        ("Watertight check", |o, p| o.watertight = Some(p)),
        ("Local  Orientation check", |o, p| {
            o.local_orientation = Some(p)
        }),
        ("Global Orientation check", |o, p| {
            o.global_orientation = Some(p)
        }),
        ("Intersection check", |o, p| o.intersection = Some(p)),
    ];
    let mut out = InputcheckOutcome {
        manifold: None,
        watertight: None,
        local_orientation: None,
        global_orientation: None,
        intersection: None,
        raw: text.to_string(),
    };
    for line in text.lines() {
        for (prefix, setter) in prefixes.iter() {
            if line.contains(prefix) {
                let ll = line.to_ascii_lowercase();
                if ll.contains("failed") {
                    setter(&mut out, false);
                } else if ll.contains("passed") {
                    setter(&mut out, true);
                }
                break;
            }
        }
    }
    out
}

/// Spawn `mesh_booleans_inputcheck <obj>` with the PR-S2 10-s subprocess
/// timeout. Returns `Err(reason)` on timeout / spawn failure / parse
/// failure (the validator is supposed to always emit all 5 lines on a
/// well-formed dump path; missing lines indicate a Cinolib loader error
/// upstream).
fn run_inputcheck(bin: &Path, obj: &Path) -> Result<InputcheckOutcome, String> {
    let mut cmd = Command::new(bin);
    cmd.arg(obj);
    match run_with_timeout(cmd, INPUTCHECK_TIMEOUT) {
        TimedRun::TimedOut => Err(format!(
            "inputcheck subprocess TIMEOUT at {}s on {}",
            INPUTCHECK_TIMEOUT.as_secs(),
            obj.display()
        )),
        TimedRun::SpawnFailed(e) => Err(format!("inputcheck spawn failed: {}", e)),
        TimedRun::Completed(out) => {
            // Per PR-S2 §2 empirical correction: the 5 check lines land
            // on STDOUT; concatenating both streams is robust to either.
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            Ok(parse_inputcheck(&combined))
        }
    }
}

/// Assert all 5 inputcheck axioms passed on one OBJ. Panics with a
/// detailed multi-line message on failure so the test failure record
/// names exactly which check(s) failed and embeds the validator's raw
/// output for debugging.
fn assert_inputcheck_all_passed(bin: &Path, obj: &Path, side: &str, case_id: &str) {
    assert!(
        obj.exists(),
        "{} side {} OBJ does not exist at `{}` — kernel did not write the dump. \
         Either YANG_DUMP_OBJ_BASE is outside the kernel's writable scope, or \
         the case short-circuited before reaching the dump site.",
        case_id,
        side,
        obj.display()
    );
    let outcome = match run_inputcheck(bin, obj) {
        Ok(o) => o,
        Err(e) => panic!("{} side {}: {}", case_id, side, e),
    };
    assert!(
        outcome.all_passed(),
        "{} side {}: mesh_booleans_inputcheck did NOT pass all 5 axioms.\n\
         Manifold:           {:?}\n\
         Watertight:         {:?}\n\
         Local Orientation:  {:?}\n\
         Global Orientation: {:?}\n\
         Intersection:       {:?}\n\
         --- raw validator output ---\n{}\n--- end raw ---",
        case_id,
        side,
        outcome.manifold,
        outcome.watertight,
        outcome.local_orientation,
        outcome.global_orientation,
        outcome.intersection,
        outcome.raw,
    );
}

/// Common driver for the inputcheck-parity tests (F0002, F0004,
/// R0014/R0017). Sets up the temp dir, runs the case under
/// `YANG_BOOLEAN=1` + `YANG_DUMP_OBJ_BASE`, then runs inputcheck on
/// both A.obj and B.obj. Asserts both sides report all 5 `passed`.
fn assert_case_inputcheck_passes(case_id: &str) {
    let bin = match cherchi_inputcheck_bin() {
        Some(p) => p,
        None => return,
    };
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15b-parity] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    // Per-case subdir so concurrent tests don't collide on the same
    // OBJ paths. Keep around on failure for post-mortem inspection
    // (matches the PR-S2 sweep's "leftover files are useful" guidance).
    let workdir = temp_root().join(case_id);
    let _ = std::fs::create_dir_all(&workdir);
    let base = workdir.join(case_id.to_lowercase());
    let base_str = base.to_string_lossy().into_owned();
    let path_a = workdir.join(format!("{}_a.obj", case_id.to_lowercase()));
    let path_b = workdir.join(format!("{}_b.obj", case_id.to_lowercase()));

    // Clear stale files so a partial run can't be mistaken for fresh data.
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);

    let (case_result, timed_out) = run_case_with_timeout(dir, case_id, Some(&base_str));
    if timed_out {
        panic!(
            "{}: WAFFLE_TIMEOUT after {}s — kernel hang on this case. \
             May be R0071-class (gear+revolve at scale 1.86e-4) or a \
             distinct hang. Spec §6.6 documents R0071 as out-of-scope; \
             other hangs require investigation.",
            case_id,
            WAFFLE_TIMEOUT.as_secs()
        );
    }
    let case = match case_result {
        Some(c) => c,
        None => panic!(
            "{} not found in corpus at `{}` — discover_cases returned no match",
            case_id, ASSAY_DIR
        ),
    };
    eprintln!(
        "[pr-y15b-parity] {} waffle_status={:?} detail={}",
        case_id, case.status, case.detail
    );

    assert_inputcheck_all_passed(&bin, &path_a, "A", case_id);
    assert_inputcheck_all_passed(&bin, &path_b, "B", case_id);
}

// ── Test 1: F0002 inputcheck parity ──────────────────────────────────────
//
// RED today: F0002 mask = (1,1,0,0,1) — M+W+I FAILED on BOTH sides per
// PR-S2 §4 + Phase 0 diagnostic. GREEN post-fix per spec §I8.
#[test]
#[ignore]
fn f0002_inputcheck_passes_postfix() {
    assert_case_inputcheck_passes("F0002");
}

// ── Test 2: F0004 inputcheck parity ──────────────────────────────────────
//
// F0004 ≡ F0002 byte-identical defect class per PR-Y14a §6 and Phase 0
// diagnostic (same 8-pair signature, identical site-3 cluster birth).
// Pinned separately so a regression on either case fires its own test.
#[test]
#[ignore]
fn f0004_inputcheck_passes_postfix() {
    assert_case_inputcheck_passes("F0004");
}

// ── Test 3: F0003 control — pass-boss-only status preserved ──────────────
//
// Per spec §1 + amendment 3 + §6.5: F0003 is `pass-boss-only`. Its
// pre-Cherchi meshes are non-watertight on both sides per PR-S2 §4, but
// the leak doesn't manifest as a Waffle failure because no boolean
// follows. Today's `waffle_status` is `Passed`. The PR-Y15b fix MUST
// NOT regress this — even if F0003's `non_watertight` mask does NOT
// migrate to `valid` (out of scope per §9), the Waffle pass status
// MUST remain `Passed` post-fix. This is the kill switch that catches
// accidental tessellation-side regressions on pass-boss-only paths.
//
// PASSES today. MUST STAY GREEN post-fix.
#[test]
#[ignore]
fn f0003_pass_boss_only_status_preserved() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15b-parity] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }
    // No OBJ dump needed — this test asserts on case_result.status only.
    let (case_result, timed_out) = run_case_with_timeout(dir, "F0003", None);
    if timed_out {
        panic!(
            "F0003: unexpected WAFFLE_TIMEOUT after {}s. F0003 is \
             pass-boss-only and historically completes in <5s; a hang \
             here indicates a serious regression in the pass-boss path.",
            WAFFLE_TIMEOUT.as_secs()
        );
    }
    let case = match case_result {
        Some(c) => c,
        None => panic!(
            "F0003 not found in corpus at `{}` — discover_cases returned no match",
            ASSAY_DIR
        ),
    };
    eprintln!(
        "[pr-y15b-parity] F0003 control waffle_status={:?} detail={}",
        case.status, case.detail
    );
    assert_eq!(
        case.status,
        AssayStatus::Passed,
        "F0003 control regression: waffle_status={:?} (expected Passed). \
         PR-Y15b MUST NOT break the pass-boss-only path. Per spec §6.5, \
         a `Passed → Errored` or `Passed → Failed` migration on F0003 \
         means PR-Y15b's fix is wrong and must revert. Detail: {}",
        case.status,
        case.detail,
    );
}

// ── Test 4: R0014 + R0017 second-tier reproducers ────────────────────────
//
// Per spec §1.46: R0014/R0017 are second-tier reproducers, both in
// PR-S2's `combined_failures` bucket. Iterates the two cases in a
// single test (saves harness setup cost) but reports per-case failures
// individually so a regression on either is clearly attributed.
//
// Both RED today; both GREEN post-fix. If one panics with
// WAFFLE_TIMEOUT, that's acceptable per spec §6.6 (R-cases may be
// R0071-class) — the panic message attributes the timeout to the
// specific case so the failure mode is clear. We collect per-case
// outcomes and panic at end with a combined report.
#[test]
#[ignore]
fn r0014_r0017_inputcheck_passes_postfix() {
    let bin = match cherchi_inputcheck_bin() {
        Some(p) => p,
        None => return,
    };
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr-y15b-parity] SKIP: assay corpus dir `{}` not present",
            dir.display()
        );
        return;
    }

    let cases = ["R0014", "R0017"];
    let mut failures: Vec<String> = Vec::new();

    for case_id in cases.iter() {
        // Per-case isolation: separate subdir per case, fresh env state.
        let workdir = temp_root().join(case_id);
        let _ = std::fs::create_dir_all(&workdir);
        let base = workdir.join(case_id.to_lowercase());
        let base_str = base.to_string_lossy().into_owned();
        let path_a = workdir.join(format!("{}_a.obj", case_id.to_lowercase()));
        let path_b = workdir.join(format!("{}_b.obj", case_id.to_lowercase()));
        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);

        let (case_result, timed_out) = run_case_with_timeout(dir, case_id, Some(&base_str));
        if timed_out {
            failures.push(format!(
                "{}: WAFFLE_TIMEOUT after {}s (may be R0071-class per spec §6.6)",
                case_id,
                WAFFLE_TIMEOUT.as_secs()
            ));
            continue;
        }
        let case = match case_result {
            Some(c) => c,
            None => {
                failures.push(format!("{}: not found in corpus", case_id));
                continue;
            }
        };
        eprintln!(
            "[pr-y15b-parity] {} waffle_status={:?} detail={}",
            case_id, case.status, case.detail
        );

        // Inline assertion-style check that captures failures into the
        // shared report instead of panicking immediately, so both R-cases
        // get reported in a single test run.
        for (side, obj) in [("A", &path_a), ("B", &path_b)] {
            if !obj.exists() {
                failures.push(format!(
                    "{} side {}: OBJ missing at `{}` (kernel did not dump)",
                    case_id,
                    side,
                    obj.display()
                ));
                continue;
            }
            match run_inputcheck(&bin, obj) {
                Err(e) => failures.push(format!("{} side {}: {}", case_id, side, e)),
                Ok(outcome) if outcome.all_passed() => {
                    eprintln!(
                        "[pr-y15b-parity] {} side {} all 5 axioms PASSED",
                        case_id, side
                    );
                }
                Ok(outcome) => failures.push(format!(
                    "{} side {}: NOT all axioms passed — \
                     M={:?} W={:?} LO={:?} GO={:?} I={:?}",
                    case_id,
                    side,
                    outcome.manifold,
                    outcome.watertight,
                    outcome.local_orientation,
                    outcome.global_orientation,
                    outcome.intersection,
                )),
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "R0014/R0017 second-tier reproducers: {} sub-failures:\n  - {}",
            failures.len(),
            failures.join("\n  - ")
        );
    }
}

// ── Pure-logic unit tests for the parser (no Cherchi binary needed) ──────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_passed_lowercase() {
        let text = "Manifold check: passed\n\
                    Watertight check: passed\n\
                    Local  Orientation check: passed\n\
                    Global Orientation check: passed\n\
                    Intersection check: passed\n";
        let o = parse_inputcheck(text);
        assert!(o.all_passed());
    }

    #[test]
    fn parse_f0002_uppercase_failed() {
        // F0002 reference signature per PR-S2 §4 + diagnostic memo:
        // M+W+I failed, LO+GO passed.
        let text = "Manifold check:                    FAILED\n\
                    Watertight check:                  FAILED\n\
                    Local  Orientation check:          passed\n\
                    Global Orientation check:          passed\n\
                    Intersection check:                FAILED\n";
        let o = parse_inputcheck(text);
        assert!(!o.all_passed());
        assert_eq!(o.manifold, Some(false));
        assert_eq!(o.watertight, Some(false));
        assert_eq!(o.local_orientation, Some(true));
        assert_eq!(o.global_orientation, Some(true));
        assert_eq!(o.intersection, Some(false));
    }

    #[test]
    fn parse_missing_lines_yields_none_fields() {
        let text = "Manifold check: passed\n\
                    Intersection check: passed\n";
        let o = parse_inputcheck(text);
        assert!(!o.all_passed());
        assert_eq!(o.manifold, Some(true));
        assert_eq!(o.watertight, None);
    }
}
