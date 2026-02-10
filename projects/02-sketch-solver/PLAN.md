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
- [ ] Test: slot shape (rectangle + semicircles) → 1 outer profile (deferred: requires arc tangent setup)

### M7: Reference Sketch Tests ✅
- [x] Rectangle with dimensions: 4 lines + h/v constraints + 2 distance + dragged origin → verify positions analytically
- [x] Circle with center + radius → verify
- [x] Square with equal-length constraints → verify
- [x] Perpendicular lines → verify
- [x] Parallel lines → verify
- [x] Midpoint constraint → verify
- [x] Symmetric about line → verify
- [ ] Slot (lines + tangent arcs) → verify (deferred: requires arc tangent setup)

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

## Blockers

- **SymmetricH/SymmetricV semantics**: The slvs crate's `SymmetricVert` and `SymmetricHoriz` constraints have naming that may not match intuitive expectations. `SymmetricVert` appears to enforce same-x (not mirrored-x). The `Symmetric` (about a line) constraint works correctly and is the primary symmetric constraint for sketch use. Further investigation needed if SymmetricH/V are used in the UI.

## Interface Change Requests

(None yet)

## Notes

- The slvs crate vendors SolveSpace source as a git submodule — `cargo build` handles this automatically.
- clang + libclang + cmake must be installed for the build to work.
- The `Dragged` constraint is critical for interactive UX — Onshape uses this pattern extensively.
- The slvs 0.6.0 build.rs needed patching: removed `-x c++ -std=c++11` clang args that broke bindgen with newer libclang. Fix is in `crates/slvs-patch/slvs-0.6.0/build.rs`.
- 31 tests covering: solve + position extraction, status detection, profile extraction, reference sketches, dragged constraint, edge cases, and performance benchmarks.
