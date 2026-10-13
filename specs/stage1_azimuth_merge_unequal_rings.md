# Stage 1 — the azimuth-merge band as a ZIPPER of two sorted rings

**Status:** LANDED 2026-10-13 (the wedge-arm follow-up of the P0057 landing;
customers P0044 / P0054 / P0058, guard P0047). Owner: `yang-rs`
`stage1_tessellate::tessellate_band_azimuth_merge` and its two dispatchers
(`tessellate_lateral_azimuth_merge` for the cylinder tube, the cone frustum
band in `surface_face_tessellators.rs`).

## 1. The defect, measured

The P0057 landing (`docs/yang_tail_triage.md` §"2026-10-13 (P0057)") found
that the §4.3.3 WEDGE arm's localization, `segment_risk_intervals`, cuts a
segment only at the roots of ρ(t) = r − thresh (the thresh-offset surface)
and classifies each window ONCE at its midpoint. A segment that also pierces
the surface's infinite extension — at a parameter inside or outside the band
the caller cares about — changes the predicate's value at that piercing too,
so the window straddling it is classified on the wrong side and the derived
span is too wide or absent. Adding the surface's own roots as cuts converts
P0044 / P0054 / P0058 and holds P0031 / P0057, but moves P0047 onto a NEW
Stage-1 STOP in a different op:

```
azimuth-merge rims have mismatched / too-few samples (23 vs 24)
```

That op's two rims carry 14 and 15 override points (the LOCAL graze samples,
equal per closure by `RimAngleOverrides`, plus a per-rim mint source); the
corrected spans move a Stage-0 disc-rim crossing onto a rim, `inserted_rims`
selects the azimuth-merge strip, and that strip paired the two sorted rings
POSITIONALLY — `bot[k] ↔ top[(k + shift) % n]` — refusing unequal counts
and any pairwise azimuth disagreement past `tol`.

## 2. Contract

The band between two coaxial rims is the zipper of two sorted circular
polylines. Given the bottom ring sorted by shared-frame azimuth
(`bot[0..m]`) and the top ring likewise (`top[0..n]`, rotated so `top[shift]`
is circularly nearest `bot[0]` — the pre-existing F0086 wrap alignment):

1. **Unwrapped azimuths.** `ub[i]` = cumulative azimuth from `bot[0]`
   (`ub[m] = 2π`); `ut[j]` likewise from the start partner, with `ut[0]` the
   SIGNED circular offset of `top[shift]` from `bot[0]` (it may sit a hair
   before, across the 0/2π cut) and `ut[n] = ut[0] + 2π`.
2. **Walk.** From the start pair `(bot[0], top[shift])`, while either ring
   has edges left: if both do and `|ub[i+1] − ut[j+1]| ≤ tol`, the next
   samples agree — close the QUAD split along `b0 → t1` (`[b0, b1, t1]`,
   `[b0, t1, t0]`, the positional strip's own split) and advance both;
   otherwise the ring whose next sample comes first advances alone and
   closes a TRIANGLE against the other ring's current sample. When one
   ring is exhausted the other fans to the wrapped start sample.
   `tol = (2π / max(m, n)) · 0.25`.
3. **Orientation.** Every triangle is oriented against the caller's target
   (`orient_tri`), exactly as before.
4. **Postcondition (loud).** A zipper of `m + n` rim edges emits exactly
   `m + n` triangles; anything else is a typed STOP. By construction every
   rim edge is used once and every diagonal twice, so the strip is a closed
   annulus for ANY two rings of ≥ 3 samples.

**Byte-identity.** Two equal rings whose sorted azimuths pair within `tol`
at every slot take the quad branch at every step, in slot order — the
triangle list is identical to the positional strip's (pin
`equal_matched_rings_are_byte_identical_to_the_positional_strip`).

**What is no longer refused.** Unequal counts, and equal counts whose
samples disagree past `tol` (a phase offset). Neither is a malformed input:
a band whose rims carry distinct sample sets is ordinary geometry with one
honest triangulation. The refusal protected the POSITIONAL pairing from
twisting; the zipper cannot twist.

**Dispatch.** The uniform cylinder tube and the uniform cone frustum band
keep their index pairing for equal rings (byte-identical) and route
UNEQUAL rings to the zipper instead of refusing them.

## 3. Companion change: `segment_risk_intervals` cuts at surface roots

`boolean/rim_junction.rs::segment_risk_intervals` adds the roots of
ρ(t) = L_surface(t) (cylinder: `l0 = radius`; cone: `l0 = 0`, the same
affine `l1 = tan α`) to its breakpoints beside the thresh-offset roots. The
window classification is unchanged (exact signed distance at the midpoint);
only the breakpoints are complete. Pin
`risk_span_of_a_piercing_segment_is_the_shallow_run_inside`: a segment from
ρ = 0.5 to ρ = 1.5 through a unit cylinder at thresh 0.1 has the at-risk run
t ∈ (0.4, 0.5) exactly; without the surface root the one window (0.4, 1.0)
read "outside" at its midpoint and nothing was at risk.

## 3b. Companion change: one rim sample per model-scale point

Landing §2 + §3 alone moved P0047 (and P0044's seed-4 lineage) onto the
inc-4e junction splice's loud refusal `interior junction … within the weld
band of 2 (3) distinct grid edges`. Measured (`YANG_SPLICE_PROBE`,
`YANG_SPLIT_PROBE`): the junction sat exactly on a ruling, and the rim
carried TWO samples at that azimuth — two free §4.3.3 samples from two arms
spelling one point 1 ULP apart (`0.000121` vs `0.00012099999999999999`), and
on the next site two sites' span endpoints 7.5e-4 rad apart on a 9e-5 radius
(6.8e-8 of arc, below `TAU_MODEL`) — so the cap fan held a 1-ULP sliver and
the lateral two coincident rulings, both within the splice's band. The ring
builder's bit-exact override dedup (E-C1) is right for Stage-0 crossing
twins ≥ 1e-10 apart that the cap overlay carries both of; it is blind to
sample REQUESTS below the model's own coincidence band. Two rules, both
A14.2 (nothing below `TAU_MODEL·(1 + scale)` is a representable feature):

1. **`RimAngleOverrides::finish`** (every LOCAL rim-refinement arm): two
   sample requests on one rim within the band of each other are one request
   (the first in azimuth order stands); a request within the band of a point
   the rim ALREADY carries — an earlier arm's sample, a Stage-1 junction
   mint, a Stage-0 crossing standing on the operand — is that point and is
   dropped (the standing point keeps its bits; a junction's are shared with
   the partner's edge chain).
2. **`brep::merge_rim_points`** (the composition of a B-Rep's STANDING rim
   samples with an incoming override map — every `rebuilt_with_*_overrides`
   and the Stage-0 mesh build): an incoming point within `TAU_WORK` — the
   provenance spec's ULP-twin discriminator, NOT the model band: a first cut
   used `TAU_MODEL·(1 + scale)` here and the corpus refuted it (R0072, whose
   ~1e-7 micro-scale edge carries real rim points 6e-8 apart, went
   non-manifold) — of a standing point takes that slot with the incoming bits (the arrival is
   either exact — a crossing, a junction — and rightly wins, or a free
   sample from an arm that ran on the un-rebuilt operand and could not see
   the standing one, in which case either spelling serves). Each standing
   point is replaced at most once and incoming points never absorb each
   other, so a band-close pair arriving together — E-C1's population — still
   both enter; the ring builder's bit-exact policy for overrides is
   UNCHANGED (a first cut put a `TAU_WORK` twin rule in the ring builder
   itself and the E-C1 pins `rim_override_ulp_twins_exact_order_both_rims`
   and `rim_override_band_close_distinct_points_both_inserted` refuted it:
   the cap overlay carries both members of a crossing twin pair, and the
   ring must too). Both the cap fan and the lateral read the one composed
   ring, so the merge is conformal by construction.

Pins: `merge_rim_points_replaces_a_standing_twin_once`,
`finish_merges_sample_requests_within_the_model_band`,
`finish_drops_a_free_sample_the_rim_already_carries`,
`uniform_tube_routes_unequal_rings_to_the_zipper`.

## 4. Non-goals

- `RimAngleOverrides`' per-closure equal azimuth sets stay as they are: the
  §4.5.2 domain lens (P0031) relies on each rim's own inside-sweep subset and
  on the shared set per closure; the zipper merely stops the leftover
  per-rim differences from being a STOP.
- No vertex is created or moved; the zipper is connectivity only.

## 5. Oracles

- Pins in `tests_unit/s1_azimuth_merge_unequal_rings.rs`: byte-identity for
  equal matched rings; one extra sample on one rim (P0047's shape);
  arbitrary unequal sets; phase-offset equal rings; a start partner across
  the wrap — each checked for `m + n` triangles, every rim edge used once,
  every diagonal used twice with opposite directions, every triangle outward
  and non-degenerate.
- The categorized corpus (release, 8 jobs, 900 s): the expected moves are
  P0044 / P0054 / P0058 ERROR → CORRECT with P0047 / P0031 / P0057 holding;
  the measurement is in `docs/yang_tail_triage.md` §"2026-10-13 (wedge)".
