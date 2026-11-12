# 02 — Sketch Solver: Architecture

`crates/sketch-solver` is the 2D geometric constraint solver and the home of
the sketch editing operations. It is pure Rust (Levenberg-Marquardt on
`nalgebra`) and compiles to `wasm32-unknown-unknown` as part of the single
WASM bundle. Its only workspace dependency is `waffle-types`, which defines
every type it consumes and produces.

Design spec: `specs/clean_room_constraint_solver.md`. Drag behaviour:
`specs/sketch_drag_stability.md`, `specs/pinned_constraint.md`.

## Solving

`solve_sketch(&Sketch) -> SolvedSketch` is stateless. Each call:

1. **Lays out parameters** (`entity_mapping.rs`, `ParamLayout`). A point is
   two parameters (x, y); a circle adds one (radius). Lines and arcs add none —
   they are defined by their point entities. Splines are skipped; gears and
   sprockets are expanded to primitives before they reach the solver.
2. **Compiles constraints** (`constraint_mapping.rs`). Each driving
   `SketchConstraint` becomes a `CompiledConstraint` with entity ids resolved
   to parameter indices. Each variant supplies its residual rows and an
   analytic Jacobian. A constraint that names an unknown entity or an
   unsupported entity combination fails the solve with `SolveFailed`.
   Reference (driven) dimensions are filtered out here and nowhere else.
3. **Minimizes** `Σ (wᵢ·rᵢ)²` with the `levenberg-marquardt` crate
   (`solver.rs`). Every constraint has weight 1 except `Dragged`, which has
   1/20: a drag is a hint, a `Pinned` point is a lock.
4. **Regularizes.** Every solve appends rows `ε·(x − x₀)` (ε = 1e-5) that
   anchor each parameter to its pre-solve value, so under-constrained geometry
   stays where the user left it instead of drifting along free directions.
   These rows are excluded from status classification.
5. **Classifies** the result by rank-revealing QR on the constraint rows of
   the final Jacobian:

   | Residual | Rank | Status |
   |---|---|---|
   | below 1e-6 | dof = 0 | `FullyConstrained` |
   | below 1e-6 | dof > 0 | `UnderConstrained { dof }` |
   | above | rank < rows | `OverConstrained { conflicts }` |
   | above | full rank | `SolveFailed { reason }` |

   A failed solve returns the input positions unchanged.
6. **Reports.** `SolvedSketch` carries solved point positions, solved radii,
   closed profiles, the status, and a `SketchSolveReport` (per-constraint
   residuals, conflicts, redundant constraints, moved points, the free
   directions, and why the minimizer stopped). Every constraint index in the
   result indexes the caller's own `sketch.constraints`, reference dimensions
   included.

The solve path is deterministic: parameters, residuals and Jacobian rows are
assembled in declaration order, and no `HashMap` is iterated.

## Profile extraction

`extract_profiles` delegates to `waffle_types::profiles::extract_profiles`,
the one implementation shared with the rest of the workspace.

## Sketch operations

`src/ops/` implements trim, extend, offset, fillet, mirror and projection as
pure functions on a `Sketch`. Each returns a `SketchEdit` or a typed
`SketchOpError`; nothing is a silent no-op. The sketch UI and the agent tools
both call these through the bridge, so there is one implementation.

- Operations reuse point ids wherever geometry survives, so constraints on the
  kept geometry survive the edit.
- Operations weld points only on exact coincidence (`POINT_WELD_TOL`, 1e-9).
  Snapping is the UI's decision; the caller passes the position it chose.
- `apply_ops` applies a list of `SketchOp` in order; `apply_edit` applies one
  `SketchEdit`.

## Files

| File | Contents |
|---|---|
| `src/solver.rs` | `solve_sketch`, LM problem, status classification, solve report |
| `src/entity_mapping.rs` | `ParamLayout` |
| `src/constraint_mapping.rs` | `CompiledConstraint`: compile, residuals, Jacobian |
| `src/profiles.rs` | `extract_profiles` (delegates to `waffle-types`) |
| `src/ops/mod.rs` | trim, extend, offset, fillet, mirror, project, `apply_ops` |
| `src/ops/chain.rs` | connected-chain discovery and ordering |
| `src/ops/offset.rs` | chain offset geometry |
| `src/ops/geom.rs` | 2D primitives and intersections |
