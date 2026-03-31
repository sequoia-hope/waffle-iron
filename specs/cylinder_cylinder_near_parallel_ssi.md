# Spec: Cylinder–Cylinder Near-Parallel SSI (Pair #5 Completion)

**FIP Phase 1** | **A15.4 pair #5** | **Priority: 2 (SSI solvers)**

## Goal

Remove the 15° minimum-angle guard in `cylinder_cylinder_ssi_non_parallel()` so
that the solver handles inter-axis angles down to TAU_PARALLEL (≈ 0.00006°).
This completes pair #5 in the A15.4 quadric SSI matrix.

### History

- Sprint 68: Solver lowered from 60° to 15° (dual-ellipse equal-R + Degree4CylCyl unequal-R).
- This spec: Remove the remaining 15° floor. The formulas are exact at any α > 0.

## Parameters

| Parameter | Type | Constraint |
|-----------|------|-----------|
| `cyl_a_origin` | `[f64; 3]` | arbitrary |
| `cyl_a_axis` | `[f64; 3]` | unit vector |
| `cyl_a_radius` | `f64` | > 0 |
| `cyl_b_origin` | `[f64; 3]` | arbitrary |
| `cyl_b_axis` | `[f64; 3]` | unit vector |
| `cyl_b_radius` | `f64` | > 0 |

## Branch Table

| # | Condition | Result | Status |
|---|-----------|--------|-------|
| B1 | cos ≥ 1 − TAU_PARALLEL | Delegate to parallel solver | unchanged |
| B2 | Equal-R, any angle > TAU_PARALLEL | 2 Ellipses (dual-ellipse) | **extended** (was ≥15°) |
| B3 | Unequal-R, any angle > TAU_PARALLEL | 2 Degree4CylCyl | **extended** (was ≥15°) |
| B4 | Skew axes (dist ≥ 5%·R_max) | NotSupported | unchanged |
| B5 | Zero radius | NotSupported | unchanged |

## Implementation Approach

1. **Remove the `SSI_CYL_CYL_MIN_ANGLE_COS` guard check** in
   `cylinder_cylinder_ssi_non_parallel()` (line ~1149).
2. **Remove or deprecate the `SSI_CYL_CYL_MIN_ANGLE_COS` constant** from `units.rs`.
3. **No formula changes**: The dual-ellipse and Degree4CylCyl parametrizations
   are valid at all angles above TAU_PARALLEL.

### Numerical Stability Analysis

- **Equal-R**: semi_major = R/sin(α/2). At 1° → 57.3R. At 0.1° → 573R.
  At 0.01° → 5730R. Float64 handles this without issue.
- **Unequal-R**: z(θ) = (R_A sin θ cos α ± √(R_B² − R_A² cos²θ)) / sin α.
  Division by sin(α) amplifies, but numerator also shrinks proportionally
  (it approaches R_A sin θ ± R_B as α → 0). No catastrophic cancellation.
- **Limiting behavior**: As α → 0 with equal-R, the dual-ellipse degenerates
  to two parallel lines (matching the parallel solver output). The transition
  is smooth.

## Invariants

### I1 — On-surface invariant
Every returned curve point lies on both cylinder surfaces within TAU_MODEL:
- dist(P, axis_A) ≈ R_A (within TAU_MODEL = 1e-7)
- dist(P, axis_B) ≈ R_B (within TAU_MODEL = 1e-7)

### I2 — Semi-axis formula (equal-R)
- Curve 1: semi_major = R / sin(α/2), semi_minor = R
- Curve 2: semi_major = R / cos(α/2), semi_minor = R

### I3 — Curve count
Exactly 2 curves for non-degenerate intersecting configurations.

### I4 — No NaN/infinity
All returned coordinates and parameters are finite.

## Oracles

### O1 — On-surface distance oracle
Sample 32 points along each curve. Assert each point lies on both cylinder
surfaces within relaxed tolerance (0.01 for near-parallel, TAU_MODEL otherwise).

### O2 — Semi-axis magnitude oracle (equal-R)
Assert semi_major ≈ R/sin(α/2), semi_minor ≈ R within 1% relative error.

### O3 — Finiteness oracle
All output fields finite, no NaN.

### O4 — Curve type oracle
Equal-R → Ellipse, Unequal-R → Degree4CylCyl.

## Failure Modes

| Condition | Expected behavior |
|-----------|------------------|
| Parallel (cos > 1 − TAU_PARALLEL) | Empty vec (redirect to parallel solver) |
| Skew axes | `KernelError::NotSupported` |
| Zero radius | `KernelError::NotSupported` |

## Research Basis

- **[#1] Patrikalakis et al., Ch.5**: Dual-ellipse formula R/sin(α/2) and
  R/cos(α/2) is exact for any α ∈ (0°, 180°). The degree-4 parametric
  curve z(θ) for unequal-R is similarly exact.
- **[#25] Yang et al. (2023)**: Topology-guaranteed SSI.
