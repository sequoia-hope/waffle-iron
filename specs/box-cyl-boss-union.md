# Box-Cylinder Boss-on-Top Union

## Problem

`box_cyl_disjoint()` returns `true` for Z-touching surfaces due to tolerance check
`cyl_z_max < aabb.min[2] + 1e-9`. This causes `build_disjoint_box_cyl_union()` to
create two separate shells. Volume = box only (cylinder tessellation fails because
`cylinder_params: None` in merged result).

## Geometry

- Box: 10x10x10 at origin (Z=[0,10])
- Cylinder: center=(5,5), r=4, sits ON TOP of box (Z=[10,15])
- Expected: single merged solid with annular top face

## Fix

1. Fix `box_cyl_disjoint()` Z tolerance to treat Z-touching as NOT disjoint
2. Add Z-touching boss case to box-cyl union: detect cylinder bottom Z == box top Z
3. Build: box with annular top face + cylinder wall + cylinder top cap

## Topology

- Box 4 side faces + box bottom face + annular top face + cyl wall + cyl top cap = 8 faces
- V-E+F = 2

## Volume

V = box_volume + cylinder_volume = 1000 + pi * 16 * 5 = 1251.33
