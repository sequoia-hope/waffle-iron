//! N-mb-3c dispatch tests — multi-target Cut/Intersect for extrude.
//!
//! Spec `optional_booleans_multibody_extrude.md` §4.2: a Cut/Intersect whose
//! target set resolves to N (>1) bodies produces N result bodies (each a
//! `target_i` boolean tool), with all N targets consumed.
//!
//! These are RED until N-mb-3c lands: today the dispatch returns a loud
//! "multi-target cut/intersect is not yet implemented (N-mb-3)" error when a
//! Cut/Intersect resolves to >1 target, so `get_result(e2)` is None.
//!
//! Single-target Cut/Intersect is covered in combine_dispatch.rs — NOT
//! duplicated here.
//!
//! Observation model mirrors combine_dispatch.rs:
//!   - Consumption: `engine.consumed_features: HashSet<Uuid>`.
//!   - Renderable body count via `renderable_body_count`.
//!   - A feature's own output-body count: entries in
//!     `engine.get_result(id).outputs` whose key is Main | Body{..}.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

// ── Fixtures (copied from combine_dispatch.rs) ───────────────────────────────

/// A simple closed-rectangle sketch on the Z=0 plane.
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
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 1.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 1.0,
                y: 1.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 1.0,
                construction: false,
            },
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
    };
    Operation::Sketch { sketch }
}

/// Fully-specified extrude builder.
#[allow(clippy::too_many_arguments)]
fn make_extrude(
    sketch_id: Uuid,
    depth: f64,
    combine: Option<CombineMode>,
    targets: Option<Vec<GeomRef>>,
    cut: bool,
    merge: bool,
) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine,
            targets,
            sketch_id,
            profile_index: 0,
            depth,
            direction: None,
            symmetric: false,
            cut,
            merge,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
        },
    }
}

/// A `GeomRef` targeting a prior extrude feature's `Main` body output.
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
    }
}

/// Count independent renderable bodies: Main/Body outputs across active,
/// non-suppressed, non-consumed features.
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

/// Count a single feature's own output bodies (Main | Body{..}).
fn own_body_count(engine: &Engine, feature_id: Uuid) -> usize {
    engine
        .get_result(feature_id)
        .map(|r| {
            r.outputs
                .iter()
                .filter(|(k, _)| matches!(k, OutputKey::Main | OutputKey::Body { .. }))
                .count()
        })
        .unwrap_or(0)
}

/// Build two independent bodies (s0→e0, s1→e1) that do NOT merge (NewBody),
/// returning `(engine, kernel, e0, e1)`.
fn two_bodies() -> (Engine, MockKernel, Uuid, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();

    let s0 = engine
        .add_feature("A Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e0 = engine
        .add_feature(
            "A Extrude".to_string(),
            make_extrude(s0, 5.0, Some(CombineMode::NewBody), None, false, false),
            &mut kernel,
        )
        .unwrap();

    let s1 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s1, 5.0, Some(CombineMode::NewBody), None, false, false),
            &mut kernel,
        )
        .unwrap();

    // Sanity: two independent bodies live before the multi-target op.
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors building two bodies: {:?}",
        engine.errors
    );
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "two NewBody extrudes yield two independent bodies"
    );

    (engine, kernel, e0, e1)
}

// ── Case 1: Cut with two explicit targets — two result bodies (RED) ──────────

#[test]
fn cut_two_explicit_targets_yields_two_bodies() {
    let (mut engine, mut kernel, e0, e1) = two_bodies();

    let s2 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Cut),
                Some(vec![body_target(e0), body_target(e1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(e2).is_some(),
        "multi-target Cut must produce a result"
    );
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // Both targets are consumed.
    assert!(
        engine.consumed_features.contains(&e0),
        "Cut must consume target e0"
    );
    assert!(
        engine.consumed_features.contains(&e1),
        "Cut must consume target e1"
    );

    // e2 itself contributes exactly two cut bodies.
    assert_eq!(
        own_body_count(&engine, e2),
        2,
        "multi-target Cut over 2 targets yields 2 result bodies"
    );

    // Total scene: e0,e1 consumed; e2 contributes its 2 bodies.
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "two cut bodies remain renderable"
    );
}

// ── Case 2: Intersect with two explicit targets — two result bodies (RED) ────

#[test]
fn intersect_two_explicit_targets_yields_two_bodies() {
    let (mut engine, mut kernel, e0, e1) = two_bodies();

    let s2 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Intersect),
                Some(vec![body_target(e0), body_target(e1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(e2).is_some(),
        "multi-target Intersect must produce a result"
    );
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    assert!(
        engine.consumed_features.contains(&e0),
        "Intersect must consume target e0"
    );
    assert!(
        engine.consumed_features.contains(&e1),
        "Intersect must consume target e1"
    );

    assert_eq!(
        own_body_count(&engine, e2),
        2,
        "multi-target Intersect over 2 targets yields 2 result bodies"
    );

    assert_eq!(
        renderable_body_count(&engine),
        2,
        "two intersect bodies remain renderable"
    );
}
