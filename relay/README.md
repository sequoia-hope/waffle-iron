# waffle-mcp-relay

The local relay of the Waffle Iron agent link (`specs/waffle_mcp_server.md`).
It is an MCP server on stdio for a local agent, and a WebSocket server for
exactly one paired Waffle Iron browser tab. It holds no modeling logic: every
page tool call is forwarded to the page, which runs it against its own engine.

Phase 0 (spike): connection tools `waffle_connect` / `waffle_status` and one
page tool, `model_summary`.

## Running

The relay has no default port. Pass one explicitly, set `$PORT`, or (on a
machine with the `proj` port registry) let `proj port` resolve it:

```
uvx waffle-mcp-relay==<version> --port <your port>
```

Development against the local dev server:

```
uv run --project relay waffle-mcp-relay --port <port> \
  --app-url http://localhost:<dev port>/ --allow-origin http://localhost:<dev port>
```

From another device on a tailnet, keep the relay on loopback and let
`tailscale serve` terminate TLS for both the app and the relay:

```
tailscale serve --bg --https=<https port> http://127.0.0.1:<dev port>
tailscale serve --bg --https=<https port> --set-path /relay http://127.0.0.1:<port>
uv run --project relay waffle-mcp-relay --port <port> \
  --app-url https://<host>.ts.net:<https port>/ \
  --public-url wss://<host>.ts.net:<https port>/relay
```

### Reconnecting and development links

A paired tab that loses its socket (a reload, a network drop, a phone
suspending a background tab) resumes the same session by itself for
`--resume-window` seconds (default 1800) — no new pairing link. Meanwhile
`waffle_status` is `page_away`, and a tool call waits up to 10 s for the tab
before returning `PageAway`.

For development, `--persistent-link [FILE]` makes the pairing link reusable
and non-expiring, so it can be bookmarked: the code is kept in FILE (default
`$XDG_STATE_HOME/waffle-mcp-relay/link-<port>.code`, mode 0600) and survives
relay restarts, and the relay logs the link to stderr when it starts. Opening
it still asks for consent, and it takes over from a tab that is already
connected. Anyone who has the link and can reach the relay from an allowed
origin can pair, so keep it private; delete the file to rotate the code.

## Host mode (no browser)

`--kernel host` runs the engine in a native `waffle-host` child process
instead of a paired browser tab (`specs/waffle_server_mode.md` §3.5). Every
engine tool (sketching, features, scripts, patterns, export, …) works with
no page open; documents live as `.waffle` files in `--documents DIR`
(default `$XDG_DATA_HOME/waffle-iron/documents`) and exports land in
`DIR/exports/`. The tools that need a page — the viewport, selection, and
for now the tab and assembly tools — are not listed in this mode.

```
cargo build -p waffle-host --release
uv run --project relay waffle-mcp-relay --port <port> --kernel host \
  --host-binary target/release/waffle-host --documents ~/waffle-docs
```

The binary is also found through `$WAFFLE_HOST_BIN` or as `waffle-host` on
PATH. A host that crashes (a kernel abort, out of memory) is restarted on
the next call and reopens the document from its own autosave; the call in
flight answers `EngineCrashed`.

## Runtime dependencies and licences

Three runtime dependencies (spec §7), checked 2026-09-14 from the installed
distributions' licence files:

- `mcp` (MCP Python SDK, 2.2.0 locked) — MIT
- `websockets` (17.1 locked) — BSD-3-Clause
- `jsonschema` (4.26.0 locked; already required by `mcp`) — MIT. Validates
  `tools/call` arguments against each tool's `inputSchema`.

## Development

```
cd relay
uv sync
uv run pytest
uv run ruff check && uv run ruff format --check
```

or, from the repository root, `./scripts/test.sh relay`.

The bundled tool manifest `src/waffle_mcp_relay/agent-tools.manifest.json` is
generated from `app/src/lib/agent/tools/` by `node app/scripts/gen-agent-manifest.mjs`;
a test fails if the committed copy is stale.
