# Yang Section 4.4.1: CDT Mesh Updating

## Overview

After SSI vertex refinement (Section 4.3) moves intersection vertices to
surface-exact positions, the face meshes need re-triangulation so that edges
exactly follow the refined intersection curves. This is Stage 4a of the
Yang 2025 hybrid boolean pipeline.

## Problem

SSI refinement (Stage 3 / Section 4.3) projects intersection vertices from
mesh-approximate positions onto exact SSI curves. This moves vertices but
does not update the surrounding triangulation. The result is face meshes
whose triangles may:

- Have edges that no longer follow the intersection curve (chord error)
- Contain degenerate or inverted triangles near moved vertices
- Lack proper CDT constraints along the refined curve

## Solution: CDT Re-meshing (Yang Section 4.4.1)

For each face that has intersection edges with refined SSI curves:

1. Collect the refined intersection vertices on that face
2. Build constraint edges along the refined curve segments
3. Re-triangulate the face using CDT (Constrained Delaunay Triangulation)
   to restore bijectivity while respecting the constraint edges

Our existing `mesh_arrangement::triangulate_single_triangle` provides the
CDT-equivalent operation. The mesh updating step bridges SSI refinement
(Section 4.3) and the final B-Rep assembly (Stage 5).

## Interface

```rust
/// Yang Section 4.4.1: Re-triangulate face meshes along refined SSI curves.
/// Ref [#24] Yang 2025, Section 4.4.1
pub(crate) fn update_mesh_along_refined_curves(
    topology: &mut ResultTopology,
    refinement: &EdgeRefinementMap,
)
```

### Inputs
- `topology` — Mutable reference to the result B-Rep from Phase 3. Contains
  the half-edge arena, face provenance, and intersection edge flags.
- `refinement` — The `EdgeRefinementMap` from Phase 4b with SSI curves for
  each refined intersection edge.

### Behavior
- For faces adjacent to refined intersection edges, re-triangulate using CDT
  with constraint edges along the refined curves.
- Faces with no refined edges (e.g., all-planar booleans) are unchanged.
- Must preserve: face count, watertightness (twin pairing), vertex positions.
- May modify: triangle connectivity within affected faces.

## Invariants

1. **No-op for empty refinement**: If `refinement.edges` is empty, topology
   is unchanged.
2. **Vertex-on-curve**: After updating, all vertices on refined intersection
   edges must lie on the SSI curve (within TAU_MODEL).
3. **Watertightness preserved**: No new unpaired edges introduced.
4. **Face count preserved**: Number of faces unchanged.

## References

- [#24] Yang, Jia & Yan (2025) — Section 4.4.1, Mesh Updating
- [#9] Cherchi et al. (2020) §5 (arrangement) — Mesh arrangement that uses
  CDT (originally earcut, replaced in [#38] Cherchi 2022 §4 with the
  [#39] Livesu et al. 2021 simplified earcut for linear-time CDT).
- [#39] Livesu et al. (2021) — Deterministic linear-time constrained
  triangulation using simplified earcut. (Sometimes referenced in older
  project docs as "Livesu & Cherchi 2022" — same paper. Cherchi 2022's own
  bibliography cites this as [Livesu et al. 2021].) Yang 2025 §4.4.1 mesh
  updating relies on this CDT to retriangulate trimmed mesh patches around
  refined SSI curves.
- [#38] Cherchi et al. (2022) — Full mesh-Boolean pipeline that Yang 2025
  §4.2 / §4.4.2 cites for mesh intersection and in/out classification.
