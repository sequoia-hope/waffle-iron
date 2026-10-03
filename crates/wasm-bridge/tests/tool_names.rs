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
        plane_face: None,
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

/// A square pocket cut into whatever body is already there, `side` wide and
/// `depth` deep, its near corner at `(x0, 5 mm)` in the sketch frame.
///
/// The CUT is a boolean, so its output faces are the ones D0's reseed does
/// NOT stamp: they take counter pids, with a lineage root that leads back to
/// the seeded operand face. Returns `(cut feature id, body id)`.
fn pocket(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    side: f64,
    depth: f64,
    base: u32,
    x0: f64,
) -> (Uuid, String) {
    let corners = [
        (base, x0, 0.005),
        (base + 1, x0 + side, 0.005),
        (base + 2, x0 + side, 0.005 + side),
        (base + 3, x0, 0.005 + side),
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
        plane_face: None,
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
    let cut = added_id(dispatch(
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
                    cut: true,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: Some(CombineMode::Cut),
                    targets: None,
                },
            },
            provenance: None,
        },
        kernel,
    ));
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    (cut, FeatureTree::body_id(cut, &OutputKey::Main))
}

/// The reference for a pocket floor of `body`: a face pointing DOWN that sits
/// above the base plane. `beyond` keeps only floors whose 3-D y is past it, so
/// a model with two pockets can name the far one (the sketch frame maps
/// sketch +x to world −y).
fn pocket_floor_ref(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    body: &str,
    beyond: f64,
) -> Value {
    let all = ok(state, kernel, "face_list", json!({ "body_id": body }));
    let floor = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .find(|f| {
            let c = &f["signature"]["centroid"];
            f["signature"]["normal"][2].as_f64().unwrap_or(0.0) < -0.5
                && c[2].as_f64().unwrap_or(0.0) > 1e-9
                && c[1].as_f64().unwrap_or(0.0) < beyond
        })
        .unwrap_or_else(|| panic!("a pocket floor past y = {beyond}: {all}"));
    floor["geom_ref"].clone()
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
/// reports, in both directions — including the refusal.
///
/// An agent's name is `Strict` (its reference comes from `face_list`, which
/// hands out `Strict` refs), so once the recorded identity is gone the ladder
/// refuses rather than rebinding by geometry (N2 §5.3 item 1, settled
/// 2026-10-03). The measure must refuse the same name the listing calls
/// unresolvable: one question answered one way. The warned-rebind half lives on
/// a `BestEffort` name —
/// `a_best_effort_name_whose_reference_is_gone_still_measures_through_its_fallback`.
///
/// The fixture is the one case that still loses a persistent identity for
/// good — a pocket floor, named, and then turned into a through hole, so
/// neither the pid nor its lineage root is on the body any more.
#[test]
fn a_name_whose_reference_is_gone_refuses_rather_than_measuring_another_face() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, _) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let (cut, body) = pocket(&mut state, &mut kernel, 0.01, 0.004, 500, 0.005);
    let floor = pocket_floor_ref(&mut state, &mut kernel, &body, f64::INFINITY);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": floor }, "name": "floor" }),
    );

    // The pocket becomes a through hole: the floor is gone, and so is the
    // operand face its root led back to.
    set_depth(&mut state, &mut kernel, cut, 0.012);
    let entry = listed(&mut state, &mut kernel, "floor");
    assert_eq!(
        entry["resolves"], false,
        "neither the pid nor its root is left, and a Strict reference does not \
         rebind by geometry: {entry}"
    );
    assert_eq!(entry["refusal"]["type"], "PidGone", "{entry}");
    assert_eq!(
        entry["refusal"]["last_seen_feature"],
        json!(cut),
        "the feature whose output it was last seen in: {entry}"
    );

    // And the measure refuses the same name, with the same account of why.
    let (code, error) = refusal(
        &mut state,
        &mut kernel,
        "measure_distance",
        json!({
            "a": { "type": "name", "name": "floor" },
            "b": { "type": "point", "point": [0.0, 0.0, 0.5] },
        }),
    );
    assert_eq!(code, "Internal", "{error}");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("does not resolve") && message.contains("floor"),
        "the refusal names the name that died: {error}"
    );
}

/// The other half of §5.3 item 1: a `BestEffort` name — what a document written
/// from a user's viewport pick carries — still measures through its authored
/// fallback once the recorded identity is gone, because a person can see the
/// geometry and the UI shows the warning. The listing and the measure agree
/// here too: both rebind.
#[test]
fn a_best_effort_name_whose_reference_is_gone_still_measures_through_its_fallback() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, _) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let (cut, body) = pocket(&mut state, &mut kernel, 0.01, 0.004, 500, 0.005);
    let mut floor = pocket_floor_ref(&mut state, &mut kernel, &body, f64::INFINITY);
    floor["policy"] = json!({ "type": "BestEffort" });
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": floor }, "name": "floor" }),
    );

    set_depth(&mut state, &mut kernel, cut, 0.012);
    let entry = listed(&mut state, &mut kernel, "floor");
    assert_eq!(
        entry["resolved_by"], "query",
        "neither the pid nor its root is left: {entry}"
    );
    assert_eq!(entry["resolves"], true, "{entry}");
    assert_eq!(entry["rebound"], true, "and it says it rebound: {entry}");

    let measured = ok(
        &mut state,
        &mut kernel,
        "measure_distance",
        json!({
            "a": { "type": "name", "name": "floor" },
            "b": { "type": "point", "point": [0.0, 0.0, 0.5] },
        }),
    );
    assert!(
        measured["distance_m"].as_f64().unwrap_or(0.0) > 0.0,
        "a name the listing calls resolvable must measure: {measured}"
    );
    assert_eq!(measured["method"], "exact", "{measured}");
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

/// An edit to the named face's OWN feature. Since D0 item 1 (content-seeded
/// face pids) the name keeps its persistent identity: the seed is the
/// feature's uuid and the role is the face's position in its constructor's
/// output, so re-executing the step hands the same face the same id.
///
/// Before the reseed this was the loud-fallback case, pinned the other way
/// (the plate's top cap was `pid 0` and no face carried it after the edit).
#[test]
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
    let stored = listed(&mut state, &mut kernel, "top_face")["geom_ref"]["selector"].clone();

    set_depth(&mut state, &mut kernel, extrude, 0.02);

    let entry = listed(&mut state, &mut kernel, "top_face");
    assert_eq!(entry["resolves"], true, "{entry}");
    assert_eq!(
        entry["resolved_by"], "pid",
        "the seeded pid survives its own feature's re-execution: {entry}"
    );
    assert!(
        entry.get("warnings").is_none(),
        "a clean pid resolution warns about nothing: {entry}"
    );
    assert_eq!(
        entry["geom_ref"]["selector"], stored,
        "nothing rewrites the stored reference: {entry}"
    );

    // And it is the right face: the top cap, now 20 mm up instead of 10.
    let z = named_face(&mut state, &mut kernel, &body, "top_face")["signature"]["centroid"][2]
        .as_f64()
        .expect("a centroid");
    assert!(
        (z - 0.02).abs() < 1e-9,
        "the name is on the new top cap (z = 20 mm): z = {z}"
    );
}

/// A BOOLEAN's own output face is the family the reseed does not cover: its
/// pid is still counter-allocated, and only its lineage ROOT is content-
/// seeded. Editing the boolean therefore loses the face's own id, and the
/// root answers instead — correctly, and with the resolver's warning saying
/// so. That warning is the honest signal, so it is pinned.
#[test]
fn a_name_on_a_boolean_output_face_answers_through_its_root_and_says_so() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, plate_body) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    let (cut, body) = pocket(&mut state, &mut kernel, 0.01, 0.004, 500, 0.005);
    assert_ne!(body, plate_body, "the cut's output is its own body");
    // A plate wall that SURVIVED the cut: a boolean output face whose root
    // leads back to the extrude's seeded face.
    let all = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body.clone() }),
    );
    let wall = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .max_by(|a, b| {
            let x = |f: &Value| f["signature"]["centroid"][0].as_f64().unwrap_or(f64::MIN);
            x(a).total_cmp(&x(b))
        })
        .expect("a wall")["geom_ref"]
        .clone();
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": wall }, "name": "far_wall" }),
    );

    set_depth(&mut state, &mut kernel, cut, 0.006);

    let entry = listed(&mut state, &mut kernel, "far_wall");
    assert_eq!(entry["resolves"], true, "{entry}");
    assert_eq!(entry["resolved_by"], "pid", "{entry}");
    // D0 item 1b (merged 2026-10-03, after this pin was written): a boolean
    // output face's own pid is now seeded from (op seed, lineage root, rank),
    // so an edit to the cut's depth leaves the wall's number intact and there
    // is nothing to recover through the root and nothing to warn about. The
    // loud fallback this pin used to hold is exercised directly by the
    // `a_recycled_pid_*` pins, which construct the recycling by hand.
    let warnings = entry["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        !warnings.iter().any(|w| w
            .as_str()
            .unwrap_or_default()
            .contains("resolved through its lineage root")),
        "with seeded boolean output pids the number itself survives the edit: {entry}"
    );
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

/// The one namespace is enforced in BOTH directions (N1 §5.2). `entity_name`
/// already refused a name a body held; `body_rename` must refuse one an
/// entity holds, or the same string would answer as the entity through a
/// `{"type":"name"}` operand and as the body through `require_body`.
#[test]
fn a_body_cannot_be_renamed_onto_an_entity_name() {
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

    let (code, error) = refusal(
        &mut state,
        &mut kernel,
        "body_rename",
        json!({ "body_id": body.clone(), "new_name": "top_face" }),
    );
    assert_eq!(code, "NameTaken", "{error}");
    assert_eq!(error["details"]["taken_by"], "an entity name", "{error}");

    // The other direction, which already held: a body's display name is not
    // available to an entity either.
    ok(
        &mut state,
        &mut kernel,
        "body_rename",
        json!({ "body_id": body.clone(), "new_name": "plate" }),
    );
    let face = top_face_ref(&mut state, &mut kernel, &body);
    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "plate" }),
    );
    assert_eq!(code, "NameTaken");
}

/// An `n`-gon prism (n + 2 faces) as its own body, so a document can be
/// rebuilt with one body carrying a different number of faces than before.
/// Returns `(sketch feature id, extrude feature id, body id)`.
fn ngon_prism(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    n: u32,
    r: f64,
    depth: f64,
    base: u32,
) -> (Uuid, Uuid, String) {
    let sketch = ngon_sketch(n, r, base);
    let profile = sketch.solved_profiles[0].entity_ids.clone();
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
                    profile_entity_ids: Some(profile),
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
    (
        sketch_feature,
        extrude,
        FeatureTree::body_id(extrude, &OutputKey::Main),
    )
}

/// A regular `n`-gon on the XY plane, entity ids from `base`.
fn ngon_sketch(n: u32, r: f64, base: u32) -> Sketch {
    let mut entities = Vec::new();
    let mut solved_positions: HashMap<u32, (f64, f64)> = HashMap::new();
    for i in 0..n {
        let a = std::f64::consts::TAU * f64::from(i) / f64::from(n);
        let (x, y) = (r * a.cos(), r * a.sin());
        entities.push(point(base + i, x, y));
        solved_positions.insert(base + i, (x, y));
    }
    let mut profile = Vec::new();
    for i in 0..n {
        let id = base + 100 + i;
        profile.push(id);
        entities.push(line(id, base + i, base + (i + 1) % n));
    }
    Sketch {
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
        plane_face: None,
        solved_profiles: vec![ClosedProfile {
            entity_ids: profile,
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    }
}

/// The face of `body` that carries `name`, by its `face_list` entry.
fn named_face(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    body: &str,
    name: &str,
) -> Value {
    let all = ok(state, kernel, "face_list", json!({ "body_id": body }));
    let faces: Vec<&Value> = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .filter(|f| f["name"] == json!(name))
        .collect();
    assert_eq!(faces.len(), 1, "exactly one face carries {name}: {all}");
    faces[0].clone()
}

/// A name must never answer `resolved_by: "pid"` for an entity that is not
/// the one it was given to.
///
/// Before D0 item 1 this was the branch's worst defect, and it is why the
/// pin exists. Face pids were a per-arena monotonic counter
/// (`BrepArena::alloc_pid`), so a reopened document numbered every face
/// again from the recipe: a document edited ELSEWHERE — here a hexagonal
/// prism that became a pentagonal one, one face fewer — shifted the numbers
/// of every body built after it, and the untouched body's stored pid named
/// its NEIGHBOUR, reported `resolves: true`, `resolved_by: "pid"`, no
/// warnings. Measured 2026-10-03: the name moved from the second body's top
/// cap (z = 2 mm) to its bottom cap (z = 0).
///
/// Green since the reseed (`crates/kernel-v2/src/pid.rs::seeded_face_pid`):
/// a face's id is a function of its step's uuid and its role in that step's
/// output, so nothing about another feature's face count can reach it.
/// Re-confirmed by withdrawing the seed in `feature_engine::rebuild` — the
/// assertion below goes red again.
#[test]
fn a_name_does_not_follow_a_reused_pid_onto_another_face_after_a_reload() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    // Body A first, so body B's faces are numbered after it.
    let (sketch_a, extrude_a, _body_a) = ngon_prism(&mut state, &mut kernel, 6, 0.010, 0.002, 1);
    let (_, _, body_b) = ngon_prism(&mut state, &mut kernel, 3, 0.004, 0.002, 1000);
    let face = top_face_ref(&mut state, &mut kernel, &body_b);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": face }, "name": "b_top" }),
    );
    let authored = named_face(&mut state, &mut kernel, &body_b, "b_top");

    // The document is edited elsewhere: body A loses one face. Written into
    // the tree and saved, which is what a session that edited body A leaves
    // on disk; body B is untouched.
    let pentagon = ngon_sketch(5, 0.010, 1);
    let profile = pentagon.solved_profiles[0].entity_ids.clone();
    for feature in state.engine.tree.features.iter_mut() {
        if feature.id == sketch_a {
            feature.operation = Operation::Sketch {
                sketch: pentagon.clone(),
            };
        }
        if feature.id == extrude_a {
            if let Operation::Extrude { params } = &mut feature.operation {
                params.profile_entity_ids = Some(profile.clone());
            }
        }
    }
    let json_data = match dispatch(&mut state, UiToEngine::SaveProject, &mut kernel) {
        EngineToUi::SaveReady { json_data } => json_data,
        other => panic!("expected SaveReady, got {other:?}"),
    };

    // Reopened in a fresh engine and a fresh arena: every face is numbered
    // again from the recipe.
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    match dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json_data },
        &mut kernel,
    ) {
        EngineToUi::ModelUpdated { errors, .. } => {
            assert!(errors.is_empty(), "the reload rebuilds: {errors:?}")
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(&mut state, &mut kernel);

    let entry = listed(&mut state, &mut kernel, "b_top");
    let reloaded = named_face(&mut state, &mut kernel, &body_b, "b_top");
    assert_eq!(
        reloaded["signature"]["centroid"], authored["signature"]["centroid"],
        "the name must still be on the face it was given to, or refuse: {entry}"
    );
}

/// The same claim as the test above, for the face family the reseed does NOT
/// cover: a BOOLEAN's own output faces — and the net that keeps it loud.
///
/// Their pids are still counter-allocated: `boolean/from_yang.rs` withdraws
/// the construct seed around its `finalize_solid`, because a boolean output's
/// identity is its journal lineage and `pid::solid_pids` reads the root. Only
/// that root is content-seeded. So the counter is still live for this family,
/// and a reopened document re-mints its numbers from the recipe: an edit that
/// changes an EARLIER boolean's output face count shifts them all.
///
/// Measured 2026-10-03 on the model below — one plate, two pockets, the body
/// being the second cut's output. The name was given to the second pocket's
/// FLOOR (`pid 22`). The first pocket was then deepened into a through hole,
/// which costs that cut's output its own floor. Before the root cross-check,
/// the reopened name sat on the second pocket's SIDE WALL while the floor it
/// was given to was still there unnamed, reported `resolves: true`,
/// `resolved_by: "pid"`, no warnings at all.
///
/// `resolve_by_pid` now requires the recorded `root_pid` as well as the
/// number, so a recycled number is not a match: the floor descends from the
/// cutter's end cap and the wall from its lateral, their roots differ, and
/// resolution falls through to the recorded root — which still names the
/// floor. The name is where it belongs and the warning says the number was
/// re-minted. That is the loud outcome this pin holds. The IDEAL, where the
/// number itself survives and there is nothing to warn about, is the
/// `#[ignore]`d pin below.
#[test]
fn a_name_on_a_boolean_output_face_does_not_move_after_a_reload() {
    let (entry, authored, reloaded) = boolean_output_name_across_a_reload();
    assert_eq!(
        reloaded["signature"]["centroid"], authored["signature"]["centroid"],
        "the name must still be on the face it was given to: {entry}"
    );
    assert_eq!(entry["resolves"], true, "{entry}");
    // D0 item 1b (merged 2026-10-03): the number itself now survives the
    // reopen, so there is no re-mint to warn about. The root cross-check
    // stays as the net for a pid that is NOT seeded (an unattributable
    // output face keeps a counter id) — the `a_recycled_pid_*` pins hold it.
    let warnings = entry["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        !warnings.iter().any(|w| w
            .as_str()
            .unwrap_or_default()
            .contains("re-minted onto something else")),
        "the recorded id still names the floor, no re-mint: {entry}"
    );
}

/// The ideal the pin above settles for less than: a boolean output face's own
/// id survives a reopen, so the name resolves by number with nothing to warn
/// about. Red until the stamping pass moves after `boolean_op` records the
/// journal (`H(root, rank within the root's split group)`), because only then
/// is a boolean output's own pid content-derived rather than counter-allocated.
#[test]
fn a_boolean_output_face_keeps_its_own_pid_across_a_reload() {
    let (entry, authored, reloaded) = boolean_output_name_across_a_reload();
    assert_eq!(
        reloaded["signature"]["centroid"], authored["signature"]["centroid"],
        "{entry}"
    );
    assert_eq!(entry["resolved_by"], "pid", "{entry}");
    assert!(
        entry.get("warnings").is_none(),
        "the recorded id itself survived, so there is nothing to warn about: {entry}"
    );
}

/// One plate, two pockets; the body is the SECOND cut's output, so its
/// counter pids are allocated after the first cut's. Name the second
/// pocket's floor, deepen the FIRST pocket into a through hole (its boolean
/// output loses a face, shifting every counter pid after it), save, and
/// reopen in a fresh engine and kernel.
///
/// Returns `(the name's listing entry, the face as authored, the face the
/// name is on after the reopen)`.
fn boolean_output_name_across_a_reload() -> (Value, Value, Value) {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, _) = plate(&mut state, &mut kernel, 0.04, 0.01, 1);
    // Two pockets in one plate. The body is the SECOND cut's output, so its
    // counter pids are allocated after the first cut's.
    let (first_cut, _) = pocket(&mut state, &mut kernel, 0.01, 0.004, 500, 0.005);
    let (_, body) = pocket(&mut state, &mut kernel, 0.01, 0.004, 900, 0.025);
    let floor = pocket_floor_ref(&mut state, &mut kernel, &body, -0.02);
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": floor }, "name": "p2_floor" }),
    );
    let authored = named_face(&mut state, &mut kernel, &body, "p2_floor");

    // The FIRST pocket becomes a through hole: its boolean output loses a
    // face, so every counter pid allocated after it shifts on a full rebuild.
    set_depth(&mut state, &mut kernel, first_cut, 0.012);
    let json_data = match dispatch(&mut state, UiToEngine::SaveProject, &mut kernel) {
        EngineToUi::SaveReady { json_data } => json_data,
        other => panic!("expected SaveReady, got {other:?}"),
    };

    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    match dispatch(
        &mut state,
        UiToEngine::LoadProject { data: json_data },
        &mut kernel,
    ) {
        EngineToUi::ModelUpdated { errors, .. } => {
            assert!(errors.is_empty(), "the reload rebuilds: {errors:?}")
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(&mut state, &mut kernel);

    let entry = listed(&mut state, &mut kernel, "p2_floor");
    let reloaded = named_face(&mut state, &mut kernel, &body, "p2_floor");
    (entry, authored, reloaded)
}
