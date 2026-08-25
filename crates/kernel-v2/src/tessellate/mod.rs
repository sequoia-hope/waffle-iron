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
    arc_interior_samples, arc_interior_samples_frac, ellipse_interior_samples,
    hyperbola_interior_samples, surface_pair_edge_samples,
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
    // Dev-only ring provenance (env-gated, print-only): maps each ring index
    // range back to the half-edge that emitted it, so a self-intersecting
    // output ring can be traced to the edge whose samples fold.
    let prov = std::env::var_os("KV2_RING_PROVENANCE").is_some();
    for h in arena.loop_half_edges(lid)? {
        let he = arena.half_edge(h)?;
        let origin = arena.vertex(he.origin)?.point;
        let prov_idx = pts.len();
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
        if prov {
            let fid = arena.loop_(he.loop_id)?.face;
            let kind = match he.curve {
                Curve::LineSegment => "LineSegment",
                Curve::Arc { .. } => "Arc",
                Curve::Circle { .. } => "Circle",
                Curve::EllipseArc { .. } => "EllipseArc",
                Curve::HyperbolaArc { .. } => "HyperbolaArc",
                Curve::SurfacePair { .. } => "SurfacePair",
            };
            let o = origin.as_array();
            eprintln!(
                "KV2_RING_PROV face={fid:?} loop={lid:?} idx={prov_idx} he={h:?} twin={:?} \
                 canon={} kind={kind} n_interior={} origin=[{:.12},{:.12},{:.12}]",
                he.twin,
                h <= he.twin,
                pts.len() - prov_idx - 1,
                o[0],
                o[1],
                o[2],
            );
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

mod surfaces;
pub(crate) use surfaces::*;
/// Boundary-edge geometry kind in the unrolled patch triangulation: what a
/// refinement split of the edge must follow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
            let pts3: Vec<[f64; 3]> = outer.iter().map(|&i| p3[i as usize]).collect();
            eprintln!("KV2_RING_REJECT_PROBE outer_pts3={pts3:?}");
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

#[cfg(test)]
mod pinched_ring_patch_tests;

#[cfg(test)]
mod arc_grid_sampling_tests;
