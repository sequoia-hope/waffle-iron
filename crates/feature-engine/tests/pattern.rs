//! Circular / linear pattern features (spec
//! `specs/custom_features_and_modeling_roadmap.md` §B1) on MockKernel:
//! outputs, custody (consumption), placements, roles, errors, suppression,
//! undo, expression-driven parameters. Real-geometry oracles (volumes,
//! Euler characteristic, booleans) live in
//! `crates/test-harness/tests/pattern_kv2.rs`.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::{KernelIntrospect, MockKernel};
use waffle_types::*;

// ── Fixtures ────────────────────────────────────────────────────────────────

fn make_sketch_op() -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (1.0, 0.0));
    solved_positions.insert(3, (1.0, 1.0));
    solved_positions.insert(4, (0.0, 1.0));
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
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
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: (1..=4)
            .map(|id| SketchEntity::Point {
                id,
                x: solved_positions[&id].0,
                y: solved_positions[&id].1,
                construction: false,
            })
            .collect(),
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: vec![],
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    };
    Operation::Sketch { sketch }
}

fn make_extrude(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: Some(CombineMode::NewBody),
            targets: None,
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth: 1.0,
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

fn body_ref(feature_id: Uuid, key: OutputKey, policy: ResolvePolicy) -> GeomRef {
    GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: key,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy,
        scope: None,
    }
}

fn z_axis() -> AxisRef {
    AxisRef::Explicit {
        origin: [0.0, 0.0, 0.0],
        direction: [0.0, 0.0, 1.0],
    }
}

fn circular(seed: Uuid, count: u32, angle_deg: f64) -> Operation {
    Operation::PatternCircular {
        params: PatternCircularParams {
            seeds: PatternSeeds::Selected(vec![body_ref(
                seed,
                OutputKey::Main,
                ResolvePolicy::Strict,
            )]),
            axis: z_axis(),
            count,
            angle_deg,
            angle_expr: None,
            skip: vec![],
            combine: None,
            targets: None,
        },
    }
}

fn linear(seed: Uuid, count: u32, spacing: f64) -> Operation {
    Operation::PatternLinear {
        params: PatternLinearParams {
            seeds: PatternSeeds::Selected(vec![body_ref(
                seed,
                OutputKey::Main,
                ResolvePolicy::Strict,
            )]),
            direction: AxisRef::Explicit {
                origin: [0.0; 3],
                direction: [1.0, 0.0, 0.0],
            },
            count,
            spacing,
            spacing_expr: None,
            second: None,
            skip: vec![],
            combine: None,
            targets: None,
        },
    }
}

/// `(engine, kernel, extrude_id)`: one unit box body.
fn one_box() -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s = engine
        .add_feature("Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e = engine
        .add_feature("Box".into(), make_extrude(s), &mut kernel)
        .unwrap();
    (engine, kernel, e)
}

fn body_count(engine: &Engine, id: Uuid) -> usize {
    engine
        .get_result(id)
        .map(|r| {
            r.outputs
                .iter()
                .filter(|(k, _)| matches!(k, OutputKey::Main | OutputKey::Body { .. }))
                .count()
        })
        .unwrap_or(0)
}

fn error_of(engine: &Engine, id: Uuid) -> Option<String> {
    engine
        .errors
        .iter()
        .find(|(f, _)| *f == id)
        .map(|(_, m)| m.clone())
}

/// Centroid of a body's faces (mock faces carry centroids).
fn centroid(kernel: &MockKernel, handle: &waffle_types::kernel::KernelSolidHandle) -> [f64; 3] {
    let faces = kernel.list_faces(handle);
    let mut c = [0.0; 3];
    for f in &faces {
        let sig = kernel.compute_signature(*f, TopoKind::Face);
        let p = sig.centroid.unwrap_or([0.0; 3]);
        for k in 0..3 {
            c[k] += p[k] / faces.len() as f64;
        }
    }
    c
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn circular_new_body_emits_every_instance_and_takes_custody_of_the_seed() {
    let (mut engine, mut kernel, e) = one_box();
    let p = engine
        .add_feature("Pattern".into(), circular(e, 6, 360.0), &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, p), None);
    assert_eq!(body_count(&engine, p), 6, "seed + 5 copies");
    assert!(
        engine.consumed_features.contains(&e),
        "the seed's feature is consumed (custody moves to the pattern)"
    );
    let r = engine.get_result(p).unwrap();
    assert_eq!(r.outputs[0].0, OutputKey::Main);
    assert_eq!(r.outputs[1].0, OutputKey::Body { index: 1 });
    // Instance 0 IS the seed body (same handle), not a copy.
    let seed_handle = engine.get_result(e).unwrap().outputs[0].1.handle.clone();
    assert_eq!(r.outputs[0].1.handle.raw(), seed_handle.raw());
    // Every instance's faces carry PatternInstance { index }.
    for i in 0..6 {
        let h = &r.outputs[i].1.handle;
        let faces = kernel.list_faces(h);
        assert_eq!(faces.len(), 6);
        for f in faces {
            assert!(r
                .provenance
                .role_assignments
                .contains(&(f, Role::PatternInstance { index: i })));
        }
    }
    // Copies are created entities; the seed's are not.
    assert_eq!(r.provenance.created.len(), 5 * (6 + 12 + 8));
}

#[test]
fn circular_full_turn_spaces_by_count_and_partial_sweep_ends_at_angle() {
    // Unit box centroid (0.5, 0.5, 0.5); rotate about z through the origin.
    let (mut engine, mut kernel, e) = one_box();
    let p = engine
        .add_feature("Pattern".into(), circular(e, 4, 360.0), &mut kernel)
        .unwrap();
    let r = engine.get_result(p).unwrap();
    let c1 = centroid(&kernel, &r.outputs[1].1.handle);
    // 90°: (0.5, 0.5) → (−0.5, 0.5)
    assert!(
        (c1[0] + 0.5).abs() < 1e-12 && (c1[1] - 0.5).abs() < 1e-12,
        "{c1:?}"
    );
    let c3 = centroid(&kernel, &r.outputs[3].1.handle);
    // 270°: (0.5, 0.5) → (0.5, −0.5)
    assert!(
        (c3[0] - 0.5).abs() < 1e-12 && (c3[1] + 0.5).abs() < 1e-12,
        "{c3:?}"
    );

    // Partial sweep: 3 instances over 90° ⇒ step 45°, last at exactly 90°.
    let (mut engine, mut kernel, e) = one_box();
    let p = engine
        .add_feature("Pattern".into(), circular(e, 3, 90.0), &mut kernel)
        .unwrap();
    let r = engine.get_result(p).unwrap();
    let c2 = centroid(&kernel, &r.outputs[2].1.handle);
    assert!(
        (c2[0] + 0.5).abs() < 1e-12 && (c2[1] - 0.5).abs() < 1e-12,
        "{c2:?}"
    );
    let c1 = centroid(&kernel, &r.outputs[1].1.handle);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    // 45°: (0.5, 0.5) → (0, 0.5·√2)
    assert!(c1[0].abs() < 1e-12 && (c1[1] - s).abs() < 1e-12, "{c1:?}");
}

#[test]
fn linear_places_by_spacing_and_grid_indexes_row_major() {
    let (mut engine, mut kernel, e) = one_box();
    let mut op = linear(e, 3, 2.0);
    if let Operation::PatternLinear { params } = &mut op {
        params.second = Some(LinearSecondDirection {
            direction: AxisRef::Explicit {
                origin: [0.0; 3],
                direction: [0.0, 1.0, 0.0],
            },
            count: 2,
            spacing: 5.0,
            spacing_expr: None,
        });
    }
    let p = engine.add_feature("Grid".into(), op, &mut kernel).unwrap();
    assert_eq!(error_of(&engine, p), None);
    assert_eq!(body_count(&engine, p), 6);
    let r = engine.get_result(p).unwrap();
    // index = i + j·count: index 2 = (i=2, j=0) → x + 4; index 4 = (1, 1) → x + 2, y + 5.
    let c2 = centroid(&kernel, &r.outputs[2].1.handle);
    assert!(
        (c2[0] - 4.5).abs() < 1e-12 && (c2[1] - 0.5).abs() < 1e-12,
        "{c2:?}"
    );
    let c4 = centroid(&kernel, &r.outputs[4].1.handle);
    assert!(
        (c4[0] - 2.5).abs() < 1e-12 && (c4[1] - 5.5).abs() < 1e-12,
        "{c4:?}"
    );
}

#[test]
fn skip_omits_instances_and_seed_cannot_be_skipped() {
    let (mut engine, mut kernel, e) = one_box();
    let mut op = circular(e, 5, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.skip = vec![2, 4];
    }
    let p = engine
        .add_feature("Pattern".into(), op, &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, p), None);
    assert_eq!(body_count(&engine, p), 3);
    let r = engine.get_result(p).unwrap();
    // Surviving instances 0, 1, 3 keep their instance index in the roles.
    let h1 = &r.outputs[1].1.handle;
    let f = kernel.list_faces(h1)[0];
    assert!(r
        .provenance
        .role_assignments
        .contains(&(f, Role::PatternInstance { index: 1 })));
    let h2 = &r.outputs[2].1.handle;
    let f = kernel.list_faces(h2)[0];
    assert!(r
        .provenance
        .role_assignments
        .contains(&(f, Role::PatternInstance { index: 3 })));

    let (mut engine, mut kernel, e) = one_box();
    let mut op = circular(e, 5, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.skip = vec![0];
    }
    let p = engine
        .add_feature("Pattern".into(), op, &mut kernel)
        .unwrap();
    let err = error_of(&engine, p).expect("skipping the seed is an error");
    assert!(err.contains("cannot be skipped"), "{err}");
    assert!(engine.get_result(p).is_none(), "no partial output on error");
}

#[test]
fn invalid_parameters_are_loud_typed_errors_with_no_output() {
    let cases: Vec<(&str, Operation, &str)> = {
        let (_, _, e) = one_box();
        vec![
            ("count 1", circular(e, 1, 360.0), "count must be at least 2"),
            ("zero angle", circular(e, 4, 0.0), "angle must be"),
            ("zero spacing", linear(e, 3, 0.0), "spacing must be"),
            (
                "zero axis",
                Operation::PatternCircular {
                    params: PatternCircularParams {
                        seeds: PatternSeeds::Selected(vec![body_ref(
                            e,
                            OutputKey::Main,
                            ResolvePolicy::Strict,
                        )]),
                        axis: AxisRef::Explicit {
                            origin: [0.0; 3],
                            direction: [0.0; 3],
                        },
                        count: 4,
                        angle_deg: 360.0,
                        angle_expr: None,
                        skip: vec![],
                        combine: None,
                        targets: None,
                    },
                },
                "zero-length",
            ),
            (
                "budget",
                circular(e, 20_001, 360.0),
                "exceeds the pattern budget",
            ),
        ]
    };
    for (label, op, needle) in cases {
        let (mut engine, mut kernel, e) = one_box();
        // Re-anchor the seed to THIS engine's box.
        let op = match op {
            Operation::PatternCircular { mut params } => {
                params.seeds = PatternSeeds::Selected(vec![body_ref(
                    e,
                    OutputKey::Main,
                    ResolvePolicy::Strict,
                )]);
                Operation::PatternCircular { params }
            }
            Operation::PatternLinear { mut params } => {
                params.seeds = PatternSeeds::Selected(vec![body_ref(
                    e,
                    OutputKey::Main,
                    ResolvePolicy::Strict,
                )]);
                Operation::PatternLinear { params }
            }
            other => other,
        };
        let p = engine
            .add_feature("Pattern".into(), op, &mut kernel)
            .unwrap();
        let err = error_of(&engine, p).unwrap_or_else(|| panic!("{label}: expected an error"));
        assert!(err.contains(needle), "{label}: {err}");
        assert!(engine.get_result(p).is_none(), "{label}: no partial output");
        assert!(
            !engine.consumed_features.contains(&e),
            "{label}: a failed pattern does not consume its seed"
        );
    }
}

#[test]
fn parallel_second_direction_is_refused() {
    let (mut engine, mut kernel, e) = one_box();
    let mut op = linear(e, 3, 2.0);
    if let Operation::PatternLinear { params } = &mut op {
        params.second = Some(LinearSecondDirection {
            direction: AxisRef::Explicit {
                origin: [0.0; 3],
                direction: [-2.0, 0.0, 0.0],
            },
            count: 2,
            spacing: 5.0,
            spacing_expr: None,
        });
    }
    let p = engine.add_feature("Grid".into(), op, &mut kernel).unwrap();
    let err = error_of(&engine, p).expect("parallel grid directions are refused");
    assert!(err.contains("parallel"), "{err}");
}

#[test]
fn seed_already_consumed_is_refused_and_seed_as_target_is_refused() {
    // Box A, pattern P1 of A (consumes A), pattern P2 of A ⇒ refused.
    let (mut engine, mut kernel, e) = one_box();
    let p1 = engine
        .add_feature("P1".into(), circular(e, 3, 360.0), &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, p1), None);
    let p2 = engine
        .add_feature("P2".into(), circular(e, 3, 360.0), &mut kernel)
        .unwrap();
    let err = error_of(&engine, p2).expect("consumed seed is refused");
    assert!(err.contains("already consumed"), "{err}");

    // Seed that is also a target.
    let (mut engine, mut kernel, e) = one_box();
    let mut op = circular(e, 3, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.combine = Some(CombineMode::Add);
        params.targets = Some(vec![body_ref(e, OutputKey::Main, ResolvePolicy::Strict)]);
    }
    let p = engine.add_feature("P".into(), op, &mut kernel).unwrap();
    let err = error_of(&engine, p).expect("seed == target is refused");
    assert!(err.contains("both a seed and a target"), "{err}");
}

#[test]
fn cut_and_intersect_need_a_target_but_add_does_not() {
    for (mode, needs) in [
        (CombineMode::Cut, true),
        (CombineMode::Intersect, true),
        (CombineMode::Add, false),
    ] {
        let (mut engine, mut kernel, e) = one_box();
        let mut op = circular(e, 3, 360.0);
        if let Operation::PatternCircular { params } = &mut op {
            params.combine = Some(mode);
        }
        let p = engine.add_feature("P".into(), op, &mut kernel).unwrap();
        match (error_of(&engine, p), needs) {
            (Some(err), true) => assert!(err.contains("requires at least one target"), "{err}"),
            (None, false) => assert!(body_count(&engine, p) >= 1),
            (e, _) => panic!("{mode:?}: unexpected {e:?}"),
        }
    }
}

#[test]
fn add_into_a_target_consumes_the_target_and_emits_the_fold() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s = engine
        .add_feature("S".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let base = engine
        .add_feature("Base".into(), make_extrude(s), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("S2".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let seed = engine
        .add_feature("Seed".into(), make_extrude(s2), &mut kernel)
        .unwrap();
    let mut op = circular(seed, 3, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.combine = Some(CombineMode::Add);
        params.targets = Some(vec![body_ref(base, OutputKey::Main, ResolvePolicy::Strict)]);
    }
    let p = engine.add_feature("P".into(), op, &mut kernel).unwrap();
    assert_eq!(error_of(&engine, p), None);
    assert!(engine.consumed_features.contains(&base));
    assert!(engine.consumed_features.contains(&seed));
    // MockKernel's union always merges (it never reports disjoint lumps), so
    // the fold collapses everything into one body.
    assert_eq!(body_count(&engine, p), 1);
    assert_eq!(engine.get_result(p).unwrap().outputs[0].0, OutputKey::Main);
}

#[test]
fn consumed_target_is_refused_strict_and_dropped_best_effort() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s = engine
        .add_feature("S".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let base = engine
        .add_feature("Base".into(), make_extrude(s), &mut kernel)
        .unwrap();
    let s2 = engine
        .add_feature("S2".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let seed_a = engine
        .add_feature("SeedA".into(), make_extrude(s2), &mut kernel)
        .unwrap();
    // P1 consumes Base.
    let mut op = circular(seed_a, 3, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.combine = Some(CombineMode::Add);
        params.targets = Some(vec![body_ref(base, OutputKey::Main, ResolvePolicy::Strict)]);
    }
    let p1 = engine.add_feature("P1".into(), op, &mut kernel).unwrap();
    assert_eq!(error_of(&engine, p1), None);

    let s3 = engine
        .add_feature("S3".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let seed_b = engine
        .add_feature("SeedB".into(), make_extrude(s3), &mut kernel)
        .unwrap();
    for (policy, expect_err) in [
        (ResolvePolicy::Strict, true),
        (ResolvePolicy::BestEffort, false),
    ] {
        let mut op = circular(seed_b, 3, 360.0);
        if let Operation::PatternCircular { params } = &mut op {
            params.combine = Some(CombineMode::Add);
            params.targets = Some(vec![body_ref(base, OutputKey::Main, policy)]);
        }
        let p2 = engine
            .add_feature(format!("P2 {policy:?}"), op, &mut kernel)
            .unwrap();
        if expect_err {
            let err = error_of(&engine, p2).expect("consumed Strict target refused");
            assert!(err.contains("already consumed"), "{err}");
            engine.remove_feature(p2, &mut kernel).unwrap();
        } else {
            assert_eq!(error_of(&engine, p2), None);
            assert!(engine
                .warnings
                .iter()
                .any(|w| w.contains("already consumed") && w.contains("dropped")));
            // Dropped target ⇒ Add merges instances among themselves only.
            assert_eq!(body_count(&engine, p2), 1);
        }
    }
}

#[test]
fn suppress_and_undo_restore_the_seed_to_its_own_feature() {
    let (mut engine, mut kernel, e) = one_box();
    let p = engine
        .add_feature("Pattern".into(), circular(e, 4, 360.0), &mut kernel)
        .unwrap();
    assert!(engine.consumed_features.contains(&e));

    engine.set_suppressed(p, true, &mut kernel).unwrap();
    assert!(
        !engine.consumed_features.contains(&e),
        "suppressed pattern releases the seed"
    );
    assert!(engine.get_result(p).is_none());
    engine.set_suppressed(p, false, &mut kernel).unwrap();
    assert!(engine.consumed_features.contains(&e));
    assert_eq!(body_count(&engine, p), 4);

    engine.undo(&mut kernel).unwrap(); // unsuppress
    engine.undo(&mut kernel).unwrap(); // suppress
    engine.undo(&mut kernel).unwrap(); // add pattern
    assert!(engine.get_result(p).is_none());
    assert!(!engine.consumed_features.contains(&e));
    engine.redo(&mut kernel).unwrap();
    assert_eq!(body_count(&engine, p), 4);
    assert!(engine.consumed_features.contains(&e));
}

#[test]
fn edit_count_regenerates_and_expressions_drive_angle_and_spacing() {
    let (mut engine, mut kernel, e) = one_box();
    let p = engine
        .add_feature("Pattern".into(), circular(e, 4, 360.0), &mut kernel)
        .unwrap();
    engine
        .edit_feature(p, circular(e, 7, 360.0), &mut kernel)
        .unwrap();
    assert_eq!(body_count(&engine, p), 7);

    // angle_expr drives angle_deg from a design parameter (degrees).
    let mut op = circular(e, 3, 1.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.angle_expr = Some("sweep".into());
    }
    engine.edit_feature(p, op, &mut kernel).unwrap();
    engine.set_parameters(vec![DesignParameter::new("sweep", "90")], &mut kernel);
    let Operation::PatternCircular { params } = &engine.tree.features[2].operation else {
        panic!("pattern");
    };
    assert_eq!(params.angle_deg, 90.0);
    let r = engine.get_result(p).unwrap();
    let c2 = centroid(&kernel, &r.outputs[2].1.handle);
    assert!(
        (c2[0] + 0.5).abs() < 1e-12 && (c2[1] - 0.5).abs() < 1e-12,
        "{c2:?}"
    );

    // spacing_expr is mm-space → meters.
    let mut op = linear(e, 2, 1.0);
    if let Operation::PatternLinear { params } = &mut op {
        params.spacing_expr = Some("pitch * 2".into());
    }
    engine.edit_feature(p, op, &mut kernel).unwrap();
    engine.set_parameters(vec![DesignParameter::new("pitch", "10")], &mut kernel);
    let Operation::PatternLinear { params } = &engine.tree.features[2].operation else {
        panic!("pattern");
    };
    assert!((params.spacing - 0.02).abs() < 1e-15, "{}", params.spacing);
}

#[test]
fn pattern_json_round_trips_and_is_a_known_tag() {
    let (_, _, e) = one_box();
    let op = circular(e, 5, 180.0);
    let json = serde_json::to_value(&op).unwrap();
    assert_eq!(json["type"], "PatternCircular");
    assert_eq!(json["params"]["axis"]["method"], "explicit");
    let back: Operation = serde_json::from_value(json).unwrap();
    assert!(matches!(back, Operation::PatternCircular { .. }));
    assert!(OPERATION_TAGS.contains(&"PatternCircular"));
    assert!(OPERATION_TAGS.contains(&"PatternLinear"));

    // Defaults: angle_deg absent ⇒ 360; skip absent ⇒ empty.
    let minimal = serde_json::json!({
        "type": "PatternCircular",
        "params": {
            "seeds": [],
            "axis": { "method": "explicit", "origin": [0,0,0], "direction": [0,0,1] },
            "count": 3
        }
    });
    let op: Operation = serde_json::from_value(minimal).unwrap();
    let Operation::PatternCircular { params } = op else {
        panic!()
    };
    assert_eq!(params.angle_deg, 360.0);
    assert!(params.skip.is_empty() && params.combine.is_none());
}

// ── Mirror pattern (FEATURE_NOTES §4) ───────────────────────────────────────

fn mirror(seed: Uuid, origin: [f64; 3], normal: [f64; 3]) -> Operation {
    Operation::PatternMirror {
        params: PatternMirrorParams {
            seeds: PatternSeeds::Selected(vec![body_ref(
                seed,
                OutputKey::Main,
                ResolvePolicy::Strict,
            )]),
            plane: AxisRef::Explicit {
                origin,
                direction: normal,
            },
            combine: None,
            targets: None,
        },
    }
}

#[test]
fn mirror_emits_the_seed_and_its_reflection_and_takes_custody() {
    let (mut engine, mut kernel, e) = one_box();
    // Unit box, centroid (0.5, 0.5, 0.5); mirror in x = 2 ⇒ centroid (3.5, …).
    let p = engine
        .add_feature(
            "Mirror".into(),
            mirror(e, [2.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, p), None);
    assert_eq!(body_count(&engine, p), 2, "the seed and its reflection");
    assert!(
        engine.consumed_features.contains(&e),
        "the seed's feature is consumed, as with every pattern"
    );
    let r = engine.get_result(p).unwrap();
    // Instance 0 IS the seed body, not a copy.
    let seed_handle = engine.get_result(e).unwrap().outputs[0].1.handle.clone();
    assert_eq!(r.outputs[0].1.handle.raw(), seed_handle.raw());
    let c = centroid(&kernel, &r.outputs[1].1.handle);
    assert!(
        (c[0] - 3.5).abs() < 1e-12 && (c[1] - 0.5).abs() < 1e-12 && (c[2] - 0.5).abs() < 1e-12,
        "reflected centroid {c:?}"
    );
    for (i, out) in r.outputs.iter().enumerate() {
        for f in kernel.list_faces(&out.1.handle) {
            assert!(r
                .provenance
                .role_assignments
                .contains(&(f, Role::PatternInstance { index: i })));
        }
    }
}

#[test]
fn a_mirror_plane_with_no_normal_is_a_loud_typed_error() {
    let (mut engine, mut kernel, e) = one_box();
    let p = engine
        .add_feature(
            "Mirror".into(),
            mirror(e, [0.0; 3], [0.0, 0.0, 0.0]),
            &mut kernel,
        )
        .unwrap();
    let err = error_of(&engine, p).expect("a zero plane normal is refused");
    assert!(err.contains("zero-length"), "{err}");
    assert_eq!(body_count(&engine, p), 0, "no body from a refused mirror");
    assert!(
        !engine.consumed_features.contains(&e),
        "a refused mirror takes custody of nothing"
    );
}

#[test]
fn mirror_json_round_trips_and_is_a_known_tag() {
    let (_, _, e) = one_box();
    let op = mirror(e, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    let json = serde_json::to_value(&op).unwrap();
    assert_eq!(json["type"], "PatternMirror");
    let back: Operation = serde_json::from_value(json).unwrap();
    assert!(matches!(back, Operation::PatternMirror { .. }));
    assert!(OPERATION_TAGS.contains(&"PatternMirror"));
}

// ── Seeds = every live body (FEATURE_NOTES §7) ──────────────────────────────

#[test]
fn all_seeds_pattern_every_live_body_without_naming_one() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s = engine
        .add_feature("Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let a = engine
        .add_feature("Box A".into(), make_extrude(s), &mut kernel)
        .unwrap();
    let b = engine
        .add_feature("Box B".into(), make_extrude(s), &mut kernel)
        .unwrap();
    let mut op = circular(a, 3, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.seeds = PatternSeeds::All;
    }
    let p = engine
        .add_feature("Pattern".into(), op, &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, p), None);
    // Two seeds × three instances.
    assert_eq!(body_count(&engine, p), 6);
    assert!(
        engine.consumed_features.contains(&a) && engine.consumed_features.contains(&b),
        "custody of every body it patterned"
    );
    // The sketch is not a body and is not consumed.
    assert!(!engine.consumed_features.contains(&s));
}

#[test]
fn all_seeds_with_nothing_to_pattern_is_a_loud_typed_error() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s = engine
        .add_feature("Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let mut op = circular(s, 3, 360.0);
    if let Operation::PatternCircular { params } = &mut op {
        params.seeds = PatternSeeds::All;
    }
    let p = engine
        .add_feature("Pattern".into(), op, &mut kernel)
        .unwrap();
    let err = error_of(&engine, p).expect("nothing to pattern is refused");
    assert!(err.contains("no live bodies"), "{err}");
    assert_eq!(body_count(&engine, p), 0);
}

#[test]
fn seeds_read_both_written_forms_and_a_list_writes_back_as_a_list() {
    // The array form is what every document written before the `All` set
    // holds, and is still what a list of picked bodies writes.
    let listed = serde_json::json!({
        "type": "PatternCircular",
        "params": {
            "seeds": [serde_json::to_value(body_ref(Uuid::nil(), OutputKey::Main, ResolvePolicy::Strict)).unwrap()],
            "axis": { "method": "explicit", "origin": [0,0,0], "direction": [0,0,1] },
            "count": 3
        }
    });
    let op: Operation = serde_json::from_value(listed.clone()).unwrap();
    let Operation::PatternCircular { ref params } = op else {
        panic!()
    };
    assert_eq!(params.seeds.listed().map(<[_]>::len), Some(1));
    assert_eq!(
        serde_json::to_value(&op).unwrap()["params"]["seeds"],
        listed["params"]["seeds"],
        "a list round-trips as the same list"
    );

    let all = serde_json::json!({
        "type": "PatternCircular",
        "params": {
            "seeds": { "type": "All" },
            "axis": { "method": "explicit", "origin": [0,0,0], "direction": [0,0,1] },
            "count": 3
        }
    });
    let op: Operation = serde_json::from_value(all.clone()).unwrap();
    let Operation::PatternCircular { ref params } = op else {
        panic!()
    };
    assert!(params.seeds.listed().is_none());
    assert_eq!(
        serde_json::to_value(&op).unwrap()["params"]["seeds"],
        all["params"]["seeds"]
    );

    // Anything else is a parse error, not a silent empty pattern.
    let bogus = serde_json::json!({
        "type": "PatternCircular",
        "params": {
            "seeds": { "type": "Every" },
            "axis": { "method": "explicit", "origin": [0,0,0], "direction": [0,0,1] },
            "count": 3
        }
    });
    let err = serde_json::from_value::<Operation>(bogus)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("Every") || err.contains("unknown set"),
        "{err}"
    );
}
