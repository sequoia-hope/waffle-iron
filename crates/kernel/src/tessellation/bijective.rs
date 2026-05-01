//! Bijective tessellation mapping — maps each mesh triangle to its source B-Rep face.
//!
//! This is stage 1 infrastructure for the Yang 2025 hybrid B-Rep/mesh boolean
//! pipeline [#24]. After exact mesh boolean (stage 2), the bijective map enables
//! topology extraction (stage 3): determining which original B-Rep faces survive
//! and how they are trimmed.
//!
//! Ref #24: Yang, Jia & Yan (2025) — Hybrid B-Rep/mesh boolean pipeline.
//! Ref #9: Cherchi et al. (2020) — Fast exact mesh arrangements.

use crate::topology::arena::TopoArena;
use crate::topology::half_edge::{EdgeIdx, FaceIdx};
use crate::types::RenderMesh;
use std::collections::BTreeMap;

/// Maps each mesh triangle to its source B-Rep face, with optional parametric
/// coordinates per vertex for the Yang 2025 bijective mapping requirement.
///
/// Invariant: `tri_face_ids.len() == mesh.indices.len() / 3` — every triangle
/// maps to exactly one face (bijective property).
/// Ref [#24]: Yang 2025 Section 4.1 — bijective mapping between mesh and surfaces.
#[derive(Debug, Clone)]
pub struct BijectiveMap {
    /// For triangle `i`, `tri_face_ids[i]` is the source B-Rep face index.
    pub tri_face_ids: Vec<FaceIdx>,
    /// For vertex `i`, parametric (u, v) coordinates on its source surface.
    /// None for planar-face vertices or when inverse evaluation is unavailable.
    /// Populated by `compute_vertex_params()` after tessellation.
    pub vertex_params: Vec<Option<(f64, f64)>>,
}

impl BijectiveMap {
    /// Create a BijectiveMap from triangle-to-face mappings.
    /// vertex_params starts empty; call compute_vertex_params() to populate.
    pub fn from_tri_face_ids(tri_face_ids: Vec<FaceIdx>) -> Self {
        BijectiveMap {
            tri_face_ids,
            vertex_params: Vec::new(),
        }
    }

    /// Derive a `BijectiveMap` from a `RenderMesh` and its `face_map`.
    ///
    /// Each `FaceRange` in the mesh tells us which triangle index range belongs
    /// to which `KernelId`. We invert the `face_map` (KernelId → FaceIdx) to
    /// recover the B-Rep `FaceIdx` for each triangle.
    ///
    /// Triangles whose `KernelId` is not found in `face_map` are mapped to
    /// `FaceIdx(usize::MAX)` as a sentinel (should not happen in correct usage).
    pub fn from_render_mesh(mesh: &RenderMesh, face_map: &BTreeMap<u64, FaceIdx>) -> Self {
        let tri_count = mesh.indices.len() / 3;
        let mut tri_face_ids = vec![FaceIdx(usize::MAX); tri_count];

        // Invert face_map: KernelId(u64) → FaceIdx
        let kid_to_face: BTreeMap<u64, FaceIdx> = face_map.clone();

        for range in &mesh.face_ranges {
            let face_idx = kid_to_face
                .get(&range.face_id.0)
                .copied()
                .unwrap_or(FaceIdx(usize::MAX));

            // FaceRange uses index offsets into the indices array.
            // Each triangle is 3 consecutive indices.
            let start_tri = range.start_index as usize / 3;
            let end_tri = range.end_index as usize / 3;

            let end = end_tri.min(tri_count);
            if start_tri < end {
                tri_face_ids[start_tri..end].fill(face_idx);
            }
        }

        BijectiveMap {
            tri_face_ids,
            vertex_params: Vec::new(), // populated later by compute_vertex_params
        }
    }

    /// Number of triangles in this map.
    pub fn tri_count(&self) -> usize {
        self.tri_face_ids.len()
    }

    /// Check the bijective invariant: every triangle maps to a valid face
    /// (not the sentinel value).
    pub fn is_complete(&self) -> bool {
        self.tri_face_ids.iter().all(|f| f.0 != usize::MAX)
    }

    /// Return the set of distinct face indices referenced by this map.
    pub fn referenced_faces(&self) -> Vec<FaceIdx> {
        let mut seen = std::collections::BTreeSet::new();
        for &f in &self.tri_face_ids {
            if f.0 != usize::MAX {
                seen.insert(f);
            }
        }
        seen.into_iter().collect()
    }

    /// Compute parametric (u, v) coordinates for each mesh vertex by projecting
    /// onto the analytical surface of its source face.
    ///
    /// For planar faces, vertex_params remains None (parametric coords not meaningful).
    /// For curved faces, calls `SurfaceGeom::inverse_evaluate()` to recover (u, v).
    /// Ref [#24]: Yang 2025 Section 4.1 — bijective mapping between mesh and surfaces.
    pub fn compute_vertex_params(
        &mut self,
        mesh: &RenderMesh,
        face_geometry: &BTreeMap<FaceIdx, crate::geometry::surface::SurfaceGeom>,
    ) {
        let vert_count = mesh.vertices.len() / 3;
        self.vertex_params = vec![None; vert_count];

        for (tri_idx, &face_idx) in self.tri_face_ids.iter().enumerate() {
            let geom = match face_geometry.get(&face_idx) {
                Some(g) => g,
                None => continue,
            };
            for k in 0..3 {
                let vi = mesh.indices[tri_idx * 3 + k] as usize;
                if vi >= vert_count || self.vertex_params[vi].is_some() {
                    continue;
                }
                let pt = crate::geometry::point::Point3::new(
                    mesh.vertices[vi * 3] as f64,
                    mesh.vertices[vi * 3 + 1] as f64,
                    mesh.vertices[vi * 3 + 2] as f64,
                );
                self.vertex_params[vi] = geom.inverse_evaluate(pt);
            }
        }
    }

    /// Count triangles belonging to a specific face.
    pub fn tri_count_for_face(&self, face: FaceIdx) -> usize {
        self.tri_face_ids.iter().filter(|&&f| f == face).count()
    }
}

// ─── Bijective tessellation oracle ──────────────────────────────────────
//
// The oracle below measures whether tessellation honors the Yang 2025
// §4.1.1 bijective contract: along every B-Rep edge shared by two faces,
// both faces must emit the SAME directed mesh edges (byte-identical f64
// position pairs, oriented oppositely) at the boundary.
//
// "Same" here means byte-identical f64 — no welding, no tolerance. The
// Yang/Cherchi pipeline relies on this exact-equality property so that
// the downstream mesh arrangement (Cherchi 2022 §4) can identify shared
// edges by vertex-id alone, with no fuzzy comparison. The current
// `weld_mesh_vertices` quantization in `boolean/exact_mesh.rs:1684-1754`
// is the symptomatic A15.6 violation (audit finding D-10) that this
// oracle exists to expose.
//
// Operational definition of "non-bijective":
//   The boundary of face A's tessellation consists of directed mesh
//   edges (p, q) that appear in exactly one of face A's triangles in
//   that orientation (i.e., have no in-face partner (q, p)).
//   Per Yang §4.1.1, every such face-A boundary edge that lies on the
//   B-Rep edge shared with face B must appear byte-identically as
//   (q, p) on face B's boundary. If face A has a boundary edge (p, q)
//   that face B doesn't reciprocate, the pair is non-bijective.
//
// This formulation handles linear, circular, and self-loop B-Rep edges
// uniformly without needing curve-geometry-specific reasoning.

/// Diagnostic record for one face pair that violates bijectivity.
#[derive(Debug, Clone)]
pub struct NonBijectivePair {
    pub face_a: FaceIdx,
    pub face_b: FaceIdx,
    /// Source of the shared boundary if known — a B-Rep edge index when
    /// we could trace it through the arena. `None` for the polygon-soup
    /// fallback (no parametric provenance).
    pub edge: Option<EdgeIdx>,
    /// Number of unmatched face-A boundary edges (no byte-identical
    /// (q, p) partner on face B).
    pub unmatched_a_count: usize,
    /// Number of unmatched face-B boundary edges (no byte-identical
    /// (q, p) partner on face A).
    pub unmatched_b_count: usize,
    /// First few unmatched face-A directed boundary edges, for
    /// diagnostics. Each entry is `(p, q)` as f64 positions.
    pub sample_unmatched_a: Vec<([f64; 3], [f64; 3])>,
    /// First few unmatched face-B directed boundary edges.
    pub sample_unmatched_b: Vec<([f64; 3], [f64; 3])>,
}

/// Result of running the bijective oracle on a tessellated mesh.
#[derive(Debug, Clone, Default)]
pub struct BijectivityReport {
    /// Total number of face pairs the oracle examined.
    pub total_pairs_examined: usize,
    /// Pairs whose two faces emitted byte-identical reciprocal boundary
    /// edges (every face-A boundary edge has a face-B (q, p) partner).
    pub bijective_pairs: usize,
    /// Pairs that violated bijectivity (with diagnostic details).
    pub non_bijective_pairs: Vec<NonBijectivePair>,
}

impl BijectivityReport {
    pub fn is_bijective(&self) -> bool {
        self.non_bijective_pairs.is_empty()
    }
}

#[inline]
fn vertex_pos(mesh: &RenderMesh, idx: usize) -> [f64; 3] {
    [
        mesh.vertices[idx * 3] as f64,
        mesh.vertices[idx * 3 + 1] as f64,
        mesh.vertices[idx * 3 + 2] as f64,
    ]
}

#[inline]
fn pos_key(p: [f64; 3]) -> [u64; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

type DirEdgeKey = ([u64; 3], [u64; 3]);
type DirEdge = ([f64; 3], [f64; 3]);
// BTreeMap (not HashMap) for deterministic iteration order. Per T2's PR12
// diagnostic (`docs/audits/pr12_stage1_diagnostic.md` §4): HashMap RandomState
// caused the bijective oracle's verdicts and counts to flap across consecutive
// runs on the same fixture (R0014, R0034, R0046, F0076). The Yang §4.1.1
// matching predicate is order-independent in principle, but iteration over a
// HashMap during e.g. sample collection picks different witnesses on each run.
type DirEdgeMap = std::collections::BTreeMap<DirEdgeKey, DirEdge>;

/// Compute the boundary directed edges of a single face's tessellation.
///
/// A directed edge (p, q) is INTERIOR if (q, p) also appears in this
/// face's triangles — both half-edges are inside the same face. The
/// remainder are BOUNDARY directed edges. They face outward from this
/// face into adjacent faces. Per Yang §4.1.1, each such boundary edge
/// must reciprocate as (q, p) byte-identically on the adjacent face.
fn face_boundary_directed_edges(
    rendermesh: &RenderMesh,
    face_map: &BTreeMap<u64, FaceIdx>,
    target_face: FaceIdx,
) -> DirEdgeMap {
    let mut count: BTreeMap<DirEdgeKey, ([f64; 3], [f64; 3], usize)> = BTreeMap::new();
    for range in &rendermesh.face_ranges {
        let mapped = match face_map.get(&range.face_id.0).copied() {
            Some(f) => f,
            None => continue,
        };
        if mapped != target_face {
            continue;
        }
        let start = range.start_index as usize;
        let end = (range.end_index as usize).min(rendermesh.indices.len());
        let mut i = start;
        while i + 2 < end {
            let v = [
                rendermesh.indices[i] as usize,
                rendermesh.indices[i + 1] as usize,
                rendermesh.indices[i + 2] as usize,
            ];
            for k in 0..3 {
                let p = vertex_pos(rendermesh, v[k]);
                let q = vertex_pos(rendermesh, v[(k + 1) % 3]);
                let key: DirEdgeKey = (pos_key(p), pos_key(q));
                let entry = count.entry(key).or_insert((p, q, 0));
                entry.2 += 1;
            }
            i += 3;
        }
    }
    // Boundary directed edges: count == 1 AND the reverse direction
    // is NOT present (otherwise the edge is interior to the face).
    let mut boundary: DirEdgeMap = BTreeMap::new();
    for (k, &(p, q, _)) in &count {
        let rev = (k.1, k.0);
        if !count.contains_key(&rev) {
            boundary.insert(*k, (p, q));
        }
    }
    boundary
}

/// Match face A's boundary directed edges against face B's reverse
/// directed edges. Returns (unmatched_in_a, unmatched_in_b) — directed
/// edges from each side that lack a byte-identical reciprocal partner.
fn diff_boundaries(
    boundary_a: &DirEdgeMap,
    boundary_b: &DirEdgeMap,
) -> (Vec<DirEdge>, Vec<DirEdge>) {
    let mut un_a = Vec::new();
    for (k, &(p, q)) in boundary_a {
        let rev = (k.1, k.0);
        if !boundary_b.contains_key(&rev) {
            un_a.push((p, q));
        }
    }
    let mut un_b = Vec::new();
    for (k, &(p, q)) in boundary_b {
        let rev = (k.1, k.0);
        if !boundary_a.contains_key(&rev) {
            un_b.push((p, q));
        }
    }
    (un_a, un_b)
}

/// Run the bijective oracle on a tessellated rendermesh.
///
/// For each pair of faces (A, B) that share a B-Rep edge — or, in
/// polygon-soup mode, share at least one position-coincident mesh edge —
/// the oracle compares face A's boundary directed edges against face B's
/// reverse directed edges, byte-identically. Pairs with any unmatched
/// boundary edges are recorded as non-bijective (Yang 2025 §4.1.1).
///
/// **Polygon-soup fallback**: if `arena.faces` is empty (boolean output
/// with no parametric provenance), the oracle falls back to position-based
/// face-pair identification. It enumerates triangles by `face_map` label
/// and finds undirected position-coincident mesh edges that span two
/// distinct face labels — those identify the face pairs to examine.
pub fn check_face_pair_bijective(
    rendermesh: &RenderMesh,
    face_map: &BTreeMap<u64, FaceIdx>,
    arena: &TopoArena,
) -> BijectivityReport {
    if !arena.edges.is_empty() && !arena.faces.is_empty() {
        check_brep_mode(rendermesh, face_map, arena)
    } else {
        check_polygon_soup_mode(rendermesh, face_map)
    }
}

/// Helper: given a face's boundary directed edges, restrict to those
/// whose UNDIRECTED form is also present in the other face's boundary.
/// This identifies edges that lie on the shared B-Rep boundary between
/// the two faces (as opposed to the face's other boundary edges, which
/// are shared with different neighbors).
fn restrict_to_shared_boundary(bnd_self: &DirEdgeMap, bnd_other: &DirEdgeMap) -> DirEdgeMap {
    let undir = |k: &DirEdgeKey| -> ([u64; 3], [u64; 3]) {
        if k.0 <= k.1 {
            (k.0, k.1)
        } else {
            (k.1, k.0)
        }
    };
    let other_undir: std::collections::BTreeSet<([u64; 3], [u64; 3])> =
        bnd_other.keys().map(undir).collect();
    let mut out: DirEdgeMap = BTreeMap::new();
    for (k, v) in bnd_self {
        if other_undir.contains(&undir(k)) {
            out.insert(*k, *v);
        }
    }
    out
}

fn check_brep_mode(
    rendermesh: &RenderMesh,
    face_map: &BTreeMap<u64, FaceIdx>,
    arena: &TopoArena,
) -> BijectivityReport {
    use std::collections::BTreeSet;
    let mut report = BijectivityReport::default();

    // Cache per-face boundary edge sets once (re-used across pairs).
    let mut boundary_cache: BTreeMap<FaceIdx, DirEdgeMap> = BTreeMap::new();
    for &face_idx in face_map.values() {
        boundary_cache
            .entry(face_idx)
            .or_insert_with(|| face_boundary_directed_edges(rendermesh, face_map, face_idx));
    }

    // For each B-Rep edge whose two adjacent faces are distinct, examine
    // the face pair exactly once. (Same pair may share multiple B-Rep
    // edges, e.g. a cylinder seam plus circular edge between the side
    // and a cap — examine the pair once and aggregate.)
    let mut visited_pairs: BTreeSet<(FaceIdx, FaceIdx)> = BTreeSet::new();
    let mut pair_to_edge: BTreeMap<(FaceIdx, FaceIdx), EdgeIdx> = BTreeMap::new();

    for (i, edge) in arena.edges.iter().enumerate() {
        let edge_idx = EdgeIdx(i);
        let he_a_idx = edge.half_edge;
        let he_a = &arena.half_edges[he_a_idx.0];
        let he_b_idx = he_a.twin;
        let he_b = &arena.half_edges[he_b_idx.0];

        if he_a.loop_.0 >= arena.loops.len() || he_b.loop_.0 >= arena.loops.len() {
            continue;
        }
        let face_a = arena.loops[he_a.loop_.0].face;
        let face_b = arena.loops[he_b.loop_.0].face;
        if face_a == face_b {
            continue;
        }
        let pair = if face_a <= face_b {
            (face_a, face_b)
        } else {
            (face_b, face_a)
        };
        pair_to_edge.entry(pair).or_insert(edge_idx);
        visited_pairs.insert(pair);
    }

    for &pair in &visited_pairs {
        let (face_a, face_b) = pair;
        let empty: DirEdgeMap = DirEdgeMap::new();
        let bnd_a = boundary_cache.get(&face_a).unwrap_or(&empty);
        let bnd_b = boundary_cache.get(&face_b).unwrap_or(&empty);

        if bnd_a.is_empty() || bnd_b.is_empty() {
            // No tessellation for one of the faces — cannot judge.
            continue;
        }

        // Restrict to directed boundary edges whose UNDIRECTED form
        // appears in both faces. Otherwise we'd flag every face's
        // OTHER boundary edges (with different neighbors) as unmatched.
        let bnd_a_in_pair = restrict_to_shared_boundary(bnd_a, bnd_b);
        let bnd_b_in_pair = restrict_to_shared_boundary(bnd_b, bnd_a);

        let (un_a, un_b) = diff_boundaries(&bnd_a_in_pair, &bnd_b_in_pair);

        report.total_pairs_examined += 1;
        if un_a.is_empty() && un_b.is_empty() {
            report.bijective_pairs += 1;
        } else {
            let edge = pair_to_edge.get(&pair).copied();
            const SAMPLE: usize = 4;
            report.non_bijective_pairs.push(NonBijectivePair {
                face_a,
                face_b,
                edge,
                unmatched_a_count: un_a.len(),
                unmatched_b_count: un_b.len(),
                sample_unmatched_a: un_a.into_iter().take(SAMPLE).collect(),
                sample_unmatched_b: un_b.into_iter().take(SAMPLE).collect(),
            });
        }
    }

    report
}

fn check_polygon_soup_mode(
    rendermesh: &RenderMesh,
    face_map: &BTreeMap<u64, FaceIdx>,
) -> BijectivityReport {
    use std::collections::BTreeSet;
    let mut report = BijectivityReport::default();

    // Polygon-soup pair detection by SHARED VERTEX presence rather
    // than shared undirected mesh edge. Two faces are candidates for
    // adjacency if they have ≥ 2 byte-identical vertex positions in
    // common — that's enough to anchor a shared B-Rep edge endpoint
    // pair. This catches T-junction cracks: when face B inserts an
    // extra midpoint M that face A doesn't have, the undirected mesh
    // edges differ on the two sides, but the endpoints of the shared
    // B-Rep edge still appear in both face vertex sets, exposing the
    // pair to inspection.

    let mut face_vertices: BTreeMap<FaceIdx, BTreeSet<[u64; 3]>> = BTreeMap::new();
    let mut all_face_labels: BTreeSet<FaceIdx> = BTreeSet::new();

    for range in &rendermesh.face_ranges {
        let face_label = match face_map.get(&range.face_id.0).copied() {
            Some(f) => f,
            None => continue,
        };
        all_face_labels.insert(face_label);
        let start = range.start_index as usize;
        let end = (range.end_index as usize).min(rendermesh.indices.len());
        let mut i = start;
        while i + 2 < end {
            for k in 0..3 {
                let p = vertex_pos(rendermesh, rendermesh.indices[i + k] as usize);
                face_vertices
                    .entry(face_label)
                    .or_default()
                    .insert(pos_key(p));
            }
            i += 3;
        }
    }

    let labels: Vec<FaceIdx> = all_face_labels.iter().copied().collect();
    let mut candidate_pairs: BTreeSet<(FaceIdx, FaceIdx)> = BTreeSet::new();
    for i in 0..labels.len() {
        for j in (i + 1)..labels.len() {
            let fa = labels[i];
            let fb = labels[j];
            let empty: BTreeSet<[u64; 3]> = BTreeSet::new();
            let va = face_vertices.get(&fa).unwrap_or(&empty);
            let vb = face_vertices.get(&fb).unwrap_or(&empty);
            if va.intersection(vb).count() >= 2 {
                candidate_pairs.insert((fa, fb));
            }
        }
    }

    let mut boundary_cache: BTreeMap<FaceIdx, DirEdgeMap> = BTreeMap::new();
    for face_idx in &all_face_labels {
        boundary_cache.insert(
            *face_idx,
            face_boundary_directed_edges(rendermesh, face_map, *face_idx),
        );
    }

    for &(face_a, face_b) in &candidate_pairs {
        let empty_b: DirEdgeMap = DirEdgeMap::new();
        let bnd_a = boundary_cache.get(&face_a).unwrap_or(&empty_b);
        let bnd_b = boundary_cache.get(&face_b).unwrap_or(&empty_b);
        let empty_v: BTreeSet<[u64; 3]> = BTreeSet::new();
        let va = face_vertices.get(&face_a).unwrap_or(&empty_v);
        let vb = face_vertices.get(&face_b).unwrap_or(&empty_v);

        // A directed boundary edge of face A is "on the candidate
        // shared boundary with face B" if BOTH its endpoints appear in
        // face B's vertex set. T-junctions emerge here as asymmetry:
        // face A's single (P, Q) edge is anchored on both ends in
        // face B (B has both endpoints), so it enters bnd_a_in_pair.
        // Face B's two sub-edges (P, M) and (M, Q) reference midpoint
        // M, which is NOT in face A's vertex set, so neither sub-edge
        // enters bnd_b_in_pair. Diff yields un_a = 1 (the unmatched
        // (P, Q)), un_b = 0 — non-bijective.
        let mut bnd_a_in_pair: DirEdgeMap = DirEdgeMap::new();
        for (k, &(p, q)) in bnd_a {
            if vb.contains(&pos_key(p)) && vb.contains(&pos_key(q)) {
                bnd_a_in_pair.insert(*k, (p, q));
            }
        }
        let mut bnd_b_in_pair: DirEdgeMap = DirEdgeMap::new();
        for (k, &(p, q)) in bnd_b {
            if va.contains(&pos_key(p)) && va.contains(&pos_key(q)) {
                bnd_b_in_pair.insert(*k, (p, q));
            }
        }

        if bnd_a_in_pair.is_empty() && bnd_b_in_pair.is_empty() {
            // Faces share vertices but no directed boundary edge has
            // both endpoints in the other face's vertex set. Either a
            // single shared corner, or a hard T-junction split where
            // BOTH sides have midpoints the other doesn't. Treat the
            // latter as bijectivity-violating only if at least one
            // face has SOME boundary edges anchored — otherwise it's
            // ambiguous, skip.
            continue;
        }

        let (un_a, un_b) = diff_boundaries(&bnd_a_in_pair, &bnd_b_in_pair);

        report.total_pairs_examined += 1;
        if un_a.is_empty() && un_b.is_empty() {
            report.bijective_pairs += 1;
        } else {
            const SAMPLE: usize = 4;
            report.non_bijective_pairs.push(NonBijectivePair {
                face_a,
                face_b,
                edge: None,
                unmatched_a_count: un_a.len(),
                unmatched_b_count: un_b.len(),
                sample_unmatched_a: un_a.into_iter().take(SAMPLE).collect(),
                sample_unmatched_b: un_b.into_iter().take(SAMPLE).collect(),
            });
        }
    }

    report
}

/// Assertion wrapper around `check_face_pair_bijective`. Panics with a
/// structured message if any face pair is non-bijective.
#[allow(dead_code)] // used in tests only
pub fn assert_face_pair_bijective(
    rendermesh: &RenderMesh,
    face_map: &BTreeMap<u64, FaceIdx>,
    arena: &TopoArena,
) {
    let report = check_face_pair_bijective(rendermesh, face_map, arena);
    if !report.is_bijective() {
        let detail = report
            .non_bijective_pairs
            .iter()
            .take(8)
            .map(|p| {
                format!(
                    "  pair faces=({:?},{:?}) edge={:?}: \
                     unmatched_in_A={} unmatched_in_B={}\n    \
                     sample_A={:?}\n    sample_B={:?}",
                    p.face_a,
                    p.face_b,
                    p.edge,
                    p.unmatched_a_count,
                    p.unmatched_b_count,
                    p.sample_unmatched_a,
                    p.sample_unmatched_b,
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "Bijective tessellation oracle failed (Yang 2025 §4.1.1):\n  \
             {} of {} face pairs non-bijective\nFirst 8 pairs:\n{}",
            report.non_bijective_pairs.len(),
            report.total_pairs_examined,
            detail
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FaceRange, KernelId};

    #[test]
    fn empty_mesh_produces_empty_map() {
        let mesh = RenderMesh {
            vertices: vec![],
            normals: vec![],
            indices: vec![],
            face_ranges: vec![],
        };
        let face_map = BTreeMap::new();
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 0);
        assert!(bmap.is_complete());
        assert!(bmap.referenced_faces().is_empty());
    }

    #[test]
    fn single_triangle_maps_to_face() {
        // One triangle (3 indices), one face
        let mesh = RenderMesh {
            vertices: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: KernelId(42),
                start_index: 0,
                end_index: 3,
            }],
        };
        let mut face_map = BTreeMap::new();
        face_map.insert(42, FaceIdx(7));

        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 1);
        assert!(bmap.is_complete());
        assert_eq!(bmap.tri_face_ids[0], FaceIdx(7));
        assert_eq!(bmap.referenced_faces(), vec![FaceIdx(7)]);
        assert_eq!(bmap.tri_count_for_face(FaceIdx(7)), 1);
    }

    #[test]
    fn multiple_faces_map_correctly() {
        // 4 triangles across 2 faces: face A gets tri 0,1; face B gets tri 2,3
        let mesh = RenderMesh {
            vertices: vec![0.0; 5 * 3], // 5 vertices (enough for 4 triangles sharing verts)
            normals: vec![0.0; 5 * 3],
            indices: vec![0, 1, 2, 1, 2, 3, 0, 3, 4, 3, 4, 2], // 4 triangles = 12 indices
            face_ranges: vec![
                FaceRange {
                    face_id: KernelId(10),
                    start_index: 0,
                    end_index: 6, // tri 0, 1
                },
                FaceRange {
                    face_id: KernelId(20),
                    start_index: 6,
                    end_index: 12, // tri 2, 3
                },
            ],
        };
        let mut face_map = BTreeMap::new();
        face_map.insert(10, FaceIdx(0));
        face_map.insert(20, FaceIdx(1));

        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 4);
        assert!(bmap.is_complete());
        assert_eq!(bmap.tri_face_ids[0], FaceIdx(0));
        assert_eq!(bmap.tri_face_ids[1], FaceIdx(0));
        assert_eq!(bmap.tri_face_ids[2], FaceIdx(1));
        assert_eq!(bmap.tri_face_ids[3], FaceIdx(1));
        assert_eq!(bmap.tri_count_for_face(FaceIdx(0)), 2);
        assert_eq!(bmap.tri_count_for_face(FaceIdx(1)), 2);

        let faces = bmap.referenced_faces();
        assert_eq!(faces.len(), 2);
    }

    // ── Integration tests with real kernel primitives ────────────

    use crate::traits::Kernel;
    use crate::waffle_kernel::WaffleKernel;
    use std::collections::HashMap;

    const XY_ORIGIN: [f64; 3] = [0.0, 0.0, 0.0];
    const XY_NORMAL: [f64; 3] = [0.0, 0.0, 1.0];
    const XY_X_AXIS: [f64; 3] = [1.0, 0.0, 0.0];
    const Z_DIR: [f64; 3] = [0.0, 0.0, 1.0];

    /// Build a face_map from a RenderMesh's face_ranges by assigning each
    /// unique KernelId a sequential FaceIdx. This mirrors what the kernel
    /// stores internally, letting us test BijectiveMap without accessing
    /// private kernel fields.
    fn face_map_from_mesh(mesh: &crate::types::RenderMesh) -> BTreeMap<u64, FaceIdx> {
        let mut map = BTreeMap::new();
        let mut next_idx = 0usize;
        for range in &mesh.face_ranges {
            map.entry(range.face_id.0).or_insert_with(|| {
                let idx = FaceIdx(next_idx);
                next_idx += 1;
                idx
            });
        }
        map
    }

    fn make_box_kernel(
        w: f64,
        h: f64,
        depth: f64,
    ) -> (WaffleKernel, crate::types::KernelSolidHandle) {
        use crate::types::ClosedProfile;
        let mut k = WaffleKernel::new();
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (w, 0.0));
        positions.insert(3, (w, h));
        positions.insert(4, (0.0, h));
        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = k
            .make_faces_from_profiles(&[profile], XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles for box");
        let solid = k
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face for box");
        (k, solid)
    }

    fn make_cylinder_kernel(r: f64, depth: f64) -> (WaffleKernel, crate::types::KernelSolidHandle) {
        use crate::types::{CircleProfile, ClosedProfile};
        let mut k = WaffleKernel::new();
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        let profile = ClosedProfile {
            entity_ids: vec![1],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(CircleProfile {
                center_u: 0.0,
                center_v: 0.0,
                radius: r,
            }),
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = k
            .make_faces_from_profiles(&[profile], XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles for cylinder");
        let solid = k
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face for cylinder");
        (k, solid)
    }

    #[test]
    fn box_bijective_map_is_complete() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert!(
            bmap.is_complete(),
            "Box bijective map must be complete (no sentinel values)"
        );
    }

    #[test]
    fn box_bijective_map_references_six_faces() {
        let (mut k, solid) = make_box_kernel(2.0, 3.0, 4.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let faces = bmap.referenced_faces();
        assert_eq!(
            faces.len(),
            6,
            "Box should reference exactly 6 faces, got {}",
            faces.len()
        );
    }

    #[test]
    fn box_bijective_map_tri_count_matches_mesh() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let expected_tris = mesh.indices.len() / 3;
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert_eq!(
            bmap.tri_count(),
            expected_tris,
            "BijectiveMap tri_count ({}) must match mesh triangle count ({})",
            bmap.tri_count(),
            expected_tris
        );
    }

    #[test]
    fn box_every_face_has_at_least_two_triangles() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        for face in bmap.referenced_faces() {
            let count = bmap.tri_count_for_face(face);
            assert!(
                count >= 2,
                "Box face {:?} should have >= 2 triangles (quad -> 2 tris), got {}",
                face,
                count
            );
        }
    }

    #[test]
    fn box_triangle_sum_equals_total() {
        let (mut k, solid) = make_box_kernel(5.0, 3.0, 2.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate box");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let sum: usize = bmap
            .referenced_faces()
            .iter()
            .map(|&f| bmap.tri_count_for_face(f))
            .sum();
        assert_eq!(
            sum,
            bmap.tri_count(),
            "Sum of per-face triangle counts ({}) must equal total tri_count ({})",
            sum,
            bmap.tri_count()
        );
    }

    #[test]
    fn cylinder_bijective_map_is_complete() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert!(
            bmap.is_complete(),
            "Cylinder bijective map must be complete (no sentinel values)"
        );
    }

    #[test]
    fn cylinder_bijective_map_references_three_faces() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let faces = bmap.referenced_faces();
        assert_eq!(
            faces.len(),
            3,
            "Cylinder should reference 3 faces (top cap, bottom cap, side), got {}",
            faces.len()
        );
    }

    #[test]
    fn cylinder_bijective_map_tri_count_matches_mesh() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let expected_tris = mesh.indices.len() / 3;
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert_eq!(
            bmap.tri_count(),
            expected_tris,
            "Cylinder BijectiveMap tri_count ({}) must match mesh triangle count ({})",
            bmap.tri_count(),
            expected_tris
        );
    }

    #[test]
    fn cylinder_triangle_sum_equals_total() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let sum: usize = bmap
            .referenced_faces()
            .iter()
            .map(|&f| bmap.tri_count_for_face(f))
            .sum();
        assert_eq!(
            sum,
            bmap.tri_count(),
            "Sum of per-face triangle counts ({}) must equal total tri_count ({})",
            sum,
            bmap.tri_count()
        );
    }

    #[test]
    fn sphere_bijective_map_is_complete() {
        let mut k = WaffleKernel::new();
        let solid = k
            .make_sphere([0.0, 0.0, 0.0], 1.0)
            .expect("make_sphere should succeed");
        let mesh = k.tessellate(&solid, 0.01).expect("tessellate sphere");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert!(
            bmap.is_complete(),
            "Sphere bijective map must be complete (no sentinel values)"
        );
    }

    #[test]
    fn sphere_bijective_map_tri_count_matches_mesh() {
        let mut k = WaffleKernel::new();
        let solid = k
            .make_sphere([0.0, 0.0, 0.0], 1.0)
            .expect("make_sphere should succeed");
        let mesh = k.tessellate(&solid, 0.01).expect("tessellate sphere");
        let expected_tris = mesh.indices.len() / 3;
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        assert_eq!(
            bmap.tri_count(),
            expected_tris,
            "Sphere BijectiveMap tri_count ({}) must match mesh triangle count ({})",
            bmap.tri_count(),
            expected_tris
        );
    }

    #[test]
    fn sphere_triangle_sum_equals_total() {
        let mut k = WaffleKernel::new();
        let solid = k
            .make_sphere([0.0, 0.0, 0.0], 1.0)
            .expect("make_sphere should succeed");
        let mesh = k.tessellate(&solid, 0.01).expect("tessellate sphere");
        let face_map = face_map_from_mesh(&mesh);
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);

        let sum: usize = bmap
            .referenced_faces()
            .iter()
            .map(|&f| bmap.tri_count_for_face(f))
            .sum();
        assert_eq!(
            sum,
            bmap.tri_count(),
            "Sum of per-face triangle counts ({}) must equal total tri_count ({})",
            sum,
            bmap.tri_count()
        );
    }

    #[test]
    fn missing_face_in_map_uses_sentinel() {
        let mesh = RenderMesh {
            vertices: vec![0.0; 3 * 3],
            normals: vec![0.0; 3 * 3],
            indices: vec![0, 1, 2],
            face_ranges: vec![FaceRange {
                face_id: KernelId(99),
                start_index: 0,
                end_index: 3,
            }],
        };
        // face_map does NOT contain KernelId(99)
        let face_map = BTreeMap::new();
        let bmap = BijectiveMap::from_render_mesh(&mesh, &face_map);
        assert_eq!(bmap.tri_count(), 1);
        assert!(!bmap.is_complete()); // sentinel present
    }

    // ── Bijective oracle face-pair shared-edge tests (PR1) ─────────────
    //
    // Yang 2025 §4.1.1 contract: along every B-Rep edge shared by two
    // faces, the tessellation must emit byte-identical f64 vertex
    // positions on both sides. The `assert_face_pair_bijective` oracle
    // (above) measures conformance to this contract.
    //
    // PR1 lands these three tests as instrumentation. PR2 will lift the
    // bounded-path gate at `tessellation/mod.rs:217-235` so that
    // cylinder/curved/polygon-soup inputs route through the bounded
    // path; PR3 will then remove `weld_mesh_vertices` (audit D-10).

    /// Yang 2025 §4.1.1 bijectivity check on a 6-face cube.
    ///
    /// A cube's only edges are linear, so the bounded path applies
    /// (`tessellation/mod.rs:217-235`: cylinder_params/revolve_params/
    /// sphere_params/cone_params/torus_params all None, no arc edges,
    /// not polygon-soup). The bounded path discretizes each edge once
    /// in a shared `EdgeDiscretization.positions` pool, so both faces
    /// adjacent to a B-Rep edge emit byte-identical positions along it.
    /// Oracle should report zero non-bijective pairs.
    #[test]
    fn test_cube_is_bijective() {
        let (mut k, solid) = make_box_kernel(1.0, 1.0, 1.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cube");
        let ws = k.get_solid(&solid).expect("get_solid for cube");
        let report = check_face_pair_bijective(&mesh, &ws.face_map, &ws.arena);
        assert!(
            report.is_bijective(),
            "Cube should be bijective per Yang §4.1.1; \
             {} of {} face pairs non-bijective",
            report.non_bijective_pairs.len(),
            report.total_pairs_examined
        );
        // Sanity: a closed-shell cube has 12 manifold edges, each shared
        // by exactly two faces, so the oracle should examine 12 pairs.
        assert_eq!(
            report.total_pairs_examined, 12,
            "Cube should expose 12 face-pair-sharing-edge instances"
        );
        assert_eq!(report.bijective_pairs, 12);
    }

    /// Sensitivity proof for the bijective oracle: a hand-crafted
    /// rendermesh with a deliberate T-junction crack between two
    /// adjacent faces. Face A is one rectangle whose right edge is a
    /// single segment (1,0)→(1,1). Face B sits flush against A on the
    /// right and encodes its left boundary with an EXTRA midpoint at
    /// (1, 0.5, 0), splitting the shared edge into two sub-segments.
    /// Per Yang §4.1.1, the bijective contract requires both sides to
    /// emit the same directed mesh edges along the shared B-Rep edge;
    /// the T-junction violates it.
    ///
    /// This test exists to prove the oracle is sensitive enough to
    /// detect T-junctions even on byte-identical positions — the
    /// failure mode here is SEGMENTATION DIFFERENCE, not rounding.
    /// Welding cannot fix this: quantization would not introduce the
    /// missing midpoint into face A's tessellation.
    ///
    /// Polygon-soup mode is exercised because we pass an empty arena.
    #[test]
    fn oracle_detects_t_junction_sensitivity() {
        let vertices: Vec<f32> = vec![
            // face A: 0,1,2,3 (rectangle 0..1 × 0..1)
            0.0, 0.0, 0.0, // 0
            1.0, 0.0, 0.0, // 1
            1.0, 1.0, 0.0, // 2
            0.0, 1.0, 0.0, // 3
            // face B: 4..8 (rectangle 1..2 × 0..1 with extra midpoint)
            1.0, 0.0, 0.0, // 4  (byte-identical to A's vertex 1)
            1.0, 0.5, 0.0, // 5  (T-junction midpoint, NOT on face A)
            1.0, 1.0, 0.0, // 6  (byte-identical to A's vertex 2)
            2.0, 0.0, 0.0, // 7
            2.0, 1.0, 0.0, // 8
        ];
        let normals: Vec<f32> = (0..9).flat_map(|_| [0.0f32, 0.0, 1.0]).collect();
        let indices: Vec<u32> = vec![
            // face A: 2 tris CCW outward (+z normal): 0,1,2 / 0,2,3
            0, 1, 2, 0, 2, 3,
            // face B: 3 tris using the midpoint at vertex 5
            //   4-7-5, 5-7-8, 5-8-6 — winding CCW on +z
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

        let report = check_face_pair_bijective(&mesh, &face_map, &arena);
        assert!(
            !report.is_bijective(),
            "Oracle must detect T-junction crack between coplanar faces, \
             but reported {} of {} pairs bijective",
            report.bijective_pairs,
            report.total_pairs_examined
        );
        assert_eq!(report.total_pairs_examined, 1);
        assert_eq!(report.non_bijective_pairs.len(), 1);
        let p = &report.non_bijective_pairs[0];
        assert!(
            p.unmatched_a_count > 0 || p.unmatched_b_count > 0,
            "T-junction must surface as unmatched directed edges; got \
             unmatched_a={} unmatched_b={}",
            p.unmatched_a_count,
            p.unmatched_b_count
        );
    }

    /// Yang 2025 §4.1.1 bijectivity check for an analytic cylinder
    /// (top + bottom planar caps + cylindrical side; circular edges).
    ///
    /// `#[ignore]`d as a placeholder for PR3. The cylinder primitive
    /// today is gated AWAY from the bounded path at
    /// `tessellation/mod.rs:217-235` (`cylinder_params.is_some()`) and
    /// goes through the fan path, which then runs
    /// `weld_shared_edge_vertices` (`tessellation/mod.rs:759`,
    /// `tessellation/mod.rs:851-922`) to converge per-face vertices to
    /// shared indices via TAU_MODEL=1e-7 quantization. After welding,
    /// the rendermesh DOES satisfy the byte-identical-position contract
    /// for the simple cylinder case (cosines are deterministic across
    /// faces), so this oracle as written cannot distinguish true
    /// pre-welding bijectivity from welding-induced convergence on
    /// this input. PR3 removes `weld_shared_edge_vertices`; if the
    /// underlying tessellation is not actually bijective, this test
    /// will then fail and stop being `#[ignore]`d.
    ///
    /// Audit finding D-10 (Cluster I, blocked by tessellation) in
    /// `docs/audits/cherchi_port_audit.md`.
    #[test]
    #[ignore]
    fn test_cylinder_is_bijective() {
        let (mut k, solid) = make_cylinder_kernel(5.0, 10.0);
        let mesh = k.tessellate(&solid, 0.1).expect("tessellate cylinder");
        let ws = k.get_solid(&solid).expect("get_solid for cylinder");
        assert_face_pair_bijective(&mesh, &ws.face_map, &ws.arena);
    }

    /// Yang 2025 §4.1.1 bijectivity check for boolean output (no
    /// parametric provenance — "polygon soup").
    ///
    /// `#[ignore]`d as a placeholder for PR3. The boolean output here
    /// is classified `is_polygon_soup=true`
    /// (`waffle_kernel.rs:1297,1317`), which gates it AWAY from the
    /// bounded path at `tessellation/mod.rs:217-235`. The fan path
    /// then runs `weld_shared_edge_vertices`, converging per-face
    /// vertices to shared indices. Post-welding, this simple
    /// box-minus-box output IS bijective — the oracle correctly
    /// returns "bijective." PR3 removes the welding workaround; if
    /// the underlying tessellation is not actually bijective, this
    /// test will then fail and stop being `#[ignore]`d.
    ///
    /// Audit finding D-10 (Cluster I, blocked by tessellation).
    #[test]
    #[ignore]
    fn test_boolean_box_minus_box_is_bijective() {
        // Two overlapping boxes; subtract A from B to produce a boolean
        // output that the kernel marks polygon-soup.
        // Box A: 10×10×10 at origin. Box B: 10×10×10 shifted by +5 in x.
        // A − B leaves a 5×10×10 slab on the −x half plus matching
        // boundary faces.
        let mut k = WaffleKernel::new();
        let (_, solid_a) = make_box_kernel_in(&mut k, 10.0, 10.0, 10.0, 0.0, 0.0);
        let (_, solid_b) = make_box_kernel_in(&mut k, 10.0, 10.0, 10.0, 5.0, 0.0);
        let result = k
            .boolean_subtract(&solid_a, &solid_b)
            .expect("boolean_subtract A − B");
        let mesh = k
            .tessellate(&result, 0.1)
            .expect("tessellate boolean output");
        let ws = k.get_solid(&result).expect("get_solid for boolean result");
        assert_face_pair_bijective(&mesh, &ws.face_map, &ws.arena);
    }

    /// Helper: append a box at origin (ox, oy, 0) of size w × h × depth
    /// into an existing kernel. Used by the boolean test to build A and
    /// B in the same kernel without overlapping handles.
    fn make_box_kernel_in(
        k: &mut WaffleKernel,
        w: f64,
        h: f64,
        depth: f64,
        ox: f64,
        oy: f64,
    ) -> ((), crate::types::KernelSolidHandle) {
        use crate::types::ClosedProfile;
        let mut positions = HashMap::new();
        positions.insert(1, (ox, oy));
        positions.insert(2, (ox + w, oy));
        positions.insert(3, (ox + w, oy + h));
        positions.insert(4, (ox, oy + h));
        let profile = ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let face_ids = k
            .make_faces_from_profiles(&[profile], XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles for box");
        let solid = k
            .extrude_face(face_ids[0], Z_DIR, depth)
            .expect("extrude_face for box");
        ((), solid)
    }

    /// Yang 2025 §4.1.1 bijectivity check on a partial-revolve solid built
    /// from a multi-segment "gear-like" profile.
    ///
    /// Fixture: an 8-vertex stepped profile in the XY sketch plane (all
    /// vertices on the +x side of an offset Y-axis at x=-2.0), revolved
    /// 90° around that axis. The profile alternates between two radial
    /// distances to model the multi-segment outline that R0005-class gear
    /// inputs exhibit. The +x-only constraint satisfies revolve_polygon's
    /// "no straddling axis" check.
    ///
    /// Topology after revolve_polygon (8-vertex profile, partial sweep):
    ///   - 1 start cap face (planar, at θ=0)
    ///   - 1 end cap face (planar, at θ=π/2)
    ///   - 8 lateral faces (one per profile edge — cylindrical when the
    ///     edge is parallel to the axis, planar when perpendicular,
    ///     conical otherwise; here the stepped profile produces a mix
    ///     of cylindrical and planar laterals)
    /// All 16 cap-to-lateral B-Rep edges (8 each on start cap + end cap)
    /// are linear (cap edges between consecutive profile vertices at θ=0
    /// or θ=angle_rad). Each is shared between one cap face and one
    /// lateral face — these are the pairs the oracle examines.
    ///
    /// Expected dispatch path (`tessellation/mod.rs:217-235`): because
    /// `revolve_params.is_some()`, the solid is gated AWAY from the
    /// bounded path (which would share an `EdgeDiscretization.positions`
    /// pool across faces). It enters the per-face dispatch loop, where
    /// caps go through `tessellate_polygon_face` and laterals through
    /// `tessellate_revolve_lateral`. The two emitters allocate vertices
    /// independently — `tessellate_polygon_face` walks `arena.faces[i]
    /// .outer_loop` and pushes f64-rounded-to-f32 cap loop vertices,
    /// while `tessellate_revolve_lateral` rotates `start_v0/start_v1`
    /// via Rodrigues and pushes the resulting f64-rounded-to-f32
    /// positions for ring 0 and ring n on each lateral. The two
    /// trajectories diverge at the f32 rounding step.
    ///
    /// Currently RED on main: `needs_fan_welding` stays false for partial
    /// revolves (`tessellation/mod.rs:215, 217-235` — revolve_params is
    /// Some so the `has_arcs` branch is never taken; spherical-fallback
    /// branch at line 264-270 also skipped), so
    /// `weld_shared_edge_vertices` does NOT run on this fixture. The
    /// boundary-only welds that DO run (`weld_boundary_vertices*` at
    /// line 624, 687, 721; `close_near_boundary_chains` at 618, 664)
    /// quantize at TAU_MODEL=1e-7 = 100nm, which per PR1 measurement is
    /// insufficient to converge cap↔lateral boundaries for R0005-class
    /// inputs.
    ///
    /// GREEN after PR2 fix: a pre-computed profile/end-ring pool — built
    /// once in `tessellate_solid_ext` from `arena.vertices` of cap loop
    /// vertices and re-used by both `tessellate_polygon_face` and
    /// `tessellate_revolve_lateral` for boundary positions — guarantees
    /// byte-identical f64 → f32 rounding because the source position
    /// is identical.
    ///
    /// PR2 of multi-PR tessellation work. References:
    ///   - Yang 2025 §4.1.1 (bijective tessellation contract)
    ///   - audit D-10 in `docs/audits/cherchi_port_audit.md` (Cluster I,
    ///     blocked-by-tessellation)
    ///   - PR1 oracle landing commit 5f5423c
    ///   - PR1 corpus baseline a445c18
    #[test]
    fn test_revolve_partial_gear_is_bijective() {
        use crate::types::ClosedProfile;

        let mut k = WaffleKernel::new();

        // Profile in XY plane (sketch normal = +Z, sketch x-axis = +X).
        // Revolve axis: along world +Y at x=-2.0, z=0 — lies IN the
        // sketch plane, fully on the −x side of every profile vertex.
        // Stepped 8-vertex profile, all at x ≥ 1.07 (so saw_neg never
        // triggers in revolve_polygon's straddle-check). Two distinct
        // radii (1.07 and 2.83 from sketch origin → 3.07 and 4.83
        // from axis at x=-2.0) producing alternating outer/inner rim
        // segments, modeling a multi-segment gear-like outline.
        //
        // Coordinate values are deliberately NON-round (avoiding 1.0,
        // 2.0, 3.0) so f64 → f32 rounding produces lossy positions on
        // the cap loop, while Rodrigues rotation in
        // tessellate_revolve_lateral produces a SECOND lossy f32
        // trajectory along the start ring (theta=0) and end ring
        // (theta=angle_rad). The cap's lossy f64→f32 trajectory and
        // the lateral's lossy Rodrigues→f32 trajectory diverge —
        // surfacing the position-side of the bijectivity violation
        // (Yang §4.1.1) that PR2 must close.
        //
        // Vertex layout (counter-clockwise on +Z plane), closed loop:
        //   1: (1.07, 0.13)  ── inner-left, bottom
        //   2: (2.83, 0.13)  ── outer-left, bottom (outer rim)
        //   3: (2.83, 1.21)  ── outer-left, top
        //   4: (1.93, 1.21)  ── valley step inward
        //   5: (1.93, 2.07)  ── valley climb
        //   6: (2.83, 2.07)  ── outer-right step out (peak)
        //   7: (2.83, 2.91)  ── outer-right, top
        //   8: (1.07, 2.91)  ── inner-right (closing back to v1)
        // 8 edges → 8 lateral faces; cap polygon has 8 vertices.
        let mut positions: HashMap<u32, (f64, f64)> = HashMap::new();
        positions.insert(1, (1.07, 0.13));
        positions.insert(2, (2.83, 0.13));
        positions.insert(3, (2.83, 1.21));
        positions.insert(4, (1.93, 1.21));
        positions.insert(5, (1.93, 2.07));
        positions.insert(6, (2.83, 2.07));
        positions.insert(7, (2.83, 2.91));
        positions.insert(8, (1.07, 2.91));

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4, 5, 6, 7, 8],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };

        let face_ids = k
            .make_faces_from_profiles(&[profile], XY_ORIGIN, XY_NORMAL, XY_X_AXIS, &positions)
            .expect("make_faces_from_profiles for partial-gear profile");

        // Revolve axis: origin (-2.0, 0, 0), direction +Y, sweep
        // ≈87.4° (non-round so cos/sin land at lossy f64 values that
        // don't round-trip exactly to f32). Axis lies in the sketch
        // plane (XY), so axis_dir × plane_normal = (0,1,0) × (0,0,1)
        // = (1,0,0) — the splitting reference is +X, and every
        // profile vertex has positive +X component, so the straddle
        // check (revolve_polygon line 1465-1482) passes.
        //
        // The non-round angle is the second leg of the f32-divergence
        // trap: even if cap loop f64 vertices happened to round-trip
        // exactly, Rodrigues at non-round theta values produces lossy
        // start-ring and end-ring f64 positions which then take a
        // second f32-rounding hit. The cap's f32 cap-loop positions
        // and the lateral's f32 ring-end positions therefore diverge.
        let axis_origin = [-2.0, 0.0, 0.0];
        let axis_direction = [0.0, 1.0, 0.0];
        let angle_deg: f64 = 87.41;

        let solid = k
            .revolve_face(face_ids[0], axis_origin, axis_direction, angle_deg)
            .expect("revolve_face for partial-gear profile");

        // Tessellate via the kernel's primitive-dispatch path.
        // tess_tol is irrelevant for revolve dispatch — circle_segments
        // controls the lateral ring count via thread-local override
        // default of 64; the cap fan triangulation walks the loop vertex
        // list directly. Tolerance only affects analytic paths
        // (sphere/cone/torus) and bounded fallback.
        let mesh = k
            .tessellate(&solid, 0.05)
            .expect("tessellate partial-gear revolve");

        let ws = k
            .get_solid(&solid)
            .expect("get_solid for partial-gear revolve");

        let report = check_face_pair_bijective(&mesh, &ws.face_map, &ws.arena);

        // Sanity: the oracle must be examining real face pairs. With 8
        // profile edges + 2 caps, B-Rep mode walks `arena.edges`,
        // identifies adjacent face pairs sharing a manifold edge, and
        // examines each pair once. Every cap edge (8 on start cap,
        // 8 on end cap) is shared with exactly one lateral face — that
        // already gives 16 cap-lateral pair instances. Adjacent
        // laterals also share lateral-lateral edges. We require strictly
        // > 0 to confirm the fixture exercises the oracle.
        assert!(
            report.total_pairs_examined > 0,
            "Fixture must produce face pairs sharing B-Rep edges; \
             the oracle examined zero pairs. \
             Either revolve_polygon failed to allocate per-edge faces \
             or face_map/arena are inconsistent."
        );

        // Diagnostic dump BEFORE the bijectivity assertion so the red
        // signal magnitude is captured in test output even when the
        // assertion fails. The implementer uses this count as a PR2
        // regression target — the fix is GREEN when non_bijective_pairs
        // drops to 0.
        eprintln!(
            "[partial-gear-bijective] total_pairs_examined={} bijective_pairs={} \
             non_bijective_pairs={}",
            report.total_pairs_examined,
            report.bijective_pairs,
            report.non_bijective_pairs.len(),
        );
        for (i, p) in report.non_bijective_pairs.iter().take(8).enumerate() {
            eprintln!(
                "[partial-gear-bijective] pair[{}] face_a={:?} face_b={:?} edge={:?} \
                 unmatched_a={} unmatched_b={}",
                i, p.face_a, p.face_b, p.edge, p.unmatched_a_count, p.unmatched_b_count,
            );
            for (j, (s, e)) in p.sample_unmatched_a.iter().enumerate() {
                eprintln!(
                    "[partial-gear-bijective]   sample_a[{}] ({:.10}, {:.10}, {:.10}) → \
                     ({:.10}, {:.10}, {:.10})",
                    j, s[0], s[1], s[2], e[0], e[1], e[2],
                );
            }
            for (j, (s, e)) in p.sample_unmatched_b.iter().enumerate() {
                eprintln!(
                    "[partial-gear-bijective]   sample_b[{}] ({:.10}, {:.10}, {:.10}) → \
                     ({:.10}, {:.10}, {:.10})",
                    j, s[0], s[1], s[2], e[0], e[1], e[2],
                );
            }
        }

        // The Yang 2025 §4.1.1 bijective contract: every shared B-Rep
        // edge must emit byte-identical reciprocal directed mesh edges
        // on both adjacent faces. RED on main: the cap-to-lateral and
        // side-cap-to-lateral boundaries are emitted independently by
        // tessellate_polygon_face (caps via fan triangulation walking
        // arena.faces[i].outer_loop) and tessellate_revolve_lateral
        // (laterals via Rodrigues rotation of start_v0/start_v1 at
        // theta=0 and theta=angle_rad). The two trajectories diverge
        // at the f64 → f32 rounding step; the 100nm
        // weld_shared_edge_vertices quantization is gated off for
        // partial revolves (revolve_params.is_some() routes around the
        // welding branch), and the boundary-only welds that DO run are
        // insufficient at TAU_MODEL=1e-7.
        //
        // GREEN after PR2 fix: a pre-computed profile/end-ring pool
        // shares boundary vertex IDs across cap and lateral faces.
        assert!(
            report.non_bijective_pairs.is_empty(),
            "Yang §4.1.1 bijectivity violated on partial-revolve \
             primitive-dispatch tessellation: {} of {} face pairs \
             non-bijective (cap↔lateral boundary divergence). See \
             stderr [partial-gear-bijective] dump above for per-pair \
             unmatched directed-edge samples.",
            report.non_bijective_pairs.len(),
            report.total_pairs_examined,
        );
    }

    /// Reserved slot for the bounded-path bijectivity red test.
    ///
    /// PR3 originally scoped this against a `discretize_edges` dedup
    /// hypothesis (empirically falsified — the oracle keys on f32→f64-cast
    /// position bit patterns, not pool indices). PR4 anchored a corpus-
    /// derived RED test on R0033 (`pr4_r0033_t_junction_diagnosis` in
    /// `crates/test-harness/tests/`). PR5 attempted two tessellation-side
    /// fixes (cap-polygon RevolvePool extension; planar-bounded Newell-
    /// reverse desync via PR2's post-fix-normal-flip pattern) and
    /// empirically falsified BOTH — neither code path is reached for R0033,
    /// and the planar-bounded check `dot(arena_natural_newell, stored_normal)
    /// = 1.0` exactly for all 6 faces, so the Newell-reverse code never
    /// fires.
    ///
    /// PR5's empirical investigation (`specs/tessellation_bounded_residuals.md`
    /// §9) traced the residual mechanism to a half-edge twin convention
    /// violation upstream in `boolean/topology_extract.rs::flood_fill_patches`
    /// (likely via `yang_pipeline_result_for_disjoint`'s degenerate-input
    /// path for AABB-disjoint Subtract). The tessellator faithfully
    /// reproduces a malformed arena and cannot fix it — no tessellation-
    /// level change can make non-twin half-edges produce reciprocal mesh
    /// edges.
    ///
    /// PR6 anchor: investigate `flood_fill_patches` twin-pairing for the
    /// AABB-disjoint short-circuit path. Likely fix sites:
    /// `boolean/topology_extract.rs::flood_fill_patches` (Steps 5/5a/6 —
    /// patch boundary classification or twin assignment) or
    /// `boolean/yang_integration.rs::result_topology_to_waffle_solid`
    /// (post-flood-fill arena-build that may be losing twin pairing).
    ///
    /// References:
    /// - `specs/tessellation_bounded_residuals.md` §9 (PR5 closure)
    /// - `crates/test-harness/tests/pr4_r0033_t_junction_diagnosis.rs`
    /// - `docs/audits/cherchi_port_audit.md` D-10
    /// - Yang 2025 §4.1.1 (bijective tessellation contract)
    /// - PR1 oracle commit `5f5423c`
    /// - PR2 fix commit `f01dd68`
    /// - PR2 corpus delta commit `c4f0fcb`
    /// - PR4 RED diagnostic commit `7ee4805`
    #[test]
    #[ignore = "PR6 anchor — bug is upstream in flood_fill_patches; see specs/tessellation_bounded_residuals.md §9"]
    fn test_bounded_path_brep_t_junction_is_bijective() {
        // PR6 implementer fills this in after fixing the upstream
        // `flood_fill_patches` twin-pairing bug. The kernel-internal slot
        // remains reserved; the canonical RED test for R0033 lives in
        // `crates/test-harness/tests/pr4_r0033_t_junction_diagnosis.rs`
        // (full LoadProject path required for R0033 reproduction).
        //
        // Expected shape once PR6 anchors a kernel-internal fixture:
        //   1. Build (or load) a solid that triggers the
        //      `flood_fill_patches` twin-pairing failure. The R0033
        //      reproducer requires LoadProject + Yang AABB-disjoint
        //      short-circuit; a minimal kernel-only fixture would need
        //      to exercise `flood_fill_patches` directly.
        //   2. Tessellate via the bounded path.
        //   3. Run `check_face_pair_bijective`.
        //   4. RED on main: oracle reports ≥1 non-bij pair where
        //      adjacent faces emit the shared B-Rep edge in the SAME
        //      forward 3D direction (twin convention violation).
        //   5. GREEN after PR6 fix in `boolean/topology_extract.rs`:
        //      oracle reports 0 non-bij pairs.
        unimplemented!(
            "PR6 anchor — bug is upstream in flood_fill_patches twin-pairing. See \
             specs/tessellation_bounded_residuals.md §9 for the PR5 falsification trace. \
             The canonical R0033 reproducer lives in crates/test-harness/tests/\
             pr4_r0033_t_junction_diagnosis.rs (still RED on main)."
        );
    }
}
