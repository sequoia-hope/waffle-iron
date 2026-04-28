# Spec: Edge-on-Plane Intersection Detection (Phase 2c)

## Goal

Handle the degenerate case in `tri_tri_intersect` where two vertices of one
triangle lie exactly on the plane of the other triangle. This "edge-on-plane"
configuration is currently returned as `CrossingResult::None`, causing the Yang
pipeline to miss intersection segments at coplanar boundaries.

This is the blocking issue for correct manifold topology, volume accuracy, and
Euler characteristic in the Yang boolean pipeline.

## Research Basis

- **Ref #9**: Cherchi et al. 2020 §5 (arrangement) — Edge-on-plane is a
  degenerate intersection configuration requiring 2D intersection of the
  coplanar edge segment with the other triangle's boundary. The conformal
  mesh arrangement requires all such intersections to be detected.
- **Ref #38**: Cherchi et al. 2022 — Full mesh-Boolean pipeline (arrangement
  speedups + ray-cast in/out, §5). Yang 2025 stage 2 builds on this paper.
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

## Implementation Progress (2026-04-02)

### Completed: Basic edge-on-plane detection

**Change**: In `tri_tri_intersect`, intercept the n_coplanar==2 case BEFORE calling
`find_crossing_edges`. When two vertices of one triangle lie on the other's plane,
call `clip_edge_on_plane()` directly to detect intersection.

**Key design choice**: `clip_edge_on_plane` uses **strict interior testing**
(`point_strictly_inside_triangle_3d`) to classify endpoints. Points ON the triangle
boundary (on edges or at vertices) are treated as "outside" to avoid creating
constraint segments for degenerate edge contacts. Without strict testing, axis-aligned
box edges that touch triangle boundaries would create non-conformal subdivisions that
break trim loop extraction.

**Result**: Edge-on-plane detection works correctly for edges that pass through the
strict interior of the opposing triangle. 3 tests un-ignored and passing:
- `edge_on_plane_crossing_detected` ✅ (both endpoints strictly inside)
- `edge_on_plane_partial_crossing` ✅ (one endpoint strictly inside)
- `edge_on_plane_no_crossing` ✅ (both endpoints outside)

### Investigated and reverted: winding number offset

**Attempted**: Offset A's centroids along +parent_normal, B's along -parent_normal,
to break the co-planar face symmetry. Results:
- Fixed co-planar face deduplication for overlapping boxes
- **But broke touching-box classification** — the offset pushes centroids across
  the touching plane, incorrectly classifying boundary faces as Inside.
- Reverted: the offset approach is too aggressive for cases where the centroid is
  near (but not on) the opposing mesh surface.

### Investigated and reverted: vertex position merging

**Attempted**: Quantize vertex positions in `extract_trim_boundaries` and merge
indices that map to the same position. Results:
- Found 0 merges for the axis-aligned box case — the non-conformal vertices have
  genuinely different positions (different intersection points on different edges),
  not duplicate positions with different indices.
- The real issue is not duplicate positions but missing conformal vertex sharing:
  the same geometric point needs to be split into ALL adjacent triangles sharing the
  edge, which requires cross-mesh edge-split propagation or the full Cherchi
  2020 §5 conformal mesh arrangement algorithm (with [#38] Cherchi 2022 §4
  speed-ups).

### Investigated: cross-mesh edge-split propagation

**Attempted**: After same-mesh edge-split detection, scan other mesh's sub-tris for
vertices on this mesh's edges. Results:
- Correctly identified cross-mesh splits (e.g., vertex at [2,0,1] on A's edge)
- But propagation created MORE fragmented sub-tris instead of fewer, because the
  propagated splits were not themselves propagated to further adjacent tris
- Iterating the propagation (up to 4 rounds) didn't converge
- Root cause: the simplified triangle splitting doesn't produce conformal meshes.
  The proper solution is the Cherchi 2020 §5 conformal mesh arrangement algorithm
  (with [#38] Cherchi 2022 §4 speed-ups), which ensures all intersection
  points create shared vertices across all
  adjacent triangles. This is a substantial implementation effort.

### Remaining blockers for axis-aligned box boolean

Two of the 3 original blockers have been partially addressed:

1. **Winding number ambiguity** — ✅ PARTIAL FIX: `label_sub_tri` offsets the
   centroid along -normal (into the sub-triangle's own solid) when winding
   number is ambiguous (w ∈ [0.3, 0.7]). This correctly disambiguates A sub-tris
   on B's surface. Touching-box cases work correctly with -normal (unlike the
   previous +normal attempt).

2. **Co-planar face deduplication** — ❌ STILL NEEDED: The winding offset correctly
   labels individual sub-tris, but coplanar B face groups still survive for
   Subtract/Intersect because their sub-tris are genuinely inside A (offset
   confirms this). A separate dedup pass was implemented but caused regressions
   in conservation tests (it removes valid B sub-tris for Union). The dedup
   requires operation-aware logic that's coupled with the conservation invariant.

3. **Conformal vertex sharing** — ❌ STILL NEEDED: Intersection vertices created
   by the passthrough case in `clip_edge_on_plane` are shared via constraint
   segments in both meshes. However, vertices at triangle boundary crossings
   (e.g., (1,1,0) on a diagonal edge) split one mesh's triangles but not all
   adjacent triangles of the other mesh that share the same geometric edge.
   This creates non-conformal meshes where trim loops have 5+ edges per face
   instead of the expected 4, and twin pairing fails for boundary edges.

The conformal mesh arrangement (Cherchi 2020 §5 [#9] / Cherchi 2022 §4 [#38])
remains the prerequisite for the
topology_extract Euler/manifold tests. Current topology: V=20, E=16, F=10,
HE=48 for overlapping box subtract (target: V-E+F=2, HE=2*E).

### Progress (2026-04-02)

**Completed**:
- Passthrough case in `clip_edge_on_plane`: detects edges crossing through
  triangles when neither endpoint is strictly inside (axis-aligned geometry)
- Winding offset in `label_sub_tri`: -normal offset for ambiguous centroids
- Both improvements have zero regressions (952 tests pass)
- Subdivision now produces 28 sub-tris per mesh (up from 12 unsplit)
- New intersection vertices at (1,1,0), (1,1,2), (1,0,1), (1,2,1) etc.

**Not completed**:
- Coplanar face dedup (regressions in conservation tests)
- Conformal vertex sharing (requires Cherchi 2020 §5 / Cherchi 2022 §4 mesh arrangement)
- Un-ignoring the 3 topology_extract tests (V-E+F=2, manifold, all-ops)

### Test status

Tests passing (un-ignored):
- `edge_on_plane_crossing_detected` ✅ — basic detection
- `edge_on_plane_partial_crossing` ✅ — one endpoint strictly inside
- `edge_on_plane_no_crossing` ✅ — both outside (passes)

Tests still ignored (3 in topology_extract, need conformal subdivision):
- `test_brep_euler_characteristic` — V-E+F=14 (need 2)
- `test_brep_all_ops` — same Euler issue for all ops
- `test_brep_manifold_edges` — HE=48 vs 2*E=32 (need HE=2*E)
