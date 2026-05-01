//! PR13 — Trim-loop chaining bijective fix: red-phase tests.
//!
//! ## Role and FIP §1 P5 separation
//!
//! Authored by `agent-test` on team `yang-trim-loop-chaining-pr13`. Per FIP
//! §1 P5, agent-test is distinct from agent-impl (T4) and adversary (T5).
//! Implementer MUST NOT modify this file.
//!
//! ## Spec scope (Approach A — edge-canonical reference)
//!
//! Per `specs/yang_trim_loop_chaining.md` §8 (commit `d13bcf0`), PR13
//! finalized the fix as **Approach A — edge-canonical reference applied
//! to `flood_fill_patches::Step 6`** at
//! `crates/kernel/src/boolean/topology_extract.rs:607-684`. The PR12
//! archaeological anchor that named `extract_trim_boundaries` was wrong
//! per T2's diagnosis (`docs/audits/pr13_trim_loop_diagnostic.md` §2):
//! `extract_trim_boundaries` is test-only code never invoked by the
//! production Yang pipeline. Production uses `flood_fill_patches`.
//!
//! T2's empirical findings (`docs/audits/pr13_trim_loop_diagnostic.md`):
//!
//! - 9/9 violations have `byte_eq=true / reciprocal=false` — both
//!   adjacent faces emit the same directed edge along the shared B-Rep
//!   edge (PR12 anchor confirmed at the higher abstraction level).
//! - Two clusters share one root cause: `flood_fill_patches::Step 6`
//!   has no inter-face direction consistency.
//!   - **D1** (5/9, R0021 dominant): duplicate boundary edges +
//!     LIFO `outgoing.pop()` at `n_cands=3` branch points (line 664).
//!   - **D2** (4/9, R0020 + R0021 #0,#1): no branch points fire; bug
//!     is the START-vertex pick via `adj.iter().find(...)` over a
//!     `HashMap` (line 651-654, line 644).
//! - PR12 missed `flood_fill_patches`: line 644 HashMap is a residual
//!   PR12 non-determinism source (R0021 NB count flaps 5/6/7 across
//!   runs).
//!
//! ## Six red-phase tests
//!
//! 1. **`synthetic_two_face_bijective_baseline`** — positive sanity:
//!    two coplanar rectangles sharing one edge with byte-identical
//!    reciprocal directed mesh edges (Yang §4.1.1 contract holds).
//!    Asserts `Ok(())` (zero non-bijective pairs). Passes today; must
//!    continue to pass post-PR13 — false-positive guard.
//!
//! 2. **`synthetic_same_direction_anti_fixture_caught`** — sensitivity
//!    proof: same two rectangles but face B's triangulation winding is
//!    flipped so that face B emits the shared-edge boundary in the SAME
//!    direction as face A. Per Yang §4.1.1 this is the exact failure
//!    mode PR13 fixes (`un_a[i] == un_b[i]` byte-identical). Asserts
//!    oracle reports `≥1` non-bijective pair with at least one
//!    byte-identical-non-reciprocal sample. Passes today (oracle is
//!    sensitive to the synthetic defect); demonstrates the oracle catches
//!    the exact failure mode PR13 targets.
//!
//! 3. **`r0020_corpus_regression_red_phase`** — corpus anchor: load
//!    R0020.waffle through `with_yang_oracle_capture` and assert the
//!    Stage 1 (`BijectiveFacePairOracle`) verdict is NOT
//!    `ContractViolated`. Currently FAILS per T2 (R0020 reports
//!    `A 2 pair(s) of 19`). Post-PR13 (Approach A on
//!    `flood_fill_patches::Step 6`): passes. `#[ignore]`-gated.
//!
//! 4. **`r0021_corpus_regression_red_phase`** — corpus anchor: same as
//!    Test 3 for R0021 (T2: `A 7 pair(s) of 23`). Currently FAILS.
//!    Post-PR13: passes.
//!
//! 5. **`cluster_x_cascade_resolution_red_phase`** — Cluster X cascade
//!    hypothesis verification: per PR12 T2 §5 + PR13 spec §8 success
//!    criterion, R0020 + R0021 fire S1 + S2 + S6 simultaneously
//!    (`. X X . . X`). PR13's S1 fix should ALSO drop S6 (twin
//!    symmetry restored as `build_result_brep` finds both sides of every
//!    directed edge). Asserts BOTH `Stage1Bijective` AND `Stage6Assembly`
//!    verdicts are NOT violated for R0020 and R0021. Currently FAILS
//!    (S1 + S6 both fire). Post-PR13: ideally passes. `#[ignore]`-gated.
//!
//! 6. **`r0021_determinism_stability_red_phase`** — determinism anchor:
//!    per T2 §6, R0021's NB-pair count flaps 5/6/7 across runs because
//!    `flood_fill_patches::Step 6` line 644 uses a `HashMap` adjacency
//!    map. Approach A (per spec §8 step 1) replaces this with a
//!    `BTreeMap`. Run R0021 through the bijective oracle 3 times
//!    in-process and assert ALL invocations report the SAME Stage 1
//!    verdict and SAME message body. Currently FAILS (R0021 NB count
//!    varies 5/6/7 across runs per T2 §6). Post-PR13: passes.
//!    `#[ignore]`-gated (3× corpus probe; ~90 s wall-clock).
//!
//! ## Visibility and constraints
//!
//! - `kernel::tessellation::bijective::check_face_pair_bijective` is
//!   `pub`, reachable from test-harness directly.
//! - `BijectivityReport`, `NonBijectivePair` are `pub`.
//! - `BijectiveFacePairOracle` and `TwinSymmetryOracle` are
//!   `pub(crate)` (kernel-internal). Tests 3-6 use the public
//!   `kernel::diagnostics::with_yang_oracle_capture` wrapper, mirroring
//!   `pr11_per_patch_labeling.rs` and `pr12_stage1_bijective.rs`.
//! - Tests 1, 2 use polygon-soup mode (empty `TopoArena`) — same idiom
//!   as `bijective.rs::oracle_detects_t_junction_sensitivity` and
//!   `pr12_stage1_bijective.rs::synthetic_*`.
//!
//! ## Refs
//!
//! - `specs/yang_trim_loop_chaining.md` §8 (Approach A finalized).
//! - `docs/audits/pr13_trim_loop_diagnostic.md` §2-§7 (T2 diagnosis).
//! - `crates/test-harness/tests/pr13_trim_loop_diagnostic.rs` (T2 probe;
//!   `with_yang_oracle_capture` + per-feature re-tessellation pattern).
//! - `crates/test-harness/tests/pr12_stage1_bijective.rs` (PR12 reds —
//!   Tests 3+4 already target R0020/R0021 from a different abstraction
//!   angle; this file's Tests 3+4 are anchored to `flood_fill_patches`
//!   (the corrected production anchor) per spec §8 amendment).
//! - `crates/kernel/src/boolean/topology_extract.rs::flood_fill_patches`
//!   (line 351; Step 6 line 607-684 — the fix surface).
//! - `crates/kernel/src/tessellation/bijective.rs::check_face_pair_bijective`
//!   (the oracle being validated).
//! - Yang 2025 §4.1.1 (bijective tessellation contract).
//! - Cherchi 2022 §3 (input precondition: manifold, watertight, no
//!   self-intersections).
//! - Memory: `feedback_yang_only.md`, `feedback_no_last_bug.md`,
//!   `feedback_no_regression_chasing.md`,
//!   `feedback_validate_against_corpus.md`,
//!   `feedback_anchor_before_fix.md`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use kernel::diagnostics::{with_yang_oracle_capture, OracleRunSummary, ViolationKind, YangStage};
use kernel::tessellation::bijective::{check_face_pair_bijective, BijectivityReport};
use kernel::topology::arena::TopoArena;
use kernel::topology::half_edge::FaceIdx;
use kernel::types::{FaceRange, KernelId, RenderMesh};
use wasm_bridge::messages::UiToEngine;
use wasm_bridge::{dispatch, EngineState};

const ASSAY_DIR: &str = "../../app/tests/cases/assay";

/// Per-case timeout for end-to-end pipeline runs. Mirrors
/// `pr12_stage1_bijective.rs::PER_CASE_TIMEOUT`.
const PER_CASE_TIMEOUT: Duration = Duration::from_secs(60);

#[inline]
fn pos_key(p: [f64; 3]) -> [u64; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

// ── Synthetic fixture builders ──────────────────────────────────────────

/// Build two coplanar rectangles A and B that share the segment from
/// `(1,0,0)` to `(1,1,0)` using byte-identical reciprocal mesh edges.
///
/// Per Yang 2025 §4.1.1, this discretization is bijective: face A's
/// directed edge `(1,0,0)→(1,1,0)` appears as `(1,1,0)→(1,0,0)` on
/// face B. Polygon-soup mode (empty `TopoArena`) — same idiom as
/// `pr12_stage1_bijective.rs::build_synthetic_bijective_pair` and
/// `bijective.rs::oracle_detects_t_junction_sensitivity`.
fn build_synthetic_bijective_pair() -> (RenderMesh, BTreeMap<u64, FaceIdx>, TopoArena) {
    let vertices: Vec<f32> = vec![
        // face A
        0.0, 0.0, 0.0, // 0
        1.0, 0.0, 0.0, // 1 — shared with B[4]
        1.0, 1.0, 0.0, // 2 — shared with B[5]
        0.0, 1.0, 0.0, // 3
        // face B
        1.0, 0.0, 0.0, // 4 — byte-identical to A[1]
        1.0, 1.0, 0.0, // 5 — byte-identical to A[2]
        2.0, 0.0, 0.0, // 6
        2.0, 1.0, 0.0, // 7
    ];
    let normals: Vec<f32> = (0..8).flat_map(|_| [0.0f32, 0.0, 1.0]).collect();
    // Face A CCW on +z: (0,1,2), (0,2,3) — emits 1→2 = (1,0,0)→(1,1,0).
    // Face B CCW on +z: (4,6,5), (5,6,7) — emits 5→4 = (1,1,0)→(1,0,0)
    // (reciprocal of A's 1→2). Yang §4.1.1 holds.
    let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 5, 6, 7];
    let face_ranges = vec![
        FaceRange {
            face_id: KernelId(100),
            start_index: 0,
            end_index: 6,
        },
        FaceRange {
            face_id: KernelId(200),
            start_index: 6,
            end_index: 12,
        },
    ];
    let mesh = RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    };
    let mut face_map = BTreeMap::new();
    face_map.insert(100u64, FaceIdx(0));
    face_map.insert(200u64, FaceIdx(1));
    let arena = TopoArena::new(); // empty → polygon-soup mode
    (mesh, face_map, arena)
}

/// Build an anti-fixture exhibiting the EXACT failure mode PR13 fixes:
/// two coplanar rectangles sharing the segment `(1,0,0)–(1,1,0)`, but
/// face B's triangulation winding is flipped so face B's boundary on the
/// shared edge emits the SAME direction `(1,0,0)→(1,1,0)` as face A.
///
/// This is the synthetic analog of T2's R0020/R0021 finding
/// (`docs/audits/pr13_trim_loop_diagnostic.md` §3): every NB pair has
/// `un_a[i] == un_b[i]` byte-identically (`byte_eq=true,
/// reciprocal=false`). Both faces walk the shared edge in the same
/// direction; per Yang §4.1.1, twin half-edges should walk it
/// reciprocally.
///
/// Construction: face B uses indices `(4,5,6)` and `(4,6,7)` instead of
/// `(4,6,5),(5,6,7)`. The first tri's edge `4→5` is exactly
/// `(1,0,0)→(1,1,0)` — same as face A's `1→2`. Face B's effective
/// normal points to `-z` (winding flipped), but the oracle runs on raw
/// directed mesh edges — it does not consult per-face normals — so the
/// directional violation surfaces directly as one or more
/// byte-identical-non-reciprocal samples in `BijectivityReport`.
fn build_synthetic_same_direction_anti_fixture() -> (RenderMesh, BTreeMap<u64, FaceIdx>, TopoArena)
{
    let vertices: Vec<f32> = vec![
        // face A
        0.0, 0.0, 0.0, // 0
        1.0, 0.0, 0.0, // 1 — shared with B[4] byte-identically
        1.0, 1.0, 0.0, // 2 — shared with B[5] byte-identically
        0.0, 1.0, 0.0, // 3
        // face B (winding deliberately reversed for shared edge)
        1.0, 0.0, 0.0, // 4 — byte-identical to A[1]
        1.0, 1.0, 0.0, // 5 — byte-identical to A[2]
        2.0, 1.0, 0.0, // 6
        2.0, 0.0, 0.0, // 7
    ];
    let normals: Vec<f32> = (0..8).flat_map(|_| [0.0f32, 0.0, 1.0]).collect();
    // Face A CCW on +z: (0,1,2), (0,2,3) — emits 1→2 = (1,0,0)→(1,1,0).
    // Face B reversed winding: (4,5,6), (4,6,7) — face B emits 4→5 =
    // (1,0,0)→(1,1,0), the SAME direction as face A. The reverse
    // direction (1,1,0)→(1,0,0) does NOT appear on either face along
    // the shared edge → both face A and face B's `un_*` lists contain
    // the byte-identical (1,0,0)→(1,1,0) sample.
    let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    let face_ranges = vec![
        FaceRange {
            face_id: KernelId(100),
            start_index: 0,
            end_index: 6,
        },
        FaceRange {
            face_id: KernelId(200),
            start_index: 6,
            end_index: 12,
        },
    ];
    let mesh = RenderMesh {
        vertices,
        normals,
        indices,
        face_ranges,
    };
    let mut face_map = BTreeMap::new();
    face_map.insert(100u64, FaceIdx(0));
    face_map.insert(200u64, FaceIdx(1));
    let arena = TopoArena::new(); // empty → polygon-soup mode
    (mesh, face_map, arena)
}

// ── Shared corpus probe (Tests 3-6) ────────────────────────────────────

/// Run one corpus case end-to-end with snapshot capture and return the
/// `OracleRunSummary`. Mirrors `pr12_stage1_bijective.rs::run_corpus_case_summary`
/// but without that module's PR12-step-2-specific commentary.
fn run_corpus_case_summary(case_id: &str) -> Option<OracleRunSummary> {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr13-trim] Assay corpus not present at {ASSAY_DIR}; cannot \
             run red-phase probe (regenerate via `cargo run -p test-harness \
             --bin assay_gen`)."
        );
        return None;
    }
    let waffle_path = dir.join(format!("{case_id}.waffle"));
    if !waffle_path.exists() {
        eprintln!("[pr13-trim] {case_id}.waffle missing at {waffle_path:?}");
        return None;
    }

    let case_id_owned = case_id.to_string();
    let (tx, rx) = mpsc::channel::<Option<OracleRunSummary>>();
    thread::spawn(move || {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let waffle_json = std::fs::read_to_string(&waffle_path).ok()?;
            std::env::set_var("YANG_BOOLEAN", "1");
            let id_for_capture = case_id_owned.clone();
            let (summary, _err_log) = with_yang_oracle_capture(&id_for_capture, move || {
                let mut state = EngineState::new();
                let mut kernel_inst = kernel::WaffleKernel::new();
                let _ = dispatch(
                    &mut state,
                    UiToEngine::LoadProject { data: waffle_json },
                    &mut kernel_inst,
                );
                state.engine.errors.clone()
            });
            Some(summary)
        }));
        let _ = tx.send(res.unwrap_or(None));
    });

    rx.recv_timeout(PER_CASE_TIMEOUT).ok().flatten()
}

/// Returns the per-stage verdict tuple `(violated, message)` from a
/// summary for the requested stage. `violated == true` iff the oracle at
/// `stage` reported `ContractViolated`.
fn stage_verdict(summary: &OracleRunSummary, stage: YangStage) -> (bool, Option<String>) {
    summary
        .per_oracle
        .iter()
        .find(|v| v.stage == stage)
        .map(|v| match &v.violation {
            None => (false, None),
            Some(viol) => (
                matches!(viol.kind, ViolationKind::ContractViolated),
                Some(viol.message.clone()),
            ),
        })
        .unwrap_or((false, None))
}

/// Convenience: dump the full per-stage verdict matrix for a corpus run,
/// per `feedback_anchor_before_fix.md` — every test prints the matrix
/// before asserting so a failing run carries the diagnostic context
/// inline (mirrors `pr12_stage1_bijective.rs` test 3/4 anchor pattern).
fn dump_anchor(case_id: &str, summary: &OracleRunSummary) {
    eprintln!(
        "[pr13-trim] [ANCHOR] {case_id} first_failing_stage={:?} per_oracle.len={}",
        summary.first_failing_stage,
        summary.per_oracle.len(),
    );
    for v in &summary.per_oracle {
        eprintln!(
            "  {case_id} stage={:?} oracle={} kind={:?} message={:?}",
            v.stage,
            v.oracle_name,
            v.violation.as_ref().map(|x| x.kind.clone()),
            v.violation.as_ref().map(|x| x.message.clone()),
        );
    }
}

// ── Test 1 — Synthetic two-face bijective baseline (positive sanity) ───

/// Yang 2025 §4.1.1: two coplanar rectangles sharing one segment with
/// byte-identical reciprocal directed mesh edges. The oracle MUST return
/// zero non-bijective pairs.
///
/// Passes today (oracle correct on the simple bijective case). Continues
/// to pass post-PR13 — guards against the Approach A fix (canonical-edge
/// reference at branch points + BTreeMap adjacency) introducing a
/// false-positive on a clean fixture.
#[test]
fn synthetic_two_face_bijective_baseline() {
    let (mesh, face_map, arena) = build_synthetic_bijective_pair();

    // [ANCHOR] per `feedback_anchor_before_fix.md`: confirm fixture
    // shape before assertion.
    eprintln!(
        "[pr13-trim-test1] [ANCHOR] synthetic bijective fixture: \
         {} verts, {} indices ({} tris), {} face_ranges, arena empty={}",
        mesh.vertices.len() / 3,
        mesh.indices.len(),
        mesh.indices.len() / 3,
        mesh.face_ranges.len(),
        arena.faces.is_empty(),
    );
    assert_eq!(mesh.face_ranges.len(), 2, "fixture must have 2 face_ranges");
    assert_eq!(
        mesh.indices.len(),
        12,
        "fixture must have 12 indices (4 tris)"
    );

    let report: BijectivityReport = check_face_pair_bijective(&mesh, &face_map, &arena);

    eprintln!(
        "[pr13-trim-test1] report: total_pairs_examined={}, bijective_pairs={}, \
         non_bijective_pairs.len={}",
        report.total_pairs_examined,
        report.bijective_pairs,
        report.non_bijective_pairs.len(),
    );

    // Yang §4.1.1: reciprocal byte-identical edges → zero non-bijective
    // pairs. Polygon-soup mode requires ≥2 byte-identical shared
    // vertices; both shared corners (1,0,0) and (1,1,0) qualify, so the
    // oracle examines exactly 1 pair.
    assert_eq!(
        report.total_pairs_examined, 1,
        "[pr13-trim-test1] expected oracle to examine exactly 1 face pair \
         (the two rectangles sharing edge x=1); got {}",
        report.total_pairs_examined
    );
    assert_eq!(
        report.bijective_pairs, 1,
        "[pr13-trim-test1] expected bijective_pairs=1 (Yang §4.1.1 holds \
         on byte-identical reciprocal edges)"
    );
    assert!(
        report.is_bijective(),
        "[pr13-trim-test1] FALSE POSITIVE: oracle flagged {} non-bijective \
         pairs on a clean bijective fixture (Yang §4.1.1 holds). \
         Diagnostic samples: {:?}",
        report.non_bijective_pairs.len(),
        report
            .non_bijective_pairs
            .iter()
            .map(|p| (p.unmatched_a_count, p.unmatched_b_count))
            .collect::<Vec<_>>()
    );
}

// ── Test 2 — Synthetic same-direction anti-fixture caught ──────────────

/// Yang 2025 §4.1.1 sensitivity proof for the EXACT failure mode PR13
/// fixes: two coplanar rectangles sharing one segment, but face B's
/// triangulation winds the shared edge in the SAME direction as face A
/// (not the reciprocal direction Yang demands). Per T2's diagnosis
/// (`docs/audits/pr13_trim_loop_diagnostic.md` §3), all 9 R0020/R0021
/// violations exhibit `byte_eq=true / reciprocal=false`: both adjacent
/// faces emit the SAME directed edge along the shared B-Rep edge.
///
/// This synthetic anti-fixture reproduces that exact pattern in
/// polygon-soup mode. The oracle must catch it (≥1 non-bijective pair
/// with at least one byte-identical-non-reciprocal sample).
///
/// Passes today (oracle is sensitive to the failure mode PR13 targets).
/// Continues to pass post-PR13 — guards against Approach A introducing
/// a false-negative that would let the load-bearing failure pattern
/// slip through.
#[test]
fn synthetic_same_direction_anti_fixture_caught() {
    let (mesh, face_map, arena) = build_synthetic_same_direction_anti_fixture();

    // [ANCHOR]: confirm fixture shape before assertion.
    eprintln!(
        "[pr13-trim-test2] [ANCHOR] same-direction anti-fixture: \
         {} verts, {} indices ({} tris), {} face_ranges",
        mesh.vertices.len() / 3,
        mesh.indices.len(),
        mesh.indices.len() / 3,
        mesh.face_ranges.len(),
    );
    assert_eq!(mesh.face_ranges.len(), 2);
    assert_eq!(
        mesh.indices.len(),
        12,
        "anti-fixture must have 12 indices (2 tris per face)"
    );

    let report = check_face_pair_bijective(&mesh, &face_map, &arena);

    eprintln!(
        "[pr13-trim-test2] report: total_pairs_examined={}, bijective_pairs={}, \
         non_bijective_pairs.len={}",
        report.total_pairs_examined,
        report.bijective_pairs,
        report.non_bijective_pairs.len(),
    );

    assert_eq!(
        report.total_pairs_examined, 1,
        "[pr13-trim-test2] expected oracle to examine exactly 1 face pair; \
         got {}",
        report.total_pairs_examined
    );
    assert!(
        !report.is_bijective(),
        "[pr13-trim-test2] FALSE NEGATIVE: oracle missed the deliberate \
         same-direction shared-edge anti-fixture (this is the EXACT failure \
         mode PR13 targets — `un_a[i] == un_b[i]` byte-identical per T2 \
         §3). Post-PR13 fix MUST preserve this sensitivity."
    );
    assert_eq!(
        report.non_bijective_pairs.len(),
        1,
        "[pr13-trim-test2] expected exactly 1 non-bijective pair; got {}",
        report.non_bijective_pairs.len()
    );
    let pair = &report.non_bijective_pairs[0];
    assert!(
        pair.unmatched_a_count > 0,
        "[pr13-trim-test2] face A must report ≥1 unmatched directed edge \
         on the shared boundary; got unmatched_a_count={}",
        pair.unmatched_a_count
    );
    assert!(
        pair.unmatched_b_count > 0,
        "[pr13-trim-test2] face B must report ≥1 unmatched directed edge \
         on the shared boundary; got unmatched_b_count={}",
        pair.unmatched_b_count
    );

    // Sensitivity check: at least one sample pair (un_a[i], un_b[j]) must
    // demonstrate the byte-identical-non-reciprocal pattern PR13 targets.
    // Per T2 §3 this is the load-bearing diagnostic on R0020/R0021.
    let mut found_byte_eq_non_recip = false;
    for (p_a, q_a) in &pair.sample_unmatched_a {
        for (p_b, q_b) in &pair.sample_unmatched_b {
            let byte_eq = pos_key(*p_a) == pos_key(*p_b) && pos_key(*q_a) == pos_key(*q_b);
            let recip = pos_key(*p_a) == pos_key(*q_b) && pos_key(*q_a) == pos_key(*p_b);
            if byte_eq && !recip {
                found_byte_eq_non_recip = true;
                eprintln!(
                    "[pr13-trim-test2] byte-identical-non-reciprocal sample \
                     confirmed: a=({:?}→{:?}) b=({:?}→{:?})",
                    p_a, q_a, p_b, q_b,
                );
                break;
            }
        }
        if found_byte_eq_non_recip {
            break;
        }
    }
    assert!(
        found_byte_eq_non_recip,
        "[pr13-trim-test2] anti-fixture must surface ≥1 \
         `byte_eq=true / reciprocal=false` sample (the PR13 failure-mode \
         signature per T2 §3). Got samples a={:?}, b={:?}",
        pair.sample_unmatched_a, pair.sample_unmatched_b,
    );
}

// ── Test 3 — R0020 corpus regression (red-phase) ───────────────────────

/// R0020 corpus regression. T2 (`docs/audits/pr13_trim_loop_diagnostic.md`
/// §3): R0020 has 2 NB pairs, both in operand A, both
/// `byte_eq=true / reciprocal=false`. Per spec §8 cluster D2, R0020's
/// defect mechanism is the START-vertex pick over the non-deterministic
/// `HashMap` adjacency in `flood_fill_patches::Step 6` (line 644 +
/// line 651-654). Approach A (BTreeMap + canonical-edge reference) must
/// fix it.
///
/// **Red-phase verdict** (T2 + PR12 baseline): R0020 fires Stage 1
/// `ContractViolated` with `A 2 pair(s) of 19, B 0 pair(s) of 2`.
///
/// **Post-PR13 expectation**: agent-impl fixes
/// `flood_fill_patches::Step 6` per spec §8; R0020 Stage 1 verdict
/// flips to `Ok`.
///
/// Note: this test is intentionally distinct from
/// `pr12_stage1_bijective.rs::r0020_cluster_x_non_coplanar_red_phase`.
/// The PR12 test was anchored against a presumed tessellation defect at
/// the bijective oracle layer; PR13's anchor is the production
/// `flood_fill_patches::Step 6` per T2's anchor correction. Both reds
/// flip green together if Approach A is correct — but they MEASURE the
/// same observable (Stage 1 oracle verdict on R0020), so duplicate
/// coverage is acceptable per spec §8 success criterion.
///
/// `#[ignore]`-gated per FIP §4.4 + PR11/PR12 precedent (long-running
/// pipeline-driven probe with ~30 s timeout).
#[test]
#[ignore = "Long-running corpus probe (~30 s); runs via --include-ignored. \
Red-phase: R0020 fires Stage 1 ContractViolated per T2 \
('A 2 pair(s) of 19'). Post-PR13 (Approach A on flood_fill_patches::Step 6 \
per spec §8): passes."]
fn r0020_corpus_regression_red_phase() {
    let summary = match run_corpus_case_summary("R0020") {
        Some(s) => s,
        None => panic!(
            "[pr13-trim-test3] R0020 corpus probe failed (missing fixture, \
             timeout, or load error). Cannot demonstrate red-phase. \
             Regenerate corpus via `cargo run -p test-harness --bin \
             assay_gen` if needed."
        ),
    };

    dump_anchor("R0020", &summary);

    let (s1_violated, s1_message) = stage_verdict(&summary, YangStage::Stage1Bijective);
    let s1_msg = s1_message.as_deref().unwrap_or("(no s1 violation)");

    // Yang §4.1.1: post-PR13 the bijective contract must hold on R0020.
    assert!(
        !s1_violated,
        "[pr13-trim-test3] Stage 1 ContractViolated on R0020 (Yang §4.1.1 \
         bijective contract): {}. Per spec §8 (Approach A), agent-impl \
         must fix `flood_fill_patches::Step 6` (line 607-684) so adjacent \
         patches' boundary chaining respects canonical edge orientation \
         (Cherchi 2022 §3 manifold/watertight precondition). T2 §7 D2 \
         classifies R0020 as the START-vertex non-determinism cluster.",
        s1_msg,
    );
}

// ── Test 4 — R0021 corpus regression (red-phase) ───────────────────────

/// R0021 corpus regression. T2 (`docs/audits/pr13_trim_loop_diagnostic.md`
/// §3): R0021 has 6-7 NB pairs (count flaps across runs per §6), all in
/// operand A, all `byte_eq=true / reciprocal=false`. Per spec §8
/// cluster classification, R0021 mixes D1 (5/9 — duplicate boundary
/// edges + LIFO `outgoing.pop()` at `n_cands=3` branches) and D2 (2/9 —
/// START-vertex non-determinism).
///
/// **Red-phase verdict** (T2 + PR12 baseline): R0021 fires Stage 1
/// `ContractViolated` with `A 7 pair(s) of 23, B 0 pair(s) of 2` (count
/// flaps 5/6/7).
///
/// **Post-PR13 expectation**: same Approach A fix as R0020 resolves both
/// D1 (canonical-direction picker over LIFO pop at line 664) and D2
/// (BTreeMap + canonical start-vertex picker at line 644+651). R0021
/// Stage 1 verdict flips to `Ok`.
#[test]
#[ignore = "Long-running corpus probe (~30 s); runs via --include-ignored. \
Red-phase: R0021 fires Stage 1 ContractViolated per T2 \
('A 7 pair(s) of 23'; count flaps 5/6/7 across runs per T2 §6). Post-PR13 \
(Approach A on flood_fill_patches::Step 6): passes."]
fn r0021_corpus_regression_red_phase() {
    let summary = match run_corpus_case_summary("R0021") {
        Some(s) => s,
        None => panic!(
            "[pr13-trim-test4] R0021 corpus probe failed (missing fixture, \
             timeout, or load error). Cannot demonstrate red-phase."
        ),
    };

    dump_anchor("R0021", &summary);

    let (s1_violated, s1_message) = stage_verdict(&summary, YangStage::Stage1Bijective);
    let s1_msg = s1_message.as_deref().unwrap_or("(no s1 violation)");

    assert!(
        !s1_violated,
        "[pr13-trim-test4] Stage 1 ContractViolated on R0021 (Yang §4.1.1 \
         bijective contract): {}. Per spec §8, Approach A on \
         `flood_fill_patches::Step 6` must address both D1 (LIFO pick over \
         duplicate edges at line 664) and D2 (HashMap-driven start-vertex \
         non-determinism at line 644+651). Both clusters share one root \
         cause: Step 6 has no inter-face direction consistency.",
        s1_msg,
    );
}

// ── Test 5 — Cluster X cascade resolution (S1 + S6) ────────────────────

/// Per PR12 T2 §5 + PR13 spec §8 success criterion, R0020 and R0021 are
/// Cluster X non-coplanar cases (`. X X . . X` per
/// `pr12_stage1_diagnostic.md`): they fire Stage 1 + Stage 2 + Stage 6
/// simultaneously. The PR13 spec hypothesizes that S1 fix on
/// `flood_fill_patches::Step 6` will ALSO drop S6 (cascade resolution):
/// when adjacent patches' loops walk the shared edge reciprocally, twin
/// pairing in `build_result_brep` finds both halves of every directed
/// edge, restoring `twin.twin == self`.
///
/// **Test asserts**: post-PR13, BOTH `Stage1Bijective` AND
/// `Stage6Assembly` verdicts are NOT violated for R0020 and R0021.
///
/// **Honest framing per `feedback_no_last_bug.md`** (spec §9 honesty
/// clause #8): the cascade is a HYPOTHESIS, not a guarantee. If PR13
/// lands the S1 fix but S6 still fires, the cascade hypothesis is
/// partially wrong, and a deeper defect lurks in S6 that was masked by
/// S1 first-fail. T5 adversary must report cascade outcome honestly. As
/// a TEST, this asserts the strongest expectation (S1 + S6 both Ok); if
/// it fails post-fix in CI the failure surfaces the cascade gap, which
/// is the desired signal.
#[test]
#[ignore = "Long-running cluster-X cascade probe (~60 s, 2 cases × 30 s); \
runs via --include-ignored. Red-phase: R0020 + R0021 fire S1 + S6 \
simultaneously (Cluster X per PR12 T2 §5). Post-PR13 (cascade hypothesis): \
both stages flip to Ok."]
fn cluster_x_cascade_resolution_red_phase() {
    for case_id in ["R0020", "R0021"] {
        let summary = match run_corpus_case_summary(case_id) {
            Some(s) => s,
            None => panic!(
                "[pr13-trim-test5] {case_id} corpus probe failed (missing \
                 fixture, timeout, or load error). Cannot demonstrate \
                 cascade red-phase."
            ),
        };

        dump_anchor(case_id, &summary);

        let (s1_violated, s1_msg) = stage_verdict(&summary, YangStage::Stage1Bijective);
        let (s6_violated, s6_msg) = stage_verdict(&summary, YangStage::Stage6Assembly);

        eprintln!(
            "[pr13-trim-test5] {case_id} cascade verdicts: \
             S1_violated={s1_violated} S1_msg={:?} \
             S6_violated={s6_violated} S6_msg={:?}",
            s1_msg, s6_msg,
        );

        assert!(
            !s1_violated,
            "[pr13-trim-test5] {case_id} Stage 1 still violated post-fix: \
             {:?}. PR13's Approach A on `flood_fill_patches::Step 6` did \
             not resolve the bijective contract violation.",
            s1_msg.as_deref().unwrap_or("(no msg)"),
        );
        assert!(
            !s6_violated,
            "[pr13-trim-test5] {case_id} Stage 6 (twin symmetry) still \
             violated post-fix: {:?}. The Cluster X cascade hypothesis \
             (PR13 spec §8 success criterion) predicted S6 would also \
             flip to Ok via twin pairing in `build_result_brep` finding \
             both halves of every directed edge once Stage 1 produces \
             reciprocal walks. If this assertion fires, a deeper Stage 6 \
             defect was masked by Stage 1 first-fail and PR14+ must \
             investigate (per spec §9 honesty clause #8).",
            s6_msg.as_deref().unwrap_or("(no msg)"),
        );
    }
}

// ── Test 6 — R0021 determinism stability ───────────────────────────────

/// Per T2 §6 (`docs/audits/pr13_trim_loop_diagnostic.md`), R0021's NB
/// pair count varies 5/6/7 across consecutive runs because
/// `flood_fill_patches::Step 6` line 644 uses
/// `HashMap<usize, Vec<...>>` whose iteration order depends on Rust
/// `HashMap`'s RandomState. PR12's Step 1 widening converted four
/// `boolean/` files but missed `flood_fill_patches`; line 644 is a
/// confirmed PR12-residual non-determinism source.
///
/// PR13 spec §8 (Approach A, step 1) replaces this `HashMap` with a
/// `BTreeMap` so iteration is structurally deterministic. Post-PR13,
/// R0021 must produce identical Stage 1 verdicts AND identical Stage 1
/// messages across in-process repeated runs.
///
/// **Anchor**: load R0021.waffle three times within one test process.
/// Capture each run's Stage 1 verdict + verbatim message. Assert all
/// three runs agree on (a) binary verdict, (b) message body. T2 §6
/// reports R0021 NB count 5/6/7 across runs; pre-fix at least one run
/// in three should diverge with high probability. The Step 1 fix
/// (HashMap → BTreeMap) makes determinism structural.
///
/// **Honest framing** (spec §9 honesty clause #6 +
/// `feedback_no_last_bug.md`): probabilistic-flap tests can pass pre-fix
/// by chance if RandomState seeds identically across calls within one
/// process. Per T2 §6 the flap fires within-process. If this test
/// passes pre-fix, it's a known false-negative — fix is still warranted
/// because the structural guarantee (BTreeMap iteration is
/// deterministic) removes the flap mechanism entirely.
///
/// `#[ignore]`-gated (3× corpus probe; ~90 s wall-clock).
#[test]
#[ignore = "Long-running determinism probe (~90 s, 3 invocations of R0021); \
runs via --include-ignored. Red-phase: NB pair count flaps 5/6/7 across \
runs per T2 §6 (HashMap on flood_fill_patches::Step 6 line 644). Post-PR13 \
(BTreeMap conversion in Approach A step 1): identical messages across \
runs."]
fn r0021_determinism_stability_red_phase() {
    eprintln!(
        "[pr13-trim-test6] [ANCHOR] determinism probe on R0021 \
         (NB count flaps 5/6/7 per T2 §6)"
    );

    const REPEATS: usize = 3;
    let mut verdicts: Vec<(bool, Option<String>)> = Vec::with_capacity(REPEATS);
    for i in 0..REPEATS {
        let summary = match run_corpus_case_summary("R0021") {
            Some(s) => s,
            None => panic!(
                "[pr13-trim-test6] R0021 corpus probe failed on run {i} \
                 (missing fixture, timeout, or load error). Cannot \
                 demonstrate determinism red-phase."
            ),
        };
        let v = stage_verdict(&summary, YangStage::Stage1Bijective);
        eprintln!(
            "[pr13-trim-test6] run {i}: s1_violated={} s1_message={:?} \
             first_failing_stage={:?}",
            v.0,
            v.1.as_deref().unwrap_or("(none)"),
            summary.first_failing_stage,
        );
        verdicts.push(v);
    }

    // Per spec §8 step 1: all three runs MUST produce identical Stage 1
    // verdicts AND identical Stage 1 messages. BTreeMap's structural
    // determinism is the post-fix guarantee.
    for i in 1..REPEATS {
        assert_eq!(
            verdicts[0].0, verdicts[i].0,
            "[pr13-trim-test6] R0021 Stage 1 binary verdict flap: run 0 \
             violated={}, run {i} violated={}. Per spec §8 step 1, \
             `flood_fill_patches::Step 6` line 644 `HashMap` must be \
             replaced with `BTreeMap` so the bijective oracle's view of \
             this op is structurally deterministic (Yang §4.1.1: \
             byte-identical contract demands stable evaluation).",
            verdicts[0].0, verdicts[i].0,
        );
        assert_eq!(
            verdicts[0].1, verdicts[i].1,
            "[pr13-trim-test6] R0021 Stage 1 message flap: run 0 = {:?}, \
             run {i} = {:?}. Counts within the Stage 1 message flap \
             because the patch chaining order varies with RandomState. \
             Step 1 fix: BTreeMap iteration → identical messages.",
            verdicts[0].1, verdicts[i].1,
        );
    }

    eprintln!(
        "[pr13-trim-test6] determinism contract held: 3 runs of R0021 \
         produced identical Stage 1 verdicts. Stable verdict was: \
         violated={} message={:?}",
        verdicts[0].0,
        verdicts[0].1.as_deref().unwrap_or("(none)"),
    );
}
