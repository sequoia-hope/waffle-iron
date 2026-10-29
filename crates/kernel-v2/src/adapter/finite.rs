//! The adapter's finiteness gate (2026-10-28 review §1.4, remediation Q11).
//!
//! Every [`super::KernelV2Adapter`] entry that takes caller geometry —
//! points, vectors, distances, angles, placements, profiles, view frames —
//! runs its numbers through here BEFORE any arena work, and a NaN or an
//! infinity comes back as a typed [`KernelError::InvalidInput`] naming the
//! argument.
//!
//! Why at the boundary and not in the predicates: the exact predicates in
//! `cherchi-rs` read a NaN as sign `Zero` (it compares neither above nor below
//! zero) and then panic converting it to a rational, so a NaN that reaches one
//! kills the engine — and in WASM, the page. A predicate is the wrong place to
//! decide what a NaN MEANT; the caller who sent it is the only one who knows,
//! and the only one who can fix it. Several inner constructors already refuse
//! a NaN of their own (`extrude`'s depth, `Profile::new`, `transform_solid`,
//! `revolve`'s angle, `ingest`'s vertices), each as an untyped message; this
//! gate makes the refusal one typed answer at every door, ahead of all of
//! them, so a consumer can branch on it (`ErrorKind::InvalidParameter`) and a
//! door that has no inner check of its own is covered too.
//!
//! Each check refuses input that has no geometric meaning at all; none of them
//! is a tolerance, and none of them changes what a finite input does.

use waffle_types::kernel::{
    AnalyticCurve, AnalyticShellData, AnalyticSurface, ClosedProfile, ImportedBodyData,
    KernelError, PipePathSegment, RigidPlacement, SweepSection, ViewFrame,
};
use waffle_types::sketch3d::{Chain3d, Edge3dKind};
use waffle_types::{Region, RegionEdge};

/// The refusal: `argument` is the caller-facing name of the number (or the
/// path into a structure, `profiles[0].circle.radius`).
pub(super) fn non_finite(argument: impl Into<String>, value: impl std::fmt::Debug) -> KernelError {
    KernelError::InvalidInput {
        argument: argument.into(),
        reason: format!("{value:?} is not a finite number"),
    }
}

/// One number.
pub(super) fn scalar(argument: &str, x: f64) -> Result<(), KernelError> {
    if x.is_finite() {
        Ok(())
    } else {
        Err(non_finite(argument, x))
    }
}

/// An optional number: `None` is not a number and passes.
pub(super) fn opt_scalar(argument: &str, x: Option<f64>) -> Result<(), KernelError> {
    match x {
        Some(x) => scalar(argument, x),
        None => Ok(()),
    }
}

/// A point or a vector in model space.
pub(super) fn vec3(argument: &str, v: [f64; 3]) -> Result<(), KernelError> {
    if v.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(non_finite(argument, v))
    }
}

/// A point in a sketch plane's `(u, v)`.
pub(super) fn uv(argument: &str, p: (f64, f64)) -> Result<(), KernelError> {
    if p.0.is_finite() && p.1.is_finite() {
        Ok(())
    } else {
        Err(non_finite(argument, p))
    }
}

/// A polyline or polygon in `(u, v)`.
fn uv_loop(argument: &str, pts: &[(f64, f64)]) -> Result<(), KernelError> {
    for (i, &p) in pts.iter().enumerate() {
        uv(&format!("{argument}[{i}]"), p)?;
    }
    Ok(())
}

/// A rigid placement: its translation and every row of its rotation.
pub(super) fn placement(argument: &str, p: &RigidPlacement) -> Result<(), KernelError> {
    vec3(&format!("{argument}.translation"), p.translation)?;
    for (i, row) in p.rotation.iter().enumerate() {
        vec3(&format!("{argument}.rotation[{i}]"), *row)?;
    }
    Ok(())
}

/// An optional placement (`None` is the identity, which is finite).
pub(super) fn opt_placement(argument: &str, p: Option<&RigidPlacement>) -> Result<(), KernelError> {
    match p {
        Some(p) => placement(argument, p),
        None => Ok(()),
    }
}

/// A `cad_primitives` point or vector, by its three coordinates.
fn xyz(argument: &str, x: f64, y: f64, z: f64) -> Result<(), KernelError> {
    vec3(argument, [x, y, z])
}

/// One segment of a pipe's planar path.
pub(super) fn pipe_segment(argument: &str, seg: &PipePathSegment) -> Result<(), KernelError> {
    match *seg {
        PipePathSegment::Line { a, b } => {
            uv(&format!("{argument}.a"), a)?;
            uv(&format!("{argument}.b"), b)
        }
        PipePathSegment::Arc {
            a,
            b,
            center,
            radius,
            ..
        } => {
            uv(&format!("{argument}.a"), a)?;
            uv(&format!("{argument}.b"), b)?;
            uv(&format!("{argument}.center"), center)?;
            scalar(&format!("{argument}.radius"), radius)
        }
    }
}

/// A sweep's section: its plane frame and its polygon loops.
pub(super) fn sweep_section(argument: &str, s: &SweepSection) -> Result<(), KernelError> {
    vec3(&format!("{argument}.plane_origin"), s.plane_origin)?;
    vec3(&format!("{argument}.plane_normal"), s.plane_normal)?;
    vec3(&format!("{argument}.plane_x_axis"), s.plane_x_axis)?;
    uv_loop(&format!("{argument}.outer"), &s.outer)?;
    for (i, hole) in s.holes.iter().enumerate() {
        uv_loop(&format!("{argument}.holes[{i}]"), hole)?;
    }
    Ok(())
}

/// A 3D path: every edge's endpoints and, for an arc, its circle.
pub(super) fn chain3d(argument: &str, path: &Chain3d) -> Result<(), KernelError> {
    for (i, e) in path.edges.iter().enumerate() {
        let at = format!("{argument}.edges[{i}]");
        vec3(&format!("{at}.a"), e.a)?;
        vec3(&format!("{at}.b"), e.b)?;
        if let Edge3dKind::Arc {
            center,
            normal,
            radius,
        } = e.kind
        {
            vec3(&format!("{at}.center"), center)?;
            vec3(&format!("{at}.normal"), normal)?;
            scalar(&format!("{at}.radius"), radius)?;
        }
    }
    Ok(())
}

/// A closed profile's own numbers: its circle, its arcs' circles and its
/// spline control points. Its polygon vertices live in the caller's
/// `positions` map and are checked where they are read.
pub(super) fn closed_profile(argument: &str, p: &ClosedProfile) -> Result<(), KernelError> {
    if let Some(c) = &p.circle {
        uv(
            &format!("{argument}.circle.center"),
            (c.center_u, c.center_v),
        )?;
        scalar(&format!("{argument}.circle.radius"), c.radius)?;
    }
    for (i, a) in p.arc_segments.iter().enumerate() {
        uv(
            &format!("{argument}.arc_segments[{i}].center"),
            (a.center_u, a.center_v),
        )?;
        scalar(&format!("{argument}.arc_segments[{i}].radius"), a.radius)?;
    }
    for (i, sp) in p.spline_segments.iter().enumerate() {
        uv_loop(
            &format!("{argument}.spline_segments[{i}].control_points"),
            &sp.control_points,
        )?;
    }
    Ok(())
}

/// One edge of a region boundary.
fn region_edge(argument: &str, e: &RegionEdge) -> Result<(), KernelError> {
    match *e {
        RegionEdge::Line { a, b } => {
            uv(&format!("{argument}.a"), a)?;
            uv(&format!("{argument}.b"), b)
        }
        RegionEdge::Arc {
            a,
            b,
            center,
            radius,
            ..
        } => {
            uv(&format!("{argument}.a"), a)?;
            uv(&format!("{argument}.b"), b)?;
            uv(&format!("{argument}.center"), center)?;
            scalar(&format!("{argument}.radius"), radius)
        }
    }
}

/// A sketch region: its polygon loops and its recovered boundary edges.
/// (`area` is a derived report the kernel does not read.)
pub(super) fn region(argument: &str, r: &Region) -> Result<(), KernelError> {
    uv_loop(&format!("{argument}.outer"), &r.outer)?;
    for (i, hole) in r.holes.iter().enumerate() {
        uv_loop(&format!("{argument}.holes[{i}]"), hole)?;
    }
    for (i, e) in r.outer_edges.iter().enumerate() {
        region_edge(&format!("{argument}.outer_edges[{i}]"), e)?;
    }
    for (h, edges) in r.hole_edges.iter().enumerate() {
        for (i, e) in edges.iter().enumerate() {
            region_edge(&format!("{argument}.hole_edges[{h}][{i}]"), e)?;
        }
    }
    Ok(())
}

/// A projection's view frame.
pub(super) fn view_frame(argument: &str, v: &ViewFrame) -> Result<(), KernelError> {
    vec3(&format!("{argument}.origin"), v.origin)?;
    vec3(&format!("{argument}.dir"), v.dir)?;
    vec3(&format!("{argument}.up"), v.up)
}

/// A mesh-backed import: every vertex position and every edge polyline point.
///
/// Normals are NOT checked: they shade the render and nothing else reads
/// them, and a degenerate source triangle legitimately has no normal — a
/// refusal there would turn a drawable import into a failed one over a number
/// no computation uses.
pub(super) fn imported_body(argument: &str, data: &ImportedBodyData) -> Result<(), KernelError> {
    for (s, shell) in data.shells.iter().enumerate() {
        for (f, face) in shell.faces.iter().enumerate() {
            if let Some((i, x)) = face
                .positions
                .iter()
                .enumerate()
                .find(|(_, x)| !x.is_finite())
            {
                return Err(non_finite(
                    format!("{argument}.shells[{s}].faces[{f}].positions[{i}]"),
                    x,
                ));
            }
        }
        for (e, edge) in shell.edges.iter().enumerate() {
            for (i, p) in edge.polyline.iter().enumerate() {
                vec3(
                    &format!("{argument}.shells[{s}].edges[{e}].polyline[{i}]"),
                    *p,
                )?;
            }
        }
    }
    Ok(())
}

/// An exact analytic shell: every vertex, curve and surface number.
pub(super) fn analytic_shell(argument: &str, shell: &AnalyticShellData) -> Result<(), KernelError> {
    for (i, p) in shell.vertices.iter().enumerate() {
        xyz(&format!("{argument}.vertices[{i}]"), p.x(), p.y(), p.z())?;
    }
    for (i, e) in shell.edges.iter().enumerate() {
        let at = format!("{argument}.edges[{i}].curve");
        match e.curve {
            AnalyticCurve::Line => {}
            AnalyticCurve::Circle {
                center,
                normal,
                radius,
                interior,
            } => {
                xyz(&format!("{at}.center"), center.x(), center.y(), center.z())?;
                xyz(&format!("{at}.normal"), normal.x(), normal.y(), normal.z())?;
                scalar(&format!("{at}.radius"), radius)?;
                xyz(
                    &format!("{at}.interior"),
                    interior.x(),
                    interior.y(),
                    interior.z(),
                )?;
            }
            AnalyticCurve::Ellipse {
                center,
                normal,
                major_axis,
                major_radius,
                minor_radius,
                interior,
            } => {
                xyz(&format!("{at}.center"), center.x(), center.y(), center.z())?;
                xyz(&format!("{at}.normal"), normal.x(), normal.y(), normal.z())?;
                xyz(
                    &format!("{at}.major_axis"),
                    major_axis.x(),
                    major_axis.y(),
                    major_axis.z(),
                )?;
                scalar(&format!("{at}.major_radius"), major_radius)?;
                scalar(&format!("{at}.minor_radius"), minor_radius)?;
                xyz(
                    &format!("{at}.interior"),
                    interior.x(),
                    interior.y(),
                    interior.z(),
                )?;
            }
        }
    }
    for (i, f) in shell.faces.iter().enumerate() {
        let at = format!("{argument}.faces[{i}].surface");
        match f.surface {
            AnalyticSurface::Plane { origin, normal } => {
                xyz(&format!("{at}.origin"), origin.x(), origin.y(), origin.z())?;
                xyz(&format!("{at}.normal"), normal.x(), normal.y(), normal.z())?;
            }
            AnalyticSurface::Cylinder {
                axis_point,
                axis_dir,
                radius,
            } => {
                xyz(
                    &format!("{at}.axis_point"),
                    axis_point.x(),
                    axis_point.y(),
                    axis_point.z(),
                )?;
                xyz(
                    &format!("{at}.axis_dir"),
                    axis_dir.x(),
                    axis_dir.y(),
                    axis_dir.z(),
                )?;
                scalar(&format!("{at}.radius"), radius)?;
            }
            AnalyticSurface::Cone {
                apex,
                axis_dir,
                half_angle,
            } => {
                xyz(&format!("{at}.apex"), apex.x(), apex.y(), apex.z())?;
                xyz(
                    &format!("{at}.axis_dir"),
                    axis_dir.x(),
                    axis_dir.y(),
                    axis_dir.z(),
                )?;
                scalar(&format!("{at}.half_angle"), half_angle)?;
            }
            AnalyticSurface::Sphere { center, radius } => {
                xyz(&format!("{at}.center"), center.x(), center.y(), center.z())?;
                scalar(&format!("{at}.radius"), radius)?;
            }
            AnalyticSurface::Torus {
                center,
                axis_dir,
                major_radius,
                minor_radius,
            } => {
                xyz(&format!("{at}.center"), center.x(), center.y(), center.z())?;
                xyz(
                    &format!("{at}.axis_dir"),
                    axis_dir.x(),
                    axis_dir.y(),
                    axis_dir.z(),
                )?;
                scalar(&format!("{at}.major_radius"), major_radius)?;
                scalar(&format!("{at}.minor_radius"), minor_radius)?;
            }
        }
    }
    Ok(())
}
