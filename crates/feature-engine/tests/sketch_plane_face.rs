//! N2 of `specs/agent_mechanical_design.md` §5.3 item 3: a sketch drawn on a
//! model face re-resolves that face on EVERY rebuild, through the whole ladder,
//! and refuses loudly when it is gone.
//!
//! Before this, a local sketch-on-face kept no record of the face at all — its
//! `Sketch::plane` was a placeholder datum with a freshly minted uuid, and its
//! frame was a pair of cached vectors. Delete the boss it was drawn on and the
//! sketch stayed exactly where it was, extruding into space, with nothing said.
//!
//! What this file pins is the ENGINE half: the three outcomes (silent, warned,
//! refused) and that a refusal names the face's last-known signature. The
//! kernel-v2 half — that the pid survives an edit to the body, so a sketch on a
//! boss top is only ever refused because the top is really gone — is
//! `crates/wasm-bridge/tests/tool_sketch_plane_face.rs`.
//!
//! Mutation-checked 2026-10-03: withdrawing the whole ladder (returning
//! immediately from `resolve_sketch_plane_face`, the pre-N2 behaviour) turns 4
//! of the 6 red, leaving only the two that pin what must NOT change — the
//! untouched-frame and no-pinned-face cases. Withdrawing only the drift report
//! (`drift > f64::MAX`) turns exactly 1 red,
//! `a_face_that_moved_is_reported_and_the_sketch_keeps_its_solved_frame`.

use std::collections::HashMap;

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::{KernelIntrospect, MockKernel};
use waffle_types::*;

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

/// A square sketch on the given frame. `plane_face` is filled in afterwards by
/// [`pin_plane_face`] when the sketch is meant to sit on a model face.
fn square_sketch(origin: [f64; 3], normal: [f64; 3], side: f64) -> Sketch {
    let mut solved_positions = HashMap::new();
    for (id, x, y) in [
        (1, 0.0, 0.0),
        (2, side, 0.0),
        (3, side, side),
        (4, 0.0, side),
    ] {
        solved_positions.insert(id, (x, y));
    }
    Sketch {
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
            policy: ResolvePolicy::BestEffort,
            scope: None,
        },
        plane_origin: origin,
        plane_normal: normal,
        plane_x_axis: None,
        entities: vec![
            point(1, 0.0, 0.0),
            point(2, side, 0.0),
            point(3, side, side),
            point(4, 0.0, side),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: vec![],
        plane_face: None,
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    }
}

fn extrude(sketch_id: Uuid, depth: f64) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index: 0,
            profile_entity_ids: Some(vec![10, 11, 12, 13]),
            depth,
            depth_expr: None,
            direction: None,
            symmetric: false,
            cut: false,
            merge: false,
            target_body: None,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine: Some(CombineMode::NewBody),
            targets: None,
        },
    }
}

/// The positive end cap of `block`, as a tool would author the reference.
fn top_face_ref(block: Uuid) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id: block,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

/// Pin `authored` the way `dispatch::pin_sketch_plane_face` does at
/// `BeginSketch`: the persistent id, the authored reference as the fallback,
/// and the face's signature for the refusal to report.
fn pin_plane_face(engine: &Engine, kernel: &MockKernel, authored: &GeomRef) -> SketchFaceRef {
    let pinned = feature_engine::resolve::pin_identity(authored, &engine.feature_results, kernel)
        .expect("the authored reference resolves at pinning time");
    SketchFaceRef {
        target: pinned.target,
        fallback: pinned.fallback,
        signature: kernel.compute_signature(pinned.kernel_id, TopoKind::Face),
    }
}

/// A plate (one extruded square, depth 0.5) and its feature id.
fn plate() -> (Engine, MockKernel, Uuid) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0),
            },
            &mut kernel,
        )
        .expect("sketch");
    let block = engine
        .add_feature("Plate".into(), extrude(sketch, 0.5), &mut kernel)
        .expect("extrude");
    assert!(
        engine.errors.is_empty(),
        "the fixture must build: {:?}",
        engine.errors
    );
    (engine, kernel, block)
}

/// Add a second sketch pinned to `plate`'s top face, then extrude it. Returns
/// (sketch feature id, extrude feature id).
fn sketch_on(engine: &mut Engine, kernel: &mut MockKernel, block: Uuid) -> (Uuid, Uuid) {
    let face = pin_plane_face(engine, kernel, &top_face_ref(block));
    let (origin, normal) = (
        face.signature.centroid.expect("the mock gives a centroid"),
        face.signature.normal.expect("and a normal"),
    );
    let mut sketch = square_sketch(origin, normal, 0.4);
    sketch.plane_face = Some(face);
    let sid = engine
        .add_feature("Boss sketch".into(), Operation::Sketch { sketch }, kernel)
        .expect("sketch on the face");
    let eid = engine
        .add_feature("Boss".into(), extrude(sid, 0.3), kernel)
        .expect("extrude the boss");
    (sid, eid)
}

fn error_of(engine: &Engine, id: Uuid) -> Option<&FeatureError> {
    engine.feature_errors.iter().find(|e| e.feature_id == id)
}

// ─────────────────────────────────────────────────────────────────────────────

/// Outcome 1: the face is there, where it was. Nothing is reported, and the
/// sketch keeps the frame it was solved in BIT-FOR-BIT — re-deriving an
/// agreeing frame would still move every point by the last few ulps, which is
/// the determinism §5.3 asks to hold.
#[test]
fn a_sketch_on_a_face_that_is_still_there_says_nothing_and_keeps_its_frame() {
    let (mut engine, mut kernel, block) = plate();
    let before = {
        let face = pin_plane_face(&engine, &kernel, &top_face_ref(block));
        (
            face.signature.centroid.unwrap(),
            face.signature.normal.unwrap(),
        )
    };
    let (sid, eid) = sketch_on(&mut engine, &mut kernel, block);

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    assert!(
        !engine
            .warnings
            .iter()
            .any(|w| w.contains("face this sketch")),
        "a face that has not moved is not worth a word: {:?}",
        engine.warnings
    );
    assert!(
        engine.feature_results.contains_key(&eid),
        "and the boss built"
    );

    let Operation::Sketch { sketch } = &engine
        .tree
        .features
        .iter()
        .find(|f| f.id == sid)
        .expect("the sketch is in the tree")
        .operation
    else {
        panic!("not a sketch")
    };
    assert_eq!(
        sketch.plane_origin, before.0,
        "the frame was not re-derived"
    );
    assert_eq!(sketch.plane_normal, before.1);
}

/// Outcome 3, and the case §5.3 names: the face is gone after an upstream
/// edit. The sketch REFUSES, by name, with the face's last-known signature —
/// it is not re-bound to whichever face now scores best, and nothing
/// downstream of it builds.
#[test]
fn a_sketch_whose_face_is_gone_refuses_with_the_face_s_last_known_signature() {
    let (mut engine, mut kernel, block) = plate();
    let (sid, eid) = sketch_on(&mut engine, &mut kernel, block);
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    // Delete the plate: its top face, and every face, is gone.
    engine
        .remove_feature(block, &mut kernel)
        .expect("delete the plate");

    let err = error_of(&engine, sid).expect("the SKETCH is the feature that failed");
    assert!(
        err.message
            .contains("the face this sketch is drawn on is gone"),
        "{}",
        err.message
    );
    assert!(
        err.message.contains("It was a planar face"),
        "the refusal names the face that went missing: {}",
        err.message
    );
    assert!(
        err.message.contains("centroid ["),
        "including where it was: {}",
        err.message
    );
    assert!(
        matches!(err.kind, ErrorKind::ResolutionFailed { .. }),
        "{:?}",
        err.kind
    );
    // And it never silently became a sketch on something else.
    assert!(
        !engine.feature_results.contains_key(&sid),
        "a refused sketch produces no result"
    );
    assert!(
        !engine.feature_results.contains_key(&eid),
        "and nothing downstream of it builds"
    );
}

/// The same refusal reaches a host CLASSIFIED, so an agent branches on the
/// reason rather than reading the sentence (§5.3 item 2 meeting item 3).
#[test]
fn the_sketch_s_refusal_is_classified_for_a_host() {
    let (mut engine, mut kernel, block) = plate();
    let (sid, _) = sketch_on(&mut engine, &mut kernel, block);
    engine
        .remove_feature(block, &mut kernel)
        .expect("delete the plate");

    let err = error_of(&engine, sid).expect("the sketch failed");
    let ErrorKind::ResolutionFailed {
        reason, reference, ..
    } = &err.kind
    else {
        panic!("{:?}", err.kind)
    };
    assert!(
        reason.is_some(),
        "the refusal is classified, not a bare string: {:?}",
        err.kind
    );
    let reference = reference.as_ref().expect("and it names the reference");
    assert_eq!(reference.kind, TopoKind::Face);
    assert_eq!(reference.anchor_feature, Some(block));
}

/// A sketch with no pinned face — a datum sketch, or any sketch authored
/// before N2 — is left exactly as it was: nothing to re-resolve, nothing to
/// report, and it still builds when its anchor feature is deleted (it never
/// had one).
#[test]
fn a_sketch_with_no_pinned_face_is_untouched() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sid = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0),
            },
            &mut kernel,
        )
        .expect("sketch");
    let eid = engine
        .add_feature("Plate".into(), extrude(sid, 0.5), &mut kernel)
        .expect("extrude");
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(engine.feature_results.contains_key(&sid));
    assert!(engine.feature_results.contains_key(&eid));
    assert!(
        !engine
            .warnings
            .iter()
            .any(|w| w.contains("face this sketch")),
        "{:?}",
        engine.warnings
    );
}

/// Outcome 2: the pinned identity is gone but the authored selector still finds
/// a face. That is a REBIND, so it is reported — "may be a different face" —
/// and the sketch still builds, because something plausible did answer.
#[test]
fn a_face_re_found_by_geometry_is_reported_as_a_rebind() {
    let (mut engine, mut kernel, block) = plate();
    let mut face = pin_plane_face(&engine, &kernel, &top_face_ref(block));
    // Break the pid, keep the authored role selector as the fallback: the
    // shape a reopened document takes when the kernel re-minted the number
    // (N1's D0 item 1b note) — the ladder must fall through to the fallback.
    face.target.selector = Selector::Pid {
        pid: u64::MAX,
        root_pid: u64::MAX,
    };
    assert!(face.fallback.is_some(), "the authored selector is kept");

    let (origin, normal) = (
        face.signature.centroid.unwrap(),
        face.signature.normal.unwrap(),
    );
    let mut sketch = square_sketch(origin, normal, 0.4);
    sketch.plane_face = Some(face);
    let sid = engine
        .add_feature(
            "Boss sketch".into(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .expect("the sketch is accepted");

    assert!(
        engine.errors.is_empty(),
        "it still builds: {:?}",
        engine.errors
    );
    assert!(
        engine.feature_results.contains_key(&sid),
        "a rebind is a warning, not a refusal"
    );
    assert!(
        engine
            .warnings
            .iter()
            .any(|w| w.contains("lost its persistent identity")
                && w.contains("may be a different face")),
        "the rebind is reported: {:?}",
        engine.warnings
    );
}

/// Outcome 2, the other half: the face is there but has MOVED. The sketch
/// keeps the frame it was solved in — moving it would move every point of it
/// and every feature downstream — and says so, loudly, naming how far off the
/// face its plane now sits.
#[test]
fn a_face_that_moved_is_reported_and_the_sketch_keeps_its_solved_frame() {
    let (mut engine, mut kernel, block) = plate();
    let face = pin_plane_face(&engine, &kernel, &top_face_ref(block));
    let normal = face.signature.normal.unwrap();
    let centroid = face.signature.centroid.unwrap();
    // The sketch was solved on a plane 2 mm ABOVE the face it names — the
    // state a rebuild reaches when the plate gets thinner and the sketch's
    // cached frame stays where the old top was.
    let stale_origin = [
        centroid[0] + normal[0] * 0.002,
        centroid[1] + normal[1] * 0.002,
        centroid[2] + normal[2] * 0.002,
    ];
    let mut sketch = square_sketch(stale_origin, normal, 0.4);
    sketch.plane_face = Some(face);
    let sid = engine
        .add_feature(
            "Boss sketch".into(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .expect("the sketch is accepted");

    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    assert!(engine.feature_results.contains_key(&sid));
    let moved: Vec<_> = engine
        .warnings
        .iter()
        .filter(|w| w.contains("has moved"))
        .collect();
    assert_eq!(moved.len(), 1, "exactly one report: {:?}", engine.warnings);
    assert!(
        moved[0].contains("2.000e-3"),
        "naming how far off the face it is: {}",
        moved[0]
    );
    assert!(
        moved[0].contains("keeps the frame it was solved in"),
        "and what the engine did about it: {}",
        moved[0]
    );

    // An IN-PLANE difference is not a move: the engine's on-face origin is the
    // face centroid and the UI's is a rendered triangle's centroid, so the two
    // legitimately differ by a translation within the plane.
    let (mut engine, mut kernel, block) = plate();
    let face = pin_plane_face(&engine, &kernel, &top_face_ref(block));
    let normal = face.signature.normal.unwrap();
    let centroid = face.signature.centroid.unwrap();
    let in_plane = [centroid[0] + 0.1, centroid[1] - 0.2, centroid[2]];
    assert!(
        normal[2].abs() > 0.99,
        "the fixture's top face is z-normal, so x/y is in-plane"
    );
    let mut sketch = square_sketch(in_plane, normal, 0.4);
    sketch.plane_face = Some(face);
    engine
        .add_feature(
            "Boss sketch".into(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .expect("the sketch is accepted");
    assert!(
        !engine.warnings.iter().any(|w| w.contains("has moved")),
        "an in-plane origin is the same plane: {:?}",
        engine.warnings
    );
}
