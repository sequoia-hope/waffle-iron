# Spec: Cherchi 2020 Indirect Predicates for Mesh Arrangement

## Goal

Implement indirect geometric predicates per Cherchi et al. 2020 [#9] Sections 4.1-4.3.
These are the mathematical foundation for exact mesh arrangement — all geometric
decisions (orientation, point comparison, point-in-triangle) operate on implicit
intersection point representations without materializing coordinates.

## Research Basis

- Cherchi et al. 2020 [#9]: "Fast and Robust Mesh Arrangements using Floating-point Arithmetic"
- Local reference: `docs/references/cherchi-indirect-predicates-2020.md`
- Filter constants from Table 1 (Section 4.2.2)
- Point comparison from Section 4.3

## Implicit Point Types (Section 4.1)

### E — Explicit Point
Input vertex with known f64 coordinates. Trivial.

### L — Line-Plane Intersection (LPI)
Edge (q1, q2) intersects plane defined by triangle (r, s, t).
5 defining points. Coordinates are:
```
p_L = (λ_Lx/d_L, λ_Ly/d_L, λ_Lz/d_L)
d_L = det|(q1-q2), (s-r), (t-r)|
n = det|(q1-r), (s-r), (t-r)|
λ_Lx = d_L·q1x + n·(q2x - q1x)
```
Undefined if d_L = 0 (edge parallel to plane).

### T — Three-Plane Intersection (TPI)
Three non-coplanar triangles define a unique intersection point.
9 defining points (3 triangles × 3 vertices). Coordinates via 3×3 linear system.
Undefined if d_T = 0 (planes not linearly independent).

## Orient2d Predicates (Section 4.2)

10 variants for all E/L/T combinations:

| Variant | Points | Filter epsilon power |
|---------|--------|---------------------|
| EEE | E, E, E | δ² |
| LEE | L, E, E | δ⁵ |
| LLE | L, L, E | δ¹¹ |
| LLL | L, L, L | δ¹⁴ |
| TEE | T, E, E | δ⁸ |
| LTE | L, T, E | δ¹⁴ |
| LLT | L, L, T | δ¹⁷ |
| LTT | L, T, T | δ²⁰ |
| TTE | T, T, E | δ²⁰ |
| TTT | T, T, T | δ²⁶ |

Each operates on a 2D projection (drop one coordinate axis).

## Point Comparison (Section 4.3)

6 variants per axis for lexicographic sorting:

| Variant | Points | Filter epsilon power |
|---------|--------|---------------------|
| EE | E, E | exact |
| LE | L, E | δ⁴ |
| LL | L, L | δ⁷ |
| TE | T, E | δ⁷ |
| LT | L, T | δ¹⁰ |
| TT | T, T | δ¹³ |

## Two-Stage Filtering

Each predicate uses:
1. **Float with semi-static error bound** — compute expression and bound, if |result| > epsilon, return sign
2. **Expansion arithmetic fallback** — use Shewchuk adaptive expansions for guaranteed correct sign

(Interval arithmetic Stage 2 from the paper is skipped — no Rust interval crate. Float filter handles >99.99% of cases, expansion handles the rest.)

## Invariants

1. orient2d_indirect with 3 explicit points matches geometry_predicates::orient2d exactly
2. orient2d_indirect never returns wrong sign (correctness by expansion fallback)
3. point_compare_on_axis is a total order (antisymmetric, transitive)
4. LPI point is undefined (returns None) when edge is parallel to plane
5. TPI point is undefined (returns None) when planes are coplanar

## Branch Table

| Point combo | orient2d variant | Filter power | Expansion depth |
|---|---|---|---|
| All explicit | EEE | δ² | 2-expansion |
| 1 LPI + 2 explicit | LEE | δ⁵ | 5-expansion |
| 2 LPI + 1 explicit | LLE | δ¹¹ | 11-expansion |
| All LPI | LLL | δ¹⁴ | 14-expansion |
| 1 TPI + 2 explicit | TEE | δ⁸ | 8-expansion |
| Higher combos | see table | up to δ²⁶ | up to 26-expansion |

## Failure Modes

- LPI with d_L = 0: return None (caller must handle — edge parallel to plane)
- TPI with d_T = 0: return None (caller must handle — planes coplanar)
- Expansion arithmetic overflow: theoretically impossible with Shewchuk's algorithm (exact)
