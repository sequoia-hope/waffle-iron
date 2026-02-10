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

| File | Tests | Status |
|------|-------|--------|
| helpers (unit) | 5 | ✅ |
| oracle_tests.rs | 15 | ✅ |
| workflow_tests.rs | 10 | ✅ |
| report_tests.rs | 8 | ✅ |
| scenarios_mock.rs | 15 | ✅ |
| scenarios_truck.rs | 4+3i | ✅ |
| stl_tests.rs | 6 | ✅ |
| **Total** | **63+3i** | ✅ |
