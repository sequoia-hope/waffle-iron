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

## 7. Addendum 2026-09-03 — I1 was violated by the most-recent-body walk (FIXED)

The exact-membership oracle (`docs/audits/exact_membership_sweep_2026_09_
03.md`, class C) found the consumed body coming BACK: R0034's gear revolve
(`merge: true`, `MostRecentLegacy`) auto-unioned with the box its cut had
just consumed (kernel volume +36 % over the exact chain, the engine warning
"cut consumed the entire target body" present all along); R0007's and
R0088's second cuts re-cut the consumed cylinder; R0058's third boss merged
with the consumed gear (+6.3 %). All four were SUPPORTED_CORRECT — the
categorized runner checks neither monotonicity nor the volume of a cut
chain.

Anchor: `find_most_recent_solid_outputs` / `find_most_recent_consumed`
(`rebuild.rs`) walk back to the first feature whose result carries outputs
and never consult `already_consumed`; a feature that consumed its target
carries NO outputs, so the walk steps past it to the consumed body's own
feature — which still holds the pre-cut handle — and returns it as the
"most recent" body. `resolve_share_a_face` did honour the set; the legacy
strategy, the corpus's default, did not. The through-all depth
(`resolve_depth`) measured the same resurrected body.

Fix: the set is threaded into both walks, `resolve_combine_targets` and
`resolve_depth`; a consumed feature is skipped like a suppressed one. After
a consumption the walk finds nothing: a boss creates its own body (I3), a
cut has no target and keeps its typed `ResolutionFailed` (§6) — the model
has no material to cut, and saying so is the honest answer.

Oracles: the §5 engine fixture now also asserts the follow-up boss's volume
is its OWN (0.5, not 0.5 + the resurrected 0.5 — one live body either way,
which is why I3's count alone never caught it). Kernel-v2 pins the empty
result itself for a body inside a planar and a curved tool
(`crates/kernel-v2/tests/containment_subtract.rs`: `EmptyBooleanResult`,
with the contained-tool cavity and the intersection as controls).
Post-fix exact-oracle readings: R0034 4.126e7 vs 4.152e7 exact (was
+36 %), R0058 −0.2 % (was +6.3 %), R0023 −1.1 %, R0007 a loud engine error
(the exact chain is empty). Corpus impact in the roadmap entry.
