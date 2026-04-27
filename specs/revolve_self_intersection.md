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
- Profiles with a vertex on or near the axis line (`|perp| < TAU_MODEL`).

It does **NOT** catch profiles that straddle the axis with all vertices well clear
of the axis line itself. R0002's rectangle (PR14 Phase A investigation) had signed
perpendicular distances `[-0.31, -0.58, +0.31, +0.58]` — all ≫ TAU_MODEL, so
Check 2 passed. But the profile is bisected by the axis: revolving sweeps the
negative-side region 180° onto the positive-side region, producing 5280 inter-face
penetrations. Check 3 (added PR14) closes this gap.

### 3. Signed-Side Straddle Check (PR14)

After Check 2 passes, compute a stable in-plane reference direction perpendicular
to the axis:

```
ref_dir = axis_dir × plane_normal
if |ref_dir| < TAU_NORMALIZE:
    ref_dir = perps[0]  // fallback: use first vertex's perp component
ref_unit = ref_dir / |ref_dir|
```

For each profile vertex, project its `perp` component onto `ref_unit`:

```
signed = perp · ref_unit
if signed > +TAU_MODEL: saw_pos = true
if signed < -TAU_MODEL: saw_neg = true
```

If both `saw_pos && saw_neg`, return `KernelError::Other` with message
"revolve self-intersection: profile straddles the revolve axis".

The reference direction `axis_dir × plane_normal` is geometrically intrinsic
(in-plane direction perpendicular to the axis) and stable under profile
rotation/reordering. The fallback to `perps[0]` covers the degenerate case
where `axis_dir ∥ plane_normal` (revolve axis perpendicular to the profile
plane); Check 2's strict `dist > TAU_MODEL` guarantees `|perps[0]| > 0`.

The strict `> TAU_MODEL` / `< -TAU_MODEL` band gives vertices that brush
the axis (`|signed| ≤ TAU_MODEL`) the benefit of the doubt — they don't
update `saw_pos` or `saw_neg`. The genuine straddling case requires at
least one vertex strictly positive and another strictly negative, both
beyond the band.

## Featured Test Cases

| Case | Description | Expected |
|------|-------------|----------|
| F0073 | Rect boss + revolve with axis through profile center | `expect_rebuild_error: true` |
| F0074 | Circle boss + revolve with axis barely inside profile | `expect_rebuild_error: true` |
| F0075 | Rect boss + revolve with properly offset axis | `expect_rebuild_error: false` |

## Kernel Unit Tests

| Test | What it checks |
|------|---------------|
| `test_revolve_rejects_zero_axis` | axis_direction=[0,0,0] rejected (Check 1) |
| `test_revolve_rejects_profile_on_axis` | vertex coincident with axis rejected (Check 2) |
| `test_revolve_rejects_profile_crossing_axis` | vertex on axis line rejected (Check 2) |
| `test_revolve_accepts_offset_profile` | valid offset revolve succeeds |
| `pr14_validator_tests::test_revolve_axis_straddling_profile_rejected` | profile straddles axis (all vertices off-axis) rejected (Check 3, added PR14) |
| `pr14_validator_tests::test_revolve_one_sided_profile_succeeds` | one-sided profile passes Check 3 (regression guard) |

## Generator Status (PR14 Phase A finding)

The current `gen.rs` axis-construction logic at lines 614-628 (post-PR12+PR13)
still produces axis-straddling profiles in many R-series cases. The intended
offset is `1.5 × profile_size` along an in-plane perpendicular, but the
implementation uses `tangent` (which IS the axis direction itself) for the
offset, making it geometrically a no-op. This is **Defect 2** in the PR14
Phase A memo and is the scope of PR15.

Engineer-a's verification (R0002 specifics):
- Generator-computed `axis_origin - plane_origin = (-1.198, 0.833, 0)`,
  magnitude 1.460 = 1.5 × 0.973 (profile_size). Magnitude correct.
- But `dot(axis_origin - plane_origin, in_plane_perp)` ≈ 0 — the offset
  is along the axis line, not perpendicular to it. The axis line itself
  is unchanged.

Until PR15 lands and the corpus is regenerated, R-series cases with
profile_size large enough to extend past the (non-existent) offset will
fail Check 3 with "revolve self-intersection: profile straddles the revolve
axis". This is a test-result migration, not a regression — the kernel is
correctly rejecting invalid input.

## References

- Onshape behavior: self-intersecting revolves show "Revolve results in self-intersecting geometry"
- SolidWorks: similar rejection with "The resulting body would be self-intersecting"
- Mantyla [#16]: Euler operator validity requires manifold topology, which self-intersection violates
