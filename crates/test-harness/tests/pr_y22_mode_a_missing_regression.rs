//! PR-Y22-MODE-A-MISSING — RED-phase regression tests for the NONCONF
//! (same-patch retroactive pairing) + DEGEN (canon-induced quantization
//! filter) sub-mechanisms in `topology_extract.rs::flood_fill_patches`.
//!
//! ## RED PHASE STATUS
//!
//! Expected to fail on current `main` (post-PR-Y20-MODE-A). Becomes GREEN
//! when implementer-z lands the M1+M2 fixes per `specs/yang_pr_y22_mode_a_missing.md`:
//!  - **M1 (NONCONF Path A):** `crates/kernel/src/boolean/topology_extract.rs:857-881`
//!    — at the Step 6 boundary collection `is_boundary` predicate (L862-866),
//!    when `directed_edge_to_tris.get(&(v1, v0))` returns neighbors with at
//!    least one neighbor in the SAME patch (`tri_to_patch[nt] == pi`), emit
//!    BOTH `(v0, v1)` and `(v1, v0)` as boundary HEs in the patch so they
//!    pair naturally at Step 7 L1232 `[the_one]` arm. Per Yang §4.4.2
//!    directional-symmetry mandate.
//!  - **M2 (DEGEN canon-induced filter):** `crates/kernel/src/boolean/topology_extract.rs:468-474`
//!    — in the Step 2 `all_tris` builder, after `cv = [canon_v(raw[0]),
//!    canon_v(raw[1]), canon_v(raw[2])]`, skip pushing the `FlatSubTri` if
//!    `cv[0] == cv[1] || cv[1] == cv[2] || cv[0] == cv[2]`. Mirrors
//!    `exact_mesh.rs:1771-1775` welded_tris filter. Cherchi 2022 §4
//!    "well-formed simplicial complex" preserved through canon_v.
//!
//! ## Spec contract under test (spec §5 + §6)
//!
//! Spec §5 invariants:
//!  - **I1 (DEGEN filter, M2):** post-M2, `all_tris` contains no duplicate-vertex
//!    tris.
//!  - **I2 (NONCONF retroactive emit, M1 Path A):** when Step 6 detects
//!    `rev_in_same_patch`, BOTH `(v0,v1)` and `(v1,v0)` appear in patch's
//!    boundary set.
//!  - **I3 (F0020 Mode A residual GREEN):** F0020 Extrude 3 [twin-oracle]
//!    `unpaired_count == 0` post-PR (was 8 on main).
//!
//! Spec §6 gating:
//!  - F0020 Extrude 3 [twin-oracle] `unpaired_count` drops 8 → 0.
//!  - F0044 b#5 [twin-oracle] `unpaired_count` drops by 2 (the DEGEN entries
//!    `(31,169)` + `(197,200)` filtered at M2; canary §2 + §5 edge case #3).
//!  - F0030 stays clean (no Mode A; canary §2).
//!
//! Spec §6 informational (NOT gating):
//!  - F0020 spotlight `Status:Failed` MAY persist (downstream tessellation
//!    NMM-render layer banked from PR-Y21 ABORT). Per `feedback_yang_only.md`
//!    no-movement at status level → next-layer outcome.
//!
//! ## Pre-fix empirical baseline (canary §1 + §2)
//!
//! Per canary memo `docs/audits/pr_y22_mode_a_missing_canary.md` §1, current
//! main emits these per-boolean summaries:
//!
//! ```text
//! [topo-extract] summary: paired=66, unpaired=8, ambiguous=0   ← F0020 b#2 (Extrude 3)
//! [topo-extract] summary: paired=101, unpaired=2, ambiguous=0  ← F0044 batch b#5 (R0092 b#1)
//! ```
//!
//! The 8 MISSING on F0020 Extrude 3 breaks down as 7 NONCONF (chain
//! `(71,69)…(67,66)`) + 1 DEGEN (`(96,26)` ti=89 pi=11). The 2 MISSING on
//! the F0044 batch b#5 are both DEGEN (`(31,169)` + `(197,200)`).
//!
//! ### Counter terminology note (spec/canary discrepancy resolved here)
//!
//! Spec §1 + §6 use the term `[twin-oracle] unpaired_count` for the gating
//! metric, but the cited values (8 / 2) come from `[topo-extract] summary:
//! unpaired=N` (the pre-pairing miss counter at L1346, which counts pair-
//! search misses where `is_nmm == false`). The post-pairing `[twin-oracle]
//! unpaired_count=N` line at L1458 counts arena HEs with twin issues +
//! `twin=None AND reverse-in-arena` — an EMPIRICALLY DIFFERENT counter that
//! already reports 0 for these cases on main (DEGEN's reverse is not in
//! arena because canon_v collapsed it; NONCONF's reverse is not in arena
//! because Step 6 dropped both directions).
//!
//! These tests assert **both** counters, with the load-bearing gate on
//! `[topo-extract] summary: unpaired=N` (the counter where the canary
//! observed the 8/2 baselines). The `[twin-oracle]` counter is asserted as
//! a regression guard against PR-Y22 making it WORSE.
//!
//! ## Assertion-2 baseline amendment (test-amender-n, PR-Y22-RECOVERY R-b, 2026-05-08)
//!
//! Per adversary-22 v1 §6 (Gate 2) the F0020 `[twin-oracle] unpaired_count`
//! pre-PR baseline on plain HEAD is **2**, not 0 (canary-runner-9 §2 + plain-
//! HEAD reproduction). The 2 residual edges are a SEPARATE downstream layer
//! (in-process B-Rep build path / validator-panic chain) BANKED for PR-Y23+.
//! PR-Y22 targets the PRE-pairing `[topo-extract] summary unpaired=N` counter
//! (assertion 1), NOT this POST-pairing counter.
//!
//! Assertion 2 is therefore amended to `<= baseline` form (regression guard
//! against PR-Y22 making the post-pairing state worse, while permitting the
//! documented baseline):
//!  - F0020: `assert!(twin_unpaired <= 2, ...)`
//!  - F0044 batch: `assert!(twin_unpaired <= 0, ...)` (canary §2 baseline 0)
//!
//! Per `feedback_yang_only.md`, `<= baseline` is a regression-guard idiom
//! (catches upward regression), NOT a fallback path that produces right
//! answers for wrong reasons.
//!
//! Per canary §2 cohort table (using the spec's "F0044 b#k" numbering = the
//! k-th boolean across the F0044+F0045+R0092 spotlight batch):
//! - F0020 b#1 (Extrude 2): 0 MISSING
//! - F0020 b#2 (Extrude 3): 8 MISSING (7 NONCONF + 1 DEGEN)  ← I3 gating target
//! - F0044 batch b#1-4,6,7: 0 MISSING each
//! - F0044 batch b#5 (R0092's first boolean): 2 MISSING (0 NONCONF + 2 DEGEN)
//!   ← spec §6 secondary gating target
//!
//! ## How to run
//!
//! ```text
//! YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
//!     --test pr_y22_mode_a_missing_regression -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the tests perform process-global
//! stderr FD redirection and `set_var` on env vars; parallel execution would
//! race those operations. Mirrors `pr_y19_mode_b_regression.rs` +
//! `pr_y20_mode_a_regression.rs`.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Process-global FD swap: redirect stderr → tempfile, run `f`, restore, return
/// `(f's return value, captured stderr as lossy UTF-8)`. Safe only with
/// `--test-threads=1`. Mirrors `pr_y19_mode_b_regression.rs` +
/// `pr_y20_mode_a_regression.rs`.
fn capture_stderr<F, R>(f: F) -> (R, String)
where
    F: FnOnce() -> R,
{
    let mut tmp = tempfile::tempfile().expect("tempfile for stderr capture");
    let _ = std::io::stderr().flush();
    let original_stderr_fd = unsafe { libc::dup(libc::STDERR_FILENO) };
    assert!(original_stderr_fd >= 0, "dup(STDERR_FILENO) failed");
    let tmp_fd = tmp.as_raw_fd();
    let dup2_rc = unsafe { libc::dup2(tmp_fd, libc::STDERR_FILENO) };
    assert!(dup2_rc >= 0, "dup2 to STDERR_FILENO failed");

    let result = f();

    let _ = std::io::stderr().flush();
    let restore_rc = unsafe { libc::dup2(original_stderr_fd, libc::STDERR_FILENO) };
    assert!(restore_rc >= 0, "dup2 restore failed");
    unsafe { libc::close(original_stderr_fd) };

    tmp.seek(SeekFrom::Start(0)).expect("seek captured tmpfile");
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read captured tmpfile");
    (result, String::from_utf8_lossy(&buf).into_owned())
}

/// Find the MAX value of `[twin-oracle] <key>=N` across ALL emissions in stderr.
/// Returns `None` if no such line exists.
fn max_twin_oracle_field(stderr: &str, key: &str) -> Option<usize> {
    let needle = format!("[twin-oracle] {}=", key);
    let mut max_val: Option<usize> = None;
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix(&needle) {
            if let Some(val) = rest.split_whitespace().next().and_then(|t| t.parse().ok()) {
                max_val = Some(max_val.map_or(val, |m: usize| m.max(val)));
            }
        }
    }
    max_val
}

/// Count how many distinct `[twin-oracle] <key>=N` lines were emitted.
fn count_twin_oracle_lines(stderr: &str, key: &str) -> usize {
    let needle = format!("[twin-oracle] {}=", key);
    stderr.lines().filter(|l| l.starts_with(&needle)).count()
}

/// Find the MAX value of the `unpaired=N` field across ALL
/// `[topo-extract] summary: paired=P, unpaired=N, ambiguous=A` lines in
/// stderr. This is the canary §1's load-bearing counter: the pair-search
/// miss counter incremented at `topology_extract.rs:1309` ONLY when
/// `is_nmm == false` (so legitimate NMM is excluded; this counter
/// surfaces NONCONF + DEGEN MISSING residual specifically).
///
/// Returns `None` if no such line exists.
fn max_topo_extract_unpaired(stderr: &str) -> Option<usize> {
    let prefix = "[topo-extract] summary:";
    let mut max_val: Option<usize> = None;
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix(prefix) {
            // Parse `unpaired=N` from the comma-separated field list.
            for field in rest.split(',') {
                let f = field.trim();
                if let Some(val_str) = f.strip_prefix("unpaired=") {
                    if let Ok(val) = val_str.split_whitespace().next().unwrap_or("").parse() {
                        max_val = Some(max_val.map_or(val, |m: usize| m.max(val)));
                    }
                }
            }
        }
    }
    max_val
}

/// Count how many distinct `[topo-extract] summary:` lines were emitted.
fn count_topo_extract_summary_lines(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|l| l.starts_with("[topo-extract] summary:"))
        .count()
}

/// PR-Y22-MODE-A-MISSING spec §5 I3 + §6 primary gating: assert F0020's
/// MAX `[topo-extract] summary: unpaired=N` across all flood_fill_patches
/// invocations drops from 8 → 0 post-PR.
///
/// F0020 has 3 sequential extrudes; flood_fill_patches fires once per
/// non-degenerate boolean. Per canary §1: b#1 (Extrude 2) has unpaired=0,
/// b#2 (Extrude 3) has unpaired=8 — so MAX across F0020 = 8 on current main,
/// expected 0 post-PR-Y22 (M1 resolves the 7 NONCONF cases by emitting both
/// directions and pairing them; M2 filters the DEGEN tri so its (96,26)
/// MISSING falls into the PR-Y20-MODE-A NMM branch — `twin=None` without
/// incrementing `unpaired_count`; canary §5 edge case #4).
///
/// **Both M1 and M2 are needed** for full F0020 Extrude 3 GREEN per canary §5
/// edge case #4: M2 alone drops 8 → 7 (only the DEGEN goes away); M1 alone
/// drops 8 → 1 (DEGEN remains as `(96,26)` since its degenerate reverse tri
/// still ghosts the directed_edge_to_tris); M1+M2 drops 8 → 0.
///
/// Counter selection: this test uses `[topo-extract] summary: unpaired=N`
/// (the L1346 pre-pairing miss counter) as the load-bearing gate because
/// that is the counter where the canary observed the 8 baseline. The
/// post-pairing `[twin-oracle] unpaired_count` (L1458) already reports
/// non-zero only when `twin=None AND reverse-in-arena`; for the F0020
/// MISSING residual the reverse is NOT in arena (Step 6 dropped both
/// directions), so `[twin-oracle]` reports a smaller number. The
/// `[twin-oracle]` counter is asserted as a regression guard (must stay 0).
#[test]
#[ignore]
fn pr_y22_f0020_mode_a_missing_zero() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 routes through the Yang pipeline (the only path that
    // exercises Stage 4 flood_fill_patches Step 6/7). TWIN_DEBUG=1 enables
    // the `[twin-oracle]` lines this test parses.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    let dir_owned = dir.to_path_buf();
    let (result, stderr) = capture_stderr(move || run_single_case(&dir_owned, "F0020", true));
    let r = result.expect("F0020 must exist in corpus — regenerate via assay_gen");

    let n_topo_summaries = count_topo_extract_summary_lines(&stderr);
    let n_twin_oracle = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_topo_unpaired = max_topo_extract_unpaired(&stderr);
    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y22-test] F0020 [topo-extract] summary lines: {} ; \
         [twin-oracle] unpaired_count lines: {} \
         (canary §1: F0020 has 3 sequential extrudes; b#1 + b#2 are \
         non-degenerate booleans that fire the oracle; b#1 unpaired=0, \
         b#2 unpaired=8)",
        n_topo_summaries, n_twin_oracle
    );
    eprintln!(
        "[pr-y22-test] F0020 max `[topo-extract] summary: unpaired=N`: {:?} \
         (pre-PR-Y22 baseline: 8; post-PR-Y22 expected: 0; LOAD-BEARING GATE)",
        max_topo_unpaired
    );
    eprintln!(
        "[pr-y22-test] F0020 max `[twin-oracle] unpaired_count`: {:?} \
         (regression guard: must stay 0)",
        max_twin_unpaired
    );
    eprintln!(
        "[pr-y22-test] F0020 case status={:?} detail={}",
        r.status, r.detail
    );

    // ── Assertion 1: max [topo-extract] summary unpaired across ALL invocations == 0
    //   (spec §5 I3 + §6 primary gating; load-bearing per canary §1 baseline)
    //
    // Per spec §6 informational (NOT gating): F0020 spotlight Status:Failed MAY
    // persist (downstream tessellation NMM-render layer banked from PR-Y21
    // ABORT). This test asserts ONLY the layer-targeted MISSING residual drop,
    // NOT case Status:Passed. Per `feedback_yang_only.md` no-movement at
    // status level → next-layer outcome (NOT a regression of this PR).
    let topo_unpaired = max_topo_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y22-test] no `[topo-extract] summary: ... unpaired=N ...` line \
             in F0020 stderr. TWIN_DEBUG=1 gate failed or pipeline aborted before \
             flood_fill_patches. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        topo_unpaired, 0,
        "[pr-y22-test] PR-Y22-MODE-A-MISSING spec §5 I3 + §6 primary gating \
         violation (Yang §4.4.2 directional-symmetry + Cherchi 2022 §4 well-formed \
         simplicial complex preserved through canon_v): expected MAX \
         `[topo-extract] summary: unpaired=0` across all {} flood_fill_patches \
         invocations on F0020 post-PR, got max unpaired={}. \
         Pre-PR baseline (canary §1): F0020 Extrude 3 has unpaired=8 = 7 NONCONF \
         (chain `(71,69)→(69,70)→(70,73)→(73,72)→(72,68)→(68,66)→(66,67)`, all \
         with rev_in_de2t=true and rev_in_same_patch=true) + 1 DEGEN \
         (canon edge `(96,26)` from ti=89 in pi=11; rev tri has duplicate canon \
         vertex `(96,96)` produced by canon_v nanometer quantization). \
         Both M1 and M2 are needed (canary §5 edge case #4): \
         - M1 alone: 8 → 1 (DEGEN persists as ghost reverse) \
         - M2 alone: 8 → 7 (NONCONF chain unaffected) \
         - M1+M2 together: 8 → 0 ← THIS PR's gating target \
         Fix per spec §3 (M2) + §4 Path A (M1): \
         - M2: `crates/kernel/src/boolean/topology_extract.rs:468-474` — in \
           the Step 2 `all_tris` builder, after computing canonical vertex \
           triplet `cv`, skip `all_tris.push(...)` if `cv[0]==cv[1] || \
           cv[1]==cv[2] || cv[0]==cv[2]`. Mirrors `exact_mesh.rs:1771-1775` \
           welded_tris filter. Cherchi 2022 §4 invariant preserved at the \
           Waffle consumer layer. \
         - M1 Path A: `crates/kernel/src/boolean/topology_extract.rs:857-881` \
           Step 6 `is_boundary` predicate at L862-866 — when reverse exists \
           in `directed_edge_to_tris` AND any reverse neighbor is in the \
           SAME patch (`tri_to_patch[nt] == pi`), emit BOTH directions as \
           boundary HEs. They naturally pair at Step 7 L1232 `[the_one]` arm. \
           Per Yang §4.4.2 directional-symmetry: a manifold edge (incidence 2) \
           MUST have both directions present and paired. \
         If `unpaired==1`: M1 Path A is missing or the same-patch rev detection \
         missed the chain — re-check L862's `tri_to_patch[nt] == pi` test. \
         If `unpaired==7`: M2 fired but M1 did not — re-check L468-474 cv \
         duplicate-skip filter (must run BEFORE Step 4 manifold-incidence \
         counting at L504+, per canary §5 edge case #2). \
         If `unpaired==8`: neither fix landed (or wrong anchors per \
         `feedback_anchor_before_fix.md`).",
        n_topo_summaries, topo_unpaired
    );

    // ── Assertion 2: max [twin-oracle] unpaired_count <= 2 (regression guard)
    //
    // EMPIRICAL BASELINE CORRECTION (test-amender-n, PR-Y22-RECOVERY R-b):
    // Per adversary-22 v1 §6 (Gate 2) the pre-PR baseline on plain HEAD is
    // `[twin-oracle] unpaired_count=2` for F0020 (NOT 0 as previously
    // asserted). Plain-HEAD reproduction:
    //
    //   [topo-extract] summary: paired=48, unpaired=0, ambiguous=0
    //   [twin-oracle] unpaired_count=0
    //   [topo-extract] summary: paired=66, unpaired=8, ambiguous=0
    //   [twin-oracle] unpaired_count=2     ← THIS IS THE PRE-PR BASELINE
    //
    // The 2 residual `[twin-oracle]` edges are a SEPARATE downstream layer
    // (in-process B-Rep build path surfaces `twin=None + reverse-in-arena`
    // for 2 edges of the F0020 Mode A chain whose downstream symptom is
    // the validator panic). Resolving those is BANKED for PR-Y23+ and is
    // NOT part of PR-Y22's targeted layer. PR-Y22's load-bearing target is
    // the `[topo-extract] summary unpaired=N` PRE-pairing counter
    // (assertion 1 above), not this POST-pairing counter.
    //
    // The `<= 2` form is a regression-guard, not a fallback per
    // `feedback_yang_only.md`: it catches PR-Y22 making the post-pairing
    // counter WORSE (e.g., M1 Path A emitting reverse into arena but
    // failing to pair, which would push the value above 2), while
    // permitting the documented pre-PR baseline.
    let twin_unpaired = max_twin_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y22-test] no `[twin-oracle] unpaired_count=` line in F0020 \
             stderr. TWIN_DEBUG=1 gate failed. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert!(
        twin_unpaired <= 2,
        "[pr-y22-test] PR-Y22-MODE-A-MISSING regression guard: expected MAX \
         `[twin-oracle] unpaired_count<=2` across all {} flood_fill_patches \
         invocations on F0020 post-PR, got max twin_unpaired={}. \
         Pre-PR baseline (adversary-22 v1 §6, Gate 2): F0020 plain HEAD reports \
         `[twin-oracle] unpaired_count=2` on the second flood_fill_patches \
         invocation (Extrude 3). The 2 residual edges are a SEPARATE downstream \
         layer (in-process B-Rep build path / validator-panic chain) BANKED for \
         PR-Y23+; PR-Y22's targeted layer is the PRE-pairing counter \
         `[topo-extract] summary unpaired=N` (assertion 1), NOT this POST-pairing \
         counter. A value `> 2` indicates PR-Y22 made the post-pairing state \
         WORSE — e.g., M1 Path A emitted the reverse HE into the arena but \
         pairing failed at Step 7 L1232 [the_one] (reverse-in-arena=true AND \
         twin=None on a NEW edge that wasn't in the pre-PR baseline). Audit \
         the directed_he keying contract between Step 6 emit and Step 7 search \
         per spec §7 NO-FALLBACK contract.",
        n_twin_oracle,
        twin_unpaired
    );
}

/// PR-Y22-MODE-A-MISSING spec §6 secondary gating: assert the F0044 batch
/// (F0044+F0045+R0092 — same composition as `spotlight_f0044`) MAX
/// `[topo-extract] summary: unpaired=N` across all flood_fill_patches
/// invocations drops from 2 → 0 post-PR.
///
/// Per canary §2 cohort table: the batch has 7 booleans; b#1-4,6,7 all
/// have `unpaired=0`; b#5 has `unpaired=2` (both DEGEN, no NONCONF).
/// So MAX across the F0044 batch = 2 on current main, expected 0 post-PR
/// (M2 filter alone resolves both DEGEN entries).
///
/// **Only M2 is needed for this case** (canary §2: 0 NONCONF on F0044
/// batch). M1 has no effect here. The two DEGEN entries are:
/// - `he_fwd=45 canon=(31,169) rev_tris=[133] rev_patches=[41]`
/// - `he_fwd=75 canon=(197,200) rev_tris=[27] rev_patches=[31]`
///
/// **Batch composition rationale:** canary §2's "F0044 b#k" numbering
/// refers to the k-th flood_fill_patches invocation in the
/// `spotlight_f0044` batch (F0044+F0045+R0092). Within that numbering:
/// b#1 = F0044's only boolean, b#2-4 = F0045's 3 booleans, b#5-7 =
/// R0092's 3 booleans. The DEGEN-bearing b#5 is therefore R0092's first
/// boolean — unreachable by running F0044 alone (which only fires 1
/// invocation). This test mirrors the canary's batch composition so the
/// b#5 baseline is reproducible.
#[test]
#[ignore]
fn pr_y22_f0044_b5_mode_a_missing_drops_by_2() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 + TWIN_DEBUG=1 — mirror pr_y22_f0020_mode_a_missing_zero.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    // Mirror spotlight_f0044 batch composition: F0044 + F0045 + R0092.
    // Per canary §2 the b#5 DEGEN entries surface in the 5th flood_fill_patches
    // invocation across the batch (R0092's first boolean).
    let cases = ["F0044", "F0045", "R0092"];
    let dir_owned = dir.to_path_buf();
    let (results, stderr) = capture_stderr(move || {
        cases
            .iter()
            .map(|id| {
                run_single_case(&dir_owned, id, true)
                    .unwrap_or_else(|| panic!("{id} not found in corpus"))
            })
            .collect::<Vec<_>>()
    });

    let n_topo_summaries = count_topo_extract_summary_lines(&stderr);
    let n_twin_oracle = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_topo_unpaired = max_topo_extract_unpaired(&stderr);
    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y22-test] F0044 batch [topo-extract] summary lines: {} ; \
         [twin-oracle] unpaired_count lines: {} \
         (canary §2 expects 7 = F0044 b#1 + F0045 b#2-4 + R0092 b#5-7)",
        n_topo_summaries, n_twin_oracle
    );
    eprintln!(
        "[pr-y22-test] F0044 batch max `[topo-extract] summary: unpaired=N`: {:?} \
         (pre-PR-Y22 baseline: 2; post-PR-Y22 expected: 0; LOAD-BEARING GATE)",
        max_topo_unpaired
    );
    eprintln!(
        "[pr-y22-test] F0044 batch max `[twin-oracle] unpaired_count`: {:?} \
         (regression guard: must stay 0)",
        max_twin_unpaired
    );
    for (case_id, r) in cases.iter().zip(results.iter()) {
        eprintln!(
            "[pr-y22-test] {} status={:?} detail={}",
            case_id, r.status, r.detail
        );
    }

    // ── Assertion 1: max [topo-extract] summary unpaired across ALL invocations == 0
    //   (spec §6 secondary gating; load-bearing per canary §2 baseline)
    let topo_unpaired = max_topo_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y22-test] no `[topo-extract] summary: ... unpaired=N ...` line \
             in F0044 batch stderr. TWIN_DEBUG=1 gate failed or pipeline aborted \
             before flood_fill_patches. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        topo_unpaired, 0,
        "[pr-y22-test] PR-Y22-MODE-A-MISSING spec §6 secondary gating violation \
         (Cherchi 2022 §4 well-formed simplicial complex preserved through canon_v): \
         expected MAX `[topo-extract] summary: unpaired=0` across all {} \
         flood_fill_patches invocations on F0044+F0045+R0092 batch post-PR, got \
         max unpaired={}. \
         Pre-PR baseline (canary §2): batch b#5 (R0092's first boolean) has \
         unpaired=2 = 2 DEGEN entries: \
         - `he_fwd=45 canon=(31,169) class=DEGEN rev_tris=[133] rev_patches=[41]` \
         - `he_fwd=75 canon=(197,200) class=DEGEN rev_tris=[27] rev_patches=[31]` \
         (both reverse tris have a duplicate canon vertex from canon_v nanometer \
         quantization collapsing two upstream-distinct vertices into the same \
         canonical index; canary §4 verified `subdivide_mesh_pair` output is \
         degen-free in 14/14 booleans probed — the DEGEN is introduced post-canon_v \
         inside `flood_fill_patches`). Only M2 is needed for this batch (canary §2: \
         0 NONCONF outside F0020 Extrude 3); M1 has no effect here. \
         Fix per spec §3: `crates/kernel/src/boolean/topology_extract.rs:468-474` \
         — skip `all_tris.push(...)` when `cv[0]==cv[1] || cv[1]==cv[2] || \
         cv[0]==cv[2]`. \
         If `unpaired==2`: M2 filter did not land or has wrong anchor (e.g., the \
         brief's empirically-refuted `face_survival_detect` L1823+L1842 site — \
         per canary §4 + §5, that anchor would iterate over already-clean \
         Cherchi output and filter nothing). Re-anchor to L468-474 in the \
         `all_tris` builder per canary §5 self-canaried recommendation. \
         If `unpaired==1`: M2 filter caught one DEGEN but not both — \
         vertex-index-agnostic check at L468-474 should match both \
         `(31,169)/(133)` AND `(197,200)/(27)` regardless of canon range \
         (canary §5 edge case #3).",
        n_topo_summaries, topo_unpaired
    );

    // ── Assertion 2: max [twin-oracle] unpaired_count <= 0 (regression guard)
    //
    // EMPIRICAL BASELINE CONFIRMATION (test-amender-n, PR-Y22-RECOVERY R-b):
    // Per canary-runner-9 §2 + adversary-22 v1 §6 the pre-PR baseline on
    // plain HEAD for the F0044+F0045+R0092 batch is
    // `[twin-oracle] unpaired_count=0` across all 7 flood_fill_patches
    // invocations. The DEGEN's reverse is not in arena because canon_v
    // collapsed it, so the `twin=None AND reverse-in-arena` test excludes
    // it. The `<= 0` form is effectively `== 0` but uses the regression-
    // guard idiom for consistency with the F0020 sibling assertion above.
    //
    // This guards against M2 accidentally introducing a new asymmetric
    // pairing path (e.g., filtering only one direction of the DEGEN's
    // adjacency and leaving a stale half-edge with twin=None and
    // reverse-in-arena). PR-Y20-MODE-A regression check inherited from
    // PR-Y20's spec §2 I2.
    let twin_unpaired = max_twin_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y22-test] no `[twin-oracle] unpaired_count=` line in F0044 \
             batch stderr. TWIN_DEBUG=1 gate failed. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert!(
        twin_unpaired <= 0,
        "[pr-y22-test] PR-Y22-MODE-A-MISSING regression guard: expected MAX \
         `[twin-oracle] unpaired_count<=0` across all {} flood_fill_patches \
         invocations on F0044+F0045+R0092 batch post-PR, got max twin_unpaired={}. \
         Pre-PR baseline (canary-runner-9 §2 + adversary-22 v1 §6): batch reports \
         `[twin-oracle] unpaired_count=0` on plain HEAD (the DEGEN's reverse is \
         not in arena because canon_v collapsed it, so the `twin=None AND \
         reverse-in-arena` test excludes it). A non-zero value post-PR-Y22 \
         means M2's filter introduced an asymmetric arena state — likely \
         filtered one direction of a DEGEN-adjacent tri but left a stale HE \
         pointing into the filtered position. Re-audit the L468-474 anchor: it \
         must filter the FlatSubTri push BEFORE Step 4 manifold-incidence \
         counting (canary §5 edge case #2) so downstream `tri_to_patch` \
         indexing never sees the degen.",
        n_twin_oracle,
        twin_unpaired
    );

    // Reference the results so they stay bound. We do NOT assert on
    // r.status per spec §6 informational — the case-level outcomes are
    // downstream of the [topo-extract] / [twin-oracle] layer this PR targets.
    let _ = results;
}
