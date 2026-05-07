//! PR-Y20-MODE-A — RED-phase regression test for the NMM (non-manifold meeting)
//! sub-mechanism in `topology_extract.rs::flood_fill_patches` Step 7's `[]` arm.
//!
//! ## RED PHASE STATUS
//!
//! Expected to fail on current `main` (post-PR-Y19-MODE-B). Becomes GREEN
//! when implementer-x lands the `HalfEdge.twin: HalfEdgeIdx → Option<HalfEdgeIdx>`
//! type-system change at:
//!  - `crates/kernel/src/topology/half_edge.rs:38-51` (struct field type change)
//!  - `crates/kernel/src/boolean/topology_extract.rs:1241-1254` (`[]` arm sets
//!    `twin = None` instead of leaving the default `HalfEdgeIdx(0)` sentinel)
//!  - `crates/kernel/src/boolean/yang_integration.rs::validate_yang_result_topology`
//!    (validator distinguishes legitimate NMM `twin=None` from missing-edge defect
//!    via geometric existence check on `directed_edge_to_tris`)
//!
//! per `specs/yang_pr_y20_mode_a.md` §3 + §5 + canary §4 NMM-branch verdict.
//!
//! ## Spec contract under test
//!
//! Spec §2 invariants (paper-faithful per Yang 2025 §4.4.2 directional-symmetry
//! mandate; `feedback_yang_brep_extension_over_cherchi_pure_mesh.md` paper-extension
//! framing; `feedback_yang_only.md` no-fallback discipline):
//!
//! - **I1 (NMM directional-symmetry):** if `he_fwd` has canonical `(v0, v1)` and
//!   `directed_edge_to_tris` lacks `(v1, v0)`, then `arena.half_edges[he_fwd.0].twin
//!   == None` — production proxy: `[twin-oracle] unpaired_count == 0` (the 125 NMM
//!   HEs across F0020/F0044/F0051/cohort flip from "unpaired counter incremented"
//!   to "twin=None, NOT counted as unpaired").
//!
//! - **I2 (manifold 1:1 preserved):** the `[the_one]` arm still pairs up with
//!   `Some(_)` wrapping — production proxy: `[twin-oracle] collision_count == 0`
//!   (post-PR-Y19-MODE-B GREEN; this test acts as canary regression check).
//!
//! ## Pre-fix empirical baseline (canary §3 + reproduced 2026-05-06 by test-author-k)
//!
//! `YANG_BOOLEAN=1 TWIN_DEBUG=1 spotlight_f0044` on current main emits across the
//! 7 booleans in the F0044+F0045+R0092 batch (per canary §3 numbering b#5/b#6/b#7):
//!
//! ```text
//! [topo-extract] summary: paired=68,  unpaired=0,  ambiguous=0   ← F0044 b#1
//! [topo-extract] summary: paired=117, unpaired=0,  ambiguous=0   ← F0045 b#2
//! [topo-extract] summary: paired=165, unpaired=0,  ambiguous=0   ← F0045 b#3
//! [topo-extract] summary: paired=230, unpaired=0,  ambiguous=0   ← F0045 b#4
//! [topo-extract] summary: paired=101, unpaired=31, ambiguous=0   ← R0092 b#5  ← canary §3 row
//! [topo-extract] summary: paired=118, unpaired=37, ambiguous=0   ← R0092 b#6  ← canary §3 row
//! [topo-extract] summary: paired=182, unpaired=36, ambiguous=0   ← R0092 b#7  ← canary §3 row
//! ```
//!
//! Max `[twin-oracle] unpaired_count` across all 7 invocations: **46** (R0092 b#5;
//! the `unpaired_count` post-pairing oracle sees 31 `[]` arm misses + extras from
//! HE chain follow-through). Max `collision_count`: **1** (one F0020-style
//! cross-patch dedup case).
//!
//! Per canary §3 the F0044 batch is **98% NMM (102 of 104 Mode A `[]` cases)**;
//! per spec §5 the post-fix expectation is `unpaired_count` drops 31+37+36 → 1+1+0
//! across the three R0092 booleans (the 2 MISSING residuals stay; banked PR-Y21+
//! per spec §7 anti-scope).
//!
//! **Spec-vs-brief gap (flagged for team-lead per brief §3 deliverable):** the brief
//! asks for `max unpaired_count == 0` as the primary assertion. Spec §5 expects 1+1+0
//! → max=1 residual post-NMM-fix (the 2 MISSING canonical pairs in R0092, banked).
//! This test follows the brief's contract (max == 0). If implementer-x's NMM fix
//! lands cleanly but the 2 MISSING residual persists, this assertion will report
//! `max=1` and the diagnostic message points to the spec §7 banked MISSING follow-on.
//!
//! ## How to run
//!
//! ```text
//! cargo test -p test-harness --test pr_y20_mode_a_regression -- \
//!     --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the test performs process-global stderr
//! FD redirection and `set_var` on env vars; parallel execution would race those
//! operations. Mirrors `pr_y19_mode_b_regression.rs` + `pr_y17_coplanar_regression.rs`.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;
use test_harness::assay::scoring::AssayStatus;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Process-global FD swap: redirect stderr → tempfile, run `f`, restore, return
/// `(f's return value, captured stderr as lossy UTF-8)`. Safe only with
/// `--test-threads=1`. Mirrors `pr_y19_mode_b_regression.rs`.
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

/// PR-Y20-MODE-A spec §5: assert F0044 batch (F0044/F0045/R0092 — the 98% NMM
/// cohort per canary §3) produces a 1:1 manifold + twin=None NMM topology
/// (Yang §4.4.2 directional-symmetry mandate).
///
/// Asserts in priority order so the FIRST failure gives implementer-x the
/// MOST useful diagnostic:
///
/// 1. **NMM invariant — primary RED→GREEN gate:** max `[twin-oracle] unpaired_count`
///    across all flood_fill_patches invocations == 0. Pre-fix baseline: max=46
///    across 7 booleans (F0044 b#1=0, F0045 b#2-4=0, R0092 b#5-7=35/46/44; per
///    canary §3 the 7 `[topo-extract]` summaries report `unpaired=31/37/36` for
///    the failing R0092 trio). Spec §3 type-system change converts NMM cases
///    from "unpaired counter incremented" → "twin=None, not counted".
///
/// 2. **Manifold 1:1 regression check:** max `[twin-oracle] collision_count` == 0.
///    Already GREEN post-PR-Y19-MODE-B; this assertion guards against the NMM fix
///    accidentally regressing PR-Y19's R3 source-face routing.
///
/// 3. **Case-level GREEN:** F0044 spotlight runs ≥1 case to Passed status.
///    Spec §5 expects F0044 b#1 (already paired=68 unpaired=0 on main) to be
///    Passed if NMM fix doesn't introduce regressions; canary §3 expects R0092
///    b#7 to flip Passed (100% NMM, 36 NMM cases all resolve).
#[test]
#[ignore]
fn pr_y20_mode_a_nmm_invariant() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 routes through the Yang pipeline (the only path that
    // exercises Stage 4 flood_fill_patches Step 7). TWIN_DEBUG=1 enables the
    // `[twin-oracle]` lines that assertions #1 + #2 parse.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    // Mirror spotlight_f0044 batch composition: F0044 + F0045 + R0092. Per
    // canary §3 the high `unpaired_count` (31/37/36) appears in the R0092 trio
    // (numbered b#5/b#6/b#7 in canary's overall sequence numbering). This test
    // asserts the contract across the entire 7-boolean batch.
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

    let n_invocations = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    let max_collision = max_twin_oracle_field(&stderr, "collision_count");
    eprintln!(
        "[pr-y20-test] flood_fill_patches invocations across F0044+F0045+R0092: {} \
         (canary §3 expects 7 = F0044 b#1 + F0045 b#2-4 + R0092 b#5-7)",
        n_invocations
    );
    eprintln!(
        "[pr-y20-test] max [twin-oracle] unpaired_count across invocations: {:?}",
        max_unpaired
    );
    eprintln!(
        "[pr-y20-test] max [twin-oracle] collision_count across invocations: {:?}",
        max_collision
    );
    for (case_id, r) in cases.iter().zip(results.iter()) {
        eprintln!(
            "[pr-y20-test] {} status={:?} detail={}",
            case_id, r.status, r.detail
        );
    }

    // ── Assertion 1: max unpaired_count across ALL invocations == 0 (spec §2 I1 — Yang §4.4.2 NMM directional-symmetry)
    //
    // Per brief deliverable #2: this is the load-bearing RED→GREEN gate. F0044
    // batch is 98% NMM per canary §3; post-NMM-fix the legitimate non-manifold
    // HEs get `twin=None` (paper-faithful per Yang §4.4.2) and are NOT counted
    // as unpaired by the post-pairing oracle.
    //
    // SPEC-VS-BRIEF GAP (flagged for team-lead): spec §5 expects post-fix
    // 1+1+0 (max=1) due to 2 MISSING residual canonical pairs in R0092
    // (banked PR-Y21+ per spec §7 anti-scope). The brief asks for max==0.
    // Following the brief's contract; if implementer-x ships the NMM fix
    // cleanly but max=1 persists, the residual is the spec §7 banked MISSING
    // follow-on (degenerate triangles in R0092: vertex repeats producing
    // self-loop edges; per canary §3 sub-mechanism distribution).
    let unpaired = max_unpaired.unwrap_or_else(|| {
        panic!(
            "[pr-y20-test] no `[twin-oracle] unpaired_count=` line in F0044 batch stderr. \
             TWIN_DEBUG=1 gate failed or pipeline aborted before flood_fill_patches. \
             Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        unpaired, 0,
        "[pr-y20-test] PR-Y20-MODE-A spec §2 I1 violation (Yang §4.4.2 NMM \
         directional-symmetry mandate): expected MAX `[twin-oracle] unpaired_count=0` \
         across all {} flood_fill_patches invocations on F0044+F0045+R0092 batch \
         post-NMM-fix, got max unpaired_count={}. \
         Pre-fix baseline (canary §3 cohort table): F0044 batch has 7 booleans \
         with unpaired summaries 0/0/0/0/31/37/36, post-pairing oracle reports \
         max≈46. Of the 104 Mode A `[]` arm cases in the trailing 3 booleans, \
         **98% (102) are NMM** (rev_in_directed_edge_to_tris=false per canary §1 \
         probe) — these are paper-faithful non-manifold meetings where Yang §4.4.2 \
         allows twin=None for the boundary half. \
         Fix per spec §3: \
         (a) `crates/kernel/src/topology/half_edge.rs:38-51` — change \
             `pub twin: HalfEdgeIdx` → `pub twin: Option<HalfEdgeIdx>` \
             (paper-extension over Cherchi 2022 §5 pure-mesh per \
             feedback_yang_brep_extension_over_cherchi_pure_mesh.md); \
         (b) `crates/kernel/src/boolean/topology_extract.rs:1241-1254` Step 7 `[]` \
             arm — emit `twin = None` (no silent fallback per feedback_yang_only.md); \
         (c) `crates/kernel/src/boolean/yang_integration.rs` validator — accept \
             `twin=None` ONLY when `directed_edge_to_tris.get(&(v1,v0)).is_none()`, \
             panic on missing-edge defect (twin=None + rev present); \
         (d) ~150 read sites + ~27 write sites across 16 files mechanical adapt \
             (spec §3 file table). \
         If max unpaired stays >0 post-fix: residual is the 2 MISSING canonical \
         pairs in R0092 (canary §3 sub-mechanism: degenerate triangles with \
         repeated vertex indices), banked PR-Y21+ per spec §7 anti-scope.",
        n_invocations, unpaired
    );

    // ── Assertion 2: max collision_count == 0 (spec §2 I2 — PR-Y19-MODE-B regression check)
    //
    // Already GREEN post-PR-Y19-MODE-B's R3 source-face routing. This guards
    // against the NMM type-system change accidentally regressing the manifold
    // 1:1 pairing path (e.g., wrapping `Some(_)` incorrectly in the `[the_one]`
    // arm or breaking the `directed_he` cross-patch dedup).
    let collision = max_twin_oracle_field(&stderr, "collision_count").unwrap_or_else(|| {
        panic!(
            "[pr-y20-test] no `[twin-oracle] collision_count=` line in F0044 batch stderr. \
             TWIN_DEBUG=1 gate failed. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    // PR-Y20-MODE-A 0d empirical amendment (implementer-x option C per brief):
    // the canary §3 baseline R0092 b#5 already had `collision_count=1`
    // pre-fix; this assertion was wrong-baselined by 0c. The 1 R0092 b#5
    // collision is pre-existing orthogonal Mode-A residual (banked PR-Y21+).
    // Per `feedback_yang_only.md` we expose it honestly rather than allow
    // the assertion to mask the post-fix outcome. Allow max ≤ 1; tighten
    // back to ≤ 0 when the R0092 b#5 collision residual is fixed.
    assert!(
        collision <= 1,
        "[pr-y20-test] PR-Y20-MODE-A spec §2 I2 regression (PR-Y19-MODE-B R3 \
         source-face routing): expected MAX `[twin-oracle] collision_count<=1` \
         across all {} flood_fill_patches invocations on F0044+F0045+R0092 batch \
         (1 = pre-existing R0092 b#5 collision, banked PR-Y21+), got max \
         collision_count={}. A value >1 here means the type change at \
         `topology/half_edge.rs:38-51` regressed the manifold 1:1 pairing \
         path — likely a missed `Some(_)` wrap in the `[the_one]` arm of \
         `topology_extract.rs:1241-1254` or a downstream consumer expecting \
         `HalfEdgeIdx(0)` as sentinel rather than `None`.",
        n_invocations,
        collision
    );

    // ── Assertion 3: case-level GREEN (per spec §5 + canary §5 self-canaried recommendation)
    //
    // Spec §5 expectations for F0044 batch post-NMM-fix:
    //   - F0044 b#1 (paired=68 unpaired=0 today): may STAY Failed at downstream
    //     watertight_mesh / outward_normals (NOT a Mode A target; out of scope
    //     for PR-Y20). Counted toward "any Passed".
    //   - F0045 b#2-4 (paired=117/165/230 unpaired=0 today): unrelated to Mode A;
    //     stays at watertight_mesh failure; out of scope.
    //   - R0092 b#5-7 (the canary §3 NMM cluster): expected to improve significantly.
    //     b#7 is 100% NMM (36/36); spec §5 says "likely Passed" post-fix. b#5+b#6
    //     have 1 MISSING residual each (banked PR-Y21+ per spec §7).
    //
    // GREEN contract: ≥1 case in the batch reaches Passed status. RED today: 0/3.
    let passed_count = results
        .iter()
        .filter(|r| r.status == AssayStatus::Passed)
        .count();
    assert!(
        passed_count >= 1,
        "[pr-y20-test] PR-Y20-MODE-A spec §5 case-level GREEN contract violation: \
         expected ≥1 case in F0044+F0045+R0092 batch to reach Status:Passed post-fix, \
         got 0/3 Passed. Per canary §3 + spec §5, R0092 b#7 (the 100% NMM boolean \
         with 36/36 NMM cases — paired=182 unpaired=36 ambiguous=0 today) is the \
         load-bearing target: when all 36 NMM HEs flip to `twin=None`, R0092 b#7 \
         flips Passed and the case-level test goes GREEN. \
         If assertions #1 + #2 pass but this fails, the NMM fix is correctly \
         emitting `twin=None` but downstream consumers (validator, retess, \
         brep_assembly) panic or produce wrong results on `twin=None` inputs — \
         per spec §8 no-fallback discipline, the failure path is informative \
         (informative panic with `manifold-context: <reason>`), not a silent \
         pass. Per spec §10 the residual case-level Failed status with \
         assertions #1+#2 GREEN suggests a downstream layer (post-flood-fill \
         retessellation or normals/Euler in F0050-class) — bank PR-Y21+. \
         Per-case detail: F0044={:?}/{} F0045={:?}/{} R0092={:?}/{}",
        results[0].status,
        results[0].detail,
        results[1].status,
        results[1].detail,
        results[2].status,
        results[2].detail,
    );
}
