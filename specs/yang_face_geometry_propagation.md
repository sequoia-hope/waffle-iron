# Spec: Yang face_geometry Propagation Fix

## Goal

Ensure every face in a Yang boolean result has a `SurfaceGeom` entry in `face_geometry`,
enabling chained booleans where a Yang result becomes an input operand.

## Root Cause

`result_topology_to_waffle_solid()` in `yang_integration.rs:190-197` populates
`face_geometry` by looking up each result face's `(source.mesh_id, source.face_idx)`
in `surface_map`. Faces whose source is not found are silently skipped. On chained
booleans (A+B→C, then C+D→E), result C has incomplete `face_geometry`, so the second
Yang boolean fails with "one or both solids missing face_geometry" at the guard on
line 535.

This affects **50/190** assay cases with `YANG_BOOLEAN=1`.

## Parameters

None — this is a bug fix with no new user-facing parameters.

## Branch Table

| Source in surface_map? | Face has ≥3 vertices? | Newell normal length ≥ TAU_NORMALIZE? | Action |
|---|---|---|---|
| Yes | N/A | N/A | Use source geometry (existing behavior) |
| No | Yes | Yes | Compute Planar via Newell normal + centroid |
| No | Yes | No | Skip (degenerate zero-area face) |
| No | No | N/A | Skip (degenerate face with <3 vertices) |

## Invariants

1. `face_geometry.len() == face_provenance.len()` — every result face has geometry
   (excluding degenerate faces, which should be rare in valid topology)
2. Fallback Planar normal is consistent with Newell normal of face vertices
   (dot product > 0.99 for well-conditioned faces)
3. Chained boolean `yang_boolean_inner(yang_boolean_inner(A, B, op), C, op)` must
   not error with "missing face_geometry"

## Oracles

- **Face count equality**: `face_geometry.len() == face_map.len()` after any Yang boolean
- **Normal consistency**: For each fallback Planar entry, Newell normal of face vertices
  dot the stored normal ≥ 0.99
- **Chained boolean success**: 3-box chained union produces non-empty valid topology

## Failure Modes

- Degenerate faces (zero-area triangles from subdivision) may not get geometry — this is
  acceptable as they should not survive topology validation
- Curved surfaces (cylinder, cone) get Planar approximation instead of true analytical
  geometry — acceptable for topology extraction; SSI refinement (Step 6) can upgrade
  these when solvers exist

## Research Basis

- Newell normal: standard polygon normal computation [#33 Stroud, Ch. 4]
- Used throughout codebase: `boolean/mod.rs:426`, `vecmath.rs:61`
- Fallback pattern matches `extract_face_polys()` at `boolean/mod.rs:421-431`
