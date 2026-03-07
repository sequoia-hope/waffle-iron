# B21: Torus-Plane Boolean Face Division Failure

**Status**: Partial fix implemented (try_separate_wire_faces recovery). RB1/RB6 still fail — deeper IC edge identity issue remains.
**Sprint**: 60-61
**Severity**: Medium — RB1/RB6 are not `#[ignore]`, fail in CI
**Related**: D1.6 (boundary-coincident IC skip), D1.7 (all_on_boundary three-way logic)

## Problem

RB1 (torus ∪ box) and RB6 (box ∪ torus) fail with 8 non-manifold edges
(refs=1) after shell assembly. The perturbation cascade exhausts 20 attempts
(~125s) without recovery because the failure is structural, not numerical.

## Geometry

- **Shell0**: 360° revolve torus — 12 faces (inner/outer tube × 6 sectors),
  revolution axis at x=0, center at (4.5, -2.5, 0), major radius 4.5,
  minor radius 0.5
- **Shell1**: Axis-aligned box — 6 planar faces, (0,0,0)→(10,-10,10)
- **Operation**: Union

## Root Cause Analysis

### Symptom: NotSimpleWire on torus faces

Torus faces 0 and 2 receive 4 planar ICs each, forming a rectangular
intersection curve where the torus cross-section meets the box z=0 plane:

```
IC (0,0): (2,-2,0) → (2,-3,0)   f0: w0e0Front → w0e0Back
IC (1,0): (2,-3,0) → (7,-3,0)   f0: w0e0Front → w0e0Back
IC (2,0): (7,-2,0) → (7,-3,0)   f0: w0e2Front → w0e0Front
IC (3,0): (2,-2,0) → (7,-2,0)   f0: w0e2Front → w0e0Front
```

All IC endpoints land on torus boundary edges. `add_edge` splices these into
the boundary wire, but the resulting composite wire is non-simple (self-
intersecting or figure-8), causing `Face::try_new` to fail with `NotSimpleWire`.

### Why face division fails: v-seam topology

The torus face boundary is a single closed wire with 4 edges (top arc, right
seam, bottom arc, left seam). The v-seam edges run along x≈0 (the revolution
axis). When 4 ICs are spliced into this boundary:

1. ICs share endpoints on the same boundary edge (`w0e0`)
2. Splicing creates a wire that visits the same vertex multiple times
3. The wire becomes non-simple (figure-8 at shared splice points)
4. `Face::try_new` rejects it → fallback to `face.clone()` (undivided)

### Cascade effect: edge mismatch

When face 0 falls back to undivided:
- Neighboring faces (8, 10) divide successfully into 2 fragments each
- The divided faces have new IC boundary edges expecting to pair with
  divided fragments of face 0
- But face 0 is undivided — its old boundary edges don't match
- Result: 8 unpaired edges at the v-seam → shell assembly failure

### Why perturbation doesn't help

Each perturbation attempt shifts the box slightly, but the torus v-seam is
always at x≈0. The 4 ICs always form a rectangle on the torus face, always
create a non-simple wire, and always cause the same structural failure.
Perturbation only affects numerical precision, not the wire topology problem.

## B21 Fix: `try_separate_wire_faces` (Partial)

### Implementation (Sprint 61)

Added `try_separate_wire_faces` recovery function in
`vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs`:

1. Iterates over each BoundaryWire in loops
2. For simple wires: filters by `is_closed && !is_biangle_wire && len >= 3`
3. For non-simple wires: applies `split_wire_recursive` to decompose
   figure-8 wires, then filters sub-loops by same criteria
4. Skips wires with spatial extent < `tol * 0.1` (degenerate slivers)
5. Creates `Face::try_new(vec![wire], surface)` for each valid wire
6. Preserves face orientation and wire status
7. Returns `Some(faces)` if ≥ 2 faces created, else `None`

### Injection points

1. **All-Unknown path (line ~851)**: When `Face::try_new` fails in the
   all-Unknown block (D1.7 reset makes IC-affected faces appear all-Unknown).
   Activated on perturbation attempts where wires become non-simple.

2. **divide_one_face_v2 None path (line ~995)**: After `construct_ring_disc_direct`
   fails. Activated when FBG and legacy division both fail.

### Why RB1/RB6 still fail

The B21 recovery successfully splits torus faces into separate wire faces,
but the 8 open edges persist because:

1. **len >= 3 guard**: The torus all-Unknown path has wire[0] (4 edges) and
   wire[1] (2 edges). Wire[1] is a 2-edge closed wire (IC edge + boundary
   segment), which is filtered by `len >= 3`. Relaxing to `len >= 2` causes
   downstream "Two same vertices cannot construct an edge" panics on other
   tests (g2_stacked_boxes_coplanar_face).

2. **IC edge identity between shells**: Even when B21 splits the torus face
   correctly, the 8 open edges remain because IC edges from the torus shell
   don't pair with corresponding IC edges on the box shell. The box face
   uses FBG parametric-space decomposition which may create new edge objects
   with different Arc<Edge> identity.

3. **v-seam edge splitting inconsistency**: When `add_edge` inserts IC
   endpoints on torus boundary edges, the v-seam edges get split into segments.
   But neighboring torus sector faces sharing those v-seam edges may not get
   the same split (depends on whether `add_polygon_vertex` propagated the
   edge cut via `swap_edge_into_wire` to all faces).

### What the B21 recovery DOES help

- Prevents `face.clone()` fallback (undivided face) for faces where
  `Face::try_new` fails due to non-disjoint wires
- Successfully activates for the "v2 None" path (3 wires, all with len >= 3)
- Provides a foundation for future torus-plane boolean fixes
- Zero regressions on all existing test suites

## Debug Evidence

### Open edges (consistent across all 20 attempts)

```
open[0]: (0.0007,-2.0000,2.0000)->(0.0007,-3.0000,2.0000) refs=1  — v-seam
open[1]: (0.0007,-3.0000,7.0000)->(0.0007,-2.0000,7.0000) refs=1  — v-seam
open[2]: (0.0007,-3.0000,7.0000)->(7.0000,-3.0000,0.0000) refs=1  — seam→IC
open[3]: (2.0000,-2.0000,0.0000)->(2.0000,-3.0000,0.0000) refs=1  — IC
open[4]: (7.0000,-2.0000,0.0000)->(7.0000,-3.0000,0.0000) refs=1  — IC
open[5]: (0.0007,-3.0000,2.0000)->(2.0000,-3.0000,0.0000) refs=1  — seam→IC
open[6]: (0.0007,-2.0000,7.0000)->(7.0000,-2.0000,0.0000) refs=1  — seam→IC
open[7]: (0.0007,-2.0000,2.0000)->(2.0000,-2.0000,0.0000) refs=1  — seam→IC
```

### Face division log (with B21)

```
face 0 all-Unknown — rebuilding from loops_store
face 0 try_new FAILED (NotSimpleWire) — trying B21 recovery
  wire[0]: len=4, simple, closed, Unknown → passes
  wire[1]: len=2, simple, closed, Unknown → filtered (len < 3)
  1 face created → recovery fails (need ≥ 2)
```

### AABB-skipped ICs

ICs (1,1), (3,1), (9,0/1), (11,0/1) are correctly skipped by B17 AABB
guard — these are on torus faces far from the box intersection plane.

## Initial Hypothesis (Disproved)

The plan hypothesized that `has_boundary_edge_between` (D1.6) returns `true`
for torus face ICs because IC endpoints coincide with torus v-seam edge
vertices, incorrectly skipping `add_edge`. **This is wrong.** No `[ic_skip]`
messages appear for the critical torus faces. The D1.6/D1.7 skip logic is
not triggering — the failure is downstream in face division (NotSimpleWire).

## Proposed Fix Approaches (for remaining open edges)

### Approach A: Multi-IC face division (recommended)

When a face receives multiple ICs that form a closed loop, recognize this
as a "face subdivision" case rather than individual edge splices. Instead of
splicing each IC into the boundary wire sequentially (creating figure-8s),
construct the subdivision directly:

1. Detect when N ≥ 3 ICs on a face form a closed loop
2. Build the IC loop as a separate inner wire
3. Divide the face into inside/outside regions using the IC loop
4. This avoids non-simple wires entirely

**Complexity**: Medium. Requires new face division path for multi-IC loops.

### Approach B: Pre-split boundary at IC splice points

Before adding IC edges, split the boundary wire at all IC endpoint
locations. This creates separate boundary segments between splice points,
so adding ICs never creates figure-8 self-intersections:

1. Collect all IC endpoints on the face boundary
2. Split boundary edges at those parameter values
3. Then add IC edges between the split vertices
4. The resulting wire is a simple polygon with IC edges as cross-cuts

**Complexity**: Medium-high. Boundary edge splitting must preserve surface
parametrization and neighbor face edge sharing.

### Approach C: Torus face pre-decomposition

Before boolean operations, decompose full-revolution torus faces (360°)
into smaller angular sectors (e.g., 4 × 90° sectors). This:

1. Eliminates the v-seam ambiguity problem
2. Ensures each sector face gets at most 1-2 ICs
3. Keeps face division simple (single IC per face)

**Complexity**: Low for torus, but must be generalized for other periodic
surfaces.

## Affected Tests

| Test | Status | Notes |
|------|--------|-------|
| RB1 `rb1_revolve_union_with_box` | FAIL | 8 open edges, NotSimpleWire (B21 activates but insufficient) |
| RB6 `rb6_box_union_with_revolve` | FAIL | Same root cause (commutative) |
| RB2, RB5, RB8 | ignored | Different root cause (IC edge refs=1) |
| RB3, RB4, RB7 | PASS (RB3 pre-existing fail) | Partial revolve (not 360°), no v-seam |

## Invariants

- **I-TORUS1**: Face division must handle N ≥ 3 co-face ICs forming closed
  loops without creating non-simple wires
- **I-TORUS2**: Torus union with box must produce 0 open edges and valid
  manifold solid

## Future Work

- Implement Approach A (multi-IC face division) or Approach C (torus
  pre-decomposition) to fully resolve the 8 open edges
- Investigate IC edge identity between shells (FBG vs loops_store Arc<Edge>)
- Generalize to any periodic surface (sphere, cone with seam)
- RB2/RB5/RB8 have a different root cause (IC edge sharing refs=1) that
  requires separate investigation
