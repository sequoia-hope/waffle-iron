//! `Operation::Sketch3d` through the engine (`specs/sketch3d.md` S2).
//!
//! A 3D sketch is reference geometry: it produces no body, its evaluated
//! chains are read after the rebuild from `Engine::sketch3d`, its expressions
//! are driven by design parameters, and every way it can be malformed is a
//! typed per-feature error rather than a downstream surprise. The pure
//! geometry (fillet tangency, chain walking, circumcircles) is pinned in
//! `waffle_types::sketch3d`'s own tests; this file pins the wiring.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::{KernelIntrospect, MockKernel};
use waffle_types::sketch3d::{Attachment, Axis, Edge3dKind, Sketch3d, Sketch3dEntity};
use waffle_types::*;

fn pt(id: u32, xyz: [f64; 3]) -> Sketch3dEntity {
    Sketch3dEntity::Point {
        id,
        xyz,
        attach: None,
        xyz_expr: None,
        construction: false,
    }
}

fn pt_expr(id: u32, xyz: [f64; 3], exprs: [Option<&str>; 3]) -> Sketch3dEntity {
    Sketch3dEntity::Point {
        id,
        xyz,
        attach: None,
        xyz_expr: Some(exprs.map(|e| e.map(str::to_string))),
        construction: false,
    }
}

fn line(id: u32, start_id: u32, end_id: u32) -> Sketch3dEntity {
    Sketch3dEntity::Line {
        id,
        start_id,
        end_id,
        construction: false,
    }
}

/// An L in space: (0,0,0) → (1,0,0) → (1,0,1). Two segments, one corner.
fn l_bend() -> Sketch3d {
    Sketch3d::new(
        Uuid::new_v4(),
        vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt(2, [1.0, 0.0, 0.0]),
            pt(3, [1.0, 0.0, 1.0]),
            line(4, 1, 2),
            line(5, 2, 3),
        ],
    )
}

fn add_sketch3d(engine: &mut Engine, kernel: &mut MockKernel, sketch: Sketch3d) -> Uuid {
    engine
        .add_feature("Path".into(), Operation::Sketch3d { sketch }, kernel)
        .expect("the feature is added")
}

#[test]
fn a_3d_sketch_rebuilds_produces_no_body_and_its_chains_are_readable() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let id = add_sketch3d(&mut engine, &mut kernel, l_bend());

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);

    // No body: a 3D sketch is reference geometry.
    let result = engine
        .feature_results
        .get(&id)
        .expect("the feature has a result");
    assert!(result.outputs.is_empty(), "a 3D sketch produces no body");

    // The chains are read after the rebuild, like a connector's frame.
    let ev = engine.sketch3d.get(&id).expect("evaluated");
    assert_eq!(ev.resolved.len(), 3);
    assert_eq!(ev.chains.len(), 1);
    assert_eq!(ev.chains[0].edges.len(), 2);
    assert!(!ev.chains[0].closed);
    assert_eq!(ev.chains[0].g1, vec![false], "a square corner is not G1");
}

#[test]
fn a_3d_sketch_does_not_become_a_boolean_target() {
    // The exclusion lists: a following extrude must not try to combine with
    // the 3D sketch, and `UnionAll` must not count it as a body.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    add_sketch3d(&mut engine, &mut kernel, l_bend());

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
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Circle {
                id: 2,
                center_id: 1,
                radius: 0.5,
                construction: false,
            },
        ],
        constraints: vec![],
        solve_status: SolveStatus::Unsolved,
        solved_positions: Default::default(),
        solved_profiles: vec![],
        projected: Default::default(),
    };
    let sid = engine
        .add_feature("Sketch".into(), Operation::Sketch { sketch }, &mut kernel)
        .unwrap();
    let eid = engine
        .add_feature(
            "Extrude".into(),
            Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sid,
                    profile_index: 0,
                    profile_entity_ids: None,
                    depth: 1.0,
                    depth_expr: None,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: true,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: vec![],
                    combine: None,
                    targets: None,
                },
            },
            &mut kernel,
        )
        .expect("extrude added");

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    assert!(
        !engine.feature_results[&eid].outputs.is_empty(),
        "the extrude still produces its body"
    );
}

#[test]
fn an_expression_drives_a_point_coordinate_in_mm_space() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree.parameters.push(DesignParameter {
        id: Uuid::new_v4(),
        name: "run".into(),
        expression: "250".into(),
        value: 0.0,
        error: None,
    });
    let sketch = Sketch3d::new(
        Uuid::new_v4(),
        vec![
            pt(1, [0.0, 0.0, 0.0]),
            pt_expr(2, [0.0, 0.0, 0.0], [Some("run"), None, None]),
            line(3, 1, 2),
        ],
    );
    let id = add_sketch3d(&mut engine, &mut kernel, sketch);

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    let ev = engine.sketch3d.get(&id).expect("evaluated");
    // 250 mm becomes 0.25 m, the same conversion every other `*_expr` uses.
    assert_eq!(ev.resolved[&2], [0.25, 0.0, 0.0]);
    assert!((ev.chains[0].length() - 0.25).abs() < 1e-12);
}

#[test]
fn a_fillet_radius_expression_is_driven_too() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree.parameters.push(DesignParameter {
        id: Uuid::new_v4(),
        name: "bend".into(),
        expression: "100".into(),
        value: 0.0,
        error: None,
    });
    let mut sketch = l_bend();
    sketch.entities.push(Sketch3dEntity::Fillet {
        id: 6,
        at_point_id: 2,
        radius: 0.0,
        radius_expr: Some("bend".into()),
    });
    let id = add_sketch3d(&mut engine, &mut kernel, sketch);

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    let ev = engine.sketch3d.get(&id).expect("evaluated");
    let arc = ev.chains[0]
        .edges
        .iter()
        .find(|e| matches!(e.kind, Edge3dKind::Arc { .. }))
        .expect("the fillet minted an arc");
    let Edge3dKind::Arc { radius, .. } = arc.kind else {
        unreachable!()
    };
    assert!(
        (radius - 0.1).abs() < 1e-15,
        "100 mm is 0.1 m, got {radius}"
    );
    assert_eq!(ev.chains[0].g1, vec![true, true], "the bend is tangent");
}

#[test]
fn a_malformed_3d_sketch_is_this_features_typed_error() {
    // A fillet too large for its legs fails the SKETCH, loudly, rather than
    // surfacing later as a sweep that cannot find its path.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let mut sketch = l_bend();
    sketch.entities.push(Sketch3dEntity::Fillet {
        id: 6,
        at_point_id: 2,
        radius: 50.0,
        radius_expr: None,
    });
    let id = add_sketch3d(&mut engine, &mut kernel, sketch);

    assert!(!engine.errors.is_empty(), "the bad fillet must be loud");
    assert!(
        engine
            .errors
            .iter()
            .any(|(fid, msg)| *fid == id && msg.contains("does not fit between its neighbours")),
        "the message names the defect: {:?}",
        engine.errors
    );
    assert!(
        !engine.sketch3d.contains_key(&id),
        "a failed sketch publishes no chains"
    );
}

#[test]
fn a_dangling_attachment_is_loud_rather_than_placed_at_the_origin() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sketch = Sketch3d::new(
        Uuid::new_v4(),
        vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [9.0, 9.0, 9.0],
                attach: Some(Box::new(Attachment::Vertex {
                    reference: GeomRef {
                        kind: TopoKind::Vertex,
                        anchor: Anchor::FeatureOutput {
                            feature_id: Uuid::new_v4(), // no such feature
                            output_key: OutputKey::Main,
                        },
                        selector: Selector::Position {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        policy: ResolvePolicy::Strict,
                        scope: None,
                    },
                })),
                xyz_expr: None,
                construction: false,
            },
            pt(2, [1.0, 0.0, 0.0]),
            line(3, 1, 2),
        ],
    );
    let id = add_sketch3d(&mut engine, &mut kernel, sketch);

    assert!(
        engine
            .errors
            .iter()
            .any(|(fid, msg)| *fid == id && msg.contains("does not resolve")),
        "a dangling attachment must be named, not silently ignored: {:?}",
        engine.errors
    );
}

/// A unit square sketch on the XY datum, ready to extrude.
fn unit_square() -> Sketch {
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
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
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
            SketchEntity::Line {
                id: 5,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 6,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 7,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 8,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
        ],
        constraints: vec![],
        solve_status: SolveStatus::Unsolved,
        solved_positions: Default::default(),
        solved_profiles: vec![],
        projected: Default::default(),
    }
}

fn extrude_params(sketch_id: Uuid, depth: f64) -> ExtrudeParams {
    ExtrudeParams {
        sketch_id,
        profile_index: 0,
        profile_entity_ids: None,
        depth,
        depth_expr: None,
        direction: None,
        symmetric: false,
        cut: false,
        merge: true,
        target_body: None,
        depth_mode: DepthMode::Blind,
        second_direction: None,
        region: None,
        regions: vec![],
        combine: None,
        targets: None,
    }
}

/// A feature-output reference named by a viewport PICK: a Position selector.
fn picked(kind: TopoKind, feature_id: Uuid, at: [f64; 3], policy: ResolvePolicy) -> GeomRef {
    GeomRef {
        kind,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Position {
            x: at[0],
            y: at[1],
            z: at[2],
        },
        policy,
        scope: None,
    }
}

/// The body of `eid`, as the kernel holds it now.
fn body_of(engine: &Engine, eid: Uuid) -> waffle_types::kernel::KernelSolidHandle {
    engine.feature_results[&eid].outputs[0].1.handle.clone()
}

/// The vertex of the body with the largest z (the mock box stands on z = 0,
/// so this is one that MOVES when the extrude depth changes) and its position.
fn top_vertex(kernel: &MockKernel, handle: &waffle_types::kernel::KernelSolidHandle) -> [f64; 3] {
    kernel
        .list_vertices(handle)
        .into_iter()
        .filter_map(|v| kernel.compute_signature(v, TopoKind::Vertex).centroid)
        .max_by(|a, b| a[2].partial_cmp(&b[2]).unwrap())
        .expect("the extrude has vertices")
}

/// Sketch → extrude (depth 2) → a 3D sketch whose point 1 is attached to
/// the box's TOP vertex by a pick under `policy`, with point 2 three metres
/// above it. Returns the extrude id, the 3D sketch id and the picked position.
fn box_with_point_on_top_vertex(
    kernel: &mut MockKernel,
    engine: &mut Engine,
    policy: ResolvePolicy,
) -> (Uuid, Uuid, [f64; 3]) {
    let sid = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: unit_square(),
            },
            kernel,
        )
        .unwrap();
    let eid = engine
        .add_feature(
            "Extrude".into(),
            Operation::Extrude {
                params: extrude_params(sid, 2.0),
            },
            kernel,
        )
        .unwrap();
    let picked_at = top_vertex(kernel, &body_of(engine, eid));

    let sketch3d = Sketch3d::new(
        Uuid::new_v4(),
        vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [0.0, 0.0, 0.0], // a stale hint; the attachment wins
                attach: Some(Box::new(Attachment::Vertex {
                    reference: picked(TopoKind::Vertex, eid, picked_at, policy),
                })),
                xyz_expr: None,
                construction: false,
            },
            Sketch3dEntity::Point {
                id: 2,
                xyz: [0.0, 0.0, 0.0],
                attach: Some(Box::new(Attachment::AlongAxis {
                    from: 1,
                    axis: Axis::Z,
                    distance: 3.0,
                })),
                xyz_expr: None,
                construction: false,
            },
            line(3, 1, 2),
        ],
    );
    let pid = add_sketch3d(engine, kernel, sketch3d);
    (eid, pid, picked_at)
}

/// Change the extrude's depth in place and rebuild from it.
fn set_extrude_depth(engine: &mut Engine, kernel: &mut MockKernel, eid: Uuid, depth: f64) {
    let Operation::Extrude { params } = &engine.tree.find_feature(eid).unwrap().operation else {
        panic!("not an extrude");
    };
    let mut params = params.clone();
    params.depth = depth;
    engine
        .edit_feature(eid, Operation::Extrude { params }, kernel)
        .expect("the edit is accepted");
}

#[test]
fn a_point_attaches_to_a_model_vertex() {
    // The attachment that only the rebuild walk can resolve: a point on a
    // vertex of an earlier feature.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let (_, pid, expected) =
        box_with_point_on_top_vertex(&mut kernel, &mut engine, ResolvePolicy::Strict);

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    let ev = engine.sketch3d.get(&pid).expect("evaluated");
    assert_eq!(
        ev.resolved[&1], expected,
        "the attached point sits ON the model vertex, not on its stale hint"
    );
    // And the axis run is measured from the resolved position, not the hint.
    assert_eq!(
        ev.resolved[&2],
        [expected[0], expected[1], expected[2] + 3.0]
    );
    assert!(
        ev.warnings.is_empty(),
        "an exact pick has nothing to warn about"
    );
}

#[test]
fn a_best_effort_point_follows_the_vertex_when_the_model_moves_and_says_so() {
    // The re-bind path: the extrude grows, the picked vertex moves off the
    // stored position, and a BestEffort attachment re-binds to the nearest
    // vertex — which for a small move is the SAME vertex, moved. The move
    // must be reported, because a pick that lands somewhere new is a path
    // that sweeps somewhere new.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let (eid, pid, picked_at) =
        box_with_point_on_top_vertex(&mut kernel, &mut engine, ResolvePolicy::BestEffort);
    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);

    set_extrude_depth(&mut engine, &mut kernel, eid, 2.5);
    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);

    let moved_to = top_vertex(&kernel, &body_of(&engine, eid));
    assert!(
        (moved_to[2] - picked_at[2] - 0.5).abs() < 1e-9,
        "the top vertex moved up by the depth change: {picked_at:?} -> {moved_to:?}"
    );
    let ev = engine.sketch3d.get(&pid).expect("evaluated after the edit");
    assert_eq!(
        ev.resolved[&1], moved_to,
        "the attached point FOLLOWED the vertex rather than keeping the stale pick"
    );
    assert_eq!(
        ev.resolved[&2],
        [moved_to[0], moved_to[1], moved_to[2] + 3.0]
    );
    // The resolver's warning survives the anchor boundary and reaches both
    // the evaluation and the engine's warning list.
    assert!(
        ev.warnings
            .iter()
            .any(|w| w.starts_with("point 1:") && w.contains("geometry may have moved")),
        "the re-bind is reported against its point: {:?}",
        ev.warnings
    );
    assert!(
        engine
            .warnings
            .iter()
            .any(|w| w.contains("geometry may have moved")),
        "the warning reaches the engine: {:?}",
        engine.warnings
    );
}

#[test]
fn a_strict_point_fails_loudly_with_the_resolvers_reason_when_the_model_moves() {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let (eid, pid, _) =
        box_with_point_on_top_vertex(&mut kernel, &mut engine, ResolvePolicy::Strict);
    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);

    set_extrude_depth(&mut engine, &mut kernel, eid, 2.5);

    assert!(
        engine.errors.iter().any(|(fid, msg)| *fid == pid
            && msg.contains("point 1")
            && msg.contains("No Vertex within")),
        "the sketch fails with the resolver's own reason, not a generic one: {:?}",
        engine.errors
    );
    assert!(
        !engine.sketch3d.contains_key(&pid),
        "a failed sketch publishes no chains"
    );
}

#[test]
fn a_point_attaches_to_a_picked_planar_face() {
    // OnPlane with the reference a viewport pick produces (a Position
    // selector) — the same form a Vertex attachment accepts. It resolves
    // through the same nearest-entity route, then lands at the face plane's
    // uv, in the basis every sketch plane uses.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let sid = engine
        .add_feature(
            "Sketch".into(),
            Operation::Sketch {
                sketch: unit_square(),
            },
            &mut kernel,
        )
        .unwrap();
    let eid = engine
        .add_feature(
            "Extrude".into(),
            Operation::Extrude {
                params: extrude_params(sid, 2.0),
            },
            &mut kernel,
        )
        .unwrap();
    let handle = body_of(&engine, eid);
    // The face whose normal points +z: the top of the box.
    let (centroid, normal) = kernel
        .list_faces(&handle)
        .into_iter()
        .map(|f| kernel.compute_signature(f, TopoKind::Face))
        .filter_map(|s| Some((s.centroid?, s.normal?)))
        .max_by(|a, b| a.1[2].partial_cmp(&b.1[2]).unwrap())
        .expect("the box has faces");
    assert!(normal[2] > 0.9, "picked the top face: {normal:?}");

    let sketch3d = Sketch3d::new(
        Uuid::new_v4(),
        vec![
            Sketch3dEntity::Point {
                id: 1,
                xyz: [0.0; 3],
                attach: Some(Box::new(Attachment::OnPlane {
                    reference: picked(TopoKind::Face, eid, centroid, ResolvePolicy::Strict),
                    uv: [0.1, 0.2],
                })),
                xyz_expr: None,
                construction: false,
            },
            pt(2, [5.0, 5.0, 5.0]),
            line(3, 1, 2),
        ],
    );
    let pid = add_sketch3d(&mut engine, &mut kernel, sketch3d);

    assert!(engine.errors.is_empty(), "errors: {:?}", engine.errors);
    let ev = engine.sketch3d.get(&pid).expect("evaluated");
    let expected = SketchPlaneBasis::from_origin_normal(centroid, normal).local_to_world(0.1, 0.2);
    assert_eq!(ev.resolved[&1], expected);
}

#[test]
fn an_expression_on_an_attached_point_is_this_features_typed_error() {
    // Both an attachment and a driving expression: the attachment derives
    // the point, so the expression would be evaluated into a coordinate
    // nothing reads. Refused, and the message says why.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine.tree.parameters.push(DesignParameter {
        id: Uuid::new_v4(),
        name: "height".into(),
        expression: "10".into(),
        value: 0.0,
        error: None,
    });
    let sketch = Sketch3d::new(
        Uuid::new_v4(),
        vec![
            pt(1, [0.0; 3]),
            Sketch3dEntity::Point {
                id: 2,
                xyz: [0.0; 3],
                attach: Some(Box::new(Attachment::AlongAxis {
                    from: 1,
                    axis: Axis::X,
                    distance: 0.5,
                })),
                xyz_expr: Some([None, None, Some("height".into())]),
                construction: false,
            },
            line(3, 1, 2),
        ],
    );
    let id = add_sketch3d(&mut engine, &mut kernel, sketch);
    assert!(
        engine.errors.iter().any(|(fid, msg)| *fid == id
            && msg.contains("point 2")
            && msg.contains("both an attachment and a driving expression")),
        "the contradiction is named: {:?}",
        engine.errors
    );
    assert!(!engine.sketch3d.contains_key(&id));
}

#[test]
fn a_fillet_id_beyond_the_generated_range_is_this_features_typed_error() {
    // Before the check, `id * 100_000` overflowed u32 here: a panic in this
    // build, a silent wraparound in the WASM bundle.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let mut sketch = l_bend();
    sketch.entities.push(Sketch3dEntity::Fillet {
        id: waffle_types::MAX_GENERATOR_ENTITY_ID + 1,
        at_point_id: 2,
        radius: 0.1,
        radius_expr: None,
    });
    let id = add_sketch3d(&mut engine, &mut kernel, sketch);
    assert!(
        engine
            .errors
            .iter()
            .any(|(fid, msg)| *fid == id && msg.contains("largest a fillet can have")),
        "{:?}",
        engine.errors
    );
}

#[test]
fn a_3d_sketch_round_trips_through_the_operation_enum() {
    let mut sketch = l_bend();
    sketch.entities.push(Sketch3dEntity::Fillet {
        id: 6,
        at_point_id: 2,
        radius: 0.25,
        radius_expr: Some("bend".into()),
    });
    let op = Operation::Sketch3d { sketch };
    assert_eq!(op.type_tag(), "Sketch3d");
    assert!(
        OPERATION_TAGS.contains(&"Sketch3d"),
        "the tag is known, so it is not opaque to this build"
    );

    let json = serde_json::to_value(&op).unwrap();
    assert_eq!(json["type"], "Sketch3d");
    let back: Operation = serde_json::from_value(json.clone()).unwrap();
    assert!(
        !matches!(back, Operation::Unknown(_)),
        "a known tag must not fall through to the opaque branch"
    );
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
}

#[test]
fn an_unattached_point_omits_its_optional_fields_on_the_wire() {
    // The boxing and the `skip_serializing_if`s are what keep a thousand-point
    // frame path from paying for fields it does not use.
    let json = serde_json::to_value(pt(1, [1.0, 2.0, 3.0])).unwrap();
    assert_eq!(json["type"], "Point");
    assert!(json.get("attach").is_none(), "no attach key: {json}");
    assert!(json.get("xyz_expr").is_none(), "no xyz_expr key: {json}");
}
