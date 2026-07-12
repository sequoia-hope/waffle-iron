# M8: Exact opposite-rim projection — task #144 P10 REFUTATION RECORD

**Status:** REFUTED / ABORTED (2026-07-12). The planned fix was implemented,
caught by the adversary suite, and reverted per P10. This file is the design
record for the real (future) increment.
**Corpus drivers:** C0048 (`azimuth-merge rims have mismatched / too-few
samples (66 vs 69)`), F0067 (`572 vs 571`) — the named "exact opposite-rim
projection" follow-up from M-C and tasks #142/#143.

## Measured mechanism (probes: `[opp-proj]`, `[ring-build]`, both env-gated
## under `YANG_SPLIT_PROBE=1`, banked in this task)

C0048's failing union, lateral face 2 between rim edges 4 (cap with own
overlay crossings) and 0 (opposite, no own crossings in this op):

- Edge 4: 58 cap crossings → ring 14 uniform + 55 inserted + 3 merged = 69.
- Edge 0: receives the 58 through `collect_ring_crossings`' f64 radial
  renormalisation; **3 pairwise bit-collisions** → 55 entries → ring
  14 + 52 + 3 merged = 66.
- **Merge counts are SYMMETRIC (3 = 3).** The entire 69−66 deficit is the
  3 pairwise collapses. (The earlier "merge asymmetry" hypothesis is
  disproven by the `[ring-build]` measurement.)
- The collapsing pairs are **same-ray radial twins**: a #142 fused-emission
  survivor at chord depth (e.g. `[0.99499, 1.07699]`, radius 1.4663 on the
  r=1.5 rim) plus its on-circle twin at bit-identical exact azimuth. Their
  on-circle images coincide **even in exact arithmetic** — arbitrary
  precision cannot separate them; only a deterministic tangential
  separation (snap-rounding grade, [#52] Hobby family) can.
- F0067 is the same arm (1 collapse, `572 vs 571`), plus two independent
  Newell-normal failures (separate class).

## The refuted fix (implemented → adversary-caught → reverted)

Exact translation `opp = p + (oc − cc)` per coordinate in rational, gated on
bit-equal rim radii + axis-parallel centre delta. It made counts match by
construction (C0048 progressed past the azimuth-merge wall to a deeper
arrangement wall) and bit-mirrored the #143 merge decisions. **Refuted by
`n2_rim_mint_adversary`** (4 tests): a mirrored chord-DEEP sample lands on
rims with **no own crossings**, where it is pure scaffolding that nothing
relocates (Stage-4 relocation only touches intersection-adjacent vertices) —
output loop vertices sat off their face's analytic surface by ≈ the sagitta
on `Ok` outputs (SILENT-WRONG; the contract is fully-valid or loud).

Key asymmetry the plan missed: cap-side chord-deep crossings are legitimate
ring members (the stage1 override band explicitly admits up to sagitta depth,
`stage1_tessellate.rs` band check) because they are intersection-curve points
that Stage 4 refines onto exact geometry; opposite-side projections may be
pure scaffolding and must therefore lie ON the analytic circle themselves.

## Constraints any future fix must satisfy (all measured/pinned)

1. **On-circle within the stage1 rim band** (`|r − radius| ≤ 1e-9·(1+radius)`,
   axial ≤ band) — pinned by
   `opposite_rim_projection_lands_on_circle_within_band`.
2. **Injective**: bit-distinct cap samples → bit-distinct opposite samples,
   including exact-same-azimuth radial twins ⇒ requires a deterministic
   tangential separation for colliding images (≥ TAU_MODEL separation also
   keeps the #143 merge gate from swallowing the separated image — a 2e-9
   tangential offset is ~1.3e-18 off-circle radially, far within band).
3. **Merge-mirroring**: the #143 uniform-slot merge decision for an image
   must equal its source's decision on the cap rim, or the counts desync
   again (symmetric today; a separation scheme must not break it — beware
   the "two distinct overrides claim uniform sample k" wall when separating
   a twin whose partner merged into the slot).
4. **Exact-order consistency**: the separated images' azimuth ORDER must
   match the cap ring's exact-tiebreak order, or the azimuth-merge strip
   twists (the `m8_holed_disc_coplanar_overlay` increment-3 lesson).
5. **Cross-cap bit-absorption**: when the opposite cap HAS its own coinciding
   crossings (stacked congruent caps — vertical features give bit-identical
   x,y overlays on both caps), projected images must keep bit-matching those
   own points (today: on-circle own points are bit-reproduced because the
   renormalisation computes scale = 1.0 exactly for them; the deep members'
   collapse is absorbed by the own set). A projection change that breaks the
   bit-match explodes the entry counts on healthy stacked laterals.

## Banked artifacts

- Probes: `[opp-proj]` (collision kind: PREEXISTING vs PAIRWISE-COLLAPSE),
  `[rim-count]`, `[ring-build]` (per-edge n_seg/overrides/merged/inserted/
  ring_len) — all under `YANG_SPLIT_PROBE=1`.
- Unit pins (`tests_unit/stage0_rim_projection.rs`): on-circle-within-band
  contract, same-ray-twin characterisation, renormalisation byte-pin.
- The `azimuth-merge rims have mismatched / too-few samples` wall remains the
  honest LOUD verdict for C0048/F0067 (never silent-wrong).

## Research basis

- [#24] Yang et al. 2025 §4.5.5 — shared-sample conformality of coplanar
  overlap boundaries across incident faces.
- [#52] Hobby snap rounding (family) — the deterministic separation grade the
  real fix needs; cited by the #142 fused-emission spec.
- Constitution P9/P10 — this abort; the translation arm was reverted rather
  than patched around.
