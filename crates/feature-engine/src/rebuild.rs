use std::collections::HashMap;

use modeling_ops::{
    execute_boolean, execute_chamfer, execute_extrude, execute_fillet, execute_revolve,
    execute_shell, BooleanKind, OpResult,
};
use uuid::Uuid;
use waffle_types::kernel::units::TAU_WORK;

use crate::resolve::{resolve_by_position, resolve_with_fallback};
use crate::types::{
    normalize_extrude_combine, BooleanOp, CombineMode, DepthMode, EngineError, Feature,
    FeatureTree, Operation, PlaneDefinition, SecondDirection, TargetStrategy,
};
use modeling_ops::KernelBundle;
use waffle_types::kernel::KernelIntrospect;
use waffle_types::{
    OutputKey, ProjectedKind, ProjectedSource, Selector, Sketch, SketchEntity, SketchPlaneBasis,
    TopoKind,
};

/// Reproject a sketch's externally-driven points (projection feature). For each
/// binding, resolve the source geometry against the features built so far and
/// overwrite the bound Point's `(x, y)` with the source position in
/// sketch-plane coordinates. Sources that cannot be resolved are left at their
/// last position (BestEffort dangling). Only Point entity coords are mutated;
/// the caller clears and recomputes derived data afterward.
/// See `specs/projected_sketch_geometry.md`.
pub(crate) fn reproject_sketch(
    sketch: &mut Sketch,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) {
    if sketch.projected.is_empty() {
        return;
    }
    let basis = SketchPlaneBasis::from_origin_normal(sketch.plane_origin, sketch.plane_normal);

    // Resolve all bindings first, then apply (avoids overlapping borrows).
    let mut updates: Vec<(u32, f64, f64)> = Vec::new();
    for binding in &sketch.projected {
        if let Some(p3) = resolve_projected_point(&binding.source, feature_results, introspect) {
            let (u, v) = basis.world_to_local(p3);
            updates.push((binding.point_id, u, v));
        }
    }
    for (pid, u, v) in updates {
        for e in &mut sketch.entities {
            if let SketchEntity::Point { id, x, y, .. } = e {
                if *id == pid {
                    *x = u;
                    *y = v;
                }
            }
        }
    }
}

/// Resolve a projected source to a 3D world point: the source vertex's position,
/// or a point sampled along the source edge at parameter `t`.
fn resolve_projected_point(
    source: &ProjectedSource,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn KernelIntrospect,
) -> Option<[f64; 3]> {
    let pos = match &source.geom_ref.selector {
        Selector::Position { x, y, z } => [*x, *y, *z],
        _ => return None,
    };
    let resolved = resolve_by_position(&source.geom_ref, feature_results, introspect, pos).ok()?;
    let kid = resolved.kernel_id;
    match &source.kind {
        ProjectedKind::Vertex => introspect.compute_signature(kid, TopoKind::Vertex).centroid,
        ProjectedKind::EdgeSample { t } => {
            let (a, b) = introspect.edge_vertices(kid);
            let pa = introspect.compute_signature(a, TopoKind::Vertex).centroid?;
            let pb = introspect.compute_signature(b, TopoKind::Vertex).centroid?;
            Some([
                pa[0] + (pb[0] - pa[0]) * t,
                pa[1] + (pb[1] - pa[1]) * t,
                pa[2] + (pb[2] - pa[2]) * t,
            ])
        }
    }
}

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
    /// KV13 F6: persistent-id → the feature that INTRODUCED it. Captured per
    /// feature right after its op runs (before later ops churn the arena), by
    /// querying `face_provenance` for the faces it created. A face's
    /// *creating* feature is then `pid_to_feature[face_provenance(face).root_pid]`
    /// — the original extrude/revolve, resolved through chained booleans.
    /// (Populated only for features executed this pass; the kernel must track
    /// persistent ids — empty under `MockKernel`.)
    pub pid_to_feature: HashMap<u64, Uuid>,
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
        pid_to_feature: HashMap::new(),
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
            let consumed_ids = find_consumed_feature_ids(
                feature,
                &state.feature_results,
                tree,
                &state.consumed_features,
                Some(kb.as_introspect()),
            );
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
        let consumed_ids = find_consumed_feature_ids(
            feature,
            &state.feature_results,
            tree,
            &state.consumed_features,
            Some(kb.as_introspect()),
        );

        match execute_feature(
            feature,
            kb,
            &state.feature_results,
            tree,
            &state.consumed_features,
        ) {
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
                // KV13 F6: capture this feature's created-face persistent ids
                // NOW (its faces are current; later ops churn the arena).
                capture_face_pids(&mut state, feature.id, &result, kb);
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

/// KV13 F6: record each FACE a feature created → the feature, keyed by the
/// face's persistent id. Called while the faces are current (right after the
/// op), so the kernel ids are valid. The kernel must track persistent ids
/// (`face_provenance`); under `MockKernel` it returns `None` and nothing is
/// recorded.
fn capture_face_pids(
    state: &mut RebuildState,
    feature_id: Uuid,
    result: &OpResult,
    kb: &dyn KernelBundle,
) {
    let introspect = kb.as_introspect();
    // Enumerate the feature's OUTPUT-body faces (current right now) rather than
    // `provenance.created` — robust regardless of how the op records created
    // entities. A boolean's output faces carry FRESH pids (mapped to this
    // feature); their lineage roots are earlier constructor pids (mapped to
    // those features when they ran), so the root lookup still resolves to the
    // introducing feature.
    for (_key, body) in &result.outputs {
        for face in introspect.list_faces(&body.handle) {
            if let Some(prov) = introspect.face_provenance(face) {
                // The output face's own (fresh) pid → this feature.
                state.pid_to_feature.insert(prov.pid, feature_id);
                // Its lineage ROOT → this feature, but only if unclaimed: a
                // root introduced WITHIN this op (e.g. a sub-step extrude that
                // an auto-union then consumed — never surfaced as an output)
                // is claimed here, while a root from an EARLIER feature (claimed
                // when that feature ran) is preserved. `or_insert` =
                // first-claimant-wins, and features run in order, so the
                // introducing feature always wins.
                state
                    .pid_to_feature
                    .entry(prov.root_pid)
                    .or_insert(feature_id);
            }
        }
    }
}

/// Execute a single feature's operation.
fn execute_feature(
    feature: &Feature,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
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
            resolve_plane_definition(
                &params.definition,
                tree,
                feature_results,
                kb.as_introspect(),
            )?;
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
            // Projection: re-derive externally-driven point positions from the
            // features built so far, then force a fresh recompute of derived data
            // (positions + profiles) from the updated points.
            if !sketch_expanded.projected.is_empty() {
                reproject_sketch(&mut sketch_expanded, feature_results, kb.as_introspect());
                sketch_expanded.solved_positions.clear();
                sketch_expanded.solved_profiles.clear();
            }
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

            // Normalize the boolean-combine choice once (Constitution §7) and
            // resolve its target bodies up front: the cut-direction reversal
            // below needs the primary target, and the final dispatch reuses the
            // same set. See specs/optional_booleans_multibody_extrude.md.
            let eff = normalize_extrude_combine(params);
            let is_cut = matches!(eff.mode, CombineMode::Cut);
            let mut combine_warnings: Vec<String> = Vec::new();
            let combine_targets = match eff.mode {
                CombineMode::NewBody => Vec::new(),
                _ => match &eff.targets {
                    // Auto default: bodies that share a face with the sketch.
                    TargetStrategy::ShareAFace => resolve_share_a_face(
                        params.sketch_id,
                        params.profile_index,
                        params.region.as_ref(),
                        &params.regions,
                        feature,
                        feature_results,
                        tree,
                        already_consumed,
                        Some(kb.as_introspect()),
                    ),
                    _ => resolve_combine_targets(
                        &eff.targets,
                        feature,
                        feature_results,
                        tree,
                        &mut combine_warnings,
                    )?,
                },
            };

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
            let should_reverse_for_cut = if is_cut && params.direction.is_none() {
                if let Some((_, target_handle)) = combine_targets.first() {
                    let verts = kb.list_vertices(target_handle);
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
            let (extrude_direction, extrude_depth, face_origin) = match (is_cut, second_depth) {
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

            // Multi-region selection: the user picked ≥2 sketch regions to extrude
            // as ONE body. Union their 2D footprints FIRST (sketch plane), so
            // adjacent regions with shared / coplanar side walls merge into one
            // clean face — NO 3D boolean, hence no Yang Stage-0 coplanar wall. The
            // union yields one region per connected component; each becomes its own
            // face and the (now provably DISJOINT) component solids are unioned (a
            // disjoint union has no coplanar contact, so it always succeeds).
            let extrude_result = if params.regions.len() >= 2 {
                let merged = waffle_types::union_regions(&params.regions);
                if merged.is_empty() {
                    return Err(EngineError::ResolutionFailed {
                        reason: "multi-region extrude: 2D union produced no area".into(),
                    });
                }
                let mut acc: Option<OpResult> = None;
                for region in &merged {
                    let fid =
                        kb.make_face_from_region(region, face_origin, sketch.plane_normal, x_axis)?;
                    let res = execute_extrude(kb, fid, extrude_direction, extrude_depth, None)?;
                    acc = Some(match acc {
                        None => res,
                        Some(prev) => {
                            let a = prev
                                .outputs
                                .first()
                                .map(|(_, b)| b.handle.clone())
                                .ok_or_else(|| EngineError::ResolutionFailed {
                                    reason: "multi-region extrude: empty solid".into(),
                                })?;
                            let b = res
                                .outputs
                                .first()
                                .map(|(_, b)| b.handle.clone())
                                .ok_or_else(|| EngineError::ResolutionFailed {
                                    reason: "multi-region extrude: empty solid".into(),
                                })?;
                            execute_boolean(kb, &a, &b, BooleanKind::Union)?
                        }
                    });
                }
                acc.expect("merged is non-empty")
            } else {
                // A genuine sub-region of overlapping shapes (annulus, lens, …)
                // carries an explicit boundary that no whole-loop profile_index
                // denotes — build its face directly. Otherwise use the profile list
                // (the analytical path: Profile::circle / exact loops).
                let face_id = if let Some(region) = &params.region {
                    kb.make_face_from_region(region, face_origin, sketch.plane_normal, x_axis)?
                } else {
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
                    face_ids[face_index]
                };

                execute_extrude(kb, face_id, extrude_direction, extrude_depth, None)?
            };

            // Dispatch the boolean combine per the normalized mode + resolved
            // targets. See specs/optional_booleans_multibody_extrude.md §4.
            let mut result =
                dispatch_combine(kb, &eff, &combine_targets, extrude_result, "extrude")?;
            result.diagnostics.warnings.extend(combine_warnings);
            Ok(result)
        }

        Operation::Revolve { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let sketch_ref = find_sketch_in_tree(params.sketch_id, tree)?;
            let mut sketch_expanded = sketch_ref.clone();
            // Projection: re-derive externally-driven point positions from the
            // features built so far, then force a fresh recompute of derived data
            // (positions + profiles) from the updated points.
            if !sketch_expanded.projected.is_empty() {
                reproject_sketch(&mut sketch_expanded, feature_results, kb.as_introspect());
                sketch_expanded.solved_positions.clear();
                sketch_expanded.solved_profiles.clear();
            }
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

            // Dispatch the boolean combine (same model as extrude, spec §4).
            let eff = crate::types::normalize_revolve_combine(params);
            let mut combine_warnings: Vec<String> = Vec::new();
            let combine_targets = match eff.mode {
                CombineMode::NewBody => Vec::new(),
                _ => match &eff.targets {
                    TargetStrategy::ShareAFace => resolve_share_a_face(
                        params.sketch_id,
                        params.profile_index,
                        None,
                        &[],
                        feature,
                        feature_results,
                        tree,
                        already_consumed,
                        Some(kb.as_introspect()),
                    ),
                    _ => resolve_combine_targets(
                        &eff.targets,
                        feature,
                        feature_results,
                        tree,
                        &mut combine_warnings,
                    )?,
                },
            };
            let mut result =
                dispatch_combine(kb, &eff, &combine_targets, revolve_result, "revolve")?;
            result.diagnostics.warnings.extend(combine_warnings);
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
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
    solid: &waffle_types::kernel::KernelSolidHandle,
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
            let (origin, _normal) =
                find_datum_plane_data(*datum_id, tree, feature_results, kb.as_introspect())?;
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
) -> Result<waffle_types::kernel::KernelSolidHandle, EngineError> {
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
pub(crate) fn find_consumed_feature_ids(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
    introspect: Option<&dyn KernelIntrospect>,
) -> Vec<Uuid> {
    match &feature.operation {
        Operation::Extrude { params } => {
            // Consumption follows the normalized combine: NewBody consumes
            // nothing; Add/Cut/Intersect consume their resolved targets (legacy
            // most-recent, or the explicit target features). ShareAFace is
            // resolved in N-mb-3 (until then it errors in dispatch, so its
            // consumption set is empty here).
            let eff = normalize_extrude_combine(params);
            match eff.mode {
                CombineMode::NewBody => vec![],
                CombineMode::Add | CombineMode::Cut | CombineMode::Intersect => {
                    match &eff.targets {
                        TargetStrategy::MostRecentLegacy => {
                            find_most_recent_consumed(feature, feature_results, tree)
                        }
                        // Only targets that actually RESOLVE are consumed. A
                        // dropped (deleted/suppressed) BestEffort target is not
                        // combined, so it must not be marked consumed (which
                        // would leave the consumed set inconsistent with the
                        // bodies dispatch actually merged). A failing Strict
                        // target errors the feature, so its consumption is never
                        // applied anyway.
                        TargetStrategy::Explicit(list) => list
                            .iter()
                            .filter(|gr| find_solid_handle(gr, feature_results).is_ok())
                            .filter_map(|gr| match &gr.anchor {
                                waffle_types::Anchor::FeatureOutput { feature_id, .. } => {
                                    Some(*feature_id)
                                }
                                _ => None,
                            })
                            .collect(),
                        TargetStrategy::ShareAFace => resolve_share_a_face(
                            params.sketch_id,
                            params.profile_index,
                            params.region.as_ref(),
                            &params.regions,
                            feature,
                            feature_results,
                            tree,
                            already_consumed,
                            introspect,
                        )
                        .into_iter()
                        .map(|(id, _)| id)
                        .collect(),
                    }
                }
            }
        }
        Operation::Revolve { params } => {
            // Mirror the extrude consumption: honor the normalized combine.
            let eff = crate::types::normalize_revolve_combine(params);
            match eff.mode {
                CombineMode::NewBody => vec![],
                CombineMode::Add | CombineMode::Cut | CombineMode::Intersect => {
                    match &eff.targets {
                        TargetStrategy::MostRecentLegacy => {
                            find_most_recent_consumed(feature, feature_results, tree)
                        }
                        TargetStrategy::Explicit(list) => list
                            .iter()
                            .filter(|gr| find_solid_handle(gr, feature_results).is_ok())
                            .filter_map(|gr| match &gr.anchor {
                                waffle_types::Anchor::FeatureOutput { feature_id, .. } => {
                                    Some(*feature_id)
                                }
                                _ => None,
                            })
                            .collect(),
                        TargetStrategy::ShareAFace => resolve_share_a_face(
                            params.sketch_id,
                            params.profile_index,
                            None,
                            &[],
                            feature,
                            feature_results,
                            tree,
                            already_consumed,
                            introspect,
                        )
                        .into_iter()
                        .map(|(id, _)| id)
                        .collect(),
                    }
                }
            }
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
) -> Option<waffle_types::kernel::KernelSolidHandle> {
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
) -> Result<waffle_types::kernel::KernelSolidHandle, EngineError> {
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

/// Apply a normalized boolean combine (NewBody / Add / Cut / Intersect) of a
/// freshly-built tool solid against a resolved target set, shared by extrude and
/// revolve. `op_name` only flavors the legacy empty-Cut error wording.
/// See `specs/optional_booleans_multibody_extrude.md` §4.
fn dispatch_combine(
    kb: &mut dyn KernelBundle,
    eff: &crate::types::EffectiveCombine,
    combine_targets: &[(Uuid, waffle_types::kernel::KernelSolidHandle)],
    tool_result: OpResult,
    op_name: &str,
) -> Result<OpResult, EngineError> {
    let tool_handle = tool_result
        .outputs
        .first()
        .map(|(_, b)| b.handle.clone())
        .ok_or_else(|| EngineError::ResolutionFailed {
            reason: format!("{op_name} produced no solid output"),
        })?;

    match eff.mode {
        // Standalone body — no boolean.
        CombineMode::NewBody => Ok(tool_result),

        // Union the tool into every target, folding them into one body.
        CombineMode::Add => {
            if combine_targets.is_empty() {
                // Add-into-nothing is a benign no-op union → standalone,
                // SILENTLY (spec §4.1, amended 2026-07-11): the dialog
                // defaults to Add, so every first extrude on an empty
                // document takes this branch, and a warning baked into the
                // feature's diagnostics replays on every rebuild.
                return Ok(tool_result);
            }
            // Spec §4.2: the result is the SET union `(b0 ∪ b1 ∪ …) ∪ tool`,
            // order-independent, with every resulting body emitted. A pairwise
            // union of DISJOINT operands legitimately returns MULTIPLE lumps
            // (kernel `boolean_union_multi`), so a fold that keeps only
            // `.outputs.first()` silently loses target material (C0079-F1).
            // Fold connected components instead:
            //  1. targets → pairwise-disjoint lumps (a multi-lump union step
            //     proves the pair disjoint; keep the original operands);
            //  2. sweep the tool through the lumps, merging every lump it
            //     touches (this is what bridges disjoint targets);
            //  3. lumps still disjoint from the tool survive as their own
            //     output bodies, with a warning — never dropped.
            let legacy = matches!(eff.targets, TargetStrategy::MostRecentLegacy);

            // 1. Fold the targets into pairwise-disjoint lumps. An incoming
            //    target merges with every existing lump it touches (merging
            //    cannot connect two existing lumps to each other: those are
            //    already pairwise disjoint, so only the incoming material
            //    bridges — a forward scan suffices).
            let mut lumps: Vec<(Vec<Uuid>, waffle_types::kernel::KernelSolidHandle)> = Vec::new();
            for (fid, handle) in combine_targets {
                let mut lump = (vec![*fid], handle.clone());
                let mut i = 0;
                while i < lumps.len() {
                    let res = execute_boolean(kb, &lumps[i].1, &lump.1, BooleanKind::Union)?;
                    match res.outputs.len() {
                        0 => {
                            return Err(EngineError::ResolutionFailed {
                                reason: "Add: target union produced no solid".into(),
                            });
                        }
                        1 => {
                            let merged = res.outputs.into_iter().next().unwrap().1.handle;
                            let (mut ids, _) = lumps.remove(i);
                            ids.append(&mut lump.0);
                            lump = (ids, merged);
                            // continue at the same index (next un-scanned lump)
                        }
                        // Disjoint: the split result duplicates the operands —
                        // discard it and keep the originals.
                        _ => i += 1,
                    }
                }
                lumps.push(lump);
            }

            // 2. Sweep the tool through the lumps. Operand order (target,
            //    tool) matches the pre-existing single-target fold so the
            //    legacy path stays byte-identical.
            let mut acc = tool_handle.clone();
            let mut merged_result: Option<OpResult> = None;
            let mut leftovers: Vec<(Vec<Uuid>, waffle_types::kernel::KernelSolidHandle)> =
                Vec::new();
            for (ids, handle) in lumps {
                match execute_boolean(kb, &handle, &acc, BooleanKind::Union) {
                    Ok(res) => match res.outputs.len() {
                        0 => {
                            return Err(EngineError::ResolutionFailed {
                                reason: "Add: tool union produced no solid".into(),
                            });
                        }
                        1 => {
                            acc = res.outputs[0].1.handle.clone();
                            merged_result = Some(res);
                        }
                        // Tool-disjoint lump: keep the ORIGINAL operand.
                        _ => leftovers.push((ids, handle)),
                    },
                    // Legacy auto-union degrades to a standalone body on failure
                    // (byte-identical to pre-N-mb-2); an explicit Add propagates.
                    Err(e) if legacy => {
                        let mut result = tool_result;
                        result.diagnostics.warnings.push(format!(
                            "Auto-union failed: {}. Body created as standalone.",
                            e
                        ));
                        return Ok(result);
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            // 3. Assemble: the tool's connected component is the base result;
            //    tool-disjoint lumps follow as Body{index} outputs + warning.
            let mut result = match merged_result {
                Some(res) => res,
                // Every lump was disjoint from the tool: the tool itself is a
                // standalone component.
                None => tool_result,
            };
            if !leftovers.is_empty() {
                let names: Vec<String> = leftovers
                    .iter()
                    .flat_map(|(ids, _)| ids.iter().map(|id| id.to_string()))
                    .collect();
                result.diagnostics.warnings.push(format!(
                    "Add: target body(ies) [{}] remain disjoint from the tool; \
                     kept as separate bodies.",
                    names.join(", ")
                ));
                let base = result.outputs.len();
                for (i, (_, handle)) in leftovers.into_iter().enumerate() {
                    result.outputs.push((
                        OutputKey::Body { index: base + i },
                        modeling_ops::BodyOutput {
                            handle,
                            mesh: None,
                            edges: None,
                        },
                    ));
                }
            }
            Ok(result)
        }

        // Subtract / intersect the tool from each target INDEPENDENTLY, yielding
        // one result body per target (feature scope, spec §4.2).
        CombineMode::Cut | CombineMode::Intersect => {
            let kind = if matches!(eff.mode, CombineMode::Cut) {
                BooleanKind::Subtract
            } else {
                BooleanKind::Intersect
            };
            if combine_targets.is_empty() {
                // Preserve the legacy wording/behavior for a legacy cut with no
                // prior body (invariance); explicit-empty gets the new message.
                let reason = if matches!(eff.targets, TargetStrategy::MostRecentLegacy) {
                    format!("Cut {op_name} requires an existing body to subtract from")
                } else {
                    format!("{:?} requires at least one target body", eff.mode)
                };
                return Err(EngineError::ResolutionFailed { reason });
            }
            let mut bodies = Vec::new();
            let mut created = Vec::new();
            let mut deleted = Vec::new();
            let mut role_assignments = Vec::new();
            // Spec `cut_consumes_body` I2: per-target warnings (e.g. the
            // consumed-body notice for a legitimately EMPTY subtract) must
            // reach the feature's diagnostics — the rebuild loop forwards
            // `OpResult.diagnostics.warnings` into `engine_warnings`.
            let mut warnings = Vec::new();
            for (_, target) in combine_targets {
                let res = execute_boolean(kb, target, &tool_handle, kind)?;
                created.extend(res.provenance.created);
                deleted.extend(res.provenance.deleted);
                role_assignments.extend(res.provenance.role_assignments);
                warnings.extend(res.diagnostics.warnings);
                for (_, body) in res.outputs {
                    bodies.push(body);
                }
            }
            let outputs = bodies
                .into_iter()
                .enumerate()
                .map(|(i, body)| {
                    let key = if i == 0 {
                        OutputKey::Main
                    } else {
                        OutputKey::Body { index: i }
                    };
                    (key, body)
                })
                .collect();
            Ok(OpResult {
                outputs,
                provenance: modeling_ops::Provenance {
                    created,
                    deleted,
                    modified: Vec::new(),
                    role_assignments,
                },
                diagnostics: modeling_ops::Diagnostics {
                    warnings,
                    ..modeling_ops::Diagnostics::default()
                },
            })
        }
    }
}

/// Compute the "share-a-face" auto target set (spec §4.3).
///
/// **N-mb-3a — anchor-ownership path (a):** the body whose face the sketch is
/// drawn on, read from the sketch's `plane` GeomRef anchor. A sketch drawn on a
/// datum plane has a `Datum` anchor and yields no target (⇒ a new body). The
/// geometric plane-coincidence + profile-overlap path (b) and multi-body
/// coincidence are N-mb-3b. Consumed (non-live) bodies are excluded.
#[allow(clippy::too_many_arguments)]
fn resolve_share_a_face(
    sketch_id: Uuid,
    profile_index: usize,
    region: Option<&waffle_types::Region>,
    regions: &[waffle_types::Region],
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
    introspect: Option<&dyn KernelIntrospect>,
) -> Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)> {
    let sketch = match find_sketch_in_tree(sketch_id, tree) {
        Ok(s) => s.clone(),
        Err(_) => return Vec::new(),
    };
    let mut sketch = sketch;
    sketch.recompute_derived(); // no-op if already populated

    let mut out: Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)> = Vec::new();
    let mut seen: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    // (a) Anchor ownership: the sketch is drawn on a body's face → that body,
    // unless it was already consumed by an earlier feature (not live).
    if let waffle_types::Anchor::FeatureOutput {
        feature_id,
        output_key,
    } = &sketch.plane.anchor
    {
        if !already_consumed.contains(feature_id) {
            if let Some(result) = feature_results.get(feature_id) {
                if let Some((_, body)) = result.outputs.iter().find(|(k, _)| k == output_key) {
                    if seen.insert(*feature_id) {
                        out.push((*feature_id, body.handle.clone()));
                    }
                }
            }
        }
    }

    // (b) Geometric coincidence + profile overlap (needs a kernel to read faces).
    if let Some(intro) = introspect {
        let profile_pts = profile_footprint_2d(&sketch, profile_index, region, regions);
        if profile_pts.len() >= 3 {
            let profile_hull = crate::share_a_face::convex_hull_2d(&profile_pts);
            let s_o = sketch.plane_origin;
            let s_n = sketch.plane_normal;
            let x_axis = tangent_x_from_normal(s_n);
            let y_axis = [
                s_n[1] * x_axis[2] - s_n[2] * x_axis[1],
                s_n[2] * x_axis[0] - s_n[0] * x_axis[2],
                s_n[0] * x_axis[1] - s_n[1] * x_axis[0],
            ];
            // Live bodies from features BEFORE this one, not consumed.
            let active = tree.active_features();
            for f in active.iter().take_while(|f| f.id != feature.id) {
                if f.suppressed || already_consumed.contains(&f.id) || seen.contains(&f.id) {
                    continue;
                }
                if matches!(
                    f.operation,
                    Operation::Sketch { .. } | Operation::DatumPlane { .. }
                ) {
                    continue;
                }
                let Some(result) = feature_results.get(&f.id) else {
                    continue;
                };
                for (key, body) in &result.outputs {
                    if !matches!(key, OutputKey::Main | OutputKey::Body { .. }) {
                        continue;
                    }
                    if body_face_shares_sketch(
                        intro,
                        &body.handle,
                        s_o,
                        s_n,
                        x_axis,
                        y_axis,
                        &profile_hull,
                    ) {
                        if seen.insert(f.id) {
                            out.push((f.id, body.handle.clone()));
                        }
                        break;
                    }
                }
            }
        }
    }
    out
}

/// The extrude profile's 2D footprint (sketch-plane coords) as a point set.
/// Region-based extrudes carry the exact polygon; profile extrudes use the
/// profile's ordered vertices (or a circle's bbox). The caller convex-hulls it.
fn profile_footprint_2d(
    sketch: &Sketch,
    profile_index: usize,
    region: Option<&waffle_types::Region>,
    regions: &[waffle_types::Region],
) -> Vec<[f64; 2]> {
    if let Some(region) = region {
        return region.outer.iter().map(|&(x, y)| [x, y]).collect();
    }
    if !regions.is_empty() {
        return regions
            .iter()
            .flat_map(|r| r.outer.iter().map(|&(x, y)| [x, y]))
            .collect();
    }
    let Some(profile) = sketch.solved_profiles.get(profile_index) else {
        return Vec::new();
    };
    if let Some(circle) = &profile.circle {
        let (u, v, r) = (circle.center_u, circle.center_v, circle.radius);
        return vec![
            [u - r, v - r],
            [u + r, v - r],
            [u + r, v + r],
            [u - r, v + r],
        ];
    }
    let ids: Vec<u32> = if !profile.vertex_ids.is_empty() {
        profile.vertex_ids.clone()
    } else {
        profile
            .entity_ids
            .iter()
            .filter_map(|eid| {
                sketch.entities.iter().find_map(|e| match e {
                    SketchEntity::Line { id, start_id, .. } if id == eid => Some(*start_id),
                    _ => None,
                })
            })
            .collect()
    };
    ids.iter()
        .filter_map(|pid| sketch.solved_positions.get(pid).map(|&(x, y)| [x, y]))
        .collect()
}

/// Does any planar face of `handle` lie in the sketch plane AND overlap the
/// profile footprint (both reduced to convex hulls)? Spec §4.3(b).
fn body_face_shares_sketch(
    intro: &dyn KernelIntrospect,
    handle: &waffle_types::kernel::KernelSolidHandle,
    s_o: [f64; 3],
    s_n: [f64; 3],
    x_axis: [f64; 3],
    y_axis: [f64; 3],
    profile_hull: &[[f64; 2]],
) -> bool {
    for face in intro.list_faces(handle) {
        let sig = intro.compute_signature(face, TopoKind::Face);
        let (Some(fnorm), Some(fc)) = (sig.normal, sig.centroid) else {
            continue;
        };
        if !crate::share_a_face::plane_coincident(s_o, s_n, fnorm, fc) {
            continue;
        }
        let mut pts2d: Vec<[f64; 2]> = Vec::new();
        for edge in intro.face_edges(face) {
            let (v0, v1) = intro.edge_vertices(edge);
            for v in [v0, v1] {
                let vs = intro.compute_signature(v, TopoKind::Vertex);
                if let Some(p) = vs.centroid {
                    let d = [p[0] - s_o[0], p[1] - s_o[1], p[2] - s_o[2]];
                    pts2d.push([
                        d[0] * x_axis[0] + d[1] * x_axis[1] + d[2] * x_axis[2],
                        d[0] * y_axis[0] + d[1] * y_axis[1] + d[2] * y_axis[2],
                    ]);
                }
            }
        }
        if pts2d.len() >= 3 {
            let face_hull = crate::share_a_face::convex_hull_2d(&pts2d);
            if crate::share_a_face::polygons_overlap_2d(profile_hull, &face_hull) {
                return true;
            }
        }
    }
    false
}

/// Resolve a normalized combine's target strategy into the concrete set of
/// `(feature_id, solid handle)` to boolean against. See
/// `specs/optional_booleans_multibody_extrude.md` §4.2/§4.3.
///
/// - `MostRecentLegacy` → the single most-recent solid (0 or 1), preserving the
///   legacy auto-boolean behavior byte-for-byte.
/// - `Explicit(list)` → each listed body, resolved via `find_solid_handle`.
///   A target that fails to resolve (deleted / suppressed / rolled-back body)
///   is handled per its `GeomRef::policy` (spec §9): `Strict` ⇒ a loud
///   `ResolutionFailed`; `BestEffort` ⇒ drop that target and push a warning so
///   the remaining live targets still combine. (Empty list ⇒ empty set.)
/// - `ShareAFace` → **not yet implemented** (sub-increment N-mb-3); a loud STOP.
fn resolve_combine_targets(
    targets: &TargetStrategy,
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    warnings: &mut Vec<String>,
) -> Result<Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)>, EngineError> {
    match targets {
        TargetStrategy::ShareAFace => Err(EngineError::ResolutionFailed {
            reason: "share-a-face auto-target selection is not yet implemented (N-mb-3); \
                     pick explicit target bodies for now"
                .to_string(),
        }),
        TargetStrategy::MostRecentLegacy => {
            match find_most_recent_solid(feature, feature_results, tree) {
                Some(handle) => {
                    // Pair the handle with its feature id for consumption. The
                    // most-recent-solid and most-recent-consumed walks are the
                    // same order, so the first consumed id is this handle's owner.
                    let fid = find_most_recent_consumed(feature, feature_results, tree)
                        .into_iter()
                        .next();
                    Ok(fid.map(|id| vec![(id, handle)]).unwrap_or_default())
                }
                None => Ok(Vec::new()),
            }
        }
        TargetStrategy::Explicit(list) => {
            let mut out = Vec::with_capacity(list.len());
            for gr in list {
                let feature_id = match &gr.anchor {
                    waffle_types::Anchor::FeatureOutput { feature_id, .. } => *feature_id,
                    _ => {
                        return Err(EngineError::ResolutionFailed {
                            reason: "combine target must anchor to a feature output".to_string(),
                        });
                    }
                };
                // Honor the target's ResolvePolicy on failure (spec §9): a Strict
                // target that no longer resolves stops the feature loudly; a
                // BestEffort one is dropped with a warning so the surviving live
                // targets still combine (a valid target must not be lost because
                // an unrelated one was deleted/suppressed).
                match find_solid_handle(gr, feature_results) {
                    Ok(handle) => out.push((feature_id, handle)),
                    Err(e) => match gr.policy {
                        waffle_types::ResolvePolicy::Strict => return Err(e),
                        waffle_types::ResolvePolicy::BestEffort => {
                            warnings.push(format!(
                                "combine target {} could not be resolved and was dropped: {}",
                                feature_id, e
                            ));
                        }
                    },
                }
            }
            Ok(out)
        }
    }
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
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
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
                find_datum_plane_data(*base_plane_id, tree, feature_results, introspect)?;
            let origin = [
                base_origin[0] + base_normal[0] * distance,
                base_origin[1] + base_normal[1] * distance,
                base_origin[2] + base_normal[2] * distance,
            ];
            Ok((origin, base_normal))
        }
        PlaneDefinition::OffsetFromFace { base, distance } => {
            let (base_origin, base_normal) = resolve_face_plane(base, feature_results, introspect)?;
            let origin = [
                base_origin[0] + base_normal[0] * distance,
                base_origin[1] + base_normal[1] * distance,
                base_origin[2] + base_normal[2] * distance,
            ];
            Ok((origin, base_normal))
        }
    }
}

/// Resolve a planar face GeomRef to its base plane `(origin, normal)`.
///
/// `origin` is a deterministic point ON the face (the average of the face's
/// boundary vertices, from `compute_signature`); `normal` is the planar face's
/// outward normal. Because the face is resolved from the *current* geometry
/// every rebuild, a face-base datum plane tracks the face as it moves — it is
/// not frozen at creation.
///
/// A non-planar base face is a loud `ResolutionFailed` — never a guessed plane.
///
/// JS agreement (`computeFacePlane` in `store.svelte.js`): both resolvers
/// return the SAME outward `normal` (the planar face normal) and an `origin`
/// that lies ON the same planar face — so the offset `origin + normal*distance`
/// lands on the SAME infinite plane in both. (JS uses the first rendered
/// triangle's centroid; the engine uses the face-centroid from
/// `compute_signature`. The `KernelIntrospect` contract does not expose raw
/// per-face triangles, and feature-engine must not reach into kernel-v2
/// internals, so the two on-face origin points may differ by an in-plane
/// translation — they are guaranteed coplanar, which is the property the
/// offset depends on.)
fn resolve_face_plane(
    base: &waffle_types::GeomRef,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
) -> Result<([f64; 3], [f64; 3]), EngineError> {
    let resolved = resolve_with_fallback(base, feature_results)?;
    let sig = introspect.compute_signature(resolved.kernel_id, TopoKind::Face);

    match sig.surface_type.as_deref() {
        Some("planar") => {}
        other => {
            return Err(EngineError::ResolutionFailed {
                reason: format!(
                    "Datum plane base face is not planar (surface_type: {})",
                    other.unwrap_or("unknown")
                ),
            });
        }
    }

    let normal = sig.normal.ok_or_else(|| EngineError::ResolutionFailed {
        reason: "Datum plane base face has no normal".into(),
    })?;
    let origin = sig.centroid.ok_or_else(|| EngineError::ResolutionFailed {
        reason: "Datum plane base face has no centroid".into(),
    })?;

    let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if len < TAU_WORK {
        return Err(EngineError::ResolutionFailed {
            reason: "Datum plane base face normal is zero-length".into(),
        });
    }
    let n = [normal[0] / len, normal[1] / len, normal[2] / len];
    Ok((origin, n))
}

/// Look up origin and normal for a datum plane by its UUID.
///
/// Checks the three built-in planes first, then searches the feature tree
/// for user-created DatumPlane features.
fn find_datum_plane_data(
    datum_id: Uuid,
    tree: &FeatureTree,
    feature_results: &HashMap<Uuid, OpResult>,
    introspect: &dyn waffle_types::kernel::KernelIntrospect,
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
                return resolve_plane_definition(
                    &params.definition,
                    tree,
                    feature_results,
                    introspect,
                );
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
    use waffle_types::{GeomRef, ResolvePolicy, Selector};

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
            ..Default::default()
        };
        let results = HashMap::new();
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
        );
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
            ..Default::default()
        };
        let results = HashMap::new();

        // Verify execution succeeds
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
        );
        assert!(result.is_ok(), "Offset from built-in should succeed");

        // Verify resolution
        let mk = waffle_types::kernel::MockKernel::new();
        let (origin, normal) =
            resolve_plane_definition(&def, &tree, &results, mk.as_introspect()).unwrap();
        assert!(
            (origin[2] - 10.0).abs() < 1e-10,
            "Origin Z should be offset by 10"
        );
        assert!((normal[2] - 1.0).abs() < 1e-10, "Normal should be [0,0,1]");
    }

    /// Build a box solid (rectangle sketch → extrude) and return the rebuild
    /// state plus the extrude feature id, so face-base datum tests have real
    /// MockKernel faces to resolve against.
    fn make_box_rebuild() -> (FeatureTree, Uuid, RebuildState) {
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
        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(&tree, &mut kb, 0, &existing);
        (tree, extrude_id, state)
    }

    #[test]
    fn test_offset_from_face_matches_equivalent_offset_from_plane() {
        // A face-base offset datum plane must resolve to (base-face plane
        // origin + distance*normal), and must produce the SAME plane as an
        // explicit PointNormal offset from that same base face plane. This is
        // the dual-resolver agreement contract in plane space: same normal,
        // and the offset origin lies the correct signed distance off the face
        // along its normal.
        let (tree, extrude_id, state) = make_box_rebuild();
        assert!(
            !state.errors.iter().any(|(id, _)| *id == extrude_id),
            "box extrude should succeed: {:?}",
            state.errors
        );

        // Build the kernel fresh and re-resolve so we hold a live introspect.
        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(&tree, &mut kb, 0, &existing);
        let extrude_result = state
            .feature_results
            .get(&extrude_id)
            .expect("extrude result");

        // Pick a planar face from the extrude provenance.
        let face = extrude_result
            .provenance
            .created
            .iter()
            .find(|e| {
                e.kind == TopoKind::Face && e.signature.surface_type.as_deref() == Some("planar")
            })
            .expect("extrude should create at least one planar face");

        // GeomRef pinned to that exact face by its own signature.
        let face_ref = GeomRef {
            kind: TopoKind::Face,
            anchor: waffle_types::Anchor::FeatureOutput {
                feature_id: extrude_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Signature {
                signature: face.signature.clone(),
            },
            policy: ResolvePolicy::Strict,
        };

        let introspect = kb.as_introspect();
        let distance = 0.02_f64;

        // Resolve the base face plane directly (the "must match" reference).
        let (base_origin, base_normal) =
            resolve_face_plane(&face_ref, &state.feature_results, introspect).unwrap();

        // (a) offset-from-face
        let off_face = PlaneDefinition::OffsetFromFace {
            base: face_ref.clone(),
            distance,
        };
        let (face_origin, face_n) =
            resolve_plane_definition(&off_face, &tree, &state.feature_results, introspect).unwrap();

        // (b) equivalent explicit PointNormal offset from the same base plane.
        let expected_origin = [
            base_origin[0] + base_normal[0] * distance,
            base_origin[1] + base_normal[1] * distance,
            base_origin[2] + base_normal[2] * distance,
        ];

        // Normals identical.
        for k in 0..3 {
            assert!(
                (face_n[k] - base_normal[k]).abs() < 1e-12,
                "normal mismatch axis {}: {} vs {}",
                k,
                face_n[k],
                base_normal[k]
            );
        }
        // Origin = base origin + distance*normal.
        for k in 0..3 {
            assert!(
                (face_origin[k] - expected_origin[k]).abs() < 1e-12,
                "origin mismatch axis {}: {} vs {}",
                k,
                face_origin[k],
                expected_origin[k]
            );
        }

        // The resolved offset origin lies exactly `distance` off the face
        // along the normal: (origin - base_origin)·normal == distance.
        let signed = (face_origin[0] - base_origin[0]) * base_normal[0]
            + (face_origin[1] - base_origin[1]) * base_normal[1]
            + (face_origin[2] - base_origin[2]) * base_normal[2];
        assert!(
            (signed - distance).abs() < 1e-12,
            "signed offset should equal distance: {} vs {}",
            signed,
            distance
        );

        // A negative distance flips to the back side (no separate variant).
        let off_back = PlaneDefinition::OffsetFromFace {
            base: face_ref,
            distance: -distance,
        };
        let (back_origin, _) =
            resolve_plane_definition(&off_back, &tree, &state.feature_results, introspect).unwrap();
        let signed_back = (back_origin[0] - base_origin[0]) * base_normal[0]
            + (back_origin[1] - base_origin[1]) * base_normal[1]
            + (back_origin[2] - base_origin[2]) * base_normal[2];
        assert!(
            (signed_back + distance).abs() < 1e-12,
            "negative distance should offset to the back side"
        );
    }

    #[test]
    fn test_offset_from_non_planar_face_is_resolution_failed() {
        // A non-planar base face must be a loud ResolutionFailed, never a
        // guessed plane. We fillet the box to introduce a cylindrical face
        // (MockKernel models a fillet face as "cylindrical"), then attempt to
        // base an offset datum plane on it.
        use waffle_types::kernel::Kernel;

        let (_tree, extrude_id, state) = make_box_rebuild();
        let extrude_result = state
            .feature_results
            .get(&extrude_id)
            .expect("extrude result");
        let handle = extrude_result
            .outputs
            .first()
            .map(|(_, body)| body.handle.clone())
            .expect("extrude main output");

        // Re-run on a fresh kernel so the handle is live in `kb`.
        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(&_tree, &mut kb, 0, &existing);
        let handle = state
            .feature_results
            .get(&extrude_id)
            .and_then(|r| r.outputs.first())
            .map(|(_, body)| body.handle.clone())
            .unwrap_or(handle);

        let edges = kb.as_introspect().list_edges(&handle);
        let edge0 = *edges.first().expect("box has edges");
        let filleted = kb.fillet_edges(&handle, &[edge0], 0.001).unwrap();

        let intro = kb.as_introspect();
        let faces = intro.list_faces(&filleted);
        let cyl_face = faces
            .iter()
            .copied()
            .find(|&f| {
                intro
                    .compute_signature(f, TopoKind::Face)
                    .surface_type
                    .as_deref()
                    == Some("cylindrical")
            })
            .expect("fillet should create a cylindrical face");

        // Expose that real kernel face through a synthetic OpResult so
        // resolve_with_fallback can find it by signature.
        let feat_id = Uuid::new_v4();
        let cyl_sig = intro.compute_signature(cyl_face, TopoKind::Face);
        let mut results = HashMap::new();
        results.insert(
            feat_id,
            modeling_ops::OpResult {
                outputs: vec![],
                provenance: modeling_ops::Provenance {
                    created: vec![modeling_ops::EntityRecord {
                        kernel_id: cyl_face,
                        kind: TopoKind::Face,
                        signature: cyl_sig.clone(),
                    }],
                    deleted: vec![],
                    modified: vec![],
                    role_assignments: vec![],
                },
                diagnostics: modeling_ops::Diagnostics::default(),
            },
        );

        let cyl_ref = GeomRef {
            kind: TopoKind::Face,
            anchor: waffle_types::Anchor::FeatureOutput {
                feature_id: feat_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Signature { signature: cyl_sig },
            policy: ResolvePolicy::Strict,
        };

        let err = resolve_face_plane(&cyl_ref, &results, kb.as_introspect()).unwrap_err();
        assert!(
            matches!(err, EngineError::ResolutionFailed { .. }),
            "non-planar base must be ResolutionFailed, got: {:?}",
            err
        );
        let msg = err.to_string();
        assert!(
            msg.contains("not planar"),
            "error should be loud about non-planar base, got: {}",
            msg
        );
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
            ..Default::default()
        };
        let results = HashMap::new();
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
        );
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
            ..Default::default()
        };
        let results = HashMap::new();
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
        );
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
            projected: vec![],
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
                            combine: None,
                            targets: None,
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
                            region: None,
                            regions: Vec::new(),
                        },
                    },
                    suppressed: false,
                    references: vec![],
                },
            ],
            active_index: None,
            ..Default::default()
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

        let mut kb = waffle_types::kernel::MockKernel::new();
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

        let mut kb = waffle_types::kernel::MockKernel::new();
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

        let mut kb = waffle_types::kernel::MockKernel::new();
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
            ..Default::default()
        };

        // Build results for first plane
        let mut results = HashMap::new();
        let r1 = execute_feature(
            &first_plane,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
        )
        .unwrap();
        results.insert(first_plane.id, r1);

        // Resolve the second plane's definition
        let mk = waffle_types::kernel::MockKernel::new();
        let (origin, normal) =
            resolve_plane_definition(&second_plane_def, &tree, &results, mk.as_introspect())
                .unwrap();
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

    /// Projection integration (incr 4): a sketch point bound to a box vertex
    /// reprojects to that vertex, and tracks it when an upstream edit moves it.
    #[test]
    fn reproject_tracks_moved_source_vertex() {
        use modeling_ops::{BodyOutput, Diagnostics, Provenance};
        use waffle_types::kernel::{Kernel, KernelSolidHandle, MockKernel};
        use waffle_types::{
            Anchor, ClosedProfile, GeomRef, ProjectedEntity, ProjectedKind, ProjectedSource,
            ResolvePolicy, SolveStatus,
        };

        fn box_kernel(corners: [(f64, f64); 4], height: f64) -> (MockKernel, KernelSolidHandle) {
            let mut kernel = MockKernel::new();
            let profile = ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            };
            let mut positions = HashMap::new();
            for (i, c) in corners.iter().enumerate() {
                positions.insert((i + 1) as u32, *c);
            }
            let face_ids = kernel
                .make_faces_from_profiles(
                    &[profile],
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [1.0, 0.0, 0.0],
                    &positions,
                )
                .unwrap();
            let handle = kernel
                .extrude_face(face_ids[0], [0.0, 0.0, 1.0], height)
                .unwrap();
            (kernel, handle)
        }

        let fid = Uuid::new_v4();
        let results_with = |handle: KernelSolidHandle| {
            let mut m: HashMap<Uuid, OpResult> = HashMap::new();
            m.insert(
                fid,
                OpResult {
                    outputs: vec![(
                        OutputKey::Main,
                        BodyOutput {
                            handle,
                            mesh: None,
                            edges: None,
                        },
                    )],
                    provenance: Provenance {
                        created: vec![],
                        deleted: vec![],
                        modified: vec![],
                        role_assignments: vec![],
                    },
                    diagnostics: Diagnostics::default(),
                },
            );
            m
        };

        // GeomRef picking the top vertex over footprint corner (2, 3).
        let gref = GeomRef {
            kind: TopoKind::Vertex,
            anchor: Anchor::FeatureOutput {
                feature_id: fid,
                output_key: OutputKey::Main,
            },
            selector: Selector::Position {
                x: 2.0,
                y: 3.0,
                z: 5.0,
            },
            policy: ResolvePolicy::BestEffort,
        };
        let make_sketch = || Sketch {
            id: Uuid::new_v4(),
            plane: gref.clone(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![SketchEntity::Point {
                id: 100,
                x: 0.0,
                y: 0.0,
                construction: true,
            }],
            constraints: vec![],
            solve_status: SolveStatus::UnderConstrained { dof: 0 },
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            projected: vec![ProjectedEntity {
                point_id: 100,
                source: ProjectedSource {
                    geom_ref: gref.clone(),
                    kind: ProjectedKind::Vertex,
                },
            }],
        };
        let point_xy = |s: &Sketch| -> (f64, f64) {
            for e in &s.entities {
                if let SketchEntity::Point { id, x, y, .. } = e {
                    if *id == 100 {
                        return (*x, *y);
                    }
                }
            }
            panic!("point 100 not found");
        };
        let basis = SketchPlaneBasis::from_origin_normal([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

        // Independently find the nearest vertex to the pick (MockKernel builds a
        // square box of side sqrt(area), so we read actual vertex positions
        // rather than assume the footprint).
        let nearest_pos = |k: &MockKernel, h: &KernelSolidHandle| -> [f64; 3] {
            let pick = [2.0, 3.0, 5.0];
            k.list_vertices(h)
                .into_iter()
                .filter_map(|v| k.compute_signature(v, TopoKind::Vertex).centroid)
                .min_by(|a, b| {
                    let da = (a[0] - pick[0]).powi(2)
                        + (a[1] - pick[1]).powi(2)
                        + (a[2] - pick[2]).powi(2);
                    let db = (b[0] - pick[0]).powi(2)
                        + (b[1] - pick[1]).powi(2)
                        + (b[2] - pick[2]).powi(2);
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap()
        };

        // Box 1.
        let (k1, h1) = box_kernel([(0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 3.0)], 5.0);
        let mut s1 = make_sketch();
        reproject_sketch(&mut s1, &results_with(h1.clone()), k1.as_introspect());
        let (eu1, ev1) = basis.world_to_local(nearest_pos(&k1, &h1));
        let p1 = point_xy(&s1);
        assert!(
            (p1.0 - eu1).abs() < 1e-6 && (p1.1 - ev1).abs() < 1e-6,
            "box1 projection {p1:?} != {:?}",
            (eu1, ev1)
        );

        // Box 2: a larger footprint moves the resolved vertex; the same pick
        // resolves to the (moved) nearest vertex, so the projected point follows.
        let (k2, h2) = box_kernel([(0.0, 0.0), (2.5, 0.0), (2.5, 3.2), (0.0, 3.2)], 5.0);
        let mut s2 = make_sketch();
        reproject_sketch(&mut s2, &results_with(h2.clone()), k2.as_introspect());
        let (eu2, ev2) = basis.world_to_local(nearest_pos(&k2, &h2));
        let p2 = point_xy(&s2);
        assert!(
            (p2.0 - eu2).abs() < 1e-6 && (p2.1 - ev2).abs() < 1e-6,
            "box2 projection {p2:?} != {:?}",
            (eu2, ev2)
        );

        // Parametric: the projected point MOVED because the source moved.
        assert!(
            (p1.0 - p2.0).abs() > 1e-3 || (p1.1 - p2.1).abs() > 1e-3,
            "projected point must track the upstream edit ({p1:?} -> {p2:?})"
        );
    }

    /// Adversarial (incr 7): a projected binding whose source cannot be resolved
    /// (the source feature has no result — deleted/dangling) leaves the bound
    /// point at its last position rather than failing or moving to garbage.
    #[test]
    fn reproject_dangling_source_keeps_last_position() {
        use waffle_types::kernel::MockKernel;
        use waffle_types::{
            Anchor, GeomRef, ProjectedEntity, ProjectedKind, ProjectedSource, ResolvePolicy,
            SolveStatus,
        };

        let gref = GeomRef {
            kind: TopoKind::Vertex,
            anchor: Anchor::FeatureOutput {
                feature_id: Uuid::new_v4(),
                output_key: OutputKey::Main,
            },
            selector: Selector::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            policy: ResolvePolicy::BestEffort,
        };
        let mut sketch = Sketch {
            id: Uuid::new_v4(),
            plane: gref.clone(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![SketchEntity::Point {
                id: 100,
                x: 7.0,
                y: -4.0,
                construction: true,
            }],
            constraints: vec![],
            solve_status: SolveStatus::UnderConstrained { dof: 0 },
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            projected: vec![ProjectedEntity {
                point_id: 100,
                source: ProjectedSource {
                    geom_ref: gref,
                    kind: ProjectedKind::Vertex,
                },
            }],
        };

        // Empty feature_results → the source feature is unresolvable.
        let empty: HashMap<Uuid, OpResult> = HashMap::new();
        let kernel = MockKernel::new();
        reproject_sketch(&mut sketch, &empty, kernel.as_introspect());

        // The point keeps its last (x, y) — nothing moved, no panic.
        match &sketch.entities[0] {
            SketchEntity::Point { x, y, .. } => {
                assert!((*x - 7.0).abs() < 1e-12 && (*y + 4.0).abs() < 1e-12);
            }
            _ => panic!("expected point"),
        }
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
