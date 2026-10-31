# Phase 2b — §4.4.1 as ONE per-patch update (merge-with-retriangulation)

**Status:** DESIGN (2026-10-16). Plan of record: `specs/yang_faithful_cleanup.md`
Phase 2b. Ledger: N2 (OPEN), N58 (inc-4c fan repair). Paper cites are lines in
`refs/text/yang2025_hybrid_boolean.txt`.

## 1. The paper's operation

§4.4.1 (`:605-609`, right column `:546-571`), applied to every mesh patch an
intersection curve trims:

1. map each exact intersection point r onto both meshes and **set r_A = r_B = r**;
2. **CDT in the surface's parametric domain**, the intersection polylines and the
   patch boundary as constraints ("maps boundary curves to boundary curves, and
   contains no flipping triangles since the intersection curves are regular");
3. **remove a mesh vertex too close to the intersection curve**;
4. **insert a point** in an intersection loop that has none;
5. **recalculate d(T)** for the new boundary triangles.

Fig. 11 adds the pre-CDT edits: (a) split the mesh edge that contains q at q;
(b) if the split edge's endpoint p is too close to q, merge p with q; (c) insert.

The paper therefore never produces a fold to repair: the triangulation is
REBUILT around the exact curve, not dragged onto it.

## 2. What the live code does (survey 2026-10-16)

Stage 4 (`stage4_correct.rs`, `stage4_relocate_and_correct_inner`, ≈ 5.4k lines)
does step 1 by moving arrangement vertices in place (relocation arms E/F, the
triple block C), then mends what in-place motion breaks, in this order:
§4.5.1 repair (G), KV9 junction dedup (H), the minted block — N47 weld, the
beyond-corner trim, `repair_fan_cluster` (I, N58) — the §4.5.3 sweep (J),
the §4.4.1(b) merge (K, N55), the N47 weld (L), the §4.4.1(a) "unzip" edge
split (M), the degenerate-triangle STOP (N), doubled-membrane removal (O), the
pinch split (P), sub-TAU_WORK collapse (R), the watertight gate (S), rim
relocation (T), the corner transit (U), and two domain postconditions (V).
After Stage 4, `reconstruct_topology_stage4` alternates the construct pass
(seam-chain replacement + **`rebuild_patch_planar`**) with the fold-merge pass
(fold merge, run / group absorb, rehome — each a local fan re-CDT), then
KV15b and a second sub-TAU_WORK collapse.

Step 2 already exists as a primitive: `stage4_construct::rebuild_patch_planar`
re-CDTs one whole patch against its cycles in a `SurfaceChart` (plane,
cylinder, cone — no sphere chart; torus only for local windows) with
`cherchi_rs::cdt_with_interior_constraints`, drops planar interiors (step 3's
redundant case), carries cylinder / cone interiors off the boundary, seeds lifted
chart points, and accepts only if the new max d(T) does not exceed the replaced
triangles' (step 5). It runs only on patches whose cycles the construct pass
modified, AFTER the in-Stage-4 repairs and the degenerate STOP (N) have run.

## 3. The design

**One operation, `update_patch`, applied to EVERY patch an intersection curve
bounds, immediately after the §4.3 dedups (K, L) and before any repair or
validation step (M onward).** It is `rebuild_patch_planar` generalized:

- **Population.** Every Phase-A patch with ≥ 1 cycle edge carrying a Stage-3
  curve or a both-input incidence (an intersection edge). Untouched patches keep
  their Stage-1 triangles — the paper trims only intersected meshes.
- **Constraints.** The patch's cycles, unchanged: boundary vertices are shared
  with the neighbouring patch across every edge, so both sides stay
  index-identical and watertightness is inherited (§4.4.3), never re-derived.
- **Near-curve removal (step 3).** An interior vertex whose chart distance to
  the patch's intersection polylines is below the patch's own d_ε chord band
  is dropped. (The paper gives no number; d_ε is the band within which the
  mesh already represents the curve. Ledgered as the one design choice.)
- **Empty loop (step 4).** A CDT of a polygon with holes needs no interior
  point; step 4 is subsumed by step 5's d(T) seeding.
- **d(T) (step 5).** The live budget rule: the new max d(T) ≤ the replaced
  triangles' max, with seeded retries. A patch whose budget cannot be met
  keeps its triangles (recorded, not silent — a census line).
- **Failure is a §4.5.2 site, never a STOP.** A CDT refusal (crossing
  constraints — the cycles are not a simple polygon in the chart) is the
  §4.5.4 illegal-output condition: it records a failure site naming the
  patch's face and the run continues, so the refinement loop re-discretizes
  that face and its one-ring (`boolean::failures`, T3c's family).
- **Unsupported chart** (sphere; a whole torus patch; a cone patch through its
  apex): the patch keeps relocate-in-place — a NAMED gap, listed per case in
  the census, closed by Phase 3's per-face charts.

## 4. Increments (each one corpus run; acceptance = REMOVAL with the corpus held)

0. **Census (probe only, `YANG_441_UPDATE_CENSUS`).** For every intersected
   patch: chart supported?, CDT ok / refused, d(T) budget met / not, and which
   later repair passes then touch that patch's triangles. Prints only. This
   names the population before any flow changes.
1. **`update_patch` wired after K/L under one series switch**, corpus run;
   every move probed. Byte-identity is NOT expected — every intersected patch is
   re-triangulated; acceptance is no CORRECT → non-CORRECT move, with each new
   ERROR anchored to a named gap or fixed in the paper's form before the flip.
2. **Retire the repairs it subsumes, one per corpus run**, the Phase-1 recipe:
   pass OFF → corpus → each protected case recovered through `update_patch` or
   the loop, or the operation fixed → delete. Expected order (cheapest proof
   first): the §4.4.1(a) unzip (M) and the degenerate STOP's population (N),
   `repair_fan_cluster` (I-iii, N58), the post-Stage-4 fold-merge / absorb /
   rehome family, the construct pass's own seam replacement (subsumed when its
   patches are already rebuilt), doubled-membrane removal (O). Each retirement
   amends its ledger entry.
3. **Split `stage4_relocate_and_correct_inner` along the paper's sub-steps** as
   the passes go: relocation (step 1), §4.3 dedup, §4.5.3, `update_patch`,
   validation.

## 5. Risks recorded before code

- The construct pass's history (spec `yang_441_trim_cdt_construction.md`
  I1–I13) shows whole-patch rebuilds refusing on θ-unwrap, apex, and d(T)
  degradation; the census (increment 0) measures those refusal rates on the
  corpus before any wiring.
- N55 measured in 2026-07 that a re-CDT path "greens nothing": it was aimed at
  ERROR cases. This phase's metric is removal, not conversion.
- Cost: every intersected patch re-CDTs once per Stage-4 run, and the §4.5.2
  loop re-runs Stage 4. The census records per-case time.

## 6. Increment 0 measured (2026-10-16)

`stage4_construct::update_census` (print-only, `YANG_441_UPDATE_CENSUS=<file>`,
called after the N47 weld, before the unzip). Corpus run with the probe set:
**358C / 0W / 11E / 5EE — verdict-identical**; census cost 7.7 s CPU over the
whole corpus (R0044 2.2 s). 559 Stage-4 runs in 179 cases; 17,693 intersected
patch evaluations:

| outcome of `rebuild_patch_planar` at the site | patches | cases (C / E) |
|---|---|---|
| ok | 14,962 | 158 (153 / 5) |
| ok, patch carries degenerate triangles now (the rebuild removes them) | 182 | 48 (46 / 2) |
| `ChordDegradation` (new max d(T) > the replaced triangles' max) | 1,711 | 31 (31 / 0) |
| `ThetaUnwrap` (a cylinder band encircling the axis) | 396 | 105 (104 / 1) |
| CDT `TriangulationFailed` (cycles cross in the chart) | 226 | 36 (34 / 2) |
| CDT `DegenerateInput` | 84 | 38 (38 / 0) |
| unsupported chart: torus / sphere | 81 / 2 | 20 / 2 |
| CDT `DuplicateVertex` | 4 | 3 |
| (`+degen` variants of the refusals) | 31 | — |

57 cases carry degenerate relocated triangles at the site (the unzip / STOP
population). Reading: the operation already serves 85 % of intersected
patches as-is; the refusals sit almost entirely on CORRECT cases — the states
the later repairs mend. The worklist, largest first:

1. **`ThetaUnwrap` (105 cases): a periodic cylinder chart.** A band that wraps
   the axis needs a seam cut (two copies of one generator, the Stage-1
   universal-cover idea, `yang_stage1_curved_holed_patch.md` Slice I) — the
   paper's parametric domain of a closed surface is periodic.
2. **`ChordDegradation` (31 cases): the budget.** The live rule (new max ≤ old
   max) is stricter than the paper's ("recalculate d(T) to maintain
   controllable error" — the bound is the patch's d_ε, §4.1). Anchor one case:
   does seeding to d_ε pass?
3. **CDT `TriangulationFailed` / `DegenerateInput` (36 + 38 cases):** anchor
   each class on its smallest case — a cycle crossing in the chart before
   the repairs is either a relocation overshoot (§4.5.3 / §4.5.1 territory,
   a site for the loop) or a chart artifact (a near-seam θ branch, a cone
   near its apex).
4. Torus / sphere: Phase-3 charts.

## 7. Increment 1 LANDED (2026-10-16)

`stage4_construct::update_patches`, called right after the census at the
planned site: every intersected patch with a supported chart and ONE
attribution is planned with `rebuild_patch_planar` and written back in one
batch (`apply_rebuild_batch`); a refused plan keeps its triangles.

First measurement (series switch on): **F0064 and F0067 CORRECT → ERROR**,
two error-text drifts (P0056, P0061), nothing else. Anchor (F0064,
`NONMANIFOLD_SITE_PROBE`): edge (1208, 1408) used twice in ONE direction by
B-faces 1 and 4 after the update, 10 unpaired edges along their shared line —
a rebuilt patch's boundary no longer matched its neighbour's. The paper's
update "maps boundary curves to boundary curves"; that is now the
postcondition — `directed_boundary(new) == directed_boundary(old)` (directed
edges with multiplicity), else the plan is refused (census class
`boundary_changed`). With it F0064 (88.4 s) and F0067 (324.6 s, canonical
356.3 s) are CORRECT again.

Corpus with the postcondition: **358C / 0W / 11E / 5EE / 0T, zero moves**
(wall 1079.3 s vs 1111.2 s); 14,983 patches rebuilt over 499 Stage-4 runs;
drifts: P0056 `NonManifoldVertex` → kernel-v2 `bounded cylinder patch must have
exactly one material-CCW loop` (FaceId 32), P0061 the same CDT ring text on
FaceId 30 (was 34) — both open-tail ERRORs. The switch is deleted: the update
is always on.

Next (increment 2): retire the repairs one per corpus run — first the
§4.4.1(a) unzip (M) and the degenerate STOP's population, then
`repair_fan_cluster`; and widen the operation's reach (periodic cylinder
chart, the d_ε budget) so fewer patches fall back to relocate-in-place.

## 8. Increment 2 measurements (2026-10-16) — repairs OFF, and the band chart PARKED

**Repairs OFF (§4.4.1(a) unzip + doubled-membrane removal, one run):** four
CORRECT → ERROR, all `DegenerateTriangle` (the unzip's population) —
F0067, P0005, R0059, R0072. Census on them: the degenerate triangles sit on
patches the update REFUSED — R0059 / R0072 `ThetaUnwrap` (cylinder bands),
P0005 `ChordDegradation` and CDT `TriangulationFailed`. The unzip stays
until the update reaches those classes; the membrane pass was not separated
(no case attributed to it).

**Band chart (§6 item 1), built and PARKED on branch `phase2b-band-chart`:**
`stage4_project::band_patch_from_cycles` cuts a cylinder / cone band (two
encircling cycles of opposite winding + holes) open along the shortest seam
clear of every cycle edge on every 2π copy; `rebuild_patch_planar` takes it
when `unwrap_theta` refuses, choosing each interior vertex's branch by
point-in-polygon and reading old-triangle d(T) per triangle on one branch.
Unit pins pass (`a_cylinder_band_cuts_open_into_one_simple_polygon`). Corpus:
`ThetaUnwrap` 396 → 7 patches, R0059 / R0072's bands rebuilt (degenerates
gone) — but **F0064 and F0067 CORRECT → ERROR at kernel-v2**: "an undirected
output edge is not used by (exactly two | two OPPOSITE) directed edges".
Anchor (F0067, `KV2_OUT_TWIN_PROBE` + a temporary loop dump): output faces
294 (the rebuilt band, B face 2, cylinder r 0.128, z 0.879…1.079, two 36-vertex
rims, 72 → 72 triangles — a pure strip) and 282 both walk rim arc 543 → 545;
same for 668 / 647 on the r 0.209 band (2,310 → 2,310 triangles). The yang
MESH pairs at every Stage-4 checkpoint (`NONMANIFOLD_SITE_PROBE` silent), and
the plan postcondition holds (no repeated vertex, no doubled directed edge,
directed boundary unchanged). So a pass AFTER the update whose result depends
on the band's interior triangulation (§4.3.4 seam insert / reorder in the
construct pass — the band's emitted cycle grows 72 → 1,538 vertices —
chain decimation, the envelope prepass, or Stage-6 cycle extraction) emits
two faces' loops inconsistently. Next session: bisect that pass on F0067
with the branch's temporary probes (`TMP_VERTS` in `emit_topology`,
`TMP_LOOP_DUMP` in kernel-v2's `from_yang`), fix it there, then re-land the
band chart.

**Kept on main:** the plan postcondition strengthened to a full local
validity check (`plan_keeps_boundary`: no triangle repeats a mesh vertex — a
cycle pinched at one vertex appears twice in the chart pool — no directed
edge twice, directed boundary unchanged) and the census's
`mixed_attribution` class.

## 9. The band chart LANDED (2026-10-16, same day)

The F0064 / F0067 regressions anchored to two latent defects the band chart
EXPOSED, both in code that predates it:

1. **`rebuild_patch_planar` oriented its result by the area vector**
   (`dot(area_vector(old), area_vector(new))`). A closed band's area vector
   sums to ~0 around the axis — the sign is rounding noise — and the F0067
   strip band came out flipped. A band is now oriented by its BOUNDARY: a
   directed boundary edge of the replaced triangles must be traversed the same
   way by the new ones. Pin `a_closed_band_rebuilds_with_its_boundary_unchanged`.
2. **The construct pass applied a seam batch that changed its OUTER
   boundary** (F0064: a planar cap rebuilt to a 3-cycle while its cylinder
   neighbour kept the rim samples between — a crack). A seam edit changes both
   patches' boundaries and the changes must cancel inside the batch; the batch
   now must keep its combined directed boundary, or it is not applied (the
   pass stops as when no seam remains). Previously unreachable: band patches
   refused, so their seams were never edited.

Anchoring method (F0067): kernel-v2's `KV2_OUT_TWIN_PROBE` names the edge by
coordinate; a temporary coordinate probe in `emit_topology` showed planar
patch 282 and band patch 294 both traversing mesh edge 543 → 1015 — a doubled
directed MESH edge minted after Stage 4 (1015 a §4.3.4 seam insert), i.e. the
construct pass. Kernel-v2 vertex ids are re-indexed at import; never match
them to yang mesh ids.

Corpus (band chart + both postconditions): **358C / 0W / 11E / 5EE / 0T, no
move, no drift** (wall 1065.6 s); rebuilt patches 14,797 → 15,141;
`ThetaUnwrap` refusals 392 → 7.

3. **The d(T) budget was the old triangles' max, not the paper's d_ε.** The
   yang-rs suites then failed three pins the corpus did not (backend parity
   on `cylinder_union_box`, the yr13 cavity-wall winding, and
   `corner_in_band_refines_membership_no_phantom_junction` at volume 28.17
   against 30–31): a relocated band certified d(T) = 0.86 on a unit cylinder,
   so "no worse than the old" admitted a rebuild of secant triangles that
   shaved 7 % of the volume, and the old-max budget is
   triangulation-dependent (the parity backends triangulate differently).
   `rebuild_patch_planar_bounded` takes the budget as `min(old max, d_ε)`
   (`stage4_chord_band(a, b)`), Yang §4.4.1's "recalculate d(T) to maintain
   controllable error" — never looser than before, stricter exactly where the
   old triangulation exceeded the paper's bound. The update, its census and
   the construct pass use it; the unit pins keep the unbounded form.

Corpus with all three: **358C / 0W / 11E / 5EE / 0T, no move** (wall
1062.1 s; P0056 / P0061 face-id drift only); rebuilt patches 14,577;
`ChordDegradation` 1,741 → 2,312 (the d_ε bound refusing what the old-max
rule had accepted — those patches keep relocate-in-place); `ThetaUnwrap` 7.

## 10. The §4.4.1(a) unzip RETIRED (F2 Step 2, 2026-10-30/31)

Antigravity's Step 2 moved the Fig. 11(a) split into `update_patches_ex`
(split chains collected from relocated collapsed triangles, split patches
re-triangulated by the patch CDT, coupled components committed atomically)
and deleted the post-hoc `(3d)` unzip. An independent review found its
fallback, `local_fig11a_patch_plans`, was the unzip's arms moved inside the
update: full-corpus census, 166 split seeds in 24 cases; 7 patches resolved
by CDT, 24 by the arms. The arms' refusal reasons on those 24: 11
`ChordDegradation` (this spec's §9 item 3 budget), 6
plan-keeps-moved-degenerate (F0067), 5 planar CDT errors, 2 `ChordCertify`.

**Arms narrowed to the N106 class.** A first arms-off run held 360C — but it
still carried the d(T) refinement trial below, which rebuilt the
`ChordDegradation` patches. With the arms off and no trial, R0059 and P0005
go CORRECT → ERROR (`DegenerateTriangle`; the §4.5.2 loop does not clear
them) — §8's two `ChordDegradation`-class cases again. So the arms run only
for a patch refused with `ChordDegradation`; every other declined component
reaches the STOP (F0067, P0040, R0051 do, and end CORRECT). The arms retire
with N106.

**Fixed on the way:** `patch_from_cycles_shifted` took `cycles[0]` as the
outer loop; a patch whose hole cycle came first was charted inside-out and
the CDT returned nothing (`DegenerateInput`, F0082). The outer loop is now
the largest-|area| loop.

**Found, not landed — the d(T) refusal is not the paper's (ledger N106).**
§4.1.2 (`:357-378`) makes a boundary triangle's d(T) its own error bound; the
paper never refuses a triangulation for it. Trial (refine to the budget by
centroid insertion) needed Delaunay in the isometric chart (r·θ, y) — the
(θ, y) chart's Delaunay builds circumferential slivers that Fig. 6's
covering rectangle charges at full width — and a floor at the patch's own
boundary chords (a long diagonal cycle chord certifies 2× d_ε alone). It
converted the 11 `ChordDegradation` patches and held the corpus but broke
four suite pins, each because refused patches were rebuilt with seed
vertices. Next: carry per-triangle d(T) as the bound (N106 remediation), and
re-measure §9 item 3's volume regressions with the isometric chart.

**Open refusal:** a split that lands on a vertex already on the same cycle
(P0040: vertex 73 inserted into edge (53, 69) of a cycle that also visits 73)
makes the trimmed region two lobes touching at a point; the patch CDT takes
one simple outer loop (`DuplicateVertex`).
