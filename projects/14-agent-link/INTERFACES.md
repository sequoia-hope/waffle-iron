# Sub-project 14: Agent Link — Interfaces

## Consumes

- `wasm_bridge::messages::{UiToEngine, EngineToUi}` through the page's
  `EngineBridge` (never direct WASM imports from UI components).
- `docs/schema/waffle-v5.schema.json` — tool input schemas `$ref` its
  definitions for `Operation`, `SketchEntity`, `SketchConstraint`, `GeomRef`,
  `Parameter`, `TopoQuery`.
- App store getters (`store.svelte.js`) and storage providers
  (`app/src/lib/storage/index.js`).

## Provides

- **MCP tools** to agents: spec §2.5 (Phase 0: `waffle_connect`,
  `waffle_status`, `model_summary`).
- **`waffle-agent-link/1`** WebSocket frames between relay and page: spec §2.3.
- **Tool manifest** `agent-tools.manifest.json`, generated from
  `app/src/lib/agent/tools/`, bundled by the relay.

## Interface change requests (owned by other sub-projects)

| ICR | Owner | Change | Status |
|---|---|---|---|
| ICR-4 | feature-engine, wasm-bridge | `provenance` on `AddFeature`/`EditFeature`/`FinishSketch`; `ModelUpdated.feature_id` | landed 2026-09-14 |
| ICR-2 | wasm-bridge, feature-engine | `EngineToUi::Error.kind`; `ModelUpdated.feature_errors` | landed 2026-09-14 (`KernelStop` kind still needs a kernel-v2 variant) |
| ICR-1 | waffle-types, kernel-v2, wasm-bridge | `KernelIntrospect::solid_volume`/`solid_surface_area`; `MeasureBody` | open |
| ICR-3 | wasm-bridge | `ListFaces` → `FacesListed` | open |
