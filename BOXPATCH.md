# BOXPATCH — box.waffle investigation (RESOLVED)

Status: **diagnosed and fixed.** The defect was NOT in the Rust region/profile
layer (BOXPATCH's original hypothesis was wrong). It was the UI sending **raw
drawn coordinates** instead of the solver output to region extraction and to the
finished-sketch wireframe.

## The file

`box.waffle` first sketch (`887d8844…`) = two nested squares centered on origin,
drawn click-by-click then constrained, plus two real diagonals (lines 9 & 19)
that drive `Midpoint` centering constraints. Outer solves to 0.05², inner to
0.04², both centered on the origin; the diagonals cross at the origin.

Two coordinate sources coexist per entity: raw drawn `entity.x/y` (pre-solve
scratch), and `sketch.solved_positions` (the solver output, serialized in the
.waffle and present on the JS feature tree).

## Root cause (corrected)

The original note guessed the bug was in `crates/waffle-types/src/profiles.rs` /
`regions.rs` — that the arrangement didn't split edges at interior crossings,
producing pinched loops and `ProfileRepeatedVertex`. **This was wrong.**

Step-0 diagnostic (`crates/waffle-types/tests/box_xinsquare_regions.rs`) feeds
the SOLVED coordinates straight into `compute_regions` and proves the
arrangement is already correct: the X-in-square yields **6 simple regions**
(4 inner triangles + 2 frame pieces), total area exactly 0.0025, **zero holes,
zero repeated vertices, zero pinched provenance**. `regions.rs` runs through
`i_overlay`, a true planar arrangement that splits at every crossing — the
pinched `extract_profiles` loops are never used by the single-region extrude
path (`rebuild.rs:446` builds the face from `region.outer`, not `solved_profiles`).

The actual defect was upstream, in the UI:

1. **`app/src/lib/engine/store.svelte.js` `computeAllSketchRegions`** built the
   `solved_positions` map sent to the `ComputeRegions` query from raw `e.x/e.y`
   instead of `sketch.solved_positions`. Fed raw coords (where the "inner"
   square is actually larger than the outer and offset), the arrangement is
   garbage — and the stored extrude `region.outer` baked from it had a hole and
   triggered `ProfileRepeatedVertex { loop_index: 1 }`.
2. **`app/src/lib/sketch/InactiveSketchRenderer.svelte`** likewise read raw
   `entity.x/y` for the finished-sketch wireframe (a stale comment wrongly
   claimed `solved_positions` "is not serialized from Rust" — it is, via
   `skip_serializing_if = "HashMap::is_empty"`). The active-sketch renderer
   already used live solver output, which is why edit mode looked correct and
   finish looked offset.

Both are A2.1 / A5.2 violations: the UI must derive geometry from the engine's
authoritative solved output, never pick an alternate coordinate source.

## The fix

- Both UI sites now read `sketch.solved_positions`, falling back to raw `x/y`
  only for a point with no solved entry yet (freshly drawn, pre-solve). Gear
  expansions keep their own deterministic coords.
- The `ProfileRepeatedVertex` rejection in the kernel is **correct** and stays
  — a pinched loop is not extrudable; the defect was upstream.
- No tolerance widening, no special-case branch, no construction-flag
  workaround (the rejected workaround stays rejected: real interior lines
  subdivide the face into selectable triangles, which `compute_regions` already
  does).

## Tests

- `crates/waffle-types/tests/box_xinsquare_regions.rs` — locks the X-in-square
  arrangement correctness (regression guard for `compute_regions`).
- `app/tests/gui/region-uses-solved-positions.spec.js` — loads a fixture whose
  raw coords differ sharply from `solved_positions` and asserts regions reflect
  the SOLVED geometry. Red on the raw-coords path (total area 0.00119), green
  with the fix (0.0025).

## Stale data note

box.waffle's stored extrude regions were baked from raw coords at selection
time; re-selecting a region after this fix recomputes them correctly. The fix
corrects the source, not pre-baked feature params.
