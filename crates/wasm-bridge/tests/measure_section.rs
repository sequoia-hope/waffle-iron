//! Q4 of `specs/agent_mechanical_design.md` §4.2/§4.3 end to end: the
//! `measure_section` tool over the real kernel.
//!
//! The kernel suite owns the cutting geometry
//! (`crates/kernel-v2/src/projection/section/tests.rs`, D1d). What this file
//! pins is the layer a host crosses — tool args → `UiToEngine` →
//! `KernelProjection::section_with_plane` → the wire shape — and the four
//! things that layer can get wrong on its own:
//!
//! 1. The analytic arms must survive serialization. A cap bounded by a circle
//!    must arrive as a circle, not as the 71 chords a flattened polyline would
//!    be, because the area and the drawing both change if it does not.
//! 2. A plane that MISSES must be a typed empty section, distinguishable from
//!    a body the kernel refused — and from which SIDE it missed on.
//! 3. The cap frame must be the kernel's own, carried, not re-derived.
//! 4. A plane named by a face (an N1 name) must cut the same plane the face
//!    lies in, with the face's own normal.

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

/// A rectangular prism from the sketch rectangle `[x0, y0] × [x1, y1]`,
/// `depth` tall, as `combine` says. Returns its body id.
#[allow(clippy::too_many_arguments)]
fn prism(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    (x0, y0, x1, y1): (f64, f64, f64, f64),
    depth: f64,
    base: u32,
    combine: CombineMode,
    symmetric: bool,
) -> String {
    let corners = [
        (base, x0, y0),
        (base + 1, x1, y0),
        (base + 2, x1, y1),
        (base + 3, x0, y1),
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
    let cut = matches!(combine, CombineMode::Cut);
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
                    symmetric,
                    cut,
                    merge: cut,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    combine: Some(combine),
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

/// A cylinder of radius `r` and height `h` centred at `(cx, cy)` in the sketch
/// frame, as `combine` says.
#[allow(clippy::too_many_arguments)]
fn cylinder(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    (cx, cy): (f64, f64),
    r: f64,
    h: f64,
    base: u32,
    combine: CombineMode,
) -> String {
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: datum_xy(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: Vec::new(),
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        projected: Vec::new(),
        plane_face: None,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![base],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(CircleProfile {
                center_u: cx,
                center_v: cy,
                radius: r,
            }),
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
    let cut = matches!(combine, CombineMode::Cut);
    let extrude = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch_feature,
                    profile_index: 0,
                    profile_entity_ids: Some(vec![base]),
                    depth: h,
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
                    combine: Some(combine),
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
        Some(&json!({ "agent_name": "q4-test" })),
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

#[track_caller]
fn one_body(out: &Value) -> Value {
    let bodies = out["bodies"].as_array().expect("bodies");
    assert_eq!(bodies.len(), 1, "one sectioned body: {out}");
    bodies[0].clone()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

// ---------------------------------------------------------------------------

/// A 40 × 40 × 10 mm plate cut at mid height: ONE outer loop of four exact
/// lines, the area exactly its cross-section, and the centroid at the plate's
/// own centre.
///
/// The area is an equality, not a band: the cap is integrated in closed form
/// from exact lines, and the tool must not round it on the way out.
#[test]
fn a_plate_cut_at_mid_height_reports_its_exact_cross_section() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, 0.005], "normal": [0.0, 0.0, 1.0] },
        }),
    );
    let b = one_body(&out);
    assert_eq!(b["method"], "exact", "{out}");
    assert_eq!(b["kept_material"], true);
    assert_eq!(b["cap_shared_with_model"], false);
    assert_eq!(b["centroid_exact"], true, "a prismatic cap is all lines");
    let loops = b["loops"].as_array().expect("loops");
    assert_eq!(loops.len(), 1, "a convex cap is one loop: {b}");
    assert_eq!(loops[0]["kind"], "outer");
    assert_eq!(loops[0]["exact"], true);
    let curves = loops[0]["curves"].as_array().expect("curves");
    assert_eq!(curves.len(), 4, "four walls, four cap edges: {b}");
    assert!(
        curves.iter().all(|c| c["type"] == "line"),
        "a prismatic cap arrives as exact lines: {curves:?}"
    );
    // 40 mm × 40 mm = 1.6e-3 m², exactly.
    assert_eq!(f(&b["area_m2"]), 1.6e-3, "{b}");
    assert_eq!(f(&loops[0]["signed_area_m2"]), 1.6e-3);
    assert_eq!(f(&out["total_area_m2"]), 1.6e-3);

    // The cap's centroid is the plate's own centre, at the cut height. The
    // sketch frame maps sketch `(x, y)` to world `(x, −y)`, so the 40 mm
    // square spans `x ∈ [0, 0.04]`, `y ∈ [−0.04, 0]`. (NOT from
    // `body_measure`'s bbox: that comes from the render mesh and is f32, which
    // puts 0.04 at 0.03999999955 — a 4.5e-10 m error in an assertion about an
    // exactly integrated centroid.)
    let centre = [0.02, -0.02, 0.005];
    let c = b["centroid"].as_array().expect("centroid");
    for k in 0..3 {
        assert!(
            (f(&c[k]) - centre[k]).abs() < 1e-12,
            "centroid[{k}] = {} wants {}: {b}",
            f(&c[k]),
            centre[k]
        );
    }

    // The frame is the kernel's own, and it is orthonormal and right-handed:
    // a consumer maps `(u, v)` back to the world through exactly this.
    let basis = &out["basis"];
    let axis = |name: &str| {
        let a = basis[name].as_array().expect(name);
        [f(&a[0]), f(&a[1]), f(&a[2])]
    };
    let (u, v, w) = (axis("u_axis"), axis("v_axis"), axis("w_axis"));
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    for (name, a) in [("u", u), ("v", v), ("w", w)] {
        assert!(
            (dot(a, a) - 1.0).abs() < 1e-15,
            "{name} is not unit: {basis}"
        );
    }
    assert!(dot(u, v).abs() < 1e-15, "u·v: {basis}");
    assert!(dot(u, w).abs() < 1e-15, "u·w: {basis}");
    // The line of sight is the NEGATED plane normal: the viewer stands on the
    // discarded side.
    assert_eq!(w, [0.0, 0.0, -1.0], "{basis}");
    assert_eq!(axis("origin"), [0.0, 0.0, 0.005]);
}

/// A cylinder cut obliquely: the cap is an ELLIPSE on the wire — the analytic
/// arm, with the semi-axes `r` and `r / cos θ` — and its area is `π·a·b`.
///
/// This is the test that would fail if the wire type flattened a conic.
#[test]
fn an_oblique_cylinder_cap_arrives_as_an_ellipse_with_area_pi_a_b() {
    use std::f64::consts::{FRAC_1_SQRT_2, PI};
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    const R: f64 = 0.004;
    let body = cylinder(
        &mut state,
        &mut kernel,
        (0.0, 0.0),
        R,
        0.02,
        1,
        CombineMode::NewBody,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            // 45° about the sketch frame's in-plane axis, through mid height.
            "plane": { "origin": [0.0, 0.0, 0.01], "normal": [0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2] },
        }),
    );
    let b = one_body(&out);
    assert_eq!(
        b["method"], "exact",
        "an oblique cylinder cap stays analytic"
    );
    assert_eq!(
        b["centroid_exact"], false,
        "a conic cap's centroid is flattened, and must say so"
    );
    let loops = b["loops"].as_array().expect("loops");
    assert_eq!(loops.len(), 1);
    let curves = loops[0]["curves"].as_array().expect("curves");
    assert!(
        curves.iter().all(|c| c["type"] == "ellipse"),
        "every cap edge is an ellipse arc: {curves:?}"
    );
    let want_major = R / FRAC_1_SQRT_2;
    let mut sweep = 0.0;
    for c in curves {
        assert!(
            (f(&c["minor_radius"]) - R).abs() < 1e-15,
            "the minor semi-axis IS the radius: {c}"
        );
        assert!(
            (f(&c["major_radius"]) - want_major).abs() < 1e-15,
            "the major semi-axis is r/cos θ = {want_major}: {c}"
        );
        sweep += f(&c["end_param"]) - f(&c["start_param"]);
    }
    assert!(
        (sweep - std::f64::consts::TAU).abs() < 1e-9,
        "the arcs tile one full turn, got {sweep}"
    );
    let want = PI * want_major * R;
    assert!(
        (f(&b["area_m2"]) - want).abs() < 1e-15,
        "π·a·b = {want}, got {}",
        f(&b["area_m2"])
    );
}

/// A bored plate cut through the bore: TWO loops — the outer positive, the
/// bore NEGATIVE and typed `hole` — and the bore arrives as one full circle of
/// its own radius.
#[test]
fn a_bored_plate_reports_an_outer_loop_and_a_typed_hole() {
    use std::f64::consts::PI;
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    const R: f64 = 0.005;
    prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    // Through the plate from above: a blind cut with no direction reverses
    // into the material, so this bores downward from the top face.
    let body = cylinder(
        &mut state,
        &mut kernel,
        (0.02, 0.02),
        R,
        0.012,
        500,
        CombineMode::Cut,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, 0.005], "normal": [0.0, 0.0, 1.0] },
        }),
    );
    let b = one_body(&out);
    let loops = b["loops"].as_array().expect("loops");
    assert_eq!(loops.len(), 2, "an outer loop and the bore: {b}");
    let outer: Vec<&Value> = loops.iter().filter(|l| l["kind"] == "outer").collect();
    let holes: Vec<&Value> = loops.iter().filter(|l| l["kind"] == "hole").collect();
    assert_eq!(outer.len(), 1, "{b}");
    assert_eq!(holes.len(), 1, "{b}");
    assert_eq!(f(&outer[0]["signed_area_m2"]), 1.6e-3);
    assert!(
        (f(&holes[0]["signed_area_m2"]) + PI * R * R).abs() < 1e-15,
        "a hole's signed area is −π·r² = {}, got {}",
        -PI * R * R,
        f(&holes[0]["signed_area_m2"])
    );
    assert!(
        (f(&b["area_m2"]) - (1.6e-3 - PI * R * R)).abs() < 1e-15,
        "the net area is w·d − π·r²: {b}"
    );
    assert_eq!(b["method"], "exact");
    let bore = holes[0]["curves"].as_array().expect("curves");
    assert_eq!(bore.len(), 1, "a bore's cap edge is one curve: {b}");
    assert_eq!(bore[0]["type"], "circle", "{b}");
    assert!((f(&bore[0]["radius"]) - R).abs() < 1e-15, "{b}");
    // One full turn, so a consumer can hatch it without an endpoint check.
    assert!(
        (f(&bore[0]["end_angle_rad"]) - f(&bore[0]["start_angle_rad"]) - std::f64::consts::TAU)
            .abs()
            < 1e-12,
        "{b}"
    );
}

/// A plane that misses the body is an EMPTY section, typed — and which side it
/// missed on is readable, because `kept_material` says whether anything
/// survived. Neither is an error, and neither is a decline.
#[test]
fn a_plane_that_misses_is_a_typed_empty_section_on_both_sides() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );

    let above = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, 0.05], "normal": [0.0, 0.0, 1.0] },
        }),
    );
    let b = one_body(&above);
    assert!(b["loops"].as_array().expect("loops").is_empty(), "{above}");
    assert_eq!(f(&b["area_m2"]), 0.0);
    assert_eq!(b["centroid"], Value::Null, "no cap, no centroid");
    assert_eq!(
        b["kept_material"], true,
        "the plane missed on the KEPT side: the whole body survives"
    );
    assert!(
        above["declines"].is_null(),
        "a miss is not a decline: {above}"
    );

    let below = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, -0.05], "normal": [0.0, 0.0, 1.0] },
        }),
    );
    let b = one_body(&below);
    assert!(b["loops"].as_array().expect("loops").is_empty());
    assert_eq!(
        b["kept_material"], false,
        "the plane missed on the DISCARDED side: no material is left"
    );
}

/// A cut plane COPLANAR with a model face — a pocket floor — is a legitimate
/// section, and the answer says the cap came through the §4.5.5 Stage-0
/// overlay rather than from the cutting box's own lineage.
#[test]
fn a_cut_coplanar_with_a_pocket_floor_says_the_cap_is_shared() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    // A 10 mm square pocket 4 mm deep, cut UP from the sketch plane at z = 0
    // (so the pocket is open at the plate's underside): its CEILING is a
    // horizontal model face at z = 4 mm.
    let body = prism(
        &mut state,
        &mut kernel,
        (0.005, 0.005, 0.015, 0.015),
        0.004,
        500,
        CombineMode::Cut,
        false,
    );

    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            // Keep the material ABOVE the ceiling plane: the viewer looks down
            // at a cap that IS the former ceiling, extended over the whole
            // plate.
            "plane": { "origin": [0.0, 0.0, 0.004], "normal": [0.0, 0.0, -1.0] },
        }),
    );
    let b = one_body(&out);
    assert_eq!(
        b["cap_shared_with_model"], true,
        "the cap of a coplanar cut is the Stage-0 shared trimmed surface, and \
         the answer must say so: {out}"
    );
    // The kept half is the full 40 × 40 × 6 block above the ceiling, so the cap
    // is the whole square — including the part that WAS the pocket ceiling.
    let loops = b["loops"].as_array().expect("loops");
    assert_eq!(loops.len(), 1, "{b}");
    assert_eq!(f(&b["area_m2"]), 1.6e-3, "{b}");
    assert_eq!(b["method"], "exact");
    assert_eq!(b["kept_material"], true);

    // THE OTHER SIDE of the same coplanar cut: the kept slab is the 4 mm
    // bottom with the pocket through it, so the cap is a ring — and the flag
    // is FALSE, because every face of THAT cap descended from the cutting
    // half-space and the kernel's own lineage could name it. The flag is about
    // how the kept cap was attributed, not about whether some face of the body
    // happens to be coplanar with the plane. Measured 2026-10-03; pinned
    // because an agent reading it as "a face is coplanar here" would read the
    // two sides of one cut as two different geometries.
    let other = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, 0.004], "normal": [0.0, 0.0, 1.0] },
        }),
    );
    let b = one_body(&other);
    assert_eq!(b["cap_shared_with_model"], false, "{other}");
    let loops = b["loops"].as_array().expect("loops");
    assert_eq!(loops.len(), 2, "the ring around the pocket: {b}");
    assert_eq!(f(&b["area_m2"]), 1.6e-3 - 1.0e-4, "40² − 10² mm²: {b}");
}

/// The same cut, asked for by the pocket ceiling's N1 NAME: a named face's
/// plane is cut with the face's OWN outward normal, so the kept side is the
/// material behind the face — which is the same answer the explicit
/// `normal: [0, 0, −1]` form gives above, to the last bit.
///
/// The name is resolved the way `names_list` reports it, which is what
/// `measure_distance` does with a name operand: one question answered one way.
#[test]
fn a_plane_can_be_named_by_a_face_name() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    let body = prism(
        &mut state,
        &mut kernel,
        (0.005, 0.005, 0.015, 0.015),
        0.004,
        500,
        CombineMode::Cut,
        false,
    );
    // The pocket floor: a face pointing DOWN that sits above the base plane.
    let all = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    let floor = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .find(|face| {
            let c = &face["signature"]["centroid"];
            face["signature"]["normal"][2].as_f64().unwrap_or(0.0) < -0.5
                && c[2].as_f64().unwrap_or(0.0) > 1e-9
        })
        .unwrap_or_else(|| panic!("a pocket floor: {all}"))
        .clone();
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": floor["geom_ref"] }, "name": "floor" }),
    );

    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({ "body_ids": [body], "plane": { "name": "floor" } }),
    );
    let b = one_body(&out);
    assert!(
        out["name_warnings"].is_null(),
        "the name resolved by its own persistent id: {out}"
    );
    // The ceiling's outward normal points down, so the kept half-space is the
    // material above it: the full 40 mm square cap, coplanar with the face the
    // name was given to.
    let loops = b["loops"].as_array().expect("loops");
    assert_eq!(loops.len(), 1, "{b}");
    assert_eq!(f(&b["area_m2"]), 1.6e-3, "40² mm², exactly: {b}");
    assert_eq!(b["method"], "exact");
    assert_eq!(b["cap_shared_with_model"], true, "{b}");
    assert_eq!(f(&b["centroid"][2]), 0.004, "the cut is at the face: {b}");

    // A name that is not a planar face is refused where the caller can fix it.
    let (code, message) = refusal(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({ "body_ids": [body], "plane": { "name": "no_such_name" } }),
    );
    assert_eq!(code, "InvalidArguments");
    assert!(
        message.contains("no_such_name"),
        "the refusal names the name: {message}"
    );
}

/// `{"plane": "XY"}` cuts on the datum plane, through the SAME resolver
/// `sketch_create` uses — so a plane an agent can sketch on is one it can
/// section with.
#[test]
fn the_datum_plane_form_cuts_on_the_datum() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    // Symmetric about the sketch plane, so the XY datum cuts it at mid height.
    let body = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.02),
        0.01,
        1,
        CombineMode::NewBody,
        true,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({ "body_ids": [body], "plane": { "plane": "XY" } }),
    );
    let b = one_body(&out);
    assert_eq!(f(&b["area_m2"]), 0.04 * 0.02, "{b}");
    assert_eq!(b["method"], "exact");
    assert_eq!(f(&out["basis"]["origin"][2]), 0.0, "the datum is z = 0");
}

/// No `body_ids` sections EVERY body of the open Part, and the total is the
/// sum — so "section the model" is one call.
#[test]
fn body_ids_defaults_to_every_body() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let a = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    let b = prism(
        &mut state,
        &mut kernel,
        (0.06, 0.0, 0.08, 0.04),
        0.01,
        500,
        CombineMode::NewBody,
        false,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({ "plane": { "origin": [0.0, 0.0, 0.005], "normal": [0.0, 0.0, 1.0] } }),
    );
    let bodies = out["bodies"].as_array().expect("bodies");
    assert_eq!(bodies.len(), 2, "both bodies: {out}");
    let ids: Vec<&str> = bodies
        .iter()
        .map(|b| b["body_id"].as_str().unwrap_or_default())
        .collect();
    assert!(
        ids.contains(&a.as_str()) && ids.contains(&b.as_str()),
        "{out}"
    );
    assert_eq!(
        f(&out["total_area_m2"]),
        1.6e-3 + 0.02 * 0.04,
        "the total is the sum of the caps: {out}"
    );
}

/// A degenerate plane is refused at the boundary, with the numbers named — not
/// cut as some default plane.
#[test]
fn a_zero_length_normal_is_refused() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    let (_, message) = refusal(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, 0.005], "normal": [0.0, 0.0, 0.0] },
        }),
    );
    assert!(
        message.contains("zero-length") || message.contains("not finite"),
        "the refusal says what is wrong with the plane: {message}"
    );
}

/// A name that resolves to a CURVED face is refused where the caller can fix
/// it — never cut on some tangent plane of it.
///
/// `{"name": …}` is the one plane form whose geometry the tool cannot check
/// from the argument itself: an `{origin, normal}` is a plane by construction
/// and a datum is one by definition, but a name points at whatever face it was
/// given to. So the surface type is read before the cut, and a cylinder's
/// barrel is an `InvalidArguments` naming the surface it found and the form to
/// use instead.
#[test]
fn a_name_on_a_curved_face_is_refused() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = cylinder(
        &mut state,
        &mut kernel,
        (0.0, 0.0),
        0.010,
        0.020,
        1,
        CombineMode::NewBody,
    );
    let all = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    let barrel = all["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .find(|face| face["signature"]["surface_type"] == "cylindrical")
        .unwrap_or_else(|| panic!("a cylinder has a barrel: {all}"))
        .clone();
    ok(
        &mut state,
        &mut kernel,
        "entity_name",
        json!({ "target": { "type": "entity", "geom_ref": barrel["geom_ref"] }, "name": "barrel" }),
    );

    let (code, message) = refusal(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({ "body_ids": [body], "plane": { "name": "barrel" } }),
    );
    assert_eq!(code, "InvalidArguments");
    assert!(
        message.contains("cylindrical") && message.contains("planar"),
        "the refusal names what the face is and what a plane needs: {message}"
    );
}

/// A plane that GRAZES the body — exactly on one of its faces, touching it and
/// cutting nothing — is a typed empty section on both sides, and which side
/// keeps the material is the convention `(p − origin)·n̂ ≤ 0` says it is.
///
/// This is the boundary case of the two misses already pinned above: the plane
/// is not clear of the body, it is ON it. The kept half-space CONTAINS the
/// plane, so a body sitting entirely on the normal's negative side (including
/// its grazing face) survives whole with no cap, and the same plane turned
/// around discards all of it. Neither is an error and neither is a decline:
/// the cut is decided from the body's conservative bounds before any boolean
/// runs, so a graze cannot reach the Stage-0 coplanar wall by accident.
#[test]
fn a_plane_grazing_a_face_is_a_typed_empty_section_on_both_sides() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let h = 0.01;
    let body = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        h,
        1,
        CombineMode::NewBody,
        false,
    );

    // On the top face, looking UP: every point of the plate is at or below the
    // plane, so the whole plate is kept and there is no cap.
    let up = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, h], "normal": [0.0, 0.0, 1.0] },
        }),
    );
    let b = one_body(&up);
    assert!(
        b["loops"].as_array().expect("loops").is_empty(),
        "a graze cuts nothing: {up}"
    );
    assert_eq!(
        b["kept_material"], true,
        "the plane's own side is KEPT: {up}"
    );
    assert_eq!(f(&b["area_m2"]), 0.0, "{up}");
    assert!(up["declines"].is_null(), "a graze is not a decline: {up}");

    // The same plane turned around: every point is at or above it, so nothing
    // survives — and that is still a typed empty section.
    let down = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, h], "normal": [0.0, 0.0, -1.0] },
        }),
    );
    let b = one_body(&down);
    assert!(b["loops"].as_array().expect("loops").is_empty(), "{down}");
    assert_eq!(
        b["kept_material"], false,
        "the body is entirely on the discarded side: {down}"
    );
    assert!(down["declines"].is_null(), "{down}");
}

/// A section is a QUERY: it leaves the body's own faces exactly where they
/// were, named by the same persistent ids.
///
/// `section_with_plane` cuts with the real Intersect in the LIVE arena, so this
/// is not free by construction — a cut that renumbered the model's faces would
/// move every `GeomRef`, name binding and rule an agent had already written,
/// and nothing in the section's own answer would say so. Pinned over two
/// sections of a bored plate, the geometry most likely to re-key: the full face
/// listing must come back byte-identical.
#[test]
fn a_section_leaves_the_bodys_faces_where_they_were() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    let body = cylinder(
        &mut state,
        &mut kernel,
        (0.0, 0.0),
        0.006,
        0.02,
        500,
        CombineMode::Cut,
    );
    let before = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    for z in [0.005, 0.0075] {
        ok(
            &mut state,
            &mut kernel,
            "measure_section",
            json!({
                "body_ids": [body],
                "plane": { "origin": [0.0, 0.0, z], "normal": [0.0, 0.0, 1.0] },
            }),
        );
    }
    let after = ok(
        &mut state,
        &mut kernel,
        "face_list",
        json!({ "body_id": body }),
    );
    assert_eq!(
        before, after,
        "two sections moved the body's own face listing"
    );
}

/// A non-unit normal is normalized, and the answer reports the normal it
/// actually cut with rather than echoing the caller's.
#[test]
fn a_non_unit_normal_is_normalized_in_the_answer() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = prism(
        &mut state,
        &mut kernel,
        (0.0, 0.0, 0.04, 0.04),
        0.01,
        1,
        CombineMode::NewBody,
        false,
    );
    let out = ok(
        &mut state,
        &mut kernel,
        "measure_section",
        json!({
            "body_ids": [body],
            "plane": { "origin": [0.0, 0.0, 0.005], "normal": [0.0, 0.0, 7.0] },
        }),
    );
    assert_eq!(out["plane"]["normal"], json!([0.0, 0.0, 1.0]), "{out}");
    assert_eq!(f(&one_body(&out)["area_m2"]), 1.6e-3);
}
