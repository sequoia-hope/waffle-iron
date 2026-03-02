# Spec: BooleanTolerance — Per-Stage Tolerance Configuration

**Burndown ID**: A2
**Status**: Implemented
**Location**: `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`

## History

Originally specified as `BooleanOptions` in `crates/kernel-fork/src/types.rs` with 6 fields
(`tau_model`, `tau_mesh`, `tau_weld`, `tau_work`, `tau_coplanar`, `min_feature_size`).
During implementation the struct was renamed to `BooleanTolerance`, placed directly in
truck-shapeops where it is consumed, and the field set was revised:

- `tau_work` and `min_feature_size` were dropped (not needed by any pipeline stage).
- `tau_boundary` was added (IC-on-boundary filter, replacing an inline `tol * 0.5`).
- `tau_edge_cluster` was added (midpoint clustering, replacing an inline `tol * 5.0`).
- `tau_area` was added (minimum parametric face area, replacing an inline `tol * tol`).

## Problem

The boolean pipeline has multiple stages with different precision needs. Using a single
`tol` for all stages causes failures: a tolerance right for mesh collision is too coarse
for vertex welding, and vice versa. Each stage needs its own tolerance scaled from a
single model-level precision value.

## Struct Definition

```rust
#[derive(Clone, Debug)]
pub struct BooleanTolerance {
    /// Main coincidence/intersection tolerance (model precision).
    pub tau_model: f64,
    /// Mesh collision resolution tolerance (triangulation accuracy).
    pub tau_mesh: f64,
    /// Vertex unification tolerance in weld_coincident_edges.
    pub tau_weld: f64,
    /// Coplanar face detection threshold (normal parallelism + plane distance).
    pub tau_coplanar: f64,
    /// IC-on-boundary filter tolerance.
    pub tau_boundary: f64,
    /// Phase 1 midpoint clustering tolerance in weld_coincident_edges.
    pub tau_edge_cluster: f64,
    /// Minimum parametric face area threshold in divide_one_face.
    pub tau_area: f64,
}
```

## Constructor: `from_model_tol(tau_model)`

Derives all per-stage tolerances from a single model tolerance:

| Field | Scaling | Rationale |
|-------|---------|-----------|
| `tau_mesh` | `1.0 * tau_model` | Triangulation needs model-level precision |
| `tau_weld` | `0.4 * tau_model` | Conservative: slightly wider than old `tol * 0.2` default, but below the failure threshold of ~0.10 * min_edge. A 2x multiplier was too aggressive and merged vertices across small features. |
| `tau_coplanar` | `1.0 * tau_model` | Uses `tol` directly (not squared); a multiplier caused false coplanar detection for merely-close faces. |
| `tau_boundary` | `0.5 * tau_model` | IC endpoint projection onto face boundary — needs tighter than model tolerance to avoid spurious boundary hits. |
| `tau_edge_cluster` | `5.0 * tau_model` | Midpoint clustering in Phase 1 of weld needs wider tolerance to find coincident edge pairs across small gaps. |
| `tau_area` | `tau_model^2` | Area is a squared quantity; threshold scales as tolerance squared. |

```rust
pub fn from_model_tol(tau_model: f64) -> Self {
    Self {
        tau_model,
        tau_mesh: tau_model,
        tau_weld: 0.4 * tau_model,
        tau_coplanar: tau_model,
        tau_boundary: 0.5 * tau_model,
        tau_edge_cluster: 5.0 * tau_model,
        tau_area: tau_model * tau_model,
    }
}
```

## Deprecated Constructor: `uniform(tol)`

Sets all fields to the same value (except `tau_area = tol^2`). Matches legacy single-tol
behavior. Marked `#[deprecated]` — use `from_model_tol()` for new code.

## Invariants

For any valid `BooleanTolerance`:

- `tau_model > 0`
- `tau_weld < tau_model` (must not merge vertices beyond coincidence tolerance)
- `tau_boundary < tau_model` (IC-on-boundary must be tighter than general coincidence)
- `tau_edge_cluster > tau_model` (clustering must be wider to find pairs)
- `tau_area > 0`

## Integration

`BooleanTolerance` is consumed by the boolean pipeline in truck-shapeops. The caller
(kernel-fork's `TruckKernel`) computes `tau_model` from the geometry's bounding box
extent via `compute_adaptive_tol()`, then passes `BooleanTolerance::from_model_tol(tau_model)`
into the `and` / `or` operations.

## Files

| File | Role |
|------|------|
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | Struct definition + constructors |
| `crates/kernel-fork/src/truck_kernel.rs` | Computes `tau_model` from geometry, calls `from_model_tol()` |
| `crates/kernel-fork/src/healing.rs` | Receives tolerance for cascade healing |
