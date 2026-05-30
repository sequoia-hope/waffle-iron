# Spec: PR-SSI7 — ssi-rs sphere∩cone coaxial (second degree-4 pair)

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-7
**Roles (P5):** Spec Writer = Manager; Test Author, Implementer, and Adversary are
**distinct** agents. Implementer never edits tests; test author never writes
production code; adversary adds tests only.

## Goal

Second of the **degree-4** quadric∩quadric pairs (after PR-SSI6 sphere∩cylinder).
The general sphere∩cone intersection is a degree-4 space curve, but the **coaxial**
configuration (the sphere center lies on the cone's axis line) reduces to **one or
two circles** — exact, reusing the existing `Circle` curve. PR-SSI7 implements the
coaxial case; the **general (non-coaxial) degree-4 curve** is deferred (returns
`Err(AnalyticalSolutionNotAvailable)`) to a later increment that introduces a new
degree-4 `SsiCurve` variant.

**Scope (special-cases-first, the SSI6 pattern):** coaxial → circles; non-coaxial →
loud `Err`. This continues the **coaxial-detect → reduce-to-circles → general-ASNA**
pattern established by PR-SSI6 (sphere∩cylinder) and reused by the remaining
circle-reducible pairs (cyl∩cone, cone∩cone).

**Precision (unchanged):** true analytical curves, f64. Clean-room from legacy.

## No new types

Reuses `SsiCurve::Circle`. **No enum change** ⇒ no exhaustive-match fixture
migration in the test files.

## Solver: `sphere_cone(sphere, cone) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis & Maekawa §5.8.3 (Case F8, implicit/implicit quadric
pair) + the coaxial reduction + the `g`-gate justification. `intersect` dispatch:
`(Sphere, Cone) => sphere_cone(a, b)`, `(Cone, Sphere) => sphere_cone(b, a)`; the
remaining degree-4 pairs (cyl∩cyl, cyl∩cone, cone∩cyl, cone∩cone) stay
`Err(AnalyticalSolutionNotAvailable)`.

### The math

Cone: apex `P`, unit axis `â = normalize(axis_dir)`, half-angle `α` (so `tanα`,
`secα`, `cosα`). Sphere: center `C`, radius `r_s`. **Coaxial** ::= `C` lies on the
cone axis line: `d_ax = |rel − (rel·â)·â| < TAU_MODEL`, `rel = C − P`.

Let `h0 = (C − P)·â` (signed axial height of the sphere center on the axis). A cone
point at axial height `h` has radial distance `|h|·tanα`; it lies on the sphere iff

```
(h − h0)² + h²·tan²α = r_s²
⇒  sec²α·h² − 2·h0·h + (h0² − r_s²) = 0
⇒  h = (h0 ± √D)·cos²α ,   D = sec²α·r_s² − h0²·tan²α
```

Each real root `h` → one `Circle { center = P + h·â, normal = â, radius = |h|·tanα }`
(`h < 0` is the other nappe of the double cone).

### Branch gate (the one design choice — gate on the LINEAR quantity)

Per the SSI2/3/6 lesson, gate on a geometrically-meaningful *linear* quantity, not
on a length² (`D`) and not on a square (`r_s²…`). Factor the discriminant using
`tan²α = sec²α·sin²α`:

```
D = sec²α·(r_s − |h0|·sinα)·(r_s + |h0|·sinα)
```

Since `sec²α > 0` and `r_s + |h0|·sinα > 0`, `sign(D) = sign(g)` where

```
g = r_s − |h0|·sinα
```

is the linear gap (sphere radius minus the on-axis tangent radius `|h0|·sinα`). Gate
on `g`.

### Branch table

| # | case | condition | result |
|---|---|---|---|
| E1 | degenerate input | `r_s ≤ 0` / non-finite; OR `α` non-finite / `α ≤ TAU_MODEL` / `α ≥ π/2 − TAU_MODEL`; OR zero/non-finite `axis_dir` | `Err(DegenerateInput)` |
| NC | non-coaxial (general degree-4) | `d_ax ≥ TAU_MODEL` | `Err(AnalyticalSolutionNotAvailable)` — **staged**, never a fallback (A15.2) |
| X0 | empty | coaxial and `g < −TAU_MODEL` (sphere too small to reach the cone) | `Ok(vec![])` |
| X1 | one tangent circle | coaxial and `\|g\| ≤ TAU_MODEL` | one `Circle` at `h_t = h0·cos²α`: `center = P + h_t·â`, `normal = â`, `radius = \|h_t\|·tanα` |
| X2 | two circles | coaxial and `g > TAU_MODEL` (⇒ `D > 0`, `√D` safe) | two `Circle`s at `h_± = (h0 ± √D)·cos²α`, **+√D first** (determinism): `center = P + h_±·â`, `normal = â`, `radius = \|h_±\|·tanα` |

Gating X2 on `g > TAU_MODEL` guarantees `D > 0` *strictly*, so `√D` never sees a
negative argument (exactly how SSI6's `r_s − r_c` gate protects `√(r_s²−r_c²)`).
The E1 α-bounds mirror `plane_cone` (`alpha ≤ TAU_MODEL || alpha ≥ FRAC_PI_2 −
TAU_MODEL`). No new epsilons — `TAU_MODEL` only (A14.3).

### Verified concrete cases (for the canonical tests)
- **Canonical X2:** apex = origin, axis = +z, `α = π/4` (`tanα = 1`, `sec²α = 2`);
  sphere center = origin (`h0 = 0`), `r_s = 2` ⇒ `D = 8 > 0` ⇒ two circles at
  `z = ±√2`, radius `√2`, normal +z. Check point `(√2, 0, √2)`: on cone (radial
  `√2 = |z|·tanα`), on sphere (`|·| = 2`).
- **Asymmetric X2 (non-symmetric roots):** `h0 = 3`, `r_s = 4` (`α = π/4`) ⇒
  `D = 23 > 0`, roots `h = (3 ± √23)/2 ≈ 3.898, −0.898` (one circle per nappe).
- **X0 empty:** `h0 = 3`, `r_s = 2` (`α = π/4`) ⇒ `g = 2 − 3·(√2/2) < 0` ⇒
  `D = −1 < 0` ⇒ `Ok([])`.
- **X1 tangent:** `r_s = |h0|·sinα` exactly (e.g. `h0 = 2`, `α = π/4` ⇒ `r_s = √2`);
  `h_t = h0·cos²α = 1`, `radius = |h_t|·tanα = 1`.

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — core):** sample each result circle at N params via `eval`;
  every sample satisfies **both** surfaces within `TAU_MODEL` — sphere
  `| |x − C| − r_s | < TAU_MODEL`; cone radial residual
  `| |(x − P) − ((x − P)·â)·â| − |h|·tanα | < TAU_MODEL`, `h = (x − P)·â` (reuse the
  cone residual already in the ssi6 test helpers). Absolute-`TAU_MODEL` oracle valid
  to coordinate magnitude ~1e8 (PR-SSI1 finding).
- **I2 (analytical geometry):** every result `Circle` has `radius == |h|·tanα`,
  `normal ∥ â`, `center == P + h·â` (on the axis line); the roots `h` satisfy the
  quadratic `sec²α·h² − 2h0·h + (h0² − r_s²) = 0`. X2 → exactly two circles
  (one per root, `+√D` first); X1 → one circle at `h_t = h0·cos²α`; X0 → `Ok(vec![])`.
- **I3 (branch coverage, P4):** X2, X1, X0, NC, E1 each ≥1 test (E1 covers `r_s ≤ 0`,
  bad `α` low and high, and zero axis).
- **I4 (symmetry):** `intersect(sphere, cone) == intersect(cone, sphere)` (same
  circle set; order / normal-sign tolerant).
- **I5 (determinism):** identical inputs → byte-identical output (two-circle order
  `+√D` first).

## Failure modes
- `r_s ≤ 0` / non-finite, bad `α`, zero/non-finite `axis_dir` → `Err(DegenerateInput)`.
  No `panic!` / `unwrap` in production paths.
- **NC (non-coaxial) → `Err(AnalyticalSolutionNotAvailable)`** — a deliberate staged
  limitation (the general degree-4 curve + its new `SsiCurve` variant is a later
  increment). Loud, never a mesh/grid fallback (A15.2). The doc-comment must say so.
- Other unimplemented pairs (cyl∩cyl, cyl∩cone, cone∩cyl, cone∩cone) unchanged →
  `Err(AnalyticalSolutionNotAvailable)`.

### Characterization notes (for the adversary — describe, do not "fix")
- **Coaxial-detection band** `d_ax < TAU_MODEL` is an **absolute** distance, so the
  coaxial/NC split is scale-sensitive like every absolute-tolerance gate (cf. the
  PR-SSI1 ~1e8 finding and the SSI6 ~1e8→1e9 coaxial-band ceiling). A truly-coaxial
  config at very large coordinate magnitude may read as NC → `Err(ASNA)` (a loud,
  never-wrong failure mode, not a spurious circle). Characterize, don't force green.
- **Apex-grazing sub-case** `r_s = |h0|` (sphere passes through the apex) makes the
  constant term `h0² − r_s² = 0`, so one root is `h = 0` ⇒ a `Circle` of `radius =
  0` at the apex (a degenerate point), the other a proper circle on one nappe. This
  sits at the boundary of the staged scope; the X2 formula emits it verbatim. The
  adversary may **characterize** this honestly (it is a documented at-boundary
  degeneracy of the reduction, not a `√(negative)`/NaN bug). If the adversary judges
  the radius-0 emission a *real* correctness defect, it STOPS and reports rather than
  patching production.

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8.3** (Case F8,
  implicit/implicit quadric intersection). The coaxial reduction
  (`(h−h0)² + h²tan²α = r_s²` from `x²+y² = h²tan²α` ∧ sphere) is classical; cite
  §5.8.3 + the reduction.
- **Governance:** A15.1 (exact SSI), A15.2 (no fallback — non-coaxial `Err`), A15.4
  (pair sphere-cone), P8 (cite research), A14.3 (`TAU_MODEL` only).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN→Adversary separate commits by distinct agents; every
branch (X2/X1/X0/NC/E1) tested; numeric/structural oracles (on-surface with sphere +
cone radial residuals + analytical geometry, not "no panic"); canonical (two-circle)
+ asymmetric-root + non-unit/oblique-axis + edge (tangent X1, empty X0,
non-coaxial→ASNA, degenerate) cases; symmetry + determinism; no `unsafe` / `panic!` /
`unwrap` in production paths; CI gate (`cargo test`, `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`) clean for `ssi-rs`; SSI1–6 suites
untouched & green.
