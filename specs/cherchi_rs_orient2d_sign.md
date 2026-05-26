# `cherchi-rs::predicates::orient2d` — Spike PR-CR10

## Goal

2D orientation predicate returning a 3-valued `Sign`. Mirrors PR-CR6's
`orient3d` wrapper one dimension lower. Wraps Shewchuk's adaptive
`orient2d` from `geometry-predicates` and classifies the determinant
via `Sign::from_f64`.

`orient2d` is the foundational 2D predicate. With it in place, the
`Sign`-returning predicate pair is symmetric (orient2d + orient3d),
unblocking the 2D refinement work the Cherchi 2022 §4 coplanar handler
will need (segment-segment-2d, point-in-triangle-2d, segment-triangle-2d).

This PR pivots from the originally-planned `orient3D_LPI` (Cherchi's
indirect predicates), which is blocked on the upstream
`Indirect_Predicates` repo being LGPL-2.1, incompatible with the MIT
licensing posture of PR-CR1–CR9. PR-CR10 banks the orient2d completion
while a LGPL-boundary decision is pending.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `a, b, c` | `Point2` | Three points in the plane |

Returns: `Sign` —
- `Sign::Positive` — `(a, b, c)` in CCW order (c is LEFT of a→b)
- `Sign::Negative` — `(a, b, c)` in CW order (c is RIGHT of a→b)
- `Sign::Zero` — `a, b, c` collinear

All inputs must be finite. NaN / infinite inputs are undefined.

## Branch table

| Input class | Output |
|-------------|--------|
| `(a, b, c)` in CCW order | `Sign::Positive` |
| `(a, b, c)` in CW order | `Sign::Negative` |
| `a, b, c` collinear (degenerate triangle) | `Sign::Zero` |
| `a == b` (degenerate line) | `Sign::Zero` |
| `a == b == c` (all coincident) | `Sign::Zero` |
| NaN / infinite inputs | Undefined |

Note: `orient2d` uses the natural geometric convention. CCW → Positive,
matching the standard math-book reading of the determinant of the matrix
formed by the vectors `b - a` and `c - a`. This is **opposite** to
`orient3d`, which uses Shewchuk's "d below the CCW plane → Positive"
convention. Both are correct per their respective Shewchuk source.

## Invariants

1. **Antisymmetry under single swap**:
   `orient2d(b, a, c) == flip(orient2d(a, b, c))`. Similarly for any
   single argument swap.
2. **Symmetry under double swap**: composing two single swaps preserves
   the sign.
3. **Determinism**: same input → same output across runs and platforms
   (inherits from `geometry_predicates::orient2d` adaptive precision).

## Oracles

1. **Canonical CCW (Positive)**: standard unit triangle
   `(0,0)→(1,0)→(0,1)`
2. **Canonical CW (Negative)**: the CCW triangle with last two args
   reversed `(0,0)→(0,1)→(1,0)`
3. **Canonical Collinear (Zero)** — x-axis: `(0,0)→(1,0)→(2,0)`
4. **Canonical Collinear (Zero)** — y=x diagonal: `(0,0)→(1,1)→(2,2)`
5. **Antisymmetry**: swap(a,b) flips; swap(b,c) flips; swap(a,c) flips;
   double-swap preserves
6. **Determinism**: 100 invocations identical
7. **Degenerate**: `a == b` returns `Sign::Zero` (degenerate line; not
   an error)

## Failure modes

- **NaN / infinite input**: undefined behavior. Caller's responsibility.
- **All-coincident input**: returns `Sign::Zero` (degenerate; not an
  error).
- No `Result` return — totality via `Sign::from_f64` fallthrough.

## Research basis

- **Shewchuk 1997 §2.1** — `orient2d` foundational adaptive predicate
- **Cherchi 2020 §3** — downstream consumer uses same sign convention
- Wraps `geometry_predicates::orient2d` (MIT-licensed Shewchuk port)

## Method

1. Call `geometry_predicates::orient2d(a.as_array(), b.as_array(),
   c.as_array())` to get the adaptive determinant.
2. Classify via `Sign::from_f64`.

Implementation is 4 lines of wrapping. The work is in the spec, the
type-shape (`Point2` newtype in `cad-primitives`), and the test design.

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/orient.rs` header — extended from
PR-CR6 to cover both `orient2d` and `orient3d`:

```rust
//! Foundational `Sign` enum + `Point2`/`Point3`-typed orientation
//! predicates.
//!
//! `orient2d` and `orient3d` both wrap Shewchuk's adaptive predicates
//! from the `geometry-predicates` crate (MIT — Shewchuk port) and
//! convert their f64 results into a 3-valued `Sign`.
//!
//! Shewchuk 1997 §2.1 (adaptive orient2d, orient3d).
//! Cherchi 2020 §3 (predicates as foundation for arrangement).
//!
//! **Sign convention notes:**
//! - `orient2d` uses the natural geometric convention: CCW → Positive,
//!   CW → Negative, collinear → Zero.
//! - `orient3d` uses Shewchuk's convention: `d` BELOW the plane through
//!   CCW `(a,b,c)` → Positive (counter-intuitive — "below" not "above").
//!   Cherchi 2020 and downstream consumers expect this. See
//!   `specs/cherchi_rs_orient3d_sign.md`.
//!
//! No deviation from upstream behavior — both are type-shape wrappers.
```

(Plain attribution — no `Deliberate deviation`. Both wrappers preserve
upstream semantics. The asymmetric sign conventions are upstream
properties, not deviations.)

## Scope discipline

This PR introduces:
- One `Point2` newtype in `cad-primitives` (storage + accessors + `From`)
- One `orient2d` function in `cherchi-rs::predicates::orient`

Does NOT add:
- `Vector2`, `Mat2`, or any other 2D type beyond `Point2`
- Refactor of PR-CR1 / PR-CR5 internal 2D projection callers (banked)
- Indirect predicates (orient3D_LPI / orient3D_TPI — LGPL blocker)
- 2D refinement consumers (segment-segment-2d, point-in-triangle-2d,
  segment-triangle-2d — each its own future PR)
- `orient2d_inexact` / non-adaptive performance variant

## Verification

```bash
# RED phase
cargo test -p cad-primitives
# expect: 6 pass (3 Point3 + 3 Point2)

cargo test -p cherchi-rs predicates::orient
# expect: orient2d tests FAIL (unimplemented!()), orient3d + Sign tests pass

# GREEN phase
cargo test -p cherchi-rs
# expect: 133 pass (123 PR-CR1-CR9 + 10 PR-CR10)

cargo check --workspace
# expect: clean

cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```
