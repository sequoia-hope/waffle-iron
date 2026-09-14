# Sub-project 14: Agent Link — Plan

Spec: `specs/waffle_mcp_server.md` (§8 phases, §9 ICRs, §10 decisions).

## Phase 0 — Spike + ICRs (IN PROGRESS, started 2026-09-14)

Exit: O23 browser matrix recorded in the spec; go/no-go note on the default
connection path; ICR-1…ICR-4 merged.

### ICRs (Rust, owning crates)
- [x] **ICR-4** provenance + feature id on feature commands (2026-09-14):
  `Engine::add_feature_with_provenance` / `edit_feature_with_provenance`;
  `Command::AddFeature.provenance`, `Command::EditFeature.provenance`
  (`(old, new)`); undo/redo carry the record. Bridge: `AddFeature`,
  `EditFeature`, `FinishSketch` gain `provenance` (serde-defaulted);
  `ModelUpdated.feature_id` names the created/edited feature (also
  `ImportStep`). Fixed a latent defect found on the way: undo of an add left
  the feature's provenance record orphaned (STEP import → undo kept an
  `Import` record in the file). Tests: `feature-engine/tests/provenance_commands.rs`,
  `wasm-bridge/tests/feature_provenance.rs`.
- [x] **ICR-2** typed errors (2026-09-14): `feature_engine::types::ErrorKind`
  + `FeatureError`; `Engine::feature_errors` is built from the same sources
  in the same order as `Engine::errors` (parameter errors ⇒ `Expression`,
  context errors ⇒ `Context`, rebuild errors ⇒ `ErrorKind::from(&EngineError)`).
  New `EngineError::SourceUnavailable` (same message text as the
  `RebuildFailed` it replaces). Bridge: `ModelUpdated.feature_errors`,
  `EngineToUi::Error.kind` (engine errors only; bridge-level failures leave
  it absent). Tests: `feature-engine/tests/typed_errors.rs`,
  `wasm-bridge/tests/typed_errors.rs`. **Known limit (open):** yang STOPs
  reach the engine as `KernelError::BooleanFailed` / `Other`, so they are
  `KernelFailure{kernel}` — a distinct `KernelStop` kind needs kernel-v2 to
  map STOPs to their own `KernelError` variant. Not done by parsing messages.
- [x] **ICR-1** exact measurement (2026-09-14): `KernelIntrospect::solid_volume`
  / `solid_surface_area` (default `NotSupported`; kernel-v2 via
  `geom::signed_volume` / `introspect::surface_area`, imported bodies
  `NotSupported`). Bridge `MeasureBody` → `BodyMeasured{volume_m3,
  surface_area_m2: Measured{value, method, exact_unavailable}, bbox (mesh),
  counts, closed}`; it fills missing meshes first, because native dispatch
  never tessellates. Measured: the 20×10×5 mm box is exact to 1e-15 and the
  r5 h10 cylinder is exact to 1e-15. Tests: `kernel-v2/tests/icr1_solid_measure.rs`,
  `wasm-bridge/tests/measure_body.rs`, mock default in `waffle-types` `mock.rs`.
- [x] **ICR-3** face listing (2026-09-14): bridge `ListFaces{body_id, filter}` →
  `FacesListed{faces: [ListedFace{geom_ref, signature}]}`, ordered by
  canonical `GeomRef` JSON. The refs come from `wasm_bridge::face_refs::
  face_geom_refs`, which the viewport's `wasm_api::build_face_entries` now
  also uses, so a listed ref equals a picked ref by construction. The filter
  is `feature_engine::resolve::passes_all_filters` (made public); `tie_break`
  is ignored. Test `wasm-bridge/tests/list_faces.rs`: every listed ref
  resolves (`resolve_face_plane`) to a plane whose normal matches its
  signature.
  **Known limitation (open, inherited):** a roleless face on a non-ghost
  body (an imported STEP body) gets the viewport's index-only `Signature`
  fallback. Per the existing comment in `face_refs`, `signature_similarity`
  ignores `adjacency_hash`, so that ref may resolve to an arbitrary face.
  It is the same ref the viewport hands out today; fixing it is a
  viewport-picking change, not ICR-3.

### Spike (relay + page)
- [x] `relay/` Python package `waffle-mcp-relay` (2026-09-14): CLI port
  resolution, bind, origins, pairing codes, sessions, heartbeat, frames; MCP
  tools `waffle_connect`, `waffle_status`, `model_summary`. pytest: 61 tests
  covering O13, O15–O18 and O20, plus a manifest-equals-generation test; ruff
  clean. Locked `mcp` 2.2.0.
- [x] App (2026-09-14): `/agent` consent route, link singleton, executor with
  `model_summary`, agent bar with Disconnect, manifest generator. GUI spec
  `agent-link.spec.js`: pairing + `model_summary` vs `__waffle`, and O14 (no
  socket without consent).
- [x] Relay tier (2026-09-14): `./scripts/test.sh relay` (pytest + ruff check +
  ruff format --check; part of `all-fast` and `all`), documented in
  `docs/TESTING.md`. `REFERENCES.md` #53–#58.
- [x] Relay-side argument validation (2026-09-14): `server.ArgumentValidator`
  checks `tools/call` arguments against the tool's `inputSchema`
  (`Draft202012Validator`) before forwarding; a failure is `-32602` with the
  JSON pointer in `data` (`/arguments/...`) and no frame reaches the page. A
  manifest whose `inputSchema` is not a valid schema is refused at adoption.
  `jsonschema` became a direct dependency (it was already installed as a
  dependency of `mcp`; MIT). Tests: `tests/test_arguments.py`, O20 in
  `tests/test_stdio.py`.
- [x] `EngineCrashed` (2026-09-14): store `isEngineCrashed()` is set when the
  worker reports `needsRestart`; the executor answers `EngineCrashed` before
  `EngineNotReady`.
- [x] `tools/list_changed` for both protocol eras (2026-09-14). Measured in the
  SDK source (`mcp` 2.2.0, `mcp/server/connection.py` `NotifyOnlyOutbound`):
  on a 2026-07-28 connection a session-level `list_changed` is dropped; such
  clients get it only on a `subscriptions/listen` stream. The relay now serves
  `subscriptions/listen` (`ListenHandler` over an `InMemorySubscriptionBus`)
  and publishes `ToolsListChanged` there as well as on the session. Handshake
  clients (≤ 2025-11-25, which the GUI helper and the pytest client use) keep
  receiving it directly (O18). Tested: bus publication and the advertised
  capability in both eras. Not tested: an end-to-end 2026-07-28 stdio client
  (no such client in the test harness). Which revision a given client
  (Claude Code, Claude Desktop) negotiates was not measured.
- [ ] O23 browser matrix: headed Chrome (does the local-network prompt appear
  for a WebSocket opened from the Allow click?), Edge, Safari — manual, and
  not runnable in this container (only headless Playwright Chromium is
  installed). Needs the user on a desktop browser.
  - [x] Firefox (headed, user-reported 2026-09-14): pairing and
    `model_summary` work end to end. Not recorded: the page origin (hosted
    default vs dev server) and whether any prompt appeared.
  - [x] Chromium (headless Playwright chromium-1228), page `http://localhost`
    dev origin (5174 worktree, 5173 main) → `ws://127.0.0.1`: socket opened,
    `welcome` received (2026-09-14).
  - [x] Chromium (headless), page **hosted** `https://sequoia-hope.github.io/waffle-iron/agent`
    → `ws://127.0.0.1`, default permissions: **BLOCKED**,
    `net::ERR_BLOCKED_BY_LOCAL_NETWORK_ACCESS_CHECKS` (2026-09-14).
  - [x] Same, with the context permission `local-network-access` granted:
    socket opened, `welcome` received, relay `waffle_status` = `ready`
    (2026-09-14). (`loopback-network` and `local-network` are unknown
    permission names in this Playwright.) Probe script: `o23-hosted.mjs`
    pattern, reusing `app/tests/gui/helpers/mcp-relay.js`.
  - **Go/no-go (Chromium): GO**, conditional on the user granting the
    local-network-access permission. A denial is the expected failure mode,
    so the consent route must handle it.

## Phase 1 — carried from Phase 0 findings
- [x] `/agent` route detects a local-network block (2026-09-14). Measured:
  Chromium 149 answers `navigator.permissions.query({name:
  "local-network-access"})` (`prompt` by default, `granted` after a grant).
  A failed socket is classified `permission_denied` (state `denied`) or
  `permission_blocked` (state `prompt`, public page, loopback/private relay),
  and the consent card shows how to allow local network access, above the
  §6.3 fallbacks. Not covered by a GUI test: the dev-server origin is
  loopback, so neither class occurs there.

## Phase 1 — Live authoring (IN PROGRESS, started 2026-09-14)

Exit (spec §8): O1–O20 green (O4 `NotSupported` row after ICR-2);
`sketch-drawing-regression.spec.js` still green.

### Landed
- [x] **Store layer** (`a01e06d5`). The bridge send gate makes every
  user-originated send hold the FIFO engine lock from send to response;
  hover/select stay ungated. `sendAgentMessage` is the non-swallowing entry
  point. `getUserBusyReason` covers sketch mode, feature dialogs and edit
  context. While `setAgentActivity` is set, the toolbar, shortcuts and tree
  refuse modeling with "Agent is working" (G8). `bridge.recordSends` tags
  each send with its origin (O7). Finish Sketch's profile conversion moved
  verbatim to `$lib/sketch/finishProfiles.js` (byte-identical on six
  sketches).
- [x] **Executor + 19 page tools** (spec §2.5 minus documents/storage,
  viewport_capture, export): `model_summary`, `feature_get`,
  `selection_get`, `body_measure`, `face_list`, `sketch_regions`,
  `expression_evaluate`, `sketch_create`, `feature_add`, `feature_edit`,
  `feature_delete`, `feature_suppress`, `feature_reorder`,
  `feature_rename`, `body_rename`, `rollback_set`, `parameters_set`,
  `undo`, `redo`.
  - `$lib/agent/executor.js`: gates G3–G7, lock wait G2 (10 s; another call
    of the same agent just queues), activity, cancellation (A18: undo the
    finished step).
  - `commands.js`: `applyStep` does the rollback and verifies it byte-exact
    on the canonical feature tree; a mismatch is `Internal` and pauses the
    session. It also sets `kept_with_error`, adds agent provenance and
    toasts once per step.
  - `queries.js`: the engine queries (`body_measure`, `face_list`,
    `sketch_regions`, `expression_evaluate`) take the lock too.
  - `delta.js`: the ModelDelta, plus `order_changed`.
  - `sketchInput.js`: the A13 id checks.
- [x] **Tool schemas**: engine types are `#/$defs/<Name>` refs with each
  tool's `$defs` generated from `docs/schema/waffle-v5.schema.json` into
  `tools/engineSchemas.generated.js` (`gen-agent-manifest.mjs` regenerates
  both it and the relay manifest; `--stdout` fails on a stale file). O19:
  pytest `test_o19_…` (value-equal to the golden) + `agent-executor-pure`.
- [x] **Link/bar**: Pause/Resume (the page's own pause carries a reason),
  running-tool label, `status` frames (`ready`/`paused`/`busy{reason}`),
  `cancel` frames.
- [x] Tests: `agent-authoring.spec.js` has 12 tests:
  - O1 exact 20×10×5 mm box.
  - O2 exact cylinder.
  - O4: no-loop profile, edit breaking a downstream extrude, over-constrained
    sketch, and a typed `NotSupported` (arc profile committed without its
    `vertex_ids` polygon).
  - O5 keep; O6 real Ctrl+Z; O10 provenance + badge.
  - O7 no user send between the agent's sends, and nothing queued (G8).
  - O8 busy gates: sketch mode entered by real clicks, the extrude dialog,
    Pause.
  - O9 pause mid-call, Disconnect ≤ 1 s; O11 deferred.
  - O12 real face click → `selection_get` → `sketch_create`, normal ± 1e-9.
  - G2 a 25 s user `LoadProject` makes the call `UserBusy{rebuilding}` after
    10 s.
  - A18 cancel undoes the finished step.
  - A7/A13/A14/A16 refusals.

  The slow call is a 21-tooth gear extrude (~2 s here).
  `agent-executor-pure.spec.js` has 15 tests; the relay suite 71.
  29/29 across the three agent specs.
- [x] `sketch_create` regions come from the committed feature through
  `sketchRegionsRequest`, so gear sketches report theirs. Before, raw Gear
  entities gave `[]`; found by the probe.

### Decisions and findings (recorded in the spec)
- The viewport's datum-plane refs (`anchor: {type: "DatumPlane"}`) are not
  golden `GeomRef`s. `selection_get` returns a datum plane's
  `{origin, normal}`, which `sketch_create` accepts.
- No `name` argument on `sketch_create`/`feature_add`: `AddFeature` and
  `FinishSketch` carry no name, and a follow-up `RenameFeature` would make
  the call two undo steps (I5). **ICR-5 (proposed):** `name:
  Option<String>` on `AddFeature`/`FinishSketch`.
- Rollback is an engine `Undo`, so the rolled-back step sits on the redo
  stack (a user Ctrl+Shift+Z re-applies it). I3 holds for the document and
  the undo depth, not the redo stack. Discarding it needs an engine
  "drop redo" message (not proposed yet).
- `expression_evaluate` returns `{value_mm: null, error}` for a failing
  expression instead of an `isError` result; the closed code set has no
  expression code.
- `tools/list` is ~160 KB of compact JSON (`feature_add` and
  `feature_edit` each embed the ~54 KB Operation closure). Correct but
  expensive for agent context. Open: a shared schema resource, or schema
  pruning that O19 can still pin.
- A query that sends a bridge message takes the lock (I6 covers agent
  messages during a user action); the spec's exception list named only
  `body_measure`/`face_list`.

### Open
- [ ] O3 parity: 15 scripted sequences via the agent vs the same messages
  through the store entry point in a fresh page, canonical bytes equal.
- [ ] G5 read-only document and G7 Assembly tab rows of O8.
- [ ] O13–O20 are relay-harness oracles and green in pytest. Recheck them
  against Phase 1 frames: `status` busy reason, `cancel`.
- Finding (2026-09-14): F0064 (the spec's coplanar `NotSupported` example)
  builds with no feature errors in the app. The O4 `NotSupported` row uses
  kernel-v2's arc-profile wall instead.
- [ ] Documents and storage tools: `document_info`, `storage_list`,
  `document_open`, `document_new`, `document_save`, `tab_switch`.
- [ ] Rerun `sketch-drawing-regression.spec.js` at the phase exit.
- Not caused by this work: `planetary-gear.spec.js` "created stage extrudes
  into a solid" times out identically on the pre-session sources
  (1d9c0b39, verified with `git stash -u`).

## Blockers

- Environment: the workspace disk is at ~100% (16 GB free on 2026-09-14);
  watch it during WASM builds. Only Chromium is installed for Playwright.
