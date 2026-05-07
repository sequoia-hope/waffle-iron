//! PR7 — empirical mechanism classifier for R0033-class non-bijective face pairs.
//!
//! Given a tessellated rendermesh + the source arena + face_map + edge_geometry,
//! and a specific (face_a, face_b, edge) triple from the bijective oracle,
//! classify the mechanism into ONE of:
//!
//! - `arena-missing-edge`: arena has no half-edge twin pair connecting these
//!   two faces' loops (the oracle's reported `edge` resolves to a face that is
//!   NOT one of {face_a, face_b}).
//! - `pool-not-shared`: the shared edge exists, but the pool indices that
//!   `collect_loop_boundary` produces for face_a vs face_b at this edge don't
//!   match (modulo direction reversal). This means the discretization layer
//!   gave the two faces different pool entries for the same edge.
//! - `positional-drift`: pool indices match, but the rendermesh f32 byte
//!   pattern differs between the two faces' emissions of the shared boundary
//!   vertices.
//! - `direction-reciprocity`: pool indices match AND f32 byte-identical, but
//!   the rendermesh emits same-forward-direction edges on both faces (a
//!   half-edge twin convention violation).
//!
//! This module exists to anchor PR7's fix selection per the stop condition in
//! the PR7 brief: each classification points at a distinct fix site (or scopes
//! the fix out of PR7 entirely).

use crate::geometry::curve::CurveGeom;
use crate::topology::arena::TopoArena;
use crate::topology::half_edge::{EdgeIdx, FaceIdx, HalfEdgeIdx, LoopIdx};
use crate::types::RenderMesh;
use std::collections::BTreeMap;

use super::discretize_edges;

/// Classification outcomes per the PR7 brief.
#[derive(Debug, Clone)]
pub enum Pr7Classification {
    /// Arena has no shared half-edge twin pair connecting the two faces.
    /// The oracle's reported `edge` does not belong to either face's loop.
    ArenaMissingEdge {
        edge: EdgeIdx,
        edge_he_face: Option<FaceIdx>,
        edge_twin_face: Option<FaceIdx>,
    },
    /// Shared edge exists; `collect_loop_boundary` produces non-matching pool
    /// indices for face_a vs face_b at this edge.
    PoolNotShared {
        edge: EdgeIdx,
        face_a_pool_at_edge: Vec<usize>,
        face_b_pool_at_edge: Vec<usize>,
    },
    /// Pool indices match (modulo direction); rendermesh f32 emissions differ.
    PositionalDrift {
        edge: EdgeIdx,
        face_a_emitted: Vec<[u32; 3]>,
        face_b_emitted: Vec<[u32; 3]>,
    },
    /// Pool indices and f32 emissions match; emitted directed edges go the
    /// same forward direction on both faces (twin convention violation).
    DirectionReciprocity {
        edge: EdgeIdx,
        sample_dir: ([f32; 3], [f32; 3]),
    },
    /// None of the four classes fit. This is the "5th class" the stop
    /// condition warns about — surface as-is so the lead can decide.
    Other { reason: String },
}

/// Classify a non-bijective pair `(face_a, face_b, edge)` against the given
/// rendermesh + arena. The `edge` should be the value produced by the
/// bijective oracle for this pair (`NonBijectivePair::edge`). If `edge` is
/// `None` the function returns `ArenaMissingEdge` with `edge_he_face` and
/// `edge_twin_face` both None.
pub fn classify_pr7_pair(
    rendermesh: &RenderMesh,
    arena: &TopoArena,
    face_map: &BTreeMap<u64, FaceIdx>,
    edge_geometry: &BTreeMap<EdgeIdx, CurveGeom>,
    face_a: FaceIdx,
    face_b: FaceIdx,
    edge: Option<EdgeIdx>,
) -> Pr7Classification {
    // ── Step 1: arena-missing-edge check ─────────────────────────────────
    let edge = match edge {
        Some(e) if e.0 < arena.edges.len() => e,
        _ => {
            return Pr7Classification::ArenaMissingEdge {
                edge: edge.unwrap_or(EdgeIdx(usize::MAX)),
                edge_he_face: None,
                edge_twin_face: None,
            };
        }
    };

    let he_a_idx = arena.edges[edge.0].half_edge;
    if he_a_idx.0 >= arena.half_edges.len() {
        return Pr7Classification::ArenaMissingEdge {
            edge,
            edge_he_face: None,
            edge_twin_face: None,
        };
    }
    let he_a = &arena.half_edges[he_a_idx.0];
    // PR-Y20-MODE-A: NMM (twin=None) — treat as ArenaMissingEdge for
    // classification purposes (no opposing twin to inspect).
    let he_b_idx = match he_a.twin {
        Some(t) => t,
        None => {
            return Pr7Classification::ArenaMissingEdge {
                edge,
                edge_he_face: None,
                edge_twin_face: None,
            };
        }
    };
    if he_b_idx.0 >= arena.half_edges.len() {
        return Pr7Classification::ArenaMissingEdge {
            edge,
            edge_he_face: None,
            edge_twin_face: None,
        };
    }
    let he_b = &arena.half_edges[he_b_idx.0];

    let edge_he_face = if he_a.loop_.0 < arena.loops.len() {
        Some(arena.loops[he_a.loop_.0].face)
    } else {
        None
    };
    let edge_twin_face = if he_b.loop_.0 < arena.loops.len() {
        Some(arena.loops[he_b.loop_.0].face)
    } else {
        None
    };

    let edge_face_set = [edge_he_face, edge_twin_face];
    if !edge_face_set.contains(&Some(face_a)) || !edge_face_set.contains(&Some(face_b)) {
        return Pr7Classification::ArenaMissingEdge {
            edge,
            edge_he_face,
            edge_twin_face,
        };
    }

    // ── Step 2: pool-not-shared check ─────────────────────────────────────
    let disc = discretize_edges(arena, edge_geometry);

    // Find the half-edge on each face's outer loop that lives on `edge`.
    // (Inner loops are not implicated for R0033's planar faces, but if the
    // edge appears in an inner loop instead we'll catch it with the wider
    // loop search.)
    let he_for_face = |face: FaceIdx| -> Option<HalfEdgeIdx> {
        let face_data = &arena.faces[face.0];
        let mut all_loops: Vec<LoopIdx> = vec![face_data.outer_loop];
        all_loops.extend(face_data.inner_loops.iter().copied());
        for lp in all_loops {
            if lp.0 >= arena.loops.len() {
                continue;
            }
            let start = arena.loops[lp.0].half_edge;
            let mut he = start;
            for _ in 0..1_000_000 {
                if arena.half_edges[he.0].edge == edge {
                    return Some(he);
                }
                he = arena.half_edges[he.0].next;
                if he == start {
                    break;
                }
            }
        }
        None
    };

    let he_a_face = match he_for_face(face_a) {
        Some(h) => h,
        None => {
            return Pr7Classification::ArenaMissingEdge {
                edge,
                edge_he_face,
                edge_twin_face,
            };
        }
    };
    let he_b_face = match he_for_face(face_b) {
        Some(h) => h,
        None => {
            return Pr7Classification::ArenaMissingEdge {
                edge,
                edge_he_face,
                edge_twin_face,
            };
        }
    };

    // What pool indices does the loop walker append for each face's HE on
    // this edge? Re-implement the per-edge slice of `collect_loop_boundary`
    // (mod.rs:3218) so we can inspect just this edge in isolation.
    let pool_for = |he_idx: HalfEdgeIdx| -> Vec<usize> {
        let edge_idx = arena.half_edges[he_idx.0].edge;
        let edge_data = &arena.edges[edge_idx.0];
        let verts = match disc.edge_verts.get(&edge_idx) {
            Some(v) => v.clone(),
            None => return Vec::new(),
        };
        let is_primary = edge_data.half_edge == he_idx;
        let is_self_loop = arena.half_edges[he_idx.0].next == he_idx;
        if is_self_loop {
            if is_primary {
                verts
            } else {
                verts.into_iter().rev().collect()
            }
        } else if verts.len() <= 2 {
            if is_primary {
                vec![verts[0]]
            } else {
                vec![verts[verts.len() - 1]]
            }
        } else {
            let is_full_circle = verts.len() == super::circle_segments();
            if is_full_circle {
                if is_primary {
                    verts
                } else {
                    verts.into_iter().rev().collect()
                }
            } else if is_primary {
                verts[..verts.len() - 1].to_vec()
            } else {
                verts.into_iter().rev().skip(1).collect()
            }
        }
    };

    let pool_a = pool_for(he_a_face);
    let pool_b = pool_for(he_b_face);

    // For a shared edge between two faces, both faces' loop walkers should
    // each emit ONE position from the shared edge (the half-edge's origin
    // for linear edges) — but those two positions taken together cover the
    // edge's two endpoints. The "pool is shared" property is that the
    // *position in the pool* both walkers reference IS valid (i.e. the
    // walkers share the underlying f64 pool entries for this edge).
    //
    // The PR7 brief's `pool-not-shared` symptom is: the walkers return
    // DIFFERENT pool indices for what should be the same arena vertex.
    // We test this by comparing the f64 positions that pool_a and pool_b
    // resolve to: if they're at the same arena edge endpoint but different
    // pool indices with byte-different f64 → pool-not-shared at the
    // discretization layer. If same f64 byte pattern and same pool index
    // → pool IS shared.
    //
    // Stronger: if `disc.edge_verts[edge]` was a full sequence (n>2 for an
    // arc/circle), check whether collecting both walkers gives the same
    // *set* of pool indices.
    let pool_set_a: std::collections::BTreeSet<usize> = pool_a.iter().copied().collect();
    let pool_set_b: std::collections::BTreeSet<usize> = pool_b.iter().copied().collect();
    let f64_a: Vec<[f64; 3]> = pool_a.iter().map(|&i| disc.positions[i]).collect();
    let f64_b: Vec<[f64; 3]> = pool_b.iter().map(|&i| disc.positions[i]).collect();
    let f64_key = |p: [f64; 3]| -> [u64; 3] { [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()] };
    let f64_set_a: std::collections::BTreeSet<[u64; 3]> =
        f64_a.iter().map(|&p| f64_key(p)).collect();
    let f64_set_b: std::collections::BTreeSet<[u64; 3]> =
        f64_b.iter().map(|&p| f64_key(p)).collect();

    // Pool-not-shared = the f64 endpoints both walkers report are DISJOINT.
    // For a properly discretized linear edge, the two walkers should report
    // the two f64 endpoints of the edge between them (one each), so
    // f64_set_a ∪ f64_set_b should be size 2 with the two endpoints. The
    // walkers individually report 1 endpoint each. The check is whether
    // those endpoints come from the same f64 byte pattern as the arena
    // vertex positions.
    let arena_origin_a = arena.vertices[he_a.origin.0].position;
    let arena_dest_a = arena.vertices[he_b.origin.0].position;
    let arena_endpoints: std::collections::BTreeSet<[u64; 3]> = [arena_origin_a, arena_dest_a]
        .iter()
        .map(|&p| f64_key(p))
        .collect();

    // If a walker's reported f64 is NOT in arena_endpoints → pool-not-shared.
    let _ = pool_set_a; // pool indices logged below
    let _ = pool_set_b;
    let bad_a: Vec<[f64; 3]> = f64_a
        .iter()
        .filter(|&&p| !arena_endpoints.contains(&f64_key(p)))
        .copied()
        .collect();
    let bad_b: Vec<[f64; 3]> = f64_b
        .iter()
        .filter(|&&p| !arena_endpoints.contains(&f64_key(p)))
        .copied()
        .collect();
    if !bad_a.is_empty() || !bad_b.is_empty() {
        return Pr7Classification::PoolNotShared {
            edge,
            face_a_pool_at_edge: pool_a.clone(),
            face_b_pool_at_edge: pool_b.clone(),
        };
    }

    // For linear edges, both walkers should each report ONE endpoint, and
    // together they should cover both endpoints. Verify they don't overlap
    // (which would mean both faces walk the edge in the same direction,
    // indicating a half-edge twin pairing problem rather than pool issue).
    if f64_set_a == f64_set_b && f64_set_a.len() == 1 {
        // Both walkers report the SAME endpoint. This is structurally
        // wrong: the two adjacent half-edges should go opposite directions.
        // This is direction-reciprocity at the arena level (not the
        // rendermesh level), but we report it as PoolNotShared since the
        // walker is producing co-oriented input rather than reciprocal.
        return Pr7Classification::PoolNotShared {
            edge,
            face_a_pool_at_edge: pool_a.clone(),
            face_b_pool_at_edge: pool_b.clone(),
        };
    }

    // ── Step 3: positional-drift check ────────────────────────────────────
    // Find the two endpoints' f32 emissions in the rendermesh per-face.
    // For each face, find the f32 vertex index range from face_ranges, then
    // check whether the f32-cast of arena_origin_a and arena_dest_a appears
    // in face_a's emission range AND face_b's emission range with byte-
    // identical f32 patterns.
    let f32_key = |p: [f32; 3]| -> [u32; 3] { [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()] };
    let arena_origin_f32: [f32; 3] = [
        arena_origin_a[0] as f32,
        arena_origin_a[1] as f32,
        arena_origin_a[2] as f32,
    ];
    let arena_dest_f32: [f32; 3] = [
        arena_dest_a[0] as f32,
        arena_dest_a[1] as f32,
        arena_dest_a[2] as f32,
    ];

    // Find rendermesh f32 keys present in each face's emitted vertex
    // sub-buffer. A face's emitted vertex range is the unique vertex
    // indices referenced by its triangles in `face_ranges`.
    let face_emitted_keys = |target: FaceIdx| -> std::collections::BTreeSet<[u32; 3]> {
        let mut out = std::collections::BTreeSet::new();
        for range in &rendermesh.face_ranges {
            let mapped = match face_map.get(&range.face_id.0).copied() {
                Some(f) => f,
                None => continue,
            };
            if mapped != target {
                continue;
            }
            let start = range.start_index as usize;
            let end = (range.end_index as usize).min(rendermesh.indices.len());
            for &i in &rendermesh.indices[start..end] {
                let i = i as usize;
                let p = [
                    rendermesh.vertices[i * 3],
                    rendermesh.vertices[i * 3 + 1],
                    rendermesh.vertices[i * 3 + 2],
                ];
                out.insert(f32_key(p));
            }
        }
        out
    };

    let keys_a = face_emitted_keys(face_a);
    let keys_b = face_emitted_keys(face_b);
    let origin_key = f32_key(arena_origin_f32);
    let dest_key = f32_key(arena_dest_f32);

    // Both faces must contain BOTH endpoints with byte-identical f32. If
    // either face is missing one of the endpoints, that's positional-drift.
    let a_has_origin = keys_a.contains(&origin_key);
    let a_has_dest = keys_a.contains(&dest_key);
    let b_has_origin = keys_b.contains(&origin_key);
    let b_has_dest = keys_b.contains(&dest_key);
    if !(a_has_origin && a_has_dest && b_has_origin && b_has_dest) {
        // Collect the actual emitted bit patterns for diagnostic output.
        let face_a_emitted: Vec<[u32; 3]> = keys_a.iter().take(8).copied().collect();
        let face_b_emitted: Vec<[u32; 3]> = keys_b.iter().take(8).copied().collect();
        return Pr7Classification::PositionalDrift {
            edge,
            face_a_emitted,
            face_b_emitted,
        };
    }

    // ── Step 4: direction-reciprocity check ───────────────────────────────
    // Walk the rendermesh triangles of each face; collect directed edges
    // along this B-Rep edge (i.e., directed edges (P,Q) where {P,Q} ==
    // {origin_key, dest_key}). Face_a should emit one direction; face_b
    // should emit the reverse.
    let face_directed_along = |target: FaceIdx| -> Vec<([u32; 3], [u32; 3])> {
        let mut out = Vec::new();
        let endpoints = std::collections::BTreeSet::from([origin_key, dest_key]);
        for range in &rendermesh.face_ranges {
            let mapped = match face_map.get(&range.face_id.0).copied() {
                Some(f) => f,
                None => continue,
            };
            if mapped != target {
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
                    let p = [
                        rendermesh.vertices[v[k] * 3],
                        rendermesh.vertices[v[k] * 3 + 1],
                        rendermesh.vertices[v[k] * 3 + 2],
                    ];
                    let q = [
                        rendermesh.vertices[v[(k + 1) % 3] * 3],
                        rendermesh.vertices[v[(k + 1) % 3] * 3 + 1],
                        rendermesh.vertices[v[(k + 1) % 3] * 3 + 2],
                    ];
                    let pk = f32_key(p);
                    let qk = f32_key(q);
                    if endpoints.contains(&pk) && endpoints.contains(&qk) && pk != qk {
                        out.push((pk, qk));
                    }
                }
                i += 3;
            }
        }
        out
    };

    let dir_a = face_directed_along(face_a);
    let dir_b = face_directed_along(face_b);

    // ── Step 4a: subdivision-mismatch check ───────────────────────────────
    // If one face emits a directed mesh edge directly between the two arena
    // endpoints (origin → dest or dest → origin) while the other face emits
    // NONE — that's a boundary-subdivision mismatch: one face's tessellation
    // walks the edge in a single mesh-edge segment, the other walks it via
    // interior subdivision vertices. This is `PoolNotShared` at the
    // rendermesh level (the per-face triangulation produces different mesh-
    // vertex sequences along the same B-Rep edge).
    //
    // Mechanism: both endpoints are present in both faces' emissions
    // (positional-drift step passed), but face A's triangulation routes via
    // interior vertices between the endpoints while face B has a single
    // direct edge. The repair pipeline (Steiner-fan, edge-flip, etc.)
    // re-tessellates one face but not the other, breaking shared-boundary
    // discretization.
    if dir_a.is_empty() != dir_b.is_empty() {
        // Asymmetric: one face directly connects endpoints, the other
        // doesn't. This is the strongest subdivision-mismatch signal.
        return Pr7Classification::PoolNotShared {
            edge,
            face_a_pool_at_edge: pool_a.clone(),
            face_b_pool_at_edge: pool_b.clone(),
        };
    }

    if dir_a.is_empty() && dir_b.is_empty() {
        return Pr7Classification::Other {
            reason: format!(
                "neither face emits a directed mesh edge along arena EdgeIdx({}) endpoints \
                 origin={:?} dest={:?}; both faces' triangulations route via interior \
                 vertices — possibly Steiner-fan re-tessellation of both faces",
                edge.0, arena_origin_f32, arena_dest_f32
            ),
        };
    }

    // Check: at least one directed edge in dir_a should have its REVERSE in
    // dir_b (proper twin reciprocity). If ALL of dir_a's edges share their
    // forward direction with dir_b's edges (no reverses), → direction-
    // reciprocity violation.
    let dir_b_set: std::collections::BTreeSet<([u32; 3], [u32; 3])> =
        dir_b.iter().copied().collect();
    let mut reciprocal_count = 0;
    let mut same_direction_count = 0;
    for &(p, q) in &dir_a {
        if dir_b_set.contains(&(q, p)) {
            reciprocal_count += 1;
        }
        if dir_b_set.contains(&(p, q)) {
            same_direction_count += 1;
        }
    }

    if reciprocal_count == 0 && same_direction_count > 0 {
        let sample = dir_a[0];
        let sample_p = [
            f32::from_bits(sample.0[0]),
            f32::from_bits(sample.0[1]),
            f32::from_bits(sample.0[2]),
        ];
        let sample_q = [
            f32::from_bits(sample.1[0]),
            f32::from_bits(sample.1[1]),
            f32::from_bits(sample.1[2]),
        ];
        return Pr7Classification::DirectionReciprocity {
            edge,
            sample_dir: (sample_p, sample_q),
        };
    }

    // If reciprocal_count > 0, the shared B-Rep edge IS bijective in the
    // rendermesh — the oracle's nb signal is from edges OTHER than this
    // arena edge (likely the position-coincidence heuristic catching
    // unrelated boundary segments). Surface as Other.
    Pr7Classification::Other {
        reason: format!(
            "shared B-Rep EdgeIdx({}) reports {} reciprocal + {} same-direction emitted edges \
             across faces ({}, {}); oracle's nb signal must be from other position-coincident \
             boundary segments outside this edge — see oracle's restrict_to_shared_boundary heuristic",
            edge.0, reciprocal_count, same_direction_count, face_a.0, face_b.0
        ),
    }
}
