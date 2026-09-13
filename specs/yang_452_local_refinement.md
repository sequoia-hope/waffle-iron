# §4.5.2 Local Refinement — Customer Census & Adjudication

**Status: CENSUS (2026-08-29).** Roadmap item 3d/4 (`docs/yang_functional_roadmap.md`
§0.0 phase 3d + the after-epic item in `specs/yang_441_trim_cdt_construction.md` §5)
names the §4.5.4-removal / §4.5.2 guard-shell loop as the recorded next structural
item after the §I13(f) epic closed (canonical 273C/0W/34E/1EE/0T). Per the standing
discipline (case-first; no wiring against an unconfirmed bucket), this spec measures
the question that decides whether the recovery loop gets built at all:

> **Is any member of the current Stage-4 STOP family density-limited — i.e., would
> the paper's §4.5.2 "increase the mesh resolution of the parametric surfaces
> associated with the erroneous regions and re-optimize" actually convert it?**

## 1. Paper contract (what §4.5.2 IS)

`refs/text/yang2025_hybrid_boolean.txt:652-670` (§4.5 collect-then-repair loop +
termination argument) and `:659-680` (§4.5.2 proper): after optimization, collect
point pairs that cannot converge to distance 0 within their domains; §4.5.1 applies
only when the failure region is bounded by two successfully optimized points on the
SAME surface **and** the failure points are interior (boundary points gliding along
boundary curves are Fig-13-excluded, `:637-651`); everything else takes §4.5.2:
refine the surfaces traversed by the failed segment C_p plus a one-ring of
neighbors, recompute mesh intersections in the refined regions, splice the improved
polyline between the bounding points p_f/p_b, re-optimize; repeat while failure
persists. Termination: mesh intersections converge to the true surface
intersections under refinement.

The binding guard-shell contract (`docs/yang_junction_research_findings.md` Q3,
restated at roadmap 3d): transversality entry gate, per-pass strict-decrease
monitor, budget, watertight/oracle-gated output — refinement may only STOP, never
silently accept. Q3's 2026-07-17 prediction: §4.5.2 recovers ~zero current cases
(every confirmed customer is tangential / missing-solver / micro-feature). This
census re-tests that prediction against the post-I13f tail, where the I10/I11/I12
selector censuses had since assigned the whole `Stage4RegionInvalid` family to
§4.5.2 by the paper's own selector.

## 2. The family (canonical 2026-08-29 report)

10 of the 34 canonical ERRORs carry `Stage-4 relocation region around vertex N is
invalid`, three reason sub-kinds:

- `OffCurveBeyondChordBand`: R0015, R0074, R0077, C0065
- `RelocationCrossedCarrierVertex`: R0011, R0044, R0085 (×2 ops)
- `LocalRefinementRequired`: R0038 (sentinel vertex), R0050, C0067

## 3. Instrument — `YANG_CHORD_REFINE` (the uniform density ladder)

`chord_rel()` in `crates/yang-rs/src/stage1_tessellate/normals_chord_bounds.rs` is
now the ONE home of the `1e-2` relative chord-bound base (A14.3): every
`*_chord_bound` (circle-rim AABB, ellipse/hyperbola, sphere, cone; the torus path
routes through `ellipse_chord_bound`; the stray stage-1 hyperbola literal now calls
it too). Debug builds honor `YANG_CHORD_REFINE=<f>` (f ≥ 1): every chord bound
divides by f, so ALL curved tessellation densities refine uniformly (≈ √f more
segments) and every derived Stage-3/4/6 band tightens consistently
(`fix_all_gates_sharing_a_metric`). Release builds compile the knob out.

Why this and not `YANG_NSEG_FLOOR`: the floor only lifts the circle-chain branch —
sphere/cone/torus faces never feel it (measured: C0067's sphere at floor 96 is
byte-identical, same STOP vertex 128).

**Semantic note (deliberate):** the knob co-scales mesh error AND acceptance bands,
exactly as the paper's refinement does (both derive from d_ε). The ladder therefore
answers "is the defect structural relative to its own band" — the §4.5.2 question —
NOT "would a finer mesh pass the natural-density bands" (which would be tolerance
widening through the back door, P9).

Rungs: f=4 (≈2× segments), f=16 (≈4×), f=64 (≈8×, where informative). Debug
`single_case` runner; natural-density debug verdicts reproduce the canonical
release ERRORs (spot-anchored on C0067/R0038/R0011).

## 4. Census (measured 2026-08-29)

| case | natural (canonical) | f=4 (2×) | f=16 (4×) | f=64 (8×) | site (YANG_LRR_PROBE) |
|---|---|---|---|---|---|
| R0011 | CrossedCarrier v27 | CrossedCarrier v38 | CrossedCarrier v96 | — | `stage4_correct.rs:5506` — the §4-I9 corner-crosser STOP |
| R0015 | OffCurve v84 | OffCurve v173 | OffCurve v313 | — | `stage4_correct.rs:8462` — torus arm (wedge gate / partner-hull) |
| R0038 | LRR sentinel | LRR sentinel | **completes; WRONG χ=2 vs telescoped 4 (2 shells); watertight+volume PASS** | **same, stable** | `stage4_correct.rs:9489` — #168 degenerate-cylinder-cap wall |
| R0044 | CrossedCarrier v8 | CrossedCarrier v99 (517s) | CrossedCarrier v121 (785s) | — | (same corner-crosser family as R0011) |
| R0050 | LRR v125 | **input-B-Rep-not-2-manifold** | **same** | — | upstream producer regresses under refinement; op-2 site unmeasurable globally |
| R0074 | OffCurve v91 | OffCurve v112 | CrossedCarrier v122 | — | flips between the sibling reasons; persists |
| R0077 | OffCurve v154 | OffCurve v475 | **completes; WRONG (167/167 unpaired edges, volume rel 7.4e-1)** | **OffCurve v6005** | rung accident at f=16 — see §5 |
| R0085 | CrossedCarrier v387 + op-3 failure | CrossedCarrier v393 + op-3 LRR v8 (666s) | (uneconomic at debug speed) | — | (corner-crosser family; op-3's reason shifts within the family) |
| C0065 | OffCurve v8 | OffCurve v81 | OffCurve v57 | — | `stage4_correct.rs:8462` — the #137-anchored partner-hull wall |
| C0067 | LRR v128 | LRR v449 | LRR v1847 | — | `stage4_correct.rs:7354` — coplanar disc∩disc junction: exact circle∩circle returns None (tangent/graze — no corner exists) |

## 5. Adjudication

**§4.5.2-as-recovery has ZERO customers in the current Stage-4 STOP family.**
Findings Q3's prediction holds after every I-series increment:

1. **Persist class** (R0011, R0015, R0044, R0074, R0085, C0065, C0067): the SAME
   typed STOP survives 2×–4× uniform refinement (vertex ids move with the mesh;
   the configuration reproduces at every rung). Site attribution says why —
   these are tangency / junction-topology gaps, not approximation error:
   - C0067: a coplanar circle∩circle junction with no analytic corner
     (tangent/graze). Refinement cannot mint a corner that does not exist —
     the Q3 transversality entry gate would DECLINE this case.
   - R0011/R0044/R0074(f16)/R0085: the §4-I10 corner-crosser anatomy — the
     traveller rides its carrier model edge chasing a far surface whose zero
     lies past the edge's endpoint. Scale-free in d_ε. Same-day
     `YANG_S4_CARRIER_DOMAIN=census` probes over R0011/R0044/R0074 sharpen
     the shape to ONE uniform configuration: the traveller carries
     {far-surface, base-face, facet_k} and crosses the still MODEL CORNER
     {base, facet_k, facet_k±1} of a many-facet chained operand (R0011:
     giant-cylinder base B:1 + planar facets, far Plane; R0044: cone-band
     facets, far Cone; R0074: operands swapped, traveller on A's edge chasing
     B:2) — and the corner junction ALREADY EXISTS as a mesh vertex (the
     `TruncateAtVertex` answer names it, t = 0.14–0.32 of the travel). The
     missing capability is corner TRANSIT — truncate at the existing corner,
     swap facet_k → facet_k±1 in the constraint set, re-solve the crossing
     analytically on the next edge's carrier, re-route the loop through the
     corner — i.e. the paper's Fig-12(c–e) mechanism transposed to a corner,
     made deterministic by the analytic constraint sets the paper's mesh-only
     §4.5.2 lacks (the `feedback_yang_brep_extension_over_cherchi_pure_mesh`
     theme). Epic #169 / I13f-rehome vocabulary applies to the re-route half.
   - R0015/C0065: the torus near-tangency arm (`tangent_plane_corridor` /
     `planar_partner_hull_contains`) — the wedge gate and the mesh both scale
     with d_ε; the near-tangent loop-closure race is invariant. Owner: the
     §4.3.3 tangent-point insertion milestone (or P10 sign-off).
2. **R0038 is the single empirically density-limited member** — at 4× and 8× the
   #168 degenerate-cap wall dissolves and the case completes END-TO-END with
   watertight + volume + monotonicity PASSING and a LADDER-STABLE topology of
   2 shells / χ_total = 2 (per-op χ-audit: op-2 severs the boss into two χ=2
   pieces; op-3's coaxial torus cut turns one piece genus-1, χ=0 minted by the
   exact stage-2 arrangement, all edges 2-used). The remaining oracle flag is
   the telescoped χ=2·shells expectation — the same formula class the R0003
   `expected_shell_count` adjudication fixed. A genus-1 ring is geometrically
   plausible for a coaxial torus cut punching an annular tunnel; the closed-form
   handle certification (stage4_slit precedent) is DEFERRED — it cannot move the
   canonical corpus while the natural-density wall stands, and R0038's recorded
   owner stays #169 C/D (the banked `YANG_N2_RECDT_ENABLE` re-CDT's re-entry
   note). If a future increment clears the natural-density wall, adjudicate the
   authored shell count THEN, under the `historical_authoring_fixes_pinned`
   protocol.
3. **Refinement UNMASKS downstream/upstream defects rather than fixing them**:
   - R0077 at f=16 slips past the Stage-4 STOP into a catastrophically broken
     output (100% unpaired edges, volume off 74%) — caught by the in-line
     composition oracle; at f=64 the STOP returns. The natural-density STOP is
     GUARDING real garbage (P10 vindicated); any future refinement loop must
     keep the full oracle gate, exactly as Q3's contract demands.
   - R0050 under refinement fails EARLIER: the op-1 producer emits a
     non-2-manifold body at 2×–4× (a latent upstream defect the natural density
     masks), so the op-2 site is unmeasurable by a GLOBAL ladder. A faithful
     per-boolean-local §4.5.2 would not perturb op-1; recorded as the one
     methodological caveat of the uniform knob (adjudicable later with per-op
     scoping if ever needed).

**Consequence for roadmap 3d/4:** do NOT build the §4.5.2 recovery loop now — it
would convert 0 of 34 ERRORs at the cost of a second full pipeline pass per STOP,
and the guard-shell posture (the existing typed STOPs + in-line oracles) already
implements the paper-faithful "STOP, never accept" stance. The §4.5.4 REMOVAL half
(the `YANG_SELFX_PROBE` fire-list: relocation-minted seam chord-crossings on ~33
CORRECT cases) is a DIFFERENT customer set this census did not measure — it stays
open as its own item.

**Ownership routing out of this census** (the tail's real owners, by size):
- Corner-crosser transit (R0011, R0044, R0074, R0085; R0015/C0065 are the
  torus-tangency variants): a DIRECT KIN of the §I13(f) inverted-junction-pair
  family, presenting at the §4-I9 carrier-domain postcondition instead of
  I13d's certificates. The R0011 v27 probe shows the relocation MINTED an
  exact triple junction {A:2, B:1, B:212} (post is on all three surfaces,
  d ≤ 3e-14 at scale ~5e3) that is a PHANTOM — outside facet B:212's
  rim-bounded extent, past the still model corner {B:1,B:212,B:213} which
  already exists as a mesh vertex — while the TRUE junction {A:2, B:1, B:213}
  belongs on the adjacent facet. That is the I13f anatomy verbatim (exact
  solve outside its band's domain; true topology needs the mirror crossing on
  the adjacent band), so the always-on `YANG_441_REHOME` recognize-and-rehome
  machinery is the natural vehicle; the epic's inc-0 is a recognizer-
  feasibility census (per site: does the adjacent-facet junction solve
  converge in-domain — the I13f f1-planner analog) plus a measurement of why
  the I13d certificate layer does not currently claim these sites. First
  answer, measured same-day: `YANG_441_REHOME=census` on R0011 prints
  NOTHING — the rehome census hooks the I13d selector's `not_richer` branch
  (`stage4_rehome.rs` header), and R0011's flow never enters the I13d
  selector at all; the phantom is caught only by the §4-I9 stage-end
  postcondition. The epic's recognizer therefore needs its own hook at the
  I9 detection (or pre-emptively at the relocation arms), not a widening of
  the I13d branch. **Epic opened: `specs/yang_451_corner_transit.md`** (inc-0
  feasibility census landed same day).
- #168 degenerate-cap wall (R0038): epic #169 C/D two-sided junction-aware
  re-CDT.
- Tangent disc∩disc (C0067): §4.3.3 tangent-point insertion milestone.
- R0050: the upstream producer's refinement-latent non-manifold emission is a
  NEW recorded latent (fires only under the dev knob today).

## 6. Increment 1–2 (2026-09-13) — the `d_ε` rung primitive, the op-level
## refinement pass, and the SECOND adjudication: R0050's op-3 wall is TANGENCY,
## not a resolution deficit

**Why re-open.** §5's adjudication measured the family as it stood on
2026-08-29. R0050's row there was its **op-2** wall (`LRR v125`), and the
ladder read `input-B-Rep-not-2-manifold` at f=4/f=16 — recorded as
"upstream producer regresses under refinement; op-2 site unmeasurable
globally", i.e. NO verdict for R0050. The 2026-09-12 triple-block fix
(`docs/yang_tail_triage.md`, R0050 op 2 → op 3) retired that wall and exposed
a NEW one: `RelocationCrossedCarrierVertex v413`. A new member of the §4-I9
fire list gets its own ladder.

### 6.1 The site, measured (`YANG_S4_CARRIER_DOMAIN=census` + the new
### `YANG_451_CORNER_PROBE`)

R0050 op 3 = the Revolve-3 auto-union. TWO symmetric fires, `(413, 209)` and
`(496, 228)`, identical metrics (travel 6.9169e-2, overrun 3.7229e-2). Both
DECLINE `NoRealCandidate` — the corner-transit planner finds no junction on a
corner-incident model edge, so the epic's repair has nothing to plan.

The operands (`YANG_451_CORNER_PROBE` prints each face's surface and every
loop edge with `q`'s own reading against it):

| face | surface |
|---|---|
| A:5 | Torus R = 3.9508518457613926, r = 2.6339012305075946, axis (0.8096, 0.5870, 0) |
| A:6 | Plane n = (−0.8096, −0.5870, 0) — ⊥ the axis (a revolve end cap) |
| A:9 | Plane n = (0.4651, −0.6415, 0.6101), d = −12.335 — CONTAINS the axis |
| B:2 | Torus R = 3.7759280618729063, r = 2.517285374581937, SAME axis direction |

`q = v209` is EXACTLY A's B-Rep vertex 76 (nearest-vertex distance **0.0**) —
a true model corner where A:5, A:6 and A:9 meet. A:9 is the **meridian disc**
of A's torus (radius r = 2.6339 — its rim circle lies on A:5 to ≤ 8.9e-16)
with three straight CHORDS bitten out of it by earlier ops: its 6-edge loop is
v76 –seg– v41 –arc51– v40 –seg– v86 –arc100– v85 –seg– v77 –arc91– v76, the
three arcs being arcs of ONE circle at θ = [0, 1.9363], [3.1416, 3.3033] and
[4.9162, 5.0779] (θ measured CCW from v40) and the three segments being chords
across the gaps.

Both candidate corrected triples converge and BOTH land outside A:9's face:

- `{B:2, A:6, A:5}` → 5.6417e-1 from q, 4.775e-2 off the nearest edge's line:
  not on a model edge at all.
- `{B:2, A:9, A:5}` → ON the meridian circle to **1.404e-15**, at θ = 3.0597 —
  inside the gap (1.9363, 3.1416) that the chord v41–v76 cuts away. The
  nearest-edge ranking reports it against arc 51 (`d_q_end` 2.986,
  `not-corner-incident`) because `curve_aware_distance` measures the FULL
  circle and arcs 51/91 tie at 1.4e-15; read against the corner-incident arc
  91 instead it is `arc-cw-only`. Either reading refuses, and correctly: the
  point is outside the face.

**The exact crossings of B:2 with A:9's whole boundary are TWO, both far from
q** (bisection along every one of the 6 edges): interior to seg v76–v41 at
t = 0.84306 and interior to seg v40–v86 at t = 0.15694; no arc carries one.
`d_B2` at the six model vertices is v40 +0.26684, v41 +0.26684, v76 −0.01863,
v77 −0.05288, v85 −0.05288, v86 −0.01863 — the corner q sits 1.863e-2 INSIDE
B, and the whole v76/v77/v85/v86 side of the face is inside B. So the mesh
crossing at v413 (t = +0.0107 on the chord, 3.194e-2 from q) is SPURIOUS: the
exact torus crosses that chord's LINE 3.7229e-2 PAST q, outside the segment,
and B's mesh sits 3.4e-2 off its own surface there (within its honest
`torus_chord_bound(3.7759, 2.5173)` = 6.2932e-2).

### 6.2 Increment 1 — the `d_ε` rung primitive (LANDED, byte-identical)

`chord_rel()` was the paper's `d_ε` base with a DEBUG-ONLY env knob. It is now
`CHORD_BASE / chord_refine_scale()`, and the scale is a composable,
panic-safe, thread-local rung set by
`stage1_tessellate::with_refined_chord(factor, body)` — Yang §4.5.2's
"increase the mesh resolution of the parametric surfaces" expressed as the ONE
quantity the paper uses. Refining `d_ε` moves the mesh density AND every
derived Stage-3/4/6 membership band together, which is what the paper's single
`d_ε` means and what `fix_all_gates_sharing_a_metric` demands. Factors ≤ 1 are
clamped (refinement only — coarsening the mesh while loosening every band is
P9 through the back door); nesting composes; the rung is restored on unwind.
Pinned by `tests_unit::s452_chord_refine` (5 tests). Production reads the
natural rung, so the change is byte-identical; the debug `YANG_CHORD_REFINE`
ladder composes with it unchanged.

`BRep::retessellated_at_current_d_eps()` re-derives a B-Rep's Stage-1
discretization at the rung in force, topology untouched, preserving any
phantom-guard `forced_rim_n`.

### 6.3 Increment 2 — the op-level refinement pass under the Q3 guard shell
### (LANDED, GATED OFF)

`boolean::refine_452` (`YANG_452_REFINE`: unset/other = off, `census` = run
every rung and report, `1|on` = the dev A/B adopt arm; `YANG_452_ROUNDS=3,6,8`
overrides the ladder for census; `YANG_452_PROBE` reports on the adopt arm).

- TRIGGER: the paper's own — `Err(Stage4RegionInvalid{..})`, our typed form of
  "the point pairs that cannot converge to a distance of 0 within their
  domains" (`refs/text/yang2025_hybrid_boolean.txt:648-651`). §4.5.1 has
  already refused by then, which is the paper's ordering (`:665-668`).
- ACTION: re-tessellate BOTH operands at `d_ε/f` and re-run the op. **The
  operand rebuild is load-bearing**: `boolean_once` consumes `a.as_mesh()`, so
  wrapping it in the rung alone tightens every band against a mesh that is
  still as coarse as before and manufactures fresh `OffCurveBeyondChordBand`
  STOPs — measured on R0050 before the rebuild landed. The mesh and the bands
  must move together.
- GUARD SHELL (`docs/yang_junction_research_findings.md` Q3): clause 2, the
  per-pass strict-decrease monitor on the unpaired-undirected-edge count
  (Q3 names "unpaired-edge count / |χ−2|"; the unpaired count is the half
  valid at ANY genus — `|χ−2|` presumes genus 0 and would misjudge every
  handle-carrying union in the corpus); clause 3, the rung budget; clause 4,
  output adopted ONLY at functional zero. Clause 1, the transversality entry
  gate, needs the failing site's geometry, which the typed error does not
  carry — deferred, and a cost (a bounded futile ladder) rather than a
  correctness hazard, because of clause 4.

### 6.4 The op-level ladder on R0050 — NO CONVERGENCE

`YANG_452_REFINE=census YANG_452_ROUNDS=1.5,2,3,4,6,8,12,16` (release, 2.9 s):

| rung | operands (tris) | result |
|---|---|---|
| d_ε/1.5 | — | `Err RelocationCrossedCarrierVertex v492` |
| d_ε/2 | a 558→758, b 392→722 | **Ok**, unpaired = 0, **improper = 55** → the kernel-v2 render gate rejects it (`SelfIntersectingBooleanOutput` FaceId 27/38) |
| d_ε/3 | — | `Err OffCurveBeyondChordBand v324` |
| d_ε/4 | a 558→1764, b 392→1404 | `Err OffCurveBeyondChordBand v547` |
| d_ε/6 | — | `Err OffCurveBeyondChordBand v685` |
| d_ε/8 | — | `Err OffCurveBeyondChordBand v775` |
| d_ε/12 | — | `Err OffCurveBeyondChordBand v1254` |
| d_ε/16 | — | `Err OffCurveBeyondChordBand v1197` |

Every rung from d_ε/3 up fails on the SIBLING out-of-domain reason at a
vertex that moves with the mesh. The one rung that emits a body emits an
illegally self-intersecting one. The guard shell therefore reports
`BUDGET EXHAUSTED` and the standing Stage-4 STOP stands — which is the
designed behaviour, and the honest one.

### 6.5 The WHOLE-CASE ladder OSCILLATES — and the reason is EXACT TANGENCY

The whole-case debug ladder (`YANG_CHORD_REFINE=f`, all three ops refined) is
not monotone:

| f | 1 | 2 | 3 | 4 | 5 | 6 | 8 |
|---|---|---|---|---|---|---|---|
| R0050 | ERROR (CrossedCarrier v413) | ERROR (non-2-manifold) | ERROR (CDT ring reject F34) | **CORRECT** (51.3 s) | ERROR (SelfIntersecting 27/38) | **CORRECT** (57.8 s) | ERROR (CDT ring reject F29) |

Two rungs out of seven pass, with failures of three different kinds between
them. That is the #137 "χ wanders under refinement" signature, not
convergence, and Yang's termination guarantee (`:668-670`) does not cover it
— the guarantee holds for TRANSVERSAL intersections only (Q3; yang2023 §5.4
certifies refinement does not converge near tangency).

**The certificate: A:5 and B:2 are EXACTLY TANGENT.** Their axes are parallel
(axis-direction identical; axial offset of the centres 2.8e-16) and offset
perpendicular by 0.1749237839 — which equals `R_A − R_B = 3.9508518457613926
− 3.7759280618729063 = 0.1749237839` **exactly**. Minimising |d_B2| over a
400×400 sample of A's torus and refining the minimum gives **0.0000e+00** at
(u, v) = (247.51°, 239.40°); the separation over the rest of the surface runs
up to 0.4663. So the operands touch.

The v413 SITE itself is transversal (|n_B2 × n_A9| = 0.999521, 88.23°;
|n_B2 × n_A6| = 0.495790, 29.72°; the crease pierces B at 29.7°), so a
per-site transversality gate would ADMIT it — the non-convergence comes from
the tangency elsewhere on the same surface pair, which the whole-op
re-tessellation is subject to.

### 6.6 A REFUTED discriminator, recorded

`YANG_S4_CARRIER_DOMAIN-RESOLUTION` (census) reports, per §4-I9 fire,
`|d_far(q)|` against the far FACE's own Stage-1 chord bound
(`stage4_correct::face_chord_bound`), testing "can the far mesh even decide
which side of itself the corner is on?" It separates R0050 (ratio 2.96e-1)
from nothing: **R0044**, a CONVERTED corner-transit case, reads
`UNDER-RESOLVED` at every one of its 6 sites (ratios 1.5e-1 … 6.1e-1),
because its far face is a Cylinder whose band is the whole-solid circle-rim
AABB × 1e-2 (72.67) — a per-SOLID quantity that says nothing local. The test
is NOT a discriminator; the printer is kept as the evidence. The planner's own
`NoRealCandidate` verdict remains the real separator, and R0085 shows it is
per-SITE, not per-case: 6 of its 9 op-2 fires decline `NoRealCandidate` while
3 classify and then decline at the corridor walk (`AmbiguousExit`).

### 6.7 SECOND ADJUDICATION

**§4.5.2-as-recovery still has ZERO customers**, now including the members
that arrived after 2026-08-29:

- **R0050 (op 3) — TANGENCY, not a resolution deficit.** Re-diagnosed above
  with an exact certificate. Vehicle moves from the §4.5.1 corner-transit epic
  (which correctly refuses it: `NoRealCandidate`) to the **§4.3.3
  tangent-point insertion milestone** — the same owner §5 already assigns to
  the R0015/C0065 torus near-tangency arm. Adopting either lucky whole-case
  rung would be a right answer for a wrong reason (P9).
- **R0085 (op 2) — PERSIST class, confirmed on today's tree.** Whole-case
  f=1 → two failures (op 2 `CrossedCarrier v386`, op 3 non-2-manifold);
  f=2 → ONE failure (op 3 clears; op 2 `CrossedCarrier` persists, v386 →
  v388, the vertex moving with the mesh). Same reason at a moved vertex is
  §5's persist signature.

The pass therefore stays GATED OFF. It is retained as the census instrument
this adjudication was made with, and as the ladder any future §4.5.2 claim
must be re-measured against; its adopt arm (`YANG_452_REFINE=1`) is a dev A/B
knob whose adoption is REFUTED by §6.4–§6.5, not a candidate for a flip.

### 6.8 Where R0050 actually lands (routing, so it is not re-derived)

The Case-IV reading of v413 is exact and matches `stage4_phantom.rs`'s claim
shape verbatim — a junction vertex carrying {two same-input surfaces, one
other-input surface} = {A:6, A:9, B:2}, claiming A's edge A6∩A9 pierces B:2
there. But `stage4_phantom`'s certificate as written ("phantom iff the exact
line(edge)×surface solve has no root inside the edge's own segment") does NOT
flag it: the chord v76–v41 DOES have an in-segment root, at t = 0.84306. The
honest per-CROSSING form is "the exact root nearest THIS mesh crossing lies
outside the segment, and every in-segment root is farther than the relocation
budget" — which is precisely what `RelocationCrossedCarrierVertex` plus the
transit planner's `NoRealCandidate` already certify together.

So R0050 is a Case-IV rule-out customer. `specs/yang_433_case_iv_corner_phantom.md`
§7 has already adjudicated that route and REFUSED it, in both directions:

* as a Stage-1 density guard — two full gate-on corpus sweeps; the sharper
  (corner-cluster + inside-only) trigger still boosts 26 cases, converts 2
  (R0100, R0049) and regresses 8 including R0011 ERROR → **SUPPORTED_WRONG**;
* as a downstream rule-out — "ruling out the A-side loop leaves the B-side
  pieces bounded by the phantom vertices; their true boundary routes through
  geometry that does not exist in the mesh and must be created. That is the
  phase-3 junction-layer conformal mesh update (epic #169), not an increment
  of this spec."

**R0050's structural owner is therefore epic #169's phase-3 junction-layer
conformal mesh update**, reached via §4.3.3 — the same machinery the rest of
the tangency family needs. It is NOT a quick win, and the §4.3.3 tangent-point
insertion milestone named in §6.7 is the milestone, not a separate shortcut.

Tail shape after this session (9 actionable): **tangency family 6** — R0038
(plane tangent to a cylinder along one generator), R0050 (torus∩torus exact
tangency), F0058 (equal-R perpendicular cyl−cyl, the exact tangency point),
F0060 (B tangent to both caps along a LINE), C0058 (tangency-neck
figure-eight), C0065 (torus∩plane grazing loop) — vs **3** on other vehicles
(R0019 CDT ring-reject / I7 GROSS relocation overrun, R0085 §4-I9 persist +
Stage-6, R0100 KV9-F2a deep chords). The junction layer is the dominant
remaining vehicle in the tail, by a factor of two.

**What WOULD reopen §4.5.2:** a §4-I9 fire whose case ladder is MONOTONE — the
same typed failure weakening and then clearing as `d_ε` shrinks, with no
oscillation between failure kinds — on a surface pair with no tangency
anywhere. Yang's Table 3 (`:866-873`: 4 of 400 operations need resolution
enhancement at `d_ε = 1e-2`, 1 of 400 at `1e-3`, #Fail 0 throughout) says such
cases exist and that the loop is load-bearing in the paper; none of ours is
one yet.
