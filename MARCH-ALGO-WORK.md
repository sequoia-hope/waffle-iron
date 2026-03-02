# Boolean Operations Algorithm Improvement Plan

Actionable improvement tasks for the truck boolean pipeline, prioritized by impact.
Based on deep-dive research across the codebase, test suite, specs, and academic literature (2026-03-01).

Work through in order within each tier. Each task is self-contained. Mark `[x]` when done.

---

## Current State

**Test pass rate: ~80%.** Determinism fully solved (BTreeMap iteration + fixed cascade limit).
Core box-box, circle-on-box, and chained operations (up to 5) are reliable.

**8 ignored tests remain**, all traceable to:
1. Torus-plane IC shell assembly (5 tests: RB1, RB2, RB6, RB8, MO4)
2. Torus-cylinder IC (1 test: RB5)
3. Complex multi-boolean cascade exhaustion (1 test: S3)
4. Revolve+planar boolean (2 tests in revolve_cylinder_truck.rs)

**Roadmap status:** Phases A-D complete, E planned, F partial, G complete, H planned.

---

## Target Architecture

truck-shapeops was built as a research kernel. Several of its core designs are not
production-grade. Where OCCT (OpenCascade) has proven designs backed by 30+ years of
industrial use, we adopt them. The target is an **OCCT-inspired General Fuse
architecture** adapted to Rust/truck idioms.

### What Changes

| Subsystem | Current (truck) | Target (OCCT-inspired) | Why |
|-----------|----------------|----------------------|-----|
| **Fragmentation** | Full recompute per boolean op | Compute once, select buckets for union/intersect/difference | Eliminates redundant work in chained booleans |
| **Interference** | Jump straight to F/F (mesh-based IC) | Bottom-up V/V → V/E → E/E → V/F → E/F → F/F | Lower-dimensional results prevent redundant higher-dimensional computation |
| **Edge splitting** | IC endpoint projection + corner-touch snap | Pave-block-based deterministic splitting | Eliminates tolerance-dependent edge placement |
| **Tolerance model** | 7 hand-tuned scaling factors from `tau_model` | Single `tau_model` + shrunk ranges per pave block | Mathematically principled; eliminates `tau_weld`, `tau_boundary`, `tau_edge_cluster`, `tau_area` |
| **Classification** | Post-hoc cascade: coplanar → winding → ray-cast → edge-neighbor | IN/ON/OUT pre-computed during interference | Classification becomes a lookup, not a geometric computation |
| **Coplanar handling** | Multi-module heuristic cascade (`coplanar.rs`, `coplanar_overlay.rs`, `coplanar_splitting.rs`) | Same-domain connexity chains with `UnifySameDomain` post-pass | Systematic instead of heuristic |
| **Assembly** | Radial assembly + progressive tolerance weld (v2) fallback with force_merge | Pave-block-paired edges with shrunk-range validation | Eliminates the destructive 5.0x tolerance escalation |

### What Stays

These truck designs are already production-grade:

| Subsystem | Design | Assessment |
|-----------|--------|-----------|
| Robust predicates | Shewchuk + SoS + lazy exact escalation | State-of-the-art, matches CGAL |
| Winding numbers | Jacobson 2013 with Van Oosterom-Strackee | Correct algorithm, keep as secondary classifier |
| Deterministic ordering | BTreeMap/BTreeSet everywhere | Solved the nondeterminism problem completely |
| Analytical SSI | Plane-cylinder/cone/sphere special cases | Good, needs extension (A1, A2) |
| Diagnostics | `BooleanDiagnostics` stage-level tracking | Useful, keep and extend |

### Incremental Migration

This is NOT a rewrite. The migration is staged so that each step produces a working
system with passing tests. The existing `pave_block.rs` (Phase 1 data model already
in tree) is the starting point.

---

## Tier 1: High Impact, Achievable

### A1. Torus-Plane Analytical SSI
**Would unblock:** RB1, RB2, RB6, RB8, MO4 (5+ ignored tests)
**Files:** `vendor/truck/truck-shapeops/src/transversal/intersection_curve/analytical.rs`
**Problem:** 360deg revolve produces 3 lateral face patches sharing RevolutedCurve surfaces. Mesh-based IC finds overlapping triangles but `search_triple` Newton iteration diverges on noisy mesh points near torus-plane intersections. All 50 cascade perturbations fail the same way.
**Root cause chain:**
```
360deg revolve -> 3 lateral face patches (division=3 in rsweep)
  -> extract_interference finds SOME triangle overlaps (polylines non-empty)
  -> try_new calls search_triple (Newton) for each polyline point
  -> Newton diverges on noisy mesh points near torus-plane IC
  -> try_new returns None -> collect::<Option<Vec>> returns None
  -> loops_store skips face pair -> face undivided -> open edges
  -> cascade exhaustion (50 attempts)
```
**Fix:** Implement analytical torus-plane intersection. The intersection of a torus with a plane is a degree-4 space curve that can be decomposed into at most two closed loops. For the common case (plane perpendicular to torus axis), the result is two concentric circles. For tilted planes, parametric sampling of the degree-4 curve with dense polyline output.
**Alternative (lower effort):** Improve mesh-based IC quality for torus surfaces specifically — denser tessellation near expected IC location, better Newton seeding from analytical approximation.
**Verify:** RB1, RB2, RB6, RB8 tests pass (remove `#[ignore]`).

### A2. Cylinder-Cylinder Analytical SSI
**Would unblock:** CC1 edge case, general industrial model coverage
**Files:** `vendor/truck/truck-shapeops/src/transversal/intersection_curve/analytical.rs`
**Spec:** `specs/boolean_phase_f_analytical_ssi.md` (F4, already specified)
**Problem:** Cylinder-cylinder intersections fall back to mesh-based polyline extraction, which produces polylines with numerical drift. This is extremely common in industrial CAD models (~60% of remaining SSI needs).
**Fix:** Implement for equal-radius, intersecting axes (angle > 60deg), closest distance < 5% of R. Returns two elliptic curves (one per cylinder interior intersection). Refactor `AnalyticalIC.ellipse` to `AnalyticalIC.ellipses: Vec<EllipseParams>`.
**Verify:** New test for perpendicular equal-radius cylinders passes. Existing boolean tests unaffected.

### A3. Fragmentation Cache
**Would improve:** Chained boolean performance; enables GFA-style operation selection
**Files:** `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`
**Problem:** `and_result()`, `or_result()`, and `difference_result()` each call the full pipeline (`create_loops_stores()` → tessellation → BVH → face division → classification) independently. In Waffle Iron's parametric history (sketch → extrude → boolean → boolean → ...), this means the same expensive intersection computation runs multiple times on the same geometry.
**Design:** The `ClassifiedShellBuckets` struct already stores the four face sets (`and0`, `or0`, `and1`, `or1`). This is structurally identical to GFA's decomposition (Sp1/Sp2/Sp12). Expose it as a public "fragmentation result" type:
```rust
pub struct FragmentationResult<P, C, S> {
    pub and0: Shell<P, C, S>,  // S1 faces inside S2
    pub or0: Shell<P, C, S>,   // S1 faces outside S2
    pub and1: Shell<P, C, S>,  // S2 faces inside S1
    pub or1: Shell<P, C, S>,   // S2 faces outside S1
}

// Boolean ops become bucket selections:
// Union     = or0 + or1 + and0 (or and1, they overlap)
// Intersect = and0 + and1
// Diff(A-B) = or0 + inverse(and1)
```
**Fix:** Refactor `classify_one_pair_of_shells_result_with_tol()` to return `FragmentationResult`. Add public `fragment()` method. Implement `union_from()`, `intersect_from()`, `difference_from()` as thin selectors on the cached result.
**Verify:** All boolean tests pass. Add test: fragment once, select union and intersection from same result, both valid.

---

## Tier 2: Architectural Migration (GFA Adoption)

These tasks replace truck's ad-hoc designs with production-grade OCCT-inspired
equivalents. Each step is independently valuable and preserves test compatibility.

### D1. Pave Block Integration into Face Division
**Replaces:** Current tolerance-dependent IC endpoint projection in `loops_store`
**Files:** `vendor/truck/truck-shapeops/src/transversal/pave_block.rs`, `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs`, `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs`
**Problem:** Face division currently projects IC endpoints onto face boundaries using tolerance-based matching. This causes figure-8 wires when IC endpoints land near face boundary vertices (the "corner-touch bug" partially fixed by MV3 snap). The root issue is that edge splitting is tolerance-dependent rather than topology-driven.
**Existing work:** `pave_block.rs` already contains Phase 1 data model: `PaveBlock<C>`, `IcVertex`, `IcSegment<C>`, `FaceInterference<C>`, `InterferenceTable<C>`. Comments indicate Phase 2 (wire into face division) and Phase 3 (replace `divide_one_face`) are planned.
**Fix:** Implement Phase 2 and Phase 3 as described in `pave_block.rs`:
- Phase 2: Wire `InterferenceTable` into `divide_faces_with_coplanar()`. Face edges are split at pave positions before IC curves are inserted. This ensures IC endpoints align exactly with edge split points.
- Phase 3: Replace `divide_one_face()` with pave-block-based face division. Each face's boundary is pre-split by paves, and IC segments connect pave-to-pave with guaranteed topological consistency.
**Verify:** All boolean tests pass. Corner-touch snap (MV3) should become unnecessary (remove or keep as belt-and-suspenders).

### D2. Shrunk Ranges
**Replaces:** The 7-tolerance system (`tau_weld`, `tau_boundary`, `tau_edge_cluster`, `tau_area`)
**Files:** `vendor/truck/truck-shapeops/src/transversal/pave_block.rs`, `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`
**Depends on:** D1 (pave blocks must be integrated first)
**Problem:** truck derives 7 tolerance values from `tau_model` via hand-tuned scaling factors (0.4x for weld, 5.0x for edge cluster, etc.). These interact unpredictably: `tau_weld = 0.4x` merges vertices across narrow bosses, while `tau_boundary = 0.5x` is too tight for coarse IC approximations. The V2 assembly Level 2 (5.0x) destroys fine features.
**Design:** Each pave block computes a **shrunk range** — the parametric interval reduced by the tolerance spheres of its bounding vertices:
```
shrunk_start: C(t) where dist(C(t), V_front) = Tol(V_front) + Tol(C)
shrunk_end:   C(t) where dist(C(t), V_back)  = Tol(V_back)  + Tol(C)
```
The shrunk range defines where interference can actually occur. Portions inside tolerance spheres are topologically part of the vertex, not the edge. If the shrunk range is empty (tolerance spheres consume the entire edge), the bounding vertices merge into a single same-domain vertex.
**Fix:**
- Add `shrunk_range: Option<(f64, f64)>` to `PaveBlock`
- Compute via `fill_shrunk_data()` before E/E and E/F interference tests
- Replace `tau_weld` usage with shrunk-range-based vertex merging
- Replace `tau_boundary` usage with shrunk-range-based IC filtering
- Replace `tau_edge_cluster` (the destructive 5.0x) with shrunk-range-based edge pairing
- `BooleanTolerance` simplifies to: `tau_model` + `tau_mesh` + `tau_coplanar` (3 values instead of 7)
**Verify:** All boolean tests pass. Remove `tau_weld`, `tau_boundary`, `tau_edge_cluster`, `tau_area` from `BooleanTolerance`. The tolerance sensitivity issues documented in truck issue #68 should improve.

### D3. Bottom-Up Interference Computation
**Replaces:** Current F/F-only approach (jumps straight to mesh-based intersection curves)
**Files:** `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` (new interference pipeline)
**Depends on:** D1 (pave blocks), D2 (shrunk ranges)
**Problem:** truck's boolean pipeline starts at Face/Face intersection (mesh tessellation → AABB culling → IC computation). It has no Vertex/Vertex, Vertex/Edge, or Edge/Edge interference detection. This means:
- Coincident vertices between operands are not merged, causing duplicate geometry in the result
- Shared edges are not detected, leading to redundant IC computation
- Edge crossings are discovered only via F/F intersection, missing lower-dimensional contacts
**Design:** Implement OCCT-style ascending-dimension interference:
1. **V/V** — Merge coincident vertices (distance ≤ sum of tolerances). Connected components form connexity chains. Each chain produces a single replacement vertex.
2. **V/E** — Project vertices onto edge curves, creating paves. Each projection adds a split point to the affected pave block.
3. **E/E** — Detect edge crossings and overlapping segments. Overlapping pave blocks form **common blocks** (shared geometry, single representative edge in result).
4. **V/F, E/F** — Project vertices/edges onto surfaces (existing functionality, reorganized).
5. **F/F** — Compute intersection curves (existing `create_loops_stores`, now only for genuinely intersecting face pairs that weren't resolved by lower dimensions).
**Fix:** Implement as a new `PaveFiller` struct that processes the data structure in dimension order. Wire into the boolean pipeline before `create_loops_stores()`. The existing F/F code becomes the last stage rather than the only stage.
**Verify:** All boolean tests pass. Add tests for V/V merging (coincident vertices across operands), E/E common blocks (shared edges between operands).

### D4. Same-Domain Connexity Chains
**Replaces:** Multi-module coplanar heuristic cascade (`coplanar.rs`, `coplanar_overlay.rs`, `coplanar_splitting.rs`)
**Files:** `vendor/truck/truck-shapeops/src/transversal/coplanar.rs` and related
**Depends on:** D1 (pave blocks), D3 (bottom-up interference for proper face identification)
**Problem:** truck handles coplanar faces through a heuristic cascade:
1. `classify_coplanar_via_overlay()` — 2D polygon boolean via iOverlay
2. `classify_coplanar_fragment()` — point-in-face with normal sense check, straddling detection
3. Dense boundary sampling (32 points/edge) for faces with few vertices
4. Multi-point containment testing with anti-sense/same-sense branching

This is fragile and produces incorrect results when faces are partially coplanar or when the 2D projection introduces errors.
**Design:** After face splitting (D1), identify same-domain face pairs using `AreFacesSameDomain()`:
- Normal dot product near ±1.0 AND sample point within tolerance of the other surface
- Build undirected graph of same-domain pairs
- Connected components become **connexity chains**
- Each chain collapses to a single representative face in the result

Post-processing with `UnifySameDomain`-style merge:
- Detect groups of neighboring faces on coincident surfaces
- Unify into single faces with merged boundaries
- Concatenate neighboring BSpline/Bezier edges with C1 continuity
**Fix:** Replace the three coplanar modules with a single `same_domain.rs` implementing connexity chain detection and face merging. The iOverlay dependency may become unnecessary.
**Verify:** All boolean tests pass (especially the coplanar pipeline tests in boolean_workflows.rs Category G). Sprint 41's reverted coplanar face merging should now work correctly because it's based on topological connexity rather than geometric heuristics.

### D5. Pre-Computed IN/ON/OUT Classification
**Replaces:** Post-hoc classification cascade (winding → ray-cast → edge-neighbor propagation)
**Files:** `vendor/truck/truck-shapeops/src/transversal/faces_classification/mod.rs`
**Depends on:** D3 (bottom-up interference provides the state data)
**Problem:** truck's classification cascade exists because it lacks the deterministic state pre-computation that GFA provides. Each tier handles cases where the previous one was ambiguous:
1. Coplanar overlay → 2. Coplanar fragment → 3. Winding number → 4. Ray-cast (8-ray majority voting, escalated perturbation) → 5. Edge-neighbor propagation (10-round limit)

This cascade has multiple failure modes: corner-coplanar ray-cast degeneracy, edge-neighbor stalling, Strategy 5 "accept any single ray-cast" fallback.
**Design:** With bottom-up interference (D3), each face maintains a `FaceInfo` structure with six collections:
- Pave blocks with state IN (edges interior to face)
- Vertices with state IN
- Pave blocks with state ON (boundary edges)
- Vertices with state ON
- Pave blocks from intersection curves (section edges)
- Vertices from intersection points

Classification becomes a lookup: IN/ON/OUT is determined by the interference data, not by geometric sampling.
**Fix:** Add `FaceInfo` to the interference data structure. Populate during bottom-up interference. Replace the classification cascade with state lookups. Keep winding number as a validation check (not primary classifier).
**Verify:** All boolean tests pass. The edge-neighbor propagation fallback should never trigger. The ray-cast cascade becomes unnecessary (remove or keep as assertion-mode validation).

---

## Tier 3: Quick Wins (Low Effort)

These are independently valuable regardless of the architectural migration.

### C1. Wrap `eprintln!` in Debug Feature Flag
**Files:** `vendor/truck/truck-shapeops/src/transversal/` (89 occurrences across module)
**Problem:** 89 `eprintln!` calls in the transversal module, including 42 in integrate/mod.rs hot path. Each is a syscall. Production users see hundreds of lines of stderr spam. Diagnostics already collected in-memory via `BooleanDiagnostics`.
**Fix:** Wrap all `eprintln!` in `#[cfg(feature = "boolean_debug")]` or `#[cfg(debug_assertions)]`. Add the feature flag to `Cargo.toml`. Enable in test builds, disable in release/WASM builds.
**Verify:** `grep -rn "eprintln!" vendor/truck/truck-shapeops/src/transversal/ | grep -v "cfg("` returns 0 matches. Existing tests pass with the feature enabled.

### C2. Reduce V2 Assembly Tolerance Escalation
**Files:** `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`
**Problem:** Level 2 tolerance escalation uses 5.0x `tau_model`, merging vertices 5x apart. This destroys fine features and can create topologically invalid faces.
**Fix:** Reduce to 2.0x. Change Level 1 from 0.4x to 1.0x (full `tau_model`). The progression becomes: 0.2x → 1.0x → 2.0x instead of 0.2x → 0.4x → 5.0x.
**Note:** This is an interim fix. D2 (shrunk ranges) will eliminate the V2 assembly tolerance escalation entirely.
**Verify:** All currently-passing boolean tests still pass. Run proptest suite to check for regressions.

### C3. Scale-Aware Gap-Filling Threshold
**Files:** `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` (line ~1249)
**Problem:** Maximum gap size for wire repair is hard-coded at 5.0 units. Breaks for very small or very large models.
**Fix:** Replace with `max(10.0 * tau_model, 0.1 * model_extent)`. Pass model extent through to the gap-filling function.
**Note:** This is an interim fix. D1 (pave-block-based face division) should eliminate gap-filling entirely by ensuring topological consistency from the start.
**Verify:** Existing tests pass. Test with a 0.001-unit scale model and a 10000-unit scale model.

### C4. IC Healing: Ellipse Fitting
**Files:** `crates/kernel-fork/src/healing.rs`
**Problem:** Tier 2 IC healing only fits circles (3-point circumscribed circle). Cylinder-cylinder intersections produce ellipses, forcing fallback to BSpline approximation which has higher residual error.
**Fix:** Implement 5-point ellipse fitting (circumscribed ellipse algorithm). Extend `analytical_circle_arc_from_leader()` to try ellipse fit when circle fit fails validation. Validate ellipse against both surfaces with `validate_bspline_on_surfaces()`.
**Verify:** IC healing diagnostics show increased Tier 2 (analytical) usage for cylinder-cylinder pairs.

### C5. Cascade Instrumentation (Phase E)
**Spec:** `specs/phase_e_cascade_deprecation.md` (already approved)
**Files:** `crates/kernel-fork/src/healing.rs`
**Problem:** `CascadeReport` and `BooleanDiagnosticsSummary` fields are declared but empty. No structured data on direct vs perturbation success rates.
**Fix:** Implement the already-specified counters: `CASCADE_DIRECT_SUCCESS`, `CASCADE_PERTURBATION_SUCCESS`, `CASCADE_EULER_FALLBACK`, `CASCADE_EXHAUSTED`, `CASCADE_TOTAL`. Add `CascadeStats` public API. Zero behavior change.
**Note:** This data is critical for validating that D1-D5 are reducing cascade reliance. Implement before or alongside architectural migration.
**Verify:** CM3 invariant: `direct + perturbation + euler_fallback + exhausted == total`. CM1: For 5 simple booleans, `direct_success >= 3`.

---

## Rejected Approaches

These were evaluated during research but are **NOT part of this plan**:

- **Mesh boolean fallback** (Cherchi 2022, Geogram-style) — User rejected. Would produce mesh-only results (not BREP), losing geometric fidelity and breaking downstream operations that require surface representation.
- **Hybrid BREP-mesh proxy** (Yang et al. SIGGRAPH 2025) — Very high effort, requires bijective mesh-to-BREP mapping infrastructure that doesn't exist. Interesting for long-term research but not actionable now.
- **Exact arithmetic for SSI computation** — NURBS intersection curves are inherently approximate. Exact arithmetic adds cost without benefit for construction stages (as opposed to predicate stages where it's already implemented).
- **Fillet/chamfer/shell operations** — Per project governance, deferred indefinitely.
- **Fast winding numbers (FMM, Barill 2018)** — Performance optimization only, not a robustness improvement. Current winding number speed is adequate.

---

## What We're Already Doing Right

| Technique | Source | Status |
|-----------|--------|--------|
| Shewchuk adaptive-precision predicates | Shewchuk 1997 | Implemented in `robust_classify.rs` |
| Simulation of Simplicity (SoS) | Edelsbrunner-Mucke | Implemented (`sos_orient3d_tiebreak`) |
| Generalized winding numbers | Jacobson 2013 | Implemented in `winding.rs` |
| Lazy exact escalation | CGAL-inspired | Implemented (`lazy_exact_triple_sign`) |
| Deterministic iteration ordering | - | Implemented (BTreeMap everywhere) |
| Analytical SSI (plane-cyl/cone/sphere) | Classical | Implemented in `analytical.rs` |
| Coplanar exact detection | Custom | Implemented in `coplanar.rs` |
| Diagnostics infrastructure | Custom | Implemented (`BooleanDiagnostics`) |
| Pave block data model (Phase 1) | OCCT-inspired | Implemented in `pave_block.rs` |

---

## Where We're Extending Beyond truck's Research Scope

truck was designed as a research kernel — its boolean pipeline is impressive given that
scope. These are areas where production CAD demands go beyond what truck was built for,
and where we're investing to close the gap.

| Area | Current Behavior | Production Requirement | Task |
|------|-----------------|----------------------|------|
| Per-operation recomputation | Full pipeline per boolean call | Cache fragmentation across chained ops | A3 |
| Tolerance derivation | 7 values scaled from `tau_model` | Shrunk ranges from single `tau_model` | D2 |
| Interference scope | F/F intersection only | Bottom-up V/V → V/E → E/E → F/F | D3 |
| Edge splitting | IC endpoint projection + tolerance | Pave-block-based deterministic splitting | D1 |
| Coplanar handling | Multi-module heuristic cascade | Same-domain connexity chains | D4 |
| Face classification | Multi-tier fallback cascade | Pre-computed IN/ON/OUT from interference data | D5 |
| Assembly recovery | Progressive tolerance escalation to 5.0x | Shrunk-range-based edge pairing | D2 |
| Debug output | 89 `eprintln!` always active | Feature-gated debug output | C1 |
| Gap-filling threshold | Fixed 5.0-unit maximum | Scale-aware formula → pave blocks eliminate gaps | C3 |

---

## Surface Pair Coverage Matrix

| Surface Pair | Analytical SSI | IC Healing | Status |
|--------------|---------------|------------|--------|
| Plane-Plane | Exact Line | Line | Done |
| Plane-Cylinder | Ellipse/circle | Circle arc | Done |
| Plane-Cone | Conic section | Circle (limited) | Done |
| Plane-Sphere | Circle | Circle arc | Done |
| Cylinder-Cylinder (equal-R) | Two ellipses | TBD | Specified (A2) |
| Cylinder-Cylinder (unequal-R) | Not implemented | BSpline | Deferred |
| Torus-Plane | Not implemented | Not implemented | Planned (A1) |
| Torus-Cylinder | Not implemented | Not implemented | Deferred |
| Curved-Curved (generic) | None | BSpline | Fallback only |

---

## Test Suite Summary

| Test File | Tests | Pass | Ignored | Primary Blocker |
|-----------|-------|------|---------|----------------|
| boolean_failures.rs | 22 | 21 | 0 | Angled extrude (1 fail) |
| boolean_edge_cases.rs | 7 | 7 | 0 | - |
| boolean_recovery.rs | 15 | 14 | 1 | S3: cascade exhaustion |
| boolean_workflows.rs | 30+ | 30+ | 0 | - |
| boolean_shell_closure.rs | 4 | 4 | 0 | - |
| boolean_determinism.rs | 3 | 3 | 0 | - |
| boolean_properties.rs | 27 | 27 | 0 | - |
| revolve_boolean.rs | 8 | 3 | 5 | Torus-plane IC |
| revolve_cylinder_truck.rs | 2 | 0 | 2 | Curved+planar boolean |
| multi_op_chains.rs | varies | most | 1 | Torus-plane IC |
| assay_box_box.rs | 4 (proptest) | 4 | 0 | - |

---

## Validation

After completing each task, run:
```bash
./scripts/test.sh full              # All ~910 Rust tests
cargo test -p test-harness          # Boolean-specific tests
```

If any currently-passing test breaks, fix it before moving to the next task.

---

## Key References

- [OCCT Boolean Operations Specification](https://dev.opencascade.org/doc/overview/html/specification__boolean_operations.html)
- [OCCT Boolean Operations User Guide (v7.4)](https://dev.opencascade.org/doc/occt-7.4.0/overview/html/occt_user_guides__boolean_operations.html)
- [BOPAlgo_CellsBuilder (GFA cell selection)](https://dev.opencascade.org/doc/refman/html/class_b_o_p_algo___cells_builder.html)
- [ShapeUpgrade_UnifySameDomain](https://dev.opencascade.org/doc/refman/html/class_shape_upgrade___unify_same_domain.html)
- [Shewchuk's Robust Predicates](https://www.cs.cmu.edu/~quake/robust.html)
- [Robust Inside-Outside Segmentation (Jacobson 2013)](https://igl.ethz.ch/projects/winding-number/)
- [Boolean Operations via Generalized Winding Numbers](https://arxiv.org/pdf/1601.07953)
- [Interactive and Robust Mesh Booleans (Cherchi 2022)](https://arxiv.org/abs/2205.14151)
- [Boolean Operation for CAD Models Using Hybrid Representation (Yang 2025)](https://dl.acm.org/doi/10.1145/3730908)
- [NURBS Approximation of SSI Curves](https://link.springer.com/article/10.1007/BF02519033)
- [CGAL Robustness Manual](https://doc.cgal.org/latest/Manual/devman_robustness.html)
- [Fast Winding Numbers (Barill 2018)](https://www.dgp.toronto.edu/projects/fast-winding-numbers/)
- [Fuzzy Boolean Operations (OCCT Forum)](https://dev.opencascade.org/content/fuzzy-boolean-operations)
