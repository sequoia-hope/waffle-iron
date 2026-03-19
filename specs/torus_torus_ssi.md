# Torus–Torus SSI Solver

**A15 pair #15** — Surface-surface intersection for two tori.

**Status**: Spec phase
**References**: [#1] Patrikalakis Ch.5 (SSI algorithms for analytic surfaces), [#13] Keyser et al. ESOLID

---

## Goal

Implement an analytical SSI solver for the torus–torus surface pair. This is the final A15 quadric pair, completing 15/15 coverage for analytical primacy.

The intersection of two tori is a degree-8 algebraic space curve in general. Special cases (coaxial) reduce to exact circles.

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `torus_a_center` | `[f64; 3]` | meters | Center of torus A |
| `torus_a_axis` | `[f64; 3]` | unitless | Unit axis vector of torus A |
| `torus_a_major_radius` | `f64` | meters | Major radius R_A (> 0) |
| `torus_a_minor_radius` | `f64` | meters | Minor (tube) radius r_A (> 0) |
| `torus_b_center` | `[f64; 3]` | meters | Center of torus B |
| `torus_b_axis` | `[f64; 3]` | unitless | Unit axis vector of torus B |
| `torus_b_major_radius` | `f64` | meters | Major radius R_B (> 0) |
| `torus_b_minor_radius` | `f64` | meters | Minor (tube) radius r_B (> 0) |

### Valid Ranges
- All radii > 0
- Axes must be unit vectors

### Error Conditions
- Zero or negative radius → `KernelError::InvalidInput`

---

## Branch Table

| Case | Condition | SSI Result |
|------|-----------|------------|
| **No intersection** | Tori don't overlap | Empty vec |
| **Coaxial, same center** | Same axis, same center | 0, 1, 2, 3, or 4 circles |
| **Coaxial, offset center** | Same axis, centers offset along axis | 0–4 circles |
| **General position** | Arbitrary orientation | Degree-8 curve → sampled Line segment(s) |
| **Tangent** | Tori just touch | Empty (below feature size) |
| **Disjoint (bounding sphere)** | Too far apart | Empty (fast reject) |

---

## Algorithm

### Coaxial case (axes parallel and collinear)

Both tori share the same axis. Transform to frame where axis = Z.

Torus A: (ρ - R_A)² + z_A² = r_A²  where z_A = z - d_A (d_A = axial offset of torus A center)
Torus B: (ρ - R_B)² + z_B² = r_B²  where z_B = z - d_B (d_B = axial offset of torus B center)

At intersection: a point has the same (ρ, z) and satisfies both equations.

Subtract equations:
(ρ - R_A)² + (z - d_A)² - (ρ - R_B)² - (z - d_B)² = r_A² - r_B²

Expand:
ρ² - 2R_Aρ + R_A² + z² - 2d_Az + d_A² - ρ² + 2R_Bρ - R_B² - z² + 2d_Bz - d_B² = r_A² - r_B²

Simplify:
2(R_B - R_A)ρ + (R_A² - R_B²) + 2(d_B - d_A)z + (d_A² - d_B²) = r_A² - r_B²

This is linear in ρ and z: Aρ + Bz = C, giving ρ as a function of z (or vice versa).

Substitute back into one torus equation to get a quadratic in z. Solve for z, then compute ρ. For each valid (ρ > 0, z) pair, return a circle.

Special case: R_A = R_B and d_A = d_B (concentric tori) — equation reduces to r_A² = r_B². Equal minor radii → overlap (identical tori), different → disjoint or solve a simpler equation.

### General case (non-coaxial)

1. **Bounding sphere fast reject**: Each torus bounded by sphere of radius R + r centered at its center. If bounding spheres are disjoint, return empty.
2. **Numerical scanning**: Sample torus A surface at (θ, φ) grid. For each sample point, compute signed distance to torus B surface using `torus_signed_distance()`. Track sign changes.
3. **Curve extent**: Return Line segment(s) from found intersection points.

Sampling resolution: n_theta = 360, n_phi = 36.

---

## Invariants

1. All returned curve points lie on both torus surfaces within TAU_MODEL.
2. Circle normals for coaxial case are aligned with the shared axis.
3. Coaxial case returns at most 4 circles.
4. Empty result for tangent touches below MIN_FEATURE_SIZE.
5. Determinism: same inputs → same output.

---

## Oracles

- **Circle radius** in coaxial case satisfies both torus equations
- **Circle z-height** satisfies both (ρ - R_i)² + (z - d_i)² = r_i²
- **Point-on-surface**: every result point satisfies both surface equations
- **Torus A check**: (ρ_A - R_A)² + z_A² = r_A²
- **Torus B check**: (ρ_B - R_B)² + z_B² = r_B²

---

## Failure Modes

| Condition | Behavior |
|-----------|----------|
| Zero or negative radius | Return `KernelError::InvalidInput` |
| Near-tangent | Return empty (below feature size) |
| Near-coaxial | Route to coaxial branch |
| Identical tori (same params) | Return empty (overlap, not intersection curve) |

---

## Research Basis

- **[#1] Patrikalakis Ch.5**: Torus-torus is degree ≤ 8 algebraic curve.
- **[#13] Keyser et al. ESOLID**: Exact quadric-quadric intersection.
- Coaxial case reduces to quadratic via equation subtraction (exact circles).
- General case: numerical scanning (consistent with all other torus pair solvers).

### Analytical vs. Approximate Method Justification

- **Method**: Exact (closed-form) for coaxial case; semi-analytical (numerical scanning) for general case.
- **Justification**: Torus-torus is a quadric pair (A15.1). Mesh approximation prohibited. Coaxial case has exact circle solutions. General case uses proven numerical scanning approach.
- **Surface pair coverage**: Torus–Torus only (final A15 pair).
