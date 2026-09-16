# 03 — WASM Bridge: Plan

## Milestones

### M1: Build Pipeline ✅
- [x] Create bridge crate skeleton
- [x] Set up wasm-pack build for Rust engine crates
- [x] Verify wasm-bindgen output (4 exported functions: init, process_message, get_feature_tree, get_mesh_json)
- [x] WASM binary: 1.9 MB (release, wasm-opt)
- [x] Feature-gated sketch-solver (native-solver feature) — excluded from WASM build because libslvs C++ can't compile to wasm32-unknown-unknown

### M2: Message Types ✅
- [x] Implement `UiToEngine` serialization (JSON via serde_json)
- [x] Implement `EngineToUi` serialization (JSON for metadata)
- [x] Round-trip tests: serialize → deserialize for all message variants (7 serde tests)

### M3: Web Worker Setup ✅
- [x] Worker script that loads WASM module (js/worker.js)
- [x] postMessage handler for incoming commands
- [x] postMessage sender for outgoing results
- [x] Worker error handling (onerror)
- [x] Main-thread bridge API (js/bridge.js) with Promise-based send()

### M4: Command Dispatch ✅
- [x] Deserialize UiToEngine in Worker
- [x] Dispatch to appropriate engine function
- [x] Handle all command variants (feature ops, selection, hover, undo/redo, save/load)
- [x] Undo/Redo wired to feature-engine undo/redo
- [x] SaveProject: serializes feature tree to JSON via file-format
- [x] LoadProject: deserializes, replaces tree, rebuilds
- [x] SolveSketch: wired to sketch-solver (native builds only)
- [x] Full sketch workflow: BeginSketch → AddEntity → Solve → FinishSketch
- [x] ExportStep: not yet wired (requires TruckKernel, not generic KernelBundle)
- [x] ReorderFeature and RenameFeature message dispatch wired
- [x] Tests: 21 total (7 serde + 8 dispatch + 3 engine state + 3 sketch workflow)

### M5: Result Callback ✅
- [x] Serialize EngineToUi in Worker (JSON via serde_json in wasm_api.rs)
- [x] postMessage to main thread (worker.js sends response)
- [x] Main thread message handler (bridge.js Promise-based API + event handlers)
- [x] Covered by M3 worker/bridge implementation

### M6: Mesh Transfer (partial) ✅
- [x] Expose RenderMesh vertex/normal/index data as TypedArray views (get_mesh_vertices, get_mesh_normals, get_mesh_indices)
- [x] Transfer via postMessage with Transferable objects (worker.js collectMeshes())
- [x] Copy-from-WASM-view pattern (views invalidated by memory growth, copy to standalone ArrayBuffers)
- [x] get_mesh_count() helper to enumerate features with meshes
- [x] ModelUpdated responses automatically attach typed array mesh data
- [ ] Browser integration test (requires browser environment)
- [ ] Benchmark: measure transfer time for various mesh sizes (requires browser environment)

### M7: libslvs WASM Module ✅
- [x] Emscripten build: em++ compiles vendored SolveSpace C++ + mimalloc to slvs.wasm (226KB)
- [x] Worker loads libslvs via fetch+blob dynamic import (bypasses Vite bundling)
- [x] JS bridge (slvs-solver.js): maps sketch entities/constraints to slvs C API structs on Emscripten heap
- [x] SolveSketchLocal worker message type: intercepts solve requests, calls libslvs, returns solved positions
- [x] Store integration: triggerSolve() sends sketch state to worker, handles SketchSolved response
- [x] DOF counter displayed in status bar
- [ ] Browser integration test (requires browser environment)

### M8: Error Propagation (partial) ✅
- [x] Install console_error_panic_hook (in wasm_api.rs init())
- [x] Convert EngineError → EngineToUi::Error (via BridgeError in dispatch.rs)
- [x] Worker-level error forwarding (onerror handler in worker.js)
- [x] Convert solver errors → SketchSolved response with error status (via JS bridge)
- [x] Tests: dispatch errors verified (undo empty, delete nonexistent, unimplemented)

### M9: Latency Benchmarking
- [ ] Measure command round-trip time (UI → Worker → engine → Worker → UI)
- [ ] Measure mesh transfer time for 10K, 100K, 1M triangle meshes
- [ ] Document baseline performance
- [ ] Identify bottlenecks if any

### M10: Server-mode prep — S0 render view off the wasm gate ✅
Spec: `specs/waffle_server_mode.md` §2.3 (P-A).
- [x] `src/render_view.rs`: renderable bodies, body metadata/naming, face and edge entries, ghost baking, legacy per-feature accessors — target-independent (2026-09-15)
- [x] `src/process.rs`: the `process_message` pipeline with injected clock and logger (2026-09-15)
- [x] `src/wasm_api.rs` reduced to bindings; exported JS API unchanged (20 functions, same signatures and imports)
- [x] Oracle `tests/render_view_parity.rs` + `.mjs`: rebuilt bundle census byte-identical to the pre-move bundle; native census structurally equal to the bundle's
- [ ] S1: request ids in the bridge envelope (replaces FIFO pairing in `bridge.js`)
- [ ] Known, not fixed: `feature_engine::preview_mesh::decimate_mesh` orders output by `HashMap` iteration, so a native process's `preview_mesh` varies run to run (spec §2.7 H3)
- [ ] Known, not fixed: `cargo clippy -p wasm-bridge --target wasm32-unknown-unknown` flags the `thread_local!` initializer in `wasm_api.rs` (pre-existing; CI does not lint wasm32)

### M11: Server-mode S2 — the document session in Rust
Spec: `specs/waffle_server_mode.md` §2.3 S2 (P-A). Staged as four atomic
checkpoints; the JS store is untouched until C4, so the browser path is
unchanged throughout.
- [x] **C1 — the session type, wired to nothing** (2026-09-16): `src/session.rs`
      `DocumentSession` owns the document metadata, the tab list, every
      inactive tab's tree, assembly trees, per-tab preview meshes, a per-tab
      undo history, and a monotonic `revision`. 15 unit tests.
      `feature_engine::Engine` gained `take_history`/`set_history` (the stack
      was private with no accessor, so a host could not park it per tab).
- [ ] C2: `EngineState` owns the session; `LoadProject`/`NewDocument` populate
      it and `ModelUpdated` reports it. No message signature changes.
- [ ] C3: new messages `AddTab`/`CloseTab`/`RenameTab`/`MoveTab`/
      `SetDocumentMeta`/`EditAssembly`; `SwitchTab` takes a `tab_id` not a
      tree; `OpenAssembly` takes a `tab_id` and the session supplies the part
      trees (today JS re-sends every tree on every assembly evaluation —
      the largest payload this removes); `SaveDocument` loses its payload
      (v4 §4 inv. 7 one writer). Regenerate the render-view parity fixtures.
- [ ] C4: the JS store's tab/assembly/metadata `$state` becomes a mirror fed
      by `ModelUpdated`; delete the second `.waffle` parser in
      `initDocumentState`. A2.1 compliance.
- Found by C1, to fix in C2/C3: **undo already leaks across tabs today** —
  `SwitchTab` replaces `engine.tree` and `rebuild_from_scratch` clears only
  results, never `undo_stack`, so an `Undo` after a switch pops a command
  recorded against the tab you just left. The per-tab stack is a behavior
  fix, not only a relocation.
- Found by C1: the JS `switchTab` writes `kind.features` without checking the
  tab kind, stamping an empty `features` key onto an Assembly tab that is
  then serialized (harmless — `TabKind` ignores unknown keys — but the JS and
  Rust views of an Assembly tab differ). The session refuses to do this.
- Found by C1: `feature_engine::preview_mesh::PreviewMesh` and
  `file_format::PreviewMesh` are structurally identical, nominally distinct,
  and have no conversion anywhere. JS never noticed (both are JSON on the
  wire); `DocumentSession::set_preview_mesh` converts field-wise.

## Blockers

- ~~Depends on kernel-fork (M6 needs tessellation output)~~ RESOLVED
- libslvs Emscripten build (M7) may have platform-specific issues
- M6 mesh TypedArray views need browser testing environment

## Interface Change Requests

(None)

## Notes

- Never serialize mesh vertices as JSON — always use TypedArray views.
- The two-WASM-module approach (Rust + libslvs) is a short-term solution. Long-term: port solver to pure Rust.
- sketch-solver is feature-gated (`native-solver`) because libslvs C++ code can't compile to wasm32-unknown-unknown without Emscripten.
- Removed unused sketch-solver dependency from feature-engine crate.
- WASM build command: two-step process (wasm-pack can't do -Zbuild-std):
  1. `cargo +nightly build -p wasm-bridge --target wasm32-unknown-unknown --release --no-default-features -Zbuild-std`
  2. `wasm-bindgen target/wasm32-unknown-unknown/release/wasm_bridge.wasm --out-dir crates/wasm-bridge/pkg --target web --no-typescript`
