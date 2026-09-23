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
//! 4. The shipped `sprocket.rhai` builds an ISO 606 sprocket both ways — the
//!    ported arcs and the built-in `sprocket` entity — and, the two being
//!    bit-identical sketches (`script_sprocket_parity.rs`), the solids have
//!    the same exact volume and one cylindrical wall per arc.

use std::collections::BTreeMap;

use feature_engine::script::library::{GEAR_RHAI, SPROCKET_RHAI};
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
fn sprocket_script_builds_both_routes_on_the_real_kernel() {
    let mut b = ModelBuilder::kernel_v2();
    let src = add_source(&mut b, SPROCKET_RHAI);
    // ISO 08B, 12 teeth, 5 mm plate: 1 + 12 × 7 points, 48 arcs.
    b.add_operation(
        "Sprocket port",
        script_op(
            src,
            json!({
                "tooth_count": 12, "pitch": 0.0127, "roller_diameter": 0.00851,
                "face_width": 0.005, "plane": plane_z()
            }),
        ),
    )
    .unwrap();
    assert_clean(&b, "sprocket port");
    let port = b.tessellate_last_with_tol(0.001).unwrap();
    let wt = oracle::check_watertight_mesh(&port);
    assert!(wt.passed, "port: {}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&port, 2);
    assert!(chi.passed, "port: {}", chi.detail);
    let port_handle = b.solid_handle("Sprocket port").unwrap();
    let v_port = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&port_handle)
        .expect("exact volume");
    // 4 arcs per tooth ⇒ 4z cylindrical walls + 2 caps.
    let faces_port = b
        .kernel_ref()
        .as_introspect()
        .list_faces(&port_handle)
        .len();
    assert_eq!(faces_port, 4 * 12 + 2, "port face count");

    // Route 2: the built-in sprocket entity, via a tiny script.
    let src2 = add_source(
        &mut b,
        r#"
// @feature name="Sprocket entity" version=1
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.sprocket(#{ tooth_count: 12, pitch: 0.0127, roller_diameter: 0.00851, center_x: 0.2 });
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.005 })
}
"#,
    );
    b.add_operation(
        "Sprocket entity",
        script_op(src2, json!({ "plane": plane_z() })),
    )
    .unwrap();
    assert_clean(&b, "sprocket entity");
    let ent = b.tessellate_last_with_tol(0.001).unwrap();
    let wt = oracle::check_watertight_mesh(&ent);
    assert!(wt.passed, "entity: {}", wt.detail);
    let ent_handle = b.solid_handle("Sprocket entity").unwrap();
    let v_ent = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&ent_handle)
        .expect("exact volume");
    assert_eq!(
        b.kernel_ref().as_introspect().list_faces(&ent_handle).len(),
        faces_port
    );

    // The two sketches are bit-identical (parity oracle) up to the 0.2 m
    // placement, so the exact volumes agree to rounding.
    assert!(
        (v_port - v_ent).abs() < 1e-12 * v_ent.max(1e-9),
        "port {v_port} vs entity {v_ent}"
    );
    // Sanity: a 12T 08B sprocket (pitch Ø 49.07 mm, tip Ø ≈ 53.5 mm) is the
    // pitch-circle disc (π·0.02454²·0.005 ≈ 9.46e-6) less twelve roller
    // seats plus the tooth tips — measured 7.995e-6 (≈ 85 % of the disc).
    assert!(v_ent > 7.0e-6 && v_ent < 1.0e-5, "{v_ent}");

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

// ── A-M3: query chains, named outputs, connectors, outer references ─────────

/// 5. A boss placed on the base's top face through a QUERY CHAIN (planar,
///    farthest along +z) and merged into it: one body with the exact summed
///    volume; the named top face and the script's connector both sit at the
///    boss's top plane.
#[test]
fn boss_on_top_via_query_chain_named_face_and_connector_on_the_real_kernel() {
    let mut b = ModelBuilder::kernel_v2();
    let src = add_source(
        &mut b,
        r#"
// @feature name="Boss on top" version=1
// @param plane: plane
// @output main: main
// @output top: face
// @output top_pin: connector
fn feature(ctx, p) {
    let a = ctx.sketch(p.plane);
    a.rect(0.0, 0.0, 0.02, 0.02);
    let base = ctx.extrude(a.finish().regions()[0], #{ depth: 0.01 });
    let base_top = created_by(base).faces().surface_type("planar").farthest_along([0.0, 0.0, 1.0]);
    let s = ctx.sketch(base_top);
    s.rect(-0.005, -0.005, 0.01, 0.01);
    let boss = ctx.extrude(s.finish().regions()[0], #{ depth: 0.004, combine: "Add", targets: [base] });
    let top = created_by(boss).faces().surface_type("planar").farthest_along([0.0, 0.0, 1.0]);
    ctx.mate_connector(#{ name: "top_pin", on: top });
    #{ main: boss, top: top }
}
"#,
    );
    let id = b
        .add_operation("Boss", script_op(src, json!({ "plane": plane_z() })))
        .unwrap();
    assert_clean(&b, "boss on top");
    assert_eq!(b.distinct_solid_count(), 1);
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&b.solid_handle("Boss").unwrap())
        .expect("exact volume");
    let expect = 0.02 * 0.02 * 0.01 + 0.01 * 0.01 * 0.004;
    assert!((v - expect).abs() < 1e-15, "{v} vs {expect}");

    // The script's connector: on the boss top, z up, at z = 0.014.
    let pin = b
        .state
        .engine
        .connectors
        .iter()
        .find(|c| c.name == "top_pin")
        .expect("script connector exposed")
        .clone();
    assert_eq!(pin.feature_id, id);
    assert!(
        (pin.frame.origin[2] - 0.014).abs() < 1e-12,
        "{:?}",
        pin.frame
    );
    assert!((pin.frame.z_axis[2] - 1.0).abs() < 1e-12, "{:?}", pin.frame);

    // A tree connector on the NAMED face lands on the same plane.
    let on_top = Operation::MateConnector {
        params: MateConnectorParams {
            name: "On top".into(),
            geom_ref: Some(waffle_types::GeomRef {
                kind: waffle_types::TopoKind::Face,
                anchor: waffle_types::Anchor::FeatureOutput {
                    feature_id: id,
                    output_key: waffle_types::OutputKey::Main,
                },
                selector: waffle_types::Selector::Role {
                    role: waffle_types::Role::Named { name: "top".into() },
                    index: 0,
                },
                policy: waffle_types::ResolvePolicy::Strict,
                scope: None,
            }),
            ..Default::default()
        },
    };
    b.add_operation("On top", on_top).unwrap();
    assert_clean(&b, "connector on the named face");
    let c = b
        .state
        .engine
        .connectors
        .iter()
        .find(|c| c.name == "On top")
        .unwrap();
    assert!((c.frame.origin[2] - 0.014).abs() < 1e-12, "{:?}", c.frame);
    // Same face as the script's own connector ⇒ the same frame (the boss is
    // centred on the base top, 10 mm from each edge; the sketch plane's v axis
    // is not world y, so compare frames rather than hard-code the basis).
    assert_eq!(
        c.frame.origin, pin.frame.origin,
        "{:?} vs {:?}",
        c.frame, pin.frame
    );
    assert!(
        (c.frame.origin[0].abs() - 0.01).abs() < 1e-12
            && (c.frame.origin[1].abs() - 0.01).abs() < 1e-12,
        "{:?}",
        c.frame
    );
}

/// 6. A script CUTS an OUTER body handed in as a `body` parameter: the tree
///    body is consumed by the node, and the node's body has the exact
///    remaining volume.
#[test]
fn a_script_cuts_an_outer_body_parameter_on_the_real_kernel() {
    let mut b = ModelBuilder::kernel_v2();
    b.rect_sketch("Sk", [0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.0, 0.02, 0.02)
        .unwrap();
    let block = b.extrude_no_merge("Block", "Sk", 0.01).unwrap();
    let src = add_source(
        &mut b,
        r#"
// @feature name="Square bore" version=1
// @param target: body
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.rect(0.005, 0.005, 0.005, 0.005);
    ctx.extrude(sk.finish().regions()[0], #{ depth: 0.03, combine: "Cut", targets: [p.target] })
}
"#,
    );
    let target = json!({
        "kind": { "type": "Solid" },
        "anchor": { "type": "FeatureOutput", "feature_id": block, "output_key": { "type": "Main" } },
        "selector": { "type": "Role", "role": { "type": "EndCapPositive" }, "index": 0 },
        "policy": { "type": "Strict" }
    });
    let bore = b
        .add_operation(
            "Bore",
            script_op(src, json!({ "plane": plane_z(), "target": target })),
        )
        .unwrap();
    assert_clean(&b, "square bore");
    assert!(
        b.consumed_features().contains(&block),
        "the outer block is consumed"
    );
    assert_eq!(b.state.engine.consumed_by.get(&bore), Some(&vec![block]));
    assert_eq!(b.distinct_solid_count(), 1);
    let v = b
        .kernel_ref()
        .as_introspect()
        .solid_volume(&b.solid_handle("Bore").unwrap())
        .expect("exact volume");
    let expect = 0.02 * 0.02 * 0.01 - 0.005 * 0.005 * 0.01;
    assert!((v - expect).abs() < 1e-15, "{v} vs {expect}");
}
