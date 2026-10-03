//! D2 end to end against real geometry (`specs/drawings_and_mbd.md` §6; P4
//! of `specs/agent_mechanical_design.md` §6).
//!
//! The headline claim of the increment is that a measurement can DRIVE a
//! dimension: a boss between two walls whose depth is
//! `distance(wall_a, wall_b) / 2` rebuilds to half the measured gap, and
//! moving a wall moves the boss. That claim needs a kernel that can actually
//! measure, so it is pinned here rather than in feature-engine, whose mock
//! answers `KernelMeasure` with `NotSupported`.
//!
//! Also pinned here: `expression_evaluate` on a measurement (the preview and
//! the rebuild must agree, so they go through one measurer), and that `mass`
//! refuses by name until M1.

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

/// A square block of side `s` whose near corner is at `(x0, 0)`, extruded
/// `depth` — or driven by `depth_expr`. Returns (feature id, body id).
fn block(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    x0: f64,
    s: f64,
    depth: f64,
    depth_expr: Option<&str>,
    base: u32,
) -> (Uuid, String) {
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
        plane_face: None,
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: Vec::new(),
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
                    depth,
                    depth_expr: depth_expr.map(str::to_string),
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
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    (extrude, FeatureTree::body_id(extrude, &OutputKey::Main))
}

fn call(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    name: &str,
    args: Value,
) -> wasm_bridge::ToolResult {
    execute_tool(state, kernel, name, &args, None)
}

fn ok(state: &mut EngineState, kernel: &mut KernelV2Adapter, name: &str, args: Value) -> Value {
    let r = call(state, kernel, name, args);
    assert!(!r.is_error, "{name} failed: {r:?}");
    r.structured_content
}

/// The centre of a body's bounding box.
fn centre(state: &mut EngineState, kernel: &mut KernelV2Adapter, body: &str) -> [f64; 3] {
    let m = ok(state, kernel, "body_measure", json!({ "body_id": body }));
    let mid = |k: usize| {
        (m["bbox_min"][k].as_f64().expect("a bound") + m["bbox_max"][k].as_f64().expect("a bound"))
            / 2.0
    };
    [mid(0), mid(1), mid(2)]
}

/// The reference to the face of `body` whose outward normal points most
/// nearly along `dir`.
///
/// Normals, not centroid coordinates: the sketch frame decides which world
/// axis the blocks are separated along AND which way round, so "the face
/// pointing at the other block" is the only description that does not depend
/// on either. N0 fills `signature.normal` for every planar face.
fn facing_face(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    body: &str,
    dir: [f64; 3],
) -> Value {
    let listed = ok(state, kernel, "face_list", json!({ "body_id": body }));
    let faces = listed["faces"].as_array().expect("faces").clone();
    assert!(!faces.is_empty(), "the block has faces: {listed}");
    let score = |f: &Value| -> f64 {
        let n = &f["signature"]["normal"];
        (0..3)
            .map(|k| n[k].as_f64().unwrap_or(0.0) * dir[k])
            .sum::<f64>()
    };
    let best = faces
        .iter()
        .max_by(|a, b| score(a).total_cmp(&score(b)))
        .expect("a face");
    assert!(
        score(best) > 0.0,
        "no face of {body} points along {dir:?}: {listed}"
    );
    best["geom_ref"].clone()
}

fn name_it(state: &mut EngineState, kernel: &mut KernelV2Adapter, face: Value, name: &str) {
    ok(
        state,
        kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": name }),
    );
}

fn depth_of(state: &EngineState, feature: Uuid) -> f64 {
    match &state
        .engine
        .tree
        .find_feature(feature)
        .expect("the feature exists")
        .operation
    {
        Operation::Extrude { params } => params.depth,
        other => panic!("expected an extrude, got {other:?}"),
    }
}

/// Two 10 mm walls with a gap between them, their facing faces named
/// `wall_a` and `wall_b`. Returns the gap in metres and the two feature ids.
fn two_walls(state: &mut EngineState, kernel: &mut KernelV2Adapter, gap: f64) -> (f64, Uuid, Uuid) {
    let s = 0.01;
    let (fa, a) = block(state, kernel, 0.0, s, s, None, 1);
    let (fb, b) = block(state, kernel, s + gap, s, s, None, 101);
    // The facing pair: each block's face that points at the other.
    let ca = centre(state, kernel, &a);
    let cb = centre(state, kernel, &b);
    let a_to_b = [cb[0] - ca[0], cb[1] - ca[1], cb[2] - ca[2]];
    let b_to_a = [-a_to_b[0], -a_to_b[1], -a_to_b[2]];
    let face_a = facing_face(state, kernel, &a, a_to_b);
    let face_b = facing_face(state, kernel, &b, b_to_a);
    name_it(state, kernel, face_a, "wall_a");
    name_it(state, kernel, face_b, "wall_b");
    // The named pair must be the gap itself, or the rest of the test is
    // measuring something else.
    let d = ok(
        state,
        kernel,
        "measure_distance",
        json!({ "a": { "type": "name", "name": "wall_a" },
                "b": { "type": "name", "name": "wall_b" } }),
    );
    let measured = d["distance_m"].as_f64().expect("a distance");
    assert!(
        (measured - gap).abs() < 1e-12,
        "the named faces are {measured} m apart, not the {gap} m gap: {d}"
    );
    (gap, fa, fb)
}

#[test]
fn a_measurement_drives_a_depth_to_the_measured_value() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (gap, _, _) = two_walls(&mut state, &mut kernel, 0.015);

    // The boss: a third block whose depth is half the measured gap. Its
    // stored `depth` is deliberately WRONG (1 mm), so a depth that comes out
    // right can only have come from the measurement.
    let (boss, _) = block(
        &mut state,
        &mut kernel,
        0.05,
        0.004,
        0.001,
        Some("distance(wall_a, wall_b) / 2"),
        201,
    );
    assert!(
        state.engine.errors.is_empty(),
        "the measured rebuild must be clean: {:?}",
        state.engine.errors
    );
    let expected = gap / 2.0;
    let measured = depth_of(&state, boss);
    assert!(
        (measured - expected).abs() < 1e-12,
        "depth {measured} should be half the {gap} m gap ({expected})"
    );

    // And it TRACKS: widening the gap moves the boss. This is what makes it
    // a measurement rather than a one-time read.
    let mut state2 = EngineState::new();
    let mut kernel2 = KernelV2Adapter::new();
    let (gap2, _, _) = two_walls(&mut state2, &mut kernel2, 0.03);
    let (boss2, _) = block(
        &mut state2,
        &mut kernel2,
        0.05,
        0.004,
        0.001,
        Some("distance(wall_a, wall_b) / 2"),
        201,
    );
    let measured2 = depth_of(&state2, boss2);
    assert!(
        (measured2 - gap2 / 2.0).abs() < 1e-12,
        "depth {measured2} should be half the {gap2} m gap"
    );
    assert!(
        measured2 > measured,
        "a wider gap is a deeper boss: {measured2} vs {measured}"
    );
}

#[test]
fn a_measuring_parameter_drives_a_depth_and_reports_its_dimension() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (gap, _, _) = two_walls(&mut state, &mut kernel, 0.015);

    // The same thing one level of indirection away: a PARAMETER measures,
    // and the depth reads the parameter. The parameter has no position in
    // the tree, so its ordering is checked at its reader — the boss, which
    // is after both walls, so this is legal.
    let response = dispatch(
        &mut state,
        UiToEngine::SetParameters {
            parameters: vec![DesignParameter::new("gap", "distance(wall_a, wall_b)")],
            renames: Vec::new(),
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "{response:?}"
    );
    let (boss, _) = block(
        &mut state,
        &mut kernel,
        0.05,
        0.004,
        0.001,
        Some("gap / 3"),
        201,
    );
    let measured = depth_of(&state, boss);
    assert!(
        (measured - gap / 3.0).abs() < 1e-12,
        "depth {measured} should be a third of the {gap} m gap"
    );

    // `parameters_get` reports the measured value in the working space
    // (mm) with the dimension the measurement committed.
    let table = ok(&mut state, &mut kernel, "parameters_get", json!({}));
    let row = table["parameters"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|p| p["name"] == "gap")
        .expect("the row")
        .clone();
    assert!(
        row["error"].is_null(),
        "a measuring parameter resolves: {row}"
    );
    assert!(
        (row["value_mm"].as_f64().expect("a value") - gap * 1000.0).abs() < 1e-9,
        "{row}"
    );
    assert_eq!(row["dimension"]["label"], "length", "{row}");
    assert_eq!(
        row["dimension"]["committed"], true,
        "a measurement commits its dimension, unlike a bare number: {row}"
    );
    assert!(
        row["depends_on"].as_array().expect("a list").is_empty(),
        "an entity name is not a parameter dependency: {row}"
    );
    // But the row must still SAY it reads the model, in the other namespace.
    // `depends_on: []` alone would tell an agent this parameter is a
    // constant it may reorder the tree under.
    assert_eq!(
        row["measures"],
        json!(["wall_a", "wall_b"]),
        "the entity namespace is reported as its own list: {row}"
    );
    // And a parameter that measures nothing carries no such list at all, so
    // a document that does not measure answers as it did before D2.
    let plain = table["parameters"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|p| p["name"] != "gap")
        .cloned();
    if let Some(plain) = plain {
        assert!(plain["measures"].is_null(), "{plain}");
    }
}

#[test]
fn expression_evaluate_measures_the_live_model() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (gap, _, _) = two_walls(&mut state, &mut kernel, 0.015);

    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "distance(wall_a, wall_b) / 2" }),
    );
    assert!(out["error"].is_null(), "{out}");
    assert!(
        (out["value_mm"].as_f64().expect("a value") - gap * 1000.0 / 2.0).abs() < 1e-9,
        "the preview reports working-space mm: {out}"
    );
    assert_eq!(out["dimension"], "length", "{out}");

    // A dimension the caller names and the expression cannot produce is
    // refused here, where a dialog can show it, rather than at the rebuild.
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "area(wall_a)", "dimension": "Length" }),
    );
    assert!(
        out["error"]
            .as_str()
            .is_some_and(|e| e.contains("length^2")),
        "an area is not a length: {out}"
    );

    // A name the document does not have refuses by name, never by zero.
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "distance(wall_a, nowhere)" }),
    );
    let err = out["error"].as_str().expect("an error");
    assert!(
        err.contains("nowhere") && err.contains("distance"),
        "the function and the name: {err}"
    );

    // `mass` is in the grammar (so the spelling cannot drift) and refuses
    // until M1 gives it a density.
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "mass(wall_a)" }),
    );
    let err = out["error"].as_str().expect("an error");
    assert!(err.contains("M1") && err.contains("mass"), "{err}");
}

#[test]
fn an_area_and_an_edge_length_measure_the_real_geometry() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let s = 0.01;
    let (_, body) = block(&mut state, &mut kernel, 0.0, s, s, None, 1);
    let c = centre(&mut state, &mut kernel, &body);
    let out = [c[0] + 1.0, c[1], c[2]];
    let face = facing_face(&mut state, &mut kernel, &body, out);
    name_it(&mut state, &mut kernel, face, "top");

    // A 10 mm square face is 100 mm² — and the evaluator's working space for
    // an area is mm², so that is the number.
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "area(top)" }),
    );
    assert!(out["error"].is_null(), "{out}");
    assert!(
        (out["value_mm"].as_f64().expect("a value") - 100.0).abs() < 1e-9,
        "{out}"
    );
    assert_eq!(out["dimension"], "length^2", "{out}");

    // `sqrt` of it is the side, which IS a length a depth would take.
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "sqrt(area(top))", "dimension": "Length" }),
    );
    assert!(out["error"].is_null(), "{out}");
    assert!(
        (out["value_mm"].as_f64().expect("a value") - 10.0).abs() < 1e-9,
        "{out}"
    );

    // An edge of that face: 10 mm of arc length.
    let listed = ok(
        &mut state,
        &mut kernel,
        "entity_list",
        json!({ "body_id": body, "kind": "edge" }),
    );
    let edge = listed["entities"]
        .as_array()
        .expect("edges")
        .first()
        .expect("at least one edge")
        .clone();
    name_it(&mut state, &mut kernel, edge["geom_ref"].clone(), "rim");
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "length(rim)" }),
    );
    assert!(out["error"].is_null(), "{out}");
    assert!(
        (out["value_mm"].as_f64().expect("a value") - 10.0).abs() < 1e-9,
        "every edge of a 10 mm cube is 10 mm: {out}"
    );
    assert_eq!(out["dimension"], "length", "{out}");

    // And `volume` of the body, in mm³.
    ok(
        &mut state,
        &mut kernel,
        "body_rename",
        json!({ "body_id": body, "new_name": "cube" }),
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "expression_evaluate",
        json!({ "expression": "volume(cube)" }),
    );
    assert!(out["error"].is_null(), "{out}");
    assert!(
        (out["value_mm"].as_f64().expect("a value") - 1000.0).abs() < 1e-6,
        "a 10 mm cube is 1000 mm³: {out}"
    );
    assert_eq!(out["dimension"], "length^3", "{out}");
}

#[test]
fn a_feature_measuring_its_own_output_is_a_typed_cycle_against_the_real_kernel() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let s = 0.01;
    let (feature, body) = block(&mut state, &mut kernel, 0.0, s, s, None, 1);
    let c = centre(&mut state, &mut kernel, &body);
    let out = [c[0] + 1.0, c[1], c[2]];
    let face = facing_face(&mut state, &mut kernel, &body, out);
    name_it(&mut state, &mut kernel, face, "own_top");

    // Edit the block's OWN depth to measure its OWN face.
    let mut op = state
        .engine
        .tree
        .find_feature(feature)
        .expect("there")
        .operation
        .clone();
    if let Operation::Extrude { params } = &mut op {
        params.depth_expr = Some("sqrt(area(own_top))".to_string());
    }
    let response = dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: feature,
            operation: op,
            provenance: None,
        },
        &mut kernel,
    );
    let EngineToUi::ModelUpdated { errors, .. } = &response else {
        panic!("expected ModelUpdated, got {response:?}");
    };
    let joined = errors
        .iter()
        .map(|(_, m)| m.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        joined.contains("circular measurement"),
        "expected a typed cycle, got: {joined}"
    );
    // The depth is untouched, so the body is still the one it was.
    assert_eq!(depth_of(&state, feature), s);
}
