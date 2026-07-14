# R0026 — exact cylinder∩plane generator-line membership band (N46)

**Task:** #164. **Status:** DIAGNOSED → fix landing. **Deviation:** N46.
**Class:** Stage-3 SSI curve selection (sibling of N38/N39 band-derivation fixes,
and of the R0072/R0008 generator-line selection work).

## Goal

Retire R0026's spurious `AmbiguousCurve { candidates: 2, matched: 0 }` by
measuring a `cylinder ∩ plane` **generator-line** membership band with the
EXACT radial→perpendicular metric conversion instead of its first-order
linearization. No band widening: the exact band is a derived consequence of the
same Stage-1 chord contract the linear factor approximates.

## Reproduce

```
YANG_S3_AMBIG_PROBE=1 ASSAY_CASE=R0026 ASSAY_CASE_TIMEOUT_SECS=120 \
  cargo test -p test-harness --test assay_kv2 --release single_case -- --ignored --nocapture
```

## Measured defect (R0026, edge (131,197))

A cutting plane **parallel to the cylinder axis** (both candidate lines carry
`dir == axis_dir`) sections the cylinder into **two parallel generator lines**.

- Cylinder `R = 0.0357580`, axis-to-plane distance `d = 0.0307460` (`d/R = 0.860`
  — near-tangent-ish).
- Base chord band `tol = 1.498e-3` (the cylinder's own Stage-1 `curved_chord_bound`).
  The on-both-surfaces gate passes: both endpoints are radially `≈1.43e-3` inside
  the cylinder (`< tol`) and `≈5e-18` off the plane (on it).
- Correct generator (`cand 1`): both endpoints' perpendicular distance is
  `p_s = 2.990e-3`, `p_e = 2.982e-3`.
- Wrong generator (`cand 0`): both endpoints `≈3.35e-2` away (11× farther).
- **Linear band** `amp·tol = (R/√(R²−d²))·tol = 1.959 · 1.498e-3 = 2.934e-3`.
  Both endpoints (`2.99e-3`) fall **just outside** it → `matched = 0` → loud
  `AmbiguousCurve`. The margin is only ~2 %.

## Root cause

`line_band_amplification` returns the constant `amp = R/√(R²−d²)`, which is the
**derivative** (tangent slope) of the in-plane offset `η(radial) = √(radial²−d²)`
at `radial = R`. The map from a point's radial distance to its in-plane
perpendicular distance to the generator is this `η`, and `η` is **concave**
(`η'' = −d²/(radial²−d²)^{3/2} < 0`). For a point radially `ρ = tol` *inside* the
cylinder the exact offset drop

```
B_in = √(R²−d²) − √((R−tol)²−d²)
```

is therefore **larger** than the tangent estimate `amp·tol`, and the gap grows as
`d → R` (near tangency, where `η` is steep). R0026 sits exactly there: the linear
band under-predicts by ~7 %, enough to reject a legitimate chord point.

`curve_contains_point` for a `Line` measures the 3-D perpendicular distance to
the line. A point within radial `tol` of the cylinder AND within `tol` of the
plane is displaced from the generator by an in-plane-perpendicular component
`≤ B_in` and an out-of-plane component `≤ tol`, and these two directions are
mutually orthogonal and both perpendicular to the (in-plane) generator
direction. So the exact worst-case membership band is

```
band = √( B_in² + tol² )
```

The inside case (`radial = R − tol`) dominates the outside case
(`√((R+tol)²−d²) − √(R²−d²)`) by concavity, so `B_in` is the worst in-plane term.

For R0026: `B_in = 3.143e-3`, `band = √(B_in² + tol²) = 3.482e-3`. Both endpoints
(`2.99e-3`) are admitted; the wrong generator (`3.35e-2`) stays rejected →
`matched = 1`. R0026 advances past Stage-3.

## Fix

`stage4_relocate.rs::cyl_plane_generator_band(surf0, surf1, tol) -> Option<f64>`
returns `√(B_in² + tol²)`. Wired into `stage3_ssi.rs::build_intersection_curves`'s
`point_tol` closure: for the `Line` arm, use `cyl_plane_generator_band` when the
pair is `cylinder ∩ plane`; otherwise fall back to the existing
`line_amp.map_or(tol, |a| a·tol)` (unchanged for `cylinder ∩ cylinder` Steinmetz
generators and `cone ∩ plane` apex lines).

## Branch table

| pair | Line-arm band |
|---|---|
| cylinder ∩ plane, `d < R`, `R − tol > d` | `√(B_in² + tol²)` (**this fix**) |
| cylinder ∩ plane, `R − tol ≤ d` (near-tangent) | `None` → loud stop stays (task #137) |
| cylinder ∩ plane, `d ≥ R` (plane misses) | `None` → linear fallback (no real generators) |
| cylinder ∩ cylinder | `line_amp` Steinmetz `1/sinα` (unchanged) |
| cone ∩ plane (apex lines) | raw `tol` (unchanged) |

## Invariants / oracles

- **O1 (concavity)** `cyl_plane_generator_band > line_band_amplification·tol` for
  a finite `tol` with `d < R` (the exact band strictly exceeds its linearization).
- **O2 (load-bearing RED)** with R0026's geometry, `curve_contains_point(correct
  generator, endpoint, linear_band)` is `false` but `curve_contains_point(...,
  exact_band)` is `true`.
- **O3 (no false positive)** `curve_contains_point(wrong generator, endpoint,
  exact_band)` stays `false` (the 11×-farther generator is not admitted).
- **O4 (None guards)** `cyl_plane_generator_band` returns `None` for a
  non-cyl/plane pair, for `d ≥ R`, and for `R − tol ≤ d`.
- **O5 (assay zero-regression)** full release corpus stays `≥ 239C / 0W`; R0026
  moves off `AmbiguousCurve` (to its next honest wall).

## Failure modes

- `R − tol ≤ d`: a radially-inside chord point no longer reaches the plane — the
  two generators have merged below mesh resolution (genuine near-tangency).
  Return `None`; the loud `AmbiguousCurve` stands (P9 — never widen into a
  tangency we cannot resolve). This is task #137 territory.

## What NOT to do

- Do not extend the `matched == 0` case with a "pick the nearest anyway"
  fallback — that is band-widening in disguise (P9). The fix is a tighter,
  EXACT band, not a looser selection rule.
- Do not change `line_band_amplification` itself: the `cylinder ∩ cylinder`
  Steinmetz branch and the stage-4 `LineReloc` budget rely on its current
  linear form; scope the exact band to stage-3 cyl∩plane Line membership.

## Research basis

Yang et al. 2025 §4.3 (surface-pair curve refinement, chord-band membership).
Same derived-metric-conversion principle as PR-YR19 (sphere section-circle
`R/r_c`), N38 (cone band-to-owner match), N39 (cone∩plane conic `1/sinα`).
