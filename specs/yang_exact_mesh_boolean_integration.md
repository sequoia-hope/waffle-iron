# Spec: Exact Mesh Boolean Integration Tests (Yang Pipeline Phase 2f)

**Status**: Active
**Task**: `specs/yang_hybrid_migration.md` Phase 2, Task 2f
**Author**: Manager (auto-waffle session 2026-03-31)

---

## 1. Goal

Validate the end-to-end exact mesh boolean pipeline (subdivide → label → select)
on axis-aligned box pairs for all three boolean operations (union, subtract, intersect).
Verify structural correctness using numeric oracles, not just "it produces output."

## 2. Parameters

| Parameter | Value |
|-----------|-------|
| Box A | [0,0,0] → [2,2,2], 12 triangles, outward-facing normals |
| Box B | [1,0,0] → [3,2,2], 12 triangles, outward-facing normals |
| Operations | Union, Subtract (A−B), Intersect |

## 3. Branch Table

| Operation | A cells kept | B cells kept | B flip | Expected Volume |
|-----------|-------------|-------------|--------|-----------------|
| Union | Outside B | Outside A | No | 12.0 |
| Subtract | Outside B | Inside A | Yes | 4.0 |
| Intersect | Inside B | Inside A | No | 4.0 |

## 4. Invariants

1. **Manifold (zero unpaired edges)**: Every edge in the result mesh is shared
   by exactly 2 triangles. An "edge" is an unordered pair of vertex positions.
2. **Euler characteristic**: V − E + F = 2 for a genus-0 closed manifold.
3. **Positive volume**: The signed volume (via divergence theorem) must be positive,
   indicating outward-facing normals.
4. **Volume accuracy**: Signed volume within 1e-6 of the known-correct value.
5. **No degenerate triangles**: Every triangle has area > 0.
6. **Non-empty result**: All three operations on overlapping boxes produce triangles.

## 5. Oracles

| Oracle | Method |
|--------|--------|
| Unpaired edges | Build edge→face-count map; assert all counts == 2 |
| Euler characteristic | Count unique V, E, F from result mesh; assert V−E+F == 2 |
| Volume sign | Divergence theorem: Σ (v0 · (v1 × v2)) / 6; assert > 0 |
| Volume value | Assert |computed − expected| < 1e-6 |
| No degenerates | Assert tri_area_3d > 0 for every triangle |

## 6. Failure Modes

- **Unpaired edges**: Subdivision produced gaps or the label/select step
  dropped triangles that share edges with kept triangles.
- **Wrong Euler**: Topology is broken (holes, non-manifold vertices).
- **Negative volume**: Normal orientation was flipped (subtract B flip bug).
- **Wrong volume**: Cell labeling misclassified inside/outside.

## 7. Research Basis

- [#24] Yang, Jia & Yan 2025: Hybrid B-Rep/mesh boolean pipeline
- [#9] Cherchi et al. 2020: Indirect predicates for exact mesh arrangements
- [#4] Shewchuk 1997: Adaptive precision predicates
- [#10] Levy 2025: Exact constructions + radial sort

The tests validate the mesh-level output of stages 2-3 of the Yang pipeline.
Mesh correctness (manifold, correct volume) is a prerequisite for Phase 3
(topology extraction back to B-Rep).
