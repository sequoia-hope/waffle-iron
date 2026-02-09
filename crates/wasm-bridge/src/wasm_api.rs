//! WASM entry points for the web worker.
//!
//! This module is only compiled for the `wasm32` target. It provides the
//! `#[wasm_bindgen]` functions that JavaScript calls from the web worker.

use wasm_bindgen::prelude::*;

use crate::dispatch;
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use kernel_fork::RenderMesh;

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

        dispatch::dispatch(&mut engine.state, msg, &mut engine.kernel)
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
