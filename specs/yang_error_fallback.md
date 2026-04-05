# Yang Pipeline Error Dispatch (A15.6)

## Goal

When the Yang hybrid boolean pipeline (A15.6) encounters an error during
`YANG_BOOLEAN=1` execution, the error must **propagate as a hard failure** — not
fall back to the legacy S-H pipeline. This enforces A15.6: Yang errors should
fail, not silently degrade to the broken legacy path.

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| YANG_BOOLEAN | env var | unset | "1" enables Yang pipeline; any other value disables |

No new parameters are introduced. This spec defines error handling dispatch only.

## Branch Table

| Yang Result | YANG_BOOLEAN=1 | YANG_BOOLEAN unset |
|-------------|----------------|---------------------|
| Ok(result) | Use result | Use result |
| Err(NotSupported) with "not enabled" | N/A | Fall through to legacy (env-var gate) |
| Err(other) | **Propagate as hard error (A15.6)** | **Propagate as hard error (A15.6)** |
| Panic (catch_unwind) with YANG_BOOLEAN=1 | **Propagate as hard error** | N/A |
| Panic (catch_unwind) without YANG_BOOLEAN | N/A | Log diagnostic, fall through (Yang wasn't requested) |

## Invariants

1. **A15.6 enforcement**: When `YANG_BOOLEAN=1` is set and the Yang pipeline
   returns any error other than "not enabled", the error propagates. No fallback
   to legacy S-H pipeline. The legacy path's assay score has no value — only the
   Yang path score matters.

2. **Panic safety**: The Yang pipeline must not panic on any input. Empty results,
   degenerate geometry, and subdivision failures must produce `Err(KernelError)`
   not panics. Panics are caught by `catch_unwind` and converted to hard errors
   when Yang was explicitly requested.

3. **Diagnostic preservation**: Yang errors are logged with full diagnostics for
   debugging. Error messages include pipeline stage and failure context.

## Oracles

- **Hard-fail oracle**: With `YANG_BOOLEAN=1`, any Yang pipeline error (except
  "not enabled") must surface as `Err(KernelError)` to the caller.
- **Panic oracle**: No `catch_unwind` triggers in the Yang pipeline for any assay input.
- **Empty-result oracle**: `yang_boolean_pipeline` returns empty `ResultTopology`
  (not panic) when boolean produces zero surviving faces.

## Failure Modes

| Condition | Expected Behavior |
|-----------|-------------------|
| Yang pipeline returns Err (YANG_BOOLEAN=1) | Hard error propagated to caller |
| Yang pipeline panics (YANG_BOOLEAN=1) | catch_unwind → hard error propagated |
| Yang pipeline panics (YANG_BOOLEAN unset) | catch_unwind → log, fall through to legacy |
| Yang produces empty topology | Returns NotSupported — handled per branch table |

## History

- **Original spec**: Described fallback-on-error dispatch (Yang errors fell through
  to legacy). This was an A15.6 violation — it allowed the legacy path to mask
  Yang failures silently.
- **Commit 198b588**: Corrected dispatch to propagate Yang errors as hard failures,
  enforcing A15.6.
- **Commit 079e139**: Introduced Yang→S-H timeout fallback (A15.6 violation),
  corrected by 198b588.

## Research Basis

- [#24] Yang et al. 2025 — the pipeline itself
- Error dispatch is standard defensive programming; A15.6 mandates fail-loud

## Analytical vs. Approximate Method Justification

This spec does not introduce any new SSI or boolean computation. It only defines
error handling dispatch in the integration layer.
