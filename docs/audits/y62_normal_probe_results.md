## Status: bug is UPSTREAM of `tessellate_planar_face_bounded`

## What Y62 captured

Env-gated probe (`Y62_NORMAL_PROBE=1`) in `tessellate_solid_bounded` dumps per-call
`(kid, face_idx, stored_normal, first_3_boundary_positions)` for each planar-face
tessellation. Pairs with Y60's polygon dump (positions, no normals) to determine
whether F0020's reciprocal-walk pair (face 4 ↔ face 13) has SAME or OPPOSITE
stored_normals. Default-off byte-identical.

Capture: 816 lines on F0020.

## Pair #0 (face 4 ↔ face 13) — both have IDENTICAL stored_normal

```
kid=59 face_idx=4  normal=(-0.170359, 0.985382, -0.000000) n_verts=10
   [0] pos=(-0.247187, 0.104006, -0.226984)   ← same anchor as face 13's [0]
kid=68 face_idx=13 normal=(-0.170359, 0.985382, -0.000000) n_verts=10
   [0] pos=(-0.247187, 0.104006, -0.226984)
```

Both faces appear 4× across the probe run (different Boolean stages). All
8 invocations report the same stored_normal `(-0.170359, 0.985382, -0.000000)`.

**The RED test's premise is correct**: F0020 has two coplanar faces with the
SAME stored_normal walking the shared boundary in OPPOSITE arena directions.

## What this proves

The RED test
`tessellate_planar_face_bounded_coplanar_same_normal_reciprocates` at
`mod.rs:6896` accurately reproduces F0020 Pair #0's actual configuration.

But **the bug is NOT in `tessellate_planar_face_bounded`**.

The function emits triangles CCW relative to `stored_normal` (its load-bearing
contract). This contract is REQUIRED by:
- B-Rep face geometry assembly (`face_geometry`'s stored_normal must align
  with the loop's Newell normal — checked by
  `boolean::yang_integration::tests::test_yang_subtract_face_geometry_complete`)
- Volume calculation via divergence theorem (`waffle_kernel::tests::
  k1_cyl_minus_enclosed_box_volume` exercises this)

A naïve "post-flip CDT tris when polygon walks CW relative to stored_normal"
fix (candidate shape #1 from y61 memo) was attempted in this cycle and
caused 2 regressions in those tests. Reverted.

**The actual bug is upstream**: Yang coplanar preprocessing produced two
same-normal coplanar faces sharing a boundary edge but walking it in
opposite arena directions. In a well-formed 2-manifold B-Rep, two faces
sharing an edge MUST have outward normals that differ at the shared edge
(typically opposite signs). Same-normal sharing implies either:
- The faces are not a proper 2-manifold pair (e.g., non-manifold edge from
  Yang's intentional overlap region encoding)
- The stored_normal field is wrong for one of the two faces
- The face splitting in `coplanar_preprocess` placed both faces on the same
  side of the shared boundary

## Updated diagnosis chain (Y58 → Y62)

| Probe | Cycle | What was ruled in/out |
|---|---|---|
| Y58 | per-pair detail | Oracle reports byte-identical same-direction unmatched edges |
| Y59 | HE-to-loop | Arena correct (HE_primary/secondary in different faces, opposite-twin'd) |
| Y58 fix | multi-vert linear | Real fix but F0020 doesn't exercise this path |
| Y60 | collect_loop_boundary | Polygon-level reciprocity HOLDS at the boundary-collection layer |
| Y61 | rendermesh tris | Same-direction at the rendermesh layer — bug appeared bounded to `tessellate_planar_face_bounded` |
| Y61 RED test | d4b2d4b | Minimal 4-vert reproduction; assumed same-normal scenario |
| **Y62 (this cycle)** | **stored_normal capture** | **Same-normal premise CONFIRMED. But fix at `tessellate_planar_face_bounded` violates downstream contract → bug is UPSTREAM (Yang coplanar preprocessing)** |

## Methodology lesson

Y60/Y61 narrowed the bug to a function via probe inference (each probe ruled
out one layer until only `tessellate_planar_face_bounded` remained). The
inference was *correct in the sense that the function's output is the
proximate site of the bijectivity violation*. But the **function is doing
what it's supposed to do given its inputs**. The localization missed one
step: verify whether the inputs to the function are themselves well-formed.

This is the corollary to "Verify Fix Anchor Before Coding": the anchor
function may exhibit the symptom, but the upstream that *produced its
inputs* may be the real defect. When the proposed fix would violate the
function's load-bearing contract, that's a signal the localization
stopped one layer too early.

## Next cycle anchor candidates

1. **`boolean::coplanar_preprocess`** — When producing the post-split face
   list, audit whether two same-normal coplanar faces ever end up with
   reciprocal boundary walks. If yes, the producer must either re-orient
   one polygon or assign different stored_normals.
2. **Yang §4.5.5 overlap region encoding** — The non-manifold edge bounded
   by two same-normal faces may be Yang's intentional overlap encoding.
   If so, the Stage1 BijectiveFacePairOracle's contract needs adjustment
   for overlap-region non-manifold edges (vs. regular 2-manifold edges).
3. **Stored_normal assignment** — Audit where each face's stored_normal is
   set; verify it's derived from the polygon walk direction, not copied
   from some upstream parent that may have had different orientation.

## DoD checklist (Infrastructure / Tooling Change per DoD §6)

- [x] Default-off byte parity verified (Y62 probe is env-gated)
- [x] Kernel `cargo test -p kernel --lib` baseline preserved: 1250/34/43
- [x] Y62 dump produced for F0020 (816 lines)
- [x] Phase 3 analysis confirms face_4 and face_13 SAME stored_normal
- [x] Memo documents findings + next-cycle anchor candidates
- [N/A] WASM rebuild (probe is default-off; production unchanged)

## Verification

```bash
cd /home/claude/workspace

# Y62 measurement
Y62_NORMAL_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 \
  | grep "y62-nrm" > /tmp/y62_f0020.log
wc -l /tmp/y62_f0020.log  # expect: ~816 lines

# Find F0020 face_4 / face_13 stored_normals
grep -B1 "(-0.247187,0.104006,-0.226984)" /tmp/y62_f0020.log
# expect: face_4 and face_13 BOTH show normal=(-0.170359, 0.985382, -0.000000)

# Kernel regression
cargo test -p kernel --lib 2>&1 | tail -3  # expect 1250/34
```
