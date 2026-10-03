//! Content-seeded persistent ids for edges and vertices — D0 of
//! `specs/drawings_and_mbd.md` §4 (items 2 and 3).
//!
//! # Why content-seeded, and seeded from what
//!
//! A face's [`Pid`] is stamped by the constructor that made the face
//! ([`BrepArena::assign_face_pids`], KV13 F1) and is therefore **fresh on
//! every boolean**: `boolean_op` builds new faces, so the plate's far wall
//! has one pid before a hole is drilled and a different one after. The
//! stable name of that wall is its **lineage root** — the pid where the
//! geometry was introduced, which `journal::face_lineage` recovers through
//! any number of chained booleans.
//!
//! So an edge is named from the *roots* of its two adjacent faces, not from
//! their current pids:
//!
//! ```text
//! edge_pid    = H("edge",   root(face₁), root(face₂), rank)   // roots sorted
//! vertex_pid  = H("vertex", edge_pid… ,                rank)   // sorted, deduped
//! ```
//!
//! This is the spec's rule read through the journal. §4 item 2 says an
//! intersection edge born in a boolean "take[s] the pair of operand face
//! Pids they lie on" — the adjacent output faces of such an edge descend
//! from exactly those operand faces, so their roots *are* that pair. The
//! same formula therefore covers construct-born and boolean-born edges with
//! no special case, and an edge of untouched geometry keeps its name across
//! an edit elsewhere on the body.
//!
//! # The `rank` disambiguator
//!
//! Two different edges can share a root pair (a boolean that splits one
//! operand face into two patches leaves both patches rooted at the same
//! face, and each may meet a common third face). The spec calls for "a
//! disambiguator when the same face pair shares more than one edge". The
//! disambiguator here is the edge's **rank** inside its root-pair group,
//! ordered by the edge's unordered endpoint pair under a total order on
//! coordinates. Groups of one — the overwhelming majority — rank 0, so the
//! common case is pure content. Two edges of one group whose endpoint pairs
//! compare *equal* are an unresolvable ambiguity and a loud
//! [`KernelV2Error::PidAmbiguous`], never a coin flip.
//!
//! Vertices rank the same way inside their incident-edge-pid-set group,
//! ordered by position.
//!
//! # Not stored
//!
//! These ids are **derived, never stored**: [`solid_pids`] recomputes them
//! from the arena and the journal. There is no second source of truth to
//! invalidate, nothing added to [`BrepArena`] (whose `Debug` string the
//! determinism oracle compares), and no way for a stale id to outlive the
//! geometry it named. The cost is one `O(E log E)` pass per query; callers
//! that want every id at once ask for [`solid_pids`] directly.

use std::collections::{BTreeMap, BTreeSet};

use crate::arena::{BrepArena, FaceId, HalfEdgeId, Pid, SolidId, VertexId};
use crate::error::KernelV2Error;
use crate::journal::face_lineage;
use cad_primitives::Point3;

/// Domain tag mixed into every edge pid, so an edge and a vertex with
/// numerically equal keys cannot land on the same id.
const DOMAIN_EDGE: u64 = 0x4544_4745_5f56_3100; // "EDGE_V1"
/// Domain tag mixed into every vertex pid.
const DOMAIN_VERTEX: u64 = 0x5645_5254_5f56_3100; // "VERT_V1"

/// 64-bit mixing step (the SplitMix64 finalizer applied to `state ^ value`).
/// Chosen because it is a well-studied avalanche function that needs no
/// dependency and is identical on every target — the ids are persisted in
/// documents, so the hash must never drift.
fn mix(state: u64, value: u64) -> u64 {
    let mut z = state.rotate_left(27) ^ value.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fold a sequence of `u64` words into one id, starting from a domain tag.
fn digest(domain: u64, words: &[u64]) -> Pid {
    let mut h = mix(domain, words.len() as u64);
    for &w in words {
        h = mix(h, w);
    }
    // Pid(0) is a legal monotonic face pid; keeping content ids off it costs
    // nothing and makes an unset id obvious in a dump.
    Pid(if h == 0 { 1 } else { h })
}

/// Every persistent id of one solid, in one pass.
///
/// `faces` is what the arena stamped (monotonic, churns on every boolean);
/// `face_roots` is each face's lineage root; `edges` and `vertices` are the
/// content-seeded ids this module derives. `edges` is keyed by the
/// **canonical** half-edge of each twin pair (`min(h, twin(h))`), the same
/// key `KernelV2Adapter` encodes as an edge `KernelId`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SolidPids {
    /// Stamped pid per face.
    pub faces: BTreeMap<FaceId, Pid>,
    /// Lineage root per face (equals `faces[f]` for construct-born geometry).
    pub face_roots: BTreeMap<FaceId, Pid>,
    /// Content-seeded pid per canonical half-edge.
    pub edges: BTreeMap<HalfEdgeId, Pid>,
    /// Content-seeded pid per vertex.
    pub vertices: BTreeMap<VertexId, Pid>,
}

/// Total order on a coordinate triple (`f64::total_cmp`, lexicographic).
/// Exact — no quantization — so the order is a function of the bits and
/// cannot flip under a re-run.
fn point_key(p: Point3) -> [u64; 3] {
    let a = p.as_array();
    [a[0], a[1], a[2]].map(|c| {
        // Monotone map from f64 to u64 (IEEE-754 ordering), so a plain
        // integer compare reproduces `total_cmp`.
        let bits = c.to_bits();
        if bits & (1 << 63) == 0 {
            bits ^ (1 << 63)
        } else {
            !bits
        }
    })
}

/// The faces of a solid, ascending, deduped.
fn solid_faces(arena: &BrepArena, solid: SolidId) -> Result<Vec<FaceId>, KernelV2Error> {
    let shells = arena.solid(solid)?.shells.clone();
    let mut faces: Vec<FaceId> = Vec::new();
    for sh in shells {
        faces.extend(arena.shell(sh)?.faces.iter().copied());
    }
    faces.sort_unstable_by_key(|f| f.0);
    faces.dedup();
    Ok(faces)
}

/// All half-edges of a face (outer loop then rings), in walk order.
fn face_half_edges(arena: &BrepArena, face: FaceId) -> Result<Vec<HalfEdgeId>, KernelV2Error> {
    let f = arena.face(face)?;
    let mut loops = vec![f.outer_loop];
    loops.extend(f.inner_loops.iter().copied());
    let mut out = Vec::new();
    for lid in loops {
        out.extend(arena.loop_half_edges(lid)?);
    }
    Ok(out)
}

/// The canonical half-edge of `h`'s twin pair: `min(h, twin(h))`.
pub fn canonical_edge(arena: &BrepArena, h: HalfEdgeId) -> Result<HalfEdgeId, KernelV2Error> {
    let he = arena.half_edge(h)?;
    Ok(h.min(he.twin))
}

/// The face on each side of a canonical half-edge, `(own, twin)`.
fn edge_faces(arena: &BrepArena, canonical: HalfEdgeId) -> Result<(FaceId, FaceId), KernelV2Error> {
    let he = arena.half_edge(canonical)?;
    let own = arena.loop_(he.loop_id)?.face;
    let twin_he = arena.half_edge(he.twin)?;
    let other = arena.loop_(twin_he.loop_id)?.face;
    Ok((own, other))
}

/// The two endpoint positions of a canonical half-edge, as an **unordered**
/// pair put in total order — direction-independent, so the key does not
/// depend on which half of the twin pair happened to be canonical.
fn edge_endpoint_key(
    arena: &BrepArena,
    canonical: HalfEdgeId,
) -> Result<[[u64; 3]; 2], KernelV2Error> {
    let he = arena.half_edge(canonical)?;
    let a = point_key(arena.vertex(he.origin)?.point);
    let next = arena.half_edge(he.next)?;
    let b = point_key(arena.vertex(next.origin)?.point);
    Ok(if a <= b { [a, b] } else { [b, a] })
}

/// Rank the members of each equal-key group, loudly refusing a group whose
/// members cannot be told apart.
///
/// `items` is `(entity, content_key, tie_key)`. Returns the rank of each
/// entity inside its `content_key` group, with members ordered by `tie_key`.
fn rank_groups<E: Copy + Ord, C: Ord + Clone, T: Ord + Clone>(
    items: &[(E, C, T)],
    kind: &'static str,
) -> Result<BTreeMap<E, u64>, KernelV2Error> {
    let mut groups: BTreeMap<C, Vec<(T, E)>> = BTreeMap::new();
    for (e, c, t) in items {
        groups.entry(c.clone()).or_default().push((t.clone(), *e));
    }
    let mut ranks: BTreeMap<E, u64> = BTreeMap::new();
    for (_, mut members) in groups {
        if members.len() > 1 {
            members.sort();
            for w in members.windows(2) {
                if w[0].0 == w[1].0 {
                    return Err(KernelV2Error::PidAmbiguous { kind });
                }
            }
        }
        for (i, (_, e)) in members.into_iter().enumerate() {
            ranks.insert(e, i as u64);
        }
    }
    Ok(ranks)
}

/// Every persistent id of `solid`: the stamped face pids, their lineage
/// roots, and the content-seeded edge and vertex pids.
///
/// Errors loudly rather than inventing an identity:
/// - [`KernelV2Error::PidMissing`] — a face of the solid carries no stamped
///   `Pid` (a raw Euler-operator arena that never reached a constructor's
///   `finalize_solid`). There is nothing to seed from.
/// - [`KernelV2Error::PidAmbiguous`] — two edges (or vertices) agree on both
///   their content key and their geometric tie-break key.
/// - [`KernelV2Error::PidCollision`] — two distinct keys hashed to one id.
pub fn solid_pids(arena: &BrepArena, solid: SolidId) -> Result<SolidPids, KernelV2Error> {
    let faces = solid_faces(arena, solid)?;

    // --- faces: stamped pid + lineage root --------------------------------
    let mut face_pids: BTreeMap<FaceId, Pid> = BTreeMap::new();
    let mut face_roots: BTreeMap<FaceId, Pid> = BTreeMap::new();
    for &f in &faces {
        let pid = arena
            .face_pid(f)
            .ok_or(KernelV2Error::PidMissing { face: f })?;
        face_pids.insert(f, pid);
        face_roots.insert(f, face_lineage(&arena.journal, pid).root);
    }

    // --- edges: H(root pair, rank) ----------------------------------------
    let mut canonical: BTreeSet<HalfEdgeId> = BTreeSet::new();
    for &f in &faces {
        for h in face_half_edges(arena, f)? {
            canonical.insert(canonical_edge(arena, h)?);
        }
    }
    let mut edge_items: Vec<(HalfEdgeId, [u64; 2], [[u64; 3]; 2])> = Vec::new();
    for &e in &canonical {
        let (fa, fb) = edge_faces(arena, e)?;
        let ra = *face_roots
            .get(&fa)
            .ok_or(KernelV2Error::PidMissing { face: fa })?;
        let rb = *face_roots
            .get(&fb)
            .ok_or(KernelV2Error::PidMissing { face: fb })?;
        let pair = if ra <= rb { [ra.0, rb.0] } else { [rb.0, ra.0] };
        edge_items.push((e, pair, edge_endpoint_key(arena, e)?));
    }
    let edge_ranks = rank_groups(&edge_items, "edge")?;
    let mut edges: BTreeMap<HalfEdgeId, Pid> = BTreeMap::new();
    let mut seen: BTreeSet<Pid> = BTreeSet::new();
    for (e, pair, _) in &edge_items {
        let rank = edge_ranks[e];
        let pid = digest(DOMAIN_EDGE, &[pair[0], pair[1], rank]);
        if !seen.insert(pid) {
            return Err(KernelV2Error::PidCollision { kind: "edge" });
        }
        edges.insert(*e, pid);
    }

    // --- vertices: H(sorted incident edge pids, rank) ---------------------
    let mut incident: BTreeMap<VertexId, BTreeSet<Pid>> = BTreeMap::new();
    for (&e, &pid) in &edges {
        let he = arena.half_edge(e)?;
        let twin = arena.half_edge(he.twin)?;
        incident.entry(he.origin).or_default().insert(pid);
        incident.entry(twin.origin).or_default().insert(pid);
    }
    let mut vertex_items: Vec<(VertexId, Vec<u64>, [u64; 3])> = Vec::new();
    for (&v, pids) in &incident {
        let key: Vec<u64> = pids.iter().map(|p| p.0).collect();
        vertex_items.push((v, key, point_key(arena.vertex(v)?.point)));
    }
    let vertex_ranks = rank_groups(&vertex_items, "vertex")?;
    let mut vertices: BTreeMap<VertexId, Pid> = BTreeMap::new();
    let mut seen_v: BTreeSet<Pid> = BTreeSet::new();
    for (v, key, _) in &vertex_items {
        let mut words = key.clone();
        words.push(vertex_ranks[v]);
        let pid = digest(DOMAIN_VERTEX, &words);
        if !seen_v.insert(pid) {
            return Err(KernelV2Error::PidCollision { kind: "vertex" });
        }
        vertices.insert(*v, pid);
    }

    Ok(SolidPids {
        faces: face_pids,
        face_roots,
        edges,
        vertices,
    })
}

/// The solid that owns `face`.
pub fn solid_of_face(arena: &BrepArena, face: FaceId) -> Result<SolidId, KernelV2Error> {
    let shell = arena.face(face)?.shell;
    Ok(arena.shell(shell)?.solid)
}

/// The solid that owns a half-edge (via its loop's face).
pub fn solid_of_half_edge(arena: &BrepArena, h: HalfEdgeId) -> Result<SolidId, KernelV2Error> {
    let he = arena.half_edge(h)?;
    let face = arena.loop_(he.loop_id)?.face;
    solid_of_face(arena, face)
}

/// The solid that owns a vertex, found through its first outgoing half-edge.
/// A [`Vertex`](crate::arena::Vertex) stores only its position, so this scans
/// the half-edge slots in id order (deterministic).
pub fn solid_of_vertex(arena: &BrepArena, v: VertexId) -> Result<SolidId, KernelV2Error> {
    for (i, he) in arena.half_edges.iter().enumerate() {
        if he.as_ref().is_some_and(|he| he.origin == v) {
            return solid_of_half_edge(arena, HalfEdgeId(i as u32));
        }
    }
    Err(KernelV2Error::InvalidId { kind: "vertex" })
}

/// The content-seeded pid of one edge, named by any half-edge of its pair.
///
/// Derives the owning solid's whole map (the rank disambiguator is only
/// defined relative to the solid), so prefer [`solid_pids`] when asking for
/// more than a handful.
pub fn edge_pid(arena: &BrepArena, h: HalfEdgeId) -> Result<Pid, KernelV2Error> {
    let solid = solid_of_half_edge(arena, h)?;
    let canonical = canonical_edge(arena, h)?;
    solid_pids(arena, solid)?
        .edges
        .get(&canonical)
        .copied()
        .ok_or(KernelV2Error::InvalidId { kind: "half_edge" })
}

/// The content-seeded pid of one vertex. Same caveat as [`edge_pid`].
pub fn vertex_pid(arena: &BrepArena, v: VertexId) -> Result<Pid, KernelV2Error> {
    let solid = solid_of_vertex(arena, v)?;
    solid_pids(arena, solid)?
        .vertices
        .get(&v)
        .copied()
        .ok_or(KernelV2Error::InvalidId { kind: "vertex" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_domain_separated_and_nonzero() {
        let e = digest(DOMAIN_EDGE, &[7, 9, 0]);
        let v = digest(DOMAIN_VERTEX, &[7, 9, 0]);
        assert_ne!(e, v, "edge and vertex domains must not alias");
        assert_ne!(e.0, 0);
        assert_ne!(v.0, 0);
    }

    #[test]
    fn digest_is_stable_and_order_sensitive() {
        assert_eq!(
            digest(DOMAIN_EDGE, &[1, 2, 0]),
            digest(DOMAIN_EDGE, &[1, 2, 0])
        );
        assert_ne!(
            digest(DOMAIN_EDGE, &[1, 2, 0]),
            digest(DOMAIN_EDGE, &[2, 1, 0])
        );
        assert_ne!(
            digest(DOMAIN_EDGE, &[1, 2, 0]),
            digest(DOMAIN_EDGE, &[1, 2, 1])
        );
    }

    #[test]
    fn point_key_is_monotone_in_each_coordinate() {
        let lo = point_key(Point3::new(-1.0, 0.0, 0.0));
        let mid = point_key(Point3::new(0.0, 0.0, 0.0));
        let hi = point_key(Point3::new(1.0, 0.0, 0.0));
        assert!(lo < mid && mid < hi);
        // -0.0 and +0.0 are distinct bit patterns; the map keeps them ordered.
        assert!(point_key(Point3::new(-0.0, 0.0, 0.0)) <= mid);
    }

    #[test]
    fn rank_groups_numbers_members_by_tie_key() {
        let items = vec![(10u32, "a", 2u32), (11, "a", 1), (12, "b", 9)];
        let ranks = rank_groups(&items, "edge").expect("distinct tie keys");
        assert_eq!(ranks[&11], 0, "lower tie key ranks first");
        assert_eq!(ranks[&10], 1);
        assert_eq!(ranks[&12], 0, "a lone member always ranks 0");
    }

    #[test]
    fn rank_groups_refuses_an_indistinguishable_pair() {
        let items = vec![(10u32, "a", 5u32), (11, "a", 5)];
        assert_eq!(
            rank_groups(&items, "edge"),
            Err(KernelV2Error::PidAmbiguous { kind: "edge" })
        );
    }
}
