# PR-Y44 adversary — ACCEPT canary findings; sub-class (a) = 100% byte-verified across 6 F0020 + 2 cohort reruns

**Verdict:** **ACCEPT.** Canary memo's load-bearing claim (Case D sub-class (a) = 100% on F0020 in both Cherchi non-det modes; cohort F0044/F0045 also 100% (a)) byte-reproduced in this independent re-run. 8/8 gates GREEN. No fabrication. The δ probe code is sound (sub-class predicates `(m1x=3, m5x=3)` for (a) and `(m1x ∈ {0,1}, m5x=2)` for (b) are correctly disjoint from priority-ordered A/B/C → catches no spurious classifications). PR-Y45 anchor recommendation (α/γ co-equal) carries forward as authoritative; γ does NOT have a credible promote-to-primary case at this layer — see §5.

---

## §1 Mandate + worktree state

Per the brief: independent re-run of PR-Y44 δ Case-D sub-class probe at HEAD `d14c654` (impl-y44), verify canary memo (`docs/audits/pr_y44_canary.md`) byte-for-byte, look for fabrication / hand-waving / miscount / stale evidence.

**Worktree state:**
- Branch: `worktree-canary-y36`
- HEAD: `b0009bd` (PR-Y42 audit) with d14c654's tree applied as uncommitted modifications to `cherchi_differential_diff.rs` + new untracked memo files. Per `git show d14c654:<path> | diff - <path>` byte-comparison: `cherchi_differential_diff.rs`, `docs/audits/pr_y44_canary.md`, `specs/yang_pr_y44_case_d_subclass.md` all match d14c654 byte-identically (0-line diff).
- Working harness LOC: 1652 (1520 PR-Y43 base + 132 PR-Y44 δ).
- Cherchi sidecar binary: `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` present (827136 bytes).
- No destructive git used; no `git stash` / `git checkout --` / `git reset`. Worktree isolation honored.

---

## §2 8-gate independent results

| Gate | Description | Expected (brief / canary) | Observed (adversary) | Status |
|---|---|---|---|---|
| **A** | `cargo build -p test-harness --test cherchi_differential_diff` | Clean; 58 pre-existing kernel + 1 slvs warnings | Clean; finished in 0.04s; 58 kernel + 1 slvs pre-existing warnings | **GREEN** |
| **B** | F0020 spotlight default-off byte parity | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` | EXACT match: 40 unpaired (39 boundary, 1 non-manifold); 8 of 113 degen; 10 self-int; `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical | **GREEN** |
| **C1** | F0020 δ run 1 | 100% subclass_a regardless of mode | target=**47**; A=7, B=14, C=0, D=26; subclass_a=26/26=100%, subclass_b=0, other=0; bucket-sum OK | **GREEN** (47-mode) |
| **C2** | F0020 δ run 2 | (idem) | target=**47**; A=7, B=14, C=0, D=26; subclass_a=26/26=100% | **GREEN** (47-mode) |
| **C3** | F0020 δ run 3 | (idem) | target=**47**; A=7, B=14, C=0, D=26; subclass_a=26/26=100% | **GREEN** (47-mode) |
| **C4** | F0020 δ run 4 | (idem) | target=**42**; A=4, B=14, C=0, D=24; subclass_a=24/24=100% — canary's 42-mode load-bearing claim reproduced byte-exact | **GREEN** (42-mode) |
| **C5** | F0020 δ run 5 | (idem) | target=**42**; A=4, B=14, C=0, D=24; subclass_a=24/24=100% | **GREEN** (42-mode) |
| **C6** | F0020 δ run 6 | (idem) | target=**47**; A=7, B=14, C=0, D=26; subclass_a=26/26=100% | **GREEN** (47-mode) |
| **C-aggregate** | 6 runs: subclass_a invariant at 100%; mode mix 4× 47 / 2× 42 | canary §4 reported "byte-stable at 100% across 4 reruns at both modes" | Adversary 6 reruns: 4/6 at 47-mode (67%), 2/6 at 42-mode (33%); subclass_a = **100% in every single run**; load-bearing finding (sub-class (a) dominates Case D) is robust to mode mix | **GREEN** |
| **D** | F0020 Case D per-tri table spot-checks (42-mode, Run 4) | Per canary memo §4.1 lines 238 / 243 / 253 / 261 | d[0] qa=(-2.749190e-1,+9.921200e-2,-1.570730e-1), qb=(-2.749190e-1,+9.921200e-2,-1.416830e-1), qc=(-2.487970e-1,+1.037280e-1,-2.076910e-1), (3,3,3,3) (a) ✓; d[5] qa=(-2.477850e-1,-3.667120e-1,+3.216100e-1), qb=(-9.698300e-2,-1.225730e-1,+2.130240e-1), qc=(+7.431900e-2,-4.362940e-1,+3.216100e-1), (3,3,3,3) (a) ✓; d[15] qa=(+1.421790e-1,-1.221610e-1,-8.008300e-2), qb=(+1.421790e-1,-1.221610e-1,+6.998500e-2), qc=(+2.046780e-1,-1.113550e-1,-1.150490e-1), (3,2,3,3) (a) ✓; d[23] qa=(+2.749190e-1,-9.921200e-2,-1.052630e-1), qb=(+2.749190e-1,-9.921200e-2,-1.052630e-1), qc=(+2.749190e-1,-9.921200e-2,+1.367030e-1), (3,3,3,3) (a) ✓ | **GREEN** |
| **E1** | Cohort F0044/F0045/R0092 run 1 | F0044 D=8/16 (50%), 100% (a); F0045 D=2/4 (50%), 100% (a); R0092 vacuous | F0044 D=8/16 (50.0%), subclass_a=8/8=100%; F0045 D=2/4 (50.0%), subclass_a=2/2=100%; R0092 target=0 (vacuous all-zero) — bucket-sum OK on all 3 | **GREEN** |
| **E2** | Cohort run 2 (stability check) | Identical to E1 | IDENTICAL byte-for-byte to E1 | **GREEN** |
| **F** | PR-Y43 A/B/C/D baselines preserved | 47-mode: A=7,B=14,C=0,D=26; 42-mode: A=4,B=14,C=0,D=24; Case B dump 14 entries | All 6 F0020 runs reproduce: 4× 47-mode (7/14/0/26); 2× 42-mode (4/14/0/24); Case B dump 14 entries in all modes. PR-Y43 invariants hold | **GREEN** |
| **G1** | `cargo test -p kernel --lib` | 1262 / 24 / 42 | **1262 passed; 24 failed; 42 ignored** — IDENTICAL | **GREEN** |
| **G2** | `YANG_BOOLEAN=1 yang_fast` | 10/157 | **10/157 passed**, 139 failed, 8 errored (skipped 33 known timeouts) — IDENTICAL | **GREEN** |
| **H** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | GREEN: 136 common / 0 missing / 0 extras | F0044 Subtract: 136 tris / 72 verts / well_formed=true χ=4; missing=0, extras=0, common=136 — IDENTICAL | **GREEN** |

**15/15 gates GREEN** (8 gates per brief + 7 sub-gates within Gate C and Gate E for robustness characterization). Zero RED. Zero fabrication.

---

## §3 Code review of +132 LOC δ extension

Reviewed `crates/test-harness/tests/cherchi_differential_diff.rs:1124-1127, 1252-1254, 1402-1404, 1456-1466, 1522-1631`. Findings:

### §3.1 Sub-class predicate correctness

The δ probe defines:
- **(a)** `is_a = tup.match_at_1x == 3 && tup.match_at_5x == 3` (line 1552, 1600)
- **(b)** `is_b = (tup.match_at_1x == 0 || tup.match_at_1x == 1) && tup.match_at_5x == 2` (line 1553-1554, 1601-1602)
- **other** = residual catch-all

**Cross-verification against priority-ordered A→B→C→D classification at L1229-1239:**
- A fires when `match_at_5x == 3 && match_at_1x < 3` ⇒ `(0,3), (1,3), (2,3)`.
- B fires when `match_at_1x == 2` (regardless of m5x) — catches `(2, *)`.
- C fires when `match_at_5x <= 1` — catches `(0,0), (0,1), (1,0), (1,1), (2,0), (2,1), (3,0), (3,1)` (modulo above).
- D = residual.

Priority-ordering subjected to (a)'s predicate `(3, 3)`: NOT A (m1x=3 not <3) ✓; NOT B (m1x=3 ≠ 2) ✓; NOT C (m5x=3 > 1) ✓. So (a) is correctly captured in Case D.

Priority-ordering subjected to (b)'s predicate `(0|1, 2)`: NOT A (m5x=2 ≠ 3) ✓; NOT B (m1x ∈ {0,1} ≠ 2) ✓; NOT C (m5x=2 > 1) ✓. So (b) is correctly captured in Case D.

**No priority-ordering bug exists.** The sub-class predicates are well-formed; they exhaustively cover the two known Case D sub-mechanisms from audit-y43 §3.2; the "other" bucket correctly catches anything not in (a) or (b).

### §3.2 Joint condition for (a) is load-bearing — verified

Brief flagged: "Sub-class (a) predicate is **exactly** `m1x=3 AND m5x=3` (NOT just `m1x=3` — the canary memo's verdict hinges on the joint condition)."

Verified: L1552 reads `tup.match_at_1x == 3 && tup.match_at_5x == 3`. Both conditions are required. If only `m1x=3` were checked, an entry with `(3, 2)` or `(3, 1)` would be mis-classified as (a). The probe does NOT have this bug.

Empirical confirmation: across all 6 F0020 runs and 2 cohort runs, every reported (a) entry has `m5x=3` in the per-tri dump. No entry shows `(3, 1)`, `(3, 2)`, or `(3, 0)` mis-classified as (a). The joint condition is correctly enforced.

### §3.3 Data structure additivity

`CaseDSubclassTuple` (L1124-1129) is a `Copy + Clone + Debug` struct with 4 `u8` fields. `NearestAttributionResult` (L1245-1255) gains exactly one new field `case_d_tuples: Vec<CaseDSubclassTuple>`. No existing field is removed, renamed, or changed in type. PR-Y43's `case_a`, `case_b`, `case_c`, `case_d`, `target_tri_count`, `case_id` all preserved.

`case_d_entries: Vec<([(i64, i64, i64); 3], CaseDSubclassTuple)>` at L1404 is a new local. The Case D match arm at L1454-1466 pushes to this Vec; the existing `case_d += 1` accounting is preserved (L1455). No existing classification logic touched.

### §3.4 Determinism / sort order

Per-tri table emission at L1597-1614 sorts `case_d_entries` by quantized triangle key (`a.0.cmp(&b.0)` on the 3-tuple of `(i64,i64,i64)` vertex keys). This inherits the canonical key from `missing_sorted.sort()` at L1319. Deterministic. The `d[i]` indexing in the canary's per-tri table matches the adversary's run output (verified Gate D spot-checks).

### §3.5 Bucket-sum check

L1577-1587 emits `a + b + other` and checks against `case_d_entries.len()`. Across all 10 F0020/cohort runs above the check reports `OK`. The check is well-formed (sum is computed from `subclass_a + subclass_b + subclass_other`, the same counters incremented inside the classification loop).

### §3.6 Smells / minor issues (NONE blocking)

- The probe uses `case_d_entries.clone()` at L1597 to create a sorted copy for the per-tri table. Cheap (the list is at most ~26 entries for F0020). Acceptable.
- Comments at L1532-1535 correctly cite `feedback_phase1_diagnosis_ranking_is_inference` as motivation for the probe — appropriate documentation, not load-bearing.
- L1564 `let dn = case_d_entries.len().max(1);` is a divide-by-zero guard for vacuous cohort (e.g., R0092 with Case D=0). Confirmed empirically: R0092 reports `0 / 0 = 0.0%` cleanly, no panic.

**No bugs, no smells, no fabrication risk vectors in the +132 LOC.**

---

## §4 Verdict

**ACCEPT canary findings.** All 15 gates GREEN. Sub-class (a) = 100% byte-reproduced across 6 F0020 reruns at both Cherchi non-det modes (4× 47-mode, 2× 42-mode) and 2 cohort reruns. Per-tri spot-checks (d[0], d[5], d[15], d[23]) byte-match canary memo §4.1. Code review confirms no priority-ordering bug, correct joint-condition enforcement for (a), additive data structures, deterministic sort. PR-Y43 baselines (A/B/C/D histogram, Case B dump) preserved. Kernel lib + yang_fast baselines preserved. PR-Y31 hard gate preserved.

The PR-Y45 anchor recommendation **(α) F.0 `remove_winding_insensitive_duplicates` + (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation, CO-EQUAL** carries forward from canary §7.2 as authoritative.

**Caveats forward-carried (per `feedback_no_last_bug`):**
1. F0020 closure ceiling at ~20 unpaired even if PR-Y45 lands (Cherchi well_formed=false for F0020 union; PR-Y42 §6 caveat preserved).
2. Cherchi TBB non-det persists at `TBB_NUM_THREADS=1` — adversary saw 4/6 at 47-mode (67%) vs canary §3 reported 50/50; **sub-class proportion (100% (a)) is invariant in both modes** so the load-bearing finding is robust, but the *target_tris* set varies by 5 triangles between modes (PR-Y45 canary must account for both).
3. The cohort 100% (a) signal is for the unpaired-edge-bordering subset only (F0044 target=16/136 missing, F0045 target=4 of all missing). It does not speak to whether the rest of the cohort's missing triangles share the same mechanism — they may be Case A/B/C-dominant if classified.

---

## §5 PR-Y45 anchor stress-test: does (α/γ) co-equal hold up?

Brief asked: "should γ be primary instead given 108-tri drop magnitude argument?"

**Stress-test the magnitude argument:**

The "108-tri drop" at pre-F.0 Boolean LOD → Render LOD re-tessellation (`yang_integration.rs:1024`) is from PR-Y41 / canary-y43 §7.4. Argument: this layer drops 246 → 138 = 108 tris, which is 5.7× larger than F.0's 19-tri drop. By raw magnitude, γ should be primary.

**Counter-arguments preserving co-equal:**

1. **The 108-tri drop is a re-tessellation (re-meshing), not a deduplication.** Boolean LOD → Render LOD re-tessellation changes the *grid*, not the *triangulation logic*. The dropped 108 are upstream-of-arrangement; if they were the load-bearing source, F0020's pre-arrangement mesh (`tris_a=290, tris_b=130`) would already show divergence from Cherchi's input. But Cherchi STAGE1 receives our mesh and produces 64 verts / 420 tris (no jolly_creations) — so the re-tessellation produces mesh that arrangement accepts. The 108-tri drop is *legitimate downsampling*, not error.

2. **The (a) signature says vertices are present at 1× grid.** All 24/26 Case D entries have `m1x=3` — all 3 vertex positions are correctly produced and stored in Waffle's Render LOD vertex set. If γ (re-tessellation) were the primary source, we'd expect (b) signature `(m1x ∈ {0,1}, m5x=2)` — partial vertex production, requiring 5× grid to find proximity. Empirically (b) = 0%. So **the vertices survive γ correctly**; what fails is triangle emission downstream.

3. **F.0 dedup (α) is exactly at the triangle-emission layer.** `remove_winding_insensitive_duplicates` drops triangles whose `(v0, v2, v1)` or any rotation exists elsewhere — a topology-emission decision. PR-Y40 found 4 canonical-key collisions + distributed winners at this pass. The (a) signature points at this layer mechanistically.

4. **108-tri ≫ 19-tri is a layer-magnitude argument, but Case D = 24 tri is a defect-magnitude argument.** The defect we're trying to close is ~24 missing tris in F0020 Render LOD. γ's 108-tri drop is ~4.5× the defect size; α's 19-tri drop is ~0.8× the defect size. Both are within an order of magnitude of the defect; neither dominates.

**Adversary recommendation: KEEP (α/γ) CO-EQUAL.** The "γ as primary" framing relies on raw layer-magnitude, but the mechanism evidence (m1x=3 across all 24 entries = vertices present + triangles missing) points at α more than γ. However, γ is unprobed (PR-Y41 banked) so neither anchor has direct measurement of (a)-tri-attribution; the canary memo's "co-equal" stance is the disciplined posture.

**PR-Y45 canary must do the (a)-attribution bisection:** of the 19 α-dropped tris, how many position-match the 24 Case D entries? Of the 108 γ-dropped tris, how many? Both answers needed before promoting either to primary. This is exactly canary §7.2 prescription; adversary endorses.

---

## §6 Cohort generalization stress-test: is 100% (a) for cohort load-bearing or coincidence?

**The finding:** F0044 D=8/16 with 100% (a); F0045 D=2/4 with 100% (a); R0092 vacuous. Both reruns identical.

**Audit-y43 §6.2 hypothesis 1:** "Cohort Case B/D semantics differ from F0020's. F0020 Case D is '3-of-3 at 1×' (canary inference); cohort Case D may be '1-or-2 at 1× + 1-or-2 at 5×' (residual catch-all) because cohort `common=0`."

**Empirical refutation:** Cohort F0044/F0045 100% (a) means cohort Case D is **identical mechanism** to F0020 Case D. Audit-y43's caveat that "cohort may have different sub-class distribution" is empirically refuted at the unpaired-edge-bordering subset.

**Stress-tests against "coincidence" reading:**

1. **Sample size.** F0044 N=8, F0045 N=2. Combined N=10. Probability of 10 independent draws all landing in (a) by chance, if true (a) proportion were 50%, is `0.5^10 ≈ 0.001`. Even at 80% true (a), `0.8^10 ≈ 0.107`. The finding is statistically meaningful, especially combined with F0020's N=24 100% (a) (cumulative N=34, 100% in (a)).

2. **Mechanism-side reading.** F0044's 8 (a) entries all have **m2x=3** (perfect at 2× grid too) — uniformly clean (a). F0020 has 6 entries with m2x ∈ {1, 2} (Case D entries with f32 round-trip near cell boundaries per canary §4.3 banked item #5). F0044's "cleaner (a)" suggests F0044's underlying geometry has fewer near-boundary vertices, not a different mechanism. Both cases produce vertices correctly and miss triangles in the same way.

3. **Methodological caveat.** F0044's `common=0` at triangle level (PR-Y31 / PR-Y42 §6.2 method-limit) means *every* Cherchi triangle is "missing-from-Waffle". The `target_tris=16` is the subset that borders Waffle's unpaired edges. The 8 (a) entries are 8/16 of that subset, NOT 8/136 (entire missing set). So the generalization claim is **valid for the unpaired-edge-bordering subset only**.

4. **R0092 vacuity is consistent.** target=0 means R0092 has no Cherchi-only triangles bordering Waffle's unpaired edges in this slice. R0092's failure mechanism may be elsewhere in the pipeline; this probe is silent on R0092.

**Adversary verdict: cohort generalization HOLDS (not coincidence) at the unpaired-edge-bordering subset, with the methodological caveat that:**
- Validates: same topology-emission mechanism for the unpaired-edge-bordering Case D entries across F0020, F0044, F0045.
- Does NOT validate: that fixing α/γ closes F0044/F0045 wholesale (the `common=0` method-limit per PR-Y42 §6.2 still applies; fix-effectiveness on F0044/F0045 will be bounded by the wider triangle-topology divergence).
- Canary memo §5.2 honestly carries this caveat forward ("cohort fix-effectiveness is bounded by the `common=0` method-limit; vertex-level Case B fix-shape may be the more durable cohort closure"). Adversary endorses the framing.

---

## §7 Open / banked

### §7.1 Forward-carried for PR-Y45

1. **PR-Y45 must do α-vs-γ attribution bisection.** Of the 19 α-dropped tris, how many position-match the 24 Case D entries (a-class)? Of the 108 γ-dropped tris, how many? This decides α vs γ promotion. Canary §7.2 already prescribes; adversary endorses.
2. **Account for both Cherchi non-det modes in PR-Y45 canary.** target_tris varies 42 ↔ 47; PR-Y45 anchor candidate sets should be characterized at both modes.
3. **F0044 cohort fix-effectiveness ceiling.** Even if α/γ closes F0020 Case D (a), F0044's wider `common=0` divergence (136 missing in Subtract, 128 in Union) is not bounded by this PR-Y45. Bank cohort closure as separate goal (PR-Y46+, distinct from F0020 closure).

### §7.2 Banked / minor

1. **Cherchi non-det mode mix.** PR-Y43 §3.3 reported 5/8 reruns at 42-mode (62%). PR-Y44 canary reported 2/4 at 42-mode (50%). Adversary saw 2/6 at 42-mode (33%). Combined PR-Y43 + PR-Y44 canary + adversary: 5/8 + 2/4 + 2/6 = 9/18 = **50% at 42-mode** across 18 reruns. Mode mix is genuinely non-deterministic under `TBB_NUM_THREADS=1`; this confirms PR-Y31's banked Cherchi TBB caveat.
2. **F0044 (a)-entries all have m2x=3.** Different from F0020's mix of m2x ∈ {1, 2, 3}. Could be cohort-specific geometry artifact (simpler topology near unpaired edges); not load-bearing for PR-Y45 anchor decision but worth noting if PR-Y46+ tries cohort generalization.
3. **R0092 vacuity may be informative.** target=0 means R0092 has zero Cherchi-only triangles at unpaired edges in this slice. PR-Y46+ R0092 investigation should probe at a different layer (not the Cherchi-Render-LOD diff).

### §7.3 Methodological observations

1. **`feedback_phase1_diagnosis_ranking_is_inference` empirically vindicated.** Audit-y43 §3.2 flagged "Case D 3-of-3-at-1× sub-class is inferred, not measured." PR-Y44 δ probe measured it; the canary's prior framing was correct but the measurement matters because it (a) rules out sub-class (b) as a competing mechanism (b=0%), (b) confirms cohort generalization (F0044/F0045 100% (a)).
2. **`feedback_multi_stage_anchor_probe` empirically vindicated.** Per-Case-D 4-tuple emission (m1x, m2x, m5x, m10x) was load-bearing: m2x variation among F0020 (a) entries (6 with m2x∈{1,2}) vs F0044 (all m2x=3) is only visible at multi-grid sweep, not single-grid 1× probe.
3. **Adversary-y43's §3.2 audit correction is now empirically discharged.** Audit-y43 carried forward the "Case D sub-class unmeasured" gap as authoritative; PR-Y44 closed it. The audit-corrects-canary-without-forcing-edit policy (Adjudication-2 = policy B) worked: canary memo §5.2 honors audit-y43's caveat by explicitly distinguishing "measured" from "inferred"; PR-Y44 makes "measured" load-bearing.

---

**End of adversary memo. Verdict: ACCEPT.** 15/15 gates GREEN. Sub-class (a) = 100% byte-reproduced. Code review clean. PR-Y45 (α/γ) co-equal anchor framing endorsed. Cohort generalization holds at the unpaired-edge-bordering subset with methodological caveat preserved.
