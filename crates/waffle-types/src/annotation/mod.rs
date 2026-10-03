//! Annotations — dimensions, notes, centre marks, datums
//! (`specs/drawings_and_mbd.md` §7, increment D3).
//!
//! One `Annotation` enum serves both consumers the spec names: the drawing
//! tab's `annotations` list (D4a, anchors scoped to a view source) and the
//! part tree's `Pmi` feature (M2, anchors in the part's own scope). Nothing
//! here renders and nothing here measures text — per spec §3, "Rust produces
//! curves and numbers; the app draws them".
//!
//! ## The three pieces
//!
//! | module | what it holds | who writes it |
//! |---|---|---|
//! | this one | the **document model**: what the user authored | the UI / MCP |
//! | [`measure`] | the **value**, computed from geometry | the rebuild |
//! | [`layout`] | the **layout record** the app's renderer consumes | the rebuild |
//!
//! A dimension's number is never typed in. The model says *which entities*
//! and *how to measure them* ([`DimensionKind`]); [`measure::measure`] turns
//! the anchors' resolved geometry into the number; the layout record carries
//! that number plus the resolved anchor geometry out to the renderer. A
//! document therefore cannot hold a dimension that disagrees with its model —
//! the usual CAD failure where a drawing says 40 and the part is 38.
//!
//! ## Anchors are persistent ids
//!
//! Every anchor is a [`GeomRef`], and the selector a writer should use is
//! [`crate::geom_ref::Selector::Pid`] (D0, landed 2026-10-03), whose
//! defining property is that it **never rebinds**: an annotation whose entity
//! is gone fails loudly instead of silently dimensioning a neighbour. See
//! that variant's docs for why a `Signature` fallback is the wrong answer
//! here. Edge and vertex pids come from
//! [`crate::kernel::KernelIntrospect::entity_pid`]; face pids are
//! content-seeded from the creating feature.
//!
//! ## Units
//!
//! Lengths are **meters** and angles are **radians**, matching the kernel and
//! [`crate::kernel::projection`]. [`Dimension::precision`] and
//! `dual_unit` describe how the app should *display* that number; the
//! conversion itself happens at the UI boundary (`app/src/lib/units.js`), not
//! here.
//!
//! ## Deviations from the §7 sketch, and why
//!
//! - **[`Measured`] has a third arm, [`Measured::FromGeometry`], and it is the
//!   default.** §7 sketches `enum Measured { Expr(String), Value(f64) }`. With
//!   only those two, the ordinary case — "this dimension is however wide the
//!   part is" — has to be written either as a synthesized expression string
//!   (`distance(…)` naming anchors the annotation already holds, so two
//!   sources of truth for the same reference) or as `Value(f64)`, which is
//!   exactly the typed-in number the spec forbids. `FromGeometry` names the
//!   ordinary case directly; `Expr` stays for a derived value (D2's measure
//!   functions, a title-block `mass(part)`), and `Value` is documented as a
//!   cache / imported nominal, not something the UI offers.
//! - **No `tolerance` field and no `FeatureControlFrame` variant.** Both need
//!   M1's `Tolerance` / `GeometricTolerance`, which this increment does not
//!   invent. Adding the field later is additive (`#[serde(default)]`); adding
//!   the variant is not — see the note on persistence below.
//! - **`Dimension` carries `anchors: Vec<GeomRef>`, as §7 says, and
//!   [`DimensionKind::arity`] states what each kind needs.** A fixed-size
//!   array per kind would be tighter, but the kinds disagree on arity (one
//!   anchor for a radius, two for a distance) and the file format wants one
//!   shape.
//!
//! ## Persistence
//!
//! Nothing here is persisted *by this increment*. `Annotation` becomes
//! reachable from a `.waffle` file when D4a adds `TabKind::Drawing` and M2
//! adds the `Pmi` feature, and that is when the reader floor moves: these are
//! serde-tagged enums, so a new variant is unreadable by an older build
//! (`docs/FILE_FORMAT.md` §13 rule 3 — the same rule that made v7 of N1 a
//! bump because its additive `names` field carried a `Selector::Pid`). The
//! schema is pinned regardless, by `tests/annotation_schema_golden.rs`, so
//! the shape cannot drift between now and then unnoticed.

pub mod layout;
pub mod measure;

use serde::{Deserialize, Serialize};

use crate::geom_ref::GeomRef;

/// One annotation on a drawing sheet or in a part's PMI.
///
/// `GeomRef` is large (it carries a `TopoSignature`), so every variant that
/// holds one is large. Same call as [`crate::geom_ref::Selector`] and
/// feature-engine's `Operation`: a serialized document type, one per
/// annotation rather than one per vertex, matched and constructed at a
/// handful of sites. Boxing would ripple through the file format to buy a
/// smaller discriminant.
/// `Annotation` is deliberately NOT `PartialEq`: [`GeomRef`] is not, and
/// making it so means deriving `PartialEq` down through `TopoSignature` and
/// `TopoQuery` — a change to the reference types several other consumers
/// share, for the sake of a convenience here. Two annotations are compared by
/// their serialized form, which is also the form that is persisted and the
/// only one equality would need to agree with.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[allow(clippy::large_enum_variant)]
pub enum Annotation {
    /// A dimension: how to measure, what to measure, and where the label sits.
    Dimension {
        kind: DimensionKind,
        /// The entities measured, in the order [`DimensionKind`] expects.
        anchors: Vec<GeomRef>,
        #[serde(default)]
        value: Measured,
        /// Decimal places to show. `None` ⇒ the document setting.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        precision: Option<u8>,
        /// A second unit shown in brackets beneath the primary one (a unit key
        /// of `app/src/lib/units.js`: `"mm"`, `"in"`, …). `None` ⇒ the
        /// document setting, which may itself be none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dual_unit: Option<String>,
        #[serde(default)]
        placement: Placement2,
    },
    /// Free text, optionally with a leader to an entity.
    Note {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        leader: Option<GeomRef>,
        #[serde(default)]
        placement: Placement2,
    },
    /// The centre cross of a circular entity.
    CentreMark { anchor: GeomRef },
    /// A centre line between two entities (two parallel walls, two holes).
    CentreLine { anchors: [GeomRef; 2] },
    /// A datum feature label (`A`, `B`, …) for M1/M2's tolerance frames.
    Datum {
        label: String,
        anchor: GeomRef,
        #[serde(default)]
        placement: Placement2,
    },
}

impl Annotation {
    /// Every entity this annotation references, in order. The rebuild resolves
    /// these and nothing else; a caller that collects dependencies reads this
    /// rather than matching the variants itself.
    pub fn anchors(&self) -> Vec<&GeomRef> {
        match self {
            Annotation::Dimension { anchors, .. } => anchors.iter().collect(),
            Annotation::Note { leader, .. } => leader.iter().collect(),
            Annotation::CentreMark { anchor } | Annotation::Datum { anchor, .. } => vec![anchor],
            Annotation::CentreLine { anchors } => anchors.iter().collect(),
        }
    }

    /// The cosmetic label offset, for the variants that have one.
    pub fn placement(&self) -> Option<Placement2> {
        match self {
            Annotation::Dimension { placement, .. }
            | Annotation::Note { placement, .. }
            | Annotation::Datum { placement, .. } => Some(*placement),
            Annotation::CentreMark { .. } | Annotation::CentreLine { .. } => None,
        }
    }
}

/// How a dimension measures its anchors.
///
/// §7: "`DimensionKind` reuses the seven sketch dimension kinds, with
/// `Ordinate` added." The seven are the value-bearing arms of
/// [`crate::sketch::SketchConstraint`] and the names here are deliberately
/// theirs, so the correspondence is checkable by grep rather than by belief.
/// The drafting vocabulary a user sees differs: `Distance` is an **aligned**
/// dimension, `HDistance`/`VDistance` are **linear** horizontal/vertical, and
/// `Angle` is **angular**. That translation is the UI's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum DimensionKind {
    /// Aligned linear: the distance between two anchors, measured along the
    /// line joining them — or, for two parallel lines, between them.
    /// (`SketchConstraint::Distance`.)
    Distance,
    /// Perpendicular distance from a point to a line.
    /// (`SketchConstraint::PointLineDistance`.)
    PointLineDistance,
    /// `|Δu|` only: the horizontal component, leaving the vertical free.
    /// (`SketchConstraint::HDistance`.)
    HDistance,
    /// `|Δv|` only. (`SketchConstraint::VDistance`.)
    VDistance,
    /// Angle between two lines, in RADIANS. (`SketchConstraint::Angle`;
    /// note that constraint's *expression* evaluates in degrees — the
    /// conversion is the UI's, as everywhere else here.)
    Angle,
    /// Radius of a circle, arc or ellipse. (`SketchConstraint::Radius`.)
    Radius,
    /// Diameter — twice the radius, drawn through the centre.
    /// (`SketchConstraint::Diameter`.)
    Diameter,
    /// One coordinate of a single anchor, measured from the view's ordinate
    /// origin along `axis`. Added by §7; it has no sketch counterpart.
    Ordinate { axis: OrdinateAxis },
}

impl DimensionKind {
    /// How many anchors this kind measures. The rebuild checks this before
    /// resolving, so a malformed annotation is one typed error rather than a
    /// surprise inside the measurement.
    pub fn arity(&self) -> usize {
        match self {
            DimensionKind::Radius | DimensionKind::Diameter | DimensionKind::Ordinate { .. } => 1,
            DimensionKind::Distance
            | DimensionKind::PointLineDistance
            | DimensionKind::HDistance
            | DimensionKind::VDistance
            | DimensionKind::Angle => 2,
        }
    }

    /// Whether the measured number is an angle (radians) rather than a length
    /// (meters). A formatter must know this before it converts units.
    pub fn is_angular(&self) -> bool {
        matches!(self, DimensionKind::Angle)
    }
}

/// Which view-plane axis an [`DimensionKind::Ordinate`] dimension reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum OrdinateAxis {
    /// The view's horizontal axis.
    U,
    /// The view's vertical axis.
    V,
}

/// What determines a dimension's value.
///
/// Never a number a user typed. See the module docs for why this has three
/// arms where §7 sketched two.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum Measured {
    /// The default: the value is whatever [`measure::measure`] reads off the
    /// anchors' geometry, recomputed every rebuild. Nothing is stored, so
    /// nothing can go stale.
    #[default]
    FromGeometry,
    /// Re-measured every rebuild by evaluating this expression in the
    /// document's parameter environment — which, from D2, includes the
    /// measurement functions (`distance(…)`, `radius(…)`, `mass(…)`). For a
    /// dimension whose number is a *derived* quantity rather than the plain
    /// measurement of its own anchors.
    ///
    /// A struct variant, not §7's `Expr(String)` newtype: serde's
    /// internally-tagged representation (`#[serde(tag = "type")]`, which every
    /// persisted enum in this tree uses) cannot serialize a newtype variant
    /// wrapping a primitive — there is nowhere to put the tag. Same for
    /// [`Measured::Value`].
    Expr { expr: String },
    /// A literal, in meters (radians for an angular dimension).
    ///
    /// **Not authorable.** This arm exists for two readers: a nominal
    /// imported from a PMI-bearing file, and a cache written by a build that
    /// wants the last known number available without a kernel. A UI must not
    /// offer it — a typed dimension is how a drawing comes to disagree with
    /// its part.
    Value { value: f64 },
}

/// The cosmetic offset of a label from its computed position, in view-plane
/// units (meters, like [`crate::kernel::projection::Curve2`]).
///
/// The same idea the sketch dimension labels already have, where the app
/// keeps it per constraint in `dimensionLabelOffsets`. Zero means "wherever
/// the layout puts it", which is the only value a freshly authored
/// annotation has.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Placement2 {
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
}

impl Placement2 {
    /// An offset of `(dx, dy)`.
    pub fn new(dx: f64, dy: f64) -> Placement2 {
        Placement2 { dx, dy }
    }

    /// Whether this is the default (no offset at all).
    pub fn is_zero(&self) -> bool {
        self.dx == 0.0 && self.dy == 0.0
    }
}

#[cfg(test)]
mod tests;
