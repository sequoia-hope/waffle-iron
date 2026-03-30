# Inter-Face Self-Intersection Oracle

## Problem

Current mesh oracles (watertight, Euler, winding, normals, etc.) are purely
topological or surface-orientation checks. They miss geometric defects where
triangles belonging to different B-Rep faces penetrate each other — for example,
pocket walls extending through the floor, or internal face remnants from a bad
boolean cut. R0098 (gear boss + oversized rectangle cut on a tilted plane)
reportedly exhibits such defects while passing all 8 existing oracles.

## Oracle Signature

```rust
pub fn check_no_self_intersection(mesh: &RenderMesh) -> OracleVerdict
```

- **Input**: `RenderMesh` with `face_ranges` partitioning triangles by B-Rep face.
- **Output**: `OracleVerdict` with pass/fail, count of intersecting triangle pairs,
  and the first few violating face-pair indices.

## Algorithm

1. **Partition** triangles into per-face groups using `mesh.face_ranges`.
2. **Broad phase**: Compute an AABB per face group. Only test face pairs (i < j)
   whose AABBs overlap.
3. **Narrow phase**: For each candidate face pair, test every triangle pair:
   - **Skip shared edges**: If two triangles share ≥2 quantized vertex positions,
     they are topologically adjacent — not a self-intersection.
   - **Möller triangle-triangle test** (separating-axis variant):
     a. Compute plane of triangle A; classify B's vertices against it.
     b. If all B vertices are on one side → no intersection → skip.
     c. Compute plane of triangle B; classify A's vertices against it.
     d. If all A vertices are on one side → no intersection → skip.
     e. Compute the intersection line (cross product of the two normals).
     f. Project both triangles onto that line; compute overlap intervals.
     g. If intervals overlap → intersection detected.
   - **Penetration depth filter**: Reject intersections where the signed distances
     of all vertices from the opposing plane are < `max_abs * 1e-4` (grazing
     contacts due to floating-point coincidence at shared boundaries).
4. **Early exit** after 10 violations (enough for diagnostics, avoids O(n²) blowup).
5. **Return** pass (0 violations) or fail (count + first violating face pairs).

## Tolerances

- **Vertex quantization**: Same grid as `check_watertight_mesh` (`max_abs * 1e-5`,
  floor `1e-10`) for shared-edge detection.
- **Grazing rejection**: `max_abs * 1e-4` penetration depth threshold. This is 10×
  the quantization grid, ensuring we only flag genuine penetrations.
- **AABB inflation**: None. Face AABBs are exact from triangle vertices.

## Branch Table

| Condition | Action |
|-----------|--------|
| No `face_ranges` or ≤1 face | Pass (nothing to compare) |
| Face pair AABBs disjoint | Skip pair |
| Triangle pair shares ≥2 quantized verts | Skip (adjacent) |
| All B verts on one side of A's plane | Skip (no intersection) |
| All A verts on one side of B's plane | Skip (no intersection) |
| Interval overlap but max penetration < threshold | Skip (grazing) |
| Interval overlap and penetration ≥ threshold | Count as violation |
| ≥10 violations accumulated | Early exit, report |

## Research Basis

- **Möller (1997)**: "A Fast Triangle-Triangle Intersection Test", Journal of
  Graphics Tools 2(2). The separating-axis method with interval overlap on the
  intersection line. O(1) per triangle pair, no special cases for coplanar
  triangles (treated as non-intersecting for our purposes, since coplanar faces
  from different B-Rep regions would be caught by other oracles or are legitimate
  tangent contacts).
