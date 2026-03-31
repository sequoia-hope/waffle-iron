# Yang Pipeline Task 2e — Radial Sort for Non-Manifold Edge Resolution

**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6
**Pipeline**: Yang 2025 hybrid B-Rep/mesh boolean, Phase 2 (exact mesh boolean)
**Predecessor**: Task 2d (cell labeling via GWN)
**Successor**: Task 2f (integration tests), Phase 3 (topology extraction)

---

## 1. Goal

Implement radial sorting of triangles around non-manifold edges produced by
mesh boolean subdivision. When two meshes are intersected, the intersection
edges become non-manifold: 4 or more triangles meet at a single edge. The
radial sort determines their angular ordering around the edge axis, which is
required for:

- **Phase 3 topology extraction**: determining which sub-triangles are adjacent
  in the arrangement, enabling half-edge B-Rep construction
- **Manifold output**: pairing triangles into cells (volumetric regions) so that
  boolean selection produces a manifold 2-manifold boundary
- **Replacing deprecated tolerance-based edge pairing** (stitch.rs)

The radial sort uses exact orient3d predicates — no tolerance parameters.

## 2. Research Basis

- **[#10] Levy 2025**: Exact constructions + radial sort for mesh arrangements.
  Triangles around a non-manifold edge are sorted by angular position using
  the sign of orient3d(edge_start, edge_end, v_i, v_j) where v_i, v_j are
  the opposite vertices of adjacent triangles. This is exact (no atan2, no
  tolerance).
- **[#9] Cherchi 2020**: Indirect predicates — the radial sort integrates with
  indirect point representation from task 2b.
- **[#12] Barki 2015**: Radial sort for classification in mesh arrangements —
  similar approach using orient predicates for angular ordering.
- **[#4] Shewchuk 1997**: Adaptive precision orient3d used as the comparison primitive.

## 3. Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `edge` | `[usize; 2]` | Vertex indices of the non-manifold edge endpoints |
| `triangles` | `&[RadialTriangle]` | Triangles meeting at the edge, each with its opposite vertex and mesh origin |
| `verts` | `&[[f64; 3]]` | Shared vertex position array |

### `RadialTriangle` fields

| Field | Type | Description |
|-------|------|-------------|
| `opposite_vertex` | `usize` | Index of the vertex NOT on the shared edge |
| `mesh_id` | `MeshId` | Which input mesh (A or B) this triangle came from |
| `sub_tri_index` | `usize` | Index into the SubdividedMesh's tris_a or tris_b |

### Return type

`Vec<usize>` — indices into the input `triangles` slice, sorted in CCW angular
order around the edge axis (from edge[0] toward edge[1]).

## 4. Branch Table

| # | Condition | Behavior |
|---|-----------|----------|
| B1 | 0 or 1 triangles | Return input order unchanged (no sort needed) |
| B2 | 2 triangles | Return as-is if from different meshes; compare via orient3d if same mesh |
| B3 | 4 triangles (standard boolean edge) | Full radial sort — typical case for mesh boolean |
| B4 | N > 4 triangles (multiple intersections at same edge) | Full radial sort — generalizes to N |
| B5 | Two triangles with same opposite vertex (degenerate) | Coplanar pair — use mesh_id to break tie |
| B6 | Opposite vertex coincident with edge endpoint | Degenerate — triangle is zero-area; exclude from sort |

## 5. Invariants

- **I1 — Angular ordering is consistent**: For any three consecutive triangles
  T_i, T_j, T_k in the sorted output, orient3d(edge[0], edge[1], v_i, v_j) and
  orient3d(edge[0], edge[1], v_j, v_k) must have consistent sign (both indicate
  same rotational direction).

- **I2 — All input triangles appear in output**: The output permutation is a
  bijection of the input indices (no triangles lost or duplicated), except for
  B6 degenerates which are excluded.

- **I3 — Mesh alternation at standard boolean edges**: For a standard 4-triangle
  boolean edge (2 from A, 2 from B), the sorted order alternates: A, B, A, B
  (or B, A, B, A). This is a necessary property of the mesh arrangement at a
  transversal intersection.

- **I4 — No tolerance parameters**: The sort comparison uses only orient3d
  (exact) — no epsilon, no threshold, no tolerance constant.

## 6. Oracles

- **O1**: For axis-aligned test cases, the angular order can be verified
  analytically (e.g., triangles in the +x, +y, -x, -y quadrants around a
  z-axis edge).
- **O2**: Invariant I3 (A-B alternation) is checkable for standard 4-triangle
  boolean edges.
- **O3**: The sorted list forms a valid cyclic permutation: applying the sort
  twice yields the identity.
- **O4**: orient3d(edge[0], edge[1], v_sorted[i], v_sorted[i+1]) has consistent
  sign for all consecutive pairs (I1).

## 7. Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| Edge endpoints are coincident (zero-length edge) | Return input order unchanged (cannot define angular ordering) |
| All opposite vertices are collinear with edge | Return input order unchanged (degenerate — all triangles coplanar) |
| Opposite vertex coincides with an edge endpoint | Exclude that triangle from sort, include in output at end |

## 8. Algorithm

The radial sort uses orient3d as a comparison predicate:

```
radial_compare(edge, v_i, v_j) → Ordering:
    // Are v_i and v_j on the same side of the edge?
    let o = orient3d(edge[0], edge[1], v_i, v_j)
    // This gives the signed volume of the tetrahedron.
    // Positive → v_j is CCW from v_i (around edge axis)
    // Negative → v_j is CW from v_i
    // Zero → v_i and v_j are coplanar with edge (need tiebreak)
```

For the coplanar case (orient3d == 0), use a reference plane and orient2d:
1. Pick a reference direction perpendicular to the edge
2. Project opposite vertices onto plane perpendicular to edge
3. Use atan2 of projected coordinates (or orient2d chain) to break ties

Full sort: insertion sort (N is small, typically 4) using `radial_compare`.

## 9. File Location

`crates/kernel/src/boolean/exact_mesh.rs` — extends the existing Phase 2 module.
