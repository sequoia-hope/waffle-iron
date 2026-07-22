# #188 — Stage-5/6 boundary-envelope selection for osculating curve pairs

**Status: SPEC (2026-07-21); inc-0 DONE (2026-07-21) — probe shipped, both
§7.9 open questions answered, and §2's "v925 = mandatory free-space switch"
premise REFUTED by measurement: see §7 before building inc-1. inc-1 DONE
(2026-07-21) — `stage5_envelope.rs` switch-point solver + §7.6 band
classifier, UNWIRED, all pins green: see §8. inc-2 DONE (2026-07-21) —
§3.3 selection rebuild WIRED gated (`YANG_S5_ENVELOPE_ENABLE`, off by
default), e2e synthetic + two hand-built weave fixtures green, gate-OFF
corpus byte-identical: see §9. Production untouched until inc-3; every
increment lands behind the standard ledger.**

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

## 9. inc-2 record (2026-07-21, gated `YANG_S5_ENVELOPE_ENABLE`, off in prod)

Shipped the §3.3 rebuild as **pure boundary SELECTION over existing
output vertices** (inc-0 §7.4: every junction the correct loop needs is
already minted — insertion is deliberately NOT implemented; a missing
junction vert bails). `stage5_envelope.rs` gains
`rebuild_osculating_loops` (+ `LoopRebuild`), hooked into
`emit_topology`'s curved branch after the probe; gate off ⇒ no call ⇒
byte-identical.

### 9.1 Mechanism

- **Detection (wiring form of §3.1)**: per cylinder patch, intersection
  conics on attributed loop edges are matched (either orientation) to a
  PARTNER planar patch for outward orientation; owner planar patches
  sharing ≥2 loop verts are original-conic candidates. A pair fires at
  `band_frac ≥ 0.7` with the C0118 floor measured from the ACTUAL loop
  edges (probe-identical computation). Exactly ONE osculating pair is the
  inc-2 vocabulary (F0085's two-pair loop bails → inc-3). Walls = ALL
  partner planar patches sharing ≥1 loop vert (§8.2's face-365 lesson).
- **Rebuild (per weaving cycle, ≥3 ambiguous verts)**: §3.2.2 bands →
  per live band keep the verts ON the live conic (TAU_EVAL-scale plane
  membership) in azimuth order; wall-complex slivers copy the ORIGINAL
  traversal byte-identically between their crossing junctions with
  **curve-adjacency pairing** (the entry junction is the crossing on the
  PREVIOUS band's curve even when it is the θ-far boundary — the F0082
  WC364 swap); free-space triples mint BY IDENTITY onto existing verts;
  standalone crossings emit both curves' crossing verts. Dead-side verts
  (on a pair curve but in the other's band) are dropped.
- **Fail-closed bails (`Ok(None)`, loop untouched)**: no/multiple pairs,
  classify error, missing junction vert, ambiguous wall-section path, a
  drop that would remove a vert on NEITHER pair curve (foreign
  subdivision). **Loud P10 errors after commit**: repeated vert,
  coincident adjacent pair, degenerate length, new edge inside a wall
  sliver (sites `s5-envelope-*` under `NONMANIFOLD_SITE_PROBE`).
- **Curve vocabulary**: NEW edges (absent from the original cycle) get
  the live support's analytic conic — the attributed intersection conic
  on the int side, the constructed `cylinder ∩ owner-plane`
  Circle/Ellipse on the rim side. Existing edges keep their attribution
  (the #158 LineSegment status quo is neither widened nor migrated).
- **Forensics**: `YANG_S5_ENVELOPE_PROBE` prints fired pairs, band
  tables, per-cycle orig→new vert lists, and every bail reason — the
  inc-3 triage instrument.

### 9.2 Evidence

- **e2e (`tests/s188_envelope_gate.rs`, own process)**: slab ∪ tilted
  tube (cap center ON the slab top, tilt 5e-3 ⇒ amp 1e-3 ≪ floor
  1.36e-2; triples exactly (0, ±0.2, 1)): gate-ON boolean succeeds; the
  probe shows the pair FIRING and the bottom cycle rebuilt; the emitted
  bottom cycle is simple, winds exactly once with zero azimuth folds,
  carries exactly the two triple junctions, and every other vert lies on
  the classifier's band-live support. NOTE: this benign case's gate-OFF
  loop is ALREADY correct — the rebuild reproduces it BYTE-IDENTICALLY
  (idempotence / no-damage), so the repair semantics are pinned by the
  hand-built fixtures instead.
- **Repair fixtures (`tests_unit/s188_envelope.rs`)**:
  `rebuild_drops_dead_side_detour_and_reorders` — a §7.4-miniature
  (dead-side ellipse overshoot past the triple + fold + dead rim vert in
  the ellipse band) rebuilds to the monotone alternating loop with both
  triple junctions and true conic vocabulary on exactly the new edges;
  `rebuild_keeps_wall_sections_byte_identical_with_swap_pairing` — a
  tilted wall (x − 0.05z = 0.6) yields two WC slivers whose crossing
  θ-order OPPOSES band adjacency on both sides (the WC364 shape): the
  healthy wall-arc traversals (physical micro-backsteps included) come
  through byte-identical while a dead ellipse vert in the far rim band
  is dropped.
- **Ledger**: gate-OFF full assay byte-identical (results.json
  unchanged); rewrite tier green; clippy/fmt clean.

### 9.3 inc-3 notes — F0082 gate-ON preview (measured 2026-07-21)

Single-case gate-ON run (`YANG_S5_ENVELOPE_ENABLE=1
YANG_S5_ENVELOPE_PROBE=1 ASSAY_CASE=F0082 … single_case`):

- **The flagship tube patch (info=373) rebuilds EXACTLY as designed**:
  pair fired with the §7.1 numbers; both triples WallMasked at
  +1.2920777e-3 (inc-1 pins to 11 digits); the 8-boundary band table
  matches §8.2 boundary-for-boundary; cycle REBUILT 30 → 28 verts — the
  §7.4 detour is REPAIRED (v926 dropped; v925 re-seated in monotone
  order between v960 and v951; the fold gone) and the near-dup stub
  v938 dropped (v932 survives on-curve).
- Three other osculating patches (info=326/360/2 — the case's earlier
  ops' benign pairs, the F0084/F0076 class) rebuild BYTE-IDENTICALLY
  (idempotence); one second pair on info=360 bails fail-closed
  (len-41 cycle, junction lookup miss) — honest, loop untouched.
- **The case moves to the next loud layer** (meta-oracle §6 satisfied):
  gate-OFF `TessellationFailed FaceId(3727) "ring rejected by CDT"` →
  gate-ON `InvalidBooleanOutput("an undirected output edge is not used
  by exactly two directed edges")` — the §9.3(a) risk realized: the
  dropped dead-side verts (v926/v938) still appear in NEIGHBOR patch
  loops (cap ring / wall ring), so the shared solid edges now subdivide
  differently. **inc-3's work = propagate the selection to the
  neighbors' copies of the shared chains** (drop the same dead verts
  from every loop that carries them), NOT re-adding the dead verts.
- Remaining inc-3 items: (b) if the render CDT still rejects after
  neighbor propagation, the §4.3-sliver dedup is the vehicle for
  residual micro-stubs; (c) the §3.1 detector-promotion decision from
  the §7.7 fire set (band_frac-gated) stands unchanged; (d) full
  gate-ON assay ledger + sidecar parity per §5.

## 10. inc-3 design (2026-07-22, measured before build)

### 10.1 The §9.3 "drop the same dead verts everywhere" instruction is
### WRONG as stated — measured refutation

Junction forensics (probe extension in `rebuild_cycle`, run on F0082
gate-ON) place the W0/W2 wall-crossing junctions at verts 937/943 and
921/949 — **the 925/926/951 cluster is NOT at a wall crossing**. It sits
mid-Orig-band at the **wall-masked triple** (θ = −1.0886; the stored
mask margin +1.2920777e-3 equals x(v925) − x(wall) exactly). The full
unpaired-edge set gate-ON (10 edges, measured via `KV2_OUT_TWIN_PROBE`)
decomposes into two sites:

- **W2-ellipse-run site (v938)**: v938 is a degree-2 pass-through in the
  planar world (both non-owner edges in face 363, same conic) — the
  plain splice `(932,938)+(938,937) → (932,937)` closes it, curve keys
  already agree. The §9.3 instruction is RIGHT here.
- **Masked-triple site (v925/v926/v951)**: v926 is a **degree-3 planar
  junction** (edges to 925 [face 362, EllSmall], 951 [face 368, EllBig],
  and wall-top 927 [shared 362/368]). No completion that erases v926
  can close: exhaustive case analysis leaves odd edge-use parity at
  v925/v951 or puts an off-plane vert (gap ≈ 1.05e-4 ≫ the TAU_MODEL
  planarity band) into a planar loop (`s6-planar-loop-nonplanar`).

### 10.2 The measured completion — the fold is a NOTCH; pinch-split
### (final form, reached through three refuted intermediates: see §10.6)

The fold segment `[951 → 926 → 925]` (θ-backward run) is not a defect
to erase — it is the boundary of a NOTCH: the strip region between rim
and ellipse at the masked triple has NO kept mesh surface (measured:
zero kept triangles inside `{951,926,925}`), so it is genuinely not
part of the tube face. The repaired owner is the PINCH-SPLIT of the
original self-touching ring:

- **Main chain**: azimuth-monotone on-live-conic members —
  `[…960, 925, 959…]` (30→27). BOTH non-pinch fold verts leave the main
  chain (951 and 926); only the PINCH (v925, on both pair curves — the
  osculation point) stays. The main hop `(925,959)` is an ORIGINAL
  gate-OFF edge — 370's chord pairs it untouched.
- **Notch cycle**: the fold run byte-identical + a closing band-conic
  arc — `[951→926, 926→925, 925→951(Circle)]` — emitted as an
  ADDITIONAL CYCLE of the owner patch, i.e. an INNER LOOP (hole). Its
  material-CW winding is exactly what a hole must do (the curved-branch
  validator confirmed: as a standalone face it fails material-CCW; as a
  hole it passes). Its run edges keep their original EllSmall/EllBig
  attributions, pairing 362/368 byte-identically.
- **Neighbor rewrite — ONE rule**: replace each stale maximal run of
  old-owner-chain edges (EXCLUDING notch edges) with the new main
  chain's sub-path between the same endpoints, filtered to verts on the
  neighbor's surface. A run endpoint living only on the notch (the
  non-pinch anchor, e.g. 371's 951) enters the rail THROUGH the notch's
  closing edge: 371's chord `(951,960)` → `(951,925),(925,960)`; 363's
  chain splices out the pass-through v938; 362/368/370 are
  byte-unchanged.
- **Curve keys**: per-(info, cycle, pair) override map (one vert pair
  may carry different curves on different loops); each neighbor mirror
  of an owner edge resolves to the owner's curve for that edge (owner
  list in cycle order; multiply-typed pairs consumed in order,
  probe-logged); both emission branches consult the map first.

### 10.3 The keep/drop/notch rule (replaces "on the live conic" alone)

For each maximal ORIGINAL-cycle run of verts dropped by the live-conic
member selection:
- **no planar junction in the run** (every vert has non-owner edge
  degree ≤ 2): plain drop — the neighbors' chains splice (v938);
- **run contains a planar junction** (degree ≥ 3, e.g. v926 carrying
  the wall-top edge): the run becomes a NOTCH cycle
  `[prev, run…, next]` + closing hop. Exactly one anchor must be the
  PINCH (on BOTH pair curves); the other anchor is removed from the
  main chain (keeping it provably pinches its vertex umbrella into two
  cones — an odd-χ assembly, measured). No unique pinch ⇒ fail closed.

The band table itself is UNCHANGED (the inc-1 8-boundary table stays
authoritative; no new boundary vocabulary).

### 10.4 Architecture and fail-closed contract

The gated rebuild moves from a per-patch call inside `emit_topology`'s
curved branch to a **pre-pass**: (1) run detection+rebuild for every
curved patch against the pristine `subdivided_cycles`; (2) run neighbor
propagation, producing rewritten cycles for affected infos plus the
global curve-override map; (3) the main per-info loop consumes the
rewritten cycles in both branches with no per-info rebuild call.

Fail-closed (revert EVERYTHING for that owner — gate-ON can then not be
worse than gate-OFF): a stale run whose endpoints are missing from the
new chain; a plane-filter that would drop a kept junction; a rewrite
that repeats a vert within one loop; two rebuilds touching the same
neighbor edge; and a final **local pairing audit** — every touched
undirected edge must have exactly 2 uses with equal curve keys, and
every substituted vert must satisfy its face's TAU_MODEL planarity
band. Audit failure ⇒ bail (the untouched loops keep the old loud
STOP). Post-commit violations remain loud typed `s5-envelope-*` errors
(P10).

### 10.5 inc-3 record (2026-07-22, gated; production byte-identical)

Shipped the §10.2/§10.3/§10.4 machinery: pre-pass in `emit_topology`
(owner rebuilds → notch split → neighbor propagation → pairing +
planarity audits; per-(info,cycle,pair) curve overrides consulted by
both branches). Gate-OFF: F0082 byte-identical (same
`TessellationFailed FaceId(3727)`); yang-rs 74/74 binaries + rewrite
tier green.

**F0082 gate-ON peeled FOUR defect layers, each measured**
(`KV2_OUT_TWIN_PROBE` / `KV2_RING_REJECT_PROBE` forensics):
1. `InvalidBooleanOutput("…exactly two directed edges")` — closed by
   neighbor propagation + notch split (all 2-use).
2. `…two OPPOSITE directed edges` on (925,926) — REFUTED the Option-W
   "junction rides the main chain" design: the zig arcs must run
   θ-backward (that is what the fold was); hence the notch SPLIT.
3. `Euler characteristic not genus-representable` — REFUTED keeping the
   non-pinch anchor (951) on the main chain: its umbrella pinches into
   two cones (odd χ). Hence the §10.3 pinch rule.
4. `CurvedGeometryMismatch "exactly one material-CCW loop"` — REFUTED
   emitting the notch as its own FACE (it has no kept mesh surface —
   phantom); as an INNER LOOP its CW winding is correct.

**Landing state**: every boolean-output validation gate in `from_yang`
passes (pairing counts, orientations, curve-key lens, Euler/genus,
cylinder-patch winding). The case now STOPs one layer deeper, in
RENDER tessellation: `TessellationFailed FaceId(3727) "ring rejected by
CDT"` — the notch hole shares the pinch vertex v925 with the outer
ring, and `cdt_polygon_with_holes_floodfill` rejects vertex-touching
holes. The vertex umbrella at 925 is a SINGLE 5-edge cycle
(surface-manifold); the self-touch is face-level only — a legitimate
pinched-ring B-Rep the render CDT cannot yet triangulate.

### 10.6 inc-4 = pinched-ring render tessellation (kernel-v2)

The remaining F0082 blocker is a CAPABILITY gap in kernel-v2's
`triangulate_ring`: support a hole sharing exactly one vertex with the
outer ring (keyhole decomposition at the shared vertex, or
flood-fill-CDT admission of coincident ring points). Everything
upstream of render is repaired and validated. Then: full gate-ON assay
ledger + §7.7 detector-promotion decision + flip per §5.

### 10.7 inc-4a record (2026-07-22) — capability SHIPPED; §10.6's
### "sole blocker" premise REFUTED by measurement

**Shipped always-on (kernel-v2, production gate-OFF byte-identical —
see assay note):** pass 1.5 *shared-vertex canonicalization* in
`tessellate_developable_patch`. Root cause class: each loop walk
unrolls its azimuth independently, so ONE B-Rep vertex on TWO loops
of a face (the pinched ring) is minted as two `PatchNode`s whose `u`
differs — by Δθ-accumulation rounding (measured 2.3e-15 on F0082
face 3727: outer[17] vs hole[2], heights bitwise-equal), or by a FULL
SPAN when the later walk's atan2 anchor picks the other (−π, π]
branch. Consequences pre-fix: (a) the §6b M3b flood-fill weld —
which requires BITWISE coincidence (spade `insert` merges exact
positions only) — engages only by luck; (b) even a lucky weld leaves
the two copies with DISTINCT node ids, so the refinement's
boundary-kind registry misses the pinch-adjacent hole edges (silently
`Interior`) and lifts their split midpoints onto the SURFACE instead
of the 3D chord — a ~1e-3 sagitta conformality crack vs the
neighboring face's copy (silent-wrong class, P9). Fix: identity is
exact (same 3D position bits, never a distance band); per chain the
first match fixes the seam-window offset `k = round(Δu/span)`, the
chain is rigidly translated by `−k·span` (wrap chains and
double-anchored chains reject loudly), matched entries re-point to
the canonical node, pinned chains skip the mid-window shift, and in
the cut-frame (2-wrap) branch pinned holes re-seat their shared
vertices on the ring copy EXACTLY with rigid Δu translation of the
rest. Tests `pinched_ring_patch_tests.rs`: same-window barrel,
across-seam-branch barrel (both RED pre-fix via the conformality
pin), bounded-branch notch.

**Measured refutation.** With the pinch bitwise-welded (probe: hole[2]
= outer[17] to the bit), F0082 still STOPs at the same
`TessellationFailed FaceId(3727)` — and exact orientation on the
probe ring shows why, and why it MUST: the notch is not a contained
tangent hole. Against outer edge 17→18 (the rim run u 0.3337→0.4116,
y≈0): h0 = (0.336466, +3.3e-16) lies strictly ABOVE it (margin
5.6e-14), h1 = (0.336462, −1.053e-4) lies far BELOW it, so hole edges
h0→h1 and (pre-weld) h2→h0 CROSS the outer boundary. The emitted
inner loop escapes the outer cycle — the outer ring retains the rim
run THROUGH the notch's u-range while the notch strip (the beyond-wall
phantom, thickness 1.05e-4) hangs OUTSIDE (below the rim). The
kernel-v2 reject is therefore CORRECT (P10): no CDT capability can
admit a hole that crosses its outer ring. inc-3's layer-4 resolution
("emit the notch as an inner loop") satisfied the pairing/winding
gates but is GEOMETRICALLY inconsistent as a face-minus-hole.

**Corrected next layer (inc-5, yang-rs §10.3 revision).** The notch
must reach render as a CONTAINED pinched ring or not at all. Evidence
for the shape of the fix: the ANTIPODAL step of the same magnitude
(outer indexes 24→27 at u≈1.0035, Δy = +1.053e-4 — the same
beyond-wall margin) is woven INTO the outer cycle. The strip's
boundary treatment must decide between (a) splitting the rim run
17→18 at the strip's u-range and routing the outer along the strip's
upper boundary (rim run under the strip goes to the removed side), or
(b) resolving the strip's edges entirely within the wall-face complex
so the tube face never carries them. Decide against the §2 contract +
the kept-mesh oracle (the strip has ZERO kept tris; whatever routing
is chosen, the rendered area must equal the kept-mesh area). NOTE: a
pre-existing stash `canon_u chart weld (inc-4 WIP, unvalidated)` held
an earlier band-based (0.5·w_facet) draft of the kernel-v2 half; it
is superseded by the exact pass-1.5 canonicalization and should be
dropped.
