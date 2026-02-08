# 02 — Sketch Solver: Interfaces

## Types This Crate IMPLEMENTS

| Type | Role |
|------|------|
| Solving logic | Maps SketchEntity/SketchConstraint to slvs calls, runs solver |
| `SolvedSketch` | Output: solved positions + profiles + status |
| `ClosedProfile` | Extracted closed loops from solved geometry |
| `SolveStatus` | Solver result classification |

## Types This Crate CONSUMES

| Type | Source | Purpose |
|------|--------|---------|
| `Sketch` | INTERFACES.md (feature-engine) | Input sketch to solve |
| `SketchEntity` | INTERFACES.md | Geometric entities to map to slvs |
| `SketchConstraint` | INTERFACES.md | Constraints to map to slvs |
| `GeomRef` | INTERFACES.md | Sketch plane reference |

## Public API

```rust
/// Solve a sketch: map entities/constraints to slvs, run solver, extract results.
pub fn solve_sketch(sketch: &Sketch) -> SolvedSketch;

/// Extract closed profiles from a solved sketch.
/// Called automatically by solve_sketch, but also available standalone.
pub fn extract_profiles(
    entities: &[SketchEntity],
    positions: &HashMap<u32, (f64, f64)>,
) -> Vec<ClosedProfile>;
```

## Notes

- This crate does NOT depend on kernel-fork. It operates purely in 2D sketch space.
- The solver is stateless — each `solve_sketch` call creates a fresh slvs System.
- Profile extraction is a graph algorithm on solved positions, not a geometric computation.
