# PR-YR18 — yang-rs Stage 5: intersection-edge attribution fix (the AmbiguousCurve bulk)

Context: the curved fuzz (`crates/yang-rs/tests/fuzz_curved.rs`, seed
`0xCF1_CADE_F00D_2026`) reports `SsiRefinementFailed::AmbiguousCurve` as the
dominant loud refusal. A driver investigation (residual + per-endpoint
surface-distance instrumentation, now reverted) established **what this is and
is NOT**:

- **It is NOT rim-selection ambiguity.** Across the slice, **0** cases had
  `matched ≥ 2`. Every `AmbiguousCurve` was `matched == 0` (no returned SSI
  curve passed through *both* mesh endpoints). So there is no "pick the right
  curve among several" problem to solve.
- **It is NOT (mostly) missing conic support.** The bulk is **cylinder + sphere**
  (curves are `Circle`/`Ellipse`, both fully handled by `curve_contains_point`),
  not cone. (Cone `Parabola`/`Hyperbola` is a *separate, deferred* feature — see
  Scope; do NOT implement conics here.)
- **It IS a surface-attribution defect.** Decisive evidence for a cylinder∩plane
  edge `(p_s, p_e)` (`surf0 = Cylinder`, `surf1 = Plane`):
  ```
  tol = 3.1e-2  (the cylinder Stage-1 chord band)
  p_s: d(cylinder)=5e-17  d(plane)=0.0      ← on BOTH surfaces ✓ (on the ellipse)
  p_e: d(cylinder)=0.0    d(plane)=8.9e-2   ← on the cylinder, 0.089 OFF the plane
  ```
  `ssi_rs::intersect` returned the **correct** ellipse. The failure is that the
  edge was *classified as a `cylinder∩plane` intersection edge*, but only `p_s`
  lies on both surfaces — `p_e` is ~0.089 off `surf1` (far beyond any chord
  band). A point on both surfaces is necessarily on their intersection curve, so
  **this edge is not a true intersection edge along its whole length**: it is an
  internal facet edge of the cylinder where a box patch and the cylinder patch
  meet in the mesh, mis-classified as a refinable intersection arc.

## The anchor (verify BEFORE writing any fix — P-anchor-before-fix)

Incidence is built **per patch** in `compute_phase_a_structures`
(`crates/yang-rs/src/lib.rs` ~3371-3384): `flood_fill_patches` groups triangles
by identical `(InputId, face)` attribution (2-of-3 majority vote per triangle,
`TriangleAttribution`), then **every boundary edge of a patch inherits that
patch's single `face.surface`** (`info.inherited`). An "intersection edge" is
then any edge whose incidence has exactly two entries with *different* `InputId`
(`build_intersection_curves`, ~2532), and it is fed to `ssi_rs::intersect`.

The defect lives in this classification: the patch-boundary polyline between a
box patch and the cylinder patch contains edges that are **not** on both
surfaces (one endpoint is a pure-cylinder vertex). They must not be treated as
intersection arcs.

**The GREEN sub-agent must first reproduce ONE concrete mis-tagged edge and
probe the anchor to confirm the *exact* mechanism** — e.g. add temporary
`eprintln`s (gated by an env var) at the incidence-construction / classification
site that print, for each edge sent to `ssi_rs::intersect`, both endpoints'
distance to both attributed surfaces; confirm a non-trivial fraction have one
endpoint off one surface beyond the chord band. Reproduce deterministically from
the fuzz seed above (scan for the first cylinder/sphere `matched=0` case). If the
mechanism is materially different from "patch-boundary edges that are not on both
surfaces get classified as intersection edges" — **STOP and report** (do not
improvise a fix for a mechanism you did not confirm).

## Correctness target (what must hold; mechanism is the implementer's to design)

An edge handed to `ssi_rs::intersect` as a `(surfA, surfB)` intersection edge
**must have BOTH endpoints on BOTH attributed surfaces within the Stage-1 chord
band** of the relevant curved surface (the same band `build_intersection_curves`
already uses for `tol`). The fix is to **stop classifying patch-boundary edges
that fail this as intersection edges** — they are internal edges of a single
surface (a cylinder/sphere/cone facet edge, or a planar edge) and must
reassemble as such (`Curve::LineSegment` for the planar fallback, or carried on
the single owning curved surface), NOT raise `AmbiguousCurve`.

Design freedom (pick what the confirmed mechanism warrants, and justify it in the
plan): tag each edge by the **local incident triangle's** attributed face surface
rather than the patch's single `inherited` surface; and/or add an explicit
on-both-surfaces predicate as the gate for "is this a true intersection edge"
before calling `ssi_rs::intersect`. Whatever you choose:
- **No tolerance widening / no hack-to-green (P9/P10).** The band is the existing
  Stage-1 chord bound, not a new looser constant. The off endpoint here is 0.089
  vs a 0.031 band — it is genuinely off-surface, so the fix is *classification*,
  not loosening.
- A true intersection edge (both endpoints on both surfaces) must STILL be
  refined to its exact SSI curve exactly as today (the YR9/YR11/YR15/YR16 demo
  pairs must not regress — including the PR-YR11 incidence-driven cylinder-ellipse
  consumer at ~3563).

## Scope

- **Cylinder + sphere intersection-edge mis-classification only.** Cone
  `Parabola`/`Hyperbola` analytic-conic support is explicitly **out of scope**
  (a separate deferred feature; those `matched=0` cases stay loud `AmbiguousCurve`
  and that is correct for this PR).
- All planar booleans and the YR8–YR17 curved demo cases must be **byte-for-byte
  unchanged** unless a case was itself a victim of this defect (in which case it
  flips from a loud `AmbiguousCurve` to a correct result — verify against the
  sidecar).

## RED contract

A deterministic fixture (NO `rand`, NO system time, NO FS side effects) that
reproduces a mis-classified edge: an edge classified as a two-different-input
intersection edge whose one endpoint is off one attributed surface beyond the
chord band — and which therefore currently raises
`SsiRefinementFailed::AmbiguousCurve { matched: 0 }`. Pin it minimally (a
cylinder/sphere + a rigid-transformed box, or a controlled mesh+attribution at
the unit level). The RED test asserts the *correct* post-fix behaviour: the
boolean succeeds (or refuses for an unrelated, correctly-classified reason), and
every edge fed to `ssi_rs::intersect` satisfies the on-both-surfaces predicate.
The RED author must NOT write production code; the GREEN author must NOT edit the
RED tests.

## CI gate (FULL crate)

`cargo test -p yang-rs` (whole crate — all prior tests unregressed; the curved
fuzz, run with the sidecar via `CHERCHI2022_BIN` +
`CHERCHI2022_INPUTCHECK_BIN`, must show the **cylinder + sphere** `matched=0`
`AmbiguousCurve` count drop materially with **ZERO new silent-wrong** and the
fuzz still green), `cargo fmt -p yang-rs -- --check`, `cargo clippy -p yang-rs
--all-targets -- -D warnings`.

Sidecar binaries (for the fuzz arm):
`CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
`CHERCHI2022_INPUTCHECK_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans_inputcheck`

On completion: update `docs/yang_functional_roadmap.md` (record PR-YR18 — the
AmbiguousCurve attribution fix: report the before/after cylinder+sphere
`matched=0` counts from the curved fuzz) and `docs/yang_deviations.md` if the
classification predicate introduces or resolves a deviation. Note in the commit
that cone conic support remains the separate deferred follow-up.
