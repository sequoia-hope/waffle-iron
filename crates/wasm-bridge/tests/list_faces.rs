//! ICR-3 of `specs/waffle_mcp_server.md`: `ListFaces` gives a host every face
//! of a body as the SAME `GeomRef` the viewport's face ranges carry, plus its
//! geometric signature, so an agent can address "the top face" without a pick
//! — and a listed ref is a real engine reference: it resolves.

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

/// 20 mm × 10 mm × 5 mm box on XY; returns its body id.
fn box_body(state: &mut EngineState, kernel: &mut KernelV2Adapter) -> String {
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
    let extrude = added_id(dispatch(
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
    ));
    FeatureTree::body_id(extrude, &OutputKey::Main)
}

fn list(
    state: &mut EngineState,
    kernel: &mut KernelV2Adapter,
    body_id: &str,
    filter: Option<TopoQuery>,
) -> Vec<ListedFace> {
    match dispatch(
        state,
        UiToEngine::ListFaces {
            body_id: body_id.to_string(),
            filter,
        },
        kernel,
    ) {
        EngineToUi::FacesListed {
            body_id: answered,
            faces,
        } => {
            assert_eq!(answered, body_id);
            faces
        }
        other => panic!("expected FacesListed, got {other:?}"),
    }
}

fn role_of(face: &ListedFace) -> Option<&Role> {
    match &face.geom_ref.selector {
        Selector::Role { role, .. } => Some(role),
        _ => None,
    }
}

#[test]
fn a_box_lists_six_role_addressed_faces_with_signatures() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = box_body(&mut state, &mut kernel);

    let faces = list(&mut state, &mut kernel, &body, None);
    assert_eq!(faces.len(), 6, "{faces:#?}");
    assert_eq!(
        faces
            .iter()
            .filter(|f| role_of(f) == Some(&Role::EndCapPositive))
            .count(),
        1
    );
    assert_eq!(
        faces
            .iter()
            .filter(|f| matches!(role_of(f), Some(Role::SideFace { .. })))
            .count(),
        4
    );
    let total: f64 = faces.iter().map(|f| f.signature.area.unwrap()).sum();
    let expected = 2.0 * (0.02 * 0.01 + 0.01 * 0.005 + 0.02 * 0.005);
    assert!(
        (total - expected).abs() <= 1e-12,
        "{total:e} vs {expected:e}"
    );
}

#[test]
fn a_filter_picks_the_top_face() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = box_body(&mut state, &mut kernel);

    let top = list(
        &mut state,
        &mut kernel,
        &body,
        Some(TopoQuery {
            filters: vec![
                Filter::SurfaceType {
                    surface_type: "planar".to_string(),
                },
                Filter::NormalDirection {
                    direction: [0.0, 0.0, 1.0],
                    tolerance: 1e-6,
                },
            ],
            tie_break: None,
        }),
    );
    assert_eq!(top.len(), 1, "{top:#?}");
    assert_eq!(role_of(&top[0]), Some(&Role::EndCapPositive));
}

#[test]
fn a_filter_that_matches_nothing_is_an_empty_list() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = box_body(&mut state, &mut kernel);
    let none = list(
        &mut state,
        &mut kernel,
        &body,
        Some(TopoQuery {
            filters: vec![Filter::SurfaceType {
                surface_type: "toroidal".to_string(),
            }],
            tie_break: None,
        }),
    );
    assert!(none.is_empty());
}

#[test]
fn every_listed_ref_resolves_to_its_own_face() {
    // A listed ref is an engine reference, not a label: it resolves, and to
    // the face whose signature was listed with it.
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = box_body(&mut state, &mut kernel);

    for face in list(&mut state, &mut kernel, &body, None) {
        let (_, normal) = feature_engine::rebuild::resolve_face_plane(
            &face.geom_ref,
            &state.engine.feature_results,
            &kernel,
        )
        .unwrap_or_else(|e| panic!("{:?} does not resolve: {e}", face.geom_ref));
        let listed = face.signature.normal.expect("planar faces carry a normal");
        for (a, b) in normal.iter().zip(listed) {
            assert!((a - b).abs() <= 1e-9, "{normal:?} vs {listed:?}");
        }
    }
}

#[test]
fn the_listing_is_deterministic_and_canonically_ordered() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let body = box_body(&mut state, &mut kernel);

    let first = list(&mut state, &mut kernel, &body, None);
    let second = list(&mut state, &mut kernel, &body, None);
    let keys = |faces: &[ListedFace]| -> Vec<String> {
        faces
            .iter()
            .map(|f| serde_json::to_string(&f.geom_ref).unwrap())
            .collect()
    };
    assert_eq!(keys(&first), keys(&second));
    let mut sorted = keys(&first);
    sorted.sort();
    assert_eq!(keys(&first), sorted, "ordered by canonical GeomRef JSON");
}

#[test]
fn an_unknown_body_is_a_loud_error() {
    let mut state = EngineState::new();
    let mut kernel = KernelV2Adapter::new();
    let response = dispatch(
        &mut state,
        UiToEngine::ListFaces {
            body_id: "no-such-feature/Main".to_string(),
            filter: None,
        },
        &mut kernel,
    );
    let EngineToUi::Error { message, .. } = response else {
        panic!("expected Error, got {response:?}");
    };
    assert!(message.contains("no-such-feature/Main"), "{message}");
}
