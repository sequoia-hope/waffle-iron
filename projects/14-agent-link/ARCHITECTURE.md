# Sub-project 14: Agent Link — Architecture

Authoritative design: `specs/waffle_mcp_server.md`. This page is the map.

```
 agent (Claude Code / Desktop / any MCP client)
   │  MCP over stdio
 waffle-mcp-relay  (local Python process, --port chosen by the user)
   ▲  WebSocket, waffle-agent-link/1 — the PAGE dials out, after consent
───┼──────────────────────────── browser tab (main thread) ───
 agent link      app/src/lib/agent/link.js       module singleton; survives navigation
 agent host      app/src/lib/agent/executor.js   gates · engine lock · rollback · provenance
   ├─ queries → store state (tree, meshes, selection, errors)
   ├─ save    → storage/index.js active provider
   └─ commands → store (non-swallowing send) → EngineBridge → Worker
───┼──────────────────────────── Web Worker ───
 worker.js → wasm_bridge::dispatch → feature-engine → kernel-v2
```

- The relay relays; it never models (spec I1).
- The agent is another client of the same `UiToEngine` protocol the toolbar
  uses; the bridge and worker are unchanged except for the ICRs.
- The engine lock serializes whole agent calls against user actions (I6),
  because `bridge.js` pairs responses FIFO without request ids.
- GitHub Pages stays a static host; nothing of ours runs server-side.
