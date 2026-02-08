# 03 — WASM Bridge: Architecture

## Purpose

Communication layer between the Rust/WASM engine (running in a Web Worker) and the JavaScript presentation layer (running on the main thread). All cross-language communication flows through this bridge.

## Communication Architecture

### Two Channels

1. **Commands (JS → WASM):** User actions in the UI produce `UiToEngine` messages. These are serialized as JSON, sent via `postMessage` to the Web Worker, deserialized, and dispatched to the appropriate engine crate (feature-engine, sketch-solver).

2. **Results (WASM → JS):** Engine operations produce `EngineToUi` messages. Metadata (feature tree state, solve status, errors) is serialized as JSON. Mesh data (vertices, normals, indices) is transferred as TypedArray views into WASM linear memory for near-zero-copy performance.

### Web Worker Setup

```
Main Thread                          Web Worker
┌─────────────┐                    ┌─────────────────────────┐
│  Svelte UI  │   postMessage      │  wasm-bridge            │
│  + Threlte  │ ───────────────►   │  ├── deserialize cmd    │
│  + sketch   │                    │  ├── dispatch to engine  │
│             │   postMessage      │  ├── engine processes    │
│             │ ◄───────────────   │  ├── serialize result    │
│             │   (+ Transferable) │  └── mesh as TypedArray  │
└─────────────┘                    │                         │
                                   │  WASM modules:          │
                                   │  ├── Rust engine (.wasm) │
                                   │  └── libslvs (.wasm)    │
                                   └─────────────────────────┘
```

## Message Serialization

### Commands (UiToEngine)
- Serialized as JSON using serde_json.
- JSON is well-supported by postMessage and human-readable for debugging.
- Command frequency is low (user actions), so JSON overhead is negligible.

### Results (EngineToUi)
- **Metadata** (feature tree, solve status, errors): JSON via serde_json.
- **Mesh data** (vertices, normals, indices): TypedArray views into WASM linear memory.
  - `Float32Array` for vertices and normals
  - `Uint32Array` for indices
  - Transferred as `Transferable` objects in postMessage for zero-copy semantics
  - A 1MB mesh buffer transfers in ~1ms

### Why Not Binary Everywhere?
JSON for metadata is fine — these messages are small (kilobytes) and infrequent. Binary (bincode) would save ~50% size but adds complexity and hurts debuggability. The performance-critical path is mesh data, which uses TypedArray views regardless.

## Mesh Transfer Protocol

When the model changes and new meshes are produced:

1. Engine tessellates the solid → `RenderMesh` (Vec<f32>, Vec<f32>, Vec<u32>).
2. Bridge exposes these Vecs as TypedArray views via wasm-bindgen:
   ```rust
   #[wasm_bindgen]
   pub fn get_vertices() -> js_sys::Float32Array {
       // Create a view into WASM memory — no copy
       unsafe { js_sys::Float32Array::view(&self.mesh.vertices) }
   }
   ```
3. Worker sends to main thread via `postMessage` with Transferable.
4. Main thread receives `Float32Array`/`Uint32Array`, passes directly to three.js `BufferGeometry`.

This is the universal pattern used by every WASM CAD tool surveyed.

## libslvs WASM Module

The constraint solver (libslvs) is C code compiled to WASM via Emscripten as a separate module. Both WASM modules (Rust engine + libslvs) run in the same Web Worker.

Bridge flow for sketch solving:
1. Rust engine receives `SolveSketch` command.
2. Rust engine maps sketch entities/constraints to a data structure.
3. JS glue code passes data to the libslvs WASM module.
4. libslvs solves, returns results.
5. JS glue code passes results back to Rust engine.
6. Rust engine produces `SketchSolved` response.

Constraint solving is infrequent, so the JS bridge overhead between the two WASM modules is negligible.

## Error Propagation

- Rust panics are caught by `console_error_panic_hook` and converted to structured error messages.
- `KernelError` and solver errors are converted to `EngineToUi::Error` messages with human-readable messages and (when applicable) the feature ID that caused the error.
- Worker errors (WASM instantiation failure, memory issues) are caught by the Worker's `onerror` handler and forwarded to the main thread.

## Performance Characteristics

- **JS → WASM call overhead:** ~2.5 nanoseconds per call (Mozilla benchmarks). Not a bottleneck.
- **Data marshalling:** Primitives (i32, f64) are free. Strings are expensive. Avoid string-heavy protocols.
- **TypedArray views:** Near-zero cost (creates a view, no copy).
- **postMessage:** Structured clone for JSON data. Transferable for TypedArray buffers.
- **Heavy operations** (boolean, tessellation) run in the Worker and don't block the UI thread.
