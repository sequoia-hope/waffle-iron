# 01 — Kernel Fork: Plan

## Milestones

### M1: Fork and Build Setup ✓
- [x] Fork truck repository
- [x] Set up as workspace dependency (crates.io path dependency)
- [x] Verify `cargo build` for truck-topology, truck-geometry, truck-modeling, truck-shapeops, truck-meshalgo
- [x] Create kernel-fork crate skeleton with Cargo.toml

### M2: Higher-Level Primitive API ✓
- [x] `make_box(width, height, depth) -> Solid`
- [x] `make_cylinder(radius, height) -> Solid`
- [x] `make_sphere(radius) -> Solid`
- [x] Unit tests for each primitive (vertex/edge/face counts, bounding box)

### M3: Kernel Trait Adapter ✓
- [x] Implement `TruckKernel` struct
- [x] `extrude_face()` via `builder::tsweep`
- [x] `revolve_face()` via `builder::rsweep`

### M4-M11: Previous milestones ✓
- All complete (see git history)

---

## Boolean Robustness Milestones

### B1: Adaptive Tolerance ✅ (42cfbae)
- `compute_adaptive_tol()`: `(extent * 0.005).clamp(0.005, 0.05)`
- All boolean methods use adaptive tol, never exceeds proven 0.05 baseline

### B2: Scale-Aware Perturbation ✅ (42cfbae)
- `try_boolean_with_perturbation` takes `tol` parameter
- Epsilons scale as `[extent*1e-6 .. extent*1e-3]`

### B3: Adaptive Plane Triangulation ✅ (42cfbae)
- `Plane::parameter_division()` uses `sqrt(span/tol)` subdivision
- File: `vendor/truck/truck-geometry/src/specifieds/plane.rs:203`

### B4: Ray-Cast Fallback ✅ (42cfbae)
- Multi-direction centroid fallback when primary ray fails
- File: `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`

### B5: Adaptive Polyline Quantization ✅ (42cfbae)
- Grid spacing = `max(min_seg_len * 0.5, 2*TOLERANCE)`
- File: `vendor/truck/truck-shapeops/src/transversal/polyline_construction/mod.rs`

### B6: Degenerate Polyline Guard ✅ (6fafbec)
- `IntersectionCurve::try_new` returns None for polylines < 2 points
- Fixed 4 crashing boolean_workflow tests

### B7: Coplanar Face Splitting ✅ (D1.6+D1.7, Sprint 52-53)

Solved via a different approach than originally planned. Instead of 2D polygon
clipping + synthetic edge injection, the fix uses the existing IC pipeline with
two targeted improvements:

**D1.6 — Boundary-coincident IC skip** (Sprint 52, 833e3c8):
When an IC edge connects two boundary vertices already joined by an existing
boundary edge AND shares a vertex with another IC on the same face, skip
`add_edge` (which would create a biangle→figure-8) and instead leave
classification to winding number / edge-neighbor propagation.

**D1.7 — all_on_boundary three-way logic** (Sprint 53):
For ICs where all 4 endpoints lie on boundary edges:
1. Both faces boundary-coincident → legacy `inject_ic_edges_direct`
2. At least one face has a cross-chord → `add_edge` on the cross-chord face,
   leave boundary-coincident face as Unknown for classification
3. Per-face vertex maps now include ALL ICs (removed `!all_on_boundary` filter)

Tests recovered:
- `coplanar_partial_overlap_union` (integrate/tests.rs) — ✅ un-ignored
- `test_coplanar_partial_overlap_union` (kernel-fork) — ✅ un-ignored
- CM1, T1, T3, MV1 boolean_properties tests — ✅ all pass
- `rect_cut_at_box_boundary` — still ignored (requires more work)
- `circle_cut_crossing_box_edge` — still ignored (requires more work)

### B8: Multiple Coplanar Face Pairs ✅ (via D1.6+D1.7)
- T1 geometry (4 coplanar face pairs) passes correctly
- MV1 geometry (2 coplanar face pairs + symmetric overlap) passes correctly
- Corner boss (2-3 coplanar planes) not directly tested but infrastructure handles it

### B9: Coplanar Subtract ✅ (via D1.6+D1.7)
- MV1 intersection (And operation) produces exact volume (250.0)
- boolean_properties inclusion-exclusion identity holds exactly

### B10: Healed Edge Validation — NOT STARTED (P2)
### B11: Exact Arc Healing — NOT STARTED (P3)
### B12: Eliminate Perturbation — NOT STARTED (P3)

### B13: Revolve v-Seam Parametric Continuity ✅ (Sprint 53)

Fix parametric discontinuities across the v=0/2π seam of full 360° RevolutedCurve
surfaces. Two-layer approach:

**Layer 1 — `search_parameter` branch selection** (`revolved_curve.rs`):
When a v-hint is provided and differs from the computed angle by >π, shift by ±2π
to the branch closest to the hint. Guard: only activates when |Δ| > π, preventing
false shifts for legitimate revolution arcs. Safety: existing `subs(t, ang).near(&point)`
check validates correctness.

**Layer 2 — v-seam unwrapping in callers**:
- `create_parameter_boundary` (`divide_face/mod.rs`): Post-hoc `unwrap_v_seam_periodic`
  scans adjacent vertices for |Δv| > 2π−0.8 (≈5.48), shifts by ±2π.
- `FaceBoundaryGraph::from_loops` (`face_boundary_graph.rs`): Same unwrapping pass
  applied per-wire after vertex mapping.

**Threshold rationale**: Uses 2π−0.8 ≈ 5.48 instead of π to avoid false positives on
non-periodic surfaces (planes, general NURBS) where large |Δv| is legitimate.

**Result**: Fixes parametric continuity across seam. Does NOT resolve RB2/RB8/MO4 —
remaining failures are in shell assembly (IC edges have refs=1 due to non-shared
vertices between torus and box face fragments). See `specs/revolve_face_division_fix.md`.

**Known regressions discovered**: RB1 and RB6 (previously tracked as passing) now fail.
These are pre-existing failures from D1.6/D1.7 commits (1138e87, 833e3c8), NOT caused
by this work. Confirmed by testing on clean code with all changes reverted.

New tests: 3 revolved_curve (branch selection), 3 truck-shapeops (2 ignored torus, 1 partial revolve).

---

## Test Scoreboard

| Suite | Pass | Fail | Ignored |
|-------|------|------|---------|
| truck-shapeops | 370 | 3* | 2 |
| truck-geometry (revolved_curve) | 11 | 0 | 0 |
| kernel-fork | 203 | 1* | 2 |
| test-harness/boolean_properties | 28 | 0 | 0 |
| test-harness/boolean_workflows | 38 | 0 | 0 |
| test-harness/boolean_edge_cases | 8 | 0 | 0 |
| test-harness/boolean_recovery | 14 | 0 | 1 |
| test-harness/boolean_shell_closure | 3 | 1* | 0 |
| test-harness/boolean_determinism | 3 | 0 | 0 |
| test-harness/coplanar_curved | 3 | 0 | 7 |
| test-harness/tolerance_sensitivity | 6 | 0 | 0 |
| test-harness/revolve_boolean | 3 | 2† | 3 |
| test-harness (total) | 400+ | 3 | 13 |

*Pre-existing failures (fillet, euler_characteristic, shell_closure_overlapping_cuts)
†RB1, RB6 — pre-existing regression from D1.6/D1.7 commits (boundary-coincident IC skip)

Last updated: Sprint 53 (2026-03-05)
