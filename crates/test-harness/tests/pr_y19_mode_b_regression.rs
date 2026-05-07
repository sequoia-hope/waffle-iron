//! PR-Y19-MODE-B — RED-phase regression test for cross-patch directed-edge
//! ownership routing in `topology_extract.rs::flood_fill_patches` Step 6.
//!
//! ## RED PHASE STATUS
//!
//! Expected to fail on current `main` (post-PR-Y17-COPLANAR). Becomes GREEN
//! when implementer-w lands the R3 source-face-ownership routing fix at
//! `crates/kernel/src/boolean/topology_extract.rs:751-826` per
//! `specs/yang_pr_y19_mode_b.md` §3 + §4.
//!
//! ## Spec contract under test
//!
//! Spec §2 invariants:
//! - **I1 (Yang §4.4.2 1:1 mandate):** every canonical directed edge maps to
//!   exactly one half-edge → `directed_he[k].len() == 1` for every key.
//!   Production proxy: `[twin-oracle] collision_count == 0` (a non-singleton
//!   directed_he key produces two distinct B-Rep `Edge` indices for the same
//!   canonical undirected vertex pair, which is exactly what
//!   `topology_extract.rs:1228-1236` measures).
//! - **I2 (Twin-pairing exactly-one):** every forward HE has exactly one reverse.
//!   Production proxy: `[twin-oracle] unpaired_count == 0` (the
//!   `he.twin.twin == self` symmetric invariant fails iff a HE was left
//!   unpaired or paired asymmetrically).
//!
//! ## Pre-fix empirical baseline (canary memo §1)
//!
//! `YANG_BOOLEAN=1 spotlight_f0020` on current main emits:
//!
//! ```text
//! [twin-oracle] unpaired_count=28
//! [twin-oracle] collision_count=1
//! [A15.6] half_edge[16].twin = 0 but twin.twin = 31 (expected 16)
//! ```
//!
//! Canary §3 cohort table: F0030's failing boolean exhibits the same B2
//! mechanism (3 reverse candidates from 3 distinct B-Rep faces for directed
//! HE `(5 → 6)`).
//!
//! Spec §3 picks **R3 source-face ownership** as the routing rule; spec §6
//! test plan names this test `pr_y19_mode_b_directed_he_singleton`. Note the
//! spec-vs-brief naming gap: the brief refers to a `canon_to_brep_invariant`
//! test (vestigial from the pre-canary plan that assumed B1 / L940 collapse).
//! Canary §2 ruled out B1 (`canon_to_brep_size == unique_positions == 36` on
//! F0020); the load-bearing invariant is on the cross-patch dedup at L765,
//! detected via the downstream `[twin-oracle]` signals. This test follows the
//! spec name + signal.
//!
//! ## How to run
//!
//! ```text
//! cargo test -p test-harness --test pr_y19_mode_b_regression -- \
//!     --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the test performs process-global
//! stderr FD redirection and `set_var` on env vars; parallel execution would
//! race those operations. Mirrors `pr_y17_coplanar_regression.rs`.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;
use test_harness::assay::scoring::AssayStatus;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Process-global FD swap: redirect stderr → tempfile, run `f`, restore,
/// return `(f's return value, captured stderr as lossy UTF-8)`. Safe only
/// with `--test-threads=1`. Mirrors `pr_y17_coplanar_regression.rs`.
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
/// stderr. F0020 has multiple `flood_fill_patches` invocations (3 sequential
/// extrudes); the failing boolean is one of them, the others may pass with
/// zero counts. Per spec §6 GREEN contract, EVERY invocation must report
/// zero — equivalently, the max across all invocations must be zero. Returns
/// `None` if no such line exists.
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
/// Used in failure messages to report multi-invocation context.
fn count_twin_oracle_lines(stderr: &str, key: &str) -> usize {
    let needle = format!("[twin-oracle] {}=", key);
    stderr.lines().filter(|l| l.starts_with(&needle)).count()
}

/// PR-Y19-MODE-B spec §6: assert F0020's cross-patch directed-edge ownership
/// routing produces a 1:1 canonical↔BRep mapping (Yang §4.4.2 mandate).
///
/// Spec-aligned name: `pr_y19_mode_b_directed_he_singleton` per spec §6
/// table. (Brief calls it `canon_to_brep_invariant` — vestigial; canary §2
/// ruled out the canon_to_brep mechanism.)
///
/// Asserts in priority order so the FIRST failure gives implementer-w the
/// most useful diagnostic:
///
/// 1. **Detection baseline (RED today, GREEN post-fix):**
///    `[twin-oracle] collision_count == 0`. A non-zero collision_count is
///    the production proxy for spec §2 I1 violation: two distinct B-Rep
///    `Edge` entries got built for the same canonical undirected vertex
///    pair, which happens iff the directed_he map at L956 had ≥2 HEs
///    accumulated under one `(BrepVIdx, BrepVIdx)` key. Per canary §1 Probe
///    2, F0020 has 10 such non-singleton keys → `collision_count=1` on main.
///    Per spec §3 the R3 routing fix at L765 region drives this to 0.
///
/// 2. **Twin-pairing symmetric closure (RED today, GREEN post-fix):**
///    `[twin-oracle] unpaired_count == 0`. Spec §2 I2 violation: forward
///    HEs without a unique reverse. Per canary §1, F0020 has
///    `unpaired_count=28` on main (9 ambiguous × 2 reverses + 1 unpaired
///    fwd + 9 ambig fwd that landed `[]`). Per spec §3 the same R3 routing
///    fix drives this to 0 (it is the same root cause as #1).
///
/// 3. **Case-level GREEN (RED today, GREEN post-fix):** the `AssayResult`
///    status is `Passed`. Today F0020 reports
///    `Status: Failed, Detail: ...half_edge[16].twin = 0 but twin.twin = 31...`
///    (canary §1 Probe 2's pairing summary surfaces as the validator panic
///    in the user-visible error). Post-fix the auto-union completes and the
///    case passes.
#[test]
#[ignore]
fn pr_y19_mode_b_directed_he_singleton() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 routes through the Yang pipeline (the only path that
    // exercises Stage 4 flood_fill_patches Step 6). TWIN_DEBUG=1 enables
    // the `[twin-oracle]` lines that assertions #1 and #2 parse.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    let dir_owned = dir.to_path_buf();
    let (result, stderr) = capture_stderr(move || run_single_case(&dir_owned, "F0020", true));
    let r = result.expect("F0020 must exist in corpus — regenerate via assay_gen");

    let n_invocations = count_twin_oracle_lines(&stderr, "unpaired_count");
    let max_unpaired = max_twin_oracle_field(&stderr, "unpaired_count");
    let max_collision = max_twin_oracle_field(&stderr, "collision_count");
    eprintln!(
        "[pr-y19-test] flood_fill_patches invocations: {} (F0020 has 3 sequential extrudes; \
         each non-degenerate boolean fires the oracle once)",
        n_invocations
    );
    eprintln!(
        "[pr-y19-test] max [twin-oracle] unpaired_count across invocations: {:?}",
        max_unpaired
    );
    eprintln!(
        "[pr-y19-test] max [twin-oracle] collision_count across invocations: {:?}",
        max_collision
    );
    eprintln!(
        "[pr-y19-test] case status={:?} detail={}",
        r.status, r.detail
    );

    // ── Assertion 1: max collision_count across ALL invocations == 0 (spec §2 I1 — Yang §4.4.2 1:1 mandate)
    //
    // F0020 has 3 sequential extrudes; flood_fill_patches fires once per
    // non-degenerate boolean. The failing boolean (canary §1: Extrude 2)
    // emits `collision_count=1`; the others may emit 0. Spec §2 I1's GREEN
    // contract is "every directed_he key has exactly 1 HE" — equivalently
    // every flood_fill invocation reports 0 — equivalently the MAX across
    // all invocations is 0.
    let collision = max_collision.unwrap_or_else(|| {
        panic!(
            "[pr-y19-test] no `[twin-oracle] collision_count=` line in F0020 stderr. \
             TWIN_DEBUG=1 gate failed or pipeline aborted before flood_fill_patches. \
             Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        collision, 0,
        "[pr-y19-test] PR-Y19-MODE-B spec §2 I1 violation (Yang §4.4.2 1:1 \
         canonical↔BRep mandate): expected MAX `[twin-oracle] collision_count=0` \
         across all {} flood_fill_patches invocations on F0020 post-fix, got \
         max collision_count={}. \
         Root cause (canary §1 Probe 2 + canary §2 verdict B2): the same canonical \
         directed edge `(v0 → v1)` is emitted by ≥2 patches sourced from different \
         B-Rep faces (e.g. canary §1 Probe 3: `(23 → 21)` from \
         `mesh_A FaceIdx(3)` AND `mesh_B FaceIdx(2)`). The per-patch \
         `seen: BTreeSet<(usize,usize)>` at topology_extract.rs:753 dedups \
         intra-patch but allows cross-patch duplicates → directed_he map at L956 \
         accumulates ≥2 HEs per key → twin-pairing creates two B-Rep `Edge` \
         entries for one canonical undirected pair → collision_count > 0. \
         Fix per spec §3: R3 source-face ownership routing using \
         `directed_edge_to_tris` + `tri_to_patch` to assign each canonical \
         directed edge to exactly one owner patch, with deterministic R1-style \
         (mesh_id, face_idx, patch_index) lex tie-breaker. Spec §4 implementation \
         site: pre-pass at L751-L771 building `edge_owner: BTreeMap<(usize,usize),usize>`.",
        n_invocations, collision
    );

    // ── Assertion 2: max unpaired_count across ALL invocations == 0 (spec §2 I2 — twin-pairing exactly-one)
    let unpaired = max_twin_oracle_field(&stderr, "unpaired_count").unwrap_or_else(|| {
        panic!(
            "[pr-y19-test] no `[twin-oracle] unpaired_count=` line in F0020 stderr. \
             TWIN_DEBUG=1 gate failed. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    assert_eq!(
        unpaired, 0,
        "[pr-y19-test] PR-Y19-MODE-B spec §2 I2 violation (twin-pairing exactly-one): \
         expected MAX `[twin-oracle] unpaired_count=0` across all {} \
         flood_fill_patches invocations on F0020 post-fix, got max \
         unpaired_count={}. Same root cause as assertion #1 (cross-patch dedup \
         failure at L765 region). Canary §1 baseline: F0020 has \
         unpaired_count=28 on the failing boolean on main (9 ambiguous × 2 \
         reverses + 1 fwd unpaired + 9 fwd that landed `[]`). Spec §3 R3 \
         routing fix drives this to 0 along with collision_count. If only one \
         of these two counters drops to 0, the R3 routing is asymmetric \
         (likely deduping both directions to one side instead of routing fwd \
         to one patch and rev to the other) — see canary §5 'Risk to flag for \
         implementer-w' on loop-closure.",
        n_invocations, unpaired
    );

    // ── Assertion 3: case-level GREEN (the user-visible contract)
    assert_eq!(
        r.status,
        AssayStatus::Passed,
        "[pr-y19-test] F0020 case status must be Passed post-fix; got {:?}. \
         Detail: {}. Pre-fix baseline (canary §1): \
         `auto-union-failed: ...yang_boolean: result validation failed: \
         half_edge[16].twin = 0 but twin.twin = 31 (expected 16)`. \
         If assertions #1 + #2 pass but this fails, there is a residual \
         downstream defect not in scope for PR-Y19-MODE-B (e.g. F0050-class \
         normals/Euler defect, banked separately).",
        r.status,
        r.detail
    );
}
