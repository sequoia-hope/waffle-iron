# Spec: BooleanOptions — Layered Tolerance Context

**Burndown ID**: A2
**Author**: tolerance-architect
**Status**: Draft

## Problem

The current boolean pipeline uses two ad-hoc functions (`compute_adaptive_tol` and
`compute_healing_tol` in `truck_kernel.rs`) that derive a single tolerance from bounding
box extent. The spec (`SHAPEOPS-BOOLEAN-SPEC.md`) requires a layered tolerance model with
6 distinct tolerance fields, each serving a different stage of the boolean pipeline.

The global `truck_base::tolerance::TOLERANCE = 1e-6` is used pervasively but is too large
to preserve 1 um features when coordinates are in meters.

## Requirements

### R1: BooleanOptions struct

Add to `crates/kernel-fork/src/types.rs`:

```rust
#[derive(Debug, Clone)]
pub struct BooleanOptions {
    /// Model absolute tolerance — coincidence decisions, join/weld admissibility.
    /// Default: 1e-7 (preserves 1 um features at 10x margin).
    pub tau_model: f64,
    /// Meshing/intersection tolerance — tessellation and polyline construction.
    /// Must satisfy: tau_mesh <= tau_model.
    pub tau_mesh: f64,
    /// Vertex/edge welding tolerance — snapping during stitching.
    /// Derived as 0.4 * tau_model.
    pub tau_weld: f64,
    /// Numeric floor / working precision — iterative solver convergence.
    /// Must satisfy: tau_work << tau_model.
    pub tau_work: f64,
    /// Coplanar detection tolerance — same-plane / same-surface decisions.
    pub tau_coplanar: f64,
    /// Minimum preserved feature size. Default: 1e-6 (1 micrometer).
    pub min_feature_size: f64,
}
```

### R2: Default values

`BooleanOptions::default()` must produce spec-compliant values:
- `tau_model = 1e-7`
- `tau_mesh = 0.5 * tau_model = 5e-8`
- `tau_weld = 0.4 * tau_model = 4e-8`
- `tau_work = 1e-12`
- `tau_coplanar = 5.0 * tau_model = 5e-7`
- `min_feature_size = 1e-6`

### R3: Scale-aware constructor

`BooleanOptions::for_scale(extent: f64)` computes tolerances scaled to geometry size:
- `tau_model = (extent * 1e-7).clamp(1e-9, 1e-5)`
- Other fields derived from `tau_model` with the same ratios as defaults.

### R4: Validation

`BooleanOptions::validate() -> Result<(), String>` must reject:
- `tau_mesh > tau_model`
- `tau_work >= tau_model`
- `tau_weld < 0.1 * tau_model`
- `min_feature_size < tau_model`
- Any negative or zero tolerance

### R5: Invariants

These must hold for all valid `BooleanOptions`:
- `tau_work < tau_model`
- `tau_mesh <= tau_model`
- `tau_weld >= 0.1 * tau_model` (typically `0.4 * tau_model`)
- `min_feature_size >= tau_model`

### R6: Backward compatibility

`BooleanOptions::for_boolean_tol(tol: f64)` creates options that produce the same
operational behavior as the current `compute_adaptive_tol()` pipeline. This is used
in `TruckKernel` boolean methods so that existing behavior is preserved until callers
explicitly opt into layered tolerances.

### R7: Integration into TruckKernel

Replace `compute_adaptive_tol()` / `compute_healing_tol()` calls in
`TruckKernel::boolean_union/subtract/intersect` with `BooleanOptions::for_scale(extent)`.
The `healing.rs` functions should accept `heal_tol` derived from `options.tau_mesh`.

## Files to Modify

1. `crates/kernel-fork/src/types.rs` — Add `BooleanOptions` struct
2. `crates/kernel-fork/src/truck_kernel.rs` — Use `BooleanOptions` in boolean methods
3. `crates/kernel-fork/src/healing.rs` — Accept tolerance from options (no functional change)

## Test Plan

Tests go in `crates/kernel-fork/src/types.rs` (unit) and `crates/kernel-fork/src/truck_kernel.rs` (integration).

### Unit tests (types.rs)

1. `test_boolean_options_default` — default() produces spec values
2. `test_boolean_options_invariants` — default satisfies all R5 invariants
3. `test_boolean_options_for_scale` — scales correctly at 0.001m, 1m, 100m
4. `test_boolean_options_validate_rejects_bad` — rejects tau_mesh > tau_model, etc.
5. `test_boolean_options_validate_accepts_good` — accepts default and for_scale values
6. `test_boolean_options_for_scale_clamps` — extreme scales hit clamp bounds

### Integration tests (truck_kernel.rs)

7. `test_boolean_with_options_matches_existing` — for_scale produces same results as
   current compute_adaptive_tol for existing test corpus (box-box, box-cylinder)

## Non-Goals

- Changing truck-shapeops internals to consume BooleanOptions directly (future Phase A4)
- Local per-edge tolerances (Phase E3)
- Modifying `truck_base::tolerance::TOLERANCE` (too risky for Sprint 1)
