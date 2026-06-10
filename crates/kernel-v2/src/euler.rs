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

use crate::arena::{BrepArena, FaceId, HalfEdgeId, LoopId, ShellId, SolidId, VertexId};
use crate::error::KernelV2Error;
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

/// Make Vertex, Face, Solid — Stroud 2006 §F.8 ("the basic Eulerian object
/// creation operator"; his MBFV / "make a vertex, a face, and a new object").
///
/// Creates a new solid containing one shell, one face, one outer loop holding
/// the single vertex at `p`, and the vertex itself. The face's surface is
/// `None` — a lone vertex has no Newell orientation.
///
/// Counts: V+1, F+1, S+1 ⇒ Euler–Poincaré holds (1 − 0 + 1 − 0 = 2(1 − 0)).
pub fn mvfs(_arena: &mut BrepArena, _p: Point3) -> Result<MvfsResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("mvfs"))
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
    _arena: &mut BrepArena,
    _loop_id: LoopId,
    _p: Point3,
) -> Result<MevResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("mev_lone"))
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
    _arena: &mut BrepArena,
    _anchor: HalfEdgeId,
    _p: Point3,
) -> Result<MevResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("mev"))
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
    _arena: &mut BrepArena,
    _he_from: HalfEdgeId,
    _he_to: HalfEdgeId,
) -> Result<MefResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("mef"))
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
pub fn kemr(_arena: &mut BrepArena, _he: HalfEdgeId) -> Result<KemrResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("kemr"))
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
    _arena: &mut BrepArena,
    _face_kill: FaceId,
    _face_recv: FaceId,
) -> Result<LoopId, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("kfmrh"))
}
