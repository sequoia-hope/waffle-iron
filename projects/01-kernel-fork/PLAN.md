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

### B7: Coplanar Face Splitting — NOT STARTED (CRITICAL)

Root cause: `create_loops_stores` skips coplanar face pairs because
`IntersectionCurve` construction fails. Faces remain undivided. The
existing coplanar classifier handles fully-overlapping faces but cannot
split partially-overlapping ones.

Algorithm:
1. Detect coplanar face pairs in `create_loops_stores` (reuse `check_coplanar`)
2. Project both face boundaries into 2D parameter space of shared plane
3. Compute 2D polygon clipping for intersection contour
4. Inject synthetic intersection edges into LoopsStore
5. `divide_faces` splits along synthetic contours

Injection point: `loops_store/mod.rs` ~line 514

Files:
- `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs`
- `vendor/truck/truck-shapeops/src/transversal/coplanar.rs`

Tests to un-ignore when done:
- `coplanar_partial_overlap_union` (integrate/tests.rs)
- `test_coplanar_partial_overlap_union` (kernel-fork)
- `rect_cut_at_box_boundary` (boolean_failures)
- `circle_cut_crossing_box_edge` (boolean_failures)

### B8: Multiple Coplanar Face Pairs — NOT STARTED
- Depends: B7
- Corner boss shares 2-3 coplanar planes

### B9: Coplanar Subtract — NOT STARTED
- Depends: B7
- Pocket on shared face

### B10: Healed Edge Validation — NOT STARTED (P2)
### B11: Exact Arc Healing — NOT STARTED (P3)
### B12: Eliminate Perturbation — NOT STARTED (P3)

## Test Scoreboard

| Suite | Pass | Fail | Ignored |
|-------|------|------|---------|
| kernel-fork | 138 | 0 | 3 |
| test-harness/boolean_failures | 17 | 0 | 2 |
| test-harness/boolean_workflows | 23 | 0 | 1 |
| test-harness (total) | 178 | 0 | 6 |
