# Spec: Flood-Fill Patch Segmentation for Yang Boolean Pipeline

## Goal

Replace `build_result_brep_from_mesh()` (750+ lines of boundary-edge-chaining)
with flood-fill patch segmentation per Yang 2025 [#24] Section 4.4.2. This
eliminates the root cause of 94 self-intersection failures in the Yang assay.

## Research Basis

Yang 2025 [#24] Section 4.4.2:

> "Our algorithm segments the mesh Boolean results into patches along the boundary
> curves, which correspond to either the original boundary curves or the intersection
> curves. Starting from an inner triangle, i.e. not on the boundaries of each mesh
> patch, using it as a seed triangle for the patch, our algorithm expands the patch
> by including more neighboring inner triangles, until all the neighboring triangles
> of the patch are on the boundaries."

## Root Cause of Current Failures

`build_result_brep_from_mesh` uses boundary-edge-chaining + greedy twin-pairing.
This is fragile at perpendicular junctions, multi-entry directed edges, and T-junctions.
When twin-pairing fails (even 1 unpaired HE out of hundreds), partial repair or discard
produces non-manifold B-Rep topology. Retessellation on non-manifold input produces
self-intersecting triangles.

## Algorithm

### Input
- `FaceSurvivalMap`: surviving sub-triangles grouped by source B-Rep face
- `SubdividedMesh`: vertex positions + sub-triangle definitions

### Steps

1. **Flatten**: Collect all surviving sub-triangles with source-face tracking into a
   flat array. Apply winding flip for Subtract operation's B-inside-A triangles.

2. **Canonical vertices**: Quantize vertex positions (nanometer precision, 1e9 scale)
   to canonicalize shared vertices across per-face meshes.

3. **Edge adjacency**: For each directed edge (cv0, cv1) in the canonical vertex space,
   record which triangle owns it. Two triangles are adjacent if one has (cv0, cv1) and
   the other has (cv1, cv0).

4. **Boundary classification**: An edge is a B-Rep boundary if:
   - Its reverse is owned by a triangle from a DIFFERENT source face group, OR
   - It has no reverse (mesh boundary — surviving triangle borders non-surviving space)
   An edge is also an intersection edge if the two source face groups come from
   different meshes (MeshId::A vs MeshId::B).

5. **Flood-fill**: BFS from each unvisited triangle, expanding to neighbors across
   non-boundary edges. Each connected component = one patch = one B-Rep face.

6. **Boundary loop extraction**: For each patch, walk its boundary edges in winding order.
   Use Newell normal to determine outer vs inner loops (holes).

7. **Build B-Rep**: For each patch → create Face + Loop + HalfEdges in TopoArena.
   Twin-pair boundary HEs: each boundary edge has exactly one HE from each adjacent patch.
   This is 1:1 by construction (conformal mesh + flood-fill guarantees).

### Output
- `ResultTopology`: same struct as before (arena, face_provenance, edge_is_intersection)

## Branch Table

| Scenario | Behavior |
|---|---|
| Normal overlapping faces | Flood-fill produces correct patches |
| Perpendicular junctions | Boundary edges naturally separate patches — no T-junction splitting |
| Coplanar merged faces | Flood-fill respects merged source groups — single patch |
| Single surviving face | One patch with boundary = face outline |
| Empty survival | Return empty ResultTopology |

## Invariants

1. Every surviving sub-triangle belongs to exactly one patch
2. Every patch maps to exactly one B-Rep face (via source face of seed triangle)
3. Every boundary edge has exactly 2 adjacent patches → 1:1 twin pairing
4. Result B-Rep is manifold: twin symmetry holds for ALL half-edges
5. Zero unpaired half-edges (no partial repair needed)
6. `face_provenance.len()` = number of patches = number of B-Rep faces

## Oracles

- `validate_yang_result_topology` passes (Euler characteristic correct per component)
- Self-intersection oracle: 0 inter-face triangle penetrations after retessellation
- Watertight mesh oracle: 0 unpaired edges in final output

## Failure Modes

- If conformal subdivision is incomplete (T-junctions remain), flood-fill may produce
  patches with non-conformal boundaries. This is a subdivision bug, not a flood-fill bug.
  Fix in `subdivide_mesh_pair`, not here.
- If boundary loop extraction fails on multi-component boundaries (outer + holes),
  fall back to simpler boundary tracing. Log a diagnostic.

## What This Replaces

Deletes entirely:
- `build_result_brep_from_mesh()` (topology_extract.rs:334-1100)
- `rebuild_topology_without_faces()` (partial repair helper)
- All T-junction splitting, open-chain reconciliation, synthesized face logic
