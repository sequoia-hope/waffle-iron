//! Per-feature tessellation runner. Extracted from `wasm_api.rs` so it
//! can be exercised by native integration tests (the `wasm_api` module is
//! `#[cfg(target_arch = "wasm32")]`-gated and therefore not callable from
//! `cargo test` on native targets).
//!
//! This module is the post-dispatch tessellation stage:
//!   `process_message` (wasm32) → `dispatch::dispatch` (native+wasm) →
//!   here (native+wasm).
//!
//! PR-VIZ-3a-fix: each per-feature `kernel.tessellate(...)` call is
//! wrapped with `start_yang_debug_capture` / `drain_yang_debug_capture`
//! so that the F.0–F.4 stage probes — which fire during tessellation,
//! AFTER the dispatch wrap has already drained — land in the existing
//! `state.yang_debug_captures` map. Without this, F.* stages would be
//! lost in the WASM browser environment because the dispatch-level
//! capture window closes before tessellation begins.

use crate::engine_state::EngineState;
use modeling_ops::KernelBundle;

/// Tessellate feature results that have a solid handle but no mesh data.
/// Skips features consumed by a later boolean (they won't be rendered).
/// Also extracts edge polylines for edge overlay rendering.
///
/// Each tessellation/edge-extraction call is wrapped in `catch_unwind` to
/// prevent panics in tessellation code from crashing the WASM module.
///
/// PR-VIZ-3a-fix: when `state.yang_debug_capture_enabled` is true, each
/// per-feature tessellation is bracketed with start/drain capture so the
/// F.0–F.4 stages are appended to the existing capture entry from the
/// dispatch wrap (or create one if dispatch did not).
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

        // Snapshot error-bit BEFORE the mutable borrow so failed_at_stage
        // can be assigned without an additional borrow conflict.
        let feature_errored = state.engine.errors.iter().any(|(id, _)| *id == fid);
        let capture_enabled = state.yang_debug_capture_enabled;

        if capture_enabled {
            kernel::start_yang_debug_capture();
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

        if capture_enabled {
            let f_stages = kernel::drain_yang_debug_capture();
            if !f_stages.is_empty() {
                use std::collections::hash_map::Entry;
                match state.yang_debug_captures.entry(fid.to_string()) {
                    Entry::Occupied(mut e) => {
                        e.get_mut().stages.extend(f_stages);
                    }
                    Entry::Vacant(e) => {
                        let failed_at = if feature_errored {
                            Some(f_stages.len().saturating_sub(1))
                        } else {
                            None
                        };
                        e.insert(kernel::FeatureStageCapture {
                            feature_id: fid.to_string(),
                            stages: f_stages,
                            failed_at_stage: failed_at,
                        });
                    }
                }
            }
        }
    }
}
