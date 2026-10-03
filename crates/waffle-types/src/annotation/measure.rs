//! A dimension's value, computed from its anchors' geometry —
//! `specs/drawings_and_mbd.md` §7, the half of the model that makes a
//! dimension *measured* rather than typed.
//!
//! One entry point, [`measure`]: a [`DimensionKind`] plus the resolved
//! anchors in, one number out — meters, or radians for
//! [`DimensionKind::Angle`]. It is a pure function of its inputs with no
//! tolerance knob, so the same view measures the same number every time.
//!
//! ## It refuses rather than guesses
//!
//! Every case where the question has no single answer is a typed
//! [`MeasureError`], never a nearest-interpretation:
//!
//! - **Two non-parallel lines have no "distance".** There are infinitely many
//!   point pairs between them and they measure everything from zero up, so
//!   [`DimensionKind::Distance`] over a non-parallel pair is
//!   [`MeasureError::AnchorsNotParallel`]. A user who wants a specific gap
//!   dimensions the two vertices, which *is* a single answer.
//! - **A sampled polyline has no witness point.** Its midpoint moves with the
//!   chord tolerance that sampled it, so a dimension on one would read a
//!   different number at a different render density
//!   ([`MeasureError::NotMeasurable`]).
//! - **Only a circle or an ellipse has a radius**
//!   ([`MeasureError::NotMeasurable`]).
//!
//! This is the §"Fix It Right or Don't Fix It" posture applied to a number a
//! machinist will cut to: a loud refusal is recoverable, a plausible wrong
//! dimension is not.
//!
//! ## Parallelism is tested on direction, not on distance
//!
//! Two lines count as parallel when `|cross(d₁, d₂)| ≤` [`PARALLEL_SIN_TOL`],
//! i.e. when the angle between them is at most about 0.0057° — a test on the
//! *sine of the angle*, so it does not scale with the lines' length or with
//! how far apart they are. `TAU_MODEL` (1e-7 m) is a length tolerance and
//! would mean something different on a 1 mm edge than on a 1 m one.

use super::layout::{AnchorGeometry, LayoutCurve};
use super::{DimensionKind, OrdinateAxis};

/// How near-parallel two lines must be for an aligned distance between them
/// to have one answer: `|sin θ| ≤ 1e-7`, about 0.0057°.
///
/// Numerically the same figure as `TAU_MODEL`, and deliberately a different
/// quantity — this one is dimensionless. Picked to match the kernel's
/// modelling tolerance in spirit: two faces the kernel would call parallel
/// should dimension, and two it would not should refuse.
pub const PARALLEL_SIN_TOL: f64 = 1e-7;

/// Why a dimension could not be measured.
///
/// Each arm is a question with no single answer; see the module docs.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeasureError {
    /// The annotation holds the wrong number of anchors for its kind.
    #[error("a {kind} dimension measures {expected} anchor(s), got {got}")]
    WrongArity {
        kind: &'static str,
        expected: usize,
        got: usize,
    },
    /// The anchors are the wrong sort of geometry for this kind — a radius on
    /// a straight edge, a point-to-line distance whose second anchor is not a
    /// line, a dimension on a sampled polyline.
    #[error("a {kind} dimension cannot measure this geometry: {why}")]
    NotMeasurable {
        kind: &'static str,
        why: &'static str,
    },
    /// An aligned distance between two lines that are not parallel. There is
    /// no single distance between them.
    #[error(
        "an aligned distance between two lines needs them parallel; these cross at {degrees:.4}° \
         — dimension the vertices instead"
    )]
    AnchorsNotParallel { degrees: f64 },
    /// A degenerate anchor: a zero-length line, a zero-radius circle, a
    /// non-finite coordinate.
    #[error("a {kind} dimension cannot measure a degenerate anchor: {why}")]
    Degenerate {
        kind: &'static str,
        why: &'static str,
    },
}

/// The value of a dimension of `kind` over `anchors`, in meters — or radians
/// when [`DimensionKind::is_angular`].
///
/// Anchors are in **view-plane `(u, v)`**: a dimension measures what the
/// drawing shows, which is the projection. A 3-D PMI dimension (M2) measures
/// in the annotation plane the same way, by resolving its anchors into that
/// plane first.
pub fn measure(kind: DimensionKind, anchors: &[AnchorGeometry]) -> Result<f64, MeasureError> {
    let name = kind_name(kind);
    if anchors.len() != kind.arity() {
        return Err(MeasureError::WrongArity {
            kind: name,
            expected: kind.arity(),
            got: anchors.len(),
        });
    }
    let value = match kind {
        DimensionKind::Distance => aligned_distance(name, &anchors[0], &anchors[1])?,
        DimensionKind::PointLineDistance => {
            let p = witness(name, &anchors[0])?;
            let line = anchors[1].as_curve().ok_or(MeasureError::NotMeasurable {
                kind: name,
                why: "the second anchor must be a line",
            })?;
            point_line_distance(name, p, line)?
        }
        DimensionKind::HDistance | DimensionKind::VDistance => {
            let a = witness(name, &anchors[0])?;
            let b = witness(name, &anchors[1])?;
            let axis = usize::from(kind == DimensionKind::VDistance);
            (b[axis] - a[axis]).abs()
        }
        DimensionKind::Angle => angle_between(name, &anchors[0], &anchors[1])?,
        DimensionKind::Radius => radius_of(name, &anchors[0])?,
        DimensionKind::Diameter => 2.0 * radius_of(name, &anchors[0])?,
        DimensionKind::Ordinate { axis } => {
            let p = witness(name, &anchors[0])?;
            p[match axis {
                OrdinateAxis::U => 0,
                OrdinateAxis::V => 1,
            }]
        }
    };
    if !value.is_finite() {
        return Err(MeasureError::Degenerate {
            kind: name,
            why: "a coordinate was not finite",
        });
    }
    Ok(value)
}

/// The single point an anchor measures from, or a typed refusal.
fn witness(kind: &'static str, anchor: &AnchorGeometry) -> Result<[f64; 2], MeasureError> {
    anchor.witness_point().ok_or(MeasureError::NotMeasurable {
        kind,
        why:
            "a sampled polyline has no witness point — its midpoint moves with the chord tolerance",
    })
}

/// Aligned distance. Two parallel lines measure across the gap between them;
/// anything else measures between the two anchors' witness points.
fn aligned_distance(
    kind: &'static str,
    a: &AnchorGeometry,
    b: &AnchorGeometry,
) -> Result<f64, MeasureError> {
    if let (Some(ca), Some(cb)) = (a.as_curve(), b.as_curve()) {
        if let (Some(da), Some(db)) = (ca.direction(), cb.direction()) {
            let cross = (da[0] * db[1] - da[1] * db[0]).abs();
            if cross > PARALLEL_SIN_TOL {
                return Err(MeasureError::AnchorsNotParallel {
                    degrees: cross.clamp(-1.0, 1.0).asin().to_degrees(),
                });
            }
            // Parallel: the gap is the perpendicular distance from any point
            // of one to the other, which is what a drafter's linear dimension
            // between two walls reads.
            return point_line_distance(kind, witness(kind, b)?, ca);
        }
    }
    let pa = witness(kind, a)?;
    let pb = witness(kind, b)?;
    Ok(((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2)).sqrt())
}

/// Perpendicular distance from `p` to the infinite line through `line`.
fn point_line_distance(
    kind: &'static str,
    p: [f64; 2],
    line: &LayoutCurve,
) -> Result<f64, MeasureError> {
    let LayoutCurve::Line { start, .. } = line else {
        return Err(MeasureError::NotMeasurable {
            kind,
            why: "the line anchor must be a straight edge",
        });
    };
    let dir = line.direction().ok_or(MeasureError::Degenerate {
        kind,
        why: "a zero-length line has no direction",
    })?;
    let (vx, vy) = (p[0] - start[0], p[1] - start[1]);
    Ok((vx * dir[1] - vy * dir[0]).abs())
}

/// Angle between two lines, in radians, in `[0, π/2]`.
///
/// Reported as the **acute** angle between the two directions. A projected
/// edge is an undirected point set (`projection::Curve2`'s own contract:
/// "traversal direction is deliberately not preserved"), so its direction is
/// known only up to sign and the obtuse supplement is not distinguishable
/// from the acute one. A drafter who wants the supplement reads it off the
/// drawing; inventing one here would mean picking a sign the projection does
/// not carry.
fn angle_between(
    kind: &'static str,
    a: &AnchorGeometry,
    b: &AnchorGeometry,
) -> Result<f64, MeasureError> {
    let dirs: Vec<[f64; 2]> = [a, b]
        .iter()
        .map(|anchor| {
            anchor
                .as_curve()
                .and_then(LayoutCurve::direction)
                .ok_or(MeasureError::NotMeasurable {
                    kind,
                    why: "an angular dimension measures two straight edges",
                })
        })
        .collect::<Result<_, _>>()?;
    let dot = (dirs[0][0] * dirs[1][0] + dirs[0][1] * dirs[1][1]).abs();
    Ok(dot.clamp(-1.0, 1.0).acos())
}

/// Radius of a circular or elliptical anchor.
fn radius_of(kind: &'static str, anchor: &AnchorGeometry) -> Result<f64, MeasureError> {
    let r = anchor
        .as_curve()
        .and_then(LayoutCurve::radius)
        .ok_or(MeasureError::NotMeasurable {
            kind,
            why: "only a circle, arc or ellipse has a radius",
        })?;
    if r <= 0.0 || !r.is_finite() {
        return Err(MeasureError::Degenerate {
            kind,
            why: "the radius is zero or not finite",
        });
    }
    Ok(r)
}

/// The name used in a [`MeasureError`]. Kept next to the match it serves so a
/// new [`DimensionKind`] cannot be added without naming it.
fn kind_name(kind: DimensionKind) -> &'static str {
    match kind {
        DimensionKind::Distance => "Distance",
        DimensionKind::PointLineDistance => "PointLineDistance",
        DimensionKind::HDistance => "HDistance",
        DimensionKind::VDistance => "VDistance",
        DimensionKind::Angle => "Angle",
        DimensionKind::Radius => "Radius",
        DimensionKind::Diameter => "Diameter",
        DimensionKind::Ordinate { .. } => "Ordinate",
    }
}

#[cfg(test)]
mod tests;
