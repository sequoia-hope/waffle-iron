# PR-SSI9 — ssi-rs cone∩cone coaxial → circles (M5 / roadmap §4b Phase 1)

Context: the first three degree-4 coaxial pairs landed — sphere∩cyl (PR-SSI6),
sphere∩cone (PR-SSI7), cyl∩cone (PR-SSI8). This is the **last circle-reducible
coaxial pair: cone∩cone**. Same family pattern — coaxial reduces to circles
(reuse `SsiCurve::Circle`, no new curve type); general (non-coaxial) position is
the degree-4 curve, deferred behind `Err(AnalyticalSolutionNotAvailable)`.

Read `specs/ssi_pr_ssi8_cylinder_cone_coaxial.md` and `crates/ssi-rs/src/lib.rs`
(`cylinder_cone`, `sphere_cone`, `plane_cone`) for the established conventions and
field layout: `Cone { apex, axis_dir, half_angle }`, both nappes, radial
= |h|·tanα with h = (x−apex)·â. Mirror them.

Scope (special-cases-first):
- **Coaxial** = the two cone axis *lines* coincide: axes parallel (`|â₂ × â₁| <
  TAU`) AND apex₂ lies on cone₁'s axis line (`dist(apex₂, axis₁Line) < TAU_MODEL`,
  the SSI8 `d_ax` test). A double cone is symmetric under `â → −â`, so the axis
  *orientation* sign does not matter — only the half-angle and the apex position
  along the shared axis. Then the intersection is circles.
- **Non-coaxial** (apex₂ off axis₁, or axes not parallel) → `Err(
  AnalyticalSolutionNotAvailable)` (general degree-4, deferred; loud, never a
  fallback — A15.2).

Verified reduction (coaxial, shared unit axis â; cone₁ apex P₁, half-angle α₁;
cone₂ apex P₂, half-angle α₂; m₁=tanα₁, m₂=tanα₂; signed apex offset
δ = (P₂−P₁)·â). Parameterize by axial height t = (x−P₁)·â. A point is on both
cones iff `|t|·m₁ = |t−δ|·m₂`. Squaring is exact (both sides ≥ 0):
`(m₁²−m₂²)t² + 2m₂²δ·t − m₂²δ² = 0`, whose discriminant is the perfect square
`(2·m₁·m₂·δ)² ≥ 0`. Hence:

- **Unequal half-angles** (`|α₁−α₂| > TAU`, i.e. m₁≠m₂), **δ ≠ 0** → **always two
  circles** at `t = (−m₂²δ ± m₁·m₂·|δ|) / (m₁²−m₂²)`. (No empty/tangent branch —
  the discriminant is a perfect square, always real. Do NOT manufacture an empty
  branch; that would be hack-to-pattern, P9/P10.) If `δ = 0` the two roots
  collapse to t = 0 (apexes coincide → contact only at the shared apex, a
  radius-0 point-circle) → treat as **X0 empty** (no proper circle; note it like
  SSI7's apex-grazing point-circle).
- **Equal half-angles** (`|α₁−α₂| ≤ TAU`), **δ ≠ 0** → **one circle** at the
  bisector `t = δ/2`.
- **Equal half-angles**, **δ ≈ 0** → the two double cones **coincide** (identical
  surface) → `Err(DegenerateInput)` (not a curve; loud).

Each circle: `center = P₁ + t·â`, `normal = â`, `radius = |t|·m₁` (equals
`|t−δ|·m₂` by construction — assert this equality in the oracle; pick the `|t|·m₁`
form and justify). Gate the equal-vs-unequal split on the geometrically-meaningful
linear quantity `|α₁−α₂|` (or `|m₁−m₂|`), per the SSI2/3/6/7/8 lesson.

E1 (`Err(DegenerateInput)`): either αᵢ ∉ (0, π/2) / non-finite; zero or non-finite
axis on either cone.

Dispatch: wire `(Cone,Cone) => cone_cone(a,b)` in `intersect` (cone∩cone is a
same-type symmetric pair — one call, handle internal ordering/determinism). The
remaining degree-4 pair (cyl∩cyl) stays ASNA (PR-SSI10/11 next).

Oracles (P1, DoD §1): on-surface — for sample points on every result circle, BOTH
cone radial residuals `| |(x−Pᵢ)−hᵢ·âᵢ| − |hᵢ|·tanαᵢ |` within TAU (reuse the
SSI3/7/8 helper); analytical geometry (centers on the shared axis, normal ∥ â,
radius `|t|·m₁ = |t−δ|·m₂`, the two unequal-α roots match the quadratic, the
equal-α circle at the δ/2 bisector); branch coverage (two-circles / one-circle /
empty-point / coincident-Err / NC / E1) — and assert the unequal-α coaxial case is
**always** two circles (a small α₁/α₂/δ sweep, mirroring SSI8's anti-hack
invariant); symmetry `intersect(c₁,c₂) == intersect(c₂,c₁)`; determinism (stable
circle order — larger-t / h>0 first). Cite Patrikalakis §5.8.3. No new `SsiCurve`
variant ⇒ no enum-match migration. On completion, update
`docs/yang_functional_roadmap.md` (M5 step 9) and note the next increment is the
cyl∩cyl special cases (PR-SSI10 parallel→lines, PR-SSI11 equal-R→ellipses).

Verified concrete cases (use as canonical tests):
1. Unequal α, offset: cone₁ apex=origin, axis=+z, α₁=π/4 (m₁=1); cone₂ apex=
   (0,0,2) (δ=2), axis=+z, m₂=3 (α₂=atan 3). Roots t=(−9·2 ± 1·3·2)/(1−9)=
   (−18±6)/(−8) → t=1.5 and t=3. Two circles: z=1.5 r=1.5, z=3 r=3. Check cone₂ at
   z=3: |3−2|·3 = 3 ✓.
2. Equal α, offset: both α=π/4, cone₂ apex=(0,0,2) → one circle at z=1, r=1.
3. Coincident: same apex, same α=π/4 → `Err(DegenerateInput)`.
4. NC: cone₂ apex=(1,0,0) (off the z-axis) → ASNA.
5. E1: α=0 or α≥π/2 → `Err(DegenerateInput)`.
