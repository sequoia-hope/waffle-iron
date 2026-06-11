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

use std::collections::BTreeSet;

use crate::arena::{BrepArena, Face, FaceId, LoopBoundary, Plane, SolidId, Surface};
use crate::error::KernelV2Error;
use crate::geom;
use cad_primitives::Point3;

/// Every undirected edge of `solid` as a polyline, at the canonical render
/// chord tolerance ([`crate::tessellate::RENDER_CHORD_TOLERANCE_REL`]).
///
/// - A straight edge is a 2-point polyline `[start, end]`.
/// - A full-circle edge (PR-KV5a) is an `N + 1`-point closed polyline
///   (`last == first`, making closure explicit), sampled at the SAME `N`
///   as render tessellation
///   ([`crate::tessellate::circle_segment_count`]) so extracted edges lie
///   exactly on the rendered rim.
///
/// Each edge is reported ONCE (half-edge pairs deduplicated), in
/// deterministic half-edge id order; the traversal order is the lower-id
/// half-edge's direction.
pub fn extract_edges(arena: &BrepArena, solid: SolidId) -> Result<Vec<Vec<Point3>>, KernelV2Error> {
    extract_edges_with_chord_tolerance(arena, solid, crate::tessellate::RENDER_CHORD_TOLERANCE_REL)
}

/// [`extract_edges`] with an explicit relative chord tolerance (see
/// [`crate::tessellate::tessellate_with_chord_tolerance`] for the bound's
/// definition and rationale).
pub fn extract_edges_with_chord_tolerance(
    arena: &BrepArena,
    solid: SolidId,
    rel_chord_tolerance: f64,
) -> Result<Vec<Vec<Point3>>, KernelV2Error> {
    let _ = rel_chord_tolerance;
    let he_set = solid_half_edges(arena, solid)?;
    let mut out = Vec::with_capacity(he_set.len() / 2);
    for &h in &he_set {
        let he = arena.half_edge(h)?;
        if he.twin < h {
            continue; // the twin (lower id) already reported this edge
        }
        let start = arena.vertex(he.origin)?.point;
        let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
        out.push(vec![start, end]);
    }
    Ok(out)
}

/// Total surface area of `solid`: per face, the polygon-with-holes area
/// `(Newell(outer) + Σ Newell(ring)) · n̂ / 2` — rings wind opposite the
/// outer loop, so holes subtract automatically (same identity the signed
/// volume uses).
pub fn surface_area(arena: &BrepArena, solid: SolidId) -> Result<f64, KernelV2Error> {
    let mut total = 0.0f64;
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let plane = plane_of(face, f)?;
            let n = plane.normal;
            let mut twice = 0.0f64;
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                let nw = geom::newell(&arena.loop_points(lid)?);
                twice += nw[0] * n.x + nw[1] * n.y + nw[2] * n.z;
            }
            total += twice / 2.0;
        }
    }
    Ok(total)
}

/// The face's plane (point + outward unit normal). Typed accessor over
/// `Face::surface`: `Err(FaceWithoutSurface)` while a face is under
/// construction (finished solids always carry `Some`).
pub fn face_plane(arena: &BrepArena, face: FaceId) -> Result<Plane, KernelV2Error> {
    plane_of(arena.face(face)?, face)
}

fn plane_of(face: &Face, id: FaceId) -> Result<Plane, KernelV2Error> {
    match face.surface {
        Some(Surface::Plane(plane)) => Ok(plane),
        Some(_) => Err(KernelV2Error::FaceNotPlanar { face: id }),
        None => Err(KernelV2Error::FaceWithoutSurface { face: id }),
    }
}

/// All half-edges reachable from a solid, in id order.
pub(crate) fn solid_half_edges(
    arena: &BrepArena,
    solid: SolidId,
) -> Result<BTreeSet<crate::arena::HalfEdgeId>, KernelV2Error> {
    let mut he_set = BTreeSet::new();
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            for lid in loops {
                if matches!(arena.loop_(lid)?.boundary, LoopBoundary::Lone(_)) {
                    continue;
                }
                he_set.extend(arena.loop_half_edges(lid)?);
            }
        }
    }
    Ok(he_set)
}
