# Spec: Conformal Mesh Subdivision for Yang Pipeline

## Goal

Fix the Yang hybrid boolean pipeline's mesh subdivision (Phase 2) to produce
conformal triangulations. Without conformal subdivision, adjacent triangles
sharing a split edge have non-matching vertex indices, causing the B-Rep
reconstruction (Phase 3) to produce non-manifold topology.

## Status

**Conformal edge-split propagation**: DONE — `subdivide_mesh_pair` now
propagates split points to all adjacent triangles sharing a split edge.

**Remaining blocker**: Edge-on-plane intersection detection (Phase 2c/2d).
When two vertices of one triangle lie on the other's plane (common in
axis-aligned box geometry), `find_crossing_edges` returns `None` (n_coplanar==2
case). This means triangle pairs with shared coplanar edges are not subdivided,
leaving the B-Rep with unpaired half-edges. The ignored `test_brep_euler_*` and
`test_brep_manifold_*` tests cannot pass until this is fixed.

## Research Basis

- **Ref #9**: Cherchi et al. 2020 — Conformal mesh arrangements require split
  propagation to all incident triangles. Edge-on-plane is a degenerate case
  requiring 2D intersection.
- **Ref #24**: Yang 2025 — Hybrid B-Rep/mesh boolean pipeline, Stage 2.
- **Ref #4**: Shewchuk 1997 — Exact predicates for point classification.

## Changes Made

1. **`split_at_edge_point`**: New helper that splits a triangle at a point on
   one edge, producing 2 sub-triangles. No vertex nudging.

2. **`split_triangle_by_segment` refactored**: Eliminated vertex nudging for
   at-vertex hits. When constraint line passes through a triangle vertex and
   an edge interior point, uses `split_at_edge_point` (2 sub-triangles) instead
   of `split_two_edge_points` with nudge (3 sub-triangles including 1 degenerate).

3. **Edge-split propagation** in `subdivide_mesh_pair`:
   - After direct constraint splitting, detects which new vertices lie on
     original mesh edges (`detect_edge_splits`)
   - Builds edge adjacency maps (`build_edge_adjacency`)
   - Propagates split points to all adjacent triangles (`propagate_edge_splits`)
   - Guards against degenerate triangles from near-endpoint splits

4. **Vertex deduplication**: Position-based dedup (1e-15 quantization) merges
   exact duplicate vertices from independent intersection computations.
