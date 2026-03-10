# Concentric Cylinder Subtract

## Problem

`cyl_cyl_boolean()` divides by zero when cylinders are concentric (d=0).
Line ~1059: `a = (r1^2 - r2^2 + d^2) / (2*d)` produces NaN.

## Geometry

Concentric cylinder subtract produces a **tube** (hollow cylinder):
- Outer cylinder (blank): center=C, radius=R1, z=[z_min, z_max]
- Inner cylinder (tool): center=C, radius=R2, z=[z_min, z_max]
- Result: 4 faces (outer wall, inner wall, top annulus, bottom annulus)

## Cases

| Case | Condition | Result |
|------|-----------|--------|
| Normal tube | R2 < R1, same Z range | Tube with 4 faces |
| Partial Z | R2 < R1, tool Z subset of blank Z | Inner hole only in overlap Z |
| Tool encloses blank | R2 >= R1 | Error |
| Equal radius | R2 == R1 | Error (complete removal) |

## Topology (full Z overlap)

- 4 faces: outer cylinder wall, inner cylinder wall (inward normal), top annular cap, bottom annular cap
- 4 edges: outer top circle, outer bottom circle, inner top circle, inner bottom circle
- 2 vertices: top seam point, bottom seam point
- V-E+F = 2-4+4 = 2 (genus-0 with inner loop)

## Volume

V = pi * (R1^2 - R2^2) * height

## Reference

Mantyla [#16]: kemr creates inner loops for annular faces.
