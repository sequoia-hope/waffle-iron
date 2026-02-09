use slvs::entity::Point;
use std::collections::HashMap;

use crate::entity_mapping::SketchToSlvs;
use crate::profiles::extract_profiles;
use crate::status::classify_status;
use crate::types::{Sketch, SolveStatus, SolvedSketch};

/// Solve a sketch: map entities/constraints to slvs, run solver, extract results.
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch {
    let mut mapping = SketchToSlvs::new();
    mapping.add_entities(&sketch.entities);
    mapping.add_constraints(&sketch.constraints);

    let result = mapping.system.solve(&mapping.group);
    let status = classify_status(result);

    let positions = extract_positions(&mapping);
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

/// Extract solved positions for all point entities.
fn extract_positions(mapping: &SketchToSlvs) -> HashMap<u32, (f64, f64)> {
    let mut positions = HashMap::new();

    for (id, handle) in &mapping.point_handles {
        if let Ok(data) = mapping.system.entity_data(handle) {
            match data {
                Point::OnWorkplane { coords: [u, v], .. } => {
                    positions.insert(*id, (u, v));
                }
                Point::In3d {
                    coords: [x, y, _], ..
                } => {
                    positions.insert(*id, (x, y));
                }
            }
        }
    }

    positions
}
