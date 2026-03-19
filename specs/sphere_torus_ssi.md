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
| **General intersection** | Sphere intersects torus, non-axial | 1 or 2 closed curves (degree-4 algebraic) represented as sampled polylines |
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

### General case

Transform both surfaces to torus-centered frame (torus axis = Z, torus center = origin).

Sphere implicit: `(x-cx)² + (y-cy)² + (z-cz)² = s²`
Torus implicit: `(√(x²+y²) - R)² + z² = r²`
Expanded torus: `x² + y² + z² - 2R√(x²+y²) + R² = r²`

Let `Σ = x² + y² + z²`. Sphere gives `Σ = s² + 2cx·x + 2cy·y + 2cz·z - cx² - cy² - cz² + Σ_correction`.

Subtracting sphere from torus equation eliminates the Σ term, leaving:
`-2R√(x²+y²) + R² - r² = s² - 2cx·x - 2cy·y - 2cz·z + (sphere const terms)`

This is `√(x²+y²) = (linear in x,y,z + constant) / (2R)`.

Squaring: `x² + y² = [(linear in x,y,z + constant)]² / (4R²)`

This is a degree-2 equation in x,y,z — combined with the sphere equation (also degree 2), the system reduces to the intersection of two quadrics, yielding a degree-4 curve.

Sample the curve by sweeping the azimuthal angle θ around the torus axis, solving the resulting 1D equation at each θ to get z values. Return sampled polyline(s).

**Sampling resolution**: N = max(32, ceil(2π·R / TAU_MODEL)) points around the azimuth.

---

## Invariants

1. **Symmetry**: If sphere center is on torus axis, result curves must be exact circles (not sampled polylines).
2. **Containment**: Every returned curve point must lie on both surfaces within TAU_MODEL tolerance.
3. **Separation**: If distance(sphere_boundary, torus_boundary) > TAU_MODEL, result must be empty.
4. **Closure**: Each returned curve must be closed (start ≈ end within TAU_MODEL for polylines, exact for circles).

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

- **Method**: Exact (closed-form) for axial-symmetric case; semi-analytical (algebraic reduction + 1D numerical sweep) for general case.
- **Justification**: The sphere-torus pair is a quadric pair (A15.1). Mesh approximation is prohibited. The axial case admits exact circle solutions. The general case uses algebraic elimination to reduce to a 1D sweep, which is analytical in spirit but uses numerical root-finding at each azimuthal sample.
- **Surface pair coverage**: Sphere–Torus only.
