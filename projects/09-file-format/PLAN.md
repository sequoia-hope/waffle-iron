# 09 — File Format: Plan

## Milestones

### M1: JSON Schema Definition ✅
- [x] Define complete JSON structure for FeatureTree serialization
- [x] Document all Operation variant serializations (Sketch, Extrude, Revolve, Fillet, Chamfer, Shell, BooleanCombine)
- [x] Document GeomRef serialization (with Anchor, Selector, ResolvePolicy)
- [x] Validate against INTERFACES.md serde annotations
- [x] Tests: save_produces_valid_json, save_includes_format_and_version, save_includes_project_metadata, save_includes_features_array, save_serializes_operation_type_tags, save_serializes_geom_refs

### M2: Save (Serialize) ✅
- [x] `save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String`
- [x] Serialize FeatureTree to JSON via serde_json
- [x] Include format version (v1) and metadata (name, created, modified)
- [x] Pretty-print for human readability
- [x] Tests: save_empty_tree, save_all_operation_types, save_preserves_suppressed_flag

### M3: Load (Deserialize) ✅
- [x] `load_project(json: &str) -> Result<(FeatureTree, ProjectMetadata), LoadError>`
- [x] Deserialize JSON to FeatureTree
- [x] Validate format identifier ("waffle-iron")
- [x] Validate format version (reject future versions)
- [x] Tests: load_round_trip_simple_tree, load_preserves_feature_ids, load_preserves_operation_params, load_preserves_sketch_entities_and_constraints, load_preserves_geom_refs, load_rejects_unknown_format, load_rejects_future_version, load_rejects_invalid_json, load_preserves_active_index, load_preserves_suppressed_features

### M4: STEP Export ✅
- [x] `export_step(tree: &FeatureTree, kb: &mut TruckKernel) -> Result<String, ExportError>`
- [x] Rebuild model from FeatureTree using Engine + TruckKernel
- [x] Export final solid via truck's CompressedSolid + StepModel (AP203)
- [x] Handle export failures: NoSolid, StepExportFailed errors
- [x] Tests: step_export_simple_box (validates ISO-10303-21, MANIFOLD_SOLID_BREP, FACE_SURFACE), step_export_empty_tree_returns_error, step_export_suppressed_only_returns_error
- **Note**: Blocker resolved — TruckKernel now implements KernelIntrospect directly

### M5: Version Migration ✅
- [x] Migration framework defined (migrate.rs)
- [x] Currently v1 only — no migrations needed yet
- [x] Error handling for unknown migration paths
- [ ] Define migration functions for version N → N+1 (when format changes)

### M6: Round-Trip Tests ✅
- [x] Save → load round-trip for simple trees (load_round_trip_simple_tree)
- [x] Feature ID preservation across round-trip
- [x] Operation parameters preservation
- [x] GeomRef preservation
- [x] Save → load → rebuild → compare topology (round_trip_save_load_rebuild_produces_solid)
- [x] Feature IDs preserved through rebuild (round_trip_preserves_feature_ids_through_rebuild)
- [x] STEP export matches after round-trip (round_trip_step_export_matches_original)
- [x] Topology comparison: created entities, roles match (round_trip_rebuild_topology_matches)

## Test Summary

| Test Suite | Count | Status |
|-----------|-------|--------|
| M1 Schema | 6 | ✅ All pass |
| M2 Save | 3 | ✅ All pass |
| M3 Load | 10 | ✅ All pass |
| M4 STEP Export | 3 | ✅ All pass |
| M6 Round-Trip | 4 | ✅ All pass |
| **Total** | **26** | **✅** |

## Discovered tasks (2026-08-28 format audit — see `docs/FILE_FORMAT.md` §14–15)

- [x] Write an accurate spec of the v3 format → `docs/FILE_FORMAT.md` (supersedes this dossier's v1 description)
- [x] **(2026-08-28 seam fixes)** Fix `created` timestamp destroyed on every save — `initDocumentState` adopts it, the writer latches it
- [x] **(2026-08-28)** Fix `extractDisplayUnit` v2-only path — reads `document.display_unit ?? project.display_unit`; `initDocumentState` adopts the unit too (covers empty docs)
- [x] **(2026-08-28)** Multi-tab File→Open: picker branch adopts the file's tabs via `initDocumentState` under a fresh storage doc id; download path (`saveProject`) writes the full document via `buildDocumentJson`
- [x] **(2026-08-28)** Forward-compat: `min_reader_version` written by all writers (Rust `save.rs` + JS via `$lib/engine/format.js`), enforced by Rust loaders + JS open paths → clean `FutureVersion` / toast instead of parse noise
- [x] **(2026-08-28)** Save-time guard against non-finite floats: bridge `SaveProject` uses `save_project_verified` (serialize + self-load); regression tests in `format_tests.rs` + `app/tests/gui/document-format-seam.spec.js`
- [ ] Consolidate to a single document writer: route JS `buildDocumentJson` through Rust `save_document` via a bridge `SaveDocument{tabs…}` message (the JS-writer envelope is regression-pinned by `document-format-seam.spec.js` in the meantime)
- [ ] Load-time hardening for shared files: cap STEP blob inflation, bounds-check counts
- [ ] Deduplicate `PreviewMesh` (defined in both file-format and feature-engine)
- [ ] (defer) Region size: `outer` + `outer_edges` store the same boundary twice (374 KB in one observed extrude)

## Blockers

(None — all milestones complete)

## Notes

- All feature-engine and waffle-types types already have serde derives with `#[serde(tag = "type")]`
- The native format stores the recipe (operations + parameters), NOT geometry
- Files use `.waffle` extension
- Format version is 1 (FORMAT_VERSION constant)
