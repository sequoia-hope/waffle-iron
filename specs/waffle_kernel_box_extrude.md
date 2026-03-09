# Spec: WaffleKernel Box Extrude + Flat-Face Tessellation

**Status:** Draft
**Author:** Claude Code
**Date:** 2026-03-09

## Goal

Implement `make_faces_from_profiles` (rectangular profiles only), `extrude_face` (linear extrude), `tessellate`, `extract_edges`, and `KernelIntrospect` methods for the resulting box solid in the clean-sheet WaffleKernel.

## Parameters

| Parameter | Type | Units | Range | Default |
|-----------|------|-------|-------|---------|
| profile rect (cx, cy, w, h) | f64 | meters | w,h > 0 | -- |
| plane_origin | [f64;3] | meters | any | -- |
| plane_normal | [f64;3] | unit vector | \|n\|=1 | -- |
| plane_x_axis | [f64;3] | unit vector | perpendicular to normal | -- |
| direction | [f64;3] | unit vector | \|d\|=1 | -- |
| depth | f64 | meters | > 0 | -- |
| tolerance (tessellate) | f64 | meters | > 0 | -- |

## Branch Table

| Case | Profile | Depth | Direction | Expected |
|------|---------|-------|-----------|----------|
| Unit box | 1x1 rect at origin | 1.0 | +Z | 8V, 12E, 6F, vol=1.0 |
| Scaled box | 2x3 rect | 5.0 | +Z | 8V, 12E, 6F, vol=30.0 |
| Off-origin | rect centered at (10,20) | 1.0 | +Z | same topo, shifted bbox |
| Non-Z normal | rect on XY | 1.0 | +Y | same topo, Y-extruded |
| Micro scale | 1e-4 x 1e-4 rect | 1e-4 | +Z | vol=1e-12 |
| Macro scale | 1e3 x 1e3 rect | 1e3 | +Z | vol=1e9 |

## Invariants

- V - E + F = 2 (Euler-Poincare for genus-0 solid)
- Volume = w * h * depth (within 1% tolerance)
- Watertight mesh (every mesh edge shared by exactly 2 triangles)
- 6 faces, 12 edges, 8 vertices for any rectangular extrude
- Face normals point outward (positive volume via divergence theorem)
- Bounding box matches profile extent + depth along direction

## Oracles

- `mesh_volume()` (divergence theorem) ~ w*h*depth +/- 1%
- `check_watertight()` = true
- `topology_counts()` = (8, 12, 6)
- `mesh_bbox()` matches expected corners +/- tolerance
- Euler formula from introspection: V-E+F = 2

## Failure Modes

- Zero-width or zero-height profile -> `KernelError::Other`
- Zero depth -> `KernelError::Other`
- Non-unit normal -> normalize internally
- Face ID not found in `extrude_face` -> `KernelError::EntityNotFound`

## Research Basis

- [#16] Mantyla -- Euler operators (mvfs, mev, mef) for topology construction
- [#33] Stroud Ch.4 -- Half-edge B-Rep fundamentals
- Fan triangulation for convex planar faces -- standard CG technique
