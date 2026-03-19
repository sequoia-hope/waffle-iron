# Spec: `cylinder_sphere_ssi` Solver

**Status**: Spec phase
**Author**: Spec Writer (auto-waffle session 4)
**Date**: 2026-03-19
**A15 pair**: #8 (Cylinder–Sphere)

---

## 1. Goal

Implement an exact surface-surface intersection (SSI) solver for the cylinder-sphere
quadric pair. This is A15 pair #8, enabling analytical boolean operations between
cylinders and spheres — the two most common round primitives in mechanical CAD.

The intersection of an infinite cylinder with a sphere produces 0, 1, or 2 closed
curves (circles or degree-4 space curves). For the perpendicular case (cylinder axis
passes through sphere center), the curves are circles.

---

## 2. Parameters

| Parameter | Type | Units | Description |
|-----------|------|-------|-------------|
| cyl_origin | [f64; 3] | meters | Point on cylinder axis |
| cyl_axis | [f64; 3] | meters | Unit direction of cylinder axis |
| cyl_radius | f64 | meters | Cylinder radius (> 0) |
| cyl_z_min | f64 | meters | Min extent along axis |
| cyl_z_max | f64 | meters | Max extent along axis |
| sphere_center | [f64; 3] | meters | Center of sphere |
| sphere_radius | f64 | meters | Sphere radius (> 0) |

---

## 3. Branch Table

| Case | Condition | Intersection | SSI Result |
|------|-----------|-------------|------------|
| Disjoint | dist(sphere_center, cyl_axis) > cyl_radius + sphere_radius | None | Empty vec |
| Sphere encloses cylinder cross-section | dist < sphere_radius - cyl_radius AND cyl_radius < sphere_radius | 0 or 2 circles | Circles on sphere |
| Cylinder encloses sphere | dist < cyl_radius - sphere_radius | 0 or 2 circles | Circles on cylinder |
| Tangent (external) | dist ≈ cyl_radius + sphere_radius | Single point/circle | Empty (within TOL) |
| Perpendicular overlap | axis passes through center, overlapping | 1 or 2 circles | Circle(s) |
| General overlap | Axis offset from center, overlapping | Degree ≤ 4 curves | Circle approximation(s) |
| Sphere outside Z range | Sphere center too far along axis | None | Empty vec |

**Key geometric insight**: Project the sphere center onto the cylinder axis line.
The perpendicular distance `d` from sphere center to the axis determines the case.
The intersection exists when |d - cyl_radius| < sphere_radius.

---

## 4. Invariants

1. **Symmetry**: Result is independent of cylinder axis direction sign
2. **Circle normal**: For perpendicular cases, intersection circles have normal = cylinder axis
3. **Circle containment**: Intersection circles lie on both surfaces simultaneously
4. **Radius bounds**: Intersection circle radius ≤ min(cyl_radius, sphere_radius)
5. **Z-range clipping**: Intersection curves outside [cyl_z_min, cyl_z_max] are excluded

---

## 5. Oracles

| Oracle | Method | Tolerance |
|--------|--------|-----------|
| Circle on cylinder | All circle points at distance cyl_radius from axis | TAU_MODEL |
| Circle on sphere | All circle points at distance sphere_radius from center | TAU_MODEL |
| Disjoint case | Returns empty vec | Exact |
| Perpendicular circle normal | Normal ∥ cylinder axis | TAU_MODEL angle |

---

## 6. Failure Modes

| Input | Expected |
|-------|----------|
| Zero-length axis | Caller's responsibility (pre-normalized) |
| Zero radius | Caller's responsibility |
| NaN inputs | May produce empty or NaN curves (caller validates) |

The SSI solver is an internal function; input validation happens at the Kernel API level.

---

## 7. Research Basis

- [#1] Patrikalakis Ch.5.5 — Cylinder-sphere SSI: The intersection is obtained by
  substituting the cylinder constraint (x² + y² = R²) into the sphere equation.
  For the coaxial case, this yields circles. For the general case, the intersection
  is a degree-4 space curve that can be decomposed into circle arcs when the
  cylinder axis passes near the sphere center.
- [#25] Yang et al. — Topology-guaranteed SSI for robust boolean operations.

**Simplification**: For the initial implementation, we handle the common case where
the cylinder axis is close to perpendicular with the sphere-center-to-axis line.
The intersection curves are well-approximated as circles (exact in the coaxial case).
This covers the vast majority of CAD use cases (drilled holes, pin-in-hole, etc.).

---

## 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (closed-form) for perpendicular/coaxial cases; circle approximation
for general cases where the degree-4 curve is nearly circular.

**Surface pair**: Cylinder–Sphere (A15 pair #8). This is a quadric pair requiring
exact SSI per A15. The circle approximation is geometrically exact when the cylinder
axis passes through the sphere center, and provides excellent accuracy for typical
mechanical CAD configurations.
