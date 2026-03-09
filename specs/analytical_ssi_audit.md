# Analytical SSI Audit Report

Sprint 38 WS1 introduced analytical surface-surface intersection (SSI) for
plane-cylinder pairs. This document audits the implementation for correctness,
edge-case coverage, and future extensibility.

## Research Basis

- **[#1] Patrikalakis Ch.5** — SSI algorithms: lattice, marching, subdivision. Ch.6: differential geometry of intersection curves (tangent = n1 x n2).
- **[#27] Li et al. (2026)** — SSI method survey. Hybrid architecture recommended: analytical for simple pairs, topology-guaranteed [#25] for complex pairs.
- **[#25] Yang et al. (2023)** — Topology-guaranteed SSI via Dixon resultant. Determines correct number of IC branches before tracing. Future direction for replacing mesh-based IC extraction.
- **[#32] Piegl & Tiller Ch.7** — Conics and circles as rational NURBS quadratics. Used for exact representation of plane-cylinder IC arcs.

## 1. Overview

The analytical SSI system operates at **two layers**:

| Layer | Location | When | What it does |
|-------|----------|------|--------------|
| **Polyline refinement** (Sprint 38) | `vendor/truck/truck-shapeops/src/transversal/intersection_curve/analytical.rs` | During IC extraction, before `IntersectionCurveWithParameters::try_new` | Projects mesh-based polyline points onto exact ellipse/circle |
| **NURBS arc healing** (Sprint 6) | `crates/kernel-fork/src/healing.rs` (`analytical_circle_arc_from_leader`) | Post-boolean, during `heal_intersection_curves` | Replaces IC edges with exact rational NURBS arcs |

Both layers target the same problem — BSpline drift in plane-cylinder
intersections — but at different stages of the boolean pipeline.

### Data flow

```
  mesh-based polyline (from polygon interference)
         │
         ▼
  [Layer 1] refine_polyline() — projects points onto analytical ellipse
         │
         ▼
  IntersectionCurveWithParameters::try_new() — builds IC with refined leader
         │
         ▼
  boolean operation (classify, divide, finalize)
         │
         ▼
  [Layer 2] heal_intersection_curves() — replaces IC edges with NURBS arcs
         │
         ▼
  healed solid ready for chained booleans
```

## 2. Surface Pairs Handled

### Currently implemented

| Pair | Layer 1 (polyline refinement) | Layer 2 (NURBS healing) |
|------|-------------------------------|------------------------|
| **Plane-Plane** | N/A (not an IC) | Replaced with exact `Line` |
| **Plane-Cylinder** | Ellipse/circle projection (Sprint 38) | Circle arc fitting |
| **Plane-Cone** | Conic section projection (Sprint 40) | Circle arc fitting (circle cases only) |
| **Plane-Sphere** | Circle projection (Sprint 41) | Circle arc fitting |

### Detected but falling back to BSpline

| Pair | Diagnostic message | Status |
|------|-------------------|--------|
| **Cylinder-Cylinder** | `cylinder-cylinder IC detected — BSpline fallback` | Not implemented |
| **Plane-CurvedOther** | (no message) | Falls through to BSpline |
| **Curved-Curved** | (no message) | Falls through to BSpline |

## 3. Algorithm Description

### 3.1 Surface Detection (Layer 1)

**Plane detection** (`detect_plane`, analytical.rs:86-114):
1. Reject surfaces with any periodic direction (planes are never periodic).
2. Evaluate second derivatives (uu, uv, vv) at the parameter midpoint.
3. If all second derivatives have magnitude² < 1e-10, surface is planar.
4. Extract origin = `subs(u_mid, v_mid)`, normal = `normal(u_mid, v_mid)`.

**Cylinder detection** (`detect_cylinder`, analytical.rs:120-210):
1. Require exactly one periodic direction with period within 0.1 of 2π.
2. In the non-periodic (axial) direction, require zero second derivative (magnitude² < 1e-8).
3. Extract the axis from the first derivative in the axial direction.
4. Sample 3 points at 0, period/3, 2·period/3 in the angular direction.
5. Fit circumscribed circle through the 3 points (`circumcenter_3d`).
6. Validate with a 4th point at period/4 — radius must match within 1%.

### 3.2 Plane-Cylinder Intersection (Layer 1)

**`compute_plane_cylinder_intersection`** (analytical.rs:216-255):

Given plane with normal **n** and origin **o**, cylinder with axis **a**, center **c**, radius **r**:

1. Compute `dot_na = n · a`. If `|dot_na| < 1e-6`, plane is parallel to axis → return None.
2. Ellipse center: `center = c + t·a` where `t = n·(o - c) / dot_na`.
3. Build orthonormal frame {E1, E2} perpendicular to **a**.
4. Project onto plane: `U = r·(E1 - (n·E1/dot_na)·a)`, `V = r·(E2 - (n·E2/dot_na)·a)`.
5. Intersection curve: `X(θ) = center + cos(θ)·U + sin(θ)·V`.

**Mathematical correctness**: The formula is correct. For perpendicular cuts (`n ∥ a`),
|U| = |V| = r (circle). For oblique cuts, one axis stretches by 1/cos(α) where α is
the angle between **n** and **a** (ellipse). The test at analytical.rs:438-473
verifies the 45° case produces minor=1, major=√2.

### 3.3 Polyline Refinement (Layer 1)

**`refine_polyline`** (analytical.rs:312-323):
- Maps each point in the mesh polyline through `project_to_ellipse`.
- Preserves point count and topology (open/closed, ordering).

**`project_to_ellipse`** (analytical.rs:331-379):
1. Compute 2D coordinates of point in ellipse frame: `du = d·U`, `dv = d·V`.
2. Initial guess: `θ₀ = atan2(dv, du)` (exact for circles).
3. For non-circular ellipses (|U·V| > 1e-12 or ||U|² - |V|²| relative > 1e-12):
   Newton refinement for 5 iterations on the distance-minimizing equation:
   `f(θ) = sin(θ)·du - cos(θ)·dv + sin(θ)cos(θ)·(|V|²-|U|²) + (cos²θ-sin²θ)·(U·V)`

### 3.4 NURBS Arc Healing (Layer 2)

**`analytical_circle_arc_from_leader`** (healing.rs:110-216):
1. Require at least one surface to be `Surface::Plane`.
2. Sample the leader curve at front, midpoint, and back.
3. Fit circumscribed circle through the 3 points.
4. Validate radius consistency within 0.01 (loose, since leader has ~1e-3 error).
5. Build local coordinate frame (X = center→front, Z = plane normal, Y = Z×X).
6. Compute arc angle from front to back; verify midpoint is inside the arc.
7. Construct exact NURBS arc via `TrimmedCurve<UnitCircle>` → transform.
8. Validate: 20 sample points must be within `TOLERANCE * 0.5` of both surfaces.

**Key difference from Layer 1**: Layer 2 produces a *circle arc only*, not an
ellipse. It works on the leader BSpline curve *after* the boolean, while
Layer 1 works on the mesh polyline *before* IC construction.

## 4. Edge Cases Analysis

### 4.1 Tested Edge Cases

| Edge Case | Test | Status |
|-----------|------|--------|
| Perpendicular plane-cylinder (circle) | `test_plane_cylinder_perpendicular` | Covered |
| 45° oblique cut (ellipse) | `test_plane_cylinder_oblique_45deg` | Covered |
| Parallel plane (returns None) | `test_plane_parallel_to_cylinder_returns_none` | Covered |
| Non-plane/cylinder surfaces (returns None) | `test_analytical_fallback_non_plane_cylinder` | Covered |
| Plane detection from truck `Plane` type | `test_detect_plane_from_truck_plane` | Covered |
| Cylinder detection from `RevolutedCurve<Line>` | `test_detect_cylinder_from_revolved_line` | Covered |
| Plane not detected as cylinder | `test_plane_not_detected_as_cylinder` | Covered |
| Cylinder not detected as plane | `test_cylinder_not_detected_as_plane` | Covered |
| Full detection from truck surfaces | `test_analytical_plane_cylinder_detection` | Covered |
| Polyline refinement (noisy points) | `test_refine_polyline_circle` | Covered |
| Point count preservation | `test_refine_preserves_point_count` | Covered |
| Circle projection correctness | `test_project_to_ellipse_circle` | Covered |
| Ellipse sampling closure | `test_sample_ellipse_closure` | Covered |
| Circumcenter equilateral triangle | `test_circumcenter_equilateral` | Covered |
| Collinear points (returns None) | `test_circumcenter_collinear_returns_none` | Covered |

### 4.2 Untested Edge Cases — Risk Assessment

| Edge Case | Risk | Description |
|-----------|------|-------------|
| **Tangential intersection** (cylinder just touches plane) | **HIGH** | When dot_na ≈ 1.0 (axis perpendicular to plane) but the plane just grazes the cylinder at its edge, the intersection degenerates to a point or zero-length curve. The current code would produce a valid but meaningless ellipse. The mesh polyline would likely have 0-1 points, which `try_new` rejects (len < 2 guard at mod.rs:33), so this is likely safe in practice. |
| **Near-parallel plane** (dot_na close to 1e-6 threshold) | **MEDIUM** | As the plane approaches parallelism with the cylinder axis, the ellipse major axis → ∞. The 1e-6 threshold prevents infinite axes but may still produce very elongated ellipses (e.g., major/minor > 1000) that are numerically ill-conditioned for projection. |
| **Very small cylinder** (radius < 1e-6) | **LOW** | `detect_cylinder` rejects radius < 1e-10, but radii between 1e-10 and 1e-6 may produce ellipses whose axes are at floating-point noise level. In practice, CAD models rarely have sub-micron features. |
| **Very large cylinder** (radius > 1e6) | **LOW** | No upper bound check on radius. Circumcenter computation may lose precision for very large radii (relative errors in cross-product). Unlikely in practice. |
| **Zero-radius cylinder** | **NONE** | Explicitly rejected by `radius < 1e-10` guard. |
| **Oblique angle near 0°** (nearly perpendicular) | **NONE** | Produces a circle (well-conditioned). |
| **Oblique angle near 90°** (nearly parallel) | **MEDIUM** | Same as near-parallel case above. |
| **Cylinder axis not unit-length** | **NONE** | `detect_cylinder` normalizes the axis. |
| **Negative cylinder radius** | **NONE** | Radius is computed from point distances, always positive. |
| **Cylinder not aligned with coordinate axes** | **LOW** | The orthonormal frame construction uses `unit_x` or `unit_y` pivot. If the cylinder axis is exactly [1,0,0] or [0,1,0], the fallback works. If the axis is near these directions, the cross product may be small but the 0.9 threshold handles this. Not tested with arbitrary orientations but the math is correct. |
| **Reversed cylinder winding** | **LOW** | Detection samples at positive angles. If the cylinder parameterization is reversed, the 4th-point validation catches inconsistency. |
| **Polyline with duplicate points** | **MEDIUM** | `project_to_ellipse` would project both to the same point. The downstream `IntersectionCurveWithParameters::try_new` does `search_triple` which may fail on zero-length segments. |
| **Polyline with single point** | **NONE** | `try_new` rejects polylines with < 2 points (mod.rs:33). |
| **Ellipse projection at center** | **MEDIUM** | If a mesh point happens to be exactly at the ellipse center, `atan2(0, 0)` returns 0, projecting to `center + axis_u`. This is correct (closest point on the ellipse from the center is ambiguous; any point is equally valid). |
| **Newton divergence in `project_to_ellipse`** | **LOW** | Limited to 5 iterations with good initial guess from atan2. The derivative `fp` has a 1e-15 zero guard. Very eccentric ellipses (near-parallel cuts) could theoretically oscillate, but in 5 iterations the impact is bounded. |
| **NurbsSurface cylinder** (NURBS-represented, not RevolutedCurve) | **MEDIUM** | `detect_cylinder` relies on periodicity. NURBS surfaces stored as `BSplineSurface` lack periodicity metadata and would not be detected, falling through to mesh-based extraction. This is the expected behavior — Layer 2 (healing.rs) handles this via circle fitting on the leader curve. |

### 4.3 Interaction Between Layers 1 and 2

The two analytical layers are **complementary, not redundant**:

- **Layer 1** (polyline refinement) improves IC *construction* quality. Operates on
  generic `ParametricSurface3D` trait objects. Detects via derivative analysis.
- **Layer 2** (NURBS healing) improves the *final edge curves*. Operates on
  concrete `Surface` enum variants. Detects via `Surface::Plane` pattern match.

A plane-cylinder intersection typically passes through **both** layers:
1. Layer 1 refines the mesh polyline → better IC leader curve.
2. Layer 2 then replaces the IC edge with an exact NURBS arc.

If Layer 1 fails (e.g., NURBS-stored cylinder), Layer 2 still works from the
leader curve. If Layer 2 fails (e.g., the IC is an ellipse, not a circle),
the refined polyline from Layer 1 still provides better BSpline accuracy.

**Potential concern**: Double refinement could mask Layer 1 bugs. If Layer 1
produces incorrect ellipse parameters, the refined polyline will have wrong
points, but Layer 2 may still produce a correct NURBS arc (since it fits
independently from the leader). The healed solid would be correct, but IC
accuracy during the boolean itself (classification, face division) could be
affected.

## 5. Test Coverage Summary

### Layer 1 Tests (analytical.rs, 15 tests)

| # | Test | Category |
|---|------|----------|
| 1 | `test_circumcenter_equilateral` | Math utility |
| 2 | `test_circumcenter_collinear_returns_none` | Math utility |
| 3 | `test_plane_cylinder_perpendicular` | Core algorithm |
| 4 | `test_plane_cylinder_oblique_45deg` | Core algorithm |
| 5 | `test_plane_parallel_to_cylinder_returns_none` | Rejection |
| 6 | `test_detect_plane_from_truck_plane` | Detection |
| 7 | `test_detect_cylinder_from_revolved_line` | Detection |
| 8 | `test_plane_not_detected_as_cylinder` | Cross-detection |
| 9 | `test_cylinder_not_detected_as_plane` | Cross-detection |
| 10 | `test_analytical_fallback_non_plane_cylinder` | Rejection |
| 11 | `test_analytical_plane_cylinder_detection` | Integration |
| 12 | `test_refine_polyline_circle` | Refinement |
| 13 | `test_refine_preserves_point_count` | Refinement |
| 14 | `test_project_to_ellipse_circle` | Projection |
| 15 | `test_sample_ellipse_closure` | Sampling |

### Layer 2 Tests (healing.rs, relevant subset)

| # | Test | Category |
|---|------|----------|
| 1 | `test_analytical_arc_quality` | End-to-end healing |
| 2 | `test_healed_solid_supports_chained_boolean` | Chained operations |
| 3 | `test_classify_surface_pair_*` (3 tests) | Surface classification |

### Gaps

1. **No oblique ellipse projection test.** `test_project_to_ellipse_circle` tests
   circle projection only. There's no test of the Newton-refined path for genuinely
   eccentric ellipses (e.g., 30° or 60° oblique cuts).
2. **No near-parallel threshold test.** No test verifying behavior when `dot_na` is
   just above 1e-6 (extremely elongated ellipse).
3. **No arbitrary-orientation test.** All tests use axis-aligned cylinders (Z-axis).
   Missing tests for cylinders along [1,1,1] or other non-axis directions.
4. **No off-center cylinder test.** All test cylinders are centered at or near the
   origin. Missing test for cylinder at e.g. center=[50, 30, 10].
5. **No integration test through full boolean.** The tests are all unit tests on
   isolated functions. There's no test that verifies analytical refinement improves
   an actual boolean operation's output.
6. **No test for Layer 1 + Layer 2 interaction.** No test that verifies both layers
   activate on the same plane-cylinder pair and produce consistent results.

## 6. Phase 2 Recommendations

### 6.1 Next Surface Pairs to Add

P1 (plane-cone) and P2 (sphere-plane) are now implemented (Sprints 40-41). Remaining:

| Priority | Pair | Intersection Type | Difficulty | Notes |
|----------|------|-------------------|------------|-------|
| ~~**P1**~~ | ~~**Plane-Cone**~~ | ~~Conic section~~ | ~~Medium~~ | **DONE** (Sprint 40). `detect_cone` + `compute_plane_cone_intersection`. Handles perpendicular (circle), oblique (ellipse), through-apex (None), near-parallel (None). |
| ~~**P2**~~ | ~~**Sphere-Plane**~~ | ~~Circle~~ | ~~Easy~~ | **DONE** (Sprint 41). `detect_sphere` + `compute_sphere_plane_intersection`. Always a circle. |
| **P3** | **Cylinder-Cylinder** | Bezout curve (degree ≤ 4) | Hard | `SurfacePairType::CylinderCylinder` already detected. Intersection is generally a degree-4 space curve. Special cases: parallel axes → pair of lines/ellipses; perpendicular axes → degree-4 curve. |
| **P4** | **Torus-Plane** | Degree-4 curve | Hard | Required for revolve+boolean operations. Currently all 8 revolve+boolean tests fail (torus-plane IC unsupported). See `crates/test-harness/tests/revolve_boolean.rs`. |

### 6.2 Architectural Recommendations

1. **Unify the two layers.** Both layers solve the same problem with similar math
   (circumcenter fitting, plane detection). Consider:
   - Making Layer 1 produce the NURBS arc directly, eliminating the IC entirely for
     recognized pairs.
   - Or, extending Layer 1's `AnalyticalIC` to carry enough information for Layer 2
     to skip the re-fitting step.

2. ~~**Add a `detect_cone` function.**~~ **DONE** (Sprint 40).

3. ~~**Add a `detect_sphere` function.**~~ **DONE** (Sprint 41).

4. **Extend `EllipseParams` to `ConicParams`.** For plane-cone, the intersection
   can be any conic section. Currently only ellipses are handled — parabolas and
   hyperbolas (from near-parallel or through-apex cuts) return None. Generalize
   the parameterization to handle parabolas and hyperbolas.

5. **Add near-parallel safeguard.** When `dot_na` is small (say < 0.01), compute
   the ellipse eccentricity and warn or reject if it exceeds a threshold. Very
   eccentric ellipses (eccentricity > 0.999) are numerically equivalent to the
   parallel case and should fall through to mesh-based extraction.

6. **Coplanar face merging needs research.** Sprint 41 attempted a post-boolean
   `merge_coplanar_faces` pass to simplify redundant topology (e.g., abutting
   box union producing 10 faces instead of 6). The implementation caused 3
   regressions and was reverted. See Section 8 for details and required research.

## 7. Correctness Concerns

### 7.1 No significant bugs found

The implementation is mathematically correct for its intended scope (plane-cylinder
intersections producing ellipses/circles). The ellipse parameterization formula is
standard, the Newton projection converges quickly, and the guards against degenerate
inputs are reasonable.

### 7.2 Minor concerns

1. **Threshold asymmetry.** Plane detection uses `1e-10` for second derivatives while
   cylinder detection uses `1e-8`. The 100x difference could cause a surface that is
   "barely not a plane" (say, a very gentle BSpline) to be detected as a plane. In
   practice, this is unlikely because truck's `Plane` type has exactly zero second
   derivatives, but it could be a concern for NURBS-approximated planes.

2. **`_tol` parameter unused.** `try_analytical_plane_cylinder_ic` accepts a `_tol`
   parameter but ignores it (the name with leading underscore confirms this). The
   detection thresholds are hardcoded. This means the analytical path uses different
   tolerances than the rest of the boolean pipeline. Consider using `tol` to scale
   the detection thresholds.

3. **Cylinder detection period tolerance (0.1).** The check
   `(period - 2π).abs() > 0.1` accepts periods from about 6.18 to 6.38. While 2π ≈
   6.283, a tolerance of 0.1 (1.6%) is generous. This could theoretically match a
   non-cylindrical surface with a near-2π period, though such surfaces are uncommon.

4. **4th-point validation tolerance (1%).** `detect_cylinder` validates the circle fit
   using a 4th point with 1% relative tolerance. For large-radius cylinders (r=1000),
   this allows 10 units of error. Consider using an absolute + relative combined
   threshold: `max(radius * 0.001, 1e-6)`.

5. **`sample_ellipse` is `#[cfg(test)]` only.** If this function is ever needed at
   runtime (e.g., for diagnostics or visualization), it would need to be made public.
   This is not a bug, just a note.

6. **Projection ambiguity at ellipse center.** As noted in Section 4.2, projecting a
   point at the exact ellipse center gives `atan2(0,0) = 0`, which maps to
   `center + axis_u`. This is mathematically valid (all points on the ellipse are
   equidistant from the center) but could cause inconsistent behavior if two nearby
   polyline points straddle the center. In practice, mesh polyline points lie on or
   near the cylinder surface, never at the ellipse center, so this is academic.

### 7.3 Positive observations

1. **Layer 1 preserves mesh topology.** By projecting points rather than re-sampling,
   the refined polyline maintains the same connectivity as the mesh-based one. This
   means trimming, clipping, and closed-loop detection all work unchanged.

2. **Graceful fallback.** Both layers return `None` on any detection failure, falling
   through to the existing mesh-based/BSpline path. No panics or error states.

3. **Both orders tried.** `try_analytical_plane_cylinder_ic` tries (surface0=plane,
   surface1=cylinder) and (surface0=cylinder, surface1=plane), handling both argument
   orderings.

4. **Newton refinement is bounded.** 5 iterations with a convergence check prevents
   runaway computation.

## 8. Sprint 41: Coplanar Face Merging — Attempted and Reverted

### 8.1 Goal

After boolean operations on axis-aligned boxes, coplanar face fragments remain
in the result shell. For example, unioning two abutting boxes [0,2]³ and
[1,3]×[0,2]×[0,2] produces 14 faces instead of the minimal 6 (the shared
internal faces at x=1 and x=2 get split, and the top/bottom/front/back each
have two coplanar fragments).

The goal was to merge these fragments post-boolean to reduce topology complexity
and fix Euler characteristic violations (e.g., T2 box-cylinder union had chi=3
instead of chi=2).

### 8.2 Implementation (Reverted)

Added to `finalize_boolean_shell_inner` in `integrate/mod.rs`, after weld:

1. `is_planar_surface(S)` — check second derivatives at parameter midpoint
2. `build_merged_wire(face_a, face_b)` — remove shared edges, order remaining
   edges by vertex connectivity into a closed wire
3. `merge_coplanar_faces(shell, tols)` — iteratively find coplanar face pairs
   sharing edges, merge them via `build_merged_wire`, create new Face

### 8.3 Failure Modes (3 Regressions)

**test_robust_ray_grazing_edge**: Two boxes [0,2]³ and [1,3]×[0,2]×[0,2] union.
Expected volume 12.0, got 8.0 (one box). The merge collapsed 14 faces down to
6, but the resulting 6-face solid was a cube (volume 8) not the elongated union
(volume 12). Root cause: the merged wire traced an incorrect boundary, likely
because edge orientation was wrong after combining edges from two different face
boundaries.

**test_coplanar_angular_threshold**: Stacked boxes sharing a face union failing.
Same class of issue — the merge produced incorrect wire topology.

**test_degenerate_closed_ic_no_panic**: Coplanar-adjacent box union volume wrong.

### 8.4 T2 Already Passes Without Merge

T2 (`t2_euler_invariant_box_cylinder_union`) already passes with chi=3, which
is correct: the annular ring face (box top minus cylinder footprint) has 1 inner
boundary loop. Generalized Euler: V - E + F - L_inner = 2 → chi = 3.

The merge was solving a problem that didn't actually need solving for T2.

### 8.5 Research Needed for Future Attempt

The fundamental approach (merge coplanar adjacent faces) is correct — commercial
kernels like OCCT do this routinely. But the implementation has several issues:

1. **Edge orientation in merged wires.** When two faces share an edge, the edge
   appears with opposite orientations in each face's boundary wire. The current
   `build_merged_wire` uses `boundaries()` (orientation-corrected) edges, but
   doesn't account for the fact that shared edges have **different semantics** in
   each face. Simply removing shared edges and concatenating non-shared ones can
   produce a wire that doesn't trace the correct merged boundary.

2. **Surface compatibility.** The merged face uses face_a's surface. If face_a
   and face_b have different surface parameterizations (common after boolean
   operations split a face), the merged wire may not be compatible with face_a's
   surface domain. Need to verify that all wire edges' parameter ranges map
   correctly onto the chosen surface.

3. **Over-aggressive merging.** The current code merges ANY two coplanar faces
   sharing an edge. It should only merge faces that produce a simpler topology
   (fewer edges, valid simple wire). Merging faces that create a more complex
   wire (e.g., non-convex merged boundary) may be incorrect.

4. **Reference approaches to study:**
   - OCCT's `ShapeUpgrade_UnifySameDomain` — merges same-domain faces/edges
   - CGAL's `Polygon_mesh_processing::remove_degenerate_faces`
   - Academic: Braid (1978) "Design with Volumes" — face merger via Euler
     operators (MFKRH = Make Face, Kill Ring, Hole)

5. **Suggested implementation strategy for retry:**
   - Only merge faces where the merged wire is **strictly simpler** (fewer edges)
   - Validate merged wire area matches sum of original face areas (within tolerance)
   - Validate merged face normal matches original normals
   - Start with the most conservative case: merge only when exactly 2 faces share
     exactly 1 edge, and the resulting wire has exactly N-2 edges (where N is the
     sum of original edge counts)
   - Test against the full kernel-fork boolean_properties suite before committing

---

*Audit performed: Sprint 39, 2026-02-25*
*Updated: Sprint 41, 2026-02-26 (plane-cone, sphere-plane, coplanar merge findings)*
*Implementation audited: Sprint 38 WS1 (commit 86b334e), Sprint 40 WS1, Sprint 41 WS2-WS3*
