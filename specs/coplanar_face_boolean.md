# Coplanar Face Handling in Planar-Planar Boolean Union

**Status**: Bug fix spec
**References**: [#16] Mantyla (Euler operators), [#33] Stroud Ch.4 (B-Rep topology), [#24] Barton et al. (hybrid boolean pipeline)
**Governance**: P3 (test-first), P9-P10 (fix it right or don't fix it), A15.5 (surface tier preservation)

---

## 1. Goal

Fix the union of two stacked axis-aligned box extrusions (the simplest multi-feature CAD operation). Currently, `planar_planar_boolean` produces non-manifold meshes with unpaired edges when two boxes share a coplanar face (e.g., Box A's top face at z=0.3 touches Box B's bottom face at z=0.3).

Assay cases affected: F0001 (identical rectangles), F0002 (small scale), F0003+ (various scales and profiles).

## 2. Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| solid_a | WaffleSolid | First operand (all-planar box) |
| solid_b | WaffleSolid | Second operand (all-planar box, stacked on A) |
| op | BoolOp::Union | Union operation |

Inputs flow through `do_boolean` → `planar_planar_boolean` → `collect_union_fragments` → `build_brep_from_polygons`.

## 3. Branch Table

| Case | A face vs B | Classification | Expected Union Behavior |
|------|-------------|---------------|------------------------|
| Interior coplanar (anti-parallel) | A top (+Z) vs B bottom (-Z), fully overlapping | CoplanarTouching | Remove both faces (interior boundary) |
| Interior coplanar (anti-parallel, partial) | A side vs B side, partially overlapping | AntiParallelCoplanarPartial | Remove overlap region, keep outside frags |
| Same-direction coplanar (full overlap) | Identical faces | CoplanarPartial (empty outside_frags) | Primary: emit face; Secondary: emit nothing |
| Same-direction coplanar (partial overlap) | Partially overlapping same-direction faces | CoplanarPartial (non-empty outside_frags) | Primary: emit full face; Secondary: emit outside frags |
| Non-coplanar outside | A bottom vs all B faces | Outside | Keep face |
| Non-coplanar inside | A face fully inside B volume | Inside | Remove face |

## 4. Invariants

1. **Euler characteristic**: Union of two genus-0 solids without through-holes must satisfy V - E + F = 2
2. **Watertight**: Zero unpaired edges (every half-edge has a twin)
3. **No non-manifold edges**: Every edge shared by exactly 2 faces
4. **Volume conservation**: Volume(A ∪ B) ≤ Volume(A) + Volume(B)
5. **Face count**: Stacked boxes union → ≤ 10 faces (5 from A + 5 from B, shared face removed)

## 5. Oracles

| Oracle | How to check |
|--------|-------------|
| Euler formula | Count V, E, F in result mesh; assert V - E + F = 2 |
| Watertight | Assert zero unpaired edges in half-edge structure |
| Manifold | Assert no edge shared by >2 faces |
| Bounding box | Result bbox = union of A and B bboxes |
| Triangle count minimum | Result mesh has ≥ (result_faces × 2) triangles |

## 6. Failure Modes

| Failure | Current behavior | Expected behavior |
|---------|-----------------|-------------------|
| Non-manifold edges from coplanar face duplication | V=24, E=30, F=24, χ=18 | V-E+F=2, watertight |
| Strict stitch rejects non-manifold | Falls to boolean_op_tolerant | Should not need fallback |
| Coplanar detection fails at boundary | Face classified as Outside instead of CoplanarTouching | Correct coplanar classification |

## 7. Research Basis

- **[#16] Mantyla**: Face merge during boolean via Euler operator kef (kill edge, face) — the standard approach for eliminating shared coplanar faces
- **[#33] Stroud Ch.4**: Half-edge topology invariants that must be preserved during face removal
- **[#24] Barton et al.**: Hybrid boolean pipeline handles coplanar faces by detecting face-pair overlap before polygon clipping
- **Parasolid [#36]**: Coplanar face fusion: same-direction pairs merged, anti-parallel pairs eliminated

### 7a. Analytical vs. Approximate Method Justification

- **Method**: Exact (all surfaces are planar — Tier 1 analytic)
- **Surface pair coverage**: Plane-Plane only. Exact SSI per A15
- **No mesh fallback needed**: Both operands are all-planar polyhedra

## 8. Diagnosis and Fix Strategy

### Root Cause Hypothesis

The bug is in the interaction between `classify_face` and `collect_union_fragments` for the stacked-box case. When two boxes share an anti-parallel coplanar face (A's top at z=0.3 with normal +Z, B's bottom at z=0.3 with normal -Z):

1. `classify_coplanarity` should detect `AntiParallel`
2. The S-H clip of A's top face against B should return inside_area ≈ original_area
3. `classify_face` should return `CoplanarTouching`
4. `collect_union_fragments` should discard both faces

If any step fails (e.g., S-H clip produces inside_area = 0 due to the face being exactly ON B's boundary plane), the face gets classified as `Outside` instead of `CoplanarTouching`, and BOTH stacked boxes' full face sets appear in the output.

### Fix Approach

1. Add a dedicated test that directly calls `planar_planar_boolean` on two stacked boxes
2. Instrument the classification to understand what F0001's faces actually get classified as
3. Fix the root cause: either tighten the coplanar tolerance or add an explicit check for on-boundary coplanar faces before S-H clipping
4. Verify the fix resolves F0001's mesh invariant violations
