# 02 — Sketch Solver: Plan

## Milestones

### M1: Dependency Setup
- [ ] Add `slvs` crate (v0.6.0) as dependency
- [ ] Verify build (requires clang, libclang, cmake)
- [ ] Create sketch-solver crate skeleton

### M2: Entity Mapping
- [ ] Map `SketchEntity::Point` → slvs `Point2d` on workplane
- [ ] Map `SketchEntity::Line` → slvs `LineSegment`
- [ ] Map `SketchEntity::Circle` → slvs `Circle`
- [ ] Map `SketchEntity::Arc` → slvs `Arc`
- [ ] Handle construction geometry (excluded from profiles, still solved)
- [ ] Unit tests: create each entity type, verify no solve errors

### M3: Constraint Mapping
- [ ] Map all geometric constraints (Coincident, Horizontal, Vertical, Parallel, Perpendicular, Tangent, Equal, Symmetric, SymmetricH, SymmetricV, Midpoint, OnEntity, SameOrientation)
- [ ] Map all dimensional constraints (Distance, Angle, Radius, Diameter, EqualAngle, Ratio, EqualPointToLine)
- [ ] Map Dragged constraint
- [ ] Unit tests: each constraint type individually

### M4: Solve + Position Extraction
- [ ] Run solver, extract solved positions → `SolvedSketch`
- [ ] Test: rectangle with width/height dimensions → verify positions
- [ ] Test: circle with center + radius → verify position
- [ ] Test: equilateral triangle with equal-length constraints

### M5: SolveStatus Detection
- [ ] Detect FullyConstrained (dof=0)
- [ ] Detect UnderConstrained (dof>0)
- [ ] Detect OverConstrained (conflicting constraints → failed constraint list)
- [ ] Detect SolveFailed (convergence failure)
- [ ] Unit tests for each status

### M6: Profile Extraction
- [ ] Build connectivity graph from solved sketch
- [ ] Find closed loops (simple cycle detection)
- [ ] Classify loops as outer/inner (winding direction)
- [ ] Return `Vec<ClosedProfile>`
- [ ] Test: rectangle → 1 outer profile
- [ ] Test: circle → 1 outer profile
- [ ] Test: rectangle with circle hole → 1 outer + 1 inner
- [ ] Test: slot shape (rectangle + semicircles) → 1 outer profile

### M7: Reference Sketch Tests
- [ ] Rectangle with dimensions: 4 lines + 4 coincident + 2 distance + 2 horizontal + 2 vertical → verify positions analytically
- [ ] Circle with center + radius → verify
- [ ] Slot (lines + tangent arcs) → verify
- [ ] Complex profile with tangent arcs → verify

### M8: Dragged Constraint for Interactive Use
- [ ] Implement dragged constraint workflow: set point position → add Dragged → solve → read result
- [ ] Test: drag a point in an under-constrained sketch → verify it moves while maintaining constraints
- [ ] Test: drag a point in a fully-constrained sketch → verify it stays put

### M9: Performance Benchmarking
- [ ] Benchmark solve time for 10, 50, 100, 300 constraints
- [ ] Document baseline performance
- [ ] Verify sub-millisecond for typical sketches

### M10: WASM Strategy
- [ ] Document Emscripten build process for libslvs
- [ ] Prototype two-module WASM approach
- [ ] Measure bridge overhead

## Blockers

(None yet)

## Interface Change Requests

(None yet)

## Notes

- The slvs crate vendors SolveSpace source as a git submodule — `cargo build` handles this automatically.
- clang + libclang + cmake must be installed for the build to work.
- The `Dragged` constraint is critical for interactive UX — Onshape uses this pattern extensively.
