# PR-Y36 Adversary — Inverse-direction probe; INFRA-CLASS; **ACCEPT**

| Field | Value |
|---|---|
| Author | adversary-y36 |
| Date | 2026-05-13 |
| Live tree HEAD | `d8fa288` (PR-Y36 impl; not pushed) |
| Parent (baseline) | `8778907` (PR-Y35.1 ACCEPT) |
| Class | **INFRASTRUCTURE-ONLY** — probe + memo + spec; no production logic |
| Verdict | **ACCEPT** — 9/9 gates GREEN; independent attribution byte-for-byte matches canary §3; methodology validated by 4-case cohort (F0044/F0045/R0092/R0045); zero destructive git on live tree |

---

## §0 Single-paragraph verdict

PR-Y36 ships infrastructure only — an additive, env-gated probe in `crates/kernel/src/tessellation/mod.rs::tessellate_solid_bounded` that classifies each F0020 final-mesh unpaired edge against PR-Y28's D.1 sub-mechanism taxonomy. Independent re-aggregation of the inv#6 attribution TSV matches the canary's load-bearing numbers to the edge: 39 total, D.1a=9, D.1b=0, D.1c=0, D.1d=8, OTHER=22. The "D.1c=0% at HEAD" finding (the canary's load-bearing refutation of PR-Y28's dominant hypothesis) is independently reproduced. All four cohort cases (F0044/F0045/R0092 from the `spotlight_f0044` test-harness path + a bonus R0045 spotlight) attribute 100% OTHER and 0% D.1 — methodology validated. Default-off byte parity holds (Gate G: kernel lib 1262/24/42 exact baseline; Gate H: yang_fast 10/157 exact baseline). The probe is genuinely additive (Gate F: 0 occurrences of `Y36_INVERSE_PROBE` in baseline `8778907`). No "closes Yang" / "last gap" language is introduced; all matches in Gate I.3 are explicit negations enforcing `feedback_no_last_bug`. **Recommendation: ACCEPT.**

---

## §1 Discipline — non-destructive git proof

Live tree at session end:

```
$ git -C /home/claude/workspace status
On branch main
Your branch is ahead of 'origin/main' by 1 commit.
  modified:   app/tests/cases/assay/results.json

$ git -C /home/claude/workspace log --oneline -3
d8fa288 infra(yang-pr-y36): inverse-direction Render LOD probe for F0020 attribution | INFRASTRUCTURE-ONLY, no production fix
8778907 audit(yang-pr-y35-1): ACCEPT — triangulation gate widening validated
0d93b8d feat(yang-pr-y35-1): widen triangulation gate for edge2pts-driven conformal subdivision | re-enables test_subdivision_shared_edge_split_propagation
```

`results.json` is the test-harness runner's existing side-effect (also visible in the canary's §1 worktree status). No `git stash`, `git checkout <ref>`, `git reset`, or `git restore` on the live tree at any point during the adversary session. Per `feedback_adversary_no_destructive_git`.

Baseline replay (Gate F) used `git worktree add -f /tmp/y36-adv-baseline 8778907` (read-only checkout into a separate worktree) followed by `git worktree remove /tmp/y36-adv-baseline --force` — both non-destructive operations on the live tree.

---

## §2 Gate-by-gate verification

| Gate | Spec | Result | Evidence |
|---|---|---|---|
| **A** | Commit shape: 4 files, +462/-3 in tessellation/mod.rs; results.json NOT staged | **PASS** | `git show d8fa288 --stat` = 4 files (wasm_bridge_bg.wasm, tessellation/mod.rs +465/-3, canary memo +412/-0, spec +275/-0); numstat confirms +462/-3 in tessellation/mod.rs |
| **B** | Probe-off F0020 baseline: Status=Failed, 40 unpaired (39 boundary + 1 NMM), 8 degenerate, 10 self-int | **PASS** | `/tmp/y36-adv-probeoff.log`: `Status: Failed` / `watertight_mesh: 40 unpaired edges out of 188 total (39 boundary, 1 non-manifold); no_degenerate_triangles: 8 of 113; no_self_intersection: 10 inter-face triangle penetrations` |
| **C** | Probe-on fires; TSVs per invocation | **PASS** | 12 TSV files in `/tmp/y36-adv-probe/` (6 attribution + 6 inventory across F0020 inv#1..#6); summary lines reproduce canary §3.2 byte-identically |
| **D** | Independent inv#6 attribution within ±2 of canary §3 | **PASS** | See §3 below — perfect match: D.1a=9, D.1b=0, D.1c=0, D.1d=8, OTHER=22, total=39 |
| **E** | Cohort sanity — F0044/R0045 majority OTHER, 0% D.1; methodology validation | **PASS** | All 4 cohort cases 100% OTHER, 0% D.1 — see §4 |
| **F** | Baseline replay confirms probe is additive (not pre-existing) | **PASS** | `grep -c Y36_INVERSE_PROBE` in `8778907`'s tessellation/mod.rs = **0** |
| **G** | kernel lib full suite no regression vs `8778907` (1262/24/42) | **PASS** | `1262 passed; 24 failed; 42 ignored; 0 measured; 0 filtered out; finished in 12.92s` — exact match |
| **H** | yang_fast ≥10/157 | **PASS** | `Yang fast: 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)` — exact match |
| **I** | Paper-grounding audit: Yang §4.4.1 + Cherchi §3 + no-last-bug grep | **PASS** | See §5 below |

---

## §3 Independent attribution aggregation (Gate D)

Independent re-aggregation of `/tmp/y36-adv-probe/F0020_inv006_inverse_attribution.tsv`:

```
$ head -1 .../F0020_inv006_inverse_attribution.tsv | tr '\t' '\n' | nl
     1  unpaired_edge_id
     ...
    10  classification
     ...

$ tail -n +2 .../F0020_inv006_inverse_attribution.tsv | wc -l
39

$ tail -n +2 .../F0020_inv006_inverse_attribution.tsv | awk -F'\t' '{print $10}' | sort | uniq -c
      9 D1a
      8 D1d
     22 OTHER
```

| Class | Canary §3 | Adversary (independent) | Δ |
|---|---|---|---|
| Total | 39 | 39 | 0 |
| D.1a | 9 (23.1%) | 9 (23.1%) | 0 |
| D.1b | 0 (0.0%) | 0 (0.0%) | 0 |
| D.1c | 0 (0.0%) | 0 (0.0%) | 0 |
| D.1d | 8 (20.5%) | 8 (20.5%) | 0 |
| D.1 total | 17 (43.6%) | 17 (43.6%) | 0 |
| OTHER | 22 (56.4%) | 22 (56.4%) | 0 |

**Perfect agreement, zero delta across all categories.** The adversary's independent aggregation byte-for-byte matches the canary's primary finding.

**Critical claim re-verified:** D.1c = 0% at HEAD. PR-Y28's dominant hypothesis (the 48-tri D.1c cluster) is empirically absent from the source-attribution set for F0020 inv#6's unpaired edges. The canary's load-bearing "5th-refutation framing" is supported by adversary-independent measurement.

The stderr summary line from the adversary's probe run also reproduces verbatim:

```
[y36-inverse-probe] case=F0020 inv#6 total_unpaired=39 D1a=9 D1b=0 D1c=0 D1d=8 OTHER=22 wrote=/tmp/y36-adv-probe/F0020_inv006_inverse_attribution.tsv
```

The probe is deterministic and reproducible.

---

## §4 Cohort sanity (Gate E) — methodology validation

The `spotlight_f0044` test-harness path drives F0044, F0045, and R0092 sequentially within a single test run (3 invocations of `tessellate_solid_bounded`). Adversary also ran the `spotlight_r0045` test for additional cohort coverage. All 4 cases:

| Case | inv# | Total unpaired | D.1a | D.1b | D.1c | D.1d | OTHER | D.1 % |
|---|---|---|---|---|---|---|---|---|
| F0044 | 1 | 12 | 0 | 0 | 0 | 0 | 12 | **0%** |
| F0045 | 2 | 38 | 0 | 0 | 0 | 0 | 38 | **0%** |
| R0092 | 3 | 43 | 0 | 0 | 0 | 0 | 43 | **0%** |
| R0045 | 1 | 88 | 0 | 0 | 0 | 0 | 88 | **0%** |

Adversary independently re-aggregated each cohort case's TSV's classification column:

```
$ for f in /tmp/y36-adv-cohort/*_inverse_attribution.tsv; do
    tail -n +2 $f | awk -F'\t' '{print $10}' | sort | uniq -c
  done
# F0044: 12 OTHER
# F0045: 38 OTHER
# R0045: 88 OTHER
# R0092: 43 OTHER
```

**100% OTHER across all four cohort cases, 0% to any D.1 sub-mechanism.** The methodology criterion ("If any cohort case attributes >50% to D.1, methodology is broken — REJECT") is fully satisfied. The probe correctly distinguishes drop-source mechanisms (D.1) from kept-but-unpaired mechanisms (OTHER). The PR-Y27 cohort split (D.1 = F0020-only, D.2 = F0044+F0045, D.3 = R0092) survives at HEAD.

**Methodology gap noted (banked):** there is no dedicated `spotlight_r0092` test in `crates/test-harness/tests/assay_randomized.rs`. R0092 coverage comes incidentally from `spotlight_f0044`'s 3-case run. This is a banked methodology observation, not a defect — the data is available; the test-file organization is implicit.

---

## §5 Paper-grounding audit (Gate I)

### §5.1 Yang §4.4.1 citation

Spec §5.3 cites Yang §4.4.1 ("mesh updating prescribes re-mesh-along-refined-curves") as the paper-prescribed context for the OTHER cluster's H1 sub-hypothesis. Adversary verified `refs/text/yang2025_hybrid_boolean.txt:605-610`:

> 4.4.1 Mesh updating. As the intersections on the surfaces are relocated and refined during the optimization, the bijectivity is essentially broken. Each intersection curve is no longer mapped to the corresponding intersection curve between the two meshes, thus causing gaps or self-intersections.

The spec's framing — "Yang §4.4.1 mesh-updating prescribes re-mesh-along-refined-curves to keep bijectivity across optimization-shifted intersection curves" — is faithful to the paper's text. The paper does NOT prescribe Render-LOD fixes per se; the spec correctly frames the citation as relevant *background* for PR-Y37's H1 sub-hypothesis, not as a claim that PR-Y36 itself applies §4.4.1. Per `feedback_yang_only`, no §X claims are made on behalf of PR-Y36's own changes.

### §5.2 Cherchi §3 citation

Spec §2.1 claims "Cherchi 2022 §3 paper scope ends at the arrangement output; Render LOD is downstream and unreferenced." Adversary verified `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-320`: §3 is the numerical-models background; §4 is the mesh arrangement (the spec's "§3" is a slightly imprecise label for the §4 arrangement section, but the substantive claim — that Cherchi 2022's scope ends at arrangement output and does NOT cover Render LOD — is empirically correct. The paper's §4 explicitly defines arrangement as "Detect Intersections → Insert Points → Insert Segments → Output", and §5 covers Boolean classification. There is no Render-LOD content in Cherchi 2022.

**Banked finding:** the §3 vs §4 reference in spec §2.1 is slightly imprecise. The load-bearing claim (no Render-LOD reference exists in Cherchi 2022, so the in-situ probe IS the empirical reference) is correct.

### §5.3 No-last-bug grep

```
$ grep -i "closes yang\|last gap\|status.*pass\|fixes yang" specs/yang_pr_y36_inverse_probe.md docs/audits/pr_y36_canary.md
specs/yang_pr_y36_inverse_probe.md:- **NOT a "this closes Yang" claim.** Per `feedback_no_last_bug`. The OTHER cluster is the largest unknown.
specs/yang_pr_y36_inverse_probe.md:No "closes Yang" / "last gap" / "Status flips to Pass" language is made in this spec or in the canary memo.
docs/audits/pr_y36_canary.md:| **Gate 2** | F0020 spotlight re-confirm baseline (oracle 40 unpaired / E_lod conformal 56 unpaired / Status:Failed) | **PASS** — exact match with brief's Phase 1 re-measurement |
docs/audits/pr_y36_canary.md:- **NOT a "this closes Yang" claim.** Per `feedback_no_last_bug`. The OTHER cluster is the largest unknown.
```

All 4 matches inspected:

1. spec line 1: **NEGATION** — "NOT a 'this closes Yang' claim" enforces the rule.
2. spec line 2: **NEGATION** — "No 'closes Yang' / 'last gap' / 'Status flips to Pass' language is made" enforces the rule.
3. canary line: substring match on "Status:Failed" (PR status of F0020 spotlight test, NOT a claim of "status flips to pass"). Adversary classifies as benign.
4. canary line: **NEGATION** — same as spec line 1.

**Zero violations.** All matches are explicit negations enforcing `feedback_no_last_bug` or benign substring matches on the PR status terminology (`Status:Failed`). Per the brief's "zero hits expected" — adversary classifies this as effectively zero (4 matches, all explicit negations / benign).

---

## §6 Banked findings

1. **Methodology gap (low impact, banked for PR-Y37):** there is no dedicated `spotlight_r0092` test in `assay_randomized.rs`. R0092 coverage at the cohort sanity gate comes incidentally from `spotlight_f0044`'s sequential 3-case run. Future PRs investigating R0092 should consider adding an explicit spotlight test for clarity. The current PR-Y36 cohort sanity data is unaffected (R0092's 43-edge OTHER attribution is captured).

2. **Cherchi §3 vs §4 spec citation imprecision (clerical, banked):** spec §2.1 references "Cherchi 2022 §3 paper scope ends at the arrangement output" — the arrangement is actually §4, not §3 (Cherchi 2022's §3 is numerical-models background). The substantive load-bearing claim (no Render-LOD content in Cherchi 2022; the in-situ probe IS the empirical reference) is correct.

3. **R0045 cohort bonus (informational):** `spotlight_r0045` was run independently and produced 88 unpaired edges, 100% OTHER, 0% D.1 — additional cohort data point supporting the methodology validation. Not in canary's §3.3 table; banked as supplementary evidence.

4. **Banked PR-Y37 anchor recommendations are inference, not observation:** per `feedback_adversary_recommendations_need_canary`, the canary's PR-Y37 anchor recommendation (extending the probe to classify the OTHER cluster's H1/H2/H3 sub-hypotheses) is a candidate anchor pending in-situ canary verification. The adversary's role is to verify PR-Y36's empirical claims, not to second-guess PR-Y37 scoping. No PR-Y37 fix-shape recommendations are made.

5. **Probe writes f64 boundary positions but final attribution uses f32 quantization:** subtle data flow noted while reading the probe code. Default-off byte parity holds (verified), but PR-Y37's extended probe should ensure consistent quantization between dispatch-time capture and emission-time attribution. Low risk for PR-Y36; banked for the PR-Y37 extension.

6. **F.4-grid vs oracle-grid 1-edge discrepancy is acknowledged in both canary §6 and spec §7.3 as a benign banked finding.** Adversary re-affirms: the discrepancy (probe reports 39; oracle reports 40 with the 1 extra NMM edge under count!=2 vs count==1 dispatch) does not affect verdict. Documented honestly.

---

## §7 Recommendation — **ACCEPT**

All 9 verification gates GREEN. Independent re-aggregation of attribution data byte-for-byte matches canary §3 across all categories (zero delta). The D.1c=0% load-bearing finding (canary's "5th refutation" of PR-Y28's dominant hypothesis) is reproduced under independent measurement. Cohort sanity validates the probe's methodology (4/4 cases attribute majority to OTHER, 0% to D.1). Default-off byte parity is preserved (kernel lib 1262/24/42 exact; yang_fast 10/157 exact). The probe is genuinely additive (0 occurrences in `8778907` baseline replay). No "closes Yang" language; all grep matches are explicit negations or benign PR-status terminology.

The PR is INFRASTRUCTURE-CLASS — zero production logic changes, additive env-gated probe code only. Per `feedback_anchor_before_fix` strategic escalation, this is the right next step after 4 consecutive canary ABORTs: data collection that materially shrunk the candidate space (PR-Y28's D.1c is empirically dead at HEAD; OTHER is the new dominant unknown; PR-Y37 has a load-bearing data foundation).

**Recommendation: ACCEPT.** Banked findings are clerical/methodological (Cherchi §3 vs §4 citation imprecision; missing `spotlight_r0092` test; PR-Y37 probe-quantization consistency note) and do not block the SHIP-INFRA verdict.

End of memo.
