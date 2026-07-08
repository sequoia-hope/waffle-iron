# Cut that consumes the entire target body

Bug-fix/feature cycle per FIP §8 (ERROR-census campaign 3: R0023 / R0027 /
R0058 / R0088 — all `EmptyBooleanResult`).

## 1. Goal

A Cut (or Intersect) whose boolean legitimately produces NO material — the
tool engulfs the whole target (R0023: square cut 327.8 across × depth 770
over a cylinder r=150.4 × h=213.7, concentric same-plane) — must CONSUME the
target body (zero output bodies, a warning) instead of surfacing an engine
ERROR. This is standard CAD semantics: removing all material deletes the
body; the feature itself succeeded.

Measured: yang classifies every input-A triangle `inside_B = true`
(geometrically correct), the output B-Rep carries zero faces, kernel-v2
loudly returns its typed `EmptyBooleanResult` (kernel-v2 has no empty solid —
correct at that layer), and the engine records an operation ERROR (wrong
layer for the policy).

## 2. Parameters

None new. Applies to the existing `CombineMode::Cut | Intersect` dispatch.

## 3. Branch table

| # | Boolean kind | Kernel result | Behavior |
|---|---|---|---|
| 1 | Subtract (Cut) | non-empty | UNCHANGED (result bodies emitted) |
| 2 | Subtract (Cut) | `BooleanEmptyResult` | target consumed: `OpResult` with ZERO outputs + warning "cut consumed the entire target body" |
| 3 | Intersect | `BooleanEmptyResult` | same as 2 (warning: "intersect produced no material") — the shared engine arm stays branch-free (Constitution §7) |
| 4 | Union | `BooleanEmptyResult` | UNCHANGED loud error (a union of non-empty operands cannot be empty — a kernel defect must stay loud) |
| 5 | any | any other error | UNCHANGED (propagates) |

## 4. Invariants

- I1: a consumed target stays consumed (the engine's existing
  `already_consumed` body-lifetime tracking); later features neither resolve
  nor auto-union with it.
- I2: the warning reaches `engine_warnings` (via `OpResult.diagnostics.
  warnings`, the existing auto-union warning channel).
- I3: subsequent features continue to rebuild — a boss after a
  consumed-body cut creates its own body; the model's final body count
  reflects only live bodies.
- I4: kernel-v2 keeps its typed `EmptyBooleanResult` (no empty solids in the
  kernel); the policy lives above the kernel contract.

## 5. Oracles

- Unit (modeling-ops): `execute_boolean(Subtract)` on an engulfed pair
  returns `Ok` with zero outputs + the warning; `Union` on a kernel
  empty-result still errors (mutation-inverting the kind gate must fail).
- Engine (test-harness RED → GREEN): a two-extrude fixture (small boss,
  engulfing cut) rebuilds with NO engine errors, ONE warning, ZERO live
  bodies; adding a third boss yields exactly one live body.
- Corpus trackers: R0023 / R0027 / R0058 / R0088 replays must not carry
  `EmptyBooleanResult` in their failure sets.

## 6. Failure modes

- Union-empty stays `BooleanFailed` (branch 4).
- Cut with no resolvable target keeps its existing `ResolutionFailed`
  pre-flight error (unchanged).

## 7. Research basis

No published algorithm applies — this is body-lifetime policy, matching
mainstream CAD behavior (a feature that removes all material deletes the
body and reports a status, e.g. Parasolid's empty-body result codes [#36]).
The kernel/engine split follows A6.2 (structured, typed errors across the
boundary): a new `KernelError::BooleanEmptyResult` variant replaces the
stringly `BooleanFailed("… EmptyBooleanResult")` mapping so the engine can
discriminate without string matching.

### Method (7a)

Not an SSI operation; no surface-pair analysis. The boolean itself already
ran to a (correct, empty) conclusion.
