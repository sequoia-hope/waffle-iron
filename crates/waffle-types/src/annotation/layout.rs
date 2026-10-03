//! The layout record the renderers consume — `specs/drawings_and_mbd.md` §7,
//! "both consuming the same layout record the engine emits", and the
//! `ViewGeometry, AnnotationLayout (JSON)` edge of the §3 diagram.
//!
//! An [`Annotation`](super::Annotation) names entities; a renderer needs
//! *geometry*. This module is the handoff: the rebuild resolves each anchor
//! through its `GeomRef`, projects it into the view plane, measures the value,
//! and emits a [`ViewLayout`] — a flat, serde-able record with no `GeomRef`,
//! no kernel handle and no expression left in it. The SVG sheet renderer and
//! the 3-D PMI overlay then each read the same record.
//!
//! ## Why these types mirror `projection::Curve2` instead of reusing it
//!
//! [`crate::kernel::projection::Curve2`] is the kernel's own 2-D curve and is
//! the right type inside Rust. It cannot be serialized, because it is built on
//! `cad_primitives::Point2`, which derives no serde impls — and adding them
//! means editing a crate two layers down the stack for the sake of a
//! consumer two layers up. [`LayoutCurve`] is its serde-able twin over plain
//! `[f64; 2]` pairs, with [`LayoutCurve::from_curve2`] the one conversion and
//! `tests::every_curve2_arm_has_a_layout_twin` the pin that keeps the arms in
//! step. If `Point2` ever gains serde, this type collapses into an alias and
//! the conversion becomes the identity.
//!
//! ## Determinism
//!
//! Everything here is a plain data record built by a pure conversion: same
//! `ViewGeometry` and same annotations in, byte-identical JSON out. That is
//! what lets the app's renderer be a byte oracle (a V3-style response hash)
//! rather than a pixel comparison.

use serde::{Deserialize, Serialize};

use super::{DimensionKind, Placement2};
use crate::kernel::projection::{
    Aabb2, Curve2, CurveKind, ProjectedCurve, ViewGeometry, Visibility,
};

/// A 2-D curve in the view plane, in view `(u, v)` and model units (meters).
///
/// Arm for arm the serde-able twin of [`Curve2`]; see the module docs. Angles
/// run counter-clockwise from `+u` and `start < end`, exactly as there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum LayoutCurve {
    /// A line seen end-on.
    Point {
        at: [f64; 2],
    },
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    /// A full circle when `end_angle - start_angle == 2π`.
    Circle {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    /// A full ellipse when `end_param - start_param == 2π`.
    Ellipse {
        center: [f64; 2],
        major_axis: [f64; 2],
        major_radius: f64,
        minor_radius: f64,
        start_param: f64,
        end_param: f64,
    },
    Polyline {
        points: Vec<[f64; 2]>,
        /// `true` when the last point joins the first (the list does NOT
        /// repeat it).
        closed: bool,
    },
}

impl LayoutCurve {
    /// The serde-able twin of a kernel-side curve.
    pub fn from_curve2(curve: &Curve2) -> LayoutCurve {
        match *curve {
            Curve2::Point(p) => LayoutCurve::Point { at: [p.x(), p.y()] },
            Curve2::Line { start, end } => LayoutCurve::Line {
                start: [start.x(), start.y()],
                end: [end.x(), end.y()],
            },
            Curve2::Circle {
                center,
                radius,
                start_angle,
                end_angle,
            } => LayoutCurve::Circle {
                center: [center.x(), center.y()],
                radius,
                start_angle,
                end_angle,
            },
            Curve2::Ellipse {
                center,
                major_axis,
                major_radius,
                minor_radius,
                start_param,
                end_param,
            } => LayoutCurve::Ellipse {
                center: [center.x(), center.y()],
                major_axis,
                major_radius,
                minor_radius,
                start_param,
                end_param,
            },
            Curve2::Polyline { ref points, closed } => LayoutCurve::Polyline {
                points: points.iter().map(|p| [p.x(), p.y()]).collect(),
                closed,
            },
        }
    }

    /// The single point this curve contributes when a dimension needs one
    /// place to measure from: the point itself, a line's midpoint, a
    /// circle's or ellipse's centre.
    ///
    /// `None` for a [`LayoutCurve::Polyline`], deliberately. A sampled curve
    /// has no canonical witness — its midpoint depends on the chord tolerance
    /// that sampled it, so a dimension anchored to one would read a different
    /// number at a different render density. [`super::measure::measure`]
    /// refuses instead.
    pub fn witness_point(&self) -> Option<[f64; 2]> {
        match self {
            LayoutCurve::Point { at } => Some(*at),
            LayoutCurve::Line { start, end } => {
                Some([0.5 * (start[0] + end[0]), 0.5 * (start[1] + end[1])])
            }
            LayoutCurve::Circle { center, .. } | LayoutCurve::Ellipse { center, .. } => {
                Some(*center)
            }
            LayoutCurve::Polyline { .. } => None,
        }
    }

    /// The radius this curve dimensions, for [`DimensionKind::Radius`] and
    /// [`DimensionKind::Diameter`]: a circle's radius, or an ellipse's
    /// **major** radius.
    ///
    /// `None` for the arms that have no radius. An ellipse is included
    /// because a circular hole seen obliquely projects to one, and its major
    /// radius is the hole's true radius — which is the number a radial
    /// dimension on that hole must read.
    pub fn radius(&self) -> Option<f64> {
        match self {
            LayoutCurve::Circle { radius, .. } => Some(*radius),
            LayoutCurve::Ellipse { major_radius, .. } => Some(*major_radius),
            LayoutCurve::Point { .. } | LayoutCurve::Line { .. } | LayoutCurve::Polyline { .. } => {
                None
            }
        }
    }

    /// The direction this curve runs, unit length, for the kinds that have
    /// one. `None` for everything but a line, and for a zero-length line.
    pub fn direction(&self) -> Option<[f64; 2]> {
        let LayoutCurve::Line { start, end } = self else {
            return None;
        };
        let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 || !len.is_finite() {
            return None;
        }
        Some([dx / len, dy / len])
    }
}

/// What one annotation anchor resolved to in the view plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AnchorGeometry {
    /// A vertex, or a face/edge reduced to one representative point.
    Point { at: [f64; 2] },
    /// An edge or a silhouette, as projected.
    Curve { curve: LayoutCurve },
}

impl AnchorGeometry {
    /// A point anchor at `at`.
    pub fn point(at: [f64; 2]) -> AnchorGeometry {
        AnchorGeometry::Point { at }
    }

    /// A curve anchor.
    pub fn curve(curve: LayoutCurve) -> AnchorGeometry {
        AnchorGeometry::Curve { curve }
    }

    /// The one place to measure from — see [`LayoutCurve::witness_point`].
    pub fn witness_point(&self) -> Option<[f64; 2]> {
        match self {
            AnchorGeometry::Point { at } => Some(*at),
            AnchorGeometry::Curve { curve } => curve.witness_point(),
        }
    }

    /// The curve, when this anchor is one.
    pub fn as_curve(&self) -> Option<&LayoutCurve> {
        match self {
            AnchorGeometry::Curve { curve } => Some(curve),
            AnchorGeometry::Point { .. } => None,
        }
    }
}

/// One annotation, resolved and measured, ready to draw.
///
/// The `GeomRef`s of [`Annotation`](super::Annotation) have become geometry
/// and the [`Measured`](super::Measured) has become a number. A renderer that
/// reads this cannot reach the model, which is the point: it has no way to
/// draw a value other than the measured one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AnnotationLayout {
    Dimension {
        kind: DimensionKind,
        /// The resolved anchors, in the order `kind` expects.
        anchors: Vec<AnchorGeometry>,
        /// The measured value — meters, or radians when
        /// [`DimensionKind::is_angular`].
        value: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        precision: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dual_unit: Option<String>,
        #[serde(default)]
        placement: Placement2,
    },
    Note {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        leader: Option<AnchorGeometry>,
        #[serde(default)]
        placement: Placement2,
    },
    CentreMark {
        at: [f64; 2],
        /// Half-length of each arm of the cross, meters. The layout derives it
        /// from the marked entity's own size so a tiny hole does not get a
        /// cross the size of the part.
        half_size: f64,
    },
    CentreLine {
        from: [f64; 2],
        to: [f64; 2],
    },
    Datum {
        label: String,
        anchor: AnchorGeometry,
        #[serde(default)]
        placement: Placement2,
    },
}

/// One projected curve of the view, serde-able.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LayoutCurveEntry {
    pub geometry: LayoutCurve,
    pub visibility: Visibility,
    pub kind: CurveKind,
}

impl LayoutCurveEntry {
    /// The serde-able twin of a kernel-side projected curve. The `source`
    /// `KernelId` is deliberately dropped: a renderer must not be able to
    /// reach back into the kernel, and a resolved layout record is the whole
    /// of what it needs.
    pub fn from_projected(curve: &ProjectedCurve) -> LayoutCurveEntry {
        LayoutCurveEntry {
            geometry: LayoutCurve::from_curve2(&curve.geometry),
            visibility: curve.visibility,
            kind: curve.kind,
        }
    }
}

/// Everything the app needs to draw one annotated view: the projected curves
/// and the resolved annotations, in one view frame, in meters.
///
/// The sheet's own paper-space transform — the view scale and where on the
/// sheet it sits — is D4a's and is deliberately not here. This record is the
/// view's *contents*, in the view's own coordinates.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ViewLayout {
    pub curves: Vec<LayoutCurveEntry>,
    /// `[min, max]` over every curve; `None` when there are none (an
    /// empty-but-present box is a lie — the same call
    /// [`ViewGeometry::bbox`] makes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[[f64; 2]; 2]>,
    #[serde(default)]
    pub annotations: Vec<AnnotationLayout>,
}

impl ViewLayout {
    /// The curves of `view`, with no annotations yet.
    pub fn from_view(view: &ViewGeometry) -> ViewLayout {
        ViewLayout {
            curves: view
                .curves
                .iter()
                .map(LayoutCurveEntry::from_projected)
                .collect(),
            bbox: view.bbox.map(aabb_pair),
            annotations: Vec::new(),
        }
    }

    /// This layout with `annotations` attached — the form the rebuild emits.
    pub fn with_annotations(mut self, annotations: Vec<AnnotationLayout>) -> ViewLayout {
        self.annotations = annotations;
        self
    }
}

fn aabb_pair(bbox: Aabb2) -> [[f64; 2]; 2] {
    [[bbox.min.x(), bbox.min.y()], [bbox.max.x(), bbox.max.y()]]
}

#[cfg(test)]
mod tests;
