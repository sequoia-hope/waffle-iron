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
| R0044 | Stage-4 LRR v11 | torus×torus lateral∩lateral — missing degree-4 SSI curve (STOP at `stage4_correct.rs` torus×torus gate) | CONFIRMED (N52) | P2-M5 |
| R0096 | Stage-4 LRR v7 | torus×torus — same as R0044 | CONFIRMED (N52) | P2-M5 |
| R0038 | Stage-4 LRR (u32::MAX) | plane tangent to cylinder along one generator; degree-2 gate self-validates (`bad_degree=[(18,4),(19,4)]`) — near-tangency pinch, NOT a CDT ring | CONFIRMED (#168 WIP4, 9f4cb604) | P3b-#137 |
| R0072 | Stage-4 LRR (u32::MAX) | real ~1e-7 micro-scale edge (0.4% span); force-merge is the R0091 silent-wrong trap — needs curved re-CDT | CONFIRMED (N55) | P3c |
| C0058 | Stage-4 LRR (u32::MAX) | fails downstream of the Stage-4 gate per #169 Phase-0 triage; specific site unknown | PROBE | PROBE |
| C0067 | Stage-4 LRR v128 | on-axis-sphere revolve (KV6d) boolean; revolve itself landed (#136), boolean relocation region invalid | PROBE | PROBE |
| R0008 | Stage-4 LRR v42 | cone-apex crossing-generator tie-break shipped (#163/N45) but a residual region remains | PROBE | PROBE |
| R0009 | Stage-4 LRR (u32::MAX) | previously InvalidBooleanOutput (N52 census); error class CHANGED since — re-diagnose | PROBE | PROBE |
| R0020 | Stage-4 LRR v44 | — | PROBE | PROBE |
| R0025 | Stage-4 LRR v1760 | torus-profile rim crossing shipped (#131/N28); residual region | PROBE | PROBE |
| R0032 | Stage-4 LRR v32 | — | PROBE | PROBE |
| R0035 | Stage-4 LRR v194 | — | PROBE | PROBE |
| R0047 | Stage-4 LRR (u32::MAX) | `YANG_LRR_STOP` tagged site exists (N52 probe infra) | PROBE | PROBE |
| R0049 | Stage-4 LRR (u32::MAX) | fails downstream per #169 Phase-0 triage | PROBE | PROBE |
| R0050 | Stage-4 LRR v58 | torus-profile rim crossing shipped (#131/N28); residual region | PROBE | PROBE |
| R0063 | Stage-4 LRR (u32::MAX) | re-entry zigzag class shipped (#145); residual | PROBE | PROBE |
| R0077 | Stage-4 LRR v3 | — | PROBE | PROBE |
| R0091 | Stage-4 LRR (u32::MAX) | the historical silent-wrong trap case, now a loud STOP — treat with P3c-level care | PROBE | PROBE |

### OffCurveBeyondChordBand (6)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0065 | Stage-4 OffCurve v8 | torus∩plane grazing loop reaches \|y\|=0.384 outside the box face; needs exact triple-junction corner insert + stitch (primitive proven, N-137.1) | CONFIRMED (#137 spec) | P3b-#137 |
| R0074 | Stage-4 OffCurve v89 | torus∩plane grazing — same class as C0065 | CONFIRMED (#137) | P3b-#137 |
| R0003 | Stage-4 OffCurve v4233 | multi-map over-band chain (v4233→v8508); needs ellipse×hyperbola junction handling, band-fixing exhausted (N45/N46) | CONFIRMED (N51/N52) | P3-junction |
| R0015 | Stage-4 OffCurve v84 | no-curve-type structural (N51: not band-fixable) | PROBE (class known, site not) | PROBE |
| R0026 | Stage-4 OffCurve v218 | exact generator band shipped (#164/N46); residual structural | PROBE | PROBE |
| R0070 | Stage-4 OffCurve v1028 | no-curve-type structural (N51) | PROBE | PROBE |

### Reassembly non-2-manifold (8) — the #146 junction-mint bucket

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0082 | non-2-manifold | near-duplicate junction verts v588≈v601 (0.012 apart 3D, ~4e-4 in-plane = off-plane); spurious in-patch overlap triangle; re-CDT REFUTED as tool | CONFIRMED (#169 Phase B, 0b655da2) | P3a-#146 |
| R0095 | non-2-manifold | EVERY face has a ~1e-24-area boundary triple — upstream degenerate junction geometry | CONFIRMED (#169, 0b655da2) | P3a-#146 |
| C0044 | non-2-manifold | 3-patch junction fires the Stage-4 gate | CONFIRMED (#169 Phase 0) | P3a-#146 |
| F0064 | non-2-manifold | wall vert 0.083 off floor plane; minted in Stage-4 mutation window OR inherited via lineage-less chained B (4 hypotheses eliminated, N51 session) | PARTIAL (#146) | P3a-#146 |
| R0051 | non-2-manifold | in the #146 Newell-normal class per task | SUSPECTED | P3a-#146 |
| F0058 | non-2-manifold | fails downstream per #169 Phase-0 triage | PROBE | PROBE→P3a |
| F0060 | non-2-manifold | fails downstream per #169 Phase-0 triage | PROBE | PROBE→P3a |
| F0085 | non-2-manifold | re-entry zigzag class shipped (#145); residual | PROBE | PROBE→P3a |

### CDT / tessellation failures (8) — mostly chained-input casualties

Chained models feed a yang boolean OUTPUT back in as an operand; degenerate
junction verts in that output then poison CDT. Suspected downstream of P3a.

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0045 | ring rejected by CDT (FaceId 9) | degenerate/self-intersecting ring | PROBE | PROBE→P3a? |
| R0011 | ring rejected by CDT (FaceId 407) | — | PROBE | PROBE→P3a? |
| R0016 | ring rejected by CDT (FaceId 1885) | — | PROBE | PROBE→P3a? |
| R0028 | ring rejected by CDT (FaceId 32) | — | PROBE | PROBE→P3a? |
| R0017 | holed lateral CDT: degenerate input | hyperbola vocabulary landed (#124); residual degenerate loop | PROBE | PROBE |
| R0085 | converted-input CDT backend failed (face 1) | chained-input | PROBE | PROBE→P3a? |
| R0100 | holed lateral CDT backend failed (face 4) | chained-input | PROBE | PROBE→P3a? |
| F0067 | converted-input CDT failed (face 272) | M8 opposite-rim projection class (#142/#143/#144) | CONFIRMED (task #144) | M8 |

### Stage-3 SSI (2)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0043 | AmbiguousCurve {1, 0} edge (23,93) | one candidate curve, zero matched — selection/junction gap | PROBE | PROBE |
| C0056 | AmbiguousCurve {1, 0} edge (37,70) | same signature as C0043 | PROBE | PROBE |

### NonPlanarFace (3)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| F0069 | NonPlanarFace FaceId(6227) | off-plane planar-face emission 3e-8 @ 2m | CONFIRMED (task #153) | #153 |
| F0072 | NonPlanarFace FaceId(10329) | same class (also the known assay-timeout artifact case) | CONFIRMED (#153) | #153 |
| R0081 | NonPlanarFace FaceId(666) | likely same class | SUSPECTED | #153 |

### Misc structural (5)

| Case | Loud error | Root cause | Confidence | Vehicle |
|---|---|---|---|---|
| C0046 | NonManifoldVertex(17) | kernel-v2 B-Rep-level vertex defect | PROBE | PROBE |
| C0048 | azimuth-merge rims mismatched (66 vs 69) | M8 rim-crossing/uniform-sample merge (#143 landed; residual = #144 snap-rounding) | CONFIRMED (#144) | M8 |
| C0075 | InvalidBooleanOutput (undirected edge ≠ 2 directed) | — | PROBE | PROBE |
| R0019 | input B-Rep not 2-manifold | chained input carries a defect from an earlier output | PROBE | PROBE→P3a? |
| R0053 | patch flood-fill LabelMismatch {seed 2, tri 3890} | native arrangement patch labeling — Stage 2/3 | PROBE | PROBE |

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
| C0111 | **SUPPORTED_WRONG** | 1e-8 m wall (below MIN_FEATURE_SIZE) silently dissolved, χ 0→2 — out-of-contract must reject loudly | **#178** |
| C0113 | **SUPPORTED_WRONG** | wall at exactly TAU_MODEL (1e-7 m) silently dissolved — the R0091 hazard rung is live | **#178** |
| C0116 | ~~SUPPORTED_WRONG~~ **ERROR (loud STOP, 2026-07-17)** | 0.01-deep cyl×cyl graze: WAS watertight/χ/volume-passing with a self-intersecting shell; the #173/N6 render-level gate now rejects the auto-union (`SelfIntersectingBooleanOutput`, 40 penetrations). Root fix = M5 exact degree-4 cyl×cyl curve (#172) — the chord-accurate path's trims interpenetrate sub-sagitta (spec `yang_173_selfx_detector.md` §6) | **#172** (root), gate shipped |

The 4 SUPPORTED_WRONG entries change the corpus summary from 240C/0W to a
baseline that shows WRONG > 0 **by exposure, not regression** — the 295
pre-existing cases are byte-identical. The 0-WRONG ratchet (#174) applies to
the pre-existing set immediately and to the whole corpus once #177/#178/#173
convert these to loud STOPs (the ratchet may then never regress).

## Rollup

| Vehicle | Cases |
|---|---|
| P2-M5 (SSI solvers) | 2 confirmed (R0044, R0096) — more may emerge from the probe queue |
| P3a-#146 (junction mint) | 5 confirmed/partial + 3 probe-suspected + up to 7 chained-CDT suspects |
| P3b-#137 (grazing corner) | 3 confirmed (C0065, R0074, R0038) |
| P3c (curved re-CDT) | 1 confirmed (R0072) |
| P3-junction (other) | 1 confirmed (R0003) |
| M8 residue | 4 confirmed (R0007, R0071, C0048, F0067) |
| #153 NonPlanarFace | 2 confirmed + 1 suspected |
| KV6/scope | 2 (C0063, R0004) |
| **PROBE queue (root cause unconfirmed)** | **26** |

**Reading:** the confirmed set already covers ~28 of 54 cases and justifies the
plan's ordering (P3a-#146 is plausibly the single biggest lever if the
chained-CDT suspects trace back to it). The 26-row PROBE queue is the rest of
Phase 1: run `YANG_LRR_PROBE` / `YANG_LRR_STOP` / `NONMANIFOLD_SITE_PROBE` /
`YANG_S6_NONPLANAR_PROBE` per case and upgrade rows to CONFIRMED before any
build targets them.
