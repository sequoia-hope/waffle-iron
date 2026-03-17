# Oblique Plane-Cylinder SSI

**Sprint**: G
**Status**: Implementing
**References**: Patrikalakis Ch.5 — SSI algorithms for analytic surfaces

## Problem Statement

`plane_cylinder_ssi()` returns `NotSupported` for oblique cuts (where the cutting
plane is neither perpendicular nor parallel to the cylinder axis). This is the #1
SSI gap — every box-cylinder boolean with non-axis-aligned geometry hits it.

The intersection of an oblique plane with a cylinder is an **ellipse**. This requires
a new `SSICurve::Ellipse` variant and a corresponding `Ellipse3D` curve geometry type.

## Math Derivation

Given:
- Cylinder axis direction: `W` (unit vector)
- Cylinder radius: `r`
- Cylinder origin: point on axis
- Cutting plane normal: `N` (unit vector)
- Cutting plane origin: point on plane

### Angle between plane and cylinder

```
cos_angle = |W · N|
sin_gamma = sqrt(1 - cos_angle²)
```

- `sin_gamma ≈ 0`: degenerate (plane nearly parallel to axis) → handled by parallel case
- `cos_angle ≈ 1`: perpendicular → handled by perpendicular case
- Otherwise: oblique → ellipse

### Ellipse parameters

```
semi_minor = r                    (cylinder radius)
semi_major = r / sin_gamma        (≥ semi_minor, stretching from tilt)
```

### Major axis direction

The major axis lies in the cutting plane, in the direction of the projection of
the cylinder axis onto the plane:

```
major_axis = normalize(W - (W · N) * N)
```

### Center point

The center of the ellipse is where the cylinder axis intersects the cutting plane:

```
t = ((plane_origin - cyl_origin) · N) / (W · N)
center = cyl_origin + t * W
```

### Height range check

The parameter `t` must fall within `cyl_height_range` for the intersection to exist.

## Edge Cases

1. **Near-parallel** (`sin_gamma < TOL`): Return empty (degenerate ellipse with
   infinite semi_major). Already handled by the parallel branch.
2. **Center outside height range**: Return empty.
3. **Near-perpendicular** (`cos_angle > 1 - TOL`): Return circle (already handled).

## Test Plan

| Test | Input | Expected |
|------|-------|----------|
| `test_plane_cylinder_oblique_45deg` | 45° tilt | semi_major = r√2, semi_minor = r |
| `test_plane_cylinder_oblique_30deg` | 30° tilt | semi_major = 2r |
| `test_plane_cylinder_oblique_near_perp` | 89° tilt | near-circular ellipse |
| `test_plane_cylinder_oblique_tilted_axis` | non-Z cylinder | correct center/axes |
| `test_plane_cylinder_oblique_out_of_range` | center outside height | empty |
