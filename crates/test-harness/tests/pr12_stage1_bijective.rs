//! PR12 — Yang Stage 1 bijective face-pair contract: red-phase tests.
//!
//! ## Role and FIP §1 P5 separation
//!
//! Authored by `agent-test` on team `yang-stage1-bijective-pr12`. Per FIP §1
//! P5, agent-test is distinct from agent-impl (T4) and adversary (T5). These
//! tests encode the spec invariants from `specs/yang_stage1_bijective.md` §8
//! (Branch II two-step scope finalized at commit `036c262`). Implementer
//! MUST NOT modify this file.
//!
//! ## Spec scope (Branch II two-step)
//!
//! Per `specs/yang_stage1_bijective.md` §8 (commit `036c262`):
//!
//! - **Step 1** — Determinism fix: replace `HashMap<DirEdgeKey, ...>` with
//!   `BTreeMap` (or sorted iteration) in
//!   `crates/kernel/src/tessellation/bijective.rs::face_boundary_directed_edges`
//!   and related per-face boundary surfaces. T2 §4 measured 4 of 15 cases
//!   (R0014, R0034, R0046, F0076) flapping S1 fire/no-fire across runs
//!   because Rust's `HashMap` RandomState reorders directed-edge iteration.
//!   The bijective oracle's matching logic itself is deterministic given
//!   ordered input, so stabilising the iteration removes the flap signal.
//!
//! - **Step 2** — R0020 + R0021 Cluster X non-coplanar tessellation defect:
//!   T2 §5 isolated R0020/R0021 as the cleanest red-phase target — both
//!   fire S1+S2+S6 in PR12, neither fires S0 OracleStub, neither flaps,
//!   and both have small per-operand non-bijective ratios on operand A
//!   (R0020 "A 2 pair(s) of 19", R0021 "A 7 pair(s) of 23"). Pure
//!   tessellation defect independent of coplanar preprocessing.
//!
//! ## Five red-phase tests
//!
//! 1. **`synthetic_two_rectangle_bijective_baseline`** — positive sanity:
//!    two coplanar rectangles sharing one B-Rep edge with byte-identical
//!    reciprocal directed mesh edges (Yang §4.1.1 contract holds). Asserts
//!    `Ok(())` (zero unmatched pairs). Passes today; must continue to pass
//!    post-PR12 — false-positive guard.
//!
//! 2. **`synthetic_t_junction_anti_fixture_caught`** — sensitivity proof:
//!    same two rectangles but face B inserts an extra midpoint that face A
//!    does not have. Per Yang §4.1.1 the segmentation difference is a
//!    bijective contract violation. Asserts oracle reports
//!    `non_bijective_pairs.len() == 1` with at least one unmatched directed
//!    edge. Passes today (oracle catches the synthetic defect). Must
//!    continue to pass post-PR12 — sensitivity guard.
//!
//! 3. **`r0020_cluster_x_non_coplanar_red_phase`** — Step 2 anchor: load
//!    R0020.waffle through `with_yang_oracle_capture` and assert the
//!    Stage 1 (`BijectiveFacePairOracle`) verdict is NOT `ContractViolated`.
//!    Currently FAILS per T2 canonical run (R0020 reports `. X X . . X`
//!    with S1 message "operand A 2 pair(s) of 19, operand B 0 pair(s) of
//!    2"). Post-Step-2 fix: passes. `#[ignore]`-gated (long-running corpus
//!    probe with ~30 s per-case timeout).
//!
//! 4. **`r0021_cluster_x_non_coplanar_red_phase`** — Step 2 anchor: same
//!    pattern as Test 3 but for R0021 (T2 canonical: `. X X . . X` with
//!    S1 message "operand A 7 pair(s) of 23, operand B 0 pair(s) of 2").
//!    Currently FAILS; post-Step-2: passes.
//!
//! 5. **`oracle_determinism_across_repeated_invocations_red_phase`** —
//!    Step 1 anchor: invoke `check_face_pair_bijective` on the synthetic
//!    T-junction fixture from Test 2 four times in succession. Assert all
//!    invocations return identical verdicts AND identical
//!    `BijectivityReport` content (same `total_pairs_examined`,
//!    `bijective_pairs`, `non_bijective_pairs.len()`, and identical sorted
//!    samples). Currently FAILS per T2 §4 (HashMap iteration order
//!    non-determinism causes counts and sample positions to flap across
//!    consecutive invocations on the same fixture, per the
//!    `pr4_r0033_t_junction_diagnosis.rs` flap note); post-Step-1: passes.
//!
//! ## Visibility and constraints
//!
//! - `kernel::tessellation::bijective::check_face_pair_bijective` is `pub`,
//!   reachable from test-harness directly.
//! - `BijectivityReport`, `NonBijectivePair` are `pub`.
//! - `BijectiveFacePairOracle` is `pub(crate)` (kernel-internal). Tests 3
//!   and 4 use the public `kernel::diagnostics::with_yang_oracle_capture`
//!   wrapper, mirroring `pr11_per_patch_labeling.rs` and
//!   `pr12_stage1_diagnostic.rs`.
//! - Tests 1, 2, 5 use polygon-soup mode (empty `TopoArena`) — same
//!   pattern as the in-kernel `oracle_detects_t_junction_sensitivity` test
//!   at `bijective.rs:1064-1127`. No kernel-internal access required.
//!
//! ## Refs
//!
//! - `specs/yang_stage1_bijective.md` §8 (Branch II finalized scope).
//! - `docs/audits/pr12_stage1_diagnostic.md` (T2 cluster classification +
//!   per-case verbatim diagnostic).
//! - `crates/test-harness/tests/pr12_stage1_diagnostic.rs` (oracle
//!   invocation pattern).
//! - `crates/test-harness/tests/pr11_per_patch_labeling.rs` (`#[ignore]`-
//!   gating idiom for long-running corpus probes).
//! - `crates/kernel/src/tessellation/bijective.rs` (oracle being validated).
//! - Yang 2025 §4.1.1 (bijective tessellation contract).
//! - Cherchi 2022 §3 (input precondition: manifold, watertight, no
//!   self-intersections).
//! - Memory: `feedback_yang_only.md`, `feedback_no_last_bug.md`,
//!   `feedback_no_regression_chasing.md`, `feedback_validate_against_corpus.md`,
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
/// `pr12_stage1_diagnostic.rs::PER_CASE_TIMEOUT`.
const PER_CASE_TIMEOUT: Duration = Duration::from_secs(60);

// ── Synthetic fixture builders ──────────────────────────────────────────

/// Build two coplanar rectangles A and B that share one common edge from
/// (1,0,0) to (1,1,0) using byte-identical reciprocal mesh edges. Per Yang
/// 2025 §4.1.1 the discretization is bijective on this pair: face A's
/// directed edge (1,0,0)→(1,1,0) appears as (1,1,0)→(1,0,0) on face B.
///
/// Polygon-soup mode (empty arena) — same idiom as
/// `oracle_detects_t_junction_sensitivity` at `bijective.rs:1064-1127`.
fn build_synthetic_bijective_pair() -> (RenderMesh, BTreeMap<u64, FaceIdx>, TopoArena) {
    // Face A: rectangle 0..1 × 0..1, vertices 0..3.
    // Face B: rectangle 1..2 × 0..1, vertices 4..7. Shared corner positions
    // (1,0,0) and (1,1,0) are byte-identical between the two faces.
    let vertices: Vec<f32> = vec![
        // Face A
        0.0, 0.0, 0.0, // 0
        1.0, 0.0, 0.0, // 1 — shared with B[4]
        1.0, 1.0, 0.0, // 2 — shared with B[5]
        0.0, 1.0, 0.0, // 3
        // Face B
        1.0, 0.0, 0.0, // 4 — byte-identical to A[1]
        1.0, 1.0, 0.0, // 5 — byte-identical to A[2]
        2.0, 0.0, 0.0, // 6
        2.0, 1.0, 0.0, // 7
    ];
    let normals: Vec<f32> = (0..8).flat_map(|_| [0.0f32, 0.0, 1.0]).collect();
    // Face A: 2 tris CCW on +z: (0,1,2) and (0,2,3) — boundary directed
    // edge (1,0,0)→(1,1,0) emerges from tri (0,1,2) as 1→2.
    // Face B: 2 tris CCW on +z: (4,6,5) and (5,6,7) — boundary directed
    // edge (1,1,0)→(1,0,0) emerges from tri (4,6,5) as 5→4 (i.e.
    // (1,1,0)→(1,0,0)). Reciprocal of A's boundary edge.
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

/// Build the T-junction anti-fixture from `bijective.rs:1064-1127`. Face A
/// is a 0..1 × 0..1 rectangle; face B sits flush against A on the right
/// (x = 1) but encodes its left boundary with an extra midpoint at
/// (1, 0.5, 0). Per Yang §4.1.1 this is non-bijective — the segmentation
/// difference is the contract violation. Welding cannot fix it because no
/// quantization on face A's mesh introduces the missing midpoint.
fn build_synthetic_t_junction_pair() -> (RenderMesh, BTreeMap<u64, FaceIdx>, TopoArena) {
    let vertices: Vec<f32> = vec![
        // face A
        0.0, 0.0, 0.0, // 0
        1.0, 0.0, 0.0, // 1
        1.0, 1.0, 0.0, // 2
        0.0, 1.0, 0.0, // 3
        // face B with T-junction midpoint
        1.0, 0.0, 0.0, // 4 — byte-identical to A[1]
        1.0, 0.5, 0.0, // 5 — T-junction midpoint (NOT on face A)
        1.0, 1.0, 0.0, // 6 — byte-identical to A[2]
        2.0, 0.0, 0.0, // 7
        2.0, 1.0, 0.0, // 8
    ];
    let normals: Vec<f32> = (0..9).flat_map(|_| [0.0f32, 0.0, 1.0]).collect();
    let indices: Vec<u32> = vec![
        // face A: 2 tris CCW on +z
        0, 1, 2, 0, 2, 3, // face B: 3 tris using the midpoint at vertex 5
        4, 7, 5, 5, 7, 8, 5, 8, 6,
    ];
    let face_ranges = vec![
        FaceRange {
            face_id: KernelId(100),
            start_index: 0,
            end_index: 6,
        },
        FaceRange {
            face_id: KernelId(200),
            start_index: 6,
            end_index: 6 + 9,
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

// ── Shared corpus probe (Tests 3 + 4) ───────────────────────────────────

/// Run one corpus case end-to-end with snapshot capture and return the
/// `OracleRunSummary`. Mirrors the worker pattern in
/// `pr12_stage1_diagnostic.rs::run_one_case` + the timeout pattern in
/// `pr11_per_patch_labeling.rs::run_corpus_case`.
fn run_corpus_case_summary(case_id: &str) -> Option<OracleRunSummary> {
    let dir = Path::new(ASSAY_DIR);
    if !dir.exists() {
        eprintln!(
            "[pr12-bij] Assay corpus not present at {ASSAY_DIR}; cannot run \
             red-phase probe (regenerate via `cargo run -p test-harness \
             --bin assay_gen`)."
        );
        return None;
    }
    let waffle_path = dir.join(format!("{case_id}.waffle"));
    if !waffle_path.exists() {
        eprintln!("[pr12-bij] {case_id}.waffle missing at {waffle_path:?}");
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

/// Returns the Stage 1 verdict tuple `(violated, message)` from a summary.
/// `violated == true` iff `BijectiveFacePairOracle` reported
/// `ContractViolated`. The message is the human-readable
/// "non-bijective face pairs: operand A N pair(s) of M, operand B P pair(s) of Q"
/// emitted by `pipeline_oracles.rs:276-283`.
fn stage1_verdict(summary: &OracleRunSummary) -> (bool, Option<String>) {
    summary
        .per_oracle
        .iter()
        .find(|v| v.stage == YangStage::Stage1Bijective)
        .map(|v| match &v.violation {
            None => (false, None),
            Some(viol) => (
                matches!(viol.kind, ViolationKind::ContractViolated),
                Some(viol.message.clone()),
            ),
        })
        .unwrap_or((false, None))
}

// ── Test 1 — Synthetic bijective baseline (positive sanity) ─────────────

/// Yang 2025 §4.1.1: the discretization is bijective if every B-Rep edge
/// shared by two faces emits byte-identical reciprocal directed mesh
/// edges. Two coplanar rectangles sharing one segment with matching
/// vertex pairs satisfy the contract — the oracle MUST return zero
/// non-bijective pairs.
///
/// Passes today (oracle already correct on the simple bijective case).
/// Continues to pass post-PR12 — guards against the Step 1 / Step 2 fix
/// introducing a false-positive on a clean fixture.
#[test]
fn synthetic_two_rectangle_bijective_baseline() {
    let (mesh, face_map, arena) = build_synthetic_bijective_pair();

    // [ANCHOR] per `feedback_anchor_before_fix.md`: confirm the fixture
    // shape before assertion. The synthetic mesh has 8 vertices, 4
    // triangles, 2 face_ranges; arena is empty (polygon-soup mode).
    eprintln!(
        "[pr12-bij-test1] [ANCHOR] synthetic bijective fixture: \
         {} vertices, {} indices ({} tris), {} face_ranges, arena empty={}",
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
        "[pr12-bij-test1] report: total_pairs_examined={}, bijective_pairs={}, \
         non_bijective_pairs.len={}",
        report.total_pairs_examined,
        report.bijective_pairs,
        report.non_bijective_pairs.len(),
    );

    // Yang §4.1.1: reciprocal byte-identical edges => zero non-bijective
    // pairs. Polygon-soup pair detection requires ≥2 byte-identical
    // shared vertices (per `bijective.rs::check_polygon_soup_mode`,
    // ~line 490) — both shared corners (1,0,0) and (1,1,0) qualify, so
    // the oracle examines exactly 1 pair.
    assert_eq!(
        report.total_pairs_examined, 1,
        "[pr12-bij-test1] expected oracle to examine exactly 1 face pair \
         (the two rectangles sharing edge x=1); got {}",
        report.total_pairs_examined
    );
    assert_eq!(
        report.bijective_pairs, 1,
        "[pr12-bij-test1] expected the bijective pair count = 1 (Yang \
         §4.1.1 holds on byte-identical reciprocal edges)"
    );
    assert!(
        report.is_bijective(),
        "[pr12-bij-test1] FALSE POSITIVE: oracle flagged {} non-bijective \
         pairs on a clean bijective fixture (Yang §4.1.1 contract holds \
         on this pair). Diagnostic samples: {:?}",
        report.non_bijective_pairs.len(),
        report
            .non_bijective_pairs
            .iter()
            .map(|p| (p.unmatched_a_count, p.unmatched_b_count))
            .collect::<Vec<_>>()
    );
}

// ── Test 2 — Synthetic anti-fixture (T-junction caught) ─────────────────

/// Yang 2025 §4.1.1 sensitivity proof: a hand-crafted T-junction crack
/// between two coplanar faces. Face A's right edge is one segment
/// (1,0)→(1,1); face B inserts a midpoint at (1, 0.5) splitting the
/// shared edge into two sub-segments — same positions, different
/// segmentation. Welding cannot fix this because no quantization on face
/// A's mesh introduces the midpoint. Per Cherchi 2022 §3 input
/// precondition (manifold, watertight, no self-intersections), the mesh
/// arrangement REQUIRES this contract. Oracle must catch the violation.
///
/// Passes today (oracle correctly detects the T-junction). Continues to
/// pass post-PR12 — guards against Step 1 / Step 2 introducing a
/// false-negative that would let real T-junctions slip through.
#[test]
fn synthetic_t_junction_anti_fixture_caught() {
    let (mesh, face_map, arena) = build_synthetic_t_junction_pair();

    // [ANCHOR]: confirm fixture shape before assertion.
    eprintln!(
        "[pr12-bij-test2] [ANCHOR] T-junction anti-fixture: \
         {} vertices, {} indices ({} tris), {} face_ranges",
        mesh.vertices.len() / 3,
        mesh.indices.len(),
        mesh.indices.len() / 3,
        mesh.face_ranges.len(),
    );
    assert_eq!(mesh.face_ranges.len(), 2);
    assert_eq!(
        mesh.indices.len(),
        15,
        "T-junction fixture must have 15 indices (2+3 tris)"
    );

    let report = check_face_pair_bijective(&mesh, &face_map, &arena);

    eprintln!(
        "[pr12-bij-test2] report: total_pairs_examined={}, bijective_pairs={}, \
         non_bijective_pairs.len={}",
        report.total_pairs_examined,
        report.bijective_pairs,
        report.non_bijective_pairs.len(),
    );

    assert_eq!(
        report.total_pairs_examined, 1,
        "[pr12-bij-test2] expected oracle to examine exactly 1 face pair; \
         got {}",
        report.total_pairs_examined
    );
    assert!(
        !report.is_bijective(),
        "[pr12-bij-test2] FALSE NEGATIVE: oracle missed a T-junction on a \
         deliberate anti-fixture (Yang §4.1.1 violation on segmentation \
         difference). The post-PR12 fix MUST preserve this sensitivity."
    );
    assert_eq!(
        report.non_bijective_pairs.len(),
        1,
        "[pr12-bij-test2] expected exactly 1 non-bijective pair on the \
         T-junction fixture; got {}",
        report.non_bijective_pairs.len()
    );
    let pair = &report.non_bijective_pairs[0];
    assert!(
        pair.unmatched_a_count > 0 || pair.unmatched_b_count > 0,
        "[pr12-bij-test2] T-junction must surface as ≥1 unmatched directed \
         edge; got unmatched_a={} unmatched_b={}",
        pair.unmatched_a_count,
        pair.unmatched_b_count,
    );
}

// ── Test 3 — R0020 cluster X non-coplanar (Step 2 anchor) ───────────────

/// R0020 is a Cluster X case per T2 canonical run
/// (`docs/audits/pr12_stage1_diagnostic.md` §5): fires Stage 1, Stage 2,
/// Stage 6 with no S0 OracleStub (no coplanar preprocessing involvement).
/// Pure tessellation defect — `tessellate_waffle_solid` produces an
/// operand-A mesh whose face boundaries violate Yang §4.1.1 byte-identity
/// independent of injection.
///
/// **Red-phase verdict** (T2 canonical run, commit `69664ec`):
/// `R0020 | . X X . . X | A 2 pair(s) of 19, B 0 pair(s) of 2`.
/// Stage 1 = `ContractViolated`; small ratio (2/19 ≈ 11 %) on operand A only.
///
/// **Stable**: T2 §4 measured R0020 across 4 runs — S1 `X` on every run
/// (no flap). So this red-phase test does NOT need stochastic gating.
///
/// **Post-Step-2 expectation**: agent-impl roots-causes the operand-A
/// tessellation defect (per spec §8 Step 2 anchor — likely
/// `crates/kernel/src/tessellation/mod.rs` per-face dispatch), and R0020
/// drops from S1 = `X` to S1 = `Ok` (cascade also resolves S2/S6).
///
/// `#[ignore]`-gated per FIP §4.4 + `pr12_stage1_diagnostic.rs` precedent
/// (long-running pipeline-driven probe; ≤ 60 s timeout).
#[test]
#[ignore = "Long-running corpus probe (~30 s); runs via --include-ignored. \
Red-phase fails on current code (R0020 fires Stage 1 ContractViolated per \
T2 canonical run with 'A 2 pair(s) of 19'). Post-PR12 Step 2 (R0020/R0021 \
non-coplanar tessellation defect fix per spec §8): passes."]
fn r0020_cluster_x_non_coplanar_red_phase() {
    let summary = match run_corpus_case_summary("R0020") {
        Some(s) => s,
        None => panic!(
            "[pr12-bij-test3] R0020 corpus probe failed (missing fixture, \
             timeout, or load error). Cannot demonstrate red-phase. \
             Regenerate corpus via `cargo run -p test-harness --bin assay_gen` \
             if needed."
        ),
    };

    let (violated, message) = stage1_verdict(&summary);
    let s1_msg = message.as_deref().unwrap_or("(no s1 violation)");

    // [ANCHOR] per `feedback_anchor_before_fix.md`: dump the per-stage
    // verdict tuple AND the Stage 1 message before asserting, so a
    // failing run includes the full diagnostic context (mirrors
    // `pr12_stage1_diagnostic.rs::dump_records`).
    eprintln!(
        "[pr12-bij-test3] [ANCHOR] R0020 first_failing_stage={:?} s1_violated={} \
         s1_message=`{}` per_oracle.len={}",
        summary.first_failing_stage,
        violated,
        s1_msg,
        summary.per_oracle.len(),
    );
    for v in &summary.per_oracle {
        eprintln!(
            "  R0020 stage={:?} oracle={} kind={:?} message={:?}",
            v.stage,
            v.oracle_name,
            v.violation.as_ref().map(|x| x.kind.clone()),
            v.violation.as_ref().map(|x| x.message.clone()),
        );
    }

    // Yang §4.1.1: post-Step-2 the bijective contract must hold on R0020.
    // The structural assertion is "Stage 1 oracle returns Ok" — i.e. the
    // operand-A face-pair scan finds zero unmatched directed edges
    // (equivalent to `BijectivityReport::is_bijective() == true` on both
    // operands; see `pipeline_oracles.rs:268-272`).
    assert!(
        !violated,
        "[pr12-bij-test3] Stage 1 ContractViolated on R0020 (Yang §4.1.1 \
         bijective contract): {}. Per spec §8 Step 2, agent-impl must \
         root-cause the operand-A tessellation defect such that the \
         per-face boundary directed edges along every shared B-Rep edge \
         reciprocate byte-identically (Cherchi 2022 §3 manifold/watertight \
         input precondition).",
        s1_msg,
    );
}

// ── Test 4 — R0021 cluster X non-coplanar (Step 2 anchor) ───────────────

/// R0021 is the second Cluster X non-coplanar case per T2 canonical run.
/// Same pattern as R0020 but slightly larger ratio: "A 7 pair(s) of 23"
/// (≈ 30 % on operand A only). Also stable across 4 runs (no flap) and
/// no S0 OracleStub fire.
///
/// **Red-phase verdict** (T2 canonical, `69664ec`):
/// `R0021 | . X X . . X | A 7 pair(s) of 23, B 0 pair(s) of 2`.
///
/// **Post-Step-2 expectation**: same fix as R0020 (per spec §8: "the
/// defect is operand-A-asymmetric; investigate whether solid_a's geometry
/// triggers a specific tessellation codepath"). Both R0020 and R0021 are
/// in the same cluster — one fix should resolve both.
#[test]
#[ignore = "Long-running corpus probe (~30 s); runs via --include-ignored. \
Red-phase fails on current code (R0021 fires Stage 1 ContractViolated per \
T2 canonical run with 'A 7 pair(s) of 23'). Post-PR12 Step 2 (R0020/R0021 \
non-coplanar tessellation defect fix): passes."]
fn r0021_cluster_x_non_coplanar_red_phase() {
    let summary = match run_corpus_case_summary("R0021") {
        Some(s) => s,
        None => panic!(
            "[pr12-bij-test4] R0021 corpus probe failed (missing fixture, \
             timeout, or load error). Cannot demonstrate red-phase."
        ),
    };

    let (violated, message) = stage1_verdict(&summary);
    let s1_msg = message.as_deref().unwrap_or("(no s1 violation)");

    eprintln!(
        "[pr12-bij-test4] [ANCHOR] R0021 first_failing_stage={:?} s1_violated={} \
         s1_message=`{}` per_oracle.len={}",
        summary.first_failing_stage,
        violated,
        s1_msg,
        summary.per_oracle.len(),
    );
    for v in &summary.per_oracle {
        eprintln!(
            "  R0021 stage={:?} oracle={} kind={:?} message={:?}",
            v.stage,
            v.oracle_name,
            v.violation.as_ref().map(|x| x.kind.clone()),
            v.violation.as_ref().map(|x| x.message.clone()),
        );
    }

    assert!(
        !violated,
        "[pr12-bij-test4] Stage 1 ContractViolated on R0021 (Yang §4.1.1 \
         bijective contract): {}. Per spec §8 Step 2, agent-impl must \
         root-cause the operand-A tessellation defect (same fix as R0020 — \
         both are in the Cluster X non-coplanar subset per T2 §5).",
        s1_msg,
    );
}

// ── Test 5 — Determinism stability across repeated invocations ──────────

/// Per spec §8 Step 1 (determinism fix anchor): the bijective oracle's
/// internal `face_boundary_directed_edges` and related per-face boundary
/// surfaces use `HashMap<DirEdgeKey, ...>` whose iteration order depends
/// on Rust `HashMap`'s RandomState. T2 §4 measured 4 of 15 cases
/// (R0014, R0034, R0046, F0076) flapping S1 fire/no-fire across runs;
/// counts within S1 messages also flap (e.g. R0014 reports `9 pair(s)`
/// in one run and `7 pair(s)` in another). The flap originates upstream
/// in HashMap RandomState (used by `face_boundary_directed_edges`'s
/// `count: HashMap<...>` and Cherchi arrangement vertex-merge), surfaces
/// in the `BijectivityReport` counts and sample positions.
///
/// **Anchor strategy**: load R0014.waffle (a known flap-prone case per
/// T2 §4) THREE times within the same test process via
/// `with_yang_oracle_capture`. Capture each run's Stage 1 verdict + raw
/// message string. Per spec §8 Step 1, ALL three runs must agree on:
///  (a) Stage 1 binary verdict (`ContractViolated` vs `Ok`),
///  (b) the verbatim Stage 1 message body (which encodes operand A/B
///      pair counts).
///
/// **Red-phase rationale**: per T2 §4 across 4 PR12 runs of R0014, the
/// S1 verdict alternated `X / . / X / X`. Pre-Step-1, three runs in one
/// process should produce a divergent verdict OR divergent message
/// counts on at least one of the 4 flap-prone cases with high
/// probability. The Step 1 fix (HashMap → BTreeMap in
/// `face_boundary_directed_edges`) makes the determinism guarantee
/// structural — BTreeMap is order-deterministic by definition — so all
/// three runs produce byte-identical messages.
///
/// **Honest framing per `feedback_no_last_bug.md`**: probabilistic-flap
/// tests can pass pre-fix by chance if the OS RandomState happens to
/// seed identically across the three calls within one process. Per T2
/// §4 evidence the flap fires within-process on R0014 across 4 runs in
/// the diagnostic probe. If this test passes pre-fix on R0014 in some
/// CI run, that's a known false-negative — the fix is still warranted
/// because the flap rate across the 4 corpus cases is empirically
/// non-zero. The structural guard is that BTreeMap iteration is
/// deterministic by construction, removing the flap mechanism entirely.
///
/// `#[ignore]`-gated per the same long-running pipeline-driven probe
/// idiom as Tests 3 + 4 (~3 × 30 s ≤ 90 s wall-clock). Runs via
/// `--include-ignored`.
#[test]
#[ignore = "Long-running corpus probe (~90 s, 3 invocations of R0014); \
runs via --include-ignored. Red-phase: verdict/message flap across \
invocations per T2 §4 evidence (R0014 fired X/./X/X across 4 runs). \
Post-PR12 Step 1 (HashMap → BTreeMap in face_boundary_directed_edges): \
passes — BTreeMap is structurally deterministic so all runs produce \
identical Stage 1 messages."]
fn oracle_determinism_across_repeated_invocations_red_phase() {
    eprintln!(
        "[pr12-bij-test5] [ANCHOR] determinism probe on R0014 \
         (flap-prone per T2 §4: fired X/./X/X across 4 runs)"
    );

    const REPEATS: usize = 3;
    let mut verdicts: Vec<(bool, Option<String>)> = Vec::new();
    for i in 0..REPEATS {
        let summary = match run_corpus_case_summary("R0014") {
            Some(s) => s,
            None => panic!(
                "[pr12-bij-test5] R0014 corpus probe failed on run {i} \
                 (missing fixture, timeout, or load error). Cannot \
                 demonstrate determinism red-phase."
            ),
        };
        let v = stage1_verdict(&summary);
        eprintln!(
            "[pr12-bij-test5] run {i}: s1_violated={} s1_message=`{}` \
             first_failing_stage={:?}",
            v.0,
            v.1.as_deref().unwrap_or("(none)"),
            summary.first_failing_stage,
        );
        verdicts.push(v);
    }

    // Per spec §8 Step 1: all three runs MUST produce identical Stage 1
    // verdicts AND identical Stage 1 messages. BTreeMap's structural
    // determinism is the post-fix guarantee.
    for i in 1..REPEATS {
        assert_eq!(
            verdicts[0].0, verdicts[i].0,
            "[pr12-bij-test5] R0014 Stage 1 binary verdict flap: run 0 \
             violated={}, run {i} violated={}. Per spec §8 Step 1 \
             determinism fix, the HashMap-keyed iteration in \
             `face_boundary_directed_edges` must be replaced with \
             BTreeMap or sorted iteration so the bijective oracle \
             produces structurally deterministic verdicts (Yang §4.1.1: \
             byte-identical contract demands stable evaluation).",
            verdicts[0].0, verdicts[i].0,
        );
        assert_eq!(
            verdicts[0].1, verdicts[i].1,
            "[pr12-bij-test5] R0014 Stage 1 message flap: run 0 = {:?}, \
             run {i} = {:?}. Counts within the Stage 1 violation message \
             flap because the unmatched directed-edge sets enumerated by \
             the HashMap-keyed boundary scan vary with RandomState. \
             Step 1 fix: BTreeMap iteration → identical messages.",
            verdicts[0].1, verdicts[i].1,
        );
    }

    // Per FIP §4.3 (numeric/structural assertion): the determinism check
    // above IS the structural assertion. The eprintln above logs the
    // verdict tuple for each run so post-fix validation can confirm the
    // expected stable verdict (T2 §3 shows R0014 settles to S1=Ok in 3/4
    // PR12 runs and S1=X in 1/4; whichever the post-fix path lands on,
    // the SAME outcome must repeat).
    eprintln!(
        "[pr12-bij-test5] determinism contract held: 3 runs of R0014 \
         produced identical Stage 1 verdicts. Stable verdict was: \
         violated={} message={:?}",
        verdicts[0].0,
        verdicts[0].1.as_deref().unwrap_or("(none)"),
    );

    // Belt-and-suspenders: the synthetic T-junction fixture also exercises
    // the same HashMap path; assert it produces a stable signature across
    // in-process invocations. This catches a future regression where the
    // synthetic path develops its own non-determinism source even if the
    // corpus path stabilises.
    let (mesh, face_map, arena) = build_synthetic_t_junction_pair();
    const SYN_REPEATS: usize = 3;
    let reports: Vec<BijectivityReport> = (0..SYN_REPEATS)
        .map(|_| check_face_pair_bijective(&mesh, &face_map, &arena))
        .collect();

    // Helper: extract a comparable canonical signature for one report.
    // Sort sample positions lexicographically by the f64 bits so the SET
    // comparison is iteration-order-independent.
    fn canonical_signature(r: &BijectivityReport) -> CanonSig {
        let mut pairs: Vec<CanonPair> = r
            .non_bijective_pairs
            .iter()
            .map(|p| {
                let mut sa: Vec<([u64; 3], [u64; 3])> = p
                    .sample_unmatched_a
                    .iter()
                    .map(|(p, q)| {
                        (
                            [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()],
                            [q[0].to_bits(), q[1].to_bits(), q[2].to_bits()],
                        )
                    })
                    .collect();
                sa.sort();
                let mut sb: Vec<([u64; 3], [u64; 3])> = p
                    .sample_unmatched_b
                    .iter()
                    .map(|(p, q)| {
                        (
                            [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()],
                            [q[0].to_bits(), q[1].to_bits(), q[2].to_bits()],
                        )
                    })
                    .collect();
                sb.sort();
                CanonPair {
                    face_a: p.face_a.0,
                    face_b: p.face_b.0,
                    unmatched_a: p.unmatched_a_count,
                    unmatched_b: p.unmatched_b_count,
                    sorted_sample_a: sa,
                    sorted_sample_b: sb,
                }
            })
            .collect();
        // Canonicalize pair order (face_a ≤ face_b assumed by the
        // BijectiveFacePairOracle's pair-canonicalization in
        // `bijective.rs:391-395`, but the OUTER iteration order over
        // BTreeSet is already sorted). Sort here defensively so
        // canonical signatures are robust against future oracle changes.
        pairs.sort_by_key(|p| (p.face_a, p.face_b));
        CanonSig {
            total_pairs: r.total_pairs_examined,
            bijective_pairs: r.bijective_pairs,
            non_bij_count: r.non_bijective_pairs.len(),
            is_bijective: r.is_bijective(),
            pairs,
        }
    }

    let sigs: Vec<CanonSig> = reports.iter().map(canonical_signature).collect();

    for (i, sig) in sigs.iter().enumerate() {
        eprintln!(
            "[pr12-bij-test5] run {}: is_bij={} total_pairs={} bij={} non_bij={} pairs={:?}",
            i, sig.is_bijective, sig.total_pairs, sig.bijective_pairs, sig.non_bij_count, sig.pairs,
        );
    }

    // Determinism contract: all synthetic runs produce byte-identical
    // canonical signatures (same verdict, same counts, same sorted
    // samples).
    for i in 1..SYN_REPEATS {
        assert_eq!(
            sigs[0], sigs[i],
            "[pr12-bij-test5] Stage 1 oracle non-determinism on synthetic \
             T-junction fixture: run 0 and run {i} produced divergent \
             canonical signatures on the SAME inputs. Per spec §8 Step 1, \
             replace the HashMap-based iteration in \
             `face_boundary_directed_edges` and `restrict_to_shared_boundary` \
             with BTreeMap or sorted iteration so the bijective oracle is \
             structurally deterministic (Yang §4.1.1: byte-identical \
             contract requires stable evaluation). Run 0 = {:?}, \
             Run {i} = {:?}",
            sigs[0], sigs[i],
        );
    }

    // Per FIP §4.3 (numeric/structural assertion): pin the expected
    // signature on the T-junction fixture so a future oracle behavior
    // change (e.g., a new same-edge dedup rule) flips this test loudly
    // rather than silently. Pre-Step-1 + post-Step-1 both must satisfy
    // these structural facts; only the SET equality across runs is the
    // load-bearing red signal.
    assert_eq!(
        sigs[0].total_pairs, 1,
        "[pr12-bij-test5] T-junction fixture must produce exactly 1 \
         examined pair (regression guard for polygon-soup pair detection)"
    );
    assert!(
        !sigs[0].is_bijective,
        "[pr12-bij-test5] T-junction fixture must FAIL bijective check \
         (sensitivity guard); got is_bijective=true with {} non-bij pairs",
        sigs[0].non_bij_count
    );
}

// ── Determinism canonical-signature support types ───────────────────────

/// Canonical signature of one `NonBijectivePair`, with samples sorted to
/// be iteration-order-invariant. Used by Test 5 only.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonPair {
    face_a: usize,
    face_b: usize,
    unmatched_a: usize,
    unmatched_b: usize,
    sorted_sample_a: Vec<([u64; 3], [u64; 3])>,
    sorted_sample_b: Vec<([u64; 3], [u64; 3])>,
}

/// Canonical signature of one `BijectivityReport`. Used by Test 5 only.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonSig {
    total_pairs: usize,
    bijective_pairs: usize,
    non_bij_count: usize,
    is_bijective: bool,
    pairs: Vec<CanonPair>,
}
