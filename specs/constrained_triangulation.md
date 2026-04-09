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
| Multiple segments, non-crossing boundary chords | N>1 | **Batch**: build boundary polygon, partition by chords, fan-triangulate |
| Multiple segments, crossing chords | N>1 | **Crossing resolution**: compute intersection points, split segments, fan-triangulate sectors around Steiner points |
| Multiple segments, interior endpoints (no crossings) | N>1 | **Sequential fallback**: split one segment at a time |

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

For task 2c, we use a polygon-with-chords approach: build the boundary polygon
by inserting constraint endpoints along triangle edges (sorted by parametric t),
then partition the polygon by non-crossing interior chords, and fan-triangulate
each sub-polygon. This is simpler than full CDT and sufficient because all
constraint points from tri-tri intersection lie on the triangle boundary.

When constraint segments cross (e.g., from different box edges meeting at a
shared corner) or have interior endpoints (e.g., blade triangle crossings),
the algorithm falls back to sequential splitting.

---

## 7b. Chord Non-Crossing Proof

**Claim**: For a triangle T with constraint segments from tri-tri intersections,
if all segment endpoints lie on T's boundary, then the corresponding polygon
chords do not cross, provided that any geometric crossing point has been
resolved as a shared vertex.

**Proof sketch**: A crossing of two chords (a,b) and (c,d) on the boundary
polygon implies the corresponding constraint segments S1 and S2 intersect in
the triangle interior. Such an intersection point P must be a vertex shared
by three triangles (T, T1, T2 where S1 comes from T-T1 intersection and S2
from T-T2). In the tri-tri intersection computation, P would be detected as
a vertex of both segments, splitting each segment at P. After this splitting,
the resulting half-segments no longer cross.

**Consequence**: When no pre-splitting has occurred (e.g., the constraint
segments come directly from pairwise tri-tri intersection without resolving
tri-tri-tri meeting points), chords CAN cross. The batch algorithm detects
this case and resolves it via crossing-chord resolution (see §7c).

**Invariant**: All constraint edges survive simultaneously in the batch output.
No constraint edge is lost by a later split overwriting an earlier one.

---

## 7c. Crossing-Chord Resolution

When two constraint chords cross on the boundary polygon, their underlying
3D segments intersect inside the triangle. This intersection point is a
**Steiner point** — an interior vertex that must exist in any valid
constrained triangulation.

**Algorithm** (implemented in `subdivide_triangle_batch`):

1. **Detect crossing pairs**: For each pair of chords, check if their polygon
   endpoint intervals interleave (existing logic from §7b).

2. **Compute intersection points**: Use `segment_segment_intersect_3d` to find
   the parametric intersection `(s, t)` of crossing 3D segments. Both `s` and
   `t` must be strictly interior `(eps, 1-eps)`.

3. **Add Steiner vertices**: Create new vertex at the intersection point with
   nanometer-scale dedup (`QUANT_NANOMETER_SCALE`). Split both crossing
   segments at the intersection vertex.

4. **Iterate**: Repeat until no crossing pairs remain. Each iteration resolves
   at least one crossing, so convergence is guaranteed (bounded by
   `MAX_CROSSING_DEPTH = 20`).

5. **Rebuild boundary polygon**: Re-classify all endpoints. Steiner points
   are classified as `Interior`. Boundary polygon contains only edge/vertex
   endpoints.

6. **Fan-triangulate sectors**: For each Steiner point X:
   - Find all boundary polygon vertices connected to X via segments
   - Sort by polygon index
   - For each consecutive pair `(p_i, p_{i+1})`, build sub-polygon
     `[X, polygon[p_i], ..., polygon[p_{i+1}]]`
   - Fan-triangulate from X

**Invariant**: After crossing resolution, the resulting segment set has zero
crossings. All original constraint edges are reconstructable as chains through
the Steiner points.

**Complexity**: For k crossing pairs, the algorithm creates at most k Steiner
points and 2k additional sub-segments. The fan triangulation from each Steiner
point is O(n) where n is the polygon size.

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
