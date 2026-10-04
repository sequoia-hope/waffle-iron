//! `parameters_get` / `parameters_set`'s `scope`, and the instance overrides
//! `instance_edit` writes — P2 of `specs/agent_mechanical_design.md` §6.
//!
//! What these pin is the SHAPE of the three scopes and every refusal they can
//! make. The scoping and pinning semantics underneath are pinned in
//! `feature-engine` (`tests/param_scopes.rs`); here the question is whether an
//! agent is told the truth about them — which table it is reading, which rows
//! it inherits, which of those are shadowed, and what an override did.
//!
//! `MockKernel` throughout: a parameter table is arithmetic over the tree, and
//! an extrude's depth is the same number whether or not anything was
//! tessellated from it. `tests/param_scopes.rs` is where the SOLID is
//! measured.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::*;

fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": "param-scopes-test" });
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
/// is a real feature field reading whichever table resolves the name.
fn plate_with_depth_expr(state: &mut EngineState, depth_expr: &str) -> Uuid {
    let corners = [(0.0, 0.0), (0.02, 0.0), (0.02, 0.01), (0.0, 0.01)];
    let mut solved_positions = std::collections::HashMap::new();
    for (i, (x, y)) in corners.iter().enumerate() {
        solved_positions.insert(i as u32 + 1, (*x, *y));
    }
    let sketch = Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane_face: None,
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

// ── scope: document ─────────────────────────────────────────────────────────

#[test]
fn a_document_parameter_drives_a_tab_field_and_reads_back_evaluated() {
    let mut state = EngineState::new();
    let answer = ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "12" }] }),
    );
    assert_eq!(answer["scope"], "document");
    assert_eq!(row(&answer, "stock")["value_mm"], json!(12.0));

    let extrude = plate_with_depth_expr(&mut state, "stock * 2");
    assert!((extrude_depth(&state, extrude) - 0.024).abs() < 1e-15);

    let got = ok(&mut state, "parameters_get", json!({ "scope": "document" }));
    assert_eq!(got["scope"], "document");
    assert_eq!(names(&got), vec!["stock"]);
    assert_eq!(row(&got, "stock")["value_mm"], json!(12.0));
    assert_eq!(row(&got, "stock")["scope"], "document");
}

#[test]
fn a_document_edit_rebuilds_the_tab_field_that_reads_it() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "12" }] }),
    );
    let extrude = plate_with_depth_expr(&mut state, "stock");
    assert!((extrude_depth(&state, extrude) - 0.012).abs() < 1e-15);
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "merge": true,
                "parameters": [{ "name": "stock", "expression": "25" }] }),
    );
    assert!((extrude_depth(&state, extrude) - 0.025).abs() < 1e-15);
}

#[test]
fn the_document_table_is_stored_on_the_document_not_the_tab() {
    // Which is the whole point: it must survive a tab switch and be the same
    // table on a tab that has no table of its own.
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "12" }] }),
    );
    let asm = ok(&mut state, "tab_add", json!({ "kind": "Assembly" }));
    let asm = asm["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    let got = ok(&mut state, "parameters_get", json!({ "scope": "document" }));
    assert_eq!(names(&got), vec!["stock"]);
    assert_eq!(row(&got, "stock")["value_mm"], json!(12.0));
    // And an Assembly tab has no table of its own, which the tab scope says
    // by answering an empty one rather than refusing.
    let tabbed = ok(&mut state, "parameters_get", json!({ "scope": "tab" }));
    assert_eq!(
        tabbed["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["scope"] == "tab")
            .count(),
        0
    );
}

#[test]
fn a_document_rename_is_refused_whole_and_says_what_to_do_instead() {
    // The rewrite reaches every TAB, and this call holds only the open one.
    // Half a rename is worse than none — the same rule `ParameterNameTaken`
    // enforces for a tab rename.
    let mut state = EngineState::new();
    let set = ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "12" }] }),
    );
    let id = row(&set, "stock")["id"].as_str().unwrap().to_string();
    let err = refused(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "merge": true,
                "parameters": [{ "id": id, "name": "plate_t", "expression": "12" }] }),
    );
    assert_eq!(err["code"], "ParameterRenameNotSupported");
    let message = err["message"].as_str().unwrap();
    assert!(message.contains("Nothing was changed"), "{message}");
    assert!(message.contains("read by every tab"), "{message}");
    // Still there, under its old name.
    let got = ok(&mut state, "parameters_get", json!({ "scope": "document" }));
    assert_eq!(names(&got), vec!["stock"]);
}

#[test]
fn a_document_delete_is_refused_by_a_reader_in_another_tab() {
    // The reason the in-use check walks every part tab: checking only the
    // OPEN tab would let the delete through and break a Part nobody is
    // looking at.
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "12" }] }),
    );
    // Tab 1 reads it.
    plate_with_depth_expr(&mut state, "stock");
    // Switch to a second Part tab, which does not.
    let second = ok(&mut state, "tab_add", json!({ "kind": "Part" }));
    let second = second["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": second }));

    let err = refused(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "merge": true, "delete": ["stock"] }),
    );
    assert_eq!(err["code"], "ParameterInUse");
    let message = err["message"].as_str().unwrap();
    assert!(message.contains("Nothing was changed"), "{message}");
    // Named with the TAB it is in, because "Extrude1 depth" alone does not
    // say which tab to go and fix.
    assert!(message.contains("Part 1 › "), "{message}");
    assert!(message.contains("depth"), "{message}");
}

#[test]
fn an_unknown_scope_is_refused_by_name_on_both_tools() {
    let mut state = EngineState::new();
    for tool_name in ["parameters_get", "parameters_set"] {
        let err = refused(
            &mut state,
            tool_name,
            json!({ "scope": "global", "parameters": [] }),
        );
        assert_eq!(err["code"], "InvalidArguments", "{tool_name}");
        let message = err["message"].as_str().unwrap();
        assert!(
            message.contains("unknown scope `global`"),
            "{tool_name}: {message}"
        );
        assert!(message.contains("instance"), "{tool_name}: {message}");
    }
}

// ── scope: tab, with the document rows it inherits ──────────────────────────

#[test]
fn the_tab_scope_lists_what_it_inherits_and_marks_what_it_shadows() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "parameters": [
            { "name": "stock", "expression": "12" },
            { "name": "wall", "expression": "3" },
        ]}),
    );
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "wall", "expression": "1.5" }] }),
    );
    let extrude = plate_with_depth_expr(&mut state, "wall");
    assert!(
        (extrude_depth(&state, extrude) - 0.0015).abs() < 1e-15,
        "local first, then document"
    );

    let got = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(got["scope"], "tab");
    // The tab's own row, then everything it inherits — one answer to "what
    // can an expression here read".
    assert_eq!(names(&got), vec!["wall", "stock", "wall"]);
    let rows = got["parameters"].as_array().unwrap();
    assert_eq!(rows[0]["scope"], "tab");
    assert_eq!(rows[0]["value_mm"], json!(1.5));
    assert_eq!(rows[1]["scope"], "document");
    assert_eq!(rows[1].get("shadowed"), None, "nothing shadows `stock`");
    assert_eq!(rows[2]["scope"], "document");
    assert_eq!(
        rows[2]["shadowed"],
        json!(true),
        "the document `wall` is hidden by the tab's"
    );
    assert_eq!(
        rows[2]["value_mm"],
        json!(3.0),
        "a shadowed row still reports its own value — it is listed to be \
         recognised, not to be used"
    );
    // The field use is on the TAB's row, which is the one driving it.
    assert_eq!(rows[0]["used_by_fields"][0]["field"], "depth");
}

#[test]
fn a_document_parameter_does_not_claim_a_tab_parameter_as_a_dependent() {
    // `used_by` is a relation WITHIN a table. A document `w` and a tab `w`
    // are two parameters, and the tab one SHADOWS the document one — calling
    // it a dependent would be exactly backwards.
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "parameters": [
            { "name": "w", "expression": "40" },
            { "name": "двойной", "expression": "w * 2" },
        ]}),
    );
    let got = ok(&mut state, "parameters_get", json!({ "scope": "document" }));
    assert_eq!(row(&got, "w")["used_by"], json!(["двойной"]));
    // And the document scope says plainly that it is not reporting field
    // readers, rather than answering an empty list that reads as "none".
    assert!(got["used_by_fields_scope"]
        .as_str()
        .unwrap()
        .contains("span every tab"));
}
