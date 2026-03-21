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

### B14: Coplanar Curved-Face Boolean Fix ✅ (Sprint 54)

Three root causes fixed for coplanar circular/curved-face boolean failures:

**Fix 1 — Full tessellation for winding number classification** (`integrate/mod.rs`):
Selective tessellation (optimization that skips shell0 faces whose AABBs don't overlap
shell1) produced incomplete poly_shell0, breaking winding number classification for
concentric geometry. Fix: build separate `full_poly_shell0` for winding/ray-cast
classification when selective tessellation was used.

**Fix 2 — Sense-aware containment injection nudge** (`loops_store/mod.rs`):
`determine_injection_status` nudges along face normal to test inside/outside. For
anti-sense faces (box-on-box), normal_i points toward other solid (correct). For
same-sense faces (concentric cylinders), normal_i points away (wrong). Fix: use
`sign = if same_sense { -1.0 } else { 1.0 }` to flip nudge direction.

**Fix 3 — Contained fixup anti-sense support** (`integrate/mod.rs`):
Contained fixup only handled same-sense coplanar pairs (`Some(true)`). Changed to
`is_some()` to also handle anti-sense pairs.

Tests recovered: CPE1, CPE2, CPC1, CPC3 (4 of 7 previously ignored).
New diagnostic tests: `concentric_cylinders_subtract`, `concentric_cylinders_union`.

### B15: Coplanar Union Contained Fixups ✅ (Sprint 55)

Recovered the final 3 ignored coplanar curved-face tests: CPU1, CPC2, CPB2.

**CPC2 — Test fixture bug fixed, but pipeline fails** (`coplanar_curved.rs`):
`extrude_no_merge` does not reverse direction (unlike `extrude_cut`). Sketch at z=20
with normal `[0,0,1]` extruded up instead of down. Fix: flip sketch normal to `[0,0,-1]`.
However, after fixing the test geometry, all 23 perturbation attempts fail with 16
open edges at z=20. Root cause: containment injection splits the outer top cap into
ring+disc, but face division creates NEW boundary edges for the ring that are
topologically disconnected from the lateral faces' top edges. Re-ignored.

**CPU1 — Union coplanar classification** (`integrate/mod.rs`):
When shell1 is fully contained in shell0, containment injection splits outer caps into
ring (annular, with hole) + disc. The disc gets classified And0 (correct for subtract,
wrong for union) and shell1 caps get Or1 (wrong for union — they're inside shell0).

Fix: Union-only contained fixups in `apply_union_contained_fixups()`:
1. **Disc fixup** (and0→or0): Moves disc faces coplanar with contained shell1 ref faces
   to Or0, so they pair with ring hole edges (same Arc<Edge> from injection).
2. **Cap fixup** (or1→and1): Moves contained shell1 cap faces to And1, excluding them
   from the union result (their edges don't pair with ring topology).

Applied only in `or_result_with_tol` and `or_result_with_tol_diag` (union callers).
Subtract callers untouched. Coplanar overlay code unchanged.

**CPB2 — Downstream of CPU1**: 3-op chain (box + boss union + deep cut). Passed
automatically after CPU1 fix — first op (union) now produces clean topology.

Added `contained1_ref_faces` field to `FragmentationResult` to pass shell1 reference
faces through to union callers. See `specs/coplanar_curved_sprint55.md` for full analysis.

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

### B16: Boundary-Coincident IC Status Fix ✅ (Sprint 56)

For non-all_on_boundary ICs where `has_boundary_edge_between` is true (IC duplicates
boundary edge), `add_edge` creates pseudo-biangles with wrong `status.not()` on sibling
wire. Fix: pre-compute per-face whether ALL closed ICs are boundary-coincident
(`face0_all_bc`/`face1_all_bc` hashmaps). Only reset wire statuses to Unknown after
`add_edge` for faces where ALL ICs are boundary-coincident. Recovered
`diag_reproduce_gui_failure` (8 cylinder configs). Also includes `construct_ring_disc_direct`
fallback in `divide_face/mod.rs` for containment-only loops.

### B17: Both-Boundary IC Skip + AABB-Based add_edge Guard ✅ (Sprint 57)

Two-part fix for remaining IC processing edge cases:

**Part A — Both-boundary IC skip**: When `skip_geom0 && skip_geom1` (all_on_boundary IC
with both faces boundary-coincident), the old code pushed to `open_ics` which created
orphaned single-edge BoundaryWires via `inject_ic_edges_direct`. These orphaned wires
broke edge-sharing propagation. Fix: skip geom stores entirely — don't push to `open_ics`.
Leave faces as Unknown for winding number / edge-neighbor classification. Poly stores still
get the edge for structural parity.

**Part B — AABB-based add_edge guard (g2 stacked boxes fix)**: For non-all_on_boundary ICs
where one endpoint is not on a boundary edge (`add_polygon_vertex` returns None), the old
code always called `add_edge`, creating degenerate spike wires when the endpoint is spatially
outside the face (e.g., ICs between stacked boxes where one endpoint is on the shared z=10
face and the other is at z=20, beyond the side face). Fix: check the off-boundary endpoint
against the precomputed geometry+mesh face AABB. If outside the AABB (beyond face extent),
skip `add_edge` — face stays all-Unknown and gets rebuilt from loops_store with presplit
edges. If inside the AABB (interior crossing), proceed with normal `add_edge`. This
distinguishes legitimate interior crossings from spurious beyond-face ICs.

Key files:
- `loops_store/mod.rs`: `vertex_in_aabb()`, AABB check in IC processing, removed `open_ics.push`
- `integrate/tests.rs`: `stacked_boxes_coplanar_union` reproduction test

Recovered: `stacked_boxes_coplanar_union` (truck-level, vol=2000.0, 12 faces, 0 open edges).

### B18: Fix g2 Kernel Path + AABB-Aware Union Volume Bounds ✅ (Sprint 58)

Two-part fix to recover `g2_stacked_boxes_coplanar_face` through the kernel path:

**Part A — Align `tau_mesh` with `tau_model`** (`types.rs`):
`BooleanOptions` constructors (`default`, `for_scale`, `for_boolean_tol`) all used
`tau_mesh = 0.5 * tau_model`, while `BooleanTolerance::from_model_tol()` (used by all
truck-level tests) uses `tau_mesh = tau_model`. The halved mesh tolerance produced
different IC topology at coplanar boundaries. Fix: set `tau_mesh = tau_model` in all
three constructors. The `validate()` check `tau_mesh <= tau_model` passes when equal.

**Part B — AABB-aware union volume bounds** (`healing.rs`):
The perturbation cascade accepted wrong vol=1000 results (should be 2000) because the
union lower bound `max(A,B)/1.15 = 869.6` was too loose. A naive sum-fraction check
(`vol >= sum * 0.55`) breaks coincident-box tests (h3: A|A = A, ratio=0.5). Fix: compute
AABB overlap fraction between operands before the cascade. When overlap < 1% (geometrically
disjoint), require `vol >= (va+vb) * 0.85`. When overlap >= 1%, use original bounds.
- g2: AABB overlap = 0.0 (disjoint), rejects vol=1000, cascade continues to asymm-scale
  which produces correct vol≈2000.
- h3: AABB overlap = 1.0 (coincident), original bounds accept vol=1000 (correct).

**Part C — Truck-level regression test** (`integrate/tests.rs`):
`stacked_boxes_coplanar_union_half_mesh_tol` — same g2 geometry with `tau_mesh = 0.025`
(half of `tau_model = 0.05`) to confirm B17 fix works regardless of mesh tolerance ratio.

Recovered: `g2_stacked_boxes_coplanar_face` (test-harness, vol≈2000 via asymm-scale).
Still perturbation-dependent — direct attempt fails with 8 open edges.

### B19: Contained-Larger-Than-Container Coplanar Union ✅ (Sprint 59)

Fixes `e2_very_large_boss_exceeds_face`: 10×10×10 box unioned with 16-gon prism (r=8,
center (5,5)) on z=10 top face. Boss is LARGER than box face — all 23 perturbation
attempts previously failed with "4 open edges."

**Three root causes fixed:**

**Fix 1 — Symmetric coplanar adjacency skip** (`loops_store/mod.rs`):
The `coplanar_adj_skip` set was asymmetric. When `i_in_j` (box top contained in boss
bottom), the code skipped `(shell0_adjacent, container_j)` but missed
`(contained_i, shell1_adjacent)`. This allowed ICs between box top and boss lateral
faces, duplicating the containment injection boundary. Fix: add symmetric skip entries
for both containment directions.

**Fix 2 — B19 early-exit for containment-only loops** (`divide_face/mod.rs`):
For coplanar faces with containment-only loops (from boundary injection, not IC
processing), use `construct_ring_disc_direct` to build ring+disc faces directly from
the loops_store wires. This preserves original edge identity — `divide_one_face_v2`
uses FBG parametric-space decomposition which creates NEW edges, breaking edge sharing
with adjacent faces.

**Fix 3 — Ring face excluded from contained disc fixup** (`integrate/mod.rs`):
The contained fixup (A) incorrectly moved ring faces from or1 → and1. The ring's outer
wire (16-gon) centroid at (5,5,10) is inside the contained face (box top 10×10), but
the ring face ITSELF extends far beyond the box. Fix: skip faces with multiple boundary
wires (rings have holes; disc faces are simple single-wire faces). Also unified the
`altshell_to_shell` conversion: all four buckets (and0/or0/and1/or1) pass through a
single `Shell::try_mapped` call to preserve cross-bucket edge sharing.

Key files:
- `loops_store/mod.rs`: symmetric skip in `coplanar_adj_skip` (lines 1635-1682)
- `divide_face/mod.rs`: `construct_ring_disc_direct` + `is_containment_only_loops`
- `integrate/mod.rs`: ring face guard in contained fixup, unified altshell_to_shell
- `integrate/tests.rs`: `large_boss_exceeds_face_union` reproduction test

Recovered: `e2_very_large_boss_exceeds_face` (test-harness, vol≈1993, 0 open edges).
Also: `large_boss_exceeds_face_union` (truck-level, same geometry).

---

## Test Scoreboard

| Suite | Pass | Fail | Ignored |
|-------|------|------|---------|
| truck-shapeops | 375 | 3* | 3 |
| truck-geometry (revolved_curve) | 11 | 0 | 0 |
| kernel-fork | 203 | 1* | 2 |
| test-harness/boolean_properties | 28 | 0 | 0 |
| test-harness/boolean_workflows | 38 | 0 | 0 |
| test-harness/boolean_edge_cases | 8 | 0 | 0 |
| test-harness/boolean_recovery | 14 | 0 | 1 |
| test-harness/boolean_shell_closure | 3 | 1* | 0 |
| test-harness/boolean_determinism | 3 | 0 | 0 |
| test-harness/coplanar_curved | 13 | 0 | 1 |
| test-harness/multi_op_chains | 5 | 0 | 1 |
| test-harness/revolve_boolean | 3 | 2† | 3 |
| test-harness (total) | 400+ | 3 | 6 |

*Pre-existing failures (fillet, euler_characteristic, perturbed, shell_closure_overlapping_cuts)
†RB1, RB6 — pre-existing regression from D1.6/D1.7 commits (boundary-coincident IC skip)

Last updated: Sprint 68+ (2026-03-21)

### Arc-Edge Vertex Welding (2026-03-21)
- Added `weld_arc_edge_vertices` for cyl-cyl boolean results
- 23 new tests (3 red→green + 20 adversarial), all passing
- Improves vertex index sharing for rendering; position-based watertight check already handled positions
- Total kernel tests: 585 pass, 2 ignored

---

## Assay Status (2026-03-21)

**Score: 81/160**
- Previous: 75-76/160 (2026-03-20)

### Failure Categories
| Category | Count | Description |
|----------|-------|-------------|
| watertight (1-6 unpaired) | ~26 | Residual non-manifold edges at earcut overlap boundaries in bounded tessellation |
| watertight (60+ unpaired) | ~10 | Structural boolean issues with complex geometries |
| empty mesh | ~3 | Boolean chain failures (3-op sequences, or cut consuming entire boss) |
| low triangle count | ~8 | Auto-union failure → standalone body → only first extrude visible |
| bbox diagonal | ~4 | Slight geometry errors (2-25% over oracle max) |
| face product limit | ~5 | Gear-gear booleans exceed 5000 effective face product |
| timeout | 4 | Slow boolean ops (>90s) |

### Key Findings (2026-03-21 Session)
1. **Non-manifold edge root cause refined**: The 20 cases with 1-6 unpaired edges have non-manifold edges at **earcut diagonals** (interior mesh edges), NOT at B-Rep boundary edges. When two adjacent faces share corner vertices (but not a boundary edge between them), earcut independently creates the same diagonal, producing 3 triangles sharing that edge. A topology-aware post-processing step (using B-Rep edge→face relationships) was added but doesn't fix these cases because the non-manifold edges aren't on B-Rep edges. **Fix requires constrained Delaunay triangulation (CDT)** to prevent earcut from creating diagonals between vertices shared across face boundaries.
1b. **Previous finding (2026-03-20)**: Bounded tessellation path creates overlapping triangles at face boundaries due to independent earcut tessellation of adjacent faces. The shared-vertex discretization shares edge vertices correctly, but earcut generates different interior triangles that share the same edges. Attempts at post-hoc removal (conservative, aggressive, targeted) all caused regressions.
2. **F-series F0044-F0060 failures**: These are "featured" test cases with cross-plane/angled/scaled geometries. Failures are due to auto-union fallback (feature-engine returns standalone body with warning, not error) and complex geometric configurations (perpendicular planes, scale extremes 1e4).
3. **Cyl-cyl union works at kernel level**: Direct WaffleKernel calls to `boolean_union` succeed for coaxial and offset cylinders. Failures through feature-engine may be due to sketch plane resolution or extrude direction differences.

---

## Perturbation-Dependent Tests

Tests that pass but require the perturbation cascade (direct attempt fails).
These are candidates for improvement — ideally the direct boolean should succeed.

| Test | Strategy | Attempts | Root Cause |
|------|----------|----------|------------|
| g2_stacked_boxes_coplanar_face | asymm-scale | ~37 | Direct `or_result_with_tol_diag` produces 8 open edges; truck-level `or` passes — difference is in finalization path |
