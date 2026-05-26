# `cherchi-rs::predicates::segment_intersects_triangle_3d` — Spike PR-CR8

## Goal

Classify the 3D-spatial relationship between a segment `(p, q)` and a
triangle `(a, b, c)` as `Disjoint`, `Intersects`, or `Coplanar`.

Core primitive of Cherchi 2022 §3's non-coplanar triangle-triangle
intersection branch — for each (T1 edge, T2 triangle) and (T2 edge,
T1 triangle) pair, this test determines whether they share any point.

This is the first cherchi-rs predicate that uses **`Sign`-pattern
combinations** (multiple orient3d results combined via Sign-tagged
case analysis) to drive a 3-state enum classification. Establishes
the pattern for future intersection predicates (segment-segment,
triangle-triangle).

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `p, q` | `Point3` | Segment endpoints |
| `a, b, c` | `Point3` | Triangle vertices |

Returns: `SegmentTriangleIntersection` —
- `Disjoint`: segment does not touch the triangle
- `Intersects`: segment crosses or touches triangle (interior, edge,
  vertex, or endpoint on triangle)
- `Coplanar`: segment lies in triangle's plane (caller runs a separate
  2D segment-triangle algorithm to refine — not in scope here)

All inputs must be finite. NaN / infinite inputs are undefined.

## Branch table

| Input class | Output |
|-------------|--------|
| Both segment endpoints on same non-zero side of triangle's plane | `Disjoint` |
| Both segment endpoints on triangle's plane | `Coplanar` |
| Endpoints span plane, line passes outside triangle | `Disjoint` |
| Endpoints span plane, line passes through triangle | `Intersects` |
| One endpoint on plane, other on a side, that endpoint inside or on triangle | `Intersects` |
| One endpoint on plane, other on a side, that endpoint outside triangle | `Disjoint` |
| Segment endpoint == triangle vertex | `Intersects` |
| Segment lies entirely on triangle edge | `Coplanar` (caller handles 2D) |
| NaN / infinite inputs | Undefined |
| Degenerate triangle (collinear vertices) | Caller's responsibility to filter |

## Invariants

1. **Endpoint-swap symmetry**:
   `segment_intersects_triangle_3d(p, q, a, b, c) == segment_intersects_triangle_3d(q, p, a, b, c)`
   (swapping segment endpoints doesn't change the classification).
2. **Cyclic vertex-permutation invariance**: cyclic permutations of
   `(a, b, c)` preserve the result. Odd permutations flip orient3d's
   signs but the "all-same-sign" check is sign-flip-symmetric (both
   all-positive and all-negative satisfy it).
3. **Determinism**: same input → same output across runs and platforms
   (inherits from PR-CR6's `orient3d` and PR-CR1's exact predicates).

## Oracles

1. **Canonical Disjoint**:
   - Segment far above triangle, doesn't cross plane → `Disjoint`
   - Segment far below triangle → `Disjoint`
   - Segment crosses plane but line passes to the side → `Disjoint`
2. **Canonical Intersects**:
   - Segment crosses through interior (one endpoint above, one below)
   - Segment endpoint on triangle interior, other endpoint elsewhere
   - Segment endpoint on triangle vertex
   - Segment endpoint on triangle edge midpoint
3. **Coplanar**: both endpoints on triangle's plane
4. **Properties**: endpoint-swap symmetry; cyclic-vertex-permutation invariance
5. **Determinism**: 100 invocations identical

## Failure modes

- **NaN / infinite input**: undefined.
- **Degenerate triangle (collinear vertices)**: deterministic but may
  misclassify; caller's responsibility to filter via
  `points_are_collinear_3d` (PR-CR1).
- No error return; the enum is the only output.

## Research basis

- **Cherchi 2022 §3** — triangle-triangle intersection; the non-coplanar
  branch tests each edge of T1 against T2 (and vice versa) using a
  segment-triangle test of this kind
- **Shewchuk 1997 §2.1** — `orient3d` is the foundational predicate
  used throughout
- **Classic Möller-Trumbore-style approach** — adapted from
  floating-point to exact arithmetic via Shewchuk's `orient3d`

## Method

5 `orient3d` tests + Sign-pattern combination:

1. Compute `s_p = orient3d(a, b, c, p)` and `s_q = orient3d(a, b, c, q)`
   — which side of triangle's plane is each segment endpoint on?
2. If both `Zero` → **`Coplanar`**
3. If both have the same non-zero sign (both Positive or both Negative)
   → **`Disjoint`** (segment doesn't cross plane)
4. Otherwise, compute 3 line-vs-triangle-edge orientations:
   - `l_ab = orient3d(p, q, a, b)`
   - `l_bc = orient3d(p, q, b, c)`
   - `l_ca = orient3d(p, q, c, a)`
5. If any line-test is `Positive` AND any is `Negative` (mixed signs)
   → line passes outside triangle → **`Disjoint`**
6. Otherwise (all same sign, possibly with `Zero`) → line passes
   through (or along edge of) triangle → **`Intersects`**

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/segment_triangle.rs` header (new file):

```rust
//! 3D segment-triangle intersection classification.
//!
//! `segment_intersects_triangle_3d` is the core primitive of Cherchi
//! 2022 §3's non-coplanar triangle-triangle intersection branch:
//! for each pair of (T1 edge, T2 triangle) and (T2 edge, T1 triangle),
//! this test determines whether they share any point.
//!
//! Cherchi 2022 §3 (triangle-triangle intersection; non-coplanar branch).
//! Shewchuk 1997 §2.1 (orient3d as the foundational predicate).
//!
//! No specific cinolib function flagged in audit for this predicate —
//! the algorithm (5 orient3d tests + Sign-pattern combination) is
//! standard computational geometry (Möller-Trumbore-style, adapted to
//! exact arithmetic). The orient3d primitive is from PR-CR6's wrapper
//! over `geometry-predicates` (MIT).
```

(Plain attribution — no `Deliberate deviation`. The 3-state enum
collapses richer cinolib enums (which distinguish interior / boundary
/ on-vertex / on-edge) per YAGNI, documented in §"Scope discipline" below.)

## Scope discipline

This PR introduces:
- One `SegmentTriangleIntersection` enum (3 variants)
- One `segment_intersects_triangle_3d` function

Does NOT add:
- 2D segment-triangle (the `Coplanar` case) — caller handles separately
- Interior / Boundary distinction in the enum (richer cinolib variants)
  — current callers don't need it (YAGNI)
- Explicit intersection point computation — separate function later if needed
- Segment-segment-3d or other intersection predicates — separate PRs

## Verification

```bash
# RED phase
cargo test -p cherchi-rs predicates::segment_triangle
# expect: all tests FAIL (function unimplemented!())

# GREEN phase
cargo test -p cherchi-rs
# expect: 97 (PR-CR1-CR7) + 12 (PR-CR8) = 109 pass

cargo check --workspace
# expect: clean

cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```
