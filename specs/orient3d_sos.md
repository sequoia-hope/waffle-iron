# orient3d SoS Tiebreak

**Status:** COMPLETE
**Sprint:** 37
**File:** `vendor/truck/truck-shapeops/src/transversal/robust_classify.rs`

## Research Basis

- **[#5] Edelsbrunner & Mucke (1990)** — Simulation of Simplicity: virtual infinitesimal perturbation for deterministic degenerate-case resolution. Cofactor chain peeling (SignDet) for orient3d at D=3.
- **[#4] Shewchuk (1997)** — Adaptive precision floating-point for exact orient3d sign computation. The `robust` crate implements this.
- **[#19] Devillers & Preparata (1998)** — Filter failure probability for orient3d is ~10^-14 (essentially never fails at double precision).

## Goal

When `robust_orient3d(a, b, c, d)` returns exactly `0.0` (point `d` is coplanar
with triangle `(a, b, c)`), provide a deterministic non-zero tiebreak using
Simulation of Simplicity (SoS). This eliminates `None` returns from
`robust_ray_triangle_cross` for coplanar-origin and vertex-case degeneracies.

## Algorithm: Edelsbrunner-Mucke Cofactor Chain (D=3)

The SoS method for `orient3d` follows the same pattern as the existing
`sos_orient2d_tiebreak`:

1. Take the 4 input points `(a, b, c, d)` as indices `[0, 1, 2, 3]`.
2. Sort indices lexicographically by point coordinates (x, then y, then z).
3. Count the number of transpositions (swaps) required by the sort.
4. Return `+1` if the permutation parity is even, `-1` if odd.

This is the standard Edelsbrunner-Mucke (1990) approach: when the real orient3d
determinant is zero, the SoS infinitesimal perturbation makes the sign depend
solely on the lexicographic ordering of the input points.

### Key Properties

- Uses `f64::total_cmp` for bitwise-deterministic lexicographic comparison.
- Bubble sort on 4 elements counts transpositions exactly.
- Result is always `+1` or `-1` (never zero).

## Invariants

1. **Always non-zero:** Returns `+1` or `-1`, never `0`.
2. **Deterministic:** Same 4 points in same order always produce the same sign.
3. **Vertex-fan consistent:** For a ray through a shared vertex of N triangles,
   the SoS signs across the fan are consistent — exactly one triangle in each
   pair of adjacent triangles "owns" the vertex, ensuring correct crossing count.

## Integration into `robust_ray_triangle_cross`

### Change 1: Coplanar origin resolution

Replace the early `return None` when `orient_origin == 0.0` with SoS resolution.
When the origin is ON the plane:
- The "intersection point" is the origin itself (t = 0).
- Project to 2D and test containment using the existing orient2d + SoS pipeline.
- If inside the triangle projection, return `Some(1)` (crossing); otherwise `Some(0)`.

### Change 2: Remove zero_count >= 2 guard + derivative perturbation

The existing guard at `zero_count >= 2` returned `None` for vertex cases.
Removing the guard lets vertex-fan cases resolve deterministically.

The simple permutation-parity SoS for orient2d doesn't achieve vertex-fan
consistency (when the intersection point IS a triangle vertex, two orient2d
inputs have identical coordinates). The fix: replace the orient2d SoS tiebreak
with a **directional-derivative symbolic perturbation**:

For `orient2d(u, v, p)`, the derivative w.r.t. `p` in direction `δ = (1, π)` is:
```
deriv = (v[0] - u[0]) * π - (v[1] - u[1]) * 1
```

This is independent of `p`, ensuring vertex-fan consistency: when multiple
triangles share a vertex, exactly one triangle "claims" the crossing. The
irrational ratio `(1, π)` avoids new degeneracies with rational-coordinate edges.

## References

- Edelsbrunner & Mucke (1990), "Simulation of Simplicity"
- Shewchuk (1997), "Adaptive Precision Floating-Point Arithmetic"
- Existing `sos_orient2d_tiebreak` in same file (pattern to follow)
