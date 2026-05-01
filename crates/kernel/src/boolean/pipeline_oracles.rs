//! Per-stage pipeline-oracle harness for the Yang 2025 hybrid B-Rep/mesh
//! boolean pipeline.
//!
//! Replaces the prior end-to-end-oracle-anchor-archaeology methodology with
//! per-stage contract validation: each Yang pipeline stage gets a contract
//! oracle that runs against a snapshot of that stage's output. The runner
//! reports the EARLIEST failing stage so root-cause attribution survives
//! downstream cascading symptoms.
//!
//! ## Stage map (Yang 2025 §4)
//!
//! - Stage 0 — Coplanar preprocessing (§4.5.5)
//! - Stage 1 — Bijective tessellation (§4.1.1)
//! - Stage 2 — Mesh arrangement / subdivision (§4.2; Cherchi 2022 §4)
//! - Stage 3 — SSI refinement (§4.3) [currently stubbed]
//! - Stage 4a — Mesh updating along refined curves (§4.4.1) [currently stubbed]
//! - Stage 4b — Inside/outside cell classification (§4.4.2; Cherchi 2022 §5)
//! - Stage 5 — Manifold-edge patch segmentation (§4.4)
//! - Stage 6 — B-Rep assembly + retessellation (§4.5)
//!
//! ## Audit context
//!
//! See `docs/audits/yang_audit_2026-04-30.md` Cluster Y-I. The dominant
//! 92-case YB-01 failure bucket (twin-symmetry violation in
//! `validate_yang_result_topology`) is the downstream symptom of upstream
//! stage contracts not being enforced. This harness lets agents wire
//! per-stage oracles so the failing stage is named directly rather than
//! inferred from end-of-pipeline rubble.
//!
//! ## Scope
//!
//! Oracles are diagnostic-only: they observe pipeline state, never mutate
//! it. The harness has no production behavior side-effect — it is invoked
//! explicitly from tests / corpus runners.
//!
//! Refs: Yang 2025 §4 (full pipeline); Cherchi 2022 §4 (arrangement) + §5
//! (ray-cast in/out + manifold-patch graph).

// PR9 harness infrastructure. Consumed by agents A/B/C (Stage 0/2/4b
// oracles) and the corpus runner; not yet wired into production paths.
#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::boolean::coplanar_preprocess::CoplanarFacePair;
use crate::boolean::exact_mesh::{
    build_manifold_patch_graph, CellLabeling, ManifoldPatchGraph, SubdividedMesh,
};
use crate::boolean::oracles::{
    arrangement_wellformed::MeshArrangementWellFormedOracle,
    coplanar_identical::CoplanarMeshIdenticalOracle,
    label_consistency::LabelConsistencyWithinPatchOracle,
};
use crate::boolean::topology_extract::{FaceSurvivalMap, ResultTopology};
use crate::tessellation::bijective::{check_face_pair_bijective, BijectivityReport};
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::FaceIdx;
use crate::types::RenderMesh;

// ── Stage enumeration ───────────────────────────────────────────────────

/// One stage of the Yang 2025 hybrid B-Rep/mesh boolean pipeline.
///
/// `Ord` impl follows pipeline order (Stage 0 < Stage 1 < ... < Stage 6) so
/// the runner can identify the earliest failing stage by `min`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum YangStage {
    /// Coplanar face preprocessing — Yang 2025 §4.5.5.
    Stage0Coplanar,
    /// Bijective tessellation — Yang 2025 §4.1.1.
    Stage1Bijective,
    /// Exact mesh arrangement / subdivision — Yang 2025 §4.2; Cherchi 2022 §4.
    Stage2Arrangement,
    /// SSI refinement — Yang 2025 §4.3 (currently stubbed in tree).
    Stage3SsiRefinement,
    /// Mesh updating along refined curves — Yang 2025 §4.4.1 (currently stubbed).
    Stage4aMeshUpdating,
    /// Inside/outside classification — Yang 2025 §4.4.2; Cherchi 2022 §5.
    Stage4bClassification,
    /// Manifold-edge patch segmentation — Yang 2025 §4.4.
    Stage5PatchSegment,
    /// B-Rep assembly + retessellation — Yang 2025 §4.5.
    Stage6Assembly,
}

impl YangStage {
    /// Yang 2025 paper section that defines this stage's contract.
    pub fn yang_section(self) -> &'static str {
        match self {
            YangStage::Stage0Coplanar => "Yang 2025 §4.5.5",
            YangStage::Stage1Bijective => "Yang 2025 §4.1.1",
            YangStage::Stage2Arrangement => "Yang 2025 §4.2",
            YangStage::Stage3SsiRefinement => "Yang 2025 §4.3",
            YangStage::Stage4aMeshUpdating => "Yang 2025 §4.4.1",
            YangStage::Stage4bClassification => "Yang 2025 §4.4.2",
            YangStage::Stage5PatchSegment => "Yang 2025 §4.4",
            YangStage::Stage6Assembly => "Yang 2025 §4.5",
        }
    }
}

// ── Pipeline state snapshots ────────────────────────────────────────────

/// Snapshot of Stage 0 (coplanar preprocessing) output.
///
/// Stage 0 detects coplanar face pairs between the two operand solids and
/// — per Yang §4.5.5 — emits a shared trimmed surface so both meshes carry
/// bitwise-identical triangulation in the overlap region.
///
/// Held shape is a placeholder: the production pipeline emits coplanar
/// pair records (`CoplanarFacePair`) alongside side-effecting B-Rep
/// mutation and post-tessellation mesh injection; no single struct
/// currently captures the full Stage 0 contract surface. Agent A will
/// define what additional snapshot fields the Stage 0 oracle needs.
#[derive(Debug, Default)]
pub(crate) struct CoplanarPreprocessSnapshot {
    /// Coplanar face pairs detected between operand A and operand B.
    pub pairs: Vec<CoplanarFacePair>,
    /// Tessellated render mesh for operand A AFTER Stage 0 + tessellation.
    /// `None` if not snapshotted.
    pub mesh_a: Option<RenderMesh>,
    /// Tessellated render mesh for operand B AFTER Stage 0 + tessellation.
    /// `None` if not snapshotted.
    pub mesh_b: Option<RenderMesh>,
}

/// Snapshot of Stage 1 (bijective tessellation) output.
///
/// Carries the rendermesh + face_map + arena required by
/// `check_face_pair_bijective` (PR1). Both operands are snapshotted because
/// bijectivity is a per-operand property (each operand's face boundaries
/// must reciprocate byte-identically across shared B-Rep edges).
pub(crate) struct BijectiveSnapshot<'a> {
    pub rendermesh_a: &'a RenderMesh,
    pub face_map_a: &'a BTreeMap<u64, FaceIdx>,
    pub arena_a: &'a TopoArena,
    pub rendermesh_b: &'a RenderMesh,
    pub face_map_b: &'a BTreeMap<u64, FaceIdx>,
    pub arena_b: &'a TopoArena,
}

/// Snapshot of pipeline state to run oracles against.
///
/// Each field is `Option` because not every test case produces a snapshot
/// for every stage (upstream errors short-circuit downstream stages, and
/// stubbed stages produce no output). The runner skips oracles whose
/// stage's snapshot is `None` — except where the oracle's contract IS
/// "this snapshot must be present"; those oracles report
/// `ViolationKind::StateMissing` and their stage is named.
///
/// Lifetimes: the Stage 1 bijective snapshot borrows from the caller
/// (`RenderMesh` + `TopoArena` are large). Other fields own their data
/// because they're produced fresh by the pipeline and can be moved into
/// the snapshot.
pub(crate) struct PipelineState<'a> {
    pub stage_0_coplanar: Option<CoplanarPreprocessSnapshot>,
    pub stage_1_bijective: Option<BijectiveSnapshot<'a>>,
    pub stage_2_subdivided: Option<SubdividedMesh>,
    pub stage_4b_labeling: Option<CellLabeling>,
    pub stage_5_face_survival: Option<FaceSurvivalMap>,
    pub stage_6_result_topology: Option<ResultTopology>,
}

impl<'a> PipelineState<'a> {
    /// Empty pipeline state (every stage `None`). Useful as a starting
    /// point for tests that snapshot specific stages.
    pub(crate) fn empty() -> Self {
        PipelineState {
            stage_0_coplanar: None,
            stage_1_bijective: None,
            stage_2_subdivided: None,
            stage_4b_labeling: None,
            stage_5_face_survival: None,
            stage_6_result_topology: None,
        }
    }
}

// ── Oracle trait + violation type ───────────────────────────────────────

/// A per-stage contract oracle.
///
/// Each oracle wraps an existing pipeline invariant check (PR1
/// bijectivity, PR8 patch-graph conservation, twin symmetry) or a
/// new contract derived from Yang/Cherchi paper sections (Stage 0/2/4b
/// — agents A/B/C). Oracles are diagnostic-only: they read pipeline
/// state and return a verdict.
///
/// Implementors MUST cite the Yang section (and Cherchi section where
/// applicable, see Cherchi 2022 §4 for arrangement and §5 for
/// inside/outside + manifold patches).
pub(crate) trait StageOracle: Send + Sync {
    /// The pipeline stage this oracle targets.
    fn stage(&self) -> YangStage;
    /// Human-readable name (`CamelCase`, e.g. `"BijectiveFacePairOracle"`).
    fn name(&self) -> &'static str;
    /// Yang 2025 paper section this oracle's contract derives from.
    fn yang_section(&self) -> &'static str {
        self.stage().yang_section()
    }
    /// Cherchi paper section if this oracle's contract derives from Cherchi
    /// 2020/2022 (arrangement, ray-cast, manifold patches).
    fn cherchi_section(&self) -> Option<&'static str> {
        None
    }
    /// Run the oracle. Returns `Ok(())` if the contract is satisfied,
    /// `Err(violation)` otherwise.
    ///
    /// Oracles whose stage's snapshot is `None` should return
    /// `Err(ViolationKind::StateMissing)` ONLY if the missing snapshot
    /// is itself a contract violation; otherwise the runner skips them.
    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation>;
}

/// Diagnostic record for one oracle that rejected pipeline state.
#[derive(Debug, Clone)]
pub struct OracleViolation {
    pub stage: YangStage,
    pub oracle_name: &'static str,
    /// Free-form message describing what failed. Should be specific
    /// enough that a reader can locate the failing data in the pipeline
    /// state (e.g. include indices, counts, or sample positions).
    pub message: String,
    pub kind: ViolationKind,
}

/// Categorical reason for an oracle's rejection. Used by the corpus
/// runner to bucket failures into a histogram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationKind {
    /// The pipeline state for this stage was not produced (upstream
    /// error, stubbed stage, or test fixture didn't populate it).
    /// Oracles report this only when the missing snapshot IS the
    /// contract failure; otherwise the runner just skips.
    StateMissing,
    /// The contract was checked and rejected (the data is present
    /// but violates an invariant).
    ContractViolated,
    /// The oracle is intentionally a placeholder (no contract logic
    /// implemented yet). Reported so the corpus histogram can
    /// distinguish "passing" from "not yet checked".
    OracleStub,
}

// ── Integrated oracles ──────────────────────────────────────────────────

/// Stage 1 oracle — wraps PR1's `check_face_pair_bijective`.
///
/// Per Yang §4.1.1, the tessellation must be bijective: every face's
/// boundary directed edges must reciprocate byte-identically as
/// `(q, p)` on adjacent faces sharing the same B-Rep edge. The oracle
/// runs the check on BOTH operands and reports a violation if either
/// fails.
pub(crate) struct BijectiveFacePairOracle;

impl StageOracle for BijectiveFacePairOracle {
    fn stage(&self) -> YangStage {
        YangStage::Stage1Bijective
    }
    fn name(&self) -> &'static str {
        "BijectiveFacePairOracle"
    }
    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation> {
        let snap = match state.stage_1_bijective.as_ref() {
            Some(s) => s,
            None => return Ok(()), // Skip when not snapshotted.
        };
        let report_a = check_face_pair_bijective(snap.rendermesh_a, snap.face_map_a, snap.arena_a);
        let report_b = check_face_pair_bijective(snap.rendermesh_b, snap.face_map_b, snap.arena_b);
        if report_a.is_bijective() && report_b.is_bijective() {
            return Ok(());
        }
        Err(OracleViolation {
            stage: self.stage(),
            oracle_name: self.name(),
            message: format!(
                "non-bijective face pairs: operand A {} pair(s) of {}, \
                 operand B {} pair(s) of {}",
                report_a.non_bijective_pairs.len(),
                report_a.total_pairs_examined,
                report_b.non_bijective_pairs.len(),
                report_b.total_pairs_examined,
            ),
            kind: ViolationKind::ContractViolated,
        })
    }
}

impl BijectiveFacePairOracle {
    /// Run the underlying check and return the raw `BijectivityReport`s
    /// for callers who want detailed diagnostics (sample unmatched
    /// edges, per-pair breakdown). Returns `None` if no Stage 1
    /// snapshot is present.
    pub(crate) fn raw_reports(
        state: &PipelineState,
    ) -> Option<(BijectivityReport, BijectivityReport)> {
        let snap = state.stage_1_bijective.as_ref()?;
        Some((
            check_face_pair_bijective(snap.rendermesh_a, snap.face_map_a, snap.arena_a),
            check_face_pair_bijective(snap.rendermesh_b, snap.face_map_b, snap.arena_b),
        ))
    }
}

/// Stage 5 oracle — wraps PR8's `build_manifold_patch_graph`.
///
/// Verifies the conservation invariant: the patch graph partitions the
/// combined sub-triangle soup, so the sum of patch sizes must equal the
/// total number of sub-triangles. A violation indicates a defect in the
/// patch-graph builder (a sub-triangle was lost or double-counted).
///
/// Refs: Yang 2025 §4.4; Cherchi 2022 §5 (manifold-edge patch graph).
pub(crate) struct ManifoldPatchConservationOracle;

impl StageOracle for ManifoldPatchConservationOracle {
    fn stage(&self) -> YangStage {
        YangStage::Stage5PatchSegment
    }
    fn name(&self) -> &'static str {
        "ManifoldPatchConservationOracle"
    }
    fn cherchi_section(&self) -> Option<&'static str> {
        Some("Cherchi 2022 §5")
    }
    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation> {
        let subdivided = match state.stage_2_subdivided.as_ref() {
            Some(s) => s,
            None => return Ok(()), // Skip when Stage 2 didn't run.
        };
        let graph: ManifoldPatchGraph = build_manifold_patch_graph(subdivided);
        let expected = subdivided.tris_a.len() + subdivided.tris_b.len();
        let actual: usize = graph.patches.iter().map(|p| p.len()).sum();
        if actual == expected {
            return Ok(());
        }
        Err(OracleViolation {
            stage: self.stage(),
            oracle_name: self.name(),
            message: format!(
                "patch-graph conservation violated: Σ patches[i].len() = {actual}, \
                 expected tris_a + tris_b = {expected} ({} patches)",
                graph.patches.len(),
            ),
            kind: ViolationKind::ContractViolated,
        })
    }
}

/// Stage 6 oracle — verifies twin symmetry of the result topology.
///
/// Mirrors the YB-01 failure-mode check that
/// `validate_yang_result_topology` performs in `yang_integration.rs`:
/// for every half-edge `i`, `arena.half_edges[arena.half_edges[i].twin].twin == i`.
/// A violation is the dominant 92-case assay-failure bucket per the
/// 2026-04-30 audit (Cluster Y-I).
///
/// We re-implement the check inline rather than calling the private
/// `validate_yang_result_topology` so this oracle stays self-contained
/// in `pipeline_oracles.rs` (per task scope).
///
/// Refs: Yang 2025 §4.5; Mantyla 1988 §4.2 (half-edge invariants).
pub(crate) struct TwinSymmetryOracle;

impl StageOracle for TwinSymmetryOracle {
    fn stage(&self) -> YangStage {
        YangStage::Stage6Assembly
    }
    fn name(&self) -> &'static str {
        "TwinSymmetryOracle"
    }
    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation> {
        let result = match state.stage_6_result_topology.as_ref() {
            Some(r) => r,
            None => return Ok(()), // Skip when Stage 6 didn't run.
        };
        let arena = &result.arena;
        let n_he = arena.half_edges.len();
        for (i, he) in arena.half_edges.iter().enumerate() {
            if he.twin.0 >= n_he {
                return Err(OracleViolation {
                    stage: self.stage(),
                    oracle_name: self.name(),
                    message: format!(
                        "half_edge[{i}].twin = {} but only {n_he} half_edges exist",
                        he.twin.0
                    ),
                    kind: ViolationKind::ContractViolated,
                });
            }
            let twin_he = &arena.half_edges[he.twin.0];
            if twin_he.twin.0 != i {
                return Err(OracleViolation {
                    stage: self.stage(),
                    oracle_name: self.name(),
                    message: format!(
                        "half_edge[{i}].twin = {} but twin.twin = {} (expected {i})",
                        he.twin.0, twin_he.twin.0
                    ),
                    kind: ViolationKind::ContractViolated,
                });
            }
        }
        Ok(())
    }
}

// ── Runner ──────────────────────────────────────────────────────────────

/// Result of running a battery of oracles against one pipeline state.
#[derive(Debug)]
pub(crate) struct OracleRunResult {
    /// Caller-supplied case identifier (assay case ID, test name, etc.).
    pub case_id: String,
    /// One entry per oracle, in stage order. The verdict is `Ok(())` for
    /// passing oracles and skipped oracles whose snapshots were `None`.
    pub per_oracle: Vec<(YangStage, &'static str, Result<(), OracleViolation>)>,
    /// Earliest stage with a failing oracle. `None` if all oracles passed
    /// (or were skipped). Used for histogram bucketing per audit
    /// methodology — root cause attribution to upstream stages.
    pub first_failing_stage: Option<YangStage>,
}

/// Run a battery of oracles against pipeline state.
///
/// Oracles run in stage order (`YangStage::Ord`), so violations are
/// reported in pipeline-order. The earliest failing stage is recorded
/// as `first_failing_stage` — that is the root-cause attribution.
/// Oracles for stages whose snapshot is `None` should self-skip
/// (return `Ok(())` from `check`); the runner does not pre-filter,
/// so an oracle whose contract IS "the snapshot must be present"
/// can still report `StateMissing`.
///
/// Stages without a registered oracle are simply omitted from the
/// result — the runner does not require all stages to be covered
/// (agents A/B/C may not have shipped their oracles yet).
pub(crate) fn run_pipeline_oracles(
    case_id: &str,
    state: &PipelineState,
    oracles: &[Box<dyn StageOracle>],
) -> OracleRunResult {
    // Sort oracles by stage so violations are reported in pipeline order.
    // Use a stable index sort to preserve registration order within a stage.
    let mut order: Vec<usize> = (0..oracles.len()).collect();
    order.sort_by_key(|&i| oracles[i].stage());

    let mut per_oracle = Vec::with_capacity(order.len());
    let mut first_failing: Option<YangStage> = None;
    for i in order {
        let oracle = &oracles[i];
        let stage = oracle.stage();
        let name = oracle.name();
        let verdict = oracle.check(state);
        if verdict.is_err() {
            first_failing = Some(match first_failing {
                Some(prev) => prev.min(stage),
                None => stage,
            });
        }
        per_oracle.push((stage, name, verdict));
    }
    OracleRunResult {
        case_id: case_id.to_string(),
        per_oracle,
        first_failing_stage: first_failing,
    }
}

// ── Default oracle registry ─────────────────────────────────────────────

/// PR9 default oracle registry: the six oracles wired in this PR. Order is
/// registration order; the runner sorts by `YangStage` before running.
///
/// Returned as `Vec<Box<dyn StageOracle>>` so the corpus runner can extend
/// or replace entries (e.g. a future PR may add a Stage 3 SSI-refinement
/// oracle and slot it in by appending to this vec).
pub(crate) fn default_oracle_registry() -> Vec<Box<dyn StageOracle>> {
    vec![
        Box::new(CoplanarMeshIdenticalOracle),
        Box::new(BijectiveFacePairOracle),
        Box::new(MeshArrangementWellFormedOracle),
        Box::new(LabelConsistencyWithinPatchOracle),
        Box::new(ManifoldPatchConservationOracle),
        Box::new(TwinSymmetryOracle),
    ]
}

// ── Thread-local snapshot collector (PR9 corpus-runner instrumentation) ─

/// Owned, lifetime-free analog of `PipelineState`. The thread-local
/// collector accumulates owned data; the corpus runner converts it to a
/// borrowed `PipelineState` only at the moment of running oracles.
///
/// PR9 instrumentation; not stable API. Production callers do NOT install
/// a collector — `record_*` calls are no-ops when the thread-local is
/// `None`. The corpus runner installs/uninstalls a collector around each
/// case via [`with_snapshot_collector`].
#[derive(Default)]
pub(crate) struct OwnedSnapshotBundle {
    pub stage_0_coplanar: Option<CoplanarPreprocessSnapshot>,
    pub stage_1_rendermesh_a: Option<RenderMesh>,
    pub stage_1_face_map_a: Option<BTreeMap<u64, FaceIdx>>,
    pub stage_1_arena_a: Option<TopoArena>,
    pub stage_1_rendermesh_b: Option<RenderMesh>,
    pub stage_1_face_map_b: Option<BTreeMap<u64, FaceIdx>>,
    pub stage_1_arena_b: Option<TopoArena>,
    pub stage_2_subdivided: Option<SubdividedMesh>,
    pub stage_4b_labeling: Option<CellLabeling>,
    pub stage_5_face_survival: Option<FaceSurvivalMap>,
    pub stage_6_result_topology: Option<ResultTopology>,
}

impl OwnedSnapshotBundle {
    /// Produce a borrowed `PipelineState` referencing this bundle's owned
    /// data. The bundle must outlive the returned state.
    pub(crate) fn as_pipeline_state(&self) -> PipelineState<'_> {
        let stage_1 = match (
            self.stage_1_rendermesh_a.as_ref(),
            self.stage_1_face_map_a.as_ref(),
            self.stage_1_arena_a.as_ref(),
            self.stage_1_rendermesh_b.as_ref(),
            self.stage_1_face_map_b.as_ref(),
            self.stage_1_arena_b.as_ref(),
        ) {
            (Some(rm_a), Some(fm_a), Some(ar_a), Some(rm_b), Some(fm_b), Some(ar_b)) => {
                Some(BijectiveSnapshot {
                    rendermesh_a: rm_a,
                    face_map_a: fm_a,
                    arena_a: ar_a,
                    rendermesh_b: rm_b,
                    face_map_b: fm_b,
                    arena_b: ar_b,
                })
            }
            _ => None,
        };
        PipelineState {
            stage_0_coplanar: self.stage_0_coplanar.as_ref().map(|s| {
                // CoplanarPreprocessSnapshot is owned; clone to keep `self` borrowed.
                CoplanarPreprocessSnapshot {
                    pairs: s.pairs.clone(),
                    mesh_a: s.mesh_a.clone(),
                    mesh_b: s.mesh_b.clone(),
                }
            }),
            stage_1_bijective: stage_1,
            stage_2_subdivided: self.stage_2_subdivided.clone(),
            stage_4b_labeling: self.stage_4b_labeling.as_ref().map(|l| CellLabeling {
                labels_a: l.labels_a.clone(),
                labels_b: l.labels_b.clone(),
            }),
            stage_5_face_survival: None, // Stage 5 oracle reads stage_2; survival not needed.
            stage_6_result_topology: self.stage_6_result_topology.as_ref().map(|t| {
                ResultTopology {
                    arena: t.arena.clone(),
                    face_provenance: t.face_provenance.clone(),
                    edge_is_intersection: t.edge_is_intersection.clone(),
                }
            }),
        }
    }
}

thread_local! {
    /// Active snapshot collector. `None` when no diagnostic run is in
    /// progress; production callers never install one.
    static SNAPSHOT_COLLECTOR: std::cell::RefCell<Option<OwnedSnapshotBundle>> =
        const { std::cell::RefCell::new(None) };
}

/// Install a fresh snapshot collector for the duration of `f`, then take
/// the populated bundle out and return it. If `f` panics, the collector
/// is still cleared (so a panicking pipeline does not leak stale state
/// into the next run).
///
/// PR9 instrumentation; not stable API.
pub(crate) fn with_snapshot_collector<F, R>(f: F) -> (OwnedSnapshotBundle, R)
where
    F: FnOnce() -> R,
{
    SNAPSHOT_COLLECTOR.with(|cell| {
        *cell.borrow_mut() = Some(OwnedSnapshotBundle::default());
    });
    // RAII guard ensures the collector is always cleared, even on panic.
    struct ClearOnDrop;
    impl Drop for ClearOnDrop {
        fn drop(&mut self) {
            SNAPSHOT_COLLECTOR.with(|cell| {
                // Leave the bundle in place so the caller can take it; only
                // null out the collector if the caller did not.
                let _ = cell.borrow_mut();
            });
        }
    }
    let _guard = ClearOnDrop;
    let result = f();
    let bundle = SNAPSHOT_COLLECTOR.with(|cell| cell.borrow_mut().take().unwrap_or_default());
    (bundle, result)
}

/// Record a stage snapshot if a collector is active. No-op otherwise.
///
/// `update` receives `&mut OwnedSnapshotBundle` and writes the relevant
/// stage field. PR9 instrumentation; not stable API. Called from
/// `yang_boolean_inner` and `yang_boolean_pipeline` at stage boundaries.
pub(crate) fn record_snapshot<F>(update: F)
where
    F: FnOnce(&mut OwnedSnapshotBundle),
{
    SNAPSHOT_COLLECTOR.with(|cell| {
        if let Some(bundle) = cell.borrow_mut().as_mut() {
            update(bundle);
        }
    });
}

// ── Module tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::exact_mesh::SubTriangle;
    use crate::topology::half_edge::{EdgeIdx, HalfEdge, HalfEdgeIdx, LoopIdx, VertexIdx};

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Build a `SubdividedMesh` with two valid sub-triangles per side
    /// that share an edge — the patch-graph conservation invariant
    /// holds trivially (Σ |patches[i]| == 4 == tris_a + tris_b) for
    /// ANY well-formed graph builder, regardless of patch grouping.
    fn synthetic_subdivided_ok() -> SubdividedMesh {
        SubdividedMesh {
            verts: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            tris_a: vec![
                SubTriangle {
                    verts: [0, 1, 2],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
                SubTriangle {
                    verts: [1, 3, 2],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
            ],
            tris_b: vec![
                SubTriangle {
                    verts: [0, 2, 1],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
                SubTriangle {
                    verts: [1, 2, 3],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
            ],
            params_a: vec![None; 4],
            params_b: vec![None; 4],
            // Spec §F1 default for synthetic fixtures.
            upstream_tri_count: 4,
        }
    }

    /// Helper: build a half-edge with the given twin index. Other fields
    /// are filler — the Stage 6 oracle only reads `twin`.
    fn he(twin: usize) -> HalfEdge {
        HalfEdge {
            origin: VertexIdx(0),
            edge: EdgeIdx(0),
            twin: HalfEdgeIdx(twin),
            next: HalfEdgeIdx(0),
            prev: HalfEdgeIdx(0),
            loop_: LoopIdx(0),
        }
    }

    /// Build a `ResultTopology` with a single half-edge whose twin
    /// points to itself — a valid (degenerate) twin-symmetric arena.
    fn result_topology_ok() -> ResultTopology {
        let mut arena = TopoArena::new();
        arena.half_edges.push(he(0)); // self-twin: he.twin.twin == he holds.
        ResultTopology {
            arena,
            face_provenance: BTreeMap::new(),
            edge_is_intersection: BTreeMap::new(),
        }
    }

    /// Build a `ResultTopology` with two half-edges where twin symmetry
    /// is violated: `he[0].twin = 1` but `he[1].twin = 1` (should be 0).
    fn result_topology_twin_broken() -> ResultTopology {
        let mut arena = TopoArena::new();
        arena.half_edges.push(he(1));
        arena.half_edges.push(he(1)); // BROKEN — should point back to 0.
        ResultTopology {
            arena,
            face_provenance: BTreeMap::new(),
            edge_is_intersection: BTreeMap::new(),
        }
    }

    // ── Stage ordering ──────────────────────────────────────────────────

    #[test]
    fn yang_stage_ord_follows_pipeline_order() {
        assert!(YangStage::Stage0Coplanar < YangStage::Stage1Bijective);
        assert!(YangStage::Stage1Bijective < YangStage::Stage2Arrangement);
        assert!(YangStage::Stage2Arrangement < YangStage::Stage3SsiRefinement);
        assert!(YangStage::Stage3SsiRefinement < YangStage::Stage4aMeshUpdating);
        assert!(YangStage::Stage4aMeshUpdating < YangStage::Stage4bClassification);
        assert!(YangStage::Stage4bClassification < YangStage::Stage5PatchSegment);
        assert!(YangStage::Stage5PatchSegment < YangStage::Stage6Assembly);
    }

    #[test]
    fn yang_stage_section_strings_nonempty() {
        for stage in [
            YangStage::Stage0Coplanar,
            YangStage::Stage1Bijective,
            YangStage::Stage2Arrangement,
            YangStage::Stage3SsiRefinement,
            YangStage::Stage4aMeshUpdating,
            YangStage::Stage4bClassification,
            YangStage::Stage5PatchSegment,
            YangStage::Stage6Assembly,
        ] {
            assert!(!stage.yang_section().is_empty());
        }
    }

    // ── Runner: empty oracle list ───────────────────────────────────────

    #[test]
    fn runner_with_no_oracles_reports_no_failure() {
        let state = PipelineState::empty();
        let oracles: Vec<Box<dyn StageOracle>> = Vec::new();
        let result = run_pipeline_oracles("empty", &state, &oracles);
        assert_eq!(result.case_id, "empty");
        assert!(result.per_oracle.is_empty());
        assert!(result.first_failing_stage.is_none());
    }

    // ── Stage 5 oracle (PR8 patch-graph conservation) ───────────────────

    #[test]
    fn stage5_oracle_passes_on_well_formed_subdivided() {
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(synthetic_subdivided_ok());
        let oracles: Vec<Box<dyn StageOracle>> = vec![Box::new(ManifoldPatchConservationOracle)];
        let result = run_pipeline_oracles("ok", &state, &oracles);
        assert!(result.first_failing_stage.is_none());
        assert!(result.per_oracle[0].2.is_ok());
    }

    #[test]
    fn stage5_oracle_skips_when_stage2_missing() {
        // No Stage 2 snapshot → oracle silently passes (skipped).
        // Per design: oracles whose snapshot is missing self-skip
        // unless missing-snapshot IS the contract violation.
        let state = PipelineState::empty();
        let oracles: Vec<Box<dyn StageOracle>> = vec![Box::new(ManifoldPatchConservationOracle)];
        let result = run_pipeline_oracles("missing", &state, &oracles);
        assert!(result.first_failing_stage.is_none());
    }

    // ── Stage 6 oracle (twin symmetry) ──────────────────────────────────

    #[test]
    fn stage6_oracle_passes_on_self_twin_arena() {
        let mut state = PipelineState::empty();
        state.stage_6_result_topology = Some(result_topology_ok());
        let oracles: Vec<Box<dyn StageOracle>> = vec![Box::new(TwinSymmetryOracle)];
        let result = run_pipeline_oracles("ok", &state, &oracles);
        assert!(result.first_failing_stage.is_none());
    }

    #[test]
    fn stage6_oracle_catches_twin_violation() {
        let mut state = PipelineState::empty();
        state.stage_6_result_topology = Some(result_topology_twin_broken());
        let oracles: Vec<Box<dyn StageOracle>> = vec![Box::new(TwinSymmetryOracle)];
        let result = run_pipeline_oracles("broken", &state, &oracles);
        assert_eq!(result.first_failing_stage, Some(YangStage::Stage6Assembly));
        let (_, _, verdict) = &result.per_oracle[0];
        let violation = verdict.as_ref().unwrap_err();
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert_eq!(violation.stage, YangStage::Stage6Assembly);
        assert!(violation.message.contains("twin"));
    }

    // ── Earliest-stage attribution ──────────────────────────────────────

    /// Synthetic oracle that always rejects, parameterized by stage.
    struct AlwaysFailingOracle {
        stage: YangStage,
        name: &'static str,
    }

    impl StageOracle for AlwaysFailingOracle {
        fn stage(&self) -> YangStage {
            self.stage
        }
        fn name(&self) -> &'static str {
            self.name
        }
        fn check(&self, _state: &PipelineState) -> Result<(), OracleViolation> {
            Err(OracleViolation {
                stage: self.stage,
                oracle_name: self.name,
                message: "synthetic always-fail".to_string(),
                kind: ViolationKind::ContractViolated,
            })
        }
    }

    #[test]
    fn runner_reports_earliest_failing_stage() {
        // Three failing oracles at stages 1, 5, 6 — out of registration
        // order — runner must report stage 1 as first_failing_stage.
        let state = PipelineState::empty();
        let oracles: Vec<Box<dyn StageOracle>> = vec![
            Box::new(AlwaysFailingOracle {
                stage: YangStage::Stage6Assembly,
                name: "fail6",
            }),
            Box::new(AlwaysFailingOracle {
                stage: YangStage::Stage1Bijective,
                name: "fail1",
            }),
            Box::new(AlwaysFailingOracle {
                stage: YangStage::Stage5PatchSegment,
                name: "fail5",
            }),
        ];
        let result = run_pipeline_oracles("multi", &state, &oracles);
        assert_eq!(result.first_failing_stage, Some(YangStage::Stage1Bijective));
        // Verdicts come back in stage order.
        assert_eq!(result.per_oracle[0].0, YangStage::Stage1Bijective);
        assert_eq!(result.per_oracle[1].0, YangStage::Stage5PatchSegment);
        assert_eq!(result.per_oracle[2].0, YangStage::Stage6Assembly);
        assert!(result.per_oracle.iter().all(|(_, _, v)| v.is_err()));
    }

    #[test]
    fn runner_with_passing_oracle_after_failing_still_attributes_to_first() {
        // Failing stage 1 + passing stage 6 — first_failing_stage = 1.
        let mut state = PipelineState::empty();
        state.stage_6_result_topology = Some(result_topology_ok());
        let oracles: Vec<Box<dyn StageOracle>> = vec![
            Box::new(AlwaysFailingOracle {
                stage: YangStage::Stage1Bijective,
                name: "fail1",
            }),
            Box::new(TwinSymmetryOracle),
        ];
        let result = run_pipeline_oracles("mixed", &state, &oracles);
        assert_eq!(result.first_failing_stage, Some(YangStage::Stage1Bijective));
        assert!(result.per_oracle[0].2.is_err());
        assert!(result.per_oracle[1].2.is_ok());
    }

    // ── PipelineState construction ──────────────────────────────────────

    #[test]
    fn default_registry_covers_six_stages() {
        // The 6-oracle registry hits Stage 0, 1, 2, 4b, 5, 6. Stages 3 and
        // 4a are intentionally absent (those Yang stages are stubbed in
        // tree per the audit) and the runner does not require coverage.
        let registry = default_oracle_registry();
        assert_eq!(registry.len(), 6);
        let stages: Vec<YangStage> = registry.iter().map(|o| o.stage()).collect();
        assert!(stages.contains(&YangStage::Stage0Coplanar));
        assert!(stages.contains(&YangStage::Stage1Bijective));
        assert!(stages.contains(&YangStage::Stage2Arrangement));
        assert!(stages.contains(&YangStage::Stage4bClassification));
        assert!(stages.contains(&YangStage::Stage5PatchSegment));
        assert!(stages.contains(&YangStage::Stage6Assembly));
    }

    #[test]
    fn default_registry_on_empty_state_passes() {
        // All snapshots None → every oracle in the default registry should
        // self-skip and report Ok. This test guards against any future
        // oracle that incorrectly reports StateMissing on a nominally-empty
        // pipeline state.
        let state = PipelineState::empty();
        let registry = default_oracle_registry();
        let result = run_pipeline_oracles("empty", &state, &registry);
        assert!(
            result.first_failing_stage.is_none(),
            "expected no failures on empty state, got first_failing_stage = {:?}",
            result.first_failing_stage
        );
        assert_eq!(result.per_oracle.len(), 6);
    }

    #[test]
    fn pipeline_state_empty_has_all_none() {
        let state = PipelineState::empty();
        assert!(state.stage_0_coplanar.is_none());
        assert!(state.stage_1_bijective.is_none());
        assert!(state.stage_2_subdivided.is_none());
        assert!(state.stage_4b_labeling.is_none());
        assert!(state.stage_5_face_survival.is_none());
        assert!(state.stage_6_result_topology.is_none());
    }
}
