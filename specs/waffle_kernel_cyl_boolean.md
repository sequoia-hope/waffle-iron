# WaffleKernel Cylindrical Boolean Operations

## Goal

Enable boolean operations (union, subtract, intersect) between box and cylinder
solids, and between two cylinder solids, using analytical Surface-Surface
Intersection (SSI) rather than mesh-based booleans.

## Parameters

- Both solids must be Z-axis aligned
- Boxes must be axis-aligned
- Cylinders defined by `CylinderParams` (center_bottom, radius, depth, direction, x_axis, y_axis)
- Operations: `BoolOp::Union`, `BoolOp::Subtract`, `BoolOp::Intersect`

## SSI Algorithm

For axis-aligned geometries, all intersection curves are circles, arcs, or lines:

| Face Pair | SSI Curve |
|-----------|-----------|
| Plane perpendicular to Z vs Cylinder | Circle at plane height |
| Plane parallel to Z vs Cylinder | 0 or 2 vertical lines |
| Cylinder vs Cylinder (parallel) | 0 or 2 vertical lines |

## Implementation

### Box-Cylinder Booleans

Three cases handled:
1. **Enclosed cylinder** (cylinder fully inside box): subtract creates hole via `kemr` inner loops
2. **Inscribed cylinder** (cylinder circumscribes box): union/intersect dispatch
3. **Disjoint**: union merges both solids

### Cylinder-Cylinder Booleans

Compute 2D circle-circle intersection points, then build partial cylindrical
patches bounded by vertical lines and arcs. Each patch has analytical
`SurfaceGeom::Cylindrical` geometry and `CurveGeom::Arc` edge geometry.

### Tessellation

- Full cylindrical faces: parametric grid in (theta, z)
- Partial cylindrical patches: angular subrange from arc edge geometry
- Planar faces with holes: advancing-front triangulation between outer rectangle and inner circle
- Planar cap faces with arcs: expand arc edges to polyline segments

## Invariants

- Volume within tolerance (5.0 for box-cyl, 10.0 for cyl-cyl)
- Watertight mesh (every edge shared by exactly 2 triangles)
- Euler characteristic V - E + F = 2
- Result contains analytical `SurfaceGeom::Cylindrical` faces and `CurveGeom::Arc` edges

## Scope

- Z-axis aligned cylinders with axis-aligned boxes: implemented
- Revolve solid booleans: returns `NotSupported`
- Non-axis-aligned orientations: deferred

## Research Basis

- **[#1] Patrikalakis Ch.5** — SSI algorithms for analytic surfaces. Provides the
  mathematical foundation for exact plane-cylinder and cylinder-cylinder intersection
  curves used in this implementation.
- **[#16] Mantyla** — Euler operators for B-Rep manipulation. The topological
  operations (mev, mef, kemr) used to construct boolean results.
- **[#33] Stroud §6.1** — Curve-face classification. Sequencing rules for
  ENTERS/LEAVES at intersection boundaries.
- **[#25] Yang et al. (2023)** — Topology-guaranteed SSI via Dixon resultant.
  Ensures correct number of intersection curve branches before tracing.
- **[#27] Li et al. (2026)** — SSI method survey recommending hybrid architecture:
  analytical for simple pairs, topology-guaranteed for complex pairs.
- **[#24] Barton et al. (2025)** — Hybrid B-Rep/mesh boolean pipeline. Bijective
  re-mapping preserves analytical surfaces through mesh boolean stage.

### Analytical Primacy (A15)

This spec implements exact SSI per invariant A15 (governance/ARCHITECTURAL_INVARIANTS.md).
All plane-cylinder and cylinder-cylinder intersections use closed-form analytical
solutions. No mesh/polygon fallback is used for these quadric surface pairs.
