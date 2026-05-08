//! PR-Y23-OPEN-LOOP-EMISSION — RED-phase regression tests for the
//! open-chain emission residual at `flood_fill_patches::Step 6`.
//!
//! ## RED PHASE STATUS
//!
//! Expected to fail on `8de94e5` (PR-Y22-RECOVERY baseline). Becomes GREEN
//! when impl-z23 lands the closure check at the canary's named anchor per
//! `specs/yang_pr_y23_open_loop_emission.md`:
//!  - **Anchor:** `crates/kernel/src/boolean/topology_extract.rs:961`
//!    (the `loops.push(chain)` site at the close of Step 6's loop-chaining
//!    body). Spec §4 selected fix-shape (a): drop chains where
//!    `chain.last().v1 != chain.first().v0` — i.e. open chains — instead of
//!    pushing them into `loops`.
//!
//! ## Defect class (canary memo §1 P1+P2+P3)
//!
//! When the R3 ownership pre-pass strips a direction from a patch (here:
//! `(10→11)` from patch 7 because patch 6 wins the lex tie-break on
//! `SourceFace = midA_face4`), patch 7 is left with an open path
//! `11 → 12 → 10`. The Step 6 loop-chaining body walks this adjacency to
//! `chain = [(11,12,_), (12,10,_)]` and exits via `outgoing = None`
//! (`first_v0 = 11`, `last_v1 = 10`, `closed = false`). At L961 the chain
//! is `loops.push(chain)`-ed unconditionally. Step 7 then constructs an
//! `n=2` half-edge ring at L1131-1146 with a circular `next` pointer:
//!  - HE 58 (i=0): `next = 59` ✓ (origin BV38 agrees with declared dest BV38)
//!  - HE 59 (i=1): `next = 58` ← wraps back to BV27, NOT declared dest BV26
//!
//! The wrap-back manufactures a phantom `(BV38 → BV27)` arena-traversal
//! directed edge that has no construction-time entry in `directed_he`. The
//! `[twin-oracle]` at L1445-1449 sees both HE 58 and HE 59 as
//! `twin = None` with their reverse present in arena traversal — and
//! reports `unpaired_count = 2`. The downstream
//! `validate_yang_result_topology` (`yang_integration.rs:1300-1308`) then
//! panics on `half_edge[58].twin = None but arena contains a HE for the
//! reverse direction (38->27)`.
//!
//! Yang 2025 §3 (line 252 of `/tmp/yang2025.txt`):
//!
//! > "...with each edge shared by two adjacent faces."
//!
//! Cherchi 2022 §3 (line 248 of `/tmp/cherchi2022.txt`):
//!
//! > "the arrangement is guaranteed to be a well formed simplicial complex
//! >  and surface patches are bounded by closed loops of non-manifold
//! >  edges, namely the intersection lines."
//!
//! Both papers mandate that patch boundaries be closed loops; emitting an
//! open chain at L961 violates the contract. PR-Y23 restores it.
//!
//! ## Spec contract under test (spec §5 + §6)
//!
//! Spec §5 invariants (paper-cited):
//!  - **I1 (Cherchi 2022 §3):** for every HE with `twin == None`, the arena-
//!    traversal directed edge `(he.origin, arena.half_edges[he.next].origin)`
//!    equals the construction-time directed edge `(v0_brep, v1_brep)`.
//!  - **I2 (Yang §3 + §4.4.2; Cherchi 2022 §3):** every chain pushed to
//!    `loops` in `Step 6` satisfies `chain.last().v1 == chain.first().v0`.
//!  - **I3 (load-bearing):** F0020 Extrude 3 `[twin-oracle] unpaired_count
//!    == 0` for every `flood_fill_patches` invocation.
//!
//! Spec §6 gating (this test enforces §6.2 + the F0044 cohort guard from
//! §6.3.4; the spotlight Status §6.3.1 + corpus deltas §6.3.5..§6.3.10 are
//! adversary-phase oracles, not test-phase asserts):
//!  - F0020 [twin-oracle] `unpaired_count` MAX across booleans drops 2 → 0.
//!  - F0044 batch [twin-oracle] `unpaired_count` MAX stays at 0 (PR-Y22
//!    GREEN must not regress).
//!
//! ## Pre-fix empirical baseline
//!
//! Per canary memo §1 P3 + P4 ("Twin-oracle confirmation") on `8de94e5`:
//!
//! ```text
//! [topo-extract] summary: paired=48, unpaired=0, ambiguous=0   ← F0020 b#1 (Extrude 2)
//! [twin-oracle] unpaired_count=0
//! [topo-extract] summary: paired=65, unpaired=0, ambiguous=0   ← F0020 b#2 (Extrude 3)
//! [twin-oracle] unpaired_count=2                              ← THE LOAD-BEARING RED
//! [twin-oracle] offender he=58 ... origin=v27 dest=v38
//! [twin-oracle] offender he=59 ... origin=v38 dest=v27
//! ```
//!
//! MAX [twin-oracle] unpaired_count across F0020 = 2 on `8de94e5`,
//! expected 0 post-PR-Y23 (closure check at L961 drops patch 7's
//! `chain_len=2 closed=false` chain; HE 58 / HE 59 are never constructed;
//! oracle has no orphan to report).
//!
//! For F0044 batch (F0044 + F0045 + R0092, mirroring `spotlight_f0044`),
//! per PR-Y22-RECOVERY adversary §6 + this test's sibling
//! `pr_y22_f0044_b5_mode_a_missing_drops_by_2`:
//!
//! ```text
//! [twin-oracle] unpaired_count=0   ← all 7 invocations
//! ```
//!
//! MAX = 0 on `8de94e5`. Post-PR-Y23 must remain 0 (regression guard).
//!
//! ## How to run
//!
//! ```text
//! YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
//!     --test pr_y23_open_loop_emission_regression -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the tests perform process-global
//! stderr FD redirection and `set_var` on env vars; parallel execution
//! would race those operations. Mirrors `pr_y22_mode_a_missing_regression.rs`.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Process-global FD swap: redirect stderr → tempfile, run `f`, restore,
/// return `(f's return value, captured stderr as lossy UTF-8)`. Safe only
/// with `--test-threads=1`. Copied verbatim from
/// `pr_y22_mode_a_missing_regression.rs`.
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
/// stderr. Returns `None` if no such line exists.
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

/// PR-Y23-OPEN-LOOP-EMISSION spec §5 I3 + §6.2 load-bearing gate: assert
/// F0020's MAX `[twin-oracle] unpaired_count` across all flood_fill_patches
/// invocations drops from 2 → 0 post-PR.
///
/// F0020 has 3 sequential extrudes; flood_fill_patches fires once per
/// non-degenerate boolean. Per canary §1: b#1 (Extrude 2) has
/// `[twin-oracle] unpaired_count=0`, b#2 (Extrude 3) has
/// `[twin-oracle] unpaired_count=2` — so MAX across F0020 = 2 on `8de94e5`,
/// expected 0 post-PR-Y23.
///
/// ## Why this counter, not [topo-extract] summary
///
/// PR-Y22 closed `[topo-extract] summary unpaired=N` to 0 on F0020 by
/// addressing the PRE-pairing miss counter (M1 NMM-incidence + M2 canon-
/// degen filter). The 2 residual edges left in `[twin-oracle] unpaired_
/// count` are a downstream layer: open-chain emission at Step 6 L961 →
/// circular `next`-ring at Step 7 L1131-1146 → arena-traversal keying at
/// L1445-1449 sees the wrap-back as a phantom reverse. The `[twin-oracle]`
/// counter (L1458 `twin=None AND reverse-in-arena`) is therefore the
/// correct gate for PR-Y23's targeted layer.
///
/// Per spec §6.3.1 (informational): F0020 spotlight `Status:Failed` MAY
/// NOT clear if other downstream layers panic post-fix. This test asserts
/// only the [twin-oracle] counter; the spotlight status is verified by
/// adversary-z23 against §6.3.1.
#[test]
#[ignore]
fn pr_y23_f0020_twin_oracle_zero() {
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

    let n_twin_oracle = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y23-test] F0020 [twin-oracle] unpaired_count lines: {} \
         (canary §1: F0020 has 3 sequential extrudes; b#1 + b#2 are \
         non-degenerate booleans that fire the oracle; canary baseline \
         b#1 unpaired_count=0, b#2 unpaired_count=2)",
        n_twin_oracle
    );
    eprintln!(
        "[pr-y23-test] F0020 max `[twin-oracle] unpaired_count`: {:?} \
         (pre-PR baseline on 8de94e5: 2; post-PR-Y23 expected: 0; \
          LOAD-BEARING GATE per spec §5 I3 + §6.2)",
        max_twin_unpaired
    );
    eprintln!(
        "[pr-y23-test] F0020 case status={:?} detail={}",
        r.status, r.detail
    );

    // ── Load-bearing assertion: max [twin-oracle] unpaired_count == 0
    //    (spec §5 I3 + §6.2)
    //
    // Per spec §6.3.1 (informational): F0020 spotlight Status MAY remain
    // Failed if a different downstream layer panics post-fix. This test
    // asserts ONLY the layer-targeted [twin-oracle] residual drop, NOT
    // case Status:Passed. Status check is adversary-z23's gate §6.3.1.
    let twin_unpaired = max_twin_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y23-test] no `[twin-oracle] unpaired_count=` line in F0020 \
             stderr. TWIN_DEBUG=1 gate failed or pipeline aborted before \
             flood_fill_patches. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        twin_unpaired, 0,
        "[pr-y23-test] PR-Y23-OPEN-LOOP-EMISSION spec §5 I3 + §6.2 \
         load-bearing gate violation (Yang 2025 §3 'each edge shared by \
         two adjacent faces' + Cherchi 2022 §3 'surface patches are \
         bounded by closed loops of non-manifold edges' patch-boundary \
         closed-loop invariant): expected MAX `[twin-oracle] unpaired_\
         count == 0` across all {} flood_fill_patches invocations on \
         F0020 post-PR-Y23, got max twin_unpaired={}. \
         Pre-PR baseline (canary §1 P4 on 8de94e5): F0020 Extrude 3 \
         (b#2) reports `[twin-oracle] unpaired_count=2` with offenders \
         he=58 (origin=v27, traversal-dest=v38) and he=59 (origin=v38, \
         traversal-dest=v27 via wrap-back). Construction-time, HE 59's \
         destination was BV26 — the dest=v27 is the arena-traversal \
         consequence of an open-chain `next`-ring wrapping `next_idx = \
         (1+1) % 2 = 0` back to HE 58. \
         Mechanism (canary §1 P0-P4 + §3 layer table): \
         - Layer 1 (PR-Y23 ANCHOR, `topology_extract.rs:913-963`): R3 \
           ownership at L810-863 strips `(10→11)` from patch 7 (lex \
           tie-break with patch 6 on same SourceFace=midA_face4). Patch \
           7 left with `(11→12)` and `(12→10)` — open path `11→12→10`. \
           At L961 `loops.push(chain)` emits the open chain (chain_len=2, \
           first_v0=11, last_v1=10, closed=false). \
         - Layer 2 (downstream consumer, `topology_extract.rs:1131-1146`): \
           Step 7's `next_idx = HalfEdgeIdx(he_base.0 + (i+1) % n)` with \
           n=2 wraps HE 59 back to HE 58. Symptom site, not anchor — \
           correct on closed inputs. \
         - Layer 3 (downstream consumer, `topology_extract.rs:1445-1449`): \
           [twin-oracle] keys `arena_dir_edges` on `(he.origin, \
           arena.half_edges[he.next].origin)`. Sees the wrap-back as a \
           phantom reverse for HE 58's `(27→38)`. Diagnostic site, not \
           anchor — correct on closed inputs. \
         Fix per spec §4 (a): closure check at `topology_extract.rs:961` \
         — drop chains where `chain.last().v1 != chain.first().v0` \
         instead of `loops.push(chain)`-ing them. Restores Yang §3 + \
         Cherchi 2022 §3 patch-boundary closed-loop invariant at the \
         site that violates it. \
         If `twin_unpaired==2`: closure check did not land OR was \
         applied at the wrong site (verify L961 is the loops.push site). \
         If `twin_unpaired==1`: one of HE 58 / HE 59 was dropped but the \
         other still constructs — closure check is firing on first_v0 \
         vs last_v1 inconsistently, or chain order was rotated by an \
         unrelated change. \
         If `twin_unpaired>=3`: closure check overshot and dropped \
         legitimately-closed chains, leaving NEW orphans elsewhere — \
         re-audit the closure predicate (must be strict integer \
         equality on canonical-vertex indices, no tolerance, no \
         alternative endpoint matching).",
        n_twin_oracle, twin_unpaired
    );
}

/// PR-Y23-OPEN-LOOP-EMISSION spec §6.3.4 cohort regression guard: assert
/// the F0044 batch (F0044+F0045+R0092 — same composition as
/// `spotlight_f0044`) MAX `[twin-oracle] unpaired_count` across all
/// flood_fill_patches invocations stays at 0 post-PR-Y23.
///
/// PR-Y22-RECOVERY achieved `[twin-oracle] unpaired_count=0` for the F0044
/// batch (the M2 canon-degen filter eliminated the two DEGEN entries that
/// would otherwise have surfaced via wrap-back; their reverse is not in
/// arena because canon_v collapsed it). PR-Y23's closure check at L961
/// MUST NOT regress this state — i.e. it must not drop any chains that
/// would have closed legitimately on the F0044 batch.
///
/// Pre-fix baseline on `8de94e5`: MAX = 0. Post-fix expectation: MAX = 0.
/// This is a NON-REGRESSION test (already 0 pre-fix; the test runs
/// alongside the F0020 RED test to verify post-impl that PR-Y23's anchor
/// is bounded — i.e. the closure check fires on F0020's open chains
/// without spuriously dropping closed chains in the cohort).
///
/// Per spec §12 recommendation #2: this test "may treat as a sanity
/// assertion that runs alongside" the F0020 RED test. FIP §4.3 requires
/// numeric assertions; `assert_eq!(twin_unpaired, 0)` is numeric.
#[test]
#[ignore]
fn pr_y23_f0044_twin_oracle_no_regression() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 + TWIN_DEBUG=1 — mirror pr_y23_f0020_twin_oracle_zero.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    // Mirror spotlight_f0044 batch composition: F0044 + F0045 + R0092.
    // Per PR-Y22-RECOVERY canary §2 the b#5 invocation (R0092's first
    // boolean) was the locus of pre-PR-Y22 DEGEN residual. PR-Y22
    // resolved it; PR-Y23 must keep it resolved.
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

    let n_twin_oracle = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_twin_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    eprintln!(
        "[pr-y23-test] F0044 batch [twin-oracle] unpaired_count lines: {} \
         (PR-Y22-RECOVERY canary §2 expects 7 = F0044 b#1 + F0045 b#2-4 \
         + R0092 b#5-7)",
        n_twin_oracle
    );
    eprintln!(
        "[pr-y23-test] F0044 batch max `[twin-oracle] unpaired_count`: {:?} \
         (pre-PR baseline on 8de94e5: 0; post-PR-Y23 expected: 0; \
          COHORT REGRESSION GUARD per spec §6.3.4)",
        max_twin_unpaired
    );
    for (case_id, r) in cases.iter().zip(results.iter()) {
        eprintln!(
            "[pr-y23-test] {} status={:?} detail={}",
            case_id, r.status, r.detail
        );
    }

    // ── Cohort regression guard: max [twin-oracle] unpaired_count == 0
    //    (spec §6.3.4)
    let twin_unpaired = max_twin_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y23-test] no `[twin-oracle] unpaired_count=` line in F0044 \
             batch stderr. TWIN_DEBUG=1 gate failed. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        twin_unpaired, 0,
        "[pr-y23-test] PR-Y23-OPEN-LOOP-EMISSION spec §6.3.4 cohort \
         regression guard violation: expected MAX `[twin-oracle] \
         unpaired_count == 0` across all {} flood_fill_patches \
         invocations on F0044+F0045+R0092 batch post-PR-Y23, got max \
         twin_unpaired={}. \
         Pre-PR baseline (PR-Y22-RECOVERY adversary-22 §6 + canary-\
         runner-9 §2 on 8de94e5): batch reports `[twin-oracle] \
         unpaired_count=0` across all 7 invocations — PR-Y22's M2 \
         canon-degen filter at L468-491 eliminated the two DEGEN \
         entries that would otherwise have surfaced via wrap-back. A \
         non-zero value post-PR-Y23 means the closure check at L961 \
         either dropped a chain that legitimately closes, OR introduced \
         a side-effect that allows a degenerate-closed chain (where \
         endpoint coincidence is canon_v-collapse-induced rather than \
         genuine cycle closure) to slip past — both shapes constitute a \
         cohort regression. \
         Action per spec §7.3 + §6.3.4: ABORT. The closure check has \
         overshot or has an unanticipated interaction with the M1+M2 \
         classification path. Bank as PR-Y24 with diagnostic: dump \
         pre-/post-fix diff of dropped vs emitted chain counts on each \
         cohort case; consider whether option (b) (strengthen R3 \
         ownership pre-pass at L810-863) becomes the right anchor.",
        n_twin_oracle, twin_unpaired
    );

    // Reference the results so they stay bound. We do NOT assert on
    // r.status per spec §6 informational — case-level outcomes for the
    // F0044 batch are downstream of the [twin-oracle] layer this PR
    // targets.
    let _ = results;
}
