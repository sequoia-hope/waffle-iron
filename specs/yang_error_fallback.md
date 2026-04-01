# Yang Pipeline Error Fallback

## Goal

When the Yang hybrid boolean pipeline (A15.6) fails during its env-var-gated
phase (`YANG_BOOLEAN=1`), the boolean operation must fall through to the legacy
pipeline instead of propagating the error. This ensures that enabling Yang for
testing cannot cause regressions.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| YANG_BOOLEAN | env var | unset | "1" enables Yang pipeline; any other value disables |

No new parameters are introduced. This spec changes error handling behavior only.

## Branch Table

| Yang Result | Current Behavior | New Behavior |
|-------------|------------------|--------------|
| Ok(result) | Use result | Use result (unchanged) |
| Err(NotSupported) | Fall through to legacy | Fall through to legacy (unchanged) |
| Err(other) | **Propagate error, abort** | **Fall through to legacy** |

## Invariants

1. **No regressions**: Any assay case that passes with legacy must also pass when
   `YANG_BOOLEAN=1` is set. The Yang pipeline is additive — it may improve results
   but must never worsen them during the gated phase.

2. **Panic safety**: The Yang pipeline must not panic on any input. Empty results,
   degenerate geometry, and subdivision failures must produce `Err(KernelError)`
   not panics.

3. **Diagnostic preservation**: Yang errors should be logged/traceable for debugging
   but must not block the boolean operation from completing via legacy fallback.

## Oracles

- **Regression oracle**: Run assay suite with `YANG_BOOLEAN=1`. All cases that pass
  without the flag must also pass with it.
- **Panic oracle**: No `catch_unwind` triggers in the Yang pipeline for any assay input.
- **Empty-result oracle**: `yang_boolean_pipeline` returns empty `ResultTopology`
  (not panic) when boolean produces zero surviving faces.

## Failure Modes

| Condition | Expected Error |
|-----------|---------------|
| Yang pipeline panics | Caught by catch_unwind, falls through to legacy |
| Yang produces empty topology | Returns NotSupported, falls through to legacy |
| Yang times out | N/A (timeout is external) — but legacy gets a chance |

## Research Basis

- [#24] Yang et al. 2025 — the pipeline itself
- No published technique for the error fallback — this is standard defensive dispatch

## Analytical vs. Approximate Method Justification

This spec does not introduce any new SSI or boolean computation. It only changes
error handling dispatch in the integration layer.
