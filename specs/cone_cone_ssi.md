# Cone–Cone SSI Solver

**A15 pair #9** — Surface-surface intersection for two cones.

**Status**: Spec phase
**References**: [#1] Patrikalakis Ch.5 (SSI algorithms for analytic surfaces), [#25] Yang et al. (topology-guaranteed SSI tracing), [#13] Keyser et al. ESOLID (exact arithmetic on quadrics)

---

## Goal

Implement an exact analytical SSI solver for the cone–cone surface pair. This enables boolean operations between conical solids without mesh approximation fallback, per A15.1.

The intersection of two general cones is a degree-4 algebraic space curve. Special cases include coaxial cones (circles), same-apex cones (lines through the shared apex), and identical cones (overlap).

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `apex_a` | `[f64; 3]` | meters | Apex of cone A |
| `axis_a` | `[f64; 3]` | unitless | Unit axis vector of cone A (from apex toward base) |
| `half_angle_a` | `f64` | radians | Half-angle of cone A (0 < α < π/2) |
| `height_range_a` | `(f64, f64)` | meters | (min, max) distance from apex along axis |
| `apex_b` | `[f64; 3]` | meters | Apex of cone B |
| `axis_b` | `[f64; 3]` | unitless | Unit axis vector of cone B |
| `half_angle_b` | `f64` | radians | Half-angle of cone B (0 < β < π/2) |
| `height_range_b` | `(f64, f64)` | meters | (min, max) distance from apex along axis |

### Valid Ranges
- `0 < half_angle < π/2` (proper cones only, not degenerate cylinders or planes)
- `height_range.0 >= 0, height_range.1 > height_range.0`
- Axes must be unit vectors

### Error Conditions
- Half-angle out of range → `KernelError::InvalidInput`
- Zero-length height range → `KernelError::InvalidInput`
- Non-unit axis → normalize internally (with TAU_NORMALIZE check)

---

## Branch Table

| Case | Condition | SSI Result |
|------|-----------|------------|
| **No intersection** | Cones don't intersect within height ranges | Empty vec |
| **Coaxial, same angle** | Same axis, same half-angle, different apex | 0 or 1 circle (where cone surfaces meet) |
| **Coaxial, different angle** | Same axis, different half-angle | 0, 1, or 2 circles |
| **Same apex, different axis** | Apices coincide, axes differ | Lines through shared apex (2 or 4 lines) |
| **Parallel axes** | Axes parallel but offset | Degree-4 curve, sampled as polyline(s) |
| **General position** | No special alignment | Degree-4 curve, sampled as polyline(s) |
| **Tangent** | Cones just touch | Single tangent point/line (empty — below feature size for point) |

---

## Algorithm

### Coaxial case (axes parallel and collinear)

Transform to local frame where shared axis = Z.

Cone A surface at height h from apex: radius = h·tan(α)
Cone B surface at height h from apex: radius = (h - d)·tan(β) (where d is axial separation)

Set equal: `h·tan(α) = (h - d)·tan(β)` → solve for h (linear equation, 0 or 1 solution).

If same half-angle: `h·tan(α) = (h-d)·tan(α)` → 0 = -d·tan(α) → no solution (parallel generators) unless d=0 (coincident).

Check if solution h is within both height ranges. If so, return circle at that height with appropriate radius.

### Same-apex case

Both cones share apex. Any ray from the apex that lies on both cone surfaces is an intersection line.

A ray from apex in direction **d** lies on cone A if: `angle(d, axis_a) = half_angle_a`
Same ray lies on cone B if: `angle(d, axis_b) = half_angle_b`

This defines two circles on the unit sphere (one around each axis). Their intersection gives 0, 2, or 4 directions, each defining a line through the apex.

Solve: `cos(half_angle_a) = d·axis_a` and `cos(half_angle_b) = d·axis_b` simultaneously.

Return SSICurve::Line segments for each solution direction, clipped to height ranges.

### General case

Transform to frame where cone A's apex = origin, cone A's axis = Z.

Cone A implicit: `x² + y² = z²·tan²(α)` (for z > 0)
Cone B implicit: requires transforming cone B into cone A's frame.

The intersection of two degree-2 implicit surfaces yields a degree-4 space curve.

Sample by sweeping azimuthal angle θ around cone A's axis:
- Parameterize cone A: `x = t·tan(α)·cos(θ)`, `y = t·tan(α)·sin(θ)`, `z = t`
- Substitute into cone B's implicit equation → quadratic in t for each θ
- Solve quadratic → 0, 1, or 2 intersection points per θ

Sampling resolution: N = max(32, appropriate for angular extent).

Collect points into connected curves. Return sampled polylines, clipped to height ranges.

---

## Invariants

1. **Point-on-surface**: Every returned curve point lies on both cone surfaces within TAU_MODEL.
2. **Separation**: If cone surfaces don't meet within height ranges (gap > TAU_MODEL), result is empty.
3. **Coaxial symmetry**: Coaxial case must return exact circles, not sampled polylines.
4. **Same-apex lines**: Same-apex case must return exact line segments.
5. **Closure**: General-case curves are closed loops (for full-revolution cones) or open arcs clipped by height bounds.
6. **Determinism**: Same inputs always produce same output.

---

## Oracles

1. **Point-on-surface test**: For each curve point, verify `|angle_to_axis_a - half_angle_a| < TAU_MODEL` AND `|angle_to_axis_b - half_angle_b| < TAU_MODEL`.
2. **Circle validation (coaxial)**: Center on shared axis, all points equidistant from axis, within both height ranges.
3. **Line validation (same-apex)**: All points collinear with apex, on both cone surfaces.
4. **Empty validation**: Verify no intersection when cones are separated.
5. **Quadratic solution count**: For coaxial case with different angles, verify correct number of circles (0, 1, or 2 depending on height range overlap).

---

## Failure Modes

| Condition | Behavior |
|-----------|----------|
| Degenerate cone (half_angle ≈ 0 or ≈ π/2) | Return `KernelError::InvalidInput` |
| Near-tangent | Return empty (tangent points below feature size) |
| Near-coaxial (axes within TAU_NORMALIZE) | Route to coaxial branch |
| Near-coincident apex (distance < TAU_MODEL) | Route to same-apex branch |
| Numerical instability | Return `KernelError::NumericalFailure` with diagnostic |

---

## Research Basis

- **[#1] Patrikalakis Ch.5**: The cone-cone pair is classified as a degree-4 algebraic intersection. The general approach uses implicitization and substitution.
- **[#13] Keyser et al. ESOLID**: Demonstrates exact quadric-quadric intersection; we adapt the algebraic reduction approach with IEEE 754 floating-point.
- **[#25] Yang et al.**: Topology-guaranteed tracing ensures correct curve connectivity for the general case.

### Analytical vs. Approximate Method Justification

- **Method**: Exact (closed-form) for coaxial and same-apex cases; semi-analytical (parameterization + quadratic solve per azimuthal sample) for general case.
- **Justification**: Cone-cone is a quadric pair (A15.1). Mesh approximation is prohibited. Special cases admit exact geometric solutions. The general case parameterizes one cone and solves a quadratic (not a quartic) at each sample because substituting a cone parameterization into another cone's implicit equation reduces to degree 2.
- **Surface pair coverage**: Cone–Cone only.
