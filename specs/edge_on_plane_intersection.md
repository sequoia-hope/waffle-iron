# Spec: Edge-on-Plane Intersection Detection (Phase 2c)

## Goal

Handle the degenerate case in `tri_tri_intersect` where two vertices of one
triangle lie exactly on the plane of the other triangle. This "edge-on-plane"
configuration is currently returned as `CrossingResult::None`, causing the Yang
pipeline to miss intersection segments at coplanar boundaries.

This is the blocking issue for correct manifold topology, volume accuracy, and
Euler characteristic in the Yang boolean pipeline.

## Research Basis

- **Ref #9**: Cherchi et al. 2020 — Edge-on-plane is a degenerate intersection
  configuration requiring 2D intersection of the coplanar edge segment with the
  other triangle's boundary. The conformal mesh arrangement requires all such
  intersections to be detected.
- **Ref #4**: Shewchuk 1997 — `orient2d` predicates provide exact classification
  of point vs edge in the projected plane.
- **Ref #24**: Yang 2025 — Stage 2 (exact mesh boolean) must handle all
  triangle-triangle intersection configurations for correct topology.

## Parameters

- **Input**: Two triangles T_A (indices `tri_a[3]`) and T_B (indices `tri_b[3]`),
  shared vertex array `verts`.
- **Precondition**: `find_crossing_edges` has determined `n_coplanar == 2` for one
  triangle's vertices against the other's plane.
- **Output**: A `CrossingResult` indicating intersection points (if any) between
  the coplanar edge of one triangle and the interior/boundary of the other.

## Branch Table

| Coplanar edge position | Result |
|------------------------|--------|
| Edge entirely outside triangle | `CrossingResult::None` |
| Edge endpoint inside, other outside | `CrossingResult::VertexOnPlane` |
| Both endpoints inside triangle | `CrossingResult::TwoEdges` (both as vertex-to-self IPs) |
| Edge crosses triangle boundary (enters/exits) | `CrossingResult::TwoEdges` (entry + exit IPs) |
| Edge endpoint on triangle boundary, other outside | `CrossingResult::VertexOnPlane` |
| Edge overlaps triangle edge (collinear) | `CrossingResult::TwoEdges` (overlap endpoints) |

## Invariants

1. **No missed intersections**: If the coplanar edge of T has any point inside the
   other triangle T', `find_crossing_edges` must return a non-None result.
2. **Exact predicates**: All point-vs-triangle classification uses `orient2d`
   (Shewchuk) — no tolerance parameters.
3. **Symmetry**: The intersection result must be the same regardless of which
   triangle is T_A vs T_B (commutativity of detection, not of the boolean op).
4. **No regression**: All existing passing tests must remain passing.

## Oracles

- **Manifold check**: For axis-aligned box boolean, every edge of the result mesh
  is shared by exactly 2 triangles.
- **Euler characteristic**: V - E + F = 2 for genus-0 closed manifold result.
- **Volume**: Signed volume matches analytical prediction (e.g., Union of two
  overlapping boxes = a + b - overlap).

## Failure Modes

- **Collinear edges**: Two edges from different triangles may be exactly collinear.
  Must correctly handle partial overlap.
- **Vertex-on-edge**: A coplanar vertex may lie exactly on the other triangle's edge.
  Must return correct point contact.
- **Degenerate projection**: If the triangle's normal is nearly axis-aligned, the 2D
  projection must choose the correct dominant plane to avoid degenerate orient2d.

## Implementation Location

- `crates/kernel/src/boolean/exact_mesh.rs`, function `find_crossing_edges`
- Modify the `n_coplanar == 2` branch (currently line 204-209)

## Investigation Findings (2026-04-01)

### Attempted approach: TwoEdges with degenerate IndirectPoints

Returning `CrossingResult::TwoEdges` with vertex-to-self `IndirectPoint`s for the
two coplanar vertices. This correctly detects edge-on-plane intersections and
subdivides axis-aligned boxes (28 sub-tris instead of 12 per mesh).

### Blockers discovered

1. **Winding number ambiguity at mesh surfaces**: Sub-triangles created by
   edge-on-plane splitting have centroids ON the opposing mesh's surface, giving
   w = 0.5 (exactly at the inside/outside threshold). The `classify_winding`
   function (w >= 0.5 → Inside) produces inconsistent results depending on
   floating-point rounding. Attempted fix: offset centroid along parent triangle
   normal before evaluating winding number. This fixed the classification for
   most cases but not for co-surface sub-triangles where the offset direction
   is parallel to the opposing surface.

2. **Co-surface face deduplication**: For axis-aligned boxes, faces from both
   meshes lie on the same plane (e.g., A's z=0 face and B's z=0 face). After
   edge-on-plane detection, BOTH faces' sub-triangles are classified as
   Outside the other mesh, so both survive in Union — producing duplicate
   faces on the same plane. The correct fix requires co-planar face
   deduplication in the selection stage, which is described in [#24] Yang 2025
   but not yet implemented.

3. **Conformal vertex sharing at boundaries**: For axis-aligned boxes, edge-on-
   plane intersection points lie on shared mesh edges/vertices (not strictly
   inside triangles). These boundary intersection points are not properly merged
   with existing mesh vertices, creating non-conformal meshes where adjacent
   sub-triangles don't share vertex indices. This causes fragmented trim loops
   in Phase 3 (5+ loops per face instead of 1).

### Recommendation

Edge-on-plane detection requires three coordinated changes:

1. Modify `find_crossing_edges` to return TwoEdges for n_coplanar==2
2. Add co-planar face deduplication to `face_survival_detect` or a new post-
   processing step
3. Use parent-normal offset + strict winding threshold in `label_cells`

These changes interact with each other and cannot be implemented independently
without causing regressions. They should be done as a single atomic change.

### Test status

Red-phase tests added (all ignored pending implementation):
- `edge_on_plane_crossing_detected` — basic detection
- `edge_on_plane_partial_crossing` — one endpoint inside
- `edge_on_plane_no_crossing` — both outside (passes on current code)
- `edge_on_plane_axis_aligned_boxes` — box subdivision
- `edge_on_plane_box_boolean_manifold` — full pipeline manifold check
