# FIP: Lazy Exact Escalation for Boolean Predicates

**Status:** Implementing
**Phase:** C (Boolean Precision Improvement Roadmap)
**Classification:** Refactor (DoD 3) — no behavior change for well-conditioned inputs

## Problem

Three critical functions in the boolean pipeline use floating-point arithmetic
for geometric predicates where catastrophic cancellation can produce wrong signs:

1. **`solid_angle`** (winding.rs) — scalar triple product for winding number sign
2. **`is_midpoint_on_face_boundary`** (loops_store/mod.rs) — point-to-segment distance for IC filtering
3. **`check_coplanar`** (coplanar.rs) — angular parallelism test for normal comparison

The `robust` crate (Shewchuk adaptive-precision predicates) is already integrated
in `robust_classify.rs` and used by `robust_ray_triangle_cross`, `point_in_polygon`,
`check_coplanar` (distance only), and `exact_points_coplanar`. But the three
functions above still use raw floating-point for sign-critical computations.

## Solution: Lazy Exact Escalation

Compute the fast floating-point result first, escalate to exact predicates only
when the result is ambiguous (near zero). This gives exact correctness with
minimal performance overhead.

### Target 1: `solid_angle` sign robustness

The scalar triple product `numerator = pa . (pb x pc)` suffers catastrophic
cancellation when point `p` is near the triangle plane. A wrong sign flips the
solid angle contribution, potentially flipping inside/outside classification.

**Fix:** Compute an error bound for the FP triple product. When
`|numerator| <= error_bound`, call `robust_orient3d(a, b, c, p)` for the exact
sign. If exact sign disagrees with FP sign, correct it.

### Target 2: `is_midpoint_on_face_boundary` exact collinearity

Uses `(mid - closest).magnitude() < boundary_tol` — floating-point distance
comparison. When IC midpoints are exactly on boundary edges, rounding errors
may push the result above or below the tolerance threshold.

**Fix:** Add exact collinearity pre-check using `robust_orient2d` on the
dominant 2D projection plane. If the point is exactly collinear with the edge
and within parametric bounds, return true immediately.

### Target 3: `check_coplanar` angular parallelism

Uses `(1.0 - dot.abs()) > tol * tol` — floating-point dot product of normals.
For faces with very large coordinates, the normal computation and dot product
may accumulate errors.

**Fix:** Add exact parallelism pre-check. Test if `n0 x n1 == 0` using
`robust_orient2d` on all three coordinate plane projections (XY, XZ, YZ).

## New Functions in `robust_classify.rs`

### `lazy_exact_triple_sign`

```rust
pub(crate) fn lazy_exact_triple_sign(
    p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3],
    fp_value: f64,
) -> (i32, f64)
```

Returns `(exact_sign, corrected_fp_value)`. Only calls `robust_orient3d` when
`|fp_value|` is within the computed error bound (24 * eps * max_coord^3).

### `exact_point_on_segment_3d`

```rust
pub(crate) fn exact_point_on_segment_3d(
    p: [f64; 3], a: [f64; 3], b: [f64; 3],
) -> bool
```

Projects to dominant 2D plane, uses `robust_orient2d` for collinearity, then
checks parametric containment. Returns true only if exactly on segment.

### `exact_vectors_parallel`

```rust
pub(crate) fn exact_vectors_parallel(
    a: [f64; 3], b: [f64; 3],
) -> bool
```

Tests `a x b == 0` via `robust_orient2d` on XY, XZ, YZ projections. Returns
true if all three return 0 (exactly parallel or anti-parallel).

## Files Modified

| File | Change |
|------|--------|
| `robust_classify.rs` | Add 3 utility functions + unit tests |
| `winding.rs` | Call `lazy_exact_triple_sign` in `solid_angle` |
| `loops_store/mod.rs` | Call `exact_point_on_segment_3d` as pre-check |
| `coplanar.rs` | Call `exact_vectors_parallel` as pre-check |

## Risks

| Risk | Mitigation |
|------|-----------|
| Error bound too loose (frequent escalation) | Conservative 24x factor; orient3d fast filter is O(1) |
| Error bound too tight (missed corrections) | Adversarial tests with known cancellation cases |
| exact_point_on_segment false negatives | Pre-check only; tolerance still runs as fallback |
