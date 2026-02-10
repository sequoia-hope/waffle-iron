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

### M6: Mesh Transfer ✅
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
- WASM build command: `wasm-pack build crates/wasm-bridge --target web --no-typescript -- --no-default-features`
