# PR-YR7 (P2a) — yang-rs curved Stage-1 tessellation: CYLINDER only

**Milestone:** M5 / Phase 2 step 2 (curved Stage-1 tessellation, cylinder).
**Predecessor:** PR-YR6 (curved `Surface`/`Curve` types + loud rejection).
**Crate:** `crates/yang-rs/` only.

## Goal (narrow, highest-risk single cycle)

Generalize Stage-1 tessellation (`BRep::new`) so a **closed solid cylinder**
B-Rep tessellates into a watertight, error-bounded triangle mesh with a correct
`TessellationMap`, and resolve cylinder faces by point-to-surface distance.

**Out of scope (do NOT implement here):** no boolean wiring of a cylinder, no
`ssi-rs` call, no exact intersection curves (P2c/P3). `Surface::Sphere` and
`Surface::Cone` MUST still reject loudly with
`YangError::CurvedSurfaceNotYetSupported`. The existing planar box path MUST be
behaviorally unchanged (byte-for-byte same mesh + map).

## Design decisions

### 1. Cylinder B-Rep encoding — seam edge (NOT a `BRepFace` two-loop change)

A closed solid cylinder = lateral tube + 2 planar disk caps, encoded with a
**seam edge** so the lateral is a topological disk with a single loop that
traverses the seam twice. `BRepFace` type is UNCHANGED.

Fixture helper `cylinder_brep(axis_point, axis_dir, radius, height)`:
- **Vertices (2):** `v0` = bottom-rim seam point (bottom circle at angle 0),
  `v1` = top-rim seam point.
- **Edges (3):**
  - `e0` bottom rim = `Curve::Circle { center: bottom_center, normal: -axis_dir, radius }`, `start = end = v0`.
  - `e1` top rim = `Curve::Circle { center: top_center, normal: +axis_dir, radius }`, `start = end = v1`.
  - `e2` seam = `Curve::LineSegment`, `v0 → v1`.
- **Faces (3):**
  - `f0` lateral `Surface::Cylinder { axis_point, axis_dir, radius }`, `outer_loop = [e0, e2, e1, e2]`.
  - `f1` bottom cap `Surface::Plane { normal: -axis_dir_unit, d: -(normal·bottom_center) }`, `outer_loop = [e0]`.
  - `f2` top cap `Surface::Plane { normal: +axis_dir_unit, d: -(normal·top_center) }`, `outer_loop = [e1]`.

Here `bottom_center = axis_point`, `top_center = axis_point + height·axis_dir_unit`
(the helper normalizes `axis_dir`). The plane `d` is `-(normal · center)` so the
cap plane passes through its rim center.

### 2. Validation relaxation (`BRep::new`)

Current rule rejects any `outer_loop.len() < 3`. A disk bounded by one closed
circle has loop length 1. Relax to: **`len >= 3` is required only when EVERY
loop edge is `Curve::LineSegment`** (unchanged for the box). A loop containing
≥1 non-`LineSegment` curve may have `len >= 1`. Out-of-range edge-index checks
are unchanged.

### 3. Curved Stage-1 tessellation (the core)

Dispatch in `BRep::new` by face surface type. **A face that is `Surface::Plane`
with all `LineSegment` boundary edges takes the EXACT existing Newell-fan path
verbatim** (box invariant — do not refactor it in a way that changes its output).

- **Shared rim-ring generator (watertightness mechanism, paper §4.1.2).** A
  pre-pass over B-Rep edges: for each `Curve::Circle` edge, generate `N` sample
  points ONCE and cache `edge_idx → [mesh vertex indices]`. Both the lateral and
  the adjacent cap index the SAME cached ring → shared vertices → watertight.
  - The circle's seam vertex (its `start`/`end` B-Rep vertex, at angle 0) is
    REUSED as `ring[0]` (keeps the source `BRepVertex`, avoids a duplicate at the
    seam). `ring[1..N]` are new Steiner verts with source
    `TessellationSource::BRepEdge { edge, t = angle_radians }`.
- **N from chord error:** smallest `N` (with `N >= 3`) such that
  `r·(1 − cos(π/N)) ≤ d_ε`. `d_ε = 1e-2 × AABB_diagonal`, where the AABB is
  computed ANALYTICALLY from the two rim circles' exact extents: per axis `i`,
  a circle of center `c`, unit normal `n`, radius `r` spans
  `c_i ± r·√(max(0, 1 − n_i²))`; combine the min/max over both rims.
  Both rim circles of one cylinder share the same `N` (use the cylinder's two
  rim circles to choose a single `N`; in this fixture both rims have equal `r`).
- **Lateral:** 2 axial rings suffice (cylinder is ruled along the axis — exact
  axially). Connect `bottom_ring[i]`–`top_ring[i]` into `N` quads → `2N`
  triangles. The lateral reads its two `Circle` boundary edges to obtain the rim
  centers (axial extent). If the lateral face does NOT have exactly 2 `Circle`
  boundary edges → `YangError::MalformedTopology` (loud).
- **Caps:** disk fan = a new center Steiner vertex + fan over the cached rim ring
  → `N` triangles each. Cap center source =
  `TessellationSource::BRepFace { face: cap_face_idx, u: 0.0, v: 0.0 }`.
- **Winding (governance A15.5):** orient each emitted triangle by the ANALYTIC
  surface normal at its centroid (lateral: radial-outward from the axis; caps:
  the plane normal), flipping the triangle's winding if the geometric normal
  opposes it — NOT the planar Newell path.

Mesh totals for the minimal cylinder: `V = 2N (rings) + 2 (cap centers) = 2N+2`;
`F = 4N`; `E = 6N` → **Euler `V − E + F = 2`**.

### 4. The bijection + `eval_source` (makes `TessellationMap` first-class)

Sources emitted for the minimal cylinder:
- Rim verts → `BRepEdge { edge, t = angle_radians }` (shared lateral+cap — the
  watertight verts; `ring[0]` keeps its `BRepVertex` source).
- Cap centers → `BRepFace { face: cap, u, v }`.
- Lateral interior → none (2 axial rings have no interior; document this).

Add production helper `BRep::eval_source(&self, src: TessellationSource) -> Point3`
(the bijection's inverse):
- `BRepVertex(i)` → `self.vertices[i].point`.
- `BRepEdge { edge, t }`:
  - `LineSegment` → `lerp(start_point, end_point, t)` with `t ∈ [0,1]`.
  - `Circle { center, normal, radius }` → `center + r·(cos t · e1 + sin t · e2)`,
    `t` in radians, where `(e1, e2) = ortho_basis(normal)`.
- `BRepFace { face, u, v }`:
  - `Plane { normal, d }` → `O + u·e1 + v·e2`, `O = -d·normal_unit`, `(e1,e2) = ortho_basis(normal)`.
  - `Cylinder { axis_point, axis_dir, radius }` →
    `axis_point + v·axis_dir_unit + r·(cos u · e1 + sin u · e2)`, `(e1,e2) = ortho_basis(axis_dir)`.
  - For the minimal cylinder, the cap-center source is on a `Plane` cap face with
    `u = v = 0`, evaluating to `O` (the rim center) — which is the cap center. Good.
- `eval_source` may return a `Result` only if a curved variant truly cannot be
  evaluated; Sphere/Cone surfaces in `eval_source` → return the rim center / the
  plane formula is not applicable. Since the cylinder pipeline never emits Sphere/
  Cone sources, `eval_source` handles `Plane`/`Cylinder`/`LineSegment`/`Circle`/
  `BRepVertex` and may treat the remaining cases as defensively documented. Prefer
  an infallible `-> Point3` (the round-trip oracle calls it on real emitted
  sources only); if a Sphere/Cone surface is hit, it is a bug, so a documented
  fallback (e.g. the surface's center/apex) keeps it panic-free (P9: no panic).

**Critical coupling:** ONE private `ortho_basis(n) -> (Vector3, Vector3)`
deterministic helper (normalizes `n`, picks the stablest cross axis to seed `e1`,
then `e2 = n × e1`) is used by BOTH tessellation sampling AND `eval_source`, or
the round-trip fails. Update the `TessellationSource::BRepEdge` doc comment:
`t ∈ [0,1]` for `LineSegment`, radians for `Circle` (faithful contract
refinement — no existing test emits `BRepEdge` sources).

### 5. Point-to-surface face resolution

Extract `signed_distance_to_surface(surface: Surface, point: Point3) -> Result<f64, YangError>`:
- `Plane { normal, d }` → `(normal_unit · x) + d` ... note current planar code
  treats `n·x + d` with the stored (unit) normal; keep that convention — return
  `n·x + d` using the stored normal as-is (planar fixtures use unit normals).
- `Cylinder { axis_point, axis_dir, radius }` → `dist(x, axis_line) − radius`.
- `Sphere`/`Cone` → `Err(YangError::CurvedSurfaceNotYetSupported { face })` — but
  this free function has no face index; give it signature
  `signed_distance_to_surface(surface, point) -> Result<f64, YangError>` and on a
  Sphere/Cone return `Err(YangError::CurvedSurfaceNotYetSupported { face: usize::MAX })`
  OR keep the existing `plane_dist` closure shape. Simpler: make
  `signed_distance_to_surface` handle `Plane`+`Cylinder` and return `Err` with a
  sentinel for Sphere/Cone; the `boolean()` `plane_dist` closure wraps it and
  substitutes the real face index. (Implementer picks the cleanest shape that
  keeps the loud Sphere/Cone rejection and is unit-testable.)
- Wire the `boolean()` distance closure (currently `plane_dist`) to use `.abs()`
  of this for `Plane` + `Cylinder`. Unexercised by a cylinder boolean this PR
  (no cylinder boolean), but tested directly as a unit and reused by oracle #1,
  so it is not dead code.
- **Leave `reconstruct_topology` rejecting `Cylinder` loudly** — that site does
  output-surface inheritance + a plane-specific normal flip (cavity sense), which
  is curved Stage-6 reassembly, explicitly deferred to P2c. Document this.

## RED oracle (all four hard)

New integration test file `crates/yang-rs/tests/yr7_cylinder.rs`. For tessellated
cylinders across several radii/heights/axes **including an off-axis, non-unit
`axis_dir`**:

1. **Surface-to-mesh distance ≤ d_ε:** sample points across every triangle
   (e.g. centroid + vertices); assert each sample's distance to the cylinder
   surface (lateral: `|dist(x, axis) − r|`; caps: `|plane signed dist|`, whichever
   face the triangle belongs to) is `≤ d_ε`. The mesh must not bulge beyond the
   chord-error bound.
2. **Watertight + 2-manifold:** every undirected mesh edge is shared by EXACTLY
   two triangles. If the sidecar `inputcheck` is available
   (`cherchi_sidecar_rs::inputcheck`, self-skip on `SidecarError::BinaryNotFound`),
   assert it passes; otherwise assert exact-2-manifold directly and note the
   sidecar check is env-gated.
3. **Bijection round-trip:** for every mesh vertex `v`,
   `eval_source(map.lookup(v))` reproduces `mesh.verts[v]` within tol.
4. **Euler:** `V − E + F = 2`.

Plus:
- A unit test of `signed_distance_to_surface` for the cylinder formula (point at
  known distance from the axis).
- Sphere/Cone faces still → `CurvedSurfaceNotYetSupported`.
- The planar box / cube tessellation is unchanged (same vert/tri counts; reuse an
  existing cube fixture pattern).

### Faithful test migration (standing rule)

A cylinder face is no longer rejected at `BRep::new`. A cylinder face on a
*triangle* (no `Circle` rims) is now `MalformedTopology` (the lateral lacks its 2
`Circle` rims), NOT `CurvedSurfaceNotYetSupported`. Migrate — changing ONLY the
expected outcome, preserving every structural assertion:
- `crates/yang-rs/src/lib.rs` in-lib test `brep_new_rejects_cylinder_face`.
- `crates/yang-rs/tests/yr6_adversary.rs`: `adversary_cylinder_face0_rejected_exact`
  and the cylinder arm of `adversary_curved_face2_reports_index_2`.
- `adversary_curved_never_ok` asserts only `is_err()` → UNCHANGED; sphere/cone
  arms UNCHANGED.

The Adversary independently verifies these migrations were not weakened (the
cylinder-on-a-triangle still errors loudly; it merely changed error *kind*).

## Files

- `crates/yang-rs/src/lib.rs` — `BRep::new` dispatch + rim-ring pre-pass +
  cylinder/cap tessellation; `ortho_basis`; `BRep::eval_source`;
  `signed_distance_to_surface`; loop-length relaxation; wire `boolean()` distance
  closure; `TessellationSource::BRepEdge` doc; migrate the one in-lib test; add a
  `cylinder_brep` fixture available to integration tests (a `pub fn` test helper
  or duplicated independently in the test file — RED author decides, but the
  integration test must be able to build a cylinder B-Rep).
- `crates/yang-rs/tests/yr7_cylinder.rs` — NEW, the RED oracle.
- `crates/yang-rs/tests/yr6_adversary.rs` — faithful migration of the two cylinder
  assertions.
- `docs/yang_functional_roadmap.md` — record PR-YR7/P2a done; next = P2b (sphere)
  → P2c → P3.

## STOP-and-report triggers (P9/P10)

- If watertightness needs a `BRepFace` two-loop change that ripples beyond the
  documented loop-length relaxation → STOP and report.
- If the distance oracle cannot pass honestly without tolerance widening or
  vertex snapping → STOP and report. No tolerance widening, no snapping hacks, no
  fallback paths that produce right answers for wrong reasons.

## Execution process (role-separated)

1. **Spec (Manager):** this file. Commit.
2. **RED sub-agent:** author `yr7_cylinder.rs` (4 oracles + extras) + migrate the
   obsoleted assertions. Tests must FAIL against current code. Never writes
   production. Commit.
3. **GREEN sub-agent:** implement the tessellation/eval/distance in `lib.rs` until
   RED passes. Never touches tests. Commit.
4. **Adversary sub-agent:** independent verification — fresh fixtures, confirm
   watertightness isn't faked (no snapping), confirm migrations not weakened,
   attack off-axis/non-unit cases. Commit.

## CI gate (before done)

- `cargo test -p yang-rs` (FULL crate — a Stage-1 change can regress the planar path)
- `cargo fmt -p yang-rs -- --check`
- `cargo clippy -p yang-rs --all-targets -- -D warnings`

All clean.
