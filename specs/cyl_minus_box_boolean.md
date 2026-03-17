# Cylinder-Minus-Box Boolean (Sprint K)

## Summary

Analytical SSI-based boolean for `cylinder - box` when the box is fully enclosed
within the cylinder. Previously returned `NotSupported`; now builds a correct
B-Rep (cylinder with rectangular through-hole).

## Topology

Result: 7 faces
- 1 outer cylinder wall (with seam edge)
- 2 annular end caps (outer circle + inner rectangle hole via inner loops)
- 4 inner rectangular wall faces (planar, inward-facing normals)

Vertex count: 10 (4 box corners × 2 Z-levels + 2 cylinder seam vertices)
Edge count: 15 (4 rect edges × 2 caps + 4 vertical edges + 2 circle edges + 1 seam)
Euler check: V - E + F = 10 - 15 + 7 = 2 ✓

## Dispatch Cases

| Case | Result |
|------|--------|
| Box fully enclosed in cylinder (XY + Z) | `build_cyl_minus_enclosed_box` |
| Box disjoint from cylinder | Cylinder unchanged |
| Partial overlap | `NotSupported` (future work) |

## Spatial Helpers

- `box_enclosed_in_cyl(aabb, cyl)` — checks all 4 AABB corners lie within the
  cylinder circle in XY
- Reuses existing `box_cyl_disjoint`, `cyl_z_range`, `compute_rotated_box_aabb`

## Frame Rotation

All operations work in Z-aligned frame (cylinder direction → Z). Uses existing
`rotation_to_z` / `mat3_transpose` / `rotate_boolean_result` infrastructure.

## References

- Mantyla [#16]: Euler operators, half-edge B-Rep
- Barton [#24]: Frame normalization before boolean
- A15: Analytical primacy invariant
