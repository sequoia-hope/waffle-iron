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
| R0044 | Stage-4 LRR ~~v11~~ v13 | ~~torus×torus (N52)~~ ~~**RE-DIAGNOSED (#172):** the surface-pair endpoint-mix STOP~~ **ENDPOINT-MIX LAYER RESOLVED 2026-07-28 (triple-block wiring):** the mix vertices (v8, v12) have exactly 3 incident surfaces `{cyl_A, plane_B, cone_B}` and relocate through the increment-5 triple block. R0044 now STOPs one layer deeper at `stage4_correct.rs:5646` — v13 is a **pure** surface-pair vertex (`n_maps == 1`, correctly not a triple) whose `relocate_onto_implicit_pair` NEWTON DIVERGES. Same family as the torus `pair_newton_none` cases | CONFIRMED (2026-07-28 `#[track_caller]` LRR-site trace) | M5 surface-pair Newton convergence (with R0025/R0032/R0077) **2026-08-19 (M5 spec §"2026-08-19"): the pair-Newton "divergence" was the cone step overshoot (sec α, KV16 fix missing from the pair solver) → FIXED; then the same-type SurfacePair junction (cyl×cone_B1 ∩ cyl×cone_B2, one-slot map) → FIXED via `same_type_junction`; then kernel-v2 K9 cone sag radius 0 → FIXED (`pair_surface_local_scale`); then the projector's bare 1e-13 tau at |x|≈6e3 → FIXED (8·ε·L floor). NOW: kernel-v2 render `ring rejected by CDT` FaceId(460) — ring-reject family; MEASURED (`KV2_RING_REJECT_PROBE`/`KV2_PATCH_PROV`): face 460 is a curved patch, 184-node ring, with a REVERSAL at idx 176→177→178 in the unrolled frame (177 sits ~3.8 units BEHIND 176 along the 176→178 direction at coordinate scale 3e3; twins 34907/34990/35000 — three different neighbour edges, i.e. the crease-adjacent chain vertices) — a §4.5.3 reversed-intersection on a PROCEDURAL (surface-pair) chain, which the conic-loop sweep does not cover; NOT the K9 samples (n_interior/positions are the ring's own vertices). Vehicle: §4.5.3 sweep over surface-pair chains / junction placement at the crease |
| ~~R0096~~ | ~~Stage-4 LRR v7~~ | ~~torus×torus~~ **FLIPPED CORRECT 2026-07-17 (#172):** torus×torus lateral∩lateral + torus×torus×plane junctions now relocate via the implicit-pair/triple Newton (torus-block scope lift) | — | ~~P2-M5~~ DONE |
| R0038 | Stage-4 LRR (u32::MAX) | plane tangent to cylinder along one generator; degree-2 gate self-validates (`bad_degree=[(18,4),(19,4)]`) — near-tangency pinch, NOT a CDT ring | CONFIRMED (#168 WIP4, 9f4cb604) | P3b-#137 |
| ~~R0072~~ | ~~Stage-4 LRR (u32::MAX)~~ | ~~real ~1e-7 micro-scale edge (0.4% span); force-merge is the R0091 silent-wrong trap — needs curved re-CDT~~ **FLIPPED CORRECT 2026-07-28 (#195 inc-5):** the §4.5.4 detect-then-refine rim boost + §4.4.1 rim-snap, both now always-on, resolve it WITHOUT a curved re-CDT — the micro-scale edge was an under-sampled rim, not an irreducible feature | — | ~~P3c~~ DONE |
| C0058 | non-2-manifold (reassembly) | probe 2026-07-17: `NONMANIFOLD_SITE s6-curved-degenerate-loop` — Stage-6 curved face 2 emits a 64-vertex loop with \|Newell N\| = 2.3e-16 (degenerate junction loop) | CONFIRMED (#171 sweep) | P3a-#146 |
| C0067 | Stage-4 LRR v128 | probe 2026-07-18 (#171 pass 2): v128 is a **circle×circle junction** (`circle_junction=true`, endpoint) — two sphere-section Circles (both r=0.371, centers [0.15,0,0.5]/[0,0.15,0.5], normals x̂/ŷ) meet at [0.15,0.15,0.83]; junction relocation region invalid. Needs two-curve junction relocation (mint-once contract) | CONFIRMED (#171 pass 2) | P3-junction |
| ~~R0008~~ | ~~Stage-4 LRR v42~~ | ~~probe 2026-07-18: `YANG_LRR_SITE site=lineseg_combo` edge (42,43) — LineSegment edge whose incidence is **Cone(A, half-angle 1.5525 rad ≈ 88.9°, near-flat) × Plane(B)**; the Stage-4 LineSegment arm has closed forms only for cyl×plane / cyl∥cyl / plane×plane — the **cone-generator line closed form is missing**~~ **FLIPPED CORRECT 2026-07-28 (cone-generator arm):** the closed form was never missing — `ssi_rs::plane_cone` has emitted `SsiCurve::Line` for through-apex cuts all along and Stage 3 already banded them via `cone_chord_tol_for_owner`. TWO wiring gaps, both in Stage 4: (a) the LineSegment pair match classified `Cone` as `other_curved` → STOP before selection; (b) once admitted, the tie-break called the R0072-only `select_disjoint_parallel_line`, whose parallelism precheck rejects the two CROSSING apex generators (`AmbiguousCurve{2,2}`). **#163/N45 was not a "residual theory" — it was CORRECT and already shipped, at Stage 3 only**; the two stages had been running different tie-breaks since 9fca8393 | — | ~~Stage-4 cone-generator LineSegment arm~~ DONE |
| R0009 | ~~Stage-4 LRR (u32::MAX)~~ kernel-v2 `CurvedGeometryMismatch` FaceId(10) (op 2) + Stage-4 shell gate double-cover (op 3) | ~~probe 2026-07-17: `site=split_max_passes` — the chord-split loop exhausts its pass budget (§4.5.2 refinement demand, non-convergent)~~ **RE-DIAGNOSED + LAYER PEELED 2026-08-19 (spec `yang_n2_stage4_cdt_mesh_updating.md` §5c.13):** NOT a §4.5.2 demand — the §4.4.1(a) unzip loop's degeneracy test was the ABSOLUTE `MIN_FEATURE_SIZE²` area floor, which at this 1.05e-4 model scale flagged HEALTHY triangles (h/l 0.007–0.40) and ping-ponged a 4-action flip cycle to the pass cap. Fixed (scale-free collinearity identity + cycle certificate). Now advances to `s4-shell-euler double-cover edge (32,33) fwd=2 rev=2` — A cyl-2 ×2 + B plane-1/plane-5 ×2 on one intersection edge (the #146 double-cover family), pre-existing (zero unzip actions post-fix; connectivity untouched by Stage 4) | CONFIRMED (2026-08-19 `YANG_LRR_SITE` + shape census) | P3a-#146 double-cover (was P3-§4.5.2) |
| R0020 | ~~Stage-4 LRR v44~~ TessellationFailed FaceId(21) | ~~probe 2026-07-18: v44 is the surface-pair endpoint-mix STOP — the R0044 class exactly~~ **ENDPOINT-MIX LAYER RESOLVED 2026-07-28:** v44's incidence is exactly 3 (`{plane_A, cone_A, cyl_B}`) and relocates through the triple block. Two deeper layers now: a pure surface-pair Newton divergence at `:5646` (R0044's new class), and the fatal one — kernel-v2 **`surface-pair refinement needs a positive finite chord tolerance`**, i.e. the OUTPUT B-Rep now carries a `Curve::SurfacePair` edge that kernel-v2's render tessellation cannot band | CONFIRMED (2026-07-28) | kernel-v2 surface-pair render band + M5 pair-Newton **2026-08-19: the "needs a positive finite chord tolerance" wall was `pair_surface_scale(Cone)=0` feeding the K9 sag radius → FIXED (`pair_surface_local_scale`). NOW: KV9-F2 `patch triangulation folded (inverted triangle)` FaceId(21) — the unrolled patch CDT |
| R0025 | ~~Stage-4 LRR v1760~~ ~~VertexOffSurface(859)~~ ring rejected by CDT (FaceId 588) | ~~probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — torus×plane implicit-pair **Newton non-convergence** at v1760 (torus R=494/r=329, scale ~1300; siblings on the same op relocate fine). #131/N28 rim-crossing theory refuted~~ **TWO LAYERS PEELED 2026-07-28/29, both evaluation-floor defects:** (1) the pair-Newton `tau=1e-13` was below one ulp at scale ~1300 (3892080e — Newton had CONVERGED); (2) the strict-validation torus band (1e-12·minor length² ⇒ 5e-13 linear) was below the f64 evaluation floor — the "off-surface" vertex measured **8.6e-13 linear ≈ 4 ulps of its coordinates**, i.e. exact (floored via `eval_floor_linear`, with R0027). Now fails as a **ring-reject** (FaceId 588) — the ring-fold family | CONFIRMED (2026-07-29 `KV2_OFFSURF_PROBE`) | ring-reject family (re-probe with `KV2_RING_PROVENANCE` to sub-classify; was P3b-#137) |
| R0032 | Stage-4 LRR v32 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — **torus×Cone** implicit-pair Newton non-convergence (torus R=45.6/r=30.4 × cone half-angle 1.19 rad); sibling verts with cone partners relocate fine — v32's specific pair diverges | CONFIRMED (#171 pass 2) | P3b/M5-residual (torus×cone pair Newton) **2026-08-19: NOT a Newton limitation** — the torus×cone divergence was the cone step overshoot (α=1.19 rad, ratio −1.7 = 1−sec α; `YANG_PAIR_NEWTON_TRACE`) → FIXED. NOW: Stage-6 `reassembled output would be non-2-manifold` (unprobed) |
| ~~R0035~~ | ~~Stage-4 LRR v194~~ | ~~v194 is `ellipse=true + surface_pair=true + endpoint` — Ellipse endpoint also on `SurfacePair{Cylinder×Cylinder}` → surface-pair endpoint-mix STOP, R0044 class~~ **FLIPPED CORRECT 2026-07-28 (triple-block wiring):** v194/v195 have exactly 3 incident surfaces `{cyl_A, cyl_B, plane_B}` — the increment-5 conic triple junction, which had simply never counted `vert_surface_pair` as a curve-bearing map | — | ~~P3-junction~~ DONE |
| R0047 | ~~Stage-4 LRR (u32::MAX)~~ reassembled output non-2-manifold (Stage 6) | ~~probe 2026-07-17: `site=split_max_passes` — same class as R0009~~ **RE-DIAGNOSED + LAYER PEELED 2026-08-19:** the R0009 absolute-floor class exactly (2.09e-4 scale; 5168 healthy-triangle unzips in 62 s before the cap). Post-fix zero unzip actions; advances to a Stage-6 reassembly non-2-manifold wall (unprobed) | CONFIRMED (2026-08-19) | Reassembly non-2-manifold family (was P3-§4.5.2) |
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

**⚠ The `merge` column above is REFUTED (same day).** Reading the built primitive
(`stage4_update.rs:176-234`): every Fig-11 case KEEPS existing mesh vertices in place
— a boundary-vertex merge snaps the CURVE POINT onto the vertex and holds the vertex
fixed (there is a regression test at `:901` because an earlier version dragged it and
broke area conservation). `merge` never fuses two mesh vertices, so it cannot address
R0074, whose fold is two MESH vertices 9.1e-6 apart each relocated ~3e-4. And the only
near-dup removal pass (#194, TAU_WORK = 1e-12) is seven orders away; widening it is the
barred tolerance tuning. Both cases therefore need the SAME thing — the relocated curve
re-derived as a proper monotone polyline (§4.3.4) with the patch re-triangulated
(§4.4.1) — which is a HYPOTHESIS by elimination, not a measurement. Full reasoning and
the verification it needs first: epic spec §8i.

**⚠ That hypothesis is now REFUTED FOR R0074 (epic spec §8j): it has no analytic curve
to re-sample, by design.** `stage3_ssi.rs:711-718` (KV6d Tier B) deliberately skips
ANY torus edge — "a TORUS intersection edge is degree-4 … Leave it as the
`Curve::LineSegment` fallback; Stage 4 relocates its endpoints via the implicit-pair
Newton" — which is exactly why R0074 reports `n_intersection_curves=0`. Promoting to
`Curve::SurfacePair` does not help: that variant explicitly has "no closed-form
parameterization" (`geom.rs:144`), and ssi-rs's SurfacePair producers are all
quadric-based (`surface_to_quadric` refuses a torus). A monotone re-sampling needs a
parameter to be monotone in. **⇒ R0074 leaves Phase C**; giving it one needs genuine
new capability (curve tracing/marching on the implicit plane∩torus pair), adjacent to
the KV6d torus scope, not a wiring of existing parts. **R0011 is the only viable
Phase-C target** — it carries real analytic `Ellipse` curves (28/45), which do have a
closed-form parameterization, and it is the only case whose folds are 100 %
Stage-4-minted. **⚠ Superseded: the re-sample was RUN on R0011 and is a NO-OP — see
below.**

### RUN + CORRECTED — re-sample is a no-op on R0011, and the own-rim counts were a probe artifact (2026-07-29)

**The monotone re-sample was tried on R0011 and does nothing.** `YANG_S5_CHAIN` walks
each loop's maximal runs of consecutive edges sharing a bit-identical ellipse and reports
every vertex's exact `ellipse_param` in traversal order (unwrapped across the atan2 seam
first). **All 31 ellipse chains are MONOTONE** (31/31, run lengths 2–7), so re-sampling
at the same vertex count reproduces the existing order and cannot clear a fold. It could
not have anyway: **not one R0011 fold has both incident edges on an ellipse** — the 10
split Line→Ellipse ×4, Ellipse→Line ×2, Line→Line ×4, so every apex is a chain JUNCTION,
never a chain interior. The §4.3.4 hypothesis is refuted for both cases; Phase C has no
grounded lead case left here. Full detail: epic spec §8k.

**CORRECTION to the own-rim rows above — the error was in my own probe.** Increment 1
stored per-vertex incidence as `BTreeSet<String>` keyed on the operand-qualified LABEL,
collapsing two DISTINCT surfaces that share a label (a vertex on two different
`A:Plane`s) into one entry. Increment 2 changed it to `Vec<(String, Surface)>`. Same
fold, same vertices: `inc=[B:Plane | B:Plane | A:Cylinder+B:Plane]` became
`inc=[B:Plane+B:Plane+B:Plane | B:Plane+B:Plane+B:Plane | A:Cylinder+B:Plane+B:Plane]`.

| case | apex own-rim | apex is Fig-11 q | previously published |
|---|---|---|---|
| R0074 | **67/78** | **66/78** (`A:Plane+A:Plane+B:Torus`) | "0/78" ✗ |
| R0011 | **10/10** | **6/10** (`A:Cylinder+B:Plane+B:Plane`) | "0/10" ✗ |
| F0045 | 4/4 | 3/4 | 4/4 ✓ |

**This UNIFIES the bucket** rather than splitting it in three: all three cases are
dominated by Fig-11 **q** points (on one operand's own rim AND the other operand's
surface) — the F0083/v80 class — consistent with every fold sitting at a
cross-chain/own-rim junction.

**Next measurement, sharply defined.** §8h already showed these q vertices satisfy all
their surfaces to ~1e-13, so a triple-point reseat is a no-op on them. What is NOT
established is whether each sits at the **nearest** valid root: `A:Cylinder ∩
(B:Plane₁ ∩ B:Plane₂)` has up to 2 roots and a vertex can be exactly on all three
surfaces at the WRONG one. Solve both roots in closed form and compare distances to the
pre-relocation position — the same test `circle_plane_nearest_root`
(`stage4_boundary_curve.rs:290`) already encodes for inc-3's geometry.

**METRIC CAVEAT — this inflates every fold count in this section.** `turn > 120°` is a
proxy for "self-intersecting ring", not the thing itself, and conflates legitimate sharp
corners with genuine retraces. R0074's turns are bimodal: 63 in a 120–146° cluster, 15
at ≥153°, 11 at ≥177°. And kernel-v2's ring probe found **ONE** proper self-crossing in
R0011's 392-point ring, not 10. Future increments should be scored against the ring
self-crossing count (`KV2_RING_PROVENANCE`), with turn angle used only to localize.

Recorded for whoever lifts the torus boundary: a second, redundant skip fires first.
The selection-tolerance ladder (`stage3_ssi.rs:559-593`) has arms for Cylinder, Sphere
and Cone but **none for Torus**, so `tol` falls through to `TAU_WORK = 1e-12` and the
on-both gate (`:615`) rejects every torus edge before the deliberate skip is reached.
Measured (`YANG_V_PROBE=125,123,126`): `tol=1.000e-12` against `d_s=(0.000e0,
3.472e-4)` — exactly on the plane, and off the torus by its Stage-1 chord error, which
the ladder is meant to admit. Those distances match this bucket's pre-residuals to
every digit, cross-validating both probes. Harmless today (the torus arm would skip
anyway) but it must be fixed BEFORE any torus curve is implemented, and the probe's
"on-both gate SKIP" line misattributes the cause for these edges.

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
| ~~R0016~~ | ~~ring rejected by CDT (FaceId 1885)~~ | probe 2026-07-18: 646-pt micro-scale ring (r≈0.03) with **15 periodic near-dup pairs** at (i, i+2) ~1.1e-4 apart (spike/needle pattern repeating with period ~310) + 1 crossing — the #146 near-duplicate junction-vert mint materialized in an output ring | CONFIRMED (#171 pass 2) | P3a-#146 (near-dup junction mint) | **FLIPPED CORRECT 2026-08-19 (§5c.13 degeneracy identity):** its later "reassembled output would be non-2-manifold" wall was the §4.4.1(a) unzip flipping healthy sub-1e-12-area gear-tooth slivers under the absolute floor; post-fix zero unzip actions, all oracles incl. the in-line composition oracle pass |
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
| ~~C0075~~ | ~~InvalidBooleanOutput (undirected edge ≠ 2 directed)~~ **CONVERTED 2026-08-19** | ~~probe 2026-07-18: two overlapping 12-tooth gear extrudes, union, BY CONSTRUCTION — the known non-convex gear-profile capability tail~~ **The real wall was the Stage-0 split collector's exact-collinearity test dropping rounding-perturbed boundary subdivisions (4 splits on this case) → T-junctions → the unpaired-edge reject. With the identity fix the union completes and measures χ=−2 — the two interleaved gears enclose TWO through-pockets (genus 2; independently derived); the authored `euler_target: 2` was the wrong one and is corrected + pinned.** | CONVERTED | SUPPORTED_CORRECT |
| R0019 | input B-Rep not 2-manifold | probe 2026-07-17 REFUTES the chained-defect suspicion: the FIRST boolean (`op=Subtract a: 2v/3f`) rejects — operand A is a 2-vertex/3-face revolve-primitive B-Rep the yang input gate cannot accept (primitive topology vocabulary, KV6-class) | CONFIRMED (#171 sweep) | KV6/scope |
| R0053 | patch flood-fill LabelMismatch {seed 2, tri 3890} | probe 2026-07-18 (new `CHERCHI_PATCH_PROBE`): the flood from a seed labeled `[InputId(0)]` reaches tri 3890 labeled `[InputId(1)]` after 956 triangles — **genuinely DISJOINT single labels** (not the L2a compatible coplanar-sheet case). An A-only region floods into a B-only region across 2-incident MANIFOLD edges ⇒ the A×B intersection curve is missing/unsplit there — a Stage-2 arrangement incidence gap (revolve×revolve op, kin to R0050's empty-partner signature) | CONFIRMED (#171 pass 2) | Stage-2/3 arrangement incidence (near-coincident revolve surfaces, R0050 kin) **2026-08-19: DRIFTED long before this date** — the live wall was the pure surface-pair pair-Newton at `:6097` (cyl×cone, ratio −2.6 cone overshoot) → FIXED with R0032/R0044. NOW: Stage-6 `reassembled output would be non-2-manifold` (unprobed) |

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
| P3b-#137 (torus∩plane + grazing/tangency Stage-4) | 6 confirmed (C0065, R0038, R0015, R0026, R0077, R0032-torus×cone) + F0085 (open seam, R0038-type); ~~R0074~~ **re-vehicled 2026-07-29** → planar output-loop seam-overlap (the OffCurve layer is gone; it is now a ring-reject and the lead witness of that class); ~~R0025~~ **re-vehicled 2026-07-29** → ring-reject family (both its STOPs were evaluation-floor artifacts, peeled by 3892080e + the `eval_floor_linear` validation floor) |
| P3c (curved re-CDT) | **0 open** — ~~R0072~~ FLIPPED CORRECT 2026-07-28 (#195 inc-5); the vehicle has no remaining case |
| P3-junction (other junction vocabulary) | 4 confirmed (R0003, C0067, R0070-v1028, R0085-op1) + R0085-op2 (torus×line endpoint-mix). **The R0044 surface-pair endpoint-mix sub-bucket is CLOSED 2026-07-28** — ~~R0035~~ CORRECT; R0044/R0020/R0070-op2 cleared this layer and re-vehicled to their deeper causes. ~~F0045, R0011, R0028~~ **split out 2026-07-29** into the two rows below |
| **§4.4.1/§4.5.2 MESH UPDATING after relocation** (new, 2026-07-29; supersedes the same day's "partial relocation" and "seam-overlap" framings) | **3 confirmed — R0074, R0011, F0045.** Stage 4 relocates a SUBSET of a boundary chain onto the exact analytic geometry and leaves the rest at their Stage-1/2 mesh positions; the moved↔still boundary retraces as a near-180° fold, which kernel-v2's render CDT loudly rejects. **81 of 92 folds straddle that boundary; ZERO lie entirely in un-moved geometry.** Measured at yang's own emission site (`stage5_topology.rs` `push_loop`), so the mint is Stage 4, not Stage 5 ordering and not kernel-v2. **CORRECTED same day:** the relocation SET is CORRECT — un-moved fold vertices carry `Plane`-only incidence and are genuinely not on the intersection curve. The defect is that Stage 4 displaces relocated vertices by up to **101x the incident mesh edge length** (25/78 folds displace further than their shortest incident edge) **without updating the incident mesh**, which Yang §4.4.1/§4.5.2 require. These 3 cases are therefore CUSTOMERS OF THE MESH-UPDATING EPIC (#169 / N2, specs already written), not a standalone fix site. **RE-SCOPED again 2026-07-29 (anchor verification, §"SCOPED — the epic owns 16 of R0074's 78 folds"): this row is THREE classes, not one.** Operand-qualified incidence + the pre-Stage-4 turn angle show R0074 = **16 folds MINTED** by Stage 4 (turn_pre 0.00° → 179.9x°) and **62 INHERITED** from the Stage-2/3 boundary cycle (already folds before Stage 4, which perturbed them by a median 1.25°) ⇒ the epic owns 16, the rest route upstream to #146, and **R0074 cannot green from mesh-updating alone**. F0045 is instead the **Fig-11 q triple point** with a CYLINDER third surface (`A:Cylinder+A:Plane+B:Cylinder`, 4/4 apexes own-rim) — the built inc-3 `plan_triple_point_reseats` skips it because its closed form requires a `Plane`. R0011 is a third signature. The acceptance criterion is CONFIRMED but with the **pre-relocation spacing** as denominator (not the post-move incident edge): `max_disp / min_pre_spacing` is >1 for 14/16 minted (median 3.85, max **81×**) and ≤1 for 56/62 inherited (median 0.22) — it separates the populations. Blocking gap: F0045 and R0011 both take a §4.5.3 collapse, so their positional oracle is unavailable and their minting is UNMEASURED. |
| **Developable-patch ring rejects** (new, 2026-07-29) | 2 open (R0028 closure-fold, R0049 ~97-run fragmentation) — different builder (`tessellate_developable_patch`), different mints, both PARTIAL. Do NOT fold into the planar seam-overlap spec |
| M5 surface-pair Newton convergence (new, 2026-07-28) | 2 confirmed (R0044 v13, R0020) — pure `vert_surface_pair` vertices whose `relocate_onto_implicit_pair` diverges; kin to the torus `pair_newton_none` trio (R0025, R0032, R0077) |
| kernel-v2 surface-pair render band (new, 2026-07-28) | 1 confirmed (R0020, fatal) — `surface-pair refinement needs a positive finite chord tolerance` on an output `Curve::SurfacePair` edge |
| Stage-4 cone-generator LineSegment arm | **0 open** — ~~R0008~~ FLIPPED CORRECT 2026-07-28; ~~R0085-op2~~ and ~~R0081~~ resolved at this layer but still ERROR one layer deeper. The vehicle has no remaining case. **It had 3 cases, not 2** — R0081 sat under `#153 / SUSPECTED` and was found only by re-probing after the fix |
| Stage-2/3 arrangement incidence (near-coincident surfaces) | 1 confirmed (R0053; R0050 kin, counted under P3a) + **R0049 PARTIAL** (2026-07-29: ~97 adjacency runs on one 214-edge ring — fragmentation consistent with this family, mint unconfirmed, run-count caveated) |
| P3-§4.5.2 (split budget) | 4 confirmed (R0009, R0047, ~~R0063~~ **CONVERTED 2026-07-30** (provenance inc-2 — its live failure had drifted to the silent off-surface class), R0091) |
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

## The strict-validation VertexOffSurface tail is TWO classes, not one (2026-07-29)

The three cases 5b891ec2 exposed (F0083, R0027, R0099) plus R0025's post-3892080e
layer were all `VertexOffSurface` and looked like one "constructor/relocation
exactness" family. Probing each split them:

| Case | residual | linear equivalent | class | outcome |
|---|---|---|---|---|
| **R0027** | 3.725e-9 (length², torus minor 2137.7) | **9.1e-13 = 1 ulp of ρ(5344)** | validator false positive | **CONVERTED 2026-07-29** (ERROR → CORRECT, 257C) |
| **R0025** (layer 2) | 5.675e-10 (length², minor 329.5) | **8.6e-13 ≈ 4 ulps of coords ~1300** | validator false positive | layer peeled → ring-reject (see its row) |
| F0083 | 2.3046e-3 / 1.914e-3 | 3.3× / 2.76× the op chord band | REAL — unclaimed Fig-11 q + unbuilt cross curve | `specs/yang_s3_intersection_edge_provenance.md` |
| R0099 | 8.651e-2 (`cylpatch-vertex`) | **2.8% of r=3.125** | REAL — ~~F0083 family~~ **RE-DIAGNOSED 2026-07-30: Stage-0 fold-gate revert leak** (coplanar-only op, Stage 4 never runs; see the dated section below) | M8 overlay mesh-updating, multi-class cavity arm — `specs/m8_stage0_multiclass_cavity_arm.md` (inc-1/2 SHIPPED always-on 2026-07-30; residual = inc-3 region-form parity, wedge polygons NON-SIMPLE at the mints) |

**The false-positive mechanism:** the canonical strict-validation bands
(`CURVED_SURFACE_DEBUG_TOLERANCE` = 1e-12, absolute for cylinder/cone/planar-
anchor, ·minor for the length² torus form ⇒ **5e-13 linear** — the length²
convention silently HALVES the linear tolerance) sit BELOW the f64 evaluation
floor `8·ε·L` once coordinates reach L ≳ 1e3. A mathematically perfect vertex
cannot pass: the validator's own arithmetic rounds by more than the band. Fix:
`validate::eval_floor_linear` (8·ε·L, the yang-rs Newton convention) max'd into
all six canonical-band sites (torus, cyl vertex, cyl rim-center, cone with
tan(α) amplification, sphere, planar-circle-anchor). Unit-scale verdicts
unchanged (floor ~1.8e-15 ≪ every band); real defects unaffected (the tail's
smallest real residual is 1.9e-3, eight orders above the floor at its scale).
Witness regression tests: `kv6a_revolve.rs` §12 (R0027's authored revolve
verbatim — RED pre-fix, GREEN post; plus oracle-power and unit-scale guards).

**Corpus: 256C/0W/54E/0T → 257C/0W/53E/0T, exactly two deltas** (R0027
converted; R0025 error-string only). This is the 3892080e lesson at the next
gate down: **when a producer and a consumer share a metric, they must share its
floor** — Stage-4's Newton accepts at 8·ε·L, so a validator demanding 5e-13 at
L=6700 re-rejects the producer's own contract.

**The REAL-defect half (F0083/R0099) got its structural spec:**
`specs/yang_s3_intersection_edge_provenance.md` (N10's named durable target).
The §17 chain-discriminator lead is REFUTED at design review by the YR18
fixture itself — its seam ring is 45/46-exact yet must be skipped, while
F0083's chain is 1/3-exact yet must be admitted; chain health points the wrong
way, and admit-by-witness / which-surface-off each mis-handle a legitimate
vertex class. Sixth refuted discriminator on the thread, first one refuted
BEFORE building. Only the producer (the arrangement's constraint segments,
already computed in `cherchi-rs::group_constraint_segments` and enforced via
`set_edge_constr`) knows which edges are intersection-minted — Yang §4.2.3
assumes exactly this provenance.

## Provenance inc-2 SHIPPED ALWAYS-ON (2026-07-30) — +2 CORRECT (F0083, R0063), 259C/0W/51E/0T

`specs/yang_s3_intersection_edge_provenance.md` inc-2: provenance-first
classification (witness selection + single-candidate no-witness arm) plus
provenance-vouched exemptions on four Stage-4 band gates (circle, ellipse,
and both their junction loops — each destination is a closed-form point ON
the defining surfaces, a certificate the band cannot strengthen). Flip
measurement: OFF byte-identical (257C); ellipse-arms-only ON 258C (F0083);
with the circle arms **259C/0W/51E/0T — R0063 converts too** (its
6.9%-of-radius silently-off vertex, the case that motivated
strict-validation, now relocates onto its circle). Zero CORRECT→ERROR at
every step. Collateral: F0082 3→1 failing ops (Extrude-7/10 defects FIXED,
frontier = the known #130 Extrude-14 wall), F0085 2→1 (chain runs to op 20),
R0026 reclassifies to a loud Stage-3 stop.

- **~~F0083~~ CONVERTED** — the three provenance-confirmed edges the on_both
  gate refused are admitted; v118 nearest-point-relocates 1.914e-3 onto its
  ellipse; v73/v80 resolve through the `(plane∩plane)∩cylinder` junction.
- **~~R0063~~ CONVERTED** — circle-arm provenance relocation.
- **R0099 is NOT this class (measured):** its failing subtract has an
  arrangement with ZERO constraint edges and its producing boolean's 34
  confirmed edges all pass the gate — the 8.65e-2 vertex has a different,
  unprobed mint. Stays in the tail as its own row.

Permanent pins: yr18 oracle3 (closed provenance fixture — boolean succeeds,
the 2.9×-band drifted vertex is gone, its exact projection present;
stash-verified RED pre-inc-2); oracle1/2 hold the provenance-less path
byte-identical.

## R0099 producing-op probe COMPLETE (2026-07-30) — Stage-0 fold-gate REVERT leak, not the F0083 family

The 2026-07-29 "F0083 family" classification is RETRACTED — provenance
measured zero constraint edges at its failing op, and the producing-op probe
names a different mechanism entirely.

**The op:** Revolve 3's cut contacts the tube ONLY coplanarly — its θ=0 and
θ=180 profile rectangles lie exactly in the bottom-cap plane (Stage-0
cross-pairs, gaps 0 and 1.8e-15); everything else of B is outside and
dropped (`YANG_KEEP_PROBE`: 480 A tris kept, 308 B dropped, 52 shared-sheet
tris). Zero transversal intersections ⇒ `has_conic = false` ⇒ **Stage 4
never runs on this op** — no rim-snap, no relocation, nothing downstream
can rescue a mis-positioned vertex.

**The mint (`YANG_SPLIT_PROBE`, exact-position match to the failing loop):**
Stage-0's overlay mints exact on-rim-circle vertices where the wedge
rectangles cross the cap's rim chords (the rectangle u-extent ±3.1205 vs
r=3.1251 — crossings hug the rim). Moving those mints chord→circle folds
local overlay slivers; the repair ladder then fails end to end: flips
constraint-blocked (`class-boundary` / `domain-boundary` / `replacements
invalid`), amendment-5 cavity relocation rejects — **`multi-class cavity
with constraint-blocked fan`** (verts 4/9/116/153/182…) and `cavity polygon
not simple` (vert 120) — and amendment-2 REVERTS the mints to chord lifts,
deliberately loud ("observable via kernel-v2's vertex-on-surface tripwire —
never silently blessed"). `[fold-revert] vert=9 … -> chord (-2.24898,
-7.43299, 8.03287)` IS the `VertexOffSurface(18)` point digit-for-digit;
vert 120 is the loop's i7. Three reverted mints survive into face 18's
boundary at 6.1e-2/8.7e-2/9.1e-2 inside the cylinder (chord depths of the
13-gon rim).

**Vehicle: M8 Stage-0 overlay mesh-updating — the MULTI-CLASS cavity arm.**
The mints sit ON the intersection curve (class boundary), so their cavity
spans ≥2 region classes and a single-class re-fan is constraint-blocked by
construction. The amendment-5/6 machinery needs the two-sided form: carve
per class along the constraint polyline, move the boundary WITH the mint,
re-fan each side (Yang Fig 11 at the overlay level — same theme as #169).
Kin to, but distinct from, the transversal provenance relocation that
converted F0083/R0063 — that path never sees this op (no arrangement
constraints exist to vouch for anything).

Probe banked: `nary-fold-revert` (under `YANG_COPLANAR_PROBE`) mirrors the
1×1 path's `[fold-revert]` (under `YANG_SPLIT_PROBE`) so both fold-gate
leak sites are now observable; a corpus census of this class is one
env-gated full run away.

Spec written 2026-07-30: **`specs/m8_stage0_multiclass_cavity_arm.md`**
(amendment 12 — per-class WEDGE decomposition of the deferred cavity, the
two-sided Fig-11 form at the overlay level; the census above is its inc-0).

**inc-0 census COMPLETE same day (spec §7): 19 revert cases — 10 ERROR
(R0099 the only PROVEN revert-caused one; the rest name varied walls,
several with existing vehicles) + 9 SUPPORTED_CORRECT carrying 63
chord-lift revert events that pass every current check (the latent class —
includes freshly-converted R0063/R0072/R0021). ZERO n-ary events (inc-4
deferred, no customers). The `interior vertex` reject arm out-weighs
`multi-class` 616:139 event-wise — inc-1 instruments the class-transition
count at that reject site to measure the wedge arm's true coverage.
Sweep method: parallel `single_case` subprocesses with stderr captured
(the ASSAY_JOBS driver nulls child stderr); 312/312, zero verdict drift.**

**inc-1 + inc-2 SHIPPED 2026-07-30 (spec §8): the wedge arm is ALWAYS-ON.**
Corpus OFF/ON measured ZERO category changes (259C/0W/51E/0T both ways;
6 ERROR cases shifted detail only — R0025/R0026 now share an
`input B-Rep is not 2-manifold` signature worth an inc-3-era look). The
§7.2 transition census: interior rejects are **100% 2-transition**
(428/428 across R0099/R0085/F0067/C0048/R0088) — the arm structurally
covers the whole dominant class. **R0099 remains ERROR**: its folded wedge
polygons are NON-SIMPLE (interacting mints — a neighbor mint's collapsed
chord passes through v's minted position), the NonSimple propagation now
ARMS the amendment-6 joint path (R0085: region commits 0→6, fold-reverts
65→10), and the REGION form's guards are the residual wall (`crossing
edges ungrowable` / `region too small`) — **inc-3 region-form parity is
R0099's named vehicle** (census gate armed, spec §4/§8; conversion pin
`kernel-v2/tests/m8_r0099_multiclass_chain.rs` quarantined on it).**

**Amendment-13 inc-3.5/3.6 SHIPPED 2026-07-30 (spec §10d): the Fig-11
MERGE arm is ALWAYS-ON, protected by the rim-chain boundary-order settle
check.** The R0059 no-flip counterexample was anchored to a BOUNDARY-ORDER
inversion, not a guard gap: a kept junction mint (circle∩line — azimuthally
displaced from its chord anchor by up to the snap displacement) leapt past
a fold-reverted neighbor, so the cap overlay (chord-parameter order) and
the ring builder (azimuth order — the revolved lateral's arc-length
parameter) desynchronized on the shared rim chain. The settle check runs at
gate quiescence, reverts the displaced member of an exactly-inverted pair,
restores merge partners (merges now propagate through the revert path), and
re-runs the ladder; `collect_edge_splits` collapses a surviving merge's
duplicate same-position entries. Corpus flip: zero CORRECT→ERROR; new
canonical **259C/0W/49E/0T** — F0067 + F0072 recategorized ERROR →
UNSUPPORTED(coplanar-boolean) (their repaired meshes now reach the loud
typed M8 wall). The same-order inversion class existed MERGE-FREE in
canonical outputs (R0059 op 002, absorbed downstream by luck) and is now
policed everywhere. R0099 unchanged (merges fire, settle discriminates);
its conversion still awaits the inc-3.2 vertex-inserting split + inc-3
region-form parity.

**Amendment-14 SHIPPED 2026-07-30 (spec §11): the Fig-11(a)
vertex-inserting SPLIT is ALWAYS-ON and R0099 is CONVERTED — the corpus
flip's only category change was R0099 ERROR → SUPPORTED_CORRECT (i6
seam-free; new canonical 260C/0W/48E/0T).** Vert 9's true repair: A's
rim circle near-tangent to B's real wedge edge (bulge 2.23e-4 over a
4.2e-3 span, crossings at t_chain 0.9955/0.0026 — the Fig-11(b)
too-close form with BOTH merge directions constraint-deadlocked), so
q_a/q_b are minted with exact rational UVs ON B's edge (the
`collect_edge_splits` leg rides for free), the mint's star re-cuts into
side remnants + material (closed along C's sub-segment — the 2-incident
AOnly|Overlap boundary) + the AOnly bulge fanning the mint alone
(count = cavity + 1), and the subdivided chain propagates into A's rim
chains via the `ExtraRimPoint` side-channel with an unconsumed-extras
loud pair-fail. The build's double-cover misstep (material closed
THROUGH the mint) was caught by the i6 fwd/rev oracle before any
corpus exposure — the emission-side invariant again. The chain
fixture's strict `v3 < v2` oracle was corrected by deriving op 3 as a
MEASURE-ZERO cut (the 36.31° unswept sector's uncovered bound
tan(36.31°)·ρ⊥ ≥ 4.58 exceeds the tube height 1.95): true Δv = 0 and
the +0.069% is chord→circle mint volume restored toward the analytic.
The R0099 row above is CLOSED; inc-3 region-form parity keeps its
census gate but has lost its lead customer — re-census before building.

**inc-3 region-form parity RE-CENSUS (2026-07-30, post-amendment-14,
spec §12): NO PROVEN CUSTOMER — DEMOTED to census-armed.** 312-case
probe sweep, zero verdict drift vs the 260C canonical. R0099 (the only
proven revert-caused ERROR) is converted with ZERO residual reverts;
every remaining ERROR carrier of region rejects fails at a wall named
elsewhere (C0048 #144 azimuth-merge, F0064 TessellationFailed
collapsed-planar-tri, R0085 TessellationFailed CDT-ring, R0050 Stage-4
relocation, R0026 Stage-3 AmbiguousCurve, R0051 SelfIntersecting).
The split arm found two in-chain customers beyond R0099 (F0064 genuine;
C0048 femto-endpoint, watch); its reject census names the future §11
extension classes (open-link 74, crossing-count 48, class-pair 18,
chord-not-boundary 18). Latent chord-lift watch list: 5 CORRECT cases /
25 revert events (R0059 14). Next anchors in evidence order: F0064's
collapsed-planar-triangle, C0048's #144 family, split-open-link.

## Stage-4 STOP site census (2026-08-19, post-I5-2 flip) — permanent instrument + the absolute-floor anchor

Instrument: every `Stage4RegionInvalid` is now built through the
`#[track_caller]` constructor `YangError::stage4_region_invalid` (`errors.rs`);
under `YANG_LRR_PROBE` each construction prints `YANG_LRR_SITE
loc=<file>:<line> reason=… v=…`. Eager `.ok_or(YangError::…)` constructions
were converted to `.ok_or_else` so a printed site is a STOP that fired (the
first census run had 23–250 phantom sites per case from eager `ok_or`
arguments — the LAST line is the fatal one, but with lazy construction there
is exactly one). Line numbers are as of commit `a1adca26`+this change.

| Case | reason | site | mechanism (from the site's code) |
|---|---|---|---|
| R0009 | LRR u32::MAX | `stage4_correct.rs` (3d) unzip `split_max_passes` | ABSOLUTE area floor mis-firing at 1e-4 scale → FIXED (§5c.13); advances to double-cover |
| R0047 | LRR u32::MAX | same | same → FIXED; advances to Stage-6 non-2-manifold |
| R0038 | LRR u32::MAX | (3d) unzip `degenerate_no_longedge` give-up (`:7333`) | tangency (spec §5c.10, unchanged) |
| R0032 | LRR v10 | torus block, `[s1]` arm pair-Newton `None` (`:6190`) | torus×cone implicit-pair Newton non-convergence (ledger row unchanged) |
| R0044 | LRR v16 | pure surface-pair `relocate_onto_implicit_pair` `None` (`:6097`) | M5 surface-pair Newton (ledger row unchanged; vertex id drifted 22→16) |
| R0053 | LRR v0 | same site `:6097` | pure surface-pair Newton divergence — **NEW attribution** (ledger row said Stage-2 patch flood-fill / near-coincident revolve incidence; re-probe before building) |
| R0050 | LRR v125 | torus block partner set (`:6172`) | near-coincident revolve tori, empty/degenerate partner set (ledger row unchanged) |
| C0067 | LRR v128 | M8 disc∩disc circle×circle junction (`:5211`) | `coplanar_circle_circle_intersection` → None (ledger row unchanged) |
| R0003 | OffCurve v10583 | cone-ellipse loop residual (`:5801`) | multi-map over-band chain (ledger row unchanged; vertex 4233→10583 is the same chain) |
| R0015 | OffCurve v82 | torus bounded-face containment (`:6393`) | C0065 containment class, micro scale (unchanged) |
| C0065 | OffCurve v3 | same `:6393` | grazing loop outside the box face (unchanged) |
| R0028 | OffCurve v64 | torus corridor gate (`:6284`) | developable cap overshoot (2026-08-04 anchor) |
| R0077 | OffCurve v154 | same `:6284` | torus×plane at extreme scale (unchanged) |

Also measured (spec §5c.13): the 12 micro-scale CORRECT cases (scale
< 2e-3) — R0091 ×1, R0072 ×6, R0063 ×68 healthy-triangle unzips under the
old floor (silent edge flips inside CORRECT verdicts; gone post-fix, all
remain CORRECT); R0016 (5e-2 gear) ERROR→CORRECT.

## CDT ring-reject fold census (2026-08-19d) — every fold is MINTED_BY_S4

`YANG_S6_LOOP_SIMPLICITY` + `YANG_S5_FOLD_PROBE` over the nine `ring rejected by
CDT` cases, then the §4.4.1 Fig-11 merge selector
(`stage4_fold_risk::fold_merge_sites`) over the same set. Spec:
`specs/yang_441_trim_cdt_construction.md` §4-I6.

**Family-level result: `cross_inherited = 0`.** Every non-simple output loop the
planar scan can measure is `class=MINTED_BY_S4` with `cross_pre=0` — the loops
were simple before Stage 4 and are not after it. (R0053 and R0044's third op
fail on CURVED patches the planar scan does not cover, and are counted as
unmeasured, not as absent.)

| Case | minted loops | Fig-11 sites | outcome (gate ON) |
|---|---|---|---|
| **F0045** | 1 | 1 — `v71→v68`, `chord_t = −0.0920` | **ERROR → SUPPORTED_CORRECT** |
| **R0090** | 1 | 1 — `v41→v28`, `chord_t = +1.0289` | **ERROR → SUPPORTED_CORRECT** |
| R0011 | 6 | 1 | `FanNotSimple` on holder 186 — pinched victim |
| R0074 | 2 | 1 | `FanNotSimple` on holder 163 |
| R0085 | 40 | 1 per op | `FanNotSimple` (op1), fan-polygon `Cdt` (op2) |
| R0044 | 6 | 13 | every holder set includes a cone/torus patch (I2a scope) |
| R0025 | 3 | 0 | all 4 inversions are `apex_moved` |
| R0095 | 5 | 0 | all 20 inversions are `apex_moved` |
| R0053 | — (curved) | 0 | all 83 inversions are `apex_moved` |

**The mechanism, stated once (F0045 is the clean witness).** The arrangement puts
a crossing vertex where the two INSCRIBED meshes cross; Stage 4 relocates it onto
the exact analytic junction, which — because an inscribed polygon is smaller than
its circle — generally lies on the FAR side of the neighbouring rim grid vertex.
The relocation steps over its own neighbour, and the kept patch's boundary walks
out and back over it. F0045: junction moves 2.382e-2 across a 1.283e-2 spacing;
the neighbour's turn goes 27.69° (exactly the rim's 360/13 grid step) → 167.34°.
This is Yang Fig-11 verbatim, reached from the other side, and its remedy is
Fig-11(b)→(c): merge the overrun vertex into the relocated one.

**The residue is TWO defects, not one.** Every rejected inversion has an apex
that genuinely moved (`apex_minted = 0` family-wide). Split by whether BOTH of
the apex's incident cycle edges are intersection-curve edges:

* **ON-CURVE — R0044 (163/188, 96/109), R0053 (62/83, 12/12), R0095 (13/20):**
  two vertices of the SAME chain crossed each other = chain ORDER, owned by
  §4.3.4's `ReorderConic` (I2b). Their curves are `Hyperbola`/`SurfacePair`,
  which I5-1b already records as staying per-segment.
* **OFF-CURVE — R0011, R0025, R0074, R0085 (100 % of their inversions):** a
  RELOCATED vertex crossed a neighbour on a PLAIN boundary. **Anchored
  2026-08-19e (spec §4-I7) and it is TWO classes, both now owned.** The
  relocation itself is correct — the torus arm's gate
  (`tangent_plane_corridor(d_eps, sinθ)`) is satisfied with room to spare
  (worst ratios 0.69/0.32/0.23, sinθ 0.90–1.00) — but `d_eps` is 27–1000× the
  local segment there, so a move well inside the OFF-CURVE budget is still many
  local edges long. Splitting by displacement over the corner's own shorter
  edge:

  | case | corners | median | max | >2× | >10× |
  |---|---|---|---|---|---|
  | R0011 | 6 | 1.48 | 3.01 | 2 | 0 |
  | R0025 | 8 | 3.13 | 8.73 | 4 | 0 |
  | R0074 | 36 | 2.26 | 101.43 | 19 | 5 |
  | R0085 | 174 | 6.07 | 1737.00 | 144 | 74 |

  F0045 (the I6 case that CONVERTED) sits at 1.86. So **LOCAL** (R0011, most of
  R0074) is absorbable by a merge and needs only a survivor rule for the
  both-moved case (surface-incidence richness, the KV15b I1b rule); **GROSS**
  (R0085 median 6.07, 42 % beyond 10×) is §4.5.2 local refinement's own trigger
  — roadmap item 4, not an unowned class.

  Two hypotheses were retracted by measurement en route: the `(2s)` surface-pair
  arm's missing gate (probe fires 0 times — wrong arm) and a ballooning
  near-tangency corridor (sinθ is 0.90–1.00).

## Stage-6 non-2-manifold site census (2026-08-19, post-5c.13) — the second absolute-floor anchor

`NONMANIFOLD_SITE_PROBE` over the nine `reassembled output would be
non-2-manifold` cases (the largest remaining ERROR family):

| Case | site | mechanism |
|---|---|---|
| F0058 | `s4-shell-euler` double-cover χ=3, edge (1,30) on A cyl-2, four tris (two per x-side, apexes z=±0.0285) — preceded by `s6-wedge-walk-not-outgoing` at v30 | equal-R perpendicular cyl−cyl CUT: v30 = (0,−0.2,0) is the exact tangency point where A's seam passes; the kept upper/lower sheets both fan onto the LOWER seam segment (1,30) — the vertex-pinch construction defect (`yang_tangency_pinch_split.md` sibling class) |
| F0060 | `s4-shell-euler` double-cover χ=3 on both cap planes (z=±0.3) along the line x=0 | B (r=0.3, axis y through the origin) is TANGENT to both caps of A along a line — a line-pinch solid (two half-wedges per cap touching along the tangent line); not 2-manifold-representable |
| R0032 | `s4-shell-euler` double-cover χ=3, edges (450,452)/(450,717) torus A ×2 + cones B191/B192 | torus × two-cone junction double cover (#146 family) |
| C0107 / C0108 | `s6-curved-empty-cycles: face 0` | designed 0D point-tangency (7b); loud reject IS the designed green |
| C0058 | `s6-curved-degenerate-loop` face 2 cycle len 64, ratio 5.9e-16 | the tangency-neck figure-eight (unchanged; §4.3.3 tangent-point insertion milestone) |
| **R0047** | `s6-curved-degenerate-loop` face 367 cycle len 4, `\|N\|=4.9e-13` | **ABSOLUTE `MIN_FEATURE_SIZE²` Newell floor at 2.09e-4 scale on a HEALTHY 2.3e-6 × 1.2e-7 quad (ratio 8.6e-2) → FIXED (spec §5c.14, four gates moved to the identity); advanced to kernel-v2 `output ellipse-arc endpoint does not lie on its ellipse` (1.109e-9 vs 1e-9 band = 4.8e-6 RELATIVE off) → ANCHORED + FIXED same day: the Stage-6 KV15b sub-resolution collapse merged a CERTIFIED plane∩cone₁∩cone₂ crease junction (3 surfaces) into its cone₁∩plane neighbour (2 surfaces) at 5.3e-8, and the I1b "adopt the richer endpoint's coordinates" rule counted PLANES only (1–1 tie → survivor kept its own position, off cone₂'s ellipse). Generalized to surface-incidence (`kv15b_mint_site_subresolution_collapse.md` I1b-curved; pin `kv15b_i1b_adopts_surface_incidence_richer_junction_coordinates`, red-verified). Op 2 now emits every conic endpoint on-curve (`YANG_OUT_INCIDENCE_PROBE` 0 hits); then op 3's kernel-v2 `to_yang` wall — a 4-edge CONE lateral `[HyperbolaArc, Line, EllipseArc, Line]` (FaceId 499) fell to the typed pattern wall because the Slice-D/E CDT re-entry routed only non-4-edge/holed laterals — FIXED (routed by PATTERN, `four_edge_structured`; pin `four_edge_non_structured_cone_lateral_reenters`). **R0047 ERROR → SUPPORTED_CORRECT; corpus 262C/0W/46E/1EE/0T NEW CANONICAL.** |
| C0044 | `i6-edge-overuse` (14,15) fwd=1 rev=0 → `s4-halfedge-pairing` | M8 flush annular stack (Stage-0 coplanar family, unchanged) |
| R0053 | `i6-input-overuse`: input B edge (180,181) fwd=0 rev=1 — the STAGE-0 mesh of B (the FRESH gear revolve, not the chained body) is not conformal | ANCHORED (enriched probe: owning faces via the Stage-0 `tri_face` map): B's planar end cap f0 (448-gon) was overlay-triangulated with its boundary edge (180,181) subdivided at overlay vertex 1469 while the adjacent cone flank f270 kept the whole edge — `collect_edge_splits`' EXACT 2D collinearity test dropped the split at an 8.4e-16 rounding miss (`YANG_SPLIT_PROBE` census: 522 misses ≤1e-13 vs 216 ≥1e-4, nothing between). FIXED: a side-region BOUNDARY vertex collinear to the scale-free identity registers (spec `m8_stage0_inputcheck_clean_emission.md` addendum 2026-08-19; pin red-verified). Advances to kernel-v2 render-CDT `ring rejected` (FaceId 474). Side effect: **C0075 completed for the first time and exposed its authored `euler_target: 2` as wrong — the two interleaved gears enclose TWO pockets (genus 2, χ=−2, independently derived); meta corrected, pinned in `historical_authoring_fixes_pinned`; C0075 ERROR → SUPPORTED_CORRECT.** |

