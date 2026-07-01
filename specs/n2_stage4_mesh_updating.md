# N2 — Stage-4 Mesh Updating (§4.4.1 CDT) — Spec

Closes deviation **N2** (`docs/yang_deviations.md`): Stage 4 today is
*relocation-only*; the paper (Yang 2025 §4.4.1, Fig 11) performs **mesh
updating** — insert each intersection polyline into the affected mesh patch as a
constrained boundary and re-triangulate via CDT, with `split` / `merge` /
`insert` vertex handling and a per-triangle `d(T)` recompute. Relocation moves
the crossing vertices onto the exact curve; mesh updating makes the *mesh
topology* conform to that curve so the trimmed patch is bijective with the
trimmed surface and contains no flipped / sliver triangles.

## Decomposition (N2 is an M5 sub-milestone)

Per roadmap §0.1 ("general over piecemeal"), we build the ONE general algorithm
the paper describes, not another per-conic special case. N2 lands in increments:

- **N2-1 (this spec / this PR): the parametric-domain mesh-updating primitive.**
  A pure, deterministic function that, given a patch triangulation in a 2D
  parametric domain and an ordered intersection polyline, returns the updated
  triangulation with the polyline realized as constrained edges — implementing
  Fig 11 `split` (polyline points become boundary vertices), `merge` (a patch
  vertex within `merge_tol` of a polyline point is fused into it), and `insert`
  (a closed intersection loop enclosing no patch vertex receives one interior
  point). CDT backend = `cherchi_rs::triangulation`. **Not wired into the
  pipeline** — unit-tested in isolation against §4.4.1's invariants.
- **N2-2:** per-triangle `d(T)` recompute for the boundary triangles the update
  generates (§4.4.1 last sentence; controllable-error metric).
- **N2-3+:** wire the primitive into `stage4_relocate_and_correct`, extracting
  each affected face patch's parametric domain, replacing the
  `LocalRefinementRequired` bailouts one surface-pair family at a time, each
  behind a watertight / reference-parity oracle.

This spec covers **N2-1 only**. N2-2/N2-3 get their own specs.

## 1. Goal

A function `stage4_mesh_update(patch, polyline, opts) -> Result<PatchUpdate>`
that updates a single mesh patch (already triangulated in a 2D parametric
domain) so that a given intersection polyline becomes a chain of constrained
edges of the re-triangulated patch, faithfully to Yang 2025 §4.4.1 / Fig 11.

## 2. Parameters

Input `patch`:
- `verts: Vec<Point2>` — the patch's parametric-domain vertices.
- `boundary: Vec<u32>` — outer boundary loop, CCW, indices into `verts`.
- `holes: Vec<Vec<u32>>` — inner boundary loops (existing holes), indices.
- (interior patch vertices are the `verts` not named by any loop.)

Input `polyline`:
- `points: Vec<Point2>` — ordered intersection points in the SAME parametric
  domain. An `open` polyline runs boundary→boundary (a chord splitting the
  patch); a `closed` polyline is a loop interior to the patch (a punched hole).
- `closed: bool` — whether `points` forms a closed loop (last connects to
  first).

Input `opts`:
- `merge_tol: f64` — Fig 11(b): a patch vertex nearer than this to a polyline
  point is merged. Default caller-supplied; must be `> 0` and `< d_eps` (a merge
  must not move a vertex off the curve budget). Valid range `(0, d_eps)`.
  **Boundary-preserving rule:** a merged BOUNDARY vertex is kept FIXED (the curve
  point snaps onto it) and a boundary-edge split point is PROJECTED onto the
  edge, so the boundary polygon never moves — merges/splits only re-partition the
  interior. Only interior patch vertices are moved onto the curve. (This is the
  guarantee behind I4; without it a curve point sitting perpendicular-off a
  boundary vertex would drag the boundary inward and change the area.)
- `d_eps: f64` — the Stage-1 chord budget (`stage4_chord_band`); a polyline
  point must lie within `d_eps` of the patch (in-domain) or it is not this
  patch's crossing (`OffPatchBeyondChordBand`).

Defaults: none are optional; the caller (N2-3 wiring) supplies all. Units:
parametric-domain units (dimensionless u,v).

## 3. Branch table

| Case | `polyline.closed` | Merge hit? | Loop encloses a patch vertex? | Behavior |
|------|-------------------|------------|-------------------------------|----------|
| Open chord | false | — | n/a | Split boundary at entry/exit points; polyline becomes an interior constrained chain; CDT both sides. |
| Open chord, endpoint near existing boundary vertex | false | yes | n/a | Snap the endpoint onto that boundary vertex, kept fixed (Fig 11 b/c); then as above. |
| Closed loop, non-empty | true | maybe | yes | Add loop as a new hole constraint; CDT the annulus + the loop interior as separate patches. |
| Closed loop, empty | true | maybe | no | Insert ONE interior point at the loop centroid (Fig 11 insert `i`), then as above. |

No implicit modes. `merge` is applied uniformly in every case as a pre-pass.

## 4. Invariants (measurable)

For the returned `PatchUpdate { verts, tris }`:

- **I1 (constraint realized):** every consecutive pair in the (possibly merged)
  polyline is an edge of some output triangle. (Split faithful: polyline points
  are boundary vertices of the trimmed sub-patches.)
- **I2 (no flips):** every output triangle has the SAME signed-area sign as the
  input patch's orientation (CCW). No inverted triangles (§4.4.1: "no flipping
  triangles since the intersection curves are regular").
- **I3 (boundary→boundary):** the original outer boundary vertices (minus any
  merged-away) all remain on the output boundary; no original boundary vertex
  becomes interior. (Paper: "maps boundary curves to boundary curves".)
- **I4 (area conservation):** total signed area of output tris == signed area of
  the input patch, within `1e-9` (merge/insert/re-triangulation is a
  re-partition of the SAME region — it neither adds nor removes area).
- **I5 (merge monotonicity):** a merge strictly reduces the vertex count by the
  number of merged patch vertices; an empty-loop insert increases it by exactly
  the loop count. (Mutation-sanity oracle for the merge/insert branches.)
- **I6 (determinism):** two calls on identical input return byte-identical
  `verts` and `tris` (inherited from the canonicalized CDT).

## 5. Oracles

- **Canonical (open chord):** unit square patch `[0,1]²` (2 tris), polyline the
  diagonal chord `(0,0)→(1,1)` already present as an edge → I1 (diagonal is an
  edge), I2 (both tris CCW), I4 (area == 1). A chord `(0,0.5)→(1,0.5)` NOT
  aligned to the existing diagonal → output has the horizontal chord as an edge,
  I1–I4 hold, vertex count grew by the 2 new boundary points.
- **Merge branch:** square patch with an extra boundary vertex at `(0.5, 0)`;
  polyline endpoint `(0.5+ε, 0)` with `ε < merge_tol` → I5 (that vertex merged
  away, count net change = polyline-points − 1), I1–I4 hold.
- **Insert branch:** square patch (4 boundary verts, no interior); closed
  triangular polyline loop strictly inside → I5 (exactly one interior point
  added: the loop centroid), the loop is a hole of the outer region and a
  boundary of the inner region, I2/I4 hold.
- **Mutation sanity:** flip the merge comparison (`>` vs `<`) → the merge test
  must fail (proves the merge branch is exercised, not dead).
- **Determinism:** two invocations, assert `==` on the whole `PatchUpdate`.

## 6. Failure modes

Loud `Result::Err` (P9/P10 — never a silent snap / tolerance widen):

- `PolylineOffPatch` — a polyline point lies farther than `d_eps` from the patch
  region (not this patch's crossing).
- `MergeTolTooLarge` — `merge_tol >= d_eps` (a merge could move a vertex off the
  curve budget) or `merge_tol <= 0`.
- `SelfIntersectingPolyline` — the polyline crosses itself or a patch hole
  boundary (CDT constraint conflict; we never Steiner-split to resolve it).
- `CdtFailed(CdtError)` — the CDT backend rejected the constraints.
- `DegeneratePolyline` — fewer than 2 points (open) / 3 points (closed), or
  consecutive coincident points.

Every failure names the offending index where applicable.

## 7. Research basis

- **#24 Yang et al. 2025 §4.4.1 + Fig 11** — the mesh-updating algorithm this
  implements (`refs/text/yang2025_hybrid_boolean.txt:546-573`). Split = "locate
  the constrained edge containing q … split it using q"; merge = "if an endpoint
  p of the split edge is too close to q, we merge p with q"; insert = "If there
  are no other mesh vertices within an intersection loop, we insert one point i
  into it."
- **CDT backend:** `cherchi_rs::triangulation::cdt_polygon_with_holes*` (spade
  v2, exact `robust` predicates, deterministic, WASM-clean). This is CDT, not
  ear-clipping (deviation D1 forbids ear-clipping here).

### 7a. Analytical vs approximate

The primitive operates in the **parametric domain** and re-partitions an
existing exact-boundary region; it introduces no surface approximation. The
intersection polyline points are supplied by the exact SSI/relocation upstream
(Stage 3/4). No quadric-pair SSI is performed here (that is Stage 3 / `ssi-rs`),
so A15 surface-pair coverage is N/A for this primitive. The CDT operates on 2D
parametric coordinates with exact orientation predicates → no robustness debt.

## 8. Scope / non-goals (this PR)

- No pipeline wiring (that is N2-3). The primitive is exercised only by its unit
  tests.
- No `d(T)` recompute (that is N2-2).
- No 3D↔parametric extraction of real face patches (that is N2-3).
- Curved-boundary CDT (NURBS §4.1.2) stays out of scope (N5 / NURBS milestone).
