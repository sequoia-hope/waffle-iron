# Spec: Boolean Vertex Welding Fix (Track B)

## Problem

Boolean operations produce non-manifold results with unpaired half-edges.
Affects ~10 assay cases: R0004, R0043, R0052, R0064, R0076, R0083, R0096, etc.

Error message: "non-manifold result: N half-edges unpaired out of M"

## Root Cause

In `build_brep_from_polygons()` (boolean.rs ~line 1265):

1. **Vertex quantization boundary straddling**: Vertices quantized to
   `tau_weld=1e-7` grid can land on different sides of a grid boundary,
   causing same-position vertices to get different indices.

2. **Index-based twin pairing fails**: Twin lookup uses
   `directed_he.get(&(dest_idx, origin_idx))` which fails when same-position
   vertices got different quantized indices.

3. **T-junction gaps**: Clipping produces T-junctions between face fragments
   that don't get resolved, leaving boundary edges without twins.

## Fix

### Part 1: Position-based twin pairing

Replace the index-based `directed_he` map with a position-based lookup:

```
// Key: (quantize(origin_pos), quantize(dest_pos))
// Instead of: (origin_idx, dest_idx)
```

This ensures that edges connecting the same physical locations pair correctly
even if vertex indices differ.

### Part 2: Fallback position-based twin sweep

After the primary twin pairing pass, sweep remaining unpaired half-edges and
try to match them using quantized position keys instead of vertex indices.

## Location

- `crates/kernel/src/boolean.rs`
- Function: `build_brep_from_polygons()`
- Lines ~1265-1557

## Oracles

- `check_watertight_mesh(mesh)`: 0 unpaired edges
- Euler's formula: V - E + F = 2
- Boolean succeeds (no BooleanFailed error)

## Research References

- Ref #2 Hoffmann Ch.4: Numerical robustness in B-Rep operations
- Ref #4 Shewchuk: Adaptive precision predicates
- Ref #6 Sugihara-Iri: Topology-oriented robustness

## Tests (RED phase)

- BW4: gear boss + rect cut → no "non-manifold" error
- BW5: gear boss + gear union → watertight mesh
- BW6: two overlapping gear bosses → no unpaired edges

## Expected Yield

~10 assay cases move from fail to pass.
