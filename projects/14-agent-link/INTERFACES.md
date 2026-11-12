# 14 — Agent Link: Interfaces

## Provides

**MCP tools.** The authoritative list with schemas is
`relay/src/waffle_mcp_relay/agent-tools.manifest.json`.

| Group | Tools |
|---|---|
| Connection (relay) | `waffle_connect`, `waffle_status` |
| Documents | `document_info`, `document_open`, `document_new`, `document_import`, `document_save`, `storage_list` |
| Tabs | `tab_switch`, `tab_add`, `tab_move`, `tab_rename` |
| Inspection | `model_summary`, `feature_get`, `selection_get`, `face_list`, `entity_list`, `names_list`, `sketch_regions`, `sketch_solve_state`, `sketch3d_get`, `parameters_get`, `expression_evaluate`, `entity_meta` |
| Measurement | `body_measure`, `measure_distance`, `measure_interference`, `measure_mass`, `measure_section`, `measure_thickness` |
| Authoring | `sketch_create`, `sketch_edit`, `feature_add`, `feature_edit`, `feature_delete`, `feature_suppress`, `feature_reorder`, `feature_rename`, `body_rename`, `rollback_set`, `parameters_set`, `undo`, `redo` |
| Names and materials | `entity_name`, `entity_unname`, `material_list`, `material_set`, `body_material_set` |
| Scripts | `script_run_check`, `script_source_add`, `script_source_get`, `script_source_update`, `script_feature_add` |
| Import and export | `import_step`, `kicad_link`, `export_step`, `export_stl`, `export_dxf`, `export_svg`, `export_pdf` |
| Assemblies | `assembly_get`, `instance_add/edit/delete`, `connector_add/edit/delete`, `mate_add/edit/delete` |
| Drawings | `drawing_get`, `drawing_view_add/edit/delete`, `drawing_annotation_add/edit/delete`, `drawing_sheet_edit` |
| Viewport | `viewport_view`, `viewport_capture` |

**Protocols.**

- `waffle-agent-link/1` — WebSocket frames between the relay and a paired
  page (`specs/waffle_mcp_server.md` §2.3).
- `waffle-host/1` — stdio frames between the relay and `waffle-host`.
- `waffle-viewer/1` — WebSocket frames between the relay and a viewer in
  host mode (`specs/waffle_server_mode.md` §4).

## Consumes

- `wasm_bridge::tools::execute_tool` and the `Tool` / `ToolResult` messages.
- The store's agent surface (`store.svelte.js`): `withEngineLock`,
  `sendAgentMessage`, `getUserBusyReason`, `isEngineCrashed`,
  `isEngineReady`, `isDocumentReadOnly`, `setAgentActivity`, and the
  document flows the Home screen uses.
- Storage providers (`app/src/lib/storage/index.js`).
- `docs/schema/waffle-v5.schema.json` — tool input schemas reference its
  definitions for engine types (`Operation`, `SketchEntity`,
  `SketchConstraint`, `GeomRef`, …).

## Errors

A failed call is an `isError` result whose `structuredContent.error` is
`{code, message, details}`. `code` comes from a closed set
(`specs/waffle_mcp_server.md` §6.1); examples are `PageAway`,
`EngineCrashed`, `HostCapability`, `ViewerUnavailable`, `Deferred`.
