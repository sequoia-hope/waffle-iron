//! Q2 and Q3 of `specs/agent_mechanical_design.md` §4.3 end to end: the
//! `measure_interference` and `measure_mass` tools over the real kernel.
//!
//! The kernel-v2 suites own the geometry oracles
//! (`crates/kernel-v2/tests/q2_interference.rs`, `q3_mass_properties.rs`).
//! What this file pins is the layer a host actually crosses — tool args →
//! `UiToEngine` → `KernelMeasure` → kernel → the wire shape — and the things
//! that layer can get wrong on its own: the three Q2 outcomes must each have
//! their own wire shape, `disjoint` must carry Q1's own number rather than a
//! second derivation of it, and the Q3 density default must be REPORTED
//! rather than silently assumed.

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

/// An `s`-sided cube whose near corner sits at `(x0, 0)` in the sketch frame,
/// as its own body. Returns its body id.
fn cube(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    x0: f64,
    s: f64,
    base: u32,
) -> String {
    let corners = [
        (base, x0, 0.0),
        (base + 1, x0 + s, 0.0),
        (base + 2, x0 + s, s),
        (base + 3, x0, s),
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
                    depth: s,
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
    // The host tessellates after every dispatch; the tools that list or
    // measure a body require a rendered one.
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    FeatureTree::body_id(extrude, &OutputKey::Main)
}

fn interference(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    a: &str,
    b: &str,
) -> wasm_bridge::ToolResult {
    execute_tool(
        state,
        kernel,
        "measure_interference",
        &json!({ "a": a, "b": b }),
        None,
    )
}

fn mass(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    args: Value,
) -> wasm_bridge::ToolResult {
    execute_tool(state, kernel, "measure_mass", &args, None)
}

#[track_caller]
fn ok(r: wasm_bridge::ToolResult) -> Value {
    assert!(!r.is_error, "the tool failed: {r:?}");
    r.structured_content
}

#[test]
fn interference_reports_each_of_the_three_outcomes_on_the_wire() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    // 10 mm cubes: one at 0, one overlapping it by 5 mm, one touching, one
    // 5 mm away. They share their other two spans, so the only variable is
    // the offset along the sketch's u axis.
    let a = cube(&mut state, &mut kernel, 0.0, 0.01, 1);
    let over = cube(&mut state, &mut kernel, 0.005, 0.01, 101);
    let touch = cube(&mut state, &mut kernel, 0.01, 0.01, 201);
    let apart = cube(&mut state, &mut kernel, 0.015, 0.01, 301);

    let out = ok(interference(&mut state, &mut kernel, &a, &over));
    assert_eq!(out["kind"], "interferes", "{out}");
    assert_eq!(out["a"], json!(a));
    assert_eq!(out["b"], json!(over));
    assert_eq!(out["method"], "exact", "{out}");
    let v = out["volume_m3"].as_f64().expect("a volume");
    assert!(
        (v - 0.005 * 0.01 * 0.01).abs() < 1e-18,
        "the analytic 5 × 10 × 10 mm overlap: {out}"
    );
    let regions = out["regions"].as_array().expect("regions").clone();
    assert_eq!(regions.len(), 1, "{out}");
    assert!(
        (regions[0]["volume_m3"].as_f64().unwrap() - v).abs() < 1e-21,
        "the one lump is the whole region: {out}"
    );
    assert_eq!(
        regions[0]["centroid"].as_array().map(Vec::len),
        Some(3),
        "a lump names where it is: {out}"
    );

    let out = ok(interference(&mut state, &mut kernel, &a, &touch));
    assert_eq!(out["kind"], "contact", "{out}");
    assert_eq!(
        out["evidence"], "empty_intersection_at_zero_distance",
        "{out}"
    );
    assert_eq!(out["closest"]["value_m"], json!(0.0), "{out}");
    assert_eq!(out["closest"]["method"], "exact", "{out}");

    let out = ok(interference(&mut state, &mut kernel, &a, &apart));
    assert_eq!(out["kind"], "disjoint", "{out}");
    let gap = out["distance"]["value_m"].as_f64().expect("a gap");
    assert!((gap - 0.005).abs() < 1e-15, "{out}");
    // The same number `measure_distance` gives for the same pair: `disjoint`
    // reuses Q1 rather than deriving a second gap.
    let q1 = ok(execute_tool(
        &mut state,
        &mut kernel,
        "measure_distance",
        &json!({
            "a": { "type": "body", "body_id": a },
            "b": { "type": "body", "body_id": apart },
        }),
        None,
    ));
    assert_eq!(
        out["distance"]["value_m"], q1["distance_m"],
        "{out} vs {q1}"
    );
}

#[test]
fn interference_refuses_a_body_it_cannot_find_and_names_it() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let a = cube(&mut state, &mut kernel, 0.0, 0.01, 1);
    let r = interference(&mut state, &mut kernel, &a, "nope/Main");
    assert!(r.is_error, "a missing body is an error: {r:?}");
    assert_eq!(
        r.structured_content["error"]["code"], "BodyNotFound",
        "{:?}",
        r.structured_content
    );
}

#[test]
fn mass_reports_the_density_it_used_and_the_tier_it_is() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let s = 0.01;
    let a = cube(&mut state, &mut kernel, 0.0, s, 1);

    let out = ok(mass(&mut state, &mut kernel, json!({ "body_id": a })));
    assert_eq!(out["body_id"], json!(a));
    assert_eq!(out["method"], "exact", "a box is the exact tier: {out}");
    assert!(
        out.get("chord_bound_m").is_none(),
        "an exact answer carries no band: {out}"
    );
    let v = out["volume_m3"].as_f64().expect("a volume");
    assert!((v - s * s * s).abs() < 1e-21, "{out}");
    assert!(
        (out["surface_area_m2"].as_f64().unwrap() - 6.0 * s * s).abs() < 1e-18,
        "{out}"
    );
    // The default density is REPORTED, not assumed silently — the document
    // model has no material table to read one from.
    assert_eq!(out["density_kg_m3"], json!(1.0), "{out}");
    assert!(
        (out["mass_kg"].as_f64().unwrap() - v).abs() < 1e-21,
        "at density 1 the mass is numerically the volume: {out}"
    );
    // I_xx = m(b² + c²)/12 for a cube of side s.
    let want_i = v * (s * s + s * s) / 12.0;
    for k in 0..3 {
        let got = out["inertia_at_centroid"][k][k].as_f64().expect("a tensor");
        assert!(
            (got - want_i).abs() <= 1e-10 * want_i,
            "I[{k}][{k}] = {got:e}, want {want_i:e}: {out}"
        );
        assert!(
            (out["principal_moments"][k].as_f64().unwrap() - want_i).abs() <= 1e-10 * want_i,
            "{out}"
        );
    }
    assert_eq!(
        out["principal_axes"].as_array().map(Vec::len),
        Some(3),
        "{out}"
    );

    // Density scales mass and inertia, and nothing else.
    let steel = ok(mass(
        &mut state,
        &mut kernel,
        json!({ "body_id": a, "density_kg_m3": 7850.0 }),
    ));
    assert_eq!(steel["density_kg_m3"], json!(7850.0), "{steel}");
    assert!(
        (steel["mass_kg"].as_f64().unwrap() - 7850.0 * v).abs() <= 1e-12 * 7850.0 * v,
        "{steel}"
    );
    assert_eq!(steel["volume_m3"], out["volume_m3"], "{steel}");
    assert_eq!(steel["centroid"], out["centroid"], "{steel}");

    // A density that cannot scale anything is refused, not used.
    for bad in [json!(0.0), json!(-1.0), json!("heavy")] {
        let r = mass(
            &mut state,
            &mut kernel,
            json!({ "body_id": a, "density_kg_m3": bad }),
        );
        assert!(r.is_error, "density {bad} must be refused: {r:?}");
    }

    let r = mass(&mut state, &mut kernel, json!({ "body_id": "nope/Main" }));
    assert!(r.is_error, "{r:?}");
    assert_eq!(r.structured_content["error"]["code"], "BodyNotFound");
}
