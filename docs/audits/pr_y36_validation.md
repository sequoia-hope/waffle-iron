# PR-Y36 Validation — Inverse-direction Render LOD probe; INFRASTRUCTURE-CLASS; **ACCEPT**

| Field | Value |
|---|---|
| Author | audit-y36 |
| Date | 2026-05-13 |
| Live tree HEAD | `d8fa288` (PR-Y36 impl; not pushed) |
| Parent (baseline) | `8778907` (PR-Y35.1 ACCEPT) |
| Class | **INFRASTRUCTURE-ONLY** — additive env-gated probe + spec + canary memo |
| FIP §5 | GREEN — 4-phase artifact set complete, role separation intact |
| DoD | GREEN — default-off byte parity verified (kernel lib 1262/24/42 exact; yang_fast 10/157 exact) |
| Verdict | **ACCEPT — authorize Phase 8 push and close-out** |

---

## §0 Single-paragraph verdict

PR-Y36 is an infrastructure-only PR that adds an additive, env-gated inverse-direction probe at the end of `tessellate_solid_bounded` and ships a canary memo plus spec; **no production logic is changed**. FIP §5 phase artifacts are present for all 4 expected phases (canary / spec / impl / adversary), with 4 distinct role-separated agents (no test-author phase, consistent with PR-Y29's infra-class pattern). DoD's load-bearing constraint — default-off byte parity — is verified independently by adversary Gates G (kernel lib 1262/24/42 exact baseline) and H (yang_fast 10/157 exact baseline). The empirical centerpiece — F0020 inv#6 attribution `total=39, D1a=9, D1b=0, D1c=0, D1d=8, OTHER=22` — is independently re-aggregated by adversary §3 with zero delta across all categories, including the load-bearing "D.1c=0% at HEAD" finding that refutes PR-Y28's dominant hypothesis. The INFRA-CLASS framing is honored: the PR-Y37 anchor is banked (canary §4.2 + spec §5.3), not shipped here; per `feedback_anchor_before_fix`'s strategic escalation rule, no fix shape is proposed without an empirical chain "fix → unpaired count to 0." `feedback_no_last_bug` is respected — all 4 grep hits in adversary Gate I.3 are explicit negations or benign Status-terminology substrings. **Recommendation: ACCEPT and authorize Phase 8 push.**

---

## §1 FIP §5 phase-artifact checklist

| Phase | Artifact | Path | Agent | Present |
|---|---|---|---|---|
| Canary | Inverse-direction probe + attribution memo | `docs/audits/pr_y36_canary.md` (412 LOC) | canary-y36 | ✓ |
| Spec | Probe design + 5th-refutation framing | `specs/yang_pr_y36_inverse_probe.md` (275 LOC) | spec-y36 | ✓ |
| Tests | (Not required — infra-class, no behavior change; FIP §4 satisfied by default-off byte parity gates G+H) | — | (none) | n/a |
| Implementation | Probe code applied to live tree, default-off byte-identical | `d8fa288` (`tessellation/mod.rs` +462/-3, +WASM rebuild, +memos) | impl-y36 | ✓ |
| Adversarial validation | Independent re-aggregation + 9-gate verification | `docs/audits/pr_y36_adversary.md` (192 LOC) | adversary-y36 | ✓ |
| Audit | This memo | `docs/audits/pr_y36_validation.md` | audit-y36 | ✓ (in progress) |

**No test-author phase.** PR-Y36 is INFRASTRUCTURE-CLASS with zero production logic change; FIP §4 regression coverage is satisfied by the existing kernel lib + yang_fast suites remaining GREEN (adversary Gates G + H, both PASS). This mirrors PR-Y29's precedent (Cherchi differential diff harness — also infra-class, also 4-phase, also no dedicated test-author). The plan §4 explicitly authorizes this framing.

---

## §2 Role separation verification

Four distinct agents produced four artifacts:

| Agent | Role | Artifact | Worktree |
|---|---|---|---|
| canary-y36 | Empirical probe build + attribution measurement + cohort sanity | `pr_y36_canary.md` | worktree-canary-y36 @ `8778907` |
| spec-y36 | Spec drafting + 5th-refutation framing | `yang_pr_y36_inverse_probe.md` | worktree (separate) |
| impl-y36 | Live-tree implementation commit + WASM rebuild | `d8fa288` | live tree main |
| adversary-y36 | Independent gate verification + paper-citation audit | `pr_y36_adversary.md` | non-destructive baseline worktree `/tmp/y36-adv-baseline` |

Per `feedback_oracle_credibility_via_role_separation`: oracle-build (canary) and oracle-interpret (adversary independent re-aggregation) are on different agents. Audit (this memo) weighs evidence; it does not re-run gates.

---

## §3 DoD checklist

| Item | Status | Evidence |
|---|---|---|
| Default-off byte parity (load-bearing for infra-class) | **GREEN** | Adversary Gate B: probe-off F0020 spotlight reproduces exact baseline (Status:Failed, 40 unpaired = 39 boundary + 1 NMM, 8 degenerate, 10 self-int). Adversary Gate G: `cargo test -p kernel --lib` = `1262 passed; 24 failed; 42 ignored` — exact match with `8778907`. Adversary Gate H: yang_fast = `10/157 passed, 139 failed, 8 errored` — exact match. |
| Probe is genuinely additive (not pre-existing) | **GREEN** | Adversary Gate F: `grep -c Y36_INVERSE_PROBE` against baseline `8778907`'s tessellation/mod.rs = `0`. |
| Commit hygiene (no `results.json` staged; explicit file list) | **GREEN** | `git show d8fa288 --stat` = 4 files (WASM binary + tessellation/mod.rs + 2 memos); `results.json` confirmed in unstaged side-effect tree per adversary §1, NOT in commit. |
| WASM rebuild bundled with Rust changes | **GREEN** | `app/static/pkg/wasm_bridge_bg.wasm` updated in same commit (4945236 → 5037814 bytes). |
| Verbatim git diff in impl report (`feedback_implementer_anti_fabrication_diff`) | **GREEN** | Adversary Gate A reproduces +462/-3 in tessellation/mod.rs; numstat confirms. |
| No destructive git on live tree (`feedback_adversary_no_destructive_git`) | **GREEN** | Adversary §1: baseline replay used `git worktree add`/`remove`, both non-destructive. |
| No "closes Yang" / "last-bug" language (`feedback_no_last_bug`) | **GREEN** | Adversary Gate I.3: 4 grep matches, all explicit negations or benign Status:Failed substring. |

All DoD gates GREEN.

---

## §4 Empirical evidence cross-check

The load-bearing empirical finding — F0020 inv#6 attribution table — must be reproduced independently by canary and adversary, byte-for-byte. This is the key claim PR-Y36 ships.

| Class | Canary §3.1 | Adversary §3 | Δ |
|---|---|---|---|
| Total | 39 | 39 | 0 |
| D.1a | 9 (23.1%) | 9 (23.1%) | 0 |
| D.1b | 0 (0.0%) | 0 (0.0%) | 0 |
| **D.1c** | **0 (0.0%)** | **0 (0.0%)** | **0** |
| D.1d | 8 (20.5%) | 8 (20.5%) | 0 |
| D.1 total | 17 (43.6%) | 17 (43.6%) | 0 |
| OTHER | 22 (56.4%) | 22 (56.4%) | 0 |

**Zero delta across all categories** under independent measurement. The probe is deterministic and reproducible. The load-bearing "D.1c=0% at HEAD" refutation of PR-Y28's dominant hypothesis is independently verified by adversary §3.

Cohort sanity (methodology validation) also reproduced independently: F0044, F0045, R0092 each attribute 100% OTHER and 0% D.1 (canary §3.3 = adversary §4 = `12/0`, `38/0`, `43/0`); adversary added R0045 as bonus (88/0). Methodology criterion ("if cohort cases attribute >50% to D.1, methodology is broken — REJECT") satisfied 4/4.

---

## §5 Architectural invariant compliance

**A15.6 (Hybrid Boolean Pipeline — Yang 2025).** The probe operates at `tessellate_solid_bounded`, which is the Render LOD layer **downstream** of the Yang pipeline's boolean output. Yang 2025 §4.4.1 (mesh updating) is cited in spec §5.3 as relevant *background* context for the PR-Y37 H1 sub-hypothesis, NOT as a claim that PR-Y36 itself applies §4.4.1; adversary Gate I.1 verified the citation is faithful to `refs/text/yang2025_hybrid_boolean.txt:605-610`.

**Render LOD is outside Cherchi 2022 paper scope.** Adversary Gate I.2 verified the spec's load-bearing claim — Cherchi 2022's scope ends at arrangement (§4) and Boolean classification (§5); there is no Render-LOD content. Therefore, per `feedback_external_coherence`, no external reference exists for the Render LOD layer, and the in-situ inverse probe IS the empirical reference. This framing is sound: the PR is not attempting to substitute an internal oracle for a missing external reference — it is *building* the empirical reference for a layer where no paper exists.

**Banked clerical finding (adversary §5.2 / §6.2):** spec §2.1 references "Cherchi 2022 §3" where it should be §4 (the arrangement chapter). The load-bearing claim (no Render-LOD content in Cherchi 2022) is correct; the section-number imprecision is clerical and does not warrant rejection. Re-banked for future spec hygiene.

**A15 (Analytical Primacy) is not in scope** — the probe is observation-only and does not alter the analytical/mesh role assignment for any surface.

---

## §6 INFRA-CLASS framing audit

The plan, spec, canary, adversary, and commit message all consistently frame PR-Y36 as INFRA-CLASS / SHIP-INFRA-ONLY. Three sub-claims to verify:

| Sub-claim | Status | Evidence |
|---|---|---|
| No production fix shape shipped (probe is default-off byte-identical) | **VERIFIED** | Adversary Gates G + H exact-baseline match. Probe code wrapped `if y36_on { … }` per canary §2.3 and spec §3.5. |
| PR-Y37 anchor banked, not shipped | **VERIFIED** | PR-Y37 anchor recommendations live in canary §4.2 and spec §5.3 / §5.4. Commit message body explicitly labels "PR-Y37 banked" and "Alternative narrower PR-Y37 banked." Zero production fix code shipped. |
| Strategic escalation rule honored | **VERIFIED** | Per `feedback_anchor_before_fix`, "three wrong anchors → stop bisecting, build a reference comparison." PR-Y36 is the 5th investigational PR on F0020 Render LOD (Y25/Y26/Y27/Y28 all canary-stage ABORTed; PR-Y29 pivoted to boolean pipeline). Spec §2.1 + canary §4.1 + commit message all explicitly invoke this rule. No empirical chain "fix → unpaired count to 0" is claimed; therefore no fix shape is proposed. |

The infra-class framing is internally consistent across all 5 artifacts (plan, spec, canary, impl commit, adversary memo).

---

## §7 Banked findings disposition

The following banked findings carry forward beyond PR-Y36; none block the SHIP-INFRA verdict:

1. **PR-Y37 anchor candidates** (canary §4.2, §4.2-alt; spec §5.3, §5.4):
   - **Primary (canary recommendation):** investigational canary on the OTHER cluster (22/39 = 56.4%); resolve into H1 (sub-grid seam) / H2 (NMM-pair render asymmetry) / H3 (new sub-mechanism); cross-check against F0044/F0045 (50 edges) and R0092 (43 edges). ~80–150 LOC infra-class, no production fix.
   - **Narrower alternative:** D.1d kids (218, 232, 233) survival fix at `tessellation/repair.rs:585`; ~30–80 LOC production change; predicted F0020 outcome 40 → ~32 unpaired (does NOT close Status:Failed); cohort regression risk on F0044/F0045 (D.2 invariant must be verified preserved).
   - Per `feedback_adversary_recommendations_need_canary`: both are candidate anchors requiring in-situ canary verification, not directive.

2. **OTHER cluster (22/39 = 56.4%)** is the new dominant unknown at HEAD; not in PR-Y28's D.1 framework. 11 partial-NMM kept-face attributions (50–69% NMM: kids 226, 229, 231) + 11 zero-NMM larger-boundary kept-face attributions. Banked for PR-Y37 investigation.

3. **D.1c is empirically dead at HEAD** (0% attribution). PR-Y28's dominant 48-tri D.1c cluster resolved by post-Y34/Y35/Y35.1 byte-parity work. β-shape (peer-patch synthesis) is therefore empirically unsupported as a PR-Y37 anchor.

4. **Arena topology shift** 33 → 65 faces between PR-Y28 (2026-05-08) and HEAD (2026-05-13) — by-product of correct upstream byte-parity work, not a regression. Per `feedback_no_regression_chasing`, do not chase the 36 → 40 unpaired increase as a regression.

5. **Cherchi C++ TBB non-determinism** — banked since PR-Y31, not in PR-Y36 scope.

6. **F.4-grid vs oracle-grid 1-edge discrepancy** (probe reports 39, oracle reports 40 = 39 + 1 NMM). Documented in canary §6 and spec §7.3; banked for PR-Y37 probe extension (triple-bucket `count<2 / count==2 / count>2`).

7. **Adversary clerical findings:** (a) missing `spotlight_r0092` test — R0092 coverage is incidental; (b) spec §2.1 Cherchi §3 vs §4 imprecision; (c) probe writes f64 boundary positions but final attribution uses f32 quantization — banked for PR-Y37 probe-extension consistency. None of (a)/(b)/(c) blocks ACCEPT.

8. **Open work items beyond PR-Y36** (per commit message + plan §6): F0020 Render LOD Status:Failed remains; F0045 tessellation-grid divergence; R0092 NMM-edge tessellation; 139 still-failing yang_fast cases. PR-Y36 explicitly does not address any of these. Per `feedback_no_last_bug`, no "closes Yang" claim is made.

---

## §8 Final recommendation — **ACCEPT**

PR-Y36 satisfies FIP §5 (4 phase artifacts present, 4 distinct role-separated agents; no test-author phase consistent with PR-Y29's infra-class precedent), DoD (default-off byte parity verified independently by adversary Gates G + H; commit hygiene clean; WASM rebuild bundled; verbatim diff per `feedback_implementer_anti_fabrication_diff`), and A15 architectural framing (Render LOD outside Cherchi 2022 scope; probe IS the empirical reference per `feedback_external_coherence`; Yang §4.4.1 cited as background context only). The load-bearing empirical claim — F0020 inv#6 attribution table — is independently re-aggregated by adversary with zero delta across all categories; the "D.1c=0% at HEAD" refutation of PR-Y28's dominant hypothesis is reproduced under independent measurement; cohort sanity validates the probe methodology 4/4 (F0044/F0045/R0092 from canary + R0045 adversary bonus, all 100% OTHER and 0% D.1). The INFRA-CLASS framing is honored: no production fix shipped, PR-Y37 anchor banked, strategic escalation rule observed.

**Recommendation: ACCEPT — authorize Phase 8 close-out (audit-memo commit, push origin main per `feedback_always_push`, memory entry, `TeamDelete`).** Banked findings are clerical or carried forward to PR-Y37; none block the SHIP-INFRA verdict.

Phase 8 close-out must:
1. Commit this audit memo as `audit(yang-pr-y36): ACCEPT — inverse-direction probe validated`.
2. Plain `git push origin main` (NOT force-push, per `feedback_always_push`).
3. Add memory entry `yang_pr_y36_shipped.md` + one-line MEMORY.md index. Memory MUST explicitly state: "INFRASTRUCTURE-CLASS, no production fix; PR-Y37 anchor banked at OTHER cluster investigation (or D.1d narrower alternative)."
4. `TeamDelete` per `feedback_per_plan_cycle_team`.
5. NO "closes Yang" / "last gap" language in any artifact.

End of memo.
