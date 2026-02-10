use feature_engine::types::Operation;
use file_format::ProjectMetadata;
use modeling_ops::KernelBundle;

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
        } => {
            let sketch = state.finish_sketch(
                solved_positions,
                solved_profiles,
                plane_origin,
                plane_normal,
            )?;
            let op = Operation::Sketch { sketch };
            state.engine.add_feature("Sketch".to_string(), op, kb)?;
            Ok(model_updated_response(state))
        }

        // -- Feature operations --
        UiToEngine::AddFeature { operation } => {
            let name = operation_name(&operation);
            state.engine.add_feature(name, operation, kb)?;
            Ok(model_updated_response(state))
        }

        UiToEngine::EditFeature {
            feature_id,
            operation,
        } => {
            state.engine.edit_feature(feature_id, operation, kb)?;
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
            let meta = ProjectMetadata::new(&state.project_name);
            let json = file_format::save_project(&state.engine.tree, &meta);
            Ok(EngineToUi::SaveReady { json_data: json })
        }

        UiToEngine::LoadProject { data } => {
            let (tree, meta) =
                file_format::load_project(&data).map_err(|e| BridgeError::Serialization {
                    reason: e.to_string(),
                })?;
            state.project_name = meta.name;
            state.engine.tree = tree;
            state.engine.rebuild_from_scratch(kb);
            Ok(model_updated_response(state))
        }

        UiToEngine::ExportStep => Err(BridgeError::NotImplemented {
            operation: "ExportStep (requires TruckKernel)".to_string(),
        }),
    }
}

/// Build a ModelUpdated response from the current engine state.
fn model_updated_response(state: &EngineState) -> EngineToUi {
    EngineToUi::ModelUpdated {
        feature_tree: state.engine.tree.clone(),
        meshes: Vec::new(),
        edges: Vec::new(),
    }
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
    }
}
