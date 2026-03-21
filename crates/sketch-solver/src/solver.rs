//! Sketch solver: orchestrates core/ modules to solve a sketch.
//!
//! Replaces the previous slvs-based implementation with pure Rust.
//! Supports constraint graph decomposition: independent subsystems
//! are detected via union-find and solved separately.

use crate::core::builder::{build_constraints, build_eq_to_constraint_map};
use crate::core::constraint::ConstraintEq;
use crate::core::error::SolveError;
use crate::core::graph::decompose;
use crate::core::lm::lm_solve;
use crate::core::params::ParamLayout;
use crate::core::rank::{analyze_rank, refine_dof_with_hessian};
use crate::core::status::classify_solve;
use crate::core::types::SolveOptions;
use crate::profiles::extract_profiles;
use crate::types::{Sketch, SolveStatus, SolvedSketch};

/// Solve a sketch: build parameter layout, constraints, run LM solver,
/// classify status, extract positions and profiles.
pub fn solve_sketch(sketch: &Sketch) -> Result<SolvedSketch, SolveError> {
    let entities = &sketch.entities;
    let constraints = &sketch.constraints;

    // Build parameter layout from entities
    let layout = ParamLayout::from_entities(entities);
    let x0 = layout.initial_params(entities);

    // Build constraint equations
    let constraint_impls = build_constraints(constraints, entities, &layout, &x0)?;

    // Decompose into independent subsystems
    let subsystems = decompose(&constraint_impls, layout.num_params());

    // Fast path: 0 or 1 subsystem — use monolithic solve (zero overhead)
    if subsystems.len() <= 1 {
        return solve_monolithic(&constraint_impls, &x0, &layout, entities);
    }

    // Multi-subsystem path: solve each independently, aggregate results
    let options = SolveOptions::default();
    let mut params = x0.clone();
    let mut total_dof: usize = 0;
    let mut worst_status = SolveStatus::FullyConstrained;

    for subsystem in &subsystems {
        // Build constraint ref slice for this subsystem
        let sub_constraints: Vec<&_> = subsystem
            .constraint_indices
            .iter()
            .map(|&i| &constraint_impls[i])
            .collect();

        let eq_scale_types: Vec<_> = sub_constraints
            .iter()
            .flat_map(|c| c.scale_types().iter().copied())
            .collect();
        let num_equations: usize = sub_constraints.iter().map(|c| c.num_equations()).sum();

        // Solve with full param vector — LM reads global indices from Jacobian entries.
        // Unused columns get zero gradient; weak springs hold them at x0.
        let outcome = lm_solve(
            &params,
            &x0,
            &sub_constraints,
            &eq_scale_types,
            num_equations,
            &options,
        );

        // Copy solved params back
        for &pi in &subsystem.param_indices {
            params[pi] = outcome.params[pi];
        }

        // Rank analysis — extract sub-Jacobian with only this subsystem's param columns.
        // The full Jacobian has zero columns for unused params which would inflate DOF.
        let sub_jacobian = extract_columns(&outcome.jacobian_scaled, &subsystem.param_indices);
        let eq_to_constraint: Vec<usize> = sub_constraints
            .iter()
            .enumerate()
            .flat_map(|(i, c)| std::iter::repeat_n(i, c.num_equations()))
            .collect();
        let mut rank = analyze_rank(&sub_jacobian, &outcome.residual_scaled, options.tolerance);
        let refined_dof =
            refine_dof_with_hessian(&rank, &sub_constraints, &outcome.params, &outcome.d_row);
        rank.dof = refined_dof;

        let status = classify_solve(&outcome, &rank, &eq_to_constraint, layout.num_params());

        total_dof += rank.dof;
        worst_status = worse_status(worst_status, status);
    }

    // If decomposed solve yielded DOF info, update status
    let status = match worst_status {
        SolveStatus::FullyConstrained if total_dof > 0 => SolveStatus::UnderConstrained {
            dof: total_dof as u32,
        },
        SolveStatus::UnderConstrained { dof } => SolveStatus::UnderConstrained {
            dof: dof.max(total_dof as u32),
        },
        other => other,
    };

    // Extract positions and radii from aggregated params
    let positions = layout.extract_positions(&params);
    let radii = layout.extract_radii(&params);

    let profiles = if matches!(
        status,
        SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
    ) {
        extract_profiles(entities, &positions)
    } else {
        Vec::new()
    };

    Ok(SolvedSketch {
        positions,
        radii,
        profiles,
        status,
    })
}

/// Monolithic solve path — used when there's 0 or 1 subsystem.
fn solve_monolithic(
    constraint_impls: &[crate::core::constraint::ConstraintImpl],
    x0: &[f64],
    layout: &ParamLayout,
    entities: &[crate::types::SketchEntity],
) -> Result<SolvedSketch, SolveError> {
    let eq_scale_types: Vec<_> = constraint_impls
        .iter()
        .flat_map(|c| c.scale_types().iter().copied())
        .collect();
    let num_equations: usize = constraint_impls.iter().map(|c| c.num_equations()).sum();

    let options = SolveOptions::default();
    let outcome = lm_solve(
        x0,
        x0,
        constraint_impls,
        &eq_scale_types,
        num_equations,
        &options,
    );

    let eq_to_constraint = build_eq_to_constraint_map(constraint_impls);
    let mut rank = analyze_rank(
        &outcome.jacobian_scaled,
        &outcome.residual_scaled,
        options.tolerance,
    );
    let refined_dof =
        refine_dof_with_hessian(&rank, constraint_impls, &outcome.params, &outcome.d_row);
    rank.dof = refined_dof;

    let status = classify_solve(&outcome, &rank, &eq_to_constraint, layout.num_params());

    let positions = layout.extract_positions(&outcome.params);
    let radii = layout.extract_radii(&outcome.params);

    let profiles = if matches!(
        status,
        SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
    ) {
        extract_profiles(entities, &positions)
    } else {
        Vec::new()
    };

    Ok(SolvedSketch {
        positions,
        radii,
        profiles,
        status,
    })
}

/// Return the "worse" of two statuses for aggregation.
/// Priority: SolveFailed > OverConstrained > UnderConstrained > FullyConstrained.
fn worse_status(a: SolveStatus, b: SolveStatus) -> SolveStatus {
    fn severity(s: &SolveStatus) -> u8 {
        match s {
            SolveStatus::FullyConstrained => 0,
            SolveStatus::UnderConstrained { .. } => 1,
            SolveStatus::OverConstrained { .. } => 2,
            SolveStatus::SolveFailed { .. } => 3,
        }
    }
    if severity(&b) > severity(&a) {
        b
    } else {
        a
    }
}

/// Extract selected columns from a matrix, producing a smaller matrix.
fn extract_columns(matrix: &nalgebra::DMatrix<f64>, cols: &[usize]) -> nalgebra::DMatrix<f64> {
    let nrows = matrix.nrows();
    let mut sub = nalgebra::DMatrix::zeros(nrows, cols.len());
    for (j_new, &j_old) in cols.iter().enumerate() {
        sub.set_column(j_new, &matrix.column(j_old));
    }
    sub
}
