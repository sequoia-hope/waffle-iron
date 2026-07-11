# M8 slice: torus-profile rim crossings (task #131)

Status: IMPLEMENTING
Drivers: R0025 / R0046 / R0050 — coplanar disc caps whose rims are TORUS
profile circles (revolved-circle tools). Stage-0 admits the pairs (1×1 disc
path, or the n-ary group after slice g), the overlap boundary crosses the
disc rim, and `collect_ring_crossings` dies at `rim-lateral-none`:
`lateral_for_cap` admits only `Surface::Cylinder` laterals with 2
full-circle rims. Canonical fixture: the KV6d 90° bent tube's seam disc
flush against a box face whose rectangle pokes across the rim (the
CONTAINED variant is green end-to-end today — the downstream KV6d torus
machinery already survives this class, so the slice can close it).

## 1. Scope

1. **Cap-lateral classification** (`stage0/rim_chords.rs`): generalize
   `lateral_for_cap` into a typed `CapLateral` — the existing CYLINDER arm
   byte-identical, plus a TORUS arm: the cap edge appears in a
   `Surface::Torus` face's outer loop; its 2 distinct full-circle rims of
   radius ≈ minor (band 1e-9·(1+R+r)) are the profile circles; the
   opposite rim is the other one. Guards stay loud
   (`rim-lateral-torus-not-2profile`).
2. **Poloidal opposite-rim projection** (`collect_ring_crossings`): a
   crossing point on a profile circle carries an intrinsic poloidal angle
   φ = atan2(τ, ρ − R) (τ = axial component about the torus axis, ρ =
   distance from the axis — the SAME convention as
   `tessellate_torus_face`'s `phi_slot`). Mint the opposite point on the
   opposite profile circle at the same φ:
   `c₁ + r₁(cos φ · u + sin φ · a)` with `u` = the outward radial unit at
   the opposite meridian (from the axis to c₁) and `a` = the torus axis —
   exact for equal and unequal profile radii, 1:1 (no grid search), so the
   two rims keep matched sample counts.
3. **Grid conformality** (`tessellate_torus_face`): the structured (θ×φ)
   grid slot-aligns profile rings assuming UNIFORM φ sampling
   (`phi_slot` rounds to 2π/n_phi slots) — inserted crossing samples break
   that. Align columns by the rings' ACTUAL intrinsic φ values instead:
   sort ring0's φ list (grid column angles), match rowa's vertices to
   columns by nearest φ (tolerance = half the minimum column gap, loud on
   ambiguity), and sample interior rows at the column φ values. Uniform
   rings reproduce today's grid (the column set is the uniform set).
   `tessellate_torus_band` (holed/periodic) is NOT generalized in this
   slice — mismatched rings there stay the loud typed error.

## 2. Branch table

| # | Branch | Behavior |
|---|---|---|
| B1 | cap edge on a cylinder lateral | today's path, byte-identical |
| B2 | cap edge on a torus lateral with exactly 2 minor-radius full-circle profile rims | poloidal projection (NEW) |
| B3 | torus lateral without 2 clean profile rims | loud `rim-lateral-torus-not-2profile` |
| B4 | cap edge on neither | loud `rim-lateral-none` (unchanged) |
| B5 | torus grid, uniform rings | today's grid (column set = uniform set) |
| B6 | torus grid, inserted samples with matched φ sets | φ-value columns (NEW) |
| B7 | torus grid, rings with mismatched counts / ambiguous nearest-φ match | loud `MalformedTopology` (unchanged failure class) |

## 3. Invariants

- I1: cylinder cap-lateral behavior byte-identical (B1).
- I2: the minted opposite point lies ON the opposite profile circle
  (residual ≤ import band) at the cap crossing's intrinsic φ.
- I3: both profile rings of a torus face keep equal sample counts through
  Stage-0 overrides (paired insertion), and the grid consumes them
  column-for-column (watertight against both seam discs).
- I4: corpus zero-lost; R0025/R0046/R0050 re-censused (may land on deeper
  typed walls — KV6d downstream is the ceiling).

## 4. Oracles

- `kv6d_torus_boolean.rs`:
  - `flush_box_crossing_seam_disc_union` (RED → green): bent tube ∪ flush
    box crossing the θ=0 profile rim → watertight, χ=2, torus face
    survives, volume ∈ (V_tube, V_tube + V_box).
  - contained-variant canary (already green) stays green.
- assay: R0025/R0046/R0050 re-census + full P9 zero-lost gate.

## 5. Research basis

- [#24] Yang 2025 §4.5.5: overlap boundaries become intersection curves;
  shared boundary sampling must propagate into every face sharing the
  subdivided edge — the torus profile rim is shared by the seam disc and
  the torus band exactly as a cylinder rim is shared by cap and lateral.
- KV6d torus tessellation (θ×φ bijective grid) — the conformality target.

## 6. Ledger

- 2026-07-11: spec written; contained-variant probe GREEN end-to-end,
  crossing variant red at `rim-lateral-none` (1×1 path — no n-ary needed
  for the driver).
- 2026-07-11: SHIPPED. Driver red→green in two increments (CapLateral torus
  arm + poloidal projection; φ-value grid columns with the uniform path
  kept byte-identical as the FIRST arm). Ring matching is INDEX-WISE on the
  sorted seam-anchored offsets with a fixed 1e-9 band — a min-gap-derived
  tolerance collapses under femto-close crossing twins (R0050's Δφ≈9e-16
  vs a 4e-16 twin gap). Corpus: R0046 UNSUPPORTED→SUPPORTED_CORRECT;
  R0025/R0050 → typed Stage-4 LocalRefinementRequired (the N2 epic class);
  R0085's UNSUPPORTED(revolve) verdict was HIDING this wall (categorizer
  keys on the feature name) → now the pre-existing CDT wall. Assay
  **237C/0W/49E/9U/0T**, zero CORRECT→worse. The
  UNSUPPORTED(coplanar) rim-lateral-none mechanism is fully retired.
