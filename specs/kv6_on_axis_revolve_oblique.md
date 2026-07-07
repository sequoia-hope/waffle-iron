# KV6 — On-axis full-turn revolve, slice 2: oblique profiles → solid frustum / solid cone

Status: increment A **SHIPPED** (2026-07-07, task #66 — C0064 flips
ERROR → SUPPORTED_CORRECT on its exact-volume oracle, pinned green);
increment B **SHIPPED** (2026-07-07 — C0063 moves ERROR →
UNSUPPORTED(curved-profile): the solid cone builds, the oblique slab cut
lands on the typed curved re-entry wall, pinned). Corpus drivers: C0064 (three stacked solid
frusta, exact-volume oracle) and C0063 (solid cone + oblique slab cut —
tracker case; the cut stays on the conic-patch typed boundary, but the
PRIMARY solid must build). Slice 1 (`kv6_on_axis_revolve_rectangle.md`)
recovered the on-axis RECTANGLE as the canonical cylinder; this slice
recovers the two on-axis shapes with an OBLIQUE edge, using the KV6c cone
vocabulary (`Surface::Cone`, `validate_cone_face`,
`tessellate_cone_lateral`, the cone `signed_volume` flux).

Two increments, independently committable:

- **Increment A — solid frustum** (the C0064 profile): on-axis 4-gon whose
  off-axis edge is oblique. Output is the 2-rim seamed cone band the KV6c
  vocabulary already validates/tessellates/booleans (PR-KV6c 5c) — only
  the CONSTRUCTION is new.
- **Increment B — solid apex cone** (the C0063 primary): on-axis 3-gon
  whose oblique edge reaches the axis (apex ON the axis). New 1-rim apex
  form in `validate_cone_face`, `signed_volume`, and the tessellator;
  booleans stay on the typed `UnsupportedCurvedBoolean` boundary (the
  lateral's 1-half-edge loop fails `to_yang`'s 4-edge pattern loudly).

## 0. Goal

Full-turn revolve of the two most common on-axis lathe profiles with a
slanted edge:

- **Frustum quad** `(0,t₀) (0,t₁) (r₁,t₁) (r₀,t₀)` — on-axis edge, two
  axis-perpendicular cap edges, one oblique off-axis edge with r₀ ≠ r₁ →
  the **solid frustum** (truncated cone). Same topology as the solid
  cylinder (2 seam vertices, 3 edges: 2 rims + 1 seam ruling, 3 faces),
  lateral surface `Surface::Cone`.
- **Apex triangle** `(0,t₀) (0,t₁) (r,t₀)` (either orientation) — on-axis
  edge, ONE axis-perpendicular cap edge, one oblique edge from the
  off-axis vertex to an on-axis vertex → the **solid cone**. Topology:
  1 seam vertex, 1 edge (the base rim circle), 2 faces (disc cap + apex
  cone lateral), χ = 1−1+2 = 2. The apex is an interior singular point of
  the lateral (yang's own cone model), NOT a topological vertex.

## 1. Parameters

Unchanged public surface: `revolve(arena, profile, axis_origin,
axis_direction, angle_rad)`. No new tunables — reuses
`REVOLVE_MIN_AXIS_CLEARANCE_REL` (on-axis classification) and
`REVOLVE_EDGE_ALIGNMENT_TOLERANCE` (edge-alignment bands).

## 2. Branch table

| Profile × axis (full turn) | Today | After |
|---|---|---|
| on-axis 4-gon, off-axis pair at EQUAL radius (slice 1) | solid cylinder | unchanged |
| on-axis 4-gon, perpendicular caps, off-axis radii DIFFER | `RevolveAxisIntersectsProfile` | **solid frustum** (increment A) |
| on-axis 3-gon, one perpendicular cap, oblique edge to on-axis vertex | `RevolveAxisIntersectsProfile` | **solid apex cone** (increment B) |
| on-axis 3-gon, BOTH connector edges oblique (bicone / spinning top) | `RevolveAxisIntersectsProfile` | unchanged (typed; later slice if ever needed) |
| on-axis 4-gon with an OBLIQUE cap edge (pencil: cylinder + cone tip) | `RevolveAxisIntersectsProfile` | unchanged (typed; needs mixed lateral+apex vocabulary) |
| on-axis, > 4 vertices / non-aligned / crossing | `RevolveAxisIntersectsProfile` | unchanged |
| any on-axis profile, PARTIAL angle | `RevolveAxisIntersectsProfile` | unchanged |
| off-axis profiles (washers, conical washers, partials) | existing behavior | byte-identical (I4) |

## 3. Invariants

- **I1 (frustum shape):** census V=2, E=3, F=3 (2 disc caps + 1 seamed
  cone lateral); lateral `Surface::Cone` with `apex` = slant∩axis,
  `axis_dir` oriented so both rims sit at τ > 0, `half_angle =
  atan(|Δr|/H)`, `reversed = false`; `validate_solid` clean.
- **I2 (frustum volume):** `signed_volume = (π·H/3)(r₀² + r₀r₁ + r₁²)`
  exactly (the dashu-exact π-coefficient path), tessellation-independent.
- **I3 (apex-cone shape):** census V=1, E=1, F=2 (disc cap + apex
  lateral); the lateral's outer loop is the single closed base-rim circle;
  `Surface::Cone` with apex ON the revolve axis at the on-axis oblique
  endpoint; `validate_solid` clean.
- **I4 (no regression):** every currently-supported revolve byte-identical
  — the new branches are reached only where today's code returns
  `RevolveAxisIntersectsProfile` full-turn.
- **I5 (apex-cone volume):** `signed_volume = π·r²·h/3` exactly.
- **I6 (result contract):** frustum keeps the slice-1 convention —
  `start_cap` at the axial minimum (outward −â), `end_cap` at the maximum
  (+â), `walls` = the lateral. The apex cone has exactly ONE planar cap;
  the apex end has no face to name, so `start_cap` and `end_cap` BOTH
  reference the single disc cap (documented on the constructor) and
  `walls` = the lateral.
- **I7 (boolean boundary):** frustum outputs enter yang booleans through
  the existing 2-rim path (PR-KV6c 5c: flat ⊥-axis cuts work); apex-cone
  operands return typed `UnsupportedCurvedBoolean` — never silent.
- **I8 (determinism):** identical inputs → bit-identical arenas/meshes.

## 4. Oracles

- **Canonical A:** revolve `(0,0),(H,0),(H,r₁),(0,r₀)` about x, 360° →
  V/E/F census, `Surface::Cone` params (apex/axis/half-angle), exact
  frustum volume, watertight mesh with volume in the chord band,
  `validate_solid`.
- **Canonical B:** revolve `(0,0),(H,0),(0,R)` (apex at `(H,0)`) → census
  per I3, exact volume `π·R²·H/3`, watertight mesh, `validate_solid`.
- **Branch coverage:** bicone triangle stays typed; pencil quad stays
  typed; partial-angle frustum profile stays typed; crossing stays typed
  (existing pin); slice-1 rectangle byte-identical.
- **Chain (A):** frustum − coaxial ⊥ slab cut (the PR-KV6c 5c supported
  class) succeeds and keeps a `Surface::Cone` face.
- **Boundary (B):** apex-cone boolean operand → typed
  `UnsupportedCurvedBoolean`/`NotSupported`, arena untouched.
- **Corpus (P9 gate):** C0064 leaves ERROR (primary frusta build; the
  chained coaxial unions either go CORRECT or land on a deeper honest
  typed wall); C0063 leaves `RevolveAxisIntersectsProfile` for its real
  boundary (the oblique conic cut). Full assay: 0 SUPPORTED_WRONG, zero
  CORRECT lost.

## 5. Failure modes

- Crossing profiles: `RevolveAxisIntersectsProfile` (unchanged).
- On-axis, not a slice-1/2 shape: `RevolveAxisIntersectsProfile`
  (unchanged, pre-mutation).
- Degenerate (zero height, zero base radius, apex-AND-equal-radii
  contradictions): `RevolveAxisIntersectsProfile` — never a silent sliver.
- Apex-cone boolean operand: `UnsupportedCurvedBoolean` (typed, loud).

## 6. Research basis

- [#23 Mäntylä / Stroud §3.1.4] single-fake-edge closed-curved-edge
  topology — the frustum assembler mirrors `extrude_circle` verbatim; the
  apex cone uses the same 1-closed-edge disc-cap form already in the
  vocabulary (PR-KV5a caps).
- [#24 Yang et al. 2025] cone tessellation/apex handling: yang-rs's own
  `tessellate_cone_face` models the apex-pointed cone as a disk with the
  apex an interior singular point — the kernel-v2 apex form matches it.
- Frustum flux `−(π/3)(apex·axis)(ρ_hi²−ρ_lo²)` (PR-KV6c increment 2)
  specializes to the apex cone with `ρ_lo = 0` — same derivation, no new
  formula.

## 7. Analytical vs. approximate

Exact: analytic `Surface::Cone` + `Curve::Circle` rims; no SSI involved
in construction. Boolean refinement reuses the existing cone∩plane SSI
when chained (unchanged by this slice).

## 8. Design

`on_axis_rectangle_revolve` generalizes to an `on_axis_revolve`
classifier (same recovery entry point, same frame re-derivation): after
the shared gates (hole-free polygon, exactly 2 ADJACENT on-axis
vertices), dispatch on vertex count and edge classes:

- 4-gon, both cap edges perpendicular, off-axis radii equal → slice-1
  delegation to `extrude` (unchanged).
- 4-gon, both cap edges perpendicular, radii differ → direct frustum
  assembler (new `build_on_axis_frustum`, mirroring `extrude_circle`:
  same ids/order/curve conventions, per-rim radii, `Surface::Cone` from
  the slant via the SAME apex/axis/half-angle formulas as
  `EdgeClass::Oblique` classification).
- 3-gon, exactly one connector perpendicular, other connector oblique
  with its on-axis endpoint as apex → direct apex-cone assembler
  (increment B): 1 vertex, 2 half-edges (cap rim + lateral rim twins),
  2 faces.
- anything else → the original typed error, pre-mutation.
