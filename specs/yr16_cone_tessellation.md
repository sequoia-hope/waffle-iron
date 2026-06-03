# PR-YR16 — yang-rs cone Stage-1 tessellation (CONE only, no boolean)

Spec of record for the PR-YR16 role-separated FIP cycle. The cylinder (PR-YR7)
and sphere (PR-YR12) already tessellate watertight + bijective through
`BRep::new`. The **cone** is the last curved primitive that still rejects
everywhere (`Surface::Cone` → `CurvedSurfaceNotYetSupported`). This PR adds cone
Stage-1 tessellation + cone point-to-surface resolution so **all three curved
primitives tessellate**, verified by the same 4-part oracle used for the
cylinder/sphere. It mirrors PR-YR7 / PR-YR12 exactly.

## Hard scope (P9 / P10)

**CONE only. No boolean, no `ssi-rs`, no NURBS.** The cylinder + sphere + plane
paths stay **byte-for-byte**. The boolean-path cone rejections stay LOUD — the
cone never enters the boolean in this PR. The only three production behaviours
that change are at `BRep::new` Stage-1 dispatch, `eval_source`, and
`signed_distance_to_surface` (all enumerated below).

### STOP conditions

If apex/base watertightness or the tilted-normal winding can't pass the oracle
honestly, or `cone_chord_bound` sizing can't hold the radial residual — STOP and
report. No faked watertightness, no `d_ε` widening, no fallback paths.

---

## 1. B-Rep encoding (minimal — justified)

Unlike the cylinder (whose lateral is an annular tube needing a seam
`LineSegment`), the cone lateral is topologically a **disk**: its only boundary
is the base circle; the apex is a single interior singular point. So **no seam
edge** is needed.

Given `apex`, unit axis `â` (apex→base), `half_angle = α`, `height = h`:

- `R = h·tan(α)`; `base_center = apex + h·â`.
- `verts = [apex (v0), base_seam (v1)]` (mirrors the sphere's `[south, north]`).
  `base_seam` is any point ON the base rim (the fixture chooses an angle-0
  convention; the rim pre-pass recovers its azimuth `phi0`, so the exact choice
  is free).
- `edges = [e0 base rim Circle { center = base_center, normal = â, radius = R,
  start = 1, end = 1 }]` — one closed-loop Circle, **shared** by lateral + base
  cap (the watertightness mechanism; `start = end = v1`).
- `faces = [`
  - `f0 lateral Surface::Cone { apex, axis_dir, half_angle } outer_loop = [e0]`
    (the apex is interior — no edge references it),
  - `f1 base cap Surface::Plane { normal = â, d = −â·base_center } outer_loop = [e0]`
  `]`, both `reversed: false`.

### Apex identification

The apex lies on no edge (unlike sphere poles, recovered as `seam.start` /
`seam.end`). The apex IS pre-seeded as a B-Rep vertex (`verts` are seeded 1:1
into `out_verts` at the top of `BRep::new`). `tessellate_cone_face` locates the
pre-seeded apex mesh vertex by **exact position match** to `Surface::Cone.apex`
(within `cad_primitives::TAU_MODEL`); no match → loud
`YangError::MalformedTopology`. This keeps the apex a real `BRepVertex` shared by
the whole fan (no duplicate → watertight + Euler hold; round-trip maps it back
via `TessellationSource::BRepVertex`).

A cone face on a *triangle* (no base rim Circle in its loop) → loud
`MalformedTopology` (mirrors the cylinder/sphere "wrong boundary" rejection).

---

## 2. Tessellation = apex fan + base cap fan

The base rim Circle is collected by the existing rim-ring pre-pass (it is NOT a
sphere-seam edge), giving a shared cached ring `ring = [v1 (BRepVertex),
steiner_1, …, steiner_{N-1}]` with `ring[k]` at azimuth `phi0 + 2πk/N`.

- **Lateral** (new `tessellate_cone_face`): `N` triangles `apex → ring[k] →
  ring[(k+1) % N]`, reusing the cached ring → watertight with the cap. Pure fan,
  **no interior rings** (the cone is exact along each generator). Each triangle
  oriented outward via `cone_outward_normal` + the existing `orient_tri`.
- **Base cap**: reuses the existing `tessellate_cap_face` over the same ring —
  **no change** to that helper. Its center Steiner vertex = `base_center`,
  source `BRepFace { face: f1, u, v }` (the existing off-origin-cap solve already
  handles a non-world-origin rim center).

### Why no centroid amplification (no `/2` factor)

Because the cone is **ruled** (straight generators apex→rim, exactly on the
surface), all three vertices of every lateral fan triangle lie EXACTLY on the
cone (apex + two rim points). The only deviation is the flat triangle chord
across the curved lateral. At axial fraction `f∈[0,1]` from the apex, the
triangle cross-section is a chord of the circle of radius `f·R`, whose maximum
dip below the arc is `f·R·(1−cos(π/N)) ≤ R·(1−cos(π/N))` (worst at the base,
`f = 1`). So the worst residual anywhere on a lateral triangle — including its
centroid — is exactly the base-rim angular sagitta `R·(1−cos(π/N))`. There is no
centroid amplification (unlike the sphere, whose pole-fan triangles dip more than
their edge midpoints), hence **no `/2` factor** in the N-sizing.

The base cap is planar; its fan vertices (`base_center` + rim points) all lie
exactly in the cap plane, so the cap residual is identically 0.

---

## 3. Chord bound (new single-source helper)

```
cone_chord_bound(height, half_angle):
    R = height · tan(half_angle)
    return 1e-2 · sqrt((2R)^2 + height^2)
```

This mirrors `sphere_chord_bound` (`src/lib.rs:1485`) as the **single source**
(governance A14.3) of the cone's `1e-2` chord-bound literal: both the Stage-1
N-sizing (via the pre-pass, below) and the test-side oracle compute this exact
value, so they agree by construction.

### Pre-pass min-bound wiring

The rim-AABB `curved_chord_bound` (`src/lib.rs:1443`) for the cone's single rim
circle is `1e-2 · 2R√2` (the rim's AABB diagonal), which **ignores the height**
and can EXCEED the cone's honest bound for wide-short cones (`h < 2R`). Sizing N
from the rim bound alone would then permit a residual larger than
`cone_chord_bound`. So the pre-pass folds the cone bound in by taking the
**tighter (min)** `d_ε` whenever any `Surface::Cone` face is present:

```
d_eps = curved_chord_bound(&edges)          // existing rim-AABB bound (Some, since the rim exists)
if any face is Surface::Cone:
    for each cone face:
        derive height_f from the cone's apex + its rim circle:
            (rim_center, R) from the Circle edge in the cone face's outer_loop
            height_f = |(rim_center − apex) · â|     // â = unit(axis_dir)
        d_eps = min(d_eps, cone_chord_bound(height_f, half_angle))
```

`max_r` (the radius driving N) is unchanged — it is still the max rim radius,
which for the cone is `R`. Cylinder / sphere / all-planar inputs see **no cone
face** ⇒ the min branch is never entered ⇒ those paths stay **byte-for-byte**.

Because production's effective bound is `min(rim, cone) ≤ cone`, the actual
sized residual `R·(1−cos(π/N)) ≤ min(rim, cone) ≤ cone_chord_bound`. The
test-side oracle therefore asserts `residual ≤ cone_chord_bound(height,
half_angle)` and passes (production may use a smaller bound → finer mesh → still
within).

---

## 4. Winding — the one cone subtlety (governance A15.5)

The cone lateral's outward normal is **tilted** ⟂ the generator (NOT purely
radial like the cylinder). At a triangle centroid:

```
â  = unit(axis_dir)
w  = centroid − apex
r̂  = unit( w − (w·â)·â )          // unit radial component perpendicular to â
n̂  = unit( r̂ − tan(half_angle)·â )
```

Derivation: a cone point is `P = apex + s·â + s·tanα·r̂`; the generator direction
is `g = â + tanα·r̂`. The surface normal lies in `span{â, r̂}` (⟂ the
circumferential direction `â × r̂`). Writing `n = a·r̂ + b·â` and imposing
`n·g = 0` gives `b = −a·tanα`, so the outward (positive-radial, `a > 0`) normal
is `n̂ = unit(r̂ − tanα·â)`. Check: `n̂·g = (r̂ − tanα·â)·(â + tanα·r̂) = tanα −
tanα = 0`. ✓ Perpendicular to the generator.

New `cone_outward_normal(verts, tri, apex, axis_dir, half_angle)` helper (analog
of `radial_outward_normal` `src/lib.rs:1380` / `sphere_outward_normal`
`src/lib.rs:1363`), feeding the existing `orient_tri` (`src/lib.rs:1410`). The
fan-triangle centroid sits at `(apex + 2·rim)/3` (≈ 2/3 of the way to the rim),
so its radial component is ≈ `(2/3)R` — never degenerate near the apex. The
surface-to-mesh oracle (oracle 1) is the safety net for the sign.

---

## 5. Three Stage-1 production sites change (`crates/yang-rs/src/lib.rs`)

1. **`BRep::new` dispatch** (`src/lib.rs:647`): replace the `Surface::Cone { .. }
   => Err(CurvedSurfaceNotYetSupported)` arm with a call to
   `tessellate_cone_face(...)`. A cone-on-a-triangle (no base rim Circle) →
   `MalformedTopology` (mirrors cylinder/sphere).
2. **`eval_source`** (`src/lib.rs:842-843`): replace the apex-fallback
   `Surface::Cone { apex, .. } => apex` with the real cone FACE arm. For
   `BRepFace { face, u, v }` with a cone surface, `v` is the **axial** param
   (height from apex) and `u` is the **angular** param:
   ```
   (ê1, ê2) = ortho_basis(axis_dir)
   point(u, v) = apex + v·â + v·tan(half_angle)·(cos u·ê1 + sin u·ê2)
   ```
   The pure apex-fan emits **no** `BRepFace`-cone vertices, so this arm is not
   exercised by the round-trip oracle; it is given honest coverage by a focused
   `eval_source` cone-FACE unit test in `tests/yr16_cone.rs`.
3. **`signed_distance_to_surface`** (`src/lib.rs:1529`): replace the
   `Surface::Cone { .. } => Err(...)` arm with the SIGNED radial residual
   ```
   â = unit(axis_dir); w = point − apex
   h_axial = w·â
   radial  = | w − h_axial·â |
   Ok( radial − |h_axial|·tan(half_angle) )
   ```
   This is the cone's radial residual, SIGNED to match the cylinder/sphere
   convention (positive **outside** the lateral, negative inside, ≈ 0 on the
   surface). The classification oracle takes `.abs()` itself, so this is
   compatible; the signed form is the honest analog of the `Sphere`/`Cylinder`
   arms (the prior plan text's outer `|…|` is dropped because the function is
   `signed_distance` and its siblings are signed). LOUD `Ok` — never a panic or
   planar approximation.

### Boolean-path Cone sites STAY LOUD — do NOT touch

These keep rejecting the cone loudly (the cone never enters the boolean this PR):

- `surface_to_quadric` (`src/lib.rs:1840`) → `UnsupportedSurfaceForSsi`.
- `emit_topology` curved-output arm (`src/lib.rs:2541`) →
  `CurvedSurfaceNotYetSupported { face: fi }`.
- Stage-6 reassembly arm (`src/lib.rs:3579-3580`) →
  `CurvedSurfaceNotYetSupported { face: face_idx }`.

After this PR, `CurvedSurfaceNotYetSupported` is no longer reachable from
`BRep::new` for ANY of the three curved surfaces on a triangle (all become
`MalformedTopology`); the variant survives only on these boolean Stage-6 paths.

---

## 6. RED oracle contract — `tests/yr16_cone.rs`

Mirror `tests/yr7_cylinder.rs` (the cone, like the cylinder, has lateral +
planar-cap surfaces, so use the cylinder's surface-CLASSIFICATION oracle style —
classify each triangle to the unique surface all 3 vertices lie near, then bound
its samples).

`cone_brep(apex, axis_dir, half_angle, height) -> BRep` fixture per §1 (normalize
`axis_dir` internally so callers may pass a non-unit tilted direction). Four
surface-classified oracles + two unit tests:

1. **Surface-to-mesh distance ≤ d_ε** — classify each triangle to its surface
   (cone lateral vs base-cap plane) using a test-local distance copy; for the
   cone the residual is `radial − |h_axial|·tan(α)` (test-side reimplementation,
   independent of the production fn), for the cap the plane distance `n·x + d`.
   Sample the 3 vertices + centroid; assert each `.abs() ≤ d_ε`, with
   `d_ε = cone_chord_bound(height, half_angle)` computed test-side from params
   alone (identical literal to production, per §3).
2. **Watertight + 2-manifold** — every undirected edge shared by EXACTLY two
   triangles (apex + base included). Plus an env-gated `inputcheck` arm against
   the live Cherchi sidecar (self-skips on `SidecarError::BinaryNotFound`).
3. **Bijection round-trip** — `eval_source(map.lookup(v))` reproduces
   `mesh.verts[v]` for every mesh vertex (apex → `BRepVertex`; base_seam →
   `BRepVertex`; Steiner rim → `BRepEdge { e0, θ }`; cap center → `BRepFace { f1,
   u, v }`), tol `1e-9`.
4. **Euler** — `V − E + F = 2`.

Plus:
- A **`signed_distance_to_surface` cone unit test**: a 45° cone
  (`apex = origin`, `axis_dir = +z`, `half_angle = π/4`, so `tanα = 1`):
  - on-surface `(1,0,1)` → ≈ 0;
  - outside `(2,0,1)` → `+1` (positive);
  - inside `(0.5,0,1)` → `−0.5` (negative).
- An **`eval_source` cone-FACE-arm unit test**: build a `cone_brep`, then call
  `eval_source(BRepFace { face: 0, u, v })` for a chosen `(u, v)` and assert it
  equals the §5.2 formula evaluated test-side (since Stage-1 emits no such
  source, this is the arm's only coverage).

### Corpus of 4 (mirror the cylinder corpus shape)

| name | apex | axis_dir | half_angle | height |
|---|---|---|---|---|
| z-up unit | (0,0,0) | (0,0,1) | atan(1.0) (45°, R=1) | 1.0 |
| z-up wide-short | (2,−1,0.5) | (0,0,1) | atan(5.0/0.5) (R=5) | 0.5 |
| x-axis tall-thin | (−3,4,1) | (1,0,0) | atan(0.3/7.0) (R=0.3) | 7.0 |
| off-axis non-unit | (1,2,−1) | (1,2,2) (‖·‖=3) | atan(2.0/4.0) (R=2) | 4.0 |

(Choose `half_angle` so the listed `R = height·tan(α)` holds; the off-axis case
exercises normalization + `ortho_basis` on a non-unit axis — the key adversarial
case.) The wide-short case is the one that exercises the §3 `min` bound being
load-bearing.

---

## 7. Faithful guard migration (known multi-file sweep)

Per the `yang_curved_primitive_guard_migration` lesson — enabling a curved
primitive sweeps reject-guards across MANY test files. **The approved plan named
9 guard sites; this spec adds 3 more (+ 1 inline `src/lib.rs` test) discovered
during the spec sweep**, exactly the under-enumeration the YR15 cycle and the
lesson anticipate. This is NOT a P9/P10 STOP — the diagnosis (cone enable ⇒ guard
sweep) is correct; only the enumeration was incomplete. The RED author migrates
**all** of them, changing ONLY the expected outcome (error kind / Ok value) and
its comment, **preserving every structural assertion** (loud error, never silent
`Ok`; same fixtures; same control tests).

### (B) cone-on-triangle: `CurvedSurfaceNotYetSupported { face: N }` → `MalformedTopology(_)`

1. `tests/yr7_cylinder.rs::cone_face_still_rejected` (face 0).
2. `tests/yr6_adversary.rs::adversary_cone_face0_rejected_exact` (face 0).
3. `tests/yr6_adversary.rs::adversary_curved_face2_reports_index_2` — cone arm
   (was the `face: 2` witness). After migration ALL three arms assert
   `MalformedTopology(_)`. The indexed-rejection witness is **retired**
   (documented in the comment): no curved-on-triangle surface returns an indexed
   `CurvedSurfaceNotYetSupported` anymore — they are all seam/rim-malformed. The
   structural intent (curved face at a non-zero index still errors loudly, never
   silently `Ok`) is preserved.
4. `tests/yr7_adversary.rs::attack6_sphere_malformed_cone_still_curved_not_supported`
   — cone arm (face 0).
5. `tests/yr8_adversary.rs::attack4_sphere_malformed_cone_still_loudly_rejected`
   — cone arm (face 0).
6. `tests/yr12_adversary.rs::attack6_cone_on_triangle_still_curved_not_supported_face0`
   (face 0).
7. **`tests/yr8_curved_boolean.rs::t3_cone_face_still_loudly_rejected`** (face 0)
   — *plan-omitted; added here.*
8. **`tests/yr9_stage3_ssi.rs::t4_cone_face_still_loudly_rejected`** (face 0)
   — *plan-omitted; added here.*
9. **`src/lib.rs` inline `#[cfg(test)] mod tests::brep_new_rejects_cone_face`**
   (face 0) — *plan-omitted; added here.* This is the ONLY inline-`src` test
   affected. **The RED test-author edits this test (test code only, inside the
   `#[cfg(test)] mod tests` block); the GREEN implementer must NOT touch the test
   module.** Since the phases run sequentially (RED commits before GREEN), GREEN
   sees the already-migrated inline test and simply makes it pass.

### (C) `signed_distance_to_surface(Cone, …)`: `.is_err()` → `Ok(value)`

10. `tests/yr7_cylinder.rs::signed_distance_to_surface_sphere_ok_cone_reject`.
11. `tests/yr12_adversary.rs::attack6_signed_distance_sphere_ok_cone_err`.
12. **`tests/yr7_adversary.rs::attack7_signed_distance_sign_sanity`** (cone arm,
    "Cone must Err") — *plan-omitted; added here.*

For each, assert a concrete signed value from §5.3 (pick the cone so the
expected number is clean), not a bare `.is_ok()`.

### (D) STAYS UNCHANGED — verify, do NOT edit

- `tests/yr6_adversary.rs::adversary_curved_never_ok` — iterates
  `[sphere, cylinder, cone]` on a triangle, asserts all `is_err()`. After
  migration cone-on-triangle is still `MalformedTopology` (still err) → no change.
- `tests/yr15_adversary.rs::adv_box_faces_outward_no_cone_and_deterministic`
  (boolean OUTPUT has no Cone face) — no boolean is added → stays TRUE.
- The `yr8_adversary` / `yr14_adversary` boolean-output "no Sphere/Cone faces"
  assertions stay TRUE and unchanged.

---

## 8. Production helpers GREEN adds (`src/lib.rs`)

- `tessellate_cone_face(f_idx, f, edges, rim_rings, verts, apex, axis_dir,
  half_angle, out_verts, sources, out_tris)` — locate apex by `TAU_MODEL` match,
  fan over the cached rim ring, orient via `cone_outward_normal` + `orient_tri`.
  Wrong/missing rim → `MalformedTopology`.
- `cone_chord_bound(height, half_angle)` — §3 single source.
- `cone_outward_normal(verts, tri, apex, axis_dir, half_angle)` — §4.
- Pre-pass min-bound wiring — §3 (guarded on "any cone face present").

GREEN never touches tests (including the inline `#[cfg(test)]` module — RED owns
the inline `brep_new_rejects_cone_face` migration).

---

## 9. Verification (full crate gate)

A Stage-1 change can regress siblings, so the FULL crate gate is required:

- `cargo test -p yang-rs` — new `tests/yr16_cone.rs` 4 oracles + 2 unit tests
  green; all migrated guards green; cylinder/sphere/planar tests byte-for-byte
  unchanged (regression check on `tests/yr7_cylinder.rs`, `tests/yr12_sphere.rs`,
  `tests/fuzz_boxes.rs`).
- `cargo fmt -p yang-rs -- --check` — clean.
- `cargo clippy -p yang-rs --all-targets -- -D warnings` — clean.
- `inputcheck` arm runs against the live Cherchi sidecar present in this env
  (self-skips on `BinaryNotFound`).

## 10. Adversary contract

A THIRD distinct sub-agent:

- Independently re-derive the §4 tilted normal and witness outward sense on a
  SECOND off-axis mock (distinct apex/axis/half-angle from the corpus).
- Mutation-verify BOTH the `cone_chord_bound` pre-pass `min` wiring AND the
  tilted normal are load-bearing — each mutation must red a DISTINCT yr16 oracle
  (e.g. dropping the `min` reds the wide-short oracle 1; replacing the tilted
  normal with the pure radial `r̂` reds an oriented-winding check on a steep
  cone). Revert all mutations; final tree clean.
- Verify all §7 migrations were not weakened (every structural assertion intact;
  no guard downgraded to a bare `is_err()`/`is_ok()`).
- Non-destructive git only (no stash/checkout/reset on the live tree).

## 11. Close-out

Update `docs/yang_functional_roadmap.md` (PR-YR16 — cone Stage-1 tessellation;
all three curved primitives now tessellate; next: PR-YR17 cone cavity
`box − cone`). Commit. Push to `origin/main`.
