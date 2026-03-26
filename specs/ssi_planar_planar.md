# SSI: Planar-Planar Boolean Operations

Spec for exact boolean operations on all-planar solid pairs (A15 compliance).

## Goal

Replace the polygon-clipping fallback path (`boolean_op` / `boolean_op_tolerant`)
for all-planar solid pairs with a dedicated function that fixes the known
classification bugs (inward-offset sampling, self-twin boundary construction).

## Parameters

- `solid_a: &WaffleSolid` — first operand, all faces `SurfaceGeom::Planar`
- `solid_b: &WaffleSolid` — second operand, all faces `SurfaceGeom::Planar`
- `op: BoolOp` — Union, Subtract, or Intersect
- `id_alloc: &mut dyn FnMut() -> u64` — kernel ID allocator

## Precondition

Both operands have ONLY planar faces. No cylindrical, conical, spherical, or
toroidal surfaces. Verified by `WaffleKernel::all_faces_planar()` before dispatch.

## Branch Table

| Op | A convex | B convex | Coplanar faces | Disjoint | Enclosed | Partial |
|----|----------|----------|----------------|----------|----------|---------|
| Union | Y | Y | N | emit both | emit outer | clip+merge |
| Union | Y | N | N | emit both | emit outer | clip+merge |
| Union | N | N | N | emit both | emit outer | clip+merge |
| Union | * | * | Y | coplanar merge | coplanar merge | clip+coplanar |
| Subtract | * | * | N | emit A | empty | clip+invert B |
| Subtract | * | * | Y | emit A | empty | clip+coplanar |
| Intersect | * | * | N | empty | emit inner | clip+merge |
| Intersect | * | * | Y | empty | emit inner | clip+coplanar |

## Algorithm

1. Extract face polygons via `extract_face_polys_general`
2. Compute adaptive tau/tau_weld via `compute_adaptive_tau_weld`
3. AABB disjoint fast-path: build proper B-Rep via `build_brep_from_polygons_inner`
4. Classify faces using S-H (convex opposing) or progressive splitting (non-convex)
5. For non-convex classification: use `point_in_solid(centroid)` WITHOUT inward offset
   (fragment centroids of plane-plane splits lie in the face interior, not on curved surfaces)
6. Collect result fragments per boolean semantics
7. Post-process: dedup, merge, T-junction resolution
8. Build B-Rep: `build_brep_from_polygons_inner` with `allow_boundary = false`

## Invariants

- **A15.1**: All input faces are planar (quadric) — exact SSI
- **A15.5**: All output faces are `SurfaceGeom::Planar` — surface type preservation
- **V-E+F=2**: Euler characteristic for genus-0 result (watertight manifold)
- **Watertight**: Every mesh edge shared by exactly 2 triangles
- **Positive volume**: Union/subtract results have volume > 0

## Oracles

- Watertight mesh (no unpaired edges)
- Euler characteristic = 2 (genus-0)
- Volume > 0 (union, subtract, intersect of overlapping solids)
- Volume magnitude: union ≤ vol_a + vol_b, intersect ≤ min(vol_a, vol_b)

## Failure Modes

- **Near-coplanar faces**: Two face planes within ~1° of parallel. Handled by
  `classify_coplanarity` with TAU_PARALLEL threshold.
- **Sliver fragments**: Very thin intersection regions. Filtered by TAU_NORMALIZE
  area threshold in progressive splitting.
- **T-junctions**: One face's edge passes through another's vertex. Resolved by
  `resolve_t_junctions` post-processing step.

## References

- Patrikalakis [#1] Ch.5: Plane-plane intersection is a line
- Shewchuk [#4]: Robust floating-point geometric predicates
- Jacobson [#7]: Generalized winding numbers for inside/outside classification
