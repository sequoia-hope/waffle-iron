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
- [ ] **ICR-2** typed errors: `EngineToUi::Error.kind`,
  `ModelUpdated.feature_errors`. Known limit: yang STOPs reach the engine as
  `KernelError::Other` / `BooleanFailed` strings, so a typed `KernelStop`
  needs a kernel-v2 mapping change — record, do not parse messages.
- [ ] **ICR-1** exact measurement: `KernelIntrospect::solid_volume` /
  `solid_surface_area`; bridge `MeasureBody` → `BodyMeasured`.
- [ ] **ICR-3** face listing: bridge `ListFaces` → `FacesListed`, refs equal to
  viewport face-range refs, deterministic order.

### Spike (relay + page)
- [ ] `relay/` Python package `waffle-mcp-relay`: CLI port resolution, bind,
  origins, pairing codes, sessions, heartbeat, frames; MCP tools
  `waffle_connect`, `waffle_status`, `model_summary`; pytest O13, O15–O17, O20.
- [ ] App: `/agent` consent route, link singleton, executor with
  `model_summary`, agent bar with Disconnect; manifest generator; GUI spec
  (pairing + `model_summary` vs `__waffle`; O14 no socket without consent).
- [ ] O23 browser matrix: Chromium automated (localhost dev origin; hosted
  https origin after deploy); Edge, Firefox, Safari manual.

## Phase 1 — Live authoring
Not started. See spec §8.

## Blockers

- Environment: the workspace disk is at ~100% (16 GB free on 2026-09-14);
  watch it during WASM builds. Only Chromium is installed for Playwright.
