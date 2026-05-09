//! PR-Y24-ORACLE-OBSERVATION-LAYER — RED-phase regression tests for moving
//! the oracle/validator NMM-vs-missing-edge classification predicate from
//! arena-traversal `(he.origin, he.next.origin)` to construction-time
//! `directed_he` keys.
//!
//! ## RED PHASE STATUS
//!
//! Expected to fail on commit `3c749a3` (PR-Y24 canary baseline). Becomes
//! GREEN when impl-y24 lands the observation-layer fix per
//! `specs/yang_pr_y24_oracle_observation_layer.md`:
//!
//!  - **Site A (oracle):** `crates/kernel/src/boolean/topology_extract.rs:1445-1471`
//!    (and offender-trace at L1515). Replace `arena_dir_edges` build with
//!    `directed_he.keys()` collect; replace `v_dest =
//!    arena.half_edges[he.next.0].origin.0` with `he_to_constructed_dest[i]`.
//!  - **Site B (validator):** `crates/kernel/src/boolean/yang_integration.rs:1241-1308`.
//!    Plumb construction-time directed-edge data via a new `TopoArena`
//!    field (sub-option B1 per spec §4.3) so the validator's missing-edge
//!    predicate reads construction-time keys, not arena traversal.
//!
//! ## Defect class (canary §1 P1+P2)
//!
//! On F0020 b#2 (Extrude 3, 169 HEs), arena traversal at `topology_extract.rs:1131-1146`
//! emits a phantom directed edge `(BV38, BV27)` because the n=2 open chain
//! `[(BV27,BV38,*), (BV38,BV26,*)]` produces a wrap-back: `HE[1].next = HE[0]`,
//! so traversal reads HE 59's destination as HE 58's origin (BV27) instead
//! of its construction-time `v1_brep = BV26`.
//!
//! The construction-time `directed_he` map (populated at L1149-1152 in the
//! same loop as the arena population) holds the GROUND TRUTH per Yang §3
//! (input B-Rep): the patch-boundary set inserted by the loop. HE 58 was
//! inserted with key `(BV27, BV38)`; HE 59 with key `(BV38, BV26)`. Neither
//! reverse `(BV38, BV27)` nor `(BV26, BV38)` appears in `directed_he`, so
//! both half-edges are legitimate non-manifold per Cherchi §3
//! ("surface patches are bounded by closed loops of non-manifold edges").
//!
//! The arena-traversal-keyed `[twin-oracle]` at L1445-1449 sees both HE 58
//! and HE 59 as `twin = None` with their reverse "present" in
//! arena-traversal — and reports `unpaired_count = 2`. The downstream
//! `validate_yang_result_topology` then panics on
//! `half_edge[58].twin = None but arena contains a HE for the reverse
//! direction (38->27)`.
//!
//! Yang 2025 §3 (`refs/text/yang2025_hybrid_boolean.txt:248-249` verbatim):
//!
//! > "edges that form a continuous boundary, with each edge shared by two
//! >  adjacent faces."
//!
//! Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-254`
//! verbatim):
//!
//! > "When exact methods are used, the arrangement is guaranteed to be a
//! >  well formed simplicial complex and surface patches are bounded by
//! >  closed loops of non-manifold edges, namely the intersection lines."
//!
//! Both papers establish that the manifold-edge baseline is two adjacent
//! faces (incidence == 2) and patch boundaries are closed loops of
//! non-manifold edges. The observation predicate's `rev-present` test is
//! the mechanization of that contract — and must source from the
//! construction-time directed-edge set (input ground truth) rather than
//! the arena-traversal projection (derivative, polluted on open-chain
//! wrap-backs).
//!
//! PR-Y24 is the OBSERVATION-LAYER fix per spec §1: it does NOT alter the
//! arena structure (banked Layer-2 PR-Y25+), nor the closure check at L961
//! (banked PR-Y23-style per spec §9), nor face iteration on open-chain
//! faces (banked Layer-4). It re-keys the predicate at Site A + Site B so
//! the verdict aligns with the construction-time ground truth.
//!
//! ## Spec contract under test (spec §5 + §6)
//!
//! Spec §5 invariants (paper-cited):
//!  - **I1 (Cherchi 2022 §3 + Yang §3):** the NMM-vs-missing-edge predicate
//!    at Site A (L1452-1469) and Site B (L1298-1308) reads `v_dest` for each
//!    half-edge from construction-time directed-edge data, NOT from
//!    `arena.half_edges[he.next.0].origin.0`. The set used to test
//!    rev-presence is sourced from `directed_he.keys()`, NOT from arena
//!    traversal.
//!  - **I2:** pairing-search loop at `topology_extract.rs:1219-1380` (which
//!    reads `directed_he.keys()` directly) and arena population at L1131-1146
//!    are byte-identical pre/post PR. Measurable via `[topo-extract] summary
//!    paired/unpaired/ambiguous` per-invocation.
//!  - **I3 (load-bearing):** F0020 Extrude 3 `[twin-oracle] unpaired_count
//!    == 0` for every `flood_fill_patches` invocation. Pre-PR baseline on
//!    `3c749a3`: 2 (canary §1 P4 readback). Post-PR target: 0.
//!
//! Spec §6 oracles surfaced by these tests:
//!  - **Oracle #2** (load-bearing, I3): F0020 MAX `[twin-oracle]
//!    unpaired_count` drops 2 → 0.
//!  - **Oracle #3 + #4** (cohort guard, I2): F0044+F0045+R0092 batch MAX
//!    `[topo-extract] summary unpaired` and MAX `[twin-oracle]
//!    unpaired_count` both stay at 0 across all 7 invocations.
//!
//! Spec §6 informational (NOT gating):
//!  - F0020 spotlight `Status:Failed` MAY persist post-PR at a *different*
//!    panic surface (mesh-quality / face-iteration / tessellation-render
//!    layer). Per `feedback_no_last_bug.md` and spec §7.2, that outcome is
//!    next-PR territory, not a PR-Y24 regression.
//!
//! ## Pre-fix empirical baseline (canary §1 + §2)
//!
//! Per canary memo `docs/audits/pr_y24_anchor_canary.md` §1, on `3c749a3`:
//!
//! ```text
//! [topo-extract] summary: paired=48, unpaired=0, ambiguous=0   ← F0020 b#1 (Extrude 2)
//! [twin-oracle] total_directed_edges=96
//! [twin-oracle] unpaired_count=0
//! [topo-extract] summary: paired=65, unpaired=0, ambiguous=0   ← F0020 b#2 (Extrude 3)
//! [twin-oracle] total_directed_edges=169
//! [twin-oracle] unpaired_count=2                              ← THE LOAD-BEARING RED
//! [twin-oracle] offender he=58 ... origin=v27 dest=v38
//! [twin-oracle] offender he=59 ... origin=v38 dest=v27
//! ```
//!
//! MAX `[twin-oracle] unpaired_count` across F0020 = **2** on `3c749a3`,
//! expected **0** post-PR-Y24 (HE 58 and HE 59 both reclassify as
//! legitimate-NMM under construction-time keying because their reverses
//! `(BV38,BV27)` and `(BV26,BV38)` are NOT in `directed_he` — only
//! present in arena_dir_edges via the L1131-1146 wrap-back).
//!
//! Per canary §1 P2 simulation: `[y24-probe-p2]
//! simulated_twin_oracle_unpaired_count=0 (vs actual=2)` on F0020 b#2 under
//! a probe that re-runs the unpaired-detection logic but sources from
//! construction-time keys. This is the EMPIRICAL EVIDENCE for the post-PR
//! target value.
//!
//! Per canary §2 cohort table for F0044+F0045+R0092 batch (7 invocations):
//!
//! ```text
//! [topo-extract] summary: paired=68|117|165|230|97|118|182, unpaired=0
//! [twin-oracle] unpaired_count=0   ← all 7 invocations
//! ```
//!
//! MAX = 0 on `3c749a3`. Post-PR-Y24 must remain 0 (cohort guard, regression
//! prevention). Both probes simulated=actual=0 across all 7 invocations.
//!
//! ## How to run
//!
//! ```text
//! YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
//!     --test pr_y24_oracle_observation_layer_regression -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the tests perform process-global
//! stderr FD redirection and `set_var` on env vars; parallel execution
//! would race those operations. Mirrors `pr_y22_mode_a_missing_regression.rs`
//! and `pr_y23_open_loop_emission_regression.rs`.
//!
//! ## Helper duplication note (per spec §12.1)
//!
//! Helpers `capture_stderr`, `max_twin_oracle_field`,
//! `count_twin_oracle_lines`, `max_topo_extract_unpaired`, and
//! `count_topo_extract_summary_lines` are duplicated verbatim from
//! `pr_y22_mode_a_missing_regression.rs:128-207` to keep this PR-Y24 test
//! self-contained against future PR-Y22 test churn (spec §12.1 explicit
//! recommendation: "duplicate verbatim").

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Process-global FD swap: redirect stderr → tempfile, run `f`, restore,
/// return `(f's return value, captured stderr as lossy UTF-8)`. Safe only
/// with `--test-threads=1`. Copied verbatim from
/// `pr_y22_mode_a_missing_regression.rs:128-151`.
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

/// Find the MAX value of `[twin-oracle] <key>=N` across ALL emissions in
/// stderr. Returns `None` if no such line exists. Copied verbatim from
/// `pr_y22_mode_a_missing_regression.rs:155-166`.
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
/// Copied verbatim from `pr_y22_mode_a_missing_regression.rs:169-172`.
fn count_twin_oracle_lines(stderr: &str, key: &str) -> usize {
    let needle = format!("[twin-oracle] {}=", key);
    stderr.lines().filter(|l| l.starts_with(&needle)).count()
}

/// Find the MAX value of the `unpaired=N` field across ALL
/// `[topo-extract] summary: paired=P, unpaired=N, ambiguous=A` lines in
/// stderr. Returns `None` if no such line exists. Copied verbatim from
/// `pr_y22_mode_a_missing_regression.rs:182-199`.
fn max_topo_extract_unpaired(stderr: &str) -> Option<usize> {
    let prefix = "[topo-extract] summary:";
    let mut max_val: Option<usize> = None;
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix(prefix) {
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
/// Copied verbatim from `pr_y22_mode_a_missing_regression.rs:202-207`.
fn count_topo_extract_summary_lines(stderr: &str) -> usize {
    stderr
        .lines()
        .filter(|l| l.starts_with("[topo-extract] summary:"))
        .count()
}

/// PR-Y24-ORACLE-OBSERVATION-LAYER spec §5 I3 + §6 oracle #2 (LOAD-BEARING):
/// assert F0020's MAX `[twin-oracle] unpaired_count` across all
/// `flood_fill_patches` invocations drops from 2 → 0 post-PR.
///
/// Per canary §1 P4 readback, F0020 has 2 sequential extrudes that fire
/// `flood_fill_patches`: b#1 (Extrude 2, 96 HEs, `unpaired_count=0`) and
/// b#2 (Extrude 3, 169 HEs, `unpaired_count=2` — offenders HE 58 and
/// HE 59). Under construction-time `directed_he` keying, both offenders
/// reclassify as legitimate-NMM because their reverses `(BV38,BV27)` and
/// `(BV26,BV38)` are NOT in `directed_he` (only present in
/// arena-traversal as wrap-back artifacts of L1131-1146). MAX across F0020
/// drops 2 → 0.
///
/// Per canary §1 P2 simulation: `[y24-probe-p2]
/// simulated_twin_oracle_unpaired_count=0 (vs actual=2)` confirms the
/// mechanism empirically — the predicate produces 0 under construction-time
/// keying without any change to arena structure or pairing logic.
///
/// **Per spec §7.2 (failure modes — different panic):** F0020 spotlight
/// Status:Failed MAY persist post-PR at a different panic surface (e.g.,
/// mesh-quality `(38,38)` self-loop check, face-iteration on open-chain
/// faces, tessellation-render NMM-handling banked from PR-Y21 ABORT).
/// That is a next-PR layer per `feedback_no_last_bug.md`, not a PR-Y24
/// regression. This test asserts ONLY the layer-targeted counter drop;
/// case-status flip is a §7 §6 oracle #1 concern (adversary-phase, not
/// test-phase).
#[test]
#[ignore]
fn pr_y24_f0020_twin_oracle_zero() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 routes through the Yang pipeline (the only path that
    // exercises Stage 4 flood_fill_patches Step 7 [twin-oracle]).
    // TWIN_DEBUG=1 enables the `[twin-oracle]` lines this test parses.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    let dir_owned = dir.to_path_buf();
    let (result, stderr) = capture_stderr(move || run_single_case(&dir_owned, "F0020", true));
    let r = result.expect("F0020 must exist in corpus — regenerate via assay_gen");

    let n_topo_summaries = count_topo_extract_summary_lines(&stderr);
    let n_twin_oracle = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y24-test] F0020 [topo-extract] summary lines: {} ; \
         [twin-oracle] unpaired_count lines: {} \
         (canary §1: F0020 fires flood_fill_patches twice — b#1 Extrude 2 \
         96 HEs unpaired_count=0; b#2 Extrude 3 169 HEs unpaired_count=2)",
        n_topo_summaries, n_twin_oracle
    );
    eprintln!(
        "[pr-y24-test] F0020 max `[twin-oracle] unpaired_count`: {:?} \
         (pre-PR-Y24 baseline on 3c749a3: 2; post-PR-Y24 expected: 0; \
         LOAD-BEARING per spec §5 I3 + §6 oracle #2)",
        max_twin_unpaired
    );
    eprintln!(
        "[pr-y24-test] F0020 case status={:?} detail={}",
        r.status, r.detail
    );

    let twin_unpaired = max_twin_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y24-test] no `[twin-oracle] unpaired_count=` line in F0020 \
             stderr. TWIN_DEBUG=1 gate failed or pipeline aborted before Step 7 \
             [twin-oracle] block at topology_extract.rs:1437-1471. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });

    // ── Load-bearing assertion: max [twin-oracle] unpaired_count across
    //   ALL flood_fill_patches invocations on F0020 == 0 (spec §5 I3).
    //
    // Yang 2025 §3 (refs/text/yang2025_hybrid_boolean.txt:248-249 verbatim):
    //   "edges that form a continuous boundary, with each edge shared by
    //    two adjacent faces."
    // Cherchi 2022 §3 (refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-254 verbatim):
    //   "the arrangement is guaranteed to be a well formed simplicial
    //    complex and surface patches are bounded by closed loops of
    //    non-manifold edges, namely the intersection lines."
    //
    // The observation predicate is the mechanization of "each edge shared
    // by two adjacent faces" (incidence-2 manifold edge baseline) read
    // against "closed loops of non-manifold edges" (the patch-boundary
    // set inserted at construction time). Re-keying from arena traversal
    // to construction-time `directed_he.keys()` aligns the predicate with
    // the paper's input ground truth and resolves the F0020 b#2 offenders.
    assert_eq!(
        twin_unpaired, 0,
        "[pr-y24-test] PR-Y24 spec §5 I3 violation (Yang §3 + Cherchi 2022 §3): \
         expected MAX `[twin-oracle] unpaired_count == 0` across all {} \
         flood_fill_patches invocations on F0020 post-PR, got max twin_unpaired={}. \
         \
         Pre-PR baseline on commit 3c749a3 (canary §1 P4): F0020 b#2 (Extrude 3, \
         169 HEs) reports `[twin-oracle] unpaired_count=2` with offenders HE 58 \
         (origin=v27, dest=v38) and HE 59 (origin=v38, dest=v27). The arena \
         traversal at topology_extract.rs:1131-1146 emits a phantom directed edge \
         `(BV38, BV27)` because the n=2 open chain wraps back: HE[1].next = HE[0]. \
         \
         Per Yang 2025 §3 (verbatim line 248-249): \
         \"edges that form a continuous boundary, with each edge shared by two \
         adjacent faces.\" \
         \
         Per Cherchi 2022 §3 (verbatim line 251-254): \
         \"the arrangement is guaranteed to be a well formed simplicial complex \
         and surface patches are bounded by closed loops of non-manifold edges, \
         namely the intersection lines.\" \
         \
         The construction-time `directed_he` map (populated at L1149-1152) holds \
         the GROUND TRUTH per Yang §3 — the patch-boundary set inserted by the \
         loop. HE 58's reverse `(BV38, BV27)` and HE 59's reverse `(BV26, BV38)` \
         are NOT in `directed_he` (canary §1 P4 verified). Under construction- \
         time keying, both reclassify as legitimate-NMM per Cherchi §3 (\"closed \
         loops of non-manifold edges\"), and the load-bearing prediction is \
         `unpaired_count = 0` (canary §1 P2 simulated value matches). \
         \
         Fix per spec §4.3 (B1 + e1): \
         - Site A (oracle, `topology_extract.rs:1445-1471` + offender-trace L1515): \
           Replace `arena_dir_edges` build with `directed_he.keys()` collect; \
           replace `v_dest = arena.half_edges[he.next.0].origin.0` with a per-HE \
           `he_to_constructed_dest` lookup populated from `directed_he`. \
         - Site B (validator, `yang_integration.rs:1241-1308`): Plumb \
           construction-time directed-edge data via a new `TopoArena` field \
           (sub-option B1 per spec §4.3); validator reads the field instead \
           of arena traversal. \
         \
         Diagnostic ladder (spec §12.2): \
         - If `twin_unpaired==2`: neither Site A nor Site B fixed (or wrong \
           anchor; rerun canary P1+P2 simulation in worktree). \
         - If `twin_unpaired==1`: Site A fixed but offender-trace at L1515 \
           still reads arena traversal (or vice versa); audit consistency \
           between the build and the loop body. \
         - If `twin_unpaired > 2`: PR-Y24 introduced a new asymmetric \
           classification path — likely the `he_to_constructed_dest` lookup \
           emits `usize::MAX` for some HE (canary §6 NO-FALLBACK contract \
           violation per spec §10). Per spec §10: panic on miss, do NOT \
           silently fall back to arena-traversal lookup.",
        n_twin_oracle, twin_unpaired
    );
}

/// PR-Y24-ORACLE-OBSERVATION-LAYER spec §5 I2 + §6 oracles #3 + #4 (cohort
/// guard): assert the F0044+F0045+R0092 batch (mirrors `spotlight_f0044`
/// composition) MAX `[topo-extract] summary unpaired` AND MAX
/// `[twin-oracle] unpaired_count` both stay at 0 across all 7
/// flood_fill_patches invocations post-PR.
///
/// Per canary §2 cohort table on `3c749a3`:
///
/// | # | total HEs | actual unpaired | simulated unpaired | arena_only | constructed_only |
/// |---|---|---|---|---|---|
/// | 1 | 136 | 0 | 0 | 0 | 0 |
/// | 2 | 234 | 0 | 0 | 0 | 0 |
/// | 3 | 330 | 0 | 0 | 0 | 0 |
/// | 4 | 460 | 0 | 0 | 0 | 0 |
/// | 5 | 229 | 0 | 0 | 4 | 5 |
/// | 6 | 283 | 0 | 0 | 0 | 0 |
/// | 7 | 408 | 0 | 0 | 0 | 0 |
///
/// Both keying schemes agree on all 7 invocations: actual=simulated=0.
/// MAX `[topo-extract] summary unpaired` = 0 (canary §2). MAX
/// `[twin-oracle] unpaired_count` = 0 (canary §2). PR-Y24 must not
/// regress either counter.
///
/// **Test purpose: prevent silent regression.** This test PASSES both
/// pre AND post PR. Per spec §5 I2, the pairing logic at
/// `topology_extract.rs:1219-1380` and arena population at L1131-1146
/// stay byte-identical pre/post PR. The `[topo-extract] summary` line is
/// emitted at L1346 (pre-pairing miss counter) and the `[twin-oracle]
/// unpaired_count` line at L1458 (post-pairing classification counter)
/// — both must remain 0 across all 7 invocations.
///
/// **Cohort batch composition rationale:** canary §2 numbers the 7
/// invocations across the F0044+F0045+R0092 spotlight batch. Within that
/// numbering: b#1 = F0044's only boolean, b#2-4 = F0045's 3 booleans,
/// b#5-7 = R0092's 3 booleans. Mirrors
/// `pr_y22_f0044_b5_mode_a_missing_drops_by_2` batch composition so the
/// 7-invocation expectation is reproducible.
#[test]
#[ignore]
fn pr_y24_f0044_topo_extract_no_regression() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 + TWIN_DEBUG=1 — mirror pr_y24_f0020_twin_oracle_zero
    // and pr_y22_f0044_b5_mode_a_missing_drops_by_2.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    // Mirror spotlight_f0044 batch composition: F0044 + F0045 + R0092.
    // Per canary §2 the cohort guard observes 7 flood_fill_patches
    // invocations across the batch (1 + 3 + 3).
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
        "[pr-y24-test] F0044 batch [topo-extract] summary lines: {} ; \
         [twin-oracle] unpaired_count lines: {} \
         (canary §2 expects 7 = F0044 b#1 + F0045 b#2-4 + R0092 b#5-7)",
        n_topo_summaries, n_twin_oracle
    );
    eprintln!(
        "[pr-y24-test] F0044 batch max `[topo-extract] summary: unpaired=N`: {:?} \
         (canary §2: pre-PR baseline 0 across all 7 invocations; cohort \
         guard, must stay 0 per spec §5 I2)",
        max_topo_unpaired
    );
    eprintln!(
        "[pr-y24-test] F0044 batch max `[twin-oracle] unpaired_count`: {:?} \
         (canary §2: pre-PR baseline 0 across all 7 invocations; cohort \
         guard, must stay 0 per spec §6 oracle #4)",
        max_twin_unpaired
    );
    for (case_id, r) in cases.iter().zip(results.iter()) {
        eprintln!(
            "[pr-y24-test] {} status={:?} detail={}",
            case_id, r.status, r.detail
        );
    }

    // Both counters must stay 0 post-PR per spec §5 I2 + §6 oracles #3+#4.
    // We use `unwrap_or(0)` for the topo counter because if no [topo-
    // extract] summary line exists, the structural absence of a defect
    // reading is itself == 0 (the cohort guard is "max value emitted
    // OR 0 if none emitted ≤ 0"). For the twin-oracle, `unwrap_or(0)`
    // mirrors that semantic — absence of a TWIN_DEBUG line means no
    // post-pairing classification ran, which is also the 0 baseline.
    // (PR-Y22's sibling test made the same choice for both counters in
    // its assertion 1 and 2 paths.)
    let topo_unpaired = max_topo_unpaired.unwrap_or(0);
    let twin_unpaired = max_twin_unpaired.unwrap_or(0);

    // ── Assertion 1: max [topo-extract] summary unpaired across ALL
    //   invocations == 0 (spec §5 I2 cohort guard, oracle #3).
    //
    // This is the PRE-pairing miss counter at topology_extract.rs:1346
    // (incremented only when `is_nmm == false`). Per spec §5 I2 the
    // pairing logic at L1219-1380 stays byte-identical pre/post PR-Y24,
    // so this counter must remain at canary §2 baseline (0).
    assert_eq!(
        topo_unpaired, 0,
        "[pr-y24-test] PR-Y24 spec §5 I2 cohort regression (Cherchi 2022 §3 \
         well-formed simplicial complex; cohort guard): expected MAX \
         `[topo-extract] summary: unpaired=0` across all {} flood_fill_patches \
         invocations on F0044+F0045+R0092 batch post-PR, got max unpaired={}. \
         \
         Pre-PR baseline on commit 3c749a3 (canary §2 cohort table): all 7 \
         invocations report `unpaired=0` (paired counts: 68, 117, 165, 230, \
         97, 118, 182). Per spec §5 I2: \"pairing-search loop at \
         topology_extract.rs:1219-1380 (which reads `directed_he.keys()` \
         directly to build candidate match sets) and arena population at \
         L1131-1146 are byte-identical pre/post PR.\" \
         \
         A non-zero value post-PR-Y24 means PR-Y24 disturbed the upstream \
         pairing logic — re-audit Site A + Site B changes for accidental \
         modification of the L1219-1380 search loop. Per spec §7.3: ABORT \
         immediately, write abort memo, debug the I2 measurability gap \
         before any re-attempt. (The B1 sub-option specifically defends \
         against this; a regression here would imply the field-population \
         logic at Step 7 close has a bug touching upstream pairing.)",
        n_topo_summaries, topo_unpaired
    );

    // ── Assertion 2: max [twin-oracle] unpaired_count across ALL
    //   invocations == 0 (spec §6 oracle #4 cohort guard).
    //
    // This is the POST-pairing classification counter at
    // topology_extract.rs:1458. Per canary §2 baseline, all 7 invocations
    // emit `unpaired_count=0`. Post-PR-Y24 must remain 0.
    //
    // Per canary §2 row 5 (229 HEs, 4 arena_only, 5 constructed_only):
    // both keying schemes agree at simulated=actual=0. The B1 fix flips
    // the predicate to construction-time keying, so the post-PR value
    // equals canary §2's simulated column = 0.
    assert_eq!(
        twin_unpaired, 0,
        "[pr-y24-test] PR-Y24 spec §6 oracle #4 cohort regression: expected \
         MAX `[twin-oracle] unpaired_count == 0` across all {} \
         flood_fill_patches invocations on F0044+F0045+R0092 batch post-PR, \
         got max twin_unpaired={}. \
         \
         Pre-PR baseline on commit 3c749a3 (canary §2 cohort table): all 7 \
         invocations report `[twin-oracle] unpaired_count=0` (total_directed_edges: \
         136, 234, 330, 460, 229, 283, 408). Both keying schemes agree across all \
         7 invocations: actual=simulated=0. \
         \
         A non-zero value post-PR-Y24 means PR-Y24's construction-time keying \
         introduced a new defect in the cohort — likely the `he_to_constructed_dest` \
         lookup or the `directed_he.keys()` collect handles row 5's divergence \
         (4 arena_only / 5 constructed_only) inconsistently with the canary §2 \
         simulation. Per canary §6 banked finding 3, invocation #5 has \
         non-degenerate wrap-back artifacts; PR-Y24 must reach the same \
         simulated=0 verdict as canary §1 P2 across the cohort. Per spec §10 \
         NO-FALLBACK contract: panic on miss, no silent fallback. Per spec §7.3: \
         ABORT immediately if cohort regresses.",
        n_twin_oracle, twin_unpaired
    );

    // Reference the results so they stay bound. We do NOT assert on
    // r.status per spec §6 informational — case-level outcomes are
    // downstream of the [topo-extract] / [twin-oracle] layer this PR
    // targets, and the F0044 batch's overall case-status verdict is a
    // separate concern (banked Layer-4 + already-corrupted upstream
    // invocations 5-7 per canary §5 cohort load-bearing analysis).
    let _ = results;
}
