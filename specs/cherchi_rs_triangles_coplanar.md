# `cherchi-rs::predicates::triangles_are_coplanar` — Spike PR-CR7

## Goal

Test whether two 3D triangles `T1 = (a, b, c)` and `T2 = (d, e, f)` lie
in the same plane. First step of Cherchi 2022 §3's triangle-triangle
intersection algorithm — the algorithm branches on coplanarity, with
different sub-algorithms for the coplanar vs. non-coplanar cases.

This is the **first inter-primitive predicate in cherchi-rs**: prior
ports were single-element predicates (point, triangle, point+triangle).
PR-CR7 establishes the pattern for predicates that test a relationship
between two of the same primitive.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `a, b, c` | `Point3` | Vertices of `T1` |
| `d, e, f` | `Point3` | Vertices of `T2` |

Returns: `bool` — `true` iff all 6 vertices lie in a common plane.

All inputs must be finite. NaN / infinite inputs are undefined.

## Branch table

| Input class | Output |
|-------------|--------|
| Both triangles non-degenerate, in same plane | `true` |
| Both triangles non-degenerate, in different planes | `false` |
| Triangles share an edge in same plane | `true` |
| Triangles share a vertex but different planes | `false` |
| Identical triangles (same 3 vertices) | `true` |
| Translated copy in same plane | `true` |
| Parallel planes (different `z` for axis-aligned XY triangles) | `false` |
| Degenerate triangle (collinear vertices) | Deterministic; may return `true` even when geometric interpretation is ambiguous (caller's responsibility to filter via `points_are_collinear_3d` first) |
| NaN / infinite inputs | Undefined |

## Invariants

1. **Symmetry**: `triangles_are_coplanar(T1, T2) == triangles_are_coplanar(T2, T1)`
   for all triangle orderings.
2. **Vertex-permutation invariance**: any permutation of `(a, b, c)`
   or of `(d, e, f)` preserves the result. (`orient3d`'s antisymmetry
   cancels in the `== Zero` test.)
3. **Determinism**: same input → same output across runs and platforms.

## Oracles

1. **Canonical coplanar**:
   - Two distinct triangles, both in `z=0` plane → `true`
   - Identical triangle (same 3 vertices) → `true`
   - Translated copy in `z=0` plane → `true`
   - Both triangles in tilted plane `x + y + z = 1` → `true`
   - Triangles sharing an edge in `z=0` plane → `true`
2. **Canonical non-coplanar**:
   - Triangle in `z=0` plane, triangle in `z=1` plane → `false`
   - Triangle in XY plane, triangle in XZ plane sharing origin → `false`
   - Triangle in XY plane, triangle in tilted plane → `false`
3. **Property: symmetry** — swapping `(a,b,c)` with `(d,e,f)` preserves result
4. **Property: vertex-permutation invariance** — all 6 permutations of
   `(a, b, c)` give the same result
5. **Determinism** — 100 invocations identical

## Failure modes

- **NaN / infinite input**: undefined.
- **Degenerate triangle (collinear vertices)**: deterministic but may
  falsely report coplanar. `orient3d` returns `Zero` for any 4th
  point against a collinear "triangle"; the reverse-direction check
  partially mitigates this for ONE degenerate triangle; both degenerate
  is ambiguous. Caller's responsibility to filter via
  `points_are_collinear_3d` (PR-CR1) first.
- No error return; `bool` is the only output.

## Research basis

- **Cherchi 2022 §3** — triangle-triangle intersection branches on
  coplanarity test as the first step
- **Shewchuk 1997 §2.1** — `orient3d` is the predicate that determines
  whether 4 points are coplanar (returns `Zero` iff coplanar)

## Method

6 `orient3d` tests using PR-CR6's `Sign`-returning wrapper. Each vertex
of each triangle is tested against the other triangle's plane:

```
triangles_are_coplanar(a, b, c, d, e, f) :=
       orient3d(a, b, c, d) == Zero
    && orient3d(a, b, c, e) == Zero
    && orient3d(a, b, c, f) == Zero
    && orient3d(d, e, f, a) == Zero
    && orient3d(d, e, f, b) == Zero
    && orient3d(d, e, f, c) == Zero
```

## Robustness: why 6 tests, not 3?

The naïve test is 3 calls (each vertex of T2 against T1's plane). This
works for non-degenerate T1.

But if T1 is degenerate (a, b, c collinear), `orient3d(a, b, c, X)`
for any X returns `Zero` — the "tetrahedron" is degenerate regardless
of X. The 3-call test would falsely return `true` for any T2.

The reverse-direction check (each vertex of T1 against T2's plane)
catches this: if T2 is non-degenerate and T1's vertices don't lie on
T2's plane, the second 3 tests return non-zero.

If both triangles are degenerate, all 6 tests return `Zero` and the
function returns `true` — which is acceptable but not geometrically
meaningful (degenerate triangles aren't really triangles). Caller's
responsibility to filter.

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/triangle_pair.rs` header (new file):

```rust
//! Inter-primitive predicates: relationships between pairs of triangles.
//!
//! `triangles_are_coplanar` is the first step of Cherchi 2022 §3's
//! triangle-triangle intersection algorithm: the algorithm branches on
//! coplanarity (different sub-algorithms for coplanar vs. non-coplanar).
//!
//! Cherchi 2022 §3 (triangle-triangle intersection; coplanarity branch).
//! Shewchuk 1997 §2.1 (orient3d as the foundational coplanarity predicate).
//!
//! No specific cinolib function flagged in audit for this predicate —
//! the algorithm (6 orient3d tests, robust against single-degenerate-
//! triangle inputs) is standard computational geometry. The orient3d
//! primitive itself is from `geometry-predicates` (MIT) via PR-CR6's
//! wrapper.
```

(Plain attribution — no `Deliberate deviation` subsection. The
implementation is standard. The 6-test robustness choice is documented
in this spec's §"Robustness" but isn't a deviation from any specific
upstream.)

## Scope discipline

One function. Plus its tests. Do NOT also port:
- Triangle-triangle intersection (separate, bigger PR)
- A `Triangle` newtype (YAGNI; flat 6-arg matches existing predicates)
- Segment-segment / point-segment predicates (separate PRs)

## Verification

```bash
# RED phase
cargo test -p cherchi-rs predicates::triangle_pair
# expect: all 12 new tests FAIL (function unimplemented!())

# GREEN phase
cargo test -p cherchi-rs
# expect: 84 (PR-CR1-CR6) + 12 (PR-CR7) = 96 pass

# Workspace check
cargo check --workspace
# expect: clean

# Legacy regression
cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```
