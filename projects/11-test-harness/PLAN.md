# Project 11 — Test Harness

## Overview

`crates/test-harness/` — Rust integration/regression testing crate for the Waffle Iron CAD engine. Provides `ModelBuilder`, a fluent API for scripted CAD workflows (sketch → extrude → boolean → verify), plus verification oracles (topology, mesh quality, provenance) and a report module.

> **Note**: The original plan described a 3-layer Node.js architecture (harness / model-ops / assertions). This was superseded by the current Rust-native `ModelBuilder` design.

## Milestones

### M1: Core API ✅

- `ModelBuilder::mock()` / `ModelBuilder::truck()` — kernel-backed builders
- Sketch shortcuts: `add_sketch_on(plane)`, `add_rectangle()`, `add_circle()`
- Feature ops: `add_extrude(depth)`, `add_extrude_cut(depth)`, `add_revolve()`, `add_boolean_union/subtract/intersect()`
- History: `undo()`, `redo()`, `feature_count()`
- File I/O: `save()`, `load(path)`
- Assertions: `assert_feature_count()`, `assert_solid_count()`
- Oracle runners: `run_topology_oracle()`, `run_mesh_oracle()`, `run_all_oracles()`

### M2: Verification Oracles ✅

- **TopologyOracle** — Euler check (V-E+F=2), manifold edges, consistent normals, genus
- **MeshOracle** — degenerate triangles, normal consistency, watertightness
- **ProvenanceOracle** — feature-to-face mapping integrity
- **Composite runners** — `run_all_oracles()` with configurable strictness

### M3: Report Module ✅

- `ModelReport` — structured test results
- Oracle result aggregation
- `to_text()` output for test diagnostics

### M4: Test Scenarios — MockKernel ✅

- `scenarios_mock.rs` — basic MockKernel workflow tests
- `scenarios_advanced.rs` — advanced multi-op MockKernel tests
- `workflow_tests.rs` — end-to-end workflow tests

### M5: Test Scenarios — TruckKernel ✅

TruckKernel integration tests covering extrude chains, boolean workflows, regressions, and saved test cases.

### M6: Utility Tests ✅

- `oracle_tests.rs` — oracle unit tests
- `report_tests.rs` — report formatting tests
- `stl_tests.rs` — STL export tests

## Test Summary

| File | Tests | Kernel |
|------|-------|--------|
| auto_union_detection.rs | 7 | Truck |
| boolean_determinism.rs | 3 | Truck |
| boolean_failures.rs | 19 (1 ignored) | Truck |
| boolean_shell_closure.rs | 4 | Truck |
| boolean_workflows.rs | 38 (1 ignored) | Truck |
| extrude_chains.rs | 46 | Truck |
| extrude_on_extrude.rs | 7 | Truck |
| geomref_truck.rs | 17 | Truck |
| multi_body_workflows.rs | 6 | Both |
| oracle_tests.rs | 17 | Mock |
| report_tests.rs | 8 | Mock |
| revolve_cylinder_truck.rs | 8 (2 ignored) | Truck |
| saved_test_cases.rs | 12 | Truck |
| scenarios_advanced.rs | 38 | Mock |
| scenarios_mock.rs | 15 | Mock |
| scenarios_truck.rs | 38 (2 ignored) | Truck |
| size_probe.rs | 4 | Truck |
| stl_tests.rs | 6 | None (utility) |
| suppress_undo_interactions.rs | 5 | Mock |
| workflow_tests.rs | 10 | Mock |
| helpers.rs (src) | 5 | None (unit) |
| **Total** | **313 (6 ignored)** | |

## Blockers

None — all milestones complete.
