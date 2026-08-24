# Spec: §4.3.4 refinement of OUTPUT intersection polylines — the KV9-F2a fold family's owner

**Status: DESIGN CHECKPOINT 2026-08-24 (anchor measured, increments named,
nothing built). Follows the §4.5.1/§4.5.3 session that converted
R0028/R0025; this is the next recorded build.**

## 1. The family, measured (KV2_PATCH_FOLD_PROBE, extended this session)

All four KV9-F2 "patch triangulation folded (inverted triangle)" cases,
probed with per-corner node origin (pool/split), deviation from the ideal
development (`dev = |pos − surface_point(p2)|`), and per-edge kinds:

| case | face | dot | fold triangle | mechanism |
|---|---|---|---|---|
| R0003 | 435 | −0.76 | a=pool ON-surface (seam x2=0), b=Chord-split **dev=0.242**, c=Interior-split dev=0; height 0.044 | **F2a** |
| R0100 | 14 | −0.64 | a=pool (seam x2=0), b=Chord-split **dev=1.893**, c=Interior dev=0 | **F2a** |
| R0020 | 21 | −0.13 | a,b Interior dev=0, c=boundary-split **dev=0.250**; height 0.031 | **F2a** |
| R0017 | 14 | −0.998 | ALL nodes on-surface (dev ≤ 3e-12), huge scale (r_unroll=4073) | **F2b — different, UNANCHORED** |

**F2a mechanism (three cases, quantitative):** a boundary `Chord`-kind
edge splits COLLINEARLY on its original 3D segment (the T-junction closure
rule — correct and load-bearing), so a split node keeps the ORIGINAL
chord's local sagitta as a permanent deviation below the surface;
adjacent `Interior` splits sit exactly ON the surface
(`surface_point(mp2)`). A triangle bridging the two layers folds whenever
its 2D height < the depth mismatch. Fits: R0003's dev 0.242 = the sagitta
of a ~19.3-wide chord at r_unroll=191.9; face 435's w_facet=16.98 ⇒ facet
band ≈ 0.188 — the chord is only ~1.3× the render band deep, and the
folding sliver is band/4 thin.

**Where the deep chords come from:** the boolean OUTPUT B-Rep's
intersection edges. yang-rs emits pair-curve (torus/surface-pair)
intersections as `LineSegment` polylines chordized at MESH density — the
§4.3.4 refine-after-repair debt, deferred at §4.5.1 inc-2b with the
recorded trigger "the day R0003 completes". R0003's Stage 4 now completes
(inc-4); the fold is that debt materializing, on schedule.

## 2. The owner (the paper's own step)

Yang §4.3.4: after optimization, "refine the curve … to obtain the final
intersection curve" to the tolerance. Applied here: refine the OUTPUT
intersection polylines (the pair-curve `LineSegment` chains and any other
out-of-band chordized curve edges) by inserting samples ON the analytic
curve (pair-Newton from chord midpoints — the primitive exists) until
chord sagitta ≤ the consumer band. Depth then collapses quadratically and
the F2a precondition dissolves for any sliver height. The I5 §4.3.4
seam-density machinery (census + insert + chain-merge, always-on since
I5-2) is the in-crate precedent and likely shares the acceptance
(h/l/α test, d_p = 1e-7, N58).

Rejected alternatives, recorded: (B) render-side corridor blending of
interior nodes toward deep chords — fixes skinning locally but is not the
paper's step and adds off-surface render geometry; (C) sliver
suppression in the CDT/LEPP — a quality heuristic that leaves the depth
mismatch in place. Both stay unbuilt unless §4.3.4 measures out.

## 3. Increments (each lands separately, measured)

- **inc-0 (census):** `KV2_CHORD_DEPTH_CENSUS` in the developable (and
  torus-patch) tessellators: per face, max boundary-split dev, w_facet,
  min emitted 2D triangle height, fold-fired. Run corpus-wide: which
  CORRECT cases carry facet-deep chords WITHOUT folding (is the fold
  precondition rare or ubiquitous), and the F2a case ratios. This decides
  the refinement band target (render band vs d_p) and the blast radius.
- **inc-1 (gated primitive):** §4.3.4 refinement pass over output
  intersection polylines in yang-rs (Stage 6 emission side), gated
  `YANG_434_OUT`: insert pair-Newton samples until sagitta ≤ band;
  acceptance = the N58 h/l/α test; chain-merge interaction with I5
  measured (the merge must remain the identity on refined chains).
- **inc-2:** pin R0003 (expect FaceId 435 to skin; name the next wall),
  then the corpus proofs (default byte-identical while gated; gated run
  deltas each explained), then the flip per the standing bar.
- **R0017 (F2b) is NOT this spec's customer** — all-on-surface inversion,
  unanchored; needs its own vertex-level anchor (2D winding vs 3D
  orientation at r_unroll=4073 — suspect development overlap/seam-copy
  connection). Do not fold it into F2a's fix.

## 4. Constraints

- The Chord collinear-split closure rule is LOAD-BEARING (T-junction
  safety with neighbor faces) — the fix densifies the SOURCE polyline in
  the B-Rep so both faces see the same denser chain; never bend splits
  off the recorded segment.
- P10: no acceptance bands to squeak folds through; the fold tripwire
  stays exactly as loud as today.
