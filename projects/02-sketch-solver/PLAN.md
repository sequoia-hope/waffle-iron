# 02 — Sketch Solver: Plan

## Milestones

### M1: Dependency Setup ✅
- [x] Add `slvs` crate (v0.6.0) as dependency
- [x] Verify build (requires clang, libclang, cmake)
- [x] Create sketch-solver crate skeleton
- [x] Patch slvs build.rs to fix bindgen C++ mode issue

### M2: Entity Mapping ✅
- [x] Map `SketchEntity::Point` → slvs `Point2d` on workplane
- [x] Map `SketchEntity::Line` → slvs `LineSegment`
- [x] Map `SketchEntity::Circle` → slvs `Circle`
- [x] Map `SketchEntity::Arc` → slvs `Arc`
- [x] Handle construction geometry (excluded from profiles, still solved)
- [x] Unit tests: create each entity type, verify no solve errors

### M3: Constraint Mapping ✅
- [x] Map all geometric constraints (Coincident, Horizontal, Vertical, Parallel, Perpendicular, Tangent, Equal, Symmetric, SymmetricH, SymmetricV, Midpoint, OnEntity, SameOrientation)
- [x] Map all dimensional constraints (Distance, Angle, Radius, Diameter, EqualAngle, Ratio, EqualPointToLine)
- [x] Map Dragged constraint
- [x] Unit tests: each constraint type individually

### M4: Solve + Position Extraction ✅
- [x] Run solver, extract solved positions → `SolvedSketch`
- [x] Test: rectangle with width/height dimensions → verify positions
- [x] Test: circle with center + radius → verify position
- [x] Test: equilateral triangle with equal-length constraints

### M5: SolveStatus Detection ✅
- [x] Detect FullyConstrained (dof=0)
- [x] Detect UnderConstrained (dof>0)
- [x] Detect OverConstrained (conflicting constraints → failed constraint list)
- [x] Detect SolveFailed (convergence failure)
- [x] Unit tests for each status

### M6: Profile Extraction ✅
- [x] Build connectivity graph from solved sketch
- [x] Find closed loops (half-edge traversal with angle-sorted adjacency)
- [x] Classify loops as outer/inner (winding direction via shoelace formula)
- [x] Return `Vec<ClosedProfile>`
- [x] Test: rectangle → 1 outer profile
- [x] Test: circle → 1 outer profile
- [x] Test: rectangle with circle hole → outer + circle profiles found
- [ ] Test: slot shape (rectangle + semicircles) → 1 outer profile (ArcLineTangent constraint is implemented; test not yet written)

### M7: Reference Sketch Tests ✅
- [x] Rectangle with dimensions: 4 lines + h/v constraints + 2 distance + dragged origin → verify positions analytically
- [x] Circle with center + radius → verify
- [x] Square with equal-length constraints → verify
- [x] Perpendicular lines → verify
- [x] Parallel lines → verify
- [x] Midpoint constraint → verify
- [x] Symmetric about line → verify
- [ ] Slot (lines + tangent arcs) → verify (ArcLineTangent constraint is implemented; test not yet written)

### M8: Dragged Constraint for Interactive Use ✅
- [x] Implement dragged constraint workflow: set point position → add Dragged → solve → read result
- [x] Test: drag a point in an under-constrained sketch → verify distance maintained
- [x] Test: drag a point in a fully-constrained sketch → verify rectangle forms correctly

### M9: Performance Benchmarking ✅
- [x] Benchmark solve time for 14, 49, 105, 301 constraints (chain of connected rectangles)
- [x] Document baseline performance:
  - 14 constraints: ~1.6ms (2 rectangles)
  - 49 constraints: ~2.9ms (7 rectangles)
  - 105 constraints: ~5.8ms (15 rectangles)
  - 301 constraints: ~8.7ms (43 rectangles)
- [x] All sub-10ms, well within interactive thresholds

### M10: WASM Strategy ✅
- [x] Document Emscripten build process for libslvs (WASM_STRATEGY.md)
- [x] Document two-module WASM architecture (Rust via wasm-pack + libslvs via Emscripten)
- [x] Analyze bridge overhead (<0.1ms, negligible vs. solve time)
- [x] Projected WASM solve times: 2-15ms for 14-301 constraints (within interactive budget)
- [x] Emscripten build: slvs.wasm (226KB) + slvs.js (15KB) via em++ with Emscripten 5.0.0
- [x] JS bridge: slvs-solver.js maps SketchEntity/SketchConstraint to slvs C API structs
- [x] Worker integration: SolveSketchLocal message type bypasses Rust engine, calls libslvs directly

### M11: Drag Stability + Pinned Constraint ✅ (2026-07-05)

Bug-fix cycle for the two-rectangle drag explosion + origin-pin drift
(specs/sketch_drag_stability.md, specs/pinned_constraint.md). Commits
c3939caa, 085ee94e, 1e723cdb, 9a8d3a22.

- [x] Proximal regularization (ε=1e-5 rows `ε·(x−x₀)`) — LM can no longer
      walk free-DOF geometry out along flat null-space valleys (was: 10mm →
      4e8 in two pointermoves with a second unconstrained rectangle present)
- [x] Classification sliced to constraint rows (dof/status semantics unchanged)
- [x] SolveFailed echoes input positions (failed solve is inert)
- [x] LM xtol decoupled from SOLVE_TOL (1e-12 backstop; the relative-step stop
      was misclassifying satisfiable post-drag solves as SolveFailed)
- [x] `SketchConstraint::Pinned {point,x,y}` (weight 1.0, target authoritative);
      bridge lowers non-drag WhereDragged→Pinned (origin snap is a real lock)
- [x] Store apply-gate (SolveFailed / non-finite results never enter sketch
      state) + auto-fit clamps (frustumHalf ≤ maxDistance·2, finite extents)
- [x] Sketch undo restores camera (snapshot in every undo record,
      'waffle-restore-camera' event, lastAutoFitExtent re-armed); drags push
      real undo records (positionsBefore/After) and redo re-applies them
- [x] Tests: 14 new Rust (drag_stability_tests, pinned_constraint_tests) + 5
      new GUI tests; key branches mutation-verified

**M11b (2026-07-05): second explosion mechanism — drag ↔ auto-fit camera
feedback loop.** User's sketch.waffle still exploded intermittently after the
solver fix: mid-drag auto-fit rescaled the pointer→sketch mapping →
drag target teleported outward → geometry grew → fit again (exponential,
26mm → 4.4m in one gesture, solver healthy throughout). Fix: sketch auto-fit
gated on the new `sketchDragActive` store flag (I6 pointer-mapping
stability, spec §4b); fit runs on release. Regression:
sketch-drag-autofit-feedback.spec.js (real pointer, reproduction document in
tests/gui/fixtures/) + waffle_repro.rs solver-robustness hunts.

Follow-ups (discovered, not blocking):
- [x] **Pin persistence** (2026-07-05): FinishSketch now lowers persistent
      WhereDragged → Pinned{point,x,y} into the feature (both edit + new
      paths); enterSketchEditMode upconverts Pinned → WhereDragged so the
      in-session format stays uniform (badges/snap/deletion untouched).
      Spec B6; GUI sketch-pin-persistence.spec.js (finish→re-edit→lock still
      holds, double round-trip idempotent, pin-less docs unchanged); both
      boundary branches mutation-verified.
- [ ] **Conflict index mapping**: solver.rs find_conflict_constraints returns
      residual ROW indices; the UI consumes them as CONSTRAINT indices →
      wrong over-constraint badges when multi-row constraints precede the
      conflict.
- [ ] **Hard pin elimination**: remove pinned coords from the parameter
      vector for exactly-zero sag during drags (soft weight-1.0 pin sags
      (w_drag/w_pin)²·offset ≈ 0.25% of drag offset — invisible, deferred).
- [ ] Pre-existing GUI reds (NOT from this cycle, both deterministic):
      dimension-tool.spec.js:241 (mm vs m, known) and
      snap-preview-candidates.spec.js:143 (active-snap filter; fails with
      pre-cycle JS too — snap path is pure JS, no WASM involvement).

## Blockers

- **SymmetricH/SymmetricV semantics**: The slvs crate's `SymmetricVert` and `SymmetricHoriz` constraints have naming that may not match intuitive expectations. `SymmetricVert` appears to enforce same-x (not mirrored-x). The `Symmetric` (about a line) constraint works correctly and is the primary symmetric constraint for sketch use. Further investigation needed if SymmetricH/V are used in the UI.

## Interface Change Requests

(None yet)

## Notes

- The slvs crate vendors SolveSpace source as a git submodule — `cargo build` handles this automatically.
- clang + libclang + cmake must be installed for the build to work.
- The `Dragged` constraint is critical for interactive UX — Onshape uses this pattern extensively.
- The slvs 0.6.0 build.rs needed patching: removed `-x c++ -std=c++11` clang args that broke bindgen with newer libclang. Fix is in `crates/slvs-patch/slvs-0.6.0/build.rs`.
- 59 tests covering: solve + position extraction, status detection, profile extraction, reference sketches, dragged constraint, edge cases, and performance benchmarks.
