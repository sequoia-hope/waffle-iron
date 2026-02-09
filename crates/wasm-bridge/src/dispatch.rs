use feature_engine::types::Operation;
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
            // Solver integration would go here
            Err(BridgeError::NotImplemented {
                operation: "SolveSketch".to_string(),
            })
        }

        UiToEngine::FinishSketch => {
            let sketch = state.finish_sketch()?;
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

        UiToEngine::SetRollbackIndex { index } => {
            state.engine.set_rollback(index, kb);
            Ok(model_updated_response(state))
        }

        // -- History --
        UiToEngine::Undo => Err(BridgeError::NotImplemented {
            operation: "Undo".to_string(),
        }),

        UiToEngine::Redo => Err(BridgeError::NotImplemented {
            operation: "Redo".to_string(),
        }),

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
        UiToEngine::SaveProject => Err(BridgeError::NotImplemented {
            operation: "SaveProject".to_string(),
        }),

        UiToEngine::LoadProject { .. } => Err(BridgeError::NotImplemented {
            operation: "LoadProject".to_string(),
        }),

        UiToEngine::ExportStep => Err(BridgeError::NotImplemented {
            operation: "ExportStep".to_string(),
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
