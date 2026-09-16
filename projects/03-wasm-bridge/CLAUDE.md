# 03 — WASM Bridge: Agent Instructions

You are working on **wasm-bridge**. Read ARCHITECTURE.md in this directory first.

## Your Job

Build the communication layer between the WASM engine (Rust, running in a Web Worker) and the JavaScript presentation layer (Svelte, running on the main thread). All messages cross this bridge.

## Critical Rules

1. **WASM engine runs in a Web Worker.** All communication with the main thread is via `postMessage`. Never call WASM functions from the main thread directly.
2. **Mesh data must use TypedArray views.** Never serialize mesh vertices/normals/indices as JSON. Use `Float32Array`/`Uint32Array` views into WASM linear memory and transfer via `Transferable` objects.
3. **JSON is fine for metadata.** Feature tree state, solve status, error messages — all can be JSON. Only mesh data needs TypedArray treatment.
4. **The sketch solver is pure Rust** (Levenberg-Marquardt + nalgebra) and
   compiles to `wasm32-unknown-unknown` inside this same module. There is no
   Emscripten build and no second WASM module — the libslvs era is over.
5. **Every send carries a request id.** The bridge posts `{id, msg}` and the
   worker echoes the id back on the answer (`specs/waffle_server_mode.md` S1).
   Never pair an answer to a caller by arrival order.

## Build & Test

```bash
cargo test -p wasm-bridge
cargo clippy --all-targets -p wasm-bridge -- -D warnings
./scripts/build-wasm.sh   # rebuild the bundle; `git add` new files FIRST,
                          # the fingerprint hashes tracked files only
```

## Key Files

- `src/lib.rs` — module wiring
- `src/wasm_api.rs` — the `#[wasm_bindgen]` shim, and the only browser-coupled
  file in the crate. Keep it a binding layer: target-independent logic belongs
  below, or a second host has to duplicate it (`specs/waffle_server_mode.md` S0)
- `src/process.rs` — the message pipeline (parse → dispatch → tessellate →
  preview → serialize), clock and logger injected
- `src/dispatch.rs` — `UiToEngine` dispatch, the single message entry point
- `src/messages.rs` — `UiToEngine` / `EngineToUi` definitions
- `src/session.rs` — `DocumentSession`: tabs, inactive trees, assembly trees,
  document metadata, per-tab undo, the revision
- `src/render_view.rs` — renderable bodies, body metadata/naming, face and edge
  entries, ghost baking
- `src/assembly_view.rs`, `src/face_refs.rs`, `src/stl_export.rs`,
  `src/tessellation_runner.rs`, `src/engine_state.rs`

**The JS side lives in the app, not here**: `app/src/lib/engine/bridge.js`
(main thread) and `app/src/lib/engine/worker.js` (worker). A dead pre-SvelteKit
copy of both sat in `crates/wasm-bridge/js/` until 2026-09-16; it was deleted
because it drifted silently. Do not recreate it.

## Dependencies

- wasm-bindgen, js-sys, web-sys — **target-gated to wasm32**, and used only in
  `wasm_api.rs`. The rest of the crate builds and tests natively; keep it that way
- serde, serde_json, chrono, uuid, thiserror, base64
- Engine crates: feature-engine, kernel-v2, modeling-ops, sketch-solver,
  step-import, file-format, waffle-types (the legacy `kernel` crate is deleted)
