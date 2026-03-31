# Sphere–Torus SSI Solver

**A15 pair #14** — Surface-surface intersection for sphere and torus.

**Status**: Spec phase
**References**: [#1] Patrikalakis Ch.5 (SSI algorithms for analytic surfaces), [#25] Yang et al. (topology-guaranteed SSI tracing), [#13] Keyser et al. ESOLID (exact arithmetic on quadrics)

---

## Goal

Implement an exact analytical SSI solver for the sphere–torus surface pair. This enables boolean operations between spherical and toroidal solids without mesh approximation fallback, per A15.1.

The intersection of a sphere with a torus is a degree-4 algebraic space curve. In symmetric configurations (sphere center on torus axis), the curve degenerates to one or two circles.

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `sphere_center` | `[f64; 3]` | meters | Center of the sphere |
| `sphere_radius` | `f64` | meters | Radius of the sphere (> 0) |
| `torus_center` | `[f64; 3]` | meters | Center of the torus (center of the generating circle's path) |
| `torus_axis` | `[f64; 3]` | unitless | Unit axis vector of the torus |
| `torus_major_radius` | `f64` | meters | Major radius R (distance from center to tube center, > 0) |
| `torus_minor_radius` | `f64` | meters | Minor radius r (tube radius, > 0, r < R for ring torus) |

### Valid Ranges
- `sphere_radius > MIN_FEATURE_SIZE`
- `torus_major_radius > MIN_FEATURE_SIZE`
- `torus_minor_radius > MIN_FEATURE_SIZE`
- `torus_minor_radius < torus_major_radius` (ring torus only; horn/spindle torus deferred)

### Error Conditions
- Zero or negative radii → `KernelError::InvalidInput`
- Non-unit axis → normalize internally (with TAU_NORMALIZE check)

---

## Branch Table

| Case | Condition | SSI Result |
|------|-----------|------------|
| **No intersection** | Distance between surfaces > TOL | Empty vec |
| **Tangent (external)** | Sphere just touches torus outer surface | Single tangent point (empty — below feature size) |
| **Tangent (internal)** | Sphere just touches torus inner surface | Single tangent point (empty — below feature size) |
| **Axial symmetric** | Sphere center lies on torus axis | 1 or 2 circles (axis of symmetry reduces quartic to quadratic in r²) |
| **General intersection** | Sphere intersects torus, non-axial | 1 or 2 `Degree4SphereTorus` parametric curves (harmonic equation in φ at each θ) |
| **Sphere encloses torus** | Sphere fully contains torus | Empty vec (no surface intersection) |
| **Torus encloses sphere** | Torus tube fully contains sphere | Empty vec |

---

## Algorithm

### Axial-symmetric case (sphere center on torus axis)

Transform to local frame where torus axis = Z and torus center = origin.

The torus equation in cylindrical coordinates: `(ρ - R)² + z² = r²`
The sphere equation: `ρ² + (z - h)² = s²` (where h = axial offset of sphere center, s = sphere radius)

Substituting and eliminating: solve for z values where both equations hold simultaneously.
This yields a quadratic in z², giving 0, 1, or 2 circles.

Each circle: center on the torus axis, normal = torus axis, radius from cylindrical ρ.

### General case (Degree4SphereTorus parametric curve)

Use the torus parameterization (θ = azimuthal, φ = poloidal):

```
P(θ, φ) = center + (R + r·cos φ)·(cos θ·u + sin θ·v) + r·sin φ·axis
```

where `u, v` are orthonormal vectors in the torus equatorial plane.

Transform sphere center to torus frame: let `d_u, d_v, d_a` be the projections of
`(sphere_center - torus_center)` onto `u, v, axis` respectively.

Substituting the torus parameterization into the sphere equation `|P - S|² = s²`
and simplifying yields a **harmonic equation** in φ at each θ:

```
p(θ)·cos φ + q·sin φ = c(θ)
```

where:
- `D(θ) = d_u·cos θ + d_v·sin θ` (projection of sphere center onto torus equatorial direction at angle θ)
- `p(θ) = 2r·(R - D(θ))` (cosine coefficient — depends on θ)
- `q = -2r·d_a` (sine coefficient — constant w.r.t. θ)
- `c(θ) = s² - R² - r² - |d|² + 2R·D(θ)` (right-hand side — depends on θ)
- `|d|² = d_u² + d_v² + d_a²`

This is the **same harmonic form** as the plane-torus solver (`Degree4PlaneTorus`).
Solution: `φ = atan2(q, p(θ)) ± acos(c(θ) / √(p(θ)² + q²))`.

The valid θ range is where the discriminant `p(θ)² + q² - c(θ)² ≥ 0`.

Two branches (± sign on the acos) produce two separate degree-4 parametric curves,
stored as `SSICurve::Degree4SphereTorus`. The `evaluate_degree4` method evaluates
at any θ within the valid range to produce a world-space point.

**Derivation**: Expanding `|P(θ,φ) - S|² = s²` with the torus parameterization:

```
(R + r·cos φ)² - 2(R + r·cos φ)·D(θ) + D(θ)² + (d_a - r·sin φ)²
  + (sphere center perp component)² = s²  [after grouping]
```

Using `(R + r·cos φ)² + r²·sin²φ = R² + 2Rr·cos φ + r²` and collecting terms
yields the harmonic equation above.

**Precomputed constants** stored in `Degree4SphereTorus`:
- Torus geometry: `torus_center, torus_axis, R, r, u_dir, v_dir`
- Sphere center projections: `d_u, d_v, d_a`
- Derived constant: `k = s² - R² - r² - (d_u² + d_v² + d_a²)` (θ-independent part of c)
- Valid θ range: `(theta_min, theta_max)` where discriminant ≥ 0
- Branch sign: `+1.0` or `-1.0`

---

## Invariants

1. **Symmetry**: If sphere center is on torus axis, result curves must be exact circles (not sampled polylines).
2. **Containment**: Every returned curve point must lie on both surfaces within TAU_MODEL tolerance.
3. **Separation**: If distance(sphere_boundary, torus_boundary) > TAU_MODEL, result must be empty.
4. **Closure**: Each returned curve must be closed (parametric curves are periodic in θ, exact for circles).

---

## Oracles

1. **Point-on-surface test**: For each returned SSICurve point, verify `|dist_to_sphere - sphere_radius| < TAU_MODEL` AND `|dist_to_torus_surface| < TAU_MODEL`.
2. **Circle validation (axial case)**: Verify circle center lies on torus axis, circle normal = torus axis, all circle points at distance sphere_radius from sphere center.
3. **Empty cases**: Verify no intersection when sphere is fully inside torus tube or fully outside.
4. **Curve count**: Axial case with sphere straddling torus tube → exactly 2 circles.

---

## Failure Modes

| Condition | Behavior |
|-----------|----------|
| Degenerate torus (r ≥ R) | Return `KernelError::InvalidInput` |
| Near-tangent (gap < TAU_MODEL) | Return empty (tangent points below feature size) |
| Numerical instability in quartic | Return `KernelError::NumericalFailure` with diagnostic |

---

## Research Basis

- **[#1] Patrikalakis Ch.5**: Provides the general SSI framework for analytic surfaces. The sphere-torus pair is classified as a degree-4 intersection.
- **[#25] Yang et al.**: Topology-guaranteed tracing for SSI curves — relevant for the general case sampling strategy.
- **[#13] Keyser et al. ESOLID**: Demonstrates exact computation on quadric surfaces; we use the algebraic reduction approach but with floating-point arithmetic (exact arithmetic deferred to future work).

### Analytical vs. Approximate Method Justification

- **Method**: Exact (closed-form) for axial-symmetric case; analytical parametric (`Degree4SphereTorus`) for general case.
- **Justification**: The sphere-torus pair is a quadric pair (A15.1). Mesh approximation is prohibited. The axial case admits exact circle solutions. The general case uses algebraic elimination to reduce the sphere-torus system to a harmonic equation `p(θ)·cos φ + q·sin φ = c(θ)` — the same form used by the completed plane-torus solver. The result is stored as a parametric curve type with exact evaluation at any parameter value.
- **Surface pair coverage**: Sphere–Torus only.
