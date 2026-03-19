# Spec: `plane_torus_ssi` Solver

**Status**: Spec phase
**Author**: Spec Writer (auto-waffle session 4)
**Date**: 2026-03-19
**A15 pair**: #6 (Plane-Torus)

---

## 1. Goal

Implement an exact surface-surface intersection (SSI) solver for the plane-torus
quadric pair. This is A15 pair #6, enabling analytical boolean operations between
planes and tori -- common in mechanical CAD (O-ring grooves, toroidal fillets cut
by flat faces, pipe-flange intersections).

The torus with center C, axis A, major radius R, and minor radius r has the
implicit equation (in its local frame with center at origin, axis = Z):

    (x^2 + y^2 + z^2 + R^2 - r^2)^2 = 4 R^2 (x^2 + y^2)

The intersection of a plane with a torus is, in general, a degree-4 algebraic
curve. However, when the plane is perpendicular to the torus axis (i.e., the
plane normal is parallel to the torus axis), the intersection decomposes into
circles that can be computed in closed form.

For this initial implementation, we support:
- **Perpendicular planes** (plane normal parallel to torus axis): exact circle solutions.
- **All other orientations**: return `KernelError::NotSupported` (degree-4 curves
  including Villarceau circles are deferred).

---

## 2. Parameters

| Parameter | Type | Units | Description |
|-----------|------|-------|-------------|
| plane_origin | [f64; 3] | meters | Point on the plane |
| plane_normal | [f64; 3] | meters | Unit normal of the plane |
| torus_center | [f64; 3] | meters | Center of the torus |
| torus_axis | [f64; 3] | meters | Unit axis of the torus |
| torus_major_radius | f64 | meters | Major radius R (> 0) |
| torus_minor_radius | f64 | meters | Minor radius r (> 0, r < R for ring torus) |

---

## 3. Branch Table

### 3.1 Orientation dispatch

| Case | Condition | Action |
|------|-----------|--------|
| Perpendicular | \|plane_normal . torus_axis\| > 1 - TAU_MODEL | Solve as cross-section (Section 3.2) |
| Non-perpendicular | Otherwise | Return `NotSupported` |

### 3.2 Perpendicular plane cross-sections

When the plane normal is parallel to the torus axis, the plane cuts the torus
at a constant height z = d along the axis, where d is the signed distance from
the torus center to the plane along the axis direction:

    d = (plane_origin - torus_center) . torus_axis

The cross-section of the torus at height z = d consists of circles. In the
torus's local frame, the cross-section satisfies:

    x^2 + y^2 = (R +/- sqrt(r^2 - d^2))^2

yielding two concentric circles of radii R + sqrt(r^2 - d^2) (outer) and
R - sqrt(r^2 - d^2) (inner), centered on the torus axis at height d.

| Case | Condition | Intersection | SSI Result |
|------|-----------|-------------|------------|
| Disjoint | \|d\| > r | Plane misses the tube | Empty vec |
| Tangent (top/bottom) | \|d\| approx r (within TAU_MODEL) | Single circle at radius R | 1 circle, radius = R |
| Equatorial (d = 0) | \|d\| < TAU_MODEL | 2 concentric circles at R+r and R-r | 2 circles |
| General perpendicular | 0 < \|d\| < r | 2 concentric circles | 2 circles |
| Inner circle degeneracy | \|d\| < r AND R - sqrt(r^2 - d^2) < TAU_MODEL | Inner circle degenerates to point | 1 circle (outer only) |

**Key geometric insight**: The signed distance d from the torus center to the
plane along the torus axis fully determines the cross-section. When |d| < r,
the plane intersects the tube, producing an outer circle of radius
R_outer = R + sqrt(r^2 - d^2) and an inner circle of radius
R_inner = R - sqrt(r^2 - d^2). Both circles are centered on the torus axis
at the intersection height and have normals parallel to the torus axis.

**Self-intersecting (spindle) torus note**: When r >= R, the inner radius
R_inner = R - sqrt(r^2 - d^2) can be negative for |d| small enough. In this
regime the inner circle does not exist (the torus tube self-intersects). Emit
only the outer circle when R_inner <= 0.

---

## 4. Invariants

1. **Axis alignment**: For perpendicular planes, all intersection circles have
   normals parallel to the torus axis.
2. **Circle containment**: Every point on an intersection circle lies on both
   the plane and the torus surface simultaneously.
3. **Concentricity**: The two intersection circles share the same center
   (projection of torus center onto the plane along the axis).
4. **Radius ordering**: R_outer >= R_inner >= 0 always holds.
5. **Symmetry**: The result is symmetric about d = 0 (top and bottom
   cross-sections at +d and -d produce circles of equal radii).
6. **Monotonicity**: As |d| increases from 0 to r, R_outer decreases from R+r
   to R, and R_inner increases from R-r (or 0) to R. At |d| = r, both radii
   converge to R.

---

## 5. Oracles

| Oracle | Method | Tolerance |
|--------|--------|-----------|
| Point on torus | All circle points satisfy torus implicit equation = 0 | TAU_MODEL |
| Point on plane | All circle points satisfy plane equation (n . (p - o) = 0) | TAU_MODEL |
| Disjoint case | Returns empty vec when \|d\| > r | Exact |
| Tangent case | Returns exactly 1 circle when \|d\| approx r | TAU_MODEL |
| Circle count | Returns 2 circles when 0 < \|d\| < r and R_inner > TAU_MODEL | Exact |
| Circle normal | Normal parallel to torus axis | TAU_MODEL angle |
| Radius validation | R_outer = R + sqrt(r^2 - d^2), R_inner = R - sqrt(r^2 - d^2) | TAU_MODEL |

---

## 6. Failure Modes

| Input | Expected |
|-------|----------|
| Zero-length axis or normal | Caller's responsibility (pre-normalized) |
| Zero major or minor radius | Caller's responsibility |
| r >= R (spindle torus) | Supported: omit inner circle when R_inner <= 0 |
| NaN inputs | May produce empty or NaN curves (caller validates) |
| Non-perpendicular plane | Return `KernelError::NotSupported` |

The SSI solver is an internal function; input validation happens at the Kernel API level.

---

## 7. Research Basis

- [#1] Patrikalakis Ch.5 -- SSI for quadric surfaces: The plane-torus intersection
  is obtained by substituting the plane constraint into the torus implicit equation.
  For a plane perpendicular to the torus axis at height d, the fourth-degree
  equation factors into two circle equations. For oblique planes, the intersection
  is a general degree-4 curve (Villarceau circles arise only at a specific
  inclination angle arctan(R/r) through the center).
- [#33] Stroud -- Boundary representation and geometric modelling: torus
  cross-section classification and topology.
- [#25] Yang et al. -- Topology-guaranteed SSI for robust boolean operations.

**Simplification**: For the initial implementation, we handle only the
perpendicular case (plane normal parallel to torus axis) where the intersection
decomposes into circles with closed-form radii. This covers the most common
CAD scenario: flat faces cutting through O-ring grooves, toroidal channels,
and pipe flanges, where the cutting plane is perpendicular to the feature axis.
Oblique plane-torus intersection (degree-4 curves) is deferred and returns
`NotSupported`.

---

## 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (closed-form) for perpendicular cases; deferred (`NotSupported`)
for oblique cases.

**Surface pair**: Plane-Torus (A15 pair #6). This is a quadric pair requiring
exact SSI per A15. The perpendicular case factors cleanly into circle equations,
giving exact closed-form solutions with no approximation. The oblique case
produces degree-4 algebraic curves that require either quartic root-finding or
numerical tracing; these are deferred rather than approximated, consistent with
A15.2 (no mesh fallback for quadric pairs).
