# Yang Global Edge Conformality

## Problem

The current per-triangle mesh arrangement (`triangulate_single_triangle`) processes
each triangle independently without sharing edge constraint points between adjacent
triangles. When two triangles share an original mesh edge and an intersection point
falls on that edge, only the triangle whose intersection was detected receives the
point. The neighbor gets a different subdivision of the shared edge, producing
non-conformal boundaries and unpaired half-edges downstream.

This is the root cause of ~100/148 Yang boolean watertight failures.

## Reference

Cherchi et al. 2020 C++ reference (`aux_structure.h:190`, `intersection_classification.cpp:464`)
uses a **global edge-centric architecture**:

1. Every shared edge gets ONE global ID (canonicalized as `(min(v0,v1), max(v0,v1))`)
2. Constraint points are stored **per-edge** (not per-triangle)
3. When processing triangles T1 and T2 that share edge E, BOTH fetch edge points
   from the SAME `edge2pts[E]` list
4. Result: both triangles have IDENTICAL boundary vertices on shared edges
5. No post-processing stitching needed — conformality by construction

## Solution

### Step 1: `build_global_edge_points_map`

After intersection detection produces `constraints_a` and `constraints_b` (per-triangle
constraint segments), scan all constraint segment endpoints. For each endpoint that
lies on an original mesh edge, add it to a global map:

```
edge_points_map: HashMap<(usize, usize), Vec<usize>>
```

Key is the canonicalized edge `(min(v0,v1), max(v0,v1))`. Value is the list of
intersection point indices on that edge, sorted by parametric position along the edge.

### Step 2: `enrich_constraints_with_shared_edge_points`

For each triangle in the mesh, look up all three of its edges in the global map.
Produce an `EnrichedConstraints` that includes:
- The triangle's own constraint segments
- Edge points from the global map (even if the triangle itself had no intersection)
- Interior points (constraint endpoints not on any edge)

### Step 3: Non-intersected neighbor propagation

Triangles that share an edge with an intersected triangle but have no intersection
themselves must ALSO be subdivided at the shared edge points. Without this, the
shared edge has different subdivision on each side.

## Verification

1. `test_global_edge_map_shared_points` — Two triangles sharing an edge with an
   intersection point on it. Global map must contain the point for that edge.
2. `test_non_intersected_neighbor_gets_edge_points` — Non-intersected triangle
   sharing an edge with an intersected one receives the edge points after enrichment.
3. `test_conformality_after_enrichment` — Two overlapping boxes through
   `subdivide_mesh_pair`. Zero non-conformal edges after enrichment.
4. `test_enrichment_watertight_pipeline` — Full Yang pipeline through flood_fill.
   Zero unpaired half-edges.
