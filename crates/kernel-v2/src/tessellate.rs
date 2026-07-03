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
                    if face_has_circle_edge(arena, f)? {
                        tessellate_cone_lateral(arena, f, n_seg, &mut mesh)?
                    } else {
                        // Partial cone patch (boolean output) — KV6c increment 5.
                        return Err(KernelV2Error::CurvedGeometryMismatch {
                            face: f,
                            reason: "tessellation: partial Surface::Cone patch not yet implemented (KV6c increment 5)",
                        });
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
                None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            }
        }
    }
    Ok(mesh)
}

/// Interior sample points of an arc half-edge at the chord-bound angular
/// resolution (endpoints excluded), IN THE HALF-EDGE'S WALK DIRECTION.
///
/// Bitwise twin-symmetric: the samples are computed on the CANONICAL
/// (lower-id) half-edge of the twin pair and reversed for the other side,
/// so the two faces sharing the arc emit identical sample positions —
/// load-bearing for cross-face watertightness (a planar annulus face and
/// the cylinder patch share their intersection-circle arcs).
pub(crate) fn arc_interior_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if !matches!(he.curve, Curve::Arc { .. }) {
        return Ok(Vec::new());
    }
    let canon = h.min(he.twin);
    let che = arena.half_edge(canon)?;
    let Curve::Arc {
        center,
        normal,
        radius,
    } = che.curve
    else {
        // Twin curve consistency is a validated invariant.
        return Err(KernelV2Error::CurveTwinMismatch { half_edge: canon });
    };
    let fid = arena.loop_(che.loop_id)?.face;
    let start = arena.vertex(che.origin)?.point;
    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
    let n_arr = [normal.x, normal.y, normal.z];
    let Some(sweep) = crate::geom::ccw_sweep(center, n_arr, start, end) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate arc (endpoint has no radial direction)",
        });
    };
    // e1 anchored at the canonical start so sample 0 continues from it.
    let Some((e1, e2)) = circle_frame(center, normal, start) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate arc frame (start not radial)",
        });
    };
    let step = 2.0 * std::f64::consts::PI / f64::from(n_seg);
    let k = (sweep / step).ceil().max(1.0) as u32;
    let mut samples = Vec::with_capacity(k as usize - 1);
    for j in 1..k {
        let theta = sweep * f64::from(j) / f64::from(k);
        let (s, c) = theta.sin_cos();
        samples.push(Point3::new(
            center.x() + radius * (c * e1[0] + s * e2[0]),
            center.y() + radius * (c * e1[1] + s * e2[1]),
            center.z() + radius * (c * e1[2] + s * e2[2]),
        ));
    }
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}

/// Interior sample points of an ELLIPSE-arc half-edge (PR-KV9), endpoints
/// excluded, in the half-edge's walk direction. Twin-canonical exactly like
/// [`arc_interior_samples`]: computed on the lower-id half-edge of the twin
/// pair and reversed for the other side, so both incident faces emit
/// identical positions. The parametric step is the SAME angular step the
/// circle sampling uses (`2π/n_seg`): for a cylinder-section ellipse the
/// parameter equals the cylinder azimuth, so per-chord surface deviation
/// matches the lateral's own chord bound (shared contract, no new
/// tolerance).
pub(crate) fn ellipse_interior_samples(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    n_seg: u32,
) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if !matches!(he.curve, Curve::EllipseArc { .. }) {
        return Ok(Vec::new());
    }
    let canon = h.min(he.twin);
    let che = arena.half_edge(canon)?;
    let Curve::EllipseArc {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = che.curve
    else {
        return Err(KernelV2Error::CurveTwinMismatch { half_edge: canon });
    };
    let fid = arena.loop_(che.loop_id)?.face;
    let start = arena.vertex(che.origin)?.point;
    let end = arena.vertex(arena.half_edge(che.next)?.origin)?.point;
    let nu = [normal.x, normal.y, normal.z];
    let mr = [major_axis.x, major_axis.y, major_axis.z];
    let (Some(t0), Some(sweep)) = (
        crate::geom::ellipse_param(center, nu, mr, major_radius, minor_radius, start),
        crate::geom::ellipse_ccw_sweep(center, nu, mr, major_radius, minor_radius, start, end),
    ) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate ellipse arc (endpoint projects to the center)",
        });
    };
    let step = 2.0 * std::f64::consts::PI / f64::from(n_seg);
    let k = (sweep / step).ceil().max(1.0) as u32;
    let mut samples = Vec::with_capacity(k as usize - 1);
    for j in 1..k {
        let t = t0 + sweep * f64::from(j) / f64::from(k);
        samples.push(crate::geom::ellipse_point_at(
            center,
            nu,
            mr,
            major_radius,
            minor_radius,
            t,
        ));
    }
    if h != canon {
        samples.reverse();
    }
    Ok(samples)
}

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
    let (half_angle, axis_dir, reversed) = match face.surface {
        Some(Surface::Cone {
            half_angle,
            axis_dir,
            reversed,
            ..
        }) => (half_angle, axis_dir, reversed),
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

    // Recover (w0, α) from the +axis seam arc (radius major+minor).
    let hes = arena.loop_half_edges(face.outer_loop)?;
    let mut seam = None;
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
    let alpha =
        crate::geom::ccw_sweep(center, ax, v0, valpha).ok_or(fail("degenerate torus sweep"))?;
    let m0 = [
        ax[1] * w0[2] - ax[2] * w0[1],
        ax[2] * w0[0] - ax[0] * w0[2],
        ax[0] * w0[1] - ax[1] * w0[0],
    ];

    // φ matches the caps (n_seg); θ steps keep a comparable chord at radius R+r.
    let n_phi = n_seg.max(3) as usize;
    let n_theta = {
        let per = (2.0 * PI / n_seg as f64) * r_min / (r_maj + r_min);
        ((alpha / per).ceil() as usize).max(2)
    };
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
    for i in 0..=n_theta {
        let theta = alpha * (i as f64) / (n_theta as f64);
        for j in 0..n_phi {
            let phi = 2.0 * PI * (j as f64) / (n_phi as f64);
            let (p, nrm) = point(theta, phi);
            out.positions.extend_from_slice(&p);
            out.normals.extend_from_slice(&nrm);
        }
    }
    let idx = |i: usize, j: usize| base + (i * n_phi + (j % n_phi)) as u32;
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
    let mut tris = yang_rs::cdt_polygon_with_holes_floodfill(p2, outer, holes)
        .map_err(|_| "ring rejected by CDT (degenerate/self-intersecting)")?;
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

fn tessellate_cylinder_patch(
    arena: &BrepArena,
    fid: FaceId,
    n_seg: u32,
    out: &mut RenderMesh,
) -> Result<(), KernelV2Error> {
    use std::f64::consts::PI;
    let fail = |reason: &'static str| KernelV2Error::TessellationFailed { face: fid, reason };

    let face = arena.face(fid)?;
    let Some(Surface::Cylinder {
        axis_point,
        axis_dir,
        radius,
        reversed,
    }) = face.surface
    else {
        return Err(KernelV2Error::FaceWithoutSurface { face: fid });
    };
    let a = [axis_dir.x, axis_dir.y, axis_dir.z];
    let ap = [axis_point.x(), axis_point.y(), axis_point.z()];
    let sense = if reversed { -1.0 } else { 1.0 };
    let w_facet = 2.0 * PI * radius / f64::from(n_seg);

    let mut all_loops = vec![face.outer_loop];
    all_loops.extend(face.inner_loops.iter().copied());

    // ---- shared angular frame (anchored at the first outer vertex) -------
    let theta_h = |p: Point3, e1: [f64; 3], e2: [f64; 3]| -> Result<(f64, f64), KernelV2Error> {
        let d = [p.x() - ap[0], p.y() - ap[1], p.z() - ap[2]];
        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return Err(fail("patch vertex lies on the cylinder axis"));
        }
        let x = r[0] * e1[0] + r[1] * e1[1] + r[2] * e1[2];
        let y = r[0] * e2[0] + r[1] * e2[1] + r[2] * e2[2];
        Ok((y.atan2(x), h))
    };
    let outer_hes = arena.loop_half_edges(face.outer_loop)?;
    if outer_hes.is_empty() {
        return Err(fail("patch with an empty boundary loop"));
    }
    let p0 = arena.vertex(arena.half_edge(outer_hes[0])?.origin)?.point;
    let d0 = [p0.x() - ap[0], p0.y() - ap[1], p0.z() - ap[2]];
    let h00 = d0[0] * a[0] + d0[1] * a[1] + d0[2] * a[2];
    let r0 = [d0[0] - h00 * a[0], d0[1] - h00 * a[1], d0[2] - h00 * a[2]];
    let r0l = (r0[0] * r0[0] + r0[1] * r0[1] + r0[2] * r0[2]).sqrt();
    if !(r0l.is_finite() && r0l > 0.0) {
        return Err(fail("patch anchor vertex lies on the cylinder axis"));
    }
    let e1 = [r0[0] / r0l, r0[1] / r0l, r0[2] / r0l];
    let e2 = [
        a[1] * e1[2] - a[2] * e1[1],
        a[2] * e1[0] - a[0] * e1[2],
        a[0] * e1[1] - a[1] * e1[0],
    ];
    // (u, v) ← 3D, and 3D ← (u, v) for on-surface points.
    let unroll_u = |theta: f64| sense * theta * radius;
    let surface_point = |u: f64, v: f64| -> [f64; 3] {
        let theta = sense * u / radius;
        let (s, c) = theta.sin_cos();
        [
            ap[0] + v * a[0] + radius * (c * e1[0] + s * e2[0]),
            ap[1] + v * a[1] + radius * (c * e1[1] + s * e2[1]),
            ap[2] + v * a[2] + radius * (c * e1[2] + s * e2[2]),
        ]
    };

    // TEMP diagnostic (uncommitted): boundary feature-size survey.
    if std::env::var_os("KV2_PATCH_MINLEN_PROBE").is_some() {
        let mut min_edge = f64::INFINITY;
        let mut min_pair = f64::INFINITY;
        let mut all_pts: Vec<[f64; 3]> = Vec::new();
        for &lid in &all_loops {
            if let Ok(hes) = arena.loop_half_edges(lid) {
                for &h in &hes {
                    if let Ok(he) = arena.half_edge(h) {
                        if let (Ok(p), Ok(nx)) = (arena.vertex(he.origin), arena.half_edge(he.next))
                        {
                            if let Ok(q) = arena.vertex(nx.origin) {
                                let d = [
                                    q.point.x() - p.point.x(),
                                    q.point.y() - p.point.y(),
                                    q.point.z() - p.point.z(),
                                ];
                                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                                min_edge = min_edge.min(l);
                                all_pts.push([p.point.x(), p.point.y(), p.point.z()]);
                            }
                        }
                    }
                }
            }
        }
        for i in 0..all_pts.len() {
            for j in (i + 1)..all_pts.len() {
                let d = [
                    all_pts[i][0] - all_pts[j][0],
                    all_pts[i][1] - all_pts[j][1],
                    all_pts[i][2] - all_pts[j][2],
                ];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                if l < min_pair {
                    min_pair = l;
                }
            }
        }
        eprintln!(
            "[minlen-probe] face={fid:?} boundary_verts={} min_edge={min_edge:.3e} \
             min_pair={min_pair:.3e}",
            all_pts.len()
        );
    }

    // ---- pass 1: per-loop unrolled chains ---------------------------------
    struct Chain {
        /// (node index, kind of the edge to the NEXT chain entry, cyclic).
        entries: Vec<(usize, PatchEdgeKind)>,
        wrap: i64,
    }
    let mut nodes: Vec<PatchNode> = Vec::new();
    let mut chains: Vec<Chain> = Vec::new();
    for &lid in &all_loops {
        let hes = arena.loop_half_edges(lid)?;
        if hes.len() < 3 {
            return Err(fail("patch loop with fewer than 3 edges"));
        }
        let mut entries: Vec<(usize, PatchEdgeKind)> = Vec::new();
        let mut u_cur = f64::NAN;
        let mut total_theta = 0.0f64;
        for (i, &h) in hes.iter().enumerate() {
            let he = arena.half_edge(h)?;
            let p = arena.vertex(he.origin)?.point;
            let q = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
            let (theta_p, hp) = theta_h(p, e1, e2)?;
            if i == 0 {
                u_cur = unroll_u(theta_p);
            }
            let origin_node = nodes.len();
            nodes.push(PatchNode {
                p2: Point2::new(u_cur, hp),
                pos: [p.x(), p.y(), p.z()],
            });
            match he.curve {
                Curve::LineSegment => {
                    let (theta_q, _) = theta_h(q, e1, e2)?;
                    let delta = crate::geom::wrap_to_pi(theta_q - theta_p);
                    entries.push((origin_node, PatchEdgeKind::Chord));
                    u_cur += sense * delta * radius;
                    total_theta += delta;
                }
                Curve::Arc {
                    center,
                    normal,
                    radius: _,
                } => {
                    let n_arr = [normal.x, normal.y, normal.z];
                    let Some(sweep) = crate::geom::ccw_sweep(center, n_arr, p, q) else {
                        return Err(fail("degenerate patch arc (endpoint not radial)"));
                    };
                    let dir = if normal.x * a[0] + normal.y * a[1] + normal.z * a[2] > 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let samples = arc_interior_samples(arena, h, n_seg)?;
                    let k = samples.len() + 1;
                    entries.push((origin_node, PatchEdgeKind::ArcSample));
                    for (j, sp) in samples.iter().enumerate() {
                        let frac = (j + 1) as f64 / k as f64;
                        let su = u_cur + sense * dir * sweep * frac * radius;
                        let (_, sh) = theta_h(*sp, e1, e2)?;
                        entries.push((nodes.len(), PatchEdgeKind::ArcSample));
                        nodes.push(PatchNode {
                            p2: Point2::new(su, sh),
                            pos: [sp.x(), sp.y(), sp.z()],
                        });
                    }
                    u_cur += sense * dir * sweep * radius;
                    total_theta += dir * sweep;
                }
                Curve::EllipseArc {
                    center,
                    normal,
                    major_axis,
                    major_radius,
                    minor_radius,
                } => {
                    // PR-KV9: oblique-section arc on this cylinder. The
                    // azimuth advance equals the SIGNED parametric sweep
                    // (the axis-⊥ projection of a cylinder-section ellipse
                    // is the radius-r circle: Δθ = s_w·Δt, s_w the frame
                    // handedness sign — see geom::cylinder_arc_patch_flux).
                    let nu = [normal.x, normal.y, normal.z];
                    let mr = [major_axis.x, major_axis.y, major_axis.z];
                    let m_dot_a = mr[0] * a[0] + mr[1] * a[1] + mr[2] * a[2];
                    let e1r = [
                        mr[0] - m_dot_a * a[0],
                        mr[1] - m_dot_a * a[1],
                        mr[2] - m_dot_a * a[2],
                    ];
                    let e1l = (e1r[0] * e1r[0] + e1r[1] * e1r[1] + e1r[2] * e1r[2]).sqrt();
                    if e1l < 1e-12 {
                        return Err(fail(
                            "patch ellipse-arc major axis parallel to the cylinder axis",
                        ));
                    }
                    let e2v = [
                        (a[1] * e1r[2] - a[2] * e1r[1]) / e1l,
                        (a[2] * e1r[0] - a[0] * e1r[2]) / e1l,
                        (a[0] * e1r[1] - a[1] * e1r[0]) / e1l,
                    ];
                    let w = [
                        nu[1] * mr[2] - nu[2] * mr[1],
                        nu[2] * mr[0] - nu[0] * mr[2],
                        nu[0] * mr[1] - nu[1] * mr[0],
                    ];
                    let s_w = if w[0] * e2v[0] + w[1] * e2v[1] + w[2] * e2v[2] >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    let Some(sweep) = crate::geom::ellipse_ccw_sweep(
                        center,
                        nu,
                        mr,
                        major_radius,
                        minor_radius,
                        p,
                        q,
                    ) else {
                        return Err(fail("degenerate patch ellipse arc"));
                    };
                    let samples = ellipse_interior_samples(arena, h, n_seg)?;
                    let k = samples.len() + 1;
                    entries.push((origin_node, PatchEdgeKind::ArcSample));
                    for (j, sp) in samples.iter().enumerate() {
                        let frac = (j + 1) as f64 / k as f64;
                        let su = u_cur + sense * s_w * sweep * frac * radius;
                        let (_, sh) = theta_h(*sp, e1, e2)?;
                        entries.push((nodes.len(), PatchEdgeKind::ArcSample));
                        nodes.push(PatchNode {
                            p2: Point2::new(su, sh),
                            pos: [sp.x(), sp.y(), sp.z()],
                        });
                    }
                    u_cur += sense * s_w * sweep * radius;
                    total_theta += s_w * sweep;
                }
                Curve::Circle { .. } => {
                    return Err(fail("full-circle edge inside a partial cylinder patch"))
                }
            }
        }
        let wraps_f = total_theta / (2.0 * PI);
        let wraps = wraps_f.round();
        if (wraps_f - wraps).abs() > 1e-3 || wraps.abs() > 1.0 {
            return Err(fail("patch loop's net axis winding is not a valid integer"));
        }
        // Mirror the wrap into the (sense-applied) unrolled frame.
        if std::env::var_os("KV2_PATCH_PASS_PROBE").is_some() {
            let us: Vec<f64> = entries.iter().map(|(n, _)| nodes[*n].p2.x()).collect();
            let (umin, umax) = us
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &u| {
                    (lo.min(u), hi.max(u))
                });
            eprintln!(
                "[pass-probe] face={fid:?} loop: entries={} total_theta={total_theta:.6} \
                 wraps={wraps} u_extent=[{umin:.6},{umax:.6}] w_facet={w_facet:.6}",
                entries.len()
            );
        }
        chains.push(Chain {
            entries,
            wrap: (sense * wraps) as i64,
        });
    }

    // ---- pass 2: assemble one simple polygon + holes ----------------------
    let span = 2.0 * PI * radius; // |u| span of one full wrap (sense-free)
    fn mid_u(c: &Chain, nodes: &[PatchNode]) -> f64 {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &(n, _) in &c.entries {
            let u = nodes[n].p2.x();
            lo = lo.min(u);
            hi = hi.max(u);
        }
        (lo + hi) / 2.0
    }

    let wrapping: Vec<usize> = (0..chains.len()).filter(|&i| chains[i].wrap != 0).collect();
    // Boundary-edge registry for refinement (node-index pairs, unordered).
    let mut boundary: std::collections::BTreeMap<(usize, usize), PatchEdgeKind> =
        std::collections::BTreeMap::new();
    let register_chain =
        |c: &Chain, boundary: &mut std::collections::BTreeMap<(usize, usize), PatchEdgeKind>| {
            let m = c.entries.len();
            for i in 0..m {
                let (n0, kind) = c.entries[i];
                let (n1, _) = c.entries[(i + 1) % m];
                let key = (n0.min(n1), n0.max(n1));
                boundary.insert(key, kind);
            }
        };

    // Shift a chain's u coordinates by k·span (re-pointing its nodes).
    let shift_chain = |c: &Chain, k: f64, nodes: &mut Vec<PatchNode>| {
        if k == 0.0 {
            return;
        }
        for &(n, _) in &c.entries {
            let p = nodes[n].p2;
            nodes[n].p2 = Point2::new(p.x() + k * span, p.y());
        }
    };

    let (poly, holes): (Vec<Node>, Vec<Vec<Node>>);
    match wrapping.len() {
        0 => {
            // Bounded patch: outer = the unique CCW loop (validated).
            let shoelace2 = |c: &Chain| -> f64 {
                let m = c.entries.len();
                let mut s = 0.0;
                for i in 0..m {
                    let p = nodes[c.entries[i].0].p2;
                    let q = nodes[c.entries[(i + 1) % m].0].p2;
                    s += p.x() * q.y() - q.x() * p.y();
                }
                s
            };
            let Some(outer_idx) = (0..chains.len()).find(|&i| shoelace2(&chains[i]) > 0.0) else {
                return Err(fail("bounded patch has no CCW loop in the unrolled frame"));
            };
            let outer_mid = mid_u(&chains[outer_idx], &nodes);
            for (i, c) in chains.iter().enumerate() {
                if i != outer_idx {
                    let k = ((outer_mid - mid_u(c, &nodes)) / span).round();
                    shift_chain(c, k, &mut nodes);
                }
            }
            for c in &chains {
                register_chain(c, &mut boundary);
            }
            poly = chains[outer_idx]
                .entries
                .iter()
                .map(|&(n, _)| Node {
                    p2: nodes[n].p2,
                    vid: n as u32,
                })
                .collect();
            holes = (0..chains.len())
                .filter(|&i| i != outer_idx)
                .map(|i| {
                    chains[i]
                        .entries
                        .iter()
                        .map(|&(n, _)| Node {
                            p2: nodes[n].p2,
                            vid: n as u32,
                        })
                        .collect()
                })
                .collect();
        }
        2 => {
            // Barrel segment: cut the two wrapping loops open along a seam
            // bridge pair (universal cover).
            let (ci_p, ci_m) = if chains[wrapping[0]].wrap > 0 {
                (wrapping[0], wrapping[1])
            } else {
                (wrapping[1], wrapping[0])
            };
            if chains[ci_p].wrap + chains[ci_m].wrap != 0 {
                return Err(fail("patch wrapping loops do not wind oppositely"));
            }
            // Place windows near the +wrap loop's span first (they are
            // re-checked by bridge validity below).
            let pmid = mid_u(&chains[ci_p], &nodes);
            for (i, c) in chains.iter().enumerate() {
                if i != ci_p && i != ci_m && c.wrap == 0 {
                    let k = ((pmid - mid_u(c, &nodes)) / span).round();
                    shift_chain(c, k, &mut nodes);
                }
            }
            for c in &chains {
                register_chain(c, &mut boundary);
            }

            // Candidate anchors: x over the +wrap chain's nodes; y = the
            // u-closest node of the −wrap chain (mod span).
            let pe = &chains[ci_p].entries;
            let me = &chains[ci_m].entries;
            type BuiltRing = (Vec<Node>, Vec<(usize, usize)>);
            let mut built: Option<BuiltRing> = None;
            'anchors: for xi in 0..pe.len() {
                let xu = nodes[pe[xi].0].p2.x();
                // y: minimize |principal Δu|.
                let mut best: Option<(usize, f64)> = None;
                for (yi, &(yn, _)) in me.iter().enumerate() {
                    let du = nodes[yn].p2.x() - xu;
                    let dpr = du - (du / span).round() * span;
                    if best.map(|(_, b)| dpr.abs() < b.abs()).unwrap_or(true) {
                        best = Some((yi, dpr));
                    }
                }
                let Some((yi, dpr)) = best else {
                    continue;
                };
                // Rotated + unwrapped polygon: x..x'(+span), bridge to
                // y'(x'+dpr), M walked from y (REVERSED in u: its wrap is
                // −1, so walking its stored order decreases u), back to y,
                // bridge to x.
                let mut ring: Vec<Node> = Vec::new();
                let m = pe.len();
                let base_x = nodes[pe[xi].0].p2.x();
                for j in 0..=m {
                    let (n, _) = pe[(xi + j) % m];
                    let mut u = nodes[n].p2.x();
                    if j > 0 && (xi + j) >= m {
                        u += span; // continued past the wrap
                    }
                    if j == m {
                        u = base_x + span; // the closing duplicate x'
                    }
                    ring.push(Node {
                        p2: Point2::new(u, nodes[n].p2.y()),
                        vid: n as u32,
                    });
                }
                let mm = me.len();
                let y_target = base_x + span + dpr;
                let y_base = nodes[me[yi].0].p2.x();
                for j in 0..=mm {
                    let (n, _) = me[(yi + j) % mm];
                    let mut u = nodes[n].p2.x();
                    if j > 0 && (yi + j) >= mm {
                        u += f64::from(chains[ci_m].wrap as i32) * span; // −span past the wrap
                    }
                    if j == mm {
                        u = y_base + f64::from(chains[ci_m].wrap as i32) * span;
                    }
                    ring.push(Node {
                        p2: Point2::new(u - y_base + y_target, nodes[n].p2.y()),
                        vid: n as u32,
                    });
                }
                // Bridge edges: (x' → y) at the right seam and (y_end → x)
                // at the left; check both against every boundary edge.
                let bridge_pairs = [
                    (ring[m].p2, ring[m + 1].p2),
                    (ring[m + 1 + mm].p2, ring[0].p2),
                ];
                let blocked = |p: Point2, q: Point2| -> bool {
                    let mut edges_iter: Vec<(Point2, Point2)> = Vec::new();
                    let rl = ring.len();
                    for i in 0..rl {
                        // The two seam-bridge edges themselves (at indices m
                        // and m+1+mm) are the candidates under test — they
                        // must not self-block.
                        if i == m || i == m + 1 + mm {
                            continue;
                        }
                        edges_iter.push((ring[i].p2, ring[(i + 1) % rl].p2));
                    }
                    for (ci, c) in chains.iter().enumerate() {
                        if ci == ci_p || ci == ci_m {
                            continue;
                        }
                        let cm = c.entries.len();
                        for i in 0..cm {
                            edges_iter.push((
                                nodes[c.entries[i].0].p2,
                                nodes[c.entries[(i + 1) % cm].0].p2,
                            ));
                        }
                    }
                    edges_iter
                        .into_iter()
                        .any(|(ea, eb)| exact2d::bridge_blocked_by(p, q, ea, eb))
                };
                if bridge_pairs.iter().any(|&(p, q)| p == q || blocked(p, q)) {
                    continue 'anchors;
                }
                // Register the bridge edges (Chord kind) for refinement.
                let xs = pe[xi].0;
                let ys = me[yi].0;
                let b1 = (xs.min(ys), xs.max(ys));
                built = Some((ring, vec![b1]));
                break;
            }
            let Some((ring, bridges)) = built else {
                return Err(fail(
                    "no unblocked seam bridge for the wrapping patch loops",
                ));
            };
            for key in bridges {
                boundary.insert(key, PatchEdgeKind::Chord);
            }
            poly = ring;
            holes = (0..chains.len())
                .filter(|&i| i != ci_p && i != ci_m)
                .map(|i| {
                    chains[i]
                        .entries
                        .iter()
                        .map(|&(n, _)| Node {
                            p2: nodes[n].p2,
                            vid: n as u32,
                        })
                        .collect()
                })
                .collect();
        }
        _ => return Err(fail("patch must have exactly 0 or 2 axis-wrapping loops")),
    }

    // ---- pass 3: constrained-Delaunay triangulation -----------------------
    // (spec kv2_cdt_triangulation_core §3, branches C1–C4; §6b M1/M2/M3). The
    // greedy exact ear-clip + f64 flip minted sub-f32 slivers from healthy
    // boundaries: the flip's plain-f64 incircle is catastrophically
    // ill-conditioned exactly on slivers, so it could not remove them and LEPP
    // then propagated them into dozens of B2 twins. `triangulate_ring` runs the
    // flood-fill CDT (M2, topological interior classification) + the M1
    // grid-degeneracy flip pass, so if any triangulation avoids the sliver, the
    // CDT avoids it and the flip pass repairs the residual grid-flat wedges.
    // Hole loops are passed NATIVELY (no bridge corridors — corridor-doubled
    // vertices would be rejected by the CDT as coincident).
    //
    // Register every outer-ring adjacency (covers the no-hole case and the
    // seam duplicates); hole adjacencies were registered by `register_chain`
    // above. Kinds set earlier (arc samples, seam bridges) survive `or_insert`.
    {
        let m = poly.len();
        for i in 0..m {
            let (a_id, b_id) = (poly[i].vid as usize, poly[(i + 1) % m].vid as usize);
            if a_id == b_id {
                continue;
            }
            let key = (a_id.min(b_id), a_id.max(b_id));
            boundary.entry(key).or_insert(PatchEdgeKind::Chord);
        }
    }

    // CDT vertex pool: the outer ring's per-ring Point2 (CUT frame — carrying
    // the seam-shifted u values and duplicate node ids at DISTINCT 2D
    // positions, which the CDT accepts), then each hole loop's Point2. Each
    // pool index keeps its original node-table id so the refinement and emit
    // keys below stay in node-id space.
    let mut pool_p2: Vec<Point2> = Vec::with_capacity(poly.len());
    let mut pool_node: Vec<usize> = Vec::with_capacity(poly.len());
    let mut outer_cdt: Vec<u32> = Vec::with_capacity(poly.len());
    for nd in &poly {
        outer_cdt.push(pool_p2.len() as u32);
        pool_p2.push(nd.p2);
        pool_node.push(nd.vid as usize);
    }
    let holes_cdt: Vec<Vec<u32>> = holes
        .iter()
        .map(|hole| {
            hole.iter()
                .map(|nd| {
                    let idx = pool_p2.len() as u32;
                    pool_p2.push(nd.p2);
                    pool_node.push(nd.vid as usize);
                    idx
                })
                .collect()
        })
        .collect();
    // 3D positions per pool index (for the M1 grid-degeneracy flip pass).
    let pool_p3: Vec<[f64; 3]> = pool_node.iter().map(|&nd| nodes[nd].pos).collect();
    // Branch C4: any CDT rejection (coincident verts / crossing constraints /
    // zero area) is a loud typed failure — never a fallback (P9). The M1
    // grid-degeneracy flip pass (spec §6b) runs BEFORE pass 4 refinement.
    let cdt_tris =
        triangulate_with_pinch_split(&pool_p2, &pool_p3, &outer_cdt, &holes_cdt).map_err(fail)?;

    // ---- pass 4: conforming chord-bound refinement -------------------------
    // Triangles in "work" coordinates: each corner = (p2 in the CUT frame,
    // node id). Two corners may share a node id at different p2 (the seam);
    // refinement keys edges by (node-id pair + p2 pair bits) so the two
    // seam instances refine independently but their splits stay collinear
    // 3D chords (closure-safe).
    #[derive(Clone, Copy)]
    struct WNode {
        p2: Point2,
        node: usize,
    }
    let mut wnodes: Vec<WNode> = pool_p2
        .iter()
        .zip(pool_node.iter())
        .map(|(&p, &n)| WNode { p2: p, node: n })
        .collect();
    let mut wtris: Vec<[usize; 3]> = cdt_tris
        .iter()
        .map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
        .collect();
    // Edge kind lookup for work edges: boundary by node-id pair, else
    // Interior.
    let kind_of = |wa: &WNode,
                   wb: &WNode,
                   boundary: &std::collections::BTreeMap<(usize, usize), PatchEdgeKind>|
     -> PatchEdgeKind {
        let key = (wa.node.min(wb.node), wa.node.max(wb.node));
        *boundary.get(&key).unwrap_or(&PatchEdgeKind::Interior)
    };
    // Cache of split midpoints keyed by the WORK edge (p2-bit pair), so the
    // two triangles sharing an edge get the same midpoint node (conforming).
    // Midpoint cache: WORK edge (p2-bit pair) → wnode index of its split.
    let mut split_cache: std::collections::BTreeMap<EKey, usize> =
        std::collections::BTreeMap::new();
    let w_limit = w_facet * (1.0 + 1e-9);
    // Refinement by Rivara longest-edge propagation (LEPP), EUCLIDEAN
    // metric, with the chord-bound STOP criterion in Δu: a triangle needs
    // refinement while any of its edges spans more than one facet width in
    // `u`; the edge BISECTED is always a locally-longest (Euclidean) edge,
    // found by walking strictly-longer neighbor maxima to a terminal edge.
    // Euclidean longest-edge bisection is the classic quality-preserving
    // scheme (Rivara 1984: finitely many similarity classes, angles bounded
    // below by the initial mesh's) — a Δu-metric variant tried first
    // produced sliver cascades (degenerate metric ⇒ no angle bound) that
    // blew the triangle count up and emitted zero-area slivers. Convergence
    // of the stop criterion: bisection halves edge lengths geometrically,
    // and an edge's Δu is bounded by its length.
    let max_du = |t: [usize; 3], wnodes: &[WNode]| -> f64 {
        let mut best = -1.0f64;
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            best = best.max((wnodes[t[i]].p2.x() - wnodes[t[j]].p2.x()).abs());
        }
        best
    };
    let longest_edge = |t: [usize; 3], wnodes: &[WNode]| -> (usize, usize, f64) {
        let mut best = (t[0], t[1], -1.0f64);
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let dx = wnodes[t[i]].p2.x() - wnodes[t[j]].p2.x();
            let dy = wnodes[t[i]].p2.y() - wnodes[t[j]].p2.y();
            let l2 = dx * dx + dy * dy;
            if l2 > best.2 {
                best = (t[i], t[j], l2);
            }
        }
        best
    };
    // Edge → incident-triangle adjacency (by p2-bit key), kept current
    // across splits so LEPP walks and conforming splits are O(degree), not
    // full-mesh scans.
    type EKey = ((u64, u64), (u64, u64));
    let ekey = |wa: &WNode, wb: &WNode| -> EKey {
        let (ka, kb) = (
            (wa.p2.x().to_bits(), wa.p2.y().to_bits()),
            (wb.p2.x().to_bits(), wb.p2.y().to_bits()),
        );
        if ka <= kb {
            (ka, kb)
        } else {
            (kb, ka)
        }
    };
    let mut edge_tris: std::collections::BTreeMap<EKey, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (ti, t) in wtris.iter().enumerate() {
        for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
            edge_tris
                .entry(ekey(&wnodes[t[i]], &wnodes[t[j]]))
                .or_default()
                .push(ti);
        }
    }
    let mut work: std::collections::VecDeque<usize> = (0..wtris.len()).collect();
    let mut guard = 0usize;
    while let Some(seed) = work.pop_front() {
        if max_du(wtris[seed], &wnodes) <= w_limit {
            continue;
        }
        guard += 1;
        if guard > 4_000_000 {
            return Err(fail("refinement did not converge (split budget exhausted)"));
        }
        // LEPP walk: follow strictly-longer neighbor maxima (Euclidean
        // length strictly increases each hop, so the walk is finite; the
        // inner guard is a loud tripwire, never a silent clamp).
        let mut cur = seed;
        let mut hops = 0usize;
        let (ia, ib) = loop {
            hops += 1;
            if hops > wtris.len() + 16 {
                return Err(fail("refinement LEPP walk did not terminate"));
            }
            let (ia, ib, l2) = longest_edge(wtris[cur], &wnodes);
            let ck = ekey(&wnodes[ia], &wnodes[ib]);
            let mut next = None;
            if let Some(tris) = edge_tris.get(&ck) {
                for &tj in tris {
                    if tj != cur && longest_edge(wtris[tj], &wnodes).2 > l2 {
                        next = Some(tj);
                        break;
                    }
                }
            }
            match next {
                Some(tj) => cur = tj,
                None => break (ia, ib),
            }
        };
        let (wa, wb) = (wnodes[ia], wnodes[ib]);
        let kind = kind_of(&wa, &wb, &boundary);
        let ckey = ekey(&wa, &wb);
        let mid_w = match split_cache.get(&ckey) {
            Some(&mi) => mi,
            None => {
                let mp2 = Point2::new((wa.p2.x() + wb.p2.x()) / 2.0, (wa.p2.y() + wb.p2.y()) / 2.0);
                let (pa, pb) = (nodes[wa.node].pos, nodes[wb.node].pos);
                let pos = match kind {
                    // Boundary edges split ON their own straight 3D
                    // geometry. An ArcSample sub-edge is the chord between
                    // two on-circle samples — already within the chord band;
                    // its lerped split point stays ON that chord (exactly
                    // collinear), so the neighboring face's unsplit copy of
                    // the chord remains closure-safe (T-junction).
                    PatchEdgeKind::Chord | PatchEdgeKind::ArcSample => [
                        (pa[0] + pb[0]) / 2.0,
                        (pa[1] + pb[1]) / 2.0,
                        (pa[2] + pb[2]) / 2.0,
                    ],
                    PatchEdgeKind::Interior => surface_point(mp2.x(), mp2.y()),
                };
                let node_idx = nodes.len();
                nodes.push(PatchNode { p2: mp2, pos });
                // Sub-edges inherit the kind so further splits stay on
                // the same geometry.
                if kind != PatchEdgeKind::Interior {
                    let k1 = (wa.node.min(node_idx), wa.node.max(node_idx));
                    let k2 = (wb.node.min(node_idx), wb.node.max(node_idx));
                    boundary.insert(k1, kind);
                    boundary.insert(k2, kind);
                }
                let mi = wnodes.len();
                wnodes.push(WNode {
                    p2: mp2,
                    node: node_idx,
                });
                split_cache.insert(ckey, mi);
                mi
            }
        };
        // Split EVERY triangle currently containing this work edge
        // (1 on boundary, 2 interior; corridor duplicates share the key) —
        // conforming. Adjacency is updated in place.
        let incident = edge_tris.remove(&ckey).unwrap_or_default();
        for tj in incident {
            let tt = wtris[tj];
            let mut found = None;
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                if ekey(&wnodes[tt[i]], &wnodes[tt[j]]) == ckey {
                    found = Some((i, j));
                    break;
                }
            }
            let Some((i, j)) = found else {
                continue; // stale adjacency entry (triangle already replaced)
            };
            let k = 3 - i - j;
            let (na, nb, nc) = (tt[i], tt[j], tt[k]);
            // Unregister tj's old edges, register the two children's.
            for (x, y) in [(na, nb), (nb, nc), (nc, na)] {
                if let Some(v) = edge_tris.get_mut(&ekey(&wnodes[x], &wnodes[y])) {
                    v.retain(|&t| t != tj);
                }
            }
            let new_idx = wtris.len();
            wtris[tj] = [na, mid_w, nc];
            wtris.push([mid_w, nb, nc]);
            for (ti2, tri) in [(tj, [na, mid_w, nc]), (new_idx, [mid_w, nb, nc])] {
                for (x, y) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                    edge_tris
                        .entry(ekey(&wnodes[x], &wnodes[y]))
                        .or_default()
                        .push(ti2);
                }
                work.push_back(ti2);
            }
        }
        // The seed may still carry an over-limit edge — requeue it.
        work.push_back(seed);
    }

    if std::env::var_os("KV2_PATCH_PASS_PROBE").is_some() {
        eprintln!(
            "[pass-probe] face={fid:?} cdt_tris={} refined_tris={} wnodes={} splits={}",
            cdt_tris.len(),
            wtris.len(),
            wnodes.len(),
            split_cache.len()
        );
    }

    // ---- pass 5: emit ------------------------------------------------------
    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    // One render vertex per WORK node (seam duplicates emit twice at the
    // same position — per-face vertices are never shared anyway).
    for wn in &wnodes {
        let pos = nodes[wn.node].pos;
        let d = [pos[0] - ap[0], pos[1] - ap[1], pos[2] - ap[2]];
        let h = d[0] * a[0] + d[1] * a[1] + d[2] * a[2];
        let r = [d[0] - h * a[0], d[1] - h * a[1], d[2] - h * a[2]];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if !(rl.is_finite() && rl > 0.0) {
            return Err(fail("patch render vertex has no radial direction"));
        }
        out.positions.extend_from_slice(&pos);
        out.normals
            .extend_from_slice(&[sense * r[0] / rl, sense * r[1] / rl, sense * r[2] / rl]);
    }
    for t in &wtris {
        // PR-KV9 fold tripwire (KV7-F1 class): a folded unrolled
        // triangulation emits triangles whose 3D winding faces INTO the
        // surface. Each emitted triangle's normal must agree with the
        // sense-adjusted outward radial at its centroid — a clear-margin
        // check (unit dot < −0.1 is a fold, not sliver noise; slivers with
        // sub-resolution area are skipped). Loud failure beats silently
        // shipping inverted geometry (P9).
        let pnt = |w: usize| nodes[wnodes[w].node].pos;
        let (pa, pb, pc) = (pnt(t[0]), pnt(t[1]), pnt(t[2]));
        // Render-precision degeneracy gate (spec
        // `kv2_patch_render_degeneracy_gate`, the F0047 class): geometry
        // that is valid at f64 but COLLAPSED at f32 render precision must
        // fail loudly — the f64 ear-clip/refinement can converge while
        // emitting sub-f32 slivers whose render edges then pair wrong
        // (silent non-manifold output past every fold tripwire below).
        // B2: two vertices bitwise-identical after f32 rounding. B3: the
        // f32 cross product exactly zero (collinear at render precision).
        // Always-on (I3) — never debug-gated, never a skip/snap (P9).
        if f32_render_degenerate(pa, pb, pc) {
            return Err(fail("patch triangle collapsed at render precision"));
        }
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n3 = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let nl = (n3[0] * n3[0] + n3[1] * n3[1] + n3[2] * n3[2]).sqrt();
        if nl > 1e-12 * (1.0 + radius * radius) {
            let cen = [
                (pa[0] + pb[0] + pc[0]) / 3.0,
                (pa[1] + pb[1] + pc[1]) / 3.0,
                (pa[2] + pb[2] + pc[2]) / 3.0,
            ];
            let dch = [cen[0] - ap[0], cen[1] - ap[1], cen[2] - ap[2]];
            let hh = dch[0] * a[0] + dch[1] * a[1] + dch[2] * a[2];
            let rr = [dch[0] - hh * a[0], dch[1] - hh * a[1], dch[2] - hh * a[2]];
            let rrl = (rr[0] * rr[0] + rr[1] * rr[1] + rr[2] * rr[2]).sqrt();
            if rrl > 0.0 {
                let dot = (n3[0] * rr[0] + n3[1] * rr[1] + n3[2] * rr[2]) / (nl * rrl);
                if sense * dot < -0.1 {
                    return Err(fail(
                        "patch triangulation folded (inverted triangle) — KV9-F2: the                          unrolled ear-clip/refinement produced inward-facing geometry;                          loud instead of silently-wrong render output",
                    ));
                }
            }
        }
        out.indices.extend_from_slice(&[
            base + t[0] as u32,
            base + t[1] as u32,
            base + t[2] as u32,
        ]);
    }
    // PR-KV11 fold tripwire extension (KV7-F1/KV9-F2 class): a folded
    // unrolled triangulation can keep its 3D winding within the −0.1 radial
    // margin yet stack jittered sliver layers over one boundary strip — the
    // render edges then triple up after seam quantization and the closed
    // mesh goes non-manifold (the F0046 class). In the unrolled 2D domain a
    // valid triangulation is a planar subdivision: every non-sliver work
    // triangle has the SAME orientation sign. Mixed signs ⇒ a fold. Loud
    // failure beats silently-wrong render output (P9). Sub-resolution
    // slivers are excluded with the scale-relative band (KV8b pattern).
    {
        let mut max_c = 0.0_f64;
        for wn in &wnodes {
            max_c = max_c.max(wn.p2.x().abs()).max(wn.p2.y().abs());
        }
        let area_eps = 1e-12 * (1.0 + max_c) * (1.0 + max_c);
        let (mut pos_n, mut neg_n) = (0usize, 0usize);
        for t in &wtris {
            let (a2, b2, c2) = (wnodes[t[0]].p2, wnodes[t[1]].p2, wnodes[t[2]].p2);
            let area2 =
                (b2.x() - a2.x()) * (c2.y() - a2.y()) - (b2.y() - a2.y()) * (c2.x() - a2.x());
            if area2 > area_eps {
                pos_n += 1;
            } else if area2 < -area_eps {
                neg_n += 1;
            }
        }
        if pos_n > 0 && neg_n > 0 {
            return Err(fail(
                "patch triangulation folded (mixed 2D orientation) — KV7-F1/KV9-F2: \
                 the unrolled ear-clip/refinement self-overlapped; loud instead of \
                 silently-wrong render output",
            ));
        }
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

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
mod annulus_sweep_tests {
    use super::annulus_sweep_triangles;

    // Build the 3D positions for a ring of `n` samples at azimuths `az`,
    // radius `r`, height `z`, in the XY plane (axis = +z).
    fn ring_positions(az: &[f64], r: f64, z: f64) -> Vec<[f64; 3]> {
        az.iter().map(|&a| [r * a.cos(), r * a.sin(), z]).collect()
    }

    // Every emitted triangle must wind CCW around +z (its geometric normal's
    // z-component is strictly positive) and have non-zero area.
    fn assert_all_wind_up(tris: &[[u32; 3]], outer: &[[f64; 3]], inner: &[[f64; 3]]) {
        let pos = |idx: u32| -> [f64; 3] {
            let i = idx as usize;
            if i < outer.len() {
                outer[i]
            } else {
                inner[i - outer.len()]
            }
        };
        let mut reversed = 0usize;
        let mut degenerate = 0usize;
        for t in tris {
            let (o, a, b) = (pos(t[0]), pos(t[1]), pos(t[2]));
            let u = [a[0] - o[0], a[1] - o[1], a[2] - o[2]];
            let v = [b[0] - o[0], b[1] - o[1], b[2] - o[2]];
            let nz = u[0] * v[1] - u[1] * v[0];
            if nz.abs() < 1e-18 {
                degenerate += 1;
            } else if nz < 0.0 {
                reversed += 1;
            }
        }
        assert_eq!(
            reversed,
            0,
            "{reversed} of {} triangles wind DOWN",
            tris.len()
        );
        assert_eq!(degenerate, 0, "{degenerate} zero-area triangles");
    }

    #[test]
    fn aligned_seams_wind_consistently() {
        let n = 16usize;
        let az: Vec<f64> = (0..n)
            .map(|k| std::f64::consts::TAU * (k as f64) / (n as f64))
            .collect();
        let outer = ring_positions(&az, 2.0, 0.0);
        let inner = ring_positions(&az, 1.0, 0.0);
        let tris = annulus_sweep_triangles(&az, &az, 0, n as u32);
        assert_eq!(tris.len(), 2 * n, "n outer + n inner edges → 2n triangles");
        assert_all_wind_up(&tris, &outer, &inner);
    }

    #[test]
    fn offset_seams_wind_consistently() {
        // The gear's counterbore-floor case: the inner ring's seam is ~108°
        // ahead of the outer ring's. A column-k strip would twist and reverse
        // half the triangles; the azimuth sweep must keep them all up.
        let n = 32usize;
        let phase = 108.0_f64.to_radians();
        let outer_az: Vec<f64> = (0..n)
            .map(|k| std::f64::consts::TAU * (k as f64) / (n as f64))
            .collect();
        let inner_az: Vec<f64> = (0..n)
            .map(|k| {
                (phase + std::f64::consts::TAU * (k as f64) / (n as f64))
                    .rem_euclid(std::f64::consts::TAU)
            })
            .collect();
        let outer = ring_positions(&outer_az, 5.909, 0.0);
        let inner = ring_positions(&inner_az, 4.903, 0.0);
        let tris = annulus_sweep_triangles(&outer_az, &inner_az, 0, n as u32);
        assert_eq!(tris.len(), 2 * n);
        assert_all_wind_up(&tris, &outer, &inner);
    }

    #[test]
    fn offset_seams_unequal_counts_wind_consistently() {
        // Robustness: differing sample counts (general annulus) still all-up.
        let no = 24usize;
        let ni = 17usize;
        let phase = 1.234_f64;
        let outer_az: Vec<f64> = (0..no)
            .map(|k| std::f64::consts::TAU * (k as f64) / (no as f64))
            .collect();
        let inner_az: Vec<f64> = (0..ni)
            .map(|k| {
                (phase + std::f64::consts::TAU * (k as f64) / (ni as f64))
                    .rem_euclid(std::f64::consts::TAU)
            })
            .collect();
        let outer = ring_positions(&outer_az, 3.0, 0.0);
        let inner = ring_positions(&inner_az, 1.5, 0.0);
        let tris = annulus_sweep_triangles(&outer_az, &inner_az, 0, no as u32);
        assert_eq!(tris.len(), no + ni);
        assert_all_wind_up(&tris, &outer, &inner);
    }
}

#[cfg(test)]
mod cone_tess_tests {
    use super::tessellate;
    use crate::arena::UnitVector3;
    use crate::cone_fixtures::build_frustum;
    use cad_primitives::Point3;
    use std::f64::consts::FRAC_PI_4;

    #[test]
    fn frustum_lateral_tessellates_with_tilted_outward_normals() {
        // 45° frustum, apex at the origin, axis +z, rims at radii 1 and 2.
        let plus_z = UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        let (arena, solid, lat) = build_frustum(
            Point3::new(0.0, 0.0, 0.0),
            plus_z,
            1.0,
            2.0,
            FRAC_PI_4,
            FRAC_PI_4,
        );
        let mesh = tessellate(&arena, solid).expect("frustum tessellates");

        let nv = mesh.num_vertices();
        assert!(
            mesh.indices.iter().all(|&i| (i as usize) < nv),
            "all triangle indices in range"
        );

        // Isolate the cone lateral's triangles.
        let fr = mesh
            .face_ranges
            .iter()
            .find(|r| r.face == lat)
            .expect("lateral face range present");
        assert!(fr.count > 0 && fr.count % 3 == 0, "whole triangles");

        let want_z = -(FRAC_PI_4.sin()); // tilt toward the apex: n·axis = −sin α
        let want_xy = FRAC_PI_4.cos(); // radial magnitude = cos α
        let s = fr.start as usize;
        let e = s + fr.count as usize;
        for &idx in &mesh.indices[s..e] {
            let i = idx as usize;
            let n = [
                mesh.normals[3 * i],
                mesh.normals[3 * i + 1],
                mesh.normals[3 * i + 2],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-9, "unit normal, got {len}");
            assert!((n[2] - want_z).abs() < 1e-9, "n.z={} want {want_z}", n[2]);
            let xy = (n[0] * n[0] + n[1] * n[1]).sqrt();
            assert!((xy - want_xy).abs() < 1e-9, "radial magnitude cos(α)");
            // Outward: the radial component agrees with the position's radial
            // (apex at origin, axis +z ⇒ position radial = (x, y)).
            let p = [mesh.positions[3 * i], mesh.positions[3 * i + 1]];
            assert!(n[0] * p[0] + n[1] * p[1] > 0.0, "outward radial");
        }
    }
}

#[cfg(test)]
mod torus_patch_tess_tests {
    use super::tessellate_torus_patch;
    use crate::arena::{
        BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
        Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
    };
    use crate::tessellate::RenderMesh;
    use cad_primitives::Point3;
    use std::collections::BTreeMap;

    /// A boolean-output torus PATCH (arbitrary polyline boundary, no full-circle
    /// edge) tessellates — via the UV-CDT consumer — into a watertight, on-tube
    /// mesh with the boundary preserved. This exercises the kernel-v2 render
    /// wiring in isolation; the full boolean → reconstruction path is gated on
    /// torus Stage-4 SSI relocation (its output boundary is chord-approximate,
    /// see `kv6d_torus_boolean_recovery`).
    #[test]
    fn boolean_output_torus_patch_tessellates_watertight_and_on_surface() {
        let (r_maj, r_min) = (3.0_f64, 1.0_f64);
        // Torus center origin, axis +z, e1=+x, e2=+y.
        let eval = |u: f64, v: f64| -> Point3 {
            let rad = r_maj + r_min * u.cos();
            Point3::new(rad * v.cos(), rad * v.sin(), r_min * u.sin())
        };
        // A UV-rectangle patch boundary, 8 samples/side, all exactly on the tube.
        let (u0, u1, v0, v1) = (0.2_f64, 1.2, 0.5, 1.8);
        let ns = 8;
        let mut bpts: Vec<Point3> = Vec::new();
        let mut push = |u: f64, v: f64| bpts.push(eval(u, v));
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u0 + (u1 - u0) * t, v0);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u1, v0 + (v1 - v0) * t);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u1 - (u1 - u0) * t, v1);
        }
        for k in 0..ns {
            let t = k as f64 / ns as f64;
            push(u0, v1 - (v1 - v0) * t);
        }
        let n = bpts.len();

        // Minimal arena: one torus face bounded by a single LineSegment loop.
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        for p in &bpts {
            arena.vertices.push(Some(Vertex { point: *p }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32), // self — line segments never read the twin
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop {
            face: fid,
            boundary: LoopBoundary::Edges(HalfEdgeId(0)),
            kind: LoopKind::Outer,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Torus {
                center: Point3::new(0.0, 0.0, 0.0),
                axis_dir: UnitVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                major_radius: r_maj,
                minor_radius: r_min,
                reversed: false,
            }),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell {
            solid,
            faces: vec![fid],
            genus: 0,
        }));
        arena.solids.push(Some(Solid {
            shells: vec![shell],
        }));

        let mut mesh = RenderMesh::default();
        tessellate_torus_patch(&arena, fid, 24, &mut mesh).expect("torus patch tessellates");
        assert!(!mesh.indices.is_empty(), "non-empty patch mesh");

        // Boundary vertices preserved exactly (conformal): the first n emitted
        // positions are the input boundary.
        for (i, p) in bpts.iter().enumerate() {
            let k = i * 3;
            assert_eq!(mesh.positions[k], p.x(), "boundary x {i}");
            assert_eq!(mesh.positions[k + 1], p.y(), "boundary y {i}");
            assert_eq!(mesh.positions[k + 2], p.z(), "boundary z {i}");
        }

        // Steiner interior points added (refinement fired).
        assert!(mesh.num_vertices() > n, "interior Steiner points added");

        // Every render vertex lies on the tube within a tight band.
        for i in 0..mesh.num_vertices() {
            let k = i * 3;
            let (px, py, pz) = (
                mesh.positions[k],
                mesh.positions[k + 1],
                mesh.positions[k + 2],
            );
            let rho = (px * px + py * py).sqrt();
            let resid = (((rho - r_maj).powi(2) + pz * pz).sqrt() - r_min).abs();
            assert!(resid < 1e-9, "vertex {i} off tube: {resid}");
            // Outward normal agrees with (p − tubeCentre).
            let nrm = [mesh.normals[k], mesh.normals[k + 1], mesh.normals[k + 2]];
            let rhat = [px / rho, py / rho, 0.0];
            let out = [px - r_maj * rhat[0], py - r_maj * rhat[1], pz];
            assert!(
                nrm[0] * out[0] + nrm[1] * out[1] + nrm[2] * out[2] > 0.0,
                "vertex {i} normal not outward"
            );
        }

        // Watertight: every undirected (index) edge shared by 1 (boundary) or 2.
        let mut ec: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in mesh.indices.chunks_exact(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let e = if a < b { (a, b) } else { (b, a) };
                *ec.entry(e).or_insert(0) += 1;
            }
        }
        assert!(ec.values().all(|&c| c == 1 || c == 2), "non-manifold edge");
        assert_eq!(
            ec.values().filter(|&&c| c == 1).count(),
            n,
            "boundary loop is exactly the original n edges (no slits)"
        );
    }
}

#[cfg(test)]
mod patch_render_degeneracy_gate_tests {
    //! KV2 render-degeneracy gate (spec `specs/kv2_patch_render_degeneracy_gate.md`).
    //!
    //! `tessellate_cylinder_patch` is private, so these tests drive it directly
    //! on a hand-built cylinder-patch arena (same in-module pattern as
    //! `torus_patch_tess_tests`). RED: the gate does not exist yet, so a
    //! sub-f32 boundary tessellates silently into a degenerate render mesh
    //! instead of failing loudly.
    use super::tessellate_cylinder_patch;
    use crate::arena::{
        BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
        Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
    };
    use crate::error::KernelV2Error;
    use crate::tessellate::RenderMesh;
    use cad_primitives::Point3;

    const N_SEG: u32 = 32;

    /// A unit cylinder (axis +z through the origin) PATCH bounded by a single
    /// LineSegment loop (a boolean-output patch). Boundary sampled as a
    /// rectangle in (θ, z); `with_twin` inserts one extra boundary vertex
    /// 1e-12 above its neighbor in z — below f32 resolution at this scale
    /// (f32 ulp ≈ 1.2e-7 near magnitude 1), so the pair rounds to bitwise-equal
    /// f32 positions while staying f64-valid (passes every existing loud gate).
    fn build_cylinder_patch(with_twin: bool) -> (BrepArena, FaceId, usize) {
        let eval = |theta: f64, z: f64| Point3::new(theta.cos(), theta.sin(), z);
        let mut tz: Vec<(f64, f64)> = vec![(0.2, 0.0), (1.2, 0.0), (1.2, 0.5)];
        if with_twin {
            // Consecutive twin on the right edge: M = (1.2, 0.5) above, then
            // M2 = (1.2, 0.5 + 1e-12) — a ~1e-12 boundary edge.
            tz.push((1.2, 0.5 + 1e-12));
        }
        tz.push((1.2, 1.0));
        tz.push((0.2, 1.0));
        let bpts: Vec<Point3> = tz.iter().map(|&(t, z)| eval(t, z)).collect();
        let n = bpts.len();

        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        for p in &bpts {
            arena.vertices.push(Some(Vertex { point: *p }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32), // self — line segments never read the twin
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop {
            face: fid,
            boundary: LoopBoundary::Edges(HalfEdgeId(0)),
            kind: LoopKind::Outer,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: UnitVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                radius: 1.0,
                reversed: false,
            }),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell {
            solid,
            faces: vec![fid],
            genus: 0,
        }));
        arena.solids.push(Some(Solid {
            shells: vec![shell],
        }));
        (arena, fid, n)
    }

    /// Count emitted triangles with two bitwise-identical f32 vertex positions
    /// (the B2 degeneracy — the assay `no_degenerate_triangles` witness applied
    /// at the render channel's precision).
    fn count_f32_degenerate(mesh: &RenderMesh) -> usize {
        let key = |i: usize| -> [u32; 3] {
            [
                (mesh.positions[3 * i] as f32).to_bits(),
                (mesh.positions[3 * i + 1] as f32).to_bits(),
                (mesh.positions[3 * i + 2] as f32).to_bits(),
            ]
        };
        let mut count = 0;
        for t in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (key(t[0] as usize), key(t[1] as usize), key(t[2] as usize));
            if a == b || b == c || a == c {
                count += 1;
            }
        }
        count
    }

    /// B2 (RED): a sub-f32 patch boundary must fail loudly with the typed
    /// render-degeneracy reason — today it tessellates SILENTLY into a mesh
    /// carrying degenerate f32 triangles.
    #[test]
    fn sub_f32_patch_boundary_fails_loudly() {
        let (arena, fid, _n) = build_cylinder_patch(true);
        let mut mesh = RenderMesh::default();
        let result = tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh);

        match result {
            Err(KernelV2Error::TessellationFailed { face, reason }) => {
                assert_eq!(face, fid, "the gate must fail THIS patch face");
                assert_eq!(
                    reason, "patch triangle collapsed at render precision",
                    "the gate must use the spec's typed reason"
                );
            }
            Ok(()) => {
                // RED witness: today the patch tessellates AND emits degenerate
                // f32 triangles — a silently wrecked render mesh.
                let deg = count_f32_degenerate(&mesh);
                assert!(
                    deg > 0,
                    "fixture defect: expected a sub-f32 degenerate triangle, found none in {} tris",
                    mesh.indices.len() / 3
                );
                panic!(
                    "B2 RED: sub-f32 patch tessellated OK with {deg} of {} triangle(s) carrying \
                     two bitwise-identical f32 vertices (silent degenerate render mesh); spec \
                     requires TessellationFailed {{ reason: \"patch triangle collapsed at render \
                     precision\" }}",
                    mesh.indices.len() / 3
                );
            }
            Err(e) => panic!(
                "expected the render-degeneracy gate (TessellationFailed), got a different \
                 error: {e:?}"
            ),
        }
    }

    /// B1 / I2 guard: the SAME patch without the sub-f32 twin tessellates and
    /// emits NO f32-degenerate triangle — pins that the gate leaves clean
    /// patches alone (mutation tripwire). (The full-solid canonical KV5b patch
    /// path is covered end-to-end by `tests/kv5b_curved_boolean.rs`; this is
    /// the direct-drive gate counterpart, not a duplicate.)
    #[test]
    fn canonical_patch_tessellates_without_f32_degeneracy() {
        let (arena, fid, n) = build_cylinder_patch(false);
        let mut mesh = RenderMesh::default();
        tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
            .expect("B1: a clean cylinder patch must tessellate");
        assert!(mesh.num_vertices() >= n, "boundary vertices emitted");
        assert_eq!(
            count_f32_degenerate(&mesh),
            0,
            "B1: a clean patch must emit no f32-degenerate triangle"
        );
    }

    // Build a cylinder patch of the given radius with an arbitrary (theta,z)
    // boundary chain (LineSegment loop). Returns (arena, face, n).
    fn build_patch(radius: f64, tz: &[(f64, f64)]) -> (BrepArena, FaceId, usize) {
        let eval = |theta: f64, z: f64| Point3::new(radius * theta.cos(), radius * theta.sin(), z);
        let bpts: Vec<Point3> = tz.iter().map(|&(t, z)| eval(t, z)).collect();
        let n = bpts.len();
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        for p in &bpts {
            arena.vertices.push(Some(Vertex { point: *p }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop {
            face: fid,
            boundary: LoopBoundary::Edges(HalfEdgeId(0)),
            kind: LoopKind::Outer,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Cylinder {
                axis_point: Point3::new(0.0, 0.0, 0.0),
                axis_dir: UnitVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                radius,
                reversed: false,
            }),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell {
            solid,
            faces: vec![fid],
            genus: 0,
        }));
        arena.solids.push(Some(Solid {
            shells: vec![shell],
        }));
        (arena, fid, n)
    }

    // (b2, b3_only): triangles with two bitwise-equal f32 verts; and triangles
    // with ALL THREE distinct f32 verts but exactly-zero f32 cross (B3-only).
    fn scan_degeneracy(mesh: &RenderMesh) -> (usize, usize) {
        let key = |i: usize| -> [u32; 3] {
            [
                (mesh.positions[3 * i] as f32).to_bits(),
                (mesh.positions[3 * i + 1] as f32).to_bits(),
                (mesh.positions[3 * i + 2] as f32).to_bits(),
            ]
        };
        let fpos = |i: usize| -> [f32; 3] {
            [
                mesh.positions[3 * i] as f32,
                mesh.positions[3 * i + 1] as f32,
                mesh.positions[3 * i + 2] as f32,
            ]
        };
        let (mut b2, mut b3) = (0usize, 0usize);
        for t in mesh.indices.chunks_exact(3) {
            let (ka, kb, kc) = (key(t[0] as usize), key(t[1] as usize), key(t[2] as usize));
            if ka == kb || kb == kc || ka == kc {
                b2 += 1;
                continue;
            }
            let (fa, fb, fc) = (
                fpos(t[0] as usize),
                fpos(t[1] as usize),
                fpos(t[2] as usize),
            );
            let uu = [fb[0] - fa[0], fb[1] - fa[1], fb[2] - fa[2]];
            let vv = [fc[0] - fa[0], fc[1] - fa[1], fc[2] - fa[2]];
            let cx = uu[1] * vv[2] - uu[2] * vv[1];
            let cy = uu[2] * vv[0] - uu[0] * vv[2];
            let cz = uu[0] * vv[1] - uu[1] * vv[0];
            if cx == 0.0 && cy == 0.0 && cz == 0.0 {
                b3 += 1;
            }
        }
        (b2, b3)
    }

    // ── ADVERSARY (FIP Phase 4, governance/FEATURE_IMPLEMENTATION_PROTOCOL §6) ──
    // Attacks on the f32 render-precision gate. In-module (tessellate_cylinder_patch
    // is private). Purely additive; touches no existing test. `build_patch` +
    // `scan_degeneracy` above were localized with a throwaway probe.

    /// Assert the FIRST returned error is the typed render-degeneracy failure.
    fn assert_gate_fires(arena: &BrepArena, fid: FaceId) {
        let mut mesh = RenderMesh::default();
        match tessellate_cylinder_patch(arena, fid, N_SEG, &mut mesh) {
            Err(KernelV2Error::TessellationFailed { face, reason }) => {
                assert_eq!(face, fid);
                assert_eq!(reason, "patch triangle collapsed at render precision");
            }
            other => panic!("expected the render-degeneracy gate, got {other:?}"),
        }
    }

    /// MUTATION KILLER (b) — the B3 arm. A 3-vertex boundary triangle with a
    /// SUB-f32 theta width (1e-9): all three vertices collapse to ONE cylinder
    /// ruling in x,y at f32 while keeping DISTINCT z, so the emitted triangle has
    /// three DISTINCT f32 positions but an exactly-zero f32 cross product — a
    /// B3-ONLY degeneracy (no bitwise-equal pair; verified b2=0 via probe). The
    /// B2 arm cannot see it; only B3 fires.
    ///
    /// This is the case the RED suite never witnessed (it only exercised the B2
    /// twin), so dropping the B3 arm SURVIVES the RED test but is KILLED here.
    #[test]
    fn adversary_b3_only_f32_collinear_ruling_fails_loudly() {
        let (arena, fid, _n) = build_patch(1.0, &[(1.2, 0.0), (1.2 + 1e-9, 0.5), (1.2, 1.0)]);
        assert_gate_fires(&arena, fid);
    }

    /// No over-fire: a single triangle whose theta width is JUST above f32
    /// resolution (2.4e-7 ≈ 2× f32 ulp at scale 1) has three DISTINCT f32
    /// vertices AND a nonzero f32 cross, so it must tessellate cleanly. A gate
    /// widened from bitwise-exact to a tolerance would over-fire here.
    #[test]
    fn adversary_just_above_f32_resolution_does_not_over_fire() {
        let (arena, fid, _n) = build_patch(1.0, &[(1.2, 0.0), (1.2 + 2.4e-7, 0.5), (1.2, 1.0)]);
        let mut mesh = RenderMesh::default();
        tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
            .expect("a supra-f32 triangle must tessellate (no over-fire)");
        assert_eq!(
            scan_degeneracy(&mesh),
            (0, 0),
            "a supra-f32 triangle must emit no f32-degenerate triangle"
        );
    }

    /// Scale-appropriateness: the gate is bitwise-f32, so it fires exactly when
    /// the render precision at THAT coordinate magnitude can't resolve the
    /// feature. The SAME 5e-5 z pair collapses at z≈1024 (f32 ulp ≈ 1.2e-4 >
    /// 5e-5 → fires) but stays resolvable at z≈0.5 (f32 ulp ≈ 6e-8 ≪ 5e-5 →
    /// Ok). Pins that firing tracks scale automatically (no absolute tolerance).
    #[test]
    fn adversary_gate_is_scale_appropriate() {
        // z≈1024: the 5e-5 pair rounds to one f32 → fires.
        let (arena, fid, _n) = build_patch(
            1.0,
            &[
                (0.2, 1024.0),
                (1.2, 1024.0),
                (1.2, 1024.25),
                (1.2, 1024.25 + 5e-5),
                (1.2, 1024.5),
                (0.2, 1024.5),
            ],
        );
        assert_gate_fires(&arena, fid);

        // z≈0.5: the same 5e-5 pair stays distinct at f32 → tessellates clean.
        let (arena, fid, _n) = build_patch(
            1.0,
            &[
                (0.2, 0.0),
                (1.2, 0.0),
                (1.2, 0.25),
                (1.2, 0.25 + 5e-5),
                (1.2, 0.5),
                (0.2, 0.5),
            ],
        );
        let mut mesh = RenderMesh::default();
        tessellate_cylinder_patch(&arena, fid, N_SEG, &mut mesh)
            .expect("the same feature at unit scale is f32-resolvable → Ok");
        assert_eq!(
            scan_degeneracy(&mesh),
            (0, 0),
            "unit-scale feature must emit no f32-degenerate triangle"
        );
    }
}

#[cfg(test)]
mod cdt_core_red_tests {
    //! KV2 CDT triangulation core — RED tests (spec
    //! `specs/kv2_cdt_triangulation_core.md`).
    //!
    //! The two f64 render-triangulation cores mint sub-f32 slivers from healthy
    //! boundaries today (greedy exact ear-clip + f64 flip). This module banks
    //! the two measured witnesses from the §6b sliver root-cause instrumentation
    //! (2026-07-02) and asserts the SPEC TARGET, so they are RED until the cores
    //! are replaced by the exact-predicate CDT primitive:
    //!
    //! * `red_f0047_patch_ring_tessellates_clean`  — I1 root fix, cylinder patch.
    //! * `red_r0064_planar_ring_no_f32_degenerate` — I1/I3 root fix, planar face.
    //! * `red_planar_sub_f32_twin_fails_loudly`    — G1 new planar gate (I6).
    //! * `cdt_determinism_guard`                    — I5 guard (PASSES today).
    //!
    //! In-module because the target fns are private (same idiom as
    //! `patch_render_degeneracy_gate_tests`). The `scan_degeneracy` predicate is
    //! re-declared here — it is not importable across cfg(test) sibling modules.
    //! The banked coordinate arrays are verbatim measured fixtures; the builders
    //! carry `#[rustfmt::skip]` to preserve the banked one-triple-per-line layout.
    use super::tessellate_cylinder_patch;
    use crate::arena::{
        BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
        Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
    };
    use crate::error::KernelV2Error;
    use crate::tessellate::RenderMesh;
    use cad_primitives::Point3;

    /// (b2, b3_only): triangles with two bitwise-equal f32 verts (B2); and
    /// triangles with three DISTINCT f32 verts but an exactly-zero f32 cross
    /// (B3-only). Re-declared from `patch_render_degeneracy_gate_tests` (private,
    /// not importable across cfg(test) modules) — the gate's own predicate.
    fn scan_degeneracy(mesh: &RenderMesh) -> (usize, usize) {
        let key = |i: usize| -> [u32; 3] {
            [
                (mesh.positions[3 * i] as f32).to_bits(),
                (mesh.positions[3 * i + 1] as f32).to_bits(),
                (mesh.positions[3 * i + 2] as f32).to_bits(),
            ]
        };
        let fpos = |i: usize| -> [f32; 3] {
            [
                mesh.positions[3 * i] as f32,
                mesh.positions[3 * i + 1] as f32,
                mesh.positions[3 * i + 2] as f32,
            ]
        };
        let (mut b2, mut b3) = (0usize, 0usize);
        for t in mesh.indices.chunks_exact(3) {
            let (ka, kb, kc) = (key(t[0] as usize), key(t[1] as usize), key(t[2] as usize));
            if ka == kb || kb == kc || ka == kc {
                b2 += 1;
                continue;
            }
            let (fa, fb, fc) = (
                fpos(t[0] as usize),
                fpos(t[1] as usize),
                fpos(t[2] as usize),
            );
            let uu = [fb[0] - fa[0], fb[1] - fa[1], fb[2] - fa[2]];
            let vv = [fc[0] - fa[0], fc[1] - fa[1], fc[2] - fa[2]];
            let cx = uu[1] * vv[2] - uu[2] * vv[1];
            let cy = uu[2] * vv[0] - uu[0] * vv[2];
            let cz = uu[0] * vv[1] - uu[1] * vv[0];
            if cx == 0.0 && cy == 0.0 && cz == 0.0 {
                b3 += 1;
            }
        }
        (b2, b3)
    }

    /// Triangles whose f32-ARITHMETIC cross product is exactly zero — the
    /// probe's oracle-style scan (cast each position to f32, cross in f32).
    /// Distinct from `scan_degeneracy`: this fires on ANY f32-collinear triple,
    /// so it also catches the planar silent sliver even without a bitwise pair.
    fn f32_zero_cross(mesh: &RenderMesh) -> usize {
        let fp = |i: u32| -> [f32; 3] {
            let i = i as usize * 3;
            [
                mesh.positions[i] as f32,
                mesh.positions[i + 1] as f32,
                mesh.positions[i + 2] as f32,
            ]
        };
        let mut zero = 0usize;
        for t in mesh.indices.chunks_exact(3) {
            let (pa, pb, pc) = (fp(t[0]), fp(t[1]), fp(t[2]));
            let ax = pb[0] - pa[0];
            let ay = pb[1] - pa[1];
            let az = pb[2] - pa[2];
            let bx = pc[0] - pa[0];
            let by = pc[1] - pa[1];
            let bz = pc[2] - pa[2];
            let cx = ay * bz - az * by;
            let cy = az * bx - ax * bz;
            let cz = ax * by - ay * bx;
            if cx == 0.0 && cy == 0.0 && cz == 0.0 {
                zero += 1;
            }
        }
        zero
    }

    /// Highest incidence count over undirected triangle edges. A watertight
    /// per-face partition has every edge count 1 (boundary) or 2 (interior);
    /// any edge shared by >=3 triangles is a non-manifold fan. (Single-face
    /// fixtures tessellate into a fresh mesh, so every index is this face's.)
    fn max_edge_incidence(mesh: &RenderMesh) -> usize {
        use std::collections::HashMap;
        let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
        for t in mesh.indices.chunks_exact(3) {
            for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        counts.values().copied().max().unwrap_or(0)
    }

    /// Banked F0047 FaceId(17): the 27-half-edge cylinder-patch ring (EllipseArc
    /// + LineSegment curves, oblique axis) that mints 64 sliver twins under the
    /// ear-clip today. Verbatim measured fixture (§6b, 2026-07-02).
    #[rustfmt::skip]
    fn build_f0047_patch() -> (BrepArena, FaceId) {
                let verts: [[f64; 3]; 27] = [
            [-1.58670146809873014e-1, 1.39694186833137252e-1, -3.31479825440074705e-2],
            [-1.87132769422497458e-1, 1.27013376600575545e-1, 2.04041421835764493e-2],
            [-1.35920713809961791e-1, 1.49829628204667664e-1, 6.93455679889168897e-2],
            [-1.87122845816030392e-2, 2.02048914843496852e-1, 8.72319664002812190e-2],
            [3.65690528624461374e-3, 2.12014948389699420e-1, 8.80325642760462906e-2],
            [1.17181260672449730e-1, 2.62592887992069091e-1, 8.59991256624206279e-2],
            [1.01457681008560230e-1, 2.75865067849882406e-1, 6.07217061358272109e-2],
            [6.18431605953580626e-2, 2.89559560482457790e-1, -4.91223646853791857e-2],
            [5.84435014026544741e-2, 2.47782872325004039e-1, -1.58966435506585541e-1],
            [9.23380712032888240e-2, 1.63798806687539544e-1, -2.33935789963175877e-1],
            [1.52765583573145847e-1, 6.42717110069079001e-2, -2.50228187723328721e-1],
            [2.20540730295195719e-1, -1.91992650631445383e-2, -2.02670907614777296e-1],
            [2.74145381161490898e-1, -6.01126763981100209e-2, -1.06363050048302515e-1],
            [2.80834861142647108e-1, -6.16824146166900650e-2, -8.60777717273490395e-2],
            [2.60348084740481323e-1, -7.06782437684575771e-2, -9.75455578700840931e-2],
            [2.54361732905385385e-1, -7.30286097859711852e-2, -1.00838467988211500e-1],
            [1.43661909131507393e-1, -9.39416134239650369e-2, -1.57027501045288342e-1],
            [1.10506662300814587e-1, -9.10359998358734601e-2, -1.71943903668324238e-1],
            [3.76847584863033644e-2, -6.26052815555838971e-2, -2.00107130906309494e-1],
            [1.03502949582084496e-2, -3.83939134598064655e-2, -2.07854351723265857e-1],
            [5.11575273297650186e-3, -3.21228141672878081e-2, -2.08996985263826041e-1],
            [-2.56375319032608046e-2, 3.71822371293874590e-2, -2.08939002198881008e-1],
            [-2.83501266669233740e-2, 8.50064270306883363e-2, -2.00233621172709209e-1],
            [9.55592976974356523e-3, 2.04820700642129899e-1, -1.57495627767186619e-1],
            [1.75647835890361120e-2, 2.18211256013756405e-1, -1.50953027701538123e-1],
            [3.65690528624432101e-3, 2.12014948389699254e-1, -1.43667283291585846e-1],
            [-1.46811705915443347e-1, 1.44977418834731309e-1, -4.48256956284058192e-2],
        ];
        let curves: [Curve; 27] = [
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::LineSegment,
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(2.31656322267193571e-1, 1.51238921585485631e-1, -6.46900079063544853e-2),
                    normal: UnitVector3 { x: 4.16638297756302234e-1, y: 1.85622781897892580e-1, z: -8.89919497304794693e-1 },
                    major_axis: UnitVector3 { x: 7.14378362809429346e-1, y: 5.38556930633577235e-1, z: 4.46788526280901321e-1 },
                    major_radius: 3.25442310341877206e-1,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
            Curve::EllipseArc {
                    center: Point3::new(1.01471869922056035e0, 6.62468264542925178e-1, -2.83360108771785579e-1),
                    normal: UnitVector3 { x: -4.06962527885757264e-1, y: 9.13444853779818655e-1, z: 0.00000000000000000e0 },
                    major_axis: UnitVector3 { x: 8.88858336909114533e-1, y: 3.96008619703775822e-1, z: -2.30451795452917663e-1 },
                    major_radius: 1.35122611928868741e0,
                    minor_radius: 2.08654307526429911e-1,
                },
        ];
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        let n = verts.len();
        for p in &verts {
            arena.vertices.push(Some(Vertex { point: Point3::new(p[0], p[1], p[2]) }));
        }
        for (i, curve) in curves.iter().enumerate() {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: curve.clone(),
            }));
        }
        arena.loops.push(Some(Loop { face: fid, boundary: LoopBoundary::Edges(HalfEdgeId(0)), kind: LoopKind::Outer }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Cylinder {
                axis_point: Point3::new(-0.17590825403137722, -0.11484329189602588, 0.04912236468537917),
                axis_dir: UnitVector3 { x: 0.8153544101386442, y: 0.5323114883828126, z: -0.2276876483324 },
                radius: 0.2086543075264299,
                reversed: false,
            }),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell { solid, faces: vec![fid], genus: 0 }));
        arena.solids.push(Some(Solid { shells: vec![shell] }));
        (arena, fid)
    }

    /// Banked R0064 FaceId(289): the 280-vertex all-LineSegment planar gear loop
    /// (coordinate scale ~572) that mints ONE silent f32-zero-cross triangle
    /// under the ear-clip today. Verbatim measured fixture (§6b, 2026-07-02).
    #[rustfmt::skip]
    fn build_r0064_planar() -> (BrepArena, FaceId) {
        let verts: [[f64; 3]; 280] = [
            [5.72444123145398748e2, -3.00433535756816532e1, -1.39860340904570997e1],
            [5.07919355288268264e2, -6.45491731989768880e1, -4.95853362299673677e1],
            [5.37618957700605733e2, -1.29165974130948257e2, -6.43063844497772550e1],
            [5.37910959946767207e2, -1.29876725761533947e2, -6.44802747430262002e1],
            [5.38663569925024944e2, -1.32041509935472163e2, -6.50570960004470322e1],
            [5.39686789978010211e2, -1.35694891822323711e2, -6.61157671694181772e1],
            [5.40785411752486652e2, -1.40852207999495022e2, -6.77295748667915944e1],
            [5.41760706098615969e2, -1.47509362096596931e2, -6.99655112805180011e1],
            [5.42412142356884146e2, -1.55642761787995283e2, -7.28836565507602501e1],
            [5.42539125786032514e2, -1.65209397762602066e2, -7.65366097544369381e1],
            [5.41942741761506454e2, -1.76147064431604946e2, -8.09689723243191111e1],
            [5.40427495315825013e2, -1.88374721266591848e2, -8.62168874183449674e1],
            [5.37803034600730939e2, -2.01792992795994110e2, -9.23076384178244780e1],
            [5.33885846926415297e2, -2.16284804434544071e2, -9.92593093774682274e1],
            [5.28500916174444683e2, -2.31716150483665103e2, -1.07080509878054770e2],
            [4.87256857436774737e2, -2.38207171060388816e2, -1.23820806924579472e2],
            [4.74166915776827011e2, -2.25683414144056172e2, -1.23498258630826754e2],
            [4.62919056166569021e2, -2.13578288903105602e2, -1.22701835336864889e2],
            [4.53423613233747062e2, -2.02067456805623152e2, -1.21530357402450676e2],
            [4.45574186386932865e2, -1.91313619204098444e2, -1.20083411889432057e2],
            [4.39248753664704282e2, -1.81465313192289159e2, -1.18460502898776980e2],
            [4.34310890827809885e2, -1.72655816989690436e2, -1.16760207915609129e2],
            [4.30611087358488248e2, -1.65002171592528555e2, -1.15079345642756294e2],
            [4.27988150457711299e2, -1.58604324703616641e2, -1.13512160720411458e2],
            [4.26270687611384460e2, -1.53544402193625217e2, -1.12149530615252473e2],
            [4.25278657832902411e2, -1.49886111558182449e2, -1.11078199817797682e2],
            [4.24824981283009890e2, -1.47674281019587823e2, -1.10380046311846883e2],
            [4.24717196623038717e2, -1.46934537088344484e2, -1.10131385076751371e2],
            [4.17808696829201779e2, -7.87308555101240728e1, -8.61597965481743415e1],
            [3.40418771894381905e2, -6.65596755718083699e1, -1.08161375246168248e2],
            [3.27753541443815607e2, -1.31684979789230709e2, -1.37697679940919500e2],
            [3.27583761892850077e2, -1.32381070958693840e2, -1.38025251329180037e2],
            [3.26946493450165917e2, -1.34421327641732489e2, -1.39033555182628703e2],
            [3.25654068972682580e2, -1.37720340195032207e2, -1.40754345197647808e2],
            [3.23525777180571652e2, -1.42175437571205663e2, -1.43210304614100465e2],
            [3.20389469997901585e2, -1.47667523198464153e2, -1.46414814576489903e2],
            [3.16083110928021767e2, -1.54062032539324179e2, -1.50371789861773777e2],
            [3.10456254204546269e2, -1.61210006226232792e2, -1.55075583155937693e2],
            [3.03371444870987261e2, -1.68949271956451128e2, -1.60510958642693652e2],
            [2.94705530412757980e2, -1.77105727651505447e2, -1.66653135243638076e2],
            [2.84350875090043530e2, -1.85494717753902250e2, -1.73467899423661294e2],
            [2.72216468698977565e2, -1.93922493945740712e2, -1.80911787048386799e2],
            [2.58228922114226407e2, -2.02187751038216078e2, -1.88932333357536692e2],
            [2.18097961827552155e2, -1.82898811354278536e2, -1.95326557847539732e2],
            [2.14109237556411955e2, -1.65993262531824939e2, -1.90170241811593172e2],
            [2.11487872353995698e2, -1.50473127068637012e2, -1.85077449585972545e2],
            [2.10049717103239914e2, -1.36415924438304899e2, -1.80141680516868348e2],
            [2.09603692933211647e2, -1.23879816889681777e2, -1.75451345105660835e2],
            [2.09953492147067095e2, -1.12903369615806056e2, -1.71089085396358200e2],
            [2.10899305918773081e2, -1.03505453378066392e2, -1.67131141174010850e2],
            [2.12239567392294248e2, -9.56852896989002772e1, -1.63646765940028871e2],
            [2.13772698749130910e2, -8.94226378657662906e1, -1.60697696317329417e2],
            [2.15298850809177395e2, -8.46781221230482970e1, -1.58337678203883314e2],
            [2.16621623795179346e2, -8.13936965755389679e1, -1.56612052641069937e2],
            [2.17549758023290735e2, -7.94932444861210001e1, -1.55557403992631635e2],
            [2.17898783479367808e2, -7.88833078324570067e1, -1.55201272645972153e2],
            [2.52740028174525662e2, -2.44169013372904651e1, -1.22131575967539291e2],
            [1.92045267100313453e2, 2.97823011093903176e1, -1.22131575967539305e2],
            [1.41852891343625828e2, -1.09758537102927249e1, -1.55201272645977099e2],
            [1.41286180013423490e2, -1.13914012509241385e1, -1.55557403992576241e2],
            [1.39502448034919070e2, -1.25278217360535429e1, -1.56612052641084802e2],
            [1.36388041249471542e2, -1.42123542896731898e1, -1.58337678203885616e2],
            [1.31845771017385118e2, -1.62635370903352481e1, -1.60697696317319753e2],
            [1.25795825050526574e2, -1.84927642072737370e1, -1.63646765940034413e2],
            [1.18176553449439325e2, -2.07058979707320816e1, -1.67131141174009201e2],
            [1.08945124591873451e2, -2.27049263722665700e1, -1.71089085396359252e2],
            [9.80780463109021241e1, -2.42896547027213003e1, -1.75451345105660380e2],
            [8.55715486195498443e1, -2.52594204100508080e1, -1.80141680516868149e2],
            [7.14418250807187860e1, -2.54148199976080917e1, -1.85077449585973852e2],
            [5.57251307809681151e1, -2.45594366870657979e1, -1.90170241811590614e2],
            [3.84777357395986286e1, -2.25015575393053133e1, -1.95326557847539732e2],
            [1.47885367322583576e1, 1.51996230529755874e1, -1.88932333357536777e2],
            [2.14245869497542216e1, 3.00296187298490196e1, -1.80911787048388192e2],
            [2.84309885655538928e1, 4.30366001783488201e1, -1.73467899423658650e2],
            [3.55994474215832710e1, 5.42707997234581825e1, -1.66653135243643248e2],
            [4.27271920014762401e1, 6.38008102222483870e1, -1.60510958642687740e2],
            [4.96186117409568936e1, 7.17127689752888102e1, -1.55075583155939199e2],
            [5.60868334075065462e1, 7.81094206687739216e1, -1.50371789861771617e2],
            [6.19552254947934031e1, 8.31090659020767504e1, -1.46414814576489846e2],
            [6.70588210401601827e1, 8.68444025389179046e1, -1.43210304614100068e2],
            [7.12456497916228386e1, 8.94612677607946409e1, -1.40754345197648007e2],
            [7.43779712208565513e1, 9.11172892946968460e1, -1.39033555182631829e2],
            [7.63334004980445116e1, 9.19804548308182746e1, -1.38025251329171397e2],
            [7.70059202088179262e1, 9.22276091361767101e1, -1.37697679940941043e2],
            [1.40288738129762748e2, 1.12152424512200568e2, -1.08161375246168504e2],
            [1.19472476707453467e2, 1.87677396295751208e2, -8.61597965481743557e1],
            [5.09247372076361273e1, 1.86854620696262572e2, -1.10131385076744351e2],
            [5.01775585644335465e1, 1.86878341820931013e2, -1.10380046311815065e2],
            [4.79286880391457686e1, 1.87079831533579636e2, -1.11078199817795252e2],
            [4.41818964829441470e1, 1.87653213159952799e2, -1.12149530615262023e2],
            [3.89606406534709464e1, 1.88789427047507701e2, -1.13512160720407920e2],
            [3.23079296316629936e1, 1.90674547430927191e2, -1.15079345642756834e2],
            [2.42860482814577878e1, 1.93488131120854405e2, -1.16760207915608419e2],
            [1.49761393486749359e1, 1.97401608909608541e2, -1.18460502898777946e2],
            [4.47764666520993515e0, 2.02576730338205351e2, -1.20083411889431943e2],
            [-7.09237722138249538e0, 2.09164072157694648e2, -1.21530357402449027e2],
            [-1.96000948361396006e1, 2.17301620445571189e2, -1.22701835336866154e2],
            [-3.28958340128616555e1, 2.27113435907284781e2, -1.23498258630826498e2],
            [-4.68151588224433368e1, 2.38708411405574054e2, -1.23820806924579458e2],
            [-4.50141276959065024e1, 2.80421263335945127e2, -1.07080509878054784e2],
            [-3.02880486218036964e1, 2.87511151571679420e2, -9.92593093774581234e1],
            [-1.63300874661959838e1, 2.93036754183213361e2, -9.23076384178294802e1],
            [-3.29312213942680954e0, 2.97156868253263610e2, -8.62168874183537639e1],
            [8.68583502399541629e0, 3.00040641604824884e2, -8.09689723243117356e1],
            [1.94865871793577696e1, 3.01866012510959990e2, -7.65366097544369666e1],
            [2.90065759109062036e1, 3.02818096127469744e2, -7.28836565507659486e1],
            [3.71615722939237685e1, 3.03087528367488858e2, -6.99655112805156847e1],
            [4.38862319943055894e1, 3.02868778172164070e2, -6.77295748667955593e1],
            [4.91345111592232442e1, 3.02358439302439876e2, -6.61157671694165998e1],
            [5.28799407094289009e1, 3.01753512882894597e2, -6.50570960004229164e1],
            [5.51157575143372469e1, 3.01249691968756110e2, -6.44802747430297813e1],
            [5.58548918084258545e1, 3.01039659381690740e2, -6.43063844497556829e1],
            [1.23407397413239480e2, 2.78812281384503535e2, -4.95853362299672256e1],
            [1.50420739987452293e2, 3.46815050282984203e2, -1.39860340904570162e1],
            [8.97005433813657191e1, 3.86241926217610626e2, -1.97031824155941031e1],
            [8.90582942713637067e1, 3.86695855344159213e2, -1.97493933989852266e1],
            [8.72032773035712410e1, 3.88158293032749157e2, -1.98243808525418217e1],
            [8.42552480023476562e1, 3.90770576546379516e2, -1.98322049332229078e1],
            [8.03493488383814167e1, 3.94660192035576017e2, -1.96769686440182952e1],
            [7.56349822546144850e1, 3.99939608005790319e2, -1.92636574923168062e1],
            [7.02745771773647903e1, 4.06705215809730248e2, -1.84989745462234509e1],
            [6.44422569495086179e1, 4.15036384287698070e2, -1.72921653975427887e1],
            [5.83224172379943226e1, 4.24994634985521714e2, -1.55558275957699834e1],
            [5.21082230301995395e1, 4.36622943652298318e2, -1.32066992044904374e1],
            [4.60000343466535071e1, 4.49945172954744066e2, -1.01664212503429781e1],
            [4.02037707529244059e1, 4.64965640552587161e2, -6.36226898378618611e0],
            [3.49292251519390220e1, 4.81668825859782487e2, -1.72784705014276874e0],
            [6.15325537368123534e1, 5.11460457458533654e2, 1.89642980629006637e1],
            [7.87237999822355050e1, 5.08102141923514523e2, 2.35987199965451779e1],
            [9.43018539298620340e1, 5.04035773308804210e2, 2.74028722631021111e1],
            [1.08227648082692525e2, 4.99468058366562616e2, 3.04431502172484549e1],
            [1.20482263342994884e2, 4.94604091166450587e2, 3.27922786085265443e1],
            [1.31066827694964417e2, 4.89645644581611521e2, 3.45286164103015949e1],
            [1.40002271368568472e2, 4.84789496539765707e2, 3.57354255589818166e1],
            [1.47328940607125418e2, 4.80225801828490034e2, 3.65001085050752252e1],
            [1.53106073019788028e2, 4.76136519940555900e2, 3.69134196567756163e1],
            [1.57411138339594345e2, 4.72693909081680488e2, 3.70686559459816891e1],
            [1.60339049224992380e2, 4.70059096040307281e2, 3.70608318653007629e1],
            [1.62001247530842875e2, 4.68380731140819591e2, 3.69858444117448073e1],
            [1.62524672230142272e2, 4.67793736970859982e2, 3.69396334283471361e1],
            [2.08544104403000546e2, 4.11904268514590910e2, 3.12224851032155080e1],
            [2.73068872260130433e2, 4.46410088137885793e2, 6.68217872427254349e1],
            [2.43369269847860636e2, 5.11026889069709682e2, 8.15428354625017278e1],
            [2.43077267601608014e2, 5.11737640700506404e2, 8.17167257558007947e1],
            [2.42324657623405813e2, 5.13902424874273493e2, 8.22935470131746172e1],
            [2.41301437570385104e2, 5.17555806761251802e2, 8.33522181821824972e1],
            [2.40202815795912102e2, 5.22713122938406741e2, 8.49660258795507843e1],
            [2.39227521449784604e2, 5.29370277035497566e2, 8.72019622932735103e1],
            [2.38576085191513755e2, 5.37503676726929143e2, 9.01201075635276538e1],
            [2.38449101762365785e2, 5.47070312701505600e2, 9.37730607671927885e1],
            [2.39045485786891447e2, 5.58007979370504700e2, 9.82054233370733556e1],
            [2.40560732232574935e2, 5.70235636205504306e2, 1.03453338431104868e2],
            [2.43185192947671709e2, 5.83653907734921177e2, 1.09544089430590901e2],
            [2.47102380621979393e2, 5.98145719373441352e2, 1.16495760390220426e2],
            [2.52487311373953901e2, 6.13577065422573696e2, 1.24316960890812709e2],
            [2.93731370111624528e2, 6.20068085999297750e2, 1.41057257937337738e2],
            [3.06821311771572482e2, 6.07544329082964509e2, 1.40734709643584893e2],
            [3.18069171381826436e2, 5.95439203842018287e2, 1.39938286349623297e2],
            [3.27564614314655955e2, 5.83928371744526885e2, 1.38766808415208203e2],
            [3.35414041161458613e2, 5.73174534143018377e2, 1.37319862902191886e2],
            [3.41739473883699930e2, 5.63326228131189168e2, 1.35696953911533512e2],
            [3.46677336720599897e2, 5.54516731928580384e2, 1.33996658928363729e2],
            [3.50377140189890895e2, 5.46863086531481940e2, 1.32315796655524792e2],
            [3.53000077090709226e2, 5.40465239642469328e2, 1.30748611733155315e2],
            [3.54717539937004858e2, 5.35405317132569053e2, 1.29385981628020829e2],
            [3.55709569715481507e2, 5.31747026497147203e2, 1.28314650830572219e2],
            [3.56163246265467933e2, 5.29535195958094505e2, 1.27616497324476839e2],
            [3.56271030925328432e2, 5.28795452027564124e2, 1.27367836089618734e2],
            [3.63179530719196919e2, 4.60591770449032936e2, 1.03396247560932409e2],
            [4.40569455654017645e2, 4.48420590510717091e2, 1.25397826258926557e2],
            [4.53234686104595198e2, 5.13545894728196345e2, 1.54934130953703544e2],
            [4.53404465655540548e2, 5.14241985897576569e2, 1.55261702341925059e2],
            [4.54041734098221809e2, 5.16282242580610728e2, 1.56270006195371138e2],
            [4.55334158575725951e2, 5.19581255133962600e2, 1.57990796210417557e2],
            [4.57462450367823408e2, 5.24036352510108372e2, 1.60446755626854866e2],
            [4.60598757550490404e2, 5.29528438137361036e2, 1.63651265589241007e2],
            [4.64905116620396939e2, 5.35922947478260767e2, 1.67608240874549466e2],
            [4.70531973343838331e2, 5.43070921165126151e2, 1.72312034168684903e2],
            [4.77616782677408708e2, 5.50810186895357106e2, 1.77747409655449644e2],
            [4.86282697135642934e2, 5.58966642590415859e2, 1.83889586256397450e2],
            [4.96637352458357441e2, 5.67355632692812378e2, 1.90704350436420611e2],
            [5.08771758849421417e2, 5.75783408884649475e2, 1.98148238061144923e2],
            [5.22759305434172461e2, 5.84048665977125097e2, 2.06168784370294873e2],
            [5.62890265720846742e2, 5.64759726293187441e2, 2.12563008860297884e2],
            [5.66878989991980916e2, 5.47854177470759623e2, 2.07406692824359169e2],
            [5.69500355194400868e2, 5.32334042007550124e2, 2.02313900598731522e2],
            [5.70938510445158727e2, 5.18276839377197575e2, 1.97378131529620134e2],
            [5.71384534615187476e2, 5.05740731828565799e2, 1.92687796118409437e2],
            [5.71034735401333364e2, 4.94764284554730352e2, 1.88325536409122833e2],
            [5.70088921629624224e2, 4.85366368316960234e2, 1.84367592186762636e2],
            [5.68748660156114624e2, 4.77546204637861138e2, 1.80883216952810528e2],
            [5.67215528799250478e2, 4.71283552804614601e2, 1.77934147330058124e2],
            [5.65689376739217209e2, 4.66539037061938416e2, 1.75574129216632684e2],
            [5.64366603753257777e2, 4.63254611514533849e2, 1.73848503653874502e2],
            [5.63438469525136611e2, 4.61354159425096043e2, 1.72793855005425115e2],
            [5.63089444068892931e2, 4.60744222771150589e2, 1.72437723658599651e2],
            [5.28248199373872808e2, 4.06277816276199587e2, 1.39368026980297373e2],
            [5.88942960448085387e2, 3.52078613829518474e2, 1.39368026980297401e2],
            [6.39135336204516420e2, 3.92836768648993143e2, 1.72437723658566142e2],
            [6.39702047535228303e2, 3.93252316189998282e2, 1.72793855005485483e2],
            [6.41485779513437478e2, 3.94388736674939707e2, 1.73848503653819535e2],
            [6.44600186298921244e2, 3.96073269228578738e2, 1.75574129216640358e2],
            [6.49142456531021935e2, 3.98124452029247607e2, 1.77934147330082055e2],
            [6.55192402497883563e2, 4.00353679146186892e2, 1.80883216952798080e2],
            [6.62811674098959429e2, 4.02566812909642010e2, 1.84367592186767695e2],
            [6.72043102956508051e2, 4.04565841311172733e2, 1.88325536409110327e2],
            [6.82910181237546340e2, 4.06150569641637333e2, 1.92687796118438371e2],
            [6.95416678928801048e2, 4.07120335348959088e2, 1.97378131529609476e2],
            [7.09546402467683151e2, 4.07275734936516869e2, 2.02313900598732971e2],
            [7.25263096767429715e2, 4.06420351625974661e2, 2.07406692824348369e2],
            [7.42510491808800111e2, 4.04362472478214272e2, 2.12563008860297828e2],
            [7.66199690816140219e2, 3.66661291885933508e2, 2.06168784370294901e2],
            [7.59563640598647339e2, 3.51831296209066863e2, 1.98148238061149954e2],
            [7.52557238982833951e2, 3.38824314760541370e2, 1.90704350436405775e2],
            [7.45388780126816755e2, 3.27590115215450908e2, 1.83889586256401856e2],
            [7.38261035546943276e2, 3.18060104716686510e2, 1.77747409655463002e2],
            [7.31369615807417517e2, 3.10148145963594516e2, 1.72312034168678963e2],
            [7.24901394140903562e2, 3.03751494270144804e2, 1.67608240874537444e2],
            [7.19033002053586642e2, 2.98751849036816338e2, 1.63651265589235351e2],
            [7.13929406508251304e2, 2.95016512399997850e2, 1.60446755626865183e2],
            [7.09742577756815763e2, 2.92399647178138423e2, 1.57990796210429153e2],
            [7.06610256327546949e2, 2.90743625644217559e2, 1.56270006195393677e2],
            [7.04654827050085146e2, 2.89880460107975296e2, 1.55261702341792073e2],
            [7.03982307339896806e2, 2.89633305802832012e2, 1.54934130953846733e2],
            [6.40699489418636063e2, 2.69708490426708693e2, 1.25397826258926742e2],
            [6.61515750840945429e2, 1.94183518643157782e2, 1.03396247560932508e2],
            [7.30063490340948533e2, 1.95006294242648636e2, 1.27367836089567504e2],
            [7.30810668983796859e2, 1.94982573117991478e2, 1.27616497324520296e2],
            [7.33059539509291426e2, 1.94781083405324381e2, 1.28314650830564716e2],
            [7.36806331065407448e2, 1.94207701778964179e2, 1.29385981628006959e2],
            [7.42027586894943852e2, 1.93071487891395634e2, 1.30748611733169412e2],
            [7.48680297916750760e2, 1.91186367507976428e2, 1.32315796655518085e2],
            [7.56702179266944654e2, 1.88372783818053250e2, 1.33996658928367310e2],
            [7.66012088199706341e2, 1.84459306029308038e2, 1.35696953911533001e2],
            [7.76510580883223270e2, 1.79284184600685506e2, 1.37319862902194956e2],
            [7.88080604769753791e2, 1.72696842781231538e2, 1.38766808415204338e2],
            [8.00588322384551589e2, 1.64559294493328622e2, 1.39938286349625287e2],
            [8.13884061561260978e2, 1.54747479031624266e2, 1.40734709643584836e2],
            [8.27803386370842532e2, 1.43152503533335022e2, 1.41057257937337766e2],
            [8.26002355244305591e2, 1.01439651602964886e2, 1.24316960890813419e2],
            [8.11276276170186179e2, 9.43497633672226215e1, 1.16495760390207963e2],
            [7.97318315014595555e2, 8.88241607556954591e1, 1.09544089430587789e2],
            [7.84281349687834336e2, 8.47040466856476826e1, 1.03453338431115810e2],
            [7.72302392524410379e2, 8.18202733340860959e1, 9.82054233370730572e1],
            [7.61501640369034703e2, 7.99949024279486025e1, 9.37730607671927459e1],
            [7.51981651637492405e2, 7.90428188114395311e1, 9.01201075635241011e1],
            [7.43826655254497837e2, 7.87733865714211419e1, 8.72019622932820653e1],
            [7.37101995554012547e2, 7.89921367667493257e1, 8.49660258795275922e1],
            [7.31853716389278702e2, 7.95024756364556424e1, 8.33522181822051209e1],
            [7.28108286838952381e2, 8.01074020560203905e1, 8.22935470131773457e1],
            [7.25872470033946456e2, 8.06112229701809468e1, 8.17167257557590574e1],
            [7.25133335740086750e2, 8.08212555571811464e1, 8.15428354625387755e1],
            [6.57580830135159317e2, 1.03048633554405839e2, 6.68217872427254918e1],
            [6.30567487560946461e2, 3.50458646559251790e1, 3.12224851032153126e1],
            [6.91287684166977215e2, -4.38101127866494267e0, 3.69396334283470935e1],
            [6.91929933277070290e2, -4.83494040527804714e0, 3.69858444117447220e1],
            [6.93784950244827542e2, -6.29737809384002389e0, 3.70608318652999245e1],
            [6.96732979546042543e2, -8.90966160746153690e0, 3.70686559459815683e1],
            [7.00638878710031463e2, -1.27992770966821681e1, 3.69134196567753605e1],
            [7.05353245293790678e2, -1.80786930668892509e1, 3.65001085050741167e1],
            [7.10713650371037829e2, -2.48443008708258795e1, 3.57354255589811203e1],
            [7.16545970598879876e2, -3.31754693487735324e1, 3.45286164103033570e1],
            [7.22665810310413576e2, -4.31337200466287953e1, 3.27922786085250380e1],
            [7.28880004518192891e2, -5.47620287133761607e1, 3.04431502172514463e1],
            [7.34988193201750391e2, -6.80842580158468991e1, 2.74028722630982742e1],
            [7.40784456795469396e2, -8.31047256136645274e1, 2.35987199965478922e1],
            [7.46059002396459277e2, -9.98079109208739084e1, 1.89642980629005677e1],
            [7.19455673811586394e2, -1.29599542519624549e2, -1.72784705014250584e0],
            [7.02264427566164272e2, -1.26241226984605660e2, -6.36226898378676520e0],
            [6.86686373618556900e2, -1.22174858369900349e2, -1.01664212503390168e1],
            [6.72760579465673800e2, -1.17607143427641461e2, -1.32066992044968501e1],
            [6.60505964205441387e2, -1.12743176227557086e2, -1.55558275957614871e1],
            [6.49921399853396224e2, -1.07784729642682748e2, -1.72921653975489669e1],
            [6.40985956179874393e2, -1.02928581600881486e2, -1.84989745462180721e1],
            [6.33659286941239316e2, -9.83648868895571837e1, -1.92636574923196271e1],
            [6.27882154528616638e2, -9.42756050016512575e1, -1.96769686440171228e1],
            [6.23577089208792358e2, -9.08329941427614642e1, -1.98322049332238670e1],
            [6.20649178323419846e2, -8.81981811014113362e1, -1.98243808525430296e1],
            [6.18986980017555993e2, -8.65198162019104586e1, -1.97493933989865731e1],
            [6.18463555318256567e2, -8.59328220319509626e1, -1.97031824155889375e1],
        ];
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        let n = verts.len();
        for p in &verts {
            arena.vertices.push(Some(Vertex { point: Point3::new(p[0], p[1], p[2]) }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop { face: fid, boundary: LoopBoundary::Edges(HalfEdgeId(0)), kind: LoopKind::Outer }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: Point3::new(5.72444123145398748e2, -3.00433535756816532e1, -1.39860340904570997e1),
                normal: UnitVector3 { x: 3.06392220406831617e-1, y: 3.43112108169570662e-1, z: -8.87917726200803115e-1 },
            })),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell { solid, faces: vec![fid], genus: 0 }));
        arena.solids.push(Some(Solid { shells: vec![shell] }));
        (arena, fid)
    }

    /// A z=0 planar rectangle at coordinate magnitude ~1 with one extra vertex
    /// `offset` above its right-edge neighbor in y — below f32 resolution at this
    /// scale (f32 ulp ~ 6e-8 near 0.5) so the pair rounds to one f32 while staying
    /// f64-distinct. The planar analogue of `build_cylinder_patch(true)`.
    fn build_planar_twin(offset: f64) -> (BrepArena, FaceId) {
        let bpts = [
            Point3::new(0.2, 0.0, 0.0),
            Point3::new(1.2, 0.0, 0.0),
            Point3::new(1.2, 0.5, 0.0),
            Point3::new(1.2, 0.5 + offset, 0.0), // sub-f32 twin of the previous
            Point3::new(1.2, 1.0, 0.0),
            Point3::new(0.2, 1.0, 0.0),
        ];
        let n = bpts.len();
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        for p in &bpts {
            arena.vertices.push(Some(Vertex { point: *p }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop {
            face: fid,
            boundary: LoopBoundary::Edges(HalfEdgeId(0)),
            kind: LoopKind::Outer,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: Point3::new(0.2, 0.0, 0.0),
                normal: UnitVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
            })),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell {
            solid,
            faces: vec![fid],
            genus: 0,
        }));
        arena.solids.push(Some(Solid {
            shells: vec![shell],
        }));
        (arena, fid)
    }

    /// RED (I1, cylinder root fix): the banked F0047 patch ring must tessellate
    /// with ZERO f32-degenerate triangles and a watertight per-face partition.
    /// TODAY it fails loudly at the render-degeneracy gate (the ear-clip minted
    /// slivers), so `expect` panics — RED via the gate error.
    #[test]
    fn red_f0047_patch_ring_tessellates_clean() {
        let (arena, fid) = build_f0047_patch();
        let mut mesh = RenderMesh::default();
        tessellate_cylinder_patch(&arena, fid, 71, &mut mesh).expect(
            "I1: the banked F0047 patch ring must tessellate cleanly (RED today: the \
             render-degeneracy gate rejects the ear-clip's minted slivers)",
        );
        assert_eq!(
            scan_degeneracy(&mesh),
            (0, 0),
            "I1: zero f32-degenerate triangles (B2 + B3) on the healthy F0047 ring"
        );
        assert!(
            max_edge_incidence(&mesh) <= 2,
            "I1: watertight per-face partition — no undirected edge shared by >2 triangles"
        );
    }

    /// RED (I1/I3, planar root fix): the banked R0064 gear loop must tessellate
    /// with ZERO f32-arithmetic-degenerate triangles and an exact 278-triangle
    /// partition of the simple 280-gon. TODAY it returns `Ok` with exactly one
    /// silent f32-zero-cross sliver — RED on the zero-degenerate assertion.
    #[test]
    fn red_r0064_planar_ring_no_f32_degenerate() {
        let (arena, fid) = build_r0064_planar();
        let mut mesh = RenderMesh::default();
        super::tessellate_planar_face(&arena, fid, 71, &mut mesh)
            .expect("I1: the banked R0064 planar gear loop must tessellate");
        assert_eq!(
            f32_zero_cross(&mesh),
            0,
            "I1/I3: zero f32-arithmetic-degenerate triangles (RED today: one silent \
             zero-cross sliver spanning three near-collinear boundary vertices)"
        );
        assert_eq!(
            mesh.indices.len() / 3,
            278,
            "I3: an exact partition of the simple 280-gon has 280 - 2 = 278 triangles"
        );
    }

    /// RED (G1, new planar gate / I6): a planar loop carrying a sub-f32 twin must
    /// fail loudly with the typed reason. TODAY the planar path has NO such gate,
    /// so it tessellates `Ok` with a silently degenerate triangle — RED because
    /// it expects `Err` but gets `Ok`. (Twin verified to mint at least one
    /// f32-degenerate triangle today via the fallthrough witness below.)
    #[test]
    fn red_planar_sub_f32_twin_fails_loudly() {
        let (arena, fid) = build_planar_twin(1e-12);
        let mut mesh = RenderMesh::default();
        match super::tessellate_planar_face(&arena, fid, 71, &mut mesh) {
            Err(KernelV2Error::TessellationFailed { face, reason }) => {
                assert_eq!(face, fid, "G1 must fail THIS planar face");
                assert_eq!(
                    reason, "planar triangle collapsed at render precision",
                    "G1 must use the spec's typed reason"
                );
            }
            Ok(()) => {
                // RED witness: today the planar path tessellates silently.
                let (b2, b3) = scan_degeneracy(&mesh);
                assert!(
                    b2 + b3 > 0,
                    "fixture defect: expected a sub-f32 degenerate triangle, found none in {} tris",
                    mesh.indices.len() / 3
                );
                panic!(
                    "G1 RED: planar sub-f32 twin tessellated OK with {b2} B2 + {b3} B3 \
                     degenerate triangle(s) (silent); spec requires TessellationFailed \
                     {{ reason: \"planar triangle collapsed at render precision\" }}"
                );
            }
            Err(e) => panic!(
                "expected the planar render-degeneracy gate (TessellationFailed), got a \
                 different error: {e:?}"
            ),
        }
    }

    /// GUARD (I5): PASSES today and must keep passing. Byte-identical output for
    /// identical input — build the R0064 fixture twice and tessellate each into
    /// its own mesh; the planar core is deterministic and the CDT primitive that
    /// replaces it must preserve that (it canonicalizes its own output).
    #[test]
    fn cdt_determinism_guard() {
        let (a1, f1) = build_r0064_planar();
        let (a2, f2) = build_r0064_planar();
        let mut m1 = RenderMesh::default();
        let mut m2 = RenderMesh::default();
        super::tessellate_planar_face(&a1, f1, 71, &mut m1).expect("R0064 must tessellate (run 1)");
        super::tessellate_planar_face(&a2, f2, 71, &mut m2).expect("R0064 must tessellate (run 2)");
        assert_eq!(
            m1.indices, m2.indices,
            "I5: byte-identical triangle indices"
        );
        assert_eq!(
            m1.positions, m2.positions,
            "I5: byte-identical vertex positions"
        );
    }
}

#[cfg(test)]
mod cdt_core_round2_red_tests {
    //! KV2 CDT triangulation core — RED tests, ROUND 2 (spec
    //! `specs/kv2_cdt_triangulation_core.md` §6b: the three Phase-4-assay
    //! follow-up mechanisms M1/M2/M3).
    //!
    //! Round 1 swapped both f64 render cores to the exact-predicate CDT
    //! primitive. The full assay then measured three regression classes; this
    //! module banks the measured witnesses and asserts the round-2 SPEC TARGET,
    //! so each test is RED until the corresponding mechanism lands:
    //!
    //! * `red_m1_f0016_planar_ring_no_grid_degenerate` — M1 grid-degeneracy
    //!   flip pass, planar path. The banked F0016 FaceId(61) 6-vertex ring: the
    //!   CDT prefers the on-line chord diagonal, minting a boundary-chord sliver
    //!   flatter than the render weld grid (`max_abs·TAU_TESS_GRID_FACTOR`) — a
    //!   grid-degenerate the bitwise B2/B3 gates cannot see.
    //! * `red_m1_r0040_patch_ring_tessellates_clean` — M1 flip pass, cylinder
    //!   patch path. The banked R0040 FaceId(23) 28-vertex barrel-cut ring:
    //!   today rejected loudly by the G0 gate (the same chord sliver, bitwise).
    //! * `red_m3_pinch_ring_tessellates` — M3 pinch-splitting. A weakly-simple
    //!   ring visiting one geometric point through two distinct arena vertices
    //!   at bitwise-identical positions → spade `DuplicateVertex` today.
    //! * `guard_m3_consecutive_duplicate_stays_loud` — GUARD (M3 boundary):
    //!   a zero-length edge (two CONSECUTIVE coincident vertices) must stay
    //!   loud both before and after M3.
    //!
    //! (M2 flood-fill interior classification is exercised at the cherchi-rs
    //! primitive level — see `triangulation::floodfill_red_tests` — and E2E by
    //! the full-assay F0047 diff; the primitive-level centroid-parity defect
    //! could not be reproduced synthetically, see that module's note.)
    //!
    //! In-module because the target fns are private (same idiom as
    //! `cdt_core_red_tests`). Predicate helpers are re-declared (not importable
    //! across cfg(test) sibling modules). The banked coordinate builders carry
    //! `#[rustfmt::skip]` to preserve the banked one-triple-per-line layout.
    use super::{tessellate_cylinder_patch, tessellate_planar_face};
    use crate::arena::{
        BrepArena, Curve, Face, FaceId, HalfEdge, HalfEdgeId, Loop, LoopBoundary, LoopId, LoopKind,
        Plane, Shell, ShellId, Solid, SolidId, Surface, UnitVector3, Vertex, VertexId,
    };
    use crate::error::KernelV2Error;
    use crate::tessellate::RenderMesh;
    use cad_primitives::Point3;
    use waffle_types::kernel::units::{TAU_TESS_GRID_FACTOR, TAU_TESS_GRID_MIN};

    // ── shared predicates (re-declared; private per cfg(test) module) ──────

    /// (b2, b3_only): triangles with two bitwise-equal f32 verts (B2); and
    /// triangles with three DISTINCT f32 verts but an exactly-zero f32 cross
    /// (B3-only). The bitwise render-degeneracy gate's own predicate.
    fn scan_degeneracy(mesh: &RenderMesh) -> (usize, usize) {
        let key = |i: usize| -> [u32; 3] {
            [
                (mesh.positions[3 * i] as f32).to_bits(),
                (mesh.positions[3 * i + 1] as f32).to_bits(),
                (mesh.positions[3 * i + 2] as f32).to_bits(),
            ]
        };
        let fpos = |i: usize| -> [f32; 3] {
            [
                mesh.positions[3 * i] as f32,
                mesh.positions[3 * i + 1] as f32,
                mesh.positions[3 * i + 2] as f32,
            ]
        };
        let (mut b2, mut b3) = (0usize, 0usize);
        for t in mesh.indices.chunks_exact(3) {
            let (ka, kb, kc) = (key(t[0] as usize), key(t[1] as usize), key(t[2] as usize));
            if ka == kb || kb == kc || ka == kc {
                b2 += 1;
                continue;
            }
            let (fa, fb, fc) = (
                fpos(t[0] as usize),
                fpos(t[1] as usize),
                fpos(t[2] as usize),
            );
            let uu = [fb[0] - fa[0], fb[1] - fa[1], fb[2] - fa[2]];
            let vv = [fc[0] - fa[0], fc[1] - fa[1], fc[2] - fa[2]];
            let cx = uu[1] * vv[2] - uu[2] * vv[1];
            let cy = uu[2] * vv[0] - uu[0] * vv[2];
            let cz = uu[0] * vv[1] - uu[1] * vv[0];
            if cx == 0.0 && cy == 0.0 && cz == 0.0 {
                b3 += 1;
            }
        }
        (b2, b3)
    }

    /// M1 grid-degeneracy count (spec §6b): emitted triangles whose f32-rounded
    /// height is below the shared render weld grid
    /// `(max_abs·TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN)` — the SAME
    /// constant the watertight oracle welds at (A3.3 single ownership), ~100×
    /// coarser than f32 ulp, so it sees the boundary-chord sliver the bitwise
    /// `scan_degeneracy` cannot. Heights on the f32-rounded 3D positions with
    /// f32 arithmetic, matching `oracle::check_no_degenerate_triangles`.
    fn grid_degenerate(mesh: &RenderMesh) -> usize {
        let fp = |i: u32| -> [f32; 3] {
            let i = i as usize * 3;
            [
                mesh.positions[i] as f32,
                mesh.positions[i + 1] as f32,
                mesh.positions[i + 2] as f32,
            ]
        };
        let max_abs = mesh
            .positions
            .iter()
            .map(|&p| (p as f32).abs())
            .fold(0.0_f32, f32::max) as f64;
        let grid = (max_abs * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
        let mut count = 0usize;
        for t in mesh.indices.chunks_exact(3) {
            let (pa, pb, pc) = (fp(t[0]), fp(t[1]), fp(t[2]));
            let ax = pb[0] - pa[0];
            let ay = pb[1] - pa[1];
            let az = pb[2] - pa[2];
            let bx = pc[0] - pa[0];
            let by = pc[1] - pa[1];
            let bz = pc[2] - pa[2];
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
            if (height as f64) < grid {
                count += 1;
            }
        }
        count
    }

    /// Highest incidence count over undirected triangle index-edges. A
    /// watertight per-face partition has every edge count 1 (boundary) or 2
    /// (interior); any edge shared by ≥3 triangles is a non-manifold fan.
    fn max_edge_incidence(mesh: &RenderMesh) -> usize {
        use std::collections::HashMap;
        let mut counts: HashMap<(u32, u32), usize> = HashMap::new();
        for t in mesh.indices.chunks_exact(3) {
            for &(a, b) in &[(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
        counts.values().copied().max().unwrap_or(0)
    }

    // ── fixture builders ───────────────────────────────────────────────────

    /// Build a single planar face from a projected loop of z-plane points at
    /// `normal = +z, plane point = origin`, all LineSegment half-edges. Shared
    /// builder for the synthetic M3 pinch + consecutive-duplicate fixtures.
    fn build_planar_loop(pts: &[Point3]) -> (BrepArena, FaceId) {
        let n = pts.len();
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        for p in pts {
            arena.vertices.push(Some(Vertex { point: *p }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop {
            face: fid,
            boundary: LoopBoundary::Edges(HalfEdgeId(0)),
            kind: LoopKind::Outer,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: Point3::new(0.0, 0.0, 0.0),
                normal: UnitVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
            })),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell {
            solid,
            faces: vec![fid],
            genus: 0,
        }));
        arena.solids.push(Some(Solid {
            shells: vec![shell],
        }));
        (arena, fid)
    }

    /// Banked F0016 FaceId(61): a 6-vertex all-LineSegment planar ring (boolean
    /// output, coordinate scale ~0.28) that mints ONE boundary-chord sliver
    /// under the round-1 CDT — grid-degenerate but NOT bitwise-degenerate.
    /// Verbatim measured fixture (§6b, 2026-07-02).
    #[rustfmt::skip]
    fn build_f0016_planar() -> (BrepArena, FaceId) {
        let verts: [[f64; 3]; 6] = [
            [1.43678157469419809e-1, 1.15954355224674524e-1, 1.63568283439396556e-1],
            [1.43678157469419809e-1, 1.15954355224674524e-1, 1.84341198824998137e-1],
            [1.25307302742208193e-1, 1.39843650855904000e-1, 1.69508915250426079e-1],
            [6.58462835491393506e-2, 2.17166229861079974e-1, 1.88736982064941494e-1],
            [1.27043537062290351e-1, 1.37585867232146331e-1, 2.75724685742304187e-1],
            [1.95063117800456154e-1, 4.91338111305104769e-2, 1.46951793108171941e-1],
        ];
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        let n = verts.len();
        for p in &verts {
            arena.vertices.push(Some(Vertex { point: Point3::new(p[0], p[1], p[2]) }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop { face: fid, boundary: LoopBoundary::Edges(HalfEdgeId(0)), kind: LoopKind::Outer }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: Point3::new(1.43678157469419809e-1, 1.15954355224674524e-1, 1.63568283439396556e-1),
                normal: UnitVector3 { x: 7.92712605646587187e-1, y: 6.09595542018639081e-1, z: 0.00000000000000000e0 },
            })),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell { solid, faces: vec![fid], genus: 0 }));
        arena.solids.push(Some(Solid { shells: vec![shell] }));
        (arena, fid)
    }

    /// Banked R0040 FaceId(23): a 28-vertex all-LineSegment CYLINDER-PATCH ring
    /// (barrel-cut boundary, n_seg=71, coordinate scale ~44) that mints the
    /// same boundary-chord sliver under the round-1 CDT — today rejected loudly
    /// by the G0 gate. Verbatim measured fixture (§6b, 2026-07-02).
    #[rustfmt::skip]
    fn build_r0040_patch() -> (BrepArena, FaceId) {
        let verts: [[f64; 3]; 28] = [
            [-2.29658777157921712e1, 9.28562110120933148e-1, 2.61019467100763265e1],
            [-2.82085496603433690e1, -1.33598555438530724e1, 2.64480149505118050e1],
            [-3.29625819957986295e1, -2.63165311222440863e1, 2.00224421426300090e1],
            [-3.62461006855921326e1, -3.52654577212390876e1, 8.15233392327518658e0],
            [-3.73809442191168628e1, -3.83583688456449750e1, -6.71071698596150323e0],
            [-3.61327276947762712e1, -3.49564701170955985e1, -2.14969704650776876e1],
            [-3.27592515040658157e1, -2.57623726667096093e1, -3.31525477507866242e1],
            [-2.79572565294419420e1, -1.26749793498302701e1, -3.92701642664410002e1],
            [-2.27185227708299706e1, 1.60270514248613161e0, -3.85863181259494041e1],
            [-1.81250320699813869e1, 1.41218393392356756e1, -3.12422474873586040e1],
            [-1.51255009980336563e1, 2.22967839429324997e1, -1.87547599308271913e1],
            [-1.43394376848438565e1, 2.44391268245991000e1, -3.70295861521555469e0],
            [-1.59291917115374382e1, 2.01063992115175623e1, 1.08044327755573200e1],
            [-1.95664232408577021e1, 1.01934609722397500e1, 2.17711302577531427e1],
            [-3.87002022096730958e0, 4.43417441332113427e0, 2.17711302577531498e1],
            [-2.32788691647042967e-1, 1.43471126525989554e1, 1.08044327755573129e1],
            [1.35696533504653694e0, 1.86798402656804861e1, -3.70295861521556091e0],
            [5.70902021856738884e-1, 1.65374973840138928e1, -1.87547599308271842e1],
            [-2.42862905009098906e0, 8.36255278031708116e0, -3.12422474873585934e1],
            [-7.02211975093957541e0, -4.15658141643247703e0, -3.85863181259494112e1],
            [-1.22608535095515538e1, -1.84342659087488983e1, -3.92701642664410073e1],
            [-1.70628484841754258e1, -3.15216592256282446e1, -3.31525477507866242e1],
            [-2.04363246748858813e1, -4.07157566760142231e1, -2.14969704650776983e1],
            [-2.16845411992264729e1, -4.41176554045635996e1, -6.71071698596152633e0],
            [-2.05496976657017427e1, -4.10247442801577193e1, 8.15233392327518125e0],
            [-1.72661789759082396e1, -3.20758176811627109e1, 2.00224421426300196e1],
            [-1.25121466404529738e1, -1.91191421027716792e1, 2.64480149505118121e1],
            [-7.26947469590178130e0, -4.83072444879768526e0, 2.61019467100763265e1],
        ];
        let mut arena = BrepArena::new();
        let (shell, solid, lid, fid) = (ShellId(0), SolidId(0), LoopId(0), FaceId(0));
        let n = verts.len();
        for p in &verts {
            arena.vertices.push(Some(Vertex { point: Point3::new(p[0], p[1], p[2]) }));
        }
        for i in 0..n {
            arena.half_edges.push(Some(HalfEdge {
                twin: HalfEdgeId(i as u32),
                next: HalfEdgeId(((i + 1) % n) as u32),
                prev: HalfEdgeId(((i + n - 1) % n) as u32),
                origin: VertexId(i as u32),
                loop_id: lid,
                curve: Curve::LineSegment,
            }));
        }
        arena.loops.push(Some(Loop { face: fid, boundary: LoopBoundary::Edges(HalfEdgeId(0)), kind: LoopKind::Outer }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Cylinder {
                axis_point: Point3::new(-17.99445556589601, -9.791477331725282, -6.338879609194002),
                axis_dir: UnitVector3 { x: 0.938800151062225, y: -0.3444623003545433, z: 0.0 },
                radius: 33.49858032434566,
                reversed: false,
            }),
            outer_loop: lid,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.shells.push(Some(Shell { solid, faces: vec![fid], genus: 0 }));
        arena.solids.push(Some(Solid { shells: vec![shell] }));
        (arena, fid)
    }

    // ── tests ──────────────────────────────────────────────────────────────

    /// RED (M1, planar): the banked F0016 6-vertex ring must tessellate into an
    /// exact 4-triangle partition of the simple hexagon with ZERO
    /// grid-degenerate triangles and a watertight per-face partition. TODAY the
    /// CDT emits the on-line boundary-chord sliver (grid-degenerate, invisible
    /// to the bitwise gates), so `grid_degenerate` returns ≥1 — RED.
    #[test]
    fn red_m1_f0016_planar_ring_no_grid_degenerate() {
        let (arena, fid) = build_f0016_planar();
        let mut mesh = RenderMesh::default();
        // n_seg is irrelevant for an all-LineSegment planar loop.
        tessellate_planar_face(&arena, fid, 32, &mut mesh)
            .expect("M1: the banked F0016 planar ring must tessellate");
        assert_eq!(
            grid_degenerate(&mesh),
            0,
            "M1: zero grid-degenerate triangles (RED today: the CDT mints the \
             on-line boundary-chord sliver flatter than max_abs·TAU_TESS_GRID_FACTOR)"
        );
        assert_eq!(
            mesh.indices.len() / 3,
            4,
            "exact partition of the simple hexagon: 6 - 2 = 4 triangles"
        );
        assert!(
            max_edge_incidence(&mesh) <= 2,
            "watertight per-face partition — every undirected index-edge count 1 or 2"
        );
    }

    /// RED (M1, cylinder patch): the banked R0040 28-vertex barrel-cut ring
    /// must tessellate with ZERO f32-degenerate (bitwise B2/B3) AND ZERO
    /// grid-degenerate triangles. TODAY it fails loudly at the G0 render gate
    /// (the ear-clip/CDT boundary-chord sliver), so `expect` panics — RED via
    /// the gate error.
    #[test]
    fn red_m1_r0040_patch_ring_tessellates_clean() {
        let (arena, fid) = build_r0040_patch();
        let mut mesh = RenderMesh::default();
        tessellate_cylinder_patch(&arena, fid, 71, &mut mesh).expect(
            "M1: the banked R0040 patch ring must tessellate cleanly (RED today: \
             the G0 render-degeneracy gate rejects the boundary-chord sliver)",
        );
        assert_eq!(
            scan_degeneracy(&mesh),
            (0, 0),
            "zero bitwise f32-degenerate triangles (B2 + B3) on the healthy R0040 ring"
        );
        assert_eq!(
            grid_degenerate(&mesh),
            0,
            "M1: zero grid-degenerate triangles on the healthy R0040 ring"
        );
    }

    /// RED (M3, pinch-splitting): a weakly-simple planar ring visiting the
    /// geometric point (0, -2) twice — through two DISTINCT arena vertices at
    /// bitwise-identical positions (a tangent pinch) — must tessellate into an
    /// exact partition of the two CCW sub-rings. TODAY the coincident pool
    /// vertices make spade return `DuplicateVertex`, mapped to a loud
    /// `TessellationFailed`, so `expect` panics — RED.
    ///
    /// Geometry: a big CCW square (−2,−2)..(2,2) whose bottom edge is pinched
    /// at (0,−2) by a diamond lobe protruding INTO the square. Both sub-rings
    /// are CCW by hand-shoelace: square pentagon area 16 (the pinch point sits
    /// collinear on the bottom edge), diamond area 0.7 → partition area 16.7.
    #[test]
    fn red_m3_pinch_ring_tessellates() {
        // Loop order (weakly simple; edges share only the pinch point):
        //   square: (2,-2)(2,2)(-2,2)(-2,-2)  → P1(0,-2)
        //   diamond CCW: (0.5,-1.2)(0,-0.6)(-0.5,-1.2) → P2(0,-2)  → close.
        // P1 (idx 4) and P2 (idx 8) are two vertex ids at identical coords.
        let z = 0.0;
        let pts = [
            Point3::new(2.0, -2.0, z),  // 0
            Point3::new(2.0, 2.0, z),   // 1
            Point3::new(-2.0, 2.0, z),  // 2
            Point3::new(-2.0, -2.0, z), // 3
            Point3::new(0.0, -2.0, z),  // 4  P1 (pinch)
            Point3::new(0.5, -1.2, z),  // 5
            Point3::new(0.0, -0.6, z),  // 6
            Point3::new(-0.5, -1.2, z), // 7
            Point3::new(0.0, -2.0, z),  // 8  P2 (pinch twin of P1)
        ];
        let (arena, fid) = build_planar_loop(&pts);
        let mut mesh = RenderMesh::default();
        tessellate_planar_face(&arena, fid, 32, &mut mesh).expect(
            "M3: the pinch ring must tessellate (RED today: coincident pinch \
             vertices → CDT DuplicateVertex → TessellationFailed)",
        );

        // Exact partition: triangle areas (f64, xy plane, normal +z) sum to the
        // analytic partition area = square 16 + diamond 0.7 = 16.7.
        const ANALYTIC_AREA: f64 = 16.7;
        let pos = |vid: u32| -> [f64; 2] {
            let i = vid as usize * 3;
            [mesh.positions[i], mesh.positions[i + 1]]
        };
        let signed = |t: &[u32]| -> f64 {
            let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
            0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
        };
        let mut area_sum = 0.0;
        for t in mesh.indices.chunks_exact(3) {
            area_sum += signed(t).abs();
        }
        assert!(
            (area_sum - ANALYTIC_AREA).abs() < 1e-9,
            "M3: exact partition area {area_sum} != analytic {ANALYTIC_AREA}"
        );

        // Non-inverted: every triangle shares one winding sign in the projected
        // frame (I4 — winding follows the CCW ring).
        let all_pos = mesh.indices.chunks_exact(3).all(|t| signed(t) > 0.0);
        let all_neg = mesh.indices.chunks_exact(3).all(|t| signed(t) < 0.0);
        assert!(
            all_pos || all_neg,
            "M3: all triangles must share one winding sign (no inverted triangle)"
        );

        // Watertight local pairing (the two sub-rings share the pinch VERTEX
        // but no edge — every undirected index-edge count 1 or 2).
        assert!(
            max_edge_incidence(&mesh) <= 2,
            "M3: watertight per-face partition — no edge shared by >2 triangles"
        );
    }

    /// GUARD (M3 boundary): a planar loop with two CONSECUTIVE coincident
    /// vertices (a zero-length edge) must FAIL loudly. This passes TODAY (the
    /// CDT rejects the coincident pair) and must keep passing after M3 — pinch
    /// splitting handles NON-consecutive duplicates only; a zero-length edge
    /// stays loud. Labeled a guard (not RED).
    #[test]
    fn guard_m3_consecutive_duplicate_stays_loud() {
        let z = 0.0;
        let pts = [
            Point3::new(0.0, 0.0, z), // 0
            Point3::new(2.0, 0.0, z), // 1
            Point3::new(2.0, 0.0, z), // 2  consecutive twin of 1 (zero-length edge)
            Point3::new(2.0, 2.0, z), // 3
            Point3::new(0.0, 2.0, z), // 4
        ];
        let (arena, fid) = build_planar_loop(&pts);
        let mut mesh = RenderMesh::default();
        match tessellate_planar_face(&arena, fid, 32, &mut mesh) {
            Err(KernelV2Error::TessellationFailed { face, .. }) => {
                assert_eq!(face, fid, "the guard must fail THIS planar face");
            }
            other => panic!(
                "a consecutive-duplicate (zero-length-edge) ring must fail loudly \
                 with TessellationFailed, got {other:?}"
            ),
        }
    }

    // ── ROUND 3 (M3 amendment, spec §6b M3a/M3b/M3c) ───────────────────────

    /// Even-odd point-in-polygon in f64 (orientation-independent). Used to
    /// assert hole exclusion — no emitted triangle centroid lands inside the
    /// keyhole's diamond lobe.
    fn point_in_poly_xy(px: f64, py: f64, poly: &[[f64; 2]]) -> bool {
        let n = poly.len();
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = (poly[i][0], poly[i][1]);
            let (xj, yj) = (poly[j][0], poly[j][1]);
            if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// RED (M3b, keyhole): a weakly-simple planar ring whose pinch split yields
    /// ONE CCW sub-ring (the square) and ONE CW sub-ring (a tangent diamond
    /// lobe → a HOLE touching the outer boundary at the pinch). TODAY round-2's
    /// both-CCW rule rejects it loudly with `"pinch sub-ring is not CCW"`, so
    /// `expect` panics — RED. TARGET (spec §6b M3b): outer = the CCW sub-ring,
    /// hole = the CW sub-ring, triangulated via the flood-fill welding variant;
    /// area = square − diamond.
    ///
    /// Hand-shoelace: square pentagon 16 (the pinch sits collinear on the
    /// bottom edge), diamond lobe wound CW area 0.7 → full-loop shoelace
    /// 16 − 0.7 = 15.3.
    #[test]
    fn red_m3b_keyhole_ring_tessellates() {
        // Loop: square (2,-2)(2,2)(-2,2)(-2,-2) CCW → P1(0,-2), then the diamond
        // detour wound CW: (-0.5,-1.2)(0,-0.6)(0.5,-1.2) → P2(0,-2) → close.
        // P1 (idx 4) and P2 (idx 8) are two vertex ids at identical coords.
        let z = 0.0;
        let pts = [
            Point3::new(2.0, -2.0, z),  // 0
            Point3::new(2.0, 2.0, z),   // 1
            Point3::new(-2.0, 2.0, z),  // 2
            Point3::new(-2.0, -2.0, z), // 3
            Point3::new(0.0, -2.0, z),  // 4  P1 (pinch)
            Point3::new(-0.5, -1.2, z), // 5  ┐ diamond, CW → a HOLE
            Point3::new(0.0, -0.6, z),  // 6  │
            Point3::new(0.5, -1.2, z),  // 7  ┘
            Point3::new(0.0, -2.0, z),  // 8  P2 (pinch twin of P1)
        ];
        let (arena, fid) = build_planar_loop(&pts);
        let mut mesh = RenderMesh::default();
        tessellate_planar_face(&arena, fid, 32, &mut mesh).expect(
            "M3b: the keyhole ring must tessellate (RED today: the CW diamond \
             sub-ring is rejected by the round-2 both-CCW rule)",
        );

        // Exact partition: triangle areas (f64, xy plane, normal +z) sum to
        // square 16 − diamond 0.7 = 15.3 (the diamond is a hole).
        const DIAMOND_AREA: f64 = 0.7;
        const KEYHOLE_AREA: f64 = 16.0 - DIAMOND_AREA;
        let pos = |vid: u32| -> [f64; 2] {
            let i = vid as usize * 3;
            [mesh.positions[i], mesh.positions[i + 1]]
        };
        let signed = |t: &[u32]| -> f64 {
            let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
            0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
        };
        let mut area_sum = 0.0;
        for t in mesh.indices.chunks_exact(3) {
            area_sum += signed(t).abs();
        }
        assert!(
            (area_sum - KEYHOLE_AREA).abs() < 1e-9,
            "M3b: keyhole area {area_sum} != square − diamond {KEYHOLE_AREA}"
        );

        // Hole exclusion: no emitted triangle centroid lies inside the diamond.
        let diamond = [[0.0, -2.0], [-0.5, -1.2], [0.0, -0.6], [0.5, -1.2]];
        for t in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (pos(t[0]), pos(t[1]), pos(t[2]));
            let cx = (a[0] + b[0] + c[0]) / 3.0;
            let cy = (a[1] + b[1] + c[1]) / 3.0;
            assert!(
                !point_in_poly_xy(cx, cy, &diamond),
                "M3b: a triangle centroid ({cx}, {cy}) lies inside the excluded diamond hole"
            );
        }

        // Non-inverted + watertight (the hole boundary and outer share only the
        // pinch VERTEX, no edge — every undirected index-edge count 1 or 2).
        let all_pos = mesh.indices.chunks_exact(3).all(|t| signed(t) > 0.0);
        let all_neg = mesh.indices.chunks_exact(3).all(|t| signed(t) < 0.0);
        assert!(
            all_pos || all_neg,
            "M3b: all triangles must share one winding sign (no inverted triangle)"
        );
        assert!(
            max_edge_incidence(&mesh) <= 2,
            "M3b: watertight per-face partition — no edge shared by >2 triangles"
        );
    }

    /// GUARD (M3c): a pinched ring whose BOTH sub-rings are CW (the round-2
    /// M3a fixture with the whole loop reversed) is invalid winding and must
    /// FAIL loudly. Passes TODAY (round-2 both-CCW rule) and must keep passing
    /// after M3b — a mutation tripwire so the keyhole path (exactly-one-CCW)
    /// never admits a fully-inverted (CW + CW) ring. Labeled a guard (not RED).
    #[test]
    fn guard_m3c_double_cw_stays_loud() {
        // The M3a both-CCW fixture reversed end-to-end: pinch at (0,-2) again,
        // but now the diamond sub-ring AND the square sub-ring are both CW.
        let z = 0.0;
        let pts = [
            Point3::new(0.0, -2.0, z),  // 0  P1 (pinch)
            Point3::new(-0.5, -1.2, z), // 1  ┐ diamond (CW)
            Point3::new(0.0, -0.6, z),  // 2  │
            Point3::new(0.5, -1.2, z),  // 3  ┘
            Point3::new(0.0, -2.0, z),  // 4  P2 (pinch)  → square (CW) follows
            Point3::new(-2.0, -2.0, z), // 5
            Point3::new(-2.0, 2.0, z),  // 6
            Point3::new(2.0, 2.0, z),   // 7
            Point3::new(2.0, -2.0, z),  // 8
        ];
        let (arena, fid) = build_planar_loop(&pts);
        let mut mesh = RenderMesh::default();
        match tessellate_planar_face(&arena, fid, 32, &mut mesh) {
            Err(KernelV2Error::TessellationFailed { face, .. }) => {
                assert_eq!(face, fid, "the guard must fail THIS planar face");
            }
            other => panic!(
                "a double-CW (invalid-winding) pinch ring must fail loudly with \
                 TessellationFailed, got {other:?}"
            ),
        }
    }
}
