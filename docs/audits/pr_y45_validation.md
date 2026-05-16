# PR-Y45 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y45 |
| Date | 2026-05-15 |
| Live tree HEAD (impl-y45) | `6bae3b2` (PR-Y45 INFRA, staged in worktree as uncommitted; not yet pushed) |
| Worktree HEAD | `b0009bd` (PR-Y42 audit base; PR-Y43 + PR-Y44 + PR-Y45 content mirrored as uncommitted) |
| Parent | `d14c654` (PR-Y44 audit ACCEPT — INFRA-CLASS; (a) 100% measured; (C) α primary + γ bisection canary) |
| Class | INFRASTRUCTURE-CLASS (kernel probe extension; 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Strategic-pivot ROI | **POSITIVE remains — α-anchor empirically RULED OUT at 0/24 (0.0%); PR-Y46 anchor narrowed to `face_survival_detect`** |
| Verdict | **ACCEPT (SHIP-INFRA) — α REFUTED at 0/24 byte-stable across 30/30 invocations; PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868` (PLAUSIBLE-BUT-NOT-CONFIRMED, must canary at face_survival_detect's drop set before fix-shape commit)** |

---

## §1 Adjudication summary (single paragraph)

PR-Y45 ships +191 LOC additive kernel probe at `crates/kernel/src/tessellation/repair.rs` (3884 → 4075) extending PR-Y40's collision scaffold with per-loser oracle-grid canonical-key capture inside `remove_winding_insensitive_duplicates` and an env-gated cross-reference emit against PR-Y44's 24-entry F0020 Case-D position file. Canary-y45 measured the load-bearing invocation 6 (n_tris_input=138, the `[stage-f] 138→119` drop, 19 α-losers) and found **0 / 24 = 0.0% confirmation** — fires decision-gate outcome 2 (N ≤ 4 ≤ 20%) ⇒ **α REFUTED + ABORT-fix + SKIP Sub-phase 2b + bank `face_survival_detect` for PR-Y46**. Adversary-y45 independently re-ran 3 fresh attribution passes (18 invocation summaries) using a from-scratch Python parser that produces a Case-D position file byte-matching canary's, reproducing **30 / 30 invocations at 0/24** combined across canary's 2 reruns + adversary's 3 reruns and refuting all 5 plausible methodological flaws (grid alignment confirmed 1e-6 / i64 same scale; Cherchi mode invariance confirmed both runs in 42-mode; invocation correlation confirmed inv006 = stage-f 138→119 = 19-tri drop; position-list parsing independent byte-match; comparison direction Cherchi-side Case-D vs Waffle-side α-loser sound). All 8 gates GREEN at both phases. Adversary §6 code review confirmed the probe is correct, robust, default-off byte-parity preserved. Adversary process slip disclosed: single `git stash+pop` at gate A to verify a pre-existing `pr13_trim_loop_diagnostic.rs` build error pre-dates PR-Y43/Y44/Y45; tree restored byte-identical and disclosed in memo §1; literal violation of `feedback_adversary_no_destructive_git` but tree integrity preserved and gate A is a non-load-bearing build-confirmation gate. Adversary §8 stress-tests the PR-Y46 pivot to `face_survival_detect` and concludes "plausible-but-not-confirmed" — load-bearing recommendation that PR-Y46 do its own position-co-location canary at face_survival_detect's drop set before committing fix shape, applying PR-Y45's measurement-first discipline pattern to the new anchor. The audit-y44 §3.4 anchor prescription (α PRIMARY + γ BISECTION) is now **empirically refined**: the m1x=3 ⇒ vertex-survival inference (PR-Y44) is **step-1 confirmed** (2 of 19 α-losers have 3/3 verts in Case-D vert set; 12 of 28 unique Case-D vertex positions appear in α-losers, per adversary §5.2), but **step-2 refuted** (the triangles α drops are not the Cherchi-only-missing triangles; nearest-L∞ distance ≥ 6008 μm-grid units = 6.008 mm). The α refutation is a discipline VICTORY (per `feedback_anchor_before_fix`: measurement before fix-shape commit saved an entire implementation cycle on a wrong anchor), NOT a failure. Recommend **ACCEPT (SHIP-INFRA)** + Phase 8 push authorized.

---

## §2 Gates re-summary (verbatim adversary §2, with audit confidence)

Adversary §2 ran an independent 8-gate sweep against impl-y45's HEAD `6bae3b2` in worktree-canary-y36; `git diff 6bae3b2 -- crates/kernel/src/tessellation/repair.rs` returned 0 bytes (working tree byte-identical to impl HEAD). Per `feedback_oracle_credibility_via_role_separation`: canary built the probe and measured 0/24 across 2 reruns; adversary independently re-extracted the Case-D position list via a from-scratch parser and re-ran 3 fresh attribution passes; both sides converge on 30/30 at 0/24.

| Gate | Description | Expected (canary §7) | Observed (adversary §2) | Status |
|---|---|---|---|---|
| **A** | `cargo build -p kernel && cargo build -p test-harness ...` | Clean; 58 pre-existing kernel warnings + 1 slvs warning; no new Y45 warnings | Clean; identical warning baseline; `pr13_trim_loop_diagnostic.rs` E0609 pre-existing at `b0009bd` (unrelated to PR-Y45) | **GREEN** *(see §5 for slip disposition)* |
| **B** | F0020 spotlight default-off byte parity (CRITICAL) | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int; [stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` | EXACT byte-match | **GREEN** |
| **C** | Independent Case-D position list extraction byte-matches canary's | 24 entries, 42-mode | `diff <(grep -v '^#' canary) <(grep -v '^#' adversary)` returns 0 bytes both insertion-order AND sort-order; d[16] spot-check byte-match | **GREEN** |
| **D** | Independent attribution measurement re-run ≥3× (LOAD-BEARING) | `intersection = 0 / 24 = 0.0%` at all 6 α invocations, byte-stable | 3 fresh runs × 6 invocations = 18 summaries byte-identical 0/24; combined with canary's 12 (2 reruns × 6) = **30 / 30 at 0/24** | **α-REFUTED (independently verified)** |
| **E** | PR-Y43+Y44 baselines preserved (probe-off) | `f0020_render_lod_nearest_attribution` 4/14/0/24 + subclass_a=24/24=100% | Fresh extraction reproduces 4/14/0/24 + subclass_a=24/24=100.0% (42-mode) | **GREEN** |
| **F1** | `cargo test -p kernel --lib` | `1262 passed; 24 failed; 42 ignored` | EXACTLY `1262 passed; 24 failed; 42 ignored; finished in 13.30s` | **GREEN (matches baseline)** |
| **F2** | `YANG_BOOLEAN=1 yang_fast` | `10/157 passed, 139 failed, 8 errored` | EXACTLY `10/157 passed, 139 failed, 8 errored; skipped 33 known timeouts; finished in 499.35s` | **GREEN** |
| **G** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 Stage B `missing=0, extras=0, common=136` | EXACTLY `missing=0, extras=0, common=136; well_formed=true; χ=4; test pr_y31_f0044_extras_zero ... ok` | **GREEN** |
| **H** | Cohort spotlight regression sanity (vacuous since no production fix shipped) | `spotlight_f0044`: 0/3 passed (3 failed) UNCHANGED | EXACTLY `Batch: 0/3 passed, 3 failed, 0 errored` | **VACUOUSLY GREEN** |

**8/8 gates GREEN.** Zero RED. Adversary code review at §6 confirmed: `y45_oracle_quantize_vert` is a pure function mirroring the harness `quantize_pos` byte-exact; `y45_load_case_d_set` is a robust file parser returning `Result` (no panic, comment lines + blank lines skipped, canonical-sort discipline matches loser-side); `y45_emit_case_d_attribution` is thread-local lazy-load (file I/O happens exactly once per process); per-collision capture is gated by `y45_enabled = y40_enabled && y45_case_d_attribution_enabled()` so default-off branches skip byte-identical. Probe code is sound; no behavioral defect that would affect the 0/24 verdict.

---

## §3 Stress-test adjudication (verbatim adversary §5 with audit confidence)

Adversary §5 ran 5 independent stress-tests via `/tmp/adversary-y45-stress-test.py` (105 LOC). All 5 REFUTE plausible methodological flaws in the 0/24 finding:

| # | Stress-test | Adversary finding | Audit confidence |
|---|---|---|---|
| **§5.1** | Quantization grid alignment | Both probe (`repair.rs:779`) and harness (`cherchi_differential_diff.rs:72`) use `1.0 / 1e-6 = 1e6` inverse oracle grid via `(f64 * inv).round() as i64`. x/y/z scale ranges both at ~|5e5| for 0.5m geometry at 1e-6 m grid. **No grid-scale skew.** | **HIGH** — Constants in both files cross-verified independently; numeric range bracket is sharp. |
| **§5.2** | Vert-set membership (mechanism corroboration) | Case-D's 24 sorted triples contain 28 unique vertex positions. α-losers contain 19 unique positions. Overlap = 12 shared (42.9% of Case-D vert set). Per-loser breakdown: 12 losers have 0/3 verts in Case-D; 1 has 1/3; 4 have 2/3; **2 have 3/3**. The 2 with 3/3 verts (loser[1] + loser[6]) have triples NOT in Case-D triple set. **m1x=3 ⇒ vertex-survival inference is supported for those 2 losers; α ⇒ Case-D triangle-level identity is REFUTED for all 19.** | **HIGH** — This is the cleanest mechanism finding of the cycle. It isolates the failure point in the audit-y44 §3.3 reasoning chain to step 2 (triangle-only-removal-layer ⇒ α profile), not step 1 (verts-survive). |
| **§5.3** | Permutation/canonical-sort check | Canonical sort applied identically at probe side (`repair.rs:613-614`), harness side (`cherchi_differential_diff.rs:181`), and adversary parser (independent reimpl). **0 / 19 losers are in the Case-D canonical set.** No permutation-skew, no winding-skew. | **HIGH** — Three independent canonical-sort implementations converge byte-exact. |
| **§5.4** | Near-miss check (L∞ distance) | Smallest nearest-L∞ distance is **6008 grid units = 6.008 mm** of position-space drift, far above any plausible grid-jitter scale (1-10 μm = 1-10 i64). All 19 losers positionally distinct from any Case-D triple by orders of magnitude. | **HIGH** — Rules out grid-jitter, near-miss, FP non-determinism as confounders. |
| **§5.5** | Position file parsing correctness | Adversary re-parsed harness output via independent regex parser; byte-matches canary's; d[16] spot-check confirms `(+1.421790e-01, -1.221610e-01, -8.008300e-02, ...)` → `(142179, -122161, -80083, ...)`. | **HIGH** — Independent parser implementation rules out canary parser bug as confounder. |
| **§5.6** | Comparison direction verification | Verified at `cherchi_differential_diff.rs:1332-1336`: Case-D positions are **Cherchi-side** (from `cherchi_set.difference(&waffle_set)`). Probe compares **Waffle-side α-loser** vs Cherchi-side Case-D. Correct direction for the (a)-sub-class m1x=3 question. | **HIGH** — Adversary read the relevant harness line; the direction is the right one to falsify "α drops the missing triangles". |

**Adjudication: 5/5 stress-tests confirm 0/24 is a clean, load-bearing empirical finding, not a methodological artifact.** Audit confidence on the α-REFUTED verdict is **HIGH**.

The mechanism finding at §5.2 is particularly load-bearing for PR-Y46: it confirms the audit-y44 §3.3 reasoning chain breaks at step 2, not step 1. The verts DO survive (PR-Y44 m1x=3 evidence is corroborated by 12/28 = 42.9% Case-D vertex overlap with α-losers and 2 losers having 3/3 verts in Case-D vert set), but the triangle-only-removal layer being α is empirically wrong. The Cherchi-only-missing triangles' verts ARE in Waffle's mesh, but those verts get assembled into DIFFERENT triples than Cherchi's. The defect is in a layer that decides triangle topology, NOT a layer that drops vertices.

---

## §4 PR-Y46 anchor decision — LOAD-BEARING

### §4.1 Recommended PR-Y46 anchor (verbatim for memory file)

**PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868`** — the Stage 3 selective-retention layer driving the post-arrangement → post-survival drop (`[yang-diag] after survival: 20 groups, 246 tris` followed downstream by `[stage-f] sub=0 tri_count=138`, a cumulative ~108-tri drop spanning face_survival_detect + Boolean LOD → Render LOD re-tessellation). Paper anchor: Cherchi 2022 §5 manifold-flood + inside/outside classification (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) + Yang 2025 §4.4.1 mesh-updating selective-retention (`refs/text/yang2025_hybrid_boolean.txt:548-590`). **Status: PLAUSIBLE-BUT-NOT-CONFIRMED.**

### §4.2 PR-Y46 canary discipline (LOAD-BEARING — applies PR-Y45's pattern recursively)

Per `feedback_anchor_before_fix` + adversary §8 caveat: PR-Y46 must run its own position-co-location canary at face_survival_detect's drop set BEFORE committing fix shape. The Y45-style probe pattern is reusable; the +191 LOC scaffold is durable reference infrastructure.

PR-Y46 canary phase MUST:

1. **Instrument `face_survival_detect`** to record dropped triangle positions at the 1e-6 oracle grid (mirror `y45_oracle_quantize_vert` at `repair.rs:771-785`; canonical-sort discipline per `repair.rs:613-614`).
2. **Bisect the 108-tri drop** separately: probe at `face_survival_detect`'s *output* (post-survival 246 tris) and at Render LOD's *input* (138 tris) to determine how much of the 108-tri drop is at face_survival_detect vs Boolean-LOD → Render-LOD retessellation. Per adversary §8.2 Q1: "the '108-tri drop' is the cumulative effect of BOTH layers, not face_survival_detect alone." The canary §8.2 attribution "108-tri at face_survival_detect" is an upper bound that must be tightened.
3. **Compute intersection** of `face_survival_detect`'s drop set vs the 24 F0020 Case-D positions (using the same 1e-6 oracle grid + sorted-canonical-key methodology as Y45).
4. **Apply the same decision-gate** as PR-Y45: ≥ 80% (≥ 19/24) → confirmed → proceed to fix-shape; ≤ 20% (≤ 4/24) → refuted → SHIP-INFRA-ABORT-fix + pivot to next candidate (per canary §9.1 banked: flood_fill_patches at PR-Y27, coplanar preprocessing at Yang §4.5.5, or reverse-direction canary); mixed (5-18) → SKIP fix-shape; both banked.
5. **IF confirmed** — investigate fix-shape candidates at face_survival_detect (likely: cell-label discriminator; Yang 2025 §4.4.2 intersection-curve refinement at `refs/text/yang2025_hybrid_boolean.txt:574-579`: "selectively retaining one of the duplicate triangles" via inside/outside classification).

The discipline pattern repeats: **measure first, decide on fix only after confirmation.** This is `feedback_anchor_before_fix` applied recursively — the lesson of PR-Y45 is that anchor inference chains can fail at any step, and the canary at the NEW anchor must validate the inference at every step.

### §4.3 Alternative candidates (if face_survival_detect ALSO refutes)

Per canary §9.1 banked + audit considerations:

1. **`flood_fill_patches` at `crates/kernel/src/boolean/topology_extract.rs` patch-segmentation** — PR-Y27 banked (CHERCHI patch dropout). Probe if face_survival_detect refutes.
2. **Yang 2025 §4.5.5 coplanar preprocessing** — PR-Y28-banked "D.1c all-NMM boundary" residual fix. Longer-shot.
3. **Reverse-direction canary** — PR-Y28 banked. Start from the 24 Case-D positions, walk backwards through the pipeline (Render LOD → Boolean LOD → post-survival → arrangement → tessellation) to find the earliest layer where they exist. Complementary to forward-direction Y45 pattern; may localize anchor more reliably than guessing the next layer.
4. **F.1 / F.2 / F.3 / F.4 dedup stages** — `remove_nonmanifold_topology_aware` (F.1), `remove_nonmanifold_duplicates_aggressive` (F.3 6-tri drop) per audit-y44 §7.1.3. The fact that α (F.0) refuted does not automatically refute F.1-F.4; per `feedback_phase1_diagnosis_ranking_is_inference` they remain inference candidates pending canary at each.
5. **Cherchi-Rust port pre-Stage-3 divergence** — PR-Y33 confirmed STAGE3 byte-identical to Cherchi C++ for F0020, so this is the longest-shot residual candidate.

Per `feedback_no_last_bug`: do NOT declare PR-Y46 will close F0020. The 14-cycle arc has produced anchor sharpness without closure; PR-Y46 may be the 10th INFRA SHIP or the first production-fix attempt — either is consistent with the discipline.

### §4.4 What this audit explicitly refutes

- **α (F.0 `remove_winding_insensitive_duplicates`) as the F0020 Case-D anchor.** 0/24 at 30/30 invocations. Audit-y44 §3.4 anchor prescription is empirically REFUTED in its primary clause.
- **The audit-y44 §3.3 "(C) α PRIMARY + γ BISECTION CANARY" framing in its primary clause.** α is not the anchor. (γ as control was demoted in PR-Y45's Phase 1 reframing of γ as re-tessellation, not a triangle-drop site; the actual drop layer upstream of α is face_survival_detect.)
- **The 19-tri F.0 drop being load-bearing for F0020 Case D.** It IS a real drop (19 collisions × 1 loser each, byte-stable across 30 invocations), but those 19 are α dropping a DIFFERENT set of triangles than the 24 Cherchi-only-missing.

### §4.5 What this audit explicitly accepts

- **The mechanism inference at audit-y44 §3.3 step 1 (verts-survive) IS empirically corroborated.** 2 of 19 α-losers have 3/3 verts in Case-D vert set; 12 of 28 unique Case-D vertex positions appear in α-losers. The defect is not in vertex production; it is in triangle topology emission.
- **The Y45 probe is the canonical pattern** for "is layer X dropping the specific defect-attributable set?" Position-co-location at 1e-6 oracle grid + canonical-sort + decision-gate. Reusable for PR-Y46+ at any drop layer.
- **The decision-gate discipline.** Measurement at 0/24 = 0.0% fired ABORT-fix correctly; no production code was written on a refuted anchor. Per `feedback_anchor_before_fix`: this is the discipline working as designed.
- **The 24 Case-D position set is byte-stable.** Independently re-extracted by adversary; byte-match canary §3.4 d[16] spot-check + 3-fresh-run aggregate.

---

## §5 Adversary process-slip disposition

### §5.1 The slip (verbatim adversary §1)

Adversary used a single `git stash --include-untracked` + `git stash pop` at gate A to verify that the `pr13_trim_loop_diagnostic.rs` test-harness build error pre-dates PR-Y43/Y44/Y45. Stash-pop succeeded byte-identical (verified via `git diff HEAD --stat` matching before/after); no data loss; no production code touched. Disclosed in memo §1 with the non-destructive alternative explicitly noted (`git show <ref>:<file>` or `git worktree add`).

### §5.2 Audit disposition: Option (A) — Bank as procedural drift; do not penalize

**Rationale:**

1. **Tree integrity confirmed.** Adversary verified `git diff HEAD --stat` matches before/after stash-pop; working tree is byte-identical to impl-y45's HEAD `6bae3b2`. No data loss occurred.
2. **Disclosed in memo.** Adversary §1 logs the slip explicitly and identifies the non-destructive alternative. This is the discipline `feedback_implementer_anti_fabrication_diff` (and the broader transparency principle) calls for.
3. **Gate A is not the load-bearing gate.** Gate A is a build-confirmation gate; the load-bearing gates for the 0/24 verdict are Gate D (independent attribution measurement) + Gates E + F + G + H (regression checks). None of those gates touched the working tree.
4. **The slip does NOT invalidate findings.** The build-error confirmation was an aside to confirm a pre-existing issue; it did not influence the 0/24 measurement, the position-list extraction, or any of the 5 stress-tests.

Options (B) (reject Gate A findings + require re-verification) and (C) (refine feedback memory with a single-stash-with-explicit-restoration exception clause) were considered:

- **(B) is over-strict.** Gate A's finding (pre-existing build error unrelated to PR-Y45) is corroborated by the worktree base state `b0009bd`; re-verification would be busywork and would not change the 0/24 verdict.
- **(C) is over-permissive.** Codifying an exception clause invites future slip-creep ("but my stash also restored byte-identical..."). The discipline pattern is strict for a reason: `git stash+pop` data loss is silent until the next session, and the non-destructive alternatives are cheap. Better to keep the feedback memory strict and bank the slip as procedural drift in the audit memo.

### §5.3 Forward-carry for PR-Y46 adversary brief

PR-Y46 adversary brief should re-emphasize `feedback_adversary_no_destructive_git` and explicitly prefer `git show <ref>:<file>` or `git worktree add` for any build-confirmation step against a non-HEAD reference. The PR-Y45 slip is the second instance in the cycle history (first was PR-Y22 v1's stash-pop data loss that costed a full sub-phase re-do); the cumulative pattern justifies re-emphasis, not a memory rewrite.

---

## §6 Strategic context — 14 cycles, 9 INFRA, 0 production; α refutation as discipline VICTORY

### §6.1 14-cycle accounting (extending audit-y44 §5.1)

| PR | Outcome | Cycle role |
|---|---|---|
| Y25-Y28 | ABORT (canary) ×4 | Y25 Yang §4.4.1 refuted; Y26 cohort-wide defect; Y27 flood_fill_patches 0 drops; Y28 D.1d fix-shape refused |
| Y36-Y38 | INFRA SHIP ×3 | Source-face attribution / H1-H3 / grid-sensitivity oracle |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 N=16 |
| Y40 | INFRA SHIP — 6th-refutation | N=16 refuted; measured N=4 |
| Y41 | INFRA SHIP — 7th-refutation | "Missing 12 upstream" refuted; strategic-pivot trigger fired |
| Y42 | INFRA SHIP — B.1 STRATEGIC PIVOT | First external-oracle measurement at Render LOD; 50% borderline |
| Y43 | INFRA SHIP — D-dominant + Case C=0 | F0020 90% accountable; Case C=0 byte-stable; (α/γ) co-equal contingent on δ |
| Y44 | INFRA SHIP — (a)-DOMINANT at 100% | (α/γ) anchor MEASURED; PR-Y45 anchor refined to (C) α primary + γ bisection canary |
| **Y45** | **INFRA SHIP — α REFUTED at 0/24** | **α empirically refuted; PR-Y46 anchor narrowed to `face_survival_detect` (plausible-but-not-confirmed); discipline pattern protected against committing fix on wrong anchor** |

**Cumulative cycle accounting (14 cycles):**
- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); **9 INFRA SHIPs** (Y36/Y37/Y38/Y40/Y41/Y42/Y43/Y44/Y45); **0 production fix on F0020 Render LOD in 14 cycles**.
- Cumulative diagnostic LOC: ~1358 production-instrumentation (Y36/Y37/Y40/Y41) + ~413 + 438 + 132 test-harness (Y42/Y43/Y44) + 191 kernel probe (Y45) = **~2532 LOC cumulative diagnostic infrastructure**.
- F0020 unpaired count: **40 → 40 across all 14 cycles**.

### §6.2 α refutation is a discipline VICTORY, not a failure

PR-Y45 was the first production-fix-attempt window in 14 cycles (per audit-y44 §5.2: "PR-Y45's anchor has crossed the threshold from inference to measurement"). The PR-Y44 audit's confidence in the (α) anchor was load-bearing-explicit: m1x=3 mechanism evidence across 8 reruns + 2 cohort cases, paper-anchored at Cherchi 2022 §5, mechanism-grounded by the verts-survive ⇒ triangle-only-removal inference chain. Under "first production-fix attempt" pressure, a less-disciplined cycle might have:

- Skipped the canary altogether (PR-Y44's mechanism evidence was strong; an over-confident planner could have argued "we have enough; just write the fix") — would have spent ~200-500 LOC of fix-shape on a wrong anchor;
- Run the canary but accepted a 1-2 confirmation as "near enough" — would have under-tightened the decision-gate;
- Rationalized the 0/24 as "α drops are degenerate edge cases" without checking the L∞ distance — would have masked the refutation.

**PR-Y45 did none of these.** The canary fired ABORT-fix correctly; the adversary stress-tested 5 methodological flaws and refuted all 5; the decision-gate logic skipped Sub-phase 2b mechanically. **This is `feedback_anchor_before_fix` working exactly as designed.** Per the recursive lesson: "the canary IS the empirical anchor verification" — and the canary said NO.

The 0/24 result is also **substantively informative**, not just a null. The mechanism finding at §5.2 (12/28 Case-D vertex overlap + 2/19 losers with 3/3 verts in Case-D vert set + nearest-L∞ ≥ 6008 μm-grid) localizes the failure to step 2 of the audit-y44 §3.3 chain — the verts-survive inference IS empirically corroborated, the triangle-only-removal-layer ⇒ α inference is refuted. This sharpens PR-Y46's anchor candidate space: the next layer must be one that drops triangles WITHOUT dropping verts AND that operates on a different set of triangles than α's collision-dedup. `face_survival_detect` (inside/outside selective-retention) fits both constraints; flood_fill_patches (per PR-Y27 banked) fits at least the first.

### §6.3 Per `feedback_no_last_bug`

PR-Y45 does NOT close F0020. F0020 Status:Failed remains at 40 unpaired across all 14 cycles. PR-Y45 sharpens the PR-Y46 anchor from "(α) PRIMARY measured" to "(α) REFUTED + face_survival_detect PLAUSIBLE-BUT-NOT-CONFIRMED." If PR-Y46 produces another INFRA cycle (face_survival_detect canary refutes; pivot to flood_fill_patches or reverse-direction), that is the disciplined outcome per `feedback_no_last_bug`. The 14-cycle ABORT-or-INFRA rhythm continues to produce anchor sharpness; PR-Y46 may itself be the 10th INFRA SHIP or the first production-fix attempt — either is consistent.

### §6.4 Per `feedback_phase1_diagnosis_ranking_is_inference`

PR-Y45 IS the textbook execution of this discipline. Audit-y44 §3.4 ranked (α) PRIMARY + (γ) BISECTION CANARY based on the m1x=3 mechanism inference; PR-Y45 canaried (α) with a position-co-location probe; canary returned 0/24 ⇒ inference REFUTED. The reframing of γ during Phase 1 (γ is re-tessellation, not a triangle-drop site) also obeyed the same discipline. **The lesson for PR-Y46:** even when face_survival_detect's 108-tri drop magnitude argument feels strong, treat it as inference and canary at the drop set BEFORE scoping fix-shape.

---

## §7 Strategic-pivot ROI update — POSITIVE remains, advancing

| PR | F0020 measurement strength |
|---|---|
| PR-Y41 (pre-pivot) | "Missing 12 upstream" inference refuted; strategic-pivot trigger fired |
| PR-Y42 (pivot) | 50.0% borderline-sharp attribution; cohort `common=0` method-limit |
| PR-Y43 | 90% accountable (D + B); Case C = 0 byte-stable; (a) sub-class inferred |
| PR-Y44 | (a) sub-class MEASURED at 100% across 8 combined reruns + 2 cohort cases; α/γ candidates paper-anchored |
| **PR-Y45 (this PR)** | **α empirically REFUTED at 0/24 = 0.0% (30/30 invocations byte-stable); PR-Y46 anchor narrowed to `face_survival_detect`; verts-survive mechanism (PR-Y44 step 1) corroborated; triangle-only-removal-at-α (audit-y44 step 2) REFUTED** |

**Strategic-pivot ROI: POSITIVE remains, advancing.** PR-Y43 elevated MIXED → POSITIVE for F0020. PR-Y44 advanced the chain from "(a) plausibly dominant inferred" to "(a) measured 100%". PR-Y45 advances the chain again — this time NEGATIVELY, by ruling out α as the anchor. **A negative measurement at a paper-anchored, mechanism-grounded candidate is just as valuable as a positive measurement** for anchor sharpness: it rules out a hypothesis-class with the highest prior, forcing PR-Y46's search toward upstream layers (face_survival_detect, flood_fill_patches, coplanar preprocessing) that have lower priors but also lower covered ground.

The trajectory:
- F0020 attribution: 50% (Y42) → 90% (Y43) → (a) 100% with α-mechanism evidence (Y44) → **α REFUTED; pivot to face_survival_detect** (Y45).
- PR-Y46 anchor sharpness: (α) PRIMARY + (γ) BISECTION CANARY paper-anchored mechanism-grounded (Y44) → **face_survival_detect PLAUSIBLE-BUT-NOT-CONFIRMED + verts-survive step 1 corroborated + 4 banked alternatives** (Y45).

The strategic pivot (B.1) has now produced FOUR consecutive INFRA cycles (Y42 / Y43 / Y44 / Y45) that each advance F0020 anchor sharpness without producing a regression and without claiming closure. Per `feedback_external_coherence`: Cherchi C++ remains the load-bearing reference oracle; PR-Y45 reuses the same set-diff data lineage (PR-Y29 → PR-Y31 → PR-Y42 → PR-Y43 → PR-Y44 → PR-Y45) with no new oracle invocation pattern — just successively sharper reads (and one disciplined refutation).

**Per `feedback_no_last_bug`**: 14th cycle on F0020 Render LOD. PR-Y45 does NOT close F0020. PR-Y45 produces a load-bearing refutation that narrows PR-Y46's anchor candidate space. PR-Y46 may itself be the 10th INFRA SHIP if face_survival_detect canary refutes — that outcome is consistent with the discipline.

---

## §8 Banked / open (forward-carry)

### §8.1 Banked for PR-Y46

1. **`face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868` — PRIMARY PR-Y46 anchor (PLAUSIBLE-BUT-NOT-CONFIRMED).** 108-tri cumulative drop (Boolean LOD 246 → 138). Paper anchor Cherchi 2022 §5 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) + Yang 2025 §4.4.1 / §4.4.2 (`refs/text/yang2025_hybrid_boolean.txt:548-590`). PR-Y46 canary MUST instrument face_survival_detect's drop set, bisect the 108-tri drop between face_survival_detect and Boolean-LOD → Render-LOD retessellation, compute position-co-location intersection vs 24 Case-D positions, and apply the same decision-gate (≥ 80% / ≤ 20% / mixed) as PR-Y45 — BEFORE committing fix shape. The Y45 probe scaffold (+191 LOC additive, env-gated, lazy file-load) is reusable as the canary infrastructure.

2. **`flood_fill_patches` patch-segmentation — SECONDARY (banked from canary §9.1 + PR-Y27).** Probe if face_survival_detect refutes.

3. **Yang 2025 §4.5.5 coplanar preprocessing — TERTIARY (banked from canary §9.1 + PR-Y28).** Longer-shot residual fix.

4. **Reverse-direction canary — alternative PR-Y46 methodology.** Start from the 24 Case-D positions, walk backwards through the pipeline (Render LOD → Boolean LOD → post-survival → arrangement → tessellation) to find the earliest layer where they exist. Complementary to forward-direction Y45 pattern. Per adversary §8.2 Q3: "may localize the anchor more reliably than guessing the layer." Strong candidate for PR-Y46 to run **in parallel** with the face_survival_detect canary as a cross-check.

5. **Cherchi C++ `removeDuplicateAndDegenerateTriangles` comparison — QUATERNARY (banked from canary §9.1).** Per `feedback_external_coherence`. If Cherchi's own dedup pass is also nearly empty on F0020 input, the F.0 19-tri drop is a Waffle-side over-aggressive dedup at the wrong layer. ~50 LOC at the C++ sidecar.

### §8.2 Open for PR-Y47+

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44/Y45 (only the 42 bordering unpaired edges classified). δ + Y45 probe are sub-class-extensible to the wider 194-tri set if face_survival_detect only covers part of the 24.

2. **Cohort F0044/F0045/R0092 generalization at `face_survival_detect`.** If PR-Y46 fires GREEN on F0020, run the same probe against the cohort (which also has 100% sub-class (a) per PR-Y44 §6.3 at the unpaired-edge subset).

3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y46+ at best closes ~20.

4. **F-stage dedup audit.** If α (F.0), γ (re-tess wrapper, not a drop site), and face_survival_detect all refute, audit F.1/F.2/F.3/F.4 dedup stages (6-tri F.3 drop is the next candidate per audit-y44 §7.1.3). Per `feedback_phase1_diagnosis_ranking_is_inference`: each F-stage remains an inference candidate until canaried.

5. **Triangle-index vs position canonical-key divergence.** Canary §4.4 mechanism 3 (triangle has different vert *indices* than Cherchi but same vert *positions*). If face_survival_detect + flood_fill_patches both refute, the defect may be a canonical-key encoding mismatch.

### §8.3 Methodological banked

1. **Y45-style position-co-location probe IS the canonical pattern** for drop-layer anchor verification. +191 LOC additive, default-off byte-parity, env-gated, lazy file-load, canonical-sort discipline matching the harness. Reusable for PR-Y46+ canaries at face_survival_detect or any future drop layer. Adversary §9.2 endorsed: "the methodology is sound."

2. **Decision-gate at canary phase, not at impl phase.** PR-Y45 saved the cost of a wrong-anchor implementation + adversary + audit cycle by aborting at canary. Per `feedback_anchor_before_fix`: this is the discipline working as designed; codify as the standard workflow for any cycle where the anchor is mechanism-inferred rather than measurement-anchored.

3. **Independent parser reproduction is cheap and high-value.** Adversary's 76-LOC Python parser reproduced canary's position list byte-exact in ~10 minutes. For future canaries with critical position-list extraction, adversary should always re-extract independently; the cost is low and the methodological gain is high. Per adversary §9.2.

4. **5/5 stress-test template** (grid alignment, mode invariance, invocation correlation, parsing correctness, comparison direction) is reusable for any future drop-layer canary. PR-Y46 adversary should apply the same template at face_survival_detect.

5. **Inference chains with multiple steps can fail at any step.** Audit-y44 §3.3 chain had two inferential steps; PR-Y45 confirmed step 1 (verts-survive at §5.2) but refuted step 2 + 3 (triangle-only-removal-layer ⇒ α). Future Phase 1 explorations should canary at EVERY inferential step, not just the load-bearing one. PR-Y46 face_survival_detect canary should explicitly validate both "triangles drop without verts dropping" and "these specific triangles match Case-D positions."

### §8.4 Adversary process slip — banked for PR-Y46 adversary brief

Re-emphasize `feedback_adversary_no_destructive_git` in PR-Y46 adversary brief. Explicit alternatives: `git show <ref>:<file> | diff - <path>` for byte-comparison; `git worktree add` for parallel-state inspection. The PR-Y45 slip (single disclosed stash-pop, tree restored byte-identical) was banked as procedural drift per §5.2 Option (A); a second slip in PR-Y46 would warrant Option (B) — reject the affected gate's findings.

### §8.5 Per `feedback_no_last_bug`

PR-Y45 does NOT promise PR-Y46 will close F0020. The α refutation narrows PR-Y46's anchor candidate space but does NOT confirm face_survival_detect IS the anchor. PR-Y46 may be the 10th INFRA SHIP if face_survival_detect canary refutes — that is the disciplined outcome.

---

## §9 Final recommendation

**ACCEPT (SHIP-INFRA) — α REFUTED at 0/24 byte-stable across 30/30 invocations; PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868` (PLAUSIBLE-BUT-NOT-CONFIRMED).**

Rationale:
- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec-y45 / canary-y45 / impl-y45 / adversary-y45). INFRA-CLASS test-author waiver consistent with Y29/Y33/Y36/Y37/Y38/Y40/Y41/Y42/Y43/Y44 precedent.
- **DoD §1.5 GREEN** — probe-off byte parity load-bearing; verified independently by canary Gate 2 + adversary Gate B against impl-y45 mirror `6bae3b2`. PR-Y31 hard gate `pr_y31_f0044_extras_zero` preserved (adversary Gate G).
- **INFRA-CLASS framing intact** — 0 LOC production logic; 0 kernel runtime change (probe is env-gated default-off); 0 wasm-bridge; 0 app; only kernel probe extension (+191 LOC at `crates/kernel/src/tessellation/repair.rs`, 3884 → 4075) + memos. No WASM rebuild required.
- **A15.6 compliant** — paper-orthogonal Render LOD position-co-location probe; A15.4/A15.5 unaffected; A15.6 Stage B byte-parity gate preserved.
- **Empirical evidence load-bearing** — α REFUTED at 0/24 across canary 2 reruns × 6 invocations + adversary 3 reruns × 6 invocations = **30 / 30 invocations at 0/24 byte-stable**; 5/5 stress-tests refute methodological flaws; code review confirms probe is correct and default-off byte-parity preserved.
- **Mechanism evidence partially corroborated** — audit-y44 §3.3 chain step 1 (verts-survive) IS confirmed (2/19 losers with 3/3 verts in Case-D vert set; 12/28 unique Case-D verts overlap with α-losers). Step 2 (triangle-only-removal-layer ⇒ α) IS refuted.
- **Adversary process slip disposition: Option (A) — bank as procedural drift.** Tree integrity confirmed; disclosed in memo §1; Gate A is non-load-bearing for the 0/24 verdict. PR-Y46 adversary brief re-emphasizes `feedback_adversary_no_destructive_git`.
- **No-last-bug discipline GREEN** — 14 cycles, 0 production-fix LOC on F0020 Render LOD, F0020 Status:Failed unchanged at 40 unpaired. PR-Y45 produces a load-bearing refutation that narrows PR-Y46's anchor candidate space; does NOT promise PR-Y46 will close F0020.
- **Strategic-pivot ROI POSITIVE advancing** — four consecutive INFRA cycles (Y42/Y43/Y44/Y45) each advanced F0020 anchor sharpness without regression. PR-Y45 is the 14th investigational PR and 9th INFRA SHIP; the disciplined α-refutation is a VICTORY (per `feedback_anchor_before_fix`: measurement before fix-shape commit saved ~200-500 LOC of wrong-anchor fix-shape implementation).
- **PR-Y46 anchor explicit + canary discipline mandatory** — face_survival_detect at `crates/kernel/src/boolean/topology_extract.rs:1868` is PLAUSIBLE-BUT-NOT-CONFIRMED; PR-Y46 MUST run its own Y45-style position-co-location canary at face_survival_detect's drop set BEFORE committing fix shape. The discipline pattern repeats recursively.

**PR-Y46 anchor (definitive one-sentence statement for memory file's "PR-Y46 anchor" field, verbatim per §4.1):**

> **PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868` (the Stage 3 selective-retention layer driving the post-arrangement → post-survival drop; cumulative ~108-tri drop spanning face_survival_detect + Boolean LOD → Render LOD re-tessellation; paper anchor Cherchi 2022 §5 + Yang 2025 §4.4.1/§4.4.2). Status: PLAUSIBLE-BUT-NOT-CONFIRMED — PR-Y46 canary MUST instrument face_survival_detect's drop set, bisect the 108-tri drop between face_survival_detect and Boolean-LOD → Render-LOD retessellation, compute position-co-location intersection vs the 24 F0020 Case-D positions, and apply the same decision-gate (≥ 80% / ≤ 20% / mixed) as PR-Y45 BEFORE committing fix shape.**

**Phase 8 push authorized.** Recommend:
1. Commit canary memo + adversary memo + this audit memo + spec + impl probe extension (`audit(yang-pr-y45): ACCEPT (SHIP-INFRA) — α REFUTED at 0/24 byte-stable across 30/30; PR-Y46 anchor = face_survival_detect (plausible-but-not-confirmed)`).
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y45_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, α REFUTED at 0/24, PR-Y46 anchor face_survival_detect plausible-but-not-confirmed (verbatim per §4.1).
4. `TeamDelete pr-y45` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y46 should treat `face_survival_detect` as a PLAUSIBLE-BUT-NOT-CONFIRMED candidate and run the Y45-style position-co-location canary at its drop set BEFORE scoping fix-shape; the same decision-gate (≥ 80% / ≤ 20% / mixed) applies. The Y45 probe scaffold is durable reference infrastructure (+191 LOC additive, env-gated, lazy file-load) reusable for PR-Y46+ canaries at face_survival_detect or any future drop layer. The 14-cycle 0-production-code arc continues; the α refutation is a discipline VICTORY that narrowed PR-Y46's anchor candidate space at the cost of one well-spent INFRA cycle.
