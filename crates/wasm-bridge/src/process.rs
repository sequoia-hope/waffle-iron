//! The message pipeline every engine host runs: parse a `UiToEngine`,
//! dispatch it, tessellate new bodies, attach the decimated preview, and
//! serialize the `EngineToUi` response.
//!
//! Target-independent. `wasm_api::process_message` supplies the browser's
//! clock and console; a native host supplies its own
//! (`specs/waffle_server_mode.md` §2.3 S0).

use modeling_ops::KernelBundle;

use crate::dispatch;
use crate::engine_state::EngineState;
use crate::messages::{EngineToUi, UiToEngine};

/// Dispatch one message and bring the render data up to date: after a
/// `ModelUpdated`, tessellate any solids that don't have mesh data yet and
/// recompute the preview from the new meshes. `now_ms` times the two stages;
/// `log` receives one line when together they exceed 100 ms.
pub fn process_message(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    msg: UiToEngine,
    now_ms: &dyn Fn() -> f64,
    log: &dyn Fn(&str),
) -> EngineToUi {
    let msg_type = format!("{:?}", std::mem::discriminant(&msg));
    let t0 = now_ms();
    let mut response = dispatch::dispatch(state, msg, kernel);
    let dispatch_ms = now_ms() - t0;

    if matches!(response, EngineToUi::ModelUpdated { .. }) {
        let t1 = now_ms();
        crate::tessellation_runner::tessellate_missing_meshes(state, kernel);
        dispatch::attach_preview_mesh(state, &mut response);
        let tess_ms = now_ms() - t1;
        if dispatch_ms + tess_ms > 100.0 {
            log(&format!(
                "[wasm] {} dispatch={:.1}s tess={:.1}s total={:.1}s",
                msg_type,
                dispatch_ms / 1000.0,
                tess_ms / 1000.0,
                (dispatch_ms + tess_ms) / 1000.0,
            ));
        }
    }

    response
}

/// `process_message` on a JSON-serialized `UiToEngine`, returning the
/// JSON-serialized `EngineToUi` response. A message that does not parse
/// becomes an `Error` response.
pub fn process_message_json(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    json_input: &str,
    now_ms: &dyn Fn() -> f64,
    log: &dyn Fn(&str),
) -> String {
    let response = match serde_json::from_str::<UiToEngine>(json_input) {
        Ok(msg) => process_message(state, kernel, msg, now_ms, log),
        Err(e) => EngineToUi::Error {
            kind: None,
            message: format!("Failed to parse message: {}", e),
            feature_id: None,
        },
    };

    serde_json::to_string(&response).unwrap_or_else(|e| {
        format!(
            r#"{{"type":"Error","message":"Serialization failed: {}","feature_id":null}}"#,
            e
        )
    })
}
