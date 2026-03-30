# Cylinder-Sphere Offset SSI — Analytical Solver

## Goal

Replace the Line approximation in the cylinder-sphere offset (non-coaxial) SSI
path with an exact parametric degree-4 curve. This completes SSI pair #8 in the
A15.4 implementation sequence, making it fully analytical.

## Parameters

| Parameter | Type | Units | Description |
|-----------|------|-------|-------------|
| cyl_origin | [f64; 3] | meters | Point on cylinder axis |
| cyl_axis | [f64; 3] | unit vector | Cylinder axis direction |
| cyl_radius | f64 | meters | Cylinder radius R_c > 0 |
| sph_center | [f64; 3] | meters | Sphere center |
| sph_radius | f64 | meters | Sphere radius R_s > 0 |

Derived:
- `d` = perpendicular distance from sphere center to cylinder axis
- `φ₀` = azimuthal angle of sphere center projection onto cylinder cross-section
- `c` = axial projection of sphere center onto cylinder axis
- `radicand(θ)` = R_s² − d² − R_c² + 2·R_c·d·cos(θ − φ₀)

## Branch Table

| Branch | Condition | Result |
|--------|-----------|--------|
| Coaxial | d < TAU_MODEL | Delegate to existing coaxial path (circles) |
| Disjoint | d > R_s + R_c | Empty (no intersection) |
| Enclosed-disjoint | d + R_c < R_s *and* axial range disjoint | Empty |
| External tangent | max(radicand) ≈ 0 | Empty (within tolerance) |
| Single loop | radicand(θ) > 0 for all θ in [0, 2π) | Two Degree4CylSphere curves (upper/lower), each spanning full 2π |
| Two arcs | radicand has two zeros θ₁, θ₂ | Two Degree4CylSphere curves, each spanning [θ₁, θ₂] |

## Invariants

1. **On-surface (cylinder)**: Every point P on the curve satisfies
   `|‖P_perp‖ − R_c| < TAU_MODEL` where P_perp is the perpendicular component
   relative to the cylinder axis.

2. **On-surface (sphere)**: Every point P on the curve satisfies
   `|‖P − sph_center‖ − R_s| < TAU_MODEL`.

3. **Parametric form**: `z(θ) = c ± √(R_s² − d² − R_c² + 2·R_c·d·cos(θ − φ₀))`
   where θ parameterizes the cylinder azimuth.

4. **Continuity**: The curve is C∞ everywhere except at radicand zeros (where
   upper and lower branches meet, C⁰ only).

5. **Symmetry**: The curve is symmetric about the plane containing the cylinder
   axis and the sphere center.

## Oracles

1. **Point-on-surface oracle**: Sample N points on each returned curve at evenly
   spaced θ values. Verify each point lies on both surfaces within TAU_MODEL.

2. **Volume oracle**: For boolean operations using this solver, verify the result
   volume matches analytical prediction (cylinder cap area × height ± sphere cap
   contribution).

3. **Curve count oracle**: Verify the number of returned curves matches the
   geometric configuration (0 for disjoint, 2 for intersecting).

4. **θ-range oracle**: Verify the parametric range covers exactly the region
   where the radicand is non-negative.

## Failure Modes

| Condition | Error |
|-----------|-------|
| R_c ≤ 0 or R_s ≤ 0 | Invalid input (precondition) |
| d < TAU_MODEL | Delegate to coaxial path (not an error) |
| Surfaces disjoint | Return empty Vec (not an error) |

## Research Basis

- [#1] Patrikalakis et al. Ch.5 — SSI via surface parameterization on quadrics
- [#25] Yang et al. (2023) — topology-guaranteed SSI, Dixon resultant for
  degenerate detection
- [#27] Li et al. (2026) — hybrid SSI architecture; recommends analytical
  parameterization for cylinder-sphere pairs

The parametric form z(θ) = c ± √(f(θ)) follows directly from substituting the
cylinder parameterization into the sphere implicit equation, a standard technique
from [#1] Ch.5.2.

## Analytical vs. Approximate Justification

- **Method**: Exact (closed-form parametric SSI)
- **Surface pairs**: Cylinder-Sphere (both quadric, Tier-1 analytic)
- **A15 compliance**: Full. No mesh fallback, no sampling.
