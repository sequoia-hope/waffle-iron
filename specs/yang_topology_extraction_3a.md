# Yang Pipeline Phase 3, Task 3a — Face Survival Detection

**Parent spec**: `specs/yang_hybrid_migration.md` (Phase 3)
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6
**File**: `crates/kernel/src/boolean/topology_extract.rs`

---

## 1. Goal

After the exact mesh boolean (Phase 2) selects which sub-triangles survive,
determine which original B-Rep faces those sub-triangles came from. Group the
surviving sub-triangles by their source face, producing a `FaceSurvivalMap`
that Phase 3b–3d will consume to extract trim boundaries and build the result
B-Rep.

**User-visible behavior**: None (internal pipeline infrastructure). This is a
data-structure-only task that connects Phase 2 output to Phase 3 input.

---

## 2. Parameters

### Inputs

| Parameter | Type | Description |
|-----------|------|-------------|
| `subdivided` | `&SubdividedMesh` | Phase 2 subdivided mesh with sub-triangles |
| `labeling` | `&CellLabeling` | Phase 2 inside/outside labels per sub-triangle |
| `op` | `MeshBooleanOp` | Boolean operation (Union, Subtract, Intersect) |
| `bijective_a` | `&BijectiveMap` | Phase 1 bijective map for mesh A |
| `bijective_b` | `&BijectiveMap` | Phase 1 bijective map for mesh B |

### Output

| Parameter | Type | Description |
|-----------|------|-------------|
| `FaceSurvivalMap` | struct | Groups of surviving sub-triangles keyed by (MeshId, FaceIdx) |

### Constants

No new tolerance constants. This is exact bookkeeping over integer indices.

---

## 3. Branch Table

| Op | Sub-tri from | Label | Selected? | Winding flipped? |
|----|-------------|-------|-----------|-----------------|
| Union | A | Outside | Yes | No |
| Union | A | Inside | No | — |
| Union | B | Outside | Yes | No |
| Union | B | Inside | No | — |
| Subtract | A | Outside | Yes | No |
| Subtract | A | Inside | No | — |
| Subtract | B | Inside | Yes | Yes |
| Subtract | B | Outside | No | — |
| Intersect | A | Inside | Yes | No |
| Intersect | A | Outside | No | — |
| Intersect | B | Inside | Yes | No |
| Intersect | B | Outside | No | — |

Additional branches:

| Condition | Behavior |
|-----------|----------|
| Face has zero surviving sub-triangles | Face is not in survival map (eliminated) |
| Face has all sub-triangles surviving | Face is fully preserved (no trimming) |
| Face has some sub-triangles surviving | Face is partially trimmed |
| parent_tri out of range for bijective map | Error: `InvalidBijectiveMap` |

---

## 4. Invariants

1. **Conservation**: Every selected sub-triangle in the boolean result appears
   in exactly one face group. No triangle is lost or duplicated.
   `sum(group.len() for group in map.values()) == total_selected_count`

2. **Bijective consistency**: `parent_tri` of each sub-triangle indexes into
   the original mesh's triangle array. `bijective_map.tri_face_ids[parent_tri]`
   must be a valid FaceIdx (not sentinel `usize::MAX`).

3. **Operation consistency**: The selection logic must agree with
   `select_boolean_result()` — same sub-triangles selected, same flip rules.

4. **Surface preservation (A15.5)**: The FaceSurvivalMap carries the source
   MeshId so that downstream phases can look up the original face's
   `SurfaceGeom` for analytical surface preservation.

---

## 5. Oracles

| Test | Oracle |
|------|--------|
| Box-box subtract: face count | Exactly 10 source faces contribute (6 from A partially/fully, 4-6 from B partially) |
| Conservation | `selected_count == sum of all group sizes` |
| No empty groups | Every group has `len() >= 1` |
| Parent tri validity | All `parent_tri` values < original mesh tri count |
| Bijective validity | All mapped `FaceIdx` values != `usize::MAX` |
| Box-box union face count | At most 12 source faces (6+6), some partially trimmed |
| Box-box intersect | All surviving faces partially trimmed |

---

## 6. Failure Modes

| Failure | Handling |
|---------|----------|
| `bijective_map.tri_face_ids.len()` < max `parent_tri` | Return `Err(KernelError::InternalError)` with diagnostic |
| Empty subdivided mesh (no triangles) | Return empty `FaceSurvivalMap` |
| All sub-triangles eliminated by boolean op | Return empty `FaceSurvivalMap` |

---

## 7. Research Basis

- **[#24] Yang, Jia & Yan (2025)**: Stage 3 of the hybrid pipeline — topology
  extraction from exact mesh boolean result. The bijective mapping enables
  unambiguous assignment of result triangles to source B-Rep faces.
- **[#9] Cherchi et al. (2020) §5 (arrangement)**: The subdivided mesh preserves
  parent triangle provenance via `SubTriangle.parent_tri`, enabling this face
  grouping. The Cherchi 2022 [#38] full pipeline used for Yang stage 2 inherits
  this provenance unchanged.

### 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (integer index mapping).

This task performs no geometric computation — it maps sub-triangle indices through
the bijective map to face indices. All operations are on integer indices, so no
approximation is involved. Surface geometry is preserved by carrying `MeshId` +
`FaceIdx` forward for downstream lookup.

---

## 8. Data Structure Design

```rust
/// Key identifying a source B-Rep face in the boolean result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SourceFace {
    pub mesh_id: MeshId,
    pub face_idx: FaceIdx,
}

/// A surviving sub-triangle in the boolean result, with provenance.
#[derive(Debug, Clone)]
pub(crate) struct SurvivingSubTri {
    /// Vertex indices in SubdividedMesh.verts.
    pub verts: [usize; 3],
    /// Whether winding was flipped (Subtract B-inside-A).
    pub flipped: bool,
}

/// Maps each surviving source face to its contributing sub-triangles.
/// Produced by face_survival_detect(), consumed by Phase 3b trim boundary extraction.
#[derive(Debug)]
pub(crate) struct FaceSurvivalMap {
    /// Keyed by (MeshId, FaceIdx), value is the sub-triangles from that face.
    pub groups: BTreeMap<SourceFace, Vec<SurvivingSubTri>>,
}
```
