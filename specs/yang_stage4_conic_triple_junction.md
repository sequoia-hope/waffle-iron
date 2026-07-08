# Spec: Stage-4 general conic triple-surface junction relocation

> **Status (2026-07-08): DESIGN — prototyped and reverted.** The handler below
> was implemented and validated (0 WRONG, no CORRECT lost across 78/294 assay
> cases; F0059 & R0019 junction vertices relocate cleanly onto all 3 surfaces)
> but it converts ZERO cases alone — resolving the junction advances F0059 to a
> Stage-6 `NonManifoldOutput` T-junction (the N2 CDT gap) and R0019 to a second
> junction. Per P4/DoD (no green reproduction, 0 conversions) it was reverted.
> **This handler MUST land together with the N2 Fig-11 CDT mesh-update**
> (`stage4_update::stage4_mesh_update` + `stage4_dt::d_of_t`, both built &
> unit-tested but unwired) so Stage 6 sees T-junction-free topology. See
> `docs/yang_deviations.md` N2 for the full diagnosis and the combined
> next-increment plan.

## Goal

Retire the dominant Stage-4 `LocalRefinementRequired` error class (12 assay
cases, `target/assay_kv2_report.json`) for the sub-family where a mesh vertex is
the crossing of **two analytical intersection curves** — i.e. an
"over-determined junction". Today such a vertex trips a loud P10 STOP in one of
the no-skip audits (`lib.rs` sites 9801/9817/9832/9849/9867: "a vertex shared by
BOTH a circle and an ellipse edge … loud STOP rather than silently picking
one"). That STOP is conservative but wrong-in-spirit: the vertex's exact
position is *not* ambiguous — it is the unique transversal common point of the
distinct surfaces incident at the vertex.

This is the same problem the **torus block already solves** for degree-4 torus
junctions (`lib.rs` ~10560–10657): aggregate the distinct incident surfaces per
vertex and, for exactly three, relocate onto all three via
`relocate_onto_implicit_triple` (Newton on the 3×3 unit-normal Jacobian). This
spec generalizes that block to **conic** junctions (circle × ellipse,
cone × cone, cone × cylinder, …) so ONE handler covers every curve-type mix,
per §0.1 "general over piecemeal".

## Parameters (inputs)

- `mesh`, `attribution`, `a`, `b` — as `stage4_relocate_and_correct`.
- The per-vertex conic maps already built earlier in the function
  (`vert_circle`, `vert_ellipse`, `vert_cone_ellipse`, `vert_parabola`,
  `vert_cone_hyperbola`, `vert_line`).
- `inc0` — the incidence map `(start,end) → [(InputId, Surface)]` from
  `compute_phase_a`: the source of the DISTINCT incident surfaces per vertex.
- `d_eps` — the Stage-4 chord band from `stage4_chord_band`.

## Branch table

For each vertex present in **≥ 2** of the six single-curve conic maps (the
existing line+circle extraction at `lib.rs:9774` runs first and removes those):

| # distinct incident surfaces (deduped from `inc0`) | Action |
|---|---|
| exactly 3 | **NEW**: transversal triple point → `relocate_onto_implicit_triple`; remove vertex from the six maps and from `endpoints`; move it; add to `moved`. |
| 2 | leave in maps → existing over-determined audit STOPs (unchanged). |
| ≥ 4 | leave in maps → existing over-determined audit STOPs (unchanged). |

In the 3-surface relocation:

| condition | Action |
|---|---|
| Newton diverges / degenerate 3×3 (`relocate_onto_implicit_triple` → `None`) | STOP `LocalRefinementRequired` |
| `surface_value_and_normal` fails at proj | STOP `LocalRefinementRequired` |
| displacement `ρ = |proj−p|` > `2·d_eps / sinθ` (θ = angle between two incident normals at proj) | STOP `OffCurveBeyondChordBand` |
| `ρ ≤ TAU_WORK` | retag only (no move) |
| otherwise | move vertex to proj, add to `moved` |

## Invariants

- **I1 (exactness)**: the relocated vertex satisfies `Fᵢ(proj) = 0` for all
  three surfaces to Newton tolerance (1e-13), hence lies exactly on BOTH
  incident intersection curves.
- **I2 (no silent pick)**: a vertex is relocated only when the incident-surface
  set has exactly 3 members and Newton converges within the derived
  displacement band; every other configuration keeps a loud STOP (P9/P10).
- **I3 (bookkeeping balance)**: resolved vertices are removed from `endpoints`
  and are NOT added to `relocations`/`processed`, so the no-skip audit
  (`processed == endpoint_set == relocation_keys`, `lib.rs:10475`) stays
  balanced — identical treatment to the torus junction block (source stays
  `BRepVertex`; position is exact).
- **I4 (no double relocation)**: removed from the six single-curve maps, so no
  per-curve loop touches the vertex afterward.
- **I5 (determinism)**: `BTreeMap` iteration + structural `Surface` dedup in
  incidence order ⇒ deterministic surface triple.

## Oracles

- **Unit**: a cylinder × cylinder 90° union fixture (F0059 shape) whose Steinmetz
  ellipse meets a cap circle; assert the boolean succeeds and the shared vertex
  lies on all three surfaces to ≤ 1e-10 (was `Err(LocalRefinementRequired)`).
- **Assay** (`assay_kv2`): **0 WRONG preserved**, `supported_correct` strictly
  increases (target cases F0059, R0004, R0017, R0019 and any other 3-surface
  conic junctions), no previously-CORRECT case regresses.

## Failure modes

- Non-transversal triple (parallel/tangent normals): `relocate_onto_implicit_triple`
  returns `None` → `LocalRefinementRequired` (unchanged loudness).
- Off-band displacement (real off-curve error, not a Stage-1 chord artifact):
  `OffCurveBeyondChordBand`.
- ≥ 4 incident surfaces (genuinely over-determined) / 2 surfaces (handled by the
  closed-form ell×ell / disc∩disc / line+circle junctions elsewhere): existing
  STOPs untouched.

## Research Basis

- [#24] Yang et al. 2025 §4.4.1 (mesh updating; relocation breaks bijectivity,
  intersection points relocated onto the exact surfaces),
  `refs/text/yang2025_hybrid_boolean.txt:531–573`.
- [#1] Patrikalakis Ch.5 (surface intersection / Newton projection).
- Reuses the existing `relocate_onto_implicit_triple` primitive and mirrors the
  KV6d Tier-B torus junction block (`lib.rs` ~10560), the disc∩disc / cyl×cyl /
  line+circle junction displacement-band derivation (1/sinθ gradient metric,
  NOT tolerance widening).
