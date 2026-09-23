//! The custom-feature-script agent tools in the engine
//! (`specs/custom_features_and_modeling_roadmap.md` §A8, A-M4):
//! `script_run_check`, `script_source_add`, `script_source_get`,
//! `script_source_update`, `script_feature_add` — the agent's authoring loop
//! on `MockKernel`. Real geometry through the same tools is
//! `app/tests/gui/agent-script-tools.spec.js`.

use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use wasm_bridge::*;

const AGENT: &str = "script-test";

const BOX_SCRIPT: &str = r#"// @feature name="Box" version=1
// @param width: length = 0.02 min=0.001
// @param height: length = 0.01
// @param depth: length = 0.005
// @param plane: plane
// @output body: main
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.height);
    let r = sk.finish().regions();
    ctx.log("regions: " + r.len());
    ctx.extrude(r[0], #{ depth: p.depth })
}
"#;

/// The box script with a runtime failure only a run finds.
const BROKEN_AT_RUNTIME: &str = r#"// @feature name="Box" version=1
// @param width: length = 0.02
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.width, p.width);
    let r = sk.finish().regions();
    ctx.extrude(r[7], #{ depth: 0.001 })
}
"#;

fn tool(state: &mut EngineState, name: &str, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": AGENT });
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

fn plane() -> Value {
    json!({ "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] })
}

fn add_box_source(state: &mut EngineState) -> Uuid {
    let added = ok(
        state,
        "script_source_add",
        json!({ "name": "box.rhai", "text": BOX_SCRIPT }),
    );
    Uuid::parse_str(added["source_id"].as_str().unwrap()).unwrap()
}

// ── script_run_check ─────────────────────────────────────────────────────

#[test]
fn run_check_reports_the_interface_of_unsaved_text() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": BOX_SCRIPT }),
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["entry"], "feature");
    let iface = &out["interface"];
    assert_eq!(iface["name"], "Box");
    assert_eq!(iface["version"], 1);
    let params = iface["params"].as_array().unwrap();
    assert_eq!(params.len(), 4);
    assert_eq!(params[0]["name"], "width");
    assert_eq!(params[0]["type"], "length");
    assert_eq!(params[0]["default"], 0.02);
    assert_eq!(params[0]["min"], 0.001);
    assert!(params[0].get("max").is_none(), "no max ⇒ omitted");
    assert_eq!(params[3]["type"], "plane");
    assert!(params[3].get("default").is_none());
    assert_eq!(iface["outputs"][0]["name"], "body");
    assert_eq!(iface["outputs"][0]["kind"], "main");
    assert!(out.get("dry_run").is_none(), "no args ⇒ no dry run");
    // Nothing was added to the document.
    assert!(state.sources.is_empty());
}

#[test]
fn run_check_is_typed_on_header_and_parse_failures() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": "// @param a: nope\nfn feature(ctx, p) {}" }),
    );
    assert_eq!(out["ok"], false);
    assert_eq!(out["error"]["stage"], "header");
    assert!(
        out["error"]["reason"]
            .as_str()
            .unwrap()
            .starts_with("line 1:"),
        "{out}"
    );

    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": "// @feature name=\"x\"\nfn feature(ctx, p) { let = ; }" }),
    );
    assert_eq!(out["ok"], false);
    assert_eq!(out["error"]["stage"], "parse");

    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": "// @feature name=\"x\"\nfn other(ctx, p) { 1 }" }),
    );
    assert_eq!(out["ok"], false);
    assert_eq!(out["error"]["stage"], "parse");
    assert!(out["error"]["reason"]
        .as_str()
        .unwrap()
        .contains("no `fn feature(ctx, p)`"));

    // A named entry that exists passes.
    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": "// @feature name=\"x\"\nfn other(ctx, p) { 1 }", "entry": "other" }),
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["entry"], "other");
}

#[test]
fn run_check_with_args_dry_runs_the_script_without_a_kernel() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": BOX_SCRIPT, "args": { "plane": plane() } }),
    );
    assert_eq!(out["ok"], true);
    let dry = &out["dry_run"];
    assert_eq!(dry["ok"], true, "{dry}");
    assert_eq!(dry["children"], json!(["sketch", "extrude"]));
    assert_eq!(dry["logs"], json!(["regions: 1"]));
    assert_eq!(dry["outputs"], json!(["body"]));

    // A missing required argument is an `args` failure of the dry run, not
    // of the check.
    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": BOX_SCRIPT, "args": {} }),
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["dry_run"]["ok"], false);
    assert_eq!(out["dry_run"]["error"]["stage"], "args");
    assert!(out["dry_run"]["error"]["reason"]
        .as_str()
        .unwrap()
        .contains("`plane` (plane) is required"));

    // A runtime failure is found by the dry run.
    let out = ok(
        &mut state,
        "script_run_check",
        json!({ "text": BROKEN_AT_RUNTIME, "args": { "plane": plane() } }),
    );
    assert_eq!(out["ok"], true);
    assert_eq!(out["dry_run"]["ok"], false);
    assert_eq!(out["dry_run"]["error"]["stage"], "runtime");
}

#[test]
fn run_check_needs_text_or_a_known_source() {
    let mut state = EngineState::new();
    let err = refused(&mut state, "script_run_check", json!({}));
    assert_eq!(err["code"], "InvalidScript");
    let err = refused(
        &mut state,
        "script_run_check",
        json!({ "source_id": Uuid::new_v4() }),
    );
    assert_eq!(err["code"], "SourceNotFound");
}

// ── script_source_add / get ──────────────────────────────────────────────

#[test]
fn source_add_embeds_a_script_source_the_document_carries_and_get_reads_it_back() {
    let mut state = EngineState::new();
    let added = ok(
        &mut state,
        "script_source_add",
        json!({ "name": "box.rhai", "text": BOX_SCRIPT }),
    );
    let id = Uuid::parse_str(added["source_id"].as_str().unwrap()).unwrap();
    assert_eq!(added["name"], "box.rhai");
    assert_eq!(added["interface"]["name"], "Box");

    // In the sources table as an embedded Script source, content in the store.
    let entry = state
        .sources
        .iter()
        .find(|s| s.id == id)
        .expect("in the table");
    assert!(matches!(entry.kind, file_format::SourceKind::Script));
    assert!(matches!(entry.locator, file_format::Locator::Embedded));
    assert!(entry.effective_pack());
    assert_eq!(state.engine.sources.text(id).as_deref(), Some(BOX_SCRIPT));

    let got = ok(&mut state, "script_source_get", json!({ "source_id": id }));
    assert_eq!(got["name"], "box.rhai");
    assert_eq!(got["text"], BOX_SCRIPT);
    assert_eq!(got["check"]["ok"], true);
    assert_eq!(
        got["check"]["interface"]["params"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(got["features"], json!([]));

    // The list form.
    let list = ok(&mut state, "script_source_get", json!({}));
    let scripts = list["scripts"].as_array().unwrap();
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0]["source_id"], json!(id));
    assert_eq!(scripts[0]["feature_name"], "Box");
    assert_eq!(scripts[0]["ok"], true);
    assert_eq!(list["library"], json!(["gear", "sprocket"]));

    // The document round-trips the source (a save writes the embed).
    let saved = dispatch(&mut state, UiToEngine::SaveProject, &mut MockKernel::new());
    let EngineToUi::SaveReady { json_data } = saved else {
        panic!("SaveReady expected, got {saved:?}");
    };
    let doc: Value = serde_json::from_str(&json_data).unwrap();
    let sources = doc["sources"].as_array().expect("sources");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["kind"]["type"], "Script");
    assert!(sources[0]["embed"]["blob"].is_string());
}

#[test]
fn source_add_takes_the_name_from_the_header_and_seeds_from_the_library() {
    let mut state = EngineState::new();
    let added = ok(
        &mut state,
        "script_source_add",
        json!({ "text": BOX_SCRIPT }),
    );
    assert_eq!(added["name"], "Box");

    let gear = ok(
        &mut state,
        "script_source_add",
        json!({ "library": "gear" }),
    );
    assert_eq!(gear["name"], "Spur gear");
    assert_eq!(gear["interface"]["name"], "Spur gear");
    let id = Uuid::parse_str(gear["source_id"].as_str().unwrap()).unwrap();
    assert_eq!(
        state.engine.sources.text(id).as_deref(),
        Some(feature_engine::script::library::GEAR_RHAI)
    );

    let err = refused(
        &mut state,
        "script_source_add",
        json!({ "library": "widget" }),
    );
    assert_eq!(err["code"], "InvalidScript");
    let err = refused(
        &mut state,
        "script_source_add",
        json!({ "library": "gear", "text": BOX_SCRIPT }),
    );
    assert_eq!(err["code"], "InvalidScript");
    let err = refused(&mut state, "script_source_add", json!({}));
    assert_eq!(err["code"], "InvalidScript");
}

#[test]
fn source_add_refuses_a_script_that_does_not_check_and_adds_nothing() {
    let mut state = EngineState::new();
    let err = refused(
        &mut state,
        "script_source_add",
        json!({ "text": "fn feature(ctx, p) { 1 }" }),
    );
    assert_eq!(err["code"], "InvalidScript");
    assert_eq!(err["details"]["stage"], "header");
    assert!(state.sources.is_empty());
    assert!(state.engine.sources.is_empty());

    let err = refused(
        &mut state,
        "script_source_add",
        json!({ "text": "// @feature name=\"x\"\nfn feature(ctx, p) { let = ; }" }),
    );
    assert_eq!(err["details"]["stage"], "parse");
    assert!(state.sources.is_empty());
}

#[test]
fn source_get_refuses_an_unknown_or_non_script_source() {
    let mut state = EngineState::new();
    let err = refused(
        &mut state,
        "script_source_get",
        json!({ "source_id": Uuid::new_v4() }),
    );
    assert_eq!(err["code"], "SourceNotFound");
    let err = refused(
        &mut state,
        "script_source_get",
        json!({ "source_id": "not-a-uuid" }),
    );
    assert_eq!(err["code"], "SourceNotFound");

    // A STEP source is not a script source.
    let entry = file_format::SourceEntry::embedded(
        "part.step",
        file_format::SourceKind::Step,
        "ISO-10303-21;",
    );
    let id = entry.id;
    state.engine.sources.insert_text(id, "ISO-10303-21;");
    state.sources.push(entry);
    let err = refused(&mut state, "script_source_get", json!({ "source_id": id }));
    assert_eq!(err["code"], "SourceNotFound");
    let list = ok(&mut state, "script_source_get", json!({}));
    assert_eq!(list["scripts"], json!([]));
}

// ── script_feature_add ───────────────────────────────────────────────────

#[test]
fn feature_add_places_one_node_named_by_the_script_with_agent_provenance() {
    let mut state = EngineState::new();
    let id = add_box_source(&mut state);
    let added = ok(
        &mut state,
        "script_feature_add",
        json!({ "source_id": id, "args": { "plane": plane(), "width": 0.03 } }),
    );
    let feature_id = Uuid::parse_str(added["feature_id"].as_str().unwrap()).unwrap();
    assert_eq!(added["features_added"], json!([feature_id]));
    assert_eq!(added["errors"], json!([]));

    let feature = state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id == feature_id)
        .unwrap();
    assert_eq!(
        feature.name, "Box",
        "the node takes the script's declared name"
    );
    let feature_engine::types::Operation::Script { params } = &feature.operation else {
        panic!("a Script node");
    };
    assert_eq!(params.source_id, id);
    assert_eq!(params.entry, "feature");
    assert_eq!(params.args["width"], 0.03);
    assert_eq!(
        state.engine.get_result(feature_id).map(|r| r.outputs.len()),
        Some(1)
    );
    let origin = state
        .engine
        .tree
        .provenance
        .get(&feature_id)
        .map(|p| &p.origin);
    assert!(
        matches!(origin, Some(feature_engine::types::ProvenanceOrigin::Agent { name }) if name == AGENT),
        "{origin:?}"
    );

    // feature_get reads the node; the source knows its features.
    let got = ok(
        &mut state,
        "feature_get",
        json!({ "feature_id": feature_id }),
    );
    assert_eq!(got["operation"]["type"], "Script");
    assert!(got.get("error").is_none());
    let src = ok(&mut state, "script_source_get", json!({ "source_id": id }));
    assert_eq!(src["features"][0]["feature_id"], json!(feature_id));
    assert_eq!(src["features"][0]["name"], "Box");
}

#[test]
fn feature_add_with_arg_exprs_drives_the_argument_from_a_design_parameter() {
    let mut state = EngineState::new();
    let id = add_box_source(&mut state);
    ok(
        &mut state,
        "parameters_set",
        json!({ "parameters": [{ "name": "w", "expression": "40" }] }),
    );
    let added = ok(
        &mut state,
        "script_feature_add",
        json!({ "source_id": id, "args": { "plane": plane() }, "arg_exprs": { "width": "w" } }),
    );
    let feature_id = Uuid::parse_str(added["feature_id"].as_str().unwrap()).unwrap();
    let feature = state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id == feature_id)
        .unwrap();
    let feature_engine::types::Operation::Script { params } = &feature.operation else {
        panic!("a Script node");
    };
    assert_eq!(params.arg_exprs["width"], "w");
    assert_eq!(params.arg_values["width"], 40.0, "mm-space, cached");
    assert_eq!(added["errors"], json!([]));

    let err = refused(
        &mut state,
        "script_feature_add",
        json!({ "source_id": id, "args": { "plane": plane() }, "arg_exprs": { "width": 3 } }),
    );
    assert_eq!(err["code"], "InvalidScript");
}

#[test]
fn feature_add_rolls_back_a_node_that_fails_and_names_the_script_error() {
    let mut state = EngineState::new();
    let id = add_box_source(&mut state);
    // No plane: the node's `args` stage fails; the add is undone.
    let err = refused(
        &mut state,
        "script_feature_add",
        json!({ "source_id": id, "args": {} }),
    );
    assert_eq!(err["code"], "FeatureRebuildFailed");
    assert_eq!(err["details"]["rolled_back"], true);
    assert_eq!(err["details"]["engine_error"]["kind"]["type"], "Script");
    assert_eq!(err["details"]["engine_error"]["kind"]["stage"], "args");
    assert!(state.engine.tree.features.is_empty());

    // `keep` leaves it, erroring.
    let kept = ok(
        &mut state,
        "script_feature_add",
        json!({ "source_id": id, "args": {}, "on_error": "keep" }),
    );
    assert_eq!(kept["kept_with_error"], true);
    assert_eq!(state.engine.tree.features.len(), 1);
    let feature_id = kept["feature_id"].as_str().unwrap();
    let got = ok(
        &mut state,
        "feature_get",
        json!({ "feature_id": feature_id }),
    );
    assert!(got["error"]
        .as_str()
        .unwrap()
        .contains("`plane` (plane) is required"));

    let err = refused(
        &mut state,
        "script_feature_add",
        json!({ "source_id": Uuid::new_v4(), "args": {} }),
    );
    assert_eq!(err["code"], "SourceNotFound");
}

// ── script_source_update ─────────────────────────────────────────────────

#[test]
fn source_update_regenerates_every_node_and_rolls_back_a_breaking_edit() {
    let mut state = EngineState::new();
    let id = add_box_source(&mut state);
    let added = ok(
        &mut state,
        "script_feature_add",
        json!({ "source_id": id, "args": { "plane": plane() } }),
    );
    let feature_id = added["feature_id"].as_str().unwrap().to_string();

    // A compatible edit (a new default) regenerates the node cleanly.
    let edited = BOX_SCRIPT.replace("depth: length = 0.005", "depth: length = 0.007");
    let out = ok(
        &mut state,
        "script_source_update",
        json!({ "source_id": id, "text": edited }),
    );
    assert_eq!(out["source_id"], json!(id));
    assert_eq!(out["errors"], json!([]));
    assert_eq!(out["interface"]["params"][2]["default"], 0.007);
    assert_eq!(
        state.engine.sources.text(id).as_deref(),
        Some(edited.as_str())
    );
    let entry = state.sources.iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        entry.content_hash.as_deref(),
        Some(file_format::git_blob_sha1(edited.as_bytes()).as_str()),
        "the entry's hash follows the text"
    );

    // An edit that does not check is refused before anything changes.
    let err = refused(
        &mut state,
        "script_source_update",
        json!({ "source_id": id, "text": "fn feature(ctx, p) {}" }),
    );
    assert_eq!(err["code"], "InvalidScript");
    assert_eq!(
        state.engine.sources.text(id).as_deref(),
        Some(edited.as_str())
    );

    // An edit that breaks the node at runtime is rolled back to the previous
    // text and the node's error is named.
    let err = refused(
        &mut state,
        "script_source_update",
        json!({ "source_id": id, "text": BROKEN_AT_RUNTIME }),
    );
    assert_eq!(err["code"], "FeatureRebuildFailed");
    assert_eq!(err["details"]["rolled_back"], true);
    assert_eq!(err["details"]["feature_id"], feature_id);
    assert_eq!(err["details"]["engine_error"]["kind"]["stage"], "runtime");
    assert_eq!(
        state.engine.sources.text(id).as_deref(),
        Some(edited.as_str())
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);

    // `keep` leaves the broken text in place with the node erroring.
    let kept = ok(
        &mut state,
        "script_source_update",
        json!({ "source_id": id, "text": BROKEN_AT_RUNTIME, "on_error": "keep" }),
    );
    assert_eq!(kept["kept_with_error"], true);
    assert_eq!(kept["errors"][0]["feature_id"], feature_id);
    assert_eq!(
        state.engine.sources.text(id).as_deref(),
        Some(BROKEN_AT_RUNTIME)
    );
}

#[test]
fn source_update_refuses_unknown_sources_and_missing_text() {
    let mut state = EngineState::new();
    let id = add_box_source(&mut state);
    let err = refused(
        &mut state,
        "script_source_update",
        json!({ "source_id": Uuid::new_v4(), "text": BOX_SCRIPT }),
    );
    assert_eq!(err["code"], "SourceNotFound");
    let err = refused(
        &mut state,
        "script_source_update",
        json!({ "source_id": id }),
    );
    assert_eq!(err["code"], "InvalidScript");
}

// ── the engine messages behind the editor ────────────────────────────────

#[test]
fn the_editor_messages_read_check_and_set_a_script_source() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();
    // AddScriptSource accepts text that does not check (work in progress)
    // and reports the check.
    let wip = "// @feature name=\"Draft\"\nfn feature(ctx, p) { let = ; }";
    let added = dispatch(
        &mut state,
        UiToEngine::AddScriptSource {
            name: None,
            text: Some(wip.to_string()),
            library: None,
        },
        &mut kernel,
    );
    let EngineToUi::ScriptSourceAdded {
        source_id,
        name,
        sources,
        check,
    } = added
    else {
        panic!("ScriptSourceAdded expected, got {added:?}");
    };
    assert_eq!(name, "Draft");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].kind, "Script");
    assert!(sources[0].available);
    assert!(!check.ok);
    assert_eq!(check.error.as_ref().unwrap().stage, "parse");

    let read = dispatch(
        &mut state,
        UiToEngine::ReadSource { source_id },
        &mut kernel,
    );
    let EngineToUi::SourceContent { text, kind, .. } = read else {
        panic!("SourceContent expected, got {read:?}");
    };
    assert_eq!(text, wip);
    assert_eq!(kind, "Script");

    let checked = dispatch(
        &mut state,
        UiToEngine::CheckScript {
            source_id: Some(source_id),
            text: None,
            entry: None,
            args: None,
        },
        &mut kernel,
    );
    let EngineToUi::ScriptChecked { check, .. } = checked else {
        panic!("ScriptChecked expected, got {checked:?}");
    };
    assert!(!check.ok);

    let set = dispatch(
        &mut state,
        UiToEngine::SetScriptSource {
            source_id,
            text: BOX_SCRIPT.to_string(),
        },
        &mut kernel,
    );
    assert!(matches!(set, EngineToUi::ModelUpdated { .. }), "{set:?}");
    assert_eq!(
        state.engine.sources.text(source_id).as_deref(),
        Some(BOX_SCRIPT)
    );

    // Not for a non-script source.
    let entry = file_format::SourceEntry::embedded(
        "part.step",
        file_format::SourceKind::Step,
        "ISO-10303-21;",
    );
    let step_id = entry.id;
    state.engine.sources.insert_text(step_id, "ISO-10303-21;");
    state.sources.push(entry);
    let set = dispatch(
        &mut state,
        UiToEngine::SetScriptSource {
            source_id: step_id,
            text: BOX_SCRIPT.to_string(),
        },
        &mut kernel,
    );
    assert!(matches!(set, EngineToUi::Error { .. }), "{set:?}");
    let unknown = dispatch(
        &mut state,
        UiToEngine::ReadSource {
            source_id: Uuid::new_v4(),
        },
        &mut kernel,
    );
    assert!(matches!(unknown, EngineToUi::Error { .. }));
}

#[test]
fn the_script_tools_are_migrated_and_the_right_ones_mutate() {
    for name in [
        "script_run_check",
        "script_source_add",
        "script_source_get",
        "script_source_update",
        "script_feature_add",
    ] {
        assert!(tools::MIGRATED.contains(&name), "{name} is not in MIGRATED");
    }
    assert!(!tools::mutates("script_run_check"));
    assert!(!tools::mutates("script_source_get"));
    assert!(tools::mutates("script_source_add"));
    assert!(tools::mutates("script_source_update"));
    assert!(tools::mutates("script_feature_add"));
}
