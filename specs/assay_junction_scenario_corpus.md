# Assay Junction-Scenario Corpus (Group 7, C0102–C0117) — task #176

**Status**: implemented 2026-07-17 (audit + first tranche; baseline section filled
after the full-corpus run)
**Owner**: test-harness (`crates/test-harness/src/assay/gen_complexity.rs`, Group 7)
**Charter**: roadmap §0.0 "Continuous" — *assay coverage grows with the scenario
space* (user directive 2026-07-17); audit list from
`docs/yang_junction_research_findings.md`.

## Part 1 — Coverage audit (295-case corpus @ baseline 240C/0W/51E, commit 9d231008)

Method: static audit of case authoring (`gen_complexity.rs` families, legacy
`gen.rs` random series) cross-checked against the committed categorized run
(`app/tests/cases/assay/results.json`). Per audit class from the #176 charter:

### A1. 3-surface corner junctions beyond torus∩plane∩plane — PARTIAL GAP

- **cyl∩plane∩plane transversal**: abundantly covered (any circle-extrude +
  rect through-cut; legacy R/F series and C-series; many SUPPORTED_CORRECT).
  However no case pins a *partial-depth* notch corner (floor∩wall∩lateral
  triple) with an exact volume oracle. → **C0102**.
- **cyl∩plane∩plane GRAZING corner**: NOT covered. R0038 is the plane-tangent-
  to-cylinder-along-a-generator case but has no corner (full-span tangency,
  random legacy geometry, ERROR). No case makes the tangent generator END at a
  transversal floor plane — the cylinder analog of the #137 grazing-corner
  class. → **C0103**.
- **sphere∩plane∩plane transversal**: covered by C0067 (sphere + polar notch;
  typed Stage-4 LRR ERROR at the {sphere,wall,wall} triple junctions —
  the N2 conic-junction class, F0059 family). No gap.
- **sphere∩plane POINT-grazing**: NOT covered (plane exactly tangent to a
  sphere = 0-dimensional contact of the cut boundary; the sphere analog of
  #137's torus equator graze). → **C0104**.
- **cone∩plane∩plane**: exercised only incidentally via legacy gear-revolve
  frusta (R0003/R0004/R0008 class, LRR/conic-band ERRORs) and walled upstream
  in C0063 (oblique cut, UNSUPPORTED(curved-profile)). No curated
  frustum-notch case with exact oracles. → **C0105**.
- **curved×curved×plane**: NOT covered. C0051/C0052's tool caps sit at
  x∈{−1.5, 2}, outside the boss (|x|≤0.5) — no cap plane ever crosses the
  bicylinder curve. C0056's cap does cross but the case dies at Stage-3 on
  the tangency first. → **C0106**.

### A2. Tangential-contact families beyond R0038 — PARTIAL GAP

- **cyl×cyl parallel external / internal / near-tangent**: covered —
  C0055 (external, PASSES), C0056 (internal, Stage-3 SSI ERROR), C0057
  (1e-6 overlap, PASSES).
- **plane tangent to cylinder generator**: only R0038 (random legacy, ERROR).
  Curated deterministic twins: → **C0103** (cut, with corner) and **C0110**
  (union, line contact, no overlap).
- **sphere tangencies**: NOT covered at all (no sphere×sphere, sphere×cyl,
  sphere×plane tangency anywhere in the corpus). → **C0104, C0107, C0108,
  C0109**.
- **torus tangencies**: torus∩plane equator graze covered (C0065, R0074 —
  the #137 pair). torus×torus is M5 (R0044/R0096, covered as ERROR walls).

### A3. cyl×cyl lateral∩lateral — SUSPICION REFUTED (covered)

The findings doc's "suspected ZERO corpus cases" is **wrong at corpus level**:
C0051–C0058 (Group 2b, spec `assay_complexity_corpus.md`) exercise exactly
this — perpendicular unequal-R (C0051 union / C0052 cut), 45° oblique
(C0053), skew (C0054), parallel tangencies (C0055–C0057), equal-R 30°
Steinmetz union (C0058). Legacy random-normal circle-extrude pairs also cross
laterally. Current categories: C0051–C0055, C0057 **SUPPORTED_CORRECT**
(chord-accurate output passes the volume/χ oracles without the analytical M5
solver); C0056 Stage-3 SSI ERROR; C0058 Stage-4 relocation ERROR. What M5
(#172) still lacks is *analytical* refinement — the corpus coverage exists
and will hold it to account. Residual sub-gap: no cap-plane-through-crossing
corner (→ C0106, counted under A1).

### A4. Micro-feature scale sweeps (R0072-class) — GAP (relative + absolute)

C0029–C0031 sweep a planar sliver wall at ε ∈ {1e-3, 1e-5, 2e-6} on a 1 m
body — RELATIVE sweep only at one absolute scale, stopping above the 1e-6 m
feature floor. Missing per the charter: the same feature across ABSOLUTE
scales (validates the scale-relative TAU·(1+scale) criteria ratified in
N55/N56 — an absolute-tolerance bug is invisible at scale 1) and the
below-floor/at-TAU_MODEL rungs which must STOP loudly, never silently weld
(the R0091 trap). → **C0111** (1e-5 rel @ 1e-3 m body ⇒ 1e-8 m wall, below
floor), **C0112** (1e-5 rel @ 1e3 m body ⇒ 1e-2 m wall, must PASS),
**C0113** (1e-7 rel @ 1 m body ⇒ wall = TAU_MODEL exactly).

### A5. Coplanar zero-thickness / edge-only-touch rejects — PARTIAL GAP

- **0D/1D planar contact**: covered — C0045 (edge-only box union, PASSES),
  C0046 (corner-only, loud NonManifoldVertex ERROR), C0044 (cap-to-cap
  annulus, loud non-2-manifold ERROR), C0049 (flush cut, PASSES).
- **zero-thickness RESULT geometry**: NOT covered — no case where a boolean
  chain leaves an exactly-zero-thickness membrane or coincident interior
  walls. → **C0114** (coincident pocket walls — the wall between two cuts is
  exactly nothing; green = merged pocket), **C0115** (opposite-side pockets
  with exactly coplanar floors — zero-thickness membrane; green = through
  opening).
- **curved 0D/1D contact**: NOT covered (all contact-degeneracy cases are
  boxes). → **C0107** (point contact sphere–cylinder), **C0108** (point
  contact sphere–sphere), **C0109** (internal point tangency cavity),
  **C0110** (line contact plane–cylinder).

### A6. Post-boolean self-intersection fixtures (#173 red phase) — GAP

No corpus case is designed as a relocation-across-thin-gap self-intersection
hazard, and none can be *certified* self-intersecting until #173's detector
exists (only the detector can measure the output shell). Fixtures land now
with honest volume/χ oracles; #173's red phase runs its detector over the
whole corpus with these as the designed stress cases. → **C0116** (deep-graze
perpendicular cyl×cyl, 0.01 overlap wedge), **C0117** (1e-4 curved tube wall —
coaxial bore; parallel curved surfaces one relocation band apart). C0057 and
C0029–C0031 double as existing hazard material.

## Part 2 — New cases (Group 7: junction scenarios, C0102–C0117)

Same rules as the C-series spec: deterministic, no RNG, exact
generation-time oracles (closed form where available, fixed-step Simpson
quadrature for the two conic/elliptic integrals — kernel-independent either
way), `write_c_case` writer, centered profiles (noop-guard box model),
trackers carry χ and (where computable) volume so they self-verify when
their wall lifts. A Group-7 case that fails at baseline is recorded honest —
by design most of 7b/7e EXPECT loud typed rejects.

**7a — corner junctions [J1]:**

- **C0102** cyl∩plane∩plane transversal notch corner (bug hunter). Cylinder
  r=0.5, z∈[0,2]; rect notch x∈[0.2,0.8], y∈[−0.3,0.3], z∈[1,3] (partial
  depth). Exact removed area A = 0.3·0.4 + 0.25·asin(0.6) − 0.12 per the
  circular-segment integral; V = π/2 − A. χ=2.
- **C0103** cyl∩plane∩plane GRAZING corner (tracker, R0038+corner /
  #137-cyl class). Same cylinder; notch x∈[−0.5,0.2] (left tool wall EXACTLY
  tangent along the (−0.5,0,z) generator), y full-span, z∈[1,3]. The tangent
  generator ends at the notch floor z=1 → grazing corner (−0.5,0,1).
  V (if green) = π/2 − (π/4 − seg(0.2))·1, χ=2. Expected today: loud
  tangency/Stage-4 ERROR.
- **C0104** sphere∩plane point-graze (tracker, #137-sphere). Sphere r=0.4 at
  (0,0,0.5) (on-axis circle revolve); through-cut x∈[−0.4,0.1] — the left
  tool wall tangent to the sphere at the single point (−0.4,0,0.5).
  Remaining cap h=0.3: V = πh²(3r−h)/3, χ=2.
- **C0105** cone∩plane∩plane frustum notch (bug hunter / conic-junction
  probe). Frustum r 0.8→0.4 over z∈[0,1] (trapezoid revolve, no apex); rect
  notch x≥0.45, y full-span, z∈[0.5,2]. Removed volume by Simpson over the
  circular-segment area seg(r(z), 0.45), z∈[0.5, 0.875]. χ=2. Expected
  category may land in the N38/N39 conic LRR class — recorded honest.
- **C0106** cyl×cyl×plane cap-through-crossing corner (tracker, M5-corner).
  C0052's geometry but the tool bore is BLIND: circle r=0.3 from [2,0,1]
  along −X, depth 2.0 → cap plane x=0 crosses the bicylinder curve inside
  the boss → cyl-lateral∩cyl-lateral∩cap-plane corners. V by Simpson
  (elliptic integrand), χ=2.

**7b — curved tangencies / degenerate contact [J2+J5-curved]:**

- **C0107** point-tangent sphere⊕cylinder union (tracker). Cylinder r=0.5
  z∈[0,2] + sphere r=0.4 at (0.9,0,0.5), tangent at (0.5,0,0.5), zero
  overlap. Green definition = loud reject or honest 2-body outcome; silent
  single-shell pinch is the failure it sentinels. χ pinned 4 (2 shells).
- **C0108** externally tangent equal spheres (tracker). r=0.4 spheres at
  (0,0,0.5) and (0.8,0,0.5); point contact at (0.4,0,0.5). As C0107.
- **C0109** internally tangent sphere cavity (tracker, `internal-void`).
  Sphere r=0.5 at (0,0,0.5) minus sphere r=0.3 at (0.2,0,0.5) — cavity
  touches the outer surface at exactly (0.5,0,0.5). Green = loud reject
  (pinched shell is invalid); χ pinned 4.
- **C0110** line-tangent box⊕cylinder union (tracker). Cylinder r=0.5
  z∈[0,2] + box x∈[0.5,1.5], y∈[−0.4,0.4], z∈[0.25,1.75]: face plane
  tangent along a generator SEGMENT, zero overlap. Planar twin C0045
  passes; this pins the curved-contact behavior. χ pinned 4.

**7d — micro-feature scale sweep [J4]** (C0030's geometry, uniformly scaled;
exact box-arithmetic volumes via `chain_vol`):

- **C0111** s=1e-3 (mm body): ε_rel=1e-5 ⇒ 1e-8 m wall, below the 1e-6 m
  feature floor — MUST be loud (reject or documented wall), never a silent
  weld. χ=0 + exact volume pinned for the honest-green case.
- **C0112** s=1e3 (km body): ε_rel=1e-5 ⇒ 1e-2 m wall — comfortably in
  contract, MUST pass. The absolute-band sentinel: any to_bits/absolute-TAU
  shape would corrupt or reject here. χ=0, exact volume.
- **C0113** s=1: ε=1e-7 ⇒ wall exactly TAU_MODEL — the R0091 hazard rung.
  Loud STOP acceptable; silent watertight-but-wrong is the failure. χ=0,
  exact volume.

**7e — zero-thickness results [J5]:**

- **C0114** coincident pocket walls. 2×1×1 slab; two blind pockets
  z∈[0.5,1], x∈[−0.9,0] and x∈[0,0.9] (shared wall plane x=0, thickness
  exactly 0). Green = the merged single pocket (V=1.37, χ=2) or a loud
  typed reject; silently keeping a zero-thickness wall is the failure.
- **C0115** opposite-side coplanar floors. Unit cube; pocket from above
  z∈[0.5,1] (x,y∈[−0.3,0.3]) + pocket from below z∈[0,0.5] (x,y∈[−0.25,0.35])
  — floors exactly coplanar at z=0.5, overlapping footprint ⇒ zero-thickness
  membrane. Green = through opening (exact volume, χ=0) or loud reject.

**7f — #173 self-intersection hazards [J6]:**

- **C0116** deep-graze perpendicular cyl×cyl union. Boss r=0.5 z-axis; tool
  r=0.3 x-axis at (·, 0.79, 1) — axis distance 0.79 vs r+R=0.8 ⇒ 0.01-deep
  crossing wedge. Relocation across the wedge is the §4.5.4 hazard. χ=2,
  volume ≈ sum (lens ≪ tol).
- **C0117** coaxial bore leaving a 1e-4 curved tube wall (curved twin of
  C0034). Two parallel cylinder surfaces one relocation band apart along the
  full circumference. χ=0, exact volume (5% tol; the inscribed-chord area
  deficit largely cancels in the annulus difference).

## Mechanics

- New `family_junction_scenarios(dir)` appended in `generate_complexity_cases`;
  C-series count assertion 101 → 117; `assay_kv2.rs` corpus assertion
  295 → 311; tempdir generator test updated.
- Regeneration: `cargo run -p test-harness --bin assay_gen -- --complexity-only
  --output app/tests/cases/assay` (byte-stable FNV UUIDs; manifest merge).
- Baseline: run the 16 new cases via `ASSAY_CASE=<id> single_case`, then the
  full categorized release run to refresh `results.json`; pin representatives
  in `smoke_corpus_boundary_categories`; record categories below. New
  non-green cases are self-triaged by construction (each names its designed
  root-cause family above) — no unexplained ERRORs are added to the tail.

## Baseline categories (first run, 2026-07-17)

One authoring bug was found and fixed before baselining: C0116's tool was
initially extruded along +normal away from the boss (bosses extrude +normal,
cuts −normal), leaving disjoint bodies — caught by its own bbox oracle
(diag 6.52 = the runaway tool span). Re-authored to cross the boss.

| Case | Category | Disposition |
|---|---|---|
| C0102 | SUPPORTED_CORRECT | transversal cyl notch corners pass with exact volume |
| C0103 | SUPPORTED_CORRECT | **boundary correction**: the tangent-generator + floor grazing corner PASSES (better than the R0038 suspicion implied) |
| C0104 | SUPPORTED_CORRECT | **boundary correction**: sphere point-graze cut passes |
| C0105 | **SUPPORTED_WRONG** | **FINDING C0105-F1**: silent non-watertight, self-intersecting shell (51 unpaired edges, 10 penetrations, χ=−1) at cone∩plane∩plane notch corners — the conic-junction class has a silent-wrong path. Must become a loud STOP (P10). |
| C0106 | SUPPORTED_CORRECT | cap plane through the bicylinder curve passes (M5 surface-pair curve carries it) |
| C0107 | ERROR (loud) | point-tangent sphere⊕cyl → typed non-2-manifold reject ✓ desired posture |
| C0108 | ERROR (loud) | tangent spheres → typed non-2-manifold reject ✓ |
| C0109 | ERROR (loud) | internal point-tangent cavity → Stage-3 AmbiguousCurve{0,0} (loud; curve-vocabulary gap for the degenerate SSI, M5-adjacent) |
| C0110 | SUPPORTED_CORRECT | line-tangent box⊕cyl union passes (matches planar twin C0045) |
| C0111 | **SUPPORTED_WRONG** | **FINDING C0111-F1**: 1e-8 m wall (below the 1e-6 m floor) silently dissolved, χ 0→2 — out-of-contract input must be rejected LOUDLY, not mutated silently |
| C0112 | SUPPORTED_CORRECT | 1e-2 m wall @ km body passes — the scale-relative criteria hold at large absolute scale |
| C0113 | **SUPPORTED_WRONG** | **FINDING C0113-F1**: wall at exactly TAU_MODEL silently dissolved — the R0091 hazard rung is live (silent weld, not a STOP) |
| C0114 | SUPPORTED_CORRECT | coincident pocket walls merge exactly (zero-thickness wall correctly = nothing) |
| C0115 | SUPPORTED_CORRECT | coplanar-floor membrane opens exactly (through opening, χ=0) |
| C0116 | **SUPPORTED_WRONG** | **FINDING C0116-F1**: 0.01-deep cyl×cyl graze passes watertight/χ/volume but the shell SELF-INTERSECTS (10 penetrations) with no kernel STOP — N6 (#173) demonstrated in-corpus for the first time. Designed red-phase fixture; flips to a typed STOP when §4.5.4 lands. |
| C0117 | SUPPORTED_CORRECT | 1e-4 curved tube wall survives end to end |

Net: 10 correct, 4 SUPPORTED_WRONG (all four are FINDINGS — real silent-wrong
exposures, none authoring artifacts; per corpus policy they stay honest and
must be converted to loud STOPs by kernel work, never re-authored away),
3 loud ERRORs (desired reject posture for 0D-contact degeneracies).
Contrast pins C0031/C0112 prove the in-contract scale-relative behavior is
correct; the failures are exactly at/below the contract floor.

The corpus-level `no_self_intersection` oracle already exists in the assay
runner and is what caught C0105/C0116 — #173's job is the KERNEL-side §4.5.4
detector so these become typed STOPs instead of silent emissions.

**Committed baseline (full 311-case categorized run, 300 s CPU budget):
249 CORRECT / 4 SUPPORTED_WRONG / 54 ERROR / 4 UNSUPPORTED.** The 295
pre-existing cases are category-identical to the prior 240C/0W baseline
(verified by per-case diff of `results.json` vs HEAD) — WRONG > 0 is by
EXPOSURE, not regression. Follow-up tasks: #177 (C0105), #178 (C0111/C0113),
#173 red fixture (C0116). One stale pre-existing smoke pin was found and
moved in passing (C0048: the M8 campaign had already lifted its
UNSUPPORTED(coplanar) wall to the deeper azimuth-merge ERROR; the stale pin
sat behind the debug tier's fail-fast, the C0065/C0071 precedent).
