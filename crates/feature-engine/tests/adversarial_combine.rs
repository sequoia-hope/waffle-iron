//! ADVERSARIAL tests for optional-boolean + multi-body-targeting (extrude/revolve).
//!
//! Probes the risk areas the happy-path suites do NOT cover:
//!   - Edit round-trips (Cut→NewBody, retarget) must RECOMPUTE the consumed set,
//!     never leak a stale consumption.
//!   - Deleting / suppressing an explicit target must be loud or honor the
//!     target's `ResolvePolicy` (spec §9) — never panic, never silently
//!     mis-resolve.
//!   - Non-solid explicit targets must be a loud error.
//!   - Multi-target Add folds into ONE body; multi-target Cut yields exactly N.
//!   - Incremental rebuild preserves multi-target consumption.
//!   - Pure share-a-face predicates on pathological inputs (no panic, sane).
//!
//! Observation model mirrors combine_dispatch.rs: engine.consumed_features,
//! renderable_body_count, own_body_count, engine.errors substrings.

use feature_engine::share_a_face::{convex_hull_2d, plane_coincident, polygons_overlap_2d};
use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::*;

// ── Fixtures (mirror combine_dispatch.rs / share_a_face.rs) ───────────────────

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

/// A `GeomRef` targeting a prior feature's `Main` body output, with a chosen policy.
fn body_target_policy(feature_id: Uuid, policy: ResolvePolicy) -> GeomRef {
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
        policy,
    }
}

fn body_target(feature_id: Uuid) -> GeomRef {
    body_target_policy(feature_id, ResolvePolicy::BestEffort)
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

fn has_resolution_failed(engine: &Engine, feature_id: Uuid) -> bool {
    engine
        .errors
        .iter()
        .any(|(id, msg)| *id == feature_id && msg.to_lowercase().contains("resolution failed"))
}

/// Build one NewBody body `(engine, kernel, extrude_id)`.
fn one_body(name: &str) -> (Engine, MockKernel, Uuid) {
    let mut engine = Engine::new();
    let mut kernel = MockKernel::new();
    let s = engine
        .add_feature(format!("{name} Sketch"), make_sketch_op(), &mut kernel)
        .unwrap();
    let e = engine
        .add_feature(
            format!("{name} Extrude"),
            make_extrude(s, 5.0, Some(CombineMode::NewBody), None, false, false),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.get_result(e).is_some());
    (engine, kernel, e)
}

/// Add a NewBody extrude and return its id.
fn add_newbody(engine: &mut Engine, kernel: &mut MockKernel, name: &str) -> Uuid {
    let s = engine
        .add_feature(format!("{name} Sketch"), make_sketch_op(), kernel)
        .unwrap();
    engine
        .add_feature(
            format!("{name} Extrude"),
            make_extrude(s, 5.0, Some(CombineMode::NewBody), None, false, false),
            kernel,
        )
        .unwrap()
}

// ── 2. Edit round-trip: Cut→NewBody brings the consumed target back live ──────

#[test]
fn edit_cut_to_newbody_restores_consumed_target() {
    let (mut engine, mut kernel, e0) = one_body("A");

    let s1 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s1,
                5.0,
                Some(CombineMode::Cut),
                Some(vec![body_target(e0)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    // Precondition: Cut consumed e0.
    assert!(engine.consumed_features.contains(&e0));
    assert_eq!(renderable_body_count(&engine), 1);

    // Edit e1 to NewBody: e0 must come back as a live body (consumed set recomputes).
    engine
        .edit_feature(
            e1,
            make_extrude(s1, 5.0, Some(CombineMode::NewBody), None, false, false),
            &mut kernel,
        )
        .unwrap();

    assert!(
        !engine.consumed_features.contains(&e0),
        "editing Cut→NewBody must un-consume the previously-cut target (no stale leak)"
    );
    assert!(engine.get_result(e1).is_some());
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "e0 restored + e1 standalone = two live bodies"
    );
}

// ── 2b. Edit round-trip: retargeting an Add recomputes which body is consumed ─

#[test]
fn edit_add_retarget_moves_consumption_no_leak() {
    let (mut engine, mut kernel, e0) = one_body("A");
    let e1 = add_newbody(&mut engine, &mut kernel, "B");

    // Tool e2 = Add explicit [e0].
    let s2 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Add),
                Some(vec![body_target(e0)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.consumed_features.contains(&e0));
    assert!(!engine.consumed_features.contains(&e1));

    // Retarget e2 to Add into e1 instead of e0.
    engine
        .edit_feature(
            e2,
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

    assert!(
        !engine.consumed_features.contains(&e0),
        "retarget must release the old target e0 (no leaked consumption)"
    );
    assert!(
        engine.consumed_features.contains(&e1),
        "retarget must consume the new target e1"
    );
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "e0 live + (e1 merged into e2) = two live bodies"
    );
}

// ── 3. Delete an explicit target — Strict errors loud, no panic ───────────────

#[test]
fn delete_strict_explicit_target_is_loud_error() {
    let (mut engine, mut kernel, e0) = one_body("A");

    let s1 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s1,
                5.0,
                Some(CombineMode::Cut),
                Some(vec![body_target_policy(e0, ResolvePolicy::Strict)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.get_result(e1).is_some());

    // Delete the target body feature. e1's Strict target now dangles.
    engine.remove_feature(e0, &mut kernel).unwrap();

    assert!(
        engine.get_result(e1).is_none(),
        "Cut whose Strict target was deleted must not emit a body"
    );
    assert!(
        has_resolution_failed(&engine, e1),
        "deleted Strict target ⇒ loud ResolutionFailed; errors: {:?}",
        engine.errors
    );
}

// ── 3b. Delete ONE of two BestEffort targets — drop it + cut the survivor ─────
//
// Spec §9: "target GeomRef fails to resolve (deleted/rolled-back body) — per
// ResolvePolicy: Strict ⇒ error; BestEffort ⇒ drop that target + warn."
// A valid live target must NOT be lost because an unrelated one vanished.

#[test]
fn delete_one_of_two_besteffort_targets_drops_and_cuts_survivor() {
    let (mut engine, mut kernel, e0) = one_body("A");
    let e1 = add_newbody(&mut engine, &mut kernel, "B");

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
                Some(vec![body_target(e0), body_target(e1)]), // both BestEffort
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();
    assert_eq!(
        own_body_count(&engine, e2),
        2,
        "two targets ⇒ two cut bodies"
    );

    // Delete e1 (one of the two targets).
    engine.remove_feature(e1, &mut kernel).unwrap();

    assert!(
        engine.get_result(e2).is_some(),
        "BestEffort must drop the dead target and still cut the survivor e0"
    );
    assert!(
        !has_resolution_failed(&engine, e2),
        "a BestEffort drop must not fail the whole feature; errors: {:?}",
        engine.errors
    );
    assert_eq!(
        own_body_count(&engine, e2),
        1,
        "one target dropped ⇒ exactly one cut body remains"
    );
    assert!(
        engine.consumed_features.contains(&e0),
        "the surviving target e0 must still be consumed by the cut"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "only the cut body renders"
    );
}

// ── 4. Non-solid explicit target (anchored to a sketch feature) — loud ────────

#[test]
fn non_solid_explicit_target_is_loud_error() {
    let (mut engine, mut kernel, _e0) = one_body("A");

    // s_extra is a SKETCH feature — it produces an OpResult with NO Main output.
    let s_extra = engine
        .add_feature("Extra Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
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
                // Strict so a non-resolvable non-solid is a hard stop, not a drop.
                Some(vec![body_target_policy(s_extra, ResolvePolicy::Strict)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(
        engine.get_result(e2).is_none(),
        "a Cut whose target has no solid output must not emit a body"
    );
    assert!(
        has_resolution_failed(&engine, e2),
        "non-solid target ⇒ loud ResolutionFailed; errors: {:?}",
        engine.errors
    );
}

// ── 5. Multi-target Add folds into ONE body; all targets consumed ─────────────

#[test]
fn add_two_explicit_targets_merges_into_one_body() {
    let (mut engine, mut kernel, e0) = one_body("A");
    let e1 = add_newbody(&mut engine, &mut kernel, "B");

    let s2 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e2 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Add),
                Some(vec![body_target(e0), body_target(e1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    assert!(engine.consumed_features.contains(&e0), "Add consumes e0");
    assert!(engine.consumed_features.contains(&e1), "Add consumes e1");
    assert_eq!(
        own_body_count(&engine, e2),
        1,
        "Add over 2 targets folds tool+targets into ONE body"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "single merged body remains"
    );
}

// ── 5b. Three-target Cut yields exactly 3 bodies, all consumed ────────────────

#[test]
fn cut_three_explicit_targets_yields_three_bodies() {
    let (mut engine, mut kernel, e0) = one_body("A");
    let e1 = add_newbody(&mut engine, &mut kernel, "B");
    let e2 = add_newbody(&mut engine, &mut kernel, "C");

    let s3 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e3 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s3,
                5.0,
                Some(CombineMode::Cut),
                Some(vec![body_target(e0), body_target(e1), body_target(e2)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    for t in [e0, e1, e2] {
        assert!(engine.consumed_features.contains(&t), "Cut consumes {t}");
    }
    assert_eq!(
        own_body_count(&engine, e3),
        3,
        "Cut over 3 targets yields exactly 3 result bodies"
    );
    assert_eq!(renderable_body_count(&engine), 3, "three cut bodies render");
}

// ── 1/regression. Incremental rebuild preserves multi-target Add consumption ──

#[test]
fn incremental_rebuild_preserves_multitarget_consumption() {
    let (mut engine, mut kernel, e0) = one_body("A");
    let e1 = add_newbody(&mut engine, &mut kernel, "B");

    let s2 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let _e2 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s2,
                5.0,
                Some(CombineMode::Add),
                Some(vec![body_target(e0), body_target(e1)]),
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.consumed_features.contains(&e0));
    assert!(engine.consumed_features.contains(&e1));

    // Append an unrelated NewBody LATER — forces an incremental rebuild whose
    // earlier features (incl. the multi-target Add) go through the `i<from_index`
    // consumption-reapplication path. Consumption must survive.
    let e3 = add_newbody(&mut engine, &mut kernel, "D");

    assert!(
        engine.consumed_features.contains(&e0) && engine.consumed_features.contains(&e1),
        "incremental rebuild must preserve BOTH multi-target consumptions"
    );
    assert!(!engine.consumed_features.contains(&e3));
    assert_eq!(
        renderable_body_count(&engine),
        2,
        "merged body + the new standalone = two bodies"
    );
}

// ── 1b. Suppressing an explicit Add target is not silently consumed ───────────

#[test]
fn suppress_besteffort_add_target_degrades_to_standalone() {
    let (mut engine, mut kernel, e0) = one_body("A");

    let s1 = engine
        .add_feature("Tool Sketch".to_string(), make_sketch_op(), &mut kernel)
        .unwrap();
    let e1 = engine
        .add_feature(
            "Tool Extrude".to_string(),
            make_extrude(
                s1,
                5.0,
                Some(CombineMode::Add),
                Some(vec![body_target(e0)]), // BestEffort
                false,
                false,
            ),
            &mut kernel,
        )
        .unwrap();
    assert!(engine.consumed_features.contains(&e0));

    // Suppress the target body. Its result vanishes; the BestEffort Add must
    // NOT silently keep e0 consumed (it isn't merged into anything).
    engine.set_suppressed(e0, true, &mut kernel).unwrap();

    assert!(
        !engine.consumed_features.contains(&e0),
        "a suppressed target must not be recorded as consumed by the Add"
    );
    assert!(
        engine.get_result(e1).is_some(),
        "BestEffort Add with a vanished target degrades to a standalone body"
    );
    assert_eq!(
        renderable_body_count(&engine),
        1,
        "only e1 (standalone) renders; suppressed e0 does not"
    );
}

// ── 6. Pure share-a-face predicate adversarials (no panic, sane results) ──────

#[test]
fn plane_coincident_boundary_and_degenerate() {
    // Zero-length face normal ⇒ cannot normalize ⇒ not coincident (no NaN).
    assert!(!plane_coincident(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
    ));
    // Zero-length sketch normal ⇒ not coincident.
    assert!(!plane_coincident(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0],
    ));
    // Offset just OUTSIDE TAU_MODEL (1e-7) ⇒ not coincident.
    assert!(!plane_coincident(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1e-6],
    ));
    // Offset well within TAU_MODEL ⇒ coincident.
    assert!(plane_coincident(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1e-9],
    ));
}

#[test]
fn polygons_overlap_degenerate_inputs_no_panic() {
    let square = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
    // Fewer than 3 points ⇒ false, never panic.
    assert!(!polygons_overlap_2d(&[], &square));
    assert!(!polygons_overlap_2d(&[[0.0, 0.0]], &square));
    assert!(!polygons_overlap_2d(&[[0.0, 0.0], [1.0, 1.0]], &square));
    // Collinear (zero-area) "polygon" vs a real square ⇒ no positive area ⇒ false.
    let collinear = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]];
    assert!(!polygons_overlap_2d(&collinear, &square));
    assert!(!polygons_overlap_2d(&square, &collinear));
    // A degenerate polygon of 3 coincident points strictly INSIDE the square:
    // the documented conservative heuristic ("vertex strictly inside ⇒ overlap,
    // can only widen the auto-merge") reports overlap. We only require no panic
    // and a stable bool — not a particular answer for a zero-area polygon.
    let dup = vec![[1.0, 1.0], [1.0, 1.0], [1.0, 1.0]];
    let _ = polygons_overlap_2d(&dup, &square);
    // A degenerate polygon of coincident points OUTSIDE the square ⇒ false.
    let dup_out = vec![[9.0, 9.0], [9.0, 9.0], [9.0, 9.0]];
    assert!(!polygons_overlap_2d(&dup_out, &square));
    // Self-touching bowtie vs an overlapping square: must not panic (any bool ok).
    let bowtie = vec![[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]];
    let _ = polygons_overlap_2d(&bowtie, &square);
}

#[test]
fn convex_hull_degenerate_inputs_no_panic() {
    // Empty / single / two points ⇒ returned as-is (deduped), < 3 points.
    assert!(convex_hull_2d(&[]).is_empty());
    assert_eq!(convex_hull_2d(&[[1.0, 1.0]]).len(), 1);
    assert_eq!(convex_hull_2d(&[[0.0, 0.0], [1.0, 1.0]]).len(), 2);
    // All identical points ⇒ dedup to 1.
    let same = vec![[3.0, 3.0], [3.0, 3.0], [3.0, 3.0], [3.0, 3.0]];
    assert_eq!(convex_hull_2d(&same).len(), 1);
    // All collinear points ⇒ hull is degenerate (the 2 extreme points), no panic.
    let line = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
    let h = convex_hull_2d(&line);
    assert!(
        h.len() <= line.len(),
        "collinear hull must not fabricate points"
    );
    // A real square's hull is 4 points regardless of an interior duplicate.
    let sq = vec![
        [0.0, 0.0],
        [2.0, 0.0],
        [2.0, 2.0],
        [0.0, 2.0],
        [1.0, 1.0],
        [1.0, 1.0],
    ];
    assert_eq!(convex_hull_2d(&sq).len(), 4);
}

// ── 8. Mutation sanity: NewBody must NOT consume (guards the empty-consume arm)─
//
// If the NewBody consumption arm were inverted to consume the prior body, this
// test would fail — pinning that NewBody leaves the prior body live.

#[test]
fn newbody_mutation_guard_prior_body_stays_live() {
    let (mut engine, mut kernel, e0) = one_body("A");
    let _e1 = add_newbody(&mut engine, &mut kernel, "B");
    assert!(
        !engine.consumed_features.contains(&e0),
        "NewBody must never consume a prior body"
    );
    assert_eq!(renderable_body_count(&engine), 2);
}
