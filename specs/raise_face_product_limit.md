# Raise Non-Convex Boolean Face Product Limit

## Goal

The polygon boolean pipeline currently rejects non-convex boolean operations
when the AABB-filtered effective face product exceeds 5,000. This limit is too
conservative for real-world models containing gear profiles and other
high-face-count non-convex solids. Three assay cases are blocked solely by this
limit:

- **R0058**: gear x gear, 15,224 effective pairs
- **R0075**: gear x circle, 12,657 effective pairs
- **R0081**: gear x revolve(gear), 41,828 effective pairs

Additionally, 6 timeout cases may benefit from a higher limit if their
computation completes within the existing 90-second timeout.

This change raises the effective face-product limit from 5,000 to 50,000,
allowing these models to proceed to the O(n^2) face classification stage while
retaining the 90-second timeout as the ultimate safety net.

## Parameters

| Parameter                  | Current Value | Proposed Value | Unit          |
|----------------------------|---------------|----------------|---------------|
| Total face hard limit      | 8,000         | 8,000 (unchanged) | faces     |
| Effective face-product limit | 5,000       | 50,000         | face pairs    |
| Computation timeout        | 90            | 90 (unchanged) | seconds       |

- **Total face hard limit**: Maximum number of faces summed across both solids.
  Applied unconditionally regardless of convexity.
- **Effective face-product limit**: Maximum AABB-filtered face pair count for
  non-convex x non-convex operations. After AABB filtering eliminates spatially
  disjoint pairs, the remaining pair count must be below this limit.
- **Computation timeout**: Hard wall-clock limit on any single boolean
  operation. Unchanged by this spec.

All limits apply only at the polygon boolean layer. The analytical SSI pipeline
(A15) is unaffected.

## Branch Table

| A faces | B faces | Both non-convex | Raw product | Effective product | Action               |
|---------|---------|-----------------|-------------|-------------------|----------------------|
| <=8000 total | <=8000 total | Yes       | >5000       | <=50000           | Allow (raised limit) |
| <=8000 total | <=8000 total | Yes       | >5000       | >50000            | Reject (NotSupported)|
| Any     | Any     | No (one convex) | Any         | Any               | Allow (unchanged)    |
| >8000 total | Any  | Any             | Any         | Any               | Reject (unchanged)   |

Notes:
- "Both non-convex" means neither solid is classified as convex by the existing
  convexity test (all face normals form a convex cone).
- "Effective product" is the count of (face_A, face_B) pairs whose AABBs
  overlap, i.e., the pairs that actually enter the O(n^2) classification loop.
- When effective product <= 5000 (the old limit), behavior is identical to
  before -- no change in this branch.

## Invariants

1. **All existing tests pass unchanged.** Cases that currently succeed must
   produce identical geometry. Cases that currently hit the 5,000 limit and
   return NotSupported will now either succeed or timeout -- they must not
   produce incorrect geometry.

2. **No performance regression on passing cases.** Cases with effective product
   below 5,000 do not hit the limit check at all, so their performance is
   unchanged.

3. **Timeout remains the ultimate guard.** Any case that takes longer than 90
   seconds is aborted regardless of face count or product. This prevents
   truly pathological inputs from hanging the system.

4. **Euler invariant holds for new successes.** Any case that now proceeds past
   the raised limit must produce a result satisfying V - E + F = 2 (genus-0)
   or the appropriate genus formula, with all half-edges paired (watertight).

5. **Determinism.** Identical inputs must produce identical outputs. The limit
   change is a pure threshold adjustment with no algorithmic branching.

## Oracles

| Case  | Description            | Effective Pairs | Current Result       | Expected After Change |
|-------|------------------------|-----------------|----------------------|-----------------------|
| R0058 | gear x gear            | 15,224          | NotSupported (limit) | Success or Timeout    |
| R0075 | gear x circle          | 12,657          | NotSupported (limit) | Success or Timeout    |
| R0081 | gear x revolve(gear)   | 41,828          | NotSupported (limit) | Success or Timeout    |

For cases that succeed:
- Watertight mesh (every half-edge has a twin)
- Non-negative volume
- No NaN coordinates in any vertex
- Euler formula satisfied

For all existing boolean tests:
- Identical results (same geometry, same pass/fail status)

## Failure Modes

| Condition                          | Behavior                        |
|------------------------------------|---------------------------------|
| Effective product > 50,000         | Return KernelError::NotSupported with diagnostic message |
| Total faces > 8,000               | Return KernelError::NotSupported (unchanged) |
| Computation exceeds 90s timeout    | Return KernelError::Timeout (unchanged) |
| AABB filter produces 0 pairs      | Disjoint solids, return unchanged inputs (unchanged) |
| One solid is convex                | Skip face-product limit entirely (unchanged) |

No new error types are introduced. The only change is the numeric threshold in
the existing limit check.

## Research Basis

- **Empirical origin of the 5,000 limit**: The original limit was chosen
  conservatively during initial non-convex boolean support to avoid timeout on
  O(n^2) face classification. At the time, gear profiles were not yet generated
  by the assay corpus.

- **AABB filtering effectiveness**: The Axis-Aligned Bounding Box filter
  reduces the raw face product (A_faces x B_faces) to a much smaller set of
  spatially overlapping pairs. For gear x gear with ~120 faces each, the raw
  product is ~14,400 but AABB filtering produces ~15,224 effective pairs (gear
  teeth interleave, so filtering is less effective than for spatially separated
  geometry). Even so, the 90s timeout provides adequate protection.

- **Timeout as safety net**: The 90-second wall-clock timeout (implemented via
  `std::time::Instant` check in the classification loop) is independent of face
  count and catches any case where the raised limit admits a computation that
  is too expensive in practice.

### Analytical vs. Approximate Method Justification

- **Method**: Approximate (polygon/mesh boolean).
- **Justification**: This spec modifies a threshold in the polygon boolean
  pipeline. It does not introduce any new surface-surface intersection logic.
  The polygon boolean path is used here because the affected cases involve
  tessellated gear profiles that are already routed through the mesh pipeline.
  Per A15, quadric surface pairs use the analytical SSI pipeline, which is
  unaffected by this change.
- **Surface pair coverage**: Not applicable -- this is a limit adjustment, not
  a new boolean operation. No new surface pairs are introduced.
