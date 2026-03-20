# Tessellation Non-Manifold Edge Repair Improvement

## Goal

Reduce non-manifold edges (shared by 3+ triangles) in the fan-path tessellation
pipeline for boolean result meshes. Currently, the fan path uses conservative
non-manifold removal that preserves non-manifold edges when removing any excess
triangle would create boundary edges. This leaves near-miss cases with 1-200
non-manifold edges in otherwise-correct meshes of thousands of edges.

## Parameters

- **Input**: Fan-path tessellation pipeline output (vertices, normals, indices, face_ranges)
- **Output**: Same mesh with fewer non-manifold edges
- **Tolerance**: Uses the existing oracle quantization grid (max_abs * 1e-5, min 1e-10)

## Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| B1 | Winding-insensitive duplicates exist | Remove second copy (keep first = real face triangle) |
| B2 | Non-manifold edge, conservative removal succeeds | Remove excess triangle (existing behavior) |
| B3 | Non-manifold edge, conservative fails, cluster removal possible | Remove connected cluster of overlapping triangles as a unit |
| B4 | Non-manifold edge persists after all passes | Leave as-is (no hack-to-green) |

## Invariants

1. **No new boundary edges**: Every repair step must not increase the count of boundary (unpaired) edges.
2. **Triangle count non-increasing**: Repair only removes triangles, never adds them.
3. **Winding preservation**: Remaining triangles keep their original winding order.
4. **Face range integrity**: Face ranges are updated consistently after triangle removal.

## Oracles

- **O1**: Winding-insensitive dedup reduces non-manifold count for meshes with fill-vs-real overlap
- **O2**: Conservative removal maintains zero boundary edges when starting from zero boundary
- **O3**: After all passes, non-manifold count ≤ count before improvement (monotonic decrease)
- **O4**: No regression in existing kernel test suite (548 tests)

## Failure Modes

- Fill triangle removal creates boundary edge: prevented by safety check
- Cluster removal removes too many triangles: bounded by checking post-removal edge counts
- Degenerate triangle produced: existing degenerate removal handles this

## Research Basis

- Ref #7: Jacobson et al. — mesh repair via winding number consistency
- Ref #33: Stroud — B-Rep mesh validation and non-manifold edge classification

## Analytical vs. Approximate Method Justification

This is a mesh-level post-processing improvement, not a geometric computation.
It operates on the tessellated output, not on B-Rep topology. No SSI implications.
