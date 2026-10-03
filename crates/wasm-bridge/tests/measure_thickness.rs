//! Q5 of `specs/agent_mechanical_design.md` §4.2/§4.3 end to end: the
//! `measure_thickness` tool over the real kernel.
//!
//! The kernel suite owns the geometry (`crates/kernel-v2/tests/q5_thickness.rs`
//! — a plate's exact wall, a tube's `r_out − r_in`, the acute-corner
//! correction, the rigid-motion invariance). What this file pins is the layer a
//! host crosses, and the three things only it can get wrong:
//!
//! 1. The thinnest site's two faces must arrive as something an agent can keep
//!    — a persistent id as a decimal STRING, plus the N1 name — and not as a
//!    transient kernel id alone.
//! 2. `method` must be `sampled` and nothing else, with `spacing_m` and
//!    `samples` beside it, so no consumer can read a sampled wall as exact.
//! 3. `spacing_m` must reach the kernel, and a bad one must be refused where
//!    the caller can fix it.

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

/// A `side` × `side` × `depth` plate with its near corner at the sketch
/// origin. Returns its body id.
fn plate(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    side: f64,
    depth: f64,
    base: u32,
) -> String {
    quad(
        state,
        kernel,
        [(0.0, 0.0), (side, 0.0), (side, side), (0.0, side)],
        depth,
        base,
    )
}

/// A prism over the sketch QUADRILATERAL `corners`, `depth` tall. Returns its
/// body id.
///
/// The plate is the rectangular case; a trapezoid is how an ACUTE corner
/// reaches this boundary, which is what the two minima are for.
fn quad(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    corners: [(f64, f64); 4],
    depth: f64,
    base: u32,
) -> String {
    let corners = [
        (base, corners[0].0, corners[0].1),
        (base + 1, corners[1].0, corners[1].1),
        (base + 2, corners[2].0, corners[2].1),
        (base + 3, corners[3].0, corners[3].1),
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
    FeatureTree::body_id(extrude, &OutputKey::Main)
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
        Some(&json!({ "agent_name": "q5-test" })),
    )
}

#[track_caller]
fn ok(state: &mut EngineState, kernel: &mut KernelV2Adapter, tool: &str, args: Value) -> Value {
    let r = call(state, kernel, tool, args);
    assert!(!r.is_error, "{tool} failed: {r:?}");
    r.structured_content
}

#[track_caller]
fn refusal(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    tool: &str,
    args: Value,
) -> (String, String) {
    let r = call(state, kernel, tool, args);
    assert!(r.is_error, "{tool} should have been refused: {r:?}");
    let error = r.structured_content["error"].clone();
    (
        error["code"].as_str().unwrap_or_default().to_string(),
        error["message"].as_str().unwrap_or_default().to_string(),
    )
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

// ---------------------------------------------------------------------------

/// A 40 × 40 × 5 mm plate: the wall is 5 mm, the tier is `sampled`, and the
/// thinnest site names its two faces by persistent id — as STRINGS.
#[test]
fn a_plates_wall_crosses_the_wire_with_its_faces_named_by_persistent_id() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = plate(&mut state, &mut kernel, 0.04, 0.005, 1);
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": body }),
    );

    assert_eq!(out["body_id"], json!(body));
    assert_eq!(f(&out["min_m"]), 0.005, "the plate's wall, exactly: {out}");
    assert_eq!(
        out["method"], "sampled",
        "a sampled wall has exactly one tier: {out}"
    );
    assert!(f(&out["spacing_m"]) > 0.0, "{out}");
    assert!(
        out["samples"].as_u64().unwrap_or(0) > 100,
        "a plate is sampled densely: {out}"
    );
    assert!(f(&out["max_m"]) >= f(&out["min_m"]));
    assert!(
        f(&out["mean_m"]) >= f(&out["min_m"]) && f(&out["mean_m"]) <= f(&out["max_m"]),
        "{out}"
    );

    // The thinnest site spans two DIFFERENT faces, each with a persistent id
    // that is a decimal string — never a JSON number, which would round an id
    // above 2^53 onto another entity.
    let (from, to) = (&out["thinnest"]["from"], &out["thinnest"]["to"]);
    for side in [from, to] {
        let pid = side["pid"].as_str().unwrap_or_else(|| {
            panic!("a face's pid must be a decimal STRING, got {side}");
        });
        assert!(
            pid.parse::<u64>().is_ok(),
            "the pid parses as a u64: {pid} in {side}"
        );
        assert!(
            side["root_pid"].is_string(),
            "the lineage root is a string too: {side}"
        );
        assert!(side["kernel_id"].is_u64(), "{side}");
        assert!(
            side["name"].is_null(),
            "an unnamed face carries no name: {side}"
        );
    }
    assert_ne!(from["pid"], to["pid"], "a wall is between two faces: {out}");
    assert_eq!(
        f(&out["thinnest"]["thickness_m"]),
        f(&out["min_m"]),
        "the thinnest site IS the minimum: {out}"
    );
    // A right-angled plate has no corner reading, so the WALL minimum is the
    // same number at the same site — and the flag says so on both.
    assert_eq!(
        out["thinnest"]["faces_share_an_edge"],
        json!(false),
        "{out}"
    );
    assert_eq!(f(&out["min_wall_m"]), 0.005, "the wall is the plate: {out}");
    assert_eq!(
        out["thinnest_wall"], out["thinnest"],
        "with no corner to leave out, the two sites are one: {out}"
    );
    // Its two ends are 5 mm apart, on the two plate planes.
    let p = out["thinnest"]["point"].as_array().expect("point");
    let q = out["thinnest"]["opposite"].as_array().expect("opposite");
    let d = (0..3)
        .map(|k| (f(&p[k]) - f(&q[k])).powi(2))
        .sum::<f64>()
        .sqrt();
    assert!(
        (d - 0.005).abs() < 1e-12,
        "the site spans its own wall: {out}"
    );

    // The histogram accounts for every site.
    let total: u64 = out["histogram"]
        .as_array()
        .expect("histogram")
        .iter()
        .map(|b| b["count"].as_u64().unwrap_or(0))
        .sum();
    assert_eq!(total, out["samples"].as_u64().unwrap_or(0), "{out}");
}

/// A WEDGE crosses the wire with two different minima: `min_m` is its acute
/// corner and `min_wall_m` is its thin end's wall, each with the site it was
/// measured at and the flag that says which kind it is.
///
/// This is the pair a wall-thickness decision rests on. The plate above cannot
/// show it — a right-angled body has no corner reading, so both minima are the
/// same number there — and a tool that published only `min_m` would hand an
/// agent the corner for every tapered part it ever sees.
#[test]
fn a_wedge_reports_the_corner_and_the_wall_as_two_numbers() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (l, t0, t1, w) = (0.040, 0.006, 0.002, 0.020);
    let body = quad(
        &mut state,
        &mut kernel,
        [(0.0, 0.0), (l, 0.0), (l, t1), (0.0, t0)],
        w,
        1,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": body }),
    );

    let slope = (t0 - t1) / l;
    let cos_a = 1.0 / (1.0 + slope * slope).sqrt();
    let (min, wall) = (f(&out["min_m"]), f(&out["min_wall_m"]));
    // The corner is below every wall this body has; the wall is the thin end's
    // own, between the base and the slant — two faces that do not meet.
    assert!(
        min < t1 * cos_a,
        "min_m is the acute corner, under the thin end's wall {:e}: {out}",
        t1 * cos_a
    );
    // The wall is the thin end's own, measured at whichever site the
    // subdivision put nearest the end: at worst one spacing short of it, where
    // the taper is `slope` thicker, and at best the perpendicular `t1·cos α` a
    // cast from the slant itself would give. Both bounds are closed forms, and
    // the band between them is the REPORTED spacing — not one chosen to pass.
    let spacing = f(&out["spacing_m"]);
    assert!(
        wall >= t1 * cos_a * (1.0 - 1e-9) && wall <= t1 + slope * spacing,
        "min_wall_m is the thin end's wall, between {:e} and {:e} at spacing {:e}: {out}",
        t1 * cos_a,
        t1 + slope * spacing,
        spacing
    );
    assert!(wall > min, "the wall is thicker than the corner: {out}");
    // And each number says which kind of site it came from.
    assert_eq!(
        out["thinnest"]["faces_share_an_edge"],
        json!(true),
        "the overall minimum crossed a corner: {out}"
    );
    assert_eq!(
        out["thinnest_wall"]["faces_share_an_edge"],
        json!(false),
        "the wall minimum did not: {out}"
    );
    assert_eq!(
        f(&out["thinnest_wall"]["thickness_m"]),
        wall,
        "thinnest_wall IS min_wall_m: {out}"
    );
    // The wall site's two faces are named the same way the thinnest site's
    // are — persistent ids as decimal STRINGS, not JSON numbers.
    for side in [&out["thinnest_wall"]["from"], &out["thinnest_wall"]["to"]] {
        assert!(
            side["pid"]
                .as_str()
                .is_some_and(|p| p.parse::<u64>().is_ok()),
            "a wall face's pid is a decimal string: {side}"
        );
    }
}

/// An N1 name on one of the thinnest site's faces comes back ON it — so an
/// agent that named the face it cares about reads the answer in its own terms.
#[test]
fn a_named_face_is_named_in_the_thinnest_site() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = plate(&mut state, &mut kernel, 0.04, 0.005, 1);

    // Name both of the plate's large faces, so whichever the thinnest site
    // starts from is named.
    let faces = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    let mut named = 0;
    for face in faces["faces"].as_array().expect("faces").clone() {
        let n = &face["signature"]["normal"];
        if n[2].as_f64().unwrap_or(0.0).abs() > 0.5 {
            let label = if n[2].as_f64().unwrap_or(0.0) > 0.0 {
                "top"
            } else {
                "bottom"
            };
            ok(
                &mut state,
                &mut kernel,
                "entity_name",
                json!({
                    "target": { "type": "entity", "geom_ref": face["geom_ref"] },
                    "name": label,
                }),
            );
            named += 1;
        }
    }
    assert_eq!(named, 2, "a plate has two faces normal to z: {faces}");

    let out = ok(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": body }),
    );
    let names: Vec<&str> = ["from", "to"]
        .iter()
        .filter_map(|side| out["thinnest"][*side]["name"].as_str())
        .collect();
    assert_eq!(
        names.len(),
        2,
        "both ends of the plate's wall are named faces: {out}"
    );
    assert!(
        names.contains(&"top") && names.contains(&"bottom"),
        "the names are the ones that were assigned: {names:?} in {out}"
    );
}

/// `spacing_m` reaches the kernel: a denser request is more samples and a
/// smaller reported spacing, and it does not move a flat wall.
#[test]
fn a_requested_spacing_reaches_the_kernel_and_is_reported() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = plate(&mut state, &mut kernel, 0.04, 0.005, 1);
    let coarse = ok(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": body, "spacing_m": 0.008 }),
    );
    let fine = ok(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": body, "spacing_m": 0.001 }),
    );
    assert!(
        fine["samples"].as_u64().unwrap_or(0) > coarse["samples"].as_u64().unwrap_or(0),
        "a denser spacing is more samples: {} vs {}",
        fine["samples"],
        coarse["samples"]
    );
    assert!(
        f(&fine["spacing_m"]) < f(&coarse["spacing_m"]),
        "the reported spacing follows the request: {fine} vs {coarse}"
    );
    assert_eq!(
        f(&fine["min_m"]),
        0.005,
        "a flat wall does not move: {fine}"
    );
    assert_eq!(f(&coarse["min_m"]), 0.005, "{coarse}");
}

/// A degenerate `spacing_m` and an unknown body are both refused where the
/// caller can fix them — typed, and naming what is wrong.
#[test]
fn a_bad_request_is_refused_at_the_boundary() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = plate(&mut state, &mut kernel, 0.04, 0.005, 1);

    let (code, message) = refusal(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": body, "spacing_m": 0.0 }),
    );
    assert_eq!(code, "InvalidArguments", "{message}");
    assert!(
        message.contains("spacing"),
        "the refusal names the argument: {message}"
    );

    let (code, _) = refusal(
        &mut state,
        &mut kernel,
        "measure_thickness",
        json!({ "body_id": "no-such-body" }),
    );
    assert_eq!(code, "BodyNotFound");
}

/// MockKernel refuses the measurement, typed — the tool must pass a capability
/// wall through as one rather than as a bad request, so a host wired to a
/// kernel that cannot measure sees `NotImplemented` and stops asking.
#[test]
fn a_kernel_that_cannot_measure_is_a_capability_wall() {
    use waffle_types::kernel::{KernelError, KernelMeasure, ThicknessOpts};
    let kernel = waffle_types::kernel::MockKernel::new();
    let handle = waffle_types::kernel::KernelSolidHandle::from_raw(1);
    let err = kernel
        .thickness(&handle, &ThicknessOpts::default())
        .expect_err("the mock has no tessellation to sample");
    match err {
        KernelError::NotSupported { operation } => assert!(
            operation.contains("MockKernel") && operation.contains("thickness"),
            "the refusal names itself: {operation}"
        ),
        other => panic!("want a typed capability refusal, got {other:?}"),
    }
}
