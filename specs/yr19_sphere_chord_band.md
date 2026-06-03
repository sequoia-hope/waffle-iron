# PR-YR19 — sphere∩plane chord-band metric consistency

Spec of record for the role-separated FIP cycle. Manager (this file) → RED
sub-agent → GREEN sub-agent → Adversary sub-agent. Test author never writes
production code; implementer never edits tests.

## 1. Problem

PR-YR18 added an on-both-surfaces gate before `ssi_rs::intersect`
(`build_intersection_curves`, `crates/yang-rs/src/lib.rs`) and eliminated the
entire **cylinder** `AmbiguousCurve` mass (driver-verified curved fuzz N=90:
21 → 0). The **sphere** `AmbiguousCurve` only dropped 20 → 15. A driver
investigation (env-gated prints, since reverted) found the 15 residual sphere
cases share ONE root cause, distinct from YR18:

Every residual case is `surf0 = Sphere`, `surf1 = Plane`, `candidates == 1` (a
single section `Circle` — sphere∩plane is never ambiguous). The mesh endpoints
**pass the YR18 gate** (within the Stage-1 chord band `tol` of both the sphere
and the plane, measured along the surface normal via
`signed_distance_to_surface`), but **fail `curve_contains_point`** because the
**in-plane radial** deviation `|radial − r_circle|` exceeds `tol`, even though
the **sphere-normal** distance is within `tol`. So `matched == 0` and the edge
raises a loud `AmbiguousCurve` — wrongly.

This is a **metric inconsistency**, not a real off-curve point.

## 2. Metric derivation (the propagated bound)

`d_ε = sphere_chord_bound(R) = 1e-2 · 2R√3` bounds the **surface-normal**
tessellation error of a sphere of radius `R`. A mesh vertex within `d_ε` of the
sphere *along its normal*, intersected with the cutting plane, projects to an
**in-plane radial** deviation up to `(R / r_circle) · d_ε`.

Derivation. Let `C` be the sphere centre, `h` the (fixed) signed distance from
`C` to the cutting plane, and `radial` the in-plane distance from the section
circle's centre. A point on the cut plane satisfies
`|p − C| = √(h² + radial²)`. The sphere-normal residual is
`d_sphere = |p − C| − R`. Differentiating,
`d/d(radial) √(h² + radial²) = radial / √(h² + radial²)`, which at the section
circle (`radial = r_circle`, `√(h² + r_circle²) = R`) equals `r_circle / R`.
Hence to first order

```
d_sphere ≈ (r_circle / R) · dr     ⇔     dr ≈ (R / r_circle) · d_sphere
```

where `dr = |radial − r_circle|` is the in-plane radial deviation. When the cut
plane is far from the sphere centre the section circle `r_circle` is small and
the amplification `R / r_circle` is large; a mesh vertex genuinely on the
section circle within the Stage-1 chord error can have `dr` up to
`(R / r_circle) · d_ε` while its sphere-normal residual `d_sphere` stays `≤ d_ε`.

The **axial** (out-of-plane) component keeps the unscaled band `d_ε`: the cut
plane is exact (planar inputs have zero chord error), so an on-curve point's
out-of-plane deviation is bounded by `d_ε` directly, not amplified.

Carrying the SAME `d_ε` correctly through the section projection is **not**
tolerance widening (P9/P10): the band is *derived* from the section geometry,
not picked to pass. A point off by more than the propagated band still STOPs
loudly.

## 3. Chosen approach: (A) projection-scaled radial band

Bound the in-plane radial residual of a sphere section `Circle` by
`radial_band = (R / r_circle) · d_ε`, where `R` is the originating sphere radius
and `r_circle` is the section circle's own radius. The axial component keeps the
unscaled `d_ε`. Surface-type-gated on the curved owner being a `Surface::Sphere`;
every non-sphere path stays byte-for-byte identical (factor = 1, i.e.
`source_radius = None`).

### Why (A) and not (B) (surface-distance unification)

`curve_contains_point` does double duty — it is also the per-candidate
**disambiguator** for the `matched == 1` selection (e.g. parallel
cylinder∩plane returns two `Line` candidates; the membership test picks the one
both endpoints lie on). The on-both-surfaces predicate is curve-independent: an
endpoint on both surfaces is on the intersection *set*, so it would test true
for *every* candidate, collapsing `matched` to `candidates` and re-raising
`AmbiguousCurve` on legitimate multi-candidate cases — a regression of the
cylinder result PR-YR18 just fixed. Approach (A) keeps the per-curve geometric
test intact and only corrects the band in the metric it already uses.

## 4. Production changes (`crates/yang-rs/src/lib.rs` only)

Two sites, each gated on the curved owner being a `Surface::Sphere`; all other
curve types and the all-planar / cylinder / cone paths remain byte-identical.

### Site 1 — selection: `curve_contains_point` + caller `build_intersection_curves`

- Thread the originating sphere radius into the membership test via a new
  parameter `source_radius: Option<f64>` on `curve_contains_point`. In the
  `Circle` arm:

  ```
  let radial_tol = match source_radius {
      Some(big_r) if radius > MIN_FEATURE_SIZE => (big_r / radius) * tol,
      _ => tol,
  };
  axial.abs() <= tol && (radial - radius).abs() <= radial_tol
  ```

  The axial band stays `tol`. `Line` / `Ellipse` / `Parabola` / `Hyperbola`
  arms are unchanged.
- Near-tangent guard: `radius > MIN_FEATURE_SIZE` in the scale (a near-tangent
  section is not a real edge) so the factor cannot blow up — fail **closed**
  (keep the unscaled band) rather than admit garbage.
- In `build_intersection_curves`, the sphere branch already computes
  `tol = sphere_chord_bound(radius)`. Pass `Some(R)` (the sphere's `radius`)
  into both `curve_contains_point` calls for the sphere case, and `None` for the
  cylinder / cone / plane cases (preserving current behavior exactly). Thread the
  factor through the same `if/else if` chain that selects `tol`.

### Site 2 — Stage-4 relocation guard: `stage4_relocate_and_correct`

- The per-vertex circle map `vert_circle` currently stores
  `(center, normal, r_circle)`. Extend it to carry the source sphere radius
  `Option<f64>`. When populating it from a `Curve::Circle` edge, look up that
  edge's incidence in `inc0` (already available — the ellipse arm does the
  analogous lookup) for an incident `Surface::Sphere { radius, .. }` → `Some(R)`,
  else `None`.
- Replace the combined `circle_residual(p) > d_eps` check with a split
  per-component check:

  ```
  let (axial, radial_dev) = circle_residual_split(p, center, normal, r_circle);
  let radial_band = match src_r {
      Some(big_r) if r_circle > MIN_FEATURE_SIZE => (big_r / r_circle) * d_eps,
      _ => d_eps,
  };
  if axial > d_eps || radial_dev > radial_band { return Err(... OffCurveBeyondChordBand) }
  ```

  For `None` (cylinder / cone circle) this is identical to
  `circle_residual > d_eps` (`max(axial, radial_dev) > d_eps`), so those paths
  stay byte-identical. Add a small `circle_residual_split(...) -> (axial, radial_dev)`
  helper rather than mutating `circle_residual`'s combined-max contract used
  elsewhere.
- `project_onto_circle` (the radial snap) is unchanged. The `ellipse_residual`
  guard is **unchanged** — sphere∩plane never yields an ellipse; oblique-cylinder
  ellipses are out of scope for this PR.

No change to Stage-6 face resolution (`tol_for`): it uses
`signed_distance_to_surface` (the projection-independent surface-normal metric),
which is already correct and not amplified.

## 5. Two sites, both load-bearing

Fixing only Site 1 converts the 15 `AmbiguousCurve` into ~15
`Stage4RegionInvalid::OffCurveBeyondChordBand` with **zero net `ok_correct`
gain**. The success criterion is `ok_correct` **rising**, not the
`AmbiguousCurve` count alone. The RED fixture below forces BOTH fixes.

## 6. RED contract (test-author sub-agent)

A deterministic, sidecar-free fixture (NO `rand`, NO system time, NO FS side
effects), modeled on `tests/yr15_subtract_sphere.rs` (hand-built `LabelMock`
`LabeledArrangement` → public `boolean(&box, &sphere, Subtract, &mock)`). File:
new `crates/yang-rs/tests/yr19_sphere_chord_band.rs`.

- A box with a sphere whose center is **above the box top** so only a **small
  cap** dips through one face → a small section circle `r_c ≪ R` (large
  `R/r_c`). E.g. center `(0,0,2+a)`, `R = 1`, `a ≈ 0.95` → `r_c ≈ 0.312`,
  `R/r_c ≈ 3.2`. Genus-0 dimple (χ=2), same topology family as YR15.
- Author the **rim (intersection) vertices on the cut plane** (axial-to-plane =
  0) at radial distance `radial = r_c + dr`, with `dr` chosen in the OPEN band
  `(d_ε, (R/r_c)·d_ε)` (e.g. `dr ≈ 0.9·(R/r_c)·d_ε`): then
  `d_sphere ≈ (r_c/R)·dr < d_ε` (passes the YR18 on-both gate) but `dr > d_ε`
  (current radial metric over-rejects). This makes the band **magnitude
  load-bearing in the mock** (the gap YR15's exactly-on-circle mock left), so the
  bug reproduces WITHOUT the sidecar.
- Keep YR15's mandatory `mock_is_valid_genus0` self-check (no `boolean()` call)
  so the fixture is proven a valid closed χ=2 shell before the boolean oracles
  run.
- RED status: today this raises `AmbiguousCurve` (selection) — and after a
  selection-only fix would raise `OffCurveBeyondChordBand` (Stage 4) — so the
  test FORCES both fixes.

Assert the **correct post-fix behavior** (these FAIL today):
1. `boolean(...)` returns `Ok` with the exact `Curve::Circle` (`center`,
   `normal`, `radius == r_c` to `TAU_MODEL`) for the section rim.
2. The relocated intersection vertices lie on the exact circle to `TAU_MODEL`
   (`|x − center| = r_c`, on the cut plane, on the sphere `|x − SPH_CENTER| = R`).
3. Watertight 2-manifold, `χ = 2 − 2g` (here χ=2), 0 unpaired half-edges,
   signed volume > 0.
4. Env-gated sidecar parity (LOUD skip when `CHERCHI2022_BIN` unset), mirroring
   YR15 O5.

## 7. GREEN (a DIFFERENT sub-agent — implementer)

Implement §4 to make the RED suite pass. Implementer must NOT edit tests. If the
diagnosis turns out wrong or a genuine conflict appears, **STOP and report**
(P9/P10) — no improvised alternative, no tolerance widening, no fallback. Verify
the planar `fuzz_boxes` and YR8–YR18 demo tests stay byte-for-byte
(`source_radius = None` everywhere except the sphere arm guarantees this).

## 8. Adversary (a THIRD sub-agent)

Independently pin the safety property and scope:
- A point off by **more than** the propagated band `(R/r_c)·d_ε` radially (or
  beyond `d_ε` axially) still STOPs loudly (`AmbiguousCurve` /
  `OffCurveBeyondChordBand`) — mutation-verify the band is load-bearing in both
  sites.
- Multi-candidate disambiguation unchanged (parallel cylinder∩plane two-line
  selection still `matched == 1`); cone perpendicular-cut `Circle` selection
  unchanged (`None` factor); coincident-plane / cone-conic loud STOPs unchanged.
- Near-tangent guard: `r_c → 0` does not blow the band into admitting garbage.
- Confirm no test migration weakened (any YR-case that was itself a victim of
  this metric bug flips Err→Ok against the sidecar, preserving every structural
  assertion).

## 9. Verification / CI gate (FULL crate)

- `cargo test -p yang-rs` (whole crate; all prior tests unregressed).
- `cargo fmt -p yang-rs -- --check`.
- `cargo clippy -p yang-rs --all-targets -- -D warnings`.
- Curved fuzz (sidecar): sphere `ok_correct` must rise materially with ZERO new
  silent-wrong. Honesty note: this container has a known sidecar-zombie blocker
  for the 300-case curved fuzz (memory: `curved_fuzz_sidecar_zombie_blocker`). If
  the worker cannot run it to completion, it will say so and NOT fabricate
  numbers; the driver reproduces the delta. The sidecar-backed RED oracle (#4)
  and YR15's sidecar oracle DO run here (binary present), giving real sphere-rim
  coverage regardless.

## 10. Deviation record

`docs/yang_deviations.md` gains **N11** (the propagated-band predicate —
selection + Stage-4 radial membership scaled by `R/r_circle`),
cross-referencing N10. `docs/yang_functional_roadmap.md` records PR-YR19 (the
sphere chord-band metric fix; approach A and its geometric justification
`dr ≈ (R/r_c)·d_sphere`); the remaining cone analytic-conic (Parabola/Hyperbola)
follow-up stays out of scope.
