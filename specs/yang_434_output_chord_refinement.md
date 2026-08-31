# Spec: carried-edge curve restoration in the boolean OUTPUT — the KV9-F2a fold family's owner

**Status: FLIPPED ALWAYS-ON 2026-08-26 (N60 RESOLVED) — NEW CANONICAL
271C/0W/36E/1EE/0T (+R0020, +R0095, and +C0105 via the inc-7 hole-window
fix; zero CORRECT regressions; seven explained detail shifts). `YANG_434_OUT=0|off` is the dev off-knob;
`census` stays read-only. Both flip blockers were fixed structurally the
same day: R0054 by kernel-v2 conformal grid-aligned arc sampling (inc-4)
and F0085 by the invariant-4 debug-tiering alignment (inc-5). One NEW
capability gap recorded: the M8 n-ary mixed-arc vocabulary's seam-split
(4-arc) strip lateral (inc-6, zero corpus customers, quarantined naming
fixture). inc-0..inc-3 were BUILT AND MEASURED 2026-08-24/25 gated;
the I5-1b merge open-run LOOP FLOOR from inc-3 is ALWAYS-ON since then
(converted R0009 → the prior canonical 268C/0W/39E/1EE/0T). Design
REVISED BY MEASUREMENT — the owner moved from §4.3.4 (intersection-curve
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
  mechanism (all-on-surface 2D/3D inversion at r_unroll=4073) is separate and
  is **not this spec's customer**. **ANCHORED AND RESOLVED 2026-08-27** —
  `specs/kv9_f2b_lift_faithful_refinement.md`: the refinement MINTS that
  sliver (surface-metric worst aspect 204 → 3473 on the fold face, while its
  same-development control face holds at 109.80), and the repair is a second
  refinement criterion — refine while the chart→3D lift inverts. R0017
  converts; canonical 271C → **272C/0W/35E/1EE/0T**.
  The two families are told apart with no tuned constant, by comparing what
  bisection CAN remove against what it cannot: `dev` (nodes off the ideal
  development, immovable — F2a, this spec) vs `sag` (the ideal chart-lift
  sagitta, removable — F2b). Measured they separate by 21 orders of
  magnitude: R0017 face 17 `dev/sag = 7.9e-14`, R0003 face 577
  `dev/sag = 5.1e+07`.

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
    fold-margin band (P10). **→ inc-4 below.**
  - **F0085** — NonPlanarFace(35097) deep in the 20-op chain (≈300s/run);
    unanchored; suspect the same family through a chained re-entry.
- **inc-4 (BUILT 2026-08-25, always-on): conformal grid-aligned arc
  sampling in kernel-v2** (`tessellate/sampling.rs::arc_grid_samples`,
  red-verified per mechanism, face fixture in
  `arc_grid_sampling_tests.rs`). The R0054 probe anchored the full shape:
  FaceId(548) is a 0.0089-wide cone strip between TWO coaxial restored rim
  arcs (a fine revolve-profile step), each rim independently resampled
  (upper u-step 4.8043, lower 4.7643/4.5271) — the grids drift out of
  phase and the CDT builds needles whose apex sits mid-chord where the
  chord sags 0.052 below the surface. The grazing "pool vertex" is the
  OTHER rim's arc sample, not a B-Rep vertex — per-vertex insertion alone
  is the wrong shape, and the strip's junction vertices (u −134.52 upper /
  −142.93 lower, mutually mid-chord of the opposite rim) fold it a second
  way (measured: the face fixture still folds with grid alignment alone).
  Two mechanisms, both band-free:
  1. **Global azimuth grid**: Arc interior samples sit on `{j·2π/n_seg}`
     in an axis-canonical frame derived from the circle NORMAL alone
     (sign-canonicalized; no anchor vertex) — every coaxial arc samples at
     the SAME azimuths, so opposing chords pair into ladder rungs
     (sample-vs-sample needles impossible). This restores analytically the
     phase-lock Stage-1's shared revolve grid gave the mesh polylines for
     free. Chord-bound density contract unchanged (interior steps exactly
     Δ, end steps shorter).
  2. **Conforming vertex inserts**: for each boundary vertex of the two
     incident faces (canonical pool — twin-symmetric) whose EXACT 3D
     distance to the circle is within 4× the arc's own max chord sag, an
     interior sample is inserted at the vertex's azimuth — a junction
     vertex then faces a chord ENDPOINT, never a mid-chord
     (vertex-vs-sample needles impossible). Below the f32 render quantum
     the insert is skipped (would coincide); beyond 4× sag a needle's apex
     already clears the fold margin geometrically. The 4× is a
     constructive-coverage radius (over-inclusion adds harmless on-curve
     samples), not an acceptance band.
  Both twin-canonical as before (computed on the lower-id half-edge,
  reversed for the other side). ALWAYS-ON: the defect is general
  (any coaxial-arc-bounded thin strip), the restoration only made it
  common; default-corpus neutrality proven by the corpus run below.

  **inc-4 measurements (2026-08-25/26):** kernel-v2 suite green (299
  tests); gated singles — **R0054 ERROR→SUPPORTED_CORRECT** (blocker
  cleared), R0020/R0095/R0047/F0076/F0084/R0063/R0009 all stay CORRECT,
  R0003/R0100 stand at the ring-CDT next wall (FaceId 437/15, R0004's
  family) and R0017 at its F2b fold — pre-existing ERROR families, not
  regressions. **Full default corpus: score identical to canonical
  (268C/0W/39E/1EE/0T), EXACTLY ONE detail-only delta** — R0051 (already
  ERROR) keeps SelfIntersectingBooleanOutput on the same face pair (8, 10)
  with penetrations 116→127: the count is measured over the tessellation,
  which changed phase — the underlying defect is untouched. baseline
  results.json updated with that count. **Remaining flip blocker: F0085
  alone** (NonPlanarFace(35097) — investigation inc-5 below).
- **inc-5 (ANCHORED 2026-08-26): F0085's NonPlanarFace is a DOCUMENTED
  DEBUG-TIER CHECK RUNNING IN PRODUCTION, tripped by chained re-entry
  precision ratchet — not a restoration defect.** Probe chain
  (`KV2_PLANARITY_PROBE` + new `[nonplanar-probe]` site prints at all four
  NonPlanarFace raise sites, permanent):
  - The failing vertex (Extrude 19's union output, planar FaceId(35097))
    sits d=3.712e-12 off its face's stored plane, band 3.307e-12 — and it
    is EXACT (1e-17/1e-18) on both twin faces' planes (a triple-plane
    corner: a rational arrangement point of the two neighbor planes).
  - The WHOLE loop is smoothly 0.7–3.7e-12 off the stored plane — the
    stored (bit-inherited) plane vs the mesh-carried boundary disagree by
    a small tilt that RATCHETS per chained op (each op re-derives boundary
    vertices from the previous op's near-band mesh); op 19 is where the
    max crossed the band. The restoration's re-minted boundary geometry
    only shifts the lottery (default landed ≤3.3e-12, gated 3.712e-12).
  - The raise site is `validate_planar_face`'s per-loop-vertex planarity
    check at `PLANARITY_DEBUG_TOLERANCE` (1e-12 tier), reached in
    PRODUCTION via `from_yang_brep → finalize_solid → validate_solid`.
    The validate.rs module docs declare invariant 4 (on-surface geometry)
    "**debug builds only**" and the constant "not a production gate"; the
    F1 gate (`validate_boolean_output_planarity`, TAU_EVAL=1e-9, design
    review 2026-07-12) was BUILT as the production boolean-output
    planarity contract precisely because "planar by construction is false
    for yang re-entry" — but the strict check never got debug-gated, so
    the F1 gate is currently unreachable dead code for planar faces.
  - **Fix: gate the loop-vertex strict planarity check to debug builds**
    (code aligned with the ratified design; the F1 1e-9 gate + selfx stay
    the loud production walls — defect-class residuals ≥ MIN_FEATURE_SIZE
    exceed 1e-9 by ≥1000×, so no silent-wrong window opens; P10 satisfied
    by F1's own design analysis). Debug builds (the test tiers) keep the
    strict tripwire. Scope: ONLY the fired site. The curved analogs
    (validate_cylinder/torus/cone per-vertex strict residuals) share the
    same documented divergence but are left untouched until a case names
    them — several recorded loud walls (the R0028 VertexOffSurface class)
    live at those sites and cannot be re-tiered without their own
    adjudication.
- **inc-6 (FLIP, 2026-08-26): restoration ALWAYS-ON; N60 RESOLVED.**
  F0085 fixed (inc-5) → flip corpus (with inc-7)
  **271C/0W/36E/1EE/0T**: +R0020 +R0095 +C0105 (its selfx penetrations
  76→3→0 across the fixes — the inc-7 filled-corridor latent was its
  root), zero CORRECT regressions, seven explained detail shifts (R0003
  435→437 and R0100 14→15 = the recorded ring-CDT advances; R0017 14→17
  keeps its F2b fold; R0015 same Stage-4 wall at a shifted vertex index;
  R0026 ADVANCES past its input-manifold wall to a pre-existing Stage-3
  SSI AmbiguousCurve; R0051 ADVANCES past its subtract's selfx wall to a
  later union's pre-existing non-2-manifold reassembly wall; F0082 hits
  its same face-372 CDT wall two ops earlier). Flip fallout in yang-internal
  re-entry (the Stage-0 unit fixtures, NOT a production path): a yang
  OUTPUT brep carries per-face directed duplicate curved edges, and two
  independently-computed chains of one arc agree geometrically but not
  BITWISE — production only re-enters through the kernel round-trip, whose
  `to_yang` shares one edge record per twin pair (why 19-deep chains are
  watertight corpus-wide). The two fixture tests were normalized with
  `share_twin_curved_edges` (models `to_yang`'s dedup exactly; merges only
  bit-certified twins/duplicates — a same-normal swapped pair is the
  COMPLEMENTARY arc and never merges). t133 green;
  `nary_tessellated_group_stage0_meshes` now names a NEW capability gap —
  the restored rims move its flush-pocket top faces from the polygon
  overlay class into the MIXED class, whose `arc_lateral_opposite`
  vocabulary requires the 2-arc strip while this lateral's rims are
  seam-split (4 arcs) → loud `mixed-arc-lateral-unpaired`
  (`CoplanarFacesUnsupported`); zero corpus customers; quarantined with
  the milestone tag until the vocabulary extension.
- **inc-7 (2026-08-26, always-on): barrel-arm HOLE WINDOWING latent fixed
  in kernel-v2** — unmasked by the flip, red-verified by
  `boolean_chains::curved_output_reentry_through_boss` (a slot cut through
  a recovered boss; the selfx gate caught 2 penetrations). The seam-cut
  ring is anchored at the CHOSEN bridge azimuth `[base_x, base_x+span)`,
  but hole chains were only pre-centered near the +wrap chain's MIDPOINT
  and `blocked()` tested bridge candidates against holes at those raw
  positions: a hole left at a ±span image of its in-ring position lies
  OUTSIDE the outer polygon (measured: hole u∈[0, 0.615] vs ring
  [0.6438, 5.6703]), the flood-fill CDT ignores it, and the corridor over
  its territory is silently FILLED — the notch tessellated as solid wall.
  Pre-flip, mesh-density rim chains happened to yield anchors left of
  every hole; the restoration's short arc chains shifted the anchor and
  unmasked it. Fix: `blocked()` tests each un-pinned hole chain at its
  image inside the candidate window (rigid `k·span`, chain-mid nearest
  the window center), and acceptance applies the SAME validated shift
  before hole-ring construction. Pinned chains stay frame-locked.
  `KV2_PATCH_PASS_PROBE` now also prints ring/hole window extents (a
  hole outside the outer ring's window is this defect's signature).
- **NEXT WALL (named, not this spec):** the boundary-hook self-intersection
  at the rim×cut junction (R0003 face 437 / R0100 face 15 / R0004's
  family): the cut's hyperbola hook extends below the face's own rim —
  face-domain/junction assembly territory, needs its own anchor.
  *(RESOLVED through I13/I13d/I13e in `yang_441_trim_cdt_construction.md`;
  R0003 then advanced 437 → 467 → 517 → the FaceId(577) fold below.)*
- **inc-8 (2026-08-28): the R0003 FaceId(577) F2a fold ANCHORED and FIXED —
  the deviant sampler was `ellipse_interior_samples`, whose uniform-parameter
  grid bounds chord sag only at the ELLIPSE'S OWN scale.** Fold-probe walk
  (new `[deep-chord]` localizer + `KV2_PATCH_CHAIN_PROBE=<fid>` +
  `KV2_ARC_CONFORM_PROBE=<fid>`): face 577's `Chord`-kind boundary is clean
  (deepest 5.7e-14 — the restoration + I5-1b did their jobs), and the
  dev = 9.122e-2 split node descends from an **EllipseArc sub-chord**: the
  steep cut-plane×cone section (R_maj ≈ 93 ≫ r_local ≈ 71→4.3-band) got
  only 3 interior samples for a ~29-long arc — k = ceil(sweep/(2π/n_seg))
  is uniform in PARAMETER, so max chord sag ≈ R_maj·(1−cos(π/n_seg)) =
  0.091, which swallowed the 0.102-tall strip between the ellipse and the
  rim arc below it (chart-aspect-498 needle → inverted lift → tripwire).
  Every other boundary sampler already honors the circle-step sag contract
  at a SURFACE-derived scale (arc/circle: own radius; surface-pair: the
  smallest defining-surface local radius). Fix, in `sampling.rs`:
  1. **Sag-bound ellipse sampling** — keep the k-grid, then recursively
     bisect each span while its measured sag (ellipse point at the
     parametric midpoint vs the chord midpoint) exceeds
     `r_local·(1−cos(π/n_seg))`, `r_local` = the smallest local radius of
     the two incident faces' surfaces at the edge endpoints (planes
     contribute none; no scale at all ⇒ the ellipse's own scale stands =
     the prior behavior). Depth-capped with a typed failure; an endpoint at
     a cone apex (local radius 0) fails loud, matching the surface-pair
     sampler's apex posture. Dev off-knob `KV2_ELLIPSE_SAG=0|off` restores
     the pure k-grid byte-identically. 4 unit tests
     (`arc_grid_sampling_tests.rs` §ellipse sag contract).
  2. **EllipseArc chain walk unified** (`developable.rs`) — the cylinder
     patch's uniform-fraction azimuth shortcut (`Δθ = s_w·Δt·frac`)
     assumed samples uniform in parameter; non-uniform sag-bound samples
     scrambled its chart (fold in `ellipse_bounded_tunnel_reentry`). All
     EllipseArc boundaries now use the per-sample wrapped-Δθ walk that was
     already the cone-section path (position-derived, kind-agnostic).
  3. **inc-8a, built GATED OFF (`KV2_ARC_CONFORM_CURVES=1|on`)**: the
     conforming CURVE-SAMPLE pool — `arc_interior_samples_frac` can extend
     its conforming pool with the incident boundary curves' own sample
     points (Arc/Circle contribute pure-grid samples, no recursion), the
     completion of inc-4's "…or CDT-split the graze" sentence. Built first
     on the f577 hypothesis, measured byte-identical there (the ellipse's
     sparse samples sat outside every window) — NO corpus customer, so it
     stays off: an arc-vs-curve graze inside one band fails LOUD (fold
     tripwire), never silent.
- **inc-8b (2026-08-31): FIRST CORPUS CUSTOMER + the pool's INSERT gap
  closed — R0044 FaceId(626) under the corner-transit gate set**
  (`specs/yang_451_corner_transit.md` §3s RESOLVED block, inc-2c-3b-11).
  A legitimate ~304° near-sliver carried cone band (strip 1.19 wide,
  facet chord sag 3.60) rung-pairs perfectly under the R0054 grid EXCEPT
  where a twin-face vertex (d3=1.656, mid-grid azimuth) mints a
  mechanism-2 insert on ONE rail: the pool is EDGE-local (the arc's two
  incident faces) while the fold constraint is FACE-local, so the
  opposing rail of the SAME strip face never sees the vertex and its
  ladder stays unpaired mid-chord (sample-vs-INSERT — the R0054 needle
  geometry one mechanism level up). F2b behaves exactly as designed: it
  fires on the inverted all-on-surface triangle, LEPP-splits the rung,
  lerps the split on the chord (the load-bearing T-junction rule), and
  then `dev ≥ sag` rightly declines its own minted off-development node.
  inc-8a as built could NOT fix it: pool arcs contributed PURE-grid
  samples, which already align and dedup — the insert azimuth was
  invisible (measured: gate-on byte-identical fold). **Fix: pool arcs
  contribute their grid samples PLUS their own vertex-pool conforming
  inserts** (`boundary_curve_pool_samples`; the vertex pool is static
  B-Rep data — depth-1, no recursion into curves). The azimuth-set
  closure is exact at depth 1: every chain azimuth originates in some
  arc's grid+vertex-insert set, which every in-window neighbour conforms
  to directly. Unit-pinned both ways
  (`pool_curves_carry_their_vertex_inserts_across_the_strip`: gate off =
  the pinned loud fold, gate on = paired ladder, tessellates; un-pin the
  off arm when the mechanism flips always-on). Measured: face 626
  converts (`n_split=0 fold=0`), R0044 advances to the unmasked
  FaceId(627) ring rejection (yang-side emission defect — a
  SurfacePair→HyperbolaArc junction 0.15 off the face's cone; owned by
  the corner-transit epic, not this spec). Gate stays DEFAULT OFF —
  the flip is its own decision with a full-corpus W-scan (the R0053
  mask lesson); today it is the fourth knob of R0044's conversion path.
  Measured: R0003 f577 SKINS (ellipse gets 7 sag-bound samples); the case
  advances to **FaceId(903), "ring rejected by CDT"** — a WALL-PLANE ring
  whose crossing is between four B-REP VERTEX origins (LineSegment h7641's
  16.2-long span × the h7643 hyperbola piece 0.04 past its end), i.e.
  pre-existing yang-side geometry MASKED behind 577, untouched by this
  render-side change: the typed hyperbola chain takes ONE step in the
  wrong direction at a LineSegment junction (out-and-back spur — the I13d
  "junction hopped its first chain samples" shape at an UNTYPED junction).
  Its anchor is the next increment (yang-rs, `yang_441` I13 family).
  **Sibling recorded, not changed (no customer):**
  `hyperbola_interior_samples`' tol is `max(a,b)·(1−cos(π/n_seg))` — the
  hyperbola's OWN scale, the same class of deviation this increment fixed
  for the ellipse. Corpus hyperbola chains are per-mesh-piece today
  (n_interior = 0 everywhere measured), so the tol never engages; a future
  I5-1b-merged long HyperbolaArc edge is its customer.

## 4. Constraints

- The Chord collinear-split closure rule in kernel-v2 is LOAD-BEARING
  (T-junction safety) and UNTOUCHED — the fix types the source curve so
  splits become on-curve `ArcSample`s instead.
- P10: no acceptance bands to squeak folds through; the fold tripwire is
  unchanged. The restoration is certification-driven and self-declining;
  every decline keeps the per-segment status quo loudly countable
  (`[s434-restore]` stats line under `YANG_441_VERBOSE`).
