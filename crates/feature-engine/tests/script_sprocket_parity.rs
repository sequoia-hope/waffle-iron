//! A-M5 acceptance gate (`specs/custom_features_and_modeling_roadmap.md`
//! §A, milestone A-M5): the built-in ISO 606 sprocket generator's sketch
//! entities and positions must equal `sprocket.rhai`'s EXACTLY — same ids,
//! same coordinates bit for bit — over a matrix of `SprocketParams`, and
//! the script must refuse exactly the parameter sets the generator refuses.
//!
//! Two routes are pinned, as for the gear:
//! - the PORT: `sprocket_sketch` draws points/arcs through the sketch API;
//! - the ENTITY: `sk.sprocket(#{…})` records a `SketchEntity::Sprocket`
//!   that the engine expands with the same generator, which must match
//!   `generate_sprocket_profile` including its finished profile.

use std::collections::{BTreeMap, HashMap};

use feature_engine::script::{self, library::SPROCKET_RHAI};
use feature_engine::types::{Operation, ScriptParams};
use serde_json::json;
use uuid::Uuid;
use waffle_types::sketch::generated_entity_id_base;
use waffle_types::sprocket::{generate_sprocket_profile, SprocketParams};
use waffle_types::SketchEntity;

fn params_for(s: &SprocketParams) -> ScriptParams {
    let args: BTreeMap<String, serde_json::Value> = serde_json::from_value(json!({
        "tooth_count": s.tooth_count,
        "pitch": s.pitch,
        "roller_diameter": s.roller_diameter,
        "center_x": s.center_x,
        "center_y": s.center_y,
        "rotation_offset": s.rotation_offset,
        "seating_radius": s.seating_radius.unwrap_or(0.0),
        "flank_radius": s.flank_radius.unwrap_or(0.0),
        "tip_diameter": s.tip_diameter.unwrap_or(0.0),
        "seating_angle_deg": s.seating_angle_deg.unwrap_or(0.0),
        "face_width": 0.005,
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

type Recorded = (Vec<SketchEntity>, HashMap<u32, (f64, f64)>);

/// The recorded sketch child's entities and positions, or the script's error.
fn script_sketch(s: &SprocketParams) -> Result<Recorded, String> {
    let rec = script::record(SPROCKET_RHAI, &params_for(s)).map_err(|e| e.to_string())?;
    assert_eq!(rec.children.len(), 2, "sketch + extrude");
    let Operation::Sketch { sketch } = &rec.children[0].feature.operation else {
        panic!("first child is the sketch");
    };
    assert!(matches!(
        rec.children[1].feature.operation,
        Operation::Extrude { .. }
    ));
    Ok((sketch.entities.clone(), sketch.solved_positions.clone()))
}

fn entity_json(e: &SketchEntity) -> serde_json::Value {
    serde_json::to_value(e).unwrap()
}

/// Bitwise-exact comparison of two entity lists and position maps (entity
/// points only: the finish-profile builder also mints synthetic arc-sample
/// points, which are profile machinery, not sketch entities).
fn assert_identical(
    label: &str,
    (ea, pa): (&[SketchEntity], &HashMap<u32, (f64, f64)>),
    (eb, pb): (&[SketchEntity], &HashMap<u32, (f64, f64)>),
) {
    assert_eq!(ea.len(), eb.len(), "{label}: entity count");
    for (i, (a, b)) in ea.iter().zip(eb).enumerate() {
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
    let point_ids: std::collections::BTreeSet<u32> = ea
        .iter()
        .filter(|e| matches!(e, SketchEntity::Point { .. }))
        .map(|e| e.id())
        .collect();
    let only = |m: &HashMap<u32, (f64, f64)>| -> HashMap<u32, (f64, f64)> {
        m.iter()
            .filter(|(id, _)| point_ids.contains(id))
            .map(|(k, v)| (*k, *v))
            .collect()
    };
    let (pa, pb) = (only(pa), only(pb));
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

/// ISO 606 / ANSI chains (pitch, roller diameter) in metres.
const CHAINS: [(f64, f64); 4] = [
    (0.0127, 0.00851),   // ISO 08B
    (0.009525, 0.00635), // ISO 06B
    (0.0254, 0.01588),   // ISO 16B
    (0.00635, 0.0033),   // ANSI 25
];

fn matrix() -> Vec<SprocketParams> {
    let mut out = Vec::new();
    for &tooth_count in &[5u32, 7, 9, 11, 16, 20, 34, 52, 120] {
        for &(pitch, roller_diameter) in &CHAINS {
            out.push(SprocketParams {
                tooth_count,
                pitch,
                roller_diameter,
                ..SprocketParams::default()
            });
        }
    }
    // Offsets and rotation exercise the placement path.
    out.push(SprocketParams {
        tooth_count: 24,
        center_x: 0.0371,
        center_y: -0.0123,
        rotation_offset: 0.4321,
        ..SprocketParams::default()
    });
    out.push(SprocketParams {
        tooth_count: 13,
        pitch: 0.01905,
        roller_diameter: 0.01207,
        center_x: -1.5,
        center_y: 2.25,
        rotation_offset: -2.9,
        ..SprocketParams::default()
    });
    // Explicit ISO overrides (in range).
    out.push(SprocketParams {
        tooth_count: 17,
        seating_radius: Some(0.0043),
        flank_radius: Some(0.02),
        tip_diameter: Some(0.078),
        seating_angle_deg: Some(126.0),
        ..SprocketParams::default()
    });
    // A tip diameter far too large for the tooth count: pointed teeth, refused.
    out.push(SprocketParams {
        tooth_count: 9,
        tip_diameter: Some(0.09),
        ..SprocketParams::default()
    });
    // A seat smaller than the roller: refused.
    out.push(SprocketParams {
        tooth_count: 20,
        seating_radius: Some(0.004),
        ..SprocketParams::default()
    });
    out
}

#[test]
fn sprocket_script_port_matches_the_builtin_generator_bit_for_bit() {
    let m = matrix();
    assert!(m.len() > 30);
    let mut accepted = 0;
    let mut refused = 0;
    for s in &m {
        match (generate_sprocket_profile(s), script_sketch(s)) {
            (Ok(expect), Ok((entities, positions))) => {
                accepted += 1;
                assert_identical(
                    &format!("{s:?}"),
                    (&entities, &positions),
                    (&expect.entities, &expect.positions),
                );
                // Every id is unique and the counter ran 1..=N with no gaps.
                let mut ids: Vec<u32> = entities.iter().map(|e| e.id()).collect();
                ids.sort_unstable();
                assert_eq!(ids, (1..=entities.len() as u32).collect::<Vec<_>>());
                // 1 centre + 7 points per gap + 4 arcs per gap.
                assert_eq!(entities.len(), 1 + 11 * s.tooth_count as usize);
            }
            (Err(e), Err(msg)) => {
                refused += 1;
                assert!(
                    msg.contains("sprocket:"),
                    "{s:?}: script refusal should name the sprocket: {msg} (generator: {e})"
                );
            }
            (Ok(_), Err(msg)) => panic!("{s:?}: generator accepted, script refused: {msg}"),
            (Err(e), Ok(_)) => panic!("{s:?}: generator refused ({e}), script accepted"),
        }
    }
    assert!(accepted >= 30, "accepted {accepted}");
    assert!(refused >= 2, "refused {refused}");
}

#[test]
fn sprocket_entity_route_matches_the_generator_and_its_profile() {
    let s = SprocketParams {
        tooth_count: 21,
        pitch: 0.015875,
        roller_diameter: 0.01016,
        center_x: 0.01,
        center_y: -0.02,
        rotation_offset: 0.25,
        ..SprocketParams::default()
    };
    let text = r#"
// @feature name="Sprocket entity" version=1
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.sprocket(#{
        tooth_count: 21, pitch: 0.015875, roller_diameter: 0.01016,
        center_x: 0.01, center_y: -0.02, rotation_offset: 0.25
    });
    let regions = sk.finish().regions();
    ctx.extrude(regions[0], #{ depth: 0.005 })
}
"#;
    let mut params = params_for(&s);
    params.args = serde_json::from_value(json!({
        "plane": { "origin": [0, 0, 0], "normal": [0, 0, 1] },
    }))
    .unwrap();
    let rec = script::record(text, &params).unwrap();
    let Operation::Sketch { sketch } = &rec.children[0].feature.operation else {
        panic!()
    };
    // The engine expands a sprocket into the entity's own id range
    // (`generated_entity_id_base`; the compact entity was the sketch's first
    // allocation, id 1), so the generator's result is shifted there before
    // the exact comparison.
    let base = generated_entity_id_base(1);
    let expect = generate_sprocket_profile(&s).unwrap();
    let expect_entities: Vec<SketchEntity> = expect
        .entities
        .iter()
        .map(|e| e.with_ids_offset(base))
        .collect();
    let expect_positions: HashMap<u32, (f64, f64)> = expect
        .positions
        .iter()
        .map(|(k, v)| (base + k, *v))
        .collect();
    assert_identical(
        "entity route",
        (&sketch.entities, &sketch.solved_positions),
        (&expect_entities, &expect_positions),
    );
    assert_eq!(sketch.solved_profiles.len(), 1);
    assert_eq!(
        serde_json::to_value(&sketch.solved_profiles[0]).unwrap(),
        serde_json::to_value(expect.profiles[0].with_ids_offset(base)).unwrap()
    );
}

#[test]
fn sprocket_script_header_declares_the_generator_parameters() {
    let iface = script::header::parse_header(SPROCKET_RHAI).unwrap();
    assert_eq!(iface.name, "Roller-chain sprocket");
    for name in [
        "tooth_count",
        "pitch",
        "roller_diameter",
        "center_x",
        "center_y",
        "rotation_offset",
        "seating_radius",
        "flank_radius",
        "tip_diameter",
        "seating_angle_deg",
        "face_width",
        "plane",
    ] {
        assert!(iface.param(name).is_some(), "missing @param {name}");
    }
}
