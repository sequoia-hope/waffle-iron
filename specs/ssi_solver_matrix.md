# SSI Solver Matrix

Authoritative enumeration of all 15 quadric surface–surface intersection (SSI)
pairs, their sub-cases, implementation status, and acceptance criteria.

This is a **living document**. Update it as solvers are implemented.

## Goal

Define exactly what "done" means for each SSI pair and track progress toward
full analytical coverage per A15.1 (exact SSI for analytical surface pairs).

## References

- [#1] Patrikalakis et al. Ch.5 — exact SSI algorithms for all quadric pairs
- [#25] Yang et al. (2023) — topology-guaranteed SSI via Dixon resultant
- [#27] Li et al. (2026) — hybrid SSI architecture survey

## Sub-case dimensions

For each of the 15 pairs, sub-cases arise from:

- **Orientation**: parallel, perpendicular, oblique, general
- **Position**: coaxial, offset, tangent, disjoint, enclosed
- **Size**: equal radii/angles, unequal

## Acceptance criteria (per sub-case)

A sub-case is "done" when:

1. Returns analytical `SSICurve` (Circle, Ellipse, Line, or future Conic — never a polyline approximation)
2. Unit test with geometric oracle (curve lies on both surfaces within `TAU_MODEL`)
3. Integration test via boolean op (watertight result, correct volume)
4. No sampling loops or `SSI_SAMPLE_ON_SURFACE_TOL` usage in that code path

---

## Status matrix

### Legend

| Method | Meaning |
|--------|---------|
| `analytical` | Closed-form, returns exact Circle/Ellipse/Line SSICurve |
| `sampling` | Grid scan + zero-crossing, returns polyline Line approximation |
| `not-supported` | Returns `KernelError::NotSupported` |

| Status | Meaning |
|--------|---------|
| `done` | Analytical, tested, all orientations |
| `partial` | Analytical for some sub-cases, NotSupported or sampling for others |
| `stub` | Sampling-based approximation exists — violates A15.1 |
| `missing` | Not implemented at all |

---

### 1. Plane–Plane

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Intersecting | analytical | done | Returns `SSICurve::Line` — handled in `boolean.rs` via plane normal cross-product |
| Parallel (disjoint) | analytical | done | Returns empty |
| Coplanar | analytical | done | Returns overlap indicator |

**Implementation**: `boolean.rs` plane-plane logic. All sub-cases covered.

---

### 2. Plane–Cylinder

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Perpendicular (plane ⊥ axis) | analytical | done | Returns `Circle` (`plane_cylinder_perp`) |
| Parallel (plane ∥ axis) | analytical | done | Returns 0 or 2 `Line` segments (`plane_cylinder_parallel`) |
| Oblique | analytical | done | Returns `Ellipse` with semi_minor=R, semi_major=R/sin(γ) (`plane_cylinder_oblique`) |
| Disjoint (parallel, no contact) | analytical | done | Returns empty |
| Tangent (parallel, grazing) | analytical | done | Returns empty (within TOL) |
| Arbitrary axis orientation | analytical | done | General-position solver accepts any axis direction |

**Implementation**: `ssi.rs:plane_cylinder_ssi` (lines 91–132). Fully analytical.

---

### 3. Plane–Cone

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Perpendicular (plane ⊥ axis) | analytical | done | Returns `Circle` at cut height (`plane_cone_ssi`) |
| Oblique (ellipse, γ > β) | analytical | done | Returns `Ellipse` with exact semi-axes |
| Oblique (parabola, γ ≈ β) | analytical | done | Returns `SSICurve::Parabola` with vertex, axis, focal_length |
| Oblique (hyperbola, γ < β) | analytical | done | Returns `SSICurve::Hyperbola` with center, axes, semi-transverse/conjugate |
| Through apex (γ < β) | analytical | done | Returns 2 `Line` generator segments |
| Through apex (γ > β) | analytical | done | Returns empty (degenerate point) |

**Implementation**: `ssi.rs:plane_cone_ssi`. All six sub-cases: perpendicular (circle), oblique ellipse, oblique parabola, oblique hyperbola, through-apex (lines), and no-intersection (empty).

---

### 4. Plane–Sphere

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Cutting | analytical | done | Returns `Circle` (`plane_sphere_ssi`) |
| Tangent | analytical | done | Returns empty (within TOL) |
| Disjoint | analytical | done | Returns empty |

**Implementation**: `ssi.rs:plane_sphere_ssi` (lines 528–549). Fully analytical.

---

### 5. Cylinder–Cylinder

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Parallel, offset (overlapping) | analytical | done | Returns 2 `Line` segments (`cylinder_cylinder_ssi`) |
| Parallel, coaxial (same axis) | analytical | done | Returns empty (coaxial) |
| Parallel, disjoint | analytical | done | Returns empty |
| Non-parallel, equal-R, ≥15° | analytical | done | Returns 2 `Ellipse` (`cylinder_cylinder_ssi_non_parallel`). Extended from ≥60° to ≥15° (Sprint 69). |
| Non-parallel, equal-R, <15° | not-supported | missing | Returns `KernelError::NotSupported` (near-parallel, eccentricity > 0.99) |
| Non-parallel, unequal-R, ≥15° | analytical | done | Returns 2 `Degree4CylCyl` parametric curves. Formula: z(θ) = (R_A sin θ cos α ± √(R_B² − R_A² cos²θ)) / sin α. 10 tests with on-surface oracle. |
| Skew axes (non-intersecting) | not-supported | missing | Returns `KernelError::NotSupported` |

**Implementation**: `ssi.rs:cylinder_cylinder_ssi` (lines 292–359) and
`cylinder_cylinder_ssi_non_parallel` (lines 374–522). Partial coverage.

---

### 6. Plane–Torus

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Perpendicular (normal ∥ axis) | analytical | done | Returns 1–2 `Circle` (`plane_torus_ssi`) |
| Perpendicular, tangent (|d|=r) | analytical | done | Returns 1 `Circle` at radius R |
| Perpendicular, disjoint | analytical | done | Returns empty |
| Non-perpendicular (general) | not-supported | missing | Returns `KernelError::NotSupported` |

**Implementation**: `ssi.rs:plane_torus_ssi` (lines 1134–1196). Only axis-perpendicular.

---

### 7. Cylinder–Cone

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial | analytical | done | Returns `Circle` where cone radius = cyl radius (`cylinder_cone_ssi`) |
| Non-coaxial, parallel offset | sampling | **stub** | 72×200 grid scan, returns `Line` approximation |
| Non-coaxial, general | sampling | **stub** | 72×200 grid scan with sign-change detection |
| Same-apex (degenerate) | sampling | **stub** | Falls through to general scan |

**Implementation**: `ssi.rs:cylinder_cone_ssi` (lines 1429–1632).
Grid: 72 θ-samples × 200 z-samples. Uses `SSI_SAMPLE_ON_SURFACE_TOL`.

---

### 8. Cylinder–Sphere

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial (sphere center on axis) | analytical | done | Returns 0–2 `Circle` (`cylinder_sphere_ssi`) |
| Coaxial, tangent | analytical | done | Returns empty |
| Offset, overlapping | analytical | done | Returns 2 `Degree4CylSphere` parametric curves (upper/lower branches) |
| Disjoint | analytical | done | Returns empty |
| Enclosed (sphere inside cyl) | analytical | done | Returns empty |

**Implementation**: `ssi.rs:cylinder_sphere_ssi`.
Coaxial path returns exact circles. Offset path returns exact `Degree4CylSphere`
parametric curves via cylinder parameterization into sphere equation:
z(θ) = z_center ± √(R_s² − d² − R_c² + 2·R_c·d·cos θ). 11 tests with on-surface oracle.

---

### 9. Cone–Cone

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial, different apices | analytical | done | Returns `Circle` where radii match (`cone_cone_ssi`) |
| Same apex | sampling | **stub** | 72×200 grid scan with `SSI_SAMPLE_ON_SURFACE_TOL` |
| General position | sampling | **stub** | 72×100 grid scan with `SSI_SAMPLE_ON_SURFACE_TOL` |
| Parallel generators (same angle+direction) | analytical | done | Returns empty |

**Implementation**: `ssi.rs:cone_cone_ssi` (lines 1638–1937).
Coaxial path is exact. Same-apex and general paths use numerical scanning.

---

### 10. Cylinder–Torus

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial | analytical | done | Returns 0–2 `Circle` (`cylinder_torus_ssi`) |
| Coaxial, tangent | analytical | done | Returns empty |
| General position | sampling | **stub** | 360×200 grid scan with sign-change detection |
| Disjoint | analytical | done | Bounding-sphere reject |

**Implementation**: `ssi.rs:cylinder_torus_ssi` (lines 1964–2132).
Coaxial path is exact. General path uses `torus_signed_distance` + 360×200 grid.

---

### 11. Cone–Sphere

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial (sphere center on axis) | analytical | done | Returns 0–2 `Circle` via quadratic (`cone_sphere_ssi`) |
| Coaxial, tangent | analytical | done | Returns empty |
| Offset, overlapping | analytical | done | Exact `Degree4ConeSphere` parametric curve via coplanar circle intersection at each axial height |
| Disjoint | analytical | done | Returns empty |

**Implementation**: `ssi.rs:cone_sphere_ssi`.
Coaxial path is exact (quadratic in h). Offset path uses an exact
`Degree4ConeSphere` parametric curve — no sampling or mesh fallback.

---

### 12. Sphere–Sphere

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Overlapping | analytical | done | Returns `Circle` (`sphere_sphere_ssi`) |
| Tangent | analytical | done | Returns empty |
| Disjoint | analytical | done | Returns empty |
| Enclosed | analytical | done | Returns empty |
| Coincident | analytical | done | Returns empty (degenerate) |

**Implementation**: `ssi.rs:sphere_sphere_ssi` (lines 605–650). Fully analytical.

---

### 13. Cone–Torus

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial | analytical | done | Returns 0–2 `Circle` via quadratic (`cone_torus_ssi`) |
| Coaxial, tangent | analytical | done | Returns single `Circle` |
| General position | sampling | **stub** | 360×200 grid scan with sign-change detection |
| Disjoint | analytical | done | Bounding-sphere reject |

**Implementation**: `ssi.rs:cone_torus_ssi` (lines 2141–2314).
Coaxial path solves sec²(α)·h² + … = 0. General path scans cone surface with
`torus_signed_distance`.

---

### 14. Sphere–Torus

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Axial (sphere center on torus axis) | analytical | done | Returns 0–2 `Circle` via quadratic (`sphere_torus_ssi`) |
| Axial, tangent | analytical | done | Returns single `Circle` |
| Off-axis, overlapping | sampling | **stub** | 360×36 grid scan with `SSI_SAMPLE_ON_SURFACE_TOL` |
| Disjoint | analytical | done | Distance check |
| Enclosed | analytical | done | Returns empty |

**Implementation**: `ssi.rs:sphere_torus_ssi` (lines 1207–1412).
Axial path is exact. Off-axis path scans 360 θ × 36 φ samples on torus surface.

---

### 15. Torus–Torus

| Sub-case | Method | Status | Notes |
|----------|--------|--------|-------|
| Coaxial | analytical | done | Returns 0–2 `Circle` via quadratic (`torus_torus_ssi`) |
| Coaxial, identical geometry | analytical | done | Returns empty (degenerate) |
| General position | sampling | **stub** | 360×36 grid scan with `SSI_SAMPLE_ON_SURFACE_TOL` |
| Disjoint | analytical | done | Bounding-sphere reject |

**Implementation**: `ssi.rs:torus_torus_ssi` (lines 2323–2517).
Coaxial path is exact. General path scans 360 θ × 36 φ samples on torus A surface.

---

## Summary

| # | Pair | Overall Status | Analytical sub-cases | Sampling/missing sub-cases |
|---|------|----------------|---------------------|---------------------------|
| 1 | Plane–Plane | **done** | All | — |
| 2 | Plane–Cylinder | **done** | All (perp, parallel, oblique) | — |
| 3 | Plane–Cone | **done** | All (perp, oblique ellipse/parabola/hyperbola, through-apex) | — |
| 4 | Plane–Sphere | **done** | All | — |
| 5 | Cylinder–Cylinder | **partial** | Parallel + equal-R non-parallel ≥15° + unequal-R non-parallel ≥15° | Near-parallel (<15°), skew |
| 6 | Plane–Torus | **partial** | Axis-perpendicular only | All other orientations |
| 7 | Cylinder–Cone | **stub** | Coaxial only | General position (72×200 scan) |
| 8 | Cylinder–Sphere | **done** | All (coaxial circles + offset Degree4CylSphere parametric) | — |
| 9 | Cone–Cone | **stub** | Coaxial offset | Same-apex + general (72×100–200 scan) |
| 10 | Cylinder–Torus | **stub** | Coaxial | General position (360×200 scan) |
| 11 | Cone–Sphere | **done** | All (coaxial circles + offset Degree4ConeSphere parametric) | — |
| 12 | Sphere–Sphere | **done** | All | — |
| 13 | Cone–Torus | **stub** | Coaxial | General position (360×200 scan) |
| 14 | Sphere–Torus | **stub** | Axial | Off-axis (360×36 scan) |
| 15 | Torus–Torus | **stub** | Coaxial | General position (360×36 scan) |

**Fully analytical**: 7 of 15 pairs (Plane–Plane, Plane–Cylinder, Plane–Cone, Plane–Sphere, Cylinder–Sphere, Cone–Sphere, Sphere–Sphere)
**Partial**: 2 of 15 pairs (Cyl–Cyl, Plane–Torus)
**Stub (sampling)**: 6 of 15 pairs (Cyl–Cone, Cone–Cone, Cyl–Torus, Cone–Torus, Sphere–Torus, Torus–Torus)

---

*Created: Sprint 68, 2026-03-25*
*Source of truth for SSI solver status. Governance table (A15.4) links here.*
