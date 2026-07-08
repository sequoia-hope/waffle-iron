# M5 — Procedural Surface-Pair Curve (general degree-4 SSI, Option B)

**Status**: Implementation spec (governance gate CLEARED by user 2026-07-08 —
Option B; Constitution P8 degree-4 clarification merged at 35d2332f)
**References**: [#24] Yang et al. 2025 §4.1.2, §4.3, §4.4.1 (d_p = 1e-7);
[#1] Patrikalakis Ch.5 (degree-4 nature of quadric-pair intersections)
**Governance**: P8 (degree-4 procedural-curve clarification), A15.1
**Supersedes**: `specs/cyl_cyl_unequal_r_ssi.md`'s `Degree4CylCyl`
θ-parameterized representation (its branch table B1–B5 remains the case
analysis; the closed-form Levin-style variant is explicitly NOT built — the
paper is the spec and the paper carries refined point sequences, each point
Newton-projected onto BOTH surfaces).

---

## Goal

Represent general-position quadric-pair intersection curves (first producer:
unequal-radius / skew cylinder×cylinder) exactly and procedurally: the curve
is defined implicitly by its TWO analytic surfaces; every concrete point is
certified by Newton projection onto both surfaces (residual ≤ band). This
lifts the `AnalyticalSolutionNotAvailable` stop for general cyl×cyl
(ssi-rs lib.rs:1545) and gives kernel-v2 an output vocabulary for the
surviving quartic edge (the tunnel-mouth rim of C0052/C0053/C0054;
R0019/R0044 stops).

The curve arrives in kernel-v2 the same way circles/ellipses do: yang tags
per-mesh-edge output `BRepEdge`s with the shared analytic curve descriptor,
so a quartic loop is a chain of short surface-pair edges whose endpoints are
the certified point sequence (relocated by Stage 4). No point array is
carried on any curve variant — consecutive output vertices ARE the samples.

## Parameters (the new vocabulary, per crate)

| Crate | New variant | Fields |
|---|---|---|
| ssi-rs | `SsiCurve::SurfacePair` | `a: QuadricSurface, b: QuadricSurface` (Copy) |
| yang-rs | `Curve::SurfacePair` | `a: Surface, b: Surface` (yang's unsigned Copy surface descriptors) |
| kernel-v2 | `arena::Curve::SurfacePair` | `a: PairSurface, b: PairSurface` — Copy `PairSurface` enum in arena.rs (`#[non_exhaustive]`): `Cylinder { axis_point, axis_dir, radius }` (cyl×cyl) and `Cone { apex, axis_dir, half_angle }` (cone-pair producer, landed 2026-07-08) |

No `reversed`/cavity flags anywhere on the pair descriptors — the curve is a
point set; orientation comes from the half-edge traversal (origin → dest),
twins share IDENTICAL `a`, `b` (like `EllipseArc` twins share `major_axis`).

Ordering: `a`/`b` order is the ssi call's argument order, preserved verbatim
through yang → kernel-v2. Twin/pairing comparisons use exact equality on the
ordered pair (no canonicalization needed — both twins are minted from the
same descriptor).

## Branch table

### ssi-rs `cylinder_cylinder` (existing arms unchanged unless noted)

| # | Condition | Output |
|---|---|---|
| S1 | non-parallel, equal-R, coplanar axes | dual ellipses (existing, unchanged) |
| S2 | non-parallel, unequal-R (any coplanarity) | `vec![SurfacePair{a,b}]` (NEW — was ASNA) |
| S3 | non-parallel, equal-R, skew axes | `vec![SurfacePair{a,b}]` (NEW — was ASNA) |
| S4 | parallel arms (coincident/concentric/disjoint/tangent/secant) | existing, unchanged |
| S5 | degenerate inputs (E1) | `DegenerateInput` (existing, unchanged) |

ONE descriptor for the whole intersection set: branch/component separation is
topological (mesh connectivity of the point-sequence chains), not part of the
curve descriptor — per the paper, the curve is the zero set of both surfaces.
Disjoint-surface configurations return the descriptor too; Stage-3 membership
simply never matches it (no intersection mesh edges exist for that pair).

### yang-rs

| # | Site | Behavior |
|---|---|---|
| Y1 | `ssi_curve_to_curve` | `SsiCurve::SurfacePair` → `Curve::SurfacePair` (QuadricSurface → yang Surface, field-for-field; `Plane` operand → unreachable for the cyl×cyl producer, typed error if hit) |
| Y2 | `curve_contains_point` | max(residual to `a`, residual to `b`) ≤ tol |
| Y3 | Stage-3 tangent tie-break | tangent at p = normalize(n̂_a × n̂_b); parallel normals (tangency) → no tangent, candidate not tie-breakable (existing AmbiguousCurve stop stays loud) |
| Y4 | Stage-4 relocation | `relocate_onto_implicit_pair(p, a, b)` (EXISTING, tested — lib.rs:4552); `None` → existing loud `SsiRefinementFailed` path |
| Y5 | output emission | per-mesh-edge `BRepEdge` tagged `Curve::SurfacePair` exactly like circles/ellipses (`intersection_curves` map) |

### kernel-v2

| # | Site | Behavior |
|---|---|---|
| K1 | `classify_edge` (from_yang) | yang `Curve::SurfacePair` with both operands Cylinder → `EdgeKind::SurfacePair`; any non-Cylinder operand → `UnsupportedBooleanOutputCurve` (typed, until PairSurface grows) |
| K2 | `classify_edge`, `start == end` | `UnsupportedBooleanOutputCurve` ("closed surface-pair loop edge — no producer") |
| K3 | `classify_edge` endpoint check | each endpoint's residual to BOTH surfaces ≤ import band (1e-9 scaled, same shape as circle/ellipse bands) |
| K4 | `CurveKey` | new arm keyed on the ordered pair bits (distinct quartics on the same vertex pair pair separately) |
| K5 | `validate_solid` twin check | `curves_twin_consistent`: exact equality of `a`, `b` on both twins |
| K6 | `validate_solid` closed check | SurfacePair half-edge with origin == dest → `CurveTwinMismatch` (like Arc/EllipseArc) |
| K7 | `validate_solid` endpoint residual | per-point on-BOTH-surfaces residual ≤ validate band (the memory contract: validate = per-point residual) |
| K8 | `validate_solid` planar-face loop | SurfacePair edge on a `Plane` face → invalid (a transversal quadric-pair curve is never planar; degenerate configs produce conics upstream) |
| K9 | tessellate: edge samples | `surface_pair_interior_samples`: recursive chord midpoint → Newton projection onto both surfaces (Gauss-Newton, [#24] §4.3), split while sag > chord bound, depth-capped; non-convergence → typed error (loud, no chord fallback) |
| K10 | tessellate: cylinder patch boundary | SurfacePair boundary edges enter the unroll via their K9 samples (same role as `arc_interior_samples`) |
| K11 | re-entry `to_yang_brep` | any SurfacePair edge → `UnsupportedCurvedBoolean { face }` (typed re-entry wall, same as EllipseArc — chained booleans on quartic-bounded bodies are a later milestone) |

## Invariants

1. **On-both-surfaces**: every SurfacePair edge endpoint (and every render
   sample) satisfies `residual_a(p) ≤ band ∧ residual_b(p) ≤ band` where
   `residual` is the surface's implicit distance (|dist(p, axis) − r| for a
   cylinder) and band = 1e-9·(1 + max coordinate/radius magnitude) at import,
   matching the circle/ellipse import bands.
2. **Twin identity**: twins carry bit-identical `a`, `b`.
3. **Transversality at samples**: Newton projection succeeds only where
   surface normals are non-parallel; tangency points fail LOUD (upstream
   Stage-4 relocation already stops there — KV9-F1 class).
4. **No point arrays**: the certified sequence is the output vertex chain;
   `Curve` stays `Copy`.

## Oracles

| Oracle | Method |
|---|---|
| ssi S2/S3 descriptor | returned `SurfacePair` operands == inputs verbatim |
| membership | points constructed on both surfaces pass Y2 at tol; points off either surface by 10× tol fail |
| relocation | perturbed near-curve points relocate onto both surfaces (residuals ≤ 1e-13, existing `relocate_onto_implicit_pair` contract) |
| kernel validate | hand-built lens prism (two secant PARALLEL equal-R cylinders; tip edges are genuine SurfacePair lines — both residuals 0) passes `validate_solid`; mutated twin descriptor / off-surface endpoint / closed edge / planar-face placement each fail with the typed error |
| tessellation | K9 samples of a perpendicular unequal-R pair edge lie on both surfaces within band and satisfy the chord bound; lens prism tessellates with finite, NaN-free mesh |
| corpus (end-to-end) | C0052 (perp unequal-R), C0053 (45°), C0054 (skew) move ERROR → SUPPORTED_CORRECT with exact-volume oracles; R0019/R0044 stops lift; zero WRONG introduced |

## Failure modes

| Condition | Behavior |
|---|---|
| tangent pair (parallel normals at contact) | Stage-4 relocation `None` → existing loud `SsiRefinementFailed`; K9 sampling non-convergence → typed tessellation error |
| non-Cylinder operand reaching K1 | typed `UnsupportedBooleanOutputCurve` |
| closed single-edge quartic loop | K2/K6 typed rejection (no producer constructs them) |
| chained boolean on quartic-bounded body | K11 `UnsupportedCurvedBoolean` (typed wall, roadmap item) |
| near-half ambiguity | N/A — no minor-arc derivation exists for SurfacePair (no normal to flip); traversal is endpoint-determined |

## Research basis

- [#24] Yang et al. 2025: §4.1.2 (intersection curves as refined point
  sequences), §4.3 (Newton projection onto both surfaces / local refinement),
  §4.4.1 (mesh updating with relocated = certified vertices), output
  tolerance d_p = 1e-7. The procedural surface-pair curve is the paper's
  representation; Levin pencil parameterization is deliberately avoided.
- [#1] Patrikalakis Ch.5: degree-4 algebraic nature of the general quadric
  pair (why no conic closed form exists here).
- Constitution P8 "Degree-4 procedural-curve clarification" (2026-07-08):
  a procedural curve whose defining surfaces are exact IS an analytical
  representation.

## Increments (P7)

1. **kernel-v2 vocabulary (gating)**: `PairSurface` + `Curve::SurfacePair`,
   K5–K11. Tests: hand-built lens-prism fixture + direct sampler tests.
   No producer yet — compile-time exhaustiveness sweeps every match site.
2. **ssi-rs**: S2/S3 arms return the descriptor (was ASNA).
3. **yang-rs**: Y1–Y5 plumbing (membership, tangent, relocation dispatch,
   emission).
4. **kernel-v2 from_yang**: K1–K4 acceptance; corpus measure (C0052/53/54,
   R0019/R0044) + assay gate.
5. **Cone-pair producer (landed 2026-07-08).** Extends the SurfacePair
   vocabulary from cyl×cyl to the cone-pair arms (cyl×cone, cone×cone):
   - ssi-rs: `cylinder_cone`/`cone_cone` NC (non-coaxial) arms return
     `SurfacePair` instead of ASNA (ssi8/ssi9 NC tests + adversary gate-boundary
     assertions flipped RED→GREEN; cylinder-first canonical order for cyl×cone,
     argument order preserved for the symmetric cone×cone).
   - yang-rs: `quadric_to_surface` Cone arm; Y2 membership + Y3 tangent
     generalized to Cone operands (`radial − |h|·tanα` residual; unit gradient
     `cosα·r̂ − sign(h)·sinα·â`). Y4 relocation already generic
     (`relocate_onto_implicit_pair`/`surface_value_and_normal` handle Cone).
   - kernel-v2: `PairSurface::Cone`; `pair_surface_residual_gradient` uses the
     TRUE signed-distance form `radial·cosα − |h|·sinα` (unit gradient ⇒ the
     shared Gauss-Newton step is exact, matching the cylinder's radial form);
     `pair_surface_scale` returns 0 for a cone (no constant radius; the point's
     own magnitude tracks local scale); `PairSurfaceKey`/`pair_surface_key` and
     `yang_surface_to_pair_surface` Cone arms.
   - Unit tests: ssi8/ssi9 flips, yang `m5_cone_pair_*` (Y1/Y2/Y3/Y4), kernel-v2
     `surface_pair_sampler_cone_cylinder`. All green; no regression on
     C0052/R0002. **Corpus note:** the R0008/R0003/R0019 cases previously
     tagged "cone-pair AmbiguousCurve class" were re-measured and are NOT
     cone-pair NC edges — R0003/R0008 are cone∩PLANE conic-selection walls
     (ellipse chord-band / apex-crossing generator-line pair) and R0019 is a
     Stage-4 `LocalRefinementRequired` (N2) wall — all orthogonal to the
     producer. The cone-pair NC producer ships as capability closure (per the
     roadmap "a faithful stage that lights up zero corpus cases still ships");
     a corpus case that exercises it end-to-end is not yet identified.

*Created: 2026-07-08*
