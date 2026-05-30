# PR-SSI11 — cyl∩cyl EQUAL-R intersecting axes → two ellipses (M5 Step 11)

## Summary

Extend `crates/ssi-rs`'s `cylinder_cylinder` solver (PR-SSI10) so that the
single `Err(AnalyticalSolutionNotAvailable)` (ASNA) arm that currently covers
**every** non-parallel configuration is split to handle the next
analytically-solvable special case from Patrikalakis & Maekawa
*Shape Interrogation for CAD/M* §5.8 (natural quadrics):

> **Two cylinders of equal radius whose axes are coplanar and intersect →
> the intersection degenerates into two ellipses** lying in the two planes
> that bisect the angle between the axes.

Everything still uncovered — **unequal radius**, or **skew (non-coplanar) axes**
— remains the general degree-4 space curve and stays staged ASNA (A15.2: loud,
never a silent fallback). This completes all circle/conic-reducible coaxial &
special quadric cases for cyl∩cyl. The next increment (general degree-4 curve +
a new parametric `SsiCurve` variant) is a human-planned step and is out of scope
here.

No new `SsiCurve` variant is introduced — the result reuses the existing
`Ellipse { center, normal, major_axis, major_radius, minor_radius }`. Contract:
`major_radius ≥ minor_radius`; the minor direction is `normal × major_axis`
(unit). Reusing the variant means **no exhaustive-match fixture migration** in
the ssi1–ssi10 suites.

## References

- Patrikalakis & Maekawa, *Shape Interrogation for CAD/M*, §5.8 (natural
  quadrics, cylinder/cylinder intersection special cases).
- Architectural Invariant A15.2 — analytical primacy; staged solvers return a
  loud `Err(AnalyticalSolutionNotAvailable)` for configurations without a
  closed form rather than a numerical fallback.

## The math

Let the two cylinders have equal radius `r`, unit axis directions `û₁, û₂`, and
axis lines that are **coplanar and intersect** at a point `O`. Define
`β = acos(û₁·û₂) ∈ (0, π)` (the angle between the axes; the parallel limits
β→0 and β→π are excluded by the non-parallel guard).

Build the mutually-orthonormal frame:

- `b̂₊ = unit(û₁ + û₂)`  — bisector of the two axes
- `b̂₋ = unit(û₁ − û₂)`  — bisector of the axis and the reflected axis
- `ŵ  = unit(û₁ × û₂)`  — normal to the plane containing both axes

These are mutually orthogonal: `(û₁+û₂)·(û₁−û₂) = |û₁|² − |û₂|² = 0`, and
`ŵ ⟂` both by construction.

The two intersection curves are ellipses, both centred at `O`, each lying in a
plane spanned by `ŵ` and one of the bisectors, with semi-minor axis `r` (the
across-the-axis width, along `ŵ`) and semi-major axis `r / sin ψ`, where `ψ` is
the half-angle of the relevant bisecting plane:

- **Ellipse A** (emitted first — determinism I5):
  - `center      = O`
  - `normal      = b̂₋`
  - `major_axis  = b̂₊`
  - `major_radius = r / sin(β/2)`
  - `minor_radius = r`
  - `eval`'s minor direction is `normal × major_axis = b̂₋ × b̂₊ ∝ +ŵ` (unit,
    length r) — the across-axis width.
- **Ellipse B**:
  - `center      = O`
  - `normal      = b̂₊`
  - `major_axis  = b̂₋`
  - `major_radius = r / cos(β/2)`   (= `r / sin(π/2 − β/2)`)
  - `minor_radius = r`

On `β ∈ (0, π)`: `sin(β/2) ∈ (0,1)` and `cos(β/2) ∈ (0,1)`, so both
`major_radius ≥ r`, with equality only at the excluded parallel limits — the
`major_radius ≥ minor_radius` contract holds strictly inside the domain.

### Intersection point of the two axes

`O = axis₁ ∩ axis₂` via the standard two-line closest-point. With
`d1 = û₁`, `d2 = û₂`, `w0 = Q₁ − Q₂`, `b = d1·d2`, `dd = d1·w0`, `ee = d2·w0`,
the parameter on line 1 is `sc = (b·ee − dd) / (1 − b²)`, and `O = Q₁ + sc·d1`.
The denominator `1 − b² = 1 − (û₁·û₂)² = sin²β` is bounded away from 0 by the
non-parallel guard (`|û₁×û₂| ≥ TAU_MODEL`), so it never divides by ~0; a
defensive guard returns `Err(DegenerateInput)` if `denom < TAU_MODEL²`.

## Implementation (`crates/ssi-rs/src/lib.rs`, `cylinder_cylinder` only)

Replace the current non-parallel early-return (the `if norm(cross(uhat, uhat2))
>= TAU_MODEL { return Err(ASNA) }` block) with a branch that, when the axes are
**non-parallel** (`cross_norm = norm(cross(uhat, uhat2)) >= TAU_MODEL`),
classifies on the **linear** geometric quantities:

- `equal_r  = (r1 − r2).abs() <= TAU_MODEL`;
- `line_gap = dot(rel, axis_cross).abs() / cross_norm` — the skew-line distance
  between the two axis lines (`rel = Q₂ − Q₁`, `axis_cross = cross(uhat, uhat2)`);
  this is the coplanarity test (`line_gap < TAU_MODEL` ⟺ axes coplanar, i.e.
  they intersect since they are non-parallel);
- if `equal_r && line_gap < TAU_MODEL` → build the two ellipses via a private
  helper and return `Ok`;
- else (unequal R, or skew) → `Err(AnalyticalSolutionNotAvailable)`
  (unchanged staged behaviour).

The **parallel branch** below (the circle∩circle → lines reduction from
PR-SSI10) is **untouched**. The E1 guards (radii finite/positive, axis
non-zero/finite via `normalize`, `axis_point` finiteness) already run first and
are reused unchanged.

Add two private helpers near `cylinder_cylinder` (no `unwrap`/`panic`/`unsafe`;
all paths return `Result`):

- `cyl_cyl_equal_radius_ellipses(q1, q2, uhat, uhat2, r) -> Result<Vec<SsiCurve>, SsiError>`
  - `b_plus  = normalize(add(uhat, uhat2))?`
  - `b_minus = normalize(sub(uhat, uhat2))?`
  - `o       = line_line_intersection(q1, uhat, q2, uhat2)?`
  - `beta    = dot(uhat, uhat2).clamp(-1.0, 1.0).acos()`
  - returns `vec![Ellipse_A, Ellipse_B]` (A first), with the radii and
    axes as in *The math* above.
- `line_line_intersection(p1, d1, p2, d2) -> Result<[f64; 3], SsiError>`
  - `b = dot(d1, d2)`, `w0 = sub(p1, p2)`, `dd = dot(d1, w0)`, `ee = dot(d2, w0)`,
    `denom = 1 − b²`;
  - guard `denom < TAU_MODEL * TAU_MODEL` → `Err(DegenerateInput)` (defensive;
    not reachable through `cylinder_cylinder` given the non-parallel guard);
  - `sc = (b·ee − dd) / denom`; returns `add(p1, scale(d1, sc))`.

Update the `cylinder_cylinder` doc-comment: describe the new equal-R
intersecting → two-ellipse branch, keep the §5.8 + A15.2 citations, and keep the
parallel→lines description. State that unequal-R and skew remain staged ASNA.

## Branch table (post-PR-SSI11)

| Configuration | Gate | Result |
|---|---|---|
| rᵢ ≤ 0 / non-finite, zero/non-finite axis, non-finite `axis_point` | E1 | `Err(DegenerateInput)` |
| Non-parallel, **equal R**, **coplanar** (`line_gap < TAU`) | `cross_norm ≥ TAU` ∧ `equal_r` ∧ coplanar | **two `Ellipse`s** (A first) |
| Non-parallel, unequal R (intersecting or skew) | `cross_norm ≥ TAU` ∧ ¬`equal_r` | `Err(ASNA)` |
| Non-parallel, equal R, **skew** | `cross_norm ≥ TAU` ∧ `equal_r` ∧ ¬coplanar | `Err(ASNA)` |
| Parallel, coincident axes, equal R | `cross_norm < TAU`, `d ≤ TAU`, equal r | `Err(DegenerateInput)` (2D overlap) |
| Parallel, coincident axes, unequal R | `cross_norm < TAU`, `d ≤ TAU`, unequal r | `Ok(vec![])` (concentric) |
| Parallel, disjoint / contained | `cross_norm < TAU`, `d > r₁+r₂` or `d < |r₁−r₂|` | `Ok(vec![])` |
| Parallel, tangent | `cross_norm < TAU`, `d = r₁+r₂` or `d = |r₁−r₂|` | one `Line` |
| Parallel, secant | `cross_norm < TAU`, otherwise | two `Line`s (+h first) |

## Tests

### RED — `crates/ssi-rs/tests/ssi11.rs`

Reuse the ssi10 helper set (inline `dot`/`cross`/`norm`/`unit`,
`implicit_residual` with the `Cylinder` arm). Add ellipse-aware helpers:

- **On-surface oracle (load-bearing — exercise densely):** sample each result
  `Ellipse` via `SsiCurve::eval` at many `t ∈ [0, 2π)` and assert, for **both**
  cylinders, the radial residual `|dist(x, axisᵢ line) − r| ≤ TAU_MODEL`.
- `ellipse_key` for SET comparison: canonicalize center, plane (normal up to
  sign), major axis (up to sign), and the two radii, so axis-flip / argument-swap
  invariance can be checked as an unordered set.

Cases (I3 branch coverage):

- **Canonical 90°** (β=90°, O=origin): cyl₁ `Q=0, û=+x, r=2`; cyl₂ `Q=0, û=+y, r=2`.
  Expect 2 ellipses. A: `normal=(1,−1,0)/√2, major=(1,1,0)/√2, major_r=2√2, minor_r=2`.
  B: `normal=(1,1,0)/√2, major=(1,−1,0)/√2, major_r=2√2, minor_r=2`. Spot checks:
  major end of A `(2,2,0)`, minor end `(0,0,2)`, both dist-2 to each axis.
- **Non-perpendicular 60°** (β=60°, nothing hardcoded to 90°): `major_r = 2/sin30° = 4`
  and `2/cos30° = 4/√3`. Verify via the on-surface oracle + radii formulas.
- **Equal-R but skew** (cyl₂ axis_point=(0,0,5), dir=+y, perpendicular but
  z-offset) → ASNA.
- **Unequal R intersecting** (cyl₂ dir=+y, r=3) → ASNA.
- **Parallel still → lines** (SSI10 path intact, e.g. the 3-4-5 two-line secant
  case) → 2 Lines.
- **E1** (r=0) → `DegenerateInput`.
- **Symmetry I4:** `intersect(c₁,c₂)` == `intersect(c₂,c₁)` as an ellipse SET.
- **Determinism I5:** identical input → byte-identical output; the b̂₋-normal
  ellipse (A) is `curves[0]`.
- **Contract:** assert `major_radius ≥ minor_radius` on every returned ellipse.

### ADVERSARY — `crates/ssi-rs/tests/ssi11_adversary.rs`

Attacks:

- **Parallelism / coplanarity `TAU_MODEL` band edges:** just-inside intersecting →
  ellipses, just-outside → ASNA.
- **Equal-R band edge:** `|r₁−r₂|` just over TAU → ASNA.
- **Near-π / near-0 angle conditioning** of `b̂₊` / `b̂₋` (must stay analytically
  correct; loud where the absolute-TAU oracle ceiling bites ~1e8; **no
  `TAU_MODEL` widening**).
- **Axis-flip / argument-swap** SET invariance.
- **Non-unit / off-origin oblique** intersecting axes.
- **Determinism sweep.**

## Verification / CI gate (all must pass for `ssi-rs`)

```
cargo test -p ssi-rs
cargo fmt -p ssi-rs --check
cargo clippy -p ssi-rs --all-targets -- -D warnings
```

Plus confirm the SSI1–SSI10 suites stay green (no contract migration — no enum
change). No `unsafe` / `panic!` / `unwrap` in production paths (P3). No
hack-to-green: gate on the **linear** quantities (`|r₁−r₂|`, skew-line
`line_gap`, `|û₁×û₂|`); no tolerance widening (P9/P10). If the implementer hits
a genuine conflict, **STOP and report** — do not improvise.

## Workflow (role-separated TDD, P5)

1. **Spec** (Manager) — this file; commit `docs(ssi-rs): PR-SSI11 spec`.
2. **RED** (sub-agent A) — `tests/ssi11.rs` (failing); commit `test(ssi-rs): PR-SSI11 RED`.
3. **GREEN** (sub-agent B ≠ A) — extend `cylinder_cylinder`; commit `feat(ssi-rs): PR-SSI11 GREEN`.
4. **Adversary** (sub-agent C ≠ A,B) — `tests/ssi11_adversary.rs` (+ a separate
   fix commit if a real bug is found); commit `test(ssi-rs): PR-SSI11 ADVERSARY`.
5. **Roadmap + push** — update `docs/yang_functional_roadmap.md` M5 Step 11;
   commit `docs(roadmap)`; push `origin/main`.

All commit messages end with:
`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
