//! N-mb-3a share-a-face default target (anchor-ownership half) tests — spec §4.3(a).
//!
//! When an extrude's `combine` is `Some(Add|Cut|Intersect)` and `targets` is
//! `None` (the Auto default), the target set defaults to bodies that share a
//! face with the sketch geometry. N-mb-3a covers the dominant signal: the body
//! whose face the sketch is *drawn on*, detected via the sketch's `plane`
//! GeomRef anchor being `Anchor::FeatureOutput { feature_id, OutputKey::Main }`.
//!
//! Behaviors pinned (all `combine: Some(..)`, `targets: None`):
//!   1. Auto Add merges into the sketch's base body (anchor-owned) — target
//!      consumed, one body remains.
//!   2. Auto Cut subtracts from the sketch's base body — target consumed, one
//!      body remains.
//!   3. Sketch on a plain datum ⇒ Auto resolves to ∅ ⇒ Add yields a standalone
//!      new body (§4.1 "Add ∅ ⇒ standalone"); prior body NOT consumed.
//!   4. Auto Cut with a datum sketch (no shared face) ⇒ ∅ targets ⇒ loud
//!      `ResolutionFailed`, no body (§9).
//!
//! Geometric plane-coincidence (§4.3(b)) and multi-target Cut/Intersect are
//! N-mb-3b / N-mb-3c and are intentionally NOT tested here.
//!
//! RED expectation: the current code makes `targets: None` (ShareAFace) a loud
//! "not yet implemented" stop, so cases 1–3 currently error; case 4 may be
//! incidentally green if that stop already yields an error + no body.
//!
//! Observation model mirrors combine_dispatch.rs (consumed_features / renderable
//! body count / engine.errors substring).

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

// ── Fixtures (copied from combine_dispatch.rs) ───────────────────────────────

/// The 4 solved corner positions + entities shared by both sketch builders.
fn rect_solved_positions() -> std::collections::HashMap<u32, (f64, f64)> {
    let mut solved_positions = std::collections::HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (1.0, 0.0));
    solved_positions.insert(3, (1.0, 1.0));
    solved_positions.insert(4, (0.0, 1.0));
    solved_positions
}

fn rect_entities() -> Vec<SketchEntity> {
    vec![
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
    ]
}

fn rect_profiles() -> Vec<ClosedProfile> {
    vec![ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    }]
}

/// A simple closed-rectangle sketch on the Z=0 plane, anchored to a plain datum.
fn make_sketch_op() -> Operation {
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
        entities: rect_entities(),
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: rect_solved_positions(),
        projected: vec![],
        solved_profiles: rect_profiles(),
    };
    Operation::Sketch { sketch }
}

/// Same geometry as `make_sketch_op`, but the sketch `plane` anchor is
/// `Anchor::FeatureOutput { feature_id: body_feature_id, OutputKey::Main }` —
/// i.e. the sketch is drawn *on* the face of the body produced by that feature.
/// Only the anchor differs; selector/kind/policy are any-valid.
fn make_sketch_on_body(body_feature_id: Uuid) -> Operation {
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::FeatureOutput {
                feature_id: body_feature_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::Strict,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        entities: rect_entities(),
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: rect_solved_positions(),
        projected: vec![],
        solved_profiles: rect_profiles(),
    };
    Operation::Sketch { sketch }
}

/// Fully-specified extrude builder (copied from combine_dispatch.rs).
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

/// Legacy boss extrude (`combine: None`, `merge: true`).
fn make_extrude_legacy(sketch_id: Uuid) -> Operation {
    make_extrude(sketch_id, 5.0, None, None, false, true)
}

/// Count independent renderable bodies (copied from combine_dispatch.rs).
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

/// Build a single legacy boss body `e0` and return `(engine, kernel, e0_id)`.
fn base_body(name: &str) -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s0 = engine
        .add_feature(format!("{name} Sketch"), make_sketch_op(), &mut kernel)
        .unwrap();
    let e0 = engine
        .add_feature(
            format!("{name} Extrude"),
            make_extrude_legacy(s0),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.get_result(e0).is_some());
    (engine, kernel, e0)
}

// ── Case 1: Auto Add merges into the sketch's base body (RED) ────────────────

#[test]
fn auto_add_merges_into_sketch_base_body() {
    let (mut engine, mut kernel, e0) = base_body("A");

    // Sketch s1 drawn ON e0's body face (anchor ownership).
    let s1 = engine
        .add_feature("B Sketch".to_string(), make_sketch_on_body(e0), &mut kernel)
        .unwrap();
    // Auto default: combine=Add, targets=None ⇒ ShareAFace ⇒ resolves to e0.
    let e1 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s1, 5.0, Some(CombineMode::Add), None, false, false),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e1).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // The anchor-owned base body is unioned into and consumed.
    assert!(
        engine.consumed_features.contains(&e0),
        "Auto Add must consume the body the sketch is drawn on"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "Auto Add into the base body merges into one body"
    );
}

// ── Case 2: Auto Cut subtracts from the sketch's base body (RED) ─────────────

#[test]
fn auto_cut_subtracts_from_sketch_base_body() {
    let (mut engine, mut kernel, e0) = base_body("A");

    let s1 = engine
        .add_feature("B Sketch".to_string(), make_sketch_on_body(e0), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s1, 5.0, Some(CombineMode::Cut), None, false, false),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e1).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    assert!(
        engine.consumed_features.contains(&e0),
        "Auto Cut must consume the body the sketch is drawn on"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "Auto Cut from the base body yields one (cut) body"
    );
}

// ── Case 3: datum sketch ⇒ Auto resolves to ∅ ⇒ Add is a standalone body ─────

#[test]
fn auto_add_on_datum_sketch_is_standalone_new_body() {
    let (mut engine, mut kernel, e0) = base_body("A");

    // Sketch on a plain datum — no anchor ownership, no shared face.
    let s1 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s1, 5.0, Some(CombineMode::Add), None, false, false),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.get_result(e1).is_some());
    assert!(
        engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        engine.errors
    );

    // Add with empty auto-target set ⇒ standalone body (§4.1); e0 survives.
    assert!(
        !engine.consumed_features.contains(&e0),
        "Auto Add with no shared face must not consume the prior body"
    );
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "Auto Add resolving to ∅ targets yields a standalone new body (two total)"
    );
}

// ── Case 4: datum sketch + Auto Cut ⇒ ∅ targets ⇒ loud ResolutionFailed ──────

#[test]
fn auto_cut_on_datum_sketch_is_loud_resolution_failed_not_a_body() {
    let (mut engine, mut kernel, _e0) = base_body("A");

    let s1 = engine
        .add_feature("B Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "B Extrude".to_string(),
            make_extrude(s1, 5.0, Some(CombineMode::Cut), None, false, false),
            &mut kernel,
        )
        .unwrap();

    // Cut resolving to ∅ targets must STOP loudly, never emit a boss body (§9).
    assert!(
        engine.get_result(e1).is_none(),
        "Auto Cut with no shared face must not emit a standalone body"
    );
    assert!(
        has_resolution_failed(&engine, e1),
        "Auto Cut resolving to ∅ targets must raise ResolutionFailed; errors: {:?}",
        engine.errors
    );
}
