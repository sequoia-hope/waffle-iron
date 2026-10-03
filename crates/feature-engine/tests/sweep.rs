//! `Operation::Sweep` (spec `specs/b6_general_sweep.md` S6) through the
//! engine on MockKernel: a planar sketch profile is the section, the path
//! comes from a planar sketch (open with a corner, or closed) or from a
//! `Sketch3d` chain, an explicit sub-region can be the section, the combine
//! dispatch consumes its target, a curved section is a typed capability
//! wall (B6 S4) rather than a chord approximation, the operation
//! round-trips, and the script API's `ctx.sweep` reaches the same
//! operation. Real-geometry oracles belong to the kernel-v2 sweep tests and
//! the assay.

use std::collections::{BTreeMap, HashMap};

use feature_engine::types::*;
use feature_engine::Engine;
use serde_json::json;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::sketch3d::{Sketch3d, Sketch3dEntity};
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

fn sketch_on(normal: [f64; 3], entities: Vec<SketchEntity>) -> Sketch {
    Sketch {
        id: Uuid::new_v4(),
        plane: plane_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: normal,
        plane_x_axis: None,
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
fn line(id: u32, s: u32, e: u32, construction: bool) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: s,
        end_id: e,
        construction,
    }
}

/// The SECTION: a 0.2 m square centred on the origin, drawn on the plane
/// of normal +x (so a path leaving the origin along +x pierces it). Points
/// 1–4, lines 5–8.
fn square_section() -> Sketch {
    sketch_on(
        [1.0, 0.0, 0.0],
        vec![
            pt(1, -0.1, -0.1),
            pt(2, 0.1, -0.1),
            pt(3, 0.1, 0.1),
            pt(4, -0.1, 0.1),
            line(5, 1, 2, false),
            line(6, 2, 3, false),
            line(7, 3, 4, false),
            line(8, 4, 1, false),
        ],
    )
}

/// A path sketch on z = 0: an open L (0,0)→(1,0)→(1,1) as construction
/// lines 10, 11 and, sharing its first corner, a closed unit square ring of
/// lines 10, 11, 12, 13.
fn path_sketch() -> Sketch {
    sketch_on(
        [0.0, 0.0, 1.0],
        vec![
            pt(1, 0.0, 0.0),
            pt(2, 1.0, 0.0),
            pt(3, 1.0, 1.0),
            pt(4, 0.0, 1.0),
            line(10, 1, 2, true),
            line(11, 2, 3, true),
            line(12, 3, 4, true),
            line(13, 4, 1, true),
        ],
    )
}

fn sweep_params(section: Uuid, path: SweepPathRef) -> SweepParams {
    SweepParams {
        sketch_id: section,
        profile_index: 0,
        profile_entity_ids: None,
        region: None,
        path,
        combine: None,
        targets: None,
    }
}

fn sweep_op(section: Uuid, path: SweepPathRef) -> Operation {
    Operation::Sweep {
        params: sweep_params(section, path),
    }
}

fn add_sketch(engine: &mut Engine, kernel: &mut MockKernel, sketch: Sketch) -> Uuid {
    engine
        .add_feature("sketch".into(), Operation::Sketch { sketch }, kernel)
        .unwrap()
}

fn roles_of(result: &modeling_ops::OpResult) -> Vec<Role> {
    result
        .provenance
        .role_assignments
        .iter()
        .map(|(_, r)| r.clone())
        .collect()
}

fn feature_error(engine: &Engine, id: Uuid) -> &FeatureError {
    engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == id)
        .unwrap_or_else(|| panic!("feature {id} was expected to fail: {:?}", engine.errors))
}

#[test]
fn open_planar_path_with_a_corner_builds_a_capped_body() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let section = add_sketch(&mut engine, &mut kernel, square_section());
    let path = add_sketch(&mut engine, &mut kernel, path_sketch());
    // Picked out of order: the chain is ordered at rebuild, and the corner
    // at (1,0) is allowed (the kernel mitres it).
    let sw = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch {
                    sketch_id: path,
                    entity_ids: vec![11, 10],
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let result = &engine.feature_results[&sw];
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs[0].0, OutputKey::Main);
    let roles = roles_of(result);
    assert_eq!(
        roles.iter().filter(|r| **r == Role::EndCapNegative).count(),
        1,
        "{roles:?}"
    );
    assert_eq!(
        roles.iter().filter(|r| **r == Role::EndCapPositive).count(),
        1,
        "{roles:?}"
    );
    assert!(roles.iter().any(|r| matches!(r, Role::SideFace { .. })));
    // Neither sketch is written back.
    for id in [section, path] {
        let f = engine.tree.features.iter().find(|f| f.id == id).unwrap();
        let Operation::Sketch { sketch } = &f.operation else {
            panic!()
        };
        assert_eq!(sketch.entities.len(), 8);
    }
}

#[test]
fn closed_planar_path_builds_a_ring_without_caps() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let section = add_sketch(&mut engine, &mut kernel, square_section());
    let path = add_sketch(&mut engine, &mut kernel, path_sketch());
    let sw = engine
        .add_feature(
            "ring".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch {
                    sketch_id: path,
                    entity_ids: vec![10, 11, 12, 13],
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let roles = roles_of(&engine.feature_results[&sw]);
    assert!(!roles.is_empty());
    assert!(
        roles.iter().all(|r| matches!(r, Role::SideFace { .. })),
        "a ring has no caps: {roles:?}"
    );
}

#[test]
fn the_path_may_live_in_the_section_sketch() {
    // A construction line in the section sketch itself is a legal path
    // reference: the engine looks the path sketch up by id, whichever it is.
    let mut section = square_section();
    section.entities.push(pt(9, 0.0, 0.0));
    section.entities.push(pt(10, 0.5, 0.0));
    section.entities.push(line(20, 9, 10, true));
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let sk = add_sketch(&mut engine, &mut kernel, section);
    let sw = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                sk,
                SweepPathRef::Sketch {
                    sketch_id: sk,
                    entity_ids: vec![20],
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(engine.feature_results[&sw].outputs.len(), 1);
}

fn pt3(id: u32, xyz: [f64; 3]) -> Sketch3dEntity {
    Sketch3dEntity::Point {
        id,
        xyz,
        attach: None,
        xyz_expr: None,
        construction: false,
    }
}
fn line3(id: u32, start_id: u32, end_id: u32) -> Sketch3dEntity {
    Sketch3dEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

/// An L in space (0,0,0)→(1,0,0)→(1,0,1), lines 4 and 5, plus (with
/// `second`) a separate segment (5,5,5)→(6,5,5), line 8.
fn l_bend_3d(second: bool) -> Sketch3d {
    let mut ents = vec![
        pt3(1, [0.0, 0.0, 0.0]),
        pt3(2, [1.0, 0.0, 0.0]),
        pt3(3, [1.0, 0.0, 1.0]),
        line3(4, 1, 2),
        line3(5, 2, 3),
    ];
    if second {
        ents.push(pt3(6, [5.0, 5.0, 5.0]));
        ents.push(pt3(7, [6.0, 5.0, 5.0]));
        ents.push(line3(8, 6, 7));
    }
    Sketch3d::new(Uuid::new_v4(), ents)
}

#[test]
fn sketch3d_path_by_only_chain_or_by_entity() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let section = add_sketch(&mut engine, &mut kernel, square_section());
    let one = engine
        .add_feature(
            "path".into(),
            Operation::Sketch3d {
                sketch: l_bend_3d(false),
            },
            &mut kernel,
        )
        .unwrap();
    let sw = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch3d {
                    sketch_id: one,
                    entity_id: None,
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let roles = roles_of(&engine.feature_results[&sw]);
    assert!(roles.contains(&Role::EndCapNegative), "{roles:?}");
    assert!(roles.contains(&Role::EndCapPositive), "{roles:?}");

    // Two chains: the sweep must be told which.
    let two = engine
        .add_feature(
            "paths".into(),
            Operation::Sketch3d {
                sketch: l_bend_3d(true),
            },
            &mut kernel,
        )
        .unwrap();
    let ambiguous = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch3d {
                    sketch_id: two,
                    entity_id: None,
                },
            ),
            &mut kernel,
        )
        .unwrap();
    let err = feature_error(&engine, ambiguous);
    assert!(
        matches!(err.kind, ErrorKind::ResolutionFailed { .. }),
        "{}",
        err.message
    );
    assert!(err.message.contains("2 chains"), "{}", err.message);
    assert!(!engine.feature_results.contains_key(&ambiguous));

    let named = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch3d {
                    sketch_id: two,
                    entity_id: Some(5),
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(
        !engine.feature_errors.iter().any(|e| e.feature_id == named),
        "{:?}",
        engine.feature_errors
    );
    assert_eq!(engine.feature_results[&named].outputs.len(), 1);

    // An entity that is in no chain, and a sketch that is not a Sketch3d.
    let missing = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch3d {
                    sketch_id: two,
                    entity_id: Some(99),
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(matches!(
        feature_error(&engine, missing).kind,
        ErrorKind::ResolutionFailed { .. }
    ));
    let wrong_kind = engine
        .add_feature(
            "sweep".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch3d {
                    sketch_id: section,
                    entity_id: None,
                },
            ),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(
        feature_error(&engine, wrong_kind).kind,
        ErrorKind::SketchNotFound { id: section }
    );
}

#[test]
fn malformed_planar_paths_are_typed_errors() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let section = add_sketch(&mut engine, &mut kernel, square_section());
    let path = add_sketch(&mut engine, &mut kernel, path_sketch());
    for (ids, needle) in [
        (vec![10u32, 12], "connected"),
        (vec![99], "not in the sketch"),
        (vec![], "no entities"),
        (vec![1], "not a line or an arc"),
    ] {
        let sw = engine
            .add_feature(
                "sweep".into(),
                sweep_op(
                    section,
                    SweepPathRef::Sketch {
                        sketch_id: path,
                        entity_ids: ids.clone(),
                    },
                ),
                &mut kernel,
            )
            .unwrap();
        let err = feature_error(&engine, sw);
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
        assert!(!engine.feature_results.contains_key(&sw));
        engine.remove_feature(sw, &mut kernel).unwrap();
    }
}

#[test]
fn an_explicit_region_is_the_section() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let section = add_sketch(&mut engine, &mut kernel, square_section());
    let path = add_sketch(&mut engine, &mut kernel, path_sketch());
    let region: Region = serde_json::from_value(json!({
        "outer": [[-0.05, -0.05], [0.05, -0.05], [0.05, 0.05], [-0.05, 0.05]]
    }))
    .unwrap();
    let mut params = sweep_params(
        section,
        SweepPathRef::Sketch {
            sketch_id: path,
            entity_ids: vec![10, 11],
        },
    );
    params.profile_index = 42; // ignored when a region is given
    params.region = Some(region);
    let sw = engine
        .add_feature("sweep".into(), Operation::Sweep { params }, &mut kernel)
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(engine.feature_results[&sw].outputs.len(), 1);

    // Without a region, an out-of-range index is the usual typed error.
    let mut params = sweep_params(
        section,
        SweepPathRef::Sketch {
            sketch_id: path,
            entity_ids: vec![10, 11],
        },
    );
    params.profile_index = 42;
    let bad = engine
        .add_feature("sweep".into(), Operation::Sweep { params }, &mut kernel)
        .unwrap();
    assert!(matches!(
        feature_error(&engine, bad).kind,
        ErrorKind::ProfileOutOfRange { index: 42, .. }
    ));
    // …and `profile_entity_ids` addresses the square by its loop.
    let mut params = sweep_params(
        section,
        SweepPathRef::Sketch {
            sketch_id: path,
            entity_ids: vec![10, 11],
        },
    );
    params.profile_index = 42;
    params.profile_entity_ids = Some(vec![8, 7, 6, 5]);
    let by_ids = engine
        .add_feature("sweep".into(), Operation::Sweep { params }, &mut kernel)
        .unwrap();
    assert!(
        !engine.feature_errors.iter().any(|e| e.feature_id == by_ids),
        "{:?}",
        engine.feature_errors
    );
}

fn body_target(feature_id: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

#[test]
fn combine_add_consumes_its_explicit_target() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let section = add_sketch(&mut engine, &mut kernel, square_section());
    let path = add_sketch(&mut engine, &mut kernel, path_sketch());
    let first = engine
        .add_feature(
            "member".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch {
                    sketch_id: path,
                    entity_ids: vec![10],
                },
            ),
            &mut kernel,
        )
        .unwrap();
    let mut params = sweep_params(
        section,
        SweepPathRef::Sketch {
            sketch_id: path,
            entity_ids: vec![10, 11],
        },
    );
    params.combine = Some(CombineMode::Add);
    params.targets = Some(vec![body_target(first)]);
    let second = engine
        .add_feature("joined".into(), Operation::Sweep { params }, &mut kernel)
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(engine.consumed_features.contains(&first));
    assert_eq!(engine.feature_results[&second].outputs.len(), 1);

    // A combine with no targets falls back to the most recent solid.
    let mut params = sweep_params(
        section,
        SweepPathRef::Sketch {
            sketch_id: path,
            entity_ids: vec![11],
        },
    );
    params.combine = Some(CombineMode::Cut);
    let third = engine
        .add_feature("notch".into(), Operation::Sweep { params }, &mut kernel)
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(engine.consumed_features.contains(&second));
    assert!(engine.feature_results.contains_key(&third));
}

#[test]
fn a_curved_section_is_a_typed_capability_wall_not_a_chord_polygon() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let disc = sketch_on(
        [1.0, 0.0, 0.0],
        vec![
            pt(1, 0.0, 0.0),
            SketchEntity::Circle {
                id: 2,
                center_id: 1,
                radius: 0.1,
                construction: false,
            },
        ],
    );
    let section = add_sketch(&mut engine, &mut kernel, disc);
    let path = add_sketch(&mut engine, &mut kernel, path_sketch());
    let sw = engine
        .add_feature(
            "tube".into(),
            sweep_op(
                section,
                SweepPathRef::Sketch {
                    sketch_id: path,
                    entity_ids: vec![10, 11],
                },
            ),
            &mut kernel,
        )
        .unwrap();
    let err = feature_error(&engine, sw);
    assert!(
        matches!(err.kind, ErrorKind::NotSupported { .. }),
        "{:?}: {}",
        err.kind,
        err.message
    );
    assert!(err.message.contains("S4"), "{}", err.message);
    assert!(!engine.feature_results.contains_key(&sw));
}

#[test]
fn operation_tag_and_json_round_trip() {
    assert!(OPERATION_TAGS.contains(&"Sweep"));
    let section = Uuid::new_v4();
    let path = Uuid::new_v4();
    let op = sweep_op(
        section,
        SweepPathRef::Sketch {
            sketch_id: path,
            entity_ids: vec![10, 11],
        },
    );
    let v = serde_json::to_value(&op).unwrap();
    assert_eq!(v["type"], "Sweep");
    assert_eq!(v["params"]["sketch_id"], json!(section.to_string()));
    assert_eq!(v["params"]["profile_index"], json!(0));
    assert_eq!(v["params"]["path"]["type"], "Sketch");
    assert_eq!(v["params"]["path"]["entity_ids"], json!([10, 11]));
    assert!(v["params"].get("region").is_none());
    assert!(v["params"].get("profile_entity_ids").is_none());
    let back: Operation = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(back.type_tag(), "Sweep");
    assert_eq!(serde_json::to_value(&back).unwrap(), v);

    let op3 = sweep_op(
        section,
        SweepPathRef::Sketch3d {
            sketch_id: path,
            entity_id: None,
        },
    );
    let v3 = serde_json::to_value(&op3).unwrap();
    assert_eq!(
        v3["params"]["path"],
        json!({ "type": "Sketch3d", "sketch_id": path })
    );
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
            arg_dimensions: Default::default(),
        },
    }
}

const SWEEP_SCRIPT: &str = r#"
// @feature name="Bent bar" version=1
// @param w: length = 0.02
fn feature(ctx, p) {
    let sec = ctx.sketch(plane([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    sec.rect(-p.w / 2, -p.w / 2, p.w, p.w);
    let section = sec.finish().regions()[0];
    let path = ctx.sketch(plane([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]));
    let a = path.point(0.0, 0.0);
    let b = path.point(1.0, 0.0);
    let c = path.point(1.0, 1.0);
    let l1 = path.line(a, b, #{ construction: true });
    let l2 = path.line(b, c, #{ construction: true });
    let ps = path.finish();
    ctx.sweep(section, #{ path_sketch: ps, entity_ids: [l1, l2] })
}
"#;

#[test]
fn script_sweep_builds_a_body() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let src = Uuid::new_v4();
    engine.sources.insert_text(src, SWEEP_SCRIPT);
    let id = engine
        .add_feature("script".into(), script_op(src, json!({})), &mut kernel)
        .unwrap();
    assert!(
        engine.feature_errors.is_empty(),
        "{:?}",
        engine.feature_errors
    );
    let result = &engine.feature_results[&id];
    assert_eq!(result.outputs.len(), 1, "one swept body");
}
