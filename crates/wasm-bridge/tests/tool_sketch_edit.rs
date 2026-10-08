//! `sketch_edit` and `sketch_solve_state` — the S3 sketch door
//! (`specs/agent_mechanical_design.md` §10.3).
//!
//! What these pin, which nothing else can:
//!
//! - **The operations reach a STORED sketch.** S1 put trim/extend/offset/
//!   fillet/mirror/project in Rust and S1's own tests drive them as pure
//!   functions; the bridge message `ApplySketchOps` drives them on the UI's
//!   LIVE sketch. Neither touches a sketch in the tree, which is the only
//!   thing an agent has.
//! - **The index space of the report.** Every index the solver reports indexes
//!   the array it was handed, and `sketch_edit` hands it the sketch's own
//!   constraints followed by a `MovePoint`'s transient pin. An off-by-one here
//!   points an agent at the wrong constraint, which is exactly the defect S2
//!   removed from three other callers.
//! - **One undo step per call**, including a batch that adds, removes and
//!   changes geometry at once.

use feature_engine::types::*;
use serde_json::{json, Value};
use waffle_types::kernel::MockKernel;
use wasm_bridge::*;

const AGENT: &str = "sketch-edit-test";

fn call(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": AGENT });
    execute_tool(state, &mut kernel, name, &args, Some(&context))
}

fn ok(state: &mut EngineState, name: &str, args: Value) -> Value {
    let result = call(state, name, args);
    assert!(!result.is_error, "{name} failed: {result:?}");
    result.structured_content
}

fn refused(state: &mut EngineState, name: &str, args: Value) -> Value {
    let result = call(state, name, args);
    assert!(result.is_error, "expected {name} to refuse: {result:?}");
    result.structured_content["error"].clone()
}

fn point(id: u32, x: f64, y: f64) -> Value {
    json!({ "type": "Point", "id": id, "x": x, "y": y })
}

fn line(id: u32, start: u32, end: u32) -> Value {
    json!({ "type": "Line", "id": id, "start_id": start, "end_id": end })
}

/// A 20 × 10 mm rectangle: points 1–4 counter-clockwise from the origin,
/// lines 5–8.
fn rectangle() -> Vec<Value> {
    vec![
        point(1, 0.0, 0.0),
        point(2, 0.02, 0.0),
        point(3, 0.02, 0.01),
        point(4, 0.0, 0.01),
        line(5, 1, 2),
        line(6, 2, 3),
        line(7, 3, 4),
        line(8, 4, 1),
    ]
}

/// The world XY plane with its +x pinned, so a position in the answer is a
/// position the caller can predict.
fn xy() -> Value {
    json!({ "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0], "x_axis": [1.0, 0.0, 0.0] })
}

/// A committed sketch's feature id, with the given entities and constraints.
fn sketch(state: &mut EngineState, entities: Vec<Value>, constraints: Vec<Value>) -> String {
    let out = ok(
        state,
        "sketch_create",
        json!({ "plane": xy(), "entities": entities, "constraints": constraints }),
    );
    out["feature_id"]
        .as_str()
        .expect("a feature id")
        .to_string()
}

/// The stored sketch of a feature, read out of the tree.
fn stored(state: &EngineState, feature_id: &str) -> waffle_types::Sketch {
    let feature = state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id.to_string() == feature_id)
        .expect("the feature");
    match &feature.operation {
        Operation::Sketch { sketch } => sketch.clone(),
        other => panic!("expected a Sketch, got {other:?}"),
    }
}

fn position(state: &EngineState, feature_id: &str, point: u32) -> (f64, f64) {
    *stored(state, feature_id)
        .solved_positions
        .get(&point)
        .unwrap_or_else(|| panic!("no solved position for point {point}"))
}

// ── sketch_solve_state ──────────────────────────────────────────────────

#[test]
fn a_free_rectangle_reports_eight_degrees_of_freedom_and_names_them() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);

    let out = ok(
        &mut state,
        "sketch_solve_state",
        json!({ "feature_id": id }),
    );
    assert_eq!(out["solve_status"], "UnderConstrained");
    // Four points, two parameters each, nothing fixing any of them.
    assert_eq!(out["dof"], 8);
    let s = &out["state"];
    assert_eq!(s["params"], 8);
    assert_eq!(s["rank"], 0);
    assert_eq!(s["rows"], 0);
    assert_eq!(s["constraints"], 0);
    assert_eq!(s["conflicts"], json!([]));
    assert_eq!(s["redundant"], json!([]));
    // `free.len() == dof` is the S2 contract, and it is the whole value of the
    // field: a count with no directions is the `dof` number again.
    assert_eq!(s["free"].as_array().expect("free").len(), 8);
    // Nothing moved: there was nothing to satisfy.
    assert_eq!(s["moved"], json!([]));
    assert_eq!(s["positions"]["2"], json!([0.02, 0.0]));
}

#[test]
fn a_fully_constrained_rectangle_reports_zero_dof_and_no_free_directions() {
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![
            json!({ "type": "Pinned", "point": 1, "x": 0.0, "y": 0.0 }),
            json!({ "type": "Horizontal", "entity": 5 }),
            json!({ "type": "Horizontal", "entity": 7 }),
            json!({ "type": "Vertical", "entity": 6 }),
            json!({ "type": "Vertical", "entity": 8 }),
            json!({ "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.02 }),
            json!({ "type": "VDistance", "point_a": 1, "point_b": 4, "value": 0.01 }),
        ],
    );

    let out = ok(
        &mut state,
        "sketch_solve_state",
        json!({ "feature_id": id }),
    );
    assert_eq!(out["solve_status"], "FullyConstrained");
    assert_eq!(out["dof"], 0);
    let s = &out["state"];
    assert_eq!(s["free"], json!([]));
    assert_eq!(s["constraints"], 7);
    assert_eq!(s["residuals"].as_array().expect("residuals").len(), 7);
    for row in s["residuals"].as_array().unwrap() {
        assert_eq!(row["satisfied"], json!(true), "row {row}");
    }
    // One closed loop, 20 mm × 10 mm.
    let regions = out["regions"].as_array().expect("regions");
    assert_eq!(regions.len(), 1);
    let area = regions[0]["area_m2"].as_f64().expect("an area");
    assert!((area - 2.0e-4).abs() < 1e-12, "area {area}");
}

#[test]
fn a_reference_dimension_is_reported_and_never_an_offender() {
    // The S2 index-space contract, through this door: a reference dimension
    // occupies its own index in the array the report describes, so the
    // constraint a `conflicts` entry names is the one the caller declared
    // there. Before S3 `sketch_create` filtered references out before the
    // solve, and every index came back one short per reference ahead of it.
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![
            // index 0: a reference dimension, deliberately first.
            json!({ "type": "Distance", "entity_a": 1, "entity_b": 2, "value": 0.02, "reference": true }),
            // index 1: the real constraint.
            json!({ "type": "Horizontal", "entity": 5 }),
        ],
    );

    let out = ok(
        &mut state,
        "sketch_solve_state",
        json!({ "feature_id": id }),
    );
    let rows = out["state"]["residuals"].as_array().expect("residuals");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[0]["kind"], "Distance");
    assert_eq!(rows[0]["reference"], json!(true));
    assert_eq!(rows[1]["index"], 1);
    assert_eq!(rows[1]["kind"], "Horizontal");
    assert_eq!(rows[1]["reference"], json!(false));
    // One driving row: the reference dimension does not constrain.
    assert_eq!(out["state"]["rows"], 1);
    assert_eq!(out["dof"], 7);
}

#[test]
fn the_state_of_a_feature_that_is_not_a_sketch_is_refused_by_kind() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    let extrude = ok(
        &mut state,
        "feature_add",
        json!({
            "operation": {
                "type": "Extrude",
                "params": {
                    "sketch_id": id,
                    "profile_index": 0,
                    "depth": 0.005,
                    "cut": false,
                    "merge": false,
                    "symmetric": false,
                    "depth_mode": { "type": "Blind" },
                    "combine": { "type": "NewBody" }
                }
            },
            "on_error": "keep"
        }),
    );
    let extrude_id = extrude["feature_id"].as_str().expect("a feature id");

    let error = refused(
        &mut state,
        "sketch_solve_state",
        json!({ "feature_id": extrude_id }),
    );
    assert_eq!(error["code"], "OperationKindMismatch");
    assert_eq!(error["details"]["got"], "Extrude");
}

// ── sketch_edit: the operations ─────────────────────────────────────────

#[test]
fn adding_an_entity_with_id_zero_gets_an_id_and_the_answer_says_which() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);

    let out = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [
                { "type": "AddEntity", "entity": { "type": "Point", "id": 0, "x": 0.03, "y": 0.0 } },
            ],
        }),
    );

    let added = out["edit"]["added"].as_array().expect("added");
    assert_eq!(added.len(), 1);
    assert_eq!(added[0]["type"], "Point");
    let minted = added[0]["id"].as_u64().expect("an id") as u32;
    // Past every id the sketch already holds (1–8), so nothing collides.
    assert!(minted > 8, "minted {minted}");
    assert_eq!(out["features_changed"], json!([id]));
    assert_eq!(
        stored(&state, &id).entities.len(),
        9,
        "the point is in the committed sketch"
    );
    // A loose point adds two freedoms.
    assert_eq!(out["dof"], 10);
}

#[test]
fn a_constraint_added_through_an_op_drives_the_solve_it_shares() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    // Point 2 is 0.02 across and 0 up; a Vertical on line 5 (1→2) must bring
    // the two onto one x.
    let out = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "AddConstraint", "constraint": { "type": "Vertical", "entity": 5 } }],
        }),
    );

    assert_eq!(out["state"]["constraints"], 1);
    assert_eq!(out["dof"], 7);
    let (x1, _) = position(&state, &id, 1);
    let (x2, _) = position(&state, &id, 2);
    assert!(
        (x1 - x2).abs() < 1e-9,
        "the line is vertical: x1 {x1}, x2 {x2}"
    );
    // The solve moved something, and the report says which point.
    let moved = out["state"]["moved"].as_array().expect("moved");
    assert!(!moved.is_empty(), "a vertical constraint moved a point");
}

#[test]
fn removing_an_entity_cascades_to_the_constraints_that_named_it() {
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![
            json!({ "type": "Horizontal", "entity": 5 }),
            json!({ "type": "Vertical", "entity": 6 }),
        ],
    );

    let out = ok(
        &mut state,
        "sketch_edit",
        json!({ "feature_id": id, "ops": [{ "type": "RemoveEntity", "ids": [6] }] }),
    );

    // The line, and the Vertical that named it. The rectangle's corner points
    // survive: each is still an endpoint of a surviving line.
    assert_eq!(out["edit"]["removed"], json!([6]));
    assert_eq!(out["edit"]["constraints_removed"], json!([1]));
    let after = stored(&state, &id);
    assert!(!after.entities.iter().any(|e| e.id() == 6));
    assert_eq!(after.constraints.len(), 1);
    assert_eq!(out["state"]["constraints"], 1);
}

#[test]
fn a_move_point_is_a_drag_that_leaves_no_constraint_behind() {
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![json!({ "type": "Horizontal", "entity": 5 })],
    );

    let out = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "MovePoint", "id": 2, "to": [0.05, 0.004] }],
        }),
    );

    // The pin drove this solve and is named as transient, at the index it had
    // in the array the solver saw — one past the sketch's own constraint.
    assert_eq!(out["state"]["constraints"], 1);
    let transient = out["state"]["transient_constraints"]
        .as_array()
        .expect("the pin is reported");
    assert_eq!(transient.len(), 1);
    assert_eq!(transient[0]["index"], 1);
    assert_eq!(transient[0]["kind"], "Pinned");

    // The committed sketch holds only the Horizontal: a drag is not a
    // constraint.
    let after = stored(&state, &id);
    assert_eq!(after.constraints.len(), 1);
    assert_eq!(after.constraints[0].kind(), "Horizontal");

    // Point 2 landed where it was asked for in x; the Horizontal pulled the
    // line flat rather than letting the point keep its y.
    let (x2, y2) = position(&state, &id, 2);
    assert!((x2 - 0.05).abs() < 1e-9, "x {x2}");
    let (_, y1) = position(&state, &id, 1);
    assert!((y1 - y2).abs() < 1e-9, "still horizontal: {y1} vs {y2}");
}

#[test]
fn a_fillet_mints_an_arc_and_shortens_the_two_legs_in_one_undo_step() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    let entities_before = stored(&state, &id).entities.len();

    let out = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "Fillet", "corner": 2, "radius": 0.002 }],
        }),
    );

    // An arc and its two new endpoints come in; the two lines are repointed,
    // not re-created, which is what keeps a constraint on a filleted leg
    // alive (S1 decision 2).
    let added = out["edit"]["added"].as_array().expect("added");
    assert!(
        added.iter().any(|e| e["type"] == "Arc"),
        "an arc: {added:?}"
    );
    let changed = out["edit"]["changed"].as_array().expect("changed");
    assert_eq!(changed.len(), 2, "the two legs: {changed:?}");
    // Two tangent constraints came with it (S1), so the arc stays a fillet.
    assert_eq!(out["edit"]["constraints_added"], 2);
    assert_eq!(out["state"]["constraints"], 2);

    // One batch is ONE undo step, however many entities it touched: a single
    // undo puts the rectangle back, arc, repointed legs, tangents and all.
    ok(&mut state, "undo", json!({}));
    let back = stored(&state, &id);
    assert_eq!(back.entities.len(), entities_before);
    assert!(back.constraints.is_empty(), "{:?}", back.constraints);
}

#[test]
fn an_operation_that_cannot_run_is_refused_by_name_and_changes_nothing() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    let before = stored(&state, &id);

    // Point 1 is a corner of two lines, so a fillet there is legal; point 1 of
    // a radius larger than either leg is not.
    let error = refused(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "Fillet", "corner": 1, "radius": 1.0 }],
        }),
    );
    assert_eq!(error["code"], "SketchOpRefused");
    assert_eq!(error["details"]["reason"]["type"], "FilletDoesNotFit");
    assert_eq!(error["details"]["reason"]["corner"], 1);
    assert_eq!(
        stored(&state, &id).entities.len(),
        before.entities.len(),
        "nothing was committed"
    );
}

#[test]
fn an_op_naming_an_entity_that_is_not_there_is_refused_with_its_id() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);

    let error = refused(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "RemoveConstraint", "index": 3 }],
        }),
    );
    assert_eq!(error["code"], "SketchOpRefused");
    assert_eq!(error["details"]["reason"]["type"], "NoSuchConstraint");
    assert_eq!(error["details"]["reason"]["index"], 3);
}

#[test]
fn an_empty_batch_is_refused_rather_than_committing_an_identical_sketch() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    let error = refused(
        &mut state,
        "sketch_edit",
        json!({ "feature_id": id, "ops": [] }),
    );
    assert_eq!(error["code"], "InvalidSketch");
    assert!(
        error["message"]
            .as_str()
            .expect("a message")
            .contains("/ops"),
        "the pointer names the field: {error}"
    );
}

// ── sketch_edit: the solve verdict ──────────────────────────────────────

#[test]
fn an_edit_that_over_constrains_the_sketch_commits_nothing_by_default() {
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![json!({ "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.02 })],
    );
    let before = stored(&state, &id);

    // A second, different horizontal distance between the same two points.
    let error = refused(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{
                "type": "AddConstraint",
                "constraint": { "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.03 }
            }],
        }),
    );

    assert_eq!(error["code"], "SketchSolveFailed");
    let status = error["details"]["status"].as_str().expect("a status");
    assert!(
        status == "OverConstrained" || status == "SolveFailed",
        "status {status}"
    );
    // The offenders are named, in the index space of the sketch that was
    // solved: both horizontal distances are candidates, and at least one of
    // the two indices has to be there.
    let conflicts = error["details"]["conflicts"]
        .as_array()
        .expect("conflicts")
        .iter()
        .filter_map(Value::as_u64)
        .collect::<Vec<_>>();
    assert!(
        conflicts.iter().any(|i| *i == 0 || *i == 1),
        "conflicts {conflicts:?}"
    );
    let after = stored(&state, &id);
    assert_eq!(
        after.constraints.len(),
        before.constraints.len(),
        "the contradictory constraint was not committed"
    );
}

#[test]
fn on_error_keep_commits_an_edit_whose_solve_failed() {
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![json!({ "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.02 })],
    );

    let out = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{
                "type": "AddConstraint",
                "constraint": { "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.03 }
            }],
            "on_error": "keep",
        }),
    );

    let status = out["solve_status"].as_str().expect("a status");
    assert!(
        status == "OverConstrained" || status == "SolveFailed",
        "status {status}"
    );
    let after = stored(&state, &id);
    assert_eq!(after.constraints.len(), 2, "both are committed");
    // The failed verdict is in the document, so the UI and the next rebuild
    // see the same thing the agent was told.
    assert!(
        !matches!(after.solve_status, waffle_types::SolveStatus::Unsolved),
        "the status was recorded: {:?}",
        after.solve_status
    );
}

#[test]
fn a_redundant_constraint_is_named_without_un_greening_the_sketch() {
    let mut state = EngineState::new();
    let id = sketch(
        &mut state,
        rectangle(),
        vec![json!({ "type": "Horizontal", "entity": 5 })],
    );

    // The same Horizontal twice: satisfiable, so the sketch stays green, but
    // the second row adds no rank.
    let out = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "AddConstraint", "constraint": { "type": "Horizontal", "entity": 5 } }],
        }),
    );

    assert_eq!(out["solve_status"], "UnderConstrained");
    assert_eq!(out["state"]["conflicts"], json!([]));
    assert_eq!(
        out["state"]["redundant"],
        json!([1]),
        "the later duplicate is the dependent one"
    );
    assert_eq!(out["state"]["rank"], 1);
    assert_eq!(out["state"]["rows"], 2);
}

// ── The answer's own shape ──────────────────────────────────────────────

#[test]
fn the_edit_records_the_calling_agent_on_the_feature() {
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "AddEntity", "entity": { "type": "Point", "id": 0, "x": 0.0, "y": 0.03 } }],
        }),
    );

    let provenance = state
        .engine
        .tree
        .provenance_of(state.engine.tree.features[0].id)
        .cloned()
        .expect("a provenance record");
    assert!(
        matches!(
            provenance.origin,
            ProvenanceOrigin::Agent { ref name } if name == AGENT
        ),
        "{provenance:?}"
    );
}

#[test]
fn the_three_sketch_tools_report_the_same_state_object() {
    // One shape for the whole door: whatever a caller learned to read off
    // `sketch_create` it can read off the other two.
    let mut state = EngineState::new();
    let created = ok(
        &mut state,
        "sketch_create",
        json!({ "plane": xy(), "entities": rectangle(), "constraints": [] }),
    );
    let id = created["feature_id"].as_str().expect("an id").to_string();
    let queried = ok(
        &mut state,
        "sketch_solve_state",
        json!({ "feature_id": id }),
    );
    let edited = ok(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": id,
            "ops": [{ "type": "AddConstraint", "constraint": { "type": "Horizontal", "entity": 5 } }],
        }),
    );

    let keys = |v: &Value| {
        let mut k: Vec<String> = v["state"]
            .as_object()
            .expect("a state object")
            .keys()
            .cloned()
            .collect();
        k.sort();
        k
    };
    assert_eq!(keys(&created), keys(&queried));
    assert_eq!(keys(&created), keys(&edited));
    // And the untouched sketch reads the same through create and through the
    // query that follows it.
    assert_eq!(created["state"], queried["state"]);
}

#[test]
fn an_answer_lists_positions_in_ascending_id_order() {
    // A map that reorders between two runs of the same input makes every
    // byte-comparison of an answer worthless, and the goldens in
    // `app/tests/gui/fixtures/` are byte comparisons.
    let mut state = EngineState::new();
    let id = sketch(&mut state, rectangle(), vec![]);
    let out = ok(
        &mut state,
        "sketch_solve_state",
        json!({ "feature_id": id }),
    );
    let ids: Vec<&String> = out["state"]["positions"]
        .as_object()
        .expect("positions")
        .keys()
        .collect();
    assert_eq!(ids, vec!["1", "2", "3", "4"]);
}

#[test]
fn a_sketch_edit_on_a_feature_that_is_not_there_names_the_id() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        "sketch_edit",
        json!({
            "feature_id": "00000000-0000-0000-0000-00000000dead",
            "ops": [{ "type": "RemoveEntity", "ids": [1] }],
        }),
    );
    assert_eq!(error["code"], "FeatureNotFound");
}
