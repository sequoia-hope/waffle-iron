# F2 — the patch chart is the face's parameter domain

**Roadmap:** `docs/yang_faithfulness_roadmap.md` F2, item 2 ("the patch
chart is the face's parameter domain (F5a's interface), for every surface:
whole-patch sphere and torus charts exist, and a cone apex is a point of
the domain, not an `ApexInPatch` refusal"). **Matrix:** 4.4-09.
**Ledger:** N2. **Substrate:** `specs/yang_f5a_surface_interface.md`
(`ParamSurface`, `AnalyticInverse`). Paper cites are lines in
`refs/text/yang2025_hybrid_boolean.txt`.

## 1. The paper's operation

§4.4.1 triangulates each trimmed patch "totally ... in the parametric
domain" (`:560-561`): a CDT whose constraints are the intersection
polylines and the patch boundary, in the (u, v) rectangle of the patch's
surface. The paper's surfaces are NURBS faces from CAD models. Their domain
is a rectangle: a closed surface's seam and a cone's apex or a sphere's pole
are EDGES of that rectangle (a seam edge appears twice in the face's loop,
a degenerate edge maps to one point). The paper is silent on how the CDT
treats them because, in its input, they are already part of the face loop.

Our B-Rep has no seam or degenerate edges: a cylinder band is bounded by
two circles, a cone cap by one. The patch chart therefore has to supply
what the NURBS face loop carries: cut the patch open along a seam, and
represent an apex or pole as a degenerate edge.

## 2. What the code does (2026-11-18)

`stage4_construct::rebuild_patch_inner` (the update primitive behind
`update_patches_ex_with_bounds`) charts a patch with `SurfaceChart`
(plane, cylinder, cone; `stage4_project.rs`):

- a patch whose cycles all have azimuth winding 0 is unwrapped onto one
  θ branch (`stage4_splice::unwrap_theta`);
- a patch with two opposite encircling cycles is a BAND, cut open along a
  clear seam between two cycle vertices (`band_patch_from_cycles`);
- anything else refuses: sphere and torus patches (`NonPlanarPatch`; the
  update gates them out as `chart-unsupported:{Sphere,Torus}`), a cone cap
  (one encircling cycle — `ThetaUnwrap`), a cone patch touching its apex
  (`ApexInPatch`).

## 3. The design

A new module `stage4_patch_chart.rs` builds the chart polygon of a patch in
its surface's `ParamSurface` domain, for every surface. It reads the
patch's own triangles for topology — the old triangulation IS the patch
region, so no question about which side of a cycle the patch lies on is
answered by a guess.

1. **Development.** Breadth-first over the patch's old triangles (through
   edges shared by exactly two of them), each triangle's corners get
   (u, v) from `AnalyticInverse::inverse`, each periodic coordinate on the
   copy nearest the already-placed neighbour. A corner exactly on a
   degenerate iso-line (the inverse returns v on it: a cone apex, a sphere
   pole) has no azimuth; it takes its triangle's other corners' mean u and
   is marked degenerate.
2. **Period lattice.** Where a second path reaches a placed vertex, the two
   coordinates differ by a whole number of periods (degenerate vertices are
   not compared). The differences span the image of the patch's loops in
   the period lattice: {0} (the patch does not wrap), a multiple of the u
   period (it wraps the azimuth), a multiple of the v period (a torus tube
   band), or both (a torus patch of genus ≥ 1 — refused, named).
3. **Degenerate points in the region.** A degenerate iso-line's point lies
   in the patch when an old vertex sits exactly on it or an old triangle
   winds once around it (its corner azimuths, consecutively unwrapped, sum
   to ±2π; on a sphere the corners' mean latitude names the pole).
4. **Cycle windings.** Each cycle is unwrapped along itself from its first
   vertex's developed coordinate; (w_u, w_v) are its windings.
5. **Classes** (everything else refuses with a named class):

   | lattice | degenerate in region | cycles | class | chart polygon |
   |---|---|---|---|---|
   | {0} | none | all (0, 0) | DISC | developed coordinates; outer = largest \|area\| |
   | u | none | two of w_u = ±1, opposite; rest (0, 0) | U-BAND | cut along the shortest clear seam between the two encircling cycles (the cylinder band's construction, on the domain) |
   | v | none | two of w_v = ±1, opposite; rest (0, 0) | V-BAND | the same with the axes swapped (torus tube band) |
   | u | one | one of w_u = ±1; rest (0, 0) | CAP | the encircling cycle once round from a vertex a, the seam from a straight to the degenerate iso-line, the degenerate edge back, the seam up |
   | u | two (sphere) | all (0, 0) | SPHERE-ZONE | the whole domain rectangle cut along a clear meridian, both poles degenerate edges, every cycle a hole |

6. **Degenerate edges.** A degenerate edge is subdivided into pieces of at
   most π/2 in u (the Bézier split of F5a); every copy maps to ONE mesh
   vertex — the old vertex on that point if there is one, else a new vertex
   at `eval`. After the CDT, a triangle with two corners on the degenerate
   edge has two identical 3D corners and is dropped; its two other edges
   each pair with a neighbour's (both lift to the same 3D edge in
   opposite directions), so the fan around the apex or pole closes. Any
   other repeated vertex refuses.
7. **Seams.** Both copies of a seam map to the same mesh vertices. A seam
   between two cycle vertices is one chart segment (as today). A seam from
   a cycle vertex to a degenerate edge (CAP) or from pole to pole
   (SPHERE-ZONE) is an iso-line: straight in 3D on a cone, a meridian arc on
   a sphere, subdivided in v until each piece's chord sag is at most the
   patch's budget (new vertices at `eval`, shared by both copies).
8. **Interiors** keep today's rule (an old interior vertex is carried unless
   it lies on a cycle edge or is excluded); each takes its developed
   coordinate, shifted by whole periods into the polygon.
9. **d(T)** is computed per triangle on the polygon's coordinates; a
   degenerate corner takes the u of another corner of its triangle, so the
   covering rectangle is the wedge the triangle actually spans. The budget
   is today's `min(old max, d_ε)`, the old max read on the developed old
   triangles.
10. **Orientation** is read from the boundary (a directed cycle edge of the
    old triangles must be traversed the same way), as for bands today.

The CDT's Delaunay criterion runs in the chart scaled to arc length at the
patch's reference radius (the isometric idea of N106, one scale per axis).

**Design choices the paper does not make** (one ledger entry, N122):
the seam (where the cut goes), the degenerate-edge split into ≤ π/2 pieces
and the dropping of its double-degenerate triangles, and the scaled metric.
They exist because our faces carry no seam or degenerate edges, which the
paper's NURBS faces do.

## 4. Increments

0. **Census** (`YANG_441_UPDATE_CENSUS`, a `PATCH` line per intersected
   patch: surface, outcome). Names the population: chart-unsupported sphere
   and torus patches, cone caps (`ThetaUnwrap` on a cone), `ApexInPatch`.
1. **The module, unwired**, with unit pins on synthetic patches: a cone cap
   (apex vertex and no apex vertex), a sphere cap, a sphere band, a sphere
   zone, a torus disc, a torus u-band, a torus v-band, and the refusals
   (genus-1 torus patch, apex on a cycle).
2. **Wired for the refused classes only**: `rebuild_patch_inner` routes
   sphere and torus patches, and cone patches that reach the apex, through
   the domain chart; the update's gate admits them. Plane, cylinder and
   cone-without-apex patches keep today's path byte for byte. Gate: corpus
   held, every moved case anchored.
3. **One chart for every surface**: the plane, cylinder and cone paths move
   onto the module (`SurfaceChart` retires from the update). Not
   hash-identical (the inverse's branch is `[0, 2π)`, `atan2`'s is
   `(−π, π]`); gate: corpus held.
4. The fan-local `ApexInPatch` sites (`rebuild_merge_fan`,
   `refill_fan_hole_*`, `fan_rebuild_core`, `rebuild_group_fan`) belong to
   repairs F2 retires; they are not converted, they go with their passes.

## 5. Increments 0–2 measured (2026-11-18)

**Census (increment 0)**, a `PATCH` line per intersected patch in
`update_patches_ex_with_bounds`, corpus at db97281e (360C / 0W / 9E / 5EE):
17,519 lines. The update took 11,839 plane, 2,357 cone and 295 cylinder
patches. It refused 802 cone and 572 cylinder patches by the d(T) budget
(N106), 344 by crossing cycles, 88 torus and 2 sphere patches as
chart-unsupported (22 + 2 cases), and 7 by `ThetaUnwrap` (6 cylinder, 1
cone). `ApexInPatch` never fired at the update: a cone cap refuses earlier,
as `ThetaUnwrap`.

**Wired (increments 1–2)**: the module and its pins
(`stage4_patch_chart::tests`), and `rebuild_patch_update` in the update.
Corpus: **360C / 0W / 9E / 5EE / 0T, no category move**; canonical geometry
hash identical on 373 of 374 cases, P0050 changed (CORRECT both ways, every
oracle detail identical). 83 domain charts: 45 DISC, 36 V-BAND, 2 CAP (the
two sphere cases). Outcomes on the previously refused patches: 7 rebuilt
(P0050 ×4, R0032 ×2, R0038), 59 refused by the d(T) budget (N106, the same
refusal the cylinder and cone patches meet), 4 crossing cycles, 11
`InteriorBranch` (P0034, P0049 — the cylinder `ThetaUnwrap` retries — and
R0026, R0050, R0085, R0096), 1 `Lattice` (C0065, a genus-1 torus patch).

**Open.** `InteriorBranch`: an old interior vertex lies outside the chart
polygon on every copy — a relocated vertex past the curve. The `SurfaceChart`
CDT drops such a vertex silently (its triangles classify outside); the paper
removes a vertex too close to the curve (`:564-565`). Next: anchor one case,
then drop it as that removal, named in the census. Increment 3 (one chart
for every surface) follows.
