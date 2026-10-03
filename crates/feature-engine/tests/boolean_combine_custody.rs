//! Custody of the bodies a pair boolean consumes but never names — assay rows
//! P0010 / P0011 (2026-10-03, `docs/yang_tail_triage.md`).
//!
//! `Operation::BooleanCombine` names exactly ONE output per operand
//! (`body_a`/`body_b` carry an `OutputKey`), yet `find_consumed_feature_ids`
//! marks both operand FEATURES consumed whole. A multi-output operand feature
//! therefore lost every output the boolean did not name: no error, no warning,
//! watertight leftovers. P0010 lost 64 % of the model that way and P0011 25 %.
//!
//! The invariant these pin: **a boolean feature's live body set is
//! `(input set − named operands) ∪ result`** — nothing is hidden that no
//! boolean touched. It is the same custody rule the legacy most-recent path
//! (`find_most_recent_solid_outputs`) and the explicit-target combines
//! (F9, `combine_sibling_outputs.rs`) already follow; `UnionAll{Selected}` had
//! the identical hole and is pinned here too.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

fn make_sketch_op() -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (1.0, 0.0));
    solved_positions.insert(3, (1.0, 1.0));
    solved_positions.insert(4, (0.0, 1.0));
    let point = |id, x, y| SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    };
    Operation::Sketch {
        sketch: Sketch {
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
            entities: vec![
                point(1, 0.0, 0.0),
                point(2, 1.0, 0.0),
                point(3, 1.0, 1.0),
                point(4, 0.0, 1.0),
            ],
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
        },
    }
}

fn make_extrude(sketch_id: Uuid, combine: CombineMode, targets: Option<Vec<GeomRef>>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: Some(combine),
            targets,
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth: 5.0,
            direction: None,
            symmetric: false,
            cut: matches!(combine, CombineMode::Cut),
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

fn body_ref(feature_id: Uuid, output_key: OutputKey) -> GeomRef {
    GeomRef {
        kind: TopoKind::Solid,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn union_op(a: GeomRef, b: GeomRef) -> Operation {
    Operation::BooleanCombine {
        params: BooleanParams {
            body_a: a,
            body_b: b,
            operation: BooleanOp::Union,
        },
    }
}

/// The live body set: every Main/Body output of every active, unsuppressed,
/// unconsumed feature — what the renderer and the assay's volume sum walk.
fn live_body_count(engine: &Engine) -> usize {
    engine
        .tree
        .active_features()
        .iter()
        .filter(|f| !f.suppressed)
        .filter(|f| !engine.consumed_features.contains(&f.id))
        .filter_map(|f| engine.get_result(f.id))
        .map(|r| {
            r.outputs
                .iter()
                .filter(|(k, _)| matches!(k, OutputKey::Main | OutputKey::Body { .. }))
                .count()
        })
        .sum()
}

fn add_body(engine: &mut Engine, kernel: &mut MockKernel, name: &str) -> Uuid {
    let s = engine
        .add_feature(format!("{name} Sketch"), make_sketch_op(), kernel)
        .unwrap();
    engine
        .add_feature(
            format!("{name} Extrude"),
            make_extrude(s, CombineMode::NewBody, None),
            kernel,
        )
        .unwrap()
}

/// A feature with TWO solid outputs: a Cut with two explicit targets yields one
/// result body per target (spec §4.2). The P0010/P0011 operand shape, reached
/// without needing a kernel whose union can split.
fn two_output_feature(engine: &mut Engine, kernel: &mut MockKernel) -> Uuid {
    let a = add_body(engine, kernel, "A");
    let b = add_body(engine, kernel, "B");
    let ts = engine
        .add_feature("Tool Sketch".into(), make_sketch_op(), kernel)
        .unwrap();
    let cut = engine
        .add_feature(
            "Two-target Cut".into(),
            make_extrude(
                ts,
                CombineMode::Cut,
                Some(vec![
                    body_ref(a, OutputKey::Main),
                    body_ref(b, OutputKey::Main),
                ]),
            ),
            kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(
        engine.get_result(cut).expect("cut result").outputs.len(),
        2,
        "a two-target Cut yields two outputs"
    );
    cut
}

/// P0010/P0011: a Union naming ONE output of a two-output operand feature must
/// carry the other, not hide it. Before the fix the live set went 3 → 1.
#[test]
fn union_naming_one_output_keeps_the_untargeted_sibling() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let cut = two_output_feature(&mut engine, &mut kernel);
    let standalone = add_body(&mut engine, &mut kernel, "C");
    assert_eq!(live_body_count(&engine), 3, "two cut bodies + standalone");

    let u = engine
        .add_feature(
            "Union".into(),
            union_op(
                body_ref(cut, OutputKey::Main),
                body_ref(standalone, OutputKey::Main),
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    // (3 inputs − 2 named operands) ∪ 1 result body = 2.
    assert_eq!(
        live_body_count(&engine),
        2,
        "the union's own result plus the untargeted sibling of its operand"
    );
    let result = engine.get_result(u).expect("union result");
    assert_eq!(
        result.outputs.len(),
        2,
        "the union carries its result AND the sibling it consumed but never named"
    );
    assert!(
        result
            .diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("not targeted")),
        "carrying a sibling is reported, never silent: {:?}",
        result.diagnostics.warnings
    );
}

/// The carried sibling keeps its custom name (F9b for the boolean path): the
/// name must follow the BODY, not the output slot it lands in.
#[test]
fn carried_sibling_keeps_its_custom_name() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let cut = two_output_feature(&mut engine, &mut kernel);
    let standalone = add_body(&mut engine, &mut kernel, "C");
    engine.rename_body(
        FeatureTree::body_id(cut, &OutputKey::Main),
        "Top tube".into(),
    );
    engine.rename_body(
        FeatureTree::body_id(cut, &OutputKey::Body { index: 1 }),
        "Down tube".into(),
    );

    let u = engine
        .add_feature(
            "Union".into(),
            union_op(
                body_ref(cut, OutputKey::Main),
                body_ref(standalone, OutputKey::Main),
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    let name = |key: OutputKey| {
        engine
            .display_body_name_override(&FeatureTree::body_id(u, &key))
            .map(str::to_string)
    };
    assert_eq!(
        name(OutputKey::Main),
        Some("Top tube".into()),
        "the result inherits the name of the body_a output it actually unioned"
    );
    assert_eq!(
        name(OutputKey::Body { index: 1 }),
        Some("Down tube".into()),
        "the carried sibling keeps its own name"
    );
}

/// A Subtract names one output of each operand too — the custody rule is the
/// op's, not the Union's.
#[test]
fn subtract_naming_one_output_keeps_the_untargeted_sibling() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let cut = two_output_feature(&mut engine, &mut kernel);
    let tool = add_body(&mut engine, &mut kernel, "C");

    engine
        .add_feature(
            "Subtract".into(),
            Operation::BooleanCombine {
                params: BooleanParams {
                    body_a: body_ref(cut, OutputKey::Body { index: 1 }),
                    body_b: body_ref(tool, OutputKey::Main),
                    operation: BooleanOp::Subtract,
                },
            },
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(
        live_body_count(&engine),
        2,
        "the subtract result plus the untargeted Main of its target feature"
    );
}

/// `UnionAll{Selected}` had the identical hole: it names a body list but
/// consumes those features whole.
#[test]
fn union_all_selected_keeps_the_untargeted_sibling() {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let cut = two_output_feature(&mut engine, &mut kernel);
    let standalone = add_body(&mut engine, &mut kernel, "C");

    engine
        .add_feature(
            "Union All".into(),
            Operation::UnionAll {
                params: UnionAllParams {
                    targets: UnionTargets::Selected {
                        bodies: vec![
                            body_ref(cut, OutputKey::Main),
                            body_ref(standalone, OutputKey::Main),
                        ],
                    },
                },
            },
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(
        live_body_count(&engine),
        2,
        "the fold's result plus the untargeted sibling of a named feature"
    );
}

/// The invariant itself, over a family of models: for any number of extra
/// standalone bodies, a Union's live set is `(inputs − 2 named operands) ∪ 1
/// result body`. MockKernel's union always yields one body, so the expected
/// count is exact.
#[test]
fn boolean_output_set_is_inputs_minus_operands_plus_result() {
    for extra in 1..=4usize {
        let mut engine = Engine::new();
        let mut kernel = MockKernel::new();
        let cut = two_output_feature(&mut engine, &mut kernel);
        let mut standalones = Vec::new();
        for i in 0..extra {
            standalones.push(add_body(&mut engine, &mut kernel, &format!("S{i}")));
        }
        let inputs = live_body_count(&engine);
        assert_eq!(
            inputs,
            2 + extra,
            "setup: {extra} standalone(s) + 2 cut bodies"
        );

        // Name one output of the multi-output feature and one standalone.
        engine
            .add_feature(
                "Union".into(),
                union_op(
                    body_ref(cut, OutputKey::Body { index: 1 }),
                    body_ref(standalones[0], OutputKey::Main),
                ),
                &mut kernel,
            )
            .unwrap();
        assert!(engine.errors.is_empty(), "{:?}", engine.errors);
        assert_eq!(
            live_body_count(&engine),
            inputs - 2 + 1,
            "extra={extra}: a boolean's live set is (inputs − named operands) ∪ result"
        );
    }
}
