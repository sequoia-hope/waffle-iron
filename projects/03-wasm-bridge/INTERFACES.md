# 03 — WASM Bridge: Interfaces

## Rust API

```rust
pub use dispatch::dispatch;
pub use engine_state::{BridgeError, EngineState};
pub use messages::{EngineToUi, UiToEngine};
pub use session::{DocumentSession, SessionError, TabInfo};
pub use tools::{execute_tool, ExportFile, ToolResult, MAX_AGENT_PAYLOAD_BYTES};

// process.rs — the pipeline every host runs
pub fn process_message(
    state: &mut EngineState,
    kernel: &mut dyn KernelBundle,
    msg: UiToEngine,
    now_ms: &dyn Fn() -> f64,
    log: &dyn Fn(&str),
) -> EngineToUi;
```

`UiToEngine` and `EngineToUi` are defined in `src/messages.rs`. Every type
that crosses the boundary derives serde `Serialize`/`Deserialize`.

## WASM exports (`src/wasm_api.rs`)

| Export | Purpose |
|---|---|
| `init()` | install the panic hook, create the engine |
| `process_message(json) -> json` | one `UiToEngine` in, one `EngineToUi` out |
| `set_progress_sink(fn)` | progress callback for long operations |
| `get_feature_tree() -> json` | current feature tree |
| `get_body_count()`, `get_body_metadata()` | renderable bodies |
| `get_body_vertices/normals/indices(i)` | body mesh as typed arrays |
| `get_body_face_data(i)`, `get_body_edge_data(i)`, `get_body_edge_vertices(i)` | face ranges and edges for picking and overlay |
| `get_mesh_*`, `get_face_data`, `get_edge_*`, `get_renderable_feature_indices` | the same data indexed by feature |

## JS API (`app/src/lib/engine/bridge.js`)

`EngineBridge`: `init(wasmUrl)`, `send(message) -> Promise`, `sendUngated`,
`setSendGate`, `on(event, callback)`, `recordSends`, `getSendLog`,
`terminate`. The worker's envelope is `{id, msg}` in both directions; an
answer that carries a model has its mesh buffers on `msg.meshes` (or
`msg.model.meshes` for a `ToolResult`).

## Consumes

`feature-engine` (the engine, feature tree, operations), `modeling-ops`
(`KernelBundle`), `kernel-v2` (the kernel), `sketch-solver`, `file-format`,
`step-import`, `kicad-pcb`, and the shared types in `waffle-types`.
