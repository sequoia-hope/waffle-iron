# M8 — Holed-disc coplanar overlay (Stage-0 generalization) + N4 provenance

Status: **INCREMENT 2 GREEN (2026-07-05).** The polygon-partner holed-disc
containment case is CORRECT end-to-end (watertight/outward/volume oracles);
full yang-rs suite 537/0. Remaining: increment 3 (disc-partner with a disc rim
interior to the annulus → cherchi arrangement robustness) and the chained
corpus cases F0086–F0090. Increment of the general §4.5.5 Stage-0 program
(`docs/yang_functional_roadmap.md` M8) and the N4 face-provenance campaign
(`docs/yang_deviations.md` N4). Predecessor slices: exact 2D overlay engine
(YR25), `boolean()` wiring (YR26), flat-disc∩polygon containment (M8-disc),
disc∩disc crossing (`kernel_v2_m8_disc_disc_crossing`), coincident-cylinder
provenance (`n4_coincident_cylinder_provenance`).

## 0. Goal

Generalize Stage-0 §4.5.5 coplanar preprocessing to admit a **planar face with
circular inner-loop holes** (an annular / "swiss-cheese" cap face) as a
participant in a coplanar A×B pair, replacing the current loud
`CoplanarFacesUnsupported` (`stage0.rs` `overlay-face-unsupported` wall) with a
correct overlay. Emit N4 per-triangle provenance (`tri_face`) for the resulting
triangles so Stage-6 attributes them by producer lineage, not geometric
proximity.

**Why this is the target.** A full-corpus Stage-0 wall census (2026-07-05,
`YANG_COPLANAR_PROBE=1`) shows `face-unsupported` is the dominant remaining
Stage-0 wall (76 raw hits → 6 distinct cases). Five of those six are the
**swiss-cheese disc** family F0086–F0090 (`outer=1 holes=1 circle=2`,
`expect_rebuild_error: false` → genuinely expected-supported). `disc_circle_edge`
(`stage0.rs`) admits only a hole-free single-circle disc, and
`overlay_face_supported` rejects any face carrying a non-`LineSegment` edge — so
an annular cap falls straight through to the loud wall. The exact 2D overlay
engine (`coplanar_overlay`) ALREADY consumes `PolygonWithHoles`; the gap is the
admission gate + tessellating the circular outer/hole rims into that polygon.

## 1. Parameters (inputs)

Unchanged public surface (`boolean(a, b, op)`). Internal Stage-0 only:
- **Face admission** (`overlay_face_supported`, `disc_circle_edge` / a new
  `annular_disc_face` classifier): a planar face whose outer loop is a single
  `Curve::Circle` and whose inner loops are each a single `Curve::Circle`
  (concentric or not, non-overlapping — the B-Rep guarantees valid loops)
  becomes overlay-eligible.
- **Polygon extraction** (`face_polygon_2d_tessellated`): tessellate the outer
  rim AND each hole rim into the `PolygonWithHoles { outer, holes }` in the pair
  frame, minting rim vertices on the exact `Curve::Circle` (N2-3a rim-mint
  discipline — every overlay boundary vertex on its face's surface).
- **N4 provenance** (`build_stage0_mesh`): the re-tessellated annular face's
  triangles tag `tri_face = f_idx` (they already would, via the override path);
  the invariant is that NO annular-face triangle carries the `u32::MAX` sentinel.

## 2. Branch table

| Face A | Face B | Overlap | Handling |
|--------|--------|---------|----------|
| plain disc (0 holes) | plain disc/polygon | containment / crossing | EXISTING (M8-disc, disc∩disc) — unchanged |
| **annular disc (≥1 circular hole)** | plain disc / polygon | containment | **NEW** — tessellated `PolygonWithHoles` overlay |
| **annular disc** | **annular disc** | containment | **NEW (increment 2)** — both faces holed |
| annular disc | any | CROSSING (overlap boundary cuts a hole rim) | walled this increment (`disc-hole-crossing`), loud residue |
| planar face w/ mixed line+arc edges (F0075) | — | — | out of scope (separate frontier) |

Increment 1 (this spec): the single **NEW** containment row where exactly one
face of the pair is annular and the overlap does not cross a hole rim. Everything
else keeps its current behavior (existing rows byte-identical; new-but-unscoped
rows stay the loud `CoplanarFacesUnsupported` residue — P9).

## 3. Invariants

- **I1 (rim on surface):** every minted outer/hole rim vertex lies on its exact
  `Curve::Circle` (‖p − center‖ = r within TAU_WORK; N2-3a).
- **I2 (exact coverage):** the overlay's rational post-conditions hold —
  `area(AOnly)+area(Overlap) == area(A)` in `RBig`, every input edge tiled
  gap-free, no zero-exact-area triangle (inherited from `coplanar_overlay`;
  holes reduce `area(A)` exactly).
- **I3 (identical overlap mesh):** Overlap triangles are bit-identical in both
  solids' Stage-0 meshes (§4.5.5 shared common surface).
- **I4 (N4 completeness):** no triangle of a re-tessellated annular face carries
  the `u32::MAX` provenance sentinel — every one attributes to its owning face.
- **I5 (output correctness):** the boolean OUTPUT is watertight, orientable,
  positive-volume, and matches the case's Euler / bbox / volume-monotonicity
  oracles. (The GREEN gauntlet = the assay `SUPPORTED_CORRECT` checks.)
- **I6 (no regression):** all-planar and hole-free-disc corpus cases are
  byte-identical (the admission gate only ADDS the annular arm; hole-free faces
  never reach it).

## 4. Oracles

- **RED (canonical, isolated):** a NON-chained minimal fixture — an annular disc
  solid (outer R, one concentric bore r) unioned/subtracted with a coplanar
  partner whose cap overlaps the annulus — replays to oracle-correct geometry
  (watertight + Euler + volume). This proves the Stage-0 fix in ISOLATION,
  BEFORE the chained corpus cases (P10: a minimal fixture localizes the fix and
  guards against the wall merely moving downstream to chained-output re-entry).
- **RED (corpus):** F0086 (the smallest swiss-cheese, 6 ops) replays to
  `SUPPORTED_CORRECT` via `assay_kv2`. F0087–F0090 follow as they clear.
- **Provenance oracle (I4):** a yang-rs unit test on `stage0_preprocess` for the
  annular pair asserts every annular-face `tri_face` entry is a valid face index
  (no sentinel).
- **Edge/adversarial:** hole rim tangent to the overlap boundary; hole rim
  touching the outer rim (thin web); multiple holes; a hole entirely outside the
  overlap (A-only) vs entirely inside (reduces Overlap area).

## 5. Failure modes

- Overlap boundary CROSSES a hole rim → `disc-hole-crossing` loud residue this
  increment (rim-crossing propagation into the bore lateral is increment 3).
- Annular-on-annular → increment 2 (both faces holed; the overlay handles two
  holed polygons but the rim-mint + provenance need the symmetric treatment).
- Overlay `RoundingCollapse` (a femto-sliver from f64 rim projection) stays the
  existing typed error — never silent.
- A hole loop that is NOT a single circle (e.g. a polygonal pocket) → not
  admitted by `annular_disc_face`; stays the loud residue (out of scope).

## 6. Research Basis

- **[#24] Yang et al. 2025 §4.5.5** — coplanar 2D Boolean before discretization;
  the overlap becomes a shared trimmed common surface with identical meshes. A
  holed face is a valid planar trim region; the paper's method is topology-
  agnostic over the loop structure (Fig. 16 shows arbitrary trims).
- **[#24] §4.2.3** — per-triangle provenance (N4): the re-tessellated annular
  face's triangles map to their B-Rep face via lineage, not geometry.
- **[#39] Livesu et al. 2021** — the CDT the overlay's exact ear-clip realizes
  for a polygon-with-holes cell decomposition.

## 7. Analytical vs. Approximate

Method: **exact** (rational `RBig` overlay + exact rim-circle vertex mint). The
hole rims are tessellated to `d_ε`-chord `PolygonWithHoles` boundaries exactly as
the outer disc rim already is (A15/A14.3 — the SAME Stage-1 chord bound, not a
new tolerance). No SSI change: plane∩plane seam curves come from the overlay
boundary (`Curve::LineSegment` / the hole `Curve::Circle`), analytic and exact.
Mesh is an exact computational intermediate, never the final representation
(A15.6).

## 8. Progress log

**Increment 1 (2026-07-05) — Stage-0 machinery LANDED, GREEN blocked downstream.**
Implemented in `crates/yang-rs/src/stage0.rs`:
- `annular_disc_face(brep, fi)` classifier — planar face, single-circle outer
  loop + each inner loop a single closed circle → `(outer_edge, [hole_edges])`.
- `overlay_face_supported` admits annular faces; `stage0_preprocess`'s disc
  fast-path (`build_disc_pair`) is bypassed when either face is annular (routes
  to the general `PolygonWithHoles` overlay).
- `annular_rim_rings` — extracts outer + each hole rim from Stage 1's own
  tessellation (no interior Steiner in the planar-curved CDT → every vertex is a
  rim vertex), classifying each vertex to the circle it lies on (robust for
  off-centre holes); `face_polygon_2d_tessellated` builds `PolygonWithHoles` +
  a rim_map over all rim points.
- `collect_rim_crossings` refactored: the per-ring body is `collect_ring_crossings`;
  the dispatcher runs it for the outer rim (→ outer lateral) and each hole rim
  (→ bore lateral) via `lateral_for_cap(rim_edge)`.

RED test `crates/yang-rs/tests/m8_holed_disc_coplanar.rs`: the crossing-boundary
test passes (loud residue preserved); the two containment tests now traverse
Stage 0 and are `#[ignore]`d on the **downstream** wall.

**Increment 2 (next) — the blocking wall.** kernel-v2 reassembly fails:
`azimuth-merge rims have mismatched samples (24 vs 30)`. Root cause (probe
`YANG_SPLIT_PROBE=1`): a hole's interior trapezoidal sweep-event lines subdivide
the OUTER rim with femto-twin near-duplicate split points (t=0.30717…496 vs
…497), which the opposite-rim azimuth projection (`collect_ring_crossings`,
720-step grid + refine) collapses inconsistently, so the outer lateral's two
rims end with different sample counts. This is the R0078/R0088 rim-merge
femto-twin class. Fix: femto-dedup the outer-rim split set bit-exactly and
mirror the IDENTICAL set onto both rims (exact opposite-rim placement, not an
f64 azimuth grid search). Then F0086 → SUPPORTED_CORRECT.

**Increment 2 (2026-07-05) — GREEN (polygon partner).** The `24 vs 30`
azimuth-merge wall was the OPPOSITE-RIM projection: `collect_ring_crossings`
placed each cap-rim split onto the opposite rim by a 720-step f64 azimuth grid
search, which collapsed femto-close split pairs to a single theta (probe:
`cap_pts=18 → opp_entry=12`), desynchronising the shared lateral's two rims.
Fix: replace the grid search with EXACT AXIAL PROJECTION — strip the split
point's axial component, renormalise the radial to `opp_radius`, re-attach at the
opposite rim centre (`stage0.rs collect_ring_crossings`). Direct 1:1, so both
rims get identical counts (`18 → 18`). `annular_cap_in_polygon_union_succeeds`
passes the full oracle gauntlet; full yang-rs suite 537/0 (no regression to the
existing disc rim-crossing cases the projection also serves). Corpus P9 gate
(assay_kv2, all 194): **0 SUPPORTED_WRONG, 86 CORRECT (no loss), ERROR 21→20**
(one disc-crossing case improved — the exact projection is more robust than the
grid search). F0086–F0090 stay ERROR/UNSUPPORTED (chained swiss-cheese →
increment 3).

**Increment 3 (next) — disc partner, disc rim interior to the annulus.**
`annular_cap_under_disc` (partner disc r=1.2 strictly between bore 0.5 and outer
1.5) walls in cherchi with `DeepRecursionRequired/SegmentNotLocatable`: the
overlap is a sub-annulus whose outer boundary is a NEW circle inside the annulus.
Separate cherchi-arrangement-robustness frontier. Then the chained swiss-cheese
corpus cases F0086–F0090.
