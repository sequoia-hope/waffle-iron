//! Per-instance parameter overrides through the tools — P2 of
//! `specs/agent_mechanical_design.md` §6.
//!
//! `tool_param_scopes.rs` covers the document scope; this covers the third
//! one. The question here is the same: whether an agent is told the truth
//! about what an override did, and whether two instances of one Part tab stop
//! sharing an engine the moment their overrides differ.
//!
//! `MockKernel`: the depth an instance built with is arithmetic over its own
//! tree. `crates/feature-engine/tests/param_scopes.rs` is where the SOLID is
//! tessellated and measured.

use feature_engine::types::*;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;
use wasm_bridge::*;

fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": "param-instance-test" });
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

/// A rectangle sketch plus an extrude whose depth is `depth_expr`.
fn plate_with_depth_expr(state: &mut EngineState, depth_expr: &str) {
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
    ok(
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
}

/// A document with a parameterised plate in `Part 1` and an empty, ACTIVE
/// Assembly tab. Returns the part tab's id.
fn parameterised_part_and_assembly(state: &mut EngineState) -> String {
    let part = state.session.tabs()[0].id.clone();
    ok(
        state,
        "parameters_set",
        json!({ "parameters": [
            { "name": "height", "expression": "10" },
            { "name": "wall", "expression": "height / 4" },
        ]}),
    );
    plate_with_depth_expr(state, "wall");
    let asm = ok(state, "tab_add", json!({ "kind": "Assembly" }));
    let asm = asm["tab_id"].as_str().unwrap().to_string();
    ok(state, "tab_switch", json!({ "tab_id": asm }));
    part
}

/// The extrude depth of the part engine one instance was built with.
fn instance_depth(state: &EngineState, instance: Uuid) -> f64 {
    let engine = state
        .assembly
        .as_ref()
        .expect("an open assembly")
        .engine_for_instance(instance)
        .unwrap_or_else(|| panic!("instance {instance} has no built part"));
    match &engine
        .tree
        .features
        .iter()
        .find(|f| matches!(f.operation, Operation::Extrude { .. }))
        .expect("the extrude")
        .operation
    {
        Operation::Extrude { params } => params.depth,
        other => panic!("expected an extrude, got {other:?}"),
    }
}

fn add_instance(state: &mut EngineState, part: &str) -> Uuid {
    let added = ok(state, "instance_add", json!({ "tab_id": part }));
    Uuid::parse_str(added["instance_id"].as_str().unwrap()).unwrap()
}

#[test]
fn two_instances_of_one_part_build_different_depths_and_do_not_share_an_engine() {
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let a = add_instance(&mut state, &part);
    let b = add_instance(&mut state, &part);

    // Both are the part's own build until one is overridden, and they share
    // ONE engine because they are the same build — which is the behaviour
    // the override must break, not the behaviour it replaces.
    assert!((instance_depth(&state, a) - 0.0025).abs() < 1e-15);
    assert!((instance_depth(&state, b) - 0.0025).abs() < 1e-15);
    assert_eq!(
        state.assembly.as_ref().unwrap().parts.len(),
        1,
        "two instances of one build are one engine"
    );

    // Override `height` on B only. `wall = height / 4` must follow.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "height": 40.0 } }),
    );
    assert!(
        (instance_depth(&state, a) - 0.0025).abs() < 1e-15,
        "A is untouched"
    );
    assert!(
        (instance_depth(&state, b) - 0.010).abs() < 1e-15,
        "B's dependent parameter followed the override"
    );
    assert_eq!(
        state.assembly.as_ref().unwrap().parts.len(),
        2,
        "two builds, two engines — the cache must not conflate them"
    );
}

#[test]
fn parameters_get_on_an_instance_names_the_overridden_rows() {
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let b = add_instance(&mut state, &part);
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "height": 40.0 } }),
    );
    let got = ok(
        &mut state,
        "parameters_get",
        json!({ "scope": "instance", "instance_id": b.to_string() }),
    );
    assert_eq!(got["scope"], "instance");
    assert_eq!(got["instance_id"], b.to_string());
    assert_eq!(row(&got, "height")["scope"], "instance");
    assert_eq!(row(&got, "height")["override"], json!(40.0));
    assert_eq!(row(&got, "height")["value_mm"], json!(40.0));
    // The row that is NOT overridden says so, and carries the value the
    // override drove it to.
    assert_eq!(row(&got, "wall")["scope"], "tab");
    assert_eq!(row(&got, "wall").get("override"), None);
    assert_eq!(row(&got, "wall")["value_mm"], json!(10.0));
    assert_eq!(got["overrides_matching_no_parameter"], json!([]));
}

#[test]
fn an_override_naming_no_parameter_is_reported_by_the_instance_scope() {
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let b = add_instance(&mut state, &part);
    // Accepted by the tool — the refusal belongs to the rebuild, which names
    // the part and the name — and then findable without reading the error
    // list.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "heigth": 40.0 } }),
    );
    let got = ok(
        &mut state,
        "parameters_get",
        json!({ "scope": "instance", "instance_id": b.to_string() }),
    );
    assert_eq!(got["overrides_matching_no_parameter"], json!(["heigth"]));
    let errors = state.assembly.as_ref().unwrap().errors.join(" | ");
    assert!(
        errors.contains("override of 'heigth'"),
        "the rebuild must say so too: {errors}"
    );
}

#[test]
fn instance_overrides_merge_clear_and_replace_as_documented() {
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let b = add_instance(&mut state, &part).to_string();

    fn overrides_of(state: &EngineState, b: &str) -> Value {
        let tree = state.assembly.as_ref().unwrap().tree.clone();
        let inst = tree
            .instances
            .iter()
            .find(|i| i.id.to_string() == b)
            .expect("the instance");
        serde_json::to_value(&inst.parameter_overrides).unwrap()
    }

    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b,
                "parameter_overrides": { "height": 40.0, "wall": 2.0 } }),
    );
    assert_eq!(
        overrides_of(&state, &b),
        json!({ "height": 40.0, "wall": 2.0 })
    );

    // Omitting the key KEEPS the map (the `unit`/`comment` rule).
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "name": "Renamed" }),
    );
    assert_eq!(
        overrides_of(&state, &b),
        json!({ "height": 40.0, "wall": 2.0 })
    );

    // merge edits one name and leaves the rest.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "merge": true,
                "parameter_overrides": { "height": 60.0 } }),
    );
    assert_eq!(
        overrides_of(&state, &b),
        json!({ "height": 60.0, "wall": 2.0 })
    );

    // merge + null removes ONE.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "merge": true,
                "parameter_overrides": { "wall": null } }),
    );
    assert_eq!(overrides_of(&state, &b), json!({ "height": 60.0 }));

    // Without merge the map REPLACES.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "parameter_overrides": { "wall": 1.0 } }),
    );
    assert_eq!(overrides_of(&state, &b), json!({ "wall": 1.0 }));

    // `null` clears every override, and the instance is the part's build
    // again — which the file records by carrying no key at all.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "parameter_overrides": null }),
    );
    assert_eq!(overrides_of(&state, &b), Value::Null);

    // An empty map is the same as none, so `{}` is not a second cache entry.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "parameter_overrides": {} }),
    );
    assert_eq!(overrides_of(&state, &b), Value::Null);
}

#[test]
fn a_non_numeric_override_is_refused_by_name() {
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let b = add_instance(&mut state, &part).to_string();
    for bad in [json!("40mm"), json!({}), json!([40.0])] {
        let err = refused(
            &mut state,
            "instance_edit",
            json!({ "instance_id": b,
                    "parameter_overrides": { "height": bad } }),
        );
        assert_eq!(err["code"], "InvalidArguments");
        assert!(err["message"]
            .as_str()
            .unwrap()
            .contains("must be a finite number"));
    }
    let err = refused(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b, "parameter_overrides": "height=40" }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    assert!(err["message"]
        .as_str()
        .unwrap()
        .contains("must be an object of {name: number}"));
}

#[test]
fn parameters_set_on_an_instance_writes_the_same_overrides() {
    // The scope routes to `instance_edit` rather than writing an instance a
    // second way, so one set of preconditions governs both spellings.
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let b = add_instance(&mut state, &part);
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "instance", "instance_id": b.to_string(),
                "overrides": { "height": 40.0 } }),
    );
    assert!((instance_depth(&state, b) - 0.010).abs() < 1e-15);

    let err = refused(
        &mut state,
        "parameters_set",
        json!({ "scope": "instance", "instance_id": b.to_string() }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    assert!(err["message"]
        .as_str()
        .unwrap()
        .contains("`overrides` is required"));

    let err = refused(
        &mut state,
        "parameters_set",
        json!({ "scope": "instance", "overrides": { "height": 40.0 } }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    assert!(err["message"]
        .as_str()
        .unwrap()
        .contains("instance_id is required"));
}

#[test]
fn the_instance_scope_refuses_without_an_open_assembly() {
    let mut state = EngineState::new();
    let err = refused(
        &mut state,
        "parameters_get",
        json!({ "scope": "instance", "instance_id": Uuid::new_v4().to_string() }),
    );
    assert_eq!(err["code"], "InvalidArguments");
    assert!(err["message"]
        .as_str()
        .unwrap()
        .contains("needs an open Assembly tab"));
}

#[test]
fn an_instance_sees_the_document_table_too() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "8" }] }),
    );
    let part = state.session.tabs()[0].id.clone();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "t", "expression": "stock" }] }),
    );
    plate_with_depth_expr(&mut state, "t");
    let asm = ok(&mut state, "tab_add", json!({ "kind": "Assembly" }));
    let asm = asm["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    let b = add_instance(&mut state, &part);
    assert!(
        (instance_depth(&state, b) - 0.008).abs() < 1e-15,
        "the instance resolved `stock` through the document table"
    );

    // And the override wins over the document value it would inherit.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "t": 3.0 } }),
    );
    assert!((instance_depth(&state, b) - 0.003).abs() < 1e-15);

    let got = ok(
        &mut state,
        "parameters_get",
        json!({ "scope": "instance", "instance_id": b.to_string() }),
    );
    // Both the part's row and the document row it inherits are listed.
    assert_eq!(names(&got), vec!["t", "stock"]);
    assert_eq!(row(&got, "stock")["scope"], "document");
    assert_eq!(row(&got, "t")["scope"], "instance");
}

#[test]
fn a_document_edit_rebuilds_the_parts_of_an_open_assembly() {
    // §6's oracle: "a document parameter edited in the Assembly tab rebuilds
    // both Parts that read it". The assembly tab holds no tree of its own, so
    // nothing but the explicit re-evaluation reaches the part engines.
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "8" }] }),
    );
    let first = state.session.tabs()[0].id.clone();
    plate_with_depth_expr(&mut state, "stock");
    let second = ok(&mut state, "tab_add", json!({ "kind": "Part" }));
    let second = second["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": second }));
    plate_with_depth_expr(&mut state, "stock * 2");

    let asm = ok(&mut state, "tab_add", json!({ "kind": "Assembly" }));
    let asm = asm["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    let a = add_instance(&mut state, &first);
    let b = add_instance(&mut state, &second);
    assert!((instance_depth(&state, a) - 0.008).abs() < 1e-15);
    assert!((instance_depth(&state, b) - 0.016).abs() < 1e-15);

    // Edit the document table from the ASSEMBLY tab.
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "merge": true,
                "parameters": [{ "name": "stock", "expression": "20" }] }),
    );
    assert!(
        (instance_depth(&state, a) - 0.020).abs() < 1e-15,
        "the first part followed"
    );
    assert!(
        (instance_depth(&state, b) - 0.040).abs() < 1e-15,
        "and so did the second"
    );
}
