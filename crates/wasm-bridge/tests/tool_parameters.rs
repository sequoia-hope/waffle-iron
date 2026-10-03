//! `parameters_get` and `parameters_set`'s merge mode — P5 of
//! `specs/agent_mechanical_design.md` §6.
//!
//! What these pin is the SHAPE of the two answers and every refusal the
//! merge mode can make. The semantics underneath (the AST rename, the
//! dependency graph, the cycle naming) are pinned in `feature-engine`
//! (`params.rs` unit tests, `tests/parameters.rs`); here the question is
//! whether an agent gets told the truth about them.
//!
//! `MockKernel` renders no bodies, which is irrelevant: a parameter table is
//! arithmetic over the tree, and an extrude's depth is the same number
//! whether or not anything was tessellated from it.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::*;

fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": "parameters-test" });
    execute_tool(state, &mut kernel, name, &args, Some(&context))
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

/// One row of a `parameters_get` answer, by name.
fn row<'a>(answer: &'a Value, name: &str) -> &'a Value {
    answer["parameters"]
        .as_array()
        .expect("the table")
        .iter()
        .find(|p| p["name"] == name)
        .unwrap_or_else(|| panic!("no parameter named '{name}' in {answer:#}"))
}

fn names(answer: &Value) -> Vec<String> {
    answer["parameters"]
        .as_array()
        .expect("the table")
        .iter()
        .map(|p| p["name"].as_str().unwrap_or("").to_string())
        .collect()
}

/// A rectangle sketch plus an extrude whose depth is `depth_expr`, so there
/// is a real feature field reading the table.
fn plate_with_depth_expr(state: &mut EngineState, depth_expr: &str) -> Uuid {
    let corners = [(0.0, 0.0), (0.02, 0.0), (0.02, 0.01), (0.0, 0.01)];
    let mut solved_positions = std::collections::HashMap::new();
    for (i, (x, y)) in corners.iter().enumerate() {
        solved_positions.insert(i as u32 + 1, (*x, *y));
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
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities: corners
                .iter()
                .enumerate()
                .map(|(i, (x, y))| SketchEntity::Point {
                    id: i as u32 + 1,
                    x: *x,
                    y: *y,
                    construction: false,
                })
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
    };
    let added = ok(
        state,
        "feature_add",
        json!({ "operation": serde_json::to_value(&sketch).unwrap() }),
    );
    let sketch_id = added["feature_id"].as_str().expect("a feature id");
    let extrude = ok(
        state,
        "feature_add",
        json!({ "operation": {
            "type": "Extrude",
            "params": {
                "sketch_id": sketch_id,
                "profile_index": 0,
                "profile_entity_ids": [1, 2, 3, 4],
                "depth": 0.004,
                "depth_expr": depth_expr,
                "symmetric": false,
                "cut": false,
            }
        }}),
    );
    Uuid::parse_str(extrude["feature_id"].as_str().expect("a feature id")).expect("a uuid")
}

fn extrude_depth(state: &EngineState, id: Uuid) -> f64 {
    match &state
        .engine
        .tree
        .find_feature(id)
        .expect("the extrude")
        .operation
    {
        Operation::Extrude { params } => params.depth,
        other => panic!("expected an extrude, got {other:?}"),
    }
}

// ── parameters_get ───────────────────────────────────────────────────────

#[test]
fn parameters_get_on_an_empty_document_is_an_empty_table_not_a_refusal() {
    let mut state = EngineState::new();
    let answer = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(answer["parameters"], json!([]));
    assert_eq!(answer["cycles"], json!([]));
}

#[test]
fn parameters_get_carries_the_expression_value_dimension_and_both_directions() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "wall", "expression": "2", "comment": "nozzle × 4" },
            { "name": "bore", "expression": "10" },
            { "name": "od", "expression": "bore + 2 * wall" },
        ]}),
    );
    let extrude = plate_with_depth_expr(&mut state, "wall * 3");
    assert!((extrude_depth(&state, extrude) - 0.006).abs() < 1e-15);

    let answer = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(names(&answer), vec!["wall", "bore", "od"]);

    let od = row(&answer, "od");
    assert_eq!(od["expression"], "bore + 2 * wall");
    assert_eq!(od["value_mm"], json!(14.0));
    // Read off the AST, so it is the DIRECT dependencies, sorted.
    assert_eq!(od["depends_on"], json!(["bore", "wall"]));
    assert_eq!(od["used_by"], json!([]));
    assert_eq!(od["used_by_fields"], json!([]));
    assert!(od.get("error").is_none());

    let wall = row(&answer, "wall");
    assert_eq!(wall["depends_on"], json!([]));
    assert_eq!(wall["used_by"], json!(["od"]));
    assert_eq!(wall["comment"], "nozzle × 4");
    // The FEATURE field that consumes it, named so an agent can go and
    // change the right thing.
    assert_eq!(
        wall["used_by_fields"],
        json!([{
            "feature_id": extrude,
            "feature": "Extrude",
            "field": "depth",
            "expression": "wall * 3",
        }])
    );
    // A bare number commits to no dimension, which is what lets `wall * 3`
    // mean millimetres in a depth (P1).
    assert_eq!(
        wall["dimension"],
        json!({ "length": 0, "angle": 0, "committed": false, "label": "unitless" })
    );
}

#[test]
fn parameters_get_reports_the_dimension_an_expression_committed_itself() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "w", "expression": "2cm" },
            { "name": "turn", "expression": "90deg" },
            { "name": "face", "expression": "2cm * 3cm" },
        ]}),
    );
    let answer = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(row(&answer, "w")["value_mm"], json!(20.0));
    assert_eq!(
        row(&answer, "w")["dimension"],
        json!({ "length": 1, "angle": 0, "committed": true, "label": "length", "kind": "Length" })
    );
    assert_eq!(
        row(&answer, "turn")["dimension"],
        json!({ "length": 0, "angle": 1, "committed": true, "label": "angle", "kind": "Angle" })
    );
    // An area has no `kind`: no field can accept it, and saying otherwise
    // would invite an agent to try.
    let face = &row(&answer, "face")["dimension"];
    assert_eq!(face["length"], json!(2));
    assert_eq!(face["label"], "length^2");
    assert!(face.get("kind").is_none(), "{face:#}");
}

#[test]
fn parameters_get_isolates_a_failing_parameter_instead_of_failing_the_table() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "good", "expression": "10" },
            { "name": "broken", "expression": "nope +" },
            { "name": "unknown", "expression": "absent * 2" },
        ]}),
    );
    let answer = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(row(&answer, "good")["value_mm"], json!(10.0));
    assert!(row(&answer, "good").get("error").is_none());

    // No value is offered for either failure — a stale number presented as
    // the answer is worse than no answer.
    assert_eq!(row(&answer, "broken")["value_mm"], Value::Null);
    assert!(row(&answer, "broken")["error"].is_string());
    // It does not parse, so it has no dependency list at all.
    assert_eq!(row(&answer, "broken")["depends_on"], json!([]));

    // This one parses: the thing it was trying to read is reported even
    // though the read failed, which is usually what needs fixing.
    assert_eq!(row(&answer, "unknown")["value_mm"], Value::Null);
    assert_eq!(row(&answer, "unknown")["depends_on"], json!(["absent"]));
    assert!(row(&answer, "unknown")["error"]
        .as_str()
        .expect("an error")
        .contains("absent"));
}

#[test]
fn parameters_get_names_the_cycle_rather_than_leaving_it_to_be_inferred() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "a", "expression": "b + 1" },
            { "name": "b", "expression": "c + 1" },
            { "name": "c", "expression": "a + 1" },
            { "name": "free", "expression": "5" },
        ]}),
    );
    let answer = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(answer["cycles"], json!([["a", "b", "c", "a"]]));
    assert_eq!(
        row(&answer, "a")["error"],
        "circular reference: a → b → c → a"
    );
    // The rest of the table still answers.
    assert_eq!(row(&answer, "free")["value_mm"], json!(5.0));
    assert!(row(&answer, "free").get("error").is_none());
}

// ── parameters_set: merge ─────────────────────────────────────────────────

#[test]
fn merge_sets_one_parameter_without_resending_the_table() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "a", "expression": "1" },
            { "name": "b", "expression": "2" },
            { "name": "c", "expression": "3" },
        ]}),
    );
    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [{ "name": "b", "expression": "20" }] }),
    );
    // Order preserved: an edit must not reshuffle the panel.
    assert_eq!(names(&answer), vec!["a", "b", "c"]);
    assert_eq!(row(&answer, "b")["value_mm"], json!(20.0));
    assert_eq!(row(&answer, "a")["value_mm"], json!(1.0));
    assert_eq!(row(&answer, "c")["value_mm"], json!(3.0));

    // Without merge, the same one-row call is the WHOLE table.
    let replaced = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "b", "expression": "20" }] }),
    );
    assert_eq!(names(&replaced), vec!["b"]);
}

#[test]
fn merge_adds_a_new_parameter_and_keeps_the_identity_of_the_ones_it_touches() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "a", "expression": "1" }] }),
    );
    let a_id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [
            { "id": a_id, "expression": "11" },
            { "name": "b", "expression": "2" },
        ]}),
    );
    assert_eq!(names(&answer), vec!["a", "b"]);
    assert_eq!(row(&answer, "a")["id"], json!(a_id));
    assert_eq!(row(&answer, "a")["value_mm"], json!(11.0));
    // A row that names only the id and the expression keeps its own name.
    assert_eq!(row(&answer, "a")["expression"], "11");
}

#[test]
fn merge_keeps_a_declared_unit_it_was_not_asked_about() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "turn", "expression": "90", "unit": "Angle", "comment": "half" },
        ]}),
    );
    // Setting the expression says nothing about the unit, so stripping it
    // would silently remove a contract the author wrote.
    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [{ "name": "turn", "expression": "45" }] }),
    );
    assert_eq!(row(&answer, "turn")["unit"], "Angle");
    assert_eq!(row(&answer, "turn")["comment"], "half");
    assert_eq!(row(&answer, "turn")["value_mm"], json!(45.0));

    // An explicit null clears it.
    let cleared = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [{ "name": "turn", "unit": null }] }),
    );
    assert!(row(&cleared, "turn").get("unit").is_none());
}

#[test]
fn merge_deletes_a_parameter_nothing_reads() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "a", "expression": "1" },
            { "name": "spare", "expression": "2" },
        ]}),
    );
    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "delete": ["spare"] }),
    );
    assert_eq!(names(&answer), vec!["a"]);
}

#[test]
fn deleting_a_parameter_something_still_reads_is_refused_naming_the_dependents() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "wall", "expression": "2" },
            { "name": "od", "expression": "wall * 4" },
        ]}),
    );
    plate_with_depth_expr(&mut state, "wall * 3");

    let error = refused(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "delete": ["wall"] }),
    );
    assert_eq!(error["code"], "ParameterInUse");
    assert_eq!(
        error["details"]["blocked"],
        json!([{
            "parameter": "wall",
            "dependents": ["parameter 'od'", "Extrude depth"],
        }])
    );
    // Nothing was written: a refusal that half-applied would be worse than
    // the break it is preventing.
    assert_eq!(
        state
            .engine
            .tree
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["wall", "od"]
    );

    // Freeing the PARAMETER reader in the same call is not enough — the
    // table a delete is checked against is the one the call produces, but
    // the feature field is not in the table, and it still reads `wall`.
    let error = refused(
        &mut state,
        "parameters_set",
        json!({ "merge": true,
                "parameters": [{ "name": "od", "expression": "8" }],
                "delete": ["wall"] }),
    );
    assert_eq!(error["code"], "ParameterInUse");
    assert_eq!(
        error["details"]["blocked"],
        json!([{ "parameter": "wall", "dependents": ["Extrude depth"] }])
    );

    // A parameter NOTHING reads goes.
    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "delete": ["od"] }),
    );
    assert_eq!(names(&answer), vec!["wall"]);
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
}

#[test]
fn deleting_a_parameter_that_is_not_there_is_refused_not_ignored() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "delete": ["ghost"] }),
    );
    assert_eq!(error["code"], "ParameterNotFound");
}

#[test]
fn delete_without_merge_is_refused_because_it_would_mean_two_things() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        "parameters_set",
        json!({ "parameters": [], "delete": ["a"] }),
    );
    assert_eq!(error["code"], "InvalidArguments");
}

// ── parameters_set: rename ────────────────────────────────────────────────

#[test]
fn renaming_through_the_id_rewrites_dependents_and_leaves_a_longer_name_alone() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "w", "expression": "10" },
            { "name": "w2", "expression": "20" },
            { "name": "total", "expression": "w + w2" },
        ]}),
    );
    let w_id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let extrude = plate_with_depth_expr(&mut state, "w * 2 + w2");
    assert!((extrude_depth(&state, extrude) - 0.040).abs() < 1e-15);

    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [{ "id": w_id, "name": "width" }] }),
    );
    assert_eq!(names(&answer), vec!["width", "w2", "total"]);
    // The echo is why `expression` is in the answer: the caller sent no
    // expression at all, and two of these are not what the table held.
    assert_eq!(row(&answer, "width")["expression"], "10");
    assert_eq!(row(&answer, "total")["expression"], "width + w2");
    assert_eq!(row(&answer, "w2")["expression"], "20");

    // The FEATURE field followed too, and `w2` inside it did not.
    let fields = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(
        row(&fields, "width")["used_by_fields"][0]["expression"],
        "width * 2 + w2"
    );
    // A rename changes names, never geometry.
    assert!((extrude_depth(&state, extrude) - 0.040).abs() < 1e-15);
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
}

#[test]
fn renaming_onto_a_taken_name_is_refused_with_nothing_written() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "w", "expression": "10" },
            { "name": "h", "expression": "20" },
            { "name": "total", "expression": "w + h" },
        ]}),
    );
    let w_id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let error = refused(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [{ "id": w_id, "name": "h" }] }),
    );
    assert_eq!(error["code"], "ParameterNameTaken");
    // Had this applied, `total` would read `h + h` — both halves the same
    // parameter, and the author's `w` gone.
    assert_eq!(state.engine.tree.parameters[0].name, "w");
    assert_eq!(state.engine.tree.parameters[2].expression, "w + h");
}

#[test]
fn renaming_to_a_reserved_word_is_refused_by_name() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "w", "expression": "10" }] }),
    );
    let w_id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    for taken in ["mm", "pi", "min", "rad"] {
        let error = refused(
            &mut state,
            "parameters_set",
            json!({ "merge": true, "parameters": [{ "id": w_id, "name": taken }] }),
        );
        assert_eq!(error["code"], "InvalidParameterName", "renaming to {taken}");
    }
    assert_eq!(state.engine.tree.parameters[0].name, "w");
}

#[test]
fn a_full_table_send_that_keeps_an_id_and_changes_the_name_is_a_rename_too() {
    // The panel sends the whole table. Before P5 that lost every dependent
    // of a renamed variable, silently: the id says it is the same parameter.
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "w", "expression": "10" },
            { "name": "total", "expression": "w * 2" },
        ]}),
    );
    let w_id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let total_id = first["parameters"][1]["id"]
        .as_str()
        .expect("an id")
        .to_string();

    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "id": w_id, "name": "width", "expression": "10" },
            // The caller re-sent the OLD expression, as the panel does.
            { "id": total_id, "name": "total", "expression": "w * 2" },
        ]}),
    );
    assert_eq!(row(&answer, "total")["expression"], "width * 2");
    assert_eq!(row(&answer, "total")["value_mm"], json!(20.0));
    assert!(row(&answer, "total").get("error").is_none());
}

#[test]
fn undo_of_a_rename_restores_the_dependent_field_too() {
    let mut state = EngineState::new();
    let first = ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "w", "expression": "10" }] }),
    );
    let w_id = first["parameters"][0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let extrude = plate_with_depth_expr(&mut state, "w * 2");

    ok(
        &mut state,
        "parameters_set",
        json!({ "merge": true, "parameters": [{ "id": w_id, "name": "width" }] }),
    );
    ok(&mut state, "undo", json!({}));
    assert_eq!(state.engine.tree.parameters[0].name, "w");
    match &state
        .engine
        .tree
        .find_feature(extrude)
        .expect("the extrude")
        .operation
    {
        Operation::Extrude { params } => {
            assert_eq!(params.depth_expr.as_deref(), Some("w * 2"))
        }
        other => panic!("expected an extrude, got {other:?}"),
    }
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    assert!((extrude_depth(&state, extrude) - 0.020).abs() < 1e-15);
}
