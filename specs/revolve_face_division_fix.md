# Spec: Revolve Face Division Fix

## Goal

Fix face division for faces on RevolutedCurve surfaces with full 360° revolution,
enabling torus-plane boolean operations (RB2, RB8, MO4).

## Parameters

- **Surface type**: RevolutedCurve (v ∈ [0, 2π) maps to revolution angle)
- **Revolution range**: Full 360° (v spans entire [0, 2π))
- **Face boundary wires**: May cross the v=0/2π seam
- **IC (intersection curve) edges**: Created by boolean intersection, may also cross seam

## Problem Analysis

### Root cause: parametric v-seam discontinuity

`RevolutedCurve::search_parameter` returns v ∈ [0, 2π) via `proj_angle`. When
consecutive vertices on a wire cross the v=0/2π seam, the returned v-values jump
by ~2π (e.g., 6.2 → 0.1), corrupting:

1. **Legacy face division** (`create_parameter_boundary`): Shoelace area formula
   computed on discontinuous polyline → wrong signed areas → fragments dropped
   or misclassified.
2. **FBG face division** (`FaceBoundaryGraph::from_loops`): Vertices mapped
   independently with `None` hints → inconsistent v-coordinates at seam →
   wrong radial sort → incorrect face fragment topology.

### Why partial revolves pass

Partial revolves (RB3=90°, RB4=180°) never cross the v=0/2π seam. Their v-values
stay within a contiguous range, so `proj_angle` returns consistent values.

## Branch Table

| Case | Description | Hint v | Computed v | Action |
|------|-------------|--------|------------|--------|
| (a) | Point near v=0, hint near 2π | ~6.0 | ~0.1 | Shift +2π → ~6.38 (search_parameter) or unwrap (caller) |
| (b) | Point near v=2π, hint near 0 | ~0.1 | ~6.2 | Shift -2π → ~-0.08 (search_parameter) or unwrap (caller) |
| (c) | Point away from seam | ~3.0 | ~3.5 | No shift needed |
| (d) | No hint provided | None | [0, 2π) | No shift possible |

## Implemented Fix (Two-Layer)

### Layer 1: `RevolutedCurve::search_parameter` branch selection

When a v-hint is provided and the computed angle differs by more than π from
the hint, the result is shifted by ±2π to the branch closest to the hint.
This maintains parametric continuity for callers that chain hints.

**Guard**: Only activates when |computed - hint| > π, preventing false shifts
for revolution arcs with large but legitimate Δv.

**Safety**: The existing `subs(t, ang).near(&point)` check validates the result.
Since `subs` uses `Matrix3::from_axis_angle` (periodic in 2π), any ±2π shift
produces the same 3D point, so the `near` check always passes.

### Layer 2: v-seam unwrapping in callers

Post-hoc normalization in `create_parameter_boundary` and `FaceBoundaryGraph::from_loops`:
scan adjacent vertices for |Δv| > 2π−0.8 (≈5.48) and shift by ±2π.

**Threshold**: Uses 2π−0.8 ≈ 5.48 instead of π to avoid false positives on
non-periodic surfaces (planes, general NURBS) where large |Δv| is legitimate.

## Invariants

1. **Parametric continuity**: Adjacent vertices on a wire should have |Δv| < 2π−0.8
   after unwrapping.
2. **Correct signed area**: Shoelace formula on unwrapped polyline gives correct
   positive (outer) or negative (hole) area.
3. **Radial sort consistency**: FBG departure angles computed from unwrapped
   v-coordinates produce correct CCW ordering.

## Oracles

- Volume within 10% of analytical expectation
- Euler characteristic χ = 2 (genus-0 solids)
- Expected face count for torus-plane intersection
- All mesh vertices finite (no NaN/Inf)

## Failure Modes

- `search_parameter` returns `None`: Possible if `near` check fails after branch
  selection (shouldn't happen since ±2π shift gives same 3D point)
- Parametric area degeneracy: If unwrapping produces zero-area fragments, they
  are filtered by `tau_area` threshold
- Assembly open edges: If IC edge vertices aren't shared between face fragments
  (upstream IC construction issue, not addressed by this fix)

## Current Status

The fix eliminates parametric discontinuities but does NOT resolve the target tests
(RB2, RB8, MO4). The remaining failures are in **shell assembly** — IC edges on
torus faces have refs=1 (non-manifold) because the IC construction doesn't produce
shared edges between torus face fragments and adjacent box face fragments. This is
an upstream issue in the IC construction pipeline for full-revolution surfaces.

## Remaining Work

1. **IC vertex unification at patch seams**: After face division, unify IC vertices
   that correspond to the same 3D point on both shells.
2. **Investigate RB1/RB6 regression**: These tests regressed in the D1.6/D1.7 commits
   (boundary-coincident IC skip / all_on_boundary three-way logic). The regression
   is independent of this fix.
3. **RB2 timeout**: May need IC marching timeout + fallback for torus surfaces.
