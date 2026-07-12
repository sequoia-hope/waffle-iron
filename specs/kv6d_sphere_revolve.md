# KV6d increment 2: on-axis full-turn circle revolve → SPHERE (C0067)

## Goal

A full-turn (360°) revolve of a circle profile whose center lies ON the
revolve axis produces a CLOSED sphere solid (genus 0) that renders,
validates, and enters the yang boolean pipeline as an operand (C0067:
sphere + polar notch cut). This retires the
`RevolveOnAxisCircleUnsupported` typed wall (the variant is REMOVED —
the configuration is now supported).

## Parameters

Same as `revolve()` (no new user-facing parameters):

- `profile`: circle region (`ProfileRegion::Circle { center, radius }`),
  radius `r > 0`.
- `axis_origin`, `axis_direction`: in-plane revolve axis (existing
  `REVOLVE_AXIS_IN_PLANE_TOLERANCE` checks unchanged).
- `angle_rad`: full-turn branch only
  (`|angle − 2π| ≤ REVOLVE_FULL_TURN_TOLERANCE`). Partial-angle on-axis
  circles keep `RevolveAxisIntersectsProfile` (a partial lathe of an
  axis-crossing profile is invalid input, unchanged).

Derived: sphere center = the profile-circle center's projection onto the
axis (`axis_origin + t_c·â`; the center is within
`REVOLVE_MIN_AXIS_CLEARANCE_REL`-scaled clearance of the axis by the
branch gate, so the snap is sub-tolerance); sphere radius = `r`.

## Branch table (delta from `kv6d_closed_torus_revolve.md`)

| Profile | Angle | Center offset | Behavior |
|---|---|---|---|
| Circle | full turn | R ≤ clearance (center on axis) | **NEW: closed sphere** (was typed wall) |
| Circle | partial | R ≤ clearance | `RevolveAxisIntersectsProfile` (UNCHANGED) |
| all other rows | | | UNCHANGED (byte-identical) |

## Topology (the closed-sphere B-Rep)

Minimal seam structure of S², the PR-YR12 yang contract
(`specs/yang_pr_yr12_sphere_tessellation.md` §1) mirrored into the arena:

- **V = 2**: south pole `center − r·ẑ`, north pole `center + r·ẑ`
  (WORLD z — the sphere is isotropic, so the seam frame is CANONICAL
  z-up regardless of the revolve axis; this makes `to_yang` a direct
  emission of the PR-YR12 fixture and is a documented convention, not a
  restriction).
- **E = 1**: one meridian seam `Curve::Arc` twin pair on the X–Z
  great circle through `center + r·x̂`:
  - `seam_fwd`: south → north, `Arc { center, normal: −ŷ, radius: r }`
    (CCW around −ŷ sweeps south → +x̂ → north);
  - `seam_back` (twin): north → south, `Arc { center, normal: +ŷ, ... }`
    (same point set, opposite traversal — the existing curve-twin
    sign-canonicalized consistency rule).
- **F = 1**: one `Surface::Sphere { center, radius, reversed: false }`
  face; outer loop = `[seam_fwd, seam_back]` (the twin pair internal to
  one loop — closed-torus precedent).
- **Shell genus = 0**; Euler–Poincaré `V − E + F − R = 2 − 1 + 1 − 0 =
  2 = 2(S − G)` ✓.
- `RevolveResult`: `start_cap = end_cap = None`, `walls = [sphere face]`.

## New kernel-v2 vocabulary

1. `Surface::Sphere { center: Point3, radius: f64, reversed: bool }` —
   `reversed` is the cavity sense every kernel-v2 curved surface carries
   (a spherical dimple wall from a Subtract, cf. PR-YR15).
2. `geom::sphere_residual(p, center, radius) = |p − center| − radius`
   (plain length units).
3. `validate_sphere_face`: params finite, `radius > 0`; debug tier:
   every loop vertex within `CURVED_SURFACE_DEBUG_TOLERANCE ·
   radius.max(1.0)` of the surface. Topology-agnostic (accepts both the
   closed seam loop and boolean-output trimmed patches) — the torus
   validator precedent.
4. Polygonal-walk exemption: `(Some(Surface::Sphere), no-curved-edges)`
   cannot occur for a real patch (its boundary always carries arcs), but
   the sphere joins the cylinder/cone/torus arm (no Newell walk).

## Render tessellation

- **Closed sphere** (outer loop = the 2-half-edge seam Arc twin pair):
  z-up lat/long grid, poles emitted ONCE (single vertex each, fan
  closure), longitude wrap via modular column indexing (no duplicated
  seam column). `n_lon = n_seg`, `n_lat = max(2, n_seg/2)`. Normals
  analytic `(p − center)/r`, negated when `reversed`. Triangles wound to
  agree with the analytic outward normal (torus `emit` precedent).
- **Boolean-output sphere PATCH**: new `yang_rs::tessellate_sphere_patch`
  (public, mirrors `tessellate_torus_patch`): project boundary/hole
  polylines into scaled `(u·r, v·r)` lon/lat coordinates (z-up frame,
  `v = asin((z−c_z)/r)` clamped, `u` unwrapped per loop);
  - all loops non-wrapping → disk + period-shifted holes → refined CDT;
  - exactly ONE loop with net longitude wrap `wu = ±1`, no holes → the
    patch contains exactly one pole (`wu = +1` → north for an outward
    face, flipped when `reversed`): bridge the boundary to the pole with
    a two-sided meridian seam (both copies share BIT-IDENTICAL 3D
    sample points) + a degenerate-at-the-pole bottom edge (two UV
    corners, one 3D point); post-CDT, weld bit-identical positions and
    drop 3D-degenerate triangles → a proper watertight pole fan;
  - anything else (two wrapping loops / wrapped loop with holes /
    boundary within band of a pole) → `None` → kernel-side typed
    `TessellationFailed` (loud wall, later slice).
  Steiner refinement budget: `max_area = seg²`,
  `seg = 2π·r / n_seg` (equator chord spacing, the torus recipe).

## Boolean pipeline (to_yang / from_yang)

- `to_yang`: NEW dedicated arm for `Surface::Sphere` faces gated on the
  PRISTINE closed sphere (no inner loops; outer loop exactly the seam
  Arc twin pair; arc radius == sphere radius; endpoints at
  `center ± r·ẑ` within the export band). Emits the PR-YR12 fixture:
  2 pole verts (via `vid_map`), 1 seam `Curve::Circle { center,
  normal: (0,−1,0), radius }` edge `start = south, end = north`, face
  `outer_loop = [seam]`. A boolean-OUTPUT sphere patch re-entering a
  boolean is a typed `UnsupportedCurvedBoolean` wall (later slice —
  same shipping order as the torus: closed operand first, Slice-F-style
  band re-entry later).
- `from_yang`: `FaceSurf::Sphere { center, radius, reversed }` +
  `yang_rs::Surface::Sphere` output arm (radius finite > 0), the
  `FaceSurf → Surface` arena arm, and sphere joins the
  Cylinder|Cone|Torus list in the full-circle-edge sense derivation
  (`n_for`) — a cap-cut sphere shares its rim circle with a planar cap.
- Stage 1–5 are ALREADY sphere-capable (PR-YR12 tessellation, PR-YR15
  plane×sphere exact circles + `signed_distance_to_surface`,
  stage4 relocation arms) — this increment adds NO yang pipeline logic,
  only the patch render helper.

## Invariants

1. Every `validate_solid` arena invariant (twin pairing, curve-twin
   consistency, vertex fan, Euler–Poincaré genus 0).
2. Both seam-arc endpoints and all seam samples lie on the sphere
   (`sphere_residual = 0` up to the curved-surface debug band).
3. Volume: solid volume = `4/3·π·r³` (render-mesh volume within the
   chord sag band; improves with `n_seg`).
4. Render mesh: watertight, 2-manifold, outward normals, positive
   volume.
5. Determinism: identical revolves → byte-identical arenas/meshes.
6. Boolean re-entry: `to_yang` output is accepted by `yang_rs::BRep::new`
   and Stage 1 produces the watertight PR-YR12 lat/long grid; the C0067
   chain (sphere − polar notch box) completes with χ = 2 and the meta
   volume.

## Oracles

- kernel-v2 `tests/kv6d_sphere_revolve.rs`: topology census
  (V/E/F/loop shape/genus), seam residuals, volume via render mesh
  (r = 0.4 → `4/3·π·0.064 ≈ 0.26808`), watertight + manifold render
  mesh, determinism, rejection branches (partial-angle on-axis →
  `RevolveAxisIntersectsProfile`; off-center crossing unchanged), and a
  kernel-boolean e2e (sphere − polar box → validate + render + χ = 2 +
  volume) exercising to_yang/from_yang/patch-render together.
- yang-rs unit: `tessellate_sphere_patch` pole-cap case is watertight
  (every undirected edge used exactly twice after welding) and disk
  case matches boundary bit-for-bit.
- e2e: assay `C0067` flips `UNSUPPORTED(revolve)` → `SUPPORTED_CORRECT`.
  Full assay zero-lost.
- Existing tests updated: `kv6a_revolve.rs:599` and
  `kv6d_closed_torus.rs:276` asserted the removed typed wall — they now
  assert the sphere BUILDS (and the closed-torus spec's branch table row
  is superseded by this spec).

## Failure modes

- Partial-angle on-axis circle → `RevolveAxisIntersectsProfile`
  (unchanged, invalid input).
- Sphere-patch re-entry into a second boolean → typed
  `UnsupportedCurvedBoolean` (later slice).
- Patch render outside the disk/single-pole-cap scope → typed
  `TessellationFailed` (loud, later slice).

## Research basis

- [#24] Yang et al. 2025 — hybrid pipeline consuming the operand
  (Stage 1 structured tessellation; PR-YR12 lat/long arm).
- Stroud 2006 §3.1.4 — seam representation of closed curved faces
  (cylinder/torus precedent; the sphere's seam is the standard
  2-vertex/1-edge/1-face CW structure of S²).
- Mäntylä 1988 — Euler–Poincaré bookkeeping (`2(S − G)`).

## Analytical vs. approximate method justification

- The operand construction is exact/analytic (`Surface::Sphere` +
  `Curve::Arc` seam). No SSI is computed by this increment.
- The C0067 downstream boolean uses the EXISTING exact plane×sphere
  circle producer (PR-YR15/ssi-rs). The patch render CDT is a
  tessellation primitive (render only), not a boolean ingredient — the
  same precision posture as the shipped torus patch.
