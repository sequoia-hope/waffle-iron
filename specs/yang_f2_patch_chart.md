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

## 2. What the code does (2026-11-20)

The §4.4.1 update (`update_patches_ex_with_bounds`) rebuilds every
intersected patch through `stage4_construct::rebuild_patch_update`, which is
the domain chart of §3 for every surface (`rebuild_patch_domain`), and
re-plans a patch for N106's coincident-simplex separation on the same chart
(`rebuild_patch_domain_separating`).

The whole-patch fallback of the §4.3.4 chain decimation
(`stage4_chain_decimate.rs`) calls the same rebuild with the shortened
cycles, on every surface.

`SurfaceChart` (`stage4_project.rs`; plane, cylinder, cone, with
`unwrap_theta` and `band_patch_from_cycles`) still charts the construct
pass's seam replacement (`stage5_topology/construct.rs`, `stage4_construct.rs`
`rebuild_patch_planar_bounded`) and the fan-local rebuilds (§4 item 4, and
the chain decimation's primary path `rebuild_decimated_patch_fans`).

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
8. **Interiors.** An old interior vertex is carried unless it lies on a
   cycle edge or is excluded; each takes its developed coordinate, shifted
   by whole periods into the polygon. One inside on no copy has been
   relocated past the curve and is removed (§4.4.1's near-curve removal,
   `:563-564`). On an affine surface (`ParamSurface::is_affine`) no interior
   is carried: d(T) ≡ 0 for any triangulation of the polygon.
9. **d(T)** is computed per triangle on the polygon's coordinates; a
   degenerate corner takes the u of another corner of its triangle, so the
   covering rectangle is the wedge the triangle actually spans. The budget
   is today's `min(old max, d_ε)`, the old max read on the developed old
   triangles.
10. **Orientation** is read from the boundary: a directed boundary edge of
    the old triangles, else a directed cycle edge, must be traversed the
    same way (a caller that shortens a cycle may leave no old boundary edge
    standing).
11. **Seeded retry.** A CDT that certifies above the budget is retried with
    a deterministic interior grid of surface points (`i2e_seed_grid`, on the
    chart polygon) at the replaced triangles' own sampling scale, halved
    once: the widest old triangle along an axis the surface curves in (a
    nonzero second derivative across the tangent plane at the chart centre),
    as an arc length, the smaller of the two where it curves in both. The
    d(T) gate re-verifies every attempt.
12. **Coincident-simplex points** (N106 increment 2). A re-plan inserts one
    surface point per simplex the other operand's mesh also holds: the chart
    centroid of a triangle or midpoint of an edge, its corners read on the
    copy nearest one chart vertex, placed on the copy inside the polygon
    (`DomainChart::place`).

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
   `(−π, π]`); gate: corpus held. **Landed 2026-11-20 (§6).**
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

**`InteriorBranch` retired (same day).** Anchor (R0096, a tube band): the
refused interior vertex sat 7.9e-5 from the nearest cycle vertex, outside the
polygon on every copy — relocated past the curve. The `SurfaceChart` CDT drops
such a vertex silently (its triangles classify outside); §4.4.1 removes a
vertex too close to the curve (`:563-564`), and one past it is the extreme
case. The domain chart now removes it, counted (`removed_outside` in the
`DOMAIN` census line); the batch's foreign-reference scan still guards it.
Corpus: **360C / 0W / 9E / 5EE / 0T, no category move**, hash moved only on
P0049 (CORRECT, oracles pass): its two cylinder bands (2,225 and 8,874
triangles, the old `ThetaUnwrap` refusals) now rebuild, removing 29 and 15
vertices. Removals elsewhere: P0034 (3, 12), R0026 (4, 4), R0050 (1, 2, 6,
9), R0085 (2, 2), R0096 (3). Torus and sphere patches now: 7 rebuilt, 77
refused by the d(T) budget, 5 crossing cycles, 1 `Lattice`.

## 6. Increment 3 measured (2026-11-20)

The domain rebuild first gained what the `SurfaceChart` rebuild had and it
lacked: the seeded retry (§3 item 11), the coincident-simplex points
(item 12) and the affine interior rule (item 8).

**Per-patch comparison**, one release binary, both rebuilds run on every
patch the `SurfaceChart` rebuild serves (16,249 patches), the corpus at
1906b5e8:

| surface | same outcome | differs |
|---|---|---|
| plane | 12,182 (11,871 rebuilt, 306 crossing cycles, 5 duplicate vertex) | 0 |
| cylinder | 884 (295 rebuilt, 572 d(T) budget, 14 crossing, 3 duplicate) | 0 |
| cone | 2,619 (2,156 rebuilt, 440 d(T) budget, 23 crossing) | 564 |

563 of the cone differences are the d(T) budget; the other is a patch
(R0003) whose cycles crossed in the old chart only.

- **505 are rounding**, all in seven gear cases (R0003, R0019, R0032, R0044,
  R0047, R0053, R0081): the refused side's bound exceeds the budget by less
  than 1 part in 10¹² (R0003 patch 15: `1.2536554243056315` against
  `1.2536554243056457`). 304 the old chart refused and the new rebuilds, 201
  the reverse. The rebuild returns triangles with the bound of the ones it
  replaces, and the strict comparison `new > min(old max, d_ε)` is decided
  by the rounding of d(T) on two azimuth branches.
- **58 are real, and every one is a patch the old chart refused and the new
  rebuilds**: 57 with seeds, 1 without (54 in the gear cases, one cone patch
  each in P0008, P0017, P0049, P0052). The seed grid's spacing along the
  axis now reads the cone's radius at the chart centre; the old read it at
  the widest station.

**Flipped.** Corpus: **360C / 0W / 9E / 5EE / 0T, no category move, every
oracle detail string identical on all 374 cases.** Canonical geometry hash
identical on 364 cases; the ten that moved are the seven gear cases above,
P0008 and P0049 (a seeded cone patch each) and R0025 (a torus patch rebuilt
by the seeded retry). The update's census: cone patches rebuilt 2,357 → 2,519
(d(T) refusals 803 → 642), torus patches rebuilt 7 → 20 (the seeded retry:
P0050 4 → 7, R0032 2 → 10, R0025 and R0050 one each), plane and cylinder
counts unchanged. 16,345 domain charts: 15,893 DISC, 409 U-BAND, 41 V-BAND,
2 CAP. The coincident-simplex separation re-planned no patch on the corpus
(as before).

**Open, named.**

- The d(T) budget comparison is decided by rounding where a rebuild returns
  triangles of equal bound. §4.4.1 recalculates d(T) "for the newly
  generated boundary triangles" (`:566-568`); a triangle the CDT returns
  unchanged is not newly generated and should keep its bound. N106.
- The construct pass and the fan-local rebuilds still chart with
  `SurfaceChart` (§2).

## 7. The chain decimation's whole-patch fallback (2026-11-20)

`decimate_patch_chains_441` rebuilds a touched patch fan-locally and, where
that is not certified, by a whole-patch CDT. The fallback was the
`SurfaceChart` rebuild, gated to plane, cylinder and cone; a sphere patch
declined before reaching either (`declined_sphere_unsupported`). It is now
`rebuild_patch_domain` for every surface and the sphere skip is deleted: a
sphere patch reaches the fallback and declines only as the rebuild does.

Corpus: **360C / 0W / 9E / 5EE / 0T, no category move, every oracle detail
string identical**; geometry hash moved on R0019 and R0081 only (both
CORRECT). The fallback ran 63 times in 15 cases: 19 rebuilt (13 cone, 5
plane, 1 cylinder), 36 refused by the d(T) budget (17 cone, 12 cylinder, 7
torus), 6 plane rebuilds not certified, 2 crossing cycles. No sphere patch
reached it on the corpus; the unit pins hold a sphere patch through a pole
(`DegenerateOnCycle`) and one clear of the poles (charted, refused by the
d(T) budget).

**Next:** an unchanged triangle keeps its bound in the d(T) comparison
(§6, N106); then F2's next unchecked item.
