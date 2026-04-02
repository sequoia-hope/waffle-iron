# Spec: Conformal Winding Number Classification for Boundary Sub-Triangles

## 1. Goal

Fix winding number misclassification of sub-triangles near the mesh boolean
intersection boundary, restoring correct volume, manifold topology, and Euler
characteristic for the exact mesh boolean pipeline.

Currently, `label_cells` evaluates the generalized winding number at each
sub-triangle's centroid. For sub-triangles near the intersection boundary,
the centroid lies very close to the other mesh's surface, producing winding
numbers ≈ 0.5 — right at the classification threshold. This causes
approximately half of boundary sub-triangles to be misclassified, producing
incorrect boolean results (wrong volume, non-manifold edges, V-E+F ≠ 2).

## 2. Parameters

| Parameter | Value | Unit | Description |
|-----------|-------|------|-------------|
| `WINDING_OFFSET_EPS` | 1e-8 | meters | Normal offset distance for boundary sub-triangles |
| `WINDING_INSIDE_THRESHOLD` | 0.5 | dimensionless | Classification threshold (existing) |

## 3. Branch Table

| Sub-triangle type | Classification method | Expected behavior |
|-------------------|----------------------|-------------------|
| Non-intersected parent | Centroid winding number (unchanged) | Standard GWN classification |
| Boundary (parent was split) | Normal-offset winding number | Offset centroid along parent face normal by ±ε, pick the side with clearer signal |

A sub-triangle is "boundary" if it is a child of a parent triangle that was
split by at least one constraint segment (i.e., the parent has >1 sub-triangles
in the subdivision).

## 4. Invariants

- **I1**: Volume accuracy — signed volume of Union(box_A, box_B) within 0.01 of
  analytical value (14.046875 for the standard test case).
- **I2**: Manifold — every edge in the result mesh is shared by exactly 2 triangles.
- **I3**: Euler characteristic — V-E+F = 2 for genus-0 closed manifold result.
- **I4**: Backward compatibility — non-boundary sub-triangles are classified
  identically to the current implementation.
- **I5**: Area conservation — total area of selected triangles is unchanged
  (same triangles are selected, just classified correctly).

## 5. Oracles

- Signed volume within 0.01 of analytical expected value (existing test assertion).
- Manifold edge count (every edge shared by exactly 2 triangles).
- Euler characteristic V-E+F = 2 for Union and Intersect.
- No degenerate triangles in result (existing passing test).

## 6. Failure Modes

- **Near-coincident meshes**: If both meshes share a face (touching), the offset
  may push the evaluation point into the wrong cell. This is a known limitation
  and is addressed separately by the edge-on-plane detection (Category B).
- **Concave parent face**: If the parent face is concave (only possible with
  fine tessellation), the normal may point inward. Mitigated by using original
  face normal, which is always outward for properly oriented input meshes.

## 7. Research Basis

- **Ref #7**: Jacobson et al. 2013 — Generalized winding numbers. Standard
  approach for inside/outside classification.
- **Ref #24**: Yang et al. 2025 — Hybrid B-Rep/mesh boolean pipeline. Uses
  cell labeling via winding numbers; recommends evaluating at points away from
  the mesh surface.
- **Ref #9**: Cherchi et al. 2020 — Mesh arrangements. Uses exact predicates
  for cell labeling; our offset approach approximates this.

### 7a. Analytical vs. Approximate Method Justification

- **Method**: Approximate (offset-based floating-point winding number).
- **Justification**: Exact winding number evaluation at boundary points requires
  symbolic computation (e.g., indirect predicates from Ref #9). The offset
  approach achieves equivalent correctness for non-degenerate cases with
  significantly simpler implementation. The offset is small enough (1e-8) to
  stay within the correct cell for any non-touching geometry.
- **Surface pair coverage**: N/A — this is classification logic, not SSI.

## 8. Algorithm

```
for each sub-triangle T in mesh A:
    if parent(T) has only 1 sub-triangle:
        // Non-intersected: use standard centroid classification
        label = classify(winding_number(centroid(T), mesh_B))
    else:
        // Boundary: use normal-offset classification
        c = centroid(T)
        n = outward_normal(parent_face(T))  // computed from parent triangle vertices
        w_plus  = winding_number(c + ε*n, mesh_B)
        w_minus = winding_number(c - ε*n, mesh_B)
        // Pick the side further from the ambiguous threshold
        if |w_plus - 0.5| > |w_minus - 0.5|:
            label = classify(w_plus)
        else:
            label = classify(w_minus)
```

Same algorithm applied symmetrically for mesh B sub-triangles against mesh A.
