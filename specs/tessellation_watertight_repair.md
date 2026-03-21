# Tessellation Watertight Repair — Spec

**FIP Phase 1 — Specification**
**Date**: 2026-03-21
**Author**: auto-waffle (Manager)

## Goal

Eliminate unpaired edges from the tessellation output of boolean operations,
producing watertight manifold meshes for all assay corpus cases that currently
pass volume/topology checks but fail the watertight oracle.

## Parameters

| Parameter | Default | Range | Unit |
|-----------|---------|-------|------|
| `weld_passes` | 4 | 1–8 | count |
| `weld_scale_factors` | [2, 5, 10, 20] | each ≥1 | multiples of grid |
| `gap_bridge_threshold` | 3 | 1–10 | grid cells |
| `nm_removal_fillable_limit` | 32 | 4–128 | max cycle edges |

## Branch Table

| # | Condition | Behavior |
|---|-----------|----------|
| B1 | All edges paired after initial stitching | No repair needed, return immediately |
| B2 | Boundary-only unpaired edges (count=1) | Progressive weld → gap-bridge → fill holes |
| B3 | Non-manifold-only edges (count≥3) | Conservative removal → two-phase removal with fill |
| B4 | Mixed boundary + non-manifold | Full pipeline: weld → bridge → fill → nm-remove → fill |
| B5 | Repair exhausted, unpaired remain | Log diagnostic, return best-effort mesh |

## Invariants

1. **Volume preservation**: Signed volume of repaired mesh must be within 1% of
   pre-repair volume (excluding degenerate-face removal).
2. **No winding corruption**: Consistent normals percentage must not decrease.
3. **Triangle count monotonicity**: Repair may add triangles (fill) or remove
   (degenerate/nm-removal), but net change must be documented.
4. **Idempotency**: Running repair twice produces the same mesh as running once.
5. **Convergence**: The convergence loop must terminate (unpaired count strictly
   decreases or plateaus → exit).

## Oracles

1. **Watertight mesh**: Every triangle edge shared by exactly 2 triangles
   (quantized position-based matching at 1e-5 relative scale).
2. **Manifold check**: No edge shared by >2 triangles.
3. **Positive signed volume**: Volume > 0 after repair.
4. **Face range coverage**: All triangles assigned to a face range.

## Failure Modes

1. **Gap too wide**: If boundary chain endpoints are >gap_bridge_threshold apart,
   bridging fails. Mesh returned with remaining unpaired edges.
2. **Cycle too long**: If nm-removal creates boundary cycle >nm_removal_fillable_limit,
   the removal is reverted. Non-manifold edges remain.
3. **Volume inversion**: If repair creates negative volume, revert to pre-repair mesh.

## Research Basis

- [#7] Jacobson et al. (2013) — Winding number classification for inside/outside
- [#33] Stroud — B-Rep topology validation
- Mantyla — Euler formula for manifold solids (V-E+F=2)
- General mesh repair literature: vertex welding, hole filling, non-manifold removal

## Method Justification

This is a **tessellation post-processing** improvement, not a geometry operation.
It does not affect B-Rep topology or analytical surface intersections (A15 preserved).
The changes are in the mesh output pipeline only.
