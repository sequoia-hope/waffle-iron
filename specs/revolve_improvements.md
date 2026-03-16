# Spec: Revolve Improvements — Axis Fix, Circle Profiles, Full 360°

**Status:** Draft
**Author:** Claude Code
**Date:** 2026-03-16

## Goal

Three improvements to the revolve operation:

1. **Axis Selection Bug Fix** — Fix `computePlaneBasis()` in RevolveDialog.svelte to match the canonical algorithm used in `buildSketchPlane()` and `tangent_x_from_normal()`.
2. **Circle Profile Segmentation** — Approximate circle profiles as 64-segment polygons (matching `CIRCLE_SEGMENTS` in `tessellation/mod.rs`) before revolving, instead of the previous 32.
3. **360° Full Revolution** — Support full revolutions (angle = 360°) with cap-less, welded-seam topology (genus-1 closed surface).

## Parameters

| Parameter | Type | Units | Range | Default |
|-----------|------|-------|-------|---------|
| face | KernelId | -- | valid face ID | -- |
| axis_origin | [f64; 3] | meters | any | -- |
| axis_direction | [f64; 3] | unit vector | \|d\|=1 | -- |
| angle | f64 | degrees | 0 < θ ≤ 360 | -- |

## Branch Table

| Case | Angle | Profile | Result |
|------|-------|---------|--------|
| Partial polygon revolve | 0 < θ < 360 | Polygon (M vertices) | V=2M, E=3M, F=M+2, χ=2 |
| Full polygon revolve | θ = 360 | Polygon (M vertices) | V=M, E=2M, F=M, χ=0 |
| Partial circle revolve | 0 < θ < 360 | Circle (64-gon) | V=128, E=192, F=66, χ=2 |
| Full circle revolve | θ = 360 | Circle (64-gon) | V=64, E=128, F=64, χ=0 |
| Near-360 | θ = 359.9 | Polygon | Partial (with caps), V=2M, E=3M, F=M+2, χ=2 |
| angle ≤ 0 | θ ≤ 0 | any | Error: KernelError::Other |

### Topology Derivation

**Partial revolve (0 < θ < 360), M profile vertices:**
- 2 cap faces (start, end profile copies) + M lateral faces = M+2 faces
- 2M cap edges (M per cap) + M arc edges (one per vertex) = 3M edges
- M start vertices + M end vertices = 2M vertices
- χ = 2M - 3M + (M+2) = 2 ✓ (genus-0 solid)

**Full revolve (θ = 360), M profile vertices:**
- No cap faces (start and end profiles are identified)
- M lateral faces
- M lateral "parallel" edges + M "meridian" edges = 2M edges
- Start and end vertex rings are identified (shared): M vertices
- χ = M - 2M + M = 0 ✓ (genus-1 closed surface, torus-like)

## Improvement 1: Axis Selection Bug Fix

### Problem

`computePlaneBasis()` in `RevolveDialog.svelte` computes the sketch plane's X-axis using an algorithm that does not match the canonical algorithm in `buildSketchPlane()` and `tangent_x_from_normal()`.

### Canonical Algorithm

```
ref = |n·Z| < 0.99 ? Z : X
xAxis = normalize(cross(ref, n))
```

Where `n` is the plane normal, `Z = (0,0,1)`, `X = (1,0,0)`.

### Fix

Replace `computePlaneBasis()` with the canonical algorithm above. All three sites must produce identical results:
- `RevolveDialog.svelte` — `computePlaneBasis()`
- `buildSketchPlane()` — sketch plane construction
- `tangent_x_from_normal()` — kernel tangent computation

### Invariant

For any input normal `n`, all three functions return the same `xAxis` vector (bitwise identical).

## Improvement 2: Circle Profile Segmentation

### Problem

Circle profiles are approximated as 32-segment polygons before revolving. The tessellation module uses `CIRCLE_SEGMENTS = 64` for cylinder tessellation. The mismatch means revolve-of-circle produces coarser geometry than extrude-of-circle.

### Fix

Change circle profile approximation to 64 segments, matching the `CIRCLE_SEGMENTS` constant in `tessellation/mod.rs`.

### Impact on Topology

For a partial circle revolve (64-gon):
- V = 2 × 64 = 128
- E = 3 × 64 = 192
- F = 64 + 2 = 66
- χ = 128 - 192 + 66 = 2 ✓

For a full circle revolve (64-gon, torus):
- V = 64
- E = 2 × 64 = 128
- F = 64
- χ = 64 - 128 + 64 = 0 ✓

## Improvement 3: 360° Full Revolution

### Problem

Full revolutions (angle ≥ 360°) are currently rejected with `NotSupported`. The seam topology requires identifying (welding) the first and last vertex rings instead of creating separate cap faces.

### Fix

For `angle == 360.0`:
1. **No start/end caps** — the surface is closed, no boundary.
2. **Identify first and last vertex rings** — the M vertices at θ=0 and θ=360 are the same M vertices (shared, not duplicated).
3. **Lateral faces wrap** — each lateral face's "end" edge is the same as the "start" edge of the next ring position.

### Topology for Full Revolution

For M profile vertices:
- **Vertices:** M (single ring, shared between start/end)
- **Edges:** 2M (M meridian edges + M parallel edges)
- **Faces:** M (all lateral, no caps)
- **χ = M - 2M + M = 0** (genus-1, torus-like)

### Angle Threshold

Use exact comparison `angle == 360.0`. Near-360 values (e.g., 359.9) produce partial revolves with caps. This avoids ambiguity and floating-point edge cases.

## Invariants

1. **Euler characteristic:**
   - Partial revolve: χ = V - E + F = 2 (genus-0 solid)
   - Full 360° revolve: χ = V - E + F = 0 (genus-1 closed surface)
2. **Volume oracle (Pappus' centroid theorem):**
   V = θ_rad × R_centroid × Area
   where R_centroid is the distance from the profile centroid to the axis.
   Tolerance: within 5% for polygon profiles, within 5% for 64-gon circle profiles.
3. **Watertight tessellation mesh:** every mesh edge shared by exactly 2 triangles.
4. **Plane basis consistency:** `computePlaneBasis()`, `buildSketchPlane()`, and `tangent_x_from_normal()` all produce identical basis vectors for any input normal.
5. **Manifoldness:** every B-Rep edge has exactly 2 incident faces.

## Oracles

- **Euler formula check:** compute V, E, F from B-Rep topology; verify χ = 2 (partial) or χ = 0 (full).
- **Pappus volume:** `mesh_volume()` via divergence theorem matches `θ_rad × R_centroid × Area` within 5%.
- **Bounding box assertions:** for known geometries (unit square revolved 90° around Z-axis), verify bbox corners.
- **Manifoldness:** every edge in the half-edge structure has exactly 2 incident faces (no boundary, no non-manifold).
- **Watertight check:** `check_watertight()` on tessellated mesh returns true.
- **Basis consistency:** unit test comparing output of all three basis functions for a set of normals (Z-aligned, X-aligned, 45° oblique, near-degenerate).

## Failure Modes

| Condition | Error |
|-----------|-------|
| angle ≤ 0 | KernelError::Other("invalid angle") |
| Profile vertex lies on axis (zero radius) | KernelError::Other("degenerate: vertex on axis") |
| Non-axis-aligned profile edges | KernelError::NotSupported("conical surfaces deferred") |
| Nonexistent face ID | KernelError::EntityNotFound |

## Research Basis

- [#33] Stroud §6.2 — Sweep/revolve construction with Euler operators; seam edge handling for full revolutions
- [#16] Mantyla — Euler operators (mvfs, mev, mef, kemr, kfmrh) for solid modeling; genus-1 topology via kfmrh
- [#24] Barton — Pappus' centroid theorem for volume validation of revolution solids
