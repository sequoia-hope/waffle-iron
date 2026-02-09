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

### M4: STEP Export
- [ ] `export_step(tree: &FeatureTree, engine: &mut Engine) -> Result<String, ExportError>`
- [ ] Rebuild model to get final solid
- [ ] Export via truck's ruststep (AP203)
- [ ] Handle export failures gracefully
- [ ] Test: export a simple box → verify valid STEP output
- **Blocker**: TruckKernel doesn't implement KernelBundle (see modeling-ops M10 notes)

### M5: Version Migration ✅
- [x] Migration framework defined (migrate.rs)
- [x] Currently v1 only — no migrations needed yet
- [x] Error handling for unknown migration paths
- [ ] Define migration functions for version N → N+1 (when format changes)

### M6: Round-Trip Tests
- [x] Save → load round-trip for simple trees (load_round_trip_simple_tree)
- [x] Feature ID preservation across round-trip
- [x] Operation parameters preservation
- [x] GeomRef preservation
- [ ] Save → load → rebuild → compare topology (needs Engine integration)

## Test Summary

| Test Suite | Count | Status |
|-----------|-------|--------|
| M1 Schema | 6 | ✅ All pass |
| M2 Save | 3 | ✅ All pass |
| M3 Load | 10 | ✅ All pass |
| **Total** | **19** | **✅** |

## Blockers

- M4 (STEP export) blocked: TruckKernel doesn't implement KernelBundle trait
- M6 (full round-trip with rebuild) needs Engine integration tests

## Notes

- All feature-engine and waffle-types types already have serde derives with `#[serde(tag = "type")]`
- The native format stores the recipe (operations + parameters), NOT geometry
- Files use `.waffle` extension
- Format version is 1 (FORMAT_VERSION constant)
