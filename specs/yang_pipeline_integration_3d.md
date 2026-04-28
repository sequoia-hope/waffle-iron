# Yang Pipeline Phase 3 Task 3d — Full Pipeline Integration Tests

**Reference**: [#24] Yang, Jia & Yan (2025) — Hybrid B-Rep/mesh boolean pipeline.
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6.
**Parent task**: `specs/yang_hybrid_migration.md` Phase 3, Task 3d.

---

## 1. Goal

Provide an integration function `yang_boolean_pipeline()` that chains all
existing Yang pipeline stages (tessellate → exact mesh boolean → topology
extraction → B-Rep construction), and comprehensive tests verifying
end-to-end correctness for box-box boolean operations.

This is the capstone test for Phase 3 — proving that stages 1-3 compose
correctly before Phase 4 adds SSI refinement.

---

## 2. Parameters

### `yang_boolean_pipeline` function

| Parameter | Type | Description |
|-----------|------|-------------|
| `mesh_a` | `(Vec<[f64;3]>, Vec<[usize;3]>)` | Vertices and triangles of solid A |
| `mesh_b` | `(Vec<[f64;3]>, Vec<[usize;3]>)` | Vertices and triangles of solid B |
| `bijective_a` | `BijectiveMap` | Triangle→face mapping for solid A |
| `bijective_b` | `BijectiveMap` | Triangle→face mapping for solid B |
| `op` | `MeshBooleanOp` | Union, Subtract, or Intersect |

Returns: `ResultTopology` (arena + face_provenance + edge_is_intersection).

---

## 3. Branch Table

| Op | A-faces kept | B-faces kept | B-flipped | Expected faces (box-box overlap) |
|----|-------------|-------------|-----------|----------------------------------|
| Union | Outside-B | Outside-A | No | 10 source faces (both boxes minus shared interior) |
| Subtract | Outside-B | Inside-A | Yes | Faces from A + interior pocket faces from B |
| Intersect | Inside-B | Inside-A | No | Faces bounding the overlap region |

---

## 4. Invariants

1. **Non-empty result**: For overlapping box pairs, all three ops produce non-empty topology (V>0, E>0, F>0).
2. **Provenance completeness**: Every face in the result has a provenance entry mapping to a valid SourceFace.
3. **Edge classification completeness**: Every edge has an `is_intersection` classification.
4. **Intersection edges exist**: For overlapping solids, at least one intersection edge must be present.
5. **Face index validity**: All SourceFace.face_idx values are in range [0, 5] for box inputs.
6. **SurfaceGeom preservation**: Face provenance traces back to source faces. For boxes, all source faces are Planar. This is verifiable by checking that provenance points to valid face indices.
7. **Conservation**: The pipeline function's result face count must equal the number of entries in the FaceSurvivalMap (one result face per surviving source face group).

---

## 5. Oracles

- Face count > 0 for all three ops on overlapping boxes
- Edge count > 0 for all three ops
- Vertex count > 0 for all three ops
- `face_provenance.len() == arena.faces.len()`
- `edge_is_intersection.len() == arena.edges.len()`
- At least one edge classified as intersection
- All provenance face_idx values in [0, 5]
- Both MeshId::A and MeshId::B appear in provenance for Subtract

---

## 6. Failure Modes

- Empty meshes → empty ResultTopology (no panic)
- Non-overlapping boxes → empty or trivial result (both boxes unaffected by op)
- Identical boxes → degenerate but structurally valid result

---

## 7. Research Basis

- [#24] Yang, Jia & Yan (2025) — Full 6-stage pipeline. This test validates stages 1-3.
- [#9] Cherchi et al. (2020) — Indirect predicates (§4) and arrangement (§5);
  parent triangle provenance.
- [#38] Cherchi et al. (2022) — Full Boolean pipeline; per-patch ray-cast
  in/out classification (§5 / Algorithm 1). Yang 2025 stage 2 cites this paper.
- [#4] Shewchuk (1997) — Exact orient3d predicates used in stage 2.
- [#16] Mantyla (1988) — Half-edge B-Rep construction in stage 3.

### 7a. Analytical vs. Approximate Method Justification

The pipeline uses mesh boolean as an **exact computational intermediate** for
topology derivation (not as final representation). This is the approved hybrid
approach per A15.6 and P8 hybrid boolean corollary. Analytical surface types
are preserved through face provenance and will be refined in Phase 4.
