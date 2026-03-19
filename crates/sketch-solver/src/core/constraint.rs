//! Constraint trait and internal constraint enum.
//!
//! `ConstraintEq` is the trait that all constraints implement.
//! `ConstraintImpl` is the enum of all concrete constraint variants,
//! storing pre-resolved typed indices from `ParamLayout`.
//!
//! Wave 1: `num_equations()` and `scale_types()` are implemented.
//! `residuals()` and `jacobian()` are `todo!()` stubs — filled in by Fork A.

use nalgebra::Point2;

use super::types::*;

/// Singularity guard: clamp denominators away from zero.
pub const EPSILON: f64 = 1e-12;

/// A constraint that contributes scalar equations to the solver.
pub trait ConstraintEq {
    /// Number of scalar equations this constraint contributes.
    fn num_equations(&self) -> usize;

    /// Scale type per equation (Distance or Angle) for D_row construction.
    fn scale_types(&self) -> &[ScaleType];

    /// Compute residuals f(x). Should be zero when satisfied.
    /// `out` slice has length == `num_equations()`.
    fn residuals(&self, params: &[f64], out: &mut [f64]);

    /// Append sparse Jacobian entries: (global_row, param_col, value).
    /// `eq_offset` is this constraint's starting row in the global Jacobian.
    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>);
}

/// Internal constraint representation with pre-resolved typed indices.
///
/// Built from `SketchConstraint` + `ParamLayout` by the builder.
/// All entity ID lookups are resolved at build time — the solver never
/// touches entity IDs.
pub enum ConstraintImpl {
    // ── Group 1: Linear (constant Jacobian) ──────────────────────────
    Coincident { p1: PointIdx, p2: PointIdx },
    Horizontal { line: LineIdx },
    Vertical { line: LineIdx },
    SymmetricH { p1: PointIdx, p2: PointIdx },
    SymmetricV { p1: PointIdx, p2: PointIdx },
    Midpoint { point: PointIdx, line: LineIdx },
    Dragged { point: PointIdx, target: Point2<f64> },
    Radius { r: RadiusIdx, target: f64 },
    Diameter { r: RadiusIdx, target: f64 },
    HDistance { p1x: usize, p2x: usize, d: f64 },
    VDistance { p1y: usize, p2y: usize, d: f64 },

    // ── Group 2: Nonlinear fundamentals ──────────────────────────────
    DistancePP { p1: PointIdx, p2: PointIdx, d: f64 },
    EqualLength { l1: LineIdx, l2: LineIdx },
    Parallel { l1: LineIdx, l2: LineIdx },
    Perpendicular { l1: LineIdx, l2: LineIdx },
    Angle { l1: LineIdx, l2: LineIdx, value_rad: f64 },

    // ── Group 3: Point-on-entity ─────────────────────────────────────
    OnLine { point: PointIdx, line: LineIdx },
    OnCircle { point: PointIdx, center: PointIdx, radius: RadiusDef },

    // ── Group 4: Normalized point-line distance ──────────────────────
    DistancePL { point: PointIdx, line: LineIdx, d: f64 },

    // ── Group 5: Tangent ─────────────────────────────────────────────
    TangentLineCircle { line: LineIdx, center: PointIdx, radius: RadiusDef },
    TangentArcArc {
        c1: PointIdx,
        r1: RadiusDef,
        c2: PointIdx,
        r2: RadiusDef,
        internal: bool,
    },

    // ── Group 6: Symmetric about arbitrary line ──────────────────────
    /// 2 equations: perpendicularity (Angle-type) + midpoint-on-line (Distance-type).
    SymmetricLine { p1: PointIdx, p2: PointIdx, line: LineIdx },

    // ── Group 7: Compound ────────────────────────────────────────────
    EqualAngle { l1: LineIdx, l2: LineIdx, l3: LineIdx, l4: LineIdx },
    Ratio { l1: LineIdx, l2: LineIdx, k: f64 },
    EqualPointToLine { p1: PointIdx, p2: PointIdx, line: LineIdx },
    /// No-op in 2D. Matches existing behavior + oracle test.
    SameOrientation,
    EqualRadius { r1: RadiusIdx, r2: RadiusIdx },
}

impl ConstraintEq for ConstraintImpl {
    fn num_equations(&self) -> usize {
        match self {
            Self::Coincident { .. } => 2,
            Self::Horizontal { .. } => 1,
            Self::Vertical { .. } => 1,
            Self::SymmetricH { .. } => 2,
            Self::SymmetricV { .. } => 2,
            Self::Midpoint { .. } => 2,
            Self::Dragged { .. } => 2,
            Self::Radius { .. } => 1,
            Self::Diameter { .. } => 1,
            Self::HDistance { .. } => 1,
            Self::VDistance { .. } => 1,
            Self::DistancePP { .. } => 1,
            Self::EqualLength { .. } => 1,
            Self::Parallel { .. } => 1,
            Self::Perpendicular { .. } => 1,
            Self::Angle { .. } => 1,
            Self::OnLine { .. } => 1,
            Self::OnCircle { .. } => 1,
            Self::DistancePL { .. } => 1,
            Self::TangentLineCircle { .. } => 1,
            Self::TangentArcArc { .. } => 1,
            Self::SymmetricLine { .. } => 2,
            Self::EqualAngle { .. } => 1,
            Self::Ratio { .. } => 1,
            Self::EqualPointToLine { .. } => 1,
            Self::SameOrientation => 0,
            Self::EqualRadius { .. } => 1,
        }
    }

    fn scale_types(&self) -> &[ScaleType] {
        use ScaleType::{Angle as A, Distance as D};
        match self {
            Self::Coincident { .. } => &[D, D],
            Self::Horizontal { .. } => &[D],
            Self::Vertical { .. } => &[D],
            Self::SymmetricH { .. } => &[D, D],
            Self::SymmetricV { .. } => &[D, D],
            Self::Midpoint { .. } => &[D, D],
            Self::Dragged { .. } => &[D, D],
            Self::Radius { .. } => &[D],
            Self::Diameter { .. } => &[D],
            Self::HDistance { .. } => &[D],
            Self::VDistance { .. } => &[D],
            Self::DistancePP { .. } => &[D],
            Self::EqualLength { .. } => &[D],
            Self::Parallel { .. } => &[A],
            Self::Perpendicular { .. } => &[A],
            Self::Angle { .. } => &[A],
            Self::OnLine { .. } => &[D],
            Self::OnCircle { .. } => &[D],
            Self::DistancePL { .. } => &[D],
            Self::TangentLineCircle { .. } => &[D],
            Self::TangentArcArc { .. } => &[D],
            Self::SymmetricLine { .. } => &[A, D],
            Self::EqualAngle { .. } => &[A],
            Self::Ratio { .. } => &[D],
            Self::EqualPointToLine { .. } => &[D],
            Self::SameOrientation => &[],
            Self::EqualRadius { .. } => &[D],
        }
    }

    fn residuals(&self, _params: &[f64], _out: &mut [f64]) {
        todo!("Fork A: constraint residuals")
    }

    fn jacobian(&self, _params: &[f64], _eq_offset: usize, _out: &mut Vec<(usize, usize, f64)>) {
        todo!("Fork A: constraint Jacobians")
    }
}
