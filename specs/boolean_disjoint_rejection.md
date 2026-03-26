# Spec: Disjoint Boolean Union Returns Error

## Goal

Boolean union of AABB-disjoint operands returns `KernelError::BooleanFailed`
instead of producing a topologically invalid two-shell solid (chi=4).

## Branch Table

| Operation  | Disjoint operands | Result                        |
|------------|-------------------|-------------------------------|
| Union      | disjoint          | `BooleanFailed { reason }`    |
| Subtract   | disjoint          | Operand A unchanged           |
| Intersect  | disjoint          | Empty result / `BooleanFailed`|

## Rationale

A single `WaffleSolid` must be one connected shell (Euler chi=2). Disjoint
union produces two disconnected shells (chi=4), which is topologically invalid.
Real CAD systems (SolidWorks, Fusion 360) reject disjoint unions as user errors.

## Invariants

- AABB disjointness check uses adaptive tau margin — conservative (no false rejections).
- Subtract and intersect of disjoint operands remain valid (unchanged behavior).
- The AABB check itself is not modified; only the action taken for union changes.

## Error Format

```rust
KernelError::BooleanFailed {
    reason: "operands are disjoint (bounding boxes do not overlap)".into(),
}
```

Reuses existing `BooleanFailed` variant — no new error type needed.

## Affected Code Paths

1. `boolean_op_from_polys_inner` (polygon clipping path)
2. `planar_planar_boolean` (exact planar path)
3. `box_cyl_boolean` (box-cylinder SSI)
4. `box_sphere_boolean` (box-sphere SSI)
5. `sphere_sphere_boolean` (sphere-sphere SSI)
6. `cyl_cyl_boolean_z_aligned` (cylinder-cylinder SSI)
7. `waffle_kernel.rs` fallback chain (short-circuit disjoint errors)
