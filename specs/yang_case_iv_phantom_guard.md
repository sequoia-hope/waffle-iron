# Yang Case-IV Phantom-Intersection Guard (M8 increment 15) — Spec

Task #72, 2026-07-07. Fixes F0088 op 4's Stage-3
`AmbiguousCurve { candidates: 0, matched: 0 }`.

## 1. Goal

A boolean whose two curved surfaces are ANALYTICALLY DISJOINT but whose
Stage-1 chord meshes intersect (Yang Fig. 8 **Case IV** — "the meshes
detect intersections that do not exist in surfaces",
`refs/text/yang2025_hybrid_boolean.txt:436-447`) must not manufacture a
phantom intersection curve. The output topology follows the analytic
truth: the thin wall between the surfaces SURVIVES (measured F0088: gap
0.0115 = 115× MIN_FEATURE_SIZE — a real feature, A14.2).

The guard realizes the paper's Case-IV filter at Stage 1: raise the rim
sampling density of BOTH inputs until their combined chord sagitta
clears the analytic gap, so the meshes stop intersecting where the
surfaces do not and the phantom never reaches the arrangement.

## 2. Parameters

- Inputs: the two operand `BRep`s of `yang_rs::boolean` (any `BoolOp`).
- No user-facing parameters. The forced minimum rim segment count is
  DERIVED per pair: the smallest `N ≥ 3` with
  `sag(r_a, N) + sag(r_b, N) ≤ gap / 2`, where
  `sag(r, N) = r · (1 − cos(π/N))`, maximized over all analytically
  disjoint cylinder-face pairs (A×B). The `gap/2` margin keeps the
  combined band strictly clear of the gap (factor-2 safety, not a
  tolerance: a finer N is always chord-valid — it only shrinks the
  sagitta; governance A14.3, same argument as the coincident-cylinder
  `min_n_seg` path).

## 3. Branch table

| Case | Behavior |
|---|---|
| cylinder(A) × cylinder(B), axes parallel, externally disjoint (`d_axes > r_a + r_b`) | gap = `d_axes − r_a − r_b` → N requirement derived |
| cylinder(A) × cylinder(B), axes parallel, nested disjoint (`d_axes + r_small < r_large`) | gap = `r_large − d_axes − r_small` → N requirement derived (the measured F0088 case) |
| cylinder(A) × cylinder(B), skew/non-parallel axes, `d_lines > r_a + r_b` | gap = `d_lines − r_a − r_b` → N requirement derived |
| cylinder pair with `gap ≤ 0` (infinite surfaces intersect) | no requirement (a real intersection — SSI refines it) |
| far disjoint pair (large gap) | derived N ≤ the natural Stage-1 N → `max()` no-op (self-limiting; no mode branch) |
| non-cylinder curved pairs (sphere/cone/torus), cylinder × plane | out of scope this increment — unmeasured (P10); the loud `AmbiguousCurve` stop remains their tripwire |
| operand without B-Rep faces (`from_mesh`, chained boolean output) | scan finds no cylinder faces → `None` → byte-identical path |
| **INTRA-solid disjoint pair (M8 increment 16)** — two of ONE solid's own cylinders closer than the chord bands (the chained F0088 output: hole 4's lateral 0.0115 from the plate wall) | folded into **Stage 1's own N selection** (`stage1_tessellate_inner`), so EVERY tessellation of the solid — input conversion, Stage-0 rebuilds, the guard's rebuilds — picks it up natively. Without it the cap's outer-rim chords dip across the hole rim and the planar CDT gets CROSSING constraints (`CDT triangulation failed`, measured corpus F0088 ops 7/15 at conversion time). The `boolean()` guard stays CROSS-only. |

## 4. Invariants

- I1: with the guard active, no intersection edge in the Stage-2
  arrangement connects two analytically disjoint cylinder surfaces
  (the Stage-3 zero-candidate stop becomes unreachable for the scoped
  pairs).
- I2: a finer forced N never violates any Stage-1 chord bound
  (sagitta is monotone decreasing in N).
- I3: an operand pair with no disjoint cylinder pairs tessellates
  byte-identically to the pre-guard pipeline.
- I4: the thin wall survives — output volume equals the analytic value
  (no phantom notch).

## 5. Oracles

- Pin retire: `f0088_cut4_stays_loud_phantom_intersection_wall`
  (kernel-v2 chain, expects `AmbiguousCurve` today) → converts to a
  positive regression on cut 4 with a valid output.
- `f0088_engine_frame_chain_no_offsurface_residue` extends to assert NO
  `AmbiguousCurve` residue either (all 15 cuts succeed) with the volume
  oracle over all 15 holes.
- Corpus F0088 → SUPPORTED_CORRECT (its last error).
  **Post-ship measurement (2026-07-07):** the DIRECT chain met every
  oracle (all 15 cuts green, no `AmbiguousCurve` / `VertexOffSurface`,
  volume in band). The CORPUS path retired the phantom class but exposed
  a distinct pre-existing wall at ops 7/15: `face 0: CDT triangulation
  failed` — the sketch-extrude + auto-union chained re-entry feeds the
  next boolean a recovered cap whose CDT fails at the boosted rim
  density. F0088 corpus: `AmbiguousCurve + 0 VertexOffSurface` (1 error)
  → `2× CdtFailed` (2 errors, new mechanism, loud). The guard is
  retained (topology now follows analytic truth; the family's direct
  chain is fully green); the CDT wall is the next measured lever.
  **Increment 16 (same day, task #73) closed it:** the CDT crossings are
  the INTRA-solid form of the same criterion — the chained body's own
  hole-near-wall pair. A `boolean()`-level intra scan was tried first
  and made the corpus WORSE (10 conversion-time failures: the boosted
  outputs' recovered rims re-entered `BRep::new` at natural N and the
  cap CDT crossed there — the guard cannot reach conversion). Moved to
  Stage 1's N selection (the single place every tessellation flows
  through). **Corpus F0088 → SUPPORTED_CORRECT (289s solo — the
  heavy-chain container band).** F0086/F0087/F0089 unchanged CORRECT.
- Unit: `phantom_min_rim_segments` — nested-disjoint pair yields the
  derived N (F0088 numbers: N ≥ 24); crossing pair yields none; far
  pair yields a no-op N; empty-faces operand yields none.
- Family pins (chain suite) + full yang-rs / rewrite tier: no
  regression.

## 6. Failure modes

- A disjoint pair whose derived N is impractically large (gap approaches
  0 while surfaces stay disjoint — true near-tangency): the N loop is
  capped at 4096 segments; beyond it the guard yields no requirement and
  the loud Stage-3 `AmbiguousCurve` remains the tripwire (P9 — never
  silently proceed with phantom topology).
- Out-of-scope surface pairs keep today's loud stop.

## 7. Research basis

- [#24] Yang, Jia & Yan 2025 §4.2.1 (Fig. 8 Case IV; the d_ε filter and
  optimization-based intersection computation), §7 limitation note on
  small loops below mesh resolution. The guard is the Case-IV filter
  realized at the tessellation stage: instead of discarding detected
  phantom pairs post-hoc, the discretization is refined so the phantom
  never appears — equivalent to the paper's "locally increasing
  discretization resolution" (used there for Case V) applied with the
  ANALYTIC gap as the terminating criterion.
- Governance A14.3 (single tolerance source): the derived N reuses the
  Stage-1 sagitta formula; the `gap/2` margin is a derived bound, not a
  widened tolerance.

## 7a. Analytical vs. approximate method

Exact: disjointness and gap are closed-form on the analytic cylinder
axes/radii (line-line distance). The mesh is never consulted for the
decision. Surface pair coverage: cylinder × cylinder (all axis poses);
other quadric pairs are out of scope this increment and keep the loud
stop.
