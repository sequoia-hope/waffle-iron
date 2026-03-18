# Partial Box-Cylinder Boolean Specification

## Summary

Enable boolean operations (union, subtract, intersect) for box-cylinder and
cylinder-box pairs where the solids partially overlap — neither fully enclosed
nor disjoint. This is the #1 gap by assay failure count (30+ cases).

## Approach: Polygon Clipping with Geometry Preservation

When the SSI analytical path returns `NotSupported` for a partial overlap,
fall through to the polygon-clipping boolean pipeline (`boolean_op`), which:

1. Converts the cylinder to 32-sided polygon approximation (`cylinder_to_face_polys`)
2. Extracts box faces from B-Rep topology
3. Classifies face fragments (inside/outside) via Sutherland-Hodgman clipping
4. Stitches the result into a new B-Rep

**Geometry preservation (A15.5)**: Cylindrical face fragments retain their
`SurfaceGeom::Cylindrical` tag through the pipeline via `FacePoly.surface_geom`.
The stitch pass reconstructs `CurveGeom::Circular` edges at cylinder-plane
perpendicular boundaries.

## Key Design Decision

The A15 invariant requires analytical SSI for quadric pairs. For partial overlaps
where we don't yet have a fully analytical solution, we use the polygon-clipping
pipeline as an interim implementation that:
- Preserves surface type metadata (A15.5 compliance)
- Produces correct topology (Euler V-E+F=2)
- Handles all spatial configurations (no more `NotSupported`)

This is explicitly an interim step. Full analytical partial-overlap booleans
(with exact trimming curves) are a future enhancement that will replace this path.

## Changes

### waffle_kernel.rs
- Catch `NotSupported` from `ssi_boolean_op` and fall through to `boolean_op`
  with strict→tolerant fallback chain (same pattern as the all-quadric branch)

### boolean/analytical.rs
- Remove `#[allow(dead_code)]` from `ellipse_to_polygon` (now used)
- (Partial overlap `NotSupported` returns remain in place — they are now caught
  by the caller rather than propagating)

### boolean/stitch.rs
- Extend `reconstruct_edge_geometry` to handle oblique plane-cylinder edges
  (elliptical curves) in addition to perpendicular (circular) curves

## Invariants

- Result topology: V-E+F = 2
- Surface type preservation: unmodified faces keep original SurfaceGeom
- Determinism: same inputs → identical output (sort-based face ordering)
- No mesh fallback for cases WITH analytical solutions (enclosure, disjoint)

## Test Plan

- Unit: partial box-cyl subtract at various overlap fractions
- Unit: partial box-cyl union / intersect
- Unit: partial cyl-minus-box subtract
- Property: Euler characteristic on all results
- Assay: score increase from 39/155 baseline

## References

- ADR-1: Hybrid B-Rep/mesh boolean pipeline
- A15, A15.5: Analytical primacy, surface type preservation
- [#24] Barton: Bijective re-mapping of analytical surfaces through mesh booleans
