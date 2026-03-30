# Spec: Plane-Torus General SSI Solver

## Goal

Implement an analytical surface-surface intersection solver for arbitrary
plane-torus configurations. Currently only the perpendicular case (plane
normal ∥ torus axis) is handled; all other orientations return
`KernelError::NotSupported`. This spec covers the general case.

The solver must return exact parametric curves — no sampling or mesh
approximation (per A15.1).

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| plane_origin | [f64; 3] | meters | A point on the plane |
| plane_normal | [f64; 3] | — | Unit normal of the plane |
| torus_center | [f64; 3] | meters | Center of the torus |
| torus_axis | [f64; 3] | — | Unit axis of the torus |
| torus_major_radius | f64 | meters | Major radius R (tube center to torus center) |
| torus_minor_radius | f64 | meters | Minor radius r (tube radius) |

**Preconditions:**
- plane_normal and torus_axis are unit vectors
- R > 0, r > 0
- R and r are above MIN_FEATURE_SIZE (1e-6)

## Mathematical Foundation

### Torus parameterization

```
P(θ, φ) = C + (R + r·cos φ)·(cos θ·u + sin θ·v) + r·sin φ·a
```
where u, v are an orthonormal basis perpendicular to axis a.

### Plane constraint

n · P = n · plane_origin

### Derivation

Substituting P into the plane equation:

```
n · C + (R + r·cos φ)·(n_u·cos θ + n_v·sin θ) + r·n_a·sin φ = D
```

where:
- n_u = n · u, n_v = n · v, n_a = n · a
- D = n · plane_origin

Let A(θ) = n_u·cos θ + n_v·sin θ and d' = D - n · C.

Then: R·A(θ) + r·A(θ)·cos φ + r·n_a·sin φ = d'

This is a harmonic equation in φ:
```
r·A(θ)·cos φ + r·n_a·sin φ = d' - R·A(θ)
```

Of the form p·cos φ + q·sin φ = c where:
- p = r·A(θ)
- q = r·n_a
- c = d' - R·A(θ)

**Solution:** When p² + q² ≥ c²:
```
φ₀ = atan2(q, p)
Δφ = acos(c / √(p² + q²))
φ = φ₀ ± Δφ
```

This yields 0 or 2 solutions per θ (1 at tangent points).

### Valid θ range

The discriminant condition p² + q² ≥ c² expands to:

```
r²·A(θ)² + r²·n_a² ≥ (d' - R·A(θ))²
```

Since A(θ) = n_perp·cos(θ - θ₀) where n_perp = √(n_u² + n_v²), this is
a trigonometric inequality in θ with at most 2 zero-crossings, giving a
single contiguous valid interval (or the full circle, or empty).

## Branch Table

| Sub-case | Condition | Result | Curve type |
|----------|-----------|--------|------------|
| Perpendicular (n ∥ a) | n_perp < TAU_PARALLEL | 0–2 circles | SSICurve::Circle (existing) |
| Oblique, intersecting | n_perp ≥ TAU_PARALLEL, valid θ range non-empty | 2 branches | SSICurve::Degree4PlaneTorus |
| Oblique, tangent | discriminant touches zero at exactly 1 θ range point | 1 degenerate curve | Filtered by MIN_FEATURE_SIZE extent |
| Oblique, disjoint | discriminant negative for all θ | Empty | Vec::new() |
| Through center (d' = 0) | plane passes through torus center | 2 branches (may be Villarceau circles) | SSICurve::Degree4PlaneTorus |
| Spindle torus (r > R) | minor radius exceeds major | Same algorithm applies | SSICurve::Degree4PlaneTorus |

## Invariants

1. **On-surface (plane):** For any point P on a returned curve, |n · (P - plane_origin)| < TAU_MODEL
2. **On-surface (torus):** For any point P, |dist_to_tube_center(P) - r| < TAU_MODEL
   where dist_to_tube_center = √((ρ - R)² + h²), ρ = radial distance from axis, h = axial offset
3. **Continuity:** Each branch is C∞ (smooth) over its θ range
4. **Completeness:** Every point on the plane-torus intersection is within TAU_MODEL of some returned curve
5. **No approximation:** Returned curves are exact closed-form — no Line approximations

## Oracles

1. **Point-on-plane oracle:** Sample 100 points per curve at uniform θ intervals.
   Assert |n · (P - plane_origin)| < TAU_MODEL for each.
2. **Point-on-torus oracle:** For each sampled point, compute torus signed distance.
   Assert |torus_signed_distance(P)| < TAU_MODEL.
3. **No-Line oracle:** Assert no SSICurve::Line variants in results (assert_no_line_approximations).
4. **Perpendicular regression:** Existing perpendicular tests must still pass unchanged.

## Failure Modes

| Condition | Expected behavior |
|-----------|------------------|
| Zero-length plane normal | Caller responsibility (precondition) |
| Zero-length torus axis | Caller responsibility (precondition) |
| r ≤ 0 or R ≤ 0 | Caller responsibility (precondition) |
| Degenerate torus (r = 0) | Returns empty (no surface) |
| Near-perpendicular (borderline) | Smooth transition: as n_perp → 0, degree-4 curves approach circles |

## Research Basis

- [#1] Patrikalakis et al. Ch.5 — SSI algorithms for all quadric pairs, including
  torus intersections. The plane-torus intersection yields a spiric section (degree-4
  algebraic curve). Our parametric approach follows the θ-parameterization technique.
- [#25] Yang et al. (2023) — Topology-guaranteed SSI. Our approach guarantees correct
  topology by solving the harmonic equation exactly.
- Classical result: The plane-torus intersection is a quartic curve known as a spiric
  section of Perseus. Special cases include Villarceau circles (oblique plane tangent
  to inner hole) and Cassini ovals (in 2D projection).

### Analytical vs. Approximate Method Justification

- **Method:** Exact (closed-form SSI via harmonic equation in φ)
- **Justification:** The harmonic equation p·cos φ + q·sin φ = c has an exact
  analytical solution via atan2/acos. No numerical iteration or sampling needed.
- **Surface pair coverage:** Plane-Torus only. Plane is degree-1, torus is degree-4.
  The intersection is algebraically degree-4. Our parametric representation captures
  this exactly.
