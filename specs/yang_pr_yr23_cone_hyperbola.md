# PR-YR23 — cone∩plane HYPERBOLA end-to-end + two-branch selection (closes out cone)

**Crate:** `crates/yang-rs/`
**Stage:** Yang 2025 §4.4.1 (mesh updating / relocation) + §4.3.2 (parametric
surface relocation) — the hyperbola sibling of PR-YR21 (cone ellipse) and
PR-YR22 (cone parabola).
**Roadmap:** last proper-conic step of the cone analytic-conic sequence
PR-YR21→YR24 (`docs/yang_functional_roadmap.md`).

## 1. Problem

After PR-YR22 cone is `5/26` in the curved fuzz: 5 ellipse cases work; the ~21
remaining cone `AmbiguousCurve` cases are (near-)all **hyperbola** — a random
box cut of a cone almost always pierces both nappes' symmetry-plane generators
with opposite signs (`plane_cone` HYPE case). Unlike the parabola (measure-zero,
θ=α exactly), the hyperbola is the conic that **actually moves the fuzz number**:
wiring it through should raise cone `ok_correct` from 5 toward ~26.

The analytic math is **already DONE** in `ssi-rs` (PR-SSI4): `plane_cone` returns
the HYPE case as **two** `SsiCurve::Hyperbola` candidates (one per nappe, opposite
`major_axis`, `+m̂` first). `yang-rs` currently rejects hyperbola loudly
(`ssi_curve_to_curve` → `Err(UnsupportedCurve)`; `curve_contains_point` → `false`).
This PR is purely a `yang-rs` integration: wire the existing type-agnostic Stage-4
cone-section relocation (`project_onto_cone_section`, YR21) through to the
hyperbola, plus the one genuinely-new mechanism below.

## 2. The new mechanism — two-branch selection (load-bearing)

`ssi_rs::intersect(Plane, Cone)` returns **2** `Hyperbola` for the HYPE case — the
**first** legitimate matched-among-multiple case in `build_intersection_curves`
(`lib.rs:2962-2981`). The matched loop already counts how many candidates contain
both endpoints and requires `matched == 1`; the discrimination falls out once
`curve_contains_point` distinguishes the two branches:

- The YR18 on-both-surfaces gate (`signed_distance_to_surface`) passes for **both**
  branches (both lie on cone∩plane). So `curve_contains_point` is the branch
  discriminator: for the edge's branch, endpoints satisfy `(u/a)² − (v/b)² = 1`
  with **`u > 0`** where `u = (p − center)·major_axis`. For the OTHER branch's
  frame (opposite `major_axis`) the same point gives `u < 0` → rejected.
- `matched` MUST end exactly **1**. `matched == 2` (or 0) stays a LOUD
  `SsiRefinementError::AmbiguousCurve` — for a real single-nappe edge it must not
  happen. If it does → STOP and report; do **not** pick arbitrarily.

No structural change to `build_intersection_curves` is needed — the two-branch
selection is purely a consequence of the new `curve_contains_point` Hyperbola arm.

## 3. Parameterization (mirror byte-for-byte; confirmed against `ssi-rs`)

ssi-rs `SsiCurve::Hyperbola { center, normal, major_axis, semi_transverse a,
semi_conjugate b }` traces the single `+major_axis` branch (`lib.rs:293-311`):

```
eval(t) = center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis),  t ∈ ℝ
```

- **`hyperbola_point(center, normal, major_axis, a, b, t)`** (new public helper):
  reproduce the eval above, with the conjugate in-plane direction
  `normal × major_axis` (unit since both are unit and orthogonal). Mirror
  `parabola_point` byte-for-byte in structure.
- **Stage-4 relocation:** project the endpoint via the **unchanged**
  `project_onto_cone_section` (relocates `p` onto the correct nappe automatically
  from `p`), then tag `t = asinh(v / b)` where
  `v = (relocated − center)·(normal × major_axis)` (well-defined ∀ v — `sinh` is
  bijective over ℝ). Residual gated by the cone chord band
  (`cone_plane_residual` ≤ `cone_d_eps`), identical to YR22.
- **`curve_contains_point` membership:** out-of-plane reject first
  (`|(p − center)·normal| ≤ tol`), then the in-plane relation
  `(u/a)² − (v/b)² = 1` AND `u > 0`. The implicit residual `(u/a)² − (v/b)² − 1`
  is **dimensionless**; convert it to a geometric (length) residual the same way
  the Ellipse/Parabola arms do (residual → length via the in-plane gradient /
  the `min(a, b)` scaling), then compare against the cone chord band `tol`.

## 4. P9/P10 guard rails (for the GREEN implementer)

- The membership in-plane metric is dimensionless `(u/a)² − (v/b)²`. If the chord
  band needs the PR-YR19 **propagated-band** reasoning (surface-normal `d_ε`
  amplified into the in-plane radial metric), apply it as a *justified* derived
  band — **NOT** a flat widening. If a flat widening is the only way to pass,
  **STOP and report** (do not hack to green).
- **No new relocation method.** Reuse the YR21 projector
  (`project_onto_cone_section`) + budget helpers (`cone_plane_residual`,
  `cone_chord_budget_from_owner`) unchanged.
- Axis-parallel / through-apex sections stay LOUD via the existing YR21 guards in
  `project_onto_cone_section` (`n_dot_g ≈ 0` and `s ≤ 0` → `LocalRefinementRequired`).

## 5. Anchors in `crates/yang-rs/src/lib.rs` (mirror YR21/YR22)

| Concern | Location | Action |
|---|---|---|
| `Curve` enum (LineSegment/Circle/Ellipse/Parabola) | ~156–189 | + `Hyperbola { center, normal, major_axis, semi_transverse, semi_conjugate }` |
| `eval_source` curve match | ~840–890 | + `Curve::Hyperbola` arm → `hyperbola_point(...)` |
| `hyperbola_point` helper (new, public) | after `parabola_point` ~1085 | mirror `parabola_point` |
| `ConeHyperbolaReloc` struct (new) | after `ConeParabolaReloc` ~2237 | center/normal/major_axis/a/b + cone+plane + `cone_d_eps` |
| `project_onto_cone_section` | ~2254 | **REUSE unchanged** |
| `cone_plane_residual` / `cone_chord_budget_from_owner` | ~2331 / ~2378 | **REUSE unchanged** |
| `ssi_curve_to_curve` (Hyperbola → `Err`) | ~2622 | + Hyperbola arm → field-for-field `Ok(Curve::Hyperbola)` |
| `curve_contains_point` (Hyperbola → `false`) | ~2749 | + Hyperbola arm (membership + `u>0` discriminator) |
| `build_intersection_curves` matched loop | ~2962–2981 | **no structural change** (two-branch selection falls out) |
| Stage-4 reloc collection `match *curve` | ~3879 | + `Curve::Hyperbola` arm (cone+plane incidence → populate `vert_cone_hyperbola`) |
| Stage-4 ambiguity audit (`vert_parabola.keys()`) | ~4104 | + `vert_cone_hyperbola` cross-checks |
| Stage-4 reloc loop (`vert_parabola`) | ~4228 | + `vert_cone_hyperbola` loop (`asinh(v/b)` tag) |
| `is_reversed` `match conic` | ~4470 | + `Curve::Hyperbola` arm (defensive tangent; never reached — open-arc excluded from `all_conic`) |
| conic-detection sites | ~4698, ~4989 | + `Curve::Hyperbola { .. }` so a hyperbola edge enters Stage 4 |

ssi-rs needs **NO change** (hyperbola already shipped, PR-SSI4).

## 6. RED contract (test-author, `tests/yr23_cone_hyperbola.rs`)

Mirror `tests/yr22_cone_parabola.rs` structure **verbatim**, retargeted to a
deterministic **hyperbola** fixture (a cutting plane whose symmetry-plane
generators pierce OPPOSITE nappes — `plane_cone` HYPE case). Sample ONE branch's
arc on the solid nappe, close into a watertight ring exactly as YR22 does.

Oracles:
1. independent `ssi_rs::intersect` oracle: returns **2** `Hyperbola` (NOT
   Ellipse/Parabola) for the fixture's plane+cone.
2. `Ok` with a `Curve::Hyperbola` edge; stored fields match the ssi-rs branch.
3. relocated vertices on the exact hyperbola (`(u/a)² − (v/b)² = 1`, `u > 0`) AND
   on both cone+plane to `TAU_MODEL`; chord deviation strictly decreases, ends
   ≤ `TAU_MODEL`.
4. `eval_source(point→t)` round-trip via `hyperbola_point` (`asinh` tag) ≤ `TAU_MODEL`.
5. **two-branch selection oracle:** the OTHER branch's candidate is rejected →
   `matched == 1` (assert exactly one branch passes both endpoints).
6. **oracle4 = the YR22 reframe invariant:** watertight (0 unpaired) + χ=2 +
   signed-volume > 0 + per-facet degenerate-area. **Do NOT** reintroduce the
   per-triangle winding-vs-analytic-normal check (false-positives on the ring
   scaffold — the very bug YR22 reframed).
7. **out-of-scope LOUD fixture:** a genuinely-ambiguous / axis-parallel /
   through-apex case stays `Err` (e.g.
   `Stage4RegionInvalid{LocalRefinementRequired}` or `AmbiguousCurve`).
8. optional env-gated real-sidecar E2E (mirror yr22 oracle8).

RED must compile-fail/red against current production.

## 7. STOP conditions (P9/P10)

- A flat tolerance widening is the only way to pass membership → STOP & report.
- The two-branch selection yields `matched != 1` on a real single-nappe edge →
  STOP & report (do not pick arbitrarily).
- The plan's diagnosis is wrong (e.g. ssi-rs does not actually return 2 Hyperbola
  for the fixture) → STOP & report; do not improvise.

## 8. Scope / non-regression

Cone∩plane HYPERBOLA only. Must NOT regress (byte-for-byte): cone ellipse (YR21) /
parabola (YR22), cylinder ellipse (YR11), circle (YR17), sphere, all planar,
YR8–YR22 demos, `fuzz_boxes` 900/900. Planar path stays byte-identical (Stage-4
early-returns when no conic edge exists).

## 9. Calibrated metric

Cone `AmbiguousCurve` → ~0; cone `ok_correct` rises materially (toward ~26); ZERO
new silent-wrong. If the in-container curved fuzz can't complete (sidecar-zombie
blocker, `curved_fuzz_sidecar_zombie_blocker`), stand on the unit oracles + the
real-sidecar E2E — never fabricate fuzz numbers.
