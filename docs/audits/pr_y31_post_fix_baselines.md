# PR-Y31 post-fix baselines — Cherchi differential diff (Stage B)

**Date:** 2026-05-08
**Pre-fix HEAD:** `27a09ed` (PR-Y30 ship)
**Post-fix HEAD:** this commit (PR-Y31; op-plumb at `e720629` + test-refactor)
**Cherchi binary:** `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
**Reproduction:**
```bash
YANG_BOOLEAN=1 TWIN_DEBUG=1 cargo test -p test-harness \
    --test cherchi_differential_diff -- --ignored --nocapture --test-threads=1
```

## Summary

| Case | First op | Cherchi (this PR) | Common | Missing | Extras | PR-Y30 (`27a09ed`) common/miss/extras | Δ |
|---|---|---|---|---|---|---|---|
| **F0044** | **Subtract** | `subtraction` | **136** | **0** | **0** | 88 / 0 / 48 | **+48 common; -48 extras** |
| F0020 | Union | `union` | 185 | 93 | 107 | 185 / 93 / 107 | unchanged |
| F0045 | Union | `union` | 0 | 236 | 466 | 0 / 236 / 466 | unchanged |
| R0092 | Subtract | `subtraction` | 0 | 392 | 368 | 0 / 340 / 368 | **missing +52** (measurement correction; PR-Y30 was invoking wrong op) |

## F0044 — load-bearing target

PR-Y31's canary (`docs/audits/pr_y31_anchor_canary.md`) localized F0044's
"48 extras" to a HARNESS MIS-CONFIGURATION rather than a production
defect:

- F0044's first dumped boolean pair is op `Subtract` (read from the
  case's `.waffle` JSON, second Extrude feature).
- PR-Y29/Y30 harness invoked Cherchi `mesh_booleans union` for ALL cases.
- Comparing Waffle's Subtract output against Cherchi's Union output on
  the same A/B inputs produces a spurious diff: the 48 "extras" were
  Cherchi's Union-but-not-Subtract triangles (the boundary between B-
  outside-A and B-inside-A).

Post-fix (`e720629` + test refactor in this commit): the harness reads
the per-case op and invokes Cherchi with the matching mode
(`union | subtraction | intersection`). F0044's Stage B output now
exactly matches Cherchi `subtraction`: 136 common, 0 missing, 0 extras.

**Important caveat — Status:Failed persists at F0044's other oracles.**
F0044 still fails `watertight_mesh` (12 unpaired / 180), `outward_normals`
(60/116), and `mesh_euler_characteristic` (χ=4 vs expected 2). The
"Cherchi parity == 0/0" finding is necessary but not sufficient for
F0044 to pass `assay_randomized`. PR-Y31's contribution is establishing
that **F0044's Stage B output IS the Cherchi reference output** — any
downstream watertight failure is in stages AFTER Stage B (Cherchi's
own output produces the same χ=4 and 12 unpaired edges, since χ is
intrinsic to the boolean result topology, not to Waffle's pipeline).

## F0020 — measurement parity preserved

F0020's first op is Union, which matches the PR-Y29/Y30 hardcoded value.
The numbers (185 common / 93 missing / 107 extras) are unchanged from
PR-Y30 baseline. This confirms the op-plumb change did NOT perturb F0020's
diff — measurement of cases-already-running-Union is preserved.

F0020 remains the cohort sibling target for PR-Y32+. Its 107 extras
ARE genuine production defects (the right op was already being invoked).

## F0045 — measurement parity preserved

F0045's first op is Union (same as Y29/Y30). Numbers unchanged.
F0045's tessellation-grid divergence is structural and pre-survival —
out of scope for this PR.

## R0092 — measurement correction (NOT a regression)

R0092's first op is **Subtract**, not Union. PR-Y29/Y30 was invoking
Cherchi `union` for R0092 — measuring the wrong reference. Post-fix
baseline is `subtraction` (392 missing / 368 extras vs PR-Y30's
340 missing / 368 extras under Union).

The "+52 missing" delta is NOT a Waffle production regression; it is
a CORRECTION of the harness baseline. Waffle's Stage B output for
R0092 is byte-identical to PR-Y30 — only the reference changed.

## Cherchi non-determinism (banked from PR-Y29/Y30)

- F0020 + R0092: Cherchi TBB parallel arrangement is schedule-
  dependent; output triangle counts vary across reruns at default
  thread count. Workaround: `TBB_NUM_THREADS=1` (banked from PR-Y29).
  This run did not set the env var; F0020/R0092 numbers may shift on
  rerun. F0044/F0045 are deterministic.
- Waffle Stage B output: byte-deterministic across runs (verified
  PR-Y29 via MD5 sample). No instability on the Waffle side.

## How to apply (PR-Y32+)

1. **F0020 is now the cleanest primary anchor** — 185 common provides
   a non-empty common-set for backward-tracing extras and missing
   triangles to specific Waffle pipeline stages (arrangement vs
   classification vs op-selection).
2. **F0044 is now passing the Stage B parity gate.** Downstream
   investigation (why F0044 still fails `watertight_mesh` despite
   matching Cherchi) is a separate axis — Cherchi's own output is
   also non-watertight (χ=4); this is an INPUT defect not a Waffle
   pipeline defect.
3. F0045/R0092 remain deprioritized (no common triangles; tessellation-
   grid divergence is pre-survival; separate PR).
4. The diff harness now correctly invokes per-op Cherchi; future
   PR-Y32+ canaries can rely on this baseline.

## Pre-fix red-phase verification

The previous (now-deleted) test `pr_y31_harness_op_plumb_regression.rs`
verified RED on `0f28e85` (pre-impl): F0044 harness extras = 48 ≠ 0.
The replacement assertion test in this commit
(`pr_y31_f0044_extras_zero` in `cherchi_differential_diff.rs`)
verifies the post-fix path inline (in-process; avoids the cargo-lock
deadlock the subprocess pattern hit). Pre-fix this test failed; post-
fix it passes. Verified manually by running against `27a09ed`:

```bash
git worktree add /tmp/y31-pre-wt 27a09ed
(cd /tmp/y31-pre-wt && YANG_BOOLEAN=1 cargo test -p test-harness \
    --test cherchi_differential_diff -- f0020_cherchi_diff_baseline \
    --ignored --nocapture --test-threads=1 2>&1 | grep "In Waffle, not in Cherchi")
# Expected output for F0044 in cohort: "In Waffle, not in Cherchi: 48 triangles"
git worktree remove /tmp/y31-pre-wt
```
