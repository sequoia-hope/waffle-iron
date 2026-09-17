//! Agent tool semantics in the engine (`specs/waffle_server_mode.md` §2.3 S3),
//! checkpoint C1: the `Tool` message, the `ToolResult` shape, and the first
//! migrated tool, `model_summary`.
//!
//! The differential against the JS implementation runs in the page
//! (`app/tests/gui/agent-rust-tools.spec.js`, which shadows every call). These
//! tests pin what that differential cannot see: the shape a host gets, and the
//! two places where the store's MIRROR of `ModelUpdated` is not the engine's
//! own shape — deduplicated warnings, and bodies that have no mesh.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

/// Run `model_summary` and return its structured content.
fn summary(state: &mut EngineState) -> Value {
    let mut kernel = MockKernel::new();
    let result = execute_tool(state, &mut kernel, "model_summary", &json!({}), None);
    assert!(!result.is_error, "model_summary failed: {result:?}");
    result.structured_content
}

fn make_sketch_op() -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (0.02, 0.0));
    solved_positions.insert(3, (0.02, 0.01));
    solved_positions.insert(4, (0.0, 0.01));

    let point = |id: u32, x: f64, y: f64| SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    };

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
            entities: vec![
                point(1, 0.0, 0.0),
                point(2, 0.02, 0.0),
                point(3, 0.02, 0.01),
                point(4, 0.0, 0.01),
            ],
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

fn make_extrude_op(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: None,
            targets: None,
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth: 0.005,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            depth_expr: None,
        },
    }
}

/// Add a feature through the real message path, returning its id.
fn add_feature(state: &mut EngineState, operation: Operation, agent: Option<&str>) -> Uuid {
    let mut kernel = MockKernel::new();
    let provenance = agent.map(|name| Provenance {
        origin: ProvenanceOrigin::Agent {
            name: name.to_string(),
        },
        at: None,
    });
    let response = wasm_bridge::dispatch(
        state,
        UiToEngine::AddFeature {
            operation,
            provenance,
        },
        &mut kernel,
    );
    match response {
        EngineToUi::ModelUpdated { feature_tree, .. } => {
            feature_tree.features.last().expect("a feature").id
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

#[test]
fn a_fresh_document_summarizes_as_empty() {
    let mut state = EngineState::new();
    let s = summary(&mut state);

    assert_eq!(s["document_name"], "Untitled");
    assert_eq!(s["features"], json!([]));
    assert_eq!(s["rollback_index"], Value::Null);
    assert_eq!(s["bodies"], json!([]));
    assert_eq!(s["errors"], json!([]));
    assert_eq!(s["warnings"], json!([]));
    assert_eq!(s["parameters"], json!([]));
    assert_eq!(s["connectors"], json!([]));
}

#[test]
fn a_feature_carries_its_kind_suppression_and_provenance() {
    let mut state = EngineState::new();
    let id = add_feature(&mut state, make_sketch_op(), Some("test-agent"));

    let s = summary(&mut state);
    let features = s["features"].as_array().expect("features");
    assert_eq!(features.len(), 1);
    assert_eq!(features[0]["id"], json!(id));
    assert_eq!(features[0]["kind"], "Sketch");
    assert_eq!(features[0]["suppressed"], false);
    // The ORIGIN only: `Provenance.at` is a timestamp, and reporting it would
    // break I14 determinism.
    assert_eq!(
        features[0]["provenance"],
        json!({ "type": "Agent", "name": "test-agent" })
    );
    assert!(features[0].get("at").is_none());
}

#[test]
fn a_feature_with_no_provenance_record_is_a_user_feature() {
    let mut state = EngineState::new();
    add_feature(&mut state, make_sketch_op(), None);

    let s = summary(&mut state);
    assert_eq!(s["features"][0]["provenance"], json!({ "type": "User" }));
}

#[test]
fn the_rollback_index_is_reported() {
    let mut state = EngineState::new();
    let sketch = add_feature(&mut state, make_sketch_op(), None);
    add_feature(&mut state, make_extrude_op(sketch), None);

    let mut kernel = MockKernel::new();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SetRollbackIndex { index: Some(0) },
        &mut kernel,
    );

    assert_eq!(summary(&mut state)["rollback_index"], 0);
}

#[test]
fn warnings_are_deduplicated_in_first_seen_order() {
    // The store holds warnings in a `Set` (`store.svelte.js`
    // `lastRebuildWarnings`), so a warning the engine emits twice reaches the
    // agent once. Reading `engine.warnings` naively would report it twice.
    let mut state = EngineState::new();
    state.engine.warnings = vec![
        "auto-union fallback".to_string(),
        "body created as standalone".to_string(),
        "auto-union fallback".to_string(),
    ];

    assert_eq!(
        summary(&mut state)["warnings"],
        json!(["auto-union fallback", "body created as standalone"])
    );
}

#[test]
fn errors_are_in_tree_order_and_orphans_sort_last() {
    let mut state = EngineState::new();
    let first = add_feature(&mut state, make_sketch_op(), None);
    let second = add_feature(&mut state, make_sketch_op(), None);

    // Two ids the tree does not hold, deliberately out of sorted order.
    let orphan_b = Uuid::parse_str("ffffffff-ffff-4fff-8fff-ffffffffffff").unwrap();
    let orphan_a = Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
    state.engine.errors = vec![
        (second, "second failed".to_string()),
        (orphan_b, "orphan b".to_string()),
        (first, "first failed".to_string()),
        (orphan_a, "orphan a".to_string()),
    ];

    let s = summary(&mut state);
    assert_eq!(
        s["errors"],
        json!([
            { "feature_id": first.to_string(), "message": "first failed" },
            { "feature_id": second.to_string(), "message": "second failed" },
            { "feature_id": orphan_a.to_string(), "message": "orphan a" },
            { "feature_id": orphan_b.to_string(), "message": "orphan b" },
        ])
    );

    // The same error is on the feature row too.
    assert_eq!(s["features"][0]["error"], "first failed");
}

#[test]
fn a_feature_that_built_carries_no_error_key() {
    let mut state = EngineState::new();
    add_feature(&mut state, make_sketch_op(), None);
    assert!(summary(&mut state)["features"][0].get("error").is_none());
}

#[test]
fn parameters_report_their_expression_and_last_value() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SetParameters {
            parameters: vec![DesignParameter::new("width", "20")],
        },
        &mut kernel,
    );

    let s = summary(&mut state);
    let parameters = s["parameters"].as_array().expect("parameters");
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0]["name"], "width");
    assert_eq!(parameters[0]["expression"], "20");
    assert_eq!(parameters[0]["value_mm"], 20.0);
    assert!(parameters[0].get("error").is_none());
}

#[test]
fn a_parameter_that_does_not_evaluate_reports_its_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SetParameters {
            parameters: vec![DesignParameter::new("width", "nope +")],
        },
        &mut kernel,
    );

    let parameters = summary(&mut state)["parameters"].clone();
    assert!(
        parameters[0].get("error").is_some(),
        "a parameter that cannot evaluate must say so: {parameters}"
    );
}

#[test]
fn the_document_name_comes_from_the_session() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SetDocumentMeta {
            name: Some("Bracket".to_string()),
            display_unit: None,
            id: None,
            created: None,
        },
        &mut kernel,
    );

    assert_eq!(summary(&mut state)["document_name"], "Bracket");
}

#[test]
fn an_unknown_tool_is_loud() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let result = execute_tool(&mut state, &mut kernel, "no_such_tool", &json!({}), None);

    assert!(result.is_error);
    assert_eq!(
        result.structured_content["error"]["code"],
        "ToolUnavailable"
    );
    assert_eq!(
        result.structured_content["error"]["details"]["tool"],
        "no_such_tool"
    );
}

#[test]
fn a_tool_not_migrated_yet_is_unavailable_not_silent() {
    // The page still answers these in JS; an engine asked for one must refuse
    // rather than return nothing. This named `feature_add` until C4 moved the
    // twelve authoring tools and `sketch_create` until C5 moved that; the
    // export pair (C6) is what stands for "not here yet" now.
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let result = execute_tool(&mut state, &mut kernel, "export_step", &json!({}), None);

    assert!(result.is_error);
    assert_eq!(
        result.structured_content["error"]["code"],
        "ToolUnavailable"
    );
    assert!(!wasm_bridge::tools::MIGRATED.contains(&"export_step"));
    assert!(wasm_bridge::tools::MIGRATED.contains(&"model_summary"));
}

#[test]
fn the_tool_message_answers_in_the_mcp_wire_shape() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let msg: UiToEngine = serde_json::from_value(json!({
        "type": "Tool",
        "name": "model_summary",
        "arguments": {},
    }))
    .expect("Tool parses without a context");

    let response = wasm_bridge::dispatch(&mut state, msg, &mut kernel);
    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["type"], "ToolResult");
    assert_eq!(json["isError"], false);
    assert_eq!(json["structuredContent"]["document_name"], "Untitled");
    // `content` is what an MCP client reads when it ignores the structured half.
    assert_eq!(json["content"][0]["type"], "text");
    assert!(json["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("document_name"));

    // And it round-trips back into the enum (a host parses its own answers).
    let parsed: EngineToUi = serde_json::from_value(json).unwrap();
    assert!(matches!(parsed, EngineToUi::ToolResult { .. }));
}

#[test]
fn a_tool_failure_answers_in_the_error_shape() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let msg = UiToEngine::Tool {
        name: "no_such_tool".to_string(),
        arguments: json!({}),
        context: None,
    };

    let json = serde_json::to_value(wasm_bridge::dispatch(&mut state, msg, &mut kernel)).unwrap();
    assert_eq!(json["isError"], true);
    assert_eq!(
        json["structuredContent"]["error"]["code"],
        "ToolUnavailable"
    );
    assert!(json["content"][0]["text"]
        .as_str()
        .expect("text")
        .starts_with("ToolUnavailable: "));
}
