# Spec: Boolean Tessellation Routing Fix

## Goal

Fix AABB collapse in tessellation output for boolean results that used the
polygon-clipping fallback from the SSI pipeline.

## Problem

When `ssi_boolean_op` returns `NotSupported` (e.g., partial cylinder-box overlap),
the dispatch falls back to `boolean_op` (polygon clipping). The result's
`is_polygon_soup` flag remains `false`, causing `tessellate_solid_bounded` to be
used. Bounded tessellation assumes clean analytical B-Rep topology, but the
polygon-clipping result has reconstructed topology that it mishandles.

## Parameters

- **Affected path**: `waffle_kernel.rs` SSI fallback (lines 179-204)
- **Affected cases**: Cylinder-box partial subtract (F0036-F0045 in assay)
- **Observable symptom**: All output vertices lie on AABB faces

## Branch Table

| SSI result | Polygon fallback | polygon_soup flag | Tessellation path | Status |
|---|---|---|---|---|
| Ok | N/A | false | bounded | CORRECT |
| NotSupported → polygon Ok | Used | false | bounded | BUG: AABB collapse |
| NotSupported → polygon Ok | Used | true | fan | FIX |
| NotSupported → tolerant Ok | Used | true | fan | FIX |

## Invariants

1. Any boolean result produced by polygon clipping MUST have `is_polygon_soup = true`
2. Only pure SSI results (analytical B-Rep) may have `is_polygon_soup = false`

## Oracles

1. Tessellation of cyl-minus-box partial subtract produces > 0 non-AABB vertices
2. Volume of result is strictly less than volume of cylinder alone
3. Euler formula: V - E + F = 2 for the boolean result topology

## Failure Modes

- If `is_polygon_soup = true` produces non-watertight output: acceptable tradeoff
  (non-watertight is better than AABB-collapsed)

## Research Basis

- Barton et al. [#24]: hybrid mesh/analytical boolean pipeline
- Boolean pipeline code comments: "polygon-soup B-Rep from S-H clipping may
  contain internal faces"
