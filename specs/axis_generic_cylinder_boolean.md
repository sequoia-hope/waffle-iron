# Axis-Generic Cylinder-Cylinder Boolean

## Summary

Enable cylinder-cylinder boolean operations for arbitrary parallel axis directions
(not just Z-aligned) using a frame-rotation approach.

## Parameters

- **Input**: Two `CylinderParams` with parallel axis directions, a `BoolOp`
- **Output**: `BooleanResult` with topology and geometry in the original coordinate frame
- **Tolerance**: TAU_WORK=1e-12 for FP comparisons; parallelism check uses 1e-9

## Approach: Rotate → Process → Rotate-back

1. Check axes are parallel (`|dot(dir_a, dir_b)| > 1 - 1e-9`)
2. Compute rotation matrix `M` that maps `dir_a → [0,0,1]` (Rodrigues' formula)
3. Rotate both cylinder params into Z-aligned frame: `cyl_a_z = M · cyl_a`, `cyl_b_z = M · cyl_b`
4. Perform boolean using existing Z-assumption logic (SSI, build functions)
5. Rotate result back: `result = M⁻¹ · result_z` (M⁻¹ = Mᵀ for orthonormal M)

For Z-aligned inputs, `rotation_to_z([0,0,1])` returns the identity matrix exactly,
preserving existing behavior bit-for-bit.

## Branch Table

| Condition | Behavior |
|-----------|----------|
| Parallel, Z-aligned | Identity rotation, zero overhead |
| Parallel, X/Y/arbitrary | Rotate to Z, process, rotate back |
| Antiparallel (e.g., [0,0,-1]) | 180° rotation around X axis |
| Non-parallel | Return `NotSupported` |

## Invariants

- Rotation is an isometry: distances, angles, volumes preserved
- `M · Mᵀ = I` (orthonormal rotation matrix)
- Euler characteristic V-E+F=2 preserved through rotation
- Watertightness preserved (rotation doesn't create gaps)

## Failure Modes

- Non-parallel cylinders: explicit `NotSupported` error
- Near-parallel (within 1e-9): treated as parallel, rotation may introduce ~1e-12 error

## Research Citations

- **Ref #24 Barton (2018)**: Unit-cube/frame normalization before boolean
- **Ref #6 Sugihara-Iri (2000)**: Isometric transforms preserve manifoldness
- **Ref #4 Shewchuk (1997)**: Rotation well-conditioned; FP error O(ε·‖v‖)
- **Ref #1 Patrikalakis Ch.5**: Non-parallel cylinder SSI produces elliptical curves (unsupported)

## Numerical Stability

Orthonormal rotation matrices have condition number 1. Per Shewchuk, FP error per
coordinate is bounded by ~6.7e-13 for typical CAD geometry (coords < 1000), well
below TAU_WORK=1e-12.

## Scope

- **In scope**: Parallel-axis cylinder-cylinder booleans (all operations)
- **Out of scope**: Non-parallel cylinders, box-cylinder with non-Z cylinders (Phase 2)
