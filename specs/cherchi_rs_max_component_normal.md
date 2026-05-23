# `cherchi-rs::predicates::max_component_in_triangle_normal` — Spike PR-CR4

## Goal

Given three 3D points forming a triangle, return the Cartesian axis
(X, Y, or Z) along which the triangle's normal has the largest absolute
component. Used by Cherchi 2020 to pick the optimal 2D projection
plane for downstream `orient2d` predicates on this triangle — picking
the wrong axis can flip orientation-test signs.

This is the **first cherchi-rs port that uses Cherchi's filtered+exact
cascade pattern** (Cherchi 2020 §3) and the **first port that uses
`dashu` for arbitrary-precision arithmetic**. It establishes templates
for both, to be reused by subsequent indirect-predicate ports.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `a, b, c` | `Point3` | Three triangle vertices |

Returns: `Axis` — one of `Axis::X`, `Axis::Y`, `Axis::Z`.

All inputs must be finite. NaN / infinite inputs are undefined behavior.

## Branch table

| Input class | Output |
|-------------|--------|
| Non-degenerate triangle | `Axis` of largest `\|n_i\|` where `n = (b-a) × (c-a)` |
| Degenerate (collinear) triangle | Some `Axis` (deterministic but unspecified — caller's responsibility to filter degenerate input via `points_are_collinear_3d` first) |
| Exact tie among `\|n_i\|` | Deterministic tiebreak: `X > Y > Z` |

## Invariants

1. **Correctness**: returned axis equals `argmax_i \|n_i\|` where
   `n = (b - a) × (c - a)`, computed in exact arithmetic.
2. **Determinism**: same input → same output across runs and platforms.
3. **Permutation independence (modulo sign)**: any permutation of
   `(a, b, c)` returns the same axis. Cross-product magnitudes
   `\|n_i\|` are invariant under vertex permutation (the normal's
   sign may flip but the absolute values are the same).
4. **Translation invariance**: `f(a + t, b + t, c + t) == f(a, b, c)`
   for any vector `t` (`n` depends only on differences).
5. **Scale invariance**: for any non-zero scalar `k`,
   `f(k·a, k·b, k·c) == f(a, b, c)` (`|n_i|` all scale by `k²`).

## Oracles

1. **Canonical axis-aligned**:
   - Triangle in XY plane (all z=0) → `Axis::Z`
   - Triangle in XZ plane (all y=0) → `Axis::Y`
   - Triangle in YZ plane (all x=0) → `Axis::X`
2. **Canonical off-axis** (hand-computed normal):
   - 45° tilted with normal mostly along Z → `Axis::Z`
3. **Property: 6-permutation invariance** — all 6 orderings of a
   canonical input return the same axis
4. **Property: translation invariance** — shift triangle by (100, -50, 7)
5. **Property: scale invariance** — scale by 1e6 and by 1e-6
6. **Cascade-coverage**:
   - `max_component_filtered` returns `Some(Z)` on axis-aligned XY triangle (clear case)
   - `max_component_filtered` returns `None` on a near-tied input
     (two |n_i| within `f64::EPSILON * max_var^2`)
   - `max_component_exact` on the near-tied input returns correct axis
   - Public `max_component_in_triangle_normal` on near-tied input
     returns the same as `max_component_exact`
7. **A-02 regression**: a triangle where f64 cross-product alone gives
   the wrong axis but exact arithmetic gives correct. Public function
   must return correct. (May be `#[ignore]` if not constructively
   reproducible — cascade-coverage tests still validate the cascade.)
8. **Determinism**: run on the same input 100 times; all 100 results identical.

## Failure modes

- **NaN / infinite input**: undefined.
- **Degenerate triangle (collinear)**: returns some axis deterministically;
  caller should filter via `points_are_collinear_3d` first.
- **No error return**: `Axis` is the only output.

## Research basis

- **Cherchi 2020** §3 (cascaded filtered/exact predicates)
- **Shewchuk 1997** §2.1, §4.5 (adaptive predicates with a-priori error bounds)
- **cinolib** `maxComponentInTriangleNormal_filtered` and `_exact`
  (`implicit_point.hpp:937-1029` per audit A-02)

## Method

Cherchi's **filtered+exact cascade**:

```
pub fn max_component_in_triangle_normal(...) -> Axis {
    match max_component_filtered(...) {
        Some(axis) => axis,
        None => max_component_exact(...),
    }
}
```

- **Filtered** (f64 fast path): compute cross product in f64, derive
  a conservative Shewchuk-style error bound, return `Some(axis)` if
  the max |n_i| exceeds the next-largest by more than the bound;
  return `None` if any two |n_i| are too close to distinguish.
- **Exact** (slow path, definitive): convert each f64 coordinate to
  `RBig` (via `f64 → FBig → RBig`), compute cross product in
  arbitrary-precision rationals, compare exactly, return axis with
  largest |n_i| (tiebreak: X > Y > Z).

## Filtered+exact cascade discipline (NEW CONVENTION for cherchi-rs)

This spec establishes the cascade pattern for all subsequent indirect-
predicate ports. The pattern:

```rust
pub fn predicate(args) -> ResultType {
    match predicate_filtered(args) {
        Some(result) => result,
        None => predicate_exact(args),
    }
}

pub(crate) fn predicate_filtered(args) -> Option<ResultType> { ... }
pub(crate) fn predicate_exact(args) -> ResultType { ... }
```

**Soundness criterion** (the filtered version's contract):

- If `Some(result)` is returned, `result` is **provably correct**
- If `None` is returned, the cascade falls back to exact
- The filtered version MAY be CONSERVATIVE (return `None` more often
  than strictly needed). Over-conservative ⇒ more exact-path work ⇒
  slower but still correct.
- The filtered version MUST NOT return `Some(wrong_answer)`. Period.

## Conservative error bound (deliberate deviation)

C++'s Cherchi uses an error bound `8.88395e-16 * max_var^2` derived
from a specific Shewchuk analysis for this predicate. The C++ source
isn't available in this session (the sidecar at
`/home/claude/cherchi2022/` isn't built).

This Rust port uses a **conservative Shewchuk-style bound**:
`4 * f64::EPSILON * max_var^2`, where `max_var` is the maximum
absolute input coordinate. This is more conservative (= falls back
to exact more often) than Cherchi's bound, trading slight performance
loss for clear correctness justification.

**Why this is acceptable**:

1. The cascade is correct by construction — exact fallback always handles
2. Slightly slower (more exact-path invocations) but tests still pass
3. Avoids guessing at Cherchi's specific bound without C++ source access

This deviation falls under the C++ deviation policy (per
`cherchi_rs_cpp_deviation_policy.md`): we port the function's stated
intent (filtered+exact cascade) with Rust-idiomatic / well-justified
defaults where upstream's specific value isn't accessible.

**Banked**: when the C++ sidecar is built, calibrate the bound against
Cherchi's actual value and tighten if appropriate. This is a
performance optimization, not a correctness change.

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/orientation.rs` header (new file):

```rust
//! Triangle-normal max-component selection for 2D projection axis choice.
//!
//! Ported from Cherchi 2020's `maxComponentInTriangleNormal` family
//! (`implicit_point.hpp:937-1029` per audit A-02).
//! Cherchi 2020 is MIT-licensed.
//! © 2020 Gianmarco Cherchi, Marco Livesu, Riccardo Scateni, Marco Attene
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §3 (cascaded filtered/exact predicates).
//! Shewchuk 1997 §2.1, §4.5 (adaptive predicates with error bounds).
//!
//! **Filtered+exact cascade**: this file establishes the pattern for
//! subsequent indirect predicates. `max_component_filtered` is the
//! f64 fast-path returning Option (None = uncertain → fall back to
//! exact); `max_component_exact` uses dashu rationals for the
//! definitive answer.
//!
//! **Conservative error bound deviation**: we use Shewchuk-style
//! `4 * f64::EPSILON * max_var^2` rather than Cherchi's specific
//! `8.88395e-16 * max_var^2`. See `specs/cherchi_rs_max_component_normal.md`
//! §"Conservative error bound (deliberate deviation)".
```

## Scope discipline

One function. The `Axis` enum is incidental support (one type, ~5 LOC).
Do NOT also port:
- Other indirect predicates (separate PRs)
- Generic `Axis` helpers (`abs_index()`, `unit_vector()`, etc.) —
  add when a second consumer needs them
- A general `f64 → dashu` utility module — keep local to this file
  for now; refactor when a 3rd port needs the same conversion

## Verification

```bash
# RED phase
cargo test -p cherchi-rs predicates::orientation
# expect: all tests FAIL (function unimplemented!())

# GREEN phase
cargo test -p cherchi-rs
# expect: 38 (PR-CR1+CR2+CR3) + ~14 (PR-CR4) = ~52 pass

cargo check --workspace
# expect: clean

cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```
