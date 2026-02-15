use std::collections::HashMap;

use modeling_ops::{
    execute_boolean, execute_chamfer, execute_extrude, execute_fillet, execute_revolve,
    execute_shell, BooleanKind, OpResult,
};
use uuid::Uuid;

use crate::resolve::resolve_with_fallback;
use crate::types::{
    BooleanOp, DepthMode, EngineError, Feature, FeatureTree, Operation, SecondDirection,
};
use modeling_ops::KernelBundle;
use waffle_types::{OutputKey, Sketch, TopoKind};

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

            // Resolve primary depth from depth mode
            let primary_depth = resolve_depth(
                &params.depth_mode,
                params.depth,
                sketch.plane_origin,
                direction,
                feature,
                feature_results,
                tree,
                kb,
            )?;

            // Determine second direction: explicit field takes precedence,
            // then backward-compat symmetric flag
            let effective_second = params.second_direction.clone().or_else(|| {
                if params.symmetric {
                    Some(SecondDirection::Symmetric)
                } else {
                    None
                }
            });

            // Resolve second depth if bidirectional
            let second_depth = match &effective_second {
                Some(SecondDirection::Symmetric) => Some(primary_depth),
                Some(SecondDirection::Blind { depth: d }) => Some(*d),
                Some(SecondDirection::ThroughAll) => {
                    let neg_dir = [-direction[0], -direction[1], -direction[2]];
                    Some(resolve_depth(
                        &DepthMode::ThroughAll,
                        params.depth,
                        sketch.plane_origin,
                        neg_dir,
                        feature,
                        feature_results,
                        tree,
                        kb,
                    )?)
                }
                Some(SecondDirection::UpTo { reference }) => {
                    let neg_dir = [-direction[0], -direction[1], -direction[2]];
                    Some(resolve_depth(
                        &DepthMode::UpTo {
                            reference: reference.clone(),
                        },
                        params.depth,
                        sketch.plane_origin,
                        neg_dir,
                        feature,
                        feature_results,
                        tree,
                        kb,
                    )?)
                }
                None => None,
            };

            // Compute the face origin, extrude direction, and total depth.
            // For bidirectional: create a single extrude from (origin - second_depth * direction)
            // in +direction with total_depth = primary + second. This avoids boolean union.
            // For cut: reverse direction and offset origin by eps to avoid coplanar faces.
            let eps = 0.01;
            let (extrude_direction, extrude_depth, face_origin) = match (params.cut, second_depth) {
                (true, Some(sd)) => {
                    // Cut + bidirectional: tool starts offset behind sketch plane
                    let offset_origin = [
                        sketch.plane_origin[0] + direction[0] * (eps + sd),
                        sketch.plane_origin[1] + direction[1] * (eps + sd),
                        sketch.plane_origin[2] + direction[2] * (eps + sd),
                    ];
                    (
                        [-direction[0], -direction[1], -direction[2]],
                        primary_depth + sd + 2.0 * eps,
                        offset_origin,
                    )
                }
                (true, None) => {
                    let offset_origin = [
                        sketch.plane_origin[0] + direction[0] * eps,
                        sketch.plane_origin[1] + direction[1] * eps,
                        sketch.plane_origin[2] + direction[2] * eps,
                    ];
                    (
                        [-direction[0], -direction[1], -direction[2]],
                        primary_depth + 2.0 * eps,
                        offset_origin,
                    )
                }
                (false, Some(sd)) => {
                    // Non-cut bidirectional: offset origin backward by second_depth
                    let bidir_origin = [
                        sketch.plane_origin[0] - direction[0] * sd,
                        sketch.plane_origin[1] - direction[1] * sd,
                        sketch.plane_origin[2] - direction[2] * sd,
                    ];
                    (direction, primary_depth + sd, bidir_origin)
                }
                (false, None) => (direction, primary_depth, sketch.plane_origin),
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
            let extrude_result = execute_extrude(
                kb,
                face_ids[face_index],
                extrude_direction,
                extrude_depth,
                None,
            )?;

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

/// Resolve depth based on the depth mode.
///
/// - `Blind`: returns `blind_depth` directly
/// - `ThroughAll`: projects target body vertices onto direction, returns max extent + margin
/// - `UpTo`: resolves reference position and computes distance along direction
fn resolve_depth(
    mode: &DepthMode,
    blind_depth: f64,
    sketch_origin: [f64; 3],
    direction: [f64; 3],
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    kb: &mut dyn KernelBundle,
) -> Result<f64, EngineError> {
    match mode {
        DepthMode::Blind => Ok(blind_depth),

        DepthMode::ThroughAll => {
            // Find the target body to measure extent against
            let target = find_most_recent_solid(feature, feature_results, tree);
            match target {
                Some(handle) => {
                    let extent =
                        compute_solid_extent(kb.as_introspect(), &handle, sketch_origin, direction);
                    // Add margin and use safety floor
                    let depth = extent + 1.0;
                    Ok(depth.max(blind_depth.max(1.0)))
                }
                None => {
                    // No target body — use blind depth as fallback with a generous default
                    Ok(blind_depth.max(100.0))
                }
            }
        }

        DepthMode::UpTo { reference } => {
            // Resolve the reference to a 3D position
            let ref_position = resolve_reference_position(reference, feature_results, tree, kb)?;

            // Project reference position and sketch origin onto direction
            let dir_len = (direction[0] * direction[0]
                + direction[1] * direction[1]
                + direction[2] * direction[2])
                .sqrt();
            if dir_len < 1e-12 {
                return Err(EngineError::ResolutionFailed {
                    reason: "Extrude direction is zero-length".into(),
                });
            }
            let dir_norm = [
                direction[0] / dir_len,
                direction[1] / dir_len,
                direction[2] / dir_len,
            ];

            let ref_proj = ref_position[0] * dir_norm[0]
                + ref_position[1] * dir_norm[1]
                + ref_position[2] * dir_norm[2];
            let origin_proj = sketch_origin[0] * dir_norm[0]
                + sketch_origin[1] * dir_norm[1]
                + sketch_origin[2] * dir_norm[2];

            let depth = ref_proj - origin_proj;
            if depth <= 0.0 {
                return Err(EngineError::ResolutionFailed {
                    reason: format!(
                        "UpTo reference is behind sketch plane (depth = {:.3})",
                        depth
                    ),
                });
            }
            Ok(depth)
        }
    }
}

/// Project all vertices of a solid onto a direction vector relative to an origin.
/// Returns the maximum signed projection distance.
fn compute_solid_extent(
    introspect: &dyn kernel_fork::KernelIntrospect,
    solid: &kernel_fork::KernelSolidHandle,
    origin: [f64; 3],
    direction: [f64; 3],
) -> f64 {
    let dir_len =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    if dir_len < 1e-12 {
        return 0.0;
    }
    let dir_norm = [
        direction[0] / dir_len,
        direction[1] / dir_len,
        direction[2] / dir_len,
    ];

    let vertices = introspect.list_vertices(solid);
    let mut max_proj = 0.0f64;

    for vid in &vertices {
        let sig = introspect.compute_signature(*vid, TopoKind::Vertex);
        if let Some(centroid) = sig.centroid {
            let dx = centroid[0] - origin[0];
            let dy = centroid[1] - origin[1];
            let dz = centroid[2] - origin[2];
            let proj = dx * dir_norm[0] + dy * dir_norm[1] + dz * dir_norm[2];
            max_proj = max_proj.max(proj);
        }
    }

    max_proj
}

/// Resolve a GeomRef to a 3D position for UpTo depth mode.
fn resolve_reference_position(
    reference: &waffle_types::GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    kb: &dyn KernelBundle,
) -> Result<[f64; 3], EngineError> {
    // Try to resolve the reference via the feature engine's resolve system
    match &reference.anchor {
        waffle_types::Anchor::FeatureOutput { feature_id, .. } => {
            // Check if we can resolve via the feature result
            let resolved = resolve_with_fallback(reference, feature_results).map_err(|e| {
                EngineError::ResolutionFailed {
                    reason: format!("UpTo reference resolution failed: {}", e),
                }
            })?;

            // Get the centroid of the resolved entity
            let result =
                feature_results
                    .get(feature_id)
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: format!("UpTo reference feature {} not found", feature_id),
                    })?;

            // Get solid handle from the result
            let handle = result
                .outputs
                .first()
                .map(|(_, body)| &body.handle)
                .ok_or_else(|| EngineError::ResolutionFailed {
                    reason: "UpTo reference feature has no solid output".into(),
                })?;

            let introspect = kb.as_introspect();
            let sig = introspect.compute_signature(resolved.kernel_id, reference.kind);
            sig.centroid.ok_or_else(|| EngineError::ResolutionFailed {
                reason: "UpTo reference has no centroid".into(),
            })
        }
        waffle_types::Anchor::Datum { datum_id } => {
            // Datum planes: look up in tree for plane origin
            // Convention: datum planes at origin with standard orientations
            // Check if any feature is a datum plane with this ID
            for f in &tree.features {
                if let Operation::Sketch { sketch } = &f.operation {
                    if let waffle_types::Anchor::Datum { datum_id: did } = &sketch.plane.anchor {
                        if did == datum_id {
                            return Ok(sketch.plane_origin);
                        }
                    }
                }
            }
            // Default datum planes are at origin
            Ok([0.0, 0.0, 0.0])
        }
        _ => Err(EngineError::ResolutionFailed {
            reason: "UpTo reference anchor type not supported".into(),
        }),
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
