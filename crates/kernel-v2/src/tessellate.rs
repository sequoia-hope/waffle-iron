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
                    tessellate_cylinder_lateral(arena, f, n_seg, &mut mesh)?
                }
                Some(Surface::Plane(_)) => {
                    if face_has_circle_edge(arena, f)? {
                        tessellate_circular_cap(arena, f, n_seg, &mut mesh)?
                    } else {
                        tessellate_planar_face(arena, f, &mut mesh)?
                    }
                }
                None => return Err(KernelV2Error::FaceWithoutSurface { face: f }),
            }
        }
    }
    Ok(mesh)
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
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "circle-bounded planar face with rings is outside the KV5a vocabulary",
        });
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
    let (bot, top) = if toward(&rim_a, &rim_b) > 0.0 {
        (rim_a, rim_b)
    } else if toward(&rim_b, &rim_a) > 0.0 {
        (rim_b, rim_a)
    } else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "cylinder rims have no outward-oriented bottom (corrupt orientation)",
        });
    };
    let (cb, nub, radius, anchor) = bot;
    let ct = top.0;
    let Some((e1, e2)) = circle_frame(cb, nub, anchor) else {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "degenerate circle frame (anchor does not span a radial direction)",
        });
    };

    let range_start = out.indices.len() as u32;
    let base = out.num_vertices() as u32;
    let n = n_seg;
    // Bottom row [base .. base+n), top row [base+n .. base+2n), shared
    // angle table (columns aligned along rulings).
    for row_center in [cb, ct] {
        for k in 0..n {
            let theta = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64);
            let (s, c) = theta.sin_cos();
            let radial = [
                c * e1[0] + s * e2[0],
                c * e1[1] + s * e2[1],
                c * e1[2] + s * e2[2],
            ];
            out.positions.extend_from_slice(&[
                row_center.x() + radius * radial[0],
                row_center.y() + radius * radial[1],
                row_center.z() + radius * radial[2],
            ]);
            out.normals.extend_from_slice(&radial);
        }
    }
    for k in 0..n {
        let k1 = (k + 1) % n;
        let (bk, bk1, tk, tk1) = (base + k, base + k1, base + n + k, base + n + k1);
        // CCW-around-axis bottom row + axis toward the top row ⇒ these wind
        // with outward normals (∝ tangent × axis = radial).
        out.indices.extend_from_slice(&[bk, bk1, tk1]);
        out.indices.extend_from_slice(&[bk, tk1, tk]);
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
fn tessellate_planar_face(
    arena: &BrepArena,
    fid: FaceId,
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

    let outer_pts = arena.loop_points(face.outer_loop)?;
    if outer_pts.len() < 3 {
        return Err(KernelV2Error::TessellationFailed {
            face: fid,
            reason: "outer loop has fewer than 3 vertices",
        });
    }
    let mut poly: Vec<Node> = emit_loop(&outer_pts, out);
    let mut holes: Vec<Vec<Node>> = Vec::with_capacity(face.inner_loops.len());
    for &rid in &face.inner_loops {
        let pts = arena.loop_points(rid)?;
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
    let mut ring = poly;
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
                        out.indices.extend_from_slice(&[a.vid, b.vid, c.vid]);
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
                out.indices
                    .extend_from_slice(&[ring[0].vid, ring[1].vid, ring[2].vid]);
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
