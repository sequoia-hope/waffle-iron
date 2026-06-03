# PR-YR19 — yang-rs: sphere∩plane chord-band metric consistency (the residual sphere AmbiguousCurve)

Context: PR-YR18 (the on-both-surfaces gate before `ssi_rs::intersect`)
eliminated the **cylinder** `AmbiguousCurve` mass entirely (21 → 0) and cut the
total 56 → 30, but **sphere** `AmbiguousCurve` only dropped 20 → 15. A driver
investigation (env-gated residual prints in `build_intersection_curves`, now
reverted) found the residual sphere cases are a **single, uniform root cause**
distinct from YR18 — a geometric **metric inconsistency**, NOT a real off-curve
point and NOT missing surface support.

## The finding (decisive, all 8 sampled sphere blocks identical in shape)

Every residual sphere case is `surf0=Sphere`, `surf1=Plane`, **`candidates == 1`**
(a single `Circle` — sphere∩plane is never ambiguous), and:
- **passes the YR18 gate** — both endpoints are within the Stage-1 chord band
  `tol` of BOTH the sphere and the plane;
- but **fails `curve_contains_point`** because one endpoint's **in-plane radial**
  deviation `|radial − r_circle|` exceeds `tol`, even though its **sphere-normal**
  distance is within `tol`.

Representative evidence (`d_sphere` = endpoint's distance to the sphere; `dr` =
its `|radial − r_circle|`; `tol` = `sphere_chord_bound`):

| tol | p_e d_sphere | p_e dr | over? |
|---|---|---|---|
| 1.916e-2 | 1.44e-2 | 1.94e-2 | yes (1.3%) |
| 1.908e-2 | 1.10e-2 | 3.12e-2 | yes (2.8× d_sphere) |
| 1.795e-2 | 1.45e-2 | 2.70e-2 | yes (1.9× d_sphere) |
| 1.123e-2 | 8.72e-3 | 1.21e-2 | yes |

The amplification is exactly `dr ≈ (R / r_circle) · d_sphere` (a mesh vertex
within `d_ε` of the sphere along the surface normal projects, after intersection
with the cutting plane, to an in-plane radial deviation up to `(R/r_c)·d_ε`; when
the cut plane is far from the sphere center the section circle `r_c` is small and
the factor is large). The point is **genuinely on the section circle within the
Stage-1 chord error** — the radial *metric* under-bounds it.

## CRITICAL interaction — the fix MUST cover Stage 4 too

`d_eps` is the Stage-1 chord bound on the **surface-normal** tessellation error.
Two sites measure curve membership with the **in-plane radial** metric and
compare to `d_eps`, so BOTH over-reject the same sphere points:
1. **Selection** — `curve_contains_point` (`build_intersection_curves`, the
   `matched != 1` raise) → `AmbiguousCurve`.
2. **Stage 4 relocation guard** — `circle_residual(p, …) > d_eps`
   (lib.rs ~3606) → `Stage4RegionInvalid::OffCurveBeyondChordBand`; and the
   parallel `ellipse_residual` guard (~3631).

**A fix that only touches selection will merely convert the 15 sphere
`AmbiguousCurve` into ~15 `OffCurveBeyondChordBand` — zero net `ok_correct`
gain.** The success criterion is therefore `ok_correct` **rising**, not the
`AmbiguousCurve` count alone.

## The fix — geometrically-correct band, NOT tolerance widening (P9/P10)

The membership/residual band must reflect the **true propagated bound** of the
Stage-1 surface chord error in whatever metric the check uses — i.e. the A14.3
single-source `d_ε` carried correctly through the section projection, not a flat
multiplier picked to pass. The plan must pick ONE approach and justify it:

- **(A) projection-scaled radial band.** Bound the in-plane radial residual by
  `(R / r_circle) · d_ε` for a curved-surface section circle (and the analogous
  factor for the cylinder/cone ellipse), where `R` is the originating curved
  surface's radius (available at selection from `Surface::Sphere`/`Cylinder`, and
  carryable to Stage 4 alongside the per-vertex circle). This is the exact
  geometric propagation of the SAME `d_ε`, applied in both sites.
- **(B) surface-distance unification.** Replace the radial residual *as the
  validity criterion* with the projection-independent on-both-surfaces predicate
  (the YR18 `signed_distance_to_surface ≤ d_ε`, which is correct by construction)
  in both selection and the Stage-4 guard; relocation (`project_onto_circle`)
  still snaps radially. Requires plumbing the generating surfaces to the Stage-4
  guard.

Whichever is chosen, the change is justified as **correcting an under-tight band
that ignored the section projection** — the same `d_ε`, measured consistently —
and MUST preserve the safety property of both gates: a point genuinely off the
curve / on the WRONG curve (beyond the propagated chord error) is still rejected
loudly. The adversary must pin this (a point off by more than the propagated band
still STOPs; multi-candidate disambiguation, cone conics, coincident-plane STOPs
all unchanged).

## Scope

- **Sphere∩plane circle membership/residual only** (the 15). Do not regress the
  cylinder result (now 0 `AmbiguousCurve`) or the YR8–YR18 demo cases
  (byte-for-byte unless a case was itself a victim of this metric bug, in which
  case it flips to a correct `Ok` — verify against the sidecar).
- Cone `Parabola`/`Hyperbola` analytic support stays **out of scope** (those
  `AmbiguousCurve`/`UnsupportedCurve` cases stay loud — correct for this PR). If
  the chosen approach changes a cone case's loud *variant*, say so and migrate any
  affected test's expected variant without weakening its structural assertions.

## RED contract

A deterministic fixture (NO `rand`, NO system time, NO FS side effects): a
sphere + a plane cut **far from the sphere center** (small section circle, large
`R/r_c`) whose mesh-approximated intersection endpoints are within `d_ε` of both
surfaces but whose in-plane radial deviation exceeds `d_ε` — i.e. a case that
currently raises `AmbiguousCurve` (and, after a selection-only fix, would raise
`OffCurveBeyondChordBand`). The RED test asserts the **correct post-fix
behaviour**: the boolean returns `Ok` with the exact `Curve::Circle`, the
relocated intersection vertices lie on the exact circle to `TAU_MODEL`, and
watertight 2-manifold / `χ = 2−2g` hold. RED author ≠ GREEN author ≠ Adversary.

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate, all prior tests unregressed; the curved
fuzz, run with the sidecar, must show **sphere `ok_correct` rise materially**
with **ZERO new silent-wrong** — NOT merely an `AmbiguousCurve`→`Stage4` swap),
`cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs --all-targets --
-D warnings`.

Sidecar binaries (for the fuzz arm — note the worker environment may not be able
to run the sidecar fuzz to completion; if so, say so honestly and do NOT
fabricate numbers, the driver will reproduce the delta):
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (record PR-YR19 — the
sphere chord-band metric fix; the approach chosen and its geometric
justification) and `docs/yang_deviations.md` (extend/replace the N10 note or add
N11 for the propagated-band predicate). Note the remaining cone analytic-conic
follow-up.
