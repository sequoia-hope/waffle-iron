# Spec: AABB Disjoint Boolean Fast-Path

## Goal

Add an early-exit fast-path to the polygon boolean pipeline that detects when
two solid operands are spatially disjoint (non-overlapping axis-aligned bounding
boxes) and returns the correct result immediately without S-H polygon clipping.

This eliminates both performance bottlenecks (timeout failures from unnecessary
O(n*m) classification) and precision issues (S-H clipping accumulation errors
from clipping polygons that don't actually intersect).

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| a_faces | `Vec<FacePoly>` | Face polygons of operand A |
| b_faces | `Vec<FacePoly>` | Face polygons of operand B |
| op | `BoolOp` | Union, Subtract, or Intersect |
| tau | `f64` | Model tolerance (inflates AABB for near-touching detection) |

## Branch Table

| AABB Overlap? | Operation | Result |
|---------------|-----------|--------|
| No overlap | Union | Both face sets combined, no clipping |
| No overlap | Subtract | A faces only (B doesn't intersect A) |
| No overlap | Intersect | Empty solid (no shared volume) |
| Overlap | Any | Existing pipeline (classify + clip) |
| Near-touching (gap < tau) | Any | Existing pipeline (conservative) |

## Invariants

1. **Correctness**: For disjoint solids, `A ∪ B` contains all faces from both;
   `A - B = A`; `A ∩ B = ∅`.
2. **Conservative**: AABB inflation by `tau` ensures near-touching solids go
   through the full pipeline (no false negatives).
3. **No regression**: Overlapping solids follow the exact same path as before.
4. **Volume preservation**: Union of disjoint solids has volume = vol(A) + vol(B).
5. **Watertight**: Each disjoint solid was watertight individually; the combined
   result remains watertight (disjoint components are independently closed).
6. **Topology**: Face count of union result = face_count(A) + face_count(B).

## Oracles

- **Volume**: `vol(A ∪ B) ≈ vol(A) + vol(B)` for disjoint union (within tessellation tolerance)
- **Face count**: `faces(A ∪ B) = faces(A) + faces(B)` for disjoint union
- **Empty intersect**: `faces(A ∩ B) = 0` and `vol(A ∩ B) = 0` for disjoint intersect
- **Identity subtract**: `vol(A - B) = vol(A)` for disjoint subtract
- **Watertight**: Zero unpaired edges in tessellated mesh
- **Bounding box**: Union AABB encloses both individual AABBs

## Failure Modes

1. **Near-touching solids**: Gap exactly at tolerance boundary. Handled by
   inflating AABB by `tau` (conservative).
2. **Empty face sets**: One or both solids have no faces. Existing empty-solid
   handling already covers this (checked before AABB test).
3. **Extreme scales**: Micro (1e-4) or macro (1e4) scale. AABB test is
   scale-invariant; `tau` is adaptive via `compute_adaptive_tau_weld`.

## Research Basis

AABB overlap testing is standard computational geometry [Ericson 2005, "Real-Time
Collision Detection"]. No novel algorithm needed — this is a 6-comparison test
on axis-aligned extents. The innovation is in applying it as a boolean fast-path
to avoid unnecessary S-H clipping.

Ref #24 Barton et al. (2018): Hybrid boolean pipelines benefit from early spatial
rejection to avoid expensive intersection computation on non-interfering geometry.

## Analytical vs. Approximate Method Justification

- **Method**: N/A — this feature avoids intersection computation entirely for
  disjoint cases. No SSI or mesh approximation involved.
- **Surface pair coverage**: N/A — disjoint solids have no intersection surfaces.
