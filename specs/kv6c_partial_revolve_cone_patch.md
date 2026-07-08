# KV6c increment 5 — Partial revolve of an oblique edge: the arc-bounded cone patch

Status: SPEC (2026-07-08, task #81/#82). Corpus driver: the single largest
capability wall in the assay — **34 UNSUPPORTED(revolve) cases** (R0002/03/04/05/
08/10/16/17/18/19/20/32/33/34/35/37/44/47/49/52/53/54/55/65/68/69/71/80/81/85/
93/94/99/100), every one of them `revolve_face: partial revolve of an oblique
edge sweeps an arc-bounded CONE patch`. The census (2026-07-08) shows the rest
of the 38-case revolve stock is: 2× closed torus (KV6d), 1× partial-patch
boolean re-entry (R0051), 1× coplanar Stage 0 (F0075) — all separate walls.

Two increments, independently committable:

- **Increment 1 — kernel-v2** (task #81): `build_partial_revolve` emits the
  `Surface::Cone` wall for `EdgeClass::Oblique`; validate / signed_volume /
  render tessellation accept the partial cone patch. A standalone partial
  revolve with oblique edges builds, validates, measures, and renders.
- **Increment 2 — yang-rs Stage 1** (task #82): the partial cone STRIP arm in
  `tessellate_cone_face` (the ruled analog of the partial cylinder arm), so
  fresh partial-cone solids can enter booleans. kernel-v2's `to_yang`
  conversion already accepts the `[seg, arc, seg, arc]` wall pattern
  surface-generically (boolean.rs "Cylinder, cone, and torus laterals share
  this conversion").

## 0. Goal

Partial-angle revolve (0 < α < 2π) of a hole-free polygon profile whose edge
classification contains `EdgeClass::Oblique` — today rejected typed before any
mutation. After this slice the oblique edge sweeps an **arc-bounded cone
patch**: the wall face `[seg, arc, seg, arc]` (two slant ruling segments in
the θ=0 / θ=α cap planes + two sweep arcs at the edge's two radii), carried on
`Surface::Cone { apex, axis_dir, half_angle, reversed }`. Topologically
IDENTICAL to the existing partial cylinder wall — `build_partial_revolve`'s
half-edge layout is already class-generic; only the `Surface` arm, validation,
volume flux, and tessellation are new.

## 1. Parameters

Unchanged public surface: `revolve(arena, profile, axis_origin,
axis_direction, angle_rad)`. No new tunables. The cone parameters come from
`EdgeClass::Oblique` exactly as classified today (`apex` = slant∩axis,
`axis_dir` oriented so both rims sit at τ > 0, `half_angle = atan|Δs/Δt|`,
`reversed = dt > 0` — the same material-sense rule as `Parallel`).

## 2. Branch table

| Case | Today | After |
|---|---|---|
| partial revolve, all edges Parallel/Perpendicular | supported (KV6a) | byte-identical (I6) |
| partial revolve, ≥ 1 Oblique edge, off-axis profile | `RevolveObliqueEdgeUnsupported` | **builds** (increment 1) |
| full-turn revolve, Oblique edges | supported (KV6c 4) | byte-identical (I6) |
| partial revolve, on-axis profile | `RevolveAxisIntersectsProfile` | unchanged (typed) |
| boolean with a fresh partial-cone solid | (unreachable — construction rejected) | enters yang Stage 1 (increment 2); pairs outside the supported SSI vocabulary keep their typed walls |
| render tessellation of a NON-canonical cone patch (boolean output: chords, trim loops) | `CurvedGeometryMismatch("partial Surface::Cone patch not yet implemented")` | **tessellates** via the shared developable-patch engine (increment 1, I5) |
| `signed_volume` of a cone patch whose segment is neither a ruling nor angular-extent-free | (unreachable) | typed `CurvedGeometryMismatch` (no closed form for chord facets — mirror of the cylinder rule) |
| EllipseArc edge on a cone patch (oblique cone section) | (unreachable) | typed `CurvedGeometryMismatch` in validate/volume (the oblique conic-cut vocabulary is a later slice) |

## 3. Invariants

- **I1 (shape):** partial revolve of a k-gon with oblique edges keeps the
  KV6a census V=2k, E=3k, F=k+2, χ=2; oblique walls carry `Surface::Cone`
  with the EdgeClass parameters; `validate_solid` clean.
- **I2 (volume):** `signed_volume` of the partial solid equals Pappus:
  `V = (α/2π) · V_full_turn`, where the full-turn value is the shipped exact
  form. Per-arc cone flux closed form (§6): an arc at axial coordinate τ_c
  (from the apex) with signed sweep Δθ about `+axis_dir` contributes
  `−(tan α_c/3)·(τ_c²/2)·(apex·(t̂_start − t̂_end) − tan α_c·(apex·axis)·Δθ)`
  (α_c the half-angle); rulings contribute nothing. The formula must
  reproduce the shipped full-band closed form `−(π/3)(apex·axis)(ρ_hi²−ρ_lo²)`
  in the Δθ = ±2π limit (unit-verified).
- **I3 (validation):** `validate_cone_face` dispatches like the cylinder: any
  full-circle edge → existing canonical/apex forms (byte-identical); NO
  full-circle edge → new `validate_cone_patch`, the unrolled-winding analysis
  of `validate_cylinder_patch` in the cone's (θ, τ) development — per-arc
  surface agreement (`r_arc = τ_c·tan α_c` within 1e-9·max(expected,1), arc
  axis ∥ cone axis, τ_c > 0), segment principal-value azimuth steps, integral
  net winding, material-CCW orientation rules (bounded patch: exactly one CCW
  loop; band: +1/−1 wraps with +1 at the LOWER τ), mirrored for `reversed`.
- **I4 (watertight render):** tessellation of the partial-cone solid is
  watertight and its mesh volume converges to the exact volume within the
  chord band; shared arcs sample twin-symmetrically (cross-face watertight
  with the planar sector caps by construction).
- **I5 (one engine):** the cone patch render tessellation reuses the SAME
  unrolled-CDT machinery as the cylinder patch (crate hard rule 5 — one
  canonical implementation per surface type, one shared developable engine
  parameterized by the surface development), not a parallel re-implementation.
- **I6 (no regression):** all currently-supported revolves and boolean paths
  byte-identical; the new code is reached only where today's paths reject.
- **I7 (loud walls):** every input outside this vocabulary keeps a typed
  error. No tolerance widening, no silent skips (P9).
- **I8 (determinism):** identical inputs → bit-identical arenas and meshes.

## 4. Oracles

- **Canonical:** revolve the trapezoid `(1,0),(3,0),(2,1),(1,1)` (in-plane
  (s,t) coords; one oblique edge (3,0)→(2,1)) about an in-plane axis by 90°,
  180°, and a non-quadrant angle (e.g. 200°) → census per I1, exactly one
  `Surface::Cone` wall, `validate_solid` clean, `signed_volume` = Pappus
  fraction of the full-turn volume (analytic), watertight tessellation with
  mesh-volume convergence.
- **Cavity sense:** a profile whose oblique edge has `dt > 0` (material on
  the larger-radius side — a conical bore wall) builds with `reversed = true`
  and validates; volume matches Pappus.
- **Branch coverage:** every row of §2's table.
- **Edge:** near-quadrant angles (the `snap_trig` band), a slender oblique
  edge (small Δs), a profile with TWO oblique edges (both walls cone,
  distinct apexes/half-angles).
- **Regression:** the kv6a pin `oblique_edges_circle_profiles_and_holes_
  rejected_typed` flips its oblique arm to a green build target (the circle/
  holes arms stay typed); full-turn cone tests byte-identical.
- **Boolean chain (increment 2):** partial-cone solid ∪/− box or coaxial
  cylinder where the intersection stays within the supported SSI vocabulary →
  succeeds with correct volume; pairs outside it (e.g. non-coaxial cyl×cone
  lateral∩lateral) keep typed walls.
- **Corpus (P9 gate):** replay the 34 R-series cases: each leaves
  UNSUPPORTED(revolve); target state is SUPPORTED_CORRECT or a DEEPER honest
  typed wall (M5-class SSI, curved re-entry). Full assay: 0 WRONG, zero
  CORRECT lost.

## 5. Failure modes

- On-axis partial revolve: `RevolveAxisIntersectsProfile` (unchanged).
- Cone patch arcs off-surface / mismatched radius / non-parallel axis:
  `CurvedGeometryMismatch` (loud, per I3).
- `signed_volume` on chord-segment cone patches: typed (no closed form).
- Boolean pairs outside the supported SSI vocabulary: existing typed walls
  (`UnsupportedCurvedBoolean`, `BooleanFailed`, M5 boundaries) — never WRONG.

## 6. Research basis

- [#23 Mäntylä/Stroud] wall-loop topology unchanged from PR-KV6a's partial
  revolve (class-generic half-edge layout).
- [#24 Yang et al. 2025] Stage-1 bijective tessellation: the partial cone
  strip mirrors the partial cylinder strip (ruled surface, chain-paired
  quads) — yang-rs `tessellate_lateral_face`'s partial arm is the template.
- **Cone flux derivation** (this spec, verified against the shipped
  full-band form): on the cone `x·n̂ = cos α·(apex·r̂) − sin α·(apex·â)`
  (τ-independent — the τ terms cancel exactly since ρ = τ·tan α), so the
  divergence-theorem integral in the (θ, τ) development reduces by Green's
  theorem to per-arc boundary terms `−(τ²/2)·∫g dθ`,
  `g(θ) = apex·r̂(θ) − tan α·(apex·â)`, `∫r̂ dθ = t̂_start − t̂_end` — the
  structural twin of the shipped `cylinder_arc_patch_flux` (PR-KV6a).
- Unrolled-winding validation: the developable-surface Newell generalization
  already pinned for cylinders (PR-KV5b); a cone is developable, the same
  analysis applies in its (θ, τ) chart.

## 7. Analytical vs. approximate

Exact: analytic `Surface::Cone` + `Curve::Arc` boundaries from construction;
exact-rational π-coefficient volume path unchanged. Booleans refine to SSI
curves exactly as today (cone∩plane closed forms; degree-4 pairs remain the
M5 typed boundary — temporary, per the roadmap). Render tessellation is the
usual chord-band approximation (display only, never the representation).

## 8. Design

**construct.rs** — delete the pre-build oblique rejection in `revolve()`;
`build_partial_revolve`'s surface match arm maps `EdgeClass::Oblique
{ apex, axis_dir, half_angle, reversed }` → `Surface::Cone` verbatim. The
arc curves already carry per-vertex radii (`fr.s[i]`), correct for cones.

**validate.rs** — `validate_cone_face`: hoist a `has_full` dispatch (mirror
of `validate_cylinder_face`); no full circles → new `validate_cone_patch`
(unrolled (θ, τ) winding; shares the loop-walk structure with
`validate_cylinder_patch`; τ from apex along `axis_dir`, radial check
against `cone_radius_at`).

**geom.rs** — `signed_volume`: add the `Surface::Cone` arm to the
`has_arcs` dispatch → new `cone_arc_patch_flux` (per-arc closed form, §6;
ruling check: segment direction ⊥ t̂ at its start, i.e. zero angular extent,
else typed).

**tessellate.rs** — replace the typed reject with `tessellate_cone_patch`:
extract the cylinder patch's unroll+CDT core into a shared developable-patch
engine taking the surface development as closures (`(θ, v)` projection,
`unroll_u`, `surface_point`); cylinder supplies `v = h, u = sense·θ·r`; cone
supplies `v = τ, u = sense·θ·r_ref` (r_ref = the face's maximum boundary
radial distance — deterministic, positive; distortion is irrelevant to CDT
correctness, bijectivity is what matters).

**yang-rs lib.rs (increment 2)** — `tessellate_cone_face`: split
`circle_edges` into closed rims (`start == end`) vs arcs; `[2 arcs, rest
LineSegment]` → partial STRIP arm (index-paired chains, `cone_outward_normal`
orientation, `reversed` flip) mirroring `tessellate_lateral_face`'s partial
arm. Verify the Stage-1 chord bound gives BOTH arc chains identical sample
counts (the cone-aware `cone_chord_bound` already forces a shared count for
the frustum band's two rims — confirm the arc-chain path inherits it; if not,
force the shared count for cone-face arc chains explicitly).
