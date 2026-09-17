//! The thumbnail saved with a Part tab is the engine's DECIMATED preview.
//!
//! `dispatch` builds `ModelUpdated` before the post-dispatch tessellation
//! meshes new bodies, so after a load or full rebuild its `preview_mesh` was
//! `None`. The page then stored the whole last render mesh as the thumbnail:
//! the agent session's "Bike frame" document carried a 180k-triangle, 10 MB
//! preview (of a 24 MB file whose 44 features take 97 KB), serialized into
//! every autosave. `attach_preview_mesh`, run after tessellation, fixes that.

use feature_engine::types::*;
use modeling_ops::KernelBundle;
use test_harness::helpers::polygon_profile;
use uuid::Uuid;
use waffle_types::*;
use wasm_bridge::dispatch::attach_preview_mesh;
use wasm_bridge::tessellation_runner::tessellate_missing_meshes;
use wasm_bridge::{dispatch, EngineState, EngineToUi, UiToEngine};

fn datum_plane() -> GeomRef {
    GeomRef {
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
    }
}

fn preview_triangles(response: &EngineToUi) -> Option<usize> {
    match response {
        EngineToUi::ModelUpdated { preview_mesh, .. } => {
            preview_mesh.as_ref().map(|p| p.indices.len() / 3)
        }
        other => panic!("expected ModelUpdated, got {other:?}"),
    }
}

#[test]
fn preview_is_decimated_from_the_mesh_tessellated_after_dispatch() {
    let mut state = EngineState::new();
    let mut kernel: Box<dyn KernelBundle> = Box::new(kernel_v2::adapter::KernelV2Adapter::new());
    let k = kernel.as_mut();

    // A 200-sided prism: walls and caps mesh to well over the 500-triangle cap
    // (a true cylinder meshes to only 280).
    let ngon: Vec<(f64, f64)> = (0..200)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 200.0;
            (0.05 * a.cos(), 0.05 * a.sin())
        })
        .collect();
    let (entities, solved_positions, solved_profiles) = polygon_profile(&ngon);
    dispatch(
        &mut state,
        UiToEngine::BeginSketch {
            plane: datum_plane(),
        },
        k,
    );
    for entity in entities {
        dispatch(&mut state, UiToEngine::AddSketchEntity { entity }, k);
    }
    dispatch(
        &mut state,
        UiToEngine::FinishSketch {
            provenance: None,
            solved_positions,
            solved_profiles,
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            entities: vec![],
            constraints: vec![],
            projected: vec![],
        },
        k,
    );
    let sketch_id = state.engine.tree.features.last().expect("sketch").id;

    let mut response = dispatch(
        &mut state,
        UiToEngine::AddFeature {
            provenance: None,
            operation: Operation::Extrude {
                params: ExtrudeParams {
                    combine: Some(CombineMode::NewBody),
                    targets: None,
                    sketch_id,
                    profile_index: 0,
                    profile_entity_ids: None,
                    depth: 0.2,
                    direction: None,
                    symmetric: false,
                    cut: false,
                    merge: false,
                    target_body: None,
                    depth_mode: DepthMode::Blind,
                    second_direction: None,
                    region: None,
                    regions: Vec::new(),
                    depth_expr: None,
                },
            },
        },
        k,
    );
    assert!(state.engine.errors.is_empty(), "{:?}", state.engine.errors);
    // The bug: nothing is meshed yet when dispatch builds the response.
    assert_eq!(preview_triangles(&response), None);

    tessellate_missing_meshes(&mut state, k);
    attach_preview_mesh(&mut state, &mut response);

    let extrude_id = state.engine.tree.features.last().expect("extrude").id;
    let render_triangles = state
        .engine
        .get_result(extrude_id)
        .and_then(|r| r.outputs.iter().find_map(|(_, b)| b.mesh.as_ref()))
        .map(|m| m.indices.len() / 3)
        .expect("the extrude is meshed");
    assert!(
        render_triangles > 500,
        "fixture must exceed the preview cap, got {render_triangles}"
    );
    let preview = preview_triangles(&response).expect("a preview once the body is meshed");
    assert!(
        preview > 0 && preview < render_triangles,
        "preview {preview} triangles must be decimated below the {render_triangles}-triangle render mesh"
    );
}
