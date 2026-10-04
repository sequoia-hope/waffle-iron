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

/// The placeholder plane anchor a LOCAL sketch has always carried: a fresh
/// datum uuid that resolves to nothing, because the sketch's real frame travels
/// in `plane_origin` / `plane_normal`. Kept verbatim when N2 moved the face
/// identity into `Sketch::plane_face`, so no existing behaviour that reads
/// `Sketch::plane` moved (the share-a-face target search is the one that
/// matters).
fn placeholder_sketch_plane() -> waffle_types::GeomRef {
    waffle_types::GeomRef {
        kind: waffle_types::TopoKind::Face,
        anchor: waffle_types::Anchor::Datum {
            datum_id: uuid::Uuid::new_v4(),
        },
        selector: waffle_types::Selector::Role {
            role: waffle_types::Role::EndCapPositive,
            index: 0,
        },
        policy: waffle_types::ResolvePolicy::BestEffort,
        scope: None,
    }
}

/// Pin the LOCAL model face a `BeginSketch` names, so every later rebuild can
/// prove it is still there (N2 §5.3 item 3).
///
/// `None`, and the sketch keeps its cached frame with nothing to re-resolve,
/// when the plane is not a local model face at all — a datum anchor, or a face
/// scoped into another instance (which `feature_engine::context` re-derives
/// from the open assembly context instead). Also `None` when the kernel cannot
/// pin it: an unresolvable reference here must not stop the user from sketching
/// — `BeginSketch` only opens the editor, and the frame the sketch commits with
/// comes from `FinishSketch`. The cost of that choice is that such a sketch has
/// no identity to re-resolve, which is exactly where it was before N2.
/// Is `plane` a face of a body in THIS tab — the one case N2 pins and the one
/// case `Sketch::plane` must keep its placeholder for? A face scoped into
/// another instance is not local (`feature_engine::context` re-derives the
/// plane from it, and `Sketch::plane` is where it travels); nor is a datum.
fn is_local_model_face(plane: &waffle_types::GeomRef) -> bool {
    plane.scope.is_none()
        && plane.kind == waffle_types::TopoKind::Face
        && matches!(plane.anchor, waffle_types::Anchor::FeatureOutput { .. })
}

fn pin_sketch_plane_face(
    state: &EngineState,
    kb: &mut dyn KernelBundle,
    plane: &waffle_types::GeomRef,
) -> Option<waffle_types::SketchFaceRef> {
    if !is_local_model_face(plane) {
        return None;
    }
    // STRICT whatever the pick carried, unlike a name (N2 §5.3). A name's
    // policy is the author's: an agent's refuses when the identity is gone, a
    // user's rebinds by geometry and warns, because a person can look at what
    // it bound. §5.3 item 3 gives a sketch no such choice — "fail loudly on a
    // non-planar or missing face" — and the reason is the geometry, not the
    // audience: a sketch that rebinds to whichever face scores best moves
    // every point of itself and every feature below it, and the plate's top
    // face is always sitting right there to be found. The face a user picked
    // in the viewport is pinned exactly as hard as one an agent named.
    let mut authored = plane.clone();
    authored.policy = waffle_types::ResolvePolicy::Strict;
    let introspect = kb.as_introspect();
    let pinned =
        feature_engine::resolve::pin_identity(&authored, &state.engine.feature_results, introspect)
            .ok()?;
    let signature = introspect.compute_signature(pinned.kernel_id, waffle_types::TopoKind::Face);
    Some(waffle_types::SketchFaceRef {
        target: pinned.target,
        fallback: pinned.fallback,
        signature,
    })
}

fn handle_message(
    state: &mut EngineState,
    msg: UiToEngine,
    kb: &mut dyn KernelBundle,
) -> Result<EngineToUi, BridgeError> {
    match msg {
        // -- Sketch operations --
        UiToEngine::BeginSketch { plane } => {
            let face = pin_sketch_plane_face(state, kb, &plane);
            // A local face ref becomes the pinned identity, and the `plane`
            // field keeps the placeholder a local sketch has always carried —
            // see `EngineState::begin_sketch`.
            //
            // The placeholder stands in for EVERY local model face, not only
            // the ones that pinned: `pin_sketch_plane_face` returns `None`
            // whenever the kernel cannot pin (a mesh-backed import whose
            // signature scores below the floor, a feature with no result yet),
            // and letting the real reference through there would put a
            // `FeatureOutput` anchor in `Sketch::plane` for a local sketch.
            // `rebuild`'s share-a-face target search branches on that anchor,
            // so it would silently change which body an extrude on this sketch
            // merges into — the one thing this increment promised not to move.
            let plane = if is_local_model_face(&plane) {
                placeholder_sketch_plane()
            } else {
                plane
            };
            state.begin_sketch(plane, face);
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
                active.solve_report = solved.report.clone();
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
                // Same for a drawing: it is rebuilt by `OpenDrawing`, which
                // projects the SOURCE tabs' bodies. A drawing tab holds no
                // tree of its own.
                TabKind::Drawing { .. } => feature_engine::types::FeatureTree::new(),
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
            state.drawing = None;
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

        UiToEngine::OpenDrawing { tab_id } => {
            open_drawing(state, &tab_id, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::DrawingEdit { tab_id, edit } => {
            // The page's edit path. The whole drawing never goes out and
            // comes back, so an annotation's `u64` pids cannot be rounded by
            // a JSON round trip through JavaScript (see `EditDrawing`).
            let mut drawing = state.session.drawing(&tab_id)?.clone();
            crate::drawing_view::apply_edit(&mut drawing, &edit)
                .map_err(|reason| BridgeError::InvalidRequest { reason })?;
            state.session.set_drawing(&tab_id, drawing)?;
            if state.session.active_tab_id() == tab_id {
                open_drawing(state, &tab_id, kb)?;
            }
            Ok(model_updated_response(state))
        }

        UiToEngine::EditDrawing { tab_id, drawing } => {
            state.session.set_drawing(&tab_id, drawing)?;
            // Re-evaluate only what is on screen, exactly as `EditAssembly`
            // does: an edit to a background drawing tab is recorded, and
            // opening that tab shows it.
            if state.session.active_tab_id() == tab_id {
                open_drawing(state, &tab_id, kb)?;
            }
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
        UiToEngine::SetParameters {
            parameters,
            renames,
        } => {
            state.engine.set_parameters(parameters, &renames, kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::EvaluateExpression {
            expression,
            dimension,
        } => {
            let env = feature_engine::params::cached_env(&state.engine.tree.parameters);
            // D2: a measurement function reads the LIVE model, through the
            // same measurer the rebuild uses, so a preview and the rebuilt
            // geometry cannot disagree about what `distance(a, b)` is.
            //
            // No ordering floor: a preview drives no field, so it has no
            // position in the tree and nothing to be circular with respect
            // to. It answers "what does this measure right now", which is
            // the question asked. The rule applies when the expression is
            // STORED on a field, where the rebuild positions it.
            let measurer = feature_engine::measure::TreeMeasurer::new(
                &state.engine.tree,
                &state.engine.feature_results,
                kb.as_introspect(),
                kb.as_measure(),
                &state.engine.pid_to_feature,
            );
            // The preview reports the WORKING-SPACE magnitude (mm for a
            // length, degrees for an angle) — what the field's own boundary
            // will convert — plus the dimension the expression produced. A
            // caller that named a dimension gets the field's refusal here.
            let evaluated = feature_engine::expr::evaluate_measured(&expression, &env, &measurer)
                .and_then(|q| match dimension {
                    Some(want) => q.check(want).map(|()| q),
                    None => Ok(q),
                });
            match evaluated {
                Ok(q) => Ok(EngineToUi::ExpressionEvaluated {
                    value: Some(q.value),
                    dimension: Some(q.dimension_label()),
                    error: None,
                }),
                Err(e) => Ok(EngineToUi::ExpressionEvaluated {
                    value: None,
                    dimension: None,
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

        UiToEngine::ExportDxf {
            view_dir,
            up,
            sheet_id,
            view_id,
        } => {
            // A Drawing tab exports its SHEET; anything else exports one view
            // of the whole model (§12's flat pattern). The arguments of each
            // shape are refused on the other rather than ignored, because
            // each names a projection the other does not have.
            if state.drawing.is_some() {
                if view_dir.is_some() || up.is_some() {
                    return Err(BridgeError::InvalidRequest {
                        reason: "a Drawing tab exports the views on its sheet; `direction` and \
                                 `up` describe a view of the model and have no meaning here \
                                 (switch to the Part tab for a flat pattern)"
                            .to_string(),
                    });
                }
                return export_sheet_dxf(state, kb, sheet_id, view_id);
            }
            if sheet_id.is_some() || view_id.is_some() {
                return Err(BridgeError::InvalidRequest {
                    reason: "`sheet_id` and `view_id` name views of a Drawing tab; the open tab \
                             is not one"
                        .to_string(),
                });
            }
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
            let mut warnings = warnings;
            warnings.extend(decline_warning(&declines));
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

        UiToEngine::ApplySketchOps { live, ops, next_id } => {
            let applied =
                sketch_solver::ops::apply_ops(&live.to_sketch(), &ops, next_id).map_err(|e| {
                    BridgeError::InvalidRequest {
                        reason: e.to_string(),
                    }
                })?;
            Ok(EngineToUi::SketchOpsApplied {
                entities: applied.sketch.entities,
                constraints: applied.sketch.constraints,
                projected: applied.sketch.projected,
                edit: applied.edit,
                transient_constraints: applied.transient_constraints,
                next_id: applied.next_id,
            })
        }

        UiToEngine::QuerySketch { live, query } => Ok(EngineToUi::SketchQueried {
            result: crate::sketch_query::answer(&live, &query),
        }),

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
        UiToEngine::MeasureSection {
            body_ids,
            plane_origin,
            plane_normal,
        } => measure_section(state, kb, &body_ids, plane_origin, plane_normal),
        UiToEngine::MeasureThickness { body_id, spacing_m } => {
            measure_thickness(state, kb, &body_id, spacing_m)
        }
        UiToEngine::ListFaces { body_id, filter } => {
            list_faces(state, kb, &body_id, filter.as_ref())
        }
        UiToEngine::ListEntities {
            body_id,
            kind,
            filter,
        } => list_entities(state, kb, &body_id, kind, filter.as_ref()),

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

/// What the projection declined to decide, named and counted, as a warning
/// to travel with an exported file.
///
/// Every counter but `cross_body` is a line the drawing does NOT carry, so a
/// file with a large count is a degenerate view rather than a clean one — and
/// without this the caller has no way to tell. One function, two exports
/// (the model's view and the sheet), so the two cannot word it differently.
fn decline_warning(
    declines: &waffle_types::kernel::projection::ProjectionDeclines,
) -> Option<String> {
    let declined: Vec<String> = declines
        .counts()
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    if declined.is_empty() {
        return None;
    }
    Some(format!(
        "the projection declined to decide some of this drawing ({}); every one but \
         cross_body means a line the drawing does not carry — see \
         specs/drawings_and_mbd.md D1c",
        declined.join(", ")
    ))
}

/// `ExportDxf` on a `Drawing` tab (D4a): the sheet, or one of its views.
///
/// Each view is projected in its OWN frame and then placed in paper space —
/// scaled by the view's scale and moved to its position — and the placed
/// views are composed into one `ViewGeometry` that the D1a writer writes. The
/// composition is in paper METERS, not millimetres, because `write_dxf`
/// converts meters to millimetres itself; handing it millimetres would write
/// a sheet a thousand times too large.
fn export_sheet_dxf(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    sheet_id: Option<uuid::Uuid>,
    view_id: Option<uuid::Uuid>,
) -> Result<EngineToUi, BridgeError> {
    use waffle_types::kernel::projection::ViewGeometry;

    let open = state
        .drawing
        .as_ref()
        .ok_or_else(|| BridgeError::InvalidRequest {
            reason: "no drawing is open".to_string(),
        })?;
    let tab_id = open.tab_id.clone();
    let drawing = state.session.drawing(&tab_id)?.clone();
    let sheet = match sheet_id {
        Some(id) => drawing
            .sheet(id)
            .ok_or_else(|| BridgeError::InvalidRequest {
                reason: format!("this drawing has no sheet {id}"),
            })?,
        None => drawing
            .sheets
            .first()
            .ok_or_else(|| BridgeError::InvalidRequest {
                reason: "this drawing has no sheets".to_string(),
            })?,
    };
    let wanted: Vec<&feature_engine::drawing::DrawingView> = match view_id {
        Some(id) => vec![sheet.view(id).ok_or_else(|| BridgeError::InvalidRequest {
            reason: format!("sheet `{}` has no view {id}", sheet.name),
        })?],
        None => sheet.views.iter().collect(),
    };

    let part_trees = state.session.part_trees(&state.engine);
    let assembly_trees = state.session.assembly_trees();
    let mut reuse = state.take_part_engines();
    let document_name = state.session.document().name.clone();
    let eval = crate::drawing_view::evaluate(
        &drawing,
        &document_name,
        &part_trees,
        &assembly_trees,
        &state.engine.sources,
        kb,
        &mut reuse,
    );
    state.park_unused_part_engines(reuse, &eval.parts);

    let mut composed = ViewGeometry::default();
    // The hatch, placed into paper space beside the curves (D4c). Kept apart
    // from `composed` because a hatch line is NOT a projected curve — it has
    // no source entity and no visibility to derive a layer from, which is
    // exactly why the writer now takes a layer per curve.
    let mut hatch: Vec<waffle_types::kernel::projection::Curve2> = Vec::new();
    let mut warnings = eval.warnings.clone();
    warnings.extend(eval.errors.iter().cloned());
    let mut drawn = 0usize;
    for view in &wanted {
        let Some(geometry) = eval.geometry.get(&view.id) else {
            continue;
        };
        // A DETAIL view's curves are CLIPPED to its crop disc (D4c), not just
        // culled to the disc's box as the layout carries them.
        //
        // D4b left this open and the SVG and the PDF have always clipped — by
        // a `clipPath`, which the DXF has no equivalent of. So the export
        // trimmed nothing and a cutting table given a detail got the
        // overhang: every curve that merely REACHED the disc, whole. The
        // trimming happens in VIEW coordinates, before the placement, because
        // the crop is authored there; and `Curve2::clipped_to_disc` keeps each
        // piece's kind, so a detail's DXF still carries `LINE` and `ARC`
        // entities rather than the chord polylines a clip through sampled
        // geometry would leave.
        //
        // Still not clipped in the LAYOUT: the renderer's `clipPath` is exact
        // and free, and trimming there would make the detail — the one view
        // that exists to be looked at closely — the only view drawn from
        // geometry the engine had to cut.
        let cropped;
        let geometry = match &view.projection {
            feature_engine::drawing::Projection::Detail { center, radius, .. } => {
                let kept: Vec<_> = geometry
                    .curves
                    .iter()
                    .flat_map(|c| {
                        c.geometry
                            .clipped_to_disc(
                                *center,
                                *radius,
                                kernel_v2::dxf_export::DEFAULT_POLYLINE_SAGITTA,
                            )
                            .into_iter()
                            .map(|piece| waffle_types::kernel::projection::ProjectedCurve {
                                geometry: piece,
                                ..c.clone()
                            })
                    })
                    .collect();
                // The BOX stays the crop's, which is what `rebuild_view_in`
                // gave the layout: a detail is laid out on its disc rather
                // than on whatever survived the cut, so "2:1 doubles the
                // paper span of the same crop" holds in the file as well as
                // on the sheet.
                cropped = waffle_types::kernel::projection::ViewGeometry {
                    curves: kept,
                    bbox: geometry.bbox,
                    declines: geometry.declines,
                };
                &cropped
            }
            _ => geometry,
        };
        // One view alone goes at the paper origin: a cutting table given a
        // single part should not have to find it at the sheet coordinates of
        // a drawing it is not reading.
        let offset = if view_id.is_some() {
            [0.0, 0.0]
        } else {
            // `placement_mm` is the view's CENTRE, and the curves are in
            // view-plane coordinates, so the offset is the placement less
            // the scaled centre of the view's own box.
            let centre = geometry
                .bbox
                .map(|b| [0.5 * (b.min.x() + b.max.x()), 0.5 * (b.min.y() + b.max.y())])
                .unwrap_or([0.0, 0.0]);
            [
                view.placement_mm[0] / 1000.0 - centre[0] * view.scale,
                view.placement_mm[1] / 1000.0 - centre[1] * view.scale,
            ]
        };
        let Some(placed) = geometry.transformed(view.scale, offset) else {
            warnings.push(format!(
                "view `{}` has scale {} and could not be placed on the sheet",
                view.name, view.scale
            ));
            continue;
        };
        composed.extend(placed);
        // A section view's hatch, through the same similarity as its curves
        // (D4c). The segments are the ENGINE's — one scanline for the screen,
        // the PDF and this file — so a reader measuring a hatch line on the
        // sheet measures the line in the DXF.
        if let Some(layout) = eval.layouts.get(&view.id) {
            for line in waffle_types::annotation::hatch::segments_as_curves(&layout.hatch_segments)
            {
                // The SAME similarity the curves went through, so the hatch
                // cannot drift off the cap it fills. `None` is impossible
                // here — the scale was already accepted above for the
                // curves — and skipping rather than unwrapping keeps the
                // export from panicking if that ever stops being true.
                if let Some(placed) = line.transformed(view.scale, offset) {
                    hatch.push(placed);
                }
            }
        }
        drawn += 1;
    }
    if drawn == 0 {
        return Err(BridgeError::NoMeshData);
    }
    let mut curves: Vec<kernel_v2::DxfCurve> = composed
        .curves
        .iter()
        .map(kernel_v2::dxf_export::dxf_curve)
        .collect();
    curves.extend(hatch.iter().map(|c| kernel_v2::DxfCurve {
        geometry: c,
        layer: kernel_v2::LAYER_HATCH,
    }));
    // The extents cover the hatch too: a reader zooms to `$EXTMAX`, and a
    // hatch line reaching past the outline it fills (a chord-sampled cap
    // boundary can, by its sagitta) would otherwise fall outside the box the
    // file declares.
    let bbox = hatch.iter().fold(composed.bbox, |box_, curve| {
        let own = curve.bbox();
        Some(match box_ {
            Some(b) => b.united(own),
            None => own,
        })
    });
    let dxf_data = kernel_v2::write_dxf_layers(
        &curves,
        bbox,
        kernel_v2::dxf_export::DEFAULT_POLYLINE_SAGITTA,
    );
    warnings.extend(decline_warning(&composed.declines));
    Ok(EngineToUi::DxfExportReady { dxf_data, warnings })
}

/// Open (or re-evaluate) a `Drawing` tab (D4a): build and project every
/// view's source tab, write the layouts back onto the views.
///
/// The drawing twin of [`open_assembly`], in the same order and for the same
/// reasons: refuse a tab that holds no drawing BEFORE switching to it, hand
/// the part engines of whatever is being left to this pass, and park what
/// this pass did not take.
fn open_drawing(
    state: &mut EngineState,
    tab_id: &str,
    kb: &mut dyn KernelBundle,
) -> Result<(), BridgeError> {
    state.active_sketch = None;
    state.selection.clear();
    state.hover = None;
    let drawing = state.session.drawing(tab_id)?.clone();
    let mut reuse = state.take_part_engines();
    state.session.switch_tab(tab_id, &mut state.engine)?;
    // A Drawing tab holds no tree; keep the renderer off the live one.
    state.engine.tree = feature_engine::types::FeatureTree::new();
    state.engine.rebuild_from_scratch(kb);
    let part_trees = state.session.part_trees(&state.engine);
    let assembly_trees = state.session.assembly_trees();
    let document_name = state.session.document().name.clone();
    let eval = crate::drawing_view::evaluate(
        &drawing,
        &document_name,
        &part_trees,
        &assembly_trees,
        &state.engine.sources,
        kb,
        &mut reuse,
    );
    state.park_unused_part_engines(reuse, &eval.parts);
    // The layouts are derived, and they are saved with the tab (§5.7) — so
    // they go back into the tab that was just evaluated, which is also where
    // the status reads them from.
    state
        .session
        .set_drawing_caches(tab_id, &eval.layouts, &eval.cache_keys, &eval.title_blocks);
    state.drawing = Some(eval.open(tab_id));
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

/// `ListEntities` (Q6 of `specs/agent_mechanical_design.md` §4.2): every
/// face, edge or vertex of one body with its full geometric content.
///
/// Assembled from doors that already exist rather than from a new kernel
/// listing: the persistent ids from `all_entity_pids` (D0), the signature from
/// `compute_signature` (N0), the axis line from `entity_axis`, the name from
/// the N1 table, the arc length from `KernelMeasure::edge_length` (the one new
/// kernel capability in this increment), and the body frame from Q3's
/// `mass_properties`. The FACE arm hands out the very reference `face_list`
/// does (the shared `face_refs` builder), so the two tools cannot drift.
///
/// **Order is by persistent id.** Those ids are content-seeded, so the order
/// survives a rebuild and a fresh process — which is what makes a pinned
/// listing a pin rather than a snapshot of one run's arena counters. An
/// entity with no id (a mesh-backed import) sorts after the ones that have
/// them, by its canonical signature JSON, because *some* total order has to
/// exist and it must not be insertion order.
fn list_entities(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
    kind: crate::messages::EntityListKind,
    filter: Option<&crate::messages::EntityListFilter>,
) -> Result<EngineToUi, BridgeError> {
    use crate::messages::{EntityListKind, ListedEntity};
    use waffle_types::TopoKind;

    crate::tessellation_runner::tessellate_engine(&mut state.engine, kb);
    let (named, unresolved_names) = crate::entity_names::name_bindings(state, kb, body_id);
    let topo = kind.topo();

    // Q3's own answer, carried: the body frame is the same integration
    // `measure_mass` reports, at the default density, so the axes an agent
    // reads here and there cannot disagree. A kernel refusal is reported as
    // the reason there are no axes, never as a failed listing.
    let body_frame = {
        let handle = find_body(state, body_id)
            .map(|b| b.handle.clone())
            .ok_or_else(|| BridgeError::InvalidRequest {
                reason: format!("no live body {body_id}"),
            })?;
        match kb.as_measure().mass_properties(&handle, None) {
            Ok(m) => crate::messages::ListedBodyFrame {
                centroid: Some(m.centroid),
                principal_moments: Some(m.principal_moments),
                principal_axes: Some(m.principal_axes),
                method: Some(match m.method {
                    waffle_types::kernel::Method::Exact => MeasureMethod::Exact,
                    waffle_types::kernel::Method::Mesh { .. } => MeasureMethod::Mesh,
                }),
                unavailable: None,
            },
            Err(e) => crate::messages::ListedBodyFrame {
                centroid: None,
                principal_moments: None,
                principal_axes: None,
                method: None,
                unavailable: Some(e.to_string()),
            },
        }
    };

    // The arc length of every edge, before the immutable borrow below: it
    // goes through `KernelBundle`, which the listing's borrow of the engine
    // would otherwise lock out.
    let lengths: std::collections::HashMap<waffle_types::kernel::KernelId, _> =
        if topo == TopoKind::Edge {
            let handle = find_body(state, body_id)
                .map(|b| b.handle.clone())
                .ok_or_else(|| BridgeError::InvalidRequest {
                    reason: format!("no live body {body_id}"),
                })?;
            let edges = kb.as_introspect().list_edges(&handle);
            edges
                .into_iter()
                .map(|e| (e, kb.as_measure().edge_length(e)))
                .collect()
        } else {
            std::collections::HashMap::new()
        };

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
    let introspect = kb.as_introspect();

    let pids: std::collections::HashMap<_, _> = introspect
        .all_entity_pids(&body.handle, topo)
        .into_iter()
        .collect();

    // The reference per entity. A face's is the shared builder's, so it is
    // byte-identical to `face_list`'s; an edge's or a vertex's is the
    // persistent-id selector D0 landed, minted `Strict` exactly as N1 mints a
    // name's (a pid never rebinds, so a `BestEffort` pid ref would be a
    // policy that cannot apply).
    let face_refs: std::collections::HashMap<_, _> = match kind {
        EntityListKind::Face => body
            .mesh
            .as_ref()
            .map(|mesh| {
                crate::face_refs::face_geom_refs(
                    feature_id,
                    key,
                    mesh,
                    &result.provenance.role_assignments,
                    introspect,
                )
                .into_iter()
                .collect()
            })
            .ok_or(BridgeError::NoMeshData)?,
        _ => std::collections::HashMap::new(),
    };
    let pid_ref = |pid: &waffle_types::kernel::EntityPid| waffle_types::GeomRef {
        kind: topo,
        anchor: waffle_types::Anchor::FeatureOutput {
            feature_id,
            output_key: key.clone(),
        },
        selector: waffle_types::Selector::Pid {
            pid: pid.pid,
            root_pid: pid.root_pid,
        },
        policy: waffle_types::ResolvePolicy::Strict,
        scope: None,
    };

    // Which entities to list: the KERNEL's own listing in every case, never
    // the render mesh's face ranges. `face_list` iterates the mesh, so a face
    // that renders no triangles is invisible to it; here such a face is
    // listed with a null `geom_ref`, which is strictly more informative and
    // is honest about which half of it is missing.
    let ids: Vec<waffle_types::kernel::KernelId> = match kind {
        EntityListKind::Face => introspect.list_faces(&body.handle),
        EntityListKind::Edge => introspect.list_edges(&body.handle),
        EntityListKind::Vertex => introspect.list_vertices(&body.handle),
    };

    let mut entities: Vec<ListedEntity> = ids
        .into_iter()
        .map(|id| {
            let signature = introspect.compute_signature(id, topo);
            let pid = pids.get(&id);
            let length = lengths.get(&id);
            ListedEntity {
                pid: pid.map(|p| p.pid),
                root_pid: pid.map(|p| p.root_pid),
                geom_ref: match kind {
                    EntityListKind::Face => face_refs.get(&id).cloned(),
                    _ => pid.map(pid_ref),
                },
                name: named.get(&(topo, id)).map(|b| b.name.clone()),
                name_warnings: named
                    .get(&(topo, id))
                    .map(|b| b.warnings.clone())
                    .unwrap_or_default(),
                axis: introspect.entity_axis(id, topo).map(listed_axis),
                length: length.and_then(|r| r.as_ref().ok()).map(|r| {
                    use waffle_types::kernel::LengthMethod;
                    crate::messages::ListedLength {
                        arc_length_m: r.value,
                        curve_type: r.curve_type.to_string(),
                        closed: r.closed,
                        method: match r.method {
                            LengthMethod::Exact => crate::messages::LengthTierWire::Exact,
                            LengthMethod::Quadrature { .. } => {
                                crate::messages::LengthTierWire::Quadrature
                            }
                            LengthMethod::Chords { .. } => crate::messages::LengthTierWire::Chords,
                        },
                        residual_m: match r.method {
                            LengthMethod::Quadrature { residual } => Some(residual),
                            _ => None,
                        },
                        chord_bound_m: match r.method {
                            LengthMethod::Chords { chord_bound } => chord_bound,
                            _ => None,
                        },
                    }
                }),
                length_unavailable: length.and_then(|r| r.as_ref().err()).map(|e| e.to_string()),
                position: (topo == TopoKind::Vertex)
                    .then_some(signature.centroid)
                    .flatten(),
                signature,
            }
        })
        .collect();

    let mut excluded_unevaluable = 0usize;
    entities.retain(|e| match passes_entity_filter(e, filter) {
        FilterVerdict::Pass => true,
        FilterVerdict::Reject => false,
        FilterVerdict::Unevaluable => {
            excluded_unevaluable += 1;
            false
        }
    });

    // By persistent id; the id-less tail by its own content.
    // `sort_by_cached_key` so a signature is rendered once per entity rather
    // than once per comparison, and only for the entities that can need it:
    // the content key breaks ties among the id-LESS tail, so an entity with a
    // pid never pays for rendering one. (A gear body lists ~1000 edges, every
    // one of them with an id.) `TopoSignature` is a plain struct of scalars,
    // so serde emits its fields in declaration order and the string is the
    // same in every process — which the fresh-process oracle at the bottom of
    // `tests/tool_entity_list.rs` is what actually proves.
    entities.sort_by_cached_key(|e| {
        (
            e.pid.is_none(),
            e.pid.unwrap_or(0),
            e.root_pid.unwrap_or(0),
            e.pid
                .is_none()
                .then(|| serde_json::to_string(&e.signature).unwrap_or_default()),
        )
    });

    Ok(EngineToUi::EntitiesListed {
        body_id: body_id.to_string(),
        kind,
        entities,
        body: body_frame,
        excluded_unevaluable,
        unresolved_names,
    })
}

/// `KernelIntrospect::entity_axis` on the wire (Q6).
///
/// The one judgement here is the SPHERE. `EntityAxis::direction` is an
/// infallible `[f64; 3]`, and for a sphere the kernel fills it with its own
/// canonical pole — documented on `AxisKind::Spherical` as "the kernel's
/// canonical pole axis", not as the sphere's. A sphere is isotropic and has
/// no axis: publishing that pole would tell an agent a sphere is oriented
/// along z, and it would contradict `signature.axis.direction`, which N0
/// already reports as `null` for a sphere. So a spherical axis carries its
/// CENTRE and its radius, and no direction at all.
fn listed_axis(a: waffle_types::kernel::EntityAxis) -> crate::messages::ListedAxis {
    crate::messages::ListedAxis {
        kind: a.kind.label().to_string(),
        origin: a.origin,
        direction: match a.kind {
            waffle_types::kernel::AxisKind::Spherical => None,
            _ => Some(a.direction),
        },
        radius: a.radius,
    }
}

/// What a Q6 filter made of one entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterVerdict {
    /// Every present arm was satisfied. List it.
    Pass,
    /// An arm was asked and answered no. Drop it.
    Reject,
    /// An arm could not be asked of this entity at all, because the entity's
    /// own data does not carry what the arm is about. Dropped — but COUNTED,
    /// because "nothing matched" and "nothing could be asked" want opposite
    /// responses from a caller and an empty list says neither.
    Unevaluable,
}

/// Whether one listed entity passes every present arm of a Q6 filter.
///
/// The arms COMPOSE by conjunction, and each one that cannot be evaluated
/// EXCLUDES rather than admits: a listing's job is to answer "which entities
/// satisfy this", and an entity whose data cannot answer does not satisfy it.
/// An exclusion for THAT reason is reported separately (see
/// [`FilterVerdict::Unevaluable`]).
///
/// A missing name is a `Reject`, not an `Unevaluable`: "this entity's name
/// does not match the glob" is a true and complete answer when the entity has
/// no name. A missing bbox is the real `Unevaluable` — whether the entity lies
/// inside the box is a question its signature simply cannot answer.
fn passes_entity_filter(
    entity: &crate::messages::ListedEntity,
    filter: Option<&crate::messages::EntityListFilter>,
) -> FilterVerdict {
    let Some(filter) = filter else {
        return FilterVerdict::Pass;
    };
    if let Some(query) = &filter.query {
        if !feature_engine::resolve::passes_all_filters(&entity.signature, &query.filters) {
            return FilterVerdict::Reject;
        }
    }
    if let Some(glob) = &filter.name {
        match &entity.name {
            Some(name) if glob_matches(glob, name) => {}
            _ => return FilterVerdict::Reject,
        }
    }
    if let Some([min, max]) = &filter.bbox {
        let Some(bb) = entity.signature.bbox else {
            return FilterVerdict::Unevaluable;
        };
        for k in 0..3 {
            if bb[k] < min[k] || bb[k + 3] > max[k] {
                return FilterVerdict::Reject;
            }
        }
    }
    FilterVerdict::Pass
}

/// `*` (any run, including empty) and `?` (exactly one character) against a
/// whole name. Dynamic-programming match, so a pattern with several `*` is
/// linear in the product of the lengths rather than exponential.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    // `reach[j]`: the pattern's first `j` characters match the first `i`
    // characters of the name, for the `i` of the current row.
    let mut reach = vec![false; p.len() + 1];
    reach[0] = true; // the empty pattern matches the empty prefix
    for j in 0..p.len() {
        // …and a leading run of stars matches it too.
        reach[j + 1] = reach[j] && p[j] == '*';
    }
    for &c in &n {
        let mut next = vec![false; p.len() + 1];
        // The empty pattern cannot match a non-empty prefix, so `next[0]`
        // stays false.
        for j in 0..p.len() {
            next[j + 1] = match p[j] {
                // Either the star matched nothing (`next[j]`, this row) or it
                // swallowed `c` too (`reach[j + 1]`, the previous row).
                '*' => next[j] || reach[j + 1],
                '?' => reach[j],
                lit => reach[j] && lit == c,
            };
        }
        reach = next;
    }
    reach[p.len()]
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

/// Unit-length `v`, or `None` when it is zero-length or non-finite.
fn unit3(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n.is_finite() && n > 0.0).then(|| [v[0] / n, v[1] / n, v[2] / n])
}

/// One cap curve on the wire (Q4). A straight mirror of
/// `waffle_types::kernel::projection::Curve2`; see `messages::SectionCurve`
/// for why it is mirrored rather than serialized.
fn wire_section_curve(
    c: &waffle_types::kernel::projection::Curve2,
) -> crate::messages::SectionCurve {
    use crate::messages::SectionCurve as W;
    use waffle_types::kernel::projection::Curve2 as C;
    // `Point2` is `cad_primitives`', which this crate does not depend on, so
    // every coordinate is read through its accessors and never named.
    match c {
        C::Point(q) => W::Point { at: [q.x(), q.y()] },
        C::Line { start, end } => W::Line {
            start: [start.x(), start.y()],
            end: [end.x(), end.y()],
        },
        C::Circle {
            center,
            radius,
            start_angle,
            end_angle,
        } => W::Circle {
            center: [center.x(), center.y()],
            radius: *radius,
            start_angle_rad: *start_angle,
            end_angle_rad: *end_angle,
        },
        C::Ellipse {
            center,
            major_axis,
            major_radius,
            minor_radius,
            start_param,
            end_param,
        } => W::Ellipse {
            center: [center.x(), center.y()],
            major_axis: *major_axis,
            major_radius: *major_radius,
            minor_radius: *minor_radius,
            start_param: *start_param,
            end_param: *end_param,
        },
        C::Polyline { points, closed } => W::Polyline {
            points: points.iter().map(|q| [q.x(), q.y()]).collect(),
            closed: *closed,
        },
    }
}

/// The cap's area centroid in the cap frame, by Green's theorem over every
/// loop FLATTENED at `kernel_v2::dxf_export::DEFAULT_POLYLINE_SAGITTA`
/// (1e-5 m) — the same absolute sagitta the DXF writer flattens an ellipse at,
/// so the one number this layer approximates is approximated at a stated
/// density rather than at an invented one.
///
/// Holes carry their own sign: a clockwise loop's cross products are negative,
/// so a hole subtracts both area and moment without being special-cased.
/// `None` when the flattened polygons enclose no area — an empty section, or a
/// cap so degenerate there is no point to report.
fn cap_centroid(loops: &[waffle_types::kernel::projection::SectionLoop]) -> Option<[f64; 2]> {
    let sagitta = kernel_v2::dxf_export::DEFAULT_POLYLINE_SAGITTA;
    let (mut a2, mut mx, mut my) = (0.0f64, 0.0f64, 0.0f64);
    for lp in loops {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for c in &lp.curves {
            for q in c.flatten(sagitta) {
                // Consecutive curves share an endpoint; the duplicate would
                // contribute a zero-length edge, which is harmless, but it
                // doubles the point count on a many-curve loop.
                if pts.last() != Some(&[q.x(), q.y()]) {
                    pts.push([q.x(), q.y()]);
                }
            }
        }
        if pts.len() >= 2 && pts.first() == pts.last() {
            pts.pop();
        }
        for i in 0..pts.len() {
            let (p0, p1) = (pts[i], pts[(i + 1) % pts.len()]);
            let cross = p0[0] * p1[1] - p1[0] * p0[1];
            a2 += cross;
            mx += (p0[0] + p1[0]) * cross;
            my += (p0[1] + p1[1]) * cross;
        }
    }
    (a2.is_finite() && a2 != 0.0).then(|| [mx / (3.0 * a2), my / (3.0 * a2)])
}

/// `MeasureSection` (Q4 of `specs/agent_mechanical_design.md` §4.2): the cap
/// loops of a planar cut through each body, as 2D curve data.
///
/// The kernel does the cutting (D1d `section_with_plane`, the real Intersect
/// against a half-space box); this handler only turns body ids into handles
/// and the kernel's answer into the wire shape, and decides what to do with a
/// body the kernel refused.
///
/// **A refusal is a DECLINE, not an empty section.** One body in a multi-body
/// part hitting the Stage-0 coplanar wall must not make the other bodies'
/// sections unavailable, and it must not look like a plane that missed. So the
/// refusal is named in `declines` with its typed kind, and the body simply has
/// no entry in `bodies`.
fn measure_section(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    body_ids: &[String],
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
) -> Result<EngineToUi, BridgeError> {
    use crate::messages::{
        SectionBasis, SectionCapLoop, SectionDecline, SectionDeclineKind, SectionLoopKind,
        SectionedBody,
    };

    if !plane_origin.iter().all(|v| v.is_finite()) {
        return Err(BridgeError::InvalidRequest {
            reason: format!("the section plane origin {plane_origin:?} is not finite"),
        });
    }
    let normal = unit3(plane_normal).ok_or_else(|| BridgeError::InvalidRequest {
        reason: format!(
            "the section plane normal {plane_normal:?} is zero-length or not finite; a plane \
             needs a direction"
        ),
    })?;

    let mut bodies = Vec::new();
    let mut declines = Vec::new();
    let mut basis: Option<SectionBasis> = None;
    for body_id in body_ids {
        let handle = find_body(state, body_id)
            .map(|b| b.handle.clone())
            .ok_or_else(|| BridgeError::InvalidRequest {
                reason: format!("no live body {body_id}"),
            })?;
        let cut = match kb.section_with_plane(&handle, plane_origin, normal) {
            Ok(cut) => cut,
            Err(waffle_types::kernel::KernelError::NotSupported { operation }) => {
                declines.push(SectionDecline {
                    body_id: body_id.clone(),
                    kind: SectionDeclineKind::NotSupported,
                    reason: operation,
                });
                continue;
            }
            Err(other) => {
                declines.push(SectionDecline {
                    body_id: body_id.clone(),
                    kind: SectionDeclineKind::Failed,
                    reason: other.to_string(),
                });
                continue;
            }
        };
        basis.get_or_insert(SectionBasis {
            origin: cut.plane_basis.origin,
            u_axis: cut.plane_basis.u,
            v_axis: cut.plane_basis.v,
            w_axis: cut.plane_basis.w,
        });
        let centroid_uv = cap_centroid(&cut.cap_loops);
        let b = cut.plane_basis;
        bodies.push(SectionedBody {
            body_id: body_id.clone(),
            area_m2: cut.cap_area(),
            centroid_uv,
            centroid: centroid_uv
                .map(|[u, v]| [0, 1, 2].map(|k| b.origin[k] + u * b.u[k] + v * b.v[k])),
            centroid_exact: cut.cap_loops.iter().all(|l| {
                l.curves
                    .iter()
                    .all(|c| matches!(c, waffle_types::kernel::projection::Curve2::Line { .. }))
            }),
            method: if cut.exact() {
                MeasureMethod::Exact
            } else {
                MeasureMethod::Mesh
            },
            cap_shared_with_model: cut.cap_shared_with_model,
            kept_material: cut.cut_solid.is_some(),
            loops: cut
                .cap_loops
                .iter()
                .map(|lp| SectionCapLoop {
                    curves: lp.curves.iter().map(wire_section_curve).collect(),
                    signed_area_m2: lp.signed_area,
                    exact: lp.exact,
                    kind: if lp.signed_area < 0.0 {
                        SectionLoopKind::Hole
                    } else {
                        SectionLoopKind::Outer
                    },
                })
                .collect(),
        });
    }

    Ok(EngineToUi::SectionMeasured {
        plane_origin,
        plane_normal: normal,
        basis,
        bodies,
        declines,
    })
}

/// `MeasureThickness` (Q5 of `specs/agent_mechanical_design.md` §4.2): the
/// body's wall thickness, sampled.
///
/// The faces of the thinnest site arrive as transient kernel ids, and this
/// handler is where they become something an agent can keep: the persistent id
/// (as a decimal STRING — these are content-seeded `u64`s and a JSON number
/// rounds the ones above `2^53` onto a different entity) and the N1 name, from
/// the same `all_entity_pids` and `name_bindings` the Q6 listing reads. A site
/// that names "face 47 of this session" is not an answer anyone can act on
/// tomorrow.
fn measure_thickness(
    state: &mut EngineState,
    kb: &mut dyn KernelBundle,
    body_id: &str,
    spacing_m: Option<f64>,
) -> Result<EngineToUi, BridgeError> {
    use crate::messages::{
        MeasuredThickness, ThicknessBin, ThicknessDeclines, ThicknessFace, ThicknessMethod,
        ThinnestSite,
    };
    use waffle_types::kernel::ThicknessOpts;
    use waffle_types::TopoKind;

    // The sampler reads the arena, not the render mesh, but `name_bindings`
    // and the pid table are read off the built body — so the body must be
    // built, which is what every other query here ensures too.
    crate::tessellation_runner::tessellate_engine(&mut state.engine, kb);
    let handle = find_body(state, body_id)
        .map(|b| b.handle.clone())
        .ok_or_else(|| BridgeError::InvalidRequest {
            reason: format!("no live body {body_id}"),
        })?;
    let t = kb
        .as_measure()
        .thickness(&handle, &ThicknessOpts { spacing: spacing_m })
        .map_err(measure_error)?;

    let (named, _unresolved) = crate::entity_names::name_bindings(state, kb, body_id);
    let pids: std::collections::HashMap<_, _> = kb
        .as_introspect()
        .all_entity_pids(&handle, TopoKind::Face)
        .into_iter()
        .collect();
    let face = |r: waffle_types::kernel::EntityRef| {
        let pid = pids.get(&r.entity);
        ThicknessFace {
            kernel_id: r.entity.0,
            pid: pid.map(|p| p.pid),
            root_pid: pid.map(|p| p.root_pid),
            name: named
                .get(&(TopoKind::Face, r.entity))
                .map(|b| b.name.clone()),
        }
    };
    let (samples, spacing_m) = match t.method {
        waffle_types::kernel::ThicknessMethod::Sampled { samples, spacing } => (samples, spacing),
    };
    let site = |s: waffle_types::kernel::ThicknessSite| ThinnestSite {
        thickness_m: s.thickness,
        point: s.point,
        opposite: s.opposite,
        from: face(s.from),
        to: face(s.to),
        faces_share_an_edge: s.faces_share_an_edge,
    };
    Ok(EngineToUi::ThicknessMeasured {
        result: MeasuredThickness {
            body_id: body_id.to_string(),
            min_m: t.min,
            min_wall_m: t.min_wall,
            mean_m: t.mean,
            max_m: t.max,
            thinnest: site(t.thinnest),
            thinnest_wall: t.thinnest_wall.map(site),
            histogram: t
                .histogram
                .iter()
                .map(|b| ThicknessBin {
                    lo_m: b.lo,
                    hi_m: b.hi,
                    count: b.count,
                })
                .collect(),
            samples,
            spacing_m,
            chord_bound_m: t.chord_bound,
            refined: t.refined,
            declines: ThicknessDeclines {
                no_hit: t.declines.no_hit,
                below_self_band: t.declines.below_self_band,
                no_surface: t.declines.no_surface,
            },
            method: ThicknessMethod::Sampled,
        },
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

/// The open `Drawing` tab's evaluated state, or `None` when no drawing is
/// open.
///
/// The drawing itself is read from the SESSION, not from the stored
/// evaluation: the evaluation wrote the view caches into the tab, so the tab
/// is the one copy of the sheets, and a second one here would be the next
/// thing to go stale.
fn drawing_status(state: &EngineState) -> Option<crate::messages::DrawingStatus> {
    let open = state.drawing.as_ref()?;
    let drawing = state.session.drawing(&open.tab_id).ok()?.clone();
    Some(crate::messages::DrawingStatus {
        tab_id: open.tab_id.clone(),
        drawing,
        anchors: open.anchors.clone(),
        declines: open.declines.clone(),
        errors: open.errors.clone(),
        warnings: open.warnings.clone(),
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
        feature_warnings: state.engine.feature_warnings.clone(),
        consumed_features: {
            let mut v: Vec<uuid::Uuid> = state.engine.consumed_features.iter().copied().collect();
            v.sort();
            v
        },
        preview_mesh,
        sources: source_statuses(state),
        assembly: assembly_status(state),
        drawing: drawing_status(state),
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

/// Every live body of `engine` as a projection body, named the way the STEP
/// export names them (`collect_step_bodies`), so a drawing view and an
/// exported file agree about which body is which.
///
/// One collection, two consumers: `StepExportBody` and `ProjectionBody` carry
/// the same three fields, and the mapping here is the one the `ExportDxf` arm
/// already did inline. A mesh-backed imported body is left out with the same
/// named warning as in STEP export — the projection needs B-Rep edges, and a
/// mesh body's triangle edges are not the part's edges.
pub(crate) fn projection_bodies(
    engine: &feature_engine::Engine,
    kernel: &dyn KernelIntrospect,
    prefix: &str,
    placement: Option<RigidPlacement>,
) -> (Vec<waffle_types::kernel::ProjectionBody>, Vec<String>) {
    let mut bodies = Vec::new();
    let mut warnings = Vec::new();
    collect_step_bodies(
        engine,
        kernel,
        prefix,
        placement,
        &mut bodies,
        &mut warnings,
    );
    (
        bodies
            .into_iter()
            .map(|b| waffle_types::kernel::ProjectionBody {
                handle: b.handle,
                name: b.name,
                placement: b.placement,
            })
            .collect(),
        warnings,
    )
}

/// An assembly placement as the kernel's rigid motion (rotation columns =
/// the transformed basis vectors).
pub(crate) fn rigid_placement_of(t: &feature_engine::assembly::Transform) -> RigidPlacement {
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
            arg_dimensions: Default::default(),
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

#[cfg(test)]
mod q6_filter_tests {
    use super::*;
    use crate::messages::{EntityListFilter, ListedEntity};

    fn entity(bbox: Option<[f64; 6]>, name: Option<&str>) -> ListedEntity {
        ListedEntity {
            pid: Some(7),
            root_pid: Some(7),
            geom_ref: None,
            name: name.map(str::to_string),
            name_warnings: Vec::new(),
            signature: waffle_types::TopoSignature {
                bbox,
                ..waffle_types::TopoSignature::empty()
            },
            axis: None,
            length: None,
            length_unavailable: None,
            position: None,
        }
    }

    fn bbox_filter() -> EntityListFilter {
        EntityListFilter {
            bbox: Some([[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]]),
            ..EntityListFilter::default()
        }
    }

    /// Q6 §4.3: the three verdicts, and in particular that a MISSING bbox is
    /// `Unevaluable` and not a plain reject.
    ///
    /// The distinction is the whole reason `excluded_unevaluable` exists on
    /// the answer: an agent that filters by region and gets nothing back must
    /// be able to tell "no entity is in that region" from "no entity could be
    /// asked about it", because the second one means its question was never
    /// answered.
    #[test]
    fn a_bbox_arm_cannot_be_asked_of_an_entity_with_no_bbox() {
        let f = bbox_filter();
        assert_eq!(
            passes_entity_filter(
                &entity(Some([0.1, 0.1, 0.1, 0.9, 0.9, 0.9]), None),
                Some(&f)
            ),
            FilterVerdict::Pass,
            "contained"
        );
        assert_eq!(
            passes_entity_filter(
                &entity(Some([0.1, 0.1, 0.1, 9.0, 0.9, 0.9]), None),
                Some(&f)
            ),
            FilterVerdict::Reject,
            "reaches out of the box — asked and answered no"
        );
        assert_eq!(
            passes_entity_filter(&entity(None, None), Some(&f)),
            FilterVerdict::Unevaluable,
            "no bbox at all — the arm could not be asked"
        );
        // No filter at all excludes nothing and counts nothing.
        assert_eq!(
            passes_entity_filter(&entity(None, None), None),
            FilterVerdict::Pass
        );
    }

    /// A missing NAME, by contrast, is a real answer: an entity with no name
    /// genuinely does not match a glob, so it is a `Reject` and must not
    /// inflate the unevaluable count.
    #[test]
    fn an_unnamed_entity_rejects_a_glob_rather_than_being_unevaluable() {
        let f = EntityListFilter {
            name: Some("front_*".to_string()),
            ..EntityListFilter::default()
        };
        assert_eq!(
            passes_entity_filter(&entity(None, Some("front_edge")), Some(&f)),
            FilterVerdict::Pass,
            "a name arm needs no bbox"
        );
        assert_eq!(
            passes_entity_filter(&entity(None, Some("back_edge")), Some(&f)),
            FilterVerdict::Reject
        );
        assert_eq!(
            passes_entity_filter(&entity(None, None), Some(&f)),
            FilterVerdict::Reject,
            "unnamed is a no, not an unknown"
        );
    }

    /// A sphere's listed axis is a CENTRE with no direction, and every other
    /// family keeps its direction (Q6 §4.2).
    ///
    /// The kernel's `EntityAxis` cannot express "no direction" — its field is
    /// an infallible `[f64; 3]` holding a canonical pole for a sphere — so
    /// the mapping to the wire is where the distinction has to be made, and
    /// this is the pin. `signature.axis.direction` is already `null` for a
    /// sphere, so the alternative was one payload stating two contradictory
    /// things about one face.
    #[test]
    fn a_sphere_publishes_a_centre_and_no_axis_direction() {
        use waffle_types::kernel::{AxisKind, EntityAxis};
        let axis = |kind| {
            listed_axis(EntityAxis {
                kind,
                origin: [1.0, 2.0, 3.0],
                direction: [0.0, 0.0, 1.0],
                radius: Some(0.5),
            })
        };
        let sphere = axis(AxisKind::Spherical);
        assert_eq!(sphere.kind, "spherical");
        assert_eq!(sphere.origin, [1.0, 2.0, 3.0], "the centre is still there");
        assert_eq!(sphere.radius, Some(0.5));
        assert_eq!(
            sphere.direction, None,
            "a sphere is isotropic: the kernel's canonical pole is not its orientation"
        );
        for kind in [
            AxisKind::Cylindrical,
            AxisKind::Conical,
            AxisKind::Toroidal,
            AxisKind::Circular,
            AxisKind::Elliptical,
        ] {
            assert_eq!(
                axis(kind).direction,
                Some([0.0, 0.0, 1.0]),
                "{kind:?} has a real axis and keeps it"
            );
        }
        // The token is `AxisKind`'s own label, so the wire and the kernel's
        // diagnostics cannot drift into two spellings of one family.
        assert_eq!(axis(AxisKind::Toroidal).kind, AxisKind::Toroidal.label());
    }

    /// The glob itself: `*` spans any run including empty, `?` exactly one,
    /// and a pattern matches the WHOLE name.
    #[test]
    fn the_glob_matches_whole_names() {
        for (pattern, name, want) in [
            ("*", "anything", true),
            ("*", "", true),
            ("front_*", "front_edge", true),
            ("front_*", "front_", true),
            ("front_*", "a_front_edge", false),
            ("*edge", "front_edge", true),
            ("*_*", "front_edge", true),
            ("b?re", "bore", true),
            ("b?re", "bre", false),
            ("b?re", "boore", false),
            ("bore", "bore", true),
            ("bore", "bores", false),
            ("", "", true),
            ("", "x", false),
            ("**a**", "a", true),
            ("*a*b*", "xaybz", true),
            ("*a*b*", "xbya", false),
        ] {
            assert_eq!(
                glob_matches(pattern, name),
                want,
                "glob {pattern:?} against {name:?}"
            );
        }
    }
}
