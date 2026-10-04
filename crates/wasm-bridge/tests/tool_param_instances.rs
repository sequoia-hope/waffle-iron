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

#[test]
fn a_parked_part_engine_is_rejected_when_the_document_table_moved_under_it() {
    // The cache-key half nothing else reaches: a PARKED engine (the pool a
    // tab switch leaves behind) whose part tree is byte-identical to the one
    // the next pass wants, built against a document table that has since
    // changed.
    //
    // It has to be the tab that is NOT open while the edit happens. The open
    // tab's tree is rebuilt by `set_document_parameters`, so its stored tree
    // moves and the reuse hit's tree comparison rejects the parked engine on
    // its own. The other tab's stored tree does not move — its `depth` still
    // holds the value the OLD table drove — so the tree comparison PASSES and
    // only `params::table_signature` stands between the instance and a solid
    // built from a document variable that no longer has that value.
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "8" }] }),
    );
    let open = state.session.tabs()[0].id.clone();
    plate_with_depth_expr(&mut state, "stock");
    let parked = ok(&mut state, "tab_add", json!({ "kind": "Part" }));
    let parked = parked["tab_id"].as_str().unwrap().to_string();
    ok(
        &mut state,
        "tab_switch",
        json!({ "tab_id": parked.clone() }),
    );
    plate_with_depth_expr(&mut state, "stock * 2");

    let asm = ok(&mut state, "tab_add", json!({ "kind": "Assembly" }));
    let asm = asm["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": asm.clone() }));
    let a = add_instance(&mut state, &open);
    let b = add_instance(&mut state, &parked);
    assert!((instance_depth(&state, a) - 0.008).abs() < 1e-15);
    assert!((instance_depth(&state, b) - 0.016).abs() < 1e-15);

    // Leave the assembly: both part engines are parked, each an exact build
    // of its tree against `stock = 8`.
    ok(&mut state, "tab_switch", json!({ "tab_id": open.clone() }));
    assert_eq!(
        state.part_cache.len(),
        2,
        "both part engines parked for reuse"
    );

    // Move the document table while the assembly is closed and `parked` is
    // not the open tab, so nothing rebuilds its tree.
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "merge": true,
                "parameters": [{ "name": "stock", "expression": "20" }] }),
    );

    // Back to the assembly: both instances must be the new table's solids.
    ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    assert!(
        (instance_depth(&state, a) - 0.020).abs() < 1e-15,
        "the open tab's part followed the document edit"
    );
    assert!(
        (instance_depth(&state, b) - 0.040).abs() < 1e-15,
        "the PARKED part was rebuilt rather than reused against the old table"
    );
}

#[test]
fn an_overridden_instance_does_not_cost_its_siblings_their_parked_engines() {
    // The park half of keying on the BUILD: editing one instance's overrides
    // retires that build's engine and leaves every sibling's parked. Keyed on
    // the part instead, one override edit declared every build of the part
    // stale and the siblings paid a full rebuild.
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let a = add_instance(&mut state, &part);
    let b = add_instance(&mut state, &part);
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "height": 40.0 } }),
    );
    assert_eq!(state.assembly.as_ref().unwrap().parts.len(), 2);

    // Edit B's override again. A's build is untouched, so its engine must
    // still be the one the pass took rather than a fresh build.
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "height": 60.0 } }),
    );
    assert!((instance_depth(&state, a) - 0.0025).abs() < 1e-15, "A held");
    assert!((instance_depth(&state, b) - 0.015).abs() < 1e-15, "B moved");

    // The depths above are NOT the park claim: a rebuilt sibling renders the
    // same solid as a reused one, so geometry cannot tell reuse from rebuild.
    // The cache is what can. `park_unused_part_engines` saw A's build live and
    // B's 40 mm build retired, so exactly the retired build is parked — and A's
    // engine, which the pass took, is not evicted along with it. Keyed on the
    // PART rather than the BUILD this is empty, because B's new build makes
    // every build of that part look stale.
    let parked: Vec<_> = state
        .part_cache
        .iter()
        .map(|(build, _)| build.overrides.clone())
        .collect();
    assert_eq!(
        parked.len(),
        1,
        "exactly the retired build is parked, not every build of the part: {parked:?}"
    );
    assert_eq!(
        parked[0]
            .as_ref()
            .and_then(|o| o.get("height"))
            .copied()
            .unwrap(),
        40.0,
        "and the parked one is B's PREVIOUS override, not A's plain build"
    );

    // And an instance with NO overrides never becomes a second cache entry:
    // a third plain instance joins A's build.
    let c = add_instance(&mut state, &part);
    assert!((instance_depth(&state, c) - 0.0025).abs() < 1e-15);
    assert_eq!(
        state.assembly.as_ref().unwrap().parts.len(),
        2,
        "three instances, two distinct builds"
    );
}
#[test]
fn a_document_edit_leaves_an_unopened_tab_s_stored_value_stale() {
    // MEASURED, not a claim: a document-table edit rebuilds the OPEN tab and
    // re-evaluates an open Assembly or Drawing, and nothing else. Every other
    // Part tab keeps the plain value the OLD table drove until it is next
    // switched to, where `switch_tab`'s rebuild re-applies the expression.
    //
    // The geometry a v12 reader builds is therefore always right — a load
    // rebuilds the active tab and a switch rebuilds the rest. What is NOT
    // right in that window is the file: `docs/FILE_FORMAT.md` §13.3 leans on
    // "a correctly written file has `value == eval(expr)`" to argue that an
    // `*_expr` sidecar needs no floor bump, and a save taken here breaks that
    // for the tabs nobody opened. A v11 reader of such a file, which drops the
    // document table, then builds the PRE-edit size rather than the size the
    // document was saved with. See the P2/P3 implementation notes' open items.
    let mut state = EngineState::new();
    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document",
                "parameters": [{ "name": "stock", "expression": "8" }] }),
    );
    let open = state.session.tabs()[0].id.clone();
    plate_with_depth_expr(&mut state, "stock");
    let other = ok(&mut state, "tab_add", json!({ "kind": "Part" }));
    let other = other["tab_id"].as_str().unwrap().to_string();
    ok(&mut state, "tab_switch", json!({ "tab_id": other.clone() }));
    plate_with_depth_expr(&mut state, "stock");
    ok(&mut state, "tab_switch", json!({ "tab_id": open.clone() }));

    ok(
        &mut state,
        "parameters_set",
        json!({ "scope": "document", "merge": true,
                "parameters": [{ "name": "stock", "expression": "20" }] }),
    );

    // The open tab followed.
    assert!(
        (stored_depth(&state, &open) - 0.020).abs() < 1e-15,
        "the open tab rebuilt"
    );
    // The other one did not — this is the gap, pinned so it cannot change
    // silently in either direction.
    assert!(
        (stored_depth(&state, &other) - 0.008).abs() < 1e-15,
        "an unopened tab keeps the old table's value until it is switched to"
    );

    // And switching to it is what repairs it.
    ok(&mut state, "tab_switch", json!({ "tab_id": other.clone() }));
    assert!(
        (stored_depth(&state, &other) - 0.020).abs() < 1e-15,
        "the switch re-applied the expression against the new table"
    );
}

/// The extrude depth stored on a tab's tree (the ACTIVE tab's live tree, or
/// the session's copy for any other).
fn stored_depth(state: &EngineState, tab_id: &str) -> f64 {
    let tree = if state.session.active_tab_id() == tab_id {
        state.engine.tree.clone()
    } else {
        state
            .session
            .tab(tab_id)
            .and_then(|t| t.features())
            .expect("a part tab")
            .clone()
    };
    tree.features
        .iter()
        .find_map(|f| match &f.operation {
            Operation::Extrude { params } => Some(params.depth),
            _ => None,
        })
        .expect("the extrude")
}

#[test]
fn renaming_a_parameter_an_instance_overrides_does_not_move_the_override_key() {
    // P5's rename rewrites every expression that READS the parameter, on the
    // tree it holds. An instance's override key is not an expression and is
    // not on that tree — it is on the AssemblyTree in the session — so it
    // keeps the old name.
    //
    // The outcome is LOUD, which is why it is pinned rather than fixed here:
    // the override now names a parameter the part does not declare, which is
    // already a reported error and already listed under
    // `overrides_matching_no_parameter`. What it is NOT is silent: the
    // instance does not keep building 40 mm under a new name, and it does not
    // quietly adopt the renamed row.
    let mut state = EngineState::new();
    let part = parameterised_part_and_assembly(&mut state);
    let b = add_instance(&mut state, &part);
    ok(
        &mut state,
        "instance_edit",
        json!({ "instance_id": b.to_string(),
                "parameter_overrides": { "height": 40.0 } }),
    );
    assert!((instance_depth(&state, b) - 0.010).abs() < 1e-15);

    // Rename `height` to `tall` in the part tab: same id, new name.
    ok(&mut state, "tab_switch", json!({ "tab_id": part.clone() }));
    let table = ok(&mut state, "parameters_get", json!({}));
    let height_id = row(&table, "height")["id"].as_str().unwrap().to_string();
    let wall_id = row(&table, "wall")["id"].as_str().unwrap().to_string();
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [
            { "id": height_id, "name": "tall", "expression": "10" },
            { "id": wall_id, "name": "wall", "expression": "height / 4" },
        ]}),
    );
    // The dependent's expression followed the rename.
    let table = ok(&mut state, "parameters_get", json!({}));
    assert_eq!(row(&table, "wall")["expression"], "tall / 4");

    // Back in the assembly, the override addresses nothing and says so.
    let asm = state
        .session
        .tabs()
        .iter()
        .find(|t| t.kind == "Assembly")
        .expect("the assembly tab")
        .id
        .clone();
    ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    let got = ok(
        &mut state,
        "parameters_get",
        json!({ "scope": "instance", "instance_id": b.to_string() }),
    );
    assert_eq!(
        got["overrides_matching_no_parameter"],
        json!(["height"]),
        "the override key did not follow the rename, and is named for it"
    );
    assert_eq!(
        row(&got, "tall").get("override"),
        None,
        "and it did not silently re-attach to the renamed row"
    );
    assert!(
        (instance_depth(&state, b) - 0.0025).abs() < 1e-15,
        "the instance fell back to the part's own build"
    );
}
