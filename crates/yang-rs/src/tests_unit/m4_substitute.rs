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
    let splits = split_pinch_vertices(&mut mesh, &mut relocations, &[], false);
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

// =========================================================================
// EDGE-PINCH certificate (spec `yang_tangency_pinch_split.md` §0a, F0060).
//
// A face of one operand TANGENT to a face of the other along a whole LINE
// reaches Stage 4 as a chain of 4-triangle edges. The two sheets have to be
// told apart before the vertex fans can separate, and the usual discriminator
// is gone: at a tangency the curved operand's contact triangles are ZERO-AREA
// (they lie in the other operand's plane), so dihedral sorting is degenerate by
// construction. `edge_pinch_sheets` replaces it with an exact, tolerance-free
// certificate — a sheet is `face-of-A ∪ face-of-B`, so it holds exactly one
// triangle per operand, and a consistently-wound surface gives each sheet one
// FORWARD and one REVERSE triangle on the shared edge. These pin both the
// pairing it produces and every shape it must REFUSE.
// =========================================================================

/// The §0a fixture: edge (0,1) carries four triangles — A forward `[0,1,2]`,
/// A reverse `[1,0,3]`, B forward `[0,1,4]`, B reverse `[1,0,5]` — and the two
/// closers `[0,2,5]` / `[0,4,3]` make each sheet a CLOSED fan at vertex 0.
fn edge_pinch_fixture() -> (Mesh, Vec<u32>, Vec<usize>) {
    let mesh = Mesh::new(
        vec![
            p(0.0, 0.0, 0.0),  // 0 — the pinch edge's low end
            p(1.0, 0.0, 0.0),  // 1 — the pinch edge's high end
            p(0.0, 1.0, 0.0),  // 2 — A, +side
            p(0.0, -1.0, 0.0), // 3 — A, −side
            p(0.0, 0.0, -1.0), // 4 — B, −side
            p(0.0, 0.0, 1.0),  // 5 — B, +side
        ],
        vec![
            [0, 1, 2], // 0  A fwd
            [1, 0, 3], // 1  A rev
            [0, 1, 4], // 2  B fwd
            [1, 0, 5], // 3  B rev
            [0, 2, 5], // 4  closer, sheet 1
            [0, 4, 3], // 5  closer, sheet 2
        ],
    );
    // Vertex 0's star, in triangle order; the four on edge (0,1) are local
    // indices 0..4 — the shape `split_pinch_vertices` hands the certificate.
    let star: Vec<u32> = vec![0, 1, 2, 3, 4, 5];
    let local_on_edge: Vec<usize> = vec![0, 1, 2, 3];
    (mesh, star, local_on_edge)
}

fn attr(input: InputId, face: u32) -> Option<TriangleAttribution> {
    Some(TriangleAttribution { input, face })
}

/// The pairing is FORCED: each operand's forward triangle goes with the OTHER
/// operand's reverse triangle. Nothing here reads a position or an angle.
#[test]
pub(crate) fn edge_pinch_sheets_pairs_each_operand_with_the_others_opposite_winding() {
    let (mesh, star, local) = edge_pinch_fixture();
    let attribution = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 0),
        attr(InputId::B, 2),
        attr(InputId::B, 2),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    let sheets =
        edge_pinch_sheets(&mesh, &attribution, 0, 1, &star, &local).expect("certificate holds");
    let mut got: Vec<[usize; 2]> = sheets
        .iter()
        .map(|&(x, y)| {
            let mut s = [x, y];
            s.sort_unstable();
            s
        })
        .collect();
    got.sort_unstable();
    // A fwd (local 0) with B rev (local 3); A rev (local 1) with B fwd (local 2).
    assert_eq!(got, vec![[0, 3], [1, 2]], "forced per-sheet pairing");
}

/// REFUSED: four triangles from the SAME operand. A solid self-touching along a
/// line is a different defect and the certificate must not guess at it — this
/// is F0060's `(48,79)` after §4.4.1(b) collapses a chain into one edge.
#[test]
pub(crate) fn edge_pinch_sheets_refuses_a_same_operand_four_valent_edge() {
    let (mesh, star, local) = edge_pinch_fixture();
    let attribution = vec![attr(InputId::B, 2); 6];
    assert!(edge_pinch_sheets(&mesh, &attribution, 0, 1, &star, &local).is_none());
}

/// REFUSED: an operand contributing TWO forward triangles. The winding split is
/// half the certificate; without it the pairing is not determined.
#[test]
pub(crate) fn edge_pinch_sheets_refuses_two_forward_triangles_from_one_operand() {
    let (mut mesh, star, local) = edge_pinch_fixture();
    // Flip A's reverse triangle to forward: [1,0,3] -> [0,1,3].
    mesh.tris[1] = [0, 1, 3];
    let attribution = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 0),
        attr(InputId::B, 2),
        attr(InputId::B, 2),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    assert!(edge_pinch_sheets(&mesh, &attribution, 0, 1, &star, &local).is_none());
}

/// REFUSED: an unattributed triangle. `None` attribution means no `(input,
/// face)` won a majority, so the operand split cannot be read — fail closed.
#[test]
pub(crate) fn edge_pinch_sheets_refuses_an_unattributed_triangle() {
    let (mesh, star, local) = edge_pinch_fixture();
    let attribution = vec![
        None,
        attr(InputId::A, 0),
        attr(InputId::B, 2),
        attr(InputId::B, 2),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    assert!(edge_pinch_sheets(&mesh, &attribution, 0, 1, &star, &local).is_none());
}

/// The split itself stays OFF by default: with `YANG_EDGE_PINCH_SPLIT` unset
/// (the state every other test and the whole corpus runs in) the 4-valent edge
/// makes `split_pinch_vertices` leave the vertex alone, exactly as before.
#[test]
pub(crate) fn split_pinch_vertices_leaves_the_edge_pinch_untouched_by_default() {
    let (mut mesh, _, _) = edge_pinch_fixture();
    let attribution = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 0),
        attr(InputId::B, 2),
        attr(InputId::B, 2),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    let before = mesh.tris.clone();
    let before_verts = mesh.verts.len();
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    let splits = split_pinch_vertices(&mut mesh, &mut relocations, &attribution, false);
    assert_eq!(splits, 0, "gate OFF: the edge pinch must not be split");
    assert_eq!(mesh.tris, before);
    assert_eq!(mesh.verts.len(), before_verts);
}

/// ARMED: the certified pinch separates. Vertex 0's fan ring is cut at edge
/// (0,1) into the two closed fans the certificate names, so the vertex splits
/// into one copy per sheet and the 4-triangle edge becomes two 2-triangle
/// edges — which is the whole point of the operation.
#[test]
pub(crate) fn split_pinch_vertices_separates_a_certified_edge_pinch() {
    let (mut mesh, _, _) = edge_pinch_fixture();
    let attribution = vec![
        attr(InputId::A, 0),
        attr(InputId::A, 0),
        attr(InputId::B, 2),
        attr(InputId::B, 2),
        attr(InputId::A, 0),
        attr(InputId::A, 0),
    ];
    let before_verts = mesh.verts.len();
    let mut relocations: Vec<(u32, f64)> = Vec::new();
    let splits = split_pinch_vertices(&mut mesh, &mut relocations, &attribution, true);
    assert_eq!(splits, 1, "vertex 0 splits once, into its two sheets");
    assert_eq!(mesh.verts.len(), before_verts + 1);
    // The copy carries IDENTICAL position bits (I2).
    assert_eq!(
        mesh.verts[before_verts].as_array(),
        mesh.verts[0].as_array()
    );
    // No edge of the result carries more than two triangles any more.
    let mut inc: std::collections::BTreeMap<(u32, u32), usize> = std::collections::BTreeMap::new();
    for tri in &mesh.tris {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (tri[i], tri[j]);
            let k = if a < b { (a, b) } else { (b, a) };
            *inc.entry(k).or_insert(0) += 1;
        }
    }
    assert!(
        inc.values().all(|&n| n <= 2),
        "the pinch edge must be gone: {inc:?}"
    );
    // Each sheet kept one A triangle and one B triangle on its own copy.
    let low_end = |t: &[u32; 3]| t.contains(&0);
    let sheet_a: Vec<usize> = (0..4).filter(|&i| low_end(&mesh.tris[i])).collect();
    assert_eq!(sheet_a.len(), 2, "two of the four keep vertex 0");
    let inputs: std::collections::BTreeSet<InputId> = sheet_a
        .iter()
        .map(|&i| attribution[i].expect("attributed").input)
        .collect();
    assert_eq!(
        inputs.len(),
        2,
        "a sheet is one A triangle + one B triangle"
    );
}
