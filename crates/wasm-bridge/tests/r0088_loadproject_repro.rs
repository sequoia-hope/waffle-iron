//! Native reproduction of the deployed-app `LoadProject` crash on assay R0088.
//!
//! The GUI replays a whole document inside ONE `LoadProject` engine message, so
//! the browser gives no narrowing signal — just a bare wasm `unreachable` trap
//! that does not even reach the `init()` panic hook. This drives the identical
//! `dispatch(UiToEngine::LoadProject)` path natively.
//!
//! **VERDICT (2026-07-28): it is an OUT-OF-MEMORY, not a logic bug.** Natively
//! this path SUCCEEDS — `ModelUpdated`, exit 0 — in ~13.5 s of release wall
//! time at a measured **peak RSS of 6.79 GiB**. wasm32 has a 4 GiB
//! architectural address-space ceiling (32-bit pointers), and browsers cap a
//! single wasm memory well below that, so the allocation cannot succeed there.
//! A failed allocation calls `alloc::handle_alloc_error` -> `abort()`, which
//! compiles to an `unreachable` trap and **bypasses the `init()` panic hook** —
//! which is exactly why the console shows a bare `unreachable` with no
//! `WASM PANIC:` line. Every symptom is accounted for:
//!
//! - native passes, browser traps            -> 64-bit vs 32-bit address space
//! - no panic-hook output                    -> alloc failure is not a panic
//! - assay says SUPPORTED_CORRECT            -> the assay runs natively
//! - a fresh bundle does not fix it          -> not a staleness problem
//!
//! The fix is to reduce peak memory on this path (or to fail LOUDLY with a
//! typed "model exceeds the browser engine's memory" error instead of trapping).
//! Neither is done here; this test is the evidence and the reproduction.

use std::fs;

use wasm_bridge::{dispatch, EngineState, UiToEngine};

fn load_case(id: &str) -> String {
    let path = format!(
        "{}/../../app/tests/cases/assay/{id}.waffle",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
#[ignore = "diagnostic: ~14s and ~6.8 GiB peak RSS in release (~110s in debug). \
            Documents the wasm32 OOM ceiling; run explicitly, not in a tier."]
fn r0088_loadproject_does_not_crash() {
    let json = load_case("R0088");
    let msg: UiToEngine = serde_json::from_str(&format!(
        r#"{{"type":"LoadProject","data":{}}}"#,
        serde_json::to_string(&json).expect("re-encode case text")
    ))
    .expect("LoadProject message must deserialize");

    let mut state = EngineState::new();
    let mut kernel = kernel_v2::KernelV2Adapter::new();
    let resp = dispatch(&mut state, msg, &mut kernel);

    // The assertion is simply that we got HERE without aborting: in wasm this
    // call traps. A returned `Error` variant is a different (and acceptable)
    // outcome — a loud kernel STOP is not a crash.
    match resp {
        wasm_bridge::EngineToUi::Error { message, .. } => {
            eprintln!("R0088 LoadProject returned a loud error (not a crash): {message}");
        }
        other => {
            eprintln!(
                "R0088 LoadProject returned {}",
                serde_json::to_string(&other)
                    .map(|s| s.chars().take(200).collect::<String>())
                    .unwrap_or_default()
            );
        }
    }
}
