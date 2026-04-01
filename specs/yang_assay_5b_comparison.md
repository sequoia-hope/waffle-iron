# Yang Pipeline Assay Comparison (Phase 5b)

Generated: 2026-04-01
Reference: [#24] Yang et al. 2025 — Hybrid B-Rep/mesh boolean

## Summary

| Metric | Legacy | Yang | Delta |
|--------|--------|------|-------|
| Passed | 8/190 | 0/190 | -8 |
| Failed | 136 | 21 | -115 |
| Errored | 46 | 169 | +123 |
| Duration | 5314.5s | 2305.0s | -3009.5s |

## Change Summary

- **Improved** (legacy fail → Yang pass): 0
- **Regressed** (legacy pass → Yang fail/error): 8
- **Different failure** (both fail, different mode): 119
- **Unchanged**: 63

## Root Cause Analysis

### Why Yang scores zero

The Yang pipeline is dispatched **first** in `do_boolean()`. When `YANG_BOOLEAN=1`:

1. **Yang attempted for every boolean** — it checks face_geometry, tessellates both
   solids, runs exact mesh boolean, then topology extraction.
2. **Panics in topology_extract and tessellation** — the Yang pipeline hits index
   out-of-bounds panics at `topology_extract.rs:139` and `tessellation/mod.rs:2477`
   on most input geometries. These panics are caught by `catch_unwind` and converted
   to `KernelError`.
3. **Error propagation blocks legacy fallback** — `do_boolean()` only falls through
   on `NotSupported` errors. Any other error (including caught panics) propagates
   immediately, preventing the legacy pipeline from running.
4. **Timeout on complex cases** — for cases where the Yang pipeline runs slowly
   before panicking (especially multi-op F-series), the 90s per-case timeout fires.

### Regression pattern

All 8 regressions are **F-series boss-only cases** (F0001, F0002, F0004, F0005,
F0007, F0008, F0051, F0053) that passed with legacy but timeout under Yang. These
are planar box-box union cases where:
- Yang tessellates both boxes (fast)
- Exact mesh boolean runs (potentially slow for large triangle counts)
- Times out before completing or panicking

### "Different failure" pattern

119 cases moved from FAIL→ERROR. This is expected: cases that previously produced
wrong-but-renderable geometry (failed oracle checks) now error because the Yang
pipeline panics before any result is produced.

## Panic Sites

Two panic locations account for virtually all Yang errors:

1. **`topology_extract.rs:139`** — `index out of bounds: the len is 0 but the index is 0`
   The result topology has zero faces/edges after extraction. This happens when the
   exact mesh boolean produces an empty result (no surviving cells for the requested
   boolean operation).

2. **`tessellation/mod.rs:2477`** — `index out of bounds: the len is 0 but the index is 0`
   The WaffleSolid produced by Yang has empty face geometry or vertex lists that the
   tessellation path doesn't handle gracefully.

## Diagnosis

The Yang pipeline produces correct results for the unit tests in `yang_integration.rs`
(10 tests pass, covering box-box and box-cylinder operations). The assay failures
indicate that **real-world geometry** from the assay corpus triggers edge cases in:

1. **Triangle-triangle intersection** — the exact mesh boolean may produce degenerate
   configurations (coplanar faces, shared edges) that the subdivision step doesn't handle
2. **Cell labeling** — winding number computation may fail for non-manifold intermediate
   meshes
3. **Empty result handling** — the pipeline doesn't gracefully handle cases where the
   boolean operation produces zero surviving faces

## Recommended Next Steps

Before Phase 5c (make Yang default), the following must be addressed:

1. **Catch all Yang errors in dispatch** — change `do_boolean()` to treat any Yang
   error (not just `NotSupported`) as a fallback trigger. This immediately fixes all
   regressions by allowing legacy to handle cases Yang can't.

2. **Fix empty-result panics** — add bounds checks in `topology_extract.rs` and
   `tessellation/mod.rs` to return proper errors instead of panicking on empty results.

3. **Run comparison again** — after (1), the comparison should show 0 regressions
   and potentially some improvements.

4. **Profile timeout cases** — investigate why F0001 etc. timeout. If the exact mesh
   boolean is O(n²) in triangle count, add a triangle-count guard.

## Regressed Cases (Detail)

| Case | Legacy | Yang | Cause |
|------|--------|------|-------|
| F0001 | PASS (9 oracles) | ERROR (timeout 90s) | Box-box union timeout |
| F0002 | PASS (9 oracles) | ERROR (timeout 90s) | Box-box union timeout |
| F0004 | PASS (9 oracles) | ERROR (timeout 90s) | Box-box union timeout |
| F0005 | PASS (9 oracles) | ERROR (timeout 90s) | Box-box union timeout |
| F0007 | PASS (9 oracles) | ERROR (timeout 90s) | Box-box union timeout |
| F0008 | PASS (9 oracles) | ERROR (timeout 90s) | Box-box union timeout |
| F0051 | PASS (9 oracles) | ERROR (timeout 90s) | Gear+box boolean timeout |
| F0053 | PASS (9 oracles) | ERROR (timeout 90s) | Gear+box boolean timeout |

## Verdict

**The Yang pipeline is not yet ready for default dispatch.** The current env-var gate
(`YANG_BOOLEAN=1`) must remain. The pipeline produces correct results for simple unit
test cases but panics on real-world assay geometry.

**Phase 5c prerequisite**: Fix the error-propagation issue (catch all Yang errors,
fall through to legacy) so that enabling Yang cannot cause regressions. Then focus on
fixing the panics incrementally until Yang handles more cases than legacy.
