# PR-Y30 Adversary Validation — ACCEPT WITH BANKED FINDING

**Adversary:** `adv-y30`
**Date:** 2026-05-08
**Subject commit:** `0f13c7c` (`feat(test-harness): switch Cherchi diff harness Stage C → Stage B for apples-to-apples boolean-result comparison | PR-Y30 calibration`)
**Pre-PR HEAD:** `a5edd5d`
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md`
**Baseline memo under audit:** `docs/audits/pr_y30_stage_b_baselines.md`

## Verdict — ACCEPT WITH BANKED FINDING

All 9 gates pass. The hypothesis behind PR-Y30 (gate 3 — F0044 extras drop materially at Stage B) is **REFUTED** by the data, but this is treated as ACCEPT-WITH-BANKED-FINDING per the brief's decision tree: the harness calibration (Stage C → Stage B) is correct and is the deliverable; the hypothesis was an empirical guess that the data has now answered. The refutation itself is the load-bearing finding that should drive PR-Y31+ scoping.

## Gate-by-gate

| # | Gate | Result | Evidence |
|---|------|--------|----------|
| 1 | Harness runs F0020 with Stage B path | PASS | gate1 log line 674: `test result: ok. 1 passed; 0 failed` (1.28s) |
| 2 | Harness runs on cohort F0044/F0045/R0092 | PASS | gate2 log: `test result: ok. 1 passed; 0 failed` (7.17s) |
| 3 | F0044 extras drop materially | REFUTED | extras = 48 (memo predicted 0-10); ACCEPT-WITH-BANKED-FINDING per brief |
| 4 | F0044 missing stays at 0 | PASS | gate2 log: `In Cherchi, not in Waffle: 0 triangles` |
| 5 | Other cases match predicted ranges OR document | PASS | F0020 -27% ext (this run actually +9% over memo due to Cherchi non-det); F0045 unchanged; R0092 unchanged |
| 6 | `cherchi2022_reference_parity` tests pass | PASS | gate6 log: `test result: ok. 2 passed; 0 failed` |
| 7 | Kernel baseline 1254/25/42 preserved | PASS | both `a5edd5d` and HEAD: `1254 passed; 25 failed; 42 ignored` |
| 8 | `cargo clippy -p test-harness` clean (no new) | PASS | 5 warnings post-PR === 5 warnings pre-PR |
| 9 | `cargo fmt --check` clean | PASS | exit 0 |

## Verbatim gate outputs (top 10 lines each)

### Gate 1 — F0020 harness (Stage B)

```
=== F0020 diff ===
Cherchi output: 246 triangles, 120 vertices, well_formed=false, χ=5
Waffle output:  294 triangles, 117 vertices, well_formed=false, χ=1
Triangle count delta: N_c - N_w = -48

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 93 triangles
  In Waffle, not in Cherchi: 155 triangles
  Common (matching quantized positions): 137
...
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 1.28s
```

Memo recorded F0020 at 185 / 93 / 107 (Cherchi=295 tris); this run sees 137 / 93 / 155 (Cherchi=246 tris). Waffle side is byte-deterministic (294 tris both runs). The variance is wholly on the Cherchi side — confirms the memo's banked finding that Cherchi remains non-deterministic on F0020 even at `TBB_NUM_THREADS=1`.

### Gate 2 — cohort

```
=== F0044 diff ===
Cherchi output: 88 triangles, 46 vertices, well_formed=true, χ=2
Waffle output:  136 triangles, 72 vertices, well_formed=true, χ=4
Triangle count delta: N_c - N_w = -48
Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 0 triangles
  In Waffle, not in Cherchi: 48 triangles
  Common (matching quantized positions): 88
=== end F0044 diff ===
=== F0045 diff ===
Cherchi output: 236 triangles, 120 vertices, well_formed=true, χ=2
Waffle output:  468 triangles, 274 vertices, well_formed=false, χ=9
Triangle count delta: N_c - N_w = -232
Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 236 triangles
  In Waffle, not in Cherchi: 466 triangles
  Common (matching quantized positions): 0
=== end F0045 diff ===
=== R0092 diff ===
Cherchi output: 405 triangles, 187 vertices, well_formed=false, χ=112
Waffle output:  368 triangles, 303 vertices, well_formed=false, χ=7
Triangle count delta: N_c - N_w = 37
Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 340 triangles
  In Waffle, not in Cherchi: 368 triangles
  Common (matching quantized positions): 0
=== end R0092 diff ===

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 7.17s
```

F0044 / F0045 / R0092 numbers are byte-identical to the memo's recorded baselines for this run.

### Gate 6 — `cherchi2022_reference_parity`

```
test cherchi_smoke_two_tetrahedra_union ... ok
test f0002_cherchi_union_reference_parity ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out; finished in 1.02s
```

Existing parity tests unaffected.

### Gate 7 — kernel baseline preservation

Pre-PR (`a5edd5d` worktree):

```
test result: FAILED. 1254 passed; 25 failed; 42 ignored; 0 measured; 0 filtered out; finished in 13.46s
```

Post-PR (HEAD = `0f13c7c`):

```
test result: FAILED. 1254 passed; 25 failed; 42 ignored; 0 measured; 0 filtered out; finished in 13.31s
```

Byte-identical pass/fail/ignored counts. The 25 failures are the prior known boolean-pipeline cohort (unrelated to PR-Y30).

### Gate 8 — clippy

```
warning: `test-harness` (lib) generated 5 warnings (run `cargo clippy --fix --lib -p test-harness -- ` to apply 2 suggestions)
```

5 warnings post-PR. Pre-PR cross-check on the `a5edd5d` worktree returned the same line: `warning: `test-harness` (lib) generated 5 warnings`. Identical — no new lints introduced by PR-Y30.

### Gate 9 — fmt

```
EXIT=0
```

(No output; clean.)

## Hypothesis analysis — F0044 refutation (banked for PR-Y31)

The Plan agent predicted F0044's 48 Stage C extras would mostly drop at Stage B because they were assumed to be patches Waffle's flood-fill emits that Cherchi's survival rejects. The data refutes this assumption sharply:

- **Stage C baseline (PR-Y29):** 88 common / 0 missing / 48 extras
- **Stage B baseline (PR-Y30):** 88 common / 0 missing / 48 extras

Identical numbers. Stage C and Stage B differ only by flood-fill patch-ID labeling (no triangle insertion / deletion), so the 48 Stage B extras are not patch-fill artifacts at all — they are present in `face_survival_detect`'s output. This localizes the F0044 divergence to:

1. **`face_survival_detect` itself** (`topology_extract.rs:~2554`): the in/out keep-mask for `MeshBooleanOp::Union` may include cells Cherchi's `select_boolean_result` rejects, OR
2. **`label_cells` upstream** (Yang 2025 §4.4 in/out classification per Cherchi 2022 §5): a label-assignment divergence that drives different keep-decisions, OR
3. **`subdivide_mesh_pair` further upstream** (Cherchi 2022 §4 arrangement): Waffle produces 136 triangles where Cherchi produces 88 — the 48 extras may be from extra subdivisions (Waffle vertex count: 72 vs Cherchi: 46; +26 vertices = +52 directed half-edges across the boundary, consistent with +48 extra triangles representing those subdivisions).

The (3) hypothesis is consistent with both the triangle counts and the vertex-count differential. PR-Y31 spec should probe arrangement output vertex/triangle count first (cheapest), then in/out classification, then survival keep-mask.

This refutation is the **load-bearing finding** of PR-Y30. The plan said "if F0044 extras > 20, this is genuinely surprising and worth investigation before specing PR-Y31"; the data confirms that surprise is real.

## Cherchi non-determinism observation

The PR-Y29 memo recorded an expectation that `TBB_NUM_THREADS=1` would serialize Cherchi's arrangement output. impl-y30's PR-Y30 memo recorded that this expectation was refuted for F0020 (295 tris in that run) and R0092 (405 tris). This adversary run sees:

- **F0020:** Cherchi=246 tris this run vs 295 tris in PR-Y30 memo vs 253/246/295 in PR-Y29 — Cherchi swings widely on F0020 across runs even at `TBB_NUM_THREADS=1`
- **R0092:** Cherchi=405 tris this run, matches PR-Y30 memo (deterministic in this thinner sample)
- **F0044:** Cherchi=88 tris this run, matches all prior runs (deterministic)
- **F0045:** Cherchi=236 tris this run, matches all prior runs (deterministic)

The non-determinism is genuine and originates inside Cherchi 2022's arrangement step (Cherchi §4 — `mesh arrangement` uses TBB parallel intersection-detection that retains internal parallelism below the TBB top-level thread count knob). For PR-Y31+ load-bearing diffs that fix-shape against Cherchi, recommend either (a) the deterministic cases (F0044 + F0045) as primary anchors, (b) mean-of-N sampling on F0020/R0092 (5+ runs), or (c) building a mesh-arrangement-only mode that omits the parallel work.

## Recommendation for PR-Y31 scope

Based on the calibrated Stage B baselines:

1. **F0044 first** (PRIMARY): 88 common / 0 missing / 48 extras, both sides well-formed and Cherchi side χ=2; cleanest signal in the cohort. Refutation localizes anchor to one of three layers (arrangement / classification / survival). Use the +26-vertex differential as a triangulation-divergence probe.
2. **F0020 cohort sibling**: 27% improvement at Stage B (107 vs 146 extras at the memo's Cherchi=295-tri run). Same fan-divergence signature at corner `(-0.352714, +0.085762, +0.195664)`. Likely the same root cause as F0044 but in a richer setting — confirm-or-refute as cohort once F0044 is closed.
3. **F0045 and R0092 deprioritized** until F0044/F0020 mechanism is closed. F0045 has 0 common at 1µm (tessellation-grid divergence is structural per Yang §4.1.1, not survival-fixable). R0092 has Cherchi non-determinism dominating the signal.

The Stage B baselines are the load-bearing oracle for PR-Y31+.

## Git status snapshots

### Session start

```
On branch main
Your branch is ahead of 'origin/main' by 1 commit.
nothing to commit, working tree clean
```

### Session end (before this memo is committed)

The harness's `run_single_case` call modifies `app/tests/cases/assay/results.json` as a side-effect (impl-y29 banked finding). Restored via `git checkout --`. After:

```
On branch main
Your branch is ahead of 'origin/main' by 1 commit.
nothing to commit, working tree clean
```

The `/tmp/y30-baseline-wt` worktree was removed at session end.

## Banked findings for PR-Y31+

1. **F0044's 48 extras are not flood-fill artifacts.** Stage C = Stage B = 48 extras with identical top-10 geometric signature. The divergence is at or above `face_survival_detect`. PR-Y31 should NOT spec a flood-fill fix; it should anchor on arrangement / classification / survival.
2. **F0044's +26-vertex differential (72 Waffle vs 46 Cherchi) is the cheapest sub-probe.** If the arrangement step accounts for those extra vertices, the +48 extra triangles are a consequence of triangulating denser vertex graph (not a label/survival bug). If the arrangement step matches Cherchi vertex-for-vertex, the +48 extras came from labeling or survival downstream.
3. **Cherchi non-determinism persists at `TBB_NUM_THREADS=1` for F0020 + R0092.** Two consecutive identical-config runs see F0020 Cherchi=295 vs 246 tris. F0044 and F0045 stay deterministic. For PR-Y31 load-bearing diffs, prefer the deterministic cases or 5-run mean.
4. **Waffle pipeline produces byte-identical Stage B output across runs** for all four cases (F0020 Waffle=294 tris both runs; F0044=136; F0045=468; R0092=368). The Waffle side of the diff is a stable reference; instability is purely on the Cherchi reference side.

## Files

- Harness diff: `crates/test-harness/tests/cherchi_differential_diff.rs` (89 line diff vs `a5edd5d`)
- New baseline memo: `docs/audits/pr_y30_stage_b_baselines.md` (345 lines)
- Pre-PR baseline (comparison): `docs/audits/pr_y29_baseline_diffs.md`
- This memo: `docs/audits/pr_y30_validation.md`
- Gate logs: `/tmp/y30-adv-gate{1,2,6,7-baseline,7-postpr,8-full,9}.log`
