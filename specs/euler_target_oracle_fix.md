# Spec: Fix Euler Target Oracle Predictions

## Goal

Correct the assay oracle's Euler characteristic (χ) predictions to reduce
false failures. Currently ~15 cases fail solely because `euler_target` is
wrong — the kernel produces correct geometry but the oracle rejects it.

## Problem

The `compute_euler_target()` function in `gen.rs` and several hardcoded
`euler_target` values produce incorrect predictions:

1. **Through-hole over-prediction** (`compute_euler_target`): predicts genus=1
   (χ=0) when `cut_depth ≥ boss_depth`, but on multi-plane cases the cut
   may not actually penetrate the boss (different sketch planes). Result:
   oracle expects χ=0 but mesh correctly has χ=2.

2. **Hardcoded euler_target=2 for cyl-minus-box** (F0036-F0040, F0054):
   subtracting a rectangle from a cylinder creates a through-slot (genus=1,
   χ=0), but the oracle hardcodes χ=2.

3. **Generator AABB frame mismatch** (F0011-F0015): `extrude_rect_aabb()`
   uses a different local frame algorithm than the kernel's
   `tangent_x_from_normal()`, causing false disjointness predictions.

## Parameters

None (oracle/generator fix, not a feature).

## Invariant

- A case with cuts that DON'T create through-holes should have euler_target=2.
- A case with a single through-hole should have euler_target=0.
- Cases with disjoint geometry should have `expect_rebuild_error=true`.
- The generator's AABB prediction must match the kernel's actual frame.

## Oracle

Assay pass count improvement. Target: ≥13 additional passes.

## Failure Modes

- Over-conservative prediction (genus=0 always) would miss real through-hole
  defects. The fix is targeted: only suppress genus prediction for multi-plane
  cases where the cut/boss planes differ.

## Research Basis

Standard topology: χ = 2 - 2g for a closed orientable surface of genus g.
Ref #33 Stroud Ch.4: Euler operators and topological invariants.
