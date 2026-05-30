# Spec: PR-SSI8 — ssi-rs cylinder∩cone coaxial (third degree-4 pair)

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-8
**Roles (P5):** Spec Writer = Manager; Test Author, Implementer, and Adversary are
**distinct** agents. Implementer never edits tests; test author never writes
production code; adversary adds tests only.

## Goal

Third of the **degree-4** quadric∩quadric pairs (after PR-SSI6 sphere∩cylinder and
PR-SSI7 sphere∩cone). The general cylinder∩cone intersection is a degree-4 space
curve, but the **coaxial** configuration (the two axis *lines* coincide) reduces to
**exactly two circles** — exact, reusing the existing `SsiCurve::Circle`. PR-SSI8
implements the coaxial case; the **general (non-coaxial) degree-4 curve** is deferred
(returns `Err(AnalyticalSolutionNotAvailable)`) to a later increment that introduces a
new degree-4 `SsiCurve` variant.

**Scope (special-cases-first, the SSI6/SSI7 pattern):** coaxial → circles;
non-coaxial → loud `Err`. This continues the **coaxial-detect → reduce-to-circles →
general-position-ASNA** pattern established by PR-SSI6 (sphere∩cylinder) and PR-SSI7
(sphere∩cone), and reused by the last circle-reducible pair (cone∩cone).

**Precision (unchanged):** true analytical curves, f64. Clean-room from legacy.

## No new types

Reuses `SsiCurve::Circle`. **No enum change** ⇒ no exhaustive-match fixture
migration in the test files (ssi1–7 untouched).

## Why the branch structure differs from SSI6/SSI7 (anti-hack note — P9/P10)

SSI6 and SSI7 each had a discriminant with X2/X1/X0 sub-cases (two circles / one
tangent circle / empty) because a **sphere's finite radius** can miss, graze, or cut
the other surface, and a `√D` had to be gated to stay real.

**Cylinder∩cone coaxial has none of that.** The cone's per-nappe radial range is
`[0, ∞)`, so the constant cylinder radius `r_c` is met at exactly **one** axial
height per nappe (`|h| = r_c·cotα`). Coaxial cyl∩cone is therefore **always two
distinct circles** for valid inputs. There is **no discriminant, no √, no
tangent/empty branch.** Manufacturing one to mirror SSI7 would be a hack-to-pattern
(Constitution P9/P10) and is explicitly prohibited. The only outcomes are:

- **X2** — two circles (coaxial, always, for valid input)
- **NC** — non-coaxial → `Err(AnalyticalSolutionNotAvailable)` (staged, never a fallback)
- **E1** — degenerate input → `Err(DegenerateInput)`

## Solver: `cylinder_cone(cylinder, cone) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis & Maekawa §5.8.3 (Case F8, implicit/implicit quadric
pair) + the coaxial reduction + the "always two circles, no discriminant" rationale +
the staged-NC/A15.2 note. `intersect` dispatch (split the current 4-pair ASNA arm):
`(Cylinder, Cone) => cylinder_cone(a, b)`, `(Cone, Cylinder) => cylinder_cone(b, a)`
(swap so the cylinder is first, mirroring the sphere-first swaps). The remaining
degree-4 pairs (cyl∩cyl, cone∩cone) stay `Err(AnalyticalSolutionNotAvailable)`.

### The math

Cone: apex `P`, unit axis `â = normalize(cone.axis_dir)`, half-angle `α`. A cone
point at axial height `h = (x−P)·â` has radial distance `|h|·tanα`.
Cylinder: `axis_point A`, unit axis `ĉ = normalize(cyl.axis_dir)`, radius `r_c`;
implicit `dist(x, cyl axis line) = r_c`.

**Coaxial** ::= the two axis *lines* coincide:
- axes parallel: `|ĉ × â| < TAU_MODEL` (sine of the inter-axis angle), **AND**
- the cylinder `axis_point` lies on the cone axis line:
  `d_ax = |rel − (rel·â)·â| < TAU_MODEL`, `rel = A − P` (the SSI7 `d_ax` test).

When coaxial, a cone point at height `h` has radial distance `|h|·tanα` from the
shared axis; it lies on the cylinder iff `|h|·tanα = r_c`, i.e.
`|h| = r_c·cotα = r_c / tanα`. The two roots `h = ± r_c·cotα` give **exactly two
circles** `{ center = P + h·â, normal = â, radius = r_c }` (`h < 0` is the other
nappe of the double cone).

- `cotα = 1 / tanα`; with α ∈ (TAU_MODEL, π/2 − TAU_MODEL), `tanα` is bounded away
  from 0 and ∞, so `r_c / tanα` is safe (no division guard beyond the α E1 check).
- `radius = r_c` (the cylinder radius — exact-on-cylinder; matches `sphere_cylinder`
  which also emits `r_c`). Equivalently `|h|·tanα`, equal up to machine-ε.

### Branch table

| # | case | condition | result |
|---|---|---|---|
| E1 | degenerate input | `r_c ≤ 0` / non-finite; OR `α` non-finite / `α ≤ TAU_MODEL` / `α ≥ π/2 − TAU_MODEL`; OR zero / non-finite cone or cylinder `axis_dir` | `Err(DegenerateInput)` |
| NC | non-coaxial (general degree-4) | NOT (`\|ĉ × â\| < TAU_MODEL` AND `d_ax < TAU_MODEL`) | `Err(AnalyticalSolutionNotAvailable)` — staged, never a fallback (A15.2) |
| X2 | two circles | coaxial (always, for valid input) | two `Circle`s at `h = ± r_c·cotα`, `center = P ± h·â`, `normal = â`, `radius = r_c`; **h>0 nappe first** (determinism, I5) |

No `√`, no discriminant, no one-circle/empty branch. `TAU_MODEL` only (A14.3); no new
epsilons.

### Verified concrete cases (for the canonical tests)

1. **Canonical X2:** cone apex = origin, axis = +z, `α = π/4` (`tanα = 1`,
   `cotα = 1`); cylinder axis_point = origin, axis_dir = +z, `r_c = 2` ⇒ two circles
   at `z = ±2`, radius 2, normal +z. Check point `(2,0,2)`: on cone (radial
   `2 = |z|·tanα`), on cylinder (dist to z-axis `= 2 = r_c`). *(num. ✓)*
2. **cotα ≠ 1:** `α = atan(2)` (`tanα = 2`, `cotα = 0.5`), `r_c = 3` ⇒ circles at
   `z = ±1.5`, radius 3. *(num. ✓)*
3. **NC:** same cone, cylinder axis_point = `(1,0,0)` off the cone axis → ASNA. Also
   a non-parallel cylinder axis → ASNA.
4. **E1:** `r_c = 0` (or negative) → DegenerateInput.

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — core):** sample each result circle at N params via `eval`; every
  sample satisfies **both** surfaces within `TAU_MODEL` — cylinder radial residual
  `| dist(x, cylAxisLine) − r_c |`; cone radial residual
  `| |(x − P) − ((x − P)·â)·â| − |h|·tanα | `, `h = (x − P)·â` (reuse the cone
  residual from the ssi6/ssi7 helpers). Absolute-`TAU_MODEL` oracle valid to
  coordinate magnitude ~1e8 (PR-SSI1 finding).
- **I2 (analytical geometry):** `radius == r_c` exactly; the two centers at
  `P ± r_c·cotα·â` on the axis; `normal ∥ â` (unit); the two `h` equal-and-opposite;
  exactly two circles. Cases: canonical (case 1), cotα≠1 (case 2), non-unit axis,
  oblique off-origin axis.
- **I3 (branch coverage, P4):** X2, NC, E1 each ≥1 test. **Assert explicitly there is
  NO coaxial config yielding one or zero circles** (the anti-hack invariant — sweep
  several α / r_c and assert `len == 2` always). E1 covers `r_c ≤ 0`, bad `α` (low and
  high), zero cone axis, zero cylinder axis. NC covers off-axis axis_point AND
  non-parallel cylinder axis.
- **I4 (symmetry):** `intersect(cyl, cone) == intersect(cone, cyl)` (same circle set;
  order / normal-sign tolerant via `circle_key`).
- **I5 (determinism):** identical inputs → byte-identical output; **h>0 nappe first**
  (`curves[0].center` on the +â side).

## Failure modes

- `r_c ≤ 0` / non-finite, bad `α`, zero / non-finite cone or cylinder `axis_dir` →
  `Err(DegenerateInput)`. No `panic!` / `unwrap` in production paths.
- **NC (non-coaxial) → `Err(AnalyticalSolutionNotAvailable)`** — a deliberate staged
  limitation (the general degree-4 curve + its new `SsiCurve` variant is a later
  increment). Loud, never a mesh/grid fallback (A15.2). The doc-comment must say so.
- Other unimplemented pairs (cyl∩cyl, cone∩cone) unchanged →
  `Err(AnalyticalSolutionNotAvailable)`.

### Characterization notes (for the adversary — describe, do not "fix")

- **Coaxial-detection band** (both the `|ĉ × â| < TAU_MODEL` parallelism test and the
  `d_ax < TAU_MODEL` on-axis test) is an **absolute** gate, so the coaxial/NC split is
  scale-sensitive like every absolute-tolerance gate (cf. the PR-SSI1 ~1e8 finding and
  the SSI6/SSI7 ~1e8→1e9 coaxial-band ceiling). A truly-coaxial config at very large
  coordinate magnitude may read as NC → `Err(ASNA)` (a loud, never-wrong failure mode,
  not a spurious circle). Characterize, don't force green.
- **α near the E1 limits** (`tanα → 0` or `→ ∞`) drives `cotα = r_c/tanα` to large /
  small magnitude; the two circles separate to large `|z|` or collapse toward the
  apex. The E1 α-bounds keep `tanα` bounded away from 0 and ∞; the adversary may
  characterize the at-boundary behavior (always two finite circles inside the band).

## Research basis

- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8.3** (Case F8,
  implicit/implicit quadric intersection). The coaxial reduction (`|h|·tanα = r_c`
  from the cone's `x²+y² = h²tan²α` ∧ the cylinder's `x²+y² = r_c²`) is classical;
  cite §5.8.3 + the reduction.
- **Governance:** A15.1 (exact SSI), A15.2 (no fallback — non-coaxial `Err`), A15.4
  (pair cylinder-cone), P8 (cite research), A14.3 (`TAU_MODEL` only), P9/P10 (no
  hack-to-pattern — no manufactured discriminant/tangent/empty branch).

## Definition of Done (DoD §1)

Spec (this file); RED→GREEN→Adversary separate commits by distinct agents; every
branch (X2/NC/E1) tested plus the explicit no-one/zero-circle anti-hack invariant;
numeric/structural oracles (on-surface with cylinder + cone radial residuals +
analytical geometry, not "no panic"); canonical (two-circle) + cotα≠1 +
non-unit/oblique-axis + edge (non-coaxial→ASNA, non-parallel→ASNA, degenerate) cases;
symmetry + determinism; no `unsafe` / `panic!` / `unwrap` in production paths; CI gate
(`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`) clean
for `ssi-rs`; SSI1–7 suites untouched & green.
