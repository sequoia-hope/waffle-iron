//! A-M2 acceptance gate (`specs/custom_features_and_modeling_roadmap.md`
//! §A9.1): the built-in involute generator's sketch entities and positions
//! must equal `gear.rhai`'s EXACTLY — same ids, same coordinates bit for
//! bit — over a matrix of `GearParams`. If the script API cannot reproduce
//! the generator, the API is not ready.
//!
//! Two routes are pinned:
//! - the PORT: `gear_sketch` draws points/lines/arcs/splines through the
//!   sketch API (external gears);
//! - the ENTITY: `sk.gear(#{…})` records a `SketchEntity::Gear` that the
//!   engine expands with the same generator (internal gears take this
//!   route in the script), which must match `Sketch::expand_gears`.

use std::collections::{BTreeMap, HashMap};

use feature_engine::script::{self, library::GEAR_RHAI};
use feature_engine::types::{Operation, ScriptParams};
use serde_json::json;
use uuid::Uuid;
use waffle_types::gear::{generate_gear_profile, GearParams};
use waffle_types::SketchEntity;

fn params_for(g: &GearParams) -> ScriptParams {
    let args: BTreeMap<String, serde_json::Value> = serde_json::from_value(json!({
        "tooth_count": g.tooth_count,
        "module_m": g.module,
        "pressure_angle_deg": g.pressure_angle_deg,
        "backlash": g.backlash,
        "center_x": g.center_x,
        "center_y": g.center_y,
        "rotation_offset": g.rotation_offset,
        "internal": g.internal,
        "face_width": 0.01,
        "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] },
    }))
    .unwrap();
    ScriptParams {
        source_id: Uuid::new_v4(),
        entry: "feature".into(),
        args,
        arg_exprs: BTreeMap::new(),
        arg_values: BTreeMap::new(),
    }
}

/// The recorded sketch child's entities and positions.
fn script_sketch(g: &GearParams) -> (Vec<SketchEntity>, HashMap<u32, (f64, f64)>) {
    let rec = script::record(GEAR_RHAI, &params_for(g))
        .unwrap_or_else(|e| panic!("gear.rhai failed for {g:?}: {e}"));
    assert_eq!(rec.children.len(), 2, "sketch + extrude");
    let Operation::Sketch { sketch } = &rec.children[0].feature.operation else {
        panic!("first child is the sketch");
    };
    assert!(matches!(
        rec.children[1].feature.operation,
        Operation::Extrude { .. }
    ));
    (sketch.entities.clone(), sketch.solved_positions.clone())
}

fn entity_json(e: &SketchEntity) -> serde_json::Value {
    serde_json::to_value(e).unwrap()
}

/// Bitwise-exact comparison of two entity lists and position maps.
fn assert_identical(
    label: &str,
    (ea, pa): (&[SketchEntity], &HashMap<u32, (f64, f64)>),
    (eb, pb): (&[SketchEntity], &HashMap<u32, (f64, f64)>),
) {
    assert_eq!(ea.len(), eb.len(), "{label}: entity count");
    for (i, (a, b)) in ea.iter().zip(eb).enumerate() {
        // Serialize to compare floats exactly (JSON of an f64 round-trips).
        let (ja, jb) = (entity_json(a), entity_json(b));
        assert_eq!(ja, jb, "{label}: entity {i} differs");
        if let (
            SketchEntity::Point { x: xa, y: ya, .. },
            SketchEntity::Point { x: xb, y: yb, .. },
        ) = (a, b)
        {
            assert_eq!(xa.to_bits(), xb.to_bits(), "{label}: entity {i} x bits");
            assert_eq!(ya.to_bits(), yb.to_bits(), "{label}: entity {i} y bits");
        }
    }
    // Positions of the ENTITY points only: the finish-profile builder also
    // mints synthetic arc-sample points (ids ≥ 900 000) into the map, which
    // are profile machinery, not sketch entities.
    let point_ids: std::collections::BTreeSet<u32> = ea
        .iter()
        .filter(|e| matches!(e, SketchEntity::Point { .. }))
        .map(|e| e.id())
        .collect();
    let pa: HashMap<u32, (f64, f64)> = pa
        .iter()
        .filter(|(id, _)| point_ids.contains(id))
        .map(|(k, v)| (*k, *v))
        .collect();
    let pb: HashMap<u32, (f64, f64)> = pb
        .iter()
        .filter(|(id, _)| point_ids.contains(id))
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(pa.len(), pb.len(), "{label}: position count");
    assert_eq!(
        pa.len(),
        point_ids.len(),
        "{label}: every point has a position"
    );
    for (id, (xa, ya)) in &pa {
        let (xb, yb) = pb
            .get(id)
            .unwrap_or_else(|| panic!("{label}: position {id} missing"));
        assert_eq!(xa.to_bits(), xb.to_bits(), "{label}: position {id} x");
        assert_eq!(ya.to_bits(), yb.to_bits(), "{label}: position {id} y");
    }
}

fn matrix() -> Vec<GearParams> {
    let mut out = Vec::new();
    for &tooth_count in &[8u32, 12, 20, 37, 60, 101] {
        for &module in &[0.001, 0.002, 0.005] {
            for &pressure_angle_deg in &[14.5, 20.0, 25.0] {
                out.push(GearParams {
                    tooth_count,
                    module,
                    pressure_angle_deg,
                    ..GearParams::default()
                });
            }
        }
    }
    // Offsets, rotation and backlash exercise the transform path.
    out.push(GearParams {
        tooth_count: 24,
        module: 0.0015,
        pressure_angle_deg: 20.0,
        backlash: 1e-4,
        center_x: 0.0371,
        center_y: -0.0123,
        rotation_offset: 0.4321,
        internal: false,
    });
    out.push(GearParams {
        tooth_count: 13,
        module: 0.003,
        pressure_angle_deg: 22.5,
        backlash: 2.5e-5,
        center_x: -1.5,
        center_y: 2.25,
        rotation_offset: -2.9,
        internal: false,
    });
    out
}

#[test]
fn gear_script_port_matches_the_builtin_generator_bit_for_bit() {
    let m = matrix();
    assert!(m.len() > 50);
    for g in &m {
        let expect = generate_gear_profile(g);
        let (entities, positions) = script_sketch(g);
        assert_identical(
            &format!("{g:?}"),
            (&entities, &positions),
            (&expect.entities, &expect.positions),
        );
        // Every id is unique and the counter ran 1..=N with no gaps.
        let mut ids: Vec<u32> = entities.iter().map(|e| e.id()).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1..=entities.len() as u32).collect::<Vec<_>>());
    }
}

#[test]
fn gear_entity_route_matches_expand_gears() {
    for internal in [false, true] {
        let g = GearParams {
            tooth_count: 30,
            module: 0.002,
            pressure_angle_deg: 20.0,
            internal,
            ..GearParams::default()
        };
        let text = r#"
// @feature name="Gear entity" version=1
// @param plane: plane
// @param internal: bool = false
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.gear(#{ tooth_count: 30, module_m: 0.002, pressure_angle_deg: 20.0, internal: p.internal });
    let regions = sk.finish().regions();
    ctx.extrude(regions[0], #{ depth: 0.01 })
}
"#;
        let mut params = params_for(&g);
        params.args = serde_json::from_value(json!({
            "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] },
            "internal": internal
        }))
        .unwrap();
        let rec = script::record(text, &params).unwrap();
        let Operation::Sketch { sketch } = &rec.children[0].feature.operation else {
            panic!()
        };
        let expect = generate_gear_profile(&g);
        assert_identical(
            &format!("entity route internal={internal}"),
            (&sketch.entities, &sketch.solved_positions),
            (&expect.entities, &expect.positions),
        );
        // The entity route keeps the generator's own profile (with its
        // spline control points), exactly as `expand_gears` does.
        assert_eq!(sketch.solved_profiles.len(), 1);
        assert_eq!(
            serde_json::to_value(&sketch.solved_profiles[0]).unwrap(),
            serde_json::to_value(&expect.profiles[0]).unwrap()
        );
    }
    // The shipped script takes this route for internal gears.
    let g = GearParams {
        tooth_count: 40,
        module: 0.002,
        internal: true,
        ..GearParams::default()
    };
    let (entities, positions) = script_sketch(&g);
    let expect = generate_gear_profile(&g);
    assert_identical(
        "gear.rhai internal",
        (&entities, &positions),
        (&expect.entities, &expect.positions),
    );
}

#[test]
fn gear_script_header_declares_the_generator_parameters() {
    let iface = script::header::parse_header(GEAR_RHAI).unwrap();
    assert_eq!(iface.name, "Spur gear");
    for name in [
        "tooth_count",
        "module_m",
        "pressure_angle_deg",
        "backlash",
        "center_x",
        "center_y",
        "rotation_offset",
        "internal",
        "face_width",
        "plane",
    ] {
        assert!(iface.param(name).is_some(), "missing @param {name}");
    }
}
