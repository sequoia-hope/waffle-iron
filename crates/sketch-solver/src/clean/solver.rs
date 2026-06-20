//! Clean-room solver entry point.
//!
//! PR-SS1a: placeholder — returns `SolveFailed` with initial positions.
//! PR-SS1c will implement the full LM-based solver.

use std::collections::HashMap;

use crate::profiles::extract_profiles;
use crate::types::{Sketch, SketchEntity, SolveStatus, SolvedSketch};

/// Solve a sketch: map entities/constraints to a parameter vector, run LM
/// minimization, extract results.
///
/// PR-SS1a: returns `SolveFailed { reason: "not yet implemented" }` with
/// positions populated from the initial entity declarations. The legacy
/// slvs path remains the default until PR-SS1d.
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch {
    // PR-SS1a: ParamLayout is built to verify entity mapping, but the LM
    // solver is not yet implemented. PR-SS1c will wire it in.
    let _layout = crate::entity_mapping::ParamLayout::build(&sketch.entities);
    let positions = initial_positions(&sketch.entities);

    let status = SolveStatus::SolveFailed {
        reason: "clean-room solver not yet implemented (PR-SS1c)".to_string(),
    };

    let profiles = if matches!(
        status,
        SolveStatus::FullyConstrained | SolveStatus::UnderConstrained { .. }
    ) {
        extract_profiles(&sketch.entities, &positions)
    } else {
        Vec::new()
    };

    SolvedSketch {
        positions,
        profiles,
        status,
    }
}

/// Extract initial positions from point entity declarations.
fn initial_positions(entities: &[SketchEntity]) -> HashMap<u32, (f64, f64)> {
    let mut positions = HashMap::new();
    for entity in entities {
        if let SketchEntity::Point { id, x, y, .. } = entity {
            positions.insert(*id, (*x, *y));
        }
    }
    positions
}
