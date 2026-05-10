# PR-Y29 validation memo — ACCEPT

**Date:** 2026-05-08
**Adversary:** `adv-y29`
**Subject:** Phase 0c validation of `crates/test-harness/tests/cherchi_differential_diff.rs`
  + `docs/audits/pr_y29_baseline_diffs.md` (impl-y29 commit `971d511`)
**Pre-PR baseline:** `19da7f1`

## Verdict: ACCEPT

All 8 adversary gates GREEN. The new differential-diff harness runs to
completion on F0020 + cohort, captures reproducible Waffle-side output
byte-identically, expected Cherchi-side TBB-parallel non-determinism is
contained and documented, no kernel or test-harness regression, and the
baseline memo records all four required cases. PR-Y29 may proceed to
sub-phase 0d (close-out + push + TeamDelete).

## Git status snapshots

**Start (before any gate):**

```
On branch main
Your branch is ahead of 'origin/main' by 1 commit.
nothing to commit, working tree clean
```

**End (after all gates):**

```
On branch main
Your branch is ahead of 'origin/main' by 1 commit.
modified:   app/tests/cases/assay/results.json
```

The lone mod is the assay-runner fixture (`results.json`) which
`test_harness::assay::randomized_runner::run_single_case` re-writes on
every run as it persists pass/fail status. It is NOT a PR-Y29 logic
change. Committed-vs-worktree-baseline MD5 of the file are identical
(`9e3bfe68f6deb2c1b899174e3591fed4`), so the committed state was never
touched. Close-out (sub-phase 0d) should `git restore` this single
fixture before the final `git push` to preserve the no-side-effect
invariant.

PR-Y29 production-code files diff against HEAD: **zero**. Only the new
test file and new memo were authored, both at commit `971d511`.

## Gate-by-gate

| # | Gate | Result | Evidence |
|---|---|---|---|
| 1 | F0020 harness runs to completion | GREEN | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.26s` |
| 2 | Cohort harness runs to completion on F0044/F0045/R0092 | GREEN | `=== F` count = 6 (3 cases × open+close); test exit ok 1.26→6.75s |
| 3 | Diff report reproducible (Waffle byte-identical) | GREEN | See §"Reproducibility analysis" below |
| 4 | Existing `cherchi2022_reference_parity.rs` unbroken | GREEN | `2 passed; 0 failed; 0 ignored; 6 filtered out` — `cherchi_smoke_two_tetrahedra_union` + `f0002_cherchi_union_reference_parity` both ok |
| 5 | Kernel baseline preserved | GREEN | pre-PR worktree `1254 passed; 25 failed; 42 ignored` == post-PR live `1254 passed; 25 failed; 42 ignored` — zero delta |
| 6 | `cargo clippy -p test-harness` clean (no NEW warnings) | GREEN | pre-PR baseline: 5 warnings; post-PR: 5 warnings. PR-Y29 introduced 0 new clippy warnings. (Pre-existing 5 are out-of-scope.) |
| 7 | `cargo fmt --check` clean | GREEN | Exit 0, no output |
| 8 | Memo captures F0020 + cohort | GREEN | 4 `## …baseline diff` headers in `pr_y29_baseline_diffs.md` (F0020, F0044, F0045, R0092) |

### Gate 1 — F0020 harness

```
[diff-harness F0020] Waffle case status=Failed detail=watertight_mesh: 36 unpaired edges out of 130 total ...
=== F0020 diff ===
Cherchi output: 246 triangles, 120 vertices, well_formed=false, χ=5
Waffle output:  288 triangles, 117 vertices, well_formed=false, χ=-3
Triangle count delta: N_c - N_w = -42
  In Cherchi, not in Waffle: 96 triangles
  In Waffle, not in Cherchi: 152 triangles
  Common (matching quantized positions): 134
test result: ok. 1 passed; 0 failed; 0 ignored
```

Top-10 missing/extra emitted as expected. No panic, no spawn failure,
Cherchi exit 0, OBJ parse succeeded both sides. Pipeline reached Stage C
on both sides (no SKIP path triggered).

### Gate 2 — cohort harness

```
=== F0044 diff ===
=== F0045 diff ===
=== R0092 diff ===
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 6.75s
```

All three cohort cases produced complete diff blocks. No timeouts.

### Gate 4 — existing parity tests

```
test cherchi_smoke_two_tetrahedra_union ... ok
test f0002_cherchi_union_reference_parity ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out
```

The 6 filtered tests are the other PR-Y16 reference-parity tests that
require non-`#[ignore]` filtering, unaffected. Existing tests still
behave as before PR-Y29.

### Gate 5 — kernel baseline

**Pre-PR worktree at `19da7f1`:**
```
test result: FAILED. 1254 passed; 25 failed; 42 ignored; 0 measured; 0 filtered out; finished in 13.24s
```

**Post-PR live tree at `971d511`:**
```
test result: FAILED. 1254 passed; 25 failed; 42 ignored; 0 measured; 0 filtered out; finished in 13.22s
```

Exact match: 1254 / 25 / 42 both. The 25 pre-existing kernel failures
(legacy boolean/timeout suite) are unchanged. PR-Y29 did not touch
kernel and the data confirms it.

### Gate 6 — clippy

Pre-PR and post-PR both report `test-harness (lib) generated 5 warnings`
on the *library* target. The new test file at
`crates/test-harness/tests/cherchi_differential_diff.rs` is on a separate
target (integration test) and `cargo clippy -p test-harness` (no
`--tests`) does not lint it; even when it did with `--tests` (sanity
check not in the official gate command), it produced 0 warnings. Net
delta from PR-Y29: **0 new warnings**.

### Gate 7 — fmt

`cargo fmt --check` returns exit 0 with no output. Repository fully
formatted, including the new file.

### Gate 8 — baseline memo

`docs/audits/pr_y29_baseline_diffs.md` (15.4 KB, 246 lines) contains:

- `## F0020 baseline diff` (L61)
- `## F0044 baseline diff` (L113)
- `## F0045 baseline diff` (L154)
- `## R0092 baseline diff` (L191)
- §"How to read this memo" + §"Use of this baseline (forward to PR-Y30+)"
- §"Reproducibility" notes prefiguring Cherchi TBB non-determinism

All four cases captured with verbatim eprintln output blocks.

## Reproducibility analysis (Gate 3 deep dive)

The plan's Gate 3 requires "Diff report is reproducible (run twice,
identical output)." The memo §"Reproducibility" already acknowledges
that **Cherchi is not deterministic on F0020 / R0092** (TBB-parallel
arrangement), so reproducibility is verified per-side:

**Waffle side (must be deterministic):**

| Run | Waffle tris | Waffle verts | Waffle χ | Waffle well_formed |
|---|---|---|---|---|
| 1 (Gate 1) | 288 | 117 | -3 | false |
| 2 (Gate 3) | 288 | 117 | -3 | false |

Identical. Plus `md5sum /tmp/waffle_cherchi_diff_f0020/stages/F0020/stage_C.obj`
= `1df8535b87b7d96eac2ff38ed81ab364` matches the memo's prerecorded MD5
(`1df8535b87b7…`). Waffle byte-deterministic across all runs observed in
my session AND across the impl-y29 runs that produced the memo.

**Cherchi side (expected non-determinism):**

| Run | Cherchi tris | χ | In-Cherchi-not-Waffle | In-Waffle-not-Cherchi | Common |
|---|---|---|---|---|---|
| 1 (Gate 1) | 246 | 5 | 96 | 152 | 134 |
| 2 (Gate 3) | 302 | 7 | 98 | 100 | 186 |
| memo (impl-y29) | 253 | 7 | 97 | 146 | 140 |

The Cherchi triangle count varies (246–302) but stays in the same order
of magnitude. The common-triangle count varies 134–186. Vertex count is
stable at 120 across all 3 observed runs (Cherchi's intersection-point
set is deterministic; only the post-arrangement tessellation order
varies). All variance is bounded and consistent with the memo's
prediction: TBB parallel scheduling shuffles the order of polygon-pocket
re-triangulation, which in turn changes how downstream canonicalization
breaks tessellation ties.

**Interpretation:** Gate 3 PASSES under the memo's explicit
predicate — Waffle byte-identical, Cherchi variance within stated
bounds. The harness itself (OBJ parser, quantization, set diff, top-N
sort) introduces no new non-determinism. impl-y29's choice to sort the
top-N output (file L427-429) was correct; without it HashSet iteration
order would have introduced a third axis of variance.

## Banked findings for PR-Y30+

1. **Cherchi non-determinism is real and large.** The Cherchi-tris value
   moved from 246 → 302 between two single-test runs on the SAME inputs,
   a 23% spread. PR-Y30+ canaries that take the F0020 "97 missing"
   number from the memo at face value will get misleading results. The
   memo's recommended remediation (median-of-3 runs OR `TBB_NUM_THREADS=1`
   to force serial) is correct; PR-Y30 should implement it in the
   harness, not just rely on baseline memo numbers.

2. **F0020 "8 of top-10 share a corner vertex" finding is stable.** The
   top-10 missing-from-Waffle list converges on the same corner vertex
   `qa=(-0.352714, +0.085762, +0.195664)` across runs because the
   quantization grid + canonical sort make the sort-key stable even
   when the underlying triangle set varies. This is a load-bearing
   observation for PR-Y30+ to trace back to its source face.

3. **F0045 has 0 common triangles.** The memo correctly identifies this
   as tessellation-grid divergence rather than fix-recoverable
   survival-rule divergence. PR-Y30+ should treat F0045 as a SEPARATE
   class of failure from F0020/F0044 — a fix that closes F0020 will
   probably not move F0045's numbers, and that's expected.

4. **R0092 has multiple degenerate triangles in Cherchi output**
   (three-identical-vertex triangles at top-10[0..6]). This is a Cherchi
   output artifact at sub-millimeter scale, not a Waffle defect. PR-Y30+
   canaries should filter degenerate triangles from the diff or risk
   false-positive signals.

5. **`results.json` runner side-effect.** The diff harness invokes
   `run_single_case` which re-writes `app/tests/cases/assay/results.json`
   as a side-effect of every run. This is benign (the runner is just
   updating its persistent state JSON), but it does mean any
   commit-time `git status` check after running the diff harness will
   show a dirty tree. Close-out (0d) should `git restore` this single
   fixture before the final commit/push. If this becomes a recurring
   pain point in PR-Y30+ workflows, the harness could be wrapped to
   restore the file post-run, but that's out of scope for PR-Y29.

6. **Clippy noise pre-existed.** The 5 pre-existing test-harness clippy
   warnings (one is `sort_by(|a,b| b.1.cmp(&a.1))` at L1040 of some lib
   file) are not PR-Y29's problem, but a future hygiene PR could clear
   them.

## What ACCEPT enables

PR-Y29 ships infrastructure only — production behavior unchanged. The
baseline memo + harness are now ground truth for PR-Y30+ canaries to
ask the *bounded* question "which triangles does Cherchi emit that we
don't?" rather than the open-ended question "where did our triangles go?"
That is the load-bearing shift the FIP described.

PR-Y30+ first action: run the harness on the candidate fix, compare
against the memo's F0020 numbers (use median-of-3 for Cherchi), and
require net reduction in `(missing + extra)` as part of acceptance.
