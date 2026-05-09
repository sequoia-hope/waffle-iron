# PR-Y24 Adversary Validation — ACCEPT

**Author:** adv-y24
**Date:** 2026-05-08
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0e
**Implementer commit:** `8b8297c` (impl-y24)
**Baseline commit:** `3c749a3`
**Verdict:** **ACCEPT**

All 7 spec §6 / brief gates pass. F0020 spotlight surfaces a *different* panic at the next-layer mesh-quality surface (per spec §7.2 expected outcome), the `(38,27)` validate_yang_result_topology panic is GONE, F0044 cohort guard preserved at 0/0, F0030+F0050 sibling status unchanged, yang_fast meets ≥10 threshold, and kernel baseline IMPROVED 1250→1254 / 29→25 / 42→42 with zero new failures.

The implementer's structural argument (I2: pairing logic L1219-1380 untouched) holds up empirically — `git diff` hunk headers confirm modifications fall ONLY at L1414, L1437, L1458, L1512 (all OUTSIDE the L1219-1380 zone). Citation hygiene is clean: PR-Y24's NEW comments cite Yang §3 and Cherchi §3 verbatim; ZERO new uses of the refuted "Yang §4.4.2 directional-symmetry" wording.

---

## §0 Discipline check — live tree untouched

### Live tree at start of session

```
$ cd /home/claude/workspace && git status
On branch main
Your branch is ahead of 'origin/main' by 5 commits.
  (use "git push" to publish your local commits)

nothing to commit, working tree clean
$ git rev-parse HEAD
8b8297cf6abb12ef8ab6d55038538d46d96bcb48
```

### Live tree at end of session (post-gates)

```
$ git status
On branch main
Your branch is ahead of 'origin/main' by 5 commits.
  (use "git push" to publish your local commits)

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
	modified:   app/tests/cases/assay/results.json
```

The single modification is a **known spotlight-test side effect** (per brief §0(8) note: "spotlight tests write back to app/tests/cases/assay/results.json; if that's modified, that's a known test side-effect, NOT a PR-Y24 regression. Document it but do NOT revert it from this role — team-lead handles cleanup."). I did NOT revert. Adversary memo (this file) is the sole intentional addition from this role.

### Worktree usage

Per `feedback_adversary_no_destructive_git.md`: **NO** `git stash`, `git checkout --`, `git reset --hard` was used on the live tree. Baseline comparison was via `git worktree add /tmp/y24-baseline-wt 3c749a3`, removed at end of role via `git worktree remove`.

```
$ git worktree add /tmp/y24-baseline-wt 3c749a3
Preparing worktree (detached HEAD 3c749a3)
HEAD is now at 3c749a3 feat(scripts): extract-papers.sh — idempotent text view of refs/*.pdf for agent paper-reading

# (gates run)

$ git worktree remove /tmp/y24-baseline-wt
$ git worktree list
/home/claude/workspace                8b8297c [main]
```

The two prunable `/tmp/auto-waffle-*` worktrees pre-existed; not touched.

---

## §1 Required reading

- **Yang 2025 §3 + §4.4.2** — `refs/text/yang2025_hybrid_boolean.txt:240-330` + `:574-605` — read.
- **Cherchi 2022 §3 + §5** — `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:232-290` + `:385-470` — read.
- **PR-Y24 plan** — `/home/claude/.claude/plans/optimized-wandering-wind.md` — read.
- **PR-Y24 spec** — `specs/yang_pr_y24_oracle_observation_layer.md` (commit `627eaa2`) — read.
- **PR-Y24 canary** — `docs/audits/pr_y24_anchor_canary.md` (commits `69c6c2b` + `957efdf`) — read.
- **PR-Y24 test file** — `crates/test-harness/tests/pr_y24_oracle_observation_layer_regression.rs` (commit `87e1c1d`) — read.
- **PR-Y24 implementation diff** — `git show 8b8297c` for `topology_extract.rs`, `yang_integration.rs`, `topology/arena.rs` — read.
- **PR-Y23 abort memo** — `docs/audits/pr_y23_abort.md` — read for cohort regression context.
- **Memory rules** — `feedback_adversary_recommendations_need_canary.md`, `feedback_adversary_no_destructive_git.md`, `feedback_local_fix_for_global_invariant.md`, `feedback_validate_against_corpus.md` — applied throughout.

---

## §2 Gate-by-gate verdicts

### Gate 1 — F0020 spotlight (load-bearing)

**Pre-PR (`3c749a3`):** Failed at `validate_yang_result_topology` `(38,27)` panic.
**Post-PR (`8b8297c`):** Failed at *different* panic surface — `watertight_mesh / consistent_normals / no_degenerate_triangles / no_self_intersection / mesh_euler_characteristic`. The `(38,27)` panic is **GONE**.

**Verbatim status line:**

```
=== F0020 Spotlight (PR-Y16-INV) ===
Description: 3 ops, scale=1.00e0, extrude(rectangle,boss)+extrude(rectangle,boss)+extrude(rectangle,boss) — Intersecting oblique (seed 8005)
Status:      Failed
Detail:      watertight_mesh: 36 unpaired edges out of 130 total (34 boundary, 2 non-manifold);
             consistent_normals: 2 of 76 triangles have reversed normals;
             no_degenerate_triangles: 4 of 76 triangles are degenerate;
             no_self_intersection: 10 inter-face triangle penetrations, face pairs: (0,3), (0,5), (1,3), (2,5), (3,13), ...;
             mesh_euler_characteristic: V(50) - E(130) + F(76) = -4 (expected 2)
```

**Diagnostic confirmation:** the `[yang-diag]` line `NMM half-edges: 39 of 169 total (33 faces, 65 edges, 80 vertices) — legitimate per Yang §4.4.2 directional-symmetry mandate` shows the topology-extract layer now reports NMM as legitimate (instead of crashing on the `(38,27)` validator panic). The failure has moved DOWNSTREAM to mesh-quality (banked Layer-4 per spec §7.2).

**Verdict:** **ACCEPT** — Per brief decision tree: "Gate 1 surfaces *different* panic at downstream layer → ACCEPT (per spec §7.2 next-layer outcome)." Mechanism CONFIRMED: PR-Y24's observation-layer fix flipped the `(38,27)` predicate as predicted.

Log: `/tmp/y24-adv-gate1.log` (exit 0).

### Gate 2 — F0030 sibling

**Pre-PR baseline (per brief):** Failed (12 unpaired/66; Euler V-E+F=3).
**Post-PR observed:** identical signature.

```
=== F0030 Spotlight (PR-Y16-FIX-ARCH cohort) ===
Status:      Failed
Detail:      watertight_mesh: 12 unpaired edges out of 66 total;
             outward_normals: only 36 of 40 triangles (90.0%) have outward normals (need 95%);
             mesh_euler_characteristic: V(29) - E(66) + F(40) = 3 (expected 2)
```

**Verdict:** **ACCEPT** — unchanged (12/66 + V-E+F=3 matches pre-PR baseline exactly).

Log: `/tmp/y24-adv-gate2.log` (exit 0).

### Gate 3 — F0050 sibling

**Pre-PR baseline (per brief):** Failed (39 unpaired/417 watertight).
**Post-PR observed:** identical signature.

```
=== F0050 Spotlight (PR-Y16-FIX-ARCH cohort, silent fail) ===
Status:      Failed
Detail:      watertight_mesh: 39 unpaired edges out of 417 total;
             consistent_normals: 162 of 265 triangles have reversed normals;
             ...
             mesh_euler_characteristic: V(258) - E(417) + F(265) = 106 (expected 2)
```

**Verdict:** **ACCEPT** — unchanged.

Log: `/tmp/y24-adv-gate3.log` (exit 0).

### Gate 4 — F0044 batch `[topo-extract] unpaired=0` (cohort guard structural per I2)

**Verbatim from PR-Y22 regression test:**

```
[pr-y22-test] F0044 batch max `[topo-extract] summary: unpaired=N`: Some(0)
   (pre-PR-Y22 baseline: 2; post-PR-Y22 expected: 0; LOAD-BEARING GATE)
```

And from PR-Y24 own test:

```
[pr-y24-test] F0044 batch max `[topo-extract] summary: unpaired=N`: Some(0)
   (canary §2: pre-PR baseline 0 across all 7 invocations; cohort guard, must stay 0 per spec §5 I2)
```

**Verdict:** **ACCEPT** — MAX over all 7 invocations = 0. I2 cohort preservation confirmed.

Logs: `/tmp/y24-adv-gate45.log` (PR-Y22 regression) + `/tmp/y24-adv-gate45-own.log` (PR-Y24 own); both exit 0, both with `2 passed; 0 failed`.

### Gate 5 — F0044 batch `[twin-oracle] unpaired_count=0`

**Verbatim:**

```
[pr-y22-test] F0044 batch max `[twin-oracle] unpaired_count`: Some(0) (regression guard: must stay 0)
[pr-y24-test] F0044 batch max `[twin-oracle] unpaired_count`: Some(0) (cohort guard, must stay 0 per spec §6 oracle #4)
```

**Verdict:** **ACCEPT** — MAX over all 7 invocations = 0.

Both PR-Y22 tests pass (gate 4+5 brief requirement met):
- `pr_y22_f0020_mode_a_missing_zero`: ok
- `pr_y22_f0044_b5_mode_a_missing_drops_by_2`: ok

Both PR-Y24 own tests pass:
- `pr_y24_f0020_twin_oracle_zero`: ok — F0020 max `[twin-oracle] unpaired_count`: 2 → **0** (LOAD-BEARING per spec §5 I3)
- `pr_y24_f0044_topo_extract_no_regression`: ok — cohort 0/0 preserved

### Gate 6 — Yang fast ≥ 10/157

**Verbatim (last line of yang_fast log):**

```
Yang fast: 10/157 passed, 140 failed, 7 errored (skipped 33 known timeouts)
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 25 filtered out; finished in 421.08s
```

**Verdict:** **ACCEPT** — meets ≥10 threshold. Spec §6 oracle #7 expected ≥11 if F0020 returns; F0020 still Failed (next-layer per gate 1) so 10 holds. No drop, no regression.

Log: `/tmp/y24-adv-gate6.log` (exit 0).

### Gate 7 — Kernel baseline IMPROVED

**Pre-PR (`3c749a3`):** `1250 passed; 29 failed; 42 ignored`
**Post-PR (`8b8297c`):** `1254 passed; 25 failed; 42 ignored`

**Delta:** +4 passing, −4 failing, ZERO new failures.

**Verdict:** **ACCEPT** — no drop, in fact improvement. Brief decision tree: "Gate 7 shows kernel baseline drop → REJECT" — does NOT trigger; baseline IMPROVED.

Logs: `/tmp/y24-adv-baseline-kernel.log` + `/tmp/y24-adv-postpr-kernel.log`.

---

## §3 Independent test-name diff (gate 7 verification)

Per brief: "If post-PR shows different test counts, identify which tests changed."

I extracted failing-test names from both runs and diffed:

```
$ comm -23 /tmp/y24-adv-baseline-failures.txt /tmp/y24-adv-postpr-failures.txt
# Tests that FAILED in baseline but NOT in post-PR (FLIPPED FAIL→PASS):
    boolean::yang_integration::tests::test_yang_3level_chained_boolean_face_geometry
    boolean::yang_integration::tests::test_yang_chained_boolean_succeeds
    boolean::yang_integration::tests::test_yang_face_geometry_completeness
    boolean::yang_integration::tests::test_yang_subtract_face_geometry_complete

$ comm -13 /tmp/y24-adv-baseline-failures.txt /tmp/y24-adv-postpr-failures.txt
# Tests that FAILED in post-PR but NOT in baseline (PASS→FAIL — REGRESSION!):
(empty)
```

**Independently verified:** exactly 4 yang_integration tests flipped FAIL→PASS, **zero** new failures. Implementer's claim in commit message verbatim:

> Kernel baseline: 1254 pass / 25 fail / 42 ignored
> (vs 3c749a3 baseline 1250/29/42 — 4 yang_integration tests now pass
> that previously hit the (38,27) class panic; zero new failures)

is accurate to the test-name level.

---

## §4 Paper audit — citation hygiene

Per brief: "verify the implementer's code comments cite Yang §3 + Cherchi §3 verbatim phrases (not the imprecise 'Yang §4.4.2 directional-symmetry' wording PR-Y23 banked)."

### NEW comments in PR-Y24 diff (verbatim quotes)

In `crates/kernel/src/topology/arena.rs` (NEW field doc-comment, L13-26):
> "PR-Y24: construction-time directed-edge mapping per half-edge,
> populated at the close of `topology_extract::extract_topology`
> Step 7 from `directed_he` keys."

Cited papers as separate verbatim quotes — clean.

In `crates/kernel/src/boolean/topology_extract.rs` (Step 7 close, L1414-1437):
> "Yang 2025 §3 ('each edge shared by two adjacent faces') and Cherchi 2022 §3
> ('surface patches are bounded by closed loops of non-manifold edges, namely
> the intersection lines') establish that the patch-boundary directed-edge
> set inserted at L1119-1146 IS the input ground truth for the NMM
> predicate; arena-traversal `(he.origin, he.next.origin)` is a derivative
> view, polluted on open-chain wrap-backs at L1131-1146."

Verbatim Yang §3 phrase + verbatim Cherchi §3 phrase. Clean.

In `crates/kernel/src/boolean/topology_extract.rs` (Site A oracle, L1460-1471):
> "Yang 2025 §3: 'edges that form a continuous boundary, with each edge shared
> by two adjacent faces.' Cherchi 2022 §3: 'the arrangement is guaranteed to
> be a well formed simplicial complex and surface patches are bounded by
> closed loops of non-manifold edges, namely the intersection lines.'"

Verbatim quotations from both papers. Clean.

In `crates/kernel/src/boolean/yang_integration.rs` (Site B validator, L1241-1247):
> "Yang 2025 §3 + Cherchi 2022 §3: patch boundaries are closed loops of
> non-manifold edges; the predicate reads input classification, not arena
> traversal which is polluted on open-chain wrap-backs..."

Section-cited correctly. Clean.

### "Yang §4.4.2 directional-symmetry" residuals

I scanned for `directional-symmetry` AND `Yang.*§4\.4\.2.*directional` in both baseline and post-PR:

```
$ cd /tmp/y24-baseline-wt && grep -rn "directional-symmetry" crates/kernel/src/
crates/kernel/src/boolean/yang_integration.rs:1187: § 4.4.2 directional-symmetry mandate). Distinction between
crates/kernel/src/boolean/yang_integration.rs:1251: Ref: Yang 2025 § 4.4.2 (directional-symmetry mandate; NMM allowed).
crates/kernel/src/boolean/yang_integration.rs:1385: legitimate per Yang §4.4.2 directional-symmetry mandate
crates/kernel/src/topology/half_edge.rs:39:        §4.4.2 directional-symmetry mandate; PR-Y20-MODE-A): a directed edge whose

$ cd /home/claude/workspace && grep -rn "directional-symmetry" crates/kernel/src/
crates/kernel/src/boolean/yang_integration.rs:1187: § 4.4.2 directional-symmetry mandate). Distinction between
crates/kernel/src/boolean/yang_integration.rs:1265: Ref: Yang 2025 § 4.4.2 (directional-symmetry mandate; NMM allowed).
crates/kernel/src/boolean/yang_integration.rs:1406: legitimate per Yang §4.4.2 directional-symmetry mandate
crates/kernel/src/topology/half_edge.rs:39:        §4.4.2 directional-symmetry mandate; PR-Y20-MODE-A): a directed edge whose
```

All 4 residual references are **carryovers from baseline** (PR-Y20/PR-Y22 era). PR-Y24 introduced **ZERO** new uses. Per brief decision tree: "Paper audit finds new uses of 'Yang §4.4.2 directional-symmetry' or other refuted phrasing → REJECT (citation hygiene violation)" — does NOT trigger; PR-Y24 introduces zero new uses, and spec §8.3 explicitly notes "carry-forward from PR-Y23 §8.3" (banked, not retroactively scrubbed).

The `[yang-diag]` log line at runtime still emits the legacy phrasing, but this comes from baseline `yang_integration.rs:1406` and is part of the banked carryover, not a PR-Y24 introduction.

**Verdict:** **PASS** — citation hygiene clean for PR-Y24's new comments.

---

## §5 Cohort regression analysis — I2 structural argument

Per brief: "does PR-Y24's structural argument (I2: pairing logic untouched) hold up empirically? Inspect the diff at `crates/kernel/src/boolean/topology_extract.rs` — confirm L1219-1380 (pairing pass) was NOT modified."

### Hunk-header sweep

```
$ git diff 3c749a3..8b8297c -- crates/kernel/src/boolean/topology_extract.rs | grep "^@@"
@@ -1414,6 +1414,29 @@ pub(crate) fn flood_fill_patches(
@@ -1437,16 +1460,24 @@ pub(crate) fn flood_fill_patches(
@@ -1458,10 +1489,10 @@ pub(crate) fn flood_fill_patches(
@@ -1512,7 +1543,13 @@ pub(crate) fn flood_fill_patches(
```

All four hunks land at L1414, L1437, L1458, L1512 — **all OUTSIDE the L1219-1380 pairing-logic zone**. The closest hunk is at L1414, which is 34 lines DOWNSTREAM of L1380. The pairing-search loop is byte-identical pre/post.

**I2 verified:** zero modifications to L1131-1146 (arena population) or L1219-1380 (pairing-search loop).

### Empirical I2 confirmation via cohort tests

`pr_y22_f0044_b5_mode_a_missing_drops_by_2` reports identical `[topo-extract] summary: paired=N, unpaired=0, ambiguous=0` line counts and values as canary §2 baseline (7 invocations, all 0/0). PR-Y22 tests pass — pairing logic is byte-identical and the cohort guard is structurally invariant under PR-Y24's observation-layer-only change. Per `feedback_local_fix_for_global_invariant.md`: PR-Y24 is a global-invariant fix (NMM-classification predicate reads global ground truth `directed_he`) executed at the observation layer rather than per-element in isolation. The B1 plumb-via-arena-field shape preserves validator independence (spec §4.3 reasoning #1).

### Total LOC change

```
$ git diff 3c749a3..8b8297c --stat
 crates/kernel/src/boolean/topology_extract.rs | 65 +++++++++++++++++++++------
 crates/kernel/src/boolean/yang_integration.rs | 39 ++++++++++++----
 crates/kernel/src/topology/arena.rs           | 14 ++++++
 3 files changed, 95 insertions(+), 23 deletions(-)
```

Bounded scope. Three production files; no test files modified by impl-y24 (test was authored separately by test-y24 in commit `87e1c1d`). No spec file modification.

---

## §6 Banked findings (for PR-Y25+)

### §6.1 F0020 next-layer surface

The F0020 case after PR-Y24 fails on **mesh-quality oracles**, not topology. The new failure surface comprises five distinct quality issues:

1. `watertight_mesh: 36 unpaired edges out of 130 total (34 boundary, 2 non-manifold)`
2. `consistent_normals: 2 of 76 triangles have reversed normals`
3. `no_degenerate_triangles: 4 of 76 triangles are degenerate`
4. `no_self_intersection: 10 inter-face triangle penetrations, face pairs: (0,3), (0,5), (1,3), (2,5), (3,13), ...`
5. `mesh_euler_characteristic: V(50) - E(130) + F(76) = -4 (expected 2)`

The `[yang-diag] NMM half-edges: 39 of 169 total (33 faces, 65 edges, 80 vertices)` line shows the **Yang topology-extract layer** is GREEN; the failure is in **downstream mesh assembly / retessellation**. The 2 non-manifold edges in (1) and the 10 inter-face penetrations in (4) are likely the **same defect** mentioned as banked Layer-4 (NMM-render layer / face-iteration on open-chain faces) per spec §7.2 examples — PR-Y24 ABORT § 7.2 "Downstream tessellation-render NMM-handling layer (PR-Y21 ABORT residual; banked)". This is the rightful **PR-Y25 anchor**.

### §6.2 Cohort consistency: the 4 yang_integration tests that flipped FAIL→PASS

The 4 tests that flipped were:
- `test_yang_3level_chained_boolean_face_geometry`
- `test_yang_chained_boolean_succeeds`
- `test_yang_face_geometry_completeness`
- `test_yang_subtract_face_geometry_complete`

All four are face-geometry-completeness tests in `yang_integration::tests`. Their pre-PR failure was likely the same `(38,27)` validator class panic that F0020 hit (since the validator was inspecting NMM via arena-traversal). Post-PR-Y24 they pass because the validator no longer rejects legitimate-NMM HEs as missing-edge defects.

This is a **silent +4 wins** beyond the explicit PR-Y24 spec/test scope. Worth noting in the PR-Y24 close-out memory update — PR-Y24 has cohort impact wider than just F0020 b#2.

### §6.3 Three persistent failures in `boolean::yang_integration::tests`

Even after PR-Y24, these 2 yang_integration tests remain failing:
- `test_yang_face_geometry_fallback_valid_normal`
- `yang_pipeline_respects_internal_timeout`

These are **not** PR-Y24 territory; they are pre-existing failures unaffected by the observation-layer change. Banked for future investigation.

### §6.4 F0044 cohort invocation #5 fragility (canary §6 finding 3 carryover)

Canary §6 finding 3 noted invocation #5 (229 HEs) has `arena_only_count=4` (the only divergent invocation in the F0044 batch). Spec §4.3 reasoning #2 cited this as the rationale for selecting B1 over B2 — preserve validator independence to defend against future cohorts where divergence might shift verdict. PR-Y24 ships with B1 selected, so the defense-in-depth is in place. **No action needed for now**; monitor under future corpus growth.

### §6.5 `tau_weld` field surfaced in baseline tests

The 4 yang_integration tests that flipped use scenarios that exercise the `validate_yang_result_topology` path. Their flipping confirms PR-Y24's fix has cohort impact beyond the spotlight cases. If similar tests exist in feature-engine or test-harness, they may also benefit silently. Not in PR-Y24 scope to enumerate; flagging for PR-Y25 audit if relevant.

---

## §7 Final-report block

### `git diff HEAD --stat`

Pre-commit (after gates ran, before adding memo):

```
$ git diff HEAD --stat
 app/tests/cases/assay/results.json | 142 ++++++++++++++++++-------------------
 1 file changed, 71 insertions(+), 71 deletions(-)
```

The `app/tests/cases/assay/results.json` modification (71/71 line churn) is the spotlight-test side-effect explicitly excluded from this role per brief §0(8) ("if that's modified, that's a known test side-effect, NOT a PR-Y24 regression. Document it but do NOT revert it from this role — team-lead handles cleanup."). I did NOT revert.

Adversary memo (`docs/audits/pr_y24_validation.md`) is the sole intentional production-tree addition from this role; it appears as `Untracked files:` pre-commit. After commit, only the memo + (incidental) results.json delta will be in the working state — confirming zero non-memo production-code changes from this role.

### Worktree cleanup

```
$ git worktree list
/home/claude/workspace                8b8297c [main]
/tmp/auto-waffle-2026-03-20T09-00-19  66a56a5 [auto-waffle/2026-03-20T09-00-19] prunable
/tmp/auto-waffle-2026-04-04T19-23-01  a833db1 [auto-waffle/2026-04-04T19-23-01] prunable
```

`/tmp/y24-baseline-wt` removed cleanly. The two `auto-waffle-*` worktrees are pre-existing prunable artifacts unrelated to PR-Y24.

### Logs retained in `/tmp/`

- `/tmp/y24-adv-gate1.log` — F0020 spotlight (gate 1) — exit 0
- `/tmp/y24-adv-gate2.log` — F0030 spotlight (gate 2) — exit 0
- `/tmp/y24-adv-gate3.log` — F0050 spotlight (gate 3) — exit 0
- `/tmp/y24-adv-gate45.log` — PR-Y22 regression test (gates 4+5) — 2/2 pass
- `/tmp/y24-adv-gate45-own.log` — PR-Y24 own test (gates 4+5) — 2/2 pass
- `/tmp/y24-adv-gate6.log` — yang_fast (gate 6) — 10/157 ≥ 10
- `/tmp/y24-adv-baseline-kernel.log` — kernel baseline pre-PR — 1250/29/42
- `/tmp/y24-adv-postpr-kernel.log` — kernel baseline post-PR — 1254/25/42
- `/tmp/y24-adv-baseline-failures.txt` + `/tmp/y24-adv-postpr-failures.txt` — failure-name diff inputs

These should be cleaned up at close-out by `lead-y24`.

---

## §8 Routing

- **Verdict: ACCEPT.**
- **All 7 gates pass.** Gate 1 surfaces *different* panic at next-layer mesh-quality (per spec §7.2 expected outcome). Gate 7 baseline IMPROVED.
- **I2 structural argument verified empirically** — diff hunk headers confirm L1219-1380 (pairing logic) byte-identical pre/post.
- **Citation hygiene clean** — PR-Y24's NEW comments cite Yang §3 + Cherchi §3 verbatim; ZERO new uses of "Yang §4.4.2 directional-symmetry" wording. The 4 residual `directional-symmetry` strings are carryovers from baseline, not introduced by PR-Y24.
- **Cohort impact wider than spec** — silent +4 yang_integration test wins beyond the F0020 spotlight scope. Net kernel improvement.
- **Next agent:** `lead-y24` (close-out: clippy + fmt + WASM rebuild + commit memo + push + TeamDelete).
- **Banked for PR-Y25+:** F0020 next-layer mesh-quality surface (NMM-render / face-iteration on open-chain faces); 2 persistent yang_integration test failures unaffected by PR-Y24; F0044 invocation #5 fragility (defense-in-depth via B1 already in place).
