# V2 Assembly Tolerance Reduction

**Status:** Proposed
**Sprint:** 49
**FIP Classification:** Refactor (DoD 3) — reduces destructive tolerance escalation in shell assembly.

## Problem

The V2 progressive weld assembly (`assemble_boolean_shell_v2` in
`vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs`) escalates
tolerance across 4 levels when closing a shell after boolean face classification:

| Level | Label | Weld tolerance | Multiplier |
|-------|-------|---------------|------------|
| 0 | `default(0.2x)` | `0.2 * tau_model` (canonical IC edges) | 0.2x |
| 1 | `tau_weld(0.4x)` | `tols.tau_weld = 0.4 * tau_model` | 0.4x |
| 2 | `tau_edge_cluster(5.0x)` | `tols.tau_edge_cluster = 5.0 * tau_model` | 5.0x |
| 3 | `force_merge` | Geometric endpoint matching | N/A |

**Level 2 is destructive.** At 5.0x `tau_model`, it merges vertices that are
5 times the model tolerance apart. For a typical `tau_model ≈ 0.005` (5mm extent
box), Level 2 welds vertices up to 0.025 apart — destroying fine features like
thin walls, narrow slots, and closely-spaced bosses. This is the root cause of
geometry corruption in cases where the boolean IC was computed correctly but
assembly over-welds.

The gap between Level 1 (0.4x) and Level 2 (5.0x) is a 12.5x jump —
far too aggressive. Shells that need slightly more tolerance than 0.4x get
blasted with 5.0x instead of a moderate escalation.

## Proposed Change

Reduce the tolerance escalation to a smoother progression:

| Level | Label | Current | Proposed | Change |
|-------|-------|---------|----------|--------|
| 0 | `default(0.2x)` | `0.2 * tau_model` | `0.2 * tau_model` | Unchanged |
| 1 | `tau_weld` | `0.4 * tau_model` | `1.0 * tau_model` | 0.4x → 1.0x |
| 2 | `tau_edge_cluster` | `5.0 * tau_model` | `2.0 * tau_model` | 5.0x → 2.0x |
| 3 | `force_merge` | Geometric endpoint matching | Geometric endpoint matching | Unchanged |

### Rationale

- **Level 1 (0.4x → 1.0x):** The model tolerance `tau_model` is already the
  natural unit of precision for the geometry. Welding at 1.0x covers the common
  case where IC endpoints are within model tolerance but beyond the conservative
  0.2x initial weld. This replaces the current gap where many shells fall
  between 0.4x and 5.0x.

- **Level 2 (5.0x → 2.0x):** A 2.0x multiplier provides a safety margin beyond
  model tolerance without destroying features. The maximum merge distance drops
  from `5*tau_model` to `2*tau_model` — still wider than Level 1 for edge cases,
  but preserving geometry that is 2-5x `tau_model` apart.

- **Level 3 (unchanged):** `force_merge_open_edges` matches edges by geometric
  endpoint position regardless of tolerance. This is the true safety net and
  remains unchanged.

### Implementation Locations

Two changes required:

1. **`BooleanTolerance::from_model_tol()`** (line ~65):
   - `tau_weld: 0.4 * tau_model` → `tau_weld: 1.0 * tau_model`
   - `tau_edge_cluster: 5.0 * tau_model` → `tau_edge_cluster: 2.0 * tau_model`

2. **`assemble_boolean_shell_v2()` level labels** (line ~2743):
   - Update label strings to reflect new multipliers

### Branch Table

| Condition | Outcome |
|-----------|---------|
| Shell closes at Level 0 (0.2x) | No change — same as before |
| Shell closes at Level 1 (was 0.4x, now 1.0x) | More shells close here (covers former 0.4x-1.0x gap) |
| Shell closes at Level 2 (was 5.0x, now 2.0x) | Fewer shells close here; those needing 2.0-5.0x fall to force_merge |
| Shell needs force_merge (Level 3) | May increase — shells that previously needed 2.0-5.0x now need force_merge |
| Shell fails all levels | Same — already fails at 5.0x, will still fail at force_merge |

## Invariants

1. **All currently-passing tests still pass.** The full boolean test suite
   (truck-shapeops 306+, test-harness 400+, GUI 726+) must be green.
2. **Euler chi=2** for all results that currently satisfy it.
3. **`tau_weld < tau_edge_cluster`** — Level 1 must be strictly less than Level 2.
4. **`tau_edge_cluster >= tau_model`** — Level 2 must cover at least model tolerance.
5. **Monotonic escalation** — Level 0 < Level 1 < Level 2 tolerance values.

## Oracles

- `cargo test -p truck-shapeops` — all 306+ tests pass
- `cargo test -p test-harness` — all 400+ tests pass (no new regressions)
- `cargo test -p kernel-fork` — all 226 tests pass
- Euler chi=2 verification in boolean_properties tests
- Volume/bbox oracle checks in boolean_workflows tests

## Failure Modes

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Some shells that closed at 5.0x now need force_merge | Medium | force_merge is robust (Sprint 45); Level 3 catches these |
| A shell that needed exactly 2.0-5.0x and force_merge can't close it | Low | force_merge matches by endpoint geometry, not tolerance — it should handle all cases Level 2 did |
| Regression in a currently-passing test | Low | Run full suite before merge |
| Performance change from more force_merge invocations | Negligible | force_merge is O(n²) on open edges only, which are few |

## Risk Assessment

**Low.** The `force_merge_open_edges` safety net (Level 3) handles shells that
previously needed aggressive tolerance. The change makes the pipeline less
destructive without removing any fallback capability. Worst case: some shells
take one more escalation step to close.

## Files Changed

| File | Change |
|------|--------|
| `vendor/truck/truck-shapeops/src/transversal/integrate/mod.rs` | `from_model_tol()`: update `tau_weld` and `tau_edge_cluster` multipliers; `assemble_boolean_shell_v2()`: update level label strings |
| `specs/boolean_tolerance_layering.md` | Update tolerance table to reflect new values |
