# Spec: PR-SSI2 — ssi-rs plane∩cylinder solver

**Status:** active (M5 step 2 — Yang Stage 3's analytical-curve engine, 2nd increment)
**Feature cycle:** ssi-2
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Add the **`Cylinder`** quadric surface, the **`Ellipse`** intersection curve, and
the **`plane_cylinder`** solver to `ssi-rs`. This is A15.4 matrix pair #2
(Plane–Cylinder), the conic-section workhorse: a plane cuts an infinite
right-circular cylinder in a circle, an ellipse, or one/two lines. It introduces
the first **non-circular** curve while staying entirely in closed-form territory —
no Degree-4 parametric quartics (those are a later increment).

**Precision (unchanged from PR-SSI1):** analytical curve *representation*, **f64
parameters**. An `Ellipse` is the exact ellipse (zero shape error); topology
robustness lives in the exact mesh predicates, not here. f64 closed-form; no `dashu`.

> A15.4 marks pair #2 "done", but that describes the **legacy**
> `crates/kernel/src/ssi/`. For the new `ssi-rs` it is unimplemented; implement
> clean-room from this spec (ssi-rs CLAUDE.md: legacy is a math hint, never source
> to lift — do **not** read `plane_cylinder_{perp,parallel,oblique}`).

## Types (added to `crates/ssi-rs/src/lib.rs`)

```
// New QuadricSurface arm — infinite right-circular cylinder.
QuadricSurface::Cylinder {
    axis_point: Point3,   // any point on the axis
    axis_dir:   Vector3,  // axis direction (normalized defensively; need not be unit on input)
    radius:     f64,      // > 0
}   // implicit: dist(x, axis line) = radius

// New SsiCurve arm — exact ellipse.
SsiCurve::Ellipse {
    center:       Point3,   // ellipse center (= axis ∩ plane)
    normal:       Vector3,  // unit normal of the cutting plane
    major_axis:   Vector3,  // unit in-plane direction of the semi-major axis
    major_radius: f64,      // semi-major length a  (a ≥ b)
    minor_radius: f64,      // semi-minor length b
}
```

**`SsiCurve::Ellipse::eval(t)`** (extend the existing `eval`):
`center + a·cos t · major_axis + b·sin t · minor_axis`, where
`minor_axis = normal × major_axis` (unit and in-plane because `normal ⟂ major_axis`
and both are unit). Self-contained — does **not** call `in_plane_basis` (the major
axis is explicit), so the ellipse frame is exactly the one the solver chose (I5).

`Cylinder`/`Ellipse` are consumed by the new solver in this same PR, so no
dead-code-lint concern.

## Solver: `plane_cylinder(plane, cylinder) -> Result<Vec<SsiCurve>, SsiError>`

Doc-comment cites Patrikalakis §5.8 (Surface/surface intersections — natural
quadrics) + the standard plane-cylinder conic-section result (a plane section of a
quadric is a conic; `docs/references/patrikalakis-shape-interrogation.txt:1071`).

Let `n̂` = unit plane normal, `p` = plane point, `q` = `axis_point`,
`â` = unit `axis_dir`, `r` = cylinder radius, and `c = n̂·â`
(`|c|` = sine of the dihedral angle between plane and axis; `|c|=1` ⇒ plane ⟂ axis,
`|c|=0` ⇒ plane ∥ axis).

### Branch table

| # | case | condition | result |
|---|---|---|---|
| C1 | perpendicular | `\|c\| > 1 − TAU_MODEL` | one **Circle** { center = axis ∩ plane, normal = â, radius = r } |
| C2 | oblique | `TAU_MODEL ≤ \|c\| ≤ 1 − TAU_MODEL` | one **Ellipse** (see below) |
| C3a | parallel, secant | `\|c\| < TAU_MODEL` and `d < r − TAU_MODEL` | **two Lines** parallel to â (see below) |
| C3b | parallel, tangent | `\|c\| < TAU_MODEL` and `\|d − r\| ≤ TAU_MODEL` | **one Line** (the tangent line) |
| C3c | parallel, disjoint | `\|c\| < TAU_MODEL` and `d > r + TAU_MODEL` | `Ok([])` |
| E1 | degenerate | `r ≤ 0` or non-finite `r`; or `axis_dir` / `normal` zero-length or non-finite | `Err(DegenerateInput)` |

**Distances / center:**
- `d = |n̂·(q − p)|` — distance from the axis to the plane. In C3 (`â ⟂ n̂`) the
  whole axis is at this constant distance from the plane.
- **axis ∩ plane** (C1/C2, where `c ≠ 0`): `center = q + s·â`,
  `s = (n̂·(p − q)) / c`.

**C2 ellipse construction:**
- `minor_radius b = r`.
- `major_radius a = r / |c|`  (`|c|→1` ⇒ `a→r`, the C1 circle limit; `|c|→0` ⇒
  `a→∞`, the C3 line limit — both routed away by the bands).
- `major_axis = normalize(â − c·n̂)` — the projection of the axis onto the plane
  (the "uphill" in-plane direction); this is the long axis. It is in-plane
  (`n̂·(â − c n̂) = c − c = 0`) and well-defined whenever `0 < |c| < 1`.
- `normal = n̂`. (`minor_axis = n̂ × major_axis` is implied by `eval`.)

**C3a two lines:**
- `ŵ = normalize(n̂ × â)` — in-plane, perpendicular to the axis.
- `c0 = q − (n̂·(q − p))·n̂` — foot of the axis on the plane (signed `d` used here,
  not `|d|`).
- `off = √(r² − d²)`. Two lines: `{ point: c0 + off·ŵ, dir: â }` then
  `{ point: c0 − off·ŵ, dir: â }`. **Order is fixed (`+ŵ` first)** for determinism
  (I5).

**C3b one line:** `{ point: c0, dir: â }` (the single tangent line).

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — the core exactness proof):** sample each result curve at N
  parameters via `eval`; every sample satisfies **both** surfaces within
  `TAU_MODEL`:
  - plane: `|n̂·(x − p)| < TAU_MODEL`;
  - cylinder: `| dist(x, axisLine) − r | < TAU_MODEL`, with
    `dist(x, axisLine) = | (x − q) − ((x − q)·â) â |`.
  Reuse PR-SSI1's `assert_on_both_surfaces` pattern; add the cylinder residual
  helper. Absolute-`TAU_MODEL` oracle, valid to coordinate magnitude ~1e8 (the
  PR-SSI1 finding); keep test geometry within that range.
- **I2 (analytical geometry):** C1 `radius == r`, center on the axis and in the
  plane; C2 `minor == r`, `major == r/|c|`, `major_axis ⟂ minor_axis`, both ⟂
  `normal`, center = axis∩plane; C3a both lines parallel to â, each at distance
  exactly `r` from the axis, symmetric about `c0`.
- **I3 (branch coverage, P4):** C1, C2, C3a, C3b, C3c, E1 each have ≥1 test.
- **I4 (symmetry):** `intersect(plane, cyl)` == `intersect(cyl, plane)` (same
  curve geometry; line `dir` and ellipse `major_axis` may be sign-flipped — compare
  unoriented / up to sign).
- **I5 (determinism):** identical inputs → byte-identical output (the ellipse
  major-axis sign, the circle normal, and the two-line ordering are all
  deterministic functions of the inputs).

## Failure modes
- `r ≤ 0` / non-finite, zero/non-finite `axis_dir` or plane `normal`
  → `Err(DegenerateInput)`. No `panic!`.
- `AnalyticalSolutionNotAvailable` stays reserved for pairs with no solver
  (sphere_cylinder / cylinder_cylinder / cone — Degree-4, future). With the surface
  set after this PR (`Plane`/`Sphere`/`Cylinder`) the still-unimplemented pairs are
  sphere∩cylinder and cylinder∩cylinder, so `intersect` now **has** a
  triggerable-`AnalyticalSolutionNotAvailable` path — add a test that
  `intersect(sphere, cylinder)` returns `Err(AnalyticalSolutionNotAvailable)`
  (A15.2: loud, never a fallback). (This was not constructible in PR-SSI1.)

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8
  Surface/Surface Intersections** (natural quadrics) — local extract
  `docs/references/patrikalakis-shape-interrogation.txt`; conic-section grounding
  at `:1071` ("called conic sections"). The plane-section-of-a-cylinder conic
  result is standard analytic geometry; cite §5.8 + the conic-section fact.
- **Governance:** A15.1 (exact SSI for quadrics), A15.2 (no fallback — absent
  pairs `Err`), A15.4 (pair #2), P8 (cite research), A14.3 (`cad-primitives`
  tolerances — `TAU_MODEL`).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN separate commits; every branch (C1/C2/C3a/C3b/C3c/E1)
tested; numeric/structural oracles (on-surface + analytical geometry, not "no
panic"); canonical (perpendicular circle, oblique ellipse, parallel two-line) +
edge (near-band limits, tangent line, disjoint, degenerate) cases; symmetry +
determinism; the new `AnalyticalSolutionNotAvailable` path (sphere∩cylinder)
tested; no `unsafe`/`panic!`; CI gate (fmt + clippy -D warnings) clean for
`ssi-rs`; PR-SSI1 tests (`ssi1.rs`, `ssi1_adversary.rs`) untouched and still green.
