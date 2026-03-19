# Cone–Torus SSI Solver

**A15 pair #13** — Surface-surface intersection for cone and torus.

**Status**: Spec phase
**References**: [#1] Patrikalakis Ch.5 (SSI algorithms for analytic surfaces), [#13] Keyser et al. ESOLID

---

## Goal

Implement an analytical SSI solver for the cone–torus surface pair. This enables boolean operations between conical and toroidal solids without mesh approximation fallback, per A15.1.

The intersection of a cone and a torus is a degree-8 algebraic space curve in general. Special cases (coaxial) reduce to exact circles.

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `cone_apex` | `[f64; 3]` | meters | Apex of cone |
| `cone_axis` | `[f64; 3]` | unitless | Unit axis vector of cone (from apex toward base) |
| `cone_half_angle` | `f64` | radians | Half-angle of cone (0 < α < π/2) |
| `cone_height_range` | `(f64, f64)` | meters | (min, max) distance from apex along axis |
| `torus_center` | `[f64; 3]` | meters | Center of torus |
| `torus_axis` | `[f64; 3]` | unitless | Unit axis vector of torus |
| `torus_major_radius` | `f64` | meters | Major radius R (> 0) |
| `torus_minor_radius` | `f64` | meters | Minor (tube) radius r (> 0, r < R for ring torus) |

### Valid Ranges
- `0 < cone_half_angle < π/2`
- `cone_height_range.0 >= 0, cone_height_range.1 > cone_height_range.0`
- `torus_major_radius > 0`, `torus_minor_radius > 0`

### Error Conditions
- Half-angle out of range → `KernelError::InvalidInput`
- Zero-length height range → `KernelError::InvalidInput`

---

## Branch Table

| Case | Condition | SSI Result |
|------|-----------|------------|
| **No intersection** | Surfaces don't overlap | Empty vec |
| **Coaxial** | Cone axis = torus axis, cone apex on axis | 0, 1, or 2 circles |
| **General position** | Arbitrary orientation | Degree-8 curve → sampled Line segment(s) |
| **Tangent** | Surfaces just touch | Empty (below feature size) |
| **Disjoint (bounding sphere)** | Too far apart | Empty (fast reject) |

---

## Algorithm

### Coaxial case (cone axis = torus axis, cone apex on torus axis)

Transform to local frame where shared axis = Z, torus center at origin.

Cone surface at height h from apex: radius ρ = h·tan(α)
Torus surface: (ρ - R)² + z² = r²

Setting cone radius = torus radial distance:
(h·tan(α) - R)² + z_torus² = r²

where z_torus is the axial distance from torus center to the point.
If cone apex is at axial offset d from torus center, then z_torus = d + h - 0 (adjusting for frame).

More precisely, a point on the cone at height h from apex is at axial position d + h from torus center (where d is the signed axial offset of cone apex from torus center).
Its radial distance from the axis is h·tan(α).

Substituting into torus equation:
(h·tan(α) - R)² + (d + h)² = r²

Expand: h²·tan²(α) - 2Rh·tan(α) + R² + d² + 2dh + h² = r²
h²·(tan²(α) + 1) + h·(-2R·tan(α) + 2d) + (R² + d² - r²) = 0

This is quadratic in h with:
- a = tan²(α) + 1 = sec²(α)
- b = -2R·tan(α) + 2d
- c = R² + d² - r²

Solve for h. For each valid h (within cone_height_range and h > 0), compute ρ = h·tan(α) and return a circle at that height.

### General case (non-coaxial)

1. **Bounding sphere fast reject**: Cone bounded by sphere around its midpoint. Torus bounded by sphere of radius R + r. If disjoint, return empty.
2. **Numerical scanning**: Sample the cone surface at (θ, h) grid. For each sample, compute signed distance to torus surface using `torus_signed_distance()`. Track sign changes along h-direction for each θ. Interpolate zero-crossings.
3. **Curve extent**: Return Line segment(s) from found points (maximum-extent pair).

Sampling resolution: n_theta = 360, n_h = 200.

---

## Invariants

1. All returned curve points lie on both surfaces within TAU_MODEL.
2. Circle normals for coaxial case are aligned with the shared axis.
3. Coaxial case returns at most 2 circles.
4. Empty result for tangent touches below MIN_FEATURE_SIZE.
5. Determinism: same inputs → same output.

---

## Oracles

- **Circle radius** in coaxial case = h·tan(α) where h is the solution height
- **Circle z-height** satisfies (h·tan(α) - R)² + (d+h)² = r²
- **Point-on-surface**: every result point satisfies both surface equations
- **Cone surface check**: point angle to cone axis = half_angle
- **Torus surface check**: (ρ - R)² + z² = r² where ρ = radial distance from axis

---

## Failure Modes

| Condition | Behavior |
|-----------|----------|
| Degenerate cone (half_angle ≈ 0 or ≈ π/2) | Return `KernelError::InvalidInput` |
| Near-tangent | Return empty (below feature size) |
| Near-coaxial (axes within TAU_COINCIDENT) | Route to coaxial branch |
| Numerical instability | Return empty (conservative) |

---

## Research Basis

- **[#1] Patrikalakis Ch.5**: Cone-torus is degree ≤ 8 algebraic curve.
- **[#13] Keyser et al. ESOLID**: Exact quadric-quadric intersection approach.
- Coaxial case reduces to quadratic in h (exact circles).
- General case: numerical scanning (consistent with cylinder-torus, sphere-torus solvers).

### Analytical vs. Approximate Method Justification

- **Method**: Exact (closed-form) for coaxial case; semi-analytical (parameterization + numerical scanning) for general case.
- **Justification**: Cone-torus is a quadric pair (A15.1). Mesh approximation is prohibited. The coaxial case admits exact circle solutions. The general case uses the same numerical scanning approach as cylinder-torus and sphere-torus solvers.
- **Surface pair coverage**: Cone–Torus only.
