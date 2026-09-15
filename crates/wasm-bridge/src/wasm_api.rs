//! WASM entry points for the web worker.
//!
//! This module is only compiled for the `wasm32` target. It provides the
//! `#[wasm_bindgen]` functions that JavaScript calls from the web worker.
//! It is a binding shim: the pipeline lives in `process` and the render data
//! in `render_view`, both target-independent, so a native host serves the
//! same data from the same code.

use std::borrow::Cow;

use wasm_bindgen::prelude::*;

use crate::engine_state::EngineState;
use crate::render_view;

// Global engine state — single-threaded in the web worker.
thread_local! {
    static ENGINE_STATE: std::cell::RefCell<Option<WasmEngine>> = std::cell::RefCell::new(None);
}

/// Holds the engine state and kernel for the WASM module.
///
/// Since the Phase 6 migration (2026-06-11) the kernel is kernel-v2 behind
/// its legacy-trait adapter. The whole stack is Result-based — no panic
/// machinery is needed (the legacy catch_unwind wrappers existed to survive
/// panics deep in the old kernel's internals).
struct WasmEngine {
    state: EngineState,
    kernel: kernel_v2::KernelV2Adapter,
}

/// Initialize the WASM engine. Must be called once before any other function.
#[wasm_bindgen]
pub fn init() {
    // Surface panic messages in the browser console: without a hook a
    // panic reaches JS as a bare `unreachable` trap with no context (the
    // 2026-07-22 deployed-app debugging cost). Production paths are
    // Result-based and must not panic (crate contract) — this is the
    // loud last-resort diagnostic when one slips through, not a recovery
    // mechanism.
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("WASM PANIC: {info}").into());
    }));
    ENGINE_STATE.with(|cell| {
        *cell.borrow_mut() = Some(WasmEngine {
            state: EngineState::new(),
            kernel: kernel_v2::KernelV2Adapter::new(),
        });
    });
}

/// Run `f` against the initialized engine; `None` before `init()`.
fn with_engine<T>(f: impl FnOnce(&WasmEngine) -> T) -> Option<T> {
    ENGINE_STATE.with(|cell| cell.borrow().as_ref().map(f))
}

/// A zero-copy view of borrowed data, or a copy of owned (ghost-baked) data.
///
/// IMPORTANT: a view is invalidated by any WASM memory growth. Copy or
/// transfer the data immediately after the call that returned it.
fn f32_array(data: Cow<'_, [f32]>) -> js_sys::Float32Array {
    match data {
        Cow::Borrowed(slice) => unsafe { js_sys::Float32Array::view(slice) },
        Cow::Owned(vec) => js_sys::Float32Array::from(vec.as_slice()),
    }
}

fn entries_json(entries: &[serde_json::Value]) -> String {
    serde_json::to_string(entries).unwrap_or_else(|_| "[]".to_string())
}

/// Process a JSON message from the UI and return a JSON response.
///
/// This is the main entry point for the web worker's message handler.
/// The input should be a JSON-serialized `UiToEngine` message.
/// Returns a JSON-serialized `EngineToUi` response.
#[wasm_bindgen]
pub fn process_message(json_input: &str) -> String {
    ENGINE_STATE.with(|cell| {
        let mut engine = cell.borrow_mut();
        let engine = engine
            .as_mut()
            .expect("Engine not initialized. Call init() first.");
        crate::process::process_message_json(
            &mut engine.state,
            &mut engine.kernel,
            json_input,
            &js_sys::Date::now,
            &|line| web_sys::console::log_1(&line.into()),
        )
    })
}

/// Get the current feature tree as JSON.
///
/// Useful for the UI to query state without sending a full command.
#[wasm_bindgen]
pub fn get_feature_tree() -> String {
    with_engine(|e| serde_json::to_string(&e.state.engine.tree).unwrap_or_default())
        .unwrap_or_else(|| r#"{"features":[],"active_index":null}"#.to_string())
}

/// Get mesh data for a specific feature by index.
///
/// Returns a JSON object with vertices, normals, and indices arrays.
/// For high-performance rendering, the web worker should use the
/// `get_mesh_vertices`, `get_mesh_normals`, and `get_mesh_indices`
/// functions instead, which return typed arrays directly.
#[wasm_bindgen]
pub fn get_mesh_json(feature_index: usize) -> String {
    with_engine(|e| render_view::feature_mesh_json(&e.state, feature_index))
        .unwrap_or_else(|| r#"{"error":"Engine not initialized"}"#.to_string())
}

/// Get mesh vertex positions as a Float32Array view into WASM memory.
///
/// Returns the vertices of the feature's first output mesh as a zero-copy
/// typed array view. The array contains [x0, y0, z0, x1, y1, z1, ...].
#[wasm_bindgen]
pub fn get_mesh_vertices(feature_index: usize) -> js_sys::Float32Array {
    with_engine(|e| {
        render_view::feature_mesh(&e.state, feature_index)
            .map(|mesh| f32_array(Cow::Borrowed(&mesh.vertices)))
    })
    .flatten()
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get mesh vertex normals as a Float32Array view into WASM memory.
///
/// Returns [nx0, ny0, nz0, nx1, ny1, nz1, ...].
#[wasm_bindgen]
pub fn get_mesh_normals(feature_index: usize) -> js_sys::Float32Array {
    with_engine(|e| {
        render_view::feature_mesh(&e.state, feature_index)
            .map(|mesh| f32_array(Cow::Borrowed(&mesh.normals)))
    })
    .flatten()
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get mesh triangle indices as a Uint32Array view into WASM memory.
///
/// Returns [i0, i1, i2, i3, i4, i5, ...] where each triple is a triangle.
#[wasm_bindgen]
pub fn get_mesh_indices(feature_index: usize) -> js_sys::Uint32Array {
    with_engine(|e| {
        render_view::feature_mesh(&e.state, feature_index)
            .map(|mesh| unsafe { js_sys::Uint32Array::view(&mesh.indices) })
    })
    .flatten()
    .unwrap_or_else(|| js_sys::Uint32Array::new_with_length(0))
}

/// Get the number of features with mesh data.
#[wasm_bindgen]
pub fn get_mesh_count() -> usize {
    with_engine(|e| render_view::mesh_count(&e.state)).unwrap_or(0)
}

/// Get which feature indices should be rendered (see
/// `render_view::renderable_feature_indices`).
#[wasm_bindgen]
pub fn get_renderable_feature_indices() -> js_sys::Uint32Array {
    let indices =
        with_engine(|e| render_view::renderable_feature_indices(&e.state)).unwrap_or_default();
    let arr = js_sys::Uint32Array::new_with_length(indices.len() as u32);
    arr.copy_from(&indices);
    arr
}

/// Get face data for a specific feature by index, as a JSON array (see
/// `render_view::feature_face_entries`).
#[wasm_bindgen]
pub fn get_face_data(feature_index: usize) -> String {
    with_engine(|e| {
        entries_json(&render_view::feature_face_entries(
            &e.state,
            &e.kernel,
            feature_index,
        ))
    })
    .unwrap_or_else(|| "[]".to_string())
}

/// Get edge vertex positions as a Float32Array view into WASM memory.
///
/// Returns the edge polyline vertices for a feature as a zero-copy typed array.
/// The array contains [x0, y0, z0, x1, y1, z1, ...] where consecutive pairs
/// of vertices form line segments for rendering with THREE.LineSegments.
#[wasm_bindgen]
pub fn get_edge_vertices(feature_index: usize) -> js_sys::Float32Array {
    with_engine(|e| {
        render_view::feature_edges(&e.state, feature_index)
            .map(|edges| f32_array(Cow::Borrowed(&edges.vertices)))
    })
    .flatten()
    .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Get edge range data for a specific feature by index, as a JSON array (see
/// `render_view::feature_edge_entries`).
#[wasm_bindgen]
pub fn get_edge_data(feature_index: usize) -> String {
    with_engine(|e| entries_json(&render_view::feature_edge_entries(&e.state, feature_index)))
        .unwrap_or_else(|| "[]".to_string())
}

/// Number of renderable bodies (mesh-bearing outputs across non-consumed
/// features). This is the count the worker iterates for rendering.
#[wasm_bindgen]
pub fn get_body_count() -> usize {
    with_engine(|e| render_view::collect_renderable_bodies(&e.state).len()).unwrap_or(0)
}

/// Metadata for every renderable body as a JSON array, in body-index order
/// (see `render_view::body_metadata`).
#[wasm_bindgen]
pub fn get_body_metadata() -> String {
    with_engine(|e| entries_json(&render_view::body_metadata(&e.state)))
        .unwrap_or_else(|| "[]".to_string())
}

/// Body mesh vertex positions as a Float32Array (by flat body index). A
/// ghost's vertices are a baked copy; everything else is a zero-copy view.
#[wasm_bindgen]
pub fn get_body_vertices(body_index: usize) -> js_sys::Float32Array {
    with_engine(|e| render_view::body_vertices(&e.state, body_index).map(f32_array))
        .flatten()
        .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Body mesh vertex normals as a Float32Array (by flat body index).
#[wasm_bindgen]
pub fn get_body_normals(body_index: usize) -> js_sys::Float32Array {
    with_engine(|e| render_view::body_normals(&e.state, body_index).map(f32_array))
        .flatten()
        .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Body mesh triangle indices as a Uint32Array view (by flat body index).
#[wasm_bindgen]
pub fn get_body_indices(body_index: usize) -> js_sys::Uint32Array {
    with_engine(|e| {
        render_view::body_indices(&e.state, body_index)
            .map(|indices| unsafe { js_sys::Uint32Array::view(indices) })
    })
    .flatten()
    .unwrap_or_else(|| js_sys::Uint32Array::new_with_length(0))
}

/// Body face-range data (GeomRef-enriched) as JSON, by flat body index.
#[wasm_bindgen]
pub fn get_body_face_data(body_index: usize) -> String {
    with_engine(|e| {
        entries_json(&render_view::body_face_entries(
            &e.state, &e.kernel, body_index,
        ))
    })
    .unwrap_or_else(|| "[]".to_string())
}

/// Body edge vertex positions as a Float32Array (by flat body index).
#[wasm_bindgen]
pub fn get_body_edge_vertices(body_index: usize) -> js_sys::Float32Array {
    with_engine(|e| render_view::body_edge_vertices(&e.state, body_index).map(f32_array))
        .flatten()
        .unwrap_or_else(|| js_sys::Float32Array::new_with_length(0))
}

/// Body edge-range data (GeomRef-enriched) as JSON, by flat body index.
#[wasm_bindgen]
pub fn get_body_edge_data(body_index: usize) -> String {
    with_engine(|e| entries_json(&render_view::body_edge_entries(&e.state, body_index)))
        .unwrap_or_else(|| "[]".to_string())
}
