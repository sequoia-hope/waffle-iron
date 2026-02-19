# Spec: Boolean Difference Operation

**Burndown ID**: B2 (partial — difference only, XOR deferred)
**Author**: difference-impl
**Status**: Draft

## Problem

The current `boolean_subtract` in `TruckKernel` implements difference as:
```rust
let mut b_neg = b.clone();
b_neg.not();
truck_shapeops::and(a, &b_neg, tol)
```

This `not() + and()` approach has problems:
1. `not()` inverts ALL faces of solid B, but difference only needs faces of B that
   are inside A to be inverted.
2. The full solid inversion can interact badly with face classification — the
   inverted B's boundary conditions don't match what `process_one_pair_of_shells`
   expects for "And" classification.
3. It's semantically wrong: `A ∩ ¬B` classically works but the implementation
   here pre-inverts before face splitting, which changes how intersection curves
   are computed.

A proper `difference()` selects from shell0 the faces classified as `Or` (outside
shell1), and from shell1 the faces classified as `And` (inside shell0) with
**inverted orientation**. This is more correct and avoids the full solid inversion.

## Requirements

### R1: difference() function

Add to `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`:

```rust
/// Difference operation: A \ B.
/// Selects faces of A outside B (Or), plus faces of B inside A (And) with inverted orientation.
pub fn difference<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    solid0: &Solid<Point3, C, S>,
    solid1: &Solid<Point3, C, S>,
    tol: f64,
) -> Option<Solid<Point3, C, S>>
```

### R2: Implementation

Uses the same `process_one_pair_of_shells` infrastructure but selects differently:
- From shell0: take `Or` faces (outside shell1) — these are kept
- From shell1: take `And` faces (inside shell0) — these are **inverted** (orientation flipped)

This requires refactoring `process_one_pair_of_shells` to return the raw
classified face buckets `[and0, or0, and1, or1]` instead of pre-merged `[and, or]`,
OR adding a new helper that returns all 4 buckets.

### R3: difference_result() function

Also add a `Result`-returning variant (if Agent 2's error types are merged first):

```rust
pub fn difference_result<C: ShapeOpsCurve<S>, S: ShapeOpsSurface>(
    solid0: &Solid<Point3, C, S>,
    solid1: &Solid<Point3, C, S>,
    tol: f64,
) -> Result<Solid<Point3, C, S>, BooleanStageError>
```

If Agent 2's work isn't merged yet, just implement `difference()` returning `Option`.

### R4: Export

Export `difference` from:
- `truck-shapeops/src/transversal/mod.rs`
- `truck-shapeops/src/lib.rs`

### R5: Kernel integration

Update `TruckKernel::boolean_subtract` in `crates/kernel-fork/src/truck_kernel.rs`
to use `truck_shapeops::difference()` instead of `not() + and()`:

```rust
fn boolean_subtract(&mut self, a: &KernelSolidHandle, b: &KernelSolidHandle)
    -> Result<KernelSolidHandle, KernelError>
{
    // ... get solid_a, solid_b, heal ...
    let result = truck_shapeops::difference(&solid_a, &solid_b, tol)
        .ok_or_else(|| KernelError::BooleanFailed {
            reason: "truck difference() returned None".to_string(),
        })?;
    // ... heal result, store ...
}
```

### R6: Behavioral equivalence

`difference(A, B)` must produce the same result as the current `not(B) + and(A, ¬B)`
for all inputs in the existing test corpus. Any divergence must be investigated —
it likely indicates the new implementation is more correct.

## Files to Modify

1. `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` — Add `difference()`,
   refactor `process_one_pair_of_shells` to expose per-shell buckets
2. `vendor/truck/truck-shapeops/src/transversal/mod.rs` — Export `difference`
3. `vendor/truck/truck-shapeops/src/lib.rs` — Export `difference`
4. `crates/kernel-fork/src/truck_kernel.rs` — Use `difference()` in `boolean_subtract`

## Test Plan

### Algebraic property tests (integrate/tests.rs)

1. `test_difference_box_box_offset` — vol(A\B) approx vol(A) - vol(A cap B) within tolerance
2. `test_difference_non_commutative` — diff(A,B) != diff(B,A) (different face counts)
3. `test_difference_disjoint` — diff(A,B) = A when A and B don't overlap
4. `test_difference_contained` — diff(A,B) = empty/error when B fully contains A
5. `test_difference_self` — diff(A,A) = empty/error

### Equivalence tests

6. `test_difference_matches_not_and` — difference(A,B) produces same shell topology
   as and(A, not(B)) for box-box offset
7. `test_difference_matches_not_and_box_cyl` — same for box-cylinder (punched cube)

### Integration tests (truck_kernel.rs)

8. `test_boolean_subtract_uses_difference` — kernel subtract still works after switch
9. `test_subtract_then_tessellate` — subtract result tessellates cleanly

### Manifold check

10. `test_difference_result_manifold` — result shell is Closed

## Architecture

The refactoring of `process_one_pair_of_shells` introduces a 4-bucket variant:

```rust
fn classify_one_pair_of_shells<C, S>(
    shell0: &Shell<Point3, C, S>,
    shell1: &Shell<Point3, C, S>,
    tol: f64,
) -> Option<ClassifiedShells<Point3, C, S>>

struct ClassifiedShells<P, C, S> {
    and0: Shell<P, C, S>,  // shell0 faces inside shell1
    or0: Shell<P, C, S>,   // shell0 faces outside shell1
    and1: Shell<P, C, S>,  // shell1 faces inside shell0
    or1: Shell<P, C, S>,   // shell1 faces outside shell0
}
```

Then `and()`, `or()`, and `difference()` are all thin wrappers:
- `and()`: merge and0 + and1
- `or()`: merge or0 + or1
- `difference()`: merge or0 + inverted(and1)

## Dependencies

- Should merge AFTER Agent 2's error types if possible (to add `difference_result`)
- Can write spec + tests in parallel with Agent 2
- No dependency on Agent 1 (tolerance) or Agent 3 (predicates)

## Non-Goals

- XOR operation (deferred to later sprint)
- Changing the classification logic itself
- Multi-shell difference (single-shell pair for now)
