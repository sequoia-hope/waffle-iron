# Spec: Pave Block Corner Touch Detection

## Goal

Prevent figure-8 (self-intersecting) wires at face boundary corners by detecting
when an IC endpoint lands within tolerance of an existing boundary vertex and
snapping to the exact vertex position. This ensures `add_polygon_vertex` returns
`Front`/`Back` instead of `Inner(near-boundary-t)`, avoiding near-duplicate vertex
creation that produces non-simple wires.

## Parameters

| Parameter | Description | Source |
|-----------|-------------|--------|
| `ic_endpoint` | 3D position of IC polyline front/back point | `polyline.front()` / `polyline.back()` |
| `boundary_vertex_positions` | All vertex positions from the face's boundary wires | `geom_shell[face_index].absolute_boundaries()` |
| `tau_model` | Model tolerance (corner-touch threshold) | `BooleanTolerance::tau_model` |

## Branch Table

| # | Condition | Action | Result |
|---|-----------|--------|--------|
| 1 | IC endpoint within `tol` of a boundary vertex | Snap endpoint to exact vertex position | `add_polygon_vertex` → `Front`/`Back` |
| 2 | IC endpoint interior to a boundary edge (not near any vertex) | No snap, normal processing | `add_polygon_vertex` → `Inner(t)`, edge split |
| 3 | IC endpoint not near any boundary edge or vertex | No snap, normal processing | `add_polygon_vertex` → `None` or normal result |

## Invariants

- **INV-A1**: All face boundary wires are simple (no repeated vertices except
  closure) after IC vertex insertion.
- **INV-A2**: Euler characteristic χ = V − E + F = 2 for genus-0 inputs.
- **INV-A3**: No near-duplicate vertices (distance < `tol`, different VertexID)
  on any face boundary wire after IC vertex insertion.

## Oracles

| Oracle | Target | Tolerance |
|--------|--------|-----------|
| MV3 χ | 2 | exact |
| MV3 volume | 875.0 | ±1.0 |
| Wire simplicity | All divided face wires are simple | boolean |
| Near-duplicate scan | No vertex pairs within `tol` on same wire | distance check |

## Failure Modes

| Mode | Cause | Detection |
|------|-------|-----------|
| False negative (threshold too tight) | IC endpoint within tol of vertex but snap threshold narrower | χ ≠ 2, wire simplicity check fails |
| False positive (threshold too loose) | Interior IC crossing misidentified as corner touch | Volume oracle fails, regression tests fail |

## Implementation Strategy

1. Add `find_corner_touch_snap(ic_endpoint, boundary_vertices, tol) -> Option<Point3>`
   in `interference.rs`. Returns exact boundary vertex position if within `tol`.

2. Before IC vertex creation (`loops_store/mod.rs` lines 1273-1276), collect
   boundary vertex positions from both faces' geometric boundaries. Call
   `find_corner_touch_snap` for each of the 4 IC endpoints (pv0, pv1 for
   shell0 face; gv0, gv1 for shell1 face — though they share positions, the
   boundary vertices differ per face). Snap the polyline endpoints to exact
   boundary positions before creating Vertex objects.

3. This extends the existing coincident-vertex snapping (lines 1186-1207) to
   also handle face-local boundary vertices that aren't cross-shell coincident.

## Test Plan

| Test | Type | Asserts |
|------|------|---------|
| `mv3_euler_invariant_subtract` (un-ignore) | Integration | χ=2 |
| `corner_touch_reuses_boundary_vertex` | Integration | χ=2, volume≈875 |
| `non_corner_ic_still_splits_edge` | Integration | χ=2 (interior IC endpoints) |
| `corner_touch_detected_at_vertex` | Unit | snap returns Some |
| `interior_crossing_not_corner_touch` | Unit | snap returns None |

## References

- MV3 root cause: MEMORY.md Sprint 42 section
- Pave block infrastructure: `vendor/truck/truck-shapeops/src/transversal/pave_block.rs`
- Interference module: `vendor/truck/truck-shapeops/src/transversal/interference.rs`
- IC vertex creation: `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs:1273-1276`
