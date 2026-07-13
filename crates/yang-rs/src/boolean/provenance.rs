//! N4 triangle-provenance diagnostics (`ProvMiss`, `provenance_face_reason`)
//! and the KV15 mixed-operand near-weld pass (`kv15_curved_touch`,
//! `kv15_near_weld_pass`) — the vertex-provenance eligibility + weld helpers
//! consulted by the `boolean()` driver. Extracted verbatim from `boolean.rs`
//! (move-only, spec `specs/yang_rs_lib_decomposition.md` F9).

#[allow(clippy::wildcard_imports)]
use crate::*;

/// Boolean operation on two B-Rep solids via a `MeshBoolean` backend.
///
/// **M3 functional pipeline** (replaces the PR-YR3/YR4 spatial-match +
/// majority-vote substitute, now a `#[cfg(test)]` differential oracle):
///
/// 0. **XOR is deferred (spec §Scope)** — its symmetric-difference result
///    is multi-shell / has a void that `reconstruct_topology` cannot
///    reassemble yet. `boolean()` errors loudly with `UnsupportedOp` once it
///    sees a non-empty XOR kept-set (a degenerate XOR with nothing to
///    reassemble still trivially yields an empty result).
/// 1. Obtain the real Stage-2 [`LabeledArrangement`] from
///    `backend.labeled_arrangement(..)` (full arrangement mesh +
///    per-triangle `surface`/`inside`/`patch` labels).
/// 2. **I6 weld** — the C++ producer does NOT always weld coincident
///    vertices (e.g. A@[0,0,0]/B@[0.7,0.3,0.4] emits a bit-exact duplicate
///    vertex used by shared triangles), so yang welds: map each vertex to
///    the *original index* of its first bit-identical occurrence. yang's
///    index-based adjacency then sees coincident points as one index. A
///    kept triangle that welds to a repeated index is a zero-area sliver at
///    that coincident point — dropped (no surface/volume; its edges pair up
///    so the output stays watertight). Two *distinct* surviving triangles
///    that weld to the same 3 indices are genuinely coincident faces →
///    `NonManifoldInput` (the a4 bit-exact-coincident-vertex case).
/// 3. `keep = la.keep_set(op)` — Stage 4 face survival.
/// 4. Compact the welded kept tris into a fresh sub-mesh (the output mesh).
/// 5. **Geometric face resolution** (Stage 6) per kept tri → a FULL
///    `TriangleAttributionMap` (every entry `Some`). A SURVIVING
///    multi-solid `surface[t]` (a §4.5.5 overlap-sheet triangle the (3b)
///    side rule kept) attributes to input A — the dedup survivor's side,
///    whose winding it carries (PR-YR26; B's coincident face has the same
///    plane, so the inherited output surface is identical). For a
///    *non-degenerate* (positive-area) triangle: pick the unique labeled-solid
///    face plane within `TAU_WORK` of the centroid; no match / a genuine tie →
///    `FaceResolutionFailed` (F3). For a *degenerate* (zero-area sliver, kept
///    because its edges pair into the watertight result) triangle: attribute
///    to the LOWEST labeled-solid face index within `TAU_WORK` (its centroid
///    sits on a solid edge, so the two adjacent planes tie — harmless for a
///    zero-area tri; never F3). Never a silent `None` (P9).
/// 6. `reconstruct_topology(..)` — flood-fill patches, walk boundary
///    cycles, inherit input-face `Surface`; full attribution ⇒ closed
///    boundary cycles ⇒ watertight 2-manifold output.
///
/// **N4 (provenance):** before the geometric resolution in step 5, a kept
/// triangle is attributed DIRECTLY from cherchi's per-triangle provenance
/// (`LabeledArrangement.source` → the parent input triangle → its B-Rep face via
/// the Stage-1 `tri_face` map) whenever that is unambiguous. The geometric path
/// remains the fallback. See [`provenance_face_reason`].
///
/// N4 helper: resolve a kept arrangement triangle's B-Rep face from cherchi's
/// per-triangle provenance (`§4.2.3`), not geometric centroid-proximity.
///
/// The triangle is attributed to `surface_input` (A or B — the side the keep-rule
/// kept it on; for a coplanar overlap sheet the §4.5.5 survivor convention picks
/// A). We select that side's parent from `source` and resolve it through that
/// input mesh's per-triangle face map (`tri_face_a` for A, `tri_face_b` for B).
/// This handles BOTH a non-coplanar triangle (its only parent) AND a coplanar
/// overlap sheet (the parent on the kept side). Returns `None` (→ geometric
/// fallback) when that side has no parent in `source`, the parent is beyond
/// the face map (a Stage-0 path that did not emit provenance, or a lineage-less
/// `from_mesh` / boolean-output input), or the parent maps to the `u32::MAX`
/// sentinel (a producer that emitted a map but could not attribute THAT
/// triangle — e.g. a coincident-cylinder band-strip column with no covering
/// arc-patch face). Never a wrong face.
/// Why N4 provenance attribution could not name a face for a kept triangle —
/// the exact reason the Stage-6 geometric fallback is still reached. Used by the
/// `YANG_N4_FALLBACK_PROBE` measurement (N4 retirement: prove the geometric path
/// is dead in production, or name the producers that still leave a triangle
/// un-provenanced).
#[derive(Debug, Clone, Copy)]
pub(crate) enum ProvMiss {
    /// The kept triangle's `source` has no parent triangle from this input
    /// (e.g. a cut/arrangement triangle with only the OTHER input's lineage).
    /// On a lineage-carrying input this is a producer FAULT (loud).
    NoSourceEntry,
    /// This input emitted NO provenance map at all (empty `tri_face`) — a
    /// LINEAGE-LESS input: a yang boolean OUTPUT chained directly back in,
    /// or a `from_mesh` B-Rep. This is the documented geometric-resolution
    /// path (task #53), NOT a fault.
    NoLineage,
    /// The map is present but the parent-triangle index lies beyond it —
    /// the producer emitted a TOO-SHORT provenance map (fault, loud).
    NoMap,
    /// The producer minted this triangle but could not attribute it to a face
    /// (`u32::MAX` sentinel — e.g. the coincident-cylinder band strip with no
    /// covering arc column). Fault, loud.
    Sentinel,
}

/// N4 (§4.2.3): map a kept triangle to its owning B-Rep face via the
/// arrangement's per-triangle provenance. `Ok(face)` on a hit; `Err(reason)`
/// records WHY it missed — `NoLineage` is the one non-fault reason (the
/// input never had a provenance map), everything else is loud at the caller
/// (task #53, spec `specs/n4_retire_stage6_fallback.md`).
pub(crate) fn provenance_face_reason(
    source: &[(LaInputId, u32)],
    surface_input: InputId,
    tri_face_a: &[u32],
    tri_face_b: &[u32],
) -> Result<u32, ProvMiss> {
    let (want_k, tf): (u32, &[u32]) = match surface_input {
        InputId::A => (0, tri_face_a),
        InputId::B => (1, tri_face_b),
    };
    if tf.is_empty() {
        return Err(ProvMiss::NoLineage);
    }
    let &(_, local) = source
        .iter()
        .find(|&&(LaInputId(k), _)| k == want_k)
        .ok_or(ProvMiss::NoSourceEntry)?;
    match tf.get(local as usize).copied() {
        None => Err(ProvMiss::NoMap),
        Some(f) if f == u32::MAX => Err(ProvMiss::Sentinel),
        Some(f) => Ok(f),
    }
}

/// KV15 (spec `kv15_mixed_operand_planar_near_weld` §3): per-vertex weld
/// eligibility for MIXED operands. A vertex is CURVED-ADJACENT (ineligible
/// for the near-weld, `true` in the returned vec) when ANY incident
/// arrangement triangle fails to prove planar descent: empty provenance
/// (`source[t]` empty — e.g. the sidecar parity producer, spec W4),
/// out-of-range / `u32::MAX`-sentinel `tri_face` entries, an out-of-range
/// face index, or a face whose surface is not `Surface::Plane`
/// (`face_planar` returns `Some(false)` — or `None` for a bad index).
/// Conservative by construction: only positively-proven all-planar descent
/// yields eligibility.
pub(crate) fn kv15_curved_touch(
    n_verts: usize,
    tris: &[[u32; 3]],
    source: &[Vec<(LaInputId, u32)>],
    tri_face_a: &[u32],
    tri_face_b: &[u32],
    face_planar: impl Fn(u32, u32) -> Option<bool>,
) -> Vec<bool> {
    let mut curved = vec![false; n_verts];
    for (t, tri) in tris.iter().enumerate() {
        let src = source.get(t).map(Vec::as_slice).unwrap_or(&[]);
        let tri_curved = src.is_empty()
            || src.iter().any(|&(LaInputId(k), local)| {
                let tf: &[u32] = if k == 0 { tri_face_a } else { tri_face_b };
                match tf.get(local as usize).copied() {
                    Some(fi) if fi != u32::MAX => !matches!(face_planar(k, fi), Some(true)),
                    _ => true,
                }
            });
        if tri_curved {
            for &v in tri {
                if let Some(slot) = curved.get_mut(v as usize) {
                    *slot = true;
                }
            }
        }
    }
    curved
}

/// KV15 (spec §3): near-union among planar-only weld roots — the identical
/// grid, per-pair band `TAU_WORK·(1+max|coord|)`, and min-index-survivor
/// rule as the all-planar KV10 weld (spec I2/I4). `weld` enters as the
/// bit-exact weld map (each entry pointing at its cluster's original
/// representative) and leaves fully resolved. Roots flagged in
/// `root_curved` never participate (kv9 junction-duplicate protection).
pub(crate) fn kv15_near_weld_pass(verts: &[Point3], weld: &mut [u32], root_curved: &[bool]) {
    use std::collections::HashMap;
    let mut parent: Vec<u32> = weld.to_vec();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    let scale = verts
        .iter()
        .flat_map(|v| v.as_array())
        .fold(0.0f64, |m, c| m.max(c.abs()));
    let band = cad_primitives::TAU_WORK * (1.0 + scale);
    let cell = |c: f64| -> i64 { (c / band).floor() as i64 };
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    for i in 0..verts.len() as u32 {
        if weld[i as usize] != i || root_curved[i as usize] {
            continue;
        }
        let p = verts[i as usize].as_array();
        let key = [cell(p[0]), cell(p[1]), cell(p[2])];
        for dx in -1..=1i64 {
            for dy in -1..=1i64 {
                for dz in -1..=1i64 {
                    let Some(occ) = grid.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) else {
                        continue;
                    };
                    for &j in occ {
                        let q = verts[j as usize].as_array();
                        let pair_band = cad_primitives::TAU_WORK
                            * (1.0 + p.iter().chain(q.iter()).fold(0.0f64, |m, c| m.max(c.abs())));
                        if (0..3).all(|k| (p[k] - q[k]).abs() <= pair_band) {
                            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                            if ri != rj {
                                parent[ri.max(rj) as usize] = ri.min(rj);
                            }
                        }
                    }
                }
            }
        }
        grid.entry(key).or_default().push(i);
    }
    for w in weld.iter_mut() {
        *w = find(&mut parent, *w);
    }
}
