//! PR-VIZ-3a — native-callable shim for the Yang debug capture API.
//!
//! The `#[wasm_bindgen]` exports in `wasm_api.rs` are thin wrappers that
//! call into these free fns. Re-exported from `lib.rs` so native tests
//! (e.g. `crates/wasm-bridge/tests/pr_viz_3a_capture.rs`) can drive the
//! same surface without `wasm-pack`.
//!
//! On `wasm32`, the shim routes through `wasm_api::ENGINE_STATE` so the
//! capture flag/map lives on the same per-worker engine that `dispatch`
//! mutates. On native (test-only), the shim uses its own thread-local
//! `EngineState` — no real engine exists outside the wasm worker, but
//! the API still has to be callable so the spec §5 contract holds.
//!
//! Spec: specs/yang_pr_viz_3a_in_memory_capture.md §5 + §6

use crate::engine_state::EngineState;

#[cfg(not(target_arch = "wasm32"))]
std::thread_local! {
    static NATIVE_ENGINE_STATE: std::cell::RefCell<EngineState> =
        std::cell::RefCell::new(EngineState::new());
}

/// Spec §5: arm or disarm in-memory Yang stage capture for the current
/// engine. Subsequent `dispatch::dispatch` AddFeature/EditFeature calls
/// will populate `EngineState::yang_debug_captures`.
pub fn set_yang_debug_capture(enabled: bool) {
    with_state_mut(|s| s.yang_debug_capture_enabled = enabled);
}

/// Spec §5: look up the captured stages for one feature_id (Uuid string).
/// Returns the literal JSON string `"null"` when absent (matches the
/// `get_face_data`/`get_mesh_json` JSON-getter precedent).
pub fn get_yang_stages_json(feature_id: &str) -> String {
    with_state(|s| {
        s.yang_debug_captures
            .get(feature_id)
            .map(|c| serde_json::to_string(c).unwrap_or_else(|_| "null".to_string()))
            .unwrap_or_else(|| "null".to_string())
    })
}

/// Spec §5: clear the captures map (free memory). The capture-enabled
/// flag is left unchanged.
pub fn clear_yang_debug_captures() {
    with_state_mut(|s| s.yang_debug_captures.clear());
}

#[cfg(target_arch = "wasm32")]
fn with_state<R>(f: impl FnOnce(&EngineState) -> R) -> R {
    crate::wasm_api::with_engine_state(f)
}

#[cfg(target_arch = "wasm32")]
fn with_state_mut<R>(f: impl FnOnce(&mut EngineState) -> R) -> R {
    crate::wasm_api::with_engine_state_mut(f)
}

#[cfg(not(target_arch = "wasm32"))]
fn with_state<R>(f: impl FnOnce(&EngineState) -> R) -> R {
    NATIVE_ENGINE_STATE.with(|cell| f(&cell.borrow()))
}

#[cfg(not(target_arch = "wasm32"))]
fn with_state_mut<R>(f: impl FnOnce(&mut EngineState) -> R) -> R {
    NATIVE_ENGINE_STATE.with(|cell| f(&mut cell.borrow_mut()))
}
