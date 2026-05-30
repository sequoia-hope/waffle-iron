# Spec: PR-SSI4 — ssi-rs plane∩cone unbounded conics (parabola + hyperbola)

**Status:** active (M5 / roadmap §4b Phase 1 — finish the analytical SSI engine)
**Feature cycle:** ssi-4
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

Implement the **proper unbounded** plane∩cone sections — **parabola** and
**hyperbola** — completing the four proper conic sections of pair #3. PR-SSI3
landed circle + ellipse and gated these behind `Err(AnalyticalSolutionNotAvailable)`
(the PH branch); PR-SSI4 replaces that `Err` with the analytical curves.

Introduces the **first two unbounded `SsiCurve` types** (`Parabola`, `Hyperbola`),
each with its own `eval`, plus the closed-form parameter extraction. On the
infinite double cone, a hyperbola has **two branches** → returned as **two**
`Hyperbola` curves.

**Scope:** parabola + hyperbola only. The **through-apex** degenerate conics
(point / single line / crossed lines) stay `Err(DegenerateInput)` and are deferred
to PR-SSI5 — keeping this increment focused on the two new curve types.

**Precision (unchanged):** true analytical curves, f64 params; no `dashu`.
Clean-room from legacy (the legacy curve-field shapes were extracted as a math hint
only — do **not** read the legacy cone solver source).

## New `SsiCurve` variants (`crates/ssi-rs/src/lib.rs`)

```
SsiCurve::Parabola {
    vertex:       Point3,   // turning point (lies on the cone & in the plane)
    normal:       Vector3,  // unit normal of the cutting plane
    axis_dir:     Vector3,  // unit in-plane axis of symmetry; opens toward +axis_dir
    focal_length: f64,      // f > 0 (the `y² = 4f·x` focal length)
}

SsiCurve::Hyperbola {
    center:          Point3,   // midpoint of the two branch vertices
    normal:          Vector3,  // unit normal of the cutting plane
    major_axis:      Vector3,  // unit transverse axis; THIS branch opens toward +major_axis
    semi_transverse: f64,      // a (center → vertex distance)
    semi_conjugate:  f64,      // b
}
```

**`eval(t)` arms** (extend the existing `SsiCurve::eval`):
- `Parabola`: `vertex + (t²/(4·focal_length))·axis_dir + t·(normal × axis_dir)`, `t ∈ ℝ`.
- `Hyperbola`: `center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)`,
  `t ∈ ℝ` (traces the single branch opening toward `+major_axis`).

`minor`/conjugate in-plane direction is `normal × major_axis` (resp. `normal ×
axis_dir`) — unit and in-plane because `normal ⟂ axis` and both are unit. These
arms are self-contained (no `in_plane_basis`).

## `plane_cone` — PH branch (replaces the SSI3 `AnalyticalSolutionNotAvailable`)

**Reuse SSI3's setup verbatim** — `n̂ = normalize(plane.normal)`,
`â = normalize(cone.axis_dir)`, `α`, `k = n̂·â`, the stable `proj = n̂ − k·â` +
`proj_norm` C1 gate, `û = normalize(proj)`, `g_± = cosα·â ± sinα·û`,
`gd_± = n̂·g_±`, the E1 invalid-cone / AP through-apex `Err`, and the C1 circle / C2
ellipse branches. **Only the PH outcome changes** (`cosα = α.cos()`, etc.):

| # | case | condition | result |
|---|---|---|---|
| PARA | parabola | exactly one `\|gd_±\| < TAU_MODEL` (one generator ∥ plane) | one **Parabola** |
| HYPE | hyperbola | `gd₊.signum() ≠ gd₋.signum()` (both `\|gd_±\| ≥ TAU_MODEL`; vertices on opposite nappes) | **two Hyperbola** (one per branch) |
| AP | through-apex | `\|n̂·(apex−p)\| < TAU_MODEL` (unchanged from SSI3) | `Err(DegenerateInput)` — deferred to PR-SSI5 |

Evaluation order is unchanged from SSI3 (E1 → AP → C1 → compute û/g/gd → then
PARA/HYPE/C2). The PARA test (`min(|gd₊|,|gd₋|) < TAU_MODEL`) precedes HYPE; C2 is
the remaining same-nappe case.

### Hyperbola construction (verified: α=π/4, plane x=1 → center (1,0,0), a=b=1, vertices (1,0,±1))
- `rhs = n̂·(p − apex)`; `V_± = apex + (rhs/gd_±)·g_±` (the two branch vertices, one
  per nappe).
- `center C = ½(V₊ + V₋)`; `m̂ = normalize(V₊ − V₋)`; `semi_transverse a = ½|V₊ − V₋|`.
- `d = C − apex`; `semi_conjugate b = √( |d|² − (d·â)²/cos²α )`. **Sign-flip of the
  SSI3 ellipse `b²`:** the hyperbola center lies *outside* the cone, so
  `|d|² − (d·â)²/cos²α > 0`.
- Return `vec![ Hyperbola{C, n̂, major_axis: m̂, a, b}, Hyperbola{C, n̂, major_axis: −m̂, a, b} ]`
  — **`+m̂` first** (determinism). `+m̂` opens toward `V₊` (`C + a·m̂ = V₊`), `−m̂`
  toward `V₋`. (`eval`'s conjugate dir `n̂ × m̂` is ∥ `cross(n̂,â)`, consistent with
  the `b` derivation, since `m̂ ∈ span{â,n̂}`.)

### Parabola construction (verified: α=π/4, n̂=(1,0,1)/√2, plane through (0,0,1) → vertex (½,0,½), f=1/(2√2), axis (−1,0,1)/√2; eval(1)=(0,−1,1) on both surfaces)
- `g_fin` = the generator with the larger `|gd|`; the other is the ∥ one
  (`|gd| < TAU_MODEL`). `vertex V = apex + (rhs/gd_fin)·g_fin` — the finite
  generator∩plane, which is the vertex (it lies on the in-plane symmetry axis).
- `m̂0 = normalize(â − k·n̂)` — the **in-plane projection of the cone axis** (NOT
  `û = normalize(n̂ − k·â)`; for a parabola `|â − k·n̂| = cosα ≠ 0`, so `normalize`
  is safe).
- `d0 = V − apex`; signed `f = ( (d0·â)/cosα − d0·m̂0 ) / 2`.
- `focal_length = |f|`; `axis_dir = if f ≥ 0 { m̂0 } else { −m̂0 }` (orient so the
  parabola opens outward / toward the widening cone, `focal_length > 0`).
- Return `vec![ Parabola{ vertex: V, normal: n̂, axis_dir, focal_length } ]`.

## Invariants / oracles (P1, DoD §1)

- **I1 (on-surface — the core exactness proof):** sample each result curve via
  `eval` over a **bounded** parameter range (e.g. parabola `t ∈ [−T,T]`, each
  hyperbola branch `t ∈ [−T,T]`; pick `T` so coordinates stay ≪1e8 for the absolute
  oracle, per the PR-SSI1 finding) and assert every sample satisfies **both**
  surfaces within `TAU_MODEL`:
  - plane: `|n̂·(x − p)| < TAU_MODEL`;
  - cone **radial** residual: with `h = (x − apex)·â`,
    `| |(x − apex) − h·â| − |h|·tanα | < TAU_MODEL` (reuse the SSI3 helper).
- **I2 (analytical geometry):**
  - **Parabola:** `vertex` on cone & in plane; `axis_dir` unit, in-plane
    (`|n̂·axis_dir| < TAU_MODEL`) & ∥ the in-plane cone-axis projection; `focal_length`
    finite & `> 0`; the parabola opens toward the widening cone.
  - **Hyperbola:** exactly **two** curves with shared `center` (in the plane), `a`,
    `b`, `normal`; `major_axis` unit, in-plane, **opposite signs** on the two; each
    branch vertex `center ± a·major_axis` on the cone & in the plane; `a, b > 0`
    finite.
- **I3 (branch coverage, P4):** PARA and HYPE (2 curves) each ≥1 test; SSI3 branches
  (C1/C2/AP/E1) still pass (regression — guaranteed by the untouched ssi3 suite).
- **I4 (symmetry):** `intersect(plane,cone) == intersect(cone,plane)` for a parabola
  and a hyperbola case (same curve set; axis sign / branch order tolerant).
- **I5 (determinism):** identical inputs → byte-identical output, including the
  two-`Hyperbola` order (`+m̂` first).

## Failure modes
- Through-apex (AP) → `Err(DegenerateInput)` (staged; the point/line/two-line
  sub-classification is PR-SSI5). Invalid cone / zero vectors → `Err(DegenerateInput)`
  (E1, unchanged). No `panic!`/`unwrap`.
- `AnalyticalSolutionNotAvailable` is **no longer** returned by `plane_cone` for any
  proper conic; it remains the verdict for still-unimplemented *pairs*
  (sphere∩cone, cyl∩cone, cone∩cone, sphere∩cyl, cyl∩cyl).

## Research basis
- **Patrikalakis & Maekawa**, *Shape Interrogation for CAD/M*, **§5.8** (natural
  quadrics) — `docs/references/patrikalakis-shape-interrogation.txt` (elliptic-cone
  implicit form `:8193`, conics `:1071`/`:5343`). The conic-type-by-generator
  construction (Dandelin) is classical; cite §5.8 + the conic fact.
- **Governance:** A15.1 (exact SSI for quadrics), A15.2 (no fallback — through-apex
  `Err`, staged), A15.4 (pair #3), P8 (cite research), A14.3 (`TAU_MODEL`).

## Definition of Done (DoD §1)
Spec (this file); RED→GREEN separate commits; every new branch (PARA, HYPE)
tested + the two-branch hyperbola contract; numeric/structural oracles (on-surface
w/ cone radial residual + analytical geometry, not "no panic"); canonical
(parabola, hyperbola) + edge (near the ellipse↔parabola and parabola↔hyperbola
boundaries; very-open parabola / eccentric hyperbola; oblique non-axis cone) cases;
symmetry + determinism; no `unsafe`/`panic!`; CI gate (fmt + clippy -D warnings)
clean for `ssi-rs`; SSI1–3 suites untouched & still green (only mechanical
`Parabola`/`Hyperbola` match arms added to their helpers).
