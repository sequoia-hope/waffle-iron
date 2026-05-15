# PR-Y43 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y43 |
| Date | 2026-05-15 |
| Live tree HEAD (impl-y43) | `f335efc` (PR-Y43 INFRA, NOT pushed) |
| Worktree HEAD | `b0009bd` (PR-Y42 SHIP-INFRA; impl-y43 content mirrored as uncommitted) |
| Parent | `b0009bd` (PR-Y42 audit; BORDERLINE-sharp ACCEPT) |
| Class | INFRASTRUCTURE-CLASS (test-harness extension; 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Strategic-pivot ROI | **POSITIVE — F0020 attribution moved from 50% borderline (PR-Y42) to 90% accountable (PR-Y43)** |
| Verdict | **ACCEPT (SHIP-INFRA) — D-dominant + Case C = 0 stands; two canary framing defects reconciled in this memo; PR-Y44 anchor ranking adjusted to co-equal (α)/(γ)+(δ)** |

---

## §1 Adjudication summary (single paragraph)

PR-Y43 ships +438 LOC test-file harness extension at `crates/test-harness/tests/cherchi_differential_diff.rs` (1082 → 1520) that classifies each of F0020's 42 missing-attributable Cherchi-only triangles into one of four cases (A/B/C/D) by counting Cherchi-side vertex matches against Waffle's Render LOD vertex set at four grid scales (1×/2×/5×/10×). Canary-y43 measured A=4 (9.5%) / B=14 (33.3%) / **C=0 (0.0%)** / **D=24 (57.1%)** at the 42-target Cherchi-non-det mode. Adversary-y43 independently re-ran 4 times against the impl-y43 mirror in the worktree and **byte-reproduced the canary's load-bearing histogram exactly in 1 of 4 reruns (42-mode)** while reproducing the 47-mode histogram (A=7 / B=14 / C=0 / D=26) in 3 of 4 reruns; **in BOTH modes the load-bearing invariants hold** — Case B count = 14 (BYTE-STABLE), Case C count = 0 (BYTE-STABLE), Case D ≥ 55% (BYTE-STABLE-IN-DOMINANCE). All 8 gates GREEN (build, probe-off byte parity, F0020 histogram, Case B vertex dump, cohort sanity, PR-Y42 baseline, kernel lib 1262/24/42 + yang_fast 10/157, PR-Y31 hard gate `pr_y31_f0044_extras_zero`). Adversary §3.2 flagged two **framing/accounting defects in the canary memo prose** — neither in the probe code, neither in the load-bearing histogram, neither in the verdict logic: (defect 1) canary §4.2/§6.4/§9.1 mis-aggregates "5 distinct off-vertex positions / 11 of 14 Case B entries" — the empirically-correct accounting is **10 distinct off-vertex positions; 3 distinct positions are shared by 7 of 14 entries; the remaining 7 are unique**; (defect 2) canary §6.2 claims "for 24 of the 42 ... ALL THREE vertex positions appear" as a measured Case D semantic — that 3-of-3-at-1× sub-class is **logically inferred from the priority-ordered classification, not directly measured by the probe**; Case D's residual catch-all admits a 2nd sub-mechanism (`match_at_1x ∈ {0,1}` with `match_at_5x == 2`) the probe does not distinguish. Per PR-Y42 precedent (audit memo adjudicates canary findings without forcing edits), this audit carries the corrections forward as authoritative (Adjudication 2 = policy B). PR-Y44 anchor recommendation is re-ranked to co-equal (α) `remove_winding_insensitive_duplicates` at F.0 and (γ) Boolean LOD → Render LOD pre-F.0 re-tessellation (108-tri drop layer); (β) F.3 `remove_nonmanifold_duplicates_aggressive` demoted to tertiary; PRECEDED by NEW (δ) Case D sub-class disambiguation probe that emits per-Case-D `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` tuples to separate the 3-of-3-at-1× sub-class (a) from the 0-or-1-at-1×-plus-2-at-5× sub-class (b) before any fix attempt. Recommend **ACCEPT (SHIP-INFRA)** + Phase 8 push authorized.

---

## §2 Gates re-summary (verbatim adversary §2 table)

Per adversary §2 — independent re-run against impl-y43 mirror in worktree. No canary log re-use; all gates run fresh from this worktree's shell session.

| Gate | Description | Expected (canary) | Observed (adversary) | Status |
|---|---|---|---|---|
| **A** | `cargo build -p test-harness --test cherchi_differential_diff` | Clean; 58 pre-existing kernel warnings | Clean; finished in 0.04s; 58 kernel + 1 slvs pre-existing warnings | **GREEN** |
| **B** | F0020 spotlight default-off byte parity | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` + `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` | EXACT match: 40 unpaired (39 boundary, 1 non-manifold); 8 of 113 degen; 10 self-int. `[stage-f]` trace byte-identical | **GREEN** |
| **C1** | F0020 A/B/C/D run 1 | 4/14/0/24 at target=42 (75%) OR 7/14/0/26 at target=47 (25%) | target=**47**; A=7, B=14, C=0, D=26 | GREEN (47-mode) |
| **C2** | F0020 A/B/C/D run 2 | (idem) | target=**47**; A=7, B=14, C=0, D=26 | GREEN (47-mode) |
| **C3** | F0020 A/B/C/D run 3 | (idem) | target=**42**; A=4, B=14, C=0, D=24 ← **canary's load-bearing claim reproduced exactly** | GREEN (42-mode) |
| **C4** | F0020 A/B/C/D run 4 | (idem) | target=**47**; A=7, B=14, C=0, D=26 | GREEN (47-mode) |
| **C-aggregate** | Cherchi non-det mode mix | canary §3.3 reported 3/4 runs at 42-mode (75% / 25%) | Adversary saw **1/4 at 42-mode** (25% / 75%) — non-det split is wider than canary stated, though the **load-bearing invariants HOLD** in all modes | GREEN with caveat |
| **D** | F0020 Case B 14-entry table | Specific (off_idx, C_pos, W_pos, cell_dist) per canary §4 | All 14 entries byte-match canary §4 in 42-mode run (R3). Spot-checked b[0] cell_dist=12,661 ✓, b[1] cell_dist=1,238 ✓, b[3] cell_dist=12,793 ✓, b[9] cell_dist=815 ✓, b[13] cell_dist=6,884 ✓ | **GREEN** |
| **E** | Cohort F0044 / F0045 / R0092 | F0044 target=16, B=8 (50%), D=8 (50%); F0045 target=4, B=2, D=2; R0092 target=0 | F0044 target=16, A=0, B=8 (50.0%), C=0, D=8 (50.0%); F0045 target=4, A=0, B=2 (50.0%), C=0, D=2 (50.0%); R0092 target=0 (vacuous all-zero) | **GREEN** |
| **F** | PR-Y42 baseline `f0020_render_lod_diff_baseline` | common=36, attribution 20/40 = 50.0%, target_tris=42; missing 194 (or 201 off-mode) | common=36, attribution 20/40 = 50.0%, target_tris=42, missing=194, extras=76 | **GREEN** |
| **G1** | `cargo test -p kernel --lib` | 1262 / 24 / 42 | 1262 / 24 / 42 — IDENTICAL | **GREEN** |
| **G2** | `YANG_BOOLEAN=1 yang_fast` | 10/157 | 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts) | **GREEN** |
| **H** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 Stage B `missing=0, extras=0, common=136` | F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4 | **GREEN** |

**8/8 gates GREEN** (counting C-aggregate as one gate per adversary §2). Per `feedback_oracle_credibility_via_role_separation`: canary built the probe and measured the 42-mode histogram; adversary independently re-ran from the worktree mirror without inheriting canary's reasoning chain and reproduced (a) the 42-mode load-bearing histogram byte-exact in 1/4 reruns + (b) the 47-mode histogram exactly in 3/4 reruns + (c) the byte-identical Case B 14-entry vertex dump in the 42-mode rerun. The Cherchi non-det 42/47 mode split is wider than canary's claim (adversary §2.1: combined 8-rerun evidence is 4/8 at 42-mode, ≈50/50, not 75/25), but the load-bearing invariants (Case B = 14, Case C = 0, Case D dominant) hold in BOTH modes — verdict is robust.

---

## §3 Framing-defect adjudication

The adversary's §3.2 + §4.2 identified two framing/accounting defects in canary memo prose and §2.1 + §4.3 identified one narrative defect on Cherchi non-det mode mix. None of the three are in the probe code (adversary §3.1 confirms classification logic correctness, mutual exclusivity, off-vert selection determinism, oracle-grid parity), nor in the load-bearing histogram (adversary §2 reproduces it byte-exact), nor in the verdict logic (D-dominant + Case C = 0 holds in both modes). They are corrections to **canary memo prose**.

### §3.1 Defect 1 — Case B off-vertex aggregation (canary §4.2 / §6.4 / §9.1)

**Canary claim (defect):** "5 distinct off-vertex positions account for 11 of 14 Case B entries" (canary §4.2 bullet, §6.4 secondary anchor, §9.1 banked item 2; spec §6.4 mirrors).

**Adversary measurement (correct):** parsed Case B dump from R3 log + canary's own `/tmp/y43-f0020-attribution.log` via `sort | uniq -c` on C_pos column (adversary §3.2 code block at L96-107): **10 distinct off-vertex positions; 3 distinct positions are shared by 7 of 14 entries** (b[9-11] share one; b[1-2] share another; b[6-7] share another), **the remaining 7 entries each have a unique off-vertex**.

**Origin of canary's error:** the canary §4.2 prose correctly identifies the 3 shared groups (b[9-11], b[1-2], b[6-7]) and then incorrectly aggregates "b[4] near b[8]" as a fourth-and-fifth shared group when those positions are merely close (cell_dist 2612 and 9267 respectively — different positions, not identical). The §4.1 cell-distance distribution table is correct; only the §4.2 summary bullet, the §6.4 forward reference to "5 positions," and the §9.1 banked item are misaggregated.

**Audit correction (authoritative):** the Case B secondary-anchor data set has **10 distinct off-vertex positions**, of which **3 share** between 2–3 entries each (7 total) and **7 are unique**. PR-Y44 secondary anchor data is 2× larger than canary states. **Still a compact data set** (10 positions vs 14 triangles is meaningful structural compression), but not as compact as "~5 positions" implies. Recommendation §4 (PR-Y44 anchor ranking) treats Case B as the same secondary-priority anchor with the corrected count.

**Adjudication-2 policy = (B):** match PR-Y42 precedent. Audit memo carries the correction forward; canary memo is shipped as-is with this audit as the authoritative reconciliation. Close-out should NOT force a canary memo edit. Rationale: PR-Y42's audit memo similarly adjudicated canary findings without forcing source-of-truth edits; per `feedback_per_plan_cycle_team` extra round-trip cost outweighs the modest framing-correctness benefit; the audit memo is by mandate the cycle's final word.

### §3.2 Defect 2 — Case D sub-mechanism inference (canary §6.2)

**Canary claim (defect):** "for 24 of the 42 missing-from-Waffle Cherchi triangles, ALL THREE of their vertex positions DO appear somewhere in Waffle's Render LOD vertex set at the base grid" — framed as a measured Case D semantic.

**Adversary tracing (correct):** the classification predicate at `cherchi_differential_diff.rs:1215-1225` is priority-ordered (A → B → C → D as catch-all). Case D = `¬A ∧ ¬B ∧ ¬C` admits the following `(match_at_1x, match_at_5x)` combinations:
- `(3, 3)` — 3-of-3 at every grid level ← canary's claimed "3-of-3 at 1×" sub-class **(a)**
- `(0, 2)` — 0-at-1× but 2-at-5× ← residual catch-all sub-class **(b)**
- `(1, 2)` — 1-at-1× and 2-at-5× ← residual catch-all sub-class **(b)**

Sub-classes `(0, 3)` and `(1, 3)` fire as Case A (because `match_at_5x == 3 ∧ match_at_1x < 3`). The probe **does not emit per-Case-D `(match_at_1x, match_at_5x)` distribution**; the 24-count is the residual after A/B/C fail, with sub-mechanism distribution unknown.

**Audit correction (authoritative):** Case D's 24 entries are an empirically-measured residual catch-all bucket whose sub-mechanism distribution is **unmeasured**. The canary's framing assumes sub-class (a) ("3-of-3 at 1×, triangle missing → indexing/winding/edge-pair defect") dominates. That assumption is plausible but **not load-bearing measurement**; sub-class (b) ("partial-proximity at 5× only, triangle missing") would have a different fix-shape (closer to Case B's "investigate off-vertex production" mechanism than to (α/β/γ)'s "triangle-emission" mechanism). The PR-Y44 anchor candidates (α/β/γ in canary §7.4 + spec §6.3) implicitly assume sub-class (a) dominates — that assumption needs measurement before any production fix.

**Implication for PR-Y44 anchor (§4 below):** sub-class disambiguation becomes a NEW (δ) probe-extension as a **prerequisite Phase 1 measurement** before α/β/γ fix-attempts. This is the disciplined-no-last-bug posture per `feedback_no_last_bug` + `feedback_anchor_before_fix` + `feedback_phase1_diagnosis_ranking_is_inference`: the Case D dominance is measured, but the sub-mechanism within Case D is inferred. Apply the same standard.

**Adjudication-2 policy = (B):** carried forward in this audit; canary memo not edited.

### §3.3 Defect 3 — Cherchi non-det mode mix narrative (canary §3.3 + §8.1 banked item 4 + spec §8 + §9 verification)

**Canary claim (defect):** "3 of 4 reruns produced `target_tris=42`; one produced 47" + spec §8 "**~75/25 split** (3/4 reruns gave 194; 1/4 gave 201)."

**Adversary measurement (correct):** independent 4 reruns saw **1/4 at 42-mode, 3/4 at 47-mode**. Combined 8-rerun evidence (canary 3+1, adversary 1+3) = **4/8 at 42-mode = 50/50** with both modes stable.

**Audit correction (authoritative):** the Cherchi non-det mode split is observed empirically **~50/50** across 8 combined reruns, not the canary's 75/25. Two stable modes are reachable; neither dominates statistically. The load-bearing invariants (Case B = 14, Case C = 0, Case D dominant) hold in BOTH modes, so this correction does not affect the verdict; it does affect the spec §8 stability table prose and the verification-block expected counts (canary §10 + spec §10) which both say "expect 42-mode (~75% of runs)."

**Adjudication-2 policy = (B):** carried forward. Close-out's memory update should reference "50/50 split observed across 8 combined canary+adversary reruns; both modes stable; load-bearing invariants hold in both modes."

### §3.4 Why these defects do NOT alter the verdict

- The probe code is correct (adversary §3.1 audit: classification mutual exclusivity, oracle-grid parity, off-vert selection determinism, Cherchi op-string per PR-Y31).
- The load-bearing histogram is reproduced byte-exact (adversary §2 gates C3 + D).
- Verdict logic depends on Case D ≥ 40% AND Case C = 0; both hold in both modes. The 5-vs-10 off-vertex count does not affect the verdict (Case B is a secondary anchor regardless). The Case D sub-mechanism unknown does not affect SHIP-INFRA (no production code is modified); it only narrows the PR-Y44 anchor ranking which is the §4 adjudication.
- 0 LOC production code modified; 0 regression risk.

**Net of framing defects: ACCEPT (SHIP-INFRA) with corrections carried forward in this audit memo.**

---

## §4 PR-Y44 anchor ranking (audit-adjudicated)

The canary memo §7.4 ranked candidates α (F.0 `remove_winding_insensitive_duplicates`, drops 19) > β (F.3 `remove_nonmanifold_duplicates_aggressive`, drops 6) > γ (pre-F.0 Boolean LOD → Render LOD re-tessellation, drops ~108). Adversary §5 argued γ should be **co-equal with α** (108 >> 19+6 = 25 by raw tri-drop magnitude) and that PR-Y44 Phase 1 should be a sub-class disambiguation probe-extension before any fix attempt.

This audit accepts the adversary's structural argument with one refinement: the 19-tri F.0 drop has **prior empirical work** that γ does not (PR-Y40 directly probed `remove_winding_insensitive_duplicates`, found 4 collisions + distributed winners). PR-Y41 §6.3 noted γ as a banked-but-unprobed candidate. So α has an existing measurement scaffold; γ does not. By raw magnitude γ dominates, but by anchor-readiness (measurement scaffolding + paper-citation precision) α is at least equal. **Net: co-equal**, both should be canaried at PR-Y44 Phase 1.

### §4.1 Adjudicated PR-Y44 anchor ranking

**(δ) Case D sub-class disambiguation probe** — NEW; PREREQUISITE.
Extend `f0020_render_lod_nearest_attribution` to emit per-Case-D `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` distribution + per-triangle dump. Separates sub-class (a) `(3, _, _, _)` from sub-class (b) `(0 or 1, _, 2, _)`. Cheap (~50 LOC additive to existing harness; infra-only; default-off byte parity by construction). Outputs (a) vs (b) proportion among the 24 Case D entries; if (a) ≥ 80%, (α) and (γ) are the load-bearing PR-Y44 targets; if (b) ≥ 40%, anchor shifts toward vertex-production for the residual sub-class (Case B-like mechanism for a different mismatch pattern). **Until (δ) lands, (α) and (γ) candidate priorities are inferred, not measured** — adversary §3.2 + this §3.2 + `feedback_phase1_diagnosis_ranking_is_inference` apply.

**(α) F.0 `remove_winding_insensitive_duplicates`** — CO-EQUAL with (γ).
Drops 19 triangles from 138 → 119. Paper anchor: Cherchi 2022 §5 manifold-flood (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`); the dedup pass implicitly assumes canonical input. PR-Y40 prior probe found 4 collisions + distributed winners. PR-Y44 canary should bisect: of the 19 dropped tris, how many have all-3-verts matching Cherchi-only-missing positions? Co-equal weight reason: scaled measurement scaffold + paper-citation precision + cheap canary path.

**(γ) Pre-F.0 Boolean LOD → Render LOD re-tessellation** — CO-EQUAL with (α).
Drops ~108 triangles from 246 → 138 (5.7× the F.0 magnitude). Anchored at `yang_integration.rs:1024` per canary §7.4 and PR-Y41 §6.3 banked. By raw tri-drop magnitude this layer dominates the F-stage cascade; if sub-class (a) is dominant in Case D, γ is statistically the most-likely source. **PR-Y41 banked γ as unprobed**; PR-Y44 should remove that gap.

**(β) F.3 `remove_nonmanifold_duplicates_aggressive`** — TERTIARY.
Drops 6 triangles from 119 → 113. Paper anchor: Yang 2025 §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:548-590`) — "selectively retaining one of the duplicate triangles." Demoted to tertiary because 6 tris << 24 Case D + 19 (α) + 108 (γ). Bank for PR-Y45 if (α)+(γ) doesn't close enough Case D entries.

**Case B (secondary anchor, distinct from D candidates)** — BANK for PR-Y45.
14 entries, **10 distinct off-vertex positions** (corrected from canary's "5"), of which 3 positions account for 7 of 14 entries and 7 are unique. Cell-distance range 815–12,793 cells (4.4 mm – 69 mm at F0020 base grid). Cohort F0044 / F0045 show 50% Case B as well. Fix-shape: investigate off-vertex upstream production. Independent of D fix-shape; both could ship.

### §4.2 Sequencing decision (recommendation to PR-Y44 planner)

**Phase 1 of PR-Y44: ship (δ) sub-class probe ONLY.** Independent INFRA SHIP, ~50 LOC harness extension; outputs the (a) vs (b) Case D proportion. ~1 day cycle.

**Phase 2 of PR-Y44 (depends on δ output):**
- If (a) ≥ 80%: canary (α) at F.0 + (γ) at pre-F.0 in parallel; pick whichever bisects the 24 Case D entries with higher signal. If both bisect overlapping sets, ship the cheaper fix first.
- If (a) and (b) are mixed (e.g., 60/40): split PR-Y44 into two separate PRs — one for sub-class (a) (topology emission) at α/γ, one for sub-class (b) (residual proximity defect) at the off-vertex production layer.
- If (b) ≥ 40%: shift PR-Y44 anchor to a Case-B-like mechanism (vertex production); α/γ deprioritized.

**Per `feedback_phase1_diagnosis_ranking_is_inference`**: the ranking above is structural inference + paper-citation alignment + tri-drop magnitude until (δ) produces measurement. PR-Y44 planner should treat α/γ as candidates contingent on δ's output, not as pre-decided fix targets.

### §4.3 What the audit explicitly rejects from the canary's PR-Y44 framing

- **Canary §7.4 ranks α PRIMARY** based on the count coincidence "19 + 6 = 25 ≈ 24 Case D." Audit rejects this primacy: 25 ≈ 24 is suggestive but the F.0+F.3 drops are TOTAL drops, not drops at unpaired-edge positions. Adversary §5.1 caught this. α has measurement-scaffold advantage but not magnitude advantage.
- **Canary §7.4 ranks γ TERTIARY** with the reasoning "banked as investigation target without localizing." Audit rejects this demotion: γ's 108-tri drop is 5.7× α's 19-tri drop; the localization gap is precisely what makes γ a high-value PR-Y44 canary target.
- **Canary §7.5 / §10 verdict ("D-dominant → α primary")** is sound at the **D-dominant** layer but the **α-primary** sub-claim is inference. Audit replaces with **D-dominant → (α/γ) co-equal, contingent on (δ) sub-class disambiguation**.

### §4.4 What the audit accepts from the canary's PR-Y44 framing

- **D-dominant verdict** holds (57.1% in 42-mode, 55.3% in 47-mode; both ≥ 40%).
- **Case C = 0 verdict** holds (byte-stable across all 4 adversary reruns + all 5 canary reruns). This is the strong refutation of "Option C pause": the defect IS at the Render LOD layer (or just-pre-Render-LOD); it is NOT upstream-of-Render-LOD-and-too-diffuse.
- **Case B = 14 secondary anchor** holds (byte-stable across all reruns). With the corrected off-vertex count (10 distinct, not 5), the structural argument is preserved.
- **Paper citations** (Cherchi 2022 §5 for α; Yang 2025 §4.4.1 for β; Yang 2025 §4.4.1 for γ) are aligned correctly.

---

## §5 Strategic context — 12-cycle ROI; D-dominant + Case C = 0 means F0020 stays IN scope

### §5.1 12-cycle accounting

| PR | Outcome | Cycle role |
|---|---|---|
| Y25 | ABORT (canary) | Yang §4.4.1 mesh-updating refuted as immediate anchor |
| Y26 | ABORT (canary) | Cohort-wide missing-triangle defect; not the 3 plan candidates |
| Y27 | ABORT (canary) | flood_fill_patches drops 0 SourceFaces; D.1 split into 3 sub-mechanisms |
| Y28 | ABORT (canary) | D.1d kids 218/232/233 identified; fix-shape refused commit |
| Y36 | INFRA SHIP | Inverse-probe source-face attribution (downstream) |
| Y37 | INFRA SHIP | H1/H2/H3 classification refined |
| Y38 | INFRA SHIP | Grid-sensitivity oracle gate; phantom-hypothesis refuted |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 with N=16 attribution |
| Y40 | INFRA SHIP — 6th-refutation | PR-Y39 §2.5's N=16 attribution refuted; measured N=4; banked "missing 12 upstream" |
| Y41 | INFRA SHIP — 7th-refutation | PR-Y40 §6's banked "missing 12 upstream" refuted; 18 indices EXACT; strategic-pivot trigger fired |
| Y42 | INFRA SHIP — B.1 STRATEGIC PIVOT executed; BORDERLINE-sharp | First external-oracle measurement at Render LOD; F0020 50.0% borderline; cohort `common=0` method-limit discovered |
| **Y43** | **INFRA SHIP — D-dominant + Case C = 0** | **F0020 90% accountable (D + B = 57% + 33%); Case C = 0 refutes Option C pause for F0020** |

**Cumulative cycle accounting (12 cycles):**
- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); 7 INFRA SHIPs (Y36/Y37/Y38/Y40/Y41/Y42/Y43); **0 production fix on F0020 Render LOD in 12 cycles**.
- Cumulative probe LOC: ~1358 production-instrumentation (Y36/Y37/Y40/Y41) + ~413 test-harness (Y42) + ~438 test-harness (Y43) = **~2209 LOC cumulative diagnostic infrastructure**.
- F0020 unpaired count: **40 → 40 across all 12 cycles**.

### §5.2 Strategic-pivot trajectory

| PR | F0020 measurement strength |
|---|---|
| PR-Y41 (pre-pivot) | "Missing 12 upstream" inference refuted; 18 indices EXACT; strategic-pivot trigger fired |
| PR-Y42 (pivot) | **50.0% borderline-sharp** attribution; ±1 cell-boundary noise floor; cohort `common=0` method-limit |
| PR-Y43 (this PR) | **90% accountable** (Case D 57.1% + Case B 33.3%); Case C = 0 byte-stable; PR-Y44 has 4 ranked anchor candidates with paper citations |

PR-Y42's audit memo §0 framed the B.1 strategic pivot as **MIXED ROI** at the close of cycle 11. PR-Y43 advances the F0020 attribution from 50% borderline to 90% accountable in one cycle. **Strategic-pivot ROI for F0020 is now unambiguously POSITIVE.** The cohort method-limit (PR-Y42 §6.2: triangle-level `common=0` universal for F0044/F0045/R0092) is unchanged at the triangle-level diff layer; PR-Y43's vertex-level diff IS dense for cohort (F0044/F0045 both show 50% Case B; cohort vertex-level methodology is durable). 

**Per `feedback_no_last_bug`**: 12th cycle. PR-Y43 does NOT close F0020 (unpaired count unchanged at 40). PR-Y43 produces the sharpest empirical anchor in the 12-cycle arc and refutes Option C pause for F0020 specifically. PR-Y44 with the (δ) sub-class probe + (α/γ) co-equal anchors is the next disciplined step; it may itself be another INFRA cycle (the (δ) probe) before a production fix is attempted.

### §5.3 Option C status — F0020 stays IN scope; cohort method-limit unchanged

PR-Y41/Y42 §6 framed Option C ("pause F0020 Render LOD") on the rationale that the defect was too diffuse / too far upstream for further Render LOD investigation. **PR-Y43's empirical 0% Case C directly refutes that framing for F0020 specifically.** F0020's defect is at (or just-pre-) the Render LOD layer; it is NOT upstream-of-Render-LOD-and-too-diffuse for measurement.

Audit position: **Option C is NOT triggered for F0020.** PR-Y44 should pursue the (δ) probe + (α/γ) co-equal anchors. The cohort F0044/F0045/R0092 Option C question is independently scoped and unaffected by PR-Y43 — the cohort triangle-level `common=0` method-limit is unchanged from PR-Y42 §6.2; cohort vertex-level investigation (B/D 50/50 split per Gate E) is a separate methodology decision for PR-Y45+ if the F0020 fix doesn't generalize.

**Per `feedback_phase1_diagnosis_ranking_is_inference`**: this Option C refutation IS measurement (Case C = 0/42 across all 5 canary reruns + all 4 adversary reruns at both 42-mode and 47-mode), not ranking. The strong-refutation framing is appropriate.

---

## §6 Banked / open (forward-carry from adversary §6 + canary §8)

### §6.1 Banked for PR-Y44

1. **(δ) Case D sub-class disambiguation probe** — NEW; PREREQUISITE; ~50 LOC harness extension. Per-Case-D `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` tuple emission to separate sub-class (a) `(3, _, _, _)` from sub-class (b) `(0 or 1, _, 2, _)`. Adjudicated §4.1.
2. **(α) F.0 `remove_winding_insensitive_duplicates` canary** — CO-EQUAL with (γ); bisect which of the 19 dropped tris bound the 24 (or 26) Case D entries. PR-Y40 §3.3 scaffold preserved.
3. **(γ) Pre-F.0 Boolean LOD → Render LOD re-tessellation canary** — CO-EQUAL with (α); 108-tri drop layer at `yang_integration.rs:1024`. PR-Y41 §6.3 banked-unprobed; PR-Y44 closes the gap.
4. **(β) F.3 `remove_nonmanifold_duplicates_aggressive`** — TERTIARY; bank for PR-Y45.
5. **Case B secondary anchor (10 distinct off-vertex positions, NOT 5)** — bank for PR-Y45 if PR-Y44 Case D fix doesn't close enough. Cohort F0044/F0045 also have 50% Case B; the Case B fix-shape may generalize.
6. **Cherchi non-det 42/47 mode pinning** — `TBB_NUM_THREADS=1` did not produce determinism in canary's OR adversary's runs. PR-Y44 should pin Cherchi tighter (single-thread without TBB altogether) for production-fix verification. Combined 8-rerun split is 50/50; use missing-count (deterministic in our runs) as the load-bearing gate, not extras.
7. **F0020 closure ceiling at ~20 unpaired even if PR-Y44 lands.** Cherchi well_formed=false for F0020 union means matching Cherchi exactly is bounded by Cherchi's own correctness on this case (PR-Y42 §6 + spec §9 caveat preserved). The other 20 unpaired edges are not Cherchi-only-attributable.

### §6.2 Open for PR-Y45+

1. **Cohort Case B/D semantics differ from F0020's.** F0020 Case D is "3-of-3 at 1× but tri missing" (canary inference); cohort Case D may be "1-or-2 at 1× + 1-or-2 at 5×" (residual catch-all) because cohort `common=0`. Need finer Case D sub-classification across F0020 vs cohort once (δ) lands.
2. **The 42 attributable tris vs the OTHER 152 missing tris.** PR-Y43 only classified the 42 that border unpaired edges. Are the other 152 also Case D-dominant or do they shift toward A/B? Banked for finer canary.
3. **PR-VIZ-3a yang debug capture** can render the 24 Case D triangles vs Waffle's 113 Render LOD triangles visually. Banked for PR-Y44 canary if (δ) probe output suggests visual disambiguation would help.
4. **Cohort F0044 / F0045 Case B mechanism generalization.** 8/16 + 2/4 entries are Case B; same off-vertex pattern as F0020. A Case B fix might generalize to cohort even if a Case D fix doesn't (cohort `common=0` means triangle-topology defects differ from F0020's).

### §6.3 Methodological banked

1. **Vertex-level diff IS the right grain for analytic-surface cohort cases.** Triangle-level diff has `common=0` method-limit (PR-Y42 finding); vertex-level diff (PR-Y43 contribution) IS dense. Future cohort canaries should default to vertex-level.
2. **The 4-grid-level sweep was useful.** Case A (4 tris) only manifests at the 5×/10× sweep — would have been missed with single-grid analysis. `feedback_multi_stage_anchor_probe` empirically vindicated.
3. **Case D was assumed unlikely in the plan** ("would mean the triangle exists but with different vertex INDICES that happen to coincide positionally; unlikely but should report"). Plans should not pre-judge case likelihoods; let the data speak. Per `feedback_phase1_diagnosis_ranking_is_inference`.
4. **Canary memo prose can drift from probe code** even when probe code is correct. Adversary §3.2 caught two framing/accounting defects (5-vs-10, Case D inference) and one narrative defect (75/25-vs-50/50). The independent re-run protocol is load-bearing.

---

## §7 Strategic-pivot status — B.1 ROI now unambiguously POSITIVE

PR-Y41 audit §0 fired the strategic-pivot trigger. PR-Y42 audit §0 framed B.1 ROI as **MIXED** at the close of cycle 11 — "paid off for F0020 specifically (50% sharp-but-borderline); METHOD-LIMITED at the cohort level". PR-Y43 advances the F0020 measurement strength from 50% borderline (PR-Y42) to 90% accountable (this PR), with Case C = 0 byte-stable across 9 combined reruns and a sharp PR-Y44 anchor menu with paper citations.

| Dimension | Pre-pivot (PR-Y41) | Pivot (PR-Y42) | Pivot-extension (PR-Y43) |
|---|---|---|---|
| F0020 attribution | "Missing 12 upstream" inference refuted | 50.0% borderline-sharp | **90% accountable** (D + B) |
| Cherchi well_formed caveat | N/A | Acknowledged | Acknowledged |
| Cohort triangle-level `common=0` | N/A | Method-limit discovered | Unchanged |
| Cohort vertex-level density | N/A | Untested | **Dense** (50/50 B/D for F0044/F0045) |
| PR-Y44 anchor sharpness | Strategic-pivot trigger | BORDERLINE + Option C disclosure | **D-dominant + 4 ranked candidates with citations** |
| Option C status (F0020) | TBD | TBD | **REFUTED** (Case C = 0 byte-stable) |
| Option C status (cohort) | TBD | TBD | Independently scoped; unaffected |
| Production-fix landing | 0 | 0 | 0 |
| Cumulative diagnostic LOC | ~1358 | ~1771 | **~2209** |

**Net B.1 strategic-pivot ROI verdict — POSITIVE for F0020, METHOD-LIMITED-but-extendable for cohort.** PR-Y42's MIXED framing is updated by PR-Y43 to POSITIVE on the F0020 axis (90% accountable, Case C refuted, 4 ranked PR-Y44 candidates). The cohort method-limit dimension (PR-Y42 §6.2 triangle-level `common=0` universal) is unchanged at the triangle-level diff layer; PR-Y43's vertex-level finding (F0044 50% B + 50% D, F0045 50% B + 50% D) demonstrates the methodology IS extendable to cohort at the vertex-level grain. Whether PR-Y44's (α/γ) Case D fix or PR-Y45's Case B fix generalizes to cohort is the open question that the next 2–3 cycles answer.

**Per `feedback_no_last_bug`**: PR-Y43 does NOT close F0020. PR-Y43 does NOT promise PR-Y44 will close F0020. PR-Y43 closes the strategic-pivot's "MIXED → POSITIVE" advancement for F0020 specifically and produces the sharpest PR-Y44 anchor menu of the 12-cycle arc. The (δ) probe-prerequisite framing in §4 enforces the no-last-bug discipline for the next cycle.

**Per `feedback_external_coherence`**: Cherchi C++ remains the load-bearing reference oracle; PR-Y43's A/B/C/D classification is a new lens on PR-Y42's same set-diff data — no new oracle, just a sharper read. The strategic-pivot prescription continues to deliver.

**Per `feedback_phase1_diagnosis_ranking_is_inference`**: the D-dominant verdict IS measurement (byte-stable across 9 reruns); the within-Case-D sub-mechanism is inference per defect 2; the (δ) probe in §4 is the disciplined sub-class disambiguation step before any production fix.

---

## §8 Final recommendation

**ACCEPT (SHIP-INFRA) — D-dominant + Case C = 0 stands; two canary framing defects reconciled in this memo; PR-Y44 anchor ranking adjusted to (δ)-prerequisite + (α/γ) co-equal + (β) tertiary + Case B PR-Y45 bank.**

Rationale:
- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec-y43 / canary-y43 / impl-y43 / adversary-y43). INFRA-CLASS test-author waiver consistent with Y29/Y33/Y36/Y37/Y38/Y40/Y41/Y42 precedent.
- **DoD §1.5 GREEN** — probe-off byte parity load-bearing; verified independently by canary Gate 2 + adversary Gate B against impl-y43 mirror. PR-Y31 hard gate `pr_y31_f0044_extras_zero` preserved (adversary Gate H).
- **INFRA-CLASS framing intact** — 0 LOC production logic; 0 kernel; 0 wasm-bridge; 0 app; only test-harness extension (+438 LOC at `cherchi_differential_diff.rs`) + memos. No WASM rebuild required.
- **A15.6 compliant** — paper-orthogonal Render LOD diff harness (Cherchi paper scope ends at arrangement output); A15.4/A15.5 unaffected; A15.6 Stage B byte-parity gate preserved.
- **Empirical evidence load-bearing** — F0020 A/B/C/D histogram byte-reproduced in adversary's 1/4 reruns (42-mode); 47-mode histogram byte-reproduced in adversary's 3/4 reruns; load-bearing invariants (B=14, C=0, D-dominant) byte-stable in BOTH modes across all 9 combined canary+adversary reruns.
- **Adversary corrections accepted as authoritative**: (1) Case B has 10 distinct off-vertex positions, NOT 5; 3 shared positions cover 7 entries; 7 entries unique. (2) Case D's 24 entries are a residual catch-all bucket whose sub-mechanism distribution is UNMEASURED; "3-of-3 at 1× / triangle missing" is plausible but inferred. (3) Cherchi non-det 42/47 mode split is ~50/50 across 8 combined reruns, NOT 75/25. None of these corrections alter the SHIP-INFRA verdict or the load-bearing histogram; they refine the PR-Y44 anchor ranking.
- **No-last-bug discipline GREEN** — 12 cycles, 0 production-fix LOC on F0020 Render LOD, F0020 Status:Failed unchanged at 40 unpaired. PR-Y43 produces the sharpest anchor menu of the arc and does NOT promise PR-Y44 will fix F0020. The (δ) probe-prerequisite framing in §4 enforces the discipline for next cycle.
- **D-dominant + Case C = 0 framing intact** — Case C byte-stable at 0 across all 9 combined reruns at both 42-mode and 47-mode is a strong measurement (not inference); the Option C refutation for F0020 specifically is sound. Cohort Option C status is independently scoped and unaffected.
- **Strategic-pivot ROI advanced from MIXED to POSITIVE** for F0020 axis; cohort method-limit-at-triangle-level is unchanged but **extendable at the vertex-level grain** (F0044/F0045 both show 50% Case B at the vertex level).
- **PR-Y44 anchor explicit** — `(δ)` Case D sub-class disambiguation probe is the PREREQUISITE Phase 1 measurement; `(α)` F.0 `remove_winding_insensitive_duplicates` and `(γ)` pre-F.0 Boolean LOD → Render LOD re-tessellation are CO-EQUAL fix candidates contingent on (δ)'s output; `(β)` F.3 `remove_nonmanifold_duplicates_aggressive` is TERTIARY; Case B is BANKED for PR-Y45.

**PR-Y44 anchor (definitive, for memory file's "PR-Y44 anchor" field):**
> **D-dominant + Case C = 0**. PR-Y44 Phase 1 ships (δ) Case D sub-class disambiguation probe (~50 LOC harness extension; per-Case-D `(match_at_1x, match_at_5x)` tuple emission). PR-Y44 Phase 2 canaries (α) F.0 `remove_winding_insensitive_duplicates` and (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024` as **co-equal** fix candidates contingent on (δ)'s sub-class proportion. (β) F.3 `remove_nonmanifold_duplicates_aggressive` is tertiary; Case B (10 distinct off-vertex positions, NOT 5) is banked for PR-Y45.

**Phase 8 push authorized.** Recommend:
1. Commit this audit memo + adversary memo + canary memo + spec + impl harness extension (`audit(yang-pr-y43): ACCEPT (SHIP-INFRA) — D-dominant + Case C = 0; framing defects reconciled; PR-Y44 (δ)+(α/γ) co-equal anchors`).
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y43_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, F0020 90% accountable, D-dominant + Case C = 0, PR-Y44 (δ)+(α/γ) co-equal anchor with adversary corrections (10 not 5 off-vert positions; Case D sub-mechanism inferred not measured; Cherchi non-det 50/50 not 75/25).
4. `TeamDelete pr-y43` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y44 should treat (δ) probe-extension as the PREREQUISITE Phase 1 measurement and (α/γ) as co-equal candidates contingent on (δ)'s output. The harness scaffold is durable reference infrastructure preserved regardless of PR-Y44's outcome.
