//! **D1d** of `specs/drawings_and_mbd.md` (§5.2 increment 4): cut a solid
//! with a plane, and report the cap's hatchable loops plus the cut solid.
//!
//! ## The cut is the kernel's own boolean, and nothing else
//!
//! §5.2 says the section "runs the yang pipeline with a half-space operand
//! built as a box that encloses the solid's AABB with a margin", and that is
//! literally what happens here. The reason is Q2's reason
//! ([`crate::interference`]): a second implementation of "which side of this
//! plane is the material on" — a face-by-face trim, a classify-and-stitch —
//! would be a second source of truth about the same question, free to disagree
//! with the Subtract the user runs against the same plane a moment later. A
//! section view that shows a cap the model does not have is worse than no
//! section view.
//!
//! ## The scratch arena, and why the result still lands in the live one
//!
//! The half-space box is scaffolding: it must not appear in the caller's
//! arena, and the boolean's journal entries about it must not move the
//! persistent ids every later reference resolves through. So the box is built
//! in a scratch [`BrepArena`] alongside a deep copy of the solid
//! ([`crate::transform::copy_solid_into`], the same move Q2 makes) and the
//! Intersect runs there.
//!
//! Unlike Q2, a section is not a pure query: its whole point is the cut body,
//! which the caller projects with D1a–c. So the result is copied BACK into the
//! live arena and the scratch is dropped. What the live arena gains is one
//! solid and one `Transform` journal entry — not a boolean's worth of
//! entities, and not the box.
//!
//! **Pid hygiene.** `copy_solid_into` records `(source pid → copy pid)` in the
//! destination's journal, so a fresh scratch arena's own allocator would hand
//! out numbers that collide with the live pids appearing as those sources, and
//! [`crate::journal::face_lineage`] would follow a chain through the
//! collision. The scratch arena's allocator is therefore started at the live
//! arena's `next_pid` and the live arena's is advanced past the scratch's
//! afterwards, so every pid in both arenas comes from ONE monotonic sequence
//! and a lineage walk cannot cross wires. That is what makes the cap's
//! attribution below a fact rather than a coincidence.
//!
//! ## Finding the cap: lineage, cross-checked against geometry
//!
//! §5.2 names the cap as "the face whose plane equals the cut plane, found
//! through `face_provenance` as the only face descended from the box operand".
//! Both halves of that sentence are used, and they check each other:
//!
//! - **Lineage** ([`crate::journal::face_lineage`]): the box's base face is
//!   the only one of its six that can meet the solid at all, because the box
//!   encloses the solid's AABB with a margin. So an output face whose lineage
//!   root is a box face pid is a cap face, and if its root is a WALL or the
//!   far face the box did not enclose the solid — a loud
//!   [`KernelV2Error::SectionCapNotOnCutPlane`], which is the margin
//!   derivation's own test.
//! - **Geometry**: a cap face lies in the cut plane with outward normal along
//!   the cut normal. Every lineage-attributed face must pass this, or the two
//!   disagree and the section STOPs.
//!
//! Geometry is also what catches the case lineage CANNOT: a cut plane coplanar
//! with a model face goes through the §4.5.5 Stage-0 overlay, which replaces
//! the overlapping region with ONE shared trimmed surface — and that surface
//! may be attributed to the model operand rather than to the box. The cap is
//! then found by its plane and [`SectionCut::cap_shared_with_model`] says so,
//! rather than the section reporting an empty cap for a cut that plainly has
//! one.
//!
//! ## The containment net (deviation N69's class)
//!
//! Whatever the Intersect returns, every point of `solid ∩ halfspace`
//! satisfies `(p − origin)·normal ≤ 0`. That is checked
//! ([`KernelV2Error::SectionCutOutsideHalfSpace`]) and it is not hypothetical:
//! a box meeting a solid along an EDGE comes back from `Intersect` as a
//! bit-for-bit copy of an operand (measured 2026-10-03, deviation N69), and a
//! copy of the SOLID straddles the plane. A containment proof, not a
//! tolerance; the only slack is the render chord band the tessellated operands
//! already carry.
//!
//! ## Exactness of the cap curves
//!
//! The cap lies IN the cut plane, so the projection into the plane's own
//! `(u, v)` is an ISOMETRY — not a general orthographic projection. A line
//! stays a line, a circle stays a circle of the same radius, and an ellipse
//! arc stays an ellipse arc with its semi-axes unchanged. That last arm is why
//! this module has its own curve conversion rather than reusing
//! [`super::project_edge`] wholesale: `project_edge` samples
//! [`Curve::EllipseArc`] into a polyline, which is the honest answer for a
//! general view (D1a sets the analytic bar at line and circle) but throws away
//! an exactness the isometric case has for free — and an oblique plane cut of
//! a cylinder is an ELLIPSE, the canonical section a drawing needs.
//!
//! A [`Curve::HyperbolaArc`] or [`Curve::SurfacePair`] cap edge is still a
//! polyline, and the loop it bounds reports `exact = false` so its area is
//! known to be low by the polyline's sagitta deficit rather than quietly
//! wrong.

use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use waffle_types::kernel::projection::{Curve2, SectionLoop, ViewBasis, ViewFrame};

use crate::arena::{BrepArena, Curve, FaceId, Pid, SolidId, Surface};
use crate::error::KernelV2Error;

/// What [`section_with_plane`] found, in the kernel's own vocabulary; the
/// adapter lifts this into [`waffle_types::kernel::projection::SectionResult`].
#[derive(Debug, Clone, PartialEq)]
pub struct SectionCut {
    /// The cap's boundary loops in `plane_basis`'s `(u, v)`, outer loops and
    /// holes alike (each loop's own `signed_area` says which it is). Empty
    /// when the plane misses the solid.
    pub cap_loops: Vec<SectionLoop>,
    /// The kept half-space, in the LIVE arena — `None` when the cut keeps no
    /// material. When the plane misses the solid on the kept side this is the
    /// INPUT solid itself (nothing was cut, so nothing was copied).
    pub cut_solid: Option<SolidId>,
    /// The frame the loops are expressed in.
    pub plane_basis: ViewBasis,
    /// Whether at least one cap face was found by its PLANE rather than by
    /// its descent from the cutting box — the §4.5.5 Stage-0 signature of a
    /// cut plane coplanar with a model face (see the module docs).
    pub cap_shared_with_model: bool,
}

/// Cut `solid` with the plane through `origin` with normal `normal`, keeping
/// `(p − origin)·n̂ ≤ 0`.
///
/// `rel_chord_tolerance` is the relative chord bound for the cap edges that
/// cannot stay analytic (a hyperbola or surface-pair boundary).
///
/// See the module docs for the construction, the attribution of the cap and
/// the containment net. The error contract is
/// [`KernelV2Error::SectionDegeneratePlane`],
/// [`KernelV2Error::SectionSolidUnbounded`],
/// [`KernelV2Error::SectionCutOutsideHalfSpace`],
/// [`KernelV2Error::SectionCapNotOnCutPlane`], plus every error
/// [`crate::boolean_op`] can raise EXCEPT
/// [`KernelV2Error::EmptyBooleanResult`], which is the typed "the cut keeps
/// nothing" answer rather than a failure.
pub fn section_with_plane(
    arena: &mut BrepArena,
    solid: SolidId,
    origin: [f64; 3],
    normal: [f64; 3],
    rel_chord_tolerance: f64,
) -> Result<SectionCut, KernelV2Error> {
    // ---- the plane (all validation before any arena work) ----------------
    if origin.iter().any(|c| !c.is_finite()) || normal.iter().any(|c| !c.is_finite()) {
        return Err(KernelV2Error::SectionDegeneratePlane);
    }
    let nlen = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if !(nlen.is_finite() && nlen > 0.0) {
        return Err(KernelV2Error::SectionDegeneratePlane);
    }
    let n = [normal[0] / nlen, normal[1] / nlen, normal[2] / nlen];
    // The viewer stands on the DISCARDED side and looks along `−n̂`, so the
    // cap is seen with the kept material behind it (the drafting convention)
    // and `(u, v, n̂)` is right-handed — which is what makes a loop traversed
    // counter-clockwise about the cap's outward normal come out with a
    // POSITIVE signed area.
    let plane_basis = ViewFrame {
        origin,
        dir: [-n[0], -n[1], -n[2]],
        up: ViewFrame::looking_along([-n[0], -n[1], -n[2]]).up,
    }
    .basis()
    .ok_or(KernelV2Error::SectionDegeneratePlane)?;

    // ---- scale, and where the solid sits relative to the plane -----------
    // CONSERVATIVE on purpose: every point of the solid is inside this box, so
    // the "the plane misses the solid" decisions below are proofs. A `None`
    // here is a solid with no sound bound at all, which is a decline.
    let (lo, hi) = crate::introspect::conservative_aabb(arena, solid)?
        .ok_or(KernelV2Error::SectionSolidUnbounded { solid })?;
    let diag = crate::mass::diagonal(lo, hi);
    let band = crate::tessellate::RENDER_CHORD_TOLERANCE_REL * diag;
    // Signed plane distance of the box: over its eight corners, which is the
    // exact range of the box (the distance is linear).
    let mut dmin = f64::INFINITY;
    let mut dmax = f64::NEG_INFINITY;
    for i in 0..8 {
        let c = [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let d = (0..3).map(|k| (c[k] - origin[k]) * n[k]).sum::<f64>();
        dmin = dmin.min(d);
        dmax = dmax.max(d);
    }
    let empty_cap = |cut_solid: Option<SolidId>| SectionCut {
        cap_loops: Vec::new(),
        cut_solid,
        plane_basis,
        cap_shared_with_model: false,
    };
    if dmax <= 0.0 {
        // Every point of the solid is on the kept side (and the box is
        // conservative, so this is a proof). Nothing is cut, so nothing is
        // copied: the cut solid IS the input. Typed empty cap, not an error.
        return Ok(empty_cap(Some(solid)));
    }
    if dmin >= 0.0 {
        // Every point is on the discarded side: no material survives, and
        // kernel-v2 has no empty solid to name.
        return Ok(empty_cap(None));
    }

    // ---- the half-space box ---------------------------------------------
    // The margin is derived from the solid's own box and nothing else: a
    // half-diagonal is the radius of the AABB about its centre, so a base
    // rectangle of half-extent `R + margin` covers the solid's projection
    // onto the cut plane whatever the plane's orientation, and a depth of
    // `−dmin + margin` clears its deepest point below the plane. An absolute
    // pad would be a second scale in the file: too small for a bridge and a
    // numerical insult to a bearing (the same reasoning that removed the 1 m
    // sweep pad at P0012).
    let r = 0.5 * diag;
    let margin = diag;
    let half = r + margin;
    let depth = -dmin + margin;
    let centre = [
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    ];
    let s: f64 = (0..3).map(|k| (centre[k] - origin[k]) * n[k]).sum();
    // The base rectangle's centre: the AABB centre dropped onto the plane.
    let base = [
        centre[0] - s * n[0],
        centre[1] - s * n[1],
        centre[2] - s * n[2],
    ];
    let (bu, bv) = (plane_basis.u, plane_basis.v);
    // `cross(u, v) = n̂` (the basis makes `(u, v, −w) = (u, v, n̂)`
    // right-handed), so the profile's front normal is `+n̂` and extruding
    // along `−n̂` puts the base face in the cut plane with outward normal
    // `+n̂` — pointing out of the kept material.
    let profile = crate::Profile::new(
        Point3::new(base[0], base[1], base[2]),
        Vector3::new(bu[0], bu[1], bu[2]),
        Vector3::new(bv[0], bv[1], bv[2]),
        vec![
            Point2::new(-half, -half),
            Point2::new(half, -half),
            Point2::new(half, half),
            Point2::new(-half, half),
        ],
        Vec::new(),
    )?;

    let mut scratch = BrepArena::new();
    // One monotonic pid sequence across both arenas — see the module docs.
    scratch.next_pid = arena.next_pid;
    let copied = crate::transform::copy_solid_into(arena, solid, &mut scratch)?;
    let boxed = crate::construct::extrude(
        &mut scratch,
        &profile,
        Vector3::new(-n[0], -n[1], -n[2]),
        depth,
    )?;
    let box_pids: Vec<(FaceId, Option<Pid>)> = std::iter::once(boxed.base)
        .chain(std::iter::once(boxed.top))
        .chain(boxed.walls.iter().copied())
        .map(|f| (f, scratch.face_pid(f)))
        .collect();

    // ---- the cut ---------------------------------------------------------
    let cut = match crate::boolean_op(&mut scratch, copied, boxed.solid, BoolOp::Intersect) {
        Ok(cut) => cut,
        // The regularized intersection has no material. The conservative box
        // straddled the plane but the solid itself does not reach across it —
        // a miss, typed, not an error.
        Err(KernelV2Error::EmptyBooleanResult) => {
            arena.next_pid = arena.next_pid.max(scratch.next_pid);
            return Ok(empty_cap(None));
        }
        // Every other pipeline error is itself: a Stage-0 coplanar refusal,
        // an N69 graze STOP, a Stage-3/4/5 wall. Never an answer about the
        // geometry.
        Err(other) => return Err(other),
    };

    // ---- P10: the result lies in the kept half-space ---------------------
    let mut worst = f64::NEG_INFINITY;
    for &h in &crate::introspect::solid_half_edges(&scratch, cut)? {
        let p = scratch
            .vertex(scratch.half_edge(h)?.origin)?
            .point
            .as_array();
        let d = (0..3).map(|k| (p[k] - origin[k]) * n[k]).sum::<f64>();
        worst = worst.max(d);
    }
    if worst > band {
        return Err(KernelV2Error::SectionCutOutsideHalfSpace {
            detail: format!(
                "cut of {solid:?} reaches {worst:e} past the plane (band {band:e}); \
                 the Intersect did not return solid ∩ half-space"
            ),
        });
    }

    // ---- the cap faces ---------------------------------------------------
    let mut cap_faces: Vec<FaceId> = Vec::new();
    let mut cap_shared_with_model = false;
    let mut faces: Vec<FaceId> = Vec::new();
    for &sh in &scratch.solid(cut)?.shells {
        faces.extend(scratch.shell(sh)?.faces.iter().copied());
    }
    for face in faces {
        let from_box = match scratch.face_pid(face) {
            Some(pid) => {
                let root = crate::journal::face_lineage(&scratch.journal, pid).root;
                box_pids.iter().find(|(_, p)| *p == Some(root)).copied()
            }
            None => None,
        };
        let on_plane = face_is_on_plane(&scratch, face, origin, n, band)?;
        match (from_box, on_plane) {
            // Attributed to the box AND on the cut plane: the cap, as §5.2
            // describes it.
            (Some((bf, _)), true) => {
                if Some(bf) != Some(boxed.base) {
                    return Err(KernelV2Error::SectionCapNotOnCutPlane {
                        detail: format!(
                            "cap candidate {face:?} descends from box face {bf:?}, not its base \
                             {:?}: the box did not enclose the solid (half {half:e}, depth \
                             {depth:e}, diag {diag:e})",
                            boxed.base
                        ),
                    });
                }
                cap_faces.push(face);
            }
            // Attributed to the box but NOT on the cut plane: lineage and
            // geometry disagree, and nothing here can say which is right.
            (Some((bf, _)), false) => {
                return Err(KernelV2Error::SectionCapNotOnCutPlane {
                    detail: format!(
                        "{face:?} descends from box face {bf:?} but its surface is not the cut \
                         plane (origin {origin:?}, normal {n:?}, band {band:e})"
                    ),
                });
            }
            // On the cut plane but attributed to the MODEL: the Stage-0
            // shared trimmed surface of a coplanar cut (module docs).
            (None, true) => {
                cap_shared_with_model = true;
                cap_faces.push(face);
            }
            (None, false) => {}
        }
    }

    let n_seg = crate::tessellate::circle_segment_count(rel_chord_tolerance);
    let mut cap_loops = Vec::new();
    for face in cap_faces {
        let f = scratch.face(face)?;
        for lp in std::iter::once(f.outer_loop).chain(f.inner_loops.iter().copied()) {
            cap_loops.push(cap_loop(&scratch, lp, &plane_basis, n, n_seg)?);
        }
    }

    // ---- the cut solid, back in the live arena ---------------------------
    let live = crate::transform::copy_solid_into(&scratch, cut, arena)?;
    arena.next_pid = arena.next_pid.max(scratch.next_pid);
    Ok(SectionCut {
        cap_loops,
        cut_solid: Some(live),
        plane_basis,
        cap_shared_with_model,
    })
}

/// Whether `face` is a plane whose outward normal is `n` and which contains
/// `origin`, to within `band`.
///
/// Both halves matter: the cap's normal points OUT of the kept material, i.e.
/// along `+n̂`, so the opposite face of a zero-thickness residue would not be
/// mistaken for it.
fn face_is_on_plane(
    arena: &BrepArena,
    face: FaceId,
    origin: [f64; 3],
    n: [f64; 3],
    band: f64,
) -> Result<bool, KernelV2Error> {
    let Some(Surface::Plane(p)) = arena.face(face)?.surface else {
        return Ok(false);
    };
    let fnormal = [p.normal.x, p.normal.y, p.normal.z];
    let dot = (0..3).map(|k| fnormal[k] * n[k]).sum::<f64>();
    if dot < 1.0 - cad_primitives::TAU_EVAL {
        return Ok(false);
    }
    let pt = p.point.as_array();
    let off = (0..3).map(|k| (pt[k] - origin[k]) * n[k]).sum::<f64>();
    Ok(off.abs() <= band)
}

/// One cap loop: its curves in B-Rep walk order, with the signed area
/// accumulated from the WALK's direction (which a [`Curve2`] cannot carry —
/// see [`SectionLoop`]).
fn cap_loop(
    arena: &BrepArena,
    lp: crate::arena::LoopId,
    basis: &ViewBasis,
    n: [f64; 3],
    n_seg: u32,
) -> Result<SectionLoop, KernelV2Error> {
    let mut curves = Vec::new();
    let mut signed_area = 0.0;
    let mut exact = true;
    for h in arena.loop_half_edges(lp)? {
        let he = arena.half_edge(h)?;
        let curve = cap_curve(arena, h, basis, n_seg)?;
        exact &= !matches!(curve, Curve2::Polyline { .. });
        // Which direction the WALK runs along the normalized curve. For a
        // conic the question is settled exactly by the 3-D curve's own
        // directional normal: a half-edge traverses counter-clockwise about
        // that normal, a traversal counter-clockwise about `+n̂` is
        // counter-clockwise in `(u, v)` (the basis makes `(u, v, n̂)`
        // right-handed), and `(u, v)` counter-clockwise is the direction a
        // `Curve2` conic's parameter increases in — the normalization
        // `project_circle` and `project_in_plane_ellipse` both apply.
        //
        // The other three arms come back from the projection in the
        // half-edge's OWN direction, by construction: a `LineSegment` as
        // `origin → destination`, a hyperbola or surface-pair curve as
        // `edge_polyline`'s sample run, which starts at the origin. Nothing
        // to recover.
        let forward = match he.curve {
            Curve::Circle { normal, .. }
            | Curve::Arc { normal, .. }
            | Curve::EllipseArc { normal, .. } => {
                normal.x * n[0] + normal.y * n[1] + normal.z * n[2] > 0.0
            }
            Curve::LineSegment | Curve::HyperbolaArc { .. } | Curve::SurfacePair { .. } => true,
        };
        signed_area += green_area(&curve, forward);
        curves.push(curve);
    }
    Ok(SectionLoop {
        curves,
        signed_area,
        exact,
    })
}

/// One cap edge as a 2-D curve in the cut plane's frame.
///
/// The cap lies in the plane, so this map is an isometry and every analytic
/// arm survives — including [`Curve::EllipseArc`], which
/// [`super::project_edge`] samples (see the module docs for why that is right
/// there and wrong here).
fn cap_curve(
    arena: &BrepArena,
    h: crate::arena::HalfEdgeId,
    basis: &ViewBasis,
    n_seg: u32,
) -> Result<Curve2, KernelV2Error> {
    let he = arena.half_edge(h)?;
    if let Curve::EllipseArc {
        center,
        normal,
        major_axis,
        major_radius,
        minor_radius,
    } = he.curve
    {
        let start = arena.vertex(he.origin)?.point;
        let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
        if let Some(c) = project_in_plane_ellipse(
            basis,
            center,
            normal,
            major_axis,
            major_radius,
            minor_radius,
            start,
            end,
        ) {
            return Ok(c);
        }
    }
    super::project_edge(arena, h, basis, n_seg)
}

/// A [`Curve::EllipseArc`] that lies IN the view plane, as an exact
/// [`Curve2::Ellipse`].
///
/// `None` when the arc's own frame or sweep cannot be measured, or when the
/// ellipse is NOT in the plane (its projected minor axis would then be shorter
/// than the stored radius) — the caller falls back to the sample polyline
/// rather than inventing a curve.
#[allow(clippy::too_many_arguments)]
fn project_in_plane_ellipse(
    basis: &ViewBasis,
    center: Point3,
    normal: crate::arena::UnitVector3,
    major_axis: crate::arena::UnitVector3,
    major_radius: f64,
    minor_radius: f64,
    start: Point3,
    end: Point3,
) -> Option<Curve2> {
    let n3 = [normal.x, normal.y, normal.z];
    let e1 = [major_axis.x, major_axis.y, major_axis.z];
    // Minor direction, completing a right-handed frame about `normal` so the
    // arc's counter-clockwise parameter runs `e1 → e2`.
    let e2 = [
        n3[1] * e1[2] - n3[2] * e1[1],
        n3[2] * e1[0] - n3[0] * e1[2],
        n3[0] * e1[1] - n3[1] * e1[0],
    ];
    let a2 = basis.project_dir(e1);
    let b2 = basis.project_dir(e2);
    // In-plane test: both axes must keep unit length through the projection,
    // which only happens when the ellipse's plane IS the view plane.
    let slack = 1e-9;
    if (a2[0].hypot(a2[1]) - 1.0).abs() > slack || (b2[0].hypot(b2[1]) - 1.0).abs() > slack {
        return None;
    }
    // `Curve2::Ellipse` fixes the minor direction as `perp(major) = (−y, x)`;
    // if the ellipse's own minor runs the other way the parameter is mirrored.
    let perp = [-a2[1], a2[0]];
    let sense = if perp[0] * b2[0] + perp[1] * b2[1] >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let t_of = |p: Point3| -> f64 {
        let d = [p.x() - center.x(), p.y() - center.y(), p.z() - center.z()];
        let x = (0..3).map(|k| d[k] * e1[k]).sum::<f64>() / major_radius;
        let y = (0..3).map(|k| d[k] * e2[k]).sum::<f64>() / minor_radius;
        y.atan2(x)
    };
    let t0 = t_of(start);
    let t1raw = t_of(end);
    if !(t0.is_finite() && t1raw.is_finite()) {
        return None;
    }
    // CCW sweep from `t0` to `t1` about `normal`, lifted into `(0, 2π]`; a
    // CLOSED arc (`start == end`, difference 0) is the full turn. `rem_euclid`
    // rather than a `while` that adds `2π`: the correction is arithmetic, not a
    // search, and a loop driven by float data is one bad input from spinning.
    let sweep = match (t1raw - t0).rem_euclid(std::f64::consts::TAU) {
        s if s > 0.0 => s,
        _ => std::f64::consts::TAU,
    };
    let (s0, s1) = {
        let (a, b) = (sense * t0, sense * (t0 + sweep));
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    };
    Some(Curve2::Ellipse {
        center: basis.project(center.as_array()).0,
        major_axis: a2,
        major_radius,
        minor_radius,
        start_param: s0,
        end_param: s1,
    })
}

/// `½∫(u dv − v du)` along `curve`, traversed `forward` or reversed —
/// Green's theorem, so a closed chain's contributions sum to its area.
///
/// Exact in closed form for every analytic arm; a polyline contributes its
/// chord polygon, which is what makes the loop's `exact` flag worth carrying.
fn green_area(curve: &Curve2, forward: bool) -> f64 {
    let sign = if forward { 1.0 } else { -1.0 };
    let cross = |a: Point2, b: Point2| a.x() * b.y() - a.y() * b.x();
    match *curve {
        Curve2::Point(_) => 0.0,
        Curve2::Line { start, end } => sign * 0.5 * cross(start, end),
        Curve2::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            // `p = c + r(cos t, sin t)`, `p × p' = r² + r(c_x cos t + c_y sin t)`.
            let p =
                |t: f64| Point2::new(center.x() + radius * t.cos(), center.y() + radius * t.sin());
            let dt = end_angle - start_angle;
            sign * 0.5 * (radius * radius * dt + cross(center, delta(p(start_angle), p(end_angle))))
        }
        Curve2::Ellipse {
            center,
            major_axis,
            major_radius,
            minor_radius,
            start_param,
            end_param,
        } => {
            // `p = c + a cos t·e₁ + b sin t·e₂` with `e₂ = perp(e₁)`, so
            // `p × p' = cross(c, p') + a·b`.
            let perp = [-major_axis[1], major_axis[0]];
            let p = |t: f64| {
                Point2::new(
                    center.x()
                        + major_radius * t.cos() * major_axis[0]
                        + minor_radius * t.sin() * perp[0],
                    center.y()
                        + major_radius * t.cos() * major_axis[1]
                        + minor_radius * t.sin() * perp[1],
                )
            };
            let dt = end_param - start_param;
            sign * 0.5
                * (major_radius * minor_radius * dt
                    + cross(center, delta(p(start_param), p(end_param))))
        }
        Curve2::Polyline { ref points, closed } => {
            let mut acc = 0.0;
            for w in points.windows(2) {
                acc += 0.5 * cross(w[0], w[1]);
            }
            if closed && points.len() > 2 {
                acc += 0.5 * cross(points[points.len() - 1], points[0]);
            }
            sign * acc
        }
    }
}

fn delta(a: Point2, b: Point2) -> Point2 {
    Point2::new(b.x() - a.x(), b.y() - a.y())
}

/// What a cap loop's own geometry says about itself — the numbers the §5.3
/// section oracle pins.
///
/// A cap loop comes out of a 2-manifold B-Rep face, so it is closed and simple
/// BY CONSTRUCTION; measuring it is how a defect in the projection (a wrong
/// parameter range, a mis-signed sense, a dropped curve) is caught, since the
/// topology would still look right.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoopDefects {
    /// Curve endpoints with no partner within the band among the OTHER
    /// curves' endpoints. A closed chain has none; anything else is an open
    /// loop.
    pub unmatched_ends: u32,
    /// Crossings between two DISTINCT curves of the loop, away from the
    /// endpoint they legitimately share.
    pub self_crossings: u32,
    /// Near-tangential contacts the crossing search declined to resolve —
    /// under-reports of `self_crossings`, counted rather than hidden (the same
    /// contract D1c's [`waffle_types::kernel::projection::ProjectionDeclines`]
    /// carries).
    pub tangency_declines: u32,
    /// Whether the crossing search ran out of its work budget before finishing
    /// — which makes `self_crossings` an UNDER-report rather than a count.
    ///
    /// Reported for the same reason the tangencies are: a loop of thousands of
    /// polyline pieces (a surface-pair cap edge on a gear) could otherwise
    /// exhaust the budget and come back looking clean.
    pub budget_exhausted: bool,
}

/// Measure [`LoopDefects`] of one cap loop, `band` in the loop's own units.
///
/// The crossing search is D1c's ([`super::crossings`]) — the same conic and
/// segment root finder the visibility split uses, so the oracle and the
/// classification cannot disagree about whether two curves meet.
///
/// What it does NOT cover: a single curve crossing ITSELF, which only a
/// [`Curve2::Polyline`] cap edge (a hyperbola or surface-pair boundary) can
/// do. The loop's `exact` flag names those loops, so a caller can say how much
/// of its sample the measurement reaches.
pub fn loop_defects(curves: &[Curve2], band: f64) -> LoopDefects {
    let mut out = LoopDefects::default();
    let ends: Vec<Option<(Point2, Point2)>> = curves.iter().map(|c| c.endpoints()).collect();
    // Closure: every endpoint pairs with an endpoint of another curve. A
    // single-curve loop closes on ITSELF (a full circle or ellipse), which
    // `is_closed` answers directly.
    if curves.len() == 1 {
        if !curves[0].is_closed() {
            out.unmatched_ends = 2;
        }
    } else {
        for (i, e) in ends.iter().enumerate() {
            let Some((a, b)) = *e else { continue };
            for p in [a, b] {
                let matched = ends.iter().enumerate().any(|(j, o)| {
                    j != i
                        && o.is_some_and(|(c, d)| {
                            (p.x() - c.x()).hypot(p.y() - c.y()) <= band
                                || (p.x() - d.x()).hypot(p.y() - d.y()) <= band
                        })
                });
                if !matched {
                    out.unmatched_ends += 1;
                }
            }
        }
    }

    let decomposed: Vec<super::crossings::Decomposed> = curves
        .iter()
        .map(super::crossings::Decomposed::of)
        .collect();
    let mut budget = 1u64 << 24;
    for i in 0..curves.len() {
        for j in (i + 1)..curves.len() {
            if decomposed[i].is_empty() || decomposed[j].is_empty() {
                continue;
            }
            if !super::crossings::boxes_overlap(&decomposed[i].bbox, &decomposed[j].bbox, band) {
                continue;
            }
            let mut hits = Vec::new();
            out.tangency_declines += super::crossings::crossings(
                &decomposed[i],
                &decomposed[j],
                band,
                &mut hits,
                &mut budget,
            );
            for h in hits {
                // A crossing AT a shared endpoint is the chain joining up, not
                // a self-intersection.
                let Some(p) = curves[i].eval(h.a) else {
                    continue;
                };
                let shared = [ends[i], ends[j]].iter().flatten().any(|(a, b)| {
                    [*a, *b]
                        .iter()
                        .any(|q| (p.x() - q.x()).hypot(p.y() - q.y()) <= band)
                });
                if !shared {
                    out.self_crossings += 1;
                }
            }
        }
    }
    out.budget_exhausted = budget == 0;
    out
}

#[cfg(test)]
mod tests;
