# Y61 Tri Probe Results — Bug Cornered in `tessellate_planar_face_bounded`

## Status: bug location empirically pinned

## What Y61 captured

Env-gated probe (`Y61_TRI_PROBE=1`) in `tessellate_solid_bounded` dumps the rendermesh tris appended by each `tessellate_planar_face_bounded` call: per-tri vertex indices into `rendermesh.vertices` + 3D positions at f64 precision. Default-off byte-identical (F0020 spotlight unchanged; kernel 1250/34/42 unchanged).

Capture: 447 lines across 9 tessellate_solid_bounded sections. Section 4 (19 faces) matches Y60's section 2 — the oracle's snapshot of F0020 operand A.

## Pair #0 (face 4 ↔ face 13) — rendermesh boundary directed edges are SAME direction

**Computed via the exact same logic as `face_boundary_directed_edges`** (`bijective.rs:240-283`): for each tri, enumerate 3 directed edges (i→i+1 cyclically); edges appearing exactly once with no reverse in this face are boundary.

| Face | Tris | Boundary directed edges | Reciprocal with face 13 | Same-direction with face 13 |
|---|---|---|---|---|
| 4 | 9 | 9 | **0** | **9 (100%)** |
| 13 | 9 | 9 | **0** | **9 (100%)** |

Sample SAME-direction edges (face 4 has them; face 13 has the IDENTICAL DIRECTED edges, NOT reverses):

```
((0.023082, 0.150732, -0.189332), (-0.247187, 0.104006, -0.226984))
((0.023082, 0.150732, -0.069305), (0.023082, 0.150732, -0.189332))
((0.023082, 0.150732,  0.040074), (0.023082, 0.150732, -0.069305))
((-0.274919, 0.099212,  0.05152),  (-0.274919, 0.099212, 0.105263))
((-0.274919, 0.099212, -0.210205), (-0.274919, 0.099212,  0.05152))
...
```

## What this proves

**`collect_loop_boundary`'s output (Y60) was reciprocal — but the rendermesh's per-face tris are NOT reciprocal.** Therefore the bug is in the code path between `collect_loop_boundary`'s output and the rendermesh: that's **`tessellate_planar_face_bounded`** (specifically its 2D projection step + CDT call).

Y60 showed face 4's polygon walks `[24, 28, 32, 34, 36, 40, 42, 46, 48, 27]` (CCW around face 4) at positions starting at `(-0.247)` going through `(0.023, 0.151, ...)`.
Y60 showed face 13's polygon walks `[26, 49, 47, 43, 41, 37, 35, 33, 29, 25]` — same positions in REVERSE order, starting at `(-0.247)` going through `(-0.275, 0.099, ...)`.

But the rendermesh tris have boundary edges in the SAME 3D direction for both faces. Therefore `tessellate_planar_face_bounded` is:
1. Receiving face 4's polygon (3D walk: pos0 → pos1 → ... → pos9)
2. Receiving face 13's polygon (3D walk: pos0 → pos9 → ... → pos1) — REVERSED
3. Producing tris for face 4 with boundary edges (pos0→pos1), (pos1→pos2), ...
4. Producing tris for face 13 with boundary edges ALSO (pos0→pos1), (pos1→pos2), ... — instead of the expected (pos9→pos8), (pos8→pos7), ...

The boundary direction is being normalized to a single canonical direction regardless of input polygon walk. This breaks Yang §4.1.1 bijectivity for adjacent face pairs whose loops walk a shared edge in opposite arena directions.

## Likely culprit — `compute_plane_basis` is normal-direction-sign-dependent

`tessellate_planar_face_bounded` (`tessellation/mod.rs:3115+`) projects the 3D boundary polygon to 2D using `compute_plane_basis(stored_normal)`. The `stored_normal` for face 4 and face 13 differ (adjacent faces have different outward normals at a shared edge).

If `compute_plane_basis(N)` and `compute_plane_basis(-N)` produce DIFFERENT (u_axis, v_axis) bases that are NOT just sign-flipped versions of each other (e.g., they might be RELATED but oriented differently), the 2D projection of face 13's REVERSED polygon could end up walking CCW in 2D — the same as face 4's CCW projection.

CDT then triangulates both as CCW 2D polygons, producing tris with consistent CCW winding. Back in 3D, the tris have boundary edges in the SAME direction.

This is the **stored_normal sign-dependence bug**: the 2D projection step loses face-local directional information.

**Alternative hypothesis**: CDT itself flips winding when the input 2D polygon is CW. Some CDT implementations always output CCW tris regardless of input. If `spade`'s `cdt_triangulate_2d_with_loops` does this, that's the bug.

Next cycle's investigation: inspect `compute_plane_basis` and `cdt_triangulate_2d_with_loops` to determine which.

## Full diagnosis chain (Y58 → Y61)

| Probe | Cycle | What was ruled in/out |
|---|---|---|
| Y58 | per-pair detail | Oracle reports byte-identical unmatched edges on both sides |
| Y59 | HE-to-loop | Arena correct (HE_primary/secondary in different faces, opposite-twin'd) |
| Y58 fix | multi-vert linear | Real bug fixed (off-by-one) but F0020 doesn't exercise this path |
| Y60 | collect_loop_boundary | Polygon-level reciprocity HOLDS at the boundary-collection layer |
| **Y61** | **rendermesh tris** | **Same-direction at the rendermesh layer — bug is in `tessellate_planar_face_bounded`** |

The bug is bounded to `tessellation/mod.rs::tessellate_planar_face_bounded` (function at L3115+, ~250 LOC including CDT call). Within that function, the suspect operations are:
- `compute_plane_basis(stored_normal)` projection axis selection
- `cdt::cdt_triangulate_2d_with_loops` winding behavior

## Next step — Y62 or direct test

Two options:

1. **Y62 probe**: instrument `tessellate_planar_face_bounded` to dump per-face the 2D projected polygon BEFORE CDT, and CDT's output tris in 2D. Cross-reference to determine whether the 2D walk for face 13 is CCW or CW. If CW, CDT likely flipped; if CCW, projection lost reciprocity.

2. **Direct test**: write a unit test calling `tessellate_planar_face_bounded` for face 4's polygon and face 13's polygon (with their respective stored_normals from F0020), check that the output tris have reciprocal boundary edges. If RED, the test pinpoints whether projection or CDT is at fault by inspecting intermediate state.

Option 2 is tighter discipline (RED → fix → GREEN). Recommended.

## DoD checklist (Infrastructure / Tooling Change per DoD §6)

- [x] Default-off byte parity verified (F0020 spotlight unchanged)
- [x] Kernel `cargo test -p kernel --lib` baseline preserved: 1250/34/42
- [x] Y61 dump produced for F0020 (447 lines, 9 sections)
- [x] Phase 3 analysis classifies face 4 and face 13: **same-direction at rendermesh layer**
- [x] Memo documents findings
- [x] Decision-gate for next cycle: Y62 or direct unit test in `tessellate_planar_face_bounded`

## Discipline note

Y58→Y61 is 5 consecutive probes that rule out 5 candidate layers via direct observation. No mechanism inference. Each cycle's data definitively eliminates one possibility and surfaces the next.

The bug is now LITERALLY in one function (`tessellate_planar_face_bounded`). Bounded scope. The next cycle either lands the fix or surfaces the specific sub-path (projection vs CDT).
