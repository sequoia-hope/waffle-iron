//! Render tessellation (PR-KV3, Phase 4a): solid → triangle mesh.
//!
//! ## Single canonical path (crate hard rule 5)
//!
//! ONE implementation per surface type:
//!
//! - planar faces with polygonal loops — exact-predicate constrained
//!   Delaunay triangulation (the shared `cdt_polygon_with_holes` core);
//! - planar disk caps bounded by one full-circle edge (PR-KV5a) —
//!   rim sampling at the chord-bound `N` + a convex fan;
//! - cylinder laterals (PR-KV5a) — `N` quad-pairs between the two rims
//!   with exact analytic radial normals at the corners.
//!
//! The planar routine constrained-Delaunay triangulates the face's outer
//! loop with its hole loops passed natively (no bridge corridors). No
//! `reverse_outer` masking, no `bulk_flip`, no force-aligning: the polygon
//! walk direction IS the source of truth, and the emitted triangle winding
//! follows it (triangle normals equal the face's Newell normal by
//! construction, never by post-hoc correction).
//!
//! ## Why constrained Delaunay (documented decision — spec
//! `kv2_cdt_triangulation_core` §8)
//!
//! The render cores triangulate exactly-sampled boundary rings for the
//! render channel. The exact-predicate constrained-Delaunay triangulation
//! ([`cdt_polygon_with_holes`], the spade `robust` backend) is the
//! max-min-angle triangulation of the constrained point set: if any
//! triangulation of the ring avoids a render-degenerate sliver, the CDT
//! avoids it, and its flip decisions use exact orientation / in-circle
//! predicates rather than f64. kernel-v2 may depend on yang-rs but not on
//! cherchi-rs directly, so it consumes the primitive through a yang-rs
//! re-export — the same seam as `NativeBoolean` and the torus UV consumer.
//! Hole loops (through-cuts) are first-class and passed NATIVELY: the CDT
//! inserts each as a hard constraint loop; there are no bridge corridors
//! (their doubled vertices would be rejected as coincident).
//!
//! Historical note: the original KV3/KV5b cores greedily ear-clipped with
//! exact `orient2d` and a plain-f64 Delaunay flip. That flip's f64 incircle
//! is catastrophically ill-conditioned exactly on slivers, so it minted
//! sub-f32 render-degenerate triangles from HEALTHY boundaries (measured on
//! F0047/R0064, spec §6b); the CDT is the root fix (spec §1). Ear clipping
//! is also project doctrine as a sliver liability
//! (`docs/yang_deviations.md` D1).
//!
//! ## Algorithm
//!
//! Per planar face:
//!
//! 1. Project the outer loop and rings onto the dominant-axis coordinate
//!    plane of the face normal, with an axis order chosen so orientation
//!    is preserved (outer CCW, rings CW — guaranteed by the validated
//!    Newell/ring-winding invariants).
//! 2. Constrained-Delaunay triangulate the projected outer loop with its
//!    hole loops as native constraint loops ([`cdt_polygon_with_holes`]);
//!    the returned triangles index the shared boundary vertex pool with CCW
//!    winding and add no Steiner points (boundary vertex set in = out). A
//!    ring the CDT cannot triangulate (self-intersecting projection,
//!    coincident vertices) fails LOUDLY
//!    ([`KernelV2Error::TessellationFailed`]) — never a fallback.
//! 3. Gate every emitted triangle at f32 render precision (G1): a triangle
//!    collapsed to a sub-f32 sliver fails LOUDLY, matching the
//!    cylinder-patch gate — never a skip, snap, or f64 guess.
//!
//! ## Output shape
//!
//! [`RenderMesh`] is flat-array oriented for downstream render consumers:
//! `positions`/`normals` are `3·N` coordinate arrays, `indices` is `3·T`
//! vertex indices, and `face_ranges` maps each face to its contiguous
//! index range (per-face vertex duplication — vertices are NOT shared
//! across faces, so per-face flat normals are exact and per-face picking
//! is a range lookup).
//!
//! ## Exactness guarantees (asserted by the KV3 oracles)
//!
//! - Triangle areas sum to the face area (a CDT without Steiner points is
//!   an exact partition of the polygon-with-holes, same as the ear-clip it
//!   replaced); the f64 oracle tolerance only absorbs summation rounding.
//! - Every triangle winds with the face: its normal direction equals the
//!   face plane normal.
//! - Mesh signed volume equals the solid's B-Rep signed volume (same
//!   region, exact partition).

use std::cmp::Ordering;

use crate::arena::{BrepArena, Curve, FaceId, SolidId, Surface, UnitVector3};
use crate::error::KernelV2Error;
use crate::exact2d;
use cad_primitives::{Point2, Point3, Vector3};
use waffle_types::kernel::units::{TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN};

/// Flat-array triangle mesh for rendering, with per-face index ranges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderMesh {
    /// Vertex positions, `[x0, y0, z0, x1, …]` (meters). Vertices are
    /// per-face (not shared across faces).
    pub positions: Vec<f64>,
    /// Per-vertex unit normals, same layout as `positions`. Planar faces
    /// are flat-shaded: every vertex of a face carries the face normal.
    pub normals: Vec<f64>,
    /// Triangle vertex indices into `positions`/`normals`, `3·T` entries.
    pub indices: Vec<u32>,
    /// Per-face contiguous ranges of `indices`, in solid face walk order.
    pub face_ranges: Vec<FaceRange>,
}

impl RenderMesh {
    /// Number of (per-face) vertices.
    pub fn num_vertices(&self) -> usize {
        self.positions.len() / 3
    }

    /// Number of triangles.
    pub fn num_triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// One face's contiguous range in [`RenderMesh::indices`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceRange {
    /// The arena face this range tessellates.
    pub face: FaceId,
    /// Offset into `indices` (index entries, not triangles).
    pub start: u32,
    /// Number of index entries (a multiple of 3).
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Chord-error bound for circular geometry (PR-KV5a)
// ---------------------------------------------------------------------------

/// Canonical relative chord tolerance for render tessellation of circular
/// geometry: the inscribed-polygon **sagitta band** `d_ε = 1e-3 · r`.
///
/// A full circle of radius `r` approximated by an inscribed regular `N`-gon
/// deviates from the true circle by at most the sagitta
/// `s(N) = r · (1 − cos(π/N))` (the mid-chord depth — the exact maximum
/// radial error, not an estimate). Requiring `s(N) ≤ rel · r` gives
/// `N = ⌈π / arccos(1 − rel)⌉` ([`circle_segment_count`]).
///
/// The band is **relative to the radius** (the same style of justification
/// as yang-rs's chord bounds, which scale with geometry size): an absolute
/// band in meters would over-tessellate large parts and corrupt small ones,
/// while `d_ε ∝ r` is scale-free — and per the chord-band propagation
/// lesson, any consumer converting this band into a derived metric must
/// carry the documented `d_ε(r) = rel · r` rather than re-deriving its own.
/// `N` is a deterministic pure function of the tolerance (no adaptive,
/// view-dependent, or time-dependent refinement — crate hard rule 5).
/// At `rel = 1e-3`, `N = 71`.
pub const RENDER_CHORD_TOLERANCE_REL: f64 = 1e-3;

/// Floor on the circle segment count: even an absurdly loose tolerance
/// keeps a recognizable (and strictly convex-in-projection) rim.
pub const MIN_CIRCLE_SEGMENTS: u32 = 8;

/// Number of inscribed-polygon segments for a full circle at relative chord
/// tolerance `rel` (see [`RENDER_CHORD_TOLERANCE_REL`] for the bound):
/// `N = max(8, ⌈π / arccos(1 − min(rel, 2))⌉)`. Deterministic in `rel`;
/// because the band is radius-relative, `N` is the same for every radius
/// (the absolute band `d_ε = rel · r` scales instead).
pub fn circle_segment_count(rel_chord_tolerance: f64) -> u32 {
    let rel = if rel_chord_tolerance.is_finite() && rel_chord_tolerance > 0.0 {
        rel_chord_tolerance.min(2.0)
    } else {
        // Non-finite / non-positive tolerance: fall back to the canonical
        // band rather than guessing tighter.
        RENDER_CHORD_TOLERANCE_REL
    };
    let n = (std::f64::consts::PI / (1.0 - rel).acos()).ceil();
    (n as u32).max(MIN_CIRCLE_SEGMENTS)
}

/// Tessellate every face of `solid` into a [`RenderMesh`] at the canonical
/// chord tolerance ([`RENDER_CHORD_TOLERANCE_REL`]).
///
/// Deterministic: faces in shell walk order, loop points in walk order,
/// exact-arithmetic ear selection with fixed scan order, circle sampling at
/// a tolerance-determined fixed `N`. Errors are loud: a face that cannot be
/// tessellated returns [`KernelV2Error::TessellationFailed`] (never a
/// silent skip, never an f64 guess).
pub fn tessellate(arena: &BrepArena, solid: SolidId) -> Result<RenderMesh, KernelV2Error> {
    tessellate_with_chord_tolerance(arena, solid, RENDER_CHORD_TOLERANCE_REL)
}

/// [`tessellate`] with an explicit relative chord tolerance (the parameter
/// only affects circular geometry; planar straight-edge faces are exact at
/// any tolerance). Exposed so callers — and the convergence oracles — can
/// tighten the band; both entry points share the single canonical per-face
/// routines (crate hard rule 5).
pub fn tessellate_with_chord_tolerance(
    arena: &BrepArena,
    solid: SolidId,
    rel_chord_tolerance: f64,
) -> Result<RenderMesh, KernelV2Error> {
    let n_seg = circle_segment_count(rel_chord_tolerance);
    let mut mesh = RenderMesh::default();
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            let face = arena.face(f)?;
            match face.surface {
                Some(Surface::Cylinder { .. }) => {
                    // Canonical full lateral (full-circle rims, KV5a) vs a
                    // partial boolean-output patch (arc/segment loops, KV5b).
                    if face_has_circle_edge(arena, f)? {
                        tessellate_cylinder_lateral(arena, f, n_seg, &mut mesh)?
                    } else {
                        tessellate_cylinder_patch(arena, f, n_seg, &mut mesh)?
                    }
                }
                Some(Surface::Plane(_)) => {
                    if planar_face_is_canonical_cap(arena, f)? {
                        tessellate_circular_cap(arena, f, n_seg, &mut mesh)?
                    } else {
                        tessellate_planar_face(arena, f, n_seg, &mut mesh)?
                    }
                }
                Some(Surface::Cone { .. }) => {
                    // Canonical full lateral (full-circle rims, KV6c) vs a
                    // partial arc-bounded patch (the partial-revolve oblique
                    // wall / boolean outputs — KV6c increment 5).
                    if face_has_circle_edge(arena, f)? {
                        tessellate_cone_lateral(arena, f, n_seg, &mut mesh)?
                    } else {
                        tessellate_cone_patch(arena, f, n_seg, &mut mesh)?
                    }
                }
                Some(Surface::Torus { .. }) => {
                    // Canonical modeling lateral (structured seam-arc loop, KV6d
                    // 1-3) vs a boolean-output patch (trimmed polyline boundary,
                    // KV6d 5b2 — delegated to yang-rs's UV-CDT consumer).
                    if face_has_circle_edge(arena, f)? {
                        tessellate_torus_lateral(arena, f, n_seg, &mut mesh)?
                    } else {
                        tessellate_torus_patch(arena, f, n_seg, &mut mesh)?
                    }
                }
                Some(Surface::Sphere { .. }) => {
                    // Closed modeling sphere (the seam-arc twin-pair loop, KV6d
                    // increment 2) vs a boolean-output patch (trimmed loops —
                    // delegated to yang-rs's lat/long UV-CDT consumer).
                    if sphere_face_is_closed(arena, f)? {
                        tessellate_sphere_closed(arena, f, n_seg, &mut mesh)?
                    } else {
                        tessellate_sphere_patch(arena, f, n_seg, &mut mesh)?
                    }
                }
                None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            }
        }
    }
    Ok(mesh)
}

mod sampling;
pub use sampling::surface_pair_interior_samples;
pub(crate) use sampling::{
    arc_interior_samples, ellipse_interior_samples, hyperbola_interior_samples,
    surface_pair_edge_samples,
};

/// A loop's boundary polyline for planar tessellation: origin vertices in
/// walk order, with arc edges (PR-KV5b) expanded to their chord-bound
/// samples. Pure-segment loops come back exactly as `loop_points` (the
/// KV3 planar path is byte-identical).
fn sampled_loop_points(
    arena: &BrepArena,
    lid: crate::arena::LoopId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let mut pts = Vec::new();
    for h in arena.loop_half_edges(lid)? {
        let he = arena.half_edge(h)?;
        let origin = arena.vertex(he.origin)?.point;
        pts.push(origin);
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        {
            // PR-KV7: a full-circle edge inside a GENERAL planar loop (a
            // recovered round hole in a seg-bounded face, or a multi-ring
            // cap outside the KV6a disk/annulus vocabulary). Same sampling
            // convention as `tessellate_annular_cap`: uniform angles from
            // the anchor's frame, CCW around this half-edge's traversal
            // normal — so the samples agree with the adjacent lateral's rim
            // row within trig rounding (the existing cross-face contract).
            let fid = arena.loop_(he.loop_id)?.face;
            let Some((e1, e2)) = circle_frame(center, normal, origin) else {
                return Err(KernelV2Error::TessellationFailed {
                    face: fid,
                    reason: "degenerate circle frame (anchor does not span a radial direction)",
                });
            };
            for k in 1..n_seg {
                let theta = 2.0 * std::f64::consts::PI * f64::from(k) / f64::from(n_seg);
                let (sn, cs) = theta.sin_cos();
                pts.push(Point3::new(
                    center.x() + radius * (cs * e1[0] + sn * e2[0]),
                    center.y() + radius * (cs * e1[1] + sn * e2[1]),
                    center.z() + radius * (cs * e1[2] + sn * e2[2]),
                ));
            }
        } else if matches!(he.curve, Curve::EllipseArc { .. }) {
            pts.extend(ellipse_interior_samples(arena, h, n_seg)?);
        } else if matches!(he.curve, Curve::HyperbolaArc { .. }) {
            // KV16: the plane∩cone section arc IS planar — expand to its
            // sag-bound samples exactly like the ellipse arc.
            pts.extend(hyperbola_interior_samples(arena, h, n_seg)?);
        } else if matches!(he.curve, Curve::SurfacePair { .. }) {
            // M5 K8: a transversal quadric-pair curve is never planar —
            // loud, not an empty-sample fall-through.
            let fid = arena.loop_(he.loop_id)?.face;
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "surface-pair edge on a planar face (never planar)",
            });
        } else {
            pts.extend(arc_interior_samples(arena, h, n_seg)?);
        }
    }
    Ok(pts)
}

/// Is this planar face in the canonical KV5a/KV6a cap vocabulary — outer
/// loop ONE full-circle half-edge and at most one ring that is also one
/// full-circle half-edge? Those keep the byte-for-byte disk/annulus paths;
/// everything else (seg-bounded faces with round holes, multi-ring caps)
/// goes through the general planar path with circle expansion (PR-KV7).
fn planar_face_is_canonical_cap(arena: &BrepArena, fid: FaceId) -> Result<bool, KernelV2Error> {
    let face = arena.face(fid)?;
    let single_circle = |lid| -> Result<bool, KernelV2Error> {
        let hes = arena.loop_half_edges(lid)?;
        Ok(match hes[..] {
            [h] => matches!(arena.half_edge(h)?.curve, Curve::Circle { .. }),
            _ => false,
        })
    };
    if face.inner_loops.len() > 1 || !single_circle(face.outer_loop)? {
        return Ok(false);
    }
    for &lid in &face.inner_loops {
        if !single_circle(lid)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Does any loop of the face carry a `Curve::Circle` half-edge?
fn face_has_circle_edge(arena: &BrepArena, fid: FaceId) -> Result<bool, KernelV2Error> {
    let face = arena.face(fid)?;
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    for lid in loops {
        for h in arena.loop_half_edges(lid)? {
            if matches!(arena.half_edge(h)?.curve, Curve::Circle { .. }) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// In-plane orthonormal frame for sampling a circle: `e1` along the seam
/// anchor's radial direction (so sample 0 sits at the anchor), `e2 = ν × e1`
/// — sampling `center + r(cosθ·e1 + sinθ·e2)` then runs CCW around `ν`.
/// `None` when the anchor does not span a radial direction (degenerate /
/// corrupt geometry — callers fail loudly).
pub(crate) fn circle_frame(
    center: Point3,
    nu: UnitVector3,
    anchor: Point3,
) -> Option<([f64; 3], [f64; 3])> {
    let d = [
        anchor.x() - center.x(),
        anchor.y() - center.y(),
        anchor.z() - center.z(),
    ];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if !(len.is_finite() && len > 0.0) {
        return None;
    }
    let e1 = [d[0] / len, d[1] / len, d[2] / len];
    let c = [
        nu.y * e1[2] - nu.z * e1[1],
        nu.z * e1[0] - nu.x * e1[2],
        nu.x * e1[1] - nu.y * e1[0],
    ];
    let clen = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    if !(clen.is_finite() && clen > 0.5) {
        return None; // anchor (anti)parallel to the axis — not a radial dir
    }
    Some((e1, [c[0] / clen, c[1] / clen, c[2] / clen]))
}

/// The single canonical planar-disk-cap routine (PR-KV5a): a planar face
/// whose outer loop is ONE full-circle half-edge, no rings. Rim sampled at
/// the chord-bound `N` (uniform angles from the seam anchor, CCW around the
/// circle's directional normal == the face normal), fanned from sample 0 —
/// the fan of a convex polygon needs no ear search, and its winding follows
/// the boundary walk (hard rule 5: no post-hoc flips). Flat-shaded with the
/// face normal.
fn tessellate_circular_cap(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Plane(plane)) = face.surface else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    if !face.inner_loops.is_empty() {
        return tessellate_annular_cap(arena, fid, n_seg, out);
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let [h] = hes[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "circle-bounded planar loop must be a single circle half-edge (KV5a)",
        });
    };
    let he = arena.half_edge(h)?;
    let Curve::Circle {
        center,
        normal,
        radius,
    } = he.curve
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "circle-bounded planar loop must be a single circle half-edge (KV5a)",
        });
    };
    let anchor = arena.vertex(he.origin)?.point;
    let Some((e1, e2)) = circle_frame(center, normal, anchor) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate circle frame (anchor does not span a radial direction)",
        });
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg as usize;
    for k in 0..n {
        let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
        let (s, c) = theta.sin_cos();
        out.positions.extend_from_slice(&[
            center.x() + radius * (c * e1[0] + s * e2[0]),
            center.y() + radius * (c * e1[1] + s * e2[1]),
            center.z() + radius * (c * e1[2] + s * e2[2]),
        ]);
        out.normals
            .extend_from_slice(&[plane.normal.x, plane.normal.y, plane.normal.z]);
    }
    for k in 1..(n as u32) - 1 {
        out.indices
            .extend_from_slice(&[base, base + k, base + k + 1]);
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Annular planar cap (PR-KV6a, the full-turn revolve washer): outer loop
/// ONE full-circle half-edge, exactly one ring that is also one full-circle
/// half-edge, both concentric in the face plane. Sampled at the shared
/// chord-bound `N` on a single angle table anchored at the OUTER circle's
/// seam vertex (the ring is sampled at the same table re-anchored at its
/// own seam), then stitched as one quad strip — the planar analog of the
/// cylinder lateral, flat-shaded with the face normal.
fn tessellate_annular_cap(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Plane(plane)) = face.surface else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    let [ring_lid] = face.inner_loops[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "annular cap with more than one ring is outside the KV6a vocabulary",
        });
    };
    let circle_of = |lid| -> Result<(Point3, UnitVector3, f64, Point3), KernelV2Error> {
        let hes = arena.loop_half_edges(lid)?;
        let [h] = hes[..] else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "annular cap loop must be a single circle half-edge",
            });
        };
        let he = arena.half_edge(h)?;
        let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "annular cap loop must be a single circle half-edge",
            });
        };
        Ok((center, normal, radius, arena.vertex(he.origin)?.point))
    };
    let (c_o, nu_o, r_o, anchor_o) = circle_of(face.outer_loop)?;
    let (c_r, _nu_r, r_r, anchor_r) = circle_of(ring_lid)?;

    // Each ring is sampled in its OWN anchor frame (CCW around `nu_o`), so its
    // boundary samples coincide with the adjacent lateral's rim samples (both
    // anchored at the same seam vertex) — load-bearing for cross-face
    // watertightness. The two seams need NOT be at the same azimuth (the
    // gear's counterbore floor has independent outer/inner seams); the strip
    // below sweeps both rings by azimuth rather than stitching column-to-
    // column, so a phase offset between the seams no longer twists the quads.
    let Some((e1_o, e2_o)) = circle_frame(c_o, nu_o, anchor_o) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate circle frame (anchor does not span a radial direction)",
        });
    };
    let Some((e1_r, e2_r)) = circle_frame(c_r, nu_o, anchor_r) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate ring circle frame",
        });
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;

    // Emit both rings' samples (each anchored at its OWN seam — load-bearing
    // for cross-face watertightness with the adjacent lateral, which samples
    // from the same seam vertex). The outer ring occupies render indices
    // base..base+n, the inner ring base+n..base+2n.
    for (center, radius, e1, e2) in [(c_o, r_o, e1_o, e2_o), (c_r, r_r, e1_r, e2_r)] {
        for k in 0..n {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            let (sn, cs) = theta.sin_cos();
            out.positions.extend_from_slice(&[
                center.x() + radius * (cs * e1[0] + sn * e2[0]),
                center.y() + radius * (cs * e1[1] + sn * e2[1]),
                center.z() + radius * (cs * e1[2] + sn * e2[2]),
            ]);
            out.normals
                .extend_from_slice(&[plane.normal.x, plane.normal.y, plane.normal.z]);
        }
    }

    // The two rings are anchored at INDEPENDENT seam azimuths (the gear's
    // counterbore floor: outer rim seam ≠ inner bore seam — they descend from
    // different boolean-output vertices). A column-`k`-to-column-`k` strip
    // would stitch outer[k] to inner[k] across that phase offset, producing
    // twisted, self-overlapping quads whose two triangles wind OPPOSITELY —
    // half facing +normal, half −normal (PR-M8-cyl-Inc2). Instead, sweep both
    // rings by their azimuth around the shared axis (measured in the OUTER
    // frame `(e1_o, e2_o)`, CCW around `nu_o`) and advance whichever ring is
    // angularly behind — the standard two-ring annulus triangulation. Each
    // emitted triangle then winds CCW around `nu_o`, i.e. faces `+nu_o`.
    let tau = 2.0 * std::f64::consts::PI;
    // Azimuth (in the OUTER frame, CCW around nu_o, in [0, 2π)) of ring sample
    // `k`. A ring is sampled in its own anchor frame, so sample 0 sits at the
    // anchor's azimuth (measured in the outer frame) and each subsequent sample
    // advances by 2π/n.
    let azimuth = |anchor_dir: [f64; 3], k: u32| -> f64 {
        let ax = anchor_dir[0] * e1_o[0] + anchor_dir[1] * e1_o[1] + anchor_dir[2] * e1_o[2];
        let ay = anchor_dir[0] * e2_o[0] + anchor_dir[1] * e2_o[1] + anchor_dir[2] * e2_o[2];
        let base_phi = ay.atan2(ax);
        (base_phi + tau * (k as f64) / (n as f64)).rem_euclid(tau)
    };
    let dir_of = |anchor: Point3, center: Point3| -> [f64; 3] {
        [
            anchor.x() - center.x(),
            anchor.y() - center.y(),
            anchor.z() - center.z(),
        ]
    };
    let outer_dir = dir_of(anchor_o, c_o);
    let inner_dir = dir_of(anchor_r, c_r);
    let outer_az: Vec<f64> = (0..n).map(|k| azimuth(outer_dir, k)).collect();
    let inner_az: Vec<f64> = (0..n).map(|k| azimuth(inner_dir, k)).collect();

    // Two-pointer sweep. We walk both rings once around the full turn. At each
    // step the quad face (outer[oi], outer[oi+1] | inner[ii], inner[ii+1]) is
    // split by advancing the ring whose NEXT sample has the smaller forward
    // azimuth gap, emitting a triangle that always uses the current edge of one
    // ring and the leading vertex of the other. Winding `[a, b, c]` is chosen
    // so the triangle normal points along `+nu_o` (== the outer frame's CCW
    // sense); the per-vertex render normal is `plane.normal`.
    for tri in annulus_sweep_triangles(&outer_az, &inner_az, base, base + n) {
        out.indices.extend_from_slice(&tri);
    }

    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Triangulate the annulus between two concentric rings sampled CCW around a
/// common axis at INDEPENDENT seam azimuths (PR-M8-cyl-Inc2). `outer_az` /
/// `inner_az` are the per-sample azimuths (radians, in the same CCW frame);
/// `outer_base` / `inner_base` are the render-vertex indices of each ring's
/// sample 0. Sweeps both rings by azimuth — at each step advancing whichever
/// ring's current vertex is angularly behind — so every emitted triangle winds
/// CCW around the axis (faces `+axis`), regardless of the phase offset between
/// the two seams. A naive column-`k`-to-column-`k` strip would twist each quad
/// when the seams differ, flipping half its triangles.
fn annulus_sweep_triangles(
    outer_az: &[f64],
    inner_az: &[f64],
    outer_base: u32,
    inner_base: u32,
) -> Vec<[u32; 3]> {
    let tau = 2.0 * std::f64::consts::PI;
    let fwd_gap = |from: f64, to: f64| -> f64 { (to - from).rem_euclid(tau) };
    let no = outer_az.len();
    let ni = inner_az.len();
    let mut tris = Vec::with_capacity(no + ni);
    if no == 0 || ni == 0 {
        return tris;
    }
    // Align the inner walk's start to the sample nearest-ahead of outer[0] so
    // the two pointers march in lockstep around the turn (deterministic).
    let ii0 = inner_az
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            fwd_gap(outer_az[0], **a)
                .partial_cmp(&fwd_gap(outer_az[0], **b))
                .unwrap_or(Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0);
    let outer_idx = |k: usize| outer_base + (k % no) as u32;
    let inner_idx = |k: usize| inner_base + ((ii0 + k) % ni) as u32;
    let mut oi = 0usize;
    let mut ii = 0usize;
    while oi < no || ii < ni {
        let advance_outer = if oi >= no {
            false
        } else if ii >= ni {
            true
        } else {
            let o_cur = outer_az[oi % no];
            let i_cur = inner_az[(ii0 + ii) % ni];
            // Advance whichever ring's current vertex is angularly behind the
            // other's (smaller forward gap).
            fwd_gap(o_cur, i_cur) <= fwd_gap(i_cur, o_cur)
        };
        if advance_outer {
            // outer[oi], outer[oi+1], inner[ii] — CCW around +axis.
            tris.push([outer_idx(oi), outer_idx(oi + 1), inner_idx(ii)]);
            oi += 1;
        } else {
            // outer[oi], inner[ii+1], inner[ii] — CCW around +axis.
            tris.push([outer_idx(oi), inner_idx(ii + 1), inner_idx(ii)]);
            ii += 1;
        }
    }
    tris
}

/// The single canonical cylinder-lateral routine (PR-KV5a): the full tube
/// between two full-circle rims, as `N` quad-pairs. The angular frame comes
/// from the BOTTOM rim (the one whose directional normal points toward the
/// other — the validated outward-orientation rule), so the quad winding
/// follows the boundary walk; per-vertex normals are the exact analytic
/// outward radial directions at the sampled corners (smooth shading — the
/// surface, not the facets, defines the normal field).
fn tessellate_cylinder_lateral(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut rims = Vec::new();
    for &h in &hes {
        let he = arena.half_edge(h)?;
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        {
            rims.push((center, normal, radius, arena.vertex(he.origin)?.point));
        }
    }
    let [rim_a, rim_b] = rims[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "cylinder lateral must be bounded by exactly two full-circle rims (KV5a)",
        });
    };
    // Bottom rim: traversal axis points toward the opposite rim.
    let toward = |from: &(Point3, UnitVector3, f64, Point3),
                  to: &(Point3, UnitVector3, f64, Point3)| {
        (to.0.x() - from.0.x()) * from.1.x
            + (to.0.y() - from.0.y()) * from.1.y
            + (to.0.z() - from.0.z()) * from.1.z
    };
    // Material sense: outward laterals have rims pointing TOWARD each
    // other's centers; cavity walls (reversed, PR-KV6a washers) point AWAY.
    let reversed = matches!(face.surface, Some(Surface::Cylinder { reversed: true, .. }));
    let (bot, top) = match (
        reversed,
        toward(&rim_a, &rim_b) > 0.0,
        toward(&rim_b, &rim_a) > 0.0,
    ) {
        // Outward: BOTH rims traverse toward each other (the KV5a shape);
        // the walk-order first is the frame rim.
        (false, true, _) => (rim_a, rim_b),
        (false, false, true) => (rim_b, rim_a),
        // Reversed: both rims point away; the frame rim is the walk-order
        // first (deterministic), and the SAME quad index pattern then winds
        // inward (tangent CCW around an away-pointing axis × toward-top).
        (true, false, false) => (rim_a, rim_b),
        _ => {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "cylinder rim orientations disagree with the material sense",
            });
        }
    };
    let (cb, _nub, _radius, _anchor) = bot;
    let ct = top.0;

    // PR-KV7: each row is sampled with BITWISE the frame its adjacent cap
    // uses — `circle_frame(center, NEG(rim half-edge normal), rim anchor)`.
    // The cap's full-circle half-edge carries the exact negation of the
    // lateral's (a validated twin invariant) and the same anchor vertex, so
    // the cap row and the lateral row are bit-identical position sequences:
    // cross-face watertightness by construction, independent of whether the
    // two rims' anchors sit on exactly the same ruling (recovered boolean
    // outputs guarantee anchor alignment only within the recovery band;
    // the pre-KV7 single-frame scheme cracked there at f32 granularity).
    // The two rows' cap frames always advance OPPOSITELY around the
    // bottom→top axis, so one row is index-reversed to align the strip.
    let sample_row = |row: &(Point3, UnitVector3, f64, Point3),
                      out: &mut RenderMesh|
     -> Result<[f64; 3], KernelV2Error> {
        let (c0, nu, r, anc) = *row;
        let cap_nu = UnitVector3 {
            x: -nu.x,
            y: -nu.y,
            z: -nu.z,
        };
        let Some((e1, e2)) = circle_frame(c0, cap_nu, anc) else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "degenerate circle frame (anchor does not span a radial direction)",
            });
        };
        for k in 0..n_seg {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n_seg as f64);
            let (s, c) = theta.sin_cos();
            let radial = [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ];
            out.positions.extend_from_slice(&[
                c0.x() + r * radial[0],
                c0.y() + r * radial[1],
                c0.z() + r * radial[2],
            ]);
            if reversed {
                out.normals
                    .extend_from_slice(&[-radial[0], -radial[1], -radial[2]]);
            } else {
                out.normals.extend_from_slice(&radial);
            }
        }
        // The row's advance direction: CCW around cap_nu.
        Ok([cap_nu.x, cap_nu.y, cap_nu.z])
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;
    let d_bot = sample_row(&bot, out)?; // rows: bottom [base..base+n)
    let d_top = sample_row(&top, out)?; // top [base+n..base+2n)

    // Align both rows to advance CCW around the bottom→top axis: re-index
    // the row whose cap frame advances the other way (k → (n−k) mod n; the
    // positions are untouched, only the strip indexing).
    let axis_up = [ct.x() - cb.x(), ct.y() - cb.y(), ct.z() - cb.z()];
    let along = |d: &[f64; 3]| d[0] * axis_up[0] + d[1] * axis_up[1] + d[2] * axis_up[2];
    let idx_b = |k: u32| -> u32 {
        if along(&d_bot) >= 0.0 {
            base + (k % n)
        } else {
            base + ((n - (k % n)) % n)
        }
    };
    let idx_t = |k: u32| -> u32 {
        if along(&d_top) >= 0.0 {
            base + n + (k % n)
        } else {
            base + n + ((n - (k % n)) % n)
        }
    };
    for k in 0..n {
        let (bk, bk1, tk, tk1) = (idx_b(k), idx_b(k + 1), idx_t(k), idx_t(k + 1));
        if reversed {
            // Cavity sense: wind inward.
            out.indices.extend_from_slice(&[bk, tk1, bk1]);
            out.indices.extend_from_slice(&[bk, tk, tk1]);
        } else {
            // CCW-around-axis bottom row + axis toward the top row ⇒ these
            // wind with outward normals (∝ tangent × axis = radial).
            out.indices.extend_from_slice(&[bk, bk1, tk1]);
            out.indices.extend_from_slice(&[bk, tk1, tk]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Tessellate a canonical [`Surface::Cone`] frustum band (KV6c increment 3).
///
/// The two full-circle rims sit at DIFFERENT radii, so sampling each row at
/// its own rim radius/center — exactly as [`tessellate_cylinder_lateral`] —
/// yields the frustum strip directly; only the surface NORMAL differs. The
/// outward cone normal is `cos(α)·r̂ − sin(α)·axis` (the radial tilted back
/// toward the apex by the half-angle α; → r̂ as α→0, the cylinder limit),
/// negated for the cavity (`reversed`) sense. Rows are sampled with the
/// adjacent cap's BITWISE circle frame, so the band is watertight against its
/// caps by construction — the same PR-KV7 scheme the cylinder lateral uses.
fn tessellate_cone_lateral(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let (apex, half_angle, axis_dir, reversed) = match face.surface {
        Some(Surface::Cone {
            apex,
            half_angle,
            axis_dir,
            reversed,
        }) => (apex, half_angle, axis_dir, reversed),
        _ => {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "tessellate_cone_lateral called on a non-cone face",
            })
        }
    };
    let (sa, ca) = half_angle.sin_cos();
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut rims = Vec::new();
    for &h in &hes {
        let he = arena.half_edge(h)?;
        if let Curve::Circle {
            center,
            normal,
            radius,
        } = he.curve
        {
            rims.push((center, normal, radius, arena.vertex(he.origin)?.point));
        }
    }

    // KV6 slice 2B: the APEX form — a single base rim, the apex an interior
    // singular point. The base ring is sampled with the cap's bitwise frame
    // (watertight against the disc cap, the PR-KV7 scheme); the "top row" is
    // n_seg copies of the apex point carrying per-azimuth cone normals, and
    // the same bottom-row index transform as the 2-rim strip is applied so
    // each fan triangle winds outward exactly as the strip's first triangle
    // does. Only the outward solid sense has a producer; a reversed apex
    // cavity is rejected typed (matching `validate_cone_face`).
    if let [rim] = rims[..] {
        if reversed {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "apex-cone cavity (reversed) is outside the KV6c vocabulary",
            });
        }
        let (c0, nu, r, anc) = rim;
        let range_start = out.indices.len() as u32;
        let base = out.num_vertices() as u32;
        let n = n_seg;
        // Base ring: identical sampling to `sample_row` (cap frame).
        let cap_nu = UnitVector3 {
            x: -nu.x,
            y: -nu.y,
            z: -nu.z,
        };
        let Some((e1, e2)) = circle_frame(c0, cap_nu, anc) else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "degenerate circle frame (anchor does not span a radial direction)",
            });
        };
        let radial_at = |k: u32| {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            let (s, c) = theta.sin_cos();
            [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ]
        };
        for k in 0..n {
            let radial = radial_at(k);
            out.positions.extend_from_slice(&[
                c0.x() + r * radial[0],
                c0.y() + r * radial[1],
                c0.z() + r * radial[2],
            ]);
            out.normals.extend_from_slice(&[
                ca * radial[0] - sa * axis_dir.x,
                ca * radial[1] - sa * axis_dir.y,
                ca * radial[2] - sa * axis_dir.z,
            ]);
        }
        // Apex row: bit-identical apex positions, per-azimuth normals.
        for k in 0..n {
            let radial = radial_at(k);
            out.positions
                .extend_from_slice(&[apex.x(), apex.y(), apex.z()]);
            out.normals.extend_from_slice(&[
                ca * radial[0] - sa * axis_dir.x,
                ca * radial[1] - sa * axis_dir.y,
                ca * radial[2] - sa * axis_dir.z,
            ]);
        }
        // Same orientation logic as the 2-rim strip with axis_up = apex − c0:
        // the fan triangle [bk, bk1, apex(k)] is the strip's [bk, bk1, tk1]
        // with the degenerate second triangle dropped.
        let axis_up = [apex.x() - c0.x(), apex.y() - c0.y(), apex.z() - c0.z()];
        let along = cap_nu.x * axis_up[0] + cap_nu.y * axis_up[1] + cap_nu.z * axis_up[2];
        let idx = |k: u32| -> u32 {
            if along >= 0.0 {
                k % n
            } else {
                (n - (k % n)) % n
            }
        };
        for k in 0..n {
            let (bk, bk1) = (base + idx(k), base + idx(k + 1));
            let ak = base + n + idx(k);
            out.indices.extend_from_slice(&[bk, bk1, ak]);
        }
        out.face_ranges.push(FaceRange {
            face: fid,
            start: range_start,
            count: out.indices.len() as u32 - range_start,
        });
        return Ok(());
    }

    let [rim_a, rim_b] = rims[..] else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "cone lateral must be bounded by exactly two full-circle rims (KV6c)",
        });
    };
    let toward = |from: &(Point3, UnitVector3, f64, Point3),
                  to: &(Point3, UnitVector3, f64, Point3)| {
        (to.0.x() - from.0.x()) * from.1.x
            + (to.0.y() - from.0.y()) * from.1.y
            + (to.0.z() - from.0.z()) * from.1.z
    };
    // Material sense: identical to the cylinder lateral (rim traversal axes
    // point toward each other for an outward band, away for a cavity bore).
    let (bot, top) = match (
        reversed,
        toward(&rim_a, &rim_b) > 0.0,
        toward(&rim_b, &rim_a) > 0.0,
    ) {
        (false, true, _) => (rim_a, rim_b),
        (false, false, true) => (rim_b, rim_a),
        (true, false, false) => (rim_a, rim_b),
        _ => {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "cone rim orientations disagree with the material sense",
            });
        }
    };
    let (cb, _nub, _radius, _anchor) = bot;
    let ct = top.0;

    let sample_row = |row: &(Point3, UnitVector3, f64, Point3),
                      out: &mut RenderMesh|
     -> Result<[f64; 3], KernelV2Error> {
        let (c0, nu, r, anc) = *row;
        let cap_nu = UnitVector3 {
            x: -nu.x,
            y: -nu.y,
            z: -nu.z,
        };
        let Some((e1, e2)) = circle_frame(c0, cap_nu, anc) else {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "degenerate circle frame (anchor does not span a radial direction)",
            });
        };
        for k in 0..n_seg {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n_seg as f64);
            let (s, c) = theta.sin_cos();
            let radial = [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ];
            out.positions.extend_from_slice(&[
                c0.x() + r * radial[0],
                c0.y() + r * radial[1],
                c0.z() + r * radial[2],
            ]);
            // Cone normal: cos(α)·r̂ − sin(α)·axis (negated for the cavity).
            let mut nrm = [
                ca * radial[0] - sa * axis_dir.x,
                ca * radial[1] - sa * axis_dir.y,
                ca * radial[2] - sa * axis_dir.z,
            ];
            if reversed {
                nrm = [-nrm[0], -nrm[1], -nrm[2]];
            }
            out.normals.extend_from_slice(&nrm);
        }
        Ok([cap_nu.x, cap_nu.y, cap_nu.z])
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;
    let d_bot = sample_row(&bot, out)?;
    let d_top = sample_row(&top, out)?;

    let axis_up = [ct.x() - cb.x(), ct.y() - cb.y(), ct.z() - cb.z()];
    let along = |d: &[f64; 3]| d[0] * axis_up[0] + d[1] * axis_up[1] + d[2] * axis_up[2];
    let idx_b = |k: u32| -> u32 {
        if along(&d_bot) >= 0.0 {
            base + (k % n)
        } else {
            base + ((n - (k % n)) % n)
        }
    };
    let idx_t = |k: u32| -> u32 {
        if along(&d_top) >= 0.0 {
            base + n + (k % n)
        } else {
            base + n + ((n - (k % n)) % n)
        }
    };
    for k in 0..n {
        let (bk, bk1, tk, tk1) = (idx_b(k), idx_b(k + 1), idx_t(k), idx_t(k + 1));
        if reversed {
            out.indices.extend_from_slice(&[bk, tk1, bk1]);
            out.indices.extend_from_slice(&[bk, tk, tk1]);
        } else {
            out.indices.extend_from_slice(&[bk, bk1, tk1]);
            out.indices.extend_from_slice(&[bk, tk1, tk]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Tessellate a [`Surface::Torus`] lateral (KV6d): a partial torus (bent tube)
/// as a (θ × φ) quad grid. θ runs over the sweep `α`, φ over the profile circle.
/// The θ=0 / θ=α rings reproduce the start/end profile circles bit-for-bit
/// (same φ table at `n_seg`), so the band is watertight against its two disk
/// caps as position sets. The θ=0 reference `w0` and the sweep `α` are recovered
/// from the seam arc (the φ=0 longitude: radius major+minor, normal +axis).
fn tessellate_torus_lateral(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Torus {
        center,
        axis_dir,
        major_radius: r_maj,
        minor_radius: r_min,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_torus_lateral on a non-torus face",
        });
    };
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };
    let ax = [axis_dir.x, axis_dir.y, axis_dir.z];
    let c = [center.x(), center.y(), center.z()];

    // Recover (w0, α) from the +axis seam arc (radius major+minor). The
    // CLOSED torus (KV6d full turn, spec `kv6d_closed_torus_revolve.md`)
    // has no seam ARC — its toroidal seam is the closed outer-equator
    // CIRCLE; anchor θ = 0 at its seam vertex and sweep the full 2π with
    // wrapped θ rows.
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut seam = None;
    let mut closed = false;
    for &h in &hes {
        let he = arena.half_edge(h)?;
        if let Curve::Arc { radius, normal, .. } = he.curve {
            if (radius - (r_maj + r_min)).abs() <= 1e-9 * (1.0 + r_maj + r_min)
                && (normal.x * ax[0] + normal.y * ax[1] + normal.z * ax[2]) > 0.0
            {
                let v0 = arena.vertex(he.origin)?.point;
                let dest = arena.half_edge(he.next)?.origin;
                seam = Some((v0, arena.vertex(dest)?.point));
                break;
            }
        }
    }
    if seam.is_none() {
        for &h in &hes {
            let he = arena.half_edge(h)?;
            if let Curve::Circle { radius, normal, .. } = he.curve {
                if (radius - (r_maj + r_min)).abs() <= 1e-9 * (1.0 + r_maj + r_min)
                    && (normal.x * ax[0] + normal.y * ax[1] + normal.z * ax[2]) > 0.0
                {
                    let v0 = arena.vertex(he.origin)?.point;
                    seam = Some((v0, v0));
                    closed = true;
                    break;
                }
            }
        }
    }
    let Some((v0, valpha)) = seam else {
        return Err(fail("torus lateral missing its +axis seam arc"));
    };
    let wv = [v0.x() - c[0], v0.y() - c[1], v0.z() - c[2]];
    let along = wv[0] * ax[0] + wv[1] * ax[1] + wv[2] * ax[2];
    let wr = [
        wv[0] - along * ax[0],
        wv[1] - along * ax[1],
        wv[2] - along * ax[2],
    ];
    let wl = (wr[0] * wr[0] + wr[1] * wr[1] + wr[2] * wr[2]).sqrt();
    if !(wl.is_finite() && wl > 0.0) {
        return Err(fail("degenerate torus θ=0 reference"));
    }
    let w0 = [wr[0] / wl, wr[1] / wl, wr[2] / wl];
    let alpha = if closed {
        2.0 * PI
    } else {
        crate::geom::ccw_sweep(center, ax, v0, valpha).ok_or(fail("degenerate torus sweep"))?
    };
    let m0 = [
        ax[1] * w0[2] - ax[2] * w0[1],
        ax[2] * w0[0] - ax[0] * w0[2],
        ax[0] * w0[1] - ax[1] * w0[0],
    ];

    // φ matches the caps (n_seg); θ steps keep a comparable chord at radius R+r.
    let n_phi = n_seg.max(3) as usize;
    let n_theta = {
        let per = (2.0 * PI / n_seg as f64) * r_min / (r_maj + r_min);
        ((alpha / per).ceil() as usize).max(if closed { 3 } else { 2 })
    };
    // Closed torus: the θ = 2π row IS the θ = 0 row — emit n_theta rows and
    // wrap the row index instead of duplicating the seam ring.
    let n_rows = if closed { n_theta } else { n_theta + 1 };
    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let point = |theta: f64, phi: f64| -> ([f64; 3], [f64; 3]) {
        let (st, ct) = theta.sin_cos();
        let wth = [
            ct * w0[0] + st * m0[0],
            ct * w0[1] + st * m0[1],
            ct * w0[2] + st * m0[2],
        ];
        let (sp, cp) = phi.sin_cos();
        let rad = r_maj + r_min * cp;
        let p = [
            c[0] + rad * wth[0] + r_min * sp * ax[0],
            c[1] + rad * wth[1] + r_min * sp * ax[1],
            c[2] + rad * wth[2] + r_min * sp * ax[2],
        ];
        let mut nrm = [
            cp * wth[0] + sp * ax[0],
            cp * wth[1] + sp * ax[1],
            cp * wth[2] + sp * ax[2],
        ];
        if reversed {
            nrm = [-nrm[0], -nrm[1], -nrm[2]];
        }
        (p, nrm)
    };
    for i in 0..n_rows {
        let theta = alpha * (i as f64) / (n_theta as f64);
        for j in 0..n_phi {
            let phi = 2.0 * PI * (j as f64) / (n_phi as f64);
            let (p, nrm) = point(theta, phi);
            out.positions.extend_from_slice(&p);
            out.normals.extend_from_slice(&nrm);
        }
    }
    let idx = |i: usize, j: usize| base + ((i % n_rows) * n_phi + (j % n_phi)) as u32;
    let pos = |out: &RenderMesh, vi: u32| {
        let k = vi as usize * 3;
        [out.positions[k], out.positions[k + 1], out.positions[k + 2]]
    };
    // Emit a triangle, winding it so its geometric normal agrees with the
    // analytic torus outward normal at the centroid (reversed-aware).
    let emit = |a: u32, b: u32, cc: u32, out: &mut RenderMesh| {
        let (pa, pb, pc) = (pos(out, a), pos(out, b), pos(out, cc));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let cen = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let d = [cen[0] - c[0], cen[1] - c[1], cen[2] - c[2]];
        let t = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
        let rv = [d[0] - t * ax[0], d[1] - t * ax[1], d[2] - t * ax[2]];
        let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
            .sqrt()
            .max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let mut on = [
            cen[0] - (c[0] + r_maj * rhat[0]),
            cen[1] - (c[1] + r_maj * rhat[1]),
            cen[2] - (c[2] + r_maj * rhat[2]),
        ];
        if reversed {
            on = [-on[0], -on[1], -on[2]];
        }
        if gn[0] * on[0] + gn[1] * on[1] + gn[2] * on[2] >= 0.0 {
            out.indices.extend_from_slice(&[a, b, cc]);
        } else {
            out.indices.extend_from_slice(&[a, cc, b]);
        }
    };
    for i in 0..n_theta {
        for j in 0..n_phi {
            let (a, b, cc, d) = (idx(i, j), idx(i, j + 1), idx(i + 1, j + 1), idx(i + 1, j));
            emit(a, b, cc, out);
            emit(a, cc, d, out);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// KV6d increment 5b2: render-tessellate a boolean-OUTPUT torus PATCH — a
/// `Surface::Torus` face whose boundary is the trimmed intersection loop (a
/// chord polyline, possibly with surviving seam-arc spans), NOT the structured
/// seam-arc loop the modeling tessellator [`tessellate_torus_lateral`] needs.
///
/// The torus is degree-4 and NOT developable, so the cylinder patch's
/// unroll+ear-clip does not transfer; instead we delegate to yang-rs's UV-CDT
/// consumer [`yang_rs::tessellate_torus_patch`], which projects the boundary
/// into the `(meridian, longitude)` plane, constrained-Delaunay-triangulates
/// with interior Steiner points (to bound chord error), and maps back to 3D
/// with the boundary vertices kept EXACT (conformal with the neighbouring
/// faces, which sample the same arc/segment edges twin-canonically). We then
/// emit with the analytic outward torus normal, winding each triangle to agree.
fn tessellate_torus_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Torus {
        center,
        axis_dir,
        major_radius: r_maj,
        minor_radius: r_min,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_torus_patch on a non-torus face",
        });
    };
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };
    let c = [center.x(), center.y(), center.z()];
    let ax = [axis_dir.x, axis_dir.y, axis_dir.z];

    // Gather a loop as an ordered 3D polyline: each half-edge's origin, then its
    // arc interior samples (empty for a line segment), in walk order. Arc
    // samples are twin-canonical, so a surviving seam arc shared with a cap is
    // sampled identically on both faces.
    let gather = |loop_id| -> Result<Vec<Point3>, KernelV2Error> {
        let hes = arena.loop_half_edges(loop_id)?;
        let mut pts: Vec<Point3> = Vec::with_capacity(hes.len());
        for &h in &hes {
            let he = arena.half_edge(h)?;
            pts.push(arena.vertex(he.origin)?.point);
            pts.extend(arc_interior_samples(arena, h, n_seg)?);
        }
        Ok(pts)
    };
    let boundary = gather(face.outer_loop)?;
    if boundary.len() < 3 {
        return Err(fail("torus patch boundary has fewer than 3 vertices"));
    }
    // Interior holes (e.g. a window bitten out of the tube middle) become CDT
    // holes in the (u, v) parameter plane.
    let mut holes: Vec<Vec<Point3>> = Vec::with_capacity(face.inner_loops.len());
    for &lid in &face.inner_loops {
        let h = gather(lid)?;
        if h.len() < 3 {
            return Err(fail("torus patch interior loop has fewer than 3 vertices"));
        }
        holes.push(h);
    }

    // Triangle-area budget in arc-length² (the consumer scales (u,v) to
    // arc-length before refining): match the meridian grid spacing of the
    // structured tessellator (tube circumference 2π·r_min over n_seg).
    let seg = 2.0 * PI * r_min / f64::from(n_seg.max(3));
    let max_area = seg * seg;

    let axis_v = Vector3::new(ax[0], ax[1], ax[2]);
    let Some((verts, tris)) =
        yang_rs::tessellate_torus_patch(center, axis_v, r_maj, r_min, &boundary, &holes, max_area)
    else {
        return Err(fail(
            "torus patch UV-CDT failed (self-intersecting projection / seam-crossing patch)",
        ));
    };

    // Analytic outward torus normal at a point p (reversed-aware): project to
    // the tube centre circle, take p − tubeCentre.
    let normal_at = |p: [f64; 3]| -> [f64; 3] {
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        let t = d[0] * ax[0] + d[1] * ax[1] + d[2] * ax[2];
        let rv = [d[0] - t * ax[0], d[1] - t * ax[1], d[2] - t * ax[2]];
        let rl = (rv[0] * rv[0] + rv[1] * rv[1] + rv[2] * rv[2])
            .sqrt()
            .max(1e-300);
        let rhat = [rv[0] / rl, rv[1] / rl, rv[2] / rl];
        let tube = [
            c[0] + r_maj * rhat[0],
            c[1] + r_maj * rhat[1],
            c[2] + r_maj * rhat[2],
        ];
        let mut n = [p[0] - tube[0], p[1] - tube[1], p[2] - tube[2]];
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-300);
        n = [n[0] / nl, n[1] / nl, n[2] / nl];
        if reversed {
            n = [-n[0], -n[1], -n[2]];
        }
        n
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    for v in &verts {
        let p = [v.x(), v.y(), v.z()];
        out.positions.extend_from_slice(&p);
        out.normals.extend_from_slice(&normal_at(p));
    }
    let pos = |out: &RenderMesh, vi: u32| {
        let k = vi as usize * 3;
        [out.positions[k], out.positions[k + 1], out.positions[k + 2]]
    };
    // Wind each triangle so its geometric normal agrees with the analytic
    // outward normal at the centroid (reversed-aware).
    for t in &tris {
        let (a, b, cc) = (base + t[0], base + t[1], base + t[2]);
        let (pa, pb, pc) = (pos(out, a), pos(out, b), pos(out, cc));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let cen = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let on = normal_at(cen);
        if gn[0] * on[0] + gn[1] * on[1] + gn[2] * on[2] >= 0.0 {
            out.indices.extend_from_slice(&[a, b, cc]);
        } else {
            out.indices.extend_from_slice(&[a, cc, b]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Is this [`Surface::Sphere`] face the CLOSED modeling sphere — the outer
/// loop exactly the meridian seam-Arc twin pair, no inner loops (KV6d
/// increment 2, spec `kv6d_sphere_revolve.md`)? Anything else is a
/// boolean-output trimmed patch.
fn sphere_face_is_closed(arena: &BrepArena, fid: FaceId) -> Result<bool, KernelV2Error> {
    let face = arena.face(fid)?;
    if !face.inner_loops.is_empty() {
        return Ok(false);
    }
    let hes = arena.loop_half_edges(face.outer_loop)?;
    if hes.len() != 2 {
        return Ok(false);
    }
    let both_arcs = matches!(arena.half_edge(hes[0])?.curve, Curve::Arc { .. })
        && matches!(arena.half_edge(hes[1])?.curve, Curve::Arc { .. });
    Ok(both_arcs && arena.half_edge(hes[0])?.twin == hes[1])
}

/// Tessellate the CLOSED [`Surface::Sphere`] face (KV6d increment 2): a z-up
/// latitude/longitude grid. Poles are emitted ONCE (single vertex each, fan
/// closure); the longitude wrap reuses column 0 via modular indexing (no
/// duplicated seam column) — watertight by construction, mirroring the
/// closed-torus θ-row wrap.
fn tessellate_sphere_closed(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Sphere {
        center,
        radius: r,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_sphere_closed on a non-sphere face",
        });
    };
    let c = [center.x(), center.y(), center.z()];
    let n_lon = n_seg.max(3) as usize;
    let n_lat = ((n_seg / 2).max(2)) as usize;

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let sign = if reversed { -1.0 } else { 1.0 };
    let push = |p: [f64; 3], out: &mut RenderMesh| {
        out.positions.extend_from_slice(&p);
        let n = [
            sign * (p[0] - c[0]) / r,
            sign * (p[1] - c[1]) / r,
            sign * (p[2] - c[2]) / r,
        ];
        out.normals.extend_from_slice(&n);
    };
    // Vertex layout: south pole, north pole, then interior rings
    // j = 1..n_lat (bottom to top), each n_lon columns.
    push([c[0], c[1], c[2] - r], out);
    push([c[0], c[1], c[2] + r], out);
    for j in 1..n_lat {
        let v = -PI / 2.0 + PI * (j as f64) / (n_lat as f64);
        let (sv, cv) = v.sin_cos();
        for i in 0..n_lon {
            let u = 2.0 * PI * (i as f64) / (n_lon as f64);
            let (su, cu) = u.sin_cos();
            push([c[0] + r * cv * cu, c[1] + r * cv * su, c[2] + r * sv], out);
        }
    }
    let (south, north) = (base, base + 1);
    let ring = |j: usize, i: usize| base + 2 + ((j - 1) * n_lon + (i % n_lon)) as u32;
    // Winding: emitted CCW-outward by construction (u eastward, v northward);
    // a reversed (cavity) face flips.
    let emit = |a: u32, b: u32, cc: u32, out: &mut RenderMesh| {
        if reversed {
            out.indices.extend_from_slice(&[a, cc, b]);
        } else {
            out.indices.extend_from_slice(&[a, b, cc]);
        }
    };
    for i in 0..n_lon {
        emit(south, ring(1, i + 1), ring(1, i), out);
        emit(north, ring(n_lat - 1, i), ring(n_lat - 1, i + 1), out);
    }
    for j in 1..n_lat - 1 {
        for i in 0..n_lon {
            let (a, b) = (ring(j, i), ring(j, i + 1));
            let (d, cc) = (ring(j + 1, i), ring(j + 1, i + 1));
            emit(a, b, cc, out);
            emit(a, cc, d, out);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Render-tessellate a boolean-OUTPUT sphere PATCH (KV6d increment 2) — a
/// [`Surface::Sphere`] face whose boundary is the trimmed intersection loop
/// (plane∩sphere circle arcs + chord polylines) instead of the seam-arc pair
/// the modeling tessellator needs.
///
/// The sphere is not developable, so like the torus this delegates to a
/// yang-rs UV consumer ([`yang_rs::tessellate_sphere_patch`]): project the
/// boundary into the (longitude, latitude) plane, CDT with interior Steiner
/// refinement, and (for a pole-containing patch) bridge the wrapping loop to
/// the pole. Boundary polylines pass through EXACTLY, so the patch stays
/// watertight against its planar neighbors; each triangle is wound to agree
/// with the analytic outward sphere normal.
fn tessellate_sphere_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let face = arena.face(fid)?;
    let Some(Surface::Sphere {
        center,
        radius: r,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "tessellate_sphere_patch on a non-sphere face",
        });
    };
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };
    let c = [center.x(), center.y(), center.z()];

    // Gather each loop as an ordered 3D polyline (walk order; arc interior
    // samples are twin-canonical — shared with the adjacent planar face).
    // A FULL-circle boundary edge (a hemisphere's rim, shared with a disk
    // cap) is densified with BITWISE the frame the cap tessellator uses —
    // `circle_frame(center, NEG(this normal), anchor)`, reversed into walk
    // order (the cylinder-lateral recipe) — so the shared rim positions are
    // bit-identical across the two faces.
    let gather = |loop_id| -> Result<Vec<Point3>, KernelV2Error> {
        let hes = arena.loop_half_edges(loop_id)?;
        let mut pts: Vec<Point3> = Vec::with_capacity(hes.len());
        for &h in &hes {
            let he = arena.half_edge(h)?;
            pts.push(arena.vertex(he.origin)?.point);
            if let Curve::Circle {
                center: cc,
                normal,
                radius: cr,
            } = he.curve
            {
                let anchor = arena.vertex(he.origin)?.point;
                let cap_n = crate::arena::UnitVector3 {
                    x: -normal.x,
                    y: -normal.y,
                    z: -normal.z,
                };
                let Some((e1, e2)) = circle_frame(cc, cap_n, anchor) else {
                    return Err(fail("degenerate circle frame on a sphere patch rim"));
                };
                let n = n_seg.max(3) as usize;
                for k in (1..n).rev() {
                    let theta = 2.0 * PI * (k as f64) / (n as f64);
                    let (s, co) = theta.sin_cos();
                    pts.push(Point3::new(
                        cc.x() + cr * (co * e1[0] + s * e2[0]),
                        cc.y() + cr * (co * e1[1] + s * e2[1]),
                        cc.z() + cr * (co * e1[2] + s * e2[2]),
                    ));
                }
            } else {
                pts.extend(arc_interior_samples(arena, h, n_seg)?);
            }
        }
        Ok(pts)
    };
    let boundary = gather(face.outer_loop)?;
    if boundary.len() < 3 {
        return Err(fail("sphere patch boundary has fewer than 3 vertices"));
    }
    let mut holes: Vec<Vec<Point3>> = Vec::with_capacity(face.inner_loops.len());
    for &lid in &face.inner_loops {
        let h = gather(lid)?;
        if h.len() < 3 {
            return Err(fail("sphere patch interior loop has fewer than 3 vertices"));
        }
        holes.push(h);
    }

    // Triangle-area budget in arc-length²: match the equator chord spacing of
    // the structured tessellator (the torus-patch recipe).
    let seg = 2.0 * PI * r / f64::from(n_seg.max(3));
    let max_area = seg * seg;

    let Some((verts, tris)) =
        yang_rs::tessellate_sphere_patch(center, r, reversed, &boundary, &holes, max_area)
    else {
        return Err(fail(
            "sphere patch UV-CDT failed (multi-wrap / pole-crossing boundary — later slice)",
        ));
    };

    // Analytic outward sphere normal (reversed-aware).
    let normal_at = |p: [f64; 3]| -> [f64; 3] {
        let mut n = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-300);
        n = [n[0] / nl, n[1] / nl, n[2] / nl];
        if reversed {
            n = [-n[0], -n[1], -n[2]];
        }
        n
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    for v in &verts {
        let p = [v.x(), v.y(), v.z()];
        out.positions.extend_from_slice(&p);
        out.normals.extend_from_slice(&normal_at(p));
    }
    let pos = |out: &RenderMesh, vi: u32| {
        let k = vi as usize * 3;
        [out.positions[k], out.positions[k + 1], out.positions[k + 2]]
    };
    // Wind each triangle so its geometric normal agrees with the analytic
    // outward normal at the centroid (reversed-aware).
    for t in &tris {
        let (a, b, cc) = (base + t[0], base + t[1], base + t[2]);
        let (pa, pb, pc) = (pos(out, a), pos(out, b), pos(out, cc));
        let e1 = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let e2 = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let gn = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let cen = [
            (pa[0] + pb[0] + pc[0]) / 3.0,
            (pa[1] + pb[1] + pc[1]) / 3.0,
            (pa[2] + pb[2] + pc[2]) / 3.0,
        ];
        let on = normal_at(cen);
        if gn[0] * on[0] + gn[1] * on[1] + gn[2] * on[2] >= 0.0 {
            out.indices.extend_from_slice(&[a, b, cc]);
        } else {
            out.indices.extend_from_slice(&[a, cc, b]);
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// PR-KV5b: partial cylinder patches (boolean outputs)
// ---------------------------------------------------------------------------

/// Boundary-edge geometry kind in the unrolled patch triangulation: what a
/// refinement split of the edge must follow.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PatchEdgeKind {
    /// A straight 3D segment (boundary chords/rulings, seam bridges, hole
    /// bridges): splits at the 3D midpoint — exactly collinear, so a
    /// one-sided split against a neighboring face is closure-safe
    /// (T-junction).
    Chord,
    /// An arc sub-edge, already sampled at the chord bound (`Δu ≤ w`): a
    /// split is never needed; requesting one is a refinement bug and fails
    /// loudly rather than cracking the shared circle.
    ArcSample,
    /// A triangulation-interior edge: splits land ON the analytic surface.
    Interior,
}

/// A node of the unrolled patch triangulation: unrolled coordinates
/// `(u = sense·θ·r, v = h)`, the 3D position, and its (lazily emitted)
/// render-vertex id.
struct PatchNode {
    p2: Point2,
    pos: [f64; 3],
}

/// The single canonical PARTIAL-cylinder-patch routine (PR-KV5b): a
/// `Surface::Cylinder` face bounded by arc/segment loops (a yang boolean
/// output — barrel segments between intersection circles, faceted original
/// rims, ruled windows, `reversed` cavity walls).
///
/// Algorithm — the developable analog of the planar routine, sharing its
/// exact-predicate constrained-Delaunay triangulation core:
///
/// 1. **Unroll** every loop into `(u, v) = (sense·θ·r, h)` coordinates
///    (`sense = −1` for `reversed`, mirroring the frame so material still
///    winds CCW and emitted triangles face outward). Arc edges enter as
///    their chord-bound samples ([`arc_interior_samples`] — bitwise
///    twin-symmetric, so the neighboring face sharing the circle emits
///    identical positions); per-edge angular steps come from the arcs'
///    exact sweeps and the segments' principal values.
/// 2. **Cut axis-wrapping loops**: a barrel segment's two rim loops wrap
///    the axis (net ±2π) and have no closed unrolled image; they are cut
///    open along a bridge pair (the universal-cover seam) chosen
///    exactly-unblocked, forming one simple polygon spanning `2πr` in `u`.
/// 3. **Triangulate** the outer ring with its zero-wrap hole loops passed
///    natively via [`cdt_polygon_with_holes`] (spec
///    `kv2_cdt_triangulation_core` §3, branches C1–C4): the max-min-angle
///    constrained-Delaunay triangulation with exact-predicate flips, so no
///    sliver an alternative triangulation avoids is minted.
/// 4. **Refine** to the chord bound: any triangulation edge spanning more
///    than one facet width in `u` is bisected (conforming — both incident
///    triangles split), interior split points landing exactly on the
///    analytic surface, boundary chord splits on the chord (collinear ⇒
///    closure-safe against neighbors). A triangle's `u`-span is then at
///    most two facet widths, so the radial sagitta is bounded by the
///    documented band at the doubled angle.
/// 5. **Emit** with exact analytic radial normals (negated for `reversed`).
///
/// B2/B3 render-precision degeneracy predicate (spec
/// `kv2_cdt_triangulation_core` §3 G0/G1): `true` iff the triangle collapses
/// at f32 render precision — either two vertices are bitwise-equal after f32
/// rounding (B2) or the f32-arithmetic cross product is exactly zero (B3).
/// Shared by the cylinder-patch gate (G0) and the planar gate (G1); each
/// caller supplies its own typed `TessellationFailed` reason string.
fn f32_render_degenerate(pa: [f64; 3], pb: [f64; 3], pc: [f64; 3]) -> bool {
    let k32 = |p: [f64; 3]| {
        [
            (p[0] as f32).to_bits(),
            (p[1] as f32).to_bits(),
            (p[2] as f32).to_bits(),
        ]
    };
    let (ka, kb, kc) = (k32(pa), k32(pb), k32(pc));
    if ka == kb || kb == kc || kc == ka {
        return true;
    }
    let f = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
    let (fa, fb, fc) = (f(pa), f(pb), f(pc));
    let uu = [fb[0] - fa[0], fb[1] - fa[1], fb[2] - fa[2]];
    let vv = [fc[0] - fa[0], fc[1] - fa[1], fc[2] - fa[2]];
    let cx = uu[1] * vv[2] - uu[2] * vv[1];
    let cy = uu[2] * vv[0] - uu[0] * vv[2];
    let cz = uu[0] * vv[1] - uu[1] * vv[0];
    cx == 0.0 && cy == 0.0 && cz == 0.0
}

/// M1 grid-degeneracy predicate (spec `kv2_cdt_triangulation_core` §6b): is the
/// triangle's f32-rounded height below the render weld `grid`? The height is
/// computed in the SAME shape as `oracle::check_no_degenerate_triangles`
/// (`height = 2·area / longest_side`, all f32 arithmetic on f32-rounded 3D
/// positions), so a triangle flatter than the grid the watertight oracle welds
/// at is caught here — ~100× coarser than f32 ulp, which is why the bitwise
/// `f32_render_degenerate` (G0/G1) cannot see it.
fn tri_height_below_grid(pa: [f64; 3], pb: [f64; 3], pc: [f64; 3], grid: f64) -> bool {
    let f = |p: [f64; 3]| [p[0] as f32, p[1] as f32, p[2] as f32];
    let (fa, fb, fc) = (f(pa), f(pb), f(pc));
    let ax = fb[0] - fa[0];
    let ay = fb[1] - fa[1];
    let az = fb[2] - fa[2];
    let bx = fc[0] - fa[0];
    let by = fc[1] - fa[1];
    let bz = fc[2] - fa[2];
    let cx = ay * bz - az * by;
    let cy = az * bx - ax * bz;
    let cz = ax * by - ay * bx;
    let area = (cx * cx + cy * cy + cz * cz).sqrt() / 2.0;
    let max_side2 = (ax * ax + ay * ay + az * az)
        .max(bx * bx + by * by + bz * bz)
        .max((bx - ax) * (bx - ax) + (by - ay) * (by - ay) + (bz - az) * (bz - az));
    let height = if max_side2 > 0.0 {
        2.0 * area / max_side2.sqrt()
    } else {
        0.0
    };
    (height as f64) < grid
}

/// Register the undirected index-edges of one ring loop into `set` (skipping a
/// self-pair). Used to build the constraint-edge set the M1 flip pass must not
/// flip (boundary/hole edges are hard constraints, M1b).
fn add_ring_edges(set: &mut std::collections::HashSet<(u32, u32)>, ring: &[u32]) {
    let m = ring.len();
    for i in 0..m {
        let (a, b) = (ring[i], ring[(i + 1) % m]);
        if a != b {
            set.insert((a.min(b), a.max(b)));
        }
    }
}

/// The other triangle sharing the undirected edge `{p, q}` with `tris[ti]`, if
/// any (an interior manifold edge has exactly one such neighbor).
fn other_tri_on_edge(tris: &[[u32; 3]], ti: usize, p: u32, q: u32) -> Option<usize> {
    tris.iter()
        .enumerate()
        .position(|(k, t)| k != ti && t.contains(&p) && t.contains(&q))
}

/// M1 GRID-DEGENERACY TARGETED FLIP PASS (spec `kv2_cdt_triangulation_core`
/// §6b, mechanisms M1/M1b): applied to a CDT output triangle list (indexing the
/// shared 2D/3D pools) BEFORE downstream refinement/emit.
///
/// For each triangle flatter than the render weld grid (`tri_height_below_grid`
/// OR the bitwise `f32_render_degenerate` floor), find its LONGEST 2D edge; if
/// that edge is a boundary/constraint edge, leave it (M1b — input-forced, the
/// bitwise G0/G1 gates remain the loud floor). Otherwise it is an interior
/// diagonal with a neighbor: flip to the other diagonal iff the two triangles
/// form a STRICTLY convex quad (exact orient2d, all four turns strict — this
/// also guarantees both replacements are strictly CCW, so winding is preserved,
/// I4) AND the flip STRICTLY reduces the grid-degenerate count among the two.
/// Each accepted flip strictly lowers the global grid-degenerate count, so the
/// fixpoint terminates in ≤ n flips; the `4·n` budget is a loud tripwire, never
/// a silent loop.
///
/// `p2` / `p3` are the 2D triangulation-frame coordinates and 3D positions
/// indexed by pool index; `is_constraint` reports whether an undirected pool
/// index-edge is a boundary/hole constraint.
fn grid_degeneracy_flip_pass(
    tris: &mut [[u32; 3]],
    p2: &[Point2],
    p3: &[[f64; 3]],
    is_constraint: &dyn Fn(u32, u32) -> bool,
) -> Result<(), &'static str> {
    if tris.is_empty() {
        return Ok(());
    }
    // Invariant: the M1 grid reuses the render weld grid the watertight oracle
    // owns (A3.3 single ownership): grid = (max_abs·FACTOR).max(MIN), max_abs
    // over the FACE'S OWN 3D pool (per-face ≤ mesh-wide; 100× headroom to f32).
    let max_abs = p3
        .iter()
        .flat_map(|p| p.iter())
        .map(|&c| (c as f32).abs())
        .fold(0.0_f32, f32::max) as f64;
    let grid = (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
    let degen = |t: &[u32; 3]| -> bool {
        let (pa, pb, pc) = (p3[t[0] as usize], p3[t[1] as usize], p3[t[2] as usize]);
        f32_render_degenerate(pa, pb, pc) || tri_height_below_grid(pa, pb, pc, grid)
    };
    let len2 = |a: u32, b: u32| -> f64 {
        let (pa, pb) = (p2[a as usize], p2[b as usize]);
        let (dx, dy) = (pa.x() - pb.x(), pa.y() - pb.y());
        dx * dx + dy * dy
    };
    let ccw = |a: u32, b: u32, c: u32| -> bool {
        exact2d::orient2d(p2[a as usize], p2[b as usize], p2[c as usize]) == Ordering::Greater
    };
    let budget = 4 * tris.len();
    let mut flips = 0usize;
    loop {
        let mut applied = false;
        for ti in 0..tris.len() {
            let t = tris[ti];
            if !degen(&t) {
                continue;
            }
            // Longest edge, directed p→q as it appears in the CCW triangle
            // (apex r on its CCW-left).
            let dirs = [(t[0], t[1], t[2]), (t[1], t[2], t[0]), (t[2], t[0], t[1])];
            let (mut p, mut q, mut r) = dirs[0];
            let mut best_l = len2(p, q);
            for &(dp, dq, dr) in &dirs[1..] {
                let l = len2(dp, dq);
                if l > best_l {
                    best_l = l;
                    p = dp;
                    q = dq;
                    r = dr;
                }
            }
            // M1b: longest edge is a boundary/constraint edge — leave it.
            if is_constraint(p, q) {
                continue;
            }
            let Some(tj) = other_tri_on_edge(tris, ti, p, q) else {
                continue;
            };
            let Some(s) = tris[tj].iter().copied().find(|&v| v != p && v != q) else {
                continue;
            };
            // Strictly convex quad [p, s, q, r]: the diagonal p–q flips to r–s.
            if !(ccw(p, s, q) && ccw(s, q, r) && ccw(q, r, p) && ccw(r, p, s)) {
                continue;
            }
            let new1 = [p, s, r];
            let new2 = [s, q, r];
            let before = degen(&t) as usize + degen(&tris[tj]) as usize;
            let after = degen(&new1) as usize + degen(&new2) as usize;
            if after >= before {
                continue;
            }
            // Winding preserved: the convex-quad test above already forces both
            // replacements strictly CCW.
            debug_assert!(ccw(new1[0], new1[1], new1[2]) && ccw(new2[0], new2[1], new2[2]));
            tris[ti] = new1;
            tris[tj] = new2;
            flips += 1;
            if flips > budget {
                return Err("degeneracy flip pass did not converge");
            }
            applied = true;
            break;
        }
        if !applied {
            break;
        }
    }
    Ok(())
}

/// Triangulate one simple ring (with native holes) for the render channel:
/// the exact-predicate flood-fill CDT ([`yang_rs::cdt_polygon_with_holes_floodfill`],
/// spec `kv2_cdt_triangulation_core` §6b M2) followed by the M1 grid-degeneracy
/// flip pass. `p2` / `p3` are the shared 2D/3D pools; `outer` / `holes` index
/// them. Output triangles index the same pool (no new points). A CDT rejection
/// or a non-converging flip pass is a loud typed reason string (never a
/// fallback — P9).
fn triangulate_ring(
    p2: &[Point2],
    p3: &[[f64; 3]],
    outer: &[u32],
    holes: &[Vec<u32>],
) -> Result<Vec<[u32; 3]>, &'static str> {
    let mut tris = yang_rs::cdt_polygon_with_holes_floodfill(p2, outer, holes).map_err(|e| {
        if std::env::var_os("KV2_RING_REJECT_PROBE").is_some() {
            eprintln!(
                "KV2_RING_REJECT_PROBE cdt_err={e:?} outer_len={} holes={} npts={}",
                outer.len(),
                holes.len(),
                p2.len()
            );
            let pts: Vec<(f64, f64)> = outer
                .iter()
                .map(|&i| (p2[i as usize].x(), p2[i as usize].y()))
                .collect();
            eprintln!("KV2_RING_REJECT_PROBE outer_pts={pts:?}");
            for (hi, h) in holes.iter().enumerate() {
                let hp: Vec<(f64, f64)> = h
                    .iter()
                    .map(|&i| (p2[i as usize].x(), p2[i as usize].y()))
                    .collect();
                eprintln!("KV2_RING_REJECT_PROBE hole[{hi}]={hp:?}");
            }
        }
        "ring rejected by CDT (degenerate/self-intersecting)"
    })?;
    let mut cset: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    add_ring_edges(&mut cset, outer);
    for h in holes {
        add_ring_edges(&mut cset, h);
    }
    let is_constraint = |i: u32, j: u32| cset.contains(&(i.min(j), i.max(j)));
    grid_degeneracy_flip_pass(&mut tris, p2, p3, &is_constraint)?;
    Ok(tris)
}

/// The first NON-consecutive pair of ring positions whose pool points are
/// bitwise-identical in 2D (a weakly-simple "pinch": one geometric point
/// visited twice through two distinct pool indices — spec §6b M3). Returns the
/// two ring positions `(i, j)` with `i < j`. A CONSECUTIVE duplicate (a
/// zero-length edge) is intentionally NOT reported — it is left to the CDT
/// (`DuplicateVertex` → loud).
fn find_ring_pinch(p2: &[Point2], outer: &[u32]) -> Option<(usize, usize)> {
    let m = outer.len();
    let bits = |pos: usize| -> (u64, u64) {
        let p = p2[outer[pos] as usize];
        (p.x().to_bits(), p.y().to_bits())
    };
    let mut seen: std::collections::BTreeMap<(u64, u64), usize> = std::collections::BTreeMap::new();
    for pos in 0..m {
        let key = bits(pos);
        if let Some(&prev) = seen.get(&key) {
            let consecutive = pos == prev + 1 || (prev == 0 && pos == m - 1);
            if !consecutive {
                return Some((prev, pos));
            }
        } else {
            seen.insert(key, pos);
        }
    }
    None
}

/// Triangulate a (possibly weakly-simple) ring, splitting it at bitwise-pinch
/// vertices before CDT (spec `kv2_cdt_triangulation_core` §6b M3). A ring
/// visiting one geometric point twice through two distinct pool indices would
/// make spade reject the whole ring (`DuplicateVertex`); instead it is split at
/// the pinch into two sub-rings (each keeping one copy of the pinch position),
/// which recurse. No-op when the ring carries no non-consecutive duplicate
/// (the common case, incl. patch seam duplicates whose 2D positions differ by
/// `span`). Wired into BOTH cores.
fn triangulate_with_pinch_split(
    p2: &[Point2],
    p3: &[[f64; 3]],
    outer: &[u32],
    holes: &[Vec<u32>],
) -> Result<Vec<[u32; 3]>, &'static str> {
    let mut budget = 16usize;
    pinch_split_rec(p2, p3, outer, holes, &mut budget)
}

fn pinch_split_rec(
    p2: &[Point2],
    p3: &[[f64; 3]],
    outer: &[u32],
    holes: &[Vec<u32>],
    budget: &mut usize,
) -> Result<Vec<[u32; 3]>, &'static str> {
    let Some((i, j)) = find_ring_pinch(p2, outer) else {
        return triangulate_ring(p2, p3, outer, holes);
    };
    if *budget == 0 {
        return Err("pinch-ring split budget exhausted");
    }
    *budget -= 1;

    // Sub-rings [i..j] and [j..i] (cyclic); each keeps ONE copy of the pinch.
    let ring_a: Vec<u32> = outer[i..j].to_vec();
    let mut ring_b: Vec<u32> = outer[j..].to_vec();
    ring_b.extend_from_slice(&outer[..i]);
    let pts_a: Vec<Point2> = ring_a.iter().map(|&x| p2[x as usize]).collect();
    let pts_b: Vec<Point2> = ring_b.iter().map(|&x| p2[x as usize]).collect();
    for pts in [&pts_a, &pts_b] {
        if pts.len() < 3 {
            return Err("pinch sub-ring has fewer than 3 vertices");
        }
    }

    // M3 orientation dispatch (spec §6b M3a/M3b/M3c): the exact shoelace sign of
    // the two sub-rings decides whether the split is two material lobes, a
    // keyhole (material minus a tangent hole), or an invalid winding.
    let sign_a = exact2d::signed_area_sign(&pts_a);
    let sign_b = exact2d::signed_area_sign(&pts_b);
    match (sign_a, sign_b) {
        // M3a: CCW + CCW — two material lobes, CDT each separately. Each hole
        // is assigned to the sub-ring strictly containing its first vertex
        // (exact point-in-polygon); a hole in neither is out of scope → loud.
        (Ordering::Greater, Ordering::Greater) => {
            let mut holes_a: Vec<Vec<u32>> = Vec::new();
            let mut holes_b: Vec<Vec<u32>> = Vec::new();
            for h in holes {
                let Some(&first) = h.first() else {
                    continue;
                };
                let q = p2[first as usize];
                if exact2d::point_strictly_inside(q, &pts_a) {
                    holes_a.push(h.clone());
                } else if exact2d::point_strictly_inside(q, &pts_b) {
                    holes_b.push(h.clone());
                } else {
                    return Err("pinch hole not strictly contained in either sub-ring");
                }
            }
            let mut tris = pinch_split_rec(p2, p3, &ring_a, &holes_a, budget)?;
            tris.extend(pinch_split_rec(p2, p3, &ring_b, &holes_b, budget)?);
            Ok(tris)
        }
        // M3b: CCW + CW — keyhole. The CCW sub-ring is the outer; the CW
        // sub-ring is a tangent HOLE (touching the outer at the pinch),
        // passed natively to the flood-fill CDT, which welds the shared
        // pinch vertex. Triangulated area = outer − hole.
        (Ordering::Greater, Ordering::Less) | (Ordering::Less, Ordering::Greater) => {
            let (ccw_ring, ccw_pts, cw_ring) = if sign_a == Ordering::Greater {
                (ring_a, pts_a, ring_b)
            } else {
                (ring_b, pts_b, ring_a)
            };
            // A pinch INSIDE the CW hole lobe is out of scope → loud.
            if find_ring_pinch(p2, &cw_ring).is_some() {
                return Err("keyhole hole lobe carries a nested pinch (out of scope)");
            }
            // Original face holes were interior to the outer ring — reassign
            // them to the CCW keyhole outer (strict containment); the CW lobe
            // is appended as an additional native hole.
            let mut ccw_holes: Vec<Vec<u32>> = Vec::new();
            for h in holes {
                let Some(&first) = h.first() else {
                    continue;
                };
                if exact2d::point_strictly_inside(p2[first as usize], &ccw_pts) {
                    ccw_holes.push(h.clone());
                } else {
                    return Err("keyhole outer does not strictly contain a face hole");
                }
            }
            ccw_holes.push(cw_ring);
            // The CCW outer may itself carry a further pinch — recurse the
            // split BEFORE the keyhole CDT (budget unchanged); the appended CW
            // hole rides along as a native hole through the recursion.
            pinch_split_rec(p2, p3, &ccw_ring, &ccw_holes, budget)
        }
        // M3c: CW + CW (or a degenerate zero-area sub-ring) — invalid winding.
        _ => Err("pinch sub-ring is not CCW"),
    }
}

mod developable;
pub(crate) use developable::{tessellate_cone_patch, tessellate_cylinder_patch};

/// One node of the working polygon: projected 2D point + the index of its
/// (per-face) render vertex.
#[derive(Clone, Copy)]
struct Node {
    p2: Point2,
    vid: u32,
}

/// The single canonical planar-face routine (module docs, "Algorithm").
/// PR-KV5b: loops may carry arc edges (boolean outputs — e.g. an annulus
/// hole rim of exact intersection arcs); they enter the polygon as their
/// chord-bound samples via [`sampled_loop_points`]. Pure-segment faces are
/// byte-identical to the KV3 path.
fn tessellate_planar_face(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    let face = arena.face(fid)?;
    let Some(Surface::Plane(plane)) = face.surface else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    let n = plane.normal;
    let project = projector(n);

    // ---- gather loops, emit per-face render vertices ----------------------
    let range_start = out.indices.len() as u32;
    let emit_loop = |pts: &[Point3], out: &mut RenderMesh| -> Vec<Node> {
        pts.iter()
            .map(|p| {
                let vid = out.num_vertices() as u32;
                out.positions.extend_from_slice(&p.as_array());
                out.normals.extend_from_slice(&[n.x, n.y, n.z]);
                Node {
                    p2: project(*p),
                    vid,
                }
            })
            .collect()
    };

    let outer_pts = sampled_loop_points(arena, face.outer_loop, n_seg)?;
    if std::env::var("KV2_EARCLIP_PROBE").is_ok() {
        eprintln!(
            "KV2_EARCLIP_PROBE face={fid:?} plane_n=({:.6},{:.6},{:.6}) outer={} holes={} pts={:?}",
            n.x,
            n.y,
            n.z,
            outer_pts.len(),
            face.inner_loops.len(),
            outer_pts.iter().map(|p| p.as_array()).collect::<Vec<_>>()
        );
    }
    if outer_pts.len() < 3 {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "outer loop has fewer than 3 vertices",
        });
    }
    let poly: Vec<Node> = emit_loop(&outer_pts, out);
    let mut holes: Vec<Vec<Node>> = Vec::with_capacity(face.inner_loops.len());
    for &rid in &face.inner_loops {
        let pts = sampled_loop_points(arena, rid, n_seg)?;
        if pts.is_empty() {
            continue; // lone-vertex ring bounds no area
        }
        if pts.len() < 3 {
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "ring with fewer than 3 vertices",
            });
        }
        holes.push(emit_loop(&pts, out));
    }

    // ---- constrained-Delaunay triangulation + G1 gate ---------------------
    // (spec kv2_cdt_triangulation_core §3, branches P1–P3, G1; §6b M1/M2/M3).
    // The greedy exact ear-clip emitted a triangle spanning three near-collinear
    // boundary vertices — a silent sub-f32 sliver on the R0064 gear loop; the
    // `triangulate_ring` flood-fill CDT (M2) + M1 grid-degeneracy flip pass
    // avoids it. Hole loops are passed NATIVELY (no bridge corridors —
    // corridor-doubled vertices would be rejected by the CDT as coincident).
    //
    // CDT vertex pool: the outer loop's projected Point2, then each hole's.
    // Each pool index keeps its render-vertex id; vertex EMISSION order is the
    // per-loop order established by `emit_loop` above and is unchanged (only
    // triangle connectivity comes from the CDT).
    let mut pool_p2: Vec<Point2> = Vec::with_capacity(poly.len());
    let mut pool_vid: Vec<u32> = Vec::with_capacity(poly.len());
    let mut outer_cdt: Vec<u32> = Vec::with_capacity(poly.len());
    for nd in &poly {
        outer_cdt.push(pool_p2.len() as u32);
        pool_p2.push(nd.p2);
        pool_vid.push(nd.vid);
    }
    let holes_cdt: Vec<Vec<u32>> = holes
        .iter()
        .map(|hole| {
            hole.iter()
                .map(|nd| {
                    let idx = pool_p2.len() as u32;
                    pool_p2.push(nd.p2);
                    pool_vid.push(nd.vid);
                    idx
                })
                .collect()
        })
        .collect();
    // Branch P3: any CDT rejection (coincident verts / crossing constraints /
    // zero area) is a loud typed failure — never a fallback (P9). The M1
    // grid-degeneracy flip pass (spec §6b) runs BEFORE the emit loop.
    let pool_p3: Vec<[f64; 3]> = pool_vid
        .iter()
        .map(|&vid| {
            let i = vid as usize * 3;
            [out.positions[i], out.positions[i + 1], out.positions[i + 2]]
        })
        .collect();
    let cdt_tris = triangulate_with_pinch_split(&pool_p2, &pool_p3, &outer_cdt, &holes_cdt)
        .map_err(|reason| KernelV2Error::TessellationFailed { face: fid, reason })?;

    // Emit, applying the G1 render-precision gate to every triangle: geometry
    // valid at f64 but COLLAPSED at f32 render precision must fail loudly (§3
    // G1) — the planar analogue of the cylinder patch's G0 gate, always-on
    // (I6), never a skip/snap (P9).
    for t in &cdt_tris {
        let tri = [
            pool_vid[t[0] as usize],
            pool_vid[t[1] as usize],
            pool_vid[t[2] as usize],
        ];
        let pos_of = |vid: u32| -> [f64; 3] {
            let i = vid as usize * 3;
            [out.positions[i], out.positions[i + 1], out.positions[i + 2]]
        };
        if f32_render_degenerate(pos_of(tri[0]), pos_of(tri[1]), pos_of(tri[2])) {
            if std::env::var_os("KV2_RENDER_GATE_PROBE").is_some() {
                eprintln!(
                    "[render-gate-probe] face {fid:?} planar collapse: \
                     p0={:?} p1={:?} p2={:?}",
                    pos_of(tri[0]),
                    pos_of(tri[1]),
                    pos_of(tri[2]),
                );
                eprintln!(
                    "[render-gate-probe] outer ring ({} nodes):",
                    outer_cdt.len()
                );
                for (k, &pi) in outer_cdt.iter().enumerate() {
                    eprintln!(
                        "[render-gate-probe]   ring[{k}] vid={} p={:?}",
                        pool_vid[pi as usize], pool_p3[pi as usize]
                    );
                }
            }
            return Err(KernelV2Error::TessellationFailed {
                face: fid,
                reason: "planar triangle collapsed at render precision",
            });
        }
        out.indices.extend_from_slice(&tri);
    }

    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Orientation-preserving projection onto the dominant-axis coordinate
/// plane of the (unit) face normal: the retained axes are ordered so that
/// a loop winding CCW around `n` in 3D stays CCW in 2D.
fn projector(n: UnitVector3) -> impl Fn(Point3) -> Point2 {
    let (ax, ay, az) = (n.x.abs(), n.y.abs(), n.z.abs());
    // (kept axis pair, swap?) — right-handed cyclic order per dropped axis,
    // reversed when the normal points along the negative axis.
    let (k, positive) = if az >= ax && az >= ay {
        (2usize, n.z > 0.0)
    } else if ax >= ay {
        (0usize, n.x > 0.0)
    } else {
        (1usize, n.y > 0.0)
    };
    move |p: Point3| {
        let (u, v) = match k {
            2 => (p.x(), p.y()), // drop z: (x, y)
            0 => (p.y(), p.z()), // drop x: (y, z)
            _ => (p.z(), p.x()), // drop y: (z, x)
        };
        if positive {
            Point2::new(u, v)
        } else {
            Point2::new(v, u)
        }
    }
}

#[cfg(test)]
mod annulus_sweep_tests;

#[cfg(test)]
mod cone_tess_tests;

#[cfg(test)]
mod torus_patch_tess_tests;

#[cfg(test)]
mod patch_render_degeneracy_gate_tests;

#[cfg(test)]
mod cdt_core_red_tests;

#[cfg(test)]
mod cdt_core_round2_red_tests;

#[cfg(test)]
mod cdt_core_adversary_tests;
