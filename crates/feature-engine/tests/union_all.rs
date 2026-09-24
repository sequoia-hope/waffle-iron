//! `Operation::UnionAll` (`specs/b4_balanced_union.md`) on MockKernel:
//! target resolution (`All` / `Selected`), custody, the conservative-box
//! gate, progress frames, typed refusals, name inheritance, the loud
//! consumed-operand rule for `BooleanCombine`, JSON round-trip and the
//! script API. Real-geometry oracles (exact volume, χ, watertightness,
//! chain-vs-tree determinism) live in
//! `crates/test-harness/tests/union_all_kv2.rs`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use feature_engine::progress::{self, ProgressEvent};
use feature_engine::types::*;
use feature_engine::Engine;
use serde_json::json;
use uuid::Uuid;
use waffle_types::kernel::{KernelIntrospect, MockKernel};
use waffle_types::*;

// ── Fixtures ────────────────────────────────────────────────────────────────

/// A unit square on the plane z = 0 with its corner at `origin` (the mock
/// kernel builds solids in sketch coordinates, so the offset goes into the
/// points, not the plane origin).
fn make_sketch_op_at(origin: [f64; 3]) -> Operation {
    let (ox, oy) = (origin[0], origin[1]);
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (ox, oy));
    solved_positions.insert(2, (ox + 1.0, oy));
    solved_positions.insert(3, (ox + 1.0, oy + 1.0));
    solved_positions.insert(4, (ox, oy + 1.0));
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

fn union_all() -> Operation {
    Operation::UnionAll {
        params: UnionAllParams::default(),
    }
}

fn union_selected(bodies: Vec<GeomRef>) -> Operation {
    Operation::UnionAll {
        params: UnionAllParams {
            targets: UnionTargets::Selected { bodies },
        },
    }
}

fn boolean(a: Uuid, b: Uuid) -> Operation {
    Operation::BooleanCombine {
        params: BooleanParams {
            body_a: body_ref(a, OutputKey::Main, ResolvePolicy::BestEffort),
            body_b: body_ref(b, OutputKey::Main, ResolvePolicy::BestEffort),
            operation: BooleanOp::Union,
        },
    }
}

/// Add a NewBody box whose sketch sits at `origin`.
fn add_box(engine: &mut Engine, kernel: &mut MockKernel, name: &str, origin: [f64; 3]) -> Uuid {
    let s = engine
        .add_feature(format!("{name} sketch"), make_sketch_op_at(origin), kernel)
        .unwrap();
    engine
        .add_feature(name.to_string(), make_extrude(s), kernel)
        .unwrap()
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

fn warnings_of(engine: &Engine, id: Uuid) -> Vec<String> {
    engine
        .get_result(id)
        .map(|r| r.diagnostics.warnings.clone())
        .unwrap_or_default()
}

/// Install a progress sink on this thread; returns the shared event log.
fn capture_progress() -> Rc<RefCell<Vec<ProgressEvent>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s2 = Rc::clone(&seen);
    progress::install(Box::new(move |e| s2.borrow_mut().push(e.clone())));
    seen
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn all_folds_every_live_body_and_consumes_their_features() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    // Three coincident boxes (the mock's union always merges).
    let a = add_box(&mut engine, &mut kernel, "A", [0.0; 3]);
    let b = add_box(&mut engine, &mut kernel, "B", [0.0; 3]);
    let c = add_box(&mut engine, &mut kernel, "C", [0.0; 3]);
    let seen = capture_progress();
    let u = engine
        .add_feature("Union".into(), union_all(), &mut kernel)
        .unwrap();
    progress::clear();

    assert_eq!(error_of(&engine, u), None);
    assert_eq!(body_count(&engine, u), 1, "one lump");
    for f in [a, b, c] {
        assert!(engine.consumed_features.contains(&f), "{f} consumed");
    }
    assert_eq!(
        engine.consumed_by.get(&u).cloned(),
        Some(vec![a, b, c]),
        "consumed in tree order"
    );
    assert!(!engine.consumed_features.contains(&u));

    // Two unions for three connected bodies: frames 0, 1, 2 and the final.
    let done: Vec<usize> = seen.borrow().iter().map(|e| e.done).collect();
    assert_eq!(done, vec![0, 1, 2, 2], "{:?}", seen.borrow());
    assert_eq!(seen.borrow().last().unwrap().remaining, 0);
    assert!(seen.borrow().iter().all(|e| e.feature_id == u));

    // Name inheritance: Main is the first body's lump.
    let a_body = FeatureTree::body_id(a, &OutputKey::Main);
    let u_body = FeatureTree::body_id(u, &OutputKey::Main);
    engine.rename_body(a_body, "Frame".to_string());
    assert_eq!(engine.display_body_name_override(&u_body), Some("Frame"));

    // A later body sees only the union's output as live: a second UnionAll
    // folds the union's lump with the new body and consumes both.
    let d = add_box(&mut engine, &mut kernel, "D", [0.0; 3]);
    let u2 = engine
        .add_feature("Union 2".into(), union_all(), &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, u2), None);
    assert_eq!(engine.consumed_by.get(&u2).cloned(), Some(vec![u, d]));
    assert_eq!(body_count(&engine, u2), 1);
}

#[test]
fn box_disjoint_bodies_never_reach_the_kernel_and_stay_separate() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    // The mock builds every extrusion at the origin, so the far body is a
    // linear-pattern copy 10 m along x (the pattern moves vertices).
    let seed = add_box(&mut engine, &mut kernel, "Seed", [0.0; 3]);
    let pat = engine
        .add_feature(
            "Two apart".into(),
            Operation::PatternLinear {
                params: PatternLinearParams {
                    seeds: PatternSeeds::Selected(vec![body_ref(
                        seed,
                        OutputKey::Main,
                        ResolvePolicy::Strict,
                    )]),
                    direction: AxisRef::Explicit {
                        origin: [0.0, 0.0, 0.0],
                        direction: [1.0, 0.0, 0.0],
                    },
                    count: 2,
                    spacing: 10.0,
                    spacing_expr: None,
                    second: None,
                    skip: vec![],
                    combine: None,
                    targets: None,
                },
            },
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, pat), None);
    assert_eq!(body_count(&engine, pat), 2);
    // The fixture is what it claims: the two boxes are disjoint.
    let outs = &engine.get_result(pat).unwrap().outputs;
    let (_, hi_near) = kernel.solid_aabb(&outs[0].1.handle).expect("near box");
    let (lo_far, _) = kernel.solid_aabb(&outs[1].1.handle).expect("far box");
    assert!(hi_near[0] < lo_far[0], "{hi_near:?} vs {lo_far:?}");

    let seen = capture_progress();
    let u = engine
        .add_feature("Union".into(), union_all(), &mut kernel)
        .unwrap();
    progress::clear();
    assert_eq!(error_of(&engine, u), None);
    // The mock's union would have merged them; two outputs prove the gate
    // skipped the pair.
    assert_eq!(body_count(&engine, u), 2);
    let keys: Vec<OutputKey> = engine
        .get_result(u)
        .unwrap()
        .outputs
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(keys, vec![OutputKey::Main, OutputKey::Body { index: 1 }]);
    let done: Vec<usize> = seen.borrow().iter().map(|e| e.done).collect();
    assert_eq!(done, vec![0, 0], "no union ran: {:?}", seen.borrow());
    assert!(engine.consumed_features.contains(&pat));
}

#[test]
fn selected_targets_honor_policy_and_refuse_duplicates_and_self() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let a = add_box(&mut engine, &mut kernel, "A", [0.0; 3]);
    let b = add_box(&mut engine, &mut kernel, "B", [0.0; 3]);
    let c = add_box(&mut engine, &mut kernel, "C", [0.0; 3]);
    let first = engine
        .add_feature(
            "AB".into(),
            union_selected(vec![
                body_ref(a, OutputKey::Main, ResolvePolicy::Strict),
                body_ref(b, OutputKey::Main, ResolvePolicy::Strict),
            ]),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, first), None);
    assert!(engine.consumed_features.contains(&a) && engine.consumed_features.contains(&b));
    assert!(!engine.consumed_features.contains(&c), "C untouched");

    // Strict on a consumed body: loud, no output.
    let strict = engine
        .add_feature(
            "Strict".into(),
            union_selected(vec![
                body_ref(a, OutputKey::Main, ResolvePolicy::Strict),
                body_ref(c, OutputKey::Main, ResolvePolicy::Strict),
            ]),
            &mut kernel,
        )
        .unwrap();
    let err = error_of(&engine, strict).expect("strict refusal");
    assert!(err.contains("already consumed"), "{err}");
    assert_eq!(body_count(&engine, strict), 0);
    assert!(
        !engine.consumed_features.contains(&c),
        "a failed union consumes nothing"
    );
    engine.remove_feature(strict, &mut kernel).unwrap();

    // BestEffort on a consumed body: dropped with a warning; the rest folds.
    let lenient = engine
        .add_feature(
            "Lenient".into(),
            union_selected(vec![
                body_ref(c, OutputKey::Main, ResolvePolicy::BestEffort),
                body_ref(a, OutputKey::Main, ResolvePolicy::BestEffort),
                body_ref(first, OutputKey::Main, ResolvePolicy::BestEffort),
            ]),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(error_of(&engine, lenient), None);
    let w = warnings_of(&engine, lenient);
    assert!(
        w.iter()
            .any(|w| w.contains("already consumed") && w.contains("dropped")),
        "{w:?}"
    );
    assert_eq!(body_count(&engine, lenient), 1);
    assert_eq!(
        engine.consumed_by.get(&lenient).cloned(),
        Some(vec![c, first])
    );
    engine.remove_feature(lenient, &mut kernel).unwrap();

    // Duplicates and self-reference are refused.
    let dup = engine
        .add_feature(
            "Dup".into(),
            union_selected(vec![
                body_ref(c, OutputKey::Main, ResolvePolicy::Strict),
                body_ref(c, OutputKey::Main, ResolvePolicy::Strict),
            ]),
            &mut kernel,
        )
        .unwrap();
    assert!(error_of(&engine, dup).unwrap().contains("listed twice"));
    engine.remove_feature(dup, &mut kernel).unwrap();
}

#[test]
fn zero_live_bodies_is_loud_and_one_body_passes_through_with_a_warning() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let empty = engine
        .add_feature("Empty".into(), union_all(), &mut kernel)
        .unwrap();
    assert!(error_of(&engine, empty).unwrap().contains("no live bodies"));
    engine.remove_feature(empty, &mut kernel).unwrap();

    let a = add_box(&mut engine, &mut kernel, "A", [0.0; 3]);
    let one = engine
        .add_feature("One".into(), union_all(), &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, one), None);
    assert_eq!(body_count(&engine, one), 1);
    assert!(engine.consumed_features.contains(&a));
    assert!(warnings_of(&engine, one)
        .iter()
        .any(|w| w.contains("only one live body")));
}

#[test]
fn boolean_combine_on_a_consumed_operand_is_loud_with_no_output() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let a = add_box(&mut engine, &mut kernel, "A", [0.0; 3]);
    let b = add_box(&mut engine, &mut kernel, "B", [0.0; 3]);
    let ab = engine
        .add_feature("AB".into(), boolean(a, b), &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, ab), None);
    let c = add_box(&mut engine, &mut kernel, "C", [0.0; 3]);
    // The gearbox defect: `a` was consumed by `ab`; re-targeting it used to
    // duplicate the body silently.
    let stale = engine
        .add_feature("Stale".into(), boolean(a, c), &mut kernel)
        .unwrap();
    let err = error_of(&engine, stale).expect("consumed operand is loud");
    assert!(err.contains("already consumed"), "{err}");
    assert_eq!(body_count(&engine, stale), 0);
    assert!(!engine.consumed_features.contains(&c));
    // The valid form: chain onto the boolean's own output.
    engine.remove_feature(stale, &mut kernel).unwrap();
    let ok = engine
        .add_feature("Chain".into(), boolean(ab, c), &mut kernel)
        .unwrap();
    assert_eq!(error_of(&engine, ok), None);
    assert_eq!(body_count(&engine, ok), 1);
}

#[test]
fn suppress_and_remove_restore_the_sources_to_their_own_features() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let a = add_box(&mut engine, &mut kernel, "A", [0.0; 3]);
    let b = add_box(&mut engine, &mut kernel, "B", [0.0; 3]);
    let u = engine
        .add_feature("Union".into(), union_all(), &mut kernel)
        .unwrap();
    assert!(engine.consumed_features.contains(&a));
    engine.set_suppressed(u, true, &mut kernel).unwrap();
    assert!(!engine.consumed_features.contains(&a) && !engine.consumed_features.contains(&b));
    engine.set_suppressed(u, false, &mut kernel).unwrap();
    assert!(engine.consumed_features.contains(&a));
    engine.remove_feature(u, &mut kernel).unwrap();
    assert!(engine.consumed_features.is_empty());
}

#[test]
fn union_all_json_round_trips_defaults_to_all_and_is_a_known_tag() {
    let all: Operation =
        serde_json::from_value(json!({ "type": "UnionAll", "params": {} })).unwrap();
    assert!(matches!(
        &all,
        Operation::UnionAll {
            params: UnionAllParams {
                targets: UnionTargets::All
            }
        }
    ));
    assert_eq!(all.type_tag(), "UnionAll");
    let v = serde_json::to_value(&all).unwrap();
    assert_eq!(v["params"]["targets"]["type"], "All");
    let fid = Uuid::new_v4();
    let sel = union_selected(vec![body_ref(
        fid,
        OutputKey::Body { index: 1 },
        ResolvePolicy::Strict,
    )]);
    let v = serde_json::to_value(&sel).unwrap();
    assert_eq!(v["params"]["targets"]["type"], "Selected");
    assert_eq!(
        v["params"]["targets"]["bodies"][0]["anchor"]["feature_id"],
        fid.to_string()
    );
    let back: Operation = serde_json::from_value(v).unwrap();
    assert!(matches!(
        back,
        Operation::UnionAll {
            params: UnionAllParams {
                targets: UnionTargets::Selected { .. }
            }
        }
    ));
    assert!(OPERATION_TAGS.contains(&"UnionAll"));
}

#[test]
fn script_union_all_folds_the_scripts_own_bodies() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let src = Uuid::new_v4();
    engine.sources.insert_text(
        src,
        r#"
// @feature name="Three" version=1
// @param plane: plane
fn feature(ctx, p) {
    let a = ctx.sketch(p.plane);
    a.rect(0.0, 0.0, 0.02, 0.02);
    let ba = ctx.extrude(a.finish().regions()[0], #{ depth: 0.01 });
    let b = ctx.sketch(p.plane);
    b.rect(0.005, 0.005, 0.01, 0.01);
    let bb = ctx.extrude(b.finish().regions()[0], #{ depth: 0.02 });
    let c = ctx.sketch(p.plane);
    c.rect(0.0, 0.0, 0.001, 0.001);
    let bc = ctx.extrude(c.finish().regions()[0], #{ depth: 0.03 });
    let ab = ctx.union_all([ba, bb]);
    ctx.union_all()
}
"#,
    );
    let args: BTreeMap<String, serde_json::Value> = serde_json::from_value(
        json!({ "plane": { "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0] } }),
    )
    .unwrap();
    let id = engine
        .add_feature(
            "Three".into(),
            Operation::Script {
                params: ScriptParams {
                    source_id: src,
                    entry: "feature".into(),
                    args,
                    arg_exprs: BTreeMap::new(),
                    arg_values: BTreeMap::new(),
                },
            },
            &mut kernel,
        )
        .unwrap();
    let errs: Vec<_> = engine
        .feature_errors
        .iter()
        .filter(|e| e.feature_id == id)
        .map(|e| e.message.clone())
        .collect();
    assert!(errs.is_empty(), "{errs:?}");
    // ba+bb folded by the selected union, then that lump + bc by the final
    // union-all: exactly ONE body survives.
    assert_eq!(body_count(&engine, id), 1);
}
