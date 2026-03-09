# B28: Analytical Line ICs for Parallel Unequal-Radii Cylinders

## Problem

B27 introduced `try_detect_parallel_cylinders_skip` which suppresses ALL lateral-lateral
intersection curves (ICs) for parallel cylinders with unequal radii. This is incorrect when
the cylinders overlap radially — the lateral surfaces DO intersect along 2 straight lines
parallel to the shared axis. Suppressing these ICs means face division relies solely on
plane-cylinder cap ICs, which may be insufficient for correct topology, producing 4+ open edges.

The perturbation cascade masks this by shifting geometry until mesh-based IC extraction
avoids degeneracy, but this is non-deterministic and scale-dependent.

## Geometry

Two cylinders with parallel axes, unequal radii r0 and r1, axis separation s (perpendicular
distance between axes):

| Case | Condition | Lateral ICs |
|------|-----------|-------------|
| Disjoint | s >= r0 + r1 | 0 (no intersection) |
| Contained | s <= \|r0 - r1\| | 0 (one inside the other) |
| Overlapping | \|r0 - r1\| < s < r0 + r1 | 2 lines parallel to axis |
| Tangent | s = r0 + r1 or s = \|r0 - r1\| | 1 line (degenerate) |

## Algorithm

For the overlapping case, project onto the cross-section plane perpendicular to the axis:
- Circle 0: center at origin, radius r0
- Circle 1: center at (s, 0), radius r1
- Intersection x-coordinate: x = (s² + r0² - r1²) / (2s)
- Intersection y-coordinate: y = ±sqrt(r0² - x²)

These two points define two lines in 3D parallel to the cylinder axis direction.

The intersection is computed in closed form (no iteration, no mesh extraction).

## Branch Table

```
detect_cylinder(surface0) → cyl0?
detect_cylinder(surface1) → cyl1?
  → Neither detected: return None (not our case)
axes parallel (|cos| > 1 - 1e-6)?
  → No: return None (let existing cylinder-cylinder ellipse path handle)
radii nearly equal (|r0-r1|/rmax <= 0.01)?
  → Yes: return None (let existing equal-radius path handle)
compute perpendicular distance s
  → s >= r0+r1: return empty ICs (disjoint)
  → s <= |r0-r1|: return empty ICs (contained)
  → otherwise: compute 2 line ICs via circle-circle intersection
```

## Oracles

1. **0 open edges** on direct boolean (no perturbation)
2. **V-E+F=2** (Euler characteristic for genus-0 solid)
3. **Volume monotonically decreasing** through boss → cut1 → cut2
4. **Scale independence**: identical topology at 1m, 1mm, 1μm
5. **No perturbation cascade invoked** for parallel-cylinder subtract

## Replaces

- `try_detect_parallel_cylinders_skip` (B27, `analytical.rs:2151-2203`) — returns empty ICs
- B27 early return in `intersection_curves` (`mod.rs:269-275`) — skips mesh extraction

## References

- Circle-circle intersection: standard computational geometry formula
- Existing infrastructure: `AnalyticalIC.line_segments`, `generate_polylines`, `clip_polylines_to_domain`
