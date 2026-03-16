# Spec: Chained Boolean — Cylinder Geometry Preservation

**Status**: Implementing
**Priority**: P2 (spec precedes implementation)

## Problem

When two cylinders are unioned onto a box in sequence (box + cyl1 → result + cyl2),
two bugs manifest:

1. **Cylinder 1 disappears**: The merged solid's geometry is reduced to a plain
   bounding box, erasing cylinder 1.
2. **Cylinder 2 tessellates as flat facets**: The analytical `SurfaceGeom::Cylindrical`
   is lost through the boolean pipeline.

## Root Causes

### RC1 — `do_boolean` dispatch treats merged solid as a simple box

After the first boolean (box + cyl1), the result has `cylinder_params: None` and >6
faces. For the second boolean (result + cyl2), the dispatch enters `ssi_boolean_op` →
`box_cyl_boolean`, which calls `compute_rotated_box_aabb()` and reconstructs the
"box" from its AABB alone, erasing cylinder 1.

### RC2 — `build_brep_from_polygons_inner` assigns `Planar` to all faces

When the polygon-approx fallback is used, every face gets `SurfaceGeom::Planar`
regardless of its original surface type. Combined with `cylinder_params: None`,
tessellation never reaches the cylindrical code path.

## Parameters

- **Input**: Chained boolean sequence: box + cyl1 (union) → result + cyl2 (union)
- **Box**: 2×2×1 at origin
- **Cylinder 1**: radius 0.3, depth 0.5, on top face
- **Cylinder 2**: radius 0.3, depth 0.5, on top face, offset from cyl1

## Branch Table

| Solid A | Solid B | Dispatch |
|---------|---------|----------|
| simple box (≤6 faces) | primitive cylinder | `ssi_boolean_op` → `box_cyl_boolean` |
| primitive cylinder | simple box (≤6 faces) | `ssi_boolean_op` → `box_cyl_boolean` (swapped) |
| primitive cylinder | primitive cylinder | `ssi_boolean_op` → `cyl_cyl_boolean` |
| simple box | simple box | `boolean_op` (polygon clipping) |
| general solid (>6 faces) | any | `polygon_approx_boolean` |
| any | general solid (>6 faces) | `polygon_approx_boolean` |

## Invariants

1. **Volume**: V(result) = V(box) + V(cyl1) + V(cyl2) - V(overlap) (within 5% due to polygon approximation)
2. **Watertight**: Every edge shared by exactly 2 triangles
3. **Euler χ=2**: V - E + F = 2 for genus-0 solid
4. **Cylindrical geometry preserved**: Result `face_geometry` contains `SurfaceGeom::Cylindrical` faces for both cylinders' lateral surfaces

## Design

### Fix 1: Dispatch (RC1)

Only use SSI path when **both** operands are recognized primitives:
- Simple box: `cylinder_params.is_none()` AND `face_map.len() <= 6`
- Primitive cylinder: `cylinder_params.is_some()`

General solids (>6 faces, no cylinder_params) always route to `polygon_approx_boolean`.

### Fix 2: Surface geometry preservation (RC2)

Add `surface_geom: Option<SurfaceGeom>` to `FacePoly`. Tag cylindrical faces with
their source geometry during polygon extraction. Preserve through clipping and
classification. Use in `build_brep_from_polygons_inner` instead of always `Planar`.

## Research Basis

- **Ref #24 Barton et al.**: Hybrid B-Rep/mesh boolean — bijective re-mapping of
  analytical surfaces through mesh booleans
- **Ref #7 Jacobson et al. (2013)**: GWN for inside/outside classification
- **Ref #6 Sugihara-Iri (2000)**: Topology-oriented approach

### Analytical Primacy (A15)

Root cause RC2 (all faces assigned `SurfaceGeom::Planar` by `build_brep_from_polygons_inner`)
is the direct consequence of violating A15 (governance/ARCHITECTURAL_INVARIANTS.md).
When a chained boolean routes a cylinder through the mesh/polygon path, analytical
surface geometry is destroyed. The principled fix is exact SSI for all quadric
surface pairs — not improving the mesh fallback's geometry preservation. See A15.4
for the implementation sequence of the 15 quadric pair solvers.
