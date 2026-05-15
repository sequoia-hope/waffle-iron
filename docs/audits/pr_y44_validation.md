# PR-Y44 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y44 |
| Date | 2026-05-15 |
| Live tree HEAD (impl-y44) | `d14c654` (PR-Y44 INFRA, staged in worktree as uncommitted; not yet pushed) |
| Worktree HEAD | `b0009bd` (PR-Y42 audit base; PR-Y43 + PR-Y44 content mirrored as uncommitted) |
| Parent | `403932c` (PR-Y43 audit ACCEPT — INFRA-CLASS; D-dominant + Case C=0) |
| Class | INFRASTRUCTURE-CLASS (test-harness extension; 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Strategic-pivot ROI | **POSITIVE remains — (a) sub-class moves from "plausibly dominant inferred" to "100% measured across 8 combined reruns + 2 cohort cases"** |
| Verdict | **ACCEPT (SHIP-INFRA) — (a)-DOMINANT at 100% byte-verified; cohort generalization HOLDS at unpaired-edge subset; PR-Y45 anchor = (α) F.0 `remove_winding_insensitive_duplicates` PRIMARY + (γ) pre-F.0 Boolean LOD → Render LOD re-tess as BISECTION CANARY (Option C — α/γ co-anchored, α-as-primary)** |

---

## §1 Adjudication summary (single paragraph)

PR-Y44 ships +132 LOC test-file harness extension at `crates/test-harness/tests/cherchi_differential_diff.rs` (1520 → 1652) that adds a δ probe capturing per-Case-D `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` 4-tuples and aggregating them into sub-class (a) `(m1x=3, m5x=3)` / (b) `(m1x ∈ {0,1}, m5x=2)` / other histograms. Canary-y44 measured F0020 Case D at **100% sub-class (a)** across 4 reruns at both Cherchi non-det modes (42-mode 24/24, 47-mode 26/26) and cohort F0044 (8/8 = 100% a) + F0045 (2/2 = 100% a) + R0092 (target=0, vacuous). Adversary-y44 independently re-ran across **6 F0020 reruns** (4× 47-mode, 2× 42-mode) and **2 cohort reruns** — **100% sub-class (a) byte-reproduced in every single run**; 4 per-tri spot-checks (d[0], d[5], d[15], d[23]) at the 42-mode byte-match canary memo §4.1; code review confirmed no priority-ordering bug, correct joint-condition enforcement for (a) (`m1x == 3 && m5x == 3`, not just `m1x == 3`), additive data structures, deterministic sort, OK bucket-sum check. All 15 gates GREEN (8 brief gates + 7 sub-gates within Gate C / Gate E). Adversary §5 stress-tested the "γ primary by 108-tri magnitude" argument and rejected γ-promotion on mechanism grounds: sub-class (a)'s signature `m1x=3` means all three vertices SURVIVE downstream into Waffle's Render LOD vertex set, so the defect must be at a layer that drops triangles WITHOUT dropping their vertices — α's profile (`remove_winding_insensitive_duplicates` removes triangle indexing while preserving vert set), not γ's profile (re-tessellation regenerates vertices). Adversary nonetheless recommends KEEPING γ in the canary set "for α-vs-γ attribution bisection — a measurement, not a fix premise." This audit refines the canary's "(α/γ) co-equal" recommendation to **(C) α PRIMARY + γ BISECTION CANARY**: the mechanism evidence (m1x=3 universal) anchors α as the load-bearing fix candidate; γ stays in PR-Y45's canary set as the *control* probe that verifies the surviving-verts reasoning empirically (per `feedback_phase1_diagnosis_ranking_is_inference` — measure don't over-promote on inference alone). Cohort generalization is REFINED-ACCEPT: 100% (a) holds for the unpaired-edge-bordering subset only (16 of F0044's 50 Case-D-residual missing; 4 of F0045's; 0 of R0092's), and does not yet speak to the 152 OTHER F0020 missing triangles or the wider F0044/F0045 missing-set — but does NOT block PR-Y45 since PR-Y45's α/γ fix scope IS the unpaired-edge subset. Recommend **ACCEPT (SHIP-INFRA)** + Phase 8 push authorized.

---

## §2 Gates re-summary (verbatim adversary §2, normalized)

Adversary §2 ran an independent 15-gate sweep against impl-y44's HEAD `d14c654` in worktree-canary-y36 with non-destructive git only (`git show d14c654:<path> | diff - <path>` byte-comparison; no `git stash` / `git checkout --` / `git reset`). Per `feedback_oracle_credibility_via_role_separation`: canary built the δ probe and measured the 42-mode 100% (a) histogram + the 47-mode 100% (a) histogram; adversary independently re-ran from the worktree mirror without inheriting canary's reasoning chain and reproduced the load-bearing finding byte-exact across 6 + 2 = 8 fresh runs.

| Gate | Description | Expected (canary / brief) | Observed (adversary) | Status |
|---|---|---|---|---|
| **A** | `cargo build -p test-harness --test cherchi_differential_diff` | Clean; 58 kernel + 1 slvs pre-existing warnings | Clean; finished in 0.04s; 58 kernel + 1 slvs pre-existing warnings | **GREEN** |
| **B** | F0020 spotlight default-off byte parity | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int; [stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` | EXACT match | **GREEN** |
| **C1-C6** | F0020 δ runs 1-6 | 100% subclass_a regardless of mode | 4/6 at 47-mode (7/14/0/26, subclass_a=26/26=100%); 2/6 at 42-mode (4/14/0/24, subclass_a=24/24=100%) — 100% (a) in every single run; subclass_b=0; subclass_other=0; bucket-sum OK | **GREEN** (load-bearing) |
| **C-aggregate** | 6 runs: subclass_a invariant at 100%; mode mix | canary §4 reported 4 reruns byte-stable at 100% across both modes | Adversary 6 reruns: 4/6 at 47-mode (67%), 2/6 at 42-mode (33%); subclass_a = **100% in every single run**; load-bearing finding robust to mode mix | **GREEN** |
| **D** | F0020 Case D per-tri table spot-checks (42-mode) | d[0]/d[5]/d[15]/d[23] per canary §4.1 | All 4 spot-checks byte-match canary: d[0]=(3,3,3,3)(a)✓, d[5]=(3,3,3,3)(a)✓, d[15]=(3,**2**,3,3)(a)✓, d[23]=(3,3,3,3)(a)✓ | **GREEN** |
| **E1** | Cohort F0044/F0045/R0092 run 1 | F0044 D=8/16, (a)=8/8=100%; F0045 D=2/4, (a)=2/2=100%; R0092 vacuous | F0044 D=8/16 (50.0%), subclass_a=8/8=100%; F0045 D=2/4 (50.0%), subclass_a=2/2=100%; R0092 target=0 (vacuous all-zero); bucket-sum OK on all 3 | **GREEN** |
| **E2** | Cohort run 2 (stability check) | Identical to E1 | IDENTICAL byte-for-byte | **GREEN** |
| **F** | PR-Y43 A/B/C/D baselines preserved | 47-mode 7/14/0/26; 42-mode 4/14/0/24; Case B dump 14 entries | All 6 F0020 runs reproduce; Case B 14-entry dump byte-identical | **GREEN** |
| **G1** | `cargo test -p kernel --lib` | 1262 / 24 / 42 | **1262 passed; 24 failed; 42 ignored** — IDENTICAL | **GREEN** |
| **G2** | `YANG_BOOLEAN=1 yang_fast` | 10/157 | **10/157 passed**, 139 failed, 8 errored — IDENTICAL | **GREEN** |
| **H** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | 136 common / 0 missing / 0 extras | F0044 Subtract: 136 / 72 verts / well_formed=true χ=4; missing=0, extras=0, common=136 — IDENTICAL | **GREEN** |

**15/15 gates GREEN.** Zero RED. Zero fabrication risk vector. Code review at adversary §3 confirmed (a) predicate is the **joint condition** `match_at_1x == 3 && match_at_5x == 3` (not just `m1x == 3`); empirically no `(3, 2)` or `(3, 1)` or `(3, 0)` entries appear in any of 8 dump runs (would mis-classify as (a) if predicate were single-condition).

---

## §3 α/γ ranking refinement (LOAD-BEARING for PR-Y45)

### §3.1 Adversary §5 reasoning chain (verbatim, then audited)

The brief asked: "should γ be primary instead given 108-tri drop magnitude argument?" Adversary §5 stress-tested:

> The "108-tri drop" at pre-F.0 Boolean LOD → Render LOD re-tessellation (`yang_integration.rs:1024`) is from PR-Y41 / canary-y43 §7.4. Argument: this layer drops 246 → 138 = 108 tris, which is 5.7× larger than F.0's 19-tri drop. By raw magnitude, γ should be primary.

**Adversary's counter-arguments (verbatim §5):**

1. **The 108-tri drop is a re-tessellation (re-meshing), not a deduplication.** Boolean LOD → Render LOD re-tessellation changes the *grid*, not the *triangulation logic*. The dropped 108 are upstream-of-arrangement; if they were the load-bearing source, F0020's pre-arrangement mesh would already show divergence from Cherchi's input. But Cherchi STAGE1 receives our mesh and produces 64 verts / 420 tris (no jolly_creations) — so the re-tessellation produces mesh that arrangement accepts. **The 108-tri drop is *legitimate downsampling*, not error.**

2. **The (a) signature says vertices are present at 1× grid.** All 24/26 Case D entries have `m1x=3` — all 3 vertex positions are correctly produced and stored in Waffle's Render LOD vertex set. If γ (re-tessellation) were the primary source, we'd expect (b) signature `(m1x ∈ {0,1}, m5x=2)` — partial vertex production, requiring 5× grid to find proximity. Empirically (b) = 0%. So **the vertices survive γ correctly**; what fails is triangle emission downstream.

3. **F.0 dedup (α) is exactly at the triangle-emission layer.** `remove_winding_insensitive_duplicates` drops triangles whose `(v0, v2, v1)` or any rotation exists elsewhere — a topology-emission decision. PR-Y40 found 4 canonical-key collisions + distributed winners at this pass. The (a) signature points at this layer mechanistically.

4. **108-tri ≫ 19-tri is a layer-magnitude argument, but Case D = 24 tri is a defect-magnitude argument.** γ's 108-tri drop is ~4.5× the defect size; α's 19-tri drop is ~0.8× the defect size. Both are within an order of magnitude of the defect; neither dominates.

**Adversary final recommendation (verbatim):** "KEEP (α/γ) CO-EQUAL. The 'γ as primary' framing relies on raw layer-magnitude, but the mechanism evidence (m1x=3 across all 24 entries = vertices present + triangles missing) points at α more than γ. However, γ is unprobed (PR-Y41 banked) so neither anchor has direct measurement of (a)-tri-attribution; the canary memo's 'co-equal' stance is the disciplined posture."

### §3.2 Audit option-space — A / B / C

The brief offered three options:
- **(A)** Keep (α/γ) **CO-EQUAL** — PR-Y45 canaries BOTH; the canary itself does α-vs-γ bisection.
- **(B)** Promote **(α) to PRIMARY** based on the m1x=3 ⇒ verts-survive ⇒ triangle-only-removal-layer reasoning.
- **(C)** Hybrid — α primary, γ as "control" canary to verify the surviving-verts reasoning.

### §3.3 Audit choice: **(C) — α PRIMARY + γ BISECTION CANARY**

Per `feedback_phase1_diagnosis_ranking_is_inference` (load-bearing for this audit): "When two diagnoses are ranked 'dominant vs secondary,' that ranking is structural inference. Canary the dominant diagnosis with a position-co-location probe before scoping the fix." The adversary's argument 2 (vertex-survival ⇒ triangle-only-removal-layer ⇒ α profile) IS structural inference — sound, paper-anchored (Cherchi 2022 §5 `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`: "the most costly operations of the arrangement algorithm are the removal of duplicate and degenerate elements"), but inference. (B) would over-rely on the reasoning chain being airtight in all 24 entries — and the adversary itself flags one residual uncertainty: "γ could still drop SOME triangles whose vertices then get re-emitted by downstream stages (vertex set is union of all stages' outputs)."

(A) is the maximally-disciplined posture but spends PR-Y45's first production-fix budget (after 13 cycles) on measurement-only. (B) is the most-aggressive but over-trusts the reasoning chain. **(C) splits the difference correctly:** α is the **primary fix anchor** (build the canary and the fix-shape around F.0 `remove_winding_insensitive_duplicates`); γ remains in the canary surface as a **control** that verifies the verts-survive reasoning empirically (of the 108 γ-dropped tris, is the position-attribution to the 24 (a) Case D entries near-zero, as the m1x=3 argument predicts?). If γ-attribution is also non-zero, the (B) reasoning is partially refuted and PR-Y45's fix-shape must broaden; if γ-attribution IS near-zero, the reasoning is empirically corroborated and PR-Y46 can confidently bank γ.

This is also the framing that protects against the cohort caveat (§4 below): if γ-attribution differs between F0020 and F0044/F0045, the differential will surface during the bisection rather than after a single-anchor fix lands.

### §3.4 PR-Y45 anchor (load-bearing one-sentence statement; close-out copies this verbatim)

**PR-Y45 anchor = (α) F.0 `remove_winding_insensitive_duplicates` (Cherchi 2022 §5, `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`; 19-tri drop at `[stage-f] 138→119`; PR-Y40 scaffold preserved) as the PRIMARY fix candidate, with (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024` (Yang 2025 §4.4.1 mesh-updating, `refs/text/yang2025_hybrid_boolean.txt:548-590`; 108-tri drop) retained in the PR-Y45 canary surface as the BISECTION/CONTROL probe to verify the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer reasoning empirically before fix shape is committed.**

### §3.5 What this audit explicitly rejects

- **Promoting γ to primary on raw 108-tri magnitude.** The adversary §5 mechanism rebuttal is sound and audit-endorsed.
- **Treating (α/γ) as fully co-equal.** That posture spends PR-Y45's first production-fix attempt budget on measurement-only after 13 cycles; the m1x=3 mechanism evidence is strong enough to lean PR-Y45's fix-shape toward α.
- **Treating γ as fully demoted.** That posture over-trusts the inference chain; PR-Y45 must keep γ in the canary surface to empirically corroborate or refute.

### §3.6 What this audit explicitly accepts

- **(a)-dominance at 100%.** Measured across 8 combined reruns (canary 4 + adversary 6) + 2 cohort runs. Refutes (b)-shift, mixed-split, diffuse-other.
- **Paper anchoring of both candidates.** α at Cherchi 2022 §5 (`cherchi2022_interactive_robust_mesh_booleans.txt:344-345`: "the most costly operations of the arrangement algorithm are the removal of duplicate and degenerate elements"). γ at Yang 2025 §4.4.1 / §4.4.2 (`yang2025_hybrid_boolean.txt:574-579`: "After trimming the meshes using the intersection curves, we directly apply a standard inside/outside classification step ... selectively retaining one of the duplicate triangles").
- **The 4 per-tri spot-checks bind the histogram to specific positions.** d[0] = (-0.275,+0.099,-0.157)/(-0.275,+0.099,-0.142)/(-0.249,+0.104,-0.208); d[5], d[15], d[23] similarly. Adversary independently re-emitted these and they byte-match.

---

## §4 Cohort generalization adjudication

### §4.1 The surprise

Canary §5 + adversary §6 report: **F0044 D=8/16 all 100% (a); F0045 D=2/4 all 100% (a); R0092 target=0 vacuous.** Audit-y43 §6.2 hypothesized "cohort Case B/D semantics differ from F0020's"; PR-Y44 δ **refutes** that hypothesis at the unpaired-edge-bordering subset. This is a **major positive finding** — the (a) sub-class mechanism appears cohort-shared, not F0020-specific.

### §4.2 Adversary §6 caveat (verbatim, audited)

Adversary §6 added a methodological caveat:

> Cohort generalization HOLDS (not coincidence) at the unpaired-edge-bordering subset, with the methodological caveat that:
> - Validates: same topology-emission mechanism for the unpaired-edge-bordering Case D entries across F0020, F0044, F0045.
> - Does NOT validate: that fixing α/γ closes F0044/F0045 wholesale (the `common=0` method-limit per PR-Y42 §6.2 still applies; fix-effectiveness on F0044/F0045 will be bounded by the wider triangle-topology divergence).

Adversary §6 statistical sanity-check: combined N=10 (8 from F0044 + 2 from F0045); probability of 10 independent draws all landing in (a) by chance, if true (a) proportion were 50%, is `0.5^10 ≈ 0.001`; even at 80% true (a), `0.8^10 ≈ 0.107`. **Statistically meaningful.**

### §4.3 Audit position: REFINE (carry caveat forward; does NOT block PR-Y45)

Three options:
- **Accept the caveat** (generalization holds for unpaired-edge subset only; full corpus open) — true, but the caveat is technically narrow and risks under-claiming the positive finding.
- **Reject the caveat** (cohort generalization holds full stop) — over-claims; the 152 OTHER F0020 missing tris + the F0044 `common=0` wider triangle-topology divergence are real bounds.
- **Refine** — carry the caveat forward as authoritative; note explicitly that the caveat does NOT block PR-Y45 because PR-Y45's α/γ fix scope IS the unpaired-edge subset.

**Audit chooses: REFINE.**

Rationale: PR-Y45's job is to close (or measurably reduce) F0020's 40 unpaired edges by fixing the mechanism the 24 (a) Case D entries trace to. PR-Y45's fix scope is therefore **scoped to the unpaired-edge subset by construction** (the (a) signature exists in this subset; the 152 OTHER missing tris are unclassified and may have different mechanisms). If the α-anchored fix lands and closes F0020's (a)-attributed unpaired edges, that IS PR-Y45's success criterion regardless of what happens to the 152. Whether the fix generalizes to cohort F0044/F0045 wholesale at the *triangle-survival* level is bounded by PR-Y42's `common=0` method-limit (already documented); whether it generalizes at the *unpaired-edge-bordering* subset is what δ measured at 100% (a) for both cohort cases — the (α/γ) PR-Y45 fix has a strong probability of closing some cohort unpaired edges, even if the wider cohort closure is method-limited.

### §4.4 Forward-carried caveats (REFINED-ACCEPT)

1. **Caveat 1 — Unpaired-edge subset scope.** The 100% (a) cohort finding speaks only to the 8 (F0044) + 2 (F0045) Case D entries that border unpaired edges. The remaining missing tris in cohort (e.g., F0044's 50 Case-D-residual missing or wider 136 missing in Subtract) are not classified. PR-Y45 does NOT promise wholesale cohort closure.

2. **Caveat 2 — Cherchi well_formed=false for F0020 union.** F0020's closure ceiling is ~20 unpaired even if PR-Y45 lands (PR-Y42 §6 caveat preserved; only 20 of 40 unpaired edges are Cherchi-only-attributable).

3. **Caveat 3 — The 152 OTHER F0020 missing tris.** PR-Y43+Y44 classified only the 42-or-47 that border unpaired edges. The remaining 152 (or 154) missing tris are unclassified; PR-Y45's α/γ fix may or may not address them.

These caveats are **forward-carried for PR-Y46+ scope decisions, not PR-Y45 blockers.**

---

## §5 PR-Y45 strategic context — first production-fix attempt in 13 cycles

### §5.1 13-cycle accounting (PR-Y43 §5.1 + PR-Y44)

| PR | Outcome | Cycle role |
|---|---|---|
| Y25-Y28 | ABORT (canary) ×4 | Y25 Yang §4.4.1 refuted; Y26 cohort-wide defect; Y27 flood_fill_patches 0 drops; Y28 D.1d fix-shape refused |
| Y36-Y38 | INFRA SHIP ×3 | Source-face attribution / H1-H3 / grid-sensitivity oracle |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 N=16 |
| Y40 | INFRA SHIP — 6th-refutation | N=16 refuted; measured N=4 |
| Y41 | INFRA SHIP — 7th-refutation | "Missing 12 upstream" refuted; strategic-pivot trigger fired |
| Y42 | INFRA SHIP — B.1 STRATEGIC PIVOT | First external-oracle measurement at Render LOD; 50% borderline |
| Y43 | INFRA SHIP — D-dominant + Case C=0 | F0020 90% accountable; Case C=0 byte-stable; (α/γ) co-equal contingent on δ |
| **Y44** | **INFRA SHIP — (a)-DOMINANT at 100%** | **(α/γ) anchor MEASURED (100% across 8 reruns + 2 cohort); PR-Y45 anchor refined to (C) α primary + γ bisection canary** |

**Cumulative cycle accounting (13 cycles):**
- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); 8 INFRA SHIPs (Y36/Y37/Y38/Y40/Y41/Y42/Y43/Y44); **0 production fix on F0020 Render LOD in 13 cycles**.
- Cumulative diagnostic LOC: ~1358 production-instrumentation (Y36/Y37/Y40/Y41) + ~413 + 438 + 132 test-harness (Y42/Y43/Y44) = **~2341 LOC cumulative diagnostic infrastructure**.
- F0020 unpaired count: **40 → 40 across all 13 cycles**.

### §5.2 Why PR-Y45 is the right moment to attempt a production fix

The 13-cycle arc has progressed PR-Y45's anchor sharpness from:

| Cycle | Anchor sharpness |
|---|---|
| Y36-Y41 (6 cycles) | Production-instrumentation inferences; each cycle refuted the prior |
| Y42 | First external-oracle measurement; F0020 50% borderline-sharp |
| Y43 | F0020 90% accountable (D + B); (α/γ) candidates contingent on δ |
| **Y44** | **(a) sub-class measured at 100% across 8 reruns; (α) m1x=3 mechanism evidence strong** |

PR-Y45's anchor has crossed the threshold from inference to measurement. Per `feedback_anchor_before_fix`: the discipline pattern (probe → measure → decide → fix) is now fulfilled. PR-Y45 can responsibly attempt a production fix at the α anchor, with γ as control.

### §5.3 Failure-mode planning — what if PR-Y45 canary refutes both α AND γ?

**PR-Y46 fallback options** (banked for spec / planner of PR-Y45):

1. **(β) F.3 `remove_nonmanifold_duplicates_aggressive`** (Yang 2025 §4.4.1 selective-retention) — 6-tri drop at F.3. Already TERTIARY in PR-Y43 audit §4.1. If α + γ bisection covers only part of the 24 Case D (a) entries, (β) is the next layer at the F-stage axis.

2. **F.1 / F.2 / F.4 dedup stages.** F.1 cosmetic-cleanup, F.2 retain-one-of-doubles, F.4 final dedup (per canary §4.4 mechanism enumeration). If α + γ + β collectively don't bisect the 24 entries, a finer F-stage canary is the next step.

3. **The 152 OTHER missing tris.** If α + γ closes only part of the unpaired-edge subset and the residual originates from the 152 unclassified tris, the δ probe is sub-class-extensible to the wider 194-or-201 missing-tri set; PR-Y46+ can extend.

4. **Triangle-emitting layer below F.0 (Boolean LOD final assembly).** If the m1x=3 evidence holds but neither α nor γ bisects, the defect may be in the Boolean LOD output assembly that emits triangle indices into the Render LOD vertex set; PR-Y46+ canary at `yang_integration.rs` Boolean LOD final-emission.

5. **Triangle-index-canonical-key vs Cherchi-canonical-key divergence.** Canary §4.4 mechanism 3 ("triangle has different vert *indices* than Cherchi but same vert *positions*"). If α + γ both refute, the defect may be a canonical-key encoding mismatch between our `quantize_tri` and Cherchi's canonical triangle identity — a deeper PR-Y46 investigation.

### §5.4 Per `feedback_no_last_bug`

PR-Y44 does NOT promise PR-Y45 will close F0020. PR-Y44 sharpens the PR-Y45 anchor from "(α/γ) co-equal inferred" to "(α) primary measured + (γ) bisection control." If PR-Y45 produces another INFRA cycle (the bisection canary refutes both candidates and banks new candidates), that is the disciplined outcome per `feedback_no_last_bug`. The 13-cycle ABORT-or-INFRA rhythm has paid off in anchor sharpness; PR-Y45 may be the first production-fix attempt, or it may be the 9th INFRA SHIP — either is consistent with the discipline.

### §5.5 Per `feedback_phase1_diagnosis_ranking_is_inference`

The α/γ ranking refinement to (C) is **inference-aware**: the m1x=3 mechanism argument is sound but inferential; γ stays in the canary surface to empirically corroborate or refute. Audit-y44 does NOT make the (B) move (α-only) that would treat the inference as measurement.

---

## §6 Strategic-pivot ROI status — POSITIVE remains

| PR | F0020 measurement strength |
|---|---|
| PR-Y41 (pre-pivot) | "Missing 12 upstream" inference refuted; strategic-pivot trigger fired |
| PR-Y42 (pivot) | **50.0% borderline-sharp** attribution; cohort `common=0` method-limit |
| PR-Y43 | **90% accountable** (D + B); Case C = 0 byte-stable; (a) sub-class inferred |
| **PR-Y44 (this PR)** | **(a) sub-class MEASURED at 100% across 8 combined reruns + 2 cohort cases; cohort generalization HOLDS at unpaired-edge subset; (α) m1x=3 mechanism evidence STRONG** |

**Strategic-pivot ROI: POSITIVE remains, advancing.** PR-Y43 elevated MIXED → POSITIVE for F0020. PR-Y44 advances the chain from "(a) plausibly dominant inferred" to "(a) measured 100% with cohort generalization." The trajectory:

- F0020 attribution: 50% (Y42) → 90% (Y43) → **(a) 100% with α-mechanism evidence** (Y44).
- Cohort vertex-level methodology: untested (Y42 §6.2 method-limit at triangle-level) → dense at 50% B/50% D (Y43) → **100% (a) shared with F0020 at unpaired-edge subset** (Y44).
- PR-Y45 anchor sharpness: 4 ranked candidates with citations (Y43) → **(α) primary + (γ) bisection canary, both paper-anchored, m1x=3 mechanism-grounded** (Y44).

The strategic pivot (B.1) has now produced THREE consecutive INFRA cycles (Y42 / Y43 / Y44) that each advance F0020 anchor sharpness without producing a regression and without claiming closure. Per `feedback_external_coherence`: Cherchi C++ remains the load-bearing reference oracle; PR-Y44 reuses the same set-diff data lineage (PR-Y29 → PR-Y31 → PR-Y42 → PR-Y43 → PR-Y44) with no new oracle invocation pattern — just successively sharper reads.

**Per `feedback_no_last_bug`**: 13th cycle on F0020 Render LOD. PR-Y44 does NOT close F0020. PR-Y44 produces the sharpest empirical anchor in the 13-cycle arc. PR-Y45 is the first production-fix attempt window; PR-Y45 may itself be another INFRA cycle if the bisection canary refutes both candidates. The (C) framing in §3.3 protects against premature commitment to a single anchor.

---

## §7 Banked / open (forward-carry)

### §7.1 Banked for PR-Y45 (first production-fix attempt in 13 cycles)

1. **(α) F.0 `remove_winding_insensitive_duplicates` — PRIMARY fix candidate.** 19-tri drop at F.0. Paper anchor Cherchi 2022 §5 (`cherchi2022_interactive_robust_mesh_booleans.txt:340-413`). PR-Y40 scaffold preserved at `tessellation/mod.rs` instrumentation (4 collisions + distributed winners). Bisection question: of the 19 dropped tris, how many position-match the 24 (a) Case D entries? PR-Y45 builds the canary + fix-shape against this anchor.

2. **(γ) Pre-F.0 Boolean LOD → Render LOD re-tessellation — BISECTION/CONTROL canary.** 108-tri drop at `yang_integration.rs:1024`. Paper anchor Yang 2025 §4.4.1 / §4.4.2 (`yang2025_hybrid_boolean.txt:548-590`). Retained in PR-Y45 canary surface as the *control* probe verifying the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer reasoning empirically. Predicted attribution: ~0 of 108 γ-dropped tris position-match the 24 (a) Case D entries (because verts survive γ); if attribution ≫ 0, the reasoning chain is partially refuted and PR-Y45's fix-shape must broaden.

3. **(β) F.3 `remove_nonmanifold_duplicates_aggressive` — TERTIARY, banked for PR-Y46.** 6-tri drop at F.3. Paper anchor Yang 2025 §4.4.1 selective-retention.

4. **Case B secondary anchor — banked for PR-Y46+.** 14 entries with 10 distinct off-vertex positions (audit-y43 §3.1 corrected count). Cohort F0044/F0045 also show 50% Case B. Independent fix-shape from D anchors; both could ship.

### §7.2 Open for PR-Y46+

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44 (only the 42 or 47 bordering unpaired edges classified). δ probe is sub-class-extensible to the wider 194-or-201 set if (α/γ) closes only part of the 42/47.

2. **Cohort triangle-survival ceiling at `common=0`.** PR-Y42 §6.2 method-limit unaffected by PR-Y44. F0044/F0045/R0092 wholesale closure depends on fix-shapes beyond α/γ at the wider triangle-topology divergence.

3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false for F0020 union means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y45 + downstream PRs at best close ~20.

4. **Triangle-index vs position canonical-key divergence.** Canary §4.4 mechanism 3. PR-Y46+ if α+γ+β all refute.

5. **Cherchi non-det 42/47 mode pinning.** Combined 18-rerun evidence (PR-Y43 + Y44 canary + Y44 adversary): 9/18 at 42-mode (~50%); `TBB_NUM_THREADS=1` does NOT pin to one mode. Use missing-count (deterministic) as the load-bearing PR-Y45 gate, NOT extras (mode-sensitive). Sub-class proportion is mode-invariant (100% (a) in both modes).

### §7.3 Methodological banked

1. **Sub-class disambiguation IS the right granularity for catch-all Case D buckets.** δ took +132 LOC and resolved the audit-y43 §3.2 inference into measurement at 100%. Future canaries finding a "catch-all" residual case should default to sub-class disambiguation as Phase 1 measurement before fix selection.

2. **The (C) framing (primary + control) is the disciplined α/γ posture.** Pure co-equal (A) over-spends measurement budget; pure-promotion (B) over-trusts inference. (C) splits the difference: anchor the fix on the mechanism evidence, keep the alternative as empirical control.

3. **Cherchi non-det is now well-characterized over 18 combined reruns.** Sub-class proportion is mode-invariant; load-bearing invariants (sub-class proportion, Case B count, Case C count) all hold in both modes.

4. **The bucket-sum check is a cheap audit invariant.** PR-Y45's bisection canary should adopt the same pattern.

### §7.4 152-OTHER explicit flag (per brief)

Per the brief's audit instruction to "flag the 152 OTHER missing tris": **flagged.** PR-Y44 (and PR-Y43 before it) classified only the 42-or-47 missing tris that border unpaired edges. The remaining 152 (or 154) missing tris are unclassified. PR-Y45's α/γ fix scope does NOT include them by construction. PR-Y46+ may need finer canary extending the δ probe to the wider 194-or-201 missing-tri set; the harness scaffold is sub-class-extensible without further plumbing. **Not a PR-Y45 blocker; the unpaired-edge subset IS PR-Y45's success criterion.**

---

## §8 Final recommendation

**ACCEPT (SHIP-INFRA) — (a)-DOMINANT at 100% byte-verified across 8 combined reruns + 2 cohort cases; PR-Y45 anchor refined to (C) α PRIMARY + γ BISECTION CANARY.**

Rationale:
- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec-y44 / canary-y44 / impl-y44 / adversary-y44). INFRA-CLASS test-author waiver consistent with Y29/Y33/Y36/Y37/Y38/Y40/Y41/Y42/Y43 precedent.
- **DoD §1.5 GREEN** — probe-off byte parity load-bearing; verified independently by canary Gate 2 + adversary Gate B against impl-y44 mirror. PR-Y31 hard gate `pr_y31_f0044_extras_zero` preserved (adversary Gate H).
- **INFRA-CLASS framing intact** — 0 LOC production logic; 0 kernel; 0 wasm-bridge; 0 app; only test-harness extension (+132 LOC at `cherchi_differential_diff.rs`, 1520 → 1652) + memos. No WASM rebuild required.
- **A15.6 compliant** — paper-orthogonal Render LOD diff harness; A15.4/A15.5 unaffected; A15.6 Stage B byte-parity gate preserved.
- **Empirical evidence load-bearing** — sub-class (a) measured at 100% across canary 4 + adversary 6 = 10 F0020 reruns + 2 cohort runs; per-tri spot-checks (d[0]/d[5]/d[15]/d[23]) byte-match canary §4.1; code review confirmed joint-condition predicate enforced.
- **Adversary §5 mechanism reasoning accepted (with measurement-not-promotion discipline)** — m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer (α profile, not γ profile). γ retained as bisection control to corroborate empirically.
- **Cohort generalization REFINE-ACCEPT** — 100% (a) at unpaired-edge subset for F0044/F0045 is statistically meaningful (combined N=10, p ≈ 0.001 at null 50%); caveat carried forward (the 152 OTHER missing tris + cohort triangle-survival `common=0` method-limit) but does NOT block PR-Y45 since PR-Y45's α/γ fix scope IS the unpaired-edge subset.
- **No-last-bug discipline GREEN** — 13 cycles, 0 production-fix LOC on F0020 Render LOD, F0020 Status:Failed unchanged at 40 unpaired. PR-Y44 produces the sharpest anchor of the arc and does NOT promise PR-Y45 will fix F0020. The (C) α-primary-γ-control framing protects against premature commitment.
- **Strategic-pivot ROI POSITIVE advancing** — three consecutive INFRA cycles (Y42/Y43/Y44) each advanced F0020 anchor sharpness without regression. PR-Y45 is the first production-fix attempt window in 13 cycles; PR-Y45 itself may be the 9th INFRA SHIP if both candidates refute — either outcome is consistent with the discipline.
- **PR-Y45 anchor explicit** — (α) F.0 `remove_winding_insensitive_duplicates` PRIMARY fix candidate; (γ) pre-F.0 Boolean LOD → Render LOD re-tess BISECTION/CONTROL canary; (β) F.3 TERTIARY; Case B BANK PR-Y46+.

**PR-Y45 anchor (definitive one-sentence statement for memory file's "PR-Y45 anchor" field, verbatim per §3.4):**

> **PR-Y45 anchor = (α) F.0 `remove_winding_insensitive_duplicates` (Cherchi 2022 §5, `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`; 19-tri drop at `[stage-f] 138→119`; PR-Y40 scaffold preserved) as the PRIMARY fix candidate, with (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024` (Yang 2025 §4.4.1 mesh-updating, `refs/text/yang2025_hybrid_boolean.txt:548-590`; 108-tri drop) retained in the PR-Y45 canary surface as the BISECTION/CONTROL probe to verify the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer reasoning empirically before fix shape is committed.**

**Phase 8 push authorized.** Recommend:
1. Commit this audit memo + adversary memo + canary memo + spec + impl harness extension (`audit(yang-pr-y44): ACCEPT (SHIP-INFRA) — (a)-dominant at 100%; PR-Y45 anchor refined to (C) α primary + γ bisection canary`).
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y44_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, (a) 100% measured, cohort generalization HOLDS at unpaired-edge subset, PR-Y45 (C) α-primary-γ-control anchor (verbatim per §3.4).
4. `TeamDelete pr-y44` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y45 should treat (α) as the load-bearing primary fix anchor and (γ) as the bisection control; the m1x=3 mechanism reasoning is sound but is **measurement when corroborated by γ-attribution-near-zero**, not measurement on its own. The harness scaffold is durable reference infrastructure preserved regardless of PR-Y45's outcome.
