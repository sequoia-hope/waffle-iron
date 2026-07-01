//! N-mb-3b share-a-face default target — REAL-geometry integration (kernel-v2).
//! Spec §4.3(b): a body auto-merges when it has a planar face COINCIDENT with the
//! sketch plane AND OVERLAPPING the profile footprint — even when the sketch is
//! drawn on a DATUM (no anchor lineage to that body). This is precisely the case
//! the N-mb-3a anchor path CANNOT catch, so it proves the geometric half.
//!
//! Scenario:
//!   1. Body A: square (0,0)-(2,2) extruded z=0..2. Its TOP face is z=2, +z,
//!      covering [0,2]×[0,2].
//!   2. Second sketch on a DATUM at z=2 (NOT anchored to A), square (0.5,0.5)-
//!      (1.5,1.5) overlapping A's top.
//!   3. Extrude with combine=Some(Add), targets=None (Auto).
//!   ⇒ A must be geometrically auto-merged (consumed) into ONE body.
//! Control: datum at z=100 (not coincident) ⇒ A NOT consumed, TWO bodies.
//!
//! Driven with raw `wasm_bridge::dispatch` on a shared `EngineState` + one
//! `kernel_v2` adapter, because `ModelBuilder::extrude` hard-codes the legacy
//! `combine: None` path and offers no `Some(CombineMode::Add)` builder. Reading
//! `state.engine` observables (consumed_features / renderable body count /
//! errors) keeps the assertions kernel-independent.
//!
//! RED expectation: `combine: Some(Add)` + `targets: None` (ShareAFace) currently
//! resolves to ∅ on a datum sketch (only the 3a anchor signal exists), so the Add
//! yields a STANDALONE body — A is NOT consumed and TWO bodies remain. The
//! geometric assertions below therefore FAIL until N-mb-3b lands.

use feature_engine::types::*;
use modeling_ops::KernelBundle;
use test_harness::helpers::rect_profile;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::{dispatch, EngineState, UiToEngine};

// ── Raw-dispatch helpers ─────────────────────────────────────────────────────

fn new_kernel() -> Box<dyn KernelBundle> {
    Box::new(kernel_v2::adapter::KernelV2Adapter::new())
}

/// The id of the most recently appended feature.
fn last_feature_id(state: &EngineState) -> Uuid {
    state
        .engine
        .tree
        .features
        .last()
        .expect("a feature was just added")
        .id
}

/// Build a rectangular sketch on a plain DATUM plane at (origin, normal) with the
/// footprint `(x,y)-(x+w,y+h)`. Returns the sketch feature id. The datum anchor
/// means there is NO `Anchor::FeatureOutput` lineage — the 3a anchor path cannot
/// see any body through it.
fn datum_rect_sketch(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    origin: [f64; 3],
    normal: [f64; 3],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Uuid {
    let plane = GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
    };
    dispatch(state, UiToEngine::BeginSketch { plane }, kernel);

    let (entities, positions, profiles) = rect_profile(x, y, w, h);
    for entity in entities {
        dispatch(state, UiToEngine::AddSketchEntity { entity }, kernel);
    }
    dispatch(
        state,
        UiToEngine::FinishSketch {
            solved_positions: positions,
            solved_profiles: profiles,
            plane_origin: origin,
            plane_normal: normal,
            entities: vec![],
            constraints: vec![],
            projected: vec![],
        },
        kernel,
    );
    last_feature_id(state)
}

/// Add an extrude with explicit `combine` / `targets`. Returns the extrude id.
fn add_extrude(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    sketch_id: Uuid,
    depth: f64,
    combine: Option<CombineMode>,
    targets: Option<Vec<GeomRef>>,
) -> Uuid {
    dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    combine,
                    targets,
                    sketch_id,
                    profile_index: 0,
                    depth,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: false,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                },
            },
        },
        kernel,
    );
    last_feature_id(state)
}

/// Count independent renderable bodies (mirror of the observation model in
/// feature-engine's `share_a_face.rs`: unconsumed, non-suppressed active features
/// with `Main`/`Body{}` outputs).
fn renderable_body_count(state: &EngineState) -> usize {
    let engine = &state.engine;
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

/// Build body A (square 0,0-2,2 extruded z=0..2, legacy standalone) and the
/// combine=Add datum-sketch extrude at `datum_z`. Returns `(state, a_id)`.
fn build_scenario(datum_z: f64) -> (EngineState, Uuid) {
    let mut state = EngineState::new();
    let mut kernel = new_kernel();

    // Body A on a base datum at z=0, extruded up by 2 ⇒ top face z=2.
    let sa = datum_rect_sketch(
        &mut state,
        kernel.as_mut(),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        0.0,
        0.0,
        2.0,
        2.0,
    );
    let a = add_extrude(&mut state, kernel.as_mut(), sa, 2.0, None, None);
    assert!(
        state.engine.get_result(a).is_some(),
        "body A must build; errors: {:?}",
        state.engine.errors
    );

    // Second sketch on a DATUM at z = datum_z, overlapping A's top footprint.
    let sb = datum_rect_sketch(
        &mut state,
        kernel.as_mut(),
        [0.0, 0.0, datum_z],
        [0.0, 0.0, 1.0],
        0.5,
        0.5,
        1.0,
        1.0,
    );
    // Auto default: combine=Add, targets=None ⇒ ShareAFace (geometric).
    let _e = add_extrude(
        &mut state,
        kernel.as_mut(),
        sb,
        2.0,
        Some(CombineMode::Add),
        None,
    );

    (state, a)
}

// ── Case: coincident datum sketch ⇒ geometric auto-merge (RED) ───────────────

#[test]
fn coincident_datum_extrude_auto_merges_geometrically() {
    let (state, a) = build_scenario(2.0);

    assert!(
        state.engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        state.engine.errors
    );
    assert!(
        state.engine.consumed_features.contains(&a),
        "geometric share-a-face (§4.3b) must consume body A (coincident + overlapping top face)"
    );
    assert_eq!(
        renderable_body_count(&state),
        1,
        "Auto Add geometrically merging into A yields exactly ONE body"
    );
}

// ── Control: non-coincident datum ⇒ no merge, two bodies ─────────────────────

#[test]
fn far_datum_extrude_does_not_merge() {
    let (state, a) = build_scenario(100.0);

    assert!(
        state.engine.errors.is_empty(),
        "no rebuild errors: {:?}",
        state.engine.errors
    );
    assert!(
        !state.engine.consumed_features.contains(&a),
        "a datum at z=100 is not coincident with A's top ⇒ A must NOT be consumed"
    );
    assert_eq!(
        renderable_body_count(&state),
        2,
        "non-coincident Auto Add yields a standalone body ⇒ TWO bodies total"
    );
}
