# PR-YR23 — yang-rs: cone∩plane HYPERBOLA end-to-end (closes out cone; the fuzz-moving conic)

Context: PR-YR21 shipped the type-agnostic Stage-4 cone-section relocation
(`project_onto_cone_section`) + cone ELLIPSE; PR-YR22 added cone PARABOLA (correct
but fuzz-invisible — exact parabola is measure-zero in the random fuzz). The **21
remaining cone `AmbiguousCurve` in the curved fuzz are (near-)all HYPERBOLA** — so
unlike YR22, this is the cone conic that **actually moves the fuzz number** (cone
`ok_correct` should rise from 5 toward ~26). The analytic math is DONE in `ssi-rs`
(`plane_cone` returns `SsiCurve::Hyperbola` for the HYPE case). This is the final
conic; it adds the one genuinely new mechanism: **two-branch selection.**

## The exact `ssi-rs` shape to MIRROR (frame consistency is load-bearing)

```
SsiCurve::Hyperbola { center: Point3, normal: Vector3, major_axis: Vector3,
                      semi_transverse: f64 /*a*/, semi_conjugate: f64 /*b*/ }
```
- `center` = midpoint of the two branch vertices (in the plane); `normal` = unit
  plane normal; `major_axis` = unit transverse axis, **THIS branch opens toward
  `+major_axis`**; `a` = semi-transverse (center→vertex), `b` = semi-conjugate.
  Conjugate in-plane direction = `normal × major_axis`.
- **Parameterization (MUST be mirrored byte-for-byte):**
  `eval(t) = center + (a·cosh t)·major_axis + (b·sinh t)·(normal × major_axis)`,
  `t ∈ ℝ`, tracing the single branch toward `+major_axis`.
- **`plane_cone` returns TWO `Hyperbola` curves** (one per nappe of the infinite
  double cone), with **opposite `major_axis`**. A solid (single-nappe) cone's
  section lies on EXACTLY ONE branch.

## The new mechanism — two-branch selection

`ssi_rs::intersect` now legitimately returns **2 candidates** for a cone∩plane
hyperbola edge. (This is the FIRST genuine matched-among-multiple case — the
PR-YR18 "matched≥2 = 0" finding predates hyperbola support; it is now expected.)
The intersection edge lies on ONE branch (the nappe matching
`sign((p − apex)·â)`). The YR18 on-both-surfaces gate passes for both branches
(both are on cone∩plane mathematically), so **`curve_contains_point` is the branch
discriminator**: the edge's endpoints satisfy `(u/a)² − (v/b)² = 1` with `u > 0`
(on the `+major_axis` branch) for THEIR branch, and `u < 0` for the other branch's
frame → rejected. `matched` MUST be exactly 1 (the two branches sit on opposite
nappes, geometrically separated). `matched == 2` would be a genuine ambiguity →
LOUD `AmbiguousCurve` (must not happen for a real single-nappe edge; if it does,
STOP and report — do not pick arbitrarily).

## What to build (full RED→GREEN→Adversary cycle)

1. **`Curve::Hyperbola { center, normal, major_axis, semi_transverse, semi_conjugate }`**
   — mirror `SsiCurve::Hyperbola` field-for-field.
2. **`ssi_curve_to_curve`** Hyperbola arm → `Ok(Curve::Hyperbola{..})` (today
   `Err(UnsupportedCurve)`).
3. **`curve_contains_point` Hyperbola arm** (today `false`): out-of-plane
   `|(p−center)·normal| ≤ tol`; `u = (p−center)·major_axis`,
   `v = (p−center)·(normal×major_axis)`; membership `|(u/a)² − (v/b)² − 1|` within
   the chord band AND `u > 0` (THIS branch — the discriminator). Apply the PR-YR19
   propagated-band reasoning if the in-plane metric mismatch bites (justified, NOT
   a flat widening, P9/P10).
4. **`eval_source` Hyperbola arm** = `hyperbola_point(t)` with the mirrored
   `center + (a·cosh t)·major_axis + (b·sinh t)·(normal×major_axis)`.
5. **Stage-4 relocation Hyperbola arm**: a `Curve::Hyperbola` edge with
   `Surface::Cone` + `Surface::Plane` incidence relocates each endpoint via the
   YR21 `project_onto_cone_section` (type-agnostic; relocates to the correct nappe
   automatically from `p`), tagging `t = asinh(v/b)` of the relocated point
   (`v = (relocated−center)·(normal×major_axis)`; well-defined ∀ v). Residual gated
   by the cone chord band. Reuse the YR21 projector + budget helpers; add NO new
   relocation method.
6. **Two-branch selection** in `build_intersection_curves`: when ssi returns 2
   `Hyperbola` candidates, the `matched`/`curve_contains_point` loop selects the
   branch (per §"new mechanism" above). `matched != 1` stays LOUD.

## Scope / non-regression

- **Cone∩plane HYPERBOLA.** Do NOT regress: cone ellipse (YR21) / parabola (YR22),
  cylinder ellipse (YR11), circle (YR17), sphere, all planar, YR8–YR22 demos —
  byte-for-byte. Axis-parallel / through-apex sections stay LOUD (YR21 guards).

## RED contract

A deterministic fixture (NO `rand` / system time / FS): a cone + a cutting plane
oriented so the section is a proper **hyperbola** (the symmetry-plane generators
pierce OPPOSITE nappes — `plane_cone`'s HYPE case; confirm via `ssi_rs` that it
returns `Hyperbola`, NOT Ellipse/Parabola), currently failing with
`AmbiguousCurve`. Sample one branch's arc on the cone solid's nappe and close it
into a watertight ring **the way YR22's reframed fixture does** — and assert
`oracle4` as the **watertight (0 unpaired) + χ=2 + signed-volume>0 + per-facet
degenerate-area** invariant (the PR-YR22 reframe; do NOT reintroduce the
per-triangle winding-vs-analytic-normal check — it false-positives on ring-closure
scaffold). RED asserts: `Ok` with a `Curve::Hyperbola` edge; relocated vertices on
the exact hyperbola (`(u/a)²−(v/b)²=1`, `u>0`) AND on both cone+plane to
`TAU_MODEL`; the `eval_source(point→t)` round-trip; **a two-branch selection
oracle** (the OTHER branch's candidate is correctly rejected → `matched==1`); plus
an **out-of-scope** fixture (the genuinely-ambiguous or axis-parallel case) that
stays LOUD. RED author ≠ GREEN author ≠ Adversary.

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate green — the hyperbola oracles pass; all prior
unregressed, esp. YR21/YR22 cone conics + the YR22 reframed oracle4 pattern),
`cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs --all-targets --
-D warnings`. **Calibrated metric (this one DOES move the fuzz):** cone
`AmbiguousCurve` → ~0 and cone `ok_correct` rises materially (toward ~26),
**ZERO new silent-wrong**. Driver verifies the curved-fuzz delta
(`curved_fuzz_sidecar_zombie_blocker` — the worker may not complete the in-container
fuzz; stand on the unit oracles + a real-sidecar E2E and do NOT fabricate numbers).

Sidecar:
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (PR-YR23 DONE — hyperbola
+ two-branch selection; cone closed out modulo PR-YR24 residual triage) and
`docs/yang_deviations.md` if warranted. After YR23, the `prim∪/−box` curved fuzz
is essentially mined out — flag that for the driver.
