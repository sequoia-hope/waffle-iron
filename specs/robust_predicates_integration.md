# Spec: Robust Geometric Predicates Integration

**Burndown ID**: A3
**Author**: robust-predicates
**Status**: Draft

## Problem

The ray-cast classification in `truck-shapeops/src/transversal/integrate/mod.rs` uses
naive floating-point comparisons (`signed_crossing_faces`). When a ray grazes a triangle
edge or a point is nearly coplanar with a face, floating-point error can cause
misclassification. The current mitigation (irrational ray directions + majority voting)
helps but doesn't eliminate the problem.

The coplanar overlap detection in `coplanar.rs` similarly uses naive dot products and
distance checks that can give wrong answers for near-coplanar configurations.

The spec requires Shewchuk's robust adaptive predicates for load-bearing geometric
decisions (orientation tests, side tests, ray-triangle intersection).

## Requirements

### R1: Add `robust` crate dependency

Add `robust = "1.1"` to `vendor/truck/truck-shapeops/Cargo.toml` dependencies.

**WASM compatibility check**: The `robust` crate is pure Rust with no `std` dependency
beyond basic float operations. It MUST compile for `wasm32-unknown-unknown`. If it
doesn't, fall back to `robust-predicates` crate or feature-gate behind `robust-preds`.

### R2: Robust classify module

Create `vendor/truck/truck-shapeops/src/transversal/robust_classify.rs`:

```rust
//! Wrappers around Shewchuk's robust adaptive predicates for
//! geometric classification in boolean operations.

/// Robust orientation test for 4 points in 3D.
/// Returns positive if d is above the plane of (a,b,c),
/// negative if below, zero if coplanar.
pub(crate) fn robust_orient3d(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64;

/// Robust orientation test for 3 points in 2D.
/// Returns positive if c is left of line (a,b),
/// negative if right, zero if collinear.
pub(crate) fn robust_orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64;

/// Classify a point relative to a triangle using robust predicates.
/// Returns: 1 if ray crosses triangle (inside), 0 if outside, None if degenerate.
pub(crate) fn robust_ray_triangle_cross(
    ray_origin: [f64; 3],
    ray_dir: [f64; 3],
    tri: [[f64; 3]; 3],
) -> Option<i32>;
```

### R3: Integration into ray_cast_classify

Replace the naive `signed_crossing_faces` call path with robust predicate-based
classification. Specifically, use `robust_orient3d` to determine which side of a
triangle plane a point lies on, eliminating the need for epsilon-based comparisons
in the crossing count logic.

The integration should be in `try_ray_cast` or `ray_cast_classify` in `integrate/mod.rs`.

### R4: Integration into coplanar detection

Use `robust_orient3d` in `coplanar.rs` for point-on-plane tests instead of
`dot(normal, point - origin).abs() < tol`. The robust predicate gives exact sign,
which can then be compared against the tolerance bound.

Use `robust_orient2d` for point-in-polygon winding number tests in parameter space.

### R5: No behavioral regression

For inputs that the current pipeline handles correctly, the robust predicates
must produce the same classification results. The robust predicates only change
behavior for near-degenerate configurations.

## Files to Modify

1. `vendor/truck/truck-shapeops/Cargo.toml` — Add `robust = "1.1"` dependency
2. `vendor/truck/truck-shapeops/src/transversal/robust_classify.rs` — New module
3. `vendor/truck/truck-shapeops/src/transversal/mod.rs` — Add `mod robust_classify`
4. `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` — Use `robust_orient3d`
   in `ray_cast_classify` / `try_ray_cast`
5. `vendor/truck/truck-shapeops/src/transversal/coplanar.rs` — Use `robust_orient3d` and
   `robust_orient2d` where applicable

## Test Plan

### Unit tests (robust_classify.rs)

1. `test_orient3d_exact_coplanar` — 4 coplanar points return 0.0
2. `test_orient3d_clearly_above` — point clearly above plane returns positive
3. `test_orient3d_clearly_below` — point clearly below plane returns negative
4. `test_orient3d_ill_conditioned` — near-coplanar points where naive cross-product
   gives wrong sign but robust gives correct sign
5. `test_orient2d_collinear` — 3 collinear points return 0.0
6. `test_orient2d_left_right` — correct sign for left/right of line

### Integration tests (integrate/tests.rs)

7. `test_ray_cast_grazing_edge` — ray grazing a triangle edge gives correct
   classification (the main failure mode this fixes)
8. `test_existing_boolean_corpus_unchanged` — box-box and box-cylinder booleans
   produce same results with robust predicates

### WASM compatibility

9. Verify `cargo build --target wasm32-unknown-unknown -p truck-shapeops` succeeds

## Risk Mitigation

1. **WASM compat**: Check first. If `robust` doesn't compile for wasm32, use
   `robust-predicates` or feature-gate with `#[cfg(feature = "robust-preds")]`.
2. **Performance**: `robust` uses fast filters that only fall back to exact
   arithmetic for near-degenerate cases. No measurable perf impact expected
   for typical inputs.
3. **Behavioral change**: The predicates only change results for near-degenerate
   inputs. Run full existing test suite to verify no regressions.

## Non-Goals

- Replacing the entire ray-casting approach with a different classification method
- Adding robust predicates to polyline construction or surface-surface intersection
- Modifying `truck_base::tolerance::TOLERANCE`
