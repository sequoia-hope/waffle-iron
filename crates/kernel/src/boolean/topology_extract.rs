//! Phase 3, Task 3a — Face survival detection.
//!
//! After the exact mesh boolean (Phase 2) selects which sub-triangles survive,
//! this module determines which original B-Rep faces those sub-triangles came
//! from. Groups the surviving sub-triangles by their source face, producing a
//! `FaceSurvivalMap` that Phase 3b–3d will consume to extract trim boundaries
//! and build the result B-Rep.
//!
//! Ref [#24]: Yang, Jia & Yan (2025) — Stage 3 of the hybrid pipeline.
//! Ref [#9]: Cherchi et al. (2020) — parent triangle provenance.

use std::collections::BTreeMap;

use crate::boolean::exact_mesh::{CellLabel, CellLabeling, MeshBooleanOp, MeshId, SubdividedMesh};
use crate::tessellation::bijective::BijectiveMap;
use crate::topology::half_edge::FaceIdx;

/// Key identifying a source B-Rep face in the boolean result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(dead_code)] // Phase 3 building block — task 3a
pub(crate) struct SourceFace {
    pub mesh_id: MeshId,
    pub face_idx: FaceIdx,
}

/// A surviving sub-triangle in the boolean result, with provenance.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 3 building block — task 3a
pub(crate) struct SurvivingSubTri {
    /// Vertex indices in SubdividedMesh.verts.
    pub verts: [usize; 3],
    /// Whether winding was flipped (Subtract B-inside-A).
    pub flipped: bool,
}

/// Maps each surviving source face to its contributing sub-triangles.
/// Produced by face_survival_detect(), consumed by Phase 3b trim boundary extraction.
#[derive(Debug)]
#[allow(dead_code)] // Phase 3 building block — task 3a
pub(crate) struct FaceSurvivalMap {
    /// Keyed by (MeshId, FaceIdx), value is the sub-triangles from that face.
    pub groups: BTreeMap<SourceFace, Vec<SurvivingSubTri>>,
}

/// Detect which original B-Rep faces survive in the boolean result.
///
/// Walks the subdivided mesh's sub-triangles, applies the boolean selection
/// table (same logic as `select_boolean_result`), and groups surviving
/// sub-triangles by their source B-Rep face using the bijective maps.
#[allow(dead_code)] // Phase 3 building block — task 3a
pub(crate) fn face_survival_detect(
    subdivided: &SubdividedMesh,
    labeling: &CellLabeling,
    op: MeshBooleanOp,
    bijective_a: &BijectiveMap,
    bijective_b: &BijectiveMap,
) -> FaceSurvivalMap {
    let mut groups: BTreeMap<SourceFace, Vec<SurvivingSubTri>> = BTreeMap::new();

    // Determine which cell labels to keep for A and B sub-triangles.
    // Selection table matches `select_boolean_result` in exact_mesh.rs.
    // Ref #24: Yang 2025 — boolean op cell selection table.
    let (keep_a, keep_b, flip_b) = match op {
        MeshBooleanOp::Union => (CellLabel::Outside, CellLabel::Outside, false),
        MeshBooleanOp::Subtract => (CellLabel::Outside, CellLabel::Inside, true),
        MeshBooleanOp::Intersect => (CellLabel::Inside, CellLabel::Inside, false),
    };

    // Process A sub-triangles: look up source face via bijective_a.
    // Ref #9: Cherchi 2020 — parent triangle provenance through subdivision.
    for (sub_tri, label) in subdivided.tris_a.iter().zip(labeling.labels_a.iter()) {
        if *label == keep_a {
            let face_idx = bijective_a.tri_face_ids[sub_tri.parent_tri];
            let key = SourceFace {
                mesh_id: MeshId::A,
                face_idx,
            };
            groups.entry(key).or_default().push(SurvivingSubTri {
                verts: sub_tri.verts,
                flipped: false, // A sub-triangles are never flipped
            });
        }
    }

    // Process B sub-triangles: look up source face via bijective_b.
    // For Subtract, B-inside-A triangles get flipped winding to point normals outward.
    // Ref #24: Yang 2025 — Subtract reverses B-face normals.
    for (sub_tri, label) in subdivided.tris_b.iter().zip(labeling.labels_b.iter()) {
        if *label == keep_b {
            let face_idx = bijective_b.tri_face_ids[sub_tri.parent_tri];
            let key = SourceFace {
                mesh_id: MeshId::B,
                face_idx,
            };
            groups.entry(key).or_default().push(SurvivingSubTri {
                verts: sub_tri.verts,
                flipped: flip_b, // Only true for Subtract B-inside-A
            });
        }
    }

    FaceSurvivalMap { groups }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::exact_mesh::{
        label_cells, select_boolean_result, subdivide_mesh_pair, CellLabeling, SubTriangle,
        SubdividedMesh,
    };

    // ── Test helpers ──

    /// Build a box mesh with 8 vertices and 12 triangles (2 per face).
    /// Returns (vertices, triangle index arrays).
    fn make_box_mesh(min: [f64; 3], max: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let verts = vec![
            [x0, y0, z0], // 0: left-bottom-back
            [x1, y0, z0], // 1: right-bottom-back
            [x1, y1, z0], // 2: right-top-back
            [x0, y1, z0], // 3: left-top-back
            [x0, y0, z1], // 4: left-bottom-front
            [x1, y0, z1], // 5: right-bottom-front
            [x1, y1, z1], // 6: right-top-front
            [x0, y1, z1], // 7: left-top-front
        ];
        // 12 triangles, 2 per face, outward-facing (CCW from outside)
        let tris = vec![
            // Back face (z=z0) — face 0
            [0, 2, 1],
            [0, 3, 2],
            // Front face (z=z1) — face 1
            [4, 5, 6],
            [4, 6, 7],
            // Bottom face (y=y0) — face 2
            [0, 1, 5],
            [0, 5, 4],
            // Top face (y=y1) — face 3
            [3, 6, 2],
            [3, 7, 6],
            // Left face (x=x0) — face 4
            [0, 4, 7],
            [0, 7, 3],
            // Right face (x=x1) — face 5
            [1, 2, 6],
            [1, 6, 5],
        ];
        (verts, tris)
    }

    /// Count how many sub-triangles are selected by `select_boolean_result`.
    /// The function returns flat vertex coords (3 per tri), so count / 9 = tri count.
    fn count_selected_tris(
        subdivided: &SubdividedMesh,
        labeling: &CellLabeling,
        op: MeshBooleanOp,
    ) -> usize {
        let result = select_boolean_result(subdivided, labeling, op);
        // Each selected triangle emits 3 vertices of 3 floats = 9 values per tri,
        // but result is Vec<[f64;3]>, so 3 entries per triangle.
        result.len() / 3
    }

    /// Run the full Phase 2 pipeline for two overlapping boxes and return
    /// all intermediate products needed by face_survival_detect.
    fn run_overlapping_box_pipeline(
        _op: MeshBooleanOp,
    ) -> (SubdividedMesh, CellLabeling, BijectiveMap, BijectiveMap) {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);

        // Build bijective maps: for each sub-triangle, look up its parent_tri,
        // then map that to a face index via the box's 2-tris-per-face scheme.
        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        (subdivided, labeling, bijective_a, bijective_b)
    }

    /// Build a BijectiveMap that covers all sub-triangles from a subdivision.
    /// For a box mesh, parent_tri ∈ [0..12), and face = parent_tri / 2.
    /// The bijective map must have one entry per *original* triangle.
    fn build_bijective_from_subdivided(
        sub_tris: &[SubTriangle],
        original_tri_count: usize,
    ) -> BijectiveMap {
        // Build a map for the original mesh: tri_index → face_index
        // Box mesh: 12 tris, 2 per face → face = tri / 2
        let tri_face_ids: Vec<FaceIdx> = (0..original_tri_count).map(|i| FaceIdx(i / 2)).collect();

        // Verify all sub-triangle parent_tri values are in range
        for st in sub_tris {
            assert!(
                st.parent_tri < original_tri_count,
                "Sub-triangle parent_tri {} out of range for original mesh with {} triangles",
                st.parent_tri,
                original_tri_count,
            );
        }

        BijectiveMap { tri_face_ids }
    }

    // ── Test 1: Conservation ──
    // The total number of surviving sub-triangles in the FaceSurvivalMap must
    // equal the count selected by select_boolean_result.

    #[test]
    fn test_conservation() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        let survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);
        let selected_count = count_selected_tris(&subdivided, &labeling, MeshBooleanOp::Union);

        let survival_count: usize = survival.groups.values().map(|v| v.len()).sum();

        // selected_count should be > 0 for overlapping boxes with Union
        assert!(
            selected_count > 0,
            "select_boolean_result should select some triangles for Union of overlapping boxes"
        );
        assert_eq!(
            survival_count, selected_count,
            "Conservation: survival map has {survival_count} sub-tris but select_boolean_result has {selected_count}"
        );
    }

    // ── Test 2: Box subtract produces non-empty face groups ──

    #[test]
    fn test_box_subtract_face_groups() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        assert!(
            !survival.groups.is_empty(),
            "Subtract of overlapping boxes must produce at least one face group"
        );

        // Per the spec oracle: we expect contributions from both mesh A and mesh B
        let has_a = survival.groups.keys().any(|k| k.mesh_id == MeshId::A);
        let has_b = survival.groups.keys().any(|k| k.mesh_id == MeshId::B);
        assert!(has_a, "Subtract result must include faces from mesh A");
        assert!(has_b, "Subtract result must include faces from mesh B");
    }

    // ── Test 3: No empty groups (combined with non-emptiness check) ──

    #[test]
    fn test_no_empty_groups() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        // Must have groups (otherwise trivially passes the "no empty" check)
        assert!(
            !survival.groups.is_empty(),
            "Must have at least one face group to test the no-empty-groups invariant"
        );

        for (source_face, tris) in &survival.groups {
            assert!(
                !tris.is_empty(),
                "Face group {:?} must not be empty",
                source_face,
            );
        }
    }

    // ── Test 4: Bijective validity ──
    // All parent_tri → FaceIdx mappings should be valid (not usize::MAX sentinel).

    #[test]
    fn test_bijective_validity() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        let survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        // Must have groups to check validity
        assert!(
            !survival.groups.is_empty(),
            "Must have face groups to verify bijective validity"
        );

        for (source_face, _tris) in &survival.groups {
            assert_ne!(
                source_face.face_idx,
                FaceIdx(usize::MAX),
                "Source face {:?} has sentinel FaceIdx(usize::MAX) — bijective map is invalid",
                source_face,
            );
        }
    }

    // ── Test 5: Union face groups ──

    #[test]
    fn test_union_face_groups() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        let survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        assert!(
            !survival.groups.is_empty(),
            "Union of overlapping boxes must produce face groups"
        );

        // Union should include faces from both meshes
        let a_count = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::A)
            .count();
        let b_count = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::B)
            .count();
        assert!(a_count > 0, "Union must include faces from mesh A");
        assert!(b_count > 0, "Union must include faces from mesh B");

        // At most 12 source faces (6 per box)
        let total_faces = a_count + b_count;
        assert!(
            total_faces <= 12,
            "Union of two boxes cannot have more than 12 source faces, got {total_faces}"
        );
    }

    // ── Test 6: Intersect face groups ──

    #[test]
    fn test_intersect_face_groups() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Intersect);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Intersect,
            &bij_a,
            &bij_b,
        );

        assert!(
            !survival.groups.is_empty(),
            "Intersect of overlapping boxes must produce face groups"
        );

        // Intersection includes faces from both meshes
        let has_a = survival.groups.keys().any(|k| k.mesh_id == MeshId::A);
        let has_b = survival.groups.keys().any(|k| k.mesh_id == MeshId::B);
        assert!(has_a, "Intersect result must include faces from mesh A");
        assert!(has_b, "Intersect result must include faces from mesh B");
    }

    // ── Test 7: Subtract B-faces have flipped = true ──

    #[test]
    fn test_subtract_b_faces_flipped() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        // Must have B-mesh groups to check flip status
        let b_groups: Vec<_> = survival
            .groups
            .iter()
            .filter(|(k, _)| k.mesh_id == MeshId::B)
            .collect();

        assert!(
            !b_groups.is_empty(),
            "Subtract must have B-mesh face groups to verify flip status"
        );

        for (source_face, tris) in &b_groups {
            for (i, tri) in tris.iter().enumerate() {
                assert!(
                    tri.flipped,
                    "Subtract B-mesh sub-tri {i} in face {:?} should have flipped=true, got false",
                    source_face,
                );
            }
        }

        // Also check that A-mesh sub-triangles are NOT flipped
        let a_groups: Vec<_> = survival
            .groups
            .iter()
            .filter(|(k, _)| k.mesh_id == MeshId::A)
            .collect();

        for (source_face, tris) in &a_groups {
            for (i, tri) in tris.iter().enumerate() {
                assert!(
                    !tri.flipped,
                    "Subtract A-mesh sub-tri {i} in face {:?} should have flipped=false, got true",
                    source_face,
                );
            }
        }
    }

    // ── Adversarial helper ──

    /// Run the full Phase 2 pipeline for an arbitrary pair of boxes.
    fn run_box_pair_pipeline(
        min_a: [f64; 3],
        max_a: [f64; 3],
        min_b: [f64; 3],
        max_b: [f64; 3],
    ) -> (SubdividedMesh, CellLabeling, BijectiveMap, BijectiveMap) {
        let (verts_a, tris_a) = make_box_mesh(min_a, max_a);
        let (verts_b, tris_b) = make_box_mesh(min_b, max_b);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b);
        let labeling = label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b);

        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        (subdivided, labeling, bijective_a, bijective_b)
    }

    // ── Test 8: Empty input produces empty survival map ──

    #[test]
    fn test_empty_input() {
        let subdivided = SubdividedMesh {
            verts: vec![],
            tris_a: vec![],
            tris_b: vec![],
        };
        let labeling = CellLabeling {
            labels_a: vec![],
            labels_b: vec![],
        };
        let bij_a = BijectiveMap {
            tri_face_ids: vec![],
        };
        let bij_b = BijectiveMap {
            tri_face_ids: vec![],
        };

        let survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        assert!(
            survival.groups.is_empty(),
            "Empty input must produce empty FaceSurvivalMap"
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Adversarial / pathological tests (FIP Phase 4)
    // ══════════════════════════════════════════════════════════════════

    // ── A1: Touching boxes (shared face, zero overlap volume) ──
    // Box A = [0,0,0]-[1,1,1], Box B = [1,0,0]-[2,1,1]. They share x=1.
    // Subtract: B doesn't cut into A, so all surviving faces must be from A only.

    #[test]
    fn test_touching_boxes_subtract() {
        let (subdivided, labeling, bij_a, bij_b) = run_box_pair_pipeline(
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [2.0, 1.0, 1.0],
        );

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        // Touching boxes share the x=1 plane. B doesn't penetrate A's interior,
        // so the subtract result should be dominated by A's faces.
        assert!(
            !survival.groups.is_empty(),
            "Touching-box subtract must produce face groups"
        );

        // All of A's 6 faces should survive.
        let a_face_count = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::A)
            .count();
        assert_eq!(
            a_face_count, 6,
            "Touching-box subtract: all 6 faces of A should survive, got {a_face_count}"
        );

        // Conservation: survival count must match select_boolean_result count.
        let survival_count: usize = survival.groups.values().map(|v| v.len()).sum();
        let selected_count = count_selected_tris(&subdivided, &labeling, MeshBooleanOp::Subtract);
        assert_eq!(
            survival_count, selected_count,
            "Touching-box subtract: conservation violation — survival {survival_count} \
             vs selected {selected_count}"
        );

        // No NaN in any surviving sub-triangle vertex coordinates.
        for tris in survival.groups.values() {
            for tri in tris {
                for &vi in &tri.verts {
                    let v = subdivided.verts[vi];
                    assert!(
                        !v[0].is_nan() && !v[1].is_nan() && !v[2].is_nan(),
                        "NaN detected in surviving sub-triangle vertex {vi}: {v:?}"
                    );
                }
            }
        }

        // Face indices must be in range [0, 5].
        for source_face in survival.groups.keys() {
            assert!(
                source_face.face_idx.0 <= 5,
                "Touching-box subtract: FaceIdx {} out of range for {:?}",
                source_face.face_idx.0,
                source_face.mesh_id,
            );
        }
    }

    // ── A2: Identical boxes ──
    // Two identical boxes [0,0,0]-[1,1,1].
    // Union: result == one box. Intersect: result == one box.

    #[test]
    fn test_identical_boxes() {
        let (subdivided, labeling, bij_a, bij_b) = run_box_pair_pipeline(
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        );

        // Identical boxes are a degenerate boundary case for GWN classification.
        // Centroids on shared faces may be classified as Inside or Outside
        // depending on numerical perturbation. We verify structural invariants
        // rather than exact face counts.

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);

            // Conservation: survival count == select_boolean_result count.
            let survival_count: usize = survival.groups.values().map(|v| v.len()).sum();
            let selected_count = count_selected_tris(&subdivided, &labeling, op);
            assert_eq!(
                survival_count, selected_count,
                "Identical boxes {op:?}: conservation violation — \
                 survival {survival_count} vs selected {selected_count}"
            );

            // No empty groups.
            for (source_face, tris) in &survival.groups {
                assert!(
                    !tris.is_empty(),
                    "Identical boxes {op:?}: empty group for {source_face:?}"
                );
            }

            // No NaN in vertex coordinates.
            for tris in survival.groups.values() {
                for tri in tris {
                    for &vi in &tri.verts {
                        let v = subdivided.verts[vi];
                        assert!(
                            !v[0].is_nan() && !v[1].is_nan() && !v[2].is_nan(),
                            "Identical boxes {op:?}: NaN in vertex {vi}: {v:?}"
                        );
                    }
                }
            }

            // Face indices in range [0, 5].
            for source_face in survival.groups.keys() {
                assert!(
                    source_face.face_idx.0 <= 5,
                    "Identical boxes {op:?}: FaceIdx {} out of range",
                    source_face.face_idx.0,
                );
            }

            // No sentinel FaceIdx values.
            for source_face in survival.groups.keys() {
                assert_ne!(
                    source_face.face_idx,
                    FaceIdx(usize::MAX),
                    "Identical boxes {op:?}: sentinel FaceIdx in {source_face:?}"
                );
            }
        }
    }

    // ── A3: Conservation for all three ops ──

    #[test]
    fn test_conservation_all_ops() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);
            let selected_count = count_selected_tris(&subdivided, &labeling, op);
            let survival_count: usize = survival.groups.values().map(|v| v.len()).sum();

            assert!(
                selected_count > 0,
                "select_boolean_result should select some triangles for {op:?}"
            );
            assert_eq!(
                survival_count, selected_count,
                "Conservation violation for {op:?}: survival map has {survival_count} \
                 sub-tris but select_boolean_result has {selected_count}"
            );
        }
    }

    // ── A4: Face index range ──
    // For overlapping boxes with 6 faces each (FaceIdx 0..5), all face indices
    // in the survival map must be in [0, 5].

    #[test]
    fn test_face_idx_range() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);

            for source_face in survival.groups.keys() {
                assert!(
                    source_face.face_idx.0 <= 5,
                    "{op:?}: FaceIdx {} out of range [0,5] for {:?}",
                    source_face.face_idx.0,
                    source_face.mesh_id,
                );
            }
        }
    }

    // ── A5: No duplicate sub-triangles across face groups ──
    // The same sub-triangle vertex set must not appear in two different face groups.

    #[test]
    fn test_no_duplicate_subtris() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);

            // Collect all (sorted vertex triple, source face) pairs.
            let mut seen: std::collections::HashSet<[usize; 3]> = std::collections::HashSet::new();

            for (source_face, tris) in &survival.groups {
                for tri in tris {
                    let mut key = tri.verts;
                    key.sort();
                    assert!(
                        seen.insert(key),
                        "{op:?}: duplicate sub-triangle {key:?} found in face group {:?} \
                         — violates conservation (same tri in two face groups)",
                        source_face,
                    );
                }
            }
        }
    }
}
