# Spec: PR-SSI10 — ssi-rs cylinder∩cylinder PARALLEL axes → lines

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-10
**Roles (P5):** Spec Writer = Manager; Test Author, Implementer, and Adversary are
**distinct** agents. Implementer never edits tests; test author never writes
production code; adversary adds tests only.

## Goal

cylinder∩cylinder is the **only remaining degree-4 pair** — the last dispatch arm
still returning `Err(AnalyticalSolutionNotAvailable)` for an *unhandled pair*. Its
special cases split in two: **parallel axes → lines** (THIS PR, PR-SSI10) and
**equal-radius intersecting axes → ellipses** (PR-SSI11, next). PR-SSI10 creates
the `cylinder_cylinder` solver and implements ONLY the parallel-axis branch. The
non-parallel sub-case (the general degree-4 curve, incl. equal-R intersecting →
ellipses) stays `Err(AnalyticalSolutionNotAvailable)` — staged, loud, never a
fallback (A15.2).

After this PR, **no dispatch arm returns ASNA for an unhandled *pair*** —
`cylinder_cylinder` itself returns ASNA only for its non-parallel sub-case.

**Scope (special-cases-first, the SSI6/7/8/9 pattern):** parallel axes →
lines; non-parallel → loud `Err`. Unlike SSI6–9, this produces
**`SsiCurve::Line`** (not `Circle`), reusing the line-construction conventions
already proven in `plane_cylinder` C3a/C3b (two parallel secant lines / one
tangent line).

**Precision (unchanged):** true analytical curves, f64. Clean-room from legacy.

## No new types

Reuses `SsiCurve::Line`. **No enum change** ⇒ no exhaustive-match fixture
migration in the test files (ssi1–9 untouched).

## Solver: `cylinder_cylinder(c1: &QuadricSurface, c2: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis & Maekawa §5.8 (Surface/Surface Intersections —
natural quadrics) + the parallel cyl∩cyl reduction to circle∩circle lifted along
the shared axis + the staged-NP/A15.2 note + the line-construction rationale,
mirroring `cylinder_cone`.

cyl∩cyl is a **same-type symmetric pair**: internal ordering is the solver's
responsibility. Choose **cyl₁ = first arg**: `û = û₁ = normalize(cyl₁.axis_dir)`,
reference point `c₁ = Q₁` (`Q₁ = cyl₁.axis_point`).

`intersect` dispatch: change the lone unhandled arm
`(Cylinder, Cylinder) => Err(AnalyticalSolutionNotAvailable)` to
`(Cylinder, Cylinder) => cylinder_cylinder(a, b)` (single arm, same-type
symmetric pair).

### The math

Both cylinders share direction `û = û₁ = normalize(cyl₁.axis_dir)` (parallel
case). Inter-axis perpendicular distance (the linear gate quantity):
`rel = Q₂ − Q₁`, `d = | rel − (rel·û)·û |`. This is **circle∩circle** (centre
distance `d`, radii `r₁, r₂`) lifted along `û` → lines parallel to `û`.

Cross-section construction (plane ⟂ û through Q₁, so `c₁ = Q₁` is the reference
point — no projection needed since Q₁ is already in that plane):

- `n̂ = unit(rel − (rel·û)·û)` (unit perp component of `rel`; only defined when `d>0`)
- `a = (d² + r₁² − r₂²) / (2d)` — offset of the chord along `n̂`
- `h = √(max(0, r₁² − a²))` — half-chord (clamp defensive; the branch table
  guarantees `r₁² ≥ a²` in the 2-line/tangent cases, the clamp only absorbs ε)
- `p̂ = û × n̂` (unit, since `û ⟂ n̂`, both unit)
- two cross-section points `Q₁ + a·n̂ ± h·p̂`, lifted to `Line { point, dir = û }`

Verification: for `x = Q₁ + a·n̂ ± h·p̂`, perp-dist to axis 1
= `√(a²+h²) = r₁` ✓; perp-dist to axis 2 = `√((a−d)²+h²) = r₂` ✓ (algebra
confirmed with `2ad = d²+r₁²−r₂²`, `h²=r₁²−a²`).

### Branch table (gate on the LINEAR quantity `d` — the SSI2/3/6/7/8/9 lesson)

| # | case | condition | result |
|---|------|-----------|--------|
| E1 | degenerate | `rᵢ ≤ 0` / non-finite (either cyl); OR zero/non-finite axis (either cyl) | `Err(DegenerateInput)` |
| NP | non-parallel (general degree-4, incl. equal-R intersecting → ellipses) | `\|û₁ × û₂\| ≥ TAU_MODEL` | `Err(AnalyticalSolutionNotAvailable)` — staged, never a fallback (A15.2) |
| COIN | coincident cylinders | parallel, `d ≤ TAU_MODEL` (axes coincide) AND `\|r₁−r₂\| ≤ TAU_MODEL` | `Err(DegenerateInput)` (overlap is a 2D surface) |
| CONC | concentric (axes coincide, unequal r) | parallel, `d ≤ TAU_MODEL` AND `\|r₁−r₂\| > TAU_MODEL` | `Ok(vec![])` (empty) |
| EMPTY | disjoint / contained | parallel, `d > 0`, `d > r₁+r₂ + TAU_MODEL` OR `d < \|r₁−r₂\| − TAU_MODEL` | `Ok(vec![])` |
| TAN | tangent (1 line) | parallel, `d > 0`, `\|d − (r₁+r₂)\| ≤ TAU_MODEL` OR `\|d − \|r₁−r₂\|\| ≤ TAU_MODEL` | one `Line` at `Q₁ + a·n̂`, `dir = û` |
| SEC | secant (2 lines) | parallel, `d > 0`, otherwise | two `Line`s at `Q₁ + a·n̂ ± h·p̂`; **`+h·p̂` first** (determinism, I5) |

Check order in the solver: E1 (radii) → normalize axes (E1 zero axis) → NP
(parallelism) → compute `d` → `d ≤ TAU_MODEL` split (COIN vs CONC) → EMPTY →
TAN → SEC. `TAU_MODEL` only (A14.3); no new epsilons.

### Verified concrete cases (for the canonical tests)

1. **Two lines (3-4-5):** cyl₁ `Q=origin û=+z r₁=5`; cyl₂ `Q=(8,0,0) û=+z r₂=5`;
   `d=8`. `a = (64+25−25)/16 = 4`, `h = √(25−16) = 3` → points `(4,±3,*)`, two
   lines dir `+z`. Check `(4,3,0)`: dist to z-axis = 5 = r₁; dist to `(8,0,*)`
   axis = `√(16+9)` = 5 = r₂. (`n̂ = +x`, `p̂ = û×n̂ = (0,0,1)×(1,0,0) = (0,1,0) =
   +y`.) *(num. ✓)*
2. **Tangent:** cyl₁ `r=2 @origin +z`; cyl₂ `r=2 @(4,0,0) +z`; `d=4 = r₁+r₂` → 1
   line at `(2,0,*)`. *(num. ✓)*
3. **Empty (disjoint):** cyl₁ `r=1 @origin`; cyl₂ `r=1 @(5,0,0)`; `d=5 > 2` → 0
   lines.
4. **Coincident:** identical cylinders → `Err(DegenerateInput)`. **Concentric**
   (same axis, `r₁=1, r₂=2`) → `Ok(vec![])`.
5. **Non-parallel:** cyl₂ `û=+x` → ASNA. **E1:** `r=0` → `Err(DegenerateInput)`.

Additional structural cases (mirroring SSI6–9): non-unit axis (defensive
normalize), oblique off-origin shared-direction axes, internal tangent
(`d = |r₁−r₂|`, unequal radii), antiparallel `û₂ = −û₁` (set-invariant).

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — core):** sample each result `Line` at several `t` via
  `eval`; every sample satisfies **both** cylinders within `TAU_MODEL` — radial
  residual `| dist(x, axisᵢ line) − rᵢ |`. Reuse the `Cylinder` arm of the
  `implicit_residual` helper carried verbatim from ssi8/ssi9 tests.
- **I2 (analytical geometry):** each line `dir ∥ û` (unit); the cross-section
  points satisfy the circle∩circle equations; symmetric `±h·p̂` placement about
  the centre line `Q₁ + a·n̂`; `a` matches `(d²+r₁²−r₂²)/(2d)`.
- **I3 (branch coverage, P4):** SEC(2) / TAN(1) / EMPTY(0) / COIN(`Err`) /
  CONC(empty) / NP(ASNA) / E1(`Err`) each ≥ 1 test. E1 covers `rᵢ ≤ 0` (each
  cyl) and zero axis (each cyl).
- **I4 (symmetry):** `intersect(c₁,c₂) == intersect(c₂,c₁)` as a line **SET**
  (order / point-on-line / dir-sign tolerant — a `line_key` canonicalizing dir up
  to sign and the point to its foot ⟂ dir, mirroring SSI9's `circle_key`).
- **I5 (determinism):** identical inputs → byte-identical output; the `+h·p̂`
  line is `curves[0]` in SEC.

## Failure modes

- Bad `r` (`≤ 0` / non-finite, either cyl), zero / non-finite axis (either cyl)
  → `Err(DegenerateInput)`. No `panic!` / `unwrap` in production paths.
- **NP (non-parallel) → `Err(AnalyticalSolutionNotAvailable)`** — a deliberate
  staged limitation (the general degree-4 curve, incl. equal-R intersecting →
  ellipses, + its new `SsiCurve` variant is a later increment, PR-SSI11). Loud,
  never a mesh/grid fallback (A15.2). The doc-comment must say so.
- **COIN (coincident cylinders)** → `Err(DegenerateInput)` (the overlap is a 2D
  surface, not a curve — mirrors `plane_plane` coincident and `sphere_sphere`
  concentric).

### Characterization notes (for the adversary — describe, do not "fix")

- **Parallelism band** (`|û₁ × û₂| < TAU_MODEL`) is an **absolute** sine gate, so
  the parallel/NP split is scale-insensitive in the *angle* but the solver SNAPS
  the result direction to `û₁`, so a barely-in-band tilt of `û₂` leaves the lines
  exactly on cyl₁ and on cyl₂ only to the in-band slack. Characterize, don't force
  the two-surface oracle inside the band.
- **`d` collapse band** (`d ≤ TAU_MODEL`) is the COIN/CONC split, gated on the
  linear distance; just-inside → COIN (`Err`) or CONC (empty by `|r₁−r₂|`),
  just-outside → the 2-line / 1-line / 0-line conic. Probe both sides.
- **Tangent boundaries** (`d = r₁+r₂` external, `d = |r₁−r₂|` internal) are linear
  `TAU_MODEL` gates; probe just-inside (1 line) / just-outside (2 lines or empty).
  Internal tangency requires unequal radii.
- **Antiparallel axis sign** (`û₂ = −û₁`): the cylinder is symmetric under
  `û → −û`, so flipping either axis_dir must not change the line SET.
- **Absolute-`TAU` coordinate-scale ceiling:** the on-surface oracle is absolute,
  valid to coordinate magnitude ~1e8 (PR-SSI1 finding); characterize where it
  breaks (~1e9) and confirm the solver stays analytically correct (tiny relative
  residual), a loud ceiling not a logic bug. Do NOT widen `TAU_MODEL`.
- **Non-unit / oblique shared axis:** defensive `normalize` ⇒ unit `dir`; lines
  still lie on both cylinders.

## Research basis

- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8**
  (Surface/Surface Intersections — natural quadrics). The parallel cyl∩cyl
  reduction to **circle∩circle** (centres distance `d`, radii `r₁,r₂`) lifted
  along the shared axis `û` → lines parallel to `û` is classical; cite §5.8 + the
  reduction. The cross-section chord construction (`a = (d²+r₁²−r₂²)/(2d)`,
  `h = √(r₁²−a²)`) is the standard two-circle radical-line geometry, identical in
  spirit to `sphere_sphere`'s circle construction.
- **Governance:** A15.1 (exact SSI), A15.2 (no fallback — non-parallel `Err`),
  A15.4 (pair cyl-cyl), P8 (cite research), A14.3 (`TAU_MODEL` only), P9/P10 (no
  hack-to-pattern — gate on the linear `d`, no manufactured branch).

## Definition of Done (DoD §1)

Spec (this file); RED→GREEN→Adversary separate commits by distinct agents; every
branch (SEC/TAN/EMPTY/COIN/CONC/NP/E1) tested; numeric/structural oracles
(on-surface with each cylinder's radial residual + analytical geometry, not "no
panic"); canonical two-line + tangent + non-unit/oblique-axis + edge
(non-parallel→ASNA, coincident→Err, concentric→empty, degenerate) cases;
symmetry + determinism; no `unsafe` / `panic!` / `unwrap` in production paths; CI
gate (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`) clean for `ssi-rs`; SSI1–9 suites untouched & green.
