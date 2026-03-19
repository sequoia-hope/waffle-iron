# Bounded Tessellation: Shared Vertices for Polygon-Clipping Cylindrical Faces

**Status**: Spec (pre-implementation)
**References**: [#33] Stroud — B-Rep manifold closure, [#24] Barton — hybrid boolean mesh quality
**Governance**: A8.2, A10.2, P1

---

## Goal

Fix non-manifold edge defects in post-boolean tessellation by ensuring cylindrical
faces from polygon-clipping fallback results use shared boundary vertices from the
`EdgeDiscretization` pool, matching the watertight-by-construction contract of
`tessellate_solid_bounded`.

## Problem Statement

When `ssi_boolean_op` returns `NotSupported` for a box-cylinder pair, the kernel
falls back to `boolean_op` (polygon clipping). The result has:
- `is_polygon_soup = false` (intentional: bounded tessellation produces watertight output)
- Face geometry: `SurfaceGeom::Cylindrical` (preserved from input)
- Edge geometry: `CurveGeom::Linear` (polygon approximation vertices)

`tessellate_solid_bounded` calls `tessellate_cylindrical_face_bounded`, which detects
`has_curved_edges = false` and falls back to creating per-face vertices (lines 2294-2333
of tessellation/mod.rs). Adjacent planar faces use shared vertices from `disc.positions`.
This mismatch produces non-manifold edges at face boundaries.

The pattern manifests as exactly **2 non-manifold edges** in the output mesh, consistently
observed in ~10 assay cases (R0019, R0020, R0028, R0032, R0043, R0061, R0065, R0074, R0082).

## Parameters

- **Input**: WaffleSolid from polygon-clipping boolean fallback path
- **Face geometry**: SurfaceGeom::Cylindrical with Linear edges
- **Expected output**: Watertight mesh (0 unpaired edges)

## Branch Table

| Face Type | Has Curved Edges | Current Behavior | Fix |
|-----------|-----------------|-----------------|-----|
| Cylindrical | Yes (Circle/Arc/Ellipse) | Shared vertices via ring-building | No change |
| Cylindrical | No (Linear/None) | Per-face vertices (BUG) | Use shared boundary vertices |
| Planar | N/A | Shared vertices via disc pool | No change |

## Invariants

1. **Watertight output**: `tessellate_solid_bounded` must produce 0 unpaired edges
   for manifold input B-Rep.
2. **Shared vertex contract**: All faces in `tessellate_solid_bounded` must use
   vertices from the `EdgeDiscretization.positions` pool. Per-face vertex creation
   is forbidden in this path.
3. **Normal correctness**: Cylindrical face normals must be computed from the
   cylinder geometry (radial direction from axis), not from the polygon normal.

## Oracles

1. **Watertight check**: `mesh_watertight()` returns true for box-minus-cylinder results
2. **Non-manifold count**: 0 non-manifold edges
3. **Triangle count**: Consistent with input face count (no missing faces)
4. **Normal direction**: Cylinder face normals point radially outward from axis

## Failure Modes

- If the boundary vertex count is < 3 for a cylindrical face: skip face (already handled)
- If boundary vertices are collinear: degenerate fan triangulation (acceptable, invisible)

## Research Basis

- [#33] Stroud §5.4: Shared-vertex tessellation for manifold closure
- [#24] Barton: Mesh quality in hybrid boolean results

## Analytical vs. Approximate Method Justification

- **Method**: Approximate (polygon fan tessellation with cylindrical normals)
- **Justification**: The input faces are already polygon approximations from the
  S-H clipping pipeline. The tessellation simply triangulates these polygons.
  No SSI computation involved.
- **Surface pair coverage**: N/A (tessellation only, no boolean)

---

*Last updated: 2026-03-19*
