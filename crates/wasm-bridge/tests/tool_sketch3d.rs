//! The 3D sketch over the agent link (`specs/sketch3d.md` S3).
//!
//! Authoring goes through `feature_add` — a 3D sketch is a declarative
//! operation, so it needs no tool of its own — and `sketch3d_get` is what
//! reads back the thing no other tool can reach: the EVALUATED geometry, which
//! lives beside the rebuild rather than in the document because an attached
//! point resolves only during the rebuild walk.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::sketch3d::{Attachment, Axis, Sketch3d, Sketch3dEntity};
use wasm_bridge::messages::*;
use wasm_bridge::*;

fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    execute_tool(state, &mut kernel, name, &args, None)
}

fn ok(state: &mut EngineState, name: &str, args: Value) -> Value {
    let result = tool(state, name, args);
    assert!(!result.is_error, "{name} failed: {result:?}");
    result.structured_content
}

fn refused(state: &mut EngineState, name: &str, args: Value) -> Value {
    let result = tool(state, name, args);
    assert!(result.is_error, "{name} was expected to refuse: {result:?}");
    result.structured_content["error"].clone()
}

fn add_feature(state: &mut EngineState, operation: Operation) -> Uuid {
    let mut kernel = MockKernel::new();
    match wasm_bridge::dispatch(
        state,
        UiToEngine::AddFeature {
            operation,
            provenance: None,
        },
        &mut kernel,
    ) {
        EngineToUi::ModelUpdated { feature_tree, .. } => {
            feature_tree.features.last().expect("a feature").id
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

/// An L with a rounded corner, authored the way an agent would: the second
/// point is derived by an axis run rather than stated.
fn l_with_fillet() -> Sketch3d {
    Sketch3d::new(
        Uuid::new_v4(),
        vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [0.0, 0.0, 0.0],
                attach: None,
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 2,
                xyz: [0.0, 0.0, 0.0],
                attach: Some(Box::new(Attachment::AlongAxis {
                    from: 1,
                    axis: Axis::X,
                    distance: 1.0,
                })),
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 3,
                xyz: [0.0, 0.0, 0.0],
                attach: Some(Box::new(Attachment::AlongAxis {
                    from: 2,
                    axis: Axis::Z,
                    distance: 1.0,
                })),
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Line {
                id: 4,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            Sketch3dEntity::Line {
                id: 5,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            Sketch3dEntity::Fillet {
                id: 6,
                at_point_id: 2,
                radius: 0.2,
                radius_expr: None,
            },
        ],
    )
}

#[test]
fn feature_add_authors_a_3d_sketch() {
    let mut state = EngineState::new();
    let operation = serde_json::to_value(Operation::Sketch3d {
        sketch: l_with_fillet(),
    })
    .unwrap();

    let out = ok(&mut state, "feature_add", json!({ "operation": operation }));
    let feature_id = out["feature_id"].as_str().expect("a feature id");

    // Reference geometry: the step adds no body.
    assert_eq!(
        out["bodies_added"],
        json!([]),
        "a 3D sketch produces no body: {out}"
    );

    let got = ok(
        &mut state,
        "sketch3d_get",
        json!({ "feature_id": feature_id }),
    );
    assert_eq!(got["entity_count"], 6);
    assert_eq!(got["points"].as_array().unwrap().len(), 3);

    let chains = got["chains"].as_array().unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0]["closed"], false);
    // Two trimmed legs plus the fillet arc, and both joints tangent.
    assert_eq!(chains[0]["edges"].as_array().unwrap().len(), 3);
    assert_eq!(chains[0]["tangent_joints"], json!([true, true]));

    // The derived points resolved against each other, not against their hints.
    let by_id = |id: i64| -> Vec<f64> {
        got["points"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == json!(id))
            .map(|p| {
                p["xyz"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect()
            })
            .expect("point")
    };
    assert_eq!(by_id(2), vec![1.0, 0.0, 0.0]);
    assert_eq!(by_id(3), vec![1.0, 0.0, 1.0]);

    // Exactly one arc, of the radius asked for.
    let arcs: Vec<&Value> = chains[0]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == json!("Arc"))
        .collect();
    assert_eq!(arcs.len(), 1);
    assert!((arcs[0]["radius_m"].as_f64().unwrap() - 0.2).abs() < 1e-12);
}

#[test]
fn a_3d_sketch_that_does_not_evaluate_is_rolled_back_with_its_reason() {
    let mut state = EngineState::new();
    let mut sketch = l_with_fillet();
    // A radius that cannot fit between the corner's neighbours.
    sketch.entities.push(Sketch3dEntity::Fillet {
        id: 7,
        at_point_id: 3,
        radius: 99.0,
        radius_expr: None,
    });
    let operation = serde_json::to_value(Operation::Sketch3d { sketch }).unwrap();

    let error = refused(&mut state, "feature_add", json!({ "operation": operation }));
    let text = serde_json::to_string(&error).unwrap();
    assert!(
        text.contains("fillet") || text.contains("3D sketch"),
        "the refusal names the defect: {text}"
    );
    assert!(
        state.engine.tree.features.is_empty(),
        "a step that fails is rolled back by default"
    );
}

#[test]
fn sketch3d_get_refuses_a_feature_of_another_kind() {
    let mut state = EngineState::new();
    let id = add_feature(
        &mut state,
        Operation::DatumPlane {
            params: DatumPlaneParams {
                name: "P".into(),
                definition: PlaneDefinition::PointNormal {
                    origin: [0.0, 0.0, 0.0],
                    normal: [0.0, 0.0, 1.0],
                },
            },
        },
    );
    let error = refused(&mut state, "sketch3d_get", json!({ "feature_id": id }));
    assert_eq!(error["code"], "OperationKindMismatch");
    assert_eq!(error["details"]["expected"], "Sketch3d");
}

#[test]
fn sketch3d_get_says_not_evaluated_rather_than_inventing_an_empty_answer() {
    // A suppressed sketch has no evaluation; answering `chains: []` would read
    // as "this path has no segments", which is a different fact.
    let mut state = EngineState::new();
    let id = add_feature(
        &mut state,
        Operation::Sketch3d {
            sketch: l_with_fillet(),
        },
    );
    ok(
        &mut state,
        "feature_suppress",
        json!({ "feature_id": id, "suppressed": true }),
    );

    let error = refused(&mut state, "sketch3d_get", json!({ "feature_id": id }));
    assert_eq!(error["code"], "NotEvaluated");
}

#[test]
fn the_tool_is_registered_as_read_only() {
    assert!(wasm_bridge::tools::MIGRATED.contains(&"sketch3d_get"));
    assert!(
        !wasm_bridge::tools::mutates("sketch3d_get"),
        "reading a sketch changes nothing"
    );
}
