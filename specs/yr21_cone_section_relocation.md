# PR-YR21 — Cone-section relocation foundation + cone∩plane ELLIPSE

**Crate:** `crates/yang-rs/`
**Stage:** Yang 2025 §4.4.1 (mesh updating / relocation) + §4.3.2 (parametric
surface relocation), the cone analog of PR-YR11's oblique-cylinder ellipse.
**Roadmap:** first step of the cone analytic-conic sequence PR-YR21→YR24
(`docs/yang_functional_roadmap.md`).

## 1. Problem

The curved boolean fuzz is `ok_correct ≈ 61/90` after PR-YR20, but **cone is
`0/26`** — every non-perpendicular cone section is blocked. The analytic SSI math
is already DONE in `ssi-rs` (`plane_cone` returns `Circle` / `Ellipse` /
`Parabola` / `Hyperbola`). This is purely a `yang-rs` Stage-4 integration gap.

### Confirmed root cause (code reading, not assumed)

- A cone∩plane **oblique** cut already produces a correct **output edge**:
  `ssi_rs::intersect(Plane, Cone)` → `SsiCurve::Ellipse` →
  `ssi_curve_to_curve` (`lib.rs:2334`) → `Curve::Ellipse`, and
  `build_intersection_curves` (`lib.rs:2552`) selects it (cone selection tol via
  `cone_chord_tol_for_owner`, YR17; `surface_to_quadric`/`signed_distance_to_surface`
  already handle `Surface::Cone`). The edge-curve build is **not** the gap.
- The keystone gap is **Stage-4 relocation**. `stage4_relocate_and_correct`'s
  `Curve::Ellipse` arm (`lib.rs:3583`) scans the edge incidence for a
  `Surface::Cylinder` + `Surface::Plane` (YR11). A cone+plane ellipse edge has
  `(cyl = None, plane = Some)` → the `let-else` at `lib.rs:3611` fails → returns
  `Stage4RegionInvalid { LocalRefinementRequired }` (`lib.rs:3616`). So a cone
  ELLIPSE fails even though everything upstream is correct.
- Parabola/Hyperbola cone cuts fail **earlier** at selection
  (`ssi_curve_to_curve` rejects those variants; `curve_contains_point` returns
  `false` → `AmbiguousCurve`). Those are **PR-YR22/YR23, out of scope here**.

### Secondary site (fix-all-gates-sharing-a-metric / YR15 lesson)

The Stage-4 relocation **budget** `input_curved_chord_bound` (`lib.rs:3383`) maxes
only the rim-AABB `curved_chord_bound` + `sphere_chord_bound` — it has **no cone
term**. For tall-thin cones (`h > 2R`) the rim-AABB underestimates
`cone_chord_bound`, so a cone relocation must derive its residual budget from
`cone_chord_bound` (`lib.rs:1920`, the single source) — **not** the generic band.
The cone relocation loop computes its own per-cone-face budget so the
cylinder/sphere `stage4_chord_band` path stays **byte-identical**.

## 2. Intended outcome

Cone ELLIPSE cuts produce correct `Ok` booleans (cone-ellipse `ok_correct` rises;
cone-ellipse `LocalRefinementRequired` → 0), with **ZERO new silent-wrong** and
every prior case **byte-identical**.

## 3. Design

### 3.1 New helper — `project_onto_cone_section` (type-agnostic)

The cone analog of YR11's `project_onto_ellipse_via_cylinder` (`lib.rs:2093`),
mirroring its structure (avoids the generic foot-of-perpendicular quartic):

```rust
fn project_onto_cone_section(
    p: Point3, apex: Point3, axis_dir: Vector3, half_angle: f64,
    plane_n: Vector3, plane_d: f64,
) -> Result<Point3, Stage4InvalidReason>
```

1. `â = unit(axis_dir)`, `n = unit(plane_n)`, `d` normalized to `n` (reuse the
   same defensive `n_len` normalization as `project_onto_ellipse_via_cylinder`).
2. `w = p − apex`; `axial = w·â`; `radial = w − axial·â`; `ρ = |radial|`.
   `ρ < MIN_FEATURE_SIZE` → `Err(OnAxis)`. `r̂ = radial/ρ`
   (frame-independent — equals `cosθ·û + sinθ·ŵ` for any orthonormal in-plane
   basis, so no explicit basis is needed; consistent with the cone tessellation /
   `plane_cone` radial convention by construction).
3. nappe sign `= sign(axial)`; unit generator
   `g = nappe·cosα·â + sinα·r̂` (`α = half_angle`). `|g| = 1` by construction.
4. `n·g`: `|n·g| < MIN_FEATURE_SIZE` → `Err(LocalRefinementRequired)`
   (generator parallel to the plane — the asymptotic / parabola-tail direction,
   out of scope). `s = −(n·apex + d) / (n·g)`. `s ≤ 0`
   (apex-coincident / wrong-nappe) → loud STOP (reuse `LocalRefinementRequired`).
   `proj = apex + s·g`. Return `Ok(proj)`.

Returns only the relocated 3D point (type-agnostic — reused unchanged by the
YR22/YR23 parabola/hyperbola arms). Each conic type does its own param inversion.
The relocated point lies on BOTH the cone (it is on a generator at `p`'s azimuth)
AND the plane (`n·proj + d = 0`), hence exactly on `plane ∩ cone` = the ellipse.

### 3.2 Wire into Stage 4 (`stage4_relocate_and_correct`, `lib.rs:3583` arm)

- In the `Curve::Ellipse` arm, **additionally** scan `inc0` for `Surface::Cone`.
  Branch on the incidence:
  - cylinder + plane → **existing YR11 path verbatim** (`EllipseReloc` →
    `vert_ellipse`); cylinder relocation stays **byte-for-byte unchanged**.
  - cone + plane → build a new cone-ellipse reloc record (apex, axis,
    half_angle, plane n/d, stored ellipse params, and the cone's own
    `cone_chord_bound` budget) → a new `vert_cone_ellipse` map.
  - neither → the existing `LocalRefinementRequired` loud STOP (`lib.rs:3616`).
- New relocation loop after the cylinder-ellipse loop (`lib.rs:3702`), mirroring
  it: `proj = project_onto_cone_section(...)`; residual `= |p − proj|`; gate
  `residual > cone_d_eps` → `OffCurveBeyondChordBand`;
  `t = ellipse_param(proj, …stored ellipse…)` (`lib.rs:1068`) so the unchanged
  `eval_source` `Curve::Ellipse` arm (`lib.rs:859`) round-trips; move the vertex
  iff `residual > TAU_WORK`; push to `relocations` / `processed` exactly as the
  cylinder loop does. The shared §4.5.3 reversal sweep + watertight gate
  (`lib.rs:3731`+) handle it unchanged (they already iterate `Circle | Ellipse`
  loops).
- A vertex shared by a cone-ellipse AND any other conic edge (cylinder-ellipse,
  circle) is a genuine ambiguity → loud STOP (extend the existing dual-curve
  audit at `lib.rs:3646`).

### 3.3 `cone_d_eps` single source

Derived from `cone_chord_bound(height, half_angle)` (`lib.rs:1920`), with
`height` from the cone owner's rim `Curve::Circle` — the **same** derivation as
`cone_chord_tol_for_owner` (`lib.rs:2486`) and `tol_for` (`lib.rs:3144`):
`height = |(rim_center − apex)·â|`. Producer fault (cone owner has no rim Circle)
→ loud `LocalRefinementRequired`, never a `TAU_WORK` default (P10). Keep
`stage4_chord_band` / `input_curved_chord_bound` untouched for the
cylinder/sphere paths (byte-identity).

### 3.4 Untouched (byte-identity required)

`eval_source`, `ellipse_point/param/frame`, `project_onto_ellipse_via_cylinder`,
the cylinder path, `build_intersection_curves` (cone-ellipse selection already
works via YR17), and all YR8–YR20 + `fuzz_boxes` paths.

## 4. RED contract (`tests/yr21_cone_ellipse.rs`)

A deterministic hand-built cone+plane **ellipse** `LabeledArrangement` mock (NO
rand / system time / FS — same `LabelMock` / `build_tube_from_3d_rings` /
`cone_brep` patterns as `tests/yr11_stage4_ellipse.rs` + `tests/yr17_subtract_cone.rs`),
currently failing with `Stage4RegionInvalid { LocalRefinementRequired }`. The
fixture must be an **oblique** cone section (plane inclination to the cone axis
`θ ∈ (α, 90°)` strictly — an ellipse, not a circle, parabola, or hyperbola),
confirmed by an **independent** `ssi_rs::intersect(plane, cone)` oracle that the
section really is `SsiCurve::Ellipse`. Tolerances mirror YR11: on-conic /
round-trip ≤ `TAU_MODEL` (1e-7); off-band band via the cone chord bound; angular
1e-6 rad. The on-conic oracle (on the cone radial residual AND the plane
residual) is recomputed **independently of production**.

Oracles (assert the correct post-fix behavior):

1. `boolean(...)` returns `Ok` with ≥1 `Curve::Ellipse` intersection edge (no
   `Stage4RegionInvalid { LocalRefinementRequired }` STOP).
2. Every relocated intersection-edge vertex is on the exact ellipse: cone radial
   residual ≤ `TAU_MODEL` AND plane residual ≤ `TAU_MODEL`, recomputed
   independently from the fixture's true cone/plane.
3. Max chord deviation strictly DECREASES (before ≫ `TAU_MODEL`, after ≤ `TAU_MODEL`)
   — proves real relocation, not a no-op (off-curve fixture in the relocate band).
4. Watertight 2-manifold, `χ = 2 − 2g`, no inverted/degenerate triangles.
5. Relocated verts carry `TessellationSource::BRepEdge { edge, t }` round-tripping
   via the exact ellipse parameterization to the relocated position ≤ `TAU_MODEL`;
   determinism (two runs byte-identical).
6. An **axis-parallel / asymptotic** cone-section fixture (generator parallel to
   the plane, `θ ≤ α`) MUST still STOP loudly (`LocalRefinementRequired`).

## 5. Adversary contract (`tests/yr21_adversary.rs`)

Independent of RED. Must hold:

- The YR11 cylinder-ellipse tests (`yr11_stage4_ellipse`) are **byte-for-byte**
  unchanged in behavior (the cylinder relocation path is untouched).
- `SILENT_WRONG` stays 0 (no case that previously STOPped now returns a wrong
  `Ok`; no new silent-wrong introduced).
- The new loud STOPs are mutation-load-bearing: removing the cone budget gate or
  the `s ≤ 0` / `|n·g|` guards is caught.
- Any faithful contract migration is not weakened.

## 6. Scope / loud STOPs held

- Cone **ELLIPSE only** (`Curve::Ellipse` already exists upstream).
- Parabola/Hyperbola stay LOUD (`AmbiguousCurve`) — YR22/YR23.
- Axis-parallel / through-apex stay LOUD (`LocalRefinementRequired`).
- Cylinder ellipse (YR11), cone perpendicular circle (YR17), all planar
  (`fuzz_boxes`), and YR8–YR20 demos stay **byte-identical**.

## 7. Files

- `crates/yang-rs/src/lib.rs` — `project_onto_cone_section` (new), the
  `Curve::Ellipse` Stage-4 arm + new cone relocation loop, the cone reloc record
  + budget helper. (GREEN agent only.)
- `crates/yang-rs/tests/yr21_cone_ellipse.rs` — RED tests. (RED agent only.)
- `crates/yang-rs/tests/yr21_adversary.rs` — Adversary tests. (Adversary only.)
- `specs/yr21_cone_section_relocation.md` — this spec. (Manager.)
- `docs/yang_functional_roadmap.md`, `docs/yang_deviations.md` — close-out.

## 8. CI gate

- `cargo test -p yang-rs` (whole crate green; YR11 cylinder-ellipse byte-for-byte;
  cone-ellipse `LocalRefinementRequired` → 0; ZERO new silent-wrong).
- `cargo fmt -p yang-rs -- --check`.
- `cargo clippy -p yang-rs --all-targets -- -D warnings`.
- Sidecar fuzz delta reported honestly, or deferred to the driver if the
  curved-fuzz sidecar-zombie blocker prevents completion (no fabricated numbers).
