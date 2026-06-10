//! Planar primitive constructors over the Euler-operator arena (PR-KV2).
//!
//! Two constructors, both pure Euler-operator sequences (no raw arena
//! mutation), both validated loudly:
//!
//! - [`make_face_from_profile`] — a **lamina**: two opposite-normal faces
//!   sharing the profile's boundary edges (Stroud 2006 §3.4 / fig. 3.9's
//!   "square lamina" intermediate; the closed-sheet starting state for
//!   sweeping). Holes become one ring on each face via the lid/`kemr`/
//!   `kfmrh` sequence at zero height (a holed lamina is torus-like:
//!   `V − E + F − R = 0 = 2(S − G)` with `G` = number of holes).
//! - [`extrude`] — the classic linear sweep (Stroud 2006 §6.2, fig. 6.11's
//!   vertex-based sweep: "MEV — first extension vertex; MEV, MFE — per
//!   extension face; …; MFE — final extension face"): build the base
//!   lamina, erect one post (`mev`) per profile vertex and one wall (`mef`)
//!   per profile edge, the receiving face's residual loop becoming the top;
//!   then drill each hole through with the KV1 through-hole sequence
//!   (bridge `mev` + spur chain + lid `mef` + `kemr`, posts down, hole
//!   walls, membrane `kfmrh`).
//!
//! ## Result invariants (asserted by the KV2 oracles)
//!
//! - `validate_solid` green (manifold, twin-paired, Newell-consistent,
//!   Euler–Poincaré with `G` = number of through-holes).
//! - Outward orientation: `geom::signed_volume > 0` for every extrude,
//!   regardless of whether the sweep direction is along `+n` or `−n` (the
//!   constructor reverses the working loop orientation when `d · n < 0`).
//! - Base and top faces are parallel (top is the base loop translated by
//!   the sweep vector); walls are planar quads (each wall is a
//!   parallelogram — two translated copies of one profile edge).
//! - Hole-wall outward normals point INTO the hole void (outward from
//!   material).
//!
//! ## Error contract
//!
//! All input validation happens BEFORE the first arena mutation, so an
//! `Err` from argument checking leaves the arena untouched. Once a
//! (pre-validated) construction sequence starts, the individual Euler
//! operators uphold their own invariants; a mid-sequence operator error
//! would indicate a kernel-v2 bug and is returned loudly rather than
//! masked. As defense in depth, both constructors run `validate_solid` on
//! the finished solid and propagate its verdict.

use crate::arena::{BrepArena, FaceId, ShellId, SolidId};
use crate::error::KernelV2Error;
use crate::profile::Profile;
use cad_primitives::Vector3;

/// `|d̂ · n̂|` floor below which an extrude direction is rejected as
/// in-plane ([`KernelV2Error::ExtrudeDirectionInPlane`]). A *sheared*
/// extrude (direction oblique to the plane normal) is legal — walls remain
/// planar parallelograms — but a direction this close to the plane spans
/// (numerically) no volume.
pub const EXTRUDE_MIN_NORMAL_COSINE: f64 = 1e-9;

/// Entities produced by [`make_face_from_profile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaminaResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The face whose outward normal is the profile's `+normalize(u × v)`.
    pub front: FaceId,
    /// The face whose outward normal is `−normalize(u × v)`.
    pub back: FaceId,
}

/// Entities produced by [`extrude`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtrudeResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The face lying in the profile plane. Its outward normal opposes the
    /// sweep vector's normal component.
    pub base: FaceId,
    /// The translated face (base loop + sweep vector); outward normal along
    /// the sweep vector's normal component.
    pub top: FaceId,
    /// Outer wall faces, one per outer-loop edge, in loop walk order.
    pub walls: Vec<FaceId>,
    /// Hole wall faces, `hole_walls[k]` = walls of hole `k`, one per hole
    /// edge, in loop walk order.
    pub hole_walls: Vec<Vec<FaceId>>,
}

/// Build a lamina (two opposite-normal faces sharing the profile boundary)
/// from a validated [`Profile`]. See the module docs for the construction
/// and invariants. Holes yield one ring on each face and increment shell
/// genus (the holed lamina is torus-like).
pub fn make_face_from_profile(
    _arena: &mut BrepArena,
    _profile: &Profile,
) -> Result<LaminaResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented(
        "make_face_from_profile (PR-KV2 RED)",
    ))
}

/// Extrude a validated [`Profile`] along `normalize(direction) * distance`.
/// See the module docs for the construction sequence and result invariants.
///
/// Errors (all pre-mutation): [`KernelV2Error::ExtrudeNonPositiveDistance`]
/// (zero, negative, or non-finite distance),
/// [`KernelV2Error::ExtrudeDegenerateDirection`] (zero / non-finite
/// direction), [`KernelV2Error::ExtrudeDirectionInPlane`]
/// (`|d̂ · n̂| < EXTRUDE_MIN_NORMAL_COSINE`).
pub fn extrude(
    _arena: &mut BrepArena,
    _profile: &Profile,
    _direction: Vector3,
    _distance: f64,
) -> Result<ExtrudeResult, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("extrude (PR-KV2 RED)"))
}
