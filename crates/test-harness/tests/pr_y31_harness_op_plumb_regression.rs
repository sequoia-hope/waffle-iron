//! PR-Y31 — RED-phase regression tests for the Cherchi-differential-diff
//! harness's op-plumb fix.
//!
//! ## Defect class (canary 988efa4, spec 0f28e85)
//!
//! `crates/test-harness/tests/cherchi_differential_diff.rs:286` hardcodes
//! `cmd.arg("union")` when invoking Cherchi 2022's `mesh_booleans` binary,
//! regardless of what each `.waffle` model prescribes for the boolean being
//! compared. F0044's first boolean op is `Subtract` (extrude 2 has
//! `"cut": true`); the harness was therefore comparing Waffle's Subtract
//! Stage-B output against Cherchi's Union output — a category error per
//! Cherchi 2022 §3 lines 232–236: *"a Boolean operator, namely union,
//! intersection, subtraction. ... The output is a mesh B that contains the
//! result of applying the Boolean operator to the input meshes."*
//!
//! ## Empirical mechanism (canary §4)
//!
//! Canary directly re-invoked Cherchi with `subtraction` on F0044's dumped
//! A.obj / B.obj and obtained 136 triangles / 72 vertices, byte-identical
//! to Waffle Stage B at the 1µm quantization grid (`extras=0, missing=0,
//! common=136`). The 48 extras reported under hardcoded `union` are exactly
//! the 48 B-Inside-flipped triangles that Subtract's selector keeps and
//! Union's selector discards (Yang 2025 §4.4.2 per-op selection table:
//! Subtract = A-Outside ∪ B-Inside-flipped vs Union = A-Outside ∪ B-Outside).
//!
//! ## Pre-fix baseline (at 988efa4 / 0f28e85, hardcoded `union`)
//!
//! Per PR-Y30 Stage B baselines + canary §4:
//!
//! | Case | extras | missing | common |
//! |------|--------|---------|--------|
//! | F0044 | 48 | 0 | 88 |
//! | F0020 | 107 (variable) | 93 (variable) | 185 (variable) |
//! | F0045 | 466 | 236 | 0 |
//! | R0092 | 368 | 340 | 0 |
//!
//! ## Post-fix expectation (per spec §6 oracles)
//!
//! - F0044: `extras == 0 AND missing == 0 AND common == 136` (canary §4 verified)
//! - F0020: `extras <= 107` (unchanged; F0020 is all-Union, harness already correct)
//! - F0045 / R0092: Cherchi invocation succeeds (no ERROR / panic / timeout)
//!
//! ## Test strategy — subprocess-spawn the existing harness
//!
//! `cherchi_differential_diff.rs`'s helpers are all `fn` (test-local), not
//! `pub`, so this test file cannot call them directly. The cleanest path
//! that produces a clean RED→GREEN transition without modifying the test
//! file post-fix is to spawn the harness as a subprocess and parse its
//! stderr diff blocks:
//!
//! ```text
//!   === F0044 diff ===
//!   ...
//!     In Cherchi, not in Waffle: N triangles    ← MISSING
//!     In Waffle, not in Cherchi: N triangles    ← EXTRAS
//!     Common (matching quantized positions): N  ← COMMON
//!   === end F0044 diff ===
//! ```
//!
//! Pre-fix: spawned harness uses hardcoded `union` → F0044 emits extras=48.
//! Post-fix: impl-y31 plumbs the op → F0044 emits extras=0.
//!
//! ## Cherchi binary requirement
//!
//! All three tests require `CHERCHI2022_BIN` to point at a built
//! `mesh_booleans` binary (default
//! `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`).
//! If absent, the spawned harness emits `[diff-harness F0044] SKIP:
//! CHERCHI2022_BIN unset/missing` and exits; these tests detect that line
//! and bail with `eprintln + return` rather than asserting on missing data.
//! Mirrors the skip pattern in `cherchi2022_reference_parity.rs`.
//!
//! ## How to run
//!
//! ```text
//! YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
//!     --test pr_y31_harness_op_plumb_regression -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` because we spawn cargo subprocesses that themselves
//! manipulate process-global env vars (`YANG_BOOLEAN`, `YANG_STAGE_DUMP`,
//! `YANG_DUMP_OBJ_BASE`). Parallel test execution would race those.
//!
//! Refs:
//! - Spec: `specs/yang_pr_y31_harness_op_plumb.md` §6 (oracles), §10 (test recommendations)
//! - Canary: `docs/audits/pr_y31_anchor_canary.md` §4 (empirical verification)
//! - Yang 2025 §4.4.2 (mesh booleans + per-op selection); Cherchi 2022 §3 (op-parameterized output)

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use test_harness::cherchi_sidecar::{cherchi_bin, run_with_timeout, TimedRun};

/// Subprocess timeout for the spawned harness. The existing harness runs
/// `run_diff_for_case` which performs (Waffle pipeline + Cherchi invocation
/// + OBJ parse + set diff) per case; ~30s for F0044 alone, longer for the
/// cohort test that bundles three cases.
const HARNESS_SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(300);

/// Per-case counts extracted from the harness's stderr diff block.
#[derive(Debug, Clone, Copy)]
struct DiffCounts {
    /// `In Cherchi, not in Waffle: N` — triangles Cherchi emits we don't.
    missing: usize,
    /// `In Waffle, not in Cherchi: N` — triangles we emit Cherchi doesn't.
    extras: usize,
    /// `Common (matching quantized positions): N`.
    common: usize,
}

/// Outcome of spawning the existing harness for a single test name.
enum HarnessRun {
    /// Stderr captured + parsed into per-case diff blocks. Map keyed by
    /// case_id (e.g., "F0044"). Cases that appeared in stderr but lacked a
    /// complete diff block (SKIP / Cherchi invocation failed / Stage B
    /// absent) are absent from the map.
    Counts(std::collections::HashMap<String, DiffCounts>),
    /// `CHERCHI2022_BIN` not set or binary missing — the harness logged
    /// `[diff-harness <case>] SKIP` and returned early. Test should bail.
    Skipped,
}

/// Spawn `cargo test -p test-harness --test cherchi_differential_diff --
/// <harness_test_name> --ignored --nocapture --test-threads=1` and parse the
/// stderr diff blocks.
///
/// We inherit `CHERCHI2022_BIN` from the calling test's env (it must be set
/// by the user or by `cherchi_bin()`'s default-path discovery). The
/// `--test-threads=1` flag in the spawned cargo invocation is required
/// because the existing harness mutates process-global env vars.
fn spawn_harness(harness_test_name: &str) -> HarnessRun {
    let mut cmd = Command::new("cargo");
    cmd.arg("test")
        .arg("-p")
        .arg("test-harness")
        .arg("--test")
        .arg("cherchi_differential_diff")
        .arg("--")
        .arg(harness_test_name)
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1");
    // Inherit YANG_BOOLEAN/TWIN_DEBUG/CHERCHI2022_BIN/CARGO_*/etc. from our env.
    // Pin CWD to the workspace root so `cargo test -p test-harness` resolves.
    if let Some(workspace_root) = workspace_root() {
        cmd.current_dir(workspace_root);
    }

    match run_with_timeout(cmd, HARNESS_SUBPROCESS_TIMEOUT) {
        TimedRun::Completed(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            // The existing harness's `eprintln!` writes to the SPAWNED
            // process's stderr; cargo test captures it and re-emits via
            // its own stdout/stderr depending on flags. With --nocapture,
            // the harness's eprintln output appears on the spawned process's
            // stderr AND/OR stdout (cargo merges by default). Concatenate
            // both streams before parsing to be robust to either routing.
            let combined = format!("{}\n{}", stderr, stdout);
            eprintln!(
                "[pr-y31-test] harness exit status={:?}; combined output {} bytes",
                out.status,
                combined.len()
            );
            if combined.contains("SKIP: CHERCHI2022_BIN unset/missing") {
                return HarnessRun::Skipped;
            }
            HarnessRun::Counts(parse_diff_blocks(&combined))
        }
        TimedRun::TimedOut => panic!(
            "[pr-y31-test] spawned harness timed out after {}s on `{}`",
            HARNESS_SUBPROCESS_TIMEOUT.as_secs(),
            harness_test_name
        ),
        TimedRun::SpawnFailed(e) => panic!(
            "[pr-y31-test] failed to spawn `cargo test ...`: {}",
            e
        ),
    }
}

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR` (which
/// for an integration test points at `crates/test-harness/`) looking for the
/// top-level `Cargo.toml`. Returns `None` if walk-up fails — caller falls
/// back to relative cwd.
fn workspace_root() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // We're at crates/test-harness/ — walk up two levels to workspace root.
    for _ in 0..4 {
        if dir.join("Cargo.toml").exists() && dir.join("crates").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
    None
}

/// Parse all `=== <case> diff ===` ... `=== end <case> diff ===` blocks from
/// captured stderr/stdout. Returns map from case_id to `DiffCounts`.
fn parse_diff_blocks(text: &str) -> std::collections::HashMap<String, DiffCounts> {
    let mut out = std::collections::HashMap::new();
    let mut current_case: Option<String> = None;
    let mut missing: Option<usize> = None;
    let mut extras: Option<usize> = None;
    let mut common: Option<usize> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("=== ") {
            if let Some(case_part) = rest.strip_suffix(" diff ===") {
                if let Some(case_id) = case_part.strip_prefix("end ") {
                    // Block close: emit if all three counts captured.
                    if let (Some(case), Some(m), Some(e), Some(c)) =
                        (current_case.take(), missing.take(), extras.take(), common.take())
                    {
                        if case == case_id {
                            out.insert(
                                case,
                                DiffCounts {
                                    missing: m,
                                    extras: e,
                                    common: c,
                                },
                            );
                        }
                    } else {
                        // Reset partial state regardless.
                        current_case = None;
                        missing = None;
                        extras = None;
                        common = None;
                    }
                } else {
                    // Block open.
                    current_case = Some(case_part.to_string());
                    missing = None;
                    extras = None;
                    common = None;
                }
                continue;
            }
        }
        if current_case.is_some() {
            // Each is the FIRST integer on the line.
            if let Some(n) = parse_count_after(line, "In Cherchi, not in Waffle:") {
                missing = Some(n);
            } else if let Some(n) = parse_count_after(line, "In Waffle, not in Cherchi:") {
                extras = Some(n);
            } else if let Some(n) = parse_count_after(line, "Common (matching quantized positions):") {
                common = Some(n);
            }
        }
    }

    out
}

/// Extract the first whitespace-delimited integer that follows `prefix` in
/// `line`. Returns None if `prefix` is not present or no integer follows.
fn parse_count_after(line: &str, prefix: &str) -> Option<usize> {
    let rest = line.split_once(prefix)?.1;
    rest.split_whitespace()
        .next()
        .and_then(|t| t.parse::<usize>().ok())
}

/// Skip-on-missing-Cherchi guard: emit a clear skip log + return true if
/// `CHERCHI2022_BIN` is unset or binary missing. Mirrors the skip pattern
/// in `cherchi2022_reference_parity.rs`.
fn cherchi_unavailable() -> bool {
    match cherchi_bin() {
        Some(p) => {
            eprintln!("[pr-y31-test] Cherchi binary discovered at {}", p.display());
            false
        }
        None => {
            eprintln!(
                "[pr-y31-test] skipping (CHERCHI2022_BIN not set or binary missing). \
                 To run: set CHERCHI2022_BIN to a built `mesh_booleans` binary."
            );
            true
        }
    }
}

/// PR-Y31 spec §6 O1 (load-bearing): assert F0044's Stage B diff reports
/// `extras=0 AND missing=0 AND common=136` post-fix.
///
/// Pre-fix baseline at commit `988efa4` / `0f28e85`: `extras=48, missing=0,
/// common=88` (harness hardcodes Cherchi `union` while F0044's first
/// boolean is `Subtract`; canary §4 empirically verified that
/// `subtraction` re-invocation matches Waffle Stage B byte-identically
/// at the 1µm quantization grid).
///
/// Post-fix (impl-y31): the harness plumbs the actual op from each
/// `.waffle` model into `cherchi_differential_diff.rs::invoke_cherchi`,
/// so F0044 invokes Cherchi with `subtraction` and the diff collapses to
/// 0/0/136.
#[test]
#[ignore]
fn pr_y31_f0044_extras_zero() {
    if cherchi_unavailable() {
        return;
    }
    let run = spawn_harness("cohort_cherchi_diff_baseline");
    let counts = match run {
        HarnessRun::Skipped => {
            eprintln!("[pr-y31-test] spawned harness reported SKIP — bailing");
            return;
        }
        HarnessRun::Counts(c) => c,
    };

    eprintln!(
        "[pr-y31-test] parsed diff blocks for cases: {:?}",
        counts.keys().collect::<Vec<_>>()
    );

    let f0044 = counts.get("F0044").unwrap_or_else(|| {
        panic!(
            "[pr-y31-test] F0044 diff block missing from harness stderr. \
             Either the harness failed before emitting F0044's `=== F0044 diff ===` \
             block (Cherchi invocation failed / Stage B absent / Waffle pipeline \
             panic), or the spawned cargo invocation did not run \
             `cohort_cherchi_diff_baseline`. Parsed cases: {:?}",
            counts.keys().collect::<Vec<_>>()
        )
    });

    eprintln!(
        "[pr-y31-test] F0044: extras={} missing={} common={}",
        f0044.extras, f0044.missing, f0044.common
    );

    // Load-bearing assertion. Pre-fix RED: extras=48. Post-fix GREEN: extras=0.
    assert_eq!(
        f0044.extras, 0,
        "[pr-y31-test] PR-Y31 spec §5 I3 + §6 O1 violation (Cherchi 2022 §3 \
         lines 232–236: \"a Boolean operator, namely union, intersection, \
         subtraction. ... The output is a mesh B that contains the result of \
         applying the Boolean operator to the input meshes\"). Expected F0044 \
         Stage B `extras=0` post-PR-Y31, got extras={}. Pre-PR-Y31 baseline \
         (canary memo 988efa4 §4 + PR-Y30 Stage B baselines): F0044 reports \
         `extras=48` because the harness at `cherchi_differential_diff.rs:286` \
         hardcodes `cmd.arg(\"union\")` while F0044's first boolean op is \
         `Subtract` (extrude 2 has `\"cut\": true` in `F0044.waffle`). Canary \
         §4 empirically verified that re-invoking Cherchi with `subtraction` on \
         the same dumped A.obj/B.obj yields 136 triangles matching Waffle \
         Stage B byte-identically at the 1µm grid. Fix per spec §4 Branch \
         Table: plumb `MeshBooleanOp` from the `.waffle` JSON's first boolean \
         (cut=true → Subtract, cut=false → Union) through \
         `invoke_cherchi_union` (rename → `invoke_cherchi`) into the \
         `cmd.arg(op_to_cli_str(op))` invocation. If extras stays 48: impl \
         did not land (spec §7 failure mode 1: re-canary). If extras is some \
         third value: the plumbing perturbed an unrelated code path (spec §7 \
         failure mode 2). LOC budget: 15–35 in test-harness only; zero \
         production code (spec §11 + §12 acceptance gate item 9).",
        f0044.extras
    );
    assert_eq!(
        f0044.missing, 0,
        "[pr-y31-test] PR-Y31 spec §6 O1 missing-count violation: expected \
         F0044 Stage B `missing=0` post-PR-Y31, got missing={}. Pre-PR-Y31 \
         baseline: missing=0 (we already emit every triangle Cherchi-Union \
         emits). Post-PR-Y31: missing must stay 0 because the only thing \
         the harness change does is align Cherchi's op with Waffle's, which \
         can only ADD triangles to Cherchi's output (subtraction keeps more \
         than union when B ⊂ A), never remove them.",
        f0044.missing
    );
    assert_eq!(
        f0044.common, 136,
        "[pr-y31-test] PR-Y31 spec §6 O1 common-count violation: expected \
         F0044 Stage B `common=136` post-PR-Y31, got common={}. Canary §4 \
         direct measurement: Cherchi `subtraction` output = 136 triangles, \
         Waffle Stage B = 136 triangles, all 136 quantize-equal at the 1µm \
         grid. The 136 = 88 A-Outside + 48 B-Inside-flipped triangles per \
         Yang §4.4.2 selection table.",
        f0044.common
    );
}

/// PR-Y31 spec §6 O2 (cohort guard): assert F0020's Stage B diff extras
/// count does not regress above the PR-Y30 baseline (107). F0020's `.waffle`
/// ops are all `cut=false` = Union; the harness already invokes Cherchi
/// correctly for it, so PR-Y31's plumbing change must resolve to the SAME
/// `cmd.arg("union")` invocation and produce IDENTICAL diff counts.
///
/// The `<= 107` form (not `== 107`) absorbs Cherchi non-determinism on
/// F0020 (PR-Y30 banked finding: F0020 Cherchi-union triangle count varies
/// 246–295 across runs even at `TBB_NUM_THREADS=1`). Test passes pre-fix
/// AND post-fix; this is a regression guard against an accidental
/// perturbation of the Union code path.
#[test]
#[ignore]
fn pr_y31_f0020_no_regression() {
    if cherchi_unavailable() {
        return;
    }
    let run = spawn_harness("f0020_cherchi_diff_baseline");
    let counts = match run {
        HarnessRun::Skipped => {
            eprintln!("[pr-y31-test] spawned harness reported SKIP — bailing");
            return;
        }
        HarnessRun::Counts(c) => c,
    };

    eprintln!(
        "[pr-y31-test] F0020 cohort-guard parsed diff blocks: {:?}",
        counts.keys().collect::<Vec<_>>()
    );

    let f0020 = counts.get("F0020").unwrap_or_else(|| {
        panic!(
            "[pr-y31-test] F0020 diff block missing from harness stderr. \
             Parsed cases: {:?}",
            counts.keys().collect::<Vec<_>>()
        )
    });

    eprintln!(
        "[pr-y31-test] F0020: extras={} missing={} common={}",
        f0020.extras, f0020.missing, f0020.common
    );

    assert!(
        f0020.extras <= 107,
        "[pr-y31-test] PR-Y31 spec §5 I4 + §6 O2 cohort-guard violation: \
         F0020 Stage B `extras={}` exceeds the PR-Y30 baseline (107). F0020 \
         is all-Union (`.waffle` extrudes all `cut=false`); the harness \
         already invokes Cherchi with `union` correctly for F0020. PR-Y31's \
         plumbing change MUST resolve to the same `cmd.arg(\"union\")` \
         invocation for F0020 and produce IDENTICAL behavior. A value > 107 \
         indicates the plumbing perturbed the Union code path — re-audit the \
         `.waffle` JSON-read logic for the first-boolean-op extraction (spec \
         §3 + §7 failure mode 2). The `<= 107` form (not `== 107`) absorbs \
         Cherchi non-determinism on F0020 (PR-Y30 banked finding: \
         Cherchi-union triangle count varies 246–295 across runs even at \
         `TBB_NUM_THREADS=1`).",
        f0020.extras
    );
}

/// PR-Y31 spec §6 O3 (cohort guard): assert F0045 and R0092 produce diff
/// blocks (Cherchi invocation succeeds, harness reaches the diff-emission
/// path). The brief specifies "Cherchi invocation returns `Some(_)` (i.e.,
/// doesn't ERROR)"; structurally, the spawned harness emits an `=== <case>
/// diff ===` block only when both Waffle Stage B landed AND Cherchi
/// produced output. Absence of the block means one of those failed.
///
/// Pre-fix and post-fix: both cases must produce a diff block. PR-Y31 must
/// not break Cherchi's ability to run on these inputs under either op (F0045
/// is all-Union, R0092 is mixed Union+Subtract).
#[test]
#[ignore]
fn pr_y31_f0045_r0092_no_error() {
    if cherchi_unavailable() {
        return;
    }
    let run = spawn_harness("cohort_cherchi_diff_baseline");
    let counts = match run {
        HarnessRun::Skipped => {
            eprintln!("[pr-y31-test] spawned harness reported SKIP — bailing");
            return;
        }
        HarnessRun::Counts(c) => c,
    };

    eprintln!(
        "[pr-y31-test] F0045/R0092 cohort-guard parsed diff blocks: {:?}",
        counts.keys().collect::<Vec<_>>()
    );

    assert!(
        counts.contains_key("F0045"),
        "[pr-y31-test] PR-Y31 spec §6 O3 violation: F0045 diff block absent \
         from harness stderr (Cherchi invocation failed / Waffle Stage B not \
         emitted / pipeline panic). Pre-fix and post-fix: F0045's all-Union \
         `.waffle` ops must produce a diff block (extras may stay 466 — that \
         is a Yang §4.1.1 tessellation-grid divergence banked for PR-Y32+, \
         spec §9 anti-scope). Parsed cases: {:?}",
        counts.keys().collect::<Vec<_>>()
    );
    assert!(
        counts.contains_key("R0092"),
        "[pr-y31-test] PR-Y31 spec §6 O3 violation: R0092 diff block absent \
         from harness stderr. R0092 has a mixed `.waffle` op set (one \
         `cut=false` Union + one `cut=true` Subtract, canary §6 banked: \
         dumped-pair op identity unverified). Post-fix the harness will \
         invoke Cherchi with whichever op the dumped pair corresponds to; \
         BOTH operators must run successfully on these inputs. A diff-block \
         absence indicates Cherchi errored / panicked / timed out on the \
         post-fix op — spec §7 failure mode 3. Parsed cases: {:?}",
        counts.keys().collect::<Vec<_>>()
    );

    let f0045 = counts.get("F0045").unwrap();
    let r0092 = counts.get("R0092").unwrap();
    eprintln!(
        "[pr-y31-test] F0045: extras={} missing={} common={}",
        f0045.extras, f0045.missing, f0045.common
    );
    eprintln!(
        "[pr-y31-test] R0092: extras={} missing={} common={}",
        r0092.extras, r0092.missing, r0092.common
    );
}
