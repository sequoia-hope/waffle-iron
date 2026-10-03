//! N2 §5.3 item 3 over the REAL kernel: a sketch drawn on a model face is
//! pinned to that face's persistent identity at `BeginSketch`, re-resolved on
//! every rebuild, and refused loudly when the face is gone.
//!
//! What a test at this layer shows that a feature-engine one cannot:
//!
//! - the pin happens on the path the UI and the agent actually take
//!   (`BeginSketch` → `SolveSketch` → `FinishSketch`), not by hand;
//! - kernel-v2's content-seeded ids mean the pin SURVIVES an edit to the body
//!   it is on (D0 item 1) — so a refusal is only ever because the face is
//!   really gone, not because the kernel renumbered;
//! - the refusal reaches the host through `ModelUpdated.errors` /
//!   `feature_errors`, which is where a tool result reads it from.
//!
//! The model is the one §5.3 names: a plate, a boss on it, a sketch on the
//! boss's top face, then the boss taken away.

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

fn point(id: u32, x: f64, y: f64) -> SketchEntity {
    SketchEntity::Point {
        id,
        x,
        y,
        construction: false,
    }
}

fn line(id: u32, a: u32, b: u32) -> SketchEntity {
    SketchEntity::Line {
        id,
        start_id: a,
        end_id: b,
        construction: false,
    }
}

fn datum_xy() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::new_v4(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

fn added_id(response: EngineToUi) -> Uuid {
    match response {
        EngineToUi::ModelUpdated {
            feature_id: Some(id),
            errors,
            ..
        } => {
            assert!(errors.is_empty(), "rebuild errors: {errors:?}");
            id
        }
        other => panic!("expected ModelUpdated with an id, got {other:?}"),
    }
}

fn errors_of(response: &EngineToUi) -> Vec<(Uuid, String)> {
    match response {
        EngineToUi::ModelUpdated { errors, .. } => errors.clone(),
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

fn warnings_of(response: &EngineToUi) -> Vec<String> {
    match response {
        EngineToUi::ModelUpdated { warnings, .. } => warnings.clone(),
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

fn feature_errors_of(response: &EngineToUi) -> Vec<FeatureError> {
    match response {
        EngineToUi::ModelUpdated { feature_errors, .. } => feature_errors.clone(),
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

/// A square profile: its entities, its boundary loop's ids, and the solved
/// positions of its corners — the three things `FinishSketch` wants.
type Profile = (Vec<SketchEntity>, Vec<u32>, HashMap<u32, (f64, f64)>);

/// One square profile, `side` wide, its near corner at `(x0, y0)`.
fn square(base: u32, x0: f64, y0: f64, side: f64) -> Profile {
    let corners = [
        (base, x0, y0),
        (base + 1, x0 + side, y0),
        (base + 2, x0 + side, y0 + side),
        (base + 3, x0, y0 + side),
    ];
    let solved: HashMap<u32, (f64, f64)> = corners.iter().map(|&(id, x, y)| (id, (x, y))).collect();
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    let loop_ids = vec![base + 10, base + 11, base + 12, base + 13];
    entities.extend([
        line(loop_ids[0], base, base + 1),
        line(loop_ids[1], base + 1, base + 2),
        line(loop_ids[2], base + 2, base + 3),
        line(loop_ids[3], base + 3, base),
    ]);
    (entities, loop_ids, solved)
}

fn extrude_op(sketch_id: Uuid, loop_ids: &[u32], depth: f64, merge: bool) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(loop_ids.to_vec()),
            depth,
            depth_expr: None,
            direction: None,
            symmetric: false,
            cut: false,
            merge,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine: Some(if merge {
                CombineMode::Add
            } else {
                CombineMode::NewBody
            }),
            targets: None,
        },
    }
}

/// Commit a sketch the way the UI and `sketch_create` do — through
/// `BeginSketch`, which is where the plane face gets pinned.
// Eight inputs, one past clippy's default: this mirrors `FinishSketch`'s
// payload, and grouping them would only rename the message.
#[allow(clippy::too_many_arguments)]
fn commit_sketch(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    plane: GeomRef,
    origin: [f64; 3],
    normal: [f64; 3],
    entities: Vec<SketchEntity>,
    loop_ids: &[u32],
    solved: HashMap<u32, (f64, f64)>,
) -> Uuid {
    dispatch(state, UiToEngine::BeginSketch { plane }, kernel);
    added_id(dispatch(
        state,
        UiToEngine::FinishSketch {
            solved_positions: solved,
            solved_profiles: vec![ClosedProfile {
                entity_ids: loop_ids.to_vec(),
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            }],
            plane_origin: origin,
            plane_normal: normal,
            plane_x_axis: None,
            entities,
            constraints: Vec::new(),
            projected: Vec::new(),
            provenance: None,
        },
        kernel,
    ))
}

fn faces(state: &mut EngineState, kernel: &mut KernelV2Adapter, body_id: &str) -> Vec<ListedFace> {
    wasm_bridge::tessellation_runner::tessellate_missing_meshes(state, kernel);
    match dispatch(
        state,
        UiToEngine::ListFaces {
            body_id: body_id.to_string(),
            filter: None,
        },
        kernel,
    ) {
        EngineToUi::FacesListed { faces, .. } => faces,
        other => panic!("expected FacesListed, got {other:?}"),
    }
}

/// The +Z-facing planar face highest up the z axis — the top of whatever is
/// there — and its area, so a caller can tell the boss top from the plate top.
fn top_face(listed: &[ListedFace]) -> &ListedFace {
    listed
        .iter()
        .filter(|f| f.signature.normal.is_some_and(|n| n[2] > 0.9))
        .max_by(|a, b| {
            let z = |f: &ListedFace| f.signature.centroid.map_or(f64::MIN, |c| c[2]);
            z(a).partial_cmp(&z(b)).unwrap()
        })
        .expect("a body has an upward face")
}

/// A plate 20 mm square and 5 mm thick, a 8 mm boss 4 mm tall merged onto its
/// top, and a sketch on the BOSS's top face. Returns
/// `(boss extrude feature, boss sketch-on-top feature, merged body id)`.
#[allow(clippy::type_complexity)]
fn plate_with_boss_and_a_sketch_on_it(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
) -> (Uuid, Uuid, String) {
    // 1. The plate.
    let (entities, loop_ids, solved) = square(100, 0.0, 0.0, 0.020);
    let plate_sketch = commit_sketch(
        state,
        kernel,
        datum_xy(),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        entities,
        &loop_ids,
        solved,
    );
    let plate = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: extrude_op(plate_sketch, &loop_ids, 0.005, false),
            provenance: None,
        },
        kernel,
    ));
    let plate_body = FeatureTree::body_id(plate, &OutputKey::Main);

    // 2. A boss on the plate's top face — itself a sketch on a face, so the
    //    pin is exercised twice in the model.
    let plate_top = top_face(&faces(state, kernel, &plate_body)).clone();
    let (entities, loop_ids, solved) = square(200, 0.006, 0.006, 0.008);
    let boss_sketch = commit_sketch(
        state,
        kernel,
        plate_top.geom_ref.clone(),
        plate_top.signature.centroid.expect("a centroid"),
        plate_top.signature.normal.expect("a normal"),
        entities,
        &loop_ids,
        solved,
    );
    let boss = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: extrude_op(boss_sketch, &loop_ids, 0.004, true),
            provenance: None,
        },
        kernel,
    ));
    let body = FeatureTree::body_id(boss, &OutputKey::Main);

    // 3. A sketch on the BOSS's top face: the face §5.3's case takes away.
    let boss_top = top_face(&faces(state, kernel, &body)).clone();
    assert!(
        boss_top.signature.centroid.unwrap()[2] > 0.008,
        "the boss top is at z≈9 mm, above the plate top: {:?}",
        boss_top.signature.centroid
    );
    let (entities, loop_ids, solved) = square(300, 0.008, 0.008, 0.004);
    let on_boss = commit_sketch(
        state,
        kernel,
        boss_top.geom_ref.clone(),
        boss_top.signature.centroid.unwrap(),
        boss_top.signature.normal.unwrap(),
        entities,
        &loop_ids,
        solved,
    );
    (boss, on_boss, body)
}

// ─────────────────────────────────────────────────────────────────────────────

/// The pin reaches the document: a sketch committed on a model face carries
/// that face's persistent identity, and a sketch on a datum does not.
#[test]
fn a_sketch_committed_on_a_face_stores_that_face_s_persistent_identity() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (_, on_boss, _) = plate_with_boss_and_a_sketch_on_it(&mut state, &mut kernel);

    let Operation::Sketch { sketch } = &state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id == on_boss)
        .expect("the sketch is in the tree")
        .operation
    else {
        panic!("not a sketch")
    };
    let face = sketch
        .plane_face
        .as_ref()
        .expect("a sketch on a model face pins it");
    assert!(
        matches!(face.target.selector, Selector::Pid { .. }),
        "pinned by persistent id, not by index: {:?}",
        face.target.selector
    );
    assert_eq!(face.target.policy, ResolvePolicy::Strict);
    assert!(
        face.fallback.is_some(),
        "the authored reference is kept as the fallback"
    );
    assert_eq!(face.signature.surface_type.as_deref(), Some("planar"));
    assert!(
        face.signature.centroid.unwrap()[2] > 0.008,
        "the signature records WHERE the face was: {:?}",
        face.signature.centroid
    );
    // `Sketch::plane` keeps the placeholder a local sketch has always carried,
    // so the share-a-face target search is untouched.
    assert!(matches!(sketch.plane.anchor, Anchor::Datum { .. }));

    // A sketch on a datum pins nothing: there is no face to go missing.
    let Operation::Sketch { sketch: plate_sk } = &state.engine.tree.features[0].operation else {
        panic!("the first feature is the plate's sketch")
    };
    assert!(plate_sk.plane_face.is_none());
}

/// The case §5.3 names. Take the boss away and the sketch on its top face
/// REFUSES, naming the face's last-known signature — it does not quietly stay
/// where it was drawn, and it does not land on the plate's top face, which is
/// still right there and is what a `BestEffort` rebind would have found.
#[test]
fn a_sketch_on_a_vanished_boss_top_refuses_and_lands_on_no_other_face() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (boss, on_boss, _) = plate_with_boss_and_a_sketch_on_it(&mut state, &mut kernel);

    let response = dispatch(
        &mut state,
        UiToEngine::DeleteFeature { feature_id: boss },
        &mut kernel,
    );

    let errors = errors_of(&response);
    let mine: Vec<_> = errors.iter().filter(|(id, _)| *id == on_boss).collect();
    assert_eq!(
        mine.len(),
        1,
        "the SKETCH is the feature that failed, not something downstream: {errors:?}"
    );
    let message = &mine[0].1;
    assert!(
        message.contains("the face this sketch is drawn on is gone"),
        "{message}"
    );
    assert!(
        message.contains("It was a planar face"),
        "the refusal names the face that went missing: {message}"
    );
    assert!(
        message.contains("re-attach it to the face you want"),
        "and says what to do: {message}"
    );

    // Typed for a host, with the reference it refused.
    let typed = feature_errors_of(&response);
    let mine = typed
        .iter()
        .find(|e| e.feature_id == on_boss)
        .expect("typed too");
    let ErrorKind::ResolutionFailed {
        reason, reference, ..
    } = &mine.kind
    else {
        panic!("{:?}", mine.kind)
    };
    assert!(reason.is_some(), "classified: {:?}", mine.kind);
    assert_eq!(
        reference.as_ref().map(|r| r.kind),
        Some(TopoKind::Face),
        "{:?}",
        mine.kind
    );

    // And it produced nothing: no body, no silent rebind to the plate's top.
    assert!(
        !state.engine.feature_results.contains_key(&on_boss),
        "a refused sketch produces no result"
    );
}

/// The other side of the rule, and what makes the refusal above meaningful:
/// kernel-v2's content-seeded ids (D0 item 1) survive an edit to the body the
/// face is on. Make the boss shorter and the sketch's face still resolves —
/// the pin is not a tripwire that fires on every parameter change. It reports
/// that the face MOVED, and keeps the frame it was solved in.
#[test]
fn an_edit_to_the_boss_keeps_the_pin_and_reports_the_face_moving() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let (boss, on_boss, _) = plate_with_boss_and_a_sketch_on_it(&mut state, &mut kernel);

    let boss_sketch_id = match &state
        .engine
        .tree
        .features
        .iter()
        .find(|f| f.id == boss)
        .expect("the boss is in the tree")
        .operation
    {
        Operation::Extrude { params } => params.sketch_id,
        other => panic!("{other:?}"),
    };
    let loop_ids = vec![210, 211, 212, 213];
    let response = dispatch(
        &mut state,
        UiToEngine::EditFeature {
            feature_id: boss,
            // 4 mm tall becomes 2 mm: the top face is still the boss's top,
            // 2 mm lower than where the sketch was solved.
            operation: extrude_op(boss_sketch_id, &loop_ids, 0.002, true),
            provenance: None,
        },
        &mut kernel,
    );

    let errors = errors_of(&response);
    assert!(
        !errors.iter().any(|(id, _)| *id == on_boss),
        "the face is still there, so the sketch does not refuse: {errors:?}"
    );
    let warnings = warnings_of(&response);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("the face this sketch is drawn on has moved")),
        "but the move IS reported: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("keeps the frame it was solved in")),
        "with what the engine did about it: {warnings:?}"
    );
}
