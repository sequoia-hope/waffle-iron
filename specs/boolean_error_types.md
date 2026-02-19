# Spec: BooleanError + Result Propagation

**Burndown ID**: A1
**Author**: error-engineer
**Status**: Draft

## Problem

The current boolean pipeline collapses all failures into `Option<Solid>` / `None`.
When `truck_shapeops::and()` or `or()` fails, the caller gets no diagnostic info —
just `None`. The kernel wraps this as `KernelError::BooleanFailed { reason: "truck and() returned None" }`.

The spec requires structured error types that distinguish failure stages (intersection,
classification, stitching, topology validation) so that callers can diagnose and
potentially retry with adjusted parameters.

## Requirements

### R1: BooleanError enum

Add to `crates/kernel-fork/src/types.rs`:

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum BooleanError {
    #[error("invalid input topology: {detail}")]
    InvalidInput { detail: String },

    #[error("tolerance configuration error: {detail}")]
    ToleranceError { detail: String },

    #[error("intersection construction failed: {detail}")]
    IntersectionFailed { detail: String },

    #[error("face classification ambiguous: {detail}")]
    ClassificationFailed { detail: String },

    #[error("shell assembly failed: {detail}")]
    StitchingFailed { detail: String },

    #[error("result topology invalid: {detail}")]
    InvalidResult { detail: String },
}
```

### R2: BooleanStageError (internal to truck-shapeops)

Add to `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`:

```rust
#[derive(Debug)]
pub(crate) enum BooleanStageError {
    LoopsStoreCreation,
    FaceDivision,
    Classification,
    ShellAssembly(String),
}
```

This type MUST NOT be exported from truck-shapeops (architecture boundary).

### R3: Result-returning functions

Add `and_result()` and `or_result()` as new functions in `integrate/mod.rs` that
return `Result<Solid, BooleanStageError>` instead of `Option<Solid>`. The existing
`and()` and `or()` functions are preserved as wrappers: `and_result().ok()`.

### R4: KernelError integration

Update `BooleanFailed` variant in `KernelError` to carry the structured error:

```rust
#[error("boolean operation failed: {reason}")]
BooleanFailed { reason: String },
```

Keep the existing variant shape (string reason) but populate the reason from
`BooleanError::to_string()` so that existing error handlers don't break.

Add a `From<BooleanError>` impl for `KernelError`.

### R5: Export new functions

`truck-shapeops/src/lib.rs` must export `and_result` and `or_result`.
`truck-shapeops/src/transversal/mod.rs` must re-export them.

## Files to Modify

1. `crates/kernel-fork/src/types.rs` — Add `BooleanError` enum + `From` impl
2. `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` — Add `BooleanStageError`,
   `and_result()`, `or_result()`, refactor `process_one_pair_of_shells` to return `Result`
3. `vendor/truck/truck-shapeops/src/transversal/mod.rs` — Re-export new functions
4. `vendor/truck/truck-shapeops/src/lib.rs` — Re-export new functions
5. `crates/kernel-fork/src/truck_kernel.rs` — Use `and_result()`/`or_result()` in boolean methods

## Test Plan

### Unit tests (types.rs)

1. `test_boolean_error_display` — each variant has meaningful Display output
2. `test_boolean_error_from_kernel_error` — `From<BooleanError>` for `KernelError` works

### Integration tests (truck_kernel.rs)

3. `test_and_result_success` — box-box offset returns `Ok(solid)`
4. `test_or_result_success` — box-box offset returns `Ok(solid)`
5. `test_and_result_matches_and` — `and_result().ok()` equals `and()` for same inputs
6. `test_or_result_matches_or` — `or_result().ok()` equals `or()` for same inputs

### Error path tests (truck_kernel.rs)

7. `test_boolean_error_propagation` — when truck-shapeops returns Err, kernel
   returns `KernelError::BooleanFailed` with descriptive reason string

## Architecture Constraints

- `BooleanStageError` MUST stay `pub(crate)` inside truck-shapeops
- `BooleanError` lives in kernel-fork, not truck-shapeops
- The conversion `BooleanStageError -> BooleanError` happens in `truck_kernel.rs`
- Existing `and()` / `or()` function signatures MUST NOT change (backward compat)

## Non-Goals

- Changing the pipeline stages themselves (classification logic, etc.)
- Adding debug artifact emission
- Structured errors for healing failures (separate effort)
