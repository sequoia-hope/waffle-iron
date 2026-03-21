# Edge-Flip Non-Manifold Repair

## Goal

Eliminate non-manifold edges caused by conflicting earcut diagonals in bounded
tessellation. When two adjacent faces share corner vertices without a B-Rep
boundary edge between them, earcut may independently create the same diagonal
in both faces, producing 3+ triangles per edge. Instead of removing excess
triangles (which creates holes), flip the diagonal in one face to use an
alternative that doesn't conflict.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `max_iterations` | `usize` | Maximum flip passes (default: 10) |
| `min_flip_area` | `f64` | Minimum triangle area after flip to accept (default: `TAU_WORK`) |

## Branch Table

| Condition | Action |
|-----------|--------|
| Edge has ≤2 triangles | Skip (manifold) |
| Edge has ≥3 triangles AND is B-Rep boundary | Skip (boundary — handled by topology-aware pass) |
| Edge has ≥3 triangles AND is interior diagonal | Attempt edge flip |
| Edge flip produces positive-area triangles | Execute flip |
| Edge flip produces degenerate/inverted triangles | Skip flip (leave for aggressive pass) |
| All non-manifold edges resolved | Stop iterating |
| Max iterations reached | Stop iterating |

## Invariants

1. **No mesh holes introduced**: Edge flips replace 2 triangles with 2 triangles — no net change in triangle count per face.
2. **Surface preservation**: Flipped triangles cover the same quad area as originals.
3. **B-Rep boundary preservation**: Only interior diagonals are flipped; boundary edges are never modified.
4. **Winding consistency**: Flipped triangles maintain the same winding direction as originals.
5. **Determinism**: Processing order is sorted by quantized edge position for reproducibility.

## Oracles

- **Manifoldness**: Every mesh edge has exactly 2 adjacent triangles (for closed solids).
- **Unpaired edges**: Zero unpaired boundary edges after repair.
- **Triangle count**: No net change in total triangle count (flips are 2→2 replacements).
- **Surface area**: Total mesh surface area unchanged within `TAU_WORK` tolerance.

## Failure Modes

1. **Non-convex quad**: The 4 vertices of the two triangles form a non-convex quad; flipping the diagonal creates an inverted triangle. **Mitigation**: Check triangle area sign before accepting flip.
2. **Cascade**: Flipping one diagonal creates a new non-manifold edge elsewhere. **Mitigation**: Iterate with a maximum iteration cap.
3. **Shared diagonal across 3+ faces**: More than 2 faces create the same diagonal. **Mitigation**: Process one face-pair at a time; multiple passes handle cascading conflicts.

## Research Basis

- Edge flipping is fundamental to Delaunay refinement [#4 Shewchuk 1997].
- Non-manifold edge repair in mesh processing literature typically uses edge
  collapse or edge flip operations [#31 Cherchi et al. 2025].
- The selective targeting of interior diagonals (vs. B-Rep boundaries) is
  specific to our hybrid B-Rep/mesh tessellation pipeline.
