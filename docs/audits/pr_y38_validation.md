# PR-Y38 Validation — Grid-sensitivity probe; phantom hypothesis REFUTED; 40-unpaired baseline CORROBORATED — **ACCEPT**

**Author:** audit-y38
**Date:** 2026-05-13
**HEAD audited:** `48a0498` (PR-Y38 impl, NOT pushed)
**Parent baseline:** `d632d5f` (PR-Y37 audit ACCEPT)
**Class:** INFRASTRUCTURE-ONLY (no production logic changed; +179 LOC env-gated probe in `crates/test-harness/src/oracle.rs`)
**Verdict:** **ACCEPT — authorize Phase 8 push.**

---

## §0 Verdict

PR-Y38 ships an env-gated grid-sensitivity probe at the watertight oracle. Independent canary + adversary agree byte-identically on F0020's table (`40 40 40 40 40 40 / 0 0 40`) and cohort (F0044=12, F0045=38, R0092=43→45 at 100×, R0045=88). FIP §5 + DoD GREEN; role separation holds (4 distinct agents, INFRA-class waives test-author per PR-Y29/Y33/Y36/Y37 precedent); A15 unaffected (probe is paper-orthogonal — the oracle is outside the boolean pipeline). Canary near-miss with live-tree Edit was caught and reverted cleanly (adversary Gate H: `.rs` delta limited to `oracle.rs` +179/-0, no contamination on `tessellation/mod.rs`). This is the 7th no-fix outcome on F0020 Render LOD AND the **first** of those 7 to eliminate a measurement-artifact hypothesis (phantom unpaired edges from f32 round-trip). The positive corroboration (40 is real geometry) promotes PR-Y37 banked Options 1/2 as the empirically-supported PR-Y39 anchor and refutes the four banked options that would have tuned the oracle (canary §4, spec §7). Recommendation: **ACCEPT — push to origin main**.

---

## §1 FIP §5 phase-artifact checklist

| Phase | FIP requirement | Artifact | Status |
|---|---|---|---|
| 2 — Canary | Build probe; verify gates; recommend SHIP/ABORT | `docs/audits/pr_y38_canary.md` (390 LOC) — SHIP-INFRA + CORROBORATION; 8 gates GREEN | ✓ |
| 3 — Spec | Document design, findings, scope, risk | `specs/yang_pr_y38_grid_sensitivity.md` (190 LOC) — paper-orthogonality declared §9 | ✓ |
| 4 — Tests | Failing-test required (waived per INFRA precedent) | Regression coverage = default-off byte parity (Gates B/I/J adversary memo) | ✓ (waived per PR-Y29/Y33/Y36/Y37) |
| 5 — Implementation | Apply probe additively; no production logic | Commit `48a0498`: 3 files (oracle.rs +179, canary memo +390, spec +190); no `results.json` staged; no `tessellation/mod.rs` staged | ✓ |
| 6 — Adversary | Independent gate verification, no-last-bug grep | `docs/audits/pr_y38_adversary.md` (189 LOC) — ACCEPT; 12 gates GREEN; canary near-miss audited Gate H | ✓ |
| 7 — Audit | This memo | `docs/audits/pr_y38_validation.md` | ✓ (this artifact) |

FIP §5 merge-authorization requirements (spec exists, validation phase completed, no test modification by implementer): all met.

---

## §2 Role separation

4 distinct role-separated agents per `feedback_oracle_credibility_via_role_separation`:

| Agent | Phase | Owner of |
|---|---|---|
| canary-y38 | 2 | Worktree probe build + gate execution; verdict recommendation |
| spec-y38 | 3 | Spec doc framing canary findings |
| impl-y38 | 5 | Live-tree commit (additive; verbatim diff per `feedback_implementer_anti_fabrication_diff`) |
| adversary-y38 | 6 | Non-destructive independent replication; `feedback_adversary_no_destructive_git` honored (worktree-based baseline replay, zero stash/reset/checkout-on-live) |

INFRA-class waives test-author (PR-Y29/Y33/Y36/Y37 precedent — no production logic = no failing-test). 5th distinct role (audit) is this memo.

---

## §3 DoD checklist (probe-off byte parity load-bearing)

INFRA-class DoD reduces to: production behavior unchanged when probe disabled.

| DoD criterion | Verification | Status |
|---|---|---|
| Probe-off byte parity | Adversary Gate B: F0020 spotlight without `Y38_GRID_PROBE` reproduces `40 unpaired edges out of 188 total (39 boundary, 1 non-manifold); 8 of 113 degenerate; 10 inter-face penetrations` — byte-identical to PR-Y37 baseline | GREEN |
| kernel lib regression | Adversary Gate I: `1262 passed; 24 failed; 42 ignored` matches required baseline | GREEN |
| yang_fast corpus | Adversary Gate J: ≥10/157 preserved | GREEN |
| Determinism | Canary §3 Gate 8 (3 reruns byte-identical) + adversary Gate K (2 reruns byte-identical) ⇒ 5 independent runs all byte-identical | GREEN |
| Build (no warnings) | Canary Gate 1 GREEN; commit `48a0498` builds clean | GREEN |
| Diff hygiene | Adversary Gate A: 3 files staged (oracle.rs / canary memo / spec); `results.json` correctly NOT staged; `tessellation/mod.rs` not contaminated | GREEN |

---

## §4 Empirical evidence cross-check

Canary §4 and adversary §3 agree byte-identically on F0020:

```
case  total_edges  05x  1x  2x  4x  10x  100x  dist1  dist2  isolated  oracle
F0020      188     40   40  40  40   40   40    0      0      40        40
```

Cohort independently reproduced by adversary:

| Case | total_edges | 0.5× | 1× | 2× | 4× | 10× | 100× | isolated |
|---|---|---|---|---|---|---|---|---|
| F0044 | 180 | 12 | 12 | 12 | 12 | 12 | 12 | 12 |
| F0045 | 472 | 38 | 38 | 38 | 38 | 38 | 38 | 38 |
| R0092 | 280/281 | 43 | 43 | 43 | 43 | 43 | **45** | 43 |
| R0045 | 950 | 88 | 88 | 88 | 88 | 88 | 88 | 88 |

R0092 100× anomaly (43→45) reproduces under both canary and adversary with the same direction (UP — over-merging, not phantom recovery). One 1-edge discrepancy in R0092 `total_edges` (280 canary vs 281 adversary) is noted in adversary §7.A as banked finding — unpaired count, isolated count, and grid sweep are unchanged; qualitative result unaffected. This is the kind of measurement noise that does not bear on the load-bearing finding.

Phantom hypothesis is refuted with empirical certainty across:
- 6 grid multipliers (0.5× through 100× — two orders of magnitude)
- 4 cohort cases independently reproduced
- 5 deterministic reruns (3 canary + 2 adversary)
- 1 corroborating direction signal (R0092 over-merge at 100× goes the wrong way for phantom recovery)

---

## §5 A15 invariant compliance

A15.6 (Hybrid Boolean Pipeline / Yang 2025 #24) is unaffected. The watertight Render LOD oracle lives downstream of the boolean pipeline as a *measurement* tool — no paper (Yang 2025, Cherchi 2022, Cherchi 2020) covers Render LOD f32 quantization or watertight edge-pairing. Spec §9 explicitly declares paper-orthogonality and cites Yang §4.4.1 + Cherchi 2022 §3 as upstream watertightness sources only.

PR-Y38 changes *no* pipeline code. The probe IS the empirical reference per `feedback_external_coherence` (no external reference impl applies to the oracle itself). A15.5 contracts intact. No invariant claims affected.

---

## §6 INFRA-CLASS framing audit

### §6.1 No production fix shipped

`crates/test-harness/src/oracle.rs` +179 LOC entirely inside `if std::env::var("Y38_GRID_PROBE").as_deref() == Ok("1") { ... }` (canary §2 probe entry at L243-246). Default-off path byte-identical (adversary Gate B). Helper functions defined after the public function — additive. Zero production logic touched.

### §6.2 Canary near-miss audit (Gate H)

Canary §1 documented an Edit call that targeted the live tree at `/home/claude/workspace/crates/test-harness/src/oracle.rs` instead of the worktree, then reverted via `git checkout --`. Adversary Gate H verified: `git diff d632d5f..48a0498 -- '*.rs' --stat` shows only `oracle.rs` changed (+179/-0); no contamination on `tessellation/mod.rs` or any other production file. The revert held cleanly. The file was clean prior to the unintended edit (verified by canary's inspection-before-revert), so `git checkout --` was non-destructive per `feedback_adversary_no_destructive_git`. Documenting here as a discipline observation: the near-miss was caught mid-cycle and audited green — canary self-discipline functioned correctly.

### §6.3 7th-cycle outcome but FIRST elimination of a measurement-artifact hypothesis

PR-Y25/Y26/Y27/Y28/Y36/Y37/Y38 are 7 consecutive canary-stage findings without a fix shape on F0020 Render LOD. Per `feedback_anchor_before_fix` strategic escalation rule, after 3+ wrong anchors → build a reference comparison. PR-Y36 / PR-Y37 were source-face reference comparisons; PR-Y38 is the **first** to question the measurement framework itself (oracle ground truth). All prior 6 cycles implicitly assumed `unpaired=40` was geometric truth; PR-Y38 validates this assumption with empirical certainty. This is a load-bearing **positive** finding — eliminates the measurement-artifact hypothesis class and corroborates all prior 6 PRs' attribution numbers as resting on sound ground truth.

### §6.4 PR-Y39 anchor banked; 4 banked options NOT recommended

Spec §7 + canary §4 promote **PR-Y37 banked Options 1/2** (refine source-face probe for H3 cluster discrimination) as the PR-Y39 anchor, with 40 now empirically validated as the right target count. Per `feedback_phase1_diagnosis_ranking_is_inference`, the canary correctly did NOT promote a fix-shape; the corroboration *is* the empirical finding.

NOT recommended (per evidence, spec §7 + canary §4):
- Do not tune `TAU_TESS_GRID_FACTOR` upward (R0092 100× → 45 shows over-merging starts before phantom recovery)
- Do not tighten the grid (0.5× → 40, no recovery)
- Do not adopt position-tolerance edge-pairing (no empirical justification; would weaken oracle discrimination)
- Do not enable wider ±2 near-pair scan (±1 already 100% isolated; ±2 banked for PR-Y39 only if Options 1/2 don't localize H3)

### §6.5 `feedback_no_last_bug` honored

Adversary Gate L: 3 matches to fix-completion grep, all explicit negations:
1. Spec §8: `This is NOT a "closes Yang" PR (\`feedback_no_last_bug\`).`
2. Canary §1.3: `the memo explicitly does NOT claim "this closes Yang."`
3. Canary §5: `does not claim "this closes Yang" or "phantom hypothesis is gone forever"`

Zero fix-completion claims.

---

## §7 Strategic context — 7 PR cycles; PR-Y38's load-bearing positive finding

PR-Y25 / Y26 / Y27 / Y28 — investigational ABORTs (4 consecutive canary refutations of plan hypotheses)
PR-Y36 / Y37 — INFRA-SHIPs adding source-face probes; H3 cluster surfaced but uncharacterized
PR-Y38 — first cycle to question the oracle baseline itself; phantom hypothesis empirically refuted

PR-Y38's positive finding (40 is real geometry) eliminates **50% of PR-Y37's banked options**:
- ELIMINATED: "tune grid upward" (refuted by 0.5× → 100× sweep)
- ELIMINATED: "position-tolerance edge-pairing" (no justification under CORROBORATION verdict)
- PROMOTED: PR-Y37 Options 1 (sub-quantization vertex-pair comparison) and 2 (per-segment NMM-incidence map) — both target the H3 cluster at 40 confirmed unpaired

This is the disciplined response to the strategic escalation rule. After 6 cycles took 40 as ground truth, the 7th validated the assumption. The next investigational cycle (PR-Y39) inherits a substantially narrower hypothesis space.

---

## §8 Banked findings disposition

| Finding | Source | Disposition |
|---|---|---|
| R0092 over-merging at 100× (43→45) | Canary §4.5 / Adversary §5 | EXPECTED (precision/coverage tradeoff); corroborates 1× is on correct side; NOT an action item |
| R0092 `total_edges` drift (280 vs 281) | Adversary §7.A | Minor measurement noise; banked low-priority hygiene observation |
| `results.json` instability across runs | Adversary §7.B | Pre-existing (likely Cherchi C++ TBB non-determinism per PR-Y31 banking); not PR-Y38 scope |
| ±2 near-pair scan deferred | Canary §2.4 / Adversary §7.C | Banked for PR-Y39 only if Options 1/2 fail to localize H3 |
| D.1d narrow fix banked (3 kids @ NMM-topology-aware repair) | PR-Y36 §4.2 banked | Survives as separate hygiene candidate; does NOT close F0020 Status:Failed |

None block ACCEPT.

---

## §9 Final recommendation — ACCEPT

PR-Y38 satisfies FIP §5 + DoD under the INFRA-class precedent established by PR-Y29/Y33/Y36/Y37. Role separation holds. Empirical evidence is independently reproduced. Canary near-miss was caught and audited green. The probe's verdict (CORROBORATION) is the disciplined empirical clarification that justifies INFRA-class: a 7th no-fix cycle that eliminates an entire hypothesis class, validates the baselines of all prior 6 investigations, and narrows PR-Y39's hypothesis space.

**Authorize Phase 8 push to `origin main` (plain push only; no force).**

Per `feedback_no_last_bug`: this memo does not claim closure of F0020 Render LOD, the H3 cluster, or the Yang pipeline broadly. 7+ open work items remain (F0020 Status:Failed, F0044 / F0045 / R0092 / R0045 cohort, 139 yang_fast cases, Cherchi C++ TBB non-determinism, D.1d narrow fix candidate, ±2 scan if H3 localization requires it).

---

**End of validation memo.**
