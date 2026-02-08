# 06 — Feature Engine: Plan

## Milestones

### M1: Feature Tree Data Structure
- [ ] `FeatureTree` struct (ordered Vec<Feature> + active_index)
- [ ] Add feature (append + insert at position)
- [ ] Remove feature
- [ ] Reorder features (move up/down)
- [ ] Suppress/unsuppress feature
- [ ] Set active_index (rollback)
- [ ] Unit tests for all mutations

### M2: GeomRef + Anchor + Selector Types
- [ ] Implement GeomRef, Anchor, Selector, ResolvePolicy (from INTERFACES.md)
- [ ] GeomRef constructors for common cases (role-based, signature-based)
- [ ] Serde serialization tests (round-trip)

### M3: GeomRef Resolver — Role-Based
- [ ] Implement role-based resolution: anchor → OpResult → role_assignments → KernelId
- [ ] Test: extrude produces EndCapPositive/Negative roles → resolve correctly
- [ ] Test: fillet produces FilletFace roles → resolve correctly
- [ ] Test: role not found → falls through to next strategy

### M4: GeomRef Resolver — Signature-Based Fallback
- [ ] Implement signature similarity scoring (weighted fields: area, normal, centroid, surface_type, adjacency_hash)
- [ ] Implement signature matching: compute current signatures → find best match
- [ ] Test: after topology change, role fails → signature match succeeds
- [ ] Test: ambiguous signatures → BestEffort returns closest + warning
- [ ] Test: no match → Strict returns error

### M5: Rebuild Algorithm
- [ ] Identify earliest dirty feature
- [ ] Replay features from dirty point forward
- [ ] Resolve GeomRefs before each operation
- [ ] Store OpResult per feature
- [ ] Handle resolve failures (Strict vs BestEffort)
- [ ] Trigger tessellation after rebuild
- [ ] Test: change sketch dimension → verify rebuild produces correct geometry

### M6: Undo/Redo
- [ ] Command pattern: AddFeature, RemoveFeature, EditFeature, ReorderFeature, SuppressFeature
- [ ] Each command stores inverse
- [ ] Undo stack + redo stack
- [ ] Redo stack cleared on new command
- [ ] Rebuild after undo/redo
- [ ] Test: add → undo → redo → verify state

### M7: Rollback
- [ ] Set active_index → suppress features after index
- [ ] Model state reflects the partial tree
- [ ] Slider UI integration (via EngineToUi messages)
- [ ] Test: set index → verify correct features are active

### M8: Integration Test — Full Pipeline
- [ ] Sketch rectangle → extrude → fillet top edges → change sketch width → verify:
  - Rebuild succeeds
  - Fillet references still resolve (role: FilletFace)
  - Final geometry is correct (different width, same fillets)
- [ ] Test with MockKernel

### M9: Persistent Naming Stress Tests
- [ ] Add feature in middle of tree → verify downstream refs survive
- [ ] Remove feature from middle → verify error on dependent features
- [ ] Suppress feature → verify downstream suppressed or errored
- [ ] Reorder features → verify refs update correctly
- [ ] Change sketch that adds/removes edges → verify role fallback to signature

### M10: Performance Benchmarks
- [ ] Rebuild time for 10-feature tree (with MockKernel)
- [ ] Rebuild time for 20-feature tree
- [ ] Rebuild time for 50-feature tree
- [ ] Identify hotspots (GeomRef resolution? kernel ops? tessellation?)

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
