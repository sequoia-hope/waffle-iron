//! `sketch_create` in the engine (`specs/waffle_server_mode.md` §2.3 S3 C5).
//!
//! The agreement with the page's implementation is proven end to end by the
//! recorded goldens (`app/tests/gui/fixtures/agent-authoring-goldens.json`),
//! every one of which opens with a `sketch_create`. These tests pin what that
//! cannot reach:
//!
//! - **The datum-plane path, which nothing else covers.** Every agent spec
//!   passes an explicit `{origin, normal}`, and O12 covers a picked FACE ref on
//!   real geometry — so a plane named by a datum ref was, until here, resolved
//!   by code no test ran. The built-in planes resolve without the kernel, so
//!   `MockKernel` reaches them.
//! - **The refusal strings**, which carry a JSON pointer into the caller's
//!   input and are part of the contract (A13).

use feature_engine::types::*;
use serde_json::{json, Value};
use waffle_types::kernel::MockKernel;
use wasm_bridge::*;

const AGENT: &str = "sketch-test";
const FRONT_PLANE_ID: &str = "00000000-0000-0000-0000-000000000001";
const TOP_PLANE_ID: &str = "00000000-0000-0000-0000-000000000002";

fn tool(state: &mut EngineState, args: Value) -> ToolResult {
    let mut kernel = MockKernel::new();
    let context = json!({ "agent_name": AGENT });
    execute_tool(state, &mut kernel, "sketch_create", &args, Some(&context))
}

fn ok(state: &mut EngineState, args: Value) -> Value {
    let result = tool(state, args);
    assert!(!result.is_error, "sketch_create failed: {result:?}");
    result.structured_content
}

fn refused(state: &mut EngineState, args: Value) -> Value {
    let result = tool(state, args);
    assert!(result.is_error, "expected a refusal: {result:?}");
    result.structured_content["error"].clone()
}

fn point(id: u32, x: f64, y: f64) -> Value {
    json!({ "type": "Point", "id": id, "x": x, "y": y })
}

fn line(id: u32, start: u32, end: u32) -> Value {
    json!({ "type": "Line", "id": id, "start_id": start, "end_id": end })
}

/// A 20 × 10 mm rectangle: points 1–4, lines 5–8.
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

/// The plane the committed sketch ended up on.
fn committed_plane(state: &EngineState) -> ([f64; 3], [f64; 3]) {
    let feature = state.engine.tree.features.last().expect("a feature");
    match &feature.operation {
        Operation::Sketch { sketch } => (sketch.plane_origin, sketch.plane_normal),
        other => panic!("expected a Sketch, got {other:?}"),
    }
}

// ── The explicit plane ───────────────────────────────────────────────────

#[test]
fn an_explicit_origin_and_normal_commits_a_sketch() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
        }),
    );

    let id = out["feature_id"].as_str().expect("a feature id");
    assert_eq!(out["features_added"], json!([id]));
    // Four free points and no constraints: the solver has nothing to fix.
    assert_eq!(out["solve_status"], "UnderConstrained");
    assert_eq!(out["dof"], json!(8));
    assert_eq!(out["errors"], json!([]));
    assert_eq!(committed_plane(&state).1, [0.0, 0.0, 1.0]);
}

#[test]
fn the_committed_sketch_carries_the_calling_agent() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
        }),
    );
    let id = state.engine.tree.features[0].id;
    let provenance = state.engine.tree.provenance.get(&id).expect("provenance");
    assert_eq!(
        json!(provenance.origin),
        json!({ "type": "Agent", "name": AGENT })
    );
}

#[test]
fn a_closed_rectangle_reports_its_region() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
        }),
    );

    let regions = out["regions"].as_array().expect("regions");
    assert!(!regions.is_empty(), "the closed loop is a region");
    let area = regions[0]["area_m2"].as_f64().expect("an area");
    assert!(
        (area - 2e-4).abs() < 1e-12,
        "20 × 10 mm = 2e-4 m², got {area}"
    );
    assert!(out.get("regions_error").is_none());
}

// ── The datum-plane path (covered by nothing else) ───────────────────────

#[test]
fn a_built_in_datum_plane_ref_resolves() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        json!({
            "plane": {
                "kind": { "type": "Face" },
                "anchor": { "type": "DatumPlane", "id": FRONT_PLANE_ID },
            },
            "entities": rectangle(),
        }),
    );
    assert_eq!(committed_plane(&state).1, [0.0, 0.0, 1.0], "Front is +Z");
}

#[test]
fn the_top_plane_is_not_the_front_plane() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        json!({
            "plane": {
                "kind": { "type": "Face" },
                "anchor": { "type": "DatumPlane", "id": TOP_PLANE_ID },
            },
            "entities": rectangle(),
        }),
    );
    assert_eq!(committed_plane(&state).1, [0.0, 1.0, 0.0], "Top is +Y");
}

#[test]
fn the_legacy_plane_name_spelling_still_resolves() {
    // `planes.js` still accepts `{plane: "XY"}`; a ref written that way must
    // not silently fail to resolve.
    let mut state = EngineState::new();
    ok(
        &mut state,
        json!({
            "plane": {
                "kind": { "type": "Face" },
                "anchor": { "type": "DatumPlane", "plane": "XY" },
            },
            "entities": rectangle(),
        }),
    );
    assert_eq!(committed_plane(&state).1, [0.0, 0.0, 1.0]);
}

#[test]
fn the_engines_own_datum_anchor_spelling_resolves_too() {
    let mut state = EngineState::new();
    ok(
        &mut state,
        json!({
            "plane": {
                "kind": { "type": "Face" },
                "anchor": { "type": "Datum", "datum_id": TOP_PLANE_ID },
            },
            "entities": rectangle(),
        }),
    );
    assert_eq!(committed_plane(&state).1, [0.0, 1.0, 0.0]);
}

#[test]
fn a_plane_that_names_nothing_is_refused_before_anything_is_committed() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        json!({
            "plane": {
                "kind": { "type": "Face" },
                "anchor": { "type": "DatumPlane", "id": "00000000-0000-0000-0000-0000000000ff" },
            },
            "entities": rectangle(),
        }),
    );
    assert_eq!(error["code"], "InvalidSketch");
    assert_eq!(error["details"]["reason"], "unresolved plane");
    assert!(
        state.engine.tree.features.is_empty(),
        "nothing was committed"
    );
}

// ── Input shape (A13): ids only, with a pointer to the offender ──────────

#[test]
fn a_duplicate_entity_id_names_its_position() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": [point(1, 0.0, 0.0), point(1, 1.0, 1.0)],
        }),
    );
    assert_eq!(error["code"], "InvalidSketch");
    let message = error["message"].as_str().expect("a message");
    assert!(message.contains("/entities/1/id"), "got {message}");
    assert!(state.engine.tree.features.is_empty());
}

#[test]
fn an_edge_that_names_no_point_is_refused() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": [point(1, 0.0, 0.0), line(2, 1, 99)],
        }),
    );
    let message = error["message"].as_str().expect("a message");
    assert!(message.contains("/entities/1/end_id"), "got {message}");
}

#[test]
fn a_constraint_naming_no_entity_is_refused() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
            "constraints": [{ "type": "Horizontal", "entity": 99 }],
        }),
    );
    let message = error["message"].as_str().expect("a message");
    assert!(message.contains("/constraints/0/entity"), "got {message}");
}

// ── The solve ────────────────────────────────────────────────────────────

#[test]
fn a_contradictory_sketch_commits_nothing_by_default() {
    let mut state = EngineState::new();
    let error = refused(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
            "constraints": [
                { "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.02 },
                { "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.03 },
            ],
        }),
    );
    assert_eq!(error["code"], "SketchSolveFailed");
    assert!(
        ["OverConstrained", "SolveFailed"]
            .contains(&error["details"]["status"].as_str().unwrap_or("")),
        "got {:?}",
        error["details"]["status"]
    );
    assert!(
        state.engine.tree.features.is_empty(),
        "nothing was committed"
    );
    // ...and the sketch `BeginSketch` opened was closed again: there is no
    // cancel message, so a refusal that left it open would shadow the user's
    // next sketch.
    assert!(
        state.active_sketch.is_none(),
        "the abandoned sketch is still open"
    );
}

#[test]
fn on_error_keep_commits_a_sketch_that_did_not_solve() {
    let mut state = EngineState::new();
    let out = ok(
        &mut state,
        json!({
            "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] },
            "entities": rectangle(),
            "constraints": [
                { "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.02 },
                { "type": "HDistance", "point_a": 1, "point_b": 2, "value": 0.03 },
            ],
            "on_error": "keep",
        }),
    );
    assert!(
        ["OverConstrained", "SolveFailed"].contains(&out["solve_status"].as_str().unwrap_or(""))
    );
    assert_eq!(state.engine.tree.features.len(), 1, "the sketch stayed");
}

// ── A caller-chosen x axis (docs/notes/eiffel/FEATURE_NOTES.md §3) ───────

/// The committed sketch's own +x direction, if it has one.
fn committed_x_axis(state: &EngineState) -> Option<[f64; 3]> {
    let feature = state.engine.tree.features.last().expect("a feature");
    match &feature.operation {
        Operation::Sketch { sketch } => sketch.plane_x_axis,
        other => panic!("expected a Sketch, got {other:?}"),
    }
}

#[test]
fn a_plane_may_name_its_own_x_axis_and_the_answer_says_where_it_went() {
    let mut state = EngineState::new();
    // Without one, the engine picks: on the XY plane sketch +x is world −y,
    // which is exactly the derivation a caller should not have to reproduce.
    let out = ok(
        &mut state,
        json!({
            "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] },
            "entities": rectangle(),
        }),
    );
    assert_eq!(out["plane"]["x_axis"], json!([0.0, -1.0, 0.0]));
    assert_eq!(out["plane"]["y_axis"], json!([1.0, 0.0, 0.0]));
    assert_eq!(
        committed_x_axis(&state),
        None,
        "nothing stored when none given"
    );

    // With one, sketch +x IS that direction — and it is orthogonalized, so a
    // caller may hand over any vector with an in-plane part.
    let out = ok(
        &mut state,
        json!({
            "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1], "x_axis": [2, 0, 5] },
            "entities": rectangle(),
        }),
    );
    assert_eq!(out["plane"]["x_axis"], json!([1.0, 0.0, 0.0]));
    assert_eq!(out["plane"]["y_axis"], json!([0.0, 1.0, 0.0]));
    assert_eq!(
        committed_x_axis(&state),
        Some([2.0, 0.0, 5.0]),
        "stored verbatim"
    );

    // A datum-named plane takes one too.
    let out = ok(
        &mut state,
        json!({
            "plane": { "anchor": { "type": "DatumPlane", "datum_id": FRONT_PLANE_ID }, "x_axis": [1, 0, 0] },
            "entities": rectangle(),
        }),
    );
    assert_eq!(out["plane"]["x_axis"], json!([1.0, 0.0, 0.0]));
}

#[test]
fn an_x_axis_that_cannot_orient_the_plane_is_refused_with_the_reason() {
    let mut state = EngineState::new();
    let before = state.engine.tree.features.len();
    for bad in [json!([0, 0, 1]), json!([0, 0, 0])] {
        let error = refused(
            &mut state,
            json!({
                "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1], "x_axis": bad },
                "entities": rectangle(),
            }),
        );
        assert_eq!(error["code"], "InvalidSketch", "{error}");
        assert!(
            error["message"]
                .as_str()
                .unwrap_or("")
                .contains("parallel to the normal"),
            "{error}"
        );
    }
    assert_eq!(
        state.engine.tree.features.len(),
        before,
        "a refused x_axis commits nothing"
    );
    // Not three numbers is its own refusal.
    let error = refused(
        &mut state,
        json!({
            "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1], "x_axis": "up" },
            "entities": rectangle(),
        }),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap_or("")
            .contains("three numbers"),
        "{error}"
    );
}
