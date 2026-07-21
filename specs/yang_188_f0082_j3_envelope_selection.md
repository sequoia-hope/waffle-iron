# #188 — Stage-5/6 boundary-envelope selection for osculating curve pairs

**Status: SPEC (2026-07-21); inc-0 DONE (2026-07-21) — probe shipped, both
§7.9 open questions answered, and §2's "v925 = mandatory free-space switch"
premise REFUTED by measurement: see §7 before building inc-1. inc-1 DONE
(2026-07-21) — `stage5_envelope.rs` switch-point solver + §7.6 band
classifier, UNWIRED, all pins green: see §8. Production untouched until
inc-3; every increment lands behind the standard ledger.**

Successor to the F0082 J3 layer named by the P3b pierce spec
(`yang_169_p3b_curved_partner_pierce.md` §7.6), **re-characterized by
measurement in §7.9 of that spec** — read §7.9 first; this spec builds
on its ground truth and does not repeat it.

## 1. Problem

On a curved output patch, two boundary-curve supports can OSCULATE: an
intersection curve (e.g. F0082's section ellipse = tube∩top-plane) and
an original boundary curve (the cap rim = tube∩cap-plane) run within a
sub-observability gap of each other over a wide parameter range, and
cross at exact switch points (for two planes: the surface's hits of the
plane∩plane intersection line — F0082: two tube∩top∩cap triple points,
exactly antipodal because the planes' line passes through the tube
axis).

The Stage-5 patch boundary cycle is MESH-level (`patch_boundary_cycle`
on the kept mesh). Within the band where |axial gap| is below the
combined chord sagitta (F0082: gap amplitude 8.1e-3, sagitta of the
same order), the mesh cannot separate the two curves, so the kept-mesh
boundary weaves between their chains. `emit_topology` copies that weave
into the output loop verbatim (per-segment curve attribution from
`intersection_curves`, `Curve::LineSegment` fallback), producing:

- boundary segments of one curve emitted where the OTHER curve is live
  (F0082: rim run submerged inside A, sd_top −6.7e-3..0);
- fold-backs at the switch points (overshoot + dead-side stub +
  backward run — the render CDT's ring-reject, the loud STOP today);
- bare switch chords with no junction vertex where the weave jumps
  chains;
- `LineSegment` fallback vocabulary on original-curve segments (the
  #158/F6 gap, aggravated here).

This is the mesh-observability lesson of #137 ("finer mesh flips loud
ERROR to silent WRONG") and #172 C0118 (Fig. 8 Case III sub-sagitta
graze) surfacing at the OUTPUT-BOUNDARY level: no practical mesh
resolution fixes it; the envelope decision must be ANALYTIC (A15).

## 2. Ground truth (red fixture)

F0082 Extrude-11 union, FaceId 3727, the banked 61-vert ring
(pierce-spec §7.9): chart v = axial height above the cap plane;
live-curve rule for THIS case (Union):

- ellipse live ⟺ its point lies ABOVE the cap plane (sd_cap > 0) —
  elsewhere the lateral does not exist below B's own cap;
- rim live ⟺ its point lies ABOVE the top plane (sd_top > 0) —
  elsewhere the rim is interior to A;
- exactly one of the two is live at every azimuth except the two triple
  points, where both sd's vanish and the boundary switches. One switch
  (v925) is in free space and MUST be a boundary junction (it already
  exists as an output vertex); the other (J3) is masked by the wall
  complex (the switch runs through the wall×tube arc J→J2 — inc-4d's
  mint, correct, untouched by this spec).

Correct ring: single monotone-azimuth bottom chain alternating
ellipse/rim exactly at the switch vertices; no interior segments, no
folds, no bare chords.

## 3. Design

Three parts, strictly layered. Everything analytic uses the exact
surface/plane data already carried by `PatchInfo.inherited` and the
attributed `Curve`s — never mesh positions.

### 3.1 Detection (probe → typed loud STOP)

`detect_osculating_boundary_pair(patch_loop, supports)` — on a curved
patch's emitted loop, find support pairs (one intersection curve, one
original curve, both on the same underlying surface) whose gap,
evaluated analytically at the loop's parameter range, dips below the
derived observability floor (the C0118 combined-chord-sagitta
vocabulary — NOT a new band) while the loop alternates between the two
supports more than twice or carries a backward fold / sub-band switch
chord. Fires ⇒ the weave signature is present.

Promotion ladder (the #173 lesson — an exact-stage STOP can fire on
CORRECT cases): ship as a read-only probe first
(`YANG_S5_OSCULATION_PROBE`), measure the corpus fire set, and promote
to a typed `YangError` STOP only if the fire set ⊆ broken cases.

### 3.2 Envelope resolution primitive (pure, unwired first)

`resolve_boundary_envelope(surface, c_int, c_orig, op, side)`:

1. **Switch points**: exact crossings of the two supports = surface ∩
   plane(c_int) ∩ plane(c_orig) — the #137 N-137.1 triple-junction
   family. For the cylinder×plane×plane instance this is closed-form
   (two-plane line ∩ tube). Fail closed (no envelope, keep the STOP)
   for supports outside the implemented vocabulary.
2. **Band classification**: between consecutive switch points, each
   support is live or dead by an OP-RESOLVED sign test on the OTHER
   support's plane (the inc-4b `resolve_trim_beyond` style table;
   F0082/Union: ellipse live ⟺ sd_cap > 0, rim live ⟺ sd_top > 0).
   Postcondition: per band exactly ONE support live — anything else
   (both/neither) is a loud typed error, never a guess.
3. **Output**: the envelope as an ordered chain of (curve, from-switch,
   to-switch) segments with the switch points as junction vertices —
   minted BY IDENTITY onto existing output vertices when one is already
   present within the exact-dedup bit test (v925), inserted otherwise.

### 3.3 Loop rebuild (gated wiring)

In `emit_topology` (curved-surface branch), after cycles are built:
when the promoted detector fires on a loop, replace the woven section
with the §3.2 envelope — drop the dead-side segments, keep everything
outside the weave band byte-identical, resample dropped-in segments
from their analytic curves, emit true `Curve` vocabulary (no
`LineSegment` fallback inside the rebuilt section — the #158 gap must
not widen here; full #158 migration stays its own task).

Postconditions (all loud on failure — P10):
- rebuilt loop simple and closed; azimuth monotone per band;
- every vertex on the patch surface within the import band;
- no sub-band vertex pair (the C0102 pre-filter vocabulary);
- the wall-complex section (J→J2 arc) byte-identical.

## 4. What this spec does NOT do

- No tolerance/band tuning: the observability floor is the existing
  C0118 vocabulary; the sign tests are exact.
- No mesh re-tessellation, no Stage-2 label changes: the kept mesh is
  correct at mesh resolution; only the OUTPUT boundary is rewritten.
- No #137 grazing-corner work (that spec continues independently); no
  wholesale #158 curve-typing migration.
- The J/J2 wall corner and all pierce-spec machinery are frozen
  dependencies — byte-identical through every increment.

## 5. Increments (each a session-sized checkpoint, standard ledger)

- **inc-0 — probe.** `YANG_S5_OSCULATION_PROBE` in `emit_topology`:
  per curved-patch loop, print support pairs, analytic gap range,
  alternation count, fold/bare-chord signature. Run corpus-wide;
  expected fire: F0082 (the red fixture) + candidates from the S5/S6
  output-ring family (#171: F0045 class). Must also answer §7.9's open
  questions: what sits at u≈0.119 (hypothesis: the weave-band edge,
  where |gap| first exceeds the sagitta) and the kept-mesh chain shape
  at the weave site. Production byte-identical.
- **inc-1 — switch-point primitive + fixtures.** The cylinder
  two-plane triple-point solver + band classifier, unit-tested against
  the F0082 pinned geometry (v925 and J3 to 9 decimals; antipodality;
  live/dead table red/green per op). Unwired.
- **inc-2 — envelope assembly + rebuild, gated
  (`YANG_S5_ENVELOPE_ENABLE`).** Synthetic benign-scale fixture
  (tube + two near-coplanar planes, intersection line through the
  axis — the F0082 shape without the wall): gate-ON rebuilt loop passes
  the render CDT and all §3.3 postconditions; gate-OFF byte-identical
  corpus.
- **inc-3 — gate-ON ledger.** Full assay: F0082 ring accepted or the
  next defect layer exposed LOUDLY; 0-WRONG ratchet; zero
  uncharacterized regressions; sidecar parity; detector promotion
  decision from the inc-0 fire set.
- **inc-4 — flip** per the standard ledger (P3a inc-3 / P3b inc-5
  precedent), env var demoted to dev A/B knob.

## 6. Oracles

- Unit: switch-point pins, band table, envelope postconditions,
  benign-scale fixture end-to-end (closed conformal, CDT-accepted).
- Corpus: F0082 (red today); watch-list = the S5/S6 output-ring family
  + the §7.4 pierce watch-list; euler/volume/bbox on every flipped
  case.
- Meta-oracle: gate-ON minus gate-OFF may only move cases
  ERROR→CORRECT or ERROR→(different loud ERROR one layer deeper);
  any silent flip aborts the increment (P9/P10).

## 7. inc-0 findings (2026-07-21, probe-only; production byte-identical)

Probe `YANG_S5_OSCULATION_PROBE` shipped in
`stage5_osculation_probe.rs`, hooked into `emit_topology`'s curved
branch (`=walk` adds the per-vertex loop dump for firing pairs).
Per pair it reports the analytic gap g(θ) = g0 + amp·cos(θ−φ), the
combined chord-sagitta floor, the gap zeros with nearest output verts,
`band_frac` (fraction of the circle with |g| < floor), and the weave
signature (ambiguous-edge count, alternations, folds, bare switch
chords, mixed-side counts) restricted to edges within the floor of a
support. FIRE = ambiguous ≥ 3 + any weave signal.

### 7.1 The pair, measured (F0082, failing-union tube patch info=373)

int0 = tube∩A-top ellipse × orig = B's own cap plane: g0 = 4.4e-16
(the §1 odd sinusoid — the planes' line passes through the axis),
amp = 8.107403e-3, zeros antipodal to π at 6 digits: θ = −1.088507
(5.8e-14 from v925) and θ = +2.053086 (J3; nearest output vert
2.760e-3 away). **amp < floor = 1.234e-2 ⇒ band_frac = 1.0**: the
ellipse and rim are sub-observable over the ENTIRE circle at this
density — there is no "outside the weave band" for this pair; §3.3's
"keep everything outside the band" degenerates to "outside the
rebuilt segments".

### 7.2 §7.9 open question (a) ANSWERED — u≈0.119 is a WALL junction

The switch-chord endpoints are exact wall-plane junctions, not band
edges: v921 (ellipse) and v949 (rim) lie on A's face-366 plane to
≤7e-10; the opposite ends v951 (rim) and v926 (ellipse) likewise
(≤7e-10). The "submerged rim run" spans exactly the azimuth band
BEYOND face-366 (sd_366 > 0, e.g. v961 +2.38e-2), where A is absent —
**the rim is the TRUE boundary there and the run is CORRECT**; §7.9
defect #1 ("interior emitted as boundary") is hereby re-characterized.
Symmetrically on the J3 side: v943/v937 are the rim×364 and
ellipse×364 junctions (≤1.1e-9 on face-364's plane), and the rim run
v959…v954…v943 is correct.

### 7.3 BOTH triple points are wall-masked; "v925 mandatory" REFUTED

J3 sits +1.2921e-3 beyond face-364; v925 sits +1.2921e-3 beyond
face-366 (identical by model symmetry). Neither triple point is a
live boundary junction: on each side the ellipse↔rim switch is
subsumed by the wall-crossing complex (§2's masking argument applies
at BOTH switches). v925 legitimately remains an output vertex (cap
spokes) — it just must not be threaded into the tube loop.

### 7.4 The actual defect, sharpened

- **PRIMARY — dead-side detour at the v925-side wall exit**: after
  v951 [rim×366] the cycle detours v951 → v926 [ellipse×366, 1.05e-4
  BELOW the cap = dead side] → BACKWARD 2.76e-3 to v925 [beyond-wall
  triple point] → forward to v959 [rim]. Correct: continue on the rim
  v951 → v959. The three spurious segments create the azimuth
  fold-back the render CDT rejects. (The mirror wall entry
  v921→v949 is emitted correctly, as is the whole J3-side complex.)
- **SECONDARY — micro-stubs**: near-dup verts v938 (1.9e-6 off
  face-364) and v932 (1.2e-5) between v937 and the ellipse proper
  (§7.9 defect #5, §4.3-sliver flavored).
- All junctions needed for the correct loop ALREADY EXIST (inc-4d
  mints); the F0082 fix is pure boundary SELECTION — drop dead
  segments — not a new mint.

### 7.5 §7.9 open question (b) ANSWERED — no Stage-2 label leak

Weave-run rim edges each bound a kept cap-disc triangle (centroid
sd_cap = 0, sd_top down to −5.1e-3 — within the 6.2e-3 sagitta band)
plus a kept lateral triangle above: the kept mesh is consistent at
mesh resolution, and per §7.2 the run is even analytically correct.
The detour verts likewise bound kept triangles — the mesh-level chain
walker is faithfully tracing a mesh-scale-jagged kept boundary; only
the OUTPUT selection can fix it (the §1 premise stands).

### 7.6 Design impact on §3.2 (binding for inc-1)

The band classification (§3.2.2) must be an op-resolved liveness test
against ALL crossing support planes — the osculating pair PLUS any
masking wall whose plane crosses the band (F0082: rim live ⟺ beyond
either wall OR above A's top; ellipse live ⟺ inside both walls AND
above the cap) — with switch junctions at the wall crossings (minted)
rather than at wall-masked triple points. A triple point is a switch
junction ONLY when no masking plane covers it ("free-space" case —
NOT exercised by F0082; keep the primitive's fail-closed arm for it).
The §3.1 detector vocabulary is unchanged (the probe's FIRE already
keys on the weave signature, not on triple-point liveness).

### 7.7 Corpus fire set (312-case sweep, 2026-07-21)

NOTE: the full-corpus assay driver discards subprocess stderr — probe
sweeps must use the #171 xargs pattern over `ASSAY_CASE=<id>
single_case` on the release test binary.

9 cases fire; `band_frac` cleanly stratifies them:

- **True osculating pairs (band_frac ≥ 0.7)**: F0082 (1.000, ERROR —
  the red fixture), **F0085 (1.000 & 0.701, ERROR — the #171
  "open-seam" S5/S6 output-ring case: two osculating pairs)**, F0084
  (0.774, CORRECT), F0076 (1.000, CORRECT) — the last two are
  GREEN cases carrying a genuine sub-observable pair: watch-list for
  inc-3 (verify their emitted envelopes are correct, not lucky).
- **Marginal**: R0095 (0.398, ERROR — Stage-4 non-mfd bucket).
- **Transversal/graze noise (band_frac ≤ ~0.1)**: R0061 (664 fires @
  ~0.1 — the inc-4c dense chained-mint cluster, CORRECT), R0011
  (0.032, ERROR — fires on scale-1e4 sagitta, unrelated layer), F0048,
  R0091 (both CORRECT, single fires).

The raw FIRE criterion is therefore NOT promotable (fire set ⊄ broken
— R0061 alone adds 664 green fires); the §3.1 promotion decision at
inc-3 should add a band_frac (or amp<floor) gate, which by this sweep
yields fire set = {F0082, F0085, F0084, F0076} ⊇ broken ∩ class.
F0045 (the #171 candidate) does NOT fire — its defect is not an
osculating-pair weave on a cylinder patch.

## 8. inc-1 record (2026-07-21, unwired; production byte-identical)

Shipped `crates/yang-rs/src/stage5_envelope.rs` (`pub mod`, the
stage4_update unwired idiom) + `tests_unit/s188_envelope.rs` (9 tests, all
green). §3.2.1 + §3.2.2 as revised by §7.6; §3.2.3 (envelope chain
output) and all wiring remain inc-2.

### 8.1 What was built

- **`cylinder_two_plane_switch_points`** (§3.2.1): exact two-plane
  intersection line ∩ cylinder, closed form (Cramer on
  {plane, plane, line-direction·p = u·axis_point}, then the perpendicular
  quadratic). Fail-closed: `PlanesNearParallel`, `AxisParallelPairPlane`
  (no axial profile ⇒ also covers line ∥ axis), `NoTripleContact`,
  `TangentTripleContact` (hit separation < `TAU_MODEL`·scale),
  `UnsupportedSurface`.
- **`resolve_envelope_rule`** (op table, inc-4b `resolve_trim_beyond`
  style): same-side max-envelope vocabulary = `Union` (either owner) and
  `Subtract` with the patch on the BASE (partner-kept side = outside).
  `Subtract`-on-tool / `Intersect` (pair bounds OPPOSITE ends of the kept
  band — a pinch, not an envelope) and `Xor` fail closed `UnsupportedOp`.
- **`classify_bands`** (§3.2.2 per §7.6): liveness at a band sample, each
  support evaluated at ITS OWN curve point —
  int conic live ⟺ owner-extent (`sd_orig ≤ 0`) AND inside every wall
  (partner FACE extent, both existence tests, op-independent);
  orig conic live ⟺ partner-kept side (`sd_int ≥ 0`, op-resolved) OR
  beyond any wall (the §7.6 disjunct).
  Band boundaries = free-space triples + wall×curve crossing zeros
  (closed-form sinusoid zeros); adjacent same-liveness bands merge (their
  shared candidate is NOT a junction); both-live/neither-live bands where
  a wall passes BETWEEN the two curve points are `WallComplex` slivers
  (inc-4d's property — §3.3 must keep them byte-identical); otherwise
  loud `AmbiguousBand`. Triple ON a wall plane (±`TAU_MODEL`·scale),
  triple↔crossing coincidence, and sub-`TAU_WORK` deciding signs are
  `DegenerateBoundary`. Band samples avoid triple azimuths (a masked
  triple sits INSIDE a band; pair sd's vanish there without changing
  liveness — the wall disjunct covers the flip).

### 8.2 F0082 pinned-fixture results (all green)

Fixture from the inc-0 probe log: 9-decimal support planes; cylinder axis
FITTED from five top-rim verts (two independent circumcenter triples agree
to 2e-9; every pinned junction at radius 0.212325266 ± 1.5e-9).

- Switch points land on **v925 to <1e-7** (bit-exact output vertex) and
  the analytic **J3 to <1e-7**; θ = −1.088507 / +2.053086 (probe match
  <1e-5); antipodal to π within 2e-5; on all three surfaces ≤ 5e-9.
- **Both triples wall-masked** (§7.3): v925 → face-366, J3 → face-364,
  margins +1.2921e-3 (±1e-6, model-symmetric). No `FreeSpaceTriple`
  boundary exists.
- **Union band structure = 8 boundaries / 8 bands**:
  [v944·v923 | WC365] [ell] [v921·v949 | WC366] [rim] [v937·v943 | WC364]
  [ell] [v935·v945 | WC365] [rim] — the six exact wall-crossing junctions
  pinned to <1e-7 (v921/v949/v937/v943/v923/v935), the two rim×365
  near-crossing verts to <1e-5. The rim band runs STRAIGHT through the
  masked v925 AND through the absorbed wall-crossing pairs at u≈−0.228
  (v926/v951) and u≈0.229 — §7.4's "correct = rim straight v951→v959"
  falls out of the classifier with no special-casing. NOTE (new ground
  truth): wall face-365 ALSO crosses the tube (ell×365 = v923/v935
  exact, dist ≤1.6e-16 in the probe), giving the beyond-365 rim seam run
  (v953/v944, sd_365 ≈ +2.4e-2) — the correct loop has FOUR wall-complex
  slivers, not two; §2's "two-wall" narrative was incomplete.
- **Wall-free contrast**: with `walls = []` the switches sit AT the
  triples (2 `FreeSpaceTriple` boundaries, 2 bands) and θ=−2.0 / θ=3.0
  flip rim→ellipse — the §7.6 masking discriminator, red/green.
- **Op table red/green** on the live fixture: Union ok;
  Subtract-tool/Intersect/Xor → `UnsupportedOp` end-to-end.
- Synthetics: right cylinder + tilted plane (exact ±π/2 free triples);
  masked-triple wall takeover with axis-parallel-wall crossing dedup;
  degenerate reds (parallel planes, axis-parallel pair plane, line miss,
  exact graze, wall through triple, non-cylinder surface).

### 8.3 inc-2 notes

- The envelope CHAIN assembly (§3.2.3: ordered (curve, from, to) segments
  minted-by-identity onto existing verts) is NOT yet built — it needs the
  emitted-loop context (which output verts realize which boundary) and
  belongs with the §3.3 rebuild.
- `EnvelopeBands::live_at` is the diagnostic/wiring query surface.
- Wiring must map `emit_topology`'s supports to `EnvPlane`s with OUTWARD
  normals per owner — the probe's support dump already carries them; wall
  set = the §3.1 detector's neighboring planar patches of the PARTNER
  operand sharing loop verts (both A-side walls and, per the 365 finding,
  ALL crossing walls, not only those masking a triple).
