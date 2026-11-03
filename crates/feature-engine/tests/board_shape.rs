//! `Operation::BoardShape` (`specs/board_shape_feature.md`) unit and
//! incremental-rebuild tests in `feature-engine`:
//!
//! - `SketchConstraint::Coincident` welding across distinct endpoint IDs.
//! - Incremental rebuild carry-forward of `Engine::board_shapes` and
//!   `Engine::connectors` when an unrelated feature edits without re-executing
//!   the `BoardShape` node.
//! - Loud JSON deserialization refusal of non-`u32` and duplicate keys in
//!   `BoardShapeParams::roles`.
//!
//! Real-kernel B-Rep volume, genus, watertightness, assembly mating, and
//! exhaustive B1–B26 refusal tests live in
//! `crates/test-harness/tests/board_shape_kv2.rs`.

use std::collections::{BTreeMap, HashMap};

use feature_engine::board_shape::{element_stable_id, evaluate_board_shape, hole_connector_uuid};
use feature_engine::types::{
    BoardEntityRole, BoardShapeParams, BoardSide, CombineMode, DepthMode, ExtrudeParams, Operation,
};
use feature_engine::Engine;
use serde_json::json;
use uuid::Uuid;
use waffle_types::kernel::MockKernel;
use waffle_types::{
    Anchor, GeomRef, ResolvePolicy, Role, Selector, Sketch, SketchConstraint, SketchEntity,
    SolveStatus, TopoKind,
};

fn datum_plane_ref() -> GeomRef {
    GeomRef {
        kind: TopoKind::Face,
        anchor: Anchor::Datum {
            datum_id: Uuid::nil(),
        },
        selector: Selector::Role {
            role: Role::EndCapPositive,
            index: 0,
        },
        policy: ResolvePolicy::BestEffort,
        scope: None,
    }
}

#[test]
fn coincident_constraints_weld_distinct_endpoint_ids_into_closed_loop() {
    // Four independent line segments (8 distinct Point IDs) joined via
    // `SketchConstraint::Coincident` with a 4e-7 m solver residual (> TAU_MODEL
    // = 1e-7 m, <= MIN_FEATURE_SIZE = 1e-6 m) must weld cleanly into a single
    // closed 40 × 30 mm outline.
    let sketch_id = Uuid::new_v4();
    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 2,
            x: 0.040,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: 0.040 + 4e-7,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x: 0.040,
            y: 0.030,
            construction: false,
        },
        SketchEntity::Point {
            id: 5,
            x: 0.040,
            y: 0.030 + 4e-7,
            construction: false,
        },
        SketchEntity::Point {
            id: 6,
            x: 0.0,
            y: 0.030,
            construction: false,
        },
        SketchEntity::Point {
            id: 7,
            x: 0.0,
            y: 0.030 - 4e-7,
            construction: false,
        },
        SketchEntity::Point {
            id: 8,
            x: 4e-7,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Line {
            id: 10,
            start_id: 1,
            end_id: 2,
            construction: false,
        },
        SketchEntity::Line {
            id: 11,
            start_id: 3,
            end_id: 4,
            construction: false,
        },
        SketchEntity::Line {
            id: 12,
            start_id: 5,
            end_id: 6,
            construction: false,
        },
        SketchEntity::Line {
            id: 13,
            start_id: 7,
            end_id: 8,
            construction: false,
        },
    ];
    let constraints = vec![
        SketchConstraint::Coincident {
            point_a: 2,
            point_b: 3,
        },
        SketchConstraint::Coincident {
            point_a: 4,
            point_b: 5,
        },
        SketchConstraint::Coincident {
            point_a: 6,
            point_b: 7,
        },
        SketchConstraint::Coincident {
            point_a: 8,
            point_b: 1,
        },
    ];
    let sketch = Sketch {
        id: sketch_id,
        plane: datum_plane_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: Some([1.0, 0.0, 0.0]),
        entities,
        constraints,
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    };

    let mut roles = BTreeMap::new();
    for id in [10, 11, 12, 13] {
        roles.insert(id, BoardEntityRole::Outline);
    }
    let params = BoardShapeParams {
        sketch_id,
        board: "welded_pcb".to_string(),
        thickness: 0.0016,
        thickness_expr: None,
        roles,
    };

    let ev = evaluate_board_shape(Uuid::new_v4(), &params, &sketch).expect("coincident weld");
    assert_eq!(ev.outline.entity_ids, vec![10, 11, 12, 13]);
    assert!((ev.outline.area_m2 - 0.040 * 0.030).abs() < 1e-10);
}

#[test]
fn incremental_rebuild_carries_board_shape_and_connectors_when_not_reexecuted() {
    let mut engine = Engine::new();
    let mut kb = MockKernel::new();

    let board_sketch = Sketch {
        id: Uuid::new_v4(),
        plane: datum_plane_ref(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: Some([1.0, 0.0, 0.0]),
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 0.050,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.050,
                y: 0.030,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 0.030,
                construction: false,
            },
            SketchEntity::Line {
                id: 10,
                start_id: 1,
                end_id: 2,
                construction: false,
            },
            SketchEntity::Line {
                id: 11,
                start_id: 2,
                end_id: 3,
                construction: false,
            },
            SketchEntity::Line {
                id: 12,
                start_id: 3,
                end_id: 4,
                construction: false,
            },
            SketchEntity::Line {
                id: 13,
                start_id: 4,
                end_id: 1,
                construction: false,
            },
            SketchEntity::Point {
                id: 20,
                x: 0.010,
                y: 0.010,
                construction: true,
            },
            SketchEntity::Circle {
                id: 21,
                center_id: 20,
                radius: 0.0016,
                construction: false,
            },
        ],
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    };
    let sketch_fid = engine
        .add_feature(
            "BoardSketch".to_string(),
            Operation::Sketch {
                sketch: board_sketch,
            },
            &mut kb,
        )
        .unwrap();

    let mut roles = BTreeMap::new();
    for id in [10, 11, 12, 13] {
        roles.insert(id, BoardEntityRole::Outline);
    }
    roles.insert(
        21,
        BoardEntityRole::Hole {
            plated: true,
            clearance: 0.0002,
        },
    );
    let board_fid = engine
        .add_feature(
            "Board".to_string(),
            Operation::BoardShape {
                params: BoardShapeParams {
                    sketch_id: sketch_fid,
                    board: "ctrl_pcb".to_string(),
                    thickness: 0.0016,
                    thickness_expr: None,
                    roles,
                },
            },
            &mut kb,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let initial_handle = engine.get_result(board_fid).unwrap().outputs[0]
        .1
        .handle
        .clone();
    let expected_stable = element_stable_id(sketch_fid, 21);
    let expected_cid = hole_connector_uuid(board_fid, &expected_stable);
    assert_eq!(engine.connectors.len(), 1);
    assert_eq!(engine.connectors[0].feature_id, expected_cid);
    assert_eq!(
        engine.connectors[0].key.as_deref(),
        Some(expected_stable.as_str())
    );

    // Add and then edit an unrelated standalone feature AFTER `Board`.
    // Incremental rebuild must NOT re-execute `Board` (handle stays identical),
    // yet `Engine::board_shapes` and `Engine::connectors` must still carry
    // the evaluated board and hole connector!
    let mut side_sketch = Sketch {
        id: Uuid::new_v4(),
        plane: datum_plane_ref(),
        plane_origin: [0.200, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: Some([1.0, 0.0, 0.0]),
        entities: vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 0.010,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.010,
                y: 0.010,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 0.010,
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
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    };
    side_sketch.recompute_derived();
    let side_sketch_fid = engine
        .add_feature(
            "SideSketch".to_string(),
            Operation::Sketch {
                sketch: side_sketch,
            },
            &mut kb,
        )
        .unwrap();
    let make_new_body_extrude = |depth: f64| ExtrudeParams {
        sketch_id: side_sketch_fid,
        profile_index: 0,
        profile_entity_ids: None,
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
    };
    let side_ext_fid = engine
        .add_feature(
            "SideBox".to_string(),
            Operation::Extrude {
                params: make_new_body_extrude(0.005),
            },
            &mut kb,
        )
        .unwrap();
    engine
        .edit_feature(
            side_ext_fid,
            Operation::Extrude {
                params: make_new_body_extrude(0.012),
            },
            &mut kb,
        )
        .unwrap();
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    let carried_handle = engine.get_result(board_fid).unwrap().outputs[0]
        .1
        .handle
        .clone();
    assert_eq!(
        carried_handle.raw(),
        initial_handle.raw(),
        "BoardShape must not re-execute on an unrelated downstream edit"
    );
    assert!(engine.board_shapes.contains_key(&board_fid));
    assert_eq!(engine.connectors.len(), 1);
    assert_eq!(engine.connectors[0].feature_id, expected_cid);
    assert_eq!(
        engine.connectors[0].key.as_deref(),
        Some(expected_stable.as_str())
    );
}

#[test]
fn role_map_serde_rejects_non_u32_and_duplicate_keys_and_round_trips() {
    let sketch_id = Uuid::new_v4();
    let mut roles = BTreeMap::new();
    roles.insert(10, BoardEntityRole::Outline);
    roles.insert(
        21,
        BoardEntityRole::Hole {
            plated: true,
            clearance: 0.00025,
        },
    );
    roles.insert(
        31,
        BoardEntityRole::Keepout {
            side: BoardSide::Both,
            max_height: 0.003,
        },
    );
    let op = Operation::BoardShape {
        params: BoardShapeParams {
            sketch_id,
            board: "main".to_string(),
            thickness: 0.0016,
            thickness_expr: Some("1.6".to_string()),
            roles,
        },
    };
    let val = serde_json::to_value(&op).unwrap();
    assert_eq!(val["type"], "BoardShape");
    assert_eq!(val["params"]["roles"]["10"]["type"], "outline");
    assert_eq!(val["params"]["roles"]["21"]["type"], "hole");
    assert_eq!(val["params"]["roles"]["31"]["type"], "keepout");
    let back: Operation = serde_json::from_value(val).unwrap();
    match (&back, &op) {
        (Operation::BoardShape { params: a }, Operation::BoardShape { params: b }) => {
            assert_eq!(a, b);
        }
        other => panic!("expected BoardShape round-trip, got {other:?}"),
    }

    // Non-u32 key refused loudly.
    let bad_key = json!({
        "type": "BoardShape",
        "params": {
            "sketch_id": sketch_id,
            "board": "main",
            "thickness": 0.0016,
            "roles": {
                "not_a_number": { "type": "outline" }
            }
        }
    });
    let err = serde_json::from_value::<Operation>(bad_key)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a valid u32"), "{err}");

    // Duplicate u32 key ("05" vs "5") refused loudly.
    let raw_dup = format!(
        r#"{{"type":"BoardShape","params":{{"sketch_id":"{sketch_id}","board":"main","thickness":0.0016,"roles":{{"5":{{"type":"outline"}},"05":{{"type":"cutout"}}}}}}}}"#
    );
    let dup_err = serde_json::from_str::<Operation>(&raw_dup)
        .unwrap_err()
        .to_string();
    assert!(
        dup_err.contains("duplicate board role assignment"),
        "{dup_err}"
    );
}
