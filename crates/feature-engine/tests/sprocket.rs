//! The `Sprocket` sketch entity (spec
//! `specs/custom_features_and_modeling_roadmap.md` §B3) through the engine
//! on MockKernel: a compact sprocket expands at rebuild into an arc profile
//! the extrude consumes, plain entities drawn beside it keep their own
//! loops, bad parameters are a typed error, and the script API's
//! `sk.sprocket` reaches the same generator. Real-geometry oracles
//! (watertightness, exact volume, boolean operand) live in
//! `crates/test-harness/tests/sprocket_kv2.rs`.

use std::collections::{BTreeMap, HashMap};

use feature_engine::types::*;
use feature_engine::Engine;
use serde_json::json;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

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

/// A sketch carrying only compact / raw entities: no solved data, the way
/// an agent's `sketch_create` or a hand-written file delivers it.
fn sketch_of(entities: Vec<SketchEntity>) -> Sketch {
    Sketch {
        id: Uuid::new_v4(),
        plane: plane_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities,
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: vec![],
    }
}

fn extrude(sketch_id: Uuid, profile_index: usize) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: Some(CombineMode::NewBody),
            targets: None,
            sketch_id,
            profile_index,
            profile_entity_ids: None,
            depth: 0.005,
            direction: None,
            symmetric: false,
            cut: false,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            depth_expr: None,
        },
    }
}

fn sprocket_entity(id: u32, params: SprocketParams) -> SketchEntity {
    SketchEntity::Sprocket {
        id,
        params,
        construction: false,
    }
}

#[test]
fn compact_sprocket_expands_at_rebuild_into_an_arc_profile() {
    let mut sketch = sketch_of(vec![sprocket_entity(7, iso_08b(20))]);
    sketch.recompute_derived_checked().unwrap();

    // The compact entity is gone; points and arcs stand in its place, every
    // id inside the entity's own range.
    assert!(!sketch.entities.iter().any(SketchEntity::is_generator));
    let base = generated_entity_id_base(7);
    assert!(sketch
        .entities
        .iter()
        .all(|e| e.id() > base && e.id() < base + 100_000));
    assert_eq!(
        sketch
            .entities
            .iter()
            .filter(|e| matches!(e, SketchEntity::Arc { .. }))
            .count(),
        80
    );
    // One kernel-ready profile: 80 exact arcs over sampled vertices, all of
    // whose positions are known.
    assert_eq!(sketch.solved_profiles.len(), 1);
    let p = &sketch.solved_profiles[0];
    assert!(p.is_outer);
    assert_eq!(p.arc_segments.len(), 80);
    assert_eq!(p.entity_ids.len(), 80);
    assert!(p.vertex_ids.len() > 80 * 15);
    for id in &p.vertex_ids {
        assert!(
            sketch.solved_positions.contains_key(id),
            "vertex {id} has no position"
        );
        assert!(*id > base);
    }
    for id in &p.entity_ids {
        assert!(sketch.entities.iter().any(|e| e.id() == *id));
    }

    // Deterministic: a second derivation is identical.
    let mut again = sketch_of(vec![sprocket_entity(7, iso_08b(20))]);
    again.recompute_derived_checked().unwrap();
    assert_eq!(
        serde_json::to_string(&again.solved_profiles).unwrap(),
        serde_json::to_string(&sketch.solved_profiles).unwrap()
    );
}

#[test]
fn plain_entities_beside_a_sprocket_keep_their_own_loop() {
    // A bore: centre point + circle, ids that a naive expansion (minted
    // from 1) would shadow.
    let mut sketch = sketch_of(vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Circle {
            id: 2,
            center_id: 1,
            radius: 0.01,
            construction: false,
        },
        sprocket_entity(3, iso_08b(20)),
    ]);
    sketch.recompute_derived_checked().unwrap();
    assert_eq!(
        sketch.solved_profiles.len(),
        2,
        "{:?}",
        sketch.solved_profiles
    );
    assert_eq!(sketch.solved_profiles[0].arc_segments.len(), 80);
    assert_eq!(sketch.solved_profiles[1].entity_ids, vec![2]);
    // The bore's centre position survived the expansion.
    assert_eq!(sketch.solved_positions.get(&1), Some(&(0.0, 0.0)));
}

#[test]
fn sprocket_sketch_extrudes_and_the_tree_keeps_the_compact_entity() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sketch = sketch_of(vec![sprocket_entity(1, iso_08b(11))]);
    let sk = engine
        .add_feature("sk".into(), Operation::Sketch { sketch }, &mut kernel)
        .unwrap();
    let ex = engine
        .add_feature("ex".into(), extrude(sk, 0), &mut kernel)
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(engine.feature_results.contains_key(&ex));
    assert!(!engine.feature_results[&ex].outputs.is_empty());

    // The stored sketch is still the compact one: expansion is a rebuild-time
    // derivation, never written back.
    let f = engine.tree.features.iter().find(|f| f.id == sk).unwrap();
    let Operation::Sketch { sketch } = &f.operation else {
        panic!()
    };
    assert_eq!(sketch.entities.len(), 1);
    assert!(sketch.entities[0].is_generator());
}

#[test]
fn a_sprocket_that_cannot_expand_is_a_typed_extrude_error() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sketch = sketch_of(vec![sprocket_entity(1, iso_08b(3))]);
    let sk = engine
        .add_feature("sk".into(), Operation::Sketch { sketch }, &mut kernel)
        .unwrap();
    let ex = engine
        .add_feature("ex".into(), extrude(sk, 0), &mut kernel)
        .unwrap();
    let errors = &engine.feature_errors;
    let err = errors
        .iter()
        .find(|e| e.feature_id == ex)
        .expect("the extrude fails");
    assert_eq!(err.kind, ErrorKind::InvalidParameter);
    assert!(
        err.message.contains("tooth_count 3"),
        "the message names the value: {}",
        err.message
    );
    assert!(!engine.feature_results.contains_key(&ex));
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

const SPROCKET_SCRIPT: &str = r#"
// @feature name="Sprocket" version=1
// @param teeth: int = 20 min=5
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.sprocket(#{ tooth_count: p.teeth, pitch: mm(12.7), roller_diameter: mm(8.51) });
    let r = sk.finish().regions();
    ctx.log("regions: " + r.len());
    ctx.extrude(r[0], #{ depth: mm(5) })
}
"#;

#[test]
fn script_sprocket_builds_a_body() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let src = Uuid::new_v4();
    engine.sources.insert_text(src, SPROCKET_SCRIPT);
    let id = engine
        .add_feature(
            "script".into(),
            script_op(
                src,
                json!({ "teeth": 17, "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] } }),
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(!engine.feature_results[&id].outputs.is_empty());
}

#[test]
fn script_sprocket_with_bad_parameters_fails_on_its_own_line() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let src = Uuid::new_v4();
    engine.sources.insert_text(
        src,
        r#"
// @feature name="Bad sprocket" version=1
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    sk.sprocket(#{ tooth_count: 20, pitch: mm(12.7), roller_diameter: mm(8.51), seating_radius: mm(1) });
    ctx.extrude(sk.finish().regions()[0], #{ depth: mm(5) })
}
"#,
    );
    let id = engine
        .add_feature(
            "script".into(),
            script_op(
                src,
                json!({ "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] } }),
            ),
            &mut kernel,
        )
        .unwrap();
    let errors = &engine.feature_errors;
    let err = errors.iter().find(|e| e.feature_id == id).expect("fails");
    assert_eq!(
        err.kind,
        ErrorKind::Script {
            stage: "runtime".into()
        }
    );
    assert!(
        err.message.contains("seating radius") && err.message.contains("roller radius"),
        "{}",
        err.message
    );
    assert!(!engine.feature_results.contains_key(&id));
}

#[test]
fn sprocket_entity_round_trips_through_json() {
    let e = sprocket_entity(4, iso_08b(9));
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains(r#""type":"Sprocket""#));
    assert!(json.contains(r#""toothCount":9"#));
    let back: SketchEntity = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id(), 4);
    assert!(back.is_generator());
}
