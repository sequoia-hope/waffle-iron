# Yang Pipeline Phase 4, Task 4a — Intersection Edge Surface Classification

**Reference**: [#24] Yang, Jia & Yan (2025) — Stage 4 of the hybrid boolean pipeline.
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6, A15.1–A15.2.
**Prerequisite**: Phase 3 complete (topology extraction produces `ResultTopology`).

---

## Goal

Classify each intersection edge in the Yang boolean pipeline result by the
analytical surface types of its two adjacent faces. This classification enables
Phase 4b to dispatch to the correct SSI solver for geometry refinement.

---

## Parameters (Inputs)

| Parameter | Type | Description |
|-----------|------|-------------|
| `result` | `&ResultTopology` | Half-edge B-Rep from Phase 3 with face provenance and edge intersection flags |
| `surface_map` | `&BTreeMap<(MeshId, FaceIdx), SurfaceGeom>` | Maps each original B-Rep face to its analytical surface geometry |

---

## Output

`IntersectionEdgeClassification` — a mapping from `EdgeIdx` to `SurfacePairKind`
for every edge where `result.edge_is_intersection[edge] == true`.

```rust
pub(crate) enum SurfacePairKind {
    /// Both faces are planar — intersection is a line. No refinement needed.
    PlanarPlanar,
    /// At least one face is curved — SSI solver required for refinement.
    NeedsRefinement {
        surface_a: SurfaceGeom,
        surface_b: SurfaceGeom,
    },
}

pub(crate) struct IntersectionEdgeClassification {
    pub edges: BTreeMap<EdgeIdx, SurfacePairKind>,
}
```

---

## Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| B1: No intersection edges | `edge_is_intersection` has no `true` entries | Return empty classification |
| B2: Planar–Planar | Both adjacent faces have `SurfaceGeom::Planar` | Classify as `PlanarPlanar` |
| B3: Planar–Curved | One face is `Planar`, other is `Cylindrical`/`Conical`/`Spherical`/`Toroidal` | Classify as `NeedsRefinement` |
| B4: Curved–Curved | Both faces are curved (any combination) | Classify as `NeedsRefinement` |
| B5: Missing provenance | Edge is marked as intersection but adjacent face has no provenance | Panic (invariant violation — every face must have provenance) |
| B6: Missing surface | Face provenance found but no entry in `surface_map` | Return `KernelError` — caller supplied incomplete data |

---

## Invariants

1. **Coverage**: Every edge with `edge_is_intersection[e] == true` appears in the output.
2. **Completeness**: Non-intersection edges are NOT in the output.
3. **Consistency**: If both faces are planar, the result is always `PlanarPlanar`.
4. **Symmetry**: The classification does not depend on the order of the two surfaces.

---

## Oracles (Test Assertions)

1. **Box–box subtract**: All intersection edges are `PlanarPlanar` (6 faces × 6 faces, all planar).
2. **Box–cylinder subtract**: Intersection edges where cylinder meets box top/bottom face should be `NeedsRefinement` with one `Planar` and one `Cylindrical` surface.
3. **Count conservation**: `classification.edges.len()` == count of `true` values in `edge_is_intersection`.
4. **Empty input**: Empty `ResultTopology` → empty classification.

---

## Failure Modes

| Error | Condition | Handling |
|-------|-----------|----------|
| Missing surface geometry | `surface_map` doesn't contain an entry for a face referenced by provenance | Return `Err(KernelError::InternalError)` with descriptive message |
| Unpaired edge | Intersection edge has only one adjacent face (boundary edge) | Skip — only paired edges (with twin) are classifiable |

---

## Algorithm

```
classify_intersection_edges(result, surface_map):
    let mut classification = BTreeMap::new()
    for (edge_idx, &is_intersection) in result.edge_is_intersection:
        if !is_intersection: continue

        // Get one half-edge of this edge
        let he = arena.edges[edge_idx].half_edge
        let twin = arena.half_edges[he].twin

        // Get face of each half-edge via loop → face
        let face_a = arena.loops[arena.half_edges[he].loop_].face
        let face_b = arena.loops[arena.half_edges[twin].loop_].face

        // Look up provenance
        let source_a = result.face_provenance[face_a]
        let source_b = result.face_provenance[face_b]

        // Look up surface geometry
        let surf_a = surface_map[(source_a.mesh_id, source_a.face_idx)]
        let surf_b = surface_map[(source_b.mesh_id, source_b.face_idx)]

        // Classify
        match (surf_a.is_planar(), surf_b.is_planar()):
            (true, true) => PlanarPlanar
            _ => NeedsRefinement { surf_a, surf_b }

    return IntersectionEdgeClassification { edges: classification }
```

---

## Research Basis

- [#24] Yang, Jia & Yan (2025) — Stage 4: geometry refinement. Intersection edges
  on curved surfaces are replaced with exact SSI curves. The classification step
  determines which edges need this treatment.
- [#1] Patrikalakis et al. — SSI dispatch by surface type pair (Ch. 5).
- [#25] Yang, Jia & Yan (2023) — Topology-guaranteed SSI.

---

## File Location

`crates/kernel/src/boolean/ssi_refinement.rs` — new module for Phase 4 work.
Registered in `crates/kernel/src/boolean/mod.rs`.
