# PR-YR11 (Stage 4, oblique) — yang-rs: relocate mesh intersection points onto the exact ELLIPSE (oblique cylinder ∪ box)

> Manager spec of record for the role-separated FIP cycle (Constitution P5):
> Spec (this doc) → RED (test-author sub-agent) → GREEN (implementer sub-agent)
> → Adversary (fourth sub-agent). The implementer NEVER edits tests; the test
> author NEVER writes production code; the adversary uses independent oracles
> that share no code with GREEN. Stay on `main`; commit each phase; push at end.
> Paper citations are line ranges in `refs/text/yang2025_hybrid_boolean.txt`.

## 1. Objective

PR-YR10 made Stage 4 relocate mesh intersection crossings onto the exact
**circle** for the perpendicular cap of `cylinder ∪ box`, with §4.5.3
reversed-point correction, inherited watertightness, and a no-skip audit. It
**loudly STOPs on oblique cuts**: an `Ellipse` intersection edge →
`Err(Stage4RegionInvalid { reason: EllipseProjectionUnsupported })`
(`crates/yang-rs/src/lib.rs:2300-2305`).

This PR lifts that STOP for the oblique `cylinder ∪ box` case, where a box face
cuts the cylinder at an angle and `plane ∩ cylinder` is an **ellipse**. ssi-rs
(PR-SSI2) already solves it and P3 already emits `Curve::Ellipse`
(`ssi_rs::plane_cylinder` C2 branch → `ssi_curve_to_curve`, `lib.rs:1355-1367`).
**There is no P3 gap.** The work is entirely in Stage 4 + the edge-source
evaluation. Paper basis: Yang §4.4.1 (mesh updating / relocation) + §4.3.2
(parametric surface relocation) + §4.5.3 (reversed-point correction). Analytical
primacy (Invariant A15): mesh is the exact topology tool; analytical curves
survive the pipeline.

## 2. The crux — relocate via the cylinder parameterization (closed-form, no quartic)

Do **NOT** compute the ambient "nearest point on the 3D ellipse" (needs a
quartic / iteration — forbidden, P9/P10). Relocate using the **cylinder's own
parameterization** (Yang §4.3.2). A crossing point must end on BOTH the cylinder
(radius `r` about its axis) AND the cutting plane:

1. Snap to the cylinder lateral surface: keep the point's **angle θ** about the
   axis (its radial direction), set its radial distance to `r`.
2. Snap the **axial coordinate** so the point also satisfies the plane
   `n·x + d = 0` (one linear solve along the axis at the fixed radial direction).

The true `Surface::Cylinder { axis_point, axis_dir, radius }` and the cutting
`Surface::Plane { normal, d }` are both available per edge from the **incidence
map** returned by `compute_phase_a` (`lib.rs:2175-2189`), which Stage 4 currently
discards as `_inc0` (`lib.rs:2282`). Use the true cylinder; no reconstruction
from the ellipse, no sign ambiguity.

Closed form (cylinder `Q = axis_point`, unit `â = axis_dir`, radius `r`;
plane unit `n`, offset `d`):

```
w        = p − Q
along    = w·â
radial   = w − along·â ;   ρ = |radial|        (ρ < MIN_FEATURE_SIZE ⇒ Err(OnAxis))
rdir     = radial / ρ
s        = −( n·Q + r·(n·rdir) + d ) / (n·â)    (n·â ≠ 0 for oblique;
                                                 |n·â| < MIN_FEATURE_SIZE
                                                 is the out-of-scope
                                                 axis-parallel / line case)
x        = Q + s·â + r·rdir                      (on cylinder AND on plane
                                                  ⇒ on the exact ellipse)
```

Then compute the **ellipse parameter `t`** so `BRepEdge { edge, t }` round-trips.
With `minor_dir = normal × major_axis`,
`u = (x − C)·major_axis`, `v = (x − C)·minor_dir`,
`t = atan2(v / minor_radius, u / major_radius)`.
Because `x` is exactly on the ellipse, `eval_source` reproduces it to machine
precision.

## 3. Critical coupling — ONE shared ellipse frame (analogous to `ortho_basis` for circles)

The ellipse parameterization
`C + major_radius·cos t·major_axis + minor_radius·sin t·minor_dir`
(with `minor_dir = normal × major_axis`) MUST be used **identically** in all
three places, or `t` drifts and the round-trip oracle fails:

- relocation's `t` computation (§2 above),
- `eval_source` for `Curve::Ellipse` (`lib.rs:728-732`, currently a defensive
  `center` stub),
- `is_reversed`'s ellipse tangent.

Add **one** `ellipse_frame` / `ellipse_param` / `ellipse_point` helper set and
call it from all three. Match the existing `curve_contains_point` Ellipse
convention (`lib.rs:1414-1438`): `minor_dir = normalize(normal) × normalize(major_axis)`.

## 4. Files to change (production — GREEN implementer only). All in `crates/yang-rs/src/lib.rs`.

1. **`stage4_relocate_and_correct` (`lib.rs:2260-2372`)** — the core change.
   - Use the incidence map (stop discarding `_inc0`). For each `Curve::Ellipse`
     edge, look up its two incident surfaces, identify the `Surface::Cylinder`
     and the `Surface::Plane`, and build a per-vertex
     `vert_ellipse: BTreeMap<u32, EllipseReloc>` carrying
     `(cyl axis_point, axis_dir, radius, plane n, plane d, ellipse center,
     normal, major_axis, major_radius, minor_radius)` — analogous to
     `vert_circle`.
   - **Replace** the `Curve::Ellipse => return Err(EllipseProjectionUnsupported)`
     STOP (`lib.rs:2300-2305`) with collection into `vert_ellipse`.
   - Add an ellipse relocation loop mirroring the circle loop
     (`lib.rs:2319-2340`): gate on `ellipse_residual(p) > d_eps` ⇒
     `Err(OffCurveBeyondChordBand)`; relocate via the closed form; move only when
     residual `> TAU_WORK`; push `(v, t)`; insert into `processed`. Reuse the
     existing `d_eps = stage4_chord_band(a, b)` (same cylinder tessellation ⇒
     same chord band).
   - The **no-skip audit** (`lib.rs:2342-2350`): `endpoint_set` / `processed` /
     `relocation_keys` must be the **union** of circle + ellipse endpoints. If a
     vertex appears in BOTH `vert_circle` and `vert_ellipse` (two different
     curves through one vertex), that is a genuine ambiguity ⇒ LOUD STOP (do not
     relocate twice). Out of scope for oblique cyl∪box, but guard it rather than
     silently picking one (suggest reusing `LocalRefinementRequired`; do NOT add
     a silent pick).
   - New helpers:
     `project_onto_ellipse_via_cylinder(p, cyl, plane, ellipse_frame) -> Result<(Point3, f64), Stage4InvalidReason>`
     and `ellipse_residual(p, cyl, plane) -> f64`
     (`max(|dist(x, axis) − r|, |n·x + d|)` — the on-both-surfaces residual,
     matching the RED Oracle 1).

2. **`eval_source` `Curve::Ellipse` arm (`lib.rs:728-732`)** — replace the
   `center` stub with the shared ellipse-point evaluation
   `C + a·cos t·major + b·sin t·minor_dir`.

3. **`is_reversed` (`lib.rs:2487-2561`)** — keep the degenerate-tangent branch
   (`|t̃| < TAU_WORK ⇒ true`, the N3 fix from commit `a0ba8f59`) **byte-for-byte**.
   Generalize the curve lookup so an `Ellipse`-bearing edge computes the
   **ellipse tangent** at `p_r` (`−a·sin t·major + b·cos t·minor_dir`,
   normalized) instead of only the circle tangent.

4. **Reversal sweep `all_circle` filter (`lib.rs:2423-2426`)** — widen to
   `all_conic` (every edge is `Circle` OR `Ellipse`; still **exclude**
   `LineSegment`).

5. **TessellationMap update (`lib.rs:2936-2955`)** — the relocated-vertex →
   `BRepEdge { edge, t }` override currently searches for the first incident
   `Curve::Circle` edge; widen the predicate to `Circle | Ellipse` so
   ellipse-relocated vertices get the override too.

`validate_relocated_triangles`, `check_watertight_2manifold`, `collapse_vertex`,
`stage4_chord_band` are curve-agnostic and reused unchanged. Keep the
`EllipseProjectionUnsupported` enum variant (it is `pub`, harmless); it is simply
no longer reached for the oblique case. Sphere/Cone still reject loudly via their
own paths; the `LocalRefinementRequired` (§4.5.2) STOP stays.

## 5. Test changes (RED author only)

New file `crates/yang-rs/tests/yr11_stage4_ellipse.rs`, mirroring the 5-group
oracle layout of `yr10_stage4_relocate.rs`. Per the established repo convention
(integration-test files cannot share helpers), re-declare the needed harness
(`p`, array math, `cylinder_brep`, `canonical_box`, `oblique_cylinder`,
`surface_to_quadric`, `d_eps`, `LabelMock`, `hand_built_*_arrangement`,
`unpaired_half_edges`, `euler_characteristic`, ssi-rs oracle helpers) verbatim
from yr10. Tolerances (do NOT weaken): on-curve / round-trip / after-deviation =
`cad_primitives::TAU_MODEL` (1e-7); selection band = `d_ε` via `d_eps(...)`;
angular = 1e-6 rad.

1. **On exact ellipse to TAU**: each relocated crossing has cylinder radial
   residual `|dist(x, axis) − r| ≤ TAU_MODEL` AND plane residual `|n·x + d| ≤
   TAU_MODEL` (independently recomputed from the fixture's true cylinder/plane —
   NOT via production code).
2. **Oblique cyl∪box now SUCCEEDS** (no `EllipseProjectionUnsupported`) and the
   output carries `Curve::Ellipse` intersection edges. Independent ssi-rs oracle
   confirms the section is an Ellipse.
3. **Chord deviation strictly decreases** vs the pre-Stage-4 polyline. Build a
   genuinely off-curve oblique ring fixture (analogous to YR10's
   `hand_built_offcurve_tube_arrangement`) with pre-Stage-4 max deviation
   ≫ TAU_MODEL, post ≤ TAU_MODEL.
4. **Watertight 2-manifold** (0 unpaired half-edges, Euler χ = 2) AND no
   reversed / inverted / degenerate triangles; loop order matches the ellipse
   tangent.
5. **Bijection round-trips** (ellipse `BRepEdge { edge, t }` via `eval_source`
   ≤ TAU_MODEL); **determinism** (two runs byte-identical); a sidecar-independent
   direct path for the GREEN gate, plus an **env-gated** sidecar E2E with a
   **LOUD skip** (panic/`eprintln!`-style skip notice, never a silent pass).

**Faithful contract migration** (P5 / standing rule): migrate the now-obsolete
expectation in `yr10_stage4_relocate.rs::t4_ellipse_edge_rejected_loudly`
(lines 1140-1181): change ONLY the expected outcome (was:
`Err(...EllipseProjectionUnsupported)`; now: succeeds + produces `Curve::Ellipse`,
relocated points on the exact ellipse) while preserving every structural
assertion — the independent ssi-rs "section is an Ellipse" oracle (lines
1142-1162) stays verbatim. Confirm no OTHER test asserts the ellipse-rejection
(the `Ellipse` hits in `yr9_adversary.rs` / `yr6_adversary.rs` are ssi-rs /
chord-bound checks, not Stage-4 rejection — verify and leave them unchanged). Do
NOT weaken or delete the circle-path (YR10) or planar (`fuzz_boxes`) tests.

## 6. Hard scope (held by Spec, verified by Adversary)

- Oblique `cylinder ∪ box` (plane∩cylinder → ellipse) ONLY. Circle/perpendicular
  path (YR10) stays **byte-for-byte** semantically (the relocation loop is
  generalized but the circle branch is unchanged).
- `§4.5.2` local refinement stays a loud `Err(LocalRefinementRequired)` STOP.
- Axis-parallel / degenerate-line sections out of scope (the
  `|n·â| < MIN_FEATURE_SIZE` guard rejects rather than dividing by ~0).
- Sphere/Cone still reject loudly via their own paths.
- No global CDT, no Newton / iterative projection, never skip an edge, no
  tolerance widening (P9/P10).

## 7. Adversary (fourth sub-agent — independent oracles, no shared code with GREEN)

- Re-derive the ellipse from first principles (hand-computed `a = r/|cos tilt|`,
  `b = r`, `major_axis`, center = axis ∩ plane) — NOT via `ssi_rs::intersect` —
  and assert every relocated crossing lies on it and on the true cylinder/plane
  ≤ TAU_MODEL.
- In-plane fold disproof (winding-number / signed-area sweep on the elliptical
  cap), as YR10 adv1/adv2, generalized to the ellipse.
- Relocated ring is a simple, once-wrapping inscribed polygon ordered along the
  ellipse tangent.
- Independently verify the YR10 migration was not weakened (structural
  assertions intact).
- Confirm scope: circle/perpendicular YR10 unregressed; `fuzz_boxes`
  unregressed; sphere/cone + §4.5.2 still loud-STOP.

## 8. CI gate (FULL crate, all clean)

```
cargo test -p yang-rs
cargo fmt -p yang-rs -- --check
cargo clippy -p yang-rs --all-targets -- -D warnings
```

## 9. On completion (Manager)

Update `docs/yang_functional_roadmap.md`: PR-YR11 done (Stage 4 oblique —
relocate onto the exact ellipse via the cylinder parameterization; oblique
cyl∪box now conforms). Note remaining: §4.5.2 local refinement, sphere (P2b),
curved Subtract, broader SSI pairs. Commit docs/RED/GREEN/adversary phases
separately with the `Co-Authored-By: Claude Opus 4.8 (1M context)` trailer; push
to `origin/main` at the end.
