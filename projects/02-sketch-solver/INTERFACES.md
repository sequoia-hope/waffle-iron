# 02 — Sketch Solver: Interfaces

All sketch types (`Sketch`, `SketchEntity`, `SketchConstraint`, `SolvedSketch`,
`SolveStatus`, `ClosedProfile`, `SketchSolveReport`, `SketchOp`, `SketchEdit`,
`SketchOpError`) are defined in `crates/waffle-types` and re-exported here.

## Public API

```rust
// Solving
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch;

pub fn extract_profiles(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<ClosedProfile>;
```

Sketch operations live in `sketch_solver::ops`: `trim`, `trim_preview`,
`extend`, `offset`, `resolve_offset_chain`, `fillet`, `fillet_geometry`,
`fillet_default_radius`, `mirror`, `project`, `remove_entities`, `apply_edit`
and `apply_ops`. See `src/ops/mod.rs` for signatures.

## Contract

- `solve_sketch` is stateless and deterministic.
- `SolvedSketch.positions` is keyed by point entity id; `radii` by circle
  entity id. An arc's radius is its center-to-start distance.
- `OverConstrained.conflicts` holds indices into `sketch.constraints`,
  deduplicated, ordered by largest residual first.
- Reference dimensions do not drive the solve; their measured values are in
  `report.residuals`.
