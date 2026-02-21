# BACKLOG.md

Waffle Iron prioritized task queue.
Pick from the top of Active. See CLAUDE.md for constraints and priorities.

Last reviewed: 2026-02-21

## Active (pick from top)

# --- P1: Boolean shapeops reliability (vendor/truck/) ---

- [ ] Add BooleanError enum + Result-returning API wrappers in truck-shapeops
- [ ] Wire BooleanError into KernelError in kernel-fork/src/truck_kernel.rs so failures carry stage info
- [ ] Add BooleanOptions struct with layered tolerances (tau_model/mesh/weld/coplanar) to kernel-fork/src/types.rs
- [ ] Wire BooleanOptions into TruckKernel boolean methods replacing compute_adaptive_tol
- [ ] Add `robust` crate dependency to truck-shapeops + wrapper module for orient3d/orient2d
- [ ] Replace naive float comparisons in ray_cast_classify with robust predicates
- [ ] Replace naive coplanar detection in coplanar.rs with robust predicates
- [ ] Diagnose chained boolean failure: why k1, k8, l4 extrude_chain tests fail (NotSimpleWire after auto-union)
- [ ] Improve wire splitting in weld_coincident_edges for 3+ chained boolean operations
- [ ] Add input validation layer: reject non-manifold/degenerate solids before boolean entry
- [ ] Add boolean debug artifact collection: dump meshes/curves on failure for offline diagnosis
- [ ] Write spec for boolean error types (/specs/boolean_error_types.md) per FIP

# --- P2: GUI test coverage (app/tests/gui/) ---

- [ ] GUI: Test fillet/chamfer/shell disabled state (warning banners visible, Apply buttons disabled)
- [ ] GUI: Extrude-cut workflow (draw sketch, toggle cut checkbox, apply, verify mesh)
- [ ] GUI: Revolve full workflow with axis quick-pick buttons (X/Y/Z)
- [ ] GUI: Revolve with sketch-line axis selection
- [ ] GUI: Constraint tools — distance constraint between two points, verify dimension label
- [ ] GUI: Constraint tools — angle constraint between two lines
- [ ] GUI: Constraint tools — parallel/perpendicular between lines
- [ ] GUI: Multi-feature workflow — sketch + extrude + sketch-on-face + second extrude
- [ ] GUI: Error path — extrude with no sketch selected shows error
- [ ] GUI: Error path — revolve with invalid axis shows error
- [ ] GUI: Keyboard shortcuts — Ctrl+Z/Ctrl+Shift+Z undo/redo verification
- [ ] GUI: Feature tree context menu (suppress, delete) with model state verification
- [ ] GUI: Property editor — change extrude depth, verify mesh updates

# --- P3: Cross-crate integration tests (crates/test-harness/) ---

- [ ] Integration: Revolve chain tests (sketch + revolve + extrude on revolved face)
- [ ] Integration: Revolve + boolean union (revolve body + box union)
- [ ] Integration: Multi-body workflow with explicit boolean combine (not auto-union)
- [ ] Integration: Sketch-on-face + extrude stacking (3 levels deep) via ModelBuilder
- [ ] Integration: Undo/redo stress test — 10 ops, undo all, redo all, verify final state
- [ ] Integration: Save/load round-trip with complex tree (5+ features)
- [ ] Integration: Cylinder geometry via TruckKernel (circle sketch + extrude = cylinder solid)

# --- P4: Extrude/revolve pipeline polish ---

- [ ] Fix revolve role heuristic — normal-axis dot product unreliable for real geometry (modeling-ops/src/revolve.rs)
- [ ] Wire sketch-on-face plane extraction from selected face GeomRef
- [ ] Write spec for sketch-on-face (/specs/sketch_on_face.md) per FIP
- [ ] Handle extrude-cut direction ambiguity — ensure cut direction points into existing solid

# --- P5: UI/UX gaps ---

- [ ] Add browser file dialogs for save/open (.waffle files)
- [ ] Add toast notification for boolean auto-union fallback
- [ ] Wire sketch-on-face button in ViewportContextMenu to plane extraction

# --- P6: Code quality ---

- [ ] Replace Option<Solid> with Result in remaining truck-shapeops public API
- [ ] Add rustdoc to kernel-fork public types and trait methods
- [ ] Workspace-wide cargo clippy audit and fix

## Parked (intentionally deferred)

- [ ] Fillet — TruckKernel (DEFERRED INDEFINITELY: depends on boolean reliability)
- [ ] Chamfer — TruckKernel (DEFERRED INDEFINITELY: depends on boolean reliability)
- [ ] Shell — TruckKernel (DEFERRED INDEFINITELY: depends on boolean reliability)
- [ ] Assemblies (Phase 7) — blocked on everything else
- [ ] XOR boolean operation — needs classification refactor first
- [ ] 2D polygon overlay for coplanar (iOverlay/Clipper2) — big scope, needs spec + research
- [ ] Adaptive mesh refinement for intersection construction — deep truck internals
- [ ] Per-edge/per-feature tolerance tracking — needs tolerance architecture first
- [ ] TouchingPolicy enum — needs commercial kernel research
- [ ] Performance benchmarks with regression tracking
- [ ] Fuzz testing for boolean operations (cargo-fuzz)
- [ ] STEP export in WASM (ruststep not wasm-compatible)
- [ ] Deterministic multi-ray classification — current majority voting sufficient

## Done (move here, don't delete)

- [x] Sprint 22: Wire splitting + vertex dedup — 42/45 extrude chain tests pass
- [x] Sprint 21: Revolve dialog axis quick-pick (X/Y/Z buttons)
- [x] Sprint 21: Extrude dialog sketch selector (pick any sketch, add profiles)
- [x] Sprint 7: Feature-aware tolerance (compute_adaptive_tol + solid_min_edge_length)
- [x] Sprint 6: NURBS arc healing for IC edges
- [x] Sprint 5: Boolean remediation (ray-cast parity, BooleanOptions, weld tol)
- [x] Sprint 4: Coplanar fixes (boundary filter, coplanar classifier, edge welding)
- [x] Fix orbit on empty space — gate BoxSelect on Shift key
- [x] Documentation cleanup: defer fillet/chamfer/shell, update STATUS.md
