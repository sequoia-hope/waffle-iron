use std::collections::HashMap;

use kernel::units::TAU_WORK;
use modeling_ops::{
    execute_boolean, execute_chamfer, execute_extrude, execute_fillet, execute_revolve,
    execute_shell, BooleanKind, OpResult,
};
use uuid::Uuid;

use crate::resolve::resolve_with_fallback;
use crate::types::{
    BooleanOp, DepthMode, EngineError, Feature, FeatureTree, Operation, PlaneDefinition,
    SecondDirection,
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
    /// Feature IDs whose solid was consumed by a later boolean union.
    /// These features should not be rendered (their geometry is merged into the consuming feature).
    pub consumed_features: std::collections::HashSet<Uuid>,
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
        consumed_features: std::collections::HashSet::new(),
    };

    // Carry forward results from features before the rebuild point
    for (id, result) in existing_results {
        state.feature_results.insert(*id, result.clone());
    }

    let active = tree.active_features();

    for (i, feature) in active.iter().enumerate() {
        if feature.suppressed {
            continue;
        }

        if i < from_index {
            // Feature before the rebuild point — not re-executed, but we must
            // re-compute its consumption tracking from its carried-forward result.
            // Without this, incremental rebuilds lose consumption relationships
            // established by earlier features (e.g., e1 consumed by e2's union).
            let consumed_ids = find_consumed_feature_ids(feature, &state.feature_results, tree);
            if !consumed_ids.is_empty() {
                if let Some(result) = state.feature_results.get(&feature.id) {
                    let union_failed = result
                        .diagnostics
                        .warnings
                        .iter()
                        .any(|w| w.contains("Auto-union failed"));
                    if !union_failed {
                        for target_id in consumed_ids {
                            state.consumed_features.insert(target_id);
                        }
                    }
                }
            }
            continue;
        }

        // Resolve any GeomRef references before executing the feature
        resolve_feature_refs(feature, &state.feature_results, &mut state.warnings);

        // Track which features' solids would be consumed by a successful merge/boolean
        let consumed_ids = find_consumed_feature_ids(feature, &state.feature_results, tree);

        match execute_feature(feature, kb, &state.feature_results, tree) {
            Ok(result) => {
                for w in &result.diagnostics.warnings {
                    state.warnings.push(format!("{}: {}", feature.name, w));
                }
                // If this was a merge/boolean that succeeded (no auto-union fallback warning),
                // mark the target features as consumed so they don't render.
                if !consumed_ids.is_empty() {
                    let union_failed = result
                        .diagnostics
                        .warnings
                        .iter()
                        .any(|w| w.contains("Auto-union failed"));
                    if !union_failed {
                        for target_id in &consumed_ids {
                            state.consumed_features.insert(*target_id);
                        }
                    }
                }
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

        Operation::DatumPlane { params } => {
            // Validate the plane definition resolves correctly
            resolve_plane_definition(&params.definition, tree, feature_results)?;
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
            let sketch_ref = find_sketch_in_tree(params.sketch_id, tree)?;
            let mut sketch_expanded = sketch_ref.clone();
            sketch_expanded.recompute_derived();
            let sketch = &sketch_expanded;

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
            //
            // B23: cut_eps removed. The truck coplanar pipeline (containment injection
            // + ring/disc face division) handles exact coplanarity for all tested cases:
            // box-box, box-cylinder, cylinder-cylinder, NURBS circle cuts.
            // Previously cut_eps=0.1 was needed, but B14-B22 fixes resolved the
            // underlying coplanar boolean failures. Verified: BNC1-7, CPC1-4, CPB1-2,
            // CPE1-2, CPU1-2, all boolean_properties/workflows/recovery tests pass.
            let cut_eps = 0.0;
            // For cuts: determine direction based on where the target body is
            // relative to the sketch plane. Project all target body vertices
            // onto the extrude axis to find the body's extent midpoint, then
            // check which side of the sketch plane it falls on.
            //
            // Using vertex positions (not face centroids) gives the true
            // geometric bounding box along the extrude axis, robust against
            // asymmetric face counts or face centroid weighting.
            let should_reverse_for_cut = if params.cut && params.direction.is_none() {
                if let Some(target_handle) = find_most_recent_solid(feature, feature_results, tree)
                {
                    let verts = kb.list_vertices(&target_handle);
                    let sketch_proj = sketch.plane_origin[0] * direction[0]
                        + sketch.plane_origin[1] * direction[1]
                        + sketch.plane_origin[2] * direction[2];
                    let mut proj_min = f64::INFINITY;
                    let mut proj_max = f64::NEG_INFINITY;
                    for &vid in &verts {
                        let sig = kb.compute_signature(vid, TopoKind::Vertex);
                        if let Some(p) = sig.centroid {
                            let proj =
                                p[0] * direction[0] + p[1] * direction[1] + p[2] * direction[2];
                            proj_min = proj_min.min(proj);
                            proj_max = proj_max.max(proj);
                        }
                    }
                    if proj_min.is_finite() && proj_max.is_finite() {
                        let body_mid = (proj_min + proj_max) * 0.5;
                        // Reverse only when body midpoint is behind sketch plane
                        body_mid < sketch_proj
                    } else {
                        true // fallback: legacy reverse behavior
                    }
                } else {
                    true // no target body: legacy reverse behavior
                }
            } else {
                false // explicit direction or non-cut: never auto-reverse
            };
            let (extrude_direction, extrude_depth, face_origin) = match (params.cut, second_depth) {
                (true, Some(sd)) => {
                    if should_reverse_for_cut {
                        let offset_origin = [
                            sketch.plane_origin[0] + direction[0] * (cut_eps + sd),
                            sketch.plane_origin[1] + direction[1] * (cut_eps + sd),
                            sketch.plane_origin[2] + direction[2] * (cut_eps + sd),
                        ];
                        (
                            [-direction[0], -direction[1], -direction[2]],
                            primary_depth + sd + 2.0 * cut_eps,
                            offset_origin,
                        )
                    } else {
                        let offset_origin = [
                            sketch.plane_origin[0] - direction[0] * (cut_eps + sd),
                            sketch.plane_origin[1] - direction[1] * (cut_eps + sd),
                            sketch.plane_origin[2] - direction[2] * (cut_eps + sd),
                        ];
                        (direction, primary_depth + sd + 2.0 * cut_eps, offset_origin)
                    }
                }
                (true, None) => {
                    if should_reverse_for_cut {
                        let offset_origin = [
                            sketch.plane_origin[0] + direction[0] * cut_eps,
                            sketch.plane_origin[1] + direction[1] * cut_eps,
                            sketch.plane_origin[2] + direction[2] * cut_eps,
                        ];
                        (
                            [-direction[0], -direction[1], -direction[2]],
                            primary_depth + 2.0 * cut_eps,
                            offset_origin,
                        )
                    } else {
                        let offset_origin = [
                            sketch.plane_origin[0] - direction[0] * cut_eps,
                            sketch.plane_origin[1] - direction[1] * cut_eps,
                            sketch.plane_origin[2] - direction[2] * cut_eps,
                        ];
                        (direction, primary_depth + 2.0 * cut_eps, offset_origin)
                    }
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
            } else if params.merge {
                // Auto-union boss extrude with existing body
                if let Some(target_handle) = find_most_recent_solid(feature, feature_results, tree)
                {
                    if let Some(tool_handle) = extrude_result
                        .outputs
                        .first()
                        .map(|(_, b)| b.handle.clone())
                    {
                        match execute_boolean(kb, &target_handle, &tool_handle, BooleanKind::Union)
                        {
                            Ok(union_result) => Ok(union_result),
                            Err(e) => {
                                let mut result = extrude_result;
                                result.diagnostics.warnings.push(format!(
                                    "Auto-union failed: {}. Body created as standalone.",
                                    e
                                ));
                                Ok(result)
                            }
                        }
                    } else {
                        Ok(extrude_result)
                    }
                } else {
                    Ok(extrude_result)
                }
            } else {
                // merge=false: standalone body for explicit boolean operations
                Ok(extrude_result)
            }
        }

        Operation::Revolve { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let sketch_ref = find_sketch_in_tree(params.sketch_id, tree)?;
            let mut sketch_expanded = sketch_ref.clone();
            sketch_expanded.recompute_derived();
            let sketch = &sketch_expanded;

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
            let revolve_result = execute_revolve(
                kb,
                face_ids[face_index],
                params.axis_origin,
                params.axis_direction,
                params.angle,
                None,
            )?;

            if params.cut {
                // Find the target body to subtract from
                let target_handle = find_most_recent_solid(feature, feature_results, tree)
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: "Cut revolve requires an existing body to subtract from".into(),
                    })?;

                let tool_handle = revolve_result
                    .outputs
                    .first()
                    .map(|(_, body)| body.handle.clone())
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: "Revolve produced no solid output for cut".into(),
                    })?;

                let boolean_result =
                    execute_boolean(kb, &target_handle, &tool_handle, BooleanKind::Subtract)?;
                Ok(boolean_result)
            } else if params.merge {
                // Auto-union revolve with existing body
                if let Some(target_handle) = find_most_recent_solid(feature, feature_results, tree)
                {
                    if let Some(tool_handle) = revolve_result
                        .outputs
                        .first()
                        .map(|(_, b)| b.handle.clone())
                    {
                        match execute_boolean(kb, &target_handle, &tool_handle, BooleanKind::Union)
                        {
                            Ok(union_result) => Ok(union_result),
                            Err(e) => {
                                let mut result = revolve_result;
                                result.diagnostics.warnings.push(format!(
                                    "Auto-union failed: {}. Body created as standalone.",
                                    e
                                ));
                                Ok(result)
                            }
                        }
                    } else {
                        Ok(revolve_result)
                    }
                } else {
                    Ok(revolve_result)
                }
            } else {
                Ok(revolve_result)
            }
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
            if dir_len < TAU_WORK {
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
    introspect: &dyn kernel::KernelIntrospect,
    solid: &kernel::KernelSolidHandle,
    origin: [f64; 3],
    direction: [f64; 3],
) -> f64 {
    let dir_len =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    if dir_len < TAU_WORK {
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
            let _handle = result
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
            let (origin, _normal) = find_datum_plane_data(*datum_id, tree, feature_results)?;
            Ok(origin)
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
) -> Result<kernel::KernelSolidHandle, EngineError> {
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

/// Find the feature IDs of solids that would be consumed by a merge/boolean.
///
/// Returns IDs of features whose bodies are consumed by this operation.
/// For extrude with merge/cut, this is the single merge target.
/// For BooleanCombine, both body_a and body_b are consumed.
fn find_consumed_feature_ids(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
) -> Vec<Uuid> {
    match &feature.operation {
        Operation::Extrude { params } if params.merge || params.cut => {
            find_most_recent_consumed(feature, feature_results, tree)
        }
        Operation::Revolve { params } if params.merge || params.cut => {
            find_most_recent_consumed(feature, feature_results, tree)
        }
        Operation::BooleanCombine { params } => {
            // Both body_a and body_b are consumed by the boolean result
            let mut consumed = Vec::new();
            if let waffle_types::Anchor::FeatureOutput { feature_id, .. } = &params.body_a.anchor {
                consumed.push(*feature_id);
            }
            if let waffle_types::Anchor::FeatureOutput { feature_id, .. } = &params.body_b.anchor {
                consumed.push(*feature_id);
            }
            consumed
        }
        _ => vec![],
    }
}

/// Find the most recent solid handle from features built before the given feature.
///
/// Walks backwards through the feature tree to find the latest OpResult with a Main output.
fn find_most_recent_solid(
    current_feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
) -> Option<kernel::KernelSolidHandle> {
    let active = tree.active_features();
    // Walk backwards through features BEFORE the current one
    let current_idx = active
        .iter()
        .position(|f| f.id == current_feature.id)
        .unwrap_or(active.len());
    for feature in active[..current_idx].iter().rev() {
        if feature.suppressed {
            continue;
        }
        // Skip sketch and datum plane features (they produce no solid)
        if matches!(
            &feature.operation,
            Operation::Sketch { .. } | Operation::DatumPlane { .. }
        ) {
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

/// Find the most recent feature with a Main solid output (for consumption tracking).
fn find_most_recent_consumed(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
) -> Vec<Uuid> {
    let active = tree.active_features();
    let current_idx = active
        .iter()
        .position(|f| f.id == feature.id)
        .unwrap_or(active.len());
    for f in active[..current_idx].iter().rev() {
        if f.suppressed {
            continue;
        }
        if matches!(
            &f.operation,
            Operation::Sketch { .. } | Operation::DatumPlane { .. }
        ) {
            continue;
        }
        if let Some(result) = feature_results.get(&f.id) {
            if result
                .outputs
                .iter()
                .any(|(key, _)| *key == OutputKey::Main)
            {
                return vec![f.id];
            }
        }
    }
    vec![]
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
) -> Result<kernel::KernelSolidHandle, EngineError> {
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

/// Well-known UUIDs for the three built-in datum planes (must match planes.js).
const FRONT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000001";
const TOP_PLANE_ID: &str = "00000000-0000-0000-0000-000000000002";
const RIGHT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000003";

/// Resolve a PlaneDefinition to (origin, normal).
fn resolve_plane_definition(
    def: &PlaneDefinition,
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    match def {
        PlaneDefinition::PointNormal { origin, normal } => {
            let len =
                (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            if len < TAU_WORK {
                return Err(EngineError::ResolutionFailed {
                    reason: "Datum plane normal is zero-length".into(),
                });
            }
            let n = [normal[0] / len, normal[1] / len, normal[2] / len];
            Ok((*origin, n))
        }
        PlaneDefinition::Offset {
            base_plane_id,
            distance,
        } => {
            let (base_origin, base_normal) =
                find_datum_plane_data(*base_plane_id, tree, feature_results)?;
            let origin = [
                base_origin[0] + base_normal[0] * distance,
                base_origin[1] + base_normal[1] * distance,
                base_origin[2] + base_normal[2] * distance,
            ];
            Ok((origin, base_normal))
        }
    }
}

/// Look up origin and normal for a datum plane by its UUID.
///
/// Checks the three built-in planes first, then searches the feature tree
/// for user-created DatumPlane features.
fn find_datum_plane_data(
    datum_id: Uuid,
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    let id_str = datum_id.to_string();
    // Built-in planes
    if id_str == FRONT_PLANE_ID {
        return Ok(([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
    }
    if id_str == TOP_PLANE_ID {
        return Ok(([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]));
    }
    if id_str == RIGHT_PLANE_ID {
        return Ok(([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    }

    // Search user-created DatumPlane features
    for feature in &tree.features {
        if feature.id == datum_id {
            if let Operation::DatumPlane { params } = &feature.operation {
                return resolve_plane_definition(&params.definition, tree, feature_results);
            }
        }
    }

    Err(EngineError::ResolutionFailed {
        reason: format!("Datum plane {} not found", datum_id),
    })
}

/// Compute a tangent X axis from a plane normal.
/// Must match the JS formula in `sketchCoords.js:buildSketchPlane()`:
///   ref = |n·Z| < 0.99 ? Z : X   (Z=[0,0,1], X=[1,0,0])
///   xAxis = ref × n
fn tangent_x_from_normal(n: [f64; 3]) -> [f64; 3] {
    // Dot product with Z axis: n[0]*0 + n[1]*0 + n[2]*1 = n[2]
    let ref_vec = if n[2].abs() < 0.99 {
        [0.0, 0.0, 1.0] // Z
    } else {
        [1.0, 0.0, 0.0] // X
    };
    // Cross product: ref × n
    let cx = [
        ref_vec[1] * n[2] - ref_vec[2] * n[1],
        ref_vec[2] * n[0] - ref_vec[0] * n[2],
        ref_vec[0] * n[1] - ref_vec[1] * n[0],
    ];
    let len = (cx[0] * cx[0] + cx[1] * cx[1] + cx[2] * cx[2]).sqrt();
    if len < TAU_WORK {
        return [1.0, 0.0, 0.0];
    }
    [cx[0] / len, cx[1] / len, cx[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: compute cross product ref x n manually for verification.
    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    fn length(v: [f64; 3]) -> f64 {
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    // -- Branch table tests: one per row --
    // These directly test the JS formula: ref = |n.z| < 0.99 ? Z : X; xAxis = ref x n
    // Old buggy code produced different results for n=[0,0,1].

    #[test]
    fn xy_plane_normal_matches_js() {
        // n=[0,0,1]: |n.z|=1.0 >= 0.99, so ref=[1,0,0]
        // xAxis = [1,0,0] x [0,0,1] = [0*1-0*0, 0*0-1*1, 1*0-0*0] = [0,-1,0]
        let result = tangent_x_from_normal([0.0, 0.0, 1.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - (-1.0)).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XY plane: expected [0,-1,0], got {:?}",
            result
        );
    }

    #[test]
    fn xy_plane_flipped_normal_matches_js() {
        // n=[0,0,-1]: |n.z|=1.0 >= 0.99, so ref=[1,0,0]
        // xAxis = [1,0,0] x [0,0,-1] = [0*(-1)-0*0, 0*0-1*(-1), 1*0-0*0] = [0,1,0]
        let result = tangent_x_from_normal([0.0, 0.0, -1.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - 1.0).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XY flipped: expected [0,1,0], got {:?}",
            result
        );
    }

    #[test]
    fn xz_plane_normal_matches_js() {
        // n=[0,1,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [0,1,0] = [0*0-1*1, 1*0-0*0, 0*1-0*0] = [-1,0,0]
        let result = tangent_x_from_normal([0.0, 1.0, 0.0]);
        assert!(
            (result[0] - (-1.0)).abs() < 1e-10
                && (result[1]).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XZ plane: expected [-1,0,0], got {:?}",
            result
        );
    }

    #[test]
    fn yz_plane_normal_matches_js() {
        // n=[1,0,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [1,0,0] = [0*0-1*0, 1*1-0*0, 0*0-0*1] = [0,1,0]
        let result = tangent_x_from_normal([1.0, 0.0, 0.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - 1.0).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "YZ plane: expected [0,1,0], got {:?}",
            result
        );
    }

    #[test]
    fn xz_plane_flipped_normal() {
        // n=[0,-1,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [0,-1,0] = [0*0-1*(-1), 1*0-0*0, 0*(-1)-0*0] = [1,0,0]
        let result = tangent_x_from_normal([0.0, -1.0, 0.0]);
        assert!(
            (result[0] - 1.0).abs() < 1e-10
                && (result[1]).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XZ flipped: expected [1,0,0], got {:?}",
            result
        );
    }

    #[test]
    fn yz_plane_flipped_normal() {
        // n=[-1,0,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [-1,0,0] = [0*0-1*0, 1*(-1)-0*0, 0*0-0*(-1)] = [0,-1,0]
        let result = tangent_x_from_normal([-1.0, 0.0, 0.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - (-1.0)).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "YZ flipped: expected [0,-1,0], got {:?}",
            result
        );
    }

    // -- Invariant tests --

    #[test]
    fn perpendicularity_invariant_all_axis_normals() {
        let normals = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        for n in &normals {
            let x = tangent_x_from_normal(*n);
            let d = dot(x, *n);
            assert!(
                d.abs() < 1e-10,
                "Perpendicularity violated for n={:?}: dot={}, x={:?}",
                n,
                d,
                x
            );
            let l = length(x);
            assert!(
                (l - 1.0).abs() < 1e-10,
                "Unit length violated for n={:?}: |x|={}, x={:?}",
                n,
                l,
                x
            );
        }
    }

    #[test]
    fn degenerate_zero_normal_returns_fallback() {
        // Zero-length normal should produce fallback [1,0,0]
        let result = tangent_x_from_normal([0.0, 0.0, 0.0]);
        assert_eq!(
            result,
            [1.0, 0.0, 0.0],
            "Zero normal should fallback to [1,0,0]"
        );
    }

    // -- Cross-validation: verify against manual cross product --

    #[test]
    fn result_matches_manual_cross_product() {
        let normals: [[f64; 3]; 4] = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ];
        for n in &normals {
            let ref_vec = if n[2].abs() < 0.99 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let cx = cross(ref_vec, *n);
            let l = length(cx);
            let expected = if l < 1e-12 {
                [1.0, 0.0, 0.0]
            } else {
                [cx[0] / l, cx[1] / l, cx[2] / l]
            };
            let result = tangent_x_from_normal(*n);
            for i in 0..3 {
                assert!(
                    (result[i] - expected[i]).abs() < 1e-10,
                    "Mismatch for n={:?}: result={:?}, expected={:?}",
                    n,
                    result,
                    expected
                );
            }
        }
    }

    // -- Adversarial: mutation-detecting tests --

    #[test]
    fn flipping_cross_product_order_changes_sign() {
        // If someone changes ref x n to n x ref, the sign flips.
        // For n=[0,0,1], ref=[1,0,0]:
        //   ref x n = [0,-1,0]
        //   n x ref = [0,+1,0]
        // Our function must return [0,-1,0], not [0,+1,0].
        let result = tangent_x_from_normal([0.0, 0.0, 1.0]);
        assert!(
            result[1] < 0.0,
            "xAxis.y must be negative for n=[0,0,1]; got {:?} (would be positive if cross order reversed)",
            result
        );
    }

    #[test]
    fn near_threshold_normal() {
        // n=[0, 0.14, 0.99]: |n.z| = 0.99, exactly at threshold boundary
        // With < 0.99, this picks ref=X (since 0.99 is NOT < 0.99)
        let n = [0.0, 0.14, 0.99];
        let result = tangent_x_from_normal(n);
        // Must be perpendicular regardless of branch
        let d = dot(result, n);
        assert!(
            d.abs() < 1e-6,
            "Near-threshold: perpendicularity failed, dot={}",
            d
        );
        let l = length(result);
        assert!(
            (l - 1.0).abs() < 1e-6,
            "Near-threshold: unit length failed, |x|={}",
            l
        );

        // At exactly 0.99, |n.z| < 0.99 is FALSE, so ref=[1,0,0]
        let ref_vec = [1.0, 0.0, 0.0]; // because 0.99 is NOT < 0.99
        let expected_cross = cross(ref_vec, n);
        let el = length(expected_cross);
        let expected = [
            expected_cross[0] / el,
            expected_cross[1] / el,
            expected_cross[2] / el,
        ];
        for i in 0..3 {
            assert!(
                (result[i] - expected[i]).abs() < 1e-6,
                "Near-threshold: component {} mismatch: got {}, expected {}",
                i,
                result[i],
                expected[i]
            );
        }
    }

    #[test]
    fn oblique_45_degree_normal() {
        // 45-degree oblique: n = normalize([1,1,1])
        let s = 1.0 / (3.0f64).sqrt();
        let n = [s, s, s];
        let result = tangent_x_from_normal(n);

        // |n.z| = 1/sqrt(3) ~ 0.577 < 0.99, so ref=[0,0,1]
        let d = dot(result, n);
        assert!(d.abs() < 1e-10, "45deg: perpendicularity failed, dot={}", d);
        let l = length(result);
        assert!(
            (l - 1.0).abs() < 1e-10,
            "45deg: unit length failed, |x|={}",
            l
        );

        // Verify against manual cross: [0,0,1] x [s,s,s] = [-s, s, 0] normalized
        let cx = cross([0.0, 0.0, 1.0], n);
        let cl = length(cx);
        let expected = [cx[0] / cl, cx[1] / cl, cx[2] / cl];
        for i in 0..3 {
            assert!(
                (result[i] - expected[i]).abs() < 1e-10,
                "45deg: component {} mismatch",
                i
            );
        }
    }

    #[test]
    fn changing_threshold_to_half_breaks_near_threshold() {
        // If threshold were changed to 0.5 instead of 0.99,
        // then n=[0, 0.14, 0.99] would use ref=Z instead of ref=X.
        // This test verifies the actual function uses the 0.99 threshold
        // by checking that n=[0,0,0.98] (just under threshold) uses ref=Z.
        let n_under = [0.0, 0.0, 0.98];
        let result_under = tangent_x_from_normal(n_under);
        // |n.z|=0.98 < 0.99, so ref=[0,0,1]
        // cross([0,0,1], [0,0,0.98]) = [0*0.98-1*0, 1*0-0*0.98, 0*0-0*0] = [0,0,0]
        // This is degenerate (nearly parallel), should fallback to [1,0,0]
        // Actually: cross is near-zero, so fallback applies
        assert_eq!(
            result_under,
            [1.0, 0.0, 0.0],
            "n=[0,0,0.98] should hit Z branch and produce degenerate cross -> fallback"
        );

        // n=[0,0,0.995] is above threshold, uses ref=X
        let n_over = [0.0, 0.0, 0.995];
        let result_over = tangent_x_from_normal(n_over);
        // |n.z|=0.995 >= 0.99, so ref=[1,0,0]
        // cross([1,0,0], [0,0,0.995]) = [0*0.995-0*0, 0*0-1*0.995, 1*0-0*0] = [0,-0.995,0]
        // Normalized: [0,-1,0]
        assert!(
            result_over[1] < -0.99,
            "n=[0,0,0.995] should use X branch; got {:?}",
            result_over
        );
    }

    // -- DatumPlane tests --

    use crate::types::{DatumPlaneParams, PlaneDefinition};

    fn make_datum_plane_feature(name: &str, definition: PlaneDefinition) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            name: name.to_string(),
            operation: Operation::DatumPlane {
                params: DatumPlaneParams {
                    name: name.to_string(),
                    definition,
                },
            },
            suppressed: false,
            references: Vec::new(),
        }
    }

    #[test]
    fn test_datum_plane_point_normal() {
        let feature = make_datum_plane_feature(
            "Custom Plane",
            PlaneDefinition::PointNormal {
                origin: [1.0, 2.0, 3.0],
                normal: [0.0, 1.0, 0.0],
            },
        );
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
        };
        let results = HashMap::new();
        let result = execute_feature(&feature, &mut kernel::MockKernel::new(), &results, &tree);
        assert!(result.is_ok(), "PointNormal datum plane should succeed");
        assert!(
            result.unwrap().outputs.is_empty(),
            "DatumPlane produces no outputs"
        );
    }

    #[test]
    fn test_datum_plane_offset_from_builtin() {
        let front_id: Uuid = FRONT_PLANE_ID.parse().unwrap();
        let def = PlaneDefinition::Offset {
            base_plane_id: front_id,
            distance: 10.0,
        };
        let feature = make_datum_plane_feature("Offset from Front", def.clone());
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
        };
        let results = HashMap::new();

        // Verify execution succeeds
        let result = execute_feature(&feature, &mut kernel::MockKernel::new(), &results, &tree);
        assert!(result.is_ok(), "Offset from built-in should succeed");

        // Verify resolution
        let (origin, normal) = resolve_plane_definition(&def, &tree, &results).unwrap();
        assert!(
            (origin[2] - 10.0).abs() < 1e-10,
            "Origin Z should be offset by 10"
        );
        assert!((normal[2] - 1.0).abs() < 1e-10, "Normal should be [0,0,1]");
    }

    #[test]
    fn test_datum_plane_zero_normal_error() {
        let feature = make_datum_plane_feature(
            "Bad Plane",
            PlaneDefinition::PointNormal {
                origin: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 0.0],
            },
        );
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
        };
        let results = HashMap::new();
        let result = execute_feature(&feature, &mut kernel::MockKernel::new(), &results, &tree);
        assert!(result.is_err(), "Zero normal should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("zero-length"),
            "Error should mention zero-length: {}",
            err
        );
    }

    #[test]
    fn test_datum_plane_missing_base_error() {
        let missing_id = Uuid::new_v4();
        let feature = make_datum_plane_feature(
            "Offset from Missing",
            PlaneDefinition::Offset {
                base_plane_id: missing_id,
                distance: 5.0,
            },
        );
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
        };
        let results = HashMap::new();
        let result = execute_feature(&feature, &mut kernel::MockKernel::new(), &results, &tree);
        assert!(result.is_err(), "Missing base plane should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Error should mention not found: {}",
            err
        );
    }

    // -- Deserialized sketch (empty solved_profiles/solved_positions) tests --
    // These simulate loading a .waffle file where solved_profiles and solved_positions
    // are empty because they are #[serde(default, skip_serializing)].

    /// Helper: create a Sketch with empty derived data (simulating deserialization).
    fn make_deserialized_sketch(id: Uuid, entities: Vec<waffle_types::SketchEntity>) -> Sketch {
        Sketch {
            id,
            plane: waffle_types::GeomRef {
                kind: waffle_types::TopoKind::Face,
                anchor: waffle_types::Anchor::Datum {
                    datum_id: FRONT_PLANE_ID.parse().unwrap(),
                },
                selector: waffle_types::Selector::Role {
                    role: waffle_types::Role::EndCapPositive,
                    index: 0,
                },
                policy: waffle_types::ResolvePolicy::Strict,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities,
            constraints: vec![],
            solve_status: waffle_types::SolveStatus::FullyConstrained,
            // Simulating deserialization: these are empty because skip_serializing
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
        }
    }

    /// Helper: create a sketch Feature + extrude Feature pair for testing.
    fn make_sketch_extrude_tree(sketch: Sketch) -> (FeatureTree, Uuid) {
        let sketch_id = sketch.id;
        let extrude_id = Uuid::new_v4();
        let tree = FeatureTree {
            features: vec![
                Feature {
                    id: sketch_id,
                    name: "Sketch1".to_string(),
                    operation: Operation::Sketch { sketch },
                    suppressed: false,
                    references: vec![],
                },
                Feature {
                    id: extrude_id,
                    name: "Extrude1".to_string(),
                    operation: Operation::Extrude {
                        params: crate::types::ExtrudeParams {
                            sketch_id,
                            profile_index: 0,
                            depth: 0.01,
                            direction: None,
                            symmetric: false,
                            cut: false,
                            merge: false,
                            target_body: None,
                            depth_mode: crate::types::DepthMode::Blind,
                            second_direction: None,
                        },
                    },
                    suppressed: false,
                    references: vec![],
                },
            ],
            active_index: None,
        };
        (tree, extrude_id)
    }

    #[test]
    fn rebuild_recomputes_profiles_for_circle_sketch() {
        // A circle sketch loaded from a .waffle file has empty solved_profiles
        // and solved_positions (they are skip_serializing). Rebuild must
        // recompute them before attempting to extrude.
        use waffle_types::SketchEntity;

        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![
                SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Circle {
                    id: 2,
                    center_id: 1,
                    radius: 0.005, // 5mm in meters
                    construction: false,
                },
            ],
        );

        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);

        let mut kb = kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(&tree, &mut kb, 0, &existing);

        // The extrude should succeed — profiles should have been recomputed from entities.
        // Currently this fails because solved_profiles is empty after deserialization.
        let extrude_failed = state.errors.iter().any(|(id, _)| *id == extrude_id);
        assert!(
            !extrude_failed,
            "Extrude should succeed after profile recomputation, but got errors: {:?}",
            state.errors
        );
        assert!(
            state.feature_results.contains_key(&extrude_id),
            "Extrude feature should have a result"
        );
    }

    #[test]
    fn rebuild_recomputes_profiles_for_rectangle_sketch() {
        // A rectangle sketch (4 points, 4 lines) loaded from a .waffle file.
        // solved_profiles and solved_positions are empty after deserialization.
        use waffle_types::SketchEntity;

        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![
                // Four corner points of a 10mm x 5mm rectangle
                SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 2,
                    x: 0.01,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 3,
                    x: 0.01,
                    y: 0.005,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 4,
                    x: 0.0,
                    y: 0.005,
                    construction: false,
                },
                // Four lines forming the rectangle
                SketchEntity::Line {
                    id: 5,
                    start_id: 1,
                    end_id: 2,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 6,
                    start_id: 2,
                    end_id: 3,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 7,
                    start_id: 3,
                    end_id: 4,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 8,
                    start_id: 4,
                    end_id: 1,
                    construction: false,
                },
            ],
        );

        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);

        let mut kb = kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(&tree, &mut kb, 0, &existing);

        // The extrude should succeed — profiles should have been recomputed.
        // Currently this fails because solved_profiles is empty after deserialization.
        let extrude_failed = state.errors.iter().any(|(id, _)| *id == extrude_id);
        assert!(
            !extrude_failed,
            "Extrude should succeed after profile recomputation, but got errors: {:?}",
            state.errors
        );
        assert!(
            state.feature_results.contains_key(&extrude_id),
            "Extrude feature should have a result"
        );
    }

    #[test]
    fn rebuild_gear_sketch_works_without_precomputed_profiles() {
        // Gear sketches get expand_gears() called during rebuild, which populates
        // solved_profiles and solved_positions. This test confirms that baseline
        // behavior still works even when starting from empty derived data.
        use waffle_types::SketchEntity;

        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![SketchEntity::Gear {
                id: 1,
                params: waffle_types::GearParams {
                    tooth_count: 8,
                    module: 0.01,
                    ..Default::default()
                },
                construction: false,
            }],
        );

        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);

        let mut kb = kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(&tree, &mut kb, 0, &existing);

        // Gear sketches should work because expand_gears() populates profiles.
        let extrude_failed = state.errors.iter().any(|(id, _)| *id == extrude_id);
        assert!(
            !extrude_failed,
            "Gear extrude should succeed via expand_gears(), but got errors: {:?}",
            state.errors
        );
        assert!(
            state.feature_results.contains_key(&extrude_id),
            "Gear extrude feature should have a result"
        );
    }

    #[test]
    fn test_datum_plane_offset_chain() {
        // Create a user DatumPlane, then offset from it
        let first_plane = make_datum_plane_feature(
            "Plane A",
            PlaneDefinition::PointNormal {
                origin: [0.0, 0.0, 5.0],
                normal: [0.0, 0.0, 1.0],
            },
        );
        let second_plane_def = PlaneDefinition::Offset {
            base_plane_id: first_plane.id,
            distance: 7.0,
        };
        let second_plane = Feature {
            id: Uuid::new_v4(),
            name: "Plane B".to_string(),
            operation: Operation::DatumPlane {
                params: DatumPlaneParams {
                    name: "Plane B".to_string(),
                    definition: second_plane_def.clone(),
                },
            },
            suppressed: false,
            references: Vec::new(),
        };

        let tree = FeatureTree {
            features: vec![first_plane.clone(), second_plane.clone()],
            active_index: None,
        };

        // Build results for first plane
        let mut results = HashMap::new();
        let r1 = execute_feature(
            &first_plane,
            &mut kernel::MockKernel::new(),
            &results,
            &tree,
        )
        .unwrap();
        results.insert(first_plane.id, r1);

        // Resolve the second plane's definition
        let (origin, normal) =
            resolve_plane_definition(&second_plane_def, &tree, &results).unwrap();
        assert!(
            (origin[2] - 12.0).abs() < 1e-10,
            "Z should be 5+7=12, got {}",
            origin[2]
        );
        assert!(
            (normal[2] - 1.0).abs() < 1e-10,
            "Normal should still be [0,0,1]"
        );
    }
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
