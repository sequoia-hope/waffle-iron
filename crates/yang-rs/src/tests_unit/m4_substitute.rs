#[allow(unused_imports)]
use super::*;

// =====================================================================
// M4 — demoted substitutes (test-only differential oracle).
//
// These were the production PR-YR3/YR4 spatial-match + majority-vote
// attribution path. M3 replaced production attribution with real
// LabeledArrangement labels; per roadmap rule #9 the substitutes are
// RETAINED here as a second independent attribution method that
// cross-checks the true-label path (the `m4_*` differential test).
// Disagreement on a fixture localizes a label-path bug. Do NOT delete.
// =====================================================================

/// M4 oracle: try to match `target` against a vertex in `brep`'s mesh
/// within `MATCH_TOLERANCE`. Returns the matched vertex's
/// `TessellationSource` or `None`.
pub(crate) fn match_against(brep: &BRep, target: Point3) -> Option<TessellationSource> {
    let tol2 = MATCH_TOLERANCE * MATCH_TOLERANCE;
    for (i, v) in brep.as_mesh().verts.iter().enumerate() {
        let dx = v.x() - target.x();
        let dy = v.y() - target.y();
        let dz = v.z() - target.z();
        if dx * dx + dy * dy + dz * dz <= tol2 {
            return Some(brep.tessellation_map().lookup(i as u32));
        }
    }
    None
}

/// M4 oracle: match `target` against A first, then B; track which
/// input matched.
pub(crate) fn match_with_input(
    a: &BRep,
    b: &BRep,
    target: Point3,
) -> (Option<InputId>, TessellationSource) {
    if let Some(src) = match_against(a, target) {
        return (Some(InputId::A), src);
    }
    if let Some(src) = match_against(b, target) {
        return (Some(InputId::B), src);
    }
    (None, TessellationSource::Intersection)
}

/// M4 oracle: the set of `(InputId, face_idx)` pairs that a single
/// output vertex's provenance is compatible with.
pub(crate) fn face_candidates(
    input: Option<InputId>,
    source: TessellationSource,
    a: &BRep,
    b: &BRep,
) -> Vec<(InputId, u32)> {
    let Some(input) = input else {
        return Vec::new();
    };
    let brep = match input {
        InputId::A => a,
        InputId::B => b,
    };
    match source {
        TessellationSource::BRepFace { face, .. } => vec![(input, face)],
        TessellationSource::BRepEdge { edge, .. } => brep
            .faces()
            .iter()
            .enumerate()
            .filter(|(_, f)| f.outer_loop.contains(&edge))
            .map(|(i, _)| (input, i as u32))
            .collect(),
        TessellationSource::BRepVertex(v) => brep
            .faces()
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.outer_loop.iter().any(|&e| {
                    let edge = &brep.edges()[e as usize];
                    edge.start == v || edge.end == v
                })
            })
            .map(|(i, _)| (input, i as u32))
            .collect(),
        TessellationSource::Intersection | TessellationSource::Unknown => Vec::new(),
    }
}

/// M4 oracle: count votes per `(InputId, face)` across 3 candidate
/// sets; return the highest-count pair reaching ≥2 votes (ties → lowest
/// `(InputId, face)` lexicographic).
pub(crate) fn majority_vote(sets: &[Vec<(InputId, u32)>; 3]) -> Option<TriangleAttribution> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<(InputId, u32), u8> = BTreeMap::new();
    for set in sets {
        let mut uniq: Vec<(InputId, u32)> = set.clone();
        uniq.sort();
        uniq.dedup();
        for c in uniq {
            *counts.entry(c).or_insert(0) += 1;
        }
    }
    let mut best: Option<((InputId, u32), u8)> = None;
    for (key, &count) in &counts {
        if count < 2 {
            continue;
        }
        match best {
            None => best = Some((*key, count)),
            Some((_, bc)) if count > bc => best = Some((*key, count)),
            _ => {}
        }
    }
    best.map(|((input, face), _)| TriangleAttribution { input, face })
}

/// M4 oracle composite: run the full demoted substitute attribution
/// (vertex provenance → per-vertex face candidates → majority vote)
/// over `mesh`, producing a `TriangleAttributionMap`. This is exactly
/// what the pre-M3 production `boolean()` computed internally; the
/// reworked PR-YR4 substitute tests and the yr5_* reconstruction tests
/// call it directly instead of routing through production `boolean()`
/// (whose attribution is now the real-label path).
pub(crate) fn substitute_attribution(mesh: &Mesh, a: &BRep, b: &BRep) -> TriangleAttributionMap {
    let mut inputs: Vec<Option<InputId>> = Vec::with_capacity(mesh.num_verts());
    let mut sources: Vec<TessellationSource> = Vec::with_capacity(mesh.num_verts());
    for &target in &mesh.verts {
        let (inp, src) = match_with_input(a, b, target);
        inputs.push(inp);
        sources.push(src);
    }
    let mut attributions = Vec::with_capacity(mesh.num_tris());
    for tri in &mesh.tris {
        let sets = [
            face_candidates(inputs[tri[0] as usize], sources[tri[0] as usize], a, b),
            face_candidates(inputs[tri[1] as usize], sources[tri[1] as usize], a, b),
            face_candidates(inputs[tri[2] as usize], sources[tri[2] as usize], a, b),
        ];
        attributions.push(majority_vote(&sets));
    }
    TriangleAttributionMap { attributions }
}

pub(crate) fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// An empty (0-triangle) `LabeledArrangement` for backend-dispatch
/// tests that only care about the Ok/err control flow, not labels.
pub(crate) fn empty_arrangement() -> LabeledArrangement {
    LabeledArrangement {
        mesh: Mesh::empty(),
        surface: Vec::new(),
        inside: Vec::new(),
        patch: Vec::new(),
        source: Vec::new(),
        intersection_edges: Default::default(),
        num_inputs: 2,
    }
}

pub(crate) fn sample_mesh() -> Mesh {
    Mesh::new(
        vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)],
        vec![[0, 1, 2]],
    )
}

/// ADVERSARY (spec §2/I1, task #86): a vertex shared by ONE closed
/// 3-triangle fan and ONE OPEN 2-triangle fan must NOT be split. The
/// open fan's boundary edges (each incident to a single triangle) mean
/// the star is not a union of closed disks, so the honest-split guard
/// (`I1`) must leave the vertex — and the whole mesh — untouched, keeping
/// the loud downstream gates in charge. This pins the closed-fan guard:
/// the existing corpus/canonical union oracles cannot catch a weakened
/// guard because their real pinch meshes have only closed fans.
#[test]
pub(crate) fn split_pinch_vertices_leaves_open_fan_untouched() {
    // Vertex 0 is the shared apex. Closed fan: (0,1,2),(0,2,3),(0,3,1)
    // — every 0-incident edge is 2-valent. Open fan: (0,4,5),(0,5,6) —
    // edges (0,4) and (0,6) are 1-valent (boundary). The two fans share
    // no vertex besides 0, so they are separate star components; a
    // guardless split would wrongly cut them into per-fan copies.
    let mut mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),  // 0 apex
            p(1.0, 0.0, 0.0),  // 1
            p(0.0, 1.0, 0.0),  // 2
            p(-1.0, 0.0, 0.0), // 3
            p(0.0, 0.0, 1.0),  // 4
            p(0.0, 0.0, 2.0),  // 5
            p(0.0, 0.0, 3.0),  // 6
        ],
        vec![[0, 1, 2], [0, 2, 3], [0, 3, 1], [0, 4, 5], [0, 5, 6]],
    );
    let before_verts = mesh.verts.len();
    let before_tris = mesh.tris.clone();
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    let splits = split_pinch_vertices(&mut mesh, &mut relocations);
    assert_eq!(splits, 0, "open-fan vertex must not be split (I1 guard)");
    assert_eq!(
        mesh.verts.len(),
        before_verts,
        "open-fan split must not append vertices"
    );
    assert_eq!(
        mesh.tris, before_tris,
        "open-fan split must not rewrite triangle indices"
    );
}

/// ADVERSARY (spec §8/I4, task #86): a bowtie patch — two triangle lobes
/// meeting at ONE mesh-manifold pinch vertex — must walk into TWO
/// separate boundary cycles, one per lobe, NOT one chained self-crossing
/// cycle. The pinch (vertex 3) is entered MID-walk with out-degree 2, and
/// the wedge-correct continuation (stay in the incoming lobe) is
/// deliberately the HIGHER-indexed outgoing edge, so lowest-first would
/// cross into the other lobe and chain both loops into one cycle. This
/// pins the wedge walk; the union oracles cannot catch a lowest-first
/// regression because their post-split walks never hit a mid-walk pinch.
#[test]
pub(crate) fn patch_boundary_cycle_splits_bowtie_into_two_cycles() {
    // Lobe A = tri[3,6,0], Lobe B = tri[3,1,2], sharing pinch vertex 3.
    // Verts 4,5 are unused filler so index 6 is addressable.
    let mesh = Mesh::new(
        vec![
            p(1.0, 1.0, 0.0),  // 0
            p(-1.0, 0.0, 0.0), // 1
            p(-1.0, 1.0, 0.0), // 2
            p(0.0, 0.0, 0.0),  // 3 = pinch
            p(5.0, 5.0, 5.0),  // 4 filler
            p(6.0, 6.0, 6.0),  // 5 filler
            p(1.0, 0.0, 0.0),  // 6
        ],
        vec![[3, 6, 0], [3, 1, 2]],
    );
    let patch = Patch {
        attribution: TriangleAttribution {
            input: InputId::A,
            face: 0,
        },
        tri_indices: vec![0, 1],
    };
    let cycles =
        patch_boundary_cycle(&patch, &mesh).expect("bowtie patch boundary walk must succeed");
    assert_eq!(
        cycles.len(),
        2,
        "bowtie patch must split into 2 per-lobe cycles, not chain into \
             one; got {cycles:?}"
    );
    for c in &cycles {
        assert_eq!(c.len(), 3, "each lobe is a 3-edge triangle boundary");
    }
}

/// Backend whose `boolean()` always errors and which does NOT override
/// the M3 `labeled_arrangement` trait method, so it surfaces through
/// the default ("not supported") error. Used by
/// `boolean_with_err_backend` to confirm `boolean()` maps a backend
/// failure to `YangError::MeshBooleanFailed`.
pub(crate) struct MockBackend;
impl MeshBoolean for MockBackend {
    fn boolean(
        &self,
        _a: &Mesh,
        _b: &Mesh,
        _op: BoolOp,
    ) -> Result<Mesh, Box<dyn Error + Send + Sync>> {
        Err(Box::from("mock failure"))
    }
}
