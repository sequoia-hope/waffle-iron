# Spec: Chained Boolean Volume Fix

## Goal

Fix chained boolean operations (A∪B∪C, A∪B−C, etc.) to correctly preserve all
geometry from prior boolean results. Currently, chaining booleans loses geometry:
A∪B∪C produces ~1 operand's volume instead of ~3 for disjoint operands.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| solid_a | KernelSolidHandle | First boolean operand (may itself be a boolean result) |
| solid_b | KernelSolidHandle | Second boolean operand |
| op | BoolOp | Union, Subtract, or Intersect |

No new user-facing parameters. This is an internal pipeline fix.

## Branch Table

| Solid A Type | Solid B Type | Extraction Method A | Extraction Method B |
|-------------|-------------|-------------------|-------------------|
| Primitive cylinder | Any | cylinder_to_face_polys (from CylinderParams) | Standard dispatch |
| Primitive box (≤6 faces) | Any | extract_face_polys (B-Rep walk) | Standard dispatch |
| Sphere/Cone | Any | generate_analytic_face_polys | Standard dispatch |
| Polygon-soup (boolean result) with cached_face_polys | Any | **cached_face_polys** (NEW) | Standard dispatch |
| Polygon-soup without cached polys | Any | extract_face_polys (B-Rep walk) | Standard dispatch |
| Non-polygon-soup post-boolean | Any | extract_face_polys (B-Rep walk) | Standard dispatch |

The fix adds ONE new branch: polygon-soup solids with cached face polygons use the
cached data instead of attempting B-Rep walk.

## Invariants

1. **Volume preservation**: For N disjoint solids unioned in sequence, the final
   volume ≈ sum of individual volumes (within polygon approximation tolerance, ~10%).
   - Oracle: `mesh_volume(A∪B∪C) ≈ vol(A) + vol(B) + vol(C)` for disjoint operands

2. **Face count monotonicity**: Chained union of N disjoint solids produces ≥ sum
   of individual face counts (boolean adds no faces for disjoint operands).
   - Oracle: `face_count(result) >= face_count(A) + face_count(B)`

3. **Idempotency of cached polys**: Using cached_face_polys produces the same or
   better boolean result as re-extracting from B-Rep, because cached polys are the
   exact geometry that produced the solid (no quantization loss).

4. **No regression**: Existing 606 kernel tests continue to pass. Single-step
   booleans (non-chained) are not affected.

## Oracles

1. **Volume oracle**: For 3 disjoint cylinders (r=2, h=5, spacing > 4):
   - Expected: V ≈ 3 × π×4×5 ≈ 188.5
   - Tolerance: ±20% (polygon approximation)
   - Currently produces: ~63 (1 cylinder volume)

2. **Triangle count oracle**: Chained union mesh has triangles from all operands.
   - Expected: tri_count ≥ 3 × single_cyl_tri_count × 0.5

3. **Three-box chained union volume**: 3 disjoint 10×10×10 boxes:
   - Expected: V ≈ 3000
   - Tolerance: ±10%

4. **Chained subtract volume**: box − cyl1 − cyl2 (disjoint cuts):
   - Expected: V ≈ vol(box) − vol(cyl1) − vol(cyl2)
   - Tolerance: ±15%

## Failure Modes

1. **No cached_face_polys on boolean result**: Should not happen — `boolean_op`,
   `boolean_op_tolerant`, and `polygon_approx_boolean` all populate this field.
   If missing, falls through to existing B-Rep walk (current behavior, no regression).

2. **Stale cached polys**: If a solid is mutated after the boolean (not currently
   possible since WaffleSolid is immutable once stored), cached polys could be wrong.
   Mitigation: solids are never mutated after insertion.

3. **Large cached poly sets**: Boolean of two complex solids may cache many face
   polygons. Memory is bounded by the number of faces in the result (same as the
   B-Rep arena). No performance concern.

## Research Basis

- [#24] Barton et al. — Hybrid B-Rep/mesh boolean pipeline. The cached face polygons
  correspond to the "bijective mesh extraction" step. Re-extracting from the re-mapped
  B-Rep loses the original mesh quality.
- [#33] Stroud §6.1 — Stepwise boolean assembly. Chained booleans are fundamental to
  the CAD workflow (sketch → extrude → union → extrude → subtract).

### Analytical vs. Approximate Method Justification

- **Method**: Approximate (polygon clipping via Sutherland-Hodgman).
- **Justification**: This fix does not change the SSI method — it changes which polygon
  data is fed into the existing boolean pipeline. The polygon path is used for post-boolean
  solids because they are complex multi-surface objects (not simple quadric primitives).
  Per A15.2, the mesh fallback exists for "freeform surfaces that lack closed-form SSI."
  Post-boolean solids with mixed/split surfaces are analogous to freeform.
- **Surface pair coverage**: The fix preserves surface_geom annotations on cached face
  polygons, maintaining A15.5 compliance (surface tier preservation for unmodified faces).
