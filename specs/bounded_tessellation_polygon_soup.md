# Bounded Tessellation Guard Relaxation

**Sprint**: J
**Status**: Complete
**Author**: Claude

## Problem

The watertight-by-construction **bounded tessellation** path (`tessellate_solid_bounded`)
was guarded by overly restrictive conditions:
1. `!is_polygon_soup` — blocks all stitched boolean results
2. `has_circles && !has_arcs` — requires circular edges, excludes all-linear results

The `has_circles` requirement prevented box primitives (all-linear edges) from using
the bounded path, even though bounded tessellation handles linear edges correctly.

## Solution

Remove the `has_circles` requirement, keeping `!is_polygon_soup` and `!has_arcs`.

```rust
// BEFORE:
if cylinder_params.is_none() && revolve_params.is_none() && !is_polygon_soup {
    let has_circles = ...;
    let has_arcs = ...;
    if has_circles && !has_arcs { return tessellate_solid_bounded(...); }
}

// AFTER:
if cylinder_params.is_none() && revolve_params.is_none() && !is_polygon_soup {
    let has_arcs = ...;
    if !has_arcs { return tessellate_solid_bounded(...); }
}
```

## Why `is_polygon_soup` guard is kept

Investigation showed that polygon-soup B-Rep from S-H clipping may contain
**internal faces** (faces inside the solid that should not be on the boundary).
Bounded tessellation's shared vertices make internal face triangles share edges
with external face triangles, preventing `remove_isolated_triangles` from
identifying them. The fan path's per-face vertices make internal faces isolated,
allowing correct removal.

Fixing this requires B-Rep-level internal face removal (winding number test)
before tessellation — a larger change for a future sprint.

## What changed

- Box primitives now use bounded path (watertight by construction, 12 triangles)
- Box-cyl SSI results without circular edges now use bounded path
- Polygon-soup (S-H clipping results) still uses fan tessellation
- Cyl-cyl results (has arcs) still use specialized arc tessellation
