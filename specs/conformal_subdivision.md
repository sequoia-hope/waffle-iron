# Spec: Conformal Mesh Subdivision for Yang Pipeline

## Goal

Fix the Yang hybrid boolean pipeline's mesh subdivision (Phase 2) to produce
conformal triangulations. Without conformal subdivision, adjacent triangles
sharing a split edge have non-matching vertex indices, causing the B-Rep
reconstruction (Phase 3) to produce non-manifold topology.

## Status

**COMPLETE** — All conformal subdivision passes implemented and verified.

Both `e2e_box_boolean_manifold` and `e2e_box_boolean_euler` tests pass for
all three boolean operations (Union, Subtract, Intersect) on hub-spoke
(4-tri/face) meshes. V-E+F=2 (Euler) and every edge shared by exactly 2
triangles (manifold) are verified.

## Research Basis

- **Ref #9**: Cherchi et al. 2020 §5 (arrangement) — Conformal mesh
  arrangements require split propagation to all incident triangles.
  Edge-on-plane is a degenerate case requiring 2D intersection. (See also
  [#38] Cherchi 2022 §4 for the speed-improved variant of this arrangement
  used by Yang 2025.)
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

5. **Within-mesh conformal repair** (`enforce_conformal_edges`, Step 3e):
   After all direct splits and propagation, verifies that adjacent triangles
   sharing original edges have matching edge fragmentations. Collects all
   split vertices on each shared edge across all adjacent triangles and
   propagates missing ones. Runs iteratively until convergence.

6. **Cross-mesh sub-triangle conformal** (`cross_mesh_subtri_conformal`, Step 3f):
   Detects vertices from one mesh that lie on sub-triangle edges of the other
   mesh (along the intersection curve). Hub-spoke edges in mesh A create split
   vertices that mesh B doesn't have, and vice versa. Propagates these to
   ensure both meshes share identical intersection-curve vertex sets.

7. **Full sub-triangle conformal repair** (`subtri_conformal_repair`, Step 3g):
   Builds edge adjacency from current sub-triangle edges (not just original
   edges) and propagates missing vertices across ALL shared edges, including
   intersection-curve edges between sub-triangles from different parents.
   Required for Subtract/Intersect where B-inside sub-triangles must form a
   closed manifold patch.

8. **Multi-sub-tri split fix**: Removed premature `found` break from all
   conformal functions. When two sub-triangles share an edge containing a
   split vertex, ALL sharing sub-triangles are now split (not just the first
   one found). This was the root cause of Subtract/Intersect non-manifold
   edges even after cross-mesh propagation.
