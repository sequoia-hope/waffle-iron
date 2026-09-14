//! ICR-4 of `specs/waffle_mcp_server.md`: `AddFeature`, `EditFeature` and
//! `FinishSketch` carry an optional `provenance`, recorded inside the same
//! undo step, and the `ModelUpdated` answer names the feature the command
//! created or edited — so an agent host never has to diff trees to learn an
//! id, and one undo of its call restores the tree exactly (I3/I4/I5).

use feature_engine::types::*;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

fn cube_op(x: f64) -> Operation {
    let mut params = ImportedBodyParams::embedded("cube.step", CUBE_STEP);
    params.translation_m = [x, 0.0, 0.0];
    Operation::ImportedBody { params }
}

fn agent() -> Provenance {
    Provenance {
        origin: ProvenanceOrigin::Agent {
            name: "claude-code".to_string(),
        },
        at: Some("2026-09-14T04:00:00Z".to_string()),
    }
}

fn datum_plane_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

fn tree_json(state: &EngineState) -> serde_json::Value {
    serde_json::to_value(&state.engine.tree).unwrap()
}

/// The feature id a `ModelUpdated` names, or a panic naming what came back.
fn answered_feature_id(response: &EngineToUi) -> Option<Uuid> {
    match response {
        EngineToUi::ModelUpdated { feature_id, .. } => *feature_id,
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

#[test]
fn messages_without_provenance_still_parse() {
    // The app's writers send no provenance; the field is additive.
    let op = serde_json::to_value(cube_op(0.0)).unwrap();
    let add: UiToEngine =
        serde_json::from_value(serde_json::json!({"type": "AddFeature", "operation": op})).unwrap();
    assert!(matches!(
        add,
        UiToEngine::AddFeature {
            provenance: None,
            ..
        }
    ));
    let edit: UiToEngine = serde_json::from_value(serde_json::json!({
        "type": "EditFeature", "feature_id": Uuid::new_v4(), "operation": op
    }))
    .unwrap();
    assert!(matches!(
        edit,
        UiToEngine::EditFeature {
            provenance: None,
            ..
        }
    ));
    let finish: UiToEngine =
        serde_json::from_value(serde_json::json!({"type": "FinishSketch"})).unwrap();
    assert!(matches!(
        finish,
        UiToEngine::FinishSketch {
            provenance: None,
            ..
        }
    ));
}

#[test]
fn add_feature_records_provenance_and_answers_the_new_id() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let msg: UiToEngine = serde_json::from_value(serde_json::json!({
        "type": "AddFeature",
        "operation": cube_op(0.0),
        "provenance": agent(),
    }))
    .unwrap();
    let response = dispatch(&mut state, msg, &mut kernel);

    let id = answered_feature_id(&response).expect("AddFeature names its feature");
    assert_eq!(state.engine.tree.features.len(), 1);
    assert_eq!(state.engine.tree.features[0].id, id);
    assert_eq!(state.engine.tree.provenance_of(id), Some(&agent()));
}

#[test]
fn one_undo_restores_the_tree_exactly_after_an_agent_add() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let before = tree_json(&state);

    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: cube_op(0.0),
            provenance: Some(agent()),
        },
        &mut kernel,
    );
    let after = tree_json(&state);
    dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(tree_json(&state), before);
    dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert_eq!(tree_json(&state), after);
}

#[test]
fn edit_feature_replaces_provenance_and_answers_the_edited_id() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let added = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: cube_op(0.0),
            provenance: None,
        },
        &mut kernel,
    );
    let id = answered_feature_id(&added).unwrap();
    let before_edit = tree_json(&state);

    let edited = dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: id,
            operation: cube_op(1.0),
            provenance: Some(agent()),
        },
        &mut kernel,
    );
    assert_eq!(answered_feature_id(&edited), Some(id));
    assert_eq!(state.engine.tree.provenance_of(id), Some(&agent()));

    dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(tree_json(&state), before_edit);
}

#[test]
fn finish_sketch_records_provenance_and_answers_the_sketch_id() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let before = tree_json(&state);

    dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: datum_plane_ref(),
        },
        &mut kernel,
    );
    let response = dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions: [(1, (0.0, 0.0)), (2, (0.01, 0.0))].into_iter().collect(),
            solved_profiles: Vec::new(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![
                SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 2,
                    x: 0.01,
                    y: 0.0,
                    construction: false,
                },
            ],
            constraints: Vec::new(),
            projected: Vec::new(),
            provenance: Some(agent()),
        },
        &mut kernel,
    );

    let id = answered_feature_id(&response).expect("FinishSketch names its sketch");
    assert!(matches!(
        state.engine.tree.find_feature(id).map(|f| &f.operation),
        Some(Operation::Sketch { .. })
    ));
    assert_eq!(state.engine.tree.provenance_of(id), Some(&agent()));

    dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(tree_json(&state), before, "one undo, exact tree");
}

#[test]
fn commands_that_create_no_feature_answer_no_id() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: cube_op(0.0),
            provenance: None,
        },
        &mut kernel,
    );
    let undone = dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(answered_feature_id(&undone), None);
}

#[test]
fn undo_of_a_step_import_leaves_no_orphan_provenance() {
    // The import path records `Import` provenance; before ICR-4 that record
    // was written outside the undo step and survived the undo of the import.
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let before = tree_json(&state);

    dispatch(
        &mut state,
        UiToEngine::ImportStep {
            file_name: "cube.step".to_string(),
            data: CUBE_STEP.to_string(),
        },
        &mut kernel,
    );
    assert_eq!(state.engine.tree.provenance.len(), 1);

    dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(tree_json(&state), before);

    dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert_eq!(state.engine.tree.provenance.len(), 1, "redo restores it");
}
