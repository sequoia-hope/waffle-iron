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
//! - boundary edges — [`extract_edges`] (KV3 straight segments; PR-KV5a
//!   adds closed circle polylines sampled at the render-tessellation `N`)
//!
//! PR-KV5a upgraded `surface_area` and `geom::signed_volume` to analytic
//! per-surface-type evaluation (exact rational π-coefficients) so curved
//! introspection is tessellation-independent.

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
    use crate::arena::Curve;
    let n_seg = crate::tessellate::circle_segment_count(rel_chord_tolerance);
    let he_set = solid_half_edges(arena, solid)?;
    let mut out = Vec::with_capacity(he_set.len() / 2);
    for &h in &he_set {
        let he = arena.half_edge(h)?;
        if he.twin < h {
            continue; // the twin (lower id) already reported this edge
        }
        match he.curve {
            Curve::LineSegment => {
                let start = arena.vertex(he.origin)?.point;
                let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                out.push(vec![start, end]);
            }
            Curve::Arc { .. } => {
                // PR-KV5b: an arc edge (boolean-output intersection circle
                // piece) extracts as its chord-bound sample polyline —
                // endpoints + the SAME interior samples render tessellation
                // uses, so extracted edges lie exactly on the rendered seams.
                let start = arena.vertex(he.origin)?.point;
                let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let mut pl = vec![start];
                pl.extend(crate::tessellate::arc_interior_samples(arena, h, n_seg)?);
                pl.push(end);
                out.push(pl);
            }
            Curve::EllipseArc { .. } => {
                // PR-KV9: same contract as Arc — the render-identical
                // sample polyline.
                let start = arena.vertex(he.origin)?.point;
                let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let mut pl = vec![start];
                pl.extend(crate::tessellate::ellipse_interior_samples(
                    arena, h, n_seg,
                )?);
                pl.push(end);
                out.push(pl);
            }
            Curve::HyperbolaArc { .. } => {
                // KV16: same contract as Arc — the render-identical
                // sample polyline.
                let start = arena.vertex(he.origin)?.point;
                let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let mut pl = vec![start];
                pl.extend(crate::tessellate::hyperbola_interior_samples(
                    arena, h, n_seg,
                )?);
                pl.push(end);
                out.push(pl);
            }
            Curve::SurfacePair { .. } => {
                // M5: same contract as Arc — endpoints + the render-identical
                // certified sample polyline (Newton-projected onto both
                // defining surfaces).
                let start = arena.vertex(he.origin)?.point;
                let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
                let mut pl = vec![start];
                pl.extend(crate::tessellate::surface_pair_edge_samples(
                    arena, h, n_seg,
                )?);
                pl.push(end);
                out.push(pl);
            }
            Curve::Circle {
                center,
                normal,
                radius,
            } => {
                // Sampled at the SAME N as render tessellation; closed
                // polyline (last == first, bitwise).
                let anchor = arena.vertex(he.origin)?.point;
                let Some((e1, e2)) = crate::tessellate::circle_frame(center, normal, anchor) else {
                    return Err(KernelV2Error::CurvedGeometryMismatch {
                        face: arena.loop_(he.loop_id)?.face,
                        reason: "extract_edges: degenerate circle frame (anchor not radial)",
                    });
                };
                let mut pl = Vec::with_capacity(n_seg as usize + 1);
                for k in 0..n_seg {
                    let theta = 2.0 * std::f64::consts::PI * f64::from(k) / f64::from(n_seg);
                    let (s, c) = theta.sin_cos();
                    pl.push(Point3::new(
                        center.x() + radius * (c * e1[0] + s * e2[0]),
                        center.y() + radius * (c * e1[1] + s * e2[1]),
                        center.z() + radius * (c * e1[2] + s * e2[2]),
                    ));
                }
                pl.push(pl[0]);
                out.push(pl);
            }
        }
    }
    Ok(out)
}

/// Total surface area of `solid`, analytically per surface type
/// (tessellation-independent, mirroring [`geom::signed_volume`]):
///
/// - **Planar faces, polygonal loops**: `(Newell(outer) + Σ Newell(ring))
///   · n̂ / 2` — rings wind opposite the outer loop, so holes subtract
///   automatically. (Bit-identical to the KV3 implementation for
///   all-planar solids.)
/// - **Planar disks bounded by a full-circle edge** (PR-KV5a): `±π r²`,
///   signed by the circle traversal sense vs. the face normal (outer disk
///   `+`, ring `−`).
/// - **Cylinder laterals**: `2π r ℓ`, `ℓ = (c_other − c_this) · ν` from one
///   rim.
///
/// The π-terms accumulate as an exact `dashu` rational coefficient and
/// round once — a cylinder's area is `to_f64(2r² + 2rℓ) · π`, **bitwise**
/// `2πr(h + r)` when the coefficient is exactly representable.
pub fn surface_area(arena: &BrepArena, solid: SolidId) -> Result<f64, KernelV2Error> {
    use crate::arena::Curve;
    use crate::exact2d::r as rq;
    use dashu::rational::RBig;

    let mut total = 0.0f64;
    let mut pi_coeff = RBig::ZERO;
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;

            let mut loops = vec![face.outer_loop];
            loops.extend(face.inner_loops.iter().copied());
            let mut loop_data = Vec::with_capacity(loops.len());
            for &lid in &loops {
                let hes = arena.loop_half_edges(lid)?;
                let mut circles = Vec::new();
                for &h in &hes {
                    match arena.half_edge(h)?.curve {
                        Curve::Circle {
                            center,
                            normal,
                            radius,
                        } => circles.push((center, normal, radius)),
                        // PR-KV5b partial patches (+ M5 surface-pair
                        // boundaries): no analytic closed form — loud, never
                        // a silent polygonal sum over arc chords.
                        Curve::Arc { .. }
                        | Curve::EllipseArc { .. }
                        | Curve::HyperbolaArc { .. }
                        | Curve::SurfacePair { .. } => {
                            return Err(KernelV2Error::CurvedGeometryMismatch {
                                face: f,
                                reason: "surface_area: analytic area not implemented for \
                                         arc-bounded faces (KV5b partial patches)",
                            });
                        }
                        Curve::LineSegment => {}
                    }
                }
                loop_data.push((lid, hes.len(), circles));
            }

            if let Some(Surface::Cylinder { reversed: true, .. }) = face.surface {
                return Err(KernelV2Error::CurvedGeometryMismatch {
                    face: f,
                    reason: "surface_area: cavity-sense (reversed) cylinder faces are \
                             KV5b partial patches with no analytic closed form",
                });
            }
            if let Some(Surface::Cylinder { .. }) = face.surface {
                let rims: Vec<_> = loop_data
                    .iter()
                    .flat_map(|(_, _, c)| c.iter().copied())
                    .collect();
                if rims.len() != 2 {
                    return Err(KernelV2Error::CurvedGeometryMismatch {
                        face: f,
                        reason: "surface_area: cylinder face without exactly two rims",
                    });
                }
                let (c0, nu, rad) = rims[0];
                let (c1, _, _) = rims[1];
                let ell = (rq(c1.x()) - rq(c0.x())) * rq(nu.x)
                    + (rq(c1.y()) - rq(c0.y())) * rq(nu.y)
                    + (rq(c1.z()) - rq(c0.z())) * rq(nu.z);
                pi_coeff += RBig::from(2) * rq(rad) * ell;
                continue;
            }

            let plane = plane_of(face, f)?;
            let n = plane.normal;
            let mut twice = 0.0f64; // face-local, keeps KV3 rounding order
            for (lid, he_count, circles) in &loop_data {
                if circles.is_empty() {
                    let nw = geom::newell(&arena.loop_points(*lid)?);
                    twice += nw[0] * n.x + nw[1] * n.y + nw[2] * n.z;
                } else if *he_count == 1 {
                    let (_, nu, rad) = circles[0];
                    let term = rq(rad) * rq(rad);
                    if geom::dot(nu, n) > 0.0 {
                        pi_coeff += term;
                    } else {
                        pi_coeff -= term;
                    }
                } else {
                    return Err(KernelV2Error::CurvedGeometryMismatch {
                        face: f,
                        reason: "surface_area: loop mixes circle and segment edges",
                    });
                }
            }
            total += twice / 2.0;
        }
    }
    Ok(total + pi_coeff.to_f64().value() * std::f64::consts::PI)
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
