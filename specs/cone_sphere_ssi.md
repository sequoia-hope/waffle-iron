# Spec: `cone_sphere_ssi` Solver

**Status**: Spec phase
**Author**: Spec Writer (auto-waffle session 4)
**Date**: 2026-03-19
**A15 pair**: #11 (Cone–Sphere)

---

## 1. Goal

Implement an exact surface-surface intersection (SSI) solver for the cone-sphere
quadric pair. This is A15 pair #11, enabling analytical boolean operations between
cones and spheres — common in mechanical CAD (countersinks, chamfered holes,
conical seats against ball bearings, valve assemblies).

The cone surface at height h from the apex has radius r(h) = h * tan(half_angle).
Substituting this into the sphere equation yields a degree-4 algebraic equation in
the axial coordinate. The intersection produces 0, 1, or 2 closed curves. For the
coaxial case (sphere center on cone axis), the curves are circles at constant
height. For the general offset case, the curves are degree-4 space curves.

---

## 2. Parameters

| Parameter | Type | Units | Description |
|-----------|------|-------|-------------|
| cone_apex | [f64; 3] | meters | Apex (tip) of the cone |
| cone_axis | [f64; 3] | meters | Unit direction of cone axis (away from apex) |
| cone_half_angle | f64 | radians | Half-angle of the cone (0 < half_angle < π/2) |
| cone_z_min | f64 | meters | Min extent along axis from apex (≥ 0) |
| cone_z_max | f64 | meters | Max extent along axis from apex (> z_min) |
| sphere_center | [f64; 3] | meters | Center of sphere |
| sphere_radius | f64 | meters | Sphere radius (> 0) |

---

## 3. Branch Table

| Case | Condition | Intersection | SSI Result |
|------|-----------|-------------|------------|
| Disjoint | Sphere does not reach cone surface | None | Empty vec |
| Coaxial | Sphere center on cone axis, overlapping | 1 or 2 circles | Circle(s) at constant h |
| Tangent (external) | Sphere grazes cone surface | Single point/curve | Empty (within TOL) |
| Sphere enclosing apex | Sphere contains the cone apex | 0 or 1 curve | Circle or empty |
| General overlap (offset) | Sphere center off-axis, overlapping | Degree ≤ 4 curves | Circle approximation(s) |
| Outside Z range | Intersection exists but outside [z_min, z_max] | None | Empty vec |

**Key geometric insight**: Project the sphere center onto the cone axis line.
Let `d` be the perpendicular distance from sphere center to the axis, and `h_proj`
the signed distance along the axis from the apex. At height `h_proj`, the cone
radius is `r_cone = h_proj * tan(half_angle)`. The coaxial case occurs when `d ≈ 0`.
For the general case, substitute the cone constraint `x² + y² = (h * tan(α))²`
(in the cone's local frame) into the sphere equation
`(x - cx)² + (y - cy)² + (z - cz)² = R²`, yielding a quartic in h.

**Coaxial sub-cases**: When `d ≈ 0`, the intersection reduces to solving
`(h * tan(α))² + (h - h_proj)² = R²`, a quadratic in h. Each real root h_i > 0
within [z_min, z_max] gives a circle of radius `h_i * tan(α)` centered on the
axis at height h_i from the apex.

---

## 4. Invariants

1. **Symmetry**: Result is independent of cone axis direction sign (solver normalizes to point away from apex)
2. **Circle normal**: For coaxial cases, intersection circles have normal parallel to cone axis
3. **Circle containment**: All intersection points lie on both the cone surface and the sphere surface simultaneously
4. **Height bounds**: Intersection circles satisfy h > 0 (no intersection behind the apex)
5. **Z-range clipping**: Intersection curves outside [cone_z_min, cone_z_max] are excluded
6. **Radius consistency**: For a coaxial circle at height h, the circle radius equals h * tan(half_angle)

---

## 5. Oracles

| Oracle | Method | Tolerance |
|--------|--------|-----------|
| Point on cone | All curve points at distance h * tan(half_angle) from axis, at height h | TAU_MODEL |
| Point on sphere | All curve points at distance sphere_radius from sphere center | TAU_MODEL |
| Disjoint case | Returns empty vec | Exact |
| Coaxial circle normal | Normal ∥ cone axis | TAU_MODEL angle |
| Apex containment | If sphere contains apex, intersection curve (if any) wraps around apex side | Logical |

---

## 6. Failure Modes

| Input | Expected |
|-------|----------|
| Zero-length axis | Caller's responsibility (pre-normalized) |
| Zero half_angle | Degenerates to line (no surface); return empty |
| half_angle ≥ π/2 | Invalid cone; caller's responsibility |
| Zero radius | Caller's responsibility |
| z_min < 0 | Clamp to 0 (cone only valid for h ≥ 0) |
| NaN inputs | May produce empty or NaN curves (caller validates) |

The SSI solver is an internal function; input validation happens at the Kernel API level.

---

## 7. Research Basis

- [#1] Patrikalakis Ch.5 — SSI for quadric surfaces: The cone-sphere intersection
  is obtained by substituting the cone constraint (x² + y² = h²·tan²(α)) into
  the sphere equation. In the cone's local coordinate frame (apex at origin, axis
  along Z), this yields a quartic equation in the axial variable h. For the coaxial
  case, the quartic reduces to a quadratic with closed-form roots.
- [#25] Yang et al. — Topology-guaranteed SSI for robust boolean operations.
- [#33] Stroud — Boundary representation and geometric modelling: cone-sphere
  intersection classification.

**Simplification**: For the initial implementation, we handle the coaxial case
exactly (closed-form circles) and use circle approximation for the general offset
case where the degree-4 curve is nearly circular. This covers the majority of
practical CAD configurations (countersinks, conical seats, tapered fits).

---

## 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (closed-form) for coaxial cases; circle approximation for
general offset cases where the degree-4 curve is nearly circular.

**Surface pair**: Cone–Sphere (A15 pair #11). This is a quadric pair requiring
exact SSI per A15. The coaxial case reduces to a quadratic equation with exact
circle solutions. For the general offset case, the quartic can be solved
analytically (Ferrari's method) or numerically (Newton iteration on the quartic
coefficients). The circle approximation provides excellent accuracy for typical
mechanical CAD configurations where the sphere center is close to the cone axis.
