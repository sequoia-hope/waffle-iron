use base64::Engine as _;
use feature_engine::types::{ImportedBodyParams, Operation, Provenance, ProvenanceOrigin};
use file_format::{
    git_blob_sha1, Embed, ProjectMetadata, SourceEntry, SourceKind, TabKind, WaffleDocument,
};
use modeling_ops::KernelBundle;
use waffle_types::kernel::{KernelIntrospect, RenderMesh, RigidPlacement, StepExportBody};
use waffle_types::OutputKey;

use crate::engine_state::{BridgeError, EngineState};
use crate::messages::{
    AssemblyStatus, ConnectorFrameInfo, ContextInstanceInfo, ContextStatus, DocumentInfo,
    EngineToUi, PartConnectorInfo, SourceStatus, UiToEngine,
};
use crate::messages::{
    ContactEvidenceWire, InterferenceRegion, ListedFace, MeasureMethod, MeasureOperand, Measured,
    MeasuredGap, MeasuredInterference, MeasuredOn,
};
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

        UiToEngine::ImportKicad {
            file_name,
            data,
            board_step,
        } => {
            let entry = SourceEntry::embedded(file_name.clone(), SourceKind::KicadPcb, &data);
            let board_step = board_step.map(board_step_entry).transpose()?;
            link_kicad(state, kb, entry, &file_name, &data, board_step)
        }

        UiToEngine::LinkKicadFromLocator {
            file_name,
            locator,
            data,
            resolved_commit,
            board_step,
        } => {
            if !locator.is_shareable() {
                return Err(BridgeError::InvalidRequest {
                    reason: "LinkKicadFromLocator: a Local locator cannot be linked".to_string(),
                });
            }
            let mut entry = SourceEntry::linked(file_name.clone(), SourceKind::KicadPcb, locator);
            entry.set_content(&data);
            entry.fetched_at = Some(chrono::Utc::now());
            entry.resolved = resolved_commit.map(|c| file_format::Resolved {
                commit: c.to_ascii_lowercase(),
                at: chrono::Utc::now(),
            });
            let board_step = board_step.map(board_step_entry).transpose()?;
            link_kicad(state, kb, entry, &file_name, &data, board_step)
        }

        UiToEngine::QueryEntityMeta {
            body_id,
            instance_path,
        } => Ok(entity_meta(
            state,
            body_id.as_deref(),
            instance_path.as_deref(),
        )),

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

        // N1 (`specs/agent_mechanical_design.md` §5.2). The dotted body
        // segment is checked HERE, not by the caller: the body's display name
        // is the render layer's answer (a body with no override is named after
        // its producing feature plus an ordinal among that feature's rendered
        // bodies), so a sender could not check it even if it wanted to.
        UiToEngine::SetEntityName { name, named } => {
            let display = crate::entity_names::body_of_ref(&named.target)
                .and_then(|id| crate::entity_names::body_display_names(state).remove(&id));
            state
                .engine
                .set_entity_name(&name, *named, display.as_deref())?;
            Ok(model_updated_response(state))
        }

        UiToEngine::ClearEntityName { name } => {
            state.engine.clear_entity_name(&name)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::QueryEntityNames { body_id } => {
            crate::tessellation_runner::tessellate_engine(&mut state.engine, kb);
            Ok(EngineToUi::EntityNamesListed {
                names: crate::entity_names::list(state, kb, body_id.as_deref()),
            })
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
            // Linked boards' hover records are a pure function of the
            // source bytes and the tabs (spec §4 inv. 6): rebuilt here, from
            // whatever content the document embeds; a board whose content
            // is still to be fetched gets its record with `ProvideSource`.
            refresh_kicad_records(state);
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
            let kind = entry.kind.clone();
            let new_hash = git_blob_sha1(data.as_bytes());
            let changed = entry.content_hash.as_deref() != Some(new_hash.as_str());
            // A board that no longer reads is refused BEFORE the entry
            // changes: the document keeps the content it had (spec §6).
            let pcb = if kind == SourceKind::KicadPcb {
                Some(kicad_pcb::parse_kicad_pcb(&data).map_err(|e| {
                    BridgeError::InvalidRequest {
                        reason: format!("{}: {e}", entry.name),
                    }
                })?)
            } else {
                None
            };
            entry.content_hash = Some(new_hash);
            entry.fetched_at = Some(chrono::Utc::now());
            if let Some(commit) = resolved_commit {
                entry.resolved = Some(file_format::Resolved {
                    commit: commit.to_ascii_lowercase(),
                    at: chrono::Utc::now(),
                });
            }
            state.engine.sources.insert_text(source_id, &data);
            match pcb {
                // Re-sync (spec §3 R1–R6): the same bytes are a no-op for
                // the derived content (R6) — only the record is refreshed,
                // which is what a first fetch after an offline load needs.
                Some(pcb) if changed => resync_kicad(state, kb, source_id, &pcb)?,
                Some(_) => {
                    refresh_kicad_records(state);
                    state.engine.rebuild_from_scratch(kb);
                }
                None => {
                    state.engine.rebuild_from_scratch(kb);
                    // R5: a board STEP that changed re-syncs every board it
                    // models (the boards' records name their companion);
                    // a first fetch after an offline load only refreshes.
                    refresh_kicad_records(state);
                    let boards: Vec<uuid::Uuid> = state
                        .kicad_boards
                        .iter()
                        .filter(|r| r.board_step == Some(source_id))
                        .map(|r| r.source_id)
                        .collect();
                    if changed {
                        for board in boards {
                            let Some(text) = state.engine.sources.text(board) else {
                                continue;
                            };
                            let pcb = kicad_pcb::parse_kicad_pcb(&text).map_err(|e| {
                                BridgeError::InvalidRequest {
                                    reason: format!("board {board}: {e}"),
                                }
                            })?;
                            resync_kicad(state, kb, board, &pcb)?;
                        }
                    }
                }
            }
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
            let mut reuse = state.take_part_engines();
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
                &mut reuse,
            );
            state.park_unused_part_engines(reuse, &view.parts);
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
            let (bodies, warnings) = step_export_bodies(state, kb.as_introspect());
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

        UiToEngine::ExportDxf { view_dir, up } => {
            // The same body collection STEP export uses — every live body of
            // the part, or every rendered assembly instance's bodies at their
            // world placements — so one view of an assembly is the whole
            // assembly, and a mesh-backed body is left out with the same
            // named warning rather than silently.
            let (bodies, warnings) = step_export_bodies(state, kb.as_introspect());
            if bodies.is_empty() {
                return Err(BridgeError::NoMeshData);
            }
            let bodies: Vec<waffle_types::kernel::ProjectionBody> = bodies
                .into_iter()
                .map(|b| waffle_types::kernel::ProjectionBody {
                    handle: b.handle,
                    name: b.name,
                    placement: b.placement,
                })
                .collect();
            let frame = waffle_types::kernel::ViewFrame::from_parts(view_dir, up);
            let (dxf_data, declines) = kb
                .export_dxf_with_declines(
                    &bodies,
                    &frame,
                    &waffle_types::kernel::ProjectOpts::default(),
                )
                .map_err(|e| {
                    BridgeError::Engine(feature_engine::types::EngineError::RebuildFailed {
                        feature_name: "DXF export".to_string(),
                        reason: format!("{}", e),
                    })
                })?;
            // What the projection declined to decide, named and counted, on
            // the same channel as the dropped-body warnings. Every counter but
            // `cross_body` is a line the drawing does NOT carry, so a file
            // with a large count is a degenerate view rather than a clean one
            // — and without this the caller had no way to tell.
            let mut warnings = warnings;
            let declined: Vec<String> = declines
                .counts()
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(k, n)| format!("{k} {n}"))
                .collect();
            if !declined.is_empty() {
                warnings.push(format!(
                    "the projection declined to decide some of this view ({}); \
                     every one but cross_body means a line the drawing does not \
                     carry — see specs/drawings_and_mbd.md D1c",
                    declined.join(", ")
                ));
            }
            Ok(EngineToUi::DxfExportReady { dxf_data, warnings })
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
        UiToEngine::MeasureDistance { a, b, along } => measure_distance(state, kb, &a, &b, along),
        UiToEngine::MeasureInterference { a, b } => measure_interference(state, kb, &a, &b),
        UiToEngine::MeasureMass {
            body_id,
            density_kg_m3,
        } => measure_mass(state, kb, &body_id, density_kg_m3),
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
    let mut reuse = state.take_part_engines();
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
        &mut reuse,
    );
    // A part with no live instance this pass — a hidden one's — keeps its
    // engine, so un-hiding it is a solve rather than a rebuild.
    state.park_unused_part_engines(reuse, &view.parts);
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
    // Which face each N1 name points at, resolved once for the body rather
    // than per face (§5.2: a result that carries a `GeomRef` carries its
    // name). Taken before the borrow below, since resolving needs the kernel.
    let named = crate::entity_names::names_by_entity(state, kb, body_id);
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
    )
    .into_iter()
    .map(|(face, geom_ref)| ListedFace {
        geom_ref,
        signature: introspect.compute_signature(face, waffle_types::TopoKind::Face),
        name: named.get(&(waffle_types::TopoKind::Face, face)).cloned(),
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

/// `MeasureDistance` (Q1 of `specs/agent_mechanical_design.md` §4.2): the
/// minimum distance between two operands, or the gap along a direction.
///
/// The kernel answers; this handler only turns operands into kernel entities
/// and the answer into the wire shape. A `GeomRef` operand is resolved exactly
/// as a feature resolves one (`resolve_geom_ref_live`), so the face an agent
/// listed is the face it measures — and a reference that no longer identifies
/// one entity fails here, loudly, instead of measuring something else.
fn measure_distance(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    a: &MeasureOperand,
    b: &MeasureOperand,
    along: Option<[f64; 3]>,
) -> Result<EngineToUi, BridgeError> {
    use waffle_types::kernel::{DistanceOpts, MeasureEntity, Method};
    use waffle_types::TopoKind;

    let operand = |state: &EngineState,
                   introspect: &dyn KernelIntrospect,
                   op: &MeasureOperand|
     -> Result<MeasureEntity, BridgeError> {
        // An N1 name is measured as whatever it points at TODAY
        // (`specs/agent_mechanical_design.md` §5.2: every `EntityRef`
        // argument takes a name in place of a ref or a body id). An ENTITY
        // name goes through `names::resolve`, the same resolution
        // `names_list` reports — the stored persistent id first, the authored
        // fallback when that id is gone. Resolving the stored reference
        // directly instead would refuse a name the listing calls resolvable,
        // which is one question answered two ways. A body name becomes the
        // body operand, so a name reaches whichever of the two it was given
        // to.
        if let MeasureOperand::Name { name } = op {
            if let Some(named) = state.engine.tree.named_ref(name) {
                let resolved = feature_engine::names::resolve(
                    named,
                    &state.engine.feature_results,
                    introspect,
                )
                .map_err(|e| BridgeError::InvalidRequest {
                    reason: format!("the name \"{name}\" does not resolve: {e}"),
                })?;
                return match named.kind {
                    TopoKind::Face => Ok(MeasureEntity::Face(resolved.kernel_id)),
                    TopoKind::Edge => Ok(MeasureEntity::Edge(resolved.kernel_id)),
                    TopoKind::Vertex => Ok(MeasureEntity::Vertex(resolved.kernel_id)),
                    // A shell or solid name is a BODY, which the `body`
                    // operand names directly; refuse rather than guess.
                    other => Err(BridgeError::InvalidRequest {
                        reason: format!(
                            "\"{name}\" names a {other:?}, which is not a measurement operand; \
                             name the body with `body_id`"
                        ),
                    }),
                };
            }
        }
        let op = &match op {
            MeasureOperand::Name { name } => {
                match crate::entity_names::resolve_target(
                    state,
                    &crate::messages::EntityTarget::Name {
                        name: name.to_string(),
                    },
                )
                .map_err(|reason| BridgeError::InvalidRequest { reason })?
                {
                    crate::entity_names::Target::Entity(geom_ref) => {
                        MeasureOperand::Entity { geom_ref }
                    }
                    crate::entity_names::Target::Body(body_id) => MeasureOperand::Body { body_id },
                }
            }
            other => other.clone(),
        };
        match op {
            MeasureOperand::Name { .. } => unreachable!("a name was just replaced"),
            MeasureOperand::Point { point } => Ok(MeasureEntity::Point(*point)),
            MeasureOperand::Body { body_id } => {
                let body =
                    find_body(state, body_id).ok_or_else(|| BridgeError::InvalidRequest {
                        reason: format!("no live body {body_id}"),
                    })?;
                Ok(MeasureEntity::Solid(body.handle.clone()))
            }
            MeasureOperand::Entity { geom_ref } => {
                // Resolved under `Strict`, whatever the caller sent. The refs
                // `face_list` hands out are minted `BestEffort` for the
                // viewport, where a near miss is better than nothing and the
                // user sees the warning; an agent measuring a clearance sees
                // no warning, so a near miss would silently measure the wrong
                // face. §5.3 of `specs/agent_mechanical_design.md` makes this
                // the rule for every ref an agent authors through a tool.
                let mut geom_ref = geom_ref.clone();
                geom_ref.policy = waffle_types::ResolvePolicy::Strict;
                let resolved = feature_engine::resolve::resolve_geom_ref_live(
                    &geom_ref,
                    &state.engine.feature_results,
                    introspect,
                )
                .map_err(|e| BridgeError::InvalidRequest {
                    reason: format!("the reference does not resolve: {e}"),
                })?;
                Ok(match geom_ref.kind {
                    TopoKind::Face => MeasureEntity::Face(resolved.kernel_id),
                    TopoKind::Edge => MeasureEntity::Edge(resolved.kernel_id),
                    TopoKind::Vertex => MeasureEntity::Vertex(resolved.kernel_id),
                    // A shell or solid reference is a BODY, which the
                    // `body` operand names directly; refuse rather than
                    // guess which body a bare kind means.
                    other => {
                        return Err(BridgeError::InvalidRequest {
                            reason: format!(
                                "a {other:?} reference is not a measurement operand; \
                                 name the body with `body_id`"
                            ),
                        })
                    }
                })
            }
        }
    };

    let introspect = kb.as_introspect();
    let (ea, eb) = (
        operand(state, introspect, a)?,
        operand(state, introspect, b)?,
    );
    // A kernel capability wall stays a capability wall on the wire
    // (`NotImplemented`), not a bad request: an axis operand or a mesh-backed
    // imported body is a roadmap item, and a caller must not retry it with
    // different numbers.
    let d = kb
        .as_measure()
        .distance(&ea, &eb, &DistanceOpts { along })
        .map_err(|e| match e {
            waffle_types::kernel::KernelError::NotSupported { operation } => {
                BridgeError::NotImplemented { operation }
            }
            other => BridgeError::InvalidRequest {
                reason: other.to_string(),
            },
        })?;

    let on = |slot: Option<waffle_types::kernel::EntityRef>| {
        slot.map(|r| MeasuredOn {
            kind: r.kind,
            kernel_id: r.entity.0,
        })
    };
    let (method, chord_bound_m) = match d.method {
        Method::Exact => (MeasureMethod::Exact, 0.0),
        Method::Mesh { chord_bound } => (MeasureMethod::Mesh, chord_bound),
    };
    Ok(EngineToUi::DistanceMeasured {
        value_m: d.value,
        method,
        chord_bound_m,
        points: d.points,
        on: [on(d.on[0]), on(d.on[1])],
    })
}

/// Turn a kernel capability wall into `NotImplemented` and anything else into
/// a bad request — the Q1 rule, shared by Q2 and Q3: a roadmap item must not
/// look like something a caller can retry with different numbers.
fn measure_error(e: waffle_types::kernel::KernelError) -> BridgeError {
    match e {
        waffle_types::kernel::KernelError::NotSupported { operation } => {
            BridgeError::NotImplemented { operation }
        }
        other => BridgeError::InvalidRequest {
            reason: other.to_string(),
        },
    }
}

fn wire_gap(d: &waffle_types::kernel::Distance) -> MeasuredGap {
    use waffle_types::kernel::Method;
    let (method, chord_bound_m) = match d.method {
        Method::Exact => (MeasureMethod::Exact, 0.0),
        Method::Mesh { chord_bound } => (MeasureMethod::Mesh, chord_bound),
    };
    MeasuredGap {
        value_m: d.value,
        method,
        chord_bound_m,
        points: d.points,
        on: [0, 1].map(|i| {
            d.on[i].map(|r| MeasuredOn {
                kind: r.kind,
                kernel_id: r.entity.0,
            })
        }),
    }
}

/// `MeasureInterference` (Q2): whether two bodies collide, touch, or are
/// apart, from the kernel's own Intersect boolean.
///
/// A refused boolean comes back as a refusal. It is NEVER folded into
/// `disjoint`: "the kernel could not tell" and "they do not touch" are
/// different answers, and a clearance check that confused them would pass a
/// collision.
fn measure_interference(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    a: &str,
    b: &str,
) -> Result<EngineToUi, BridgeError> {
    use waffle_types::kernel::{Interference, Method};

    let handle = |id: &str| -> Result<waffle_types::kernel::KernelSolidHandle, BridgeError> {
        find_body(state, id)
            .map(|body| body.handle.clone())
            .ok_or_else(|| BridgeError::InvalidRequest {
                reason: format!("no live body {id}"),
            })
    };
    let (ha, hb) = (handle(a)?, handle(b)?);
    let answer = kb
        .as_measure()
        .interference(&ha, &hb)
        .map_err(measure_error)?;

    let result = match answer {
        Interference::Interferes {
            volume,
            bodies,
            method,
        } => {
            let (method, chord_bound_m) = match method {
                Method::Exact => (MeasureMethod::Exact, 0.0),
                Method::Mesh { chord_bound } => (MeasureMethod::Mesh, chord_bound),
            };
            MeasuredInterference::Interferes {
                volume_m3: volume,
                method,
                chord_bound_m,
                regions: bodies
                    .iter()
                    .map(|r| InterferenceRegion {
                        volume_m3: r.volume,
                        centroid: r.centroid,
                        aabb_min: r.aabb[0],
                        aabb_max: r.aabb[1],
                    })
                    .collect(),
            }
        }
        Interference::Contact { evidence, closest } => {
            use waffle_types::kernel::ContactEvidence;
            let (evidence, sliver_volume_m3) = match evidence {
                ContactEvidence::EmptyIntersectionAtZeroDistance => {
                    (ContactEvidenceWire::EmptyIntersectionAtZeroDistance, None)
                }
                ContactEvidence::SliverIntersection { volume } => {
                    (ContactEvidenceWire::SliverIntersection, Some(volume))
                }
            };
            MeasuredInterference::Contact {
                evidence,
                sliver_volume_m3,
                closest: wire_gap(&closest),
            }
        }
        Interference::Disjoint { distance } => MeasuredInterference::Disjoint {
            distance: wire_gap(&distance),
        },
    };
    Ok(EngineToUi::InterferenceMeasured {
        a: a.to_string(),
        b: b.to_string(),
        result,
    })
}

/// `MeasureMass` (Q3): volume, area, centroid and the inertia tensor about the
/// centroid, at the density the caller named (1 by default — see the message
/// docs).
fn measure_mass(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
    density_kg_m3: Option<f64>,
) -> Result<EngineToUi, BridgeError> {
    use waffle_types::kernel::Method;
    let handle = find_body(state, body_id)
        .map(|body| body.handle.clone())
        .ok_or_else(|| BridgeError::InvalidRequest {
            reason: format!("no live body {body_id}"),
        })?;
    let m = kb
        .as_measure()
        .mass_properties(&handle, density_kg_m3)
        .map_err(measure_error)?;
    let (method, chord_bound_m) = match m.method {
        Method::Exact => (MeasureMethod::Exact, 0.0),
        Method::Mesh { chord_bound } => (MeasureMethod::Mesh, chord_bound),
    };
    Ok(EngineToUi::MassMeasured {
        body_id: body_id.to_string(),
        volume_m3: m.volume,
        surface_area_m2: m.surface_area,
        centroid: m.centroid,
        inertia_at_centroid: m.inertia_at_centroid,
        principal_moments: m.principal_moments,
        principal_axes: m.principal_axes,
        density_kg_m3: m.density,
        mass_kg: m.mass,
        method,
        chord_bound_m,
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
/// `ImportKicad` / `LinkKicadFromLocator` (`specs/kicad_board_link.md`
/// §2.4, C2): read the board, register the source, derive the Board Part,
/// the placeholder Parts and the assembly into NEW tabs, remember the
/// metadata, and open the Board tab (which rebuilds it on the real kernel,
/// so the reply carries the board's own errors and warnings).
fn link_kicad(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    entry: SourceEntry,
    file_name: &str,
    data: &str,
    board_step: Option<(SourceEntry, String)>,
) -> Result<EngineToUi, BridgeError> {
    use feature_engine::kicad::{derive_board, DeriveOptions};

    // A refused file lands nothing — the source entry is not added either
    // (spec §6): there is nothing to hang it on.
    let pcb = kicad_pcb::parse_kicad_pcb(data).map_err(|e| BridgeError::InvalidRequest {
        reason: format!("{file_name}: {e}"),
    })?;
    let source_id = entry.id;

    // The board STEP (spec §3 C2–C4): a `Step` source whose products are
    // the component models. One that does not read still lands as a source
    // (its warning names why) and every component gets a placeholder.
    let mut step_warnings = Vec::new();
    let models = board_step.map(|(step_entry, text)| {
        let step_id = step_entry.id;
        let name = step_entry.name.clone();
        state.engine.sources.insert_text(step_id, &text);
        state.sources.push(step_entry);
        board_step_models(state, step_id, &name, &pcb, &mut step_warnings)
    });

    let derived = derive_board(source_id, &pcb, DeriveOptions::default(), models.flatten());

    state.engine.sources.insert_text(source_id, data);
    state.sources.push(entry);

    let stem = file_name
        .strip_suffix(".kicad_pcb")
        .unwrap_or(file_name)
        .to_string();
    let board_tab = state.session.add_tab("Part", Some(stem.clone()))?;
    state
        .session
        .set_features(&board_tab, derived.board_tree.clone())?;

    let mut placeholder_tabs = std::collections::BTreeMap::new();
    for p in &derived.placeholders {
        let short = p.footprint.rsplit(':').next().unwrap_or(&p.footprint);
        let tab = state
            .session
            .add_tab("Part", Some(format!("{short} placeholder")))?;
        state.session.set_features(&tab, p.tree.clone())?;
        placeholder_tabs.insert(p.footprint.clone(), tab);
    }

    let assembly_tab = state
        .session
        .add_tab("Assembly", Some(format!("{stem} assembly")))?;
    let (assembly, _, assembly_warnings) = derived.assembly(&board_tab, &placeholder_tabs);
    state.session.set_assembly(&assembly_tab, assembly)?;

    switch_to_tab(state, &board_tab, kb)?;
    // The hover record is read back from the tabs, the same way a reload
    // rebuilds it (spec §4 inv. 6) — one path, exercised from the start.
    refresh_kicad_records(state);
    // The rebuild replaced the engine's warnings with the board's own; the
    // reader's and the derivation's come after them.
    state
        .engine
        .warnings
        .extend(derived.warnings.iter().cloned());
    state.engine.warnings.extend(step_warnings);
    state.engine.warnings.extend(assembly_warnings);
    Ok(model_updated_response(state))
}

/// The `Step` source entry for a board STEP handed over with a board
/// (`BoardStepData`): linked when it came with a shareable locator,
/// embedded otherwise. A `Local` locator is refused, as for the board.
fn board_step_entry(
    bs: crate::messages::BoardStepData,
) -> Result<(SourceEntry, String), BridgeError> {
    let entry = match bs.locator {
        Some(locator) => {
            if !locator.is_shareable() {
                return Err(BridgeError::InvalidRequest {
                    reason: "board_step: a Local locator cannot be linked".to_string(),
                });
            }
            let mut entry = SourceEntry::linked(bs.file_name.clone(), SourceKind::Step, locator);
            entry.set_content(&bs.data);
            entry.fetched_at = Some(chrono::Utc::now());
            entry.resolved = bs.resolved_commit.map(|c| file_format::Resolved {
                commit: c.to_ascii_lowercase(),
                at: chrono::Utc::now(),
            });
            entry
        }
        None => SourceEntry::embedded(bs.file_name.clone(), SourceKind::Step, &bs.data),
    };
    Ok((entry, bs.data))
}

/// The component models of a board STEP the store holds (spec §3 C2–C4),
/// or `None` with a `BoardStepUnusable` warning when it does not read.
fn board_step_models(
    state: &EngineState,
    step_id: uuid::Uuid,
    file_name: &str,
    pcb: &kicad_pcb::Pcb,
    warnings: &mut Vec<String>,
) -> Option<feature_engine::kicad::BoardStepModels> {
    let text = state.engine.sources.text(step_id)?;
    match step_import::parse_step_products_cached(&text, file_name) {
        Ok(products) => Some(feature_engine::kicad::BoardStepModels::from_products(
            step_id, file_name, &products, pcb,
        )),
        Err(e) => {
            warnings.push(format!(
                "BoardStepUnusable: {file_name} could not be read ({e}); every component is a \
                 placeholder"
            ));
            None
        }
    }
}

/// The tabs a `KicadPcb` source derived, found from the tabs themselves
/// (the trees' and instances' `x-derived` records), not from a persisted
/// record (spec §4 inv. 6).
#[derive(Debug, Default)]
struct KicadTabs {
    board: Option<String>,
    /// Footprint name → placeholder Part tab.
    placeholders: std::collections::BTreeMap<String, String>,
    /// The assembly tab and its grounded board instance.
    assembly: Option<(String, uuid::Uuid)>,
}

fn kicad_tabs(
    source_id: uuid::Uuid,
    part_trees: &std::collections::HashMap<String, feature_engine::types::FeatureTree>,
    assembly_trees: &std::collections::HashMap<String, feature_engine::assembly::AssemblyTree>,
) -> KicadTabs {
    use feature_engine::kicad::{
        derived_of, RULE_BOARD_INSTANCE, RULE_BOARD_PART, RULE_PLACEHOLDER,
    };
    let mut tabs = KicadTabs::default();
    // Sorted so two tabs claiming one role resolve deterministically.
    let mut parts: Vec<(&String, &feature_engine::types::FeatureTree)> =
        part_trees.iter().collect();
    parts.sort_by(|a, b| a.0.cmp(b.0));
    for (tab, tree) in parts {
        match derived_of(&tree.extra) {
            Some((s, RULE_BOARD_PART, _)) if s == source_id => {
                tabs.board.get_or_insert_with(|| tab.clone());
            }
            Some((s, RULE_PLACEHOLDER, footprint)) if s == source_id => {
                tabs.placeholders
                    .entry(footprint.to_string())
                    .or_insert_with(|| tab.clone());
            }
            _ => {}
        }
    }
    let mut assemblies: Vec<(&String, &feature_engine::assembly::AssemblyTree)> =
        assembly_trees.iter().collect();
    assemblies.sort_by(|a, b| a.0.cmp(b.0));
    for (tab, tree) in assemblies {
        let board = tree.instances.iter().find(
            |i| matches!(derived_of(&i.extra), Some((s, RULE_BOARD_INSTANCE, _)) if s == source_id),
        );
        if let Some(board) = board {
            tabs.assembly.get_or_insert((tab.clone(), board.id));
        }
    }
    tabs
}

/// Rebuild every linked board's hover record from the sources and the
/// tabs (spec §4 inv. 6): a pure function of the `.kicad_pcb` bytes and
/// the derived tabs, so a reload — offline, from the embed — answers
/// `QueryEntityMeta` exactly as the linking session did. A `KicadPcb`
/// source whose content is not in the store yet has no record until
/// `ProvideSource` brings it.
pub(crate) fn refresh_kicad_records(state: &mut EngineState) {
    use feature_engine::kicad::{board_meta, component_meta, derived_of, RULE_FOOTPRINT};
    let part_trees = state.session.part_trees(&state.engine);
    let assembly_trees = state.session.assembly_trees();
    let mut records = Vec::new();
    for entry in state
        .sources
        .iter()
        .filter(|s| s.kind == SourceKind::KicadPcb)
    {
        let Some(text) = state.engine.sources.text(entry.id) else {
            continue;
        };
        let Ok(pcb) = kicad_pcb::parse_kicad_pcb(&text) else {
            continue;
        };
        let tabs = kicad_tabs(entry.id, &part_trees, &assembly_trees);
        let (Some(board_tab), Some((assembly_tab, board_instance))) = (tabs.board, tabs.assembly)
        else {
            continue;
        };
        let mut components = std::collections::BTreeMap::new();
        for inst in &assembly_trees[&assembly_tab].instances {
            let Some((s, RULE_FOOTPRINT, uuid)) = derived_of(&inst.extra) else {
                continue;
            };
            if s != entry.id {
                continue;
            }
            if let Some(fp) = pcb.footprints.iter().find(|f| f.uuid == uuid) {
                components.insert(inst.id, component_meta(fp));
            }
        }
        let board_step = feature_engine::kicad::board_step_of(&part_trees[&board_tab].extra);
        records.push(crate::engine_state::KicadBoardRecord {
            source_id: entry.id,
            board_tab,
            assembly_tab,
            board_instance,
            placeholder_tabs: tabs.placeholders,
            board_step,
            board: board_meta(entry.id, &pcb),
            components,
        });
    }
    state.kicad_boards = records;
}

/// Re-sync a linked board (`specs/kicad_board_link.md` §3 R1–R5, C5): the
/// source's bytes changed, so every feature, instance and connector derived
/// from it is regenerated by rule and reconciled by id (features: rule +
/// ordinal; instances and connectors: footprint uuid), while everything the
/// user authored on top stays untouched. Whole-rule replacement, never a
/// patch (§4 inv. 5).
fn resync_kicad(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    source_id: uuid::Uuid,
    pcb: &kicad_pcb::Pcb,
) -> Result<(), BridgeError> {
    use feature_engine::kicad::{derive_board, resync_assembly, resync_features, DeriveOptions};

    // The session's copies are what gets edited; the active tab's tree
    // lives in the engine until stashed.
    state.session.stash_active(&mut state.engine);
    let part_trees = state.session.part_trees(&state.engine);
    let assembly_trees = state.session.assembly_trees();
    let tabs = kicad_tabs(source_id, &part_trees, &assembly_trees);
    let Some(board_tab) = tabs.board else {
        return Err(BridgeError::InvalidRequest {
            reason: format!(
                "the board Part derived from source {source_id} is no longer in the document; \
                 link the board again"
            ),
        });
    };

    // R5: the companion STEP the board tree names, as the store holds it
    // now (a `ProvideSource` on the STEP itself lands here too).
    let mut warnings = Vec::new();
    let models =
        feature_engine::kicad::board_step_of(&part_trees[&board_tab].extra).and_then(|step_id| {
            let name = state
                .sources
                .iter()
                .find(|s| s.id == step_id)
                .map(|s| s.name.clone())?;
            board_step_models(state, step_id, &name, pcb, &mut warnings)
        });
    let derived = derive_board(source_id, pcb, DeriveOptions::default(), models);
    warnings.extend(derived.warnings.iter().cloned());

    // R1: the board, in place.
    let board = resync_features(&part_trees[&board_tab], &derived.board_tree, source_id);
    state.session.set_features(&board_tab, board)?;

    // Placeholders: a shape still in use is regenerated in its tab, a new
    // shape gets a tab, a shape no longer used loses its tab unless the
    // user built on it.
    let mut placeholder_tabs = std::collections::BTreeMap::new();
    for p in &derived.placeholders {
        let tab = match tabs.placeholders.get(&p.footprint) {
            Some(tab) => {
                let tree = resync_features(&part_trees[tab], &p.tree, source_id);
                state.session.set_features(tab, tree)?;
                tab.clone()
            }
            None => {
                let short = p.footprint.rsplit(':').next().unwrap_or(&p.footprint);
                let tab = state
                    .session
                    .add_tab("Part", Some(format!("{short} placeholder")))?;
                state.session.set_features(&tab, p.tree.clone())?;
                tab
            }
        };
        placeholder_tabs.insert(p.footprint.clone(), tab);
    }
    for (footprint, tab) in &tabs.placeholders {
        if placeholder_tabs.contains_key(footprint) {
            continue;
        }
        let only_derived = part_trees[tab].features.iter().all(|f| {
            matches!(
                part_trees[tab].provenance.get(&f.id).map(|p| &p.origin),
                Some(feature_engine::types::ProvenanceOrigin::Derived { source_id: s, .. }) if *s == source_id
            )
        });
        if only_derived {
            // A closed active tab names its successor; the re-open below
            // reads the session's active tab, so nothing else to do.
            state.session.close_tab(tab)?;
        } else {
            warnings.push(format!(
                "placeholder tab for {footprint} is no longer used by the board but holds your \
                 own features, so it was kept"
            ));
        }
    }

    // R2–R4: the assembly, reconciled by footprint uuid.
    match tabs.assembly {
        Some((assembly_tab, _)) => {
            let (fresh, _, assembly_warnings) = derived.assembly(&board_tab, &placeholder_tabs);
            warnings.extend(assembly_warnings);
            let (reconciled, _, dangling) =
                resync_assembly(&assembly_trees[&assembly_tab], fresh, source_id);
            for d in dangling {
                warnings.push(format!(
                    "MateTargetGone: mate `{}` ({}) references instance `{}` (footprint {}) which \
                     the board no longer has; the mate was left in place",
                    d.mate_name, d.mate_id, d.instance_name, d.footprint_uuid
                ));
            }
            state.session.set_assembly(&assembly_tab, reconciled)?;
        }
        None => warnings.push(format!(
            "the assembly tab derived from source {source_id} is no longer in the document; \
             only the board was updated"
        )),
    }
    // Re-open whatever tab is active so the screen shows the new content:
    // an assembly is re-evaluated, a part's tree is loaded and rebuilt.
    let active = state.session.active_tab_id().to_string();
    if state.session.assembly(&active).is_ok() {
        open_assembly(state, &active, kb)?;
    } else {
        state.stash_assembly_views();
        state.engine.tree = state
            .session
            .tab(&active)
            .and_then(|t| t.features().cloned())
            .unwrap_or_default();
        state.engine.rebuild_from_scratch(kb);
    }
    state.engine.warnings.extend(warnings);
    refresh_kicad_records(state);
    Ok(())
}

/// `QueryEntityMeta` (`specs/kicad_board_link.md` C4): the KiCad record
/// behind a body or an instance. A body id in an assembly starts with the
/// instance path, so its first segment is the top-level instance; a live
/// part body belongs to the open tab, which is a board when a record says
/// so. Anything else answers all-`None`.
fn entity_meta(
    state: &EngineState,
    body_id: Option<&str>,
    instance_path: Option<&[uuid::Uuid]>,
) -> EngineToUi {
    let top_instance: Option<uuid::Uuid> =
        instance_path.and_then(|p| p.first().copied()).or_else(|| {
            let id = body_id?;
            let segments: Vec<&str> = id.split('/').collect();
            // `{instance…}/{feature}/{key}`: three or more segments.
            if segments.len() >= 3 {
                segments[0].parse().ok()
            } else {
                None
            }
        });
    let source_of = |source_id: uuid::Uuid| {
        source_statuses(state)
            .into_iter()
            .find(|s| s.id == source_id)
    };
    let none = EngineToUi::EntityMeta {
        board: None,
        component: None,
        source: None,
    };
    match top_instance {
        Some(inst) => {
            for rec in &state.kicad_boards {
                if let Some(component) = rec.components.get(&inst) {
                    return EngineToUi::EntityMeta {
                        board: Some(rec.board.clone()),
                        component: Some(component.clone()),
                        source: source_of(rec.source_id),
                    };
                }
                if rec.board_instance == inst {
                    return EngineToUi::EntityMeta {
                        board: Some(rec.board.clone()),
                        component: None,
                        source: source_of(rec.source_id),
                    };
                }
            }
            none
        }
        None => {
            // A live-part body: the open tab decides.
            if body_id.is_none() {
                return none;
            }
            let active = state.session.active_tab_id();
            match state.kicad_boards.iter().find(|r| r.board_tab == active) {
                Some(rec) => EngineToUi::EntityMeta {
                    board: Some(rec.board.clone()),
                    component: None,
                    source: source_of(rec.source_id),
                },
                None => none,
            }
        }
    }
}

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
/// reported in `warnings` rather than faceted or silently dropped — asked of
/// the kernel per body (`solid_is_exact`), because since SI5 C6 an imported
/// feature's body is exact whenever the file's shell could be carried
/// exactly, and such a body IS written.
fn step_export_bodies(
    state: &EngineState,
    kernel: &dyn KernelIntrospect,
) -> (Vec<StepExportBody>, Vec<String>) {
    let mut bodies = Vec::new();
    let mut warnings = Vec::new();
    match &state.assembly {
        Some(view) => {
            for leaf in &view.leaves {
                let (_, engine) = &view.parts[leaf.part];
                let prefix = view.leaf_name(&leaf.path);
                let placement = rigid_placement_of(&leaf.transform);
                collect_step_bodies(
                    engine,
                    kernel,
                    &prefix,
                    Some(placement),
                    &mut bodies,
                    &mut warnings,
                );
            }
        }
        None => collect_step_bodies(&state.engine, kernel, "", None, &mut bodies, &mut warnings),
    }
    (bodies, warnings)
}

fn collect_step_bodies(
    engine: &feature_engine::Engine,
    kernel: &dyn KernelIntrospect,
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
        for (key, body) in solids {
            let body_name = match key {
                OutputKey::Body { index } => format!("{name} / Body {index}"),
                _ => name.clone(),
            };
            if !kernel.solid_is_exact(&body.handle) {
                warnings.push(format!(
                    "`{body_name}` is a mesh-backed imported body and was not written \
                     (its own STEP text is the document's source)"
                ));
                continue;
            }
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
        Operation::Sweep { .. } => "Sweep".to_string(),
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
