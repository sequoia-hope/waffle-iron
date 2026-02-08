# 03 — WASM Bridge: Agent Instructions

You are working on **wasm-bridge**. Read ARCHITECTURE.md in this directory first.

## Your Job

Build the communication layer between the WASM engine (Rust, running in a Web Worker) and the JavaScript presentation layer (Svelte, running on the main thread). All messages cross this bridge.

## Critical Rules

1. **WASM engine runs in a Web Worker.** All communication with the main thread is via `postMessage`. Never call WASM functions from the main thread directly.
2. **Mesh data must use TypedArray views.** Never serialize mesh vertices/normals/indices as JSON. Use `Float32Array`/`Uint32Array` views into WASM linear memory and transfer via `Transferable` objects.
3. **JSON is fine for metadata.** Feature tree state, solve status, error messages — all can be JSON. Only mesh data needs TypedArray treatment.
4. **libslvs is a separate WASM module.** Compiled via Emscripten, loaded in the same Worker, bridged via JS glue code.

## Build & Test

```bash
wasm-pack build --target web
# Or for testing:
cargo test -p wasm-bridge
```

## Key Files

- `src/lib.rs` — wasm-bindgen entry point
- `src/commands.rs` — UiToEngine deserialization and dispatch
- `src/results.rs` — EngineToUi serialization
- `src/mesh_transfer.rs` — TypedArray view creation for mesh data
- `js/worker.js` — Web Worker script
- `js/bridge.js` — Main thread API (postMessage wrapper)

## Dependencies

- wasm-bindgen, js-sys, web-sys
- serde, serde_json
- console_error_panic_hook
- All Rust engine crates (kernel-fork, feature-engine, modeling-ops, sketch-solver)
