# KV6 — On-axis PARTIAL revolve, slice 3: the wedge (lathe rectangle swept < 360°)

Status: SPEC (2026-07-08, task #85). Corpus driver: R0004 (rectangle with one
on-axis edge revolved 39.2°, currently `RevolveAxisIntersectsProfile`), plus
generality: the partial-angle counterpart of on-axis slices 1–2
(`kv6_on_axis_revolve_rectangle.md`, `kv6_on_axis_revolve_oblique.md`), and
the last "partial-angle on-axis" row those specs left typed.

## 0. Goal

Partial-angle (0 < α < 2π) revolve of the slice-1/2 lathe profiles — a 4-gon
with exactly one on-axis edge, two axis-perpendicular cap edges, and one
off-axis edge (axis-parallel OR oblique) — building the **wedge**:

- off-axis edge PARALLEL → cylindrical wedge (cheese wedge).
- off-axis edge OBLIQUE → conical-frustum wedge.

Topology (both): the two on-axis vertices are FIXED by the rotation, so the
θ=0 and θ=α cap faces SHARE the on-axis edge directly (the swept face of the
on-axis edge is degenerate and is not emitted). Census: V = 6 (2 shared
on-axis + 2 per cap ring off-axis), E = 9 (7 cap segments — 4 + 4 minus the
shared on-axis edge — plus 2 sweep arcs), F = 5 (2 planar caps + 2 planar
pie sectors + 1 curved wall), χ = 6 − 9 + 5 = 2.

Every FACE is existing vocabulary: caps = planar polygons; pie sectors =
planar faces with [seg, arc, seg] loops (the annular-sector path with inner
radius 0); the wall = the KV5b/KV6c-5 partial patch [seg, arc, seg, arc]
(cylinder or cone). Only the CONSTRUCTION is new — a direct assembler in the
`on_axis_revolve` recovery family.

## 1. Parameters

Unchanged public surface: `revolve(arena, profile, axis_origin,
axis_direction, angle_rad)`. No new tunables; the same shape gates and
constants as slices 1–2.

## 2. Branch table

| On-axis profile × angle | Today | After |
|---|---|---|
| 4-gon, one on-axis edge, parallel off-axis edge, FULL turn | solid cylinder (slice 1) | byte-identical |
| 4-gon, one on-axis edge, oblique off-axis edge, FULL turn | solid frustum (slice 2A) | byte-identical |
| 4-gon, one on-axis edge, parallel off-axis edge, PARTIAL | `RevolveAxisIntersectsProfile` | **cylindrical wedge** |
| 4-gon, one on-axis edge, oblique off-axis edge, PARTIAL | `RevolveAxisIntersectsProfile` | **frustum wedge** |
| 3-gon apex triangle, PARTIAL | `RevolveAxisIntersectsProfile` | unchanged (typed; no corpus case) |
| any other on-axis shape / crossing, any angle | `RevolveAxisIntersectsProfile` | unchanged |
| off-axis profiles, any angle | existing behavior | byte-identical |

## 3. Invariants

- **I1 (census):** V=6, E=9, F=5, χ=2; `validate_solid` clean; the wall
  carries `Surface::Cylinder` (parallel) or `Surface::Cone` (oblique, params
  from the slice-2A slant formulas); pie sectors carry ±axis planar normals;
  caps carry the ∓sweep-velocity normals (start cap −m̂, end cap rotated +m̂ —
  the same convention as `build_partial_revolve`).
- **I2 (volume):** exact Pappus fraction — cylinder wedge
  `(α/2)·r²·h`; frustum wedge `(α/2π)·(πH/3)(r₀² + r₀r₁ + r₁²)` — via the
  existing planar/cylinder/cone arc-flux closed forms (no new volume code).
- **I3 (watertight render):** tessellation watertight; mesh volume in the
  chord band of I2.
- **I4 (no regression):** all currently-supported revolves byte-identical;
  the new branch is reached only where today's code returns
  `RevolveAxisIntersectsProfile` on a PARTIAL angle.
- **I5 (booleans):** wedge operands enter yang through the existing partial
  wall + planar-arc conversion paths; pairs outside the supported SSI
  vocabulary keep typed walls (never silent-wrong).
- **I6 (determinism):** identical inputs → bit-identical arenas/meshes.

## 4. Oracles

- **Canonical:** rectangle `(0,0),(H,0),(H,r),(0,r)` (in-plane (t,s)) with
  the `(0,0)–(H,0)` edge ON the axis, revolved 90°, 200°, and R0004's 39.2°:
  census per I1, exact `(α/2)·r²·h` volume, watertight mesh in band.
- **Frustum wedge:** the slice-2A quad (radii r₀ ≠ r₁) at 200°: census,
  `Surface::Cone` wall params (apex/axis/half-angle = the full-turn values),
  exact Pappus volume.
- **Branch coverage:** every §2 row — the two full-turn rows byte-identical
  (pin by replaying a slice-1/2 fixture and comparing), the typed rows
  unchanged (apex triangle partial, crossing, pencil quad partial).
- **Edge:** near-full angle (2π − 1e-3) stays the wedge branch (does NOT
  collapse into the full-turn solid); tiny angle (1e-3 rad) builds and
  measures.
- **Chain (corpus class):** wedge ∪/− box with supported sections succeeds
  or stays typed; R0004 replay leaves `RevolveAxisIntersectsProfile`
  (its second, off-axis revolve chain continues to its own boundary).

## 5. Failure modes

- Crossing / other on-axis shapes / partial apex triangle:
  `RevolveAxisIntersectsProfile` (unchanged, pre-mutation).
- Degenerate (zero height / zero radius): `RevolveAxisIntersectsProfile`.

## 6. Research basis

- [#23 Mäntylä/Stroud] direct assembler with validated exit — the same
  pattern as slices 1–2 (`build_on_axis_frustum`); the shared on-axis edge
  is ordinary 2-manifold topology (two faces meeting at an edge).
- [#24 Yang 2025] no pipeline impact: all wedge faces are existing Stage-1
  vocabulary (planar-with-arcs + partial curved strips, KV6c increment 5).
- Volume closed forms: PR-KV6a planar arc flux + KV6c-5 cone arc flux —
  reused, not extended.

## 7. Analytical vs. approximate

Exact construction (analytic surfaces + `Curve::Arc`); no SSI in
construction; boolean refinement unchanged.

## 8. Design

`on_axis_revolve` is currently entered only for `full_turn` (the recovery
arm in `revolve()`). Widen the recovery arm to all angles; inside, the
shared shape gates (4-gon, exactly 2 ADJACENT on-axis vertices, equal-radius
or oblique off-axis classification) stay identical; dispatch:

- full turn → existing slice-1/2 builders (byte-identical);
- partial → new `build_on_axis_wedge(arena, frame-ish params, angle)`:
  direct assembler over 6 vertices / 18 half-edges / 9 edges / 5 faces.
  Half-edge layout mirrors `build_partial_revolve`'s conventions (caps wind
  CCW around ∓m̂; arcs carry axis-directional normals; the shared on-axis
  edge twins cap0 ↔ cap1 directly). The wall surface comes from the same
  EdgeClass-style classification the full-turn path uses.

The 3-gon apex-triangle partial (cone wedge with apex) is OUT of this slice
(typed): its lateral has an on-axis apex POINT on the boundary between the
two pie sectors — a distinct vocabulary (no corpus driver).
