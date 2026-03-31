# Yang Pipeline — Cell Labeling via Generalized Winding Numbers

**Pipeline position**: Phase 2, Task 2d of `specs/yang_hybrid_migration.md`

**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6 (stage 2 of hybrid pipeline)

---

## 1. Goal

After mesh subdivision (task 2c), classify each sub-triangle as inside or outside
each input solid. This determines which sub-triangles survive the boolean operation.
The cell labeling replaces S-H polygon clipping face classification entirely.

---

## 2. Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `subdivided` | `&SubdividedMesh` | Output of `subdivide_mesh_pair` |
| `original_tris_a` | `&[[usize; 3]]` | Original mesh A triangles (for GWN of B) |
| `original_tris_b` | `&[[usize; 3]]` | Original mesh B triangles (for GWN of A) |
| `verts_a` | `&[[f64; 3]]` | Original mesh A vertices |
| `verts_b` | `&[[f64; 3]]` | Original mesh B vertices |
| `op` | `BooleanOp` | Union, Subtract, or Intersect |

No tolerance parameters — GWN threshold is fixed at 0.5 per [#7] Jacobson.

---

## 3. Branch Table

| Operation | Keep from A | Keep from B | Flip B normals |
|-----------|-------------|-------------|----------------|
| Union     | outside B   | outside A   | no             |
| Subtract  | outside B   | inside A    | yes            |
| Intersect | inside B    | inside A    | no             |

---

## 4. Invariants

1. **Conservation**: For union, the total surface area of selected triangles ≥
   max(area_A, area_B) (no lost surface).

2. **Complementarity**: For any sub-triangle, inside + outside = all. No triangle
   is labeled as both or neither (except boundary cases handled by threshold).

3. **Consistency**: Sub-triangles from the same original face that don't cross
   an intersection boundary must all receive the same label.

4. **Winding correctness**: GWN at centroid of any sub-triangle fully inside a
   closed solid ≈ 1.0 (within 0.1). GWN at centroid of any sub-triangle fully
   outside ≈ 0.0.

---

## 5. Oracles

- **Two-box union**: 8-vertex axis-aligned boxes overlapping. Union result has
  all exterior sub-triangles. Count of selected triangles > count from either
  single box.
- **Two-box subtract**: A minus B. A's exterior sub-triangles that don't overlap B
  are kept. B's interior sub-triangles (inside A) are kept with flipped normals.
- **Two-box intersect**: Only sub-triangles in the overlap region survive.
  Selected triangle centroids are all within both boxes' bounding volumes.
- **Non-overlapping boxes**: Union keeps all triangles from both.
  Subtract keeps all of A. Intersect keeps nothing.

---

## 6. Failure Modes

- **Degenerate sub-triangle**: Zero-area sub-triangle has undefined centroid.
  Skip with a warning (label as Outside).
- **Centroid on boundary**: GWN ≈ 0.5. Threshold at 0.5 — classify as inside
  (≥ 0.5 is inside). This is consistent with the "closed surface includes
  boundary" convention.
- **Open mesh input**: GWN is unreliable for non-closed meshes. Precondition:
  inputs must be closed manifold meshes.

---

## 7. Research Basis

- [#7] Jacobson, Kavan & Sorkine-Hornung (2013) — Robust inside-outside
  segmentation using generalized winding numbers. The GWN is smooth, handles
  non-manifold and open meshes gracefully, and requires no tolerance parameters.
- [#24] Yang, Jia & Yan (2025) — Cell labeling step of hybrid B-Rep/mesh
  boolean pipeline. Uses winding number vectors to classify mesh cells.
- [#4] Shewchuk (1997) — Adaptive precision predicates for robust solid angle
  computation in the GWN evaluation.

### 7a. Analytical vs. Approximate Method Justification

**Method**: Exact topology via mesh winding numbers.

This is not a mesh approximation — it's an exact topological classification.
The winding number determines inside/outside with mathematical certainty for
closed meshes. The mesh is used as a computational tool per A15.6, not as
the final geometric representation. Surface geometry is preserved through the
bijective mapping (Phase 1) and refined via SSI solvers (Phase 4).
