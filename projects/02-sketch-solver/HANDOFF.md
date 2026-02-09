# 02 — Sketch Solver: Work-in-Progress Handoff

## Status Summary

All source files for milestones M1–M6 are **written and compiling** inside the Docker
container. Zero clippy warnings, clean formatting. Tests (M4–M7) are not yet written.

## What's Done

### Source files (all under `crates/sketch-solver/`):

| File | Status | Description |
|------|--------|-------------|
| `Cargo.toml` | Done | Dependencies: slvs 0.6 (patched), serde, uuid, thiserror |
| `src/lib.rs` | Done | Public API re-exports |
| `src/types.rs` | Done | All types from INTERFACES.md |
| `src/entity_mapping.rs` | Done | SketchEntity → slvs entity mapping |
| `src/constraint_mapping.rs` | Done | All 21 SketchConstraint variants → slvs constraints |
| `src/solver.rs` | Done | `solve_sketch()` orchestration + position extraction |
| `src/status.rs` | Done | SolveStatus classification from slvs SolveResult |
| `src/profiles.rs` | Done | Closed profile extraction (planar graph face-finding) |

### Build infrastructure:

- `Cargo.toml` (root): `sketch-solver` in workspace + `[patch.crates-io]` for patched slvs
- `crates/slvs-patch/slvs-0.6.0/`: Local copy of slvs with fixed build.rs
- `Dockerfile`: Extends claude-remote with Rust + C/C++ toolchain
- `docker-compose.yml`: Project-specific compose with cargo cache volumes
- `claude-remote/`: Git submodule for the base container

### slvs bindgen fix:

The `slvs` 0.6.0 crate's `build.rs` passes `-x c++ -std=c++11` to bindgen's clang args.
This causes bindgen to silently fail with newer libclang — it only generates constants,
not struct/function bindings. The header (`slvs.h`) is pure C and doesn't need C++ mode.

**Fix**: `crates/slvs-patch/slvs-0.6.0/build.rs` — removed the three clang args
(`-x`, `c++`, `-std=c++11`), keeping only `-fvisibility=default`.

## What's NOT Done

- **Tests** (M4–M7) — not yet written
- **PLAN.md** — not yet updated with completed milestones

## Resume Checklist

1. Write tests (M4–M7):
   - Rectangle solve with analytical position verification
   - Circle with center + radius
   - Status detection (fully/under/over-constrained, solve failure)
   - Profile extraction (rectangle → 1 outer, circle → 1 outer, rect+hole → outer+inner)
   - Reference sketch tests (slot shape, complex profiles)
2. `cargo test -p sketch-solver && cargo clippy -p sketch-solver`
3. Update `projects/02-sketch-solver/PLAN.md`
