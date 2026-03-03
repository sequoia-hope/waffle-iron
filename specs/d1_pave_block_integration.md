# D1 Pave Block Integration — Phase 4 (Active Pave Block Promotion)

**Type:** Refactor (DoD §3) — no intended behavior change.
**Sprint:** 48
**Depends on:** Phase 3 (two-pass IC loop, edge pre-splitting)
**Status:** Phase 4 complete — instrumentation + `build_sub_edges` curve projection fix.

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
