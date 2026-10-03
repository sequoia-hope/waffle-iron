//! Q6 of `specs/agent_mechanical_design.md` §4.2/§4.3 end to end: the
//! `entity_list` tool over the real kernel.
//!
//! The kernel-v2 suite owns the arc-length oracles
//! (`crates/kernel-v2/tests/q6_edge_length.rs`). What this file pins is the
//! layer a host crosses — tool args → `UiToEngine` → the kernel's own doors →
//! the wire shape — and the §4.4 oracles that only exist at this layer: the
//! 6/12/8 counts with exact edge lengths, a rim at `2πr`, a name that
//! round-trips from `entity_name` back into the listing, a listing that is
//! BYTE-IDENTICAL across a from-scratch rebuild in a fresh process, and
//! filters that compose.

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

fn extruded(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    sketch: Sketch,
    profile: Vec<u32>,
    depth: f64,
) -> String {
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
    // The host tessellates after every dispatch; a listing needs a rendered
    // body.
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    FeatureTree::body_id(extrude, &OutputKey::Main)
}

/// An `sx` × `sy` × `depth` box as its own body.
fn block(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    sx: f64,
    sy: f64,
    depth: f64,
    base: u32,
) -> String {
    let corners = [
        (base, 0.0, 0.0),
        (base + 1, sx, 0.0),
        (base + 2, sx, sy),
        (base + 3, 0.0, sy),
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
        plane_face: None,
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
    extruded(state, kernel, sketch, vec![l0, l1, l2, l3], depth)
}

/// A cylinder of radius `r` and height `h` as its own body.
fn cylinder(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    r: f64,
    h: f64,
    base: u32,
) -> String {
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane_face: None,
        plane: datum_xy(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: Vec::new(),
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        projected: Vec::new(),
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![base],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(CircleProfile {
                center_u: 0.0,
                center_v: 0.0,
                radius: r,
            }),
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    };
    extruded(state, kernel, sketch, vec![base], h)
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
        Some(&json!({ "agent_name": "q6-test" })),
    )
}

#[track_caller]
fn ok(state: &mut EngineState, kernel: &mut KernelV2Adapter, tool: &str, args: Value) -> Value {
    let r = call(state, kernel, tool, args);
    assert!(!r.is_error, "{tool} failed: {r:?}");
    r.structured_content
}

fn listing(state: &mut EngineState, kernel: &mut KernelV2Adapter, body: &str, kind: &str) -> Value {
    ok(
        state,
        kernel,
        "entity_list",
        json!({ "body_id": body, "kind": kind }),
    )
}

fn entities(v: &Value) -> Vec<Value> {
    v["entities"].as_array().cloned().unwrap_or_default()
}

/// §4.4: "a box lists 6/12/8 with exact edge lengths."
#[test]
fn a_box_lists_six_faces_twelve_edges_eight_vertices() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (sx, sy, depth) = (0.030, 0.020, 0.010);
    let body = block(&mut state, &mut kernel, sx, sy, depth, 1);

    for (kind, want) in [("face", 6), ("edge", 12), ("vertex", 8)] {
        let out = listing(&mut state, &mut kernel, &body, kind);
        assert_eq!(out["kind"], json!(kind));
        assert_eq!(out["count"], json!(want), "{kind}: {out}");
        assert_eq!(entities(&out).len(), want, "{kind}");
        // Every entity carries its persistent identity and a reference.
        for e in entities(&out) {
            // A decimal STRING, not a number: these ids are content-seeded
            // `u64`s and a JSON number in JavaScript is an `f64`
            // (`waffle_types::pid_str`). `is_u64()` here is what the Q6
            // listing shipped with, and it is what let the hazard through.
            assert!(e["pid"].is_string(), "{kind}'s pid is a string: {e}");
            assert!(
                e["pid"].as_str().unwrap().parse::<u64>().is_ok(),
                "{kind}'s pid is a decimal u64: {e}"
            );
            assert!(e["root_pid"].is_string(), "{kind}'s root is a string: {e}");
            assert!(e["geom_ref"].is_object(), "{kind} has a ref: {e}");
            assert!(e["signature"].is_object(), "{kind} has a signature: {e}");
        }
    }

    // The 12 edge lengths are the box's own, exactly, four of each.
    let edges = entities(&listing(&mut state, &mut kernel, &body, "edge"));
    let mut lengths: Vec<f64> = edges
        .iter()
        .map(|e| {
            assert_eq!(
                e["length"]["method"], "exact",
                "a straight edge is exact: {e}"
            );
            assert_eq!(e["length"]["curve_type"], "line");
            assert_eq!(e["length"]["closed"], json!(false));
            assert!(
                e["length"]["residual_m"].is_null() && e["length"]["chord_bound_m"].is_null(),
                "an exact length carries no band: {e}"
            );
            e["length"]["arc_length_m"].as_f64().expect("a length")
        })
        .collect();
    lengths.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    assert_eq!(
        lengths,
        vec![depth, depth, depth, depth, sy, sy, sy, sy, sx, sx, sx, sx],
        "the twelve edges are the box's three side lengths, four each, exactly"
    );

    // A vertex carries its position, and it is one of the box's 8 corners.
    for v in entities(&listing(&mut state, &mut kernel, &body, "vertex")) {
        let p = v["position"].as_array().cloned().expect("a position");
        assert_eq!(p.len(), 3);
        let z = p[2].as_f64().expect("z");
        assert!(
            z.abs() < 1e-15 || (z - depth).abs() < 1e-15,
            "a corner sits on one of the two caps: {v}"
        );
        assert!(v["length"].is_null(), "a vertex has no arc length: {v}");
    }

    // The body frame is Q3's own answer, at Q3's own tier.
    let frame = &listing(&mut state, &mut kernel, &body, "face")["body"];
    assert_eq!(frame["method"], "exact", "{frame}");
    assert_eq!(
        frame["principal_axes"].as_array().map(Vec::len),
        Some(3),
        "{frame}"
    );
    let c = frame["centroid"].as_array().cloned().expect("a centroid");
    assert!(
        (c[2].as_f64().unwrap() - depth / 2.0).abs() < 1e-15,
        "a box's centroid is at half its height: {frame}"
    );
    assert!(frame["unavailable"].is_null(), "{frame}");
}

/// §4.4: "a cylinder's rim arc length = 2πr exact." And the axes: the lateral
/// face reports the axis LINE (a point on it plus a direction), while the caps
/// report their normal — which is what §4.2 asks the listing to carry.
#[test]
fn a_cylinder_rim_is_two_pi_r_and_the_lateral_reports_its_axis() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (r, h) = (0.025, 0.040);
    let body = cylinder(&mut state, &mut kernel, r, h, 1);

    let edges = entities(&listing(&mut state, &mut kernel, &body, "edge"));
    let rims: Vec<&Value> = edges
        .iter()
        .filter(|e| e["length"]["curve_type"] == "circle")
        .collect();
    assert_eq!(rims.len(), 2, "two rims: {edges:?}");
    for rim in &rims {
        assert_eq!(
            rim["length"]["arc_length_m"].as_f64(),
            Some(std::f64::consts::TAU * r),
            "a rim is exactly 2πr: {rim}"
        );
        assert_eq!(rim["length"]["method"], "exact");
        assert_eq!(rim["length"]["closed"], json!(true));
        // The axis of a circular edge: its centre and its plane normal.
        assert_eq!(rim["axis"]["kind"], "circular", "{rim}");
        assert_eq!(rim["axis"]["radius"].as_f64(), Some(r), "{rim}");
        let d = rim["axis"]["direction"].as_array().cloned().expect("a dir");
        assert!(
            d[2].as_f64().unwrap().abs() > 0.999,
            "a rim of a z-extruded cylinder is normal to z: {rim}"
        );
    }

    let faces = entities(&listing(&mut state, &mut kernel, &body, "face"));
    assert_eq!(faces.len(), 3, "two caps and a lateral: {faces:?}");
    let lateral = faces
        .iter()
        .find(|f| f["signature"]["surface_type"] == "cylindrical")
        .expect("a cylindrical face");
    // N0: a full-turn face has NO point normal, and carries its axis
    // descriptor instead. The axis LINE comes from `entity_axis`, which is the
    // only door to a point on it.
    assert!(
        lateral["signature"]["normal"].is_null(),
        "a full-turn lateral has no point normal: {lateral}"
    );
    assert_eq!(
        lateral["signature"]["axis"]["radius"].as_f64(),
        Some(r),
        "{lateral}"
    );
    assert_eq!(
        lateral["signature"]["axis"]["extent"].as_f64(),
        Some(h),
        "the descriptor says how far the face reaches along the axis: {lateral}"
    );
    assert_eq!(lateral["axis"]["kind"], "cylindrical", "{lateral}");
    assert_eq!(lateral["axis"]["radius"].as_f64(), Some(r), "{lateral}");
    assert_eq!(
        lateral["axis"]["direction"],
        json!([0.0, 0.0, 1.0]),
        "{lateral}"
    );

    // A planar cap reports a normal and no axis — the plane's orientation IS
    // its normal (§4.2).
    let cap = faces
        .iter()
        .find(|f| f["signature"]["surface_type"] == "planar")
        .expect("a planar cap");
    assert!(cap["axis"].is_null(), "a plane has no axis: {cap}");
    assert!(cap["signature"]["normal"].is_array(), "{cap}");
}

/// A name assigned with `entity_name` comes back in the listing, on the very
/// entity it was given to — and the name glob finds it.
#[test]
fn a_name_round_trips_from_entity_name_into_the_listing() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = block(&mut state, &mut kernel, 0.030, 0.020, 0.010, 1);

    let edges = entities(&listing(&mut state, &mut kernel, &body, "edge"));
    let target = edges[0].clone();
    let _ = ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({
            "target": { "type": "entity", "geom_ref": target["geom_ref"] },
            "name": "front_edge",
        }),
    );

    let named = entities(&listing(&mut state, &mut kernel, &body, "edge"));
    let found: Vec<&Value> = named
        .iter()
        .filter(|e| e["name"] == json!("front_edge"))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "exactly one edge carries the name: {named:?}"
    );
    assert_eq!(
        found[0]["pid"], target["pid"],
        "and it is the edge the name was given to"
    );

    // The glob arm selects it, and only it.
    let globbed = ok(
        &mut state,
        &mut kernel,
        "entity_list",
        json!({ "body_id": body, "kind": "edge", "filter": { "name": "front_*" } }),
    );
    assert_eq!(globbed["count"], json!(1), "{globbed}");
    assert_eq!(entities(&globbed)[0]["pid"], target["pid"]);

    // A glob that matches nothing is an empty listing, not an error — and an
    // UNNAMED entity never matches a glob, so the other eleven are gone.
    let none = ok(
        &mut state,
        &mut kernel,
        "entity_list",
        json!({ "body_id": body, "kind": "edge", "filter": { "name": "back_*" } }),
    );
    assert_eq!(none["count"], json!(0), "{none}");

    // A body NAME works wherever a body id does (N1's `require_body`).
    let _ = ok(
        &mut state,
        &mut kernel,
        "body_rename",
        json!({ "body_id": body, "new_name": "bracket" }),
    );
    let by_name = listing(&mut state, &mut kernel, "bracket", "edge");
    assert_eq!(by_name["count"], json!(12), "{by_name}");
    assert_eq!(
        by_name["body_id"],
        json!(body),
        "the answer echoes the body ID, whichever way it was named"
    );
}

/// §4.3 filters COMPOSE: each arm narrows, and all of them together narrow to
/// the intersection. The `query` arm is the same `TopoQuery` vocabulary
/// `face_list` takes, so one filter language serves both.
#[test]
fn filters_compose_and_each_arm_narrows() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (sx, sy, depth) = (0.030, 0.020, 0.010);
    let body = block(&mut state, &mut kernel, sx, sy, depth, 1);
    let count = |state: &mut EngineState, kernel: &mut KernelV2Adapter, filter: Value| -> u64 {
        ok(
            state,
            kernel,
            "entity_list",
            json!({ "body_id": body, "kind": "face", "filter": filter }),
        )["count"]
            .as_u64()
            .expect("a count")
    };

    // The six faces of a box are all planar.
    assert_eq!(
        count(
            &mut state,
            &mut kernel,
            json!({ "query": { "filters": [{ "type": "SurfaceType", "surface_type": "planar" }] } })
        ),
        6
    );
    // One of them points at +z.
    let up = json!({ "query": { "filters": [
        { "type": "SurfaceType", "surface_type": "planar" },
        { "type": "NormalDirection", "direction": [0.0, 0.0, 1.0], "tolerance": 0.01 },
    ] } });
    assert_eq!(count(&mut state, &mut kernel, up.clone()), 1);

    // The bbox arm alone: the slab from z = depth up contains only the top
    // cap. CONTAINMENT, not overlap — the four side walls reach down to z = 0.
    let top_slab = json!({ "bbox": [[-1.0, -1.0, depth - 1e-9], [1.0, 1.0, 1.0]] });
    assert_eq!(count(&mut state, &mut kernel, top_slab.clone()), 1);
    // And it is the same face the normal filter found, so composing the two
    // keeps it while composing with the OPPOSITE normal keeps nothing.
    let mut composed = up.as_object().cloned().expect("object");
    composed.extend(top_slab.as_object().cloned().expect("object"));
    assert_eq!(count(&mut state, &mut kernel, Value::Object(composed)), 1);

    let mut contradiction = top_slab.as_object().cloned().expect("object");
    contradiction.insert(
        "query".to_string(),
        json!({ "filters": [
            { "type": "NormalDirection", "direction": [0.0, 0.0, -1.0], "tolerance": 0.01 },
        ] }),
    );
    assert_eq!(
        count(&mut state, &mut kernel, Value::Object(contradiction)),
        0,
        "the top cap does not point down, so the composition is empty"
    );

    // A face has no name, so a name glob on this body excludes all six.
    assert_eq!(count(&mut state, &mut kernel, json!({ "name": "*" })), 0);

    // …and that exclusion is a real ANSWER, not a failure to evaluate, so it
    // must not show up as unevaluable. Every arm on this box is answerable,
    // so the counter is 0 throughout — which is what makes a non-zero one
    // mean something when a degenerate signature does turn up.
    for filter in [
        json!({ "name": "*" }),
        top_slab.clone(),
        json!({ "bbox": [[9.0, 9.0, 9.0], [10.0, 10.0, 10.0]] }),
        up.clone(),
    ] {
        let answer = ok(
            &mut state,
            &mut kernel,
            "entity_list",
            json!({ "body_id": body, "kind": "face", "filter": filter }),
        );
        assert_eq!(
            answer["excluded_unevaluable"],
            json!(0),
            "every face of a box answers every arm: {answer}"
        );
    }
}

/// §4.3: an empty answer must say WHICH kind of empty it is.
///
/// `excluded_unevaluable` is present on every answer, filtered or not, and
/// counts the entities an arm could not be asked of. Without it an agent that
/// filters by region and gets nothing cannot tell "no entity is in that box"
/// from "no entity carried a box to compare", and those call for opposite
/// next moves. The rule itself is pinned on `passes_entity_filter`
/// (`dispatch::q6_filter_tests`); what this pins is that the count reaches
/// the wire and that an UNFILTERED listing never reports one.
#[test]
fn an_unfiltered_listing_excludes_nothing_and_says_so() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = block(&mut state, &mut kernel, 0.030, 0.020, 0.010, 1);

    for kind in ["face", "edge", "vertex"] {
        let answer = listing(&mut state, &mut kernel, &body, kind);
        assert_eq!(
            answer["excluded_unevaluable"],
            json!(0),
            "nothing is filtered, so nothing is excluded: {answer}"
        );
        assert_eq!(
            answer["unresolved_names"],
            json!([]),
            "this body has no names at all, let alone broken ones: {answer}"
        );
        for e in entities(&answer) {
            assert!(
                e.get("name_warnings").is_none(),
                "an unnamed entity has nothing to warn about: {e}"
            );
        }
    }
}

/// A name that resolves by its persistent id — the normal case — carries NO
/// warnings, so a listing that does carry one is saying something.
///
/// The warning is N1's loud fallback: the pid the name was stored over is
/// gone and the name was rebound through the reference it was authored with,
/// which matches by geometry and may be naming a different entity. `names_list`
/// reports that (`tool_names.rs`); a listing that printed the bare name beside
/// it would be the one place it vanished, which is why `ListedEntity` carries
/// `name_warnings` at all.
#[test]
fn a_pid_resolved_name_comes_with_no_warnings() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = block(&mut state, &mut kernel, 0.030, 0.020, 0.010, 1);
    let edges = entities(&listing(&mut state, &mut kernel, &body, "edge"));
    let target = edges[0].clone();
    let _ = ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({
            "target": { "type": "entity", "geom_ref": target["geom_ref"] },
            "name": "front_edge",
        }),
    );

    let answer = listing(&mut state, &mut kernel, &body, "edge");
    let named = entities(&answer)
        .into_iter()
        .find(|e| e["name"] == json!("front_edge"))
        .expect("the name is in the listing");
    assert!(
        named.get("name_warnings").is_none(),
        "a pid-resolved name is silent: {named}"
    );
    assert_eq!(
        answer["unresolved_names"],
        json!([]),
        "and it resolves, so it is not in the broken list: {answer}"
    );
    // The name's own listing agrees about how it got there, so the two tools
    // cannot tell an agent different stories about one name.
    let entry = ok(&mut state, &mut kernel, "names_list", json!({}))["names"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|n| n["name"] == json!("front_edge"))
        .expect("names_list has it too");
    assert_eq!(entry["resolved_by"], json!("pid"), "{entry}");
}

/// A bad `kind`, a missing `kind` and an unknown body each refuse with their
/// own code rather than answering with an empty list.
#[test]
fn the_arguments_are_checked_and_each_refusal_has_its_own_code() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = block(&mut state, &mut kernel, 0.010, 0.010, 0.010, 1);

    for (args, code) in [
        (
            json!({ "body_id": body, "kind": "shell" }),
            "InvalidArguments",
        ),
        (json!({ "body_id": body }), "InvalidArguments"),
        (
            json!({ "body_id": body, "kind": "edge", "filter": { "bbox": "everything" } }),
            "InvalidArguments",
        ),
        (
            json!({ "body_id": "no-such-body", "kind": "edge" }),
            "BodyNotFound",
        ),
    ] {
        let r = call(&mut state, &mut kernel, "entity_list", args.clone());
        assert!(r.is_error, "{args} should have been refused: {r:?}");
        assert_eq!(
            r.structured_content["error"]["code"],
            json!(code),
            "{args}: {}",
            r.structured_content
        );
    }
}

// -------------------------------------------------------------------------
// Determinism: the same listing, byte for byte, in a fresh process
// -------------------------------------------------------------------------

/// A pocketed plate as a saved document — a BOOLEAN output, so the ids in its
/// listing are the content-seeded ones rather than a constructor's.
///
/// The DOCUMENT is what travels, not the fixture code, because a feature's
/// uuid is part of its geometry's identity (D0 item 1 stamps faces from it)
/// and a freshly authored document mints new uuids every time. Two rebuilds
/// of ONE document is also exactly the scenario the oracle is about: reopen
/// the file and get the same listing.
fn pocketed_plate_document() -> String {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = block(&mut state, &mut kernel, 0.030, 0.020, 0.010, 1);

    let (base, side) = (201u32, 0.008);
    let corners = [
        (base, 0.005, 0.005),
        (base + 1, 0.005 + side, 0.005),
        (base + 2, 0.005 + side, 0.005 + side),
        (base + 3, 0.005, 0.005 + side),
    ];
    let solved_positions: HashMap<u32, (f64, f64)> =
        corners.iter().map(|&(id, x, y)| (id, (x, y))).collect();
    let mut sketch_entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    let (l0, l1, l2, l3) = (base + 10, base + 11, base + 12, base + 13);
    sketch_entities.extend([
        line(l0, base, base + 1),
        line(l1, base + 1, base + 2),
        line(l2, base + 2, base + 3),
        line(l3, base + 3, base),
    ]);
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane_face: None,
        plane: datum_xy(),
        plane_origin: [0.0, 0.0, 0.010],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: sketch_entities,
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
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Sketch { sketch },
            provenance: None,
        },
        &mut kernel,
    ));
    let cut = added_id(dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch_feature,
                    profile_index: 0,
                    profile_entity_ids: Some(vec![l0, l1, l2, l3]),
                    depth: 0.004,
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
        &mut kernel,
    ));
    // The cut really did consume the plate into one pocketed body.
    assert_ne!(
        FeatureTree::body_id(cut, &OutputKey::Main),
        body,
        "the pocket must produce a boolean output body"
    );
    match dispatch(&mut state, UiToEngine::SaveProject, &mut kernel) {
        EngineToUi::SaveReady { json_data } => json_data,
        other => panic!("expected SaveReady, got {other:?}"),
    }
}

/// `entity_list` over `kind` for the last body of `document`, rebuilt from
/// scratch in a fresh engine and a fresh arena, as JSON.
fn listing_of(document: &str, kind: &str) -> String {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    match dispatch(
        &mut state,
        UiToEngine::LoadProject {
            data: document.to_string(),
        },
        &mut kernel,
    ) {
        EngineToUi::ModelUpdated { errors, .. } => {
            assert!(errors.is_empty(), "the reload rebuilds: {errors:?}")
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(&mut state, &mut kernel);

    let body = ok(&mut state, &mut kernel, "model_summary", json!({}))["bodies"]
        .as_array()
        .and_then(|b| b.last().cloned())
        .map(|b| b["body_id"].as_str().unwrap_or_default().to_string())
        .expect("a rendered body");
    let out = ok(
        &mut state,
        &mut kernel,
        "entity_list",
        json!({ "body_id": body, "kind": kind }),
    );
    serde_json::to_string(&out).expect("serializable")
}

const DUMP_ENV: &str = "Q6_DUMP_LISTING_DOC";
const DUMP_TEST: &str = "prints_the_pocketed_plate_listing_for_its_own_child";

/// The child half of [`a_fresh_process_lists_the_same_entities_byte_for_byte`]:
/// it rebuilds the document the parent hands it through the environment and
/// prints the listing. Inert without that, so a plain run of this suite still
/// asserts something.
#[test]
fn prints_the_pocketed_plate_listing_for_its_own_child() {
    match std::env::var(DUMP_ENV) {
        Ok(document) => println!("LISTING {}", listing_of(&document, "edge")),
        Err(_) => {
            let listing = listing_of(&pocketed_plate_document(), "edge");
            assert!(listing.contains("arc_length_m"), "the listing has lengths");
        }
    }
}

/// Two rebuilds in ONE process share every per-process thing there is — an
/// allocator, a `HashMap` seed, a clock — so they cannot show that the ids and
/// the order do not depend on one. This re-executes the test binary, hands the
/// child the same document, and compares its listing to the parent's byte for
/// byte.
#[test]
fn a_fresh_process_lists_the_same_entities_byte_for_byte() {
    let document = pocketed_plate_document();
    let mine = listing_of(&document, "edge");
    // Twice in this process first: if a from-scratch rebuild drifts at all,
    // the cheaper failure is the informative one.
    assert_eq!(mine, listing_of(&document, "edge"), "same process");
    // And the listing is worth comparing: a pocketed plate has boolean-output
    // edges, every one of them with a persistent id.
    assert!(
        mine.contains(r#""type":"Pid""#) && mine.contains("arc_length_m"),
        "the listing carries persistent ids and arc lengths: {mine}"
    );

    let exe = std::env::current_exe().expect("this test binary's path");
    let out = std::process::Command::new(exe)
        .args(["--exact", DUMP_TEST, "--nocapture"])
        .env(DUMP_ENV, &document)
        .output()
        .expect("re-run this test binary as a child process");
    assert!(
        out.status.success(),
        "the child run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let theirs = stdout
        .lines()
        .find_map(|l| l.strip_prefix("LISTING "))
        .unwrap_or_else(|| panic!("the child printed no listing:\n{stdout}"));
    assert_eq!(theirs, mine, "a fresh process must list the same entities");
}
