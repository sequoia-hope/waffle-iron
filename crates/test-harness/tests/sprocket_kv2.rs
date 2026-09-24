//! The `Sprocket` sketch entity (spec
//! `specs/custom_features_and_modeling_roadmap.md` §B3) on the REAL kernel
//! (kernel-v2):
//!
//! 1. A compact sprocket sketch extrudes to a watertight χ = 2 solid whose
//!    exact B-Rep volume equals the analytic area of the arc-bounded profile
//!    times the depth — the walls are the exact cylinders the arcs name
//!    (one face per arc, plus two caps).
//! 2. The sprocket body is a boolean operand like any other: a bore cut
//!    through it removes exactly the bore's volume.
//! 3. The script API's `sk.sprocket` reaches the same generator: the same
//!    exact volume.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

use feature_engine::types::*;
use serde_json::json;
use test_harness::helpers::mesh_signed_volume;
use test_harness::oracle;
use test_harness::ModelBuilder;
use uuid::Uuid;
use waffle_types::kernel::KernelSolidHandle;
use waffle_types::*;

const DEPTH: f64 = 0.005;

fn iso_08b(z: u32) -> SprocketParams {
    SprocketParams {
        tooth_count: z,
        pitch: 0.0127,
        roller_diameter: 0.00851,
        ..Default::default()
    }
}

fn plane_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn sprocket_sketch(params: SprocketParams) -> Operation {
    Operation::Sketch {
        sketch: Sketch {
            id: Uuid::new_v4(),
            plane: plane_ref(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities: vec![SketchEntity::Sprocket {
                id: 1,
                params,
                construction: false,
            }],
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
            projected: vec![],
        },
    }
}

fn assert_clean(b: &ModelBuilder, label: &str) {
    let errors = b.engine_errors().to_vec();
    assert!(errors.is_empty(), "{label}: engine errors: {errors:?}");
}

fn exact_volume(b: &ModelBuilder, handle: &KernelSolidHandle) -> f64 {
    b.kernel_ref()
        .as_introspect()
        .solid_volume(handle)
        .expect("exact volume")
}

/// The exact area enclosed by a finished arc profile: the shoelace area of
/// its sampled vertex loop plus, per chord of every arc, the circular
/// segment between chord and arc (signed by the traversal direction about
/// the arc's centre: a convex arc bulges outward, a concave seating arc
/// bows inward).
fn exact_profile_area(profile: &ClosedProfile, positions: &HashMap<u32, (f64, f64)>) -> f64 {
    let pts: Vec<(f64, f64)> = profile.vertex_ids.iter().map(|id| positions[id]).collect();
    let n = pts.len();
    let mut area = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        area += 0.5 * (p.0 * q.1 - q.0 * p.1);
    }
    for seg in &profile.arc_segments {
        let c = (seg.center_u, seg.center_v);
        let r = seg.radius;
        let mut i = seg.start_vertex_index;
        loop {
            let j = (i + 1) % n;
            let a = (pts[i].0 - c.0, pts[i].1 - c.1);
            let b = (pts[j].0 - c.0, pts[j].1 - c.1);
            let phi = (a.0 * b.1 - a.1 * b.0).atan2(a.0 * b.0 + a.1 * b.1);
            area += phi.signum() * 0.5 * r * r * (phi.abs() - phi.abs().sin());
            if j == seg.end_vertex_index {
                break;
            }
            i = j;
        }
    }
    area
}

fn analytic_sprocket_area(params: &SprocketParams) -> f64 {
    let r = generate_sprocket_profile(params).unwrap();
    exact_profile_area(&r.profiles[0], &r.positions)
}

#[test]
fn sprocket_extrudes_to_an_exact_arc_walled_solid() {
    for z in [9u32, 20, 52] {
        let mut b = ModelBuilder::kernel_v2();
        b.add_operation("Sprocket sketch", sprocket_sketch(iso_08b(z)))
            .unwrap();
        b.extrude_no_merge("Sprocket", "Sprocket sketch", DEPTH)
            .unwrap();
        assert_clean(&b, &format!("{z}T sprocket"));

        let handle = b.solid_handle("Sprocket").unwrap();
        // One cylindrical wall per arc plus two caps.
        let faces = b.kernel_ref().as_introspect().list_faces(&handle);
        assert_eq!(faces.len(), 4 * z as usize + 2, "{z}T: face count");

        let v = exact_volume(&b, &handle);
        let expected = analytic_sprocket_area(&iso_08b(z)) * DEPTH;
        assert!(
            ((v - expected) / expected).abs() < 1e-9,
            "{z}T: exact volume {v} ≠ analytic {expected}"
        );
        // Sanity against the ISO dimensions: between the bottom and tip discs.
        let dims = sprocket_dimensions(&iso_08b(z)).unwrap();
        assert!(v > PI * (0.5 * dims.bottom_diameter).powi(2) * DEPTH);
        assert!(v < PI * (0.5 * dims.tip_diameter).powi(2) * DEPTH);

        let mesh = b.tessellate("Sprocket").unwrap();
        let wt = oracle::check_watertight_mesh(&mesh);
        assert!(wt.passed, "{z}T: {}", wt.detail);
        let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
        assert!(chi.passed, "{z}T: {}", chi.detail);
    }
}

/// A bore through the hub. `cap_offset` places the bore's sketch plane
/// below the sprocket and `depth` overshoots it, so the bore's caps are NOT
/// coplanar with the sprocket's (the general boolean path); `0.0` / `DEPTH`
/// makes them coplanar (the M8 Stage-0 path).
fn bore_through_sprocket(cap_offset: f64, depth: f64) -> ModelBuilder {
    let mut b = ModelBuilder::kernel_v2();
    b.add_operation("Sprocket sketch", sprocket_sketch(iso_08b(20)))
        .unwrap();
    b.extrude_no_merge("Sprocket", "Sprocket sketch", DEPTH)
        .unwrap();
    assert_clean(&b, "sprocket");
    b.true_circle_sketch(
        "Bore sketch",
        [0.0, 0.0, cap_offset],
        [0.0, 0.0, 1.0],
        0.0,
        0.0,
        BORE_R,
    )
    .unwrap();
    b.extrude_cut("Bore", "Bore sketch", depth).unwrap();
    b
}

const BORE_R: f64 = 0.012;

fn assert_bored_sprocket(b: &mut ModelBuilder, before_exact: f64) {
    assert_clean(b, "bore");
    // The exact integrator declines a cap loop that mixes a full-circle
    // hole with arcs (`signed_volume: loop mixes full circles with arcs`),
    // so the bored solid is measured on its tessellation: the un-bored
    // sprocket's mesh volume pins the chord error, and the bore's removal is
    // checked against the analytic cylinder within that error.
    let sprocket_mesh = b.tessellate("Sprocket").unwrap();
    let mesh_before = mesh_signed_volume(&sprocket_mesh).abs();
    let chord_rel = ((mesh_before - before_exact) / before_exact).abs();
    assert!(
        chord_rel < 5e-3,
        "sprocket mesh volume off by {chord_rel:e}"
    );
    let mesh = b.tessellate("Bore").unwrap();
    let mesh_after = mesh_signed_volume(&mesh).abs();
    let removed = PI * BORE_R * BORE_R * DEPTH;
    let got = mesh_before - mesh_after;
    assert!(
        ((got - removed) / removed).abs() < 1e-2,
        "bore removed {got} of {mesh_before}, expected {removed}"
    );
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "{}", wt.detail);
    // An annulus with teeth: genus 1, χ = 0.
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 0);
    assert!(chi.passed, "{}", chi.detail);
}

#[test]
fn sprocket_is_a_boolean_operand() {
    let before = analytic_sprocket_area(&iso_08b(20)) * DEPTH;
    let mut b = bore_through_sprocket(-0.001, DEPTH + 0.002);
    assert_bored_sprocket(&mut b, before);
}

/// The same bore with COPLANAR caps (the tool's caps on the sprocket's cap
/// planes) goes through yang's M8 Stage-0 mixed-loop overlay. It used to
/// STOP with `face N: holed lateral CDT failed: duplicate (coincident) loop
/// vertex`: both sprocket caps pair with the bore's caps, each cap's overlay
/// emits the same on-circle split points in its own frame, and
/// `collect_mixed_crossings` mirrored them onto the opposite arc by an f64
/// projection — three ULP-twin spellings of one point, kept apart by the
/// bit-exact dedup (strip chains 15 vs 16 → chart CDT → duplicate (u, v)).
/// Fixed by rim-override PROVENANCE (spec `m8_rim_override_provenance.md`):
/// a cap's own emission replaces a near-twin mirror, a mirror is absorbed by
/// a near own sample. This test is the end-to-end pin.
#[test]
fn sprocket_bore_with_coplanar_caps() {
    let before = analytic_sprocket_area(&iso_08b(20)) * DEPTH;
    let mut b = bore_through_sprocket(0.0, DEPTH);
    assert_bored_sprocket(&mut b, before);
}

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

#[test]
fn script_sprocket_matches_the_entity_route_exactly() {
    let mut b = ModelBuilder::kernel_v2();
    b.add_operation("Sprocket sketch", sprocket_sketch(iso_08b(16)))
        .unwrap();
    b.extrude_no_merge("Sprocket", "Sprocket sketch", DEPTH)
        .unwrap();
    assert_clean(&b, "entity route");
    let v_entity = exact_volume(&b, &b.solid_handle("Sprocket").unwrap());

    let src = Uuid::new_v4();
    b.state.engine.sources.insert_text(
        src,
        r#"
// @feature name="Sprocket" version=1
// @param teeth: int = 16 min=5
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.sprocket(#{ tooth_count: p.teeth, pitch: mm(12.7), roller_diameter: mm(8.51), center_x: 0.2 });
    ctx.extrude(sk.finish().regions()[0], #{ depth: mm(5) })
}
"#,
    );
    b.add_operation(
        "Script sprocket",
        script_op(
            src,
            json!({ "teeth": 16, "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] } }),
        ),
    )
    .unwrap();
    assert_clean(&b, "script route");
    let v_script = exact_volume(&b, &b.solid_handle("Script sprocket").unwrap());
    assert!(
        ((v_entity - v_script) / v_entity).abs() < 1e-12,
        "entity {v_entity} vs script {v_script}"
    );
}
