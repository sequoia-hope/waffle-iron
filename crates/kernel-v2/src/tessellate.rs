//! Render tessellation (PR-KV3, Phase 4a): solid → triangle mesh.
//!
//! ## Single canonical path (crate hard rule 5)
//!
//! ONE implementation per surface type — Phase 4a has one surface type
//! (planar), so there is exactly one tessellation routine: exact-rational
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

use crate::arena::{BrepArena, FaceId, SolidId, Surface, UnitVector3};
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

/// Tessellate every face of `solid` into a [`RenderMesh`].
///
/// Deterministic: faces in shell walk order, loop points in walk order,
/// exact-arithmetic ear selection with fixed scan order. Errors are loud:
/// a face that cannot be tessellated returns
/// [`KernelV2Error::TessellationFailed`] (never a silent skip, never an
/// f64 guess).
pub fn tessellate(arena: &BrepArena, solid: SolidId) -> Result<RenderMesh, KernelV2Error> {
    let mut mesh = RenderMesh::default();
    let solid_ref = arena.solid(solid)?;
    for &sh in &solid_ref.shells {
        for &f in &arena.shell(sh)?.faces {
            tessellate_planar_face(arena, f, &mut mesh)?;
        }
    }
    Ok(mesh)
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
