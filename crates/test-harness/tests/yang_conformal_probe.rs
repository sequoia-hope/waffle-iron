//! Conformal-probe harness tests for PR-Y14a.
//!
//! ## RED PHASE / PIN-PHASE STATUS
//!
//! RED PHASE: these tests are expected to fail until the implementer
//! wires up the oracle + probes per
//! `specs/yang_conformal_mesh_oracle.md`. The team-lead's framing splits
//! them into two flavors:
//!
//! 1. **Strict red-phase artifact** is the kernel oracle's own
//!    `#[cfg(test)] mod tests` block in
//!    `crates/kernel/src/boolean/oracles/conformal_mesh.rs`. Those
//!    tests panic today via `unimplemented!()` and turn green when
//!    the implementer fills in the oracle body.
//!
//! 2. **PIN tests** (this file) pin baseline behavior the implementer
//!    must NOT regress. Today they are tautologically green because no
//!    probe call sites exist (so probe-on equals probe-off). Their job
//!    is to FAIL the moment the implementer wires probes in a way that
//!    changes case behavior — that's the "probes are observation-only"
//!    contract enforced from the test harness side. Phase 3 (Adversary)
//!    will tighten these to pin the four `[conformal-probe]` stderr
//!    lines as concrete text assertions once empirical data exists.
//!
//! Both categories belong in the FIP §4 test phase: the kernel tests
//! prove the oracle implementation is real, the harness tests prove the
//! probe wiring is observation-only. A naive implementation that
//! changes case status (e.g. by panicking inside a probe) breaks these
//! PIN tests immediately.
//!
//! ## Test-by-test
//!
//! - `f0002_conformal_probe_pinned`, `f0004_conformal_probe_pinned`,
//!   `auto_union_failed_control_probe_on`: PIN that probe-on does not
//!   change `auto-union-failed` behavior on three different cases.
//!   Today: green (probes don't exist).
//! - `pass_genuine_control_probe_off_byte_identity`: PIN that probe-off
//!   does not change `pass-boss-only` behavior. Today: green (probe
//!   unset, no probes anyway).
//!
//! ## Pinned controls (chosen from `app/tests/cases/assay/results.json`)
//!
//! - `F0001`: status=pass, category=pass-boss-only, "9 oracles passed".
//!   Used as the pass-control. Spec asked for `pass-genuine`, but the
//!   results.json categories are `pass-boss-only`, `auto-union-failed`,
//!   etc. — no `pass-genuine` category exists, so per the team-lead's
//!   fallback rule we use `pass-boss-only`.
//! - `F0005`: status=fail, category=auto-union-failed. A different case
//!   from F0002/F0004 to calibrate "probe-on doesn't change behavior on
//!   the broader auto-union-failed cohort".
//!
//! ## How to run
//!
//! ```
//! cargo test -p test-harness --test yang_conformal_probe -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! The `--test-threads=1` flag is required because the tests
//! `set_var`/`remove_var` on `YANG_CONFORMAL_PROBE` in conflicting
//! ways. Without it, parallel test execution races the env reads in
//! `run_single_case`. This is consistent with how the existing
//! `assay_randomized.rs` Yang traces are invoked.
//!
//! Each test sets `YANG_BOOLEAN=1` and (where applicable)
//! `YANG_CONFORMAL_PROBE=1` before invoking `run_single_case`. The
//! probes emit to stderr; we either capture stderr externally
//! (--nocapture surfaces them to the user) or rely on the case status
//! as a behavior invariant. This file's tests are deliberately
//! conservative: they pin behavior + the silence invariant. Phase 3
//! tightens the assertions onto the actual probe-line text.

use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;
use test_harness::assay::scoring::AssayStatus;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

// PR-Y14b: pinned to post-fix expected state per
// `specs/yang_pr_y14b_coplanar_corner_dedup.md` §I4. The previous
// `auto-union-failed`-pinning version of this test is preserved in
// `crates/test-harness/tests/pr_y14b_coplanar_corner_dedup.rs` —
// see `f0002_distinct_failure_after_dedup_or_passes` for the new
// outcome assertion. This pinned test is now PR-Y14b's live regression
// guard for the Stage A `well_formed` invariant.

/// F0002 conformal-probe pinned to **post-fix** expected state.
///
/// Per `specs/yang_pr_y14b_coplanar_corner_dedup.md` §10.2 verification
/// item 2, this test is updated by PR-Y14b's test author to assert
/// Stage A's `well_formed=true` (or, weaker, the absence of the
/// `(v0=0, v1=0)` self-loop multi_paired entry).
///
/// **RED PHASE on current `main`** (HEAD `c60a366`, post-PR-Y14a):
/// Stage A reports `well_formed=false` with a (v0=0, v1=0) self-loop
/// per PR-Y14a findings §2.2 — this test fails. It turns green when
/// implementer-b lands the dedup at `coplanar_preprocess.rs:521`.
///
/// We cannot read the probe lines from stderr without stderr capture
/// (which lives in the new `pr_y14b_coplanar_corner_dedup.rs` test
/// file as a self-contained helper). To keep this file dep-free, we
/// pin the assertion at the `AssayResult.detail` level: post-fix,
/// F0002's detail must NOT contain the load-bearing pre-fix anchor
/// `half_edge[4].twin = 0 but twin.twin = 28` substring. This is the
/// same logic as `f0002_distinct_failure_after_dedup_or_passes` in
/// the PR-Y14b test file, but expressed against `detail` (which the
/// runner already returns). The new test file's
/// `f0002_canon0_cluster_size_pinned_postfix` is the byte-level
/// guarantee on the `well_formed` field; this test is the
/// detail-level reflection.
#[test]
#[ignore]
fn f0002_conformal_probe_pinned() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_CONFORMAL_PROBE", "1");

    let result = run_single_case(dir, "F0002", true);
    let r = result.expect("F0002 must exist in corpus");

    eprintln!("\n=== F0002 conformal-probe pinned (POST-FIX expectation) ===");
    eprintln!("Status: {:?}", r.status);
    eprintln!("Detail: {}", r.detail);

    // POST-FIX assertion (PR-Y14b §I4): F0002 must EITHER pass OR fail
    // with a different error than the pre-fix Stage-A anchor. The
    // detail string is the cheap surface for this check; the live
    // probe-line assertion lives in the PR-Y14b test file.
    let pre_fix_anchor = "half_edge[4].twin = 0 but twin.twin = 28";
    assert!(
        !r.detail.contains(pre_fix_anchor),
        "F0002 still fails with the pre-fix Stage-A coplanar-corner-cluster \
         defect: detail contains `{}`. Per PR-Y14b spec §I4 / §1, post-fix \
         must either pass or fail at a strictly later stage. Got detail: {}",
        pre_fix_anchor,
        r.detail
    );

    // Probe-on contract still holds: the env var stays set so probe
    // emit is observable to anyone running with --nocapture.
    assert_eq!(
        std::env::var("YANG_CONFORMAL_PROBE").as_deref(),
        Ok("1"),
        "YANG_CONFORMAL_PROBE must remain set across the test body"
    );
}

// PR-Y14b: pinned to post-fix expected state per
// `specs/yang_pr_y14b_coplanar_corner_dedup.md` §I4. F0004 is
// byte-identical to F0002 in PR-Y14a's measurement (findings §2.1
// four-tuple table); the dedup fix lands them together.

/// F0004 conformal-probe pinned to **post-fix** expected state.
///
/// Same contract and rationale as `f0002_conformal_probe_pinned` —
/// F0004 is the byte-identical sibling per PR-Y14a findings §2.1.
/// The pre-fix anchor substring may carry the same `[4]`/`28`
/// indices as F0002 (their twin-pair indices are deterministic and
/// shared across the two cases per the four-tuple table), so we use
/// the same `pre_fix_anchor` token for the absence assertion.
///
/// **RED PHASE on current `main`** (HEAD `c60a366`, post-PR-Y14a):
/// F0004's detail string contains the same pre-fix anchor as F0002's
/// because the two cases produce byte-identical output. This test
/// fails today; it turns green when implementer-b lands the dedup.
#[test]
#[ignore]
fn f0004_conformal_probe_pinned() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_CONFORMAL_PROBE", "1");

    let result = run_single_case(dir, "F0004", true);
    let r = result.expect("F0004 must exist in corpus");

    eprintln!("\n=== F0004 conformal-probe pinned (POST-FIX expectation) ===");
    eprintln!("Status: {:?}", r.status);
    eprintln!("Detail: {}", r.detail);

    let pre_fix_anchor = "half_edge[4].twin = 0 but twin.twin = 28";
    assert!(
        !r.detail.contains(pre_fix_anchor),
        "F0004 still fails with the pre-fix Stage-A coplanar-corner-cluster \
         defect: detail contains `{}`. Per PR-Y14b spec §I4 / §1, post-fix \
         must either pass or fail at a strictly later stage. Got detail: {}",
        pre_fix_anchor,
        r.detail
    );

    assert_eq!(
        std::env::var("YANG_CONFORMAL_PROBE").as_deref(),
        Ok("1"),
        "YANG_CONFORMAL_PROBE must remain set across the test body"
    );
}

/// Pass-control with probe OFF.
///
/// Picks a known pass-boss-only case from results.json (F0001) and
/// runs it WITHOUT the conformal probe. The behavioral contract is:
/// the case still passes (the probe being absent must not change
/// behavior either). The silence invariant is checked by
/// the "no `[conformal-probe]` text appears in stderr" — which
/// `run_single_case` does not directly hand back, but the operator
/// can verify with --nocapture.
///
/// Spec note: `pass-genuine` was the requested category but does
/// not exist in results.json. Fell back to `pass-boss-only` per
/// team-lead instruction.
#[test]
#[ignore]
fn pass_genuine_control_probe_off_byte_identity() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    // Explicitly REMOVE the probe env var (in case a prior test set it
    // and tests run in the same process).
    std::env::remove_var("YANG_CONFORMAL_PROBE");

    let result = run_single_case(dir, "F0001", true);
    let r = result.expect("F0001 must exist in corpus");

    eprintln!("\n=== F0001 pass-control (probe off) ===");
    eprintln!("Status: {:?}", r.status);
    eprintln!("Detail: {}", r.detail);

    // Behavior invariant: F0001 passes today; probe-off must keep it
    // passing. (This is the "byte-identity" half of the contract: with
    // probe off, behavior is identical to current main.)
    assert_eq!(
        r.status,
        AssayStatus::Passed,
        "F0001 must still pass under YANG_BOOLEAN=1 with probe OFF. Got status={:?} detail={}",
        r.status,
        r.detail
    );

    // Silence invariant: the env var stays unset throughout. (Direct
    // stderr capture is left for Phase 3; this assertion is the
    // mechanical pin.)
    assert!(
        std::env::var("YANG_CONFORMAL_PROBE").is_err(),
        "YANG_CONFORMAL_PROBE must remain unset during the probe-off test"
    );
}

/// Auto-union-failed control with probe ON.
///
/// Picks F0005 (auto-union-failed, NOT F0002/F0004) and runs with the
/// probe on. Behavioral contract: same as F0002/F0004 — probe-on must
/// not change the outcome.
#[test]
#[ignore]
fn auto_union_failed_control_probe_on() {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!("Assay corpus not generated yet — skipping");
        return;
    }
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("YANG_CONFORMAL_PROBE", "1");

    let result = run_single_case(dir, "F0005", true);
    let r = result.expect("F0005 must exist in corpus");

    eprintln!("\n=== F0005 auto-union-failed control (probe on) ===");
    eprintln!("Status: {:?}", r.status);
    eprintln!("Detail: {}", r.detail);

    assert_eq!(
        r.status,
        AssayStatus::Failed,
        "F0005 still expected to fail under YANG_BOOLEAN=1 with probe on (probe is observation-only). Got status={:?} detail={}",
        r.status,
        r.detail
    );
    assert!(
        r.detail.to_lowercase().contains("auto-union-failed"),
        "F0005 still expected to fail in the auto-union pipeline. detail={}",
        r.detail
    );
    assert_eq!(
        std::env::var("YANG_CONFORMAL_PROBE").as_deref(),
        Ok("1"),
        "YANG_CONFORMAL_PROBE must remain set across the test body"
    );
}
