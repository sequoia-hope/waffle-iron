# Bijective Tessellation Mapping — Spec

**Phase**: Yang Pipeline Phase 1 (A15.6)
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6, ENGINEERING_CONSTITUTION.md P8

---

## 1. Goal

Extend the tessellation layer so each output triangle carries a mapping back to
its source B-Rep face. This enables Phase 3 (topology extraction) of the Yang
hybrid boolean pipeline: after exact mesh boolean, the bijective map tells us
which original B-Rep face each result triangle came from.

This is pure infrastructure — no behavioral change to existing tessellation
output. The `RenderMesh` continues to be produced identically. A new
`BijectiveMap` is produced alongside it.

---

## 2. Parameters

### BijectiveMap struct

| Field | Type | Description |
|-------|------|-------------|
| `tri_face_ids` | `Vec<FaceIdx>` | For each triangle (by index), the source B-Rep face |
| `tri_count` | `usize` | Total number of triangles (== tri_face_ids.len()) |

### Inputs

- `RenderMesh` (indices, vertices) — the tessellation output
- `face_map: BTreeMap<u64, FaceIdx>` — maps kernel IDs to face indices
- `face_ranges: Vec<FaceRange>` — maps triangle ranges to face IDs

### Outputs

- `BijectiveMap` populated alongside `RenderMesh`

---

## 3. Branch Table

| Path | Condition | Expected Behavior |
|------|-----------|-------------------|
| Bounded tessellation | No primitive params, no arcs, not polygon soup | `BijectiveMap` populated via `FaceRange` |
| Fan tessellation | All other cases (cylinder/sphere/cone/torus params, arcs, polygon soup) | `BijectiveMap` populated via `FaceRange` |
| Empty solid | No faces | `BijectiveMap` with empty `tri_face_ids` |
| Post-processing | flip/retessellate changes indices | `BijectiveMap` remains consistent with final indices |

---

## 4. Invariants

1. **Bijective property**: Every triangle maps to exactly one face:
   `tri_face_ids.len() == mesh.indices.len() / 3`

2. **Coverage**: Every face in `face_ranges` has at least one triangle mapped to it
   (unless the face produced zero triangles, which can happen for degenerate faces).

3. **Consistency with FaceRange**: For each `FaceRange { face_id, start_index, end_index }`,
   all triangles in `[start_index/3 .. end_index/3)` must map to a `FaceIdx` that
   corresponds to `face_id` in the `face_map`.

4. **No behavioral change**: `RenderMesh` output is identical with or without
   bijective mapping enabled.

---

## 5. Oracles

- **Triangle count oracle**: `bijective_map.tri_count == mesh.indices.len() / 3`
- **Face coverage oracle**: For a box (6 faces), all 6 faces appear in the map.
  For a cylinder (3 faces), all 3 appear. For a sphere (1+ faces), all appear.
- **Consistency oracle**: Group triangles by face via `BijectiveMap` and via
  `FaceRange` — the groupings must match.

---

## 6. Failure Modes

- Empty face map → empty `BijectiveMap` (not an error)
- Face produces zero triangles → face absent from `tri_face_ids` (acceptable)
- Post-processing removes triangles → `BijectiveMap` must be updated accordingly

---

## 7. Research Basis

- [#24] Yang, Jia & Yan (2025) — Bijective mapping is stage 1 of the hybrid
  B-Rep/mesh boolean pipeline. Each mesh triangle must map back to its source
  B-Rep face for topology extraction in stage 3.
- [#9] Cherchi et al. (2020) — The exact mesh boolean operates on triangles;
  bijective mapping enables tracing results back to original B-Rep faces.

### 7a. Analytical vs. Approximate Method Justification

This feature does not involve surface-surface intersection. It is pure
tessellation infrastructure. No SSI method justification required.

---

## 8. Implementation Notes

The `BijectiveMap` can be derived from existing `FaceRange` data — each
`FaceRange` already tells us which triangles belong to which face. The
`BijectiveMap` inverts this: for each triangle, store the face.

Initial implementation: derive `BijectiveMap` from `FaceRange` after
tessellation completes. This is the simplest correct approach and requires
no changes to the tessellation inner loops.

Future phases may extend with parametric (u,v) coordinates per vertex,
but Phase 1 only needs the face mapping.
