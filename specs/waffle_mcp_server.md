# Waffle Iron MCP — Live-App Agent Link

Status: **rev 2 (live-app design)** — Phase 0 complete; Phase 1 in progress
(2026-09-14, `projects/14-agent-link/PLAN.md`). Rev 1 (a headless native Rust server editing files
under a root folder) is superseded. Why: an agent editing files the user
never sees is a dev/CI tool, not a product feature. §7 keeps the reasoning.

Sub-project: `projects/14-agent-link/`.
Components:

| Component | Location (proposed) | Language |
|---|---|---|
| **Agent host**: tool registry, executor, consent + activity UI | `app/src/lib/agent/`, route `app/src/routes/agent/` | JS / Svelte (runs in the page) |
| **Relay**: MCP server on stdio ↔ WebSocket to the paired page | `relay/` (a `pyproject.toml` package published to PyPI as `waffle-mcp-relay`, run via `uvx`) | Python ≥ 3.12 |
| **Bridge ICRs**: typed errors, measurement, face listing, provenance | `wasm-bridge`, `waffle-types`, `kernel-v2`, `feature-engine` | Rust |

Depends on `specs/waffle_v4_document_model.md` goal 5. That supplies the JSON
Schema golden, `x-` field preservation, the provenance table, §2.9 profile
addressing and opaque operations.

---

## 1. Goal

A local AI agent (Claude Code, Claude Desktop, any MCP client) works **in the
Waffle Iron page the user has open**. The user watches features appear in the
tree and bodies change in the viewport as the agent works. The user can select
a face and tell the agent "extrude this". The user can undo the agent's step
with Ctrl+Z, and can pause or disconnect the agent at any moment.

User-visible behavior:

1. The user adds the relay to their MCP client config (`uvx waffle-mcp-relay
   --port …`). The agent calls `waffle_connect`, which returns a **pairing
   link**. Opening it in the browser shows a consent screen naming the agent
   ("Allow *Claude Code* to edit documents in this tab?"). After **Allow**,
   an "Agent connected" bar stays visible with **Pause** and **Disconnect**.
2. The agent reads the open document: tabs, feature tree, rebuild errors,
   bodies and their measurements, parameters, and **the user's current
   selection**. It can also capture the viewport as an image.
3. The agent authors through the page's engine: sketches (solved in one
   call), extrude/revolve by profile entity ids, booleans, datum planes,
   edits, deletes, reorders, parameter changes. Every change goes through the
   same worker, the same rebuild, the same undo stack and the same autosave
   as a user action. Features it creates carry `Provenance{Agent{name}}` and
   show an agent badge in the feature tree.
4. A step whose feature fails to rebuild is **rolled back by default** and
   returns a structured error. Kernel capability boundaries (`NotSupported`,
   typed yang STOPs) surface verbatim and are never retried.
5. The agent never races the user. While the user is mid-interaction
   (sketching, a feature dialog open, a rebuild running), the agent's edits
   are refused with `UserBusy`. While the agent's call runs, the page's
   modeling tools are disabled; camera navigation stays live.
6. The agent saves through the page's active storage provider (this browser,
   or a connected git repository) exactly as the Save button does. It can
   receive STEP/STL exports as data.

Out of scope:
- **Fillet, chamfer, shell**: refused (§6 `Deferred`). They are deferred
  indefinitely project-wide; the agent link must not bypass the app's disabled
  Apply buttons.
- **Remote/cloud agents** reaching the page over the internet (a hosted relay).
  Only a relay the user runs is supported.
- **A second implementation of modeling semantics.** All tool logic runs in
  the page against the real store and worker (I1). The relay only relays.

---

## 2. Parameters

Units are as on the `.waffle` wire: lengths **meters**, angles **degrees**,
ids lowercase hyphenated UUIDs, sketch entity ids `u32`. The one exception is
inherited from the engine: parameter **expressions** (`depth_expr`, the
parameter table, `expression_evaluate`) are **mm-space**. Every tool
description states the space of each field. The display unit is UI state and
is not exposed.

### 2.1 Relay process

| Input | Type | Default | Valid | Error |
|---|---|---|---|---|
| `--port <n>` | u16 | none (see resolution) | 1024–65535 | exit 2 `invalid port` |
| port resolution | — | `--port`, then `$PORT`, then `proj port` | — | none resolves ⇒ exit 2 `no port: pass --port, set $PORT, or register with proj`. **No literal default, no self-picked free port.** |
| `--bind <addr>` | IP | `127.0.0.1` | loopback, or a non-loopback IP **with** TLS | non-loopback without `--tls-cert`/`--tls-key` ⇒ exit 2 `non-loopback bind requires TLS` |
| `--tls-cert`, `--tls-key` | paths | none | readable PEM pair | exit 2 `cannot read TLS material` |
| `--public-url <url>` | wss URL | none (the link carries `ws(s)://bind:port`) | `wss://host[:port][/path]`, no query or fragment; for a TLS proxy (e.g. `tailscale serve`) fronting a loopback relay | exit 2 `invalid public url` |
| `--app-url <url>` | https URL, or `http://localhost…` | `https://sequoia-hope.github.io/waffle-iron/` (the deployed app, `deploy.yml`) | absolute URL | exit 2 `invalid app url` |
| `--allow-origin <origin>` | repeatable | the origin of `--app-url` | exact origins, no wildcards | exit 2 `invalid origin` |
| `--agent-name <s>` | string | MCP `initialize.clientInfo.name`, else `"mcp-client"` | 1–128 chars, no control chars | exit 2 |
| `--open` | flag | off | — | `waffle_connect` also opens the pairing link with the OS URL handler |
| `--resume-window <s>` | integer seconds | `1800` | 0–604800 | exit 2 `invalid resume window` |
| `--persistent-link [FILE]` | flag, optional path | off; FILE defaults to `$XDG_STATE_HOME` (else `~/.local/state`)`/waffle-mcp-relay/link-<port>.code` | a file holding one 43-char base64url code, or absent (created, mode 0600) | exit 2 `invalid persistent link file` / `cannot use persistent link file`. **Development only** (P17, P18). |
| MCP protocol revision | — | newest revision the MCP Python SDK (`mcp`) supports | negotiated in `initialize` | per MCP lifecycle |

End users pick their own port and put it in their MCP client config
(`"command": "uvx", "args": ["waffle-mcp-relay==<version>", "--port",
"<their port>"]`); docs show the flag, never a suggested number, and always a
pinned version (an unpinned `uvx` fetches the newest release on every client
start, so one bad publish would reach every user at once). The hosted app URL is a product constant,
not a port. The dev server is used
by passing `--app-url http://localhost:<dev port>/` and `--allow-origin` to
match; the dev port comes from the registry as for any project.

### 2.2 Pairing and session

| Item | Value |
|---|---|
| Pairing link | `<app-url>agent?relay=<--public-url, else ws(s)://bind:port>&code=<pairing code>&name=<agent name>` |
| Pairing code | 32 random bytes, base64url; **single use**; expires **300 s** after `waffle_connect`. With `--persistent-link`: the code kept in the link file, **reusable, no expiry** (P17) |
| Consent | the `/agent` route shows agent name, relay address and the document that will be controlled. The connection opens **only on a user click** (this is also the user gesture a browser local-network permission prompt needs). |
| Session token | 32 random bytes, issued on successful pairing, kept in the page's `sessionStorage`; lets **the same browser tab** reconnect without re-consent — after a reload, a lost socket, or the OS suspending or discarding a backgrounded tab — for **`--resume-window` (1800 s)** after disconnect. The window is long because a mobile browser suspends a background tab for as long as the user is in another app. |
| Page reconnect | the page retries a lost session itself: at once when the tab becomes visible (`visibilitychange`, `pageshow`) or the network returns (`online`), else with backoff 1, 2, 4 … 30 s while visible; never while hidden. A visible tab whose socket looks open probes it with `ping` (no `pong` in 5 s ⇒ lost). A reloaded tab resumes only after its startup restore settles (the draft is reopened first), and says `reloaded: true`. |
| Paired pages | at most **one**. A second pairing attempt is refused while one is live (§3 P6). `waffle_connect` called again revokes the live session and issues a new code. |
| Link protocol | `waffle-agent-link/1` (JSON text frames, §2.3), versioned per A2.4 |
| Heartbeat | relay `ping` every 15 s; no `pong` within 30 s ⇒ disconnected |

### 2.3 Relay ↔ page frames

| Direction | Frame | Fields |
|---|---|---|
| page → relay | `hello` | `protocol: "waffle-agent-link/1"`, `code` or `session`, `app_build` (`__BUILD_INFO__`), `manifest_hash`, `reloaded?: bool` (a session resumed by a freshly loaded page, P7) |
| relay → page | `welcome` | `session`, `agent_name`, `protocol`, `manifest_required: bool` (true when `hello.manifest_hash` differs from the bundled manifest) |
| page → relay | `manifest` | `tools` — sent only after `manifest_required`; adopted only if its hash equals `hello.manifest_hash` |
| relay → page | `call` | `id`, `tool`, `arguments`, `progress: bool` |
| page → relay | `progress` | `id`, `message`, `elapsed_ms`, `progress?`, `total?` — sent only for a call whose `call.progress` was true (the client passed a `progressToken`); the relay forwards each as an MCP `notifications/progress`. Source today: rebuild progress of a `UnionAll` feature (`specs/b4_balanced_union.md` §2.3) |
| page → relay | `result` | `id`, `content[]`, `structuredContent`, `isError` |
| relay → page | `cancel` | `id` |
| page → relay | `status` | `state`: `ready` \| `paused` \| `busy{reason}`, `document_name?` |
| both | `ping` / `pong` | — |
| either | `bye` | `reason` |

### 2.4 Tool manifest

Tools are defined **once**, in `app/src/lib/agent/tools/` (name,
description, input schema, output schema, annotations). The build emits
`agent-tools.manifest.json`. The page reports its hash in `hello`, and the relay
package bundles the manifest of the app version it was published with.

- Clients see the bundled tool list at `tools/list` even before pairing, so
  clients without `tools/list_changed` support still work.
- On `hello`, a `manifest_hash` mismatch makes the relay adopt the page's
  manifest (sent in `welcome`'s reply, frame `manifest`) and emit
  `notifications/tools/list_changed`. A call to a tool the page no longer has
  returns `ToolUnavailable`.
- Input schemas for engine types (`Operation`, `SketchEntity`,
  `SketchConstraint`, `GeomRef`, `Parameter`, `TopoQuery`) are **`$ref`s into
  `docs/schema/waffle-v5.schema.json`**, the CI-pinned golden. No schema is
  hand-copied.

### 2.5 Tools

`on_error` ∈ `"rollback"` (default) | `"keep"`, on `sketch_create`,
`feature_add` and `feature_edit`. A command's result carries a `ModelDelta`:
`{features_added[], features_changed[], features_removed[], order_changed,
bodies_added[], bodies_removed[], errors[], warnings[]}`. A pure reorder changes
no feature record, so it is reported by `order_changed`.

No command takes a feature `name`: `AddFeature` and `FinishSketch` carry none,
and a follow-up `RenameFeature` would make the call two undo steps (I5). The
agent renames with `feature_rename`. ICR-5 (§9) would add the field.

**Connection** (answered by the relay itself; always available)

| Tool | Kind | Inputs | Result |
|---|---|---|---|
| `waffle_connect` | command | — | `{pairing_url, expires_at}` (`expires_at: null` for a persistent link); revokes any live or away session |
| `waffle_status` | query | — | `{state: "unpaired" \| "awaiting_consent" \| "page_away" \| "ready" \| "paused" \| "busy", busy_reason?, app_build?, document_name?}`; `page_away` = a session that can still resume (P16) |

**Documents and storage**

| Tool | Kind | Inputs (defaults) | Result |
|---|---|---|---|
| `document_info` | query | — | `document.id`, name, storage provider, tabs `{id,name,kind}`, active tab, read-only flag, sources with availability, unsaved flag |
| `storage_list` | query | `provider? (active)` | `{provider, documents: [{id, name, created, modified, tab_count, linked}]}` from `DocumentSummary` (`storage/types.js`) |
| `document_open` | command | `provider?`, `id`, `discard_unsaved (false)` | `DocumentInfo` (`openDocumentRecord`, the path the Home screen's `/doc/[id]` handoff takes) |
| `document_new` | command | `name ("Untitled")`, `discard_unsaved (false)` | `DocumentInfo` (the Home screen's `newDocumentRecord`, stored in the active provider, then opened) |
| `document_import` | command | `file_name`, `text` (the `.waffle` JSON), `name? (file name without extension)`, `discard_unsaved (false)` | `DocumentInfo` (the Home screen's file picker without the dialog: the file's own `document.id` keys the record — an existing record with that id is replaced — stored in the active provider, opened, named after the file) |
| `document_save` | command | — | `{provider, id, saved_at}` via `saveDocumentOrThrow`, the core `saveToStorage` shares |
| `tab_switch` | command | `tab_id` | `DocumentInfo` (Part tabs in Phase 1) |
| `tab_add` | command | `kind ("Part" \| "Assembly", "Part")`, `name?`, `activate (true)` | `{tab_id}` + `DocumentInfo` (the tab bar's + buttons, `addTab`) |
| `tab_move` | command | `tab_id`, `index` (0 = first; past the end = last) | `DocumentInfo` (the tab bar's drag-to-reorder, `moveTab`) |
| `tab_rename` | command | `tab_id`, `name` | `DocumentInfo` (`renameTab`) |

**Inspection**

| Tool | Kind | Inputs | Result |
|---|---|---|---|
| `model_summary` | query | — | features in tree order `{id,name,kind,suppressed,provenance,error?}`, rollback index, bodies `{body_id,name,feature_id}`, `errors[]`, `warnings[]`, parameters with values |
| `feature_get` | query | `feature_id` | `Operation` JSON, provenance, error |
| `selection_get` | query | — | the user's current selection: `[{geom_ref, kind, body_id, signature?, plane?}]`, plus the selected feature id. The viewport's datum-plane refs are not document `GeomRef`s, so a selected datum plane has `kind: "DatumPlane"` and `plane: {origin, normal}`, which `sketch_create` takes |
| `body_measure` | query | `body_id` | `{volume_m3, surface_area_m2, bbox_min, bbox_max, face_count, edge_count, vertex_count, closed, method}` (§2.6) |
| `face_list` | query | `body_id`, `filter?: TopoQuery` | `[{geom_ref, signature}]` in deterministic order |
| `sketch_regions` | query | `feature_id` | closed regions `{profile_entity_ids, area_m2}` |
| `expression_evaluate` | query | `expression` (mm-space) | `{value_mm}`, or `{value_mm: null, error}` for an expression that does not evaluate (a result, not `isError`) |
| `viewport_view` | query (camera only) | `view? (front \| back \| top \| bottom \| left \| right \| iso)`, `fit (true)`, `frame? {body_ids \| point + radius}` | `{view, fitted, framed, camera: {projection, position, target, up}}`; the View Cube's snap and the F key's Fit All, answered by `CameraControls` through the synchronous `waffle-agent-view` event. The named views are MODEL space, up = +Z (`front` an elevation along +Y, `iso` the front-right-top three-quarter). `frame` fits a REGION instead of everything — the union of some bodies' boxes, or a cube of half-size `radius` about a world point — and then `fit` is moot; `framed` echoes the box. A `body_ids` entry the view does not have is `BodyNotFound` with the camera unmoved; `point` without a positive `radius`, or both forms at once, is `InvalidArguments`. Model and undo history unchanged |
| `viewport_capture` | query | `max_edge_px (1024)` | PNG image content of the current view, composited over `--viewport-bg` (`AgentCapture.svelte`, `waffle-agent-capture`); the camera is not moved |

**Authoring**

| Tool | Kind | Inputs (defaults) | Result |
|---|---|---|---|
| `sketch_create` | command | `plane: GeomRef \| {origin, normal}`, each optionally with `x_axis`, `entities`, `constraints ([])`, `on_error` | `{feature_id, solve_status, dof, plane: {origin, normal, x_axis, y_axis}, regions[]}` + `ModelDelta`. `plane.x_axis` (2026-09-24) is the world direction the sketch's +x points along — its in-plane part, so any non-parallel vector does; zero-length or parallel to the normal is `InvalidSketch`, pre-commit. Without one the engine derives the in-plane axes, and the answer's `plane` says which it used either way, so a caller never reproduces that derivation (`docs/notes/eiffel/FEATURE_NOTES.md` §3) |
| `feature_add` | command | `operation`, `on_error` | `{feature_id}` + `ModelDelta` |
| `feature_edit` | command | `feature_id`, `operation`, `on_error` | `ModelDelta` |
| `feature_delete` / `feature_suppress` / `feature_reorder` / `feature_rename` / `body_rename` / `rollback_set` | command | as the bridge messages | `ModelDelta` |
| `parameters_set` | command | complete parameter table | `ModelDelta` + per-parameter errors |
| `undo` / `redo` | command | — | `ModelDelta` |
| `import_step` | command | `file_name`, `step_text`, `on_error` | `{feature_id}` + `ModelDelta`. Sends `ImportStep` (the engine records `Import` provenance) through the command path, so it is one undo step with A2 rollback; unlike `importStepFromText` it opens no placement dialog |
| `kicad_link` | command | `file_name`, `pcb_text`, `locator?`, `resolved_commit?`, `on_error` | `{source_id, board_tab, assembly_tab, placeholder_tabs, board, component_count}` + `ModelDelta` (2026-09-26, `specs/kicad_board_link.md` C4). Sends `ImportKicad` / `LinkKicadFromLocator`: a `KicadPcb` source, a Board Part tab (exact outline solid, Derived provenance), placeholder Parts, a Board assembly tab; opens the Board tab |
| `entity_meta` | query | `body_id?` \| `instance_path?` | `{board, component, source}` — each `null` for anything no KiCad board derived (`QueryEntityMeta`) |

**Custom feature scripts** (2026-09-23, A-M4 of
`specs/custom_features_and_modeling_roadmap.md`; reference
`docs/CUSTOM_FEATURE_SCRIPTS.md`). All run in the engine
(`crates/wasm-bridge/src/tools/script.rs`). A source add/update is NOT an
undo step (sources are assets, v4 §2.3); `script_source_update` rolls a
breaking edit back by re-setting the previous text.

| Tool | Kind | Inputs (defaults) | Result |
|---|---|---|---|
| `script_run_check` | query | `text` \| `source_id`, `entry ("feature")`, `args?` | `{ok, entry, interface?, error?{stage, reason}, dry_run?}` — header + compile + entry; with `args` a dry run (no kernel): recorded `children[]`, `logs[]`, `outputs[]` or the typed failure |
| `script_source_add` | command | `text` \| `library ("gear" \| "sprocket")`, `name?` | `{source_id, name, interface}` (+ the model update on the wire so the host's Sources panel refreshes); `InvalidScript` when the script does not check |
| `script_source_get` | query | `source_id?` | `{source_id, name, text, check, features[{feature_id, name}]}`; without an id `{scripts[], library[]}` |
| `script_source_update` | command | `source_id`, `text`, `on_error` | `ModelDelta` + `{source_id, interface}`; every node naming the source regenerates; a node the text newly breaks ⇒ `FeatureRebuildFailed{rolled_back}` (text restored) unless `keep` |
| `script_feature_add` | command | `source_id`, `entry`, `args?`, `arg_exprs?`, `on_error` | `{feature_id}` + `ModelDelta` — one `Script` node named by the script's `@feature name`; `feature_add`/`feature_edit` with a `Script` operation are equivalent |

**Export**

| Tool | Kind | Inputs (defaults) | Result |
|---|---|---|---|
| `export_step` | query (no model change; engine lock) | `deliver: "agent" \| "download" ("agent")` | `{deliver, file_name, mime_type, bytes, warnings[]}`; `agent` adds an embedded text resource `model/step` (`waffle://export/<file>`); `download`: the browser's normal download (`triggerStepDownload`) |
| `export_stl` | query | `body_id?`, `deliver ("agent")` | as above, `model/stl` as a base64 blob resource (`ExportBodyStl` for one body, `ExportStl` merged) |

**Assemblies** (2026-09-17). All run in the page over the store flows the
Assembly panel uses (`addInstance`, `addConnector`, `addMate` and their
edit/remove partners → one `EditAssembly` per call, which re-solves the open
tab). Every one needs an **Assembly tab active** (the inverse of G7:
`TabKindNotSupported{kind}` on a Part tab) and, for the commands, G3–G5. They
are document commands (§2.7): no whole-call lock, and NOT engine undo steps
— `undo`/`redo` act on a Part's feature tree; an assembly edit is reversed
with the matching delete/edit tool. Each returns the **assembly state**:
`{tab_id, name, instances[{id, name, source{tab_id, source_id}, part_name,
transform, fixed, suppressed, placement}], connectors[{id, name,
instance_path, part_connector, geom_ref, frame, anchor, flip_z, rotation_deg,
offset_m, world_frame{kind, origin, x_axis, y_axis, z_axis}}], mates[{id,
name, kind, connectors, suppressed}], part_connectors[], available_parts[],
errors[], warnings[]}` — placements and world frames from
`ModelUpdated.assembly` after the edit.

| Tool | Kind | Inputs (defaults) | Result |
|---|---|---|---|
| `assembly_get` | query (store state) | — | assembly state |
| `instance_add` | command | `tab_id`, `source_id?`, `name?`, `transform? {translation_m, rotation_quat \| rotation_euler_deg}`, `fixed (false)` | `{instance_id}` + state. A Part tab, another Assembly tab (sub-assembly) or a linked source's tab (`available_parts`); the open assembly itself is `TabNotFound` |
| `instance_edit` | command | `instance_id`, `name?`, `transform?` (given fields only), `fixed?`, `suppressed?` | state |
| `instance_delete` | command | `instance_id` | state (its connectors and their mates go too) |
| `connector_add` | command | `instance_path`, one of `part_connector` \| `geom_ref` \| `frame`, `name?` | `{connector_id}` + state. `geom_ref` is judged by `ProbeConnectorRef` first: `ConnectorRefused{reason}` when no frame derives. `part_connector` must be among the evaluated `part_connectors` of that instance, else `ConnectorNotFound` |
| `connector_edit` | command | `connector_id`, `name?`, `anchor?`, `flip_z?`, `rotation_deg?`, `offset_m?` | state (`updateConnector`; `offset_m` is stored as given, no mm round trip) |
| `connector_delete` | command | `connector_id` | state (mates using it go too) |
| `mate_add` | command | `a`, `b`, `kind (Fastened)`, `flip (true)`, `rotation_deg (0)`, `name?` | `{mate_id}` + state; `a === b` is `InvalidArguments` |
| `mate_edit` | command | `mate_id`, `name?`, `kind?`, `flip?`, `rotation_deg?`, `suppressed?` | state |
| `mate_delete` | command | `mate_id` | state |

`selection_get` gains `instance_path` (the clicked instance in an open
assembly, else `null`): the picked face or edge is in the part's space and is
what `connector_add` takes as `geom_ref`. `tab_switch` accepts Assembly tabs
(the switch evaluates the tab, as the tab bar's does).

### 2.6 Measurement method

`body_measure.method` is `"exact"` (B-Rep volume/area through ICR-1) or
`"mesh"` (the render mesh's signed volume/area, computed in the page over the
`meshes` the store already holds). It is always reported. `"mesh"` exists only
until ICR-1 lands. It underestimates curved volumes and must never be labelled
exact.

### 2.7 Execution model in the page

- **One engine lock.** Add a store-level async mutex (`withEngineLock`). Every
  store action that sends a rebuilding message takes it (`sendRebuild` and the
  sketch Begin/Finish paths), and the agent executor holds it for the
  **whole** tool call, including multi-message sequences and rollback. The
  bridge pairs responses FIFO (`bridge.js` `_pendingCallbacks`), so ordering
  is safe message by message. The lock makes it safe **call by call**.
- **Busy states** (mutating tools refused with `UserBusy{reason}`; queries
  allowed): `sketch_mode` (`getSketchMode().active`), `feature_dialog` (any
  extrude/revolve/boolean/import/datum dialog state open), `edit_context`,
  `rebuilding` (lock held by a user action), `engine_not_ready`,
  `engine_crashed`.
- **Paused**: the user pressed Pause. Mutating tools are refused with
  `AgentPaused`; queries allowed.
- **Document tools** (`document_open`, `document_new`, `document_save`,
  `tab_switch`) run the store's own multi-message flows, which send through the
  gated user path. They set the agent activity (UI refused, G8) and pass G3/G4,
  but do not hold the lock for the whole call: a nested gated send would wait
  on the call's own lock.
- **During an agent call** the page disables modeling commands (toolbar,
  feature tree edits, keyboard shortcuts that mutate) and shows the running
  tool name in the agent bar. Selection, hover and camera stay live.
- **The agent layer never uses the swallowing wrappers.** `undo()`, the
  `apply*` dialog actions and `setParameters` catch errors and toast. The
  executor sends bridge messages through a new store entry point that
  resolves with the `EngineToUi` response and rejects with the typed error.
  It reuses the same `modelUpdated` handler, so tree, meshes, autosave and
  toasts update exactly as for user actions.
- **Toasts** for agent-caused rebuild errors are shown (the user should see
  them) and prefixed with the agent name.

---

## 3. Branch table

### 3.1 Relay and pairing

| # | Situation | Behavior |
|---|---|---|
| P1 | `waffle_connect`, no live session | new code, `pairing_url`; with `--open`, the OS opens it |
| P2 | page opens `/agent` link, user clicks **Allow**, `Origin` allowed, code valid & unexpired | WebSocket opens, `hello{code}` → `welcome{session}`; `waffle_status = ready`; the agent bar appears |
| P3 | user clicks **Deny** or closes the tab | no connection; `waffle_status` stays `awaiting_consent` until the code expires, then `unpaired` |
| P4 | handshake `Origin` not in the allow list | HTTP 403 on upgrade; nothing else read; logged to stderr |
| P5 | code wrong, reused or expired | `bye{reason:"invalid_code"}`, close; the page shows "link expired, ask the agent to reconnect" |
| P6 | second page tries to pair while a session is live | `bye{reason:"already_paired"}` |
| P7 | same tab reconnects within the resume window, `hello{session}` (lost socket, suspended or discarded tab, reload) | resumes without consent; in-flight calls from before the drop already returned `PageDisconnected` (P9). With `hello.reloaded`, the next page tool result gets a trailing text note that the tab reloaded and reopened its draft (call `model_summary`). Before the resume the page reopens **the document the agent was working on** (its storage id is kept with the session; newest draft holding it, else the provider, else the local store), whatever the tab's `restoreOnReload` policy left open — a `never` policy or a discarded offer otherwise leaves the agent on the blank startup document (2026-09-23) |
| P8 | a session presented after the resume window, or after a revoke | `bye{reason:"session_expired"}`. The relay cannot tell a reload from another tab holding the same token (a duplicated tab copies `sessionStorage`). Within the window the session resumes; while a page is live, a second presenter gets `already_paired` (P6). |
| P8a | first frame is not `hello`, or no frame within 10 s | WebSocket close 1008, no `bye` (no §3.1 reason applies) |
| P9 | page disconnects (close, crash, network) with calls in flight | each in-flight call returns `isError`, `PageDisconnected`; the model state after the call is **unknown** and the error says so (the agent must `model_summary`) |
| P10 | browser blocks the socket (local-network permission denied, mixed content, Safari policy) | the page's `/agent` route shows the browser's error class and the documented fallback (§6.3); relay stays `awaiting_consent` |
| P11 | `waffle_connect` while a session is live | `bye{reason:"revoked"}` to the page; new code |
| P12 | user clicks **Disconnect** | `bye{reason:"user_disconnected"}`; session revoked (no resume) |
| P13 | protocol version in `hello` ≠ relay's | `bye{reason:"protocol_mismatch", supported}`; page shows update guidance |
| P14 | manifest hash mismatch | relay adopts the page manifest, emits `tools/list_changed` (§2.4) |
| P15 | any page tool called while `unpaired` | `isError`, `NotPaired` with the hint to call `waffle_connect` |
| P16 | any page tool called while `page_away` | waits up to **10 s** for the page to resume, then runs; else `isError`, `PageAway`: ask the user to return to the tab, and do **not** call `waffle_connect` (it would revoke the session) |
| P17 | `--persistent-link`: `waffle_connect`, or the page presents the persistent code | the same link every time, `expires_at: null`; the code admits a page (after the consent click, I7) any number of times. The relay logs the link to stderr at startup. |
| P18 | `--persistent-link`: the code is presented while another page is live | the new page is admitted; the old page gets `bye{reason:"revoked"}` and its session ends (the one-page rule holds) |

### 3.2 Page state gates (every mutating tool)

| # | State | Behavior |
|---|---|---|
| G1 | ready, lock free | lock taken; call runs |
| G2 | lock held by a user action | wait up to **10 s** for release, then `UserBusy{reason:"rebuilding"}` |
| G3 | sketch mode / dialog / edit context open | `UserBusy{reason}` immediately; nothing sent |
| G4 | paused | `AgentPaused` |
| G5 | document linked read-only (`isDocumentReadOnly()`) | `DocumentReadOnly` (fork is a user decision) |
| G6 | engine not ready / crashed (`needsRestart`) | `EngineNotReady` / `EngineCrashed` |
| G7 | active tab is not a Part (feature tools); active tab is not an Assembly (assembly tools, §2.5) | `TabKindNotSupported{kind}` |
| G8 | user presses a modeling shortcut during an agent call | ignored with a status-bar hint "Agent is working"; nothing queued |

Queries run in G1–G8 except G6. Those that read store state do not take the
lock. Those that send a bridge message (`body_measure`, `face_list`,
`sketch_regions`, `expression_evaluate`) run under the lock, because I6 also
forbids agent messages during a user action, and are refused in G2 after the
same 10 s. Another call of the same agent holding the lock is not a G2 case:
the call queues behind it.

### 3.3 Authoring

| # | Tool | Situation | `on_error` | Behavior |
|---|---|---|---|---|
| A1 | `feature_add` | Sketch/Extrude/Revolve/BooleanCombine/DatumPlane rebuilds clean | any | added with `Agent{name}` provenance (ICR-4); `ModelDelta` |
| A2 | `feature_add`/`feature_edit` | the target or any downstream feature newly errors | `rollback` | `Undo` under the same lock; `isError`, `FeatureRebuildFailed{feature_id, engine_error}`; document canonical bytes identical to before (I3); the user sees one toast "Agent step rolled back: …" |
| A3 | same | same | `keep` | change stays; result not `isError`; `kept_with_error: true`; `errors` lists every newly erroring feature |
| A4 | `feature_add`/`feature_edit` | kernel `NotSupported` or typed yang STOP | `rollback` / `keep` | A2 / A3 with `code` `NotSupported` / `KernelStop` and the kernel message verbatim |
| A5 | `feature_add`/`feature_edit` | Fillet/Chamfer/Shell | — | `Deferred{operation}`; **not sent** |
| A6 | `feature_add`/`feature_edit` | ImportedBody | — | `UseImportTool` (Phase 2 `import_step`) |
| A7 | `feature_add`/`feature_edit` | JSON not a known `Operation` variant | — | `InvalidOperation{schema_path, reason}`; the agent never authors opaque ops |
| A8 | `feature_edit` | kind differs from the feature's | — | `OperationKindMismatch` |
| A9 | `feature_edit` | `Derived` or `Import` provenance | — | `DerivedFeatureReadOnly` / `UseImportTool` |
| A10 | `sketch_create` | solves Fully/Under-constrained | any | `BeginSketch` → `SolveSketch{entities, constraints}` → `FinishSketch` under one lock; the user's sketch-mode UI is **not** entered; `dof` and regions returned |
| A11 | `sketch_create` | Over-constrained / SolveFailed | `rollback` | engine sketch discarded, tree unchanged; `SketchSolveFailed{status, conflicts}` |
| A12 | `sketch_create` | same | `keep` | committed with its failed status (as the app's Finish would) |
| A13 | `sketch_create` | undefined/duplicate ids, constraint naming a missing entity | — | `InvalidSketch{reason}`; nothing sent |
| A14 | any | `feature_id` / `body_id` not found | — | `FeatureNotFound` / `BodyNotFound` |
| A15 | `feature_delete` | dependents exist | — | deleted as in the app; dependents' errors in `ModelDelta.errors`; not `isError` |
| A16 | `undo` / `redo` | nothing to undo/redo | — | `NothingToUndo` / `NothingToRedo` (the store's wrapper swallows these; the agent entry point must not) |
| A17 | `parameters_set` | an expression fails | — | table set; per-parameter errors; features driven by it error in `errors` |
| A18 | any command | the call is cancelled (`notifications/cancelled`) | — | the call runs to completion under the lock (a kernel op cannot be interrupted), then is undone as A2; the result is discarded |

Provenance on edit: `feature_edit` of a `User` feature sets `Agent{name}`
(last author).

Rollback (A2, A4, A18) is an engine `Undo`, so the undone step moves to the
redo stack: a user's redo re-applies it. I3 covers the document and the undo
depth, not the redo stack; dropping the redo entry would need an engine message
of its own.

### 3.4 Queries, storage, export

| # | Situation | Behavior |
|---|---|---|
| Q1 | `selection_get`, nothing selected | `[]`, not an error |
| Q2 | `body_measure`, ICR-1 landed / not landed | `method: "exact"` / `"mesh"` |
| Q3 | `face_list` filter matches nothing | `[]` |
| Q4 | `viewport_capture` while the viewport is hidden (mobile panel over it, tab backgrounded) | `ViewportUnavailable`; no image of a stale frame |
| Q5 | `export_*` with no bodies | `NothingToExport` |
| Q6 | `export_*` `deliver:"agent"`, payload > **16 MiB** | `PayloadTooLarge{bytes}`; suggest `deliver:"download"` |
| Q7 | `export_step` omits content | exported; `warnings` verbatim |
| S1 | `document_save`, editable document | `saveToStorage` path; provider error (git auth, conflict) returned as `SaveFailed{provider, reason}` verbatim |
| S2 | `document_save`, linked read-only | `DocumentReadOnly` |
| S3 | `document_open` / `document_new` with unsaved changes | `UnsavedChanges` unless `discard_unsaved: true`; with the flag, the user still sees the page's normal confirm prompt, and declining returns `UserDeclined`. The app autosaves 3 s after each USER change, so "unsaved" means an autosave of the user's own edit is still pending; `document_save` supersedes it. **An agent's mutating tool stores its edit (this tab's draft and the storage record) before it answers** — as the native host rewrites its record after every mutating tool (`waffle_server_mode.md` §3.5) — so a tab the OS discards a moment later loses nothing that was answered; a store failure rides on the answer as `autosave_error` + a note, never swallowed (2026-09-23). `document_save` refuses an EMPTY document (no features, instances or sources: the blank startup document a reloaded tab shows) as `EmptyDocument` unless `allow_empty: true` |
| S4 | `document_open`, id not in provider | `DocumentNotFound` |

---

## 4. Invariants

- **I1 — One implementation.** Every tool that changes the model does so by
  sending `UiToEngine` messages through the page's bridge. The relay contains
  no modeling logic, no engine, and no schema for engine types beyond the
  bundled manifest. For any agent call sequence, the saved document equals
  the one produced by the same message sequence sent directly through the
  store in the same page. Equality is on canonical bytes: UUIDs renamed by
  first appearance, `Provenance.at` and envelope timestamps stripped.
- **I2 — Single writer.** Agent saves use `saveToStorage` → `SaveDocument`
  (v4 §4 inv. 7).
- **I3 — Exact rollback.** After A2/A4/A11/A18, `buildDocumentJson()` output
  and the engine undo depth equal their pre-call values.
- **I4 — Provenance.** Every feature added or edited by an agent call has
  `origin = Agent{name}` with the paired agent's name. Features the call did
  not touch keep their provenance record bit-for-bit.
- **I5 — One call, one undo step.** After a successful agent command, a single
  user Ctrl+Z (or `undo`) restores the pre-call document, and one redo
  restores the post-call document. This includes `sketch_create`.
- **I6 — No interleaving.** Between the first and last bridge message of an
  agent call, no bridge message originating from a user action is sent.
  Pointer feedback (`HoverEntity`, `SelectEntity`) is exempt: it touches no
  model state, and FIFO pairing keeps its answers apart.
  Conversely, while a user action holds the lock, no agent message is sent.
- **I7 — Consent.** No agent frame is processed by a page until a user click
  on that tab's consent screen, or a same-tab session resume within 120 s
  (P7). A page never auto-connects from a URL alone.
- **I8 — Origin and code.** The relay processes frames only on connections
  whose handshake `Origin` is allowed and that presented a valid unexpired
  single-use code or a live session token.
- **I9 — Loopback by default.** With no `--bind`, the relay's listening socket
  is bound to `127.0.0.1` only. A non-loopback bind exists only with TLS.
- **I10 — Nothing silent.** Every refusal is `isError: true` with
  `structuredContent.error = {code, message, details}`, from the closed set
  of §6. Every engine error, rebuild error, warning and exporter warning
  reaches the result unmodified. No tool retries with altered parameters.
- **I11 — Deferred unreachable.** No agent call causes the worker to receive a
  Fillet, Chamfer or Shell `Operation`.
- **I12 — User control.** Pause takes effect before the next mutating call is
  admitted. Disconnect closes the socket within 1 s. Neither can leave a
  half-applied call: an in-flight call completes or rolls back under the lock
  first.
- **I13 — stdout is protocol only** (relay). Logs go to stderr.
- **I14 — Determinism** (A4.2). `model_summary`, `face_list` and
  `body_measure` return identical structured content for identical canonical
  documents. Lists are ordered by tree order, then `GeomRef` canonical JSON.

---

## 5. Oracles

**Harnesses.**
- (a) `relay/tests/`: pytest (asyncio), with a fake MCP client on stdio and a
  fake page WebSocket client. No browser. Added to `./scripts/test.sh` beside
  the GUI tiers.
- (b) `app/tests/gui/agent-*.spec.js`: Playwright spawns the **real relay** and
  drives it as an MCP client over stdio; the real page pairs by clicking Allow.
  The relay's port comes from `$AGENT_RELAY_PORT` when set (not `$PORT`,
  which may already belong to the dev server). Otherwise the test fixture
  asks the OS for a free port and passes it explicitly with `--port`. This is
  a fixture allocation; the relay itself never picks a port.
- (c) A manual browser matrix, recorded in the spec at Phase 0.

| Oracle | Branches | Harness | Mechanism | Bound |
|---|---|---|---|---|
| O1 Box volume | A1, A10, Q2 | b | `sketch_create` 20×10 mm rectangle on XY → `feature_add` Extrude `profile_entity_ids`, depth 0.005 | exact: `volume_m3 = 1.0e-6 ± 1e-15`; bbox sorted extents `[0.005, 0.01, 0.02] ± 1e-7` with z ∈ `[0, 0.005]` (a plane given only by origin + normal leaves the in-plane axes to the engine: measured 2026-09-14, sketch u → world −y; whether the built-in XY datum maps u → +x is unverified, so O1 asserts extents, not coordinates); 6/12/8 faces/edges/vertices; `closed`; `__waffle.getMeshBoundingBox()` agrees ± 1e-6 |
| O2 Method honesty | Q2 | b | cylinder r=5 mm h=10 mm | exact: `|V−πr²h| ≤ 1e-12`; mesh: `method="mesh"` and `V < πr²h` |
| O3 Parity | I1 | b | 15 scripted sequences via agent vs the same messages via the store entry point in a fresh page | canonical bytes equal |
| O4 Rollback | A2, A4, A11, I3 | b | `profile_entity_ids` naming no loop; an edit breaking a downstream extrude; a typed `NotSupported`: an arc-segment profile committed without its `vertex_ids` polygon (kernel-v2's wall; F0064 builds without errors in the app, measured 2026-09-14); over-constrained sketch | pre/post `buildDocumentJson()` canonical-equal; undo depth equal; one toast |
| O5 Keep | A3, A12 | b | same, `on_error:"keep"` | feature present, `kept_with_error`, id in `errors`, `isError=false` |
| O6 Undo granularity | I5 | b | after `sketch_create` + `feature_add`, press **Ctrl+Z** with a real keyboard event | document equals post-sketch bytes; again ⇒ pre-call bytes |
| O7 No interleaving | I6, G2, G8 | b | spy wraps `EngineBridge.send`, tagging origin; during an F0065-class slow boolean agent call, click Extrude and press shortcuts | spy log contains no user-tagged send between the agent call's first and last send |
| O8 Busy gates | G3, G5, G7 | b | enter sketch mode with **real pointer events**, then `feature_add` | `UserBusy{sketch_mode}`; spy shows 0 sends; tree unchanged. Same for an open extrude dialog, a linked read-only document, an Assembly tab |
| O9 Pause/Disconnect | G4, I12, P12 | b | Pause mid-call | in-flight call completes; next command `AgentPaused` within one call; Disconnect ⇒ relay sees close ≤ 1 s; `waffle_status = unpaired` |
| O10 Provenance | I4 | b | after O1 | added features `Agent{name}` = the MCP `clientInfo.name` the test sent; the tree shows the agent badge; a pre-existing user feature keeps `User` |
| O11 Deferred | A5, I11 | b | Fillet/Chamfer/Shell via `feature_add` | `Deferred`; spy 0 sends |
| O12 Selection | Q1 | b | user clicks a face (real pointer event), agent `selection_get`, then `sketch_create` on that `geom_ref` | returned ref equals `__waffle` selected ref; sketch plane normal equals the face normal ± 1e-9 |
| O13 Origin/code security | P4, P5, P6, P8, I8 | a | handshake with `Origin: https://evil.example`; no Origin; reused code; expired code (fake clock); second client; wrong-tab session | 403 / `bye` reasons as tabled; zero `call` frames delivered to any unauthenticated socket |
| O14 Consent | I7 | b | navigate to a valid pairing link and do not click | after 10 s: no WebSocket in the page's network log (`page.on('websocket')` count 0) |
| O15 Bind | I9, §2.1 | a | start relay without `--bind`; inspect listening sockets. `--bind 0.0.0.0` without TLS | loopback only; exit code 2 with the message |
| O16 Port resolution | §2.1 | a | no `--port`, no `$PORT`, `proj` absent from PATH | exit 2 with the exact message; no socket opened |
| O17 Disconnect mid-call | P9 | a | fake page drops during a call | `PageDisconnected`, `details.state_unknown = true` |
| O18 Manifest | P14, §2.4 | a | fake page sends a different hash + manifest | `notifications/tools/list_changed` emitted once; `tools/list` equals the page's manifest |
| O19 Schema agreement | §2.4 | a+CI | every engine-type `$ref` in the manifest resolves in `docs/schema/waffle-v5.schema.json`; manifest regenerated at build equals the committed one | 0 unresolved refs; byte-equal |
| O20 Protocol hygiene | I13 | a | capture stdout over a full O1-equivalent fake session; unknown method; malformed `tools/call` | every stdout line is JSON-RPC; `-32601` / `-32602` |
| O21 Export | Q5–Q7 | b | O1 → `export_step{deliver:"agent"}`; re-import the text with `importStepFromText` in a fresh page | re-imported volume within 1e-9 of O1 |
| O22 Capture | Q4 | b | `viewport_capture` after O1; again with the tab hidden (`page.evaluate` visibility override) | PNG decodes, max edge ≤ 1024, not uniform-color; hidden ⇒ `ViewportUnavailable` |
| O23 Browser matrix | P10 | c | hosted https app → `ws://127.0.0.1` relay on Chrome, Edge, Firefox, Safari (macOS); dev server `http://localhost` on each | recorded pass/prompt/block per cell before Phase 1 starts |

---

## 6. Failure modes

### 6.1 Error codes (closed set, I10)

JSON-RPC protocol errors (not tool results): malformed frames, unknown
method, unknown tool, arguments failing `inputSchema` (`-32602` with the JSON
pointer).

Tool results with `isError: true`:

| Code | Raised by |
|---|---|
| `NotPaired` | P15 |
| `PageDisconnected` | P9 (`details.state_unknown`) |
| `PageAway` | P16 (the session can still resume; do not re-pair) |
| `ToolUnavailable` | §2.4 |
| `UserBusy` | G2, G3 (`reason`) |
| `AgentPaused` | G4 |
| `DocumentReadOnly` | G5, S2 |
| `EngineNotReady` / `EngineCrashed` | G6 |
| `TabKindNotSupported` | G7 |
| `Deferred` | A5 |
| `UseImportTool` | A6, A9 |
| `InvalidOperation` | A7 |
| `OperationKindMismatch` | A8 |
| `DerivedFeatureReadOnly` | A9 |
| `InvalidSketch` | A13 |
| `InvalidScript` / `SourceNotFound` | script tools: a script that does not check (`details.stage`, `reason`), a malformed script argument; an id that is not a Script source of the document |
| `SketchSolveFailed` | A11 |
| `FeatureNotFound` / `BodyNotFound` | A14 |
| `FeatureRebuildFailed` | A2 (`feature_id`, `engine_error`) |
| `NotSupported` / `KernelStop` | A4 (verbatim) |
| `NothingToUndo` / `NothingToRedo` | A16 |
| `ViewportUnavailable` | Q4 |
| `NothingToExport` / `PayloadTooLarge` | Q5, Q6 |
| `SaveFailed` | S1 |
| `UnsavedChanges` / `UserDeclined` | S3 |
| `DocumentNotFound` | S4 |
| `InvalidDocument` / `FormatTooNew` | `document_import` with text that is not a `.waffle` document (`file_name`, `reason`) or one that needs a newer reader (`file_version`, `supported_version`) |
| `ProviderNotFound` | `storage_list` / `document_open` naming a storage provider this tab has not connected |
| `StorageFailed` | a provider's list/get failed or the engine did not load the record (`provider`, `reason` verbatim) |
| `TabNotFound` | `tab_switch`, `tab_move` or `tab_rename` naming an id the document does not have; `instance_add` naming a tab that is not a placeable part (or the open assembly itself) |
| `InstanceNotFound` / `ConnectorNotFound` / `MateNotFound` | an assembly tool naming an id the open assembly does not have (`ConnectorNotFound` also for a `part_connector` the instance's part did not evaluate) |
| `ConnectorRefused` | `connector_add` with a `geom_ref` the engine cannot derive a frame from (`reason` verbatim from `ProbeConnectorRef`) |
| `AssemblyEditFailed` | the engine refused the `EditAssembly` (`reason` verbatim) |
| `InvalidArguments` | an argument combination the schema cannot express: both `rotation_quat` and `rotation_euler_deg`, a zero quaternion, several of `part_connector`/`geom_ref`/`frame`, `mate_add` with `a === b` |
| `Internal` | the executor detects a broken invariant (rollback not byte-exact; `ModelDelta` inconsistent). The agent session is then **paused** automatically, and the bar tells the user why. |

### 6.2 Structured-error gap

`EngineToUi::Error` and `ModelUpdated.errors` are strings today. Typing them
by parsing message text is the fragile layer this project refuses to build.
Until **ICR-2** lands, rebuild failures return `FeatureRebuildFailed` with
`engine_error = {message}` only, and O4's `NotSupported` row stays
`test.skip`-quarantined with reason `ICR-2`.

### 6.3 Browser connectivity

A public https page opening a WebSocket to a loopback address depends on
browser policy. Known classes (to be confirmed by O23, not assumed):
- A **local-network access permission prompt** (Chromium's Private/Local
  Network Access work).
- **Mixed-content** rules: `ws://` to `127.0.0.1`/`localhost` is treated as
  potentially trustworthy in Chromium and Firefox; Safari has historically
  been stricter.

**Measured 2026-09-14 (headless Chromium, Playwright chromium-1228):**

| Page origin | Permission | Result |
|---|---|---|
| `http://localhost` (dev server) | default | socket opens, `welcome` received |
| `https://sequoia-hope.github.io` (hosted) | default | blocked: `net::ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS` |
| `https://sequoia-hope.github.io` (hosted) | `local-network-access` granted | socket opens, `welcome` received, relay `ready` |

So on Chromium the hosted path depends on the user granting Chrome's
local-network-access permission, and a denial is the normal failure mode.
The page can read the permission: `navigator.permissions.query({name:
"local-network-access"})` answers `prompt` by default and `granted` after a
grant (Chromium 149, both headless; the names `local-network` and
`loopback-network` answer identically). A failed socket carries no reason,
so this query is how the `/agent` route tells a local-network block from a
generic failure.
Firefox (headed, user-reported 2026-09-14): pairing and `model_summary`
work end to end; the origin used and prompt behavior were not recorded.
Still unmeasured: headed Chrome (whether the prompt appears for a WebSocket
opened from the Allow click), Edge and Safari.

When the socket fails, the `/agent` route shows the class it can detect (a
permission denial, or a generic failure) and the documented fallbacks:
1. Run the app from the dev server on `localhost`.
2. Run the relay with `--bind` + TLS on a host name the browser trusts (e.g.
   a Tailscale `*.ts.net` certificate), or keep it on loopback behind a TLS
   proxy and advertise the proxy with `--public-url`.

Fallback 2 is also the configuration for **agent and browser on different
machines**, such as this repo's Docker + Tailscale dev container with the
browser on another device. A loopback relay is unreachable from another
machine by construction.

### 6.4 Degenerate geometry

No geometric pre-validation in the agent layer: zero depths, zero radii and
null normals go to the engine unchanged, and its typed error is A2/A3. No
tolerance lives outside `waffle_types` (A3.3, A8.1). A13's checks are shape
checks on ids, not geometry. A worker crash (`needsRestart`) during a call
returns `EngineCrashed` and pauses the session.

---

## 7. Research basis

- **Model Context Protocol specification** (modelcontextprotocol.io):
  lifecycle, tools (`inputSchema`, `outputSchema`, `structuredContent`,
  `isError`, annotations), `tools/list_changed`, progress, cancellation,
  stdio transport, embedded resources and image content. Pin the revision at
  implementation time, verified against the published spec.
- **MCP Python SDK** (`mcp`) for the relay's stdio server; **`websockets`**
  (asyncio) for the WebSocket server, using its handshake `origins=`
  allow-list for I8 and the standard library `ssl` module for the TLS bind.
  Runtime dependencies are limited to these two plus **`jsonschema`** (MIT),
  which `mcp` already requires; the relay imports it directly to validate
  `tools/call` arguments (§6.1). Licences are checked in `relay/README.md`.
- **RFC 6455** (WebSocket), §10.2 origin considerations, and the
  **cross-site WebSocket hijacking** attack class. Any web page can attempt a
  connection to a loopback port, which is why I8 needs origin checking plus a
  single-use code, and I7 needs user consent in the page.
- **W3C Secure Contexts** ("potentially trustworthy" loopback origins) and
  Chromium's **Private/Local Network Access** work: the browser-policy
  surface O23 measures. Status changes between browser releases; the spec
  records measurements, not assumptions.
- **`specs/waffle_v4_document_model.md`** goal 5, §2.7 (provenance), §2.9
  (profile addressing), §4 inv. 7 (single writer).
- **Governance:** A2.1/A2.2 (engine truth stays in the worker; the agent is
  another UI client, not a second engine), A2.4 (versioned link protocol),
  A6.1 (tool annotations follow command/query), A6.2 (ICR-2), P9/P10 (loud
  rollback, verbatim kernel errors, no retries).
- `REFERENCES.md` has no entry for any of these; add them in the first
  implementation commit.

**Alternatives considered.**
- *Headless native server* (rev 1): a second host of the engine, invisible to
  the user. It is a dev tool, and its tool semantics would drift from the
  page's. Headless batch use is served instead by running the real page under
  Playwright (Phase 3).
- *Browser-automation MCP servers* (Playwright MCP, Chrome DevTools MCP)
  against `window.__waffle`. Useful for a throwaway exploration of the tool
  set, but `__waffle` is a test API and automation is not a contract.
- *WebMCP* (pages registering tools with the browser). The right shape and
  the likely long-term target, but not yet broadly available. The tool
  registry of §2.4 is designed to be re-exposed through it without changes.
- *Hosted cloud relay*: needed only for remote agents; brings accounts and a
  hosted service. Out of scope.

### 7a. Analytical vs. approximate

The agent link performs no surface–surface intersection; booleans run the
kernel's pipeline (A15). The only approximate computation it owns is
`body_measure` `method:"mesh"`. That is declared **temporary** pending ICR-1,
always labelled, and never asserted as exact by any oracle.

---

## 8. Phases

| Phase | Content | Exit |
|---|---|---|
| **0 — Spike + ICRs** | Minimal relay + `/agent` route + one tool (`model_summary`); run O23 and record the matrix here; land ICR-1…ICR-4 with their own tests | matrix recorded; a go/no-go note on the default connection path; ICRs merged |
| **1 — Live authoring** | Pairing, consent, agent bar (Pause/Disconnect), engine lock + busy gates, executor with rollback and provenance, tools of §2.5 except `viewport_capture` and export; agent badge in the feature tree | O1–O20 green (O4 `NotSupported` row after ICR-2); `sketch-drawing-regression.spec.js` still green (the lock touches sketch paths) |
| **2 — Collaboration** | `viewport_capture`, `export_*`, `import_step` (STEP text from the agent → `importStepFromText`, `Import` provenance), assembly authoring (`assembly_get`, `instance_*`, `connector_*`, `mate_*` over `EditAssembly` — landed 2026-09-17, `agent-assembly.spec.js`), parameters as MCP resources with subscriptions | O21–O22 green |
| **3 — Headless and remote** | `--headless-app`: the relay launches the app in a headless browser (Playwright for Python) for CI/batch agents, using the same tools and the same page code; TLS bind hardening for the cross-machine case; WebMCP exposure of the same registry if browsers ship it | its own oracle addendum |

---

## 9. Interface change requests

All additive and serde-defaulted: no `MIN_READER_VERSION` bump and no
breaking bridge change (A2.4). Each lands in its owning sub-project first.

- **ICR-1 — exact measurement** (`waffle-types`, `kernel-v2`, `wasm-bridge`).
  Add `KernelIntrospect::solid_volume` and `solid_surface_area`, implemented
  in kernel-v2 by the existing `geom::signed_volume` /
  `introspect::surface_area`. Add query `UiToEngine::MeasureBody{body_id}` →
  `EngineToUi::BodyMeasured{…}`.
  **LANDED 2026-09-14.** Each quantity is a
  `Measured{value, method: exact|mesh, exact_unavailable}`, so §2.6's `method`
  comes from the bridge per quantity. The bounding box is from the render
  mesh.
- **ICR-2 — typed errors** (`wasm-bridge`, `feature-engine`). Add
  `EngineToUi::Error.kind` and `ModelUpdated.feature_errors: [{feature_id,
  kind, message}]` beside the existing string fields. `kind` mirrors
  `EngineError`, `KernelError::NotSupported` and the yang STOP variants. The
  app's toasts can adopt `kind` independently.
  **LANDED 2026-09-14** as `feature_engine::types::ErrorKind` /
  `FeatureError`. `Error.kind` is set for engine errors only; bridge-level
  failures leave it absent. One gap remains: kernel-v2 reports yang STOPs as
  `KernelError::BooleanFailed` / `Other`, so they arrive as
  `KernelFailure{kernel}`. The `KernelStop` code of §6.1 therefore needs a
  kernel-v2 change that maps STOPs to a variant of their own; until then the
  agent host reports them as `FeatureRebuildFailed`. Import sources gained a
  typed `EngineError::SourceUnavailable` with an unchanged message.
- **ICR-3 — face listing** (`wasm-bridge`). `UiToEngine::ListFaces{body_id,
  filter}` → `FacesListed{[{geom_ref, signature}]}`, deterministically
  ordered. The refs are the ones viewport face ranges carry, so a picked ref
  and a listed ref are interchangeable.
  **LANDED 2026-09-14.** The viewport and `ListFaces` share one builder,
  `wasm_bridge::face_refs::face_geom_refs`. The listing is ordered by
  canonical `GeomRef` JSON; `tie_break` is ignored. One inherited limitation:
  a roleless face on a non-ghost body (an imported STEP body) carries the
  viewport's index-only fallback ref, which may resolve to the wrong face.
  That is a picking defect shared with the viewport, tracked in the plan.
- **ICR-4 — provenance and ids on feature commands** (`wasm-bridge`).
  `AddFeature`, `EditFeature` and `FinishSketch` gain `provenance:
  Option<Provenance>`, recorded through `FeatureEngine::set_provenance`
  inside the same undo step (I4 with I5). The response carries the new
  `feature_id`. Today only `add_import_feature` records provenance, and
  `AddFeature` returns no id.
  **LANDED 2026-09-14.** `Engine::add_feature_with_provenance` /
  `edit_feature_with_provenance`; the undo `Command`s carry the record, and
  `ModelUpdated.feature_id` is also set by `ImportStep`. It fixed a latent
  defect: undoing an add left the feature's provenance record orphaned, so a
  STEP import followed by undo kept an `Import` record in the saved file,
  which would have broken I3.

- **ICR-5 — feature names on creation** (`wasm-bridge`), proposed
  2026-09-14. `AddFeature` and `FinishSketch` gain `name: Option<String>`,
  set inside the same undo step, so `sketch_create`/`feature_add` can take a
  `name` without becoming two undo steps (I5).

Store-side changes (sub-project `08-ui-chrome` / `05-sketch-ui`, not bridge
ICRs):
- the engine lock;
- a non-swallowing send entry point;
- busy-state getters for the open dialogs;
- the agent badge in the tree.

---

## 10. Open questions

Resolved 2026-09-14:
- **Ports for end users: the user picks one.** An end user passes
  `--port <n>` in their MCP client config (or sets `$PORT`). On this machine,
  `proj port` resolves it. There is no OS-assigned (bind 0) fallback. The
  pairing link carries the chosen port, so the page never guesses it.
- **Default `on_error`: `rollback`.** A failed agent step is undone by
  default. `keep` stays available per call.
- **Relay language: Python**, distributed with `uvx` and a pinned version.
  The relay holds no modeling logic, so the choice does not touch the design:
  the page speaks `waffle-agent-link/1` to whatever holds the socket,
  authoritative argument validation happens in the page, and the manifest is
  plain JSON. The alternatives and why they lost:
  - *Node* (`npx`): reference MCP SDK and the app's existing tooling, but it
    needs a Node bump in the dev container (18.20.8, end of life).
  - *Rust* (native binaries): fits the repo's gates and ships a
    build-time-fixed binary, but needs per-platform releases and OS code
    signing.
  - *Rust compiled to WASM*: the relay must listen on TCP, which a browser
    cannot do. Node's WASI has no sockets, and a Wasmtime requirement is a
    rarer install than `uv`.

  Python's costs, accepted:
  - a third ecosystem in CI (ruff lint + format check, pytest, PyPI
    publishing from CI with trusted publishing, never from a laptop);
  - `uv` added to the dev container, which has Python 3.12.3 but no `pip`
    or `uv`.

- **`tools/list_changed` on 2026-07-28 clients.** The MCP Python SDK drops a
  session-level change notification on a 2026-07-28 connection; those clients
  receive it only on a `subscriptions/listen` stream. The relay serves
  `subscriptions/listen` and publishes on both paths (Phase 0, 2026-09-14).

Still open:

1. **Package and sub-project names.** `waffle-mcp-relay`, `relay/`,
   `projects/14-agent-link/` are placeholders.
