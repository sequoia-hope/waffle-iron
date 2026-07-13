//! Primitive constructors over the B-Rep arena (PR-KV2 planar; PR-KV5a
//! adds circle profiles → right circular cylinders).
//!
//! Two public constructors. On polygon profiles both are pure
//! Euler-operator sequences (no raw arena mutation); on circle profiles
//! both dispatch to direct assemblers ([`extrude_circle`],
//! [`circle_lamina`] — justification on `extrude_circle`). All paths are
//! validated loudly:
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

use crate::arena::{
    BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
    Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
};
use crate::error::KernelV2Error;
use crate::euler::{kemr, kfmrh, mef, mev, mev_lone, mvfs};
use crate::profile::{cross, Profile, ProfileEdge, ProfileRegion};
use crate::validate::validate_solid;
use cad_primitives::{Point2, Point3, Vector3};

/// The universal constructor exit (KV13 F1): validate the finished solid,
/// then stamp a persistent id on every face that lacks one. Every public
/// constructor ends here, so all output faces carry a `Pid`. Validation runs
/// first; `Pid`s are pure metadata and never affect the geometry validate
/// checks. (Raw Euler-op test arenas that never reach a constructor simply
/// have no `Pid`s — `validate_solid` itself does not require them.)
pub(crate) fn finalize_solid(
    arena: &mut BrepArena,
    solid: crate::arena::SolidId,
) -> Result<(), KernelV2Error> {
    validate_solid(arena, solid)?;
    arena.assign_face_pids(solid)
}

fn neg(n: UnitVector3) -> UnitVector3 {
    UnitVector3 {
        x: -n.x,
        y: -n.y,
        z: -n.z,
    }
}

mod revolve;
pub use revolve::{
    revolve, RevolveResult, REVOLVE_AXIS_IN_PLANE_TOLERANCE, REVOLVE_EDGE_ALIGNMENT_TOLERANCE,
    REVOLVE_FULL_TURN_TOLERANCE, REVOLVE_MIN_AXIS_CLEARANCE_REL,
};

mod extrude;
pub use extrude::{
    extrude, make_face_from_profile, ExtrudeResult, LaminaResult, CIRCLE_EXTRUDE_MAX_AXIS_SINE,
    EXTRUDE_MIN_NORMAL_COSINE,
};
