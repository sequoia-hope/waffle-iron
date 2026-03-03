# IC Boundary Interpolation Spec

**Classification:** Bug fix (DoD §8)
**Priority:** P2
**Crate:** truck-shapeops

## Problem

Torus-plane booleans (RB1/RB2/RB6/RB8/MO4) fail because `clip_polylines_to_domain()`
uses discrete inclusion/exclusion when clipping analytically-generated IC polylines to
face parameter domains. With 64 samples around a closed ellipse (~5.6° per sample),
face boundary crossings fall between samples, creating gaps where IC sub-polylines
don't reach the face boundary. Result: face boundary edges aren't split at IC
crossings → 8+ open edges → shell assembly fails → cascade exhaustion.

## Root Cause

`clip_polylines_to_domain` iterates polyline points and classifies each as
valid/invalid via `point_on_both_surfaces`. When transitioning from valid→invalid
or invalid→valid, the current code simply drops the invalid point or starts a new
segment at the next valid point. The boundary crossing point is never computed.

For a 64-sample ellipse, the maximum gap between the last valid point and the
actual face boundary is ~5.6° of arc, which translates to geometric distance
proportional to `radius * sin(5.6°) ≈ 0.098 * radius`. For typical torus radii
(~10mm), this is ~1mm — far larger than assembly tolerances (~0.01mm).

## Fix

1. Add `find_boundary_point()`: binary search (8 bisection steps) between a valid
   and invalid point to find the face boundary crossing.
2. Rewrite `clip_polylines_to_domain()` to call `find_boundary_point()` at every
   valid↔invalid transition, inserting interpolated boundary points into sub-polylines.

Note: Sample count increase (64→128) was tested but reverted — it caused a
regression in `a3_boss_on_side_face_circle_union` (cylinder-plane boolean).

## Branch Table — `clip_polylines_to_domain`

| # | Condition | Action | Test |
|---|-----------|--------|------|
| C1 | valid→valid | Push point | `clip_entirely_inside` |
| C2 | invalid→invalid | Skip | `clip_entirely_outside` |
| C3 | valid→invalid (exit) | Bisect → push boundary → end segment | `clip_boundary_valid_to_invalid` |
| C4 | invalid→valid (entry) | Bisect → push boundary → push point | `clip_boundary_invalid_to_valid` |
| C5 | Bisect returns None | No boundary inserted | Edge case in C3/C4 |

## Branch Table — `find_boundary_point`

| # | Condition | Action |
|---|-----------|--------|
| B1 | Midpoint valid | lo = mid, best = mid |
| B2 | Midpoint invalid | hi = mid |
| B3 | best ≈ valid_pt | Return None |

## Invariants

- **INV-C1:** Every sub-polyline has ≥2 points
- **INV-C2:** Sub-polyline endpoints pass `point_on_both_surfaces`
- **INV-C3:** Boundary endpoints within `tol` of face boundary
- **INV-C4:** 329+ shapeops tests pass (0 regressions)

## Oracles

- **O1:** `cargo test -p truck-shapeops` — 329+ pass, 1 pre-existing fillet fail
- **O2:** `cargo test -p test-harness` — 400+ pass
- **O3:** Unit tests assert endpoint distance to face boundary < tol
- **O4:** RB1/RB6/RB8 progress (fewer open edges or pass)

## Files Modified

| File | Change |
|------|--------|
| `specs/ic_boundary_interpolation.md` | This spec |
| `vendor/truck/.../intersection_curve/mod.rs` | Add `find_boundary_point()`, rewrite `clip_polylines_to_domain()` |
| `vendor/truck/.../intersection_curve/tests.rs` | Add 4 clipping unit tests |
