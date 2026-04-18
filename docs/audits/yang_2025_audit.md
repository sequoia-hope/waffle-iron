# Yang 2025 Implementation Audit — 2026-04-18

Deep research audit of the Waffle Iron Yang 2025 hybrid boolean pipeline.
5 parallel audit agents each examined 4 contiguous pipeline steps, reading
both the paper text and every line of implementation code.

## Summary

| # | Yang Step | Section | Verdict | Key Issue |
|---|-----------|---------|---------|-----------|
| 1 | Error-bounded discretization | 4.1.1 | **CORRECT** | d_epsilon formula correct, adaptive segments work |
| 2 | Bijective parametric mapping | 4.1.1 | **INCOMPLETE** | vertex_params computed but NEVER READ — dead code |
| 3 | CDT boundary re-triangulation | 4.1.2 | **STUB** | NOT implemented at tessellation time; each patch independent |
| 4 | Mesh intersection (Cherchi) | 4.2 | **CORRECT** | Full pipeline, correct labels, correct output split |
| 5 | Conservative 2*d_epsilon detection | 4.2.1 | **STUB** | No octree, no d_epsilon AABB offset, O(n^2) brute force |
| 6 | Gauss map normal cone filtering | 4.2.2 | **WRONG** | NormalCone struct exists but is NEVER CALLED; current filter only checks same-mesh co-orientation, not cross-mesh per Yang |
| 7 | Newton optimization | 4.3.1 | **CORRECT** | Matches Appendix C exactly — residual sign, Jacobian, update |
| 8a | Geometric optimization | 4.3.2 | **INCOMPLETE** | Does NOT compute tangent plane intersection line L; just projects midpoint to each surface independently |
| 8b | Method selection | 4.3.3 | **WRONG** | Reversed order (geometric first, Newton fallback); paper says Newton for tangent/boundary, geometric for loops |
| 9 | Curvature-based refinement | 4.3.4 | **STUB** | Three termination conditions (arc_height, chord, angle) NOT implemented; uses fixed 10-degree sampling |
| 10 | Inside/outside classification | 4.4.2 | **CORRECT** | Ray-cast + GWN fallback, co-surface via normal offset |
| 11 | Cell selection per boolean op | 4.4.2 | **CORRECT** | Selection table matches paper, consistent between two functions |
| 12 | Flood-fill patch segmentation | 4.4.2 | **CORRECT** | BFS correct, edge classification sound, T-junction splitting works |
| 13 | B-Rep construction from patches | 4.4.2 | **CORRECT** | Twin pairing robust, unpaired HE repair functional |
| 14 | CDT mesh updating | 4.4.1 | **CORRECT** | Calls triangulate_single_triangle with constraints, properly integrated |
| 15 | Topology validation | 4.4.3 | **CORRECT** | Comprehensive checks, P9 enforced (rejects partial topology) |
| 16 | Optimize across boundaries | 4.5.1 | **INCOMPLETE** | Midpoint + re-optimize works, but NO step truncation to boundary curve, NO surface switching |
| 17 | Local mesh refinement | 4.5.2 | **CORRECT** | Real re-tessellation with halved d_epsilon, full pipeline re-run |
| 18 | Reversed curve correction | 4.5.3 | **INCOMPLETE** | Detection logic correct, but function takes `&SubdividedMesh` (immutable) — CANNOT remove points. Detection only, no modification. |
| 19 | Self-intersection removal | 4.5.4 | **STUB** | Generic topology validation only; NO self-intersection detection per 4.5.4 |
| 20 | Coplanar preprocessing | 4.5.5 | **MOSTLY CORRECT** | Anti-parallel injection correct per Fig. 16; same-direction B-Rep splitting works; T-junction cascading incomplete |

## Verdict Counts

- **CORRECT**: 10 (Steps 1, 4, 7, 10, 11, 12, 13, 14, 15, 17)
- **INCOMPLETE**: 4 (Steps 2, 8a, 16, 18)
- **WRONG**: 2 (Steps 6, 8b)
- **STUB**: 4 (Steps 3, 5, 9, 19)
- **MOSTLY CORRECT**: 1 (Step 20)

## Detailed Findings by Agent

---

### Agent 1: Steps 1-4 (Discretization + Mesh Intersection)

#### Step 1: Error-Bounded Discretization — CORRECT
- d_epsilon = 0.01 * AABB diagonal: computed correctly (yang_integration.rs:591-611)
- adaptive_circle_segments() formula mathematically correct (tessellation/mod.rs:70-81)
- Caveat: vertex parameters computed post-hoc via inverse_evaluate, not sampled during tessellation as paper implies

#### Step 2: Bijective Parametric Mapping — INCOMPLETE
- Triangle-to-face mapping (tri_face_ids) works correctly
- compute_vertex_params() IS called (yang_integration.rs:653-654)
- **BUT vertex_params is NEVER READ anywhere in the pipeline** — dead code
- grep for `.vertex_params[` across boolean/ returns zero results

#### Step 3: CDT Boundary Re-triangulation — STUB
- Yang 4.1.2 requires CDT at surface boundaries for watertightness
- NOT implemented: each patch tessellated independently
- No CGAL CDT integration for boundary handling
- Position-based vertex welding used instead (different mechanism)
- Extended d_epsilon check for boundary triangles: not implemented

#### Step 4: Mesh Intersection (Cherchi) — CORRECT
- Full Cherchi pipeline implemented (solve_intersections, 8 stages)
- Mesh merging with correct A/B labels
- Output correctly split by label back into SubdividedMesh
- d_epsilon NOT used in Cherchi detection (no AABB offset) — paper's conservative detection not enforced

---

### Agent 2: Steps 5-8 (Gauss Map + Optimization)

#### Step 5: Conservative 2*d_epsilon Detection — STUB
- detect_intersections() uses basic AABB overlap (intersection_class.rs:105)
- NO d_epsilon offset applied to AABBs
- O(n^2) brute force, not octree as paper describes
- No boundary triangle (d(T)) handling

#### Step 6: Gauss Map Normal Cone Filtering — WRONG
- NormalCone struct defined (surface.rs:22-28) with may_intersect()
- **may_intersect() is NEVER CALLED** — verified via grep
- Current filter (intersection_class.rs:113-122) only checks same-mesh co-orientation
- Yang 4.2.2 requires cross-mesh patch normal cone overlap check
- These are completely different features

#### Step 7: Newton Optimization — CORRECT
- Residual: S_B - S_A (correct per Appendix C)
- Jacobian: [dS_A/du, dS_A/dv, -dS_B/ds, -dS_B/dt] correct
- Normal equations: JJ^T a = d, update: x += J^T a — correct
- 3x3 Cramer solve works for small system
- Convergence: ||D|| < d_p — correct

#### Step 8a: Geometric Optimization — INCOMPLETE
- Paper: "project r1_k onto intersection line L_k = P_A_k intersect P_B_k"
- Code: computes midpoint of (p_A, p_B), projects INDEPENDENTLY to each surface
- Does NOT compute tangent plane intersection line L
- This is a simplified heuristic, not Yang 4.3.2

#### Step 8b: Method Selection — WRONG
- Code: geometric first, Newton fallback (intersection_opt.rs:415-417)
- Paper: Newton for tangent/boundary cases, geometric for loops (4.3.3)
- No case detection logic — same order for all vertices

---

### Agent 3: Steps 9-12 (Refinement + Classification + Segmentation)

#### Step 9: Curvature-Based Refinement — STUB
- Paper requires 3 conditions: arc_height < d_p*100, chord < d_p*1000, angle < pi/18
- Code uses fixed ~10-degree angular sampling (ssi_refinement.rs:593)
- No recursive subdivision based on curvature
- Comment at line 644: "Future: recursive subdivision for higher accuracy" — admits missing

#### Step 10: Inside/Outside Classification — CORRECT
- BVH-accelerated ray-casting as primary method
- GWN (generalized winding numbers) as fallback for degenerate axes
- Co-surface detection via point-to-plane distance + normal offset
- Consistent with Cherchi 2022

#### Step 11: Cell Selection per Boolean Op — CORRECT
- Selection table matches paper for Union/Subtract/Intersect
- Co-surface rules consistent between face_survival_detect and select_boolean_result
- Union keeps both CoSurfaceInside and CoSurfaceOutside for A
- B-face flipping for Subtract correct

#### Step 12: Flood-Fill Patch Segmentation — CORRECT
- BFS flood-fill across non-boundary edges
- Edge classification: boundary iff reverse edge has different source face
- T-junction splitting at perpendicular junctions (cross-product + parametric t test)
- Multiple loops per patch handled

---

### Agent 4: Steps 13-16 (B-Rep Construction + Failure Recovery)

#### Step 13: B-Rep Construction — CORRECT
- Half-edge creation with next/prev/loop wiring
- Twin pairing prefers different-face matches (multi-entry aware)
- Unpaired HE repair: traces closed chains, synthesizes missing faces
- Falls back to self-twins only if paired_ratio < 0.5

#### Step 14: CDT Mesh Updating — CORRECT
- Calls triangulate_single_triangle with constraint segments
- Samples intermediate points along SSI curves
- Filters degenerate triangles
- Builds new faces/edges/half-edges from CDT output
- Actually called in pipeline (yang_integration.rs:831-836)

#### Step 15: Topology Validation — CORRECT
- Index bounds checking on all half-edge references
- Twin symmetry: he.twin.twin == he
- Euler characteristic with connected component analysis
- P9 enforced: rejects partial topology with boundary HEs

#### Step 16: Optimize Across Boundaries — INCOMPLETE
- Midpoint replacement: finds good neighbors, computes midpoint — correct
- Re-optimization: calls geometric/newton with clamped seeds — correct
- **Step truncation to boundary curve Cb: NOT IMPLEMENTED**
  - clamp_params handles periodic wrapping (e.g., mod 2pi), not boundary truncation
  - Comment claims "Yang 4.5.1 step truncation" but code does seed clamping, not step clamping
- **Surface switching: NOT IMPLEMENTED**
  - No detection of step exiting surface domain
  - No mechanism to identify neighboring surface S1
  - No parameterization switching

---

### Agent 5: Steps 17-20 (Mesh Refinement + Reversal + Coplanar)

#### Step 17: Local Mesh Refinement — CORRECT
- Real re-tessellation (not fake): halves d_epsilon, calls tessellate_waffle_solid
- Rebuilds mesh arrays, bijective maps, re-runs full pipeline
- MAX_REFINEMENT_ROUNDS = 2
- Triggered when remaining_failed_verts > 0

#### Step 18: Reversed Curve Correction — INCOMPLETE
- Detection logic correct:
  - Discrete tangent from polyline neighbors
  - Analytical tangent via n_A x n_B
  - Angle comparison (45-135 degree threshold)
  - Collinearity check
- **CRITICAL: Function takes `&SubdividedMesh` (immutable) — CANNOT modify mesh**
- Only counts and reports reversals — does not remove points
- Paper explicitly requires: "remove its next point and reconnect the curve"

#### Step 19: Self-Intersection Removal — STUB
- validate_yang_result_topology checks topology consistency (bounds, twins, Euler)
- NO actual self-intersection detection (face-face penetration testing)
- count_mesh_self_intersections() exists but only used in tests, not pipeline
- Yang 4.5.4 requires detection + local refinement — neither implemented

#### Step 20: Coplanar Preprocessing — MOSTLY CORRECT
- Detection: both same-direction and anti-parallel pairs — correct
- Same-direction: pre-tessellation B-Rep splitting via split_edge_at + mef — works
- Anti-parallel: post-tessellation three-region injection per Fig. 16 — correct
- T-junction cascading repair incomplete for complex multi-face geometries

---

## Critical Issues (Priority Order)

### HIGH: Geometric method doesn't match paper (Step 8a)
The geometric optimizer projects midpoint to each surface independently instead
of computing tangent plane intersection line L = P_A intersect P_B as Yang 4.3.2
specifies. This is a simplified heuristic that may not converge for difficult cases.

### HIGH: Curvature-based refinement is a stub (Step 9)
The three termination conditions from the paper are not implemented. Fixed angular
sampling is used instead of curvature-adaptive recursive subdivision.

### HIGH: Gauss map filter is wrong (Step 6)
NormalCone::may_intersect() exists but is never called. The current filter only
checks same-mesh co-orientation — a different feature from Yang 4.2.2's cross-mesh
normal cone overlap check.

### HIGH: Reversed curve correction doesn't modify mesh (Step 18)
Function signature is `&SubdividedMesh` (immutable). Can only detect reversals,
not remove them. Paper requires actual point removal and curve reconnection.

### MEDIUM: Method selection is reversed (Step 8b)
Code tries geometric first, Newton as fallback. Paper says Newton for tangent
points and boundary curves, geometric for general intersection loops.

### MEDIUM: CDT boundary re-triangulation missing (Step 3)
Yang 4.1.2 requires CDT at surface boundaries for watertightness. Not implemented.

### MEDIUM: Step truncation not implemented in recovery (Step 16)
clamp_params handles periodic wrapping, not boundary truncation per Yang 4.5.1.
No surface switching mechanism exists.

### MEDIUM: Self-intersection detection missing (Step 19)
Only generic topology validation. No face-face penetration testing per Yang 4.5.4.

### LOW: vertex_params is dead code (Step 2)
compute_vertex_params() populates vertex_params but it's never read.

### LOW: Conservative d_epsilon detection not in Cherchi (Step 5)
d_epsilon is computed and used for tessellation but not for AABB offset in
intersection detection. Cherchi uses standard AABBs without margin.

---

## Methodology

5 parallel audit agents, each assigned 4 contiguous steps:
- Agent 1: Steps 1-4 (Discretization + Mesh Intersection)
- Agent 2: Steps 5-8 (Gauss Map + Optimization)
- Agent 3: Steps 9-12 (Refinement + Classification + Segmentation)
- Agent 4: Steps 13-16 (B-Rep Construction + Failure Recovery)
- Agent 5: Steps 17-20 (Mesh Refinement + Reversal + Coplanar)

Each agent read the Yang 2025 paper text, all implementation code for its
section, and checked the interface between its section and neighbors.
