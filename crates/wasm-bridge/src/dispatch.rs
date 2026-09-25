use base64::Engine as _;
use feature_engine::types::{ImportedBodyParams, Operation, Provenance, ProvenanceOrigin};
use file_format::{
    git_blob_sha1, Embed, ProjectMetadata, SourceEntry, SourceKind, TabKind, WaffleDocument,
};
use modeling_ops::KernelBundle;
use waffle_types::kernel::{RenderMesh, RigidPlacement, StepExportBody};
use waffle_types::OutputKey;

use crate::engine_state::{BridgeError, EngineState};
use crate::messages::{
    AssemblyStatus, ConnectorFrameInfo, ContextInstanceInfo, ContextStatus, DocumentInfo,
    EngineToUi, PartConnectorInfo, SourceStatus, UiToEngine,
};
use crate::messages::{ListedFace, MeasureMethod, Measured};
use crate::session::DocumentSession;

/// Dispatch a UI message to the engine and return a response.
///
/// This is the main entry point for processing messages from the JavaScript
/// main thread. Each message is dispatched to the appropriate engine method,
/// and the result is converted to an EngineToUi response.
pub fn dispatch(state: &mut EngineState, msg: UiToEngine, kb: &mut dyn KernelBundle) -> EngineToUi {
    match handle_message(state, msg, kb) {
        Ok(response) => response,
        Err(e) => EngineToUi::Error {
            kind: match &e {
                BridgeError::Engine(err) => Some(err.into()),
                _ => None,
            },
            message: e.to_string(),
            feature_id: None,
        },
    }
}

fn handle_message(
    state: &mut EngineState,
    msg: UiToEngine,
    kb: &mut dyn KernelBundle,
) -> Result<EngineToUi, BridgeError> {
    match msg {
        // -- Sketch operations --
        UiToEngine::BeginSketch { plane } => {
            state.begin_sketch(plane);
            Ok(model_updated_response(state))
        }

        UiToEngine::AddSketchEntity { entity } => {
            state.add_sketch_entity(entity)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::AddConstraint { constraint } => {
            state.add_sketch_constraint(constraint)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::SolveSketch {
            entities,
            constraints,
        } => {
            // Atomically replace the active sketch with the UI's live state
            // (when provided) before solving — one round-trip, no races.
            if let Some(entities) = entities {
                state.set_sketch_entities(entities)?;
            }
            if let Some(constraints) = constraints {
                state.set_sketch_constraints(constraints)?;
            }
            let sketch = state.build_sketch()?;
            let solved = sketch_solver::solve_sketch(&sketch);
            if let Some(active) = state.active_sketch.as_mut() {
                active.solve_status = solved.status.clone();
            }
            Ok(EngineToUi::SketchSolved { solved })
        }

        UiToEngine::FinishSketch {
            solved_positions,
            solved_profiles,
            plane_origin,
            plane_normal,
            plane_x_axis,
            entities,
            constraints,
            projected,
            provenance,
        } => {
            let sketch = state.finish_sketch(
                solved_positions,
                solved_profiles,
                plane_origin,
                plane_normal,
                plane_x_axis,
                entities,
                constraints,
                projected,
            )?;
            let op = Operation::Sketch { sketch };
            let id = state.engine.add_feature_with_provenance(
                "Sketch".to_string(),
                op,
                provenance,
                kb,
            )?;
            Ok(model_updated_for(state, id))
        }

        UiToEngine::ImportStep { file_name, data } => {
            // v4: the STEP text becomes a packed `Embedded` source; the
            // feature names it by id and carries Import provenance.
            let entry = SourceEntry::embedded(file_name.clone(), SourceKind::Step, &data);
            let id = add_import_feature(state, kb, entry, &file_name, &data)?;
            Ok(model_updated_for(state, id))
        }

        UiToEngine::ImportStepFromLocator {
            file_name,
            locator,
            data,
            resolved_commit,
        } => {
            if !locator.is_shareable() {
                return Err(BridgeError::InvalidRequest {
                    reason: "ImportStepFromLocator: a Local locator cannot be linked".to_string(),
                });
            }
            let mut entry = SourceEntry::linked(file_name.clone(), SourceKind::Step, locator);
            entry.set_content(&data); // hash; no embed (linked ⇒ pack false)
            entry.fetched_at = Some(chrono::Utc::now());
            entry.resolved = resolved_commit.map(|c| file_format::Resolved {
                commit: c.to_ascii_lowercase(),
                at: chrono::Utc::now(),
            });
            add_import_feature(state, kb, entry, &file_name, &data)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::ListSources => Ok(EngineToUi::SourcesListed {
            sources: source_statuses(state),
        }),

        UiToEngine::ReadSource { source_id } => {
            let entry = state
                .sources
                .iter()
                .find(|s| s.id == source_id)
                .ok_or_else(|| BridgeError::InvalidRequest {
                    reason: format!("ReadSource: {source_id} is not in the sources table"),
                })?;
            let text = state.engine.sources.text(source_id).ok_or_else(|| {
                BridgeError::InvalidRequest {
                    reason: format!(
                        "ReadSource: the content of `{}` ({source_id}) is not loaded",
                        entry.name
                    ),
                }
            })?;
            Ok(EngineToUi::SourceContent {
                source_id,
                name: entry.name.clone(),
                kind: source_kind_tag(&entry.kind),
                text,
            })
        }

        UiToEngine::AddScriptSource {
            name,
            text,
            library,
        } => {
            let text = match (text, library.as_deref()) {
                (Some(t), None) => t,
                (None, Some(lib)) => library_script(lib)?.to_string(),
                (Some(_), Some(_)) => {
                    return Err(BridgeError::InvalidRequest {
                        reason: "AddScriptSource: give `text` or `library`, not both".to_string(),
                    })
                }
                (None, None) => {
                    return Err(BridgeError::InvalidRequest {
                        reason: "AddScriptSource: `text` or `library` is required".to_string(),
                    })
                }
            };
            let check = check_script(&text, DEFAULT_SCRIPT_ENTRY, None);
            let name = name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .or_else(|| feature_engine::script::display_name(&text))
                .unwrap_or_else(|| "script.rhai".to_string());
            let entry = SourceEntry::embedded(name.clone(), SourceKind::Script, &text);
            let source_id = entry.id;
            state.engine.sources.insert_text(source_id, &text);
            state.sources.push(entry);
            Ok(EngineToUi::ScriptSourceAdded {
                source_id,
                name,
                sources: source_statuses(state),
                check,
            })
        }

        UiToEngine::SetScriptSource { source_id, text } => {
            let entry = state
                .sources
                .iter_mut()
                .find(|s| s.id == source_id)
                .ok_or_else(|| BridgeError::InvalidRequest {
                    reason: format!("SetScriptSource: {source_id} is not in the sources table"),
                })?;
            if !matches!(entry.kind, SourceKind::Script) {
                return Err(BridgeError::InvalidRequest {
                    reason: format!(
                        "SetScriptSource: `{}` is a {} source, not a script",
                        entry.name,
                        source_kind_tag(&entry.kind)
                    ),
                });
            }
            entry.content_hash = Some(git_blob_sha1(text.as_bytes()));
            state.engine.sources.insert_text(source_id, &text);
            state.engine.rebuild_from_scratch(kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::CheckScript {
            source_id,
            text,
            entry,
            args,
        } => {
            let text = match (text, source_id) {
                (Some(t), _) => t,
                (None, Some(id)) => stored_script_text(state, id)?,
                (None, None) => {
                    return Err(BridgeError::InvalidRequest {
                        reason: "CheckScript: `text` or `source_id` is required".to_string(),
                    })
                }
            };
            let entry = entry
                .filter(|e| !e.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_SCRIPT_ENTRY.to_string());
            Ok(EngineToUi::ScriptChecked {
                source_id,
                check: check_script(&text, &entry, args.map(|a| (source_id, a))),
            })
        }

        UiToEngine::UpdateSourceEntry {
            source_id,
            pack,
            git_ref,
        } => {
            let entry = state
                .sources
                .iter_mut()
                .find(|s| s.id == source_id)
                .ok_or_else(|| BridgeError::InvalidRequest {
                    reason: format!("UpdateSourceEntry: {source_id} is not in the sources table"),
                })?;
            if let Some(p) = pack {
                if !p && matches!(entry.locator, file_format::Locator::Embedded) {
                    return Err(BridgeError::InvalidRequest {
                        reason: format!(
                            "source `{}` is embedded (no origin); it cannot be unpacked",
                            entry.name
                        ),
                    });
                }
                entry.pack = Some(p);
            }
            let mut drop_content = false;
            if let Some(new_ref) = git_ref {
                let file_format::Locator::Git { git_ref, .. } = &mut entry.locator else {
                    return Err(BridgeError::InvalidRequest {
                        reason: format!("source `{}` is not a git source", entry.name),
                    });
                };
                let keeps_content = matches!(
                    (&new_ref, &entry.resolved),
                    (file_format::GitRef::Commit { sha }, Some(r)) if r.commit.eq_ignore_ascii_case(sha)
                );
                let previous = std::mem::replace(git_ref, new_ref);
                let problems = entry.locator.validate();
                if !problems.is_empty() {
                    if let file_format::Locator::Git { git_ref, .. } = &mut entry.locator {
                        *git_ref = previous;
                    }
                    return Err(BridgeError::InvalidRequest {
                        reason: problems.join("; "),
                    });
                }
                if let file_format::Locator::Git {
                    git_ref: file_format::GitRef::Commit { sha },
                    ..
                } = &mut entry.locator
                {
                    *sha = sha.to_ascii_lowercase();
                }
                if !keeps_content {
                    // Content from one commit must never be labelled with
                    // another: drop it and let the host re-resolve.
                    entry.resolved = None;
                    entry.content_hash = None;
                    entry.embed = None;
                    entry.fetched_at = None;
                    drop_content = true;
                }
            }
            if drop_content {
                state.engine.sources.remove(source_id);
                state.engine.rebuild_from_scratch(kb);
            }
            Ok(model_updated_response(state))
        }

        // -- Feature operations --
        UiToEngine::AddFeature {
            operation,
            provenance,
        } => {
            let name = feature_name_for(state, &operation);
            let id = state
                .engine
                .add_feature_with_provenance(name, operation, provenance, kb)?;
            Ok(model_updated_for(state, id))
        }

        UiToEngine::EditFeature {
            feature_id,
            operation,
            provenance,
        } => {
            state
                .engine
                .edit_feature_with_provenance(feature_id, operation, provenance, kb)?;
            Ok(model_updated_for(state, feature_id))
        }

        UiToEngine::DeleteFeature { feature_id } => {
            state.engine.remove_feature(feature_id, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::SuppressFeature {
            feature_id,
            suppressed,
        } => {
            state.engine.set_suppressed(feature_id, suppressed, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::ReorderFeature {
            feature_id,
            new_position,
        } => {
            state.engine.reorder_feature(feature_id, new_position, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::RenameFeature {
            feature_id,
            new_name,
        } => {
            state.engine.rename_feature(feature_id, new_name)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::RenameBody { body_id, new_name } => {
            state.engine.rename_body(body_id, new_name);
            Ok(model_updated_response(state))
        }

        UiToEngine::SetRollbackIndex { index } => {
            state.engine.set_rollback(index, kb);
            Ok(model_updated_response(state))
        }

        // -- History --
        UiToEngine::Undo => {
            state.engine.undo(kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::Redo => {
            state.engine.redo(kb)?;
            Ok(model_updated_response(state))
        }

        // -- Selection --
        UiToEngine::SelectEntity { geom_ref } => {
            state.selection = vec![geom_ref.clone()];
            Ok(EngineToUi::SelectionChanged {
                geom_refs: vec![geom_ref],
            })
        }

        UiToEngine::HoverEntity { geom_ref } => {
            state.hover = geom_ref.clone();
            Ok(EngineToUi::HoverChanged { geom_ref })
        }

        // -- File operations --
        UiToEngine::SaveProject => {
            let meta =
                ProjectMetadata::new(state.project_name()).with_display_unit(state.display_unit());
            let mut doc = WaffleDocument::single_part(&meta, state.engine.tree.clone());
            // `single_part` lifted any legacy inline payloads into fresh
            // entries; the live tree's `source_id`s point at the document's
            // own table, which carries the store's content.
            let mut sources = sources_for_save(state);
            sources.append(&mut doc.sources);
            doc.sources = sources;
            let json_data = verified(state, &doc)?;
            Ok(EngineToUi::SaveReady { json_data })
        }

        UiToEngine::SaveDocument => {
            // The session composes the file (S2 C3c, v4 §4 inv. 7 one writer).
            // Nothing is handed over any more: it holds the metadata, every
            // tab with its tree and thumbnail, and which tab is active, and
            // `to_document` stashes the live tree into the active tab on the
            // way. An active tab of a kind this build cannot open keeps its
            // content verbatim rather than being refused — the live tree is
            // empty while such a tab is open, so there is nothing to stamp.
            let sources = sources_for_save(state);
            let envelope_extra = state.envelope_extra.clone();
            let mut doc = state.session.to_document(
                &mut state.engine,
                sources,
                chrono::Utc::now(),
                envelope_extra,
            );
            // Unknown `document.*` keys the UI does not carry: re-attach what
            // load captured (v4 §2.6).
            for (k, v) in &state.document_extra {
                doc.document
                    .extra
                    .entry(k.clone())
                    .or_insert_with(|| v.clone());
            }
            // Tabs loaded from a v3 file may still carry inline STEP payloads;
            // lift them so the file is uniformly v4.
            let _ = doc.lift_inline_payloads();
            let json_data = verified(state, &doc)?;
            Ok(EngineToUi::SaveReady { json_data })
        }

        UiToEngine::LoadProject { data } => {
            let loaded =
                file_format::load_document(&data).map_err(|e| BridgeError::Serialization {
                    reason: e.to_string(),
                })?;
            let mut doc = loaded.document;
            let tab = doc
                .active_tab()
                .or_else(|| doc.tabs.first())
                .ok_or_else(|| BridgeError::Serialization {
                    reason: "no tabs in document".to_string(),
                })?;
            let tree = match &tab.kind {
                TabKind::Part { features, .. } => features.clone(),
                // The assembly itself is evaluated by `OpenAssembly` (the UI
                // sends it with the part trees); the live tree stays empty.
                TabKind::Assembly { .. } => feature_engine::types::FeatureTree::new(),
                TabKind::Unknown(_) => {
                    return Err(BridgeError::NotImplemented {
                        operation: format!(
                            "opening a `{}` tab (tab `{}`) in this version",
                            tab.kind.type_tag(),
                            tab.name
                        ),
                    })
                }
            };
            // Adopt the sources table; register every usable embed.
            state.engine.sources.clear();
            for (id, text) in doc.embedded_contents() {
                state.engine.sources.insert_text(id, &text);
            }
            state.sources = std::mem::take(&mut doc.sources);
            state.document_extra = doc.document.extra.clone();
            state.envelope_extra = doc.extra.clone();
            // The session takes the rest of the document: metadata (the name
            // and display unit that used to be a second copy on EngineState),
            // the tab list, and every inactive tab's tree. `tree` below is the
            // active tab's, so the live tree and the session agree from the
            // first message (S2 C2).
            state.session = DocumentSession::from_document(doc);
            state.engine.tree = tree;
            // Another document's parts are of no use to this one.
            state.assembly = None;
            state.clear_context();
            state.part_cache.clear();
            state.engine.rebuild_from_scratch(kb);
            state.engine.warnings.extend(
                loaded
                    .warnings
                    .into_iter()
                    .map(|w| format!("document: {w}")),
            );
            Ok(model_updated_response(state))
        }

        UiToEngine::ProvideSource {
            source_id,
            data,
            resolved_commit,
        } => {
            let entry = state
                .sources
                .iter_mut()
                .find(|s| s.id == source_id)
                .ok_or_else(|| BridgeError::InvalidRequest {
                    reason: format!("ProvideSource: {source_id} is not in the sources table"),
                })?;
            entry.content_hash = Some(git_blob_sha1(data.as_bytes()));
            entry.fetched_at = Some(chrono::Utc::now());
            if let Some(commit) = resolved_commit {
                entry.resolved = Some(file_format::Resolved {
                    commit: commit.to_ascii_lowercase(),
                    at: chrono::Utc::now(),
                });
            }
            state.engine.sources.insert_text(source_id, &data);
            state.engine.rebuild_from_scratch(kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::RebaseSources { base, commit } => {
            if !matches!(base, file_format::Locator::Git { .. }) {
                return Err(BridgeError::InvalidRequest {
                    reason: "RebaseSources: base must be a Git locator".to_string(),
                });
            }
            file_format::rebase_relative_sources(
                &mut state.sources,
                &base,
                &commit,
                chrono::Utc::now(),
            );
            // No geometry changed; the sources table did.
            Ok(model_updated_response(state))
        }

        // -- Tab / document management --
        // The session owns the tab bar (S2 C3): each of these names a tab
        // rather than handing over its content, so the engine and the UI can
        // no longer hold two tab lists that disagree.
        UiToEngine::SwitchTab { tab_id } => {
            switch_to_tab(state, &tab_id, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::AddTab { kind, name } => {
            // Appended last, and NOT activated: the caller sends `SwitchTab`
            // if it wants it open.
            state.session.add_tab(&kind, name)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::CloseTab { tab_id } => {
            // Closing an inactive tab touches no geometry; closing the active
            // one names a successor, which becomes the live tree.
            if let Some(successor) = state.session.close_tab(&tab_id)? {
                switch_to_tab(state, &successor, kb)?;
            }
            Ok(model_updated_response(state))
        }

        UiToEngine::RenameTab { tab_id, name } => {
            state.session.rename_tab(&tab_id, name)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::MoveTab { tab_id, index } => {
            state.session.move_tab(&tab_id, index)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::ListSourceTabs { source_id } => {
            let tabs = crate::assembly_view::source_tabs(source_id, &state.engine.sources)
                .map_err(|reason| BridgeError::InvalidRequest { reason })?
                .into_iter()
                .map(|(id, name, kind)| crate::messages::SourceTabInfo { id, name, kind })
                .collect();
            Ok(EngineToUi::SourceTabsListed { source_id, tabs })
        }

        UiToEngine::ProbeConnectorRef {
            instance_path,
            geom_ref,
        } => {
            let view = state
                .assembly
                .as_ref()
                .ok_or_else(|| BridgeError::InvalidRequest {
                    reason: "no assembly is open, so there is nothing to place a connector on"
                        .to_string(),
                })?;
            let engine = view.engine_for_path(&instance_path).ok_or_else(|| {
                BridgeError::InvalidRequest {
                    reason: format!(
                        "{instance_path:?} is not a rendered part of the open assembly"
                    ),
                }
            })?;
            Ok(
                match feature_engine::connector::resolve_connector_frame(
                    &geom_ref,
                    &engine.feature_results,
                    kb.as_introspect(),
                    feature_engine::assembly::AxialAnchor::Middle,
                ) {
                    Ok((_, kind)) => EngineToUi::ConnectorRefProbed {
                        ok: true,
                        kind: Some(kind.label().to_string()),
                        reason: None,
                    },
                    Err(e) => EngineToUi::ConnectorRefProbed {
                        ok: false,
                        kind: None,
                        reason: Some(e.to_string()),
                    },
                },
            )
        }

        UiToEngine::OpenAssembly { tab_id } => {
            open_assembly(state, &tab_id, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::EditAssembly { tab_id, assembly } => {
            state.session.set_assembly(&tab_id, assembly)?;
            // Re-evaluate only what is on screen: editing a background
            // assembly tab records the change, and opening that tab shows it.
            if state.session.active_tab_id() == tab_id {
                open_assembly(state, &tab_id, kb)?;
            }
            Ok(model_updated_response(state))
        }

        UiToEngine::OpenPartInContext {
            tab_id,
            assembly_tab_id,
            instance_path,
        } => {
            state.active_sketch = None;
            state.selection.clear();
            state.hover = None;
            // The views being left keep their part engines for this
            // evaluation (the parts that did not change are not rebuilt).
            let reuse = state.take_part_engines();
            // Opening a part in context is a tab switch too (the store leaves
            // the assembly tab for the part's), and it happens FIRST: the
            // stash must capture the outgoing tab's live tree before anything
            // replaces it, a refused tab id must not leave a context view
            // behind, and the switch is what makes the part's tree live — so
            // `part_trees` below carries the very tree being edited.
            state.session.switch_tab(&tab_id, &mut state.engine)?;
            let assembly = state.session.assembly(&assembly_tab_id)?.clone();
            let part_trees = state.session.part_trees(&state.engine);
            let assembly_trees = state.session.assembly_trees();
            let view = crate::assembly_view::evaluate(
                assembly,
                &part_trees,
                &assembly_trees,
                &state.engine.sources,
                kb,
                reuse,
            );
            let (context_view, context) =
                crate::assembly_view::ContextView::new(view, assembly_tab_id, instance_path)
                    .map_err(|reason| BridgeError::InvalidRequest { reason })?;
            state.context_view = Some(context_view);
            state.engine.context = Some(context);
            state.engine.rebuild_from_scratch(kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::NewDocument => {
            state.reset();
            Ok(model_updated_response(state))
        }

        // -- Settings --
        UiToEngine::SetDocumentMeta {
            name,
            display_unit,
            id,
            created,
        } => {
            state.session.set_meta(name, display_unit, id, created);
            Ok(model_updated_response(state))
        }

        // -- Design parameters (variables) --
        UiToEngine::SetParameters { parameters } => {
            state.engine.set_parameters(parameters, kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::EvaluateExpression { expression } => {
            let env = feature_engine::params::cached_env(&state.engine.tree.parameters);
            match feature_engine::expr::evaluate(&expression, &env) {
                Ok(v) => Ok(EngineToUi::ExpressionEvaluated {
                    value: Some(v),
                    error: None,
                }),
                Err(e) => Ok(EngineToUi::ExpressionEvaluated {
                    value: None,
                    error: Some(e.to_string()),
                }),
            }
        }

        UiToEngine::ExportStep => {
            // Whole model: every live body of the part — or, with an assembly
            // open, every rendered instance's bodies at their world
            // placements (a flat multi-body file) — written analytically.
            let (bodies, warnings) = step_export_bodies(state);
            if bodies.is_empty() {
                return Err(BridgeError::NoMeshData);
            }
            let file_name = format!("{}.step", state.project_name());
            let step_data = kb.export_step_bodies(&bodies, &file_name).map_err(|e| {
                BridgeError::Engine(feature_engine::types::EngineError::RebuildFailed {
                    feature_name: "STEP export".to_string(),
                    reason: format!("{}", e),
                })
            })?;
            Ok(EngineToUi::ExportReady {
                step_data,
                warnings,
            })
        }

        // -- Gear generation (stateless) --
        UiToEngine::GenerateGearPreview { params } => {
            let polyline = waffle_types::generate_gear_preview_polyline(&params);
            Ok(EngineToUi::GearPreviewGenerated { polyline })
        }

        UiToEngine::GenerateGearProfile { params } => {
            let result = waffle_types::generate_gear_profile(&params);
            Ok(EngineToUi::GearProfileGenerated {
                entities: result.entities,
                positions: result.positions,
                profiles: result.profiles,
                pitch_radius: result.pitch_radius,
            })
        }

        UiToEngine::GenerateSprocketPreview { params } => {
            match waffle_types::generate_sprocket_preview_polyline(&params) {
                Ok(polyline) => Ok(EngineToUi::SprocketPreviewGenerated { polyline }),
                Err(e) => Err(BridgeError::InvalidRequest {
                    reason: e.to_string(),
                }),
            }
        }

        UiToEngine::GenerateSprocketProfile { params } => {
            match waffle_types::generate_sprocket_profile(&params) {
                Ok(result) => Ok(EngineToUi::SprocketProfileGenerated {
                    entities: result.entities,
                    positions: result.positions,
                    profiles: result.profiles,
                    pitch_radius: result.pitch_radius,
                    dimensions: result.dimensions,
                }),
                Err(e) => Err(BridgeError::InvalidRequest {
                    reason: e.to_string(),
                }),
            }
        }

        UiToEngine::GeneratePlanetary { params } => {
            match waffle_types::generate_planetary(&params) {
                Ok(result) => Ok(EngineToUi::PlanetaryGenerated { result }),
                Err(e) => Err(BridgeError::InvalidRequest {
                    reason: e.to_string(),
                }),
            }
        }

        UiToEngine::GeneratePlanetaryPreview { params } => {
            let polylines = waffle_types::generate_planetary_preview(&params);
            Ok(EngineToUi::PlanetaryPreviewGenerated { polylines })
        }

        UiToEngine::ComputeRegions {
            entities,
            solved_positions,
            chord_tolerance,
        } => {
            let tol = chord_tolerance.unwrap_or(waffle_types::regions::DEFAULT_CHORD_TOLERANCE);
            let regions = waffle_types::compute_regions(&entities, &solved_positions, tol);
            Ok(EngineToUi::RegionsComputed { regions })
        }

        UiToEngine::ExportStl => {
            // Whole model: merge all renderable bodies (a multi-body model would
            // otherwise lose every body but the last).
            match all_renderable_meshes_merged(state) {
                Some(mesh) => {
                    let bytes = crate::stl_export::render_mesh_to_stl(&mesh);
                    let stl_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    Ok(EngineToUi::StlExportReady { stl_data })
                }
                None => Err(BridgeError::NoMeshData),
            }
        }

        UiToEngine::MeasureBody { body_id } => measure_body(state, kb, &body_id),
        UiToEngine::ListFaces { body_id, filter } => {
            list_faces(state, kb, &body_id, filter.as_ref())
        }

        UiToEngine::ExportBodyStl { body_id } => {
            // Single body, identified by its persistent (feature_id, OutputKey).
            match find_body_mesh(state, &body_id) {
                Some(mesh) => {
                    let bytes = crate::stl_export::render_mesh_to_stl(&mesh);
                    let stl_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    Ok(EngineToUi::StlExportReady { stl_data })
                }
                None => Err(BridgeError::NoMeshData),
            }
        }

        UiToEngine::Tool {
            name,
            arguments,
            context,
        } => {
            let result = crate::tools::execute_tool(state, kb, &name, &arguments, context.as_ref());
            // A tool that can change the document carries the model update
            // with its answer, because nothing downstream would produce one:
            // `process_message` tessellates and attaches a preview only for a
            // `ModelUpdated`. The tool has already tessellated (it had to, to
            // report `bodies_added`), so the preview built here is of the
            // meshes the host is about to receive.
            //
            // Carried whenever the tool is a mutating one, answer or refusal:
            // a refusal can still have moved the document — a rollback that
            // did not restore it exactly leaves it changed, and that is
            // precisely the state a host must not miss.
            let model = crate::tools::mutates(&name).then(|| {
                let mut model = model_updated_response(state);
                attach_preview_mesh(state, &mut model);
                Box::new(model)
            });
            Ok(EngineToUi::ToolResult { result, model })
        }
    }
}

/// Open an `Assembly` tab and evaluate it (S2 C3b).
///
/// Everything the evaluation needs is in the session: the tab's own assembly,
/// and every Part / sub-assembly tree its instances reference (the active
/// tab's tree comes from the live engine, so an assembly always builds its
/// parts from what is on screen). Parts of linked `.waffle` sources still
/// resolve through the engine's source store.
fn open_assembly(
    state: &mut EngineState,
    tab_id: &str,
    kb: &mut dyn KernelBundle,
) -> Result<(), BridgeError> {
    state.active_sketch = None;
    state.selection.clear();
    state.hover = None;
    // Refuse a tab that holds no assembly BEFORE switching to it: a loud
    // error must not leave the session on a tab it could not open.
    let assembly = state.session.assembly(tab_id)?.clone();
    // The view being replaced (an `EditAssembly` re-evaluates the open tab on
    // every connector, mate or instance edit) hands its part engines to this
    // evaluation: only a part whose tree changed is rebuilt.
    let reuse = state.take_part_engines();
    state.session.switch_tab(tab_id, &mut state.engine)?;
    // The live tree is not the assembly's content; keep the renderer on the
    // instances only. (An `Assembly` tab holds no tree, so the switch already
    // left it empty.)
    state.engine.tree = feature_engine::types::FeatureTree::new();
    state.engine.rebuild_from_scratch(kb);
    let part_trees = state.session.part_trees(&state.engine);
    let assembly_trees = state.session.assembly_trees();
    let view = crate::assembly_view::evaluate(
        assembly,
        &part_trees,
        &assembly_trees,
        &state.engine.sources,
        kb,
        reuse,
    );
    // The solved placements are derived, but they are saved WITH the tab
    // (v4 §2.5) and the session composes the file now (S2 C3c) — so they go
    // back into the tab that was just evaluated. The store keeps its own copy
    // for the panel; this is the one that reaches storage.
    state
        .session
        .set_assembly_placements(tab_id, view.placements.clone());
    state.assembly = Some(view);
    Ok(())
}

/// Make `tab_id` the active tab and rebuild it (S2 C3a).
///
/// The session stashes the live tree and the undo history into the outgoing
/// tab and loads the incoming one's, so neither crosses a switch. Everything
/// cleared here belongs to the tab being left: a half-finished sketch, the
/// selection, the hover, an open assembly view, an edit context.
fn switch_to_tab(
    state: &mut EngineState,
    tab_id: &str,
    kb: &mut dyn KernelBundle,
) -> Result<(), BridgeError> {
    state.active_sketch = None;
    state.selection.clear();
    state.hover = None;
    // The assembly view and the edit context are left, not lost: their part
    // engines wait in the cache for the next assembly evaluation.
    state.stash_assembly_views();
    state.session.switch_tab(tab_id, &mut state.engine)?;
    state.engine.rebuild_from_scratch(kb);
    Ok(())
}

/// `ListFaces` (ICR-3): every face of a body as the viewport's `GeomRef` (the
/// shared `face_refs` builder) with its signature, filtered by the `TopoQuery`
/// filter rules and ordered by canonical ref JSON so the listing is
/// deterministic (I14).
fn list_faces(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
    filter: Option<&waffle_types::TopoQuery>,
) -> Result<EngineToUi, BridgeError> {
    crate::tessellation_runner::tessellate_engine(&mut state.engine, kb);
    let engine = &state.engine;
    let (feature_id, key, result, body) = engine
        .tree
        .features
        .iter()
        .find_map(|f| {
            let result = engine.feature_results.get(&f.id)?;
            result
                .outputs
                .iter()
                .find(|(key, _)| feature_engine::types::FeatureTree::body_id(f.id, key) == body_id)
                .map(|(key, body)| (f.id, key, result, body))
        })
        .ok_or_else(|| BridgeError::InvalidRequest {
            reason: format!("no live body {body_id}"),
        })?;
    let mesh = body.mesh.as_ref().ok_or(BridgeError::NoMeshData)?;
    let introspect = kb.as_introspect();
    let mut faces: Vec<ListedFace> = crate::face_refs::face_geom_refs(
        feature_id,
        key,
        mesh,
        &result.provenance.role_assignments,
        introspect,
        false,
    )
    .into_iter()
    .map(|(face, geom_ref)| ListedFace {
        geom_ref,
        signature: introspect.compute_signature(face, waffle_types::TopoKind::Face),
    })
    .filter(|f| {
        filter.is_none_or(|q| feature_engine::resolve::passes_all_filters(&f.signature, &q.filters))
    })
    .collect();
    faces.sort_by_cached_key(|f| serde_json::to_string(&f.geom_ref).unwrap_or_default());
    Ok(EngineToUi::FacesListed {
        body_id: body_id.to_string(),
        faces,
    })
}

/// A live body output by its persistent id (`FeatureTree::body_id`).
fn find_body<'a>(state: &'a EngineState, body_id: &str) -> Option<&'a modeling_ops::BodyOutput> {
    state.engine.tree.features.iter().find_map(|feature| {
        state
            .engine
            .feature_results
            .get(&feature.id)?
            .outputs
            .iter()
            .find(|(key, _)| {
                feature_engine::types::FeatureTree::body_id(feature.id, key) == body_id
            })
            .map(|(_, body)| body)
    })
}

/// `MeasureBody` (ICR-1): exact volume and area from the kernel when it can
/// integrate the B-Rep; otherwise the render mesh's, labelled `Mesh` with the
/// kernel's reason. Never an unlabelled approximation.
fn measure_body(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
) -> Result<EngineToUi, BridgeError> {
    // Natively nothing tessellates between messages (the WASM entry point
    // does); the pass only fills missing meshes, so it is cheap when present.
    crate::tessellation_runner::tessellate_engine(&mut state.engine, kb);
    let state = &*state;
    let body = find_body(state, body_id).ok_or_else(|| BridgeError::InvalidRequest {
        reason: format!("no live body {body_id}"),
    })?;
    let mesh = body.mesh.as_ref().ok_or(BridgeError::NoMeshData)?;
    let introspect = kb.as_introspect();

    let measured =
        |exact: Result<f64, waffle_types::kernel::KernelError>, from_mesh: f64| match exact {
            Ok(value) => Measured {
                value,
                method: MeasureMethod::Exact,
                exact_unavailable: None,
            },
            Err(e) => Measured {
                value: from_mesh,
                method: MeasureMethod::Mesh,
                exact_unavailable: Some(e.to_string()),
            },
        };
    let (mesh_volume, mesh_area) = mesh_volume_and_area(mesh);
    let volume_m3 = measured(introspect.solid_volume(&body.handle), mesh_volume);
    let surface_area_m2 = measured(introspect.solid_surface_area(&body.handle), mesh_area);

    let mut bbox_min = [f64::INFINITY; 3];
    let mut bbox_max = [f64::NEG_INFINITY; 3];
    for p in mesh.vertices.chunks_exact(3) {
        for axis in 0..3 {
            bbox_min[axis] = bbox_min[axis].min(p[axis] as f64);
            bbox_max[axis] = bbox_max[axis].max(p[axis] as f64);
        }
    }

    let edges = introspect.list_edges(&body.handle);
    let closed = !edges.is_empty() && edges.iter().all(|&e| introspect.edge_faces(e).len() == 2);
    Ok(EngineToUi::BodyMeasured {
        body_id: body_id.to_string(),
        volume_m3,
        surface_area_m2,
        bbox_min,
        bbox_max,
        face_count: introspect.list_faces(&body.handle).len(),
        edge_count: edges.len(),
        vertex_count: introspect.list_vertices(&body.handle).len(),
        closed,
    })
}

/// Signed volume and area of a triangle mesh (outward winding ⇒ positive).
fn mesh_volume_and_area(mesh: &RenderMesh) -> (f64, f64) {
    let p = |i: u32| {
        let i = i as usize * 3;
        [
            mesh.vertices[i] as f64,
            mesh.vertices[i + 1] as f64,
            mesh.vertices[i + 2] as f64,
        ]
    };
    let (mut volume, mut area) = (0.0, 0.0);
    for t in mesh.indices.chunks_exact(3) {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
        let (u, v) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    }
    (volume, area)
}

/// Find a single body's cached mesh by its persistent id
/// (`FeatureTree::body_id`). Meshes are tessellated for every output during
/// rebuild (`tessellate_missing_meshes`), so the cached mesh is present.
fn find_body_mesh(state: &EngineState, body_id: &str) -> Option<RenderMesh> {
    let tree = &state.engine.tree;
    for feature in &tree.features {
        if let Some(result) = state.engine.feature_results.get(&feature.id) {
            for (key, body) in &result.outputs {
                if feature_engine::types::FeatureTree::body_id(feature.id, key) == body_id {
                    return body.mesh.clone();
                }
            }
        }
    }
    None
}

/// Merge every renderable body's mesh (all mesh-bearing outputs of non-consumed
/// active features) into one mesh for a whole-model STL export.
fn all_renderable_meshes_merged(state: &EngineState) -> Option<RenderMesh> {
    let tree = &state.engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    let consumed = &state.engine.consumed_features;
    let mut out: Option<RenderMesh> = None;
    for feature in &tree.features[..limit] {
        if feature.suppressed || consumed.contains(&feature.id) {
            continue;
        }
        if let Some(result) = state.engine.feature_results.get(&feature.id) {
            for (_key, body) in &result.outputs {
                if let Some(mesh) = &body.mesh {
                    merge_render_mesh(out.get_or_insert_with(empty_render_mesh), mesh);
                }
            }
        }
    }
    out
}

fn empty_render_mesh() -> RenderMesh {
    RenderMesh {
        vertices: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
        face_ranges: Vec::new(),
    }
}

/// Append `src` onto `dst`, offsetting indices (STL has no per-body structure,
/// so face ranges are not needed for the merged export).
fn merge_render_mesh(dst: &mut RenderMesh, src: &RenderMesh) {
    let vbase = (dst.vertices.len() / 3) as u32;
    dst.vertices.extend_from_slice(&src.vertices);
    dst.normals.extend_from_slice(&src.normals);
    dst.indices.extend(src.indices.iter().map(|i| i + vbase));
}

/// Register a STEP source (already built by the caller: embedded or linked)
/// and add the ImportedBody feature that names it, with Import provenance.
fn add_import_feature(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entry: SourceEntry,
    file_name: &str,
    data: &str,
) -> Result<uuid::Uuid, BridgeError> {
    let source_id = entry.id;
    state.engine.sources.insert_text(source_id, data);
    state.sources.push(entry);
    let params = ImportedBodyParams::from_source(file_name, source_id);
    let op = Operation::ImportedBody { params };
    // Recorded inside the add's undo step (ICR-4): undoing the import must
    // not leave an orphan `Import` record in the file.
    let provenance = Provenance {
        origin: ProvenanceOrigin::Import { source_id },
        at: Some(chrono::Utc::now().to_rfc3339()),
    };
    let id = state.engine.add_feature_with_provenance(
        format!("Import {file_name}"),
        op,
        Some(provenance),
        kb,
    )?;
    Ok(id)
}

/// The `sources` table as the host sees it (`SourceStatus` rows).
/// Every source of the open document with its availability, as
/// `ModelUpdated.sources` carries them.
pub fn source_statuses(state: &EngineState) -> Vec<SourceStatus> {
    state
        .sources
        .iter()
        .map(|e| SourceStatus {
            id: e.id,
            name: e.name.clone(),
            kind: source_kind_tag(&e.kind),
            locator: e.locator.clone(),
            content_hash: e.content_hash.clone(),
            resolved: e.resolved.clone(),
            pack: e.effective_pack(),
            available: state.engine.sources.contains(e.id),
        })
        .collect()
}

/// The `type` tag of a source kind as written in the file.
fn source_kind_tag(kind: &SourceKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
        .unwrap_or_else(|| "?".to_string())
}

/// Build a ModelUpdated response from the current engine state.
/// The document's `sources` table as it should be written: for every entry
/// whose content the store holds, `embed` follows the entry's `pack` policy
/// (packed ⇒ the content, deflate-base64; linked ⇒ none) and the hash is
/// filled in. Entries the store has no content for are written as loaded.
fn sources_for_save(state: &EngineState) -> Vec<SourceEntry> {
    state
        .sources
        .iter()
        .cloned()
        .map(|mut entry| {
            if let Some(text) = state.engine.sources.text(entry.id) {
                if entry.effective_pack() {
                    entry.embed = Some(Embed::from_text(&text));
                } else {
                    entry.embed = None;
                }
                entry
                    .content_hash
                    .get_or_insert_with(|| git_blob_sha1(text.as_bytes()));
            }
            entry
        })
        .collect()
}

/// Verified save: refuse to emit a file the loader would reject (e.g. a
/// non-finite float serialized as `null`) — a loud save error beats a file
/// that saves silently and never opens again.
fn verified(state: &mut EngineState, doc: &WaffleDocument) -> Result<String, BridgeError> {
    // The same invariant-8 self-check, through the session's verifier so the
    // tabs that did not change are not re-parsed (they were verified when they
    // last changed, and their bytes are identical).
    state
        .save_verifier
        .save(doc)
        .map_err(|e| BridgeError::Serialization {
            reason: format!("refusing to save a corrupt document: {e}"),
        })
}

/// `ModelUpdated` naming the feature the command created or edited (ICR-4).
fn model_updated_for(state: &EngineState, id: uuid::Uuid) -> EngineToUi {
    let mut response = model_updated_response(state);
    if let EngineToUi::ModelUpdated { feature_id, .. } = &mut response {
        *feature_id = Some(id);
    }
    response
}

/// The document thumbnail: the last active mesh, decimated.
fn preview_mesh(state: &EngineState) -> Option<feature_engine::preview_mesh::PreviewMesh> {
    find_last_mesh(state).and_then(|mesh| {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return None;
        }
        let decimated = feature_engine::preview_mesh::decimate_mesh(
            &mesh.vertices,
            &mesh.normals,
            &mesh.indices,
            500, // max triangles for preview
        );
        if decimated.indices.is_empty() {
            None
        } else {
            Some(decimated)
        }
    })
}

/// Recompute a `ModelUpdated`'s preview once the post-dispatch tessellation
/// has meshed its bodies. Built inside `dispatch`, the preview is taken before
/// any new body has a mesh: after a load or full rebuild it came back `None`,
/// and the page then stored the whole last mesh as the thumbnail (a 180k
/// triangle, 10 MB preview in a 44-feature document).
pub fn attach_preview_mesh(state: &mut EngineState, response: &mut EngineToUi) {
    if let EngineToUi::ModelUpdated {
        preview_mesh: slot, ..
    } = response
    {
        let mesh = preview_mesh(state);
        // The thumbnail is saved WITH its tab (v4 §2.5), and the session is
        // what composes the file now (S2 C3c) — so every tab keeps the last
        // preview its own tree produced, instead of the store holding the only
        // copy on its tab list.
        let active = state.session.active_tab_id().to_string();
        state.session.set_preview_mesh(&active, mesh.clone());
        *slot = mesh;
    }
}

/// The session as `ModelUpdated` reports it (S2 C2): what a host needs to draw
/// a tab bar and name a document state. Never a tab's tree.
/// The open document as the session knows it (`DocumentInfo`), for any
/// host that needs it outside a `ModelUpdated` (the native host's
/// `document_info` tool, `specs/waffle_server_mode.md` §3.3).
pub fn document_info(state: &EngineState) -> DocumentInfo {
    let meta = state.session.document();
    DocumentInfo {
        id: meta.id,
        name: meta.name.clone(),
        display_unit: meta.display_unit.clone(),
        created: meta.created,
        tabs: state.session.tabs(),
        active_tab: state.session.active_tab_id().to_string(),
        // The open Assembly tab's tree, so the panel has a source once the
        // store stops holding tab content (S2 C4b). `assembly()` refuses a tab
        // of any other kind, which is exactly the "not an assembly" case.
        assembly_tree: state
            .session
            .assembly(state.session.active_tab_id())
            .ok()
            .cloned(),
        revision: state.session.revision(),
    }
}

/// The open assembly as evaluated, in the shape the UI and the assembly tools
/// read (`ModelUpdated.assembly`): solved placements, every connector's frame
/// and every rendered part's named connectors in WORLD coordinates. `None`
/// while no assembly is open.
pub fn assembly_status(state: &EngineState) -> Option<AssemblyStatus> {
    state.assembly.as_ref().map(|v| AssemblyStatus {
        placements: v.placements.clone(),
        errors: v.errors.clone(),
        warnings: v.warnings.clone(),
        parts: v.parts.iter().map(|(p, _)| p.clone()).collect(),
        connectors: v
            .tree
            .connectors
            .iter()
            .filter_map(|c| {
                // In WORLD coordinates: the frame is in its top-level
                // instance's space, which the placement puts in the world.
                let world = v
                    .frames
                    .get(&c.id)?
                    .transformed(&v.placement(c.top_instance_id()?));
                let (x_axis, y_axis, z_axis) = world.basis().ok()?;
                let derived = v.connector_geometry.get(&c.id).map(|k| k.label());
                Some(ConnectorFrameInfo {
                    id: c.id,
                    kind: match (c.part_connector, derived) {
                        (Some(_), Some(label)) => Some(format!("part connector · {label}")),
                        (Some(_), None) => Some("part connector".to_string()),
                        (None, label) => label.map(str::to_string),
                    },
                    origin: world.origin,
                    x_axis,
                    y_axis,
                    z_axis,
                })
            })
            .collect(),
        part_connectors: v
            .leaves
            .iter()
            .flat_map(|leaf| {
                v.parts[leaf.part]
                    .1
                    .connectors
                    .iter()
                    .filter_map(|pc| PartConnectorInfo::new(pc, leaf.path.clone(), &leaf.transform))
                    .collect::<Vec<_>>()
            })
            .collect(),
    })
}

fn model_updated_response(state: &EngineState) -> EngineToUi {
    let preview_mesh = preview_mesh(state);

    EngineToUi::ModelUpdated {
        feature_id: None,
        feature_tree: state.engine.tree.clone(),
        meshes: Vec::new(),
        edges: Vec::new(),
        errors: state.engine.errors.clone(),
        feature_errors: state.engine.feature_errors.clone(),
        warnings: state.engine.warnings.clone(),
        consumed_features: {
            let mut v: Vec<uuid::Uuid> = state.engine.consumed_features.iter().copied().collect();
            v.sort();
            v
        },
        preview_mesh,
        sources: source_statuses(state),
        assembly: assembly_status(state),
        context: state.context_view.as_ref().map(|cv| ContextStatus {
            assembly_tab_id: cv.assembly_tab_id.clone(),
            instance_path: cv.instance_path.clone(),
            instance_name: cv.view.leaf_name(&cv.instance_path),
            placement: cv.placement,
            instances: cv
                .ghosts
                .iter()
                .filter_map(|(li, _)| cv.view.leaves.get(*li))
                .map(|leaf| {
                    let part = &cv.view.parts[leaf.part].0;
                    ContextInstanceInfo {
                        path: leaf.path.clone(),
                        name: cv.view.leaf_name(&leaf.path),
                        part_tab_id: part.tab_id.clone(),
                        part_source_id: part.source_id,
                    }
                })
                .collect(),
            errors: cv.view.errors.clone(),
            warnings: cv.view.warnings.clone(),
        }),
        connectors: state
            .engine
            .connectors
            .iter()
            .filter_map(|pc| {
                PartConnectorInfo::new(
                    pc,
                    Vec::new(),
                    &feature_engine::assembly::Transform::identity(),
                )
            })
            .collect(),
        document: Some(document_info(state)),
    }
}

/// The bodies a whole-model STEP export writes, with what it leaves out.
///
/// A Part: every solid output (`Main` / `Body{}`) of every non-suppressed,
/// non-consumed feature up to the rollback bar, at identity. An open
/// assembly: the same for each rendered leaf's part engine, placed by the
/// leaf's solved world transform and named `instance / feature`. A
/// mesh-backed imported body has no analytic geometry to write and is
/// reported in `warnings` rather than faceted or silently dropped.
fn step_export_bodies(state: &EngineState) -> (Vec<StepExportBody>, Vec<String>) {
    let mut bodies = Vec::new();
    let mut warnings = Vec::new();
    match &state.assembly {
        Some(view) => {
            for leaf in &view.leaves {
                let (_, engine) = &view.parts[leaf.part];
                let prefix = view.leaf_name(&leaf.path);
                let placement = rigid_placement_of(&leaf.transform);
                collect_step_bodies(engine, &prefix, Some(placement), &mut bodies, &mut warnings);
            }
        }
        None => collect_step_bodies(&state.engine, "", None, &mut bodies, &mut warnings),
    }
    (bodies, warnings)
}

fn collect_step_bodies(
    engine: &feature_engine::Engine,
    prefix: &str,
    placement: Option<RigidPlacement>,
    out: &mut Vec<StepExportBody>,
    warnings: &mut Vec<String>,
) {
    let tree = &engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    for feature in &tree.features[..limit] {
        if feature.suppressed || engine.consumed_features.contains(&feature.id) {
            continue;
        }
        let Some(result) = engine.feature_results.get(&feature.id) else {
            continue;
        };
        let solids: Vec<(&OutputKey, &modeling_ops::BodyOutput)> = result
            .outputs
            .iter()
            .filter(|(k, _)| matches!(k, OutputKey::Main | OutputKey::Body { .. }))
            .map(|(k, b)| (k, b))
            .collect();
        if solids.is_empty() {
            continue;
        }
        let name = if prefix.is_empty() {
            feature.name.clone()
        } else {
            format!("{prefix} / {}", feature.name)
        };
        if matches!(feature.operation, Operation::ImportedBody { .. }) {
            warnings.push(format!(
                "`{name}` is a mesh-backed imported body and was not written \
                 (its own STEP text is the document's source)"
            ));
            continue;
        }
        for (key, body) in solids {
            let body_name = match key {
                OutputKey::Body { index } => format!("{name} / Body {index}"),
                _ => name.clone(),
            };
            out.push(StepExportBody {
                handle: body.handle.clone(),
                name: body_name,
                placement,
            });
        }
    }
}

/// An assembly placement as the kernel's rigid motion (rotation columns =
/// the transformed basis vectors).
fn rigid_placement_of(t: &feature_engine::assembly::Transform) -> RigidPlacement {
    let x = t.apply_dir([1.0, 0.0, 0.0]);
    let y = t.apply_dir([0.0, 1.0, 0.0]);
    let z = t.apply_dir([0.0, 0.0, 1.0]);
    RigidPlacement {
        translation: t.translation_m,
        rotation: [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]],
    }
}

/// Find the last active feature's mesh data by iterating features in reverse.
fn find_last_mesh(state: &EngineState) -> Option<RenderMesh> {
    let tree = &state.engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    for feature in tree.features[..limit].iter().rev() {
        if feature.suppressed {
            continue;
        }
        if let Some(result) = state.engine.feature_results.get(&feature.id) {
            for (key, body) in &result.outputs {
                if *key == OutputKey::Main {
                    if let Some(mesh) = &body.mesh {
                        return Some(mesh.clone());
                    }
                }
            }
        }
    }
    None
}

/// The entry function a script node calls when none is named
/// (`ScriptParams::entry`'s default).
pub(crate) const DEFAULT_SCRIPT_ENTRY: &str = "feature";

/// A built-in library script by name (A-M4 `AddScriptSource { library }`).
pub(crate) fn library_script(name: &str) -> Result<&'static str, BridgeError> {
    use feature_engine::script::library;
    match name {
        "gear" => Ok(library::GEAR_RHAI),
        "sprocket" => Ok(library::SPROCKET_RHAI),
        other => Err(BridgeError::InvalidRequest {
            reason: format!("no built-in script library `{other}` (gear, sprocket)"),
        }),
    }
}

/// A source's text from the store, or a loud refusal (`CheckScript`).
fn stored_script_text(state: &EngineState, id: uuid::Uuid) -> Result<String, BridgeError> {
    state
        .engine
        .sources
        .text(id)
        .ok_or_else(|| BridgeError::InvalidRequest {
            reason: format!("CheckScript: source {id} is not loaded (is it in the sources table?)"),
        })
}

/// The names of the built-in library scripts, for hosts and tool schemas.
pub(crate) const LIBRARY_SCRIPTS: &[&str] = &["gear", "sprocket"];

/// Check a script the way `CheckScript` answers (A-M4): header + compile +
/// entry, and — when `args` are given — a dry run against the recorder.
/// `args` carries the source id the run should name (any id works for a
/// dry run; the recorder never reads the store) with the argument map.
pub(crate) fn check_script(
    text: &str,
    entry: &str,
    args: Option<(
        Option<uuid::Uuid>,
        std::collections::BTreeMap<String, serde_json::Value>,
    )>,
) -> crate::messages::ScriptCheck {
    use crate::messages::{ScriptCheck, ScriptCheckError, ScriptDryRun};
    use feature_engine::types::{EngineError, ScriptParams};

    let typed = |e: EngineError| match e {
        EngineError::Script { stage, reason } => ScriptCheckError { stage, reason },
        other => ScriptCheckError {
            stage: "engine".to_string(),
            reason: other.to_string(),
        },
    };
    let interface = match feature_engine::script::check(text, entry) {
        Ok(iface) => iface,
        Err(e) => {
            return ScriptCheck {
                ok: false,
                interface: None,
                error: Some(typed(e)),
                dry_run: None,
            }
        }
    };
    let dry_run = args.map(|(source_id, args)| {
        let params = ScriptParams {
            source_id: source_id.unwrap_or_else(uuid::Uuid::nil),
            entry: entry.to_string(),
            args,
            arg_exprs: Default::default(),
            arg_values: Default::default(),
        };
        match feature_engine::script::record(text, &params) {
            Ok(rec) => ScriptDryRun {
                ok: true,
                error: None,
                children: rec.children.iter().map(|c| c.label.to_string()).collect(),
                logs: rec.logs,
                outputs: rec.outputs.into_iter().map(|(n, _)| n).collect(),
            },
            Err(e) => ScriptDryRun {
                ok: false,
                error: Some(typed(e)),
                children: Vec::new(),
                logs: Vec::new(),
                outputs: Vec::new(),
            },
        }
    });
    ScriptCheck {
        ok: true,
        interface: serde_json::to_value(&interface).ok(),
        error: None,
        dry_run,
    }
}

/// The display name a new feature takes: the operation's kind, or — for a
/// `Script` node — its script's declared `@feature name` when the source is
/// loaded and its header parses (A-M4: the tree shows "Spur gear", not
/// "Script").
pub(crate) fn feature_name_for(state: &EngineState, op: &Operation) -> String {
    if let Operation::Script { params } = op {
        if let Some(name) = state
            .engine
            .sources
            .text(params.source_id)
            .and_then(|text| feature_engine::script::display_name(&text))
        {
            return name;
        }
    }
    operation_name(op)
}

/// Derive a human-readable feature name from an operation.
fn operation_name(op: &Operation) -> String {
    match op {
        Operation::Sketch { .. } => "Sketch".to_string(),
        Operation::Sketch3d { .. } => "3D sketch".to_string(),
        Operation::Extrude { .. } => "Extrude".to_string(),
        Operation::Revolve { .. } => "Revolve".to_string(),
        Operation::Pipe { .. } => "Pipe".to_string(),
        Operation::Fillet { .. } => "Fillet".to_string(),
        Operation::Chamfer { .. } => "Chamfer".to_string(),
        Operation::Shell { .. } => "Shell".to_string(),
        Operation::BooleanCombine { .. } => "Boolean Combine".to_string(),
        Operation::UnionAll { .. } => "Union All".to_string(),
        Operation::DatumPlane { params } => params.name.clone(),
        Operation::ImportedBody { params } => format!("Import {}", params.file_name),
        Operation::MateConnector { params } if !params.name.trim().is_empty() => {
            params.name.trim().to_string()
        }
        Operation::MateConnector { .. } => "Mate connector".to_string(),
        Operation::PatternCircular { .. } => "Circular pattern".to_string(),
        Operation::PatternLinear { .. } => "Linear pattern".to_string(),
        Operation::PatternMirror { .. } => "Mirror".to_string(),
        Operation::Script { .. } => "Script".to_string(),
        Operation::Unknown(_) => op.type_tag().to_string(),
    }
}
