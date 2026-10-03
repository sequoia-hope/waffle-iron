//! Orthographic projection of a solid into a view plane — **D1a** of
//! `specs/drawings_and_mbd.md` (§5.2 increment 1), the EDGES; the curved
//! faces' silhouettes are D1b and live in [`silhouette`], which
//! [`project_solid`] appends.
//!
//! The contract lives in [`waffle_types::kernel::projection`]; this is the
//! implementation. One view, one solid, every undirected edge, all tagged
//! [`Visibility::Visible`]: with the silhouettes, the outline a flat-pattern
//! DXF needs, and the floor D1c's hidden-line classification builds on.
//!
//! ## Analytic survival
//!
//! Orthographic projection is a linear map, so it maps each conic in the
//! kernel's vocabulary to a conic of the same or lower rank. D1a keeps the two
//! the spec names:
//!
//! - **A line** projects to a segment, or — when it runs along the line of
//!   sight — to a single point.
//! - **A circle or circular arc** projects to an elliptical arc in general; to
//!   a *circular* arc of the same radius when the circle's plane is
//!   perpendicular to the line of sight; and to a *segment* when the circle's
//!   plane contains it (seen edge-on).
//!
//! The remaining curves ([`Curve::EllipseArc`], [`Curve::HyperbolaArc`],
//! [`Curve::SurfacePair`]) project as polylines at the render chord density,
//! point-for-point identical to [`crate::extract_edges`] — so a drawing view
//! and the viewport's edge overlay never disagree about where an edge is. They
//! are conics too (an ellipse projects to an ellipse), but the spec sets the
//! bar at line and circle for this increment, and a polyline at render density
//! is an honest representation rather than a silently coarser one.
//!
//! ## Where the ellipse comes from
//!
//! A circle of radius `r` about `c` with in-plane frame `(f₁, f₂)` is
//! `P(t) = c + r·cos t·f₁ + r·sin t·f₂`. Projecting is linear, so the image is
//! `p₀ + cos t·a + sin t·b` with `a = Π(r·f₁)`, `b = Π(r·f₂)`, `p₀ = Π(c)`.
//! That is an ellipse whose semi-axes are the singular values of `M = [a b]`
//! and whose axes are its left singular vectors. Rather than a general SVD:
//! the right singular directions are the eigenvectors of `MᵀM`, whose major
//! one is at `α = ½·atan2(2a·b, a·a − b·b)`, and then `w₁ = M·(cos α, sin α)`
//! and `w₂ = M·(−sin α, cos α)` give the semi-axes and their directions with
//! no sign ambiguity to resolve. The circle's own parameter maps to the
//! ellipse's as `τ = s·(t − α)`, where `s = ±1` is whether the circle's
//! counter-clockwise sense survives the projection or is seen from behind.
//!
//! The projected parameter range is normalized counter-clockwise with
//! `start < end`: a projected curve is a point set to draw, and the 3-D edge
//! keeps the traversal direction.

use std::f64::consts::{PI, TAU};

use cad_primitives::{Point2, Point3};
use waffle_types::kernel::projection::{
    Curve2, CurveKind, ProjectedCurve, ProjectionDeclines, ViewBasis, ViewGeometry,
};
use waffle_types::kernel::units::TAU_MODEL;

use crate::arena::{BrepArena, Curve, HalfEdgeId, SolidId, UnitVector3};
use crate::error::KernelV2Error;
use visibility::LiftedCurve;

/// Relative slack at which a projected ellipse's two semi-axes are the same
/// number and the curve is reported as a circle. The exact case (a circle
/// plane perpendicular to the line of sight) lands here on floating-point
/// noise alone; nothing else does, so a near-circular ellipse stays an
/// ellipse rather than being rounded into a circle it is not.
const CIRCULAR_REL_SLACK: f64 = 1e-12;

/// Every undirected edge of `solid`, projected into `basis`.
///
/// Curves come back in the SAME order as [`crate::extract_edges`] — one per
/// canonical (lower-id) half-edge, in half-edge id order — so the two views of
/// an edge set can be zipped, which is what the projection oracle does.
///
/// `rel_chord_tolerance` is the relative chord bound for the curves that
/// project to polylines (see [`crate::tessellate::tessellate_with_chord_tolerance`]).
pub fn project_edges(
    arena: &BrepArena,
    solid: SolidId,
    basis: &ViewBasis,
    rel_chord_tolerance: f64,
) -> Result<ViewGeometry, KernelV2Error> {
    let n_seg = crate::tessellate::circle_segment_count(rel_chord_tolerance);
    Ok(ViewGeometry::new(
        lifted_edges(arena, solid, basis, n_seg)?
            .into_iter()
            .map(|l| l.curve)
            .collect(),
    ))
}

/// Every undirected edge projected, each carrying the 3-D sample polyline of
/// its own source — the input D1c's classification lifts a 2-D point back
/// through. The lift is [`crate::introspect::edge_polyline`], the same
/// sampling [`crate::extract_edges`] and the polyline arm of
/// [`project_edge`] use, so the drawing and the viewport cannot disagree
/// about where an edge is in depth either.
fn lifted_edges(
    arena: &BrepArena,
    solid: SolidId,
    basis: &ViewBasis,
    n_seg: u32,
) -> Result<Vec<LiftedCurve>, KernelV2Error> {
    let he_set = crate::introspect::solid_half_edges(arena, solid)?;
    let mut curves = Vec::with_capacity(he_set.len() / 2);
    for &h in &he_set {
        let he = arena.half_edge(h)?;
        if he.twin < h {
            continue; // the twin (lower id) already reported this edge
        }
        curves.push(LiftedCurve {
            curve: ProjectedCurve::visible(
                project_edge(arena, h, basis, n_seg)?,
                CurveKind::Edge,
                Some(crate::adapter::encode_edge(h)),
            ),
            lift: crate::introspect::edge_polyline(arena, h, n_seg)?,
        });
    }
    Ok(curves)
}

/// One solid's whole view: every edge (D1a) and every curved face's
/// silhouette (D1b), classified visible or hidden against the solid's own
/// tessellation (D1c, [`visibility::classify`]).
///
/// The EDGE curves come first, in [`project_edges`]'s order, and the
/// silhouettes follow in shell walk order — but D1c SPLITS a curve at its
/// crossings and drops the ones that duplicate another, so the result is no
/// longer one curve per edge. D1a's "the nth curve is the nth
/// [`crate::extract_edges`] edge" is therefore a statement about
/// [`project_edges`] from here on; what survives in a classified view is the
/// grouping (a parent's pieces are consecutive, in parameter order) and the
/// `source` on each piece, which still names the edge or face it came from.
pub fn project_solid(
    arena: &BrepArena,
    solid: SolidId,
    basis: &ViewBasis,
    rel_chord_tolerance: f64,
) -> Result<ViewGeometry, KernelV2Error> {
    let n_seg = crate::tessellate::circle_segment_count(rel_chord_tolerance);
    let mut declines = ProjectionDeclines::default();
    let mut lifted = lifted_edges(arena, solid, basis, n_seg)?;
    lifted.extend(silhouette::solid_silhouettes(
        arena,
        solid,
        basis,
        n_seg,
        &mut declines,
    )?);
    let curves = visibility::classify(arena, solid, basis, lifted, n_seg, &mut declines)?;
    Ok(ViewGeometry::with_declines(curves, declines))
}

/// One canonical half-edge's curve, projected. `n_seg` is the angular sample
/// density for the arms that cannot stay analytic.
pub(crate) fn project_edge(
    arena: &BrepArena,
    h: HalfEdgeId,
    basis: &ViewBasis,
    n_seg: u32,
) -> Result<Curve2, KernelV2Error> {
    let he = arena.half_edge(h)?;
    let start = arena.vertex(he.origin)?.point;
    let end = arena.vertex(arena.half_edge(he.next)?.origin)?.point;
    match he.curve {
        Curve::LineSegment => {
            let a = project_point(basis, start);
            let b = project_point(basis, end);
            Ok(line_or_point(a, b))
        }
        Curve::Circle {
            center,
            normal,
            radius,
        } => project_circle(basis, center, normal, radius, start, None)
            .map_or_else(|| polyline(arena, h, basis, n_seg), Ok),
        Curve::Arc {
            center,
            normal,
            radius,
        } => project_circle(basis, center, normal, radius, start, Some(end))
            .map_or_else(|| polyline(arena, h, basis, n_seg), Ok),
        // An ellipse arc, a hyperbola arc and a surface-pair curve all have
        // conic or procedural projections the spec leaves to a later
        // increment; their render-identical sample polyline is the honest
        // answer now.
        Curve::EllipseArc { .. } | Curve::HyperbolaArc { .. } | Curve::SurfacePair { .. } => {
            polyline(arena, h, basis, n_seg)
        }
    }
}

/// The edge's render polyline, projected. Shares
/// [`crate::introspect::edge_polyline`] with [`crate::extract_edges`], so the
/// drawing and the viewport sample the same points.
fn polyline(
    arena: &BrepArena,
    h: HalfEdgeId,
    basis: &ViewBasis,
    n_seg: u32,
) -> Result<Curve2, KernelV2Error> {
    let pts3 = crate::introspect::edge_polyline(arena, h, n_seg)?;
    let mut points: Vec<Point2> = pts3.iter().map(|p| project_point(basis, *p)).collect();
    // `edge_polyline` makes closure explicit by repeating the first point;
    // `Curve2::Polyline` carries it as a flag instead.
    let closed = points.len() > 2 && coincident(points[0], points[points.len() - 1]);
    if closed {
        points.pop();
    }
    Ok(Curve2::Polyline { points, closed })
}

fn project_point(basis: &ViewBasis, p: Point3) -> Point2 {
    basis.project(p.as_array()).0
}

fn coincident(a: Point2, b: Point2) -> bool {
    (a.x() - b.x()).hypot(a.y() - b.y()) <= TAU_MODEL
}

fn line_or_point(a: Point2, b: Point2) -> Curve2 {
    if coincident(a, b) {
        Curve2::Point(a)
    } else {
        Curve2::Line { start: a, end: b }
    }
}

/// The projection of a circle (`end == None`) or a counter-clockwise arc of
/// it, as the analytic curve it is. `None` when the circle's own in-plane
/// frame cannot be derived (a degenerate anchor) or the arc's sweep cannot be
/// measured — the caller then falls back to the sample polyline rather than
/// inventing a curve.
fn project_circle(
    basis: &ViewBasis,
    center: Point3,
    normal: UnitVector3,
    radius: f64,
    start: Point3,
    end: Option<Point3>,
) -> Option<Curve2> {
    let (f1, f2) = crate::tessellate::circle_frame(center, normal, start)?;
    // The arc's own parameter range: `t = 0` at the start vertex, sweeping
    // counter-clockwise about `normal` to the end (a full turn for a circle).
    let sweep = match end {
        None => TAU,
        Some(e) => crate::geom::ccw_sweep(center, [normal.x, normal.y, normal.z], start, e)?,
    };
    let p0 = project_point(basis, center);
    let a = scaled(basis.project_dir(f1), radius);
    let b = scaled(basis.project_dir(f2), radius);

    // Principal direction of MᵀM, M = [a b]: the circle's parameter offset at
    // which the projected ellipse reaches its major axis.
    let aa = dot2(a, a);
    let bb = dot2(b, b);
    let ab = dot2(a, b);
    let alpha = 0.5 * (2.0 * ab).atan2(aa - bb);
    let (sa, ca) = alpha.sin_cos();
    let w1 = [a[0] * ca + b[0] * sa, a[1] * ca + b[1] * sa];
    let w2 = [-a[0] * sa + b[0] * ca, -a[1] * sa + b[1] * ca];
    let r1 = (dot2(w1, w1)).sqrt();
    let r2 = (dot2(w2, w2)).sqrt();

    if !(r1.is_finite() && r2.is_finite()) {
        return None;
    }
    if r1 <= TAU_MODEL {
        // The whole circle lands inside the model tolerance of one point.
        return Some(Curve2::Point(p0));
    }
    let major = [w1[0] / r1, w1[1] / r1];
    // Does the circle's counter-clockwise sense survive the projection, or is
    // the circle seen from behind?
    let sense = if dot2(w2, [-major[1], major[0]]) >= 0.0 {
        1.0
    } else {
        -1.0
    };

    if r2 <= TAU_MODEL {
        // Edge-on: the ellipse has collapsed onto its major axis and the arc
        // covers the `cos τ` range of its parameter interval — exactly, not
        // as a sampling.
        let (t0, t1) = ccw_range(0.0, sweep, alpha, sense);
        let (lo, hi) = cos_range(t0, t1);
        return Some(line_or_point(
            offset(p0, major, r1 * lo),
            offset(p0, major, r1 * hi),
        ));
    }

    if r1 - r2 <= CIRCULAR_REL_SLACK * r1 {
        // The circle's plane is perpendicular to the line of sight, so the
        // projection is a circle of the same radius. Its angles come straight
        // from the projected frame rather than through `alpha`, which is pure
        // noise when the two semi-axes are equal.
        let angle0 = a[1].atan2(a[0]);
        let ccw = a[0] * b[1] - a[1] * b[0] >= 0.0;
        let (start_angle, end_angle) = if ccw {
            (angle0, angle0 + sweep)
        } else {
            (angle0 - sweep, angle0)
        };
        return Some(Curve2::Circle {
            center: p0,
            radius: 0.5 * (r1 + r2),
            start_angle,
            end_angle,
        });
    }

    let (start_param, end_param) = ccw_range(0.0, sweep, alpha, sense);
    Some(Curve2::Ellipse {
        center: p0,
        major_axis: major,
        major_radius: r1,
        minor_radius: r2,
        start_param,
        end_param,
    })
}

/// `[t0, t1]` on the circle mapped to the projected ellipse's parameter and
/// normalized counter-clockwise with `start < end`.
fn ccw_range(t0: f64, t1: f64, alpha: f64, sense: f64) -> (f64, f64) {
    let (a, b) = (sense * (t0 - alpha), sense * (t1 - alpha));
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Exact `(min, max)` of `cos` over `[t0, t1]`.
fn cos_range(t0: f64, t1: f64) -> (f64, f64) {
    let (c0, c1) = (t0.cos(), t1.cos());
    let mut lo = c0.min(c1);
    let mut hi = c0.max(c1);
    if interval_hits_period(t0, t1, 0.0) {
        hi = 1.0;
    }
    if interval_hits_period(t0, t1, PI) {
        lo = -1.0;
    }
    (lo, hi)
}

/// Whether some `base + 2kπ` lies strictly inside `(t0, t1)`.
fn interval_hits_period(t0: f64, t1: f64, base: f64) -> bool {
    let k = ((t0 - base) / TAU).floor();
    for i in [k, k + 1.0, k + 2.0] {
        let t = base + i * TAU;
        if t > t0 && t < t1 {
            return true;
        }
    }
    false
}

fn dot2(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn scaled(v: [f64; 2], s: f64) -> [f64; 2] {
    [v[0] * s, v[1] * s]
}

fn offset(p: Point2, dir: [f64; 2], s: f64) -> Point2 {
    Point2::new(p.x() + dir[0] * s, p.y() + dir[1] * s)
}

pub(crate) mod crossings;
pub mod section;
mod silhouette;
pub(crate) mod visibility;

#[cfg(test)]
mod tests;
