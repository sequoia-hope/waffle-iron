//! Introspection basics (PR-KV3): edge extraction and scalar queries.
//!
//! Together with what earlier slices already provide, this rounds out the
//! Phase-4a introspection surface:
//!
//! - element counts — [`BrepArena::euler_counts`] /
//!   [`crate::validate::TopologyReport`] (KV1)
//! - signed volume — [`crate::geom::signed_volume`] (KV2)
//! - per-face plane — [`face_plane`] (this slice; typed accessor over the
//!   stored, Newell-validated surface)
//! - surface area — [`surface_area`] (this slice)
//! - boundary edges — [`extract_edges`] (this slice; straight segments,
//!   trivial for planar faces — one polyline segment per undirected edge)

use crate::arena::{BrepArena, FaceId, Plane, SolidId};
use crate::error::KernelV2Error;
use cad_primitives::Point3;

/// Every undirected edge of `solid` as a straight segment `[start, end]`
/// (planar faces ⇒ all edge curves are line segments). Each edge is
/// reported ONCE (half-edge pairs deduplicated), in deterministic
/// half-edge id order; the endpoint order is the lower-id half-edge's
/// direction.
pub fn extract_edges(
    _arena: &BrepArena,
    _solid: SolidId,
) -> Result<Vec<[Point3; 2]>, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("extract_edges"))
}

/// Total surface area of `solid`: per face, the polygon-with-holes area
/// `(Newell(outer) + Σ Newell(ring)) · n̂ / 2` — rings wind opposite the
/// outer loop, so holes subtract automatically (same identity the signed
/// volume uses).
pub fn surface_area(_arena: &BrepArena, _solid: SolidId) -> Result<f64, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("surface_area"))
}

/// The face's plane (point + outward unit normal). Typed accessor over
/// `Face::surface`: `Err(FaceWithoutSurface)` while a face is under
/// construction (finished solids always carry `Some`).
pub fn face_plane(_arena: &BrepArena, _face: FaceId) -> Result<Plane, KernelV2Error> {
    Err(KernelV2Error::NotImplemented("face_plane"))
}
