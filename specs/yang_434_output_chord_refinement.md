# Spec: carried-edge curve restoration in the boolean OUTPUT — the KV9-F2a fold family's owner

**Status: inc-0..inc-3 BUILT AND MEASURED 2026-08-24/25. The restoration
is GATED `YANG_434_OUT=1` (flip blocked on two explained regressions:
R0054 conformal-sampling sliver, F0085 chained NonPlanarFace — see inc-3).
The I5-1b merge open-run LOOP FLOOR from inc-3 is ALWAYS-ON and converts
R0009 in default mode → NEW CANONICAL 268C/0W/39E/1EE/0T. Design REVISED
BY MEASUREMENT — the owner moved from §4.3.4 (intersection-curve
refinement) to §4.4.2 (boundary-curve restoration). Original design
checkpoint recorded 2026-08-24 after the §4.5.1/§4.5.3 session.**

## 1. The family, measured (KV2_PATCH_FOLD_PROBE + the two inc-0 censuses)

All four KV9-F2 "patch triangulation folded (inverted triangle)" cases carry
facet-deep `LineSegment` boundary chords; F2a (R0003, R0100, R0020) folds
when a `Chord`-kind boundary split keeps the ORIGINAL chord's sagitta as a
permanent off-surface deviation and a sliver thinner than that depth bridges
the layers. (Fold-probe anatomy: original spec §1, preserved in git history
at bdb78520.)

**inc-0 consumer census (`KV2_CHORD_DEPTH_CENSUS`, corpus-wide 2026-08-24):**
per developable-patch face: w_facet, r_unroll, n original chords, max chord
sagitta, n splits, max split deviation, min emitted 2D triangle height, fold
verdict (plus a torus-patch analogue row).

- 14 711 face rows across 84 cases; **372 rows in 40 cases carry chords
  deeper than their own render facet band** (sag > w²/(8r)), **29 of those
  cases SUPPORTED_CORRECT** — ratios up to **78×** (F0046), p50 ≈ 30×.
- The four folds sit at ratios 1.29–8.66 — folding is the sliver lottery on
  a UBIQUITOUS depth defect, not a property of the deepest chords. The
  depth is silent render sag everywhere else.
- R0017 (F2b) also carries a 5.2× deep chord on its fold face; its fold
  mechanism (all-on-surface 2D/3D inversion at r_unroll=4073) remains
  separate and UNANCHORED — not this spec's customer.

**inc-1 producer census (`YANG_434_OUT=census`, same day):** for every
2-use `LineSegment` output chord between two distinct faces: owner classes
(cross-input / same-input / same-surface), the owning surfaces, midpoint
depth by pair-Newton (`relocate_onto_implicit_pair`) or plain surface
distance, endpoint residuals, and `intersection_curves` map status.

- R0003: ALL cross-input seams are Plane×Plane at sag 0 (straight, correct).
  **The deep chords are SAME-INPUT seams between adjacent Cone faces** —
  6 077/9 015 per boolean, sag to 0.60, endpoints on both cones at ≤2e-12,
  `map=none`. R0100/R0020 identical in kind (R0020 adds four
  Cylinder×Plane rims at sag 0.40).
- **Carried-edge match: 15 077/15 077 deep chords lie on an INPUT `Circle`
  edge of the two attributed input faces, worst endpoint residual 1.0e-11**
  (vs the 4.3e-5 scale band) — the revolve's rim circles between adjacent
  profile-vertex strips, tessellated at Stage-1 MESH density and never
  re-typed at emission.

## 2. The owner (the paper's own step) — REVISED

The 08-24 design checkpoint attributed the chords to "yang-rs pair-curve
polylines at MESH density = the §4.3.4 refine debt". The producer census
RETRACTS that attribution: the chords are **carried input topology edges**,
not Stage-3 intersection output. The paper's owner is **§4.4.2**
(`refs/text/yang2025_hybrid_boolean.txt:592-605`): the B-Rep boolean output
restores patches bounded by "**either the original boundary curves or the
intersection curves**". Our emission types the intersection curves (the
`intersection_curves` map) but leaves original boundary curves between
same-input faces as per-mesh-segment `LineSegment` polylines.

**The fix (inc-1 as built): carried-edge curve RESTORATION**
(`stage5_output_refine::restore_carried_edge_curves`, gated `YANG_434_OUT=1`,
runs in `emit_topology` immediately BEFORE the always-on I5-1b seam
chain-merge):

- Eligibility: undirected `LineSegment` output edge with exactly 2 loop
  uses, on 2 distinct faces, SAME input, DIFFERENT input-face attribution —
  a boundary descending from the shared input edge between those two input
  faces.
- Candidates: the non-`LineSegment` curves on the two INPUT faces' own
  loops (Circle today; the census names any further kinds if they appear).
- Certification: both chord endpoints on the candidate within the classify
  band (`TAU_EVAL·(1+max(r, coord))` — the merge/from_yang band); exactly
  ONE distinct in-band circle (ambiguity declines); chord sweep ≤ π/2
  (mesh chords are ≪ that; wide arcs decline).
- Action: re-TYPE both copies in place via `orient_directed_curve` (per-copy
  traversal orientation, twin-negated normals). No vertex, loop, or index
  mutation — gate off or nothing certified ⇒ emission byte-identical.
- Downstream, unchanged machinery finishes the job: the I5-1b merge
  coalesces the typed runs into single arc edges (its own full
  certification), from_yang imports them as `Arc`s, and kernel-v2 samples
  them at render density (`ArcSample` boundary entries, on-curve splits).
  The F2a depth family dissolves at its root; no density decision needed
  (the polyline-densification design and its band question are OBSOLETE).

Rejected alternatives, recorded: (A) §4.3.4 pair-Newton densification of
the output polylines — wrong paper step for carried conics (they are not
intersection curves), and it either bloats the B-Rep at the paper's d_p
density or needs a nonstandard band; kept ONLY as the eventual owner for
genuinely-untyped intersection polylines if the census ever finds any
(2026-08-24: zero across R0003/R0100/R0020/C0001/C0002 — every measured
cross-input seam was a straight Plane×Plane). (B)/(C) render-side blending
and CDT sliver suppression — unchanged from the original checkpoint, still
rejected.

## 3. Increments (as built / next)

- **inc-0 (BUILT, permanent probes):** `KV2_CHORD_DEPTH_CENSUS` in the
  developable + torus-patch tessellators; `YANG_434_OUT=census` producer
  census with carried-edge matching. Both print-only, env-gated.
- **inc-1 (BUILT, gated `YANG_434_OUT=1`):** the restoration pass above +
  6 unit tests (typed-on-both-copies orientation, off-curve/ambiguous/
  wide-sweep declines, cross-input and same-face ineligibility, decline
  identity).
- **inc-2 (MEASURED, gated):** R0003 fold face 435 SKINS; the case advances
  to FaceId(437) "ring rejected by CDT (degenerate/self-intersecting)" —
  R0004's pre-existing family, unmasked (the crossing geometry is
  bit-identical with polylines: all rim nodes sit at constant chart v, and
  the HyperbolaArc end-cut hook (6→7→8, dipping 0.044 BELOW the strip's own
  rim and crossing the rim line 6.5e-4 from the junction vertex) predates
  the typing). R0100 likewise advances (face 14 fold → face 15 ring-CDT).
  **R0020 ERROR → SUPPORTED_CORRECT, all oracles.** R0017 keeps its F2b
  fold (face 14 → 17; its F2a-depth component resolved, the inversion
  mechanism remains — its own anchor is still owed).
- **inc-3 (MEASURED 2026-08-25; restoration flip BLOCKED, gate stays
  off):** pre-fix default corpus BIT-IDENTICAL ✓ (267C/0W/40E/1EE/0T);
  post-fix default corpus has EXACTLY ONE explained delta — **R0009
  ERROR→SUPPORTED_CORRECT** (its `CurvedGeometryMismatch` "bounded
  cylinder patch must have exactly one material-CCW loop" was the
  always-on CONIC merge's own under-3 coalescing, live in default mode
  since the I5-2 flip; the loop floor repairs it) — **NEW CANONICAL
  268C/0W/39E/1EE/0T**. First gated corpus: 263C/44E — +2 conversions (R0020,
  and **R0095**, a bonus: its coplanar-contact fold clears) but SIX CORRECT
  regressions. Two follow-up fixes recovered four of them, each red-tested:
  - **I5-1b merge open-run loop floor** (ALWAYS-ON fix, own latent): the
    merge's ≥3-edge loop rule existed only for closed runs; an open typed
    run coalescing could leave a 2-edge loop (from_yang/cone-validate
    reject). Pre-pass computes per-canonical-chain piece floors
    (conservative across ALL owner loops, twin-safe: max over owners);
    `decide_run` takes the floor as `min_pieces`. Recovered R0047 (and
    unblocked F0076/F0084/R0063's first walls).
  - **Midpoint DOMAIN certification in the restore pass**: endpoints-on-
    circle is necessary but NOT sufficient — a STRAIGHT carried edge whose
    two endpoints lie on one rim circle (a chord line of the rim; R0063's
    micro-scale anchor: the mis-typed arc's plane was EDGE-ON to the
    planar owner, arc-plane·face-normal ≈ 5e-4) was re-typed as an arc
    bulging off both faces. The restored arc's minor-arc midpoint must now
    lie on BOTH owner surfaces within the same band. Recovered
    F0076/F0084/R0063.

  **REMAINING FLIP BLOCKERS (2 explained regressions; no flip while a
  CORRECT case regresses):**
  - **R0054** — a NEW fold shape at render density: all three nodes
    ON-surface (dev 1e-14); the sliver bridges two adjacent `ArcSample`s
    (4.8 u apart, sagitta ≈ 0.052 at r_unroll 55.2, tan_a 3.05) and a
    boundary POOL vertex only 0.0089 away in the chart; dot −0.1042 vs the
    −0.1 margin. The old mesh polyline was conformal BY CONSTRUCTION (it
    passed through near-rim mesh vertices); independent arc samples are
    not. Structural fix: conformal/boundary-aware arc sampling in the
    kernel-v2 patch tessellator (insert near-coincident boundary vertices'
    chart positions into the sample chain, or CDT-split the graze) — NOT a
    fold-margin band (P10).
  - **F0085** — NonPlanarFace(35097) deep in the 20-op chain (≈300s/run);
    unanchored; suspect the same family through a chained re-entry.
- **NEXT WALL (named, not this spec):** the boundary-hook self-intersection
  at the rim×cut junction (R0003 face 437 / R0100 face 15 / R0004's
  family): the cut's hyperbola hook extends below the face's own rim —
  face-domain/junction assembly territory, needs its own anchor.

## 4. Constraints

- The Chord collinear-split closure rule in kernel-v2 is LOAD-BEARING
  (T-junction safety) and UNTOUCHED — the fix types the source curve so
  splits become on-curve `ArcSample`s instead.
- P10: no acceptance bands to squeak folds through; the fold tripwire is
  unchanged. The restoration is certification-driven and self-declining;
  every decline keeps the per-segment status quo loudly countable
  (`[s434-restore]` stats line under `YANG_441_VERBOSE`).
