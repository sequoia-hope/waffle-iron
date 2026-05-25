# `cherchi-rs::predicates::{Sign, orient3d}` — Spike PR-CR6

## Goal

Introduce the foundational `Sign { Negative, Zero, Positive }` enum
for predicate results, and the first predicate that returns it:
`orient3d(a, b, c, d) -> Sign`. The wrapper accepts `Point3` inputs
(matching cherchi-rs's API style) and converts
`geometry_predicates::orient3d`'s `f64` result to `Sign` via the
`Sign::from_f64` decoder.

This establishes the return-type convention for ALL future cherchi-rs
predicates (orient2d wrapper, indirect predicates' filtered+exact
paths, etc.). Subsequent ports use `Sign` instead of inventing
ad-hoc f64 sign-comparison logic.

## Parameters

### `Sign::from_f64`

| Name | Type | Description |
|------|------|-------------|
| `x` | `f64` | Numeric value whose sign to classify |

Returns: `Sign`.

### `orient3d`

| Name | Type | Description |
|------|------|-------------|
| `a, b, c, d` | `Point3` | Four points defining a tetrahedron |

Returns: `Sign`.

All inputs must be finite. NaN / infinite inputs are undefined.

## Branch table

### `Sign::from_f64`

| Input class | Output |
|---|---|
| `x > 0.0` | `Sign::Positive` |
| `x < 0.0` | `Sign::Negative` |
| `x == 0.0` (including -0.0) | `Sign::Zero` |
| `x.is_nan()` | `Sign::Zero` (totality fall-through) |
| `x == f64::INFINITY` | `Sign::Positive` (covered by `x > 0.0`) |
| `x == f64::NEG_INFINITY` | `Sign::Negative` (covered by `x < 0.0`) |

### `orient3d`

| Input class | Output |
|---|---|
| `d` above plane of `(a, b, c)` (positive determinant; CCW orientation) | `Sign::Positive` |
| `d` below plane | `Sign::Negative` |
| All 4 points coplanar | `Sign::Zero` |
| NaN / infinite inputs | Undefined (per `geometry-predicates`'s contract) |

## Invariants

1. **`Sign::from_f64` totality**: defined for every f64 value
   (including NaN, ±0.0, ±infinity). No panics.
2. **`orient3d` antisymmetry**: swapping any two of `(a, b, c, d)`
   flips the result's `Sign`. (Where flip: Positive↔Negative,
   Zero→Zero.)
3. **Determinism**: same inputs → same outputs across runs and
   platforms. `orient3d` inherits this from `geometry-predicates`
   (Shewchuk's adaptive precision is deterministic).
4. **No new heap allocations**: `Sign` is a 1-byte enum, `Point3` is
   24 bytes, `orient3d` calls into `geometry-predicates` which uses
   stack-only adaptive expansions. Allocation-free.

## Oracles

### `Sign::from_f64` truth table

- `Sign::from_f64(1.0)` → `Sign::Positive`
- `Sign::from_f64(-1.0)` → `Sign::Negative`
- `Sign::from_f64(0.0)` → `Sign::Zero`
- `Sign::from_f64(-0.0)` → `Sign::Zero` (IEEE 754: -0.0 == 0.0 in comparisons)
- `Sign::from_f64(f64::EPSILON)` → `Sign::Positive`
- `Sign::from_f64(f64::INFINITY)` → `Sign::Positive`
- `Sign::from_f64(f64::NEG_INFINITY)` → `Sign::Negative`
- `Sign::from_f64(f64::NAN)` → `Sign::Zero` (totality)

### `orient3d` canonical

- Standard CCW tetrahedron `(0,0,0), (1,0,0), (0,1,0), (0,0,1)` → `Positive`
- Same tetra with last two arguments swapped (CW) → `Negative`
- Four coplanar points (all `z = 0`) → `Zero`

### `orient3d` antisymmetry property

- Swapping `(a, b)` flips the Sign
- Swapping `(c, d)` flips the Sign
- Double swap `(a↔b, c↔d)` preserves the Sign (two flips cancel)

### Determinism

- 100 invocations on the same input yield identical results

## Failure modes

- NaN / infinite inputs to `orient3d` → behavior per `geometry-predicates`
  (typically returns NaN or 0); `Sign::from_f64` classifies NaN as `Zero`
- No error return; `Sign` is the only output.

## Research basis

- **Shewchuk 1997 §2.1** — `orient3d` as adaptive precision predicate
- **Cherchi 2020 §3** — predicates as the foundation for mesh
  arrangement; output-Sign-enum convention is implicit throughout

## Method

- `Sign::from_f64`: 3-way sign classification via `>`/`<` comparisons.
  NaN falls through to the `else` branch (which returns `Zero`).
- `orient3d`: thin wrapper around `geometry_predicates::orient3d`.
  Converts `Point3` → `[f64; 3]` via `as_array()`, then applies
  `Sign::from_f64` to the determinant result. No filtered+exact
  cascade — `geometry-predicates` is already adaptive (Shewchuk).

## Per-file MIT attribution

`crates/cherchi-rs/src/predicates/orient.rs` header (new file):

```rust
//! Foundational `Sign` enum + `Point3`-typed orientation predicates.
//!
//! `orient3d` wraps Shewchuk's adaptive predicate from the
//! `geometry-predicates` crate (MIT-licensed; itself a Shewchuk port)
//! and converts the f64 result to a 3-valued `Sign`.
//!
//! Shewchuk 1997 §2.1 (adaptive orient3d).
//! Cherchi 2020 §3 (predicates as foundation for arrangement).
//!
//! No deviation from upstream behavior — this is a type-shape wrapper.
```

(Plain attribution — no `Deliberate deviation` subsection. The wrapper
preserves upstream semantics exactly; only the return type changes
from `f64` to `Sign`.)

## Scope discipline

This PR introduces:
- The `Sign` enum (foundational type)
- One `Sign::from_f64` constructor
- One `orient3d` wrapper

It does NOT:
- Wrap `orient2d` (separate PR if/when needed)
- Refactor `points_are_collinear_3d` (PR-CR1) or `point_in_triangle_2d`
  (PR-CR5) to use `Sign` (separate PR — would broaden scope significantly)
- Add `Sign::flip()` or other helpers (YAGNI; add when a consumer needs them)
- Promote `Sign` to `cad-primitives` (wait for 2nd consumer)

## Verification

```bash
# RED phase (after Test Author commit)
cargo test -p cherchi-rs predicates::orient
# expect: all 15 new tests FAIL (unimplemented!())

# GREEN phase (after Implementer commit)
cargo test -p cherchi-rs
# expect: 69 (PR-CR1-CR5) + 15 (PR-CR6) = 84 pass

# Workspace check
cargo check --workspace
# expect: clean

# Legacy regression
cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 (unchanged — this spike doesn't touch legacy kernel)
```
