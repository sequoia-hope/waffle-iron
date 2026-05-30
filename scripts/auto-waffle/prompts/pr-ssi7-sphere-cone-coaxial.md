# PR-SSI7 — ssi-rs sphere∩cone coaxial → circles (M5 / roadmap §4b Phase 1)

Context: plane∩{plane,sphere,cylinder,cone} is complete (PR-SSI1–5) and the first
degree-4 pair (sphere∩cylinder coaxial, PR-SSI6) landed. Continue the degree-4
"special-cases-first" sequence: the next circle-reducible coaxial pair is
**sphere∩cone**. Same pattern as PR-SSI6 — coaxial reduces to circles (reuse
`SsiCurve::Circle`, no new curve type); the general (non-coaxial) position is the
degree-4 curve, deferred behind `Err(AnalyticalSolutionNotAvailable)`.

Read `specs/ssi_pr_ssi6_sphere_cylinder_coaxial.md` and `crates/ssi-rs/src/lib.rs`
(`sphere_cylinder`, `plane_cone`) for the established conventions; mirror them.

Scope (special-cases-first, the PR-SSI6 pattern):
- **Coaxial** = the sphere center lies on the cone's axis line
  (`dist(center, axisLine) < TAU_MODEL`). Then the intersection is circles.
- **Non-coaxial** → `Err(AnalyticalSolutionNotAvailable)` (general degree-4,
  deferred; loud, never a fallback — A15.2).

Verified reduction (coaxial, apex P, unit axis â, half-angle α, sphere center
C = P + h0·â on the axis, radius r_s): a cone point at axial height h has radial
distance |h|·tanα from the axis; it lies on the sphere iff
`(h − h0)² + h²·tan²α = r_s²`, i.e. `sec²α·h² − 2h0·h + (h0² − r_s²) = 0`, giving
`h = ( h0 ± √D ) / sec²α`, `D = sec²α·r_s² − h0²·tan²α`. Each real root h → one
Circle { center = P + h·â, normal = â, radius = |h|·tanα } (h<0 is the other
nappe of the double cone). Branch on D: D > TAU-band → two circles; |D| ≤ band →
one tangent circle; D < −band → empty. (Gate on the geometrically-meaningful
linear quantity, per the SSI2/3/6 lesson — choose the right discriminant form and
justify it; watch the √ never sees a negative argument.) E1: r_s ≤ 0,
α ∉ (0, π/2), or zero/non-finite axis → `Err(DegenerateInput)`.

Dispatch: wire `(Sphere,Cone) => sphere_cone(a,b)`, `(Cone,Sphere) => sphere_cone(b,a)`
in `intersect`; the remaining degree-4 pairs (cyl∩cyl, cyl∩cone, cone∩cone) stay ASNA.

Oracles (P1, DoD §1): on-surface (sphere residual + cone RADIAL residual
`| |(x−P)−h·â| − |h|·tanα |`, reuse the SSI3 helper) for every result circle;
analytical geometry (radius |h|tanα, centers on the axis, normal ∥ â, the two
circles' h match the quadratic roots); branch coverage (two/one/empty/NC/E1);
symmetry intersect(sphere,cone)==intersect(cone,sphere); determinism (stable
circle order). Cite Patrikalakis §5.8.3. No new `SsiCurve` variant ⇒ no enum-match
fixture migration. On completion, update `docs/yang_functional_roadmap.md`
(M5 step 7) and note the next pair.

Verified concrete case (use as the canonical test): cone apex=origin, axis=+z,
half_angle=π/4 (tanα=1, sec²α=2); sphere center=origin (h0=0) on the axis, r_s=2.
Then D = sec²α·r_s² − h0²·tan²α = 8 > 0 → two circles at z=±√2, each radius √2,
normal +z. Check point (√2,0,√2): on cone (radial √2 = z·tanα = √2), on sphere
(|·|=2). An asymmetric coaxial case (h0≠0, e.g. h0=3,r_s=4) exercises non-symmetric
roots; h0=3,r_s=2 → D<0 → empty.
