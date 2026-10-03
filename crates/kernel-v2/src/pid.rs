//! Content-seeded persistent ids — D0 of `specs/drawings_and_mbd.md` §4:
//! faces (item 1, [`seeded_face_pid`]) and edges/vertices (items 2 and 3,
//! [`solid_pids`]).
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
//! A root used to be a number from a per-arena counter, which is reproducible
//! only when the whole arena is rebuilt in the same order. Since D0 item 1 a
//! root is itself content-seeded ([`seeded_face_pid`]): the creating step's
//! stable name plus the face's role within that step. That is what makes an
//! INCREMENTAL rebuild safe — re-running one step in an arena whose counter
//! has advanced now reproduces that step's face ids, so the edges named from
//! them keep their ids too.
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
//! A rank is a *position*, so it carries the one stability caveat of this
//! scheme: inside a group of two or more, moving a member past another swaps
//! their ids even though neither changed its content key. [`rank_groups`]
//! documents the consequence in full; a group of one is immune.
//!
//! # Determinism
//!
//! Every input to the derivation is read in a deterministic order — the
//! arena's `Vec` slots and `BTreeMap`/`BTreeSet` keys, never a `HashMap` —
//! and the journal walk ([`face_lineage`]) is a reverse scan of a `Vec`.
//! Nothing reads an address, a clock, or a hash seed, so a rebuild in a
//! FRESH PROCESS reproduces the same ids as one in the same process. The
//! hash itself is frozen format: `tests/d0_pid_hash_frozen.rs` pins its
//! output against literals.
//!
//! # Not stored
//!
//! The EDGE and VERTEX ids are **derived, never stored**: [`solid_pids`]
//! recomputes them from the arena and the journal. There is no second source
//! of truth to invalidate and no way for a stale id to outlive the geometry
//! it named. The cost is one `O(E log E)` pass per query; callers that want
//! every id at once ask for [`solid_pids`] directly.
//!
//! A FACE id is stored, in `BrepArena::face_pids`, as it has been since
//! KV13 F1 — the arena is where a stamp can be read back without
//! re-executing the constructor that chose it. D0 item 1 added one more
//! arena field, `BrepArena::face_seed`, the seed the current construct step
//! stamps from; it is part of the canonical state the determinism oracle
//! compares, so two runs that install the same seeds in the same order still
//! compare equal.

use std::collections::{BTreeMap, BTreeSet};

use crate::arena::{
    BrepArena, FaceId, FaceSeed, HalfEdgeId, Pid, SolidId, VertexId, PID_CONTENT_BASE,
};
use crate::error::KernelV2Error;
use crate::journal::face_lineage;
use cad_primitives::Point3;

/// Domain tag mixed into every edge pid, so an edge and a vertex with
/// numerically equal keys cannot land on the same id.
const DOMAIN_EDGE: u64 = 0x4544_4745_5f56_3100; // "EDGE_V1"
/// Domain tag mixed into every vertex pid.
const DOMAIN_VERTEX: u64 = 0x5645_5254_5f56_3100; // "VERT_V1"
/// Domain tag mixed into every content-seeded FACE pid (D0 item 1).
const DOMAIN_FACE: u64 = 0x4641_4345_5f56_3100; // "FACE_V1"

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

/// The content-seeded persistent id of a face created by a construct step
/// (D0 item 1, the F4a reseed).
///
/// `seed` is the step's stable 128-bit name, `output` the ordinal of the
/// stamping pass within that step (one step can build several solids), and
/// `role` the face's position in its solid's own face list — for an extrude,
/// base cap 0, top cap 1, then one lateral per profile edge. None of the
/// three reads anything global, so an INCREMENTAL rebuild of one step
/// reproduces exactly the ids a full rebuild would, which the monotonic
/// counter could not. See [`BrepArena::assign_face_pids`].
///
/// The result always has its top bit set ([`PID_CONTENT_BASE`]), keeping the
/// content ids and the counter's ids in disjoint halves of the number space.
/// Same frozen-format obligation as [`digest`]: these ids are persisted in
/// documents, so this function must never drift.
pub fn seeded_face_pid(seed: FaceSeed, output: u64, role: u64) -> Pid {
    let h = digest(DOMAIN_FACE, &[seed.origin[0], seed.origin[1], output, role]);
    Pid(h.0 | PID_CONTENT_BASE)
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
/// Exact — no quantization — so the order is a function of the bits alone and
/// a re-run that reproduces the geometry reproduces the order. Being exact
/// cuts both ways: `-0.0` sorts below `+0.0` and a NaN sorts outside the
/// finite range, both of which are *reorders* rather than errors. That only
/// reaches an id through a multi-member rank group (see [`rank_groups`]).
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

/// Stamp one id per item from its key words, refusing a 64-bit hash collision
/// between two DISTINCT keys.
///
/// `hash` is a parameter only so that refusal is reachable from a test: a
/// genuine collision of this digest is not constructible, and a branch no
/// test can enter is a branch nobody knows works. Callers pass
/// `|w| digest(DOMAIN_…, w)`.
///
/// Keys are distinct by construction — two items with equal keys are an
/// ambiguity [`rank_groups`] has already refused — so a repeated id can only
/// be a hash collision.
fn stamp<E: Copy + Ord>(
    keyed: &[(E, Vec<u64>)],
    hash: impl Fn(&[u64]) -> Pid,
    kind: &'static str,
) -> Result<BTreeMap<E, Pid>, KernelV2Error> {
    let mut out: BTreeMap<E, Pid> = BTreeMap::new();
    let mut seen: BTreeSet<Pid> = BTreeSet::new();
    for (e, words) in keyed {
        let pid = hash(words);
        if !seen.insert(pid) {
            return Err(KernelV2Error::PidCollision { kind });
        }
        out.insert(*e, pid);
    }
    Ok(out)
}

/// Rank the members of each equal-key group, loudly refusing a group whose
/// members cannot be told apart.
///
/// `items` is `(entity, content_key, tie_key)`. Returns the rank of each
/// entity inside its `content_key` group, with members ordered by `tie_key`.
///
/// **The rank is positional, so it is only as stable as the group's internal
/// order.** A group of one — the overwhelming majority — ranks 0 whatever its
/// geometry does. In a group of two or more, moving ONE member past another
/// renumbers both, and the two ids swap: within such a group an id is stable
/// only while the members' relative order is. (Sign-of-zero counts: `-0.0`
/// orders below `+0.0` under [`point_key`], as it does under `f64::total_cmp`,
/// so a coordinate that comes out `-0.0` in one build and `+0.0` in another is
/// a reorder even though the point did not move.) Making multi-member groups
/// order-independent needs the content key itself to separate them, which the
/// D0 item 1 face reseed does NOT do: it stabilizes a face's ROOT, but a
/// boolean that splits one operand face into two patches still leaves both
/// patches rooted at that face, so their edges still share a root pair and
/// still need a rank. Separating them wants a per-patch discriminator inside
/// the root — see the "Still open" notes in `specs/drawings_and_mbd.md` §4.
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

/// What [`solid_face_pids`] returns: each face's stamped pid, and each face's
/// lineage root.
pub type FacePids = (BTreeMap<FaceId, Pid>, BTreeMap<FaceId, Pid>);

/// The face half of [`solid_pids`]: each face's stamped pid and its lineage
/// root, with no edge or vertex derivation.
///
/// Separate because a face's identity must not depend on the edge pass. Face
/// pids have been the kernel's answer since KV13 F5 and are what
/// `face_provenance` reports per face; folding them into the whole-solid
/// derivation would let one ambiguous EDGE group ([`KernelV2Error::PidAmbiguous`])
/// silently withdraw every FACE id of the body, so a drawing's face
/// annotations would go unresolvable because of a quirk two kinds away.
///
/// Returns `(pid per face, root per face)`. Errors with
/// [`KernelV2Error::PidMissing`] if any face of the solid is unstamped.
pub fn solid_face_pids(arena: &BrepArena, solid: SolidId) -> Result<FacePids, KernelV2Error> {
    let mut face_pids: BTreeMap<FaceId, Pid> = BTreeMap::new();
    let mut face_roots: BTreeMap<FaceId, Pid> = BTreeMap::new();
    for f in solid_faces(arena, solid)? {
        let pid = arena
            .face_pid(f)
            .ok_or(KernelV2Error::PidMissing { face: f })?;
        face_pids.insert(f, pid);
        face_roots.insert(f, face_lineage(&arena.journal, pid).root);
    }
    Ok((face_pids, face_roots))
}

/// Every persistent id of `solid`: the stamped face pids, their lineage
/// roots, and the content-seeded edge and vertex pids.
///
/// Ask [`solid_face_pids`] instead when only faces are wanted — this function
/// refuses as a whole, so an edge-side refusal costs the face ids too.
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
    let (face_pids, face_roots) = solid_face_pids(arena, solid)?;

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
    let mut edge_keys: Vec<(HalfEdgeId, Vec<u64>)> = Vec::with_capacity(edge_items.len());
    for (e, pair, _) in &edge_items {
        let rank = *edge_ranks
            .get(e)
            .ok_or(KernelV2Error::InvalidId { kind: "half_edge" })?;
        edge_keys.push((*e, vec![pair[0], pair[1], rank]));
    }
    let edges = stamp(&edge_keys, |w| digest(DOMAIN_EDGE, w), "edge")?;

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
    let mut vertex_keys: Vec<(VertexId, Vec<u64>)> = Vec::with_capacity(vertex_items.len());
    for (v, key, _) in &vertex_items {
        let mut words = key.clone();
        words.push(
            *vertex_ranks
                .get(v)
                .ok_or(KernelV2Error::InvalidId { kind: "vertex" })?,
        );
        vertex_keys.push((*v, words));
    }
    let vertices = stamp(&vertex_keys, |w| digest(DOMAIN_VERTEX, w), "vertex")?;

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

    /// A group of two whose members differ only in tie key gets ranks 0/1 by
    /// that key — so if one member later moves past the other, the two ids
    /// swap. Pinned because it is the scheme's one stability caveat and a
    /// reader should be able to see it rather than infer it.
    #[test]
    fn rank_groups_renumbers_a_group_when_a_member_moves_past_another() {
        let before = rank_groups(&[(10u32, "a", 1u32), (11, "a", 3)], "edge").expect("ranked");
        let after = rank_groups(&[(10u32, "a", 5u32), (11, "a", 3)], "edge").expect("ranked");
        assert_eq!((before[&10], before[&11]), (0, 1));
        assert_eq!(
            (after[&10], after[&11]),
            (1, 0),
            "moving one member past the other swaps both ranks"
        );
    }

    #[test]
    fn stamp_gives_one_id_per_key() {
        let keyed = vec![(10u32, vec![1, 2, 0]), (11, vec![1, 2, 1])];
        let ids = stamp(&keyed, |w| digest(DOMAIN_EDGE, w), "edge").expect("distinct");
        assert_eq!(ids[&10], digest(DOMAIN_EDGE, &[1, 2, 0]));
        assert_ne!(ids[&10], ids[&11]);
    }

    /// `PidCollision` is not reachable through the real digest — that is the
    /// point of a 64-bit avalanche hash — so the branch is exercised with a
    /// degenerate one. Without this the refusal would be code nobody has
    /// ever seen run.
    #[test]
    fn stamp_refuses_two_distinct_keys_that_hash_alike() {
        let keyed = vec![(10u32, vec![1, 2, 0]), (11, vec![9, 9, 9])];
        assert_eq!(
            stamp(&keyed, |_| Pid(42), "vertex"),
            Err(KernelV2Error::PidCollision { kind: "vertex" })
        );
    }

    /// The public single-entity doors agree with the bulk map, and the
    /// owner-lookup helpers find the solid each entity belongs to.
    #[test]
    fn single_entity_doors_agree_with_the_bulk_map() {
        use crate::{extrude, Profile};
        use cad_primitives::{Point2, Vector3};

        let mut arena = BrepArena::new();
        let profile = Profile::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ],
            vec![],
        )
        .expect("rectangle");
        let r = extrude(&mut arena, &profile, Vector3::new(0.0, 0.0, 1.0), 2.0).expect("box");
        let pids = solid_pids(&arena, r.solid).expect("solid pids");

        for (&h, &pid) in &pids.edges {
            assert_eq!(edge_pid(&arena, h).expect("edge pid"), pid);
            let twin = arena.half_edge(h).expect("half-edge").twin;
            assert_eq!(
                edge_pid(&arena, twin).expect("twin pid"),
                pid,
                "either half of the pair names the same edge"
            );
            assert_eq!(canonical_edge(&arena, twin).expect("canonical"), h);
            assert_eq!(solid_of_half_edge(&arena, h).expect("owner solid"), r.solid);
        }
        for (&v, &pid) in &pids.vertices {
            assert_eq!(vertex_pid(&arena, v).expect("vertex pid"), pid);
            assert_eq!(solid_of_vertex(&arena, v).expect("owner solid"), r.solid);
        }
        for &f in pids.faces.keys() {
            assert_eq!(solid_of_face(&arena, f).expect("owner solid"), r.solid);
        }

        // The face half on its own must agree with the whole-solid pass.
        let (faces, roots) = solid_face_pids(&arena, r.solid).expect("face pids");
        assert_eq!(
            (faces, roots),
            (pids.faces.clone(), pids.face_roots.clone())
        );
    }
}
