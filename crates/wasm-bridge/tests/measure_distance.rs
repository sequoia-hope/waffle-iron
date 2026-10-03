//! Q1 of `specs/agent_mechanical_design.md` §4.3 end to end: the
//! `measure_distance` tool over the real kernel.
//!
//! The §4.4 oracle, through every layer a host actually crosses — tool args →
//! `UiToEngine::MeasureDistance` → `KernelMeasure` → `kernel_v2::measure`:
//! two 10 mm boxes 15 mm apart measure their analytic gap, `exact`; the same
//! two with one body replaced by a point measure the point's height; an
//! `along` query reports the directional gap; and the operands Q1 does not
//! measure (an axis) come back as a CAPABILITY refusal, not a number.

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

fn datum_xy() -> GeomRef {
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

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, a: u32, b: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: a,
        end_id: b,
        construction: false,
    }
}

fn added_id(response: EngineToUi) -> Uuid {
    match response {
        EngineToUi::ModelUpdated {
            feature_id: Some(id),
            errors,
            ..
        } => {
            assert!(errors.is_empty(), "rebuild errors: {errors:?}");
            id
        }
        other => panic!("expected ModelUpdated with an id, got {other:?}"),
    }
}

/// A 10 mm cube whose near corner sits at `(x0, 0, 0)`, as its own body.
/// Returns its body id.
fn cube(state: &mut EngineState, kernel: &mut KernelV2Adapter, x0: f64, base: u32) -> String {
    let s = 0.01;
    let corners = [
        (base, x0, 0.0),
        (base + 1, x0 + s, 0.0),
        (base + 2, x0 + s, s),
        (base + 3, x0, s),
    ];
    let solved_positions: HashMap<u32, (f64, f64)> =
        corners.iter().map(|&(id, x, y)| (id, (x, y))).collect();
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    let (l0, l1, l2, l3) = (base + 10, base + 11, base + 12, base + 13);
    entities.extend([
        line(l0, base, base + 1),
        line(l1, base + 1, base + 2),
        line(l2, base + 2, base + 3),
        line(l3, base + 3, base),
    ]);
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: datum_xy(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: Vec::new(),
        plane_face: None,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![l0, l1, l2, l3],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    };
    let sketch_feature = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Sketch { sketch },
            provenance: None,
        },
        kernel,
    ));
    let extrude = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch_feature,
                    profile_index: 0,
                    profile_entity_ids: Some(vec![l0, l1, l2, l3]),
                    depth: s,
                    depth_expr: None,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: false,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: Some(CombineMode::NewBody),
                    targets: None,
                },
            },
            provenance: None,
        },
        kernel,
    ));
    // The host tessellates after every dispatch; the tools that list or
    // measure a body require a rendered one.
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    FeatureTree::body_id(extrude, &OutputKey::Main)
}

fn tool(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    args: Value,
) -> wasm_bridge::ToolResult {
    execute_tool(state, kernel, "measure_distance", &args, None)
}

fn ok(state: &mut EngineState, kernel: &mut KernelV2Adapter, args: Value) -> Value {
    let r = tool(state, kernel, args);
    assert!(!r.is_error, "measure_distance failed: {r:?}");
    r.structured_content
}

#[test]
fn two_boxes_measure_their_gap_through_the_tool_and_say_it_is_exact() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    // Cubes of 10 mm at x = 0 and x = 25 mm: a 15 mm gap.
    let a = cube(&mut state, &mut kernel, 0.0, 1);
    let b = cube(&mut state, &mut kernel, 0.025, 101);

    let out = ok(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "body", "body_id": a },
            "b": { "type": "body", "body_id": b },
        }),
    );
    let d = out["distance_m"].as_f64().expect("a distance");
    assert!((d - 0.015).abs() < 1e-15, "{out}");
    assert_eq!(out["method"], "exact", "{out}");
    assert!(
        out.get("chord_bound_m").is_none(),
        "an exact answer carries no band: {out}"
    );
    // The two points realize the distance, and each names the face it lies
    // on. (The sketch frame decides which world axis the cubes are separated
    // along, so the test reads the points rather than assuming one.)
    let coords = |v: &Value| -> [f64; 3] {
        let a = v.as_array().expect("a point");
        [
            a[0].as_f64().unwrap(),
            a[1].as_f64().unwrap(),
            a[2].as_f64().unwrap(),
        ]
    };
    let (pa, pb) = (coords(&out["points"][0]), coords(&out["points"][1]));
    let span = ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt();
    assert!(
        (span - d).abs() < 1e-15,
        "the two points are `distance_m` apart: {out}"
    );
    assert_eq!(out["on"][0]["kind"], json!({ "type": "Face" }), "{out}");
    assert_eq!(out["on"][1]["kind"], json!({ "type": "Face" }), "{out}");

    // Symmetric through the tool as well.
    let back = ok(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "body", "body_id": b },
            "b": { "type": "body", "body_id": a },
        }),
    );
    assert_eq!(back["distance_m"], out["distance_m"], "{back}");

    // A point operand: 40 mm above the first cube's top face (z = 10 mm).
    let out = ok(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "point", "point": [0.005, -0.005, 0.05] },
            "b": { "type": "body", "body_id": a },
        }),
    );
    assert!(
        (out["distance_m"].as_f64().unwrap() - 0.04).abs() < 1e-15,
        "a point 40 mm above the 10 mm cube's top face: {out}"
    );
    assert_eq!(out["method"], "exact");
    assert_eq!(out["on"][0], Value::Null, "a free point lies on nothing");

    // `along`. The datum plane's derived frame maps the sketch's u axis to
    // world −y, so the two cubes are separated along y and share their x
    // span: the gap along y is the 15 mm, and along x they OVERLAP by their
    // own 10 mm, which the answer reports as negative.
    let out = ok(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "body", "body_id": a },
            "b": { "type": "body", "body_id": b },
            "along": [0.0, 1.0, 0.0],
        }),
    );
    assert!(
        (out["distance_m"].as_f64().unwrap() - 0.015).abs() < 1e-15,
        "the gap along the separation axis: {out}"
    );
    let out = ok(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "body", "body_id": a },
            "b": { "type": "body", "body_id": b },
            "along": [1.0, 0.0, 0.0],
        }),
    );
    assert!(
        (out["distance_m"].as_f64().unwrap() + 0.01).abs() < 1e-15,
        "an overlap along the shared axis is negative: {out}"
    );
}

#[test]
fn a_face_reference_from_face_list_is_a_measurement_operand() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let a = cube(&mut state, &mut kernel, 0.0, 1);
    let b = cube(&mut state, &mut kernel, 0.025, 101);

    // Take the +z (top) face of the first cube, the way an agent would.
    let faces = execute_tool(
        &mut state,
        &mut kernel,
        "face_list",
        &json!({ "body_id": a }),
        None,
    );
    assert!(!faces.is_error, "{faces:?}");
    let listed = faces.structured_content["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .find(|f| {
            f["signature"]["normal"]
                .as_array()
                .and_then(|n| n[2].as_f64())
                .is_some_and(|z| z > 0.9)
        })
        .expect("a +z face")
        .clone();

    let out = ok(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "entity", "geom_ref": listed["geom_ref"] },
            "b": { "type": "body", "body_id": b },
        }),
    );
    // The top face's nearest point to the far cube is its +x edge, 15 mm away.
    assert!(
        (out["distance_m"].as_f64().unwrap() - 0.015).abs() < 1e-12,
        "{out}"
    );
    assert_eq!(out["method"], "exact", "{out}");
}

#[test]
fn an_axis_operand_is_a_capability_refusal_not_a_number() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let a = cube(&mut state, &mut kernel, 0.0, 1);

    let r = tool(
        &mut state,
        &mut kernel,
        json!({
            "a": { "type": "body", "body_id": a },
            "b": { "type": "axis", "origin": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 1.0] },
        }),
    );
    assert!(r.is_error, "an axis operand must refuse: {r:?}");
    let message = r.structured_content["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        message.contains("axis") || message.contains("operand"),
        "the refusal names what it cannot measure: {message}"
    );

    // A missing operand is an argument error, named.
    let r = tool(
        &mut state,
        &mut kernel,
        json!({ "a": { "type": "body", "body_id": a } }),
    );
    assert!(r.is_error, "{r:?}");
    assert_eq!(
        r.structured_content["error"]["code"], "InvalidArguments",
        "{:?}",
        r.structured_content
    );
}
