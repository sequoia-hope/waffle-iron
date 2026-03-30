# Spec: Analytical Cylinder-Cone SSI Solver

**Status**: Active
**Created**: 2026-03-30
**SSI Matrix Pair**: #7 (Cylinder-Cone)

## 1. Goal

Replace the sampling-based (72×200 grid scan) cylinder-cone SSI general case with an
exact analytical solver that returns parametric `Degree4CylCone` intersection curves.
This eliminates an A15.1 violation and brings pair #7 to full analytical coverage.

The coaxial case (already analytical, returning `Circle`) is unmodified.

## 2. Parameters

| Parameter | Type | Unit | Valid Range | Notes |
|-----------|------|------|-------------|-------|
| cyl_origin | [f64; 3] | m | any | Point on cylinder axis |
| cyl_axis | [f64; 3] | unit | \|v\| = 1 | Cylinder axis direction |
| cyl_radius | f64 | m | > 0 | Cylinder radius |
| cyl_z_min | f64 | m | any | Min axial extent |
| cyl_z_max | f64 | m | > cyl_z_min | Max axial extent |
| cone_apex | [f64; 3] | m | any | Cone apex point |
| cone_axis | [f64; 3] | unit | \|v\| = 1 | Cone axis direction |
| cone_half_angle | f64 | rad | (0, π/2) | Cone half-angle |
| cone_height_range | (f64, f64) | m | min < max | Axial extent from apex |

## 3. Branch Table

| Sub-case | Condition | Method | Output |
|----------|-----------|--------|--------|
| Coaxial | axes collinear, perp_dist < TOL | analytical (existing) | 0-2 `Circle` |
| General, intersecting | Δ(θ) ≥ 0 for some θ range | analytical (new) | 1-2 `Degree4CylCone` |
| General, tangent | Δ(θ) = 0 at single θ | analytical | empty (filtered by MIN_FEATURE_SIZE) |
| General, disjoint | Δ(θ) < 0 ∀θ, or bounding check fails | analytical | empty |
| Degenerate: zero radius/angle | R_c < TOL or tan(α) < TOL | early return | empty |
| Degenerate: Q ≈ 0 | cyl_axis · cone_axis ≈ ±cos(α) | linear solve | 0-1 `Degree4CylCone` |

## 4. Invariants

### I1: On-surface invariant
Every point P on the returned curve must satisfy:
- Distance from P to cylinder surface < TAU_MODEL
- Distance from P to cone surface < TAU_MODEL

### I2: Curve type invariant
General-position (non-coaxial) intersections return `Degree4CylCone` curves, never
`Line` approximations or sampling-based polylines.

### I3: Existing behavior preservation
Coaxial case behavior is identical to current implementation.

### I4: Height-range clipping
All returned curve points lie within both the cylinder z-range and the cone height-range.

### I5: Symmetry
Swapping the role of cylinder and cone (where applicable) produces geometrically
equivalent intersection curves.

## 5. Oracles

### O1: On-surface oracle (primary)
For N uniformly sampled points on each returned curve:
- `|perp_distance_to_cyl_axis - R_c| < TAU_MODEL`
- `|perp_distance_to_cone_axis - h·tan(α)| < TAU_MODEL` where h = axial projection

### O2: Point count oracle
General-position intersections produce at least 2 evaluable points per curve.

### O3: Regression oracle
All existing cylinder_cone_ssi tests continue to pass with equivalent or better results.

## 6. Failure Modes

| Condition | Error / Behavior |
|-----------|-----------------|
| cyl_radius ≤ 0 | Return empty (no error) |
| cone_half_angle ≤ 0 or ≥ π/2 | Return empty (no error) |
| z_max ≤ z_min | Return empty (no error) |
| Numerical instability in quadratic | Clamp discriminant, filter NaN points |

## 7. Research Basis

- **[#1] Patrikalakis et al. Ch.5**: Exact SSI algorithms for quadric surface pairs.
  The cylinder-cone intersection is a degree-4 algebraic curve. The approach of
  parameterizing on one surface and substituting into the other's implicit equation
  yields a tractable quadratic in the remaining parameter (z along cylinder axis).

- **[#25] Yang et al. (2023)**: Topology-guaranteed SSI confirms that cylinder-cone
  intersections are algebraically degree 4 and can be computed exactly via resultant
  methods. Our approach (quadratic in z for each θ) is a direct parameterization
  equivalent.

## 7a. Analytical vs. Approximate Method Justification

- **Method**: Exact (closed-form quadratic solve for each θ)
- **Justification**: Cylinder and cone are both quadric surfaces. Their intersection
  is a degree-4 algebraic curve with a closed-form parametric representation via
  cylinder angle parameterization. No approximation is needed.
- **Surface pair coverage**: Cylinder-Cone only. All sub-cases handled analytically.

## 8. Mathematical Derivation

Point on cylinder: P(θ, z) = O + z·a_c + R·(cosθ·u + sinθ·v)
where O = cyl_origin, a_c = cyl_axis, u, v = cross-section basis.

Cone implicit: |P - apex - h·a_k|² = h²·tan²α, where h = (P - apex)·a_k

Let A = O - apex, a = a_c · a_k (axis dot product).

Projection h(θ, z) = A·a_k + z·a + R·(cosθ·b_u + sinθ·b_v)
where b_u = u·a_k, b_v = v·a_k.

Expanding the cone condition and collecting z terms:
  (1 - sec²α·a²)·z² + 2·(A·a_c - sec²α·a·H₀)·z + (|A|²+R²+2R·f(θ) - sec²α·H₀²) = 0

where H₀(θ) = A·a_k + R·(cosθ·b_u + sinθ·b_v), f(θ) = cosθ·(A·u) + sinθ·(A·v).

This is a standard quadratic in z with θ-dependent coefficients. Two roots yield
two branches of the intersection curve.
