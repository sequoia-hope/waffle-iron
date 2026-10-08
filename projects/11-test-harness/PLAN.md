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

### M7: S4 — independent sketch rank oracle ✅ (2026-10-08)

`specs/agent_mechanical_design.md` §10.4 asks for "a second rank computation
from the published residual definitions … the same algebra at a different
implementation, in the reference-parity posture the Yang work uses".

- `src/sketch_rank.rs` — a second computation of a sketch's structural state:
  the constraint residuals written from their published equations
  (`specs/sketch_solver_rewrite.md` §"Constraint Types"), differentiated by
  **central finite differences**, ranked by **SVD** of the row-normalized
  Jacobian, with the null space read off the right singular vectors of a
  zero-padded matrix. Reports `params`, `rows`, `rank`, `dof`, `null_dim`,
  the dependent (rank-deficient) rows, the unsatisfiable rows and the
  null-space directions by parameter slot. Every threshold is a named
  constant with its justification, and a singular value within a decade of
  the rank threshold returns `RankVerdict::Indeterminate` rather than a
  confident disagreement.
- `tests/sketch_rank_oracle.rs` — the differential test: all 27
  `SketchConstraint` variants (enforced by `every_constraint_variant_is_exercised`
  against an explicit `ALL_VARIANTS` list), plus the under-constrained,
  fully-constrained, redundant-but-satisfied, contradictory, reference-dimension
  and three-scale shapes S4 names. 16 tests.
- Result: **the oracle and `SketchSolveReport` agree on `params`, `rows`,
  `rank`, `dof` and the dependent-row list in every case**, including the
  declaration-order redundancy walk and the caller's full index space across a
  leading reference dimension.
- Independence is partial in five places, each marked `CONSULTED:` at its
  oracle arm: `SymmetricH`/`SymmetricV`'s axis, `Tangent`'s squared form,
  `Diameter`'s factor of two, `SameOrientation`'s zero rows, and the
  point-line sign convention. Those comments are the inventory.

## Findings (sketch-solver's to fix; pinned here, NOT fixed)

| finding | pin |
|---|---|
| `PointLineDistance` / `Distance(point, line)` subtract an UNSIGNED stored value (both app emitters are `Math.abs`) from a SIGNED residual, so a dimension reporting the distance the geometry already has MIRRORS the point across the line. Measured: both endpoints pinned, point authored at (5, 7), `value: 7.0` → the solve returns (5, −6.999999998) and calls it satisfied; `value: −7.0` leaves it untouched with residual 0. | `a_point_line_distance_dimension_mirrors_the_point_across_the_line` |
| `SOLVE_TOL` is absolute (1e-6). The same 4:3 rectangle authored 3 % off its solution converges to the same RELATIVE accuracy at every scale (`residual_inf` 1.2000e-11 at 1 m → 1.2000e-6 at 1e5), and the verdict flips to `SolveFailed` exactly where the absolute threshold is crossed, with `rank` 8 and `dof` 0 unchanged. (Known open defect, §"S2 — solver state".) | `an_absolute_solve_tol_refuses_the_same_rectangle_once_it_is_authored_large` |
| `MOVED_EPS` is absolute (1e-9). Same rectangle, origin authored exactly on its `Pinned` target: `moved` = {2, 3, 4} at metre scale but {1, 2, 3, 4} at km scale, listing the pinned origin at 3.3541e-9 m — 8.4e-13 relative, nothing that moved. (Known open defect, same note.) | `an_absolute_moved_eps_lists_a_pinned_point_as_moved_once_the_sketch_is_large` |

Each pin fails loudly if the behaviour changes, with a message saying to delete
the pin and assert the fix.

## Test Summary

| File | Tests | Kernel |
|------|-------|--------|
| auto_union_detection.rs | 7 | Truck |
| boolean_determinism.rs | 3 | Truck |
| boolean_edge_cases.rs | 7 | Truck |
| boolean_failures.rs | 19 (1 ignored) | Truck |
| boolean_properties.rs | 24 (2 ignored) | Truck |
| boolean_recovery.rs | 13 (2 ignored) | Truck |
| boolean_shell_closure.rs | 4 | Truck |
| boolean_workflows.rs | 38 (1 ignored) | Truck |
| extrude_chains.rs | 46 | Truck |
| extrude_on_extrude.rs | 7 | Truck |
| geomref_fallback.rs | 19 | Truck |
| geomref_truck.rs | 3 | Truck |
| multi_body_workflows.rs | 6 | Both |
| multi_op_chains.rs | 5 (1 ignored) | Truck |
| oracle_tests.rs | 17 | Mock |
| rebuild_stability.rs | 6 | Truck |
| report_tests.rs | 8 | Mock |
| revolve_boolean.rs | 0 (8 ignored) | Truck |
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
| **Total** | **~400 (19 ignored)** | |

### Ignored Tests by Category (Sprint 41)

| ID | Test | File | Reason |
|----|------|------|--------|
| MV3 | `mv3_subtract_topology_preservation` | boolean_properties | chi=1, subtract topology |
| EC3 | `ec3_disjoint_union_multi_shell` | boolean_properties | Disjoint multi-shell |
| R3 | `r3_abutting_box_coplanar` | boolean_recovery | Abutting box coplanar |
| S3 | `s3_multi_cylinder_cascade` | boolean_recovery | Multi-cylinder cascade |
| MO4 | `mo4_revolve_then_boolean` | multi_op_chains | Revolve+boolean cascade |
| RB1-8 | `rb1..rb8_revolve_*` | revolve_boolean | Torus-plane IC unsupported |

## Blockers

None for this crate — all milestones complete. The three findings above are
`sketch-solver`'s, pinned at their measured behaviour here so a fix shows up as
a deliberate change rather than a surprise.
