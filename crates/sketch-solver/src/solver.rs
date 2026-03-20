//! Sketch solver: orchestrates core/ modules to solve a sketch.
//!
//! Replaces the previous slvs-based implementation with pure Rust.

use crate::core::builder::{build_constraints, build_eq_to_constraint_map};
use crate::core::constraint::ConstraintEq;
use crate::core::lm::lm_solve;
use crate::core::params::ParamLayout;
use crate::core::rank::analyze_rank;
use crate::core::status::classify_solve;
use crate::core::types::SolveOptions;
use crate::profiles::extract_profiles;
use crate::types::{Sketch, SolveStatus, SolvedSketch};

/// Solve a sketch: build parameter layout, constraints, run LM solver,
/// classify status, extract positions and profiles.
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch {
    let entities = &sketch.entities;
    let constraints = &sketch.constraints;

    // Build parameter layout from entities
    let layout = ParamLayout::from_entities(entities);
    let x0 = layout.initial_params(entities);

    // Build constraint equations
    let constraint_impls = build_constraints(constraints, entities, &layout);

    // Build scale type vector for row scaling
    let eq_scale_types: Vec<_> = constraint_impls
        .iter()
        .flat_map(|c| c.scale_types().iter().copied())
        .collect();
    let num_equations: usize = constraint_impls.iter().map(|c| c.num_equations()).sum();

    // Solve with LM (x0 serves as both starting guess and spring anchor
    // for initial solve — they diverge only during drag operations)
    let options = SolveOptions::default();
    let outcome = lm_solve(
        &x0,
        &x0,
        &constraint_impls,
        &eq_scale_types,
        num_equations,
        &options,
    );

    // Rank analysis on scaled, un-augmented Jacobian
    let eq_to_constraint = build_eq_to_constraint_map(&constraint_impls);
    let rank = analyze_rank(
        &outcome.jacobian_scaled,
        &outcome.residual_scaled,
        options.tolerance,
    );
    let status = classify_solve(&outcome, &rank, &eq_to_constraint, layout.num_params());

    // Extract positions and radii
    let positions = layout.extract_positions(&outcome.params);
    let radii = layout.extract_radii(&outcome.params);

    // Extract profiles (existing algorithm, unchanged)
    let profiles = if matches!(
        status,
        SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
    ) {
        extract_profiles(entities, &positions)
    } else {
        Vec::new()
    };

    SolvedSketch {
        positions,
        radii,
        profiles,
        status,
    }
}
