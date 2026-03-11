# Spec: Boolean Tessellation Normal Fix (Track A)

## Problem

Boolean result tessellations produce ~50-75% outward normals instead of 95%+.
Affects 12 assay cases: R0002, R0005, R0011, R0021, R0022, R0031, R0040, R0041,
R0045, R0066, R0080, R0089.

All are multi-extrude with merge/cut. No revolve, no engine errors — only fail
the `outward_normals` oracle.

## Root Cause

In `tessellate_polygon_face()` (tessellation/mod.rs), the flip decision compares
the Newell normal vs stored `plane.normal` and applies a SINGLE flip to ALL
triangles in the face. Boolean fragment faces have mixed vertex winding from the
clipping process, so a single flip corrects some triangles but reverses others.

## Fix

After tessellating each face (both convex fan and non-convex ear-clip paths),
add a per-triangle winding correction pass:

For each output triangle `(i0, i1, i2)`:
1. Compute geometric normal: `cross(v1-v0, v2-v0)`
2. Dot with stored face normal
3. If dot < 0, swap `i1` and `i2` (reverse winding)

This replaces the current single-flip approach with per-triangle correction.

## Oracles

- `check_normals_consistent(mesh)`: All triangles' geometric normals agree with
  stored normals (100%)
- `check_outward_normals(mesh, 0.95)`: At least 95% of triangles point outward
- `check_normals_outward(mesh)`: Returns (agree, disagree) counts

## Research References

- Ref #33 Stroud S14.1: Outward normal convention for closed solids
- Ref #2 Hoffmann Ch.3: Face normal orientation in B-Rep

## Tests (RED phase)

- BN1: rect boss + rect boss union → 100% consistent normals
- BN2: gear boss + rect cut → 100% consistent normals
- BN3: circle boss + circle cut → 100% consistent normals (via outward check)
- BN4: rect boss + rect boss union → outward normals ≥ 95%

## Expected Yield

12 assay cases move from fail to pass.
