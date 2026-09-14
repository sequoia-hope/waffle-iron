# Sub-project 14: Agent Link — Agent Instructions

The plan of record is **`specs/waffle_mcp_server.md`** (live-app agent link).
Read it before touching anything here; this file only routes work.

## What this sub-project is

A local AI agent (any MCP client) works in the Waffle Iron page the user has
open. Three parts:

| Part | Where | Language |
|---|---|---|
| Relay `waffle-mcp-relay` (MCP on stdio ↔ WebSocket to the paired page) | `relay/` | Python ≥ 3.12, `uv` |
| Agent host (tool registry, executor, `/agent` consent route, agent bar) | `app/src/lib/agent/`, `app/src/routes/agent/` | JS / Svelte |
| Bridge ICRs (typed errors, measurement, face listing, provenance) | the owning crates (`wasm-bridge`, `feature-engine`, `waffle-types`, `kernel-v2`) | Rust |

## Rules that are easy to break

- **One implementation of modeling semantics.** All tool logic runs in the
  page against the real store and worker (spec I1). The relay holds no
  modeling logic and no engine.
- **Never hardcode a port.** Relay: `--port`, then `$PORT`, then `proj port`,
  else exit 2. Test fixtures may allocate an OS port and pass it explicitly
  via `--port`; the relay itself never picks one.
- **Consent before connection.** A page never opens the WebSocket from a URL
  alone (I7).
- **Fillet, chamfer, shell are refused** (`Deferred`) — deferred indefinitely
  project-wide.
- **No error-message parsing.** Typed errors come from ICR-2, not from matching
  `Display` strings.
- ICR work lands in the owning crate with its own tests, and — for anything
  under `wasm-bridge` — with a rebuilt WASM bundle in the same commit
  (`./scripts/build-wasm.sh`).

## Testing

- Rust ICRs: `cargo test -p <crate>`; clippy with `--all-targets`.
- Relay: `cd relay && uv run pytest`; `uv run ruff check && uv run ruff format --check`.
- GUI: `app/tests/gui/agent-*.spec.js` (Playwright spawns the real relay).
