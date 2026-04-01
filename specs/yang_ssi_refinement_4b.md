# Yang Pipeline Phase 4, Task 4b — SSI Curve Refinement

**Reference**: [#24] Yang, Jia & Yan (2025) — Stage 4 of the hybrid boolean pipeline.
**Governance**: ARCHITECTURAL_INVARIANTS.md A15.6, A15.1–A15.2.
**Prerequisite**: Phase 4a complete (intersection edges classified by surface pair type).

---

## Goal

Replace mesh-approximated geometry on intersection edges with exact analytical SSI
curves. For each edge classified as `NeedsRefinement` by Phase 4a, dispatch to the
correct SSI solver based on the surface pair type, and return the analytical curve.

This is the geometry refinement step of the Yang hybrid pipeline — mesh boolean
provides correct topology (Phase 2–3), and SSI refinement restores analytical
precision to the edge geometry.

---

## Parameters (Inputs)

| Parameter | Type | Description |
|-----------|------|-------------|
| `result` | `&ResultTopology` | Half-edge B-Rep from Phase 3 with vertex positions |
| `classification` | `&IntersectionEdgeClassification` | Phase 4a output: edge → surface pair kind |
| `surface_map` | `&BTreeMap<(MeshId, FaceIdx), SurfaceGeom>` | Original B-Rep face surface geometry |

---

## Output

`EdgeRefinementMap` — a mapping from `EdgeIdx` to the analytical `SSICurve` for
every successfully refined intersection edge.

```rust
pub(crate) struct EdgeRefinementMap {
    /// Analytical SSI curve for each refined intersection edge.
    pub edges: BTreeMap<EdgeIdx, SSICurve>,
    /// Edges where refinement was skipped (PlanarPlanar — already exact).
    pub skipped_planar: usize,
    /// Edges where the SSI solver returned NotSupported.
    pub unsupported: Vec<(EdgeIdx, String)>,
}
```

---

## Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| B1: Empty classification | No entries in classification | Return empty map |
| B2: PlanarPlanar edge | `SurfacePairKind::PlanarPlanar` | Skip, increment `skipped_planar` |
| B3: Plane-Cylinder | One `Planar`, one `Cylindrical` | Call `plane_cylinder_ssi` → Circle or Ellipse |
| B4: Plane-Sphere | One `Planar`, one `Spherical` | Call `plane_sphere_ssi` → Circle |
| B5: Plane-Cone | One `Planar`, one `Conical` | Call `plane_cone_ssi` → Circle (perpendicular) or conic |
| B6: Plane-Torus | One `Planar`, one `Toroidal` | Call `plane_torus_ssi` |
| B7: Cylinder-Cylinder | Both `Cylindrical` | Call `cylinder_cylinder_ssi` → Lines (parallel) or Degree4 |
| B8: Cylinder-Sphere | One `Cylindrical`, one `Spherical` | Call `cylinder_sphere_ssi` → Degree4 |
| B9: Cylinder-Cone | One `Cylindrical`, one `Conical` | Call `cylinder_cone_ssi` → Degree4 |
| B10: Cylinder-Torus | One `Cylindrical`, one `Toroidal` | Call `cylinder_torus_ssi` → NotSupported |
| B11: Cone-Cone | Both `Conical` | Call `cone_cone_ssi` → Degree4 |
| B12: Cone-Sphere | One `Conical`, one `Spherical` | Call `cone_sphere_ssi` → Degree4 |
| B13: Cone-Torus | One `Conical`, one `Toroidal` | Call `cone_torus_ssi` → NotSupported |
| B14: Sphere-Sphere | Both `Spherical` | Call `sphere_sphere_ssi` → Circle |
| B15: Sphere-Torus | One `Spherical`, one `Toroidal` | Call `sphere_torus_ssi` |
| B16: Torus-Torus | Both `Toroidal` | Call `torus_torus_ssi` → NotSupported |
| B17: SSI solver returns empty | Solver returns `Ok(vec![])` | Edge not refined (degenerate/tangent) |
| B18: SSI solver returns error | Solver returns `Err(NotSupported)` | Record in `unsupported`, skip edge |
| B19: Multiple curves returned | Solver returns >1 curve | Select curve closest to mesh edge midpoint |

---

## Invariants

1. **Coverage**: Every `NeedsRefinement` edge is either refined, recorded as unsupported,
   or noted as empty (tangent/degenerate). `refined + unsupported + empty == NeedsRefinement count`.
2. **PlanarPlanar untouched**: PlanarPlanar edges never appear in the refined output.
3. **Symmetry**: `dispatch_ssi(plane, cyl)` produces the same result as `dispatch_ssi(cyl, plane)`.
4. **Analytical preservation**: The SSICurve returned by a solver is the *exact* intersection
   of the two analytical surfaces — not an approximation.

---

## Oracles (Test Assertions)

1. **Empty classification → empty refinement**: No edges refined.
2. **Box-box subtract → all skipped**: Every intersection edge is PlanarPlanar, `skipped_planar > 0`,
   `edges` is empty.
3. **Plane-cylinder edge → Circle or Ellipse**: Hand-crafted topology with one intersection edge
   between a planar face and a cylindrical face. Solver returns a Circle (perpendicular case).
   Assert: refined curve is `SSICurve::Circle` with correct center, normal, and radius.
4. **Plane-sphere edge → Circle**: Assert: refined curve is `SSICurve::Circle` with radius
   matching `sqrt(R² - d²)` where `d` is the plane-center distance.
5. **NotSupported pair → recorded**: Cylinder-torus edge records NotSupported, doesn't panic.
6. **Count conservation**: `refined + unsupported.len() + skipped == classification.edges.len()`.

---

## Algorithm

```
refine_intersection_edges(result, classification, surface_map):
    let mut refined = BTreeMap::new()
    let mut skipped_planar = 0
    let mut unsupported = vec![]

    for (edge_idx, kind) in classification.edges:
        match kind:
            PlanarPlanar => skipped_planar += 1; continue
            NeedsRefinement { surface_a, surface_b } =>
                // Get mesh edge midpoint for curve selection
                let midpoint = edge_midpoint(result, edge_idx)

                // Dispatch to correct SSI solver
                match dispatch_ssi(surface_a, surface_b):
                    Ok(curves) if curves.is_empty() => continue  // tangent/degenerate
                    Ok(curves) =>
                        let best = select_closest_curve(&curves, midpoint)
                        refined.insert(edge_idx, best)
                    Err(KernelError::NotSupported { operation }) =>
                        unsupported.push((edge_idx, operation))
                    Err(other) => return Err(other)

    return Ok(EdgeRefinementMap { edges: refined, skipped_planar, unsupported })
```

---

## Failure Modes

| Error | Condition | Handling |
|-------|-----------|----------|
| NotSupported solver | SSI pair not implemented (e.g., torus-torus) | Record in `unsupported`, continue |
| Internal SSI error | Solver returns non-NotSupported error | Propagate as `Err` |
| Missing vertex positions | Edge endpoints not in arena | Return `Err(InternalError)` |

---

## Research Basis

- [#24] Yang, Jia & Yan (2025) — Stage 4: geometry refinement. Intersection edges
  on curved surfaces are replaced with exact SSI curves.
- [#1] Patrikalakis et al. — SSI dispatch by surface type pair (Ch. 5). All quadric
  pairs have closed-form solutions.
- [#25] Yang, Jia & Yan (2023) — Topology-guaranteed SSI.

---

## File Location

`crates/kernel/src/boolean/ssi_refinement.rs` — extends the existing Phase 4a module.
