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
| R0044 | Stage-4 LRR v11 | ~~torus×torus (N52)~~ **RE-DIAGNOSED (probe 2026-07-17, #172):** v11 is a cylinder×cone `SurfacePair` endpoint that is ALSO a conic endpoint — the surface-pair endpoint-mix STOP (`stage4_correct.rs` `vert_surface_pair` loop), a mixed conic×degree-4 junction, NOT torus×torus. Needs a junction relocation onto the shared surface set | CONFIRMED (#172 probe) | P3-junction |
| ~~R0096~~ | ~~Stage-4 LRR v7~~ | ~~torus×torus~~ **FLIPPED CORRECT 2026-07-17 (#172):** torus×torus lateral∩lateral + torus×torus×plane junctions now relocate via the implicit-pair/triple Newton (torus-block scope lift) | — | ~~P2-M5~~ DONE |
| R0038 | Stage-4 LRR (u32::MAX) | plane tangent to cylinder along one generator; degree-2 gate self-validates (`bad_degree=[(18,4),(19,4)]`) — near-tangency pinch, NOT a CDT ring | CONFIRMED (#168 WIP4, 9f4cb604) | P3b-#137 |
| ~~R0072~~ | ~~Stage-4 LRR (u32::MAX)~~ | ~~real ~1e-7 micro-scale edge (0.4% span); force-merge is the R0091 silent-wrong trap — needs curved re-CDT~~ **FLIPPED CORRECT 2026-07-28 (#195 inc-5):** the §4.5.4 detect-then-refine rim boost + §4.4.1 rim-snap, both now always-on, resolve it WITHOUT a curved re-CDT — the micro-scale edge was an under-sampled rim, not an irreducible feature | — | ~~P3c~~ DONE |
| C0058 | non-2-manifold (reassembly) | probe 2026-07-17: `NONMANIFOLD_SITE s6-curved-degenerate-loop` — Stage-6 curved face 2 emits a 64-vertex loop with \|Newell N\| = 2.3e-16 (degenerate junction loop) | CONFIRMED (#171 sweep) | P3a-#146 |
| C0067 | Stage-4 LRR v128 | probe 2026-07-18 (#171 pass 2): v128 is a **circle×circle junction** (`circle_junction=true`, endpoint) — two sphere-section Circles (both r=0.371, centers [0.15,0,0.5]/[0,0.15,0.5], normals x̂/ŷ) meet at [0.15,0.15,0.83]; junction relocation region invalid. Needs two-curve junction relocation (mint-once contract) | CONFIRMED (#171 pass 2) | P3-junction |
| ~~R0008~~ | ~~Stage-4 LRR v42~~ | ~~probe 2026-07-18: `YANG_LRR_SITE site=lineseg_combo` edge (42,43) — LineSegment edge whose incidence is **Cone(A, half-angle 1.5525 rad ≈ 88.9°, near-flat) × Plane(B)**; the Stage-4 LineSegment arm has closed forms only for cyl×plane / cyl∥cyl / plane×plane — the **cone-generator line closed form is missing**~~ **FLIPPED CORRECT 2026-07-28 (cone-generator arm):** the closed form was never missing — `ssi_rs::plane_cone` has emitted `SsiCurve::Line` for through-apex cuts all along and Stage 3 already banded them via `cone_chord_tol_for_owner`. TWO wiring gaps, both in Stage 4: (a) the LineSegment pair match classified `Cone` as `other_curved` → STOP before selection; (b) once admitted, the tie-break called the R0072-only `select_disjoint_parallel_line`, whose parallelism precheck rejects the two CROSSING apex generators (`AmbiguousCurve{2,2}`). **#163/N45 was not a "residual theory" — it was CORRECT and already shipped, at Stage 3 only**; the two stages had been running different tie-breaks since 9fca8393 | — | ~~Stage-4 cone-generator LineSegment arm~~ DONE |
| R0009 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — the chord-split loop exhausts its pass budget (§4.5.2 refinement demand, non-convergent) | CONFIRMED (#171 sweep) | P3-§4.5.2 |
| R0020 | Stage-4 LRR v44 | probe 2026-07-18: v44 is `ellipse=true + surface_pair=true + endpoint` — a conic (cyl×plane Ellipse) endpoint that is ALSO on a `SurfacePair{Cylinder×Cone}` curve → the **surface-pair endpoint-mix STOP** (`stage4_correct.rs` vert_surface_pair loop) — the R0044 class exactly | CONFIRMED (#171 pass 2) | P3-junction (R0044 class) |
| R0025 | Stage-4 LRR v1760 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — torus×plane implicit-pair **Newton non-convergence** at v1760 (torus R=494/r=329, scale ~1300; siblings on the same op relocate fine). #131/N28 rim-crossing theory refuted | CONFIRMED (#171 pass 2) | P3b-#137 (torus∩plane relocation family) |
| R0032 | Stage-4 LRR v32 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — **torus×Cone** implicit-pair Newton non-convergence (torus R=45.6/r=30.4 × cone half-angle 1.19 rad); sibling verts with cone partners relocate fine — v32's specific pair diverges | CONFIRMED (#171 pass 2) | P3b/M5-residual (torus×cone pair Newton) |
| R0035 | Stage-4 LRR v194 | probe 2026-07-18: v194 is `ellipse=true + surface_pair=true + endpoint` — Ellipse endpoint also on `SurfacePair{Cylinder×Cylinder}` (unequal radii 0.748/0.577) → surface-pair endpoint-mix STOP, R0044 class | CONFIRMED (#171 pass 2) | P3-junction (R0044 class) |
| R0047 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — same class as R0009 | CONFIRMED (#171 sweep) | P3-§4.5.2 |
| R0049 | non-2-manifold (reassembly) | probe 2026-07-17: `s6-planar-loop-nonplanar` face 134 vert 337 off-plane 1.449e-6 (band 1.0e-7) — the F0064 class (N51) | CONFIRMED (#171 sweep) | P3a-#146 |
| R0050 | Stage-4 LRR v58 | probe 2026-07-18: `YANG_TORUS_STOP site=gt2_partners` with **partners=[] (EMPTY)** — v58 (and v362 on the sibling torus) sit on torus intersection edges whose incidence records only ONE distinct surface (the base torus itself); the model has two near-identical revolve tori (R=3.95/r=2.63 vs R=3.78/r=2.52) — a Stage-2/3 **incidence gap between near-coincident revolve surfaces** (no partner to relocate onto). #131/N28 theory refuted | CONFIRMED (#171 pass 2) | P3a-#146 / Stage-2/3 incidence (near-coincident surfaces) |
| R0063 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — same class as R0009 (the #145 zigzag residual resolves into the split-budget class) | CONFIRMED (#171 sweep) | P3-§4.5.2 |
| R0077 | Stage-4 LRR v3 | probe 2026-07-18: `YANG_TORUS_STOP site=pair_newton_none` — torus×plane implicit-pair Newton non-convergence at extreme scale (torus R=2051/r=1367, coords ~2700; the op's other two torus verts converge with rho ≈ 2e-13). Same class as R0025 | CONFIRMED (#171 pass 2) | P3b-#137 (torus∩plane relocation family) |
| R0091 | Stage-4 LRR (u32::MAX) | probe 2026-07-17: `site=split_max_passes` — same class as R0009; STILL the historical silent-wrong trap: any fix must be re-CDT/refinement, never a merge | CONFIRMED (#171 sweep) | P3-§4.5.2 |

### OffCurveBeyondChordBand (6)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0065 | Stage-4 OffCurve v8 | torus∩plane grazing loop reaches \|y\|=0.384 outside the box face; needs exact triple-junction corner insert + stitch (primitive proven, N-137.1) | CONFIRMED (#137 spec) | P3b-#137 |
| R0074 | Stage-4 OffCurve v89 | torus∩plane grazing — same class as C0065 | CONFIRMED (#137) | P3b-#137 |
| R0003 | Stage-4 OffCurve v4233 | multi-map over-band chain (v4233→v8508); needs ellipse×hyperbola junction handling, band-fixing exhausted (N45/N46) | CONFIRMED (N51/N52) | P3-junction |
| R0015 | Stage-4 OffCurve v84 | probe 2026-07-18: N51 "no-curve-type" REFUTED — v84 IS in the torus map (`torus=true`); `YANG_TORUS_PROBE` shows the pair Newton relocates it EXACTLY (rho=0, F_torus(proj)=0) and it passes the displacement gate, so the STOP is the **bounded-face containment** check below the gate (`stage4_correct.rs:4225`) — the C0065 grazing-loop-outside-face signature, at MICRO scale (torus R=5.97e-5/r=3.98e-5, coords ~1e-4) | CONFIRMED (#171 pass 2) | P3b-#137 (C0065 containment class, micro-scale) |
| R0026 | Stage-4 OffCurve v218 | probe 2026-07-18: same as R0015 — v218 `torus=true`, pair Newton rho=9.65e-6 ≪ gate 3.0e-3, then bounded-face containment STOP; micro torus∩plane (R=0.0214/r=0.0143) | CONFIRMED (#171 pass 2) | P3b-#137 (C0065 containment class, micro-scale) |
| R0070 | Stage-4 OffCurve v1028 (+op2 LRR v47) | probe 2026-07-18: v1028 sits on a micro Ellipse edge (1025,1028; major_r 0.028) AND a LineSegment edge (1028,1029) — an ellipse∩line conic junction endpoint whose ellipse relocation lands beyond band at micro scale. **op2 v47** is `line=true + surface_pair=true + endpoint` — the surface-pair endpoint-mix STOP (SurfacePair{cyl×cyl} r=0.0228/0.0069), R0044 class | CONFIRMED (#171 pass 2) | P3-junction (both halves) |

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
required (F0045 fails at its FIRST boolean). The mint is the boolean's own
Stage-5/6 output-ring assembly; two signatures: sample-misorder
zigzags/folds and #146 near-dup spikes (see per-row evidence).

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0045 | ring rejected by CDT (FaceId 9) | probe 2026-07-18 (`KV2_RING_REJECT_PROBE` + polygon analysis): 21-pt ring with ONE proper self-crossing (segs 10-11 × 12-13) — a fine-sampled arc that doubles back over itself via a coarse return chain (two different samplings of overlapping curve sections in one ring). **NOT chained-input**: F0045 is two primitive extrudes (parallel cyl boss+boss, gen.rs F0041-45) and this is the FIRST boolean — the defective ring is minted by THIS boolean's own Stage-5/6 output emission, then rejected by kernel-v2 render CDT (loud, correct) | CONFIRMED (#171 pass 2) | P3-junction (S5/S6 output-ring assembly; refutes the "chained casualties" header) |
| R0011 | ring rejected by CDT (FaceId 407) | probe 2026-07-18: 398-pt ring at scale ~2900 with 3 LOCAL zigzag crossings (each within a 4-index window: 23-27, 28-32, 390-394) — the #145 sample-misorder signature surviving in an output ring | CONFIRMED (#171 pass 2) | P3-junction (S5/S6 output-ring ordering, #145 residual class) |
| R0016 | ring rejected by CDT (FaceId 1885) | probe 2026-07-18: 646-pt micro-scale ring (r≈0.03) with **15 periodic near-dup pairs** at (i, i+2) ~1.1e-4 apart (spike/needle pattern repeating with period ~310) + 1 crossing — the #146 near-duplicate junction-vert mint materialized in an output ring | CONFIRMED (#171 pass 2) | P3a-#146 (near-dup junction mint) |
| R0028 | ring rejected by CDT (FaceId 32) | probe 2026-07-18: 146-pt ring, 2 crossings at the ring CLOSURE (segs 1×142, 4×138) — the chain tail folds back over the start (overlapping closure, not a mid-ring zigzag) | CONFIRMED (#171 pass 2) | P3-junction (S5/S6 output-ring closure) |
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
| R0004 | RevolveAxisIntersectsProfile | self-intersecting revolve — capability boundary | CONFIRMED (scope) | KV6/scope |

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
  (a) surface-pair endpoint-mix (R0020, R0035, R0070-op2 — all `conic +
  surface_pair + endpoint`, the exact R0044 class; now a **4-case bucket**);
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
| P3a-#146 (junction mint / incidence) | 9 open (C0058, R0049, F0082, C0044, F0058, F0060, R0016, R0050, R0081) + F0064 partial + R0051 suspected; ~~R0095~~ FLIPPED CORRECT (#195 inc-5) |
| P3b-#137 (torus∩plane + grazing/tangency Stage-4) | 8 confirmed (C0065, R0074, R0038, R0015, R0026, R0025, R0077, R0032-torus×cone) + F0085 (open seam, R0038-type) |
| P3c (curved re-CDT) | **0 open** — ~~R0072~~ FLIPPED CORRECT 2026-07-28 (#195 inc-5); the vehicle has no remaining case |
| P3-junction (other junction vocabulary) | 8 confirmed (R0003, R0044, C0067, R0020, R0035, R0070, F0045, R0011, R0028, R0085-op1 — last four are S5/S6 output-ring assembly) + R0085-op2 (torus×line endpoint-mix, re-vehicled here 2026-07-28) |
| Stage-4 cone-generator LineSegment arm | **0 open** — ~~R0008~~ FLIPPED CORRECT 2026-07-28; ~~R0085-op2~~ and ~~R0081~~ resolved at this layer but still ERROR one layer deeper. The vehicle has no remaining case. **It had 3 cases, not 2** — R0081 sat under `#153 / SUSPECTED` and was found only by re-probing after the fix |
| Stage-2/3 arrangement incidence (near-coincident surfaces) | 1 confirmed (R0053; R0050 kin, counted under P3a) |
| P3-§4.5.2 (split budget) | 4 confirmed (R0009, R0047, R0063, R0091) |
| M8 residue | 4 confirmed (R0007, R0071, C0048, F0067) |
| #153 NonPlanarFace | 2 confirmed (~~R0081~~ re-diagnosed 2026-07-28 → P3a-#146; it was never a #153 case) |
| kernel-v2 KV9-F2 folded ear-clip | 2 confirmed (R0017, R0100) |
| KV6/scope + designed degeneracies | 7 (C0063, R0004, R0019, C0043, C0056, C0046, C0075 — the last four sign-off candidates) |
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
