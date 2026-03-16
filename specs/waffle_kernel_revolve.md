# WaffleKernel Revolve — True Analytic Surfaces

## Goal

Revolve a polygon profile around an axis to create a solid with true analytic surfaces (cylindrical and planar faces, circular arc edges).

## Parameters

- `face`: KernelId — standalone face (polygon profile)
- `axis_origin`: [f64; 3] — point on the revolution axis
- `axis_direction`: [f64; 3] — direction of the revolution axis (normalized internally)
- `angle`: f64 — revolution angle in degrees (>0, ≤360)

## Branch Table

| Case | Result |
|------|--------|
| Partial polygon revolve (0 < angle < 360) | Implement |
| Full 360° revolution (angle = 360) | Implement (cap-less, welded seam; V=M, E=2M, F=M, χ=0) |
| Circle profile | Implement (64-gon approximation, matching CIRCLE_SEGMENTS) |
| Non-axis-aligned profile edges | NotSupported (conical surfaces deferred) |

## Topology (rect profile, M=4 vertices)

| Entity | Count | Formula |
|--------|-------|---------|
| Vertices | 2M = 8 | |
| Edges | 3M = 12 | |
| Faces | M+2 = 6 | |
| Euler | V-E+F = 2 | |

### Face Types

- 2 planar cap faces (start and end profile rectangles)
- M lateral faces, each either:
  - **Cylindrical**: constant radius from axis (profile edge parallel to axis)
  - **Planar annular**: constant height along axis (profile edge perpendicular to axis)

### Edge Types

- 2M = 8 linear edges (cap polygon edges)
- M = 4 circular arc edges (one per profile vertex, sweeping the revolution angle)

## Topology (full 360° revolution, M vertices)

| Entity | Count | Formula |
|--------|-------|---------|
| Vertices | M | Start/end rings identified (shared) |
| Edges | 2M | M meridian + M parallel |
| Faces | M | All lateral, no caps |
| Euler | V-E+F = 0 | Genus-1 closed surface |

## Volume Oracle (Pappus' Centroid Theorem)

V = angle_rad x R_centroid x Area

Where R_centroid is the distance from the profile centroid to the axis.

## Invariants

- V-E+F = 2 for partial revolves (Euler characteristic for genus-0 solid)
- V-E+F = 0 for full 360° revolves (genus-1 closed surface)
- Watertight tessellation mesh
- Volume matches Pappus formula within 5% tolerance
- Every B-Rep edge has exactly 2 incident faces (manifold)

## Failure Modes

- angle <= 0 -> KernelError::Other
- Nonexistent face -> KernelError::EntityNotFound
- Profile vertex on axis (degenerate, zero radius) -> KernelError::Other
- Non-axis-aligned profile edges -> KernelError::NotSupported
