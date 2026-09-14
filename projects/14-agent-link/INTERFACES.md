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

- **MCP tools** to agents (spec §2.5).
  - Relay: `waffle_connect`, `waffle_status`.
  - Page, Phase 1 (2026-09-14):
    - queries: `model_summary`, `feature_get`, `selection_get`,
      `body_measure`, `face_list`, `sketch_regions`, `expression_evaluate`;
    - commands: `sketch_create`, `feature_add`, `feature_edit`,
      `feature_delete`, `feature_suppress`, `feature_reorder`,
      `feature_rename`, `body_rename`, `rollback_set`, `parameters_set`,
      `undo`, `redo`.
  - Still to come: documents/storage (Phase 1), `viewport_capture` and export
    (Phase 2).
- **`waffle-agent-link/1`** WebSocket frames between relay and page: spec §2.3.
- **Tool manifest** `agent-tools.manifest.json`, generated from
  `app/src/lib/agent/tools/`, bundled by the relay. Engine-type inputs are
  `#/$defs/<Name>` refs; each tool embeds its `$defs`, generated from the
  golden into `tools/engineSchemas.generated.js`.
- **App store surface** the agent layer uses (`store.svelte.js`):
  - locking: `withEngineLock`, `EngineLockTimeout`, `getEngineLockHolder`;
  - agent entry point: `sendAgentMessage`;
  - page state: `getUserBusyReason`, `getAgentActivity`/`setAgentActivity`,
    `isEngineCrashed`;
  - shared with the app: `beginSketchPlaneRef`, `sketchRegionsRequest`, and
    `$lib/sketch/finishProfiles.js` `buildFinishProfiles`;
  - test oracles: `__waffle.recordEngineSends` / `getEngineSendLog`.

## Interface change requests (owned by other sub-projects)

| ICR | Owner | Change | Status |
|---|---|---|---|
| ICR-4 | feature-engine, wasm-bridge | `provenance` on `AddFeature`/`EditFeature`/`FinishSketch`; `ModelUpdated.feature_id` | landed 2026-09-14 |
| ICR-2 | wasm-bridge, feature-engine | `EngineToUi::Error.kind`; `ModelUpdated.feature_errors` | landed 2026-09-14 (`KernelStop` kind still needs a kernel-v2 variant) |
| ICR-1 | waffle-types, kernel-v2, wasm-bridge | `KernelIntrospect::solid_volume`/`solid_surface_area`; `MeasureBody` → `BodyMeasured` | landed 2026-09-14 |
| ICR-3 | wasm-bridge (+ `feature_engine::resolve::passes_all_filters` made public) | `ListFaces` → `FacesListed` | landed 2026-09-14 |
| ICR-5 | wasm-bridge, feature-engine | `name: Option<String>` on `AddFeature`/`FinishSketch`, in the same undo step | proposed 2026-09-14 |
