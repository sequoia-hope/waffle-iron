//! Typed index wrappers and solver options for the constraint solver.
//!
//! These types form the spine that all solver components compile against.
//! Entity ID lookups are resolved at build time into these typed indices,
//! so the solver never touches entity IDs.

use nalgebra::{DMatrix, DVector, Point2, Vector2};

// ── Typed index wrappers ─────────────────────────────────────────────────

/// Index of a 2D point's x-coordinate in the parameter vector.
/// y-coordinate is always at `self.0 + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointIdx(pub usize);

impl PointIdx {
    pub fn x(self) -> usize {
        self.0
    }
    pub fn y(self) -> usize {
        self.0 + 1
    }
    pub fn read(self, params: &[f64]) -> Point2<f64> {
        Point2::new(params[self.0], params[self.0 + 1])
    }
}

/// A line segment defined by two point indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineIdx {
    pub start: PointIdx,
    pub end: PointIdx,
}

impl LineIdx {
    pub fn delta(self, params: &[f64]) -> Vector2<f64> {
        self.end.read(params) - self.start.read(params)
    }
    pub fn length(self, params: &[f64]) -> f64 {
        self.delta(params).norm()
    }
    pub fn length_sq(self, params: &[f64]) -> f64 {
        self.delta(params).norm_squared()
    }
}

/// Index of a radius parameter in the parameter vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RadiusIdx(pub usize);

impl RadiusIdx {
    pub fn read(self, params: &[f64]) -> f64 {
        params[self.0]
    }
}

/// Source of a radius value — either an explicit parameter (circle) or
/// implicit from center-to-start distance (arc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadiusDef {
    /// Explicit radius parameter (circle).
    Param(RadiusIdx),
    /// Implicit radius = dist(center, start_point). The center PointIdx is
    /// provided separately in the constraint variant.
    Implicit(PointIdx),
}

impl RadiusDef {
    /// Read the radius value, computing distance for implicit arcs.
    pub fn read(self, params: &[f64], center: PointIdx) -> f64 {
        match self {
            RadiusDef::Param(idx) => idx.read(params),
            RadiusDef::Implicit(start) => {
                nalgebra::distance(&center.read(params), &start.read(params))
            }
        }
    }
}

// ── Scale classification ─────────────────────────────────────────────────

/// Whether a constraint equation measures distance (meters) or angle (radians).
/// Used to build D_row for Jacobian scaling (R4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleType {
    Distance,
    Angle,
}

// ── Solver options and outcome ───────────────────────────────────────────

pub struct SolveOptions {
    pub max_iterations: usize,
    pub tolerance: f64,
    pub lambda_init: f64,
    pub spring_mu: f64,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            tolerance: 1e-7,
            lambda_init: 1e-3,
            spring_mu: 1e-6,
        }
    }
}

pub struct SolveOutcome {
    pub params: Vec<f64>,
    pub converged: bool,
    pub iterations: usize,
    pub final_residual_norm: f64,
    /// Scaled, un-augmented Jacobian (for SVD rank diagnostics).
    pub jacobian_scaled: DMatrix<f64>,
    /// Scaled residual (for conflict detection).
    pub residual_scaled: DVector<f64>,
}
