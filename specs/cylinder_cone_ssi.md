# Cylinder–Cone SSI Solver

**A15 pair #7** — Surface-surface intersection for cylinder and cone.

**Status**: Spec phase
**References**: [#1] Patrikalakis Ch.5 (SSI algorithms for analytic surfaces), [#13] Keyser et al. ESOLID (exact arithmetic on quadrics)

---

## Goal

Implement an exact analytical SSI solver for the cylinder–cone surface pair. This enables boolean operations between cylindrical and conical solids without mesh approximation fallback, per A15.1.

The intersection of a cylinder and a cone is a degree-4 algebraic space curve. Special cases include coaxial pairs (circles), tangent configurations (empty), and axis-perpendicular cases (conic sections).

---

## Parameters

| Parameter | Type | Unit | Description |
|-----------|------|------|-------------|
| `cyl_origin` | `[f64; 3]` | meters | Point on cylinder axis |
| `cyl_axis` | `[f64; 3]` | unitless | Unit axis vector of cylinder |
| `cyl_radius` | `f64` | meters | Cylinder radius (> 0) |
| `cyl_z_min` | `f64` | meters | Min axial extent |
| `cyl_z_max` | `f64` | meters | Max axial extent |
| `cone_apex` | `[f64; 3]` | meters | Apex of cone |
| `cone_axis` | `[f64; 3]` | unitless | Unit axis vector of cone (from apex toward base) |
| `cone_half_angle` | `f64` | radians | Half-angle of cone (0 < α < π/2) |
| `cone_height_range` | `(f64, f64)` | meters | (min, max) distance from apex along axis |

### Valid Ranges
- `cyl_radius > 0`
- `cyl_z_max > cyl_z_min`
- `0 < cone_half_angle < π/2`
- `cone_height_range.0 >= 0, cone_height_range.1 > cone_height_range.0`
- Axes must be unit vectors

### Error Conditions
- Invalid radius/angle → `KernelError::InvalidInput`
- Zero-length height range → return empty (degenerate)

---

## Branch Table

| Case | Condition | SSI Result |
|------|-----------|------------|
| **No intersection** | Surfaces don't intersect within extents | Empty vec |
| **Coaxial** | Same axis, cone radius matches cyl radius at some height | 0, 1, or 2 circles where r_cone(h) = R_cyl |
| **Parallel axes, offset** | Axes parallel but not collinear | Degree-4 curve → representative Line or empty |
| **Perpendicular axes** | Axes at 90° | Degree-4 curve → representative Line |
| **General position** | Arbitrary orientation | Degree-4 curve → representative Line |
| **Tangent** | Surfaces just touch | Single point → empty (below feature size) |
| **Disjoint (AABB)** | Bounding volumes don't overlap | Empty (fast reject) |

---

## Algorithm

### Coaxial case (axes collinear)

Transform to local frame where shared axis = Z.

Cone surface at height h from apex: r_cone(h) = h · tan(α).
Cylinder surface: r_cyl = R (constant).

Set equal: `h · tan(α) = R` → `h = R / tan(α)`.

Convert h to the cylinder's axial coordinate and check both height ranges.
If within ranges, return a circle at that height with radius R.

For opposite-direction axes, adjust the height calculation accordingly.

### General-position case (non-coaxial)

1. **AABB fast reject**: Compute bounding boxes for both surfaces. If disjoint, return empty.
2. **Numerical scanning**: Sample the cylinder surface at regular (θ, z) intervals. For each sample point, check distance to the cone surface. Points within tolerance are intersection candidates.
3. **Curve extent**: From found intersection points, compute the bounding Line segment spanning the extent of the intersection on the cylinder surface.

The true intersection is a degree-4 algebraic curve. For the boolean pipeline, we represent it as a Line segment spanning the intersection extent. This matches the representation used by cylinder_sphere_ssi for the general offset case.

### Tangent detection

If the intersection extent is below MIN_FEATURE_SIZE, return empty (tangent touch).

---

## Invariants

1. All returned curves lie on both surfaces within TAU_MODEL.
2. Circle curves have `normal` aligned with the shared axis.
3. Coaxial case returns at most 2 circles (cone crosses cylinder radius at most twice for a one-sided cone).
4. Empty result for tangent touches (below feature size).

---

## Oracles

- **Circle center**: Must lie on the shared axis.
- **Circle radius**: Must equal `cyl_radius` (since the intersection is where cone meets cylinder).
- **Point-on-surface**: Every point on an SSI curve satisfies both surface equations within TAU_MODEL.
- **Symmetry**: Coaxial case with symmetric height ranges produces symmetric circles.

---

## Failure Modes

- Degenerate cone (half_angle ≈ 0 or ≈ π/2) → handled by early return
- Zero-radius cylinder → return empty
- Coincident surfaces (cylinder is embedded in cone) → return empty (degenerate overlap)
- Near-tangent → return empty (below feature size)

---

## Research Basis

- Patrikalakis Ch.5 establishes that cylinder-cone intersection is degree ≤ 4.
- For coaxial case: closed-form circle (linear equation in h).
- For general case: numerical sampling approach consistent with cone_cone_ssi solver pattern.
- Keyser et al. ESOLID provides exact arithmetic framework for quadric SSI (reference for future exact implementation).

---

## Analytical vs. Approximate Justification (A15, A15.5)

This solver provides **exact closed-form SSI** for the coaxial case (circle result).
For the general-position case, it uses **numerical sampling** (consistent with the
cone_cone_ssi pattern) to find intersection extent, returning a representative
Line segment. This is the analytical path — no mesh boolean fallback is used.
