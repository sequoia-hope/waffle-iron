# Non-Manifold Edge Elimination — Spec

## Goal

Eliminate non-manifold edges (shared by 3+ triangles) from tessellation output
of boolean operations. Currently, 25 of 83 failing assay cases have ≤6 unpaired
edges, almost all non-manifold. Fixing these would increase the assay score from
~73/160 to potentially ~95/160.

## Parameters

- **Input**: `RenderMesh` after all existing repair passes (weld, fill, chain-close,
  conservative non-manifold removal)
- **Mode**: Aggressive non-manifold removal for the fan-path tessellation pipeline
- **Tolerance**: Uses the same quantization grid as existing repair (TAU_TESS_GRID_FACTOR)

## Branch Table

| Path | Current | Proposed |
|------|---------|----------|
| Fan-path (polygon_soup=true) | Conservative non-manifold removal | Aggressive non-manifold removal |
| Bounded-path (polygon_soup=false) | Aggressive non-manifold removal | No change |

## Invariants

1. **Non-manifold-free**: After the fix, no edge should be shared by 3+ triangles
   in any case where the current code produces ≤6 non-manifold edges.
2. **No regression**: Cases that currently pass must continue to pass.
3. **Triangle count preservation**: Total triangle count should not decrease by more
   than 5% (aggressive removal should target only genuinely excess triangles).
4. **Deterministic**: Same inputs must produce same outputs.

## Oracles

- `count_unpaired_edges(mesh)` returns 0 for all tested cases
- `find_nonmanifold_edges(mesh)` returns empty for all tested cases
- Triangle count stays within 5% of pre-fix count

## Failure Modes

- Aggressive removal could create boundary edges (count=1) by over-removing
  triangles. This is acceptable if the net unpaired count decreases.
- Very large meshes (>10k triangles) could have O(N²) cost in edge counting.
  Mitigated by the existing quantization grid.

## Research Basis

- [#33] Stroud Ch.16 — mesh repair and validation
- [#16] Mantyla — half-edge consistency invariants
- The removal algorithm uses position-based vertex matching (same as the
  watertight check oracle) to ensure consistency between production and test.

## Analytical vs. Approximate Method Justification

This is a tessellation post-processing fix, not an SSI operation. No surface
pairs are involved. The fix operates on triangle meshes only.
