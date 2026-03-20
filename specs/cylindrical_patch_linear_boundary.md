# Cylindrical Patch Tessellation from Linear Boundaries

**Status**: Implementing
**Type**: Bug fix — tessellation defect after boolean operations

## 1. Goal

Fix `tessellate_cylindrical_patch()` to correctly tessellate cylindrical faces that
result from boolean operations where all boundary edges are linear (from polygon
clipping). Currently, these faces are tessellated as full 360° cylinder rings
regardless of the actual boundary extent, producing overlapping geometry that
fails the AABB-collapse oracle.

This fix targets 11 assay cases in the `aabb-collapse` category.

## 2. Parameters

| Parameter | Type | Source | Description |
|-----------|------|--------|-------------|
| `arena` | `&TopoArena` | B-Rep topology | Half-edge structure with vertex positions |
| `face_idx` | `FaceIdx` | Current face | Index of the cylindrical face to tessellate |
| `cyl` | `&Cylinder` | `face_geometry` map | Cylinder origin, axis, radius |
| `edge_geometry` | `HashMap<EdgeIdx, CurveGeom>` | B-Rep metadata | Edge curve types (may be all Linear) |

**Derived parameters** (computed from boundary walk):
- `t_min`, `t_max`: axial extent along cylinder axis
- `angle_min`, `angle_max`: angular extent from boundary vertex projections
- `is_full`: whether the patch covers 360°

## 3. Branch Table

| Condition | Behavior | Current | Fixed |
|-----------|----------|---------|-------|
| Has Circular edge | Full 360° ring | Correct | No change |
| Has Arc edge(s) | Partial strip from arc angles | Correct | No change |
| **No curved edges, `angle_start.is_none()`** | **Full 360° ring** | **WRONG** | **Derive angular range from boundary vertices** |
| No curved edges, narrow angular spread | Partial strip | N/A (new) | Generate parametric strip for boundary extent |
| No curved edges, ≥360° angular spread | Full ring | N/A (new) | Full ring (same as current for true full patches) |

## 4. Invariants

1. **Vertex-on-surface**: All generated vertices must lie on the cylinder surface
   (distance from axis = |radius|, within TAU_MODEL tolerance).
2. **Outward normals**: Normal vectors point radially outward (or inward if
   `radius < 0`), perpendicular to the cylinder axis.
3. **No AABB collapse**: For a cylinder-minus-box boolean, the result mesh must
   have vertices NOT all lying on AABB faces. Specifically, cylindrical faces
   must produce curved vertex positions between AABB extremes.
4. **Boundary containment**: Tessellated patch must cover only the angular range
   defined by the face's boundary vertices, not exceed it.
5. **Triangle count**: Partial patches should have `ceil(CIRCLE_SEGMENTS * sweep / TAU)`
   angular segments, minimum 4.

## 5. Oracles

1. **AABB collapse oracle**: `check_aabb_collapse()` must pass for cylinder-boolean results.
2. **Radius oracle**: For each vertex on a cylindrical face, compute distance to axis
   and verify `|dist - |radius|| < TAU_MODEL`.
3. **Normal consistency**: Dot product of vertex normal with radial direction > 0
   (or < 0 for inward faces).
4. **XY non-collapse**: `is_xy_aabb_collapsed()` must return false for cylinder-boolean meshes.

## 6. Failure Modes

| Input | Expected Behavior |
|-------|-------------------|
| All boundary vertices at same angle | Degenerate face — skip (produce no triangles) |
| Boundary vertices span >360° | Treat as full ring |
| Single boundary vertex | Degenerate — skip |
| Very small angular range (<1°) | Generate minimum 4 segments |

## 7. Research Basis

- **Ref #1** Patrikalakis et al. — Cylinder parametrization (u,v) → (r·cos(u), r·sin(u), v).
  Angular range recovery from point projection: u = atan2(dot(p-o, y_axis), dot(p-o, x_axis)).
- **Ref #33** Stroud — B-Rep tessellation with boundary recovery. Face tessellation must
  respect the topological boundary, not assume global surface extent.
- **Ref #2** Hoffmann — Parametric surface tessellation, boundary-adaptive sampling.

### 7a. Analytical vs. Approximate Method Justification

- **Method**: Analytical. Vertices are placed on the exact cylinder surface using
  parametric evaluation. No mesh approximation is involved.
- **Surface pairs**: N/A — this is tessellation, not SSI. The cylindrical surface
  geometry is preserved exactly from the boolean result's `face_geometry` map.
