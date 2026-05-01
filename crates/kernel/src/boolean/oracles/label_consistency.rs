//! Stage 4b oracle — `LabelConsistencyWithinPatchOracle`.
//!
//! Checks the Cherchi 2022 §5 / Algorithm 1 invariant: within a single
//! manifold patch (per `ManifoldPatchGraph`), every sub-triangle must share
//! one `CellLabel`. This is the contract that lets Cherchi's algorithm
//! "scale with the number of patches in the arrangement and not with the
//! number of triangles" — one ray-cast labels the whole patch, and the
//! manifold-edge graph propagates that label.
//!
//! ## Audit context (Cluster Y-I)
//!
//! This oracle directly tests the architectural defect documented as
//! Cluster Y-I in `docs/audits/yang_audit_2026-04-30.md`:
//!
//! - **YA-01** (`yang_audit_a_yang_pipeline.md`) — `label_cells` currently
//!   loops every sub-triangle, not every patch. The labeling stage runs at
//!   the wrong granularity.
//! - **YC-05** (`yang_audit_c_cherchi2022.md`) — Cherchi 2022 §5's headline
//!   complexity claim is forfeited because we do not propagate one label
//!   per patch.
//! - **YB-01** (`yang_audit_b_assay_failures.md`) — the 92-case
//!   `YANG-ERR-twin-validation` failure bucket (58.6% of the assay corpus)
//!   is the downstream symptom of mixed-label patches feeding incorrect
//!   half-edge pairing into `flood_fill_patches` / B-Rep assembly.
//!
//! ## Expected behavior on the corpus (PR11+)
//!
//! Post-PR11 the per-patch invariant holds **by construction**: `label_cells`
//! now ray-casts a single representative sub-tri per manifold-edge-bounded
//! patch (Cherchi 2022 §5 Algorithm 1) and propagates the resulting label
//! to every member of that patch. This oracle therefore transitions from
//! defect detector to *builder sentinel*: it is expected to pass on every
//! case where Stage 2 + Stage 4b populate their snapshots, and a future
//! firing would indicate a regression in either `label_cells`'s per-patch
//! propagation or `build_manifold_patch_graph`'s patch decomposition. The
//! oracle stays in the suite for that defense-in-depth role; PR11 makes no
//! behavioral change to this oracle (only this docstring).
//!
//! Refs: Yang 2025 §4.4.2; Cherchi 2022 §5 + Algorithm 1;
//! `specs/yang_per_patch_labeling.md` §5 (Oracles).

use crate::boolean::exact_mesh::{build_manifold_patch_graph, CellLabel};
use crate::boolean::pipeline_oracles::{
    OracleViolation, PipelineState, StageOracle, ViolationKind, YangStage,
};

/// Stable discriminant for `CellLabel` — used as a `BTreeSet` key so the
/// oracle remains deterministic even though `CellLabel` does not derive
/// `Ord`. Adding `Ord` to `CellLabel` would require touching
/// `exact_mesh.rs`, which this oracle is forbidden from modifying.
fn cell_label_key(label: CellLabel) -> u8 {
    match label {
        CellLabel::Inside => 0,
        CellLabel::Outside => 1,
    }
}

/// Stage 4b oracle — within each manifold patch, all sub-triangle labels
/// must be identical (Cherchi 2022 §5 / Algorithm 1).
///
/// Construction: builds the manifold-edge patch graph from
/// `state.stage_2_subdivided` (PR8's `build_manifold_patch_graph`). For each
/// patch, looks up every member sub-triangle's `CellLabel` from
/// `state.stage_4b_labeling`. If any patch contains more than one distinct
/// label, the contract is violated and the oracle reports the patch index,
/// the distinct labels seen, and a diagnostic sample of member sub-tri
/// indices (so PR10 can reproduce).
///
/// Skips silently if either snapshot is `None` — per harness convention,
/// missing snapshots are not themselves contract failures unless the
/// oracle's contract IS "this snapshot must be present".
pub(crate) struct LabelConsistencyWithinPatchOracle;

impl StageOracle for LabelConsistencyWithinPatchOracle {
    fn stage(&self) -> YangStage {
        YangStage::Stage4bClassification
    }

    fn name(&self) -> &'static str {
        "LabelConsistencyWithinPatchOracle"
    }

    fn cherchi_section(&self) -> Option<&'static str> {
        Some("Cherchi 2022 §5 + Algorithm 1")
    }

    fn check(&self, state: &PipelineState) -> Result<(), OracleViolation> {
        let subdivided = match state.stage_2_subdivided.as_ref() {
            Some(s) => s,
            None => return Ok(()), // Skip when Stage 2 didn't run.
        };
        let labeling = match state.stage_4b_labeling.as_ref() {
            Some(l) => l,
            None => return Ok(()), // Skip when Stage 4b didn't run.
        };

        // Length sanity: labelings must align with the subdivided mesh.
        // A mismatch is itself a contract violation — report as
        // ContractViolated rather than panicking.
        if labeling.labels_a.len() != subdivided.tris_a.len()
            || labeling.labels_b.len() != subdivided.tris_b.len()
        {
            return Err(OracleViolation {
                stage: self.stage(),
                oracle_name: self.name(),
                message: format!(
                    "labeling length mismatch: labels_a {} vs tris_a {}, \
                     labels_b {} vs tris_b {}",
                    labeling.labels_a.len(),
                    subdivided.tris_a.len(),
                    labeling.labels_b.len(),
                    subdivided.tris_b.len(),
                ),
                kind: ViolationKind::ContractViolated,
            });
        }

        let graph = build_manifold_patch_graph(subdivided);

        for (patch_idx, patch) in graph.patches.iter().enumerate() {
            // `CellLabel` is binary (Inside / Outside) — use a 2-slot
            // bitset over the discriminant. Determinism: insertion order
            // is irrelevant; we report based on the bitset state.
            let mut seen: [bool; 2] = [false, false];
            let mut first_label: Option<CellLabel> = None;
            let mut second_label: Option<CellLabel> = None;
            for &flat_idx in patch {
                let label = if flat_idx < graph.tris_a_count {
                    labeling.labels_a[flat_idx]
                } else {
                    labeling.labels_b[flat_idx - graph.tris_a_count]
                };
                let key = cell_label_key(label) as usize;
                if !seen[key] {
                    seen[key] = true;
                    if first_label.is_none() {
                        first_label = Some(label);
                    } else {
                        second_label = Some(label);
                    }
                }
            }
            if seen[0] && seen[1] {
                // Mixed labels — order distinct labels by stable
                // discriminant (Inside before Outside) for deterministic
                // diagnostic output.
                let mut labels_sorted: Vec<CellLabel> =
                    [first_label, second_label].into_iter().flatten().collect();
                labels_sorted.sort_by_key(|&l| cell_label_key(l));
                // Sample up to 4 sub-tri indices for diagnostic — patch
                // members are already in deterministic BFS order.
                let sample: Vec<usize> = patch.iter().take(4).copied().collect();
                return Err(OracleViolation {
                    stage: self.stage(),
                    oracle_name: self.name(),
                    message: format!(
                        "patch {} contains {} distinct labels {:?} across {} sub-tris \
                         (Cherchi 2022 §5 Algorithm 1 requires one label per patch); \
                         sample flat sub-tri indices: {:?} (tris_a_count = {})",
                        patch_idx,
                        labels_sorted.len(),
                        labels_sorted,
                        patch.len(),
                        sample,
                        graph.tris_a_count,
                    ),
                    kind: ViolationKind::ContractViolated,
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::exact_mesh::{CellLabeling, SubTriangle, SubdividedMesh};

    /// Two adjacent sub-triangles in mesh A sharing one edge (closed-loop
    /// fixture used to make a single patch via the manifold-edge BFS).
    /// Mesh B is empty so the only patch members come from mesh A.
    ///
    /// Geometry:
    ///   v0 ── v1
    ///   │  ╲   │
    ///   │   ╲  │
    ///   v2 ── v3
    /// tris_a = [(0,1,2), (1,3,2)]  — share edge (1,2).
    /// Edge (1,2) has incidence 2 → manifold; all other edges have
    /// incidence 1 → barriers, but BFS still groups via the manifold edge.
    fn two_adjacent_sub_tris_mesh_a_only() -> SubdividedMesh {
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
            tris_b: Vec::new(),
            params_a: vec![None; 4],
            params_b: vec![None; 4],
        }
    }

    // ── KNOWN-PASS: every patch has uniform labels ──────────────────────

    #[test]
    fn passes_when_every_patch_has_uniform_labels() {
        let subdivided = two_adjacent_sub_tris_mesh_a_only();
        // Both sub-tris in mesh A get the same label. Whatever patch
        // grouping the graph produces, no patch will have mixed labels.
        let labeling = CellLabeling {
            labels_a: vec![CellLabel::Inside, CellLabel::Inside],
            labels_b: Vec::new(),
        };
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(subdivided);
        state.stage_4b_labeling = Some(labeling);

        let oracle = LabelConsistencyWithinPatchOracle;
        let verdict = oracle.check(&state);
        assert!(
            verdict.is_ok(),
            "expected Ok for uniform-label patches, got {verdict:?}"
        );
    }

    // ── KNOWN-FAIL: one patch has mixed labels ──────────────────────────

    #[test]
    fn rejects_when_a_patch_has_mixed_labels() {
        // Same fixture: two mesh-A sub-tris connected by a single manifold
        // edge → one patch of 2 sub-tris. Assign Inside vs Outside to
        // them — guaranteed to land in the same patch with mixed labels.
        let subdivided = two_adjacent_sub_tris_mesh_a_only();
        let labeling = CellLabeling {
            labels_a: vec![CellLabel::Inside, CellLabel::Outside],
            labels_b: Vec::new(),
        };
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(subdivided);
        state.stage_4b_labeling = Some(labeling);

        let oracle = LabelConsistencyWithinPatchOracle;
        let verdict = oracle.check(&state);
        let violation = verdict.expect_err("expected Err for mixed-label patch");
        assert_eq!(violation.stage, YangStage::Stage4bClassification);
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert_eq!(violation.oracle_name, "LabelConsistencyWithinPatchOracle");
        assert!(
            violation.message.contains("distinct labels"),
            "violation message must describe mixed labels: {}",
            violation.message
        );
        assert!(
            violation.message.contains("patch"),
            "violation message must name the offending patch: {}",
            violation.message
        );
    }

    // ── KNOWN-FAIL with a richer fixture: three sub-tris, mixed labels ──

    /// Three sub-tris sharing a fan vertex — each adjacent pair shares one
    /// edge (incidence 2, manifold). Yields a single patch of 3.
    ///
    ///        v3
    ///       / │\
    ///      /  │ \
    ///   v0 ── v1── v2 (shared apex at v1; tris fan around (1, ?, ?))
    ///
    /// Triangles: (0,1,3), (1,2,3) share edge (1,3); (0,1,3) and (0,?,?)
    /// — keep simple: just two tris sharing one manifold edge plus a third
    /// tri attached via another manifold edge.
    fn three_sub_tris_chain_mesh_a_only() -> SubdividedMesh {
        // Verts laid out so each adjacent pair shares exactly one edge.
        // tri0 = (0,1,2)  tri1 = (1,3,2) share edge (1,2)  tri2 = (1,4,3)
        // share edge (1,3) with tri1.
        SubdividedMesh {
            verts: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.5, 1.0, 0.0],
                [2.0, 1.0, 0.0],
                [2.5, 0.0, 0.0],
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
                SubTriangle {
                    verts: [1, 4, 3],
                    parent_tri: 0,
                    cosurface_orientation: None,
                },
            ],
            tris_b: Vec::new(),
            params_a: vec![None; 5],
            params_b: vec![None; 5],
        }
    }

    #[test]
    fn rejects_two_inside_one_outside_in_a_patch() {
        let subdivided = three_sub_tris_chain_mesh_a_only();
        // Two Inside + one Outside in the same patch.
        let labeling = CellLabeling {
            labels_a: vec![CellLabel::Inside, CellLabel::Inside, CellLabel::Outside],
            labels_b: Vec::new(),
        };
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(subdivided);
        state.stage_4b_labeling = Some(labeling);

        let oracle = LabelConsistencyWithinPatchOracle;
        let verdict = oracle.check(&state);
        let violation = verdict.expect_err("expected Err for 2I+1O patch");
        assert_eq!(violation.stage, YangStage::Stage4bClassification);
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        // Message should mention the patch and mixed-label count.
        assert!(violation.message.contains("patch 0"));
    }

    // ── Skip behaviour ──────────────────────────────────────────────────

    #[test]
    fn skips_when_stage2_snapshot_missing() {
        let mut state = PipelineState::empty();
        // Stage 4b populated but Stage 2 missing.
        state.stage_4b_labeling = Some(CellLabeling {
            labels_a: Vec::new(),
            labels_b: Vec::new(),
        });
        let oracle = LabelConsistencyWithinPatchOracle;
        assert!(oracle.check(&state).is_ok());
    }

    #[test]
    fn skips_when_stage4b_snapshot_missing() {
        let mut state = PipelineState::empty();
        // Stage 2 populated but Stage 4b missing.
        state.stage_2_subdivided = Some(two_adjacent_sub_tris_mesh_a_only());
        let oracle = LabelConsistencyWithinPatchOracle;
        assert!(oracle.check(&state).is_ok());
    }

    // ── Length-mismatch contract ────────────────────────────────────────

    #[test]
    fn rejects_when_labels_length_mismatches_tri_count() {
        let subdivided = two_adjacent_sub_tris_mesh_a_only();
        // labels_a has 1 entry but tris_a has 2 — contract violation.
        let labeling = CellLabeling {
            labels_a: vec![CellLabel::Inside],
            labels_b: Vec::new(),
        };
        let mut state = PipelineState::empty();
        state.stage_2_subdivided = Some(subdivided);
        state.stage_4b_labeling = Some(labeling);
        let oracle = LabelConsistencyWithinPatchOracle;
        let verdict = oracle.check(&state);
        let violation = verdict.expect_err("expected Err for length mismatch");
        assert_eq!(violation.kind, ViolationKind::ContractViolated);
        assert!(violation.message.contains("labeling length mismatch"));
    }

    // ── Stage / metadata ────────────────────────────────────────────────

    #[test]
    fn stage_and_section_metadata() {
        let oracle = LabelConsistencyWithinPatchOracle;
        assert_eq!(oracle.stage(), YangStage::Stage4bClassification);
        assert_eq!(oracle.name(), "LabelConsistencyWithinPatchOracle");
        assert_eq!(
            oracle.cherchi_section(),
            Some("Cherchi 2022 §5 + Algorithm 1")
        );
        assert!(oracle.yang_section().contains("§4.4.2"));
    }
}
