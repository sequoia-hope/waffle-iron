//! Boolean delegation to yang-rs (PR-KV3, Phase 4a).
//!
//! kernel-v2 does NOT implement boolean labeling — that is `cherchi-rs` via
//! `yang-rs` (crate hard rule: this crate's only kernel-rewrite deps are
//! `cad-primitives` and `yang-rs`). This module is the **boundary**: it
//! converts a kernel-v2 solid to yang-rs's own `BRep` input type, invokes
//! `yang_rs::boolean` with the production native backend, and reassembles
//! yang-rs's output `BRep` into a kernel-v2 solid that passes the full
//! [`crate::validate::validate_solid`] invariant set. Per yang-rs's scope
//! rules the conversion lives HERE, on the kernel-v2 side of the boundary —
//! yang-rs never imports kernel-v2's types.
//!
//! ## The three conversion functions
//!
//! - [`to_yang_brep`] — kernel-v2 arena solid → `yang_rs::BRep` (planar
//!   faces, outer + inner loops, per-face directed edges).
//! - [`from_yang_brep`] — yang-rs *output* `BRep` → kernel-v2 solid.
//!   Planar output only in Phase 4a: a non-planar output surface is a
//!   typed, loud [`KernelV2Error::UnsupportedBooleanOutputSurface`].
//! - [`boolean_op`] — the composition: convert both inputs, run
//!   `yang_rs::boolean(a, b, op, native_backend)`, reassemble the output.
//!
//! ## Reassembly strategy: direct arena assembler (documented decision)
//!
//! `from_yang_brep` rebuilds topology with a **direct arena assembler**
//! (validated afterward by `validate_solid`), not an Euler-operator
//! sequence. Rationale: yang-rs output is a *face soup with arbitrary
//! (already-final) topology* — possibly holed faces (through-cut results),
//! genus > 0, even multiple disjoint closed shells. Deriving an Euler
//! construction schedule (which `mev` chain, where to `kemr`, which face
//! pairs to `kfmrh`) from such a soup is a graph-search problem that adds
//! no safety: the safety comes from the invariant CHECK, not from the
//! mutation path. The assembler validates the yang structure exhaustively
//! BEFORE the first arena mutation (same atomicity contract as the KV2
//! constructors), assembles, then runs the full production
//! `validate_solid` (twin pairing, loop closure, vertex fans, Newell,
//! ring winding, Euler–Poincaré) as defense in depth — the result meets
//! exactly the same bar as an Euler-built solid.
//!
//! ## Error mapping (loud, typed)
//!
//! - Coplanar input face pairs (touching or overlapping on a shared
//!   plane) are deferred by the cherchi-rs arrangement
//!   (`ArrangementError::CoplanarPairDeferred`) — Yang Stage 0 coplanar
//!   preprocessing is the M8 roadmap milestone and is NOT yet implemented.
//!   These surface as the typed [`KernelV2Error::UnsupportedCoplanar`].
//! - Every other yang-rs failure surfaces as
//!   [`KernelV2Error::BooleanFailed`] carrying the yang error's full
//!   Display text. No masking, no retry, no tolerance fallback (P9/P10).
//! - An empty result (e.g. intersection of disjoint solids) is the typed
//!   [`KernelV2Error::EmptyBooleanResult`] — kernel-v2 has no empty solid.

use crate::arena::{BrepArena, SolidId};
use crate::error::KernelV2Error;
use cad_primitives::BoolOp;

/// Convert a kernel-v2 arena solid into yang-rs's `BRep` input type.
///
/// - Every face must carry `Some(Surface::Plane)` (the only KV surface so
///   far); the yang plane is `n·x + d = 0` with `n` the face's stored
///   (Newell-derived, outward) unit normal and `d = −n·p` for the loop's
///   first vertex `p`.
/// - Loops convert in walk order: kernel-v2's outer loops are CCW viewed
///   from outside (Newell ≡ normal) and rings wind opposite — exactly
///   yang-rs's `outer_loop` / `inner_loops` conventions.
/// - Edges are emitted per loop (per-face ownership, `Curve::LineSegment`),
///   matching yang-rs's own construction pattern; yang-rs never requires
///   shared undirected edge records.
pub fn to_yang_brep(_arena: &BrepArena, _solid: SolidId) -> Result<yang_rs::BRep, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("to_yang_brep"))
}

/// Reassemble a yang-rs *output* `BRep` into a kernel-v2 solid.
///
/// Phase 4a scope: planar faces only — any other output surface is a
/// typed [`KernelV2Error::UnsupportedBooleanOutputSurface`]. The output is
/// validated structurally BEFORE the first arena mutation (loop continuity
/// and closure, exactly-two opposite directed half-edges per undirected
/// edge, orientable Newell normals agreeing with yang's stated planes),
/// assembled directly into the arena (see module docs for why a direct
/// assembler rather than an Euler sequence), split into connected shells
/// with per-shell genus derived from the Euler–Poincaré formula, and then
/// re-checked by the full [`crate::validate::validate_solid`].
pub fn from_yang_brep(
    _arena: &mut BrepArena,
    _brep: &yang_rs::BRep,
) -> Result<SolidId, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("from_yang_brep"))
}

/// Boolean operation on two kernel-v2 solids via the yang-rs pipeline with
/// the production native (cherchi-rs) backend.
///
/// The input solids stay live in the arena; the result is a NEW solid.
/// Error contract: [`KernelV2Error::UnsupportedCoplanar`] for coplanar
/// input face pairs (M8 boundary), [`KernelV2Error::EmptyBooleanResult`]
/// for an empty result, [`KernelV2Error::BooleanFailed`] for any other
/// yang-rs failure (loud Display text), plus the [`from_yang_brep`]
/// reassembly errors.
pub fn boolean_op(
    _arena: &mut BrepArena,
    _a: SolidId,
    _b: SolidId,
    _op: BoolOp,
) -> Result<SolidId, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("boolean_op"))
}
