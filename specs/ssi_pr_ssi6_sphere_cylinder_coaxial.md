# Spec: PR-SSI6 — ssi-rs sphere∩cylinder coaxial (first degree-4 pair)

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-6
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

First of the **degree-4** quadric∩quadric pairs. The general sphere∩cylinder
intersection is a degree-4 space curve, but the **coaxial** configuration (cylinder
axis passes through the sphere center) reduces to **circles** — exact, reusing the
existing `Circle` curve. PR-SSI6 implements the coaxial case; the **general
(non-coaxial) degree-4 curve** is deferred (returns `Err(AnalyticalSolutionNotAvailable)`)
to a later increment that introduces a new degree-4 `SsiCurve` variant.

**Scope (special-cases-first, the same pattern as the cone bounded/unbounded split):**
coaxial → circles; non-coaxial → loud `Err` (staged, never a fallback; no consumer
is wired to `ssi-rs` yet). This establishes the **coaxial-detect → reduce-to-circles
→ general-ASNA** pattern that the other circle-reducible pairs (sphere∩cone,
cyl∩cone, cone∩cone) will reuse.

**Verified (concrete case):** sphere C = origin, r_s = 2; cylinder z-axis, r_c = 1
(coaxial) → two circles at `(0,0,±√3)`, radius 1, normal +z. Point `(1,0,√3)`:
`|x−C| = 2 = r_s`, dist-to-axis `= 1 = r_c`. ✓

**Precision (unchanged):** true analytical curves, f64. Clean-room from legacy.

## No new types

Reuses `SsiCurve::Circle`. **No enum change** ⇒ no exhaustive-match fixture
migration in the test files (unlike SSI2/3/4).

## Solver: `sphere_cylinder(sphere, cylinder) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis §5.8.3 (Case F8, implicit/implicit quadric pair;
Example 5.8.4 sphere∩cylinder) + the coaxial reduction. `intersect` dispatch:
`(Sphere, Cylinder) => sphere_cylinder(a, b)`,
`(Cylinder, Sphere) => sphere_cylinder(b, a)`; all other degree-4 pairs stay
`Err(AnalyticalSolutionNotAvailable)`.

Let `C` = sphere center, `r_s` = sphere radius; `A` = cylinder `axis_point`,
`â = normalize(cylinder.axis_dir)`, `r_c` = cylinder radius. The **coaxial
discriminant** is the distance from the sphere center to the axis line:
`rel = C − A`, `d_ax = | rel − (rel·â)·â |`.

### Branch table

| # | case | condition | result |
|---|---|---|---|
| E1 | degenerate input | `r_s ≤ 0` or `r_c ≤ 0` or non-finite; or zero/non-finite `axis_dir` | `Err(DegenerateInput)` |
| NC | non-coaxial (general degree-4) | `d_ax ≥ TAU_MODEL` | `Err(AnalyticalSolutionNotAvailable)` — staged; general degree-4 deferred |
| X2 | two circles | coaxial (`d_ax < TAU_MODEL`) and `r_s − r_c > TAU_MODEL` | **two Circle** { center = C ± h·â, normal = â, radius = r_c }, `h = √(r_s² − r_c²)` — **`+h` first** (determinism) |
| X1 | one circle (tangent) | coaxial and `\|r_s − r_c\| ≤ TAU_MODEL` | one **Circle** { center = C, normal = â, radius = r_c } (great-circle tangent; `h ≈ 0`) |
| X0 | empty | coaxial and `r_c − r_s > TAU_MODEL` | `Ok(vec![])` (cylinder radius exceeds sphere — no contact) |

*Coaxial-detection scale note (PR-SSI6 adversary):* `d_ax` is an **absolute**
distance compared to `TAU_MODEL`, so the coaxial/NC split is scale-sensitive like
every absolute-tolerance gate (cf. the PR-SSI1 ~1e8 finding): a truly-coaxial
generic-direction axis is detected correctly to coordinate magnitude ~1e8, and at
~1e9+ f64 noise in `d_ax` can exceed `TAU_MODEL` so it conservatively reads as NC →
`Err(ASNA)` (a loud, never-wrong failure mode, not a spurious circle). The
on-surface circle oracle itself holds to ~1e9 for this pair. Both ceilings are
inherent to absolute-`TAU_MODEL` comparisons (A14.3), not solver-logic defects.

**Reduction (X2/X1):** with the axis along `â` and the sphere center on it, a point
at axial offset `h` from `C` and radial distance `r_c` from the axis lies on the
cylinder (radius `r_c`) and on the sphere iff `h² + r_c² = r_s²` ⇒
`h = ±√(r_s² − r_c²)`. Both circles have radius `r_c` and normal `â`, centered at
`C ± h·â` (on the axis). Gate on the **linear** quantity `r_s − r_c` (not the
squared difference), per the SSI2/3 lesson; `√(r_s² − r_c²)` is real & > 0 in X2
(`r_s > r_c + TAU_MODEL`).

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — core):** sample each result circle at N params via `eval`;
  every sample satisfies **both** surfaces within `TAU_MODEL` — sphere
  `| |x − C| − r_s | < TAU_MODEL`; cylinder `| dist(x, axisLine) − r_c | < TAU_MODEL`
  (reuse the SSI2 cylinder residual, e.g. `|(x − A) × â| / |â|`). Absolute-`TAU_MODEL`
  oracle valid to coord magnitude ~1e8 (PR-SSI1 finding).
- **I2 (analytical geometry):** X2 → exactly two circles, each `radius == r_c`,
  `normal ∥ â`, centers on the axis line at `C ± h·â` (symmetric about `C`,
  `|center − C| == h == √(r_s²−r_c²)`); X1 → one circle `radius == r_c` at `C`,
  `normal ∥ â`; X0 → `Ok(vec![])`.
- **I3 (branch coverage, P4):** X2, X1, X0, NC, E1 each ≥1 test.
- **I4 (symmetry):** `intersect(sphere, cyl) == intersect(cyl, sphere)` (same circle
  set; order / normal-sign tolerant).
- **I5 (determinism):** identical inputs → byte-identical output (two-circle order
  `+h` first).

## Failure modes
- `r_s ≤ 0` / `r_c ≤ 0` / non-finite / zero axis → `Err(DegenerateInput)`. No
  `panic!`/`unwrap`.
- **NC (non-coaxial) → `Err(AnalyticalSolutionNotAvailable)`** — a deliberate staged
  limitation (the general degree-4 curve + its new `SsiCurve` variant is a later
  increment). Loud, never a mesh/grid fallback (A15.2). The doc-comment must say so.
- Other unimplemented pairs (cyl∩cyl, cone∩cone, sphere∩cone, cyl∩cone) unchanged →
  `Err(AnalyticalSolutionNotAvailable)`.

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8.3** (Case F8,
  implicit/implicit quadric intersection; Example 5.8.4 sphere∩cylinder) —
  `docs/references/patrikalakis-shape-interrogation.txt:26420–26465`. The coaxial
  reduction (`x²+y² = r_c²` ∧ `x²+y²+z² = r_s²` ⇒ `z² = r_s² − r_c²`) is classical;
  cite §5.8.3 + the reduction.
- **Governance:** A15.1 (exact SSI), A15.2 (no fallback — non-coaxial `Err`), A15.4
  (pair #8 cylinder-sphere), P8 (cite research), A14.3 (`TAU_MODEL`).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN separate commits; every branch (X2/X1/X0/NC/E1) tested;
numeric/structural oracles (on-surface w/ sphere + cylinder residuals + analytical
geometry, not "no panic"); canonical (two-circle) + edge (near-tangent `r_c→r_s`,
empty `r_c>r_s`, non-coaxial→ASNA, degenerate) cases; symmetry + determinism; no
`unsafe`/`panic!`; CI gate (fmt + clippy -D warnings) clean for `ssi-rs`; SSI1–5
suites untouched & green (only the SSI2 `sphere_cylinder_not_available` ASNA-path
test possibly re-pointed to a clearly non-coaxial config, since coaxial now returns
circles).
