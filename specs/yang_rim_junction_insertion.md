# Spec: Stage-1 cap-rim junction insertion (§4.3.3 Case IV, N2/F0059 epic increment 2)

> Status (2026-07-10): DESIGN — measured on F0059 (task #122). Successor to
> increment 1 (`yang_collapse_membrane_cancellation`, SHIPPED) in the
> N2/F0059 epic; plan of record in `yang_stage4_conic_triple_junction.md`'s
> 2026-07-10 status block.

## Goal

Retire the F0059 cap-face self-intersecting-loop wall (kernel-v2 render CDT
`"ring rejected by CDT"`) and, by the same mechanism, the suspected shared
wall of the TessellationFailed ring-reject family (F0045/R0011 class census
after landing).

**Measured mechanism (F0059, Steinmetz r=0.35, h=0.5 — h/2 < r so the caps
truncate the seam):** each cap disc of operand A intersects operand B's
boundary; the kept cap material is four circular-segment lobes whose corners
are the EXACT rim junction points (rim circle ∩ ∂B, e.g. `(±0.25, ±0.2449)`
in cap frame, `0.2449 = √(r²−(h/2)²)`). The Stage-1 chord-sampled rim
polyline cuts INSIDE the exact circle, so near each corner it crosses the
trim chords: the mesh-level seam chains cannot terminate at the true
junction (it lies outside the sampled cap polygon), the lobes stay
edge-connected through corner-cutting slivers, and the emitted single
boundary loop self-intersects at sub-sagitta scale. S7's §4B split arm
cannot catch this — the junction sits a full sagitta off the crossing chord,
≫ TAU_WORK.

**Paper-faithful fix (Yang §4.3.3 Case IV):** represent the crossing IN the
mesh — force Stage-1 rim samples AT the analytically-derived junction
azimuths, before tessellation. The infrastructure exists end-to-end:

- `stage1_tessellate_with_rim_overrides` (M8 disc-rim crossing) inserts
  exact extra points into a full-circle rim ring, angle-sorted, and routes
  affected laterals through the azimuth-merge strip.
- With the rim passing exactly through the junctions, the seam chains
  terminate there, the four lobes become vertex-pinched patches → the
  KV9-F1 figure-eight wedge walk + pinch split emit them as separate loops.
- Each lobe (chord + arc between the same two vertices) is the SHIPPED
  D-face bigon vocabulary (`dface_bigon_campaign`).
- The inserted vertex lies on the rim circle AND the trim curve(s): the
  line+circle case is the SHIPPED `vert_junction` Stage-4 arm.

## Parameters

- `rim_junction_overrides(a: &BRep, b: &BRep) -> (BTreeMap<u32, Vec<Point3>>, BTreeMap<u32, Vec<Point3>>)`
  — per operand, per full-circle rim edge, the exact junction points of that
  rim circle with the OTHER operand's boundary surfaces.
- New `BRep::rebuilt_with_rim_overrides(map)` mirroring
  `rebuilt_with_min_rim_segments` (both must COMPOSE: the Case-IV phantom
  guard's `min_n_seg` and the override map thread through ONE
  `from_topology` variant).
- Wire in `boolean()` beside the phantom guard, BEFORE Stage 0.

## v1 scope (closed forms only, per A13.3/P8 — no ad-hoc root-finding)

For a rim circle C (center c, unit normal n, radius r, in plane P) of
operand X against each face of operand Y:

| Y face | section of Y's surface in plane P | junction solve | scope |
|---|---|---|---|
| Plane face (normal m) | line P∩plane | circle∩line in-plane (quadratic) | v1 |
| Cylinder lateral, axis d ∥ P (`|n·d| ≤ tol_axis`) | two parallel lines | circle∩line ×2 (quadratic) | v1 |
| Cylinder lateral, axis transversal | ellipse in P | circle∩ellipse (quartic) | LOUD-SKIP v1: no insertion, current behavior preserved |
| Sphere / cone / torus | conic / quartic in P | — | LOUD-SKIP v1 |

Each candidate point must pass ALL of:
1. **On-arc**: the rim edge is a full circle (v1: partial arcs skipped).
2. **Within Y's face extents**: for a plane face, inside the face's outer
   loop (2D containment); for a cylinder lateral, within its z-extent
   [z_lo, z_hi] along d.
3. **Transversality**: the rim crosses ∂Y at the point (the trim curve is
   not tangent) — |tangent_C × tangent_section| > derived floor; tangent
   grazes are the §4.3.3 tangency class (existing pinch machinery), skipped
   loudly.
4. **Separation from uniform samples**: `stage1_tessellate_with_rim_overrides`
   already errors loudly on an angular tie with a uniform sample; the caller
   does NOT pre-filter (never silently merge).

## Branch table

| condition | action |
|---|---|
| no junction found on any rim (the common case) | maps empty → operands byte-identical (B1 oracle) |
| junctions found on operand X's rims | X rebuilt with overrides; Y likewise independently |
| Stage-0 path re-tessellates a face of a rebuilt operand | overrides MUST thread through (`forced_rim_n` precedent — the M8 incr-15 trap: Stage-0 re-tessellations silently discard a boolean()-entry boost). v1: if a rim WITH overrides enters a Stage-0 re-tessellation path that cannot honor them → loud `CoplanarFacesUnsupported`-style typed stop, never silent divergence |
| quartic-class junction geometry (transversal-axis cyl, sphere, …) | no insertion (documented residue; current loud walls remain) |
| candidate fails transversality | no insertion (tangency class, existing machinery) |

## Invariants

- **I1 (byte-identity off)**: empty override maps ⇒ both operands and the
  whole pipeline byte-identical (existing
  `rim_override_empty_is_byte_identical` covers Stage 1; extend to
  `boolean()` level).
- **I2 (exactness)**: every inserted point satisfies |‖p−c‖−r| ≤ TAU_WORK,
  |(p−c)·n| ≤ TAU_WORK, and lies on Y's surface to TAU_WORK.
- **I3 (chord validity)**: inserting a rim sample only SHRINKS sagittas —
  never a tolerance relaxation (A14.3).
- **I4 (determinism)**: junction enumeration in (face index, root order)
  order; BTreeMap keyed by edge index.

## Oracles

- **Unit**: derivation returns the four exact corners for the F0059 cap
  configuration (r=0.35, h=0.5: `(±h/2, ±√(r²−h²/4))` in cap frame);
  empty for the kv9f1 configuration (h/2 > r: seam never reaches caps);
  empty for disjoint operands.
- **Red→green (epic target)**: truncated-Steinmetz union end-to-end in
  yang-rs — `boolean(a,b,Union)` Ok, watertight, volume =
  `2πr²h − V_common` with
  `V_common = 2·z0·h² + 8(2r³/3 − r²·z0 + z0³/3)`, `z0 = √(r²−h²/4)`
  (piecewise box-truncated bicylinder). `#[ignore]`-pinned until the
  Stage-4 over-determined-junction exactness escape (increment 3) also
  lands; the un-ignored live pin asserts the CURRENT typed wall so drift
  is loud.
- **Assay**: F0059 ERROR→CORRECT is the class oracle; full corpus 0 WRONG,
  no CORRECT lost; ring-reject family (F0045/R0011/R0012/R0016/R0028/
  R0059/R0072/R0098) re-censused after landing.

## Failure modes / residue

- After insertion the corner vertex lies on ≥3 curves (rim circle + trim
  line + seam ellipse) → today's over-determined Stage-4 audits STOP.
  **Increment 3 = exactness-first escape**: an over-determined junction
  vertex ALREADY within TAU_WORK of all incident surfaces is retagged
  without relocation (never a silent pick — the position is verified
  exact); anything else keeps the loud STOP. This likely supersedes wiring
  the banked Newton handler for the F0059 class.
- Quartic-class junctions: documented residue, loud (existing walls).

## Research Basis

- [#24] Yang et al. 2025 §4.3.3 Case IV (intersection through mesh
  vertices/edges — subdivide so the crossing is represented; also the
  tangency-insertion analog), §4.5.5 (shared-boundary sampling).
- M8 increment 15 (`yang_case_iv_phantom_guard`): pair-derived Stage-1
  density + the Stage-0 pass-through trap this spec must honor.
- M8 increment 6 (`m8_increment6_annular_rim_mint`): the rim-override
  insertion vocabulary this reuses.
- [#1] Patrikalakis Ch.5 (plane-quadric sections; circle∩line closed form).
