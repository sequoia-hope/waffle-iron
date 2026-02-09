# 03 — WASM Bridge: Plan

## Milestones

### M1: Build Pipeline (partial) ✅
- [ ] Set up wasm-pack build for Rust engine crates (requires Node.js/npm)
- [ ] Verify wasm-bindgen output (requires Node.js/npm)
- [x] Create bridge crate skeleton

### M2: Message Types ✅
- [x] Implement `UiToEngine` serialization (JSON via serde_json)
- [x] Implement `EngineToUi` serialization (JSON for metadata)
- [x] Round-trip tests: serialize → deserialize for all message variants (7 serde tests)

### M3: Web Worker Setup
- [ ] Worker script that loads WASM module
- [ ] postMessage handler for incoming commands
- [ ] postMessage sender for outgoing results
- [ ] Worker error handling (onerror)

### M4: Command Dispatch (partial) ✅
- [x] Deserialize UiToEngine in Worker
- [x] Dispatch to appropriate engine function
- [x] Handle all command variants (feature ops, selection, hover; undo/redo/file ops return NotImplemented)
- [x] Test: send command → verify engine receives it (5 dispatch tests)

### M5: Result Callback
- [ ] Serialize EngineToUi in Worker
- [ ] postMessage to main thread
- [ ] Main thread message handler
- [ ] Test: engine produces result → verify UI receives it

### M6: Mesh Transfer
- [ ] Expose RenderMesh vertex/normal/index data as TypedArray views
- [ ] Transfer via postMessage with Transferable objects
- [ ] Verify zero-copy semantics (view into WASM memory)
- [ ] Test: tessellate a box → transfer mesh → verify data integrity
- [ ] Benchmark: measure transfer time for various mesh sizes

### M7: libslvs WASM Module
- [ ] Emscripten build configuration for libslvs
- [ ] Load libslvs WASM module in Worker
- [ ] JS glue code bridging Rust engine ↔ libslvs
- [ ] Test: solve a simple sketch through the full bridge path

### M8: Error Propagation
- [ ] Install console_error_panic_hook
- [ ] Convert KernelError → EngineToUi::Error
- [ ] Convert solver errors → EngineToUi::Error
- [ ] Worker-level error forwarding
- [ ] Test: trigger an error → verify UI receives structured error message

### M9: Latency Benchmarking
- [ ] Measure command round-trip time (UI → Worker → engine → Worker → UI)
- [ ] Measure mesh transfer time for 10K, 100K, 1M triangle meshes
- [ ] Document baseline performance
- [ ] Identify bottlenecks if any

## Blockers

- Depends on kernel-fork (M6 needs tessellation output)
- libslvs Emscripten build (M7) may have platform-specific issues

## Interface Change Requests

(None yet)

## Notes

- Never serialize mesh vertices as JSON — always use TypedArray views.
- The two-WASM-module approach (Rust + libslvs) is a short-term solution. Long-term: port solver to pure Rust.
