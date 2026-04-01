# Yang Pipeline Panic Safety & Performance Guard

## Goal

Make the Yang boolean pipeline (A15.6) safe to enable via `YANG_BOOLEAN=1` by
ensuring that no panic or performance pathology in the pipeline can cause
regressions from the legacy baseline. Three changes:

1. **catch_unwind**: Wrap the Yang pipeline call so panics fall through to legacy.
2. **Triangle-count guard**: Skip the Yang pipeline for inputs that would cause
   O(n*m) timeout, returning NotSupported to trigger legacy fallback.
3. **Result return type**: Change `yang_boolean_pipeline()` to return
   `Result<ResultTopology, KernelError>` instead of panicking on internal failures.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| MAX_YANG_TRI_PAIRS | const u64 | 50,000 | Maximum n*m triangle pair count before Yang returns NotSupported |
| YANG_BOOLEAN | env var | unset | "1" enables Yang pipeline |

## Branch Table

| Condition | Behavior |
|-----------|----------|
| YANG_BOOLEAN != "1" | NotSupported (unchanged) |
| Either solid missing face_geometry | NotSupported (unchanged) |
| Tessellation produces empty mesh | NotSupported (unchanged) |
| tris_a.len() * tris_b.len() > MAX_YANG_TRI_PAIRS | **NEW**: NotSupported |
| Yang pipeline panics | **NEW**: caught by catch_unwind → legacy fallback |
| yang_boolean_pipeline returns Err | **NEW**: NotSupported → legacy fallback |
| Yang pipeline produces empty topology | NotSupported (unchanged) |
| Yang pipeline succeeds | Use result (unchanged) |

## Invariants

1. **No regressions**: Any assay case that passes without YANG_BOOLEAN=1 must also
   pass with YANG_BOOLEAN=1. The Yang pipeline is additive during the gated phase.

2. **Panic containment**: No panic in the Yang pipeline code path may propagate
   past the `do_boolean()` dispatch boundary. All panics are caught and converted
   to errors that trigger legacy fallback.

3. **Performance bound**: The Yang pipeline must return within O(1) time for inputs
   exceeding the triangle-count threshold. No expensive computation runs before
   the guard check.

4. **Error recovery**: `yang_boolean_pipeline()` returns `Result`, never panics.
   Internal failures (empty survival groups, missing provenance, degenerate geometry)
   produce `Err(KernelError)`.

## Oracles

- **Regression oracle**: Run assay suite with YANG_BOOLEAN=1. All 8 legacy-passing
  cases must still pass.
- **Panic oracle**: No catch_unwind triggers should propagate — they are caught and
  converted. Verify with test that deliberately triggers a panic path.
- **Performance oracle**: F-series boss cases (F0001, F0002, etc.) complete within
  5s (immediate NotSupported + legacy fallback), not 90s timeout.

## Failure Modes

| Condition | Expected Behavior |
|-----------|------------------|
| Yang pipeline panics (index OOB, unwrap, etc.) | catch_unwind catches → Err → legacy fallback |
| Triangle count exceeds threshold | Immediate NotSupported → legacy fallback |
| yang_boolean_pipeline internal error | Returns Err → yang_boolean_from_solids returns Err → legacy fallback |
| Tessellation of WaffleSolid fails | Existing guard returns NotSupported |

## Research Basis

- [#24] Yang et al. 2025 — hybrid B-Rep/mesh boolean pipeline
- [#9] Cherchi et al. 2020 — BVH acceleration for mesh arrangements (motivates
  the triangle-count guard as a temporary measure until BVH is implemented)
- Standard Rust panic safety: `std::panic::catch_unwind` + `AssertUnwindSafe`

## Analytical vs. Approximate Method Justification

This spec does not introduce any new SSI or boolean computation. It only changes
error handling, panic recovery, and performance dispatch in the integration layer.
The analytical primacy invariant (A15.1) is unaffected.
