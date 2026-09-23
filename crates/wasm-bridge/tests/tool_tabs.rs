//! The tab tools in the engine (`crates/wasm-bridge/src/tools/tabs.rs`):
//! what the page's JS answered until 2026-09-23, now answered by the session.
//! The page and the native host overlay their storage fields on the engine's
//! answer; this pins the engine's share — the tab list, the active tab, the
//! refusals — and that every tab tool carries the model update a host mirrors.

use serde_json::{json, Value};
use waffle_types::kernel::MockKernel;
use wasm_bridge::messages::{EngineToUi, UiToEngine};
use wasm_bridge::tools::{mutates, MIGRATED, TAB_TOOLS};
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

fn tab_ids(answer: &Value) -> Vec<String> {
    answer["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn tab_add_appends_names_and_activates_by_default() {
    let mut state = EngineState::new();
    let first = state.session.tabs()[0].id.clone();

    let a = ok(&mut state, "tab_add", json!({}));
    let a_id = a["tab_id"].as_str().unwrap().to_string();
    assert_eq!(tab_ids(&a), vec![first.clone(), a_id.clone()]);
    assert_eq!(a["tabs"][1]["name"], "Part 2");
    assert_eq!(a["tabs"][1]["kind"], "Part");
    assert_eq!(a["active_tab"], a_id, "activated by default");
    assert!(
        a["document_id"].is_string(),
        "the engine's share of document_info"
    );
    assert!(a["sources"].is_array());
    assert!(
        a.get("storage_provider").is_none(),
        "storage is the host's to overlay"
    );

    let b = ok(
        &mut state,
        "tab_add",
        json!({ "kind": "Assembly", "name": "Asm", "activate": false }),
    );
    assert_eq!(b["tabs"][2]["name"], "Asm");
    assert_eq!(b["tabs"][2]["kind"], "Assembly");
    assert_eq!(
        b["active_tab"], a_id,
        "activate: false leaves the active tab"
    );

    let err = refused(&mut state, "tab_add", json!({ "kind": "Drawing" }));
    assert_eq!(err["code"], "TabKindNotSupported");
    assert_eq!(err["details"]["kind"], "Drawing");
    assert_eq!(state.session.tabs().len(), 3, "nothing was added");
}

#[test]
fn tab_switch_opens_an_assembly_and_refuses_an_unknown_tab() {
    let mut state = EngineState::new();
    let first = state.session.tabs()[0].id.clone();
    let asm = ok(
        &mut state,
        "tab_add",
        json!({ "kind": "Assembly", "activate": false }),
    )["tab_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(state.assembly.is_none());

    let switched = ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    assert_eq!(switched["active_tab"], asm);
    assert!(
        state.assembly.is_some(),
        "an Assembly tab is evaluated on switch"
    );

    // Already active: a no-op that still answers.
    let again = ok(&mut state, "tab_switch", json!({ "tab_id": asm }));
    assert_eq!(again["active_tab"], asm);

    let back = ok(&mut state, "tab_switch", json!({ "tab_id": first }));
    assert_eq!(back["active_tab"], first);
    assert!(
        state.assembly.is_none(),
        "the assembly view is left with the tab"
    );

    let err = refused(&mut state, "tab_switch", json!({ "tab_id": "nope" }));
    assert_eq!(err["code"], "TabNotFound");
    assert_eq!(err["details"]["tab_id"], "nope");
    assert_eq!(err["message"], "The document has no tab with id nope.");
}

#[test]
fn tab_move_clamps_and_tab_rename_names() {
    let mut state = EngineState::new();
    let first = state.session.tabs()[0].id.clone();
    let b = ok(&mut state, "tab_add", json!({ "activate": false }))["tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    let moved = ok(&mut state, "tab_move", json!({ "tab_id": b, "index": 0 }));
    assert_eq!(tab_ids(&moved), vec![b.clone(), first.clone()]);
    let moved = ok(&mut state, "tab_move", json!({ "tab_id": b, "index": 99 }));
    assert_eq!(
        tab_ids(&moved),
        vec![first.clone(), b.clone()],
        "past the end moves last"
    );

    let renamed = ok(
        &mut state,
        "tab_rename",
        json!({ "tab_id": b, "name": "Base" }),
    );
    assert_eq!(renamed["tabs"][1]["name"], "Base");
    assert_eq!(state.session.tabs()[1].name, "Base");

    let err = refused(
        &mut state,
        "tab_move",
        json!({ "tab_id": "nope", "index": 0 }),
    );
    assert_eq!(err["code"], "TabNotFound");
    let err = refused(
        &mut state,
        "tab_rename",
        json!({ "tab_id": "nope", "name": "x" }),
    );
    assert_eq!(err["code"], "TabNotFound");
}

#[test]
fn every_tab_tool_is_migrated_mutating_and_carries_the_model() {
    for name in TAB_TOOLS {
        assert!(MIGRATED.contains(name), "{name} is not listed as migrated");
        assert!(mutates(name), "{name} must carry the model update");
    }
    // The host mirrors its tab bar from the model that rides with the answer.
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    let response = dispatch(
        &mut state,
        UiToEngine::Tool {
            name: "tab_add".into(),
            arguments: json!({ "name": "Bracket" }),
            context: None,
        },
        &mut kernel,
    );
    let EngineToUi::ToolResult { result, model } = response else {
        panic!("{response:?}")
    };
    assert!(!result.is_error, "{result:?}");
    let model = model.expect("a tab tool's answer carries the model");
    let EngineToUi::ModelUpdated { document, .. } = *model else {
        panic!("{model:?}")
    };
    let document = document.expect("the model names the document");
    assert_eq!(document.tabs.len(), 2);
    assert_eq!(document.tabs[1].name, "Bracket");
    assert_eq!(document.active_tab, document.tabs[1].id);
}

/// Switching back to a Part tab must render its bodies again (found
/// 2026-09-23 through the native host: the switch rebuilt the tree but
/// nothing tessellated it, so `model_summary` and a viewer's snapshot showed
/// the tab empty). Real kernel: the bodies have to exist to be missing.
#[test]
fn tab_switch_back_to_a_part_renders_its_bodies() {
    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    let mut run = |state: &mut EngineState, name: &str, args: Value| -> Value {
        let result = execute_tool(state, &mut kernel, name, &args, None);
        assert!(!result.is_error, "{name} failed: {result:?}");
        result.structured_content
    };
    let first = state.session.tabs()[0].id.clone();
    let sketch = run(
        &mut state,
        "sketch_create",
        json!({
            "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] },
            "entities": [
                { "type": "Point", "id": 1, "x": 0, "y": 0 },
                { "type": "Circle", "id": 2, "center_id": 1, "radius": 0.01 }
            ]
        }),
    );
    run(
        &mut state,
        "feature_add",
        json!({ "operation": { "type": "Extrude", "params": {
            "sketch_id": sketch["feature_id"], "profile_index": 0, "profile_entity_ids": [2],
            "depth": 0.02, "symmetric": false, "cut": false, "combine": { "type": "NewBody" }
        } } }),
    );
    assert_eq!(
        run(&mut state, "model_summary", json!({}))["bodies"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let other = run(
        &mut state,
        "tab_add",
        json!({ "kind": "Part", "name": "Other" }),
    );
    assert_eq!(
        run(&mut state, "model_summary", json!({}))["bodies"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    run(&mut state, "tab_switch", json!({ "tab_id": first }));
    assert_eq!(
        run(&mut state, "model_summary", json!({}))["bodies"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the switched-to tab's bodies are tessellated"
    );
    let _ = other;
}
