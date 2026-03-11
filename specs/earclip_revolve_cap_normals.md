# Ear-Clipping Tessellation + Revolve Cap Normal Fix

## Problem Statement

### Fan Triangulation Fails on Non-Convex Polygons

Fan triangulation picks vertex 0 as hub and creates triangles (v0, vi, vi+1). For convex polygons this is correct. For non-convex polygons (like gears with 48+ vertices), fan triangles cross concavities — the geometric normal of these crossing triangles is flipped relative to the face normal. The `consistent_normals` assay oracle detects this.

**Affected assay cases**: 9 extrude-only cases with gear profiles (e.g., R0005 `extrude(gear,boss) + extrude(rectangle,boss)`) showing ~39% flipped triangles.

### Revolve Cap Stored Normals Disagree with Loop Winding

In `revolve_polygon`, the start cap gets `normal = v3_negate(plane_normal)` and end cap gets the rotated normal. But the half-edge loop winding set by Euler operators may not agree with these assigned normals. The Newell check in `tessellate_polygon_face` detects and flips winding, but the stored normal itself needs to be derived from geometry.

**Affected assay cases**: 7 revolve-only + 11 mixed cases.

## Algorithm: Ear-Clipping with Convexity Fast-Path

### Convexity Check

Compute the Newell normal from loop vertices. Check if all cross products of consecutive edges agree in sign with the Newell normal. If all agree → polygon is convex → use fan triangulation (fast path).

### Non-Convex Path (Ear-Clipping)

1. Project 3D polygon vertices onto the face plane's 2D coordinate system using two basis vectors derived from the plane normal.
2. Call `earcutr::earcut()` to compute triangle indices via ear-clipping.
3. Apply existing Newell-based winding flip logic to each triangle.

Ear-clipping is O(n²) but practical for typical CAD face sizes (4-50 vertices). The `earcutr` crate is a pure Rust port of MapBox earcut, WASM-compatible, handles holes and degeneracies.

### Revolve Cap Normal Fix

After creating revolve caps, compute the Newell normal from the actual half-edge loop vertices and compare against the solid centroid direction. Use the outward-pointing direction as the stored face normal.

## Research Citations

- Meisters, G.H. (1975). "Polygons Have Ears". *American Mathematical Monthly*.
- O'Rourke, J. (1998). *Computational Geometry in C*. Cambridge University Press.
- `earcutr` crate: Pure Rust port of MapBox earcut algorithm. WASM-compatible.

## Oracle Validation

- **consistent_normals**: All triangle geometric normals must agree with stored normals (dot product > 0).
- **watertight**: Every mesh edge shared by exactly 2 triangles.
- **volume**: Divergence theorem volume within 1% of analytic expectation.
- **Euler formula**: V - E + F = 2 for all solids.
