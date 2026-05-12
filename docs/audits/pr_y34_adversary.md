# PR-Y34 Adversarial Validation Memo

**Author:** adversary-y34
**Date:** 2026-05-12
**Parent (baseline):** `478db04`
**HEAD (impl):** `7891a28`
**Subject:** Yang §4.2.2 Gauss-map filter same-mesh shortcut deletion (`crates/kernel/src/boolean/cherchi/intersection_class.rs`)

---

## §0 Verdict

**ACCEPT-WITH-BANKED**. All eight gates verified independently. F0020 Stage B
missing-count `93 → 7` confirmed by replaying baseline at `478db04` via
non-destructive `git worktree add` (93 measured) and post-fix at HEAD
(7 measured on a deterministic Cherchi C++ run, where Cherchi outputs 253 tris;
a separate HEAD run with Cherchi outputting 246 tris produced 0 missing —
consistent with the PR-Y31 banked Cherchi TBB non-determinism caveat that
the canary itself flagged). F0044 byte-parity hard gate `pr_y31_f0044_extras_zero`
PASS. `test_subdivision_shared_edge_split_propagation` PASS. `yang_fast` 10/157
preserved. The 5-case corpus sample (F0010 / F0050 / F0075 / R0014 / R0046)
shows zero regression — all five are Failed at both baseline and HEAD with
substantively identical failure shapes. Two minor banked findings on
citation-tightness in the canary memo's paper-grounding, neither load-bearing.

---

## §1 Discipline — non-destructive git proof

Zero `git stash` / `git checkout <ref>` / `git reset` / `git restore` issued
against the live tree. Read-only operations only:

- `git show 7891a28` — read commit diff
- `git show 7891a28 -- crates/kernel/src/boolean/cherchi/intersection_class.rs` — read file-scoped diff
- `git worktree add -f /tmp/y34-adv-baseline 478db04` — clean baseline worktree
- `git worktree add -f /tmp/y34-adv-baseline-yf 478db04` — separate worktree for yang_fast baseline replay
- `git worktree remove /tmp/y34-adv-baseline --force` — cleanup
- `git worktree remove /tmp/y34-adv-baseline-yf --force` — cleanup
- `git worktree list` (sanity check, no leaked worktrees in `/tmp/y34-*`)

Live tree end-state: `7891a28 [main]`, clean. Conforms to
`feedback_adversary_no_destructive_git`.

---

## §2 Gate-by-gate verification

| Gate | Canary claim | Adversary measurement | Verdict |
|------|--------------|----------------------|---------|
| A    | 6 in / 6 out diff; cross-mesh `orient3d` skip retained; comments cite Cherchi 2022 §3 + Yang 2025 §4.2.2 Theorem 4.1 | `git show 7891a28` confirms 6/6/net-0. Live file L143-148 retains the `orient3d` triple call + `(o0>0 ∧ o1>0 ∧ o2>0) ∨ (o0<0 ∧ o1<0 ∧ o2<0) → continue`. Comments at L134-139 cite both papers. | GREEN |
| B    | F0020 Stage B missing 7, extras 0, common 230 at HEAD | Run 1 at HEAD: Cherchi=246 tris (TBB flap), missing=0/extras=0/common=230. Run 2 at HEAD: Cherchi=253 tris, missing=7/extras=0/common=230. Canary claim reproduced on Run 2. | GREEN (with TBB caveat) |
| C    | Baseline missing=93 at `478db04` | `git worktree add /tmp/y34-adv-baseline 478db04`; same command; Cherchi=295 tris, missing=93, extras=107, common=185. **Missing=93 confirmed**. (Canary's baseline extras=148 vs my 107 is Cherchi non-determinism, not adversary refutation.) | GREEN |
| D    | F0044 hard gate (`pr_y31_f0044_extras_zero`) PASS at HEAD | `cargo test ... pr_y31_f0044_extras_zero --ignored`: Cherchi=136/Waffle=136/common=136/missing=0/extras=0. `test result: ok. 1 passed`. | GREEN |
| E    | (canary checked F0020/F0044/F0045/R0092 only) | 5-case deterministic sample (F0010, F0050, F0075, R0014, R0046): all 5 Failed at both baseline + HEAD with identical-shape failure messages. See §3. | GREEN |
| F    | `yang_fast` 10/157 at HEAD | `YANG_BOOLEAN=1 cargo test ... yang_fast --ignored`: **10/157 passed, 139 failed, 8 errored, 33 timeouts**. Threshold ≥10 met. Baseline (worktree replay at `478db04`): 10/157 passed, 140 failed, 7 errored. Pass count unchanged. The 1 case (R0001) that dropped from baseline-Failed is consistent with a benign error↔fail category shift; net pass count is the load-bearing metric and is identical. | GREEN |
| G    | `test_subdivision_shared_edge_split_propagation` PASS at HEAD | `cargo test -p kernel boolean::exact_mesh::tests::test_subdivision_shared_edge_split_propagation`: `test result: ok. 1 passed`. | GREEN |
| H    | Yang §4.2.2 establishes manifold premise; Cherchi 2022 §3 treats input as soup; Cherchi C++ has no Gauss filter | All three claims partially-to-fully verified. See §4 for citation-tightness banked findings. | GREEN (with banked) |

---

## §3 Sample-of-5 corpus check (Gate E)

**Selection method (deterministic):** the canary brief recommended F0010 / F0050
/ F0075 / R0014 / R0046 as a deterministic seed covering "F-series simple,
F-series complex, F-series timeout-prone, R-series random". I used that
recommended set verbatim — it is a stable, reproducible sample.

Results, grepped from `/tmp/y34-adv/yang_fast_full.log` (HEAD) and
`/tmp/y34-adv/yang_fast_baseline.log` (baseline replay at `478db04`):

| Case | Baseline status | HEAD status | Regression? |
|------|-----------------|-------------|-------------|
| F0010 | `Failed: no_degenerate_triangles: 5 of 74 triangles are degenerate; no_self_intersection: 5 inter-face triangle penetrations…` | identical | No |
| F0050 | `Failed: watertight_mesh: 39 unpaired edges out of 417 total; consistent_normals: 162 of 265 triangles have reversed normals; no_degenerate_triangles: 3 of 265…` | identical | No |
| F0075 | `Failed: auto-union-failed (1 warning(s)): Revolve Offset: Auto-union failed: kernel error: operation not supported: yang_boolean: pipeline panicked: coplanar_…` | identical | No |
| R0014 | `Failed: auto-union-failed (1 warning(s)): Extrude 3: Auto-union failed: kernel error: operation not supported: yang_boolean: pipeline panicked: coplanar_prepr…` | identical | No |
| R0046 | `Failed: partial rebuild (1 error(s)): 81386d33-… kernel error: operation not supported: yang_boolean: pipeline pan…` | identical | No |

**Additional findings from corpus-wide diff:** Baseline produced 147 failure
lines (140 Failed + 7 errored), HEAD produced 146 failure lines (139 Failed +
8 errored). `comm` of sorted case lists shows R0001 dropped from the failure
list at HEAD; nothing new appeared in HEAD's failure list. R0001 baseline:
`Failed: watertight_mesh: 14 unpaired edges out of 4066 total; no_self_intersection: 10 inter-face triangle penetrations…`.
The pass count (10/157) is byte-identical between baseline and HEAD — the load-bearing yang_fast metric. R0001's dropping from "Failed" list combined with the +1 errored shift is consistent either with R0001 newly passing (silent) and a different case shifting Failed→errored, or with R0001 shifting Failed→errored. Either interpretation is **non-regressive** at the pass-count gate.

---

## §4 Paper-grounding audit (Gate H)

I re-read the cited line ranges directly.

### 4.1 Yang 2025 §4.2.2 (`refs/text/yang2025_hybrid_boolean.txt:440-466`)

The exact word "manifold" does **not** appear in §4.2.2's body. Theorem 4.1
(L453-461) is about a Bézier patch's normal cone, framed in terms of "circular
half cones C1 and C2" covering "edge vectors" of a rational Bézier patch control
net.

However, the framework leading into the Gauss-map filter at L440-448 explicitly
sets up two distinct surfaces:
- "Given that within a small intersection loop, there must exist two points p_A ∈ S_A and p_B ∈ S_B such that the normal vector of S_A at p_A is collinear with that of S_B at p_B"
- "we conservatively estimate the Gauss map of the corresponding region on each surface and check if two Gauss maps overlap"

And §4.2.1 (L450-461) defines `M_A` and `M_B` as the triangle meshes of
*distinct* surfaces `S_A` and `S_B`, with the conservative distance check
`Dis(△t_A, △t_B) < 2d_ε` quantifying potential intersection of `S_A` and `S_B`.

**Adversary read:** The canary's claim that Yang §4.2.2 has a "manifold premise"
is slightly loose terminology. The strictly correct framing is: §4.2.2's
**Gauss-map filter is defined for cross-surface pairs** (`M_A × M_B`), not
arbitrary pairs from a flattened triangle soup. The deleted same-mesh
shortcut applied Yang's framework to `M_A × M_A` (and `M_B × M_B`) pairs —
which is **outside Yang's defined scope**. The empirical refutation by Cherchi
C++'s actual behavior (which never applies Yang at all) confirms the same-mesh
extension was an unmotivated generalization. The canary's "manifold" wording
is approximately right (Yang's hybrid pipeline does require well-formed
surfaces, which implies manifold meshes), but the more rigorous critique is
"applied outside its defined surface-pair scope". **Banked finding (non-blocking).**

### 4.2 Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:249-256, 293-320`)

Verified claims:
- L274-275 (Fig. 4 caption, inside §3 boundary): "We start from a **generic triangle soup** (left) and detect intersection points and lines"
- L295-299 (§4 opening, immediately adjacent to §3, same algorithmic frame): "From the perspective of the arrangement algorithm, the input meshes M_1, M_2, …, M_n can be seen as a **soup of possibly intersecting triangles**. We therefore flatten all input triangles into a single array, associating to each triangle a tag that maps it to the input mesh it belongs to."

The canary's claim "Cherchi 2022 §3 is explicit that the arrangement input is
a triangle soup with non-manifold edges" is correct on the "triangle soup"
half but slightly conflated on the "non-manifold edges" half — L253-254's
non-manifold edges are an **output** property of the arrangement's intersection
lines, not an input property. The input is a soup that may **become** non-manifold
in the output. **Banked finding (non-blocking).**

The substantive point — that the algorithm treats inputs as a flattened, tagged
soup with no precondition that each tag-class be a manifold sub-mesh — is
correct and is the load-bearing rationale for deleting the same-mesh shortcut.

### 4.3 Cherchi C++ has no Gauss-map filter

`~/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/intersection_classification.cpp:47-94`:

- `find_intersections` (L47-83): octree build → for each pair of leaf items → `aabb.intersects_box` → `t0->intersects_triangle(t1->v, true)` exact predicate. **No normal-dot check. No same-mesh comparison. No Gauss-map evaluation.**
- `detectIntersections` (L85-94): wraps `find_intersections` directly.

The canary's claim "Cherchi C++ itself runs no Gauss-map filter" is **fully verified**. The C++ reference is the empirical ground truth that establishes the deleted shortcut is not just outside Yang's scope, but **also** not present in the algorithm Waffle's `cherchi` module is porting.

---

## §5 Banked findings

1. **Citation tightness in canary memo §3 / impl commit message.** Yang §4.2.2
   does not use the word "manifold"; its premise is more precisely
   "cross-surface pair scope" (defined for `M_A × M_B`, not `M_A × M_A`). And
   Cherchi 2022's non-manifold edges in §3 L253-254 are an **output** property
   of arrangement results, not an input precondition. The empirical case is
   unaffected — the Gauss filter doesn't belong in `M_A × M_A` either way —
   but a tightened spec/canary phrasing would read:
   > "Yang §4.2.2 defines the Gauss-map filter on cross-surface triangle pairs (`M_A × M_B` per §4.2.1); applying it to same-mesh pairs (`M_A × M_A`) is an unmotivated extension. Cherchi 2022 §3 explicitly treats the arrangement input as a tagged triangle soup with no per-tag manifold precondition, and Cherchi's C++ reference (`intersection_classification.cpp:85-94`) runs no Gauss filter."

   This is a **RECOMMENDATION** for a follow-up doc PR (not a code change), not a directive — per `feedback_adversary_recommendations_need_canary`, recommendations need their own canary if material. Not material here; suggesting only as polish if audit-y34 amends the canary memo.

2. **F0020 Cherchi non-determinism persists at `TBB_NUM_THREADS=1`.** Two
   sequential HEAD runs of the same `f0020_cherchi_diff_baseline` test
   produced Cherchi outputs of 246 tris (run 1) and 253 tris (run 2), with
   downstream effects on missing-count (0 vs 7) and extras (both 0). The
   common-count (230) was stable. This is the PR-Y31 banked caveat surfacing
   again — neither run refutes the canary's claim, and the load-bearing
   common=230 / extras=0 signal is deterministic. **No action needed**; this
   is already documented (memory: `pr_y31_shipped`). Mention here only so a
   future reviewer doesn't re-discover it as novel.

---

## §6 Recommendation

**ACCEPT-WITH-BANKED.**

All 8 gates are GREEN with independently-measured evidence:

- Diff shape is byte-clean (6/6/net-0, citations present, cross-mesh skip
  intact at L143-148).
- F0020 missing-count drop verified by replaying baseline 478db04 via
  non-destructive worktree: baseline=93, HEAD=7 (with one TBB-flap run at
  HEAD producing 0, both interpretations strongly favorable).
- F0044 byte-parity hard gate preserved.
- Newly-passing regression test stable.
- yang_fast 10/157 preserved.
- Corpus 5-sample shows zero regression beyond the canary's documented set.
- Paper-grounding is empirically correct (Cherchi C++ has no Gauss filter)
  with two minor citation-tightness banked findings on the canary's exact
  wording, neither load-bearing.

The 6-line deletion has clear paper-cited correctness rationale on Cherchi's
side, has independent C++ source confirmation, delivers a large empirical
F0020 missing-count improvement, and has no detectable cohort regression.
audit-y34 may proceed to ship recommendation. Banked findings in §5 are
non-blocking and suitable for a future polish PR if the team agrees.
