# Stable Edge-Point Ordering for Conformal Mesh Arrangement

**Status:** Proposed
**Related:** Yang 2025 §4.2, Cherchi 2020 [#9] §5 arrangement
(`aux_structure.h:190` in the C++ reference); also Cherchi 2022 [#38] §4 which
inherits the same edge-centric architecture from 2020.

## Problem

100/157 watertight failures in `yang_fast` trace to non-conformal mesh subdivision.
Two triangles sharing an edge receive the same intersection points but sort them
in different orders due to floating-point instability in parametric t computation.

The parametric t value `dot(AP, AB) / |AB|^2` suffers ULP-level differences when
computed from different calling contexts (different triangle orientations, different
intermediate rounding). This causes:

1. **Sorting instability** -- same points, different order on shared edges
2. **Cross-mesh blind spots** -- face-interior points not propagated to opposite
   mesh edge subdivisions
3. **Reversal precision loss** -- reversing a point list recomputes t with different
   rounding
4. **Dedup gaps** -- same quantized position but different vertex indices create
   duplicate sub-edges

## Proposed Fix

### 1. Quantized parametric comparison

In `build_global_edge_points_map()`, quantize t values before sorting:

```rust
let t_quant = (t * 1e9).round() as i64;
```

Sort by `t_quant` (integer comparison, deterministic). Both triangles sharing an
edge see the identical sorted order regardless of floating-point path.

### 2. Cross-mesh edge-point propagation

When an intersection point lies on mesh A's face interior but also on mesh B's
edge (or vice versa), add it to the global edge map for mesh B's edge. Currently,
only constraint points from directly-involved triangles are checked.

### 3. Canonical edge storage

Store points once in canonical order (v_min -> v_max). When a triangle needs points
for edge (v_max, v_min), iterate in reverse. No re-sorting, no reversal instability.

### 4. Dedup by quantized t

After collecting all points on an edge, deduplicate by quantized parametric position:

```rust
let mut seen_t = HashSet::new();
edge_points.retain(|&(vi, t_quant)| seen_t.insert(t_quant));
```

## Verification

Four tests in `exact_mesh.rs`:

| Test | Property | Currently |
|------|----------|-----------|
| `test_stable_edge_ordering_quantized` | ULP-close t values sort stably | FAILS (unstable sort) |
| `test_cross_mesh_edge_propagation` | Face-interior point in opposite edge map | FAILS (not propagated) |
| `test_conformality_with_stable_ordering` | 0 non-conformal edges | FAILS (>0) |
| `test_full_pipeline_watertight_stable` | 0 unpaired HEs, Euler=2 | FAILS (unpaired) |

## Affected Code

- `crates/kernel/src/boolean/exact_mesh.rs`:
  - `build_global_edge_points_map()` -- quantize + canonical sort + dedup
  - `enrich_constraints_with_shared_edge_points()` -- reverse iteration, cross-mesh
