# 02 — Sketch Solver: Architecture

## Purpose

Integrate the [`slvs` crate](https://crates.io/crates/slvs) (v0.6.0) — type-safe Rust bindings for SolveSpace's libslvs — to provide 2D geometric constraint solving for Waffle Iron sketches.

## The slvs Crate

The `slvs` crate provides:
- **Complete Rust bindings** for libslvs with phantom-typed handles (`EntityHandle<T>`, `ConstraintHandle<T>`)
- **cc + bindgen build** — compiles libslvs from vendored C source (git submodule)
- **Serde serialization** for entity/constraint data
- **All 26+ constraint types** from SolveSpace

Build requirements: C compiler (clang/gcc), libclang (for bindgen), cmake.

## Integration Architecture

Our job is to map Waffle Iron's `SketchEntity`/`SketchConstraint` types (defined in INTERFACES.md) to slvs API calls, run the solver, and produce `SolvedSketch` output.

### Workflow

1. **Create** an `slvs::System`.
2. **Add a Group** — all sketch entities and constraints go in one group.
3. **Map SketchEntity** → slvs entities:
   - `SketchEntity::Point` → `system.add_point_2d(group, x, y)` on the workplane
   - `SketchEntity::Line` → `system.add_line_segment(group, point_a, point_b)`
   - `SketchEntity::Circle` → `system.add_circle(group, center, radius, normal, workplane)`
   - `SketchEntity::Arc` → `system.add_arc(group, center, start, end, normal, workplane)`
4. **Map SketchConstraint** → slvs constraints:
   - Each SketchConstraint variant maps to a specific `system.add_constraint_*()` call
   - Dimensional constraints (Distance, Angle, Radius) pass the value parameter
   - Geometric constraints (Coincident, Parallel, etc.) are value-free
5. **Solve** — `system.solve(group)` runs Newton-Raphson with Gaussian elimination
6. **Extract results:**
   - Read solved positions from entity handles
   - Determine SolveStatus from solve result (Ok{dof} → FullyConstrained/UnderConstrained, Fail → OverConstrained/SolveFailed)
   - Extract closed profiles from solved geometry

### Constraint Type Mapping

| SketchConstraint | slvs function |
|-----------------|---------------|
| Coincident | `coincident` |
| Horizontal | `horizontal` |
| Vertical | `vertical` |
| Parallel | `parallel` |
| Perpendicular | `perpendicular` |
| Tangent | `tangent` |
| Equal | `equal_length_lines` / `equal_radius` |
| Symmetric | `symmetric` |
| SymmetricH | `symmetric_horiz` |
| SymmetricV | `symmetric_vert` |
| Midpoint | `at_midpoint` |
| Distance | `pt_pt_distance` / `pt_line_distance` |
| Angle | `angle` |
| Radius | `diameter` (value * 2) |
| Diameter | `diameter` |
| OnEntity | `pt_on_line` / `pt_on_circle` |
| Dragged | `dragged` |
| EqualAngle | `equal_angle` |
| Ratio | `length_ratio` |
| EqualPointToLine | `equal_pt_ln_distances` |
| SameOrientation | `same_orientation` |

### SolveStatus Detection

- `SolveResult::Ok { dof: 0 }` → `SolveStatus::FullyConstrained`
- `SolveResult::Ok { dof: n }` where n > 0 → `SolveStatus::UnderConstrained { dof: n }`
- `SolveResult::Fail { reason: Inconsistent, failed_constraints }` → `SolveStatus::OverConstrained { conflicts }`
- `SolveResult::Fail { reason: other, .. }` → `SolveStatus::SolveFailed { reason }`

### Profile Extraction

After solving, analyze the sketch geometry to find closed loops:

1. Build a graph of connected line/arc/circle segments (using coincident endpoints).
2. Find all simple cycles in the graph (closed loops).
3. Classify each loop as outer (counter-clockwise) or inner (clockwise, i.e., a hole).
4. Return as `Vec<ClosedProfile>`.

This is a graph algorithm problem, not a geometric one — the solver has already positioned everything.

### Performance

Sub-millisecond for typical sketches (10–300 constraints). Newton-Raphson with Gaussian elimination. The constraint solver is never a bottleneck.

## WASM Compilation Strategy

### Short-term: Two WASM Modules

libslvs is C code compiled via Emscripten to its own WASM module. The Rust sketch-solver is compiled via wasm-bindgen to a separate WASM module. They are bridged via JavaScript in the Web Worker.

Constraint solving is infrequent (user adds a constraint → solve → display result), so the bridge overhead is negligible.

### Long-term Options

1. **Port solver to pure Rust** (~5K LOC of numerical code). Eliminates the two-module problem.
2. **Compile libslvs with `cc` crate for `wasm32-unknown-unknown`** — fragile but may work for pure-computation C code that doesn't use libc.

### Precedent

SolveSpace has been compiled to WASM via Emscripten (experimental web edition). `JacobStoren/SolveSpaceLib` provides isolated solver extraction.
