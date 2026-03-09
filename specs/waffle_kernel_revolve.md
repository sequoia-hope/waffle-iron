# WaffleKernel Revolve — True Analytic Surfaces

## Goal

Revolve a polygon profile around an axis to create a solid with true analytic surfaces (cylindrical and planar faces, circular arc edges).

## Parameters

- `face`: KernelId — standalone face (polygon profile)
- `axis_origin`: [f64; 3] — point on the revolution axis
- `axis_direction`: [f64; 3] — direction of the revolution axis (normalized internally)
- `angle`: f64 — revolution angle in degrees (>0, <360)

## Branch Table

| Case | Result |
|------|--------|
| Partial polygon revolve (0 < angle < 360) | Implement |
| Full 360 revolution | NotSupported (seam topology needed) |
| Circle profile | NotSupported (torus needs genus-1 topology) |
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

## Volume Oracle (Pappus' Centroid Theorem)

V = angle_rad x R_centroid x Area

Where R_centroid is the distance from the profile centroid to the axis.

## Invariants

- V-E+F = 2 (Euler characteristic for genus-0 solid)
- Watertight tessellation mesh
- Volume matches Pappus formula within 5% tolerance

## Failure Modes

- angle <= 0 or >= 360 -> KernelError::Other
- Circle profile -> KernelError::NotSupported
- Nonexistent face -> KernelError::EntityNotFound
- Non-axis-aligned profile edges -> KernelError::NotSupported
