use base64::Engine as _;
use feature_engine::types::Operation;
use file_format::ProjectMetadata;
use kernel::RenderMesh;
use modeling_ops::KernelBundle;
use waffle_types::OutputKey;

use crate::engine_state::{BridgeError, EngineState};
use crate::messages::{EngineToUi, UiToEngine};

/// Dispatch a UI message to the engine and return a response.
///
/// This is the main entry point for processing messages from the JavaScript
/// main thread. Each message is dispatched to the appropriate engine method,
/// and the result is converted to an EngineToUi response.
pub fn dispatch(state: &mut EngineState, msg: UiToEngine, kb: &mut dyn KernelBundle) -> EngineToUi {
    match handle_message(state, msg, kb) {
        Ok(response) => response,
        Err(e) => EngineToUi::Error {
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

        UiToEngine::SolveSketch => {
            #[cfg(feature = "native-solver")]
            {
                let sketch = state.build_sketch()?;
                let solved = sketch_solver::solve_sketch(&sketch);
                if let Some(active) = state.active_sketch.as_mut() {
                    active.solve_status = solved.status.clone();
                }
                Ok(EngineToUi::SketchSolved { solved })
            }
            #[cfg(not(feature = "native-solver"))]
            {
                // In WASM builds, solving is done by the Emscripten-compiled
                // libslvs module via JS glue code in the web worker.
                Err(BridgeError::NotImplemented {
                    operation: "SolveSketch (use JS bridge to libslvs WASM)".to_string(),
                })
            }
        }

        UiToEngine::FinishSketch {
            solved_positions,
            solved_profiles,
            plane_origin,
            plane_normal,
            entities,
            constraints,
        } => {
            let sketch = state.finish_sketch(
                solved_positions,
                solved_profiles,
                plane_origin,
                plane_normal,
                entities,
                constraints,
            )?;
            let op = Operation::Sketch { sketch };
            // PR-VIZ-3a-fix: arm Yang capture buffer if enabled (spec §7).
            // Mirrors the AddFeature wrap above. PR-VIZ-3a missed this third
            // dispatch path; PR-VIZ-3b's canary test #3 (sketch→extrude→
            // boolean) empirically required it to capture stages from the
            // auto-union triggered downstream of FinishSketch.
            if state.yang_debug_capture_enabled {
                kernel::start_yang_debug_capture();
            }
            let result = state.engine.add_feature("Sketch".to_string(), op, kb);
            if state.yang_debug_capture_enabled {
                let stages = kernel::drain_yang_debug_capture();
                if let Some(feature_id) = state.engine.tree.features.last().map(|f| f.id) {
                    let failed_at = if state.engine.errors.iter().any(|(id, _)| *id == feature_id) {
                        Some(stages.len().saturating_sub(1))
                    } else {
                        None
                    };
                    state.yang_debug_captures.insert(
                        feature_id.to_string(),
                        kernel::FeatureStageCapture {
                            feature_id: feature_id.to_string(),
                            stages,
                            failed_at_stage: failed_at,
                        },
                    );
                }
            }
            result?;
            Ok(model_updated_response(state))
        }

        // -- Feature operations --
        UiToEngine::AddFeature { operation } => {
            let name = operation_name(&operation);
            // PR-VIZ-3a: arm Yang capture buffer if enabled (spec §7).
            // The drain MUST run on every exit path — success OR error —
            // or a leftover buffer would leak into the next dispatch and
            // mis-attribute stages. The early `?` return therefore needs
            // the manual two-phase wrap rather than a trailing block.
            if state.yang_debug_capture_enabled {
                kernel::start_yang_debug_capture();
            }
            let result = state.engine.add_feature(name, operation, kb);
            if state.yang_debug_capture_enabled {
                let stages = kernel::drain_yang_debug_capture();
                // Per resolved ambiguity #1: the just-added feature_id is
                // `tree.features.last()` AFTER add_feature returns.
                if let Some(feature_id) = state.engine.tree.features.last().map(|f| f.id) {
                    let failed_at = if state.engine.errors.iter().any(|(id, _)| *id == feature_id) {
                        Some(stages.len().saturating_sub(1))
                    } else {
                        None
                    };
                    state.yang_debug_captures.insert(
                        feature_id.to_string(),
                        kernel::FeatureStageCapture {
                            feature_id: feature_id.to_string(),
                            stages,
                            failed_at_stage: failed_at,
                        },
                    );
                }
            }
            result?;
            Ok(model_updated_response(state))
        }

        UiToEngine::EditFeature {
            feature_id,
            operation,
        } => {
            // PR-VIZ-3a: arm Yang capture buffer if enabled (spec §7).
            if state.yang_debug_capture_enabled {
                kernel::start_yang_debug_capture();
            }
            let result = state.engine.edit_feature(feature_id, operation, kb);
            if state.yang_debug_capture_enabled {
                let stages = kernel::drain_yang_debug_capture();
                // Per resolved ambiguity #2: EditFeature uses the
                // message's own feature_id (not tree.last).
                let failed_at = if state.engine.errors.iter().any(|(id, _)| *id == feature_id) {
                    Some(stages.len().saturating_sub(1))
                } else {
                    None
                };
                state.yang_debug_captures.insert(
                    feature_id.to_string(),
                    kernel::FeatureStageCapture {
                        feature_id: feature_id.to_string(),
                        stages,
                        failed_at_stage: failed_at,
                    },
                );
            }
            result?;
            Ok(model_updated_response(state))
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
                ProjectMetadata::new(&state.project_name).with_display_unit(&state.display_unit);
            let json = file_format::save_project(&state.engine.tree, &meta);
            Ok(EngineToUi::SaveReady { json_data: json })
        }

        UiToEngine::LoadProject { data } => {
            let (tree, meta) =
                file_format::load_project(&data).map_err(|e| BridgeError::Serialization {
                    reason: e.to_string(),
                })?;
            state.project_name = meta.name;
            if let Some(ref unit) = meta.display_unit {
                state.display_unit = unit.clone();
            }
            state.engine.tree = tree;
            state.engine.rebuild_from_scratch(kb);
            Ok(model_updated_response(state))
        }

        // -- Tab / document management --
        UiToEngine::SwitchTab { features } => {
            state.active_sketch = None;
            state.selection.clear();
            state.hover = None;
            state.engine.tree = features;
            state.engine.rebuild_from_scratch(kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::NewDocument => {
            state.reset();
            Ok(model_updated_response(state))
        }

        // -- Settings --
        UiToEngine::SetDisplayUnit { unit } => {
            state.display_unit = unit;
            Ok(model_updated_response(state))
        }

        UiToEngine::ExportStep => {
            let handle = find_last_solid_handle(state);
            match handle {
                Some(handle) => {
                    let step_data = kb.export_step(&handle, "waffle_export.step").map_err(|e| {
                        BridgeError::Engine(feature_engine::types::EngineError::RebuildFailed {
                            feature_name: "STEP export".to_string(),
                            reason: format!("{}", e),
                        })
                    })?;
                    Ok(EngineToUi::ExportReady { step_data })
                }
                None => Err(BridgeError::NoMeshData),
            }
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

        UiToEngine::ExportStl => {
            let mesh = find_last_mesh(state);
            match mesh {
                Some(mesh) => {
                    let bytes = crate::stl_export::render_mesh_to_stl(&mesh);
                    let stl_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    Ok(EngineToUi::StlExportReady { stl_data })
                }
                None => Err(BridgeError::NoMeshData),
            }
        }
    }
}

/// Build a ModelUpdated response from the current engine state.
fn model_updated_response(state: &EngineState) -> EngineToUi {
    // Generate preview mesh from the last active mesh (if any)
    let preview_mesh = find_last_mesh(state).and_then(|mesh| {
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
    });

    EngineToUi::ModelUpdated {
        feature_tree: state.engine.tree.clone(),
        meshes: Vec::new(),
        edges: Vec::new(),
        errors: state.engine.errors.clone(),
        warnings: state.engine.warnings.clone(),
        preview_mesh,
    }
}

/// Find the last active feature's solid handle by iterating features in reverse.
fn find_last_solid_handle(state: &EngineState) -> Option<kernel::KernelSolidHandle> {
    let tree = &state.engine.tree;
    let limit = tree.active_index.unwrap_or(tree.features.len());
    for feature in tree.features[..limit].iter().rev() {
        if feature.suppressed {
            continue;
        }
        if let Some(result) = state.engine.feature_results.get(&feature.id) {
            for (key, body) in &result.outputs {
                if *key == OutputKey::Main {
                    return Some(body.handle.clone());
                }
            }
        }
    }
    None
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

/// Derive a human-readable feature name from an operation.
fn operation_name(op: &Operation) -> String {
    match op {
        Operation::Sketch { .. } => "Sketch".to_string(),
        Operation::Extrude { .. } => "Extrude".to_string(),
        Operation::Revolve { .. } => "Revolve".to_string(),
        Operation::Fillet { .. } => "Fillet".to_string(),
        Operation::Chamfer { .. } => "Chamfer".to_string(),
        Operation::Shell { .. } => "Shell".to_string(),
        Operation::BooleanCombine { .. } => "Boolean Combine".to_string(),
        Operation::DatumPlane { params } => params.name.clone(),
    }
}
