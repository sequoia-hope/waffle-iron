//! `Operation::BoardShape` end-to-end verification on the REAL kernel
//! (`kernel_v2::KernelV2Adapter` via `ModelBuilder::kernel_v2()` and `Engine`):
//!
//! 1. Rectangle board (80 × 50 × 1.6 mm) with four Ø 3.2 mm corner holes:
//!    exact `KernelIntrospect::solid_volume`, watertight mesh, Euler χ = −6,
//!    top-face `PartConnector`s keyed by `waffle:<sketch_id>/<entity_id>`,
//!    downstream boss feature on `EndCapPositive`, and assembly mating onto
//!    individual hole connectors.
//! 2. Round board (`Circle` outline, Ø 60 mm × 1.6 mm): exact `solid_volume`,
//!    watertight mesh, Euler χ = 2, and diameter preservation.
//! 3. Board with 90° arc corners, interior slot + circular cutouts, and
//!    `Front`/`Back`/`Both` keepouts (plus CW-drawn loop normalization and
//!    `thickness_expr` parameter updates): exact `solid_volume` proving true
//!    cylindrical walls rather than chordal approximations.
//! 4. Exhaustive invalid-role-assignment refusal (all 26 failure classes
//!    B1–B26 in `specs/board_shape_feature.md` §4): each refused with its own
//!    typed `ErrorKind::BoardShape` / `ErrorKind::SketchNotFound`, leaving zero
//!    output solids and zero connectors.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

use feature_engine::assembly::{
    AssemblyTree, AxialAnchor, Frame, Instance, Mate, MateConnector, MateKind, PartRef, Transform,
};
use feature_engine::board_shape::{element_stable_id, evaluate_board_shape, hole_connector_uuid};
use feature_engine::connector::ConnectorGeometry;
use feature_engine::sources::SourceStore;
use feature_engine::types::{
    BoardEntityRole, BoardShapeError, BoardShapeParams, BoardSide, DesignParameter, ErrorKind,
    Operation,
};
use feature_engine::Engine;
use serde_json::json;
use test_harness::helpers::mesh_signed_volume;
use test_harness::kv2_adapter::KernelV2Adapter;
use test_harness::oracle;
use test_harness::ModelBuilder;
use uuid::Uuid;
use waffle_types::gear::GearParams;
use waffle_types::kernel::{AxisKind, KernelSolidHandle};
use waffle_types::sprocket::SprocketParams;
use waffle_types::{
    Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, Sketch, SketchConstraint,
    SketchEntity, SolveStatus, TopoKind,
};
use wasm_bridge::UiToEngine;

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

fn xy_sketch(id: Uuid, entities: Vec<SketchEntity>, constraints: Vec<SketchConstraint>) -> Sketch {
    Sketch {
        id,
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
    }
}

fn exact_volume(b: &ModelBuilder, handle: &KernelSolidHandle) -> f64 {
    b.kernel_ref()
        .as_introspect()
        .solid_volume(handle)
        .expect("KernelV2Adapter must report exact solid_volume")
}

fn assert_rel_close(actual: f64, expected: f64, rel_tol: f64, label: &str) {
    let rel_err = (actual - expected).abs() / expected.abs().max(1e-30);
    assert!(
        rel_err < rel_tol,
        "{label}: actual={actual:.15e}, expected={expected:.15e}, rel_err={rel_err:.3e} >= {rel_tol:.1e}"
    );
}

fn add_sketch_feature(b: &mut ModelBuilder, name: &str, sketch: Sketch) -> Uuid {
    b.add_operation(name, Operation::Sketch { sketch })
        .expect("add sketch feature")
}

#[test]
fn rectangle_board_with_four_corner_holes_exact_volume_connectors_and_mates() {
    let mut b = ModelBuilder::kernel_v2();

    // 80 mm × 50 mm PCB on XY plane, 1.6 mm thick, four Ø 3.2 mm corner holes
    // inset 5 mm from each corner.
    let w = 0.080;
    let h = 0.050;
    let t = 0.0016;
    let hole_r = 0.0016; // Ø 3.2 mm
    let hole_centers: [(u32, u32, [f64; 2], bool, f64); 4] = [
        (20, 21, [0.005, 0.005], true, 0.00025),
        (22, 23, [0.075, 0.005], true, 0.00025),
        (24, 25, [0.075, 0.045], false, 0.0),
        (26, 27, [0.005, 0.045], true, 0.0003),
    ];

    let mut entities = vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 2,
            x: w,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: w,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x: 0.0,
            y: h,
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
    ];
    let mut roles = BTreeMap::new();
    for line_id in [10, 11, 12, 13] {
        roles.insert(line_id, BoardEntityRole::Outline);
    }
    for (pt_id, circle_id, [cx, cy], plated, clearance) in hole_centers {
        entities.push(SketchEntity::Point {
            id: pt_id,
            x: cx,
            y: cy,
            construction: true,
        });
        entities.push(SketchEntity::Circle {
            id: circle_id,
            center_id: pt_id,
            radius: hole_r,
            construction: false,
        });
        roles.insert(circle_id, BoardEntityRole::Hole { plated, clearance });
    }

    let raw_sketch_id = Uuid::new_v4();
    let sketch_fid = add_sketch_feature(
        &mut b,
        "BoardSketch",
        xy_sketch(raw_sketch_id, entities, Vec::new()),
    );
    let board_fid = b
        .board_shape("MainBoard", "BoardSketch", "main_pcb", t, roles)
        .expect("add BoardShape");
    assert!(
        b.engine_errors().is_empty(),
        "unexpected engine errors: {:?}",
        b.engine_errors()
    );

    // 1. Exact analytical volume and end-cap roles on KernelV2Adapter.
    let expected_area = w * h - 4.0 * PI * hole_r * hole_r;
    let expected_vol = expected_area * t;
    {
        let res = b.op_result("MainBoard").expect("BoardShape result");
        assert_eq!(res.outputs.len(), 1);
        assert_eq!(res.outputs[0].0, OutputKey::Main);
        let actual_vol = exact_volume(&b, &res.outputs[0].1.handle);
        assert_rel_close(
            actual_vol,
            expected_vol,
            1e-10,
            "rect board + 4 holes volume",
        );
        assert!(res
            .provenance
            .role_assignments
            .iter()
            .any(|(_, r)| *r == Role::EndCapPositive));
        assert!(res
            .provenance
            .role_assignments
            .iter()
            .any(|(_, r)| *r == Role::EndCapNegative));
    }

    // 2. Mesh watertightness and genus-4 Euler characteristic χ = 2 − 2(4) = −6.
    let mesh = b
        .tessellate_with_tol("MainBoard", 0.0002)
        .expect("tessellate");
    let wt = oracle::check_watertight_mesh(&mesh);
    assert!(wt.passed, "watertight: {}", wt.detail);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, -6);
    assert!(chi.passed, "euler characteristic: {}", chi.detail);
    let mv = mesh_signed_volume(&mesh);
    assert!(mv > 0.0, "signed mesh volume must be positive, got {mv}");
    assert!(
        (mv - expected_vol).abs() / expected_vol < 5e-3,
        "mesh volume {mv} vs exact {expected_vol}"
    );

    // 3. EvaluatedBoardShape & top-face PartConnectors keyed by stable ID.
    let ev = b
        .state
        .engine
        .board_shapes
        .get(&board_fid)
        .expect("EvaluatedBoardShape stored on Engine")
        .clone();
    assert_eq!(ev.board, "main_pcb");
    assert_eq!(ev.holes.len(), 4);
    assert_eq!(b.state.engine.connectors.len(), 4);

    for ((_, circle_id, [cx, cy], plated, clearance), hole) in hole_centers.iter().zip(&ev.holes) {
        let expected_stable = element_stable_id(sketch_fid, *circle_id);
        let expected_cid = hole_connector_uuid(board_fid, &expected_stable);
        assert_eq!(hole.entity_id, *circle_id);
        assert_eq!(hole.stable_id, expected_stable);
        assert_eq!(hole.connector_id, expected_cid);
        assert_rel_close(hole.diameter_m, 2.0 * hole_r, 1e-15, "hole diameter");
        assert_eq!(hole.plated, *plated);
        assert_eq!(hole.clearance_m, *clearance);
        assert_eq!(hole.frame.origin, [*cx, *cy, t]);
        assert_eq!(hole.frame.z_axis, [0.0, 0.0, 1.0]);
        assert_eq!(hole.frame.x_axis, [1.0, 0.0, 0.0]);

        let pc = ev
            .connectors
            .get(&expected_stable)
            .expect("connector indexed by stable ID");
        assert_eq!(pc.feature_id, expected_cid);
        assert_eq!(pc.key.as_deref(), Some(expected_stable.as_str()));
        assert_eq!(
            pc.geometry,
            Some(ConnectorGeometry::CircularEdge(AxisKind::Cylindrical))
        );
    }

    // Bridge summary exposes `"key"` on every hole connector.
    let summary = b.agent_tool("model_summary", json!({}));
    assert!(
        !summary.is_error,
        "summary error: {:?}",
        summary.structured_content
    );
    let summary_connectors = summary.structured_content["connectors"]
        .as_array()
        .expect("connectors array");
    assert_eq!(summary_connectors.len(), 4);
    assert_eq!(
        summary_connectors[0]["key"].as_str(),
        Some(element_stable_id(sketch_fid, 21).as_str())
    );

    // 4. Assembly mating: instantiate the board part + two standoff parts mated
    // onto corner holes 21 and 25 via their deterministic hole_connector_uuid!
    let board_tree = b.state.engine.tree.clone();
    let mut standoff_builder = ModelBuilder::kernel_v2();
    standoff_builder
        .rect_sketch(
            "S",
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            -0.002,
            -0.002,
            0.004,
            0.004,
        )
        .unwrap();
    standoff_builder.extrude("Post", "S", 0.010).unwrap();
    let standoff_mc_id = standoff_builder
        .add_operation(
            "BaseMC",
            Operation::MateConnector {
                params: feature_engine::types::MateConnectorParams {
                    name: "base".to_string(),
                    frame: Frame {
                        origin: [0.0, 0.0, 0.0],
                        z_axis: [0.0, 0.0, 1.0],
                        x_axis: [1.0, 0.0, 0.0],
                    },
                    ..Default::default()
                },
            },
        )
        .unwrap();

    let mut part_trees = HashMap::new();
    part_trees.insert("pcb_tab".to_string(), board_tree);
    part_trees.insert(
        "standoff_tab".to_string(),
        standoff_builder.state.engine.tree.clone(),
    );

    let board_inst_id = Uuid::new_v4();
    let standoff1_inst_id = Uuid::new_v4();
    let standoff2_inst_id = Uuid::new_v4();
    let conn_hole_1 = Uuid::new_v4();
    let conn_hole_3 = Uuid::new_v4();
    let conn_post_1 = Uuid::new_v4();
    let conn_post_2 = Uuid::new_v4();

    let mut asm = AssemblyTree::default();
    asm.instances.push(Instance {
        id: board_inst_id,
        name: "PCB".to_string(),
        source: PartRef {
            source_id: None,
            tab_id: "pcb_tab".to_string(),
        },
        transform: Transform::default(),
        fixed: true,
        suppressed: false,
        external_key: None,
        parameter_overrides: None,
        extra: serde_json::Map::new(),
    });
    for (inst_id, name) in [
        (standoff1_inst_id, "Standoff1"),
        (standoff2_inst_id, "Standoff2"),
    ] {
        asm.instances.push(Instance {
            id: inst_id,
            name: name.to_string(),
            source: PartRef {
                source_id: None,
                tab_id: "standoff_tab".to_string(),
            },
            transform: Transform::default(),
            fixed: false,
            suppressed: false,
            external_key: None,
            parameter_overrides: None,
            extra: serde_json::Map::new(),
        });
    }
    let hole1_pc_uuid = hole_connector_uuid(board_fid, &element_stable_id(sketch_fid, 21));
    let hole3_pc_uuid = hole_connector_uuid(board_fid, &element_stable_id(sketch_fid, 25));
    for (cid, inst_id, pc_uuid) in [
        (conn_hole_1, board_inst_id, hole1_pc_uuid),
        (conn_hole_3, board_inst_id, hole3_pc_uuid),
        (conn_post_1, standoff1_inst_id, standoff_mc_id),
        (conn_post_2, standoff2_inst_id, standoff_mc_id),
    ] {
        asm.connectors.push(MateConnector {
            id: cid,
            name: format!("c-{cid}"),
            instance_path: vec![inst_id],
            geom_ref: None,
            part_connector: Some(pc_uuid),
            frame: Frame::default(),
            anchor: AxialAnchor::Middle,
            flip_z: false,
            rotation_deg: 0.0,
            offset_m: [0.0, 0.0, 0.0],
            extra: serde_json::Map::new(),
        });
    }
    asm.mates.push(Mate {
        id: Uuid::new_v4(),
        name: "Fasten1".to_string(),
        kind: MateKind::Fastened {
            flip: false,
            rotation_deg: 0.0,
        },
        connectors: [conn_hole_1, conn_post_1],
        suppressed: false,
        extra: serde_json::Map::new(),
    });
    asm.mates.push(Mate {
        id: Uuid::new_v4(),
        name: "Fasten3".to_string(),
        kind: MateKind::Fastened {
            flip: false,
            rotation_deg: 0.0,
        },
        connectors: [conn_hole_3, conn_post_2],
        suppressed: false,
        extra: serde_json::Map::new(),
    });

    let mut kv2 = KernelV2Adapter::new();
    let mut reuse = Vec::new();
    let view = wasm_bridge::assembly_view::evaluate(
        asm,
        &part_trees,
        &HashMap::new(),
        &SourceStore::new(),
        &[],
        &mut kv2,
        &mut reuse,
    );
    assert!(
        view.errors.is_empty(),
        "assembly view errors: {:?}",
        view.errors
    );
    let t1 = view.placement(standoff1_inst_id);
    let t2 = view.placement(standoff2_inst_id);
    for (k, exp) in [0.005, 0.005, t].into_iter().enumerate() {
        assert!(
            (t1.translation_m[k] - exp).abs() < 1e-9,
            "standoff1 translation_m[{k}]={} vs {exp}",
            t1.translation_m[k]
        );
    }
    for (k, exp) in [0.075, 0.045, t].into_iter().enumerate() {
        assert!(
            (t2.translation_m[k] - exp).abs() < 1e-9,
            "standoff2 translation_m[{k}]={} vs {exp}",
            t2.translation_m[k]
        );
    }

    // 5. Downstream parametric feature on the board solid: resolve the top cap
    // (`Role::EndCapPositive`), then extrude a 10 × 10 mm header housing that
    // protrudes 3.0 mm above `z = t` (anchored 0.8 mm inside the board for a
    // transverse 3D boolean union into `MainBoard`).
    let top_face_ref = b
        .select_face_by_role("MainBoard", Role::EndCapPositive, 0)
        .expect("select EndCapPositive on MainBoard");
    let (top_origin, top_normal) = feature_engine::rebuild::resolve_face_plane(
        &top_face_ref,
        &b.state.engine.feature_results,
        b.kernel_ref().as_introspect(),
    )
    .expect("resolve EndCapPositive plane on MainBoard");
    assert!(
        (top_origin[2] - t).abs() < 1e-12,
        "top_origin={top_origin:?}"
    );
    assert_eq!(top_normal, [0.0, 0.0, 1.0]);

    b.rect_sketch_oriented(
        "BossSketch",
        [0.0, 0.0, t],
        [0.0, 0.0, 1.0],
        Some([1.0, 0.0, 0.0]),
        0.035,
        0.020,
        0.010,
        0.010,
    )
    .expect("boss sketch");
    b.extrude_bidirectional("HeaderHousing", "BossSketch", 0.003, 0.0008)
        .expect("boss extrude");
    assert!(
        b.engine_errors().is_empty(),
        "downstream boss merge errors: {:?}",
        b.engine_errors()
    );
    assert!(
        b.engine_warnings().is_empty(),
        "downstream boss merge warnings: {:?}",
        b.engine_warnings()
    );
    assert!(b.consumed_features().contains(&board_fid));
    let merged_res = b.op_result("HeaderHousing").unwrap();
    let merged_vol = exact_volume(&b, &merged_res.outputs[0].1.handle);
    let expected_merged = expected_vol + 0.010 * 0.010 * 0.003;
    assert_rel_close(
        merged_vol,
        expected_merged,
        1e-10,
        "board + merged boss volume",
    );
}

#[test]
fn round_board_circle_outline_exact_volume() {
    let mut b = ModelBuilder::kernel_v2();

    let outer_r = 0.030; // Ø 60 mm round PCB
    let hole_r = 0.0015; // Ø 3.0 mm mounting hole
    let t = 0.0016;

    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: true,
        },
        SketchEntity::Circle {
            id: 2,
            center_id: 1,
            radius: outer_r,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: 0.020,
            y: 0.0,
            construction: true,
        },
        SketchEntity::Circle {
            id: 4,
            center_id: 3,
            radius: hole_r,
            construction: false,
        },
    ];

    // First verify a pure round board (lone Circle outline, no holes): χ = 2.
    let mut pure_roles = BTreeMap::new();
    pure_roles.insert(2, BoardEntityRole::Outline);
    add_sketch_feature(
        &mut b,
        "RoundSketch",
        xy_sketch(Uuid::new_v4(), entities, Vec::new()),
    );
    let board_fid = b
        .board_shape("RoundBoard", "RoundSketch", "round_sensor", t, pure_roles)
        .expect("round BoardShape");
    assert!(
        b.engine_errors().is_empty(),
        "errors: {:?}",
        b.engine_errors()
    );

    let res = b.op_result("RoundBoard").unwrap();
    let expected_pure_vol = PI * outer_r * outer_r * t;
    let actual_pure_vol = exact_volume(&b, &res.outputs[0].1.handle);
    assert_rel_close(
        actual_pure_vol,
        expected_pure_vol,
        1e-10,
        "pure round board volume",
    );

    let mesh = b.tessellate_with_tol("RoundBoard", 0.0002).unwrap();
    assert!(oracle::check_watertight_mesh(&mesh).passed);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, 2);
    assert!(chi.passed, "{}", chi.detail);

    let ev = b.state.engine.board_shapes.get(&board_fid).unwrap();
    match ev.outline.segments.as_slice() {
        [feature_engine::board_shape::BoardSegment2D::Circle { diameter_m, .. }] => {
            assert_rel_close(
                *diameter_m,
                2.0 * outer_r,
                1e-15,
                "round outline diameter_m",
            );
        }
        other => panic!("expected single Circle segment, got {other:?}"),
    }

    // Edit the BoardShape to enable the Ø 3.0 mm mounting hole as well: χ = 0.
    let sketch_fid = b.feature_id("RoundSketch").unwrap();
    let mut with_hole_roles = BTreeMap::new();
    with_hole_roles.insert(2, BoardEntityRole::Outline);
    with_hole_roles.insert(
        4,
        BoardEntityRole::Hole {
            plated: true,
            clearance: 0.0002,
        },
    );
    b.dispatch(UiToEngine::EditFeature {
        feature_id: board_fid,
        provenance: None,
        operation: Operation::BoardShape {
            params: BoardShapeParams {
                sketch_id: sketch_fid,
                board: "round_sensor".to_string(),
                thickness: t,
                thickness_expr: None,
                roles: with_hole_roles,
            },
        },
    });
    assert!(b.engine_errors().is_empty());
    let res2 = b.op_result("RoundBoard").unwrap();
    let expected_holed_vol = PI * (outer_r * outer_r - hole_r * hole_r) * t;
    assert_rel_close(
        exact_volume(&b, &res2.outputs[0].1.handle),
        expected_holed_vol,
        1e-10,
        "round board with mounting hole volume",
    );
}

#[test]
fn board_with_arc_corners_cutouts_keepouts_cw_winding_and_thickness_expr() {
    let mut b = ModelBuilder::kernel_v2();

    // 60 mm × 40 mm board with four 5 mm 90° rounded corners (drawn CLOCKWISE
    // in entity order to exercise automatic CCW normalization!), an interior
    // stadium/slot cutout (two 10 mm lines + two 180° R=3 mm semicircular end
    // arcs — exercising >90° arc splitting into minor arcs!), a second circular
    // cutout (R=2.5 mm), two mounting holes (R=1.5 mm), and two closed keepouts
    // (`Front` and `Both`) that must NOT subtract from the solid volume.
    let w = 0.060;
    let h = 0.040;
    let rc = 0.005;
    let t = 0.0016;

    // Rounded-rect corners: centers at (rc, rc), (w-rc, rc), (w-rc, h-rc), (rc, h-rc).
    // Note: SketchEntity::Arc is always CCW from start_id to end_id in Waffle Iron;
    // we shuffle entity IDs and reverse line directions so the loop chain has
    // mixed forward/reversed traversal!
    let entities = vec![
        // Corner centers (1..=4)
        SketchEntity::Point {
            id: 1,
            x: rc,
            y: rc,
            construction: true,
        },
        SketchEntity::Point {
            id: 2,
            x: w - rc,
            y: rc,
            construction: true,
        },
        SketchEntity::Point {
            id: 3,
            x: w - rc,
            y: h - rc,
            construction: true,
        },
        SketchEntity::Point {
            id: 4,
            x: rc,
            y: h - rc,
            construction: true,
        },
        // Tangent junction points (11..=18)
        SketchEntity::Point {
            id: 11,
            x: rc,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 12,
            x: w - rc,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 13,
            x: w,
            y: rc,
            construction: false,
        },
        SketchEntity::Point {
            id: 14,
            x: w,
            y: h - rc,
            construction: false,
        },
        SketchEntity::Point {
            id: 15,
            x: w - rc,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 16,
            x: rc,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 17,
            x: 0.0,
            y: h - rc,
            construction: false,
        },
        SketchEntity::Point {
            id: 18,
            x: 0.0,
            y: rc,
            construction: false,
        },
        // Outline lines (some authored reversed / CW) and CCW corner arcs:
        SketchEntity::Line {
            id: 21,
            start_id: 12,
            end_id: 11,
            construction: false,
        }, // reversed bottom
        SketchEntity::Arc {
            id: 22,
            center_id: 2,
            start_id: 12,
            end_id: 13,
            construction: false,
        }, // SE corner
        SketchEntity::Line {
            id: 23,
            start_id: 13,
            end_id: 14,
            construction: false,
        }, // right
        SketchEntity::Arc {
            id: 24,
            center_id: 3,
            start_id: 14,
            end_id: 15,
            construction: false,
        }, // NE corner
        SketchEntity::Line {
            id: 25,
            start_id: 15,
            end_id: 16,
            construction: false,
        }, // top
        SketchEntity::Arc {
            id: 26,
            center_id: 4,
            start_id: 16,
            end_id: 17,
            construction: false,
        }, // NW corner
        SketchEntity::Line {
            id: 27,
            start_id: 18,
            end_id: 17,
            construction: false,
        }, // reversed left
        SketchEntity::Arc {
            id: 28,
            center_id: 1,
            start_id: 18,
            end_id: 11,
            construction: false,
        }, // SW corner
        // Interior stadium/slot cutout: x in [0.018..0.028] plus R_s = 0.003 endcaps, centered at y = 0.020.
        SketchEntity::Point {
            id: 30,
            x: 0.018,
            y: 0.020,
            construction: true,
        },
        SketchEntity::Point {
            id: 31,
            x: 0.028,
            y: 0.020,
            construction: true,
        },
        SketchEntity::Point {
            id: 32,
            x: 0.018,
            y: 0.017,
            construction: false,
        },
        SketchEntity::Point {
            id: 33,
            x: 0.028,
            y: 0.017,
            construction: false,
        },
        SketchEntity::Point {
            id: 34,
            x: 0.028,
            y: 0.023,
            construction: false,
        },
        SketchEntity::Point {
            id: 35,
            x: 0.018,
            y: 0.023,
            construction: false,
        },
        SketchEntity::Line {
            id: 36,
            start_id: 32,
            end_id: 33,
            construction: false,
        },
        SketchEntity::Arc {
            id: 37,
            center_id: 31,
            start_id: 33,
            end_id: 34,
            construction: false,
        }, // 180° right cap
        SketchEntity::Line {
            id: 38,
            start_id: 34,
            end_id: 35,
            construction: false,
        },
        SketchEntity::Arc {
            id: 39,
            center_id: 30,
            start_id: 35,
            end_id: 32,
            construction: false,
        }, // 180° left cap
        // Second interior cutout: Circle at (0.045, 0.020), R = 0.0025.
        SketchEntity::Point {
            id: 40,
            x: 0.045,
            y: 0.020,
            construction: true,
        },
        SketchEntity::Circle {
            id: 41,
            center_id: 40,
            radius: 0.0025,
            construction: false,
        },
        // Mounting hole: Circle at (0.008, 0.008), R = 0.0015.
        SketchEntity::Point {
            id: 50,
            x: 0.008,
            y: 0.008,
            construction: true,
        },
        SketchEntity::Circle {
            id: 51,
            center_id: 50,
            radius: 0.0015,
            construction: false,
        },
        // Keepout 1 (Front, max_height = 0.004): rectangle [0.010..0.050] × [0.028..0.036]
        SketchEntity::Point {
            id: 60,
            x: 0.010,
            y: 0.028,
            construction: false,
        },
        SketchEntity::Point {
            id: 61,
            x: 0.050,
            y: 0.028,
            construction: false,
        },
        SketchEntity::Point {
            id: 62,
            x: 0.050,
            y: 0.036,
            construction: false,
        },
        SketchEntity::Point {
            id: 63,
            x: 0.010,
            y: 0.036,
            construction: false,
        },
        SketchEntity::Line {
            id: 64,
            start_id: 60,
            end_id: 61,
            construction: false,
        },
        SketchEntity::Line {
            id: 65,
            start_id: 61,
            end_id: 62,
            construction: false,
        },
        SketchEntity::Line {
            id: 66,
            start_id: 62,
            end_id: 63,
            construction: false,
        },
        SketchEntity::Line {
            id: 67,
            start_id: 63,
            end_id: 60,
            construction: false,
        },
        // Keepout 2 (Both, max_height = 0.0): Circle at (0.045, 0.010), R = 0.003
        SketchEntity::Point {
            id: 70,
            x: 0.045,
            y: 0.010,
            construction: true,
        },
        SketchEntity::Circle {
            id: 71,
            center_id: 70,
            radius: 0.003,
            construction: false,
        },
    ];

    let mut roles = BTreeMap::new();
    for id in 21..=28 {
        roles.insert(id, BoardEntityRole::Outline);
    }
    for id in 36..=39 {
        roles.insert(id, BoardEntityRole::Cutout);
    }
    roles.insert(41, BoardEntityRole::Cutout);
    roles.insert(
        51,
        BoardEntityRole::Hole {
            plated: true,
            clearance: 0.0002,
        },
    );
    for id in 64..=67 {
        roles.insert(
            id,
            BoardEntityRole::Keepout {
                side: BoardSide::Front,
                max_height: 0.004,
            },
        );
    }
    roles.insert(
        71,
        BoardEntityRole::Keepout {
            side: BoardSide::Both,
            max_height: 0.0,
        },
    );

    // Place sketch on an offset XZ plane (origin = [0.01, -0.02, 0.005], normal = [0, 1, 0])
    // to prove non-XY sketch plane orientation works end-to-end!
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: datum_plane_ref(),
        plane_origin: [0.01, -0.02, 0.005],
        plane_normal: [0.0, 1.0, 0.0],
        plane_x_axis: Some([1.0, 0.0, 0.0]),
        entities,
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions: HashMap::new(),
        solved_profiles: Vec::new(),
        projected: Vec::new(),
        plane_face: None,
    };
    let sketch_fid = add_sketch_feature(&mut b, "ArcBoardSketch", sketch);

    // Drive `thickness` from a design parameter `pcb_t = 1.6` (mm)!
    b.state
        .engine
        .tree
        .parameters
        .push(DesignParameter::new("pcb_t", "1.6"));
    let board_fid = b
        .add_operation(
            "ArcBoard",
            Operation::BoardShape {
                params: BoardShapeParams {
                    sketch_id: sketch_fid,
                    board: "arc_cutout_pcb".to_string(),
                    thickness: t,
                    thickness_expr: Some("pcb_t".to_string()),
                    roles: roles.clone(),
                },
            },
        )
        .expect("add ArcBoard");
    assert!(
        b.engine_errors().is_empty(),
        "ArcBoard errors: {:?}",
        b.engine_errors()
    );

    let outline_area = w * h - (4.0 - PI) * rc * rc;
    let slot_area = 0.010 * 0.006 + PI * 0.003 * 0.003;
    let circ_cut_area = PI * 0.0025 * 0.0025;
    let hole_area = PI * 0.0015 * 0.0015;
    let net_area = outline_area - slot_area - circ_cut_area - hole_area;

    let res = b.op_result("ArcBoard").unwrap();
    assert_rel_close(
        exact_volume(&b, &res.outputs[0].1.handle),
        net_area * t,
        1e-10,
        "rounded-corner + slot + circle cutout + hole exact volume",
    );

    // Genus = 3 through-openings (slot cutout, circle cutout, mounting hole) ⇒ χ = 2 − 2(3) = −4.
    let mesh = b.tessellate_with_tol("ArcBoard", 0.0002).unwrap();
    assert!(oracle::check_watertight_mesh(&mesh).passed);
    let chi = oracle::check_mesh_euler_characteristic(&mesh, -4);
    assert!(chi.passed, "{}", chi.detail);

    // Keepouts are recorded in EvaluatedBoardShape and did NOT subtract volume.
    let ev = b.state.engine.board_shapes.get(&board_fid).unwrap();
    assert_eq!(ev.keepouts.len(), 2);
    assert_eq!(ev.holes.len(), 1);
    // Plane origin [0.01, -0.02, 0.005], normal [0, 1, 0], x_axis [1, 0, 0]
    // ⇒ y_axis = normal × x_axis = [0, 0, -1].
    // Hole at (u=0.008, v=0.008) on top face (+normal * t):
    // world = [0.01 + 0.008, -0.02 + t, 0.005 - 0.008] = [0.018, -0.0184, -0.003].
    let hole_origin = ev.holes[0].frame.origin;
    assert!((hole_origin[0] - 0.018).abs() < 1e-12);
    assert!((hole_origin[1] - (-0.02 + t)).abs() < 1e-12);
    assert!((hole_origin[2] - (-0.003)).abs() < 1e-12);

    // Update design parameter `pcb_t` from 1.6 mm to 2.4 mm and rebuild on KernelV2Adapter!
    let t_new = 0.0024;
    b.state.engine.tree.parameters[0].expression = "2.4".to_string();
    b.rebuild_from_scratch();
    assert!(b.state.engine.errors.is_empty());
    let res_updated = b.op_result("ArcBoard").unwrap();
    assert_rel_close(
        exact_volume(&b, &res_updated.outputs[0].1.handle),
        net_area * t_new,
        1e-10,
        "updated thickness_expr exact volume",
    );
    let ev_updated = b.state.engine.board_shapes.get(&board_fid).unwrap();
    assert!((ev_updated.holes[0].frame.origin[1] - (-0.02 + t_new)).abs() < 1e-12);
}

#[test]
fn board_with_concave_arc_edge_notch_exact_volume() {
    let mut b = ModelBuilder::kernel_v2();

    // 50 mm × 40 mm board with a R = 5 mm semicircular concave edge notch
    // biting inward into the top edge centered at (25 mm, 40 mm), plus a hole
    // near the notch (outside the notch lune, inside the board).
    let w = 0.050;
    let h = 0.040;
    let rn = 0.005;
    let hole_r = 0.0015;
    let t = 0.0016;

    let entities = vec![
        SketchEntity::Point {
            id: 1,
            x: 0.0,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 2,
            x: w,
            y: 0.0,
            construction: false,
        },
        SketchEntity::Point {
            id: 3,
            x: w,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x: 0.030,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 5,
            x: 0.020,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 6,
            x: 0.0,
            y: h,
            construction: false,
        },
        SketchEntity::Point {
            id: 7,
            x: 0.025,
            y: h,
            construction: true,
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
        // CCW from 5 (angle π) to 4 (angle 0) passes through (0.025, 0.035);
        // traversed 4 -> 5 on the CCW outer loop it has `ccw: false` (concave!).
        SketchEntity::Arc {
            id: 13,
            center_id: 7,
            start_id: 5,
            end_id: 4,
            construction: false,
        },
        SketchEntity::Line {
            id: 14,
            start_id: 5,
            end_id: 6,
            construction: false,
        },
        SketchEntity::Line {
            id: 15,
            start_id: 6,
            end_id: 1,
            construction: false,
        },
        // Mounting hole below the notch at (0.025, 0.030).
        SketchEntity::Point {
            id: 20,
            x: 0.025,
            y: 0.030,
            construction: true,
        },
        SketchEntity::Circle {
            id: 21,
            center_id: 20,
            radius: hole_r,
            construction: false,
        },
        // Candidate hole INSIDE the concave notch cutout space at (0.025, 0.038) —
        // left unassigned in the valid build, then enabled to verify B22 refusal!
        SketchEntity::Point {
            id: 22,
            x: 0.025,
            y: 0.038,
            construction: true,
        },
        SketchEntity::Circle {
            id: 23,
            center_id: 22,
            radius: 0.001,
            construction: false,
        },
    ];

    let mut roles = BTreeMap::new();
    for id in 10..=15 {
        roles.insert(id, BoardEntityRole::Outline);
    }
    roles.insert(
        21,
        BoardEntityRole::Hole {
            plated: true,
            clearance: 0.0,
        },
    );

    let sketch_fid = add_sketch_feature(
        &mut b,
        "NotchSketch",
        xy_sketch(Uuid::new_v4(), entities, Vec::new()),
    );
    let board_fid = b
        .board_shape(
            "NotchedBoard",
            "NotchSketch",
            "notched_pcb",
            t,
            roles.clone(),
        )
        .expect("add NotchedBoard");
    assert!(b.engine_errors().is_empty(), "{:?}", b.engine_errors());

    let expected_area = w * h - 0.5 * PI * rn * rn - PI * hole_r * hole_r;
    let res = b.op_result("NotchedBoard").unwrap();
    assert_rel_close(
        exact_volume(&b, &res.outputs[0].1.handle),
        expected_area * t,
        1e-10,
        "concave-notched board exact volume",
    );

    // Enabling circle 23 (which sits at y=38 mm inside the R=5 mm concave notch lune!)
    // must be refused as HoleOutsideOutline — proving exact curved-lune winding!
    roles.insert(
        23,
        BoardEntityRole::Hole {
            plated: false,
            clearance: 0.0,
        },
    );
    b.dispatch(UiToEngine::EditFeature {
        feature_id: board_fid,
        provenance: None,
        operation: Operation::BoardShape {
            params: BoardShapeParams {
                sketch_id: sketch_fid,
                board: "notched_pcb".to_string(),
                thickness: t,
                thickness_expr: None,
                roles,
            },
        },
    });
    assert_eq!(
        b.state.engine.feature_errors[0].kind,
        ErrorKind::BoardShape {
            error: BoardShapeError::HoleOutsideOutline { entity_id: 23 }
        }
    );
}

// ── Exhaustive refusal of every invalid case (B1–B26) on KernelV2Adapter ─────

fn base_valid_rect_entities() -> Vec<SketchEntity> {
    vec![
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
            y: 0.040,
            construction: false,
        },
        SketchEntity::Point {
            id: 4,
            x: 0.0,
            y: 0.040,
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
    ]
}

fn base_outline_roles() -> BTreeMap<u32, BoardEntityRole> {
    let mut r = BTreeMap::new();
    for id in [10, 11, 12, 13] {
        r.insert(id, BoardEntityRole::Outline);
    }
    r
}

fn run_single_board_on_kv2(
    sketch: Sketch,
    board: &str,
    thickness: f64,
    roles: BTreeMap<u32, BoardEntityRole>,
) -> (Engine, Uuid) {
    let mut engine = Engine::new();
    let mut kv2 = KernelV2Adapter::new();
    let sketch_fid = engine
        .add_feature("Sketch".to_string(), Operation::Sketch { sketch }, &mut kv2)
        .expect("add sketch");
    let board_fid = engine
        .add_feature(
            "Board".to_string(),
            Operation::BoardShape {
                params: BoardShapeParams {
                    sketch_id: sketch_fid,
                    board: board.to_string(),
                    thickness,
                    thickness_expr: None,
                    roles,
                },
            },
            &mut kv2,
        )
        .expect("add board feature");
    (engine, board_fid)
}

fn assert_board_refused<F>(
    label: &str,
    sketch: Sketch,
    board: &str,
    thickness: f64,
    roles: BTreeMap<u32, BoardEntityRole>,
    pred: F,
) where
    F: FnOnce(&BoardShapeError) -> bool,
{
    let (engine, board_fid) = run_single_board_on_kv2(sketch, board, thickness, roles);
    assert!(
        engine.get_result(board_fid).is_none(),
        "{label}: invalid BoardShape must produce NO solid output"
    );
    assert!(
        !engine.board_shapes.contains_key(&board_fid),
        "{label}: invalid BoardShape must not populate Engine::board_shapes"
    );
    assert!(
        engine.connectors.is_empty(),
        "{label}: invalid BoardShape must emit zero PartConnectors"
    );
    let ferr = engine
        .feature_errors
        .iter()
        .find(|e| e.feature_id == board_fid)
        .unwrap_or_else(|| {
            panic!(
                "{label}: expected feature error on {board_fid}, got {:?}",
                engine.feature_errors
            )
        });
    match &ferr.kind {
        ErrorKind::BoardShape { error } => {
            assert!(
                pred(error),
                "{label}: unexpected BoardShapeError variant: {error:?}"
            );
        }
        other => panic!("{label}: expected ErrorKind::BoardShape, got {other:?}"),
    }
}

#[test]
fn every_invalid_role_assignment_refused_with_typed_error_on_kv2() {
    let t = 0.0016;

    // B1: SketchNotFound (missing UUID, and sketch ordered AFTER BoardShape).
    {
        let mut engine = Engine::new();
        let mut kv2 = KernelV2Adapter::new();
        let missing_id = Uuid::new_v4();
        let board_fid = engine
            .add_feature(
                "Board".to_string(),
                Operation::BoardShape {
                    params: BoardShapeParams {
                        sketch_id: missing_id,
                        board: "pcb".to_string(),
                        thickness: t,
                        thickness_expr: None,
                        roles: base_outline_roles(),
                    },
                },
                &mut kv2,
            )
            .unwrap();
        assert!(engine.get_result(board_fid).is_none());
        assert_eq!(
            engine.feature_errors[0].kind,
            ErrorKind::SketchNotFound { id: missing_id }
        );

        // Sketch added AFTER BoardShape is also refused as SketchNotFound.
        let late_sketch_fid = engine
            .add_feature(
                "LateSketch".to_string(),
                Operation::Sketch {
                    sketch: xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
                },
                &mut kv2,
            )
            .unwrap();
        engine
            .edit_feature(
                board_fid,
                Operation::BoardShape {
                    params: BoardShapeParams {
                        sketch_id: late_sketch_fid,
                        board: "pcb".to_string(),
                        thickness: t,
                        thickness_expr: None,
                        roles: base_outline_roles(),
                    },
                },
                &mut kv2,
            )
            .unwrap();
        assert_eq!(
            engine.feature_errors[0].kind,
            ErrorKind::SketchNotFound {
                id: late_sketch_fid
            }
        );
    }

    // B2: UnsolvedSketch (OverConstrained & SolveFailed).
    {
        let mut sk = xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new());
        sk.solve_status = SolveStatus::OverConstrained { conflicts: vec![0] };
        assert_board_refused(
            "B2 OverConstrained",
            sk,
            "pcb",
            t,
            base_outline_roles(),
            |e| matches!(e, BoardShapeError::UnsolvedSketch { .. }),
        );

        let mut sk2 = xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new());
        sk2.solve_status = SolveStatus::SolveFailed {
            reason: "diverged".to_string(),
        };
        assert_board_refused("B2 SolveFailed", sk2, "pcb", t, base_outline_roles(), |e| {
            matches!(e, BoardShapeError::UnsolvedSketch { .. })
        });
    }

    // B3: InvalidSketchPlane (zero-length normal on Engine + parallel x_axis & NaN origin).
    {
        let mut sk = xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new());
        sk.plane_normal = [0.0, 0.0, 0.0];
        sk.plane_x_axis = None;
        assert_board_refused("B3 zero normal", sk, "pcb", t, base_outline_roles(), |e| {
            matches!(e, BoardShapeError::InvalidSketchPlane { .. })
        });

        let mut sk_parallel = xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new());
        sk_parallel.plane_x_axis = Some([0.0, 0.0, 2.0]);
        let params = BoardShapeParams {
            sketch_id: sk_parallel.id,
            board: "pcb".to_string(),
            thickness: t,
            thickness_expr: None,
            roles: base_outline_roles(),
        };
        assert!(matches!(
            evaluate_board_shape(Uuid::new_v4(), &params, &sk_parallel),
            Err(BoardShapeError::InvalidSketchPlane { .. })
        ));
    }

    // B4: EmptyBoardName.
    for bad_name in ["", "   \t\n "] {
        assert_board_refused(
            "B4 EmptyBoardName",
            xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
            bad_name,
            t,
            base_outline_roles(),
            |e| matches!(e, BoardShapeError::EmptyBoardName),
        );
    }

    // B5: InvalidThickness.
    for bad_t in [0.0, -0.001, 1e-7, f64::NAN, f64::INFINITY] {
        assert_board_refused(
            "B5 InvalidThickness",
            xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
            "pcb",
            bad_t,
            base_outline_roles(),
            |e| matches!(e, BoardShapeError::InvalidThickness { .. }),
        );
    }

    // B6: UnknownEntity.
    {
        let mut roles = base_outline_roles();
        roles.insert(999, BoardEntityRole::Cutout);
        assert_board_refused(
            "B6 UnknownEntity",
            xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::UnknownEntity { entity_id: 999 }),
        );
    }

    // B7: ConstructionEntity.
    {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 50,
            x: 0.025,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 51,
            center_id: 50,
            radius: 0.002,
            construction: true,
        });
        let mut roles = base_outline_roles();
        roles.insert(
            51,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            "B7 ConstructionEntity",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::ConstructionEntity { entity_id: 51, role } if role == "hole"),
        );
    }

    // B8: UnsupportedEntityKind (Point, Spline, Gear, Sprocket).
    {
        // Point
        let mut roles_pt = base_outline_roles();
        roles_pt.insert(1, BoardEntityRole::Outline);
        assert_board_refused(
            "B8 Point",
            xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
            "pcb",
            t,
            roles_pt,
            |e| matches!(e, BoardShapeError::UnsupportedEntityKind { entity_id: 1, kind, .. } if kind == "Point"),
        );

        // Spline
        let mut ents_sp = base_valid_rect_entities();
        ents_sp.push(SketchEntity::Spline {
            id: 60,
            point_ids: vec![1, 2, 3],
            construction: false,
        });
        let mut roles_sp = base_outline_roles();
        roles_sp.insert(60, BoardEntityRole::Cutout);
        assert_board_refused(
            "B8 Spline",
            xy_sketch(Uuid::new_v4(), ents_sp, Vec::new()),
            "pcb",
            t,
            roles_sp,
            |e| matches!(e, BoardShapeError::UnsupportedEntityKind { entity_id: 60, kind, .. } if kind == "Spline"),
        );

        // Gear
        let mut ents_gear = base_valid_rect_entities();
        ents_gear.push(SketchEntity::Gear {
            id: 61,
            params: GearParams::default(),
            construction: false,
        });
        let mut roles_gear = base_outline_roles();
        roles_gear.insert(61, BoardEntityRole::Outline);
        assert_board_refused(
            "B8 Gear",
            xy_sketch(Uuid::new_v4(), ents_gear, Vec::new()),
            "pcb",
            t,
            roles_gear,
            |e| matches!(e, BoardShapeError::UnsupportedEntityKind { entity_id: 61, kind, .. } if kind == "Gear"),
        );

        // Sprocket
        let mut ents_spr = base_valid_rect_entities();
        ents_spr.push(SketchEntity::Sprocket {
            id: 62,
            params: SprocketParams::default(),
            construction: false,
        });
        let mut roles_spr = base_outline_roles();
        roles_spr.insert(62, BoardEntityRole::Cutout);
        assert_board_refused(
            "B8 Sprocket",
            xy_sketch(Uuid::new_v4(), ents_spr, Vec::new()),
            "pcb",
            t,
            roles_spr,
            |e| matches!(e, BoardShapeError::UnsupportedEntityKind { entity_id: 62, kind, .. } if kind == "Sprocket"),
        );
    }

    // B9: HoleNotCircle (Line and Arc).
    {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 70,
            x: 0.020,
            y: 0.020,
            construction: false,
        });
        ents.push(SketchEntity::Point {
            id: 71,
            x: 0.025,
            y: 0.020,
            construction: false,
        });
        ents.push(SketchEntity::Point {
            id: 72,
            x: 0.020,
            y: 0.025,
            construction: false,
        });
        ents.push(SketchEntity::Line {
            id: 73,
            start_id: 70,
            end_id: 71,
            construction: false,
        });
        ents.push(SketchEntity::Arc {
            id: 74,
            center_id: 70,
            start_id: 71,
            end_id: 72,
            construction: false,
        });

        let mut roles_line = base_outline_roles();
        roles_line.insert(
            73,
            BoardEntityRole::Hole {
                plated: false,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            "B9 HoleNotCircle Line",
            xy_sketch(Uuid::new_v4(), ents.clone(), Vec::new()),
            "pcb",
            t,
            roles_line,
            |e| matches!(e, BoardShapeError::HoleNotCircle { entity_id: 73, kind } if kind == "Line"),
        );

        let mut roles_arc = base_outline_roles();
        roles_arc.insert(
            74,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            "B9 HoleNotCircle Arc",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles_arc,
            |e| matches!(e, BoardShapeError::HoleNotCircle { entity_id: 74, kind } if kind == "Arc"),
        );
    }

    // B10: InvalidHoleClearance.
    for bad_clr in [-1e-4, f64::NAN, f64::INFINITY] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 80,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 81,
            center_id: 80,
            radius: 0.002,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(
            81,
            BoardEntityRole::Hole {
                plated: true,
                clearance: bad_clr,
            },
        );
        assert_board_refused(
            "B10 InvalidHoleClearance",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| {
                matches!(
                    e,
                    BoardShapeError::InvalidHoleClearance { entity_id: 81, .. }
                )
            },
        );
    }

    // B11: InvalidKeepoutHeight.
    for bad_h in [-0.001, f64::NAN, f64::INFINITY] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 82,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 83,
            center_id: 82,
            radius: 0.004,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(
            83,
            BoardEntityRole::Keepout {
                side: BoardSide::Front,
                max_height: bad_h,
            },
        );
        assert_board_refused(
            "B11 InvalidKeepoutHeight",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| {
                matches!(
                    e,
                    BoardShapeError::InvalidKeepoutHeight { entity_id: 83, .. }
                )
            },
        );
    }

    // B12: DegenerateEntity (zero-length Line, sub-micron Circle, mismatched-radii Arc).
    {
        // Zero-length Line
        let mut ents_line = base_valid_rect_entities();
        ents_line.push(SketchEntity::Point {
            id: 90,
            x: 0.010,
            y: 0.010,
            construction: false,
        });
        ents_line.push(SketchEntity::Point {
            id: 91,
            x: 0.010,
            y: 0.010,
            construction: false,
        });
        ents_line.push(SketchEntity::Line {
            id: 92,
            start_id: 90,
            end_id: 91,
            construction: false,
        });
        let mut roles_line = base_outline_roles();
        roles_line.insert(92, BoardEntityRole::Cutout);
        assert_board_refused(
            "B12 DegenerateEntity Line",
            xy_sketch(Uuid::new_v4(), ents_line, Vec::new()),
            "pcb",
            t,
            roles_line,
            |e| matches!(e, BoardShapeError::DegenerateEntity { entity_id: 92, .. }),
        );

        // Sub-micron Circle (radius < 0.5 * MIN_FEATURE_SIZE = 5e-7)
        let mut ents_circ = base_valid_rect_entities();
        ents_circ.push(SketchEntity::Point {
            id: 93,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents_circ.push(SketchEntity::Circle {
            id: 94,
            center_id: 93,
            radius: 1e-7,
            construction: false,
        });
        let mut roles_circ = base_outline_roles();
        roles_circ.insert(
            94,
            BoardEntityRole::Hole {
                plated: false,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            "B12 DegenerateEntity Circle",
            xy_sketch(Uuid::new_v4(), ents_circ, Vec::new()),
            "pcb",
            t,
            roles_circ,
            |e| matches!(e, BoardShapeError::DegenerateEntity { entity_id: 94, .. }),
        );

        // Mismatched-radii Arc
        let mut ents_arc = base_valid_rect_entities();
        ents_arc.push(SketchEntity::Point {
            id: 95,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents_arc.push(SketchEntity::Point {
            id: 96,
            x: 0.025,
            y: 0.020,
            construction: false,
        }); // r = 5 mm
        ents_arc.push(SketchEntity::Point {
            id: 97,
            x: 0.020,
            y: 0.028,
            construction: false,
        }); // r = 8 mm
        ents_arc.push(SketchEntity::Arc {
            id: 98,
            center_id: 95,
            start_id: 96,
            end_id: 97,
            construction: false,
        });
        let mut roles_arc = base_outline_roles();
        roles_arc.insert(98, BoardEntityRole::Cutout);
        assert_board_refused(
            "B12 DegenerateEntity Arc",
            xy_sketch(Uuid::new_v4(), ents_arc, Vec::new()),
            "pcb",
            t,
            roles_arc,
            |e| matches!(e, BoardShapeError::DegenerateEntity { entity_id: 98, .. }),
        );
    }

    // B13: MissingOutline (empty roles, and roles with only hole).
    {
        assert_board_refused(
            "B13 MissingOutline empty",
            xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
            "pcb",
            t,
            BTreeMap::new(),
            |e| matches!(e, BoardShapeError::MissingOutline),
        );

        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 99,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 100,
            center_id: 99,
            radius: 0.002,
            construction: false,
        });
        let mut only_hole = BTreeMap::new();
        only_hole.insert(
            100,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            "B13 MissingOutline only hole",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            only_hole,
            |e| matches!(e, BoardShapeError::MissingOutline),
        );
    }

    // B14: OutlineNotClosed (3 sides of rectangle).
    {
        let mut open_roles = base_outline_roles();
        open_roles.remove(&13);
        assert_board_refused(
            "B14 OutlineNotClosed",
            xy_sketch(Uuid::new_v4(), base_valid_rect_entities(), Vec::new()),
            "pcb",
            t,
            open_roles,
            |e| matches!(e, BoardShapeError::OutlineNotClosed { gap_m, .. } if *gap_m > 0.03),
        );
    }

    // B15: OutlineBranching (rectangle + interior diagonal chord sharing vertices 1 and 3 ⇒ degree 3).
    {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Line {
            id: 14,
            start_id: 1,
            end_id: 3,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(14, BoardEntityRole::Outline);
        assert_board_refused(
            "B15 OutlineBranching",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::OutlineBranching { degree: 3, .. }),
        );
    }

    // B16: MultipleOutlines (rectangle + separate circle both marked Outline).
    {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 101,
            x: 0.100,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 102,
            center_id: 101,
            radius: 0.010,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(102, BoardEntityRole::Outline);
        assert_board_refused(
            "B16 MultipleOutlines",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::MultipleOutlines { count: 2 }),
        );
    }

    // B17: SelfIntersectingLoop (bowtie/figure-8 outline).
    {
        let bowtie_ents = vec![
            SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 2,
                x: 0.040,
                y: 0.040,
                construction: false,
            },
            SketchEntity::Point {
                id: 3,
                x: 0.040,
                y: 0.0,
                construction: false,
            },
            SketchEntity::Point {
                id: 4,
                x: 0.0,
                y: 0.040,
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
        ];
        assert_board_refused(
            "B17 SelfIntersectingLoop",
            xy_sketch(Uuid::new_v4(), bowtie_ents, Vec::new()),
            "pcb",
            t,
            base_outline_roles(),
            |e| matches!(e, BoardShapeError::SelfIntersectingLoop { role, .. } if role == "outline"),
        );
    }

    // B18 & B19: CutoutNotClosed and CutoutBranching.
    {
        let mut ents = base_valid_rect_entities();
        ents.extend([
            SketchEntity::Point {
                id: 110,
                x: 0.010,
                y: 0.010,
                construction: false,
            },
            SketchEntity::Point {
                id: 111,
                x: 0.020,
                y: 0.010,
                construction: false,
            },
            SketchEntity::Point {
                id: 112,
                x: 0.020,
                y: 0.020,
                construction: false,
            },
            SketchEntity::Point {
                id: 113,
                x: 0.010,
                y: 0.020,
                construction: false,
            },
            SketchEntity::Line {
                id: 120,
                start_id: 110,
                end_id: 111,
                construction: false,
            },
            SketchEntity::Line {
                id: 121,
                start_id: 111,
                end_id: 112,
                construction: false,
            },
            SketchEntity::Line {
                id: 122,
                start_id: 112,
                end_id: 113,
                construction: false,
            },
            SketchEntity::Line {
                id: 123,
                start_id: 113,
                end_id: 110,
                construction: false,
            },
            SketchEntity::Line {
                id: 124,
                start_id: 110,
                end_id: 112,
                construction: false,
            },
        ]);

        let mut open_cut = base_outline_roles();
        for id in [120, 121, 122] {
            open_cut.insert(id, BoardEntityRole::Cutout);
        }
        assert_board_refused(
            "B18 CutoutNotClosed",
            xy_sketch(Uuid::new_v4(), ents.clone(), Vec::new()),
            "pcb",
            t,
            open_cut,
            |e| matches!(e, BoardShapeError::CutoutNotClosed { .. }),
        );

        let mut branch_cut = base_outline_roles();
        for id in [120, 121, 122, 123, 124] {
            branch_cut.insert(id, BoardEntityRole::Cutout);
        }
        assert_board_refused(
            "B19 CutoutBranching",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            branch_cut,
            |e| matches!(e, BoardShapeError::CutoutBranching { degree: 3, .. }),
        );
    }

    // B20: CutoutOutsideOutline (disjoint outside, crossing boundary, and enclosing outline).
    for (label, cx, cy, r) in [
        ("B20 disjoint outside", 0.080, 0.020, 0.005),
        ("B20 crossing boundary", 0.050, 0.020, 0.005),
        ("B20 enclosing outline", 0.025, 0.020, 0.060),
    ] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 130,
            x: cx,
            y: cy,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 131,
            center_id: 130,
            radius: r,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(131, BoardEntityRole::Cutout);
        assert_board_refused(
            label,
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::CutoutOutsideOutline { .. }),
        );
    }

    // B21: CutoutsOverlapOrNested (crossing cutouts & concentric nested cutouts).
    for (label, c2x, r1, r2) in [
        ("B21 crossing cutouts", 0.024, 0.005, 0.005),
        ("B21 nested cutouts", 0.020, 0.008, 0.003),
    ] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 140,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 141,
            center_id: 140,
            radius: r1,
            construction: false,
        });
        ents.push(SketchEntity::Point {
            id: 142,
            x: c2x,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 143,
            center_id: 142,
            radius: r2,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(141, BoardEntityRole::Cutout);
        roles.insert(143, BoardEntityRole::Cutout);
        assert_board_refused(
            label,
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::CutoutsOverlapOrNested { .. }),
        );
    }

    // B22: HoleOutsideOutline (center outside, and interior center whose radius touches outline).
    for (label, hx, hy, hr) in [
        ("B22 center outside", -0.005, 0.020, 0.0015),
        ("B22 tangent to outline", 0.0015, 0.020, 0.0015),
    ] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 150,
            x: hx,
            y: hy,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 151,
            center_id: 150,
            radius: hr,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(
            151,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            label,
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| matches!(e, BoardShapeError::HoleOutsideOutline { entity_id: 151 }),
        );
    }

    // B23: HoleOverlapsCutout (hole inside cutout, and hole grazing cutout wall).
    for (label, hx) in [
        ("B23 hole inside cutout", 0.025),
        ("B23 hole grazing cutout", 0.025 + 0.006 + 0.0015),
    ] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 160,
            x: 0.025,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 161,
            center_id: 160,
            radius: 0.006,
            construction: false,
        });
        ents.push(SketchEntity::Point {
            id: 162,
            x: hx,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 163,
            center_id: 162,
            radius: 0.0015,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(161, BoardEntityRole::Cutout);
        roles.insert(
            163,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            label,
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| {
                matches!(
                    e,
                    BoardShapeError::HoleOverlapsCutout {
                        hole_entity_id: 163,
                        ..
                    }
                )
            },
        );
    }

    // B24: HolesOverlap (overlapping & tangent holes).
    for (label, h2x) in [
        ("B24 overlapping holes", 0.022),
        ("B24 tangent holes", 0.024),
    ] {
        let mut ents = base_valid_rect_entities();
        ents.push(SketchEntity::Point {
            id: 170,
            x: 0.020,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 171,
            center_id: 170,
            radius: 0.002,
            construction: false,
        });
        ents.push(SketchEntity::Point {
            id: 172,
            x: h2x,
            y: 0.020,
            construction: true,
        });
        ents.push(SketchEntity::Circle {
            id: 173,
            center_id: 172,
            radius: 0.002,
            construction: false,
        });
        let mut roles = base_outline_roles();
        roles.insert(
            171,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        roles.insert(
            173,
            BoardEntityRole::Hole {
                plated: true,
                clearance: 0.0,
            },
        );
        assert_board_refused(
            label,
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            roles,
            |e| {
                matches!(
                    e,
                    BoardShapeError::HolesOverlap {
                        first_entity_id: 171,
                        second_entity_id: 173
                    }
                )
            },
        );
    }

    // B25 & B26: KeepoutNotClosed and KeepoutBranching.
    {
        let mut ents = base_valid_rect_entities();
        ents.extend([
            SketchEntity::Point {
                id: 180,
                x: 0.010,
                y: 0.010,
                construction: false,
            },
            SketchEntity::Point {
                id: 181,
                x: 0.020,
                y: 0.010,
                construction: false,
            },
            SketchEntity::Point {
                id: 182,
                x: 0.020,
                y: 0.020,
                construction: false,
            },
            SketchEntity::Point {
                id: 183,
                x: 0.010,
                y: 0.020,
                construction: false,
            },
            SketchEntity::Line {
                id: 190,
                start_id: 180,
                end_id: 181,
                construction: false,
            },
            SketchEntity::Line {
                id: 191,
                start_id: 181,
                end_id: 182,
                construction: false,
            },
            SketchEntity::Line {
                id: 192,
                start_id: 182,
                end_id: 183,
                construction: false,
            },
            SketchEntity::Line {
                id: 193,
                start_id: 183,
                end_id: 180,
                construction: false,
            },
            SketchEntity::Line {
                id: 194,
                start_id: 180,
                end_id: 182,
                construction: false,
            },
        ]);

        let mut open_ko = base_outline_roles();
        for id in [190, 191, 192] {
            open_ko.insert(
                id,
                BoardEntityRole::Keepout {
                    side: BoardSide::Back,
                    max_height: 0.002,
                },
            );
        }
        assert_board_refused(
            "B25 KeepoutNotClosed",
            xy_sketch(Uuid::new_v4(), ents.clone(), Vec::new()),
            "pcb",
            t,
            open_ko,
            |e| matches!(e, BoardShapeError::KeepoutNotClosed { .. }),
        );

        let mut branch_ko = base_outline_roles();
        for id in [190, 191, 192, 193, 194] {
            branch_ko.insert(
                id,
                BoardEntityRole::Keepout {
                    side: BoardSide::Back,
                    max_height: 0.002,
                },
            );
        }
        assert_board_refused(
            "B26 KeepoutBranching",
            xy_sketch(Uuid::new_v4(), ents, Vec::new()),
            "pcb",
            t,
            branch_ko,
            |e| matches!(e, BoardShapeError::KeepoutBranching { degree: 3, .. }),
        );
    }
}
