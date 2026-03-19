# Spec: `make_torus` Primitive

**Status**: Spec phase
**Author**: Spec Writer (auto-waffle session 4)
**Date**: 2026-03-19

---

## 1. Goal

Add a `make_torus` method to `WaffleKernel` that creates a complete toroidal solid
(genus-1 surface of revolution). This completes the set of 5 Tier-1 analytic quadric
primitives (plane, cylinder, cone, sphere, torus).

The torus is centered at a given point, oriented along a given axis, with major
radius R (center-to-tube-center) and minor radius r (tube radius).

---

## 2. Parameters

| Parameter | Type | Units | Default | Valid Range | Error |
|-----------|------|-------|---------|-------------|-------|
| center | [f64; 3] | meters | — | any finite | InvalidInput |
| axis | [f64; 3] | meters | — | non-zero, will be normalized | InvalidInput if zero |
| major_radius | f64 | meters | — | > MIN_FEATURE_SIZE | InvalidInput if ≤ 0 or too small |
| minor_radius | f64 | meters | — | > MIN_FEATURE_SIZE, < major_radius | InvalidInput if ≤ 0 or ≥ major_radius |

**Constraint**: minor_radius < major_radius (ring torus only; horn and spindle tori are deferred).

---

## 3. Branch Table

| Case | Condition | Behavior |
|------|-----------|----------|
| Normal ring torus | 0 < r < R | Create solid torus with toroidal surface |
| Degenerate: r ≥ R | minor ≥ major | Return InvalidInput error |
| Degenerate: zero axis | axis = [0,0,0] | Return InvalidInput error |
| Degenerate: tiny radii | r or R < MIN_FEATURE_SIZE | Return InvalidInput error |

Only one behavioral branch (normal ring torus). All other cases are validation errors.

---

## 4. Invariants

1. **Volume**: V = 2π²Rr² (exact for ring torus)
2. **Surface area**: A = 4π²Rr (for reference; not directly tested)
3. **Bounding box**: center ± (R+r) in the plane perpendicular to axis, center ± r along axis
4. **Topology**: A torus is a genus-1 surface. Euler formula: V - E + F = 0 (not 2).
   For a minimal B-Rep: we use a quad-grid decomposition.
5. **Watertightness**: Tessellated mesh must be watertight (every edge shared by exactly 2 triangles)
6. **Normal consistency**: All face normals point outward
7. **Centroid**: At center point (by symmetry)
8. **All face geometry**: `SurfaceGeom::Toroidal` with correct parameters

---

## 5. Oracles

| Oracle | Method | Tolerance |
|--------|--------|-----------|
| Volume | Sum of signed tetrahedra from tessellation | 5% of analytical (tessellation is approximate) |
| Bounding box | Check tessellated mesh AABB against analytical bounds | TAU_MODEL per axis |
| Watertightness | Edge-sharing count = 2 for all edges | Exact (integer) |
| Normal consistency | All triangle normals point away from center | Dot product > 0 |
| Face surface type | All faces have `SurfaceGeom::Toroidal` | Exact match |
| Centroid | Average of all vertices ≈ center | 0.1 * minor_radius |

---

## 6. Failure Modes

| Input | Expected Error |
|-------|---------------|
| zero axis vector | `KernelError::InvalidInput("axis must be non-zero")` |
| major_radius ≤ 0 | `KernelError::InvalidInput(...)` |
| minor_radius ≤ 0 | `KernelError::InvalidInput(...)` |
| minor_radius ≥ major_radius | `KernelError::InvalidInput("minor_radius must be less than major_radius")` |
| NaN in any parameter | `KernelError::InvalidInput(...)` |

---

## 7. Research Basis

- [#1] Patrikalakis Ch.5 — Torus parametric surface definition: p(u,v) = center + (R + r·cos v)·(cos u · e1 + sin u · e2) + r·sin v · axis
- [#33] Stroud — B-Rep construction for surfaces of revolution
- [#16] Mäntylä — Euler operators for genus-1 solids (handle bodies via kfmrh/kemr)

**Topology approach**: The torus is topologically a handle body (genus 1). We decompose it
into a grid of quadrilateral faces (N_major × N_minor), each with toroidal surface geometry.
This avoids the complexity of a single-face genus-1 B-Rep while providing a clean tessellation
decomposition.

For the B-Rep, we use a 4×4 quad grid (16 faces, 16 vertices, 32 edges) as a minimal
representation that captures the topology. The Euler characteristic is V-E+F = 16-32+16 = 0,
correct for a torus.

---

## 7a. Analytical vs. Approximate Method Justification

**Method**: Exact (analytical surface geometry).

All faces are assigned `SurfaceGeom::Toroidal` with exact parameters. Tessellation approximates
the smooth surface for rendering but the B-Rep carries the exact analytical geometry. No mesh
fallback is used for the primitive construction.

**Surface pair coverage**: This spec covers primitive creation only, not boolean operations.
Torus-X SSI pairs are tracked separately in the A15 implementation table.
