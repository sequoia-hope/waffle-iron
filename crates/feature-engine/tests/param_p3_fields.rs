//! P3 — every remaining authored numeric field on an operation gets an
//! `*_expr` twin routed through `apply_field` with its own `Dimension`.
//!
//! `specs/agent_mechanical_design.md` §6 P3. One test per site, each in two
//! halves: the expression DRIVES the stored number (through the dimension's
//! own boundary — metres for a length, degrees for an angle, the plain number
//! for a count or a ratio), and an expression of the WRONG dimension is
//! refused by name with the field left alone.
//!
//! The second half is the half that matters. A sidecar that accepts anything
//! is the pre-P1 coercion back again, one field at a time: `25deg` in a
//! translation would be 25 mm, and `20.5` in a count would be 20.
//!
//! Every fixture is built through serde from the document form, so a field
//! this suite forgets is a field the struct does not have.

use feature_engine::params::apply_parameters;
use feature_engine::types::{
    AxisRef, DesignParameter, Feature, FeatureTree, Operation, PlaneDefinition, SecondDirection,
};
use serde_json::{json, Value};
use uuid::Uuid;

/// One tree holding one feature built from `op`, with `parameters`.
fn tree(op: Value, parameters: Vec<DesignParameter>) -> FeatureTree {
    FeatureTree {
        features: vec![Feature {
            id: Uuid::new_v4(),
            name: op["type"].as_str().unwrap().to_string(),
            operation: serde_json::from_value(op.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", op["type"])),
            suppressed: false,
            references: Vec::new(),
        }],
        active_index: None,
        body_names: Default::default(),
        parameters,
        ..Default::default()
    }
}

fn op_of(tree: &FeatureTree) -> &Operation {
    &tree.features[0].operation
}

/// Apply and require it clean.
#[track_caller]
fn applied(t: &mut FeatureTree) {
    let outcome = apply_parameters(t);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

/// Apply and require exactly one error, whose message contains `wants`.
#[track_caller]
fn refused(t: &mut FeatureTree, wants: &str) {
    let outcome = apply_parameters(t);
    assert_eq!(outcome.errors.len(), 1, "{:?}", outcome.errors);
    assert!(
        outcome.errors[0].1.contains(wants),
        "expected {wants:?} in {:?}",
        outcome.errors[0].1
    );
}

#[track_caller]
fn near(got: f64, want: f64) {
    assert!(
        (got - want).abs() < 1e-12,
        "expected {want}, got {got} (off by {})",
        got - want
    );
}

fn p(name: &str, expression: &str) -> DesignParameter {
    DesignParameter::new(name, expression)
}

// ── Extrude: the second direction's blind depth ─────────────────────────────

fn extrude(second: Value) -> Value {
    json!({ "type": "Extrude", "params": {
        "sketch_id": Uuid::new_v4(), "profile_index": 0,
        "depth": 0.004, "symmetric": false, "cut": false,
        "second_direction": second }})
}

fn second_depth(tree: &FeatureTree) -> f64 {
    match op_of(tree) {
        Operation::Extrude { params } => match params.second_direction.as_ref() {
            Some(SecondDirection::Blind { depth, .. }) => *depth,
            other => panic!("expected a blind second direction, got {other:?}"),
        },
        other => panic!("expected an extrude, got {other:?}"),
    }
}

#[test]
fn a_second_direction_depth_is_driven_in_meters() {
    let mut t = tree(
        extrude(json!({ "type": "Blind", "depth": 0.002, "depth_expr": "back" })),
        vec![p("back", "7")],
    );
    applied(&mut t);
    near(second_depth(&t), 0.007);
}

#[test]
fn a_second_direction_depth_refuses_an_angle() {
    let mut t = tree(
        extrude(json!({ "type": "Blind", "depth": 0.002, "depth_expr": "25deg" })),
        Vec::new(),
    );
    refused(&mut t, "second depth expression");
    near(second_depth(&t), 0.002);
}

// ── Revolve: the axis origin ────────────────────────────────────────────────

fn revolve(origin_expr: Value) -> Value {
    json!({ "type": "Revolve", "params": {
        "sketch_id": Uuid::new_v4(), "profile_index": 0,
        "axis_origin": [0.0, 0.0, 0.0],
        "axis_origin_expr": origin_expr,
        "axis_direction": [0.0, 0.0, 1.0],
        "angle": 90.0, "cut": false }})
}

fn axis_origin(tree: &FeatureTree) -> [f64; 3] {
    match op_of(tree) {
        Operation::Revolve { params } => params.axis_origin,
        other => panic!("expected a revolve, got {other:?}"),
    }
}

#[test]
fn a_revolve_axis_origin_is_driven_per_component_in_meters() {
    let mut t = tree(
        revolve(json!(["gap", "gap * 2", null])),
        vec![p("gap", "12")],
    );
    applied(&mut t);
    let got = axis_origin(&t);
    near(got[0], 0.012);
    near(got[1], 0.024);
    near(
        got[2],
        0.0,
        // An unset component is NOT driven: an author who parameterised x and
        // y has said nothing about z, and filling it in would invent geometry.
    );
}

#[test]
fn a_revolve_axis_origin_component_refuses_an_angle_and_names_the_axis() {
    let mut t = tree(revolve(json!([null, null, "25deg"])), Vec::new());
    refused(&mut t, "axis origin z expression");
    near(axis_origin(&t)[2], 0.0);
}

// ── DatumPlane: the point-normal origin ─────────────────────────────────────

fn datum_point_normal(origin_expr: Value) -> Value {
    json!({ "type": "DatumPlane", "params": { "name": "Datum", "definition": {
        "method": "point-normal",
        "origin": [0.0, 0.0, 0.0],
        "origin_expr": origin_expr,
        "normal": [0.0, 0.0, 1.0] }}})
}

fn datum_origin(tree: &FeatureTree) -> [f64; 3] {
    match op_of(tree) {
        Operation::DatumPlane { params } => match &params.definition {
            PlaneDefinition::PointNormal { origin, .. } => *origin,
            other => panic!("expected a point-normal plane, got {other:?}"),
        },
        other => panic!("expected a datum plane, got {other:?}"),
    }
}

#[test]
fn a_point_normal_origin_is_driven_in_meters() {
    let mut t = tree(
        datum_point_normal(json!([null, null, "deck"])),
        vec![p("deck", "30")],
    );
    applied(&mut t);
    near(datum_origin(&t)[2], 0.030);
}

#[test]
fn a_point_normal_origin_refuses_an_angle() {
    let mut t = tree(datum_point_normal(json!([null, null, "1rad"])), Vec::new());
    refused(&mut t, "origin z expression");
    near(datum_origin(&t)[2], 0.0);
}

// ── Patterns: the counts ────────────────────────────────────────────────────

fn pattern_circular(count_expr: Value) -> Value {
    json!({ "type": "PatternCircular", "params": {
        "axis": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                  "direction": [0.0, 0.0, 1.0] },
        "count": 4, "count_expr": count_expr, "angle_deg": 360.0 }})
}

fn circular_count(tree: &FeatureTree) -> u32 {
    match op_of(tree) {
        Operation::PatternCircular { params } => params.count,
        other => panic!("expected a circular pattern, got {other:?}"),
    }
}

#[test]
fn a_pattern_count_is_driven_as_a_whole_number() {
    let mut t = tree(pattern_circular(json!("teeth / 2")), vec![p("teeth", "24")]);
    applied(&mut t);
    assert_eq!(circular_count(&t), 12);
}

#[test]
fn a_pattern_count_refuses_a_fraction_rather_than_truncating() {
    // THE reason a count has its own dimension. Before P3 there was no
    // sidecar at all; the wrong way to add one is `as u32`, which turns
    // `20 / 3` into 6 with nothing to notice.
    let mut t = tree(pattern_circular(json!("teeth / 7")), vec![p("teeth", "20")]);
    refused(&mut t, "whole non-negative count");
    assert_eq!(circular_count(&t), 4, "the count must not change");
}

#[test]
fn a_pattern_count_refuses_a_length_and_a_negative() {
    let mut t = tree(pattern_circular(json!("25mm")), Vec::new());
    refused(&mut t, "count expression");
    assert_eq!(circular_count(&t), 4);

    let mut t = tree(pattern_circular(json!("0 - 3")), Vec::new());
    refused(&mut t, "whole non-negative count");
    assert_eq!(circular_count(&t), 4);
}

#[test]
fn a_pattern_count_too_large_for_a_u32_is_refused_not_saturated() {
    let mut t = tree(pattern_circular(json!("5000000000")), Vec::new());
    refused(&mut t, "too large for a count");
    assert_eq!(circular_count(&t), 4);
}

#[test]
fn both_linear_counts_are_driven() {
    let mut t = tree(
        json!({ "type": "PatternLinear", "params": {
            "direction": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                           "direction": [1.0, 0.0, 0.0] },
            "count": 2, "count_expr": "rows", "spacing": 0.01,
            "second": { "direction": { "method": "explicit",
                                       "origin": [0.0, 0.0, 0.0],
                                       "direction": [0.0, 1.0, 0.0] },
                        "count": 2, "count_expr": "cols", "spacing": 0.02 }}}),
        vec![p("rows", "5"), p("cols", "3")],
    );
    applied(&mut t);
    match op_of(&t) {
        Operation::PatternLinear { params } => {
            assert_eq!(params.count, 5);
            assert_eq!(params.second.as_ref().unwrap().count, 3);
        }
        other => panic!("expected a linear pattern, got {other:?}"),
    }
}

// ── Patterns and the mirror: the explicit axis origin ───────────────────────

fn explicit_origin(axis: &AxisRef) -> [f64; 3] {
    match axis {
        AxisRef::Explicit { origin, .. } => *origin,
        other => panic!("expected an explicit axis, got {other:?}"),
    }
}

#[test]
fn an_explicit_axis_origin_is_driven_on_every_pattern_that_has_one() {
    let origin_expr = json!(["x", null, null]);
    let mut t = tree(
        json!({ "type": "PatternCircular", "params": {
            "axis": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                      "origin_expr": origin_expr, "direction": [0.0, 0.0, 1.0] },
            "count": 4, "angle_deg": 360.0 }}),
        vec![p("x", "15")],
    );
    applied(&mut t);
    match op_of(&t) {
        Operation::PatternCircular { params } => near(explicit_origin(&params.axis)[0], 0.015),
        other => panic!("{other:?}"),
    }

    let mut t = tree(
        json!({ "type": "PatternMirror", "params": {
            "plane": { "method": "explicit", "origin": [0.0, 0.0, 0.0],
                       "origin_expr": origin_expr, "direction": [1.0, 0.0, 0.0] }}}),
        vec![p("x", "15")],
    );
    applied(&mut t);
    match op_of(&t) {
        Operation::PatternMirror { params } => near(explicit_origin(&params.plane)[0], 0.015),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_entity_axis_carries_no_expression_and_is_not_a_site() {
    // An entity axis is DERIVED from the picked geometry every rebuild, so an
    // expression beside it would be a second driver the resolution overwrites.
    let mut t = tree(
        json!({ "type": "PatternCircular", "params": {
            "axis": { "method": "entity", "geom_ref": {
                "kind": { "type": "Face" },
                "anchor": { "type": "Datum", "datum_id": Uuid::new_v4() },
                "selector": { "type": "Position", "x": 0.0, "y": 0.0, "z": 0.0 },
                "policy": { "type": "BestEffort" } }},
            "count": 4, "angle_deg": 360.0 }}),
        Vec::new(),
    );
    let outcome = apply_parameters(&mut t);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(feature_engine::params::field_uses(&mut t).len(), 0);
}

// ── MateConnector: the authored adjustment ──────────────────────────────────

fn mate_connector(extra: Value) -> Value {
    let mut params = json!({
        "name": "C1",
        "frame": { "origin": [0.0, 0.0, 0.0], "z_axis": [0.0, 0.0, 1.0],
                   "x_axis": [1.0, 0.0, 0.0] },
        "rotation_deg": 0.0,
        "offset_m": [0.0, 0.0, 0.0] });
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    json!({ "type": "MateConnector", "params": params })
}

#[test]
fn a_connector_rotation_is_degrees_and_its_offset_is_meters() {
    let mut t = tree(
        mate_connector(json!({ "rotation_expr": "turn", "offset_m_expr": [null, null, "lift"] })),
        vec![p("turn", "90"), p("lift", "4")],
    );
    applied(&mut t);
    match op_of(&t) {
        Operation::MateConnector { params } => {
            near(params.rotation_deg, 90.0);
            near(params.offset_m[2], 0.004);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_connector_rotation_refuses_a_length_and_the_offset_refuses_an_angle() {
    let mut t = tree(
        mate_connector(json!({ "rotation_expr": "1in" })),
        Vec::new(),
    );
    refused(&mut t, "rotation expression");
    let mut t = tree(
        mate_connector(json!({ "offset_m_expr": ["25deg", null, null] })),
        Vec::new(),
    );
    refused(&mut t, "offset x expression");
}

#[test]
fn a_connector_rotation_in_radians_converts_at_the_boundary() {
    // The field stores DEGREES, so `pi rad` is a half turn.
    let mut t = tree(
        mate_connector(json!({ "rotation_expr": "pi * 1rad" })),
        Vec::new(),
    );
    applied(&mut t);
    match op_of(&t) {
        Operation::MateConnector { params } => near(params.rotation_deg, 180.0),
        other => panic!("{other:?}"),
    }
}

// ── ImportedBody: the placement and the scale ───────────────────────────────

fn imported(extra: Value) -> Value {
    let mut params = json!({
        "file_name": "x.step", "source_id": Uuid::new_v4(),
        "translation_m": [0.0, 0.0, 0.0],
        "rotation_deg": [0.0, 0.0, 0.0],
        "scale": 1.0 });
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    json!({ "type": "ImportedBody", "params": params })
}

#[test]
fn an_imported_placement_is_driven_in_its_own_units() {
    let mut t = tree(
        imported(json!({
            "translation_m_expr": ["dx", null, null],
            "rotation_deg_expr": [null, null, "yaw"],
            "scale_expr": "shrink" })),
        vec![p("dx", "25"), p("yaw", "45"), p("shrink", "0.5")],
    );
    applied(&mut t);
    match op_of(&t) {
        Operation::ImportedBody { params } => {
            near(params.translation_m[0], 0.025);
            near(params.rotation_deg[2], 45.0);
            near(params.scale, 0.5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_imported_rotation_is_degrees_and_refuses_a_length() {
    let mut t = tree(
        imported(json!({ "rotation_deg_expr": ["25mm", null, null] })),
        Vec::new(),
    );
    refused(&mut t, "rotation x expression");
}

#[test]
fn an_imported_translation_refuses_an_angle() {
    let mut t = tree(
        imported(json!({ "translation_m_expr": [null, "90deg", null] })),
        Vec::new(),
    );
    refused(&mut t, "translation y expression");
}

#[test]
fn a_scale_is_a_ratio_so_a_length_is_refused_but_a_quotient_is_not() {
    // `Ratio` is the one dimension that says "dimensionless on purpose": a
    // scale of `25mm` means nothing, and a scale of `25mm / 1in` is 0.9842…
    let mut t = tree(imported(json!({ "scale_expr": "25mm" })), Vec::new());
    refused(&mut t, "scale expression");
    match op_of(&t) {
        Operation::ImportedBody { params } => near(params.scale, 1.0),
        other => panic!("{other:?}"),
    }

    let mut t = tree(imported(json!({ "scale_expr": "25mm / 1in" })), Vec::new());
    applied(&mut t);
    match op_of(&t) {
        Operation::ImportedBody { params } => near(params.scale, 25.0 / 25.4),
        other => panic!("{other:?}"),
    }
}

// ── the shape of the whole increment ───────────────────────────────────────

#[test]
fn every_new_sidecar_is_omitted_when_absent_so_no_document_changes_bytes() {
    // P3 adds ten sidecars across six operations. Every one is
    // `skip_serializing_if`, so a document that uses none of them writes
    // exactly the bytes it wrote before — the claim the corpus pins can only
    // confirm if it is true by construction.
    for op in [
        extrude(json!({ "type": "Blind", "depth": 0.002 })),
        revolve(Value::Null),
        datum_point_normal(Value::Null),
        pattern_circular(Value::Null),
        mate_connector(json!({})),
        imported(json!({})),
    ] {
        let operation: Operation = serde_json::from_value(op.clone()).unwrap();
        let written = serde_json::to_value(&operation).unwrap();
        let text = written.to_string();
        for key in [
            "depth_expr",
            "axis_origin_expr",
            "origin_expr",
            "count_expr",
            "rotation_expr",
            "offset_m_expr",
            "translation_m_expr",
            "rotation_deg_expr",
            "scale_expr",
        ] {
            assert!(
                !text.contains(key),
                "{} wrote {key} while it holds none: {text}",
                op["type"]
            );
        }
    }
}

#[test]
fn a_rename_reaches_every_new_sidecar_too() {
    // `expression_sites` is what a rename walks, so a site added to the apply
    // pass and not to the enumeration would leave its field reading a name
    // the document no longer has. The drift oracle in `params.rs` counts the
    // sites; this checks the rewrite actually lands on the new ones.
    let mut t = tree(
        imported(json!({
            "translation_m_expr": ["dx", "dx * 2", null],
            "scale_expr": "dx / 100" })),
        vec![p("dx", "25")],
    );
    applied(&mut t);
    let edits = feature_engine::params::rename_parameter(&mut t, "dx", "offset_x");
    assert_eq!(edits.len(), 3, "{edits:#?}");
    match op_of(&t) {
        Operation::ImportedBody { params } => {
            assert_eq!(
                params.translation_m_expr.as_ref().unwrap()[0].as_deref(),
                Some("offset_x")
            );
            assert_eq!(
                params.translation_m_expr.as_ref().unwrap()[1].as_deref(),
                Some("offset_x * 2")
            );
            assert_eq!(
                params.translation_m_expr.as_ref().unwrap()[2].as_deref(),
                None,
                "an unset component must not be invented"
            );
            assert_eq!(params.scale_expr.as_deref(), Some("offset_x / 100"));
        }
        other => panic!("{other:?}"),
    }
    applied(&mut t);
}
