//! N-mb-5 dispatch tests — the optional-boolean "combine" model propagated from
//! extrude to *revolve*.
//!
//! These mirror `combine_dispatch.rs` (the extrude cases) revolve-flavored,
//! pinning the behavior the revolve rebuild arm must produce once it dispatches
//! on the normalized combine (`NewBody`/`Add`/`Cut`/`Intersect`, explicit
//! targets, share-a-face auto default, legacy cut/merge preserved) exactly like
//! extrude:
//!   1. Legacy invariance (guard — should already pass).
//!   2. `CombineMode::NewBody` — no boolean, prior body survives.
//!   3. `CombineMode::Add` with an explicit single target — union, target consumed.
//!   4. `CombineMode::Cut` with an explicit single target — subtract, target consumed.
//!   5. `CombineMode::Cut` with an EMPTY explicit target set — loud
//!      `ResolutionFailed`, never a silent standalone body (spec §9).
//!
//! MockKernel revolve is geometry-agnostic (axis/angle are not evaluated for
//! real geometry), so we assert engine-level consumption / body-count / error
//! semantics only, exactly as `combine_dispatch.rs` does for extrude.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

// ── Fixtures ────────────────────────────────────────────────────────────────

/// A simple closed-rectangle sketch on the Z=0 plane (mirrors combine_dispatch.rs).
/// Its profile revolves fine under MockKernel.
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

/// Fully-specified revolve builder so each test can dial `combine`/`targets`
/// plus the legacy `cut`/`merge` bools independently. Axis/angle are fixed
/// (MockKernel revolve is geometry-agnostic).
fn make_revolve(
    sketch_id: Uuid,
    combine: Option<CombineMode>,
    targets: Option<Vec<GeomRef>>,
    cut: bool,
    merge: bool,
) -> Operation {
    Operation::Revolve {
        params: RevolveParams {
            sketch_id,
            profile_index: 0,
            axis_origin: [0.0, 0.0, 0.0],
            axis_direction: [0.0, 1.0, 0.0],
            angle: 360.0,
            cut,
            merge,
            combine,
            targets,
            angle_expr: None,
        },
    }
}

/// Legacy boss revolve (`combine: None`, `merge: true`) — the pre-N-mb-5 default.
fn make_revolve_legacy(sketch_id: Uuid) -> Operation {
    make_revolve(sketch_id, None, None, false, true)
}

/// A `GeomRef` targeting a prior feature's `Main` body output.
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

/// True iff a rebuild error for `feature_id` mentions "resolution failed".
fn has_resolution_failed(engine: &Engine, feature_id: Uuid) -> bool {
    engine
        .errors
        .iter()
        .any(|(id, msg)| *id == feature_id && msg.to_lowercase().contains("resolution failed"))
}

/// Build s1 → r1 (legacy boss revolve) and return `(engine, kernel, r1_id)`
/// with one live body.
fn one_body(name: &str) -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s1 = engine
        .add_feature(format!("{name} Sketch"), make_sketch_op(), &mut kernel)
        .unwrap();
    let r1 = engine
        .add_feature(
            format!("{name} Revolve"),
            make_revolve_legacy(s1),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.get_result(r1).is_some());
    (engine, kernel, r1)
}

// ── Case 1: legacy invariance (GUARD — should already be GREEN) ──────────────

#[test]
fn revolve_legacy_merge_consumes_prior() {
    let (mut engine, mut kernel, r1) = one_body("A");

    // Second sketch + legacy boss revolve (combine=None, merge=true).
    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let r2 = engine
        .add_feature(
            "B Revolve".to_string(),
            make_revolve_legacy(s2),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(r2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // Legacy auto-union: the prior body is consumed and only one body remains.
    assert!(
        engine.consumed_features.contains(&r1),
        "legacy boss revolve must consume the prior solid"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "legacy auto-union yields a single merged body"
    );
}

// ── Case 2: NewBody — no boolean, prior body survives ────────────────────────

#[test]
fn revolve_newbody_leaves_prior_live() {
    let (mut engine, mut kernel, r1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    // combine=NewBody, but legacy merge=true is *also* set to prove the dispatch
    // honors `combine`, not the stale legacy bool.
    let r2 = engine
        .add_feature(
            "B Revolve".to_string(),
            make_revolve(s2, Some(CombineMode::NewBody), None, false, true),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(r2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // NewBody performs no boolean → the prior feature is NOT consumed.
    assert!(
        !engine.consumed_features.contains(&r1),
        "NewBody must not consume the prior body"
    );
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "NewBody yields two independent bodies"
    );
}

// ── Case 3: Add, explicit single target — union, target consumed ─────────────

#[test]
fn revolve_add_explicit_target_consumes_it() {
    let (mut engine, mut kernel, r1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    // combine=Add, explicit target = r1's Main body. Legacy bools deliberately
    // OFF so the dispatch is driven purely by `combine`.
    let r2 = engine
        .add_feature(
            "B Revolve".to_string(),
            make_revolve(
                s2,
                Some(CombineMode::Add),
                Some(vec![body_target(r1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(r2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    assert!(
        engine.consumed_features.contains(&r1),
        "Add must consume its explicit target"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "Add into a single target merges into one body"
    );
}

// ── Case 4: Cut, explicit single target — subtract, target consumed ──────────

#[test]
fn revolve_cut_explicit_target_consumes_it() {
    let (mut engine, mut kernel, r1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let r2 = engine
        .add_feature(
            "B Revolve".to_string(),
            make_revolve(
                s2,
                Some(CombineMode::Cut),
                Some(vec![body_target(r1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(r2).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    assert!(
        engine.consumed_features.contains(&r1),
        "Cut must consume its explicit target"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "Cut from a single target yields one (cut) body"
    );
}

// ── Case 5: Cut with EMPTY targets — loud error, no body ─────────────────────

#[test]
fn revolve_cut_empty_targets_is_loud_error() {
    let (mut engine, mut kernel, _r1) = one_body("A");

    let s2 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    // combine=Cut with an explicit EMPTY target set ⇒ resolves to ∅ targets.
    let r2 = engine
        .add_feature(
            "B Revolve".to_string(),
            make_revolve(s2, Some(CombineMode::Cut), Some(vec![]), false, false),
            &mut kernel,
        )
        .unwrap();

    // Spec §9: a cut that cuts nothing must STOP loudly, never masquerade as a
    // standalone boss body.
    assert!(
        engine.get_result(r2).is_none(),
        "Cut-into-nothing must not emit a standalone body"
    );
    assert!(
        has_resolution_failed(&engine, r2),
        "Cut with empty targets must raise ResolutionFailed; errors: {:?}",
        engine.errors
    );
}
