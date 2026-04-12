# Yang SSI Vertex Refinement — Stage 4.3

## Context

Per Yang et al. 2025, Section 4.3: after the mesh boolean computes exact
topology (Stage 2) and extracts intersection edges (Stage 3), vertex positions
along intersection curves are only mesh-approximate. Stage 4.3 projects these
vertices onto the analytically exact SSI curves to restore surface-exact
geometry.

This is the "intersection optimization" step — it bridges the gap between
mesh-precision topology and analytical-precision geometry.

## Inputs

- `ResultTopology` — half-edge B-Rep from Stage 3 with:
  - `arena: TopoArena` — vertices, half-edges, edges, faces
  - `edge_is_intersection: BTreeMap<EdgeIdx, bool>` — flags for intersection edges
- `EdgeRefinementMap` — from Phase 4b, mapping each intersection edge to its
  analytical `SSICurve`

## Algorithm

### Step 1: `SSICurve::closest_point(pt) → [f64; 3]`

Each SSICurve variant needs a `closest_point` method that projects a 3D point
onto the curve, returning the nearest point on the curve:

- **Line**: orthogonal projection onto the line segment, clamped to [start, end]
- **Circle**: project onto the plane containing the circle, normalize to radius
- **Ellipse**: project onto ellipse plane, solve nearest-point on ellipse
- **Parabola/Hyperbola**: parametric closest-point via Newton iteration
- **Degree4 variants**: parametric closest-point via Newton iteration

Initial implementation covers Line and Circle (the two most common cases in
box/cylinder booleans). Other variants return the input point unchanged as a
safe no-op fallback until implemented.

### Step 2: `refine_vertex_positions(arena, refinement_map) → ()`

For each edge in `refinement_map.edges`:
1. Look up the edge's two endpoint vertices via half-edge traversal
2. Project each vertex position onto the SSI curve via `closest_point`
3. Update `arena.vertices[v].position` in place

Vertices shared by multiple intersection edges may be projected multiple times.
The last projection wins, which is acceptable because shared vertices lie at
curve-curve intersections where all adjacent curves should agree.

### Step 3: Topology preservation invariant

Refinement MUST NOT change topology — same number of vertices, edges, faces,
loops. Only vertex positions change. Half-edge connectivity (twin, next, prev)
is unchanged. This is verified by comparing counts and twin pairing before and
after.

## Output

Modified `TopoArena` with vertex positions snapped to analytical SSI curves.
Topology is identical; only `vertices[*].position` fields change.

## References

- Yang, Jia & Yan (2025) — Section 4.3: intersection optimization
- Patrikalakis et al. — Ch. 5: SSI curve geometry
- Existing code: `crates/kernel/src/boolean/ssi_refinement.rs` (Phase 4a/4b)
- Existing code: `crates/kernel/src/ssi/mod.rs` (SSICurve enum + solvers)
