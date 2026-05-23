# `cherchi-rs::processing::multiply_coordinates` — Spike PR-CR3

## Goal

Multiply every element of a coordinate array by a scalar multiplier, in
place. Used by Cherchi 2020 preprocessing to apply the scaling factor
produced by `compute_multiplier` (PR-CR2), completing the scale-up phase
of Cherchi's exact-arithmetic pipeline.

Typical usage:

```rust
let m = compute_multiplier(&coords);
multiply_coordinates(&mut coords, m);
// coords are now in f64-mantissa-exact integer range
```

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `coords` | `&mut [f64]` | Flat coordinate array, modified in place |
| `multiplier` | `f64` | Scalar multiplier; typically `compute_multiplier(coords)`'s output but any finite f64 is accepted |

Returns: `()` (in-place mutation).

All inputs must be finite. NaN / infinite inputs propagate per IEEE 754
multiplication semantics; not validated.

## Branch table

Single branch — no modes.

| Input class | Effect |
|-------------|--------|
| Empty slice | no-op |
| Multiplier = 1.0 | bitwise identity (each `c * 1.0 == c` exactly in IEEE 754) |
| Multiplier is any other finite f64 | each `coord` replaced by `coord * multiplier` |
| Any input is NaN / infinite | undefined / propagated per IEEE 754 |

## Invariants

1. **Pointwise**: after the call, `coords[i] == original_coords[i] * multiplier`
   for all `i` (exact f64 multiplication; result depends on f64 rounding mode).
2. **Length preserved**: `coords.len()` unchanged.
3. **Order preserved**: no permutation; coords mutated in index order.
4. **Identity (multiplier = 1.0)**: bitwise unchanged. Rust f64 guarantees
   `x * 1.0 == x` for all finite `x` (including signed zero, subnormals).
5. **Determinism**: same input → same output, byte-identical across
   runs and platforms (pure f64 multiplication, no thread state).

## Oracles

1. **Canonical truth values** (hand-computed):
   - Empty slice × any → no-op
   - `[1.0, 2.0, 3.0]` × `2.0` → `[2.0, 4.0, 6.0]`
   - `[1.0, 2.0, 3.0]` × `1.0` → `[1.0, 2.0, 3.0]` (identity)
   - `[1.0, 2.0, 3.0]` × `0.0` → `[0.0, 0.0, 0.0]`
   - `[-1.0, 2.0]` × `3.0` → `[-3.0, 6.0]` (negative coords)
   - `[1.0, 2.0]` × `-1.0` → `[-1.0, -2.0]` (negative multiplier)

2. **Property: identity** — for any non-empty slice, `multiply_coordinates(&mut coords, 1.0)`
   leaves `coords` bitwise unchanged.

3. **Property: power-of-2 round-trip** — for any slice and any
   non-zero power-of-2 multiplier `m`, applying then applying `1.0/m`
   recovers the original slice exactly (powers of 2 preserve f64 mantissa).

4. **Integration with `compute_multiplier`** — for `coords = [1e10, 1.0, 0.5]`:
   - `m = compute_multiplier(&coords)` returns `2^34` (per PR-CR2 spec)
   - After `multiply_coordinates(&mut coords, m)`, `coords.iter().map(|c| c.abs()).fold(0.0_f64, f64::max) >= 2.0_f64.powi(33)`
     — the max abs coordinate has been pushed past `2^33`, validating the
     pair's documented scale-up intent (bring max coord into
     f64-mantissa-exact integer range)
   - For `coords = [1.0]`: `m = 1.0`, slice bitwise unchanged

5. **Length / order preservation** — for a 100-element slice with mixed
   values and an arbitrary multiplier, `coords.len()` is unchanged and
   element ordering is preserved.

## Failure modes

- **NaN / infinite input**: propagated per IEEE 754 (no validation).
- **No error return**: function returns `()`; cannot fail except via
  undefined-input contract violation.

## Research basis

- **Cherchi et al. 2020**, "Fast and Robust Mesh Arrangements using
  Floating-point Arithmetic" — §3 (preprocessing for exact predicates).
  This function is the trivial scale-application step in the paper's
  preprocessing strategy, paired with `compute_multiplier`.

## Method

Plain f64 multiplication in a loop. No deviation from upstream behavior.
No exact-arithmetic backend; no `dashu`; no `unsafe`.

## Per-file MIT attribution

The file `crates/cherchi-rs/src/processing/multiplier.rs` already has
a file-level attribution header (added in PR-CR2 for `compute_multiplier`)
covering the file's MIT origin. The header explicitly attributes
Cherchi 2020 / cinolib at the file granularity.

PR-CR3 adds a new function to that file from the SAME upstream project.
Per the established convention (extended for PR-CR3):

> **Co-located function attribution pattern**: when a file's header
> already attributes its upstream project, additional functions in
> that file ported from the same upstream get a brief function-level
> doc-comment citing their specific upstream origin and referring back
> to the file header for the full license attribution. The file header
> is NOT duplicated.

Applied here:

```rust
/// Multiply each element of `coords` by `multiplier`, in place.
///
/// Pair-mate of [`compute_multiplier`]: typical usage is
/// `multiply_coordinates(&mut coords, compute_multiplier(&coords))`
/// to scale up to f64-mantissa-exact integer range.
///
/// Ported from Cherchi 2020's `multiply_coordinates` (`processing.cpp`).
/// MIT-licensed; see file header for full attribution.
pub fn multiply_coordinates(coords: &mut [f64], multiplier: f64) { ... }
```

This convention reduces redundant license boilerplate while preserving
auditability — every ported symbol still names its upstream origin.

## Scope discipline

ONE function. Not `compute_approximate_coordinates` "while we're here"
(that's PR-CR4 — Cherchi's inverse / scale-down operation, also flagged
in audit A-06 which has its own discipline question).

## Verification

```bash
# RED phase
cargo test -p cherchi-rs multiply_coordinates
# expect: all multiply_coordinates tests FAIL (function unimplemented!())

# GREEN phase
cargo test -p cherchi-rs
# expect: 27 (PR-CR1+CR2) + ~10 (PR-CR3) = ~37 pass

# Workspace check
cargo check --workspace
# expect: clean

# Legacy regression
cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 unchanged
```
