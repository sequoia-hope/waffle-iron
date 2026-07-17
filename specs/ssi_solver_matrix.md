# SSI Solver Matrix

Authoritative enumeration of the analytical surface–surface intersection (SSI)
solvers in **`crates/ssi-rs`**, their sub-cases, implementation status, and
acceptance criteria. Consumed by `yang-rs` Stage 3 (refinement of
mesh-approximate intersection curves to surface-exact analytic curves).

> **Rewritten 2026-07-12** to describe the live `crates/ssi-rs` crate at current
> HEAD. **Supersedes the pre-Phase-6 legacy matrix**, which described the deleted
> `crates/kernel/src/ssi/` solvers (the `Degree4*` parametric-curve variants,
> `plane_torus_ssi`, `torus_torus_ssi`, etc. — all gone with the Phase-6 kernel
> deletion, 2026-06-11). Every file:line anchor below points at
> `crates/ssi-rs/src/lib.rs`.

This is a **living document**. Update it as solvers land.

## Goal

Define exactly what "done" means for each SSI pair and track progress toward full
analytical coverage per A15.1 (exact SSI for analytical surface pairs). The
governance A15.4 status table links here for the detailed sub-case breakdown.

## What the crate covers

`ssi-rs` operates on the natural-quadric surfaces in
`QuadricSurface` (`lib.rs:97`):

```
QuadricSurface = { Plane, Sphere, Cylinder, Cone }
```

**Torus is deliberately NOT a `QuadricSurface`** — a torus is a degree-4 surface,
not a quadric, and its doc note (`lib.rs:95-96`) records that it "arrives with
its solver." That means the crate implements **10 unique pairs** (the 6
off-diagonal + 4 same-type pairs among 4 surfaces), not the legacy matrix's 15.
The 5 torus-bearing pairs are handled one tier up (see
[§ Torus routing](#torus-not-in-quadricsurface)).

Intersection curves are returned as `SsiCurve` (`lib.rs:135`), an exact analytic
representation — **never a polyline or sampled point set**:

```
SsiCurve = { Line, Circle, Ellipse, Parabola, Hyperbola, SurfacePair }
```

`SurfacePair { a, b }` (`lib.rs:214`) is the M5 procedural degree-4 curve: the
general-position quadric-pair intersection that has no conic closed form is
represented **implicitly and exactly by its two defining surfaces** (P8 degree-4
clarification; [#24] Yang §4.1.2/§4.3). Concrete points are certified downstream
by Newton projection onto both surfaces (yang-rs `relocate_onto_implicit_pair`),
never carried on the curve — `SsiCurve::eval` on a `SurfacePair` returns NaN by
design (`lib.rs:337`, a loud wrong answer, never a plausible-but-wrong one, P9).

The public dispatcher `intersect(a, b)` (`lib.rs:381`) is symmetric (I4: both
argument orders route to the same solver) and returns:

- `Ok(vec![...])` — the analytic curve(s), or an empty `Vec` when the surfaces
  do not meet in a curve (disjoint / tangent-point / parallel).
- `Err(SsiError::DegenerateInput)` — degenerate configuration (coincident,
  zero/negative radius, concentric, zero/non-finite direction).
- `Err(SsiError::AnalyticalSolutionNotAvailable)` — a pair with no analytic
  solver and no `SurfacePair` producer (A15.2: loud `Err`, never a silent
  mesh/grid fallback). Since F10 (2026-07-12) NO pair returns this as a staged
  capability gap; it survives only as the documented absolute-band
  scale-sensitivity fallback in the sphere solvers' coaxial discriminant.

## Error status vocabulary

| Status | Meaning |
|--------|---------|
| `done` | Closed-form analytic `SsiCurve` (Line/Circle/Ellipse/Parabola/Hyperbola) for every geometrically-real sub-case; tested. |
| `done-via-SurfacePair` | Special configs are closed-form conics; the general-position degree-4 arm returns the exact procedural `SurfacePair` (M5). Tested. |
| `not-in-crate` | Pair involves `Torus`, which is not a `QuadricSurface`; handled above `ssi-rs`. |

## Acceptance criteria (per sub-case)

A sub-case is "done" when:

1. Returns an exact analytic `SsiCurve` (or the `SurfacePair` implicit curve for
   general-position degree-4) — never a polyline or sampled approximation.
2. Unit test with a geometric oracle (returned curve lies on both surfaces
   within `TAU_MODEL`; `SurfacePair` operands verified by on-surface Newton in
   yang-rs).
3. Determinism (I5): byte-identical output across runs; emission order fixed
   (e.g. `+ŵ` / `+h` / larger-`t` first).
4. No sampling loops, no grid scans, no ad-hoc epsilons — tolerances come from
   `cad-primitives` (`TAU_MODEL`), per A14.3.

Test files: `crates/ssi-rs/tests/ssi<N>.rs` + `ssi<N>_adversary.rs`. The
`ssi<N>` numbering follows the PR-SSI implementation order, not the pair-index
column below:

| Test file | Covers |
|-----------|--------|
| `ssi1` | plane∩plane, plane∩sphere, sphere∩sphere foundation + `intersect`/`eval`/symmetry |
| `ssi2` | plane∩cylinder |
| `ssi3` | plane∩cone bounded (circle + ellipse) |
| `ssi4` | plane∩cone unbounded (parabola + hyperbola) |
| `ssi5` | plane∩cone through-apex degenerate conics (point / 1 line / 2 lines) |
| `ssi6` | sphere∩cylinder (coaxial circles) |
| `ssi7` | sphere∩cone (coaxial circles) |
| `ssi8` | cylinder∩cone (coaxial circles + general SurfacePair) |
| `ssi9` | cone∩cone (coaxial circles + general SurfacePair) |
| `ssi10`–`ssi12` | cylinder∩cylinder (parallel lines, equal-R ellipses, general SurfacePair) |

---

## The 10 implemented pairs

### 1. Plane–Plane — `done`

Solver `plane_plane` (`lib.rs:440`).

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| Transverse (`\|n_a × n_b\| > TAU`) | `Line` (dir `n_a × n_b`, point via 2×2 solve) | done | ssi1 |
| Parallel, distinct | `Ok([])` | done | ssi1 |
| Coincident (same plane) | `Err(DegenerateInput)` — 2D overlap | done | ssi1 |

### 2. Plane–Sphere — `done`

Solver `plane_sphere` (`lib.rs:506`). Signed distance `d = n·(center − p)`.

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| Cutting (`\|d\| < r`) | `Circle` radius `√(r²−d²)` at foot of perpendicular | done | ssi1 |
| Tangent (`\|d\| ≈ r`) | `Ok([])` (point contact) | done | ssi1 |
| Disjoint (`\|d\| > r`) | `Ok([])` | done | ssi1 |
| `radius ≤ 0` / non-finite | `Err(DegenerateInput)` | done | ssi1 |

### 3. Plane–Cylinder — `done`

Solver `plane_cylinder` (`lib.rs:573`). `c = n̂·â`, in-plane axis projection
`proj = â − c·n̂`.

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| C1 — perpendicular (`\|proj\| < TAU`) | `Circle` radius `r`, normal `â` | done | ssi2 |
| C2 — oblique | `Ellipse` `minor=r`, `major=r/\|c\|`, major axis `= proj` | done | ssi2 |
| C3a — parallel secant (`d < r`) | two `Line`s ∥ `â` at `c₀ ± √(r²−d²)·ŵ` | done | ssi2 |
| C3b — parallel tangent (`d ≈ r`) | one `Line` at the foot | done | ssi2 |
| C3c — parallel disjoint (`d > r`) | `Ok([])` | done | ssi2 |
| `r ≤ 0` / zero axis or normal | `Err(DegenerateInput)` | done | ssi2 |

### 4. Plane–Cone — `done`

Solver `plane_cone` (`lib.rs:736`). The full proper-conic family plus the
through-apex degenerate conics. `k = n̂·â`; symmetry-plane generators
`g_± = cosα·â ± sinα·û`, generator/plane dots `gd_± = n̂·g_±`.

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| C1 — perpendicular | `Circle` radius `\|h\|·tanα` | done | ssi3 |
| C2 — oblique, both generators same nappe | `Ellipse` (vertex method) | done | ssi3 |
| PARA — exactly one generator ∥ plane | `Parabola` (vertex + focal length) | done | ssi4 |
| HYPE — generators on opposite nappes | **two** `Hyperbola` (one per branch, `+m̂` / `−m̂`) | done | ssi4 |
| AP-pt — through apex, plane steeper than cone | `Ok([])` (apex point) | done | ssi5 |
| AP-line — through apex, one generator tangent | one `Line` (`dir = m̂`) | done | ssi5 |
| AP-lines — through apex, opposite nappes | two crossed `Line`s | done | ssi5 |
| E1 — `α ≤ TAU` / `α ≥ π/2−TAU` / non-finite / zero axis-or-normal | `Err(DegenerateInput)` | done | ssi3–5 |

### 5. Sphere–Sphere — `done`

Solver `sphere_sphere` (`lib.rs:945`). Center distance `D`, chord offset
`a = (D²+r_a²−r_b²)/(2D)`.

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| Overlapping (`\|r_a−r_b\| < D < r_a+r_b`) | `Circle` radius `√(r_a²−a²)`, normal along center line | done | ssi1 |
| Tangent (external or internal) | `Ok([])` (point contact) | done | ssi1 |
| Disjoint / contained | `Ok([])` | done | ssi1 |
| Concentric (`D < TAU`) or `radius ≤ 0` | `Err(DegenerateInput)` | done | ssi1 |

### 6. Sphere–Cylinder — `done-via-SurfacePair`

Solver `sphere_cylinder` (`lib.rs:1029`). Coaxial ::= cylinder axis passes
through the sphere center (`d_ax < TAU`).

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| X2 — coaxial, `r_s − r_c > TAU` | two `Circle`s at `C ± √(r_s²−r_c²)·â`, `+h` first | done | ssi6 |
| X1 — coaxial tangent (`\|r_s−r_c\| ≤ TAU`) | one great `Circle` at `C` | done | ssi6 |
| X0 — coaxial, cylinder wider than sphere | `Ok([])` | done | ssi6 |
| NC — non-coaxial general degree-4 (`d_ax ≥ TAU`) | `SurfacePair { a: cyl, b: sphere }` at `lib.rs:1063-1064` | done-via-SurfacePair ([F10](#f10--sphere-general-position-degree-4-closed-2026-07-12)) | ssi6 |
| E1 — `r_s ≤ 0` / `r_c ≤ 0` / zero axis | `Err(DegenerateInput)` | done | ssi6 |

### 7. Sphere–Cone — `done-via-SurfacePair`

Solver `sphere_cone` (`lib.rs:1141`). Coaxial ::= sphere center on the cone axis
line. Linear branch gate `g = r_s − \|h₀\|·sinα` (`sign(D) = sign(g)`).

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| X2 — coaxial, `g > TAU` | two `Circle`s at `h_± = (h₀ ± √D)·cos²α`, `+√D` first | done | ssi7 |
| X1 — coaxial tangent (`\|g\| ≤ TAU`) | one `Circle` at `h₀·cos²α` | done | ssi7 |
| X0 — coaxial, sphere too small (`g < −TAU`) | `Ok([])` | done | ssi7 |
| NC — non-coaxial general degree-4 (`d_ax ≥ TAU`) | `SurfacePair { a: cone, b: sphere }` at `lib.rs:1185-1186` | done-via-SurfacePair ([F10](#f10--sphere-general-position-degree-4-closed-2026-07-12)) | ssi7 |
| E1 — `r_s ≤ 0` / bad α / zero axis | `Err(DegenerateInput)` | done | ssi7 |

### 8. Cylinder–Cone — `done-via-SurfacePair`

Solver `cylinder_cone` (`lib.rs:1272`). Coaxial ::= axes parallel AND cylinder
axis-point on the cone axis line.

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| X2 — coaxial | **exactly two** `Circle`s at `h = ± r_c·cotα` (no discriminant, P9/P10) | done | ssi8 |
| NC — non-coaxial general degree-4 | `SurfacePair { cylinder, cone }` (M5), returned at `lib.rs:1323-1327` | done-via-SurfacePair | ssi8 |
| E1 — `r_c ≤ 0` / bad α / zero axis (either) | `Err(DegenerateInput)` | done | ssi8 |

### 9. Cone–Cone — `done-via-SurfacePair`

Solver `cone_cone` (`lib.rs:1397`). Coaxial ::= axes parallel AND apex₂ on cone₁
axis line. Perfect-square discriminant `(2·m₁·m₂·δ)²` ⇒ no synthetic √ sign gate.

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| X2 — coaxial, unequal α, `\|δ\| > TAU` | two `Circle`s (larger-`t` first) | done | ssi9 |
| X1 — coaxial, equal α, `\|δ\| > TAU` | one `Circle` at bisector `t = δ/2` | done | ssi9 |
| X0 — coaxial, unequal α, `\|δ\| ≤ TAU` | `Ok([])` (shared apex, radius-0 point) | done | ssi9 |
| CO — coaxial, equal α, `\|δ\| ≤ TAU` | `Err(DegenerateInput)` (identical cone, 2D overlap) | done | ssi9 |
| NC — non-coaxial general degree-4 | `SurfacePair { a, b }` (M5), returned at `lib.rs:1450-1452` | done-via-SurfacePair | ssi9 |
| E1 — bad α (either) / zero axis (either) | `Err(DegenerateInput)` | done | ssi9 |

### 10. Cylinder–Cylinder — `done-via-SurfacePair`

Solver `cylinder_cylinder` (`lib.rs:1532`); equal-R ellipse helper
`cyl_cyl_equal_radius_ellipses` (`lib.rs:1663`); axis-crossing point
`line_line_intersection` (`lib.rs:1704`).

| Sub-case | Representation | Status | Tests |
|----------|----------------|--------|-------|
| Parallel secant (`\|r₁−r₂\| < d < r₁+r₂`) | two `Line`s ∥ `û` at `center ± h·p̂`, `+h` first | done | ssi10 |
| Parallel tangent (external or internal) | one `Line` | done | ssi10 |
| Parallel disjoint / contained | `Ok([])` | done | ssi10 |
| Parallel coincident axes, unequal R | `Ok([])` (concentric) | done | ssi10 |
| Parallel coincident axes, equal R | `Err(DegenerateInput)` (2D overlap) | done | ssi10 |
| Non-parallel, equal-R, coplanar (intersecting axes) | **two** `Ellipse` in the angle-bisecting planes | done | ssi11 |
| Non-parallel general (unequal-R, or equal-R skew) | `SurfacePair { a, b }` (M5), returned at `lib.rs:1596` | done-via-SurfacePair | ssi12 |
| E1 — `r ≤ 0` (either) / non-finite axis-point / zero axis | `Err(DegenerateInput)` | done | ssi10 |

---

## Torus (not in `QuadricSurface`)

The 5 torus-bearing pairs — Plane–Torus, Cylinder–Torus, Cone–Torus,
Sphere–Torus, Torus–Torus — have **no solver in `ssi-rs`**. A torus is degree-4,
not a quadric, so it is excluded from `QuadricSurface` (`lib.rs:95-96`). Torus
intersection geometry is produced **above `ssi-rs`**, by two mechanisms:

1. **Coaxial curved rims → exact `Circle`** — in kernel-v2
   `recover.rs` (`crates/kernel-v2/src/recover.rs:353` onward, the "KV7
   extension — curved ∩ curved coaxial rim"). When two coaxial curved laterals
   (torus/cylinder/cone about a shared axis) meet, the retag step mints the rim
   as an exact `Circle` directly, bypassing SSI. This is how coaxial torus
   booleans keep circular-rim vocabulary.

2. **General-position torus edges → Stage-4 implicit relocation** — yang-rs
   Stage 3 refuses a torus to the quadric solver: `surface_to_quadric` maps
   `Surface::Torus` to `Err(UnsupportedSurfaceForSsi)`
   (`crates/yang-rs/src/stage3_ssi.rs:52`), and the Stage-3 refinement loop
   explicitly skips any edge touching a torus
   (`crates/yang-rs/src/stage3_ssi.rs:688`), leaving it as the
   `Curve::LineSegment` mesh fallback. **Stage 4** then relocates that edge's
   vertices onto the exact torus∩surface curve via the implicit-pair /
   implicit-triple Newton projection (`relocate_onto_implicit_pair` /
   `_triple`) — the same machinery that certifies `SurfacePair` points. The
   torus itself survives as `Surface::Torus` through to the tessellator
   (`crates/kernel-v2/src/tessellate.rs:1654`). **Since M5 #172 (2026-07-17)
   this includes Torus–Torus lateral∩lateral**: a torus∩torus edge's second
   torus joins the partner set (base = first torus at the vertex), so the
   degree-8 curve and its torus×torus×plane junctions relocate through the
   same pair/triple Newton (corpus customer R0096; unit oracles
   `newton_relocates_onto_torus_torus_*` in `torus_patch_tests.rs`).
   Coincident tori self-guard via the tangential rank gate (loud STOP).

A native torus SSI solver (adding `QuadricSurface::Torus` and the degree-4
torus curve vocabulary) is a future increment; until then the two mechanisms
above are the production path and there is **no `NotSupported` wall** for the
common (coaxial revolve, KV6d) torus configurations.

---

## F10 — sphere general-position degree-4: CLOSED (2026-07-12)

Design review `docs/review/design_review_2026-07-12_kernel.md` §F10 flagged the
degree-4 representation as inconsistent: the cyl×cyl / cyl×cone / cone×cone
general-position arms returned the M5 `SurfacePair` while the two **sphere**
pairs returned `Err(AnalyticalSolutionNotAvailable)`. **Now closed** (deviations
ledger N37): both sphere NC arms return `SurfacePair` (structured surface first,
sphere second). The end-to-end plumbing needed more than the review assumed —
`quadric_to_surface` (`crates/yang-rs/src/stage3_ssi.rs`) rejected `Sphere`, and
kernel-v2 had **no `PairSurface::Sphere` variant at all** — so F10 also added:
the arena `PairSurface::Sphere` variant, its `pair_surface_residual_gradient`
arm (`f = |x−c|−r`, unit radial gradient), `pair_surface_scale`,
`PairSurfaceKey::Sphere`, and the `yang_surface_to_pair_surface` sphere arm.
`ssi-rs` now has NO staged capability gap; `AnalyticalSolutionNotAvailable`
survives only as the documented absolute-band scale-sensitivity fallback.

---

## Summary

| # | Pair | Status | Analytic sub-cases | Degree-4 general position |
|---|------|--------|--------------------|---------------------------|
| 1 | Plane–Plane | done | Line / empty / degenerate | — |
| 2 | Plane–Cylinder | done | Circle + Ellipse + Lines | — (plane section is a conic) |
| 3 | Plane–Cone | done | Circle + Ellipse + Parabola + 2·Hyperbola + through-apex lines | — |
| 4 | Plane–Sphere | done | Circle | — |
| 5 | Sphere–Sphere | done | Circle | — |
| 6 | Sphere–Cylinder | done-via-SurfacePair | coaxial 0–2 Circle | `SurfacePair` (`:1063`) — F10 |
| 7 | Sphere–Cone | done-via-SurfacePair | coaxial 0–2 Circle | `SurfacePair` (`:1185`) — F10 |
| 8 | Cylinder–Cone | done-via-SurfacePair | coaxial 2 Circle | `SurfacePair` (`:1323`) |
| 9 | Cone–Cone | done-via-SurfacePair | coaxial 0–2 Circle | `SurfacePair` (`:1450`) |
| 10 | Cylinder–Cylinder | done-via-SurfacePair | parallel Lines + equal-R 2 Ellipse | `SurfacePair` (`:1596`) |
| — | Plane–Torus | not-in-crate | — | torus routes above ssi-rs |
| — | Cylinder–Torus | not-in-crate | — | torus routes above ssi-rs |
| — | Cone–Torus | not-in-crate | — | torus routes above ssi-rs |
| — | Sphere–Torus | not-in-crate | — | torus routes above ssi-rs |
| — | Torus–Torus | not-in-crate | — | torus routes above ssi-rs |

**Closed form for every geometrically-real sub-case**: all 10 crate pairs. The
five general-position degree-4 pairs (cyl×cyl, cyl×cone, cone×cone, and — since
F10, 2026-07-12 — sphere×cyl, sphere×cone) reach exactness via the M5
`SurfacePair` procedural curve; there is no remaining staged `Err` gap. Torus
pairs are
handled one tier up (coaxial-rim recovery + Stage-4 implicit relocation), not in
`ssi-rs`.

## References

- [#1] Patrikalakis & Maekawa 2002, *Shape Interrogation for CAD/M*, Ch.5 — exact
  SSI algorithms for quadric pairs (cited per-solver in `lib.rs`).
- [#24] Yang, Jia & Yan 2025 — hybrid B-Rep/mesh boolean; §4.1.2/§4.3 procedural
  implicit intersection curves (the `SurfacePair` representation).
- `specs/m5_surface_pair_curve.md` — the M5 procedural surface-pair curve.
- `governance/ARCHITECTURAL_INVARIANTS.md` A15.1/A15.2/A15.4 — analytical primacy
  invariant + the status table that links here.

---

*Created: Sprint 68, 2026-03-25 (legacy `crates/kernel` matrix).*
*Rewritten: 2026-07-12 — describes the live `crates/ssi-rs` crate; supersedes the
pre-Phase-6 legacy matrix. Torus dropped from the quadric set; `SurfacePair`
(M5) added; sphere general-position gap (F10) called out.*
