# Yang Case-III Graze Guard (M5 #172 half b) — Spec

Task #172 increment 2, 2026-07-17. Root fix for C0116's
`SelfIntersectingBooleanOutput` STOP (the #173/N6 render-gate flip of the
original silent-wrong): a shallow cyl×cyl penetration the Stage-1 chord
meshes never sample.

## 1. Goal

A boolean whose two cylinder lateral surfaces ANALYTICALLY INTERSECT at a
penetration depth smaller than the combined Stage-1 chord sagitta (Yang
Fig. 8 **Case III** — "the meshes miss intersections",
`refs/text/yang2025_hybrid_boolean.txt:436-447`) must not emit topology
that ignores the intersection. Today the mesh boolean sees two disjoint
shells, emits them, and the output's true trimmed surfaces interpenetrate
— caught only by the #173 render-level selfx gate (union path), and for
depths below render sagitta not caught at all (silent wrong topology:
two lumps where the true result is one fused body).

The guard realizes the paper's Case-III elimination (§4.2.1: proximity
`< 2d_ε` implies a potential intersection that must be resolved; §4.3.3:
"if the two meshes do not intersect but are within the distance tolerance
d_ε, there is a tangent point, or a small loop") at Stage 1, as the exact
mirror of the SHIPPED Case-IV phantom guard
(`specs/yang_case_iv_phantom_guard.md`, `phantom_min_rim_segments`):
raise the rim sampling density of BOTH inputs until the combined chord
sagitta is strictly inside the analytic penetration depth, so the meshes
MUST intersect where the surfaces do, and the existing SurfacePair
machinery (ssi S2/S3 descriptor → Stage-3 membership → Stage-4
`relocate_onto_implicit_pair` → kernel-v2 K1–K11 acceptance,
`specs/m5_surface_pair_curve.md`) refines the wedge to the true curve.

**Phase-0 measurement (2026-07-17, debug single_case sweep,
`YANG_NSEG_FLOOR`):** C0116 baseline = ERROR
`SelfIntersectingBooleanOutput { face_a: 8, face_b: 11, penetrations: 40 }`;
floor 16 → **SUPPORTED_CORRECT**; floor 24 → **SUPPORTED_CORRECT** (all
oracles: χ=2 single fused body, volume, watertight, AND the render selfx
gate). The graze wedge is transversal enough that once the mesh samples
it, the shipped pipeline handles it end-to-end — this is NOT the #137
tangential-wander class (that sweep flipped ERROR→WRONG; this one flips
ERROR→CORRECT). The guard therefore ships as a case-recovering fix, not
a speculative lever.

## 2. Parameters

- Inputs: the two operand `BRep`s of `yang_rs::boolean` (any `BoolOp`),
  CROSS pairs only (A×B). Intra-solid graze = self-intersecting input,
  out of contract, untouched.
- No user-facing parameters. The forced minimum rim segment count is
  DERIVED per pair: the smallest `N ≥ 3` with
  `sag(r_a, N) + sag(r_b, N) ≤ depth / 2`, where
  `sag(r, N) = r · (1 − cos(π/N))` and `depth` is the analytic
  penetration depth below. The `depth/2` margin guarantees mesh-level
  penetration ≥ depth/2 regardless of chord phase (inscribed polygons
  recede at most `sag` radially inward; factor-2 safety, not a
  tolerance: a finer N is always chord-valid — governance A14.3, the
  same argument as the Case-IV guard). For C0116 (r_a=0.5, r_b=0.3,
  depth=0.01) the derived N = 29, comfortably above the measured green
  floor of 16.

## 3. Branch table

| Case | Behavior |
|---|---|
| cyl(A) × cyl(B), non-parallel axes, `0 < depth = r_a + r_b − d_lines` | shallow external graze → N derived (the C0116 class) |
| cyl(A) × cyl(B), parallel axes, `\|r_a − r_b\| < d < r_a + r_b` | proper crossing; `depth = min(r_a + r_b − d, d − \|r_a − r_b\|)` (the second term is the internal graze: the inner surface barely poking through the outer) → N derived |
| deep intersection (large depth) | derived N ≤ both operands' natural Stage-1 N → self-limiting gate drops it → byte-identical (no mode branch) |
| **render-observability scope line** (Boost arm only) | boost only when `depth > 2·1e-3·(r_a + r_b)` — twice the render mesh's combined chord sagitta (kernel-v2 render ratio 1e-3·r, the #173 gate's calibration). A shallower lens cannot be represented at ANY output resolution: the render selfx gate provably cannot see it, and the boost N required is unbounded (measured C0057: parallel depth 1e-6 ⇒ N=3142 ⇒ CORRECT→TIMEOUT corpus regression in the first assay run, while its baseline "green" is the shell-credited unfused status quo). The sub-render band keeps today's byte-identical path and is routed to §4.5.2 LOCAL refinement (roadmap P3d). Above the line the derived N is bounded ≈ 71 regardless of radii (scale-free ratio) — always affordable. The 4096-cap STOP below still applies to sub-floor depths. |
| **phase-aware Case-III filter** (Boost arm only) | the paper defines Case III as "the meshes MISS intersections": a demanding pair whose NATURAL meshes already intersect is NOT a miss — demand dropped, byte-identical path. Test = Cherchi exact tri-tri classifier (AABB-prefiltered) between each flagged face's Stage-1 tris and the ENTIRE partner mesh — the contact need not be lateral×lateral (parallel-axis lateral tris are all axis-parallel and can never cross; a vertex-line sliver enters through the partner's CAP disc). Both filter failure directions are safe: spurious "disjoint" costs only a finer mesh; spurious "intersects" is the measured pre-guard baseline. The SubSagitta STOP arm is NOT filtered — a phase-fluke vertex touch cannot sample a sub-resolution lens. |
| disjoint / exact tangency (`depth ≤ 0`) | no requirement — the Case-IV guard owns `depth < 0`; measure-zero tangency contact keeps today's behavior |
| `0 < depth ≤ noise` where `noise = max(TAU_MODEL, scale·TAU_WORK)/100`, scale = max coordinate/radius magnitude of the pair | authored-tangency rounding residue (the #178-calibrated coincidence-noise line: measured intended-coincident population ≤ 2.235e-10, designed sub-resolution features ≥ 1e-8) → treated as tangency, no requirement |
| `noise < depth` but derived N > 4096 (depth below the N-cap observability floor `2·(sag_a+sag_b)@4096 ≈ 5.9e-7·(r_a+r_b)`) | **typed LOUD STOP** `YangError::SubSagittaGrazeIntersection { face_a, face_b, depth, floor }` — the mirror of #178/N57: a genuine sub-resolution intersection no practical mesh can observe; emitting would be silent-wrong topology (unfused lumps), and the render selfx gate cannot see it either (depth ≪ render sagitta). P10: STOP, never silently proceed. |
| STOP-arm extent check | the STOP (only) verifies the graze region reaches both FACES: witness = common-perpendicular feet (skew) / axial-span overlap (parallel), each face's axial span from its rim-circle centers, inflated by the wedge half-length `sqrt(2·max(r_a,r_b)·depth)` + noise. Witness outside either face's span → no STOP (the infinite surfaces graze off-face; adjacent-boss false-reject hazard). A face with no derivable span (no rim circles) is treated as spanning — conservative-loud. The BOOST arm needs no extent check (a finer mesh is always valid; mirror of Case-IV). |
| non-cylinder curved pairs (sphere/cone/torus grazes) | out of scope this increment — unmeasured (P10); the #173 render gate remains their tripwire (roadmap: generalize per-pair depth formulas) |
| operand without cylinder B-Rep faces (`from_mesh` chained output) | scan finds nothing → byte-identical |

## 4. Invariants

- I1: with the guard active, every scoped analytic intersection with
  `depth > noise` either appears in the Stage-2 arrangement (boost) or
  STOPs loudly (sub-sagitta) — no scoped Case-III emission survives.
- I2: a finer forced N never violates any Stage-1 chord bound
  (sagitta monotone decreasing in N); rebuilds go through
  `rebuilt_with_min_rim_segments` exactly like the Case-IV guard.
- I3: an operand pair with no shallow-grazing cylinder pairs
  tessellates byte-identically (self-limiting natural-N gate, same
  shape as Case IV).
- I4: the STOP can only replace a silent-wrong emission, never a
  correct one (fires only where BOTH faces reach the graze and depth is
  above authoring noise yet below mesh observability).

## 5. Oracles

- Corpus C0116 → SUPPORTED_CORRECT (Phase-0 sweep already proved the
  post-boost pipeline; the guard derives N=29 ≥ the measured floor 16).
  Expectation row flips `Category::Error` → `Category::SupportedCorrect`.
- NEW corpus case C0118 (assay-coverage directive: the STOP class is
  not exercised): C0116 geometry with tool offset `0.8 − 1e-8`
  (depth 1e-8 — above the 1e-9 noise line, below the ~4.7e-7 cap
  floor) → expected ERROR `SubSagittaGrazeIntersection`.
- Unit (`tests_unit/m5_case_iii.rs`):
  - C0116 pair (perp, d=0.79, r=0.5/0.3) → Boost(29);
  - deep crossing → requirement ≤ natural N (dropped);
  - disjoint pair and exact tangency (d = r_a+r_b) → None;
  - depth 1e-8 in-extent → SubSagitta STOP; same depth with the tool
    displaced axially off the boss span → None (extent check);
  - parallel internal graze (d − |r_a−r_b| small) → Boost;
  - depth below noise (1e-12·scale) → None.
- Unit: `graze_guard_phase_hit_pair_is_silent` — the C0057 pair stays
  byte-identical through the phase filter while the C0116 shape still
  boosts.
- Full release categorized assay: C0116 ERROR→CORRECT, C0118 new
  designed ERROR, **zero regressions elsewhere** (P10 gate — any other
  per-case diff is inspected; conversions acceptable, regressions
  abort). *First run (unfiltered Boost arm) measured C0057/F0090
  CORRECT→TIMEOUT — the phase-aware filter is the P10 response; final
  run below.*

## 6. Failure modes

- False boost on off-face infinite-surface grazes: cost only (finer
  mesh), self-limited by the natural-N gate; accepted, mirror of
  Case IV.
- A boosted N making a heavy chained case slower: bounded by the 4096
  cap and the corpus timing budget; the assay run is the verdict.
- Chained-output operands (`from_mesh`) carry no analytic cylinder
  faces: the guard cannot see their surfaces (same blind spot as
  Case IV); the #173 render gate remains the downstream tripwire.

## 7. Research basis

- [#24] Yang, Jia & Yan 2025 §4.2.1 (Fig. 8 Case III; conservative
  detection `Dis(△t_A, △t_B) < 2d_ε` ⇒ potential surface intersection),
  §4.3.3 (meshes-don't-intersect-but-within-d_ε ⇒ tangent point or
  small loop, solved by Newton), §4.5.2 (local refinement). Our
  surfaces are analytic quadrics, so the conservative proximity
  heuristic is replaced by the EXACT pair penetration depth — strictly
  stronger than the paper's filter.
- `specs/yang_case_iv_phantom_guard.md` — the shipped mirror guard
  (M8 increments 15/16) whose mechanism (derived rim-N, factor-2
  sagitta margin, self-limiting gate, 4096 cap, boolean-entry rebuild)
  this spec reuses verbatim.
- `specs/m5_surface_pair_curve.md` — the SurfacePair vocabulary that
  refines the wedge once the mesh sees it (P8 degree-4 procedural
  curve).
- `specs/yang_178_subres_coplanar_gap_stop.md` / N57 — the calibrated
  coincidence-noise line (`band/100`) and the sub-resolution loud-STOP
  pattern the SubSagitta arm mirrors.
- `specs/yang_173_selfx_detector.md` §6 — the C0116 root-cause
  measurement (sub-sagitta penetration, both trims wrong at the graze).

*Created: 2026-07-17*
