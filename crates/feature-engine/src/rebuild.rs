use std::collections::HashMap;

use modeling_ops::{
    execute_boolean, execute_chamfer, execute_extrude, execute_fillet, execute_pipe,
    execute_revolve, execute_shell, execute_sweep, BooleanKind, OpResult,
};
use uuid::Uuid;
use waffle_types::kernel::units::TAU_WORK;

use crate::context::EditContext;
use crate::resolve::{resolve_by_position, resolve_with_fallback};
use crate::sources::SourceStore;
use crate::types::{
    normalize_extrude_combine, BooleanOp, CombineMode, DepthMode, EngineError, Feature,
    FeatureTree, ImportedBodyParams, Operation, PlaneDefinition, SecondDirection, SweepParams,
    SweepPathRef, TargetStrategy,
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
    context: Option<&EditContext>,
) {
    if sketch.projected.is_empty() {
        return;
    }
    let basis = sketch_basis(sketch);

    // Resolve all bindings first, then apply (avoids overlapping borrows). A
    // source scoped to another instance (in-context) resolves through the
    // open context, into this part's frame; without one it stays put.
    let mut updates: Vec<(u32, f64, f64)> = Vec::new();
    for binding in &sketch.projected {
        let resolved = if binding.source.geom_ref.scope.is_some() {
            context.and_then(|c| c.projected_point(&binding.source, introspect))
        } else {
            resolve_projected_point(&binding.source, feature_results, introspect)
        };
        if let Some(p3) = resolved {
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
pub(crate) fn resolve_projected_point(
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
    /// The same warnings ATTRIBUTED to the feature that raised them, in the
    /// same order (N2 §5.3): `warnings[i]` is the message of
    /// `feature_warnings[j]` prefixed with the feature's name, for the subset
    /// a feature raised. A host needs the id, not the name — the feature tree
    /// shows a warning glyph on the row, and an agent asks `feature_get` for
    /// one feature's state. A warning no feature owns (a context pass, an
    /// assembly) is in `warnings` only.
    pub feature_warnings: Vec<(Uuid, String)>,
    /// What this rebuild learned about each stored reference it re-resolved,
    /// by owning feature (N2 §5.3 item 4) — which rung answered, whether the
    /// answer was a rebind, and the typed reason when nothing answered. A
    /// kernel-less read like `feature_get` cannot work any of that out, and
    /// guessing it from "did the feature fail?" misreports a sketch that
    /// failed for some other reason.
    pub feature_references: Vec<(Uuid, crate::types::ReferenceState)>,
    /// Features that failed to rebuild, with error messages.
    pub errors: Vec<(Uuid, String)>,
    /// The same errors, typed (ICR-2), in the same order.
    pub feature_errors: Vec<crate::types::FeatureError>,
    /// Feature IDs whose solid was consumed by a later boolean union.
    /// These features should not be rendered (their geometry is merged into the consuming feature).
    pub consumed_features: std::collections::HashSet<Uuid>,
    /// Consumer → the features it consumed, in target order.
    pub consumed_by: HashMap<Uuid, Vec<Uuid>>,
    /// KV13 F6: persistent-id → the feature that INTRODUCED it. Captured per
    /// feature right after its op runs (before later ops churn the arena), by
    /// querying `face_provenance` for the faces it created. A face's
    /// *creating* feature is then `pid_to_feature[face_provenance(face).root_pid]`
    /// — the original extrude/revolve, resolved through chained booleans.
    /// (Populated only for features executed this pass; the kernel must track
    /// persistent ids — empty under `MockKernel`.)
    pub pid_to_feature: HashMap<u64, Uuid>,
    /// Mate connectors placed by `Script` nodes (`ctx.mate_connector`), by
    /// node, in placement order — a script's connectors live in its private
    /// sub-tree, so `connector::part_connectors` (which walks the tree)
    /// cannot see them. Carried for a node this pass did not re-execute.
    pub script_connectors: HashMap<Uuid, Vec<crate::connector::PartConnector>>,
    /// Evaluated 3D sketches, by feature. Not in `OpResult` because a 3D
    /// sketch produces no body: this is where its resolved points and chains
    /// live, and it is what a sweep and the renderer read. Carried for a
    /// feature this pass did not re-execute.
    pub sketch3d: HashMap<Uuid, waffle_types::sketch3d::Sketch3dEvaluation>,
}

/// What the previous rebuild established that a feature this pass does NOT
/// re-execute keeps: for a `Script` node, the outer features it consumed and
/// the connectors it placed — neither is recoverable from its `OpResult`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Carried<'a> {
    pub consumed_by: Option<&'a HashMap<Uuid, Vec<Uuid>>>,
    pub script_connectors: Option<&'a HashMap<Uuid, Vec<crate::connector::PartConnector>>>,
    pub sketch3d: Option<&'a HashMap<Uuid, waffle_types::sketch3d::Sketch3dEvaluation>>,
}

/// What changed since the last rebuild, which decides what a rebuild
/// re-executes (docs/notes/agent_bicycle_session_failures_2026_09_14.md F7:
/// re-executing every later feature made each edit re-run every boolean).
#[derive(Debug, Clone)]
pub enum Changed {
    /// Every feature from the rebuild point re-executes (reorder, full rebuild).
    All,
    /// These features changed (edited, added, deleted, (un)suppressed, or a
    /// parameter or context value they use). A feature re-executes only if it
    /// is one of them, names a feature that re-executed, or finds an input by
    /// tree position after one; every other feature keeps its last result.
    Features(std::collections::HashSet<Uuid>),
}

/// Rebuild the feature tree from scratch (or from a change point).
///
/// Replays features in order, resolving GeomRefs and executing operations.
/// Features before `from_index`, and later features `changed` does not reach,
/// keep their entry in `existing_results` (or re-report their entry in
/// `previous_errors`, the last rebuild's errors).
// Nine inputs: the tree, the kernel, what changed, the last rebuild's
// outcome (results, errors, what scripts established), and the
// document/assembly environment the operations read.
#[allow(clippy::too_many_arguments)]
pub fn rebuild(
    tree: &FeatureTree,
    kb: &mut dyn KernelBundle,
    from_index: usize,
    changed: &Changed,
    existing_results: &HashMap<Uuid, OpResult>,
    previous_errors: &[crate::types::FeatureError],
    carried: Carried<'_>,
    sources: &SourceStore,
    context: Option<&EditContext>,
) -> RebuildState {
    let mut state = RebuildState {
        feature_results: HashMap::new(),
        warnings: Vec::new(),
        feature_warnings: Vec::new(),
        feature_references: Vec::new(),
        errors: Vec::new(),
        feature_errors: Vec::new(),
        consumed_features: std::collections::HashSet::new(),
        consumed_by: HashMap::new(),
        pid_to_feature: HashMap::new(),
        script_connectors: HashMap::new(),
        sketch3d: HashMap::new(),
    };

    let active = tree.active_features();

    // Features whose output may differ from `existing_results`: the changed
    // ones, then every feature this pass executes. `position_moved` is set once
    // one of them has been passed, so position-dependent features re-execute.
    let (mut reran, mut position_moved) = match changed {
        Changed::All => (std::collections::HashSet::new(), true),
        Changed::Features(ids) => (
            ids.clone(),
            // A changed feature no longer in the active tree was deleted, from
            // at or before the rebuild point.
            ids.iter().any(|id| !active.iter().any(|f| f.id == *id)),
        ),
    };

    for (i, feature) in active.iter().enumerate() {
        if feature.suppressed {
            if i >= from_index && reran.contains(&feature.id) {
                position_moved = true;
            }
            continue;
        }

        let execute = i >= from_index
            && match changed {
                Changed::All => true,
                Changed::Features(_) => {
                    reran.contains(&feature.id)
                        || !(existing_results.contains_key(&feature.id)
                            || previous_errors.iter().any(|e| e.feature_id == feature.id))
                        || {
                            let (ids, scoped) = scan_references(feature);
                            (position_moved
                                && (scoped || depends_on_tree_position(feature, tree, sources)))
                                || ids.iter().any(|id| reran.contains(id))
                        }
                }
            };

        if !execute {
            // Not re-executed: its inputs are unchanged, so its last result (or
            // its last failure) stands. Consumption tracking is still
            // recomputed from the carried result; without it a rebuild loses
            // consumption established by earlier features (e.g., e1 consumed by
            // e2's union).
            let Some(result) = existing_results.get(&feature.id) else {
                if let Some(error) = previous_errors.iter().find(|e| e.feature_id == feature.id) {
                    state.errors.push((feature.id, error.message.clone()));
                    state.feature_errors.push(error.clone());
                }
                continue;
            };
            for w in &result.diagnostics.warnings {
                state.warnings.push(format!("{}: {}", feature.name, w));
            }
            state.feature_results.insert(feature.id, result.clone());
            if matches!(feature.operation, Operation::Sketch3d { .. }) {
                // Its evaluation is not derivable from the empty `OpResult`,
                // so what the last rebuild established stands.
                if let Some(ev) = carried.sketch3d.and_then(|m| m.get(&feature.id)) {
                    state.sketch3d.insert(feature.id, ev.clone());
                }
            }
            let consumed_ids = if matches!(feature.operation, Operation::Script { .. }) {
                // A script's consumption of OUTER bodies is not derivable from
                // its result; what the last rebuild established stands (its
                // children are private; `find_consumed_feature_ids` sees none).
                if let Some(cs) = carried.script_connectors.and_then(|m| m.get(&feature.id)) {
                    state.script_connectors.insert(feature.id, cs.clone());
                }
                carried
                    .consumed_by
                    .and_then(|m| m.get(&feature.id))
                    .cloned()
                    .unwrap_or_default()
            } else {
                find_consumed_feature_ids(
                    feature,
                    &state.feature_results,
                    tree,
                    &state.consumed_features,
                    Some(kb.as_introspect()),
                )
            };
            if let Some(result) = state.feature_results.get(&feature.id) {
                record_consumption(
                    &mut state.consumed_features,
                    &mut state.consumed_by,
                    feature.id,
                    result,
                    consumed_ids,
                );
            }
            continue;
        }

        reran.insert(feature.id);
        position_moved = true;

        // Resolve any GeomRef references before executing the feature
        resolve_feature_refs(feature, &state.feature_results, &mut state.warnings);

        // Track which features' solids would be consumed by a successful merge/boolean
        let mut consumed_ids = find_consumed_feature_ids(
            feature,
            &state.feature_results,
            tree,
            &state.consumed_features,
            Some(kb.as_introspect()),
        );

        // D0 item 1 (`specs/drawings_and_mbd.md` §4): tell the kernel WHOSE
        // geometry it is about to create, so the faces it stamps are named
        // from this feature's stable identity and their role in it rather
        // than from the arena's monotonic counter. Without it an incremental
        // rebuild — which re-runs only this feature, in an arena whose
        // counter has already advanced — renames every face of the edited
        // feature, and with them the edges at its junctions with untouched
        // geometry, and any stored pid either goes missing or lands on a
        // different face.
        //
        // The feature's uuid is the seed: it is the engine's own permanent
        // name for the step, it survives a reorder, a rename and a parameter
        // edit, and it is already what `pid_to_feature` keys on. Restored
        // (not cleared) afterwards so a nested execution — a script child —
        // hands the seed back to its parent.
        let prev_seed = kb.set_construct_seed(Some(
            waffle_types::kernel::ConstructSeed::from_u128(feature.id.as_u128()),
        ));

        // A `Script` node reports what it consumed and placed AFTER running
        // (its children are private); every other operation is known before.
        let outcome = if let Operation::Sketch3d { sketch } = &feature.operation {
            // Evaluated HERE rather than in `execute_feature` so the chains
            // reach `state`: a 3D sketch's output is not a body, so it has no
            // `OpResult` to ride in. `execute_feature`'s own arm performs the
            // same evaluation for validation when called directly.
            crate::sketch3d::evaluate(sketch, &state.feature_results, kb.as_introspect(), tree).map(
                |ev| {
                    let result = crate::sketch3d::no_geometry_result(&ev);
                    state.sketch3d.insert(feature.id, ev);
                    result
                },
            )
        } else if matches!(feature.operation, Operation::Script { .. }) {
            crate::script::execute(
                feature,
                kb,
                &state.feature_results,
                tree,
                &state.consumed_features,
                sources,
            )
            .map(|o| {
                consumed_ids = o.consumed_outer;
                if !o.connectors.is_empty() {
                    state.script_connectors.insert(feature.id, o.connectors);
                }
                o.result
            })
        } else {
            execute_feature(
                feature,
                kb,
                &state.feature_results,
                tree,
                &state.consumed_features,
                sources,
                context,
            )
        };
        kb.set_construct_seed(prev_seed);
        match outcome {
            Ok(result) => {
                for w in &result.diagnostics.warnings {
                    state.warnings.push(format!("{}: {}", feature.name, w));
                    state.feature_warnings.push((feature.id, w.clone()));
                }
                // The rung that answered this feature's own stored reference,
                // for a host to read off `feature_get`. Re-asks the question
                // `execute_feature` just asked, with the same inputs — the
                // sketch's result is not in the map yet and a sketch does not
                // touch the arena — because the answer cannot ride out on an
                // `OpResult` and must not be guessed from the error list.
                let recorded =
                    resolved_plane_face_state(feature, &state.feature_results, kb.as_introspect());
                if let Some(rs) = recorded {
                    state.feature_references.push((feature.id, rs));
                }
                // A merge/boolean that took custody of its targets marks them
                // consumed so they don't render (unless its auto-union fell back).
                record_consumption(
                    &mut state.consumed_features,
                    &mut state.consumed_by,
                    feature.id,
                    &result,
                    consumed_ids,
                );
                // KV13 F6: capture this feature's created-face persistent ids
                // NOW (its faces are current; later ops churn the arena).
                capture_face_pids(&mut state, feature.id, &result, kb);
                state.feature_results.insert(feature.id, result);
            }
            Err(e) => {
                let message = e.to_string();
                if let Some(rs) = refused_plane_face_state(feature, &e) {
                    state.feature_references.push((feature.id, rs));
                }
                state.feature_errors.push(crate::types::FeatureError {
                    feature_id: feature.id,
                    kind: (&e).into(),
                    message: message.clone(),
                });
                state.errors.push((feature.id, message));
                // Continue rebuilding remaining features
            }
        }
    }

    state
}

/// Mark `consumed_ids` consumed by `feature_id`, unless `result` is a legacy
/// auto-union that fell back to a standalone body — then its targets stay
/// alive. The decision reads the typed `Diagnostics::auto_union_fell_back`
/// flag set at the fallback, never the warning text (review 2026-10-28
/// §3.5 item 2, remediation Q9).
fn record_consumption(
    consumed_features: &mut std::collections::HashSet<Uuid>,
    consumed_by: &mut HashMap<Uuid, Vec<Uuid>>,
    feature_id: Uuid,
    result: &OpResult,
    consumed_ids: Vec<Uuid>,
) {
    if consumed_ids.is_empty() || result.diagnostics.auto_union_fell_back {
        return;
    }
    consumed_features.extend(consumed_ids.iter().copied());
    consumed_by.insert(feature_id, consumed_ids);
}

/// Every UUID a feature's definition names (its sketch, targets, datum planes,
/// GeomRef anchors), and whether any reference is scoped to an assembly
/// context. Read off the serialized form so that a new reference field cannot
/// be missed; a string that only looks like a UUID costs a needless
/// re-execution, never a stale result.
fn scan_references(feature: &Feature) -> (Vec<Uuid>, bool) {
    fn walk(value: &serde_json::Value, ids: &mut Vec<Uuid>, scoped: &mut bool) {
        match value {
            serde_json::Value::String(s) => {
                if let Ok(id) = Uuid::parse_str(s) {
                    ids.push(id);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, ids, scoped);
                }
            }
            serde_json::Value::Object(map) => {
                if map.get("scope").is_some_and(|s| !s.is_null()) {
                    *scoped = true;
                }
                for item in map.values() {
                    walk(item, ids, scoped);
                }
            }
            _ => {}
        }
    }
    let mut ids = Vec::new();
    let mut scoped = false;
    let operation = serde_json::to_value(&feature.operation);
    let references = serde_json::to_value(&feature.references);
    match (operation, references) {
        (Ok(operation), Ok(references)) => {
            walk(&operation, &mut ids, &mut scoped);
            walk(&references, &mut ids, &mut scoped);
        }
        // Unreadable: assume it depends on everything.
        _ => scoped = true,
    }
    (ids, scoped)
}

/// Whether a feature finds an input by tree position rather than by id, so a
/// feature re-executed before it can change what it builds: legacy
/// most-recent-solid and share-a-face targets, through-all depths (measured
/// against the most recent solid), sketches with projected points (their
/// sources are read when the extrude or revolve runs), and the "everything
/// before me" selectors — `UnionTargets::All` and `PatternSeeds::All` fold or
/// pattern every live body before the feature and serialize no UUID for
/// `scan_references` to find (review 2026-10-28 §4.1, Q10). A `Script` node
/// depends on position when any child it records does (an argument-less
/// `union_all()` is `UnionTargets::All` over the OUTER bodies); its children
/// are only known by recording it, and a script that cannot be recorded is
/// assumed positional (it re-executes into its own loud error).
fn depends_on_tree_position(feature: &Feature, tree: &FeatureTree, sources: &SourceStore) -> bool {
    let (eff, sketch_id, through_all) = match &feature.operation {
        Operation::UnionAll { params } => {
            return matches!(params.targets, crate::types::UnionTargets::All)
        }
        Operation::PatternCircular { params } => {
            return matches!(params.seeds, crate::types::PatternSeeds::All)
        }
        Operation::PatternLinear { params } => {
            return matches!(params.seeds, crate::types::PatternSeeds::All)
        }
        Operation::PatternMirror { params } => {
            return matches!(params.seeds, crate::types::PatternSeeds::All)
        }
        Operation::Script { params } => {
            let Some(text) = sources.text(params.source_id) else {
                return true;
            };
            return match crate::script::record(&text, params) {
                Ok(recorded) => recorded
                    .children
                    .iter()
                    .any(|c| depends_on_tree_position(&c.feature, tree, sources)),
                Err(_) => true,
            };
        }
        Operation::Extrude { params } => (
            normalize_extrude_combine(params),
            params.sketch_id,
            matches!(params.depth_mode, DepthMode::ThroughAll)
                || matches!(params.second_direction, Some(SecondDirection::ThroughAll)),
        ),
        Operation::Revolve { params } => (
            crate::types::normalize_revolve_combine(params),
            params.sketch_id,
            false,
        ),
        Operation::Pipe { params } => (
            crate::types::normalize_pipe_combine(params),
            params.sketch_id,
            false,
        ),
        Operation::Sweep { params } => (
            crate::types::normalize_sweep_combine(params),
            params.sketch_id,
            false,
        ),
        _ => return false,
    };
    let positional_targets = !matches!(eff.mode, CombineMode::NewBody)
        && !matches!(eff.targets, TargetStrategy::Explicit(_));
    let has_projected =
        |id: Uuid| find_sketch_in_tree(id, tree).is_ok_and(|s| !s.projected.is_empty());
    let projected = has_projected(sketch_id)
        // A sweep's PATH sketch is read when the sweep runs, like its section.
        || matches!(
            &feature.operation,
            Operation::Sweep {
                params: SweepParams {
                    path: SweepPathRef::Sketch { sketch_id, .. },
                    ..
                },
            } if has_projected(*sketch_id)
        );
    positional_targets || through_all || projected
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
/// Resolve an ImportedBody feature's STEP text (v4 §2.3 resolution order):
/// the document source store first (the file's `embed` or host-provided
/// content, both registered by the host), then the legacy v3 inline blob.
/// Neither ⇒ a loud `SourceUnavailable` — the feature errors, the document
/// still loads.
pub fn resolve_import_text(
    params: &ImportedBodyParams,
    sources: &SourceStore,
) -> Result<String, String> {
    if let Some(id) = params.source_id {
        if let Some(text) = sources.text(id) {
            return Ok(text);
        }
    }
    if let Some(blob) = &params.blob {
        let encoding = params
            .blob_encoding
            .as_deref()
            .unwrap_or(step_import::STEP_BLOB_ENCODING);
        return step_import::decode_step_blob(encoding, blob);
    }
    Err(match params.source_id {
        Some(id) => format!(
            "SourceUnavailable: source {id} ({}) has no content in this session — \
             the document carries no embed for it and the host has not provided one",
            params.file_name
        ),
        None => format!(
            "SourceUnavailable: import {} has neither a source_id nor an inline payload",
            params.file_name
        ),
    })
}

pub(crate) fn execute_feature(
    feature: &Feature,
    kb: &mut dyn KernelBundle,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
    sources: &SourceStore,
    context: Option<&EditContext>,
) -> Result<OpResult, EngineError> {
    match &feature.operation {
        Operation::Unknown(_) => Err(EngineError::UnsupportedOperation {
            type_tag: feature.operation.type_tag().to_string(),
        }),

        Operation::Sketch { sketch } => {
            // A plane the sketch cannot be drawn on is the sketch's own error,
            // reported here rather than at the extrude that would silently
            // fall back to the derived basis and move every point.
            if let Some(x) = sketch.plane_x_axis {
                let normal = unit_normal(sketch.plane_normal);
                if !SketchPlaneBasis::x_axis_is_usable(normal, x) {
                    return Err(EngineError::ResolutionFailed {
                        reason: format!(
                            "sketch plane x_axis {x:?} cannot orient a plane of normal \
                             {normal:?}: it is zero-length, non-finite, or parallel to \
                             the normal"
                        ),
                    });
                }
            }
            // N2 §5.3 item 3: a sketch drawn on a model face re-resolves that
            // face EVERY rebuild, through the whole ladder, and refuses if it
            // is gone. Warnings ride out on the diagnostics, which the rebuild
            // loop attributes to this feature.
            let warnings = resolve_sketch_plane_face(sketch, feature_results, kb.as_introspect())?;
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
                diagnostics: modeling_ops::Diagnostics {
                    warnings,
                    ..modeling_ops::Diagnostics::default()
                },
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

        Operation::Sketch3d { sketch } => {
            // No geometry of its own: the rebuild only proves the sketch
            // resolves — a dangling attachment, an attachment cycle or a
            // fillet that does not fit is THIS feature's error, named here,
            // rather than a sweep downstream that cannot find its path. The
            // evaluated chains are read after the rebuild
            // (`RebuildState::sketch3d`), the way a connector's frame is;
            // the rebuild loop captures them there.
            let ev = crate::sketch3d::evaluate(sketch, feature_results, kb.as_introspect(), tree)?;
            Ok(crate::sketch3d::no_geometry_result(&ev))
        }

        Operation::MateConnector { params } => {
            // No geometry of its own: the rebuild only proves the frame
            // derives (loud here, so the feature is marked failed). The frame
            // itself is read after the rebuild (`Engine::connectors`).
            crate::connector::part_connector_frame(params, feature_results, kb.as_introspect())?;
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

        Operation::ImportedBody { params } => {
            // STEP import (task #138, docs/step_import_roadmap.md §3): decode
            // the embedded payload, parse (process-wide cache makes transform
            // edits cheap), then bake scale + placement into a clone and
            // ingest as one composite body.
            let fail = |reason: String| EngineError::RebuildFailed {
                feature_name: feature.name.clone(),
                reason,
            };
            let step_text = resolve_import_text(params, sources).map_err(|reason| {
                EngineError::SourceUnavailable {
                    feature_name: feature.name.clone(),
                    source_id: params.source_id,
                    reason,
                }
            })?;
            // One product of the file, in its own frame (C3): a product the
            // file does not have is the feature's own loud error. Served
            // from the mesh tier (SI5 C6 covers whole-file imports).
            if let Some(product) = &params.product {
                let products =
                    step_import::parse_step_products_cached(&step_text, &params.file_name)
                        .map_err(|e| fail(e.to_string()))?;
                let found = products
                    .products
                    .iter()
                    .find(|p| p.name == *product)
                    .ok_or_else(|| {
                        fail(format!(
                            "product `{product}` is not in {} (products: {})",
                            params.file_name,
                            products
                                .products
                                .iter()
                                .map(|p| p.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                    })?;
                let reason = crate::import_tiers::mesh_only_reason(params);
                return crate::import_tiers::execute_mesh_import(
                    params,
                    found.body.clone(),
                    kb,
                    reason,
                )
                .map_err(fail);
            }
            // Whole file: a form the exact tier does not attempt goes to the
            // mesh tier with the reason attached; everything else is served
            // per shell by the first tier that can carry it (SI5 C6,
            // `import_tiers`).
            if let Some(reason) = crate::import_tiers::mesh_only_reason(params) {
                let parsed = step_import::parse_step_cached(&step_text, &params.file_name)
                    .map_err(|e| fail(e.to_string()))?;
                return crate::import_tiers::execute_mesh_import(
                    params,
                    (*parsed).clone(),
                    kb,
                    Some(reason),
                )
                .map_err(fail);
            }
            let tiered = step_import::parse_step_tiered_cached(&step_text, &params.file_name)
                .map_err(|e| fail(e.to_string()))?;
            let file_name = params.file_name.clone();
            let mut mesh_whole_file = || {
                step_import::parse_step_cached(&step_text, &file_name)
                    .map(|p| (*p).clone())
                    .map_err(|e| e.to_string())
            };
            crate::import_tiers::execute_tiered_import(params, &tiered, kb, &mut mesh_whole_file)
                .map_err(fail)
        }

        Operation::Extrude { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let sketch_ref = find_sketch_in_tree(params.sketch_id, tree)?;
            let mut sketch_expanded = sketch_ref.clone();
            // Projection: re-derive externally-driven point positions from the
            // features built so far, then force a fresh recompute of derived data
            // (positions + profiles) from the updated points.
            if !sketch_expanded.projected.is_empty() {
                reproject_sketch(
                    &mut sketch_expanded,
                    feature_results,
                    kb.as_introspect(),
                    context,
                );
                sketch_expanded.solved_positions.clear();
                sketch_expanded.solved_profiles.clear();
            }
            sketch_expanded.recompute_derived_checked().map_err(|e| {
                EngineError::SketchGenerator {
                    sketch_id: params.sketch_id,
                    reason: e.to_string(),
                }
            })?;
            let sketch = &sketch_expanded;
            let mut warnings_regions: Vec<String> = Vec::new();

            // Region identity: a picked region is stored as the polygon it was
            // at pick time. Re-resolve it against the sketch AS IT IS NOW by its
            // boundary-entity signature, so a sketch edit (scale, moved corner)
            // moves the extrude with it instead of leaving a stale footprint.
            let (resolved_region, resolved_regions) = resolve_extrude_regions(
                sketch,
                params.region.as_ref(),
                &params.regions,
                &mut warnings_regions,
            );

            let direction = params.direction.unwrap_or(unit_normal(sketch.plane_normal));

            let profile_index = resolve_profile_index(
                sketch,
                params.profile_index,
                params.profile_entity_ids.as_deref(),
                &mut warnings_regions,
            )?;

            // Normalize the boolean-combine choice once (Constitution §7) and
            // resolve its target bodies up front: the cut-direction reversal
            // below needs the primary target, and the final dispatch reuses the
            // same set. See specs/optional_booleans_multibody_extrude.md.
            let eff = normalize_extrude_combine(params);
            let is_cut = matches!(eff.mode, CombineMode::Cut);
            let mut combine_warnings: Vec<String> = std::mem::take(&mut warnings_regions);
            let combine_targets = match eff.mode {
                CombineMode::NewBody => Vec::new(),
                _ => match &eff.targets {
                    // Auto default: bodies that share a face with the sketch.
                    TargetStrategy::ShareAFace => resolve_share_a_face(
                        params.sketch_id,
                        profile_index,
                        None,
                        resolved_region.as_ref(),
                        &resolved_regions,
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
                        already_consumed,
                        &mut combine_warnings,
                    )?,
                },
            };

            // ── The extrude axis, measured ONCE ────────────────────────────
            // Both the cut-direction reversal and a ThroughAll / UpTo depth
            // read the same quantity: where the material this feature acts on
            // sits along the extrude axis. Measure it here, decide the sweep
            // direction, and only THEN resolve the depth — a depth measured
            // along the unreversed normal and then swept the other way is how
            // a ThroughAll cut came to miss its body entirely (assay P0012).
            //
            // The measured body is this feature's PRIMARY target — its first
            // combine target, falling back to the most recent solid when it
            // has none (a NewBody boss asking to go "through all"). Measuring
            // the UNION of a multi-target cut's bodies was tried and reverted:
            // with DISJOINT targets the union's mid-extent is a compromise
            // that belongs to neither body, and it flipped assay R0091's cut
            // (a revolve sausage standing beside a box) from CORRECT to a
            // kernel STOP. A multi-target cutter that does not reach its
            // SECOND target is a real (pre-existing) gap — ledgered in
            // `docs/yang_tail_triage.md`, not papered over here.
            //
            // Only the features that actually consume the reading pay for it:
            // it walks every vertex of the body, which on a large one is not
            // free, and a plain blind boss has no use for it.
            let needs_axis_measurement = (is_cut && params.direction.is_none())
                || matches!(params.depth_mode, DepthMode::ThroughAll)
                || matches!(params.second_direction, Some(SecondDirection::ThroughAll));
            let measured_solids: Vec<waffle_types::kernel::KernelSolidHandle> =
                if !needs_axis_measurement {
                    Vec::new()
                } else if let Some((_, handle)) = combine_targets.first() {
                    vec![handle.clone()]
                } else {
                    find_most_recent_solid(feature, feature_results, tree, already_consumed)
                        .into_iter()
                        .collect()
                };
            let measured_range =
                solids_projection_range(kb.as_introspect(), &measured_solids, direction);
            let sketch_proj = unit_or_none(direction)
                .map(|d| {
                    sketch.plane_origin[0] * d[0]
                        + sketch.plane_origin[1] * d[1]
                        + sketch.plane_origin[2] * d[2]
                })
                .unwrap_or(0.0);

            // For cuts with no authored direction: sweep toward the material.
            // The decision is the measured body's mid-extent along the axis —
            // a bulk property, so it is insensitive to which vertex happens to
            // be extreme. It only ever flips a cut; a boss and an authored
            // direction sweep exactly as written.
            let should_reverse_for_cut = if is_cut && params.direction.is_none() {
                let decision = match measured_range {
                    Some((lo, hi)) => {
                        // Reverse only when the body's midpoint is behind the plane.
                        (lo + hi) * 0.5 < sketch_proj
                    }
                    // Nothing measurable: legacy reverse behaviour.
                    None => true,
                };
                if std::env::var_os("FE_CUT_TRACE").is_some() {
                    match measured_range {
                        Some((lo, hi)) => eprintln!(
                            "[fe-cut] {}: targets={} proj=[{lo:.6e}, {hi:.6e}] \
                             sketch_proj={sketch_proj:.6e} reverse={decision}",
                            feature.name,
                            measured_solids.len()
                        ),
                        None => eprintln!(
                            "[fe-cut] {}: nothing measurable — legacy reverse",
                            feature.name
                        ),
                    }
                }
                decision
            } else {
                false // explicit direction or non-cut: never auto-reverse
            };

            // The directions this extrude actually sweeps, and the measured
            // material's interval relative to the sketch plane along each.
            let primary_sweep = if should_reverse_for_cut {
                [-direction[0], -direction[1], -direction[2]]
            } else {
                direction
            };
            let second_sweep = [-primary_sweep[0], -primary_sweep[1], -primary_sweep[2]];
            let swept_primary = measured_range.map(|(lo, hi)| {
                if should_reverse_for_cut {
                    (sketch_proj - hi, sketch_proj - lo)
                } else {
                    (lo - sketch_proj, hi - sketch_proj)
                }
            });
            let swept_second = swept_primary.map(|(lo, hi)| (-hi, -lo));

            // Resolve primary depth from depth mode
            let primary_depth = resolve_depth(
                &params.depth_mode,
                params.depth,
                sketch.plane_origin,
                primary_sweep,
                swept_primary,
                feature_results,
                tree,
                kb,
                context,
            )?;

            // Determine second direction: explicit field takes precedence,
            // then backward-compat symmetric flag
            let effective_second = params
                .second_direction
                .clone()
                .or(params.symmetric.then_some(SecondDirection::Symmetric));

            // Resolve second depth if bidirectional
            let second_depth = match &effective_second {
                Some(SecondDirection::Symmetric) => Some(primary_depth),
                Some(SecondDirection::Blind { depth: d, .. }) => Some(*d),
                Some(SecondDirection::ThroughAll) => Some(resolve_depth(
                    &DepthMode::ThroughAll,
                    params.depth,
                    sketch.plane_origin,
                    second_sweep,
                    swept_second,
                    feature_results,
                    tree,
                    kb,
                    context,
                )?),
                Some(SecondDirection::UpTo { reference }) => Some(resolve_depth(
                    &DepthMode::UpTo {
                        reference: reference.clone(),
                    },
                    params.depth,
                    sketch.plane_origin,
                    second_sweep,
                    swept_second,
                    feature_results,
                    tree,
                    kb,
                    context,
                )?),
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

            let x_axis = sketch_x_axis(sketch);

            // Multi-region selection: the user picked ≥2 sketch regions to extrude
            // as ONE body. Union their 2D footprints FIRST (sketch plane), so
            // adjacent regions with shared / coplanar side walls merge into one
            // clean face — NO 3D boolean, hence no Yang Stage-0 coplanar wall. The
            // union yields one region per connected component; each becomes its own
            // face and the (now provably DISJOINT) component solids are unioned (a
            // disjoint union has no coplanar contact, so it always succeeds).
            let extrude_result = if resolved_regions.len() >= 2 {
                let merged = waffle_types::union_regions(&resolved_regions);
                if merged.is_empty() {
                    return Err(EngineError::ResolutionFailed {
                        reason: "multi-region extrude: 2D union produced no area".into(),
                    });
                }
                let mut acc: Option<OpResult> = None;
                for region in &merged {
                    let fid = kb.make_face_from_region(
                        region,
                        face_origin,
                        unit_normal(sketch.plane_normal),
                        x_axis,
                    )?;
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
                let face_id = if let Some(region) = &resolved_region {
                    kb.make_face_from_region(
                        region,
                        face_origin,
                        unit_normal(sketch.plane_normal),
                        x_axis,
                    )?
                } else {
                    let face_ids = kb.make_faces_from_profiles(
                        &sketch.solved_profiles,
                        face_origin,
                        unit_normal(sketch.plane_normal),
                        x_axis,
                        &sketch.solved_positions,
                    )?;

                    if face_ids.is_empty() {
                        return Err(EngineError::ProfileOutOfRange {
                            index: profile_index,
                            count: 0,
                        });
                    }

                    let face_index = profile_index.min(face_ids.len() - 1);
                    face_ids[face_index]
                };

                execute_extrude(kb, face_id, extrude_direction, extrude_depth, None)?
            };

            // Dispatch the boolean combine per the normalized mode + resolved
            // targets. See specs/optional_booleans_multibody_extrude.md §4.
            let mut result =
                dispatch_combine(kb, &eff, &combine_targets, extrude_result, "extrude")?;
            carry_untargeted_siblings(&mut result, feature, feature_results);
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
                reproject_sketch(
                    &mut sketch_expanded,
                    feature_results,
                    kb.as_introspect(),
                    context,
                );
                sketch_expanded.solved_positions.clear();
                sketch_expanded.solved_profiles.clear();
            }
            sketch_expanded.recompute_derived_checked().map_err(|e| {
                EngineError::SketchGenerator {
                    sketch_id: params.sketch_id,
                    reason: e.to_string(),
                }
            })?;
            let sketch = &sketch_expanded;

            // Carried into the feature's diagnostics beside the combine's own
            // warnings below: a profile re-resolved after a sketch edit has to
            // reach the user on a revolve exactly as it does on an extrude.
            let mut profile_warnings: Vec<String> = Vec::new();
            let profile_index = resolve_profile_index(
                sketch,
                params.profile_index,
                params.profile_entity_ids.as_deref(),
                &mut profile_warnings,
            )?;

            let x_axis = sketch_x_axis(sketch);
            let face_ids = kb.make_faces_from_profiles(
                &sketch.solved_profiles,
                sketch.plane_origin,
                unit_normal(sketch.plane_normal),
                x_axis,
                &sketch.solved_positions,
            )?;

            if face_ids.is_empty() {
                return Err(EngineError::ProfileOutOfRange {
                    index: profile_index,
                    count: 0,
                });
            }

            let face_index = profile_index.min(face_ids.len() - 1);
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
            let mut combine_warnings: Vec<String> = std::mem::take(&mut profile_warnings);
            let combine_targets = match eff.mode {
                CombineMode::NewBody => Vec::new(),
                _ => match &eff.targets {
                    TargetStrategy::ShareAFace => resolve_share_a_face(
                        params.sketch_id,
                        profile_index,
                        None,
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
                        already_consumed,
                        &mut combine_warnings,
                    )?,
                },
            };
            let mut result =
                dispatch_combine(kb, &eff, &combine_targets, revolve_result, "revolve")?;
            carry_untargeted_siblings(&mut result, feature, feature_results);
            result.diagnostics.warnings.extend(combine_warnings);
            Ok(result)
        }

        Operation::Pipe { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let sketch_ref = find_sketch_in_tree(params.sketch_id, tree)?;
            let mut sketch_expanded = sketch_ref.clone();
            if !sketch_expanded.projected.is_empty() {
                reproject_sketch(
                    &mut sketch_expanded,
                    feature_results,
                    kb.as_introspect(),
                    context,
                );
                sketch_expanded.solved_positions.clear();
                sketch_expanded.solved_profiles.clear();
            }
            sketch_expanded.recompute_derived_checked().map_err(|e| {
                EngineError::SketchGenerator {
                    sketch_id: params.sketch_id,
                    reason: e.to_string(),
                }
            })?;
            let sketch = &sketch_expanded;
            let chain = waffle_types::path::extract_open_chain(
                &sketch.entities,
                &sketch.solved_positions,
                &params.entity_ids,
            )
            .map_err(|e| EngineError::SketchGenerator {
                sketch_id: params.sketch_id,
                reason: e.to_string(),
            })?;
            let normal = unit_normal(sketch.plane_normal);
            let x_axis = sketch_x_axis(sketch);
            let pipe_result = execute_pipe(
                kb,
                sketch.plane_origin,
                normal,
                x_axis,
                &chain.segments,
                params.radius,
                params.inner_radius,
                None,
            )?;
            let eff = crate::types::normalize_pipe_combine(params);
            let mut combine_warnings: Vec<String> = Vec::new();
            let combine_targets = match eff.mode {
                CombineMode::NewBody => Vec::new(),
                _ => resolve_combine_targets(
                    &eff.targets,
                    feature,
                    feature_results,
                    tree,
                    already_consumed,
                    &mut combine_warnings,
                )?,
            };
            let mut result = dispatch_combine(kb, &eff, &combine_targets, pipe_result, "pipe")?;
            carry_untargeted_siblings(&mut result, feature, feature_results);
            result.diagnostics.warnings.extend(combine_warnings);
            Ok(result)
        }

        Operation::Sweep { params } => {
            let _sketch_result = find_sketch_result(params.sketch_id, feature_results)?;
            let section_sketch =
                current_sketch(params.sketch_id, feature_results, tree, kb, context)?;
            let mut section_warnings: Vec<String> = Vec::new();
            let (outer, holes) =
                resolve_sweep_section(&section_sketch, params, &mut section_warnings)?;
            let section = waffle_types::kernel::SweepSection {
                plane_origin: section_sketch.plane_origin,
                plane_normal: unit_normal(section_sketch.plane_normal),
                plane_x_axis: sketch_x_axis(&section_sketch),
                outer,
                holes,
            };
            let path = resolve_sweep_path(&params.path, feature_results, tree, kb, context)?;
            let sweep_result = execute_sweep(kb, &section, &path, None)?;
            let eff = crate::types::normalize_sweep_combine(params);
            let mut combine_warnings: Vec<String> = section_warnings;
            let combine_targets = match eff.mode {
                CombineMode::NewBody => Vec::new(),
                _ => resolve_combine_targets(
                    &eff.targets,
                    feature,
                    feature_results,
                    tree,
                    already_consumed,
                    &mut combine_warnings,
                )?,
            };
            let mut result = dispatch_combine(kb, &eff, &combine_targets, sweep_result, "sweep")?;
            carry_untargeted_siblings(&mut result, feature, feature_results);
            result.diagnostics.warnings.extend(combine_warnings);
            Ok(result)
        }

        Operation::PatternCircular { params } => crate::pattern::execute(
            feature,
            kb,
            feature_results,
            tree,
            already_consumed,
            crate::pattern::PatternSpec::Circular(params),
        ),
        Operation::PatternLinear { params } => crate::pattern::execute(
            feature,
            kb,
            feature_results,
            tree,
            already_consumed,
            crate::pattern::PatternSpec::Linear(params),
        ),
        Operation::PatternMirror { params } => crate::pattern::execute(
            feature,
            kb,
            feature_results,
            tree,
            already_consumed,
            crate::pattern::PatternSpec::Mirror(params),
        ),
        // The rebuild loop calls `script::execute` directly for the outer
        // consumption and connectors it reports; this arm serves the other
        // callers (tests, nested execution), which need only the result.
        Operation::Script { .. } => crate::script::execute(
            feature,
            kb,
            feature_results,
            tree,
            already_consumed,
            sources,
        )
        .map(|o| o.result),

        Operation::UnionAll { params } => {
            crate::union_all::execute(feature, params, kb, feature_results, tree, already_consumed)
        }

        Operation::BooleanCombine { params } => {
            // A pair op cannot drop an operand, so a consumed one is loud
            // under either policy (`specs/b4_balanced_union.md` §2.4): the
            // stale pre-consumption handle would otherwise duplicate the
            // body silently (the 2026-09-17 gearbox finding).
            for (what, gr) in [("target", &params.body_a), ("tool", &params.body_b)] {
                if let waffle_types::Anchor::FeatureOutput { feature_id, .. } = &gr.anchor {
                    if already_consumed.contains(feature_id) {
                        return Err(EngineError::ResolutionFailed {
                            reason: format!(
                                "boolean {what} body of feature {feature_id} was already \
                                 consumed by an earlier feature"
                            ),
                        });
                    }
                }
            }
            // Find the solid handles from the referenced features
            let handle_a = find_solid_handle(&params.body_a, feature_results)?;
            let handle_b = find_solid_handle(&params.body_b, feature_results)?;

            let kind = match params.operation {
                BooleanOp::Union => BooleanKind::Union,
                BooleanOp::Subtract => BooleanKind::Subtract,
                BooleanOp::Intersect => BooleanKind::Intersect,
            };

            let mut result = execute_boolean(kb, &handle_a, &handle_b, kind)?;
            // Custody: the op names ONE output per operand but consumes both
            // operand features whole, so every other live body of either
            // feature must be carried, not hidden (P0010/P0011).
            carry_untargeted_siblings(&mut result, feature, feature_results);
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

/// The sketch `sketch_id` as it is NOW: projected points re-derived from
/// the current model and its derived data (positions, profiles) recomputed
/// — the same preparation the extrude, revolve and pipe arms do inline.
fn current_sketch(
    sketch_id: Uuid,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    kb: &mut dyn KernelBundle,
    context: Option<&EditContext>,
) -> Result<Sketch, EngineError> {
    let sketch_ref = find_sketch_in_tree(sketch_id, tree)?;
    let mut sketch = sketch_ref.clone();
    if !sketch.projected.is_empty() {
        reproject_sketch(&mut sketch, feature_results, kb.as_introspect(), context);
        sketch.solved_positions.clear();
        sketch.solved_profiles.clear();
    }
    sketch
        .recompute_derived_checked()
        .map_err(|e| EngineError::SketchGenerator {
            sketch_id,
            reason: e.to_string(),
        })?;
    Ok(sketch)
}

/// A sweep section's `(outer, holes)` loops in sketch `(u, v)`.
type SectionLoops = (Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>);

/// The sweep's section polygon in the section sketch's `(u, v)`
/// (`specs/b6_general_sweep.md` S6): an explicit `region` (re-resolved
/// against the current sketch by its boundary signature, as an extrude's
/// is), else the whole-loop profile `profile_index` / `profile_entity_ids`
/// addresses, read through its vertex order. A circle, an arc-bearing or a
/// spline-bearing profile is refused as a typed capability wall (B6 S4) —
/// its chord polygon would be a silent approximation.
fn resolve_sweep_section(
    sketch: &Sketch,
    params: &SweepParams,
    warnings: &mut Vec<String>,
) -> Result<SectionLoops, EngineError> {
    if params.region.is_some() {
        let (resolved, _) = resolve_extrude_regions(sketch, params.region.as_ref(), &[], warnings);
        let region = resolved.expect("a Some region resolves to Some");
        return Ok((region.outer, region.holes));
    }
    let index = resolve_profile_index(
        sketch,
        params.profile_index,
        params.profile_entity_ids.as_deref(),
        warnings,
    )?;
    let profile = &sketch.solved_profiles[index];
    let curved = if profile.circle.is_some() {
        Some("a circle")
    } else if !profile.arc_segments.is_empty() {
        Some("an arc-bearing profile")
    } else if !profile.spline_segments.is_empty() {
        Some("a spline-bearing profile")
    } else {
        None
    };
    if let Some(what) = curved {
        return Err(EngineError::KernelError(
            waffle_types::kernel::KernelError::NotSupported {
                operation: format!(
                    "sweep: the section is {what}; a sweep section must be a plain polygon \
                     for now (holed and arc-bearing sections are B6 S4, \
                     specs/b6_general_sweep.md §2)"
                ),
            },
        ));
    }
    if profile.vertex_ids.len() < 3 {
        return Err(EngineError::OpError(
            modeling_ops::OpError::InvalidParameter {
                reason: format!(
                    "sweep: section profile {index} of sketch {} has no polygon vertex order",
                    sketch.id
                ),
            },
        ));
    }
    let mut outer = Vec::with_capacity(profile.vertex_ids.len());
    for vid in &profile.vertex_ids {
        let Some(&p) = sketch.solved_positions.get(vid) else {
            return Err(EngineError::OpError(
                modeling_ops::OpError::InvalidParameter {
                    reason: format!(
                        "sweep: section vertex {vid} of sketch {} has no solved position",
                        sketch.id
                    ),
                },
            ));
        };
        outer.push(p);
    }
    // `SweepSection::outer` is counter-clockwise; a loop addressed as a
    // hole winds the other way, so orient it (a reversal, not a reshaping).
    let twice_area: f64 = outer
        .iter()
        .zip(outer.iter().cycle().skip(1))
        .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
        .sum();
    if twice_area < 0.0 {
        outer.reverse();
    }
    Ok((outer, Vec::new()))
}

/// The sweep's path as a world-space chain (`specs/b6_general_sweep.md`
/// §8): a planar sketch's line/arc selection ordered by
/// `extract_path_chain` and embedded through the sketch's plane frame, or
/// the named chain of a `Sketch3d` evaluated against the current model.
/// Either way the kernel sees one `Chain3d`.
fn resolve_sweep_path(
    path: &SweepPathRef,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    kb: &mut dyn KernelBundle,
    context: Option<&EditContext>,
) -> Result<waffle_types::sketch3d::Chain3d, EngineError> {
    match path {
        SweepPathRef::Sketch {
            sketch_id,
            entity_ids,
        } => {
            let sketch = current_sketch(*sketch_id, feature_results, tree, kb, context)?;
            let chain = waffle_types::path::extract_path_chain(
                &sketch.entities,
                &sketch.solved_positions,
                entity_ids,
            )
            .map_err(|e| {
                EngineError::OpError(modeling_ops::OpError::InvalidParameter {
                    reason: format!("sweep path (sketch {sketch_id}): {e}"),
                })
            })?;
            Ok(waffle_types::path::chain3d_from_plane(
                sketch.plane_origin,
                unit_normal(sketch.plane_normal),
                sketch_x_axis(&sketch),
                &chain,
            ))
        }
        SweepPathRef::Sketch3d {
            sketch_id,
            entity_id,
        } => {
            let sketch = tree
                .features
                .iter()
                .find(|f| f.id == *sketch_id)
                .and_then(|f| match &f.operation {
                    Operation::Sketch3d { sketch } => Some(sketch),
                    _ => None,
                })
                .ok_or(EngineError::SketchNotFound { id: *sketch_id })?;
            let ev = crate::sketch3d::evaluate(sketch, feature_results, kb.as_introspect(), tree)?;
            let chain = match entity_id {
                Some(eid) => ev
                    .chains
                    .iter()
                    .find(|c| c.edges.iter().any(|e| e.entity_id == *eid))
                    .ok_or_else(|| EngineError::ResolutionFailed {
                        reason: format!(
                            "sweep path: 3D sketch {sketch_id} has no chain through entity {eid} \
                             ({} chains)",
                            ev.chains.len()
                        ),
                    })?,
                None => match ev.chains.as_slice() {
                    [only] => only,
                    [] => {
                        return Err(EngineError::ResolutionFailed {
                            reason: format!("sweep path: 3D sketch {sketch_id} has no chain"),
                        })
                    }
                    many => {
                        return Err(EngineError::ResolutionFailed {
                            reason: format!(
                                "sweep path: 3D sketch {sketch_id} has {} chains; name one with \
                                 `entity_id`",
                                many.len()
                            ),
                        })
                    }
                },
            };
            Ok(chain.clone())
        }
    }
}

/// The solved profile an extrude/revolve addresses (v4 §2.9,
/// `specs/waffle_v4_document_model.md`): by entity-id set when
/// `profile_entity_ids` is present — order-insensitive, exactly one loop must
/// match — else by `profile_index`, range-checked.
///
/// **A loop that GREW still carries the identity** (2026-10-09). An equal set
/// is tried first; failing that, the one loop whose set CONTAINS the stored one
/// is it. A sketch edit that adds geometry to a loop — a fillet turning
/// `{5,6,7,8}` into `{5,6,12,7,8}`, a split adding a vertex — does not make a
/// different region, and `profile_entity_ids` is the only addressing an agent
/// is offered (`sketch_create` and `sketch_regions` answer with it), so under
/// the equal-sets rule alone an agent could author a plate and could round its
/// corner, but not both: the extrude failed `ProfileNotFound` and the edit
/// rolled back. Measured through the tools, 2026-10-08.
///
/// SHRINKAGE is not re-pointed. A named entity that no longer exists means the
/// author's loop is gone — what remains may bound some larger region that was
/// never asked for — so it stays the loud error it is today. Two loops
/// containing the set is `ProfileAmbiguous`, equally loud: growth is only an
/// identity when it is unique.
///
/// A containment match pushes a WARNING naming both sets, which is what keeps
/// the rule honest in the other direction: a caller that names a strict subset
/// by MISTAKE — three edges of a square, believing in a loop that does not
/// exist — gets the loop that contains everything it named, and is told that is
/// what happened, rather than finding out from the shape. (The equal-set match
/// is silent: nothing drifted.) `resolve_extrude_regions` reports its own
/// re-resolution the same way.
pub(crate) fn resolve_profile_index(
    sketch: &Sketch,
    profile_index: usize,
    profile_entity_ids: Option<&[u32]>,
    warnings: &mut Vec<String>,
) -> Result<usize, EngineError> {
    let count = sketch.solved_profiles.len();
    if let Some(ids) = profile_entity_ids {
        let want: std::collections::BTreeSet<u32> = ids.iter().copied().collect();
        let not_found = || EngineError::ProfileNotFound {
            entity_ids: ids.to_vec(),
            count,
        };
        // The empty set names no loop (a profile always has ≥ 1 entity), and is
        // a subset of every one of them — so it has to be refused before the
        // containment pass, not by it.
        if want.is_empty() {
            return Err(not_found());
        }
        let sets: Vec<std::collections::BTreeSet<u32>> = sketch
            .solved_profiles
            .iter()
            .map(|p| p.entity_ids.iter().copied().collect())
            .collect();
        let equal: Vec<usize> = sets
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == want)
            .map(|(i, _)| i)
            .collect();
        let grown: Vec<usize> = sets
            .iter()
            .enumerate()
            .filter(|(_, s)| want.is_subset(s))
            .map(|(i, _)| i)
            .collect();
        for (tier, grew) in [(equal, false), (grown, true)] {
            match tier.as_slice() {
                [only] => {
                    if grew {
                        warnings.push(format!(
                            "profile [{}] is no longer a loop of its own; using the loop [{}], \
                             which contains it (the sketch was edited)",
                            ids.iter()
                                .map(u32::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                            sketch.solved_profiles[*only]
                                .entity_ids
                                .iter()
                                .map(u32::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                        ));
                    }
                    return Ok(*only);
                }
                [] => continue,
                many => {
                    return Err(EngineError::ProfileAmbiguous {
                        entity_ids: ids.to_vec(),
                        matches: many.len(),
                    })
                }
            }
        }
        return Err(not_found());
    }
    if profile_index >= count {
        return Err(EngineError::ProfileOutOfRange {
            index: profile_index,
            count,
        });
    }
    Ok(profile_index)
}

mod combine;
mod depth;
mod planes;
#[cfg(test)]
mod tests;

use combine::*;
pub(crate) use combine::{
    carry_untargeted_named, carry_untargeted_siblings, find_consumed_feature_ids,
    find_solid_handle, named_custody_outputs, resolve_combine_targets,
    untargeted_sibling_sources_named,
};
use depth::*;
use planes::*;
pub(crate) use planes::{find_datum_plane_data, sketch_basis, sketch_x_axis, unit_normal};
pub use planes::{resolve_datum_plane, resolve_face_plane, SKETCH_PLANE_FACE_ROLE};
