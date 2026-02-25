# Phase 3: Healing + Perturbation Analysis

## Overview

This document analyzes the perturbation cascade (`try_boolean_with_perturbation`) and IC edge healing (`heal_intersection_curves`) in `crates/kernel-fork/src/healing.rs`, maps each hack to the literature's correct algorithmic solution, and assesses whether each hack is a workaround for a known algorithmic deficiency in the core boolean pipeline.

**Files under review:**
- `crates/kernel-fork/src/healing.rs` (~1900+ lines)
- `vendor/truck/truck-shapeops/src/transversal/intersection_curve/` (IC representation)

**References consulted:**
- Patrikalakis et al. (Ch.5 — analytical SSI for quadric surfaces)
- Levy 2025 (exact constructions, homogeneous coordinates)
- OCCT General Fuse Algorithm (staged interference, no perturbation)
- Edelsbrunner-Mucke (Simulation of Simplicity — virtual perturbation)
- Sugihara-Iri (topology-oriented implementation)
- Zhou 2016 (exact predicates + winding numbers, no perturbation)
- Shewchuk (adaptive precision predicates)

---

## Section 1: Perturbation Cascade (`try_boolean_with_perturbation`)

### 1.1 Architecture

The perturbation cascade (`healing.rs:1217-1704`) is a brute-force retry mechanism that physically translates or scales one solid when the boolean pipeline fails. It is the **outermost recovery layer** in the boolean stack, wrapping the truck-shapeops pipeline.

**Key parameters:**
- 120-second cumulative timeout (`cascade_timeout`, line 1307)
- Scale-aware epsilon computation from bounding box extent (`solid_max_extent`, line 1391)
- Adaptive strategy selection based on face count (>30 triggers aggressive mode, line 1415)
- Pre-healing: vertex unification via `heal_shell_vertices` (line 1255)
- Panic catching via `catch_unwind` around every attempt (line 1320)

### 1.2 Strategy Inventory

The cascade contains **11 named strategies** that, depending on geometry complexity and coplanar configuration, can produce **up to 52+ attempts** before exhaustion:

| # | Strategy | Label | Line | Trigger | Max Attempts | What It Does |
|---|----------|-------|------|---------|-------------|-------------|
| 0 | Direct | `direct` | 1366 | Always | 1 | Try the boolean without modification |
| 1 | Scale-expand (early) | `scale-expand` | 1436 | `use_aggressive && dirs.len() >= 2` | 3 | Scale solid_b by 1.02/1.03/1.05 from centroid |
| 2 | Corner-coplanar | `corner-coplanar` | 1453 | `detect_corner_coplanar` returns Some | 2 | Translate along cross-product of 2+ coplanar normals |
| 3 | Composite | `composite` | 1479 | `dirs.len() > 1` | 3-4 | Translate along sum of all coplanar directions |
| 4 | Coplanar-dir | `coplanar-dir` | 1500 | `!dirs.is_empty()` | N×3 or N×4 | Translate along each individual coplanar normal |
| 5 | Cylinder-dir | `cylinder-dir` | 1522 | Revolved surfaces detected | M×3 or M×4 | Translate perpendicular to cylinder axes |
| 6 | Diagonal | `diagonal` | 1548 | `dirs.len() >= 2 \|\| !cyl_dirs.is_empty()` | 4×3 or 4×4 | Translate along (1,1,0), (1,0,1), (0,1,1), (1,1,1) |
| 7 | Asymm-scale | `asymm-scale` | 1574 | Always (fallback) | 4 | Scale solid_b asymmetrically along individual axes |
| 8 | Scale-expand (late) | `scale-expand` | 1614 | `dirs.len() >= 2` | 3 | Repeat of strategy 1 for non-aggressive path |
| 9 | Cardinal | `cardinal` | 1641 | Always (fallback) | 7 | Translate solid_b by fixed 1e-5 in axis combinations |
| 10 | Cardinal-A | `cardinal-A` | 1662 | Always (fallback) | 7 | Translate solid_a instead of solid_b |
| 11 | Large-final | `large-final` | 1675 | `face_count > 20` | 4 | Translate solid_b by 1% of extent |

**Worst-case attempt count**: 1 (direct) + 3 (scale-expand early) + 2 (corner) + 4 (composite) + N×4 (coplanar) + M×4 (cylinder) + 16 (diagonal) + 4 (asymm-scale) + 3 (scale-expand late) + 7 (cardinal) + 7 (cardinal-A) + 4 (large-final) = **55+ attempts** (where N = coplanar directions, M = cylinder directions).

### 1.3 Root Cause Analysis: Why Each Strategy Exists

Each perturbation strategy addresses a specific degenerate geometric configuration that the core boolean pipeline cannot handle. The following table maps each strategy to the **root cause** it works around, the **degenerate configuration class** (D1-D5 from Phase 1), and the **literature's correct solution**:

| Strategy | Root Cause | Degenerate Class | Literature's Correct Solution |
|----------|-----------|-----------------|------------------------------|
| Scale-expand (early/late) | Coincident edges between tool and target after chained booleans | D2 (coincident edges) | OCCT Pave Block splitting (detect and split coincident edges explicitly) |
| Corner-coplanar | Multiple face planes intersect at a corner, creating multi-face coplanarity | D1 (coplanar faces) + D2 | Exact coplanar overlay via topology-oriented face splitting (Sugihara-Iri) |
| Composite | Multiple independent coplanar face pairs | D1 | Full coplanar pre-scan and 2D overlay (OCCT Same-Domain analysis) |
| Coplanar-dir | Single coplanar face pair creates degenerate intersection | D1 | Exact orient3d with SoS (Edelsbrunner-Mucke) to resolve face classification |
| Cylinder-dir | Cylinder surface edges coincide with planar face boundaries | D2 + D4 (tangential) | Analytical plane-cylinder SSI (Patrikalakis Ch.5) — exact ellipse/circle curves |
| Diagonal | Corner/edge alignment not broken by axis-aligned perturbation | D2 + D3 (vertex-on-face) | SoS virtual perturbation (Edelsbrunner-Mucke) — handles all alignment cases without physical perturbation |
| Asymm-scale | Tool edges overlap target edges, producing degenerate mesh intersection | D2 | Exact edge-edge coincidence detection and splitting (OCCT VE/EE interference) |
| Cardinal/Cardinal-A | Generic fallback for unknown degenerate configurations | Unknown | Full SoS + exact predicates + winding number classification (Zhou 2016) — eliminates need for any physical perturbation |
| Large-final | Deep edge alignment in multi-boolean results that resists small perturbations | D2 + D5 (non-manifold from chaining) | Correct-by-construction results via Nef polyhedra (Hachenberger) or exact arithmetic (Levy 2025) |

### 1.4 Spec Violations

The perturbation cascade violates **4 production spec requirements**:

**V1: No-panic requirement** (spec lines 336-337: "No panics on valid inputs")
- `catch_unwind` at line 1320 masks an unknown number of panic paths in truck internals (e.g., "knot vector consists single value"). The spec requires structured errors, not panic suppression.
- `repair_non_manifold_shell` is detection-only (line 1277-1286) — non-manifold issues are logged but not repaired, then silently discarded.

**V2: Determinism** (spec lines 48-51: "identical results for identical inputs")
- Physical perturbation is deterministic given fixed inputs (scale-aware epsilons from bounding box). However, the 120s timeout (line 1307) introduces **platform-dependent non-determinism**: the same geometry may succeed or fail depending on CPU speed. On slow machines, early strategies consume more of the timeout budget, preventing later strategies from executing.
- WASM builds (#[cfg(not(target_arch = "wasm32"))]) skip the timeout entirely, producing different attempt sequences than native builds.

**V3: 1 µm feature preservation** (spec lines 57-59, 81-82, 277)
- Scale-expand by 2-5% (factors 1.02, 1.03, 1.05 at lines 1438, 1625) modifies tool geometry by up to 5% of its extent. For a 1mm tool, this is 50 µm — 50× the minimum feature size.
- Large-final at 1% of extent (line 1676) can perturb by 100 µm on a 10mm body.
- Cardinal perturbation uses a fixed 1e-5 offset (10 µm, line 1643) regardless of feature scale.
- **None of these perturbations are bounded by `L_min_feature`** — the spec's 1 µm preservation guarantee is violated whenever perturbation succeeds.

**V4: Structured errors** (spec lines 285-294: "MUST NOT collapse failures into Option::None")
- `try_boolean_with_perturbation` returns `Result<Solid, BooleanStageError>`, which is correct at the type level. However, when the cascade exhausts all attempts, the returned error is the **last failure's error** (line 1704: `Err(last_err)`), which may be from a `large-final` perturbation attempt that has nothing to do with the original failure's root cause.
- Pre-heal vertex unification failures are collapsed to `None` (line 1294: `Solid::try_new(shells).ok()`), discarding the Solid topology error.

### 1.5 Percentage of Strategies Addressing Root Causes

**0% of perturbation strategies address the root causes identified in Phase 1 and Phase 2.**

Every strategy is a **physical geometry modification** that avoids a degenerate configuration by moving geometry away from it. The literature's solutions instead **handle degeneracies directly**:

- **SoS (Edelsbrunner-Mucke)**: Virtual infinitesimal perturbation that provably resolves all degeneracies without modifying geometry. Only `orient2d` has partial SoS in our codebase (`sos_orient2d_tiebreak` in `robust_classify.rs`). `orient3d`, `incircle`, and `insphere` lack SoS.
- **Exact predicates (Shewchuk/Levy)**: Correct sign decisions under floating-point arithmetic. We use Shewchuk `orient2d`/`orient3d` for ray-cast classification, but not for face classification, intersection construction, or coplanar detection.
- **Winding numbers (Zhou 2016)**: Replaces ray-cast classification entirely. Provably correct for any mesh arrangement, including degeneracies. Not implemented.
- **Topology-oriented (Sugihara-Iri)**: Separates topological decisions from geometric computation. Our pipeline entangles them (geometry failures cascade to topology failures).

### 1.6 Perturbation Success Rate (Estimated)

Based on the cascade structure and test suite behavior:

| Scenario | Typical Attempts | Success Strategy | Time |
|----------|-----------------|-----------------|------|
| Simple box-box (no degeneracy) | 1 | Direct | <1s |
| Box-box with 1 coplanar face | 2-5 | Coplanar-dir | 1-3s |
| Box-cylinder (tangential) | 3-8 | Cylinder-dir or asymm-scale | 2-10s |
| K8 (3 bosses + 3 cuts, 31 faces) | 5-15 | Scale-expand (early) | 15-60s |
| Corner-coplanar (2+ face alignment) | 3-10 | Corner-coplanar or scale-expand | 5-30s |
| Overlapping coplanar cuts | 5-20 | Varies | 10-60s |
| Failure (timeout) | 52+ | None | 120s |

The cascade spends most of its budget on **complex shells (>30 faces)** where each attempt takes 10-15 seconds. With a 120s timeout, only ~8-10 attempts fit, making strategy ordering critical (hence the `use_aggressive` flag and `scale-expand-first` ordering for complex shells).

---

## Section 2: IC Edge Healing (`heal_intersection_curves`)

### 2.1 Why IC Healing Exists

Truck's `IntersectionCurve` type (`intersection_curve/`) stores `Box<Surface>` references to both parent surfaces and re-runs Newton iteration (`double_projection`) on every `subs(t)` call. This design means:

1. **No closed-form evaluation**: Every point query requires iterative convergence.
2. **Fragility across booleans**: When a solid containing IC edges enters a second boolean, `curve_surface_projection` in `create_loops_stores` attempts to project intersection polyline endpoints onto these IC edges. The nested Newton iteration (projection onto an IC that itself uses Newton) frequently fails to converge.
3. **Error accumulation**: Each healing pass introduces approximation error (~5e-7 per BSpline replacement). After N chained booleans, cumulative error reaches N × 5e-7, exceeding truck's TOLERANCE=1e-6 after ~2 operations.

IC healing is therefore a **necessary workaround** for truck's IC architecture, not a choice. Without it, chained booleans fail after the first operation.

### 2.2 Healing Strategy Pipeline

The `heal_intersection_curves` function (`healing.rs:336-535`) processes each IC edge through a prioritized strategy pipeline:

| Priority | Strategy | Line | Surface Pair | Curve Type Output | Error |
|----------|----------|------|-------------|-------------------|-------|
| 0 | Exact line | 358-363 | Plane-Plane | `Line` | 0 (exact) |
| 1 | Analytical NURBS arc | 400-409 | Plane-Cylinder | `NurbsCurve` | ~1e-14 (machine precision) |
| 2 | Clone leader BSpline | 455-457 | Any | `BSplineCurve` | ~1e-4 to 1e-3 (leader fit error) |
| 3 | Re-approximate (tight tol) | 471-484 | Any | `BSplineCurve` | ~1e-7 to 1e-6 (controlled) |
| 4 | Re-approximate from IC | 499-510 | Any | `BSplineCurve` | ~5e-7 (best-effort, catch_unwind) |
| 5 | Best candidate (any residual) | 520-524 | Any | `BSplineCurve` | Unknown (best available) |
| 6 | Line fallback | 528 | Any | `Line` | Large (loses curvature) |

**Critical observation**: Strategies 0 and 1 are **exact** (zero or machine-precision error). Strategies 2-6 introduce **approximation error** that compounds across chained booleans.

### 2.3 Surface Pair Classification

The `classify_surface_pair` function (`healing.rs:835-853`) identifies the IC surface types to select the optimal healing strategy:

| Surface Pair | Classification | Analytical Solution Known? | Currently Implemented? |
|-------------|---------------|--------------------------|----------------------|
| Plane-Plane | `PlanePlane` | Yes — straight line | Yes (exact Line) |
| Plane-Cylinder | `PlaneCylinder` | Yes — circle/ellipse | **Partial** (circle arc only; ellipse for oblique cuts missing) |
| Plane-Cone | `PlaneCone` | Yes — conic section (ellipse/parabola/hyperbola) | **No** — logged as unimplemented (line 373), falls through to BSpline |
| Cylinder-Cylinder | `CylinderCylinder` | Yes — ellipse or Viviani curve | **No** — logged as unimplemented (line 381), falls through to BSpline |
| Plane-Other | `PlaneCurvedOther` | Sometimes (NURBS/BSpline surfaces) | No — BSpline fallback |
| Curved-Curved | `CurvedCurved` | Rarely (general case requires marching) | No — BSpline fallback |

### 2.4 Comparison to Literature

**Patrikalakis Ch.5 — Analytical SSI for Quadric Surfaces:**
Patrikalakis provides closed-form solutions for all quadric-quadric surface intersections (plane-plane, plane-cylinder, plane-cone, plane-sphere, cylinder-cylinder, cylinder-cone, cylinder-sphere, cone-cone, cone-sphere, sphere-sphere). These produce exact parametric curves (lines, circles, ellipses, hyperbolas, parabolas) with zero approximation error.

Our implementation covers only 2 of the 10 quadric-quadric cases:
- Plane-Plane → Line (exact) ✓
- Plane-Cylinder → Circle arc (when the intersection is a circle, not an ellipse) ✓

Missing analytical cases that would eliminate BSpline error:
- Plane-Cylinder (oblique) → Ellipse
- Plane-Cone → Conic section
- Plane-Sphere → Circle
- Cylinder-Cylinder → Ellipse / Viviani curve
- Cylinder-Cone → Quartic curve (BSpline may be needed)
- Cylinder-Sphere → Quartic curve

**Levy 2025 — Exact Constructions:**
Levy's approach uses homogeneous coordinates to represent intersection points exactly as ratios of integers, avoiding floating-point error entirely. Applied to SSI, this would eliminate the need for IC healing by producing intersection curves that can be evaluated exactly. However, this requires a fundamental representation change from truck's floating-point-based `Curve` enum.

**OCCT — Intersection Edge Representation:**
OCCT stores intersection edges as `Geom_Curve` objects with explicit 3D parametric representations, not as implicit surface-pair references. The intersection computation produces explicit curve geometry at intersection time, never requiring re-computation on evaluation. This is the fundamental architectural difference — truck defers curve representation (lazy evaluation via Newton), while OCCT eagerly computes explicit curves.

### 2.5 Error Accumulation Analysis

Each healing strategy introduces a characteristic error that compounds across chained booleans:

| Strategy | Per-Pass Error | After 5 Booleans | After 10 Booleans | vs. TOLERANCE (1e-6) |
|----------|---------------|-------------------|--------------------|--------------------|
| Exact line | 0 | 0 | 0 | Safe |
| Analytical arc | ~1e-14 | ~5e-14 | ~1e-13 | Safe |
| Clone leader | ~1e-3 | ~5e-3 | ~1e-2 | **Exceeds after 1** |
| Re-approximate | ~5e-7 | ~2.5e-6 | ~5e-6 | **Exceeds after 2** |
| Line fallback | ~1e-1 (variable) | ~5e-1 | ~1 | **Exceeds immediately** |

**Key insight**: Only exact strategies (line, analytical arc) are safe for chained booleans. BSpline-based strategies accumulate error that exceeds TOLERANCE within 2-3 operations. This is why K8 (6 chained booleans) requires the perturbation cascade — the accumulated IC healing error from early operations degrades geometry quality, causing later operations to fail.

### 2.6 Percentage of IC Cases with Known Analytical Solutions

Based on the surface pair types encountered in typical CAD workflows (extrude-based modeling with prismatic and cylindrical features):

| Surface Pair | Frequency (est.) | Analytical Solution? | Currently Exact? |
|-------------|-----------------|---------------------|-----------------|
| Plane-Plane | ~50-60% | Yes — Line | Yes |
| Plane-Cylinder | ~20-30% | Yes — Circle/Ellipse | Partial (circle only) |
| Plane-Cone | ~2-5% | Yes — Conic section | No |
| Cylinder-Cylinder | ~2-5% | Yes — Ellipse | No |
| Plane-Sphere | ~1-3% | Yes — Circle | No |
| Other | ~5-10% | Sometimes | No |

**Estimated 80-95% of IC healing cases have known analytical solutions** that would produce exact (or near-exact) curve representations, eliminating the error accumulation problem entirely for typical CAD workflows.

### 2.7 Spec Violations in IC Healing

**V5: Heal-then-validate** (spec line 43: "must revalidate after healing")
- `heal_intersection_curves` mutates edges in place via `edge.set_curve()` (line 360, 407, 440, 523, 528) but performs **no post-healing validation** of the solid's topology or geometry consistency.
- The `HealingResult` struct tracks counts (healed/failed/types) but not whether the healed solid remains valid for subsequent booleans.
- Strategy 6 (line fallback, line 528) replaces curved edges with straight lines, which can produce self-intersecting faces. No check is performed.

**V6: Structured errors** (spec lines 285-294)
- Strategy 4 (`catch_unwind` at line 499) silences panics from the IC's Newton iteration. The comment justifies this as "best-effort healing" (lines 489-496), but the spec requires structured errors, not silent panic suppression.
- When all strategies fail to meet the tight threshold but a best candidate exists (lines 520-524), the IC is replaced with whatever has the lowest residual, regardless of whether that residual is within tolerance. No error or warning is emitted.

**V7: Local per-edge tolerances** (spec lines 114-127)
- Healed edges do not carry `τ_local` reflecting their approximation quality.
- Strategy 1 (analytical arc) produces machine-precision results that could carry `τ_local ≈ 1e-14`.
- Strategy 3 (re-approximate) has a measured residual that could be assigned as `τ_local` but is discarded.
- All healed edges are treated uniformly in subsequent boolean operations, ignoring their vastly different accuracy levels.

---

## Section 3: Pre-Heal Vertex Unification

### 3.1 What It Does

Before attempting the boolean, `try_boolean_with_perturbation` runs vertex unification on solid_a (lines 1233-1298):

1. Count unique vertex IDs in each shell
2. Call `heal_shell_vertices(shell, tol * 0.2)` to merge vertices within 20% of the boolean tolerance
3. If any vertices were merged, reconstruct the solid via `Solid::try_new`
4. If `Solid::try_new` fails, silently use the original solid

### 3.2 Problems

- **Silent failure**: If `Solid::try_new` fails on healed shells (line 1289-1294), the original un-healed solid is used without any diagnostic. The vertex unification may have partially mutated the shell via interior mutation (`set_point`), leaving it in an inconsistent state.
- **Only applied to solid_a**: Solid_b is never pre-healed, even though it may also carry accumulated vertex drift from prior booleans.
- **Tolerance coupling**: The heal tolerance `tol * 0.2` is derived from the boolean tolerance, not from the solid's actual vertex drift. If the solid has vertices that are exactly `tol * 0.19` apart (just below threshold), they won't be merged, causing the same failure.
- **Non-manifold detection is separate**: `repair_non_manifold_shell` (line 1282-1286) only detects issues without repairing them, and its results are discarded. The detection flag is intentionally not used to trigger `Solid::try_new` (comment at lines 1277-1280).

---

## Section 4: Consolidated Gap Table

| Gap ID | Component | Gap Description | Root Cause | Literature Fix | Severity |
|--------|-----------|----------------|-----------|---------------|----------|
| P3-G1 | Perturbation | Physical geometry modification violates 1 µm feature preservation | No exact predicate/SoS support for degenerate resolution | SoS (Edelsbrunner-Mucke) — virtual perturbation | Critical |
| P3-G2 | Perturbation | Platform-dependent timeout creates non-determinism | Timeout-based cascade with variable execution speed | Eliminate perturbation entirely via correct algorithms | Critical |
| P3-G3 | Perturbation | catch_unwind masks truck panic paths | Missing structured error handling in truck internals | Replace panic paths with Result<> returns | High |
| P3-G4 | Perturbation | Last-error propagation loses original failure diagnostics | Sequential cascade returns last attempt's error | Track and propagate root cause error from direct attempt | Medium |
| P3-G5 | Perturbation | Scale-expand changes tool geometry by up to 5% | Edge coincidence not detected/handled explicitly | OCCT Pave Block splitting for coincident edges | Critical |
| P3-G6 | IC Healing | BSpline approximation error compounds across booleans | Truck's IntersectionCurve re-evaluates via Newton | Eager explicit curve computation at intersection time (OCCT approach) | Critical |
| P3-G7 | IC Healing | Only 2/10 quadric-quadric cases have analytical solutions | Missing analytical SSI formulas | Implement Patrikalakis Ch.5 analytical SSI for all quadric pairs | High |
| P3-G8 | IC Healing | No post-healing validation of solid consistency | heal_intersection_curves has no output validation | Add Solid::try_new + geometric consistency check after healing | High |
| P3-G9 | IC Healing | catch_unwind silences Newton divergence panics | IC's subs(t) can panic when Newton fails | Replace IC evaluation with non-panicking Result<> API | Medium |
| P3-G10 | IC Healing | Healed edges don't carry τ_local | No per-edge tolerance tracking | Attach healing residual as τ_local per spec requirement | Medium |
| P3-G11 | IC Healing | Line fallback (strategy 6) loses curvature information | No better fallback available | Implement analytical solutions so fallback is never needed | High |
| P3-G12 | IC Healing | Plane-cylinder healing misses oblique (ellipse) case | analytical_circle_arc_from_leader only fits circles | Implement ellipse fitting for oblique plane-cylinder intersection | Medium |
| P3-G13 | Pre-heal | Vertex unification applied only to solid_a, not solid_b | Arbitrary choice in implementation | Apply to both operands | Low |
| P3-G14 | Pre-heal | Silent fallback to original solid on Solid::try_new failure | Error discarded with .ok() | Propagate as structured warning/error | Medium |
| P3-G15 | Pre-heal | Non-manifold detection is detection-only, no repair | repair_non_manifold_shell doesn't modify topology | Implement actual non-manifold repair or reject with error | Medium |
| P3-G16 | Perturbation | Cardinal perturbation uses fixed 1e-5 regardless of geometry scale | Not derived from bounding box extent | Either scale-aware or eliminate via correct algorithms | Low |
| P3-G17 | Perturbation | WASM builds skip timeout, native builds have 120s timeout | Platform-conditional compilation | Same algorithm on all platforms | Low |

---

## Section 5: Key Findings

### 5.1 The Perturbation Cascade Is a Symptom, Not a Solution

Every perturbation strategy in the cascade exists because the core boolean pipeline lacks one or more of:
1. **Exact predicates with SoS** for orient3d/incircle/insphere (handles D1, D3, D4)
2. **Explicit edge/vertex coincidence detection** (handles D2)
3. **Analytical SSI for quadric surfaces** (handles IC healing entirely)
4. **Topology-oriented algorithm structure** (prevents geometry failures from cascading to topology failures)

The literature provides solutions for ALL of these. Modern boolean implementations (CGAL, Blender, OCCT) do not use physical perturbation cascades.

### 5.2 IC Healing Is a Consequence of Architecture, Not Inherent Requirement

The need for IC healing is a direct consequence of truck's `IntersectionCurve` design, which defers curve representation to evaluation time. If intersection curves were computed eagerly as explicit `Line`, `NurbsCurve`, or `BSplineCurve` objects at intersection time (as OCCT does), there would be no IC edges to heal.

**Estimated impact of implementing analytical SSI for all quadric cases**: 80-95% of IC healing cases would produce exact curves with zero error, eliminating the error accumulation problem for typical prismatic/cylindrical CAD workflows.

### 5.3 Error Compounds Across Chained Booleans

The most severe consequence of BSpline-based IC healing is error accumulation:
- Each BSpline approximation introduces ~5e-7 error
- After N chained booleans: cumulative error ≈ N × 5e-7
- Exceeds TOLERANCE (1e-6) after ~2 operations
- Exceeds 1 µm feature preservation after ~2 operations

This is why complex workflows like K8 (6 chained booleans) require the perturbation cascade — earlier operations' IC healing errors degrade geometry, causing later operations to fail at intersection construction.

### 5.4 Quantified Spec Compliance

| Spec Requirement | Compliance | Violation Detail |
|-----------------|-----------|-----------------|
| No panics on valid inputs | **Partial** | catch_unwind masks panics instead of eliminating them |
| Deterministic results | **Partial** | Deterministic given fixed CPU speed; timeout creates platform variance |
| 1 µm feature preservation | **Violated** | Scale-expand and large-final perturb by 10-50 µm |
| Structured errors | **Partial** | Result<> type is correct; content loses diagnostic fidelity |
| Heal-then-validate | **Violated** | No post-healing validation |
| Local per-edge tolerances | **Violated** | Healed edges carry no τ_local |
| Layered tolerance model | **Partial** | Pre-heal uses tol*0.2; IC healing uses hardcoded TOLERANCE |

---

## Section 6: Recommended Improvements (Ordered by Impact)

### 6.1 Short-term (Can reduce perturbation cascade usage by ~50%)

1. **Implement analytical SSI for plane-cone and cylinder-cylinder** — Patrikalakis Ch.5 provides the formulas. This eliminates BSpline fallback for ~10% of IC cases and removes error accumulation for those surface pairs.
2. **Implement oblique plane-cylinder ellipse fitting** — Extends `analytical_circle_arc_from_leader` to handle non-axis-aligned plane-cylinder intersections as NURBS ellipses.
3. **Add post-healing validation** — Run `Solid::try_new` and `is_geometric_consistent` after `heal_intersection_curves`. Reject healed solids that fail validation.
4. **Attach τ_local to healed edges** — Track the healing residual as per-edge metadata for downstream tolerance-aware decisions.

### 6.2 Medium-term (Can eliminate perturbation cascade for simple geometry)

5. **Implement full SoS for orient3d** — Extends the existing `sos_orient2d_tiebreak` pattern to 3D. Eliminates the root cause of coplanar-face and vertex-on-face degeneracies.
6. **Implement explicit edge-edge coincidence detection** — Detect and split coincident edges before the boolean, replacing scale-expand strategies.
7. **Eagerly compute IC curves at intersection time** — Replace truck's lazy IntersectionCurve with eager explicit curve computation for all analytically-solvable surface pairs.

### 6.3 Long-term (Eliminates perturbation cascade entirely)

8. **Implement winding number classification** — Replaces ray-cast voting with provably correct inside/outside determination (Zhou 2016).
9. **Adopt topology-oriented algorithm structure** — Restructure the pipeline so topological decisions are made first, with numerics only choosing branches (Sugihara-Iri).
10. **Replace perturbation cascade with SoS virtual perturbation** — Edelsbrunner-Mucke's approach provides provable degeneracy resolution without physical geometry modification.
