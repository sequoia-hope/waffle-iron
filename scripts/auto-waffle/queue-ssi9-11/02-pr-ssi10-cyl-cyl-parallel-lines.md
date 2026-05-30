# PR-SSI10 — ssi-rs cylinder∩cylinder PARALLEL axes → lines (M5 / roadmap §4b Phase 1)

Context: all four circle-reducible coaxial pairs now land (sphere∩cyl PR-SSI6,
sphere∩cone PR-SSI7, cyl∩cone PR-SSI8, cone∩cone PR-SSI9). The only remaining
degree-4 pair is **cylinder∩cylinder**, whose special cases split in two:
**parallel axes → lines** (THIS PR) and **equal-radius intersecting axes →
ellipses** (PR-SSI11, next). This PR creates the `cylinder_cylinder` solver and
implements ONLY the parallel-axis branch; everything non-parallel stays ASNA for
PR-SSI11 to extend.

Read `specs/ssi_pr_ssi8_cylinder_cone_coaxial.md` and `crates/ssi-rs/src/lib.rs`
(`cylinder_cone`, `plane_cylinder`, and the `SsiCurve::Line { point, dir }` and
`Cylinder { axis_point, axis_dir, radius }` definitions). Mirror the conventions.

Scope (special-cases-first):
- **Parallel axes** (`|û₁ × û₂| < TAU`, ûᵢ = normalized axis_dir): reduce to
  circle∩circle in the plane ⟂ û, extruded along û → lines parallel to û.
- **Non-parallel axes** → `Err(AnalyticalSolutionNotAvailable)` (general degree-4
  — includes the equal-R intersecting → ellipses case handled in PR-SSI11; loud,
  never a fallback — A15.2).

Verified reduction (parallel, common direction û = û₁; radii r₁, r₂; axis points
Q₁, Q₂): the inter-axis distance is the perpendicular component
`d = | (Q₂−Q₁) − ((Q₂−Q₁)·û)·û |`. This is circle∩circle (centres distance d,
radii r₁,r₂) lifted along û. Gate on the **linear** quantity d vs r₁±r₂ (the
SSI2/3/… lesson):
- `d > r₁+r₂ + TAU` or `d < |r₁−r₂| − TAU` → **empty** (0 lines).
- `|d − (r₁+r₂)| ≤ TAU` or `|d − |r₁−r₂|| ≤ TAU` → **tangent**, 1 line.
- otherwise → **2 lines**.
- `d ≈ 0` (axes coincide): if `|r₁−r₂| ≤ TAU` → **coincident cylinders**,
  `Err(DegenerateInput)`; else concentric → **empty**.

Cross-section construction: let n̂ = unit(perp component of Q₂−Q₁), and project Q₁
into the ⟂-plane as the cross-section centre c₁. The intersection point offset
along n̂ is `a = (d² + r₁² − r₂²) / (2d)`; the half-chord `h = √(max(0, r₁² − a²))`;
in-plane perpendicular `p̂ = û × n̂` (unit). The two cross-section points are
`c₁ + a·n̂ ± h·p̂`. Lift each to a `Line { point, dir = û }`. (Watch the √ never
sees a negative argument — the branch table already guarantees `r₁² ≥ a²` in the
2-line / tangent cases; clamp defensively and justify.)

E1 (`Err(DegenerateInput)`): rᵢ ≤ 0 / non-finite; zero or non-finite axis on
either cylinder.

Dispatch: wire `(Cylinder,Cylinder) => cylinder_cylinder(a,b)` in `intersect`
(same-type symmetric pair — one call). After this PR, NO dispatch arm returns ASNA
for an unhandled *pair* — but `cylinder_cylinder` itself returns ASNA for the
non-parallel sub-case (PR-SSI11 fills the equal-R intersecting part).

Oracles (P1, DoD §1): on-surface — for sample points along every result line,
BOTH cylinder radial residuals `| dist(x, axisᵢLine) − rᵢ |` within TAU; analytical
geometry (each line direction ∥ û; the cross-section points satisfy the
circle∩circle equations; symmetric ± placement about the centre line); branch
coverage (2 lines / tangent 1 line / empty / coincident-Err / concentric-empty /
non-parallel-ASNA / E1); symmetry `intersect(c₁,c₂)==intersect(c₂,c₁)`;
determinism (stable line order — e.g. the `+h·p̂` line first). Cite Patrikalakis
§5.8. No new `SsiCurve` variant ⇒ no enum-match migration. On completion, update
`docs/yang_functional_roadmap.md` (M5 step 10) and note PR-SSI11 (equal-R
intersecting → ellipses) is next.

Verified concrete cases (use as canonical tests):
1. Two lines (3-4-5): cyl₁ axis_point=origin, dir=+z, r₁=5; cyl₂ axis_point=
   (8,0,0), dir=+z, r₂=5; d=8. a=(64+25−25)/16=4, h=√(25−16)=3 → points (4,±3,*),
   two lines dir +z. Check (4,3,0): dist to z-axis = 5 = r₁; dist to (8,0,*) axis
   = √(16+9)=5 = r₂.
2. Tangent: cyl₁ r=2 @origin +z; cyl₂ r=2 @(4,0,0) +z; d=4=r₁+r₂ → 1 line at
   (2,0,*).
3. Empty: cyl₁ r=1 @origin; cyl₂ r=1 @(5,0,0); d=5 > 2 → 0 lines.
4. Coincident: identical cylinders → `Err(DegenerateInput)`. Concentric (same
   axis, r₁=1,r₂=2) → empty.
5. Non-parallel: cyl₂ dir=+x → ASNA. E1: r=0 → `Err(DegenerateInput)`.
