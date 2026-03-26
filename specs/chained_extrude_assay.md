# Spec: Chained Extrude Assay Cases (F0063-F0072)

## Goal

Test long chains of stacked extrusions (5 to 20 steps). Each step draws a
closed shape on the top face of the previous extrusion and extrudes upward.
This exercises the boolean merge pipeline under repeated application.

## Parameters

| Case    | Chain Length | Seed  |
|---------|-------------|-------|
| F0063   | 5           | 10001 |
| F0064   | 5           | 10002 |
| F0065   | 8           | 10003 |
| F0066   | 8           | 10004 |
| F0067   | 10          | 10005 |
| F0068   | 12          | 10006 |
| F0069   | 15          | 10007 |
| F0070   | 15          | 10008 |
| F0071   | 20          | 10009 |
| F0072   | 20          | 10010 |

## Sketch Profiles

Each step uses one of 4 profile types, selected by `seed % 4`:

- **L-shape**: 6-vertex polygon (rectangle with one corner cut away)
- **T-shape**: 8-vertex polygon (rectangle with a tab on top center)
- **Notched rectangle**: 8-vertex polygon (rectangle with a rectangular notch on one side)
- **Plus/cross**: 12-vertex polygon (plus sign shape)

All profiles are centered to include the origin (0,0) in 2D sketch space,
ensuring every extrusion overlaps the Z-axis. This guarantees all boolean
unions are non-disjoint.

Profile sizes: 0.15-0.5 (randomized per step).
Extrude depths: 0.1-0.3 (randomized per step).

## Stacking Geometry

- Step 1: sketch at origin [0,0,0], normal [0,0,1], extrude depth d1
- Step 2: sketch at origin [0,0,d1], normal [0,0,1], extrude depth d2
- Step N: sketch at origin [0,0, sum(d1..d(N-1))], normal [0,0,1]

All extrudes are boss (not cut), merge=true.

## Invariants

- All operations use the same normal [0,0,1] (stacking upward)
- Every profile contains the 2D origin, so all solids share the Z-axis
- No disjoint unions possible by construction
- Euler characteristic: 2 (single connected solid, no through-holes)
- Watertight mesh expected
- Volume monotonically increases at each step
- BBox extent grows linearly with chain length

## Oracles

- `euler_target`: 2
- `expect_watertight`: true
- `volume_monotonicity`: ["increase"] × chain_length
- `max_bbox_extent`: scale × (3 + chain_length × 0.5) — conservative
- `expect_rebuild_error`: false

## Failure Modes

- Boolean merge failure on any step → engine error (partial rebuild)
- Accumulated floating-point drift at step 15-20 → potential geometry corruption
- Performance regression: 20-step chains should complete in < 30s
