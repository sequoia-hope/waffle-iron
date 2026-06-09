//! Per-feature tessellation runner. Extracted from `wasm_api.rs` so it
//! can be exercised by native integration tests (the `wasm_api` module is
//! `#[cfg(target_arch = "wasm32")]`-gated and therefore not callable from
//! `cargo test` on native targets).
//!
//! This module is the post-dispatch tessellation stage:
//!   `process_message` (wasm32) → `dispatch::dispatch` (native+wasm) →
//!   here (native+wasm).

use crate::engine_state::EngineState;
use modeling_ops::KernelBundle;

/// Tessellate feature results that have a solid handle but no mesh data.
/// Skips features consumed by a later boolean (they won't be rendered).
/// Also extracts edge polylines for edge overlay rendering.
///
/// Each tessellation/edge-extraction call is wrapped in `catch_unwind` to
/// prevent panics in tessellation code from crashing the WASM module.
pub fn tessellate_missing_meshes(state: &mut EngineState, kernel: &mut dyn KernelBundle) {
    let consumed = state.engine.consumed_features.clone();
    let feature_ids: Vec<uuid::Uuid> = state.engine.tree.features.iter().map(|f| f.id).collect();

    for fid in feature_ids {
        if consumed.contains(&fid) {
            continue;
        }

        let needs_work = state
            .engine
            .feature_results
            .get(&fid)
            .map(|r| {
                r.outputs
                    .iter()
                    .any(|(_, body)| body.mesh.is_none() || body.edges.is_none())
            })
            .unwrap_or(false);

        if !needs_work {
            continue;
        }

        if let Some(result) = state.engine.feature_results.get_mut(&fid) {
            for (_key, body) in &mut result.outputs {
                if body.mesh.is_none() {
                    let handle = body.handle.clone();
                    let mesh_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            kernel.tessellate(&handle, 0.0001)
                        }));
                    match mesh_result {
                        Ok(Ok(mesh)) => body.mesh = Some(mesh),
                        Ok(Err(_)) => {} // tessellation error, skip
                        Err(_) => {}     // panic caught, skip
                    }
                }
                if body.edges.is_none() {
                    let handle = body.handle.clone();
                    let edge_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            kernel.extract_edges(&handle, 0.0001)
                        }));
                    match edge_result {
                        Ok(Ok(edges)) => body.edges = Some(edges),
                        Ok(Err(_)) => {} // edge extraction error, skip
                        Err(_) => {}     // panic caught, skip
                    }
                }
            }
        }
    }
}
