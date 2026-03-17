# Edge Geometry Reconstruction (Sprint I)

## Problem

`build_brep_from_polygons_inner` in `stitch.rs` marks ALL edges as `CurveGeom::Linear` —
even circular boundaries where a cylinder meets a plane. Without `Circular` edge geometry,
the bounded tessellation path (`has_circles && !has_arcs`) never activates for polygon-boolean
results, producing non-watertight meshes.

## Solution

Post-stitch pass that reconstructs edge geometry from adjacent face surfaces.

### Algorithm

For each edge in the B-Rep:
1. Find both adjacent faces via HalfEdge → Loop → Face traversal
2. Match face surface geometry pairs:
   - **Cylinder × Plane (perpendicular)**: SSI is a circle (Patrikalakis Ch.5)
   - All other pairs: leave as Linear
3. Validate reconstructed geometry against edge endpoint positions

### Supported Surface Pairs

| Face A | Face B | Edge Geometry |
|--------|--------|---------------|
| Planar | Cylindrical (perp) | Circular |
| Planar | Cylindrical (oblique) | Linear (skip — ellipse) |
| Planar | Planar | Linear |
| Cylindrical | Cylindrical | Linear (skip — IC curve) |
| Self-twin boundary | — | Linear (skip) |

### Circle Construction (Cylinder × Plane perpendicular)

```
circle.center = project(cyl.origin, plane)
circle.normal = plane.normal
circle.radius = cyl.radius
```

Perpendicularity test: `|dot(plane.normal, cyl.axis)| > 1.0 - 1e-6`

### Validation

Edge endpoints must lie on the reconstructed circle within TAU_COINCIDENT:
```
|distance(endpoint, circle.center) - circle.radius| < TAU_COINCIDENT
```

### References

- Patrikalakis et al., Ch.5: Surface-Surface Intersection
- Barton et al. [#24]: Bijective re-mapping of analytical surfaces
