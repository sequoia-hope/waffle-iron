//! v4 §2.9 agent-friendly profile addressing
//! (`specs/waffle_v4_document_model.md`): `profile_entity_ids` names a solved
//! loop by its entity-id set, order-insensitively, and overrides
//! `profile_index`; no match, or two loops with the same set, is a loud
//! per-feature error rather than a silently different face.

use feature_engine::types::*;
use feature_engine::Engine;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
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

fn square_profile(edges: [u32; 4]) -> ClosedProfile {
    ClosedProfile {
        entity_ids: edges.to_vec(),
        is_outer: true,
        vertex_ids: vec![],
        circle: None,
        spline_segments: vec![],
        arc_segments: vec![],
    }
}

/// Two disjoint unit squares on a datum plane. Profile 0 is bounded by lines
/// 10–13 (points 1–4); profile 1 by lines 20–23 (points 5–8).
fn two_square_sketch() -> Sketch {
    let mut solved_positions = std::collections::HashMap::new();
    for (id, x, y) in [
        (1, 0.0, 0.0),
        (2, 1.0, 0.0),
        (3, 1.0, 1.0),
        (4, 0.0, 1.0),
        (5, 3.0, 0.0),
        (6, 4.0, 0.0),
        (7, 4.0, 1.0),
        (8, 3.0, 1.0),
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
            policy: ResolvePolicy::Strict,
            scope: None,
        },
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities: vec![
            point(1, 0.0, 0.0),
            point(2, 1.0, 0.0),
            point(3, 1.0, 1.0),
            point(4, 0.0, 1.0),
            point(5, 3.0, 0.0),
            point(6, 4.0, 0.0),
            point(7, 4.0, 1.0),
            point(8, 3.0, 1.0),
            line(10, 1, 2),
            line(11, 2, 3),
            line(12, 3, 4),
            line(13, 4, 1),
            line(20, 5, 6),
            line(21, 6, 7),
            line(22, 7, 8),
            line(23, 8, 5),
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: vec![],
        solved_profiles: vec![
            square_profile([10, 11, 12, 13]),
            square_profile([20, 21, 22, 23]),
        ],
    }
}

fn extrude(sketch_id: Uuid, profile_index: usize, ids: Option<Vec<u32>>) -> Operation {
    Operation::Extrude {
        params: ExtrudeParams {
            sketch_id,
            profile_index,
            profile_entity_ids: ids,
            depth: 0.5,
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

fn revolve(sketch_id: Uuid, profile_index: usize, ids: Option<Vec<u32>>) -> Operation {
    Operation::Revolve {
        params: RevolveParams {
            sketch_id,
            profile_index,
            profile_entity_ids: ids,
            axis_origin: [-1.0, 0.0, 0.0],
            axis_direction: [0.0, 1.0, 0.0],
            angle: 90.0,
            angle_expr: None,
            cut: false,
            merge: false,
            combine: Some(CombineMode::NewBody),
            targets: None,
        },
    }
}

/// Build sketch + one dependent feature, rebuild, return the engine and the
/// dependent feature's id.
fn run(sketch: Sketch, make: impl FnOnce(Uuid) -> Operation) -> (Engine, Uuid) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    // Extrude/revolve name the sketch FEATURE, not the sketch's own id.
    let sketch_feature = engine
        .add_feature("Sketch".into(), Operation::Sketch { sketch }, &mut kernel)
        .unwrap();
    let op = make(sketch_feature);
    let id = engine.tree.add_feature("Dependent".into(), op);
    engine.rebuild_from_scratch(&mut kernel);
    (engine, id)
}

fn error_for(engine: &Engine, id: Uuid) -> Option<&str> {
    engine
        .errors
        .iter()
        .find(|(fid, _)| *fid == id)
        .map(|(_, m)| m.as_str())
}

#[test]
fn entity_id_set_overrides_an_out_of_range_profile_index() {
    // profile_index 99 alone would be ProfileOutOfRange; the id set — given in
    // a scrambled order — resolves the second square.
    let (engine, id) = run(two_square_sketch(), |s| {
        extrude(s, 99, Some(vec![23, 21, 20, 22]))
    });
    assert_eq!(error_for(&engine, id), None, "{:?}", engine.errors);
    assert!(engine.feature_results.contains_key(&id));
}

#[test]
fn a_set_no_loop_matches_is_a_loud_error_even_with_a_valid_index() {
    let (engine, id) = run(two_square_sketch(), |s| {
        extrude(s, 0, Some(vec![10, 11, 12, 20]))
    });
    let msg = error_for(&engine, id).expect("feature error");
    assert!(
        msg.contains("no profile is bounded by entities [10, 11, 12, 20]"),
        "{msg}"
    );
    assert!(msg.contains("sketch has 2 profiles"), "{msg}");
    assert!(!engine.feature_results.contains_key(&id));
}

#[test]
fn a_subset_of_a_loop_does_not_match() {
    // Set equality, not containment: three of the four edges is not the loop.
    let (engine, id) = run(two_square_sketch(), |s| {
        extrude(s, 0, Some(vec![10, 11, 12]))
    });
    let msg = error_for(&engine, id).expect("feature error");
    assert!(msg.contains("no profile is bounded by"), "{msg}");
}

#[test]
fn two_loops_with_the_same_set_are_ambiguous() {
    let mut sketch = two_square_sketch();
    sketch
        .solved_profiles
        .push(square_profile([13, 12, 11, 10]));
    let (engine, id) = run(sketch, |s| extrude(s, 0, Some(vec![10, 11, 12, 13])));
    let msg = error_for(&engine, id).expect("feature error");
    assert!(
        msg.contains("2 profiles are bounded by entities [10, 11, 12, 13]"),
        "{msg}"
    );
}

#[test]
fn without_a_set_profile_index_is_still_range_checked() {
    let (engine, id) = run(two_square_sketch(), |s| extrude(s, 2, None));
    let msg = error_for(&engine, id).expect("feature error");
    assert!(msg.contains("profile index 2 out of range"), "{msg}");
}

#[test]
fn revolve_addresses_by_entity_ids_too() {
    let (engine, id) = run(two_square_sketch(), |s| {
        revolve(s, 99, Some(vec![12, 13, 10, 11]))
    });
    assert_eq!(error_for(&engine, id), None, "{:?}", engine.errors);

    let (engine, id) = run(two_square_sketch(), |s| revolve(s, 0, Some(vec![99])));
    let msg = error_for(&engine, id).expect("feature error");
    assert!(
        msg.contains("no profile is bounded by entities [99]"),
        "{msg}"
    );
}

#[test]
fn the_field_is_optional_on_the_wire_and_omitted_when_absent() {
    let by_index = serde_json::to_value(extrude(Uuid::nil(), 0, None)).unwrap();
    assert!(by_index["params"].get("profile_entity_ids").is_none());

    let by_set = serde_json::to_value(extrude(Uuid::nil(), 0, Some(vec![3, 4]))).unwrap();
    assert_eq!(
        by_set["params"]["profile_entity_ids"],
        serde_json::json!([3, 4])
    );

    let back: Operation = serde_json::from_value(by_set).unwrap();
    let Operation::Extrude { params } = back else {
        panic!()
    };
    assert_eq!(params.profile_entity_ids, Some(vec![3, 4]));

    // An agent-written params object with no `profile_entity_ids` key parses
    // to None (the index path).
    let raw = serde_json::json!({
        "type": "Extrude",
        "params": { "sketch_id": Uuid::nil(), "profile_index": 1, "depth": 0.01,
                    "direction": null, "symmetric": false, "cut": false, "target_body": null }
    });
    let Operation::Extrude { params } = serde_json::from_value(raw).unwrap() else {
        panic!()
    };
    assert_eq!(params.profile_entity_ids, None);
    assert_eq!(params.profile_index, 1);
}
