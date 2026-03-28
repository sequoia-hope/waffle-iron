# Spec: Cylinder-Cylinder Near-Parallel SSI Extension

Extend the non-parallel cylinder-cylinder SSI solver to support inter-axis
angles from 15° to 60° (previously only ≥60°).

## Goal

Lower the minimum inter-axis angle threshold for the analytical dual-ellipse
cylinder-cylinder SSI solver from 60° to 15°. This widens analytical coverage
for the #5 most common SSI pair in CAD, enabling boolean operations on
intersecting cylindrical features at shallower angles (e.g., angled pipe
junctions, V-block fixtures, engine port intersections).

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| cyl_a_origin | [f64; 3] | Point on cylinder A axis |
| cyl_a_axis | [f64; 3] | Unit direction of cylinder A axis |
| cyl_a_radius | f64 | Radius of cylinder A (meters) |
| cyl_b_origin | [f64; 3] | Point on cylinder B axis |
| cyl_b_axis | [f64; 3] | Unit direction of cylinder B axis |
| cyl_b_radius | f64 | Radius of cylinder B (meters) |

**Constraints (unchanged)**:
- Equal radii required: |r_a - r_b| / max(r_a, r_b) < SSI_RADII_RELATIVE_TOL (1%)
- Axes must (nearly) intersect: closest distance < SSI_SKEW_FACTOR × R (5%)
- Axes must not be parallel: |cos(angle)| < 1 - TAU_PARALLEL

**Changed constraint**:
- Minimum angle: 15° (was 60°)
- Implemented as: |cos(angle)| ≤ cos(15°) ≈ 0.9659

## Branch Table

| Inter-axis angle | cos(angle) range | Behavior | Status |
|-----------------|------------------|----------|--------|
| 0°–~0° (parallel) | > 1 - TAU_PARALLEL | Redirect to parallel solver | unchanged |
| ~0°–15° | [cos(15°), 1-TAU_PARALLEL] | NotSupported | unchanged |
| 15°–60° | [0.5, cos(15°)] | **Dual-ellipse analytical** | **NEW** |
| 60°–90° | [0, 0.5] | Dual-ellipse analytical | unchanged |

## Invariants

### I1 — On-surface invariant
Every point on the returned ellipses must lie on both cylinder surfaces
within TAU_COINCIDENT (1e-9 m):

For a point P on an ellipse:
- dist(P, axis_A) ≈ r_A (within TAU_COINCIDENT)
- dist(P, axis_B) ≈ r_B (within TAU_COINCIDENT)

where dist(P, axis) = ||(P - origin) - ((P - origin)·axis)·axis||

### I2 — Semi-axis formula invariant
The two intersection ellipses have semi-axes determined by the inter-axis
angle α:

- Curve 1: semi_major = R / sin(α/2), semi_minor = R
- Curve 2: semi_major = R / cos(α/2), semi_minor = R

where R is the (average) cylinder radius.

### I3 — Curve count invariant
For non-degenerate intersecting equal-radius cylinders with non-parallel
non-skew axes, exactly 2 ellipses are returned.

### I4 — Ellipse planarity
Each returned ellipse lies in a plane (its normal defines the plane).
The ellipse center lies in this plane.

### I5 — Continuity with existing range
Results at exactly 60° must be identical to the previous implementation
(no behavioral change at the existing boundary).

## Oracles

### O1 — On-surface distance oracle
Sample N points along each ellipse (N ≥ 16) at uniform parameter spacing.
For each point P(t) = center + semi_major·cos(t)·major_axis + semi_minor·sin(t)·minor_axis:
- Assert dist(P(t), axis_A) is within TAU_COINCIDENT of r_A
- Assert dist(P(t), axis_B) is within TAU_COINCIDENT of r_B

### O2 — Semi-axis magnitude oracle
Assert semi_major and semi_minor match the analytical formulae to within TAU_WORK.

### O3 — Eccentricity bound oracle
At 15°: eccentricity of curve 1 ≈ sqrt(1 - sin²(7.5°)) ≈ 0.9914.
Assert eccentricity < 1.0 (not degenerate).

### O4 — No NaN/infinity oracle
All output coordinates, axes, and scalars are finite and non-NaN.

## Failure Modes

| Condition | Expected behavior |
|-----------|------------------|
| angle < 15° | `KernelError::NotSupported` with message "near-parallel axes (angle < 15°)" |
| angle ≈ 0° (parallel) | Empty vec (redirect to parallel solver) |
| Unequal radii | `KernelError::NotSupported` (unchanged) |
| Skew axes | `KernelError::NotSupported` (unchanged) |
| Zero radius | `KernelError::NotSupported` (unchanged) |

## Research Basis

- **[#1] Patrikalakis et al., Ch.5**: Describes the dual-ellipse SSI formula for
  equal-radius cylinders at arbitrary intersection angles. The formula
  `semi_major = R/sin(α/2)` and `semi_major = R/cos(α/2)` for the two curves
  is exact for any α ∈ (0°, 180°). Our 15° floor is a numerical convenience,
  not a mathematical limitation.

- **[#25] Yang et al. (2023)**: Topology-guaranteed SSI via Dixon resultant.
  Informs future work on unequal-radius cylinder-cylinder SSI (degree-4 curves).

## Implementation Notes

The core dual-ellipse algorithm in `cylinder_cylinder_ssi_non_parallel` is
unchanged. The only code change is:

1. Add `SSI_CYL_CYL_MIN_ANGLE_COS: f64 = 0.9659` to `units.rs`
   (cos(15°), with margin for float comparison)
2. Replace guard `cos_angle > 0.5 + TAU_COINCIDENT` with
   `cos_angle > SSI_CYL_CYL_MIN_ANGLE_COS`
3. Update error message to say "angle < 15°"

No changes to the ellipse computation, frame construction, or output format.
