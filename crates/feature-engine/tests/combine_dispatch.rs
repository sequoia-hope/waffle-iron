//! N-mb-2 dispatch tests — explicit per-extrude boolean-combine choice wired
//! into the rebuild dispatch.
//!
//! These pin the *behavior* the N-mb-2 dispatch must produce once it consumes
//! `normalize_extrude_combine(&ExtrudeParams)` instead of the raw `cut`/`merge`
//! bools. Only the cases N-mb-2 owns are covered here:
//!   1. Legacy invariance (guard — should already pass).
//!   2. `CombineMode::NewBody` — no boolean, prior body survives.
//!   3. `CombineMode::Add` with an explicit single target — union, target consumed.
//!   4. `CombineMode::Cut` with an explicit single target — subtract, target consumed.
//!   5. `CombineMode::Cut`/`Intersect` with an EMPTY explicit target set — loud
//!      `ResolutionFailed`, never a silent standalone body (spec §9).
//!
//! ShareAFace defaults and multi-target Explicit lists are DEFERRED to N-mb-3
//! and are intentionally NOT tested here.
//!
//! Observation model (public engine surface):
//!   - Consumption: `engine.consumed_features: HashSet<Uuid>` — a consumed
//!     feature's body is not separately renderable.
//!   - Renderable body count: sum of Main/Body outputs over active,
//!     non-suppressed, non-consumed features (see `renderable_body_count`).
//!   - Rebuild errors: collected into `engine.errors: Vec<(Uuid, String)>`
//!     (stringified via `EngineError`'s `Display`), NOT returned from
//!     `add_feature`. `EngineError::ResolutionFailed` renders as
//!     "GeomRef resolution failed: ..." — asserted by substring here.
//!
//! MockKernel booleans are deterministic; we assert engine-level
//! consumption / body-count / error semantics (kernel-independent), NOT real
//! geometric volume.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

// ── Fixtures ────────────────────────────────────────────────────────────────

/// A simple closed-rectangle sketch on the Z=0 plane (mirrors engine_tests.rs).
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

/// Fully-specified extrude builder so each test can dial `combine`/`targets`
/// plus the legacy `cut`/`merge` bools independently.
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
            depth_expr: None,
        },
    }
}

/// Legacy boss extrude (`combine: None`, `merge: true`) — the pre-N-mb-2 default.
fn make_extrude_legacy(sketch_id: Uuid) -> Operation {
    make_extrude(sketch_id, 5.0, None, None, false, true)
}

/// A `GeomRef` targeting a prior extrude feature's `Main` body output.
/// `find_solid_handle` only inspects the anchor, so kind/selector/policy are
/// any-valid.
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
/// non-suppressed, non-consumed features. Sketches contribute 0 (empty result).
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

/// True iff a rebuild error for `feature_id` mentions "resolution failed"
/// (the `Display` text of `EngineError::ResolutionFailed`).
fn has_resolution_failed(engine: &Engine, feature_id: Uuid) -> bool {
    engine
        .errors
        .iter()
        .any(|(id, msg)| *id == feature_id && msg.to_lowercase().contains("resolution failed"))
}

/// Build s1 → e1 and return `(engine, kernel, e1_id)` with one live body.
fn one_body(name: &str) -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s1 = engine
        .add_feature(format!("{name} Sketch"), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            format!("{name} Extrude"),
            make_extrude_legacy(s1),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.get_result(e1).is_some());
    (engine, kernel, e1)
}

// ── Case 1: legacy invariance (GUARD — should already be GREEN) ──────────────

#[test]
fn legacy_boss_extrude_still_auto_unions_and_consumes_prior() {
    let (mut engine, mut kernel, e1) = one_body("A");

    // Second sketch + legacy boss extrude (combine=None, merge=true).
    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude_legacy(s2),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // Legacy auto-union: the prior body is consumed and only one body remains.
    assert!(
        engine.consumed_features.contains(&e1),
        "legacy boss extrude must consume the prior solid"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "legacy auto-union yields a single merged body"
    );
}

// ── Case 2: NewBody — no boolean, prior body survives (RED) ──────────────────

#[test]
fn newbody_does_not_boolean_and_leaves_prior_body_live() {
    let (mut engine, mut kernel, e1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    // combine=NewBody, but legacy merge=true is *also* set on the struct to prove
    // the dispatch honors `combine`, not the stale legacy bool.
    let e2 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s2, 5.0, Some(CombineMode::NewBody), None, false, true),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // NewBody performs no boolean → the prior feature is NOT consumed.
    assert!(
        !engine.consumed_features.contains(&e1),
        "NewBody must not consume the prior body"
    );
    // Two independent bodies result.
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "NewBody yields two independent bodies"
    );
}

// ── Case 3: Add, explicit single target — union, target consumed (RED) ───────

#[test]
fn add_explicit_single_target_unions_and_consumes_target() {
    let (mut engine, mut kernel, e1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    // combine=Add, explicit target = e1's Main body. Legacy bools deliberately
    // OFF so the *current* (legacy) dispatch would leave e1 standalone — the
    // divergence that makes this RED until N-mb-2 honors `combine`.
    let e2 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Add),
                Some(vec![body_target(e1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // The explicit target is unioned into and consumed.
    assert!(
        engine.consumed_features.contains(&e1),
        "Add must consume its explicit target"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "Add into a single target merges into one body"
    );
}

// ── Case 4: Cut, explicit single target — subtract, target consumed (RED) ────

#[test]
fn cut_explicit_single_target_subtracts_and_consumes_target() {
    let (mut engine, mut kernel, e1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Cut),
                Some(vec![body_target(e1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // The target is replaced by its cut version; the tool is consumed into it.
    assert!(
        engine.consumed_features.contains(&e1),
        "Cut must consume its explicit target"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "Cut from a single target yields one (cut) body"
    );
}

// ── Case 5: Cut/Intersect with EMPTY targets — loud error, no body (RED) ─────

#[test]
fn cut_with_empty_targets_is_loud_resolution_failed_not_a_body() {
    let (mut engine, mut kernel, _e1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    // combine=Cut with an explicit EMPTY target set ⇒ resolves to ∅ targets.
    let e2 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s2, 5.0, Some(CombineMode::Cut), Some(vec![]), false, false),
            &mut kernel,
        )
        .unwrap();

    // Spec §9: a cut that cuts nothing must STOP loudly, never masquerade as a
    // standalone boss body.
    assert!(
        engine.get_result(e2).is_none(),
        "Cut-into-nothing must not emit a standalone body"
    );
    assert!(
        has_resolution_failed(&engine, e2),
        "Cut with empty targets must raise ResolutionFailed; errors: {:?}",
        engine.errors
    );
}

#[test]
fn intersect_with_empty_targets_is_loud_resolution_failed_not_a_body() {
    let (mut engine, mut kernel, _e1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Intersect),
                Some(vec![]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(e2).is_none(),
        "Intersect-into-nothing must not emit a standalone body"
    );
    assert!(
        has_resolution_failed(&engine, e2),
        "Intersect with empty targets must raise ResolutionFailed; errors: {:?}",
        engine.errors
    );
}
