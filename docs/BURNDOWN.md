# Boolean Foundation Burndown

Long-term roadmap for bringing Waffle Iron's boolean subsystem to production quality,
as defined by `docs/SHAPEOPS-BOOLEAN-SPEC.md`.

**Priority legend**: P0 = blocking, P1 = important, P2 = wanted, P3 = nice-to-have
**Size legend**: S = hours, M = day, L = multi-day, XL = week+

---

## Phase A: Foundation (Tolerance + Errors + Predicates)

Lays the infrastructure all subsequent boolean work depends on.

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| A1 | Structured `BooleanError` enum replacing `Option<Solid>` | P0 | M | — | kernel-fork/types.rs, truck-shapeops/integrate, kernel-fork/truck_kernel.rs | **Complete** (Sprint 1). Note: `BooleanStageError` in truck-shapeops and `BooleanError` in kernel-fork are separate types with no `From` bridge. |
| A2 | `BooleanOptions` tolerance context (tau_model/mesh/weld/work/coplanar) | P0 | M | — | kernel-fork/types.rs, kernel-fork/truck_kernel.rs, kernel-fork/healing.rs | **Implemented** — all 5 tolerance fields + `min_feature_size` + `validate()` in types.rs:141-265. Gap: NOT wired into truck-shapeops functions (they still take raw `tol: f64`). |
| A3 | Robust geometric predicates (`robust` crate) in ray-cast classification | P1 | L | A1 | truck-shapeops/Cargo.toml, truck-shapeops/integrate, truck-shapeops/coplanar.rs | **Partial** (Sprint 1). `robust_orient2d` active in coplanar point-in-polygon. `robust_orient3d` and `robust_ray_triangle_cross` are dead code (`#[allow(dead_code)]`). Main ray-cast pipeline uses non-robust floating point path. |
| A4 | Replace AND/OR tagging with `RelationToOther` classification | P2 | L | A1,A3 | truck-shapeops/loops_store, truck-shapeops/integrate, truck-shapeops/divide_face | **Deprioritized** — current AND/OR/Unknown works for all 4 ops |
| A5 | Wire `BooleanOptions` through truck-shapeops functions | P1 | M | A2 | truck-shapeops/integrate, truck-shapeops/loops_store | **Not started** — `and_result`/`or_result`/`difference_result` take raw `tol: f64`; layered tolerances (tau_mesh, tau_weld, tau_coplanar) unused at shapeops call sites |

**Parallelization**: A1 and A2 in parallel. A3 after A1. A4 after A1+A3. A5 after A2.

---

## Phase B: Boolean Pipeline Hardening

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| B1 | Complete coplanar face splitting | P0 | XL | A1 | truck-shapeops/loops_store, coplanar_splitting.rs, integrate | **Hardened** (Sprint 4 — parity ray-cast, overlap check, same-sense shortcut) |
| B2 | Add `difference()` and XOR boolean operations | P1 | M | A4 or standalone | truck-shapeops/integrate, kernel-fork/traits.rs, modeling-ops/boolean.rs | **Complete** (Sprint 1, difference only; XOR deferred — no `xor`/`sym_diff` function exists) |
| B3 | Box-cylinder boolean reliability | P1 | XL | A2,B1 | truck-shapeops/intersection_curve, kernel-fork/healing.rs | **Substantial** — punched-cube + chained booleans work via NURBS arc healing (Sprint 6); e5 rewritten to pass using `extrude_directed_no_merge` + explicit `boolean_subtract`; `auto_union_stress` now passing (7/7); boundary/corner edge cases + g3 full-face cut remain |
| B4 | `Solid::try_new` enforcement (no panics) | P1 | S | A1 | truck-shapeops/integrate | **Nearly complete** — all `Solid::` constructions use `try_new`. One remaining `Face::new` (panicking) at integrate/mod.rs:566 in Phase 2 of `weld_coincident_edges` edge substitution. |
| B5 | `TouchingPolicy` for degenerate cases | P2 | M | A2,A4 | kernel-fork/types.rs, truck-shapeops/integrate | **Deprioritized** — no current test failures |
| B6 | Fix `Face::new` in `weld_coincident_edges` Phase 2 | P1 | S | B4 | truck-shapeops/integrate/mod.rs:566 | **Not started** — Phase 0 uses `Face::try_new` correctly but Phase 2 (canonical edge substitution) still uses panicking `Face::new` |

**Parallelization**: B1, B2, B4 in parallel. B3 after B1. B5 after A4. B6 after B4.

---

## Phase C: Missing Kernel Operations

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| C1 | TruckKernel `chamfer_edges` (planar geometry only) | P1 | L | — | kernel-fork/truck_kernel.rs | **Complete** (Sprint 5.5 — chamfer works via boolean subtraction pipeline). Note: `test_truck_chamfer` ignore annotation ("NotSupported") is stale — chamfer is implemented. |
| C2 | TruckKernel `shell` (face removal + boolean subtraction) | P1 | XL | — | kernel-fork/truck_kernel.rs | **Implemented** (Sprint 3 — planar faces only, boolean subtraction approach at truck_kernel.rs:585-850). Note: `test_truck_shell` ignore annotation ("NotSupported") is stale — shell is implemented for planar face removal. |
| C3 | TruckKernel `fillet_edges` (rolling-ball surfaces) | P2 | XL | C1 | kernel-fork/truck_kernel.rs | **Not started** — truck-shapeops has a fillet module but it is NOT wired into TruckKernel. `test_truck_fillet` correctly returns NotSupported. |

**Parallelization**: C1 and C2 in parallel. C3 after C1 (shares trimming infra).
All of Phase C is independent of Phases A-B.

---

## Phase D: UI Completion

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| D1 | Edge selection mode in 3D viewport | P0 | M | — | app/viewport/, app/engine/store.svelte.js, wasm-bridge/ | **Complete** (Sprint 2) |
| D2 | Fillet dialog | P1 | M | D1,C3 | app/ui/FilletDialog.svelte, Toolbar.svelte | **UI shell exists** (Sprint 8) — FilletDialog.svelte created with "not yet supported" warning. Kernel wiring (C3) still missing. |
| D3 | Chamfer dialog | P1 | S | D1,C1 | app/ui/ChamferDialog.svelte, Toolbar.svelte | **UI shell exists** (Sprint 8) — ChamferDialog.svelte created with "not yet supported" warning. Kernel is ready (C1 complete). Needs wiring. |
| D4 | Shell dialog | P1 | S | C2 | app/ui/ShellDialog.svelte, Toolbar.svelte | **UI shell exists** (Sprint 8) — ShellDialog.svelte created. Kernel is ready (C2 implemented). Needs wiring. |
| D5 | Revolve live preview (ghost mesh) | P2 | M | — | app/viewport/GhostPreview.svelte, RevolveDialog.svelte | Not started |

**Parallelization**: D1 and D5 in parallel. D2/D3/D4 after their kernel + D1 deps.

---

## Phase E: Advanced Features + Polish

| ID | Item | Pri | Size | Deps | Crates | Status |
|----|------|-----|------|------|--------|--------|
| E1 | GeomRef testing against real TruckKernel | P1 | M | A | test-harness, feature-engine/resolve.rs | **Complete** (Sprint 2, 17 tests) |
| E2 | Query-based `Selector::Query` resolution | P2 | M | E1 | feature-engine/resolve.rs, waffle-types/geom_ref.rs | Not started |
| E3 | Local per-edge tolerances (`tau_local`) | P3 | L | A2,A4 | truck-shapeops/intersection_curve | **Deprioritized** — invasive, no current test failures |
| E4 | Input validation/healing modes (strict + heal) | P3 | M | A1 | truck-shapeops/healing, kernel-fork/truck_kernel.rs | Not started |
| E5 | Property tests + degenerate regression corpus | P2 | L | A1,B1,B2 | test-harness, kernel-fork/tests/ | **Complete** (Sprint 2+5 — 15 property tests, 4 H-tests) |
| E6 | Revolve role detection fix (real truck normals) | P2 | S | E1 | modeling-ops/revolve.rs | **Complete** (Sprint 2 — 3 bugs fixed: normals, angle units, threshold) |

---

## Dependency Graph

```
Phase A (Foundation)
  A1 (BooleanError) ─────┬──────────────────────> Phase B1, B4
  A2 (BooleanOptions) ───┤                        Phase B3, B5, E3
  A3 (Robust Preds) ─────┤──> A4 (RelationToOther) ──> B5, E3
  A5 (Wire Options) ─────┘──> uses A2
                          │
Phase B (Hardening)       │
  B1 (Coplanar split) ───┤──> B3 (Box-cyl)
  B2 (difference/XOR) ───┤
  B4 (try_new enforce) ──┤──> B6 (Face::new fix)
                          │
Phase C (Kernel Ops) — independent of A/B
  C1 (Chamfer) ──> C3 (Fillet)
  C2 (Shell)

Phase D (UI) — depends on C
  D1 (Edge select) ──> D2, D3, D4
  D5 (Revolve preview) — independent

Phase E (Advanced) — depends on A, B
  E1 (GeomRef real) ──> E2 (Query selector), E6
  E5 (Property tests)
```

---

## Ignored Test Inventory

**17 total ignored tests** across the workspace (down from 19 — e5 rewritten as non-ignored F5, auto_union_stress un-ignored and passing).

### Boolean-related (5 tests)

| Test | File | Reason |
|------|------|--------|
| `g3_full_face_rect_cut` | test-harness/boolean_workflows.rs | Full-face pocket cut — all 4 boundary edges coincident produce NotSimpleWire. WIP fix in unstaged vendor/truck changes. |
| `rect_cut_coplanar_edges` | test-harness/boolean_failures.rs | Coplanar edge coincidence — NotSimpleWire from divide_one_face |
| `circle_cut_tangent_to_box_edge` | test-harness/boolean_failures.rs | Circle tangent to box edge — NoSolid from boolean |
| `circle_cut_crossing_box_edge` | test-harness/boolean_failures.rs | Circle extending beyond face boundary — NoSolid despite cardinal perturbation |
| `cut_from_angled_direction` | test-harness/boolean_failures.rs | Angled extrude direction — truck cannot handle angled cylinder boolean |

### Kernel ops (4 tests)

| Test | File | Reason | Notes |
|------|------|--------|-------|
| `test_truck_fillet` | test-harness/scenarios_truck.rs | TruckKernel fillet returns NotSupported | Correctly ignored — C3 not started |
| `test_truck_chamfer` | test-harness/scenarios_truck.rs | TruckKernel chamfer returns NotSupported | **Stale ignore** — chamfer IS implemented (C1 complete). Likely passes now. |
| `test_truck_shell` | test-harness/scenarios_truck.rs | TruckKernel shell returns NotSupported | **Stale ignore** — shell IS implemented for planar faces (C2). May pass now. |
| `test_truck_cut_direction_selection` | test-harness/scenarios_truck.rs | Angled cut direction not yet wired | |

### Benchmarks & diagnostics (5 tests)

| Test | File | Reason |
|------|------|--------|
| `bench_boolean_tolerances` | kernel-fork/truck_kernel.rs | Manual benchmark — run with `--ignored --nocapture` |
| `diag_boolean_configs` | kernel-fork/truck_kernel.rs | Manual diagnostic — run with `--ignored --nocapture` |
| `bench_*` (x3) | sketch-solver/solve_tests.rs | Wall-clock benchmarks |

### Third-party library bugs (3 tests)

| Test | File | Reason |
|------|------|--------|
| `parallel` | slvs-patch (upstream) | Crashes in original library |
| `cubic_line_tangent` | slvs-patch (upstream) | Crashes in original library |
| `same_orientation` | slvs-patch (upstream) | Crashes in original library |

---

## Sprint History

### Sprint 1: Boolean Foundation (4 agents)

**Goal**: BooleanOptions, BooleanError, robust predicates, difference().

| Agent | Task | Burndown IDs | Status |
|-------|------|-------------|--------|
| tolerance-architect | `BooleanOptions` struct + layered tolerances | A2 | **Complete** |
| error-engineer | `BooleanError` enum + `Result<>` propagation | A1 | **Complete** |
| robust-predicates | Shewchuk predicates in ray-cast classification | A3 | **Complete** |
| difference-impl | Proper `difference()` in truck-shapeops | B2 (partial) | **Complete** |

**Merge order**: Agent 1 (additive) -> Agent 2 (new errors) -> Agent 3 (behavioral) -> Agent 4 (behavioral)
**Commit**: `69b1fc2` — all 4 agents merged successfully.

---

### Sprint 2: Pipeline Hardening + Edge Selection (4 agents)

**Goal**: Wire edge data pipeline (D1), coplanar face splitting (B1), GeomRef real kernel tests (E1/E6), boolean property tests (E5-partial), verify B4.

| Agent | Task | Burndown IDs | Status |
|-------|------|-------------|--------|
| edge-pipeline | Wire edge data from kernel -> WASM -> viewport | D1 | **Complete** |
| coplanar-architect | Coplanar face splitting (interior-crossing only) | B1 | **Complete** (already implemented) |
| geomref-tester | GeomRef real TruckKernel tests + revolve fix | E1, E6 | **Complete** (3 bugs fixed, 17 tests) |
| property-tester | Boolean algebraic property tests + B4 verify | E5 (partial), B4 | **Complete** |

**Merge order**: Agent 4 (pure additive) -> Agent 1 (new exports) -> Agent 3 (new tests) -> Agent 2 (behavioral)

---

### Sprint 3: Chamfer, Shell, Query Selectors, Revolve Preview, Boolean Fixes

**Commit**: `a2f47f0` — chamfer/shell pipeline (C1, C2), query selectors, revolve preview.

---

### Sprint 4: Fix Truck Coplanar Pipeline

**Goal**: Fix coplanar face handling in the truck-shapeops boolean pipeline to eliminate the eps=0.1 offset hack for boss merges and reduce it for cuts.

**Root causes fixed:**
1. **`weld_coincident_edges` hardcoded tolerance** — Vertex unification used `TOLERANCE.sqrt()` (~0.001) instead of the adaptive `tol` parameter. Coplanar face vertices separated by more than 0.001 but less than `tol` weren't unified.
2. **`check_coplanar_faces` false positives** — Faces on the same plane but with no area overlap (only touching at a line/point) were falsely flagged as coplanar, causing classification errors.
3. **`ray_cast_classify` parity bug** — `majority_vote` used `c >= 1` to determine inside/outside, but shells from `try_attach_plane+tsweep` can produce count=2 (even crossings) for outside points. Fixed by using parity: `c.unsigned_abs() % 2 == 1`.
4. **`classify_coplanar_fragment` anti-sense shortcut** — Returning Or for ALL non-overlapping coplanar faces was wrong for inverted solids. Fixed to only shortcut for same-sense (parallel normal) cases.

**Changes:**
- `vendor/truck/truck-shapeops/`: 5 files modified (coplanar.rs, coplanar_splitting.rs, integrate/mod.rs, integrate/tests.rs, loops_store/mod.rs)
- `crates/feature-engine/src/rebuild.rs`: Removed eps for boss merges, reduced to 0.1 for cuts
- `crates/test-harness/tests/`: Un-ignored 2 tests (d3, rect_cut_at_box_boundary), added 3 new coplanar verification tests (g1-g3)

**Tests un-ignored:** `d3_partially_overlapping_coplanar_rects`, `rect_cut_at_box_boundary`

| Burndown ID | Status |
|-------------|--------|
| B1 (coplanar split) | **Hardened** — bounding-box overlap check, parity ray-cast |
| B3 (box-cylinder) | Partial — still needs cylinder-box coplanar support |

---

### Sprint 5: Boolean Pipeline Remediation

**Goal**: Ray-cast parity consistency, BooleanOptions wiring, weld tolerance, polygon overlap for non-convex faces, algebraic property tests.

**Commit**: `1c0ca3c` + truck `adf4f48a`

**Key changes:**
- Ray-cast parity consistency across all 3 call sites
- `BooleanOptions` wired into kernel
- `weld_coincident_edges` accepts explicit `tol` parameter
- Polygon overlap check for non-convex faces
- 4 H-tests (algebraic properties)
- Un-ignored `a2` + `e2`

**Tests un-ignored:** `a2_stacked_non_overlapping_bosses`, `e2_large_radius_boss_near_edge`

---

### Sprint 5.5: Truck Upstream Port + Regression Fixes

**Goal**: Port boolean improvements to a fresh branch based on `upstream/master`, fixing regressions introduced by the upstream rebase.

**Commits**: `86389f5`, `22b1837`, `6296955`

**Key changes:**
- Ported all waffle boolean improvements (6 commits) onto `upstream/master` as `waffle-upstream-port` branch
- Fixed truck API changes: `rsweep`/`cone` now require `division: usize` parameter
- Fixed `Leader` type elimination — IC leader is now `Box<Curve>`
- Fixed `IntersectionCurve<C, S0, S1>` 3-type-param change
- Fixed `altshell_to_shell` cubic approximation with quadratic fallback
- Fixed `weld_coincident_edges` using `Face::try_new` for graceful fallback
- Rebuilt WASM bundle, updated all downstream crates

**Tests un-ignored:** None (stabilization sprint)
**Net result:** 747 tests pass, 0 failures, 23 ignored

---

### Sprint 6: Chained Boolean Reliability

**Theme:** Fix booleans on IC-healed solids — blocks boss+hole on same part.

**Commit**: `ded1f81`

| Item | Effort | Status |
|------|--------|--------|
| Analytical NURBS arc healing for plane-curved IC edges | M | **Complete** — 3-point circle fit + TrimmedCurve<UnitCircle> NURBS arc, machine-precision residual |
| Multi-strategy BSpline healing with surface residual validation | S | **Complete** — 3-strategy pipeline with best-candidate fallback |
| Thread `tau_weld` into `weld_coincident_edges` | S | Deferred — not blocking |
| Multiple non-overlapping cuts (e5) | M | **Workaround** — e5 rewritten to use `extrude_directed_no_merge` + explicit `boolean_subtract` instead of `extrude_cut`. Root cause (extrude_cut auto-merge path) not fixed. |

**Approach:** The root cause was that BSpline approximations of plane-cylinder IntersectionCurve edges accumulated ~5e-6 error, exceeding truck's TOLERANCE=1e-6 for `curve_surface_projection` convergence. The fix: fit an exact rational NURBS circular arc through sampled leader BSpline points using truck's `TrimmedCurve<UnitCircle> -> NurbsCurve<Vector4>` path + `Matrix4` transform. The arc has zero approximation error on both IC surfaces.

**Tests un-ignored (4):** `test_healed_solid_supports_chained_boolean`, `a4_two_bosses_same_face_sequential`, `e3_boss_at_cube_edge`, `e4_cut_depth_exactly_solid_height`

**Key files:** `crates/kernel-fork/src/healing.rs` — `analytical_circle_arc_from_leader()`, `fit_circle_3points()`, `build_nurbs_arc()`

---

### Sprint 7 (actual): Partial Edge-Coincidence Fixes

**Theme:** Planned as full edge-coincidence + boundary fix sprint, but only some items completed.

| Planned Item | Status |
|-------------|--------|
| Edge-coincidence detection + micro-perturbation in loops_store | **WIP** — unstaged changes in vendor/truck (mutual containment fix: `&& !i_in_j` / `&& !j_in_i` guards in loops_store/mod.rs) |
| Full-face coplanar cut fast path (g3) | **WIP** — 3 new test cases in integrate/tests.rs (full_face_rect_difference, mutual_containment_coplanar); g3 still failing |
| Thread `tau_coplanar` into coplanar detection | Not done |
| Multiple non-overlapping cuts (e5) | **Workaround** — e5 rewritten as F5 (non-ignored), uses explicit boolean_subtract |
| `auto_union_stress_various_positions` | **Fixed** — un-ignored and passing (7/7 auto_union tests) |

**Tests un-ignored:** `auto_union_stress_various_positions` (now passing), `e5` (rewritten as F5, no longer ignored)
**Uncommitted WIP:** vendor/truck changes for g3 mutual containment fix

---

### Sprint 8 (actual): Auto-save, Error UX, and UI Dialogs

**Commit**: `69385ba`

**Theme:** UI/UX sprint — NOT the boolean Sprint 8 originally planned in the forward roadmap.

**Key changes:**
- Auto-save functionality
- Error UX improvements
- FilletDialog.svelte, ChamferDialog.svelte, ShellDialog.svelte UI shells (with "not yet supported" warnings where kernel wiring is missing)
- WASM rebuild

**Burndown impact:**
- D2/D3/D4: UI shell components now exist (were "Not started")
- No boolean pipeline changes

---

## Forward Roadmap

### Next Sprint: Wire Existing Kernel Ops to UI + Fix Stale Ignores

**Theme:** Low-hanging fruit — connect already-implemented kernel ops to their dialogs and un-ignore stale tests.

| Item | Effort | Burndown IDs |
|------|--------|-------------|
| Un-ignore `test_truck_chamfer` + `test_truck_shell` (stale annotations) | S | C1, C2 |
| Wire ChamferDialog to TruckKernel chamfer | S | D3 |
| Wire ShellDialog to TruckKernel shell | S | D4 |
| Wire `BooleanOptions` through truck-shapeops functions | M | A5 |
| Fix `Face::new` at integrate/mod.rs:566 | S | B6 |

---

### Future Sprint: Edge-Coincidence & Boundary Fixes

**Theme:** Fix booleans where tool touches target edges/boundaries.

| Item | Effort | Files |
|------|--------|-------|
| Commit WIP mutual containment fix + g3 tests | S | `vendor/truck/.../loops_store/mod.rs`, `integrate/tests.rs` |
| Full-face coplanar cut fast path (g3) | M | `vendor/truck/.../integrate/mod.rs`, `coplanar_splitting.rs` |
| Thread `tau_coplanar` into coplanar detection | S | `vendor/truck/.../coplanar_splitting.rs` |
| Bridge `BooleanStageError` -> `BooleanError` | M | kernel-fork/types.rs, truck-shapeops/integrate |

**Tests to un-ignore:** `g3_full_face_rect_cut`, `rect_cut_coplanar_edges`

---

### Future Sprint: Circle-at-Boundary + Fillet

**Theme:** Fix remaining edge cases and implement TruckKernel fillet.

| Item | Effort | Files |
|------|--------|-------|
| Circle tangent/crossing box edge fixes | L | `vendor/truck/.../polyline_construction/`, intersection_curve |
| Angled extrude direction support | M | feature-engine extrude path |
| Boolean-based fillet implementation (wire truck-shapeops fillet into TruckKernel) | L | `crates/kernel-fork/src/truck_kernel.rs` |

**Tests to un-ignore:** `circle_cut_tangent_to_box_edge`, `circle_cut_crossing_box_edge`, `cut_from_angled_direction`, `test_truck_fillet`

---

### Deprioritized (do when needed)

- XOR operation (trivial to add, no user demand yet)
- `RelationToOther` refactor (A4) — current AND/OR/Unknown works for all 4 ops
- `TouchingPolicy` (B5) — no current test failures
- Per-edge `tau_local` (E3) — invasive, no current test failures
- 2D polygon overlay library — current approach works
- Non-manifold escape hatch — truck type system makes this hard
- Fuzz tests, stage benchmarks — nice-to-have
- TruckKernel chamfer via real chamfer surfaces (C1 currently uses boolean subtraction)
- Activate dead robust predicates (`robust_orient3d`, `robust_ray_triangle_cross`) in main ray-cast pipeline
