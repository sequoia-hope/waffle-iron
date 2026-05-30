# PR-SSI11 — ssi-rs cylinder∩cylinder EQUAL-R intersecting axes → two ellipses (M5)

Context: PR-SSI10 created the `cylinder_cylinder` solver with the **parallel-axis →
lines** branch and left every non-parallel case as `Err(
AnalyticalSolutionNotAvailable)`. THIS PR **extends** `cylinder_cylinder` with the
next special case: **equal radius + coplanar intersecting axes → two ellipses**
(the classic Patrikalakis §5.8 result). Everything still uncovered (unequal radius,
or skew axes) stays ASNA — that is the general degree-4 curve, deferred to the
Phase-1 general solvers.

Read `specs/ssi_pr_ssi10_cyl_cyl_parallel_lines.md` (the solver you are extending),
`crates/ssi-rs/src/lib.rs` (`cylinder_cylinder`, and `SsiCurve::Ellipse { center,
normal, major_axis, major_radius, minor_radius }` — note the contract
`major_radius ≥ minor_radius` and `minor` lies along `normal × major_axis`).
Mirror the conventions.

Scope — add ONE branch to `cylinder_cylinder`, do not disturb SSI10's parallel/
lines path:
- **Equal radius AND axes intersect** (`|r₁−r₂| ≤ TAU`; non-parallel
  `|û₁×û₂| ≥ TAU`; coplanar/intersecting: closest-approach distance
  `|(Q₂−Q₁)·(û₁×û₂)| / |û₁×û₂| < TAU_MODEL`) → **two ellipses**.
- Otherwise non-parallel (unequal R, or skew) → keep `Err(
  AnalyticalSolutionNotAvailable)` (general degree-4; loud, never a fallback).

Verified reduction (equal radius r, unit axes û₁,û₂, intersection point O, angle
β = acos(û₁·û₂) ∈ (0, π)): the intersection is two ellipses, one in each
axis-bisector plane. Build the orthonormal frame
`b̂₊ = unit(û₁+û₂)`, `b̂₋ = unit(û₁−û₂)`, `ŵ = unit(û₁×û₂)`
(b̂₊ ⟂ b̂₋ since (û₁+û₂)·(û₁−û₂)=0; both ⟂ ŵ as the axes are coplanar). Then:
- **Ellipse A**: `center = O`, `normal = b̂₋`, `major_axis = b̂₊`,
  `major_radius = r / sin(β/2)`, `minor_radius = r`.
- **Ellipse B**: `center = O`, `normal = b̂₊`, `major_axis = b̂₋`,
  `major_radius = r / cos(β/2)`, `minor_radius = r`.

Derivation note (so this is not ad-hoc): the equidistant-from-both-axes locus is
the two bisector planes; cutting cylinder₁ (radius r) with bisector plane P makes
the section an ellipse whose semi-minor is r (the across-axis width, along ŵ) and
semi-major is `r / sin ψ`, where ψ is the angle between the axis and the plane —
`ψ = β/2` for plane A (normal b̂₋) and `ψ = π/2 − β/2` for plane B (normal b̂₊),
giving `r/sin(β/2)` and `r/cos(β/2)`. Both `≥ r`, so the `major ≥ minor` contract
holds on (0,π) (equality only at the parallel/antiparallel limits, which are
excluded — parallel is SSI10's lines branch). Cite Patrikalakis §5.8.

Finding O: intersect the two axis lines (standard line-line intersection; the
coplanar/intersecting test above guarantees a solution within TAU). Watch
`sin(β/2)`, `cos(β/2)` never hit 0 (β∈(0,π) open ⇒ both bounded away from 0 on any
non-parallel input; the parallel guard already excludes β→0, and β→π is also
parallel — guard both).

E1 (`Err(DegenerateInput)`): rᵢ ≤ 0 / non-finite; zero or non-finite axis. (Reuse
SSI10's E1 — it already runs before this branch.)

Oracles (P1, DoD §1) — the **on-surface oracle is the load-bearing safety net
here**, exercise it densely: for many sample points on BOTH result ellipses,
assert both cylinder radial residuals `| dist(x, axisᵢLine) − r |` within TAU.
Plus analytical geometry (both centers = O; normals b̂₋/b̂₊; major axes b̂₊/b̂₋;
major_radius = r/sin(β/2), r/cos(β/2); minor_radius = r; `major ≥ minor`
contract); branch coverage (two ellipses / equal-R-but-skew → ASNA / unequal-R →
ASNA / parallel still → lines from SSI10 / E1); symmetry
`intersect(c₁,c₂)==intersect(c₂,c₁)`; determinism (stable ellipse order — e.g.
the b̂₋-normal ellipse first). Test at **more than one angle** (90° canonical
below AND a non-perpendicular angle, e.g. 60°) so nothing is hardcoded to 90°. No
new `SsiCurve` variant ⇒ no enum-match migration. On completion, update
`docs/yang_functional_roadmap.md` (M5 step 11) and note that **all
circle/conic-reducible coaxial & special cases are now complete** — the next
increment is the general degree-4 curve (new parametric `SsiCurve` variant + the
general-position solvers), which should be planned with a human before
implementing.

Verified concrete case (canonical): cyl₁ axis_point=origin, dir=+x, r=2; cyl₂
axis_point=origin, dir=+y, r=2 (β=90°, O=origin). Frame: b̂₊=(1,1,0)/√2,
b̂₋=(1,−1,0)/√2, ŵ=(0,0,1).
- Ellipse A: center O, normal (1,−1,0)/√2, major_axis (1,1,0)/√2,
  major_radius = 2/sin45° = 2√2, minor_radius = 2.
- Ellipse B: center O, normal (1,1,0)/√2, major_axis (1,−1,0)/√2,
  major_radius = 2/cos45° = 2√2, minor_radius = 2.
Spot checks: major end of A `O + 2√2·b̂₊ = (2,2,0)` → dist to x-axis √(4+0)=2 ✓,
dist to y-axis √(4+0)=2 ✓. Minor end `O + 2·ŵ = (0,0,2)` → dist to x-axis 2 ✓,
dist to y-axis 2 ✓.
Negative cases: cyl₂ dir=+y but r₂=3 (unequal) → ASNA; cyl₂ axis_point=(0,0,5)
dir=+y (skew, perpendicular but offset in z) → ASNA.
