# Constrained Triangulation of Intersected Faces

**Yang Pipeline Phase 2, Task 2c**

**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6
**Migration plan**: `specs/yang_hybrid_migration.md` task 2c

---

## 1. Goal

Given a pair of triangle meshes and the set of triangle-triangle intersection
segments computed in task 2b, subdivide each intersected triangle along those
segments. The result is a refined mesh where every intersection segment is an
edge. Each sub-triangle inherits the bijective mapping (source B-Rep face) of
its parent triangle.

This is stage 2 infrastructure in the Yang 2025 hybrid pipeline [#24]:
tessellation → **subdivision** → cell labeling → topology extraction.

---

## 2. Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `verts_a` | `&[[f64; 3]]` | Vertex positions of mesh A |
| `tris_a` | `&[[usize; 3]]` | Triangle index triples of mesh A |
| `verts_b` | `&[[f64; 3]]` | Vertex positions of mesh B |
| `tris_b` | `&[[usize; 3]]` | Triangle index triples of mesh B |

---

## 3. Branch Table

| Case | # Segments on Triangle | Behavior |
|------|----------------------|----------|
| No intersection | 0 | Triangle passes through unchanged |
| Single segment, endpoints on 2 different edges | 1 | Split into 3 sub-triangles |
| Single segment, one endpoint on vertex | 1 | Split into 2 sub-triangles |
| Single segment, both endpoints on vertices | 1 | No split needed (edge already exists) |
| Multiple segments | N>1 | Iterative or batch subdivision via ear-clipping |

---

## 4. Invariants

1. **Coverage**: The union of all sub-triangles for a parent triangle covers
   the parent exactly (same total area, same boundary).
2. **Constraint satisfaction**: Every intersection segment is an edge in the
   subdivided mesh. No segment crosses the interior of any sub-triangle.
3. **Bijective inheritance**: `sub_tri.parent_tri` maps to the correct
   original triangle index. Through the BijectiveMap, this maps to the
   correct source B-Rep face.
4. **Vertex count**: For a triangle with K constraint segments having D
   distinct endpoints on edges (not at vertices), the subdivided vertex set
   has 3 + D vertices.
5. **Triangle count**: For a single segment with both endpoints on edges
   (not at vertices), the result is 3 sub-triangles. For a single segment
   with one endpoint at a vertex, 2 sub-triangles.

---

## 5. Oracles

- **Area conservation**: Sum of sub-triangle areas == parent triangle area
  (within f64 precision, ~1e-12 relative).
- **Sub-triangle count**: For single-segment cases, verify exact count per
  branch table.
- **Constraint edge presence**: For each intersection segment, verify that
  both endpoints appear as vertices in the subdivision and that an edge
  connects them.
- **No degenerate triangles**: All sub-triangles have positive area.

---

## 6. Failure Modes

| Condition | Behavior |
|-----------|----------|
| Degenerate parent triangle (zero area) | Skip subdivision, pass through |
| Segment endpoints coincide | Treat as point intersection, no split |
| Segment lies along an existing edge | No split needed |

---

## 7. Research Basis

- [#24] Yang, Jia & Yan (2025) — Pipeline architecture: subdivision follows
  exact tri-tri intersection.
- [#9] Cherchi et al. (2020) — Mesh arrangement: constrained subdivision of
  triangles is the "arrangement computation" step. Their approach uses
  constrained Delaunay triangulation (CDT) within each triangle.
- [#10] Levy (2025) — Exact constructions for subdivision point placement.

For task 2c, we use a simpler ear-clipping approach within each triangle
rather than full CDT, since all constraint points lie on the triangle boundary
(guaranteed by tri-tri intersection). CDT provides no benefit when all
constraints connect boundary points of a convex polygon.

---

## 7a. Analytical vs. Approximate Method Justification

The subdivision uses materialized f64 coordinates for the intersection points
(via `materialize_ip` from task 2b). This is acceptable because:
1. The topology (which triangles are split, how they connect) is determined
   by exact orient3d predicates from task 2b.
2. The materialized positions are only used for geometric placement of
   sub-triangle vertices, not for topological decisions.
3. In stage 4 (SSI refinement), these approximate positions will be replaced
   with exact analytical intersection curves.
