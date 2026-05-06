//! WASM entry points for the web worker.
//!
//! This module is only compiled for the `wasm32` target. It provides the
//! `#[wasm_bindgen]` functions that JavaScript calls from the web worker.

use wasm_bindgen::prelude::*;

use crate::dispatch;
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};
use kernel::{EdgeRenderData, RenderMesh};
use modeling_ops::KernelBundle;
use waffle_types::{Anchor, GeomRef, ResolvePolicy, Selector, TopoKind, TopoSignature};

// Global engine state — single-threaded in the web worker.
thread_local! {
    static ENGINE_STATE: std::cell::RefCell<Option<WasmEngine>> = std::cell::RefCell::new(None);
}

/// Holds the engine state and kernel for the WASM module.
struct WasmEngine {
    state: EngineState,
    kernel: kernel::WaffleKernel,
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
            kernel: kernel::WaffleKernel::new(),
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
    // Wrap the entire dispatch + tessellation in catch_unwind so that
    // panics deep in truck internals produce an error response instead of
    // killing the WASM module with an `unreachable` trap.
    // Requires `panic = "unwind"` in [profile.release] (see workspace Cargo.toml).
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ENGINE_STATE.with(|cell| {
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

            let msg_type = format!("{:?}", std::mem::discriminant(&msg));
            let t0 = js_sys::Date::now();
            let response = dispatch::dispatch(&mut engine.state, msg, &mut engine.kernel);
            let dispatch_ms = js_sys::Date::now() - t0;

            // After dispatch, tessellate any solids that don't have mesh data yet
            if matches!(response, EngineToUi::ModelUpdated { .. }) {
                let t1 = js_sys::Date::now();
                tessellate_missing_meshes(&mut engine.state, &mut engine.kernel);
                let tess_ms = js_sys::Date::now() - t1;
                if dispatch_ms + tess_ms > 100.0 {
                    web_sys::console::log_1(
                        &format!(
                            "[wasm] {} dispatch={:.1}s tess={:.1}s total={:.1}s",
                            msg_type,
                            dispatch_ms / 1000.0,
                            tess_ms / 1000.0,
                            (dispatch_ms + tess_ms) / 1000.0,
                        )
                        .into(),
                    );
                }
            }

            response
        })
    }));

    let response = match response {
        Ok(r) => r,
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown internal error".to_string()
            };
            EngineToUi::Error {
                message: format!("Internal error: {}", msg),
                feature_id: None,
            }
        }
    };

    serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(
            r#"{{"type":"Error","message":"Serialization failed: {}","feature_id":null}}"#,
            e
        )
    })
}

// ── PR-VIZ-3a: in-memory Yang stage capture WASM exports ─────────────────
//
// Spec: specs/yang_pr_viz_3a_in_memory_capture.md §5
// Thin `#[wasm_bindgen]` wrappers around `crate::yang_debug::*`. The
// non-wasm shim in `yang_debug.rs` keeps its own thread-local state for
// native tests; on wasm32 these accessors route into `ENGINE_STATE`.

/// Run a closure with an immutable reference to the wasm-worker engine
/// state. PR-VIZ-3a — used by `crate::yang_debug` to route the native
/// shim API through `ENGINE_STATE` when compiled to wasm32.
pub(crate) fn with_engine_state<R>(f: impl FnOnce(&crate::engine_state::EngineState) -> R) -> R {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine
            .as_ref()
            .expect("Engine not initialized. Call init() first.");
        f(&engine.state)
    })
}

/// Mutable counterpart to `with_engine_state`.
pub(crate) fn with_engine_state_mut<R>(
    f: impl FnOnce(&mut crate::engine_state::EngineState) -> R,
) -> R {
    ENGINE_STATE.with(|cell| {
        let mut engine = cell.borrow_mut();
        let engine = engine
            .as_mut()
            .expect("Engine not initialized. Call init() first.");
        f(&mut engine.state)
    })
}

/// PR-VIZ-3a: arm or disarm in-memory Yang stage capture. Spec §5.
#[wasm_bindgen]
pub fn set_yang_debug_capture(enabled: bool) {
    crate::yang_debug::set_yang_debug_capture(enabled);
}

/// PR-VIZ-3a: get captured stages for a feature_id as JSON. Returns
/// `"null"` when absent. Spec §5.
#[wasm_bindgen]
pub fn get_yang_stages_json(feature_id: &str) -> String {
    crate::yang_debug::get_yang_stages_json(feature_id)
}

/// PR-VIZ-3a: clear the captures map (free memory). Spec §5.
#[wasm_bindgen]
pub fn clear_yang_debug_captures() {
    crate::yang_debug::clear_yang_debug_captures();
}

/// Get the current feature tree as JSON.
///
/// Useful for the UI to query state without sending a full command.
/// Wrapped in catch_unwind to prevent panics from crashing the WASM module
/// if engine state is corrupted after a failed boolean cascade.
#[wasm_bindgen]
pub fn get_feature_tree() -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ENGINE_STATE.with(|cell| {
            let engine = cell.borrow();
            let engine = match engine.as_ref() {
                Some(e) => e,
                None => return r#"{"features":[],"active_index":null}"#.to_string(),
            };
            serde_json::to_string(&engine.state.engine.tree).unwrap_or_default()
        })
    }))
    .unwrap_or_else(|_| r#"{"features":[],"active_index":null}"#.to_string())
}

/// Get mesh data for a specific feature by index.
///
/// Returns a JSON object with vertices, normals, and indices arrays.
/// For high-performance rendering, the web worker should use the
/// `get_mesh_vertices`, `get_mesh_normals`, and `get_mesh_indices`
/// functions instead, which return typed arrays directly.
/// Wrapped in catch_unwind to prevent panics from crashing the WASM module.
#[wasm_bindgen]
pub fn get_mesh_json(feature_index: usize) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ENGINE_STATE.with(|cell| {
            let engine = cell.borrow();
            let engine = match engine.as_ref() {
                Some(e) => e,
                None => return r#"{"error":"Engine not initialized"}"#.to_string(),
            };

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
    }))
    .unwrap_or_else(|_| r#"{"error":"Internal error"}"#.to_string())
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

/// Get which feature indices should be rendered.
///
/// Returns indices of features that have mesh data and are NOT consumed
/// by a later boolean operation. When a boolean union succeeds, the target
/// feature is consumed (its geometry is merged into the result feature).
/// When union fails, both features are renderable (multi-body mode).
#[wasm_bindgen]
pub fn get_renderable_feature_indices() -> js_sys::Uint32Array {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return js_sys::Uint32Array::new_with_length(0),
        };

        let consumed = &engine.state.engine.consumed_features;
        let mut indices = Vec::new();

        for (i, feature) in engine.state.engine.tree.features.iter().enumerate() {
            if consumed.contains(&feature.id) {
                continue;
            }
            if let Some(result) = engine.state.engine.feature_results.get(&feature.id) {
                if result.outputs.iter().any(|(_, body)| body.mesh.is_some()) {
                    indices.push(i as u32);
                }
            }
        }

        let arr = js_sys::Uint32Array::new_with_length(indices.len() as u32);
        arr.copy_from(&indices);
        arr
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

/// Get edge vertex positions as a Float32Array view into WASM memory.
///
/// Returns the edge polyline vertices for a feature as a zero-copy typed array.
/// The array contains [x0, y0, z0, x1, y1, z1, ...] where consecutive pairs
/// of vertices form line segments for rendering with THREE.LineSegments.
#[wasm_bindgen]
pub fn get_edge_vertices(feature_index: usize) -> js_sys::Float32Array {
    with_edges(feature_index, |edges| unsafe {
        js_sys::Float32Array::view(&edges.vertices)
    })
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get edge range data for a specific feature by index.
///
/// Returns a JSON array of edge ranges enriched with GeomRef data.
/// Each entry contains a `geom_ref` (persistent geometry reference) plus
/// `start_index` and `end_index` into the edge vertices array (in vertex count,
/// not float count).
#[wasm_bindgen]
pub fn get_edge_data(feature_index: usize) -> String {
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

        // Find the first output with edge data
        let mut found_edges = None;
        let mut found_key = None;
        for (key, body) in &result.outputs {
            if let Some(ref edges) = body.edges {
                found_edges = Some(edges);
                found_key = Some(key.clone());
                break;
            }
        }

        let edges = match found_edges {
            Some(e) => e,
            None => return "[]".to_string(),
        };
        let output_key = found_key.unwrap();

        // Build edge data entries with GeomRef
        let mut entries = Vec::new();
        for (edge_idx, range) in edges.edge_ranges.iter().enumerate() {
            // Use Signature-based selector with edge index as adjacency_hash
            let geom_ref = GeomRef {
                kind: TopoKind::Edge,
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
                        adjacency_hash: Some(edge_idx as u64),
                        length: None,
                    },
                },
                policy: ResolvePolicy::BestEffort,
            };

            // EdgeOverlay.svelte expects start_index/end_index (vertex counts)
            entries.push(serde_json::json!({
                "geom_ref": geom_ref,
                "start_index": range.start_vertex,
                "end_index": range.end_vertex,
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

/// Helper: access the edge data for a feature and apply a function to it.
fn with_edges<T>(feature_index: usize, f: impl FnOnce(&EdgeRenderData) -> T) -> Option<T> {
    ENGINE_STATE.with(|cell| {
        let engine = cell.borrow();
        let engine = engine.as_ref()?;

        let features = &engine.state.engine.tree.features;
        let feature = features.get(feature_index)?;
        let result = engine.state.engine.feature_results.get(&feature.id)?;

        for (_key, body) in &result.outputs {
            if let Some(ref edges) = body.edges {
                return Some(f(edges));
            }
        }
        None
    })
}

/// Tessellate feature results that have a solid handle but no mesh data.
/// PR-VIZ-3a-fix: thin shim that delegates to
/// `crate::tessellation_runner::tessellate_missing_meshes` (extracted to a
/// non-wasm-gated module so it can be exercised by native integration tests).
fn tessellate_missing_meshes(state: &mut EngineState, kernel: &mut impl KernelBundle) {
    crate::tessellation_runner::tessellate_missing_meshes(state, kernel)
}
