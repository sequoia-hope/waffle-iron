# Cylinder–Torus SSI Solver

**A15 pair #10** — Surface-surface intersection for cylinder and torus.

**Status**: Spec phase
**References**: [#1] Patrikalakis Ch.5 (SSI algorithms for analytic surfaces), [#13] Keyser et al. ESOLID

---

## Goal

Implement an analytical SSI solver for the cylinder–torus surface pair. This enables boolean operations between cylindrical and toroidal solids without mesh approximation fallback, per A15.1.

The intersection of a cylinder and a torus is a degree-8 algebraic space curve in general. Special cases (coaxial) reduce to degree-4 or exact circles.

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `cyl_origin` | `[f64; 3]` | meters | Point on cylinder axis |
| `cyl_axis` | `[f64; 3]` | unitless | Unit axis vector of cylinder |
| `cyl_radius` | `f64` | meters | Cylinder radius (> 0) |
| `cyl_z_min` | `f64` | meters | Min axial extent |
| `cyl_z_max` | `f64` | meters | Max axial extent |
| `torus_center` | `[f64; 3]` | meters | Center of torus |
| `torus_axis` | `[f64; 3]` | unitless | Unit axis vector of torus |
| `torus_major_radius` | `f64` | meters | Major radius R (> 0) |
| `torus_minor_radius` | `f64` | meters | Minor (tube) radius r (> 0, r < R for ring torus) |

### Valid Ranges
- `cyl_radius > 0`, `torus_major_radius > 0`, `torus_minor_radius > 0`
- `cyl_z_max > cyl_z_min`

---

## Branch Table

| Case | Condition | SSI Result |
|------|-----------|------------|
| **No intersection** | Surfaces don't overlap | Empty vec |
| **Coaxial** | Cylinder axis = torus axis | 0, 1, 2, 3, or 4 circles |
| **Perpendicular axes** | Axes at 90° | Degree-8 curve → Line |
| **General position** | Arbitrary orientation | Degree-8 curve → Line |
| **Tangent** | Surfaces just touch | Empty (below feature size) |
| **Disjoint (bounding sphere)** | Too far apart | Empty (fast reject) |

---

## Algorithm

### Coaxial case (cylinder axis = torus axis)

By symmetry, the intersection consists of circles. In the torus midplane:
- Torus cross-section: circle of radius r centered at distance R from axis
- Cylinder: constant radius R_cyl

A point on the torus at axial height z has radial distance from axis:
  ρ = R ± sqrt(r² - z²)

Setting ρ = R_cyl: R ± sqrt(r² - z²) = R_cyl → sqrt(r² - z²) = ±(R_cyl - R)

This gives z² = r² - (R_cyl - R)², valid when |R_cyl - R| ≤ r.

Solutions: z = ±sqrt(r² - (R_cyl - R)²) for the outer branch (R + s = R_cyl),
and z = ±sqrt(r² - (R_cyl - R)²) for both branches.

Actually: z² = r² - (R_cyl - R)² (outer) and z² = r² - (R_cyl + R)² ... no.

More carefully: the torus is (ρ - R)² + z² = r². The cylinder is ρ = R_cyl.
Substituting: (R_cyl - R)² + z² = r² → z = ±sqrt(r² - (R_cyl - R)²).

This gives 0, 1, or 2 circles depending on whether |R_cyl - R| < r, = r, or > r.

But the cylinder also intersects the inner portion of the torus (ρ = R - s, s = sqrt(r²-z²)).
Setting ρ = R_cyl: R - s = R_cyl → s = R - R_cyl, valid when R_cyl < R and R - R_cyl ≤ r.
Then z² = r² - (R - R_cyl)² same as above.

So for a ring torus: up to 2 distinct z-values, each giving a circle. Total: 0-4 circles
(each z can give 2 circles if both ρ-solutions = R_cyl, but in fact both branches give
the same z-values, so max 2 circles for coaxial case).

### General case (non-coaxial)

1. **Bounding sphere fast reject**: Torus is bounded by sphere of radius R + r centered at torus_center. Cylinder is bounded by sphere along its axis. If bounding spheres don't overlap, return empty.
2. **Numerical scanning**: Sample the cylinder surface at (θ, z) grid. For each point, compute distance to torus surface. Collect near-zero-distance points as intersection candidates.
3. **Curve extent**: Return representative Line segment from found points.

---

## Invariants

1. All returned curves lie on both surfaces within TAU_MODEL.
2. Circle normals for coaxial case are aligned with the shared axis.
3. Coaxial case returns at most 4 circles.
4. Empty result for tangent touches.

---

## Oracles

- **Circle radius** in coaxial case = `cyl_radius`
- **Circle z-height** satisfies `(R_cyl - R)² + z² = r²`
- **Point-on-surface**: every result point satisfies both surface equations

---

## Research Basis

- Patrikalakis Ch.5: cylinder-torus is degree ≤ 8
- Coaxial case reduces to quadratic in z² (exact circles)
- General case: numerical scanning (consistent with other torus SSI solvers)
