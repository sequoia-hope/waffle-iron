# D1 Pave Block Integration — Phase 2+3 (Shadow Promotion)

**Type:** Refactor (DoD §3) — no intended behavior change.
**Sprint:** 48
**Depends on:** Phase 1 (pave_block.rs types, interference.rs crossing computation, CanonicalVertexMap)
**Status:** Shadow mode complete. Active wire replacement deferred (see Design Notes).

## Goal

Replace tolerance-dependent vertex insertion (`add_polygon_vertex` → `search_parameter` → `Inner(t)`)
with topology-driven edge splitting via pave blocks. Pre-compute where ICs cross boundary edges,
split edges at those parameters, and reconstruct face wires from the pre-split pieces.

This eliminates the root cause of figure-8 wires (MV3 bug) and vertex misalignment at triple points.

**Current state:** Shadow mode runs the full promotion pipeline (sub-edge creation, vertex rebinding,
wire reconstruction) but does NOT replace wires. Counts would-promote vs would-fallback for
observability. Active promotion requires restructuring the IC loop to separate vertex insertion
from edge weaving (see Design Notes).

## Parameters

| Parameter | Value | Source |
|-----------|-------|--------|
| `tol` | Model tolerance (typically 0.05) | `BooleanTolerance.tau_model` |
| `TOLERANCE` | `truck-topology` constant (~1e-7) | Used by `Edge::cut_with_parameter` |
| `canon_tol` | `tol * 0.01` | CanonicalVertexMap grid cell size |
| `snap_tol` | `tol * 0.1` | Corner-touch snap radius (INV-A1) |

## Branch Table — `build_sub_edges`

| # | Condition | Action | Test |
|---|-----------|--------|------|
| B1 | Edge has 0 crossings | Single full-span pave block, `sub_edge = None` | `zero_crossings_full_span` (existing) |
| B2 | Edge has N>0 interior crossings | N+1 pave blocks, each with `sub_edge` via `cut_with_parameter` | `sub_edge_created_for_interior_crossing` |
| B3 | Crossing is corner-touch at existing vertex | Reuse existing vertex, pave block boundary aligns, `sub_edge = None` | `corner_touch_no_sub_edge` |
| B4 | `cut_with_parameter` returns `None` (degenerate) | Skip sub-edge, mark pave block as fallback | `degenerate_cut_fallback` |

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

## Oracles

- **O1:** `cargo test -p truck-shapeops` — 329+ pass, 1 pre-existing fillet fail
- **O2:** `cargo test -p test-harness` — 400+ pass
- **O3:** Shadow mode match rate > 0 (validates pave block wires match legacy wires)
- **O4:** Promotion counters: `pave_wire_promotion_stats()` shows would-promote > 0 across test suite

## Failure Modes

| Mode | Detection | Mitigation |
|------|-----------|------------|
| `cut_with_parameter` returns None | B4 branch, test coverage | Fall back to legacy path |
| Non-closed reconstructed wire | W3 branch, `front == back` check | Fall back to legacy path |
| Poly/geom store divergence | Only modify geom store | Poly store unchanged |
| Regression in existing test | INV-D5, CI | Shadow mode gates promotion |

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
| `vendor/truck/.../pave_block.rs` | Add `build_sub_edges()`, `reconstruct_boundary_wires()` methods |
| `vendor/truck/.../interference.rs` | Add `rebind_pave_block_vertices()`, `find_closest_canonical()` |
| `vendor/truck/.../interference/tests.rs` | Add 2 rebinding tests (28 total) |
| `vendor/truck/.../loops_store/mod.rs` | Shadow promotion pass, promotion counters, canonical vertex collection |
| `vendor/truck/.../mod.rs` | Export `pave_wire_promotion_stats`, `pave_wire_stats` |
