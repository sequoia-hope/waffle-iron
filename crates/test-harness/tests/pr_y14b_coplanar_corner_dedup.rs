//! PR-Y14b — Red-phase tests for the coplanar corner-vertex deduplication fix.
//!
//! ## RED PHASE STATUS
//!
//! These tests are **expected to fail on current `main`** (HEAD `c60a366`,
//! post-PR-Y14a). They become green when implementer-b lands the per-call
//! canonical-key dedup at `crates/kernel/src/boolean/coplanar_preprocess.rs:521`
//! per `specs/yang_pr_y14b_coplanar_corner_dedup.md`.
//!
//! ## What the spec promises
//!
//! Per `specs/yang_pr_y14b_coplanar_corner_dedup.md`:
//!
//! - **§1 outcome:** F0002 and F0004 either (a) pass under `YANG_BOOLEAN=1`,
//!   OR (b) fail at a strictly later pipeline stage with the conformal probe
//!   reporting `well_formed=true` at the previously-broken Stage A.
//! - **§4 invariants:**
//!   - **I4** — Conformal-probe Stage A `well_formed=true` for F0002 and
//!     F0004, OR (weaker) Stage A's `multi_paired` count has NO entry with
//!     `(v0=0, v1=0)` self-loop.
//!   - **I5** — No new `unpaired_directed_edges` introduced at Stage A.
//!   - **I7** — Determinism preserved: two consecutive runs produce
//!     byte-identical Stage A probe output.
//! - **§5 telemetry oracle:** new counter
//!   `COPLANAR_VERTS_DEDUPED_BY_CANON_KEY` must be non-zero on F0002, and
//!   the existing `[coplanar-tele]` line must surface this new field.
//!
//! ## How the tests measure these
//!
//! - The probes emit `[conformal-probe] stage=...` lines to **stderr** when
//!   `YANG_CONFORMAL_PROBE=1` is set (per
//!   `specs/yang_conformal_mesh_oracle.md` §"Probe log format"). The
//!   counters are `pub(crate)` and not visible from the `test-harness`
//!   crate, so we capture stderr at the file-descriptor level (libc dup2)
//!   to read both probe and `[coplanar-tele]` lines.
//! - For the high-level fix-correctness assertion, the
//!   `AssayResult.detail` field surfaces the `auto-union-failed` warning
//!   text — including the load-bearing `half_edge[N].twin = ... but
//!   twin.twin = ...` substring — so we don't need to capture stderr for
//!   that test.
//!
//! ## How to run
//!
//! ```
//! cargo test -p test-harness --test pr_y14b_coplanar_corner_dedup -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the tests perform process-global
//! stderr FD redirection and `set_var`/`remove_var` on env vars; parallel
//! execution would race those operations. This mirrors the existing
//! `yang_conformal_probe.rs` invocation pattern.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;
use test_harness::assay::scoring::{AssayResult, AssayStatus};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Run a closure with the process's stderr file descriptor redirected to
/// a temporary file; return both the closure's return value and the
/// captured stderr bytes as a `String` (lossy UTF-8).
///
/// This uses `libc::dup`/`libc::dup2` to swap FD 2. It is process-global,
/// so this helper is only safe when tests run with `--test-threads=1`.
/// Mirrors the standard pattern used by `gag::BufferRedirect::stderr` —
/// reproduced here inline rather than adding a third-party dep.
fn capture_stderr<F, R>(f: F) -> (R, String)
where
    F: FnOnce() -> R,
{
    let mut tmp = tempfile::tempfile().expect("tempfile for stderr capture");
    // Make sure all in-process buffered stderr is flushed before redirect.
    let _ = std::io::stderr().flush();
    let original_stderr_fd = unsafe { libc::dup(libc::STDERR_FILENO) };
    assert!(original_stderr_fd >= 0, "dup(STDERR_FILENO) failed");
    let tmp_fd = tmp.as_raw_fd();
    let dup2_rc = unsafe { libc::dup2(tmp_fd, libc::STDERR_FILENO) };
    assert!(dup2_rc >= 0, "dup2 to STDERR_FILENO failed");

    let result = f();

    // Flush again before restoring so the captured file contains everything.
    let _ = std::io::stderr().flush();
    let restore_rc = unsafe { libc::dup2(original_stderr_fd, libc::STDERR_FILENO) };
    assert!(restore_rc >= 0, "dup2 restore failed");
    unsafe { libc::close(original_stderr_fd) };

    tmp.seek(SeekFrom::Start(0)).expect("seek captured tmpfile");
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read captured tmpfile");
    (result, String::from_utf8_lossy(&buf).into_owned())
}

/// Run F0002/F0004/etc. with the conformal probe on, return both the
/// `AssayResult` and the captured stderr bytes.
fn run_case_with_probe(case_id: &str) -> (AssayResult, String) {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_CONFORMAL_PROBE", "1");

    let case_owned = case_id.to_string();
    let dir_owned = dir.to_path_buf();
    let (result, stderr) =
        capture_stderr(move || run_single_case(&dir_owned, &case_owned, true));
    let r = result.unwrap_or_else(|| panic!("{} must exist in corpus", case_id));
    (r, stderr)
}

/// Extract the first `[conformal-probe] stage=A ...` summary line from a
/// captured stderr blob. Returns `None` if no such line was emitted.
fn first_stage_a_summary(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .find(|l| l.starts_with("[conformal-probe] stage=A "))
}

/// Extract all detail lines for Stage A multi_paired entries from a
/// captured stderr blob. The probe wiring at
/// `topology_extract.rs::emit_conformal_probe` emits these as
/// `[conformal-probe]   multi_paired #N: v0=... v1=... fwd=[...] rev=[...]`
/// indented by two spaces (note the double-space after `[conformal-probe]`).
///
/// We scan from the Stage A summary line up to the next
/// `[conformal-probe] stage=` line (exclusive) so we only return Stage A
/// detail lines.
fn stage_a_multi_paired_lines(stderr: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_stage_a = false;
    for line in stderr.lines() {
        if line.starts_with("[conformal-probe] stage=A ") {
            in_stage_a = true;
            continue;
        }
        if line.starts_with("[conformal-probe] stage=") {
            // Some other stage's summary line — Stage A region ended.
            in_stage_a = false;
            continue;
        }
        if in_stage_a && line.starts_with("[conformal-probe]   multi_paired ") {
            out.push(line);
        }
    }
    out
}

/// Parse `well_formed={true|false}` out of a Stage-A summary line.
/// Returns `Some(true|false)` on a parseable line, `None` if the field
/// is missing.
fn parse_well_formed(summary: &str) -> Option<bool> {
    for tok in summary.split_whitespace() {
        if let Some(rest) = tok.strip_prefix("well_formed=") {
            return match rest {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
        }
    }
    None
}

/// Parse `unpaired={N}` out of a Stage-A summary line.
fn parse_unpaired(summary: &str) -> Option<usize> {
    for tok in summary.split_whitespace() {
        if let Some(rest) = tok.strip_prefix("unpaired=") {
            return rest.parse().ok();
        }
    }
    None
}

/// Test if the captured stderr contains a Stage A `(v0=0, v1=0)`
/// multi_paired self-loop entry — the dominant defect signature pinned
/// in PR-Y14a's findings memo §2.2.
fn stage_a_has_zero_zero_self_loop(stderr: &str) -> bool {
    stage_a_multi_paired_lines(stderr)
        .iter()
        .any(|l| l.contains(" v0=0 v1=0 "))
}

// ─────────────────────────────────────────────────────────────────────────
// I4 — F0002 Stage A canon-0 cluster shrinks: well_formed=true OR no (0,0)
// ─────────────────────────────────────────────────────────────────────────

/// Spec invariant **I4** for F0002. Per §4 / §6.3 of the spec, post-fix
/// Stage A must report either `well_formed=true` OR (weaker) the
/// `(v0=0, v1=0)` self-loop multi_paired entry must be absent.
///
/// **Pre-fix observation** (PR-Y14a findings §2.2): Stage A reports
/// `well_formed=false` with `multi_paired=50` and `multi_paired #0:
/// v0=0 v1=0`. Both conditions of the disjunction fail today → this test
/// fails on current `main`. It turns green when the dedup at
/// `coplanar_preprocess.rs:521` shrinks the canon-0 cluster from 8 raw
/// vertices to 1.
#[test]
#[ignore]
fn f0002_canon0_cluster_size_pinned_postfix() {
    let (_r, stderr) = run_case_with_probe("F0002");
    let summary = first_stage_a_summary(&stderr)
        .expect("Stage A conformal-probe summary line must be emitted");
    eprintln!("F0002 Stage A summary: {summary}");

    let well_formed = parse_well_formed(summary)
        .expect("well_formed=... field must be parseable in Stage A summary");
    let has_self_loop = stage_a_has_zero_zero_self_loop(&stderr);

    // Post-fix: well_formed=true (strong) OR (v0=0, v1=0) self-loop is
    // absent (weak). Spec §4 I4 accepts either.
    assert!(
        well_formed || !has_self_loop,
        "F0002 Stage A defect persists: well_formed={} and (v0=0, v1=0) self-loop present={}. \
         Per spec §I4, post-fix must satisfy `well_formed || !has_self_loop`. \
         Stage A summary: {}",
        well_formed,
        has_self_loop,
        summary
    );
}

/// Spec invariant **I4** for F0004. F0004 is byte-identical to F0002 in
/// PR-Y14a's measurement (findings §2.1 four-tuple table); the dedup fix
/// must land them together.
#[test]
#[ignore]
fn f0004_canon0_cluster_size_pinned_postfix() {
    let (_r, stderr) = run_case_with_probe("F0004");
    let summary = first_stage_a_summary(&stderr)
        .expect("Stage A conformal-probe summary line must be emitted");
    eprintln!("F0004 Stage A summary: {summary}");

    let well_formed = parse_well_formed(summary)
        .expect("well_formed=... field must be parseable in Stage A summary");
    let has_self_loop = stage_a_has_zero_zero_self_loop(&stderr);

    assert!(
        well_formed || !has_self_loop,
        "F0004 Stage A defect persists: well_formed={} and (v0=0, v1=0) self-loop present={}. \
         Per spec §I4, post-fix must satisfy `well_formed || !has_self_loop`. \
         Stage A summary: {}",
        well_formed,
        has_self_loop,
        summary
    );
}

// ─────────────────────────────────────────────────────────────────────────
// I5 — No new unpaired_directed_edges introduced at Stage A on F0002
// ─────────────────────────────────────────────────────────────────────────

/// Spec invariant **I5**: the dedup MUST NOT create boundary holes.
/// PR-Y14a's findings table (§2.1) pins F0002's pre-fix Stage A
/// `unpaired=0`. Post-fix must remain `unpaired=0` — a rise from 0 to >0
/// would mean the dedup removed a split that the topology depended on
/// and is the P9 "fix it right or don't fix it" guard.
///
/// Note: this assertion is `unpaired==0`, not `<= 0`. The pre-fix
/// baseline is 0; any post-fix rise is the regression we're guarding
/// against. The test is RED today because while it passes on `unpaired`,
/// the rest of the post-fix invariant chain (I4 above) does not — so the
/// implementer cannot land a partial fix that satisfies I4 by sacrificing
/// I5. Today this test will pass on its own (current Stage A unpaired=0),
/// but it is included here so that the implementer's fix attempt is
/// guarded across **both** invariants from day one. If a future
/// implementer attempt collapses verts but leaves a hole, this test
/// flips to RED and stops the merge.
#[test]
#[ignore]
fn f0002_no_new_unpaired_at_stage_a() {
    let (_r, stderr) = run_case_with_probe("F0002");
    let summary = first_stage_a_summary(&stderr)
        .expect("Stage A conformal-probe summary line must be emitted");
    eprintln!("F0002 Stage A summary: {summary}");

    let unpaired = parse_unpaired(summary)
        .expect("unpaired=... field must be parseable in Stage A summary");

    assert_eq!(
        unpaired, 0,
        "F0002 Stage A unpaired count must remain 0 (pre-fix baseline). \
         Per spec §I5, the dedup must not create boundary holes. \
         A rise from 0 → {} is a regression. Stage A summary: {}",
        unpaired, summary
    );
}

// ─────────────────────────────────────────────────────────────────────────
// I7 — Determinism: two F0002 runs produce byte-identical Stage A probe
// ─────────────────────────────────────────────────────────────────────────

/// Spec invariant **I7**: the per-call canonical-key `BTreeMap` (NOT
/// `HashMap`) preserves determinism across runs. Two consecutive
/// invocations of F0002 with `YANG_CONFORMAL_PROBE=1` must produce
/// byte-identical Stage A summary lines AND byte-identical Stage A
/// multi_paired detail lines.
///
/// Today this test is RED because, while the current pipeline IS
/// deterministic per PR13, the test will only become semantically
/// load-bearing post-fix (the new `BTreeMap` insertion order is the
/// determinism risk). We pin the byte-identity invariant from day one
/// so that an implementer who reaches for `HashMap` is caught by the
/// same harness that caught PR13's similar issues. The assertion shape
/// itself runs today and passes on a deterministic pipeline; it is
/// "red" only in the spec-compliance sense (the rest of the spec's
/// invariants do not yet hold). Listed here for completeness and to
/// guard the implementer's `BTreeMap` choice.
#[test]
#[ignore]
fn f0002_determinism_two_runs_byte_identical() {
    let (_r1, stderr1) = run_case_with_probe("F0002");
    let (_r2, stderr2) = run_case_with_probe("F0002");

    let s1 = first_stage_a_summary(&stderr1)
        .expect("Run 1: Stage A summary line must be emitted")
        .to_string();
    let s2 = first_stage_a_summary(&stderr2)
        .expect("Run 2: Stage A summary line must be emitted")
        .to_string();

    eprintln!("F0002 run 1 Stage A summary: {s1}");
    eprintln!("F0002 run 2 Stage A summary: {s2}");

    assert_eq!(
        s1, s2,
        "F0002 Stage A summary line must be byte-identical across two \
         runs (spec §I7 determinism). Implementer-b must use `BTreeMap`, \
         not `HashMap`, for the per-call canonical-key map."
    );

    // Also pin determinism of the multi_paired detail lines — the actual
    // concrete observation that would flap if the dedup map were a
    // `HashMap` (insertion order changes which canonical-key wins as
    // the "first vertex inserted").
    let m1 = stage_a_multi_paired_lines(&stderr1).join("\n");
    let m2 = stage_a_multi_paired_lines(&stderr2).join("\n");

    assert_eq!(
        m1, m2,
        "F0002 Stage A multi_paired detail lines must be byte-identical \
         across two runs (spec §I7 determinism)."
    );
}

// ─────────────────────────────────────────────────────────────────────────
// §5 telemetry oracle — new COPLANAR_VERTS_DEDUPED_BY_CANON_KEY counter
// ─────────────────────────────────────────────────────────────────────────

/// Spec §5 telemetry oracle: the new
/// `COPLANAR_VERTS_DEDUPED_BY_CANON_KEY` atomic counter must be non-zero
/// on F0002. The counter is `pub(crate)` (matching the existing
/// `COPLANAR_VERTS_VIA_SPLIT_EDGE` pattern at
/// `coplanar_preprocess.rs:31`), so it is not directly visible from the
/// `test-harness` crate. Per spec §5, the new field must be surfaced in
/// the existing `[coplanar-tele]` line (emitted at end of
/// `split_brep_for_coplanar_pairs` ~line 395).
///
/// We assert by scanning captured stderr for a `[coplanar-tele]` line
/// containing a `verts_deduped_by_canon_key=N` token with `N > 0`. This
/// is RED today: the field does not exist, so no token matches and the
/// assertion fails.
///
/// Implementer-b MAY name the field exactly `verts_deduped_by_canon_key`
/// to satisfy this test, OR pick a different but obvious name (e.g.
/// `verts_deduped`, `dedup_canon_hits`). We accept any token of the form
/// `<name>=N` that contains the substring `dedup` AND has `N >= 1`. This
/// gives the implementer minor naming freedom while still being a strict
/// "the counter is wired and fires" assertion.
#[test]
#[ignore]
fn coplanar_dedup_counter_nonzero_for_f0002() {
    let (_r, stderr) = run_case_with_probe("F0002");

    // Find the [coplanar-tele] line emitted at end of
    // split_brep_for_coplanar_pairs.
    let tele_line = stderr
        .lines()
        .find(|l| l.starts_with("[coplanar-tele] "))
        .unwrap_or_else(|| {
            panic!(
                "F0002 must emit a `[coplanar-tele] ...` line (16 split-edge \
                 verts inserted on this case per PR-Y14a findings §2.2). \
                 Captured stderr did not contain one. Stderr tail:\n{}",
                stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
            )
        });

    eprintln!("F0002 [coplanar-tele] line: {tele_line}");

    // Look for a `<name>=<N>` token where <name> contains "dedup" and N
    // is a positive integer. This admits naming variants the
    // implementer might choose.
    let mut dedup_value: Option<u64> = None;
    let mut dedup_field: Option<&str> = None;
    for tok in tele_line.split_whitespace() {
        if let Some((name, val)) = tok.split_once('=') {
            if name.to_lowercase().contains("dedup") {
                if let Ok(n) = val.parse::<u64>() {
                    dedup_value = Some(n);
                    dedup_field = Some(name);
                    break;
                }
            }
        }
    }

    let n = dedup_value.unwrap_or_else(|| {
        panic!(
            "F0002 [coplanar-tele] line must contain a `<name>=<N>` token \
             where <name> contains `dedup` (per spec §5 telemetry oracle: \
             COPLANAR_VERTS_DEDUPED_BY_CANON_KEY counter must be surfaced \
             in [coplanar-tele]). Got line: {}",
            tele_line
        )
    });
    assert!(
        n > 0,
        "F0002 [coplanar-tele] {}={} must be > 0 — F0002's 8-way canon-0 \
         cluster pre-fix means at least 7 dedup hits per pair (PR-Y14a \
         findings §2.2 / §3.2). Got {}={}.",
        dedup_field.unwrap(),
        n,
        dedup_field.unwrap(),
        n
    );
}

// ─────────────────────────────────────────────────────────────────────────
// §1 outcome — F0002 Passes OR fails with a different (non-twin-4-28) error
// ─────────────────────────────────────────────────────────────────────────

/// Spec §1 outcome (a) OR (b): F0002 either Passes OR fails strictly
/// later in the pipeline with a NEW failure mode whose detail string
/// does NOT contain the load-bearing pre-fix anchor
/// `half_edge[4].twin = 0 but twin.twin = 28`. (The exact twin-pair
/// indices `[4]` / `28` are byte-pinned in PR-Y14a's findings memo and
/// run output — they are deterministic. Post-fix, EITHER the validation
/// passes entirely OR a different defect surfaces with different indices
/// or a different oracle name.)
///
/// This is RED today because F0002 currently fails with exactly that
/// error string in `AssayResult.detail` (verified by running
/// `f0002_conformal_probe_pinned` with `--nocapture`).
#[test]
#[ignore]
fn f0002_distinct_failure_after_dedup_or_passes() {
    let (r, _stderr) = run_case_with_probe("F0002");
    eprintln!("F0002 status: {:?}, detail: {}", r.status, r.detail);

    // Outcome (a): passes. Outcome (b): fails but with a different
    // error than the pre-fix anchor.
    let pre_fix_anchor = "half_edge[4].twin = 0 but twin.twin = 28";
    let still_failing_at_pre_fix_anchor = matches!(r.status, AssayStatus::Failed)
        && r.detail.contains(pre_fix_anchor);

    assert!(
        !still_failing_at_pre_fix_anchor,
        "F0002 still fails with the exact pre-fix Stage-A defect: \
         status={:?}, detail contains `{}`. Per spec §1, post-fix must \
         either pass OR fail at a strictly later stage with a different \
         failure mode. Got detail: {}",
        r.status, pre_fix_anchor, r.detail
    );
}
