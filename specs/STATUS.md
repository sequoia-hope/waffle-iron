# Specs Status Dashboard

Cross-reference of all spec files against the codebase. Updated 2026-03-02.

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
| 3 | `autosolver.md` | WIBRE Boolean Robustness Engine | Design | — | System spec, no implementation yet |
| 4 | `boolean_determinism.md` | Boolean Determinism | Implemented | `vendor/truck/.../polyline_construction/`, `.../face_boundary_graph.rs`, `truck-base/src/id.rs` | |
| 5 | `boolean_difference_operation.md` | Boolean Difference Operation | Draft | — | Burndown B2 |
| 6 | `boolean_error_types.md` | BooleanError + Result Propagation | Draft | `kernel-fork/src/types.rs`, `kernel-fork/src/truck_kernel.rs` | Burndown A1, partial |
| 7 | `boolean_shell_closure.md` | Boolean Shell Closure | Implemented | `vendor/truck/.../`, `test-harness/tests/boolean_workflows.rs` | Sprint 27 |
| 8 | `boolean_tolerance_layering.md` | BooleanTolerance — Per-Stage Tolerances | Implemented | `vendor/truck/.../integrate/mod.rs` | Updated 2026-03-02 (was stale) |
| 9 | `boolean-workflows.md` | Boolean Workflow Specification | Design | `test-harness/tests/boolean_workflows.rs` | End-to-end test spec |
| 10 | `deterministic_cascade.md` | Deterministic Perturbation Cascade | Complete | `vendor/truck/.../transversal/` | Sprint 37 |
| 11 | `extrude_alignment_bugfix.md` | Extrude Alignment Bug Fix | Implemented | `feature-engine/src/rebuild.rs`, `app/.../sketchCoords.js` | Coordinate system fix |
| 12 | `k8_loops_store_aabb.md` | K8: Degenerate Wire Filter + AABB Culling | Complete | `vendor/truck/.../loops_store/mod.rs` | Sprint 36 |
| 13 | `lazy_exact_escalation.md` | Lazy Exact Escalation for Predicates | Implementing | `vendor/truck/.../integrate/mod.rs`, `.../winding.rs`, `.../robust_classify.rs` | Phase C |
| 14 | `mesh_analytical_fallback.md` | Mesh-to-Analytical IC Fallback | Implemented | `vendor/truck/.../intersection_curve/mod.rs`, `.../analytical.rs` | Phase G2 bug fix |
| 15 | `multi_cut_regression.md` | Multi-Cut Disappearing Body Regression | Implementing | `test-harness/tests/boolean_workflows.rs`, `test-harness/corpus/` | Tests written, fix pending |
| 16 | `orient3d_sos.md` | orient3d SoS Tiebreak | Complete | `vendor/truck/.../robust_classify.rs` | Sprint 37 |
| 17 | `pave_block_corner_touch.md` | Pave Block Corner Touch Detection | Draft | `vendor/truck/.../pave_block.rs`, `.../loops_store/mod.rs` | |
| 18 | `phase_e_cascade_deprecation.md` | Phase E: Cascade Instrumentation | Draft | `vendor/truck/.../diagnostics.rs`, `kernel-fork/src/healing.rs` | FIP approved |
| 19 | `robust_predicates_integration.md` | Robust Geometric Predicates | Implemented | `vendor/truck/.../robust_classify.rs`, `.../coplanar.rs`, `.../bvh.rs` | R1-R4 complete, Burndown A3 |
| 20 | `SHAPEOPS-BOOLEAN-SPEC.md` | Production-Robust B-Rep Boolean Solver | Design | `vendor/truck/truck-shapeops/` | Master architecture spec |
| 21 | `snap_indicator_reactivity_bugfix.md` | Snap Indicator Reactivity Bug Fix | Implemented | `app/.../SketchRenderer.svelte`, `app/.../snap.js`, `app/.../tools.js` | Multiple GUI tests |
| 22 | `topology_first_assembly.md` | Topology-First Shell Assembly | Implemented | `vendor/truck/.../integrate/mod.rs`, `.../radial_assembly.rs` | Sprint 48 |
| 23 | `wasm_panic_recovery.md` | WASM Panic Recovery | Implementing | `wasm-bridge/src/wasm_api.rs`, `kernel-fork/src/truck_kernel.rs` | FIP Section 3 |

## Summary

| Status | Count |
|--------|-------|
| Implemented | 9 |
| Complete | 3 |
| Implementing | 3 |
| Draft | 4 |
| Design | 4 |
| **Total** | **23** |

## Flags

- `boolean_tolerance_layering.md` — Updated 2026-03-02 to match `BooleanTolerance` implementation (was stale).
- `autosolver.md` — Needs branch table per Engineering Constitution P2.
- `boolean_difference_operation.md`, `boolean_error_types.md` — Drafts with no implementation. Consider whether still relevant or should be archived.
- `multi_cut_regression.md` — Regression tests exist but root cause fix is pending.
