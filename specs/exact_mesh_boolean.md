# Exact Mesh Boolean — FIP Spec

**Feature**: Exact mesh boolean using indirect predicates (Yang pipeline Phase 2)

**Governance**: A15.6 (Hybrid B-Rep/Mesh Boolean Pipeline)

**Status**: Phase 2 of `specs/yang_hybrid_migration.md`

---

## 1. Goal

Implement triangle-triangle intersection and cell labeling using exact geometric
predicates, producing a topologically correct result mesh from two input meshes.
This replaces the Sutherland-Hodgman clipping + tolerance escalation pipeline
with a provably correct, tolerance-free approach.

The exact mesh boolean takes two tessellated solids (with bijective face maps)
and a `BoolOp`, and produces a result mesh where:
- Every triangle maps to exactly one source B-Rep face (bijective property preserved)
- No unpaired edges (watertight)
- Correct Euler characteristic for the operation
- Zero tolerance parameters in the pipeline

---

## 2. Parameters

| Parameter | Type | Default | Units | Range | Error |
|-----------|------|---------|-------|-------|-------|
| `mesh_a` | `&RenderMesh` | — | — | non-empty | `InvalidInput` if empty |
| `mesh_b` | `&RenderMesh` | — | — | non-empty | `InvalidInput` if empty |
| `map_a` | `&BijectiveMap` | — | — | `is_complete()` | `InvalidInput` if incomplete |
| `map_b` | `&BijectiveMap` | — | — | `is_complete()` | `InvalidInput` if incomplete |
| `op` | `BoolOp` | — | — | Union/Subtract/Intersect | — |

---

## 3. Branch Table

| Branch | Condition | Behavior |
|--------|-----------|----------|
| B1: Disjoint | Bounding boxes don't overlap | Short-circuit: Union = both meshes, Intersect = empty, Subtract = mesh_a |
| B2: Triangle-triangle no intersection | orient3d classifies all vertices of tri_b on same side of tri_a's plane (or vice versa) | Skip pair — no intersection segment |
| B3: Triangle-triangle coplanar | All 6 orient3d tests return 0 | Coplanar triangle pair handling — 2D intersection via orient2d |
| B4: Triangle-triangle crossing | Vertices of tri_b straddle tri_a's plane | Compute intersection segment as two indirect points |
| B5: Edge-edge intersection | Intersection segment degenerates to a point | Single indirect point (vertex of result) |
| B6: Vertex-on-face | A vertex of one triangle lies exactly on another triangle | Degenerate — handled via orient3d == 0 classification |
| B7: Cell inside A only | Winding number = (1, 0) | Include for Union and Subtract; exclude for Intersect |
| B8: Cell inside B only | Winding number = (0, 1) | Include for Union; exclude for Subtract and Intersect |
| B9: Cell inside both | Winding number = (1, 1) | Include for Union and Intersect; exclude for Subtract |
| B10: Cell outside both | Winding number = (0, 0) | Exclude for all operations |

---

## 4. Invariants

**I1 — Bijectivity preserved**: Every triangle in the result mesh maps to exactly
one source B-Rep face from either mesh_a or mesh_b. The `BijectiveMap` for the
result has `is_complete() == true`.

**I2 — Watertight**: The result mesh has zero unpaired (boundary) edges. Every
edge is shared by exactly two triangles.

**I3 — Euler characteristic**: For a closed genus-0 solid, `V - E + F = 2`.
For union of two disjoint solids, `V - E + F = 4`.

**I4 — Volume conservation**: For axis-aligned box pairs:
- Union volume = V_a + V_b - V_intersection
- Subtract volume = V_a - V_intersection
- Intersect volume = V_intersection

**I5 — No tolerance parameters**: The pipeline uses only exact predicates
(orient3d, orient2d from `geometry-predicates` and `robust` crates). No epsilon
comparisons, no tolerance factors, no convergence loops.

**I6 — Deterministic**: Same inputs always produce same output. No dependence on
iteration order of hash maps or system state.

**I7 — Surface type preservation (A15.5)**: Sub-triangles inherit `surface_geom`
from their parent triangle's bijective mapping. No surface type is lost or
downgraded through the boolean operation.

---

## 5. Oracles

**O1 — Box-box union volume**: Two unit boxes at (0,0,0) and (0.5,0,0):
intersection volume = 0.5, union volume = 1.5.
Verify `result_volume == 1.5 ± TAU_MODEL`.

**O2 — Box-box subtract face count**: Unit box minus inscribed smaller box:
result has 12 faces (6 outer + 6 inner). Verify face count.

**O3 — Box-box intersect = overlap region**: Two overlapping unit boxes:
intersection is a box. Verify bounding box of result matches expected overlap.

**O4 — Euler characteristic**: For every test case, verify `V - E + F = 2`
(assuming genus-0 result).

**O5 — Unpaired edge count**: For every test case, verify zero unpaired edges.

**O6 — Disjoint short-circuit**: Two non-overlapping boxes: union has
`V - E + F = 4`, intersect produces empty mesh, subtract = mesh_a unchanged.

**O7 — Coplanar face handling**: Two boxes sharing a face: union produces a
single box (shared face removed). Verify `V - E + F = 2` and correct volume.

---

## 6. Failure Modes

| Condition | Expected Error |
|-----------|---------------|
| Empty mesh input | `KernelError::InvalidInput("empty mesh")` |
| Incomplete BijectiveMap | `KernelError::InvalidInput("incomplete bijective map")` |
| Non-manifold input mesh | `KernelError::InvalidInput("non-manifold input")` |
| Self-intersecting input | `KernelError::InvalidInput("self-intersecting input")` — detected via orient3d sign inconsistency |
| Degenerate triangle (zero area) | Filtered during input validation |

---

## 7. Research Basis

### Primary references

- **[#24] Yang, Jia & Yan (SIGGRAPH 2025)**: The overall pipeline architecture.
  This spec implements stages 2 (exact mesh boolean) of their six-stage pipeline.
  Key insight: bijective mapping eliminates ambiguity in face survival detection.

- **[#9] Cherchi et al. (SIGGRAPH Asia 2020)**: Indirect predicates for exact
  mesh arrangements. Intersection points stored as symbolic references to input
  triangles (not materialized coordinates). Three-stage filtering. We adapt their
  approach for our `FacePoly`-based mesh representation.

- **[#10] Levy (ACM TOG 2025)**: Radial sort for non-manifold edge resolution.
  When multiple triangles meet at an intersection edge, angular sort using exact
  predicates determines cell adjacency. We use Levy's approach for task 2e.

- **[#4] Shewchuk (1997)**: Adaptive precision predicates (orient3d, orient2d).
  The `geometry-predicates` crate is a Rust port of Shewchuk's predicates.
  The `robust` crate provides an alternative implementation already in use.

### Analytical vs. Approximate Justification (A15 §7a)

This module implements **exact** mesh boolean using indirect predicates. The mesh
is an exact computational intermediate, not a final representation. Analytical
surface geometry is preserved through the pipeline via bijective mapping (I7).

For quadric surface pairs, the analytical SSI solvers (A15.4) provide exact
intersection curves in Phase 4. The mesh boolean provides exact *topology*
(which faces survive, how they connect). This is consistent with A15.1–A15.2.

---

## 8. Sub-task Decomposition

| Task | Description | Depends on |
|------|-------------|------------|
| 2a | Add `geometry-predicates` dependency, verify WASM | Phase 1 |
| 2b | Triangle-triangle intersection via orient3d → indirect points | 2a |
| 2c | Constrained triangulation of intersected faces | 2b |
| 2d | Cell labeling via generalized winding numbers | 2c |
| 2e | Radial sort for non-manifold edges ([#10] Levy) | 2d |
| 2f | Integration tests: box-box boolean (all 3 ops) | 2e |

Each sub-task is one agent session (~30-60 min). Tasks are sequential.
