//! Custom feature scripts on the REAL kernel (kernel-v2) — spec
//! `specs/custom_features_and_modeling_roadmap.md` Part A oracles:
//!
//! 1. A box script's body has the exact box volume (the script's extrude IS
//!    the ordinary extrude).
//! 2. The shipped `gear.rhai` builds a gear both ways — the ported profile
//!    (points/lines/arcs/splines) and the built-in `gear` entity — and the
//!    two solids agree in volume within the two profile builders' chord
//!    difference; both are watertight with χ = 2.
//! 3. A script feature is a boolean operand like any other.

use std::collections::BTreeMap;

use feature_engine::script::library::GEAR_RHAI;
use feature_engine::types::*;
use serde_json::json;
use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;
use uuid::Uuid;

fn script_op(source_id: Uuid, args: serde_json::Value) -> Operation {
    let args: BTreeMap<String, serde_json::Value> = serde_json::from_value(args).unwrap();
    Operation::Script {
        params: ScriptParams {
            source_id,
            entry: "feature".into(),
            args,
            arg_exprs: BTreeMap::new(),
            arg_values: BTreeMap::new(),
        },
    }
}

fn plane_z() -> serde_json::Value {
    json!({ "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] })
}

fn add_source(b: &mut ModelBuilder, text: &str) -> Uuid {
    let id = Uuid::new_v4();
    b.state.engine.sources.insert_text(id, text);
    id
}

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
}

#[test]
fn box_script_has_the_exact_volume() {
    let mut b = ModelBuilder::kernel_v2();
    let src = add_source(
        &mut b,
        r#"
// @feature name="Box" version=1
// @param w: length = 0.02
// @param h: length = 0.01
// @param d: length = 0.005
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, p.w, p.h);
    ctx.extrude(sk.finish().regions()[0], #{ depth: p.d })
}
"#,
    );
    b.add_operation(
        "Box",
        script_op(src, json!({ "plane": plane_z(), "w": 0.03 })),
    )
    .unwrap();
    assert_clean(&b, "box script");
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&b.solid_handle("Box").unwrap())
        .expect("exact volume");
    assert!((v - 0.03 * 0.01 * 0.005).abs() < 1e-15, "{v}");
}

#[test]
fn gear_script_builds_both_routes_on_the_real_kernel() {
    let mut b = ModelBuilder::kernel_v2();
    let src = add_source(&mut b, GEAR_RHAI);
    let common = json!({
        "tooth_count": 24, "module_m": 0.002, "pressure_angle_deg": 20.0,
        "face_width": 0.008, "plane": plane_z()
    });
    // Route 1: the ported profile (external gear).
    b.add_operation("Gear port", script_op(src, common.clone()))
        .unwrap();
    assert_clean(&b, "gear port");
    let port = b.tessellate_last_with_tol(0.001).unwrap();
    let wt = oracle::check_watertight_mesh(&port);
    assert!(wt.passed, "port: {}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&port, 2);
    assert!(chi.passed, "port: {}", chi.detail);
    let v_port = mesh_signed_volume(&port);

    // Route 2: the built-in gear entity, via a tiny script.
    let src2 = add_source(
        &mut b,
        r#"
// @feature name="Gear entity" version=1
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.gear(#{ tooth_count: 24, module_m: 0.002, pressure_angle_deg: 20.0, center_x: 0.2 });
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.008 })
}
"#,
    );
    b.add_operation(
        "Gear entity",
        script_op(src2, json!({ "plane": plane_z() })),
    )
    .unwrap();
    assert_clean(&b, "gear entity");
    let ent = b.tessellate_last_with_tol(0.001).unwrap();
    let wt = oracle::check_watertight_mesh(&ent);
    assert!(wt.passed, "entity: {}", wt.detail);
    let v_ent = mesh_signed_volume(&ent);

    // Same gear: pitch radius 24 mm, face 8 mm ⇒ roughly π·0.024²·0.008 ≈ 1.45e-5.
    // The two profile builders differ in how they chord the involute
    // (fitted B-spline vs the sampled points) and the tip/root arcs, so the
    // agreement is to a few tenths of a percent, not exact.
    let rel = (v_port - v_ent).abs() / v_ent;
    assert!(rel < 5e-3, "port {v_port} vs entity {v_ent}: rel {rel}");
    assert!(v_ent > 1.2e-5 && v_ent < 1.7e-5, "{v_ent}");

    // Two live bodies.
    assert_eq!(b.distinct_solid_count(), 2);
}

#[test]
fn script_body_is_a_boolean_operand() {
    let mut b = ModelBuilder::kernel_v2();
    let src = add_source(
        &mut b,
        r#"
// @feature name="Slab" version=1
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.0, 0.0, 0.1, 0.1);
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.02 })
}
"#,
    );
    let slab = b
        .add_operation("Slab", script_op(src, json!({ "plane": plane_z() })))
        .unwrap();
    // A tree-level cut into the script's body.
    b.true_circle_sketch("Hole", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.05, 0.05, 0.01)
        .unwrap();
    let cut = Operation::Extrude {
        params: ExtrudeParams {
            combine: Some(CombineMode::Cut),
            targets: Some(vec![waffle_types::GeomRef {
                kind: waffle_types::TopoKind::Solid,
                anchor: waffle_types::Anchor::FeatureOutput {
                    feature_id: slab,
                    output_key: waffle_types::OutputKey::Main,
                },
                selector: waffle_types::Selector::Role {
                    role: waffle_types::Role::EndCapPositive,
                    index: 0,
                },
                policy: waffle_types::ResolvePolicy::Strict,
                scope: None,
            }]),
            sketch_id: b.feature_id("Hole").unwrap(),
            profile_index: 0,
            profile_entity_ids: None,
            depth: 0.05,
            direction: None,
            symmetric: false,
            cut: true,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            depth_expr: None,
        },
    };
    b.add_operation("Bore", cut).unwrap();
    assert_clean(&b, "bore into script body");
    assert!(b.consumed_features().contains(&slab));
    let mesh = b.tessellate_last_with_tol(0.001).unwrap();
    let v = mesh_signed_volume(&mesh);
    let expect = 0.1 * 0.1 * 0.02 - std::f64::consts::PI * 0.01 * 0.01 * 0.02;
    assert!((v - expect).abs() < expect * 2e-3, "{v} vs {expect}");
}
