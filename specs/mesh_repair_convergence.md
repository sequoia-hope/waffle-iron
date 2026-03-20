# Mesh Repair Convergence Improvement

## Goal

Improve the mesh repair pipeline in `tessellation/mod.rs` to eliminate more
unpaired edges from boolean result meshes. The current pipeline runs non-manifold
removal only once at the end; moving it into the iterative repair loop allows
fill→remove→fill convergence that closes remaining gaps.

Target: reduce assay `watertight_mesh` failures by ≥5 cases.

## Parameters

### Convergence loop iteration limit
- **Current**: 3 iterations of weld+fill (no non-manifold removal in loop)
- **Proposed**: Up to 5 iterations of weld+degenerate+fill+degenerate+nonmanifold+winding
- **Rationale**: Non-manifold removal can expose new boundary edges that the fill
  pass can close. Without iterating, these are left unfixed. 5 iterations is a
  safe upper bound — convergence typically occurs in 2-3 passes.

### Non-manifold removal placement
- **Current**: Single `remove_nonmanifold_duplicates` call after all fills
- **Proposed**: Also run within the iterative loop, plus a final aggressive pass
- **Rationale**: Fill passes create overlapping triangles that produce non-manifold
  edges (3+ triangles sharing an edge). Removing these within the loop exposes
  boundary edges that the next fill iteration can close.

## Branch Table

| Pipeline Stage | Current | Proposed | Effect |
|---------------|---------|----------|--------|
| Iterative loop body | weld→degenerate→fill→degenerate | weld→degenerate→fill→degenerate→nonmanifold→winding | Removes non-manifold within loop |
| Loop iterations | 3 | 5 | More convergence opportunities |
| Final non-manifold | Single pass (safe mode) | Aggressive pass + safe pass | Catches remaining overlaps |
| Post-final fill | None | One more fill+weld after final nonmanifold | Closes gaps from NM removal |

## Invariants

1. **Monotonic convergence**: Total unpaired edge count must not increase across
   iterations. If it does, the loop must stop.
2. **No topology corruption**: Non-manifold removal must not create inverted
   triangles. Winding fix runs after each removal.
3. **Deterministic output**: Same input → same output, regardless of HashMap
   iteration order (use sorted collections or order-independent algorithms).
4. **No regression**: All currently-passing kernel tests must continue to pass.
5. **Bounded runtime**: Maximum 5 iterations prevents infinite loops.

## Oracles

- **Watertight check**: Count unpaired edges (oracle quantization: max_abs × 1e-5).
  Target: 0 unpaired for all geometries that currently have <10 unpaired.
- **Non-manifold check**: Count edges shared by ≥3 triangles. Must not increase
  compared to current pipeline.
- **Assay score**: Run full assay; `watertight_mesh` failures must decrease.
- **Volume sign**: Signed mesh volume must remain positive.
- **Triangle count stability**: Total triangle count should not change by more
  than 5% across the pipeline restructure for any given input.

## Failure Modes

- **Oscillation**: Fill adds triangles that non-manifold removal then removes,
  creating the same boundary edges again. Mitigated by iteration cap (5) and
  monotonic convergence check.
- **Over-removal**: Aggressive non-manifold removal removes triangles needed for
  the surface, creating new holes. Mitigated by running safe mode first (prefers
  real face triangles over fill triangles) and only using aggressive mode in
  the final pass.
- **Performance**: Additional iterations add ~10% tessellation time per iteration.
  With 5 max iterations, worst case is ~50% slower for pathological meshes.
  Acceptable for correctness.

## Research Basis

- Ref #24: Barton et al. — Hybrid B-Rep/mesh boolean pipelines require iterative
  mesh repair as a post-process. The key insight is that repair operations interact:
  vertex welding changes edge topology, hole filling adds triangles that may overlap,
  and non-manifold removal exposes new holes. Iterative convergence is the standard
  approach.
- Standard mesh repair literature: The weld→fill→clean cycle is a well-established
  pattern in computational geometry toolkits (MeshLab, libigl, CGAL).

### Analytical vs. Approximate Method Justification

This spec does not introduce new SSI operations. It improves the mesh-level repair
post-process for boolean results. All quadric surface pairs continue to use exact
SSI per A15. The mesh repair addresses discretization artifacts from the polygon
clipping step (Sutherland-Hodgman) that produces the triangle soup for stitching.
