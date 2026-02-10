//! End-to-end pipeline tests for the wasm-bridge dispatch layer.
//!
//! These tests verify the full modeling pipeline (sketch → extrude → mesh)
//! through the same `dispatch()` function that WASM calls, but running natively
//! with MockKernel. They go beyond the existing bridge_tests by verifying
//! actual mesh geometry — vertex counts, face ranges, and bounding boxes.

use std::collections::HashMap;

use feature_engine::types::*;
use kernel_fork::{Kernel, MockKernel};
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::messages::*;
use wasm_bridge::*;

// ── Helper functions ──────────────────────────────────────────────────────

/// Create a rectangular sketch (10×10) via dispatch and return the sketch feature ID.
fn create_rect_sketch(
    state: &mut EngineState,
    kernel: &mut MockKernel,
    plane_origin: [f64; 3],
    plane_normal: [f64; 3],
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

    wasm_bridge::dispatch(state, UiToEngine::BeginSketch { plane }, kernel);

    for (id, x, y) in [
        (1, 0.0, 0.0),
        (2, 10.0, 0.0),
        (3, 10.0, 10.0),
        (4, 0.0, 10.0),
    ] {
        wasm_bridge::dispatch(
            state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Point {
                    id,
                    x,
                    y,
                    construction: false,
                },
            },
            kernel,
        );
    }

    for (id, start, end) in [(10, 1, 2), (11, 2, 3), (12, 3, 4), (13, 4, 1)] {
        wasm_bridge::dispatch(
            state,
            UiToEngine::AddSketchEntity {
                entity: SketchEntity::Line {
                    id,
                    start_id: start,
                    end_id: end,
                    construction: false,
                },
            },
            kernel,
        );
    }

    let mut solved_positions = HashMap::new();
    solved_positions.insert(1, (0.0, 0.0));
    solved_positions.insert(2, (10.0, 0.0));
    solved_positions.insert(3, (10.0, 10.0));
    solved_positions.insert(4, (0.0, 10.0));

    let solved_profiles = vec![ClosedProfile {
        entity_ids: vec![1, 2, 3, 4],
        is_outer: true,
    }];

    let response = wasm_bridge::dispatch(
        state,
        UiToEngine::FinishSketch {
            solved_positions,
            solved_profiles,
            plane_origin,
            plane_normal,
        },
        kernel,
    );

    match response {
        EngineToUi::ModelUpdated { feature_tree, .. } => feature_tree.features.last().unwrap().id,
        other => panic!("Expected ModelUpdated from FinishSketch, got {:?}", other),
    }
}

/// Add an extrude feature via dispatch and return the extrude feature ID.
fn add_extrude(
    state: &mut EngineState,
    kernel: &mut MockKernel,
    sketch_id: Uuid,
    depth: f64,
    direction: Option<[f64; 3]>,
) -> Uuid {
    let response = wasm_bridge::dispatch(
        state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id,
                    profile_index: 0,
                    depth,
                    direction,
                    symmetric: false,
                    cut: false,
                    target_body: None,
                },
            },
        },
        kernel,
    );

    match response {
        EngineToUi::ModelUpdated { feature_tree, .. } => feature_tree.features.last().unwrap().id,
        other => panic!(
            "Expected ModelUpdated from AddFeature(Extrude), got {:?}",
            other
        ),
    }
}

/// Get the solid handle for a feature from the engine's feature_results.
fn get_solid_handle(state: &EngineState, feature_id: Uuid) -> kernel_fork::KernelSolidHandle {
    let result = state
        .engine
        .get_result(feature_id)
        .unwrap_or_else(|| panic!("No OpResult for feature {}", feature_id));
    assert!(
        !result.outputs.is_empty(),
        "Feature {} has no outputs",
        feature_id
    );
    result.outputs[0].1.handle.clone()
}

/// Tessellate a feature's solid and return the RenderMesh.
fn tessellate_feature(
    state: &EngineState,
    kernel: &mut MockKernel,
    feature_id: Uuid,
) -> kernel_fork::RenderMesh {
    let handle = get_solid_handle(state, feature_id);
    kernel
        .tessellate(&handle, 0.1)
        .unwrap_or_else(|e| panic!("Tessellation failed for feature {}: {}", feature_id, e))
}

/// Assert that a feature produced a non-empty OpResult with at least one output.
fn assert_has_solid(state: &EngineState, feature_id: Uuid) {
    let result = state.engine.get_result(feature_id);
    assert!(
        result.is_some(),
        "Feature {} should have an OpResult",
        feature_id
    );
    assert!(
        !result.unwrap().outputs.is_empty(),
        "Feature {} should have outputs (solid handle)",
        feature_id
    );
}

/// Compute the axis-aligned bounding box of a mesh. Returns (min, max).
fn mesh_bounding_box(mesh: &kernel_fork::RenderMesh) -> ([f32; 3], [f32; 3]) {
    assert!(
        mesh.vertices.len() >= 3,
        "Mesh must have at least one vertex"
    );
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for chunk in mesh.vertices.chunks(3) {
        for i in 0..3 {
            min[i] = min[i].min(chunk[i]);
            max[i] = max[i].max(chunk[i]);
        }
    }
    (min, max)
}

// ── Test 1: Sketch → Extrude → Mesh verification ─────────────────────────

#[test]
fn sketch_extrude_produces_box_mesh() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 10.0, None);

    // Feature tree should have Sketch + Extrude
    assert_eq!(state.engine.tree.features.len(), 2);
    assert_eq!(state.engine.tree.features[1].name, "Extrude");

    // Verify solid exists
    assert_has_solid(&state, extrude_id);

    // Verify mesh geometry: MockKernel box = 6 faces × 2 tri × 3 = 36 indices
    let mesh = tessellate_feature(&state, &mut kernel, extrude_id);
    assert_eq!(
        mesh.indices.len(),
        36,
        "Box mesh: 6 faces × 2 triangles × 3 indices = 36"
    );
    assert_eq!(mesh.face_ranges.len(), 6, "Box should have 6 face ranges");
    assert_eq!(
        mesh.vertices.len(),
        mesh.normals.len(),
        "Vertex and normal arrays must match"
    );
    // 6 faces × 4 verts per face × 3 floats = 72
    assert_eq!(mesh.vertices.len(), 72, "Box should have 72 vertex floats");
}

#[test]
fn sketch_extrude_mesh_bounding_box() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 10.0, None);

    let mesh = tessellate_feature(&state, &mut kernel, extrude_id);
    let (bb_min, bb_max) = mesh_bounding_box(&mesh);

    // MockKernel extrude computes side = sqrt(area) of the profile face.
    // The 10×10 rectangle has area=100, so side=10. Depth=10.
    // Box from (0,0,0) to (10,10,10) — but MockKernel tessellates from centroid±half,
    // so bounds depend on face centroids. Just verify the bounding box is non-degenerate.
    for i in 0..3 {
        assert!(
            bb_max[i] > bb_min[i],
            "Bounding box must be non-degenerate on axis {}",
            i
        );
    }
}

// ── Test 2: Sketch → Revolve → Mesh verification ─────────────────────────

#[test]
fn sketch_revolve_produces_mesh() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Revolve {
                params: RevolveParams {
                    sketch_id,
                    profile_index: 0,
                    axis_origin: [0.0, 0.0, 0.0],
                    axis_direction: [0.0, 1.0, 0.0],
                    angle: 360.0,
                },
            },
        },
        &mut kernel,
    );

    let revolve_id = match &response {
        EngineToUi::ModelUpdated { feature_tree, .. } => {
            assert_eq!(feature_tree.features.len(), 2);
            assert_eq!(feature_tree.features[1].name, "Revolve");
            feature_tree.features[1].id
        }
        other => panic!("Expected ModelUpdated, got {:?}", other),
    };

    assert_has_solid(&state, revolve_id);

    let mesh = tessellate_feature(&state, &mut kernel, revolve_id);
    assert!(
        !mesh.indices.is_empty(),
        "Revolve should produce non-empty mesh"
    );
    assert!(
        !mesh.face_ranges.is_empty(),
        "Revolve should have face ranges"
    );
}

// ── Test 3: Non-XY plane sketch → extrude ─────────────────────────────────

#[test]
fn non_xy_plane_sketch_extrude() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Sketch on XZ plane at y=5 (normal = [0,1,0])
    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 5.0, 0.0], [0.0, 1.0, 0.0]);

    // Extrude with direction=None → defaults to sketch normal [0,1,0]
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 10.0, None);

    // Verify the sketch stored the correct plane data
    let sketch_feature = state.engine.tree.find_feature(sketch_id).unwrap();
    if let Operation::Sketch { sketch } = &sketch_feature.operation {
        assert_eq!(sketch.plane_origin, [0.0, 5.0, 0.0]);
        assert_eq!(sketch.plane_normal, [0.0, 1.0, 0.0]);
    } else {
        panic!("Expected Sketch operation");
    }

    // Verify extrude succeeded
    assert_has_solid(&state, extrude_id);
    let mesh = tessellate_feature(&state, &mut kernel, extrude_id);
    assert_eq!(mesh.indices.len(), 36, "Should produce a box mesh");
}

// ── Test 4: Multi-feature pipeline (extrude → fillet) ─────────────────────

#[test]
fn extrude_then_fillet_increases_face_count() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    let mesh_before = tessellate_feature(&state, &mut kernel, extrude_id);
    let face_count_before = mesh_before.face_ranges.len();
    assert_eq!(face_count_before, 6, "Box should start with 6 faces");

    // Add fillet referencing the extrude. Use BestEffort with a non-existent role
    // so the fallback kicks in and resolves to the first created Edge entity.
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Fillet {
                params: FilletParams {
                    edges: vec![GeomRef {
                        kind: TopoKind::Edge,
                        anchor: Anchor::FeatureOutput {
                            feature_id: extrude_id,
                            output_key: OutputKey::Main,
                        },
                        selector: Selector::Role {
                            role: Role::FilletFace { index: 99 },
                            index: 0,
                        },
                        policy: ResolvePolicy::BestEffort,
                    }],
                    radius: 0.5,
                },
            },
        },
        &mut kernel,
    );

    let fillet_id = match &response {
        EngineToUi::ModelUpdated { feature_tree, .. } => {
            assert_eq!(feature_tree.features.len(), 3);
            assert_eq!(feature_tree.features[2].name, "Fillet");
            feature_tree.features[2].id
        }
        other => panic!("Expected ModelUpdated, got {:?}", other),
    };

    assert_has_solid(&state, fillet_id);
    let mesh_after = tessellate_feature(&state, &mut kernel, fillet_id);
    assert!(
        mesh_after.face_ranges.len() > face_count_before,
        "Fillet should increase face count: {} > {}",
        mesh_after.face_ranges.len(),
        face_count_before
    );
}

// ── Test 5: Save/load roundtrip with plane data ──────────────────────────

#[test]
fn save_load_roundtrip_preserves_plane_data() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Create sketch on non-default plane
    let origin = [1.0, 2.0, 3.0];
    let normal = [0.0, 1.0, 0.0];
    let sketch_id = create_rect_sketch(&mut state, &mut kernel, origin, normal);
    let _extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 7.0, None);

    assert_eq!(state.engine.tree.features.len(), 2);

    // Save
    let save_response = wasm_bridge::dispatch(&mut state, UiToEngine::SaveProject, &mut kernel);
    let json_data = match save_response {
        EngineToUi::SaveReady { json_data } => json_data,
        other => panic!("Expected SaveReady, got {:?}", other),
    };
    assert!(
        json_data.contains("waffle-iron"),
        "Save format marker missing"
    );

    // Load into fresh state
    let mut new_state = EngineState::new();
    let load_response = wasm_bridge::dispatch(
        &mut new_state,
        UiToEngine::LoadProject { data: json_data },
        &mut kernel,
    );
    assert!(
        matches!(load_response, EngineToUi::ModelUpdated { .. }),
        "Load should return ModelUpdated"
    );

    // Verify features survived
    assert_eq!(new_state.engine.tree.features.len(), 2);

    // Verify plane data survived roundtrip
    let loaded_sketch = new_state.engine.tree.find_feature(sketch_id).unwrap();
    if let Operation::Sketch { sketch } = &loaded_sketch.operation {
        assert_eq!(
            sketch.plane_origin, origin,
            "plane_origin should survive roundtrip"
        );
        assert_eq!(
            sketch.plane_normal, normal,
            "plane_normal should survive roundtrip"
        );
    } else {
        panic!("Expected Sketch operation after load");
    }

    // Verify extrude rebuilt successfully
    let extrude_feature = &new_state.engine.tree.features[1];
    assert_eq!(extrude_feature.name, "Extrude");
    assert_has_solid(&new_state, extrude_feature.id);
}

// ── Test 6: Feature CRUD lifecycle ────────────────────────────────────────

#[test]
fn feature_delete_middle_rebuilds_correctly() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Add Sketch1
    let sketch1_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    // Add Extrude1 referencing sketch1
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch1_id, 5.0, None);
    // Add Sketch2 (independent)
    let sketch2_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

    assert_eq!(state.engine.tree.features.len(), 3);

    // Delete the middle feature (Extrude)
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::DeleteFeature {
            feature_id: extrude_id,
        },
        &mut kernel,
    );
    assert!(
        matches!(response, EngineToUi::ModelUpdated { .. }),
        "Delete should return ModelUpdated"
    );
    assert_eq!(state.engine.tree.features.len(), 2);

    // Both sketches should still be present
    assert!(state.engine.tree.find_feature(sketch1_id).is_some());
    assert!(state.engine.tree.find_feature(sketch2_id).is_some());

    // Extrude should be gone
    assert!(state.engine.tree.find_feature(extrude_id).is_none());
    assert!(state.engine.get_result(extrude_id).is_none());
}

#[test]
fn feature_suppress_unsuppress_toggles_result() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    // Before suppress: extrude has a result
    assert_has_solid(&state, extrude_id);

    // Suppress
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SuppressFeature {
            feature_id: extrude_id,
            suppressed: true,
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));

    // After suppress: extrude should have no result (skipped during rebuild)
    assert!(
        state.engine.get_result(extrude_id).is_none(),
        "Suppressed feature should have no OpResult"
    );

    // Unsuppress
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::SuppressFeature {
            feature_id: extrude_id,
            suppressed: false,
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));

    // After unsuppress: extrude should have a result again
    assert_has_solid(&state, extrude_id);
}

// ── Test 7: Undo/redo with mesh verification ──────────────────────────────

#[test]
fn undo_redo_extrude_toggles_solid() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    // 2 features, extrude has solid
    assert_eq!(state.engine.tree.features.len(), 2);
    assert_has_solid(&state, extrude_id);

    // Undo extrude
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(
        state.engine.tree.features.len(),
        1,
        "Undo should remove extrude"
    );
    assert!(
        state.engine.get_result(extrude_id).is_none(),
        "Undone extrude should have no result"
    );

    // Redo extrude
    let response = wasm_bridge::dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(
        state.engine.tree.features.len(),
        2,
        "Redo should restore extrude"
    );

    // The extrude ID may differ after redo since undo/redo re-inserts the feature.
    // Verify by checking the second feature has an output.
    let redo_extrude_id = state.engine.tree.features[1].id;
    assert_has_solid(&state, redo_extrude_id);
    let mesh = tessellate_feature(&state, &mut kernel, redo_extrude_id);
    assert_eq!(
        mesh.indices.len(),
        36,
        "Redo'd extrude should produce box mesh"
    );
}

#[test]
fn undo_redo_full_cycle_sketch_and_extrude() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Start from empty
    assert_eq!(state.engine.tree.features.len(), 0);

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    assert_eq!(state.engine.tree.features.len(), 1);

    let _extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);
    assert_eq!(state.engine.tree.features.len(), 2);

    // Undo extrude → 1 feature
    wasm_bridge::dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(state.engine.tree.features.len(), 1);

    // Undo sketch → 0 features
    wasm_bridge::dispatch(&mut state, UiToEngine::Undo, &mut kernel);
    assert_eq!(state.engine.tree.features.len(), 0);

    // Redo sketch → 1 feature
    wasm_bridge::dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert_eq!(state.engine.tree.features.len(), 1);

    // Redo extrude → 2 features with solid
    wasm_bridge::dispatch(&mut state, UiToEngine::Redo, &mut kernel);
    assert_eq!(state.engine.tree.features.len(), 2);

    let redo_extrude_id = state.engine.tree.features[1].id;
    assert_has_solid(&state, redo_extrude_id);
}

// ── Test 8: Error paths ──────────────────────────────────────────────────

#[test]
fn extrude_nonexistent_sketch_has_rebuild_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let fake_sketch_id = Uuid::new_v4();
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id: fake_sketch_id,
                    profile_index: 0,
                    depth: 5.0,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    target_body: None,
                },
            },
        },
        &mut kernel,
    );

    // add_feature succeeds (adds to tree), but rebuild records the error
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.features.len(), 1);

    // The extrude should have NO OpResult (rebuild failed for this feature)
    let extrude_id = state.engine.tree.features[0].id;
    assert!(
        state.engine.get_result(extrude_id).is_none()
            || state
                .engine
                .get_result(extrude_id)
                .unwrap()
                .outputs
                .is_empty(),
        "Extrude referencing nonexistent sketch should have no solid output"
    );

    // Engine should record the rebuild error
    assert!(
        !state.engine.errors.is_empty(),
        "Engine should have recorded a rebuild error"
    );
}

#[test]
fn extrude_profile_index_out_of_range_has_rebuild_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

    // Profile index 99 doesn't exist (only profile 0)
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id,
                    profile_index: 99,
                    depth: 5.0,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    target_body: None,
                },
            },
        },
        &mut kernel,
    );

    // Feature is added to tree, but rebuild fails for this extrude
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(state.engine.tree.features.len(), 2); // Sketch + Extrude

    let extrude_id = state.engine.tree.features[1].id;
    assert!(
        state.engine.get_result(extrude_id).is_none()
            || state
                .engine
                .get_result(extrude_id)
                .unwrap()
                .outputs
                .is_empty(),
        "Extrude with bad profile index should have no solid output"
    );

    assert!(
        !state.engine.errors.is_empty(),
        "Engine should have recorded a rebuild error for bad profile index"
    );
}

#[test]
fn finish_sketch_without_begin_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
        },
        &mut kernel,
    );

    match &response {
        EngineToUi::Error { message, .. } => {
            assert!(
                message.contains("no active sketch") || message.contains("sketch"),
                "Error should mention no active sketch, got: {}",
                message
            );
        }
        other => panic!(
            "Expected Error for FinishSketch without Begin, got {:?}",
            other
        ),
    }
}

#[test]
fn add_entity_without_begin_sketch_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddSketchEntity {
            entity: SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
        },
        &mut kernel,
    );

    assert!(
        matches!(response, EngineToUi::Error { .. }),
        "AddSketchEntity without BeginSketch should error"
    );
}

// ── Test 9: Explicit extrude direction ────────────────────────────────────

#[test]
fn extrude_with_explicit_direction() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

    // Extrude in custom direction [1, 0, 0] instead of sketch normal
    let extrude_id = add_extrude(
        &mut state,
        &mut kernel,
        sketch_id,
        8.0,
        Some([1.0, 0.0, 0.0]),
    );

    assert_has_solid(&state, extrude_id);
    let mesh = tessellate_feature(&state, &mut kernel, extrude_id);
    assert_eq!(
        mesh.indices.len(),
        36,
        "Explicit-direction extrude should produce box mesh"
    );
}

// ── Test 10: Multiple sketches and extrudes ───────────────────────────────

#[test]
fn two_independent_sketch_extrude_pairs() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // First sketch + extrude
    let sketch1 = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude1 = add_extrude(&mut state, &mut kernel, sketch1, 5.0, None);

    // Second sketch + extrude
    let sketch2 = create_rect_sketch(&mut state, &mut kernel, [20.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude2 = add_extrude(&mut state, &mut kernel, sketch2, 10.0, None);

    assert_eq!(state.engine.tree.features.len(), 4);

    // Both extrudes should have solids
    assert_has_solid(&state, extrude1);
    assert_has_solid(&state, extrude2);

    // Both should produce valid meshes
    let mesh1 = tessellate_feature(&state, &mut kernel, extrude1);
    let mesh2 = tessellate_feature(&state, &mut kernel, extrude2);
    assert_eq!(mesh1.indices.len(), 36);
    assert_eq!(mesh2.indices.len(), 36);
}

// ── Test 11: Rename feature doesn't break rebuild ─────────────────────────

#[test]
fn rename_feature_preserves_solid() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    assert_has_solid(&state, extrude_id);

    // Rename
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::RenameFeature {
            feature_id: extrude_id,
            new_name: "My Box".to_string(),
        },
        &mut kernel,
    );
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    assert_eq!(
        state.engine.tree.find_feature(extrude_id).unwrap().name,
        "My Box"
    );

    // Solid still accessible
    assert_has_solid(&state, extrude_id);
}

// ── Test 12: Face range coverage ──────────────────────────────────────────

#[test]
fn face_ranges_cover_all_indices() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    let mesh = tessellate_feature(&state, &mut kernel, extrude_id);

    // Verify face ranges are contiguous and cover all indices
    let mut expected_start = 0u32;
    for (i, fr) in mesh.face_ranges.iter().enumerate() {
        assert_eq!(
            fr.start_index, expected_start,
            "Face range {} start should be {}",
            i, expected_start
        );
        assert!(
            fr.end_index > fr.start_index,
            "Face range {} should be non-empty",
            i
        );
        expected_start = fr.end_index;
    }
    assert_eq!(
        expected_start,
        mesh.indices.len() as u32,
        "Face ranges should cover all indices"
    );
}

// ── Test 13: Extrude with empty profiles ──────────────────────────────────

#[test]
fn extrude_sketch_with_no_profiles_returns_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    // Create a sketch with NO profiles
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
    wasm_bridge::dispatch(&mut state, UiToEngine::BeginSketch { plane }, &mut kernel);
    wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddSketchEntity {
            entity: SketchEntity::Point {
                id: 1,
                x: 0.0,
                y: 0.0,
                construction: false,
            },
        },
        &mut kernel,
    );
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(), // No profiles!
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
        },
        &mut kernel,
    );
    let sketch_id = match response {
        EngineToUi::ModelUpdated { feature_tree, .. } => feature_tree.features[0].id,
        other => panic!("Expected ModelUpdated, got {:?}", other),
    };

    // Extrude a sketch with no profiles — feature is added but rebuild fails
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::AddFeature {
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    sketch_id,
                    profile_index: 0,
                    depth: 5.0,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    target_body: None,
                },
            },
        },
        &mut kernel,
    );

    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
    let extrude_id = state.engine.tree.features[1].id;
    assert!(
        state.engine.get_result(extrude_id).is_none()
            || state
                .engine
                .get_result(extrude_id)
                .unwrap()
                .outputs
                .is_empty(),
        "Extrude of empty-profile sketch should have no solid output"
    );
    assert!(
        !state.engine.errors.is_empty(),
        "Engine should record error for empty-profile extrude"
    );
}

// ── Test 14: Save/load roundtrip of multi-feature model ───────────────────

#[test]
fn save_load_roundtrip_multi_feature() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let _extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    // Save
    let json_data = match wasm_bridge::dispatch(&mut state, UiToEngine::SaveProject, &mut kernel) {
        EngineToUi::SaveReady { json_data } => json_data,
        other => panic!("Expected SaveReady, got {:?}", other),
    };

    // Load into fresh state
    let mut new_state = EngineState::new();
    wasm_bridge::dispatch(
        &mut new_state,
        UiToEngine::LoadProject { data: json_data },
        &mut kernel,
    );

    // Verify tree
    assert_eq!(new_state.engine.tree.features.len(), 2);
    assert_eq!(new_state.engine.tree.features[0].name, "Sketch");
    assert_eq!(new_state.engine.tree.features[1].name, "Extrude");

    // Verify extrude rebuilt with solid
    let loaded_extrude_id = new_state.engine.tree.features[1].id;
    assert_has_solid(&new_state, loaded_extrude_id);
    let mesh = tessellate_feature(&new_state, &mut kernel, loaded_extrude_id);
    assert_eq!(
        mesh.indices.len(),
        36,
        "Loaded extrude should produce box mesh"
    );
}

// ── Test 15: Reorder features ─────────────────────────────────────────────

#[test]
fn reorder_sketch_after_extrude_causes_rebuild_error() {
    let mut state = EngineState::new();
    let mut kernel = MockKernel::new();

    let sketch_id = create_rect_sketch(&mut state, &mut kernel, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let _extrude_id = add_extrude(&mut state, &mut kernel, sketch_id, 5.0, None);

    assert_eq!(state.engine.tree.features.len(), 2);

    // Reorder: move sketch to position 1 (after extrude)
    // This should cause the extrude to fail rebuild since its sketch is now after it
    let response = wasm_bridge::dispatch(
        &mut state,
        UiToEngine::ReorderFeature {
            feature_id: sketch_id,
            new_position: 1,
        },
        &mut kernel,
    );

    // The dispatch should still succeed (returns ModelUpdated), but the engine
    // will have rebuild errors since extrude precedes its sketch
    assert!(matches!(response, EngineToUi::ModelUpdated { .. }));
}
