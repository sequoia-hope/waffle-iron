//! The authoring agent tools in the engine (`specs/waffle_server_mode.md`
//! §2.3 S3, checkpoint C4): `apply_step` and the twelve tools built on it.
//!
//! Agreement with the page's JS implementation is proven by the sequence
//! differential in `app/tests/gui/agent-rust-authoring.spec.js`, against the
//! real kernel. These tests pin what that cannot reach: the refusal codes, and
//! the delta shapes — including the ones that only appear when a step fails.
//!
//! `MockKernel` renders no bodies, so `bodies_added` / `bodies_removed` are
//! not exercised here; the differential covers them on real geometry.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::*;

/// The agent name every call in this file carries.
const AGENT: &str = "author-test";

/// Run a tool as the agent would.
fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": AGENT });
    execute_tool(state, &mut kernel, name, &args, Some(&context))
}

/// Run a tool that is expected to succeed.
fn ok(state: &mut EngineState, name: &str, args: Value) -> Value {
    let result = tool(state, name, args);
    assert!(!result.is_error, "{name} failed: {result:?}");
    result.structured_content
}

/// Run a tool that is expected to refuse, returning `{code, message, details}`.
fn refused(state: &mut EngineState, name: &str, args: Value) -> Value {
    let result = tool(state, name, args);
    assert!(result.is_error, "{name} was expected to refuse: {result:?}");
    result.structured_content["error"].clone()
}

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

/// A closed 20 × 10 mm rectangle whose solved loop is entities 1–4.
fn rectangle_sketch() -> Operation {
    let corners = [(0.0, 0.0), (0.02, 0.0), (0.02, 0.01), (0.0, 0.01)];
    let mut solved_positions = std::collections::HashMap::new();
    for (i, (x, y)) in corners.iter().enumerate() {
        solved_positions.insert(i as u32 + 1, (*x, *y));
    }

    Operation::Sketch {
        sketch: Sketch {
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
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: corners
                .iter()
                .enumerate()
                .map(|(i, (x, y))| point(i as u32 + 1, *x, *y))
                .collect(),
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions,
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    }
}

/// An extrude of `sketch_id`, addressing its loop by entity ids.
fn extrude(sketch_id: Uuid, profile_entity_ids: Option<Vec<u32>>) -> Value {
    json!({
        "type": "Extrude",
        "params": {
            "sketch_id": sketch_id,
            "profile_index": 0,
            "profile_entity_ids": profile_entity_ids,
            "depth": 0.005,
            "symmetric": false,
            "cut": false,
        }
    })
}

/// A document holding one committed sketch, added through the tool itself.
fn state_with_sketch() -> (EngineState, Uuid) {
    let mut state = EngineState::new();
    let operation = serde_json::to_value(rectangle_sketch()).expect("a sketch operation");
    let added = ok(&mut state, "feature_add", json!({ "operation": operation }));
    let id = Uuid::parse_str(added["feature_id"].as_str().expect("a feature id")).expect("a uuid");
    (state, id)
}

/// The feature ids of the open tree, in order.
fn feature_ids(state: &EngineState) -> Vec<String> {
    state
        .engine
        .tree
        .features
        .iter()
        .map(|f| f.id.to_string())
        .collect()
}

// ── What an agent may author ─────────────────────────────────────────────

#[test]
fn fillet_chamfer_and_shell_are_refused_by_name() {
    let mut state = EngineState::new();
    for kind in ["Fillet", "Chamfer", "Shell"] {
        let error = refused(
            &mut state,
            "feature_add",
            json!({ "operation": { "type": kind, "params": {} } }),
        );
        assert_eq!(error["code"], "Deferred", "{kind}");
        assert_eq!(error["details"]["operation"], kind);
    }
    // Nothing reached the tree.
    assert!(state.engine.tree.features.is_empty());
}

#[test]
fn an_operation_kind_that_cannot_be_authored_is_invalid_operation() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        "feature_add",
        json!({ "operation": { "type": "Warp", "params": {} } }),
    );
    assert_eq!(error["code"], "InvalidOperation");
    assert_eq!(error["details"]["schema_path"], "/operation/type");
}

#[test]
fn an_imported_body_is_placed_by_the_import_tool_not_authored() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        "feature_add",
        json!({ "operation": { "type": "ImportedBody", "params": {} } }),
    );
    assert_eq!(error["code"], "UseImportTool");
}

// ── feature_add ──────────────────────────────────────────────────────────

#[test]
fn feature_add_records_the_calling_agent_as_the_features_origin() {
    let (state, sketch_id) = state_with_sketch();
    let provenance = state
        .engine
        .tree
        .provenance
        .get(&sketch_id)
        .expect("the added feature has provenance");
    assert_eq!(
        json!(provenance.origin),
        json!({ "type": "Agent", "name": AGENT })
    );
}

#[test]
fn feature_add_answers_the_new_feature_and_an_otherwise_empty_delta() {
    let (state, sketch_id) = state_with_sketch();
    let mut state = state;
    let added = ok(
        &mut state,
        "feature_add",
        json!({ "operation": extrude(sketch_id, Some(vec![1, 2, 3, 4])) }),
    );

    let id = added["feature_id"].as_str().expect("a feature id");
    assert_eq!(added["features_added"], json!([id]));
    assert_eq!(added["features_changed"], json!([]));
    assert_eq!(added["features_removed"], json!([]));
    assert_eq!(added["order_changed"], json!(false));
    assert_eq!(added["errors"], json!([]));
}

/// A pattern is authorable through `feature_add` as plain Operation JSON
/// (`specs/custom_features_and_modeling_roadmap.md` §B1): the step adds the
/// node, its seed's feature is consumed (custody), and every instance is an
/// output of the pattern node. A malformed pattern (count 1) is a per-feature
/// rebuild error and, by default, rolled back like any failing step.
#[test]
fn feature_add_authors_a_circular_pattern_and_rolls_back_a_bad_one() {
    // One kernel across the steps: the pattern copies the extrude's body,
    // which must live in the same kernel (the file's `tool` helper makes a
    // fresh MockKernel per call, which is fine for every other test here).
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": AGENT });
    let mut run = |state: &mut EngineState, args: Value| -> ToolResult {
        execute_tool(state, &mut kernel, "feature_add", &args, Some(&context))
    };
    let ok = |r: ToolResult| -> Value {
        assert!(!r.is_error, "feature_add failed: {r:?}");
        r.structured_content
    };
    let refused = |r: ToolResult| -> Value {
        assert!(r.is_error, "feature_add was expected to refuse: {r:?}");
        r.structured_content["error"].clone()
    };

    let mut state = EngineState::new();
    let operation = serde_json::to_value(rectangle_sketch()).expect("a sketch operation");
    let added = ok(run(&mut state, json!({ "operation": operation })));
    let sketch_id = Uuid::parse_str(added["feature_id"].as_str().expect("id")).expect("uuid");
    let added = ok(run(
        &mut state,
        json!({ "operation": extrude(sketch_id, Some(vec![1, 2, 3, 4])) }),
    ));
    let seed = added["feature_id"]
        .as_str()
        .expect("a feature id")
        .to_string();
    let seed_ref = serde_json::to_value(GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id: Uuid::parse_str(&seed).unwrap(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    })
    .unwrap();
    let pattern = |count: u32| {
        json!({
            "type": "PatternCircular",
            "params": {
                "seeds": [seed_ref],
                "axis": { "method": "explicit", "origin": [0, 0, 0], "direction": [0, 0, 1] },
                "count": count,
                "angle_deg": 360
            }
        })
    };

    let bad = refused(run(&mut state, json!({ "operation": pattern(1) })));
    assert_eq!(bad["code"], "FeatureRebuildFailed", "{bad}");
    assert_eq!(bad["details"]["rolled_back"], json!(true));
    assert!(
        bad["message"]
            .as_str()
            .unwrap_or("")
            .contains("count must be at least 2"),
        "{bad}"
    );
    assert_eq!(
        feature_ids(&state).len(),
        2,
        "the failing step was rolled back"
    );

    let good = ok(run(&mut state, json!({ "operation": pattern(5) })));
    let id = Uuid::parse_str(good["feature_id"].as_str().expect("id")).expect("uuid");
    assert_eq!(good["errors"], json!([]));
    let seed_id = Uuid::parse_str(&seed).unwrap();
    assert!(state.engine.consumed_features.contains(&seed_id));
    let result = state.engine.get_result(id).expect("pattern result");
    assert_eq!(result.outputs.len(), 5);
    assert_eq!(result.outputs[0].0, OutputKey::Main);
}

// ── feature_edit ─────────────────────────────────────────────────────────

#[test]
fn feature_edit_refuses_to_change_a_features_kind() {
    let (mut state, sketch_id) = state_with_sketch();
    let error = refused(
        &mut state,
        "feature_edit",
        json!({
            "feature_id": sketch_id.to_string(),
            "operation": extrude(sketch_id, Some(vec![1, 2, 3, 4])),
        }),
    );
    assert_eq!(error["code"], "OperationKindMismatch");
    assert_eq!(error["details"]["expected"], "Sketch");
    assert_eq!(error["details"]["got"], "Extrude");
}

#[test]
fn an_id_that_names_no_feature_is_feature_not_found() {
    let (mut state, _) = state_with_sketch();
    for name in ["feature_edit", "feature_delete", "feature_rename"] {
        let error = refused(
            &mut state,
            name,
            json!({
                "feature_id": "not-a-uuid",
                "operation": { "type": "Sketch" },
                "new_name": "x",
            }),
        );
        assert_eq!(error["code"], "FeatureNotFound", "{name}");
    }
}

// ── The steps that report rather than roll back ──────────────────────────

#[test]
fn renaming_a_feature_changes_its_record_without_reordering() {
    let (mut state, sketch_id) = state_with_sketch();
    let delta = ok(
        &mut state,
        "feature_rename",
        json!({ "feature_id": sketch_id.to_string(), "new_name": "Base outline" }),
    );
    assert_eq!(delta["features_changed"], json!([sketch_id.to_string()]));
    assert_eq!(delta["order_changed"], json!(false));
    assert_eq!(state.engine.tree.features[0].name, "Base outline");
}

#[test]
fn suppressing_a_feature_changes_its_record() {
    let (mut state, sketch_id) = state_with_sketch();
    let delta = ok(
        &mut state,
        "feature_suppress",
        json!({ "feature_id": sketch_id.to_string(), "suppressed": true }),
    );
    assert_eq!(delta["features_changed"], json!([sketch_id.to_string()]));
    assert!(state.engine.tree.features[0].suppressed);
}

#[test]
fn reordering_is_reported_by_order_changed_not_as_a_changed_record() {
    let (mut state, sketch_id) = state_with_sketch();
    let second = ok(
        &mut state,
        "feature_add",
        json!({ "operation": extrude(sketch_id, Some(vec![1, 2, 3, 4])) }),
    );
    let second_id = second["feature_id"]
        .as_str()
        .expect("a feature id")
        .to_string();

    let delta = ok(
        &mut state,
        "feature_reorder",
        json!({ "feature_id": second_id, "new_position": 0 }),
    );
    assert_eq!(delta["order_changed"], json!(true));
    assert_eq!(delta["features_changed"], json!([]));
    assert_eq!(feature_ids(&state)[0], second_id);
}

#[test]
fn deleting_a_feature_reports_it_removed() {
    let (mut state, sketch_id) = state_with_sketch();
    let delta = ok(
        &mut state,
        "feature_delete",
        json!({ "feature_id": sketch_id.to_string() }),
    );
    assert_eq!(delta["features_removed"], json!([sketch_id.to_string()]));
    assert!(state.engine.tree.features.is_empty());
}

// ── body_rename and rollback_set ─────────────────────────────────────────

#[test]
fn renaming_a_body_the_viewport_does_not_show_is_body_not_found() {
    let (mut state, _) = state_with_sketch();
    let error = refused(
        &mut state,
        "body_rename",
        json!({ "body_id": "no-such-body", "new_name": "D" }),
    );
    assert_eq!(error["code"], "BodyNotFound");
}

#[test]
fn a_rollback_index_past_the_last_feature_is_refused_with_the_count() {
    let (mut state, _) = state_with_sketch();
    let error = refused(&mut state, "rollback_set", json!({ "index": 7 }));
    assert_eq!(error["code"], "FeatureNotFound");
    assert_eq!(error["details"]["index"], 7);
    assert_eq!(error["details"]["feature_count"], 1);
}

#[test]
fn rollback_set_moves_the_bar_and_null_makes_every_feature_active() {
    let (mut state, _) = state_with_sketch();
    ok(&mut state, "rollback_set", json!({ "index": 0 }));
    assert_eq!(state.engine.tree.active_index, Some(0));
    ok(&mut state, "rollback_set", json!({ "index": Value::Null }));
    assert_eq!(state.engine.tree.active_index, None);
}

// ── undo / redo ──────────────────────────────────────────────────────────

#[test]
fn undo_and_redo_with_no_history_refuse_with_their_own_codes() {
    let mut state = EngineState::new();
    let undo = refused(&mut state, "undo", json!({}));
    assert_eq!(undo["code"], "NothingToUndo");
    assert_eq!(undo["message"], "There is nothing to undo.");

    let redo = refused(&mut state, "redo", json!({}));
    assert_eq!(redo["code"], "NothingToRedo");
    assert_eq!(redo["message"], "There is nothing to redo.");
}

#[test]
fn undo_removes_the_last_step_and_redo_puts_it_back() {
    let (mut state, sketch_id) = state_with_sketch();
    let undone = ok(&mut state, "undo", json!({}));
    assert_eq!(undone["features_removed"], json!([sketch_id.to_string()]));
    assert!(state.engine.tree.features.is_empty());

    let redone = ok(&mut state, "redo", json!({}));
    assert_eq!(redone["features_added"], json!([sketch_id.to_string()]));
    assert_eq!(feature_ids(&state), vec![sketch_id.to_string()]);
}

// ── parameters_set ───────────────────────────────────────────────────────

#[test]
fn parameters_set_evaluates_the_table_and_reports_a_failing_expression() {
    let mut state = EngineState::new();
    let answer = ok(
        &mut state,
        "parameters_set",
        json!({
            "parameters": [
                { "name": "width", "expression": "20" },
                { "name": "broken", "expression": "nope +" },
            ]
        }),
    );

    let rows = answer["parameters"]
        .as_array()
        .expect("the evaluated table");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "width");
    assert_eq!(rows[0]["value_mm"], json!(20.0));
    assert!(rows[0].get("error").is_none());

    // A parameter that does not evaluate reports the error and no value —
    // never a stale number presented as the answer.
    assert_eq!(rows[1]["name"], "broken");
    assert_eq!(rows[1]["value_mm"], Value::Null);
    assert!(rows[1]["error"].is_string());

    // Every row was given an identity, and the delta rode along.
    for row in rows {
        assert!(Uuid::parse_str(row["id"].as_str().expect("an id")).is_ok());
    }
    assert!(answer.get("features_added").is_some());
}

#[test]
fn parameters_set_keeps_the_identity_of_a_parameter_it_is_given_back() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "width", "expression": "20" }] }),
    );
    let id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let second = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "id": id, "name": "width", "expression": "30" }] }),
    );
    assert_eq!(second["parameters"][0]["id"], json!(id));
    assert_eq!(second["parameters"][0]["value_mm"], json!(30.0));
}

#[test]
fn parameters_set_keeps_the_last_value_under_a_non_canonical_id_spelling() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "width", "expression": "20" }] }),
    );
    let id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    // The id is parsed leniently, so this spelling still names the parameter;
    // the last good value must be found by the parsed id, not its text,
    // or the row keeps its identity and silently loses its value.
    let braced_upper = format!("{{{}}}", id.to_uppercase());
    let second = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "id": braced_upper, "name": "width", "expression": "nope +" }] }),
    );
    assert_eq!(second["parameters"][0]["id"], json!(id));
    assert!(second["parameters"][0]["error"].is_string());
    let kept = &state.engine.tree.parameters[0];
    assert_eq!(kept.id.to_string(), id);
    assert_eq!(
        kept.value, 20.0,
        "the last good value is what dependents hold"
    );
}

// ── What a mutating tool's answer carries ─────────────────────────────────

#[test]
fn a_mutating_tools_answer_carries_the_model_update_and_its_preview() {
    // Real kernel: `bodies_added` and the preview both come from tessellated
    // meshes, which `MockKernel` never produces. This pins the two S3 C4
    // traps — the tool tessellates before its after-snapshot, and the answer
    // carries a `ModelUpdated` whose preview was attached (a `ToolResult`
    // never passes through `process_message`'s preview step).
    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    // A 20 × 10 mm rectangle with real edges (`rectangle_sketch` is points
    // only, which the mock never notices and the kernel cannot extrude).
    let corners = [
        (1, 0.0, 0.0),
        (2, 0.02, 0.0),
        (3, 0.02, 0.01),
        (4, 0.0, 0.01),
    ];
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    for (id, (a, b)) in [(10, (1, 2)), (11, (2, 3)), (12, (3, 4)), (13, (4, 1))] {
        entities.push(SketchEntity::Line {
            id,
            start_id: a,
            end_id: b,
            construction: false,
        });
    }
    let sketch = Operation::Sketch {
        sketch: Sketch {
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
                policy: ResolvePolicy::BestEffort,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: corners.iter().map(|&(id, x, y)| (id, (x, y))).collect(),
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![10, 11, 12, 13],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    };
    let added = ok(
        &mut state,
        "feature_add",
        json!({ "operation": serde_json::to_value(sketch).expect("a sketch operation") }),
    );
    let sketch_id = Uuid::parse_str(added["feature_id"].as_str().expect("an id")).expect("a uuid");
    let msg = wasm_bridge::messages::UiToEngine::Tool {
        name: "feature_add".to_string(),
        arguments: json!({ "operation": extrude(sketch_id, Some(vec![10, 11, 12, 13])) }),
        context: None,
    };
    let wasm_bridge::messages::EngineToUi::ToolResult { result, model } =
        wasm_bridge::dispatch(&mut state, msg, &mut kernel)
    else {
        panic!("a Tool message answers ToolResult");
    };
    assert!(!result.is_error, "{result:?}");
    assert_eq!(
        result.structured_content["bodies_added"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let model = model.expect("a mutating tool carries its model update");
    let wasm_bridge::messages::EngineToUi::ModelUpdated { preview_mesh, .. } = *model else {
        panic!("the carried model is a ModelUpdated");
    };
    assert!(
        preview_mesh.is_some(),
        "the preview of the new box is attached"
    );

    // And a read-only tool carries none.
    let read_only = wasm_bridge::messages::UiToEngine::Tool {
        name: "model_summary".to_string(),
        arguments: json!({}),
        context: None,
    };
    let wasm_bridge::messages::EngineToUi::ToolResult { model, .. } =
        wasm_bridge::dispatch(&mut state, read_only, &mut kernel)
    else {
        panic!("a Tool message answers ToolResult");
    };
    assert!(model.is_none());
}

// ── A step that makes a feature fail ─────────────────────────────────────

#[test]
fn a_step_that_makes_a_feature_fail_is_rolled_back_and_the_tree_restored() {
    let (mut state, sketch_id) = state_with_sketch();
    let before = feature_ids(&state);

    // A profile that names no loop: the feature builds into the tree and
    // fails, so the step is undone (A2) rather than refused up front.
    let error = refused(
        &mut state,
        "feature_add",
        json!({ "operation": extrude(sketch_id, Some(vec![99])) }),
    );

    assert_eq!(error["code"], "FeatureRebuildFailed");
    assert_eq!(error["details"]["rolled_back"], json!(true));
    assert!(error["details"]["engine_error"]["kind"].is_object());
    assert_eq!(feature_ids(&state), before, "the tree was restored");
    assert!(state.engine.errors.is_empty(), "and so were its errors");
}

#[test]
fn on_error_keep_leaves_the_failing_step_in_place_and_says_so() {
    let (mut state, sketch_id) = state_with_sketch();
    let kept = ok(
        &mut state,
        "feature_add",
        json!({
            "operation": extrude(sketch_id, Some(vec![99])),
            "on_error": "keep",
        }),
    );

    assert_eq!(kept["kept_with_error"], json!(true));
    let id = kept["feature_id"].as_str().expect("a feature id");
    assert_eq!(kept["features_added"], json!([id]));
    let errors = kept["errors"].as_array().expect("the current errors");
    assert!(errors.iter().any(|e| e["feature_id"] == json!(id)));
    assert_eq!(feature_ids(&state).len(), 2, "the feature stayed");
}

// ── The list the page shadows ────────────────────────────────────────────

#[test]
fn a_tool_this_engine_does_not_implement_is_refused_not_ignored() {
    // This named `sketch_create` until C5 moved it and `export_step` until
    // C6 did. `selection_get` reads the viewer's selection, host state by
    // §3.3, so the engine never serves it — and must say so.
    let mut state = EngineState::new();
    let error = refused(&mut state, "selection_get", json!({}));
    assert_eq!(error["code"], "ToolUnavailable");
    assert_eq!(error["details"]["tool"], "selection_get");
}

#[test]
fn every_authoring_tool_is_listed_as_migrated() {
    for name in [
        "feature_add",
        "feature_edit",
        "feature_delete",
        "feature_suppress",
        "feature_reorder",
        "feature_rename",
        "body_rename",
        "rollback_set",
        "parameters_set",
        "import_step",
        "undo",
        "redo",
    ] {
        assert!(tools::MIGRATED.contains(&name), "{name} is not in MIGRATED");
    }
}
