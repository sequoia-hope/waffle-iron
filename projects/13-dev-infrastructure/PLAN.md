# Sub-project 13: Agent-Driven Development Infrastructure

## Milestones

### M1: Crate Skeleton + Helpers + STL Export ✅
- [x] Cargo.toml + workspace member
- [x] lib.rs with module re-exports
- [x] helpers.rs: HarnessError, GeomRef constructors, profile builders, mesh math
- [x] stl.rs: binary + ASCII STL export
- [x] Unit tests for helpers (5 tests)

### M2: Verification Oracles ✅
- [x] oracle.rs: OracleVerdict struct
- [x] Topology oracles: euler_formula, manifold_edges, face_validity, topology_counts
- [x] Mesh oracles: watertight (position-based), consistent_normals, no_degenerate, unit_normals, face_range_coverage, valid_indices, bounding_box
- [x] Provenance oracles: role_exists
- [x] Composite runners: run_all_mesh_checks, run_topology_checks
- [x] oracle_tests.rs (15 tests)

### M3: ModelBuilder Workflow API ✅
- [x] workflow.rs: ModelBuilder wrapping dispatch()
- [x] Named features (string → UUID mapping)
- [x] Sketch shortcuts: rect_sketch, circle_sketch
- [x] Manual sketch: begin_sketch, add_point/line/circle/arc, finish_sketch_manual
- [x] Feature ops: extrude, extrude_cut, extrude_on_face, revolve, fillet, chamfer, shell, boolean_*
- [x] History: undo, redo
- [x] Feature management: suppress, unsuppress, delete, reorder
- [x] Queries: feature_id, feature_count, solid_handle, tessellate, topology_counts, face_signatures, select_face_by_role/normal
- [x] File I/O: save, load, export_stl
- [x] Inline assertions: assert_feature_count, assert_has_solid, assert_no_errors, assert_has_errors
- [x] Oracle integration: check_mesh, check_topology
- [x] workflow_tests.rs (10 tests)

### M4: Report Module + Assertions ✅
- [x] report.rs: ModelReport struct, FeatureEntry, MeshSummary, to_text()
- [x] ModelBuilder::report() generates full model report with oracles
- [x] assertions.rs: assert_topology_eq, assert_bounding_box, assert_role_assigned, assert_tree_structure
- [x] report_tests.rs (8 tests)

### M5: Complex Workflow Regression Tests (MockKernel) ✅
- [x] 15 scenarios including full workflow test
- [x] Box extrude, box with hole, sketch on face, revolve, fillet, chamfer, shell
- [x] Boolean union/subtract, multi-body, undo/redo, save/load, suppress/unsuppress
- [x] STL export validation
- [x] Full end-to-end workflow with report generation

### M6: TruckKernel Scenario Tests ✅
- [x] 4 passing: box extrude, revolve, tessellate+STL, boolean offset
- [x] 3 ignored: coplanar boolean, fillet, chamfer (known limitations)

### M7: Project Documentation ✅
- [x] PLAN.md (this file)
- [x] ARCHITECTURE.md
- [x] CLAUDE.md
- [x] INTERFACES.md

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
| assay_randomized.rs | 2 + 7 batches (ignored) | WaffleKernel |
| **Total** | **313 (6 ignored)** | |

---

## Completed Tasks

### Fix Euler Target Oracle Predictions (2026-03-27)

**Spec**: `/specs/euler_target_oracle_fix.md`

**Problem**: `compute_euler_target()` over-predicted through-holes for multi-plane
and 3-op cases, causing 8 assay cases to fail with correct geometry (chi=2 actual
vs chi=0 expected). The generator's `extrude_rect_aabb()` also used a different
local frame algorithm than the kernel's `tangent_x_from_normal()`.

**Fix**:
- Rewrote `compute_euler_target()` to only predict through-holes for 2-op
  same-plane extrude cases where cut_depth ≥ boss_depth
- Aligned AABB frame algorithm with kernel
- Widened disjointness margin from 1e-9 to 1e-4

**Tests**:
- 5 unit tests for `compute_euler_target` branch coverage
- `batch_euler_target_fix` integration test: 8/8 target cases pass

**Result**: Assay score 44/172 → 55/172 (+11 passes)

**Files changed**:
- `crates/test-harness/src/assay/gen.rs`
- `crates/test-harness/tests/assay_randomized.rs`
- `crates/kernel/src/tessellation/mod.rs` (clippy type alias)
- `specs/euler_target_oracle_fix.md`
