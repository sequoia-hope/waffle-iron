# Boolean Mesh Watertight Improvement

## Goal

Reduce unpaired edges in boolean result meshes by improving the mesh-level repair
pipeline in `tessellation/mod.rs`. Current assay score is 65/160 with 61 failures
attributed to `boolean-watertight`. Target: fix ≥10 of these by improving three
mesh repair functions.

## Parameters

### weld_boundary_vertices — weld distance multiplier
- **Current**: `grid * 2.5` (grid = max_abs * 1e-5)
- **Proposed**: `grid * 5.0`
- **Rationale**: S-H clipping produces edge deviations up to 10× the mesh grid.
  The stitch step (stitch.rs) uses proximity pairing up to 5000× tau_weld (= 5e-4
  for typical models), but the mesh weld only reaches 2.5× grid (= 1.25e-5). The
  gap means vertices paired at the B-Rep level but tessellated with per-face
  positions may still diverge at the mesh level. Doubling the weld radius captures
  these near-miss cases without merging distinct vertices.

### close_near_boundary_chains — max component size
- **Current**: 3–8 vertices
- **Proposed**: 3–32 vertices
- **Rationale**: Larger boundary holes arise from multi-face clipping inaccuracies.
  The current limit of 8 leaves medium-sized holes unfilled. Extending to 32
  allows the fan-fill strategy to close holes up to ~30 boundary edges, which
  covers most medium holes without performance risk (O(N²) is fine for N≤32).

### close_near_boundary_chains — open chain closure
- **Current**: Only handles closed boundary cycles
- **Proposed**: When an open boundary chain has endpoints within 10× grid distance,
  snap the endpoints together to form a closed cycle, then fill with fan triangles.
- **Rationale**: S-H divergence at face intersection boundaries creates short
  gaps between almost-closed boundary chains. Snapping the gap closed and filling
  the cycle resolves the last few unpaired edges.

## Branch Table

| Change | Parameter Value | Effect |
|--------|----------------|--------|
| Weld multiplier 2.5 (current) | grid * 2.5 | Misses some near-miss vertices |
| Weld multiplier 5.0 (proposed) | grid * 5.0 | Merges boundary vertices within 5× grid |
| Component limit 8 (current) | max 8 vertices | Fills only small holes |
| Component limit 32 (proposed) | max 32 vertices | Fills medium holes |
| No open chain closure (current) | — | Open chains left unfilled |
| Open chain closure (proposed) | snap within 10× grid | Closes near-miss open chains |

## Invariants

1. **No false welding**: Vertices further than 5× grid apart must not be merged.
   For models at scale 0.5, 5× grid = 2.5e-5 — well below the minimum feature
   size (1e-6 m stated in units, but effectively >1e-3 in practice). No two
   geometrically distinct vertices should be within 2.5e-5 of each other.
2. **Monotonic improvement**: Each repair pass must reduce or maintain the boundary
   edge count. No pass may increase unpaired edges.
3. **No topology corruption**: Fill triangles must produce valid winding relative
   to adjacent faces. Non-manifold edges (3+ triangles sharing an edge) must not
   increase.
4. **Deterministic output**: Same input → same output. No ordering-dependent behavior
   from HashMap iteration.

## Oracles

- **Watertight check**: Count unpaired edges in output mesh (same quantization as
  assay oracle: max_abs * 1e-5). Zero unpaired = pass.
- **Non-manifold check**: Count edges shared by 3+ triangles. Must not increase.
- **Assay score**: Run full assay. boolean-watertight failures must decrease.
- **Volume sign**: Signed mesh volume must remain positive for convex-ish solids.

## Failure Modes

- **Over-welding**: If weld radius is too large, geometrically distinct vertices
  merge → collapsed triangles, inverted faces. Mitigated by keeping radius at 5×
  (well below feature scale).
- **Fill winding errors**: Fan triangles may have wrong winding. Mitigated by
  existing `fix_winding_consistency` pass that runs after fills.
- **Performance**: Larger component limit increases `close_near_boundary_chains`
  work. O(N²) for N≤32 is negligible.

## Research Basis

- Ref #7: Jacobson et al. — winding number classification informs the boundary
  detection strategy (mesh edges with winding number boundary indicate true
  solid boundaries).
- Ref #24: Barton et al. — hybrid B-Rep/mesh boolean pipelines require mesh
  repair as a post-process when polygon clipping introduces discretization gaps.
- Standard mesh repair literature: vertex welding, hole filling, T-junction
  resolution are established techniques in computational geometry toolkits.

### Analytical vs. Approximate Method Justification

This spec does not introduce new SSI operations. It improves the mesh repair
post-process for the existing polygon-based boolean pipeline. All quadric surface
pairs use exact SSI per A15 where the SSI path is taken. When polygon fallback is
used (chained booleans, non-primitive inputs), this mesh repair makes the fallback
output more robust.
