//! The sweep assembler — B6 increments S2 (open path, polygon section) and
//! S5 (closed path), spec `specs/b6_general_sweep.md` §3–§5.
//!
//! A validated [`SweepPath`] already carries everything geometric: one
//! [`SweepStation`] per joint with its cut plane and transported frame,
//! and [`SweepStation::rim`] as the ONE rim-point computation. What is
//! left is bookkeeping, done the way `revolve::build_partial_revolve` and
//! `pipe::build_chain` do it — **direct assembly**, no Euler operators and
//! no booleans, because the shared rims of spec §3 are a statement about
//! WHICH vertex ids the neighbouring laterals reference, not about two
//! coincident copies agreeing to a tolerance.
//!
//! ## Layout
//!
//! With `n` path segments and a section of `e` edges (`e` vertices), the
//! working ring is the section's outer loop ordered **CCW about the path
//! tangent** (the section is stored CCW about `u × v`; it is reversed when
//! the path leaves along `−(u × v)`, exactly the extrude's `reverse`
//! flag). Ring `r` is the rim at station `r`: `e` vertices placed by
//! `stations[r].rim(p_j)`. An open path has `n + 1` rings and two caps; a
//! closed path has `n` rings (station `n` IS station 0 — `SweepPath::new`
//! computes it from the same tangents, so its cut plane is bit-identical)
//! and no caps.
//!
//! Per (segment `i`, section edge `j`) one lateral face with the
//! four-half-edge loop
//!
//! ```text
//!   wb(i,j): ring_i[j]     → ring_i[j+1]      (bottom rim edge)
//!   af(i,j+1): ring_i[j+1] → ring_{i+1}[j+1]  (longitudinal, forward, at vertex j+1)
//!   wt(i,j): ring_{i+1}[j+1] → ring_{i+1}[j]  (top rim edge)
//!   ab(i,j): ring_{i+1}[j] → ring_i[j]        (longitudinal, backward, at vertex j)
//! ```
//!
//! — the partial revolve's wall loop with the station index added. `wb(i,j)`
//! twins with `wt(i−1,j)` (or the start cap's `sc(j)` / the last segment's
//! `wt(n−1,j)` on a ring), `wt(i,j)` with `wb(i+1,j)` (or `ec(j)` / `wb(0,j)`),
//! and `af`/`ab` with each other. Half-edge ids are dense: slot
//! `4·(i·e + j) + {0: wb, 1: wt, 2: af, 3: ab}`, then the `2e` cap
//! half-edges of an open path.
//!
//! ## Surfaces (spec §1, §2)
//!
//! - **Line segment, any section edge** — a planar wall through the edge
//!   and the tangent; a mitred rim is sheared ALONG the tangent, which stays
//!   in that plane, so the wall is a planar quad whatever the corner. Its
//!   outward normal is `(edge × t̂)`, the extrude's rule.
//! - **Arc segment, edge parallel to the axis** — the cylinder about the
//!   arc's axis through the edge; `reversed` when the material lies on the
//!   larger-radius side (a bore wall).
//! - **Arc segment, edge perpendicular to the axis** — the plane through the
//!   edge normal to the axis (an annular sector), outward away from the
//!   material.
//! - **Arc segment, oblique edge** — a cone patch: increment S3, refused
//!   typed ([`KernelV2Error::SweepObliqueEdgeOnBend`]).
//!
//! Every classification is decided on the actual rim points in 3D against
//! the arc's own axis, so it holds for a 3D path as it does for a planar one.
//! The section vertex at `j` sweeps a [`Curve::Arc`] about the axis through
//! its own axis foot; the section clears the axis (a `SweepPath` gate), so
//! every such radius is positive.
//!
//! ## Refusals
//!
//! All before the first arena mutation: a non-polygon or holed section
//! ([`KernelV2Error::SweepSectionUnsupported`], S4 / `pipe`) and the oblique
//! edge above. A `SweepPath` value is the evidence for everything else.

use super::*;
use crate::construct::finalize_solid;

/// Per-edge alignment band against an arc segment's axis, relative to the
/// edge length: parallel when the radial extent is below it, perpendicular
/// when the axial extent is — the revolve's own
/// [`crate::construct::REVOLVE_EDGE_ALIGNMENT_TOLERANCE`], so a section the
/// lathe would accept as axis-aligned, the sweep does too.
pub const SWEEP_EDGE_ALIGNMENT_TOLERANCE: f64 = crate::construct::REVOLVE_EDGE_ALIGNMENT_TOLERANCE;

/// Entities produced by [`sweep`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The planar cap at the path start (outward normal `−t̂₀`); `None` for
    /// a closed path, which has no caps.
    pub start_cap: Option<FaceId>,
    /// The planar cap at the path end (outward normal `+t̂ₙ`); `None` for a
    /// closed path.
    pub end_cap: Option<FaceId>,
    /// Lateral faces: `walls[segment][edge]`, one per path segment per
    /// section edge, in chain order then working-ring order.
    pub walls: Vec<Vec<FaceId>>,
}

/// What a lateral's surface is, decided before any arena mutation.
enum Lateral {
    Plane { point: Point3, normal: V3 },
    Cylinder { radius: f64, reversed: bool },
}

/// Assemble the solid a validated [`SweepPath`] describes (module docs).
pub fn sweep(arena: &mut BrepArena, path: &SweepPath) -> Result<SweepResult, KernelV2Error> {
    // ---- section: polygon, no holes (S2) ---------------------------------
    let section = path.section();
    let outer: &[Point2] = match section.region() {
        ProfileRegion::Circle { .. } => {
            return Err(KernelV2Error::SweepSectionUnsupported {
                reason: "a circle section is the pipe (construct::pipe); the polygon assembler \
                         does not take it",
            });
        }
        ProfileRegion::ArcPolygon { .. } => {
            return Err(KernelV2Error::SweepSectionUnsupported {
                reason: "an arc-bearing section round a bend is B6 increment S4",
            });
        }
        ProfileRegion::Polygon { holes, .. } if !holes.is_empty() => {
            return Err(KernelV2Error::SweepSectionUnsupported {
                reason: "a holed section round a bend is B6 increment S4",
            });
        }
        ProfileRegion::Polygon { outer, .. } => outer,
    };

    let stations = path.stations();
    let segments = path.segments();
    let n = segments.len();
    let closed = path.closed();
    // Rings: one per station on an open path; the last station of a ring
    // IS its first.
    let rings = if closed { n } else { n + 1 };

    // ---- working ring: CCW about the start tangent -----------------------
    // The section is CCW about u × v (Profile's contract). `handed` says
    // whether the path leaves along +u × v (keep) or −u × v (reverse) — the
    // same decision `extrude` makes with its `reverse` flag, so a
    // single-segment sweep is that extrude.
    let sn = cross3(of_vector(section.u()), of_vector(section.v()));
    let t0 = of_unit(stations[0].frame.tangent);
    let ring: Vec<Point2> = if dot3(sn, t0) >= 0.0 {
        outer.to_vec()
    } else {
        outer.iter().rev().copied().collect()
    };
    let e = ring.len();

    // ---- geometry, all of it, before the first mutation -------------------
    // Rim points per (ring, vertex): the ONE computation, from the station's
    // canonical frame (spec §3).
    let rim_pts: Vec<Vec<Point3>> = (0..rings)
        .map(|r| ring.iter().map(|&p| stations[r].rim(p)).collect())
        .collect();
    // The tangent each lateral is swept along at its start station: the
    // segment's own (a line's direction; an arc's start tangent, which is
    // the station's outgoing frame tangent by construction).
    let seg_tangent = |i: usize| -> V3 {
        match segments[i].kind {
            SweepSegmentKind::Line { direction } => of_unit(direction),
            SweepSegmentKind::Arc { .. } => {
                of_unit(stations[i].frame_out.unwrap_or(stations[i].frame).tangent)
            }
        }
    };
    let ring_of = |r: usize| r % rings;

    let mut laterals: Vec<Vec<Lateral>> = Vec::with_capacity(n);
    // Per (segment, vertex): the longitudinal curve's arc centre + radius
    // when the segment is an arc (`None` on a line).
    let mut long_arcs: Vec<Vec<Option<(Point3, f64)>>> = Vec::with_capacity(n);
    for i in 0..n {
        let t = seg_tangent(i);
        let r0 = &rim_pts[i];
        let mut row = Vec::with_capacity(e);
        let mut arcs = Vec::with_capacity(e);
        match segments[i].kind {
            SweepSegmentKind::Line { .. } => {
                for j in 0..e {
                    let (a, b) = (of_point(r0[j]), of_point(r0[(j + 1) % e]));
                    let d = sub3(b, a);
                    // The wall plane contains the edge and the tangent; a
                    // mitred rim's shear is along `t`, so `d × t` is the
                    // same normal sheared or not. Outward = d × t for a
                    // ring CCW about t (the extrude's rule).
                    let normal = unit3(cross3(d, t)).ok_or(
                        // A section edge collinear with the tangent cannot
                        // happen: the edge lies in the cut plane, which is
                        // never parallel to the tangent (pierce rule +
                        // mitre gates). Refused rather than unwrapped.
                        KernelV2Error::SweepPathEdgeInvalid { segment: i },
                    )?;
                    row.push(Lateral::Plane {
                        point: r0[j],
                        normal,
                    });
                    arcs.push(None);
                }
            }
            SweepSegmentKind::Arc { center, axis, .. } => {
                let c = of_point(center);
                let ax = of_unit(axis);
                let foot = |p: V3| add3(c, scale3(ax, dot3(sub3(p, c), ax)));
                for j in 0..e {
                    let (a, b) = (of_point(r0[j]), of_point(r0[(j + 1) % e]));
                    let d = sub3(b, a);
                    let len = len3(d);
                    let along = dot3(d, ax);
                    let radial_extent = len3(cross3(d, ax));
                    // Material side: left of the edge for a ring CCW about t.
                    let left = cross3(t, d);
                    let fa = foot(a);
                    let ra = sub3(a, fa);
                    let rho = len3(ra);
                    arcs.push(Some((as_point(fa), rho)));
                    if radial_extent <= SWEEP_EDGE_ALIGNMENT_TOLERANCE * len {
                        // Parallel to the axis: a cylinder wall of radius ρ.
                        // Reversed (cavity sense) when the material lies on
                        // the larger-radius side.
                        row.push(Lateral::Cylinder {
                            radius: rho,
                            reversed: dot3(left, ra) > 0.0,
                        });
                    } else if along.abs() <= SWEEP_EDGE_ALIGNMENT_TOLERANCE * len {
                        // Perpendicular: an annular sector normal to the
                        // axis, outward away from the material.
                        let normal = if dot3(left, ax) < 0.0 {
                            ax
                        } else {
                            scale3(ax, -1.0)
                        };
                        row.push(Lateral::Plane {
                            point: r0[j],
                            normal,
                        });
                    } else {
                        return Err(KernelV2Error::SweepObliqueEdgeOnBend {
                            segment: i,
                            edge: j,
                        });
                    }
                }
            }
        }
        laterals.push(row);
        long_arcs.push(arcs);
    }

    // ---- id layout --------------------------------------------------------
    let vb = arena.vertices.len() as u32;
    let vid = |r: usize, j: usize| VertexId(vb + (ring_of(r) * e + j % e) as u32);

    let hb = arena.half_edges.len() as u32;
    let slot = |i: usize, j: usize, s: u32| HalfEdgeId(hb + 4 * ((i * e + j % e) as u32) + s);
    let wb = |i: usize, j: usize| slot(i, j, 0);
    let wt = |i: usize, j: usize| slot(i, j, 1);
    let af = |i: usize, j: usize| slot(i, j, 2);
    let ab = |i: usize, j: usize| slot(i, j, 3);
    let cap_base = hb + 4 * (n * e) as u32;
    let sc = |j: usize| HalfEdgeId(cap_base + (j % e) as u32);
    let ec = |j: usize| HalfEdgeId(cap_base + e as u32 + (j % e) as u32);

    let lb = arena.loops.len() as u32;
    let loop_wall = |i: usize, j: usize| LoopId(lb + (i * e + j % e) as u32);
    let loop_start = LoopId(lb + (n * e) as u32);
    let loop_end = LoopId(lb + (n * e) as u32 + 1);
    let fb = arena.faces.len() as u32;
    let f_wall = |i: usize, j: usize| FaceId(fb + (i * e + j % e) as u32);
    let f_start = FaceId(fb + (n * e) as u32);
    let f_end = FaceId(fb + (n * e) as u32 + 1);
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    // ---- vertices -----------------------------------------------------------
    for pts in rim_pts.iter().take(rings) {
        for &p in pts {
            arena.vertices.push(Some(Vertex { point: p }));
        }
    }

    // ---- half-edges -----------------------------------------------------------
    for i in 0..n {
        let axis_dir = match segments[i].kind {
            SweepSegmentKind::Arc { axis, .. } => Some(of_unit(axis)),
            SweepSegmentKind::Line { .. } => None,
        };
        let long_curve = |j: usize, forward: bool| -> Curve {
            match (axis_dir, long_arcs[i][j % e]) {
                (Some(ax), Some((foot, rho))) => Curve::Arc {
                    center: foot,
                    normal: as_unit(if forward { ax } else { scale3(ax, -1.0) }),
                    radius: rho,
                },
                _ => Curve::LineSegment,
            }
        };
        let below = |j: usize| -> HalfEdgeId {
            if i > 0 {
                wt(i - 1, j)
            } else if closed {
                wt(n - 1, j)
            } else {
                sc(j)
            }
        };
        let above = |j: usize| -> HalfEdgeId {
            if i + 1 < n {
                wb(i + 1, j)
            } else if closed {
                wb(0, j)
            } else {
                ec(j)
            }
        };
        for j in 0..e {
            // wb(i,j): ring_i[j] → ring_i[j+1].
            arena.half_edges.push(Some(HalfEdge {
                twin: below(j),
                next: af(i, j + 1),
                prev: ab(i, j),
                origin: vid(i, j),
                loop_id: loop_wall(i, j),
                curve: Curve::LineSegment,
            }));
            // wt(i,j): ring_{i+1}[j+1] → ring_{i+1}[j].
            arena.half_edges.push(Some(HalfEdge {
                twin: above(j),
                next: ab(i, j),
                prev: af(i, j + 1),
                origin: vid(i + 1, j + 1),
                loop_id: loop_wall(i, j),
                curve: Curve::LineSegment,
            }));
            // af(i,j): ring_i[j] → ring_{i+1}[j], in wall (i, j−1)'s loop.
            arena.half_edges.push(Some(HalfEdge {
                twin: ab(i, j),
                next: wt(i, j + e - 1),
                prev: wb(i, j + e - 1),
                origin: vid(i, j),
                loop_id: loop_wall(i, j + e - 1),
                curve: long_curve(j, true),
            }));
            // ab(i,j): ring_{i+1}[j] → ring_i[j], in wall (i, j)'s loop.
            arena.half_edges.push(Some(HalfEdge {
                twin: af(i, j),
                next: wb(i, j),
                prev: wt(i, j),
                origin: vid(i + 1, j),
                loop_id: loop_wall(i, j),
                curve: long_curve(j, false),
            }));
        }
    }
    if !closed {
        // Start cap winds CCW about −t̂₀: it visits the ring in reverse.
        for j in 0..e {
            arena.half_edges.push(Some(HalfEdge {
                twin: wb(0, j),
                next: sc(j + e - 1),
                prev: sc(j + 1),
                origin: vid(0, j + 1),
                loop_id: loop_start,
                curve: Curve::LineSegment,
            }));
        }
        // End cap winds CCW about +t̂ₙ: forward round the last ring.
        for j in 0..e {
            arena.half_edges.push(Some(HalfEdge {
                twin: wt(n - 1, j),
                next: ec(j + 1),
                prev: ec(j + e - 1),
                origin: vid(n, j),
                loop_id: loop_end,
                curve: Curve::LineSegment,
            }));
        }
    }

    // ---- loops, faces ---------------------------------------------------------
    let mut walls: Vec<Vec<FaceId>> = Vec::with_capacity(n);
    let mut shell_faces: Vec<FaceId> = Vec::with_capacity(n * e + 2);
    for (i, row) in laterals.iter().enumerate() {
        let mut faces_i = Vec::with_capacity(e);
        for (j, lat) in row.iter().enumerate() {
            arena.loops.push(Some(Loop {
                face: f_wall(i, j),
                boundary: LoopBoundary::Edges(wb(i, j)),
                kind: LoopKind::Outer,
            }));
            let surface = match *lat {
                Lateral::Plane { point, normal } => Surface::Plane(Plane {
                    point,
                    normal: as_unit(normal),
                }),
                Lateral::Cylinder { radius, reversed } => match segments[i].kind {
                    SweepSegmentKind::Arc { center, axis, .. } => Surface::Cylinder {
                        axis_point: center,
                        axis_dir: axis,
                        radius,
                        reversed,
                    },
                    // A cylinder lateral is only ever classified on an arc.
                    SweepSegmentKind::Line { .. } => {
                        return Err(KernelV2Error::SweepPathEdgeInvalid { segment: i });
                    }
                },
            };
            arena.faces.push(Some(Face {
                surface: Some(surface),
                outer_loop: loop_wall(i, j),
                inner_loops: Vec::new(),
                shell,
            }));
            faces_i.push(f_wall(i, j));
            shell_faces.push(f_wall(i, j));
        }
        walls.push(faces_i);
    }
    let (start_cap, end_cap) = if closed {
        (None, None)
    } else {
        arena.loops.push(Some(Loop {
            face: f_start,
            boundary: LoopBoundary::Edges(sc(0)),
            kind: LoopKind::Outer,
        }));
        arena.loops.push(Some(Loop {
            face: f_end,
            boundary: LoopBoundary::Edges(ec(0)),
            kind: LoopKind::Outer,
        }));
        // Both end stations are perpendicular to their tangent (a
        // `SweepPath` invariant), so the caps are the section's own plane
        // carried to each end.
        let tn = of_unit(stations[n].frame.tangent);
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: rim_pts[0][0],
                normal: as_unit(scale3(t0, -1.0)),
            })),
            outer_loop: loop_start,
            inner_loops: Vec::new(),
            shell,
        }));
        arena.faces.push(Some(Face {
            surface: Some(Surface::Plane(Plane {
                point: rim_pts[n][0],
                normal: as_unit(tn),
            })),
            outer_loop: loop_end,
            inner_loops: Vec::new(),
            shell,
        }));
        shell_faces.push(f_start);
        shell_faces.push(f_end);
        (Some(f_start), Some(f_end))
    };

    // An open sweep of a simple section is a ball; a closed one is a solid
    // torus — its through-hole is the one handle.
    arena.shells.push(Some(Shell {
        solid,
        faces: shell_faces,
        genus: u32::from(closed),
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(SweepResult {
        solid,
        shell,
        start_cap,
        end_cap,
        walls,
    })
}
