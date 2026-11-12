# 14 — Agent Link: Agent Instructions

Read `specs/waffle_mcp_server.md`, `specs/waffle_server_mode.md` and
ARCHITECTURE.md in this directory before changing anything here.

## Parts

| Part | Where | Language |
|---|---|---|
| Relay `waffle-mcp-relay` | `relay/` | Python ≥ 3.12, `uv` |
| Page side | `app/src/lib/agent/`, `app/src/routes/agent/` | JS / Svelte |
| Tool semantics | `crates/wasm-bridge/src/tools/` | Rust |
| Native host | `crates/waffle-host/` | Rust |

## Rules

- **One implementation of tool semantics.** Engine tools live in
  `wasm-bridge`; the page and the host both call them. The relay holds no
  modeling logic and no engine. Do not reimplement a tool in JS.
- **Never hardcode a port.** The relay takes `--port`, then `$PORT`, then
  `proj port`, else exits 2. A test fixture may allocate an OS port and pass
  it with `--port`.
- **Consent before connection.** The page opens the socket only on the
  user's click.
- **Errors are typed.** Match on the error `code`; never parse a message string.
- **Fillet, chamfer and shell are refused** (`Deferred`).
- **A mutating tool's `ToolResult` carries the model update**, and the tool
  tessellates before its after-snapshot.
- **Regenerate the manifest** (`node app/scripts/gen-agent-manifest.mjs`)
  when a tool or an engine type in a tool schema changes.
- A change under `wasm-bridge` ships with a rebuilt WASM bundle in the same
  commit (`./scripts/build-wasm.sh`).
- After adding a tool, restart the MCP client to see it.

## Testing

- Rust: `cargo test -p wasm-bridge` (the `tool_*` tests),
  `cargo test -p waffle-host`; clippy with `--all-targets`.
- Relay: `./scripts/test.sh relay`, or
  `cd relay && uv run pytest && uv run ruff check && uv run ruff format --check`.
- GUI: `./scripts/test.sh gui-relay` runs the `agent-*.spec.js` and
  `viewer.spec.js` specs against the real relay.
