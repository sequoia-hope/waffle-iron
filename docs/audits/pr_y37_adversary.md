# PR-Y37 Adversarial Validation — ACCEPT

**Author:** adversary-y37
**Date:** 2026-05-13
**Live tree HEAD under audit:** `3e35f8c` (PR-Y37 impl, NOT pushed)
**Parent baseline:** `1ad58ce` (PR-Y36 audit ACCEPT)
**Class:** INFRASTRUCTURE-ONLY (probe extension + memo + spec; zero production logic change)

---

## §0 Verdict

**ACCEPT.** All ten gates (A–J) GREEN. The PR-Y37 probe extension is sound, default-off byte-identical, and the H1/H2/H3 attribution data is independently reproducible to the digit. F0020 inv#6 attribution reproduced exactly (D.1a=9, D.1d=8, OtherH2=3, OtherH3=19, total=39). Cohort attribution reproduced exactly (F0044 12/12 H3, F0045 38/38 H3, R0092 31/43 H3 + 12/43 H2). The cross-cohort prediction refutation is a genuine empirical finding, not a measurement artifact. The canary memo and spec correctly refrain from "closes Yang"/"last bug" framing. Zero destructive git operations were performed during validation.

---

## §1 Discipline — Non-destructive git proof

All baseline inspection used `git worktree add 1ad58ce → /tmp/y37-adv-baseline` (read-only) followed by `git worktree remove --force` cleanup. No `git stash`, `git checkout`, `git reset`, or any other destructive operation touched the live tree. `git worktree list` after audit shows live tree at `3e35f8c [main]` unchanged.

Compliance with `feedback_adversary_no_destructive_git`: **CONFIRMED**.

---

## §2 Gate verification

| Gate | Check | Expected | Observed | Result |
|------|-------|----------|----------|--------|
| A | Diff shape | 4 files, results.json not staged, tessellation +282/-18 | 4 files committed; tessellation/mod.rs +282/-18; pr_y37_canary.md +460; spec +237; wasm bundle updated | PASS |
| B | Probe-off byte parity | F0020 Status:Failed, 40 unpaired (39 boundary + 1 NMM), 8 degenerate, 10 self-int (identical to PR-Y36) | Status:Failed, 40 unpaired (39 boundary + 1 NMM), 8 degenerate, 10 self-int | PASS |
| C | Probe-on H1/H2/H3 columns + classifications fire | New TSV header with H1/H2/H3-named columns; values include OtherH1/OtherH2/OtherH3 | Header includes `grid_aligned_count, grid_aligned_pct, nmm_asym_count, nmm_asym_pct`; values include `OtherH2, OtherH3, D1a, D1d` | PASS |
| D | F0020 inv#6 attribution: D.1a=9, D.1d=8, OtherH2=3, OtherH3=19, total=39 | D.1a=9, D.1d=8, OtherH2=3, OtherH3=19, total 39 | exact match (9+8+3+19=39) | PASS |
| E | Cohort: F0044 12/12 H3, F0045 38/38 H3, R0092 cohort attribution captured | F0044 12 H3; F0045 38 H3; R0092 12 H2 + 31 H3 = 43 | exact match (cross_cohort_summary.tsv confirms identical aggregates) | PASS |
| F | Y37 probe absent from baseline 1ad58ce | grep count = 0 in `crates/kernel/src/tessellation/mod.rs` of `1ad58ce` | 0 matches | PASS |
| G | kernel lib regression | 1262/24/42 (no regression vs 1ad58ce) | 1262 passed, 24 failed, 42 ignored | PASS |
| H | yang_fast corpus ≥10/157 | ≥10 | 10/157 passed, 139 failed, 8 errored, 33 timeouts (identical to PR-Y36) | PASS |
| I | No-last-bug / no-status-passed claims | Only explicit negations or benign "Status:Failed" | Only PR-Y37's own explicit negations + benign `Status:Failed` lines | PASS |
| J | Cross-cohort prediction refutation independently confirmed | F0020 OTHER H1=0%, F0044/F0045 H1=0%, R0092 H2=27.9% (not ≥80%) | F0020 H1=0%; F0044 H1=0%, F0045 H1=0%; R0092 H2=27.9% (12/43) | PASS |

---

## §3 Independent F0020 attribution aggregation (Gate D)

Aggregated `awk -F'\t' 'NR>1 {print $10}' /tmp/y37-adv-probe/F0020_inv006_inverse_attribution.tsv | sort | uniq -c`:

```
      9 D1a
      8 D1d
      3 OtherH2
     19 OtherH3
```

Independent total = 9 + 8 + 3 + 19 = **39** unpaired Render LOD edges. Matches canary memo §0 table EXACTLY in all five categories (D.1a=9, D.1b=0, D.1c=0, D.1d=8, OtherH1=0, OtherH2=3, OtherH3=19). Delta from canary = **0/0/0/0/0** across all categories. Independent reproduction stands.

H1 % of Other = 0/22 = 0.0%; H2 % of Other = 3/22 = 13.6%; H3 % of Other = 19/22 = 86.4%. All three match canary memo §0 percentages to one decimal.

---

## §4 Cohort attribution (Gate E)

Aggregated from `/tmp/y37-adv-cohort/cross_cohort_summary.tsv` (R0092 captured as part of `spotlight_f0044` batch composition, which mirrors F0044+F0045+R0092 per `assay_randomized.rs:516` + `pr_y22_mode_a_missing_regression.rs:391`):

| Case | Total unpaired | D.1a | D.1d | OtherH1 | OtherH2 | OtherH3 | H1 % of Other | H2 % of Other | H3 % of Other |
|------|---------------|------|------|---------|---------|---------|---------------|---------------|---------------|
| F0044 | 12 | 0 | 0 | 0 | 0 | 12 | 0.0% | 0.0% | **100.0%** |
| F0045 | 38 | 0 | 0 | 0 | 0 | 38 | 0.0% | 0.0% | **100.0%** |
| R0092 | 43 | 0 | 0 | 0 | 12 | 31 | 0.0% | 27.9% | **72.1%** |

All three cases match canary memo §0 Cohort table EXACTLY. R0092's spotlight-test gap (no dedicated `spotlight_r0092` exists) is correctly documented in the canary; R0092 surfaces via the `spotlight_f0044` 3-case batch, and the TSV writer assigns it `inv003` within that batch.

Note: an extra `R0045_inv001` row appeared in the cross-cohort summary (88 H3) because spotlight_r0045 ran as the third gated case for independent cohort capture, separate from the spotlight_f0044 batch. This is incidental probe coverage, not a contradiction of any canary claim.

---

## §5 Cross-cohort prediction refutation (Gate J)

PR-Y36 §4.2 banked the prediction that PR-Y27's D.2/D.3 sub-mechanisms would map onto the cohort cases as:
- D.2 (sub-grid seam) ≈ H1 dominant for F0044/F0045 (≥80%)
- D.3 (NMM-edge tessellation) ≈ H2 dominant for R0092 (≥80%)
- F0020 OTHER ≈ proportional mix of H1 + H2

Independent observation:

| Cohort case | Predicted | Observed | Outcome |
|-------------|-----------|----------|---------|
| F0044 | ≥80% H1 | 0% H1, 100% H3 | **REFUTED** |
| F0045 | ≥80% H1 | 0% H1, 100% H3 | **REFUTED** |
| R0092 | ≥80% H2 | 27.9% H2, 72.1% H3 | **REFUTED** (H2 present but minority) |
| F0020's 22 OTHER | mix H1+H2 | 0% H1, 13.6% H2, 86.4% H3 | **REFUTED** |

The refutation is genuine — not a measurement artifact — because:
1. The H1 detector's antecedent (axis-aligned + grid-quantized boundary edges) is well-defined and the threshold (≥80%) is explicit in `y37_sub_classify`. The detector successfully fires on no faces in the cohort, indicating those faces simply have no axis-aligned boundary edges (cohort cylinder-rim boundaries are curved-discretized).
2. The H2 detector fires meaningfully on R0092 (12/43 = 27.9% — non-zero, non-trivial), demonstrating the detector works when its antecedent is present.
3. H3 is residual-by-construction, so dominance there is a coherent signal: the cohort defect mechanism is real but doesn't surface in H1/H2 proxies as defined.

The canary correctly identifies three plausible reasons for the failure (H1 misses curved cohort boundaries; H2 can't fire on clean-arena cases; PR-Y27 D.2 is sub-quantization-granularity), and correctly refrains from declaring which is load-bearing. This is the expected output of an honest refutation.

---

## §6 Paper-grounding + no-last-bug (Gate I)

Grep for fix-completion language across spec + canary returned exactly two lines, both from the spec and both explicit negations:

1. *"Per `feedback_no_last_bug` and `feedback_phase1_diagnosis_ranking_is_inference`, this spec banks 4 candidate options for PR-Y38 with rationale. **None is promoted to 'the fix.'** Empirical chain 'fix → unpaired_count to 0' is not yet verified for any candidate."*
2. *"This memo does NOT claim 'this closes Yang' or 'this is the last gap on Render LOD.' Per `feedback_no_last_bug`, the OTHER cluster's true mechanism remains uncharacterized below H1/H2 signature granularity. **The OTHER cluster is now better measured than at PR-Y36 — but less understood as a fix shape.**"*

Both passages are correct framing. No production fix is claimed. Compliance with `feedback_no_last_bug`: **CONFIRMED**.

Paper-grounding: H1 maps to PR-Y27 D.2 (sub-grid seam framework) and H2 maps to PR-Y27 D.3 (NMM-edge tessellation framework) per the canary §0 table cross-walk. The Cherchi 2022 §3 / Yang §4.4.1 references in the brief are about mesh updating and `edgeIsManifold` semantics, which underpin the NMM-asymmetry detection logic — the H2 proxy's antecedent (NMM edges with topology-present-but-render-absent twin) is consistent with the §3 frame. No paper claim is over-extended.

---

## §7 Banked findings

Two minor banked findings, both already noted by the canary or carried forward from prior cycles:

1. **R0092 spotlight test gap.** R0092 has no dedicated `spotlight_r0092` test — it ships as the third member of the `spotlight_f0044` 3-case batch. This is a probe-coverage convention, not a defect. The canary memo correctly documents this in its Gate 5 narrative. Banked for PR-Y38+ consideration if R0092 becomes a primary anchor.
2. **H3 cross-cohort dominance is itself a signal.** Per canary §5 + commit message: H3-dominance across all four cohort cases (F0020 86.4%, F0044 100%, F0045 100%, R0092 72.1%) is a positive cross-cohort observation — just at a different mechanism than PR-Y36 predicted. The canary correctly does NOT promote this to "the anchor"; it remains a candidate observation pending PR-Y38 canary verification of any specific fix shape. Per `feedback_adversary_recommendations_need_canary` I am not promoting this either.

No findings rise to ACCEPT-WITH-BANKED level — both are already correctly handled in the canary memo and PR-Y38 banked options.

---

## §8 Recommendation

**ACCEPT.**

PR-Y37 is sound infrastructure: the probe extension is byte-identical when default-off (Gate B), kernel lib + yang_fast regression baselines unchanged (Gates G/H), Y37 code provably absent from baseline (Gate F), and the attribution data reproduces to the digit on independent re-run (Gates D/E). The cross-cohort prediction refutation is a genuine and load-bearing empirical finding (Gate J). The canary memo and spec correctly bank PR-Y38 candidates without promoting any to "the fix," in compliance with `feedback_no_last_bug` and `feedback_phase1_diagnosis_ranking_is_inference`.

This is the 6th consecutive canary-stage finding-no-fix-shape outcome on F0020 Render LOD (Y25/Y26/Y27/Y28/Y36/Y37). Per the strategic escalation rule of `feedback_anchor_before_fix`, continuing infrastructure-class PRs that refine measurement remains the appropriate posture until an empirical "fix → unpaired_count to 0" chain is established. PR-Y37 advances that measurement capability and correctly refrains from over-claiming.
