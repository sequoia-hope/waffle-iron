//! Agent-link failure F9 (docs/notes/agent_bicycle_session_failures_2026_09_14.md):
//! consumption is tracked per FEATURE, so a Cut whose explicit targets name only
//! SOME outputs of a multi-output feature consumed the whole feature — the
//! untargeted sibling bodies silently vanished (no error, no warning).
//!
//! Expected: an untargeted sibling survives, carried by the consuming feature,
//! with a warning (the same custody rule the legacy most-recent path and
//! tool-disjoint Add lumps already follow).

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

fn target(feature_id: Uuid, output_key: OutputKey) -> GeomRef {
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

fn renderable_body_count(engine: &Engine) -> usize {
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

/// Two NewBody boxes, then a two-target Cut → one feature with two outputs.
fn two_output_cut() -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let body = |engine: &mut Engine, kernel: &mut MockKernel, name: &str| {
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
    };
    let e0 = body(&mut engine, &mut kernel, "A");
    let e1 = body(&mut engine, &mut kernel, "B");
    let ts = engine
        .add_feature("Tool Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let cut = engine
        .add_feature(
            "Two-target Cut".into(),
            make_extrude(
                ts,
                CombineMode::Cut,
                Some(vec![
                    target(e0, OutputKey::Main),
                    target(e1, OutputKey::Main),
                ]),
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let outputs = engine.get_result(cut).expect("cut result").outputs.len();
    assert_eq!(outputs, 2, "a two-target Cut yields two outputs");
    assert_eq!(renderable_body_count(&engine), 2);
    (engine, kernel, cut)
}

#[test]
fn cutting_one_output_keeps_the_untargeted_sibling() {
    let (mut engine, mut kernel, cut) = two_output_cut();

    let ts = engine
        .add_feature("Second Tool Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let second = engine
        .add_feature(
            "Cut only Body 1".into(),
            make_extrude(
                ts,
                CombineMode::Cut,
                Some(vec![target(cut, OutputKey::Body { index: 1 })]),
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "the untargeted Main output of the first Cut must survive"
    );
    let result = engine.get_result(second).expect("second cut result");
    assert_eq!(
        result.outputs.len(),
        2,
        "the consuming feature carries the cut body AND the untargeted sibling"
    );
    assert!(
        result
            .diagnostics
            .warnings
            .iter()
            .any(|w| w.contains("not targeted")),
        "carrying a sibling is reported: {:?}",
        result.diagnostics.warnings
    );
}

/// F9b: display names followed the output SLOT — cutting `Body:1` (named
/// "Down tube") gave the result the custom name of the untargeted `Main`
/// ("Top tube"), and the carried sibling lost its name.
#[test]
fn names_follow_the_targeted_body_and_the_carried_sibling() {
    let (mut engine, mut kernel, cut) = two_output_cut();
    engine.rename_body(
        FeatureTree::body_id(cut, &OutputKey::Main),
        "Top tube".into(),
    );
    engine.rename_body(
        FeatureTree::body_id(cut, &OutputKey::Body { index: 1 }),
        "Down tube".into(),
    );

    let ts = engine
        .add_feature("Second Tool Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let second = engine
        .add_feature(
            "Cut only Body 1".into(),
            make_extrude(
                ts,
                CombineMode::Cut,
                Some(vec![target(cut, OutputKey::Body { index: 1 })]),
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    let name = |key: OutputKey| {
        engine
            .display_body_name_override(&FeatureTree::body_id(second, &key))
            .map(str::to_string)
    };
    assert_eq!(
        name(OutputKey::Main).as_deref(),
        Some("Down tube"),
        "the result of cutting the down tube is still the down tube"
    );
    assert_eq!(
        name(OutputKey::Body { index: 1 }).as_deref(),
        Some("Top tube"),
        "the carried sibling keeps its name"
    );
}

#[test]
fn targeting_every_output_carries_nothing_extra() {
    let (mut engine, mut kernel, cut) = two_output_cut();
    let ts = engine
        .add_feature("Second Tool Sketch".into(), make_sketch_op(), &mut kernel)
        .unwrap();
    let second = engine
        .add_feature(
            "Cut both".into(),
            make_extrude(
                ts,
                CombineMode::Cut,
                Some(vec![
                    target(cut, OutputKey::Main),
                    target(cut, OutputKey::Body { index: 1 }),
                ]),
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert_eq!(renderable_body_count(&engine), 2);
    assert_eq!(engine.get_result(second).unwrap().outputs.len(), 2);
}
