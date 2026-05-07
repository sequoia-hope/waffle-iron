//! PR-Y17-COPLANAR — RED-phase regression test for `collect_face_loop_2d`
//! curve sampling fix (spec §3-§5).
//!
//! ## RED PHASE STATUS
//!
//! This test is **expected to fail on current `main`** (post-PR-Y16-FIX-ARCH).
//! It becomes GREEN when implementer-t lands the curve-sampling fix at
//! `crates/kernel/src/boolean/coplanar_preprocess.rs::collect_face_loop_2d`
//! (L420) per `specs/yang_pr_y17_coplanar_completion.md`.
//!
//! ## What the spec promises (§2 reference parity contract)
//!
//! 1. **Invariant 1** — `collect_face_loop_2d` emits chord polygons for
//!    non-`Linear` `CurveGeom` variants (`Circular`/`Arc`/`Elliptical`),
//!    not single-vertex degenerate polygons.
//! 2. **Invariant 2** — F0030's coplanar pair classifies as `partial_overlap`
//!    (currently both injection markers are zero because the i_overlay
//!    intersect returns 0 groups on a 1-vertex polygon).
//! 3. **Invariant 3** — The marker-set pair drives `inject_partial_overlap_mesh`
//!    which produces shared-triangulation in the overlap region, eliminating
//!    cap-plane stacking and dropping `[topo-extract]` `ambiguous` from 11 → 0.
//!
//! ## Pre-fix empirical baseline (canary memo §1-§3)
//!
//! On current main, `YANG_BOOLEAN=1 spotlight_f0030` emits:
//!
//! ```
//! [coplanar-tele] pairs=1 verts_existing=0 verts_split=0 verts_deduped_by_canon_key=0
//!                 verts_dropped=0 mef_ok=0 mef_no_loop=0 overlay_groups=0
//!                 overlay_holes_ignored=0 identical_footprint=0 partial_overlap=0
//! [yang-diag] after subdivide: tris_a=30, tris_b=28, verts=29
//! [topo-extract] summary: paired=21, unpaired=4, ambiguous=11
//! [twin-oracle] collision_count=3
//! ```
//!
//! Post-fix expectation (spec §2 invariants 2 + 3):
//! - `partial_overlap=1` (and `overlay_groups>=1`)
//! - `[topo-extract]` `ambiguous=0`
//! - `[twin-oracle]` `collision_count=0`
//! - `tris_a + tris_b` reduced from 58 (because Stage A no longer stacks
//!   redundant cap-plane tris from both operands).
//!
//! ## How to run
//!
//! ```
//! cargo test -p test-harness --test pr_y17_coplanar_regression -- \
//!     --ignored --nocapture --test-threads=1
//! ```
//!
//! `--test-threads=1` is required because the test performs process-global
//! stderr FD redirection and `set_var` on env vars; parallel execution
//! would race those operations. Mirrors the `pr_y14b_coplanar_corner_dedup`
//! invocation pattern.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use test_harness::assay::randomized_runner::run_single_case;

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Run a closure with the process's stderr file descriptor redirected to
/// a temporary file; return both the closure's return value and the
/// captured stderr bytes as a `String` (lossy UTF-8).
///
/// Process-global FD swap; safe only with `--test-threads=1`. Mirrors the
/// helper in `pr_y14b_coplanar_corner_dedup.rs`.
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

/// Find the LAST `[coplanar-tele] ...` line in stderr (last because the
/// counter is global and may surface multiple times across operations;
/// the final emit reflects the post-Stage-0 state).
fn last_coplanar_tele_line(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("[coplanar-tele] "))
}

/// Find the LAST `[topo-extract] summary: ...` line in stderr.
fn last_topo_extract_summary(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("[topo-extract] summary:"))
}

/// Find the LAST `[twin-oracle] collision_count=N` line in stderr.
fn last_twin_oracle_collision_line(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("[twin-oracle] collision_count="))
}

/// Find the LAST `[yang-diag] after subdivide: tris_a=N, tris_b=M, ...` line.
fn last_yang_subdivide_line(stderr: &str) -> Option<&str> {
    stderr
        .lines()
        .rev()
        .find(|l| l.starts_with("[yang-diag] after subdivide:"))
}

/// Parse `<key>=<usize>` out of a whitespace-separated token list.
/// Tokens may end with a comma which is stripped before parsing.
fn parse_usize_field(line: &str, key: &str) -> Option<usize> {
    for tok in line.split_whitespace() {
        let trimmed = tok.trim_end_matches(',');
        if let Some(rest) = trimmed.strip_prefix(key) {
            if let Some(val) = rest.strip_prefix('=') {
                return val.parse().ok();
            }
        }
    }
    None
}

/// PR-Y17-COPLANAR §3 RED-phase regression: assert F0030's coplanar
/// preprocessing completes through `inject_partial_overlap_mesh` and
/// drives downstream twin-pairing to collision-free.
///
/// Asserts (in order; first failure dominates):
/// 1. **Detection baseline**: `[coplanar-tele] pairs=1` (already GREEN
///    today; canaries this remains true post-fix).
/// 2. **Marker set** (RED today): `[coplanar-tele] partial_overlap=1`.
///    Currently `partial_overlap=0` because `collect_face_loop_2d`
///    produces a 1-vertex degenerate polygon for the cylinder cap edge,
///    causing i_overlay's Intersect to return 0 groups, which triggers
///    the silent `continue` at coplanar_preprocess.rs:264 BEFORE the
///    marker-setting line. (Spec §3 anchor.)
/// 3. **Stage A cap-plane redundancy reduced** (RED today):
///    `[yang-diag] after subdivide: tris_a + tris_b < 58`. Current main:
///    tris_a=30 + tris_b=28 = 58. Post-fix: shared overlap region produces
///    identical sampling (Yang §4.5.5 "share identical sampling points")
///    so the redundant 8 B cap-tris should not appear; expected total
///    around 50 or fewer. (Spec §2 Invariant 3.)
/// 4. **Twin-pairing collision-free** (RED today):
///    `[topo-extract] summary: ... ambiguous=0`. Current main:
///    `ambiguous=11`. Spec §2 Invariant 3: post-fix contract is `ambiguous=0`.
/// 5. **Twin-oracle clean** (RED today): `[twin-oracle] collision_count=0`.
///    Current main: `collision_count=3`. Sub-assertion of #4; reported for
///    diagnostic clarity (matches the user-facing failure mode the
///    spotlight reports today).
///
/// On current main, this test fails on assertion #2 (the marker is `=0`).
/// Failure message names the expected vs observed value to aid debugging.
#[test]
#[ignore]
fn pr_y17_coplanar_curve_sampling_red_phase() {
    let dir = Path::new(ASSAY_DIR);
    assert!(
        dir.exists(),
        "Assay corpus not generated yet at {ASSAY_DIR} — generate via assay_gen first"
    );

    // YANG_BOOLEAN=1 routes through the Yang pipeline (else the kernel
    // falls back to the deprecated S-H clipping path, which never
    // exercises coplanar_preprocess). TWIN_DEBUG=1 enables the
    // `[topo-extract]` ambiguous/unpaired detail and `[twin-oracle]`
    // collision diagnostics that assertions #4 and #5 parse.
    std::env::set_var("YANG_BOOLEAN", "1");
    std::env::set_var("TWIN_DEBUG", "1");

    let dir_owned = dir.to_path_buf();
    let (result, stderr) = capture_stderr(move || run_single_case(&dir_owned, "F0030", true));
    let _r = result.expect("F0030 must exist in corpus");

    // Echo a small slice of captured stderr on test-author console to ease
    // first-fail debugging (we replay only the load-bearing lines).
    if let Some(line) = last_coplanar_tele_line(&stderr) {
        eprintln!("[pr-y17-test] {line}");
    }
    if let Some(line) = last_yang_subdivide_line(&stderr) {
        eprintln!("[pr-y17-test] {line}");
    }
    if let Some(line) = last_topo_extract_summary(&stderr) {
        eprintln!("[pr-y17-test] {line}");
    }
    if let Some(line) = last_twin_oracle_collision_line(&stderr) {
        eprintln!("[pr-y17-test] {line}");
    }

    // ── Assertion 1: detection baseline (GREEN today; spec §2 Invariant 2 (b))
    let coplanar_line = last_coplanar_tele_line(&stderr).unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] no `[coplanar-tele]` line in F0030 stderr. The Yang \
             pipeline did not exercise coplanar_preprocess — check YANG_BOOLEAN=1 \
             gate and yang_integration.rs:703-738 call site. Stderr tail:\n{}",
            stderr.lines().rev().take(40).collect::<Vec<_>>().join("\n")
        )
    });
    let pairs = parse_usize_field(coplanar_line, "pairs").unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] cannot parse `pairs=N` from coplanar-tele line: {}",
            coplanar_line
        )
    });
    assert_eq!(
        pairs, 1,
        "[pr-y17-test] detection baseline broken: expected `[coplanar-tele] pairs=1` for F0030 \
         (one cap-plane coplanar pair per canary memo §1), got `pairs={}`. \
         Full line: {}",
        pairs, coplanar_line
    );

    // ── Assertion 2: partial_overlap marker set (RED today; spec §2 Invariant 2 (c))
    let partial_overlap =
        parse_usize_field(coplanar_line, "partial_overlap").unwrap_or_else(|| {
            panic!(
                "[pr-y17-test] cannot parse `partial_overlap=N` from coplanar-tele line: {}",
                coplanar_line
            )
        });
    assert_eq!(
        partial_overlap, 1,
        "[pr-y17-test] PR-Y17-COPLANAR §3 fix not landed: expected \
         `[coplanar-tele] partial_overlap=1` for F0030 (Yang §4.5.5 partial-overlap \
         anti-parallel injection), got `partial_overlap={}`. \
         Root cause (canary memo §2): `collect_face_loop_2d` produces a 1-vertex \
         polygon for the cylinder cap's circular edge, so i_overlay returns 0 groups \
         and `split_brep_for_coplanar_pairs` short-circuits at L264 before the marker \
         is set. Fix per spec §4: sample non-Linear CurveGeom variants into chord \
         polygons. Full line: {}",
        partial_overlap, coplanar_line
    );

    // ── Assertion 3: Stage A cap-plane redundancy reduced (RED today; spec §2 Invariant 3 (b/c))
    let subdivide_line = last_yang_subdivide_line(&stderr).unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] no `[yang-diag] after subdivide:` line in F0030 stderr. \
             Pipeline aborted before Stage 1? Stderr tail:\n{}",
            stderr.lines().rev().take(20).collect::<Vec<_>>().join("\n")
        )
    });
    let tris_a = parse_usize_field(subdivide_line, "tris_a").unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] cannot parse `tris_a=N` from subdivide line: {}",
            subdivide_line
        )
    });
    let tris_b = parse_usize_field(subdivide_line, "tris_b").unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] cannot parse `tris_b=N` from subdivide line: {}",
            subdivide_line
        )
    });
    let total_subdiv = tris_a + tris_b;
    assert!(
        total_subdiv < 58,
        "[pr-y17-test] PR-Y17-COPLANAR §3 fix not landed: Stage A cap-plane stacking \
         persists for F0030. Expected `tris_a + tris_b < 58` post-fix (per Yang §4.5.5 \
         shared-sampling contract: redundant 8 B cap-tris should not co-exist with the \
         20 A cap-tris in the overlap region), got `tris_a={} + tris_b={} = {}`. \
         Canary memo §3: current main has 28 redundant cap-plane tris (20 A + 8 B). \
         Full line: {}",
        tris_a,
        tris_b,
        total_subdiv,
        subdivide_line
    );

    // ── Assertion 4: twin-pairing ambiguous count = 0 (RED today; spec §2 Invariant 3 (a/c))
    let topo_summary = last_topo_extract_summary(&stderr).unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] no `[topo-extract] summary:` line in F0030 stderr. \
             TWIN_DEBUG=1 gate failed? Stderr tail:\n{}",
            stderr.lines().rev().take(20).collect::<Vec<_>>().join("\n")
        )
    });
    let ambiguous = parse_usize_field(topo_summary, "ambiguous").unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] cannot parse `ambiguous=N` from topo-extract line: {}",
            topo_summary
        )
    });
    assert_eq!(
        ambiguous, 0,
        "[pr-y17-test] PR-Y17-COPLANAR §3 fix not landed: twin-pairing collision count \
         not zero for F0030. Expected `[topo-extract] summary ambiguous=0` post-fix \
         (per spec §2 Invariant 3 contract), got `ambiguous={}`. Canary memo §3: \
         current main `paired=21, unpaired=4, ambiguous=11`. Full line: {}",
        ambiguous, topo_summary
    );

    // ── Assertion 5: twin-oracle collision_count = 0 (RED today; diagnostic clarity)
    let twin_line = last_twin_oracle_collision_line(&stderr).unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] no `[twin-oracle] collision_count=` line in F0030 stderr. \
             TWIN_DEBUG=1 gate failed? Stderr tail:\n{}",
            stderr.lines().rev().take(20).collect::<Vec<_>>().join("\n")
        )
    });
    let collision_count = parse_usize_field(twin_line, "collision_count").unwrap_or_else(|| {
        panic!(
            "[pr-y17-test] cannot parse `collision_count=N` from twin-oracle line: {}",
            twin_line
        )
    });
    assert_eq!(
        collision_count, 0,
        "[pr-y17-test] PR-Y17-COPLANAR §3 fix not landed: twin-oracle reports cluster \
         offenders for F0030. Expected `[twin-oracle] collision_count=0` post-fix \
         (downstream consequence of Invariant 3 holding), got `collision_count={}`. \
         Canary memo §3: current main `collision_count=3`. This is the failure mode \
         the spotlight surfaces as `half_edge[N].twin = ...` validator panic. \
         Full line: {}",
        collision_count, twin_line
    );
}
