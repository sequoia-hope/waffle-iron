# PR-YR22 — yang-rs: cone∩plane PARABOLA end-to-end

Context: PR-YR21 shipped the type-agnostic Stage-4 cone-section relocation
(`project_onto_cone_section`) and landed cone ELLIPSE (cone fuzz 0→5/26). The
**21 remaining cone refusals are all `AmbiguousCurve` = parabola + hyperbola**
(driver Step-0 split). This PR adds **parabola** (the single-candidate conic;
hyperbola with its two branches is PR-YR23). The analytic math is DONE in
`ssi-rs` (`plane_cone` returns `SsiCurve::Parabola` for the PARA case — exactly
one symmetry-plane generator parallel to the cutting plane). The relocation
foundation from YR21 is reused unchanged; this is curve-representation +
membership + eval + the Stage-4 parabola arm.

## The exact `ssi-rs` shape to MIRROR (frame consistency is load-bearing)

```
SsiCurve::Parabola { vertex: Point3, normal: Vector3, axis_dir: Vector3, focal_length: f64 }
```
- `vertex` = turning point (on the cone AND in the plane); `normal` = unit plane
  normal; `axis_dir` = unit in-plane symmetry axis (opens toward `+axis_dir`);
  `focal_length f > 0` (`y² = 4f·x`). Conjugate in-plane direction = `normal × axis_dir`.
- **`ssi-rs` parameterization (MUST be mirrored byte-for-byte):**
  `eval(t) = vertex + (t²/(4·focal_length))·axis_dir + t·(normal × axis_dir)`
  (lib.rs SsiCurve::eval). yang's `parabola_point(t)` MUST use this identical
  formula so a relocated vertex tagged `BRepEdge { edge, t }` round-trips exactly.

## What to build

1. **`Curve::Parabola { vertex, normal, axis_dir, focal_length }`** — mirror
   `SsiCurve::Parabola` field-for-field (as `Curve::Circle`/`Ellipse` mirror theirs).
2. **`ssi_curve_to_curve`** — map `SsiCurve::Parabola` → `Ok(Curve::Parabola{..})`
   (currently `Err(UnsupportedCurve)`).
3. **`curve_contains_point` Parabola arm** (currently `false`): membership residual
   — out-of-plane component (`·normal`) within `tol`, and the in-plane point
   satisfies `y² = 4f·x` to the chord band, where `x = (p−vertex)·axis_dir`,
   `y = (p−vertex)·(normal×axis_dir)`. Derive the band consistently with the
   Stage-1 chord error; **if the in-plane-vs-surface-normal metric mismatch bites
   (cf. PR-YR19 N11), apply the SAME propagated-band reasoning — justified, NOT a
   flat widening (P9/P10).** Parabola is **single-candidate** (`plane_cone` returns
   one `Parabola`), so `matched == 1`; no two-branch logic (that is YR23).
4. **`eval_source` Parabola arm** = `parabola_point(t)` with the mirrored formula above.
5. **Stage-4 relocation Parabola arm**: a `Curve::Parabola` edge whose incidence is
   a `Surface::Cone` + `Surface::Plane` (scan `inc0` as the Ellipse arm does)
   relocates each endpoint via the YR21 `project_onto_cone_section`, then tags
   `t` = the parabola parameter of the relocated 3D point — i.e. `t = (relocated −
   vertex)·(normal × axis_dir)` (the conjugate-axis coordinate; matches the
   parameterization). Residual gated by the cone chord band as in YR21. Reuse the
   YR21 projector + budget helpers; do NOT add a second relocation method.

## Scope

- **Cone∩plane PARABOLA only.** `Curve::Hyperbola` (two-branch selection) is
  PR-YR23 and OUT of scope — hyperbola cone cuts stay LOUD (`AmbiguousCurve`),
  which is correct for this PR.
- **Do NOT regress:** circle (YR17), ellipse (cone YR21 / cylinder YR11),
  sphere, all planar, and YR8–YR21 demos byte-for-byte. Axis-parallel /
  through-apex / asymptotic sections stay LOUD (the YR21 guards).

## RED contract

A deterministic fixture (NO `rand`, NO system time, NO FS side effects): a cone +
a cutting plane oriented so the section is a proper **parabola** — exactly one
symmetry-plane generator parallel to the plane (`plane_cone`'s PARA case) —
currently failing with `SsiRefinementFailed::AmbiguousCurve`. The RED test
asserts the correct post-fix behaviour: the boolean returns `Ok` with a
`Curve::Parabola` edge; every relocated intersection vertex lies on the exact
parabola (`y²=4f·x`) AND on both the cone and the plane to `TAU_MODEL`; the
`eval_source(point→t)` round-trip reproduces the relocated position; watertight
2-manifold and `χ = 2−2g` hold. RED author ≠ GREEN author ≠ Adversary.

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate, all prior tests unregressed — esp. the
ellipse/circle relocation + eval round-trip tests byte-for-byte; the curved fuzz,
run with the sidecar, must show **cone parabola cuts → `ok_correct`** and their
`AmbiguousCurve` → 0, with **ZERO new silent-wrong**), `cargo fmt -p yang-rs --
--check`, `cargo clippy -p yang-rs --all-targets -- -D warnings`.

**Calibrated metric:** cone `ok_correct` rises by the parabola share; total
`AmbiguousCurve` drops by that share. **Hyperbola `AmbiguousCurve` REMAINS**
(YR23) — that is expected, not a failure; do NOT chase it. (The
parabola-vs-hyperbola split within the 21 is not yet known — the driver verifies
the delta; surface E2E proof on the real sidecar as YR21 did if the in-container
fuzz can't complete — `curved_fuzz_sidecar_zombie_blocker`, do NOT fabricate
numbers.)

Sidecar binaries:
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (record PR-YR22 — the
parabola result + which share of the 21 it cleared) and `docs/yang_deviations.md`
if warranted. Hyperbola (YR23, two-branch selection) is the next and final conic.
