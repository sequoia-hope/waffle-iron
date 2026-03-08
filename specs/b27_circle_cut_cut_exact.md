# B27: Circle-Cut-Cut Exact Boolean — Remove Perturbation for Parallel-Axis Cylinder Subtract

## Problem

The `circle-cut-cut` test case (circle boss → circle cut → circle cut, all on XZ plane) **fails in the GUI** with "v2: 2 open edges" but passes in native Rust tests (attempt #27 of 30).

### Root Causes

1. **WASM 3-attempt perturbation limit**: `MAX_WASM_CASCADE_ATTEMPTS = 3` (`healing.rs:1968`). Native tests get 120s / 27+ attempts; WASM bails after 3.

2. **Direct (unperturbed) boolean panics with SameVertex**: For parallel-axis, different-radii cylinders (boss r=11.6, cut r=6.64), the `cylinder_cylinder_ic` analytical path returns `None` (unequal-radii guard, `analytical.rs:2017`). Lateral-lateral face pair ICs fall through to mesh-based extraction, producing short/degenerate polylines whose endpoints get canonicalized to the same vertex → `Edge::new` panics at `loops_store/mod.rs:1444`.

## Geometry

- **Boss**: True circle, r=11.6, normal=+X, extrude +X by 10
- **Cut 1**: True circle, center=(-0.226, 11.09), r=6.64, depth=10 (partial overlap with boss)
- **Cut 2**: True circle, center=(-11.17, -11.05), r=4.68, depth=10 (partial overlap)
- All cylinders have parallel axes (all along X)
- Unequal radii between every pair

## Branch Table

| Configuration | Existing Path | New Path (B27) |
|---|---|---|
| Equal-radii cylinders | `compute_cylinder_cylinder_intersection` → ellipses | Unchanged |
| Unequal-radii, parallel axes | Falls through to mesh → degenerate ICs | `try_detect_parallel_cylinders_skip` → empty ICs (suppressed) |
| Non-parallel cylinders | Mesh-based IC extraction | Unchanged |

## Fixes

### Fix 3A: Degenerate IC guard post-canonicalization

**File**: `vendor/truck/truck-shapeops/src/transversal/loops_store/mod.rs:1442`

Before `Edge::new(&gv0, &gv1, ic_curve)`, skip when `gv0 == gv1`. Safety net against SameVertex panic regardless of upstream IC quality.

### Fix 3B: Suppress lateral-lateral ICs for parallel unequal-radii cylinders

**File**: `vendor/truck/truck-shapeops/src/transversal/intersection_curve/analytical.rs`

New function `try_detect_parallel_cylinders_skip`:
- Detect both surfaces as cylinders via `detect_cylinder`
- Check axes parallel (`|cos_angle| > 1 - 1e-6`)
- Check radii unequal (`|r0-r1|/r_max > 0.01`)
- Return `Some(empty AnalyticalIC)` → no lateral-lateral ICs for this face pair
- Real ICs come from plane-cylinder pairs (end caps vs laterals), which already work analytically

### Fix 3C: Increase WASM cascade limit 3→10

**File**: `crates/kernel-fork/src/healing.rs:1968`

Increase from 3 to 10. Stack is 4MB, each attempt is iterative (not recursive). 10 covers: 1 direct + 2 coplanar-composite + 7 directional strategies.

## Oracles

- Volume monotonically decreasing across boss → cut1 → cut2
- V-E+F=2 (Euler characteristic for genus-0 solids)
- 0 open edges
- No SameVertex panic

## Failure Modes

- Degenerate lateral-lateral ICs from mesh extraction of parallel cylinders
- Canonicalization vertex merging (short polyline endpoints → same vertex)
- WASM cascade exhaustion before finding working perturbation
