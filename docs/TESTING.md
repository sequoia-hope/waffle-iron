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

## Measured Timings (2026-02-21)

Baseline timing data from `profile-rust.sh` run:

| Target | Time | Notes |
|--------|------|-------|
| waffle-types | <1s | Pure types |
| sketch-solver | <1s | Constraint math |
| feature-engine | 1s | MockKernel-based |
| file-format | 6s | Serialization |
| modeling-ops | 22s | MockKernel ops |
| wasm-bridge | <1s | Needs `--no-default-features` |
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
