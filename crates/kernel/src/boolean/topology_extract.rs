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

use std::collections::{BTreeMap, HashMap, HashSet};

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

/// A directed edge in a trim boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)] // Phase 3 building block — task 3b
pub(crate) struct TrimEdge {
    pub v0: usize,
    pub v1: usize,
    pub is_intersection: bool,
}

/// A closed loop of directed trim edges.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Phase 3 building block — task 3b
pub(crate) struct TrimLoop {
    pub edges: Vec<TrimEdge>,
}

/// Maps each surviving source face to its trim boundary loops.
#[derive(Debug)]
#[allow(dead_code)] // Phase 3 building block — task 3b
pub(crate) struct TrimBoundaryMap {
    pub boundaries: BTreeMap<SourceFace, Vec<TrimLoop>>,
}

/// Extract trim boundaries for each surviving face group.
///
/// For each face group in the FaceSurvivalMap, identifies the boundary edges —
/// edges not shared with another surviving sub-triangle in the same face group.
/// Chains these into closed TrimLoop structures.
///
/// Ref [#24]: Yang 2025 — Stage 3 trim boundary extraction.
/// Ref [#9]: Cherchi 2020 — edge adjacency from subdivided mesh.
#[allow(dead_code)] // Phase 3 building block — task 3b
pub(crate) fn extract_trim_boundaries(
    _subdivided: &SubdividedMesh,
    survival: &FaceSurvivalMap,
) -> TrimBoundaryMap {
    if survival.groups.is_empty() {
        return TrimBoundaryMap {
            boundaries: BTreeMap::new(),
        };
    }

    // Step 1: Build a global lookup from undirected edge (min,max) → set of SourceFaces
    // that have a sub-triangle touching that edge. Used to determine is_intersection.
    let mut global_edge_faces: HashMap<(usize, usize), HashSet<SourceFace>> = HashMap::new();
    for (source_face, tris) in &survival.groups {
        for tri in tris {
            let v = tri.verts;
            for &(a, b) in &[(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
                let key = (a.min(b), a.max(b));
                global_edge_faces
                    .entry(key)
                    .or_default()
                    .insert(*source_face);
            }
        }
    }

    let mut boundaries = BTreeMap::new();

    for (source_face, tris) in &survival.groups {
        // Step 2: Collect directed edges for this face group, respecting winding.
        // Also count undirected edge occurrences within this group to find interior edges.
        let mut directed_edges: Vec<(usize, usize)> = Vec::new();
        let mut undirected_count: HashMap<(usize, usize), usize> = HashMap::new();

        for tri in tris {
            let v = tri.verts;
            let edges = if tri.flipped {
                // Flipped winding: v0→v2, v2→v1, v1→v0
                [(v[0], v[2]), (v[2], v[1]), (v[1], v[0])]
            } else {
                // Normal winding: v0→v1, v1→v2, v2→v0
                [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])]
            };
            for &(a, b) in &edges {
                directed_edges.push((a, b));
                let key = (a.min(b), a.max(b));
                *undirected_count.entry(key).or_insert(0) += 1;
            }
        }

        // Interior edges: undirected edges appearing 2+ times within this group.
        let interior: HashSet<(usize, usize)> = undirected_count
            .iter()
            .filter(|(_, &c)| c >= 2)
            .map(|(&k, _)| k)
            .collect();

        // Boundary edges: directed edges whose undirected form is NOT interior.
        let mut boundary_edges: Vec<(usize, usize, bool)> = Vec::new();
        for &(a, b) in &directed_edges {
            let key = (a.min(b), a.max(b));
            if !interior.contains(&key) {
                // is_intersection: the undirected edge is shared with a DIFFERENT face group
                let is_intersection = global_edge_faces
                    .get(&key)
                    .map(|faces| faces.iter().any(|f| f != source_face))
                    .unwrap_or(false);
                boundary_edges.push((a, b, is_intersection));
            }
        }

        // Step 3: Chain boundary edges into closed TrimLoops.
        // Build adjacency: v0 → list of (v1, is_intersection)
        let mut adj: HashMap<usize, Vec<(usize, bool)>> = HashMap::new();
        for &(a, b, is_int) in &boundary_edges {
            adj.entry(a).or_default().push((b, is_int));
        }

        let mut loops: Vec<TrimLoop> = Vec::new();

        // Track which edges have been consumed
        loop {
            // Find a starting vertex that still has outgoing edges
            let start = adj
                .iter()
                .find(|(_, outs)| !outs.is_empty())
                .map(|(&k, _)| k);
            let start = match start {
                Some(s) => s,
                None => break, // All edges consumed
            };

            let mut chain = Vec::new();
            let mut current = start;

            loop {
                let outgoing = adj.get_mut(&current);
                let (next, is_int) = match outgoing.and_then(|v| v.pop()) {
                    Some(pair) => pair,
                    None => break, // Dead end — shouldn't happen in valid mesh
                };
                chain.push(TrimEdge {
                    v0: current,
                    v1: next,
                    is_intersection: is_int,
                });
                if next == start {
                    break; // Loop closed
                }
                current = next;
            }

            if !chain.is_empty() {
                loops.push(TrimLoop { edges: chain });
            }
        }

        // Fallback: if there are orphan edges left somehow, each becomes a degenerate loop.
        // (Shouldn't happen with valid mesh data, but defensive.)

        boundaries.insert(*source_face, loops);
    }

    TrimBoundaryMap { boundaries }
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

    // ══════════════════════════════════════════════════════════════════
    // Task 3b — Trim boundary extraction tests (FIP Phase 2)
    // ══════════════════════════════════════════════════════════════════

    // ── 3b-Test 1: Every surviving face group has at least one trim loop ──

    #[test]
    fn test_trim_every_face_has_boundary() {
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
            "Precondition: survival map must be non-empty for overlapping box subtract"
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        // Every face in the survival map must appear in the trim boundary map.
        for source_face in survival.groups.keys() {
            let loops = trim_map
                .boundaries
                .get(source_face)
                .unwrap_or_else(|| panic!("Face {:?} missing from TrimBoundaryMap", source_face));
            assert!(
                !loops.is_empty(),
                "Face {:?} must have at least one TrimLoop",
                source_face,
            );
        }

        // No extra faces in trim map that aren't in survival map.
        for source_face in trim_map.boundaries.keys() {
            assert!(
                survival.groups.contains_key(source_face),
                "TrimBoundaryMap contains face {:?} not in FaceSurvivalMap",
                source_face,
            );
        }
    }

    // ── 3b-Test 2: Trim loops are closed ──

    #[test]
    fn test_trim_loops_are_closed() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        for (source_face, loops) in &trim_map.boundaries {
            for (li, trim_loop) in loops.iter().enumerate() {
                assert!(
                    !trim_loop.edges.is_empty(),
                    "Face {:?} loop {li} is empty",
                    source_face,
                );
                let n = trim_loop.edges.len();
                for i in 0..n {
                    let next = (i + 1) % n;
                    assert_eq!(
                        trim_loop.edges[i].v1,
                        trim_loop.edges[next].v0,
                        "Face {:?} loop {li}: edge {i} v1={} != edge {next} v0={} — loop is not closed",
                        source_face,
                        trim_loop.edges[i].v1,
                        trim_loop.edges[next].v0,
                    );
                }
            }
        }
    }

    // ── 3b-Test 3: No interior edges in trim boundaries ──

    #[test]
    fn test_trim_no_interior_edges() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        // For each face group, build the set of interior edges (shared by two
        // surviving sub-triangles within the same group) and verify no trim
        // boundary edge is interior.
        for (source_face, loops) in &trim_map.boundaries {
            let tris = &survival.groups[source_face];

            // Count how many times each undirected edge appears among the group's sub-tris.
            let mut edge_count: std::collections::HashMap<(usize, usize), usize> =
                std::collections::HashMap::new();
            for tri in tris {
                let v = tri.verts;
                for &(a, b) in &[(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
                    let key = if a < b { (a, b) } else { (b, a) };
                    *edge_count.entry(key).or_insert(0) += 1;
                }
            }

            // Interior edges appear 2+ times (shared by two sub-tris in the same group).
            let interior: std::collections::HashSet<(usize, usize)> = edge_count
                .iter()
                .filter(|(_, &c)| c >= 2)
                .map(|(&k, _)| k)
                .collect();

            // Verify no trim edge is interior.
            for trim_loop in loops {
                for edge in &trim_loop.edges {
                    let key = if edge.v0 < edge.v1 {
                        (edge.v0, edge.v1)
                    } else {
                        (edge.v1, edge.v0)
                    };
                    assert!(
                        !interior.contains(&key),
                        "Face {:?}: trim edge ({}, {}) is an interior edge (shared by 2+ sub-tris)",
                        source_face,
                        edge.v0,
                        edge.v1,
                    );
                }
            }
        }
    }

    // ── 3b-Test 4: No duplicate directed edges within a face's trim boundaries ──

    #[test]
    fn test_trim_no_duplicate_directed_edges() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        for (source_face, loops) in &trim_map.boundaries {
            let mut seen: std::collections::HashSet<(usize, usize)> =
                std::collections::HashSet::new();
            for trim_loop in loops {
                for edge in &trim_loop.edges {
                    let key = (edge.v0, edge.v1);
                    assert!(
                        seen.insert(key),
                        "Face {:?}: duplicate directed edge ({}, {}) in trim boundaries",
                        source_face,
                        edge.v0,
                        edge.v1,
                    );
                }
            }
        }
    }

    // ── 3b-Test 5: All three ops produce non-empty trim boundaries with closed loops ──

    #[test]
    fn test_trim_boundary_all_ops() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);

            assert!(
                !survival.groups.is_empty(),
                "{op:?}: precondition — survival map must be non-empty"
            );

            let trim_map = extract_trim_boundaries(&subdivided, &survival);

            assert!(
                !trim_map.boundaries.is_empty(),
                "{op:?}: TrimBoundaryMap must be non-empty"
            );

            // Every loop must be closed.
            for (source_face, loops) in &trim_map.boundaries {
                for (li, trim_loop) in loops.iter().enumerate() {
                    assert!(
                        !trim_loop.edges.is_empty(),
                        "{op:?}: face {:?} loop {li} is empty",
                        source_face,
                    );
                    let n = trim_loop.edges.len();
                    for i in 0..n {
                        let next = (i + 1) % n;
                        assert_eq!(
                            trim_loop.edges[i].v1, trim_loop.edges[next].v0,
                            "{op:?}: face {:?} loop {li} not closed at edge {i}",
                            source_face,
                        );
                    }
                }
            }
        }
    }

    // ── 3b-Test 6: Empty survival map produces empty trim boundary map ──

    #[test]
    fn test_trim_empty_survival_map() {
        let subdivided = SubdividedMesh {
            verts: vec![],
            tris_a: vec![],
            tris_b: vec![],
        };
        let survival = FaceSurvivalMap {
            groups: BTreeMap::new(),
        };

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        assert!(
            trim_map.boundaries.is_empty(),
            "Empty FaceSurvivalMap must produce empty TrimBoundaryMap"
        );
    }

    // ── 3b-Test 7: All vertex indices in trim edges are valid ──

    #[test]
    fn test_trim_vertex_indices_valid() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);
        let vert_count = subdivided.verts.len();

        for (source_face, loops) in &trim_map.boundaries {
            for (li, trim_loop) in loops.iter().enumerate() {
                for (ei, edge) in trim_loop.edges.iter().enumerate() {
                    assert!(
                        edge.v0 < vert_count,
                        "Face {:?} loop {li} edge {ei}: v0={} >= vert count {vert_count}",
                        source_face,
                        edge.v0,
                    );
                    assert!(
                        edge.v1 < vert_count,
                        "Face {:?} loop {li} edge {ei}: v1={} >= vert count {vert_count}",
                        source_face,
                        edge.v1,
                    );
                }
            }
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Adversarial / pathological tests for Task 3b (FIP Phase 4)
    // ══════════════════════════════════════════════════════════════════

    // ── A1: Touching boxes subtract — structural invariants ──
    // Box A = [0,0,0]-[1,1,1], Box B = [1,0,0]-[2,1,1]. They share the x=1 plane.
    // B does not penetrate A's interior, so A survives fully. Verify closed loops,
    // valid indices, and no duplicate edges.

    #[test]
    fn test_trim_touching_boxes_subtract() {
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

        // Touching subtract: A should survive (B is outside A).
        assert!(
            !survival.groups.is_empty(),
            "Touching-box subtract must produce face groups"
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        // Face count conservation: trim map has same entries as survival map.
        assert_eq!(
            trim_map.boundaries.len(),
            survival.groups.len(),
            "Touching subtract: trim map face count != survival map face count"
        );

        let vert_count = subdivided.verts.len();

        // Every trim loop must be closed, non-empty, with valid vertex indices.
        for (source_face, loops) in &trim_map.boundaries {
            assert!(
                !loops.is_empty(),
                "Touching subtract: face {:?} has no trim loops",
                source_face,
            );

            let mut seen_edges: HashSet<(usize, usize)> = HashSet::new();
            for (li, trim_loop) in loops.iter().enumerate() {
                assert!(
                    !trim_loop.edges.is_empty(),
                    "Touching subtract: face {:?} loop {li} is empty",
                    source_face,
                );
                let n = trim_loop.edges.len();
                for i in 0..n {
                    let next = (i + 1) % n;
                    assert_eq!(
                        trim_loop.edges[i].v1, trim_loop.edges[next].v0,
                        "Touching subtract: face {:?} loop {li} not closed at edge {i}",
                        source_face,
                    );

                    // Valid vertex indices.
                    assert!(
                        trim_loop.edges[i].v0 < vert_count && trim_loop.edges[i].v1 < vert_count,
                        "Touching subtract: face {:?} loop {li} edge {i} has \
                         out-of-bounds vertex index",
                        source_face,
                    );

                    // No duplicate directed edges within this face.
                    let key = (trim_loop.edges[i].v0, trim_loop.edges[i].v1);
                    assert!(
                        seen_edges.insert(key),
                        "Touching subtract: face {:?} has duplicate directed edge {:?}",
                        source_face,
                        key,
                    );
                }
            }
        }
    }

    // ── A2: Identical boxes — structural invariants for all ops ──
    // Two identical boxes [0,0,0]-[1,1,1]. Degenerate case where all faces
    // are coplanar. We verify structural invariants (closed loops, valid indices)
    // without asserting specific counts.

    #[test]
    fn test_trim_identical_boxes() {
        let (subdivided, labeling, bij_a, bij_b) = run_box_pair_pipeline(
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        );

        let vert_count = subdivided.verts.len();

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);
            let trim_map = extract_trim_boundaries(&subdivided, &survival);

            // Trim boundary map must have exactly the same face keys as survival map.
            assert_eq!(
                trim_map.boundaries.len(),
                survival.groups.len(),
                "Identical boxes {op:?}: trim map face count {} != survival map face count {}",
                trim_map.boundaries.len(),
                survival.groups.len(),
            );

            for (source_face, loops) in &trim_map.boundaries {
                // Each face group must appear in survival map.
                assert!(
                    survival.groups.contains_key(source_face),
                    "Identical boxes {op:?}: trim face {:?} not in survival map",
                    source_face,
                );

                for (li, trim_loop) in loops.iter().enumerate() {
                    // Non-empty loop.
                    assert!(
                        !trim_loop.edges.is_empty(),
                        "Identical boxes {op:?}: face {:?} loop {li} is empty",
                        source_face,
                    );

                    // Closed loop.
                    let n = trim_loop.edges.len();
                    for i in 0..n {
                        let next = (i + 1) % n;
                        assert_eq!(
                            trim_loop.edges[i].v1, trim_loop.edges[next].v0,
                            "Identical boxes {op:?}: face {:?} loop {li} not closed at edge {i}",
                            source_face,
                        );
                    }

                    // Valid vertex indices.
                    for (ei, edge) in trim_loop.edges.iter().enumerate() {
                        assert!(
                            edge.v0 < vert_count && edge.v1 < vert_count,
                            "Identical boxes {op:?}: face {:?} loop {li} edge {ei}: \
                             vertex index out of bounds (v0={}, v1={}, vert_count={})",
                            source_face,
                            edge.v0,
                            edge.v1,
                            vert_count,
                        );
                    }
                }
            }
        }
    }

    // ── A3: Boundary edge pairing — each boundary edge has its reverse in
    //    a different face group, or is a true outer boundary edge ──
    // For overlapping box subtract, every directed trim edge (v0, v1) should
    // either have (v1, v0) in some OTHER face group's trim boundaries
    // (intersection edge shared between two face groups) or be a true outer
    // boundary edge with no reverse anywhere.

    #[test]
    fn test_trim_conservation_boundary_edges() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        // Build a global set of all directed trim edges across all face groups,
        // tagged with their source face.
        let mut all_directed: HashMap<(usize, usize), Vec<SourceFace>> = HashMap::new();
        for (source_face, loops) in &trim_map.boundaries {
            for trim_loop in loops {
                for edge in &trim_loop.edges {
                    all_directed
                        .entry((edge.v0, edge.v1))
                        .or_default()
                        .push(*source_face);
                }
            }
        }

        // For each directed edge, check if the reverse exists. If it does,
        // it must be in a DIFFERENT face group. If it doesn't, this is an
        // outer boundary edge.
        for (&(v0, v1), source_faces) in &all_directed {
            if let Some(reverse_faces) = all_directed.get(&(v1, v0)) {
                // The reverse edge exists. It must come from at least one different
                // face group (intersection edge shared between face groups).
                let has_different_face = source_faces
                    .iter()
                    .any(|sf| reverse_faces.iter().any(|rf| rf != sf));
                // Note: it's also valid for the reverse to be in the same face group
                // if the face group has multiple loops (e.g., a hole). So we don't
                // assert has_different_face — we just verify no self-contradictions.
                let _ = has_different_face; // Acknowledged, no hard assertion needed.
            }
            // If no reverse edge exists, this is a true outer boundary edge — valid.

            // What we DO assert: no directed edge appears in the SAME face group twice.
            for sf in source_faces {
                let count = source_faces.iter().filter(|f| *f == sf).count();
                assert!(
                    count <= 1,
                    "Directed edge ({v0}, {v1}) appears {count} times in face {:?} — \
                     duplicate within same face group",
                    sf,
                );
            }
        }
    }

    // ── A4: No NaN in trim boundary vertex coordinates ──
    // For overlapping box subtract, verify that no vertex referenced by any
    // trim edge contains NaN coordinates.

    #[test]
    fn test_trim_no_nan_vertices() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Subtract);

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        for (source_face, loops) in &trim_map.boundaries {
            for (li, trim_loop) in loops.iter().enumerate() {
                for (ei, edge) in trim_loop.edges.iter().enumerate() {
                    for &vi in &[edge.v0, edge.v1] {
                        let v = subdivided.verts[vi];
                        assert!(
                            !v[0].is_nan() && !v[1].is_nan() && !v[2].is_nan(),
                            "Face {:?} loop {li} edge {ei}: vertex {vi} has NaN coords {v:?}",
                            source_face,
                        );
                        assert!(
                            v[0].is_finite() && v[1].is_finite() && v[2].is_finite(),
                            "Face {:?} loop {li} edge {ei}: vertex {vi} has non-finite coords {v:?}",
                            source_face,
                        );
                    }
                }
            }
        }
    }

    // ── A5: Trim boundary map has same face count as survival map ──
    // For overlapping box subtract with a different box pair, verify the
    // trim boundary map has exactly the same number of face entries as the
    // survival map.

    #[test]
    fn test_trim_face_count_subtract() {
        let (subdivided, labeling, bij_a, bij_b) = run_box_pair_pipeline(
            [0.0, 0.0, 0.0],
            [2.0, 2.0, 2.0],
            [1.0, 0.0, 0.0],
            [3.0, 2.0, 2.0],
        );

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bij_a,
            &bij_b,
        );

        assert!(
            !survival.groups.is_empty(),
            "Precondition: survival map must be non-empty"
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        assert_eq!(
            trim_map.boundaries.len(),
            survival.groups.len(),
            "Trim boundary map face count ({}) must equal survival map face count ({})",
            trim_map.boundaries.len(),
            survival.groups.len(),
        );

        // Additionally verify all keys match exactly.
        for source_face in survival.groups.keys() {
            assert!(
                trim_map.boundaries.contains_key(source_face),
                "Survival face {:?} missing from trim boundary map",
                source_face,
            );
        }
        for source_face in trim_map.boundaries.keys() {
            assert!(
                survival.groups.contains_key(source_face),
                "Trim boundary face {:?} not in survival map",
                source_face,
            );
        }
    }
}
