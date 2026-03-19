# Spec: make_cone Primitive

## Goal

Create a right circular cone solid from base center, axis direction, base radius,
and height. The cone apex is at `center + axis * height`.

## Parameters

| Parameter | Type | Constraints |
|-----------|------|-------------|
| `center` | `[f64; 3]` | Base center; all components finite |
| `axis` | `[f64; 3]` | Unit direction from base to apex; finite, non-zero |
| `radius` | `f64` | Base circle radius; finite, positive, ≥ MIN_FEATURE_SIZE |
| `height` | `f64` | Cone height; finite, positive, ≥ MIN_FEATURE_SIZE |

Derived: `half_angle = atan(radius / height)`

## Branch Table

| # | Condition | Result |
|---|-----------|--------|
| B1 | Valid inputs (r > 0, h > 0, r ≥ MIN_FEATURE_SIZE, h ≥ MIN_FEATURE_SIZE) | Cone solid |
| B2 | radius ≤ 0 or not finite | Error: invalid radius |
| B3 | height ≤ 0 or not finite | Error: invalid height |
| B4 | radius < MIN_FEATURE_SIZE | Error: below minimum feature size |
| B5 | height < MIN_FEATURE_SIZE | Error: below minimum feature size |
| B6 | center contains non-finite | Error: invalid center |
| B7 | axis is zero-length or non-finite | Error: invalid axis |

## B-Rep Topology (B1 — Normal Case)

Using 4 base vertices (quad decomposition, matching sphere's octahedral approach):

- **Vertices**: 5 total — 1 apex + 4 base circle points
- **Edges**: 8 total — 4 base edges + 4 lateral edges (apex to base)
- **Faces**: 5 total — 4 lateral triangular faces + 1 base quadrilateral face
- **Euler**: V - E + F = 5 - 8 + 5 = 2 ✓

Base vertices at 90° intervals: +U, +V, -U, -V (where U,V are perpendicular to axis).

## Surface Geometry

- **Lateral faces**: `SurfaceGeom::Conical(Cone { origin: apex, axis: -axis, half_angle, apex_distance: 0.0 })`
  - The cone's origin is at the apex, axis points downward (toward base)
  - half_angle = atan(radius / height)
- **Base face**: `SurfaceGeom::Planar(Plane { origin: center, normal: -axis, u_axis })`
  - Normal points outward (away from interior = downward)

## Curve Geometry

- **Base edges**: `CurveGeom::Arc` (quarter-circle arcs on the base plane)
- **Lateral edges**: `CurveGeom::Line` (straight lines from apex to base vertices)

## Invariants

1. Euler formula: V - E + F = 2
2. Watertight mesh (all edges paired in tessellation)
3. Positive signed volume
4. Volume = (1/3) π r² h (within 5% for tessellated approximation)
5. Centroid at center + axis * (h/4) (cone centroid is 1/4 height from base)
6. All lateral faces have SurfaceGeom::Conical
7. Base face has SurfaceGeom::Planar

## Oracles

| Oracle | Formula | Tolerance |
|--------|---------|-----------|
| Volume | (1/3) π r² h | ±5% (tessellation approximation) |
| Centroid Z | h/4 from base | ±10% |
| Face count | 5 | exact |
| Edge count | 8 | exact |
| Vertex count | 5 | exact |
| Watertight | 0 unpaired edges | exact |

## Failure Modes

- Degenerate cone (r ≈ 0 or h ≈ 0) — caught by MIN_FEATURE_SIZE check
- Non-unit axis — normalize internally
- Numerical issues with very flat cones (large r, small h) or very thin cones (small r, large h)

## Research Basis

- Mantyla [#16]: Euler operator construction
- Stroud [#33]: Cone as analytic quadric surface
- ADR-11 (surface_type_taxonomy.md): Cone is Tier 1 analytic
