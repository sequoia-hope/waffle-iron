# Spec: Full-Edge Vertex Welding for Watertight Tessellation

## Goal

Extend the tessellation module's post-hoc vertex welding from arc-edges-only to
ALL shared topological edges, producing watertight meshes from the fan
tessellation path. Currently, only `weld_arc_edge_vertices` runs after the fan
path, leaving linear and circular edge boundaries un-welded. This creates
unpaired edges that fail the watertight oracle check.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| grid_resolution | f64 | 1e7 | Quantization factor for position hashing (positions rounded to nearest 1e-7 m) |
| weld_after_removal | bool | true | Whether to weld polygon-soup results after `remove_isolated_triangles` |

No new public API parameters — this is an internal tessellation improvement.

## Branch Table

| Solid Type | Has Arc Edges | Is Polygon Soup | Tessellation Path | Welding Before Fix | Welding After Fix |
|-----------|---------------|-----------------|-------------------|-------------------|------------------|
| Boolean result (planar only) | No | No | Bounded (shared vertices) | N/A (already watertight) | N/A (unchanged) |
| Boolean result (with arcs) | Yes | No | Fan + arc weld | Arc edges only | ALL shared edges |
| Boolean result (polygon soup) | Maybe | Yes | Fan + remove_isolated | None | ALL shared edges (after removal) |
| Cylinder primitive | N/A | No | Geometry-driven fan | None | ALL shared edges |
| Revolve primitive | N/A | No | Geometry-driven fan | None | ALL shared edges |
| Cone primitive | N/A | No | `tessellate_cone_solid` | N/A (dedicated) | N/A (unchanged) |
| Sphere primitive | N/A | No | `tessellate_sphere_solid` | N/A (dedicated) | N/A (unchanged) |
| Torus primitive | N/A | No | `tessellate_torus_solid` | N/A (dedicated) | N/A (unchanged) |

## Invariants

1. **Watertight mesh**: For any manifold solid, every triangle edge in the
   tessellated mesh must be paired with exactly one other triangle edge at the
   same position (opposite winding). Unpaired edges = 0.

2. **No geometry change**: Vertex positions must not change. Only index
   remapping occurs. The welding pass does not move, add, or remove vertices.

3. **Deterministic output**: Given the same input solid, the welded mesh must
   be identical across runs. The quantization grid and iteration order must be
   deterministic.

4. **Face range preservation**: `FaceRange` entries must remain valid after
   index remapping. Start/end indices don't change (only the values at those
   index positions change).

5. **Triangle validity**: No degenerate triangles introduced. If welding maps
   two vertices of a triangle to the same index, that triangle becomes
   degenerate and should be removed.

## Oracles

1. **Watertight check**: Position-quantized edge pairing (existing oracle in
   `oracle.rs`). For manifold solids, unpaired edge count must be 0.

2. **Vertex count**: Total vertex count must not increase (welding only remaps
   indices, doesn't add vertices).

3. **Triangle count**: Must remain the same or decrease (degenerate triangles
   from welding may be removed).

4. **Bounding box**: Must be identical before and after welding (no position
   changes).

5. **Signed volume**: Must be identical before and after welding (same
   triangles, same positions, just shared indices).

## Failure Modes

1. **Grid resolution mismatch**: If the quantization factor is too coarse,
   distinct vertices may be falsely welded. Mitigation: 1e-7 m resolution is
   one order of magnitude below MIN_FEATURE_SIZE (1e-6 m), so features cannot
   be smaller than the grid.

2. **Polygon-soup internal faces**: Welding before `remove_isolated_triangles`
   could connect internal face fragments to external ones, preventing removal.
   Mitigation: Always run removal BEFORE welding for polygon-soup solids.

3. **Degenerate triangles from welding**: When two vertices of a triangle are
   at the same position and get welded to the same index, the triangle
   becomes zero-area. Mitigation: Remove degenerate triangles after welding.

## Research Basis

- **[#16] Mantyla**: Half-edge B-Rep topology requires edge-paired faces for
  manifold validity. Tessellation must preserve this pairing.
- **[#33] Stroud Ch.4**: B-Rep face tessellation must maintain topological
  adjacency at shared edges for downstream operations (picking, rendering).
- **Position-based vertex welding**: Standard technique in mesh processing.
  Spatial hashing with quantized grid provides O(n) welding with deterministic
  results. Grid resolution must be below minimum feature size to prevent
  false merges.

### Analytical vs. Approximate Method Justification

- **Method**: Exact (index remapping based on position matching).
- **No SSI involved**: This is a tessellation post-processing step, not a
  surface-surface intersection operation. No approximate methods are used.
- **Surface pair coverage**: N/A — welding operates on tessellated mesh
  vertices, not on surface geometry.
