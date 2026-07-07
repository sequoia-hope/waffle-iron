# KV6 — On-axis full-turn revolve, slice 1: rectangle → solid cylinder

Status: **SHIPPED (2026-07-07, task #65).** Corpus drivers: C0061, C0062,
C0069 (and the primary op of every "lathe shaft" composition) — all three
flip ERROR → SUPPORTED_CORRECT ("all checks passed", exact-volume
oracles), including their chained groove-cut booleans; corpus 183→186
CORRECT / 0 WRONG / zero lost. C0063/C0064 (on-axis cones/frusta) are
slice 2 (KV6c vocabulary) — explicitly out of scope here, kept on the
typed boundary (pinned by `on_axis_triangle_full_turn_stays_rejected` +
`on_axis_oblique_quad_full_turn_stays_rejected`). Implementation note:
the spec's explicit crossing-rejection branch proved semantically dead
(the ŵ sign rule + shape gates already reject every crossing input with
the same error — verified by an equivalent-mutant check) and was removed
per Constitution §7; `full_turn_crossing_profile_stays_rejected` pins the
behavior.

## 0. Goal

A full-turn (360°) revolve of a rectangular profile with ONE edge lying on
the revolve axis produces the **solid cylinder** ("shaft") — the single
most common lathe operation. Today `revolve` rejects ANY profile vertex at
radial distance ≤ clearance with `RevolveAxisIntersectsProfile`, conflating
two different situations:

- **Crossing** (vertices on both radial sides): genuinely invalid input —
  the swept solid self-intersects. Stays rejected, same error.
- **Touching** (vertices ON the axis, all others strictly one side): a
  legitimate solid of revolution. The on-axis edge sweeps to a degenerate
  line interior to the solid; its adjacent perpendicular edges sweep to
  full DISCS (not annuli); the off-axis parallel edge sweeps the lateral.
  The result is topologically and geometrically EXACTLY the canonical
  cylinder solid the kernel already builds for extrude-of-circle (PR-KV5a).

## 1. Parameters

Unchanged public surface: `revolve(arena, profile, axis_origin,
axis_direction, angle_rad)`. No new tunables — reuses
`REVOLVE_MIN_AXIS_CLEARANCE_REL` (on-axis classification band) and
`REVOLVE_EDGE_ALIGNMENT_TOLERANCE` (edge-alignment bands), both existing.

## 2. Branch table

| Profile × axis | Angle | Today | After |
|---|---|---|---|
| all vertices strictly off-axis | any | washer / partial (existing) | unchanged |
| mixed radial signs (crossing) | any | `RevolveAxisIntersectsProfile` | unchanged |
| 4-gon, exactly one on-axis edge (2 adjacent on-axis vertices), off-axis pair at equal radius, perpendicular edges axial-aligned | full turn | `RevolveAxisIntersectsProfile` | **canonical solid cylinder** (delegates to the extrude-circle construction) |
| on-axis but NOT the above shape (triangle apex on axis, oblique edges, >4 vertices, non-aligned — C0063/C0064 class) | full turn | `RevolveAxisIntersectsProfile` | unchanged (slice 2, KV6c vocabulary) |
| on-axis (touching, any shape) | partial | `RevolveAxisIntersectsProfile` | unchanged (existing test pins this) |

## 3. Invariants

- **I1 (canonical shape):** the result is bit-canonical with the KV5a
  cylinder: 2 vertices, 3 edges (2 rim circles + 1 seam), 3 faces (2 disc
  caps + 1 seamed lateral), χ = 2; `validate_solid` clean.
- **I2 (geometry):** lateral radius = the off-axis edge's radial
  coordinate; caps at the two off-axis vertices' axial coordinates; axis =
  the revolve axis.
- **I3 (result contract):** `RevolveResult.start_cap` = cap at the axial
  minimum (outward −â), `end_cap` = axial maximum (outward +â), `walls` =
  the lateral — same 360° convention as the washer branch.
- **I4 (no regression):** every currently-supported revolve (off-axis
  washers, partials, cones, tori) byte-identical — the new branch is
  reached only where today's code returns `RevolveAxisIntersectsProfile`.
- **I5 (chainability):** the output IS a canonical cylinder, so booleans
  (the C0061/C0069 groove cuts) and tessellation consume it through the
  existing supported paths.

## 4. Oracles

- **Canonical:** revolve rectangle (−r,0),(0,0),(0,h),(−r,h) about z →
  volume π·r²·h (tessellated signed volume within the chord-deficit band),
  V/E/F counts per I1, watertight + outward + validate_solid.
- **Branch coverage:** crossing profile still errors
  `RevolveAxisIntersectsProfile`; on-axis triangle still typed
  (`NotImplemented`, message names the KV6 boundary); partial-angle
  on-axis typed.
- **Chain:** shaft + one full-turn groove cut (C0061's recipe, direct
  constructors) → volume = π·r²·h − groove ring volume.
- **Corpus (P9 gate):** C0061, C0062, C0069 leave ERROR (→ CORRECT or a
  deeper honest wall); full assay 0 SUPPORTED_WRONG, zero CORRECT lost.

## 5. Failure modes

- Crossing: `RevolveAxisIntersectsProfile` (unchanged).
- On-axis, not slice-1 shape: `NotImplemented` (typed, loud).
- Degenerate rectangle (zero height / zero radius after classification):
  `NotImplemented` (never a silent degenerate solid).

## 6. Research basis

- [#23 Mäntylä] Euler-operator solid construction — the cylinder assembly
  reused verbatim (PR-KV5a `extrude_circle`).
- Solid-of-revolution boundary topology: the on-axis edge is the
  degenerate orbit of the rotation's fixed line; its sweep contributes no
  2-face, and incident perpendicular edges sweep discs. (Standard sweep
  boundary analysis; no published algorithm needed beyond the existing
  constructors — stated per FIP §3.2.7.)

## 7. Analytical vs. approximate

Exact: the delegated construction stores the analytic `Surface::Cylinder`
+ `Curve::Circle` rims exactly as extrude-of-circle does. No SSI involved.

## 8. Design

`revolve()` keeps calling `validate_revolve_geometry` first. On
`Err(RevolveAxisIntersectsProfile)` (the ONLY site that returns it in the
full-turn path), a recovery classifier re-derives the axis frame (same
formulas/constants), splits crossing from touching, and for the slice-1
shape synthesizes `Profile::circle(axis_origin + t₀·â, ŵ, m̂, (0,0), r)`
and calls `extrude(…, â, t₁−t₀)`, mapping `ExtrudeResult` →
`RevolveResult` (base→start_cap, top→end_cap, walls→walls). Everything
else returns the typed boundary per the branch table.
