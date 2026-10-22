use super::*;

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
                        TargetStrategy::MostRecentLegacy => find_most_recent_consumed(
                            feature,
                            feature_results,
                            tree,
                            already_consumed,
                        ),
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
                            params.profile_entity_ids.as_deref(),
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
                        TargetStrategy::MostRecentLegacy => find_most_recent_consumed(
                            feature,
                            feature_results,
                            tree,
                            already_consumed,
                        ),
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
                            params.profile_entity_ids.as_deref(),
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
        Operation::Pipe { params } => {
            let eff = crate::types::normalize_pipe_combine(params);
            match eff.mode {
                CombineMode::NewBody => vec![],
                CombineMode::Add | CombineMode::Cut | CombineMode::Intersect => {
                    match &eff.targets {
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
                        _ => find_most_recent_consumed(
                            feature,
                            feature_results,
                            tree,
                            already_consumed,
                        ),
                    }
                }
            }
        }
        Operation::Sweep { params } => {
            let eff = crate::types::normalize_sweep_combine(params);
            match eff.mode {
                CombineMode::NewBody => vec![],
                CombineMode::Add | CombineMode::Cut | CombineMode::Intersect => {
                    match &eff.targets {
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
                        _ => find_most_recent_consumed(
                            feature,
                            feature_results,
                            tree,
                            already_consumed,
                        ),
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
        Operation::UnionAll { params } => crate::union_all::consumed_feature_ids(
            feature,
            params,
            feature_results,
            tree,
            already_consumed,
        ),
        Operation::PatternCircular { params } => crate::pattern::consumed_feature_ids(
            crate::pattern::PatternSpec::Circular(params),
            feature,
            feature_results,
            tree,
            already_consumed,
        ),
        Operation::PatternLinear { params } => crate::pattern::consumed_feature_ids(
            crate::pattern::PatternSpec::Linear(params),
            feature,
            feature_results,
            tree,
            already_consumed,
        ),
        Operation::PatternMirror { params } => crate::pattern::consumed_feature_ids(
            crate::pattern::PatternSpec::Mirror(params),
            feature,
            feature_results,
            tree,
            already_consumed,
        ),
        _ => vec![],
    }
}

/// Find the most recent solid handle from features built before the given feature.
///
/// Walks backwards through the feature tree to find the latest OpResult with a Main output.
/// All live solid outputs (`Main` + `Body{..}`) of the most recent
/// solid-bearing feature before `current_feature`, each paired with that
/// feature's id.
///
/// CUSTODY INVARIANT (2026-08-08, the R0090/R0030 base-drop anchor —
/// `docs/audits/volume_oracle_flags_anchored.md`): a boolean may mark a target
/// feature consumed ONLY if it takes custody of ALL that feature's live
/// bodies — merging what its tool touches and re-emitting the rest as its own
/// leftover `Body{index}` outputs. The previous Main-only collection paired
/// with whole-feature consumption silently DELETED a leftover body (R0090's
/// 1.74e8 base) without any boolean ever touching it.
pub(super) fn find_most_recent_solid_outputs(
    current_feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
) -> Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)> {
    let active = tree.active_features();
    // Walk backwards through features BEFORE the current one
    let current_idx = active
        .iter()
        .position(|f| f.id == current_feature.id)
        .unwrap_or(active.len());
    for feature in active[..current_idx].iter().rev() {
        // Spec `cut_consumes_body` I1: a body a later combine CONSUMED (a cut
        // that removed all its material, or a merge that absorbed it) is not
        // live — the walk must not resurrect it. Measured 2026-09-03 by the
        // exact-membership oracle (audit class C): R0034's gear revolve
        // auto-unioned with the box its cut had consumed, R0007's second cut
        // re-cut the consumed cylinder.
        if feature.suppressed || already_consumed.contains(&feature.id) {
            continue;
        }
        // Skip sketch, datum plane and connector features (they produce no solid)
        if matches!(
            &feature.operation,
            Operation::Sketch { .. }
                | Operation::Sketch3d { .. }
                | Operation::DatumPlane { .. }
                | Operation::MateConnector { .. }
        ) {
            continue;
        }
        if let Some(result) = feature_results.get(&feature.id) {
            let outs: Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)> = result
                .outputs
                .iter()
                .filter(|(key, _)| matches!(key, OutputKey::Main | OutputKey::Body { .. }))
                .map(|(_, body_output)| (feature.id, body_output.handle.clone()))
                .collect();
            if !outs.is_empty() {
                return outs;
            }
        }
    }
    Vec::new()
}

/// The most recent solid's handle alone — extent probes (`resolve_depth`
/// ThroughAll) that want one representative body, not combine custody.
pub(super) fn find_most_recent_solid(
    current_feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
) -> Option<waffle_types::kernel::KernelSolidHandle> {
    find_most_recent_solid_outputs(current_feature, feature_results, tree, already_consumed)
        .into_iter()
        .next()
        .map(|(_, handle)| handle)
}

/// Find the most recent feature with a live solid output (for consumption
/// tracking). The predicate must match `find_most_recent_solid_outputs` —
/// consumption and custody walk to the SAME feature.
pub(super) fn find_most_recent_consumed(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
) -> Vec<Uuid> {
    let active = tree.active_features();
    let current_idx = active
        .iter()
        .position(|f| f.id == feature.id)
        .unwrap_or(active.len());
    for f in active[..current_idx].iter().rev() {
        // I1 (see `find_most_recent_solid_outputs`): a consumed body is not
        // a target, so it is not consumed again.
        if f.suppressed || already_consumed.contains(&f.id) {
            continue;
        }
        if matches!(
            &f.operation,
            Operation::Sketch { .. }
                | Operation::Sketch3d { .. }
                | Operation::DatumPlane { .. }
                | Operation::MateConnector { .. }
        ) {
            continue;
        }
        if let Some(result) = feature_results.get(&f.id) {
            if result
                .outputs
                .iter()
                .any(|(key, _)| matches!(key, OutputKey::Main | OutputKey::Body { .. }))
            {
                return vec![f.id];
            }
        }
    }
    vec![]
}

/// Find the Sketch data from a feature in the tree by sketch feature ID.
pub(super) fn find_sketch_in_tree(
    sketch_id: Uuid,
    tree: &FeatureTree,
) -> Result<&Sketch, EngineError> {
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
pub(super) fn find_sketch_result(
    sketch_id: Uuid,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Result<&OpResult, EngineError> {
    feature_results
        .get(&sketch_id)
        .ok_or(EngineError::SketchNotFound { id: sketch_id })
}

/// Find the solid handle from a feature's OpResult via GeomRef.
pub(crate) fn find_solid_handle(
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
pub(super) fn dispatch_combine(
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

/// The explicit targets of a combine that RESOLVE, as `(feature_id, output_key)`,
/// in target order. Empty for NewBody and for non-explicit target strategies.
pub(super) fn resolved_explicit_targets(
    eff: &crate::types::EffectiveCombine,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Vec<(Uuid, OutputKey)> {
    if matches!(eff.mode, CombineMode::NewBody) {
        return Vec::new();
    }
    let TargetStrategy::Explicit(list) = &eff.targets else {
        return Vec::new();
    };
    list.iter()
        .filter(|gr| find_solid_handle(gr, feature_results).is_ok())
        .filter_map(|gr| match &gr.anchor {
            waffle_types::Anchor::FeatureOutput {
                feature_id,
                output_key,
            } => Some((*feature_id, output_key.clone())),
            _ => None,
        })
        .collect()
}

/// The outputs a name-based custody feature carries unchanged, as `(feature_id,
/// output_key)` in the order [`carry_untargeted_named`] appends them: for each
/// named feature (first-mention order), its Main/Body outputs that no named
/// output claims. The single source for both the carried bodies and their
/// inherited names (`Engine::inherit_source_body_id`).
pub(crate) fn untargeted_sibling_sources_named(
    targeted: &[(Uuid, OutputKey)],
    feature_results: &HashMap<Uuid, OpResult>,
) -> Vec<(Uuid, OutputKey)> {
    let mut seen = std::collections::HashSet::new();
    let mut siblings = Vec::new();
    for (fid, _) in targeted {
        if !seen.insert(*fid) {
            continue;
        }
        let Some(source) = feature_results.get(fid) else {
            continue;
        };
        for (key, _) in &source.outputs {
            if !matches!(key, OutputKey::Main | OutputKey::Body { .. }) {
                continue;
            }
            if targeted.iter().any(|(f, k)| f == fid && k == key) {
                continue;
            }
            siblings.push((*fid, key.clone()));
        }
    }
    siblings
}

/// [`carry_untargeted_siblings`] for an explicit custody list (patterns).
pub(crate) fn carry_untargeted_named(
    result: &mut OpResult,
    named: &[(Uuid, OutputKey)],
    feature_results: &HashMap<Uuid, OpResult>,
) {
    for (fid, key) in untargeted_sibling_sources_named(named, feature_results) {
        let Some(body) = feature_results
            .get(&fid)
            .and_then(|r| r.outputs.iter().find(|(k, _)| *k == key))
            .map(|(_, b)| b.clone())
        else {
            continue;
        };
        let index = result.outputs.len();
        result.outputs.push((OutputKey::Body { index }, body));
        result.diagnostics.warnings.push(format!(
            "output {key:?} of feature {fid} was not targeted; kept unchanged as a separate body"
        ));
    }
}

/// The outputs `feature` takes custody of **by name**, as `(feature_id,
/// output_key)` — the operands/targets it names in its own params. `None` when
/// the feature's custody is not name-based (a legacy most-recent combine takes
/// ALL of its target feature's live bodies; a pattern builds its own list; a
/// non-solid op names nothing).
///
/// Naming SOME outputs of a multi-output feature while consumption is tracked
/// per FEATURE is the shape of the custody defect: the unnamed siblings are
/// hidden with no boolean ever touching them. Every caller that consumes whole
/// features off a named list must therefore carry those siblings
/// ([`carry_untargeted_named`]) and let them inherit their names
/// (`Engine::inherit_source_body_id`). One list, so the two can never disagree.
pub(crate) fn named_custody_outputs(
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
) -> Option<Vec<(Uuid, OutputKey)>> {
    match &feature.operation {
        Operation::Extrude { .. }
        | Operation::Revolve { .. }
        | Operation::Pipe { .. }
        | Operation::Sweep { .. } => {
            let eff = match &feature.operation {
                Operation::Extrude { params } => crate::types::normalize_extrude_combine(params),
                Operation::Revolve { params } => crate::types::normalize_revolve_combine(params),
                Operation::Pipe { params } => crate::types::normalize_pipe_combine(params),
                Operation::Sweep { params } => crate::types::normalize_sweep_combine(params),
                _ => unreachable!("outer match restricts the operation"),
            };
            if matches!(eff.mode, crate::types::CombineMode::NewBody)
                || !matches!(eff.targets, TargetStrategy::Explicit(_))
            {
                return None;
            }
            Some(resolved_explicit_targets(&eff, feature_results))
        }
        // A pair boolean names exactly one output per operand, yet consumes
        // both operand FEATURES whole (`find_consumed_feature_ids`): every
        // other live body of either feature is in its custody. P0010/P0011
        // (2026-10-03) lost 64 % and 25 % of the model this way, silently.
        Operation::BooleanCombine { params } => Some(
            [&params.body_a, &params.body_b]
                .into_iter()
                .filter(|gr| find_solid_handle(gr, feature_results).is_ok())
                .filter_map(|gr| match &gr.anchor {
                    waffle_types::Anchor::FeatureOutput {
                        feature_id,
                        output_key,
                    } => Some((*feature_id, output_key.clone())),
                    _ => None,
                })
                .collect(),
        ),
        // `All` folds every live body of every live feature, so it names
        // nothing it does not already hold; `Selected` names a body list.
        Operation::UnionAll { params } => {
            crate::union_all::named_custody_outputs(params, feature_results)
        }
        _ => None,
    }
}

/// Carry the outputs a feature consumes but never names — the custody rule.
///
/// Consumption is tracked per FEATURE, so naming only SOME outputs of a
/// multi-output feature consumed the whole feature and silently dropped the
/// rest (docs/notes/agent_bicycle_session_failures_2026_09_14.md F9 for an
/// explicit-target cut; P0010/P0011 for a `BooleanCombine`). Re-emit them
/// unchanged as extra outputs of the consuming feature, with a warning — the
/// custody rule the legacy most-recent path already follows.
pub(crate) fn carry_untargeted_siblings(
    result: &mut OpResult,
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
) {
    let Some(named) = named_custody_outputs(feature, feature_results) else {
        return;
    };
    carry_untargeted_named(result, &named, feature_results);
}

/// Compute the "share-a-face" auto target set (spec §4.3).
///
/// **N-mb-3a — anchor-ownership path (a):** the body whose face the sketch is
/// drawn on, read from the sketch's `plane` GeomRef anchor. A sketch drawn on a
/// datum plane has a `Datum` anchor and yields no target (⇒ a new body). The
/// geometric plane-coincidence + profile-overlap path (b) and multi-body
/// coincidence are N-mb-3b. Consumed (non-live) bodies are excluded.
///
/// `profile_index` / `profile_entity_ids`: the execute path passes an
/// already-resolved index and no id set; the consumption pre-pass passes the
/// raw params (v4 §2.9). A profile the sketch cannot resolve has no footprint
/// and yields no geometric targets — the execute path reports that error.
/// Re-resolve an extrude's stored region(s) against the current sketch by
/// boundary identity ([`waffle_types::resolve_region_by_identity`]). A region
/// with no provenance, or whose signature no longer exists in the sketch
/// (an entity was deleted, the loop was broken), keeps its stored geometry and
/// a warning is pushed so the user learns the extrude is running on a stale
/// footprint rather than silently building the wrong thing.
pub(super) fn resolve_extrude_regions(
    sketch: &Sketch,
    region: Option<&waffle_types::Region>,
    regions: &[waffle_types::Region],
    warnings: &mut Vec<String>,
) -> (Option<waffle_types::Region>, Vec<waffle_types::Region>) {
    let needs = region.is_some_and(|r| r.boundary_entity_ids.is_some())
        || regions.iter().any(|r| r.boundary_entity_ids.is_some());
    if !needs {
        return (region.cloned(), regions.to_vec());
    }
    let current = waffle_types::compute_regions(
        &sketch.entities,
        &sketch.solved_positions,
        waffle_types::regions::DEFAULT_CHORD_TOLERANCE,
    );
    let resolve_one = |stored: &waffle_types::Region, warnings: &mut Vec<String>| {
        if stored.boundary_entity_ids.is_none() {
            return stored.clone();
        }
        match waffle_types::resolve_region_by_identity(stored, &current) {
            Some(r) => r.clone(),
            None => {
                warnings.push(
                    "extrude region no longer matches the sketch (its boundary entities \
                     changed); using the region as originally picked"
                        .into(),
                );
                stored.clone()
            }
        }
    };
    let one = region.map(|r| resolve_one(r, warnings));
    let many = regions.iter().map(|r| resolve_one(r, warnings)).collect();
    (one, many)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_share_a_face(
    sketch_id: Uuid,
    profile_index: usize,
    profile_entity_ids: Option<&[u32]>,
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
                                // This is the consumption PRE-PASS, not the build: it answers "which bodies
                                // would this feature touch". Its warnings are dropped because the execute
                                // path resolves the same profile a moment later and reports them there —
                                // keeping them here would say everything twice.
    let Ok(profile_index) =
        resolve_profile_index(&sketch, profile_index, profile_entity_ids, &mut Vec::new())
    else {
        return Vec::new();
    };
    // Same region identity as the build pass: the footprint must be the
    // region as the sketch is NOW (idempotent when already resolved).
    let mut region_warnings = Vec::new();
    let (region_now, regions_now) =
        resolve_extrude_regions(&sketch, region, regions, &mut region_warnings);
    let region = region_now.as_ref();
    let regions: &[waffle_types::Region] = &regions_now;

    let mut out: Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)> = Vec::new();
    let mut seen: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    // (a) Anchor ownership: the sketch is drawn on a body's face → that body,
    // unless it was already consumed by an earlier feature (not live). A plane
    // scoped to another instance (in-context) owns no body of THIS part.
    if let (
        None,
        waffle_types::Anchor::FeatureOutput {
            feature_id,
            output_key,
        },
    ) = (&sketch.plane.scope, &sketch.plane.anchor)
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
            let s_n = unit_normal(sketch.plane_normal);
            let x_axis = sketch_x_axis(&sketch);
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
                    Operation::Sketch { .. }
                        | Operation::Sketch3d { .. }
                        | Operation::DatumPlane { .. }
                        | Operation::MateConnector { .. }
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
pub(super) fn profile_footprint_2d(
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
pub(super) fn body_face_shares_sketch(
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
        // PLANAR only. Since N0 (`specs/agent_mechanical_design.md` §5.1) a
        // curved face reports a centroid and the outward normal there too, so
        // "has a normal and a centroid" no longer means "is a plane": a
        // cylinder or cone patch whose centroid normal happens to be
        // (anti)parallel to the sketch normal, with the centroid on the sketch
        // plane, would pass `plane_coincident` and be taken for a face the
        // sketch shares. The four role assigners took the same filter.
        if sig.surface_type.as_deref() != Some("planar") {
            continue;
        }
        let (Some(fnorm), Some(fc)) = (sig.normal, sig.centroid) else {
            continue;
        };
        if !crate::share_a_face::plane_coincident(s_o, s_n, fnorm, fc) {
            continue;
        }
        // The face's footprint is its BOUNDARY GEOMETRY, not its vertex set:
        // a curved edge's vertices under-represent it, and a circular cap has
        // a single seam vertex — with vertices alone such a face never
        // reached three points and a cut sketched on a cylinder cap found no
        // target ("Cut requires at least one target body").
        let mut pts2d: Vec<[f64; 2]> = Vec::new();
        for edge in intro.face_edges(face) {
            for p in intro.edge_polyline(edge) {
                let d = [p[0] - s_o[0], p[1] - s_o[1], p[2] - s_o[2]];
                pts2d.push([
                    d[0] * x_axis[0] + d[1] * x_axis[1] + d[2] * x_axis[2],
                    d[0] * y_axis[0] + d[1] * y_axis[1] + d[2] * y_axis[2],
                ]);
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
/// - `MostRecentLegacy` → ALL live bodies of the most recent solid-bearing
///   feature (2026-08-08 custody invariant; byte-identical to the historic
///   single-handle behavior whenever that feature carries one body, which the
///   old behavior silently required — a multi-body feature lost its leftover
///   bodies to whole-feature consumption, the R0090/R0030 base-drop).
/// - `Explicit(list)` → each listed body, resolved via `find_solid_handle`.
///   A target that fails to resolve (deleted / suppressed / rolled-back body)
///   is handled per its `GeomRef::policy` (spec §9): `Strict` ⇒ a loud
///   `ResolutionFailed`; `BestEffort` ⇒ drop that target and push a warning so
///   the remaining live targets still combine. (Empty list ⇒ empty set.)
/// - `ShareAFace` → **not yet implemented** (sub-increment N-mb-3); a loud STOP.
pub(crate) fn resolve_combine_targets(
    targets: &TargetStrategy,
    feature: &Feature,
    feature_results: &HashMap<Uuid, OpResult>,
    tree: &FeatureTree,
    already_consumed: &std::collections::HashSet<Uuid>,
    warnings: &mut Vec<String>,
) -> Result<Vec<(Uuid, waffle_types::kernel::KernelSolidHandle)>, EngineError> {
    match targets {
        TargetStrategy::ShareAFace => Err(EngineError::ResolutionFailed {
            reason: "share-a-face auto-target selection is not yet implemented (N-mb-3); \
                     pick explicit target bodies for now"
                .to_string(),
        }),
        TargetStrategy::MostRecentLegacy => {
            // ALL live bodies of the most recent solid-bearing feature — the
            // custody invariant (see `find_most_recent_solid_outputs`): the
            // fold merges what the tool touches and re-emits the rest as
            // leftovers, so consuming the whole feature is sound. Byte-
            // identical to the old single-handle behavior for every
            // single-body feature.
            Ok(find_most_recent_solid_outputs(
                feature,
                feature_results,
                tree,
                already_consumed,
            ))
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
                // A target whose feature an earlier feature CONSUMED is not
                // live: its stale handle would duplicate the body
                // (`specs/b4_balanced_union.md` §2.4). Same policy split as
                // an unresolvable target.
                if already_consumed.contains(&feature_id) {
                    let msg = format!(
                        "combine target body of feature {feature_id} was already consumed by \
                         an earlier feature"
                    );
                    match gr.policy {
                        waffle_types::ResolvePolicy::Strict => {
                            return Err(EngineError::ResolutionFailed { reason: msg });
                        }
                        waffle_types::ResolvePolicy::BestEffort => {
                            warnings.push(format!("{msg}; dropped"));
                            continue;
                        }
                    }
                }
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
