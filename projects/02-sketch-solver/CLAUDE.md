# 02 — Sketch Solver: Agent Instructions

You are working on **sketch-solver**. Read ARCHITECTURE.md in this directory first.

## Your Job

This crate wraps the `slvs` crate (which itself wraps SolveSpace's libslvs) to provide 2D constraint solving for Waffle Iron sketches. Map our `SketchEntity`/`SketchConstraint` types to slvs calls, run the solver, and produce `SolvedSketch` output.

## Build Requirements

You need a C compiler (clang or gcc), libclang, and cmake installed. The slvs crate's `build.rs` uses `cc` to compile libslvs from vendored source and `bindgen` to generate Rust bindings.

```bash
# Ubuntu/Debian
sudo apt-get install clang libclang-dev cmake
```

## Build & Test

```bash
cargo test -p sketch-solver
cargo clippy -p sketch-solver
```

## Critical Points

1. **Map ALL constraint types.** The slvs crate supports 26+ constraint types. Our `SketchConstraint` enum covers all of them. Map each one.
2. **Test with known-good sketches** where solved positions can be verified analytically. A 100mm x 50mm rectangle should produce corners at exactly (0,0), (100,0), (100,50), (0,50).
3. **The `Dragged` constraint is critical for interactive UX.** It tells the solver "move this point as close as possible to its current position while satisfying all other constraints." This is how Onshape implements drag-to-constrain.
4. **Profile extraction** — analyze solved geometry to find closed loops. This is what gets extruded.
5. **SolveStatus** — distinguish fully constrained, under-constrained (with DOF count), over-constrained (with conflict list), and solve failure.

## Key Files

- `src/lib.rs` — Public API (`solve_sketch`, `extract_profiles`)
- `src/entity_mapping.rs` — SketchEntity → slvs entity mapping
- `src/constraint_mapping.rs` — SketchConstraint → slvs constraint mapping
- `src/solver.rs` — Solve orchestration
- `src/profiles.rs` — Closed profile extraction
- `src/status.rs` — SolveStatus detection

## Dependencies

- `slvs` crate (v0.6.0)
- No dependencies on other Waffle Iron crates (this is a leaf dependency, parallel with kernel-fork)
