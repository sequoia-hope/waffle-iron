# Spec: Cherchi Algorithm 1 — Segment Insertion for Mesh Arrangement

## Goal

Replace the post-hoc conformal repair pipeline in `subdivide_mesh_pair()` with
Cherchi 2020 [#9] Algorithm 1 (addSegment). This guarantees conformal mesh output
by construction, eliminating the 58 watertight + 41 self-intersection failures.

## Research Basis

- Cherchi et al. 2020 [#9] Sections 5.2 (Point Insertion) and 5.3 (Segment Insertion)
- Local reference: `docs/references/cherchi-indirect-predicates-2020.md`
- Original algorithm from Shewchuk & Brown 2015 (adapted for implicit points)

## Data Structure: TriMesh

An adjacency-aware triangle mesh required by the walking algorithm:

```rust
struct TriMesh {
    verts: Vec<[f64; 3]>,             // vertex positions (materialized)
    tris: Vec<[usize; 3]>,           // triangle vertex indices
    adj: Vec<[Option<usize>; 3]>,    // tri[i].adj[j] = neighbor across edge j
    constrained: HashSet<(usize,usize)>,  // constrained edges (min,max ordering)
    parent_tri: Vec<usize>,          // original triangle index
    removed: Vec<bool>,              // soft-deleted triangles
}
```

Edge j of triangle i is the edge OPPOSITE vertex j:
- Edge 0: (verts[1], verts[2])
- Edge 1: (verts[0], verts[2])
- Edge 2: (verts[0], verts[1])

Adjacency: `adj[tri_idx][edge_idx]` = index of the triangle sharing that edge, or None.

## Algorithm 1: addSegment(v_beg, v_end)

### Preconditions
- v_beg and v_end are valid vertex indices in the TriMesh
- Both were inserted via point_insert() prior to segment insertion

### Steps

1. If (v_beg, v_end) is already a mesh edge → return (already satisfied)

2. Initialize polygons: Pl = [v_beg, v_end], Pr = [v_end, v_beg]

3. Find starting triangle t: any triangle incident to v_beg whose interior
   is crossed by the segment (v_beg → v_end). Use orient2d to test which
   adjacent triangle the segment enters.

4. Set e = edge opposite to v_beg in t, with endpoints (e0, e1)

5. WHILE current vertex v ≠ v_end:

   a. **Endpoint coincidence (e0)**: If e0 lies ON segment(v_beg, v_end)
      (collinear and between endpoints), recurse:
      addSegment(v_beg, e0); addSegment(e0, v_end); return

   b. **Endpoint coincidence (e1)**: Same for e1.

   c. **Constrained edge crossing**: If e is a constrained edge, create
      T-type intersection point v_new = segment ∩ edge e. Split e at v_new.
      Recurse: addSegment(v_beg, v_new); addSegment(v_new, v_end); return

   d. **Normal case**: Classify e0 and e1 relative to the segment using
      orient2d(v_beg, v_end, e0):
      - If e0 is left: append e0 to Pl, e1 to Pr
      - If e0 is right: append e1 to Pl, e0 to Pr

   e. **Advance**: Cross edge e to the neighbor triangle. Update v, t, e.

6. Triangulate Pl and Pr using earcut. Insert new triangles into mesh.
   Mark (v_beg, v_end) as constrained.

## Point Insertion (Section 5.2)

Before segment insertion, all intersection endpoints must be inserted:

### On-Edge Point
When a point lies on an edge shared by two triangles:
1. Split both triangles: each becomes two sub-triangles
2. Update adjacency for all 4 new triangles
3. Update constraint tracking if the split edge was constrained

### Interior Point
When a point lies inside a triangle:
1. Split the triangle into 3 sub-triangles (fan from the new point)
2. Update adjacency for all 3 new triangles

### Edge Point Sorting
Multiple points on the same edge must be sorted from one endpoint to the other
using `point_compare_on_axis()` from the indirect predicates module. Process
in sorted order: split edge at first point, then split remaining sub-edge at
next point, etc.

## Predicates Required

| Predicate | Purpose | Implementation |
|-----------|---------|---------------|
| orient2d(a, b, c) | Left/right classification | indirect_predicates::orient2d_indirect |
| point_on_segment(p, a, b) | Endpoint coincidence check | orient2d == 0 + point_compare betweenness |
| point_compare(a, b, axis) | Sort points on edge | indirect_predicates::point_compare_on_axis |
| point_in_triangle(p, t) | Locate triangle for point insertion | 3x orient2d signs |

## Invariants

1. After all segments inserted, every constraint segment is a mesh edge
2. Every edge is shared by exactly 0 (boundary) or 2 triangles
3. Adjacency is consistent: if adj[t1][e1] = t2, then adj[t2][e2] = t1 for matching edge
4. No removed triangles remain in the final output
5. Parent triangle tracking is preserved through splits

## Branch Table

| Situation during walk | Action |
|---|---|
| e0 on segment | Recurse: split segment at e0 |
| e1 on segment | Recurse: split segment at e1 |
| e is constrained | Create T-point, split e, recurse |
| e is normal, unconstrained | Classify e0/e1 to Pl/Pr, advance |
| v == v_end | Stop walk, triangulate Pl and Pr |

## Failure Modes

- Degenerate intersection (segment length ≈ 0): skip
- Walk doesn't reach v_end (mesh inconsistency): return error, fall back to old path
- Earcut fails on degenerate polygon: skip that polygon

## What Gets Deleted

After the new algorithm is verified:
- `subdivide_triangle_batch()` and helpers
- `enforce_conformal_edges()`
- `cross_mesh_subtri_conformal()`
- `subtri_conformal_repair()`
- `partition_polygon_by_chords()` and related chord logic
- All Steiner-point fan triangulation code (crossing chord resolution)

## Integration

The new code replaces only the INTERNALS of `subdivide_mesh_pair()`. The function
signature and output type (`SubdividedMesh`) remain unchanged. The caller
(`yang_boolean_pipeline` in topology_extract.rs) sees no difference.
