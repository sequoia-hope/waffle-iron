# Specs Status Dashboard

Cross-reference of all spec files against the codebase. Updated 2026-03-04.

## Status Legend

- **Implemented** — Code matches spec, tests pass.
- **Implementing** — Active work, partially complete.
- **Draft** — Spec written, implementation not started.
- **Stale** — Spec diverges from implementation (needs update).
- **Complete** — Implemented and closed (no further work planned).
- **Design** — Architecture/planning document, not a single-feature spec.

## Specs Table

| # | Filename | Title | Status | Implementation File(s) | Notes |
|---|----------|-------|--------|----------------------|-------|
| 1 | `analytical_cylinder_cylinder.md` | Analytical SSI: Cylinder-Cylinder | Implementing | `vendor/truck/.../intersection_curve/analytical.rs` | Sprint 42, P3 |
| 2 | `analytical_ssi_audit.md` | Analytical SSI Audit Report | Design | `vendor/truck/.../intersection_curve/analytical.rs`, `kernel-fork/src/healing.rs` | Sprint 38 audit |
| 3 | `boolean_determinism.md` | Boolean Determinism | Implemented | `vendor/truck/.../polyline_construction/`, `.../face_boundary_graph.rs`, `truck-base/src/id.rs` | |
| 4 | `boolean_difference_operation.md` | Boolean Difference Operation | Implemented | `vendor/truck/.../integrate/mod.rs` (`difference()`, `difference_result()`, `ClassifiedShellBuckets`) | Burndown B2 |
| 5 | `boolean_error_types.md` | BooleanError + Result Propagation | Implemented | `vendor/truck/.../integrate/mod.rs` (`BooleanStageError`, `and_result()`, `or_result()`), `kernel-fork/src/types.rs` | Burndown A1 |
| 6 | `boolean_shell_closure.md` | Boolean Shell Closure | Implemented | `vendor/truck/.../`, `test-harness/tests/boolean_workflows.rs` | Sprint 27 |
| 7 | `boolean_tolerance_layering.md` | BooleanTolerance — Per-Stage Tolerances | Implemented | `vendor/truck/.../integrate/mod.rs` | Updated 2026-03-02 (was stale) |
| 8 | `boolean-workflows.md` | Boolean Workflow Specification | Design | `test-harness/tests/boolean_workflows.rs` | End-to-end test spec |
| 9 | `deterministic_cascade.md` | Deterministic Perturbation Cascade | Complete | `vendor/truck/.../transversal/` | Sprint 37 |
| 10 | `d1_pave_block_integration.md` | D1: Pave Block Integration | Implementing | `vendor/truck/.../pave_block.rs`, `.../interference.rs`, `.../loops_store/mod.rs` | Phases 1-5 done, analytical crossings failed, realignment needed |
| 11 | `d2_shrunk_ranges.md` | D2: Shrunk Ranges | Implementing | `vendor/truck/.../pave_block.rs`, `.../integrate/mod.rs` | D2.1-D2.3 + D2.5a done |
| 12 | `extrude_alignment_bugfix.md` | Extrude Alignment Bug Fix | Implemented | `feature-engine/src/rebuild.rs`, `app/.../sketchCoords.js` | Coordinate system fix |
| 13 | `ic_boundary_interpolation.md` | IC Boundary Interpolation | Implemented | `vendor/truck/.../intersection_curve/mod.rs` | |
| 14 | `ic_loop_restructuring.md` | IC Loop Restructuring | Implemented | `vendor/truck/.../loops_store/mod.rs` | Two-pass architecture |
| 15 | `k8_loops_store_aabb.md` | K8: Degenerate Wire Filter + AABB Culling | Complete | `vendor/truck/.../loops_store/mod.rs` | Sprint 36 |
| 16 | `lazy_exact_escalation.md` | Lazy Exact Escalation for Predicates | Implementing | `vendor/truck/.../integrate/mod.rs`, `.../winding.rs`, `.../robust_classify.rs` | Phase C |
| 17 | `mesh_analytical_fallback.md` | Mesh-to-Analytical IC Fallback | Implemented | `vendor/truck/.../intersection_curve/mod.rs`, `.../analytical.rs` | Phase G2 bug fix |
| 18 | `multi_cut_regression.md` | Multi-Cut Disappearing Body Regression | Implementing | `test-harness/tests/boolean_workflows.rs`, `test-harness/corpus/` | Tests written, fix pending |
| 19 | `orient3d_sos.md` | orient3d SoS Tiebreak | Complete | `vendor/truck/.../robust_classify.rs` | Sprint 37 |
| 20 | `pave_block_corner_touch.md` | Pave Block Corner Touch Detection | Implemented | `vendor/truck/.../interference.rs` (`find_corner_touch_snap`), `.../loops_store/mod.rs` | |
| 21 | `phase_e_cascade_deprecation.md` | Phase E: Cascade Instrumentation | Implemented | `kernel-fork/src/healing.rs` (`CascadeStats`, `cascade_stats()`, `reset_cascade_stats()`) | |
| 22 | `robust_predicates_integration.md` | Robust Geometric Predicates | Implemented | `vendor/truck/.../robust_classify.rs`, `.../coplanar.rs`, `.../bvh.rs` | R1-R4 complete, Burndown A3 |
| 23 | `SHAPEOPS-BOOLEAN-SPEC.md` | Production-Robust B-Rep Boolean Solver | Design | `vendor/truck/truck-shapeops/` | Master architecture spec |
| 24 | `snap_indicator_reactivity_bugfix.md` | Snap Indicator Reactivity Bug Fix | Implemented | `app/.../SketchRenderer.svelte`, `app/.../snap.js`, `app/.../tools.js` | Multiple GUI tests |
| 25 | `topology_first_assembly.md` | Topology-First Shell Assembly | Implemented | `vendor/truck/.../integrate/mod.rs`, `.../radial_assembly.rs` | Sprint 48 |
| 26 | `torus_plane_ssi_fix.md` | Torus-Plane SSI Fix | Implemented | `vendor/truck/.../intersection_curve/analytical.rs` | Phase G |
| 27 | `v2_assembly_tolerance_reduction.md` | V2 Assembly Tolerance Reduction | Implemented | `vendor/truck/.../integrate/mod.rs` | Superseded by D2.5a |
| 28 | `wasm_panic_recovery.md` | WASM Panic Recovery | Implementing | `wasm-bridge/src/wasm_api.rs`, `kernel-fork/src/truck_kernel.rs` | FIP Section 3 |

## Summary

| Status | Count |
|--------|-------|
| Implemented | 17 |
| Complete | 3 |
| Implementing | 5 |
| Draft | 0 |
| Design | 3 |
| **Total** | **28** |

## Flags

- `boolean_tolerance_layering.md` — Updated 2026-03-02 to match `BooleanTolerance` implementation (was stale).
- `boolean_difference_operation.md`, `boolean_error_types.md`, `pave_block_corner_touch.md`, `phase_e_cascade_deprecation.md` — Were marked Draft but all 4 are fully implemented. Updated 2026-03-02.
- `multi_cut_regression.md` — Regression tests exist but root cause fix is pending.
