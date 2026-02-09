# 06 — Feature Engine: Plan

## Milestones

### M1: Feature Tree Data Structure ✅
- [x] `FeatureTree` struct (ordered Vec<Feature> + active_index)
- [x] Add feature (append + insert at position)
- [x] Remove feature
- [x] Reorder features (move up/down)
- [x] Suppress/unsuppress feature
- [x] Set active_index (rollback)
- [x] Unit tests for all mutations (7 tree tests)

### M2: GeomRef + Anchor + Selector Types ✅
- [x] Implement GeomRef, Anchor, Selector, ResolvePolicy (from INTERFACES.md) — in waffle-types crate
- [x] GeomRef constructors for common cases (role-based, signature-based)
- [x] Serde serialization tests (round-trip) — covered in wasm-bridge tests

### M3: GeomRef Resolver — Role-Based ✅
- [x] Implement role-based resolution: anchor → OpResult → role_assignments → KernelId
- [x] Test: extrude produces EndCapPositive/Negative roles → resolve correctly
- [x] Test: fillet produces FilletFace roles → resolve correctly (via pipeline test)
- [x] Test: role not found → returns error

### M4: GeomRef Resolver — Signature-Based Fallback ✅
- [x] Implement signature similarity scoring (weighted fields: kind, area, normal, centroid, surface_type, adjacency_hash)
- [x] Implement signature matching: compute current signatures → find best match
- [ ] Test: after topology change, role fails → signature match succeeds (needs more complex test scenario)
- [x] Test: ambiguous signatures → BestEffort returns closest + warning
- [x] Test: no match → Strict returns error

### M5: Rebuild Algorithm ✅
- [x] Identify earliest dirty feature
- [x] Replay features from dirty point forward
- [x] Resolve GeomRefs before each operation (resolve_with_fallback in rebuild loop)
- [x] Store OpResult per feature
- [x] Handle resolve failures (Strict vs BestEffort)
- [ ] Trigger tessellation after rebuild (deferred: no UI consumer yet)
- [x] Test: change sketch dimension → verify rebuild produces correct geometry
- [x] Test: rebuild error on missing sketch reference

### M6: Undo/Redo ✅
- [x] Command pattern: AddFeature, RemoveFeature, EditFeature, ReorderFeature, SuppressFeature
- [x] Each command stores inverse (apply_inverse / apply_forward)
- [x] Undo stack + redo stack (UndoStack in src/undo.rs)
- [x] Redo stack cleared on new command
- [x] Rebuild after undo/redo
- [x] Test: add → undo → redo → verify state (8 undo/redo tests)

### M7: Rollback ✅
- [x] Set active_index → suppress features after index
- [x] Model state reflects the partial tree
- [ ] Slider UI integration (via EngineToUi messages — deferred to UI phase)
- [x] Test: set index → verify correct features are active (3 rollback tests)
- [x] Fix: active_features() panic on empty tree with rollback
- [x] Fix: rebuild clears results for features beyond rollback window

### M8: Integration Test — Full Pipeline ✅
- [x] Sketch → extrude → sketch → extrude → boolean union → verify all results
- [x] Edit early feature → verify downstream rebuild succeeds with no errors
- [x] Undo/redo edit → verify state roundtrips correctly
- [x] Rollback mid-tree → verify inactive features lose results, restore recovers
- [x] Fillet pipeline: sketch → extrude → fillet → verify FilletFace roles
- [x] Chamfer pipeline: sketch → extrude → chamfer → verify ChamferFace roles
- [x] Shell pipeline: sketch → extrude → shell → verify ShellInnerFace roles
- [x] Fillet survives extrude edit + downstream rebuild
- [x] Extrude provenance includes SideFace roles for edge resolution

### M9: Persistent Naming Stress Tests ✅
- [x] Add feature in middle of tree → verify downstream refs survive
- [x] Remove feature from middle → verify error on dependent features
- [x] Suppress feature → verify downstream errored; unsuppress recovers
- [x] Reorder features → verify UUID-based refs survive position changes
- [x] Reorder extrude before its sketch → verify failure
- [x] Multiple undo/redo cycle (3 adds, undo all, redo all)
- [ ] Change sketch that adds/removes edges → verify role fallback to signature (deferred: needs richer sketch editing)

### M10: Performance Benchmarks ✅
- [x] Rebuild time for 10-feature tree: ~180µs (with MockKernel)
- [x] Rebuild time for 20-feature tree: ~370µs
- [x] Rebuild time for 50-feature tree: ~1.3ms
- [x] All well under interactive thresholds; no hotspots at this scale

## Blockers

- Depends on kernel-fork (Kernel + KernelIntrospect traits, especially MockKernel)
- Depends on modeling-ops (OpResult production with provenance)
- Can start M1-M4 with mock OpResults before modeling-ops is ready

## Interface Change Requests

(None yet)

## Notes

- This is the hardest sub-project. GeomRef resolution is the core algorithm.
- Start with MockKernel. Do not wait for TruckKernel.
- The rebuild algorithm must be correct before it's fast. Optimize later.
- Persistent naming is a simplified version of commercial approaches. Document limitations honestly.
