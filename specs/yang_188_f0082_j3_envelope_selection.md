# #188 — Stage-5/6 boundary-envelope selection for osculating curve pairs

**Status: SPEC (2026-07-21). Increments 0–4 below. Production untouched
until inc-3; every increment lands behind the standard ledger.**

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
