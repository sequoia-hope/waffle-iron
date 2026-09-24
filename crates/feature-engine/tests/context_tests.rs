//! In-context editing (v4 Phase 3d-4, `feature_engine::context`): scoped
//! `GeomRef`s resolve through the engine's edit context into the edited
//! part's frame — a sketch plane re-derives from another instance's face, an
//! up-to depth reaches another instance's face — and, without a context, are
//! loud and inert rather than silently resolved against the wrong part.

use std::collections::HashMap;

use feature_engine::context::{ContextInstance, EditContext};
use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::{KernelIntrospect, MockKernel};
use waffle_types::*;

fn square_sketch(origin: [f64; 3], normal: [f64; 3]) -> Sketch {
    let mut solved_positions = HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (1.0, 0.0));
    solved_positions.insert(3, (1.0, 1.0));
    solved_positions.insert(4, (0.0, 1.0));
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
            policy: ResolvePolicy::Strict,
            scope: None,
        },
        plane_origin: origin,
        plane_normal: normal,
        plane_x_axis: None,
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
    }
}

fn extrude(sketch_id: Uuid, depth: f64, depth_mode: DepthMode) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            combine: None,
            targets: None,
            sketch_id,
            profile_index: 0,
            profile_entity_ids: None,
            depth,
            direction: None,
            symmetric: false,
            cut: false,
            merge: true,
            target_body: None,
            depth_mode,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            depth_expr: None,
        },
    }
}

fn top_face_ref(feature_id: Uuid, scope: Option<RefScope>) -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope,
    }
}

/// The "other" part: a unit square extruded 5 along +z, built in `kernel`.
/// Returns its engine and the extrude feature's id.
fn other_part(kernel: &mut MockKernel) -> (Engine, Uuid) {
    let mut e = Engine::new();
    let s = e
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: square_sketch([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            },
            kernel,
        )
        .unwrap();
    let x = e
        .add_feature("Extrude".into(), extrude(s, 5.0, DepthMode::Blind), kernel)
        .unwrap();
    assert!(e.errors.is_empty(), "{:?}", e.errors);
    (e, x)
}

/// The top face's centroid and normal as the kernel reports them.
fn top_face_geometry(
    engine: &Engine,
    extrude_id: Uuid,
    kernel: &MockKernel,
) -> ([f64; 3], [f64; 3]) {
    let r = feature_engine::resolve::resolve_with_fallback(
        &top_face_ref(extrude_id, None),
        &engine.feature_results,
    )
    .unwrap();
    let sig = kernel.compute_signature(r.kernel_id, TopoKind::Face);
    (sig.centroid.unwrap(), sig.normal.unwrap())
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn near3(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

/// A context with one other instance, offset by `shift` from the edited one.
fn context_with(other: &Engine, other_path: Uuid, shift: [f64; 3]) -> (EditContext, RefScope) {
    let me = Uuid::new_v4();
    let mut ctx = EditContext::new(
        "asm",
        vec![me],
        feature_engine::assembly::Transform::identity(),
    );
    ctx.instances.push(ContextInstance::new(
        vec![other_path],
        "Other 1",
        "part-other",
        None,
        feature_engine::assembly::Transform::translation(shift),
        &other.feature_results,
    ));
    (ctx, RefScope::in_assembly("asm", vec![other_path]))
}

#[test]
fn scoped_sketch_plane_is_rederived_from_the_context_in_the_edited_frame() {
    let mut kernel = MockKernel::new();
    let (other, other_extrude) = other_part(&mut kernel);
    let (centroid, normal) = top_face_geometry(&other, other_extrude, &kernel);
    let shift = [1.0, 2.0, 3.0];
    let (ctx, scope) = context_with(&other, Uuid::new_v4(), shift);

    let mut edited = Engine::new();
    edited.context = Some(ctx);
    // The sketch arrives with a stale plane; the context pass replaces it.
    let mut sketch = square_sketch([9.0, 9.0, 9.0], [1.0, 0.0, 0.0]);
    sketch.plane = top_face_ref(other_extrude, Some(scope.clone()));
    let sid = edited
        .add_feature(
            "Ctx sketch".into(),
            Operation::Sketch { sketch },
            &mut kernel,
        )
        .unwrap();
    assert!(edited.errors.is_empty(), "{:?}", edited.errors);
    let Operation::Sketch { sketch } = &edited.tree.find_feature(sid).unwrap().operation else {
        unreachable!()
    };
    assert!(
        near3(sketch.plane_origin, add3(centroid, shift)),
        "plane origin {:?} should be the other face's centroid {:?} shifted by {:?}",
        sketch.plane_origin,
        centroid,
        shift
    );
    assert!(
        near3(sketch.plane_normal, normal),
        "{:?}",
        sketch.plane_normal
    );
    // The scoped reference itself is untouched (the recipe keeps the link).
    assert_eq!(sketch.plane.scope.as_ref(), Some(&scope));

    // An extrude on that sketch builds. (Where it lands is the real kernel's
    // business — the bridge test `open_part_in_context_…` measures it; the
    // mock kernel does not model the sketch plane's position.)
    let xid = edited
        .add_feature(
            "Ctx extrude".into(),
            extrude(sid, 2.0, DepthMode::Blind),
            &mut kernel,
        )
        .unwrap();
    assert!(edited.errors.is_empty(), "{:?}", edited.errors);
    assert_eq!(edited.feature_results[&xid].outputs.len(), 1);

    // The context is updated with the other instance moved: the plane follows
    // and the extrude rebuilds with it (rebuild widened to the sketch).
    let shift2 = [1.0, 2.0, 7.0];
    let (ctx2, _) = context_with(&other, scope.instance_path[0], shift2);
    edited.context = Some(ctx2);
    edited.rebuild_from_scratch(&mut kernel);
    assert!(edited.errors.is_empty(), "{:?}", edited.errors);
    let Operation::Sketch { sketch } = &edited.tree.find_feature(sid).unwrap().operation else {
        unreachable!()
    };
    assert!(near3(sketch.plane_origin, add3(centroid, shift2)));
    assert_eq!(edited.feature_results[&xid].outputs.len(), 1);

    // Without a context: LOUD warning, plane kept as last derived, still builds.
    edited.context = None;
    edited.rebuild_from_scratch(&mut kernel);
    assert!(edited.errors.is_empty(), "{:?}", edited.errors);
    let w = edited
        .warnings
        .iter()
        .find(|w| w.contains("Ctx sketch"))
        .unwrap_or_else(|| panic!("no context warning in {:?}", edited.warnings));
    assert!(
        w.contains("of assembly `asm`") && w.contains("open the part in that assembly's context"),
        "{w}"
    );
    let Operation::Sketch { sketch } = &edited.tree.find_feature(sid).unwrap().operation else {
        unreachable!()
    };
    assert!(near3(sketch.plane_origin, add3(centroid, shift2)));
}

#[test]
fn a_scoped_plane_whose_instance_is_gone_is_a_per_feature_error_and_keeps_the_plane() {
    let mut kernel = MockKernel::new();
    let (other, other_extrude) = other_part(&mut kernel);
    let (ctx, _) = context_with(&other, Uuid::new_v4(), [0.0; 3]);

    let mut edited = Engine::new();
    edited.context = Some(ctx);
    let mut sketch = square_sketch([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]);
    sketch.plane = top_face_ref(
        other_extrude,
        Some(RefScope::in_assembly("asm", vec![Uuid::new_v4()])),
    );
    let sid = edited
        .add_feature("Orphan".into(), Operation::Sketch { sketch }, &mut kernel)
        .unwrap();
    let (fid, msg) = edited
        .errors
        .iter()
        .find(|(id, _)| *id == sid)
        .expect("the sketch reports the unresolvable context");
    assert_eq!(*fid, sid);
    assert!(msg.contains("not in the open context"), "{msg}");
    let Operation::Sketch { sketch } = &edited.tree.find_feature(sid).unwrap().operation else {
        unreachable!()
    };
    assert_eq!(sketch.plane_origin, [0.5, 0.5, 0.5]);
}

#[test]
fn up_to_a_scoped_face_reaches_the_other_instance_and_is_loud_without_context() {
    let mut kernel = MockKernel::new();
    let (other, other_extrude) = other_part(&mut kernel);
    let (centroid, _) = top_face_geometry(&other, other_extrude, &kernel);
    let shift = [0.0, 0.0, 4.0];
    let (ctx, scope) = context_with(&other, Uuid::new_v4(), shift);

    let mut edited = Engine::new();
    edited.context = Some(ctx);
    let sid = edited
        .add_feature(
            "Base".into(),
            Operation::Sketch {
                sketch: square_sketch([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            },
            &mut kernel,
        )
        .unwrap();
    let up_to = DepthMode::UpTo {
        reference: top_face_ref(other_extrude, Some(scope)),
    };
    let xid = edited
        .add_feature("Up to other".into(), extrude(sid, 1.0, up_to), &mut kernel)
        .unwrap();
    assert!(edited.errors.is_empty(), "{:?}", edited.errors);
    let (top, _) = top_face_geometry(&edited, xid, &kernel);
    let expected = centroid[2] + shift[2];
    assert!(
        (top[2] - expected).abs() < 1e-9,
        "up-to depth reached z {} (expected the other face at {})",
        top[2],
        expected
    );

    edited.context = None;
    edited.rebuild_from_scratch(&mut kernel);
    let (_, msg) = edited
        .errors
        .iter()
        .find(|(id, _)| *id == xid)
        .expect("up-to without context fails the feature");
    assert!(
        msg.contains("open the part in that assembly's context"),
        "{msg}"
    );
}

#[test]
fn a_scoped_reference_never_resolves_against_the_local_part() {
    let mut kernel = MockKernel::new();
    let (other, other_extrude) = other_part(&mut kernel);
    // Same anchor id as a local feature would have: the guard must refuse
    // BEFORE looking the id up.
    let scoped = top_face_ref(
        other_extrude,
        Some(RefScope::in_assembly("asm", vec![Uuid::new_v4()])),
    );
    let err = feature_engine::resolve::resolve_geom_ref(&scoped, &other.feature_results)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("scoped to") && err.contains("assembly `asm`"),
        "{err}"
    );
    let err = feature_engine::resolve::resolve_by_position(
        &scoped,
        &other.feature_results,
        &kernel,
        [0.0; 3],
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("scoped to"), "{err}");
    // The unscoped twin resolves.
    assert!(feature_engine::resolve::resolve_geom_ref(
        &top_face_ref(other_extrude, None),
        &other.feature_results
    )
    .is_ok());
}

#[test]
fn scope_round_trips_through_json_and_is_absent_when_local() {
    let scope = RefScope::in_assembly("asm", vec![Uuid::new_v4(), Uuid::new_v4()]);
    let r = top_face_ref(Uuid::new_v4(), Some(scope.clone()));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["scope"]["tab_id"], "asm");
    assert_eq!(json["scope"]["instance_path"].as_array().unwrap().len(), 2);
    assert!(
        json["scope"].get("source_id").is_none(),
        "absent source_id is not written"
    );
    let back: GeomRef = serde_json::from_value(json).unwrap();
    assert_eq!(back.scope, Some(scope));

    let local = serde_json::to_value(top_face_ref(Uuid::new_v4(), None)).unwrap();
    assert!(
        local.get("scope").is_none(),
        "a local reference writes no scope key"
    );
    // A pre-v5 reference (no `scope` key) parses as local.
    let back: GeomRef = serde_json::from_value(local).unwrap();
    assert!(back.scope.is_none());
}
