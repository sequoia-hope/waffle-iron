# 03 — WASM Bridge: Agent Instructions

You are working on `crates/wasm-bridge`. Read ARCHITECTURE.md in this
directory first.

## Rules

1. **The engine runs in a Web Worker.** The main thread talks to it only
   through `postMessage`.
2. **Mesh data uses typed arrays.** Never serialize vertices, normals or
   indices as JSON. Metadata is JSON.
3. **Every send carries a request id.** The bridge posts `{id, msg}` and the
   worker echoes the id. Never pair an answer to a caller by arrival order.
4. **`wasm_api.rs` is a binding layer.** Target-independent logic goes below
   it, or the native host has to duplicate it.
5. **A mutating tool's `ToolResult` must carry the model update** — nothing
   else refreshes the host — and the tool must tessellate before taking its
   after-snapshot, or `bodies_added` is always empty.
6. **The store mirrors the session.** Do not add a second writer for tab or
   document state on the JS side.

## Build & Test

```bash
cargo test -p wasm-bridge
cargo clippy --all-targets -p wasm-bridge -- -D warnings
./scripts/build-wasm.sh   # rebuild the bundle; `git add` new files first,
                          # the fingerprint hashes tracked files only
```

## Key Files

- `src/wasm_api.rs` — `#[wasm_bindgen]` exports
- `src/process.rs` — the message pipeline
- `src/dispatch.rs` — `UiToEngine` dispatch
- `src/messages.rs` — `UiToEngine` / `EngineToUi`
- `src/session.rs` — `DocumentSession`
- `src/engine_state.rs` — `EngineState`, `BridgeError`
- `src/render_view.rs`, `src/tessellation_runner.rs`, `src/face_refs.rs`,
  `src/entity_names.rs` — render data and naming
- `src/assembly_view.rs`, `src/drawing_view.rs`, `src/sketch_query.rs`,
  `src/stl_export.rs`
- `src/tools/` — agent tool semantics (`mod.rs` holds `execute_tool`,
  `MIGRATED`, `ToolResult` and the shared helpers)

The JS side lives in the app: `app/src/lib/engine/bridge.js` (main thread) and
`app/src/lib/engine/worker.js` (worker).

## Dependencies

- `wasm-bindgen`, `js-sys`, `web-sys` — gated to `wasm32` and used only in
  `wasm_api.rs`.
- `serde`, `serde_json`, `chrono`, `uuid`, `thiserror`, `base64`
- Workspace: `feature-engine`, `kernel-v2`, `modeling-ops`, `sketch-solver`,
  `step-import`, `kicad-pcb`, `file-format`, `waffle-types`
