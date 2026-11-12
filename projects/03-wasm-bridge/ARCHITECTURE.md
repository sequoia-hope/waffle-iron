# 03 — WASM Bridge: Architecture

`crates/wasm-bridge` is the engine's message layer. It owns the engine state,
dispatches every command, and produces the render data the UI draws. In the
browser it is compiled to one WASM module that runs in a Web Worker; the
native host (`crates/waffle-host`) links the same crate as an ordinary Rust
library.

## Layers

```
Main thread                      Web Worker
┌──────────────────┐  {id, msg}  ┌───────────────────────────────┐
│ Svelte UI        │ ──────────► │ worker.js                     │
│ store.svelte.js  │             │   wasm_api::process_message   │
│ bridge.js        │ ◄────────── │   process → dispatch → engine │
└──────────────────┘  {id, msg}  │   mesh buffers (Transferable) │
                     + meshes    └───────────────────────────────┘
```

- **`app/src/lib/engine/bridge.js`** (main thread) — `EngineBridge`. `send()`
  posts `{id, msg}` and returns a promise; the worker echoes the id on the
  answer. Answers are paired to callers by id, never by arrival order.
- **`app/src/lib/engine/worker.js`** — loads the WASM module, forwards each
  message to `process_message`, and attaches mesh buffers to any answer that
  carries a model.
- **`src/wasm_api.rs`** — the `#[wasm_bindgen]` shim and the only
  browser-coupled file in the crate. It is compiled for `wasm32` only.
- **`src/process.rs`** — the pipeline every host runs: parse a `UiToEngine`,
  dispatch it, tessellate new bodies, attach the decimated preview, serialize
  the `EngineToUi`. The clock and logger are injected.
- **`src/dispatch.rs`** — the single entry point for `UiToEngine` messages.

Everything below `wasm_api.rs` is target-independent and is tested natively.

## State

- **`EngineState`** (`engine_state.rs`) — the feature `Engine`, the active
  sketch, selection and hover, the open assembly view, and the document
  session.
- **`DocumentSession`** (`session.rs`) — tabs, the trees of inactive tabs,
  assembly trees, document metadata, per-tab undo history, and the revision.
  The UI store mirrors the session from each `ModelUpdated`; it does not keep
  its own copy of the tab list or document name.

## Messages

Commands (`UiToEngine`) and answers (`EngineToUi`) are JSON via `serde_json`
(`messages.rs`). They are small and infrequent.

Mesh data never travels as JSON. After an answer that carries a model, the
worker reads each body's vertices, normals, indices and edge vertices through
the `get_body_*` typed-array accessors, copies them out of WASM memory (views
are invalidated when memory grows), and posts the buffers as `Transferable`
objects.

## Render view

`render_view.rs` decides which bodies are renderable, names them, builds the
per-face and per-edge entries used for picking, and bakes ghost bodies.
`tessellation_runner.rs` tessellates bodies that have no mesh yet.
`assembly_view.rs` and `drawing_view.rs` do the same for assembly and drawing
tabs.

## Agent tools

`src/tools/` implements the agent tools: `execute_tool(state, kernel, name,
args, ctx) -> ToolResult`. The page and the native host both route a tool call
here, so a tool behaves the same in either. `MIGRATED` lists the tool names
this crate answers.

| File | Tools |
|---|---|
| `summary.rs`, `inspect.rs`, `names.rs` | read-only queries, measurement, entity names |
| `author.rs` | feature authoring (`apply_step`) |
| `sketch.rs`, `sketch3d.rs` | sketch creation and editing |
| `assembly.rs` | assembly authoring |
| `drawing.rs` | drawings |
| `script.rs` | Rhai feature scripts |
| `tabs.rs` | document tabs |
| `export.rs` | STEP / STL export |

## Errors

The engine stack is `Result`-based. `KernelError`, solver and engine errors
become `EngineToUi::Error` with a message and, where known, the feature id. A
panic hook prints the panic message to the console. On a fatal engine error
the worker re-instantiates the WASM module and reports that the engine was
restarted.
