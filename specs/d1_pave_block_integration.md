# D1 Pave Block Integration — Phase 4 (Active Pave Block Promotion)

**Type:** Refactor (DoD §3) — no intended behavior change.
**Sprint:** 48
**Depends on:** Phase 3 (two-pass IC loop, edge pre-splitting)
**Status:** INFRASTRUCTURE COMPLETE — Phases 1-5 done, Phase 6 confirmed analytical crossings don't align with mesh path. Realignment needed (Phase 8).

## Goal

Replace tolerance-dependent vertex insertion (`add_polygon_vertex` → `search_parameter` → `Inner(t)`)
with topology-driven edge splitting via pave blocks. Pre-compute where ICs cross boundary edges,
split edges at those parameters, and reconstruct face wires from the pre-split pieces.

This eliminates the root cause of figure-8 wires (MV3 bug) and vertex misalignment at triple points.

**Current state:** Phase 4 adds pre-split effectiveness instrumentation (HIT/MISS counters in
`add_polygon_vertex`), fixes `build_sub_edges()` to use curve projection matching Phase 3's
proven approach, and validates sub-edge accuracy via shadow-mode endpoint comparison. The
`build_sub_edges` fix changes the trait bounds to require `SearchNearestParameter<D1>` and
`BoundedCurve` (matching `split_geom_edge_at_crossings`).

## Parameters

| Parameter | Value | Source |
|-----------|-------|--------|
| `tol` | Model tolerance (typically 0.05) | `BooleanTolerance.tau_model` |
| `TOLERANCE` | `truck-topology` constant (~1e-7) | Used by `Edge::cut_with_parameter` |
| `canon_tol` | `tol * 0.01` | CanonicalVertexMap grid cell size |
| `snap_tol` | `tol * 0.1` | Corner-touch snap radius (INV-A1) |

## Branch Table — `build_sub_edges` (Phase 4 — curve projection)

| # | Condition | Action | Test |
|---|-----------|--------|------|
| B1 | Edge has 0 crossings | Single full-span pave block, `sub_edge = None` | `zero_crossings_full_span` (existing) |
| B2 | Crossing projects to interior of original curve | Include in cut params | `sub_edge_created_for_interior_crossing` |
| B3 | Crossing projects to boundary of original curve | Skip (corner-touch) | `corner_touch_no_sub_edge` |
| B4 | Forward edge (orientation=true) | Process low-to-high, keep right | `sub_edge_preserves_contiguity` |
| B5 | Reversed edge (orientation=false) | Process low-to-high, keep left | (covered by B4 symmetry) |
| B6 | `cut_with_parameter` fails | Mark block as fallback, return false | `degenerate_cut_fallback` |

## Branch Table — Pre-Split Hit/Miss Instrumentation (Phase 4)

| # | Condition | Action |
|---|-----------|--------|
| H1 | `search_parameter` returns Front | Increment PAVE_PRESPLIT_HIT |
| H2 | `search_parameter` returns Back | Increment PAVE_PRESPLIT_HIT |
| H3 | `search_parameter` returns Inner(t) | Increment PAVE_PRESPLIT_MISS |
| H4 | `search_parameter` returns None | No counter change (edge not found) |

## Branch Table — `reconstruct_boundary_wires`

| # | Condition | Action | Test |
|---|-----------|--------|------|
| W1 | All edges have sub-edges or are full-span | Reconstruct wire from sub-edges | `wire_reconstruction_simple_split` |
| W2 | Some edge missing sub-edge (fallback) | Return `None`, caller uses legacy path | `wire_reconstruction_fallback_on_invalid` |
| W3 | Reconstructed wire not closed | Return `None`, fall back | `wire_reconstruction_fallback_on_invalid` |
| W4 | No crossings on any edge | Return original wire unchanged | `interference_to_wires_no_crossings` (existing) |

## Invariants

- **INV-D1:** Every pave block sub-edge's curve evaluates to its vertex positions at endpoints
- **INV-D2:** Pave block param_ranges are contiguous and non-overlapping per edge
- **INV-D3:** Reconstructed wires are closed (front vertex == back vertex)
- **INV-D4:** Total edge count across pave blocks for one original edge = num_crossings + 1
- **INV-D5:** All existing tests pass unchanged (refactor invariant)
- **INV-H1:** Instrumentation does not change any vertex or edge state
- **INV-H2:** HIT + MISS = total `add_polygon_vertex` calls (no undercounting)
- **INV-B1:** Fixed `build_sub_edges` produces same sub-edge topology as Phase 3's `split_geom_edge_at_crossings`
- **INV-B2:** Shadow mode sub-edge validation shows match > 0 for tests with pre-split edges

## Oracles

- **O1:** `cargo test -p truck-shapeops` — 333+ pass, 1 pre-existing fillet fail
- **O2:** `cargo test -p test-harness` — 400+ pass (excl. pre-existing assay)
- **O3:** PRESPLIT_HIT > 0 for tests with IC crossings on boundary edges
- **O4:** Shadow mode sub-edge validation: match > 0, diverge = 0 for `punched_cube`
- **O5:** `cargo clippy -p truck-shapeops` — no new warnings

## Failure Modes

| Mode | Detection | Mitigation |
|------|-----------|------------|
| `cut_with_parameter` returns None | B6 branch, test coverage | Fall back to legacy path |
| Non-closed reconstructed wire | W3 branch, `front == back` check | Fall back to legacy path |
| Poly/geom store divergence | Only modify geom store | Poly store unchanged |
| Regression in existing test | INV-D5, CI | Shadow mode gates promotion |
| `build_sub_edges` trait bound mismatch | Compile-time error | Same bounds as `split_geom_edge_at_crossings` |

## Design Notes — Why Active Promotion Is Deferred

The IC loop in `create_loops_stores` interleaves two operations per face pair:
1. `add_geom_vertex` / `add_polygon_vertex` — splits boundary edges at IC crossings
2. `add_edge` — weaves IC edges into boundary wires (rotate, push, split_off)

Both use **global** operations (`change_vertex`, `swap_edge_into_wire`) that affect ALL faces
in the LoopsStore, not just the current face. Deferring `add_edge` until after a promotion pass
changes the wire structure seen by subsequent `add_geom_vertex` calls, causing test failures.

**To enable active promotion**, the IC loop needs restructuring:
- Separate vertex insertion from edge weaving into distinct passes
- OR: run promotion BEFORE add_edge but AFTER add_geom_vertex per face pair (requires
  proving that add_edge for face pair (i,j) doesn't affect add_geom_vertex for pair (i+1,j+1))

### Branch Table — `rebind_pave_block_vertices`

| # | Condition | Action | Test |
|---|-----------|--------|------|
| R1 | Sub-edge endpoint matches canonical vertex (< tol) | Replace with canonical vertex | `rebind_replaces_crossing_vertex` |
| R2 | Sub-edge endpoint has no canonical match | Keep original (boundary vertex) | `rebind_preserves_boundary_vertex` |
| R3 | Multiple canonical vertices match | Use closest match | Covered by R1 |

## Files Modified

| File | Change |
|------|--------|
| `vendor/truck/.../pave_block.rs` | `build_sub_edges()` rewritten to use `search_nearest_parameter` (curve projection) |
| `vendor/truck/.../interference.rs` | `reconstruct_boundary_wires()` debug logging for non-closed wire |
| `vendor/truck/.../interference/tests.rs` | Existing tests (28 total) |
| `vendor/truck/.../loops_store/mod.rs` | PRESPLIT_HIT/MISS counters, `pave_presplit_hit_miss()` stats fn, sub-edge validation in shadow mode |
| `vendor/truck/.../mod.rs` | Export `pave_presplit_hit_miss` |

## Phase 3 — Two-Pass IC Loop + Pre-Splitting

### Branch Table — `split_geom_edge_at_crossings` / `split_poly_edge_at_positions`

| # | Condition | Action |
|---|-----------|--------|
| S1 | Crossing projects to interior of original curve | Include in split params |
| S2 | Crossing projects to boundary of original curve | Skip entire edge (return None) |
| S3 | Forward edge (orientation=true) | Process lowest-to-highest, keep right sub-edge |
| S4 | Reversed edge (orientation=false) | Process lowest-to-highest, keep left sub-edge |
| S5 | `cut_with_parameter` fails | Return None, skip edge |

### Branch Table — `presplit_one_shell`

| # | Condition | Action |
|---|-----------|--------|
| P1 | Edge has 0 crossings in table | Skip |
| P2 | Edge has >3 unique interior crossings | Skip (likely coplanar artifact) |
| P3 | Duplicate 3D positions from opposite-orientation face pairs | Deduplicate by position proximity |
| P4 | Geom or poly split fails | Skip edge, continue with next |
| P5 | Sub-wire edge counts don't match between geom/poly | Skip edge |
| P6 | Split succeeds | `swap_edge_into_wire` for both geom and poly loops stores |

## Phase 5 — Pave Block Active Promotion

**Status:** Active promotion — `D1_SHADOW_MODE=false`, promoted path live.
**Depends on:** Phase 4 (instrumentation), IC loop restructuring (f65b24d).

### Goal

Leverage pre-splitting to skip legacy vertex insertion when all IC endpoints
are at pre-split sub-edge boundaries. The promoted path uses cached
`(wire_index, edge_index, kind)` from `is_presplit_hit` to call
`change_vertex`/`set_point` directly, avoiding redundant `search_parameter`
calls in `add_polygon_vertex`/`add_geom_vertex`.

### Strategy

1. ~~**Shadow mode** (D1_SHADOW_MODE=true, initial): Run both paths, log stats, always use legacy result.~~ DONE
2. **Per-endpoint mixed promotion** (D1_SHADOW_MODE removed): Each endpoint independently promoted or legacy. **ACTIVE**
3. **Full promotion** (future): Remove legacy path entirely.

### Branch Table — `try_pave_block_vertex_insertion` (shadow/promote adapter)

| # | Condition | Action |
|---|-----------|--------|
| P5-1 | Face has 0 IC crossings in face interference table | Skip pave-block path, use legacy |
| P5-2 | All IC endpoints hit pre-split sub-edge endpoints (all Front/Back) | Pave-block path viable, increment SUCCESS |
| P5-3 | Some IC endpoints require Inner(t) cuts (any PRESPLIT_MISS) | Fall back to legacy path, increment FALLBACK |
| P5-4 | `reconstruct_boundary_wires` returns None (non-closed wire) | Fall back to legacy path (future phase) |
| P5-5 | Shadow mode: pave-block result diverges from legacy | Log, use legacy (future comparison) |
| P5-6 | Shadow mode: pave-block result matches legacy | Log match, use legacy (shadow) |

### Branch Table — `build_face_interference_from_ics`

| # | Condition | Action |
|---|-----------|--------|
| F1 | IC polyline touches face (shell_index selects face_index) | Compute crossings on face boundary |
| F2 | Face index out of bounds | Skip IC for this face |
| F3 | Empty ic_accumulator | Return empty FaceInterference per face |

### Branch Table — `is_presplit_hit`

| # | Condition | Action |
|---|-----------|--------|
| H5-1 | `search_parameter` returns Front | Return `Some((wi, ei, Front))` |
| H5-2 | `search_parameter` returns Back | Return `Some((wi, ei, Back))` |
| H5-3 | `search_parameter` returns Inner(t) | Return `None` (miss, needs cut) |
| H5-4 | `search_parameter` returns None | Return `None` (not on boundary) |

### Branch Table — Per-Endpoint Mixed Promotion (Phase 6)

| # | Condition | Action |
|---|-----------|--------|
| PR-1 | `is_presplit_hit` returns `Some((wi, ei, Front))` | Promoted: `change_vertex(absolute_front, pv)` |
| PR-2 | `is_presplit_hit` returns `Some((wi, ei, Back))` | Promoted: `change_vertex(absolute_back, pv)` |
| PR-3 | `is_presplit_hit` returns `None` | Legacy: `add_polygon_vertex` + `add_geom_vertex` |
| PR-4 | Geom store promoted vertex | `gv.set_point(old_gv.point())` before `change_vertex` |
| PR-5 | IC leader endpoints | Updated to `gv0.point()`/`gv1.point()` after geom change_vertex |
| PR-6 | promoted_count == 4 | Increment SUCCESS |
| PR-7 | promoted_count == 0 | Increment FALLBACK |
| PR-8 | promoted_count 1-3 | Increment MIXED |

### Phase 5 Invariants

- **INV-P1:** ~~Shadow mode never changes pipeline output~~ Promoted path produces identical `change_vertex`/`set_point` calls as legacy Front/Back path
- **INV-P2:** Per-face fallback guarantees legacy behavior when pave-block fails (any endpoint is Inner/None)
- **INV-P3:** Counters are append-only (no pipeline state mutation)
- **INV-P4:** PRESPLIT_HIT count increases vs Phase 4 baseline
- **INV-P5:** `build_face_interference_from_ics` produces crossing counts consistent with pre-split hit/miss
- **INV-P6:** Promoted path never splits edges (no `cut_with_parameter`), only renames vertices

### Phase 5 Counters

| Counter | Meaning |
|---------|---------|
| `PAVE_PROMOTE_SUCCESS` | ICs where all 4 endpoints were promoted (pre-split hits) |
| `PAVE_PROMOTE_FALLBACK` | ICs where 0 endpoints were promoted (all legacy) |
| `PAVE_PROMOTE_MIXED` | ICs where 1-3 endpoints were promoted (mixed promoted/legacy) |
| `PAVE_PROMOTE_ENDPOINT` | Total individual endpoints promoted across all ICs |

### Phase 5 Failure Modes

| Mode | Detection | Mitigation |
|------|-----------|------------|
| `is_presplit_hit` returns false when pre-split succeeded | FALLBACK counter > expected | Investigate snap tolerance alignment |
| `build_face_interference_from_ics` misses crossings | Face interference < pre-split table crossings | Compare with global crossing table |
| Performance regression from face_interf building | Profiling | Skip face_interf when ic_accumulator is empty |
| Shadow mode adds overhead | Negligible (4 search_parameter calls per IC) | Remove shadow checks after validation |

### Phase 5 Files Modified

| File | Change |
|------|--------|
| `vendor/truck/.../interference.rs` | `build_face_interference_from_ics` function |
| `vendor/truck/.../loops_store/mod.rs` | `is_presplit_hit`, counters, stats fn, shadow mode wiring in pass 2a |
| `vendor/truck/.../mod.rs` | Export `pave_promote_stats` |
| `specs/d1_pave_block_integration.md` | This Phase 5 section |

## Phase 6 — All-or-Nothing Promotion (Sprint 49b)

**Status:** COMPLETE (2026-03-04)
**Commit:** 729553d

### Problem

Phase 5's per-endpoint mixed promotion allowed 1-3 of 4 IC endpoints to be promoted while others used legacy `add_polygon_vertex`. This produced inconsistent wire topology — promoted endpoints used `change_vertex` (rename existing vertex) while legacy endpoints used `search_parameter` → `Inner(t)` → edge split. Mixed paths created wires where some edges were split and others weren't, leading to topology mismatches.

### Solution

Restructured promotion to check-first, execute-second:

1. **Read-only check phase:** For each IC, check all 4 endpoints via `is_presplit_hit`. Count promoted vs legacy.
2. **Decision:** If all 4 endpoints are pre-split hits → execute all-promoted path. If any endpoint is a miss → execute all-legacy path. No mixed execution.
3. **Counters updated:** SUCCESS (all 4 promoted), FALLBACK (all 4 legacy), removed MIXED counter.

### Result

0% full promotion across entire test suite (30/30 ICs are FALLBACK). This confirmed that analytical crossings fundamentally don't align with mesh-derived IC endpoints — the all-or-nothing gate simply made the 0% promotion rate visible rather than hiding it behind mixed promotion noise.

### Branch Table

| # | Condition | Action |
|---|-----------|--------|
| AO-1 | All 4 endpoints are pre-split hits | Execute promoted path for all 4 |
| AO-2 | Any endpoint is a pre-split miss | Execute legacy path for all 4 |
| AO-3 | 0 ICs in face interference table | Skip pave-block check entirely |

## Phase 7 — Post-Mortem: Why Analytical Crossings Failed

**Date:** 2026-03-04

### Root Cause

`compute_ic_edge_crossings()` in `interference.rs` uses analytical segment-segment closest-approach to find where IC polylines cross face boundary edges. This produces crossing positions on a **fundamentally different geometric path** than the mesh-derived IC endpoints used by `add_polygon_vertex`.

**Mesh IC path** (legacy, what actually works):
```
extract_interference (mesh triangles) → polyline points
  → search_triple (Newton refinement on both surfaces)
  → 3D IC curve points
  → search_parameter on face boundary edge
  → Inner(t) / Front / Back classification
```

**Analytical crossing path** (D1, what failed):
```
IC polyline segments × boundary edge segments
  → 2D/3D closest-approach computation
  → crossing position (typically 0.01-0.1 units off from mesh path)
  → search_parameter on face boundary edge
  → Different t-value than mesh path → different vertex position
```

### Why They Diverge

1. **Newton refinement vs closest-approach:** The mesh path refines each IC point onto both surfaces via Newton iteration. The analytical path uses raw polyline geometry without surface refinement.

2. **Different questions:** The mesh path asks "where does this IC point land on the boundary edge's parameter space?" The analytical path asks "where do these two line segments come closest in 3D?"

3. **Tolerance interaction:** Even when both paths find a crossing at approximately the same position, the resulting `search_parameter` t-values differ enough that `is_presplit_hit` (which checks for exact Front/Back match) never triggers.

### Conclusion

D1's analytical crossings approach is **architecturally wrong** — it tries to re-derive information that already exists in the mesh IC pipeline. The fix is not to improve the analytical computation but to **capture the crossing information from the mesh path** during `create_loops_stores`.

## Phase 8 — Proposed Realignment

**Status:** PLANNED (not yet implemented)

### Strategy

Instead of computing crossings analytically in `interference.rs`, capture them from the mesh IC pipeline during `create_loops_stores`:

1. When `add_polygon_vertex` calls `search_parameter` and gets `Inner(t)`, this IS the crossing information D1 needs.
2. Record `(face_index, wire_index, edge_index, parameter_t, vertex_position)` into the `InterferenceTable` during the IC loop.
3. Use this mesh-derived crossing data for pave block construction and pre-splitting.

### Benefits

- Crossings are on the **same geometric path** as the IC curve — guaranteed to match.
- No additional computation — just capturing data that's already computed.
- Pave blocks become a bookkeeping structure for mesh-derived crossings, not an independent geometric computation.
- D2 shrunk ranges can use these mesh-derived pave blocks directly.

### Required Changes

| File | Change |
|------|--------|
| `interference.rs` | Replace `compute_ic_edge_crossings()` with `capture_mesh_crossings()` |
| `loops_store/mod.rs` | Record crossing data during `add_polygon_vertex` |
| `pave_block.rs` | Update `PaveBlock` construction to use mesh-derived data |

### Invariants

- **INV-R1:** Every crossing recorded matches a `search_parameter` → `Inner(t)` call
- **INV-R2:** Pave block vertex positions exactly match mesh IC vertex positions
- **INV-R3:** All existing tests pass unchanged
