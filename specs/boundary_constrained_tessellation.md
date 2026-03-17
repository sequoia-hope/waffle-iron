# Boundary-Constrained Tessellation for Boolean Results

**Sprint**: H
**Status**: In Progress
**Created**: 2026-03-17

## Problem

After boolean operations, `CylinderParams` and `RevolveParams` are `None`.
Tessellation falls back to geometry-derived paths where adjacent faces independently
compute boundary ring vertices from different parametric formulas. The f32 positions
diverge between faces sharing an edge, producing unpaired edges (non-watertight mesh).

114 of 124 assay failures are `watertight_mesh` errors caused by this divergence.

## Root Cause

- Cap face: `tessellate_circular_cap()` with `make_circle_axes()` derives ring from Circle edge geometry
- Lateral face: `tessellate_cylindrical_patch()` derives ring from Cylinder face geometry
- Two independent parametric computations produce different f32 ring positions at shared boundary
- 17+ post-processing passes (~1,500 lines of weld/fill/snap) fail to close the gaps on curved boundaries

## Approach: Edge-First Tessellation (Industry Standard)

Discretize B-Rep edges first into a shared f64 vertex pool, then tessellate each face
using those shared boundary vertices. Watertight by construction — adjacent faces
reference identical vertex positions from the shared pool, so after f32 conversion
the positions are bitwise identical.

Reference: Stroud Ch.8, OpenCascade/ACIS/Parasolid edge-first tessellation.

### Scope

Only the boolean result path (`cylinder_params.is_none() && revolve_params.is_none()`).
Existing primitive extrude/revolve tessellation is untouched.

## Design

### Edge Discretization

```rust
struct EdgeDiscretization {
    positions: Vec<[f64; 3]>,                 // shared vertex pool
    edge_verts: HashMap<EdgeIdx, Vec<usize>>, // ordered vertex indices per edge
}
```

Per edge type:
- **Linear** (or no geometry): 2 vertices from arena vertex positions
- **Circular**: `CIRCLE_SEGMENTS` (64) points via `Circle3D::evaluate(t)`
- **Arc**: proportional segments via `Arc3D::evaluate(t)`
- **Elliptical**: proportional segments via `Ellipse3D::evaluate(t)`

### Bounded Face Tessellators

Each face walks its outer loop half-edges, collecting boundary vertex indices
from the shared pool (reversing for reversed half-edges).

- **Planar face**: fan or earclip triangulation using shared boundary vertices
- **Cylindrical face**: quad strip connecting top and bottom rings from shared pool
- **Fallback**: collect loop vertices from shared pool, fan triangulate

### Dispatch

In `tessellate_solid()`, when both `cylinder_params` and `revolve_params` are `None`,
route to `tessellate_solid_bounded()` which:

1. Calls `discretize_edges()` to build the shared vertex pool
2. For each face: walks outer loop half-edges to collect boundary indices
3. Dispatches to the appropriate bounded tessellator
4. Applies minimal post-processing: `fix_winding_consistency`, `remove_degenerate_triangles`, `fix_global_orientation`

## Invariants

- Adjacent faces sharing a B-Rep edge use bitwise-identical f32 vertex positions
- The shared vertex pool is computed once in f64, converted to f32 once
- No per-face parametric computation for boundary vertices (eliminates divergence source)
- Post-processing is minimal (3 passes, not 17+)

## Expected Impact

- Watertight failures: 114 → <30
- Some volume/monotonicity failures also resolve (leaky mesh → wrong signed volume)
- Conservative estimate: 39 pass → 70-85 pass on assay
