//! Agent-link failure F7 (docs/notes/agent_bicycle_session_failures_2026_09_14.md):
//! once one Cut was in the tree, every edit anywhere took > 120 s, because a
//! rebuild re-executed EVERY feature after the edit point — including booleans
//! whose inputs had not changed.
//!
//! Expected: a rebuild re-executes a feature only if its own definition
//! changed, it references a feature that re-executed, or it depends on tree
//! position (legacy "most recent solid" targets, share-a-face, through-all)
//! after something that re-executed. Everything else keeps its result.
//!
//! Oracle: `MockKernel` allocates a fresh solid handle for every operation, so
//! an unchanged raw handle id proves the feature was not re-executed.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

fn make_sketch_op(size: f64) -> Operation {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (size, 0.0));
    solved_positions.insert(3, (size, size));
    solved_positions.insert(4, (0.0, size));
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
                point(2, size, 0.0),
                point(3, size, size),
                point(4, 0.0, size),
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

fn extrude_params(sketch_id: Uuid) -> ExtrudeParams {
    ExtrudeParams {
        combine: Some(CombineMode::NewBody),
        targets: None,
        sketch_id,
        profile_index: 0,
        profile_entity_ids: None,
        depth: 5.0,
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
    }
}

fn new_body(sketch_id: Uuid) -> Operation {
    Operation::Extrude {
        params: extrude_params(sketch_id),
    }
}

fn explicit_cut(sketch_id: Uuid, targets: Vec<GeomRef>) -> Operation {
    let mut params = extrude_params(sketch_id);
    params.combine = Some(CombineMode::Cut);
    params.cut = true;
    params.targets = Some(targets);
    Operation::Extrude { params }
}

/// A pre-"combine" file's cut: the most recent solid is its target.
fn legacy_cut(sketch_id: Uuid) -> Operation {
    let mut params = extrude_params(sketch_id);
    params.combine = None;
    params.cut = true;
    Operation::Extrude { params }
}

fn target(feature_id: Uuid) -> GeomRef {
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
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn handles(engine: &Engine, id: Uuid) -> Vec<u64> {
    engine
        .get_result(id)
        .unwrap_or_else(|| panic!("feature {id} has no result: {:?}", engine.errors))
        .outputs
        .iter()
        .map(|(_, body)| body.handle.raw())
        .collect()
}

struct Frame {
    engine: Engine,
    kernel: MockKernel,
    sketch_a: Uuid,
    body_a: Uuid,
    sketch_b: Uuid,
    body_b: Uuid,
    cut_b: Uuid,
}

/// Two unrelated bodies, then a Cut that names only body B:
/// `[sketch A, body A, sketch B, body B, tool sketch, Cut(B)]`.
fn frame() -> Frame {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let k = &mut kernel;
    let sketch_a = engine
        .add_feature("A".into(), make_sketch_op(1.0), k)
        .unwrap();
    let body_a = engine
        .add_feature("Body A".into(), new_body(sketch_a), k)
        .unwrap();
    let sketch_b = engine
        .add_feature("B".into(), make_sketch_op(1.0), k)
        .unwrap();
    let body_b = engine
        .add_feature("Body B".into(), new_body(sketch_b), k)
        .unwrap();
    let tool = engine
        .add_feature("Tool".into(), make_sketch_op(0.5), k)
        .unwrap();
    let cut_b = engine
        .add_feature("Cut B".into(), explicit_cut(tool, vec![target(body_b)]), k)
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    Frame {
        engine,
        kernel,
        sketch_a,
        body_a,
        sketch_b,
        body_b,
        cut_b,
    }
}

#[test]
fn editing_an_unrelated_upstream_sketch_does_not_rerun_the_cut() {
    let mut f = frame();
    let cut_before = handles(&f.engine, f.cut_b);
    let body_a_before = handles(&f.engine, f.body_a);

    f.engine
        .edit_feature(f.sketch_a, make_sketch_op(2.0), &mut f.kernel)
        .unwrap();

    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    assert_ne!(
        handles(&f.engine, f.body_a),
        body_a_before,
        "the extrude of the edited sketch re-executes"
    );
    assert_eq!(
        handles(&f.engine, f.cut_b),
        cut_before,
        "a Cut whose inputs did not change must keep its result"
    );
    assert!(f.engine.consumed_features.contains(&f.body_b));
}

#[test]
fn editing_the_cut_target_sketch_reruns_the_cut() {
    let mut f = frame();
    let cut_before = handles(&f.engine, f.cut_b);
    let body_b_before = handles(&f.engine, f.body_b);

    f.engine
        .edit_feature(f.sketch_b, make_sketch_op(2.0), &mut f.kernel)
        .unwrap();

    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    assert_ne!(handles(&f.engine, f.body_b), body_b_before);
    assert_ne!(
        handles(&f.engine, f.cut_b),
        cut_before,
        "the Cut names body B, which re-executed"
    );
}

#[test]
fn a_position_dependent_cut_reruns_after_any_earlier_change() {
    let mut f = frame();
    let tool = f
        .engine
        .add_feature("Tool 2".into(), make_sketch_op(0.25), &mut f.kernel)
        .unwrap();
    let legacy = f
        .engine
        .add_feature("Legacy cut".into(), legacy_cut(tool), &mut f.kernel)
        .unwrap();
    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    let before = handles(&f.engine, legacy);

    f.engine
        .edit_feature(f.sketch_a, make_sketch_op(2.0), &mut f.kernel)
        .unwrap();

    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    assert_ne!(
        handles(&f.engine, legacy),
        before,
        "a most-recent-solid target is found by tree position, so it re-executes"
    );
}

#[test]
fn a_skipped_failing_feature_keeps_its_error() {
    let mut f = frame();
    let tool = f
        .engine
        .add_feature("Tool 2".into(), make_sketch_op(0.25), &mut f.kernel)
        .unwrap();
    let broken = f
        .engine
        .add_feature(
            "Cut of nothing".into(),
            explicit_cut(tool, vec![target(Uuid::new_v4())]),
            &mut f.kernel,
        )
        .unwrap();
    assert!(f.engine.errors.iter().any(|(id, _)| *id == broken));

    f.engine
        .edit_feature(f.sketch_a, make_sketch_op(2.0), &mut f.kernel)
        .unwrap();

    assert!(
        f.engine.errors.iter().any(|(id, _)| *id == broken),
        "the failure is still reported: {:?}",
        f.engine.errors
    );
    assert_eq!(f.engine.errors.len(), f.engine.feature_errors.len());
}

#[test]
fn a_parameter_change_reruns_only_the_features_that_use_it() {
    let mut f = frame();
    let cut_before = handles(&f.engine, f.cut_b);

    let mut op = new_body(f.sketch_a);
    if let Operation::Extrude { params } = &mut op {
        params.depth_expr = Some("depth_a".into());
    }
    f.engine
        .set_parameters(vec![DesignParameter::new("depth_a", "5")], &mut f.kernel);
    f.engine.edit_feature(f.body_a, op, &mut f.kernel).unwrap();
    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    let body_a_before = handles(&f.engine, f.body_a);

    f.engine
        .set_parameters(vec![DesignParameter::new("depth_a", "7")], &mut f.kernel);

    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    assert_ne!(handles(&f.engine, f.body_a), body_a_before);
    assert_eq!(handles(&f.engine, f.cut_b), cut_before);
}

#[test]
fn undoing_a_rename_reruns_nothing() {
    let mut f = frame();
    let body_a_before = handles(&f.engine, f.body_a);
    let cut_before = handles(&f.engine, f.cut_b);

    f.engine.rename_feature(f.body_a, "Renamed".into()).unwrap();
    f.engine.undo(&mut f.kernel).unwrap();

    assert_eq!(handles(&f.engine, f.body_a), body_a_before);
    assert_eq!(handles(&f.engine, f.cut_b), cut_before);
}

#[test]
fn undo_and_redo_of_an_edit_restore_the_same_topology() {
    let mut f = frame();
    let cut_before = handles(&f.engine, f.cut_b);

    f.engine
        .edit_feature(f.sketch_b, make_sketch_op(2.0), &mut f.kernel)
        .unwrap();
    f.engine.undo(&mut f.kernel).unwrap();
    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    assert_ne!(
        handles(&f.engine, f.cut_b),
        cut_before,
        "undo re-executes the Cut"
    );
    assert!(f.engine.consumed_features.contains(&f.body_b));

    f.engine.redo(&mut f.kernel).unwrap();
    assert!(f.engine.errors.is_empty(), "{:?}", f.engine.errors);
    assert!(f.engine.consumed_features.contains(&f.body_b));
}

#[test]
fn a_full_rebuild_reexecutes_everything() {
    let mut f = frame();
    let before = handles(&f.engine, f.cut_b);
    f.engine.rebuild_from_scratch(&mut f.kernel);
    assert_ne!(handles(&f.engine, f.cut_b), before);
}
