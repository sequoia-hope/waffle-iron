//! Pipe sweep (spec `specs/b2_pipe_sweep.md`): a circle of radius `r`
//! (optionally hollow, inner radius `rᵢ`) swept along a PLANAR, OPEN,
//! tangent-continuous chain of line and arc segments, assembled DIRECTLY as
//! one solid — no boolean, no cap-to-cap join.
//!
//! Each segment sweeps one lateral: a [`Surface::Cylinder`] for a line, a
//! [`Surface::Torus`] band for an arc. Consecutive laterals SHARE their rim
//! circle as one closed [`Curve::Circle`] edge (a twin pair), anchored at a
//! single seam vertex; so the solid is a chain of the `extrude_circle` /
//! `build_torus_revolve` tube templates glued rim-to-rim, plus two disc
//! caps. The seam of every segment runs along the path plane's BINORMAL
//! (anchor = rim centre `+ r·n̂`), the one phase that is the same on both
//! sides of every joint whatever way the arcs bend — on a torus band that is
//! the poloidal phase `φ₀ = ±π/2`, which the torus tessellators recover from
//! the seam arc itself (spec §2).
//!
//! Direct assembler (like the two templates): arcs and closed rims are
//! outside the Euler-operator vocabulary; the safety obligation is
//! discharged by `validate_solid` at exit. All argument validation happens
//! BEFORE the first arena mutation.

use super::*;

/// Angular agreement required of the two unit tangents meeting at an
/// interior joint (`1 − t_in · t_out ≤ tol`): the chain must be G1. A
/// non-tangent joint (a mitre) is a later slice, refused typed.
pub const PIPE_TANGENT_TOLERANCE: f64 = 1e-9;

/// Relative band for an arc endpoint's distance to its circle
/// (`| |p − c| − ρ | ≤ tol · ρ`), the same import band `ArcPolygon` uses.
pub const PIPE_ARC_ENDPOINT_TOLERANCE: f64 = 1e-9;

/// Relative clearance an arc's bend radius must keep above the tube radius
/// (`ρ − r > tol · (1 + ρ)`): a tube bent tighter than its own radius
/// pinches to a non-manifold seam (the revolve axis-clearance rule).
pub const PIPE_MIN_BEND_CLEARANCE_REL: f64 = 1e-9;

/// A validated pipe path: an orthonormal plane frame plus an open chain of
/// [`ProfileEdge`]s in `(u, v)` plane coordinates. Construction via
/// [`PipePath::new`] is the only way to obtain one, so a value IS the
/// evidence that every check in the module docs has passed.
#[derive(Debug, Clone, PartialEq)]
pub struct PipePath {
    origin: Point3,
    u: Vector3,
    v: Vector3,
    edges: Vec<ProfileEdge>,
    /// Per-joint unit tangent in plane coordinates (`edges.len() + 1`
    /// entries; interior joints hold the OUTGOING segment's start tangent,
    /// the canonical value both adjacent laterals use).
    tangents: Vec<Point2>,
    /// Per-joint point in plane coordinates.
    joints: Vec<Point2>,
    /// Per-segment sweep angle (arcs) or `0`.
    sweeps: Vec<f64>,
    length: f64,
}

impl PipePath {
    /// Validate and build a pipe path (spec §3):
    /// 1. frame finite and orthonormal (the `Profile::circle` gate);
    /// 2. ≥ 1 edge, chained head-to-tail (`edge[i].b == edge[i+1].a`
    ///    exactly), and OPEN (`PipeClosedPathUnsupported`);
    /// 3. lines of positive length; arcs of finite positive radius with
    ///    both endpoints on the circle and a sweep in `(0, 2π)` taken from
    ///    `ccw` (arcs beyond π are allowed — a U-bend is a π arc);
    /// 4. G1 at every interior joint (`PipeJoinNotTangent`).
    pub fn new(
        origin: Point3,
        u: Vector3,
        v: Vector3,
        edges: Vec<ProfileEdge>,
    ) -> Result<Self, KernelV2Error> {
        // 1. Frame.
        let frame = [
            origin.x(),
            origin.y(),
            origin.z(),
            u.x(),
            u.y(),
            u.z(),
            v.x(),
            v.y(),
            v.z(),
        ];
        if frame.iter().any(|c| !c.is_finite()) {
            return Err(KernelV2Error::ProfileNotFinite);
        }
        let c = cross(u, v);
        if c[0] * c[0] + c[1] * c[1] + c[2] * c[2] < crate::profile::BASIS_MIN_SQ_CROSS_NORM {
            return Err(KernelV2Error::ProfileDegenerateBasis);
        }
        let tol = crate::profile::CIRCLE_FRAME_ORTHONORMALITY_TOLERANCE;
        let u_sq = u.x() * u.x() + u.y() * u.y() + u.z() * u.z();
        let v_sq = v.x() * v.x() + v.y() * v.y() + v.z() * v.z();
        let uv = u.x() * v.x() + u.y() * v.y() + u.z() * v.z();
        if (u_sq.sqrt() - 1.0).abs() > tol || (v_sq.sqrt() - 1.0).abs() > tol || uv.abs() > tol {
            return Err(KernelV2Error::ProfileCircleFrameNotOrthonormal);
        }

        // 2. Chain shape.
        if edges.is_empty() {
            return Err(KernelV2Error::PipePathEmpty);
        }
        let n = edges.len();
        for (i, e) in edges.iter().enumerate() {
            let (a, b) = (edge_start(e), edge_end(e));
            let finite = |p: Point2| p.x().is_finite() && p.y().is_finite();
            if !finite(a) || !finite(b) {
                return Err(KernelV2Error::PipePathEdgeInvalid { segment: i });
            }
            if i + 1 < n && edge_start(&edges[i + 1]) != b {
                return Err(KernelV2Error::PipePathNotChained { segment: i });
            }
        }
        // 3. Per-edge geometry: tangents, sweeps, lengths.
        let mut starts: Vec<Point2> = Vec::with_capacity(n);
        let mut ends: Vec<Point2> = Vec::with_capacity(n);
        let mut sweeps: Vec<f64> = Vec::with_capacity(n);
        let mut length = 0.0f64;
        for (i, e) in edges.iter().enumerate() {
            let invalid = KernelV2Error::PipePathEdgeInvalid { segment: i };
            match *e {
                ProfileEdge::Line { a, b } => {
                    let d = [b.x() - a.x(), b.y() - a.y()];
                    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
                    if !(len.is_finite() && len > 0.0) {
                        return Err(invalid);
                    }
                    let t = Point2::new(d[0] / len, d[1] / len);
                    starts.push(t);
                    ends.push(t);
                    sweeps.push(0.0);
                    length += len;
                }
                ProfileEdge::Arc {
                    a,
                    b,
                    center,
                    radius,
                    ccw,
                } => {
                    if !(radius.is_finite() && radius > 0.0)
                        || !center.x().is_finite()
                        || !center.y().is_finite()
                    {
                        return Err(invalid);
                    }
                    let ra = [a.x() - center.x(), a.y() - center.y()];
                    let rb = [b.x() - center.x(), b.y() - center.y()];
                    let da = (ra[0] * ra[0] + ra[1] * ra[1]).sqrt();
                    let db = (rb[0] * rb[0] + rb[1] * rb[1]).sqrt();
                    let band = PIPE_ARC_ENDPOINT_TOLERANCE * radius;
                    if (da - radius).abs() > band || (db - radius).abs() > band {
                        return Err(invalid);
                    }
                    // CCW sweep from a to b about +n̂ in (0, 2π); an
                    // identical endpoint pair (a full circle) is not a
                    // path segment.
                    let mut sweep =
                        (ra[0] * rb[1] - ra[1] * rb[0]).atan2(ra[0] * rb[0] + ra[1] * rb[1]);
                    if sweep <= 0.0 {
                        sweep += 2.0 * std::f64::consts::PI;
                    }
                    if !ccw {
                        sweep = 2.0 * std::f64::consts::PI - sweep;
                    }
                    if !(sweep.is_finite() && sweep > 0.0 && sweep < 2.0 * std::f64::consts::PI) {
                        return Err(invalid);
                    }
                    let (sa, sb) = (ra[0] / da, ra[1] / da);
                    let (ea, eb) = (rb[0] / db, rb[1] / db);
                    // Tangent = radial rotated ±90°.
                    let (ts, te) = if ccw {
                        (Point2::new(-sb, sa), Point2::new(-eb, ea))
                    } else {
                        (Point2::new(sb, -sa), Point2::new(eb, -ea))
                    };
                    starts.push(ts);
                    ends.push(te);
                    sweeps.push(sweep);
                    length += sweep * radius;
                }
            }
        }

        // A closed chain (after the per-edge checks, so a lone degenerate
        // edge reads as invalid rather than closed).
        if edge_end(&edges[n - 1]) == edge_start(&edges[0]) {
            return Err(KernelV2Error::PipeClosedPathUnsupported);
        }

        // 4. G1 joints; the canonical joint tangent is the OUTGOING start
        //    tangent (joint n: the last end tangent).
        let mut tangents: Vec<Point2> = Vec::with_capacity(n + 1);
        let mut joints: Vec<Point2> = Vec::with_capacity(n + 1);
        for i in 0..n {
            if i > 0 {
                let (p, q) = (ends[i - 1], starts[i]);
                if 1.0 - (p.x() * q.x() + p.y() * q.y()) > PIPE_TANGENT_TOLERANCE {
                    return Err(KernelV2Error::PipeJoinNotTangent { joint: i });
                }
            }
            tangents.push(starts[i]);
            joints.push(edge_start(&edges[i]));
        }
        tangents.push(ends[n - 1]);
        joints.push(edge_end(&edges[n - 1]));

        Ok(Self {
            origin,
            u,
            v,
            edges,
            tangents,
            joints,
            sweeps,
            length,
        })
    }

    /// The path's segments, in chain order.
    pub fn edges(&self) -> &[ProfileEdge] {
        &self.edges
    }

    /// Exact arc length of the chain (lines + `sweep · radius` per arc).
    pub fn length(&self) -> f64 {
        self.length
    }

    /// Plane origin.
    pub fn origin(&self) -> Point3 {
        self.origin
    }

    /// In-plane unit basis vector `u`.
    pub fn u(&self) -> Vector3 {
        self.u
    }

    /// In-plane unit basis vector `v`.
    pub fn v(&self) -> Vector3 {
        self.v
    }

    /// Embed a plane point: `origin + x·u + y·v`.
    pub fn embed(&self, p: Point2) -> Point3 {
        Point3::new(
            self.origin.x() + p.x() * self.u.x() + p.y() * self.v.x(),
            self.origin.y() + p.x() * self.u.y() + p.y() * self.v.y(),
            self.origin.z() + p.x() * self.u.z() + p.y() * self.v.z(),
        )
    }

    /// Embed a plane direction: `x·u + y·v` (a unit vector for a unit input,
    /// the frame being orthonormal).
    fn embed_dir(&self, d: Point2) -> UnitVector3 {
        UnitVector3 {
            x: d.x() * self.u.x() + d.y() * self.v.x(),
            y: d.x() * self.u.y() + d.y() * self.v.y(),
            z: d.x() * self.u.z() + d.y() * self.v.z(),
        }
    }

    /// Unit plane normal `normalize(u × v)` — the pipe seam's binormal.
    pub fn unit_normal(&self) -> UnitVector3 {
        let c = cross(self.u, self.v);
        let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        UnitVector3 {
            x: c[0] / len,
            y: c[1] / len,
            z: c[2] / len,
        }
    }
}

fn edge_start(e: &ProfileEdge) -> Point2 {
    match *e {
        ProfileEdge::Line { a, .. } | ProfileEdge::Arc { a, .. } => a,
    }
}

fn edge_end(e: &ProfileEdge) -> Point2 {
    match *e {
        ProfileEdge::Line { b, .. } | ProfileEdge::Arc { b, .. } => b,
    }
}

/// Entities produced by [`pipe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The disc (or annular) cap at the path start; outward normal `−t₀`.
    pub start_cap: FaceId,
    /// The cap at the path end; outward normal `+tₙ`.
    pub end_cap: FaceId,
    /// Outer laterals, one per path segment in chain order (cylinder for a
    /// line, torus band for an arc).
    pub walls: Vec<FaceId>,
    /// Inner bore laterals (hollow pipes only), one per segment in chain
    /// order; empty for a solid tube.
    pub inner_walls: Vec<FaceId>,
}

/// Sweep a circle of `radius` (hollow when `inner_radius = Some(rᵢ)`,
/// `0 < rᵢ < radius`) along `path`. See the module docs and spec §3 for the
/// topology; `tests/b2_pipe.rs` pins the contract.
pub fn pipe(
    arena: &mut BrepArena,
    path: &PipePath,
    radius: f64,
    inner_radius: Option<f64>,
) -> Result<PipeResult, KernelV2Error> {
    // ---- argument validation (ALL before the first mutation) -------------
    if !(radius.is_finite() && radius > 0.0) {
        return Err(KernelV2Error::PipeNonPositiveRadius);
    }
    if let Some(ri) = inner_radius {
        if !(ri.is_finite() && ri > 0.0 && ri < radius) {
            return Err(KernelV2Error::PipeInnerRadiusInvalid);
        }
    }
    for (i, e) in path.edges.iter().enumerate() {
        if let ProfileEdge::Arc { radius: rho, .. } = *e {
            if rho - radius <= PIPE_MIN_BEND_CLEARANCE_REL * (1.0 + rho) {
                return Err(KernelV2Error::PipeBendRadiusTooSmall { segment: i });
            }
        }
    }

    // ---- geometry (pure) --------------------------------------------------
    let n = path.edges.len();
    let nrm = path.unit_normal();
    let centres: Vec<Point3> = path.joints.iter().map(|&p| path.embed(p)).collect();
    let tangents: Vec<UnitVector3> = path.tangents.iter().map(|&t| path.embed_dir(t)).collect();
    let offset =
        |c: Point3, d: f64| Point3::new(c.x() + d * nrm.x, c.y() + d * nrm.y, c.z() + d * nrm.z);
    let seg_geom: Vec<SegGeom> = path
        .edges
        .iter()
        .enumerate()
        .map(|(j, e)| match *e {
            ProfileEdge::Line { .. } => SegGeom::Line {
                axis_point: centres[j],
                axis_dir: tangents[j],
            },
            ProfileEdge::Arc {
                center,
                radius,
                ccw,
                ..
            } => SegGeom::Arc {
                center: path.embed(center),
                axis_dir: if ccw { nrm } else { neg(nrm) },
                major: radius,
            },
        })
        .collect();

    // ---- direct assembly ---------------------------------------------------
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);
    let mut faces: Vec<FaceId> = Vec::new();

    // Caps first (face ids stable), then chains.
    let fb = arena.faces.len() as u32;
    let (f_start, f_end) = (FaceId(fb), FaceId(fb + 1));
    let lb = arena.loops.len() as u32;
    let (loop_start, loop_end) = (LoopId(lb), LoopId(lb + 1));
    // Reserve cap loop/face slots by pushing placeholders we overwrite
    // below (the loops need the cap half-edge ids, which the chain builder
    // allocates).
    arena.loops.push(None);
    arena.loops.push(None);
    arena.faces.push(None);
    arena.faces.push(None);

    let outer = build_chain(
        arena,
        &ChainSpec {
            radius,
            reversed: false,
            centres: &centres,
            tangents: &tangents,
            seg_geom: &seg_geom,
            anchor_offset: radius,
            loop_start,
            loop_end,
            shell,
        },
        &offset,
    );
    faces.push(f_start);
    faces.push(f_end);
    faces.extend(outer.walls.iter().copied());

    let inner = inner_radius.map(|ri| {
        let ch = build_chain(
            arena,
            &ChainSpec {
                radius: ri,
                reversed: true,
                centres: &centres,
                tangents: &tangents,
                seg_geom: &seg_geom,
                anchor_offset: ri,
                loop_start,
                loop_end,
                shell,
            },
            &offset,
        );
        faces.extend(ch.walls.iter().copied());
        ch
    });

    // Cap loops: the outer circle (CCW around the cap normal) and, hollow,
    // one ring (CW). Each cap half-edge is the twin of the chain's boundary
    // rim, already allocated by `build_chain` with `next == prev == self`.
    arena.loops[loop_start.0 as usize] = Some(Loop {
        face: f_start,
        boundary: LoopBoundary::Edges(outer.cap_start_he),
        kind: LoopKind::Outer,
    });
    arena.loops[loop_end.0 as usize] = Some(Loop {
        face: f_end,
        boundary: LoopBoundary::Edges(outer.cap_end_he),
        kind: LoopKind::Outer,
    });
    let mut start_rings = Vec::new();
    let mut end_rings = Vec::new();
    if let Some(ch) = &inner {
        let rb = arena.loops.len() as u32;
        let (ring_start, ring_end) = (LoopId(rb), LoopId(rb + 1));
        arena.loops.push(Some(Loop {
            face: f_start,
            boundary: LoopBoundary::Edges(ch.cap_start_he),
            kind: LoopKind::Inner,
        }));
        arena.loops.push(Some(Loop {
            face: f_end,
            boundary: LoopBoundary::Edges(ch.cap_end_he),
            kind: LoopKind::Inner,
        }));
        // The ring half-edges were allocated pointing at the (outer) cap
        // loop ids; repoint them at their rings.
        for (he, lid) in [(ch.cap_start_he, ring_start), (ch.cap_end_he, ring_end)] {
            if let Some(Some(h)) = arena.half_edges.get_mut(he.0 as usize) {
                h.loop_id = lid;
            }
        }
        start_rings.push(ring_start);
        end_rings.push(ring_end);
    }
    arena.faces[f_start.0 as usize] = Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: centres[0],
            normal: neg(tangents[0]),
        })),
        outer_loop: loop_start,
        inner_loops: start_rings,
        shell,
    });
    arena.faces[f_end.0 as usize] = Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: centres[n],
            normal: tangents[n],
        })),
        outer_loop: loop_end,
        inner_loops: end_rings,
        shell,
    });

    // A solid tube is a ball; a hollow tube (an annulus swept along an open
    // path) is a solid torus — its through-bore is one handle.
    arena.shells.push(Some(Shell {
        solid,
        faces,
        genus: if inner.is_some() { 1 } else { 0 },
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(PipeResult {
        solid,
        shell,
        start_cap: f_start,
        end_cap: f_end,
        walls: outer.walls,
        inner_walls: inner.map(|c| c.walls).unwrap_or_default(),
    })
}

/// Per-segment surface frame, precomputed before assembly.
#[derive(Clone, Copy)]
enum SegGeom {
    Line {
        axis_point: Point3,
        axis_dir: UnitVector3,
    },
    Arc {
        center: Point3,
        /// `+n̂` for a CCW arc, `−n̂` for a CW one: the sweep from the
        /// segment's start joint to its end joint is CCW about it.
        axis_dir: UnitVector3,
        major: f64,
    },
}

struct ChainSpec<'a> {
    radius: f64,
    reversed: bool,
    centres: &'a [Point3],
    tangents: &'a [UnitVector3],
    seg_geom: &'a [SegGeom],
    anchor_offset: f64,
    loop_start: LoopId,
    loop_end: LoopId,
    shell: ShellId,
}

struct ChainBuilt {
    walls: Vec<FaceId>,
    /// The cap-side half-edge of the first rim (a closed circle,
    /// `next == prev == self`, `loop_id` = `loop_start`).
    cap_start_he: HalfEdgeId,
    /// Same for the last rim (`loop_id` = `loop_end`).
    cap_end_he: HalfEdgeId,
}

/// Assemble one chain of laterals (outer or inner bore) over `n` segments:
/// `n + 1` seam vertices, per segment the 4-half-edge lateral loop
/// `[rim_start, seam_up, rim_end, seam_dn]`, plus the two cap-side rim
/// half-edges. Rim directional normals: outward chain start `+tⱼ` / end
/// `−tⱼ₊₁` (each rim traverses TOWARD the opposite rim); reversed (bore)
/// chain the negations (AWAY — the validated cavity-wall sense).
fn build_chain(
    arena: &mut BrepArena,
    spec: &ChainSpec<'_>,
    offset: &dyn Fn(Point3, f64) -> Point3,
) -> ChainBuilt {
    let n = spec.seg_geom.len();
    let r = spec.radius;
    let sense = |t: UnitVector3| if spec.reversed { neg(t) } else { t };

    // Vertices: one seam anchor per joint.
    let vb = arena.vertices.len() as u32;
    for &c in spec.centres {
        arena.vertices.push(Some(Vertex {
            point: offset(c, spec.anchor_offset),
        }));
    }
    let vid = |i: usize| VertexId(vb + i as u32);

    // Half-edge ids: per segment 4 (lat_b, seam_up, lat_t, seam_dn) then
    // the two cap-side rims.
    let hb = arena.half_edges.len() as u32;
    let lat_b = |j: usize| HalfEdgeId(hb + 4 * j as u32);
    let seam_up = |j: usize| HalfEdgeId(hb + 4 * j as u32 + 1);
    let lat_t = |j: usize| HalfEdgeId(hb + 4 * j as u32 + 2);
    let seam_dn = |j: usize| HalfEdgeId(hb + 4 * j as u32 + 3);
    let cap_start_he = HalfEdgeId(hb + 4 * n as u32);
    let cap_end_he = HalfEdgeId(hb + 4 * n as u32 + 1);
    let lb = arena.loops.len() as u32;
    let loop_lat = |j: usize| LoopId(lb + j as u32);
    let fb = arena.faces.len() as u32;
    let face_lat = |j: usize| FaceId(fb + j as u32);

    let rim = |c: Point3, normal: UnitVector3| Curve::Circle {
        center: c,
        normal,
        radius: r,
    };
    let seam_curve = |j: usize, up: bool| match spec.seg_geom[j] {
        SegGeom::Line { .. } => Curve::LineSegment,
        SegGeom::Arc {
            center,
            axis_dir,
            major,
        } => Curve::Arc {
            center: offset(center, spec.anchor_offset),
            normal: if up { axis_dir } else { neg(axis_dir) },
            radius: major,
        },
    };

    for j in 0..n {
        let prev_rim = if j == 0 { cap_start_he } else { lat_t(j - 1) };
        let next_rim = if j + 1 == n { cap_end_he } else { lat_b(j + 1) };
        // rim_start: closed at v_j.
        arena.half_edges.push(Some(HalfEdge {
            twin: prev_rim,
            next: seam_up(j),
            prev: seam_dn(j),
            origin: vid(j),
            loop_id: loop_lat(j),
            curve: rim(spec.centres[j], sense(spec.tangents[j])),
        }));
        // seam_up: v_j → v_{j+1}.
        arena.half_edges.push(Some(HalfEdge {
            twin: seam_dn(j),
            next: lat_t(j),
            prev: lat_b(j),
            origin: vid(j),
            loop_id: loop_lat(j),
            curve: seam_curve(j, true),
        }));
        // rim_end: closed at v_{j+1}.
        arena.half_edges.push(Some(HalfEdge {
            twin: next_rim,
            next: seam_dn(j),
            prev: seam_up(j),
            origin: vid(j + 1),
            loop_id: loop_lat(j),
            curve: rim(spec.centres[j + 1], sense(neg(spec.tangents[j + 1]))),
        }));
        // seam_dn: v_{j+1} → v_j.
        arena.half_edges.push(Some(HalfEdge {
            twin: seam_up(j),
            next: lat_b(j),
            prev: lat_t(j),
            origin: vid(j + 1),
            loop_id: loop_lat(j),
            curve: seam_curve(j, false),
        }));
    }
    // Cap-side rims: exact negations of the chain's boundary rims.
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_b(0),
        next: cap_start_he,
        prev: cap_start_he,
        origin: vid(0),
        loop_id: spec.loop_start,
        curve: rim(spec.centres[0], neg(sense(spec.tangents[0]))),
    }));
    arena.half_edges.push(Some(HalfEdge {
        twin: lat_t(n - 1),
        next: cap_end_he,
        prev: cap_end_he,
        origin: vid(n),
        loop_id: spec.loop_end,
        curve: rim(spec.centres[n], neg(sense(neg(spec.tangents[n])))),
    }));

    let mut walls = Vec::with_capacity(n);
    for j in 0..n {
        arena.loops.push(Some(Loop {
            face: face_lat(j),
            boundary: LoopBoundary::Edges(lat_b(j)),
            kind: LoopKind::Outer,
        }));
        let surface = match spec.seg_geom[j] {
            SegGeom::Line {
                axis_point,
                axis_dir,
            } => Surface::Cylinder {
                axis_point,
                axis_dir,
                radius: r,
                reversed: spec.reversed,
            },
            SegGeom::Arc {
                center,
                axis_dir,
                major,
            } => Surface::Torus {
                center,
                axis_dir,
                major_radius: major,
                minor_radius: r,
                reversed: spec.reversed,
            },
        };
        arena.faces.push(Some(Face {
            surface: Some(surface),
            outer_loop: loop_lat(j),
            inner_loops: Vec::new(),
            shell: spec.shell,
        }));
        walls.push(face_lat(j));
    }
    ChainBuilt {
        walls,
        cap_start_he,
        cap_end_he,
    }
}
