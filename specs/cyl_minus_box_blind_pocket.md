# Spec: Cylinder-Minus-Box Blind Pocket Topology

## Goal

Handle all 4 cap-touching cases in `build_cyl_minus_enclosed_box` correctly,
mirroring the pattern already implemented in `build_box_minus_enclosed_cyl`.

## Problem

`build_cyl_minus_enclosed_box` unconditionally creates inner loops (holes) on
both cylinder caps and places box vertices at `cyl_z_min`/`cyl_z_max`. This
produces through-hole topology (chi=2) even when the box is strictly inside
the cylinder's Z-range (blind pocket, chi=4).

## Cap-Touching Branch Table

| Case | touches_bot | touches_top | Bot cap | Top cap | Extra faces | V | E | F | chi |
|------|------------|------------|---------|---------|-------------|---|---|---|-----|
| Through-hole | true | true | inner loop | inner loop | 0 | 10 | 15 | 7 | 2 |
| Top only | false | true | no loop | inner loop | +1 floor | 10 | 15 | 8 | 3* |
| Bot only | true | false | inner loop | no loop | +1 ceiling | 10 | 15 | 8 | 3* |
| Enclosed (blind) | false | false | no loop | no loop | +2 floor+ceil | 10 | 15 | 9 | 4 |

*chi=3 represents one closed outer shell + one open inner pocket touching one cap.
For the enclosed case, chi=4 = two disconnected closed genus-0 surfaces.

## Detection

```rust
let touches_bot = (box_z_min - cyl_z_min).abs() < TAU_COINCIDENT;
let touches_top = (box_z_max - cyl_z_max).abs() < TAU_COINCIDENT;
```

## Topology Rules

- When a cap IS touched: create inner rectangular loop on that cap face (hole)
- When a cap is NOT touched: leave cap face intact; create standalone rectangular
  face at the box Z position (pocket floor/ceiling) with inward-facing normal

## Oracle

- Through-hole (touches both): euler_target = 2 (single connected surface, genus 1)
- Enclosed void (touches neither): euler_target = 4 (two disconnected genus-0 surfaces)
- One-sided: euler_target = 3

For F0036-F0040 assay cases, box is always Z-centered inside cylinder,
so touches_neither applies: euler_target = 4.

## Research

- Ref #33 Stroud Ch.4: Inner shells in B-Rep boolean operations — void cavities
  produce disconnected shells with separate Euler characteristics
- Ref #16 Mantyla: Euler operators for inner loop creation (kemr)
