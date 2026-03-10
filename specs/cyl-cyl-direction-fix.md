# Cylinder-Cylinder Boolean Direction Fix

**Status**: Active
**Ref**: #33 Stroud (B-Rep boolean pipeline)

## Bug

`cyl_cyl_boolean()` and SSI functions compute Z ranges as
`center_bottom[2]` to `center_bottom[2] + depth`, ignoring the `direction`
field. When a cut extrude reverses direction to `[0,0,-1]`, the Z range is
wrong:

- Tool at z=D1 with direction [0,0,-1], depth=D2 -> code says Z=[D1, D1+D2]
  but actual extent is [D1-D2, D1].
- This causes phantom intersections or missed intersections ("no Z overlap").

## Fix

Add `cyl_z_range(cyl) -> (f64, f64)` helper that computes:
```
z0 = center_bottom[2]
z1 = z0 + depth * direction[2]
return (min(z0,z1), max(z0,z1))
```

Replace all inline Z range computations in `ssi.rs` and `boolean.rs`.

## Invariants

- `cyl_z_range` always returns `(lo, hi)` where `lo <= hi`.
- Existing tests (direction=[0,0,1]) produce identical results.
- Reversed-direction cylinders get correct Z extents.

## Oracles

- Volume of subtract(A, reversed-B) = volume of subtract(A, forward-B) for
  same geometry.
- Watertight mesh after subtract with reversed direction.
- No-overlap case correctly rejects when Z ranges don't intersect.
