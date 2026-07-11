# KV16 — Hyperbola-arc boundary vocabulary + re-entry (N2 epic increment 6)

**Milestone:** N2/R0017 epic increment 6 (roadmap §N2 trail). The R0017 wall:
`boolean_union failed: UnsupportedBooleanOutputCurve { curve: "Hyperbola" }`.
**Status:** SHIPPED 2026-07-11 (with two Stage-4 root-cause fixes and the
cone-patch EllipseArc extension discovered en route — see below).

## As executed

The vocabulary shipped as designed (all branch-table rows). Three ADDITIONAL
walls fell out of driving R0017 through it, each measured before fixing:

1. **Same-type hyperbola×hyperbola junction (yang Stage 4).** R0017 v47:
   a prism EDGE (two steep side planes of one extrude) pierces a 60° cone
   band — BOTH incident section curves are hyperbolas of the SAME cone, so
   the second `vert_cone_hyperbola.insert` silently overwrote the first
   (the residue named in `yang_stage4_conic_triple_junction`) and the
   increment-5 "≥2 maps" triple trigger never fired; the vertex was
   relocated onto one curve only, leaving the other output edge's endpoint
   ~0.117 off-branch (rejected by the new kernel import certification).
   Fix: detect the conflicting second descriptor at insert time (the
   `vert_ell_junction` precedent) into `same_type_junction`, which the
   triple trigger now honors. Mutation-verified on R0017 (routing off →
   the off-branch reject returns).

2. **Triple-junction Newton divergence at steep cones (yang Stage 4).**
   `relocate_onto_implicit_triple` pairs each surface residual with the
   UNIT normal, but `surface_value_and_normal`'s cone arm returns the
   radial-deviation form `l − |h|·tanα` = true distance × sec α. At
   half-angle 60° (sec α ≈ 2.0007) the Newton step overshoots ~2× and
   bounces for all 32 iterations (R0017 v47 diverged; 30° cones, sec α ≈
   1.15, converged — why the increment-5 fixtures never saw it). Fix:
   rescale the cone residual to the true distance `l·cosα − |h|·sinα`
   inside the triple Newton (the kernel-v2
   `pair_surface_residual_gradient` convention). Mutation-verified on
   R0017. `surface_value_and_normal` itself is UNCHANGED (its band-audit
   consumers rely on the conservative form).

3. **Cone-patch EllipseArc vocabulary (kernel-v2).** With the junction
   fixed, R0017's union output carries an oblique-section ELLIPSE arc on a
   cone patch — the KV6c increment-5 "later slice" typed reject. Extended
   `validate_cone_patch` (endpoint-azimuth walk) and the developable-patch
   tessellation (per-sample wrapped-Δθ walk — the SurfacePair/HyperbolaArc
   mechanism; the cylinder parametric-sweep shortcut stays byte-identical)
   to admit it. `signed_volume`'s cone-patch conic flux stays a typed wall
   (the render mesh carries the assay volume oracle).

**R0017 trail:** `UnsupportedBooleanOutputCurve(Hyperbola)` → off-branch
endpoint (junction) → Newton divergence → cone-patch EllipseArc reject →
**auto-union SUCCEEDS end-to-end**; the case now stops at its op-3 cut's
Stage-3 `AmbiguousCurve { candidates: 0, matched: 0 }` — a distinct,
pre-existing class (R0003/R0008/C0043/C0056 family).

**Tests:** `kernel-v2/tests/kv16_hyperbola_boolean.rs` (union with exact
Simpson-slice volume + vocabulary pin; re-entry chain with exact notch
decrement — both fixture topologies: bottom-rim bite d=1.2 wrapping-loop
patch, through-bite d=0.8 non-wrapping), kernel-v2 geom unit round-trips,
`yang-rs/tests/rim_junction_insertion.rs::same_type_hyperbola_edge_pierce_
endpoints_on_curve` (endpoints-on-own-branch class pin; NOTE: it does not
kill mutation 1 at unit scale — the pierce fixture resolves benignly
through single-curve relocation there; the mutation-killing oracle is
R0017 itself, verified both ways this session).

**Named residue:** the sibling conic maps (`vert_cone_ellipse`,
`vert_parabola`) have the same latent same-type-overwrite trap (no corpus
driver yet — document-only). The d>1 wrapping-loop cone patch cannot
re-enter yang Stage 1 (KV14 Slice E cone periodic strip, typed).

## Goal

Let a boolean output whose boundary carries a **hyperbola-arc** edge (the
axis-steep planar section of a cone — `yang_rs::Curve::Hyperbola`, produced
end-to-end by PR-YR23 since 2026-07-10's junction-wall retirement) assemble
into a kernel-v2 solid, render, validate, and RE-ENTER yang-rs Stage 1 for
chained booleans — instead of the typed `UnsupportedBooleanOutputCurve`
wall at `classify_edge`.

R0017 (`revolve + extrude-boss + extrude-cut`, scale 4e3) is the corpus
driver: its auto-union output carries the cone∩(prism side plane) hyperbola,
and op 3 chains a cut onto that body (re-entry required for CORRECT).

## Research basis

- [#24] Yang et al. 2025 §4.1.2/§4.4.2 — conic section curves are exact
  first-class output curve geometry; the section vocabulary (ellipse /
  parabola / hyperbola) mirrors ssi-rs `SsiCurve` (Patrikalakis Ch.5 [#1]).
- PR-KV9 (`yang_pr_yr11_stage4_oblique_ellipse`, EllipseArc) — the conic
  template this increment follows field-for-field.
- M5 (`m5_surface_pair_curve`) — the endpoint-determined traversal / twin
  bit-identity convention for OPEN curves (no directional normal).
- KV14 (`kv14_ellipse_arc_reentry`) — the two-sided re-entry pattern
  (kernel-v2 `to_yang_brep` conversion + yang Stage-1 chain pre-pass).

## Parameters (curve descriptor)

`Curve::HyperbolaArc` (kernel-v2 arena), mapping **field-for-field** to
`yang_rs::Curve::Hyperbola` and `ssi_rs::SsiCurve::Hyperbola`:

| field | meaning |
|---|---|
| `center: Point3` | hyperbola center (in the section plane) |
| `normal: UnitVector3` | unit section-plane normal |
| `major_axis: UnitVector3` | unit transverse-axis direction (in-plane) |
| `semi_transverse: f64` | `a > 0` |
| `semi_conjugate: f64` | `b > 0` |

Parameterization (the shared convention, `yang_rs::geom::hyperbola_point`):
`P(t) = center + a·cosh t · m̂ + b·sinh t · (n̂ × m̂)` — the single
`+major_axis` branch (`u > 0`).

**Traversal/twin convention (the M5 SurfacePair convention, NOT the
ellipse's CCW-directional one):** one branch of a hyperbola is an OPEN,
injective curve — between two distinct on-branch endpoints the arc is
UNIQUE, so traversal is endpoint-determined and there is no minor-arc
ambiguity and no directional normal. Twins carry BIT-IDENTICAL fields.
(`(n̂, m̂)` and `(−n̂, m̂)` describe the same point set mirrored in `t`;
the assembler simply copies one descriptor to both twins, so no
reconciliation is needed.)

A closed (`start == end`) hyperbola edge is impossible (the branch is
unbounded) — typed reject, no producer constructs one.

## Branch table

| # | site | input | behavior |
|---|---|---|---|
| 1 | `classify_edge` | `Curve::Hyperbola`, `start != end`, endpoints on-branch | `EdgeKind::HyperbolaArc` |
| 2 | `classify_edge` | `start == end` | typed `UnsupportedBooleanOutputCurve("closed hyperbola loop edge…")` |
| 3 | `classify_edge` | non-finite/non-positive `a`/`b` | `InvalidBooleanOutput` |
| 4 | `classify_edge` | endpoint off-branch (`u ≤ 0` or residual > band) | `InvalidBooleanOutput` |
| 5 | twin agreement (from_yang 1c + `curves_twin_consistent`) | bit-identical pairs | pass; anything else typed reject |
| 6 | planar-face winding (`winding_points` + from_yang Newell) | HyperbolaArc in loop | parametric-midpoint bulge sample (the KV9/KV11 mechanism) |
| 7 | `validate_planar_face` debug tier | center on plane, endpoints on branch (import band) | pass/loud |
| 8 | `validate_cone_patch` | HyperbolaArc edge | endpoint-azimuth walk advance (the M5 SurfacePair arm) |
| 9 | `validate_cylinder_patch` | HyperbolaArc edge | typed reject (plane∩cylinder is never a hyperbola) |
| 10 | render tessellation (planar + developable patch) | HyperbolaArc edge | closed-form chord-sag bisection samples, twin-canonical |
| 11 | `introspect::extract_edges` | HyperbolaArc | render-identical sample polyline |
| 12 | `introspect::surface_area`, `geom::signed_volume` cone patch | HyperbolaArc | typed loud (no closed form yet — same wall as EllipseArc there) |
| 13 | `to_yang_brep` (planar `convert_loop` + `convert_lateral_edge`) | HyperbolaArc | yang `Curve::Hyperbola`, twin-shared by `min(h, twin)` |
| 14 | yang Stage-1 chain pre-pass | `Curve::Hyperbola` input edge (`start != end`) | open sample chain `[start, Steiner…, end]` in `rim_rings` |
| 15 | yang Stage-1 | `start == end` hyperbola | loud `MalformedTopology` |
| 16 | yang `loop_polyline` / CDT gates (planar curved CDT, lateral holed CDT) | `Curve::Hyperbola` | admitted exactly like `Curve::Ellipse` arcs |

## Sampling rule (branch 10/14)

Closed-form recursive parameter bisection (the `surface_pair_interior_samples`
shape with exact evaluation instead of Newton): split `[t0, t1]` while the
parametric midpoint's distance to the chord midpoint exceeds `tol`;
depth-capped (2¹²) with typed failure. kernel-v2 render `tol` =
`max(a,b)·(1 − cos(π/n_seg))` — the same circle-step sag contract the
surface-pair sampling uses, at the hyperbola's own scale. yang Stage-1
chain `tol` = `1e-2·max(a,b)` (the KV14 self-contained chord-bound rule at
the conic's scale). Endpoint params via `t = asinh(v/b)` (injective —
no quadrant/branch reconciliation needed).

On-branch residual → length conversion (branches 4/7): with in-plane
coordinates `u = d·m̂`, `v = d·(n̂×m̂)`, `g = (u/a)² − (v/b)² − 1`,
`dist ≈ |g| / (2·hypot(u/a², v/b²))` (first-order distance along the
in-plane gradient), plus the out-of-plane component checked directly; both
at the scale-aware import band `1e-9·(1 + max(a, ‖p‖∞))`.

## Invariants / Oracles

- **Watertight**: shared twin sampling (canonical `min(h,twin)`, reversed
  for the other side) — both incident faces emit identical positions; the
  yang chain is built once per edge and spliced by both faces.
- **On-surface**: every render sample satisfies the branch implicit to the
  bisection tolerance; endpoints validated at the import band.
- **Volume**: E2E frustum ∪ box (axis-parallel side plane cutting the
  lateral) has slice-integrable exact overlap
  `∫ A_seg(z) dz`, `A_seg = r²·acos(d/r) − d·√(r²−d)²` — render-mesh
  signed volume within 1% of analytic.
- **Vocabulary pin**: the E2E intermediate must CARRY ≥2 HyperbolaArc
  half-edges (else the test pins nothing — the KV14 lesson).
- **Re-entry**: a second boolean on the hyperbola-bounded body succeeds
  with an exact far-notch volume decrement (the KV14 notch oracle).
- **Assay**: 0 WRONG, zero-lost (no CORRECT regresses); R0017 advances
  out of `UnsupportedBooleanOutputCurve(Hyperbola)`.

## Failure modes

- Closed hyperbola loop: typed (branches 2/15).
- Endpoint off-branch / wrong nappe: typed `InvalidBooleanOutput` (the
  producers emit on-branch endpoints; `u ≤ 0` means a wrong-nappe defect).
- Bisection depth exhausted: typed `TessellationFailed` (never a silent
  chord fallback, P9).
- Cylinder-patch placement: typed (branch 9) — no producer exists.
- Analytic `surface_area`/`signed_volume` on hyperbola-bounded cone
  patches: typed loud, same as the EllipseArc precedent (branch 12); the
  assay volume oracle uses the render mesh and is unaffected.
