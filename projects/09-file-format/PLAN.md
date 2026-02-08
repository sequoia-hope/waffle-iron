# 09 — File Format: Plan

## Milestones

### M1: JSON Schema Definition
- [ ] Define the complete JSON structure for FeatureTree serialization
- [ ] Document all Operation variant serializations
- [ ] Document GeomRef serialization
- [ ] Validate against INTERFACES.md serde annotations

### M2: Save (Serialize)
- [ ] `save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String`
- [ ] Serialize FeatureTree to JSON
- [ ] Include format version and metadata
- [ ] Pretty-print for human readability
- [ ] Test: save a 3-feature tree → verify JSON structure

### M3: Load (Deserialize)
- [ ] `load_project(json: &str) -> Result<(FeatureTree, ProjectMetadata), LoadError>`
- [ ] Deserialize JSON to FeatureTree
- [ ] Validate format version
- [ ] Trigger rebuild after load (via feature-engine)
- [ ] Test: load saved JSON → verify FeatureTree matches original

### M4: STEP Export
- [ ] `export_step(tree: &FeatureTree, kernel: &dyn Kernel) -> Result<String, ExportError>`
- [ ] Rebuild model to get final solid
- [ ] Export via truck's ruststep (AP203)
- [ ] Handle export failures gracefully
- [ ] Test: export a simple box → verify valid STEP output

### M5: Version Migration
- [ ] Define migration functions for version N → N+1
- [ ] Apply migrations on load if file version < current version
- [ ] Preserve unknown fields for forward compatibility
- [ ] Test: load a v1 file with v2 loader → verify migration works

### M6: Round-Trip Tests
- [ ] Save → load → rebuild → compare topology
- [ ] Verify all features rebuild correctly after round-trip
- [ ] Verify all GeomRefs resolve after round-trip
- [ ] Test with various feature combinations (sketch+extrude, fillet, chamfer, boolean)

## Blockers

- Depends on feature-engine (FeatureTree structure, rebuild on load)

## Interface Change Requests

(None yet)

## Notes

- The native format stores the recipe, NOT geometry. This is intentional.
- Solved sketch positions are NOT stored. Sketches re-solve on load.
- STEP export has known limitations (AP203 only, boolean results may fail).
