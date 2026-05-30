# Spec: PR-SSI9 — ssi-rs cone∩cone coaxial (fourth & last circle-reducible degree-4 pair)

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-9
**Roles (P5):** Spec Writer = Manager; Test Author, Implementer, and Adversary are
**distinct** agents. Implementer never edits tests; test author never writes
production code; adversary adds tests only.

## Goal

The **fourth and last circle-reducible** degree-4 quadric∩quadric pair (after
PR-SSI6 sphere∩cylinder, PR-SSI7 sphere∩cone, PR-SSI8 cylinder∩cone). The general
cone∩cone intersection is a degree-4 space curve, but the **coaxial**
configuration (the two axis *lines* coincide) reduces to **one or two circles** —
exact, reusing the existing `SsiCurve::Circle`. PR-SSI9 implements the coaxial
case; the **general (non-coaxial) degree-4 curve** is deferred (returns
`Err(AnalyticalSolutionNotAvailable)`) to a later increment that introduces a new
degree-4 `SsiCurve` variant.

**Scope (special-cases-first, the SSI6/SSI7/SSI8 pattern):** coaxial → circles;
non-coaxial → loud `Err`. This continues the **coaxial-detect → reduce-to-circles
→ general-position-ASNA** pattern established by PR-SSI6/7/8, and is the **last**
circle-reducible coaxial pair. After this PR the only remaining ASNA degree-4
pair is cyl∩cyl (PR-SSI10/11: equal-R intersecting → two ellipses; parallel → two
lines).

**Precision (unchanged):** true analytical curves, f64. Clean-room from legacy.

## No new types

Reuses `SsiCurve::Circle`. **No enum change** ⇒ no exhaustive-match fixture
migration in the test files (ssi1–8 untouched).

## Why the branch structure differs from SSI8 (anti-hack note — P9/P10)

SSI8 (cyl∩cone) coaxial is **always exactly two circles** — no discriminant, no √,
no empty/tangent branch — because the cone's `[0,∞)` per-nappe radial range meets
the constant cylinder radius at exactly one height per nappe.

cone∩cone coaxial is **richer but still has no manufactured discriminant gate.**
With both half-angles unequal the squared equation `|t|·m₁ = |t−δ|·m₂` has a
discriminant that is a **perfect square** `(2·m₁·m₂·δ)²` — always ≥ 0, never
negative — so for `δ ≠ 0` it is **always exactly two circles** (both roots real;
both sides of `|t|m₁=|t−δ|m₂` are ≥ 0, so squaring is an exact equivalence with no
extraneous roots). There is **no √D sign gate, no manufactured tangent/empty
sub-branch.** The only empty/degenerate outcomes come from the geometrically real
`δ → 0` apex-collapse (X0 / CO), gated on the linear geometric quantity `|δ|`, and
from the equal-vs-unequal half-angle split, gated on the linear quantity `|α₁−α₂|`
(the SSI2/3/6/7/8 lesson: gate on a linear geometric quantity, never on a length²
or a square). Manufacturing a discriminant-sign branch would be a hack-to-pattern
(Constitution P9/P10) and is prohibited. `TAU_MODEL` only — no new epsilons.

## Solver: `cone_cone(a: &QuadricSurface, b: &QuadricSurface) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis & Maekawa §5.8.3 (Case F8, implicit/implicit
quadric pair) + the coaxial reduction + the "perfect-square discriminant, no
synthetic branch" rationale + the staged-NC/A15.2 note.

cone∩cone is a **same-type symmetric pair**: internal ordering is the solver's
responsibility. Choose **cone₁ = first arg**: define `â = normalize(cone₁.axis_dir)`,
`δ = (P₂ − P₁)·â`, `t = (x − P₁)·â` from it, and order the two X2 circles by
**descending `t`** (larger-t first; I5).

`intersect` dispatch: split the current combined
`Cylinder∩Cylinder | Cone∩Cone => Err(ASNA)` arm so that
`(Cone, Cone) => cone_cone(a, b)` and `(Cylinder, Cylinder) => Err(AnalyticalSolutionNotAvailable)`
remains.

### The math

Cone₁: apex `P₁`, unit axis `â = normalize(cone₁.axis_dir)`, half-angle `α₁`,
`m₁ = tanα₁`. Cone₂: apex `P₂`, axis ∥ `â`, half-angle `α₂`, `m₂ = tanα₂`. A
double cone is symmetric under `â → −â`, so only the half-angles and the apex
position **along** the shared axis matter — axis orientation sign is irrelevant.

Signed apex offset `δ = (P₂ − P₁)·â`. Parameterize by axial height
`t = (x − P₁)·â`. A point lies on both cones iff `|t|·m₁ = |t−δ|·m₂`. Both sides
are ≥ 0, so squaring is an **exact equivalence** (no extraneous roots):

```
(m₁² − m₂²)·t²  +  2·m₂²·δ·t  −  m₂²·δ²  =  0
```

discriminant `D = (2·m₁·m₂·δ)²` (a perfect square ⇒ always real). Each circle:
`center = P₁ + t·â`, `normal = â`, `radius = |t|·m₁` (equals `|t−δ|·m₂` by
construction — assert in the oracle).

- **Unequal α** (`m₁ ≠ m₂`): a genuine quadratic; for `δ ≠ 0` the two roots are
  `t = (−m₂²·δ ± m₁·m₂·|δ|) / (m₁² − m₂²)` — **always two circles**.
- **Equal α** (`m₁ = m₂`): the `t²` coefficient vanishes; the equation is linear,
  `2·m²·δ·t − m²·δ² = 0` ⇒ one circle at the bisector `t = δ/2`.

### Branch table

| # | case | condition | result |
|---|------|-----------|--------|
| E1 | degenerate input | `αᵢ` non-finite / `≤ TAU_MODEL` / `≥ π/2 − TAU_MODEL` (either cone); OR zero / non-finite axis (either cone) | `Err(DegenerateInput)` |
| NC | non-coaxial (general degree-4) | NOT (axes ∥: `\|â₂ × â₁\| < TAU_MODEL` **AND** apex₂ on axis₁: `d_ax = \|rel − (rel·â)·â\| < TAU_MODEL`, `rel = P₂ − P₁`) | `Err(AnalyticalSolutionNotAvailable)` — staged, never a fallback (A15.2) |
| X2 | two circles | coaxial, **unequal** α (`\|α₁−α₂\| > TAU_MODEL`), `\|δ\| > TAU_MODEL` | two `Circle`s at `t = (−m₂²δ ± m₁·m₂·\|δ\|)/(m₁²−m₂²)`; `center = P₁+t·â`, `normal = â`, `radius = \|t\|·m₁`; **larger-t first** (I5) |
| X1 | one circle | coaxial, **equal** α (`\|α₁−α₂\| ≤ TAU_MODEL`), `\|δ\| > TAU_MODEL` | one `Circle` at the bisector `t = δ/2` |
| X0 | empty | coaxial, **unequal** α, `\|δ\| ≤ TAU_MODEL` (apexes coincide → radius-0 point-circle at the shared apex) | `Ok(vec![])` |
| CO | coincident surfaces | coaxial, **equal** α, `\|δ\| ≤ TAU_MODEL` (identical double cone) | `Err(DegenerateInput)` |

Gate equal-vs-unequal on the linear quantity `|α₁−α₂|`; gate δ-collapse on `|δ|`.
No √D sign gate (the discriminant is a perfect square). `TAU_MODEL` only (A14.3);
no new epsilons.

### Verified concrete cases (for the canonical tests)

1. **X2 (unequal + offset):** cone₁ apex = origin, axis = +z, `α₁ = π/4`
   (`m₁ = 1`); cone₂ apex = `(0,0,2)` (`δ = 2`), axis = +z, `α₂ = atan(3)`
   (`m₂ = 3`). Roots `t = (−9·2 ± 1·3·2)/(1−9) = (−18±6)/(−8)` → `t = 3` and
   `t = 1.5` ⇒ circles at z=3 r=3 and z=1.5 r=1.5. **larger-t first ⇒ z=3 is
   `curves[0]`.** *(num. ✓)*
2. **X1 (equal + offset):** both `α = π/4`, cone₂ apex = `(0,0,2)` (`δ = 2`) → one
   circle at the bisector `t = δ/2 = 1` ⇒ z=1, radius `|1|·tan(π/4) = 1`. *(num. ✓)*
3. **CO (coincident):** same apex, same `α = π/4` → `Err(DegenerateInput)`.
4. **X0 (apex-coincident, unequal α):** same apex, `α₁ = π/4`, `α₂ = atan(3)` →
   `Ok(vec![])` (the only common point is the shared apex, a radius-0 circle).
5. **NC:** cone₂ apex = `(1,0,0)` off the z-axis → ASNA. Also non-parallel axes
   (`â₂ = +x`) → ASNA. Both argument orders.
6. **E1:** `α = 0` / `α ≥ π/2` (either cone); zero axis (either cone) →
   `Err(DegenerateInput)`.

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — core):** sample each result circle at `N = 64` params via
  `eval`; every sample satisfies **both** cones within `TAU_MODEL` — cone radial
  residual `| |(x − Pᵢ) − ((x − Pᵢ)·â)·â| − |hᵢ|·tanαᵢ |`, `hᵢ = (x − Pᵢ)·âᵢ`
  (reuse the cone residual from the ssi6/7/8 helpers, evaluated against **each**
  cone's own apex / axis / half-angle). Absolute-`TAU_MODEL` oracle valid to
  coordinate magnitude ~1e8 (PR-SSI1 finding).
- **I2 (analytical geometry):** centers on the shared axis (`d_ax ≈ 0`),
  `normal ∥ â` (unit); `radius = |t|·m₁ = |t−δ|·m₂` (assert the two equal); X2
  roots match the quadratic (canonical case 1); X1 circle at the `δ/2` bisector
  (case 2). Cases: canonical X2 (case 1), X1 (case 2), non-unit axis, oblique
  off-origin.
- **I3 (branch coverage + ANTI-HACK, P4):** X2 / X1 / X0(empty) / CO(`Err`) / NC /
  E1 each ≥ 1 test. **Assert the unequal-α coaxial case (δ ≠ 0) is ALWAYS exactly
  two circles** via a small α₁/α₂/δ sweep (`len() == 2` every time, mirroring
  SSI8's invariant — no manufactured discriminant/tangent/empty sub-branch). E1
  covers bad α (low + high, each cone) and zero axis (each cone).
- **I4 (symmetry):** `intersect(c₁, c₂) == intersect(c₂, c₁)` as a circle **SET**
  (`circle_key`, order / normal-sign tolerant) for X2 and X1.
- **I5 (determinism):** identical inputs → byte-identical output; the larger-t
  circle is `curves[0]` in X2.

## Failure modes

- Bad `α` (non-finite / `≤ TAU_MODEL` / `≥ π/2 − TAU_MODEL`, either cone), zero /
  non-finite axis (either cone) → `Err(DegenerateInput)`. No `panic!` / `unwrap`
  in production paths.
- **NC (non-coaxial) → `Err(AnalyticalSolutionNotAvailable)`** — a deliberate
  staged limitation (the general degree-4 curve + its new `SsiCurve` variant is a
  later increment). Loud, never a mesh/grid fallback (A15.2). The doc-comment must
  say so.
- **CO (coincident double cone)** → `Err(DegenerateInput)` (the overlap is a 2D
  surface, not a curve — mirrors `plane_plane` coincident and `sphere_sphere`
  concentric).
- The other unimplemented pair (cyl∩cyl) is unchanged →
  `Err(AnalyticalSolutionNotAvailable)`.

### Characterization notes (for the adversary — describe, do not "fix")

- **Coaxial-detection band** (both the `|â₂ × â₁| < TAU_MODEL` parallelism test and
  the `d_ax < TAU_MODEL` on-axis test) is an **absolute** gate, so the coaxial/NC
  split is scale-sensitive like every absolute-tolerance gate (cf. the PR-SSI1
  ~1e8 finding and the SSI6/7/8 ~1e8→1e9 coaxial-band ceiling). A truly-coaxial
  config at very large coordinate magnitude may read as NC → `Err(ASNA)` (a loud,
  never-wrong failure mode, not a spurious circle). Characterize, don't force
  green.
- **`|α₁−α₂|` equal/unequal boundary** and **`|δ|` collapse boundary** are linear
  `TAU_MODEL` gates; the adversary should probe each just inside / just outside the
  band and confirm the X2↔X1 and X2/X1↔X0/CO transitions are clean (no spurious
  third branch).
- **Reversed / antiparallel axis sign** (double-cone symmetry): flipping either
  cone's `axis_dir` must not change the circle SET (it flips the sign of `â` and of
  `δ` together, leaving `t·â` and the world-space circles invariant).
- **α near both E1 limits** (`m → 0` or `m → ∞`): inside the band the two circles
  stay finite; at the boundary the E1 gate fires. Characterize.
- **Apex-grazing radius-0 point-circle (X0):** the unequal-α apex-coincident case
  collapses both roots to `t = 0` (a radius-0 circle at the shared apex);
  `Ok(vec![])` is the chosen representation. Characterize the boundary `|δ| →
  TAU_MODEL` where it transitions to two near-apex circles.

## Research basis

- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8.3** (Case F8,
  implicit/implicit quadric intersection). The coaxial reduction (`|t|·tanα₁ =
  |t−δ|·tanα₂` from the two cones' `x²+y² = (t)²tan²α₁` ∧ `x²+y² = (t−δ)²tan²α₂`
  along the shared axis) is classical; cite §5.8.3 + the reduction.
- **Governance:** A15.1 (exact SSI), A15.2 (no fallback — non-coaxial `Err`), A15.4
  (pair cone-cone), P8 (cite research), A14.3 (`TAU_MODEL` only), P9/P10 (no
  hack-to-pattern — no manufactured discriminant-sign branch).

## Definition of Done (DoD §1)

Spec (this file); RED→GREEN→Adversary separate commits by distinct agents; every
branch (X2/X1/X0/CO/NC/E1) tested plus the explicit always-two-circles anti-hack
invariant; numeric/structural oracles (on-surface with each cone's radial residual
+ analytical geometry, not "no panic"); canonical X2 + X1 + non-unit/oblique-axis +
edge (non-coaxial→ASNA, non-parallel→ASNA, coincident→Err, degenerate) cases;
symmetry + determinism; no `unsafe` / `panic!` / `unwrap` in production paths; CI
gate (`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`) clean for `ssi-rs`; SSI1–8 suites untouched & green.
