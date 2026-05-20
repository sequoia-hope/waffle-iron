# Y58 Phase 2 — F0020 Stage 1 Bijective Diagnosis: SAME-DIRECTION shared-edge tessellation

## Per-pair table (from Phase 1 dump, commit pending)

| Pair # | face_a | face_b | edge | unmatched_a | unmatched_b | sample A ≡ sample B? |
|---|---|---|---|---|---|---|
| 0 | FaceIdx(4)  | FaceIdx(13) | EdgeIdx(12) | 9 | 9 | **byte-identical** |
| 1 | FaceIdx(7)  | FaceIdx(9)  | EdgeIdx(26) | 3 | 3 | **byte-identical** |
| 2 | FaceIdx(7)  | FaceIdx(10) | EdgeIdx(27) | 2 | 2 | **byte-identical** |
| 3 | FaceIdx(7)  | FaceIdx(11) | EdgeIdx(35) | 2 | 2 | **byte-identical** |
| 4 | FaceIdx(7)  | FaceIdx(12) | EdgeIdx(31) | 3 | 3 | **byte-identical** |

Total unmatched directed edges: 9+3+2+2+3 = **19 unmatched on each side**.

## The smoking gun

In all 5 pairs, `sample_unmatched_a` and `sample_unmatched_b` are byte-identical Vec<([f64;3], [f64;3])>. Each face emits the SAME directed edge in the SAME direction. Example from Pair #0:

```
sample unmatched A: (0.023082,0.150732,-0.189332) → (-0.247187,0.104006,-0.226984)
sample unmatched B: (0.023082,0.150732,-0.189332) → (-0.247187,0.104006,-0.226984)
```

Face A walks `p → q`; face B also walks `p → q`. **Neither walks `q → p`.** No reciprocal pair exists. This is a Yang §4.1.1 bijectivity violation at the tessellation OUTPUT level.

## What this rules out

**M1 (coplanar injection asymmetry)**: would produce asymmetric unmatched counts (face A has N extra, face B has M different extras). The byte-identical signature here is wrong shape for M1.

**M2 (quantization asymmetry)**: would produce edges that differ by ~1e-7 due to f32 round-trip. The byte-identical positions here are nm-precision matches; quantization doesn't account for SAME-direction emission.

**M3 (NMM-at-edge-0)**: 36 NMM HEs at edge=0 were observed in F0020 by Y55 but contribute 0% to the cascade collisions per Y55's cross-reference. NMM HEs reference edge 0's discretization regardless of HE identity; that would produce *wrong-position* same-direction emission (both faces walk a chunk of EdgeIdx(0)'s positions, not the actual shared edge's positions). The sample data here has correct edge geometry — Pair #0's positions match edge 12's actual location.

## What this is consistent with

**M4 (NEW finding — twin HEs walk same direction in their respective loops)**: arena edge 12 has HE_primary (in face 4's outer loop) and HE_secondary = HE_primary.twin (in face 13's outer loop). For 2-manifold sharing:
- Face 4's loop walks `(p → q)` via HE_primary
- Face 13's loop walks `(q → p)` via HE_secondary
- collect_loop_boundary should reverse discretization for secondary HEs

But the tessellation output shows BOTH faces walking `(p → q)`. So either:
- (M4a) `collect_loop_boundary` is NOT reversing discretization for one of the HEs (bug in collect_loop_boundary)
- (M4b) Face 4 and face 13 both contain the SAME HE (HE_primary) in their loops — i.e., the arena builder has assigned HE_primary to TWO loops, and HE_secondary is dangling in neither (deep arena defect)
- (M4c) Some interaction between primary/secondary determination in `collect_loop_boundary` and the actual loop walks

This is a NEW mechanism not in the original M1-M3 candidate list. It requires another small probe (Y59) to distinguish M4a vs M4b vs M4c.

## Structural pattern

**Face 7 is in 4 of 5 pairs.** Face 7's outer loop has multiple shared edges (with face 9, 10, 11, 12), and ALL of them exhibit the same defect. This means whatever's wrong with face 7 is systematic to face 7 itself, not to its individual neighbors. Likely: face 7's outer loop is constructed such that ALL its boundary HEs are primary (or all secondary), and its neighbors' loops contain the OPPOSITE-sided HEs but `collect_loop_boundary` is producing same-direction output anyway.

**Edge IDs are concrete**: 12, 26, 27, 31, 35. The fix will need to trace why these specific arena edges produce same-direction tessellation in their incident faces' loops.

## Cohort context (banked for Phase 4 verification)

If the M4 mechanism is what's biting F0020, it likely affects most of the 24 Stage1Bijective cases in the corpus. Cohort sweep in Phase 4 will quantify.

## Next step — Y59 probe BEFORE writing the fix

Per the canary-first discipline + P10: do NOT write a fix yet. Add Y59 probe to surface, for each of the 5 pairs:

1. For arena edge `e_id`, identify HE_primary and HE_secondary (e.g., `arena.edges[e_id].half_edge` and its twin)
2. For each loop in face_a's outer_loop + inner_loops: enumerate HEs and check if HE_primary or HE_secondary appears
3. Same for face_b's loops
4. Determine: is HE_primary in both loops (M4b)? Is HE_secondary in both loops (M4b variant)? Are they in separate loops as expected (M4a or M4c)?
5. If M4a/M4c: dump the actual discretization fed to `collect_loop_boundary` for the affected HE and verify whether `is_primary` was computed correctly

Y59 is ~30-50 LOC of probe instrumentation. Default-off byte-identical. Should fire on the 5 known pairs in F0020.

Then Phase 3 (fix) is scoped per Y59 finding.

## What Phase 2 did NOT determine

- The exact code path that produces same-direction tessellation (need Y59)
- Whether the bug is in arena construction (loop assignment) or in tessellation (boundary collection)
- Whether the 5 pairs share a common HE identity bug or have 5 independent failures

## Y59 update — arena is correct; bug is downstream of arena (M4a/c)

Ran the Y59 probe (env-gated `Y59_BIJECTIVE_PROBE=1`) inside `check_brep_mode` to dump HE-to-loop-to-face assignment for each non-bijective pair's canonical edge. Result for all 5 pairs in F0020 operand A:

```
[y59-bij] pair (FaceIdx(4),FaceIdx(13))  edge=12 HE_p=16 p_loop=4  p_face=4  HE_s=79 s_loop_face=(13, 13) canonical_walk=v8→v9
[y59-bij] pair (FaceIdx(7),FaceIdx(9))   edge=26 HE_p=34 p_loop=7  p_face=7  HE_s=51 s_loop_face=(9, 9)   canonical_walk=v18→v19
[y59-bij] pair (FaceIdx(7),FaceIdx(10))  edge=27 HE_p=55 p_loop=10 p_face=10 HE_s=43 s_loop_face=(7, 7)   canonical_walk=v18→v27
[y59-bij] pair (FaceIdx(7),FaceIdx(11))  edge=35 HE_p=40 p_loop=7  p_face=7  HE_s=62 s_loop_face=(11, 11) canonical_walk=v24→v25
[y59-bij] pair (FaceIdx(7),FaceIdx(12))  edge=31 HE_p=37 p_loop=7  p_face=7  HE_s=67 s_loop_face=(12, 12) canonical_walk=v21→v22
```

Classification for all 5: **M4a/c: arena correct — bug in tessellation**.

- HE_primary and HE_secondary are in DIFFERENT loops (different faces), as 2-manifold sharing requires
- The arena's twin pointers are well-formed
- The loop assignments match what the oracle expects

So M4b (arena defect) is ruled out. The bug is in the path between arena and rendermesh output — specifically `collect_loop_boundary` or `tessellate_planar_face_bounded` or downstream.

## Detailed look at the unmatched edges

Pair #0 (face 4 ↔ face 13, edge 12) sample unmatched edges:

```
(0.023082, 0.150732, -0.189332) → (-0.247187, 0.104006, -0.226984)   ← off-plane transition
(0.023082, 0.150732, -0.069305) → (0.023082, 0.150732, -0.189332)
(0.023082, 0.150732,  0.040074) → (0.023082, 0.150732, -0.069305)
(0.023082, 0.150732,  0.146779) → (0.023082, 0.150732,  0.040074)
```

The middle 3 edges chain together: walk along z from 0.147 → 0.040 → -0.069 → -0.189 at constant y=0.150732 (a multi-vert discretization of one arena edge). The first edge transitions off the y=0.151 plane to (-0.247, 0.104, -0.227) — a SEAM transition between two HEs at a corner.

Both face 4 and face 13 walk these 4 edges in the SAME direction. For 2-manifold sharing, exactly one should be reversed.

## Candidate bug location — `collect_loop_boundary` multi-vert linear secondary

Inspecting `crates/kernel/src/tessellation/mod.rs:3097-3102`:

```rust
} else if is_primary {
    boundary.extend_from_slice(&verts[..verts.len() - 1]);
} else {
    for &v in verts.iter().rev().skip(1) {
        boundary.push(v);
    }
}
```

For a multi-vert linear edge with `verts = [v0, v1, ..., vn-1]`:

- **Primary**: pushes `[v0, v1, ..., vn-2]` (origin + intermediates, NOT destination). ✓ Convention: each HE pushes its origin + intermediates; next HE's origin = this HE's destination = next push.
- **Secondary** (current): `verts.iter().rev().skip(1)` = `[vn-2, vn-3, ..., v0]` — pushes intermediates + DESTINATION (v0). MISSING origin (vn-1).

Under the convention each HE pushes "origin + intermediates", secondary should push `[vn-1, vn-2, ..., v1]` (origin + intermediates in REVERSE traversal order, not destination). The current code skips origin and includes destination. Off-by-one in opposite direction.

**Proposed fix**: change `verts.iter().rev().skip(1)` to `verts[1..].iter().rev()` (or equivalently `verts.iter().rev().take(verts.len() - 1)`).

## Caveat — proposed fix needs verification before commit

The Y59 probe localized the M4 sub-variant (arena correct) but does NOT directly prove the multi-vert-linear-secondary fix is the right shape. The unmatched-edge data shows BOTH chained edges (likely from multi-vert linear) AND off-plane transitions (likely from HE seams). The off-plane transitions might be a SEPARATE bug — they're not part of any single HE's discretization.

Before scoping the fix:
- Verify the proposed fix produces correct rendermesh output for at least Pair #0
- Add a unit test in `collect_loop_boundary` exercising multi-vert linear secondary
- Check whether the off-plane seam transition is symptomatic of a DIFFERENT bug (potentially affecting the 9 vs 9 count vs the 3+1 explanation)

If the multi-vert fix only addresses ~6 of 9 unmatched edges in Pair #0, the residual indicates a second bug. Per the discipline: don't merge a fix that only partially explains the data.

## Updated next step — Phase 3 fix with empirical verification

The Y59 finding plus code reading produces a candidate fix shape. Per FIP §8 bug-fix variant:

1. **RED phase**: add a unit test in `tessellation::mod` that constructs two minimal faces sharing a multi-vert linear edge with a secondary HE on one side, asserts the boundary polygons reciprocate. This should FAIL with current code.
2. **Fix**: change line 3097-3102 secondary case from `iter().rev().skip(1)` to `verts[1..].iter().rev()` (or equivalent).
3. **GREEN phase**: unit test passes.
4. **Integration verification**: `spotlight_f0020_oracles` should show fewer (or zero) Stage 1 non-bijective pairs. If still non-zero, surface the residual and re-plan.

## Discipline note

I did NOT propose a fix in Phase 2 even though one feels close. The 5 prior canary ABORTs all happened because the implementer scoped a fix from "feels close" inference. Y59 (~50 LOC) is the cost to derisk; the alternative (~50-100 LOC fix on a guess) has a 5/6 historical refute rate.

Y59 surfaced the M4a/c classification clearly. The proposed multi-vert-secondary fix is one candidate but requires a Phase 3 RED-phase unit test BEFORE committing. The unit test is the empirical anchor.
