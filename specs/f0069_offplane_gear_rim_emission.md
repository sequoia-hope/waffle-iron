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

## Mechanism (hypothesis, to confirm)

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
