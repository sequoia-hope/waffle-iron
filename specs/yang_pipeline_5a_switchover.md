# Yang Pipeline Phase 5, Task 5a — WaffleKernel Integration

**Reference**: [#24] Yang, Jia & Yan (2025) — Stage 5 of the hybrid boolean pipeline.
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6.
**Prerequisite**: Phases 1–4d complete (full pipeline tested end-to-end).

---

## Goal

Add a `yang_boolean` code path to `WaffleKernel::do_boolean` that routes boolean
operations through the Yang hybrid pipeline (Phases 1–4) instead of the legacy
S-H clipping / polygon approximation path. Initially added as a conditional branch
that can be tested against the legacy path.

---

## Parameters

None — this is a routing change inside `do_boolean`.

---

## Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| B1: Yang path enabled | Both operands tessellable | Tessellate → mesh boolean → topology extract → classify → refine → build WaffleSolid |
| B2: Yang path fails | Pipeline returns error | Fall back to legacy path with warning |
| B3: Legacy path | Yang path disabled or unavailable | Existing behavior unchanged |

---

## Algorithm

```
yang_boolean_integrated(kernel, solid_a, solid_b, op, id_alloc):
    // Step 1: Tessellate both solids
    mesh_a = kernel.tessellate(solid_a, tolerance)
    mesh_b = kernel.tessellate(solid_b, tolerance)

    // Step 2: Convert to pipeline format
    (verts_a, tris_a) = render_mesh_to_arrays(mesh_a)
    (verts_b, tris_b) = render_mesh_to_arrays(mesh_b)

    // Step 3: Build bijective maps
    bijective_a = BijectiveMap::from_render_mesh(mesh_a, solid_a.face_map)
    bijective_b = BijectiveMap::from_render_mesh(mesh_b, solid_b.face_map)

    // Step 4: Run Yang pipeline (Phases 1-3)
    result_topo = yang_boolean_pipeline(verts_a, tris_a, verts_b, tris_b,
                                         bijective_a, bijective_b, op)

    // Step 5: Build surface map from both solids' face_geometry
    surface_map = build_surface_map(solid_a, solid_b)

    // Step 6: Classify + refine (Phases 4a-4b)
    classification = classify_intersection_edges(result_topo, surface_map)
    refinement = refine_intersection_edges(result_topo, classification, surface_map)

    // Step 7: Convert ResultTopology → WaffleSolid
    solid_result = result_topology_to_waffle_solid(result_topo, refinement,
                                                    surface_map, id_alloc)
    return solid_result
```

---

## Research Basis

- [#24] Yang et al. (2025) — Full pipeline integration
- [#9] Cherchi et al. (2020) — Bijective tessellation mapping
- [#16] Mantyla (1988) — B-Rep construction from topology

---

## File Location

`crates/kernel/src/boolean/yang_integration.rs` — new module for Phase 5 integration.
Wired into `waffle_kernel.rs` via `do_boolean`.
