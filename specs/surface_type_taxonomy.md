# Surface Type Taxonomy

Specification for Waffle Iron's surface representation strategy.

**Status**: Design spec (Tier 1 analytic types fully represented in `SurfaceGeom` enum)
**References**: [#1] Patrikalakis, [#32] Piegl & Tiller, [#33] Stroud, [#36] Parasolid, [#37] Mistry
**Governance**: ADR-11, A15, A15.5

---

## Goal

Define a surface type hierarchy that:
1. Preserves analytical geometry through boolean chains (A15)
2. Stores procedural construction recipes for editing
3. Supports NURBS as a universal fallback
4. Enables surface-type-aware tessellation and SSI dispatch

## Three-Tier Hierarchy

```
Tier 1 — Analytic (5 types)     → exact SSI, O(1) eval, compact storage
Tier 2 — Procedural (6 types)   → recipe storage, O(n) eval, convert to NURBS for SSI
Tier 3 — Freeform (1 type)      → universal fallback, O(p·q) eval, numerical SSI
```

Direction of conversion: Tier 1 → Tier 2 → Tier 3 (upward only, never downward).
Conversion is lazy — performed only when a downstream operation requires it.

---

## Tier 1 — Analytic Surfaces

Five quadric surfaces with closed-form evaluation, normals, and SSI.

### Plane

- **Math**: All points **p** such that (**p** - **origin**) · **normal** = 0
- **Parameters**: origin (Point3), normal (Vector3), u_axis (Vector3)
- **Eval cost**: O(1) — linear in parameters
- **SSI pairs**: Plane-Plane (line/overlap), Plane-Cylinder (ellipse), Plane-Cone (conic), Plane-Sphere (circle), Plane-Torus (degree-4 curve)
- **Tessellation**: Two triangles for rectangular trim; boundary-adaptive otherwise
- **Status**: In `SurfaceGeom` enum as `Planar`

### Cylinder

- **Math**: All points **p** at distance *r* from axis line
- **Parameters**: origin (Point3), axis (Vector3), radius (f64)
- **Eval cost**: O(1) — trigonometric in parameter
- **SSI pairs**: Cylinder-Plane (ellipse), Cylinder-Cylinder (degree ≤ 4), Cylinder-Cone (degree ≤ 4), Cylinder-Sphere (degree ≤ 4), Cylinder-Torus (degree ≤ 8)
- **Tessellation**: Angular strips; triangle count proportional to angular extent / tolerance
- **Status**: In `SurfaceGeom` enum as `Cylindrical`

### Cone

- **Math**: All points **p** on rays from apex at half-angle *α* to axis
- **Parameters**: origin (Point3), axis (Vector3), half_angle (f64), apex_distance (f64)
- **Eval cost**: O(1) — trigonometric in parameter
- **SSI pairs**: Cone-Plane (conic section), Cone-Cylinder (degree ≤ 4), Cone-Cone (degree ≤ 4), Cone-Sphere (degree ≤ 4), Cone-Torus (degree ≤ 8)
- **Tessellation**: Angular strips with varying radius; adaptive near apex
- **Status**: In `SurfaceGeom` enum as `Conical`

### Sphere

- **Math**: All points **p** at distance *r* from center
- **Parameters**: center (Point3), radius (f64)
- **Eval cost**: O(1) — trigonometric in two parameters
- **SSI pairs**: Sphere-Plane (circle), Sphere-Cylinder (degree ≤ 4), Sphere-Cone (degree ≤ 4), Sphere-Sphere (circle), Sphere-Torus (degree ≤ 4)
- **Tessellation**: Latitude-longitude strips or icosahedral subdivision; adaptive by curvature
- **Status**: In `SurfaceGeom` enum as `Spherical`

### Torus

- **Math**: All points **p** at distance *r* from a circle of radius *R*
- **Parameters**: center (Point3), axis (Vector3), major_radius (f64), minor_radius (f64)
- **Eval cost**: O(1) — trigonometric in two parameters
- **SSI pairs**: Torus-Plane (degree-4), Torus-Cylinder (degree ≤ 8), Torus-Cone (degree ≤ 8), Torus-Sphere (degree ≤ 4), Torus-Torus (degree ≤ 8)
- **Tessellation**: Double angular strips; adaptive by curvature ratio R/r
- **Status**: In `SurfaceGeom` enum as `Toroidal`

---

## Tier 2 — Procedural Surfaces

Six surface types defined by construction recipes. Stored natively; converted to
NURBS (Tier 3) only when needed for SSI with freeform surfaces.

### Swept Surface

- **Math**: S(u, v) = T(v) · P(u), where P is a 2D profile and T(v) is a rigid-body transform along spine curve
- **Construction recipe**: profile (wire/face), spine (curve), orientation law (Frenet/fixed/custom)
- **Eval cost**: O(n) per point — evaluate profile + spine + frame at parameter
- **SSI strategy**: Convert to NURBS, then numerical SSI (ADR-2)
- **Tessellation**: Sample along spine at curvature-adaptive intervals; sweep profile tessellation
- **Industry names**: Parasolid `PK_SURF_swept`, ACIS `swept_surface`, OCCT `GeomFill_Pipe`
- **Research basis**: [#37] Mistry — profile + spine + orientation law formalization; [#33] §6.2 — Euler-op-based sweep

### Spun / Revolved Surface

- **Math**: S(u, θ) = Rot(θ, **axis**) · P(u), where P is a profile curve
- **Construction recipe**: profile (curve), axis (line), angle range (f64, f64)
- **Eval cost**: O(n) per point — evaluate profile + rotation
- **SSI strategy**: Convert to NURBS, then numerical SSI. Note: full revolutions of lines/circles produce Tier 1 surfaces (cylinder, cone, sphere, torus) — detect and downcast
- **Tessellation**: Angular strips like cylinder/torus; adaptive along profile curvature
- **Industry names**: Parasolid `PK_SURF_spun`, ACIS `spun_surface`, OCCT `Geom_SurfaceOfRevolution`
- **Research basis**: [#33] §6.2 — swing operation with Euler ops

### Ruled Surface

- **Math**: S(u, v) = (1-v) · C₁(u) + v · C₂(u), linear interpolation between two curves
- **Construction recipe**: curve1 (curve), curve2 (curve)
- **Eval cost**: O(n) per point — evaluate both curves + interpolate
- **SSI strategy**: Convert to NURBS (degree 1 in v-direction)
- **Tessellation**: Straightforward — v-direction is linear, adapt along u-direction only
- **Industry names**: Not a distinct Parasolid type (stored as B-surface), ACIS preserves as form

### Lofted Surface

- **Math**: S(u, v) = interpolation through cross-section curves C₁(u) ... Cₖ(u)
- **Construction recipe**: sections (vec of curves), parameters (vec of f64), continuity (C0/C1/C2)
- **Eval cost**: O(k·n) per point — evaluate sections + blend
- **SSI strategy**: Convert to NURBS, then numerical SSI
- **Tessellation**: Adaptive along both u (section curvature) and v (inter-section blending)
- **Industry names**: Parasolid via B-surface, ACIS `loft_surface`, OCCT `GeomFill_BSplineCurves`
- **Research basis**: [#32] Ch.10 — surface skinning; [#33] Ch.13 — multi-section lofting

### Offset Surface

- **Math**: S_off(u, v) = S_base(u, v) + d · **n**(u, v), normal offset of base surface
- **Construction recipe**: base_surface (SurfaceGeom), distance (f64)
- **Eval cost**: Same as base surface + O(1) for normal computation
- **SSI strategy**: Depends on base surface tier; analytic base → may have closed-form
- **Tessellation**: Same strategy as base surface; offset distance affects error bounds
- **Industry names**: Parasolid `PK_SURF_offset`, ACIS `offset_surface`, OCCT `Geom_OffsetSurface`
- **Research basis**: [#1] Ch.11 — offset curve/surface properties and singularities

### Pipe Surface

- **Math**: S(u, v) = C(v) + r · (cos(u) · **n**(v) + sin(u) · **b**(v)), tube around spine curve
- **Construction recipe**: spine (curve), radius (f64)
- **Eval cost**: O(n) per point — evaluate spine + Frenet frame + circle
- **SSI strategy**: Convert to NURBS; special case: straight spine → Cylinder (Tier 1)
- **Tessellation**: Angular strips along spine; adaptive by spine curvature
- **Industry names**: Subset of Parasolid swept, ACIS `pipe_surface`, OCCT `GeomFill_Pipe`

---

## Tier 3 — Freeform Surface

### BSpline / NURBS

- **Math**: S(u, v) = Σᵢ Σⱼ Nᵢ,ₚ(u) · Nⱼ,q(v) · wᵢⱼ · Pᵢⱼ / Σᵢ Σⱼ Nᵢ,ₚ(u) · Nⱼ,q(v) · wᵢⱼ
- **Parameters**: control_points (grid of Point3), knots_u (vec), knots_v (vec), degree_u (usize), degree_v (usize), weights (optional grid of f64)
- **Eval cost**: O(p·q) per point via Cox-de Boor recursion [#32] Algorithm A3.5
- **SSI strategy**: Numerical — topology-guaranteed tracing [#25] + IATA [#29] fallback (ADR-2). Hybrid B-Rep/mesh pipeline (ADR-1)
- **Tessellation**: Curvature-adaptive with flatness bounds from control point analysis [#31] Theorem C.3 (ADR-9). R^6 embedding for curvature adaptation [#34]
- **Status**: Not yet in `SurfaceGeom` enum. Implementation requires: evaluation (A3.5), derivatives (A3.6–A3.8), point inversion (A6.1), knot insertion (A5.1), all from [#32]

---

## SurfaceGeom Enum Design

### Current

```rust
pub enum SurfaceGeom {
    Planar { origin: Point3, normal: Vector3, u_axis: Vector3 },
    Cylindrical { origin: Point3, axis: Vector3, radius: f64 },
}
```

### Target

```rust
pub enum SurfaceGeom {
    // Tier 1 — Analytic (O(1) eval, exact SSI for all pairs)
    Planar { origin: Point3, normal: Vector3, u_axis: Vector3 },
    Cylindrical { origin: Point3, axis: Vector3, radius: f64 },
    Conical { origin: Point3, axis: Vector3, half_angle: f64, apex_distance: f64 },
    Spherical { center: Point3, radius: f64 },
    Toroidal { center: Point3, axis: Vector3, major_radius: f64, minor_radius: f64 },

    // Tier 3 — Freeform (universal fallback)
    BSpline {
        control_points: Vec<Vec<Point3>>,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        degree_u: usize,
        degree_v: usize,
        weights: Option<Vec<Vec<f64>>>,
    },

    // Tier 2 — Procedural (added incrementally, after Tier 1 + Tier 3)
    // Swept { profile: ..., spine: ..., orientation: ... },
    // Spun { profile: ..., axis: ..., angle_range: ... },
    // ... deferred until needed
}
```

**Note**: Tier 3 (BSpline) is implemented before Tier 2 (procedural) because:
- BSpline is required as the conversion target for procedural surfaces
- SSI between any Tier 2 surface and another surface goes through NURBS conversion
- Tier 2 types have value only after the NURBS evaluation pipeline exists

---

## SurfaceEval Trait

```rust
/// Unified surface evaluation interface for all tiers.
pub trait SurfaceEval {
    /// Evaluate surface point at parameter (u, v).
    fn evaluate(&self, u: f64, v: f64) -> Point3;

    /// Surface normal at parameter (u, v).
    fn normal(&self, u: f64, v: f64) -> Vector3;

    /// Partial derivatives: (dS/du, dS/dv) at parameter (u, v).
    fn derivatives(&self, u: f64, v: f64) -> (Vector3, Vector3);

    /// Find parameter (u, v) for a given 3D point (inverse evaluation).
    /// Returns None if point is not on surface within tolerance.
    fn point_inversion(&self, point: &Point3, tolerance: f64) -> Option<(f64, f64)>;

    /// Parameter domain bounds: ((u_min, u_max), (v_min, v_max)).
    fn parameter_bounds(&self) -> ((f64, f64), (f64, f64));

    /// Surface tier (1, 2, or 3).
    fn tier(&self) -> u8;
}
```

---

## Boolean Pipeline Integration

How the boolean pipeline handles surface type combinations (A15, A15.5):

| Face A Tier | Face B Tier | SSI Strategy | New Face Tier | Unmodified Face Tier |
|-------------|-------------|-------------|---------------|---------------------|
| 1 (Analytic) | 1 (Analytic) | Exact closed-form [#1] | 1 (Analytic) | 1 (preserved) |
| 1 (Analytic) | 2 (Procedural) | Convert Tier 2 → NURBS, then SSI | 3 (NURBS) | 1 or 2 (preserved) |
| 1 (Analytic) | 3 (NURBS) | Analytic-NURBS SSI | 3 (NURBS) | 1 (preserved) |
| 2 (Procedural) | 2 (Procedural) | Convert both → NURBS, then SSI | 3 (NURBS) | 2 (preserved) |
| 2 (Procedural) | 3 (NURBS) | Convert Tier 2 → NURBS, then SSI | 3 (NURBS) | 2 or 3 (preserved) |
| 3 (NURBS) | 3 (NURBS) | Numerical SSI [#25, #29] | 3 (NURBS) | 3 (preserved) |

**Key invariant (A15.5)**: Unmodified faces passing through a boolean retain their
original surface tier. Only new intersection faces take the highest tier of the
two intersecting surfaces.

---

## Tessellation Strategy Per Tier

| Tier | Strategy | LOD / View-Dependent | Triangle Count |
|------|----------|---------------------|----------------|
| 1 — Analytic | Direct from parameters: angular subdivision for curved surfaces, minimal for planes | Yes — angular tolerance adapts to screen-space pixel size | Low (exact curvature known) |
| 2 — Procedural | Evaluate along construction recipe; adaptive along spine/profile curvature | Yes — sample density along spine adapts to view distance | Medium (curvature estimated from recipe) |
| 3 — Freeform | Curvature-adaptive from control point flatness bounds [#31, #34] | Yes — subdivision depth adapts to screen-space error | High (curvature computed per patch) |

**View-dependent tessellation** (future): Tessellation tolerance can be computed
from camera distance and viewport resolution. Closer faces get finer tessellation.
This is standard in Parasolid (PK_TOPOL_facet tolerance parameter), ACIS, and OCCT.
Not implemented in current Waffle Iron kernel but architecture supports it via
per-face tolerance in `TessellationOptions`.

---

## Implementation Priority

1. **Complete Tier 1 enum** — Add `Conical`, `Spherical`, `Toroidal` variants to `SurfaceGeom`. Wire into tessellation and SSI dispatch. This unblocks the remaining 11 quadric SSI pairs (A15.4).

2. **Add Tier 3 BSpline** — Implement `BSpline` variant with NURBS evaluation [#32]. Required as conversion target before Tier 2 types can be added. Enables freeform surface import and the numerical SSI path (ADR-1, ADR-2).

3. **Add Tier 2 Swept/Spun** — Procedural surface types for extrude/revolve results that don't reduce to Tier 1. Enables recipe editing and re-evaluation.

4. **Remaining Tier 2 types** — Ruled, Lofted, Offset, Pipe. Added incrementally as modeling operations demand them. Each is deferred until the corresponding modeling operation is implemented.

---

## Invariants

1. **Tier never decreases**: A surface's tier can only increase (1→2, 1→3, 2→3), never decrease. Converting a BSpline to an analytic type is forbidden (it may not be exact). Detection of analytic surfaces within BSpline data is a separate recognition step, not a conversion.

2. **Evaluate/point_inversion round-trip**: For any surface S and valid parameter (u, v):
   `S.point_inversion(S.evaluate(u, v), TAU_MODEL) ≈ (u, v)` within parameter tolerance.

3. **Normal consistency**: Surface normals must be continuous and consistent with the cross product of partial derivatives: `normal(u, v) ∝ dS/du × dS/dv`.

4. **Boolean tier preservation (A15.5)**: Unmodified faces retain their surface tier through boolean operations.

5. **Analytical primacy (A15)**: Tier 1 × Tier 1 boolean operations use exact SSI, never mesh fallback.

---

## Cross-References

- **A15** (`governance/ARCHITECTURAL_INVARIANTS.md`): Analytical Primacy — exact SSI for quadric pairs
- **A15.5** (`governance/ARCHITECTURAL_INVARIANTS.md`): Surface tier preservation through booleans
- **ADR-1** (`docs/ALGORITHM_DECISIONS.md`): Hybrid B-Rep/mesh boolean pipeline
- **ADR-2** (`docs/ALGORITHM_DECISIONS.md`): SSI architecture (topology-guaranteed + IATA)
- **ADR-9** (`docs/ALGORITHM_DECISIONS.md`): Curvature-adaptive tessellation
- **ADR-10** (`docs/ALGORITHM_DECISIONS.md`): Analytical Primacy decision
- **ADR-11** (`docs/ALGORITHM_DECISIONS.md`): Surface representation strategy (this spec's ADR)
- **[#1]** Patrikalakis — SSI algorithms for quadric pairs
- **[#32]** Piegl & Tiller — NURBS algorithms
- **[#33]** Stroud — B-Rep modelling techniques, sweep/revolve, data definitions
- **[#36]** Parasolid — 3-tier surface hierarchy, convergent modeling
- **[#37]** Mistry — Swept volume B-Rep construction

---

*Last updated: 2026-03-16*
