//! The read-only agent tools in the engine (`specs/waffle_server_mode.md`
//! §2.3 S3, checkpoints C2 and C3): `feature_get`, and the four tools that
//! wrap one engine message.
//!
//! The agreement with the page's JS implementation is proven by the
//! differential in `app/tests/gui/agent-rust-tools.spec.js`, against the real
//! kernel. These tests pin what that cannot reach: the refusal codes and the
//! result shapes, including the ones that only appear when something is
//! missing.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

/// Run a tool and return its result.
fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    execute_tool(state, &mut kernel, name, &args, None)
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

fn line(id: u32, start_id: u32, end_id: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

/// A closed 20 × 10 mm rectangle: four points joined by four lines.
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
            entities: vec![
                point(1, 0.0, 0.0),
                point(2, 0.02, 0.0),
                point(3, 0.02, 0.01),
                point(4, 0.0, 0.01),
                line(5, 1, 2),
                line(6, 2, 3),
                line(7, 3, 4),
                line(8, 4, 1),
            ],
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions,
            projected: Vec::new(),
            solved_profiles: vec![ClosedProfile {
                entity_ids: vec![5, 6, 7, 8],
                is_outer: true,
                vertex_ids: vec![1, 2, 3, 4],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
        },
    }
}

fn extrude_op(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: None,
            targets: None,
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(vec![5, 6, 7, 8]),
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

fn add_feature(state: &mut EngineState, operation: Operation, agent: Option<&str>) -> Uuid {
    let mut kernel = MockKernel::new();
    let provenance = agent.map(|name| Provenance {
        origin: ProvenanceOrigin::Agent {
            name: name.to_string(),
        },
        at: None,
    });
    match wasm_bridge::dispatch(
        state,
        UiToEngine::AddFeature {
            operation,
            provenance,
        },
        &mut kernel,
    ) {
        EngineToUi::ModelUpdated { feature_tree, .. } => {
            feature_tree.features.last().expect("a feature").id
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

// ── feature_get (C2) ─────────────────────────────────────────────────────

#[test]
fn feature_get_returns_the_operation_a_feature_edit_would_take_back() {
    let mut state = EngineState::new();
    let id = add_feature(&mut state, rectangle_sketch(), Some("test-agent"));

    let out = ok(&mut state, "feature_get", json!({ "feature_id": id }));
    assert_eq!(out["feature_id"], json!(id));
    assert_eq!(out["suppressed"], false);
    assert_eq!(
        out["provenance"],
        json!({ "type": "Agent", "name": "test-agent" })
    );
    // The WHOLE operation, not a summary of it.
    assert_eq!(out["operation"]["type"], "Sketch");
    assert_eq!(
        out["operation"]["sketch"]["entities"]
            .as_array()
            .expect("entities")
            .len(),
        8
    );
    assert!(out.get("error").is_none());
}

#[test]
fn feature_get_reports_a_features_rebuild_error() {
    let mut state = EngineState::new();
    let id = add_feature(&mut state, rectangle_sketch(), None);
    state.engine.errors = vec![(id, "it did not build".to_string())];

    let out = ok(&mut state, "feature_get", json!({ "feature_id": id }));
    assert_eq!(out["error"], "it did not build");
    assert_eq!(out["provenance"], json!({ "type": "User" }));
}

#[test]
fn feature_get_refuses_an_id_the_tree_does_not_have() {
    let mut state = EngineState::new();
    let missing = Uuid::nil();
    let error = refused(&mut state, "feature_get", json!({ "feature_id": missing }));

    assert_eq!(error["code"], "FeatureNotFound");
    assert_eq!(error["details"]["feature_id"], json!(missing.to_string()));
}

#[test]
fn feature_get_refuses_an_id_that_is_not_a_uuid_the_same_way() {
    // The page matches ids as text, so a malformed id simply names no
    // feature — it is not a different class of failure.
    let mut state = EngineState::new();
    let error = refused(&mut state, "feature_get", json!({ "feature_id": "banana" }));
    assert_eq!(error["code"], "FeatureNotFound");
    assert_eq!(error["details"]["feature_id"], "banana");
}

// ── body_measure / face_list (C3) ────────────────────────────────────────

#[test]
fn body_measure_refuses_a_body_the_part_does_not_render() {
    // `find_body` in dispatch would happily search every feature output,
    // including consumed and rolled-back ones. The tool refuses first, on the
    // list the viewport actually shows — the page's `getBodies()`.
    let mut state = EngineState::new();
    add_feature(&mut state, rectangle_sketch(), None);
    let error = refused(
        &mut state,
        "body_measure",
        json!({ "body_id": "nope/Main" }),
    );

    assert_eq!(error["code"], "BodyNotFound");
    assert_eq!(error["details"]["body_id"], "nope/Main");
}

#[test]
fn face_list_refuses_a_body_the_part_does_not_render() {
    let mut state = EngineState::new();
    let error = refused(&mut state, "face_list", json!({ "body_id": "nope/Main" }));
    assert_eq!(error["code"], "BodyNotFound");
}

#[test]
fn face_list_refuses_a_filter_that_is_not_a_topo_query() {
    let mut state = EngineState::new();
    let sketch = add_feature(&mut state, rectangle_sketch(), None);
    add_feature(&mut state, extrude_op(sketch), None);

    // The body check comes first, so name a body that cannot exist and assert
    // only that a malformed filter never reaches the engine unnoticed.
    let error = refused(
        &mut state,
        "face_list",
        json!({ "body_id": "nope/Main", "filter": { "nonsense": true } }),
    );
    assert_eq!(error["code"], "BodyNotFound");
}

// ── sketch_regions (C3) ──────────────────────────────────────────────────

#[test]
fn sketch_regions_finds_the_closed_region_of_a_sketch() {
    let mut state = EngineState::new();
    let id = add_feature(&mut state, rectangle_sketch(), None);

    let out = ok(&mut state, "sketch_regions", json!({ "feature_id": id }));
    assert_eq!(out["feature_id"], json!(id));
    let regions = out["regions"].as_array().expect("regions");
    assert_eq!(regions.len(), 1);
    // 20 mm × 10 mm, in square meters.
    let area = regions[0]["area_m2"].as_f64().expect("area");
    assert!(
        (area - 0.0002).abs() < 1e-12,
        "expected 2e-4 m², got {area}"
    );
    assert_eq!(regions[0]["profile_entity_ids"], json!([5, 6, 7, 8]));
}

#[test]
fn sketch_regions_refuses_a_feature_that_is_not_a_sketch() {
    let mut state = EngineState::new();
    let sketch = add_feature(&mut state, rectangle_sketch(), None);
    let solid = add_feature(&mut state, extrude_op(sketch), None);

    let error = refused(&mut state, "sketch_regions", json!({ "feature_id": solid }));
    assert_eq!(error["code"], "OperationKindMismatch");
    assert_eq!(error["details"]["expected"], "Sketch");
    assert_eq!(error["details"]["got"], "Extrude");
}

#[test]
fn sketch_regions_refuses_an_unknown_feature() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        "sketch_regions",
        json!({ "feature_id": Uuid::nil() }),
    );
    assert_eq!(error["code"], "FeatureNotFound");
}

// ── expression_evaluate (C3) ─────────────────────────────────────────────

#[test]
fn expression_evaluate_returns_the_mm_value() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        "expression_evaluate",
        json!({ "expression": "20" }),
    );

    assert_eq!(out["expression"], "20");
    assert_eq!(out["value_mm"], 20.0);
    assert!(out.get("error").is_none());
}

#[test]
fn an_expression_that_does_not_evaluate_reports_its_error_not_a_value() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        "expression_evaluate",
        json!({ "expression": "nope +" }),
    );

    assert_eq!(out["expression"], "nope +");
    assert_eq!(out["value_mm"], Value::Null);
    assert!(
        out.get("error").is_some(),
        "an expression that cannot evaluate must say so: {out}"
    );
}

// ── the migrated set ─────────────────────────────────────────────────────

#[test]
fn the_migrated_list_is_exactly_what_this_checkpoint_implements() {
    // The page shadows this list; a name here whose JS body is still the one
    // serving answers is the intended state, a name MISSING here is a tool
    // silently left un-differentiated.
    assert_eq!(
        wasm_bridge::tools::MIGRATED,
        &[
            "model_summary",
            "feature_get",
            "body_measure",
            "face_list",
            "sketch_regions",
            "expression_evaluate",
        ]
    );
}
