# 02 — Sketch Solver: Handoff

## Status: Complete (M1–M10)

All milestones complete. 31 tests passing. Emscripten WASM build working.

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
- Feature gate: `native-solver` — libslvs C++ can't compile to wasm32-unknown-unknown
- Emscripten WASM build at `/home/claude/emsdk`, output in `app/static/pkg/slvs/`

### slvs bindgen fix:

The `slvs` 0.6.0 crate's `build.rs` passes `-x c++ -std=c++11` to bindgen's clang args.
This causes bindgen to silently fail with newer libclang — it only generates constants,
not struct/function bindings. The header (`slvs.h`) is pure C and doesn't need C++ mode.

**Fix**: `crates/slvs-patch/slvs-0.6.0/build.rs` — removed the three clang args
(`-x`, `c++`, `-std=c++11`), keeping only `-fvisibility=default`.

## No remaining work items for this sub-project.
