//! ICR-1 of `specs/waffle_mcp_server.md`: `MeasureBody` answers a body's
//! volume and surface area from the B-Rep when the kernel can integrate it
//! exactly, and otherwise from the render mesh — ALWAYS saying which, with the
//! kernel's reason, so a chordal number is never presented as exact (§2.6).

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_v2::KernelV2Adapter;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

const CUBE_STEP: &str = include_str!("../../step-import/tests/fixtures/cube.step");

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

/// Spec oracle O1: a 20 mm × 10 mm rectangle on XY extruded 5 mm. Returns the
/// extrude feature id.
fn box_20_10_5(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> Uuid {
    let corners = [
        (1, 0.0, 0.0),
        (2, 0.02, 0.0),
        (3, 0.02, 0.01),
        (4, 0.0, 0.01),
    ];
    let solved_positions: HashMap<u32, (f64, f64)> =
        corners.iter().map(|&(id, x, y)| (id, (x, y))).collect();
    let mut entities: Vec<SketchEntity> =
        corners.iter().map(|&(id, x, y)| point(id, x, y)).collect();
    entities.extend([
        line(10, 1, 2),
        line(11, 2, 3),
        line(12, 3, 4),
        line(13, 4, 1),
    ]);
    let sketch = Sketch {
        id: Uuid::new_v4(),
        plane: datum_xy(),
        plane_origin: [0.0, 0.0, 0.0],
        plane_normal: [0.0, 0.0, 1.0],
        plane_x_axis: None,
        entities,
        constraints: Vec::new(),
        solve_status: SolveStatus::FullyConstrained,
        solved_positions,
        projected: Vec::new(),
        solved_profiles: vec![ClosedProfile {
            entity_ids: vec![10, 11, 12, 13],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }],
    };
    let sketch_feature = added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Sketch { sketch },
            provenance: None,
        },
        kernel,
    ));
    added_id(dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: sketch_feature,
                    profile_index: 0,
                    profile_entity_ids: Some(vec![10, 11, 12, 13]),
                    depth: 0.005,
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
            },
            provenance: None,
        },
        kernel,
    ))
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

fn measure(state: &mut EngineState, kernel: &mut KernelV2Adapter, body_id: &str) -> EngineToUi {
    dispatch(
        state,
        UiToEngine::MeasureBody {
            body_id: body_id.to_string(),
        },
        kernel,
    )
}

#[test]
fn a_planar_box_measures_exactly() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let extrude = box_20_10_5(&mut state, &mut kernel);
    let body_id = FeatureTree::body_id(extrude, &OutputKey::Main);

    let EngineToUi::BodyMeasured {
        body_id: answered,
        volume_m3,
        surface_area_m2,
        bbox_min,
        bbox_max,
        face_count,
        edge_count,
        vertex_count,
        closed,
    } = measure(&mut state, &mut kernel, &body_id)
    else {
        panic!("expected BodyMeasured");
    };
    assert_eq!(answered, body_id);

    assert_eq!(volume_m3.method, MeasureMethod::Exact, "{volume_m3:?}");
    assert!((volume_m3.value - 1.0e-6).abs() <= 1e-15, "{volume_m3:?}");
    assert_eq!(volume_m3.exact_unavailable, None);

    assert_eq!(surface_area_m2.method, MeasureMethod::Exact);
    let area = 2.0 * (0.02 * 0.01 + 0.01 * 0.005 + 0.02 * 0.005);
    assert!(
        (surface_area_m2.value - area).abs() <= 1e-15,
        "{surface_area_m2:?}"
    );

    // The sketch plane is given by origin + normal only, so its in-plane axes
    // are the engine's choice: check the extents, not which world axis each
    // side lands on. The extrusion is along +z from the plane.
    let mut extents: Vec<f64> = (0..3).map(|a| bbox_max[a] - bbox_min[a]).collect();
    extents.sort_by(f64::total_cmp);
    for (got, want) in extents.iter().zip([0.005, 0.01, 0.02]) {
        assert!(
            (got - want).abs() <= 1e-7,
            "bbox {bbox_min:?}..{bbox_max:?}"
        );
    }
    assert!(bbox_min[2].abs() <= 1e-7 && (bbox_max[2] - 0.005).abs() <= 1e-7);
    assert_eq!((face_count, edge_count, vertex_count), (6, 12, 8));
    assert!(closed);
}

#[test]
fn a_body_the_kernel_cannot_integrate_falls_back_to_the_mesh_and_says_why() {
    // An imported STEP body is mesh-backed in kernel-v2: no exact integral.
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let import = added_id(dispatch(
        &mut state,
        UiToEngine::ImportStep {
            file_name: "cube.step".to_string(),
            data: CUBE_STEP.to_string(),
        },
        &mut kernel,
    ));
    let body_id = FeatureTree::body_id(import, &OutputKey::Main);

    let EngineToUi::BodyMeasured { volume_m3, .. } = measure(&mut state, &mut kernel, &body_id)
    else {
        panic!("expected BodyMeasured");
    };
    assert_eq!(volume_m3.method, MeasureMethod::Mesh);
    assert!(volume_m3.value > 0.0, "{volume_m3:?}");
    let reason = volume_m3
        .exact_unavailable
        .expect("a mesh fallback names the kernel's reason");
    assert!(!reason.is_empty());
}

#[test]
fn an_unknown_body_is_a_loud_error() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let response = measure(&mut state, &mut kernel, "no-such-feature/Main");
    let EngineToUi::Error { message, .. } = response else {
        panic!("expected Error, got {response:?}");
    };
    assert!(message.contains("no-such-feature/Main"), "{message}");
}

#[test]
fn wire_names_the_method() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let extrude = box_20_10_5(&mut state, &mut kernel);
    let body_id = FeatureTree::body_id(extrude, &OutputKey::Main);
    let json = serde_json::to_value(measure(&mut state, &mut kernel, &body_id)).unwrap();
    assert_eq!(json["type"], "BodyMeasured");
    assert_eq!(json["volume_m3"]["method"], "exact");
    assert!(json["volume_m3"].get("exact_unavailable").is_none());
}
