# PR-YR18 Spec — yang-rs Stage 5: intersection-edge attribution fix

Status: in progress (FIP/TDD role-separated cycle).
Scope: `crates/yang-rs/src/lib.rs` — `build_intersection_curves` only.

## 1. Confirmed mechanism (anchor)

The curved fuzz (`crates/yang-rs/tests/fuzz_curved.rs`, seed
`0xCF1_CADE_F00D_2026`) reports `SsiRefinementError::AmbiguousCurve` as the
dominant *loud* refusal. A driver investigation established it is:

- **not** rim-selection ambiguity — 0 cases with `matched >= 2`; every
  `AmbiguousCurve` is `matched == 0`;
- **not** missing-conic support — the bulk is cylinder + sphere, both fully
  handled;
- it **is** a surface-attribution defect: an edge is classified as a
  `(surfA, surfB)` intersection edge and handed to `ssi_rs::intersect`, but only
  *one* of its two endpoints actually lies on both surfaces. Decisive evidence:
  a cylinder∩plane edge with `tol = 3.1e-2`, one endpoint on both surfaces, the
  other `8.9e-2` off the plane — ~2.9× the chord band.

Source of the mis-classification: `compute_phase_a` (`lib.rs:3244`) builds the
incidence map by pushing the patch's single `info.inherited` face surface onto
*every* boundary edge of the patch cycle (`lib.rs:3279-3289`).
`build_intersection_curves` (`lib.rs:2525`) then treats any edge whose incidence
has exactly two entries of *different* `InputId` as an intersection edge. When a
patch-boundary edge has one endpoint off one surface, no returned curve passes
through both endpoints → `matched == 0` → `AmbiguousCurve`
(`lib.rs:2594-2602`). The defect is the **classification**, not the SSI math
(`ssi_rs::intersect` returns the correct curve).

## 2. Correctness target

An edge handed to `ssi_rs::intersect` as a `(surf0, surf1)` intersection edge
**must have BOTH endpoints on BOTH attributed surfaces within the edge's
Stage-1 chord band `tol`** (the same `tol` `build_intersection_curves` already
computes). An edge that fails this is an internal edge of a single surface and
must NOT be classified as an intersection edge — it `continue`s and falls
through to the existing `Curve::LineSegment` fallback in `emit_topology`
(`lib.rs:~4087`, `~4257`), never raising `AmbiguousCurve`.

## 3. Approach — explicit on-both-surfaces predicate gate (design "B")

Reuse `signed_distance_to_surface(surface, point)` (`lib.rs:1936`, already used
by the fuzz oracle). In `build_intersection_curves`, **before** handing the edge
to `ssi_rs::intersect`, compute `tol` *first*, then gate:

```text
let on_both = |p| |sd(surf0, p)| <= tol && |sd(surf1, p)| <= tol
if !(on_both(p_s) && on_both(p_e)) { continue; }   // not a true intersection edge
```

Order: compute `tol` (needs only surfaces + breps; pass a diagnostic-only
`candidates: 0` to the producer-fault helpers — their count is untested), run
the gate, then `surface_to_quadric` + `ssi_rs::intersect` + selection only for
edges that pass. A failing edge `continue`s → `Curve::LineSegment` fallback.

`signed_distance_to_surface` returns `Result` (always `Ok` for Plane/curved
surfaces); the gate propagates any `Err` rather than swallowing it.

**Why "B" over "A" (re-tag incidence by local incident-triangle surface):** B is
surgical — touches only `build_intersection_curves`, leaves the `incidence` map
untouched (so the PR-YR11 incidence-driven ellipse relocation consumer at
`lib.rs:~3503` and Stage-4 are unperturbed), and directly enforces the invariant.

## 4. No-regression invariant (key)

The gate reuses the **same per-edge `tol`** the selection already uses. The
intersection curve lies *on* both surfaces, so any point within `tol` of the
selected curve is within `tol` of both surfaces. Therefore **every edge that
currently selects `matched == 1` necessarily passes the gate** — the gate is a
*necessary condition* of existing success. The gate can ONLY change behavior for
edges that currently produce `matched != 1` (i.e. currently raise
`AmbiguousCurve`): among those it converts the subset with an endpoint off a
surface beyond `tol` from a loud `AmbiguousCurve` into a skipped → `LineSegment`
edge, while genuine ambiguities where both endpoints *are* on both surfaces
(e.g. the coincident-plane `t5_stop_path_coincident_planes_is_loud` STOP,
`yr9_stage3_ssi.rs`) still reach the existing loud error path. Planar plane∩plane
edges (`tol = TAU_WORK`) are unaffected — they already select `matched == 1`.

**No tolerance widening / no hack-to-green (P9/P10):** `tol` is the *existing*
Stage-1 chord bound, not a new looser constant. The off endpoint (0.089 vs 0.031
band) is genuinely off-surface, so the fix is classification, not loosening.
Cone `Parabola`/`Hyperbola` analytic-conic support stays out of scope: a *true*
cone∩plane edge passes the gate and still yields a loud `AmbiguousCurve` because
`curve_contains_point` returns `false` for conics — that loud refusal is correct
for this PR and must be preserved.

## 5. Phase contracts

### RED (sub-agent A; tests only, NO production code)
- A deterministic, sidecar-free fixture (NO `rand`, NO system time, NO FS) that
  reproduces a **mis-classified** edge: an edge with two different-`InputId`
  incidence entries whose one endpoint is off one attributed surface beyond the
  chord band, which **currently** raises `AmbiguousCurve { matched: 0 }`. Build
  it with a hand-built `LabelMock` arrangement + `boolean(&a, &b, op, &mock)`
  (the deterministic pattern in `yr9_stage3_ssi.rs` / `yr13_subtract_cylinder.rs`
  / `yr15_subtract_sphere.rs`), or a controlled mesh+attribution at unit level.
- The RED test asserts the **post-fix** behaviour: the boolean succeeds (or
  refuses for an unrelated, correctly-classified reason — NOT `AmbiguousCurve`
  from this edge), AND no `AmbiguousCurve { matched: 0 }` attributable to a
  mis-classified edge.
- The RED author confirms the test currently FAILS (RED) and documents the exact
  failing reason verbatim.

### GREEN (sub-agent B; production only, must NOT edit RED tests)
- First reproduce ONE concrete mis-tagged edge and **probe the anchor** (a
  temporary env-gated `eprintln` in `build_intersection_curves` printing, per
  edge sent to `ssi_rs::intersect`, both endpoints' `signed_distance_to_surface`
  to both attributed surfaces) to confirm a non-trivial fraction have one
  endpoint off one surface beyond the band. If the mechanism is materially
  different → **STOP and report**. Remove the probe before committing.
- Implement the gate per §3. Reorder so `tol` is computed before
  `ssi_rs::intersect`; pass diagnostic-only `candidates: 0` to the producer-fault
  helpers in the pre-intersect position.
- Verify the RED test passes and `cargo test -p yang-rs` is fully green
  (YR8–YR17 demo cases byte-for-byte unchanged; coincident-plane + yr9 loud
  STOPs preserved).

### Adversary (sub-agent C; independent audit)
- Re-run the curved fuzz with the sidecar; confirm the **cylinder + sphere**
  `matched == 0` `AmbiguousCurve` count drops materially with **ZERO new
  silent-wrong** and the fuzz still green; cone conics stay loud.
- Audit the gate is not a hack-to-green: confirm `tol` is the existing chord band
  (no new constant), confirm the no-regression invariant, confirm no RED test was
  weakened. Produce a verbatim `git diff` of the GREEN change.

## 6. Files

- `crates/yang-rs/src/lib.rs` — `build_intersection_curves` (the only production
  change).
- `crates/yang-rs/tests/yr18_*.rs` (RED) + `crates/yang-rs/tests/yr18_adversary.rs`.
- `docs/yang_functional_roadmap.md` — record PR-YR18 with before/after
  cylinder+sphere `matched == 0` counts.
- `docs/yang_deviations.md` — note the classification predicate; note cone conic
  support remains a separate deferred follow-up.
