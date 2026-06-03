# PR-YR20 — yang-rs Stage 6: tiered face-resolution tie-break (cap-vs-curved-lateral rim tie)

Context: after PR-YR18 (intersection-edge attribution gate) and PR-YR19 (sphere
chord-band metric), the curved fuzz's largest **non-cone** refusal bucket is
`FaceResolutionFailed` (curved fuzz N=90: cylinder 9, cone 7 ≈ 16). A driver
investigation (env-gated prints at the Stage-6 resolution F3 site, now reverted)
found a **single, uniform root cause** — and it is NOT a no-match.

## The finding (12/12 sampled cases identical, `n_hits == 2`, zero no-match)

Stage-6 geometric face resolution (`src/lib.rs`, the non-degenerate branch
~3227-3240) attributes a kept triangle to the input face whose surface contains
the triangle **centroid** within that face's per-face tolerance `tol_for`
(`TAU_WORK` for a `Plane`; the Stage-1 chord band `d_ε` for `Cylinder`/`Sphere`/
`Cone`). It counts hits: exactly 1 → attribute; **0 or ≥2 → `FaceResolutionFailed`**.

Every `FaceResolutionFailed` in the curved fuzz is a **`n_hits == 2` tie** of this
exact shape (`input=A`, the curved primitive):

```
Plane     dist = 5.5e-17   tol = 1.0e-12 (TAU_WORK)   HIT   ← centroid EXACTLY on a cap plane
Cylinder  dist = 7.6e-3    tol = 2.4e-2  (d_ε)        HIT   ← also within the loose curved band
```

(7 cylinder + 5 cone among the sample; every one is one `Plane` HIT + one curved
HIT.) The triangle lies **exactly on a planar cap** (`dist < TAU_WORK`, zero chord
error — it genuinely IS a cap triangle), but because the curved lateral's chord
band `d_ε` (~2.4e-2) is necessarily loose, a cap triangle **near the rim** also
falls within the lateral's band → spurious second hit → tie → F3.

**Root cause:** the F3 rule treats an **exact** `TAU_WORK` planar hit and an
**approximate** `d_ε` curved-band hit as equal-weight. They are not: the centroid
is ON the plane (exact) and merely NEAR the curved face (within its tessellation
chord error). The triangle's true face is the cap.

## The fix — tiered tie-break (NOT tolerance widening, P9/P10)

Attribute to the **unique hit at the tightest tolerance tier**. An exact hit
(`dist < TAU_WORK`) dominates a chord-band hit (`dist < d_ε` but `≫ TAU_WORK`):
when a triangle is within `TAU_WORK` of a plane it lies on that plane exactly, so
that is its face regardless of being within a curved neighbour's loose band.
`FaceResolutionFailed` is raised **only when ≥2 faces hit at the MINIMUM tier**
(a genuine same-tier ambiguity).

This is the natural generalization of the existing rule, NOT a new looser
constant: each face still uses its own A14.3 single-source band; we only rank ties
by how-relatively-within the centroid is (e.g. unique minimum `dist/tol` ratio, or
an explicit exact-tier-beats-band-tier preference — pick one and justify it).

**CRITICAL non-regression (verify byte-identical for all-planar):** for an
all-planar input every `tol_for` is `TAU_WORK`, so every hit is the SAME tier and
the tiered rule reduces EXACTLY to today's "exactly one face within `TAU_WORK`"
— a genuine coplanar / multi-solid planar tie (two planes both within `TAU_WORK`)
MUST still be `FaceResolutionFailed` (that case is correctly deferred to M8; the
900-case box fuzz, m3, and yr5c planar-sliver tests must be byte-for-byte
unchanged). The fix may ONLY change mixed exact-planar-vs-curved-band ties.

Consider whether the degenerate-sliver branch (~3206-3216, "first face within
tol", lowest index, no F3) needs the same tiering for consistency; if you change
it, prove planar inputs stay byte-identical.

## Scope

- The **cap-vs-curved-lateral rim tie** only. Do NOT change the attributed face of
  any triangle that is not currently a tie. Do NOT touch the band values
  (`tol_for`) or the intersection-edge path (YR18/YR19).
- **Calibrated success metric (read carefully — avoids the "moved-the-failure"
  trap):** clearing **cylinder** `FaceResolutionFailed` unblocks cylinder, so
  **cylinder `ok_correct` MUST rise**. Clearing **cone** `FaceResolutionFailed`
  will NOT raise cone `ok_correct` — cone is still blocked by the deferred
  `AmbiguousCurve` analytic conics (`Parabola`/`Hyperbola`), so a cone triangle
  that stops being an F3 tie simply refuses later for that deferred reason. That
  is EXPECTED and correct. So the gate is: **total `FaceResolutionFailed` → ~0**
  AND **cylinder `ok_correct` rises**, with **ZERO new silent-wrong** and no new
  `NonManifoldOutput`. Cone `ok_correct` staying 0 is fine (deferred conics).

## RED contract

A deterministic fixture (NO `rand`, NO system time, NO FS side effects): a kept
**positive-area** triangle whose centroid is within `TAU_WORK` of a cylinder/cone
**cap plane** AND within the curved lateral's `d_ε` band (a near-rim triangle) —
i.e. a case that currently raises `FaceResolutionFailed { tri }` with `n_hits ==
2`. The RED test asserts the correct post-fix behaviour: the triangle attributes
to the **cap plane**, the boolean returns `Ok`, watertight 2-manifold / `χ =
2−2g` hold. Include (or have the Adversary add) a **genuine all-planar coplanar
tie** fixture that MUST still raise `FaceResolutionFailed` (the safety property).
RED author ≠ GREEN author ≠ Adversary.

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate, all prior tests unregressed — esp. the
all-planar coplanar-tie / m3 / yr5c sliver tests byte-for-byte; the curved fuzz,
run with the sidecar, must show **total `FaceResolutionFailed` drop to ~0**,
**cylinder `ok_correct` rise**, ZERO new silent-wrong), `cargo fmt -p yang-rs --
--check`, `cargo clippy -p yang-rs --all-targets -- -D warnings`.

Sidecar binaries (the worker environment may be unable to run the sidecar fuzz to
completion; if so say so honestly and do NOT fabricate numbers — the driver
reproduces the delta):
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (record PR-YR20 — the
tiered tie-break, the all-planar byte-identical argument, and the calibrated
metric: cylinder `ok_correct` rises, cone stays deferred) and
`docs/yang_deviations.md` if the tier rule warrants a note. The cone
analytic-conic support remains the separate deferred follow-up.
