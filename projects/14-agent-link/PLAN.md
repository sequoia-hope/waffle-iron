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
- [ ] **ICR-3** face listing: bridge `ListFaces` → `FacesListed`, refs equal to
  viewport face-range refs, deterministic order.

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
- [ ] Relay tier in `./scripts/test.sh`; `REFERENCES.md` entries (the licence
  check is in `relay/README.md`: `mcp` MIT, `websockets` BSD-3-Clause).
- [ ] Relay-side validation of tool arguments against `inputSchema` (today:
  unknown tool and malformed calls ⇒ -32602 only).
- [ ] `EngineCrashed` needs a store getter for the crash state (queries report
  `EngineNotReady` meanwhile).
- [ ] `tools/list_changed` under the SDK's newest protocol revision is dropped
  unless the client subscribes; check the clients we support.
- [ ] O23 browser matrix: Edge, Firefox, Safari manual; hosted https origin
  after deploy.
  - [x] Chromium (headless Playwright), page `http://localhost` dev origin →
    `ws://127.0.0.1`: socket opened and `welcome` received (2026-09-14).
    Headless, so a permission prompt could not be observed.

## Phase 1 — Live authoring
Not started. See spec §8.

## Blockers

- Environment: the workspace disk is at ~100% (16 GB free on 2026-09-14);
  watch it during WASM builds. Only Chromium is installed for Playwright.
