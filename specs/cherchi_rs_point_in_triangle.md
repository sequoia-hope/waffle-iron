# `cherchi-rs::predicates::point_in_triangle_3d` — Spike PR-CR5

## Goal

Classify a 3D point's location relative to a 3D triangle as
`StrictlyInside`, `OnBoundary`, or `StrictlyOutside`, using cinolib's
robust **all-three-projections** approach (per audit B-07 in
`docs/audits/cherchi_port_audit.md:328-336`).

The cinolib variant tests the point against the triangle in each of
the 3 cardinal 2D projections (XY, XZ, YZ) and AND-combines the
results — `StrictlyInside` only if all 3 projections agree on
strictly inside. This catches non-coplanar points that the
dominant-axis-only variant (legacy Rust port) would misclassify.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `p` | `Point3` | Query point |
| `a, b, c` | `Point3` | Triangle vertices |

Returns: `PointLocation` — one of `StrictlyInside`, `OnBoundary`,
`StrictlyOutside`.

All inputs must be finite. NaN / infinite → undefined.

## Branch table

| Input class | Result |
|-------------|--------|
| `p` strictly interior + coplanar with triangle | `StrictlyInside` |
| `p` equals one of `a, b, c` | `OnBoundary` |
| `p` on an edge of triangle, coplanar | `OnBoundary` |
| `p` outside triangle, coplanar | `StrictlyOutside` |
| `p` not coplanar with triangle | `StrictlyOutside` (3-projection AND catches it) |
| Degenerate triangle (collinear a, b, c) | Caller's responsibility; deterministic but unspecified |
| NaN / infinite | Undefined |

## Invariants

1. **Vertex permutation invariance**: swapping any two of `(a, b, c)`
   flips the triangle's winding but does NOT change the classification.
2. **Determinism**: same input → same output across runs and platforms.
3. **Non-coplanar rejection**: if `p` is not coplanar with `(a, b, c)`,
   result is NOT `StrictlyInside`. (This is the cinolib robustness
   property — dominant-axis-only could wrongly classify it as inside.)
4. **Coplanar-inside ⟹ all 3 projections agree**: if `p` is
   strictly interior + coplanar, all 3 cardinal projections classify
   `p` as strictly inside in 2D.

## Oracles

1. **Canonical classifications** on axis-aligned triangle
   `a=(0,0,0), b=(1,0,0), c=(0,1,0)`:
   - Interior `(0.25, 0.25, 0)` → `StrictlyInside`
   - Vertex `(0, 0, 0)` → `OnBoundary`
   - Edge midpoint `(0.5, 0, 0)` → `OnBoundary`
   - Coplanar outside `(2, 0, 0)` → `StrictlyOutside`
   - Far away `(10, 10, 10)` → `StrictlyOutside`
2. **Non-axis-aligned triangle** (e.g., in plane `x+y+z=1`):
   - Interior coplanar point → `StrictlyInside`
   - Coplanar outside → `StrictlyOutside`
3. **B-07 regression**: triangle `(0,0,0), (1,0,0), (0,1,0)` + point
   `(0.25, 0.25, 0.5)` — projects to interior in XY but is off-plane.
   Result must NOT be `StrictlyInside`. (Either `OnBoundary` or
   `StrictlyOutside` is acceptable; legacy dominant-axis-only would
   have returned `StrictlyInside`.)
4. **Vertex permutation invariance** — all 6 orderings of `(a, b, c)`
   yield the same classification.
5. **Direct 2D primitive tests**: `point_in_triangle_2d` on canonical
   2D inputs (interior, vertex, edge midpoint, far-away outside,
   degenerate triangle).
6. **Determinism** — 100 invocations on the same input yield
   identical results.

## Failure modes

- NaN / infinite inputs → undefined.
- Degenerate (collinear) triangle → deterministic but unspecified.
- No error return; `PointLocation` is the only output.

## Research basis

- **Cherchi 2022 §3** — point-in-triangle as primitive for triangle-
  triangle intersection.
- **cinolib `point_in_triangle_3d`** — `predicates.cpp:447-481` per
  audit B-07. Uses the robust all-three-projections approach.
- **Shewchuk 1997 §4.5** — `orient2d` is the underlying primitive.

## Method

3-projection AND-combine:

```
for each axis in {X, Y, Z}:
    drop axis from p, a, b, c to get 2D
    classify with point_in_triangle_2d using orient2d

combine:
    if ANY projection is StrictlyOutside → StrictlyOutside
    elif ALL projections are StrictlyInside → StrictlyInside
    else → OnBoundary
```

2D primitive uses 3 `orient2d` sign tests:

```
s_ab = orient2d(a, b, p)
s_bc = orient2d(b, c, p)
s_ca = orient2d(c, a, p)

if some s_i == 0 and signs are mixed → StrictlyOutside (on edge extension)
elif some s_i == 0 → OnBoundary
elif all positive OR all negative → StrictlyInside
else → StrictlyOutside (mixed non-zero signs)
```

## Deliberate deviation from cinolib (simplification)

cinolib's function returns more granular boundary info — which
specific edge or vertex was hit. Our `PointLocation` enum collapses
this to `OnBoundary` because no current cherchi-rs caller needs the
granular info.

**Reason**: YAGNI — granular boundary info adds enum variants for no
current consumer. Easy to expand later.

**Risk**: a future port that needs "which edge" must either upgrade
the enum (breaks API) or call `orient2d` separately. Both manageable.

**Tested**: no specific deviation regression test (we're RESTRICTING
output, not changing the inside/boundary/outside answer).

Per `cherchi_rs_cpp_deviation_policy.md`: documented in spec +
per-file header.

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/point_in_triangle.rs` header (new file):

```rust
//! 3D point-in-triangle classification via cinolib's robust
//! all-three-projections approach.
//!
//! Ported from cinolib's `point_in_triangle_3d` (`predicates.cpp:447-481`
//! per audit B-07). cinolib is MIT-licensed.
//! © Marco Livesu et al. — https://github.com/mlivesu/cinolib
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2022 §3 (point-in-triangle as primitive for triangle-triangle
//! intersection).
//!
//! **Deliberate deviation from cinolib**: cinolib returns granular
//! boundary info (which edge / vertex was hit). Our `PointLocation`
//! enum collapses these to `OnBoundary` (YAGNI). See
//! `specs/cherchi_rs_point_in_triangle.md` §"Deliberate deviation from
//! cinolib".
//!
//! **B-07 correctness improvement**: legacy Rust port tested only the
//! dominant-axis projection, misclassifying non-coplanar points
//! projected over the triangle interior as `StrictlyInside`. cinolib
//! variant tests ALL THREE projections and AND-combines, catching
//! this case via at least one projection's interior test failing.
```

## Scope discipline

One function. Plus its 2D primitive helper. Plus the `PointLocation`
enum. Do NOT also port:
- Granular `OnEdge(i)` / `OnVertex(i)` variants (deferred per deviation §)
- Other classify-style predicates (separate PRs)
- A general "drop axis" utility module (keep helpers local to this file)

## Verification

```bash
# RED phase
cargo test -p cherchi-rs predicates::point_in_triangle
# expect: all 14 new tests FAIL (function unimplemented!())

# GREEN phase
cargo test -p cherchi-rs
# expect: 53 (PR-CR1-CR4) + 14 (PR-CR5) = 67 pass

cargo check --workspace
# expect: clean

cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```
