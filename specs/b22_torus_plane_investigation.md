# B22: Torus-Plane Boolean Investigation

## Problem
RB1 (torus ∪ box) and RB6 (box ∪ torus) fail with non-manifold open edges.
Test: `cargo test -p test-harness --test revolve_boolean -- rb1_revolve_union_with_box`

## Geometry
- **Torus**: Rectangle profile revolved 360° around Y-axis with 3 angular sections (0°-120°, 120°-240°, 240°-360°). 12 faces = 4 types × 3 sectors. Face types per sector: inner_cyl, bottom_disc, outer_cyl, top_disc.
- **Box**: 10×1×10 at x∈[2,7], y∈[-3,-2], z∈[-5,5] (approx). 6 faces.
- **Torus face numbering** (confirmed by AABB analysis):
  - Faces 8-11 = sector 0 (θ=0° to 120°): face 8=inner_cyl, 9=bottom_disc, 10=outer_cyl, 11=top_disc
  - Faces 0-3, 4-7 = sectors 1 and 2
- **Coplanar faces**: Torus bottom disc (y=-3) is coplanar with box bottom face (y=-3). Torus top disc (y=-2) is coplanar with box top face (y=-2).

## IC Landscape (22 ICs total)

### ICs on box face 0 (z=0 face, shell1 face 0) — 10 ICs
- **ICs 0,1,5,6**: Form a closed rectangle (2,-2,0)→(2,-3,0)→(7,-3,0)→(7,-2,0). These are boundary-coincident on torus faces 0-3. They are the intersection of the box's z=0 face with the torus boundary.
- **ICs 10,16**: On torus faces 8,10 (inner/outer cylinders). Boundary-coincident on torus side, cross-interior on box side.
- **ICs 12,13,18,19**: On torus disc faces 9,11. These have **y-coordinate errors of ±0.20** (e.g., y=-3.20 instead of y=-3.00) and are **filtered out by AABB checks**. They represent spurious/noisy intersections between the disc (zero y-thickness AABB) and the box face.

### ICs on box face 1 (lateral face, shell1 face 1) — 12 ICs
- **ICs 2,3,4**: On torus face 1 (sector 1). Cross-interior on both shells.
- **ICs 7,8,9**: On torus face 3 (sector 1). Cross-interior on both shells.
- **IC 11**: faces=(8,1), gv0=(0,-3,2) gv1=(0,-2,2). **Creates biangle on shell1 face 1** (box lateral), succeeds with both_on on shell0 face 8 (torus inner cyl). Status on shell1: `And`.
- **IC 17**: faces=(10,1), gv0=(0,-3,7) gv1=(0,-2,7). **Creates biangle on shell1 face 1** (box lateral), succeeds with both_on on shell0 face 10 (torus outer cyl). Status on shell1: `Or`.
- **ICs 14,15,20,21**: On torus disc faces 9,11. Same y-error issue as above — filtered by AABB.

### ICs on other box faces (2-5)
- **NONE**. The box bottom (y=-3) and top (y=-2) faces are coplanar with torus discs. No ICs are generated between coplanar faces by the SSI algorithm.

## What B22 Does

### 1. Chain Detection (working)
Groups ICs by (shell, face_index). On shell1 face 0, finds that ICs 0,1,5,6 form a closed 4-IC chain (rectangle). Injects this chain as a 4-edge inner wire on shell1 face 0 with `Or` status via `add_independent_loop`.

### 2. B22_dual Biangle Handling (partially working)
Detects that ICs 11 and 17 create biangles on shell1 face 1 (box lateral). These share no vertices but their endpoints form a rectangle:
- IC 11: A=(0,-3,2), B=(0,-2,2)
- IC 17: C=(0,-3,7), D=(0,-2,7)

Creates two connecting line edges:
- **edge1**: B→D = (0,-2,2)→(0,-2,7) — added to torus top disc face 11
- **edge2**: C→A = (0,-3,7)→(0,-3,2) — added to torus bottom disc face 9

Also creates a 4-edge inner wire [IC11_edge, edge1, IC17_edge.inverse(), edge2] and injects it on shell1 face 1 via `add_independent_loop`.

### 3. Face Finding (working)
Uses pre-computed AABBs (`aabbs0`/`aabbs1`) to find the correct torus disc face for each connecting edge. Verifies with `search_parameter`. Uses global `add_polygon_vertex` to splice vertices into the disc boundary, propagating arc splits to adjacent cylinder faces.

### 4. Edge Addition
Calls `add_edge(conn_edge, b1.status)` on the target disc face. **b1.status = And** (the biangle status from the box face).

## Results

### Pre-B22 (baseline)
- Attempt #1 (direct): 14 open edges
- Attempt #2+ (perturbation): 8 open edges at y=-2 and y=-3

### With B22
- Attempt #1 (direct): 10 open edges (4 fewer)
- Attempt #2+ (perturbation): 6 open edges, ALL at y=-3

### Open Edges (attempt #2, with B22)
```
open[0]: (0.0001,-3.0000,2.0000)->(2.0000,-3.0000,0.0000) — inner arc θ=90°→0°
open[1]: (-1.0000,-3.0000,1.7321)->(0.0001,-3.0000,2.0000) — inner arc θ=120°→90°
open[2]: (-1.0000,-3.0000,1.7321)->(-3.5000,-3.0000,6.0622) — sector boundary θ=120°
open[3]: (-3.5000,-3.0000,6.0622)->(0.0001,-3.0000,7.0000) — outer arc θ=120°→90°
open[4]: (0.0001,-3.0000,7.0000)->(7.0000,-3.0000,0.0000) — outer arc θ=90°→0°
open[5]: (2.0000,-3.0000,0.0000)->(7.0000,-3.0000,0.0000) — sector boundary θ=0°
```
These form the **complete boundary** of torus bottom disc sector 0. All refs=1.

### Without B22_dual (chain injection only)
- Attempt #2: 8 open edges (3 at y=-3 in θ=90°→120° region + 2 at y=-2 + 3 others)

So B22_dual connecting edges reduce edges by 2 but add 2 new ones. Net: same count but different distribution.

## Root Cause Analysis

### The Status Bug (discovered this session)
`b1.status = And` (from IC 11's status on the box lateral face). This status means "inside the torus" on the box face. When applied to the connecting edge on the torus disc:

- The connecting edge divides the disc into θ=0°→90° (near box) and θ=90°→120° (away from box)
- With `And` status, the **left side** of the edge (θ>90°, outside box) is classified as And → **discard**
- This is WRONG for union: the outside-box part should be **kept** (Or)
- Result: the outside-box disc fragment is discarded, the inside-box fragment is kept
- The kept fragment's boundary edges at y=-3 don't match the box bottom face → open edges

### The Coplanar Face Gap
Even with correct status, the y=-3 problem has a deeper cause:
1. Torus disc (y=-3) and box bottom (y=-3) are **coplanar**
2. No valid ICs exist between them (SSI produces noisy ICs with y-errors, filtered by AABB)
3. Neither face is properly divided at their mutual intersection boundary
4. The disc MUST be divided into "inside box" and "outside box" regions, but the only division comes from the B22_dual connecting edge at θ=90°, which doesn't align with the actual box boundary on the disc

### Why y=-2 Works (in perturbation attempts)
- Face 11 (top disc, y=-2) also gets `And` status (wrong)
- But perturbation shifts the box slightly, breaking the y=-2 coplanarity
- With broken coplanarity, the SSI generates valid ICs for face 11
- These ICs provide proper face division, overriding the wrong B22_dual status

### Why y=-3 Doesn't Work
- Perturbation also shifts the box at y=-3, but the shifted box bottom face still doesn't generate valid ICs with the disc (the y-error in SSI-generated ICs exceeds the AABB margin even after perturbation)
- Or: the perturbation direction doesn't break the y=-3 coplanarity as effectively

## Proposed Fix: Status Flip

Use `b1.status.not()` instead of `b1.status` for the connecting edge's `add_edge` call on the OTHER shell's disc face. This flips And→Or, correctly classifying the outside-box fragment as "keep".

**Risk**: This might break face 11 (y=-2) in the non-perturbation case. But face 11 already doesn't work in attempt #1 (10 open edges include y=-2 edges). Perturbation rescues face 11 regardless of status.

## Deeper Fix Needed (Future)
The status flip is a band-aid. The real fix requires one of:
1. **D1 coplanar face handling**: Properly detect and divide coplanar face overlaps
2. **AABB margin for zero-thickness faces**: Allow larger margin in dimensions where face has zero AABB extent
3. **SSI improvement**: Generate ICs with correct y-coordinates for nearly-coplanar intersections
4. **Assembly-level coplanar merge**: During v2 assembly, detect coplanar open edge loops and pair them with opposing shell faces

## Key Code Locations

- **B22 code**: `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` lines ~3671-4399
- **OneSidedBiangle struct**: line 3671
- **Chain detection**: lines ~3570-3600 (try_build_closed_chain)
- **B22_dual connecting edges**: lines ~4200-4334
- **add_edge call with status**: line 4319: `other_store[fi].add_edge(conn_edge.clone(), b1.status)`
- **Inner wire injection**: lines 4341-4389
- **divide_face**: `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs` line ~925
- **add_polygon_vertex (global)**: loops_store/mod.rs line 1051
- **add_polygon_vertex_local**: loops_store/mod.rs line 1115

## Test Commands
```bash
# RB1 test
cargo test -p test-harness --test revolve_boolean -- rb1_revolve_union_with_box --nocapture

# Filter useful output
cargo test ... 2>&1 | grep -E 'B22|divide_face|v2_assembly|open\[|attempt|FAIL|PASS'

# Full truck-shapeops regression
cargo test -p truck-shapeops

# Full test-harness
cargo test -p test-harness
```

## File: loops_store/mod.rs Change Summary
1. Added `try_build_closed_chain` helper (~line 337)
2. Added pre-scan grouping ICs by face, chain detection (~line 3074)
3. Added `OneSidedBiangle` tracking struct (line 3671)
4. Added biangle tracking in IC processing loop (line 4038-4052)
5. Added biangle-pair connecting edge construction (lines 4200-4235)
6. Added AABB-based face finding for connecting edges (lines 4245-4334)
7. Added inner wire injection on biangle face (lines 4341-4389)
