# Spec: PR-SSI3 — ssi-rs plane∩cone (bounded sections: circle + ellipse)

**Status:** active (M5 step 3 — Yang Stage 3's analytical-curve engine, 3rd increment)
**Feature cycle:** ssi-3
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Add the **`Cone`** quadric surface and the **`plane_cone`** solver's **bounded**
branches — **circle** (plane ⟂ axis) and **ellipse** (closed section) — to
`ssi-rs`. This is A15.4 matrix pair #3 (Plane–Cone). It reuses the existing
`SsiCurve::Circle`/`Ellipse` and directly mirrors `plane_cylinder` (PR-SSI2).

**Scope decision (confirmed with user): bounded sections first.** The unbounded
conics (parabola, hyperbola) and the degenerate through-apex conics
(point/line/two-lines) are **deliberately deferred** to PR-SSI4 (they need new
unbounded-curve types with their own `eval` + two-branch handling). In PR-SSI3
those configurations return a **loud `Err`** — never a wrong answer, never a
fallback (A15.2). No consumer is wired to `ssi-rs` yet, so the staged partial
solver breaks nothing.

**Precision (unchanged):** analytical curve representation, f64 parameters; zero
shape error; topology robustness lives in the exact mesh predicates, not here.
f64 closed-form; no `dashu`.

> A15.4 marks pair #3 "done", but that describes the **legacy**
> `crates/kernel/src/ssi/`. For the new `ssi-rs` it is unimplemented; implement
> clean-room from this spec (legacy is a math hint only — do **not** read its cone
> solver source).

## New type (added to `crates/ssi-rs/src/lib.rs`)

```
QuadricSurface::Cone {            // infinite DOUBLE cone (pure quadric)
    apex:       Point3,
    axis_dir:   Vector3,          // axis direction (normalized defensively; need not be unit on input)
    half_angle: f64,              // α ∈ (0, π/2): angle between the axis and a generator
}
// implicit: x is on the cone iff its radial distance from the axis equals
// |h|·tan α, where h = (x − apex)·â. Equivalently ((x−apex)·â)² = cos²α·|x−apex|²
// (BOTH nappes — it is the full quadric double cone).
```
Double cone (not single-nappe): consistent with the infinite plane/cylinder as
pure quadrics; trimming to a nappe is a consumer concern. (A hyperbola section's
two branches — a consequence of the double cone — are a PR-SSI4 concern.)

No new `SsiCurve` variants in PR-SSI3.

## Solver: `plane_cone(plane, cone) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis §5.8 (natural-quadric SSI; local extract
`docs/references/patrikalakis-shape-interrogation.txt`, elliptic-cone implicit
form ~`:8193`, conics ~`:1071`/`:5343`) + the classical conic-section /
cone-generator construction.

Let `n̂` = unit plane normal, `p` = plane point, `â` = unit axis, `α` = half_angle,
`k = n̂·â` (signed), `s_n = √(1 − k²) = sin(angle between n̂ and â)`. Compute
`cosα, sinα, tanα` once.

### Branch table

| # | case | condition | result |
|---|---|---|---|
| E1 | invalid cone / degenerate input | `α` non-finite or `α ≤ TAU_MODEL` or `α ≥ π/2 − TAU_MODEL`; or `axis_dir`/`normal` zero-length or non-finite | `Err(DegenerateInput)` |
| AP | through-apex | `\|n̂·(apex − p)\| < TAU_MODEL` (apex lies on the cutting plane) | `Err(DegenerateInput)` — degenerate conic (point/line/two-lines), deferred to PR-SSI4 |
| C1 | circle (plane ⟂ axis) | `s_n < TAU_MODEL` | one **Circle** { center = apex + h·â, normal = â, radius = \|h\|·tanα }, `h = n̂·(p − apex)/k` |
| C2 | ellipse (closed) | `sign(gd₊) = sign(gd₋)` and `min(\|gd₊\|, \|gd₋\|) > TAU_MODEL` | one **Ellipse** (construction below) |
| PH | parabola / hyperbola | one `\|gd_±\| ≤ TAU_MODEL` (parabola: a generator ∥ plane) **or** `sign(gd₊) ≠ sign(gd₋)` (hyperbola: vertices on opposite nappes) | `Err(AnalyticalSolutionNotAvailable)` — staged gap, implemented in PR-SSI4 |

The **symmetry-plane generators** (the two cone generators in `span{â, n̂}`, the
plane that carries the ellipse's major axis) are
`g_± = cosα·â ± sinα·û`, with `û = normalize(n̂ − k·â)` (the component of `n̂`
perpendicular to `â`; `|n̂ − k·â| = s_n`, so `normalize` is well-defined because C1
already consumed `s_n < TAU_MODEL`). `gd_± = n̂·g_±` measures how steeply each
generator pierces the cutting plane.

**Evaluation order in code:** E1 → AP → C1 → (compute `û`, `g_±`, `gd_±`) → C2 vs
PH. (C1 must precede the `û` computation since `û` is undefined when `s_n→0`.)

### C2 ellipse construction (vertex method — no eccentricity formula)

- Vertices `V_± = apex + s_±·g_±`, `s_± = n̂·(p − apex) / gd_±` (intersect each
  symmetry-plane generator with the cutting plane). The C2 guard guarantees both
  are finite and on the same nappe ⇒ a genuine closed ellipse.
- `center C = (V₊ + V₋)/2`;  `major_radius a = |V₊ − V₋| / 2`;
  `major_axis = normalize(V₊ − V₋)`.
- minor direction `ŵ = normalize(n̂ × â)` (⟂ the symmetry plane, lies in the
  cutting plane).
- `minor_radius b = √( (d·â)²/cos²α − |d|² )`, `d = C − apex`. Derivation:
  intersect the cone with the in-plane line `C + u·ŵ`; since `ŵ⟂â` and `ŵ⟂d`
  (`d` lies in the symmetry plane), the cone equation collapses to
  `u² = (d·â)²/cos²α − |d|²`, positive because the ellipse center is inside the
  cone. `a ≥ b` by construction.
- Result: `Ellipse { center: C, normal: n̂, major_axis, major_radius: a, minor_radius: b }`.

**Circle-limit sanity:** as `s_n→0` (plane ⟂ axis at axial height `h`),
`C = apex + h·â`, `d·â = h`, `b = |h|·tanα` — matching the C1 branch (C1 exists
precisely because `û` and the vertex method degenerate there).

**Ellipse↔parabola boundary:** as a generator becomes parallel to the plane
(`gd_±→0`, i.e. `k→sinα`), `s_±→∞` so `a→∞`. The `min(|gd₊|,|gd₋|) > TAU_MODEL`
guard routes that limit to PH (loud `Err`) instead of emitting a blown-up ellipse
— the same "gate on the geometrically-meaningful quantity, not a derived
difference" lesson as PR-SSI2's C1 fix.

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — the core exactness proof):** sample each result curve at
  N params via `eval`; every sample satisfies **both** surfaces within `TAU_MODEL`:
  - plane: `|n̂·(x − p)| < TAU_MODEL`;
  - cone **radial** residual: with `h = (x − apex)·â`,
    `r_actual = |(x − apex) − h·â|`, assert `| r_actual − |h|·tanα | < TAU_MODEL`
    (a length — NOT the squared implicit form, whose units are length²).
  Reuse the PR-SSI2 `assert_on_both_surfaces` pattern; add a cone radial residual
  helper. Absolute-`TAU_MODEL` oracle valid to coord magnitude ~1e8 (PR-SSI1
  finding); keep test geometry in range.
- **I2 (analytical geometry):** C1 `radius == |h|·tanα`, center on axis & in plane,
  normal ∥ axis; C2 `center` = midpoint of the two vertices, **both vertices lie on
  the cone and in the plane**, `a = |V₊−V₋|/2`, `b` per the formula,
  `major_axis ⟂ minor_axis` and both ⟂ `normal`, `a ≥ b`.
- **I3 (branch coverage, P4):** C1, C2, AP, PH(parabola), PH(hyperbola), E1 each
  have ≥1 test; AP and PH assert the specific `Err` variant.
- **I4 (symmetry):** `intersect(plane, cone) == intersect(cone, plane)` (same
  curve; line/axis sign may flip — compare unoriented / up to sign).
- **I5 (determinism):** identical inputs → byte-identical output.

## Failure modes
- Invalid cone (`α` outside the open interval), zero/non-finite `axis_dir`/`normal`
  → `Err(DegenerateInput)`. No `panic!`.
- Through-apex (AP) → `Err(DegenerateInput)` (the section is a degenerate conic;
  its point/line/two-lines sub-classification is deferred to PR-SSI4).
- Parabola / hyperbola (PH) → `Err(AnalyticalSolutionNotAvailable)`. **This is a
  deliberate staged limitation, not a "no solver" verdict** — PR-SSI4 implements
  these analytically. It is loud and never a mesh/grid fallback (A15.2); the caller
  decides. The doc-comment must say so explicitly.
- Other still-unimplemented pairs (`Sphere∩Cone`, `Cylinder∩Cone`, `Cone∩Cone`,
  and any with `Cone` not paired with `Plane`) → `Err(AnalyticalSolutionNotAvailable)`.

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8
  Surface/Surface Intersections** (natural quadrics) — local extract
  `docs/references/patrikalakis-shape-interrogation.txt` (elliptic-cone implicit
  form `:8193`, conics `:1071`/`:5343`). The conic-type-by-generator construction
  (ellipse via the two symmetry-plane generators) is classical analytic geometry;
  cite §5.8 + the conic-section fact.
- **Governance:** A15.1 (exact SSI for quadrics), A15.2 (no fallback — staged gaps
  `Err`), A15.4 (pair #3), P8 (cite research), A14.3 (`cad-primitives` tolerances —
  `TAU_MODEL`).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN separate commits; every branch (E1/AP/C1/C2/PH-parabola/
PH-hyperbola) tested; numeric/structural oracles (on-surface w/ cone radial residual
+ analytical geometry, not "no panic"); canonical (perpendicular circle, oblique
ellipse) + edge (near circle/ellipse boundary, ellipse near the parabola boundary →
PH `Err`, through-apex `Err`, invalid cone `Err`) cases; symmetry + determinism;
no `unsafe`/`panic!`; CI gate (fmt + clippy -D warnings) clean for `ssi-rs`;
PR-SSI1/SSI2 tests untouched and still green (only the mechanical `Cone` match arm
added to their test helpers).
