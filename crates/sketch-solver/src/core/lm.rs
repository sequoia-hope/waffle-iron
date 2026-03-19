//! Levenberg-Marquardt solver with weak spring augmentation.
//!
//! This is the primary solver (not a fallback). Per research R2/R4,
//! LM with Marquardt damping + weak springs provides both velocity-space
//! and position-space regularization for under-constrained systems.

use super::constraint::ConstraintImpl;
use super::types::{ScaleType, SolveOptions, SolveOutcome};

/// Solve a system of geometric constraints using augmented Levenberg-Marquardt.
///
/// - `x0`: starting guess (warm start from current entity positions)
/// - `x_anchor`: spring anchor (pre-edit state for drag, or same as x0)
/// - `constraints`: pre-built internal constraints from the builder
/// - `eq_scale_types`: Distance or Angle classification per equation row
/// - `num_equations`: total number of scalar equations
/// - `options`: solver tuning parameters
pub fn lm_solve(
    _x0: &[f64],
    _x_anchor: &[f64],
    _constraints: &[ConstraintImpl],
    _eq_scale_types: &[ScaleType],
    _num_equations: usize,
    _options: &SolveOptions,
) -> SolveOutcome {
    todo!("Fork B: augmented LM solver")
}
