# BACKLOG.md

Waffle Iron prioritized task queue.
Pick from the top of Active. See CLAUDE.md for constraints and priorities.

Last reviewed: 2026-02-21

## Active (pick from top)

# --- P1: Kernel implementation (crates/kernel/) ---

- [x] **HP-1**: Auto-union fails for 3+ chained abutting extrudes — FIXED: incremental rebuild consumption tracking bug in rebuild.rs
- [x] **HP-2**: Cut operation splits previously-unioned body into fragments — FIXED: same root cause as HP-1
- [x] ~~Add BooleanError enum + Result-returning API wrappers in truck-shapeops~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Wire BooleanError into KernelError in kernel-fork/src/truck_kernel.rs~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Add BooleanOptions struct with layered tolerances to kernel-fork/src/types.rs~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Wire BooleanOptions into TruckKernel boolean methods~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Add `robust` crate dependency to truck-shapeops~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Replace naive float comparisons in ray_cast_classify with robust predicates~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Replace naive coplanar detection in coplanar.rs with robust predicates~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Diagnose chained boolean failure~~ SUPERSEDED by clean-sheet kernel
- [x] ~~Improve wire splitting in weld_coincident_edges~~ SUPERSEDED by clean-sheet kernel
- [ ] Implement analytic SSI solvers for quadric surface pairs (plane-plane, plane-cylinder, cylinder-cylinder, etc.)
- [ ] Boolean face classification using winding numbers
- [ ] Input validation layer: reject non-manifold/degenerate solids before boolean entry
- [ ] Boolean debug artifact collection: dump meshes/curves on failure for offline diagnosis

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
- [ ] Integration: Cylinder geometry via WaffleKernel (circle sketch + extrude = cylinder solid)

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

- [ ] Add rustdoc to kernel public types and trait methods
- [ ] Workspace-wide cargo clippy audit and fix

## Parked (intentionally deferred)

- [ ] Fillet — WaffleKernel (DEFERRED INDEFINITELY: depends on boolean reliability)
- [ ] Chamfer — WaffleKernel (DEFERRED INDEFINITELY: depends on boolean reliability)
- [ ] Shell — WaffleKernel (DEFERRED INDEFINITELY: depends on boolean reliability)
- [ ] Assemblies (Phase 7) — blocked on everything else
- [ ] XOR boolean operation — needs classification refactor first
- [ ] 2D polygon overlay for coplanar (iOverlay/Clipper2) — big scope, needs spec + research
- [ ] Adaptive mesh refinement for intersection construction — deep kernel internals
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
