# Spec: Triangle-Triangle Intersection (Yang Pipeline Phase 2, Task 2b)

**Status**: Active
**Module**: `crates/kernel/src/boolean/exact_mesh.rs`
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6, ENGINEERING_CONSTITUTION.md P8

---

## 1. Goal

Implement exact triangle-triangle intersection detection and intersection
segment computation using Shewchuk adaptive predicates. The output uses
**indirect points** (Cherchi 2020 §4.1, [#9]) — symbolic references to input geometry —
so that downstream predicates can evaluate exactly without materializing
floating-point coordinates.

This is stage 2 infrastructure of the Yang 2025 hybrid B-Rep/mesh boolean
pipeline. It replaces tolerance-based polygon clipping with exact predicates.

---

## 2. Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `tri_a` | `[usize; 3]` | Vertex indices of triangle A in the vertex array |
| `tri_b` | `[usize; 3]` | Vertex indices of triangle B in the vertex array |
| `verts` | `&[[f64; 3]]` | Shared vertex position array |

No tolerance parameters. All decisions are exact via `orient3d`.

---

## 3. Branch Table

| Config | s_a (T_A verts vs plane(T_B)) | s_b (T_B verts vs plane(T_A)) | Result |
|--------|-------------------------------|-------------------------------|--------|
| Separated-A | all same non-zero sign | — | `None` |
| Separated-B | — | all same non-zero sign | `None` |
| Coplanar | all zero | all zero | `Coplanar` |
| Crossing | mixed signs | mixed signs | `Segment(p1, p2)` |
| Edge-touch | one zero, two same sign | mixed or one zero | `Point(p)` or `None` |
| Vertex-on-plane | one zero, two same sign | all same non-zero | `None` (vertex touches plane but triangles don't overlap) |
| Shared-edge | two zeros | two zeros | `Coplanar` or `Segment` depending on geometry |

Key insight: the exact orient3d predicate makes the zero/nonzero distinction
reliable — there are no "near zero" ambiguities.

---

## 4. Invariants

1. **No tolerance**: No epsilon, tau, or tolerance parameter anywhere in the
   intersection computation. All decisions use exact orient3d signs.

2. **Indirect point validity**: Each `IndirectPoint` references valid edge
   endpoints and a valid plane triangle. The edge must actually cross the
   plane (one endpoint above, one below or on).

3. **Symmetry**: `intersect(A, B)` and `intersect(B, A)` must report the
   same intersection type (None, Coplanar, Point, or Segment). The indirect
   points may differ in representation but describe the same geometric point.

4. **Segment orientation**: When a Segment is returned, the two indirect
   points are distinct (they describe different geometric locations).

5. **Materialization consistency**: When an indirect point is materialized
   to floating-point coordinates (for visualization/debugging), the result
   must lie on both the edge and the plane it references.

---

## 5. Oracles

| Oracle | Method |
|--------|--------|
| Non-intersection correctness | For separated triangles, verify `None` returned |
| Segment existence | For known-intersecting axis-aligned triangles, verify `Segment` returned |
| Materialized point on edge | Compute `p = a + t*(b-a)`, verify `0 <= t <= 1` |
| Materialized point on plane | Compute `orient3d(plane_tri, point)`, verify ≈ 0 |
| Point count | Crossing configs produce exactly 2 points; touching produces 0 or 1 |
| Symmetry | `intersect(A,B).type == intersect(B,A).type` |

---

## 6. Failure Modes

| Condition | Behavior |
|-----------|----------|
| Degenerate triangle (zero area) | Undefined — caller must filter degenerate triangles before calling |
| Coplanar triangles | Return `Coplanar` variant — actual 2D intersection is deferred to task 2c |
| Vertex indices out of bounds | Panic (debug assert) — caller responsibility |

---

## 7. Research Basis

- **[#4] Shewchuk 1997**: Adaptive precision `orient3d` predicate. Provides
  exact sign computation for point-plane orientation. Used for all
  classification decisions.

- **[#9] Cherchi et al. 2020 §4.1 (indirect predicates)**: Defines the
  `ImplicitPoint3T_LPI` (Line-Plane Intersection) indirect point type. Our
  `IndirectPoint` is a simplified version storing vertex indices rather than
  coordinate arrays, since we operate on indexed meshes. (Reused in the
  [#38] Cherchi 2022 pipeline that Yang 2025 cites.)

- **Guigue & Devillers 2003**: "Fast and Robust Triangle-Triangle Overlap
  Test Using Orientation Predicates." Defines the classification-based
  algorithm for determining triangle-triangle intersection using orient3d.
  Our implementation follows this structure.

- **[#24] Yang, Jia & Yan 2025**: Hybrid B-Rep/mesh boolean pipeline.
  Triangle-triangle intersection is the core primitive of stage 2 (exact
  mesh boolean).

### 7a. Analytical vs. Approximate Method Justification

- **Method**: Exact (orient3d adaptive predicates).
- **Justification**: This operates on mesh triangles, not surface-surface
  intersection. The exact predicates guarantee correct topology. No
  approximation is involved.
- **Surface pair coverage**: N/A — this is mesh-level, not surface-level.
  Surface-level SSI is handled by A15.4 solvers in pipeline stage 4.
