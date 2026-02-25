# Phase 4: Governance Compliance, Spec Gaps, Test Coverage, and Improvement Roadmap

## Overview

This document synthesizes findings from all four review phases into governance compliance matrices, spec gap assessments, test coverage analysis, and a prioritized improvement roadmap. No code changes are made — this is the final analysis deliverable.

**Inputs:**
- Phase 1: `docs/review/phase1-intersection-division.md` — 25 gaps (P1-G1 through P1-G25)
- Phase 2: `docs/review/phase2-classification-assembly.md` — 25 gaps (P2-G1 through P2-G25)
- Phase 3: `docs/review/phase3-healing-perturbation.md` — 17 gaps (P3-G1 through P3-G17)
- Governance: `governance/ENGINEERING_CONSTITUTION.md`, `governance/ARCHITECTURAL_INVARIANTS.md`, `governance/DEFINITION_OF_DONE.md`
- Production spec: `docs/SHAPEOPS-BOOLEAN-SPEC.md` (342 lines)
- Existing specs: 8 files in `specs/boolean_*.md`

---

## Section 1: Governance Compliance Matrix

### 1.1 Engineering Constitution (P1-P7)

| Principle | State | Evidence | Key Gap | Priority |
|-----------|-------|----------|---------|----------|
| **P1**: Correctness is measurable | **Partial** | Volume oracles in extrude_chains, shell closure in boolean_shell_closure, Euler validation (diagnostic only), face count assertions | Euler check is debug-only, not enforced as oracle. No watertightness oracle. Level 6 `Solid::new_unchecked` bypasses manifoldness validation. No centroid oracles. | High |
| **P2**: Specs precede implementation | **Partial** | 8 spec files exist covering tolerance layering, error types, shell closure, determinism, difference, K8, multi-cut, predicates | 8 major components lack specs: perturbation cascade, shell assembly (finalize_boolean_shell), IC healing, edge-neighbor propagation, weld_coincident_edges, coplanar overlay, pre-heal vertex unification, wire splitting | **Critical** |
| **P3**: Tests fail before fix | **Unverifiable** | Cannot verify retroactively. `multi_cut_regression.md` shows correct test-first pattern. | Several specs show status "Draft" suggesting concurrent or post-implementation writing. | Medium |
| **P4**: Branches must be tested | **Non-compliant** | 154+ tests in truck-shapeops, 300+ in test-harness | Perturbation strategies 5/7/10/11 may never be exercised. Assembly levels 4-5 untested. IC healing strategies 2/5/6 lack dedicated tests. 10+ `new_unchecked` fallback paths untested. Edge-neighbor propagation tie-breaking untested. | **Critical** |
| **P5**: No self-approval | N/A | Process control | — | — |
| **P6**: Architecture not eroded | **Partial** | `BooleanStageError` correctly `pub(crate)`, `BooleanError` in kernel-fork, truck types don't leak to engine/bridge | `TOLERANCE` constant duplicated in `healing.rs` and `polyline_construction`. `BooleanStageError` exported publicly from truck-shapeops despite spec saying MUST NOT (architecture violation). | Medium |
| **P7**: Small auditable changes | N/A | Process control | — | — |

### 1.2 Architectural Invariants

| Invariant | State | Evidence | Key Gap | Priority |
|-----------|-------|----------|---------|----------|
| **A3.3**: Single ownership of tolerance policy | **Non-compliant** | `BooleanTolerance` (7 fields) and `BooleanOptions` (6 fields) define DIFFERENT tolerances with DIFFERENT defaults | 10+ ad-hoc tolerance derivation formulas scattered across 6+ files. `finalize_boolean_shell` ignores `tau_weld`, uses `tau_model` with hardcoded multipliers. `polyline_construction` bypasses `BooleanTolerance` entirely. | **Critical** |
| **A4.2**: Deterministic rebuild | **Partial** | `DetId`/`DetContext`, `BTreeMap` (52 locations), polyline canonicalization, determinism tests | 120s timeout creates platform-dependent non-determinism. WASM skips timeout entirely. `FxHashMap` in polyline_construction and weld Phase 0. `BTreeMap<EdgeID>` with pointer-based keys. | High |
| **A8.1**: Central tolerance policy | **Non-compliant** | Same as A3.3 | `BooleanTolerance::uniform()` collapses all fields to one value, defeating layered design. `BooleanOptions::for_scale()`/`default()` never called in production — `compute_adaptive_tol()` → `for_boolean_tol()` bypasses spec-compliant defaults. | **Critical** |
| **A8.2**: No silent healing without diagnostics | **Non-compliant** | `HealingResult` struct, `#[cfg(debug_assertions)]` logging | ALL diagnostics stripped in release builds (10 blocks in healing.rs, 29 in integrate/mod.rs). `catch_unwind` silences panics. Pre-heal failure silently falls back. `new_unchecked` produces no diagnostics. `finalize_boolean_shell` recovery level not reported to caller. | **Critical** |

### 1.3 Definition of Done Compliance

| DoD Category | Status | Key Gaps |
|-------------|--------|----------|
| **1.1 Specification** | Partial | 8 unspecified components. Branch tables incomplete for recovery pipelines. `tau_weld` spec says 2x but implementation uses 0.4x. |
| **1.2 Test Requirements** | Non-compliant | Multiple recovery branches untested. No centroid/analytical volume oracles. Missing near-zero-area and extreme-aspect-ratio tests. |
| **1.3 Invariant Validation** | Partial | Shell closure validated. Euler only as debug diagnostic. Timeout-based non-determinism. |
| **1.4 Implementation Integrity** | Non-compliant | Tolerance not normalized early. Redundant downstream multipliers. `thread_local! DET_CONTEXT` is global mutable state. |
| **1.5 Adversarial Validation** | Partial | K8 is a good adversarial case. Missing tangential intersection, vertex-on-edge, NaN guards. `new_unchecked` explicitly produces potentially invalid topology. |

---

## Section 2: Production Spec Compliance Matrix

### 2.1 Pipeline Stages (8 required)

| # | Stage | Status | Notes |
|---|-------|--------|-------|
| 1 | Preprocess | **Partial** | Minimal input validation (empty boundary/face count only). No strict-mode topology checks. Best-effort healing without revalidation. |
| 2 | Broadphase | **Implemented** | AABB culling with `tol * 2.0` inflation. O(n*m) scan, no R-tree. |
| 3 | Intersection construction | **Implemented** | Tessellation-first (FF interference only). 1/6 OCCT interference types. No adaptive refinement, no `tau_local`, hardcoded `TOLERANCE`. |
| 4 | Corefinement (imprinting) | **Partial** | Interleaved with intersection construction. Spec requires separate stage. 5-level ad-hoc recovery pipeline. |
| 5 | Classification | **Implemented** | 3-tier: coplanar overlay → 8-ray robust ray-cast → edge-neighbor propagation. Uses And/Or/Unknown (not spec's Inside/Outside/OnBoundary/Unknown). |
| 6 | Selection | **Implemented** | Clean 4-bucket design. Missing XOR. |
| 7 | Stitching | **Implemented** | 6-level recovery cascade. `tau_weld` defined but unused. Level 6 uses `Solid::new_unchecked`. |
| 8 | Postprocess & validate | **Partial** | Euler check exists but diagnostic-only. No sliver handling, no coplanar merging, no geometric consistency check. |

### 2.2 Tolerance Layers (6 required)

| # | Layer | Status | Notes |
|---|-------|--------|-------|
| 1 | `tau_model` | **Defined and used** | Default 1e-7 in `BooleanOptions`. But production path via `compute_adaptive_tol()` sets it to 1e-6 to 0.05 (orders of magnitude larger). |
| 2 | `tau_work` | **Defined, unused** | Value 1e-12 in `BooleanOptions`. Never propagated to truck-shapeops. All computation uses hardcoded `TOLERANCE = 1e-6`. |
| 3 | `tau_mesh` | **Partially used** | Passed to `triangulation()`. But `construct_polylines()` bypasses it, using hardcoded `TOLERANCE`. |
| 4 | `tau_weld` | **Partially used** | Defined in `BooleanTolerance` but `finalize_boolean_shell` ignores it — uses `tau_model` with hardcoded multipliers (2x, 5x, 10x). |
| 5 | `tau_coplanar` | **Defined and used** | Functional. Set equal to `tau_model`. |
| 6 | `tau_local` | **Not implemented** | Zero occurrences in codebase. Fundamental spec violation. |

### 2.3 Operations (4 required)

| # | Operation | Status |
|---|-----------|--------|
| 1 | Union | **Implemented** |
| 2 | Intersection | **Implemented** |
| 3 | Difference | **Implemented** (proper 4-bucket selection, not `not()` + `and()`) |
| 4 | XOR | **Not implemented** |

### 2.4 Structured Errors (6 categories)

| # | Category | Status | Notes |
|---|----------|--------|-------|
| 1 | Invalid input topology | **Implemented** | Only checks empty boundaries/faces. Doesn't wrap Truck topology errors. |
| 2 | Tolerance configuration | **Implemented** | `validate()` exists but never called in production path. |
| 3 | Intersection construction | **Implemented** | Generic string detail, not specific failure cause. |
| 4 | Classification ambiguity | **Implemented** | Generic string, no information about which faces. |
| 5 | Stitching/assembly | **Implemented** | Carries actual topology error string (best of the 6). |
| 6 | Postprocess/healing | **Partial** | Type exists (`InvalidResult`) but never constructed. |

### 2.5 Other Requirements

| Requirement | Status | Notes |
|------------|--------|-------|
| `RelationToOther` (Inside/Outside/OnBoundary/Unknown) | **Not implemented** | Still uses And/Or/Unknown (operation-entangled). |
| `TouchingPolicy` | **Not implemented** | No equivalent concept. |
| `Solid::try_new` compliance | **Partial** | Used at 9 sites. Level 6 uses `new_unchecked` (spec prohibition). |
| Determinism | **Partial** | Within a single platform. Timeout creates cross-platform variance. |
| 1 µm feature preservation | **Violated** | `compute_adaptive_tol` floor is 1e-6. Perturbation by 10-50 µm. |
| Local per-edge tolerances | **Not implemented** | Zero occurrences of `tau_local`. |
| Non-manifold escape hatch | **Partial** | Level 6 is implicit, not an explicit API. |
| Regularized output | **Not implemented** | No removal of dangling faces/edges/vertices. |
| Property tests | **Partial** | 4 of 9 algebraic identities tested. Missing intersection idempotence, De Morgan, associativity, absorption. |
| Fuzz tests | **Not implemented** | No fuzz harness exists. |
| Benchmarks | **Not implemented** | No `criterion` benchmarks. |

### 2.6 Production Spec Compliance Summary

| Category | Implemented | Partial | Not Implemented |
|----------|------------|---------|-----------------|
| Pipeline Stages (8) | 3 | 4 | 1 |
| Tolerance Layers (6) | 2 | 2 | 2 |
| Operations (4) | 3 | 0 | 1 |
| Structured Errors (6) | 4 | 1 | 1 |
| Other Requirements (11) | 1 | 4 | 6 |
| **Total (35)** | **13 (37%)** | **11 (31%)** | **11 (31%)** |

---

## Section 3: Existing Spec Status Matrix

| Spec | Status | Implemented | Divergences |
|------|--------|-------------|-------------|
| `boolean_tolerance_layering.md` | Types implemented; production path bypasses | R1-R6 implemented. R7 (kernel integration) partial. | `tau_weld` spec says 2x, impl uses 0.4x. `for_scale()`/`default()` never called in production. `compute_adaptive_tol()` → `for_boolean_tol()` bypasses spec-compliant defaults. |
| `boolean_error_types.md` | Implemented with one violation | R1, R3, R4, R5 implemented. R2 violated. | `BooleanStageError` is `pub` (spec says MUST NOT export). |
| `boolean_shell_closure.md` | Implemented (Sprint 27) | All requirements met. All tests pass. | Level 6 escape hatch deliberately violates Invariant 1. |
| `boolean_determinism.md` | Implemented for polyline/wire | Ord on PointIndex, get_one/pop_one, closed polyline canonicalization, split_wire_recursive determinism. | Does not address edge welding non-determinism (FxHashMap in Phase 0, pointer-based BTreeMap keys) or timeout behavior. |
| `boolean_difference_operation.md` | Implemented | R1-R5 implemented. R6 (equivalence tests) missing. | No `difference(A,B) == and(A, not(B))` test. |
| `k8_loops_store_aabb.md` | RESOLVED (Sprint 36) | All sprints (A-E) implemented. All K8 tests pass. | None — fully resolved. |
| `multi_cut_regression.md` | Draft / Phase 2 | Tests written. Implementation fix pending. | Bug (first body vanishing) may still be present. |
| `robust_predicates_integration.md` | Implemented (core) | R1-R5 implemented. WASM compatible. | Missing orient3d SoS (not spec-required but critical gap per Phase 2 review). |

---

## Section 4: Test Coverage Matrix

### 4.1 Degenerate Configuration Coverage

| Class | Description | Test Count | Status | Missing Scenarios |
|-------|------------|-----------|--------|-------------------|
| **D1** | Coplanar faces | ~35 tests | **Good** (unit + integration) | Oblique coplanar, near-coincident plane equations, 3+ mutually coplanar, coplanar inner holes |
| **D2** | Coincident edges | ~10 tests | **Moderate** | Curved coincident edges, EF interference, non-right-angle coincidence, near-coincident within tau_model |
| **D3** | Vertex-on-face | ~5 tests | **Poor** | Vertex on face interior (not edge/corner), VE interference, multiple simultaneous VF, multi-scale testing |
| **D4** | Tangential intersection | ~2 tests (1 ignored) | **Very Poor** | Sphere-sphere tangent, cylinder-plane tangent, cylinder-cylinder tangent, cone-plane tangent, curved-curved tangent |
| **D5** | Non-manifold from chaining | ~15 tests | **Moderate** | Non-manifold edge detection+repair, 0-face solid, self-intersecting shell, Euler/watertightness after chain |

### 4.2 Spec-Required Test Types

| Test Type | Spec Requirement | Coverage | Notes |
|-----------|-----------------|----------|-------|
| **Property: Union commutativity** | MUST | Partial | Checks face count + volume, not topology |
| **Property: Intersection commutativity** | MUST | Partial | Volume only |
| **Property: Union idempotence** | MUST | Partial | Uses offset boss, not true self-union |
| **Property: Difference non-commutativity** | MUST | Yes | Face count + bounding box comparison |
| **Property: Intersection idempotence** | MUST | **Missing** | — |
| **Property: Difference ≡ AND+NOT** | MUST | Yes | Face count equivalence (2 tests) |
| **Property: De Morgan** | SHOULD | **Missing** | — |
| **Property: Associativity** | SHOULD | **Missing** | — |
| **Property: Absorption** | SHOULD | **Missing** | — |
| **Degenerate: Coplanar overlaps** | MUST | Good | 15+ tests |
| **Degenerate: Tangency** | MUST | Poor | 1 test |
| **Degenerate: Touching at vertex** | MUST | Poor | 2 tests |
| **Degenerate: Touching at edge** | MUST | Minimal | 2 tests |
| **Degenerate: Near-coincident (~tau_model)** | MUST | **Missing** | — |
| **Degenerate: Micro-features ~1 µm** | MUST | Poor | 1 test at 0.1 scale |
| **Fuzz: No panics** | MUST | **Missing** | `catch_unwind` masks instead of preventing |
| **Fuzz: Structured errors** | MUST | **Missing** | — |
| **Benchmarks: Per-stage** | SHOULD | **Missing** | No `criterion` infrastructure |

### 4.3 Test Suite Statistics

| Category | Count |
|----------|-------|
| Total boolean-related tests | ~270 |
| truck-shapeops unit tests | ~147 |
| kernel-fork healing.rs tests | ~29 |
| test-harness integration tests | ~95 |
| Tests using perturbation cascade | ~40+ |
| Tests for determinism | 3 |
| Tests for error propagation | ~5 |
| Algebraic property tests | 6 |
| Ignored/skipped tests | 5 |
| Reliably passing (no perturbation) | ~130 |
| Reliably passing (with perturbation) | ~100 |
| Time-sensitive/flaky (120s timeout) | ~5 (K7, K8, overlapping_cuts) |

---

## Section 5: Consolidated Gap Summary

### 5.1 Critical Gaps (Must Fix)

| ID | Gap | Source | Governance Violation | Spec Violation |
|----|-----|--------|---------------------|----------------|
| **C1** | No `tau_local` per-edge tolerance | P1-G4, P2-G3, P3-G10 | A3.3, A8.1 | Spec lines 114-127 |
| **C2** | Tolerances scattered across 6+ files with 10+ ad-hoc formulas | P2-G19, A3.3 | A3.3, A8.1 | Spec lines 79-128 |
| **C3** | All diagnostics stripped in release builds | P2-G21, A8.2 | A8.2 | Spec lines 296-301 |
| **C4** | 8 major pipeline components lack specs | P2 compliance | P2 | — |
| **C5** | Multiple recovery branches untested | P4 compliance | P4 | Spec lines 311-323 |
| **C6** | Perturbation cascade violates 1 µm feature preservation | P3-G1 | — | Spec lines 57-59, 277 |
| **C7** | `compute_adaptive_tol()` bypasses spec-compliant tolerance defaults | Spec-gap review | A8.1 | Spec lines 99-110 |
| **C8** | No SoS for orient3d — root cause of perturbation cascade | P2-G1 | — | Spec lines 224-234 |

### 5.2 High-Priority Gaps

| ID | Gap | Source | Notes |
|----|-----|--------|-------|
| **H1** | Platform-dependent 120s timeout | P3-G2 | Creates non-determinism across CPU speeds |
| **H2** | `Solid::new_unchecked` at Level 6 | P2-G22 | Produces potentially non-manifold solid with no oracle |
| **H3** | Only 2/10 quadric-quadric SSI cases have analytical solutions | P3-G7 | 80-95% of IC healing could use exact curves |
| **H4** | No post-healing validation | P3-G8 | IC healing mutates edges without revalidation |
| **H5** | BSpline error compounds: N × 5e-7 per chain | P3-G6 | Exceeds TOLERANCE after ~2 booleans |
| **H6** | And/Or/Unknown entangles operation with classification | P2-G7 | Spec requires Inside/Outside/OnBoundary/Unknown |
| **H7** | No XOR operation | Spec-gap | Explicitly required by spec |
| **H8** | No fuzz tests or property tests | Test coverage | Spec requires both |

### 5.3 Medium-Priority Gaps

| ID | Gap | Source |
|----|-----|--------|
| **M1** | Only FF interference type (miss VV/VE/EE/VF/EF) | P1-G2 |
| **M2** | No winding number classification | P2-G6 |
| **M3** | `BooleanStageError` exported publicly (spec says pub(crate)) | Spec-gap |
| **M4** | `tau_weld` spec says 2x, implementation uses 0.4x | Spec-gap |
| **M5** | No TouchingPolicy for degenerate cases | Spec-gap |
| **M6** | No regularized output (dangling geometry not removed) | Spec-gap |
| **M7** | Edge-neighbor propagation tie-breaking (And bias) unspecified | P2-G10 |
| **M8** | Plane-cylinder healing misses oblique (ellipse) case | P3-G12 |
| **M9** | `FxHashMap` remaining in polyline_construction and weld Phase 0 | P2-G14 |
| **M10** | D4 (tangential intersection) is nearly untested | Test coverage |

---

## Section 6: Prioritized Improvement Roadmap

This roadmap is ordered by impact and feasibility. Each improvement follows the governance workflow: spec → failing test → implementation → validation. Each improvement should be a separate sprint with its own spec in `specs/`.

### Tier 1: Foundation Fixes (Prerequisite for everything else)

These fix structural issues that block progress on all other improvements.

#### 1.1 Consolidate Tolerance Authority
**Addresses:** C1, C2, C7, M4
**Effort:** Medium
**Impact:** Eliminates the most widespread governance violation (A3.3, A8.1)

- Unify `BooleanTolerance` and `BooleanOptions` into a single struct
- Thread `tau_weld` through `finalize_boolean_shell` (replace hardcoded multipliers)
- Thread `tau_mesh` through `polyline_construction` (replace hardcoded `TOLERANCE`)
- Eliminate `BooleanTolerance::uniform()` — always use `from_model_tol()`
- Replace `compute_adaptive_tol()` → `for_boolean_tol()` with `BooleanOptions::for_scale()`
- Add `tau_local` field to IC edge metadata
- Spec: `specs/tolerance_consolidation.md`

#### 1.2 Make Diagnostics Unconditional
**Addresses:** C3
**Effort:** Low-Medium
**Impact:** Restores governance compliance for A8.2

- Replace `#[cfg(debug_assertions)] eprintln!()` with structured `BooleanDiagnostics` return
- Track assembly recovery level, healing counts, perturbation attempts
- Always-on (not gated behind debug_assertions)
- Report `new_unchecked` usage in diagnostics
- Spec: `specs/boolean_diagnostics.md`

### Tier 2: Algorithmic Improvements (Reduce perturbation dependence)

#### 2.1 Implement orient3d SoS
**Addresses:** C8, P2-G1
**Effort:** Low
**Impact:** Eliminates root cause of coplanar-face and vertex-on-face perturbation triggers

- Extend `sos_orient2d_tiebreak` pattern to 3D (Edelsbrunner-Mucke cofactor chain)
- Integrate into `robust_classify.rs`
- Expected: ~50% reduction in perturbation cascade usage
- Spec: `specs/orient3d_sos.md`

#### 2.2 Implement Analytical SSI for All Quadric Pairs
**Addresses:** H3, H5, P3-G7, P3-G12
**Effort:** Medium
**Impact:** Eliminates BSpline error accumulation for 80-95% of IC healing cases

- Plane-Cylinder (oblique): Ellipse via Patrikalakis Ch.5
- Plane-Cone: Conic section
- Plane-Sphere: Circle
- Cylinder-Cylinder: Ellipse / Viviani curve
- Each produces exact NURBS curves with zero approximation error
- Spec: `specs/analytical_ssi.md`

#### 2.3 Add Post-Healing Validation
**Addresses:** H4, P3-G8
**Effort:** Low
**Impact:** Catches invalid geometry before it enters subsequent booleans

- Run `Solid::try_new` after `heal_intersection_curves`
- Run `is_geometric_consistent` if available
- Reject healed solids that fail validation
- Attach healing residual as `tau_local` per-edge
- Spec: `specs/post_healing_validation.md`

### Tier 3: Classification Improvements

#### 3.1 Replace And/Or/Unknown with RelationToOther
**Addresses:** H6, P2-G7
**Effort:** Medium-High
**Impact:** Decouples classification from operation, enables XOR

- Define `RelationToOther = Inside | Outside | OnBoundary | Unknown`
- Refactor classification to produce operation-independent labels
- Make selection a pure function over `RelationToOther` + operation type
- Enable XOR as a direct consequence (H7)
- Spec: `specs/relation_to_other_classification.md`

#### 3.2 Implement Winding Number Classification
**Addresses:** M2, P2-G6
**Effort:** High
**Impact:** Provably correct inside/outside determination, eliminates ray-cast voting

- BFS on cell-patch graph (Zhou 2016, Eq. 7)
- Replaces 8-ray majority vote with exact winding numbers
- Eliminates edge-neighbor propagation heuristic
- Prerequisite: exact predicates for all geometry queries
- Spec: `specs/winding_number_classification.md`

### Tier 4: Perturbation Elimination

#### 4.1 Replace Timeout with Attempt-Count Limit
**Addresses:** H1, P3-G2
**Effort:** Low
**Impact:** Determinism across platforms

- Replace 120s `Instant::now()` timeout with fixed attempt count (e.g., 20)
- Remove `#[cfg(not(target_arch = "wasm32"))]` divergence
- Same algorithm on all platforms
- Spec: `specs/deterministic_cascade.md`

#### 4.2 Implement Explicit Edge Coincidence Detection
**Addresses:** C6, P3-G5, M1
**Effort:** Medium
**Impact:** Eliminates scale-expand perturbation strategies

- Detect coincident edges before boolean (OCCT Pave Block concept)
- Split coincident edges explicitly
- Removes need for scale-expand (1.02-1.05) strategies
- Spec: `specs/edge_coincidence_detection.md`

#### 4.3 Adopt Topology-Oriented Algorithm Structure
**Addresses:** All perturbation gaps
**Effort:** Very High (long-term)
**Impact:** Eliminates perturbation cascade entirely

- Restructure pipeline per Sugihara-Iri: topological operations first, numerics only choose branches
- Geometry failures cannot cascade to topology failures
- Combined with SoS + exact predicates + winding numbers, eliminates all physical perturbation
- Spec: `specs/topology_oriented_pipeline.md`

### Tier 5: Test Suite Hardening

#### 5.1 Add Missing Degenerate Tests
**Addresses:** C5, M10, test coverage gaps
**Effort:** Medium
**Impact:** Catches regressions in under-tested areas

- D4 tangential intersection tests (sphere-sphere, cylinder-plane, cylinder-cylinder)
- D3 vertex-on-face tests (vertex on face interior, VE interference)
- Near-coincident tests at `tau_model` boundary
- Micro-feature tests at actual 1 µm scale
- Spec: `specs/degenerate_test_corpus.md`

#### 5.2 Add Property Tests and Fuzz Tests
**Addresses:** H8, test coverage
**Effort:** Medium
**Impact:** Catches panic paths and algebraic identity violations

- Add `proptest` for random solid pairs (no panics, structured errors)
- Add missing algebraic property tests: intersection idempotence, De Morgan, associativity, absorption
- Add `cargo-fuzz` harness for boolean operations
- Spec: `specs/boolean_property_fuzz_tests.md`

#### 5.3 Add Benchmarks
**Addresses:** test coverage
**Effort:** Low
**Impact:** Enables performance regression detection

- `criterion` benchmarks for intersection, classification, stitching
- Small and high-face-count models
- Track perturbation attempt counts
- No spec needed (benchmark infrastructure)

### Tier 6: Spec Backfill

#### 6.1 Write Specs for 8 Unspecified Components
**Addresses:** C4
**Effort:** Medium
**Impact:** Governance compliance for P2

Priority order:
1. `specs/perturbation_cascade.md` — Most complex, most failure-prone
2. `specs/finalize_boolean_shell.md` — 6-level recovery cascade
3. `specs/ic_edge_healing.md` — 7-strategy healing pipeline
4. `specs/weld_coincident_edges.md` — 3-phase algorithm
5. `specs/coplanar_overlay.md` — iOverlay-based 2D overlay
6. `specs/edge_neighbor_propagation.md` — Heuristic classification fallback
7. `specs/pre_heal_vertex_unification.md` — Before every boolean
8. `specs/wire_splitting.md` — `split_wire_recursive` with depth guard

---

## Section 7: Implementation Priority Matrix

| Priority | Sprint | Deliverable | Dependencies |
|----------|--------|-------------|-------------|
| **P0** | Next | Tolerance consolidation (1.1) + Diagnostics (1.2) | None |
| **P1** | Next+1 | orient3d SoS (2.1) + Attempt-count limit (4.1) | None |
| **P2** | Next+2 | Analytical SSI (2.2) + Post-healing validation (2.3) | None |
| **P3** | Next+3 | Degenerate tests (5.1) + Property/fuzz tests (5.2) | 2.1, 2.2 |
| **P4** | Next+4 | RelationToOther (3.1) + XOR operation | 1.1 |
| **P5** | Next+5 | Edge coincidence detection (4.2) | 2.1 |
| **P6** | Next+6 | Spec backfill (6.1) | 1.1, 1.2, 2.1 |
| **P7** | Future | Winding numbers (3.2) | 3.1 |
| **P8** | Future | Topology-oriented pipeline (4.3) | 3.2, 4.2 |

---

## Section 8: What Comes After This Review

This review and its 4 phase documents (`phase1-intersection-division.md`, `phase2-classification-assembly.md`, `phase3-healing-perturbation.md`, `phase4-governance-gaps-roadmap.md`) serve as the architectural justification required by P2 for subsequent implementation work.

The Phase 4 roadmap above provides the implementation order. Each improvement item will:
1. Get its own spec in `specs/` (per P2)
2. Have failing tests written first (per P3)
3. Be implemented in a single sprint (per P7)
4. Be validated against the spec before merge (per DoD)

The 67 total gaps identified across all 4 phases (P1: 25, P2: 25, P3: 17) represent the comprehensive delta between the current boolean pipeline and the production spec target. The roadmap prioritizes them by structural impact (foundation fixes first) and dependency order (each tier builds on the previous).
