# Analytical SSI: Cylinder-Cylinder Intersection

**Status**: Implementation
**Sprint**: 42
**Priority**: P3 (per SSI audit)

## Summary

Extend the analytical SSI module to compute exact intersection curves for
equal-radius cylinders with intersecting axes. The intersection consists of
two closed elliptic curves. Mesh-based polyline points are projected onto
the closest exact ellipse, eliminating BSpline drift for this surface pair.

## Mathematical Basis

For two equal-radius cylinders (radius R) with intersecting axes at angle
alpha, the intersection consists of two closed elliptic curves. In a local
frame where e1 = a1 (first cylinder axis), e2 = (a2 - (a2.a1)*a1).normalize(),
e3 = e1 x e2:

```
Curve 1: X(t) = origin + R*cos(t) * (cot(alpha/2)*e1 + e2) + R*sin(t) * e3
Curve 2: X(t) = origin + R*cos(t) * (-tan(alpha/2)*e1 + e2) + R*sin(t) * e3
```

Semi-axes:
- Curve 1: semi_u = R/sin(alpha/2), semi_v = R
- Curve 2: semi_u = R/cos(alpha/2), semi_v = R

For perpendicular axes (alpha=90 deg), both semi_u = R*sqrt(2), semi_v = R.

## Scope

**In scope**:
- Equal-radius cylinders (within 1% relative tolerance)
- Intersecting axes (closest distance < 5% of R)
- Angle between axes > 60 degrees

**Deferred** (returns None, falls back to mesh-based):
- Unequal radii (degree-4 algebraic curves)
- Parallel or coaxial axes
- Skew (non-intersecting) axes
- Near-parallel axes (angle < 60 deg)

## AnalyticalIC Refactoring

The `AnalyticalIC` struct changes from single to multi-ellipse:

```rust
// Before:
pub struct AnalyticalIC { ellipse: EllipseParams }
// After:
pub struct AnalyticalIC { ellipses: Vec<EllipseParams> }
```

All existing `try_analytical_*_ic` wrap their single ellipse in `vec![ellipse]`.
The `refine_polyline` function uses `pick_closest_ellipse` to select which
ellipse to project each polyline onto.

## Dispatch Chain

Cylinder-cylinder goes last (most specific pair):
```rust
let analytical = try_analytical_plane_cylinder_ic(...)
    .or_else(|| try_analytical_plane_cone_ic(...))
    .or_else(|| try_analytical_plane_sphere_ic(...))
    .or_else(|| try_analytical_cylinder_cylinder_ic(...));
```

## Guard Conditions (return None)

| Condition | Threshold |
|-----------|-----------|
| Both surfaces are cylinders | Required |
| Equal radii | `|R1 - R2| / max(R1, R2) < 0.01` |
| Axes intersect | closest distance < `0.05 * R` |
| Non-parallel axes | `|cos(angle)| < 1 - 1e-6` |
| Angle > 60 degrees | `|cos(angle)| < 0.5` |

## Invariants (Test Oracles)

1. Every point on each returned ellipse lies on both cylinders (dist to axis = R, tol 1e-6)
2. Both returned ellipses share the same center (axis intersection point)
3. Semi-axes: curve 1 = R/sin(alpha/2) and R; curve 2 = R/cos(alpha/2) and R
4. For perpendicular case: both semi-axis pairs are (R*sqrt(2), R)
5. Polyline refinement preserves point count
6. Single-ellipse AnalyticalIC behavior unchanged

## Test Plan

12 unit tests (CC1-CC12) + 1 integration test (CC_INT1).
See implementation for details.
