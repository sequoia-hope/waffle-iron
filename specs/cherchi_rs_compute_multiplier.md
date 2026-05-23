# `cherchi-rs::processing::compute_multiplier` — Spike PR-CR2

## Goal

Compute a power-of-2 scaling factor that, when applied to the input
coordinate array, brings the maximum-magnitude coordinate into a range
where f64 mantissa precision covers integer values exactly. Used by
Cherchi 2020 preprocessing to enable exact integer arithmetic on
scaled-then-truncated f64 coords.

This is the second porting cycle in `crates/cherchi-rs/`. It also
establishes the project's policy for handling cases where the C++
upstream has documented bugs (UB or known-broken behavior) — see
§"Discipline question" below.

## Parameters

| Name | Type | Description |
|------|------|-------------|
| `coords` | `&[f64]` | Flat coordinate array (e.g., `[x0,y0,z0, x1,y1,z1, …]` for 3D points). Function is layout-agnostic — only the maximum absolute value matters. |

Returns: `f64` — a power-of-2 multiplier in the range `[1.0, 2.0^62]`.

All inputs must be finite. NaN / infinite inputs produce undefined
behavior (caller's responsibility).

## Branch table

| Input class | Output |
|-------------|--------|
| Empty slice | `1.0` |
| All-zero coords | `1.0` |
| All coords have `|c| < 1.0` | `1.0` (already in safe range; no upscaling) |
| Some coord has `|c| ≥ 1.0`, max `|c|` is `M` | `2^⌈log₂(M)⌉`, clamped to `2^62` |
| Any coord is NaN / infinite | undefined behavior |

## Invariants

1. **Power-of-2 output**: result is always a finite f64 that exactly
   equals some `2^k` for `k ∈ [0, 62]`.
2. **Scaling sufficiency** (when `max|c| ≥ 1.0`): result is the smallest
   power of 2 such that `max(|c|) * result ≤ 2^52 * result` would
   require, equivalently — the result equals `2^⌈log₂(max|c|)⌉`. This
   property is clamped: if `⌈log₂(max|c|)⌉ > 62`, the result is `2^62`.
3. **No-scale base case** (when `max|c| < 1.0`): result is `1.0`.
4. **Determinism**: same input → same output, byte-identical across
   runs and platforms (pure float arithmetic + bit shift; no hash
   iteration, no thread-local state).
5. **Sign independence**: result depends only on absolute values; sign
   of any coordinate is ignored.

## Oracles

1. **Canonical truth values** (the primary oracle):
   - Empty / all-zero: `1.0`
   - Sub-unit max: `[0.5]` → `1.0`; `[1.0]` → `1.0`
   - Small integer max: `[3.0]` → `4.0`; `[100.0]` → `128.0` (= `2^7`)
   - Negative coords (uses absolute value): `[-100.0, 50.0]` → `128.0`
   - **CAD-scale** (the `e ≥ 31` case): `[1e10]` → `2^34`
     (`log₂(1e10) ≈ 33.22`, `⌈⌉ = 34`)
   - Clamp boundary: `[2.0_f64.powi(62)]` → `2^62`
   - Clamp on overflow: `[2.0_f64.powi(70)]` → `2^62`
2. **Property: order independence** — `compute_multiplier(coords)` equals
   `compute_multiplier(reversed_coords)`
3. **A-05 deviation regression**: `compute_multiplier(&[1e10])` returns
   `2^34`, NOT `1.0`. Documents the strict-correct deviation from C++'s
   UB-induced fallback. (See §"Discipline question".)

## Failure modes

- **NaN / infinite input**: undefined. No validation; behavior follows
  from `f64::log2().ceil() as u32`'s behavior on non-finite inputs.
- **No error return**: `f64` is the only output. This function cannot
  fail except via undefined-input contract violation.

## Research basis

- **Cherchi et al. 2020**, "Fast and Robust Mesh Arrangements using
  Floating-point Arithmetic" — §3 (preprocessing for exact predicates).
  This function is an implementation detail of the paper's
  preprocessing strategy: scale up f64 coords so that truncation
  produces exact integers within mantissa precision.

## Method

**Strict-correct per Rust language semantics**. Pure float + bit-shift
arithmetic; no exact-arithmetic backend (`dashu`) needed.

**Deliberate deviation from C++ upstream**: where C++ has documented
UB (`int multiplier = 1 << e` for `e ≥ 31` is signed-int undefined
behavior — and Cherchi's C++ explicitly acknowledges this with
`if(multiplier < 0) multiplier = 1.0; // temporary fix`), we use
`(1u64 << e.min(62)) as f64` which is well-defined and matches the
function's stated intent. See §"Discipline question" below.

## Discipline question — C++ deviation policy

PR-CR2 establishes a precedent for handling cases where the C++
upstream has documented bugs.

### The specific case

The C++ `compute_multiplier` does, in effect:

```cpp
int e = ceil(log2(max_abs_coord));
int multiplier = 1 << e;        // UB if e >= 31
if (multiplier < 0) {
    multiplier = 1.0;           // C++ author's own "temporary fix" for UB
}
return multiplier;
```

For typical CAD inputs (e.g., 100 mm parts: `e ≈ 7`), this works. For
larger-range inputs (e.g., construction-scale: `e ≈ 34`), the `1 << e`
expression has signed-integer overflow → undefined behavior. The C++
author papers over with the negative-detection fallback, but in modern
optimizing compilers the UB may also miscompile away the check itself
(undefined behavior is undefined). The result on actual UB-triggering
inputs is unspecified.

### Two interpretations of "reference parity"

Per `crates/cherchi-rs/CLAUDE.md` rule 2: "Reference parity is the
correctness oracle. … If the diff has regressions, the port is wrong
(until proven otherwise on a case-by-case basis with paper-cited
justification)."

- **Strict reading**: match C++ byte-for-byte even on UB-triggering
  inputs. Replicate `multiplier = 1.0` for `e ≥ 31`. Deviation requires
  paper-cited justification + empirical evidence downstream isn't
  calibrated to the UB.
- **Pragmatic reading**: match C++'s STATED INTENT (the function's
  documented purpose as a power-of-2 scaling factor) but use Rust's
  well-defined semantics. Treat UB-induced behavior as a known
  upstream bug; document the deviation.

### Choice for PR-CR2: pragmatic reading

Rationale:

1. The C++ author's own comment `// temporary fix` explicitly admits
   the UB behavior is wrong. Treating it as our correctness oracle
   would be absurd.
2. Rust language has no idiomatic way to replicate signed-int shift
   UB (we'd have to write `unsafe`, which is itself prohibited per
   `cherchi-rs/CLAUDE.md` rule 4).
3. The strict-correct behavior is documented in this spec, tested,
   and captured as a deliberate deviation in the implementation file's
   header comment.

### Generalization (the new convention)

**"When upstream has documented UB or known-broken behavior, the
Rust port matches the function's stated intent and documents the
deviation."**

Specifically:

- The deviation must be flagged in the per-file MIT attribution header
  under a **"Deliberate deviation from upstream"** subsection (template
  in this PR's `multiplier.rs`)
- A test must explicitly assert the strict-correct behavior on an input
  that triggers the upstream's bug
- The spec must include a §"Discipline question" / §"Deliberate
  deviation" section explaining the choice
- When the C++ sidecar becomes available, a follow-up diff test
  verifies the deviation doesn't propagate as a regression through
  the larger pipeline

Memory entry banking this convention:
`cherchi_rs_cpp_deviation_policy.md`.

### Banked for follow-up

When the C++ sidecar predicate harness is built (per
`crates/cherchi-rs/docs/sidecar_predicate_harness.md`), capture C++'s
actual behavior on `[1e10]` as the empirical anchor:

- If C++ + downstream produces correct results with `multiplier = 1.0`
  (UB fallback), then downstream is robust — the deviation is purely
  defensive
- If C++ + downstream produces wrong results too, our deviation is
  STRICTLY BETTER — and we want to be sure downstream code is calibrated
  to expect well-defined `2^e`, not UB-induced `1.0`

Either way, we don't regress; we just verify pre-merge that the
deviation doesn't break downstream consumers when they get wired
through.

## Per-file MIT attribution

The implementation file `crates/cherchi-rs/src/processing/multiplier.rs`
opens with a header that extends PR-CR1's template with a **Deliberate
deviation from upstream** subsection:

```rust
//! Power-of-2 coordinate scaling factor for exact-arithmetic preprocessing.
//!
//! Ported from Cherchi 2020's `compute_multiplier` (`processing.cpp:47-64`).
//! Cherchi 2020 is MIT-licensed.
//! © 2020 Gianmarco Cherchi, Marco Livesu, Riccardo Scateni, Marco Attene
//! https://github.com/gcherchi/FastAndRobustMeshArrangements
//! See ../../LICENSE-THIRD-PARTY.md for full attribution.
//!
//! Cherchi 2020 §3 (preprocessing for exact predicates).
//!
//! **Deliberate deviation from upstream**: C++'s `1 << e` is signed-int
//! UB for `e ≥ 31` (typical CAD inputs trigger this); upstream papers
//! over with `if(multiplier < 0) multiplier = 1.0; // temporary fix`.
//! Our impl uses `(1u64 << e.min(62)) as f64` which is well-defined and
//! matches the function's stated intent (power-of-2 scaling factor). See
//! `docs/audits/cherchi_port_audit.md:228-241` (A-05) and
//! `specs/cherchi_rs_compute_multiplier.md` §"Discipline question".
```

## Scope discipline

One function. Not "this function plus `multiply_coordinates` since
they're paired." Not "the processing module with several helpers."
Just `compute_multiplier`.

If during implementation a question arises like "should we also port
`multiply_coordinates`?" — the answer is **no, separate PR (PR-CR3
or later)**.

## Verification

```bash
# RED phase (after Test Author commit)
cargo test -p cherchi-rs processing::multiplier
# expect: all tests fail (function returns unimplemented!())

# GREEN phase (after Implementer commit)
cargo test -p cherchi-rs
# expect: 13 (PR-CR1 collinearity) + ~12 (PR-CR2 multiplier) = ~25 pass

# Workspace check
cargo check --workspace
# expect: clean

# Legacy regression
cargo test -p kernel --lib 2>&1 | tail -3
# expect: 1250/34/43 (unchanged — this spike doesn't touch legacy kernel)
```
