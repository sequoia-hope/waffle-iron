//! `Operation::Pipe` (spec `specs/b2_pipe_sweep.md` checkpoint 2) through
//! the engine on MockKernel: the sketch's open line/arc chain is
//! re-extracted at rebuild and swept, caps carry the end-cap roles, every
//! malformed path or radius is a typed per-feature error with no output,
//! expressions drive both radii, the operation round-trips, and the script
//! API's `ctx.pipe` reaches the same operation. Real-geometry oracles (exact
//! volume, watertightness, boolean combine) live in
//! `crates/test-harness/tests/pipe_kv2.rs`.

use std::collections::{BTreeMap, HashMap};

use feature_engine::types::*;
use feature_engine::Engine;
use serde_json::json;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

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

fn pt(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}
fn line(id: u32, s: u32, e: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: s,
        end_id: e,
        construction: true,
    }
}
fn arc(id: u32, c: u32, s: u32, e: u32) -> SketchEntity {
    SketchEntity::Arc {
        id,
        center_id: c,
        start_id: s,
        end_id: e,
        construction: true,
    }
}

/// Handlebar: line (−1,0)→(0,0), CCW quarter about (0,0.3) to (0.3,0.3),
/// line to (0.3,1.3). Entity ids 10, 11, 12.
fn handlebar() -> Vec<SketchEntity> {
    vec![
        pt(1, -1.0, 0.0),
        pt(2, 0.0, 0.0),
        pt(3, 0.0, 0.3),
        pt(4, 0.3, 0.3),
        pt(5, 0.3, 1.3),
        line(10, 1, 2),
        arc(11, 3, 2, 4),
        line(12, 4, 5),
    ]
}

fn pipe_op(sketch_id: Uuid, ids: &[u32], radius: f64, inner: Option<f64>) -> Operation {
    Operation::Pipe {
        params: PipeParams {
            sketch_id,
            entity_ids: ids.to_vec(),
            radius,
            radius_expr: None,
            inner_radius: inner,
            inner_radius_expr: None,
            combine: None,
            targets: None,
        },
    }
}

#[test]
fn handlebar_pipe_builds_a_body_with_cap_roles() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sk = engine
        .add_feature(
            "path".into(),
            Operation::Sketch {
                sketch: sketch_of(handlebar()),
            },
            &mut kernel,
        )
        .unwrap();
    // Picked out of order: the chain is ordered at rebuild.
    let p = engine
        .add_feature(
            "pipe".into(),
            pipe_op(sk, &[12, 10, 11], 0.05, None),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let result = &engine.feature_results[&p];
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].0, OutputKey::Main);
    let roles: Vec<&Role> = result
        .provenance
        .role_assignments
        .iter()
        .map(|(_, r)| r)
        .collect();
    assert_eq!(
        roles
            .iter()
            .filter(|r| ***r == Role::EndCapNegative)
            .count(),
        1,
        "{roles:?}"
    );
    assert_eq!(
        roles
            .iter()
            .filter(|r| ***r == Role::EndCapPositive)
            .count(),
        1,
        "{roles:?}"
    );
    assert!(roles.iter().any(|r| matches!(r, Role::SideFace { .. })));
    // The stored sketch is untouched; the open chain is never written back.
    let f = engine.tree.features.iter().find(|f| f.id == sk).unwrap();
    let Operation::Sketch { sketch } = &f.operation else {
        panic!()
    };
    assert_eq!(sketch.entities.len(), 8);
}

#[test]
fn hollow_pipe_and_radius_edits_regenerate() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sk = engine
        .add_feature(
            "path".into(),
            Operation::Sketch {
                sketch: sketch_of(handlebar()),
            },
            &mut kernel,
        )
        .unwrap();
    let p = engine
        .add_feature(
            "pipe".into(),
            pipe_op(sk, &[10, 11, 12], 0.05, Some(0.03)),
            &mut kernel,
        )
        .unwrap();
    assert!(
        engine.feature_errors.is_empty(),
        "{:?}",
        engine.feature_errors
    );
    assert!(engine.feature_results.contains_key(&p));
    // Edit to a bad bore: the error is typed, the previous output is gone.
    engine
        .edit_feature(p, pipe_op(sk, &[10, 11, 12], 0.05, Some(0.05)), &mut kernel)
        .unwrap();
    let err = engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == p)
        .expect("bore ≥ tube is refused");
    assert_eq!(err.kind, ErrorKind::InvalidParameter, "{}", err.message);
    assert!(!engine.feature_results.contains_key(&p));
}

#[test]
fn malformed_paths_are_typed_errors_naming_the_sketch_point() {
    let mut corner = handlebar();
    corner.push(pt(6, 0.0, 1.0));
    corner.push(line(13, 2, 6)); // perpendicular at point 2
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sk = engine
        .add_feature(
            "path".into(),
            Operation::Sketch {
                sketch: sketch_of(corner),
            },
            &mut kernel,
        )
        .unwrap();
    for (ids, needle) in [
        (vec![10u32, 13], "not tangent"),
        (vec![10, 11, 13], "more than two entities meet at point 2"),
        (vec![10, 12], "connected"),
        (vec![99], "not in the sketch"),
        (vec![], "no entities"),
    ] {
        let p = engine
            .add_feature("pipe".into(), pipe_op(sk, &ids, 0.05, None), &mut kernel)
            .unwrap();
        let err = engine
            .feature_errors
            .iter()
            .find(|e| e.feature_id == p)
            .unwrap_or_else(|| panic!("{ids:?}: expected a refusal"));
        assert_eq!(
            err.kind,
            ErrorKind::InvalidParameter,
            "{ids:?}: {}",
            err.message
        );
        assert!(
            err.message.contains(needle),
            "{ids:?}: message {:?} lacks {needle:?}",
            err.message
        );
        assert!(!engine.feature_results.contains_key(&p));
        engine.remove_feature(p, &mut kernel).unwrap();
    }
    // A bend tighter than the tube.
    let p = engine
        .add_feature(
            "pipe".into(),
            pipe_op(sk, &[10, 11], 0.3, None),
            &mut kernel,
        )
        .unwrap();
    let err = engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == p)
        .expect("bend ≤ tube radius is refused");
    assert!(err.message.contains("tighter"), "{}", err.message);
}

#[test]
fn expressions_drive_both_radii() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sk = engine
        .add_feature(
            "path".into(),
            Operation::Sketch {
                sketch: sketch_of(handlebar()),
            },
            &mut kernel,
        )
        .unwrap();
    let mut op = pipe_op(sk, &[10, 11, 12], 0.05, None);
    if let Operation::Pipe { params } = &mut op {
        params.radius_expr = Some("od / 2".into());
        params.inner_radius_expr = Some("od / 2 - wall".into());
    }
    let p = engine.add_feature("pipe".into(), op, &mut kernel).unwrap();
    engine.set_parameters(
        vec![
            DesignParameter::new("od", "40"),
            DesignParameter::new("wall", "4"),
        ],
        &mut kernel,
    );
    assert!(
        engine.feature_errors.is_empty(),
        "{:?}",
        engine.feature_errors
    );
    let f = engine.tree.features.iter().find(|f| f.id == p).unwrap();
    let Operation::Pipe { params } = &f.operation else {
        panic!()
    };
    // mm-space expressions land in meters.
    assert!((params.radius - 0.020).abs() < 1e-12, "{}", params.radius);
    assert!(
        (params.inner_radius.unwrap() - 0.016).abs() < 1e-12,
        "{:?}",
        params.inner_radius
    );
}

#[test]
fn operation_tag_and_json_round_trip() {
    assert!(OPERATION_TAGS.contains(&"Pipe"));
    let sk = Uuid::new_v4();
    let op = pipe_op(sk, &[10, 11, 12], 0.05, Some(0.03));
    let v = serde_json::to_value(&op).unwrap();
    assert_eq!(v["type"], "Pipe");
    assert_eq!(v["params"]["entity_ids"], json!([10, 11, 12]));
    let back: Operation = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(back.type_tag(), "Pipe");
    assert_eq!(serde_json::to_value(&back).unwrap(), v);
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

const PIPE_SCRIPT: &str = r#"
// @feature name="Handlebar" version=1
// @param od: length = 0.04
// @param plane: plane
fn feature(ctx, p) {
    let sk = ctx.sketch(p.plane);
    let a = sk.point(-1.0, 0.0);
    let b = sk.point(0.0, 0.0);
    let c = sk.point(0.0, 0.3);
    let d = sk.point(0.3, 0.3);
    let e = sk.point(0.3, 1.3);
    let l1 = sk.line(a, b, #{ construction: true });
    let bend = sk.arc(c, b, d);
    let l2 = sk.line(d, e, #{ construction: true });
    let s = sk.finish();
    ctx.pipe(s, [l1, bend, l2], #{ radius: p.od / 2, inner_radius: p.od / 2 - mm(4) })
}
"#;

#[test]
fn script_pipe_builds_a_body() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let src = Uuid::new_v4();
    engine.sources.insert_text(src, PIPE_SCRIPT);
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
    assert!(
        engine.feature_errors.is_empty(),
        "{:?}",
        engine.feature_errors
    );
    let result = &engine.feature_results[&id];
    assert_eq!(result.outputs.len(), 1, "one pipe body");
}
