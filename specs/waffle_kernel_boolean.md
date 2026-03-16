# Spec: WaffleKernel Box-Box Boolean Operations

## Overview

Box-box boolean operations (union, subtract, intersect) using convex
face-polygon clipping with Sutherland-Hodgman algorithm.

## Parameters

| Param | Type | Description |
|-------|------|-------------|
| solid_a | KernelSolidHandle | First operand |
| solid_b | KernelSolidHandle | Second operand |
| op | BoolOp | Union / Subtract / Intersect |

## Branch Table

| Condition | Behavior |
|-----------|----------|
| Both operands are boxes | Perform face-polygon clipping boolean |
| Either operand has cylinder_params | Return NotSupported |
| Invalid solid handle | Return EntityNotFound |
| Result has no faces (e.g., identical subtract) | Return BooleanFailed |
| Disjoint boxes + intersect | Return BooleanFailed (empty result) |
| Disjoint boxes + union | Return valid 12-face solid |
| Identical boxes + union | Return valid 6-face solid (same as input) |

## Invariants

1. Result satisfies Euler formula: V - E + F = 2
2. Result mesh is watertight (every edge shared by exactly 2 triangles)
3. Union volume = vol(A) + vol(B) - vol(A ∩ B)
4. Subtract volume ≤ vol(A)
5. Intersect volume ≤ min(vol(A), vol(B))
6. All result faces have planar geometry assigned
7. All result edges have linear geometry assigned

## Algorithm

1. Extract face polygons from each solid's B-Rep
2. Classify each face against opposing solid via Sutherland-Hodgman clipping
3. Select face fragments based on operation type
4. Build result B-Rep from polygon soup (vertex welding → twin pairing)

## Failure Modes

- `KernelError::NotSupported` — cylinder operand
- `KernelError::EntityNotFound` — invalid handle
- `KernelError::BooleanFailed` — empty result or manifold violation

## Research Basis

- **[#16] Mantyla** — Euler operators for constructing the result B-Rep topology.
- **[#33] Stroud §6.1** — Boolean pipeline: SSI → classification → assembly.

### Analytical Primacy (A15) — Not Applicable

Box-box booleans use Sutherland-Hodgman polygon clipping on planar face polygons.
This is NOT a violation of A15 (governance/ARCHITECTURAL_INVARIANTS.md) because:

1. All faces of both operands are **planar** — no quadric surfaces are involved.
2. Sutherland-Hodgman is **exact** for convex planar polygons (no mesh
   approximation or tessellation occurs).
3. No surface geometry is lost — input faces are planar and result faces remain
   planar with exact `SurfaceGeom::Planar` geometry.

The A15 invariant governs quadric surfaces (cylinder, cone, sphere, torus) where
mesh approximation destroys analytical geometry. Planar polygon clipping preserves
exact geometry by construction.
