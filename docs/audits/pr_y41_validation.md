# PR-Y41 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y41 |
| Date | 2026-05-14 |
| Live tree HEAD | `74cb7ef` (PR-Y41 INFRA, NOT pushed) |
| Parent | `7a3e4c3` (PR-Y40 audit) |
| Class | INFRASTRUCTURE-CLASS (env-gated probe, 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Verdict | **ACCEPT — with explicit strategic-pivot acknowledgment** |

---

## §0 Verdict (single paragraph)

PR-Y41 ships ~247 LOC additive env-gated dispatch-emission instrumentation at `crates/kernel/src/tessellation/mod.rs::tessellate_planar_face_bounded` (+ parent driver `tessellate_solid_bounded`) and produces a load-bearing empirical refutation of PR-Y40 §6's banked "missing ~12 D.1d-emitted indices upstream of F.0" inference. Direct measurement at the dispatch site shows kids 218/232/233 emit **18 indices EXACTLY** (3+6+9), reproduced byte-for-byte across canary §3 and adversary §3 independent runs. PR-Y40's §3.3 underlying measurements (4 collisions + 2 survivors = 6 tris × 3 = 18 indices) already fully accounted for the dispatched D.1d indices; the §6 over-interpretation never had empirical backing. All four FIP §5 phase artifacts exist with role separation across spec-y41 / canary-y41 / impl-y41 / adversary-y41 per the INFRA-CLASS test-author waiver chain (Y29/Y33/Y36/Y37/Y38/Y40). DoD §1.5 gates GREEN (probe-off byte parity on F0020 spotlight, kernel lib 1262/24/42 stable, yang_fast 10/157 stable). A15.6 compliance intact. **This is the cycle at which the plan's pre-baked strategic-pivot checkpoint fires**: 10 cycles, ~1358 LOC cumulative probe, 0 LOC production fix, 0 movement in F0020 unpaired count (40→40). The spec §5 + canary §6 explicitly recommend Option B.1 (extend PR-Y29 Cherchi differential harness to Render LOD) as PRIMARY for PR-Y42; this audit independently validates that framing as the empirically-correct disciplined response per `feedback_external_coherence`. Recommend **ACCEPT** + Phase 8 push authorized.

---

## §1 FIP §5 phase-artifact checklist

| Phase | Artifact | Path | Status |
|---|---|---|---|
| 1 — Spec | `yang_pr_y41_dispatch_probe.md` (328 lines) | `specs/yang_pr_y41_dispatch_probe.md` | **GREEN** — probe design at three edit-hunk sites, refutation table (PR-Y40 §6 vs Y41 measured), §4 empirical findings, §5 PR-Y42 strategic-pivot recommendation (B.1/B.2/C), §6 honest 10-cycle reckoning, §7 banked out-of-scope, paper citations (Yang §4.4.1, Cherchi 2022 §3), no-last-bug language at L246 |
| 2 — Canary | `pr_y41_canary.md` (380 lines) | `docs/audits/pr_y41_canary.md` | **GREEN** — Gates 1-8 measured; §3 F0020 inv006 18-index accounting; §4 cohort independence (F0044/F0045/R0045/R0092 = 0 fully-degen); §6 PR-Y42 anchor recommendation; verdict SHIP-INFRA + 7th-refutation framing |
| 3 — Tests | INFRA-CLASS waiver (no production logic change) | regression coverage = probe-off byte parity | **GREEN** — DoD §1.5 satisfied via canary Gates 2/7/8 + adversary Gates B/H |
| 4 — Impl | Commit `74cb7ef` (4 files: tessellation/mod.rs +247, canary memo, spec, wasm bundle) | `git show 74cb7ef` | **GREEN** — additive env-gated; production tessellation emission code verbatim preserved; `results.json` correctly NOT staged |
| 5 — Adversary | `pr_y41_adversary.md` (198 lines, ACCEPT) | `docs/audits/pr_y41_adversary.md` | **GREEN** — Gates A-J all PASS on independent re-run against live HEAD `74cb7ef`; non-destructive git via `git worktree add` + `git worktree remove --force` |
| 6 — Audit | This memo | `docs/audits/pr_y41_validation.md` | **GREEN** (this audit) |

INFRA-CLASS waiver for test-author phase is consistent with the precedent chain PR-Y29 / Y33 / Y36 / Y37 / Y38 / Y40. Default-off byte parity is the regression coverage; canary Gate 2 + adversary Gate B verify it independently against the live HEAD.

---

## §2 Role separation — 4 distinct agents

| Role | Agent | Artifact ownership |
|---|---|---|
| Canary | `canary-y41` | `docs/audits/pr_y41_canary.md`; worktree probe build + Gates 1-8 measurement at the dispatch site |
| Spec | `spec-y41` | `specs/yang_pr_y41_dispatch_probe.md`; verbatim refutation table + strategic-pivot recommendation derived from canary findings |
| Impl | `impl-y41` | Commit `74cb7ef` (live tree, branch main); applied 247-LOC probe diff + WASM rebuild (5059517 → 5079432 bytes) |
| Adversary | `adversary-y41` | `docs/audits/pr_y41_adversary.md`; independent re-run of Gates A-J via `git worktree add`; non-destructive git |

Per `feedback_oracle_credibility_via_role_separation`: canary built the probe and measured the 18-index accounting; adversary independently re-ran from a fresh worktree without inheriting canary's reasoning chain and reproduced the 18-EXACT result. The refutation of PR-Y40 §6's "missing 12" framing is reproducible across role-separated runs from independent shell sessions.

No test-author role per INFRA-CLASS waiver; this is consistent with the precedent chain.

---

## §3 DoD checklist — probe-off byte parity is load-bearing

| DoD §1.5 item | Status | Evidence |
|---|---|---|
| Pathological / near-tolerance inputs tested | **GREEN** | F0020 (degenerate-triangle cluster, NMM cohort) + F0044/R0045/F0045/R0092 cohort (canary §4, adversary §4) |
| Degenerate geometry behavior validated | **GREEN** | 13 fully-degen + 11 single-coll tris on F0020 inv006 (canary §3.5); behavior under probe is observation-only; production dispatch semantics unchanged |
| No NaN values introduced | **GREEN** | Probe uses `y41_quantize_f32_vert` (integer i64 grid quantization); no floating-point arithmetic added to emission path |
| No invalid topology produced | **GREEN** | Production emission code in `tessellate_planar_face_bounded` verbatim preserved; probe is parallel observation/classification only |
| No regression in existing test suite | **GREEN** | Adversary Gate H: kernel lib **1262 passed; 24 failed; 42 ignored** IDENTICAL to PR-Y40 baseline; yang_fast **10/157** IDENTICAL |
| Default-off byte parity (load-bearing) | **GREEN** | Adversary Gate B: F0020 spotlight at HEAD `74cb7ef` with no Y41 env vars produces `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degenerate; 10 self-int; stage-f 138→119→119→113→113 unpaired 30→42→39→39→39` — IDENTICAL to PR-Y40 baseline |

Probe-off byte parity is the load-bearing DoD anchor for INFRA-CLASS work. Independent verification by both canary (worktree) and adversary (live HEAD) confirms it.

---

## §4 Empirical evidence cross-check (canary §3 vs adversary §3)

| Quantity | Canary §3.2 | Adversary §3.1-§3.3 | Cross-check |
|---|---|---|---|
| F0020 inv006 total_tris | 138 | 138 | **byte-match** |
| Kid 218 indices_emitted | 3 | 3 | **byte-match** |
| Kid 232 indices_emitted | 6 | 6 | **byte-match** |
| Kid 233 indices_emitted | 9 | 9 | **byte-match** |
| **TOTAL D.1d indices** | **18 EXACT** | **18 EXACT** | **byte-match** |
| D.1d distinct_quantized_tris | 1+1+1 = 3 | 3 | **byte-match** |
| D.1d degenerate_collapse | 0+0+0 = 0 | 0 | **byte-match** |
| D.1d single_vert_collision | 0+1+2 = 3 | 3 | **byte-match** |
| Non-D.1d fully-degen (kids 198/231/235/256) | 1+1+7+4 = 13 | 13 | **byte-match** |
| F0044 inv001 fully-degen | 0 / 60 tris | 0 / 60 | **byte-match** |
| R0045 inv001 fully-degen | 0 / 608 tris | 0 / 608 | **byte-match** |
| F0045 inv001 fully-degen | 0 / 6630 tris | (banked) | per-canary |
| R0092 inv001 fully-degen | 0 / 13621 tris | (banked) | per-canary |

PR-Y40 §6's banked claim ("missing ~12 indices upstream of F.0") vs PR-Y41's measured value (**0** missing; 18 emitted exactly) is **refuted by direct dispatch-site measurement**. The refutation is empirically reproducible across canary + adversary independent runs.

The new Y41-specific signal — D.1d kids' per-triangle quantization classification — shows kids 232/233 emit 3 single-vert-collision tris (out of 6 D.1d total), which corresponds mechanically to PR-Y40 §3.3's 4 D.1d-loser collisions at F.0→F.1. The single-collision distribution is the rate-limiting mechanism for the F.0 canon-dedup losses, not an upstream loss.

This is the **load-bearing positive value** of PR-Y41: it forecloses PR-Y42 from being scoped against the wrong frame (upstream-of-F.0 dispatch loss). The 7th-refutation framing is empirically grounded, not rhetorical.

---

## §5 A15 compliance

A15.6 (Hybrid B-Rep/Mesh Boolean Pipeline — Yang 2025) is the governing invariant. PR-Y41 instruments `tessellate_planar_face_bounded` + `tessellate_solid_bounded`, which sit in Waffle's Render-LOD tessellation layer downstream of Yang Stage 6 (B-Rep assembly + retessellation). The probe:

- **Does not alter pipeline behavior** — production emission code in `tessellate_planar_face_bounded` (all 4 branches) verbatim preserved; probe-on/off byte parity verified at canary Gate 2 + adversary Gate B against the live HEAD.
- **Does not change analytical surface preservation** (A15.5) — operates on QPos quantized integers (`y41_inv_grid_from_verts` / `y41_quantize_f32_vert`); no geometric primitive modification.
- **Instruments the empirically-correct anchor** for the F.−1 dispatch site (canary §3.1: inv006 total_tris=138 byte-matches PR-Y40 inv006 n_tris_input=138 byte-matches stage-f sub=0 n_tris=138 — triple-anchored).
- **Respects A15.4 sequencing** — SSI solver work independent and unaffected; PR-Y41 is Render-LOD layer only.

Spec §9 explicitly notes that `tessellate_planar_face_bounded` is a Waffle Render-LOD-only operation outside the Yang 2025 + Cherchi 2022 paper scopes; per `feedback_external_coherence`, the probe IS the empirical reference at this layer. The recommended PR-Y42 pivot (B.1, extending Cherchi differential diff to Render LOD) is exactly what introduces the external paper-grounded oracle this layer currently lacks.

---

## §6 INFRA-CLASS framing audit

| Criterion | Status | Evidence |
|---|---|---|
| Production logic LOC = 0 | **GREEN** | `git diff 7a3e4c3..74cb7ef -- crates/kernel/src/tessellation/mod.rs` shows only additive env-gated blocks; all probe state writes inside `if y41_on { … }` guards; emission code paths unchanged |
| Env-gated default-off | **GREEN** | `y41_probe_enabled()` checks `Y41_DISPATCH_PROBE == "1"`; all entry/exit captures and parent-driver drain skipped when false |
| Additive only | **GREEN** | +247 LOC in mod.rs; no deletions in production code. 4 files total: mod.rs +247, canary memo +380, spec +328, wasm bundle |
| WASM rebuild included | **GREEN** | `app/static/pkg/wasm_bridge_bg.wasm` regenerated (5059517 → 5079432 bytes); consistent with WASM workflow per CLAUDE.md |
| `results.json` correctly NOT staged | **GREEN** | `git status` shows it unstaged in working tree; yang test-result drift is known background phenomenon; per project convention not auto-committed |
| Cumulative probe complexity disclosed | **GREEN** | Spec §6 + §8.3 + commit body acknowledge ~1358 LOC cumulative probe across `tessellation/mod.rs` (Y36/Y37/Y41), `repair.rs` (Y40), `oracle.rs` (Y38); explicit recognition that this is the maintenance-debt ceiling |

INFRA-CLASS framing is intact. The probe is durable scaffolding for any future at-site measurement at the F.−1 dispatch site; the production emission behavior is unchanged.

---

## §7 Strategic context — 10-cycle reckoning; ~1358 LOC; pivot-trigger fires

| PR | Outcome | Cycle role |
|---|---|---|
| Y25 | ABORT (canary) | Yang §4.4.1 mesh-updating refuted as immediate anchor |
| Y26 | ABORT (canary) | Cohort-wide missing-triangle defect; not the 3 plan candidates |
| Y27 | ABORT (canary) | flood_fill_patches drops 0 SourceFaces; D.1 split into 3 sub-mechanisms |
| Y28 | ABORT (canary) | D.1d kids 218/232/233 identified; fix-shape refused commit |
| Y36 | INFRA SHIP | Y36 inverse-probe source-face attribution (downstream) |
| Y37 | INFRA SHIP | H1/H2/H3 classification refined |
| Y38 | INFRA SHIP | Grid-sensitivity oracle gate; phantom-hypothesis refuted |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 with N=16 attribution |
| Y40 | INFRA SHIP — 6th-refutation | PR-Y39 §2.5's N=16 attribution refuted; measured N=4; banked "missing 12 upstream" |
| **Y41** | **INFRA SHIP — 7th-refutation** | **PR-Y40 §6's "missing 12 upstream" inference refuted; 18 indices EXACT; strategic-pivot trigger fires** |

**Cumulative cycle accounting:**
- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); 5 INFRA SHIPs (Y36/Y37/Y38/Y40/Y41); **0 production fix on F0020 Render LOD in 10 cycles**.
- Cumulative probe LOC: **~1358** (Y36/Y37 in mod.rs ~711, Y38 in oracle.rs ~179, Y40 in repair.rs ~151, Y41 in mod.rs ~317).
- F0020 unpaired count: **40 → 40 across all 10 cycles**.

**Strategic-pivot trigger condition: FIRED.** The PR-Y41 plan explicitly baked in the trigger: "No anomaly (Gate 4 = 18) → 7th-refutation; ~1358 LOC cumulative probe with no production code in 10 cycles. Strategic pivot recommended at this point — options (B) different diagnostic strategy or (C) pause F0020 Render LOD." Gate 4 measured 18 EXACT. The trigger condition is met.

**ROI assessment:**
- **Positive:** PR-Y41 catches PR-Y40 §6's banked inference as wrong BEFORE PR-Y42 scopes itself against it. This is the discipline of `feedback_anchor_before_fix` paying off explicitly (cf. PR-Y40 catching the indices-vs-triangles conflation, PR-Y39 catching the F.1→F.2 wrong anchor). Spec §8.4 extends the feedback rule to "banked-claim from an INFRA PR's §6 is itself inference until directly measured" — a load-bearing meta-learning.
- **Negative-but-acknowledged:** 10 cycles without a production fix on F0020 Render LOD is a non-trivial cost. The spec §6 and canary §6.2 honestly state this; commit body L46-49 names it explicitly: "0 LOC production code on F0020 Render LOD; 0 movement in F0020 unpaired count (40→40)."
- **Net:** ACCEPT under current discipline, BUT this audit explicitly endorses the spec §5 + canary §6 recommendation: **PR-Y42 should not be a 10th probe-refinement PR**. The empirically-correct next step is Option B.1 (extend PR-Y29 Cherchi differential harness to Render LOD vertex diff), which introduces external ground truth the Y36..Y41 probe stack lacks.

Per `feedback_phase1_diagnosis_ranking_is_inference`: PR-Y41's refutation of PR-Y40's "missing 12" banked inference is exactly the kind of inference-vs-measurement disambiguation that the rule mandates. The 7th refutation is itself positive empirical progress AND a structural signal that the diagnostic strategy needs to change.

Per `feedback_external_coherence`: the recommended PR-Y42 pivot (B.1) shifts diagnostic investment from internal self-consistency probes to reference-parity diff against Cherchi C++. This is exactly the prescription: "When the algorithm we're porting has a public reference implementation (Cherchi 2020/2022 C++), build differential testing against that reference as the load-bearing oracle. Internal stage oracles measure self-consistency; reference parity measures correctness."

Per `feedback_no_last_bug`: spec L246 and adversary §9 explicitly carry "We do not know how many bugs remain"; F0020 Status:Failed unchanged. No closure claim.

Per `feedback_no_regression_chasing`: probe is infra-only; pipeline test counts identical to baseline.

---

## §8 Banked findings (from canary §6 + adversary §6 + new F0020 signal)

1. **PR-Y42 PRIMARY recommendation — Option B.1 (Cherchi differential harness extended to Render LOD vertex diff).** ~150-250 LOC harness extension reusing existing `cherchi_differential_diff.rs` (671 LOC) + Cherchi C++ sidecar build + `parse_obj` + `quantize_tri` infrastructure. Provides external ground-truth oracle the Y36..Y41 probe stack lacks.
2. **PR-Y42 SECONDARY — Option B.2 (synthetic minimum-failing-case for F0020 D.1d signature).** Smaller, manually-bisectable, but still measures Waffle in isolation.
3. **PR-Y42 TERTIARY — Option C (pause F0020 Render LOD).** Pivot to cohort F0044/F0045/R0092 (D.2/D.3 mechanisms), SSI solvers (A15.4), GUI test coverage, or cross-crate integration tests.
4. **NEW from PR-Y41 — F0020-specific fully-degenerate-tri signal at dispatch (kids 235=7, 256=4, 198=1, 231=1).** Cohort F0044/F0045/R0045/R0092 has **0** fully-degen across ~21k cohort tris. F0020-specific; connection to the 40-unpaired-edge defect unproven (3 surviving fully-degen tris contribute to F0020's "8 of 113 degenerate" final count but unpaired edges reportedly elsewhere per PR-Y36 face_inventory). Banked as F0020-Render-LOD investigation thread; not load-bearing per cohort cross-check.
5. **F0045 / R0092 retess-pass 13K-collision outliers** (PR-Y40 §4.3, unchanged). Different defect (fully-degenerate Render-LOD quantization on huge planar faces). Banked.
6. **139 / 157 yang_fast failures.** Unchanged. Gate 8 confirms 10/157 baseline.
7. **Cherchi C++ TBB non-determinism** (PR-Y31 banked, unchanged). Use missing-count (deterministic) as gate.
8. **Yang §4.4.1 mesh-updating (Diagnosis B from PR-Y25).** Long-term load-bearing layer; banked.
9. **Cumulative probe scaffold ~1358 LOC.** Maintenance debt. Strategic pivot to B.1 shifts new investment to external-oracle; preserves existing scaffold as durable reference.
10. **`boundary_positions` field unused in PR-Y41 probe.** Reserved for future use; one new compiler warning. No functional impact (canary §5 row 1; adversary §7 LOW risk).

All ten items are correctly enumerated by the canary + adversary; this audit confirms they are appropriately scoped as banked-not-blocking.

---

## §9 Final recommendation

**ACCEPT — with explicit strategic-pivot acknowledgment.**

Rationale:
- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec / canary / impl / adversary).
- **DoD §1.5 GREEN** — probe-off byte parity is the load-bearing regression coverage; verified independently by canary Gate 2 + adversary Gate B.
- **INFRA-CLASS framing intact** — 0 LOC production logic; ~247 LOC additive env-gated probe; default-off byte-identical; production emission semantics verbatim preserved.
- **A15.6 compliant** — Render-LOD-layer instrumentation; no pipeline behavior change; no analytical surface impact (A15.5 unaffected).
- **Empirical evidence load-bearing** — PR-Y40 §6's banked "missing ~12 indices upstream of F.0" inference refuted by direct dispatch measurement; 18 indices EXACT; measurement byte-matches across canary and adversary independent runs.
- **No-last-bug discipline GREEN** — adversary §9 + spec L246 + commit body explicit; F0020 Status:Failed unchanged (40 unpaired).
- **Strategic context honestly disclosed** — 10 cycles, 5 ABORTs, 5 INFRA SHIPs, 0 production fix on F0020 Render LOD, ~1358 LOC cumulative probe scaffolding. Commit body L46-49 names this explicitly.

**Strategic-pivot acknowledgment.** This is the FIRST audit in the Y25..Y41 arc where the cycle's outcome explicitly recommends reconsidering continued probe-refinement. The spec §5 and canary §6 honestly frame the recommendation as a banked option set (B.1 PRIMARY / B.2 SECONDARY / C TERTIARY), not a directive — PR-Y42's team-lead retains scoping authority. This audit independently validates the framing:
- **Option B.1 is empirically supported by `feedback_external_coherence`.** The Y36..Y41 probe stack measures self-consistency; reference parity measures correctness. The cumulative 10-cycle / 0-fix metric is structural evidence that internal-consistency probing has reached its diagnostic ROI ceiling.
- **The probe scaffold itself is positive value preserved.** PR-Y36..Y41 deliver durable empirical-reference layers for future investigations regardless of which option PR-Y42 selects.
- **Continued probe-refinement (Option A) is correctly NOT recommended.** A 10th probe-refinement PR at finer granularity would compound the 1358-LOC scaffold without addressing the absent-external-oracle gap.

**Phase 8 push authorized.** Recommend:
1. Commit this audit memo (`audit(yang-pr-y41): ACCEPT — 7th-refutation; strategic pivot to B.1 endorsed | INFRA-ONLY`) + adversary memo.
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y41_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, 7th-refutation, strategic-pivot trigger fired, PR-Y42 PRIMARY anchor banked at Option B.1.
4. `TeamDelete pr-y41` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y42 scoping should explicitly weigh the strategic-pivot recommendation; treating Gate 4 = 18 EXACT as the load-bearing signal that the diagnostic frame itself (not just the anchor) needs to change. If Option B.1 (Cherchi Render-LOD diff) also fails to localize the F0020 Render LOD defect, that is the rightful escalation trigger for Option C (pause F0020 Render LOD).
