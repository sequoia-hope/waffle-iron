use feature_engine::types::*;
use kernel_fork::MockKernel;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

// ── Helper functions ─────────────────────────────────────────────────────

fn make_sketch_op() -> Operation {
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::Datum {
                datum_id: Uuid::new_v4(),
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
        },
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 1.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 1.0,
                construction: false,
            },
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
    };
    Operation::Sketch { sketch }
}

fn make_extrude_op(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: false,
            target_body: None,
        },
    }
}

fn make_geom_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::new_v4(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    }
}

// ── Serde Round-Trip Tests ───────────────────────────────────────────────

#[test]
fn serde_roundtrip_add_feature() {
    let msg = UiToEngine::AddFeature {
        operation: make_sketch_op(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: UiToEngine = serde_json::from_str(&json).unwrap();
    // Verify the type tag is present
    assert!(json.contains("\"type\":\"AddFeature\""));
    assert!(matches!(deserialized, UiToEngine::AddFeature { .. }));
}

#[test]
fn serde_roundtrip_edit_feature() {
    let msg = UiToEngine::EditFeature {
        feature_id: Uuid::new_v4(),
        operation: make_extrude_op(Uuid::new_v4()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(json.contains("\"type\":\"EditFeature\""));
    assert!(matches!(deserialized, UiToEngine::EditFeature { .. }));
}

#[test]
fn serde_roundtrip_delete_feature() {
    let id = Uuid::new_v4();
    let msg = UiToEngine::DeleteFeature { feature_id: id };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        deserialized,
        UiToEngine::DeleteFeature { feature_id } if feature_id == id
    ));
}

#[test]
fn serde_roundtrip_select_entity() {
    let msg = UiToEngine::SelectEntity {
        geom_ref: make_geom_ref(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, UiToEngine::SelectEntity { .. }));
}

#[test]
fn serde_roundtrip_engine_error() {
    let msg = EngineToUi::Error {
        message: "something went wrong".to_string(),
        feature_id: Some(Uuid::new_v4()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: EngineToUi = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, EngineToUi::Error { .. }));
}

#[test]
fn serde_roundtrip_model_updated() {
    let msg = EngineToUi::ModelUpdated {
        feature_tree: FeatureTree::new(),
        meshes: Vec::new(),
        edges: Vec::new(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: EngineToUi = serde_json::from_str(&json).unwrap();
    assert!(json.contains("\"type\":\"ModelUpdated\""));
    assert!(matches!(deserialized, EngineToUi::ModelUpdated { .. }));
}

#[test]
fn serde_roundtrip_suppress_and_rollback() {
    let suppress = UiToEngine::SuppressFeature {
        feature_id: Uuid::new_v4(),
        suppressed: true,
    };
    let rollback = UiToEngine::SetRollbackIndex { index: Some(2) };

    let json_s = serde_json::to_string(&suppress).unwrap();
    let json_r = serde_json::to_string(&rollback).unwrap();

    let ds: UiToEngine = serde_json::from_str(&json_s).unwrap();
    let dr: UiToEngine = serde_json::from_str(&json_r).unwrap();

    assert!(matches!(
        ds,
        UiToEngine::SuppressFeature {
            suppressed: true,
            ..
        }
    ));
    assert!(matches!(
        dr,
        UiToEngine::SetRollbackIndex { index: Some(2) }
    ));
}

// ── Dispatch Tests ───────────────────────────────────────────────────────

#[test]
fn dispatch_add_feature_returns_model_updated() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let msg = UiToEngine::AddFeature {
        operation: make_sketch_op(),
    };
    let response = wasm_bridge::dispatch(&mut state, msg, &mut kernel);

    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    if let EngineToUi::ModelUpdated { feature_tree, .. } = &response {
        assert_eq!(feature_tree.features.len(), 1);
    }
}

#[test]
fn dispatch_select_entity_returns_selection_changed() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let msg = UiToEngine::SelectEntity {
        geom_ref: make_geom_ref(),
    };
    let response = wasm_bridge::dispatch(&mut state, msg, &mut kernel);

    assert!(matches!(response, EngineToUi::SelectionChanged { .. }));
    assert_eq!(state.selection.len(), 1);
}

#[test]
fn dispatch_hover_entity_returns_hover_changed() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let msg = UiToEngine::HoverEntity {
        geom_ref: Some(make_geom_ref()),
    };
    let response = wasm_bridge::dispatch(&mut state, msg, &mut kernel);

    assert!(matches!(response, EngineToUi::HoverChanged { .. }));
    assert!(state.hover.is_some());
}

#[test]
fn dispatch_undo_empty_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(&mut state, UiToEngine::Undo, &mut kernel);

    assert!(matches!(response, EngineToUi::Error { .. }));
    if let EngineToUi::Error { message, .. } = &response {
        assert!(
            message.contains("undo"),
            "Expected 'undo' error, got: {}",
            message
        );
    }
}

#[test]
fn dispatch_unimplemented_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(&mut state, UiToEngine::ExportStep, &mut kernel);

    assert!(matches!(response, EngineToUi::Error { .. }));
    if let EngineToUi::Error { message, .. } = &response {
        assert!(message.contains("not implemented") || message.contains("Not"));
    }
}

#[test]
fn dispatch_delete_nonexistent_feature_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let msg = UiToEngine::DeleteFeature {
        feature_id: Uuid::new_v4(),
    };
    let response = wasm_bridge::dispatch(&mut state, msg, &mut kernel);

    assert!(matches!(response, EngineToUi::Error { .. }));
}

// ── Engine State Tests ───────────────────────────────────────────────────

#[test]
fn engine_state_sketch_workflow() {
    let mut state = EngineState::new();

    // No active sketch initially
    assert!(state.active_sketch.is_none());

    // Begin sketch
    state.begin_sketch(make_geom_ref());
    assert!(state.active_sketch.is_some());

    // Add entity
    state
        .add_sketch_entity(SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        })
        .unwrap();

    // Add constraint
    state
        .add_sketch_constraint(SketchConstraint::Horizontal { entity: 1 })
        .unwrap();

    // Finish sketch
    let sketch = state.finish_sketch().unwrap();
    assert_eq!(sketch.entities.len(), 1);
    assert_eq!(sketch.constraints.len(), 1);
    assert!(state.active_sketch.is_none());
}

#[test]
fn engine_state_no_sketch_errors() {
    let mut state = EngineState::new();

    let result = state.add_sketch_entity(SketchEntity::Point {
        id: 1,
        x: 0.0,
        y: 0.0,
        construction: false,
    });
    assert!(result.is_err());

    let result = state.finish_sketch();
    assert!(result.is_err());
}

// ── Undo/Redo Dispatch Tests ──────────────────────────────────────────

#[test]
fn dispatch_undo_redo_cycle() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Add a feature via dispatch
    let op = make_sketch_operation();
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature { operation: op },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.features.len(), 1);

    // Undo
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.features.len(), 0);

    // Redo
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.features.len(), 1);
}

// ── Save/Load Dispatch Tests ──────────────────────────────────────────

#[test]
fn dispatch_save_produces_json() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Add a feature
    let op = make_sketch_operation();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature { operation: op },
        &mut kernel,
    );

    // Save
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::SaveProject, &mut kernel);

    if let EngineToUi::SaveReady { json_data } = response {
        assert!(json_data.contains("waffle-iron"));
        assert!(json_data.contains("Sketch"));
    } else {
        panic!("Expected SaveReady, got {:?}", response);
    }
}

#[test]
fn dispatch_load_restores_tree() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Add a feature and save
    let op = make_sketch_operation();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature { operation: op },
        &mut kernel,
    );

    let save_response = wasm_bridge::dispatch(&mut state, UiToEngine::SaveProject, &mut kernel);
    let json_data = if let EngineToUi::SaveReady { json_data } = save_response {
        json_data
    } else {
        panic!("Expected SaveReady");
    };

    // Clear state
    let mut new_state = EngineState::new();
    assert_eq!(new_state.engine.tree.features.len(), 0);

    // Load
    let response = wasm_bridge::dispatch(
        &mut new_state,
        UiToEngine::LoadProject { data: json_data },
        &mut kernel,
    );

    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(new_state.engine.tree.features.len(), 1);
}

/// Helper: create a minimal sketch operation for dispatch tests.
fn make_sketch_operation() -> Operation {
    use waffle_types::Sketch;
    Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: make_geom_ref(),
            entities: Vec::new(),
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
        },
    }
}
