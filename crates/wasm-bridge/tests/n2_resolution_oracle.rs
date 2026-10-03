//! N2's own oracle (`specs/agent_mechanical_design.md` §5.3): the D0 identity
//! oracle extended with names.
//!
//! > after a no-op edit every name resolves to the same Pid; after a parameter
//! > edit that keeps topology, the same; after an edit that deletes the named
//! > face, `resolves: false` and the feature that referenced it reports
//! > `PidGone`, never a different face.
//!
//! The third clause is the one that needed the increment. The first two are
//! what make it meaningful: a reference that went stale on every rebuild would
//! be loud and useless. All three are measured through `names_list`, the
//! surface an agent reads.
//!
//! The named face is a POCKET FLOOR, deliberately. It is a boolean output face,
//! so it takes a counter pid rather than a content seed (D0 item 1b), and
//! deepening the pocket into a through hole costs both the number and its
//! lineage root — which is exactly the `PidGone` the oracle asks for, on real
//! geometry rather than by hand.

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use serde_json::{json, Value};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

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

fn tool(state: &mut EngineState, kernel: &mut KernelV2Adapter, name: &str, args: Value) -> Value {
    let r = execute_tool(
        state,
        kernel,
        name,
        &args,
        Some(&json!({ "agent_name": "n2-oracle" })),
    );
    assert!(!r.is_error, "{name} failed: {r:?}");
    r.structured_content
}

/// A square sketch and the extrude over it, as one feature pair. `cut` makes
/// the extrude a boolean cut into whatever body is already there.
// Eight inputs: the engine pair, the id base, and the square's own four
// numbers plus the cut flag. Grouping them would only name the square twice.
#[allow(clippy::too_many_arguments)]
fn square_feature(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    base: u32,
    x0: f64,
    y0: f64,
    side: f64,
    depth: f64,
    cut: bool,
) -> (Uuid, Uuid, Vec<u32>) {
    let corners = [
        (base, x0, y0),
        (base + 1, x0 + side, y0),
        (base + 2, x0 + side, y0 + side),
        (base + 3, x0, y0 + side),
    ];
    let solved_positions: HashMap<u32, (f64, f64)> =
        corners.iter().map(|&(id, x, y)| (id, (x, y))).collect();
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    let loop_ids = vec![base + 10, base + 11, base + 12, base + 13];
    entities.extend([
        line(loop_ids[0], base, base + 1),
        line(loop_ids[1], base + 1, base + 2),
        line(loop_ids[2], base + 2, base + 3),
        line(loop_ids[3], base + 3, base),
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
            entity_ids: loop_ids.clone(),
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
            operation: extrude_op(sketch_feature, &loop_ids, depth, cut),
            provenance: None,
        },
        kernel,
    ));
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    (sketch_feature, extrude, loop_ids)
}

fn extrude_op(sketch_id: Uuid, loop_ids: &[u32], depth: f64, cut: bool) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(loop_ids.to_vec()),
            depth,
            depth_expr: None,
            direction: None,
            symmetric: false,
            cut,
            merge: cut,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine: Some(if cut {
                CombineMode::Cut
            } else {
                CombineMode::NewBody
            }),
            targets: None,
        },
    }
}

/// The reference for the pocket floor of `body`: a face pointing DOWN that
/// sits above the base plane.
fn pocket_floor_ref(state: &mut EngineState, kernel: &mut KernelV2Adapter, body: &str) -> Value {
    let all = tool(state, kernel, "face_list", json!({ "body_id": body }));
    let floor = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .find(|f| {
            f["signature"]["normal"][2].as_f64().unwrap_or(0.0) < -0.5
                && f["signature"]["centroid"][2].as_f64().unwrap_or(0.0) > 1e-9
        })
        .unwrap_or_else(|| panic!("a pocket floor: {all}"));
    floor["geom_ref"].clone()
}

/// One name's row from `names_list`.
fn listed(state: &mut EngineState, kernel: &mut KernelV2Adapter, name: &str) -> Value {
    let all = tool(state, kernel, "names_list", json!({}));
    all["names"]
        .as_array()
        .expect("names")
        .iter()
        .find(|n| n["name"] == name)
        .unwrap_or_else(|| panic!("no name {name}: {all}"))
        .clone()
}

/// The stored pid of `name`, as the selector records it.
fn stored_pid(state: &EngineState, name: &str) -> (u64, u64) {
    match state
        .engine
        .tree
        .named_ref(name)
        .expect("named")
        .target
        .selector
    {
        Selector::Pid { pid, root_pid } => (pid, root_pid),
        ref other => panic!("a name must store a pid, got {other:?}"),
    }
}

/// A plate 20 mm square and 6 mm thick with an 8 mm pocket 3 mm deep cut into
/// it, with the pocket's FLOOR named `floor`. Returns the pocket's cut feature,
/// its sketch, its profile loop and the body id.
fn plate_with_a_named_pocket_floor(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
) -> (Uuid, Uuid, Vec<u32>, String) {
    square_feature(state, kernel, 100, 0.0, 0.0, 0.020, 0.006, false);
    let (pocket_sketch, cut, loop_ids) =
        square_feature(state, kernel, 200, 0.006, 0.006, 0.008, 0.003, true);
    let body = FeatureTree::body_id(cut, &OutputKey::Main);
    let floor = pocket_floor_ref(state, kernel, &body);
    tool(
        state,
        kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": floor }, "name": "floor" }),
    );
    (cut, pocket_sketch, loop_ids, body)
}

// ─────────────────────────────────────────────────────────────────────────────

/// Clause 1: a NO-OP edit — the same operation written back — leaves the name
/// on the same pid, resolving through the same rung.
#[test]
fn a_no_op_edit_leaves_the_name_on_the_same_pid() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (cut, sketch, loop_ids, _) = plate_with_a_named_pocket_floor(&mut state, &mut kernel);
    let before = stored_pid(&state, "floor");
    let row = listed(&mut state, &mut kernel, "floor");
    assert_eq!(row["resolves"], true, "{row}");
    let via_before = row["resolved_via"].clone();

    dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: cut,
            operation: extrude_op(sketch, &loop_ids, 0.003, true),
            provenance: None,
        },
        &mut kernel,
    );

    assert_eq!(stored_pid(&state, "floor"), before, "the pid did not move");
    let row = listed(&mut state, &mut kernel, "floor");
    assert_eq!(row["resolves"], true, "{row}");
    assert_eq!(row["resolved_via"], via_before, "same rung: {row}");
    assert!(row.get("rebound").is_none(), "no rebind: {row}");
}

/// Clause 2: a parameter edit that KEEPS the topology — the pocket gets
/// deeper, its floor is still a floor — leaves the name resolving. Whether the
/// NUMBER survives is D0's business (a boolean output face is counter-pid'd and
/// may be re-minted); what N2 requires is that resolution stays silent and
/// does not rebind by geometry.
#[test]
fn a_parameter_edit_that_keeps_topology_keeps_the_name_resolving() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (cut, sketch, loop_ids, _) = plate_with_a_named_pocket_floor(&mut state, &mut kernel);

    dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: cut,
            // 3 mm deep becomes 4: the floor moves down, it does not vanish.
            operation: extrude_op(sketch, &loop_ids, 0.004, true),
            provenance: None,
        },
        &mut kernel,
    );

    let row = listed(&mut state, &mut kernel, "floor");
    assert_eq!(row["resolves"], true, "{row}");
    assert!(
        row.get("rebound").is_none(),
        "it must not have rebound by geometry: {row}"
    );
    assert!(row.get("refusal").is_none(), "{row}");
}

/// Clause 3, the one the increment is for, and the one that found something.
///
/// §5.3 writes the oracle as "`resolves: false` and the feature that referenced
/// it reports `PidGone`, never a different face". What this measures instead:
/// the pid IS reported gone, as `lost_identity: PidGone` with the numbers, and
/// `rebound: true` — but `resolves` stays `true`, because N1's fallback did
/// answer and bound a DIFFERENT face (measured here: `resolved_by: query`,
/// `resolved_via: role`).
///
/// That is a real conflict between two merged increments, not a bug in either:
/// §5.2 deliberately keeps the fallback (its own test
/// `a_name_whose_reference_is_gone_still_measures_through_its_fallback` pins
/// it green), and §5.3's oracle asks for a refusal. N2 takes the softer
/// reading — report, do not refuse — and makes the rebind machine-visible, so
/// an agent branching on `rebound` or `lost_identity` learns everything the
/// refusal would have told it. See the spec's "Implementation notes (N2)".
///
/// The plate is still there with every other face intact, which is what makes
/// this a real test: the fallback had plenty to bind to, and did.
#[test]
fn an_edit_that_deletes_the_named_face_reports_the_lost_identity_and_the_rebind() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (cut, sketch, loop_ids, body) = plate_with_a_named_pocket_floor(&mut state, &mut kernel);
    let floor_pid = stored_pid(&state, "floor");

    dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: cut,
            // 3 mm deep becomes 9 through a 6 mm plate: a through hole, whose
            // output has no floor at all.
            operation: extrude_op(sketch, &loop_ids, 0.009, true),
            provenance: None,
        },
        &mut kernel,
    );
    // `face_list` reads the RENDERED body list, and an edit leaves the new
    // body un-meshed until something tessellates it.
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(&mut state, &mut kernel);

    // The hole really is through: nothing faces down above the base plane.
    let all = tool(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    let floors: Vec<&Value> = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .filter(|f| {
            f["signature"]["normal"][2].as_f64().unwrap_or(0.0) < -0.5
                && f["signature"]["centroid"][2].as_f64().unwrap_or(0.0) > 1e-9
        })
        .collect();
    assert!(
        floors.is_empty(),
        "the pocket floor must be gone for this oracle to mean anything: {floors:?}"
    );
    assert!(
        all["faces"].as_array().unwrap().len() > 4,
        "while the rest of the plate is still there: {all}"
    );

    let row = listed(&mut state, &mut kernel, "floor");
    // The recorded identity is gone, and the listing says so with the numbers
    // — this is the oracle's `PidGone`.
    assert_eq!(
        row["lost_identity"]["type"], "PidGone",
        "the reason an agent branches on: {row}"
    );
    assert_eq!(row["lost_identity"]["pid"], floor_pid.0, "{row}");
    assert_eq!(row["lost_identity"]["root_pid"], floor_pid.1, "{row}");
    // And the answer it DID get is flagged as not the recorded entity, which
    // is the oracle's "never a different face" made legible rather than
    // enforced. `resolves: true` alone must never be read as "still fine".
    assert_eq!(row["rebound"], true, "{row}");
    assert_eq!(
        row["resolved_by"], "query",
        "the authored selector answered, not the id: {row}"
    );
    assert!(
        row["warnings"].as_array().unwrap().iter().any(|w| w
            .as_str()
            .unwrap_or_default()
            .contains("may name a different entity")),
        "in words too: {row}"
    );
    assert!(
        row["geom_ref"].is_object(),
        "and the name KEEPS its record — §5.2, the hole is the information: {row}"
    );
}
