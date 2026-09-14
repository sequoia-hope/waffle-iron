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
- [ ] `/agent` route: detect a local-network-access denial and show how to
  grant it. A failing `WebSocket` exposes no error detail to page JS;
  investigate whether `navigator.permissions.query` answers for this
  permission, and fall back to the §6.3 guidance.

## Phase 1 — Live authoring
Not started. See spec §8.

## Blockers

- Environment: the workspace disk is at ~100% (16 GB free on 2026-09-14);
  watch it during WASM builds. Only Chromium is installed for Playwright.
