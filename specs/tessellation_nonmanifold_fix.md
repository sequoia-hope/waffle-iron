# Tessellation Non-Manifold Edge Fix

## Goal

Fix the systematic tessellation bug causing 1-2 non-manifold edges (edges shared
by 3+ triangles) in boolean result meshes. This is the #1 assay failure category
(60/93 failing cases cite `watertight_mesh`).

## Problem Description

The fan-path tessellation pipeline (used for boolean results with arc edges and
polygon-soup B-Rep) runs `fill_boundary_holes()` up to 6 times across multiple
passes (lines 388, 398, 408, 431, 439 in tessellation/mod.rs). Each pass detects
"boundary edges" (directed half-edges with count=1 and no reverse) and fills them
with triangles.

**Root cause**: `fill_boundary_holes` and `close_near_boundary_chains` can add fill
triangles whose edges overlap with already-paired edges in the mesh. When the oracle
(which uses undirected position-based edge matching) sees the same geometric edge
referenced by 3+ triangles, it reports a non-manifold edge.

Specifically:
1. Pass N detects a boundary hole and fills it with triangles
2. The fill triangle shares an edge with an existing triangle pair
3. That edge now has count = 3 → non-manifold

This is exacerbated by:
- `remove_duplicate_triangles` only removing same-winding duplicates (line 3417-3426)
- Multiple fill passes compounding the problem
- `close_near_boundary_chains` adding additional fill triangles

## Parameters

- **Input**: Tessellated mesh from fan-path or bounded-path tessellation of boolean results
- **Tolerance**: TAU_TESS_GRID_FACTOR = 1e-5 (position quantization grid)

## Branch Table

| Scenario | Current Behavior | Expected Behavior |
|----------|------------------|-------------------|
| Fill triangle shares edge with already-paired geometry | Creates non-manifold edge (count=3) | Skip fill triangle or remove overlapping triangle |
| Opposite-winding duplicate | Not detected by remove_duplicate_triangles | Should be detected and removed |
| Multiple fill passes compound | Each pass may add overlapping fills | Later passes should check for non-manifold edges before adding |

## Invariants

1. After tessellation, every geometric edge (position-based, undirected) must have
   exactly count = 2 (manifold) or count = 1 (boundary, indicating a real hole)
2. No edge may have count >= 3 (non-manifold)
3. Fill triangles must not create edges that are already properly paired

## Oracles

1. `check_watertight_mesh` oracle: 0 unpaired edges (all edges count = 2)
2. No non-manifold edges reported
3. Assay cases R0004, R0021, R0028, R0032, R0035, R0043, R0049, R0061, F0023
   should pass after fix

## Failure Modes

- Fill triangle creates non-manifold edge → detected by oracle, assay case fails
- Over-aggressive duplicate removal → could create boundary edges (count=1)
- Degenerate fill triangles → handled by existing remove_degenerate_triangles pass

## Research Basis

- Ref #16 (Mantyla): Half-edge topology enforces 2-manifold property. Tessellation
  output should maintain this invariant.
- Ref #33 (Stroud Ch.4): B-Rep visualization requires manifold triangle meshes.
- The fix applies standard mesh post-processing: detect and remove non-manifold
  edges caused by overlapping triangles. This is a well-known issue in boolean
  result tessellation (see Cherchi et al. 2020 mesh arrangements).

## Fix Strategy

Add a `remove_nonmanifold_duplicates()` post-processing pass that:
1. Builds position-based undirected edge counts (same method as oracle)
2. Identifies edges with count >= 3
3. For each non-manifold edge, identifies the "extra" triangle(s) — the ones
   that were added by fill passes (identifiable by face_id = KernelId(u64::MAX)
   or by being in later face ranges)
4. Removes the extra triangle(s) to bring edge count back to 2

This pass should run as the final step before `weld_mesh_vertices`, after all
fill and close passes are complete.
