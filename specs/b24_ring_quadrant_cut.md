# B24: Ring + Quadrant Cut Fix

## Problem

After creating a cylindrical ring (cylinder dia 10 extruded, dia 5 full-depth cut),
sketching a circle at a quadrant on the ring's annular top face and cutting produces:

> "v2: 6 open edges after all levels (3 weld + force_merge + fill)"

## Parameters

- **Ring outer**: true NURBS circle r=5, extrude h=10
- **Ring inner**: true NURBS circle r=2.5, extrude cut h=10 (full depth)
- **Quadrant notch**: true NURBS circle r=1, center at (3.75, 0), extrude cut h=10

The ring's top face is **annular** — it has 2 boundary loops (outer circle + inner hole).
The quadrant cut's interference curves (ICs) intersect this multi-loop topology.

## Root Cause (Hypothesis)

The annular face already has an inner loop from the ring cut. When the notch boolean
creates ICs on this face, the coplanar pipeline (containment injection, divide_face,
construct_ring_disc_direct) may not handle faces with 2+ holes correctly:

1. `inject_coplanar_boundary_loops` may not detect containment correctly against
   a face that already has an inner boundary
2. `construct_ring_disc_direct` may assume at most 1 inner loop
3. ICs between the notch lateral face and the ring's inner lateral face may create
   figure-8 wires (similar to B21/B22)

## Invariants

- **0 open edges** in result solid
- **V-E+F = 2** (or 2+h for h inner boundary loops on non-simply-connected faces)
- **Volume** = ring_vol - notch_vol (±20%)
  - ring_vol = π * (5² - 2.5²) * 10 ≈ 589.0
  - notch_vol = π * 1² * 10 ≈ 31.4
  - expected ≈ 557.6

## Test Cases

| ID    | Description                                        | Method           |
|-------|----------------------------------------------------|------------------|
| BNC9  | Ring + quadrant cut via feature engine              | extrude + cut    |
| BNC10 | Ring + notch via explicit boolean subtract          | boolean_subtract |
| BNC11 | Notch straddling inner hole boundary (hardest case) | boolean_subtract |

### BNC11 Details

Notch center at (2.5, 0), r=1.5 — the notch circle overlaps the inner boundary.
ICs must split both the outer boundary and inner boundary of the annular face.

## Key Files

| File | Purpose |
|------|---------|
| `vendor/truck/truck-shapeops/src/transversal/divide_face/mod.rs` | Face division with multi-loop |
| `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs` | IC injection, containment |
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | Shell assembly |

## Resolution

**No fix needed.** The boolean pipeline (B14-B23) already handles these scenarios correctly:

- **BNC9** (feature engine path): Ring creates annular top face with 2 boundary loops.
  Notch cut succeeds via coplanar ring/disc direct path (B19). No ICs needed for
  containment-only coplanar faces. Passes in ~1.6s.
- **BNC10** (explicit boolean path): Same result via 2 sequential `boolean_subtract` calls.
  Passes in ~1.6s.
- **BNC11** (notch straddling inner boundary): 8 ICs detected between ring and notch
  lateral faces. B22 multi-IC chain detection + B17 AABB guard + perturbation recovery
  handle the complex topology. Passes in ~88s (30 perturbation attempts for face2
  NotSimpleWire recovery).

The pipeline improvements from B14-B23 (coplanar containment, AABB guards, multi-IC chains,
boundary-coincident IC handling) collectively made these annular-face scenarios work without
any additional changes.

Tests BNC9-11 added as regression coverage (16 total coplanar_curved tests, all passing).
