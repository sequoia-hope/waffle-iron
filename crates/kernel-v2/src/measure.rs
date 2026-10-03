//! Q1 of `specs/agent_mechanical_design.md` §4.2: **minimum distance and the
//! closest points between two pieces of geometry**, in two tiers, both
//! reported.
//!
//! ## Seed
//!
//! Each operand becomes a set of exact primitives — a face's render triangles
//! ([`crate::tessellate::tessellate_face`], the same triangles the app draws),
//! an edge's chord samples, a vertex or a bare point — and the closest pair is
//! found by branch and bound over an AABB tree per operand (descend the wider
//! box, prune a pair whose box-to-box bound already exceeds the best pair
//! found). The pair kernels are exact and complete, intersection included: a
//! triangle pair's distance is the minimum over each triangle's three edges
//! against the other triangle, and a segment that pierces a triangle reports
//! zero at the piercing point — so an overlapping pair reports 0 rather than
//! the positive edge-to-edge distance an edge/vertex-only decomposition would
//! invent.
//!
//! ## Which tier the answer is
//!
//! [`DistanceResult::exact`] is true only when the number cannot be improved:
//!
//! - **The seed is already exact** when both winning primitives represent
//!   their geometry exactly — a planar face's triangle (a CDT of the face is
//!   an exact partition of it), a straight edge's segment, a vertex, a point.
//!   This covers every plane/plane, plane/edge and point/plane pair, which is
//!   why two boxes measure exactly.
//! - **Otherwise the pair is refined analytically**: alternate the exact
//!   closest-point projection ([`crate::signature::closest_point_on`], a
//!   one-step exact form over each surface's implicit signed distance) between
//!   the two carriers until both feet stop moving, then CERTIFY the result.
//!   A carrier is the analytic surface under a face, the LINE under a straight
//!   edge (its foot has a degree of freedom and must be allowed to slide: the
//!   seed foot came from the other operand's chord facets, so freezing it
//!   overclaims), or a fixed point for a vertex. The certificate: each foot
//!   must still lie on its own trimmed face (within the chord band of that
//!   face's own triangles, the only statement the tessellation licenses about
//!   the trim boundary), the segment joining them must be normal to every
//!   carrier surface, and perpendicular to a carrier line unless the foot ran
//!   into one of its endpoints. Certified ⇒ exact.
//! - **Anything else is the mesh tier**, with the chord bound
//!   `RENDER_CHORD_TOLERANCE_REL × extent` the caller can hold us to: a curved
//!   face whose refinement does not converge or converges off the face, and a
//!   curved edge's chord samples.
//!
//! A refusal to certify is never a silent downgrade: the answer carries the
//! tier, and the mesh tier's value is an inscribed distance — it is never
//! SHORTER than the truth for two convex-outward curved faces, but the bound
//! is what the consumer must carry (the chord-band propagation lesson).
//!
//! ## Not in Q1
//!
//! An infinite axis as an operand, and a mesh-backed imported body: both
//! refuse, typed, at the adapter. Interference and overlap volume are Q2.

use crate::arena::{BrepArena, Curve, FaceId, HalfEdgeId, SolidId, Surface, VertexId};
use crate::error::KernelV2Error;
use crate::signature::{closest_point_on, outward_normal_at};
use crate::tessellate::RENDER_CHORD_TOLERANCE_REL;
use cad_primitives::{Point3, TAU_EVAL, TAU_WORK};

/// What a distance is measured from or to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    /// Every face of a solid.
    Solid(SolidId),
    /// One trimmed face.
    Face(FaceId),
    /// One edge, named by a half-edge (either twin: the geometry is the same).
    Edge(HalfEdgeId),
    /// One vertex.
    Vertex(VertexId),
    /// A free point in space, part of no body.
    Point(Point3),
}

/// The entity a closest point lies on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum On {
    Face(FaceId),
    Edge(HalfEdgeId),
    Vertex(VertexId),
}

/// The answer: the distance, where it is realized on each operand, what each
/// of those points lies on, and whether the number is exact.
#[derive(Debug, Clone)]
pub struct DistanceResult {
    /// The minimum distance in meters (0 when the operands touch or overlap).
    pub value: f64,
    /// The closest point on the first operand, and on the second.
    pub points: [Point3; 2],
    /// The face / edge / vertex each of those points lies on. `None` for a
    /// free point operand.
    pub on: [Option<On>; 2],
    /// Whether `value` is exact (see the module docs for what certifies it).
    pub exact: bool,
    /// The chord band of the seed tessellation, in meters — the bound on
    /// `value` when `exact` is false. Reported either way so a consumer can
    /// see the band it escaped.
    pub chord_bound: f64,
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Prim {
    /// A render triangle of a face. `exact` when the face is planar, in which
    /// case the triangle IS the geometry (an exact partition of the trim).
    Tri {
        p: [Point3; 3],
        face: FaceId,
        exact: bool,
    },
    /// A chord of an edge. `exact` for a straight edge.
    Seg {
        p: [Point3; 2],
        edge: HalfEdgeId,
        exact: bool,
    },
    /// A vertex, or a free point (`on: None`).
    Pt { p: Point3, on: Option<On> },
}

impl Prim {
    fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let pts: &[Point3] = match self {
            Prim::Tri { p, .. } => p,
            Prim::Seg { p, .. } => p,
            Prim::Pt { p, .. } => std::slice::from_ref(p),
        };
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for q in pts {
            let q = q.as_array();
            for k in 0..3 {
                lo[k] = lo[k].min(q[k]);
                hi[k] = hi[k].max(q[k]);
            }
        }
        (lo, hi)
    }

    fn on(&self) -> Option<On> {
        match *self {
            Prim::Tri { face, .. } => Some(On::Face(face)),
            Prim::Seg { edge, .. } => Some(On::Edge(edge)),
            Prim::Pt { on, .. } => on,
        }
    }

    /// Whether this primitive represents its geometry exactly.
    fn is_exact(&self) -> bool {
        match *self {
            Prim::Tri { exact, .. } | Prim::Seg { exact, .. } => exact,
            Prim::Pt { .. } => true,
        }
    }
}

/// Every primitive of `target`, with the face / edge / vertex each came from.
fn primitives(arena: &BrepArena, target: Target) -> Result<Vec<Prim>, KernelV2Error> {
    let mut out = Vec::new();
    match target {
        Target::Solid(sid) => {
            let solid = arena.solid(sid)?;
            for &sh in &solid.shells {
                for &f in &arena.shell(sh)?.faces {
                    face_prims(arena, f, &mut out)?;
                }
            }
        }
        Target::Face(f) => face_prims(arena, f, &mut out)?,
        Target::Edge(h) => {
            let exact = matches!(arena.half_edge(h)?.curve, Curve::LineSegment);
            let pts = edge_polyline(arena, h)?;
            for w in pts.windows(2) {
                out.push(Prim::Seg {
                    p: [w[0], w[1]],
                    edge: h,
                    exact,
                });
            }
        }
        Target::Vertex(v) => out.push(Prim::Pt {
            p: arena.vertex(v)?.point,
            on: Some(On::Vertex(v)),
        }),
        Target::Point(p) => out.push(Prim::Pt { p, on: None }),
    }
    if out.is_empty() {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "measure: the target has no geometry to measure",
        });
    }
    Ok(out)
}

/// Whether `f`'s triangles ARE the face: it is planar AND every boundary edge
/// is a straight line, so the CDT is an exact partition of it.
///
/// A planar face is not enough. A circular cap's rim is a chord polygon
/// INSIDE the true disk, so its triangles under-report the disk by the
/// sagitta — measured as 5.00294 for a pair of coplanar disks whose true gap
/// is 5, while the test claimed `exact`. The curved boundary is the whole
/// difference, and nothing but the edge curves can tell.
fn face_is_exactly_triangulated(arena: &BrepArena, f: FaceId) -> bool {
    let Ok(face) = arena.face(f) else {
        return false;
    };
    if !matches!(face.surface, Some(Surface::Plane(_))) {
        return false;
    }
    let mut loops = vec![face.outer_loop];
    loops.extend(face.inner_loops.iter().copied());
    loops.into_iter().all(|lid| {
        arena.loop_half_edges(lid).is_ok_and(|hes| {
            hes.iter().all(|&h| {
                arena
                    .half_edge(h)
                    .is_ok_and(|he| matches!(he.curve, Curve::LineSegment))
            })
        })
    })
}

fn face_prims(arena: &BrepArena, f: FaceId, out: &mut Vec<Prim>) -> Result<(), KernelV2Error> {
    let exact = face_is_exactly_triangulated(arena, f);
    let mesh = crate::tessellate::tessellate_face(arena, f)?;
    let point = |i: u32| -> Point3 {
        let i = i as usize * 3;
        Point3::new(
            mesh.positions[i],
            mesh.positions[i + 1],
            mesh.positions[i + 2],
        )
    };
    for t in mesh.indices.chunks_exact(3) {
        out.push(Prim::Tri {
            p: [point(t[0]), point(t[1]), point(t[2])],
            face: f,
            exact,
        });
    }
    Ok(())
}

/// An edge's polyline from its start vertex to its end vertex, at the render
/// chord band (two points for a straight edge; a closed circle edge comes
/// back as a closed polyline).
fn edge_polyline(arena: &BrepArena, h: HalfEdgeId) -> Result<Vec<Point3>, KernelV2Error> {
    let he = arena.half_edge(h)?;
    let start = arena.vertex(he.origin)?.point;
    let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
    let n_seg = crate::tessellate::circle_segment_count(RENDER_CHORD_TOLERANCE_REL);
    let mut pts = vec![start];
    pts.extend(crate::tessellate::boundary_half_edge_samples(
        arena, h, n_seg,
    )?);
    pts.push(end);
    Ok(pts)
}

// ---------------------------------------------------------------------------
// Pair kernels — all squared distances, all return the realizing points
// ---------------------------------------------------------------------------

type Hit = (f64, Point3, Point3);

fn sub(a: Point3, b: Point3) -> [f64; 3] {
    [a.x() - b.x(), a.y() - b.y(), a.z() - b.z()]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len2(a: [f64; 3]) -> f64 {
    dot(a, a)
}
fn add_scaled(p: Point3, v: [f64; 3], t: f64) -> Point3 {
    Point3::new(p.x() + v[0] * t, p.y() + v[1] * t, p.z() + v[2] * t)
}
fn dist2(a: Point3, b: Point3) -> f64 {
    len2(sub(a, b))
}

fn pt_pt(a: Point3, b: Point3) -> Hit {
    (dist2(a, b), a, b)
}

/// Closest point of segment `q0q1` to `p`.
fn pt_seg(p: Point3, q0: Point3, q1: Point3) -> Hit {
    let d = sub(q1, q0);
    let dd = len2(d);
    let t = if dd > 0.0 {
        (dot(sub(p, q0), d) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let foot = add_scaled(q0, d, t);
    (dist2(p, foot), p, foot)
}

/// Closest point of triangle `t` to `p` (barycentric regions, Ericson §5.1.5).
fn pt_tri(p: Point3, t: &[Point3; 3]) -> Hit {
    let (a, b, c) = (t[0], t[1], t[2]);
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return (dist2(p, a), p, a);
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return (dist2(p, b), p, b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = if d1 - d3 != 0.0 { d1 / (d1 - d3) } else { 0.0 };
        let q = add_scaled(a, ab, v);
        return (dist2(p, q), p, q);
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return (dist2(p, c), p, c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = if d2 - d6 != 0.0 { d2 / (d2 - d6) } else { 0.0 };
        let q = add_scaled(a, ac, w);
        return (dist2(p, q), p, q);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let den = (d4 - d3) + (d5 - d6);
        let w = if den != 0.0 { (d4 - d3) / den } else { 0.0 };
        let q = add_scaled(b, sub(c, b), w);
        return (dist2(p, q), p, q);
    }
    let den = va + vb + vc;
    if den == 0.0 {
        // Degenerate triangle: fall back to its edges.
        return [pt_seg(p, a, b), pt_seg(p, b, c), pt_seg(p, c, a)]
            .into_iter()
            .fold(
                (f64::INFINITY, p, a),
                |best, h| if h.0 < best.0 { h } else { best },
            );
    }
    let denom = 1.0 / den;
    let v = vb * denom;
    let w = vc * denom;
    let q = Point3::new(
        a.x() + ab[0] * v + ac[0] * w,
        a.y() + ab[1] * v + ac[1] * w,
        a.z() + ab[2] * v + ac[2] * w,
    );
    (dist2(p, q), p, q)
}

/// Closest points of two segments (Ericson §5.1.9, clamped).
fn seg_seg(p0: Point3, p1: Point3, q0: Point3, q1: Point3) -> Hit {
    let d1 = sub(p1, p0);
    let d2 = sub(q1, q0);
    let r = sub(p0, q0);
    let a = len2(d1);
    let e = len2(d2);
    let f = dot(d2, r);
    let (mut s, mut t);
    if a <= 0.0 && e <= 0.0 {
        return pt_pt(p0, q0);
    }
    if a <= 0.0 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= 0.0 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            s = if denom != 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    let cp = add_scaled(p0, d1, s);
    let cq = add_scaled(q0, d2, t);
    (dist2(cp, cq), cp, cq)
}

/// Whether `p`, assumed on the triangle's plane, lies inside it.
fn in_tri(p: Point3, t: &[Point3; 3], n: [f64; 3]) -> bool {
    let e = |u: Point3, v: Point3| dot(cross(sub(v, u), sub(p, u)), n) >= 0.0;
    e(t[0], t[1]) && e(t[1], t[2]) && e(t[2], t[0])
}

/// Segment-to-triangle, zero (at the piercing point) when the segment crosses
/// the triangle.
fn seg_tri(s0: Point3, s1: Point3, t: &[Point3; 3]) -> Hit {
    let n = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    if len2(n) > 0.0 {
        let d0 = dot(n, sub(s0, t[0]));
        let d1 = dot(n, sub(s1, t[0]));
        if (d0 <= 0.0 && d1 >= 0.0) || (d0 >= 0.0 && d1 <= 0.0) {
            let den = d0 - d1;
            let u = if den != 0.0 { d0 / den } else { 0.0 };
            let x = add_scaled(s0, sub(s1, s0), u);
            if in_tri(x, t, n) {
                return (0.0, x, x);
            }
        }
    }
    let mut best = pt_tri(s0, t);
    for h in [
        pt_tri(s1, t),
        swap(seg_seg(t[0], t[1], s0, s1)),
        swap(seg_seg(t[1], t[2], s0, s1)),
        swap(seg_seg(t[2], t[0], s0, s1)),
    ] {
        if h.0 < best.0 {
            best = h;
        }
    }
    best
}

fn swap(h: Hit) -> Hit {
    (h.0, h.2, h.1)
}

fn tri_tri(a: &[Point3; 3], b: &[Point3; 3]) -> Hit {
    let mut best = (f64::INFINITY, a[0], b[0]);
    for i in 0..3 {
        let h = seg_tri(a[i], a[(i + 1) % 3], b);
        if h.0 < best.0 {
            best = h;
        }
        let h = swap(seg_tri(b[i], b[(i + 1) % 3], a));
        if h.0 < best.0 {
            best = h;
        }
    }
    best
}

/// Squared distance between two primitives, with the realizing points (the
/// first on `a`, the second on `b`).
fn prim_prim(a: &Prim, b: &Prim) -> Hit {
    match (a, b) {
        (Prim::Pt { p, .. }, Prim::Pt { p: q, .. }) => pt_pt(*p, *q),
        (Prim::Pt { p, .. }, Prim::Seg { p: q, .. }) => pt_seg(*p, q[0], q[1]),
        (Prim::Seg { p, .. }, Prim::Pt { p: q, .. }) => swap(pt_seg(*q, p[0], p[1])),
        (Prim::Pt { p, .. }, Prim::Tri { p: q, .. }) => pt_tri(*p, q),
        (Prim::Tri { p, .. }, Prim::Pt { p: q, .. }) => swap(pt_tri(*q, p)),
        (Prim::Seg { p, .. }, Prim::Seg { p: q, .. }) => seg_seg(p[0], p[1], q[0], q[1]),
        (Prim::Seg { p, .. }, Prim::Tri { p: q, .. }) => seg_tri(p[0], p[1], q),
        (Prim::Tri { p, .. }, Prim::Seg { p: q, .. }) => swap(seg_tri(q[0], q[1], p)),
        (Prim::Tri { p, .. }, Prim::Tri { p: q, .. }) => tri_tri(p, q),
    }
}

// ---------------------------------------------------------------------------
// AABB tree + branch and bound
// ---------------------------------------------------------------------------

struct Node {
    lo: [f64; 3],
    hi: [f64; 3],
    /// Leaf: `[start, start + count)` into the tree's primitive order.
    start: usize,
    count: usize,
    children: Option<(usize, usize)>,
}

struct Bvh {
    prims: Vec<Prim>,
    nodes: Vec<Node>,
}

const LEAF: usize = 4;

impl Bvh {
    fn build(prims: Vec<Prim>) -> Bvh {
        let mut bvh = Bvh {
            prims,
            nodes: Vec::new(),
        };
        let n = bvh.prims.len();
        bvh.build_node(0, n);
        bvh
    }

    /// Build the node covering `prims[start..start + count]`, splitting the
    /// slice in place at the median of the widest axis. Returns its index.
    fn build_node(&mut self, start: usize, count: usize) -> usize {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in &self.prims[start..start + count] {
            let (plo, phi) = p.bounds();
            for k in 0..3 {
                lo[k] = lo[k].min(plo[k]);
                hi[k] = hi[k].max(phi[k]);
            }
        }
        let me = self.nodes.len();
        self.nodes.push(Node {
            lo,
            hi,
            start,
            count,
            children: None,
        });
        if count <= LEAF {
            return me;
        }
        let axis = (0..3)
            .max_by(|&i, &j| {
                (hi[i] - lo[i])
                    .partial_cmp(&(hi[j] - lo[j]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        let center = |p: &Prim| {
            let (l, h) = p.bounds();
            (l[axis] + h[axis]) / 2.0
        };
        // Deterministic: a total order on the center, ties by the primitive's
        // existing position (`sort_by` is stable).
        self.prims[start..start + count].sort_by(|a, b| {
            center(a)
                .partial_cmp(&center(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let half = count / 2;
        let left = self.build_node(start, half);
        let right = self.build_node(start + half, count - half);
        self.nodes[me].children = Some((left, right));
        me
    }
}

/// Squared distance between two AABBs — 0 when they overlap. A lower bound on
/// the distance between anything inside them, which is what makes the pruning
/// exact.
fn box_box_dist2(a: &Node, b: &Node) -> f64 {
    let mut d2 = 0.0;
    for k in 0..3 {
        let gap = (b.lo[k] - a.hi[k]).max(a.lo[k] - b.hi[k]).max(0.0);
        d2 += gap * gap;
    }
    d2
}

/// One candidate closest pair: the seed distance and the two primitives.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    d2: f64,
    pa: Point3,
    pb: Point3,
    prim_a: Prim,
    prim_b: Prim,
}

/// At most this many DISTINCT entity pairs are refined. A pair beyond the cap
/// is not silently dropped: the answer falls to the mesh tier (see
/// [`CandidateSet::truncated`]), because an unrefined pair inside the band
/// could still hold the true minimum.
const MAX_CANDIDATES: usize = 64;

struct CandidateSet {
    best: Candidate,
    /// The best seed pair per DISTINCT pair of entities (face/edge/vertex)
    /// within `band` of `best` — the only pairs that can hold the true
    /// minimum, since a mesh distance differs from the true one by at most
    /// the band. Deduplicating by entity pair is what keeps this a handful of
    /// refinements instead of one per triangle pair: every triangle of a face
    /// shares that face's carrier surface, so refining the face pair once from
    /// its best seed answers for all of them.
    near: Vec<Candidate>,
    truncated: bool,
}

/// The closest primitive pair, by branch and bound over the two trees, plus
/// the best pair per distinct entity pair within `band` of it.
///
/// Two passes: the first finds the minimum (pruning hard), the second
/// collects the band around it. A single pass cannot collect the band, because
/// the threshold is only known once the minimum is.
fn closest_pairs(a: &Bvh, b: &Bvh, band: f64) -> CandidateSet {
    let mut best = Candidate {
        d2: f64::INFINITY,
        pa: Point3::new(0.0, 0.0, 0.0),
        pb: Point3::new(0.0, 0.0, 0.0),
        prim_a: a.prims[0],
        prim_b: b.prims[0],
    };
    walk(a, b, &mut |cand, cutoff| {
        if cand.d2 < best.d2 {
            best = cand;
            *cutoff = best.d2;
        }
    });

    // The band in squared terms, around the minimum DISTANCE.
    let cut = best.d2.sqrt() + band;
    let cut2 = cut * cut;
    let mut near: Vec<Candidate> = Vec::new();
    let mut truncated = false;
    walk(a, b, &mut |cand, cutoff| {
        *cutoff = cut2;
        if cand.d2 > cut2 {
            return;
        }
        let key = (cand.prim_a.on(), cand.prim_b.on());
        match near
            .iter_mut()
            .find(|c| (c.prim_a.on(), c.prim_b.on()) == key)
        {
            Some(seen) => {
                if cand.d2 < seen.d2 {
                    *seen = cand;
                }
            }
            None => {
                if near.len() < MAX_CANDIDATES {
                    near.push(cand);
                } else {
                    truncated = true;
                }
            }
        }
    });
    CandidateSet {
        best,
        near,
        truncated,
    }
}

/// Walk both trees, offering every pair that survives pruning to `visit`.
/// `visit` sets the squared-distance cutoff below which a pair is interesting;
/// anything at or beyond it is pruned.
fn walk(a: &Bvh, b: &Bvh, visit: &mut dyn FnMut(Candidate, &mut f64)) {
    let mut cutoff = f64::INFINITY;
    let mut stack = vec![(0usize, 0usize)];
    while let Some((ia, ib)) = stack.pop() {
        let (na, nb) = (&a.nodes[ia], &b.nodes[ib]);
        if box_box_dist2(na, nb) > cutoff {
            continue;
        }
        match (na.children, nb.children) {
            (None, None) => {
                for pa in &a.prims[na.start..na.start + na.count] {
                    for pb in &b.prims[nb.start..nb.start + nb.count] {
                        let (d2, x, y) = prim_prim(pa, pb);
                        visit(
                            Candidate {
                                d2,
                                pa: x,
                                pb: y,
                                prim_a: *pa,
                                prim_b: *pb,
                            },
                            &mut cutoff,
                        );
                    }
                }
            }
            (Some((l, r)), None) => {
                stack.push((l, ib));
                stack.push((r, ib));
            }
            (None, Some((l, r))) => {
                stack.push((ia, l));
                stack.push((ia, r));
            }
            // Descend the wider box so the bound tightens fast.
            (Some((al, ar)), Some((bl, br))) => {
                let span = |n: &Node| {
                    (n.hi[0] - n.lo[0])
                        .max(n.hi[1] - n.lo[1])
                        .max(n.hi[2] - n.lo[2])
                };
                if span(na) >= span(nb) {
                    stack.push((al, ib));
                    stack.push((ar, ib));
                } else {
                    stack.push((ia, bl));
                    stack.push((ia, br));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Exact refinement
// ---------------------------------------------------------------------------

/// What the refinement may move a foot along.
enum Carrier {
    /// A point that cannot move: a vertex or a free point. It IS its geometry,
    /// so there is nothing to slide.
    Fixed(Point3),
    /// A straight edge. The foot has ONE degree of freedom along it and the
    /// refinement must use it: the seed foot came from the other operand's
    /// chord facets, so freezing it reports the distance to a point that is
    /// only near the minimum — measured as 3.26e-4 m claimed EXACT for a box
    /// edge against a cylinder lateral whose true gap is 10, when the facet
    /// nearest the edge straddled the closest generator.
    Line([Point3; 2]),
    /// An analytic surface, trimmed by `face`'s loops.
    Surf(Surface, FaceId),
}

fn carrier(arena: &BrepArena, prim: &Prim) -> Option<Carrier> {
    match *prim {
        Prim::Tri { face, .. } => {
            let surface = arena.face(face).ok()?.surface?;
            Some(Carrier::Surf(surface, face))
        }
        Prim::Seg { exact: true, p, .. } => Some(Carrier::Line(p)),
        Prim::Pt { p, .. } => Some(Carrier::Fixed(p)),
        // A curved edge's chord: its foot is not on the true curve, and Q1
        // does not refine onto a curve (that is Q6's `edge_length` family).
        Prim::Seg { exact: false, .. } => None,
    }
}

/// Whether `p` lies on `face`'s trimmed surface, as far as the face's own
/// triangles can say: within `band` of them. The triangles partition the trim
/// exactly except for the chord band at a curved boundary, so this is the
/// strongest honest containment test available before Q6's trimming
/// predicates.
fn on_trimmed_face(arena: &BrepArena, face: FaceId, p: Point3, band: f64) -> bool {
    let Ok(mesh) = crate::tessellate::tessellate_face(arena, face) else {
        return false;
    };
    let point = |i: u32| -> Point3 {
        let i = i as usize * 3;
        Point3::new(
            mesh.positions[i],
            mesh.positions[i + 1],
            mesh.positions[i + 2],
        )
    };
    let band2 = band * band;
    mesh.indices
        .chunks_exact(3)
        .any(|t| pt_tri(p, &[point(t[0]), point(t[1]), point(t[2])]).0 <= band2)
}

/// Alternate the exact projections between the two carriers from the seed,
/// then certify. `Some((value, pa, pb))` only when the result is exact.
fn refine(
    arena: &BrepArena,
    ca: &Carrier,
    cb: &Carrier,
    seed: (Point3, Point3),
    band: f64,
) -> Option<(f64, Point3, Point3)> {
    let (mut pa, mut pb) = seed;
    let project = |c: &Carrier, toward: Point3| -> Option<Point3> {
        match c {
            Carrier::Fixed(p) => Some(*p),
            Carrier::Line(p) => Some(pt_seg(toward, p[0], p[1]).2),
            Carrier::Surf(s, _) => closest_point_on(s, toward),
        }
    };
    let scale = 1.0_f64.max(
        pa.as_array()
            .iter()
            .chain(pb.as_array().iter())
            .fold(0.0f64, |m, v| m.max(v.abs())),
    );
    // The stopping rule must be strictly TIGHTER than the certificate it
    // feeds, or certification is luck. The normality test below bounds a
    // dimensionless sine by `TAU_EVAL`, which is a transverse position
    // residual of `value · TAU_EVAL`; a sweep that stopped at
    // `TAU_EVAL · scale` can leave `scale / value` times more than that. Two
    // unit balls 48 m from the origin measured 5.00498 one way (the sweep
    // stopped at 1.8e-9 off-axis, sine 2.1e-9, certificate refused) and an
    // exact 5 the other — a 5 mm answer decided by operand order. `TAU_WORK`
    // is the workspace's working floor, three orders under `TAU_EVAL` and
    // still four orders above f64's spacing at metre scale, so the sweep
    // converges to well inside what the certificate asks.
    let mut converged = false;
    for _ in 0..64 {
        let na = project(ca, pb)?;
        let nb = project(cb, na)?;
        let moved = (dist2(na, pa) + dist2(nb, pb)).sqrt();
        pa = na;
        pb = nb;
        if moved <= TAU_WORK * scale {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }

    // Certificate 1: each foot is still on its own trimmed face.
    for (c, p) in [(ca, pa), (cb, pb)] {
        if let Carrier::Surf(_, face) = c {
            if !on_trimmed_face(arena, *face, p, band) {
                return None;
            }
        }
    }

    // Certificate 2: the segment is normal to both surfaces. Undefined at
    // zero length — a touching curved pair stays on the mesh tier.
    let d = sub(pb, pa);
    let value = len2(d).sqrt();
    if value <= band {
        return None;
    }
    let u = [d[0] / value, d[1] / value, d[2] / value];
    for (c, p) in [(ca, pa), (cb, pb)] {
        match c {
            Carrier::Surf(s, _) => {
                let n = outward_normal_at(s, p)?;
                // |u × n| is the sine of the angle between them; either sense
                // of n is fine (the normal may point along or against the
                // segment).
                if len2(cross(u, n)).sqrt() > TAU_EVAL {
                    return None;
                }
            }
            // A foot free to slide along an edge is a critical point of the
            // true distance only where the segment meets the edge at a right
            // angle, or where the foot has run into an endpoint (the clamped
            // minimum, which the endpoint vertex realizes exactly).
            Carrier::Line(q) => {
                let e = sub(q[1], q[0]);
                let el = len2(e).sqrt();
                let at_end = dist2(p, q[0]).sqrt().min(dist2(p, q[1]).sqrt()) <= TAU_EVAL * scale;
                if !at_end {
                    if el <= 0.0 {
                        return None;
                    }
                    let cosang = dot(u, [e[0] / el, e[1] / el, e[2] / el]).abs();
                    if cosang > TAU_EVAL {
                        return None;
                    }
                }
            }
            Carrier::Fixed(_) => {}
        }
    }
    Some((value, pa, pb))
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

/// The chord band of the seed: `RENDER_CHORD_TOLERANCE_REL × extent`, with
/// `extent` the larger of the two operands' bounding-box diagonals (the
/// documented `d_ε(r) = rel · r` band carried into a derived metric, not a
/// re-derived one).
fn chord_bound(a: &Bvh, b: &Bvh) -> f64 {
    let diag = |bvh: &Bvh| {
        let n = &bvh.nodes[0];
        ((n.hi[0] - n.lo[0]).powi(2) + (n.hi[1] - n.lo[1]).powi(2) + (n.hi[2] - n.lo[2]).powi(2))
            .sqrt()
    };
    RENDER_CHORD_TOLERANCE_REL * diag(a).max(diag(b))
}

/// Minimum distance and closest points between `a` and `b` (Q1).
///
/// The seed minimum is only good to the chord band, so EVERY pair within the
/// band of it is a candidate for the true minimum and each is refined. The
/// answer is the smallest value among them — a refined value when its pair
/// certified, the seed value otherwise — and it is reported `exact` only when
/// every candidate in the band was exact or certified. Otherwise some pair in
/// the band still holds an unrefined seed, and the true minimum could be up to
/// `chord_bound` smaller.
pub fn distance(arena: &BrepArena, a: Target, b: Target) -> Result<DistanceResult, KernelV2Error> {
    let bvh_a = Bvh::build(primitives(arena, a)?);
    let bvh_b = Bvh::build(primitives(arena, b)?);
    let band = chord_bound(&bvh_a, &bvh_b);
    let set = closest_pairs(&bvh_a, &bvh_b, band);

    let mut best: Option<(f64, Point3, Point3, Prim, Prim)> = None;
    let mut all_exact = !set.truncated;
    let candidates = if set.near.is_empty() {
        vec![set.best]
    } else {
        set.near.clone()
    };
    for c in &candidates {
        // Tier 1: both primitives represent their geometry exactly, so the
        // distance between them is the true distance between those pieces.
        let (value, pa, pb, exact) = if c.prim_a.is_exact() && c.prim_b.is_exact() {
            (c.d2.sqrt(), c.pa, c.pb, true)
        } else {
            // Tier 2: refine onto the analytic carriers and certify.
            match (carrier(arena, &c.prim_a), carrier(arena, &c.prim_b)) {
                (Some(ca), Some(cb)) => match refine(arena, &ca, &cb, (c.pa, c.pb), band) {
                    Some((v, qa, qb)) => (v, qa, qb, true),
                    None => (c.d2.sqrt(), c.pa, c.pb, false),
                },
                _ => (c.d2.sqrt(), c.pa, c.pb, false),
            }
        };
        all_exact &= exact;
        if best.is_none_or(|(b, ..)| value < b) {
            best = Some((value, pa, pb, c.prim_a, c.prim_b));
        }
    }

    let (value, pa, pb, prim_a, prim_b) = best.expect("at least one candidate");
    Ok(DistanceResult {
        value,
        points: [pa, pb],
        on: [prim_a.on(), prim_b.on()],
        exact: all_exact,
        chord_bound: band,
    })
}

/// The gap between `a` and `b` ALONG `direction`: both operands are projected
/// onto the direction and the answer is the gap between the two intervals,
/// negative when they overlap along it. The realizing points are the extreme
/// sample of each operand facing the other.
///
/// Exact only when NO inexact primitive reaches within the chord band of
/// either extreme: a curved face's extreme point along an arbitrary direction
/// is an analytic support point, which Q1 does not solve, and it lies up to the
/// band beyond that face's mesh samples. So an all-planar operand pair is
/// exact, and anything with a curved face near the extreme is honestly the mesh
/// tier even when the winning sample happens to be a plane's.
pub fn distance_along(
    arena: &BrepArena,
    a: Target,
    b: Target,
    direction: [f64; 3],
) -> Result<DistanceResult, KernelV2Error> {
    let dl = len2(direction).sqrt();
    if !dl.is_finite() || dl <= 0.0 {
        return Err(KernelV2Error::MeasureInvalidRequest {
            reason: "measure: `along` direction must be a non-zero finite vector",
        });
    }
    let u = [direction[0] / dl, direction[1] / dl, direction[2] / dl];
    let bvh_a = Bvh::build(primitives(arena, a)?);
    let bvh_b = Bvh::build(primitives(arena, b)?);
    let band = chord_bound(&bvh_a, &bvh_b);

    // The extreme sample of each operand along ±u, with what it sits on.
    //
    // The `exact` flag carries the SAME band argument `distance` makes, not
    // just the winning primitive's own tier: a curved face's true support
    // point along `u` lies up to the chord band beyond its mesh samples, so
    // any inexact primitive reaching within `band` of the extreme could hold
    // the real one. Reading only the winner would let an exact planar face
    // sitting a chord inside a curved one certify a gap that is wrong by the
    // sagitta — and, where the two tie exactly, would make the tier depend on
    // which primitive the BVH's sort visited first.
    let extreme = |prims: &[Prim], sign: f64| -> (f64, Point3, Option<On>, bool) {
        let mut best = (f64::NEG_INFINITY, Point3::new(0.0, 0.0, 0.0), None);
        let mut inexact_reach = f64::NEG_INFINITY;
        for p in prims {
            let pts: &[Point3] = match p {
                Prim::Tri { p, .. } => p,
                Prim::Seg { p, .. } => p,
                Prim::Pt { p, .. } => std::slice::from_ref(p),
            };
            for q in pts {
                let s = sign * dot(q.as_array(), u);
                if s > best.0 {
                    best = (s, *q, p.on());
                }
                if !p.is_exact() && s > inexact_reach {
                    inexact_reach = s;
                }
            }
        }
        let exact = inexact_reach < best.0 - band;
        (best.0, best.1, best.2, exact)
    };
    let (a_hi, pa_hi, on_a_hi, ex_a_hi) = extreme(&bvh_a.prims, 1.0);
    let (a_lo_n, pa_lo, on_a_lo, ex_a_lo) = extreme(&bvh_a.prims, -1.0);
    let (b_hi, pb_hi, on_b_hi, ex_b_hi) = extreme(&bvh_b.prims, 1.0);
    let (b_lo_n, pb_lo, on_b_lo, ex_b_lo) = extreme(&bvh_b.prims, -1.0);
    let (a_lo, b_lo) = (-a_lo_n, -b_lo_n);

    // b ahead of a along u, or a ahead of b; the larger gap is the real one
    // (they cannot both be positive).
    let (gap, pts, on, exact) = if (b_lo - a_hi) >= (a_lo - b_hi) {
        (
            b_lo - a_hi,
            [pa_hi, pb_lo],
            [on_a_hi, on_b_lo],
            ex_a_hi && ex_b_lo,
        )
    } else {
        (
            a_lo - b_hi,
            [pa_lo, pb_hi],
            [on_a_lo, on_b_hi],
            ex_a_lo && ex_b_hi,
        )
    };
    Ok(DistanceResult {
        value: gap,
        points: pts,
        on,
        exact,
        chord_bound: band,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_kernels_agree_with_hand_computed_cases() {
        let p = |x, y, z| Point3::new(x, y, z);
        // Point to a triangle's interior: the plane distance.
        let tri = [p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
        let (d2, _, foot) = pt_tri(p(0.25, 0.25, 2.0), &tri);
        assert!((d2 - 4.0).abs() < 1e-15, "{d2}");
        assert!((foot.z()).abs() < 1e-15);
        // Point beyond a corner: the corner.
        let (d2, _, foot) = pt_tri(p(-1.0, -1.0, 0.0), &tri);
        assert!((d2 - 2.0).abs() < 1e-15, "{d2}");
        assert_eq!(foot, tri[0]);
        // Crossed segments: zero, at the crossing.
        let (d2, _, _) = seg_seg(
            p(-1.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, -1.0, 0.0),
            p(0.0, 1.0, 0.0),
        );
        assert!(d2 < 1e-30, "{d2}");
        // A segment piercing a triangle: zero, not the edge distance.
        let (d2, x, y) = seg_tri(p(0.25, 0.25, -1.0), p(0.25, 0.25, 1.0), &tri);
        assert_eq!(d2, 0.0);
        assert_eq!(x, y);
        // Parallel segments: the perpendicular gap.
        let (d2, _, _) = seg_seg(
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 3.0, 0.0),
            p(1.0, 3.0, 0.0),
        );
        assert!((d2 - 9.0).abs() < 1e-15, "{d2}");
    }

    #[test]
    fn box_box_bound_is_zero_when_they_overlap_and_the_gap_otherwise() {
        let node = |lo: [f64; 3], hi: [f64; 3]| Node {
            lo,
            hi,
            start: 0,
            count: 0,
            children: None,
        };
        let a = node([0.0; 3], [1.0; 3]);
        let b = node([3.0, 0.0, 0.0], [4.0, 1.0, 1.0]);
        assert!((box_box_dist2(&a, &b) - 4.0).abs() < 1e-15);
        let c = node([0.5; 3], [2.0; 3]);
        assert_eq!(box_box_dist2(&a, &c), 0.0);
    }
}
