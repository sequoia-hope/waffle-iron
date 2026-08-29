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
  the I13d branch.
- #168 degenerate-cap wall (R0038): epic #169 C/D two-sided junction-aware
  re-CDT.
- Tangent disc∩disc (C0067): §4.3.3 tangent-point insertion milestone.
- R0050: the upstream producer's refinement-latent non-manifold emission is a
  NEW recorded latent (fires only under the dev knob today).
