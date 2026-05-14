# PR-Y41 Adversarial Validation — adversary-y41

**Verdict:** **ACCEPT** (INFRASTRUCTURE-CLASS + 7th-refutation framing + strategic-pivot recommendation independently confirmed)

**Hard-constraint compliance:** Zero destructive git on live tree. All baseline replay used `git worktree add` only. Worktree removed after Gate G.

---

## §0 Summary

PR-Y41 ships ~247 LOC of env-gated, default-off dispatch-emission probe instrumentation in `crates/kernel/src/tessellation/mod.rs`. The probe records per-face `indices_emitted` + per-triangle quantization classification at `tessellate_planar_face_bounded` exit, attributed by the parent `tessellate_solid_bounded` driver. Production logic is untouched.

The load-bearing measurement (**Gate D**) directly tests PR-Y40 §6's banked claim that "of 18 D.1d-emitted indices, ~12 are lost UPSTREAM of `remove_winding_insensitive_duplicates`." Independent replay reproduces **18 EXACT** (kid 218=3 + kid 232=6 + kid 233=9), refuting the missing-12 framing. PR-Y40 §3.3's underlying measurements (4 collisions + 2 survivors = 6 tris = 18 indices) were correct; only the §6 interpretation was wrong. This is the 7th distinct refutation across the Y25..Y41 F0020 Render LOD investigation arc.

**Strategic-pivot recommendation independently confirmed.** The spec (§5) explicitly recommends Option B.1 (extend PR-Y29 Cherchi differential harness to Render LOD) as PRIMARY, with B.2 (synthetic min-failing-case) and C (pause F0020 Render LOD) as fallbacks. The cumulative metric (~1358 LOC probe, 0 LOC production fix, 0 movement in unpaired count 40→40 across 10 cycles) is stated honestly. No "this is the last bug" language anywhere.

---

## §1 Hard-constraint compliance

- **Zero destructive git on live tree.** Baseline replay via `git worktree add -f /tmp/y41-adv-baseline 7a3e4c3`, grep on isolated checkout, then `git worktree remove --force`. No `git stash`, `git reset --hard`, `git checkout --`, or `git clean` on live tree.
- **Live tree HEAD unchanged.** `74cb7ef` (PR-Y41 impl) preserved throughout audit.
- **Probe-off byte parity verified.** Gate B re-produces PR-Y40 baseline byte-for-byte.

---

## §2 Verification Gates

| Gate | Description | Status | Observed |
|---|---|---|---|
| **A** | Diff shape | **GREEN** | 4 files: tessellation/mod.rs (+247 LOC), wasm_bridge_bg.wasm, pr_y41_canary.md (+380), yang_pr_y41_dispatch_probe.md (+328). results.json NOT staged. All Y41 production additions env-gated behind `Y41_DISPATCH_PROBE` / `y41_on`. Thread-local buffer + RefCell pattern matches Y36/Y37 idioms. |
| **B** | Probe-off byte parity (CRITICAL) | **GREEN** | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int; stage-f 138→119→119→113→113 unpaired 30→42→39→39→39` — IDENTICAL to PR-Y40 baseline. |
| **C** | Probe-on fires | **GREEN** | 6 invocations × 2 outputs = 12 TSVs in `/tmp/y41-adv-probe/`. `[y41-dispatch-probe]` log lines emit for each invocation. inv006 = 65 faces dispatched. |
| **D** | F0020 18-index accounting (LOAD-BEARING) | **= 18 EXACT** | inv006 `d1d_summary.tsv`: kid 218=3, kid 232=6, kid 233=9. **TOTAL_D1D_INDICES = 18 EXACT.** No upstream loss. PR-Y40 §6 "missing 12" framing REFUTED by direct measurement. |
| **E** | F0020 degenerate-quantization signals | **GREEN** | D.1d kids: 232 single_vert_coll=1, 233 single_vert_coll=2, 218 single_vert_coll=0. Sum=3 single-collision tris matches PR-Y40 §3.5 partial-degenerate pattern. D.1d kids fully-degen=0 across all three. Non-D.1d fully-degen tris (inv006): 235=7 + 256=4 + 198=1 + 231=1 = **13 EXACT** — matches canary §3.4 EXACT. |
| **F** | Cohort | **GREEN** | F0044: 60 tris dispatched, 0 fully-degen. R0045: 608 tris dispatched, 0 fully-degen. Confirms fully-degenerate signature is F0020-specific per canary §4.1. |
| **G** | Baseline replay (non-destructive) | **GREEN** | `git worktree add -f /tmp/y41-adv-baseline 7a3e4c3` ran cleanly. `grep -c "Y41_DISPATCH_PROBE\|y41_dispatch" tessellation/mod.rs` at parent commit = **0** (zero matches, grep exit-1 expected). Worktree removed via `git worktree remove --force`. Live tree untouched. |
| **H** | kernel lib + yang_fast | **GREEN** | `cargo test -p kernel --lib`: **1262 passed; 24 failed; 42 ignored** — IDENTICAL to baseline (PR-Y40 audit reported same numbers). `yang_fast`: **10/157 passed, 139 failed, 8 errored (skipped 33 timeouts)** — IDENTICAL to PR-Y31-banked baseline (10/157, not 11). |
| **I** | Paper-grounding + no-last-bug | **GREEN** | Spec L246: "No 'this closes Yang' or 'this is the last bug' language. We do not know how many bugs remain." Spec L239 + canary §0: F0020 Status:Failed unchanged (40 unpaired). Spec L273 cites Yang §4.4.1 + L275 cites Cherchi 2022 §3 honestly: dispatch site is downstream of Cherchi's arrangement stage; no paper-cited oracle for per-face dispatch emission. |
| **J** | Strategic-pivot framing | **GREEN** | Spec §5.1 = "**PRIMARY recommendation — option (B.1): extend PR-Y29 Cherchi differential harness to Render LOD vertex diff**". §5.2 = SECONDARY (B.2 synthetic min-failing-case). §5.3 = TERTIARY (C pause F0020 Render LOD). §5.4 = "B.1 is the recommended PR-Y42 anchor". Spec §6 (line 265) honestly states "~1358 LOC of env-gated probe code … 0 LOC of production fix … Zero production progress on F0020 watertight unpaired count (40 → 40)." |

---

## §3 Load-bearing measurement reconstruction (Gate D + E)

### §3.1 Gate D — 18-index accounting

`/tmp/y41-adv-probe/F0020_inv006_d1d_summary.tsv`:

```
kid	indices_emitted	distinct_quantized_tris	degenerate_collapse_count	single_vert_collision_count
218	3	1	0	0
232	6	1	0	1
233	9	1	0	2
TOTAL_D1D_INDICES	18	DISTINCT_TRIS	DEGEN_TRIS	SINGLE_COLL_TRIS
```

**18 = 3 + 6 + 9 EXACT.** Each kid emits the expected `boundary_size * 3 - 3 - 2*inner_boundary_indices` count for its earcut decomposition:
- Kid 218: boundary=3 → 1 triangle → 3 indices ✓
- Kid 232: boundary=4 → 2 triangles → 6 indices ✓
- Kid 233: boundary=5 → 3 triangles → 9 indices ✓

### §3.2 Gate E — degenerate-quantization breakdown (inv006 non-D.1d)

`awk -F'\t' 'NR>1 {degen[$1]+=$8} END {for (k in degen) if (degen[k]>0) print k": "degen[k]}'` over inv006 dispatch.tsv:

```
kid=198 degen=1
kid=231 degen=1
kid=235 degen=7
kid=256 degen=4
```

**Total non-D.1d fully-degenerate = 1+1+7+4 = 13.** Matches canary §3.4 EXACT.

### §3.3 D.1d single-vert-collision distribution

| Kid | indices | distinct | degen | single_vert_coll | dispatched tris |
|---|---|---|---|---|---|
| 218 | 3 | 1 | 0 | 0 | 1 (clean) |
| 232 | 6 | 1 | 0 | 1 | 2 (1 clean + 1 partial-degen) |
| 233 | 9 | 1 | 0 | 2 | 3 (1 clean + 2 partial-degen) |
| **D.1d total** | **18** | **3** | **0** | **3** | **6** |

3 partial-degen + 3 distinct = 6 dispatched D.1d tris. PR-Y40 §3.3 reported 4 collisions at F.0→F.1 (D.1d-D.1d intra-set collisions) leaving 2 survivors. With 3 partial-degen tris colliding pairwise in F.0's canonical-key dedup, the 4-collision count is mechanically consistent with these 3 partial-degen tris plus 1 of the clean tris colliding (or similar canonical-key intersections — the exact pairing is in PR-Y40 §3.5 row 5).

### §3.4 Strategic implication

**PR-Y40 §6's banked "missing ~12 indices upstream of F.0" framing was an arithmetic over-interpretation.** PR-Y40 §3.3 row "tris surviving F.1 = 2" is consistent with 6 dispatched → 4 collide → 2 survive. There never was a missing-12 residual; the dispatch DOES emit 18 indices. The 7th refutation in the chain is therefore not refuting a measurement but a banked inferential claim that escaped Y40 § scrutiny.

This is precisely the failure mode `feedback_phase1_diagnosis_ranking_is_inference` predicts: a banked claim's inferential nature can leak even from an INFRA PR's §6 section, not only from Phase 1 Explore agents. Y41's probe operating at the correct empirical site caught the leak before another production cycle, consistent with `feedback_anchor_before_fix`.

---

## §4 Cohort cross-validation (Gate F)

| Case | invocations | total tris | fully-degen | single-coll | distinct |
|---|---|---|---|---|---|
| F0020 (inv006) | 1 (load-bearing) | 138 | 13 | 8 | 117 |
| F0044 (inv001) | 1 | 60 | **0** | 0 | 60 |
| R0045 (inv001) | 1 | 608 | **0** | 0 | 608 |

**Cohort has zero fully-degenerate emissions.** Confirms canary §4.1's claim that the F0020 fully-degenerate cluster (kids 235/256/198/231) is F0020-specific. Cohort cases dispatch >99% clean triangles. Their Render LOD defects (where present) are NOT in the D.1d or fully-degen mechanism.

Methodology note on invocation counts: F0044 captured 1 invocation at `tessellate_solid_bounded` (the load-bearing Render LOD dispatch pass) where PR-Y40 captured 3 invocations at `remove_winding_insensitive_duplicates`. The probe sites are different: `remove_winding_insensitive_duplicates` runs many times across the F.0→F.4 repair pipeline; `tessellate_solid_bounded` runs once per Render LOD pass. Y41's site is the load-bearing dispatch pass — sufficient for the 18-index accounting.

---

## §5 Refutation-chain audit

The canary §6.2 chronicles 7 distinct refutations:

| PR | Hypothesis | Outcome |
|---|---|---|
| Y28 | D.1a/b/c/d face-classification | Identified subtypes but ABORTed at canary |
| Y29-Y31 | Cherchi differential diff (arrangement layer) | Refuted F0020 D.1 = arrangement defect after Y34/Y35 (missing 93→7) yet Status:Failed persisted |
| Y32-Y35 | Cherchi-Rust port (Yang Gauss-map filter, STAGE3/4) | Reduced missing 93→7, did NOT close F0020 |
| Y36-Y37 | H1 grid-seam single-anchor diagnoses | Refuted |
| Y38 | Phantom-grid hypothesis | Refuted |
| Y39 | Single-kid small-emission preservation | Canary REFUTED 8 D.1d unpaired unchanged |
| Y40 | "16 D.1d-loser collisions" (Y39 inference) | Refuted to 4 collisions; banked "missing 12 upstream" |
| Y41 (this PR) | "Missing 12 upstream of F.0" (Y40 §6 inference) | Refuted: 18 indices emit EXACTLY |

**7 distinct refutations is correctly characterized.** The cumulative scaffolding (~1358 LOC env-gated probe across `tessellation/mod.rs` Y36/Y37/Y41, `repair.rs` Y40, `oracle.rs` Y38) is durable but represents real complexity. The strategic-pivot trigger condition baked into the PR-Y41 plan (Gate D = 18 EXACT → strategic pivot) is empirically met.

---

## §6 Strategic-pivot recommendation — independent confirmation

The spec recommends Option B.1 (Cherchi Render-LOD diff). Independent assessment:

- **B.1 is the empirically-correct pivot.** PR-Y29's Cherchi C++ sidecar exists (built at PR-Y29, plumbed per-op at PR-Y31, used at PR-Y30 for Stage B). Extending the existing harness to compare Render-LOD vertex output is incremental (~150-250 LOC per spec, reuses existing OBJ parser + quantize + run_diff_for_case). The investigation has lacked external ground truth since PR-Y36; every internal-consistency probe has self-vindicated its measurement without producing a fix anchor. `feedback_external_coherence` directly prescribes this: when porting from a paper with public C++ reference, build differential testing AS the load-bearing oracle, not the internal probes.

- **B.2 (synthetic min-failing-case) is a reasonable secondary.** Smaller, manually-bisectable, but still measures Waffle in isolation. Less leveraged than B.1.

- **C (pause F0020 Render LOD) is a reasonable tertiary fallback.** If B.1's Cherchi Render-LOD diff also fails to localize the defect, C is the rightful escalation. F0020 remains a known-failing case; future Cherchi-port work post-Y35 may resolve it incidentally.

- **Option A (continued probe refinement) is correctly NOT recommended.** A 10th probe-refinement PR on F0020 Render LOD D.1d at finer granularity would compound the 1358-LOC scaffold without addressing the absent-external-oracle gap. The spec §5.4 explicitly states this would be "the empirically-wrong move."

**Strategic-pivot recommendation INDEPENDENTLY CONFIRMED.**

---

## §7 Risk surface

| Risk | Severity | Mitigation |
|---|---|---|
| Probe wakes a hot-path performance regression | LOW | Probe-off byte parity verified (Gate B). Probe-on is dev-only invocation (`Y41_DISPATCH_PROBE=1` env). Thread-local buffer drained per-face. |
| Probe affects test determinism | LOW | Probe-off identical results; probe writes to env-gated `Y41_DISPATCH_PROBE_DIR` (no side effect if unset). |
| Probe captures wrong code-path coverage | LOW | Gate D measurement of 18 EXACT confirms the probe is attached at the load-bearing dispatch site; cohort capture confirms it fires across cases. |
| Probe LOC complexity | MEDIUM | ~247 LOC across 4 hunks. Pattern matches Y36/Y37 idioms. Cumulative kernel scaffold is ~1358 LOC — significant maintenance burden if F0020 Render LOD is never resolved. Strategic pivot to B.1 partially addresses by shifting next-cycle investment to external-oracle (not internal probe). |
| `boundary_positions` field unused | LOW | New warning noted in canary §5 row 1; field reserved for future use. No functional impact. |
| Strategic-pivot recommendation propagates incorrectly | LOW | Recommendation is honestly framed as a banked option set (§5), not a directive. PR-Y42's team-lead is free to pick A/B.1/B.2/C based on bandwidth. The probe ships as INFRA regardless. |

---

## §8 Acceptance

**ACCEPT.** All 10 gates GREEN. Probe is correctly default-off byte-identical, fires when enabled, produces the load-bearing 18-EXACT measurement, refutes PR-Y40 §6's missing-12 framing with direct evidence, and includes honest paper-grounding + `feedback_no_last_bug` discipline. Strategic-pivot recommendation (Option B.1) is independently sound given `feedback_external_coherence` and the 10-cycle / 1358-LOC / 0-production-fix metric.

### §8.1 Banked beyond this PR (unchanged from canary §6)

- F0020 Render LOD Status:Failed (40 unpaired, 8 degen, 10 self-int)
- F0044 / F0045 / R0092 cohort Status:Failed
- 139 / 157 yang_fast failures
- Cherchi C++ TBB non-determinism (PR-Y31)
- F0020-specific fully-degen-tri signal at kids 235/256/198/231 (new from Y41, banked as F0020-Render-LOD investigation, not load-bearing per cohort cross-check)
- 1358 LOC env-gated probe scaffold (maintenance debt; strategic-pivot at B.1 shifts new investment to external-oracle)

### §8.2 PR-Y42 anchor (confirmed)

**Primary: Option B.1 — extend PR-Y29 Cherchi differential harness to Render LOD vertex diff.** Estimated ~150-250 LOC harness extension. Reuses existing Cherchi C++ sidecar + parse_obj + quantize_tri infrastructure. Provides the external ground-truth oracle the Y36..Y41 probe stack lacks.

**Secondary: Option B.2** — synthetic minimum-failing-case for F0020 D.1d signature.

**Tertiary: Option C** — pause F0020 Render LOD; pivot to other priorities.

---

## §9 Per-feedback compliance audit

- `feedback_no_last_bug`: Spec L246 + L239 explicitly state F0020 Status:Failed unchanged. Compliant.
- `feedback_external_coherence`: PR-Y42 PRIMARY recommendation is the Cherchi differential harness extension (external oracle). Compliant.
- `feedback_anchor_before_fix`: PR-Y41 IS an empirical anchor verification (testing PR-Y40 §6's banked claim before any fix-shape commit). Compliant.
- `feedback_phase1_diagnosis_ranking_is_inference`: Canary §6.5 explicitly extends this feedback to "banked-claim from INFRA PR §6 is itself inference until directly measured." Compliant.
- `feedback_validate_against_corpus`: Gate F + canary §4 confirm cohort lacks the fully-degen / D.1d signatures. Probe validates across F0020 + cohort, not single-case. Compliant.
- `feedback_adversary_no_destructive_git`: Adversary used `git worktree add` + `git worktree remove --force` only. Compliant.

---

## §10 Final disposition

**VERDICT: ACCEPT.**

PR-Y41 ships clean INFRA-class instrumentation. The 18-EXACT measurement is reproducible, refutes PR-Y40 §6's inference cleanly, and triggers the plan's pre-baked strategic-pivot checkpoint. Strategic-pivot recommendation (Option B.1) is independently confirmed.

Zero destructive git operations on live tree. All gates GREEN.

