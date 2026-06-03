# PR-YR17 — curved `Subtract`: box − cone CONICAL POCKET (genus 0, χ=2)

**Milestone:** M5 / Phase 2 step (curved `Subtract` topology — the fourth M5
increment after PR-YR13's blind cylinder pocket, PR-YR14's through-hole, and
PR-YR15's hemispherical sphere dimple).
**Predecessors:** PR-YR13 (`box − cylinder` BLIND POCKET, genus 0, cavity-sense
via `BRepFace.reversed`); PR-YR14 (through-hole, genus 1, per-shell Euler gate
χ = 2 − 2g); PR-YR15 (`box − sphere` hemispherical dimple, genus 0); PR-YR16
(cone Stage-1 tessellation — all three curved primitives now tessellate, but the
cone is still LOUDLY rejected everywhere on the boolean path).
**Crate:** `crates/yang-rs/` only. **No `ssi-rs` change expected.**

## Goal (narrow, single cycle)

Extend the curved `Subtract` cavity path to a **conical** cavity: a cone with its
**apex inside the box** (the pocket bottom) and its **base above the box top**, so
the cone exits **through the box-top plane only** and `box − cone` carves a
**conical pocket**. The result is:

- a single connected, closed, orientable 2-manifold shell of **genus 0 → χ = 2**
  (a box with a conical pocket, topologically still a sphere);
- exactly **ONE** exact `Circle` rim — `cone ∩ box-top plane`, a circle because
  the cut is **perpendicular** to the cone axis (plane ⟂ axis → ssi-rs `plane_cone`
  C1 branch → `SsiCurve::Circle`);
- a cavity wall that is the **cone lateral** from the apex up to the rim
  (`Surface::Cone`, `reversed == true`), whose effective outward normal points
  **into the pocket** (away from box material);
- the box-top face that the cone pokes through becomes an **annular planar face**
  (the box-top plane minus the rim disk) — handled by PR-YR5c's outer+inner-loop
  machinery unchanged;
- an **apex singular vertex** at the pocket bottom that closes the lateral patch
  cleanly (a single boundary cycle = the rim; the apex is an interior point of the
  patch, referenced by no edge — exactly as in the Stage-1 cone tessellation,
  PR-YR16 §1).

## Target geometry (the one in-scope case)

- Box A: axis-aligned `[-2,-2,0] .. [2,2,2]` (top face at z = 2).
- Cone B: apex `(0,0,0.5)` **inside** the box, axis `+Z`, `half_angle` chosen so
  the base lies above the box top. With the cut at z = 2 the rim radius is
  `R_rim = (2 − 0.5)·tan(half_angle) = 1.5·tan(half_angle)`. Choose
  `half_angle = atan(1.0/1.5)` so `R_rim = 1.0` (a clean unit rim).
- Rim = cone ∩ box-top plane = **perpendicular cut → exact `Circle`**: center on
  the axis at `(0,0,2)`, normal `+Z`, radius `R_rim = 1.0`, in the plane z = 2.
- Result: box with conical pocket. Cavity wall = cone lateral apex→rim;
  `Surface::Cone` with `reversed == true`. Box faces `Plane`, `reversed == false`.

## What already works (reuse unchanged — confirm, don't rebuild)

- **Stage 1 cone tessellation** (PR-YR16): `tessellate_cone_face`, the apex-fan,
  the cached rim ring, `cone_outward_normal`, `cone_chord_bound`, the pre-pass
  `min`-bound tightening (`src/lib.rs:499–526`). The cone primitive tessellates
  watertight + bijective through `BRep::new`.
- **Cone point-to-surface distance** (PR-YR16): `signed_distance_to_surface`
  Cone arm (`src/lib.rs:1757`) returns `Ok` (signed radial residual
  `radial − |h_axial|·tanα`) → so the `plane_dist` closure (`src/lib.rs:2741`)
  ALREADY succeeds for a Cone face. **The stale comment at `src/lib.rs:2786-2787`
  claiming the cone is rejected by `plane_dist` upstream is PRE-this-PR — since
  PR-YR16 made `signed_distance_to_surface(Cone)` return `Ok`, `plane_dist` no
  longer rejects the cone; the LIVE reject is the `tol_for` Cone arm at
  `src/lib.rs:2788`.** Correct the comment in passing.
- **Cavity-sense flag**: `reversed: op == BoolOp::Subtract && info.input == InputId::B`
  (`src/lib.rs:3815`) — already surface-agnostic; no change. This is exactly the
  mechanism YR13/YR15 use; the cone reuses it unchanged.
- **ssi-rs**: `QuadricSurface::Cone { apex, axis_dir, half_angle }` is
  **field-for-field identical** to yang's `Surface::Cone`. The `intersect`
  dispatch routes `Plane ∩ Cone → plane_cone` (`crates/ssi-rs/src/lib.rs:369-372`);
  the `plane_cone` **C1 branch** (`crates/ssi-rs/src/lib.rs:803-812`) returns
  `SsiCurve::Circle { center, normal: ahat, radius: |h|·tanα }` for the
  perpendicular cut. **No ssi-rs change.**
- **Stage 3/4 conic path**: `ssi_curve_to_curve` Circle arm (`src/lib.rs:2096+`)
  already maps `SsiCurve::Circle → Curve::Circle` field-for-field;
  `curve_contains_point` Circle (`src/lib.rs:2133+`) and the Stage-4 circle
  relocation are `Circle`-generic — the rim relocation reuses them with **no new
  Stage-4 code**.
- **Annular planar face** (box-top with a circular hole): the PR-YR5c
  multi-cycle / outer+inner-loop machinery in the PLANAR branch — reused unchanged.
- **Per-shell Euler gate**: `check_watertight_2manifold` already accepts
  χ = 2 − 2g (PR-YR14); a genus-0 conical pocket is χ = 2, the easy case.

## Production edits (GREEN — four faithful extensions, `src/lib.rs` only)

The cone is loudly rejected at exactly **four** boolean-path sites. Each mirrors
the existing `Cylinder` / `Sphere` precedent. No new mechanism.

1. **`emit_topology` curved branch guard** (`src/lib.rs:3751`): broaden
   `matches!(inherited, Surface::Cylinder { .. } | Surface::Sphere { .. })` to
   ALSO admit `Surface::Cone { .. }`. The branch body (push_loop, E2 degenerate
   guard, outer/inner loop assignment, `reversed` flag, `surface: inherited`) is
   already surface-agnostic — **no body change**. Confirm it composes for a cone
   cavity: the cone-lateral patch has a SINGLE boundary cycle (the rim) — it
   becomes the outer loop, no inner loops; the apex singular vertex is an interior
   patch point referenced by no edge (it closes the fan exactly as in Stage-1
   tessellation), so the cycle = the rim and half-edge pairing on the cap holds.

2. **`emit_topology` defensive arm** (`src/lib.rs:3826`): drop `Surface::Cone`
   from the loud-reject `match` set there (now handled by the curved branch
   above). After this PR all three curved surfaces are handled by the curved
   branch and this arm is unreachable-defensive for Cylinder/Sphere; keep it LOUD
   for any genuinely unexpected surface. Update the comment.

3. **`surface_to_quadric`** (`src/lib.rs:2087`, the `Surface::Cone { .. }` reject
   arm): map field-for-field →
   `Surface::Cone { apex, axis_dir, half_angle } => Ok(ssi_rs::QuadricSurface::Cone { apex, axis_dir, half_angle })`
   (identical to the Cylinder/Sphere arms). → enables `ssi_rs::intersect(Plane,
   Cone)` → the exact rim `Circle`. Confirm `ssi_curve_to_curve` already maps
   `SsiCurve::Circle → Curve::Circle` (it does). Update the doc comment
   (`src/lib.rs:2059-2067`) so `Cone` is documented as mapping field-for-field.

4. **`tol_for` face-resolution band** (`src/lib.rs:2788`, and the stale comment at
   `src/lib.rs:2786-2787`): give the Cone a real per-face resolution tolerance =
   its OWN Stage-1 chord bound `cone_chord_bound(height, half_angle)` — the SAME
   bound Stage 1 guarantees (A15 / A14.3, **NOT** tolerance widening). Correct the
   stale comment (`plane_dist` no longer rejects the cone; `tol_for` is the live
   site).

   **⚠ Confirm-or-STOP (P9/P10) — cone height derivation.** `cone_chord_bound`
   needs a *height*, but `Surface::Cone` carries only `apex`/`axis_dir`/`half_angle`
   — height is not in the surface. A **principled, non-widening** derivation
   ALREADY EXISTS and is used by the Stage-1 pre-pass (`src/lib.rs:503-525`):
   the cone face's outer loop contains its rim `Curve::Circle` edge; with
   `â = unit(axis_dir)` and `rim_center` from that Circle,
   `height = |(rim_center − apex)·â|`. The `tol_for` closure captures the input
   B-Rep, so it can locate cone face `fi`'s outer-loop Circle edge and apply the
   SAME derivation, then `Ok(cone_chord_bound(height, half_angle))`. This is the
   single-source bound, NOT a widened tolerance. **If — and only if — no rim
   Circle is found on the cone face's outer loop (so no sound height can be
   derived), STOP and report** (a genuine producer fault → loud, never a defaulted
   or widened tolerance). The Cylinder arm's `FaceResolutionFailed`/loud-on-missing
   convention is the model.

No change to: `BRep::new` (cone already tessellates), Stage 4, the planar branch,
the watertight gate (χ = 2 already accepted), Union / planar / YR13 / YR14 / YR15
paths (all keep `reversed = false` / prior behaviour byte-for-byte). **No ssi-rs
change.**

## Branch table (`emit_topology`, `src/lib.rs`) — extended from YR15

| Branch | Surface | Sense encoding | `reversed` |
|---|---|---|---|
| Planar (`src/lib.rs:3820+`) | `Plane` | possibly-flipped `Plane.normal` (winding-derived) | `false` (always) |
| Curved (`src/lib.rs:3751+`) | `Cylinder`, `Sphere`, **or `Cone`** | surface inherited UNCHANGED; flag records flip | `op == Subtract && info.input == InputId::B` |

The conical pocket wall is a single curved patch with **one** boundary cycle (the
rim); the curved branch's outer-loop selection handles a single cycle trivially
(it becomes the outer loop; no inner loops). The apex is an interior singular
vertex of that patch (no edge references it).

## Invariants

Reuse PR-YR13's **I-rev1..I-rev4** verbatim (consistency, no double-flip, exact
params, byte-identity of Union/planar) and PR-YR14's **I-genus** (per-shell Euler
gate accepts χ = 2 − 2g; here χ = 2). Add:

- **I-cone-band:** a `Surface::Cone` face's membership tolerance is its OWN
  Stage-1 chord bound `cone_chord_bound(height, half_angle) = 1e-2·√((2R)²+h²)` —
  the SAME bound Stage 1 guarantees, with `height` derived from the cone face's
  rim Circle (`|(rim_center − apex)·â|`). This is A14.3/A15, **not** tolerance
  widening. It is **not** the Circle-rim AABB bound (`curved_chord_bound`, 2R√2),
  which ignores the height.
- **I-cone-rim:** the rim is the exact `Circle` of `cone ∩ box-top plane` (a
  perpendicular cut); every rim point satisfies the cone radial residual
  `|radial − |h_axial|·tanα| = 0` AND lies on the box-top plane (z = 2) to
  `TAU_MODEL`; radius `= R_rim`; Stage-4 relocates the rim crossings onto it.
- **I-cone-winding (load-bearing for the first time):** the cone-lateral cavity
  wall's emitted mesh winding agrees with `reversed == true` — the effective
  outward normal (the YR16 tilted normal `n̂ = unit(r̂ − tanα·â)`, NEGATED because
  `reversed`) points INTO the pocket (away from box material), not into box
  material. The conical pocket is the first non-trivial winding case for the
  tilted cone normal (per `yang_cone_tessellation_oracle_findings`).

## Oracles (RED — `crates/yang-rs/tests/yr17_subtract_cone.rs`)

Mirror `tests/yr15_subtract_sphere.rs` structure exactly (helpers: `sub3/add/
scale/dot/cross/norm/unit`, `unpaired_half_edges`, `euler_characteristic`,
`signed_volume`, `box_brep`, `cone_brep` (mirror `tests/yr16_cone.rs`),
`LabelMock`, a `run_subtract()` direct-path runner, a `cavity_wall_faces` filter
selecting `Surface::Cone && reversed`).

**Fixture (sidecar-independent GREEN gate):** apex `(0,0,0.5)` inside the box,
axis `+Z`, `half_angle = atan(1.0/1.5)` so the rim at z = 2 has radius 1.0;
hand-build a `LabeledArrangement` for the FULL closed genus-0 result surface:

- box faces → `InputId(0)`, `inside = [false,false]`; box top emitted as the
  **annular ring** around the rim (outer square + inner rim hole);
- the cone-lateral cavity wall → `InputId(1)`, `inside = [true,false]`; authored
  **pre-swapped** so `flip_for_op(Subtract)` yields into-pocket (toward-axis,
  outward-from-result) winding — same convention as YR13/YR15;
- ONE rim `Circle` at the box-top plane (z = 2, r = 1, normal `+Z`);
- the lateral is an apex-fan (rim ring → single apex vertex), so the cavity wall
  is N rim verts + 1 apex vertex; the apex closes the fan (a triangle fan to the
  apex), mirroring the YR15 pole fan but with the apex as the singular tip.

**Mandatory `mock_is_valid_genus0` self-check** (mirror `yr15:563`): build the
SIMULATED Subtract output (keep-all + flip label-1) directly, NO `boolean()` call,
and assert it is watertight (0 unpaired half-edges), χ = 2, and **positive signed
volume** — so a fixture bug cannot masquerade as a code pass (memory:
`yang_mock_orientation_witness` — a hand-built arrangement can pass watertight + χ
while globally inside-out; the positive-volume + winding-sense witness is the
guard). This test PASSES today (no `boolean()` call); the boolean oracles below
FAIL today (cone rejected) — that is the RED signal.

**Oracles (5):**

1. **Cavity wall is `Surface::Cone`** with the input cone's exact `apex` /
   `axis_dir` / `half_angle`, `reversed == true`; box faces `Plane`,
   `reversed == false`; no Sphere/Cylinder faces in the output. (PART A
   surface-param + the structural checks.)
2. **Effective outward normal points INTO the pocket** — two parts:
   - PART A (surface-param): the YR16 tilted cone normal `n̂ = unit(r̂ − tanα·â)`
     NEGATED (because `reversed`) points into the removed (pocket) region, not
     into box material; assert via surface-param reasoning on sampled wall points.
   - PART B (mesh-winding **witness**): classify cavity-wall triangles
     geometrically (all 3 verts within the cone chord band of the lateral, in the
     pocket band 0.5 ≤ z ≤ 2), compute the geometric winding normal at the
     centroid, assert it points into the pocket (toward the axis / away from box
     material). Require ≥ N witnessed triangles (non-vacuous). **Sample EDGE
     MIDPOINTS in the membership classification, not just verts + centroid** (per
     `yang_cone_tessellation_oracle_findings` — the cone chord bulges most at edge
     midpoints).
3. **Watertight 2-manifold, χ = 2** (0 unpaired half-edges; positive signed
   volume; the apex singular vertex closes cleanly).
4. **Exact `Circle` rim** — ≥1 `Curve::Circle` edge; every sampled rim point lies
   on the cone (radial residual `|radial − |h_axial|·tanα| ≤ TAU_MODEL`) AND on
   the box-top plane (z = 2) to `TAU_MODEL`; radius `= R_rim` (±TAU_MODEL);
   Stage-4 relocated rim crossings onto it.
5. **Determinism** (two `run_subtract()` runs byte-identical in verts, tris, and
   per-face surface + `reversed`) + **env-gated sidecar `Subtract` mesh-parity**
   (`SidecarBoolean::from_env()`, LOUD `eprintln!("[yang-rs yr17] SKIP: …
   CHERCHI2022_BIN")` + `return` if unset) asserting the sidecar-backed output is
   watertight + χ = 2 + carries a reversed `Surface::Cone` cavity wall. The
   **direct-path (`LabelMock`) oracles 1–4 are the GREEN gate** (sidecar-independent).

## Faithful migration

If any existing fixture/test needs a `reversed` field addition or a guard
migration (e.g. an assertion that a cone cavity wall no longer rejects), change
**ONLY the expected outcome**, preserving every structural assertion. **None is
anticipated** — every cone boolean path was previously `Err`, and the YR15/YR16
adversaries assert "boolean OUTPUT has no Cone face" only for box−sphere /
no-boolean fixtures (which add no cone boolean, so they stay TRUE). The GREEN
implementer must verify NO Union/planar/YR13/YR14/YR15 regression (those keep
`reversed = false` / prior behaviour byte-for-byte). The Adversary independently
verifies no migration was weakened and no regression.

## Failure modes

- **F-cone1 — cavity patch won't reassemble:** the curved branch fails to extract
  the rim boundary cycle, or half-edge pairing fails on the apex-closed cap.
  Oracle 1/3 catches it → **STOP** (see below), not an improvised fix.
- **F-cone2 — rim won't relocate onto the exact circle:** Stage-4 leaves the rim
  crossings off the cone radial residual or off the box-top plane. Oracle 4
  catches it → STOP.
- **F-cone3 — cone not watertight / apex doesn't close:** the apex singular vertex
  duplicates or leaves the fan open. Oracle 3 catches it → STOP.
- **F-cone4 — no sound height for `tol_for`:** the cone face's outer loop has no
  rim Circle, so `cone_chord_bound` has no principled height. → **STOP and report**
  (never default or widen the tolerance).
- Reuse PR-YR13's F-rev1..F-rev3 (wrong sense, double-flip, param perturbation),
  caught by oracles 1/2.

## STOP conditions (P9/P10)

Halt the cycle and report what was learned — do **NOT** fake — if:
- the cone-lateral cavity patch won't reassemble (boundary cycle ≠ the rim, or the
  apex won't close);
- the rim won't relocate onto the exact circle;
- the conical pocket isn't watertight χ = 2;
- no sound height can be derived for the `tol_for` cone bound;
- the plan's diagnosis is wrong (an unexpected fifth rejection site, or a
  non-unique / non-circle SSI result for the perpendicular `plane ∩ cone` cut).

No tolerance widening, no fallback path, no synthetic fill.

## Scope

`box − cone` single conical pocket (apex inside, base above top, perpendicular
top-plane exit) ONLY. **Deferred (explicitly out of scope, stay LOUD):**
through-cone (base also subtracted / two rims), **oblique cuts**
(ellipse / parabola / hyperbola rims — the `plane_cone` non-C1 branches),
fully-internal cone void (multi-shell), cone-base-subtracted, side-face / corner
(triple-point) exit. Union + planar + YR13 + YR14 + YR15 byte-for-byte.

## Research basis

- **Yang et al. 2025 §4.1 (bijective tessellation) / §4.4.2 / §4.5** — Stage-6
  B-Rep reassembly: a kept face inherits its analytical surface; a subtracted
  subtrahend's bounding faces are cavity walls whose outward orientation reverses.
  A conical pocket is the same reassembly with the cavity wall a single-cycle
  `Surface::Cone` patch (apex an interior singular vertex) and the result a
  genus-0 shell. (`refs/text/yang2025_hybrid_boolean.txt`.)
- **Cone ∩ plane perpendicular to the axis** = a circle of radius `|h|·tanα`
  (Patrikalakis Ch.5 / `ssi-rs` `plane_cone` C1 branch).
- **Euler–Poincaré** — a box with a conical pocket is still a topological sphere:
  χ = 2.

## On completion

- `docs/yang_functional_roadmap.md`: append a **PR-YR17 — box − cone conical
  pocket (genus 0); `surface_to_quadric`/`tol_for`/`emit_topology` Cone wiring**
  entry in the YR16 style, noting curved Subtract now covers **cylinder + sphere +
  cone**, and the remaining curved-Subtract gaps (through-cone, oblique cuts
  ellipse/parabola/hyperbola, fully-internal voids, side-face/corner guard,
  box-as-subtrahend).
- Refresh the deferral prose in `src/lib.rs` if it lists cone cavities as deferred
  (drop the conical-pocket case; oblique/through/internal cone cuts remain
  deferred).
- Update `docs/yang_deviations.md` if any deviation is discovered.
