# Wave 4 / Fork F: Eliminate JS Solver + Feature Gate

**Executor**: Claude fork (worktree), with Gemini workers
**Depends on**: Wave 3 (working pure-Rust solver)
**Parallel with**: Fork D (proptest), Fork E (LM fallback)
**Estimated scope**: ~50 lines changed, ~2000 lines deleted

## Goal

Remove the dual-codepath: eliminate the JS/Emscripten solver, the
`native-solver` feature gate, and the slvs-patch vendored crate.
This is spec Phase 2.

## Worker Breakdown

### Worker F1: Remove feature gate from wasm-bridge

**File: `crates/wasm-bridge/Cargo.toml`**
- Change `sketch-solver = { path = "../sketch-solver", optional = true }`
  to `sketch-solver = { path = "../sketch-solver" }`
- Remove `[features]` section (or remove `native-solver` feature)

**File: `crates/wasm-bridge/src/dispatch.rs`**
- Remove `#[cfg(feature = "native-solver")]` around SolveSketch handler
- Remove the `#[cfg(not(feature = "native-solver"))]` fallback block
- SolveSketch now unconditionally calls `sketch_solver::solve_sketch()`

**File: `crates/wasm-bridge/tests/bridge_tests.rs`**
- Remove `#[cfg(feature = "native-solver")]` from test functions

### Worker F2: Remove JS solver files

Delete:
- `app/src/lib/engine/slvs-solver.js`
- `app/static/pkg/slvs/slvs.wasm`
- `app/static/pkg/slvs/slvs.js`
- `app/static/pkg/slvs/` directory

**File: worker.js (or equivalent)**
- Find and remove the `SolveSketchLocal` intercept that routes to the
  JS solver
- All SolveSketch messages now go through WASM bridge

### Worker F3: Remove slvs-patch

- Delete `crates/slvs-patch/` directory
- Remove `[patch.crates-io]` entry for `slvs` in root `Cargo.toml`
- Verify no remaining references to `slvs` in any Cargo.toml

### Worker F4: Verification

- `cargo check -p wasm-bridge` — compiles without feature flag
- `cargo check -p wasm-bridge --no-default-features` — also compiles
  (no more optional sketch-solver)
- `cargo test -p wasm-bridge` — all tests pass
- `grep -r "native-solver" crates/` — no results
- `grep -r "slvs" crates/ --include="*.toml"` — no results
- `grep -r "slvs-solver" app/` — no results

## Deliverables

- Modified `crates/wasm-bridge/Cargo.toml`
- Modified `crates/wasm-bridge/src/dispatch.rs`
- Modified `crates/wasm-bridge/tests/bridge_tests.rs`
- Modified worker.js
- Modified root `Cargo.toml`
- Deleted `crates/slvs-patch/`
- Deleted JS solver files

## Verification

- `cargo test -p wasm-bridge` — pass
- No `slvs` or `native-solver` references remain
- `cargo build -p sketch-solver` — no C/C++ compilation step
