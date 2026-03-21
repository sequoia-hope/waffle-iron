# Waffle Iron — Testing Guide

## Test Tiers

### Rust Fast (~420 tests, <30s)

Fast unit and integration tests using MockKernel and pure logic. Covers:

- **waffle-types** — Shared type definitions
- **sketch-solver** — Constraint solver logic
- **feature-engine** — Feature tree and rebuild pipeline
- **modeling-ops** — Modeling operation dispatching
- **wasm-bridge** — WASM API surface
- **file-format** — Serialization/deserialization
- **kernel-fork** — Mock/types/primitives/tessellation modules only (no truck booleans)
- **test-harness** — Fast binaries: `scenarios_mock`, `workflow_tests`, `oracle_tests`, `report_tests`, `scenarios_advanced`, `stl_tests`

### Rust Full (~910 tests, <5min)

All Rust crates including slow tests:

- Everything in Rust Fast, plus:
- **kernel-fork** — Full crate including truck boolean tests
- **test-harness** — All binaries including `scenarios_truck`, `advanced_scenarios`, `extrude_chains`, `boolean_workflows`, `boolean_failures`

### GUI Fast (~36 spec files, <2min)

Quick smoke tests for sketch drawing, UI chrome, and basic interactions:

- Sketch drawing (lines, rectangles, circles, arcs, polygons)
- Feature tree interactions
- Keyboard shortcuts and tool switching
- Snapping and grid behavior
- Dimension input and constraints
- Basic viewport interactions

### GUI Full (~55 spec files, <5min)

Everything in GUI Fast, plus heavy workflow and infrastructure specs:

- WASM engine integration (extrude, revolve, pipeline)
- Multi-feature workflow tests
- Infrastructure and dev tooling specs
- Advanced scenario tests

## Running Tests

```bash
./scripts/test.sh fast       # Rust fast tier (~30s)
./scripts/test.sh full       # All Rust tests (~5min)
./scripts/test.sh gui-fast   # Quick GUI smoke tests
./scripts/test.sh gui-full   # All GUI tests
./scripts/test.sh all-fast   # Rust fast + GUI fast
./scripts/test.sh all        # Everything
./scripts/test.sh profile    # Run timing profiler
```

## Tier Assignment Rules

### Rust Tests

| Condition | Tier |
|-----------|------|
| Uses `ModelBuilder::mock()` or `MockKernel` | Fast |
| Uses `ModelBuilder::truck()` or `TruckKernel` | Full (slow) |
| test-harness binary with "truck", "advanced", "chains", "boolean_workflows", "boolean_failures" | Full |
| Pure type/logic tests (no kernel) | Fast |

### GUI Tests

| Condition | Tier |
|-----------|------|
| Tests pure UI (sketch drawing, feature tree, keyboard) | Fast |
| Tests involving WASM engine (extrude, revolve, pipeline, workflow) | Full |
| Infrastructure/dev tooling specs | Full |

## Adding New Tests

### New Rust Crate

Add the crate name to the appropriate array in `scripts/test.sh`:

- `FAST_CRATES` — for crates with only fast tests
- The full test run includes all crates automatically

### New test-harness Binary

Add the binary name to either:

- `FAST_HARNESS_BINS` — if it only uses MockKernel
- The full list runs all binaries automatically

### New GUI Spec File

- Add to the `GUI_FAST_SPECS` array in `scripts/test.sh` if it tests pure UI
- Otherwise it will be included automatically in `gui-full`

## Profiling

Run profiling scripts to identify slow tests:

```bash
./scripts/profile-rust.sh    # Per-crate Rust timing
./scripts/profile-gui.sh     # Per-spec GUI timing
```

Results are saved to:

- `test-timings-rust.log` — Rust crate-by-crate timing data
- `test-timings-gui.json` — GUI spec-by-spec timing data

These files are gitignored and not committed.

## WASM Crash Detection in GUI Tests

### `catch_unwind` works with panic=unwind

The WASM binary is built with `cargo +nightly` and `-Zbuild-std`, which enables
`panic=unwind` on `wasm32-unknown-unknown` (see `.cargo/config.toml`). This makes
`std::panic::catch_unwind` actually catch truck boolean panics instead of killing the
module. The boolean cascade in `healing.rs` wraps each attempt in `catch_unwind`, and
a WASM-specific attempt limit (`MAX_WASM_CASCADE_ATTEMPTS`) prevents stack exhaustion.

**Without nightly + -Zbuild-std**, `panic="abort"` is forced and `catch_unwind` is a
no-op — panics emit WASM `unreachable` traps that kill the module. The worker has
auto-restart logic as a safety net, but the primary defense is `catch_unwind`.

### DO NOT use `engineReady` as a crash oracle

`getState().engineReady` is set `true` once at engine init and is only reset when the
bridge receives a `needsRestart: true` flag from the worker **and** the restart fails.
After a successful auto-restart, it stays `true` even though all features were lost.
This makes it unreliable for crash detection.

### Use `collectCrashErrors` + `expectNoAnyCrash`

Import from `helpers/state.js`:

```js
import { collectCrashErrors, expectNoAnyCrash } from './helpers/state.js';

test('my test', async ({ waffle }) => {
    const page = waffle.page;
    const crashTracker = collectCrashErrors(page);  // Set up BEFORE operations

    // ... do operations ...

    expectNoAnyCrash(crashTracker);  // Fails on ANY crash (strict)
});
```

`collectCrashErrors` returns a tracker with `all` (every crash) and `unrecovered`
(crashes where restart failed) arrays. Use `expectNoAnyCrash` (strict, zero crashes)
for new tests. `expectNoCrash` (tolerant of recovered crashes) exists for legacy tests.

### Worker crash recovery flow (safety net)

If `catch_unwind` ever fails (e.g., stack overflow beyond the 4MB limit):

1. Boolean operation panics → WASM `unreachable` trap
2. Worker catches `WebAssembly.RuntimeError` in `processMessage()`
3. Worker fetches fresh JS module via blob URL (bypasses `import()` cache)
4. Worker calls `default(wasmBinaryUrl)` to instantiate new WASM module
5. Worker calls `init()` to create fresh engine state
6. Response includes `needsRestart: false` (recovery succeeded) or `true` (failed)

## Measured Timings (2026-02-21)

Baseline timing data from `profile-rust.sh` run:

| Target | Time | Notes |
|--------|------|-------|
| waffle-types | <1s | Pure types |
| sketch-solver | <1s | Constraint math |
| feature-engine | 1s | MockKernel-based |
| file-format | 6s | Serialization |
| modeling-ops | 22s | MockKernel ops |
| wasm-bridge | <1s | Solver always included |
| kernel-fork (full) | 407s | Dominated by truck booleans |
| test-harness (all) | 1817s | See binary breakdown below |

### test-harness binary breakdown

| Binary | Time | Tier |
|--------|------|------|
| scenarios_mock | <1s | Fast |
| workflow_tests | <1s | Fast |
| oracle_tests | <1s | Fast |
| report_tests | <1s | Fast |
| scenarios_advanced | <1s | Fast |
| stl_tests | <1s | Fast |
| extrude_on_extrude | 1s | Full |
| geomref_truck | 1s | Full |
| auto_union_detection | 2s | Full |
| boolean_workflows | 26s | Full |
| scenarios_truck | 29s | Full |
| size_probe | 33s | Full |
| boolean_failures | 158s | Full |
| **extrude_chains** | **1659s** | Full (96% of harness time) |
