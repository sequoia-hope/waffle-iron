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

**Increment 3 (2026-07-06) — diagnosis: NOT cherchi robustness; Stage-0 emits a
self-intersecting, non-conformal mesh at ULP-twin rim splits.** Instrumented
(new `CHERCHI_ENFORCE_PROBE` in cherchi-rs soup.rs + `YANG_STAGE0_DUMP_DIR`
mesh dump): the failing base_tri 546 is a CAP-cylinder lateral triangle whose
submesh holds 3 same-point Lpi vertices on one edge plus a 1-ULP-twin Lpi pair —
mesh B genuinely self-intersects there. Root cause chain:

1. The exact trapezoidal overlay legitimately mints **femto-twin split pairs**
   (two sweep-event columns 1 ULP apart in `u`, from two distinct rim samples
   whose mirrored x-coordinates differ by 1 ULP) on every chord they cross.
   The twins are exact, distinct, and consistent inside the overlay.
2. `collect_ring_crossings` pushes them into `rim_overrides` in overlay-vertex
   INDEX order (not exact chord order).
3. The Stage-1 rim-ring slot sort (`lib.rs` rim construction) keys on **f64
   seam-relative angle** — the twins' angles collide (Δθ ≈ 4e-17 < ULP(θ) ≈
   2e-16) — so their ring order degrades to insertion order, independently per
   rim, in per-rim frames of OPPOSITE orientation.
4. `tessellate_lateral_azimuth_merge` re-sorts both rings by shared-frame f64
   azimuth (ties again) and pairs positionally → the strip quad between the
   twins TWISTS (observed: bottom ring `…605` before `…496`, top ring the
   projections in the opposite order; wall tri `(7,100,8)` orientation-flipped)
   and the cap-boundary walk disagrees with the lateral rim walk (`6→7→8→9`
   vs `6→8→7→10`).

Cherchi then correctly explodes on the tangle
(`DeepRecursionRequired/SegmentNotLocatable`). Fix plan (exact ordering, no
tolerances — P9):
- **F1 (stage0):** `collect_ring_crossings` sorts crossings by (chord index,
  exact `t`) before pushing — insertion order becomes the exact boundary order
  (also makes probes deterministic).
- **F2 (lib.rs ring build):** the rim-ring slot sort breaks Override-vs-Override
  f64-angle TIES with the exact sign of the 2D cross product of the two points'
  frame coordinates (RBig over the raw f64 inputs — exact). Uniform-vs-override
  ties are impossible (merge_tol guard is 1e-6·step ≫ ULP).
- **F3 (lib.rs lateral pairing):** the azimuth-merge per-ring sort gets the same
  exact tie-break in the SHARED frame, so tied clusters pair by true angular
  order on both rims.

Residual accepted risk (documented): a 1-ULP f64 atan2 inversion (strict
misorder, not a tie) or an azimuth-order inversion introduced by the f64
opposite-rim radial renormalisation would still mispair — both remain LOUD
(cherchi wall), never silent-wrong. Oracle: new lib.rs unit test pins exact
twin order on both rings + untwisted wall pairing (RED on tie-order today);
e2e `annular_cap_under_disc_union_succeeds` un-ignored; then the chained
swiss-cheese corpus cases F0086–F0090.

**Increment 3 (2026-07-06) — SHIPPED (F1+F2+F3 + two CDT fixes + §4.4.1(b)
merge extension); `annular_cap_under_disc` GREEN end-to-end.** What landed
beyond the plan above:

- **F2/F3** as planned (`exact_rim_ccw_tiebreak` in yang-rs lib.rs, used by
  the rim-ring slot sort and the lateral azimuth-merge per-ring sort);
  **F1** as planned (`collect_ring_crossings` sorts by (chord, exact t)).
  Unit oracle `rim_override_ulp_twins_exact_order_both_rims` (RED→GREEN,
  mutation-checked: neutering the tie-break flips it RED).
- **CDT fix 1 (cherchi-rs):** after the ordering fixes the wall moved to the
  Stage-1 annular-cap CDT: the plain `cdt_polygon_with_holes` classifies
  interior faces by f64 CENTROID PARITY, which misclassifies ULP-twin femto
  slivers along boundary chords (the F0047 "parity slitting" class) — the
  cap emitted constraint edges used 0×/2×. yang-rs's
  `tessellate_planar_curved_cdt_face` now uses the topological
  `cdt_polygon_with_holes_floodfill` (the same migration kernel-v2's render
  cores made).
- **CDT fix 2 (cherchi-rs):** the floodfill variant's HOLE exclusion was
  still f64 centroid parity → bore-rim twin slivers misclassified. Hole
  parity is now EXACT (`centroid_in_polygon_exact`, pure `RBig` over the
  raw f64 inputs). Regression fixture: `tests/ulp_twin_cdt.rs` (the real
  annular-cap CDT input captured bit-exactly; full boundary-conformality
  oracle: every constraint edge used exactly once, every interior edge
  exactly twice).
- **§4.4.1(b) merge widening TRIED AND REVERTED (P10):** a global scan +
  Stage-4 ENTRY pass was implemented to merge the twins before Phase-A
  curve assignment, and REVERTED after the corpus P9 gate flipped R0091
  (micro scale 1.6e-4) to SUPPORTED_WRONG (Euler −4): the ABSOLUTE
  `MIN_FEATURE_SIZE` floor collapses legitimately-distinct arrangement
  geometry at micro model scale. The (3c) relocation/conic-adjacent
  eligibility is LOAD-BEARING (documented at the block); any future twin
  merge must be scale-aware or happen at Stage-0 with full provenance
  (increment 4).

**Known residual (quarantined, follow-up task #61):** the R0072-class micro
fixture `n2_rim_mint_adversary::crossing_one_ulp_inside_rim_sample`
regresses from accidentally-green to off-band-Ok: its box side sits 1 ULP
inside a rim sample, the twin split pair's on-circle mints were FOLD-GATE
REVERTED at Stage 0 (the twin wedge folds), and the chord-position twins
carry no conic assignment at Stage 4 (YANG_S4_TWIN_PROBE: moved=false,
circle/line=false), so nothing relocates them to the rim circle and one
lands in a B-Rep loop (residual = the chord sagitta, 6e-6). The old twin
ORDER happened to thread the boundary elsewhere. Root fix (increment 4 direction): collapse
sub-floor mint pairs consistently AT STAGE-0 EMISSION (mint both twins to
ONE shared on-circle target; the existing 3D-duplicate weld already handles
the resulting 2D-distinct/3D-identical boundary pair — measured working in
the same fixture at the box↔circle junctions v19/v23), dissolving both the
fold and the twin before the arrangement. P9 note: the new exact ordering
is provably more correct at its layer (unit + fixture oracles above); the
quarantine documents a latent Stage-0 mint-gate/Stage-4 assignment gap the
old ordering masked by luck, not a new wrongness.

**Increment 5 (2026-07-06) — chained swiss-cheese (task #62): two chain walls
fixed; family bottleneck now = on-chord rim points (increment 4's root).**
The F0086 chain (disc plate + 5 same-plane cut-cylinders) walled at cut 2.
Two independent defects, both in the recovered-B-Rep re-entry path:

1. **Azimuth-merge wrap split (yang lib.rs, `tessellate_lateral_azimuth_merge`):**
   a RECOVERED rim can carry its seam vertex at y = −ε, which
   `rem_euclid(2π)` maps to 2π−ε — sorted LAST while the other rim's
   bit-zero seam sorts FIRST, shifting the positional pairing by one slot
   ("rims disagree at index 0 (bottom 0 vs top 0.4488)"). The two sorted
   rings are CIRCULAR sequences: the pairing (and the multiset check) now
   aligns them by cyclic shift (top[shift] circularly nearest bot[0]).
   Unit oracle: `rim_override_wrap_seam_cyclic_alignment`.
2. **Closed-rim fallback split by vertex count (kernel-v2 `recover.rs`,
   `closed_fallback_pieces`):** with the coplanar overlay's femto-spaced
   crossing clusters, a vertex-count "third" of a closed rim can subtend
   MORE than π; the downstream minor-side arc derivation then reconstructs
   the wrong side and the reassembled rim loop walks out-and-back (net
   winding 0 — the engine's "cylinder patch must have exactly 0 or 2
   axis-wrapping loops" wall). The fallback now splits by ACCUMULATED
   sweep (every piece < MAX_ARC_PIECE_SWEEP < π), mirroring the open-chain
   builder. e2e oracle: `kernel-v2/tests/m8_swiss_cheese_chain.rs`
   `two_through_holes_chain` (F0086's bit-exact parameters, volume band).

Result: chained swiss-cheese cut 2 GREEN end-to-end; F0086/F0089 move
ERROR → UNSUPPORTED(curved-profile) (typed boundary); F0086 assay pin
updated. Cut 3+ walls at the TYPED to_yang re-entry boundary: the cut-2
output's z=0 rim is a MIXED chain — on-circle posts + on-chord overlay
cluster points (off-circle by up to the Stage-1 sagitta) — so recover
cannot circle-fuse it, the lateral loses its canonical anchor, and the
multi-piece rim cannot re-enter Stage 1 (pinned loud:
`third_cut_stays_loud_typed_reentry_wall`; 5-hole chain `#[ignore]`d on the
milestone). F0087/F0088/F0090's residual ERRORs are `VertexOffSurface` —
the same on-chord population reaching loops. **The whole family now
bottlenecks on ONE root: Stage-0 mints rim-polygon crossing points ON
CHORDS instead of on the exact circle — increment 4's shared on-circle
mint design (task #61) is the next lever, and would also restore the
canonical-anchor path in recover.**

**Increment 4 (2026-07-06) — sub-floor shared-mint collapse SHIPPED (task
#61); `crossing_one_ulp_inside_rim_sample` un-quarantined GREEN.** Two
changes in `stage0.rs`, both scoped to the N2-3a mint path (full design:
spec `n2_stage4_junction_cluster_merge` §3 amendment 3):

1. **Shared-mint collapse:** after coords resolution and BEFORE the fold
   gate, minted on-circle vertices are grouped per rim-ctx slot by 3D
   distance < `MIN_FEATURE_SIZE` (greedy first-seen — sub-floor groups are
   isolated, real crossings sit ≥ the floor apart) and each multi-member
   group collapses to ONE shared on-circle target (a crossing-branch member
   if present — I2 keeps the junction on the other input's edge — else the
   first member; never an average). `RimResolve::OnCircle` now carries the
   `crossing` provenance flag for that preference.
2. **Gate degeneracy skip:** the fold gate ignores triangles whose resolved
   3D image is bit-degenerate (the M-B emission-drop class) — the collapsed
   twin wedge projects to 2D area exactly 0 and would otherwise revert its
   own shared mint. Judgment on every EMITTED triangle is unchanged.

The collapse dissolves both the fold and the twin before the arrangement:
`collect_ring_crossings` dedups the identified pair to one rim override, the
M-B filter drops the degenerate wedge at emission, and the existing weld
pairs the 2D-distinct/3D-identical boundary (as measured at v19/v23).
Oracles: the increment-3 quarantined R0072-class adversary
`n2_rim_mint_adversary::crossing_one_ulp_inside_rim_sample` un-ignored →
GREEN (single body, watertight, fully on-band; RED before the fix: Ok output
with 2 loop vertices off-band by the 6.006e-6 chord sagitta); the full
9-probe adversary suite (fold-gate containment probe 4, determinism sweep
probe 7) unchanged-green; yang-rs 540/0; rewrite tier green. Corpus P9 gate
(294 cases, 120s cap, quiet box): **0 SUPPORTED_WRONG, 175 CORRECT, zero
CORRECT lost**; vs the checked-in baseline the run is strictly quieter — 18
baseline TIMEOUTs resolved to real verdicts (10 → CORRECT incl. R0001/R0013/
F0063, 4 → UNSUPPORTED, 3 → known-wall ERROR, 1 → EXPECTED) and no case
flipped toward TIMEOUT/WRONG (baseline results.json refreshed in the same
commit). F0087/F0088/F0090 remain ERROR (chained on-chord population —
task #62 re-measures the family on top of this increment).

**Increment 6 (2026-07-07, task #62) — annular rim-mint contexts.**
Re-measure after increment 4: cut 3 still walls
(`UnsupportedCurvedBoolean(FaceId 15)`, `full_f0086_five_hole_chain` RED;
`third_cut_stays_loud` pin holds). Diagnosis confirmed: the N2-3a mint
context (`rim_chord_ctx`) is DISC-ONLY — an annular face admitted by the
increment-1 overlay gets NO mint context, so crossing/subdivision vertices
on ITS rim chords resolve to raw chord lifts (off-circle by the Stage-1
sagitta). Those on-chord points populate the annular face's rim overrides
(`collect_rim_crossings` pushes resolved `coords`), reach the cut-2
output's z=0 rims as a MIXED on-circle/on-chord chain, defeat recover's
canonical circle-fuse anchor at cut-3 re-entry, and surface as the
F0087/F0088/F0090 `VertexOffSurface` residues. Fix: `rim_chord_ctxs`
(plural) builds ONE `RimChordCtx` per rim circle — a disc keeps exactly
today's single ctx (byte-identical); an annular face gets outer + one per
hole, each with its own chord ring and exact circle, sharing the partner
polygon's boundary sub-segments. The coords-resolution slot enumeration
and the increment-4 sub-floor collapse become per-circle across the
concatenated ctx list (the collapse was designed slot-per-circle for
exactly this).

**Increment 6 outcome (measured):** the cut-3 typed re-entry wall is
LIFTED — `full_f0086_five_hole_chain` (F0086's bit-exact parameters,
direct kernel-v2 constructors) RED→GREEN; the `third_cut_stays_loud` pin
fired its designed retire signal and was converted to the positive
regression `third_cut_reenters_multi_hole_plate` (cut 3 succeeds,
`validate_solid` clean). yang-rs 540/0, kernel-v2 251/0 (I6: no
regression). The CORPUS family however relocated to a deeper frontier
rather than clearing: the sketch-extrude + auto-union replay path trips a
residual loud `VertexOffSurface` population the direct chain does NOT
exhibit — F0086 1 error (FaceId 15; 5 of 6 ops now replay, vs the whole
chain walling before), F0087 3, F0089 6 (plus one reassembly
non-2-manifold), F0090 27; F0088 unchanged UNSUPPORTED (typed partial-
patch re-entry). F0086's smoke pin updated UNSUPPORTED(curved-profile) →
ERROR (wall moved DOWNSTREAM of the typed boundary, always loud). **Next
increment (the remaining #62 scope): localize the corpus-path residual —
instrument the kernel-v2 VertexOffSurface tripwires (no env-gated dump
exists there today), diff the corpus recipe (hole layout/order, blind
holes, auto-union) against the green direct chain, and fix at the
producing layer.**

**Increment 7 (2026-07-07, task #62) — corpus-path residual localized +
constrained flip repair SHIPPED.** Instrumentation first: the kernel-v2
`VertexOffSurface` tripwires got an env-gated dump (`KV2_OFFSURF_PROBE` —
site, point, residual, band, surface; nine checks share the variant). It
localized F0086's FaceId 15 to `cylpatch-vertex`: a z=0 vertex 3.4e-3 (the
Stage-1 sagitta) inside the plate's OUTER rim. The corpus-vs-direct diff
was NONE of the suspected recipe deltas — it is the PRODUCTION SKETCH
FRAME: `tangent_x_from_normal([0,0,1])` = (0,−1,0) (feature-engine
`rebuild.rs`), so every corpus profile lives in a frame rotated −90° from
the direct chain's canonical (1,0,0)/(0,1,0). The rotation is bit-exact
(swap/negate) but the Stage-0 overlay's sweep-event ORDER differs, and at
cut 2 the rotated alignment builds a femto-strip (two event columns ULPs
apart in u) whose diagonal sliver spans from a rim-chord vertex to the
overlap boundary. ANY on-circle mint of that rim vertex (displacement =
sagitta ≫ strip width) inverts the sliver; the amendment-2 fold gate
reverted the mint; the chord vertex escaped into the output rims. Fix (spec
`n2_stage4_junction_cluster_merge` §3 **amendment 4**): constrained Lawson
edge-flip repair in the fold gate — before reverting, flip an edge of the
folded triangle shared with a SAME-class neighbor (class boundaries are
intersection curves, never flipped) when both replacement triangles are
valid; revert stays the fallback. Oracles: engine-frame chain tests
(`m8_swiss_cheese_chain.rs::engine_frame_{two,five}_hole_chain`, RED at
cut 2 before / GREEN after), mutation checks (inverted replacement winding
and broken flip legality both caught), yang-rs + kernel-v2 suites green.
**F0086 corpus replay → SUPPORTED_CORRECT (pin flipped); F0089 6→2, F0090
27→22 errors; F0087 3 (unchanged); F0088 unchanged typed UNSUPPORTED
(partial-patch re-entry, separate wall). REMAINING family residual = a
SECOND fold class: contiguous rim-mint clusters displaced ~4.6e-2 — an
order beyond the sagitta and beyond the hole-to-rim clearance — with no
legal flip (constraint-bounded folds). Diagnose why those chords are that
deep (coarse recovered-rim sampling?) before choosing between Stage-0
strip refinement and the full Fig-11 cavity update.**

**Increment 8 diagnosis (2026-07-07, task #62) — rim-mint COLUMN HOP,
measured end-to-end.** NOT recovered-rim degradation: the F0087 plate's
rim is a FRESH Stage-1 uniform 14-gon. The Stage-1 chord bound is global
(`d_ε = 1e-2·AABB-diag`, `n_seg` smallest with sagitta ≤ d_ε), so the
r≈1.98 plate gets N=14 with sagitta ≈ 4.9e-2 — mint displacements are ~1%
of model size by construction. At F0087 cut 7 (probe `[fold-tri]` +
`[ring-density]` + `[stage1-nseg]`, since removed/kept-tiny), a rim-chord
mint's u-component (1.3e-2) exceeds the gap (8.9e-3) to the CURRENT
tool's leftmost-x sweep column — the mint HOPS a populated event column.
Every long CDT triangle in the strip between the two columns folds
together; amendment 4's single flips provably cannot repair it (each
folded tri's rim edge is domain boundary; its side edges neighbor other
FOLDED tris — `[flip-reject]` shows only `domain-boundary` +
`replacements invalid`), and the fold-cavity's boundary polygon is
NON-SIMPLE under the moved vertex (it pokes past the hopped column), so
cavity fan/ear re-triangulation of the folded set alone also cannot fix
it. Failing ops are exactly the cuts whose tool x-extreme column lands
inside a rim mint's displacement (F0087 holes 7/9/10). **Increment 8
scope: boundary-vertex RELOCATION — Yang §4.4.1 Fig 11 delete-and-
reinsert (remove the minted vertex's star, re-insert at the on-circle
position with constrained point location + cavity carve), the first full
mesh-updating consumer at Stage-0.** Boundary pinned:
`m8_swiss_cheese_chain.rs::f0087_cut7_stays_loud_offsurface_wall`
(cuts 1–6 green, cut 7 typed VertexOffSurface) + `#[ignore]`d green
target `f0087_engine_frame_seven_hole_chain`.

**Sampling-floor alternative MEASURED AND REJECTED (same session,
`YANG_NSEG_FLOOR` dev knob):** forcing the global rim N to 28 (sagitta
1.25e-2) still walls the F0087 chain — finer sampling shrinks the hop
threshold but tool sweep columns can still land inside the (smaller)
mint displacement, so the mechanism survives at every finite N (P9: a
frequency reducer, not a fix). N=56 (sagitta 3.1e-3) happens to clear
this chain but costs 61s vs ~12s (5×) on the 7-cut chain alone —
corpus-prohibitive under exact arithmetic. Increment 8's design is
therefore fixed on VERTEX RELOCATION, not sampling.

**Increment 8 (2026-07-07, task #62) — cavity relocation SHIPPED (spec
`n2_stage4_junction_cluster_merge` §3 amendment 5).** The Fig-11
delete-and-reinsert form in the Stage-0 fold gate: when no single flip
repairs a fold, the minted vertex's star is carved out and
re-triangulated around its resolved position — constrained visibility
growth (Bowyer–Watson) that DEFERS at uncrossable edges (class boundary
= intersection curve; domain boundary; pinch), then, if any deferral
remains (the cavity is not star-shaped from the mint: F0087 cut 7's
mint crosses the LINE of a tool chord whose segment lies elsewhere), a
constrained exact ear-clip of the cavity polygon — the constraint edge
stays a cavity BOUNDARY connected to other link vertices instead of
`v`. Build-then-commit; every reject falls back to the amendment-2
revert (loud). Oracles: `f0087_engine_frame_seven_hole_chain`
RED→GREEN (pin `f0087_cut7_stays_loud_offsurface_wall` fired its retire
signal → converted to `f0087_cut7_column_hop_relocates`); 3 unit
oracles on `relocate_minted_vertex` (fan-with-growth / pinch-defer→
ear-clip / reject-without-mutation), 2 mutation checks caught (inverted
ear orientation; neutered class-boundary constraint). The F0087 chain
needs exactly ONE ear-clip relocation (8-tri cavity). Corpus P9 gate
(294, parallel 240s cap): **0 SUPPORTED_WRONG, 183 CORRECT (+5: F0065,
F0071, F0080, R0031, R0075), zero CORRECT lost, 0 TIMEOUT; 10 cases
ERROR→typed UNSUPPORTED** (F0087, F0089, F0090, F0067, F0069,
F0082–F0085, R0061); results.json refreshed in-commit. **REMAINING #62
scope: the ENTIRE residual family F0087/F0088/F0089/F0090 now sits on
ONE typed wall — `boolean: curved partial-patch operand` (a previous
cut's partial cylindrical patch cannot re-enter to_yang) — the
partial-patch re-entry boundary, next increment's lever.**
