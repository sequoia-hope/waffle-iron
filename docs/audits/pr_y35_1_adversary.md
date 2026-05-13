# PR-Y35.1 adversary — independent verification of canary + impl claims

**Verdict:** ACCEPT
**Author:** adversary-y35-1
**Date:** 2026-05-13
**Live tree HEAD:** `0d93b8d` (PR-Y35.1 impl, not pushed)
**Parent (PR-Y35 baseline):** `248dae7`
**Worktree:** read-only inspection of live tree; baseline replay via `git worktree add /tmp/y35-1-adv-baseline` and `/tmp/y35-1-adv-baseline2` (created + removed cleanly).

---

## §0 Single-paragraph verdict

Independent re-verification confirms every load-bearing PR-Y35.1 claim. The previously-`#[ignore]`'d test
`boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation` now **PASSES** at HEAD and
**FAILS** at parent `248dae7` (after non-destructively re-enabling it in a throwaway worktree) with the
exact predicted defect signature — `parent T0 has 4 sub-tris, T1 has 1 sub-tris` (sibling
passthrough'd un-split). The new isolated unit test
`boolean::cherchi::triangulation::tests::test_gate_widening_edge2pts_propagates_split_to_sibling`
also PASSES at HEAD. F0020 STAGE4 inv1 byte parity with Cherchi C++ is preserved at **84/84**
(canary's PR-Y35 win intact); F0044 hard gate (`pr_y31_f0044_extras_zero`) is preserved at
**0 missing / 0 extras / 136 common**. Kernel lib full suite lands at **1262 pass / 24 fail /
42 ignored** — one higher pass than canary's "1261/24/42" because the canary report inadvertently
did not include the impl-added unit test `test_gate_widening_edge2pts_propagates_split_to_sibling`
in its count; the plan (`plans/snappy-humming-hejlsberg.md` Phase 6 Gate I) explicitly predicts
1262 ("`PR-Y35 baseline 1260/24/43 + re-enabled test moves ignored→passed + impl-y35-1 added new
unit test = +2 pass / -1 ignored`"), so 1262 is the plan-correct value and the canary's count was
slightly low. **The failed-name 24-set is byte-identical to the PR-Y35 baseline 24-set** — zero
new RED tests. yang_fast at HEAD = **10/157 passed, 139 failed, 8 errored**, equal to PR-Y35
baseline (Gate H). Paper-grounding audit (Gate J) verified verbatim: Cherchi 2022 §3 lines
315-319 contain the segment-insertion quote; cinolib `triangulation.cpp:145-150` is byte-confirmed
to use only `triangleHasIntersections || triangleHasCoplanars` — no edge2pts check. PR-Y35.1's
edge2pts-widening is therefore a paper-grounded strict superset of Cherchi C++'s observed
behavior, exactly as the canary §4.3 claims. Two banked findings recorded: (1) Cherchi C++ TBB
non-determinism persists in F0020 Stage B missing-count (initial run measured 54; second run
measured 7 — Waffle output deterministic at 230 common in both); (2) canary §3 table reports
kernel lib total as 1261 but the correct figure including the impl-added unit test is 1262
(the plan agrees with 1262; canary's number is slightly low but is a memo-side discrepancy, not a
production defect). Recommendation: **ACCEPT**.

---

## §1 Discipline — non-destructive git proof

| Operation | Tool | Effect on live tree |
|---|---|---|
| Read `git show 0d93b8d --stat` and `git diff 248dae7..0d93b8d` | `git show` / `git diff` | none (read-only) |
| Read canary memo, spec, plan, paper text, cinolib source | `Read` | none |
| Gate D baseline replay | `git worktree add -f /tmp/y35-1-adv-baseline 248dae7` then `sed -i ...` *in worktree only* | new worktree at `/tmp/`; live tree unmodified |
| Gate D teardown | `git worktree remove /tmp/y35-1-adv-baseline --force` | worktree directory deleted; live tree unmodified |
| Gate G baseline replay (yang_fast) | `git worktree add -f /tmp/y35-1-adv-baseline2 248dae7` | new worktree; live tree unmodified |

**Forbidden ops used:** zero. No `git stash`, `git checkout <ref>`, `git reset`, or `git restore`
ever invoked on the live tree. Mutations of the baseline worktree's source files (via `sed -i`)
are confined to the throwaway worktree — that worktree is deleted after use.

`git worktree list` post-cleanup confirms only `/home/claude/workspace` (`0d93b8d [main]`) +
the pre-existing canary-y34 worktree + two prunable `auto-waffle` worktrees + the Gate G
baseline worktree (cleaned up below at §2 end). The live tree HEAD remains `0d93b8d`,
zero unintended mutations.

---

## §2 Gate-by-gate verification (A-J)

| Gate | Claim | Adversary measurement | Status |
|---|---|---|---|
| A. Diff shape & commit contents | 5 files; `triangulation.rs` widened gate + new unit test; `exact_mesh.rs` `#[ignore]` removed + docstring updated; `results.json` NOT staged | `git show 0d93b8d --stat` → 5 files: `wasm_bridge_bg.wasm` (binary, +6552 bytes), `triangulation.rs` (+115/-2 net), `exact_mesh.rs` (+10/-6 net), canary memo (+254), spec (+268). `app/tests/cases/assay/results.json` confirmed NOT in commit. Diff inspection confirms widened gate at L155-180 with `has_edge_split` closure, new unit test `test_gate_widening_edge2pts_propagates_split_to_sibling` at end of test mod, and `#[ignore = "PR-Y35.1 banked — subdivide_mesh_pair shared-edge propagation"]` line removed from `exact_mesh.rs:5418` with docstring updated from "(banked) will add" to "re-enables this test by widening the `triangulation.rs:155` gate". | PASS |
| B. Re-enabled test at HEAD | `test_subdivision_shared_edge_split_propagation` PASSES | `cargo test -p kernel boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation` → **1 passed; 0 failed; 0 ignored**. Test name no longer filtered by `--ignored`. | PASS |
| C. New unit test at HEAD | `test_gate_widening_edge2pts_propagates_split_to_sibling` PASSES | `cargo test -p kernel boolean::cherchi::triangulation::tests::test_gate_widening_edge2pts_propagates_split_to_sibling` → **1 passed; 0 failed; 0 ignored**. | PASS |
| D. Baseline replay (non-destructive) | Re-enabled test FAILS at parent `248dae7` | `git worktree add -f /tmp/y35-1-adv-baseline 248dae7`; in that throwaway worktree only, `sed -i` removes the single line `#[ignore = "PR-Y35.1 banked …"]`; `cargo test ... test_subdivision_shared_edge_split_propagation` → **FAILED**. Failure mode: `Non-conformal subdivision: parent T0 has 4 sub-tris, T1 has 1 sub-tris. Both share edge (1,2) which was intersected, so both must be split`. Exactly the canary §4.1 predicted defect signature (sibling T1 passthrough'd un-split). Worktree removed cleanly. | PASS (RED-on-baseline proven) |
| E. F0020 STAGE4 byte parity at HEAD | inv1 = 84, inv0 = 20 | `Y33_PROBE=1 ...` Cherchi-diff harness at HEAD → `wc -l /tmp/y35-1-adv/waffle/inv1/stage4_pairs.txt` = **84**; inv0 = **20**. Both byte-identical to PR-Y35 baseline. Stage B at HEAD shows common=230 (Waffle output deterministic); missing-count showed 54 on first run and 7 on a second rerun (Cherchi TBB non-determinism — banked, see §5). | PASS |
| F. F0044 hard gate | `pr_y31_f0044_extras_zero` PASS | `cargo test ... pr_y31_f0044_extras_zero` → **1 passed; 0 failed**. F0044 diff: Cherchi 136 tris, Waffle 136 tris, missing=0, extras=0, common=136. well_formed=true, χ=4 on both sides. **Byte parity 136/136.** | PASS |
| G. Sample of 5 corpus | F0030 / F0050 / F0075 / R0014 / R0055 deltas baseline vs HEAD | All 5 are pre-existing Failed/errored cases at PR-Y35 baseline per memory (`yang_f0030_coplanar_root_cause`, banked R-series panics from PR-Y17). yang_fast pass count baseline vs HEAD aggregates the corpus: same 10/157 pass count + same set of 8 errored cases (which includes R0014/R0055 by PR-Y17 banked panic). See §3 for methodology + per-case status table. | PASS |
| H. yang_fast corpus at HEAD | ≥ 10/157 | `YANG_BOOLEAN=1 cargo test ... yang_fast` at HEAD → **Yang fast: 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)**. Equal to PR-Y35 baseline (also 10/157). | PASS |
| I. Kernel lib full suite at HEAD | 1262 pass / 24 fail / 42 ignored; failed-name set identical to PR-Y35 24-name baseline | `cargo test -p kernel --lib` → **1262 passed / 24 failed / 42 ignored** (canary §3 reports 1261; the plan's Phase 6 Gate I correctly predicts 1262 because impl-y35-1 added the new unit test — see §5 banked finding 2). 24 failed test names sorted are byte-identical to canary §3's listed 24 (verified by `diff` against canary memo lines 90-114). `test_subdivision_shared_edge_split_propagation` NOT in failure list. `test_gate_widening_edge2pts_propagates_split_to_sibling` is among the passed set. **Zero new RED.** | PASS |
| J. Paper-grounding audit | Cherchi 2022 §3 quote at lines 315-319; cinolib `triangulation.cpp` does NOT widen gate; PR-Y35.1 is paper-grounded strict superset | Verified, see §4. Both claims confirmed verbatim from sources. | PASS |

**Net result: 10/10 gates PASS.**

---

## §3 Sample-of-5 corpus check (Gate G) — methodology + results

The brief suggests F0030 / F0050 / F0075 / R0014 / R0055. All 5 are documented in memory:
- **F0030 / F0050** — PR-Y17 cohort, coplanar-cap stacking + Stage 6 twin-pairing at non-manifold edges (`yang_f0030_coplanar_root_cause.md`, `yang_debug_queue.md`).
- **F0075** — banked in PR-Y17-COPLANAR REFINEMENT 1-4 as one of 5 cases triggering the L264 panic-on-YANG_BOOLEAN=1.
- **R0014 / R0055** — same PR-Y17 banked panic cohort (R0014/R0046/R0055/R0081/F0075).

**Methodology.** Rather than per-case forensic runs (which would require ~30 min apiece for the
full pipeline and would still produce the same end-state aggregates), I gate on the yang_fast
pass list + errored-set comparison between baseline and HEAD. yang_fast runs all 157 cases under
the same `YANG_BOOLEAN=1` path; the per-case pass/fail/error status for any single case is the
same as a forensic run on that case. The aggregate gate captures all 5 cases plus 152 others.

**Result.** yang_fast at baseline `248dae7` and at HEAD `0d93b8d` both report exactly the same
top-line: **10 passed / 139 failed / 8 errored (33 skipped)**. The 8 errored cases at both
baseline and HEAD include the PR-Y17 panic cohort (R0014/R0055 confirmed in errored set by
log content). F0030, F0050, F0075 remain in the failed set (not errored, not passed) — all 3
were Failed at PR-Y35 baseline, all 3 remain Failed at HEAD. Per-case status unchanged for all
five sampled cases.

**Sample-of-5 table:**

| Case | Baseline (`248dae7`) status | HEAD (`0d93b8d`) status | Δ | Notes |
|---|---|---|---|---|
| F0030 | Failed | Failed | 0 | PR-Y17 coplanar-cap cohort; pre-existing |
| F0050 | Failed | Failed | 0 | PR-Y17 cohort sibling |
| F0075 | Failed/errored | Failed/errored | 0 | PR-Y17 banked panic cohort |
| R0014 | Errored (panic) | Errored (panic) | 0 | PR-Y17 banked panic |
| R0055 | Errored (panic) | Errored (panic) | 0 | PR-Y17 banked panic |

Zero per-case regressions, zero per-case improvements among the sampled 5. The aggregate
10/157 / 139 / 8 numbers being byte-identical at baseline and HEAD entails this byte-identity
across all 5 sampled cases by construction (the only way the aggregate could be identical while
any individual case flipped status would be a perfectly offsetting flip elsewhere — extremely
unlikely on a non-randomized deterministic harness with a single-stage gate widening).

---

## §4 Paper-grounding audit (Gate J) — cinolib comparison

### §4.1 Cherchi 2022 §3 segment-insertion quote (verbatim from `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:315-319`)

> *"Segment Insertion. To make sure that intersection lines are correctly incorporated in the
> output mesh, not only intersection vertices but also intersection segments must be inserted
> (step 3 in Figure 4). Inserting a segment amounts to eliminating, from the current
> tessellation, all triangles that conflict with it, and then re-triangulate the so-generated
> polygonal pocket, while making sure that the wanted segment is part of the new tessellation."*

Confirmed verbatim at the cited lines. The contract is unambiguous: "all triangles that conflict
with it" must be re-triangulated. A triangle whose edge has been split by a sibling's intersection
**conflicts** with the resulting segment — by definition. The pre-PR-Y35.1 gate excluded such
triangles. The PR-Y35.1 widening includes them. The paper directly supports this reading.

### §4.2 cinolib reference — does Cherchi C++ widen the gate?

`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp:145-150`:

```cpp
for(uint t_id = 0; t_id < ts.numTris(); t_id++)
{
    if(g.triangleHasIntersections(t_id) || g.triangleHasCoplanars(t_id))
        tris_to_split.push_back(t_id);
    else
    {
        // triangle without intersections directly goes to the output list
        new_tris.push_back(ts.triVertID(t_id, 0));
        ...
    }
}
```

**Verbatim verified: Cherchi C++ does NOT widen this gate.** It uses exactly
`triangleHasIntersections || triangleHasCoplanars`, the same predicate Waffle had pre-PR-Y35.1.
The canary's §4.3 claim is independently confirmed.

This raises the question: how does Cherchi C++ produce correct shared-edge conformal output on
F0020 (STAGE4 84/84 byte parity per PR-Y35) when its gate is narrower than PR-Y35.1's? The
canary's explanation (§4.3) is that in real corpus geometry, mesh B has multiple triangles each
intersecting multiple A-triangles bulk-wise — every A-triangle adjacent to a split shared edge
gets `triangleHasIntersections` set redundantly through some OTHER cross-mesh pair. The diamond
fixture in `test_subdivision_shared_edge_split_propagation` is the degenerate case Cherchi C++'s
regression suite does not exercise (single B-triangle, single cross-mesh pair).

**Independent corroboration of the canary's redundancy hypothesis:** F0020 STAGE4 at HEAD =
84 pairs (byte parity with Cherchi C++) and Stage B common count = 230 (deterministic on Waffle
side) demonstrate that PR-Y35.1's widened gate does NOT add new triangles to `tris_to_split` in
F0020 — the corpus case's redundant cross-mesh flagging already covers every relevant triangle.
The widening is empirically a NO-OP on F0020/F0044, and a CORRECTNESS-FIX in the diamond
fixture's degenerate cutter case.

### §4.3 PR-Y35.1 is a paper-grounded strict superset of Cherchi C++'s observed behavior

PR-Y35.1's widening:
- **Never excludes** a triangle Cherchi C++ would include (the original `flagged` predicate is preserved as a disjunct).
- **Includes** triangles whose `edge2pts` data exists from a sibling's classification call — these are the "triangles that conflict with" the segment per §3's contract.
- Cannot produce invalid output: adding a triangle with non-empty `edge2pts` to `tris_to_split` only routes it through `triangulate_single_triangle`, which already correctly consults `edge_points_list` at L262-264. Worst-case is extra harmless triangulation work.

**The canary's cinolib parity claim is correct; PR-Y35.1's widening is a paper-justified
deliberate divergence from Cherchi C++'s observed behavior, narrower in scope than what §3's
contract permits.** No banked finding required here.

### §4.4 Was the canary's claim potentially wrong?

The brief asked to flag if the canary's cinolib claim is wrong. I find no error: `triangulation.cpp:145-150`
exactly matches what the canary §4.3 reported. The C++ code is byte-for-byte the narrow predicate.
The canary correctly identified this and correctly framed PR-Y35.1 as a paper-grounded superset.

---

## §5 Banked findings

### §5.1 Cherchi C++ TBB non-determinism in F0020 Stage B (banked, infrastructure)

Two consecutive runs of `f0020_cherchi_diff_baseline` at HEAD produced:
- Run 1: Cherchi 302 tris, Waffle 246 tris, missing=54, extras=0, common=230.
- Run 2: Cherchi 253 tris, Waffle 246 tris, missing=7, extras=0, common=230.

Both runs use `TBB_NUM_THREADS=1` (as the brief instructed). The Cherchi C++ binary's output count
varies (302 vs 253) across reruns with identical input + identical thread count; Waffle's output is
deterministic at 246 tris with 230 in common. This is the PR-Y31 banked Cherchi TBB
non-determinism finding (yang_pr_y31_shipped: "Cherchi non-det survives TBB pin in some F0020
reruns — use missing-count (deterministic) as gate, not extras") manifesting again. PR-Y35.1
does not change Cherchi's behavior; the variance is upstream of Waffle's pipeline. **Recommend
PR-Y36+ adopt min(missing) over N reruns** (or use the STAGE4 pair count gate, which IS
deterministic at 84 in both runs).

### §5.2 Canary memo's kernel lib total (1261) is +1 short of plan-predicted 1262 (banked, memo-cosmetic)

The canary §3 table reports kernel lib post-PR-Y35.1 as **1261 / 24 / 42**. The plan's Phase 6
Gate I (`plans/snappy-humming-hejlsberg.md:108`) predicts **1262 / 24 / 42** (`+2 pass / -1
ignored vs PR-Y35` because impl-y35-1 added the new unit test `test_gate_widening_edge2pts_propagates_split_to_sibling`).
The actual measured value at HEAD is **1262 / 24 / 42** — matches the plan, not the canary's
memo. The canary's number was likely measured before the new unit test was committed, or the
canary's worktree state did not include it (the canary memo §1 notes "production code in
`triangulation.rs` + `exact_mesh.rs`" suggesting the canary tested the widening + ignore-removal
but **not** the optional new unit test — which was test-y35-1's optional add).

This is a memo-side discrepancy, not a production defect. The failed-name 24-set is still
byte-identical to PR-Y35 baseline. Recommend the audit memo (Phase 7) reconcile this single
number when ratifying. The canary's other 11 gates' empirical evidence is uncompromised.

### §5.3 Sample-of-5 was aggregate-only (not per-case forensic), banked methodology note

Gate G's sample-of-5 was performed via yang_fast aggregate comparison (top-line equality:
10/139/8 at both baseline and HEAD) rather than per-case forensic runs. The brief permitted
either methodology; I chose aggregate for time efficiency. Per-case forensic runs would
strengthen the gate slightly (rule out an offsetting-pair scenario), but aggregate gate +
deterministic-pipeline assumption is sufficient for ACCEPT in this case. A future PR with
higher empirical risk should run per-case.

---

## §6 Recommendation — **ACCEPT**

All 10 gates PASS. The load-bearing acceptance gate (Gate B, re-enabled test) PASSES at HEAD
and is independently confirmed RED at the parent commit via non-destructive worktree replay.
The new unit test (Gate C) PASSES. F0020 STAGE4 byte parity (Gate E, 84/84) and F0044 hard
gate (Gate F, 136/136) are preserved. Kernel lib full suite (Gate I) lands at the plan-predicted
1262/24/42 with byte-identical 24-name failed set vs PR-Y35 baseline. yang_fast (Gate H) is
preserved at 10/157. Paper-grounding audit (Gate J) verifies the canary's cinolib claim verbatim
and confirms PR-Y35.1 is a paper-grounded strict superset of Cherchi C++'s observed behavior.

Two banked findings are infrastructure / memo-cosmetic, not production defects: (1) Cherchi TBB
non-det persists (continuation of PR-Y31 banked); (2) canary memo total 1261 is +1 short of the
plan-predicted and measured 1262 — the additional pass is the impl-added new unit test that the
canary memo did not account for. Neither blocks ship.

The first clean single-cycle Yang PR in 10+ cycles (per canary §6) with zero wrong-anchor
ABORTs. Recommendation: **ACCEPT**.

---

## §7 Reproduction artifacts

- `/tmp/y35-1-adv/f0020.log` (run 1) and `/tmp/y35-1-adv/f0020-run2.log` (run 2) — F0020 STAGE4 + Stage B (Gate E)
- `/tmp/y35-1-adv/waffle/inv1/stage4_pairs.txt` (84 lines) and `/tmp/y35-1-adv/waffle/inv0/stage4_pairs.txt` (20 lines) — F0020 STAGE4 byte parity dumps
- `/tmp/y35-1-adv/f0044.log` — F0044 hard gate (Gate F)
- `/tmp/y35-1-adv/yang_fast.log` — yang_fast at HEAD (Gate H)
- `/tmp/y35-1-adv/yang_fast_baseline.log` — yang_fast at parent (Gate G)
- `/tmp/y35-1-adv/kernel_lib.log` — kernel lib full suite (Gate I)
- `/tmp/y35-1-adv/failed_names.txt` — sorted 24-name failure set (byte-identical to canary §3's list)

Cherchi C++ binary at `~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (vanilla `master`).

Live tree HEAD post-adversary: `0d93b8d` on `main`, unmodified except for the pre-existing unstaged
`app/tests/cases/assay/results.json` side-effect (test telemetry, not staged for commit).
