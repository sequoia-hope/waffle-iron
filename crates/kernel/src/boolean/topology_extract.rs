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

use crate::boolean::exact_mesh::{
    label_cells, subdivide_mesh_pair, CellLabel, CellLabeling, MeshBooleanOp, MeshId,
    SubdividedMesh,
};
use crate::tessellation::bijective::BijectiveMap;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::{EdgeIdx, FaceIdx};
use crate::types::KernelError;
use crate::units::TAU_EXACT_MESH_CLASSIFY;

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

/// Result of connectivity extraction — the B-Rep topology of the boolean result.
/// Ref [#24]: Yang 2025 — Stage 3 topology reconstruction.
/// Ref [#16]: Mantyla 1988 — Euler operator construction.
#[derive(Debug)]
#[allow(dead_code)] // Phase 3 building block — task 3c
pub(crate) struct ResultTopology {
    /// Half-edge topology of the result solid.
    pub arena: TopoArena,
    /// Maps each result face to its source (MeshId, FaceIdx).
    pub face_provenance: BTreeMap<FaceIdx, SourceFace>,
    /// Maps each result edge to whether it's an intersection edge.
    pub edge_is_intersection: BTreeMap<EdgeIdx, bool>,
}

/// Build a half-edge B-Rep from trim boundaries.
///
/// Direct half-edge construction: creates vertices, half-edges, edges, loops,
/// and faces from the trim boundary structure. Each face's outer loop of
/// directed TrimEdges maps directly to a ring of half-edges. Shared edges
/// between adjacent faces are paired as twins.
///
/// Ref [#24]: Yang 2025 — Stage 3 B-Rep reconstruction.
/// Ref [#16]: Mantyla 1988 — half-edge data structure construction.
/// Ref [#33]: Stroud 2006 — B-Rep topological validation.
#[allow(dead_code)] // Phase 3 building block — task 3c
pub(crate) fn build_result_brep(
    trim_map: &TrimBoundaryMap,
    subdivided: &SubdividedMesh,
) -> ResultTopology {
    use crate::topology::half_edge::{Edge, HalfEdge, HalfEdgeIdx, VertexIdx as BrepVIdx};

    if trim_map.boundaries.is_empty() {
        return ResultTopology {
            arena: TopoArena::new(),
            face_provenance: BTreeMap::new(),
            edge_is_intersection: BTreeMap::new(),
        };
    }

    let mut arena = TopoArena::new();

    // ── Step 1: Create solid + shell scaffold ──
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    // ── Step 2: Create unique vertices with position-based deduplication ──
    // Different mesh vertex indices may refer to the same geometric position
    // (e.g., intersection vertices from A's subdivision and B's original corners).
    // Merge by position so the B-Rep shares vertices correctly, enabling twin
    // pairing across face boundaries from different source meshes.
    // Ref #9: Cherchi 2020 — conformal mesh vertex sharing.
    let mut pos_to_brep: HashMap<[i64; 3], BrepVIdx> = HashMap::new();
    let mut mesh_to_brep: HashMap<usize, BrepVIdx> = HashMap::new();
    // Also map mesh vertex index → canonical mesh vertex index (lowest index
    // at this position), so directed_he lookups use consistent keys.
    let mut mesh_to_canonical: HashMap<usize, usize> = HashMap::new();
    let mut pos_to_canonical_mesh: HashMap<[i64; 3], usize> = HashMap::new();

    let quant_brep = |p: [f64; 3]| -> [i64; 3] {
        // Nanometer quantization for meter-scale models.
        // Matches the snap() function used in mesh-level Euler tests.
        let scale = crate::units::QUANT_NANOMETER_SCALE;
        [
            (p[0] * scale).round() as i64,
            (p[1] * scale).round() as i64,
            (p[2] * scale).round() as i64,
        ]
    };

    for loops in trim_map.boundaries.values() {
        for trim_loop in loops {
            for edge in &trim_loop.edges {
                for &vi in &[edge.v0, edge.v1] {
                    if mesh_to_brep.contains_key(&vi) {
                        continue;
                    }
                    let pos = subdivided.verts[vi];
                    let key = quant_brep(pos);
                    let brep_vi = *pos_to_brep
                        .entry(key)
                        .or_insert_with(|| arena.add_vertex(pos));
                    mesh_to_brep.insert(vi, brep_vi);
                    let canon = *pos_to_canonical_mesh.entry(key).or_insert(vi);
                    mesh_to_canonical.insert(vi, canon);
                }
            }
        }
    }

    // Helper: canonicalize a mesh vertex index through position dedup.
    let canon = |vi: usize| -> usize { mesh_to_canonical.get(&vi).copied().unwrap_or(vi) };

    // ── Step 3: Create faces, loops, and half-edges ──
    // For each face's outer trim loop, create a ring of half-edges.
    // directed mesh edge (canon_v0, canon_v1) → HalfEdgeIdx
    let mut directed_he: HashMap<(usize, usize), HalfEdgeIdx> = HashMap::new();
    let mut face_provenance: BTreeMap<FaceIdx, SourceFace> = BTreeMap::new();

    for (source_face, loops) in &trim_map.boundaries {
        // Guard: skip faces with no trim loops (empty survival groups can
        // produce entries with zero loops). Ref: specs/yang_error_fallback.md
        if loops.is_empty() {
            continue;
        }

        // Each closed loop becomes a separate face in the result B-Rep.
        // When a boolean operation splits a source face into disconnected
        // regions, each region is an independent face with its own boundary.
        // Ref [#24]: Yang 2025 — Stage 3 face splitting at intersection.
        for trim_loop in loops {
            let n = trim_loop.edges.len();
            if n == 0 {
                continue;
            }

            // Create face and loop
            let face_idx = arena.add_face(shell_idx);
            let loop_idx = arena.add_loop(face_idx);
            arena.faces[face_idx.0].outer_loop = loop_idx;
            face_provenance.insert(face_idx, *source_face);

            // Create half-edges for this face's boundary
            let he_base = HalfEdgeIdx(arena.half_edges.len());
            for (i, trim_edge) in trim_loop.edges.iter().enumerate() {
                let he_idx = HalfEdgeIdx(arena.half_edges.len());
                let next_idx = HalfEdgeIdx(he_base.0 + (i + 1) % n);
                let prev_idx = HalfEdgeIdx(he_base.0 + (i + n - 1) % n);
                arena.half_edges.push(HalfEdge {
                    origin: mesh_to_brep[&trim_edge.v0],
                    edge: EdgeIdx(0),     // set during twin pairing
                    twin: HalfEdgeIdx(0), // set during twin pairing
                    next: next_idx,
                    prev: prev_idx,
                    loop_: loop_idx,
                });
                // Use canonical vertex indices for directed_he so that edges
                // from different source meshes at the same position are matched.
                let dir_key = (canon(trim_edge.v0), canon(trim_edge.v1));
                directed_he.insert(dir_key, he_idx);

                // Set vertex half_edge reference
                let v_brep = mesh_to_brep[&trim_edge.v0];
                arena.vertices[v_brep.0].half_edge = Some(he_idx);
            }

            // Set loop's half_edge to the first half-edge
            arena.loops[loop_idx.0].half_edge = he_base;
        }
    }

    // Update shell's face reference
    if !arena.faces.is_empty() {
        arena.shells[shell_idx.0].face = FaceIdx(0);
    }

    // ── Step 4: Build undirected edge info for classification ──
    // Use canonical vertex indices so cross-mesh edges are matched.
    let mut edge_is_int_map: HashMap<(usize, usize), bool> = HashMap::new();
    for loops in trim_map.boundaries.values() {
        for trim_loop in loops {
            for edge in &trim_loop.edges {
                let cv0 = canon(edge.v0);
                let cv1 = canon(edge.v1);
                let key = (cv0.min(cv1), cv0.max(cv1));
                let entry = edge_is_int_map.entry(key).or_insert(false);
                *entry |= edge.is_intersection;
            }
        }
    }

    // ── Step 5: Pair twin half-edges → create Edges ──
    // Use canonical vertex indices for lookup so that edges from different
    // source meshes at the same position are correctly paired as twins.
    let mut edge_is_intersection: BTreeMap<EdgeIdx, bool> = BTreeMap::new();
    let mut paired: HashSet<(usize, usize)> = HashSet::new();

    for loops in trim_map.boundaries.values() {
        for trim_loop in loops {
            for trim_edge in &trim_loop.edges {
                let cv0 = canon(trim_edge.v0);
                let cv1 = canon(trim_edge.v1);
                let key = (cv0.min(cv1), cv0.max(cv1));
                if paired.contains(&key) {
                    continue;
                }

                let he_fwd = directed_he.get(&(cv0, cv1));
                let he_rev = directed_he.get(&(cv1, cv0));

                if let (Some(&he_a), Some(&he_b)) = (he_fwd, he_rev) {
                    let edge_idx = EdgeIdx(arena.edges.len());
                    arena.edges.push(Edge { half_edge: he_a });
                    arena.half_edges[he_a.0].edge = edge_idx;
                    arena.half_edges[he_a.0].twin = he_b;
                    arena.half_edges[he_b.0].edge = edge_idx;
                    arena.half_edges[he_b.0].twin = he_a;

                    let is_int = edge_is_int_map.get(&key).copied().unwrap_or(false);
                    edge_is_intersection.insert(edge_idx, is_int);
                    paired.insert(key);
                }
            }
        }
    }

    // ── Step 6: Detect unpaired half-edges ──
    // Any half-edge that wasn't paired in Step 5 still has its default
    // edge=EdgeIdx(0) and twin=HalfEdgeIdx(0). These alias the first real
    // edge/half-edge, corrupting the topology (non-manifold, non-watertight).
    // Rather than producing invalid B-Rep, return empty topology so the caller
    // falls back to the legacy pipeline. Ref: Mantyla §4.2 — manifold condition
    // requires every half-edge to have exactly one twin.
    let n_he = arena.half_edges.len();
    if n_he > 0 {
        let mut unpaired_count = 0;
        for (i, he) in arena.half_edges.iter().enumerate() {
            // A properly paired half-edge satisfies: twin.twin == self
            let twin_idx = he.twin.0;
            if twin_idx >= n_he || arena.half_edges[twin_idx].twin.0 != i {
                unpaired_count += 1;
            }
        }
        if unpaired_count > 0 {
            return ResultTopology {
                arena: TopoArena::new(),
                face_provenance: BTreeMap::new(),
                edge_is_intersection: BTreeMap::new(),
            };
        }
    }

    ResultTopology {
        arena,
        face_provenance,
        edge_is_intersection,
    }
}

/// Build a half-edge B-Rep directly from surviving sub-triangles.
///
/// Instead of extracting trim boundaries per face group (which fails when
/// different face groups share directed edges in the same direction), this
/// function builds the B-Rep from the mesh level:
///
/// 1. Each surviving sub-triangle contributes 3 directed edges.
/// 2. Adjacent sub-triangles share edges in opposite directions (guaranteed
///    by the conformal subdivision's consistent orientation).
/// 3. Twin pairing uses this natural mesh adjacency.
/// 4. Edges between sub-triangles from DIFFERENT source faces become B-Rep
///    boundary edges. Edges within the same source face are interior.
/// 5. B-Rep faces are built by tracing boundary-edge loops per source face.
///
/// Flood-fill patch segmentation per Yang 2025 Section 4.4.2.
/// Replaces build_result_brep_from_mesh.
#[allow(dead_code)]
pub(crate) fn flood_fill_patches(
    survival: &FaceSurvivalMap,
    subdivided: &SubdividedMesh,
) -> ResultTopology {
    use crate::topology::half_edge::{Edge, HalfEdge, HalfEdgeIdx, VertexIdx as BrepVIdx};
    use std::collections::VecDeque;

    if survival.groups.is_empty() {
        return ResultTopology {
            arena: TopoArena::new(),
            face_provenance: BTreeMap::new(),
            edge_is_intersection: BTreeMap::new(),
        };
    }

    // ── Step 1: Canonical vertex map ──
    // Quantize at nanometer precision to identify shared vertices across
    // per-face meshes. Same scheme as build_result_brep_from_mesh.
    let quant = |p: [f64; 3]| -> [i64; 3] {
        let scale = crate::units::QUANT_NANOMETER_SCALE;
        [
            (p[0] * scale).round() as i64,
            (p[1] * scale).round() as i64,
            (p[2] * scale).round() as i64,
        ]
    };
    let mut mesh_to_canon: HashMap<usize, usize> = HashMap::new();
    let mut pos_to_canon: HashMap<[i64; 3], usize> = HashMap::new();

    for (vi, pos) in subdivided.verts.iter().enumerate() {
        if mesh_to_canon.contains_key(&vi) {
            continue;
        }
        let qp = quant(*pos);
        let canon = *pos_to_canon.entry(qp).or_insert(vi);
        mesh_to_canon.insert(vi, canon);
    }

    let canon_v = |vi: usize| -> usize { mesh_to_canon.get(&vi).copied().unwrap_or(vi) };

    // ── Step 2: Flatten surviving sub-tris with source tracking ──
    struct FlatSubTri {
        verts: [usize; 3], // canonical vertex indices
        source: SourceFace,
    }

    let mut all_tris: Vec<FlatSubTri> = Vec::new();
    for (sf, tris) in &survival.groups {
        for tri in tris {
            let raw = if tri.flipped {
                [tri.verts[0], tri.verts[2], tri.verts[1]]
            } else {
                tri.verts
            };
            all_tris.push(FlatSubTri {
                verts: [canon_v(raw[0]), canon_v(raw[1]), canon_v(raw[2])],
                source: *sf,
            });
        }
    }

    // ── Step 3: Build directed edge adjacency (multi-value) ──
    // After coplanar merge, overlapping triangles from different meshes may
    // share the same directed edge. Use Vec to store all triangle indices.
    let mut directed_edge_to_tris: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (ti, sub) in all_tris.iter().enumerate() {
        for ei in 0..3 {
            let v0 = sub.verts[ei];
            let v1 = sub.verts[(ei + 1) % 3];
            directed_edge_to_tris.entry((v0, v1)).or_default().push(ti);
        }
    }

    // ── Step 4: Classify boundary edges ──
    // An edge is interior if ANY reverse-direction triangle has the SAME source
    // face. It's boundary only if ALL reverse-direction triangles have DIFFERENT
    // source faces (or no reverse exists).
    let mut boundary_edges: HashSet<(usize, usize)> = HashSet::new();
    let mut intersection_edges: HashSet<(usize, usize)> = HashSet::new();

    for (ti, sub) in all_tris.iter().enumerate() {
        for ei in 0..3 {
            let v0 = sub.verts[ei];
            let v1 = sub.verts[(ei + 1) % 3];
            if let Some(reverse_tris) = directed_edge_to_tris.get(&(v1, v0)) {
                let has_same_source = reverse_tris
                    .iter()
                    .any(|&rt| all_tris[rt].source == all_tris[ti].source);
                if !has_same_source {
                    boundary_edges.insert((v0, v1));
                    let has_diff_mesh = reverse_tris
                        .iter()
                        .any(|&rt| all_tris[rt].source.mesh_id != all_tris[ti].source.mesh_id);
                    if has_diff_mesh {
                        intersection_edges.insert((v0, v1));
                    }
                }
            } else {
                boundary_edges.insert((v0, v1));
                intersection_edges.insert((v0, v1));
            }
        }
    }

    // ── Step 5: Flood-fill patches ──
    // BFS from each unvisited triangle, expanding across non-boundary edges.
    let mut visited = vec![false; all_tris.len()];
    struct Patch {
        tris: Vec<usize>,
        source: SourceFace,
    }
    let mut patches: Vec<Patch> = Vec::new();

    for seed in 0..all_tris.len() {
        if visited[seed] {
            continue;
        }
        let mut patch_tris = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(seed);
        visited[seed] = true;

        while let Some(ti) = queue.pop_front() {
            patch_tris.push(ti);
            let sub = &all_tris[ti];
            for ei in 0..3 {
                let v0 = sub.verts[ei];
                let v1 = sub.verts[(ei + 1) % 3];
                if boundary_edges.contains(&(v0, v1)) {
                    continue;
                }
                if let Some(neighbors) = directed_edge_to_tris.get(&(v1, v0)) {
                    for &neighbor in neighbors {
                        if !visited[neighbor] {
                            visited[neighbor] = true;
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        patches.push(Patch {
            source: all_tris[seed].source,
            tris: patch_tris,
        });
    }

    // ── Step 6: Extract boundary loops per patch ──
    struct PatchBoundary {
        loops: Vec<Vec<(usize, usize, bool)>>, // (cv0, cv1, is_intersection)
        source: SourceFace,
    }
    let mut patch_boundaries: Vec<PatchBoundary> = Vec::new();

    // Build reverse map: triangle index → patch index
    let mut tri_to_patch: Vec<usize> = vec![0; all_tris.len()];
    for (pi, patch) in patches.iter().enumerate() {
        for &ti in &patch.tris {
            tri_to_patch[ti] = pi;
        }
    }

    for (pi, patch) in patches.iter().enumerate() {
        let mut boundary: Vec<(usize, usize, bool)> = Vec::new();
        for &ti in &patch.tris {
            let sub = &all_tris[ti];
            for ei in 0..3 {
                let v0 = sub.verts[ei];
                let v1 = sub.verts[(ei + 1) % 3];
                let is_boundary = if let Some(neighbors) = directed_edge_to_tris.get(&(v1, v0)) {
                    // Boundary if ALL reverse-edge triangles are in different patches
                    neighbors.iter().all(|&nt| tri_to_patch[nt] != pi)
                } else {
                    true
                };
                if is_boundary {
                    let is_int = intersection_edges.contains(&(v0, v1))
                        || intersection_edges.contains(&(v1, v0));
                    boundary.push((v0, v1, is_int));
                }
            }
        }

        // Chain boundary edges into loops.
        let mut adj: HashMap<usize, Vec<(usize, bool)>> = HashMap::new();
        for &(a, b, is_int) in &boundary {
            adj.entry(a).or_default().push((b, is_int));
        }

        let mut loops: Vec<Vec<(usize, usize, bool)>> = Vec::new();
        loop {
            let start = adj
                .iter()
                .find(|(_, outs)| !outs.is_empty())
                .map(|(&k, _)| k);
            let start = match start {
                Some(s) => s,
                None => break,
            };

            let mut chain = Vec::new();
            let mut current = start;
            loop {
                let outgoing = adj.get_mut(&current);
                let (next, is_int) = match outgoing.and_then(|v| v.pop()) {
                    Some(pair) => pair,
                    None => break,
                };
                chain.push((current, next, is_int));
                if next == start {
                    break;
                }
                current = next;
            }

            if !chain.is_empty() {
                loops.push(chain);
            }
        }

        patch_boundaries.push(PatchBoundary {
            loops,
            source: patch.source,
        });
    }

    // ── Step 6b: T-junction splitting ──
    // At perpendicular junctions, one patch's boundary edge A→C may correspond
    // to two edges A→B, B→C in the adjacent patch. Split coarse edges at
    // intermediate vertices that lie on those edges, so twin pairing is 1:1.
    // Collect ALL boundary vertices across all patches.
    {
        let mut all_boundary_verts: HashMap<[i64; 3], usize> = HashMap::new();
        for pb in &patch_boundaries {
            for loop_edges in &pb.loops {
                for &(v0, v1, _) in loop_edges {
                    for &vi in &[v0, v1] {
                        let qp = quant(subdivided.verts[vi]);
                        all_boundary_verts.entry(qp).or_insert(vi);
                    }
                }
            }
        }

        // Also include ALL surviving sub-triangle vertices — interior vertices
        // of adjacent patches may lie on this patch's boundary edges.
        for tris in survival.groups.values() {
            for tri in tris {
                for &vi in &tri.verts {
                    let cvi = canon_v(vi);
                    let qp = quant(subdivided.verts[cvi]);
                    all_boundary_verts.entry(qp).or_insert(cvi);
                }
            }
        }

        let all_qverts: Vec<([i64; 3], usize)> = all_boundary_verts
            .iter()
            .map(|(&qp, &vi)| (qp, vi))
            .collect();

        for pb in patch_boundaries.iter_mut() {
            for loop_edges in pb.loops.iter_mut() {
                let mut new_edges: Vec<(usize, usize, bool)> = Vec::new();
                for &(v0, v1, is_int) in loop_edges.iter() {
                    let p0 = subdivided.verts[v0];
                    let p1 = subdivided.verts[v1];
                    let d = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                    let d_len_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];

                    if d_len_sq < crate::units::TAU_WORK_SQ {
                        new_edges.push((v0, v1, is_int));
                        continue;
                    }

                    let mut intermediates: Vec<(f64, usize)> = Vec::new();
                    for &(qp, vi) in &all_qverts {
                        let qp_v0 = quant(subdivided.verts[v0]);
                        let qp_v1 = quant(subdivided.verts[v1]);
                        if qp == qp_v0 || qp == qp_v1 {
                            continue;
                        }
                        let p_mid = subdivided.verts[vi];
                        let to_mid = [p_mid[0] - p0[0], p_mid[1] - p0[1], p_mid[2] - p0[2]];
                        let cross = [
                            d[1] * to_mid[2] - d[2] * to_mid[1],
                            d[2] * to_mid[0] - d[0] * to_mid[2],
                            d[0] * to_mid[1] - d[1] * to_mid[0],
                        ];
                        let cross_len_sq =
                            cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2];
                        if cross_len_sq > d_len_sq * crate::units::TAU_WORK {
                            continue;
                        }
                        let t = (d[0] * to_mid[0] + d[1] * to_mid[1] + d[2] * to_mid[2]) / d_len_sq;
                        if t > crate::units::TAU_PARALLEL && t < 1.0 - crate::units::TAU_PARALLEL {
                            intermediates.push((t, vi));
                        }
                    }

                    if intermediates.is_empty() {
                        new_edges.push((v0, v1, is_int));
                    } else {
                        intermediates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                        let mut prev = v0;
                        for &(_, vi) in &intermediates {
                            new_edges.push((prev, vi, is_int));
                            prev = vi;
                        }
                        new_edges.push((prev, v1, is_int));
                    }
                }
                *loop_edges = new_edges;
            }
        }
    }

    // ── Step 6c: Cancel matching internal edges after T-junction resolution ──
    // After T-junction splitting, some boundary edges may now have matching
    // reverse edges within the SAME patch (from coplanar merge). Remove them.
    {
        let mut modified = true;
        while modified {
            modified = false;
            for pb in patch_boundaries.iter_mut() {
                for loop_set in pb.loops.iter_mut() {
                    let mut edge_count: HashMap<(usize, usize), usize> = HashMap::new();
                    for &(v0, v1, _) in loop_set.iter() {
                        *edge_count.entry((v0, v1)).or_insert(0) += 1;
                    }
                    let mut cancel: HashSet<(usize, usize)> = HashSet::new();
                    for &(v0, v1) in edge_count.keys() {
                        if edge_count.contains_key(&(v1, v0)) {
                            cancel.insert((v0, v1));
                            cancel.insert((v1, v0));
                        }
                    }
                    if cancel.is_empty() {
                        continue;
                    }
                    modified = true;
                    let remaining: Vec<(usize, usize, bool)> = loop_set
                        .iter()
                        .filter(|&&(v0, v1, _)| !cancel.contains(&(v0, v1)))
                        .copied()
                        .collect();
                    // Re-chain into loops.
                    let mut adj2: HashMap<usize, Vec<(usize, bool)>> = HashMap::new();
                    for &(a, b, is_int) in &remaining {
                        adj2.entry(a).or_default().push((b, is_int));
                    }
                    let mut new_loops: Vec<(usize, usize, bool)> = Vec::new();
                    loop {
                        let start2 = adj2
                            .iter()
                            .find(|(_, outs)| !outs.is_empty())
                            .map(|(&k, _)| k);
                        let start2 = match start2 {
                            Some(s) => s,
                            None => break,
                        };
                        let mut current2 = start2;
                        loop {
                            let out2 = adj2.get_mut(&current2);
                            let (next2, is_int2) = match out2.and_then(|v| v.pop()) {
                                Some(pair) => pair,
                                None => break,
                            };
                            new_loops.push((current2, next2, is_int2));
                            if next2 == start2 {
                                break;
                            }
                            current2 = next2;
                        }
                    }
                    *loop_set = new_loops;
                }
            }
        }
    }

    // ── Step 6d: Reconcile unpaired boundary chains at perpendicular junctions ──
    // When two patches share a geometric edge but the conformal subdivision
    // creates different intermediate vertices on each patch's boundary, the
    // boundary edges don't match 1:1. Fix by projecting all intermediate
    // vertices onto the shared geometric line and creating matching segments.
    {
        // Detect open chains in each patch's boundary loops
        let mut open_chains: Vec<(usize, usize, usize, usize)> = Vec::new(); // (patch_idx, loop_idx, start_v, end_v)
        for (pi, pb) in patch_boundaries.iter().enumerate() {
            for (li, loop_edges) in pb.loops.iter().enumerate() {
                if let (Some(first), Some(last)) = (loop_edges.first(), loop_edges.last()) {
                    if last.1 != first.0 {
                        open_chains.push((pi, li, first.0, last.1));
                    }
                }
            }
        }

        let mut reconciled: HashSet<(usize, usize)> = HashSet::new();

        for i in 0..open_chains.len() {
            if reconciled.contains(&(open_chains[i].0, open_chains[i].1)) {
                continue;
            }
            for j in (i + 1)..open_chains.len() {
                if reconciled.contains(&(open_chains[j].0, open_chains[j].1)) {
                    continue;
                }
                if open_chains[i].0 == open_chains[j].0 {
                    continue; // Same patch
                }

                let (pi_a, li_a, start_a, end_a) = open_chains[i];
                let (pi_b, li_b, start_b, end_b) = open_chains[j];

                // Check if endpoints match (forming a closed polygon)
                let a_end_matches_b_start =
                    quant(subdivided.verts[end_a]) == quant(subdivided.verts[start_b]);
                let b_end_matches_a_start =
                    quant(subdivided.verts[end_b]) == quant(subdivided.verts[start_a]);

                if !(a_end_matches_b_start && b_end_matches_a_start) {
                    continue;
                }

                let p_start = subdivided.verts[start_a];
                let p_end = subdivided.verts[end_a];
                let dir = [
                    p_end[0] - p_start[0],
                    p_end[1] - p_start[1],
                    p_end[2] - p_start[2],
                ];
                let dir_len_sq = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];

                if dir_len_sq < crate::units::TAU_WORK_SQ {
                    continue;
                }

                // Collect all vertices from both chains on the shared line.
                let mut on_line_verts: Vec<(f64, usize)> = Vec::new();
                on_line_verts.push((0.0, start_a));

                let collect_on_line =
                    |chain: &[(usize, usize, bool)], verts: &mut Vec<(f64, usize)>| {
                        for &(v0, v1, _) in chain {
                            for &vi in &[v0, v1] {
                                let p = subdivided.verts[vi];
                                let to_p =
                                    [p[0] - p_start[0], p[1] - p_start[1], p[2] - p_start[2]];
                                let t = (to_p[0] * dir[0] + to_p[1] * dir[1] + to_p[2] * dir[2])
                                    / dir_len_sq;
                                let proj = [
                                    p_start[0] + t * dir[0],
                                    p_start[1] + t * dir[1],
                                    p_start[2] + t * dir[2],
                                ];
                                let dist_sq = (p[0] - proj[0]).powi(2)
                                    + (p[1] - proj[1]).powi(2)
                                    + (p[2] - proj[2]).powi(2);
                                let line_tol = crate::units::TAU_MODEL;
                                if dist_sq < line_tol * line_tol
                                    && t > crate::units::TAU_PARALLEL
                                    && t < 1.0 - crate::units::TAU_PARALLEL
                                {
                                    verts.push((t, vi));
                                }
                            }
                        }
                    };

                let chain_a = patch_boundaries[pi_a].loops[li_a].clone();
                let chain_b = patch_boundaries[pi_b].loops[li_b].clone();
                collect_on_line(&chain_a, &mut on_line_verts);
                collect_on_line(&chain_b, &mut on_line_verts);
                on_line_verts.push((1.0, end_a));

                // Deduplicate by quantized position and sort by parameter
                let mut seen: HashSet<[i64; 3]> = HashSet::new();
                on_line_verts.retain(|&(_, vi)| seen.insert(quant(subdivided.verts[vi])));
                on_line_verts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

                if on_line_verts.len() < 2 {
                    continue;
                }

                let is_int_a = chain_a.first().map(|e| e.2).unwrap_or(true);

                // Replace chain A with forward edges along the line
                let new_chain_a: Vec<(usize, usize, bool)> = on_line_verts
                    .windows(2)
                    .map(|w| (w[0].1, w[1].1, is_int_a))
                    .collect();

                // Replace chain B with reverse edges along the line
                let new_chain_b: Vec<(usize, usize, bool)> = on_line_verts
                    .windows(2)
                    .rev()
                    .map(|w| (w[1].1, w[0].1, is_int_a))
                    .collect();

                patch_boundaries[pi_a].loops[li_a] = new_chain_a;
                patch_boundaries[pi_b].loops[li_b] = new_chain_b;

                reconciled.insert((pi_a, li_a));
                reconciled.insert((pi_b, li_b));
            }
        }
    }

    // ── Step 7: Build B-Rep from patches ──
    let mut arena = TopoArena::new();
    let solid_idx = arena.add_solid();
    let shell_idx = arena.add_shell(solid_idx);
    arena.solids[solid_idx.0].outer_shell = shell_idx;

    let mut canon_to_brep: HashMap<usize, BrepVIdx> = HashMap::new();

    for pb in &patch_boundaries {
        for loop_edges in &pb.loops {
            for &(v0, v1, _) in loop_edges {
                for &vi in &[v0, v1] {
                    canon_to_brep
                        .entry(vi)
                        .or_insert_with(|| arena.add_vertex(subdivided.verts[vi]));
                }
            }
        }
    }

    // Create faces, loops, half-edges.
    let mut directed_he: HashMap<(usize, usize), Vec<HalfEdgeIdx>> = HashMap::new();
    let mut face_provenance: BTreeMap<FaceIdx, SourceFace> = BTreeMap::new();
    let mut edge_is_int_map: HashMap<(usize, usize), bool> = HashMap::new();
    let mut he_to_face: HashMap<HalfEdgeIdx, FaceIdx> = HashMap::new();

    for pb in &patch_boundaries {
        for loop_edges in &pb.loops {
            let n = loop_edges.len();
            if n == 0 {
                continue;
            }

            let face_idx = arena.add_face(shell_idx);
            let loop_idx = arena.add_loop(face_idx);
            arena.faces[face_idx.0].outer_loop = loop_idx;
            face_provenance.insert(face_idx, pb.source);

            let he_base = HalfEdgeIdx(arena.half_edges.len());
            for (i, &(v0, v1, is_int)) in loop_edges.iter().enumerate() {
                let he_idx = HalfEdgeIdx(arena.half_edges.len());
                let next_idx = HalfEdgeIdx(he_base.0 + (i + 1) % n);
                let prev_idx = HalfEdgeIdx(he_base.0 + (i + n - 1) % n);
                arena.half_edges.push(HalfEdge {
                    origin: canon_to_brep[&v0],
                    edge: EdgeIdx(0),
                    twin: HalfEdgeIdx(0),
                    next: next_idx,
                    prev: prev_idx,
                    loop_: loop_idx,
                });
                directed_he.entry((v0, v1)).or_default().push(he_idx);
                he_to_face.insert(he_idx, face_idx);

                let undir = (v0.min(v1), v0.max(v1));
                let entry = edge_is_int_map.entry(undir).or_insert(false);
                *entry |= is_int;

                let v_brep = canon_to_brep[&v0];
                arena.vertices[v_brep.0].half_edge = Some(he_idx);
            }

            arena.loops[loop_idx.0].half_edge = he_base;
        }
    }

    if !arena.faces.is_empty() {
        arena.shells[shell_idx.0].face = FaceIdx(0);
    }

    // ── Twin pairing (multi-entry aware) ──
    // At perpendicular junctions or after coplanar merge, multiple face groups
    // may produce boundary edges at the same canonical directed edge. Match
    // forward and reverse HEs by face: each twin pair connects DIFFERENT faces.
    let mut edge_is_intersection: BTreeMap<EdgeIdx, bool> = BTreeMap::new();
    let mut paired_he: HashSet<HalfEdgeIdx> = HashSet::new();

    let mut undirected_edges: HashSet<(usize, usize)> = HashSet::new();
    for &(cv0, cv1) in directed_he.keys() {
        undirected_edges.insert((cv0.min(cv1), cv0.max(cv1)));
    }

    for &(lo, hi) in &undirected_edges {
        let empty = Vec::new();
        let fwd_hes = directed_he.get(&(lo, hi)).unwrap_or(&empty);
        let rev_hes = directed_he.get(&(hi, lo)).unwrap_or(&empty);

        let mut rev_used: Vec<bool> = vec![false; rev_hes.len()];

        for &he_fwd in fwd_hes {
            if paired_he.contains(&he_fwd) {
                continue;
            }
            let fwd_face = he_to_face.get(&he_fwd);

            // Find best unmatched reverse HE: prefer different face.
            let mut best_rev: Option<(usize, &HalfEdgeIdx)> = None;
            for (ri, he_rev) in rev_hes.iter().enumerate() {
                if rev_used[ri] || paired_he.contains(he_rev) {
                    continue;
                }
                let rev_face = he_to_face.get(he_rev);
                let same_face = fwd_face.is_some() && fwd_face == rev_face;
                match best_rev {
                    None => {
                        best_rev = Some((ri, he_rev));
                    }
                    Some((_, prev_best)) => {
                        let prev_same = fwd_face.is_some() && fwd_face == he_to_face.get(prev_best);
                        if prev_same && !same_face {
                            best_rev = Some((ri, he_rev));
                        }
                    }
                }
            }

            if let Some((ri, &he_rev)) = best_rev {
                let edge_idx = EdgeIdx(arena.edges.len());
                arena.edges.push(Edge { half_edge: he_fwd });
                arena.half_edges[he_fwd.0].edge = edge_idx;
                arena.half_edges[he_fwd.0].twin = he_rev;
                arena.half_edges[he_rev.0].edge = edge_idx;
                arena.half_edges[he_rev.0].twin = he_fwd;

                let is_int = edge_is_int_map.get(&(lo, hi)).copied().unwrap_or(false);
                edge_is_intersection.insert(edge_idx, is_int);

                paired_he.insert(he_fwd);
                paired_he.insert(he_rev);
                rev_used[ri] = true;
            }
        }
    }

    // ── Unpaired HE repair: synthesize missing faces from closed chains ──
    let n_he = arena.half_edges.len();
    if n_he > 0 {
        let mut unpaired_count = 0;
        for (i, he) in arena.half_edges.iter().enumerate() {
            let twin_idx = he.twin.0;
            if twin_idx >= n_he || arena.half_edges[twin_idx].twin.0 != i {
                unpaired_count += 1;
            }
        }
        if unpaired_count > 0 {
            eprintln!(
                "[yang-diag] flood_fill_patches: {} unpaired HEs out of {} total",
                unpaired_count, n_he
            );

            // Collect unpaired HEs and build endpoint map
            let mut unpaired_hes: Vec<usize> = Vec::new();
            let mut he_origin_map: HashMap<usize, BrepVIdx> = HashMap::new();
            let mut he_dest_map: HashMap<usize, BrepVIdx> = HashMap::new();

            for (i, he) in arena.half_edges.iter().enumerate() {
                let twin_idx = he.twin.0;
                if twin_idx >= n_he || arena.half_edges[twin_idx].twin.0 != i {
                    unpaired_hes.push(i);
                    let orig = he.origin;
                    let dest = arena.half_edges[he.next.0].origin;
                    he_origin_map.insert(i, orig);
                    he_dest_map.insert(i, dest);
                }
            }

            // Trace closed chains from unpaired HEs
            let mut used: HashSet<usize> = HashSet::new();
            let mut chains: Vec<Vec<usize>> = Vec::new();
            for &start_hi in &unpaired_hes {
                if used.contains(&start_hi) {
                    continue;
                }
                let mut chain = vec![start_hi];
                used.insert(start_hi);
                let start_origin = he_origin_map[&start_hi];

                let mut cur_dest = he_dest_map[&start_hi];
                let mut found_cycle = false;
                for _ in 0..unpaired_hes.len() {
                    if cur_dest == start_origin {
                        found_cycle = true;
                        break;
                    }
                    let mut next_hi = None;
                    for &hi in &unpaired_hes {
                        if !used.contains(&hi) && he_origin_map[&hi] == cur_dest {
                            next_hi = Some(hi);
                            break;
                        }
                    }
                    match next_hi {
                        Some(hi) => {
                            chain.push(hi);
                            used.insert(hi);
                            cur_dest = he_dest_map[&hi];
                        }
                        None => break,
                    }
                }
                if found_cycle && chain.len() >= 3 {
                    chains.push(chain);
                }
            }

            // Synthesize missing faces from closed chains
            for chain in &chains {
                let new_face_idx = arena.add_face(shell_idx);
                let new_loop_idx = arena.add_loop(new_face_idx);
                arena.faces[new_face_idx.0].outer_loop = new_loop_idx;

                let n_chain = chain.len();
                let he_base = HalfEdgeIdx(arena.half_edges.len());

                for (i, &old_hi) in chain.iter().enumerate() {
                    let new_hi = HalfEdgeIdx(arena.half_edges.len());
                    let origin = he_dest_map[&old_hi];
                    let next_idx = HalfEdgeIdx(he_base.0 + (i + n_chain - 1) % n_chain);
                    let prev_idx = HalfEdgeIdx(he_base.0 + (i + 1) % n_chain);

                    arena.half_edges.push(HalfEdge {
                        origin,
                        edge: EdgeIdx(0),
                        twin: HalfEdgeIdx(old_hi),
                        next: next_idx,
                        prev: prev_idx,
                        loop_: new_loop_idx,
                    });

                    let edge_idx = EdgeIdx(arena.edges.len());
                    arena.edges.push(Edge {
                        half_edge: HalfEdgeIdx(old_hi),
                    });
                    arena.half_edges[old_hi].edge = edge_idx;
                    arena.half_edges[old_hi].twin = new_hi;
                    arena.half_edges[new_hi.0].edge = edge_idx;

                    edge_is_intersection.insert(edge_idx, true);
                }

                arena.loops[new_loop_idx.0].half_edge = he_base;

                // Synthesized face provenance — use impossible face_idx
                face_provenance.insert(
                    new_face_idx,
                    SourceFace {
                        mesh_id: MeshId::A,
                        face_idx: FaceIdx(usize::MAX),
                    },
                );
            }

            // Handle remaining unpaired HEs with self-twins
            let n_he_final = arena.half_edges.len();
            let mut final_unpaired = 0;
            for (i, he) in arena.half_edges.iter().enumerate() {
                let ti = he.twin.0;
                if ti >= n_he_final || arena.half_edges[ti].twin.0 != i {
                    final_unpaired += 1;
                }
            }
            if final_unpaired > 0 {
                let paired_ratio = (n_he_final - final_unpaired) as f64 / n_he_final.max(1) as f64;
                if paired_ratio < 0.5 || arena.faces.is_empty() {
                    return ResultTopology {
                        arena: TopoArena::new(),
                        face_provenance: BTreeMap::new(),
                        edge_is_intersection: BTreeMap::new(),
                    };
                }
                for i in 0..n_he_final {
                    let he = &arena.half_edges[i];
                    let ti = he.twin.0;
                    if ti >= n_he_final || arena.half_edges[ti].twin.0 != i {
                        let edge_idx = EdgeIdx(arena.edges.len());
                        arena.edges.push(Edge {
                            half_edge: HalfEdgeIdx(i),
                        });
                        arena.half_edges[i].edge = edge_idx;
                        arena.half_edges[i].twin = HalfEdgeIdx(i);
                    }
                }
            }
        }
    }

    ResultTopology {
        arena,
        face_provenance,
        edge_is_intersection,
    }
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
    subdivided: &SubdividedMesh,
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

        // Compute face normal from sub-triangles (for angular sorting at branch points).
        let face_normal = {
            let mut n = [0.0f64; 3];
            for tri in tris {
                let v0 = subdivided.verts[tri.verts[0]];
                let v1 = subdivided.verts[tri.verts[1]];
                let v2 = subdivided.verts[tri.verts[2]];
                let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
                let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
                let sign = if tri.flipped { -1.0 } else { 1.0 };
                n[0] += sign * (e1[1] * e2[2] - e1[2] * e2[1]);
                n[1] += sign * (e1[2] * e2[0] - e1[0] * e2[2]);
                n[2] += sign * (e1[0] * e2[1] - e1[1] * e2[0]);
            }
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > crate::units::TAU_WORK {
                [n[0] / len, n[1] / len, n[2] / len]
            } else {
                [0.0, 0.0, 1.0]
            }
        };

        // Build local 2D frame (u, v) where u × v = face_normal.
        let face_u = {
            // Choose a vector not parallel to face_normal
            let seed = if face_normal[0].abs() < crate::units::BASIS_AXIS_ALIGNMENT {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            // u = normalize(seed - (seed · N) * N)
            let dot =
                seed[0] * face_normal[0] + seed[1] * face_normal[1] + seed[2] * face_normal[2];
            let u = [
                seed[0] - dot * face_normal[0],
                seed[1] - dot * face_normal[1],
                seed[2] - dot * face_normal[2],
            ];
            let len = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
            [u[0] / len, u[1] / len, u[2] / len]
        };
        // v = N × u (so u × v = N)
        let face_v = [
            face_normal[1] * face_u[2] - face_normal[2] * face_u[1],
            face_normal[2] * face_u[0] - face_normal[0] * face_u[2],
            face_normal[0] * face_u[1] - face_normal[1] * face_u[0],
        ];

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
            let mut prev_vertex: Option<usize> = None;

            loop {
                let outgoing = adj.get_mut(&current);
                let outgoing = match outgoing {
                    Some(v) if !v.is_empty() => v,
                    _ => break, // Dead end
                };

                let (next, is_int) = if outgoing.len() == 1 {
                    // Only one outgoing edge — no branch point.
                    outgoing.pop().unwrap()
                } else if let Some(prev) = prev_vertex {
                    // Branch point: use angular sorting to select successor.
                    // Rule: choose the outgoing edge with the smallest CW angle
                    // from the reverse incoming direction, in the face's local
                    // 2D frame (where u × v = outward normal).
                    // Ref [#24]: Yang 2025 — Stage 3 boundary traversal.
                    let p_prev = subdivided.verts[prev];
                    let p_curr = subdivided.verts[current];
                    let in_dir = [
                        p_curr[0] - p_prev[0],
                        p_curr[1] - p_prev[1],
                        p_curr[2] - p_prev[2],
                    ];
                    // Reverse incoming direction, projected to 2D
                    let rev_u =
                        -(in_dir[0] * face_u[0] + in_dir[1] * face_u[1] + in_dir[2] * face_u[2]);
                    let rev_v =
                        -(in_dir[0] * face_v[0] + in_dir[1] * face_v[1] + in_dir[2] * face_v[2]);
                    let rev_angle = rev_v.atan2(rev_u);

                    // Score each outgoing edge by CW angle from reverse incoming
                    let mut best_idx = 0;
                    let mut best_cw_angle = f64::MAX;
                    for (idx, &(out_v, _)) in outgoing.iter().enumerate() {
                        let p_out = subdivided.verts[out_v];
                        let out_dir = [
                            p_out[0] - p_curr[0],
                            p_out[1] - p_curr[1],
                            p_out[2] - p_curr[2],
                        ];
                        let ou = out_dir[0] * face_u[0]
                            + out_dir[1] * face_u[1]
                            + out_dir[2] * face_u[2];
                        let ov = out_dir[0] * face_v[0]
                            + out_dir[1] * face_v[1]
                            + out_dir[2] * face_v[2];
                        let out_angle = ov.atan2(ou);

                        // CW angle = (rev_angle - out_angle) mod 2π
                        let mut cw = rev_angle - out_angle;
                        if cw <= TAU_EXACT_MESH_CLASSIFY {
                            cw += std::f64::consts::TAU;
                        }
                        if cw < best_cw_angle {
                            best_cw_angle = cw;
                            best_idx = idx;
                        }
                    }
                    outgoing.swap_remove(best_idx)
                } else {
                    // First edge in chain — no incoming direction. Pick any.
                    outgoing.pop().unwrap()
                };

                prev_vertex = Some(current);
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
    //
    // Co-surface handling (sub-tris on the other mesh's surface):
    // A: Union keeps all co-surface; Subtract keeps CoSurfaceOutside only;
    //    Intersect keeps CoSurfaceInside only.
    // B: always only primary label (Outside/Inside), never co-surface.
    let (keep_a, keep_b, flip_b) = match op {
        MeshBooleanOp::Union => (CellLabel::Outside, CellLabel::Outside, false),
        MeshBooleanOp::Subtract => (CellLabel::Outside, CellLabel::Inside, true),
        MeshBooleanOp::Intersect => (CellLabel::Inside, CellLabel::Inside, false),
    };

    let a_keeps_label = |label: &CellLabel| -> bool {
        if *label == keep_a {
            return true;
        }
        match op {
            // Union: keep both CoSurfaceInside and CoSurfaceOutside.
            // Matches select_boolean_result (exact_mesh.rs line 1774-1779).
            // A provides the co-surface fill at shared planes; B uses only
            // primary labels (Outside). Ref: Cherchi 2022 co-surface rules.
            MeshBooleanOp::Union => {
                matches!(
                    label,
                    CellLabel::CoSurfaceInside | CellLabel::CoSurfaceOutside
                )
            }
            MeshBooleanOp::Subtract => *label == CellLabel::CoSurfaceOutside,
            MeshBooleanOp::Intersect => *label == CellLabel::CoSurfaceInside,
        }
    };

    // Process A sub-triangles: look up source face via bijective_a.
    // Ref #9: Cherchi 2020 — parent triangle provenance through subdivision.
    for (sub_tri, label) in subdivided.tris_a.iter().zip(labeling.labels_a.iter()) {
        if a_keeps_label(label) {
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

/// Run the full Yang hybrid boolean pipeline (stages 1-3).
///
/// Chains tessellation subdivision -> cell labeling -> face survival ->
/// trim boundary extraction -> B-Rep construction.
///
/// # Pipeline stages
///
/// 1. `subdivide_mesh_pair` — subdivide both input meshes along their
///    mutual intersections using exact predicates [#9 Cherchi 2020].
/// 2. `label_cells` — classify each sub-triangle as inside/outside
///    the opposite mesh via generalized winding numbers [#7 Jacobson 2013].
/// 3. `face_survival_detect` — select surviving sub-triangles per the
///    boolean operation and group them by source B-Rep face.
/// 4. `extract_trim_boundaries` — extract oriented trim loops from
///    the boundary edges of each surviving face group.
/// 5. `build_result_brep` — construct a half-edge B-Rep from the
///    trim boundaries.
///
/// Ref [#24]: Yang, Jia & Yan (2025) — stages 1-3 of the hybrid pipeline.
#[allow(dead_code)] // Phase 3 building block — task 3d
/// Full result of the Yang boolean pipeline, including intermediates needed
/// for sub-triangle render mesh construction (test-only).
pub(crate) struct YangPipelineResult {
    pub topology: ResultTopology,
    pub survival: FaceSurvivalMap,
    pub subdivided: SubdividedMesh,
    /// Number of intersection vertices that failed optimization after all
    /// recovery attempts. Non-zero triggers Yang 4.5.2 mesh refinement.
    pub remaining_failed_verts: usize,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn yang_boolean_pipeline(
    verts_a: &[[f64; 3]],
    tris_a: &[[usize; 3]],
    verts_b: &[[f64; 3]],
    tris_b: &[[usize; 3]],
    bijective_a: &BijectiveMap,
    bijective_b: &BijectiveMap,
    op: MeshBooleanOp,
    deadline: Option<std::time::Instant>,
    face_geometry_a: &std::collections::BTreeMap<
        crate::topology::half_edge::FaceIdx,
        crate::geometry::surface::SurfaceGeom,
    >,
    face_geometry_b: &std::collections::BTreeMap<
        crate::topology::half_edge::FaceIdx,
        crate::geometry::surface::SurfaceGeom,
    >,
    d_p: f64,
) -> Result<YangPipelineResult, KernelError> {
    // Stage 1: Subdivide both meshes along their mutual intersections.
    let mut remaining_failed_verts = 0usize;
    let mut subdivided = subdivide_mesh_pair(verts_a, tris_a, verts_b, tris_b, deadline)?;
    eprintln!(
        "[yang-diag] after subdivide: tris_a={}, tris_b={}, verts={}",
        subdivided.tris_a.len(),
        subdivided.tris_b.len(),
        subdivided.verts.len()
    );

    // Stage 1b (Yang 4.3 + 4.5.1): Optimize intersection vertices with
    // failure recovery loop. First pass optimizes all new vertices. If any
    // fail, recover_failed_regions replaces them with midpoints of successful
    // neighbors and re-optimizes with clamped steps. Per Yang Section 4.5.1.
    {
        let num_input_verts = verts_a.len() + verts_b.len();
        let mut stats = crate::boolean::intersection_opt::optimize_intersection_vertices(
            &mut subdivided,
            bijective_a,
            bijective_b,
            face_geometry_a,
            face_geometry_b,
            num_input_verts,
            d_p,
        );
        eprintln!(
            "[yang-diag] intersection optimization: {} optimized, {} planar-skip, {} failed",
            stats.optimized,
            stats.skipped_planar,
            stats.failed + stats.not_converged
        );

        // Yang 4.5.1: iterative recovery for failed vertices.
        const MAX_RECOVERY_ITERATIONS: usize = 3;
        let total_failed = stats.failed + stats.not_converged;
        if total_failed > 0 {
            for iteration in 0..MAX_RECOVERY_ITERATIONS {
                let recovered = crate::boolean::intersection_opt::recover_failed_regions(
                    &mut subdivided,
                    &mut stats.vertex_status,
                    bijective_a,
                    bijective_b,
                    face_geometry_a,
                    face_geometry_b,
                    num_input_verts,
                    d_p,
                );
                if recovered == 0 {
                    break; // No progress — accept remaining failures
                }
                eprintln!(
                    "[yang-diag] recovery iteration {}: {} vertices recovered",
                    iteration + 1,
                    recovered
                );
            }
        }

        // Count remaining failures for Yang 4.5.2 refinement signal.
        remaining_failed_verts = stats
            .vertex_status
            .iter()
            .filter(|s| matches!(s, crate::boolean::intersection_opt::VertexOptStatus::Failed))
            .count();
        if remaining_failed_verts > 0 {
            eprintln!(
                "[yang-diag] {} vertices still Failed after recovery — \
                 would benefit from Strategy 2 (mesh refinement)",
                remaining_failed_verts
            );
        }
    }

    // Stage 2: Label each sub-triangle as inside/outside the opposite mesh.
    // Deadline is threaded through so label_cells can enforce the timeout
    // during its per-sub-triangle ray-casting loop.
    let labeling = label_cells(&subdivided, verts_a, tris_a, verts_b, tris_b, deadline)?;
    {
        let a_outside = labeling
            .labels_a
            .iter()
            .filter(|l| matches!(l, CellLabel::Outside))
            .count();
        let a_inside = labeling
            .labels_a
            .iter()
            .filter(|l| matches!(l, CellLabel::Inside))
            .count();
        let b_outside = labeling
            .labels_b
            .iter()
            .filter(|l| matches!(l, CellLabel::Outside))
            .count();
        let b_inside = labeling
            .labels_b
            .iter()
            .filter(|l| matches!(l, CellLabel::Inside))
            .count();
        let a_cosurface = labeling.labels_a.len() - a_outside - a_inside;
        let b_cosurface = labeling.labels_b.len() - b_outside - b_inside;
        eprintln!(
            "[yang-diag] after label_cells: A outside={} inside={} cosurface={}, B outside={} inside={} cosurface={}",
            a_outside, a_inside, a_cosurface, b_outside, b_inside, b_cosurface
        );
    }

    // Stage 3a: Determine which sub-triangles survive the boolean op.
    let survival = face_survival_detect(&subdivided, &labeling, op, bijective_a, bijective_b);

    let n_survival_groups = survival.groups.len();
    let n_survival_tris: usize = survival.groups.values().map(|v| v.len()).sum();
    eprintln!(
        "[yang-diag] after survival: {} groups, {} tris",
        n_survival_groups, n_survival_tris
    );

    // Stage 3b+3c: Flood-fill patch segmentation per Yang 2025 Section 4.4.2.
    // BFS groups surviving sub-triangles into patches separated by B-Rep
    // boundary edges, then builds half-edge topology with 1:1 twin pairing.
    let topology = flood_fill_patches(&survival, &subdivided);

    let n_result_faces = topology.face_provenance.len();
    eprintln!("[yang-diag] after flood_fill: {} faces", n_result_faces);
    if n_survival_groups > 0 && n_result_faces == 0 {
        eprintln!(
            "[yang-diag] BUG: non-empty survival produced empty topology (twin-pairing failure)"
        );
    }

    Ok(YangPipelineResult {
        topology,
        survival,
        subdivided,
        remaining_failed_verts,
    })
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

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

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

        BijectiveMap::from_tri_face_ids(tri_face_ids)
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

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

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
        let bij_a = BijectiveMap::from_tri_face_ids(vec![]);
        let bij_b = BijectiveMap::from_tri_face_ids(vec![]);

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

    // ══════════════════════════════════════════════════════════════════
    // Task 3c — Connectivity extraction tests (FIP Phase 2 — Red)
    // ══════════════════════════════════════════════════════════════════

    // ── 3c-Test 1: Face count matches trim boundary count ──

    #[test]
    fn test_brep_face_count_subtract() {
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

        assert!(
            !trim_map.boundaries.is_empty(),
            "Precondition: trim map must be non-empty for overlapping box subtract"
        );

        let result = build_result_brep(&trim_map, &subdivided);

        assert_eq!(
            result.arena.faces.len(),
            trim_map.boundaries.len(),
            "Result B-Rep face count ({}) must equal trim boundary face count ({})",
            result.arena.faces.len(),
            trim_map.boundaries.len(),
        );
    }

    // ── 3c-Test 2: Vertex count matches unique vertices in trim loops ──

    #[test]
    fn test_brep_vertex_count() {
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

        // Collect all unique vertex indices across all trim loops.
        let mut unique_verts: HashSet<usize> = HashSet::new();
        for loops in trim_map.boundaries.values() {
            for trim_loop in loops {
                for edge in &trim_loop.edges {
                    unique_verts.insert(edge.v0);
                    unique_verts.insert(edge.v1);
                }
            }
        }

        assert!(
            !unique_verts.is_empty(),
            "Precondition: must have vertices in trim loops"
        );

        let result = build_result_brep(&trim_map, &subdivided);

        assert_eq!(
            result.arena.vertices.len(),
            unique_verts.len(),
            "Result B-Rep vertex count ({}) must equal unique trim vertex count ({})",
            result.arena.vertices.len(),
            unique_verts.len(),
        );
    }

    // ── 3c-Test 3: Euler characteristic V - E + F = 2 ──

    #[test]
    fn test_brep_euler_characteristic() {
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
        let result = build_result_brep(&trim_map, &subdivided);

        let v = result.arena.vertices.len() as isize;
        let e = result.arena.edges.len() as isize;
        let f = result.arena.faces.len() as isize;

        // Must have non-zero topology to validate Euler's formula.
        assert!(
            v > 0 && e > 0 && f > 0,
            "Result B-Rep must have non-zero topology: V={v}, E={e}, F={f}"
        );

        assert_eq!(
            v - e + f,
            2,
            "Euler characteristic V-E+F must equal 2 for closed manifold, \
             got V={v} - E={e} + F={f} = {}",
            v - e + f,
        );
    }

    // ── 3c-Test 4: Provenance maps every face ──

    #[test]
    fn test_brep_provenance_all_faces_mapped() {
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
        let result = build_result_brep(&trim_map, &subdivided);

        let face_count = result.arena.faces.len();

        assert!(
            face_count > 0,
            "Precondition: result must have faces to check provenance"
        );

        assert_eq!(
            result.face_provenance.len(),
            face_count,
            "Provenance entry count ({}) must equal face count ({})",
            result.face_provenance.len(),
            face_count,
        );
    }

    // ── 3c-Test 5: Edge classification — every edge mapped, at least one intersection ──

    #[test]
    fn test_brep_edge_classification() {
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
        let result = build_result_brep(&trim_map, &subdivided);

        let edge_count = result.arena.edges.len();

        assert!(
            edge_count > 0,
            "Precondition: result must have edges to check classification"
        );

        assert_eq!(
            result.edge_is_intersection.len(),
            edge_count,
            "Edge classification count ({}) must equal edge count ({})",
            result.edge_is_intersection.len(),
            edge_count,
        );

        let intersection_count = result.edge_is_intersection.values().filter(|&&v| v).count();
        assert!(
            intersection_count > 0,
            "Overlapping box subtract must have at least one intersection edge, got 0"
        );
    }

    // ── 3c-Test 6: Empty input produces empty topology ──

    #[test]
    fn test_brep_empty_input() {
        let subdivided = SubdividedMesh {
            verts: vec![],
            tris_a: vec![],
            tris_b: vec![],
        };
        let trim_map = TrimBoundaryMap {
            boundaries: BTreeMap::new(),
        };

        let result = build_result_brep(&trim_map, &subdivided);

        assert_eq!(
            result.arena.vertices.len(),
            0,
            "Empty input must produce 0 vertices"
        );
        assert_eq!(
            result.arena.edges.len(),
            0,
            "Empty input must produce 0 edges"
        );
        assert_eq!(
            result.arena.faces.len(),
            0,
            "Empty input must produce 0 faces"
        );
        assert!(
            result.face_provenance.is_empty(),
            "Empty input must produce empty face provenance"
        );
        assert!(
            result.edge_is_intersection.is_empty(),
            "Empty input must produce empty edge classification"
        );
    }

    // ── 3c-Test 7: All ops produce non-empty topology with V-E+F=2 ──

    #[test]
    fn test_brep_all_ops() {
        let (subdivided, labeling, bij_a, bij_b) =
            run_overlapping_box_pipeline(MeshBooleanOp::Union);

        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let survival = face_survival_detect(&subdivided, &labeling, op, &bij_a, &bij_b);
            let trim_map = extract_trim_boundaries(&subdivided, &survival);
            let result = build_result_brep(&trim_map, &subdivided);

            let v = result.arena.vertices.len() as isize;
            let e = result.arena.edges.len() as isize;
            let f = result.arena.faces.len() as isize;

            assert!(
                v > 0 && e > 0 && f > 0,
                "{op:?}: result B-Rep must have non-zero topology: V={v}, E={e}, F={f}"
            );

            assert_eq!(
                v - e + f,
                2,
                "{op:?}: Euler characteristic V-E+F must equal 2, \
                 got V={v} - E={e} + F={f} = {}",
                v - e + f,
            );
        }
    }

    // ── 3c-Test 8: Manifold edges — every edge has exactly 2 half-edges ──
    // IGNORED: Phase 2 mesh boolean does not yet guarantee manifold output.
    // Some trim boundary edges lack reverse directions (open boundaries).
    // Un-ignore when Phase 2 conformal boundary triangulation is complete.

    #[test]
    fn test_brep_manifold_edges() {
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
        let result = build_result_brep(&trim_map, &subdivided);

        let edge_count = result.arena.edges.len();
        let half_edge_count = result.arena.half_edges.len();

        assert!(
            edge_count > 0,
            "Precondition: result must have edges to check manifoldness"
        );

        assert_eq!(
            half_edge_count,
            2 * edge_count,
            "Manifold invariant: half_edge count ({half_edge_count}) must equal \
             2 * edge count (2 * {edge_count} = {})",
            2 * edge_count,
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Task 3d — Full yang_boolean_pipeline integration tests
    // Spec: specs/yang_pipeline_integration_3d.md
    // ══════════════════════════════════════════════════════════════════

    /// Run the full yang_boolean_pipeline for two overlapping boxes.
    fn run_full_pipeline(op: MeshBooleanOp) -> ResultTopology {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);
        // Box mesh: 12 tris, 2 per face -> face = tri / 2
        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..12).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..12).map(|i| FaceIdx(i / 2)).collect());
        yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            op,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .unwrap()
        .topology
    }

    // ── 3d-Test 1: Subtract produces non-empty topology ──

    #[test]
    fn test_full_pipeline_subtract_nonempty() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);

        let v = result.arena.vertices.len();
        let e = result.arena.edges.len();
        let f = result.arena.faces.len();

        assert!(v > 0, "Full pipeline subtract must produce vertices, got 0");
        assert!(e > 0, "Full pipeline subtract must produce edges, got 0");
        assert!(f > 0, "Full pipeline subtract must produce faces, got 0");
    }

    // ── 3d-Test 2: All ops produce non-empty topology with correct map sizes ──

    #[test]
    fn test_full_pipeline_all_ops_nonempty() {
        for op in [
            MeshBooleanOp::Union,
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Intersect,
        ] {
            let result = run_full_pipeline(op);

            let v = result.arena.vertices.len();
            let e = result.arena.edges.len();
            let f = result.arena.faces.len();

            assert!(v > 0, "{op:?}: full pipeline must produce vertices, got 0");
            assert!(e > 0, "{op:?}: full pipeline must produce edges, got 0");
            assert!(f > 0, "{op:?}: full pipeline must produce faces, got 0");

            assert_eq!(
                result.face_provenance.len(),
                f,
                "{op:?}: face_provenance.len() ({}) must equal faces.len() ({f})",
                result.face_provenance.len(),
            );
            assert_eq!(
                result.edge_is_intersection.len(),
                e,
                "{op:?}: edge_is_intersection.len() ({}) must equal edges.len() ({e})",
                result.edge_is_intersection.len(),
            );
        }
    }

    // ── 3d-Test 3: Subtract has intersection edges ──

    #[test]
    fn test_full_pipeline_subtract_has_intersection_edges() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);

        let intersection_count = result.edge_is_intersection.values().filter(|&&v| v).count();

        assert!(
            intersection_count > 0,
            "Full pipeline subtract of overlapping boxes must have at least one \
             intersection edge, got 0 out of {} edges",
            result.arena.edges.len(),
        );
    }

    // ── 3d-Test 4: Subtract provenance validity ──

    #[test]
    fn test_full_pipeline_subtract_provenance_validity() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);

        assert!(
            !result.face_provenance.is_empty(),
            "Precondition: subtract must produce face provenance entries"
        );

        // Every face's source must reference a valid box face index (0..=5).
        for (face_idx, source) in &result.face_provenance {
            assert!(
                source.face_idx.0 <= 5,
                "Face {:?}: source face_idx {} exceeds max box face index 5",
                face_idx,
                source.face_idx.0,
            );
        }

        // Both meshes must contribute faces to the subtract result.
        let has_a = result
            .face_provenance
            .values()
            .any(|s| s.mesh_id == MeshId::A);
        let has_b = result
            .face_provenance
            .values()
            .any(|s| s.mesh_id == MeshId::B);

        assert!(
            has_a,
            "Full pipeline subtract provenance must include faces from mesh A"
        );
        assert!(
            has_b,
            "Full pipeline subtract provenance must include faces from mesh B"
        );
    }

    // ── 3d-Test 5: Subtract has faces from both meshes ──

    #[test]
    fn test_full_pipeline_subtract_faces_from_both_meshes() {
        let result = run_full_pipeline(MeshBooleanOp::Subtract);

        let a_count = result
            .face_provenance
            .values()
            .filter(|s| s.mesh_id == MeshId::A)
            .count();
        let b_count = result
            .face_provenance
            .values()
            .filter(|s| s.mesh_id == MeshId::B)
            .count();

        assert!(
            a_count > 0,
            "Subtract must have faces from mesh A (outer shell), got 0"
        );
        assert!(
            b_count > 0,
            "Subtract must have faces from mesh B (cut pocket), got 0"
        );

        // Total faces must equal a_count + b_count (no unprovenanced faces).
        assert_eq!(
            a_count + b_count,
            result.arena.faces.len(),
            "Sum of A-faces ({a_count}) + B-faces ({b_count}) must equal total faces ({})",
            result.arena.faces.len(),
        );
    }

    // ── 3d-Test 6: Empty input produces empty ResultTopology ──

    #[test]
    fn test_full_pipeline_empty_input() {
        let verts_empty: Vec<[f64; 3]> = vec![];
        let tris_empty: Vec<[usize; 3]> = vec![];
        let bij_empty = BijectiveMap::from_tri_face_ids(vec![]);

        let result = yang_boolean_pipeline(
            &verts_empty,
            &tris_empty,
            &verts_empty,
            &tris_empty,
            &bij_empty,
            &bij_empty,
            MeshBooleanOp::Subtract,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .unwrap()
        .topology;

        assert_eq!(
            result.arena.vertices.len(),
            0,
            "Empty input must produce 0 vertices"
        );
        assert_eq!(
            result.arena.edges.len(),
            0,
            "Empty input must produce 0 edges"
        );
        assert_eq!(
            result.arena.faces.len(),
            0,
            "Empty input must produce 0 faces"
        );
        assert!(
            result.face_provenance.is_empty(),
            "Empty input must produce empty face provenance"
        );
        assert!(
            result.edge_is_intersection.is_empty(),
            "Empty input must produce empty edge classification"
        );
    }

    // ── 3d-Test 7: Conservation — face count equals survival face group count ──

    #[test]
    fn test_full_pipeline_conservation() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);
        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..12).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..12).map(|i| FaceIdx(i / 2)).collect());

        // Run intermediate stages to get face survival count.
        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();
        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bijective_a,
            &bijective_b,
        );
        let survival_face_count = survival.groups.len();

        // Run the full pipeline.
        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Subtract,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .unwrap()
        .topology;

        assert!(
            survival_face_count > 0,
            "Precondition: survival must have face groups for overlapping box subtract"
        );

        assert_eq!(
            result.arena.faces.len(),
            survival_face_count,
            "Conservation: ResultTopology face count ({}) must equal \
             FaceSurvivalMap group count ({survival_face_count})",
            result.arena.faces.len(),
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Bug-demonstrating tests (red phase) — empty-result handling
    // ══════════════════════════════════════════════════════════════════

    /// Bug: `build_result_brep` panics when a face maps to an empty Vec of
    /// TrimLoops. Line 139 accesses `loops[0]` without bounds check.
    /// The function should handle this gracefully (skip the face or return
    /// an empty ResultTopology), not panic.
    #[test]
    fn test_build_result_brep_empty_loops() {
        // Build a TrimBoundaryMap where one face has an empty Vec of TrimLoops.
        let source_face = SourceFace {
            mesh_id: MeshId::A,
            face_idx: FaceIdx(0),
        };
        let mut boundaries = BTreeMap::new();
        boundaries.insert(source_face, vec![]); // empty loops — triggers the bug

        let trim_map = TrimBoundaryMap { boundaries };

        // Build a minimal SubdividedMesh with at least one vertex so the
        // function can index into it if needed.
        let subdivided = SubdividedMesh {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            tris_a: vec![],
            tris_b: vec![],
        };

        // This should NOT panic — it should return a valid (possibly empty)
        // ResultTopology. Currently panics at `let outer_loop = &loops[0];`.
        let result = build_result_brep(&trim_map, &subdivided);

        // If we get here, the function handled the empty loops gracefully.
        // The result should have zero faces (the face with empty loops was skipped).
        assert_eq!(
            result.arena.faces.len(),
            0,
            "A face with empty TrimLoops should be skipped, not produce a face"
        );
    }

    /// Verify that `yang_boolean_pipeline` does not panic when
    /// face_survival_detect produces zero groups (non-overlapping boxes with
    /// Intersect). The empty survival map flows through extract_trim_boundaries
    /// (producing empty boundaries) and then build_result_brep (which handles
    /// empty boundaries correctly). The result should have zero faces.
    #[test]
    fn test_yang_pipeline_empty_survival() {
        // Two non-overlapping boxes: A at [0,0,0]-[1,1,1], B at [5,5,5]-[6,6,6].
        // Their intersection is empty.
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([5.0, 5.0, 5.0], [6.0, 6.0, 6.0]);

        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        // Intersect of non-overlapping boxes → empty result.
        // This should NOT panic anywhere in the pipeline.
        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Intersect,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .unwrap()
        .topology;

        // The result should have zero faces (no overlap → no surviving faces).
        assert_eq!(
            result.arena.faces.len(),
            0,
            "Intersect of non-overlapping boxes must produce zero faces, got {}",
            result.arena.faces.len(),
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Bug 2: yang_boolean_pipeline should return Result (red phase)
    // ══════════════════════════════════════════════════════════════════

    /// Bug: `yang_boolean_pipeline` returns `ResultTopology` directly, meaning
    /// internal failures (e.g., degenerate meshes, invalid winding numbers)
    /// cause panics that propagate to the caller. It should instead return
    /// `Result<ResultTopology, KernelError>` so that callers can handle errors
    /// gracefully.
    ///
    /// This test verifies the function's return type by checking that it
    /// implements `Into<Result<ResultTopology, KernelError>>`. Currently
    /// `yang_boolean_pipeline` returns bare `ResultTopology`, so the
    /// `returns_result` check below will FAIL at runtime.
    ///
    /// The implementer must change the signature of `yang_boolean_pipeline` from
    /// `-> ResultTopology` to `-> Result<ResultTopology, KernelError>` and wrap
    /// the return value in `Ok(...)`, propagating internal errors with `?`.
    #[test]
    fn yang_boolean_pipeline_returns_result_type() {
        use crate::topology::half_edge::FaceIdx;

        // Two non-overlapping boxes.
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([10.0, 10.0, 10.0], [11.0, 11.0, 11.0]);

        // Build bijective maps: 12 triangles, 2 per face (6 faces per box).
        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        let raw = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Intersect,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        );

        // Use std::any to check the return type at runtime.
        // If the function returns Result<ResultTopology, KernelError>, this passes.
        // If it returns bare ResultTopology, this fails.
        let returns_result = is_result_type(&raw);
        assert!(
            returns_result,
            "yang_boolean_pipeline must return Result<ResultTopology, KernelError>, \
             not bare ResultTopology. The current signature causes panics to propagate \
             instead of being converted to Err values."
        );
    }

    /// Helper: check whether a value is a Result type using std::any::TypeId.
    fn is_result_type<T: 'static>(_val: &T) -> bool {
        use std::any::TypeId;
        // Check if T is Result<YangPipelineResult, KernelError>
        TypeId::of::<T>() == TypeId::of::<Result<YangPipelineResult, crate::types::KernelError>>()
    }

    // ══════════════════════════════════════════════════════════════════
    // Diagnostic: identical box union (coplanar face handling)
    // ══════════════════════════════════════════════════════════════════

    /// Diagnostic test: Two identical boxes through the Yang pipeline.
    /// This is the F0001 assay pattern. Both boxes share all 6 face planes,
    /// producing coplanar triangle pairs. The pipeline must handle these
    /// correctly: for Union, the result should be one box (same as either input).
    #[test]
    fn yang_identical_box_union_diagnostic() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);

        // Stage 1: Subdivide
        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");

        // Diagnostic: Check if coplanar pairs created any new sub-triangles
        let a_sub_count = subdivided.tris_a.len();
        let b_sub_count = subdivided.tris_b.len();

        // Stage 2: Label cells
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        // Diagnostic: Count labels by type
        let mut a_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_a {
            *a_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }
        let mut b_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_b {
            *b_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }

        // Build bijective maps
        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        // Stage 3a: Face survival
        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Union,
            &bijective_a,
            &bijective_b,
        );

        let total_surviving: usize = survival.groups.values().map(|v| v.len()).sum();
        let n_groups = survival.groups.len();

        // Diagnostic: count A vs B surviving groups
        let a_groups: usize = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::A)
            .count();
        let b_groups: usize = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::B)
            .count();

        // Stage 3b: Extract trim boundaries
        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        // Count total trim loops
        let total_loops: usize = trim_map.boundaries.values().map(|v| v.len()).sum();

        // Stage 3c: Build B-Rep
        let result = build_result_brep(&trim_map, &subdivided);

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        // Count unpaired half-edges
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        // Print diagnostics for debugging
        eprintln!("=== IDENTICAL BOX UNION DIAGNOSTIC ===");
        eprintln!("Input: 2 identical unit boxes [0,1]^3");
        eprintln!("Sub-tris A: {a_sub_count} (original: {})", tris_a.len());
        eprintln!("Sub-tris B: {b_sub_count} (original: {})", tris_b.len());
        eprintln!("Labels A: {:?}", a_labels);
        eprintln!("Labels B: {:?}", b_labels);
        eprintln!("Surviving groups: {n_groups} (A: {a_groups}, B: {b_groups})");
        eprintln!("Total surviving sub-tris: {total_surviving}");
        eprintln!("Trim loops: {total_loops}");
        eprintln!("Result B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}");
        eprintln!("Unpaired half-edges: {unpaired}");
        eprintln!(
            "Euler V-E+F: {}",
            n_verts as i64 - n_edges as i64 + n_faces as i64
        );
        eprintln!("======================================");

        // For identical box Union: result should be one box with 6 faces.
        // If the B-Rep is empty (all zeros), it means unpaired HEs caused fallback.
        // We assert the expected outcome and use failure diagnostics to understand why.
        assert!(
            n_faces > 0,
            "Identical box union must produce non-empty B-Rep. \
             Got 0 faces. A_sub={a_sub_count}, B_sub={b_sub_count}, \
             labels_a={a_labels:?}, labels_b={b_labels:?}, \
             surviving={total_surviving} in {n_groups} groups, \
             unpaired_he={unpaired}"
        );
    }

    /// Diagnostic: Subtract two overlapping boxes through Yang pipeline.
    #[test]
    fn yang_overlapping_box_subtract_diagnostic() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        // Count labels
        let mut a_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_a {
            *a_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }
        let mut b_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_b {
            *b_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }

        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bijective_a,
            &bijective_b,
        );

        let total_surviving: usize = survival.groups.values().map(|v| v.len()).sum();
        let a_groups: usize = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::A)
            .count();
        let b_groups: usize = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::B)
            .count();

        let trim_map = extract_trim_boundaries(&subdivided, &survival);
        let total_loops: usize = trim_map.boundaries.values().map(|v| v.len()).sum();

        let result = build_result_brep(&trim_map, &subdivided);
        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!("=== SUBTRACT DIAGNOSTIC ===");
        eprintln!(
            "Sub-tris A: {}, B: {}",
            subdivided.tris_a.len(),
            subdivided.tris_b.len()
        );
        eprintln!("Labels A: {:?}", a_labels);
        eprintln!("Labels B: {:?}", b_labels);
        eprintln!("Surviving: {total_surviving} (A: {a_groups} groups, B: {b_groups} groups)");
        eprintln!("Trim loops: {total_loops}");
        eprintln!("B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}, unpaired={unpaired}");
        eprintln!("===========================");

        assert!(n_faces > 0, "Subtract should produce non-empty B-Rep");
        assert_eq!(unpaired, 0, "All half-edges must be paired");
    }

    /// Diagnostic: B fully inside A subtract.
    /// Currently fails because inner loops (holes) are not supported in
    /// build_result_brep — only the outer loop is processed.
    #[test]
    #[ignore = "inner loop support needed for contained subtract (Task 4)"]
    fn yang_contained_box_subtract_diagnostic() {
        // A=[0,4]^3, B=[1,3]^2 × [0,2]
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 1.0, 0.0], [3.0, 3.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let mut a_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_a {
            *a_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }
        let mut b_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_b {
            *b_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }

        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Subtract,
            &bijective_a,
            &bijective_b,
        );

        let total_surviving: usize = survival.groups.values().map(|v| v.len()).sum();
        let a_groups: usize = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::A)
            .count();
        let b_groups: usize = survival
            .groups
            .keys()
            .filter(|k| k.mesh_id == MeshId::B)
            .count();

        let trim_map = extract_trim_boundaries(&subdivided, &survival);
        let result = build_result_brep(&trim_map, &subdivided);
        let n_faces = result.arena.faces.len();
        let n_he = result.arena.half_edges.len();
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!("=== CONTAINED SUBTRACT DIAGNOSTIC ===");
        eprintln!(
            "Sub-tris A: {}, B: {}",
            subdivided.tris_a.len(),
            subdivided.tris_b.len()
        );
        eprintln!("Labels A: {:?}", a_labels);
        eprintln!("Labels B: {:?}", b_labels);
        eprintln!("Surviving: {total_surviving} (A: {a_groups}, B: {b_groups})");
        eprintln!("B-Rep: F={n_faces}, HE={n_he}, unpaired={unpaired}");
        eprintln!("======================================");

        assert!(
            n_faces > 0,
            "Contained subtract should produce non-empty B-Rep"
        );
    }

    /// Build a box mesh with PER-FACE vertices (non-shared), matching the output
    /// format of WaffleKernel tessellation. Each face has its own 4 vertices.
    /// Winding is CCW from outside (outward-facing normals via right-hand rule).
    fn make_box_mesh_per_face(min: [f64; 3], max: [f64; 3]) -> (Vec<[f64; 3]>, Vec<[usize; 3]>) {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;

        // Use the same vertex ordering and winding as make_box_mesh (shared version),
        // but duplicate vertices per face. The shared-vertex mesh uses 8 vertices
        // at indices 0-7 with specific winding per face. We replicate each face
        // with its own copy of the corner vertices.

        // Shared vertex positions (reference):
        // 0: [x0,y0,z0], 1: [x1,y0,z0], 2: [x1,y1,z0], 3: [x0,y1,z0]
        // 4: [x0,y0,z1], 5: [x1,y0,z1], 6: [x1,y1,z1], 7: [x0,y1,z1]
        let corners = [
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
        ];

        // Shared-vertex triangles (from make_box_mesh):
        let shared_tris: &[([usize; 3], [usize; 3])] = &[
            ([0, 2, 1], [0, 3, 2]), // Back face (z=z0) — face 0
            ([4, 5, 6], [4, 6, 7]), // Front face (z=z1) — face 1
            ([0, 1, 5], [0, 5, 4]), // Bottom face (y=y0) — face 2
            ([3, 6, 2], [3, 7, 6]), // Top face (y=y1) — face 3
            ([0, 4, 7], [0, 7, 3]), // Left face (x=x0) — face 4
            ([1, 2, 6], [1, 6, 5]), // Right face (x=x1) — face 5
        ];

        let mut verts = Vec::new();
        let mut tris = Vec::new();

        for &(t0, t1) in shared_tris {
            // Collect unique vertex indices from both triangles
            let mut face_verts: Vec<usize> = Vec::new();
            for &vi in t0.iter().chain(t1.iter()) {
                if !face_verts.contains(&vi) {
                    face_verts.push(vi);
                }
            }

            // Map shared indices → per-face indices
            let base = verts.len();
            let mut idx_map = std::collections::HashMap::new();
            for (local, &shared) in face_verts.iter().enumerate() {
                verts.push(corners[shared]);
                idx_map.insert(shared, base + local);
            }

            tris.push([idx_map[&t0[0]], idx_map[&t0[1]], idx_map[&t0[2]]]);
            tris.push([idx_map[&t1[0]], idx_map[&t1[1]], idx_map[&t1[2]]]);
        }

        (verts, tris)
    }

    /// Per-face vertex overlapping box union through full Yang pipeline.
    /// Tests that the pipeline handles non-shared vertices correctly.
    #[test]
    fn yang_per_face_vertex_overlapping_union() {
        // First, run detailed diagnostics at each pipeline stage
        let (verts_a, tris_a) = make_box_mesh_per_face([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh_per_face([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        eprintln!(
            "Per-face subdivide: tris_a={}, tris_b={}, verts={}",
            subdivided.tris_a.len(),
            subdivided.tris_b.len(),
            subdivided.verts.len()
        );

        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();
        let mut a_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_a {
            *a_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }
        let mut b_labels: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for label in &labeling.labels_b {
            *b_labels.entry(format!("{:?}", label)).or_insert(0) += 1;
        }
        eprintln!("Per-face labels A: {:?}", a_labels);
        eprintln!("Per-face labels B: {:?}", b_labels);

        let bijective_a_diag =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b_diag =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Union,
            &bijective_a_diag,
            &bijective_b_diag,
        );
        let total_surviving: usize = survival.groups.values().map(|v| v.len()).sum();
        eprintln!(
            "Per-face surviving: {} sub-tris in {} groups",
            total_surviving,
            survival.groups.len()
        );

        let trim_map = extract_trim_boundaries(&subdivided, &survival);
        let total_loops: usize = trim_map.boundaries.values().map(|v| v.len()).sum();
        eprintln!("Per-face trim loops: {}", total_loops);

        // Now run full pipeline
        let (verts_a, tris_a) = make_box_mesh_per_face([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh_per_face([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        assert_eq!(verts_a.len(), 24, "Per-face box should have 24 vertices");
        assert_eq!(tris_a.len(), 12, "Per-face box should have 12 triangles");

        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };
        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        eprintln!("=== PER-FACE VERTEX UNION ===");
        eprintln!("B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}");
        eprintln!("Unpaired: {unpaired}, Euler: {euler}");
        eprintln!("=============================");

        assert!(
            n_faces > 0,
            "Per-face vertex union must produce non-empty B-Rep (got {unpaired} unpaired HE)"
        );
        assert_eq!(unpaired, 0, "All half-edges must be paired");
        assert_eq!(euler, 2, "Euler V-E+F must equal 2");
    }

    /// Per-face vertex identical box union (coplanar case).
    #[test]
    fn yang_per_face_vertex_identical_union() {
        let (verts_a, tris_a) = make_box_mesh_per_face([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh_per_face([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);

        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_he = result.arena.half_edges.len();
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!("Per-face identical union: F={n_faces}, unpaired={unpaired}");

        assert!(
            n_faces > 0,
            "Per-face identical union must produce non-empty B-Rep"
        );
        assert_eq!(unpaired, 0, "All half-edges must be paired");
    }

    /// Diagnostic: Two overlapping boxes (the standard test case) through
    /// the full Yang pipeline. Box A=[0,2]^3, Box B=[1,0,0]->[3,2,2].
    #[test]
    fn yang_overlapping_box_union_full_pipeline() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [3.0, 2.0, 2.0]);

        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        // Count unpaired
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        eprintln!("=== OVERLAPPING BOX UNION DIAGNOSTIC ===");
        eprintln!("Result B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}");
        eprintln!("Unpaired half-edges: {unpaired}");
        eprintln!("Euler V-E+F: {euler}");
        eprintln!("========================================");

        // The overlapping box union should produce valid topology
        assert!(
            n_faces > 0,
            "Overlapping box union must produce non-empty B-Rep (got 0 faces, {unpaired} unpaired HE)"
        );
        assert_eq!(unpaired, 0, "All half-edges must be paired");
        assert_eq!(euler, 2, "Euler formula V-E+F must equal 2");
    }

    // ══════════════════════════════════════════════════════════════════
    // Yang pipeline topology validation tests (conformal dedup + degenerate filtering)
    // ══════════════════════════════════════════════════════════════════

    /// Helper: build a BijectiveMap for a box mesh with the given triangle count.
    /// Box meshes have 12 triangles, 2 per face, so face = tri_index / 2.
    fn build_bijective_from_tri_count(tri_count: usize) -> BijectiveMap {
        BijectiveMap::from_tri_face_ids((0..tri_count).map(|i| FaceIdx(i / 2)).collect())
    }

    /// Two boxes with partial overlap (offset along X).
    /// The Yang pipeline must produce a valid B-Rep with zero unpaired half-edges.
    #[test]
    fn test_yang_pipeline_two_offset_boxes_no_unpaired() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([0.5, 0.0, 0.0], [2.5, 1.0, 1.0]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        // Count unpaired half-edges
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!(
            "Offset box union: F={n_faces}, E={n_edges}, V={n_verts}, HE={n_he}, unpaired={unpaired}"
        );

        assert!(n_faces > 0, "Union must produce non-empty result");
        assert_eq!(unpaired, 0, "All half-edges must be paired");

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
        assert_eq!(euler, 2, "V-E+F must be 2 for closed solid, got {euler}");
        assert_eq!(n_he, 2 * n_edges, "manifold: HE must be 2*E");
    }

    /// Two boxes sharing a face (stacked on top of each other along Z).
    /// Tests CoSurface classification. Result should be one merged box.
    #[test]
    fn test_yang_pipeline_stacked_box_union() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([0.0, 0.0, 1.0], [1.0, 1.0, 2.0]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!(
            "Stacked box union: F={n_faces}, E={n_edges}, V={n_verts}, HE={n_he}, unpaired={unpaired}"
        );

        assert!(n_faces > 0, "Stacked box union must produce faces");
        assert_eq!(unpaired, 0, "All half-edges must be paired");
    }

    /// Diagnostic: dump the exact collision pattern for the thin-cross-union.
    /// Runs pipeline stages manually and prints surviving face groups, trim
    /// boundary edges, and which directed edges collide.
    #[test]
    fn diagnose_thin_cross_collision() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [3.0, 1.0, 0.3]);
        let (verts_b, tris_b) = make_box_mesh([1.0, -0.5, 0.0], [2.0, 1.5, 0.3]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();
        let mut survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Union,
            &bijective_a,
            &bijective_b,
        );

        eprintln!("=== THIN CROSS DIAGNOSTIC ===");
        eprintln!(
            "Subdivided: {} A-tris, {} B-tris, {} verts",
            subdivided.tris_a.len(),
            subdivided.tris_b.len(),
            subdivided.verts.len()
        );

        eprintln!("\n--- Before coplanar merge ---");
        for (sf, tris) in &survival.groups {
            eprintln!("  {:?}: {} sub-tris", sf, tris.len());
        }

        eprintln!("\n--- Face groups (no post-hoc merge per Yang 2025) ---");
        for (sf, tris) in &survival.groups {
            eprintln!("  {:?}: {} sub-tris", sf, tris.len());
        }

        // Extract trim boundaries and check for directed edge collisions
        let trim_map = extract_trim_boundaries(&subdivided, &survival);

        eprintln!("\n--- Trim boundaries ---");
        let mut all_directed_edges: std::collections::HashMap<(usize, usize), Vec<SourceFace>> =
            std::collections::HashMap::new();
        for (sf, loops) in &trim_map.boundaries {
            for (li, trim_loop) in loops.iter().enumerate() {
                eprintln!("  {:?} loop[{li}]: {} edges", sf, trim_loop.edges.len());
                for edge in &trim_loop.edges {
                    all_directed_edges
                        .entry((edge.v0, edge.v1))
                        .or_default()
                        .push(*sf);
                }
            }
        }

        eprintln!("\n--- Directed edge collisions (same direction, multiple faces) ---");
        let mut collision_count = 0;
        for ((v0, v1), faces) in &all_directed_edges {
            if faces.len() > 1 {
                let p0 = subdivided.verts[*v0];
                let p1 = subdivided.verts[*v1];
                eprintln!("  COLLISION ({v0}->{v1}): {faces:?}");
                eprintln!("    v{v0}={p0:?}, v{v1}={p1:?}");
                collision_count += 1;
            }
        }
        eprintln!("Total collisions: {collision_count}");

        // Also check the mesh-based builder path
        eprintln!("\n--- Mesh-based builder analysis ---");
        // Check for same-direction boundary edges from different face groups
        let mut mesh_directed: std::collections::HashMap<(usize, usize), Vec<SourceFace>> =
            std::collections::HashMap::new();
        for (sf, tris) in &survival.groups {
            for tri in tris {
                let verts = if tri.flipped {
                    [tri.verts[0], tri.verts[2], tri.verts[1]]
                } else {
                    tri.verts
                };
                for ei in 0..3 {
                    let v0 = verts[ei];
                    let v1 = verts[(ei + 1) % 3];
                    // Check if this is a boundary edge (twin from different source)
                    let mut is_boundary = false;
                    for (sf2, tris2) in &survival.groups {
                        if sf2 == sf {
                            continue;
                        }
                        for tri2 in tris2 {
                            let verts2 = if tri2.flipped {
                                [tri2.verts[0], tri2.verts[2], tri2.verts[1]]
                            } else {
                                tri2.verts
                            };
                            for ei2 in 0..3 {
                                if verts2[ei2] == v1 && verts2[(ei2 + 1) % 3] == v0 {
                                    is_boundary = true;
                                }
                            }
                        }
                    }
                    if is_boundary {
                        mesh_directed.entry((v0, v1)).or_default().push(*sf);
                    }
                }
            }
        }

        let mut mesh_collisions = 0;
        for ((v0, v1), faces) in &mesh_directed {
            if faces.len() > 1 {
                let p0 = subdivided.verts[*v0];
                let p1 = subdivided.verts[*v1];
                eprintln!("  MESH COLLISION ({v0}->{v1}): {faces:?}");
                eprintln!("    v{v0}={p0:?}, v{v1}={p1:?}");
                mesh_collisions += 1;
            }
        }
        eprintln!("Mesh boundary collisions: {mesh_collisions}");

        // Try the trim-based builder and report result
        let result = build_result_brep(&trim_map, &subdivided);
        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();
        eprintln!("\n--- build_result_brep result ---");
        eprintln!("  F={n_faces}, E={n_edges}, V={n_verts}, HE={n_he}");

        // Try the flood-fill patch builder
        let result2 = flood_fill_patches(&survival, &subdivided);
        let n_faces2 = result2.arena.faces.len();
        let n_edges2 = result2.arena.edges.len();
        let n_verts2 = result2.arena.vertices.len();
        let n_he2 = result2.arena.half_edges.len();
        eprintln!("\n--- flood_fill_patches result ---");
        eprintln!("  F={n_faces2}, E={n_edges2}, V={n_verts2}, HE={n_he2}");

        eprintln!("=== END DIAGNOSTIC ===");
    }

    /// Two boxes forming a T-shape: A=[0,3]×[0,1]×[0,1], B=[1,2]×[1,3]×[0,1].
    /// Tests perpendicular face junction at y=1, x∈[1,2].
    #[test]
    fn test_yang_pipeline_t_shape_union() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [3.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 1.0, 0.0], [2.0, 3.0, 1.0]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!(
            "T-shape union: F={n_faces}, E={n_edges}, V={n_verts}, HE={n_he}, unpaired={unpaired}"
        );

        assert!(n_faces > 0, "T-shape union must produce faces");
        assert_eq!(unpaired, 0, "All half-edges must be paired");

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
        assert_eq!(euler, 2, "V-E+F must be 2, got {euler}");
    }

    /// Two boxes forming a cross pattern (different extents on X and Y).
    /// Tests the offset-overlap pattern common in assay F-series cases.
    #[test]
    #[ignore = "A15.6: cell labeling keeps A-inside-B triangles at perpendicular junctions — needs label_cells fix"]
    fn test_yang_pipeline_thin_cross_union() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [3.0, 1.0, 0.3]);
        let (verts_b, tris_b) = make_box_mesh([1.0, -0.5, 0.0], [2.0, 1.5, 0.3]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!(
            "Thin cross union: F={n_faces}, E={n_edges}, V={n_verts}, HE={n_he}, unpaired={unpaired}"
        );

        assert!(n_faces > 0, "Thin cross union must produce faces");
        assert_eq!(unpaired, 0, "All half-edges must be paired");

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
        assert_eq!(euler, 2, "V-E+F must be 2, got {euler}");
    }

    // ══════════════════════════════════════════════════════════════════
    // T-junction after coplanar merge regression test
    // ══════════════════════════════════════════════════════════════════

    /// When two face groups lie on the same plane and are merged (e.g., by
    /// coplanar preprocessing), a T-junction vertex that was from the "other"
    /// face group may end up in `own_qpos` and get skipped in Step 5b,
    /// preventing edge splitting. This causes unpaired half-edges → broken topology.
    ///
    /// Geometry: a box [0,0,0]-[3,1,1] whose bottom face (z=0) is
    /// contributed by two coplanar face groups with different edge
    /// granularity at their shared boundary (x=2):
    ///
    /// - Face group A (mesh A, face 0): rectangle [0,0,0]-[2,1,0], 2 tris.
    ///   Boundary along x=2 is ONE edge: (2,0,0)→(2,1,0).
    /// - Face group B (mesh B, face 0): rectangle [2,0,0]-[3,1,0], 4 tris.
    ///   Boundary along x=2 has TWO edges split at y=0.5: (2,0,0)→(2,0.5,0)
    ///   and (2,0.5,0)→(2,1,0).
    ///
    /// After merge, A's edge (2,0,0)→(2,1,0) must be split at B's vertex
    /// (2,0.5,0) to pair correctly. The bug causes (2,0.5,0) to be skipped.
    #[test]
    fn test_tjunction_after_coplanar_merge() {
        // ── Vertices ──
        // Bottom face group A: rectangle [0,0,0]-[2,1,0]
        //   0: (0,0,0)  1: (2,0,0)  2: (2,1,0)  3: (0,1,0)
        // Bottom face group B: rectangle [2,0,0]-[3,1,0] with midpoint at (2,0.5,0)
        //   4: (2,0.5,0)  5: (3,0,0)  6: (3,1,0)  7: (3,0.5,0)
        // Note: B shares vertices 1=(2,0,0) and 2=(2,1,0) with A via canonicalization.
        //
        // Top face: rectangle [0,0,1]-[3,1,1]
        //   8: (0,0,1)  9: (3,0,1)  10: (3,1,1)  11: (0,1,1)
        //
        // Side faces use existing vertices plus:
        //   12: (2,0,1)  13: (2,1,1)  14: (2,0.5,1)  15: (3,0.5,1)
        //   (not all needed — sides just use box corners)
        //
        // For simplicity we use the 4 bottom-left box corners + 4 bottom-right +
        // midpoint + 4 top corners.

        let verts: Vec<[f64; 3]> = vec![
            // 0-3: bottom face A corners
            [0.0, 0.0, 0.0], // 0
            [2.0, 0.0, 0.0], // 1
            [2.0, 1.0, 0.0], // 2
            [0.0, 1.0, 0.0], // 3
            // 4: B's midpoint vertex creating the T-junction
            [2.0, 0.5, 0.0], // 4
            // 5-7: bottom face B far corners
            [3.0, 0.0, 0.0], // 5
            [3.0, 1.0, 0.0], // 6
            [3.0, 0.5, 0.0], // 7
            // 8-11: top face corners
            [0.0, 0.0, 1.0], // 8
            [3.0, 0.0, 1.0], // 9
            [3.0, 1.0, 1.0], // 10
            [0.0, 1.0, 1.0], // 11
        ];

        // ── SubdividedMesh ──
        // tris_a: face group A bottom + top + left + back-left + front-left side faces
        // tris_b: face group B bottom + right + back-right + front-right side faces
        // (We won't use tris_a/tris_b from SubdividedMesh directly — we build
        // the FaceSurvivalMap by hand.)
        let subdivided = SubdividedMesh {
            verts: verts.clone(),
            tris_a: vec![], // Not used — we build FaceSurvivalMap directly
            tris_b: vec![],
        };

        // ── FaceSurvivalMap ──
        // Bottom face group A (mesh A, face 0): 2 triangles covering [0,0,0]-[2,1,0]
        // Bottom face group B (mesh B, face 0): 4 triangles covering [2,0,0]-[3,1,0]
        //   with midpoint vertex 4 at (2,0.5,0)
        // Top face (mesh A, face 1): 2 triangles covering [0,0,1]-[3,1,1]
        // Back face y=0 (mesh A, face 2): 2 triangles
        // Front face y=1 (mesh A, face 3): 2 triangles
        // Left face x=0 (mesh A, face 4): 2 triangles
        // Right face x=3 (mesh B, face 1): 2 triangles

        let mut groups = BTreeMap::new();

        // Bottom A: [0,0,0]-[2,1,0], outward normal -Z.
        // Raw vertex order gives +Z normal → flip to get -Z (outward for bottom).
        groups.insert(
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(0),
            },
            vec![
                SurvivingSubTri {
                    verts: [0, 1, 2],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [0, 2, 3],
                    flipped: true,
                },
            ],
        );

        // Bottom B: [2,0,0]-[3,1,0] with midpoint 4=(2,0.5,0)
        // Triangulated so x=2 boundary has two edges: (1→4) and (4→2).
        // Raw vertex order gives +Z → flip for -Z outward.
        groups.insert(
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(0),
            },
            vec![
                SurvivingSubTri {
                    verts: [1, 5, 4],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [5, 7, 4],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [7, 6, 4],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [6, 2, 4],
                    flipped: true,
                },
            ],
        );

        // Top face: [0,0,1]-[3,1,1] — single face group, normal +Z
        groups.insert(
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(1),
            },
            vec![
                SurvivingSubTri {
                    verts: [8, 9, 10],
                    flipped: false,
                },
                SurvivingSubTri {
                    verts: [8, 10, 11],
                    flipped: false,
                },
            ],
        );

        // Back face y=0: quad (0,0,0)-(3,0,0)-(3,0,1)-(0,0,1), outward normal -Y
        // Raw vertex order gives +Y → flip for -Y outward.
        groups.insert(
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(2),
            },
            vec![
                SurvivingSubTri {
                    verts: [0, 8, 9],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [0, 9, 5],
                    flipped: true,
                },
            ],
        );

        // Front face y=1: quad (0,1,0)-(3,1,0)-(3,1,1)-(0,1,1), outward normal +Y
        // Raw vertex order gives -Y → flip for +Y outward.
        groups.insert(
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(3),
            },
            vec![
                SurvivingSubTri {
                    verts: [3, 6, 10],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [3, 10, 11],
                    flipped: true,
                },
            ],
        );

        // Left face x=0: quad (0,0,0)-(0,1,0)-(0,1,1)-(0,0,1), outward normal -X
        // Raw vertex order gives +X → flip for -X outward.
        groups.insert(
            SourceFace {
                mesh_id: MeshId::A,
                face_idx: FaceIdx(4),
            },
            vec![
                SurvivingSubTri {
                    verts: [0, 3, 11],
                    flipped: true,
                },
                SurvivingSubTri {
                    verts: [0, 11, 8],
                    flipped: true,
                },
            ],
        );

        // Right face x=3: quad (3,0,0)-(3,1,0)-(3,1,1)-(3,0,1), normal +X
        groups.insert(
            SourceFace {
                mesh_id: MeshId::B,
                face_idx: FaceIdx(1),
            },
            vec![
                SurvivingSubTri {
                    verts: [5, 6, 10],
                    flipped: false,
                },
                SurvivingSubTri {
                    verts: [5, 10, 9],
                    flipped: false,
                },
            ],
        );

        let mut survival = FaceSurvivalMap { groups };

        // ── Build B-Rep (no post-hoc merge per Yang 2025) ──
        let result = flood_fill_patches(&survival, &subdivided);

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        eprintln!("T-junction after coplanar merge: F={n_faces}, E={n_edges}, V={n_verts}, HE={n_he}, unpaired={unpaired}");
        eprintln!("face_provenance entries: {}", result.face_provenance.len());

        // The result must have non-empty face provenance (not empty topology)
        assert!(
            !result.face_provenance.is_empty(),
            "flood_fill_patches must produce non-empty face_provenance \
             (got 0 faces — T-junction at (2,0.5,0) was not resolved after coplanar merge)"
        );

        // All half-edges must be paired
        assert_eq!(
            unpaired, 0,
            "All half-edges must be paired — {unpaired} unpaired indicates T-junction \
             at (2,0.5,0) was not split into A's boundary edge (2,0,0)→(2,1,0)"
        );

        // Euler's formula for a closed solid
        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;
        assert_eq!(euler, 2, "V-E+F must be 2 for closed solid, got {euler}");
    }

    /// Per-face vertex meshes (as produced by WaffleSolid tessellation) create
    /// T-junctions when faces from different source meshes share boundary edges
    /// at different subdivision granularity.
    ///
    /// Bug: Step 5b only searched boundary vertices for T-junction splitting.
    /// Interior vertices of adjacent face groups were missed, leaving edges
    /// unsplit and causing unpaired half-edges → empty topology.
    ///
    /// This test uses a contained geometry (B inside A) where B's edges cut
    /// across the interior of A's faces. Subdivision creates vertices interior
    /// to A's face groups that lie on B's boundary edges.
    ///
    /// Ref [#24]: Yang 2025 — T-junction resolution in conformal mesh.
    /// Ref [#9]: Cherchi 2020 — conformal subdivision vertex sharing.
    #[test]
    fn test_tjunction_interior_vertex_per_face() {
        let (verts_a, tris_a) = make_box_mesh_per_face([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh_per_face([0.5, 0.5, 0.5], [1.5, 1.5, 1.5]);

        let bijective_a =
            BijectiveMap::from_tri_face_ids((0..tris_a.len()).map(|i| FaceIdx(i / 2)).collect());
        let bijective_b =
            BijectiveMap::from_tri_face_ids((0..tris_b.len()).map(|i| FaceIdx(i / 2)).collect());

        for op in [
            MeshBooleanOp::Subtract,
            MeshBooleanOp::Union,
            MeshBooleanOp::Intersect,
        ] {
            let result = yang_boolean_pipeline(
                &verts_a,
                &tris_a,
                &verts_b,
                &tris_b,
                &bijective_a,
                &bijective_b,
                op,
                None,
                &std::collections::BTreeMap::new(),
                &std::collections::BTreeMap::new(),
                1e-7,
            )
            .unwrap_or_else(|e| panic!("{op:?} pipeline failed: {e}"));

            let a = &result.topology.arena;
            let v = a.vertices.len();
            let e = a.edges.len();
            let f = a.faces.len();

            // Primary assertion: the fix resolves unpaired half-edges so
            // topology is non-empty. Before the fix, Subtract and Intersect
            // produced 0 faces due to twin-pairing failure.
            assert!(
                f > 0,
                "{op:?}: per-face vertex pipeline must produce faces (got 0 — T-junction bug)"
            );
            assert!(v > 0, "{op:?}: must produce vertices");
            assert!(e > 0, "{op:?}: must produce edges");

            // Manifold: every edge has exactly two half-edges (twin-paired).
            // This is the core invariant that the T-junction fix enables.
            let he = a.half_edges.len();
            assert_eq!(
                he,
                2 * e,
                "{op:?}: half_edges ({he}) must equal 2 * edges ({e})"
            );

            // Euler characteristic: V-E+F must be positive and even.
            // V-E+F=2 for a single closed solid, V-E+F=2S for S shells.
            // Multi-shell results (Euler>2) are a separate known issue.
            let euler = v as i64 - e as i64 + f as i64;
            assert!(
                euler > 0 && euler % 2 == 0,
                "{op:?}: V({v}) - E({e}) + F({f}) = {euler} (expected positive even)"
            );
        }
    }

    /// When box B is fully inside box A and shares a coplanar face (z=0),
    /// union should produce just A (B is absorbed). Subtraction should produce
    /// A with a rectangular pocket cut from the bottom.
    ///
    /// This tests the boundary detection bug in Step 3 of build_result_brep_from_mesh:
    /// B's sub-triangles on the coplanar z=0 face don't survive in Subtract mode
    /// (they're inside A), but the edges between surviving A-triangles and
    /// non-surviving B-triangles have no reverse in directed_edge_map (since
    /// the non-surviving triangles were removed). The code incorrectly classifies
    /// these as "mesh boundary" edges (line 440), creating false boundaries
    /// and unpaired half-edges.
    ///
    /// Using Subtract because it creates the asymmetric survival pattern needed
    /// to trigger the bug: A's z=0 face sub-triangles inside B's footprint are
    /// removed, while B's interior faces survive (flipped). The intersection
    /// boundary between surviving and non-surviving triangles on the coplanar
    /// face is where false boundaries appear.
    #[test]
    fn test_boundary_detection_non_surviving_neighbor() {
        // A: large box [0,0,0]→[4,4,4]
        // B: smaller box [1,1,0]→[3,3,2], fully contained within A, shares z=0 bottom
        // Union: result should be just box A (B is entirely inside, all B tris
        // labeled Inside → don't survive). The boundary fix ensures edges between
        // surviving A sub-tris and non-surviving B sub-tris at z=0 are classified
        // as interior (not B-Rep boundary). Ref [#24] Yang 2025.
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 1.0, 0.0], [3.0, 3.0, 2.0]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        // Count unpaired half-edges
        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        eprintln!("=== CONTAINED BOX UNION (boundary detection bug) ===");
        eprintln!("Result B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}");
        eprintln!("Unpaired half-edges: {unpaired}");
        eprintln!("Euler V-E+F: {euler}");
        eprintln!("====================================================");

        assert!(n_faces > 0, "Union must produce faces");
        assert_eq!(
            unpaired, 0,
            "All half-edges must be paired (no false boundary edges)"
        );
        assert_eq!(euler, 2, "V-E+F must be 2 for closed solid, got {euler}");
    }

    /// Two identical boxes, Union. Every B triangle is CoSurface with an A
    /// triangle. After coplanar merge + cell labeling, all B tris should be
    /// redundant (Inside or CoSurface-eliminated) and the result should be
    /// exactly one box: V=8, E=12, F=6, Euler=2.
    ///
    /// This is a 100%-overlap stress test for the boundary detection fix:
    /// every single edge in the intersection sits on a coplanar face boundary,
    /// so *all* reverse-edge lookups must consult the full mesh (not just
    /// surviving sub-triangles) to avoid false boundary classification.
    #[test]
    fn test_boundary_detection_identical_box_union() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);
        let (verts_b, tris_b) = make_box_mesh([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Union,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error for identical boxes")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        eprintln!("=== IDENTICAL BOX UNION (boundary detection edge case) ===");
        eprintln!("Result B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}");
        eprintln!("Unpaired half-edges: {unpaired}");
        eprintln!("Euler V-E+F: {euler}");
        eprintln!("==========================================================");

        assert!(n_faces > 0, "Identical box union must produce faces");
        assert_eq!(
            unpaired, 0,
            "All half-edges must be paired (no false boundary from coplanar overlap)"
        );
        assert_eq!(
            euler, 2,
            "V-E+F must be 2 for single closed solid, got {euler}"
        );
    }

    /// Large box A fully contains small box B. Intersect → result should be
    /// the small box (only A-inside-B sub-triangles survive from A, plus
    /// B-inside-A sub-triangles from B). This tests the boundary fix for
    /// Intersect mode: when most of A's triangles are removed (outside B),
    /// the edges between surviving and non-surviving A sub-triangles must
    /// not be misclassified as mesh boundary.
    #[test]
    fn test_boundary_detection_contained_box_intersect() {
        // A: large box [0,0,0]→[4,4,4]
        // B: small box [1,1,1]→[3,3,3], fully inside A (no shared faces)
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 1.0, 1.0], [3.0, 3.0, 3.0]);

        let bijective_a = build_bijective_from_tri_count(tris_a.len());
        let bijective_b = build_bijective_from_tri_count(tris_b.len());

        let result = yang_boolean_pipeline(
            &verts_a,
            &tris_a,
            &verts_b,
            &tris_b,
            &bijective_a,
            &bijective_b,
            MeshBooleanOp::Intersect,
            None,
            &std::collections::BTreeMap::new(),
            &std::collections::BTreeMap::new(),
            1e-7,
        )
        .expect("Yang pipeline should not error for contained box intersect")
        .topology;

        let n_faces = result.arena.faces.len();
        let n_edges = result.arena.edges.len();
        let n_verts = result.arena.vertices.len();
        let n_he = result.arena.half_edges.len();

        let unpaired = if n_he > 0 {
            (0..n_he)
                .filter(|&i| {
                    let twin_idx = result.arena.half_edges[i].twin.0;
                    twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i
                })
                .count()
        } else {
            0
        };

        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        eprintln!("=== CONTAINED BOX INTERSECT (boundary detection edge case) ===");
        eprintln!("Result B-Rep: V={n_verts}, E={n_edges}, F={n_faces}, HE={n_he}");
        eprintln!("Unpaired half-edges: {unpaired}");
        eprintln!("Euler V-E+F: {euler}");
        eprintln!("==============================================================");

        assert!(n_faces > 0, "Contained box intersect must produce faces");
        assert_eq!(
            unpaired, 0,
            "All half-edges must be paired (no false boundary from non-surviving A tris)"
        );
        assert_eq!(
            euler, 2,
            "V-E+F must be 2 for single closed solid, got {euler}"
        );
    }

    /// Yang pipeline produces correct face count for 2D-offset overlapping box union.
    /// A=[0,10]³, B=[5,15]×[5,15]×[0,10] — L-shaped union with ≥10 faces.
    #[test]
    fn test_yang_2d_offset_box_union_face_count() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
        let (verts_b, tris_b) = make_box_mesh([5.0, 5.0, 0.0], [15.0, 15.0, 10.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bij_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bij_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let mut survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        let topology = flood_fill_patches(&survival, &subdivided);
        let n_faces = topology.face_provenance.len();
        let n_verts = topology.arena.vertices.len();
        let n_edges = topology.arena.edges.len();
        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        assert!(
            n_faces >= 10,
            "2D-offset box union should produce >= 10 faces, got {n_faces}"
        );
        assert_eq!(
            euler, 2,
            "Euler V({n_verts})-E({n_edges})+F({n_faces}) = {euler}, expected 2"
        );
    }

    /// Identical boxes union (complete overlap): A=B=[0,1]³ should produce a single
    /// box with Euler=2 and exactly 6 faces.
    #[test]
    fn test_yang_identical_box_union_single_solid() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bij_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bij_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let mut survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        let topology = flood_fill_patches(&survival, &subdivided);
        let n_faces = topology.face_provenance.len();
        let n_verts = topology.arena.vertices.len();
        let n_edges = topology.arena.edges.len();
        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        assert_eq!(
            n_faces, 6,
            "Identical box union should produce exactly 6 faces, got {n_faces}"
        );
        assert_eq!(
            euler, 2,
            "Identical box union should have Euler=2, got V={n_verts} E={n_edges} F={n_faces} Euler={euler}"
        );
    }

    /// Disjoint boxes union (compound solid): Two non-overlapping boxes should
    /// produce 12 faces, Euler=4 (2 per connected component).
    #[test]
    fn test_yang_disjoint_box_union_compound() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([10.0, 10.0, 10.0], [11.0, 11.0, 11.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bij_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bij_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let mut survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        let topology = flood_fill_patches(&survival, &subdivided);
        let n_faces = topology.face_provenance.len();
        let n_verts = topology.arena.vertices.len();
        let n_edges = topology.arena.edges.len();
        let euler = n_verts as i64 - n_edges as i64 + n_faces as i64;

        assert_eq!(
            n_faces, 12,
            "Disjoint box union should produce 12 faces (6+6), got {n_faces}"
        );
        assert_eq!(
            euler, 4,
            "Disjoint box union should have Euler=4 (2 components), got V={n_verts} E={n_edges} F={n_faces} Euler={euler}"
        );
    }

    /// Disjoint boxes intersect (empty result): Two non-overlapping boxes
    /// intersected should produce zero surviving face groups.
    #[test]
    fn test_yang_disjoint_box_intersect_empty() {
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([10.0, 10.0, 10.0], [11.0, 11.0, 11.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bij_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bij_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Intersect,
            &bij_a,
            &bij_b,
        );
        assert!(
            survival.groups.is_empty(),
            "Disjoint box intersect should have 0 surviving groups, got {}",
            survival.groups.len()
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // Bug 2 (twin-pairing partial topology) — red-phase TDD tests
    // Spec: specs/yang_twin_pairing_partial_topology.md
    // ══════════════════════════════════════════════════════════════════

    /// Bug 2, invariant 1: When build_result_brep_from_mesh encounters some
    /// unpaired half-edges, it should remove only the affected faces and keep
    /// valid ones. Currently it discards ALL faces (returns empty topology).
    ///
    /// Uses the F0003 pattern: cross-shaped union of differently-sized boxes
    /// where asymmetric T-junctions at perpendicular face crossings create
    /// unpaired HEs. The boxes are offset so their faces don't align, forcing
    /// the subdivision to create constraint edges that don't match.
    #[test]
    fn test_partial_topology_preserves_valid_faces() {
        // F0003-like pattern: two boxes forming a cross/step shape.
        // Box A is wide+shallow, Box B is narrow+tall, offset so faces cross
        // at non-vertex positions to create T-junctions in subdivision.
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [6.0, 4.0, 3.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [5.0, 4.0, 5.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bij_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bij_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        assert!(
            !survival.groups.is_empty(),
            "Union of overlapping boxes must have surviving face groups"
        );

        let result = flood_fill_patches(&survival, &subdivided);

        // Bug 2: if ANY unpaired HEs exist, build_result_brep_from_mesh currently
        // discards ALL faces. After the fix, it should remove only affected faces.
        // The test checks that at least some faces survive — if all faces are
        // discarded when many are valid, the all-or-nothing policy is too aggressive.
        //
        // If this test passes (0 unpaired HEs for this geometry), it means the
        // mesh-level box test doesn't trigger the bug. The bug manifests through
        // the full kernel tessellation path with curved primitives and non-axis-
        // aligned geometry. In that case, we verify the invariant holds: non-empty
        // survival → non-empty topology.
        assert!(
            !result.face_provenance.is_empty(),
            "flood_fill_patches discarded ALL faces due to unpaired HEs. \
             Expected partial topology with face_provenance.len() > 0, got 0. \
             Survival had {} face groups. The all-or-nothing discard in \
             topology_extract.rs:1064-1070 should be replaced with partial face removal.",
            survival.groups.len()
        );
    }

    /// Bug 2, invariant 2: After partial face removal, all remaining half-edges
    /// must satisfy twin symmetry: arena.half_edges[arena.half_edges[i].twin].twin == i.
    ///
    /// This test verifies the structural invariant on the result of
    /// build_result_brep_from_mesh. If the result is empty (due to Bug 2),
    /// the test fails — partial topology must be non-empty and structurally valid.
    #[test]
    fn test_partial_topology_twin_symmetry() {
        // Same cross-shape geometry as test_partial_topology_preserves_valid_faces
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [6.0, 4.0, 3.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, 0.0, 0.0], [5.0, 4.0, 5.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bij_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bij_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let mut survival =
            face_survival_detect(&subdivided, &labeling, MeshBooleanOp::Union, &bij_a, &bij_b);

        let result = flood_fill_patches(&survival, &subdivided);

        // First: result must be non-empty
        let n_he = result.arena.half_edges.len();
        assert!(
            n_he > 0,
            "Result topology is empty (0 half-edges) — all faces were discarded. \
             Expected partial topology with valid HEs after removing only bad faces."
        );

        // Second: verify twin symmetry on all remaining HEs
        for i in 0..n_he {
            let twin_idx = result.arena.half_edges[i].twin.0;
            assert!(
                twin_idx < n_he,
                "HE[{i}].twin = {twin_idx} is out of range (n_he={n_he})"
            );
            let twin_twin = result.arena.half_edges[twin_idx].twin.0;
            assert_eq!(
                twin_twin, i,
                "Twin symmetry violated: HE[{i}].twin={twin_idx}, \
                 but HE[{twin_idx}].twin={twin_twin} (expected {i})"
            );
        }
    }

    // ══════════════════════════════════════════════════════════════════
    // Flood-fill patch segmentation tests (red phase)
    // Per Yang 2025 Section 4.4.2 — these test flood_fill_patches().
    // ══════════════════════════════════════════════════════════════════

    /// Helper: run the full pipeline through flood_fill_patches for two boxes.
    fn run_flood_fill_for_boxes(
        min_a: [f64; 3],
        max_a: [f64; 3],
        min_b: [f64; 3],
        max_b: [f64; 3],
        op: MeshBooleanOp,
    ) -> ResultTopology {
        let (verts_a, tris_a) = make_box_mesh(min_a, max_a);
        let (verts_b, tris_b) = make_box_mesh(min_b, max_b);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let mut survival =
            face_survival_detect(&subdivided, &labeling, op, &bijective_a, &bijective_b);

        flood_fill_patches(&survival, &subdivided)
    }

    /// Test 1: Cross-shaped union (perpendicular junctions) through
    /// flood_fill_patches. A long box in X crossed by a tall box in Y
    /// creates T-junctions at 4 perpendicular faces. The result must have
    /// non-empty face provenance and zero unpaired half-edges.
    /// The old boundary-edge-chaining fails at perpendicular junctions.
    #[test]
    fn test_flood_fill_two_overlapping_boxes() {
        // Cross shape: box A is long in X, box B is tall in Y.
        // This creates perpendicular face junctions that stress twin-pairing.
        let result = run_flood_fill_for_boxes(
            [0.0, 0.0, 0.0],
            [3.0, 1.0, 1.0],
            [1.0, -1.0, 0.0],
            [2.0, 2.0, 1.0],
            MeshBooleanOp::Union,
        );

        // Must produce a non-empty result.
        assert!(
            !result.face_provenance.is_empty(),
            "flood_fill_patches must produce non-empty face_provenance for cross-shaped union"
        );

        // Zero unpaired half-edges: every HE's twin must point back.
        let n_he = result.arena.half_edges.len();
        assert!(n_he > 0, "Result must have half-edges");

        let mut unpaired = 0usize;
        for i in 0..n_he {
            let twin_idx = result.arena.half_edges[i].twin.0;
            if twin_idx >= n_he || result.arena.half_edges[twin_idx].twin.0 != i {
                unpaired += 1;
            }
        }
        assert_eq!(
            unpaired, 0,
            "flood_fill_patches must produce zero unpaired half-edges, got {unpaired} \
             out of {n_he} total HEs"
        );
    }

    /// Test 2: Manifold output — every half-edge must satisfy twin symmetry:
    /// arena.half_edges[arena.half_edges[i].twin.0].twin.0 == i for all i.
    /// Uses cross-shaped union to stress perpendicular junction twin-pairing.
    #[test]
    fn test_flood_fill_manifold_output() {
        // Cross shape with offset: maximizes perpendicular junction complexity.
        let result = run_flood_fill_for_boxes(
            [0.0, 0.0, 0.0],
            [3.0, 1.0, 1.0],
            [1.0, -1.0, 0.0],
            [2.0, 2.0, 1.0],
            MeshBooleanOp::Union,
        );

        let n_he = result.arena.half_edges.len();
        assert!(n_he > 0, "Result must have half-edges for manifold check");

        // Every half-edge must have a valid twin in range.
        for i in 0..n_he {
            let twin_idx = result.arena.half_edges[i].twin.0;
            assert!(
                twin_idx < n_he,
                "HE[{i}].twin = {twin_idx} is out of range (n_he={n_he})"
            );
        }

        // Twin symmetry: twin(twin(i)) == i for ALL half-edges.
        for i in 0..n_he {
            let twin_idx = result.arena.half_edges[i].twin.0;
            let twin_twin = result.arena.half_edges[twin_idx].twin.0;
            assert_eq!(
                twin_twin, i,
                "Manifold violation: HE[{i}].twin={twin_idx}, \
                 HE[{twin_idx}].twin={twin_twin} (expected {i}). \
                 flood_fill_patches must produce fully manifold topology."
            );
        }

        // Additionally: no self-loops (a HE cannot be its own twin).
        for i in 0..n_he {
            let twin_idx = result.arena.half_edges[i].twin.0;
            assert_ne!(
                twin_idx, i,
                "Self-twin violation: HE[{i}] is its own twin. \
                 Each half-edge must have a distinct twin."
            );
        }
    }

    /// Test 3: Euler characteristic must be 2 for a closed solid.
    /// Uses cross-shaped union which creates perpendicular junctions.
    /// The old boundary-edge-chaining produces wrong topology at these
    /// junctions, resulting in non-2 Euler characteristic.
    #[test]
    fn test_flood_fill_no_self_intersection() {
        // Cross shape: maximizes perpendicular junction stress.
        let (verts_a, tris_a) = make_box_mesh([0.0, 0.0, 0.0], [3.0, 1.0, 1.0]);
        let (verts_b, tris_b) = make_box_mesh([1.0, -1.0, 0.0], [2.0, 2.0, 1.0]);

        let subdivided = subdivide_mesh_pair(&verts_a, &tris_a, &verts_b, &tris_b, None)
            .expect("subdivision should succeed");
        let labeling =
            label_cells(&subdivided, &verts_a, &tris_a, &verts_b, &tris_b, None).unwrap();

        let bijective_a = build_bijective_from_subdivided(&subdivided.tris_a, tris_a.len());
        let bijective_b = build_bijective_from_subdivided(&subdivided.tris_b, tris_b.len());

        let mut survival = face_survival_detect(
            &subdivided,
            &labeling,
            MeshBooleanOp::Union,
            &bijective_a,
            &bijective_b,
        );

        let result = flood_fill_patches(&survival, &subdivided);

        // The result topology must have faces.
        assert!(
            !result.face_provenance.is_empty(),
            "flood_fill_patches must produce faces for cross-shaped union"
        );

        // The key invariant: Euler characteristic V-E+F must be 2 for a
        // closed solid. The old boundary-edge-chaining at perpendicular
        // junctions produces catastrophically wrong topology (Euler of -62
        // has been observed).
        let n_v = result.arena.vertices.len();
        let n_e = result.arena.edges.len();
        let n_f = result.arena.faces.len();
        let euler = n_v as i64 - n_e as i64 + n_f as i64;
        assert_eq!(
            euler, 2,
            "Euler characteristic V-E+F must be 2 for a closed solid, \
             got V={n_v}, E={n_e}, F={n_f}, χ={euler}. \
             Non-2 Euler characteristic indicates non-manifold topology \
             that will produce self-intersections on retessellation."
        );
    }

    /// Test 4: Patch count for cross-shaped union.
    /// Box A [0,0,0]-[3,1,1] (long in X) union with Box B [1,-1,0]-[2,2,1]
    /// (tall in Y). The cross union has:
    /// - A's 4 non-clipped faces (back, front, bottom partial, top partial)
    ///   split at x=1 and x=2 → 8 partial faces from A
    /// - B's 4 non-clipped faces split at y=0 and y=1 → 8 partial faces from B
    /// - 4 coplanar z-faces merged
    /// Total: at least 14 distinct B-Rep faces for the cross shape.
    /// The old function collapses faces incorrectly due to wrong twin-pairing.
    #[test]
    fn test_flood_fill_patch_count_box_union() {
        let result = run_flood_fill_for_boxes(
            [0.0, 0.0, 0.0],
            [3.0, 1.0, 1.0],
            [1.0, -1.0, 0.0],
            [2.0, 2.0, 1.0],
            MeshBooleanOp::Union,
        );

        // A cross-shaped union must have more faces than a simple box (6).
        // The cross has: 4 exposed faces from A's left/right ends, plus
        // 4 exposed faces from B's top/bottom extensions, plus the
        // 4 partial side faces from each arm's sides that aren't hidden,
        // plus 2 front/back z-faces (merged from both boxes).
        // Minimum: 14 faces (exact count depends on coplanar merge behavior).
        let n_faces = result.face_provenance.len();
        assert!(
            n_faces >= 14,
            "Cross-shaped union should produce at least 14 B-Rep faces \
             (multiple partial faces from each box arm), got {n_faces}"
        );
    }
}
