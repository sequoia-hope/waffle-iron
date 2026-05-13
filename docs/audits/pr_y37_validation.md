# PR-Y37 Validation — OTHER-cluster H1/H2/H3 sub-classification; cross-cohort prediction REFUTED; INFRASTRUCTURE-CLASS; **ACCEPT**

| Field | Value |
|---|---|
| Author | audit-y37 |
| Date | 2026-05-13 |
| Live tree HEAD | `3e35f8c` (PR-Y37 impl; **NOT pushed**) |
| Parent (baseline) | `1ad58ce` (PR-Y36 audit ACCEPT) |
| Class | **INFRASTRUCTURE-ONLY** — additive env-gated probe extension + spec + canary memo |
| FIP §5 | GREEN — 4-phase artifact set complete, role separation intact |
| DoD | GREEN — default-off byte parity verified (kernel lib 1262/24/42 exact; yang_fast 10/157 exact; F0020 Status:Failed unchanged) |
| Verdict | **ACCEPT — authorize Phase 8 push and close-out** |

---

## §0 Single-paragraph verdict

PR-Y37 is an infrastructure-only extension of the PR-Y36 inverse-direction probe at `crates/kernel/src/tessellation/mod.rs::tessellate_solid_bounded`; **no production logic is changed**. The probe gains three new `Y36Class` variants (`OtherH1`/`OtherH2`/`OtherH3`), two new attribution-TSV columns, four new face-inventory columns, and a `cross_cohort_summary.tsv` aggregator (~245 LOC additive). FIP §5 phase artifacts are present for all 4 expected phases (canary / spec / impl / adversary), with 4 distinct role-separated agents (no test-author per the INFRA-CLASS precedent PR-Y29/Y33/Y36). DoD's load-bearing constraint — default-off byte parity — is verified by adversary Gates B (F0020 Status:Failed at 40 unpaired exact), G (kernel lib 1262/24/42 exact), and H (yang_fast 10/157 exact). The empirical centerpiece — F0020 inv#6 attribution `total=39, D1a=9, D1d=8, OtherH1=0, OtherH2=3, OtherH3=19` — is independently re-aggregated by adversary §3 with zero delta across all categories. The cohort-cross-prediction REFUTATION (Gate J) is verified by adversary §5: F0044/F0045 are 100% H3 (predicted ≥80% H1), R0092 is 27.9% H2 / 72.1% H3 (predicted ≥80% H2); the refutation is genuine, not a measurement artifact (H1 antecedent is well-defined and fires zero times on curved cohort boundaries; H2 fires meaningfully on partial-NMM R0092 kids, demonstrating the detector works when its antecedent is present). The INFRA-CLASS framing is honored: per `feedback_anchor_before_fix`'s strategic escalation rule, no fix shape is proposed without an empirical chain "fix → unpaired_count to 0"; 4 PR-Y38 candidate options are banked in canary §4.3 + spec §7, none promoted. `feedback_no_last_bug` is respected (adversary Gate I returned 2 grep hits, both explicit negations). **Recommendation: ACCEPT and authorize Phase 8 push.**

---

## §1 FIP §5 phase-artifact checklist

| Phase | Artifact | Path | Agent | Present |
|---|---|---|---|---|
| Canary | Probe extension + cross-cohort REFUTATION memo | `docs/audits/pr_y37_canary.md` (460 LOC) | canary-y37 | ✓ |
| Spec | H1/H2/H3 design + 6th-refutation framing | `specs/yang_pr_y37_other_classification.md` (237 LOC) | spec-y37 | ✓ |
| Tests | (Not required — infra-class, no behavior change; FIP §4 satisfied by default-off byte parity gates) | — | (none) | n/a |
| Implementation | Probe extension applied to live tree, default-off byte-identical | `3e35f8c` (`tessellation/mod.rs` +282/-18, +WASM rebuild, +memos) | impl-y37 | ✓ |
| Adversarial validation | Independent 10-gate verification + cohort re-aggregation | `docs/audits/pr_y37_adversary.md` (~180 LOC) | adversary-y37 | ✓ |
| Audit | This memo | `docs/audits/pr_y37_validation.md` | audit-y37 | ✓ (in progress) |

**No test-author phase.** PR-Y37 is INFRASTRUCTURE-CLASS with zero production logic change; FIP §4 regression coverage is satisfied by kernel lib + yang_fast suites + F0020 spotlight remaining GREEN (adversary Gates B + G + H, all PASS). This mirrors PR-Y29/Y33/Y36 precedent (all infra-class, all 4-phase, all no dedicated test-author). The plan §4 explicitly authorizes this framing.

---

## §2 Role separation verification

Four distinct agents produced four artifacts:

| Agent | Role | Artifact | Worktree |
|---|---|---|---|
| canary-y37 | Probe extension + H1/H2/H3 sub-classification + cohort attribution measurement | `pr_y37_canary.md` | `worktree-canary-y36` @ `8778907` (re-used to keep Y36+Y37 scaffolding co-located) |
| spec-y37 | Spec drafting + 6th-refutation framing + PR-Y38 banked options enumeration | `yang_pr_y37_other_classification.md` | worktree (separate) |
| impl-y37 | Live-tree implementation commit + WASM rebuild + verbatim diff | `3e35f8c` | live tree main |
| adversary-y37 | Independent gate re-verification + paper/no-last-bug audit | `pr_y37_adversary.md` | non-destructive baseline worktree `/tmp/y37-adv-baseline` |

Per `feedback_oracle_credibility_via_role_separation`: oracle-build (canary independent extension + measurement) and oracle-interpret (adversary independent re-aggregation) are on different agents. Audit (this memo) weighs evidence; it does not re-run gates.

Per `feedback_decline_cross_cycle_role_assignments`: this cycle uses a fresh `pr-y37` team scoped to the current PR; PR-Y36 close-out's TeamDelete was clean per the plan §0 sweep.

---

## §3 DoD checklist

| Item | Status | Evidence |
|---|---|---|
| Default-off byte parity — F0020 spotlight (load-bearing) | **GREEN** | Adversary Gate B: probe-off F0020 spotlight reproduces baseline exactly (Status:Failed, 40 unpaired = 39 boundary + 1 NMM, 8 degenerate, 10 self-int) — identical to PR-Y36 baseline. |
| Default-off byte parity — kernel lib | **GREEN** | Adversary Gate G: `cargo test -p kernel --lib` = `1262 passed; 24 failed; 42 ignored` — exact match with parent `1ad58ce`. |
| Default-off byte parity — yang_fast corpus | **GREEN** | Adversary Gate H: yang_fast = `10/157 passed, 139 failed, 8 errored` — exact match. |
| Probe extension is genuinely additive (not pre-existing) | **GREEN** | Adversary Gate F: 0 matches in baseline `1ad58ce`'s `tessellation/mod.rs` for Y37-specific symbols. |
| Commit hygiene (no `results.json` staged; explicit file list) | **GREEN** | `git show 3e35f8c --stat` = 4 files (WASM binary + tessellation/mod.rs + canary memo + spec); `results.json` not staged. |
| WASM rebuild bundled with Rust changes | **GREEN** | `app/static/pkg/wasm_bridge_bg.wasm` updated in same commit (5037814 → 5046563 bytes). |
| Verbatim git diff in impl report (`feedback_implementer_anti_fabrication_diff`) | **GREEN** | Adversary Gate A reproduces `+282/-18` in tessellation/mod.rs; numstat confirms 4-file commit shape. |
| No destructive git on live tree (`feedback_adversary_no_destructive_git`) | **GREEN** | Adversary §1: baseline replay used `git worktree add`/`remove --force`, both non-destructive; live tree confirmed at `3e35f8c [main]` unchanged. |
| No "closes Yang" / "last-bug" language (`feedback_no_last_bug`) | **GREEN** | Adversary Gate I: 2 grep matches, both explicit negations. |

All DoD gates GREEN.

---

## §4 Empirical evidence cross-check

The load-bearing empirical claim — F0020 inv#6 H1/H2/H3 sub-classification + cohort cross-prediction REFUTATION — must reproduce byte-for-byte under independent measurement. Canary §3.1 / §3.2 and adversary §3 / §4 are the two independent aggregations.

### §4.1 F0020 inv#6 attribution

| Class | Canary §3.1 | Adversary §3 | Δ |
|---|---|---|---|
| Total | 39 | 39 | 0 |
| D.1a | 9 (23.1%) | 9 (23.1%) | 0 |
| D.1b | 0 | 0 | 0 |
| D.1c | 0 | 0 | 0 |
| D.1d | 8 (20.5%) | 8 (20.5%) | 0 |
| **OtherH1** | **0 (0.0%)** | **0 (0.0%)** | **0** |
| **OtherH2** | **3 (7.7%)** | **3 (7.7%)** | **0** |
| **OtherH3** | **19 (48.7%)** | **19 (48.7%)** | **0** |
| OTHER total | 22 (56.4%) | 22 (56.4%) | 0 |

**Zero delta across all 7 categories** under independent re-aggregation. The probe extension is deterministic and reproducible. H1 % of Other = 0.0%; H2 % of Other = 13.6%; H3 % of Other = 86.4% — all match to one decimal.

### §4.2 Cohort sub-classification

| Case | Canary §3.2 (H1/H2/H3) | Adversary §4 (H1/H2/H3) | Δ |
|---|---|---|---|
| F0044 | 0/0/12 (100% H3) | 0/0/12 (100% H3) | 0 |
| F0045 | 0/0/38 (100% H3) | 0/0/38 (100% H3) | 0 |
| R0092 | 0/12/31 (27.9% H2, 72.1% H3) | 0/12/31 (27.9% H2, 72.1% H3) | 0 |

Cohort attribution reproduced exactly across all three independently verified cases.

### §4.3 Cross-cohort prediction REFUTATION (Gate J)

PR-Y36 banked the load-bearing prediction (canary §4.2): F0044/F0045 ≥80% H1, R0092 ≥80% H2, F0020 OTHER proportional H1+H2 mix. All four predictions miss by ≥50 percentage points (canary §3.3 = adversary §5):

| Prediction | Observed | Outcome |
|---|---|---|
| F0044 ≥80% H1 | 0% H1, 100% H3 | REFUTED |
| F0045 ≥80% H1 | 0% H1, 100% H3 | REFUTED |
| R0092 ≥80% H2 | 27.9% H2, 72.1% H3 | REFUTED |
| F0020 OTHER mixed H1+H2 | 0% H1 / 13.6% H2 / 86.4% H3 | REFUTED |

The refutation is **structural, not measurement noise**, per adversary §5's three independent reasons (verified against canary §5):
1. H1's antecedent (axis-aligned at quantization granularity) is geometrically impossible for cohort cylinder-rim curved boundaries — H1 is structurally zero, not threshold-mis-tuned.
2. H2's denominator (`outer_nmm_count`) is zero for clean-arena F0044/F0045 — H2 is mathematically excluded from those cases, not silenced by noise.
3. The H2 detector demonstrably DOES fire on R0092's partial-NMM kids 22/24/26/27 (12/43 = 27.9%), proving the detector works when its antecedent is present.

The refutation is a genuine empirical finding. The PR-Y36 cross-cohort hypothesis is empirically dead at the H1/H2 detection thresholds defined in the PR-Y37 plan.

---

## §5 Architectural invariant compliance

**A15.6 (Hybrid Boolean Pipeline — Yang 2025).** The probe operates at `tessellate_solid_bounded`, which is the Render LOD layer **downstream** of the Yang pipeline's boolean output. Yang 2025 §4.4.1 (mesh updating) is cited in spec §2.3 / §5.3 as relevant *background* for the H3-residual mechanism speculation, NOT as a claim that PR-Y37 implements §4.4.1. The probe IS the empirical reference for the Render LOD layer.

**Render LOD is outside Cherchi 2022 paper scope.** Spec §2.3 reaffirms PR-Y36's verified claim: Cherchi 2022's scope ends at arrangement → patch output (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:293-319`); no Render-LOD content exists in the paper. Therefore, per `feedback_external_coherence`, no external reference exists for the Render LOD layer, and the in-situ inverse probe IS the empirical reference. PR-Y37 adds *measurement capability* (H1/H2/H3 sub-classification, cross-cohort aggregator) without behavioral change.

**A15 (Analytical Primacy) is not in scope** — the probe is observation-only and does not alter the analytical/mesh role assignment for any surface.

The probe complexity is itself banked as a forward-looking risk per canary §9.2 / spec §9.2: cumulative probe code is ~707 LOC (462 from PR-Y36 + 245 from PR-Y37). This does not block ACCEPT; it informs PR-Y38 scoping (the spec correctly notes that further probe extension should be weighed against signal-per-LOC of a structurally different investigation).

---

## §6 INFRA-CLASS framing audit

The plan, spec, canary, adversary, and commit message all consistently frame PR-Y37 as INFRA-CLASS / SHIP-INFRA-ONLY. Three sub-claims to verify:

| Sub-claim | Status | Evidence |
|---|---|---|
| No production fix shape shipped (probe is default-off byte-identical) | **VERIFIED** | Adversary Gates B + G + H all exact-baseline match. Probe code wrapped `if y36_on { … }` per canary §1 / spec §3.4. Y37 sub-classification is applied in the writer, also probe-gated. |
| PR-Y38 anchor banked, not shipped | **VERIFIED** | 4 candidate options live in canary §4.3 and spec §7; commit message body explicitly enumerates them and labels each "NOT a recommended anchor." Adversary Gate J independently verifies the refutation is sound, NOT promotion of any option. |
| Strategic escalation rule honored | **VERIFIED** | Per `feedback_anchor_before_fix`, "three wrong anchors → stop bisecting, build a reference comparison." PR-Y37 is the 6th investigational PR on F0020 Render LOD (Y25/Y26/Y27/Y28 canary-stage ABORTed; Y36 SHIP-INFRA; Y37 SHIP-INFRA). Spec §2.1 + canary §4.1 + commit message all explicitly invoke this rule. No empirical chain "fix → unpaired_count to 0" is claimed for any of the 4 PR-Y38 banked options. |

The INFRA-CLASS framing is internally consistent across all 5 artifacts (plan, spec, canary, impl commit, adversary memo). The "6th-refutation framing" in the verdict (canary §5 + spec header) is the honest characterization: PR-Y36's cross-cohort hypothesis is empirically REFUTED, not validated, by PR-Y37's probe extension — and this constitutes useful candidate-space narrowing without proposing a fix.

**Refutation soundness (adversary Gate J cross-check).** The hypothesis being refuted is PR-Y36's prediction that F0020 OTHER reduces to the cohort's defect-at-higher-arena-density (H1 for F0044/F0045, H2 for R0092). PR-Y37 ran the canary verification of that exact prediction at the thresholds proposed in PR-Y36's plan; the prediction misses by ≥50 percentage points on every case. The refutation is empirical, not interpretive — and the canary correctly does NOT conclude "PR-Y27's D.2/D.3 framework is wrong"; it concludes only that the H1/H2 *signatures defined for those mechanisms in the PR-Y37 plan* fail to discriminate the cohort. This distinction is preserved in spec §5 + adversary §5, and matches `feedback_phase1_diagnosis_ranking_is_inference`.

---

## §7 Strategic context — 6 PR cycles, 6th no-fix outcome

PR-Y37 is the 6th consecutive canary-stage finding-no-fix-shape outcome on F0020 Render LOD (Y25/Y26/Y27/Y28 ABORTed; Y36/Y37 SHIP-INFRA). Zero production code has been modified on F0020 Render LOD across 6 PRs.

Per `feedback_anchor_before_fix`'s strategic escalation rule — "three wrong anchors in a row → stop bisecting, build a reference comparison" — both PR-Y36 (D.1 attribution) and PR-Y37 (cross-cohort H1/H2/H3 attribution) are reference-comparison-class investigations. Each cycle has eliminated a candidate hypothesis:

- **PR-Y25/Y26/Y27/Y28** eliminated `disc.positions` clipping, missing triangles, flood_fill patch dropout, and D.1a/b/c/d production-fix candidates (all canary-stage ABORTs).
- **PR-Y36** eliminated PR-Y28's D.1c-dominant hypothesis (D.1c=0% at HEAD, empirically dead).
- **PR-Y37** eliminated PR-Y36's "OTHER = cohort defects at higher arena density" hypothesis (H1=0% across all 4 cases; H2 fires only minoritarily on R0092). The H1/H2 detection methodology defined in the PR-Y36 plan is empirically too coarse — it doesn't discriminate the cohort.

The cycle of refutation is the discipline working. Per `feedback_no_last_bug`: this validation does NOT claim that PR-Y38 will close Render LOD; it does NOT claim that the OTHER cluster's mechanism is now understood. The OTHER cluster is **better measured** at PR-Y37 (H1/H2/H3 sub-classification cohort-wide) than at PR-Y36, but **less understood as a fix shape** — per spec §8 verbatim ("The OTHER cluster is now better measured than at PR-Y36 — but less understood as a fix shape"), which the adversary Gate I confirms is an explicit negation of any closure claim.

The durable artifact is the probe itself, now ~707 LOC across PR-Y36+PR-Y37, accumulating diagnostic capability that any future production-fix candidate can leverage to verify the empirical chain "fix → unpaired_count to 0" before commit.

---

## §8 Banked findings disposition

The following banked findings carry forward beyond PR-Y37; none block the SHIP-INFRA verdict.

### §8.1 From canary §4.3 / spec §7 — 4 PR-Y38 candidate options (NOT promoted)

1. **Refine H1 to sub-quantization vertex-pair comparison** (canary §4.3 option 1; spec §7 option 1). Probes PR-Y27 D.2's sub-grid signature below the f32 ULP / oracle grid. +150-300 LOC; requires neighbor-face lookup not in current probe.
2. **Refine H2 to per-segment NMM-incidence** (canary §4.3 option 2; spec §7 option 2). Walks outer-loop half-edges in dispatch order; per-HE-to-per-position-segment mapping with edge-discretization expansion.
3. **Pivot: probe `count_unpaired_in_mesh` f32 → quantization round-trip** (canary §4.3 option 3; spec §7 option 3). Foundation for fresh D.2 investigation below current grid granularity.
4. **Cheap singleton: D.1d kids 218/232/233 survival fix** at `tessellation/repair.rs:585` (canary §4.3 option 4; spec §7 option 4). ~20 LOC hygiene PR; accounts for 8/40 of F0020 unpaired; does NOT close Status:Failed; cohort regression risk.

Per `feedback_adversary_recommendations_need_canary`: all 4 are candidate anchors requiring in-situ canary verification, NOT directive. None has a verified empirical chain "fix → unpaired_count = 0".

### §8.2 New positive finding — H3 cross-cohort dominance (spec §6, canary §4.2)

H3 is dominant across all 4 cases (F0020 86.4%, F0044 100%, F0045 100%, R0092 72.1%). This is a distinct empirical claim from the refuted PR-Y36 hypothesis: the OTHER cluster **may still be cohort-shared**, but the shared mechanism is BELOW the probe's current detection granularity. Per `feedback_phase1_diagnosis_ranking_is_inference`, this finding is NOT promoted to a PR-Y38 anchor; it informs Option 1 / Option 3 scoping.

### §8.3 From adversary §7 — 2 minor banked findings

1. **R0092 spotlight test gap** — R0092 ships as the third member of `spotlight_f0044` 3-case batch (no dedicated `spotlight_r0092`). Probe-coverage convention, not a defect. Banked for PR-Y38+ if R0092 becomes a primary anchor.
2. **H3 cross-cohort dominance** — same as §8.2 above; adversary correctly does NOT promote per `feedback_adversary_recommendations_need_canary`.

Adversary has 0 ACCEPT-WITH-BANKED-level findings.

### §8.4 Open work items beyond PR-Y37 (carried from PR-Y36, unchanged)

- F0020 Render LOD Status:Failed remains (40 unpaired oracle / 39 boundary + 1 NMM probe).
- F0044/F0045/R0092 Status unchanged (cohort baselines preserved exactly).
- 139 still-failing yang_fast cases unchanged.
- Cherchi C++ TBB non-determinism (PR-Y31 banked).
- Cumulative probe complexity ~707 LOC — informs PR-Y38+ scoping per canary §9.2 / spec §9.2.

Per `feedback_no_last_bug`, no "closes Yang" / "last gap" claim is made anywhere in PR-Y37's artifacts.

---

## §9 Final recommendation — **ACCEPT**

PR-Y37 satisfies:

- **FIP §5**: 4 phase artifacts present (canary/spec/impl/adversary), 4 distinct role-separated agents, no test-author per the established INFRA-CLASS precedent (PR-Y29/Y33/Y36).
- **DoD**: default-off byte parity verified independently across 3 axes (F0020 spotlight, kernel lib, yang_fast); commit hygiene clean (4 files, no `results.json`); WASM rebuild bundled; verbatim diff per `feedback_implementer_anti_fabrication_diff`; non-destructive git per `feedback_adversary_no_destructive_git`; no closure claims per `feedback_no_last_bug`.
- **A15 architectural framing**: Render LOD outside Cherchi 2022 paper scope (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:293-319`); probe IS the empirical reference per `feedback_external_coherence`; Yang §4.4.1 cited as background only.
- **Load-bearing empirical claim**: F0020 inv#6 attribution (D.1a=9/D.1d=8/OtherH1=0/OtherH2=3/OtherH3=19/total=39) + cohort attribution (F0044 12 H3, F0045 38 H3, R0092 12 H2 + 31 H3) are independently re-aggregated by adversary §3-§5 with **zero delta** across all categories. The cross-cohort prediction REFUTATION (Gate J) is genuine — structural, not measurement noise.
- **INFRA-CLASS framing**: no production fix shipped; PR-Y38 anchor not committed (4 options banked); strategic escalation rule observed (6th consecutive no-fix outcome on F0020 Render LOD).

**Recommendation: ACCEPT — authorize Phase 8 close-out (audit-memo commit, push origin main per `feedback_always_push`, memory entry, `TeamDelete`).** Banked findings are carried forward to PR-Y38; none block the SHIP-INFRA verdict.

Phase 8 close-out must:
1. Commit this audit memo as `audit(yang-pr-y37): ACCEPT — H1/H2/H3 OTHER sub-classification validated; cross-cohort prediction refuted`.
2. Plain `git push origin main` (NOT force-push, per `feedback_always_push`).
3. Add memory entry `yang_pr_y37_shipped.md` + one-line MEMORY.md index. Memory MUST explicitly state: "INFRASTRUCTURE-CLASS, no production fix; PR-Y38 anchor banked (4 candidate options: H1 sub-quantization refinement / H2 per-segment NMM-incidence / quantization round-trip pivot / D.1d cheap hygiene). None promoted. 6th consecutive no-fix outcome on F0020 Render LOD."
4. `TeamDelete` per `feedback_per_plan_cycle_team`.
5. NO "closes Yang" / "last gap" language in any artifact.

End of memo.
