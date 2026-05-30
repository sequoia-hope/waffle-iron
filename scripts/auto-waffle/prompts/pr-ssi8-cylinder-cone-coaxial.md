# PR-SSI8 — ssi-rs cylinder∩cone coaxial → circles (M5 / roadmap §4b Phase 1)

Context: plane∩{plane,sphere,cylinder,cone} is complete (PR-SSI1–5); the first two
degree-4 coaxial pairs landed — sphere∩cylinder (PR-SSI6) and sphere∩cone
(PR-SSI7). Continue the degree-4 "special-cases-first" sequence: the next
circle-reducible coaxial pair is **cylinder∩cone**. Same pattern — coaxial reduces
to circles (reuse `SsiCurve::Circle`, no new curve type); general (non-coaxial)
position is the degree-4 curve, deferred behind `Err(AnalyticalSolutionNotAvailable)`.

Read `specs/ssi_pr_ssi7_sphere_cone_coaxial.md` and `crates/ssi-rs/src/lib.rs`
(`sphere_cone`, `sphere_cylinder`, `plane_cone`) for the established conventions
and the field layouts — `Cone { apex, axis_dir, half_angle }` (both nappes;
radial = |h|·tanα, h=(x−apex)·â) and `Cylinder { axis_point, axis_dir, radius }`
(dist(x, axis line) = radius). Mirror them.

Scope (special-cases-first, the PR-SSI6/7 pattern):
- **Coaxial** = the two axis *lines* coincide: axes parallel (`|ĉ × â| < TAU`) AND
  the cylinder `axis_point` lies on the cone axis line
  (`dist(axis_point, coneAxisLine) < TAU_MODEL`, the SSI7 `d_ax` test). Then the
  intersection is circles.
- **Non-coaxial** (axes not coincident — non-parallel, or parallel-but-offset) →
  `Err(AnalyticalSolutionNotAvailable)` (general degree-4, deferred; loud, never a
  fallback — A15.2).

Verified reduction (coaxial, cone apex P, unit axis â, half-angle α, cylinder
radius r_c): a cone point at axial height h has radial distance |h|·tanα from the
axis; it lies on the cylinder iff `|h|·tanα = r_c`, i.e. `|h| = r_c·cotα =
r_c / tanα`. The two roots `h = ± r_c·cotα` give **exactly two circles**
{ center = P + h·â, normal = â, radius = r_c } (h<0 is the other nappe of the
double cone).

**Branch structure differs from SSI6/SSI7 — read carefully.** There is NO
discriminant and NO tangent/empty sub-case in the coaxial config: the cone's
per-nappe radial range [0, ∞) meets the constant cylinder radius r_c at exactly
one h per nappe, so coaxial cyl∩cone is **always two distinct circles** for valid
inputs. Do NOT manufacture a discriminant or a one-circle/empty branch to mirror
SSI7 — that would be a hack-to-pattern (P9/P10). The only outcomes are: two circles
(coaxial) / NC (non-coaxial → ASNA) / E1 (degenerate).

E1 (`Err(DegenerateInput)`): r_c ≤ 0; α ∉ (0, π/2); zero/non-finite cone or
cylinder axis.

Dispatch: wire `(Cylinder,Cone) => cylinder_cone(a,b)`,
`(Cone,Cylinder) => cylinder_cone(b,a)` in `intersect`; the remaining degree-4
pairs (cyl∩cyl, cone∩cone) stay ASNA.

Oracles (P1, DoD §1): on-surface — for sample points on every result circle, the
cylinder radial residual `| dist(x, cylAxisLine) − r_c |` AND the cone radial
residual `| |(x−P)−h·â| − |h|·tanα |` (reuse the SSI3/SSI7 helper) — both within
TAU; analytical geometry (radius = r_c exactly, the two centers at P ± r_c·cotα·â
on the axis, normal ∥ â, the two h equal-and-opposite); branch coverage
(two-circles / NC / E1 — and assert there is **no** coaxial config yielding one or
zero circles); symmetry intersect(cyl,cone)==intersect(cone,cyl); determinism
(stable circle order — emit the h>0 nappe first). Cite Patrikalakis §5.8.3. No new
`SsiCurve` variant ⇒ no enum-match fixture migration. On completion, update
`docs/yang_functional_roadmap.md` (M5 step 8) and note the next pair (cone∩cone).

Verified concrete cases (use as canonical tests):
1. Cone apex=origin, axis=+z, half_angle=π/4 (tanα=1, cotα=1); cylinder
   axis_point=origin, axis_dir=+z, r_c=2 → two circles at z=±2, each radius 2,
   normal +z. Check point (2,0,2): on cone (radial 2 = |z|·tanα = 2), on cylinder
   (dist to z-axis = 2 = r_c).
2. cotα≠1: half_angle = atan(2) (tanα=2, cotα=0.5), r_c=3 → circles at z=±1.5,
   each radius 3.
3. NC: same cone, cylinder axis_point=(1,0,0) (offset off the cone axis) → ASNA.
   Also a non-parallel cylinder axis → ASNA.
4. E1: r_c=0 (or negative) → DegenerateInput.
