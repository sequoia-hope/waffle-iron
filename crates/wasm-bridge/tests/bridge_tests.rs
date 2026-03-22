use feature_engine::types::*;
use kernel::MockKernel;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

// ── Helper functions ─────────────────────────────────────────────────────

fn make_sketch_op() -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(PointId(1), (0.0, 0.0));
    solved_positions.insert(PointId(2), (1.0, 0.0));
    solved_positions.insert(PointId(3), (1.0, 1.0));
    solved_positions.insert(PointId(4), (0.0, 1.0));

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
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities: vec![
            SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(2),
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(3),
                x: 1.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Point {
                id: PointId(4),
                x: 0.0,
                y: 1.0,
                construction: false,
            },
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![EntityId(1), EntityId(2), EntityId(3), EntityId(4)],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
        }],
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
            merge: true,
            target_body: None,
            depth_mode: feature_engine::types::DepthMode::Blind,
            second_direction: None,
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
        errors: Vec::new(),
        warnings: Vec::new(),
        preview_mesh: None,
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
fn dispatch_export_step_no_features_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(&mut state, UiToEngine::ExportStep, &mut kernel);

    assert!(matches!(response, EngineToUi::Error { .. }));
    if let EngineToUi::Error { message, .. } = &response {
        assert!(
            message.contains("mesh") || message.contains("data"),
            "Expected 'no mesh data' error, got: {}",
            message
        );
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
            id: PointId(1),
            x: 0.0,
            y: 0.0,
            construction: false,
        })
        .unwrap();

    // Add constraint
    state
        .add_sketch_constraint(SketchConstraint::Horizontal {
            entity: EntityId(1),
        })
        .unwrap();

    // Finish sketch
    let sketch = state
        .finish_sketch(
            std::collections::HashMap::new(),
            Vec::new(),
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            vec![],
            vec![],
        )
        .unwrap();
    assert_eq!(sketch.entities.len(), 1);
    assert_eq!(sketch.constraints.len(), 1);
    assert!(state.active_sketch.is_none());
}

#[test]
fn engine_state_no_sketch_errors() {
    let mut state = EngineState::new();

    let result = state.add_sketch_entity(SketchEntity::Point {
        id: PointId(1),
        x: 0.0,
        y: 0.0,
        construction: false,
    });
    assert!(result.is_err());

    let result = state.finish_sketch(
        std::collections::HashMap::new(),
        Vec::new(),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        vec![],
        vec![],
    );
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

// ── Sketch Workflow Dispatch Tests ────────────────────────────────────

#[test]
fn dispatch_solve_sketch_returns_solved() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Begin sketch
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: make_geom_ref(),
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));

    // Add a rectangle: 4 points + 4 lines
    for (id, x, y) in [
        (PointId(1), 0.0, 0.0),
        (PointId(2), 10.0, 0.0),
        (PointId(3), 10.0, 10.0),
        (PointId(4), 0.0, 10.0),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Point {
                    id,
                    x,
                    y,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }
    for (id, start, end) in [
        (LineId(10), PointId(1), PointId(2)),
        (LineId(11), PointId(2), PointId(3)),
        (LineId(12), PointId(3), PointId(4)),
        (LineId(13), PointId(4), PointId(1)),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Line {
                    id,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }

    // Add constraints: pin origin, fix width/height
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Dragged { point: PointId(1) },
        },
        &mut kernel,
    );
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
        },
        &mut kernel,
    );
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Vertical {
                entity: EntityId(11),
            },
        },
        &mut kernel,
    );

    // Solve
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::SolveSketch, &mut kernel);
    if let EngineToUi::SketchSolved { solved } = &response {
        // Should have positions for all 4 points
        assert_eq!(solved.positions.len(), 4);
        // Origin should be at (0,0)
        let origin = solved.positions.get(&PointId(1)).unwrap();
        assert!((origin.0).abs() < 1e-6);
        assert!((origin.1).abs() < 1e-6);
    } else {
        panic!("Expected SketchSolved, got {:?}", response);
    }
}

#[test]
fn dispatch_solve_without_sketch_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(&mut state, UiToEngine::SolveSketch, &mut kernel);
    assert!(matches!(response, EngineToUi::Error { .. }));
}

#[test]
fn dispatch_full_sketch_workflow() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Begin → Add entities → Finish → verify feature added
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: make_geom_ref(),
        },
        &mut kernel,
    );

    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddSketchEntity {
            entity: SketchEntity::Point {
                id: PointId(1),
                x: 0.0,
                y: 0.0,
                construction: false,
            },
        },
        &mut kernel,
    );

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions: std::collections::HashMap::new(),
            solved_profiles: Vec::new(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![],
            constraints: vec![],
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    if let EngineToUi::ModelUpdated { feature_tree, .. } = &response {
        assert_eq!(feature_tree.features.len(), 1);
    }

    // No active sketch after finish
    assert!(state.active_sketch.is_none());
}

// ── Sketch → Extrude Integration Test ─────────────────────────────────

#[test]
fn dispatch_sketch_then_extrude_produces_solid() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Begin sketch
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: make_geom_ref(),
        },
        &mut kernel,
    );

    // Add a rectangle: 4 points + 4 lines
    for (id, x, y) in [
        (PointId(1), 0.0, 0.0),
        (PointId(2), 10.0, 0.0),
        (PointId(3), 10.0, 10.0),
        (PointId(4), 0.0, 10.0),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Point {
                    id,
                    x,
                    y,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }
    for (id, start, end) in [
        (LineId(10), PointId(1), PointId(2)),
        (LineId(11), PointId(2), PointId(3)),
        (LineId(12), PointId(3), PointId(4)),
        (LineId(13), PointId(4), PointId(1)),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Line {
                    id,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }

    // FinishSketch with solved positions and a profile
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(PointId(1), (0.0, 0.0));
    solved_positions.insert(PointId(2), (10.0, 0.0));
    solved_positions.insert(PointId(3), (10.0, 10.0));
    solved_positions.insert(PointId(4), (0.0, 10.0));

    let solved_profiles = vec![ClosedProfile {
        entity_ids: vec![EntityId(1), EntityId(2), EntityId(3), EntityId(4)],
        is_outer: true,
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
    }];

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions,
            solved_profiles,
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![],
            constraints: vec![],
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    let sketch_id = if let EngineToUi::ModelUpdated { feature_tree, .. } = &response {
        assert_eq!(feature_tree.features.len(), 1);
        feature_tree.features[0].id
    } else {
        panic!("Expected ModelUpdated");
    };

    // Now extrude
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: make_extrude_op(sketch_id),
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    if let EngineToUi::ModelUpdated { feature_tree, .. } = &response {
        assert_eq!(feature_tree.features.len(), 2);
        assert_eq!(feature_tree.features[1].name, "Extrude");
    }
}

// ── STL Export Tests ────────────────────────────────────────────────────

#[test]
fn serde_roundtrip_export_stl() {
    let msg = UiToEngine::ExportStl;
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"ExportStl\""));
    let deserialized: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, UiToEngine::ExportStl));

    let response = EngineToUi::StlExportReady {
        stl_data: "AAAA".to_string(),
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"type\":\"StlExportReady\""));
    let deserialized: EngineToUi = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, EngineToUi::StlExportReady { .. }));
}

#[test]
fn dispatch_export_stl_no_features() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(&mut state, UiToEngine::ExportStl, &mut kernel);

    assert!(matches!(response, EngineToUi::Error { .. }));
    if let EngineToUi::Error { message, .. } = &response {
        assert!(
            message.contains("mesh"),
            "Expected 'mesh' error, got: {}",
            message
        );
    }
}

// ── GAP W3: Solve precision for all 4 corners ─────────────────────────

#[test]
fn dispatch_solve_sketch_checks_all_4_corners() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Begin sketch
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: make_geom_ref(),
        },
        &mut kernel,
    );

    // Add a rectangle: 4 points + 4 lines (10x10)
    for (id, x, y) in [
        (PointId(1), 0.0, 0.0),
        (PointId(2), 10.0, 0.0),
        (PointId(3), 10.0, 10.0),
        (PointId(4), 0.0, 10.0),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Point {
                    id,
                    x,
                    y,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }
    for (id, start, end) in [
        (LineId(10), PointId(1), PointId(2)),
        (LineId(11), PointId(2), PointId(3)),
        (LineId(12), PointId(3), PointId(4)),
        (LineId(13), PointId(4), PointId(1)),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Line {
                    id,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }

    // Constraints: pin origin, fix horizontal/vertical edges
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Dragged { point: PointId(1) },
        },
        &mut kernel,
    );
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Horizontal {
                entity: EntityId(10),
            },
        },
        &mut kernel,
    );
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Vertical {
                entity: EntityId(11),
            },
        },
        &mut kernel,
    );

    // Solve
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::SolveSketch, &mut kernel);
    if let EngineToUi::SketchSolved { solved } = &response {
        assert_eq!(solved.positions.len(), 4, "Should have 4 solved positions");

        // Check ALL 4 corners with tolerance
        let eps = 1e-4;
        let expected = [
            (PointId(1), 0.0, 0.0),
            (PointId(2), 10.0, 0.0),
            (PointId(3), 10.0, 10.0),
            (PointId(4), 0.0, 10.0),
        ];
        for (id, ex, ey) in expected {
            let pos = solved.positions.get(&id).unwrap_or_else(|| {
                panic!("Missing solved position for point {:?}", id);
            });
            assert!(
                (pos.0 - ex).abs() < eps && (pos.1 - ey).abs() < eps,
                "Point {:?} expected ({}, {}), got ({}, {})",
                id,
                ex,
                ey,
                pos.0,
                pos.1,
            );
        }
    } else {
        panic!("Expected SketchSolved, got {:?}", response);
    }
}

// ── GAP W5: ExportStep with valid solid ────────────────────────────────

#[test]
fn dispatch_export_step_with_solid_reaches_kernel() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Build sketch + extrude via dispatch (same as sketch_then_extrude test)
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: make_geom_ref(),
        },
        &mut kernel,
    );
    for (id, x, y) in [
        (PointId(1), 0.0, 0.0),
        (PointId(2), 10.0, 0.0),
        (PointId(3), 10.0, 10.0),
        (PointId(4), 0.0, 10.0),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Point {
                    id,
                    x,
                    y,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }
    for (id, start, end) in [
        (LineId(10), PointId(1), PointId(2)),
        (LineId(11), PointId(2), PointId(3)),
        (LineId(12), PointId(3), PointId(4)),
        (LineId(13), PointId(4), PointId(1)),
    ] {
        wasm_bridge::dispatch(
            &mut state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Line {
                    id,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            &mut kernel,
        );
    }
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(PointId(1), (0.0, 0.0));
    solved_positions.insert(PointId(2), (10.0, 0.0));
    solved_positions.insert(PointId(3), (10.0, 10.0));
    solved_positions.insert(PointId(4), (0.0, 10.0));
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions,
            solved_profiles: vec![waffle_types::ClosedProfile {
                entity_ids: vec![EntityId(1), EntityId(2), EntityId(3), EntityId(4)],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
            }],
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![],
            constraints: vec![],
        },
        &mut kernel,
    );
    let sketch_id = match &response {
        EngineToUi::ModelUpdated { feature_tree, .. } => feature_tree.features[0].id,
        other => panic!("Expected ModelUpdated, got {:?}", other),
    };
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id,
                    profile_index: 0,
                    depth: 5.0,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: feature_engine::types::DepthMode::Blind,
                    second_direction: None,
                },
            },
        },
        &mut kernel,
    );

    // ExportStep should find the solid handle but MockKernel's export_step
    // returns NotSupported (default trait impl). Verify we get a kernel error
    // rather than a "no mesh data" error — proving dispatch found the solid.
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::ExportStep, &mut kernel);
    match &response {
        EngineToUi::Error { message, .. } => {
            assert!(
                message.contains("not supported") || message.contains("STEP"),
                "Expected kernel 'not supported' error, got: {}",
                message
            );
            // Crucially, this should NOT be the NoMeshData error
            assert!(
                !message.contains("No mesh data"),
                "Should not be 'No mesh data' — solid handle should have been found"
            );
        }
        EngineToUi::ExportReady { .. } => {
            // If MockKernel ever gets STEP export, this is also fine
        }
        other => panic!("Expected Error or ExportReady, got {:?}", other),
    }
}

// ── GAP W6: RenameFeature nonexistent ID ───────────────────────────────

#[test]
fn dispatch_rename_nonexistent_feature_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::RenameFeature {
            feature_id: Uuid::new_v4(),
            new_name: "Ghost".to_string(),
        },
        &mut kernel,
    );

    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "RenameFeature with nonexistent ID should return Error, got {:?}",
        response
    );
}

// ── GAP W7: HoverEntity with None ──────────────────────────────────────

#[test]
fn dispatch_hover_entity_none_clears_hover() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // First, set a hover
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::HoverEntity {
            geom_ref: Some(make_geom_ref()),
        },
        &mut kernel,
    );
    assert!(state.hover.is_some(), "Hover should be set");

    // Now clear hover with None
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::HoverEntity { geom_ref: None },
        &mut kernel,
    );

    assert!(
        matches!(response, EngineToUi::HoverChanged { geom_ref: None }),
        "HoverEntity(None) should return HoverChanged with None, got {:?}",
        response
    );
    assert!(
        state.hover.is_none(),
        "State hover should be cleared after HoverEntity(None)"
    );
}

// ── Serde Round-Trip Tests: remaining UiToEngine variants ───────────

#[test]
fn serde_roundtrip_begin_sketch() {
    let msg = UiToEngine::BeginSketch {
        plane: make_geom_ref(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"BeginSketch\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, UiToEngine::BeginSketch { .. }));
}

#[test]
fn serde_roundtrip_add_sketch_entity() {
    let msg = UiToEngine::AddSketchEntity {
        entity: SketchEntity::Circle {
            id: CircleId(5),
            center_id: PointId(1),
            radius: 7.5,
            construction: true,
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"AddSketchEntity\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, UiToEngine::AddSketchEntity { .. }));
}

#[test]
fn serde_roundtrip_add_constraint() {
    let msg = UiToEngine::AddConstraint {
        constraint: SketchConstraint::Distance {
            entity_a: EntityId(1),
            entity_b: EntityId(2),
            value: 42.5,
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"AddConstraint\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, UiToEngine::AddConstraint { .. }));
}

#[test]
fn serde_roundtrip_solve_sketch() {
    let msg = UiToEngine::SolveSketch;
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"SolveSketch\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, UiToEngine::SolveSketch));
}

#[test]
fn serde_roundtrip_finish_sketch() {
    let mut positions = std::collections::HashMap::new();
    positions.insert(PointId(1), (0.0, 0.0));
    positions.insert(PointId(2), (10.0, 5.0));
    let msg = UiToEngine::FinishSketch {
        solved_positions: positions.clone(),
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![EntityId(1), EntityId(2), EntityId(3)],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
        }],
        plane_origin: [1.0, 2.0, 3.0],
        plane_normal: [0.0, 1.0, 0.0],
        entities: vec![],
        constraints: vec![],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FinishSketch\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    if let UiToEngine::FinishSketch {
        solved_positions,
        solved_profiles,
        plane_origin,
        plane_normal,
        ..
    } = d
    {
        assert_eq!(solved_positions.len(), 2);
        assert_eq!(solved_positions[&PointId(1)], (0.0, 0.0));
        assert_eq!(solved_positions[&PointId(2)], (10.0, 5.0));
        assert_eq!(solved_profiles.len(), 1);
        assert_eq!(plane_origin, [1.0, 2.0, 3.0]);
        assert_eq!(plane_normal, [0.0, 1.0, 0.0]);
    } else {
        panic!("Expected FinishSketch");
    }
}

#[test]
fn serde_roundtrip_finish_sketch_defaults() {
    // Deserialize without optional fields to test default_origin/default_normal
    let json = r#"{"type":"FinishSketch"}"#;
    let d: UiToEngine = serde_json::from_str(json).unwrap();
    if let UiToEngine::FinishSketch {
        solved_positions,
        solved_profiles,
        plane_origin,
        plane_normal,
        entities,
        constraints,
    } = d
    {
        assert!(solved_positions.is_empty());
        assert!(solved_profiles.is_empty());
        assert_eq!(plane_origin, [0.0, 0.0, 0.0]);
        assert_eq!(plane_normal, [0.0, 0.0, 1.0]);
        assert!(entities.is_empty());
        assert!(constraints.is_empty());
    } else {
        panic!("Expected FinishSketch");
    }
}

#[test]
fn serde_roundtrip_reorder_feature() {
    let msg = UiToEngine::ReorderFeature {
        feature_id: Uuid::new_v4(),
        new_position: 3,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"ReorderFeature\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        d,
        UiToEngine::ReorderFeature {
            new_position: 3,
            ..
        }
    ));
}

#[test]
fn serde_roundtrip_rename_feature() {
    let msg = UiToEngine::RenameFeature {
        feature_id: Uuid::new_v4(),
        new_name: "My Feature".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"RenameFeature\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    if let UiToEngine::RenameFeature { new_name, .. } = d {
        assert_eq!(new_name, "My Feature");
    } else {
        panic!("Expected RenameFeature");
    }
}

#[test]
fn serde_roundtrip_undo_redo() {
    let undo = UiToEngine::Undo;
    let redo = UiToEngine::Redo;
    let ju = serde_json::to_string(&undo).unwrap();
    let jr = serde_json::to_string(&redo).unwrap();
    assert!(ju.contains("\"type\":\"Undo\""));
    assert!(jr.contains("\"type\":\"Redo\""));
    let du: UiToEngine = serde_json::from_str(&ju).unwrap();
    let dr: UiToEngine = serde_json::from_str(&jr).unwrap();
    assert!(matches!(du, UiToEngine::Undo));
    assert!(matches!(dr, UiToEngine::Redo));
}

#[test]
fn serde_roundtrip_hover_entity() {
    let msg = UiToEngine::HoverEntity {
        geom_ref: Some(make_geom_ref()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"HoverEntity\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, UiToEngine::HoverEntity { geom_ref: Some(_) }));

    let msg_none = UiToEngine::HoverEntity { geom_ref: None };
    let json_none = serde_json::to_string(&msg_none).unwrap();
    let d_none: UiToEngine = serde_json::from_str(&json_none).unwrap();
    assert!(matches!(d_none, UiToEngine::HoverEntity { geom_ref: None }));
}

#[test]
fn serde_roundtrip_save_load_export() {
    let save = UiToEngine::SaveProject;
    let load = UiToEngine::LoadProject {
        data: r#"{"test": true}"#.to_string(),
    };
    let export = UiToEngine::ExportStep;

    for msg in [save, export] {
        let json = serde_json::to_string(&msg).unwrap();
        let _d: UiToEngine = serde_json::from_str(&json).unwrap();
    }

    let json = serde_json::to_string(&load).unwrap();
    assert!(json.contains("\"type\":\"LoadProject\""));
    let d: UiToEngine = serde_json::from_str(&json).unwrap();
    if let UiToEngine::LoadProject { data } = d {
        assert!(data.contains("test"));
    } else {
        panic!("Expected LoadProject");
    }
}

// ── Serde Round-Trip Tests: remaining EngineToUi variants ───────────

#[test]
fn serde_roundtrip_sketch_solved() {
    // Note: SolvedSketch.positions uses default HashMap serde (not u32_key_map),
    // so non-empty HashMap<u32, _> can't roundtrip via JSON. Test with empty positions.
    let msg = EngineToUi::SketchSolved {
        solved: SolvedSketch {
            positions: std::collections::HashMap::new(),
            radii: std::collections::HashMap::new(),
            profiles: vec![ClosedProfile {
                entity_ids: vec![EntityId(1), EntityId(2)],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
            }],
            status: SolveStatus::UnderConstrained { dof: 2 },
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"SketchSolved\""));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, EngineToUi::SketchSolved { .. }));
}

#[test]
fn serde_roundtrip_hover_changed() {
    let msg = EngineToUi::HoverChanged {
        geom_ref: Some(make_geom_ref()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"HoverChanged\""));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, EngineToUi::HoverChanged { geom_ref: Some(_) }));
}

#[test]
fn serde_roundtrip_selection_changed() {
    let msg = EngineToUi::SelectionChanged {
        geom_refs: vec![make_geom_ref(), make_geom_ref()],
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"SelectionChanged\""));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    if let EngineToUi::SelectionChanged { geom_refs } = d {
        assert_eq!(geom_refs.len(), 2);
    } else {
        panic!("Expected SelectionChanged");
    }
}

#[test]
fn serde_roundtrip_save_ready() {
    let msg = EngineToUi::SaveReady {
        json_data: r#"{"format":"waffle-iron"}"#.to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"SaveReady\""));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    if let EngineToUi::SaveReady { json_data } = d {
        assert!(json_data.contains("waffle-iron"));
    } else {
        panic!("Expected SaveReady");
    }
}

#[test]
fn serde_roundtrip_project_loaded() {
    let msg = EngineToUi::ProjectLoaded {
        feature_tree: FeatureTree::new(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"ProjectLoaded\""));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    assert!(matches!(d, EngineToUi::ProjectLoaded { .. }));
}

#[test]
fn serde_roundtrip_export_ready() {
    let msg = EngineToUi::ExportReady {
        step_data: "ISO-10303-21;".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"ExportReady\""));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    if let EngineToUi::ExportReady { step_data } = d {
        assert!(step_data.contains("ISO"));
    } else {
        panic!("Expected ExportReady");
    }
}

#[test]
fn serde_roundtrip_model_updated_with_errors() {
    let msg = EngineToUi::ModelUpdated {
        feature_tree: FeatureTree::new(),
        meshes: Vec::new(),
        edges: Vec::new(),
        errors: vec![(Uuid::new_v4(), "rebuild failed".to_string())],
        warnings: Vec::new(),
        preview_mesh: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("rebuild failed"));
    let d: EngineToUi = serde_json::from_str(&json).unwrap();
    if let EngineToUi::ModelUpdated { errors, .. } = d {
        assert_eq!(errors.len(), 1);
        assert!(errors[0].1.contains("rebuild"));
    } else {
        panic!("Expected ModelUpdated");
    }
}

#[test]
fn serde_model_updated_empty_errors_skipped() {
    let msg = EngineToUi::ModelUpdated {
        feature_tree: FeatureTree::new(),
        meshes: Vec::new(),
        edges: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
        preview_mesh: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    // skip_serializing_if = "Vec::is_empty" should omit the errors field
    assert!(
        !json.contains("\"errors\""),
        "Empty errors should be skipped in serialization"
    );
}

// ── Dispatch: Reorder & SetRollback ─────────────────────────────────

#[test]
fn dispatch_reorder_nonexistent_feature_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::ReorderFeature {
            feature_id: Uuid::new_v4(),
            new_position: 0,
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "Reorder nonexistent feature should error, got {:?}",
        response
    );
}

#[test]
fn dispatch_set_rollback_index() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Add a feature
    let op = make_sketch_operation();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature { operation: op },
        &mut kernel,
    );

    // Set rollback to 0 (before all features)
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SetRollbackIndex { index: Some(0) },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.active_index, Some(0));

    // Clear rollback
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SetRollbackIndex { index: None },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.active_index, None);
}

#[test]
fn dispatch_redo_empty_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "Redo on empty state should error, got {:?}",
        response
    );
}

#[test]
fn dispatch_suppress_nonexistent_feature_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SuppressFeature {
            feature_id: Uuid::new_v4(),
            suppressed: true,
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "Suppress nonexistent feature should error, got {:?}",
        response
    );
}

#[test]
fn dispatch_edit_nonexistent_feature_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: Uuid::new_v4(),
            operation: make_sketch_operation(),
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "Edit nonexistent feature should error, got {:?}",
        response
    );
}

#[test]
fn dispatch_load_invalid_json_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::LoadProject {
            data: "not valid json".to_string(),
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "Load invalid JSON should error, got {:?}",
        response
    );
}

#[test]
fn dispatch_add_constraint_without_sketch_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddConstraint {
            constraint: SketchConstraint::Horizontal {
                entity: EntityId(1),
            },
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "AddConstraint without active sketch should error"
    );
}

/// Helper: create a minimal sketch operation for dispatch tests.
fn make_sketch_operation() -> Operation {
    use waffle_types::Sketch;
    Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: make_geom_ref(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: Vec::new(),
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: std::collections::HashMap::new(),
            solved_profiles: Vec::new(),
        },
    }
}
