# Mesh-to-Analytical IC Fallback (Phase G2)

**Status**: Implemented
**Classification**: Bug fix (DoD S2)
**File**: `vendor/truck/truck-shapeops/src/transversal/intersection_curve/mod.rs`

## Problem

For full-revolution torus-plane face pairs (RB1/RB2/RB6/RB8/MO4), `extract_interference` finds SOME triangle overlaps (polylines are non-empty), but the resulting polylines are noisy. `search_triple` Newton iteration diverges on these noisy points, causing `try_new` to return `None`. Since `polylines.is_empty()` is false, the code takes the mesh path (not the analytical path added in Phase F). The hard-failing `collect::<Option<Vec>>` returns `None` for the entire face pair, signaling cascade retry. All 50 cascade perturbations fail the same way.

## Root Cause Chain

```
360deg revolve -> 3 lateral face patches (division=3 in rsweep)
  -> each face shares full RevolutedCurve surface
  -> extract_interference finds SOME triangle overlaps (polylines non-empty)
  -> try_new calls search_triple (Newton iteration) for each polyline point
  -> Newton diverges on noisy mesh points near torus-plane intersection
  -> try_new returns None -> collect::<Option<Vec>> returns None for face pair
  -> loops_store skips face pair -> face undivided -> open edges
  -> cascade exhaustion (50 attempts)
```

## Solution

Add analytical fallback after the mesh path fails. The mesh path retains
`collect::<Option<Vec>>` semantics (all-or-nothing) because partial ICs
cause incomplete face division. Only when ALL mesh polylines fail do we
fall back to exact analytical IC generation.

### Behavioral Change Matrix

| Scenario | Old | New |
|----------|-----|-----|
| All mesh polylines succeed | `Some(all)` | `Some(all)` -- identical |
| Some succeed, some fail | `None` (cascade) | `None` (cascade) -- identical |
| All fail, analytical exists | `None` (cascade) | Try analytical -> `Some(results)` or `None` |
| All fail, no analytical | `None` (cascade) | `None` -- identical (K8 preserved) |

### Oracles

1. **K8 preservation**: K8 has no analytical detection -> mesh path fails -> no fallback -> returns `None` -> cascade retry. Behavior identical to pre-G2.
2. **S3 preservation**: S3's mesh path succeeds on correct perturbation -> returns `Some(all)`. No change to S3 behavior.
3. **RB3/RB4/RB7 preservation**: These already pass (partial revolve / torus-torus). No torus-plane mesh noise.

### Failure Modes

- If analytical polylines also fail `search_triple`, fallback returns empty -> returns `None` -> cascade retry (no worse than before).
- Analytical IC generation is exact (64-point ellipse sampling), so `search_triple` should succeed. If it doesn't, the contingency is direct analytical parameterization (bypass Newton entirely).
