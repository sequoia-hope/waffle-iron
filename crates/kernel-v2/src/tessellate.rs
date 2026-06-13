//! Render tessellation (PR-KV3, Phase 4a): solid → triangle mesh.
//!
//! ## Single canonical path (crate hard rule 5)
//!
//! ONE implementation per surface type:
//!
//! - planar faces with polygonal loops — exact-rational ear clipping
//!   (this module's original KV3 routine, unchanged);
//! - planar disk caps bounded by one full-circle edge (PR-KV5a) —
//!   rim sampling at the chord-bound `N` + a convex fan;
//! - cylinder laterals (PR-KV5a) — `N` quad-pairs between the two rims
//!   with exact analytic radial normals at the corners.
//!
//! The planar routine is exact-rational
//! ear clipping of the face's outer loop with hole loops bridged in. No
//! `reverse_outer` masking, no `bulk_flip`, no force-aligning: the polygon
//! walk direction IS the source of truth, and the emitted triangle winding
//! follows it (triangle normals equal the face's Newell normal by
//! construction, never by post-hoc correction).
//!
//! ## Why ear clipping with exact predicates (documented decision)
//!
//! The reuse-first check required by the KV3 mandate: yang-rs's Stage-1
//! tessellation machinery is **not public** — `yang_rs::BRep::new`
//! tessellates eagerly but exposes neither a per-face triangulation API nor
//! the CDT it delegates to (`cherchi_rs::cdt_polygon_with_holes` is public
//! *on cherchi-rs*, which kernel-v2 must not depend on directly, and
//! yang-rs does not re-export it). Render tessellation is, per yang-rs's
//! own scope rules, "entirely out of scope [for yang-rs] — render
//! tessellation is in kernel-v2". So kernel-v2 implements its own planar
//! routine, following the KV2 pattern: **all orientation decisions are
//! exact** (`dashu` rationals via [`crate::exact2d`] — every finite `f64`
//! converts losslessly, so orient2d sign evaluations are decision
//! procedures, not approximations). Plain f64 ear clipping is exactly the
//! silent-wrong failure mode this rewrite exists to eliminate (a mis-signed
//! near-degenerate ear produces an overlapping or inverted triangulation
//! with no error).
//!
//! Boolean results make non-convexity and collinear chain vertices (split
//! edges) the NORMAL case, and holed faces (through-cuts) are first-class:
//! holes are bridged into the outer loop with exactly-validated bridge
//! segments (shortest visible bridge, deterministic tie-break), then the
//! merged (weakly simple) polygon is ear-clipped.
//!
//! ## Algorithm
//!
//! Per planar face:
//!
//! 1. Project the outer loop and rings onto the dominant-axis coordinate
//!    plane of the face normal, with an axis order chosen so orientation
//!    is preserved (outer CCW, rings CW — guaranteed by the validated
//!    Newell/ring-winding invariants).
//! 2. Bridge each hole into the merged polygon: holes processed
//!    rightmost-first; the bridge is the exactly-shortest segment from a
//!    hole vertex to a merged-polygon vertex that no boundary edge blocks
//!    ([`crate::exact2d::bridge_blocked_by`] — proper crossing, non-shared
//!    endpoint contact, and collinear overlap all block) and whose exact
//!    rational midpoint is strictly inside the merged polygon and strictly
//!    outside every hole. The hole is spliced in with doubled corridor
//!    vertices (standard weakly-simple-polygon bridging).
//! 3. Ear-clip the merged polygon: a vertex is an ear iff its corner is
//!    exactly convex (orient2d `Greater`) and no other polygon vertex lies
//!    inside or on the closed corner triangle (vertices at coordinates
//!    equal to the corner's own — bridge duplicates — excluded). Exactly
//!    collinear corners are dropped without emitting a zero-area triangle
//!    (area-preserving). If a full scan finds neither, the face fails
//!    LOUDLY ([`KernelV2Error::TessellationFailed`]) — never an infinite
//!    loop, never an f64 guess.
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
//! - Triangle area sums to the face area exactly in rational arithmetic
//!   (ear clipping is an exact partition of the polygon-with-holes); the
//!   f64 oracle tolerance only absorbs summation rounding.
//! - Every triangle winds with the face: its normal direction equals the
//!   face plane normal.
//! - Mesh signed volume equals the solid's B-Rep signed volume (same
//!   region, exact partition).

use std::cmp::Ordering;

use crate::arena::{BrepArena, Curve, FaceId, SolidId, Surface, UnitVector3};
use crate::error::KernelV2Error;
use crate::exact2d;
use cad_primitives::{Point2, Point3};

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

    // One shared frame (the outer circle's, CCW around the face normal ==
    // the outer circle's traversal normal); the ring row uses the same
    // angle table from ITS anchor's frame so its boundary samples match the
    // adjacent lateral's rim samples (both anchored at the same seam
    // vertex with the mirrored axis — agreement within trig rounding).
    let Some((e1_o, e2_o)) = circle_frame(c_o, nu_o, anchor_o) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate circle frame (anchor does not span a radial direction)",
        });
    };
    // Ring sampled CCW around the SAME face normal (its half-edge traverses
    // the other way, but the strip below wants aligned columns).
    let Some((e1_r, e2_r)) = circle_frame(c_r, nu_o, anchor_r) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate ring circle frame",
        });
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;
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
    for k in 0..n {
        let k1 = (k + 1) % n;
        let (ok, ok1, ik, ik1) = (base + k, base + k1, base + n + k, base + n + k1);
        // Outer row CCW around the face normal ⇒ this winding faces +normal.
        out.indices.extend_from_slice(&[ok, ok1, ik1]);
        out.indices.extend_from_slice(&[ok, ik1, ik]);
    }
    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
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
/// exact-arithmetic bridging and ear-clipping cores:
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
/// 3. **Bridge windows** (zero-wrap hole loops) with the planar routine's
///    hole-bridging, then **ear-clip** exactly.
/// 4. **Refine** to the chord bound: any triangulation edge spanning more
///    than one facet width in `u` is bisected (conforming — both incident
///    triangles split), interior split points landing exactly on the
///    analytic surface, boundary chord splits on the chord (collinear ⇒
///    closure-safe against neighbors). A triangle's `u`-span is then at
///    most two facet widths, so the radial sagitta is bounded by the
///    documented band at the doubled angle.
/// 5. **Emit** with exact analytic radial normals (negated for `reversed`).
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

    let (mut poly, mut holes): (Vec<Node>, Vec<Vec<Node>>);
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

    // ---- pass 3: bridge windows + ear-clip --------------------------------
    while !holes.is_empty() {
        let pick = holes
            .iter()
            .enumerate()
            .max_by(|(ia, ha), (ib, hb)| {
                let ax = ha
                    .iter()
                    .map(|nd| nd.p2.x())
                    .fold(f64::NEG_INFINITY, f64::max);
                let bx = hb
                    .iter()
                    .map(|nd| nd.p2.x())
                    .fold(f64::NEG_INFINITY, f64::max);
                ax.partial_cmp(&bx)
                    .unwrap_or(Ordering::Equal)
                    .then(ib.cmp(ia))
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        let hole = holes.remove(pick);
        bridge_hole(&mut poly, hole, &holes, fid)?;
        // Register every adjacency; the new ones (the bridge corridor) are
        // straight 3D segments → Chord kind. Boundary kinds set earlier
        // (arc samples) are preserved by `or_insert`.
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
    // Make sure every polygon adjacency is registered (covers the no-hole
    // case and the seam duplicates).
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

    // Ear-clip on PER-RING u coordinates: the ring carries its own Point2
    // (seam duplicates differ from the node table), so clip on a local node
    // list. Map ring entries to fresh local ids, clip, then translate back.
    let local: Vec<Node> = poly
        .iter()
        .enumerate()
        .map(|(i, n)| Node {
            p2: n.p2,
            vid: i as u32,
        })
        .collect();
    let ring_vids: Vec<u32> = poly.iter().map(|n| n.vid).collect();
    let ring_p2: Vec<Point2> = poly.iter().map(|n| n.p2).collect();
    let mut tris_local = ear_clip(local, fid)?;
    // Quality pass (see `delaunay_flip`): the cut barrel boundary is
    // densely sampled, and the slivers ear clipping leaves would seed
    // sliver cascades in the bisection refinement below.
    {
        let fixed: std::collections::BTreeSet<(u32, u32)> = (0..ring_vids.len())
            .map(|i| {
                let (a, b) = (i as u32, ((i + 1) % ring_vids.len()) as u32);
                (a.min(b), a.max(b))
            })
            .collect();
        delaunay_flip(&mut tris_local, |v| ring_p2[v as usize], &fixed);
    }

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
    let mut wnodes: Vec<WNode> = ring_vids
        .iter()
        .zip(ring_p2.iter())
        .map(|(&n, &p)| WNode {
            p2: p,
            node: n as usize,
        })
        .collect();
    let mut wtris: Vec<[usize; 3]> = tris_local
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

/// Deterministic Delaunay edge-flip pass over a 2D triangulation
/// (PR-KV5b). QUALITY heuristic only — every flip preserves the covered
/// region exactly, so correctness is untouched; what it removes are the
/// near-degenerate slivers greedy ear clipping produces on densely-sampled
/// (arc-bearing) boundaries, which would otherwise survive into the mesh
/// (and seed sliver cascades in the cylinder-patch refinement). Hence:
/// VALIDITY decisions (the new pair must be two CCW triangles) use the
/// exact `orient2d`; the in-circle test is plain f64 (a wrong quality call
/// flips nothing structural), and the flip budget bounds termination
/// against f64 co-circular jitter. `fixed` holds undirected vid pairs that
/// must never flip (boundary + bridge-corridor edges).
fn delaunay_flip<F: Fn(u32) -> Point2>(
    tris: &mut [[u32; 3]],
    p2_of: F,
    fixed: &std::collections::BTreeSet<(u32, u32)>,
) {
    use std::collections::BTreeMap;
    let incircle = |a: Point2, b: Point2, c: Point2, d: Point2| -> f64 {
        // > 0 ⇔ d strictly inside the circumcircle of CCW (a, b, c).
        let (ax, ay) = (a.x() - d.x(), a.y() - d.y());
        let (bx, by) = (b.x() - d.x(), b.y() - d.y());
        let (cx, cy) = (c.x() - d.x(), c.y() - d.y());
        let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
        ax * (by * c2 - b2 * cy) - ay * (bx * c2 - b2 * cx) + a2 * (bx * cy - by * cx)
    };
    let mut budget = 16 * tris.len() + 64;
    loop {
        // Rebuild the (undirected vid pair) → incident-triangle map each
        // sweep; flip the first improvable edge in deterministic order.
        let mut edge_map: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
        for (ti, t) in tris.iter().enumerate() {
            for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let key = (t[i].min(t[j]), t[i].max(t[j]));
                edge_map.entry(key).or_default().push(ti);
            }
        }
        let mut flipped = false;
        'scan: for (&key, owners) in &edge_map {
            if owners.len() != 2 || fixed.contains(&key) {
                continue;
            }
            let (t1, t2) = (owners[0], owners[1]);
            // Opposite vertices c (of t1) and d (of t2) across edge (a, b).
            let (a, b) = key;
            let opp = |t: [u32; 3]| t.into_iter().find(|&v| v != a && v != b);
            let (Some(c), Some(d)) = (opp(tris[t1]), opp(tris[t2])) else {
                continue;
            };
            if c == d {
                continue;
            }
            // Orient (a, b, c) CCW for the in-circle sign convention.
            let (pa, pb, pc, pd) = (p2_of(a), p2_of(b), p2_of(c), p2_of(d));
            let (a, b, pa, pb) = match exact2d::orient2d(pa, pb, pc) {
                Ordering::Greater => (a, b, pa, pb),
                Ordering::Less => (b, a, pb, pa),
                Ordering::Equal => continue,
            };
            if incircle(pa, pb, pc, pd) <= 0.0 {
                continue; // locally Delaunay (or co-circular) — keep
            }
            // Validity: the flipped pair must be two exactly-CCW triangles.
            if exact2d::orient2d(pa, pd, pc) != Ordering::Greater
                || exact2d::orient2d(pd, pb, pc) != Ordering::Greater
            {
                continue;
            }
            tris[t1] = [a, d, c];
            tris[t2] = [d, b, c];
            flipped = true;
            budget -= 1;
            if budget == 0 {
                return;
            }
            break 'scan;
        }
        if !flipped {
            return;
        }
    }
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
    if outer_pts.len() < 3 {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "outer loop has fewer than 3 vertices",
        });
    }
    let mut poly: Vec<Node> = emit_loop(&outer_pts, out);
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

    // ---- bridge holes in, rightmost hole first ----------------------------
    // (Deterministic: max-x f64 comparisons are exact; ties broken by the
    // holes' walk order.)
    while !holes.is_empty() {
        let pick = holes
            .iter()
            .enumerate()
            .max_by(|(ia, a), (ib, b)| {
                let ax = a
                    .iter()
                    .map(|nd| nd.p2.x())
                    .fold(f64::NEG_INFINITY, f64::max);
                let bx = b
                    .iter()
                    .map(|nd| nd.p2.x())
                    .fold(f64::NEG_INFINITY, f64::max);
                ax.partial_cmp(&bx)
                    .unwrap_or(Ordering::Equal)
                    .then(ib.cmp(ia)) // tie → lower index wins under max_by
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        let hole = holes.remove(pick);
        bridge_hole(&mut poly, hole, &holes, fid)?;
    }

    // ---- ear-clip the merged (weakly simple) polygon -----------------------
    // Arc-bearing faces (PR-KV5b) get the Delaunay flip pass: their densely
    // sampled boundaries make greedy ear clipping emit slivers. Pure-segment
    // faces skip it — the KV3 planar output stays byte-identical.
    let has_arcs = face_has_arc_edge(arena, fid)?;
    let fixed: std::collections::BTreeSet<(u32, u32)> = if has_arcs {
        let m = poly.len();
        (0..m)
            .map(|i| {
                let (a, b) = (poly[i].vid, poly[(i + 1) % m].vid);
                (a.min(b), a.max(b))
            })
            .collect()
    } else {
        Default::default()
    };
    let p2_by_vid: std::collections::BTreeMap<u32, Point2> = if has_arcs {
        poly.iter().map(|n| (n.vid, n.p2)).collect()
    } else {
        Default::default()
    };
    let mut tris = ear_clip(poly, fid)?;
    if has_arcs {
        delaunay_flip(&mut tris, |v| p2_by_vid[&v], &fixed);
    }
    for tri in tris {
        out.indices.extend_from_slice(&tri);
    }

    out.face_ranges.push(FaceRange {
        face: fid,
        start: range_start,
        count: out.indices.len() as u32 - range_start,
    });
    Ok(())
}

/// Does any loop of the face carry a `Curve::Arc` half-edge?
fn face_has_arc_edge(arena: &BrepArena, fid: FaceId) -> Result<bool, KernelV2Error> {
    let face = arena.face(fid)?;
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    for lid in loops {
        for h in arena.loop_half_edges(lid)? {
            if matches!(arena.half_edge(h)?.curve, Curve::Arc { .. }) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Ear-clip a merged (weakly simple) CCW polygon into `vid` triples — the
/// single canonical exact-arithmetic clipping core (module docs, step 3),
/// shared by the planar routine and the PR-KV5b cylinder-patch routine
/// (which runs it in unrolled `(θ·r, h)` coordinates with node-table ids
/// in `vid`).
fn ear_clip(mut ring: Vec<Node>, fid: FaceId) -> Result<Vec<[u32; 3]>, KernelV2Error> {
    let mut tris = Vec::new();
    'clip: while ring.len() > 3 {
        let m = ring.len();
        for i in 0..m {
            let a = ring[(i + m - 1) % m];
            let b = ring[i];
            let c = ring[(i + 1) % m];
            match exact2d::orient2d(a.p2, b.p2, c.p2) {
                // Exactly collinear corner (straight chain vertex or a
                // fully-collapsed bridge-corridor spike): drop it without
                // emitting a zero-area triangle. Area-preserving.
                Ordering::Equal => {
                    ring.remove(i);
                    continue 'clip;
                }
                Ordering::Less => continue, // reflex — not an ear
                Ordering::Greater => {
                    if is_ear(&ring, i) {
                        tris.push([a.vid, b.vid, c.vid]);
                        ring.remove(i);
                        continue 'clip;
                    }
                }
            }
        }
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "no clippable ear found",
        });
    }
    if ring.len() == 3 {
        match exact2d::orient2d(ring[0].p2, ring[1].p2, ring[2].p2) {
            Ordering::Greater => {
                tris.push([ring[0].vid, ring[1].vid, ring[2].vid]);
            }
            Ordering::Equal => {} // degenerate remainder: zero area, skip
            Ordering::Less => {
                return Err(KernelV2Error::TessellationFailed {
                    face: fid,
                    reason: "inverted final triangle",
                });
            }
        }
    }
    Ok(tris)
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

/// Bridge one hole into the merged polygon (module docs, step 2): the
/// exactly-shortest unblocked bridge whose midpoint lies in the face
/// material; splice with doubled corridor vertices.
fn bridge_hole(
    poly: &mut Vec<Node>,
    hole: Vec<Node>,
    other_holes: &[Vec<Node>],
    fid: FaceId,
) -> Result<(), KernelV2Error> {
    let mut best: Option<(dashu::rational::RBig, usize, usize)> = None; // (dist², poly j, hole i)
    for (j, pn) in poly.iter().enumerate() {
        for (i, hn) in hole.iter().enumerate() {
            let (p, h) = (pn.p2, hn.p2);
            if p == h {
                continue; // zero-length bridge is no bridge
            }
            let d2 = exact2d::squared_distance(p, h);
            if let Some((ref bd, _, _)) = best {
                if d2 >= *bd {
                    continue; // not strictly shorter — keep first (deterministic)
                }
            }
            if bridge_is_valid(p, h, poly, &hole, other_holes) {
                best = Some((d2, j, i));
            }
        }
    }
    let Some((_, j, i)) = best else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "no visible hole bridge",
        });
    };
    // Splice: … poly[j], hole[i], hole[i+1], …, hole[i-1], hole[i],
    // poly[j], poly[j+1] … (corridor edge doubled in both directions).
    let mut merged = Vec::with_capacity(poly.len() + hole.len() + 2);
    merged.extend_from_slice(&poly[..=j]);
    merged.extend(hole[i..].iter().copied());
    merged.extend(hole[..=i].iter().copied());
    merged.push(poly[j]);
    merged.extend_from_slice(&poly[j + 1..]);
    *poly = merged;
    Ok(())
}

/// Is the candidate bridge `p → h` valid? No boundary edge (merged
/// polygon, the candidate hole itself, or any remaining hole) blocks it,
/// and its exact midpoint lies strictly inside the merged polygon and
/// strictly outside every hole (belt-and-suspenders: with no boundary
/// contact the open segment cannot change region, so the midpoint decides
/// for the whole segment).
fn bridge_is_valid(
    p: Point2,
    h: Point2,
    poly: &[Node],
    hole: &[Node],
    other_holes: &[Vec<Node>],
) -> bool {
    let blocked = |loop_nodes: &[Node]| -> bool {
        let m = loop_nodes.len();
        (0..m)
            .any(|k| exact2d::bridge_blocked_by(p, h, loop_nodes[k].p2, loop_nodes[(k + 1) % m].p2))
    };
    if blocked(poly) || blocked(hole) || other_holes.iter().any(|oh| blocked(oh)) {
        return false;
    }
    let mid = exact2d::midpoint(p, h);
    let pts2 = |nodes: &[Node]| -> Vec<Point2> { nodes.iter().map(|nd| nd.p2).collect() };
    if !exact2d::point_strictly_inside_rq(&mid, &pts2(poly)) {
        return false;
    }
    if exact2d::point_strictly_inside_rq(&mid, &pts2(hole)) {
        return false;
    }
    other_holes
        .iter()
        .all(|oh| !exact2d::point_strictly_inside_rq(&mid, &pts2(oh)))
}

/// Ear test at position `i` of the ring (corner already known exactly
/// convex): no OTHER ring vertex lies inside or on the closed corner
/// triangle. Vertices at coordinates equal to a corner vertex (bridge
/// corridor duplicates) do not block — they coincide with the triangle's
/// own corners.
fn is_ear(ring: &[Node], i: usize) -> bool {
    let m = ring.len();
    let a = ring[(i + m - 1) % m].p2;
    let b = ring[i].p2;
    let c = ring[(i + 1) % m].p2;
    for (k, q) in ring.iter().enumerate() {
        if k == (i + m - 1) % m || k == i || k == (i + 1) % m {
            continue;
        }
        let q = q.p2;
        if q == a || q == b || q == c {
            continue;
        }
        if exact2d::inside_or_on_triangle(a, b, c, q) {
            return false;
        }
    }
    true
}
