# Phase 2: Pipeline Stage Mapping — Classification + Shell Assembly + Robustness Infrastructure

## Overview

This document compares the current face classification, shell assembly, and robustness infrastructure stages of the Waffle Iron boolean pipeline against the literature and the production spec (`specs/SHAPEOPS-BOOLEAN-SPEC.md`). No code changes are made — this is a pure analysis deliverable.

**Files under review:**
- `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` (~3117 lines)
- `vendor/truck/truck-shapeops/src/transversal/robust_classify.rs` (~741 lines)
- `vendor/truck/truck-shapeops/src/transversal/coplanar.rs` (~488 lines)
- `vendor/truck/truck-shapeops/src/transversal/coplanar_overlay.rs` (~932 lines)
- `vendor/truck/truck-base/src/id.rs` (DetId/DetContext)

**References consulted:**
- Zhou 2016 (mesh arrangements, winding number classification, BFS propagation)
- Cherchi 2020 (indirect predicates for classification)
- Edelsbrunner-Mucke (Simulation of Simplicity)
- Hachenberger (Nef polyhedra — correct by construction)
- OCCT General Fuse Algorithm (classification and Building Part stages)
- Barki 2015 (regularized output)
- Shewchuk (adaptive precision predicates)
- Levy 2025 (exact constructions, arithmetic filters)
- Devillers-Preparata (filter failure bounds)

---

## Section 1: Classification Stage (`classify_faces`)

### 1.1 Classification Algorithm

The pipeline is implemented in `classify_one_pair_of_shells_result_with_tol` (`mod.rs:629-870`):

1. **Triangulation** (lines 643-644): Both shells triangulated at `tau_mesh`. BVH R-trees built for O(log n) ray-cast acceleration.
2. **Loops Store Creation** (lines 652-667): Intersection curves computed, yielding intersection loops and coplanar face pairs.
3. **Face Division** (lines 704-721): Each shell's faces split along ICs producing `FacesClassification` with And/Or/Unknown status.
4. **Integrate By Component** (`faces_classification/mod.rs:43-92`): Groups `Unknown` faces into connected components, classifies by boundary edge counting (And vs. Or neighbor edges). Ties remain `Unknown`.
5. **Reset Overlapping Coplanar** (lines 724-725): Coplanar face fragments reset to `Unknown`.
6. **3-Tier Classification of Unknown Faces** (lines 737-770, 786-818):
   - **Tier 1 — Coplanar Overlay**: `classify_coplanar_via_overlay()` → `classify_coplanar_fragment()` fallback
   - **Tier 2 — Ray-Cast**: `ray_cast_classify()` with 8 irrational-direction rays, majority voting
   - **Tier 3 — Edge-Neighbor Propagation**: `classify_by_edge_neighbors()` — iterative, max 10 rounds

### 1.2 Ray-Cast Analysis

**8-ray majority-vote** (`mod.rs:333-476`): The `majority_vote` closure (lines 364-386) casts all 8 rays. Parity: `c.unsigned_abs() % 2 == 1` → inside (odd crossings), `== 0` → outside (even). Requires ≥3 of 8 agreement.

**The 8 irrational ray directions** (`mod.rs:277-299`): Use square roots of distinct primes as components. Algebraically irrational, unlikely to align with grid-structured triangulation.

**Is majority-vote provably correct?** **No.** It can fail when:
- Test point very close to boundary (within ULP of surface)
- Non-manifold or self-intersecting triangulation
- Thin features comparable to perturbation magnitude (~1e-6 × scale)
- All 8 rays graze edges/vertices (algebraically unlikely but not impossible)

**Fallback cascade** (`mod.rs:333-476`):
1. Centroid bidirectional perturbation (±perturb with `sqrt(2,3,5) * 1e-6 * scale`)
2. Each vertex bidirectional perturbation
3. Centroid escalated perturbation (1000× larger: `1e-3 * scale`)
4. Each vertex escalated perturbation
5. Face-normal ray (single ray along geometric face normal)
6. Last-resort: any single non-`None` ray from perturbed centroid

### 1.3 Edge-Neighbor Propagation (`mod.rs:484-566`)

Iteratively classifies unresolved faces by counting shared edges with already-classified faces:
- `and_adj > or_adj` → And
- `or_adj > and_adj` → Or
- Tied with any adjacency → And (biased tiebreak)
- Max 10 rounds. Remaining unresolved → `BooleanStageError::Classification`

### 1.4 Comparison to Literature

**Zhou winding numbers (Eq. 7) vs. our edge-neighbor propagation:**

| Aspect | Zhou (Eq. 7) | Our Implementation |
|---|---|---|
| Domain | Mesh arrangement cells | B-rep face fragments |
| Propagation | BFS on cell-patch graph | Iterative edge-neighbor voting (max 10 rounds) |
| Rule | w_n = w_c ± [0,...,1,...,0] (exact) | Majority voting of adjacent And/Or counts |
| Guarantee | Exact for valid PWN mesh | Heuristic — can converge to wrong answer |
| Handles ties | Always exactly determined | Biased → And |

**Not equivalent.** Zhou's is provably correct on a complete space partition; ours is a local heuristic fallback.

**Cherchi 2020 indirect predicates:** We do not use indirect predicates. All intersection points are explicit floating-point coordinates. Cherchi's approach would eliminate the root cause of many classification errors by deferring coordinate materialization.

**Edelsbrunner-Mucke SoS completeness:**

| E-M Requirement | Our Status |
|---|---|
| `orient2d` SoS | Partial — `sos_orient2d_tiebreak` (parity-based) |
| `orient3d` SoS | **Missing** — returns `None` on coplanar |
| `insphere` SoS | **Not implemented** |
| Consistent perturbation scheme | **Not implemented** |
| Vertex-on-vertex case | Returns `None` |

The incomplete SoS is the root reason for bidirectional perturbation, escalated perturbation, and face-normal ray fallback — all workarounds for cases that complete SoS would handle directly.

**OCCT classification:** OCCT uses operation-independent IN/OUT/ON states per face (FaceInfo). Our And/Or naming entangles classification with operation semantics (And = inside, Or = outside, but named after union/intersection operations).

### 1.5 Spec Gap Analysis (Classification)

| Gap | Severity | Spec Reference |
|---|---|---|
| `ShapesOpStatus` uses And/Or/Unknown instead of Inside/Outside/OnBoundary/Unknown | Medium | "Core representation: replace AND/OR tagging" |
| No SoS for `orient3d` (returns None on coplanar, triggers perturbation cascade) | High | E-M reference |
| No SoS for vertex case (2+ orient2d zero, returns None) | High | E-M reference |
| Ray-triangle intersection point computed in floating-point, then tested with exact orient2d | Medium | "Robust predicates for load-bearing decisions" |
| No winding number classification | Medium (long-term) | Zhou 2016 |
| Edge-neighbor propagation is a heuristic, not provably correct | Medium | Correctness |
| Tied→And tiebreak in edge-neighbor propagation is operation-biased | Low | Correctness |
| `BooleanStageError::Classification` lacks diagnostic detail | Low | "Structured error" requirements |

---

## Section 2: Shell Assembly Stage (`finalize_boolean_shell`)

### 2.1 Assembly Algorithm

`finalize_boolean_shell` (`mod.rs:2769-2931`) — 6-level recovery cascade:

| Level | Strategy | Tolerance | Key Function |
|---|---|---|---|
| 1 | Initial weld | `τ_model` | `weld_coincident_edges` → `Solid::try_new` |
| 2 | Wider weld | `τ_model × 2.0`, `× 5.0` | Same with wider weld_tol |
| 3 | Targeted reweld | `τ_model × 10.0` | `targeted_open_edge_reweld` — match by vertex position + midpoint |
| 4 | Position-based reweld | `τ_model × 2.0/5.0/10.0` | `position_based_edge_reweld` — quantize geometry to grid |
| 5 | Split edge propagation | `τ_model`, then `× 5.0` | `split_open_edges_at_interior_vertices` → reweld |
| 6 | Accept singular vertices | Shell closure check only | `Solid::new_unchecked` — bypasses manifoldness |

### 2.2 `weld_coincident_edges` (`mod.rs:1340-1786`)

Three phases:

**Phase 0 — Position-based vertex unification** (lines 1348-1485): Spatial grid (`FxHashMap<(i64,i64,i64), Vec<Vertex>>`), 27-cell neighborhood search, `unify_tol = weld_tol.unwrap_or((tol * 0.2).max(TOLERANCE.sqrt()))`. Rebuilds faces with unified vertices.

**Phase 1 — Edge canonicalization** (lines 1487-1629): Assigns `DetId` via spatial ordering, groups edges by `(DetId, DetId)` in `BTreeMap`, matches by 3-point curve agreement (t=0.25, 0.5, 0.75) within `3 × tau_model`. Same-face edges explicitly skipped.

**Phase 2 — Face rebuilding** (lines 1631-1688): Replaces non-canonical edges with canonical partners. T-junction protection via `used_canonicals` per wire.

**Phase 3 — Over-counted edge fixing** (lines 1690-1785): Clones edges with 3+ face references to restore 2-reference invariant.

### 2.3 Welding Tolerance Analysis

| Context | Tolerance | Source |
|---|---|---|
| Phase 0 vertex unification (default) | `(tol × 0.2).max(TOLERANCE.sqrt())` | `mod.rs:1351` |
| Phase 1 edge curve matching | `tol × 3.0` | `mod.rs:1614` |
| Level 2 wider weld | `τ_model × 2.0`, `× 5.0` | `mod.rs:2794` |
| Level 3 targeted reweld | `τ_model × 10.0` | `mod.rs:2838` |
| Level 4 position reweld | `τ_model × 2.0/5.0/10.0` | `mod.rs:2851-2854` |

**`τ_weld` defined but unused**: `BooleanTolerance::from_model_tol` defines `tau_weld = 0.4 × tau_model` (`mod.rs:137`), but `finalize_boolean_shell` ignores it and passes `tols.tau_model` with hardcoded multipliers.

**Over-welding risk**: Levels 3-4 use up to 10× model tolerance with no feature-size guard. Gap-fill repair has a hard cutoff at 5.0 units (hardcoded, scale-independent). Documentation notes that "2× multiplier was too aggressive and merged vertices across small features."

### 2.4 Comparison to Literature

**Hachenberger Nef polyhedra (correct by construction):**
- Nef polyhedra require no stitching/welding — results synthesized from sphere map overlays using exact arithmetic
- Our 6-level cascade exists because we use approximate arithmetic on a manifold B-rep representation
- **Adoptable principles**: vertex-local validation, Plucker coordinate edge sorting, non-manifold as first-class

**OCCT Building Part:**

| Aspect | OCCT | Ours |
|---|---|---|
| Edge identity | Split from pave blocks with common blocks — maintained through interference DS | Independently created, then matched by geometric similarity |
| Same-domain | Explicit connexity chains | No concept |
| Recovery levels | None — succeeds or reports errors | 6-level escalating cascade |
| Tolerance correction | Post-treatment corrects vertex/edge tolerances | No post-treatment |
| Fuzzy mode | Explicit `SetFuzzyValue()` option | Implicit escalation through wider weld tolerances |
| History | `Generated()`, `Modified()`, `IsDeleted()` tracking | No history tracking |

**OCCT does NOT have multi-level recovery like ours.** OCCT's approach is designed to produce correct results from correct interference data, or fail with structured errors. Our cascade compensates for upstream inaccuracies.

**Barki regularized output:** Barki achieves regularization inherently through orientability-based classification. Our output is **NOT guaranteed regularized** — no explicit regularization step, `Face::new_unchecked` can introduce non-simple wires with dangling sub-structures.

### 2.5 Topological Invariant Maintenance

| Invariant | Level 1-5 | Level 6 |
|---|---|---|
| Shell closure | Maintained (via `Solid::try_new`) | Maintained (explicit check) |
| Manifoldness | Maintained | **VIOLATED** (`Solid::new_unchecked`) |
| Wire simplicity | May be violated (Phase 0 `new_unchecked`) | May be violated |

### 2.6 Spec Gap Analysis (Assembly)

| Gap | Severity | Spec Reference |
|---|---|---|
| `τ_weld` defined but unused in assembly; uses `τ_model` + hardcoded multipliers | Medium | "Stitching MUST consume τ_weld and τ_local" |
| No `τ_local` per-edge tolerance | High | "Local per-edge/per-feature tolerances" |
| No `TouchingPolicy` (ErrorNonManifold/KeepSeparateComponents/FuzzyMerge) | Medium | Spec "TouchingPolicy" |
| `Solid::new_unchecked` at Level 6 bypasses validation | Medium | Spec: "MUST use Solid::try_new" |
| No regularization step | Medium | Spec: "Default booleans MUST be regularized solids" |
| No explicit non-manifold escape hatch API | Low | Spec: "non-manifold boundary output escape hatch" |
| Error detail is string-based (`ShellAssembly(String)`) | Medium | Spec: structured errors |
| Diagnostics stripped in release (`#[cfg(debug_assertions)]` only) | High | Spec: "failures are diagnosable" |
| Phase 0 vertex unification uses `FxHashMap` (non-deterministic) | Low | Spec: determinism |
| Over-welding risk at higher levels with no feature-size guard | Medium | Spec: "preserve ≥ 1 µm features" |
| No recovery-level reporting (caller can't distinguish Level 1 clean vs. Level 6 degraded) | Medium | Spec: diagnosable results |

---

## Section 3: Robustness Infrastructure

### 3.1 Robust Predicate Inventory

| Predicate | Location | Arithmetic | Status |
|---|---|---|---|
| `robust_orient3d` | `robust_classify.rs:9-32` | Exact (Shewchuk adaptive) | Fully used |
| `robust_orient2d` | `robust_classify.rs:39-45` | Exact (Shewchuk adaptive) | Fully used |
| `sos_orient2d_tiebreak` | `robust_classify.rs:58-86` | Exact sign (permutation parity) | Used for single-edge ray-triangle hits |
| `robust_ray_triangle_cross` | `robust_classify.rs:105-227` | Mixed (exact orient + float intersection) | Core ray-cast |
| `exact_points_coplanar` | `robust_classify.rs:291-299` | Exact (orient3d == 0.0) | Coplanarity |
| `signed_plane_distance` | `robust_classify.rs:309-336` | Mixed (exact numerator, float normalization) | Coplanar deviation |
| `max_coplanar_deviation` | `robust_classify.rs:345-378` | Mixed | Coplanar tolerance check |

**Missing predicates from literature:**
- `incircle`/`insphere` (Shewchuk) — needed for CDT
- `orient3d` with full SoS (E-M 15 cofactor terms)
- Indirect predicates (Cherchi L-type, T-type)
- Radial sort predicates (Levy)

**Still floating-point:**
- Normal computation in `robust_ray_triangle_cross` (lines 120-138)
- Intersection parameter `t = n_dot_diff / n_dot_dir` (line 149)
- `n_dot_dir == 0.0` parallelism test (line 138)
- `find_non_collinear_triple` uses `1e-30` heuristic floor (lines 252, 275)
- All coplanar/normal computations use `1e-30` floors

### 3.2 SoS Assessment

**Partial, ad-hoc SoS** — NOT full Edelsbrunner-Mucke:
- `sos_orient2d_tiebreak` uses permutation parity, approximating the depth-4 cofactor for D=2
- No SoS for orient3d (orient3d returning 0.0 → `None` → perturbation cascade)
- No global point indexing
- No cofactor chain evaluation (SignDet_Delta)
- Not applied consistently to all predicates

**The incomplete SoS is the architectural root cause of the perturbation cascade.** Complete SoS would eliminate `None` returns from degenerate predicates, removing the need for the multi-strategy retry in `ray_cast_classify` and ultimately the multi-strategy cascade in `healing.rs`.

### 3.3 Arithmetic Filter Analysis

**No semi-static or interval arithmetic filters implemented.** The `robust` crate internally uses Shewchuk's adaptive precision (a two-level filter), but this only covers `orient2d` and `orient3d`. All other computations are raw floating-point.

**Opportunities identified:**
1. Normal computation — Cherchi's semi-static filter formula: `ε_n = 8.88395e-16 × δ²`
2. `n_dot_dir` parallelism — filter-bounded error on dot product
3. Coplanar distance — filter could certify sign without normalization
4. Boundary midpoint tests — exact predicate + filter more reliable than tolerance comparison

### 3.4 `BooleanTolerance` Architecture

| Spec Layer | `BooleanTolerance` Field | Coverage |
|---|---|---|
| `τ_model` | `tau_model` | Covered |
| `τ_work` (numeric floor) | **MISSING** | Not in `BooleanTolerance` (exists in `BooleanOptions` at kernel-fork level) |
| `τ_mesh` | `tau_mesh` | Covered |
| `τ_weld` | `tau_weld` | Covered (ratio mismatch: 0.4× vs. spec's 2×) |
| `τ_coplanar` | `tau_coplanar` | Covered |
| `τ_local` (per-edge) | **MISSING** | Not implemented anywhere |

**Pipeline bypass points:**
- `polyline_construction/mod.rs` uses `TOLERANCE` directly (the most critical bypass)
- `loops_store/mod.rs` passes individual `f64` values, not the struct
- `divide_face/mod.rs` does not accept `BooleanTolerance` at all

**`BooleanOptions` → `BooleanTolerance` conversion collapses differentiation:** `BooleanTolerance::uniform(compute_adaptive_tol(...))` sets all fields to the same value, defeating the layering.

### 3.5 Determinism Infrastructure

**`DetId`/`DetContext`:** Thread-local monotonic counter scoped per boolean operation. `assign_vertex_det_ids` sorts vertices lexicographically by position for deterministic ID assignment.

**Usage pattern:** `BTreeMap` in `integrate/mod.rs` (39 occurrences), `FxHashMap` elsewhere (42 occurrences, each annotated "order-insensitive").

**Remaining non-determinism sources:**
1. `polyline_construction/mod.rs` — `FxHashMap` for graph walk, multi-component order non-deterministic
2. `BTreeMap<EdgeID<C>, _>` — keys are pointer-based `ID<T>`, non-deterministic across native runs
3. Phase 0 of `weld_coincident_edges` — `FxHashMap` for spatial grid
4. `AtomicU64::Relaxed` in `DetContext` — fragile under future parallelization

**WASM target:** On `wasm32-unknown-unknown`, no ASLR means pointer-based IDs are deterministic within execution. But they differ between WASM and native targets.

### 3.6 Spec Gap Analysis (Robustness)

**Load-bearing decisions coverage:**

| Decision | Robust? | Status |
|---|---|---|
| Ray-triangle crossing (orient3d side test) | Yes | COVERED |
| Ray-triangle 2D containment (orient2d) | Yes | COVERED |
| Coplanarity check (distance sign) | Partial | Exact numerator, float normalization |
| Point-in-polygon for coplanar overlap | Yes | Uses `robust_orient2d` |
| Ray-plane intersection point | No | `robust_classify.rs:143-161` |
| Winding direction (dot product) | No | `mod.rs:235` |
| Surface normal evaluation | No | `coplanar.rs:186` |
| And/Or determination from normal cross product | No | `loops_store/mod.rs:61-77` |

---

## Consolidated Gap Summary

### Critical Gaps (production correctness)

| ID | Stage | Gap | Severity |
|---|---|---|---|
| P2-G1 | Predicates | No SoS for `orient3d` — root cause of perturbation cascade | Critical |
| P2-G2 | Predicates | Ray-triangle intersection point in floating-point, then tested with exact orient2d | High |
| P2-G3 | Tolerance | `τ_local` not tracked per intersection edge — fundamental spec violation | High |
| P2-G4 | Tolerance | `polyline_construction` bypasses `BooleanTolerance`, uses global `TOLERANCE` | High |
| P2-G5 | Assembly | Diagnostics stripped in release builds (`#[cfg(debug_assertions)]` only) | High |
| P2-G6 | Assembly | No `τ_weld` usage in assembly (uses `τ_model` + hardcoded multipliers) | Medium |
| P2-G7 | Filters | No semi-static arithmetic filters on any custom computation | High |

### Medium Gaps (functional but non-compliant)

| ID | Stage | Gap | Severity |
|---|---|---|---|
| P2-G8 | Classification | `ShapesOpStatus` And/Or/Unknown vs. spec's Inside/Outside/OnBoundary/Unknown | Medium |
| P2-G9 | Classification | Edge-neighbor propagation is a heuristic, not provably correct | Medium |
| P2-G10 | Assembly | No `TouchingPolicy` | Medium |
| P2-G11 | Assembly | `Solid::new_unchecked` at Level 6 bypasses validation | Medium |
| P2-G12 | Assembly | No regularization step | Medium |
| P2-G13 | Assembly | Error detail is string-based, not structured | Medium |
| P2-G14 | Assembly | No recovery-level reporting | Medium |
| P2-G15 | Assembly | Over-welding risk at higher levels with no feature-size guard | Medium |
| P2-G16 | Tolerance | `τ_work` missing from `BooleanTolerance` | Medium |
| P2-G17 | Tolerance | `BooleanOptions` → `BooleanTolerance` collapses via `uniform()` | Medium |
| P2-G18 | Determinism | `BTreeMap<EdgeID<C>>` keys are pointer-based (non-deterministic on native) | Medium |

### Low/Aspirational Gaps

| ID | Stage | Gap | Severity |
|---|---|---|---|
| P2-G19 | Classification | No winding number classification (Zhou) | Low (long-term) |
| P2-G20 | Classification | No indirect predicates (Cherchi) | Low (long-term) |
| P2-G21 | Assembly | No non-manifold escape hatch API | Low |
| P2-G22 | Predicates | `incircle`/`insphere` not implemented | Low |
| P2-G23 | Determinism | `FxHashMap` in polyline_construction — multi-component walk order | Low |
| P2-G24 | Determinism | `AtomicU64::Relaxed` fragile under parallelization | Low |
| P2-G25 | Predicates | `sos_orient2d_tiebreak` uses permutation parity, not full E-M SignDet | Medium |
