# KV6d increment: closed-torus full-turn circle revolve (C0065)

## Goal

A full-turn (360°) revolve of a circle profile about a strictly-off-axis
in-plane axis produces a CLOSED ring torus solid (genus 1), which renders,
validates, and re-enters the yang boolean pipeline as an operand (C0065:
torus + through-notch subtract severing the ring). This retires the
`RevolveCircleProfileUnsupported` full-turn wall for the ring-torus
configuration. The ON-AXIS full-turn circle (sphere, C0067) remains a typed
wall with a sphere-specific message (KV6d increment 2).

## Parameters

Same as `revolve()` (no new user-facing parameters):

- `profile`: circle region (`ProfileRegion::Circle { center, radius }`),
  radius `r > 0` (validated by `Profile`).
- `axis_origin`, `axis_direction`: in-plane revolve axis (existing
  `REVOLVE_AXIS_IN_PLANE_TOLERANCE` checks unchanged).
- `angle_rad`: this spec covers the full-turn branch
  `|angle − 2π| ≤ REVOLVE_FULL_TURN_TOLERANCE` only; partial angles keep the
  existing `build_torus_revolve` bent-tube path byte-identically.

Derived: major radius `R` = radial distance of the embedded circle center
from the axis; minor radius `r`. Ring-torus requirement `R − r > clearance`
(existing `REVOLVE_MIN_AXIS_CLEARANCE_REL` scaling).

## Branch table

| Profile | Angle | Center offset | Behavior |
|---|---|---|---|
| Circle | partial (0, 2π) | R − r > clearance | existing bent tube (UNCHANGED, byte-identical) |
| Circle | partial (0, 2π) | R − r ≤ clearance | existing `RevolveAxisIntersectsProfile` (UNCHANGED) |
| Circle | full turn | R − r > clearance | **NEW: closed ring torus** |
| Circle | full turn | R ≤ clearance (center on axis) | **NEW typed wall**: `RevolveOnAxisCircleUnsupported` → NotSupported "sweeps a SPHERE (KV6d increment 2)" |
| Circle | full turn | 0 < R − r ≤ clearance, R > clearance (crossing/touching off-center) | `RevolveAxisIntersectsProfile` (invalid input, matches partial) |
| Circle | angle > 2π + band | existing `RevolveInvalidAngle` (UNCHANGED) |
| Polygon / ArcPolygon | any | any | UNCHANGED (out of scope) |

## Topology (the closed-torus B-Rep)

Minimal CW structure of the torus, the full-turn closure of the existing
partial-tube convention (caps vanish; the two profile rims merge into one
poloidal seam; the two longitude seam arcs merge into one toroidal seam):

- **V = 1**: seam anchor `v0` at the outer equator (θ = 0, φ = 0), position
  `center_torus + (R + r)·ŵ`.
- **E = 2**: the poloidal PROFILE circle at θ = 0 (`Curve::Circle`, radius
  `r`, center = embedded profile center `c3`) and the toroidal OUTER-EQUATOR
  circle (`Curve::Circle`, radius `R + r`, center = `center_torus`), both
  closed (`origin == destination == v0`).
- **F = 1**: one `Surface::Torus` face; outer loop = 4 half-edges
  `[prof_fwd, eq_fwd, prof_back, eq_back]` (the aba⁻¹b⁻¹ square of the cut
  torus). BOTH twin pairs are internal to the loop (precedent: the partial
  torus seam-arc twin pair lives in one loop).
- **Shell genus = 1**; Euler–Poincaré `V − E + F − R = 1 − 2 + 1 − 0 = 0 =
  2(S − G)` ✓.
- `RevolveResult`: `start_cap = end_cap = None` (KV6a-tilted made these
  `Option`), `walls = [torus face]`.
- Twin half-edges on the same circle carry opposite-sign directional
  normals (the existing curve-twin sign-canonicalized consistency rule).

## Invariants

1. Every arena invariant enforced by `validate_solid` (twin pairing,
   curve-twin consistency, vertex fan, curved orientation, Euler–Poincaré
   with genus 1).
2. Both seam circles lie exactly on the torus surface
   (`geom::torus_residual = 0` up to the curved-surface debug band).
3. Pappus: solid volume = `2π²·R·r²` (render-mesh volume within the chord
   sag band; exactness improves with `n_seg`).
4. Render mesh: watertight, 2-manifold, outward normals (mesh volume > 0).
5. Determinism: two identical revolves produce byte-identical arenas/meshes.
6. Boolean re-entry: `to_yang` converts the closed face; yang Stage 1
   produces a WATERTIGHT bijectively-mapped doubly-periodic grid
   (`V − E + F = 0` on the tessellation); the C0065 chain (torus − notch box,
   ring severed) completes with χ = 2 and the meta volume.

## Oracles

- kernel-v2 `tests/kv6d_closed_torus.rs`: topology census (V/E/F/loop
  shape/genus), on-surface residual of both seam circles, Pappus volume via
  the render mesh (R = 1.2, r = 0.3 → `2π²·1.2·0.09 ≈ 2.13189`), watertight
  + manifold render mesh, determinism, and the three rejection branches
  (on-axis sphere wall message contains "SPHERE"; crossing keeps
  `RevolveAxisIntersectsProfile`; partial path byte-identical).
- yang-rs unit: Stage-1 tessellation of the converted closed-torus BRep is
  watertight (every edge used exactly twice) and χ = 0.
- e2e: assay `C0065` flips `UNSUPPORTED(revolve)` → `SUPPORTED_CORRECT`
  (tracker knob: χ = 2, volume). `C0067` keeps `UNSUPPORTED(revolve)` with
  the new sphere message. Full assay zero-lost.

## Failure modes

- On-axis full-turn circle → typed `RevolveOnAxisCircleUnsupported` →
  `KernelError::NotSupported` (assay `UNSUPPORTED(revolve)`).
- Off-center crossing/touching → `RevolveAxisIntersectsProfile` (plain
  error, invalid input).
- Downstream boolean walls (if any torus×plane curve class is missing in
  Stage 3/4) surface as their own typed errors — NOT masked here.

## Research basis

- [#24] Yang et al. 2025 — the hybrid pipeline consuming the operand
  (Stage 1 structured tessellation; the doubly-periodic grid mirrors the
  PR-YR12 sphere lat/long arm and the KV6d-4b partial-torus grid).
- Stroud 2006 §3.1.4 — single-fake-edge / seam representation of closed
  curved faces (the KV5a cylinder precedent; here extended to the standard
  minimal CW decomposition of T²: 1 vertex, 2 seam edges, 1 face).
- Mäntylä 1988 — Euler–Poincaré bookkeeping with genus (`2(S − G)`).

## Analytical vs. approximate method justification

- **Method**: the operand construction is exact/analytic (`Surface::Torus`
  + `Curve::Circle` seams). No SSI is computed by this increment.
- **Surface pairs**: the C0065 downstream boolean encounters torus×plane;
  its curve representation is whatever the EXISTING pipeline already uses
  for C0066 (partial torus + bore, SUPPORTED_CORRECT today). This increment
  adds no new approximate path; if the closed-ring configuration exposes a
  missing torus×plane curve class it stays a loud typed wall (M5 family).
