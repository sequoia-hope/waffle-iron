//! N1 of `specs/agent_mechanical_design.md` §5.2 end to end: `entity_name`,
//! `entity_unname` and `names_list` over the real kernel.
//!
//! What a test at this layer can show that a feature-engine one cannot: that
//! the name an agent assigns through the tool is stored over the entity's
//! PERSISTENT id (so it survives an edit that rebuilds the body), that every
//! refusal comes back with its own code rather than as `Internal`, and that
//! `face_list` reports the name next to the reference it belongs to.

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

/// A square plate `side` wide and `depth` thick at the origin, as its own
/// body. Returns `(extrude feature id, body id)`.
fn plate(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    side: f64,
    depth: f64,
    base: u32,
) -> (Uuid, String) {
    let corners = [
        (base, 0.0, 0.0),
        (base + 1, side, 0.0),
        (base + 2, side, side),
        (base + 3, 0.0, side),
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
                    depth_expr: None,
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
    tool: &str,
    args: Value,
) -> wasm_bridge::ToolResult {
    execute_tool(
        state,
        kernel,
        tool,
        &args,
        Some(&json!({ "agent_name": "n1-test" })),
    )
}

fn ok(state: &mut EngineState, kernel: &mut KernelV2Adapter, tool: &str, args: Value) -> Value {
    let r = call(state, kernel, tool, args);
    assert!(!r.is_error, "{tool} failed: {r:?}");
    r.structured_content
}

fn refusal(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    tool: &str,
    args: Value,
) -> (String, Value) {
    let r = call(state, kernel, tool, args);
    assert!(r.is_error, "{tool} should have been refused: {r:?}");
    let error = r.structured_content["error"].clone();
    (
        error["code"].as_str().unwrap_or_default().to_string(),
        error,
    )
}

/// A face reference from `face_list`, the one whose signature's centroid is
/// highest in z — the plate's top cap under this sketch frame.
fn top_face_ref(state: &mut EngineState, kernel: &mut KernelV2Adapter, body: &str) -> Value {
    let listed = ok(state, kernel, "face_list", json!({ "body_id": body }));
    let faces = listed["faces"].as_array().expect("faces").clone();
    assert!(!faces.is_empty(), "the plate has faces: {listed}");
    let z = |f: &Value| f["signature"]["centroid"][2].as_f64().unwrap_or(f64::MIN);
    let best = faces
        .iter()
        .max_by(|a, b| z(a).total_cmp(&z(b)))
        .expect("a face");
    best["geom_ref"].clone()
}

#[test]
fn naming_a_face_stores_a_pid_and_the_name_comes_back_with_the_reference() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);

    let out = ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "top_face" }),
    );
    assert_eq!(out["name"], "top_face");
    assert_eq!(out["kind"], json!({ "type": "Face" }));
    assert_eq!(
        out["geom_ref"]["selector"]["type"], "Pid",
        "the stored reference must be a persistent id: {out}"
    );
    assert_eq!(out["geom_ref"]["policy"], json!({ "type": "Strict" }));
    assert_eq!(out["body_id"], json!(body));

    // The provenance is the calling agent's (ICR-4).
    let listed = ok(&mut state, &mut kernel, "names_list", json!({}));
    let entry = listed["names"]
        .as_array()
        .expect("names")
        .iter()
        .find(|n| n["name"] == "top_face")
        .expect("the name is listed")
        .clone();
    assert_eq!(entry["resolves"], true, "{entry}");
    assert_eq!(entry["resolved_by"], "pid", "{entry}");
    assert_eq!(entry["created"]["origin"]["name"], "n1-test");

    // And `face_list` reports it next to the reference it belongs to (§5.2).
    let listed = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    let named: Vec<&Value> = listed["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .filter(|f| f.get("name").is_some())
        .collect();
    assert_eq!(named.len(), 1, "exactly one face is named: {listed}");
    assert_eq!(named[0]["name"], "top_face");
}

#[test]
fn a_named_face_can_be_measured_by_name() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face.clone() }, "name": "top_face" }),
    );

    // A point 100 mm above the plate's own top face: measuring to the NAME
    // must give the same number as measuring to the reference.
    let measured = ok(
        &mut state,
        &mut kernel,
        "body_measure",
        json!({ "body_id": body }),
    );
    let bb = (measured["bbox_min"].clone(), measured["bbox_max"].clone());
    let mid = |k: usize| (bb.0[k].as_f64().unwrap() + bb.1[k].as_f64().unwrap()) / 2.0;
    let probe = json!([mid(0), mid(1), bb.1[2].as_f64().unwrap() + 0.1]);

    let by_name = ok(
        &mut state,
        &mut kernel,
        "measure_distance",
        json!({
            "a": { "type": "name", "name": "top_face" },
            "b": { "type": "point", "point": probe },
        }),
    );
    let by_ref = ok(
        &mut state,
        &mut kernel,
        "measure_distance",
        json!({
            "a": { "type": "entity", "geom_ref": face },
            "b": { "type": "point", "point": probe },
        }),
    );
    assert_eq!(by_name["distance_m"], by_ref["distance_m"], "{by_name}");
    assert_eq!(by_name["method"], "exact");
}

/// A measure by name answers through the same resolution `names_list`
/// reports: once the stored pid is gone the authored fallback answers, in
/// both places. Measured 2026-10-03: reading the stored reference directly
/// made the measure refuse (as `Internal`) a name the listing in the very
/// same state called `resolves: true`.
#[test]
fn a_name_whose_pid_is_gone_still_measures_through_its_fallback() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (extrude, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "top_face" }),
    );

    // Editing the named face's own feature re-stamps its pid (D0 item 1).
    set_depth(&mut state, &mut kernel, extrude, 0.02);
    let entry = listed(&mut state, &mut kernel, "top_face");
    assert_eq!(entry["resolved_by"], "query", "the pid is gone: {entry}");
    assert_eq!(entry["resolves"], true, "{entry}");

    // The point sits 100 mm above the new top cap (z = 20 mm).
    let measured = ok(
        &mut state,
        &mut kernel,
        "measure_distance",
        json!({
            "a": { "type": "name", "name": "top_face" },
            "b": { "type": "point", "point": [0.0, 0.0, 0.12] },
        }),
    );
    assert_eq!(
        measured["distance_m"].as_f64().map(|d| (d - 0.1).abs() < 1e-9),
        Some(true),
        "the fallback's face is 100 mm below the probe point: {measured}"
    );
}

/// Re-extrude the plate at a new depth, through the tool an agent would use.
fn set_depth(state: &mut EngineState, kernel: &mut KernelV2Adapter, extrude: Uuid, depth: f64) {
    let params = {
        let feature = state
            .engine
            .tree
            .features
            .iter()
            .find(|f| f.id == extrude)
            .expect("the extrude");
        let Operation::Extrude { params } = &feature.operation else {
            panic!("not an extrude");
        };
        let mut params = params.clone();
        params.depth = depth;
        params
    };
    let edited = ok(
        state,
        kernel,
        "feature_edit",
        json!({
            "feature_id": extrude.to_string(),
            "operation": { "type": "Extrude", "params": params },
        }),
    );
    assert_eq!(
        edited["errors"],
        json!([]),
        "the edit must rebuild: {edited}"
    );
}

/// One listed name, by name.
fn listed(state: &mut EngineState, kernel: &mut KernelV2Adapter, name: &str) -> Value {
    let all = ok(state, kernel, "names_list", json!({}));
    all["names"]
        .as_array()
        .expect("names")
        .iter()
        .find(|n| n["name"] == name)
        .unwrap_or_else(|| panic!("no name {name} in {all}"))
        .clone()
}

#[test]
fn a_name_survives_an_unrelated_edit_elsewhere_in_the_document_by_pid() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, a) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let (other, _) = plate(&mut state, &mut kernel, 0.02, 0.01, 101);
    let face = top_face_ref(&mut state, &mut kernel, &a);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "a_top" }),
    );

    // An edit to the OTHER body: the named face is untouched, and its
    // persistent identity is untouched with it.
    set_depth(&mut state, &mut kernel, other, 0.03);

    let entry = listed(&mut state, &mut kernel, "a_top");
    assert_eq!(entry["resolves"], true, "{entry}");
    assert_eq!(
        entry["resolved_by"], "pid",
        "an unrelated edit must not cost the name its persistent identity: {entry}"
    );
    assert!(
        entry.get("warnings").is_none(),
        "a clean pid resolution warns about nothing: {entry}"
    );
}

/// An edit to the named face's OWN feature. The name still points at the
/// right face — but through the authored reference, not the pid, and it says
/// so rather than quietly answering.
///
/// This is D0's open item 1 (content-seeded FACE pids, the F4a reseed)
/// measured from up here: face pids are monotonic, so re-executing a feature
/// stamps its faces fresh and the recorded pid is simply gone. Measured
/// 2026-10-03: the plate's top cap was `pid 0`, and after the depth edit no
/// face of the body carried it. The `#[ignore]`d test below is the same claim
/// under the reseed — un-ignore it when it lands.
#[test]
fn a_name_over_an_edited_feature_s_own_face_falls_back_and_says_so() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (extrude, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "top_face" }),
    );
    let stored = listed(&mut state, &mut kernel, "top_face")["geom_ref"]["selector"].clone();

    set_depth(&mut state, &mut kernel, extrude, 0.02);

    let entry = listed(&mut state, &mut kernel, "top_face");
    assert_eq!(
        entry["resolves"], true,
        "the authored reference answers when the pid is gone: {entry}"
    );
    assert_eq!(
        entry["resolved_by"], "query",
        "and the agent is TOLD the persistent identity was lost: {entry}"
    );
    assert!(
        entry["warnings"][0]
            .as_str()
            .unwrap_or_default()
            .contains("persistent id is gone"),
        "{entry}"
    );
    assert_eq!(
        entry["geom_ref"]["selector"], stored,
        "nothing rewrites the stored reference: {entry}"
    );

    // It is still the right face: the top cap, now 20 mm up instead of 10.
    let all = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    let faces = all["faces"].as_array().unwrap();
    let named: Vec<&Value> = faces.iter().filter(|f| f["name"] == "top_face").collect();
    assert_eq!(named.len(), 1, "exactly one face carries the name: {all}");
    let z = named[0]["signature"]["centroid"][2].as_f64().unwrap();
    assert!(
        (z - 0.02).abs() < 1e-9,
        "the name is on the new top cap (z = 20 mm), not somewhere else: z = {z}"
    );
}

#[test]
#[ignore = "D0 item 1: content-seeded FACE pids (the F4a reseed) not landed — an incremental rebuild stamps the edited feature's faces fresh, so a name over one of them loses its pid and falls back to the authored reference (pinned by the test above)"]
fn a_face_name_keeps_its_pid_across_an_edit_to_its_own_feature() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (extrude, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "top_face" }),
    );
    set_depth(&mut state, &mut kernel, extrude, 0.02);
    let entry = listed(&mut state, &mut kernel, "top_face");
    assert_eq!(entry["resolved_by"], "pid", "{entry}");
}

#[test]
fn every_refusal_has_its_own_code() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    let target = json!({ "type": "entity", "geom_ref": face });

    // A name that is not an identifier.
    let (code, error) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": target.clone(), "name": "top face" }),
    );
    assert_eq!(code, "InvalidName", "{error}");
    assert_eq!(error["details"]["name"], "top face");

    // An unknown name.
    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "entity_unname",
        json!({ "name": "nothing_named_this" }),
    );
    assert_eq!(code, "NameNotFound");

    // The same name twice.
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": target.clone(), "name": "top_face" }),
    );
    let (code, error) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": target.clone(), "name": "top_face" }),
    );
    assert_eq!(code, "NameTaken", "{error}");
    assert_eq!(error["details"]["taken_by"], "an entity name");

    // A reference that names nothing.
    let gone = json!({
        "kind": { "type": "Face" },
        "anchor": { "type": "FeatureOutput", "feature_id": Uuid::new_v4().to_string(),
                    "output_key": { "type": "Main" } },
        "selector": { "type": "Pid", "pid": 42, "root_pid": 42 },
        "policy": { "type": "Strict" }
    });
    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": gone }, "name": "nowhere" }),
    );
    assert_eq!(code, "ReferenceNotResolved");

    // A target that is not a target at all.
    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "name": "no_target" }),
    );
    assert_eq!(code, "InvalidArguments");
}

#[test]
fn a_dotted_name_must_match_the_body_and_the_body_name_is_the_first_segment() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    let target = json!({ "type": "entity", "geom_ref": face });

    // The plate's derived name is its feature's ("Extrude"), which is not
    // what the agent wants to type, so it renames the body first — through
    // the same tool, which is what §5.2's "bodies where not already present"
    // means: the body's display name IS its name.
    let renamed = ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "body", "body_id": body }, "name": "plate" }),
    );
    assert_eq!(renamed["name"], "plate");
    assert_eq!(renamed["kind"], json!({ "type": "Solid" }));

    // A wrong first segment is refused, and the refusal names the body.
    let (code, error) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": target.clone(), "name": "bracket.top_face" }),
    );
    assert_eq!(code, "InvalidName", "{error}");
    assert_eq!(error["details"]["body"], "plate", "{error}");

    // The right one is accepted, and keyed by the dotted name.
    let out = ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": target, "name": "plate.top_face" }),
    );
    assert_eq!(out["name"], "plate.top_face");

    // The body name and the entity name share one namespace, so both are
    // listed and neither can be taken twice.
    let listed = ok(&mut state, &mut kernel, "names_list", json!({}));
    let names: Vec<&str> = listed["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["plate", "plate.top_face"], "{listed}");
    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "body", "body_id": body }, "name": "plate" }),
    );
    assert_eq!(code, "NameTaken");
}

#[test]
fn a_name_whose_entity_is_deleted_stays_listed_and_stops_resolving() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (extrude, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "top_face" }),
    );

    ok(
        &mut state,
        &mut kernel,
        "feature_delete",
        json!({ "feature_id": extrude.to_string() }),
    );

    let listed = ok(&mut state, &mut kernel, "names_list", json!({}));
    let entry = listed["names"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "top_face")
        .expect("the name outlives the feature: the agent must see the hole")
        .clone();
    assert_eq!(entry["resolves"], false, "{entry}");
    assert!(entry.get("resolved_by").is_none(), "{entry}");
    assert!(
        !entry["warnings"].as_array().unwrap().is_empty(),
        "the listing says WHY it does not resolve: {entry}"
    );
}

#[test]
fn naming_and_unnaming_are_undo_steps() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let face = top_face_ref(&mut state, &mut kernel, &body);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "top_face" }),
    );
    assert_eq!(state.engine.tree.names.len(), 1);

    ok(&mut state, &mut kernel, "undo", json!({}));
    assert!(state.engine.tree.names.is_empty(), "undo removed the name");
    ok(&mut state, &mut kernel, "redo", json!({}));
    assert!(state.engine.tree.names.contains_key("top_face"));

    ok(
        &mut state,
        &mut kernel,
        "entity_unname",
        json!({ "name": "top_face" }),
    );
    assert!(state.engine.tree.names.is_empty());
    ok(&mut state, &mut kernel, "undo", json!({}));
    assert!(
        state.engine.tree.names.contains_key("top_face"),
        "undo restored the name the unname took"
    );
}

#[test]
fn names_list_can_be_limited_to_one_body() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, a) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let (_, b) = plate(&mut state, &mut kernel, 0.02, 0.01, 101);
    let face_a = top_face_ref(&mut state, &mut kernel, &a);
    let face_b = top_face_ref(&mut state, &mut kernel, &b);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face_a }, "name": "a_top" }),
    );
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face_b }, "name": "b_top" }),
    );

    let listed = ok(
        &mut state,
        &mut kernel,
        "names_list",
        json!({ "body_id": a }),
    );
    let names: Vec<&str> = listed["names"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] != json!({ "type": "Solid" }))
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a_top"], "{listed}");

    // A body the document does not have is `BodyNotFound`, like every other
    // body-scoped tool.
    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "names_list",
        json!({ "body_id": "no-such-body" }),
    );
    assert_eq!(code, "BodyNotFound");
}

/// §5.2's other half of "every `EntityRef` argument accepts a name string in
/// place of a `GeomRef` or body id": a `body_id` argument takes a body's NAME,
/// at every body-scoped tool at once (`require_body` is the one chokepoint).
#[test]
fn a_body_name_works_wherever_a_body_id_does() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "body", "body_id": body }, "name": "plate" }),
    );

    let by_id = ok(
        &mut state,
        &mut kernel,
        "body_measure",
        json!({ "body_id": body }),
    );
    let by_name = ok(
        &mut state,
        &mut kernel,
        "body_measure",
        json!({ "body_id": "plate" }),
    );
    assert_eq!(by_id, by_name, "a name must measure the body it names");
    // And through the other body-scoped tools.
    let faces = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": "plate" }),
    );
    assert_eq!(faces["body_id"], json!(body), "{faces}");
    let mass = ok(
        &mut state,
        &mut kernel,
        "measure_mass",
        json!({ "body_id": "plate" }),
    );
    assert!(mass["volume_m3"].as_f64().unwrap() > 0.0, "{mass}");
}
