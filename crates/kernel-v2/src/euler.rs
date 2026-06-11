//! Euler operators for the half-edge B-Rep arena.
//!
//! The set implemented here is exactly sufficient for building closed planar
//! prismatic solids (box / prism from profile loops — the KV2 extrude
//! consumer) and through-holes:
//!
//! | Operator | Δ(V,E,F,R,G,S) | Stroud 2006 reference |
//! |----------|----------------|------------------------|
//! | `mvfs`   | (+1, 0,+1, 0, 0,+1) | §F.8 "Make a vertex, a face, and a new object" (Table F.1 #79 `mbfv`) |
//! | `mev` / `mev_lone` | (+1,+1, 0, 0, 0, 0) | §F.1 "Make edge vertex (spur vertex and edge)" (Table F.1 #51 `mev`) |
//! | `mef`    | ( 0,+1,+1, 0, 0, 0) | §F.4 "Make an edge and a face" (Table F.1 #53 `mfe`) |
//! | `kemr`   | ( 0,−1, 0,+1, 0, 0) | inverse of §F.6 "Make an edge and kill a hole-loop" (Table F.1 #55 `mhke`) |
//! | `kfmrh`  | ( 0, 0,−1,+1,+1, 0) | §F.9 "Make a hole-loop and kill a face", same-shell (genus-increasing) case (Table F.1 #65 `mhgkf`) |
//!
//! `mvfs`, `mev`, and `mef` together realize Stroud's spanning-set
//! decomposition (§4.1): a cube is 1 MBFV + 7 MEV + 5 MFE; `kemr`/`kfmrh`
//! add the ring and genus axes needed for holes. The edge-splitting MEV
//! variants (§F.2/§F.3, Mäntylä's `semv`) are **not** implemented: no KV1/KV2
//! construction sequence needs them (prisms are built entirely from spur
//! `mev` + `mef`), and the crate mandate is exactly-sufficient, nothing
//! speculative.
//!
//! ## Contracts common to all operators
//!
//! - **Atomic**: all preconditions are checked before any mutation; on `Err`
//!   the arena is unmodified.
//! - **Newell invariant at exit** (crate hard rule 2): the surface plane of
//!   every face whose outer loop changed is recomputed from the loop walk
//!   (normalized Newell normal — the walk IS the source of truth), and a
//!   `debug_assert!` re-verifies the whole-arena invariant before returning.
//!   Because the stored normal is *derived from* the walk, an operator can
//!   only fail the invariant by producing a face that has no orientation at
//!   all — which `mef` rejects with `Err(DegenerateFaceNormal)` (the
//!   constructor-`Err` arm of hard rule 2). A Newell *mismatch* is therefore
//!   impossible by construction; the `debug_assert` is defense in depth.
//! - **2-manifoldness** (crate hard rule 3): preconditions reject the
//!   applications that would break it (e.g. `kemr` on an edge whose twin
//!   lies in a different loop — killing it would merge faces, not make a
//!   ring) with `Err(NonManifoldTopology)`. No silent repair.
//! - **Euler–Poincaré bookkeeping**: `debug_assert`ed at exit for the
//!   affected solid (`V − E + F − R = 2(S − G)`, Stroud §4 rule 4).

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::Point3;

/// Entities created by [`mvfs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MvfsResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// Its single face (surface `None`: a lone-vertex loop has no
    /// orientation yet).
    pub face: FaceId,
    /// The face's outer loop (`LoopBoundary::Lone(vertex)`).
    pub outer_loop: LoopId,
    /// The seed vertex.
    pub vertex: VertexId,
}

/// Entities created by [`mev`] / [`mev_lone`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MevResult {
    /// The new (leaf) vertex.
    pub vertex: VertexId,
    /// Half-edge from the base vertex to the new vertex.
    pub he_out: HalfEdgeId,
    /// Half-edge from the new vertex back to the base vertex
    /// (`twin(he_out)`).
    pub he_in: HalfEdgeId,
}

/// Entities created by [`mef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MefResult {
    /// The new face.
    pub face: FaceId,
    /// The new face's outer loop.
    pub new_loop: LoopId,
    /// New half-edge `origin(he_from) → origin(he_to)`; stays in the **old**
    /// loop (becomes its representative).
    pub he_old_side: HalfEdgeId,
    /// New half-edge `origin(he_to) → origin(he_from)`; lies in the **new**
    /// loop (its representative).
    pub he_new_side: HalfEdgeId,
}

/// Entities created by [`kemr`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KemrResult {
    /// The new inner loop (ring) of the face.
    pub ring: LoopId,
}

// ---------------------------------------------------------------------------
// Private allocation / maintenance helpers
// ---------------------------------------------------------------------------

fn push_vertex(arena: &mut BrepArena, p: Point3) -> VertexId {
    arena.vertices.push(Some(Vertex { point: p }));
    VertexId((arena.vertices.len() - 1) as u32)
}

/// Recompute a face's surface plane from its outer-loop walk. The walk
/// direction is the source of truth (hard rule 5): an orientable loop yields
/// `Some(Plane)` with `normal = normalize(Newell)`, a degenerate loop yields
/// `None` ("under construction").
fn refresh_face_plane(arena: &mut BrepArena, face_id: FaceId) -> Result<(), KernelV2Error> {
    let outer = arena.face(face_id)?.outer_loop;
    let pts = arena.loop_points(outer)?;
    let surface = geom::newell_unit(&pts).map(|normal| {
        Surface::Plane(Plane {
            point: pts[0],
            normal,
        })
    });
    arena.face_mut(face_id)?.surface = surface;
    Ok(())
}

/// Walk `start → … → stop` (exclusive of `stop`) along `next`, bounded by
/// the live half-edge count. Both must be in the same loop (checked by the
/// callers); the bound makes a corrupted arena fail loudly instead of
/// looping.
fn walk_until(
    arena: &BrepArena,
    start: HalfEdgeId,
    stop: HalfEdgeId,
) -> Result<Vec<HalfEdgeId>, KernelV2Error> {
    let budget = arena.num_half_edges();
    let mut out = Vec::new();
    let mut cur = start;
    while cur != stop {
        out.push(cur);
        cur = arena.half_edge(cur)?.next;
        if out.len() > budget {
            return Err(KernelV2Error::LoopNotClosed {
                loop_id: arena.half_edge(start)?.loop_id,
            });
        }
    }
    Ok(out)
}

/// Whole-arena invariant re-verification at operator exits (debug builds
/// only — defense in depth behind the constructive guarantees).
macro_rules! debug_assert_invariants {
    ($arena:expr) => {
        debug_assert_eq!(
            crate::validate::debug_check_arena($arena),
            Ok(()),
            "Euler operator exit invariant violated"
        );
    };
}

/// Make Vertex, Face, Solid — Stroud 2006 §F.8 ("the basic Eulerian object
/// creation operator"; his MBFV / "make a vertex, a face, and a new object").
///
/// Creates a new solid containing one shell, one face, one outer loop holding
/// the single vertex at `p`, and the vertex itself. The face's surface is
/// `None` — a lone vertex has no Newell orientation.
///
/// Counts: V+1, F+1, S+1 ⇒ Euler–Poincaré holds (1 − 0 + 1 − 0 = 2(1 − 0)).
pub fn mvfs(arena: &mut BrepArena, p: Point3) -> Result<MvfsResult, KernelV2Error> {
    let vertex = push_vertex(arena, p);

    let solid = SolidId(arena.solids.len() as u32);
    let shell = ShellId(arena.shells.len() as u32);
    let face = FaceId(arena.faces.len() as u32);
    let outer_loop = LoopId(arena.loops.len() as u32);

    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));
    arena.shells.push(Some(Shell {
        solid,
        faces: vec![face],
        genus: 0,
    }));
    arena.faces.push(Some(Face {
        surface: None,
        outer_loop,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.loops.push(Some(Loop {
        face,
        boundary: LoopBoundary::Lone(vertex),
        kind: LoopKind::Outer,
    }));

    debug_assert_invariants!(arena);
    Ok(MvfsResult {
        solid,
        shell,
        face,
        outer_loop,
        vertex,
    })
}

/// Make Edge, Vertex — first-edge case: the loop is still a lone-vertex loop
/// (straight after [`mvfs`]). Stroud 2006 §F.1, base-vertex case 1 ("the
/// base vertex has no edges attached").
///
/// Creates a new vertex at `p` and the edge `lone_vertex → new_vertex`. The
/// loop becomes a two-half-edge cycle `out → in → out`.
///
/// Errors: `InvalidId` (dead loop), `LoopNotLone` (the loop already has
/// edges — use [`mev`] with a half-edge anchor instead).
pub fn mev_lone(
    arena: &mut BrepArena,
    loop_id: LoopId,
    p: Point3,
) -> Result<MevResult, KernelV2Error> {
    let lp = arena.loop_(loop_id)?;
    let LoopBoundary::Lone(base) = lp.boundary else {
        return Err(KernelV2Error::LoopNotLone);
    };
    let face_id = lp.face;

    let vertex = push_vertex(arena, p);
    let he_out = HalfEdgeId(arena.half_edges.len() as u32);
    let he_in = HalfEdgeId(arena.half_edges.len() as u32 + 1);
    arena.half_edges.push(Some(HalfEdge {
        twin: he_in,
        next: he_in,
        prev: he_in,
        origin: base,
        loop_id,
        curve: Curve::LineSegment,
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: he_out,
        next: he_out,
        prev: he_out,
        origin: vertex,
        loop_id,
        curve: Curve::LineSegment,
    }));
    arena.loop_mut(loop_id)?.boundary = LoopBoundary::Edges(he_out);

    refresh_face_plane(arena, face_id)?;
    debug_assert_invariants!(arena);
    Ok(MevResult {
        vertex,
        he_out,
        he_in,
    })
}

/// Make Edge, Vertex — spur edge at an existing vertex. Stroud 2006 §F.1
/// ("Make edge vertex (spur vertex and edge)"; Table F.1 #51).
///
/// Creates a new vertex at `p` and a spur edge from `origin(anchor)` to it,
/// inserted into the anchor's loop **immediately before `anchor`**:
///
/// ```text
/// before:  … → prev(anchor) → anchor → …
/// after:   … → prev(anchor) → he_out → he_in → anchor → …
///                              (base→new)  (new→base)
/// ```
///
/// The anchor half-edge is exactly Stroud's "orientation information at the
/// base vertex" — a bare vertex would be ambiguous once it has more than one
/// incident edge. The new vertex is a leaf (degree 1), so vertex
/// manifoldness is preserved trivially.
///
/// Errors: `InvalidId` (dead anchor).
pub fn mev(
    arena: &mut BrepArena,
    anchor: HalfEdgeId,
    p: Point3,
) -> Result<MevResult, KernelV2Error> {
    let anchor_he = *arena.half_edge(anchor)?;
    let loop_id = anchor_he.loop_id;
    let face_id = arena.loop_(loop_id)?.face;
    let a = anchor_he.prev;

    let vertex = push_vertex(arena, p);
    let he_out = HalfEdgeId(arena.half_edges.len() as u32);
    let he_in = HalfEdgeId(arena.half_edges.len() as u32 + 1);
    arena.half_edges.push(Some(HalfEdge {
        twin: he_in,
        next: he_in,
        prev: a,
        origin: anchor_he.origin,
        loop_id,
        curve: Curve::LineSegment,
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: he_out,
        next: anchor,
        prev: he_out,
        origin: vertex,
        loop_id,
        curve: Curve::LineSegment,
    }));
    arena.half_edge_mut(a)?.next = he_out;
    arena.half_edge_mut(anchor)?.prev = he_in;

    refresh_face_plane(arena, face_id)?;
    debug_assert_invariants!(arena);
    Ok(MevResult {
        vertex,
        he_out,
        he_in,
    })
}

/// Make Edge, Face — Stroud 2006 §F.4 ("Make an edge and a face";
/// Table F.1 #53 `mfe`).
///
/// `he_from` and `he_to` must lie in the **same loop** (Stroud §F.4: vertices
/// in the same loop ⇒ MEF; different loops would be MEKH/MEKFB territory ⇒
/// `Err(MefDifferentLoops)` — this is the "mef across non-cofacial vertices"
/// error arm). A new edge is created from `origin(he_from)` to
/// `origin(he_to)`, splitting the loop in two:
///
/// ```text
/// old loop keeps:  … → prev(he_from) → he_old_side → he_to → …
/// new loop:        he_from → … → prev(he_to) → he_new_side → (he_from)
/// ```
///
/// The **new face** owns the cycle containing `he_from`; its plane is set
/// from the new loop's Newell normal (walk direction is the source of
/// truth). If that normal is numerically zero the operation is rejected
/// with `Err(DegenerateFaceNormal)` *before* mutating (hard rule 2's
/// constructor-`Err` arm). The old face's plane is recomputed from its
/// shortened loop.
///
/// Errors: `InvalidId`, `MefDifferentLoops`, `DegenerateEdge`
/// (`he_from == he_to`, or both origins equal), `DegenerateFaceNormal`.
pub fn mef(
    arena: &mut BrepArena,
    he_from: HalfEdgeId,
    he_to: HalfEdgeId,
) -> Result<MefResult, KernelV2Error> {
    let hf = *arena.half_edge(he_from)?;
    let ht = *arena.half_edge(he_to)?;
    if he_from == he_to {
        return Err(KernelV2Error::DegenerateEdge);
    }
    if hf.loop_id != ht.loop_id {
        return Err(KernelV2Error::MefDifferentLoops);
    }
    if hf.origin == ht.origin {
        return Err(KernelV2Error::DegenerateEdge);
    }
    let old_loop = hf.loop_id;
    let old_face = arena.loop_(old_loop)?.face;
    let shell = arena.face(old_face)?.shell;

    // Prospective new-loop cycle: he_from … prev(he_to), then the new
    // closing half-edge origin(he_to) → origin(he_from). Reject an
    // unorientable new face BEFORE mutating (atomicity).
    let seg = walk_until(arena, he_from, he_to)?;
    let mut new_pts = Vec::with_capacity(seg.len() + 1);
    for &h in &seg {
        new_pts.push(arena.vertex(arena.half_edge(h)?.origin)?.point);
    }
    new_pts.push(arena.vertex(ht.origin)?.point);
    let Some(new_normal) = geom::newell_unit(&new_pts) else {
        return Err(KernelV2Error::DegenerateFaceNormal);
    };

    // Allocate the closing half-edge pair and the new loop/face.
    let a = hf.prev;
    let b = ht.prev;
    let he_old_side = HalfEdgeId(arena.half_edges.len() as u32);
    let he_new_side = HalfEdgeId(arena.half_edges.len() as u32 + 1);
    let new_loop = LoopId(arena.loops.len() as u32);
    let new_face = FaceId(arena.faces.len() as u32);

    arena.half_edges.push(Some(HalfEdge {
        twin: he_new_side,
        next: he_to,
        prev: a,
        origin: hf.origin,
        loop_id: old_loop,
        curve: Curve::LineSegment,
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: he_old_side,
        next: he_from,
        prev: b,
        origin: ht.origin,
        loop_id: new_loop,
        curve: Curve::LineSegment,
    }));
    arena.half_edge_mut(a)?.next = he_old_side;
    arena.half_edge_mut(he_to)?.prev = he_old_side;
    arena.half_edge_mut(b)?.next = he_new_side;
    arena.half_edge_mut(he_from)?.prev = he_new_side;

    arena.loops.push(Some(Loop {
        face: new_face,
        boundary: LoopBoundary::Edges(he_new_side),
        kind: LoopKind::Outer,
    }));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: new_pts[0],
            normal: new_normal,
        })),
        outer_loop: new_loop,
        inner_loops: Vec::new(),
        shell,
    }));
    arena.shell_mut(shell)?.faces.push(new_face);

    // Move the split-off cycle into the new loop; reset the old loop's
    // representative to the new edge (Stroud §F.4: "simplest if the pointers
    // are reset to refer to the new edge").
    for &h in &seg {
        arena.half_edge_mut(h)?.loop_id = new_loop;
    }
    arena.loop_mut(old_loop)?.boundary = LoopBoundary::Edges(he_old_side);

    refresh_face_plane(arena, old_face)?;
    debug_assert_invariants!(arena);
    Ok(MefResult {
        face: new_face,
        new_loop,
        he_old_side,
        he_new_side,
    })
}

/// Kill Edge, Make Ring — inverse of Stroud 2006 §F.6 ("Make an edge and
/// kill a hole-loop"; the killed direction is Table F.1 #55 `mhke`).
///
/// `he` and `twin(he)` must lie in the **same loop** — that is precisely the
/// configuration where deleting the edge splits one loop into two cycles in
/// the same face. (If the twin lies in a different loop, deleting the edge
/// would merge two faces across it — a different operator (KEF) and, applied
/// here, a 2-manifoldness contract violation ⇒ `Err(NonManifoldTopology)`.)
///
/// Deleting the pair leaves two cycles:
/// - the cycle containing `next(twin(he))` **remains the original loop**;
/// - the cycle containing `next(he)` becomes the new **inner loop (ring)**
///   of the same face. If `next(he) == twin(he)` the spur tip becomes a
///   lone-vertex ring (Stroud §F.10's vertex hole-loop).
///
/// Counts: E−1, R+1.
///
/// Errors: `InvalidId`, `NonManifoldTopology`.
pub fn kemr(arena: &mut BrepArena, he: HalfEdgeId) -> Result<KemrResult, KernelV2Error> {
    let h = *arena.half_edge(he)?;
    let t_id = h.twin;
    let t = *arena.half_edge(t_id)?;
    if h.loop_id != t.loop_id {
        return Err(KernelV2Error::NonManifoldTopology(
            "kemr: twin lies in a different loop — killing this edge would merge two faces \
             (KEF), not split a loop into a ring",
        ));
    }
    let loop_id = h.loop_id;
    let face_id = arena.loop_(loop_id)?.face;

    // Cycle membership before any mutation.
    // Ring cycle: next(he) … prev(twin); empty if next(he) == twin.
    let ring_members = if h.next == t_id {
        Vec::new()
    } else {
        walk_until(arena, h.next, t_id)?
    };
    // Remaining cycle: next(twin) … prev(he); empty if next(twin) == he.
    let remaining_first = if t.next == he { None } else { Some(t.next) };

    // --- mutate ---
    let ring = LoopId(arena.loops.len() as u32);

    // Close the ring cycle.
    let ring_boundary =
        if let (Some(&first), Some(&last)) = (ring_members.first(), ring_members.last()) {
            arena.half_edge_mut(last)?.next = first;
            arena.half_edge_mut(first)?.prev = last;
            for &m in &ring_members {
                arena.half_edge_mut(m)?.loop_id = ring;
            }
            LoopBoundary::Edges(first)
        } else {
            // Spur tip becomes a lone-vertex ring.
            LoopBoundary::Lone(t.origin)
        };

    // Close the remaining cycle (the original loop).
    let old_boundary = if let Some(first) = remaining_first {
        let last = h.prev; // ends at origin(he)
        arena.half_edge_mut(last)?.next = first;
        arena.half_edge_mut(first)?.prev = last;
        LoopBoundary::Edges(first)
    } else {
        LoopBoundary::Lone(h.origin)
    };
    arena.loop_mut(loop_id)?.boundary = old_boundary;

    arena.loops.push(Some(Loop {
        face: face_id,
        boundary: ring_boundary,
        kind: LoopKind::Inner,
    }));
    arena.face_mut(face_id)?.inner_loops.push(ring);

    // Kill the edge.
    arena.half_edges[he.index()] = None;
    arena.half_edges[t_id.index()] = None;

    refresh_face_plane(arena, face_id)?;
    debug_assert_invariants!(arena);
    Ok(KemrResult { ring })
}

/// Kill Face, Make Ring-Hole — Stroud 2006 §F.9 ("Make a hole-loop and kill
/// a face"), restricted to the **same-shell** case, which "increases the
/// genus and no [shell-merge] work need be done" (Table F.1 #65 `mhgkf`).
///
/// Kills `face_kill` and transfers its outer loop to `face_recv` as an inner
/// loop (ring). The loop's half-edges are untouched — in a valid
/// through-hole configuration the killed face's outward normal opposes the
/// receiving face's, so the transferred loop already winds opposite to
/// `face_recv`'s outer loop, as a ring must. Shell genus increments.
///
/// Counts: F−1, R+1, G+1.
///
/// Errors: `InvalidId`, `KfmrhSameFace`, `KfmrhFaceHasRings` (Stroud §F.9
/// treats hole-loops in the killed face as "an error condition"),
/// `KfmrhDifferentShells` (shell-merge / object-merge interpretations are
/// out of KV1 scope).
pub fn kfmrh(
    arena: &mut BrepArena,
    face_kill: FaceId,
    face_recv: FaceId,
) -> Result<LoopId, KernelV2Error> {
    let fk = arena.face(face_kill)?.clone();
    let fr_shell = arena.face(face_recv)?.shell;
    if face_kill == face_recv {
        return Err(KernelV2Error::KfmrhSameFace);
    }
    if !fk.inner_loops.is_empty() {
        return Err(KernelV2Error::KfmrhFaceHasRings);
    }
    if fk.shell != fr_shell {
        return Err(KernelV2Error::KfmrhDifferentShells);
    }

    // Transfer the loop (half-edges keep their loop_id — the loop itself is
    // re-parented).
    let ring = fk.outer_loop;
    {
        let lp = arena.loop_mut(ring)?;
        lp.face = face_recv;
        lp.kind = LoopKind::Inner;
    }
    arena.face_mut(face_recv)?.inner_loops.push(ring);

    let shell = arena.shell_mut(fk.shell)?;
    shell.faces.retain(|&f| f != face_kill);
    shell.genus += 1;
    arena.faces[face_kill.index()] = None;

    debug_assert_invariants!(arena);
    Ok(ring)
}
