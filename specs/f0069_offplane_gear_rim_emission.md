# F0069/F0072 — off-plane gear top-rim emission at a coplanar stack seam

**Task:** #153. **Status:** DIAGNOSED, mint-site fix not yet landed.
**Class link:** same family as task #146 (yang emits *planar* output faces whose
loop vertices are off the stored face plane). #146's Newell variant is caught by
the *downstream* op's kernel ("plane normal disagrees with Newell"); this one is
caught directly by the F1 production planarity gate
(`validate_boolean_output_planarity`, `PLANARITY_BOOLEAN_OUTPUT_TOLERANCE =
TAU_EVAL`, added task #152).

## Reproduce

```
KV2_PLANARITY_PROBE=1 ASSAY_CASE=F0069 \
  cargo test -p test-harness --test assay_kv2 --release single_case -- --ignored --nocapture
```

The env-gated probe in `crates/kernel-v2/src/validate.rs`
(`validate_boolean_output_planarity`) dumps the offending vertex AND its face
plane (normal + point).

## Measured defect (F0069, 15 chained coaxial extrudes, seed 10007)

Two auto-union failures, both `NonPlanarFace`:

| face | plane.n | plane.pt (Z) | vertex p (Z) | d (off-plane) | band |
|---|---|---|---|---|---|
| 6227 (Extrude 9) | (-0.994248, 0.107098, 0) | 1.806231 | 1.922942 | -2.96e-8 | 2.92e-9 |
| 6711 (Extrude 10) | (-0.999590, -0.028627, 0) | 1.922942 | 1.922942 | +2.60e-8 | 2.92e-9 |

Key facts:

- **Both planes are VERTICAL** (`normal.z == 0`) — they are **gear tooth-flank
  faces**, not horizontal caps. The off-plane displacement is a **tangential
  (in-plane-normal) slip**, not a Z error.
- The seam is at **Z = 1.9229421278** — exactly where op 9 (gear,
  `plane_origin.z=1.8062307976`, `depth=0.1167113302`, top = 1.9229421278)
  stacks under op 10 (circle, `plane_origin.z=1.922942127810015`). Coplanar
  horizontal seam.
- Face 6227's plane is anchored at the tooth-flank **base** (Z=1.806231); the
  offending vertex is that flank's **top** corner (Z=1.922942), i.e. the top-rim
  vertex has drifted 3e-8 off the flank plane the base defines.
- **3e-8 at ~2 m is ≈ 7×10⁷ ULPs** — this is a genuine computational
  displacement, NOT f64 rounding noise. Per the prior directive (memory
  `remediation_2026_07_12_shipped`): **fix the mint site, never widen the band.**

## Empirical trace (2026-07-13, session refinement)

Ran the existing yang probes (`YANG_INPUT_VERT_PROBE`, `KV2_PLANE_TRACE`) plus a
throwaway per-vertex residual probe in `from_yang` on F0069's op-9 auto-union.
Findings, in order of what they rule out:

1. **The off-plane vertex is a pre-existing INPUT vertex to the failing
   boolean, NOT minted in it.** `YANG_INPUT_VERT_PROBE` shows the offending
   point `(-0.029050124226413905, -0.26968703059951493, 1.922942127810015)`
   present bit-identically as vertex 3 in BOTH operands (and in both Stage-0
   meshes). So op-9's own Stage-0/6 and `from_yang` are exonerated — the drift
   is **chained in** from a prior op's boolean output (or the accumulated
   body's seam). This case is squarely the **task #146 class** (chained planar
   faces carrying off-plane loop vertices).
2. **The flank IS geometrically vertical where it matters.** The base-rim twin
   directly below (vertex 951, z=1.806231) has BIT-IDENTICAL `(x,y)` to the top
   vertex 3. So the `d≠0` cannot come from a base↔top misalignment on that
   corner.
3. **Normal source is NOT the bug (hypothesis REFUTED).** `KV2_PLANE_TRACE`
   shows yang emits the flank's plane normal as EXACTLY `(-0.994248…,
   0.107098…, 0.0)` (z==0) with `d≈-8e-16`. But `from_yang.rs:984` stores the
   **Newell-fit** normal (`nu`, z-tilt ≈ 2.5e-7) computed from the loop, not
   yang's exact normal. Storing yang's exact normal was tried: the per-vertex
   residual probe shows `max_resid_newell == max_resid_yang == 5.924e-8`
   IDENTICALLY. The worst-offending loop vertex sits at the anchor's z (so the
   z-tilt is irrelevant to it) and is slipped ~5.9e-8 **tangentially in (x,y)**.
   No choice of plane normal makes the loop planar — the **vertex positions are
   genuinely non-planar**.
4. **The loop has 5 vertices, not 4** (`cycle_len=5`). A fresh flank quad has 4;
   the 5th is a **seam-crossing point inserted on the flank's edge** by a prior
   op's coplanar/overlay processing. That inserted point is the ~5.9e-8
   off-flank offender — it was computed in f64 (or relocated) instead of landing
   exactly on the flank plane.

**Conclusion:** the mint is an **upstream/chained tangential vertex slip** where
a seam-crossing point is inserted onto a planar flank edge off the flank plane.
It is the #146 shared-vertex design-increment class (a point exact on one
incident analytic surface but ~6e-8 off another), NOT a normal-source or
`from_yang` conversion bug. Fixing it requires either (a) yang Stage-0/6 to
mint the flank-edge seam-crossing point by exact intersection with the flank
plane (rational), or (b) a Stage-6 snap of shared boundary vertices exactly onto
every incident analytic surface — the same "snap-rounding grade" design
increment named for the #144 opposite-rim class. Band-widening remains
forbidden (the residual is real).

## Mechanism (original hypothesis — see Empirical trace above for refinement)

The gear's top rim sits on the gear/circle coplanar seam. During the chained
union the seam is re-processed by Stage-0 coplanar preprocessing / azimuth-merge
(the SAME seam that, in the full-assay run of the LATER op, emits
`azimuth-merge rims have mismatched / too-few samples`). The top-rim vertices are
relocated / re-sampled there and pick up a ~3e-8 tangential slip relative to the
vertical tooth flanks whose planes are pinned by the (undisturbed) base rim.

So the mint site is in **yang-rs Stage-0 / Stage-6** where the coplanar-seam rim
vertices are emitted — NOT in kernel-v2. The fix must make a rim vertex that
belongs to a vertical planar flank land **exactly** on that flank (the flank
plane is analytically known: it is the extrude's side plane), rather than a
re-sampled approximation of it.

## What NOT to do

- Do **not** widen `PLANARITY_BOOLEAN_OUTPUT_TOLERANCE` — 3e-8 is a real defect,
  and the band is deliberately ≥1000× above legitimate `TAU_WORK` noise.
- Do **not** blanket-project every planar-face loop vertex onto its plane at the
  `from_yang` conversion — that hides the emission bug (a band-widening cousin)
  and could mask a genuine off-plane classification error elsewhere.

## Next step

Trace the gear top-rim vertex through Stage-0 coplanar seam handling and the
azimuth-merge path for a chained coaxial gear→disc union; find where the rim
sample is emitted off the tooth-flank plane and pin it to the exact flank.
F0072 (3 auto-union failures, `NonPlanarFace` + `azimuth-merge mismatched`) is
the same class and should fall out with the same fix.

## Amendment 1 (2026-07-14) — a yang producer-side GROSS-non-planarity gate landed (N42), but F0069 is BELOW its band; F0069 stays F1-caught; mint-site fix still open

A Stage-6 planar-face **gross-non-planarity self-check** shipped (deviation N42,
`stage5_topology.rs::emit_topology`, guard `s6-planar-loop-nonplanar`): yang now
rejects its own planar output whose loop vertex is beyond the MODEL coplanarity
tolerance `TAU_MODEL` (1e-7) off the plane. The band is deliberately `TAU_MODEL`,
**not** `TAU_EVAL` — a first `TAU_EVAL` cut false-positived on the DESIGNED
near-coplanar fixture `yr27_face_resolution::near_partial_overlap_residual_1e8`
(a valid 1e-8-residual near-coplanar union), so the producer wall is scoped to
GROSS defects (≥ `MIN_FEATURE_SIZE`-scale).

**F0069/F0072's residual is ~3e-8 — BELOW `TAU_MODEL` — so this producer gate
does NOT catch it; F0069 continues to be adjudicated by kernel-v2's stricter
`TAU_EVAL` F1 gate exactly as before** (assay unchanged, still `ERROR`). N42
tightened attribution only for the GROSS `#146` drivers R0051 (1.187e-3) and
F0064 (8.331e-2).

So for F0069, N42 changes nothing: the **mint-site fix in this spec (exact
seam-crossing insertion on the flank plane) remains the open task-#153 work**,
still bounded by the "What NOT to do" list above (no band-widening — and note the
F1 `TAU_EVAL` vs the N42 near-coplanar `TAU_MODEL` tension is a real, deferred
design question about whether a 3e-8 seam slip is a "defect" or a legitimate
near-coplanar residual; #153 currently treats it as a defect per its measured
mechanism, an inserted point NOT on any near-coplanar seam). The sibling R0051
defect, root-caused the same session, is a DIFFERENT mint bug (an
over-determined thin-slab `cyl∩cyl` piercing that needs a Stage-4 vertex SPLIT,
task #137 family) — see deviation N42.
