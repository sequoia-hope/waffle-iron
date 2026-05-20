# Y60 Boundary Probe Results — `collect_loop_boundary` IS NOT the bug

## Status: data collected; bug localized one layer downstream

## What Y60 captured

Env-gated probe (`Y60_BOUNDARY_PROBE=1`) in `tessellate_solid_bounded` dumps per-planar-face outer-loop vertex indices + 3D positions from `collect_loop_boundary` output. Default-off byte-identical. F0020 spotlight metrics unchanged (47 unpaired, 30 degen, 175 tris); kernel 1250/34/42 unchanged.

Capture: 1000 lines across 6 sections (per-`tessellate_solid_bounded` call). Section 2 (19 faces, 48 edges, matching the operand A snapshot in F0020's spotlight oracle) is the load-bearing data.

## Pair #0 (face 4 ↔ face 13, arena edge 12) — boundary polygons ARE reciprocal

**Arena edge 12 disc_verts: `[24, 25]`** (2-vert linear; endpoints `(-0.247187, 0.104006, -0.226984)` and `(0.023082, 0.150732, -0.189332)`).

### Face 4's outer boundary (collect_loop_boundary output):

```
[0] v=24 pos=(-0.247187, 0.104006, -0.226984)
[1] v=28 pos=( 0.023082, 0.150732, -0.189332)
[2] v=32 pos=( 0.023082, 0.150732, -0.069305)
[3] v=34 pos=( 0.023082, 0.150732,  0.040074)
[4] v=36 pos=( 0.023082, 0.150732,  0.146779)
[5] v=40 pos=(-0.078557, 0.133160,  0.132619)
[6] v=42 pos=(-0.274919, 0.099212,  0.105263)
[7] v=46 pos=(-0.274919, 0.099212,  0.051520)
[8] v=48 pos=(-0.274919, 0.099212, -0.210205)
[9] v=27 pos=(-0.274919, 0.099212, -0.230847)
```

### Face 13's outer boundary:

```
[0] v=26 pos=(-0.247187, 0.104006, -0.226984)   ← same pos as face 4's v=24
[1] v=49 pos=(-0.274919, 0.099212, -0.230847)   ← same pos as face 4's v=27
[2] v=47 pos=(-0.274919, 0.099212, -0.210205)   ← same pos as face 4's v=48
[3] v=43 pos=(-0.274919, 0.099212,  0.051520)   ← same pos as face 4's v=46
[4] v=41 pos=(-0.274919, 0.099212,  0.105263)   ← same pos as face 4's v=42
[5] v=37 pos=(-0.078557, 0.133160,  0.132619)   ← same pos as face 4's v=40
[6] v=35 pos=( 0.023082, 0.150732,  0.146779)   ← same pos as face 4's v=36
[7] v=33 pos=( 0.023082, 0.150732,  0.040074)   ← same pos as face 4's v=34
[8] v=29 pos=( 0.023082, 0.150732, -0.069305)   ← same pos as face 4's v=32
[9] v=25 pos=( 0.023082, 0.150732, -0.189332)   ← same pos as face 4's v=28
```

**Face 13's position sequence is face 4's REVERSE.** Both faces walk the same 10-vertex cycle, in OPPOSITE polygon directions. This is correct 2-manifold sharing — face A walks CCW around its interior; face B walks CCW around its own interior, which is the OPPOSITE direction around the shared region.

**Crucially: face 4 and face 13 use DIFFERENT vertex indices for byte-identical positions.** Face 4 uses indices [24, 27, 28, 32, 34, 36, 40, 42, 46, 48]; face 13 uses [25, 26, 29, 33, 35, 37, 41, 43, 47, 49]. The positions are SAME — the indices differ because each `tessellate_planar_face_bounded` call appends vertices to the shared pool without dedup at that stage.

## Reciprocity check (position-based, matching oracle semantics)

For each consecutive pair in face 4's polygon walk, the reversed pair MUST exist in face 13's polygon walk:

| Face 4 directed edge | Face 13 reciprocal expected | Found in face 13? |
|---|---|---|
| pos(24)→pos(28) | pos(28)→pos(24) | YES: pos(25)→pos(26) at face 13's [9]→[0] (cyclically closing) |
| pos(28)→pos(32) | pos(32)→pos(28) | YES: pos(29)→pos(25) at face 13's [8]→[9] |
| pos(32)→pos(34) | pos(34)→pos(32) | YES: pos(33)→pos(29) at face 13's [7]→[8] |
| pos(34)→pos(36) | pos(36)→pos(34) | YES: pos(35)→pos(33) at face 13's [6]→[7] |
| ... (all 10 reciprocate) | | YES |

**`collect_loop_boundary` produces POLYGON-LEVEL RECIPROCITY for face 4 ↔ face 13.** No bug here.

## Implications

The oracle (`BijectiveFacePairOracle`) reads from `rendermesh.indices` AFTER `tessellate_planar_face_bounded` (CDT) has been called. The boundary polygon Y60 captures is the INPUT to CDT. So:

- **Input to CDT (collect_loop_boundary output)**: reciprocal ✓
- **Output of CDT and beyond (rendermesh.indices)**: non-reciprocal (per oracle)

Therefore the bug is **between collect_loop_boundary and the oracle's read** — specifically in one of:

1. **CDT itself** (`tessellate_planar_face_bounded`'s call to `cdt::cdt_triangulate_2d_with_loops`): emits tris with wrong winding for some configuration
2. **Vertex appending** in `tessellate_planar_face_bounded`: appends positions to the shared `vertices` array; each face contributes its own positions; oracle then reads `rendermesh.indices` and groups by `face_ranges`. If the index mapping somehow doesn't preserve the polygon walk direction, the rendermesh's per-face directed edges may differ from the polygon's
3. **face_boundary_directed_edges** (the oracle's own extraction code at `bijective.rs:240-283`): how it computes "boundary directed edges" from per-face tris. If it's identifying the wrong set as boundary, the verdict misattributes

**Most likely**: option 1 or 2. The oracle's logic (option 3) has been stable for many cycles and other oracles pass on F0020. The CDT-output / vertex-append path is where the polygon-walking direction can be lost.

## Updated diagnosis chain

1. **Surface symptom** (chain-builder fix shipped, commit `26d9094`)
2. **Tier A** (Y55, commit `83771d3`): localized to face pairs
3. **Path γ** (Y57, commit `ed656d6`): refuted twin-pairing
4. **Y58/Y59** (commits `5f3109d`/`a2259eb`): arena correct; bug at M4a/c
5. **Multi-vert linear off-by-one fix** (Y58, commit `96ec0a5`): correct fix, but F0020 doesn't exercise this path
6. **Y60 (this commit)**: `collect_loop_boundary` output IS reciprocal for face 4 ↔ face 13; **bug is in CDT or vertex appending or oracle's own extraction**

## Next step — Y61 probe

Inspect the rendermesh output for face 4 and face 13 directly:

1. After `tessellate_planar_face_bounded` returns, dump the new tris appended to `rendermesh.indices` for each face (with their 3D positions, not just indices)
2. For face 4 and face 13, enumerate their tris' directed edges (in face-local indexing); check whether the boundary directed edges (those appearing once) walk the polygon CCW
3. Compare against the polygon walk direction from Y60

If face 4's tris walk the polygon CCW AND face 13's tris ALSO walk CCW in their own frame: the rendermesh has both faces producing boundary edges in OPPOSITE directions → bijection holds in render → oracle bug.

If face 4's tris walk CCW BUT face 13's tris walk in the SAME direction as face 4 (post-CDT): CDT is flipping face 13's tris, producing same-direction boundary edges. **CDT input/output direction mismatch is the bug**.

Y61 sketch: ~30-50 LOC in `tessellate_solid_bounded` after the `tessellate_planar_face_bounded` call. Dump face's tris with positions, env-gated.

## DoD checklist (Infrastructure / Tooling Change per DoD §6)

- [x] Default-off byte parity verified (F0020 spotlight unchanged)
- [x] Kernel `cargo test -p kernel --lib` baseline preserved: 1250/34/42
- [x] Y60 dump produced for F0020 (1000 lines across 6 sections; section 2 = oracle's snapshot)
- [x] Phase 3 analysis classifies face 4 ↔ face 13: **polygon-level reciprocal** ✓
- [x] Memo documents findings
- [x] Decision gate: next cycle = Y61 (rendermesh tri-direction inspection)
- [N/A] WASM rebuild (probe is default-off; production unchanged)

## Discipline note

This is exactly what "follow the oracle" produces. Y60 didn't test a hypothesis — it observed the data at one layer (collect_loop_boundary output) and found that layer is correct. The bug must be in the next layer forward (CDT or vertex appending or oracle's read).

No mechanism inference. The next cycle's plan cites Y60's specific data: "face 4 and face 13 have reciprocal polygon walks at positions matching the oracle's unmatched edges; therefore the bug is in CDT or downstream."

## Verification

```bash
cd /home/claude/workspace

# Default-off byte parity
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture 2>&1 | grep "Detail:"
# expect: 47 unpaired, 30 degen, 175 tris (unchanged)

# Y60 measurement
Y60_BOUNDARY_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020_oracles --ignored --nocapture 2>&1 \
  | grep "y60-bnd" > /tmp/y60_f0020.log
wc -l /tmp/y60_f0020.log  # expect: ~1000 lines

# Kernel regression
cargo test -p kernel --lib 2>&1 | tail -3  # expect 1250/34
```
