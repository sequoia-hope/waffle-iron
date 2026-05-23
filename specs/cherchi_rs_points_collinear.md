# `cherchi-rs::predicates::points_are_collinear_3d` — Spike PR-CR1

## Goal

Determine whether three 3D points are exactly collinear, using Shewchuk's
exact `orient2d` to avoid f64 round-off false negatives. Returns `bool` —
`true` iff the three points lie on a single line, with no tolerance.

This is the first ported function in `crates/cherchi-rs/` — a "spike" whose
secondary purpose is to establish the project's conventions for porting
discipline, license attribution, test layout, and reference-parity oracle.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `a` | `Point3` | First point |
| `b` | `Point3` | Second point |
| `c` | `Point3` | Third point |

Returns: `bool`.

All inputs must be finite. NaN inputs produce undefined behavior — the
caller is responsible for filtering. (This matches Shewchuk's `orient2d`
contract; we do not add input validation here.)

## Branch table

Single branch — no modes. Output is `bool`.

| Input class | Output |
|-------------|--------|
| All three points distinct, collinear | `true` |
| All three points distinct, non-collinear | `false` |
| Two coincident, third distinct | `true` (degenerate-collinear) |
| All three coincident | `true` (degenerate-collinear) |

## Invariants

1. **Exactness**: returns `true` iff Shewchuk's `orient2d` returns exactly
   `0.0` for all three orthogonal-axis-drop projections of the input
   (drop X, drop Y, drop Z planes).
2. **Order invariance**: any permutation of `(a, b, c)` produces the same
   result. (Shewchuk's `orient2d` is anti-symmetric under argument swap —
   sign flips — but `== 0` is invariant.)
3. **Determinism**: same inputs produce same output across runs and
   platforms (inherits from Shewchuk's exact arithmetic).
4. **No tolerance**: this is an EXACT predicate. Inputs that are
   mathematically collinear but inexact in f64 representation may or may
   not be reported as collinear depending on the inputs' exact f64 bit
   patterns. The contract is "exact orient2d returns 0," not "geometrically
   close to collinear."

## Oracles

1. **Canonical truth values** (the primary oracle):
   - Known-collinear inputs (axis-aligned, off-axis, coincident-pair,
     all-coincident) — must return `true`
   - Known-non-collinear inputs (right triangle, skew tetrahedron vertex)
     — must return `false`
   - Near-collinear-but-not-exact (e.g., `(2, 1e-300, 0)` on the
     `(0,0,0)-(1,0,0)` ray) — must return `false`
2. **Property: order invariance** — all 6 permutations of any canonical
   input return the same bool
3. **Property: A-01 regression** — for inputs where the legacy port's f64
   cross-product impl returned the wrong answer (from
   `docs/audits/cherchi_port_audit.md:148-181`), our impl must agree
   with exact `orient2d`. This documents the correctness improvement
   over the legacy port.
4. **Cross-check with `geometry-predicates`** — our impl is a 3-projection
   composition of `geometry_predicates::orient2d`. If `geometry-predicates`
   is correct (it's been on crates.io for years; itself a Shewchuk port),
   our composition is correct iff projection logic matches the spec.

## Failure modes

- **NaN input**: undefined. Shewchuk's `orient2d` is unspecified on NaN;
  we do not check. This is a low-level predicate; the caller's job is
  input hygiene.
- **Infinite input**: same as NaN — undefined.
- **No error return**: no `Result<>` — `bool` is the only output. This
  predicate cannot "fail" except via undefined-input contract violation.

## Research basis

- **Cherchi et al. 2020**, "Fast and Robust Mesh Arrangements using
  Floating-point Arithmetic" — §3 (robustness via cascaded
  filtered/exact predicates)
- **Shewchuk 1997**, "Adaptive Precision Floating-Point Arithmetic and
  Fast Robust Geometric Predicates" — §4.5 (adaptive `orient2d`)
- **cinolib** — `points_are_colinear_3d` reference impl (MIT-licensed),
  the function this Rust port mirrors

## Method

**Exact** (per CLAUDE.md §"Kernel Rewrite In Progress" and Constitution
P8 corollary on analytical primacy).

Implementation pattern:

```rust
let drop_z = orient2d([a.x, a.y], [b.x, b.y], [c.x, c.y]);
let drop_y = orient2d([a.x, a.z], [b.x, b.z], [c.x, c.z]);
let drop_x = orient2d([a.y, a.z], [b.y, b.z], [c.y, c.z]);
drop_z == 0.0 && drop_y == 0.0 && drop_x == 0.0
```

No tolerance parameter; equality with 0 is exact. No `unsafe`; no panics.

## Per-file MIT attribution

The implementation file `crates/cherchi-rs/src/predicates/collinearity.rs`
opens with the per-file MIT attribution header (template for all
subsequent cherchi-rs ports):

```rust
//! 3D collinearity test using Shewchuk's exact `orient2d` on three
//! orthogonal projections.
//!
//! Ported from cinolib's `points_are_colinear_3d` (used by Cherchi 2020
//! `processing.cpp:144`). cinolib is MIT-licensed.
//! © Marco Livesu et al. — https://github.com/mlivesu/cinolib
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §3 (cascaded filtered/exact predicates).
//! Shewchuk 1997 §4.5 (adaptive orient2d).
```

The pattern: brief description, "Ported from <upstream-file>", explicit
license + copyright, link to upstream and to `LICENSE-THIRD-PARTY.md`,
then research citations.

## Scope discipline

This spike ports ONE function. Not "one function and some adjacent
helpers." Not "the predicates module with several functions." Just one.

If during implementation a question arises like "should we also port
`points_are_coincident_3d`?" — the answer is **no, separate PR**.

## Verification

```bash
# RED phase (after Test Author commit, before Implementer)
cargo test -p cherchi-rs predicates::collinearity
# expect: ALL tests fail (function returns unimplemented!())

# GREEN phase (after Implementer commit)
cargo test -p cherchi-rs
# expect: ALL tests pass

# Workspace check
cargo check --workspace
# expect: clean (legacy kernel unaffected)

# Legacy regression
cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 (unchanged — this spike doesn't touch legacy kernel)
```
