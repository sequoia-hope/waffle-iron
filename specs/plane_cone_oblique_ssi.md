# Plane-Cone Oblique SSI

Analytical surface-surface intersection for oblique plane-cone configurations.

**Status**: Implementation spec
**References**: [#1] Patrikalakis Ch.5 — SSI for quadric pairs
**Governance**: A15.1 (exact SSI for quadric pairs), A15.2 (no mesh fallback)

---

## Goal

Extend `plane_cone_ssi()` to handle oblique cuts (plane not perpendicular to cone
axis). The intersection of a plane with a cone produces a conic section whose type
depends on the angle between the cutting plane and the cone axis.

This eliminates the `KernelError::NotSupported` for the oblique sub-cases of SSI
pair #3 (Plane-Cone), advancing A15 compliance.

## Parameters

| Parameter | Type | Description | Unit |
|-----------|------|-------------|------|
| plane_origin | [f64; 3] | A point on the cutting plane | meters |
| plane_normal | [f64; 3] | Unit normal of the cutting plane | unitless |
| cone_apex | [f64; 3] | Apex point of the cone | meters |
| cone_axis | [f64; 3] | Unit axis direction of the cone (from apex) | unitless |
| half_angle | f64 | Half-angle of the cone | radians, (0, π/2) |
| max_height | f64 | Maximum height along axis from apex | meters, > 0 |

## Branch Table

Let α = angle between plane normal and cone axis = acos(|plane_normal · cone_axis|).
Let β = half_angle of cone.
Let γ = π/2 - α (angle between cutting plane and cone axis).

| # | Condition | Conic Type | SSICurve Return | Status |
|---|-----------|------------|-----------------|--------|
| B1 | α ≈ 0 (plane ⊥ axis) | Circle | `Circle` | exists |
| B2 | γ > β (steep cut) | Ellipse | `Ellipse` | **new** |
| B3 | γ ≈ β (tangent cut) | Parabola | `NotSupported` | boundary |
| B4 | γ < β (shallow cut) | Hyperbola | `NotSupported` | boundary |
| B5 | Plane through apex | Degenerate | `Line` pair | **new** |
| B6 | Plane parallel, no intersect | Empty | empty vec | **new** |

### Discriminant

The conic type is determined by comparing `cos²(α)` with `sin²(β)`:

- `cos²(α) > sin²(β) + TOL` → **Ellipse** (B2)
- `|cos²(α) - sin²(β)| ≤ TOL` → **Parabola** (B3) — return NotSupported
- `cos²(α) < sin²(β) - TOL` → **Hyperbola** (B4) — return NotSupported

### Through-apex detection (B5)

The apex lies on the plane when `|(cone_apex - plane_origin) · plane_normal| < TOL`.
When this occurs AND the cut is oblique, the intersection degenerates to two lines
through the apex.

## Invariants

### I1: Points on both surfaces
Every point on the returned SSICurve must lie on both the plane and the cone surface
within TAU_MODEL tolerance.

- Plane test: `|(p - plane_origin) · plane_normal| < TAU_MODEL`
- Cone test: `|distance_to_cone_surface(p) | < TAU_MODEL`

### I2: Ellipse geometry
For the ellipse case (B2):
- The ellipse center lies on the plane
- The ellipse normal equals the plane normal
- semi_major ≥ semi_minor > 0
- semi_minor = R_cut (radius of cone at the intersection height along plane normal)
- The ellipse is contained within the cone's bounding height [0, max_height]

### I3: Through-apex lines
For the through-apex case (B5):
- Both lines pass through the cone apex
- Both lines lie on the plane
- Both lines lie on the cone surface
- The two lines are symmetric about the projection of the cone axis onto the plane

### I4: Existing perpendicular case preserved
The existing B1 (perpendicular) case must produce identical results.

## Oracles

### O1: Ellipse on-surface oracle
Sample N points on the returned ellipse. For each point p:
- `|(p - plane_origin) · plane_normal| < TAU_MODEL`
- The radial distance from cone axis equals `h * tan(half_angle)` where h is the
  axial height, within TAU_MODEL

### O2: Ellipse semi-axes oracle
For a cone with half_angle β cut by a plane at angle γ to the axis:
- `semi_minor = h_center * tan(β)` where h_center is the axial height at ellipse center
- `semi_major = semi_minor / sin(γ)` (foreshortening due to oblique cut)

### O3: Degenerate line oracle
For through-apex case: both line endpoints lie on the plane and cone surface.

### O4: Regression oracle
Run existing perpendicular plane-cone tests — identical results.

## Failure Modes

| Condition | Behavior |
|-----------|----------|
| Parabola (γ ≈ β) | Return `KernelError::NotSupported` with diagnostic |
| Hyperbola (γ < β) | Return `KernelError::NotSupported` with diagnostic |
| Cone with half_angle ≤ 0 or ≥ π/2 | Return `KernelError::NotSupported` |
| max_height ≤ 0 | Return empty vec |
| Ellipse entirely outside [0, max_height] | Return empty vec |

## Research Basis

- **[#1] Patrikalakis et al., Ch.5**: Exact SSI algorithms for all quadric pairs.
  Plane-cone intersection produces conic sections classified by the discriminant
  of the quadratic form.
- **Classical conic section theory** (Apollonius): A plane intersecting a circular
  cone produces exactly one of {circle, ellipse, parabola, hyperbola, point,
  line, pair of lines} depending on the angle of intersection.

## Analytical Method Justification (FIP §7a)

- **Method**: Exact (closed-form SSI)
- **Justification**: Plane-cone is a Tier 1 × Tier 1 quadric pair. Closed-form
  solutions exist for all conic sections. Per A15.1, mesh approximation is prohibited.
- **Surface pairs**: Plane-Cone only. Both surfaces are Tier 1 analytic.
