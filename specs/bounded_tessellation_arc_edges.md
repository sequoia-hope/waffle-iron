# Spec: Bounded Tessellation for Arc-Edge Boolean Results

## Goal

Enable the watertight-by-construction bounded tessellation path for boolean
results that contain arc edges (CurveGeom::Arc). Currently, a blanket
`has_arcs` guard forces all arc-edge boolean results to the per-face-vertex
fan path, which produces non-shared vertices and manifold violations.

The bounded path's `discretize_edges` function already handles Arc, Circular,
and Elliptical curve types. Removing the blanket guard routes more boolean
results through the watertight path, reducing unpaired edges in the output mesh.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| (none — internal tessellation dispatch change, no new user-facing parameters) |

## Branch Table

| Condition | Tessellation Path | Change |
|-----------|------------------|--------|
| Boolean result, no arcs, no polygon-soup | Bounded | No change |
| Boolean result, has arcs, no polygon-soup | **Bounded** | **Changed from Fan** |
| Boolean result, polygon-soup | Fan (per-face) | No change |
| Pristine solid (has *Params) | Geometry-driven | No change |

## Invariants

1. **Shared vertices on shared edges**: Adjacent faces in bounded tessellation
   must reference identical vertex positions from the shared EdgeDiscretization
   pool. This is the watertight-by-construction property.

2. **No regression**: All 562 existing kernel tests must continue to pass.

3. **Correct normals**: Cylindrical faces must have radial normals (not planar
   normals), computed from the cylinder axis and vertex position.

4. **Arc edge fidelity**: Arc edges must be discretized into sufficient segments
   (proportional to sweep angle) to maintain geometric accuracy.

5. **Inner loop support**: Cylindrical faces with inner loops (from cyl-cyl
   boolean) must produce valid annular tessellation.

## Oracles

1. **Watertight mesh check**: `check_watertight_mesh()` — every triangle edge
   shared by exactly 2 triangles (position-based matching).
   - Box-cyl union: 0 unpaired edges
   - Box-cyl subtract: 0 unpaired edges

2. **Triangle count**: Output mesh must have ≥ expected minimum triangles for
   the geometry type (not collapsed to just the first body).

3. **Positive volume**: Signed volume must be positive (outward-facing normals).

4. **No degenerate triangles**: No zero-area triangles (within tolerance).

## Failure Modes

| Condition | Expected Behavior |
|-----------|------------------|
| Polygon-soup boolean result | Falls through to fan path (unchanged) |
| Degenerate arc (zero sweep) | Linear edge fallback in discretize_edges |
| Self-intersecting tessellation | Caught by winding consistency fix |

## Research Basis

- Ref #1 (Patrikalakis Ch.5): SSI algorithms produce exact intersection curves,
  which are discretized as arcs/circles in the B-Rep edge geometry. The bounded
  tessellation path must handle these curve types to maintain watertight output
  from analytical boolean results.
- Ref #24 (Barton): Bijective surface geometry mapping through boolean pipeline.
  The bounded path's cylindrical face tessellation uses the preserved
  SurfaceGeom::Cylindrical to compute correct radial normals.
