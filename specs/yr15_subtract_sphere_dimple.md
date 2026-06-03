# PR-YR15 — curved `Subtract`: box − sphere HEMISPHERICAL DIMPLE (genus 0, χ=2)

**Milestone:** M5 / Phase 2 step (curved `Subtract` topology — the third M5
increment after PR-YR13's blind pocket and PR-YR14's through-hole).
**Predecessors:** PR-YR13 (`box − cylinder` BLIND POCKET, genus 0, cavity-sense
via `BRepFace.reversed`); PR-YR14 (`box − cylinder` THROUGH-HOLE, genus 1,
per-shell Euler gate generalized to χ = 2 − 2g).
**Crate:** `crates/yang-rs/` only.

## Goal (narrow, single cycle)

Extend the curved `Subtract` cavity path to a **spherical** cavity: a sphere
centred ON one box face (poking through exactly that one face) so that
`box − sphere` carves a **hemispherical dimple**. The result is:

- a single connected, closed, orientable 2-manifold shell of **genus 0 → χ = 2**
  (a dimpled box, topologically still a sphere);
- exactly **ONE** exact `Circle` rim — `sphere ∩ box-face plane`, a **great
  circle** because the sphere centre lies on that plane;
- a cavity wall that is the **inside hemisphere** of the sphere
  (`Surface::Sphere`, `reversed = true`), whose effective outward normal points
  **toward the sphere centre** (into the dimple);
- the box face that the sphere pokes through becomes an **annular planar face**
  (the box-face plane minus the rim disk) — handled by PR-YR5c's outer+inner-loop
  machinery unchanged.

## What already works (reuse unchanged — confirm, don't rebuild)

- **Stage 1 sphere tessellation**: `tessellate_sphere_face` (`src/lib.rs:1196`),
  dispatched in `BRep::new` (`src/lib.rs:633`); `eval_source` Sphere arm
  (`src/lib.rs:831`); chord bound `d_ε = 1e-2·2r√3` (`src/lib.rs:1257`). Shipped
  PR-YR12.
- **Sphere point-to-surface distance**: `signed_distance_to_surface` Sphere arm
  (`src/lib.rs:1508`, `|x − center| − radius`) returns `Ok` → so the `plane_dist`
  closure (`src/lib.rs:2457`) already succeeds for a Sphere face. The stale
  comment at `src/lib.rs:2460` claiming Sphere "still rejects" is pre-YR12; it
  may be corrected in passing.
- **Cavity-sense flag**: `reversed: op == BoolOp::Subtract && info.input == InputId::B`
  (`src/lib.rs:3493`) — already surface-agnostic; no change.
- **ssi-rs**: `QuadricSurface::Sphere { center, radius }` + the `plane_sphere`
  solver returning `SsiCurve::Circle` (`crates/ssi-rs/src/lib.rs:358, 489`).
- **Stage 3/4 conic path**: `ssi_curve_to_curve` Circle arm (`src/lib.rs:1834`),
  `curve_contains_point` Circle (`src/lib.rs:1867`), Stage-4 `project_onto_circle`
  (`src/lib.rs:1522`) — all `Circle`-generic; the rim relocation reuses them with
  **no new Stage-4 code** (the rim IS a `Circle`; sphere interior verts are
  already exact from Stage 1).
- **Annular planar face** (box face with a circular hole): the PR-YR5c
  multi-cycle / outer+inner-loop machinery in the PLANAR branch — reused
  unchanged.
- **Per-shell Euler gate**: `check_watertight_2manifold` already accepts χ = 2 − 2g
  (PR-YR14); a genus-0 dimple is χ = 2, the easy case. No change.

### Spec-vs-reality note (carry into the cycle)

The PR brief said "plane ∩ sphere → Circle … already wired into Stage 3
(PR-YR9)." That is **partly inaccurate**: the solver lives in `ssi-rs`, but
yang's `surface_to_quadric` (`src/lib.rs:1804`) currently returns
`UnsupportedSurfaceForSsi` for `Sphere`. Wiring it is part of this PR. This does
**not** invalidate the approach — the task is exactly "confirm each stage
composes," and the three wiring sites below are that confirmation. `Surface::Sphere`
is loudly rejected at three production sites that must be faithfully extended
(each mirroring the existing `Cylinder` arm). This is honest wiring of an
already-type-supported surface, **not** new mechanism and **not** a hack.

## Production edits (GREEN — three faithful extensions + one single-source helper)

All in `crates/yang-rs/src/lib.rs`. Each mirrors the `Cylinder` precedent; `Cone`
stays a loud reject everywhere.

1. **`surface_to_quadric`** (`src/lib.rs:1822`, the `Sphere | Cone` reject arm):
   split out `Sphere` →
   `Surface::Sphere { center, radius } => Ok(ssi_rs::QuadricSurface::Sphere { center, radius })`
   (field-for-field, like the Cylinder arm). Keep `Cone` → `UnsupportedSurfaceForSsi`.
   → enables `ssi_rs::intersect(Plane, Sphere)` → exact rim `Circle`. Update the
   doc comment (`src/lib.rs:1800-1803`) so `Sphere` is documented as mapping
   field-for-field; only `Cone` rejects.

2. **`tol_for` face-resolution band** (`compute_phase_a`, `src/lib.rs:2489-2500`):
   add `Surface::Sphere { radius, .. } => Ok(sphere_chord_bound(radius))`, the
   sphere's own Stage-1 chord bound (the SAME bound Stage 1 guarantees — A15 /
   A14.3, **NOT** tolerance widening). Keep `Cone` loud.
   **Single-source helper (A14.3 — one source of truth):** extract the
   `1e-2·2r√3` literal from `tessellate_sphere_face` (`src/lib.rs:1257`) into a
   free `fn sphere_chord_bound(radius: f64) -> f64 { 1e-2 * 2.0 * radius * 3f64.sqrt() }`
   and consume it in BOTH sites (Stage 1 `d_eps` and the new `tol_for` arm).
   NOTE: the existing `curved_chord_bound(edges)` (Circle-rim AABB × 1e-2) is
   **wrong for a sphere** — the seam circle's AABB diagonal is 2r√2, not the
   sphere's 2r√3 — so do **NOT** reuse `band` for sphere faces.

3. **`emit_topology` curved branch guard** (`src/lib.rs:3429`): broaden
   `if let Surface::Cylinder { .. } = inherited` to
   `if matches!(inherited, Surface::Cylinder { .. } | Surface::Sphere { .. })`.
   The branch body (push_loop, E2 degenerate-loop guard, outer/inner loop
   assignment, `reversed` flag, `surface: inherited`) is already surface-agnostic
   — **no body change**. Update the planar-fallback arm/comment at
   `src/lib.rs:3500-3505`: `Sphere` is now handled by the curved branch above, so
   its arm there becomes unreachable-defensive (still LOUD); `Cone` stays the live
   loud reject.

No change to: `BRep::new` (sphere already tessellates), Stage 4, the planar
branch, the watertight gate (χ = 2 already accepted), Union/planar/YR13/YR14
paths (all keep `reversed = false` / Cylinder behaviour byte-for-byte).

## Branch table (`emit_topology`, `src/lib.rs`) — extended from YR14

| Branch | Surface | Sense encoding | `reversed` |
|---|---|---|---|
| Planar (`src/lib.rs:3498+`) | `Plane` | possibly-flipped `Plane.normal` (winding-derived) | `false` (always) |
| Curved (`src/lib.rs:3429+`) | `Cylinder` **or `Sphere`** | surface inherited UNCHANGED; flag records flip | `op == Subtract && info.input == InputId::B` |

The hemispherical cap wall is a single curved patch with **one** boundary cycle
(the great-circle rim). The curved branch's outer-loop selection handles a
single cycle trivially (it becomes the outer loop; no inner loops).

## Invariants

Reuse PR-YR13's **I-rev1..I-rev4** verbatim (consistency, no double-flip, exact
params, byte-identity of Union/planar) and PR-YR14's **I-genus** (the per-shell
Euler gate accepts χ = 2 − 2g; here χ = 2). Add:

- **I-sphere-band:** a `Surface::Sphere` face's membership tolerance is its OWN
  Stage-1 chord bound `sphere_chord_bound(radius) = 1e-2·2r√3` — the SAME bound
  Stage 1 guarantees, sourced from a SINGLE shared helper. It is **not** the
  Circle-rim AABB bound (`curved_chord_bound`, which would underestimate at
  2r√2). This is A14.3/A15, not tolerance widening.
- **I-sphere-rim:** the rim is the exact great `Circle` of `sphere ∩ box-face
  plane`; every rim point satisfies `|x − center| = radius` AND lies on the
  box-face plane to `TAU_MODEL`; Stage-4 relocates the rim crossings onto it.

## Oracles (RED — `crates/yang-rs/tests/yr15_subtract_sphere.rs`)

Mirror `tests/yr13_subtract_cylinder.rs` structure exactly (helpers: `sub3/add/
scale/dot/cross/norm/unit`, `unpaired_half_edges`, `euler_characteristic`,
`signed_volume`, `box_brep`, `sphere_brep` (from YR12), `LabelMock`, a
`run_subtract()` direct-path runner, a `cavity_wall_faces` filter selecting
`Surface::Sphere && reversed`).

**Fixture (sidecar-independent GREEN gate):** sphere centred on the box top face
so exactly the lower hemisphere is inside the box; hand-build a
`LabeledArrangement`:
- box faces → `InputId(0)`, `inside=[false,false]`; box top emitted as the
  **annular ring** around the rim (outer + inner loop).
- the inside-hemisphere cap → `InputId(1)`, `inside=[true,false]`; authored
  **pre-swapped** so `flip_for_op(Subtract)` yields toward-centre (into-dimple)
  winding (same convention as `pocket_arrangement` in YR13).
- ONE great-circle rim at the box-face plane.
- Include the YR14-style **mock self-check** test (`simulated_output_mesh` is
  watertight, χ = 2, positive volume) so a bad fixture fails loudly BEFORE the
  boolean oracles.

**Oracles (RED contract from the brief):**

1. **Cavity wall surface params.** Cavity wall is `Surface::Sphere` with the
   input's exact `center`/`radius`, `reversed == true`; box faces `Plane`,
   `reversed == false`. (PART A surface-param + PART B emitted-mesh-winding
   witness, like YR13 oracle 1/3.)
2. **Effective outward normal points TOWARD the centre** (into the dimple):
   sampled on the cap wall, the analytic away-from-centre normal negated (because
   `reversed`) points toward `center`, not away. Assert via both surface-param
   reasoning and the emitted mesh-triangle winding normal
   (`dot(gnorm, away_from_center) < -1e-9`).
3. **Watertight 2-manifold, χ = 2, signed_volume > 0, 0 unpaired half-edges.**
4. **Exact `Circle` rim:** ≥1 `Curve::Circle` edge; every rim point satisfies
   `|x − center| = radius` AND lies on the box-face plane to `TAU_MODEL`; Stage-4
   relocated the rim crossings onto it (chord deviation drops to `TAU_MODEL`).
5. **Determinism + sidecar parity:** two runs byte-identical (verts, tris, face
   surfaces + `reversed`) + env-gated sidecar `Subtract` mesh-parity
   (`SidecarBoolean::from_env()`, LOUD skip
   `eprintln!("[yang-rs yr15] SKIP: ...CHERCHI2022_BIN")` + `return` if absent).

**Faithful migration:** if any existing fixture/test needs a `reversed: false`
field addition or a guard-migration (e.g. a `CurvedSurfaceNotYetSupported { face }`
assertion that no longer fires for sphere), change ONLY the expected outcome,
preserving every structural assertion. The Adversary independently verifies no
migration was weakened.

## Failure modes

- **F-sphere1 — cap patch won't reassemble:** the curved branch fails to extract
  the rim boundary cycle, or half-edge pairing fails on the cap. Oracle 1/3
  catches it → this is a **STOP** (see below), not an improvised fix.
- **F-sphere2 — rim won't relocate onto the exact circle:** Stage-4 leaves the
  rim crossings off `|x − center| = radius` or off the box-face plane. Oracle 4
  catches it → STOP.
- **F-sphere3 — wrong band:** reusing `curved_chord_bound` (2r√2) instead of the
  sphere's 2r√3 chord bound. I-sphere-band / oracle 4 region catches it.
- Reuse PR-YR13's F-rev1..F-rev3 (wrong sense, double-flip, param perturbation),
  caught by oracles 1/2.

## STOP conditions (P9/P10)

Halt the cycle and report what was learned — do **NOT** fake — if:
- the sphere-cap cavity patch won't reassemble (boundary cycle ≠ the rim);
- the rim won't relocate onto the exact circle;
- the dimple isn't watertight χ = 2;
- the plan's diagnosis is wrong (e.g. an unexpected fourth rejection site, or a
  non-unique SSI selection for `plane ∩ sphere`).

No tolerance widening, no fallback path.

## Scope

`box − sphere` single-face dimple ONLY. Deferred (still loud): fully-internal
spherical void (multi-shell), through-sphere, cone cavities (`Cone` rejects
everywhere), box-as-subtrahend, side-face/corner guard. Union + planar + YR13 +
YR14 byte-for-byte.

## Research basis

- **Yang et al. 2025 §4.4.2 / §4.5** — Stage-6 B-Rep reassembly: a kept face
  inherits its analytical surface; a subtracted subtrahend's bounding faces are
  cavity walls whose outward orientation reverses. A hemispherical dimple is the
  same reassembly with the cap wall a single-cycle `Surface::Sphere` patch and
  the result a genus-0 shell. (`refs/text/yang2025_hybrid_boolean.txt`.)
- **Sphere ∩ plane through the centre** = a great circle of the same radius
  (Patrikalakis Ch.5 / `ssi-rs` `plane_sphere`).
- **Euler–Poincaré** — a dimpled box is still a topological sphere: χ = 2.

## On completion

- `docs/yang_functional_roadmap.md`: add **PR-YR15 — box − sphere hemispherical
  dimple (genus 0); `surface_to_quadric`/`tol_for`/`emit_topology` Sphere wiring;
  `sphere_chord_bound` single-source helper ✅ DONE** in the YR14 style.
  Remaining curved-Subtract: cone cavities, internal spherical voids,
  through-sphere, the side-face/corner (triple-point) guard, box-as-subtrahend.
- Refresh the deferral prose in `src/lib.rs:106-112` (drop sphere cavities from
  the still-deferred list; cone cavities remain deferred).
