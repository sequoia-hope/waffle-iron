# Phase E: Cascade Instrumentation & Progressive Gating

**FIP Status:** Approved
**Phase:** E (follows A–D)
**Classification:** Refactor (DoD 3) — no behavior change

## Goal

Add structured instrumentation to the perturbation cascade in `healing.rs` to
measure how often direct boolean operations succeed vs. requiring perturbation,
and to populate the already-declared-but-empty `CascadeReport` and
`BooleanDiagnosticsSummary` fields. This proves that Phases A–D (corner-touch
snap, IC refinement, exact predicates, topology-first assembly) have reduced
cascade reliance, and prepares data for eventual cascade removal.

## Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `CASCADE_DIRECT_SUCCESS` | atomic counter | Direct (attempt #1) successes |
| `CASCADE_PERTURBATION_SUCCESS` | atomic counter | Perturbation (attempt >1) successes |
| `CASCADE_EULER_FALLBACK` | atomic counter | chi!=2 fallback returns |
| `CASCADE_EXHAUSTED` | atomic counter | Fully exhausted cascades |
| `CASCADE_TOTAL` | atomic counter | Total cascade invocations |

## Invariants

1. **Counter consistency:** `direct + perturbation + euler_fallback + exhausted == total` at all times.
2. **Zero behavior change:** All existing boolean operations produce identical results.
3. **No new public API surface:** `CascadeStats`, `cascade_stats()`, `reset_cascade_stats()` are public but informational only. No kernel trait changes.
4. **Thread safety:** All counters are `AtomicUsize` with `Ordering::Relaxed`. Suitable for cross-test aggregation but not for single-operation guarantees under concurrent use (acceptable — booleans are single-threaded per operation).

## Oracles

- **CM3 invariant test:** After N boolean operations, `stats.direct_success + stats.perturbation_success + stats.euler_fallback + stats.exhausted == stats.total == N`.
- **CM1 direct success rate:** For 5 simple (non-coplanar) boolean operations, `direct_success >= 3` (60%+).
- **CM4 reset:** After `reset_cascade_stats()`, all fields are zero.

## What This Does NOT Do

- Does not remove the cascade
- Does not gate or skip any strategies
- Does not change any boolean behavior
- Does not un-ignore any tests
- Does not modify truck-shapeops internals
