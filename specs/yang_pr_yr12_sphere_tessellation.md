# PR-YR12 (P2b) — yang-rs curved Stage-1 tessellation: SPHERE only

**Milestone:** M5 / Phase 2 step (curved Stage-1 tessellation, sphere — the
remaining curved Stage-1 primitive after the cylinder).
**Predecessor:** PR-YR7 (P2a, curved Stage-1 cylinder tessellation).
**Crate:** `crates/yang-rs/` only.

## Goal (narrow, highest-risk single cycle)

Generalize Stage-1 tessellation (`BRep::new`) so a **closed solid sphere** B-Rep
tessellates into a watertight, error-bounded triangle mesh with a correct
`TessellationMap`, and resolve sphere faces by point-to-surface distance.

**Out of scope (do NOT implement here):** no boolean wiring of a sphere, no
`ssi-rs` call, no exact intersection curves, no NURBS. `Surface::Cone` MUST
still reject loudly with `YangError::CurvedSurfaceNotYetSupported`. The existing
planar box path and the cylinder path MUST be behaviorally unchanged
(byte-for-byte same mesh + map; the cylinder ring path stays untouched).

## Design decisions

### 1. Sphere B-Rep encoding — standard CAD sphere (one face + meridian seam)

A closed solid sphere = ONE `Surface::Sphere { center, radius }` face, bounded by
a single **meridian seam edge** whose `Curve::Circle` is the great circle through
both poles (`center`, `radius = r`, a chosen `normal`), with `start = south pole
vertex`, `end = north pole vertex`. The two **pole vertices** are real
`BRepVertex` entries placed exactly at `center ± r·ẑ`. The face has
`outer_loop = [seam_edge]` (a 1-edge non-LineSegment loop — legal per the PR-YR7
loop-length relaxation, `lib.rs:393-415`). `BRepFace` type UNCHANGED.

Fixture helper `sphere_brep(center, radius)` (test-side):
- **Vertices (2):** `v0` = south pole `center + r·(0,0,-1)`, `v1` = north pole
  `center + r·(0,0,+1)`.
- **Edges (1):** `e0` meridian seam = `Curve::Circle { center, normal: (0,-1,0),
  radius: r }`, `start = v0` (south), `end = v1` (north). The seam lies in the
  X–Z plane (its `normal` is `±Y`); the convention here uses `(0,-1,0)`.
- **Faces (1):** `f0` `Surface::Sphere { center, radius }`,
  `outer_loop = [e0]`, `inner_loops = []`.

### 2. Fixed z-up parameterization

`Surface::Sphere` carries no axis (a sphere is isotropic), so `eval_source`'s
face arm needs a fixed convention. Use world z-up:

```
face_eval(u, v) = center + r·(cos v·cos u, cos v·sin u, sin v)
```

poles on `±ẑ`, azimuth `u` measured from `+X` toward `+Y`, latitude `v ∈
[−π/2, +π/2]`, seam at `u = 0` (the `+X` meridian; plane = X–Z, seam circle
`normal = (0,-1,0)`). All corpus spheres are z-up (centers/radii vary). A future
*oriented* sphere would derive the frame from the pole vertices — **out of scope**,
documented as a known limitation (mirrors how the cylinder needs `axis_dir`).

### 3. Chord bound `d_ε = 1e-2 × 2r√3`

Paper §4.1: `d_ε = 1e-2 × (AABB diagonal of the model)`. A sphere of radius `r`
has AABB = the cube `[c−r, c+r]³`; its **space diagonal is `2r√3`**. (NB: `2r` is
the *diameter*, not the diagonal; reusing `curved_chord_bound` over the lone
planar seam-meridian circle under-counts to `r√2`.) We use the geometrically
correct AABB diagonal `2r√3`, computed from `radius` alone, **identically in the
RED oracle and the GREEN production sizing** — the only hard requirement is that
the two agree. The shared `1e-2` constant stays single-sourced (governance
A14.3): it is the same literal already used by `curved_chord_bound`.

### 4. Sphere is self-contained; the rim-ring pre-pass must SKIP the sphere seam

The existing curved pre-pass (`lib.rs:435-527`) collects *all* `Curve::Circle`
edges and builds full rings — that would build a spurious orphan ring for the
sphere seam, inflating V and breaking watertight / Euler. **Fix:** exclude any
Circle edge that appears in a `Surface::Sphere` face's loops from `circle_edges`.
With a pure sphere B-Rep this empties `circle_edges`, so the cylinder ring path
is untouched (byte-for-byte) and the sphere builds its own grid in a dedicated
function.

## Changes — `crates/yang-rs/src/lib.rs` (GREEN)

- **Pre-pass guard:** before building `circle_edges` (`~lib.rs:435`), compute
  `sphere_seam_edges: BTreeSet<u32>` = the union of `outer_loop` / `inner_loops`
  edge indices over faces whose `surface` is `Surface::Sphere`; filter those out
  of `circle_edges`.
- **Dispatch (`lib.rs:603-605`):** split the `Sphere | Cone` reject arm into
  `Surface::Sphere { center, radius } => tessellate_sphere_face(...)?;` and keep
  `Surface::Cone { .. } => Err(CurvedSurfaceNotYetSupported { face: f_idx })`.
- **New `fn tessellate_sphere_face(f_idx, f, &edges, &verts, center, radius,
  &mut out_verts, &mut sources, &mut out_tris)`** (mirrors
  `tessellate_lateral_face` / `tessellate_cap_face`):
  - Find the single `Curve::Circle` seam edge in `f.outer_loop`; read its
    `normal` and `start` / `end` (= south / north pole `BRepVertex` indices).
    `MalformedTopology` if not exactly one Circle edge.
  - `d_eps = 1e-2 * 2.0 * radius * 3f64.sqrt()`.
  - `n_lon` = smallest `N ≥ 3` with `r·(1−cos(π/N)) ≤ d_eps` (equator; same form
    as the cylinder `n_seg` loop, `lib.rs:466-473`). `n_lat` = smallest `N ≥ 2`
    with `r·(1−cos(π/(2N))) ≤ d_eps` (meridian half-circle). *Honest refinement
    only — if the oracle is marginal, raise N; never widen d_ε.*
  - **Vertices:** south pole = `verts[seam.start]` (tag `BRepVertex(seam.start)`);
    north pole = `verts[seam.end]` (tag `BRepVertex(seam.end)`). Interior rings
    `j = 1..n_lat` (exclusive of poles), longitudes `i = 0..n_lon`: position via
    `face_eval(u_i, v_j)`, `u_i = 2π·i/n_lon`, `v_j = −π/2 + π·j/n_lat`. **Column
    `i = 0` is the seam** → tag `BRepEdge { edge: seam, t }` where
    `t = atan2(w·e2, w·e1)`, `(e1,e2) = ortho_basis(seam.normal)`, `w = pos −
    center` (per-sample recovery, mirroring the cylinder `phi0`,
    `lib.rs:491-504`). **Columns `i ≥ 1`** → tag `BRepFace { face: f_idx, u: u_i,
    v: v_j }`. Store ring vertex indices for triangle assembly.
  - **Triangles (watertight via modular wrap `(i+1)%n_lon`, reusing the seam
    column):** south fan `tri(south, ring[1][i], ring[1][(i+1)%n_lon])`; north
    fan `tri(north, ring[n_lat-1][(i+1)%n_lon], ring[n_lat-1][i])`; middle bands
    `j = 1..n_lat-1` split each quad into 2 tris. Orient every triangle by the
    **outward radial normal** `normalize(centroid − center)` via the existing
    `orient_tri` (`lib.rs:1156-1173`); add a small
    `sphere_outward_normal(verts, tri, center)` helper (analog of
    `radial_outward_normal`, but full radial — no axis projection).
- **`eval_source` sphere FACE arm (`lib.rs:786`):** replace the
  `Surface::Sphere { center, .. } => center` fallback with
  `center + r·(cos v·cos u, cos v·sin u, sin v)`. (Cone keeps its `apex`
  fallback. The `BRepEdge` Circle arm at `lib.rs:718-733` already inverts the
  seam — no change. The `BRepVertex` arm already returns pole positions — no
  change.)
- **`signed_distance_to_surface` (`lib.rs:1256`):** add
  `Surface::Sphere { center, radius } => Ok(|x − center| − radius)`; keep
  `Surface::Cone` returning `CurvedSurfaceNotYetSupported`.

## RED tests — new `crates/yang-rs/tests/yr12_sphere.rs` (test author)

Mirror `tests/yr7_cylinder.rs` structure and helpers (reuse the pure-Rust vector
math `sub/add/scale/dot/cross/norm/unit/p`). A `corpus()` of ≥3 z-up
`SphereCase` (name, center, radius) — e.g. unit at origin, offset large-radius,
offset small-radius. `sphere_brep(center, radius)` fixture per §1. Four hard
oracles + guards:

1. **`oracle1_mesh_within_chord_error_of_surface`** — for each tri, sample 3
   verts + centroid; `(|sample − center| − radius).abs() ≤ d_eps`, `d_eps =
   1e-2·2r√3` (test-side helper, independent of the production fn).
2. **`oracle2_watertight_two_manifold`** — `BTreeMap` undirected edge count == 2
   for every edge (poles included). Plus
   **`oracle2_inputcheck_clean_env_gated`** — `cherchi_sidecar_rs::inputcheck`,
   self-skip on `SidecarError::BinaryNotFound` (identical to
   `yr7_cylinder.rs:512-530`).
3. **`oracle3_eval_source_round_trip`** — `tessellation_map().len() ==
   num_verts`; for every vertex `eval_source(lookup(v))` within `1e-9` of
   `mesh.verts[v]` (covers pole, seam, interior).
4. **`oracle4_euler_characteristic`** — `V − E + F == 2` (`BTreeSet` undirected
   edges).

Guards (must stay green, structural assertions preserved):
- **Migrate** `sphere_face_still_rejected` (`yr7_cylinder.rs:701`): a
  *one-triangle* `Surface::Sphere` face now lacks its seam Circle → expect
  `MalformedTopology` (mirrors how cylinder-on-a-triangle became
  `MalformedTopology`, not `CurvedSurfaceNotYetSupported`). Change **only the
  expected outcome**; keep the structure.
- `signed_distance_to_surface_sphere_*`: sphere now returns `Ok(dist)`; **cone
  still `Err`**. Migrate the sphere half of the existing
  `signed_distance_to_surface_sphere_and_cone_reject` test accordingly; cone half
  unchanged.
- `cone_face_still_rejected`, `planar_cube_tessellation_unchanged`, all cylinder
  oracles — **byte-for-byte unchanged** and still green.

## Verification / CI gate (whole crate)

```
cargo test   -p yang-rs
cargo fmt    -p yang-rs -- --check
cargo clippy -p yang-rs --all-targets -- -D warnings
```

Sequence: RED fails for the right reason (missing sphere path / new symbols) →
GREEN makes the whole suite green → Adversary re-runs the gate + audits for
P9/P10 (no d_ε widening, no faked watertightness, migration not weakened) and
emits a verbatim `git diff`.

## STOP conditions (P9/P10)

If watertight / pole closure needs a `BRepFace` two-loop change that ripples
beyond this crate, or the chord / round-trip oracle can't pass honestly, **STOP
and report** — no faked watertightness, no d_ε widening, no special-case-to-green.
