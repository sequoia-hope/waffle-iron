# Yang Compliance Endgame — Failure-Tail Triage Ledger

**Created 2026-07-16 (Phase 1 of the compliance endgame plan — see
`docs/yang_functional_roadmap.md` §0.0).** One row per non-CORRECT assay case
at baseline **240C / 0W** (commit 6d6141ef). Every row names a root cause and a
fix vehicle; rows marked **PROBE** have an unconfirmed root cause and form the
probe work-queue — no machinery may be built against a PROBE row until its
diagnosis is confirmed (the machinery-first detours of #168/#169 Phase B are
the cautionary tale).

**Maintenance:** when a case flips CORRECT, strike its row (do not delete).
When a probe confirms a diagnosis, replace PROBE with CONFIRMED + source.
Re-generate the baseline column only from a committed `results.json`.

## Vehicles (see roadmap §0.0 for ordering rationale)

| Vehicle | What it is |
|---|---|
| **P2-M5** | Degree-4 SSI solvers in `ssi-rs` (torus×torus, cyl×cyl lateral) |
| **P3a-#146** | Conformal junction sampling at Stage 1 (RE-SCOPED 2026-07-17 per `docs/yang_junction_research_findings.md` Q4: the near-dup mint is Stage-1 independent sampling near shared junctions; Stage 2/3 arrangement exonerated) |
| **P3b-#137** | Grazing-corner exact junction insert + stitch + local §4.5.2 refinement |
| **P3c** | Curved-seam re-CDT (real micro-scale features; R0072 class) |
| **M8** | Stage-0 coplanar residue (task #130) + rim-projection class (task #144) |
| **#153** | Off-plane planar-face emission wall (3e-8 @ 2m NonPlanarFace) |
| **KV6/scope** | Revolve & profile capability tail |
| **PROBE** | Diagnosis unconfirmed — goes to the probe queue first |

## The ledger (54 actionable cases; F0074 is EXPECTED_ERROR by design)

### LocalRefinementRequired (18)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| R0044 | Stage-4 LRR ~~v11~~ v13 | ~~torus×torus (N52)~~ ~~**RE-DIAGNOSED (#172):** the surface-pair endpoint-mix STOP~~ **ENDPOINT-MIX LAYER RESOLVED 2026-07-28 (triple-block wiring):** the mix vertices (v8, v12) have exactly 3 incident surfaces `{cyl_A, plane_B, cone_B}` and relocate through the increment-5 triple block. R0044 now STOPs one layer deeper at `stage4_correct.rs:5646` — v13 is a **pure** surface-pair vertex (`n_maps == 1`, correctly not a triple) whose `relocate_onto_implicit_pair` NEWTON DIVERGES. Same family as the torus `pair_newton_none` cases | CONFIRMED (2026-07-28 `#[track_caller]` LRR-site trace) | M5 surface-pair Newton convergence (with R0025/R0032/R0077) |
| ~~R0096~~ | ~~Stage-4 LRR v7~~ | ~~torus×torus~~ **FLIPPED CORRECT 2026-07-17 (#172):** torus×torus lateral∩lateral + torus×torus×plane junctions now relocate via the implicit-pair/triple Newton (torus-block scope lift) | — | ~~P2-M5~~ DONE |
| R0038 | Stage-4 LRR (u32::MAX) | plane tangent to cylinder along one generator; degree-2 gate self-validates (`bad_degree=[(18,4),(19,4)]`) — near-tangency pinch, NOT a CDT ring | CONFIRMED (#168 WIP4, 9f4cb604) | P3b-#137 |
| ~~R0072~~ | ~~Stage-4 LRR (u32::MAX)~~ | ~~real ~1e-7 micro-scale edge (0.4% span); force-merge is the R0091 silent-wrong trap — needs curved re-CDT~~ **FLIPPED CORRECT 2026-07-28 (#195 inc-5):** the §4.5.4 detect-then-refine rim boost + §4.4.1 rim-snap, both now always-on, resolve it WITHOUT a curved re-CDT — the micro-scale edge was an under-sampled rim, not an irreducible feature | — | ~~P3c~~ DONE |
| C0058 | non-2-manifold (reassembly) | probe 2026-07-17: `NONMANIFOLD_SITE s6-curved-degenerate-loop` — Stage-6 curved face 2 emits a 64-vertex loop with \|Newell N\| = 2.3e-16 (degenerate junction loop) | CONFIRMED (#171 sweep) | P3a-#146 |
| C0067 | Stage-4 LRR v128 | probe 2026-07-18 (#171 pass 2): v128 is a **circle×circle junction** (`circle_junction=true`, endpoint) — two sphere-section Circles (both r=0.371, centers [0.15,0,0.5]/[0,0.15,0.5], normals x̂/ŷ) meet at [0.15,0.15,0.83]; junction relocation region invalid. Needs two-curve junction relocation (mint-once contract) | CONFIRMED (#171 pass 2) | P3-junction |
| ~~R0008~~ | ~~Stage-4 LRR v42~~ | ~~probe 2026-07-18: `YANG_LRR_SITE site=lineseg_combo` edge (42,43) — LineSegment edge whose incidence is **Cone(A, half-angle 1.5525 rad ≈ 88.9°, near-flat) × Plane(B)**; the Stage-4 LineSegment arm has closed forms only for cyl×plane / cyl∥cyl / plane×plane — the **cone-generator line closed form is missing**~~ **FLIPPED CORRECT 2026-07-28 (cone-generator arm):** the closed form was never missing — `ssi_rs::plane_cone` has emitted `SsiCurve::Line` for through-apex cuts all along and Stage 3 already banded them via `cone_chord_tol_for_owner`. TWO wiring gaps, both in Stage 4: (a) the LineSegment pair match classified `Cone` as `other_curved` → STOP before selection; (b) once admitted, the tie-break called the R0072-only `select_disjoint_parallel_line`, whose parallelism precheck rejects the two CROSSING apex generators (`AmbiguousCurve{2,2}`). **#163/N45 was not a "residual theory" — it was CORRECT and already shipped, at Stage 3 only**; the two stages had been running different tie-breaks since 9fca8393 | — | ~~Stage-4 cone-generator LineSegment arm~~ DONE |
| R0009 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — the chord-split loop exhausts its pass budget (§4.5.2 refinement demand, non-convergent) | CONFIRMED (#171 sweep) | P3-§4.5.2 |
| R0020 | ~~Stage-4 LRR v44~~ TessellationFailed FaceId(21) | ~~probe 2026-07-18: v44 is the surface-pair endpoint-mix STOP — the R0044 class exactly~~ **ENDPOINT-MIX LAYER RESOLVED 2026-07-28:** v44's incidence is exactly 3 (`{plane_A, cone_A, cyl_B}`) and relocates through the triple block. Two deeper layers now: a pure surface-pair Newton divergence at `:5646` (R0044's new class), and the fatal one — kernel-v2 **`surface-pair refinement needs a positive finite chord tolerance`**, i.e. the OUTPUT B-Rep now carries a `Curve::SurfacePair` edge that kernel-v2's render tessellation cannot band | CONFIRMED (2026-07-28) | kernel-v2 surface-pair render band + M5 pair-Newton |
| R0025 | Stage-4 LRR v1760 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — torus×plane implicit-pair **Newton non-convergence** at v1760 (torus R=494/r=329, scale ~1300; siblings on the same op relocate fine). #131/N28 rim-crossing theory refuted | CONFIRMED (#171 pass 2) | P3b-#137 (torus∩plane relocation family) |
| R0032 | Stage-4 LRR v32 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — **torus×Cone** implicit-pair Newton non-convergence (torus R=45.6/r=30.4 × cone half-angle 1.19 rad); sibling verts with cone partners relocate fine — v32's specific pair diverges | CONFIRMED (#171 pass 2) | P3b/M5-residual (torus×cone pair Newton) |
| ~~R0035~~ | ~~Stage-4 LRR v194~~ | ~~v194 is `ellipse=true + surface_pair=true + endpoint` — Ellipse endpoint also on `SurfacePair{Cylinder×Cylinder}` → surface-pair endpoint-mix STOP, R0044 class~~ **FLIPPED CORRECT 2026-07-28 (triple-block wiring):** v194/v195 have exactly 3 incident surfaces `{cyl_A, cyl_B, plane_B}` — the increment-5 conic triple junction, which had simply never counted `vert_surface_pair` as a curve-bearing map | — | ~~P3-junction~~ DONE |
| R0047 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — same class as R0009 | CONFIRMED (#171 sweep) | P3-§4.5.2 |
| R0049 | ~~non-2-manifold (reassembly)~~ ring rejected by CDT (FaceId 575) | ~~probe 2026-07-17: `s6-planar-loop-nonplanar` face 134 vert 337 off-plane 1.449e-6 (band 1.0e-7) — the F0064 class (N51)~~ **DRIFTED 2026-07-29:** now fails as a ring-reject on a **developable** patch (FaceId 575, `tessellate_developable_patch` — not planar). 214 origin nodes, 0 arc samples, folds at idx 1/45/46 (144.2°, 180.0°, 176.6°). **NOT counted as seam-class:** the ring breaks into **~97 adjacency runs**, so ~45% of ring indices are seams and "fold near seam" carries no information. The **fragmentation itself** is the signal — a boundary shattered into ~97 micro-chains against different neighbour faces, which reads as the near-coincident-surface incidence family (R0050/R0053 kin) and is consistent with the old `s6-planar-loop-nonplanar` diagnosis. **CAVEAT: the run-splitting heuristic (twin-id delta > 12 or sign change) is crude and may over-fragment on irregular id allocation — verify the 97 before building on it** | PARTIAL (builder + fragmentation measured 2026-07-29; mint unconfirmed) | Stage-2/3 incidence (near-coincident surfaces) — was P3a-#146 |
| R0050 | Stage-4 LRR v58 | probe 2026-07-18: `YANG_TORUS_STOP site=gt2_partners` with **partners=[] (EMPTY)** — v58 (and v362 on the sibling torus) sit on torus intersection edges whose incidence records only ONE distinct surface (the base torus itself); the model has two near-identical revolve tori (R=3.95/r=2.63 vs R=3.78/r=2.52) — a Stage-2/3 **incidence gap between near-coincident revolve surfaces** (no partner to relocate onto). #131/N28 theory refuted | CONFIRMED (#171 pass 2) | P3a-#146 / Stage-2/3 incidence (near-coincident surfaces) |
| R0063 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — same class as R0009 (the #145 zigzag residual resolves into the split-budget class) | CONFIRMED (#171 sweep) | P3-§4.5.2 |
| R0077 | Stage-4 LRR v3 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — torus×plane implicit-pair Newton non-convergence at extreme scale (torus R=2051/r=1367, coords ~2700; the op's other two torus verts converge with rho ≈ 2e-13). Same class as R0025 | CONFIRMED (#171 pass 2) | P3b-#137 (torus∩plane relocation family) |
| R0091 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — same class as R0009; STILL the historical silent-wrong trap: any fix must be re-CDT/refinement, never a merge | CONFIRMED (#171 sweep) | P3-§4.5.2 |

### OffCurveBeyondChordBand (6)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0065 | Stage-4 OffCurve v8 | torus∩plane grazing loop reaches \|y\|=0.384 outside the box face; needs exact triple-junction corner insert + stitch (primitive proven, N-137.1) | CONFIRMED (#137 spec) | P3b-#137 |
| R0074 | ~~Stage-4 OffCurve v89~~ ring rejected by CDT (FaceId 593) | ~~torus∩plane grazing — same class as C0065~~ **DRIFTED + RE-DIAGNOSED 2026-07-29 (`KV2_RING_PROVENANCE`, 70ccf32c): this is no longer a #137 grazing case.** The OffCurve layer is gone; R0074 now fails as a ring-reject and is the **cleanest witness of the planar seam-overlap class**. PLANAR builder, 541 half-edges, **all LineSegment, ZERO interior samples** (sampler exonerated). 7 adjacency runs; all three crossings (111×113/114/115) sit on the run-B→run-C seam at idx 114, with folds of 179.90° / 156.70° / 177.15° against a ring median of 2.86°. The four fold points project onto the v111→v116 chord at t = 0.588, 0.590, 0.471, 0.263 — monotone **DESCENDING** where traversal demands ascending — and v112/v113 are **9.1e-6 apart (near-dup pair)** at the seam. **Control: the ring's OTHER seam (idx 58) turns a genuine 86.6°/80.9° corner and is clean** ⇒ seam does not imply fold; overlapping chain RANGES do. This is the "mint once exactly, share by identity" contract (`docs/yang_junction_research_findings.md`) violated in Stage-5/6 **OUTPUT** assembly, not the Stage-1 input sampling #146 chases | CONFIRMED (2026-07-29; mechanism settled by the positional oracle — 67/78 folds straddle the moved/still boundary, 329 of 2731 verts moved) | **Stage-4 partial relocation of a boundary chain** (with R0011, F0045). NOTE: the conic `relocations` oracle is BLIND here (torus arm records no `t` retag) — an earlier pass wrongly read `n_relocations=0` as "nothing moved" and re-vehicled this row to #146; RETRACTED |
| R0003 | Stage-4 OffCurve v4233 | multi-map over-band chain (v4233→v8508); needs ellipse×hyperbola junction handling, band-fixing exhausted (N45/N46) | CONFIRMED (N51/N52) | P3-junction |
| R0015 | Stage-4 OffCurve v84 | probe 2026-07-18: N51 "no-curve-type" REFUTED — v84 IS in the torus map (`torus=true`); `YANG_TORUS_PROBE` shows the pair Newton relocates it EXACTLY (rho=0, F_torus(proj)=0) and it passes the displacement gate, so the STOP is the **bounded-face containment** check below the gate (`stage4_correct.rs:4225`) — the C0065 grazing-loop-outside-face signature, at MICRO scale (torus R=5.97e-5/r=3.98e-5, coords ~1e-4) | CONFIRMED (#171 pass 2) | P3b-#137 (C0065 containment class, micro-scale) |
| R0026 | Stage-4 OffCurve v218 | probe 2026-07-18: same as R0015 — v218 `torus=true`, pair Newton rho=9.65e-6 ≪ gate 3.0e-3, then bounded-face containment STOP; micro torus∩plane (R=0.0214/r=0.0143) | CONFIRMED (#171 pass 2) | P3b-#137 (C0065 containment class, micro-scale) |
| R0070 | Stage-4 OffCurve v1028 (+op2 LRR v47) | probe 2026-07-18: v1028 sits on a micro Ellipse edge (1025,1028; major_r 0.028) AND a LineSegment edge (1028,1029) — an ellipse∩line conic junction endpoint whose ellipse relocation lands beyond band at micro scale. ~~**op2 v47** is the surface-pair endpoint-mix STOP, R0044 class~~ **op2's endpoint-mix layer RESOLVED 2026-07-28 (triple-block wiring)** — R0070 raises no LRR at all now; the surviving failure is the v1028 OffCurve half only | CONFIRMED (#171 pass 2; op2 half closed 2026-07-28) | P3-junction (v1028 OffCurve half only) |

### Reassembly non-2-manifold (8) — the #146 junction-mint bucket

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0082 | non-2-manifold | near-duplicate junction verts v588≈v601 (0.012 apart 3D, ~4e-4 in-plane = off-plane); spurious in-patch overlap triangle; re-CDT REFUTED as tool | CONFIRMED (#169 Phase B, 0b655da2) | P3a-#146 |
| ~~R0095~~ | ~~non-2-manifold~~ | ~~EVERY face has a ~1e-24-area boundary triple — upstream degenerate junction geometry~~ **FLIPPED CORRECT 2026-07-28 (#195 inc-5):** the always-on rim boost + rim-snap remove the degenerate boundary triples at the source | — | ~~P3a-#146~~ DONE |
| C0044 | non-2-manifold | 3-patch junction fires the Stage-4 gate. **P3a increment-0 probe (2026-07-18): ZERO transversal pierce candidates — the junction is coplanar contact (flush annular stack), NOT the pierce-mint class** | CONFIRMED (#169 Phase 0 + #146 inc-0) | ~~P3a-#146~~ Stage-0/M8 coplanar-seam family |
| F0064 | non-2-manifold | wall vert 0.083 off floor plane; minted in Stage-4 mutation window OR inherited via lineage-less chained B (4 hypotheses eliminated, N51 session) | PARTIAL (#146) | P3a-#146 |
| R0051 | non-2-manifold | in the #146 Newell-normal class per task | SUSPECTED | P3a-#146 |
| F0058 | non-2-manifold | probe 2026-07-17: `s4-shell-euler` shell root 106 χ=3 (v107 e314 f210) — Stage-4 shell-level Euler defect | CONFIRMED (#171 sweep) | P3a-#146 |
| F0060 | non-2-manifold | probe 2026-07-17: `s4-shell-euler` shell root 118 χ=3 (v49 e150 f104) — same class as F0058 | CONFIRMED (#171 sweep) | P3a-#146 |
| F0085 | non-2-manifold | probe 2026-07-17: `s4-halfedge-pairing` edge (5720,5731) fwd=1 rev=0, verts 0.043 apart — the R0038-type unpaired open seam (two-sided conformality) | CONFIRMED (#171 sweep) | P3b-#137 |

### CDT / tessellation failures (8) — mostly chained-input casualties

~~Chained models feed a yang boolean OUTPUT back in as an operand; degenerate
junction verts in that output then poison CDT. Suspected downstream of P3a.~~
**REFRAMED by #171 pass 2 (2026-07-18):** the defective rings are boolean
OUTPUT face boundaries rejected by kernel-v2's render CDT — chaining is not
required (F0045 fails at its FIRST boolean). ~~The mint is the boolean's own
Stage-5/6 output-ring assembly; two signatures: sample-misorder
zigzags/folds and #146 near-dup spikes (see per-row evidence).~~

**RE-REFRAMED 2026-07-29 (`KV2_RING_PROVENANCE` sweep, 70ccf32c) — this was
never ONE bucket.** Two facts reorganize it:

1. **The universal symptom is a near-180° FOLD, not a "zigzag" or a "closure".**
   Every rejected ring in the bucket carries a vertex whose turn angle is
   137–180° against a ring median of 0.00–3.8° (a 50×–4500× outlier). The
   self-intersection is the CONSEQUENCE; the fold is the defect. The three
   pass-2 signatures ("mid-ring zigzag", "closure fold", "fine arc doubling
   back over a coarse return") are the same shape at different ring positions.
   The density framing was a red herring — at the crossing the two segments are
   usually comparable in length (1.0×–10.9×).
2. **There are TWO ring builders, and the bucket splits across them.** Planar
   faces go through `sampled_loop_points` (`tessellate/mod.rs`); curved laterals
   go through `tessellate_developable_patch` (`tessellate/developable.rs`) in an
   unrolled (u,v) cut frame. They share nothing but the CDT call.

| Case | Builder | Ring composition | Runs | Folds | At a seam? |
|---|---|---|---|---|---|
| R0074 | planar | 541 LineSegment, 0 samples | 7 | 113–115 | **yes** |
| R0011 | planar | 380 LineSeg + 12 EllipseArc, 0 samples | 9 | 28 | **yes** |
| F0045 | planar | 8 Arc + 5 LineSeg, 8 samples | 4 | 12, 16 | **yes** (seam+1) |
| R0028 | developable | 25 origin + 121 arc samples | 3 | 0 | **no** (13 away) |
| R0049 | developable | 214 origin, 0 samples | ~97 | 1, 45, 46 | inconclusive |

**The three PLANAR cases are one confirmed mechanism** — the output loop joins
two boundary chains whose parameter ranges OVERLAP, so the ring walks the same
stretch forward-back-forward. Crucially **seam does NOT imply fold**, which is
what makes it a mechanism rather than a correlation: R0074's other seam turns a
genuine 86.6°/80.9° corner and is clean, and R0011 has six clean seams including
two ~85° corners. The fix site is yang's Stage-5/6 output-loop assembly.

### The mechanism, NAMED (2026-07-29, `YANG_S5_FOLD_PROBE`)

The "seam-overlap" framing above is the SYMPTOM seen through kernel-v2's twin
ids. Probing yang's own emission (`stage5_topology.rs`, right after `push_loop`)
names the cause: **the fold is already present in the loop Stage 5 emits, and it
sits exactly on the boundary between the vertices Stage 4 MOVED and the vertices
it left in place.** Stage 4 relocates a SUBSET of a boundary chain onto the exact
analytic geometry; the un-relocated remainder stays at its Stage-1/2 mesh
position; where the two stretches meet, the polyline retraces.

| Case | Complete oracle | Folds | Straddle a moved/still boundary | All moved | All still |
|---|---|---|---|---|---|
| F0045 | conic `relocations` (`has_torus=false`) | 4 | **4** | 0 | 0 |
| R0011 | conic `relocations` (`has_torus=false`) | 10 | **10** | 0 | 0 |
| R0074 | positional diff (`collapsed=false`) | 78 | **67** | 11 | **0** |

**81 of 92 folds straddle a moved/un-moved boundary; NONE lie entirely in
un-moved geometry.** That last column is the discriminator — a fold inherited
from an untouched Stage-2/3 boundary cycle would land there, and none do.

**Oracle warning — read before re-probing this class.** The `relocations` vector
returned by `stage4_relocate_and_correct` carries **conic `(vertex, circle-frame
angle t)` retags ONLY**. The torus arm (`stage4_correct.rs`, `vert_torus`,
~line 5674) relocates via `relocate_onto_implicit_pair` and mutates `mesh.verts`
but **never pushes to it** — every push site is upstream at 4793–5552, and torus
edges are degree-4 polylines with no analytic curve and no `t` retag. So on any
torus model `n_relocations=0` means "no conic retags", NOT "nothing moved":
R0074 reports 0 relocations while having **moved 329 of 2731 vertices**. A probe
keyed on `relocations` is BLIND there. Use the positional diff (`S4_MOVED`).
Conversely the positional diff is unavailable when a §4.5.3 collapse renumbers
vertices (F0045 89→88, R0011 853→847), which is why the two oracles are
complementary and each case above is settled by exactly one of them.

Two claims from the first pass are RETRACTED: (a) that R0074 was a different
class (#146 near-dup) — that rested on the blind oracle and is refuted by the
67/78 straddle; (b) that relocated vertices are systematically low-index and
un-relocated high-index — true in F0045/R0011, REVERSED in R0074 (moved 1140s,
still 1050s). The index ordering is incidental.

Open residual: the **11 all-moved folds on R0074** are not explained by partial
relocation — either those vertices were moved to mutually inconsistent targets,
or a second mechanism overlaps. Not blocking; worth its own probe.

### CORRECTION — "partial relocation" is a MISNOMER (2026-07-29, same probe)

Anchoring the fix revealed the framing above is still one step off, and in a way
that changes the VEHICLE. Two measurements:

**1. The un-moved vertices are not relocation candidates at all.** Per-vertex
incidence at every fold: `MOVED` vertices carry `Plane+Torus`, `still` vertices
carry **`Plane` only**. They sit on the planar face's own rim, not on the A∩B
intersection curve. **Stage 4 was RIGHT not to move them.** There is no
enumeration bug, and "relocate the rest" would be actively wrong — it would drag
rim vertices onto a curve they are not on.

**2. The relocation displacement is O(local edge length) and frequently larger.**

| max displacement / shortest incident edge | value |
|---|---|
| median | 0.42 |
| p90 | 6.74 |
| **max** | **101.4×** |
| folds where displacement ≥ shortest incident edge | **25 of 78 (32%)** |

Clinching row: `verts=(126,124,117)`, vertex 126 displaced **7.404e-4** while the
edge joining it to its un-moved neighbour 124 is **5.213e-4** long. Moving an
endpoint further than the edge is long **cannot** preserve local ordering — the
fold is not a tolerance accident, it is arithmetically forced.

**THE ACTUAL DEFECT: Stage 4 relocates intersection-curve vertices by
displacements up to 101× the incident mesh edge length and does not update the
incident mesh.** Yang §4.4.1 requires the mesh to be updated when vertices move;
§4.5.2 requires local refinement of the affected neighbourhood. Neither runs.
The rim edge that used to bracket the chain now crosses it.

**VEHICLE: the §4.4.1/§4.5.2 mesh-updating epic** (#169 / deviation N2; specs
already exist — `specs/yang_n2_stage4_cdt_mesh_updating.md`,
`specs/yang_mesh_updating_epic.md`). This is NOT a new bucket and NOT a
self-contained quick fix; the three cases are customers of the epic that is
already the default kernel priority. The measurement above gives that epic a
concrete **acceptance criterion it did not have: drive
`max_displacement / shortest_incident_edge` below 1 at every relocated
boundary-chain vertex** (equivalently, refine the incident edges until the
relocation fits inside them). `YANG_S5_FOLD_PROBE` measures it directly.

Retracted with this correction: the vehicle row "Stage-4 partial relocation of a
boundary chain" (committed 0a3d56b8) — the relocation SET is correct; what is
missing is the mesh update that must accompany it.

### SCOPED — the epic owns 16 of R0074's 78 folds, not all of them (2026-07-29)

Verifying the anchor before building the machinery split this bucket again. Two of
the previous section's load-bearing statements do not survive direct measurement,
and the third is confirmed with a corrected denominator.

**Probe upgrades** (`stage5_topology.rs`, all still env-gated print-only behind
`YANG_S5_FOLD_PROBE`): incidence is now **operand-qualified** (`A:Plane`,
`B:Torus`); a separate per-vertex oracle records which incident edges are actually
**keys of `intersection_curves`** (the map Stage 4 relocates onto); `S4_MOVED`
carries the displacement **vector**, giving both a tangential/normal split and the
**pre-Stage-4** turn angle and spacing; and the maps are **re-keyed after a
§4.5.3 / KV15b collapse**, without which their columns name the wrong vertices.

**1. The kind-only incidence signature could not support its conclusion.**
`incidence` is built from EVERY patch boundary-cycle edge (`compute_phase_a`), so
an operand's OWN rim — A's plane patch meeting A's torus patch — carries the same
`{Plane, Torus}` kind signature as a cross-input A×B edge, while only the latter
is a relocation candidate (`build_intersection_curves` skips `input0 == input1`).
Operand-qualified, the conclusion **holds for R0074** (77/78 fold apexes are
`A:Plane+B:Torus`, cross-input, 0/78 own-rim) but **fails for F0045**, whose
apexes are 4/4 own-rim — see §"F0045 is a different class" below.

**2. "Straddles a moved/still boundary" does NOT mean Stage 4 minted the fold.**
The 81-of-92 straddle statistic is a correlation; the direct measurement is each
fold's turn angle re-evaluated at the pre-Stage-4 positions. For R0074 (the one
case whose positional oracle is live):

| | folds | median &#124;turn − turn_pre&#124; |
|---|---|---|
| **MINTED** by Stage 4 (turn_pre ≤ 120°) | **16** | **179.84°** |
| **INHERITED** (turn_pre > 120°) | **62** | 1.25° |

The minted folds flip 0.00° → 179.9x°. The inherited 62 were **already folds
before Stage 4 ran** and it barely perturbed them (max 4.12°) — they come from the
Stage-2/3 boundary cycle and are NOT this epic's defect. All 78 have ≥1 moved
vertex, so the straddle test cannot separate them; only the pre/post turn can.
(Valid because a non-collapsing Stage 4 moves positions without changing topology,
so the same cycle adjacency exists before it.)

**3. The acceptance metric is confirmed, with the PRE-relocation spacing as the
denominator.** The previous section's "shortest incident edge" is measured after
the move; what the relocation actually had to respect is how far apart the
vertices were beforehand. That ratio separates the two populations cleanly:

| max displacement / min PRE spacing | MINTED (16) | INHERITED (62) |
|---|---|---|
| median | **3.85** | 0.22 |
| p90 | 14.16 | 0.94 |
| max | **81.35** | 3.55 |
| folds with ratio > 1 | **14 / 16** | 6 / 62 |

So `ratio < 1` is the right criterion — it is violated by 88% of the folds Stage 4
minted and respected by 90% of the ones it inherited. The clinching row is
sharper than the previous section's: pre-spacing **9.101e-6** against a
displacement of **7.404e-4** — a ratio of **81×**, and that pair is the known
near-duplicate. Its fold triple is 3 collinear vertices (turn_pre = 0.00°) all
relocated ~97% NORMAL to their chain; what inverts their order is not the
direction of the move but that its size dwarfs their separation. Two vertices
9.1e-6 apart cannot be independently projected 3e-4 onto the same curve and keep
their order. This is what Yang Fig-11 `merge` exists for (fuse a vertex within
`merge_tol` of a curve point instead of moving both) — the primitive is built and
unit-tested in `stage4_update::stage4_mesh_update`, still unwired.

**Consequence for scope: R0074 will NOT green from mesh-updating alone.** The epic
owns its 16 minted folds; the other 62 route upstream to the Stage-2/3 near-dup
boundary-cycle class (#146). Promise no conversion for this case.

**F0045 is a different class — the Fig-11 q, with a CYLINDER third surface.** Its
4 fold apexes are all own-rim (≥2 surfaces of one operand) and 3 of 4 also span
both operands: `A:Cylinder+A:Plane+B:Cylinder`. That is exactly Fig-11's point q
("an intersection point ON the boundary curve") — the F0083/v80 class. What is
measured is the incidence SIGNATURE (definitional for q); that F0045's q is
MIS-seated is not yet measured, since its collapse blocks the positional oracle.
Confirm it first with the apex's per-surface implicit residual (a static property of
the final position — no pre/post needed; this is what named F0083's defect). The
inc-3 machinery for it already exists (`plan_triple_point_reseats` +
`satisfies_all_surfaces`, gated `YANG_S4_TRIPLE_POINT_ENABLE`) but **skips these
vertices by construction**: it requires the other operand's surface to be a
`Plane` (`stage4_boundary_curve.rs:410`) because its closed form is circle∩plane.
The capability step is a rim-circle ∩ **cylinder** seat — for which
`relocate_onto_implicit_triple` (the ≥3-surface Newton) and the
`satisfies_all_surfaces` certificate both already exist.

**R0011 is a third signature:** 0/10 apexes own-rim, 6/10 `A:Cylinder+B:Plane` on
an `Ellipse` curve, 4/10 `B:Plane`-only with no curve.

**Blocking gap for two of the three cases.** F0045 and R0011 both take a §4.5.3
collapse (89→88, 853→847 verts), so the positional oracle is unavailable and
neither minting nor displacement can be measured for them at all — the probe now
reports `turn_pre=NaN` rather than a number that would silently equal `turn` and
read as "inherited". Making `S4_MOVED` survive a collapse (compose the
`compact_unreferenced_verts` remap) is the enabling increment for both.

⇒ **This bucket is three classes, not one.** Only R0074's 16 minted folds are a
direct §4.4.1/§4.5.2 customer. Do not build one machinery against all three.

### MEASURED — all three cases, collapse-aware oracle (2026-07-29, second pass)

The blocking gap above is closed: `S4_PRE_POS` now stores each vertex's
**pre-Stage-4 position** (not a displacement) and is re-keyed through **all four**
`compact_unreferenced_verts` sites (§4.5.3, KV15b, #194 sub-TAU collapse, N50 f32
weld — `YANG_S5_REMAP` reports each). Storing the position rather than the
displacement is what makes it survive: `pre = post − disp` is only valid if nothing
moves the vertex again, and the last three sites run **even when Stage 4 did not
collapse**.

**First result: R0074's earlier numbers were index-aligned after all** — no remap
site fires on it (`YANG_S5_REMAP` silent), so the 16/62 split stands as committed.
F0045 and R0011 each fire `site=s453` only (88 kept/1 dropped; 847/6 and 993/6).

| case | folds | MINTED | INHERITED | minted ratio >1 | minted apex |
|---|---|---|---|---|---|
| **R0011** | 10 | **10** | 0 | **10/10** (med 7.21, max 16.80) | 6/10 TANGENTIAL |
| R0074 | 78 | 16 | 62 | 14/16 (med 3.85, max 81.35) | 3/16 tangential (13 NORMAL) |
| F0045 | 4 | 1 | 3 | 1/1 (1.86) | 0/1 (NORMAL) |

**The `max_disp / min_pre_spacing < 1` criterion holds up across all three:**
violated by **25 of 27** minted folds (93%) and respected by **58 of 65** inherited
ones (89%). It discriminates rather than correlating.

**R0011 replaces R0074 as the epic's lead case — but NOT as a Fig-11 `merge`
customer.** It is the only case whose folds are 100% Stage-4-minted, so fixing the
mint could actually convert it (R0074 keeps 62 inherited folds regardless). But its
mechanism differs from R0074's: minted-fold displacements are **tangential**-dominant
(e.g. `apex_tan=324.1` vs `apex_nrm=51.3`; `316.0` vs `88.2`) and enormous in
absolute terms — up to **328 units on a ~5000-span model (~7%)**. A vertex sliding
7% of the model ALONG its own curve is not off-curve tessellation error being
corrected; it is the relocation choosing the wrong POINT on the curve. R0074's
minted folds are the opposite (~97% normal, `tan≈1e-5` vs `nrm≈3e-4`) — genuine
off-curve correction whose order breaks only because the move dwarfs a
near-duplicate spacing.

⇒ **Two different sub-mechanisms, one criterion.** Do NOT infer mis-relocation from
the printed `reloc(t=…)` values — a vertex on two curves carries one `t`, and `t`
from different curve frames is not comparable (adjacent verts 38/39 read `t=-0.428`
and `t=+2.182`, which proves nothing on its own). The decisive measurement is the
per-surface implicit residual, taken at BOTH the final and the pre-relocation
position — see the next subsection, which refutes the point-selection reading.

### REFUTED — R0011's relocations are EARNED; it is a §4.5.2 refinement case (2026-07-29)

The point-selection hypothesis above ("moved 7% of the model along its curve ⇒ wrong
root") is **refuted by measurement**. The probe now reports each fold vertex's
implicit residual against every incident surface at the final position AND at the
pre-relocation position (`resid=` / `resid_pre=`). On R0011, for every moved vertex:

| vertex | displacement | max&#124;resid&#124; PRE | max&#124;resid&#124; POST |
|---|---|---|---|
| v34 | 245.5 | **84.68** | 1.8e-12 |
| v38 | 245.2 | **107.5** | 9.1e-13 |
| v25 | 328.1 | **52.21** | 2.8e-14 |
| v24 | 179.9 | **46.82** | 9.1e-13 |
| v18 | 46.08 | **42.96** | 9.1e-13 |
| v74 | 22.21 | **10.31** | 9.1e-13 |

Still vertices have PRE ≡ POST at ~1e-13 throughout (they were already exact and
were correctly left alone). Every moved vertex was genuinely far OFF its surfaces
beforehand and is exactly ON them afterwards — **the destinations are correct and the
moves are earned.** A teleport to a different root of the same constraints would show
a SMALL pre-residual; none does.

So the tangential dominance has a different explanation: at a shallow-angle
intersection the mesh curve is offset from the true SSI curve substantially ALONG the
curve, so the nearest true-curve point lies mostly in the chain direction. That is
near-tangency — **exactly §4.5.2's target**. R0011's minted folds are therefore
genuine **local-refinement** customers: the mesh intersection curve approximates the
true curve so poorly that the per-vertex correction (245) exceeds the chain spacing
(34), and a correction larger than the spacing can reorder the polyline however
exact each individual destination is.

⇒ **Both sub-mechanisms are in scope for the epic, and the ratio criterion unifies
them:**

| case | sub-mechanism | spacing vs correction | fix |
|---|---|---|---|
| R0011 | mesh curve poorly approximates the true curve (near-tangency) | 34 vs 245 | **§4.5.2 local refinement** + re-intersect |
| R0074 | near-DUPLICATE vertices, small but relatively huge correction | 9.1e-6 vs 3e-4 | **Fig-11 `merge`** |

Both are `max_disp / min_pre_spacing > 1`; driving that ratio below 1 is the correct
shared acceptance criterion, and the two fixes are the two ways to do it (shrink the
correction by refining; or remove the sub-spacing pair by merging).

**The earned-relocation result is UNIFORM across all three cases** — every moved
vertex in R0074 (37 sampled across its 16 minted folds) and F0045's single minted
vertex show the same shape: pre-residual comparable to the displacement, post-residual
at f64 noise (R0074 e.g. v125 disp 3.005e-4 / pre 2.883e-4 / post 4.2e-17; F0045 v68
disp 2.382e-2 / pre 1.027e-2 / post 0.0). **So there is no mis-relocation anywhere in
this bucket**: Stage 4's per-vertex destinations are all correct, and the entire
defect is the absent mesh update. This is what a0182010's original instinct claimed;
it is now measured rather than inferred, and the two intervening hypotheses
(partial-relocation set, then wrong-root selection) are both retired.

**The two DEVELOPABLE cases are NOT this class** and must not be folded into its
spec (R0028: fold at the ring CLOSURE, 13 from any seam; R0049: ~97 runs makes
the seam test vacuous). **And R0004, which the error-string grouping pulled into
this bucket, cannot convert from a ring fix at all** — its op1
`RevolveAxisIntersectsProfile` scope failure is independent and survives.

**Conversion target for the planar fix: 3 cases (R0074, R0011, F0045)** — not
the 4–6 this section previously implied.

Two dead ends, closed by measurement (do not re-walk them):
- **The SAMPLER is exonerated for the planar trio.** R0074 and R0011 carry ZERO
  interior samples, so `arc_interior_samples`' twin-reversal logic cannot be
  involved; kernel-v2 faithfully walks a loop that already folds.
- **`brep.rs` `clean_spike_loop` is the wrong fix site.** It requires
  LineSegment × LineSegment and collinearity to `1e-9` relative (5.7e-8°),
  which R0074's 179.90° fold misses by ~1.7e6× and F0045's 167.3° by ~2.2e8×.
  Widening that band would merge the fold away and leave the duplicated chain
  stretch underneath — the R0091 silent-wrong trap. **Structural fix only.**

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0045 | ring rejected by CDT (FaceId 9) | probe 2026-07-18 (`KV2_RING_REJECT_PROBE` + polygon analysis): 21-pt ring with ONE proper self-crossing (segs 10-11 × 12-13) — a fine-sampled arc that doubles back over itself via a coarse return chain (two different samplings of overlapping curve sections in one ring). **NOT chained-input**: F0045 is two primitive extrudes (parallel cyl boss+boss, gen.rs F0041-45) and this is the FIRST boolean — the defective ring is minted by THIS boolean's own Stage-5/6 output emission, then rejected by kernel-v2 render CDT (loud, correct). **CONFIRMED SEAM-OVERLAP 2026-07-29 (`KV2_RING_PROVENANCE`, 70ccf32c):** PLANAR builder (`sampled_loop_points`), 13 half-edges (8 Arc + 5 LineSegment, 8 interior samples), 4 adjacency runs. Folds at idx 12 (167.3°) and 16 (165.3°), each exactly seam+1; control seam idx 8 turns 4.2° and is clean. The "coarse return chain" framing was directionally right but named the wrong quantity — the crossing segments are only 4.7x apart in length; the defect is chain-range OVERLAP at the seam, not sampling density | CONFIRMED (#171 pass 2; mechanism 2026-07-29 — 4/4 folds straddle a relocated/non-relocated boundary) | **Stage-4 partial relocation of a boundary chain** (with R0074, R0011) |
| R0011 | ring rejected by CDT (FaceId 407) | ~~probe 2026-07-18: 398-pt ring at scale ~2900 with 3 LOCAL zigzag crossings (each within a 4-index window: 23-27, 28-32, 390-394) — the #145 sample-misorder signature surviving in an output ring~~ **IMPROVED + RE-DIAGNOSED 2026-07-29 (`KV2_RING_PROVENANCE`, 70ccf32c):** the ring is now 392 pts with **ONE** crossing (26×28), down from 398/3 — something upstream between 07-18 and 07-29 removed two of them. PLANAR builder, 392 half-edges (380 LineSegment + 12 EllipseArc), **ZERO interior samples** ⇒ the sampler is exonerated; kernel-v2 walks a loop that already folds. 9 adjacency runs; fold at idx 28 (176.8°), seam+1. **Strongest control in the bucket:** six seams do NOT fold, including two genuine corners at 83.7° and 85.5°. "Sample-misorder" is the wrong name — ring order is monotone; two chain RANGES overlap | CONFIRMED (#171 pass 2; mechanism 2026-07-29 — 10/10 folds straddle a relocated/non-relocated boundary) | **Stage-4 partial relocation of a boundary chain** (with R0074, F0045) |
| R0016 | ring rejected by CDT (FaceId 1885) | probe 2026-07-18: 646-pt micro-scale ring (r≈0.03) with **15 periodic near-dup pairs** at (i, i+2) ~1.1e-4 apart (spike/needle pattern repeating with period ~310) + 1 crossing — the #146 near-duplicate junction-vert mint materialized in an output ring | CONFIRMED (#171 pass 2) | P3a-#146 (near-dup junction mint) |
| R0028 | ring rejected by CDT (FaceId 32) | probe 2026-07-18: 146-pt ring, 2 crossings at the ring CLOSURE (segs 1×142, 4×138) — the chain tail folds back over the start (overlapping closure, not a mid-ring zigzag). **SPLIT OFF THE SEAM CLASS 2026-07-29 (`KV2_RING_PROVENANCE`, 70ccf32c):** FaceId 32 is **NOT planar** — it never calls `sampled_loop_points`; it is a **developable** patch (`tessellate_developable_patch`, unrolled (u,v) cut frame). Ring = 25 origin nodes + **121 arc-sample nodes**; only 3 adjacency runs (seams at 13, 71) and the single fold is at **idx 0 — the ring closure, 13 indices from any seam**. This is NOT the planar seam-overlap mechanism and must not be folded into its spec. Undiagnosed; the unroll cut at u≈0 is the obvious first suspect (idx 0 sits at x=4.3e-19) | PARTIAL (site + builder confirmed 2026-07-29; mint unknown) | **developable-patch ring (own row)** — was P3-junction S5/S6 |
| R0017 | KV9-F2 folded patch triangulation | probe 2026-07-17: error class CHANGED — kernel-v2 `TessellationFailed` FaceId(14) "patch triangulation folded (inverted triangle)" (unrolled ear-clip fold), not the old holed-lateral CDT | CONFIRMED (#171 sweep) | kernel-v2 KV9-F2 |
| R0085 | op1: ring rejected by CDT (FaceId 566); op2: LRR v5 | probe 2026-07-18: TWO independent failures. **op1** (Revolve 2 union): 42-pt ring, 3 fold crossings (0×33, 6×32, 33×41) — output-ring fold, same family as R0028. **op2** (Revolve 3 union): ~~`YANG_LRR_SITE site=lineseg_combo` edge (5,550) — the missing cone-generator LineSegment closed form~~ **op2's lineseg layer RESOLVED 2026-07-28 (cone-generator arm)**; the case stays ERROR on op1 regardless, and op2 now STOPs one layer deeper: `YANG_V_PROBE` v5 = `line=true + torus=true + endpoint=true` → the TORUS block's `endpoint_set` guard (`stage4_correct.rs`, "a torus-edge endpoint that is also a CONIC endpoint mixes the implicit-pair and closed-form relocations — out of v1 scope"). A torus × cone-generator junction: the R0044 endpoint-mix class with a line instead of a conic | CONFIRMED (2026-07-28 probe) | op1: P3-junction (output ring); op2: P3-junction (R0044 endpoint-mix, torus×line) |
| R0100 | KV9-F2 folded patch triangulation | probe 2026-07-17: error class CHANGED — kernel-v2 `TessellationFailed` FaceId(15) folded ear-clip, same class as R0017 | CONFIRMED (#171 sweep) | kernel-v2 KV9-F2 |
| F0067 | converted-input CDT failed (face 272) | M8 opposite-rim projection class (#142/#143/#144) | CONFIRMED (task #144) | M8 |

### Stage-3 SSI (2)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0043 | AmbiguousCurve {1, 0} edge (23,93) | probe 2026-07-18 (`YANG_S3_AMBIG_PROBE`): the two surfaces are **internally tangent cylinders BY DESIGN** (r=1.0 at origin, r=0.4 at x=0.6; axis distance 0.6 = 1.0−0.4; gen_complexity.rs: "the degenerate tangency is the test", union == operand A by design). The single candidate IS the exact tangent generator Line{[1,0,0], ẑ}; the mesh intersection chords sit 4.5e-2 off it (= tol, the near-parallel-surface amplification at tangency) so matched=0 — a correct loud STOP on 1D line-contact tangency. Same contact-degeneracy family as C0107–C0110 | CONFIRMED (#171 pass 2) | degenerate-tangency SSI vocabulary (C0109 family) or scope sign-off |
| C0056 | AmbiguousCurve {1, 0} edge (37,70) | probe 2026-07-18: same signature, also BY DESIGN — internal lateral tangency cut (r=1.0 origin × r=0.5 at x=0.5, axis distance 0.5 = 1.0−0.5; "wall thins to zero at the tangent line"); candidate = Line{[1,0,0], ẑ}, chords 4.9e-2 off. Output would be zero-thickness at the tangent line (C0114/C0115 kin) | CONFIRMED (#171 pass 2) | degenerate-tangency SSI vocabulary or scope sign-off |

### NonPlanarFace (3)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0069 | NonPlanarFace FaceId(6227) | off-plane planar-face emission 3e-8 @ 2m | CONFIRMED (task #153) | #153 |
| F0072 | NonPlanarFace FaceId(10329) | same class (also the known assay-timeout artifact case) | CONFIRMED (#153) | #153 |
| R0081 | ~~NonPlanarFace FaceId(666)~~ non-2-manifold (reassembly) | ~~likely same class~~ **RE-DIAGNOSED 2026-07-28 (cone-generator arm): the #153 vehicle was wrong.** R0081's live failure was a Stage-4 LRR at v590, and `YANG_LRR_SITE site=lineseg_combo` edge (590,592) shows **Plane(A) × Cone(B, half-angle 0.954 rad ≈ 54.7°)** — the R0008 class, a THIRD case this bucket never identified (it was never probed; the row was `SUSPECTED` by proximity). With the arm wired, that layer is gone and R0081 STOPs at Stage-6 `reassembled output would be non-2-manifold` | CONFIRMED (2026-07-28 probe) | P3a-#146 (reassembly non-2-manifold) |

### Misc structural (5)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0046 | NonManifoldVertex(17) | probe 2026-07-18: **0D corner contact BY DESIGN** (gen_complexity.rs: two boxes sharing exactly one vertex, "legitimately non-manifold, loud rejection acceptable"); kernel-v2 `validate.rs` vertex-fan check rejects the union output — the DESIRED posture, same as C0107/C0108 | CONFIRMED (#171 pass 2, by construction) | none (scope boundary; sign-off candidate) |
| C0048 | azimuth-merge rims mismatched (66 vs 69) | M8 rim-crossing/uniform-sample merge (#143 landed; residual = #144 snap-rounding) | CONFIRMED (#144) | M8 |
| C0075 | InvalidBooleanOutput (undirected edge ≠ 2 directed) | probe 2026-07-18: **two overlapping 12-tooth gear extrudes, union, BY CONSTRUCTION** (gen_complexity.rs Group 2e "gear/CDT tail") — the known non-convex gear-profile capability tail; the unpaired-edge reject is its loud downstream symptom (8.3s run, deep into the pipeline) | CONFIRMED (#171 pass 2, by construction) | gear/profile tail (Phase-2, KV/scope) |
| R0019 | input B-Rep not 2-manifold | probe 2026-07-17 REFUTES the chained-defect suspicion: the FIRST boolean (`op=Subtract a: 2v/3f`) rejects — operand A is a 2-vertex/3-face revolve-primitive B-Rep the yang input gate cannot accept (primitive topology vocabulary, KV6-class) | CONFIRMED (#171 sweep) | KV6/scope |
| R0053 | patch flood-fill LabelMismatch {seed 2, tri 3890} | probe 2026-07-18 (new `CHERCHI_PATCH_PROBE`): the flood from a seed labeled `[InputId(0)]` reaches tri 3890 labeled `[InputId(1)]` after 956 triangles — **genuinely DISJOINT single labels** (not the L2a compatible coplanar-sheet case). An A-only region floods into a B-only region across 2-incident MANIFOLD edges ⇒ the A×B intersection curve is missing/unsplit there — a Stage-2 arrangement incidence gap (revolve×revolve op, kin to R0050's empty-partner signature) | CONFIRMED (#171 pass 2) | Stage-2/3 arrangement incidence (near-coincident revolve surfaces, R0050 kin) |

### Capability / scope (4)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| R0007 | NotSupported: coplanar input pair | Stage-0 M8 residue | CONFIRMED (#130) | M8 |
| R0071 | NotSupported: coplanar input pair | Stage-0 M8 residue | CONFIRMED (#130) | M8 |
| C0063 | NotSupported: curved partial-patch cone operand | curved-profile capability tail | CONFIRMED (scope) | KV6/scope |
| R0004 | RevolveAxisIntersectsProfile **+ a second, independent ring-reject (FaceId 514)** | self-intersecting revolve — capability boundary. **AMENDED 2026-07-29: R0004 is a TWO-FAILURE case** (like R0085). op1 still fails `RevolveAxisIntersectsProfile` — the scope boundary this row always named, unchanged. A *second* engine error is a `boolean_subtract` ring-reject whose ring carries the same fold signature (1 crossing 244×246, folds at v245/v246 = 136.2°/180.0°, plus a near-dup pair at (249,251) 9.6e-5 apart). **Consequence: R0004 cannot convert from any ring fix** — the revolve scope failure survives it. Do not count it in the ring bucket's conversion target | CONFIRMED (scope; second failure measured 2026-07-29) | KV6/scope (ring half is downstream noise) |

## Group-7 additions (task #176, 2026-07-17 — designed cases, root cause known by construction)

The junction-scenario corpus expansion (spec
`specs/assay_junction_scenario_corpus.md`, C0102–C0117, corpus 295→311) adds
7 non-CORRECT cases. Unlike the legacy tail these are self-triaged: each was
authored to exercise a named mechanism, so they enter CONFIRMED, not PROBE.

| Case | Category | Root cause (by construction + first-run evidence) | Vehicle |
|---|---|---|---|
| C0105 | ~~SUPPORTED_WRONG~~ **ERROR (loud STOP, 2026-07-17)** | cone∩plane∩plane notch corners → WAS a silent non-watertight, self-intersecting shell; the #173/N6 render-level gate now rejects the subtract (`SelfIntersectingBooleanOutput`, 76 penetrations). #177 residual = the watertightness half (how 51 unpaired edges evaded a gate) + the eventual P3-junction fix | **#177 residual**, then P3-junction |
| C0107 | ERROR (loud ✓) | point-tangent sphere⊕cyl union — 0D curved contact, typed non-2-manifold reject is the DESIRED posture | none (scope boundary; sign-off candidate) |
| C0108 | ERROR (loud ✓) | externally tangent spheres — same 0D-contact reject | none (scope boundary; sign-off candidate) |
| C0109 | ERROR (loud ✓) | internal point-tangent sphere cavity — Stage-3 AmbiguousCurve{0,0}: no curve vocabulary for the degenerate sphere×sphere tangency | P2-M5-adjacent (degenerate SSI vocabulary) |
| C0111 | ~~SUPPORTED_WRONG~~ **ERROR (loud STOP, 2026-07-17)** | 1e-8 m wall (below MIN_FEATURE_SIZE) WAS silently dissolved (χ 0→2); the #178/N57 Stage-0 sub-resolution coplanar-gap STOP now rejects the subtract (`SubResolutionCoplanarGap`, gap=1e-8, spec `yang_178_subres_coplanar_gap_stop.md`). Terminal: out-of-contract input, loud by design | **#178 DONE** |
| C0113 | ~~SUPPORTED_WRONG~~ **ERROR (loud STOP, 2026-07-17)** | wall at exactly TAU_MODEL WAS silently dissolved — the R0091 hazard rung; the #178/N57 STOP now rejects it (`SubResolutionCoplanarGap`, gap=1e-7). Terminal: loud by design | **#178 DONE** |
| ~~C0116~~ | ~~SUPPORTED_WRONG~~ ~~ERROR~~ **SUPPORTED_CORRECT (2026-07-17)** | 0.01-deep cyl×cyl graze: WAS watertight/χ/volume-passing with a self-intersecting shell, then a loud #173/N6 render-gate STOP. ROOT-FIXED by the #172 Case-III graze guard (spec `yang_172_case_iii_graze_guard.md`): boolean-entry rim-N boost (derived N=29) makes the chord meshes sample the wedge; the shipped SurfacePair Stage-3/4 machinery refines it — all oracles green incl. the render selfx gate | **#172 DONE** |
| C0118 | ERROR **by design** (new case, 2026-07-17) | 1e-8-deep cyl×cyl micro-graze — the #172 guard's STOP arm: genuine intersection above authoring noise but below the rim-N-cap observability floor; typed `SubSagittaGrazeIntersection`. Terminal: loud by design (the silent failure is unfused two-lump emission below even the render gate's sagitta) | **#172 DONE** (designed STOP) |

The 4 SUPPORTED_WRONG entries change the corpus summary from 240C/0W to a
baseline that shows WRONG > 0 **by exposure, not regression** — the 295
pre-existing cases are byte-identical. The 0-WRONG ratchet (#174) applies to
the pre-existing set immediately and to the whole corpus once #177/#178/#173
convert these to loud STOPs (the ratchet may then never regress).

**2026-07-17 UPDATE: all four are now loud.** #173 converted C0105/C0116;
#178 (spec `yang_178_subres_coplanar_gap_stop.md`, deviation N57) converted
C0111/C0113. **C0116 subsequently ROOT-FIXED to SUPPORTED_CORRECT by the
#172 Case-III graze guard (same day).** Committed baseline
**250C / 0 WRONG / 55E / 3T on the 312-case corpus** (after #172 increment
2: C0116 ERROR→CORRECT, new designed-ERROR C0118; F0072/F0085/F0090 are
120s-budget-borderline TIMEOUT artifacts — F0090 solo-verifies
SUPPORTED_CORRECT at 115.6s, ~0.1% above its 115.4s pre-guard solo; both
states loud. Prior rungs: 250C/57E on 311 after the torus×torus lift;
249C/58E before it). The #174 ratchet may now bind corpus-wide. #178 follow-up scenarios
recorded in N57: coincident-CYLINDER sub-resolution radius gap (needs a
corpus case FIRST, coverage directive) and tilt-only sub-resolution wedges.

## Probe sweep 2026-07-17 (#171, first pass over the queue)

All 33 queue cases replayed with `YANG_LRR_PROBE` / `NONMANIFOLD_SITE_PROBE`
/ `YANG_S6_NONPLANAR_PROBE` / `YANG_COPLANAR_PROBE` (release, 180 s caps;
logs archived in the session scratchpad). 12 rows upgraded to CONFIRMED
above. Findings that shape the remaining queue:

- **`split_max_passes` is a 4-case class** (R0009, R0047, R0063, R0091) —
  every u32::MAX LRR in the queue shares the chord-split budget-exhaustion
  site. One mechanism, one future fix vehicle (the §4.5.2 guarded
  refinement shell).
- **The #169 "fails downstream" bucket resolves into named sites**:
  s4-shell-euler (F0058/F0060), s4-halfedge-pairing open seam (F0085,
  R0038-type), s6-planar-loop-nonplanar (R0049 = F0064 class),
  s6-curved-degenerate-loop (C0058).
- **Two error classes silently drifted since their rows were written**:
  R0017/R0100 now fail as kernel-v2 KV9-F2 folded ear-clip (not holed
  lateral CDT); R0019's non-2-manifold INPUT is the first boolean's
  revolve-primitive operand (2v/3f), refuting the chained-defect theory.
- **Still PROBE after the sweep** (no deeper probe light; each needs a
  targeted per-case dig, not another sweep): the identified-vertex LRR
  regions (C0067 v128, R0008 v42, R0020 v44, R0025 v1760, R0032 v32,
  R0035 v194, R0050 v58, R0077 v3, R0085-op2 v5; all with §4.5.1
  `n_over=0` — interior-bounded recovery inapplicable), the
  OffCurve trio (R0015/R0026/R0070), the CDT ring-rejects
  (F0045/R0011/R0016/R0028/R0085-op1 — need a kernel-v2 ring-geometry
  probe), Stage-3 AmbiguousCurve{1,0} (C0043/C0056 — need a candidate/
  match probe), C0046, C0075, R0053.
- All coplanar cross pairs in the queue cases are femto-class
  (max 1.1e-13, `subres=false`) — live confirmation of the #178/N57 line.

## Probe sweep 2026-07-18 (#171, pass 2 — targeted per-case digs; QUEUE EMPTY)

All 14 remaining PROBE rows upgraded to CONFIRMED (rows above carry the
evidence). New dev-only instrumentation shipped for the digs (all env-gated
print-only, production byte-identical): `YANG_LRR_SITE` tags at every
specific-vertex `Stage4RegionInvalid` STOP (classification combos in
`stage4_correct.rs` + relocate-math guards in `stage4_relocate.rs`),
`YANG_TORUS_STOP` tags at the torus-block pre-relocation STOPs
(pair/triple Newton `None`, partner-count arm), and `CHERCHI_PATCH_PROBE`
(labels at the flood-fill `LabelMismatch`). Key discoveries that reshape the
buckets:

- **The "identified-vertex LRR" group is NOT one class — it is five**:
  (a) ~~surface-pair endpoint-mix (R0020, R0035, R0070-op2 — all `conic +
  surface_pair + endpoint`, the exact R0044 class; now a **4-case bucket**)~~
  **CLOSED 2026-07-28.** Every vertex in the bucket had EXACTLY 3 incident
  surfaces — the plain triple point the increment-5 block already resolves.
  The bucket existed because that block's candidate set enumerated the six
  *conic* maps and not `vert_surface_pair`, so an ellipse × surface-pair
  junction scored `n_maps == 1` and fell through to the "out of v1 scope"
  guard. Same shape as class (d): **the capability existed and one map was
  unreferenced.** Two of the four turned out to be carrying deeper, unrelated
  causes underneath (see their rows);
  (b) torus×plane pair-Newton non-convergence (R0025, R0077);
  (c) torus×cone pair-Newton non-convergence (R0032);
  (d) ~~missing cone-generator LineSegment closed form (R0008 near-flat cone
  88.9°, R0085-op2 needle cone 0.85° — the Stage-4 lineseg arm only knows
  cyl×plane / cyl∥cyl / plane×plane)~~ **CLOSED 2026-07-28** — and the
  "missing closed form" framing was wrong: `ssi_rs::plane_cone` had the
  through-apex `Line` all along, Stage 3 had the band and (since N45) the
  crossing tie-break. Both gaps were Stage-4 WIRING. Lesson for the rest of
  this table: a row that names a *missing capability* deserves a check for
  whether the capability exists one stage over and is merely unreferenced —
  the cheapest fixes in this tail have all been of that shape;
  (e) circle×circle junction (C0067) and empty-partner incidence gap (R0050).
- **The OffCurve trio's N51 "no-curve-type" diagnosis is REFUTED**: R0015 and
  R0026 are torus-map vertices whose pair Newton relocates them exactly
  (rho=0 / 9.6e-6 ≪ gate) and then the bounded-face CONTAINMENT gate
  (`stage4_correct.rs:4225`) rejects — the C0065 #137 signature at micro
  scale. R0070 is a micro ellipse∩line junction + an R0044-class op2.
- **The CDT ring-reject bucket's "chained-input casualties" header is
  REFUTED for its lead case**: F0045 is two primitive extrudes and fails at
  the FIRST boolean — the self-intersecting ring is minted by that boolean's
  own Stage-5/6 output emission. Ring geometry splits the bucket into
  sample-misorder zigzags/folds (F0045, R0011, R0028, R0085-op1) vs periodic
  near-dup spikes (R0016, (i,i+2) pairs ~1.1e-4 — the #146 mint materialized
  in an output ring). kernel-v2's render CDT reject is the loud backstop.
- **C0043/C0056/C0046/C0075 are designed-degeneracy cases** (internal cyl×cyl
  tangency ×2, 0D corner contact, gear tail) — loud STOPs are the correct
  posture; the tangency pair + C0046 are sign-off candidates alongside
  C0107–C0110.
- **R0053 is a Stage-2 arrangement incidence gap**: disjoint-label flood
  ([A]-only region → [B]-only region across 2-incident manifold edges) ⇒ the
  A×B intersection curve is missing/unsplit there; kin to R0050's
  empty-partner torus edges (both are near-coincident revolve-surface pairs).

## Rollup (post pass 2 — Phase 1 triage COMPLETE, 0 PROBE rows)

| Vehicle | Cases |
|---|---|
| P2-M5 (SSI solvers) | 0 open (R0096 flipped CORRECT #172; R0044 → P3-junction) |
| P3a-#146 (junction mint / incidence) | 8 open (C0058, F0082, C0044, F0058, F0060, R0016, R0050, R0081) + F0064 partial + R0051 suspected; ~~R0095~~ FLIPPED CORRECT (#195 inc-5); ~~R0049~~ **re-vehicled 2026-07-29** → Stage-2/3 incidence (its error class drifted to a developable ring-reject) |
| P3b-#137 (torus∩plane + grazing/tangency Stage-4) | 7 confirmed (C0065, R0038, R0015, R0026, R0025, R0077, R0032-torus×cone) + F0085 (open seam, R0038-type); ~~R0074~~ **re-vehicled 2026-07-29** → planar output-loop seam-overlap (the OffCurve layer is gone; it is now a ring-reject and the lead witness of that class) |
| P3c (curved re-CDT) | **0 open** — ~~R0072~~ FLIPPED CORRECT 2026-07-28 (#195 inc-5); the vehicle has no remaining case |
| P3-junction (other junction vocabulary) | 4 confirmed (R0003, C0067, R0070-v1028, R0085-op1) + R0085-op2 (torus×line endpoint-mix). **The R0044 surface-pair endpoint-mix sub-bucket is CLOSED 2026-07-28** — ~~R0035~~ CORRECT; R0044/R0020/R0070-op2 cleared this layer and re-vehicled to their deeper causes. ~~F0045, R0011, R0028~~ **split out 2026-07-29** into the two rows below |
| **§4.4.1/§4.5.2 MESH UPDATING after relocation** (new, 2026-07-29; supersedes the same day's "partial relocation" and "seam-overlap" framings) | **3 confirmed — R0074, R0011, F0045.** Stage 4 relocates a SUBSET of a boundary chain onto the exact analytic geometry and leaves the rest at their Stage-1/2 mesh positions; the moved↔still boundary retraces as a near-180° fold, which kernel-v2's render CDT loudly rejects. **81 of 92 folds straddle that boundary; ZERO lie entirely in un-moved geometry.** Measured at yang's own emission site (`stage5_topology.rs` `push_loop`), so the mint is Stage 4, not Stage 5 ordering and not kernel-v2. **CORRECTED same day:** the relocation SET is CORRECT — un-moved fold vertices carry `Plane`-only incidence and are genuinely not on the intersection curve. The defect is that Stage 4 displaces relocated vertices by up to **101x the incident mesh edge length** (25/78 folds displace further than their shortest incident edge) **without updating the incident mesh**, which Yang §4.4.1/§4.5.2 require. These 3 cases are therefore CUSTOMERS OF THE MESH-UPDATING EPIC (#169 / N2, specs already written), not a standalone fix site. **RE-SCOPED again 2026-07-29 (anchor verification, §"SCOPED — the epic owns 16 of R0074's 78 folds"): this row is THREE classes, not one.** Operand-qualified incidence + the pre-Stage-4 turn angle show R0074 = **16 folds MINTED** by Stage 4 (turn_pre 0.00° → 179.9x°) and **62 INHERITED** from the Stage-2/3 boundary cycle (already folds before Stage 4, which perturbed them by a median 1.25°) ⇒ the epic owns 16, the rest route upstream to #146, and **R0074 cannot green from mesh-updating alone**. F0045 is instead the **Fig-11 q triple point** with a CYLINDER third surface (`A:Cylinder+A:Plane+B:Cylinder`, 4/4 apexes own-rim) — the built inc-3 `plan_triple_point_reseats` skips it because its closed form requires a `Plane`. R0011 is a third signature. The acceptance criterion is CONFIRMED but with the **pre-relocation spacing** as denominator (not the post-move incident edge): `max_disp / min_pre_spacing` is >1 for 14/16 minted (median 3.85, max **81×**) and ≤1 for 56/62 inherited (median 0.22) — it separates the populations. Blocking gap: F0045 and R0011 both take a §4.5.3 collapse, so their positional oracle is unavailable and their minting is UNMEASURED. |
| **Developable-patch ring rejects** (new, 2026-07-29) | 2 open (R0028 closure-fold, R0049 ~97-run fragmentation) — different builder (`tessellate_developable_patch`), different mints, both PARTIAL. Do NOT fold into the planar seam-overlap spec |
| M5 surface-pair Newton convergence (new, 2026-07-28) | 2 confirmed (R0044 v13, R0020) — pure `vert_surface_pair` vertices whose `relocate_onto_implicit_pair` diverges; kin to the torus `pair_newton_none` trio (R0025, R0032, R0077) |
| kernel-v2 surface-pair render band (new, 2026-07-28) | 1 confirmed (R0020, fatal) — `surface-pair refinement needs a positive finite chord tolerance` on an output `Curve::SurfacePair` edge |
| Stage-4 cone-generator LineSegment arm | **0 open** — ~~R0008~~ FLIPPED CORRECT 2026-07-28; ~~R0085-op2~~ and ~~R0081~~ resolved at this layer but still ERROR one layer deeper. The vehicle has no remaining case. **It had 3 cases, not 2** — R0081 sat under `#153 / SUSPECTED` and was found only by re-probing after the fix |
| Stage-2/3 arrangement incidence (near-coincident surfaces) | 1 confirmed (R0053; R0050 kin, counted under P3a) + **R0049 PARTIAL** (2026-07-29: ~97 adjacency runs on one 214-edge ring — fragmentation consistent with this family, mint unconfirmed, run-count caveated) |
| P3-§4.5.2 (split budget) | 4 confirmed (R0009, R0047, R0063, R0091) |
| M8 residue | 4 confirmed (R0007, R0071, C0048, F0067) |
| #153 NonPlanarFace | 2 confirmed (~~R0081~~ re-diagnosed 2026-07-28 → P3a-#146; it was never a #153 case) |
| kernel-v2 KV9-F2 folded ear-clip | 2 confirmed (R0017, R0100) |
| KV6/scope + designed degeneracies | 7 (C0063, R0004, R0019, C0043, C0056, C0046, C0075 — the last four sign-off candidates). **R0004 amended 2026-07-29:** two-failure case — its op1 scope failure stands, and its second (ring-reject) failure means no ring fix can convert it |
| **PROBE queue** | **0** (was 26 → 14 after pass 1 → 0 after pass 2) |

**Reading:** Phase 1 (triage) is COMPLETE — every failing case has a confirmed
root cause and fix vehicle. The distribution vindicates the plan's ordering:
the junction layer (P3a + P3b + P3-junction ≈ 26 cases) is the dominant mass.
Within it, three sharply-defined sub-classes emerged that were invisible
before pass 2: the R0044 endpoint-mix bucket (4 cases, one STOP site), the
torus pair-Newton/containment family (6 cases), and the S5/S6 output-ring
assembly defects (4-5 cases, includes a first-boolean mint refuting the
chained-input theory). The cone-generator LineSegment arm (2 cases) is the
one small self-contained closed-form gap — a candidate quick win before the
junction epic. Sign-off batch candidate: C0043/C0056/C0046/C0075 designed
degeneracies (with C0107–C0110).

**AMENDED 2026-07-29 (ring-reject provenance sweep, 70ccf32c).** The "S5/S6
output-ring assembly defects (4-5 cases)" line above is superseded: that group
was assembled from ERROR-STRING similarity, and probing split it 3 / 1 / 1 / 1
across four different vehicles (planar seam-overlap ×3, developable closure-fold
×1, near-coincident incidence ×1, plus R0004 which cannot convert at all).
**The confirmed, self-contained conversion target is the 3-case planar
seam-overlap class** — one mechanism, one fix site, controls that hold.

Method note worth carrying forward: this is the **fourth** time a row grouped by
proximity or shared error text has split under a targeted probe (R0081 under
`#153/SUSPECTED`; the endpoint-mix group into five; the "chained casualties"
header; now this bucket). Two rows here had also silently DRIFTED to a different
error class since pass 2 (R0074 OffCurve → ring-reject, R0049 non-2-manifold →
ring-reject) and one had partially self-healed (R0011, 3 crossings → 1). **Re-run
the case before trusting any row's error class; group by measured mechanism,
never by error string.**
