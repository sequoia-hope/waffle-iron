# Revolve Self-Intersection Detection

## Problem

Revolving a profile around an axis that passes through or too close to the profile
creates self-intersecting geometry. This is physically impossible and produces
degenerate meshes. Onshape rejects such revolves with an error — we match that behavior.

## Root Cause (Generator)

The assay generator (`gen.rs:595-610`) constructed revolve axes as:
```
axis_direction = [normal.y, -normal.x, 0.0]
axis_origin += [offset * normal.y, -offset * normal.x, 0.0]
```

For the standard XY plane (normal=[0,0,1]):
- `axis_direction = [0, 0, 0]` — **degenerate zero vector**
- `axis_origin offset = [0, 0, 0]` — axis passes through profile center

For general normals, `[ny, -nx, 0]` is not guaranteed to lie in the sketch plane.

## Fix: In-Plane Tangent via Cross Product

Compute a tangent vector that lies in the sketch plane:
1. Pick the world axis least aligned with the normal (smallest |dot product|)
2. Cross the normal with this helper to get a vector perpendicular to the normal (i.e., in-plane)
3. Normalize the result

This is the same algorithm as `compute_plane_basis` in `kernel/src/vecmath.rs`.

The axis is then offset along this tangent by 1.5× the profile size, ensuring the entire
profile stays on one side of the axis.

## Kernel Validation

Two checks added to `revolve_polygon()`:

### 1. Zero-Axis Rejection
If `|axis_direction| < TAU_MODEL`, return `KernelError::Other` with message
"revolve axis direction is degenerate (zero-length)".

### 2. Profile-to-Axis Distance Check
For each profile vertex, compute perpendicular distance to the axis line:
```
to_v = vertex - axis_origin
along = dot(to_v, axis_dir)
perp = to_v - along * axis_dir
dist = |perp|
```
If `dist < TAU_MODEL` for any vertex, return `KernelError::Other` with message
"revolve self-intersection: profile vertex at distance ... from axis".

This catches:
- Profiles centered on the axis (all vertices equidistant, some at zero)
- Profiles straddling the axis (vertices on both sides, some near-zero distance)
- Profiles with a vertex exactly on the axis

## Featured Test Cases

| Case | Description | Expected |
|------|-------------|----------|
| F0073 | Rect boss + revolve with axis through profile center | `expect_rebuild_error: true` |
| F0074 | Circle boss + revolve with axis barely inside profile | `expect_rebuild_error: true` |
| F0075 | Rect boss + revolve with properly offset axis | `expect_rebuild_error: false` |

## Kernel Unit Tests

| Test | What it checks |
|------|---------------|
| `test_revolve_rejects_zero_axis` | axis_direction=[0,0,0] rejected |
| `test_revolve_rejects_profile_on_axis` | vertex coincident with axis rejected |
| `test_revolve_rejects_profile_crossing_axis` | vertex on axis line rejected |
| `test_revolve_accepts_offset_profile` | valid offset revolve succeeds |

## References

- Onshape behavior: self-intersecting revolves show "Revolve results in self-intersecting geometry"
- SolidWorks: similar rejection with "The resulting body would be self-intersecting"
- Mantyla [#16]: Euler operator validity requires manifold topology, which self-intersection violates
