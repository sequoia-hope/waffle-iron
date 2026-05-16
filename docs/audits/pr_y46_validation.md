# PR-Y46 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y46 |
| Date | 2026-05-15 |
| Live tree HEAD (impl-y46 mirror) | `2fa4058` (PR-Y46 INFRA — content present in worktree as uncommitted state; close-out commits + pushes) |
| Worktree HEAD | `b0009bd` (PR-Y42 audit base; PR-Y43+Y44+Y45+Y46 content mirrored as uncommitted) |
| Parent | `c0c2019` (PR-Y45 audit ACCEPT — INFRA-CLASS; α REFUTED at 0/24; PR-Y46 anchor `face_survival_detect` PLAUSIBLE-BUT-NOT-CONFIRMED) |
| Class | INFRASTRUCTURE-CLASS (test-harness probe extension; 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Strategic-pivot ROI | **POSITIVE remains — FIRST POSITIVE-MEASUREMENT next-cycle anchor in 15 cycles; 4-step empirical chain now connected (Y42 → Y44 → Y45 → Y46)** |
| Verdict | **ACCEPT (SHIP-INFRA) — Layer A (`face_survival_detect`) REFUTED at 0/24; Layer B (γ Render-LOD retess) CONFIRMED at 24/24 = 100.0% byte-stable across 6 independent reruns (canary 3 + adversary 3); PR-Y47 anchor = γ retess at `crates/kernel/src/boolean/yang_integration.rs:1024` COARSE-CONFIRMED; sub-anchor requires PR-Y47 INFRA canary (Option A) before fix-shape commit** |

---

## §1 Adjudication summary (single paragraph)

PR-Y46 ships +289 LOC additive test-harness probe at `crates/test-harness/tests/cherchi_differential_diff.rs` (1652 → 1943) — one `#[ignore]`-gated test fn `f0020_stage_bb_b_e_bisection` plus two helper fns (`load_case_d_positions_file`, `load_obj_canonical_tri_set`). The probe consumes three pre-existing `YANG_STAGE_DUMP` OBJ outputs (Stage Bb at `topology_extract.rs:2396`, Stage B at `topology_extract.rs:2568`, Stage E_lod=Render at `yang_integration.rs:1063-1074`) plus a PR-Y44-derived 24-entry F0020 Case-D positions file, and computes a three-stage set-difference bisection (`Bb \ B` = Layer A `face_survival_detect` drops; `B \ E` = Layer B γ Render-LOD retess drops). Canary-y46 measured **Layer A = 0/24 = 0.0%, Layer B = 24/24 = 100.0%** byte-stable across 3 independent reruns (2 stage-dump generations + 1 probe-replay); decision-gate fires Layer-B-dominant ≥ 80% ⇒ SHIP-INFRA + γ retess anchor at `yang_integration.rs:1024`. Adversary-y46 independently re-ran the probe with own stage-dump dirs (`/tmp/adversary-y46-stages-f0020-{,r2,r3}`) + own Case-D positions file (byte-identical to canary's via independent extraction) + pure-Python re-derivation of the OBJ-parse + canonical-key + set-arithmetic from scratch — and converged on the SAME numbers across all 3 reruns (171/194/41 + |B\Bb|=0 + |E\Bb|=71 + Case D 0/24 Layer A + 24/24 Layer B). All 8 gates GREEN at both phases. Adversary's 7-axis stress-test resolved 6 axes fully (OBJ parsing, |B\Bb|=0 implication, |E\Bb|=71 float-precision interpretation, Case D direction cross-reference, Cherchi non-det invariance, Bb=420 vs 246 source) and banked 1 axis (`parse_obj` line 136 silent 0-index→0 mapping — pre-existing hygiene, not Y46-introduced); the cross-implementation Rust/Python oracle agreement provides high methodological confidence. The audit-y45 §4.1 prescription that PR-Y46 anchor = `face_survival_detect` is **empirically REFUTED**, mirroring the PR-Y45 α refutation pattern (audit-y44 §3.4 → PR-Y45 0/24 refutation). The **SECOND consecutive measurement-first refutation** of an audit-recommended anchor (Y45 α 0/24 + Y46 face_survival_detect 0/24) AND the **FIRST positive-measurement next-cycle anchor in 15 cycles** (Y46 γ retess 24/24 = 100% direct measurement, not inference-from-refutation) is the substantive output: the 4-step empirical chain (Y42 50% borderline → Y44 (a) 100% measured → Y45 α refuted → Y46 γ confirmed) is now connected, and PR-Y47 enters with a STRONG coarse anchor rather than a PLAUSIBLE one. Adversary §9.2-9.4 + canary §8.3 reinforce that γ retess is endorsed at the COARSE level only; the SUB-ANCHOR within γ (F.0/F.1/F.2/F.3/F.4 sub-stages + B-Rep assembly stage upstream of γ + per-face independent CDT seam alignment) is unmeasured and must be sub-bisected by PR-Y47 INFRA canary before any fix-shape commit. Recommend **ACCEPT (SHIP-INFRA)** + PR-Y47 = **Option (A) — INFRA sub-bisection canary** before production-fix attempt. Phase 8 push authorized.

---

## §2 Gates re-summary (verbatim adversary §2, with audit confidence)

Per `feedback_oracle_credibility_via_role_separation`: canary built the bisection probe (+289 LOC test fn + 2 helpers) and measured 0/24 Layer A + 24/24 Layer B across 3 reruns. Adversary independently re-extracted the Case-D position list via a from-scratch Python parser (byte-match to canary's), independently regenerated 3 stage-dump dirs (not re-using canary's `/tmp/y46-stages-f0020/`), and re-ran the probe 3× with own dumps. Both sides converge on identical attribution.

| Gate | Description | Expected (canary §6) | Observed (adversary §2) | Status |
|---|---|---|---|---|
| **A** | `cargo build -p test-harness ...` clean | clean; pre-existing `pr13_trim_loop_diagnostic.rs` E0609 unrelated | clean; `Finished dev profile in 0.03s` | **GREEN** |
| **B** | F0020 spotlight default-off byte parity (CRITICAL) | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` | EXACT byte-match across 5 reruns | **GREEN** |
| **C** | Independent stage-dump generation | Bb=420f/141v, B=246f/141v, E=113f/219v | EXACT byte-match (3 independent dirs) | **GREEN** |
| **D** | Independent Case D positions file byte-matches canary's | 24 entries, 42-mode | `diff` returns 0 bytes; d[16] spot-check byte-match | **GREEN** |
| **E** | Independent bisection ≥ 3 reruns (LOAD-BEARING) | Layer A=0/24, Layer B=24/24 byte-stable | 3 reruns × 3 dump dirs = 9 measurements at 0/24, 24/24 byte-identical | **CONFIRMED (independently verified)** |
| **F** | PR-Y43+Y44+Y45 baselines preserved (probe-off) | A/B/C/D=4/14/0/24 (42-mode); (a)=24/24=100%; α=0/24 | EXACT byte-match | **GREEN** |
| **G** | kernel lib + yang_fast | 1262/24/42 + 10/157 | EXACTLY `1262 passed; 24 failed; 42 ignored` + `10/157 passed, 139 failed, 8 errored` | **GREEN** |
| **H** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 missing=0, extras=0, common=136 | EXACTLY `missing=0, extras=0, common=136` | **GREEN** |

**8/8 gates GREEN at both phases.** Zero RED. Adversary code review at §7 confirmed: `load_case_d_positions_file` is fail-loud on malformed input (correct per `feedback_anchor_before_fix`); `load_obj_canonical_tri_set` reuses PR-Y29..Y45 `parse_obj` + `quantize_tri` byte-exact; `f0020_stage_bb_b_e_bisection` is env-var-driven (default-off), skips cleanly if dumps missing, applies set-diff arithmetic via `HashSet::difference`, emits per-tri layer assignment with explicit `(in_a, in_b)` match arms. Probe code is sound; no behavioral defect that would affect the 0/24 + 24/24 verdict.

---

## §3 Stress-test adjudication (verbatim adversary §6, with audit confidence)

Adversary §6 ran 7 stress-test axes on the 100% Layer-B claim. All 7 RESOLVE (6 fully verified + 1 banked-as-hygiene pre-existing). Audit confidence per axis:

| # | Stress-test | Adversary finding | Audit confidence |
|---|---|---|---|
| **§6.1** | OBJ-parsing correctness | `quantize_tri` sorts 3 `(i64,i64,i64)` at 1e-6 grid → matches PR-Y45/Y43/Y44/Y30 canonical form. Pure-Python re-derivation byte-matches Rust probe. | **HIGH** — Two independent OBJ-parse + canonical-key + set-arithmetic implementations converge on identical 171/194/41 + |B\Bb|=0 + |E\Bb|=71 counts. |
| **§6.2** | `|B \ Bb| = 0` reads as "B ⊆ Bb" | Probe explicitly outputs `|B \ Bb| = 0`; pure-Python verifies. Consistent with Yang §3.3 + Cherchi 2022 §5 selective-retention semantics: face_survival_detect monotonically picks a subset of Bb. | **HIGH** — The "suspiciousness" of clean 100% is artifact-of-pattern; underlying partition is clean by construction. |
| **§6.3** | `|E \ Bb| = 71` float-precision interpretation | `|E ∩ B| = 36`; 36/112 = 32% of E's canonical tris are exact matches to Stage B's selective subset. If precision drift caused the 71 "new" tris, this 36-overlap would be ~0. | **HIGH** — Direct rebuttal of float-precision-drift confounder. The 71 ADDED tris are real geometric re-samples from per-face independent CDT at higher LOD (16-seg → 64-seg). γ retess is REPLACE-and-ADD, not just DROP. |
| **§6.4** | Case D direction cross-reference | Read PR-Y44 attribution code: `d[]` rows are Cherchi-side positions (`cherchi_set \ waffle_set`); probe asks "does Waffle's Stage X contain this Cherchi-side canonical-tri?" Correct direction. | **HIGH** — Direction-of-test rules out the most common methodological loophole. |
| **§6.5** | Cherchi non-det invariance | 8/8 reruns produced 42-mode. Even at 47-mode worst-case (2 extra entries), Layer B = 24/26 = 92.3% > 80% threshold. Decision-gate verdict invariant. | **HIGH** — Bound is sound algebraically; observed dominance reinforces it. |
| **§6.6** | Bb=420 vs 246 source discrepancy | `[yang-diag] after subdivide: tris_a=290, tris_b=130` ⇒ 420 = STAGE6 triangulation output (Cherchi-Rust arrangement of both inputs). Brief's "246-ish" was post-`face_survival_detect`, not Bb. Bisection arithmetic unaffected. | **HIGH** — Canary memo §4.2 explicitly documents and corrects the methodology mapping. Layer A = `Bb \ B` and Layer B = `B \ E` correctly attribute to the right operations regardless. |
| **§6.7** | Code review of +289 LOC | All three new fns clean + idiomatic. One minor banked: `parse_obj` line 136 silent 0-index→0 (pre-existing hygiene, not Y46-introduced). Defensive A+B branch unreachable under `B ⊆ Bb` invariant. | **HIGH** — No load-bearing bugs. Banked items are non-load-bearing for the verdict. |

**Adjudication: 7/7 stress-test axes resolved.** Audit confidence on the SHIP-INFRA + 24/24 Layer B verdict is **HIGH**. The cross-implementation Rust/Python oracle agreement is the cleanest methodological cross-check of the cycle; combined with 6 independent measurements (3 canary + 3 adversary reruns) and zero contradictions, the empirical finding is load-bearing.

The mechanism finding at §6.3 (|B ∩ E| = 36; 32% of E's canonical tris are exact preservations from Stage B) is particularly informative for PR-Y47 scoping: γ retess is a REPLACE-and-ADD layer (71 fresh tris + 36 preserved + 41 dropped-then-replaced) — NOT a pure DROP layer. The Case D 24 are dropped-and-not-replaced-with-equivalent-canonical-tri, which is a distinguishable sub-class within γ's behavior.

---

## §4 PR-Y47 anchor decision — LOAD-BEARING

### §4.1 Recommended PR-Y47 anchor (verbatim for memory file)

**PR-Y47 anchor = γ Render-LOD re-tessellation in `tessellate_waffle_solid` at `crates/kernel/src/boolean/yang_integration.rs:1024`** (and the underlying `tessellate_solid_ext_with_lod` in `crates/kernel/src/tessellation/mod.rs`). **24 / 24 = 100.0% of F0020 Case D positions are dropped at this layer; 0 / 24 = 0.0% at the previously-prescribed `face_survival_detect` anchor (audit-y45 §4.1 REFUTED).** Status: **COARSE-CONFIRMED at 100% direct measurement; SUB-ANCHOR within γ remains INFERENCE pending PR-Y47 INFRA sub-bisection canary.**

Paper anchor:
- **Yang 2025 §4.4.1** (mesh updating; bijective re-mesh; CDT) at `refs/text/yang2025_hybrid_boolean.txt:548-590`. The re-mesh step is the layer dropping the 24 Case-D triangles.
- **Livesu et al. 2021** (simplified earcut CDT; cited in CLAUDE.md) — used at the per-face CDT call inside `tessellate_solid_ext_with_lod`.
- **Cherchi 2022 §5** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) is NOT load-bearing here — that's the Stage B layer the bisection just exonerated (0/24 at Layer A).

### §4.2 PR-Y47 scoping decision: **Option (A) — INFRA sub-bisection canary BEFORE fix-shape commit**

Three options were considered:

- **(A)** PR-Y47 is another INFRA cycle: build a finer probe that bisects γ retess into F.0 / F.1 / F.2 / F.3 / F.4 / per-B-Rep-face sub-mechanisms. Measure which sub-layer drops the 24 Case D positions. Then PR-Y48 picks the fix-shape.
- **(B)** PR-Y47 directly investigates γ retess code for the specific defect that drops the 24. Risk: γ is a large function with many sub-stages; choosing a sub-anchor without measurement is inference.
- **(C)** PR-Y47 is the first PRODUCTION-CHANGE attempt at γ. Pick a sub-anchor based on Phase 1 exploration + paper citation; canary it; if Case D ⊃ predicted-drop-set ≥ 80%, ship fix; else ABORT-fix like Y45.

**Audit recommendation: Option (A).** Rationale:

1. **The 15-cycle pattern argues for measurement before fix at every layer.** PR-Y44 anchor (α F.0) ⇒ PR-Y45 measured 0/24 ⇒ REFUTED. PR-Y45 anchor (face_survival_detect) ⇒ PR-Y46 measured 0/24 ⇒ REFUTED. The audit-recommended anchor has been wrong 2/2 times when sub-bisection skipped. The base rate for "audit's inferred sub-anchor is correct" is now empirically 0/2 — far below the threshold where Option (C) is rational.
2. **γ has ≥ 5 sub-stages within `tessellate_solid_ext_with_lod` + ≥ 1 upstream B-Rep assembly stage.** Canary §4.2 enumerates: F.0 Render-LOD pre-cleanup (138 tris), F.1 NMM-removal, F.2/F.3 dedup, F.4 final dedup (113 tris) — already-captured stage dumps in `/tmp/y46-stages-f0020/F0020/stage_F.*.obj` make near-free sub-bisection possible. Adversary §9.2 Q1 raises the parallel concern that the drop may be UPSTREAM of γ retess at `assemble_brep_topology` (B-Rep assembly losing triangulation info before γ retess consumes it).
3. **The COARSE finding is robust; the SUB-ANCHOR is unmeasured.** Adversary §9.4 explicitly states "the SUB-LAYER within γ retess is NOT YET measured. PR-Y47 should not commit fix shape at any sub-layer without canary measurement at that sub-layer specifically." Per `feedback_phase1_diagnosis_ranking_is_inference`: the canary's coarse measurement (24/24 between Stage B and Stage E) is TRUTH; finer attribution within γ requires another measurement cycle.
4. **Sub-bisection is near-free.** F.0 through F.4 dumps already exist; PR-Y47 INFRA canary need only extend the `f0020_stage_bb_b_e_bisection` probe to additional partition points within `B → F.0 → F.1 → F.2 → F.3 → F.4 → E`. Estimated ~50-100 LOC extension to the same test file. Per adversary §10.3: reuse the Y46 scaffold + dump infrastructure.
5. **`feedback_anchor_before_fix` applied recursively.** Per audit-y45 §4.2, "PR-Y46 canary MUST instrument face_survival_detect's drop set BEFORE committing fix shape" was load-bearing — and prevented an entire wrong-anchor fix cycle. Same discipline applies to PR-Y47: sub-bisect γ BEFORE committing fix shape. The cost of one more INFRA cycle is dramatically lower than the cost of a wrong-anchor production-fix cycle.

### §4.3 PR-Y47 sub-bisection canary discipline (LOAD-BEARING)

PR-Y47 INFRA canary MUST:

1. **Extend `f0020_stage_bb_b_e_bisection` to a per-F-stage bisection.** Partition the Stage B → Stage E drop across F.0 / F.1 / F.2 / F.3 / F.4 dump points (`/tmp/y46-stages-f0020/F0020/stage_F.{0,1,2,3,4}.obj` already exist; counts 138 → 113 with 25 cumulative-drop across F-stages). Compute per-F-stage drop sets vs the 24 Case D positions.
2. **Probe the B-Rep assembly stage upstream of γ retess.** Per adversary §9.2 Q1: if the 24 Case D triangles are PRESENT at Stage B (verified by Y46) but ABSENT at the B-Rep assembly output (input to `tessellate_waffle_solid`), the drop is at `assemble_brep_topology` — UPSTREAM of γ retess in `topology_extract.rs`. Add a dump-point between Stage C and Stage F.0 to bisect "B-Rep assembly drop" vs "γ retess drop".
3. **Per-B-Rep-face attribution.** Cross-reference Stage E_labels.csv (`tri_idx,face_id`) — each Case D triangle SHOULD have come from a specific B-Rep face. Verify whether γ retess produced ANY tri on that face_id (face entirely missing ⇒ B-Rep assembly defect) or whether γ retess produced DIFFERENT tris on that face_id (per-face CDT seam alignment defect).
4. **Apply the same decision-gate as PR-Y45/Y46.** Sub-stage with ≥ 80% (≥ 19/24) → confirmed → proceed to PR-Y48 fix-shape; ≤ 20% (≤ 4/24) → refuted → SHIP-INFRA-ABORT-fix + pivot to next candidate; mixed (5-18) → SKIP fix-shape; banked.
5. **NEVER commit fix shape on inference.** Per `feedback_anchor_before_fix` + 15-cycle pattern: the next audit-recommended sub-anchor will be Phase 1 inference until canary measurement validates it.

### §4.4 Alternative candidates (if γ sub-bisection refutes all F-stages)

Per canary §8.4 + adversary §9.3 banked:

1. **`flood_fill_patches` patch-segmentation** at `crates/kernel/src/boolean/topology_extract.rs` (PR-Y27 banked; canary §9.1 SECONDARY). Probe if all F.0-F.4 stages drop < 20% of Case D.
2. **`assemble_brep_topology` in `topology_extract.rs`** — conversion of Stage C (post-flood-fill) into the WaffleSolid B-Rep input to γ retess. If F-stage sub-bisection shows the 24 are missing already at F.0 input, this is the drop site.
3. **`tessellate_solid_ext_with_lod` per-face independence** — each B-Rep face is re-tessellated independently; boundary-vertex alignment between adjacent faces is fragile. The 71 fresh tris at `|E \ Bb|` show γ retess produces face-by-face independent CDT; seam mismatch at intersection-curve boundaries is a plausible mechanism for sub-class (a) (verts present, triangle indices different).
4. **Reverse-direction canary** — start from the 24 Case-D positions, walk backwards through the Render LOD output's nearest-vertex set to find where they SHOULD have emerged from. Complementary to forward-direction Y45/Y46 pattern.

Per `feedback_no_last_bug`: do NOT declare PR-Y47 will close F0020. The 15-cycle arc has produced anchor sharpness without closure; PR-Y47 may be the 12th INFRA SHIP (sub-bisection refines further) or the first production-fix attempt (sub-bisection identifies a load-bearing sub-stage ≥ 80%) — either is consistent with the discipline.

### §4.5 What this audit explicitly refutes

- **`face_survival_detect` (audit-y45 §4.1 prescription) as the F0020 Case-D anchor.** 0/24 byte-stable across 6 reruns (3 canary + 3 adversary). Audit-y45 §4.1 prescription is empirically REFUTED.
- **The audit-y45 §4.1 "108-tri drop spans face_survival_detect + γ retess" framing in its primary clause.** The drop spans `Bb → B` (171 canonical-tri Layer A drops, NONE of which are Case D) + `B → E` (194 canonical-tri Layer B drops, ALL 24 Case D among them). face_survival_detect is monotonically selective (`|B \ Bb| = 0` confirmed); it does NOT drop any Case D position.

### §4.6 What this audit explicitly accepts

- **γ Render-LOD re-tessellation at `yang_integration.rs:1024` is the COARSE PR-Y47 anchor.** 24/24 = 100% direct measurement; 6 independent reruns; cross-implementation Rust/Python oracle agreement; partition-invariant verified (0 stragglers). First positive-measurement anchor in 15 cycles.
- **The bisection methodology is durable reusable infrastructure.** +289 LOC additive, env-var-driven, default-off byte-parity, set-diff arithmetic with explicit partition-invariant sanity check. PR-Y47+ can extend the same scaffold to additional partition points (F.0/F.1/F.2/F.3/F.4 + B-Rep assembly) by adding more `Y46_*_DIR` env vars and partition-difference rows.
- **The decision-gate discipline.** Layer-B-dominant at 100% fires SHIP-INFRA + anchor recommendation correctly; no production code committed on coarse anchor before sub-bisection.
- **face_survival_detect IS empirically monotone-selective.** `|B \ Bb| = 0` confirmed across 6 reruns. Yang §3.3 + Cherchi 2022 §5 selective-retention semantics validated empirically for F0020.

---

## §5 Strategic context — 15 cycles, 0 production, FIRST positive-measurement anchor

### §5.1 15-cycle accounting (extending audit-y45 §6.1)

| PR | Outcome | Cycle role |
|---|---|---|
| Y25-Y28 | ABORT (canary) ×4 | Wrong fix shapes caught at canary; D.1 split into 4 sub-mechanisms |
| Y36-Y38 | INFRA SHIP ×3 | Source-face attribution / H1-H3 / grid-sensitivity oracle |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 N=16 |
| Y40 | INFRA SHIP — 6th-refutation | N=16 refuted; measured N=4 |
| Y41 | INFRA SHIP — 7th-refutation | "Missing 12 upstream" refuted; strategic-pivot trigger fired |
| Y42 | INFRA SHIP — B.1 STRATEGIC PIVOT | First external-oracle measurement at Render LOD; 50% borderline |
| Y43 | INFRA SHIP — D-dominant + Case C=0 | F0020 90% accountable; Case C=0 byte-stable; (α/γ) co-equal |
| Y44 | INFRA SHIP — (a)-DOMINANT at 100% | (α/γ) anchor MEASURED; PR-Y45 anchor (C) α primary + γ bisection canary |
| Y45 | INFRA SHIP — α REFUTED at 0/24 | α empirically refuted; PR-Y46 anchor `face_survival_detect` PLAUSIBLE |
| **Y46** | **INFRA SHIP — face_survival_detect REFUTED at 0/24 + γ retess CONFIRMED at 24/24** | **2nd consecutive audit-recommended anchor refuted at measurement; FIRST positive-measurement next-cycle anchor; PR-Y47 anchor γ retess STRONG** |

**Cumulative cycle accounting (15 cycles):**

- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); **10 INFRA SHIPs** (Y36/Y37/Y38/Y40/Y41/Y42/Y43/Y44/Y45/Y46); **0 production fix on F0020 Render LOD in 15 cycles**.
- Cumulative diagnostic LOC: ~1358 production-instrumentation + ~413 + 438 + 132 + 289 test-harness (Y42/Y43/Y44/Y46) + 191 kernel probe (Y45) = **~2821 LOC cumulative diagnostic infrastructure**.
- F0020 unpaired count: **40 → 40 across all 15 cycles**.

### §5.2 PR-Y46 is the FIRST positive-measurement next-cycle anchor

PR-Y43/Y44/Y45 produced REFUTATION-CHAIN anchors: each cycle ruled out a candidate (Case C = 0 byte-stable; α REFUTED at 0/24 byte-stable; face_survival_detect REFUTED at 0/24 byte-stable). PR-Y43/Y44/Y45's next-cycle anchor recommendations were inferred from the refutation pattern + paper-anchored hypothesis ranking, not from positive measurement at the recommended anchor.

PR-Y46 changes this. The bisection at Bb → B → E partitions the 24 Case D positions into Layer A (0/24) and Layer B (24/24). The 24/24 at Layer B is the FIRST DIRECT POSITIVE MEASUREMENT of a load-bearing layer at the recommended anchor magnitude — not inference, not refutation-by-elimination, but a clean position-co-location measurement that 100% of the defect-attributable positions drop at the named layer.

Adversary §9.1 sharpens this distinction: the anchor recommendation has direct empirical basis ("Layer B = 24/24 = 100% direct measurement"), not just "the only un-refuted candidate". This crosses a discipline threshold per `feedback_anchor_before_fix`: positive measurement at the anchor is sufficient for the COARSE layer to be load-bearing; the SUB-ANCHOR within γ still requires another measurement cycle (per §4.2 Option A).

### §5.3 Per `feedback_phase1_diagnosis_ranking_is_inference`

PR-Y46 IS the textbook execution of this discipline. Audit-y45 §4.1 ranked `face_survival_detect` as PLAUSIBLE-BUT-NOT-CONFIRMED based on the 108-tri cumulative drop magnitude argument; PR-Y46 canaried face_survival_detect with the Stage Bb→B position-co-location probe; canary returned 0/24 ⇒ inference REFUTED. **For the second consecutive cycle, the audit-recommended anchor failed the measurement-first gate.** The base rate for "audit's inferred next-cycle anchor is correct" is now 0/2 over the cycles where this discipline was applied.

The lesson for PR-Y47: **even when the next-cycle anchor recommendation has 100% direct measurement at the coarse level (γ retess Layer B), treat the SUB-ANCHOR within γ as inference and canary at the sub-stage drop set BEFORE scoping fix-shape.** This is `feedback_phase1_diagnosis_ranking_is_inference` applied recursively to the sub-layer — measurement-discipline at every layer, not just the top one.

### §5.4 Per `feedback_no_last_bug`

PR-Y46 does NOT close F0020. F0020 Status:Failed remains at 40 unpaired across all 15 cycles. PR-Y46 sharpens the PR-Y47 anchor from "face_survival_detect PLAUSIBLE-BUT-NOT-CONFIRMED" to "γ Render-LOD retess COARSE-CONFIRMED at 100% + SUB-ANCHOR PENDING." If PR-Y47 produces another INFRA cycle (sub-bisection identifies a sub-stage at < 80%; pivot to upstream B-Rep assembly or per-face CDT), that is the disciplined outcome per `feedback_no_last_bug`. The 15-cycle ABORT-or-INFRA rhythm continues to produce anchor sharpness; PR-Y47 may itself be the 12th INFRA SHIP or the first production-fix attempt — either is consistent.

---

## §6 Strategic-pivot ROI update — POSITIVE remains, FIRST CONNECTED CHAIN

| PR | F0020 measurement strength |
|---|---|
| PR-Y41 (pre-pivot) | "Missing 12 upstream" inference refuted; strategic-pivot trigger fired |
| PR-Y42 (pivot) | 50.0% borderline-sharp attribution; cohort `common=0` method-limit |
| PR-Y43 | 90% accountable (D + B); Case C = 0 byte-stable; (a) sub-class inferred |
| PR-Y44 | (a) sub-class MEASURED at 100% across 8 combined reruns + 2 cohort cases; α/γ candidates paper-anchored |
| PR-Y45 | α empirically REFUTED at 0/24 byte-stable across 30/30 invocations; PR-Y46 anchor `face_survival_detect` |
| **PR-Y46 (this PR)** | **face_survival_detect REFUTED at 0/24 + γ retess CONFIRMED at 24/24 = 100.0% byte-stable across 6 reruns; FIRST positive-measurement next-cycle anchor in 15 cycles** |

**Strategic-pivot ROI: POSITIVE remains, FIRST CONNECTED CHAIN of 4 measurements.**

The empirical chain now consists of FOUR connected measurements that jointly localize the defect:

1. **PR-Y42:** 24/40 F0020 unpaired edges attributed to Cherchi-only-missing-from-Waffle Render LOD. (50% — borderline; the strategic pivot to Render-LOD diff harness localizes the defect to the post-pipeline output.)
2. **PR-Y44:** 24/24 Case D positions sub-classify as (a) `m1x=3, m5x=3` — all three vertices present at 1×/5× grid, only the triangle indexing is missing. (100% — sub-class concentration.)
3. **PR-Y45:** 0/24 Case D positions drop at α (F.0 `remove_winding_insensitive_duplicates`). (0% — α REFUTED.)
4. **PR-Y46:** 0/24 Case D positions drop at `face_survival_detect` (Stage Bb → Stage B); 24/24 drop at γ Render-LOD re-tessellation (Stage B → Stage E_lod=Render). (100% Layer B at γ retess; 0% Layer A at face_survival_detect — face_survival_detect REFUTED + γ retess CONFIRMED.)

**Joint conclusion:** F0020's 24 Case-D defect is **at γ Render-LOD re-tessellation (`tessellate_waffle_solid` at `yang_integration.rs:1024`), NOT at α/F.0 dedup AND NOT at face_survival_detect/Stage 3 selective-retention.** The defect manifests as triangle-topology drop while vertex-positions survive (Y44's sub-class (a) signature) — consistent with γ retess being a REPLACE-and-ADD layer (Y46's `|E \ Bb| = 71` ADDED tris + `|B ∩ E| = 36` preserved + 194 dropped — adversary §6.3) that emits a per-face independent CDT triangulation diverging from Cherchi's matched triangulation.

**This is the first cycle where the F0020 defect has been localized to a SINGLE NAMED LAYER with positive measurement evidence.** Prior cycles localized by elimination (ruling out candidates); PR-Y46 confirms γ retess directly. The PR-Y47 sub-bisection will further narrow γ to a specific sub-stage (F.0 / F.1 / F.2 / F.3 / F.4 / B-Rep assembly).

The strategic pivot (B.1) has now produced FIVE consecutive INFRA cycles (Y42/Y43/Y44/Y45/Y46) that each advance F0020 anchor sharpness without producing a regression and without claiming closure. Per `feedback_external_coherence`: Cherchi C++ remains the load-bearing reference oracle; PR-Y46 reuses the PR-Y14a + PR-VIZ-1 + PR-Y29 → PR-Y45 stage-dump + canonical-key data lineage with no new oracle invocation pattern.

Per `feedback_no_last_bug`: 15th cycle on F0020 Render LOD. PR-Y46 does NOT close F0020. PR-Y46 produces the first POSITIVE coarse-level anchor in 15 cycles and narrows PR-Y47's anchor candidate space to within γ retess. PR-Y47 may itself be the 11th INFRA SHIP if the sub-bisection refutes the top γ sub-stage candidates and pivots to upstream B-Rep assembly — that outcome is consistent with the discipline.

---

## §7 Banked / open (forward-carry)

### §7.1 Banked for PR-Y47 (PRIMARY: sub-bisection canary; secondary candidates)

1. **γ Render-LOD re-tessellation at `tessellate_waffle_solid` (`crates/kernel/src/boolean/yang_integration.rs:1024`) — COARSE PR-Y47 anchor (24/24 = 100% measured).** Paper anchor Yang 2025 §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:548-590`) + Livesu 2021 CDT. **PR-Y47 INFRA canary MUST sub-bisect F.0 / F.1 / F.2 / F.3 / F.4 + upstream B-Rep assembly stage + per-B-Rep-face attribution BEFORE fix-shape commit.** (Option A per §4.2.) Already-captured `/tmp/y46-stages-f0020/F0020/stage_F.{0,1,2,3,4}.obj` dumps make sub-bisection near-free; estimated ~50-100 LOC extension to `f0020_stage_bb_b_e_bisection` probe in `cherchi_differential_diff.rs`.

2. **`assemble_brep_topology` (`crates/kernel/src/boolean/topology_extract.rs`) — UPSTREAM ALTERNATIVE.** Per adversary §9.2 Q1: B-Rep assembly between Stage C (post-flood-fill) and γ retess input may lose triangulation info such that γ retess cannot recover it. PR-Y47 canary should add a dump-point at B-Rep assembly output to bisect "B-Rep assembly drop" vs "γ retess drop". Probe is required to discriminate; the COARSE Y46 measurement does NOT distinguish these two sub-mechanisms.

3. **`tessellate_solid_ext_with_lod` per-face independent CDT seam alignment.** Each B-Rep face is re-tessellated independently in γ; the `|E \ Bb| = 71` ADDED tris demonstrate that γ emits face-by-face fresh triangulation. Shared edges across adjacent faces may receive different vertex projections, causing crack-prone seams. If F-stage sub-bisection shows the 24 Case D are missing already at F.0 input, this is a strong candidate.

4. **`flood_fill_patches` patch-segmentation** — PR-Y27 banked; canary §9.1 SECONDARY. Probe if F.0→F.4 sub-bisection AND B-Rep assembly stage both refute.

5. **Reverse-direction canary** — start from 24 Case-D positions, walk backwards through the Render LOD output's nearest-vertex set to find where they SHOULD have emerged from. Complementary to forward-direction Y45/Y46 pattern; per audit-y45 §4.3 carry-over.

### §7.2 Banked from this audit cycle

1. **`parse_obj` line 136 silent 0-index→0 mapping** — pre-existing hygiene smell; not Y46-introduced; not load-bearing under current OBJ writer. Per adversary §7. Banked.

2. **Layer A+B unreachable branch (line 1846)** — defensive; sound under invariant `B ⊆ Bb` which the probe verifies. Per adversary §7. Banked.

3. **47-mode bound not exercised** — 8/8 reruns produced 42-mode under default thread count. Bound is sound algebraically (worst-case Layer B = 24/26 = 92.3% > 80%) but not stress-tested in 47-mode. Per adversary §6.5. Banked for future cohort cases.

4. **Vertex-emit-index swap in Render-LOD OBJ** — HashMap iteration-order non-determinism; canonical-key-invariant (probe's set-arithmetic robust). Per adversary §3.2. Banked as expected OBJ-writer non-determinism.

5. **`stage_E_lod=Adaptive_*.obj` filename collision risk** — pre-existing PR-VIZ-1 banked item; not Y46-introduced. Banked.

### §7.3 Open for PR-Y48+

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44/Y45/Y46 (only the 24 Case D bordering unpaired edges classified). δ + Y45 + Y46 probes are sub-class-extensible to the wider 194-tri set if γ retess sub-bisection covers only part of the 24 (which it doesn't — 100% — but a wider-set bisection at PR-Y48+ would still be valuable).

2. **Cohort F0044/F0045/R0092 generalization at γ retess sub-stage.** If PR-Y47 fires GREEN on F0020, run the same sub-bisection against the cohort (which also has 100% sub-class (a) per PR-Y44 §6.3 at the unpaired-edge subset).

3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y47+ at best closes ~20.

### §7.4 Methodological banked

1. **Cross-implementation Rust/Python oracle pattern IS the canonical adversary cross-check.** Adversary §5.4 byte-matched Rust probe via a pure-Python re-derivation of OBJ-parse + canonical-key + set-arithmetic. Per adversary §10.3: "recommend as standard for INFRA-PR adversary cycles going forward." Codify as the standard adversary methodology for any future INFRA cycle with critical set-arithmetic.

2. **Y46-style three-stage bisection IS the canonical pattern for cumulative-drop attribution.** Replaces single-layer canary when the cumulative drop spans ≥ 2 candidate layers. Reusable for PR-Y47+ sub-bisections at any future drop layer pair (F.0/F.1, F.1/F.2, B-Rep-assembly/γ-retess, etc.).

3. **Decision-gate at canary phase, not at impl phase.** PR-Y46 saved the cost of a wrong-anchor implementation cycle at face_survival_detect by aborting Layer A at canary. Per `feedback_anchor_before_fix`: this is the discipline working as designed; the recursive application to PR-Y47 sub-bisection follows directly.

4. **Adversary process-slip discipline maintained.** Adversary-y46 §1.3 verified zero destructive git operations (no `git stash`, no `git checkout --`, no `git reset`). Per audit-y45 §5.3 forward-carry: this is the second cycle reinforcement after the PR-Y45 stash-pop slip. PR-Y47 adversary brief should continue to re-emphasize `feedback_adversary_no_destructive_git`.

### §7.5 Per `feedback_no_last_bug`

PR-Y46 does NOT promise PR-Y47 will close F0020. The γ retess confirmation narrows PR-Y47's anchor candidate space at the COARSE level but does NOT confirm any specific sub-stage WITHIN γ retess IS the load-bearing sub-anchor. PR-Y47 may be the 11th INFRA SHIP if sub-bisection refutes the top candidates (F.0 / F.1 dedup; B-Rep assembly) — that is the disciplined outcome.

---

## §8 Final recommendation

**ACCEPT (SHIP-INFRA) — Layer A (`face_survival_detect`) REFUTED at 0/24; Layer B (γ Render-LOD retess) CONFIRMED at 24/24 = 100.0% byte-stable across 6 reruns; PR-Y47 anchor = γ retess at `crates/kernel/src/boolean/yang_integration.rs:1024` COARSE-CONFIRMED; PR-Y47 = Option (A) INFRA sub-bisection canary BEFORE fix-shape commit.**

Rationale:

- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec-y46 / canary-y46 / impl-y46 / adversary-y46). INFRA-CLASS test-author waiver consistent with Y29/Y33/Y36/Y37/Y38/Y40/Y41/Y42/Y43/Y44/Y45 precedent.
- **DoD §1.5 GREEN** — probe-off byte parity load-bearing; verified independently by canary Gate 2 + adversary Gate B against impl-y46. PR-Y31 hard gate `pr_y31_f0044_extras_zero` preserved (adversary Gate H).
- **INFRA-CLASS framing intact** — 0 LOC production logic; 0 kernel runtime change; 0 wasm-bridge; 0 app; only test-harness probe extension (+289 LOC at `crates/test-harness/tests/cherchi_differential_diff.rs`, 1652 → 1943) + memos. No WASM rebuild required.
- **A15.6 compliant** — paper-orthogonal Render LOD position-co-location bisection probe; A15.4/A15.5 unaffected; A15.6 Stage B byte-parity gate preserved.
- **Empirical evidence load-bearing** — Layer A REFUTED at 0/24 + Layer B CONFIRMED at 24/24 = 100.0%; canary 3 reruns + adversary 3 reruns = **6 / 6 byte-identical at 0/24 + 24/24**; cross-implementation Rust/Python oracle agreement on all reported counts (171/194/41 + |B\Bb|=0 + |E\Bb|=71 + |Bb∩E|=41 + |B∩E|=36 + Case D 0/24 + 24/24); 7/7 stress-test axes resolved.
- **Sub-anchor caveat preserved** — γ retess is COARSE-CONFIRMED; SUB-ANCHOR within γ (F.0/F.1/F.2/F.3/F.4 sub-stages + upstream B-Rep assembly stage + per-face independent CDT seam alignment) is INFERENCE pending PR-Y47 INFRA canary sub-bisection per Option (A) §4.2.
- **No-last-bug discipline GREEN** — 15 cycles, 0 production-fix LOC on F0020 Render LOD, F0020 Status:Failed unchanged at 40 unpaired. PR-Y46 produces the FIRST positive-measurement next-cycle anchor in 15 cycles AND simultaneously refutes the audit-y45 anchor; does NOT promise PR-Y47 will close F0020.
- **Strategic-pivot ROI POSITIVE advancing; FIRST CONNECTED CHAIN** — five consecutive INFRA cycles (Y42/Y43/Y44/Y45/Y46) each advanced F0020 anchor sharpness without regression. PR-Y46 is the 15th investigational PR and 10th INFRA SHIP; the disciplined face_survival_detect refutation + γ retess confirmation jointly localize the defect to γ retess for the first time. The 4-step empirical chain (Y42 → Y44 → Y45 → Y46) is now connected with positive measurement evidence at the γ retess COARSE anchor.
- **PR-Y47 anchor explicit + canary discipline mandatory** — γ retess at `yang_integration.rs:1024` is COARSE-CONFIRMED at 24/24 = 100%; PR-Y47 MUST run its own Y46-style position-co-location sub-bisection canary across F.0/F.1/F.2/F.3/F.4 + upstream B-Rep assembly stage BEFORE committing fix shape. The discipline pattern repeats recursively at the sub-layer.

**PR-Y47 anchor (definitive one-sentence statement for memory file's "PR-Y47 anchor" field, verbatim per §4.1 + §4.2):**

> **PR-Y47 anchor = γ Render-LOD re-tessellation in `tessellate_waffle_solid` at `crates/kernel/src/boolean/yang_integration.rs:1024` (and underlying `tessellate_solid_ext_with_lod` in `crates/kernel/src/tessellation/mod.rs`). Status: COARSE-CONFIRMED at 24/24 = 100.0% direct measurement (byte-stable across 6 reruns); SUB-ANCHOR within γ retess (F.0/F.1/F.2/F.3/F.4 sub-stages + upstream `assemble_brep_topology` B-Rep assembly stage + per-face independent CDT seam alignment) remains INFERENCE pending PR-Y47 INFRA sub-bisection canary. Paper anchor: Yang 2025 §4.4.1 (mesh-updating + bijective CDT re-mesh; `refs/text/yang2025_hybrid_boolean.txt:548-590`) + Livesu et al. 2021 simplified earcut. PR-Y47 = Option (A) — INFRA canary extending `f0020_stage_bb_b_e_bisection` to per-F-stage partition points + upstream B-Rep assembly dump-point, applying the same decision-gate (≥ 80% / ≤ 20% / mixed) as PR-Y45/Y46 BEFORE committing fix shape. The 15-cycle pattern (2/2 audit-recommended sub-anchors refuted at measurement under Option C-like assumptions) argues unambiguously for measurement-first sub-bisection at every layer.**

**Phase 8 push authorized.** Recommend:

1. Commit canary memo + adversary memo + this audit memo + spec + impl probe extension (`audit(yang-pr-y46): ACCEPT (SHIP-INFRA) — face_survival_detect REFUTED at 0/24; γ retess CONFIRMED at 24/24 = 100%; PR-Y47 anchor γ retess at yang_integration.rs:1024 (COARSE-CONFIRMED; sub-anchor pending INFRA sub-bisection)`).
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y46_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, face_survival_detect REFUTED at 0/24, γ retess CONFIRMED at 24/24, PR-Y47 anchor γ retess COARSE-CONFIRMED (Option A sub-bisection canary required) — verbatim per §4.1.
4. `TeamDelete pr-y46` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y47 should treat the SUB-ANCHOR within γ retess as INFERENCE (the COARSE anchor at γ retess IS positive-measurement-confirmed; the SUB-ANCHOR is not) and run the Y46-style position-co-location sub-bisection canary across F.0/F.1/F.2/F.3/F.4 + upstream B-Rep assembly stage BEFORE scoping fix-shape. The Y46 probe scaffold is durable reusable infrastructure (+289 LOC additive, env-gated, set-diff + decision-gate + partition-invariant sanity check) reusable for PR-Y47+ sub-bisections at any future drop-layer pair. The 15-cycle 0-production-code arc continues; the face_survival_detect refutation + γ retess confirmation jointly produce the FIRST positive-measurement next-cycle anchor in 15 cycles AND the FIRST connected 4-step empirical chain (Y42 → Y44 → Y45 → Y46) — discipline victories at the cost of one well-spent INFRA cycle.
