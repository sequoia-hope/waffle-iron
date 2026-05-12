# PR-Y32 ABORT — canary phase: L1 (arrangement) 93/93 dominant; fix-shape too coarse to commit in PR-Y32 budget

**Date:** 2026-05-12
**Final HEAD:** this commit (post-abort)
**Pre-PR baseline:** `723480c` (PR-Y31 ship)
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md`
**Canary memo (full evidence):** `docs/audits/pr_y32_anchor_canary.md` (cherry-picked from canary-y32 worktree at commit `2aab483`)

## Abort summary

PR-Y32's canary completed Phase 0a with a strong empirical finding AND a refusal to commit to a fix shape. Both decisions are correct per plan §"Phase 0a — Step 5 Acceptance gate" + `feedback_phase1_diagnosis_ranking_is_inference.md` + `feedback_adversary_recommendations_need_canary.md`.

**Empirical finding (3-of-3 probe layers strict):**
- L1 arrangement-absent: **93/93** missing triangles
- L2 mis-classified: 0/93
- L3 op-dropped: 0/93

All 93 of F0020's missing-from-Cherchi triangles are absent from Waffle's Stage A post-arrangement output (`subdivide_mesh_pair_full_cherchi`, `crates/kernel/src/boolean/exact_mesh.rs:2391-2541`). The defect lives in Waffle's Cherchi-Rust port (`crates/kernel/src/boolean/cherchi/*`) which has structurally diverged from the Cherchi 2022 C++ reference's arrangement.

**Rosetta-stone evidence:** F0044 (PR-Y31 verified Cherchi-equivalent at Stage B with 0 missing / 0 extras) has Waffle STAGE6=136 byte-matching Cherchi-C++ subtraction=136. When the Cherchi-Rust port matches C++, missing-count is 0. The defect IS in the port, definitively.

**Spatial structure:** 93 missing triangles form 3 connected components (sizes 47/44/2). Mirrors PR-Y26's 3-component unpaired-edge finding — fixing L1 should close PR-Y26's watertight defect (the empirical chain PR-Y28 §2.2 left open).

## Why ABORT instead of commit-to-fix

Plan §0a Step 5 requires:
> "Pick the SINGLE DOMINANT layer (largest N). If no clear dominant (≥40% of total), recommend ABORT with refined-scope-canary."

L1 is dominant at 100% — not the failure mode that triggers ABORT. The trigger here is that **the fix-shape recommendation requires sub-stage attribution the canary did not produce**:

- The Cherchi-Rust port spans 7 files in `crates/kernel/src/boolean/cherchi/*`, thousands of LOC across 6 stages (STAGE1-6)
- Plan §"LOC budget by layer" budgets L1 at ~50-150 LOC
- Repair of "STAGE3 segment soup" vs "STAGE4 intersection detection" vs "STAGE5 classification" vs "STAGE6 triangulation" each likely 50-200+ LOC alone; whole-port repair ~200-500+ LOC
- Choosing ONE sub-stage without per-stage byte-diff against Cherchi C++ reference is structural inference, which `feedback_phase1_diagnosis_ranking_is_inference.md` explicitly warns against
- After 4 canary-stage ABORTs (Y25-Y28) for exactly this reason, the discipline holds

The canary's call to refuse the fix-shape commit is the correct application of canary-first workflow.

## What ships with this ABORT

This is **NOT** a "wasted cycle" ABORT — three concrete artifacts ship to main:

1. **`docs/audits/pr_y32_anchor_canary.md`** — 421-line canary memo with verbatim probe output across L1/L2/L3, the 93-triangle position table, the F0044 rosetta-stone evidence, and the PR-Y33 refined-scope-canary recommendation. (Already cherry-picked at `2aab483`.)
2. **`Y32_DUMP_POSITIONS=1` harness gate** — 24-LOC env-gated `eprintln!` extension to `cherchi_differential_diff.rs::run_diff_for_case` that emits ALL missing/extras quantized positions (not just the existing top-10). The canary used this to feed the probe pipeline; PR-Y33's narrower canary will reuse it. Additive, dev-only, no behavioral change to existing tests.
3. **This abort memo.**

## Banked findings for PR-Y33+

1. **Refined PR-Y33 scope:** narrow canary that instruments Cherchi-Rust STAGE3/4/5/6 dumps and diffs per-stage against Cherchi C++ (either patch the existing `mesh_booleans` binary's verbose paths OR snapshot both at post-segment-insertion). Per-stage byte-diff localizes to ONE Cherchi-Rust sub-anchor; THEN a fix shape becomes well-anchored. Acceptance gate for PR-Y33's canary: ≥80% of size mismatch localizes to ONE sub-stage.

2. **The Cherchi-Rust port is the right repair target, not `face_survival_detect` / `label_cells`.** PR-Y22 through PR-Y31 have been investigating downstream of arrangement. With the L1=100% finding, the actual defect target is now upstream of where every prior PR has looked.

3. **F0020 missing-count → 0 prediction is conditional on FULL port repair.** A partial PR-Y33 fix that closes (say) STAGE6 triangulation divergence would leave the STAGE3-5 vertex-placement gap unaddressed. Expect multiple PRs (Y33, Y34, ...) to close the missing-count progressively.

4. **F0045/R0092 will likely cascade-improve.** Both have non-zero Stage B missing AND have Cherchi-Rust STAGE6 sizes diverging from C++. A port-side fix should benefit them too.

5. **F0044 stays at 0 even through port repair** because F0044's port output ALREADY matches C++ subtraction (136=136). F0044 is the test-time guardrail that the port fix doesn't regress already-matching cases.

## Strategic context

After 4 canary-stage ABORTs (Y25/Y26/Y27/Y28) on "no anchor at all," PR-Y31 (a HARNESS fix) re-pointed canaries with the diff harness, and now PR-Y32's canary produces THE CLEANEST EMPIRICAL ANCHOR IN 10 PR CYCLES: the defect IS at L1 in the Cherchi-Rust port, with byte-level co-location evidence (F0044 matches when port matches).

The cost of this ABORT is one canary cycle (~25 min, no production code touched). The benefit is that PR-Y33+ work on the Cherchi-Rust port now has empirical justification rather than structural-inference scoping.

The next PR (Y33) should be:
- A NARROWER CANARY first — per-stage byte-diff of Cherchi-Rust internal stages against Cherchi C++ on F0020 + F0044 (control)
- THEN, if dominant sub-stage identified, a fix in that sub-stage
- ELSE, a per-stage parity test harness (the next infrastructure PR, like Y29 was for diff)

## Cycle commits (2)

- `2aab483` audit: canary memo (cherry-picked from canary-y32 worktree commit `7bd4556`)
- this commit: probe gate + abort memo

## No production code touched

`git diff 723480c..HEAD --stat` for this ABORT:
```
crates/test-harness/tests/cherchi_differential_diff.rs  | 24 +++  (Y32_DUMP_POSITIONS hygiene)
docs/audits/pr_y32_anchor_canary.md                    | 421 +++ (canary memo)
docs/audits/pr_y32_abort.md                            | ~  (this file)
```

Zero lines under `crates/kernel/`, `crates/wasm-bridge/`, or `app/`. WASM rebuild not required.
