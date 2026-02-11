//! WASM entry points for the web worker.
//!
//! This module is only compiled for the `wasm32` target. It provides the
//! `#[wasm_bindgen]` functions that JavaScript calls from the web worker.

use wasm_bindgen::prelude::*;

use crate::dispatch;
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use kernel_fork::RenderMesh;
use modeling_ops::KernelBundle;
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector, TopoKind, TopoSignature};

// Global engine state — single-threaded in the web worker.
thread_local! {
    static ENGINE_STATE: std::cell::RefCell<Option<WasmEngine>> = std::cell::RefCell::new(None);
}

/// Holds the engine state and kernel for the WASM module.
struct WasmEngine {
    state: EngineState,
    kernel: kernel_fork::TruckKernel,
}

/// Initialize the WASM engine. Must be called once before any other function.
///
/// Sets up panic hooks for better error messages and creates the engine state.
#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();

    ENGINE_STATE.with(|cell| {
        *cell.borrow_mut() = Some(WasmEngine {
            state: EngineState::new(),
            kernel: kernel_fork::TruckKernel::new(),
        });
    });
}

/// Process a JSON message from the UI and return a JSON response.
///
/// This is the main entry point for the web worker's message handler.
/// The input should be a JSON-serialized `UiToEngine` message.
/// Returns a JSON-serialized `EngineToUi` response.
#[wasm_bindgen]
pub fn process_message(json_input: &str) -> String {
    let response = ENGINE_STATE.with(|cell| {
        let mut engine = cell.borrow_mut();
        let engine = engine
            .as_mut()
            .expect("Engine not initialized. Call init() first.");

        let msg: UiToEngine = match serde_json::from_str(json_input) {
            Ok(msg) => msg,
            Err(e) => {
                return EngineToUi::Error {
                    message: format!("Failed to parse message: {}", e),
                    feature_id: None,
                };
            }
        };

        let response = dispatch::dispatch(&mut engine.state, msg, &mut engine.kernel);

        // After dispatch, tessellate any solids that don't have mesh data yet
        if matches!(response, EngineToUi::ModelUpdated { .. }) {
            tessellate_missing_meshes(&mut engine.state, &mut engine.kernel);
        }

        response
    });

    serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(
            r#"{{"type":"Error","message":"Serialization failed: {}","feature_id":null}}"#,
            e
        )
    })
}

/// Get the current feature tree as JSON.
///
/// Useful for the UI to query state without sending a full command.
#[wasm_bindgen]
pub fn get_feature_tree() -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref().expect("Engine not initialized.");
        serde_json::to_string(&engine.state.engine.tree).unwrap_or_default()
    })
}

/// Get mesh data for a specific feature by index.
///
/// Returns a JSON object with vertices, normals, and indices arrays.
/// For high-performance rendering, the web worker should use the
/// `get_mesh_vertices`, `get_mesh_normals`, and `get_mesh_indices`
/// functions instead, which return typed arrays directly.
#[wasm_bindgen]
pub fn get_mesh_json(feature_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref().expect("Engine not initialized.");

        let results = &engine.state.engine.feature_results;
        let features = &engine.state.engine.tree.features;

        if feature_index >= features.len() {
            return r#"{"error":"Feature index out of range"}"#.to_string();
        }

        let feature_id = features[feature_index].id;
        if let Some(result) = results.get(&feature_id) {
            // Return the first output's mesh
            for (_key, body) in &result.outputs {
                if let Some(ref mesh) = body.mesh {
                    return serde_json::to_string(mesh).unwrap_or_default();
                }
            }
        }

        r#"{"error":"No mesh for this feature"}"#.to_string()
    })
}

/// Get mesh vertex positions as a Float32Array view into WASM memory.
///
/// Returns the vertices of the latest (last) feature's mesh as a zero-copy
/// typed array view. The array contains [x0, y0, z0, x1, y1, z1, ...].
///
/// IMPORTANT: The returned view is invalidated by any WASM memory growth.
/// Copy or transfer the data immediately after calling this function.
#[wasm_bindgen]
pub fn get_mesh_vertices(feature_index: usize) -> js_sys::Float32Array {
    with_mesh(feature_index, |mesh| unsafe {
        js_sys::Float32Array::view(&mesh.vertices)
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get mesh vertex normals as a Float32Array view into WASM memory.
///
/// Returns [nx0, ny0, nz0, nx1, ny1, nz1, ...].
#[wasm_bindgen]
pub fn get_mesh_normals(feature_index: usize) -> js_sys::Float32Array {
    with_mesh(feature_index, |mesh| unsafe {
        js_sys::Float32Array::view(&mesh.normals)
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get mesh triangle indices as a Uint32Array view into WASM memory.
///
/// Returns [i0, i1, i2, i3, i4, i5, ...] where each triple is a triangle.
#[wasm_bindgen]
pub fn get_mesh_indices(feature_index: usize) -> js_sys::Uint32Array {
    with_mesh(feature_index, |mesh| unsafe {
        js_sys::Uint32Array::view(&mesh.indices)
    })
    .unwrap_or_else(|| js_sys::Uint32Array::new_with_length(0))
}

/// Get the number of features with mesh data.
#[wasm_bindgen]
pub fn get_mesh_count() -> usize {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return 0,
        };

        let mut count = 0;
        for feature in &engine.state.engine.tree.features {
            if let Some(result) = engine.state.engine.feature_results.get(&feature.id) {
                if result.outputs.iter().any(|(_, body)| body.mesh.is_some()) {
                    count += 1;
                }
            }
        }
        count
    })
}

/// Get face data for a specific feature by index.
///
/// Returns a JSON array of face ranges enriched with GeomRef data.
/// Each entry contains a `geom_ref` (persistent geometry reference) plus
/// `start_index` and `end_index` into the mesh indices array.
///
/// For faces with role assignments from provenance, a Role-based selector is used.
/// For faces without roles, a Signature-based selector with a centroid fallback is used.
#[wasm_bindgen]
pub fn get_face_data(feature_index: usize) -> String {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return "[]".to_string(),
        };

        let features = &engine.state.engine.tree.features;
        let feature = match features.get(feature_index) {
            Some(f) => f,
            None => return "[]".to_string(),
        };

        let feature_id = feature.id;
        let result = match engine.state.engine.feature_results.get(&feature_id) {
            Some(r) => r,
            None => return "[]".to_string(),
        };

        // Find the first output with a mesh
        let mut found_mesh = None;
        let mut found_key = None;
        for (key, body) in &result.outputs {
            if let Some(ref mesh) = body.mesh {
                found_mesh = Some(mesh);
                found_key = Some(key.clone());
                break;
            }
        }

        let mesh = match found_mesh {
            Some(m) => m,
            None => return "[]".to_string(),
        };
        let output_key = found_key.unwrap();

        // Build a lookup from KernelId → Role from provenance
        let role_map: std::collections::HashMap<_, _> =
            result.provenance.role_assignments.iter().cloned().collect();

        // Build face data entries
        let mut entries = Vec::new();
        for (face_idx, range) in mesh.face_ranges.iter().enumerate() {
            let geom_ref = if let Some(role) = role_map.get(&range.face_id) {
                // Role-based selector — stable across rebuilds
                GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::FeatureOutput {
                        feature_id,
                        output_key: output_key.clone(),
                    },
                    selector: Selector::Role {
                        role: role.clone(),
                        index: 0,
                    },
                    policy: ResolvePolicy::BestEffort,
                }
            } else {
                // Signature-based fallback using face index
                GeomRef {
                    kind: TopoKind::Face,
                    anchor: Anchor::FeatureOutput {
                        feature_id,
                        output_key: output_key.clone(),
                    },
                    selector: Selector::Signature {
                        signature: TopoSignature {
                            surface_type: None,
                            area: None,
                            centroid: None,
                            normal: None,
                            bbox: None,
                            adjacency_hash: Some(face_idx as u64),
                            length: None,
                        },
                    },
                    policy: ResolvePolicy::BestEffort,
                }
            };

            entries.push(serde_json::json!({
                "geom_ref": geom_ref,
                "start_index": range.start_index,
                "end_index": range.end_index,
            }));
        }

        serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
    })
}

/// Helper: access the mesh for a feature and apply a function to it.
fn with_mesh<T>(feature_index: usize, f: impl FnOnce(&RenderMesh) -> T) -> Option<T> {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref()?;

        let features = &engine.state.engine.tree.features;
        let feature = features.get(feature_index)?;
        let result = engine.state.engine.feature_results.get(&feature.id)?;

        for (_key, body) in &result.outputs {
            if let Some(ref mesh) = body.mesh {
                return Some(f(mesh));
            }
        }
        None
    })
}

/// Tessellate all feature results that have a solid handle but no mesh data.
fn tessellate_missing_meshes(state: &mut EngineState, kernel: &mut impl KernelBundle) {
    let feature_ids: Vec<uuid::Uuid> = state.engine.tree.features.iter().map(|f| f.id).collect();

    for fid in feature_ids {
        let needs_tessellation = state
            .engine
            .feature_results
            .get(&fid)
            .map(|r| r.outputs.iter().any(|(_, body)| body.mesh.is_none()))
            .unwrap_or(false);

        if !needs_tessellation {
            continue;
        }

        if let Some(result) = state.engine.feature_results.get_mut(&fid) {
            for (_key, body) in &mut result.outputs {
                if body.mesh.is_none() {
                    match kernel.tessellate(&body.handle, 0.1) {
                        Ok(mesh) => {
                            body.mesh = Some(mesh);
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }
}
