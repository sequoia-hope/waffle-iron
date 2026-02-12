use std::collections::HashMap;

use modeling_ops::{
    execute_boolean, execute_chamfer, execute_extrude, execute_fillet, execute_revolve,
    execute_shell, BooleanKind, OpResult,
};
use uuid::Uuid;

use crate::resolve::resolve_with_fallback;
use crate::types::{BooleanOp, EngineError, Feature, FeatureTree, Operation};
use modeling_ops::KernelBundle;
use waffle_types::{OutputKey, Sketch};

/// State of the engine after a rebuild.
#[derive(Debug)]
pub struct RebuildState {
    /// OpResult for each successfully built feature.
    pub feature_results: HashMap<Uuid, OpResult>,
    /// Warnings accumulated during rebuild.
    pub warnings: Vec<String>,
    /// Features that failed to rebuild, with error messages.
    pub errors: Vec<(Uuid, String)>,
}

/// Rebuild the feature tree from scratch (or from a change point).
///
/// Replays features in order, resolving GeomRefs and executing operations.
pub fn rebuild(
    tree: &FeatureTree,
    kb: &mut dyn KernelBundle,
    from_index: usize,
    existing_results: &HashMap<Uuid, OpResult>,
) -> RebuildState {
    let mut state = RebuildState {
        feature_results: HashMap::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    // Carry forward results from features before the rebuild point
    for (id, result) in existing_results {
        state.feature_results.insert(*id, result.clone());
    }

    let active = tree.active_features();

    for (i, feature) in active.iter().enumerate() {
        if i < from_index {
            continue;
        }
        if feature.suppressed {
            continue;
        }

        // Resolve any GeomRef references before executing the feature
        resolve_feature_refs(feature, &state.feature_results, &mut state.warnings);

        match execute_feature(feature, kb, &state.feature_results, tree) {
            Ok(result) => {
                state.feature_results.insert(feature.id, result);
            }
            Err(e) => {
                state.errors.push((feature.id, e.to_string()));
                // Continue rebuilding remaining features
            }
        }
    }

    state
}

/// Execute a single feature's operation.
fn execute_feature(
    feature: &Feature,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
) -> Result<OpResult, EngineError> {
    match &feature.operation {
        Operation::Sketch { .. } => {
            // Sketches don't produce OpResults directly — they store solved geometry.
            // Return a minimal OpResult with no outputs.
            Ok(OpResult {
                outputs: Vec::new(),
                provenance: modeling_ops::Provenance {
                    created: Vec::new(),
                    deleted: Vec::new(),
                    modified: Vec::new(),
                    role_assignments: Vec::new(),
                },
                diagnostics: modeling_ops::Diagnostics::default(),
            })
        }

        Operation::Extrude { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let sketch = find_sketch_in_tree(params.sketch_id, tree)?;

            let direction = params.direction.unwrap_or(sketch.plane_normal);

            if sketch.solved_profiles.is_empty() {
                return Err(EngineError::ProfileOutOfRange {
                    index: params.profile_index,
                    count: 0,
                });
            }
            if params.profile_index >= sketch.solved_profiles.len() {
                return Err(EngineError::ProfileOutOfRange {
                    index: params.profile_index,
                    count: sketch.solved_profiles.len(),
                });
            }

            // For cut extrudes, reverse the direction to go INTO the target body.
            // Offset the profile slightly outward (along normal) and extend depth
            // by 2*epsilon to avoid coplanar faces with the target body,
            // which causes truck's boolean to fail.
            let eps = 0.01;
            let (extrude_direction, extrude_depth, face_origin) = if params.cut {
                let offset_origin = [
                    sketch.plane_origin[0] + direction[0] * eps,
                    sketch.plane_origin[1] + direction[1] * eps,
                    sketch.plane_origin[2] + direction[2] * eps,
                ];
                (
                    [-direction[0], -direction[1], -direction[2]],
                    params.depth + 2.0 * eps,
                    offset_origin,
                )
            } else {
                (direction, params.depth, sketch.plane_origin)
            };

            let x_axis = tangent_x_from_normal(sketch.plane_normal);
            let face_ids = kb.make_faces_from_profiles(
                &sketch.solved_profiles,
                face_origin,
                sketch.plane_normal,
                x_axis,
                &sketch.solved_positions,
            )?;

            if face_ids.is_empty() {
                return Err(EngineError::ProfileOutOfRange {
                    index: params.profile_index,
                    count: 0,
                });
            }

            let face_index = params.profile_index.min(face_ids.len() - 1);
            let extrude_result =
                execute_extrude(kb, face_ids[face_index], extrude_direction, extrude_depth, None)?;

            if params.cut {
                // Find the target body to subtract from (most recent solid before this feature)
                let target_handle = find_most_recent_solid(feature, feature_results, tree)
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: "Cut extrude requires an existing body to subtract from".into(),
                    })?;

                let tool_handle = extrude_result
                    .outputs
                    .first()
                    .map(|(_, body)| body.handle.clone())
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: "Extrude produced no solid output for cut".into(),
                    })?;

                let boolean_result =
                    execute_boolean(kb, &target_handle, &tool_handle, BooleanKind::Subtract)?;
                Ok(boolean_result)
            } else {
                Ok(extrude_result)
            }
        }

        Operation::Revolve { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let sketch = find_sketch_in_tree(params.sketch_id, tree)?;

            if sketch.solved_profiles.is_empty() {
                return Err(EngineError::ProfileOutOfRange {
                    index: params.profile_index,
                    count: 0,
                });
            }
            if params.profile_index >= sketch.solved_profiles.len() {
                return Err(EngineError::ProfileOutOfRange {
                    index: params.profile_index,
                    count: sketch.solved_profiles.len(),
                });
            }

            let x_axis = tangent_x_from_normal(sketch.plane_normal);
            let face_ids = kb.make_faces_from_profiles(
                &sketch.solved_profiles,
                sketch.plane_origin,
                sketch.plane_normal,
                x_axis,
                &sketch.solved_positions,
            )?;

            if face_ids.is_empty() {
                return Err(EngineError::ProfileOutOfRange {
                    index: params.profile_index,
                    count: 0,
                });
            }

            let face_index = params.profile_index.min(face_ids.len() - 1);
            let result = execute_revolve(
                kb,
                face_ids[face_index],
                params.axis_origin,
                params.axis_direction,
                params.angle,
                None,
            )?;
            Ok(result)
        }

        Operation::BooleanCombine { params } => {
            // Find the solid handles from the referenced features
            let handle_a = find_solid_handle(&params.body_a, feature_results)?;
            let handle_b = find_solid_handle(&params.body_b, feature_results)?;

            let kind = match params.operation {
                BooleanOp::Union => BooleanKind::Union,
                BooleanOp::Subtract => BooleanKind::Subtract,
                BooleanOp::Intersect => BooleanKind::Intersect,
            };

            let result = execute_boolean(kb, &handle_a, &handle_b, kind)?;
            Ok(result)
        }

        Operation::Fillet { params } => {
            // Find the most recent solid handle
            let solid_handle = find_latest_solid_handle(feature, feature_results)?;

            // Resolve edge GeomRefs to KernelIds
            let mut edge_ids = Vec::new();
            for edge_ref in &params.edges {
                let resolved = resolve_with_fallback(edge_ref, feature_results).map_err(|e| {
                    EngineError::ResolutionFailed {
                        reason: format!("Failed to resolve fillet edge: {}", e),
                    }
                })?;
                edge_ids.push(resolved.kernel_id);
            }

            let result = execute_fillet(kb, &solid_handle, &edge_ids, params.radius)?;
            Ok(result)
        }

        Operation::Chamfer { params } => {
            let solid_handle = find_latest_solid_handle(feature, feature_results)?;

            let mut edge_ids = Vec::new();
            for edge_ref in &params.edges {
                let resolved = resolve_with_fallback(edge_ref, feature_results).map_err(|e| {
                    EngineError::ResolutionFailed {
                        reason: format!("Failed to resolve chamfer edge: {}", e),
                    }
                })?;
                edge_ids.push(resolved.kernel_id);
            }

            let result = execute_chamfer(kb, &solid_handle, &edge_ids, params.distance)?;
            Ok(result)
        }

        Operation::Shell { params } => {
            let solid_handle = find_latest_solid_handle(feature, feature_results)?;

            let mut face_ids = Vec::new();
            for face_ref in &params.faces_to_remove {
                let resolved = resolve_with_fallback(face_ref, feature_results).map_err(|e| {
                    EngineError::ResolutionFailed {
                        reason: format!("Failed to resolve shell face: {}", e),
                    }
                })?;
                face_ids.push(resolved.kernel_id);
            }

            let result = execute_shell(kb, &solid_handle, &face_ids, params.thickness)?;
            Ok(result)
        }
    }
}

/// Find the most recent solid handle from a feature's references.
///
/// For fillet/chamfer/shell, the edges/faces point to a specific feature's output.
/// We find the solid handle by looking at the first GeomRef's anchor feature_id.
/// If no references are provided, returns an error.
fn find_latest_solid_handle(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<kernel_fork::KernelSolidHandle, EngineError> {
    // Get the target feature from the first edge/face reference
    let first_ref = match &feature.operation {
        Operation::Fillet { params } => params.edges.first(),
        Operation::Chamfer { params } => params.edges.first(),
        Operation::Shell { params } => params.faces_to_remove.first(),
        _ => None,
    };

    let geom_ref = first_ref.ok_or(EngineError::ResolutionFailed {
        reason: "Fillet/chamfer/shell needs at least one edge/face reference".to_string(),
    })?;

    find_solid_handle(geom_ref, feature_results)
}

/// Find the most recent solid handle from features built before the given feature.
///
/// Walks backwards through the feature tree to find the latest OpResult with a Main output.
fn find_most_recent_solid(
    current_feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
) -> Option<kernel_fork::KernelSolidHandle> {
    let active = tree.active_features();
    // Walk backwards from the current feature
    for feature in active.iter().rev() {
        if feature.id == current_feature.id {
            continue;
        }
        if feature.suppressed {
            continue;
        }
        // Skip sketch features (they produce no solid)
        if matches!(&feature.operation, Operation::Sketch { .. }) {
            continue;
        }
        if let Some(result) = feature_results.get(&feature.id) {
            for (key, body_output) in &result.outputs {
                if *key == OutputKey::Main {
                    return Some(body_output.handle.clone());
                }
            }
        }
    }
    None
}

/// Find the Sketch data from a feature in the tree by sketch feature ID.
fn find_sketch_in_tree(sketch_id: Uuid, tree: &FeatureTree) -> Result<&Sketch, EngineError> {
    for feature in &tree.features {
        if feature.id == sketch_id {
            if let Operation::Sketch { sketch } = &feature.operation {
                return Ok(sketch);
            }
        }
    }
    Err(EngineError::SketchNotFound { id: sketch_id })
}

/// Find a sketch OpResult by sketch ID. Sketches produce empty OpResults
/// but need to exist in the tree.
fn find_sketch_result(
    sketch_id: Uuid,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<&OpResult, EngineError> {
    feature_results
        .get(&sketch_id)
        .ok_or(EngineError::SketchNotFound { id: sketch_id })
}

/// Find the solid handle from a feature's OpResult via GeomRef.
fn find_solid_handle(
    geom_ref: &waffle_types::GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<kernel_fork::KernelSolidHandle, EngineError> {
    let (feature_id, output_key) = match &geom_ref.anchor {
        waffle_types::Anchor::FeatureOutput {
            feature_id,
            output_key,
        } => (*feature_id, output_key),
        _ => {
            return Err(EngineError::ResolutionFailed {
                reason: "Expected FeatureOutput anchor for solid handle".to_string(),
            });
        }
    };

    let op_result = feature_results
        .get(&feature_id)
        .ok_or(EngineError::ResolutionFailed {
            reason: format!("Feature {} not found in results", feature_id),
        })?;

    for (key, body_output) in &op_result.outputs {
        if key == output_key {
            return Ok(body_output.handle.clone());
        }
    }

    Err(EngineError::ResolutionFailed {
        reason: format!(
            "Output key {:?} not found in feature {}",
            output_key, feature_id
        ),
    })
}

/// Compute a tangent X axis from a plane normal.
/// Picks an arbitrary perpendicular vector, avoiding near-parallel with the normal.
fn tangent_x_from_normal(n: [f64; 3]) -> [f64; 3] {
    let up = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let cx = [
        n[1] * up[2] - n[2] * up[1],
        n[2] * up[0] - n[0] * up[2],
        n[0] * up[1] - n[1] * up[0],
    ];
    let len = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if len < 1e-12 {
        return [1.0, 0.0, 0.0];
    }
    [cx[0] / len, cx[1] / len, cx[2] / len]
}

/// Resolve all GeomRef references for a feature, collecting warnings.
///
/// Currently `feature.references` is always empty, so this is
/// forward-compatible plumbing for when features carry explicit refs.
fn resolve_feature_refs(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    warnings: &mut Vec<String>,
) {
    for geom_ref in &feature.references {
        match resolve_with_fallback(geom_ref, feature_results) {
            Ok(resolved) => {
                warnings.extend(resolved.warnings);
            }
            Err(e) => {
                warnings.push(format!(
                    "Feature '{}': reference resolution warning: {}",
                    feature.name, e
                ));
            }
        }
    }
}
