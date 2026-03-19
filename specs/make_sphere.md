# Spec: `make_sphere` Primitive

**Status**: Implementation spec
**References**: [#16] Mantyla (Euler operators), [#33] Stroud (B-Rep primitive construction), [#1] Patrikalakis Ch.5 (SSI)
**Governance**: A15 (Analytical Primacy), A14 (Units in meters)

---

## 1. Goal

Add a `make_sphere(center, radius)` method to `WaffleKernel` that constructs a
closed B-Rep sphere solid using Euler operators. This is a fundamental CAD
primitive that enables sphere-based modeling operations and boolean combinations
with existing box and cylinder primitives.

## 2. Parameters

| Parameter | Type | Default | Units | Valid Range | Error |
|-----------|------|---------|-------|-------------|-------|
| `center` | `[f64; 3]` | — | meters | any finite | `InvalidInput` if NaN/Inf |
| `radius` | `f64` | — | meters | `> MIN_FEATURE_SIZE` (1e-6) | `InvalidInput` if ≤ 0 or too small |

## 3. Branch Table

| Branch | Condition | Expected Behavior |
|--------|-----------|-------------------|
| Valid sphere | radius > MIN_FEATURE_SIZE | Returns `KernelSolidHandle` for closed sphere solid |
| Zero/negative radius | radius ≤ 0 | Returns `InvalidInput` error |
| Tiny radius | 0 < radius ≤ MIN_FEATURE_SIZE | Returns `InvalidInput` error |
| NaN/Inf center | any component is NaN/Inf | Returns `InvalidInput` error |
| NaN/Inf radius | radius is NaN/Inf | Returns `InvalidInput` error |

## 4. Topology

Octahedral B-Rep decomposition:

- **6 vertices**: at (±r, 0, 0), (0, ±r, 0), (0, 0, ±r) offset by center
- **12 edges**: connecting adjacent octahedral vertices
- **8 faces**: spherical triangular patches, each tagged `SurfaceGeom::Spherical`

This topology:
- Satisfies Euler's formula: V - E + F = 6 - 12 + 8 = 2
- Has no degenerate edges (all edges have nonzero length = r√2)
- Is compatible with existing polygon-based boolean infrastructure
- Each face is a proper spherical patch (not a UV patch with pole singularities)

Construction uses Euler operators: `mvfs` → `mev` × 5 → `mef` × 7 → remaining faces.
Ref [#16] Mantyla: Euler operators guarantee topological validity.

## 5. Invariants

1. **Euler formula**: V - E + F = 2 (always)
2. **Volume**: Mesh volume ≈ 4/3 π r³ (within tessellation tolerance, ~1% for 64-segment)
3. **Bounding box**: AABB = [center - r, center + r] (exact for octahedral vertices)
4. **Watertight**: Tessellated mesh has no boundary edges
5. **Normal consistency**: All face normals point outward (away from center)
6. **Surface containment**: All tessellated vertices lie on sphere surface (within TAU_MODEL)
7. **Symmetry**: The 8 faces have equal area (π r² / 2 each, analytically)

## 6. Oracles

| Oracle | Assertion |
|--------|-----------|
| Volume | `abs(mesh_volume - 4/3 π r³) / (4/3 π r³) < 0.02` |
| Vertex count | `V = 6` |
| Edge count | `E = 12` |
| Face count | `F = 8` |
| Euler formula | `V - E + F = 2` |
| Bounding box | `bbox_min ≈ center - [r,r,r]`, `bbox_max ≈ center + [r,r,r]` |
| Watertight | No boundary edges in tessellated mesh |
| No degenerate tris | All triangles have area > 0 |
| All verts on sphere | `abs(dist(v, center) - r) < TAU_MODEL` for tessellation vertices |
| Normal direction | For each face, `dot(face_centroid - center, face_normal) > 0` |

## 7. Tessellation Strategy

Parametric spherical tessellation via recursive subdivision of octahedral faces:

1. Each triangular face is subdivided into a grid (controlled by CIRCLE_SEGMENTS)
2. Grid vertices are projected onto the sphere surface: `v' = center + r * normalize(v - center)`
3. Normals are radial: `n = normalize(v - center)`
4. No pole singularities (unlike UV parametrization)

This produces a watertight mesh with shared vertices at octahedral edges.

## 8. Failure Modes

| Failure | Error Type | Diagnostic |
|---------|-----------|------------|
| radius ≤ 0 | `InvalidInput` | "sphere radius must be positive" |
| radius ≤ MIN_FEATURE_SIZE | `InvalidInput` | "sphere radius below minimum feature size" |
| NaN/Inf inputs | `InvalidInput` | "sphere parameters contain NaN or infinity" |

## 9. Research Basis

- **[#16] Mantyla**: Euler operators for primitive solid construction. The octahedral
  sphere uses the same `mvfs`/`mev`/`mef` sequence as box construction.
- **[#33] Stroud §4.2**: B-Rep data structures for curved surface primitives.
  Sphere as a single Spherical face with seam edges is the minimal representation,
  but octahedral decomposition provides better boolean compatibility.
- **[#1] Patrikalakis Ch.5**: SSI algorithms for plane-sphere and sphere-sphere
  pairs are already implemented. Sphere primitive enables testing these paths.

## 9a. Analytical vs. Approximate Method Justification

- **Method**: Exact (closed-form geometry).
- **Justification**: The sphere surface is stored as `SurfaceGeom::Spherical` with
  exact center and radius. No approximation is involved in the B-Rep representation.
  Tessellation is the only approximate step (controlled by segment count).
- **Surface pair coverage**: Sphere boolean operations use existing exact SSI solvers:
  - Plane-Sphere: Circle (implemented in `ssi.rs`)
  - Sphere-Sphere: Circle (implemented in `ssi.rs`)
  - Cylinder-Sphere: Degree ≤ 4 curve (to be implemented separately, returns NotSupported until then)

---

*Spec complete. Ready for Test Phase.*
