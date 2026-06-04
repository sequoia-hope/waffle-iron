# PR-YR22 (continuation) — cone∩plane parabola: GREEN + Adversary on the committed RED

**This is the GREEN + Adversary completion of PR-YR22. The RED phase is ALREADY
COMMITTED at HEAD** (`test(yang-rs): PR-YR22 RED — cone∩plane parabola`). It was
authored by the (interrupted) RED subagent and validated by the driver: the crate
compiles via a `Curve::Parabola` variant + match-arm stubs marked
`STUB for RED type-check only`, and the 6 `tests/yr22_cone_parabola.rs` oracles +
the migrated `tests/yr21_cone_ellipse.rs::oracle6_parabola_section_succeeds` all
fail with `SsiRefinementFailed { reason: AmbiguousCurve { candidates: 1,
matched: 0 } }` — i.e. parabola is not yet implemented. That is the correct RED.

**Do NOT re-author or weaken the RED tests.** A GREEN subagent (distinct from you,
the Manager) implements production; then a third Adversary subagent. The
implementer must not edit the test files (`yr22_cone_parabola.rs`,
`yr21_cone_ellipse.rs`, `yr21_adversary.rs`) except to fix a genuine RED-author
bug — and if so, STOP and flag it loudly rather than silently changing an oracle.

Context: PR-YR21 shipped the type-agnostic `project_onto_cone_section` Stage-4
relocation (used by cone ellipse). `ssi-rs` `plane_cone` returns
`SsiCurve::Parabola { vertex, normal, axis_dir, focal_length }` for the PARA case
(parameterization `vertex + (t²/(4f))·axis_dir + t·(normal × axis_dir)`).
Parabola is **single-candidate** — no two-branch logic (that is PR-YR23 hyperbola).

## GREEN — replace each `STUB for RED type-check only` arm with the real impl

1. **`curve_contains_point` Parabola arm** (currently `false` → `matched == 0`):
   real membership — out-of-plane `|(p−vertex)·normal| ≤ tol`, and the in-plane
   point satisfies `y² = 4f·x` to the chord band, with `x = (p−vertex)·axis_dir`,
   `y = (p−vertex)·(normal × axis_dir)`. Derive the band from the Stage-1 cone
   chord error; **if the in-plane-metric mismatch bites (cf. PR-YR19 N11), apply
   the SAME propagated-band reasoning — justified, NOT a flat widening (P9/P10).**
   This drives `matched == 1`.
2. **`ssi_curve_to_curve` Parabola arm**: map `SsiCurve::Parabola` →
   `Ok(Curve::Parabola { vertex, normal, axis_dir, focal_length })` (today it still
   returns `Err(UnsupportedCurve)`; selection never reaches it because membership
   fails first, but it MUST be mapped for the matched curve to build).
3. **`eval_source` Parabola arm**: replace the `return Point3::new(0,0,0)` stub
   with `parabola_point(vertex, normal, axis_dir, focal_length, t)` (the real
   `parabola_point` is already present from RED and mirrors `ssi-rs`; verify it
   does, byte-for-byte, so the round-trip oracle holds).
4. **Stage-4 relocation Parabola arm** (currently `continue`): a `Curve::Parabola`
   edge whose incidence is a `Surface::Cone` + `Surface::Plane` relocates each
   endpoint via the YR21 `project_onto_cone_section`, tagging `t = (relocated −
   vertex)·(normal × axis_dir)` (the conjugate-axis coordinate of the
   parameterization), residual gated by the cone chord band — mirror the
   cone-ellipse arm. Reuse the YR21 projector + budget helpers; add NO second
   relocation method.
5. **`is_reversed` Parabola arm** (currently `[0,0,0]` tangent stub): the real
   parabola tangent at the relocated point — `d/dt = (t/(2f))·axis_dir +
   (normal × axis_dir)` (un-normalized direction is fine if the consumer
   normalizes; match how the circle/ellipse arms supply their tangent). Needed for
   cavity-sense on Subtract.
6. Drop the `STUB for RED type-check only` comment on the `Curve::Parabola`
   variant (it is real now).

## Scope / non-regression

- **Parabola only.** Hyperbola (two-branch, YR23) stays LOUD (`AmbiguousCurve`) —
  correct, do NOT touch it.
- **Byte-for-byte unchanged:** circle (YR17), cone ellipse (YR21) + cylinder
  ellipse (YR11), sphere, all planar, YR8–YR21 demos. Axis-parallel / through-apex
  sections stay LOUD (the YR21 guards).

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate green — the 6 `yr22_cone_parabola` oracles +
`yr21_cone_ellipse::oracle6` now PASS; all prior tests unregressed),
`cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs --all-targets --
-D warnings`. **Push `main` at the end** (the RED commit becomes green once GREEN
lands — the pushed tip is green; origin currently sits at the pre-RED green
commit, so this push carries RED+GREEN+Adversary together).

## Adversary

Independently verify: cone-ellipse + cylinder-ellipse + circle paths
byte-identical (a parabola edit didn't perturb them); the parabola eval round-trip
(`eval_source(point→t)` reproduces the relocated point to `TAU_MODEL`); a point
genuinely off the parabola (beyond the band) is NOT matched (no over-acceptance,
SILENT_WRONG = 0); hyperbola + axis-parallel sections still STOP loudly. Add an
over-admit / byte-identity canary as in YR19/YR20/YR21.

Sidecar (for the E2E oracle8; if the in-container fuzz can't complete, the bounded
E2E on the real binary stands in — `curved_fuzz_sidecar_zombie_blocker`, do NOT
fabricate fuzz numbers; the driver verifies the curved-fuzz delta):
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (PR-YR22 DONE — parabola
end-to-end; note it built on the preserved RED). Hyperbola (YR23, two-branch) is
the next and final conic.
