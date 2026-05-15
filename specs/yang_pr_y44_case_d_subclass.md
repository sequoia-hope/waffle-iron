# PR-Y44 — Case D sub-class disambiguation probe (δ) over Cherchi Render LOD diff (INFRASTRUCTURE-CLASS; (a)-DOMINANT at 100%)

| Field | Value |
|---|---|
| **Verdict** | SHIP-INFRA + **(a)-DOMINANT at 100%** (sub-class (a) `(m1x=3, m5x=3)` = 24/24 at 42-mode, 26/26 at 47-mode; both ≫ 80% threshold) |
| **Class** | INFRASTRUCTURE-CLASS (test-harness extension; 0 production code) |
| **Parent commit** | `403932c` (PR-Y43 SHIP-INFRA audit ACCEPT; 2026-05-15) |
| **Date** | 2026-05-15 |
| **Authors** | spec-y44 (this file); canary-y44 (`docs/audits/pr_y44_canary.md`) |
| **LOC** | +132 in `crates/test-harness/tests/cherchi_differential_diff.rs` (1520 → 1652); 0 kernel; 0 wasm-bridge |
| **Production-code delta on F0020** | **0** (unchanged after 13 cycles) |
| **F0020 Status:Failed** | unchanged — 40 unpaired edges (39 boundary, 1 NMM); PR-Y44 changes none |
| **F0020 Case D sub-class histogram** | (a) / (b) / other = **24** / 0 / 0 = **100.0%** / 0.0% / 0.0% (42-mode); **26** / 0 / 0 = **100.0%** / 0.0% / 0.0% (47-mode) |
| **Cohort sub-class** | F0044 D=8/16, **(a)=8/8=100%**; F0045 D=2/4, **(a)=2/2=100%**; R0092 D=0 vacuous |

---

## §1 Motivation

PR-Y44 is the **13th investigational PR on F0020 Render LOD** and the **measurement prerequisite** for PR-Y45's first production-fix attempt of the 13-cycle arc. PR-Y43 (commit `403932c`, 2026-05-15) shipped the A/B/C/D nearest-vertex attribution over the Cherchi C++ Render LOD diff and produced the sharpest empirical anchor of the arc: F0020 = 4/14/0/24 = 9.5/33.3/**0**/57.1%. **Case D = 57.1% (24/42)** became the dominant outcome the PR-Y43 plan did not anticipate (its 6th verdict pattern); **Case C = 0** byte-stable refuted Option C pause for F0020 specifically.

Audit-y43 §3.2 (`docs/audits/pr_y43_validation.md`) identified that the PR-Y43 canary memo's Case D semantics ("ALL three vertex positions appear in Waffle's Render LOD vertex set at 1× grid, yet triangle missing") was **logically inferred from the priority-ordered A→B→C→D classification, NOT directly measured**. The PR-Y43 probe emitted A/B/C/D scalar counts only; it did not emit the within-Case-D `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` 4-tuples. Case D's residual catch-all admits at least two distinct sub-mechanisms:

- **sub-class (a)** `(m1x=3, m5x=3)` — true topology-emission defect. All 3 verts present in Waffle's Render LOD vertex set; the triangle's indexing/winding/edge-pair is what is missing. Aligns with PR-Y44 audit-prescribed candidates (α) F.0 `remove_winding_insensitive_duplicates` + (γ) pre-F.0 Boolean-LOD → Render-LOD re-tessellation. Cited at Cherchi 2022 §5 manifold-flood (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) and Yang 2025 §4.4.1 mesh-updating (`refs/text/yang2025_hybrid_boolean.txt:548-590`).
- **sub-class (b)** `(m1x ∈ {0,1}, m5x=2)` — partial-proximity residual. Vert(s) within 5× cells of a Waffle vert but not at 1×. Fix-shape closer to Case B (vertex-production) than to (α/γ) topology-emission.

Until the (a)/(b) proportion within Case D is measured, the audit-y43 §4.1 PR-Y44 anchor ranking ((α/γ) co-equal) is structural inference, not measurement. Per `feedback_phase1_diagnosis_ranking_is_inference` + `feedback_anchor_before_fix` + `feedback_multi_stage_anchor_probe`: measure the sub-class before scoping a production fix.

PR-Y44 closes that gap. The δ probe is a +132-LOC additive test-file extension to PR-Y43's harness that captures the already-computed 4-tuple per Case-D entry, aggregates the sub-class (a)/(b)/other histogram, and emits a per-tri table. The probe IS the load-bearing measurement; PR-Y45 owns the production fix.

---

## §2 Methodology

### §2.1 Why infrastructure-class

- **0 production LOC.** All changes in `crates/test-harness/tests/cherchi_differential_diff.rs` (test file). No kernel, wasm-bridge, or app code modified. No WASM rebuild required.
- **Default-off byte parity preserved by construction.** The δ block executes only inside the existing `#[test] #[ignore]` test bodies (`f0020_render_lod_nearest_attribution`, `cohort_render_lod_nearest_attribution`) which are env-gated on `CHERCHI2022_BIN`. Probe-off path is byte-identical to PR-Y43.
- **Additive only.** New struct, new field on `NearestAttributionResult`, new accumulator + match-arm append + emission block. No change to the classification predicate (priority-ordered A→B→C→D at `cherchi_differential_diff.rs:1215-1225`), no change to A/B/C/D aggregation scalars, no change to Case B dump (lines 1461–1487 preserved).
- **Reuses PR-Y43 + PR-Y42 infrastructure.** Probe extends PR-Y43's `run_nearest_attribution_for_case`; no new oracle, no new dump site. Underlying set-diff data lineage `PR-Y29 → PR-Y31 → PR-Y42 → PR-Y43 → PR-Y44` is preserved.

### §2.2 Sub-class definition

For each Case D entry, the probe captures the 4-tuple `(match_at_1x, match_at_2x, match_at_5x, match_at_10x)` already computed during `NearestVertAttribution` construction. Sub-class labels are pure functions of the 4-tuple:

| Sub-class | Predicate | Fix-shape implication |
|---|---|---|
| **(a)** | `match_at_1x == 3 ∧ match_at_5x == 3` | topology-emission defect (verts present, triangle indexing/winding/edge-pair missing) → (α/γ) co-equal canary |
| **(b)** | `match_at_1x ∈ {0,1} ∧ match_at_5x == 2` | partial-proximity residual (some vert(s) within 5× cells but not at 1×) → Case-B-like vertex-production mechanism |
| **other** | everything else inside the Case D catch-all | unexpected residual sub-class; PR-Y45 anchor TBD pending sub-class semantics audit |

The two patterns are **mutually exclusive** ((a) requires `m1x=3`; (b) requires `m1x ≤ 1`). They do NOT exhaustively cover Case D; the "other" bucket reports the proportion that does not fit either — this is the audit invariant the bucket-sum check (§5 Gate 5) enforces.

Why these two patterns specifically: per audit-y43 §3.2, Case D = `¬A ∧ ¬B ∧ ¬C` admits `(m1x=3, m5x=3)` (canary's claimed (a)), `(0, 2)` and `(1, 2)` ((b)); sub-classes `(0, 3)` and `(1, 3)` cannot occur within D because they fire as Case A (priority predicate `m5x==3 ∧ m1x<3`). (a) and (b) are the only structurally-distinct mechanisms inside the catch-all; everything else is a noise pattern (e.g., `m1x=2 ∧ m5x=2` which collides with Case B's `m1x==2` predicate at the boundary). The "other" bucket exists to make any noise pattern visible.

### §2.3 4-tuple semantics — why all four grid scales

Per `feedback_multi_stage_anchor_probe` ("don't conclude from a single grid level; sweep 1× / 2× / 5× / 10×"). The 4-tuple is the same data PR-Y43 swept (per audit-y43 §3.1 baseline preservation requirement); δ adds no new grid level:

| Grid | Cell at F0020 (base = `max_abs × TAU_TESS_GRID_FACTOR` = `max_abs × 1e-5`) | Role in sub-class predicate |
|---|---|---|
| **1×** | ~5.42 µm | (a) requires `m1x=3`; (b) requires `m1x ∈ {0,1}` |
| **2×** | ~10.84 µm | informational; not in (a)/(b) predicates (m2x noise observed §6) |
| **5×** | ~27.11 µm | (a) requires `m5x=3`; (b) requires `m5x=2` |
| **10×** | ~54.22 µm | informational; confirms gross positional non-coincidence is absent (24/24 had `m10x=3`) |

The empirical justification for emitting all 4: §6 of the canary memo notes 6 entries at 42-mode have `m2x ∈ {1, 2}` while `m1x = m5x = m10x = 3` — an f32-round-trip artifact at cell boundaries. Single-grid analysis at `m2x` alone would mis-classify these as "non-(a)". Per `feedback_multi_stage_anchor_probe`, the 4-tuple is the right grain regardless.

---

## §3 Probe extension surface (harness extension)

All changes live in `crates/test-harness/tests/cherchi_differential_diff.rs` (extends from 1520 → 1652 lines; **+132 LOC**). Cumulative since PR-Y42: +570 LOC (PR-Y43 +438 + PR-Y44 +132).

### §3.1 New `CaseDSubclassTuple` struct (immediately after `NearestVertAttribution`)

```rust
/// PR-Y44 δ: per-Case-D 4-tuple of grid-match counts at 1×/2×/5×/10×.
/// Separates sub-class (a) `(m1x=3, m5x=3)` ← topology-emission defect
/// from sub-class (b) `(m1x ∈ {0,1}, m5x=2)` ← partial-proximity residual.
/// All other tuples fall under "other" and indicate an unexpected
/// sub-mechanism in the Case D residual catch-all bucket.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct CaseDSubclassTuple {
    match_at_1x: u8,
    match_at_2x: u8,
    match_at_5x: u8,
    match_at_10x: u8,
}
```

### §3.2 Extended `NearestAttributionResult`

```rust
struct NearestAttributionResult {
    case_id: String,
    target_tri_count: usize,
    case_a: usize,
    case_b: usize,
    case_c: usize,
    case_d: usize,
    /// PR-Y44 δ: per-Case-D 4-tuple capture for sub-class disambiguation.
    /// Length == case_d. Same insertion order as classification loop.
    case_d_tuples: Vec<CaseDSubclassTuple>,
}
```

### §3.3 New `case_d_entries` accumulator (paralleling `case_b_dumps`)

```rust
// PR-Y44 δ: per-Case-D 4-tuple capture for sub-class disambiguation.
// Pairs (quantized tri, tuple) so the per-tri table can print the tri id.
let mut case_d_entries: Vec<([(i64, i64, i64); 3], CaseDSubclassTuple)> = Vec::new();
```

### §3.4 Capture-on-D match arm (extends the `_ => case_d += 1;` branch)

```rust
_ => {
    case_d += 1;
    // PR-Y44 δ: capture per-Case-D 4-tuple for sub-class disambiguation.
    case_d_entries.push((
        *tri,
        CaseDSubclassTuple {
            match_at_1x: attr.match_at_1x,
            match_at_2x: attr.match_at_2x,
            match_at_5x: attr.match_at_5x,
            match_at_10x: attr.match_at_10x,
        },
    ));
}
```

### §3.5 Sub-class histogram + per-tri table emission (after Case B dump, before "end" line)

```rust
// PR-Y44 δ: Case D sub-class distribution + per-tri 4-tuple table.
// Sub-class (a) = (m1x=3, m5x=3)         ← topology-emission defect
//                                          (paper anchors: Cherchi 2022 §5
//                                          manifold-flood; Yang 2025 §4.4.1
//                                          mesh-updating dup-retention)
// Sub-class (b) = (m1x ∈ {0,1}, m5x=2)   ← partial-proximity residual
//                                          (vertex-production mechanism;
//                                          Case-B-adjacent)
// Sub-class other = everything else inside the Case D residual catch-all.
let mut subclass_a = 0usize;
let mut subclass_b = 0usize;
let mut subclass_other = 0usize;
for (_tri, tup) in &case_d_entries {
    let is_a = tup.match_at_1x == 3 && tup.match_at_5x == 3;
    let is_b = (tup.match_at_1x == 0 || tup.match_at_1x == 1)
        && tup.match_at_5x == 2;
    if is_a { subclass_a += 1; }
    else if is_b { subclass_b += 1; }
    else { subclass_other += 1; }
}
// ... pct + bucket-sum check + per-tri table ...
```

### §3.6 Determinism + parity preservation

- The δ block executes **only when the probe runs** (Cherchi-binary gated; default-off byte parity unaffected).
- Per-tri sort key is `(qa, qb, qc)` inherited from `missing_sorted.sort()` at line 1319; deterministic within a single Cherchi run.
- Sub-class predicates are pure functions of the 4-tuple; no allocator/iteration-order dependence.
- Bucket-sum check `subclass_a + subclass_b + subclass_other == case_d_entries.len()` is the load-bearing audit invariant (Gate 5).

---

## §4 Contracts

| Contract | Verification |
|---|---|
| Default-off byte parity (probe-off path byte-identical to PR-Y43 HEAD `403932c`) | Gate 2 — F0020 spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` byte-identical pre- and post-probe-add (and post-probe-invoke). `[stage-f] 138→119→119→113→113` byte-identical. |
| PR-Y43 A/B/C/D baselines preserved | `f0020_render_lod_nearest_attribution` produces 4/14/0/24 (42-mode) or 7/14/0/26 (47-mode) byte-identical to PR-Y43 canary §3.1 + audit §2. Case B 14-entry vertex dump byte-identical (spot-checked b[0] cell_dist=12,661, b[1]=1,238, b[3]=12,793, b[9]=815, b[13]=6,884). |
| Cohort tuple-reuse harmless | The new field `case_d_tuples` is inert for non-Case-D entries (Case A/B/C entries never push); cohort F0044/F0045 produce a histogram only over their respective Case D entries (8 and 2). R0092 with target=0 produces a vacuous-all-zero histogram. |
| Classification priority-ordering preserved | The δ block does not touch the `classify_attribution` predicate at `cherchi_differential_diff.rs:1215-1225`. The capture-on-D branch is the existing catch-all `_` arm; the push is order-preserving with respect to the priority A→B→C→D evaluation. |
| Bucket-sum invariant `(a) + (b) + other == case_d` | Asserted at every probe run via emitted check line; failure would indicate a sub-class predicate logic bug (mutually-exclusive). Confirmed OK in all 4 canary reruns. |
| PR-Y31 hard gate preserved | `pr_y31_f0044_extras_zero` continues to pass byte-clean (F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4). |
| Cohort skip-quietly preserved | If `CHERCHI2022_BIN` is unset, harness emits `[nearest-attribution …] SKIP` and returns `None` (mirrors PR-Y29 contract). |

---

## §5 Gates

Eight gates, mirrors canary memo §6:

| Gate | Description | Pass criterion |
|---|---|---|
| **1** | `cargo build -p test-harness --test cherchi_differential_diff` | Clean build; no new warnings beyond 58 pre-existing kernel warnings + 1 slvs warning. New struct `CaseDSubclassTuple` + new `case_d_tuples` field compile clean (uses `#[allow(dead_code)]` per PR-Y43 idiom) |
| **2** | **F0020 default-off byte parity (CRITICAL)** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` byte-identical to PR-Y43 baseline. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical |
| **3** | PR-Y43 A/B/C/D + Case B baselines preserved | `f0020_render_lod_nearest_attribution` 4/14/0/24 (42-mode) or 7/14/0/26 (47-mode) byte-identical to PR-Y43 canary §3.1 / audit §2. Case B 14-entry dump byte-identical |
| **4** | **F0020 Case D sub-class histogram (LOAD-BEARING)** | 4 reruns at `TBB_NUM_THREADS=1`: 2 at 42-mode (subclass_a=24/24=**100%**), 2 at 47-mode (subclass_a=26/26=**100%**). subclass_b=0 across all 4 reruns; subclass_other=0 across all 4 reruns |
| **5** | F0020 Case D per-tri 4-tuple table + bucket-sum invariant | 24 (42-mode) or 26 (47-mode) entries dumped with `(m1x, m2x, m5x, m10x)` tuple + sub-class tag. Bucket-sum `(a) + (b) + other == case_d` passes in all 4 reruns |
| **6** | Cohort sanity F0044 / F0045 / R0092 | F0044: D=8/16, subclass_a=8/8=**100%**. F0045: D=2/4, subclass_a=2/2=**100%**. R0092: target=0, vacuous-all-zero |
| **7a / 7b** | kernel lib + yang_fast regression | `cargo test -p kernel --lib`: **1262 / 24 / 42** — IDENTICAL to PR-Y43 baseline. `YANG_BOOLEAN=1 yang_fast`: **10/157 passed** — IDENTICAL to baseline |
| **8** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4 |

**Gate 2 is the critical INFRA-class contract.** **Gate 4 is the load-bearing measurement gate.** All eight gates GREEN in the canary; reproduction commands at §10.

---

## §6 Outcome — **SHIP-INFRA + (a)-DOMINANT at 100%**

### §6.1 Verdict (resolved measurement)

**(a)-dominance is 100.0% — measured, not inferred.** subclass_a = 24/24 (42-mode) and 26/26 (47-mode); subclass_b = 0%; subclass_other = 0%. The 100% measurement strongly exceeds the 80% threshold for outcome 1 (per the PR-Y44 plan verdict-logic) and refutes outcomes 2 ((b)-dominant), 3 (mixed), 4 (diffuse-other), and 5 (ABORT). audit-y43 §3.2's framing — "(a) plausibly dominant, but inferred from priority-ordered classification" — is now resolved by δ to **"(a) measurably dominant at 100%"**.

### §6.2 PR-Y45 anchor recommendation (verbatim per verdict-logic)

**PR-Y45 anchor = (α/γ) co-equal canary**, per audit-y43 §4.1's contingent-on-δ verdict logic. This is the **first production-fix attempt in the 13-cycle arc**:

- **(α)** F.0 `remove_winding_insensitive_duplicates` (Cherchi 2022 §5; `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) — 19-tri drop at F.0 (`[stage-f] 138→119`). PR-Y40 prior probe found 4 collisions + distributed winners; measurement scaffold preserved. Bisection question: of the 19 dropped tris, how many have all-3-verts at Cherchi-only-missing positions at 1× grid?
- **(γ)** Pre-F.0 Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024` — ~108-tri drop layer (Boolean 246 → Render 138). Paper anchor: Yang 2025 §4.4.1 mesh-updating (`refs/text/yang2025_hybrid_boolean.txt:548-590`) — "the mesh boolean operations may produce a non-manifold mesh ... selectively retaining one of the duplicate triangles." PR-Y41 §6.3 banked-but-unprobed; PR-Y45 closes that gap. Bisection question: of the 108 dropped tris in the Boolean→Render LOD transition, how many have all-3-verts at Cherchi-only-missing positions at 1× grid?

Both candidates are paper-anchored, magnitude-comparable (19 vs 108), **mutually exclusive at the F-stage axis** (α acts within F.0, γ acts pre-F.0). They should be canaried in parallel; the PR-Y45 verdict picks whichever bisects the 24/26 Case D entries with higher signal (or, if both bisect overlapping sets, ships the cheaper fix first).

(β) F.3 `remove_nonmanifold_duplicates_aggressive` (Yang 2025 §4.4.1 selective-retention) remains TERTIARY at the 6-tri F.3 drop scale. (β) is banked for PR-Y46.

### §6.3 Cohort positive surprise — (α/γ) likely to generalize

audit-y43 §6.2 hypothesized "cohort Case B/D semantics differ from F0020's"; δ **refutes** that hypothesis at the sub-class level. F0044 D = 8/16 with **100% sub-class (a)**; F0045 D = 2/4 with **100% sub-class (a)**. The Case D mechanism is **cohort-shared, not F0020-specific**. The PR-Y45 (α/γ) fix is therefore likely to generalize to cohort at the sub-class-mechanism level — but cohort fix-effectiveness at the *triangle-survival* level remains bounded by the PR-Y42 §6.2 `common=0` triangle-level method-limit (every cohort triangle is missing-attributable, only 50% overlap unpaired-edges and are classified). R0092 produces a vacuous-all-zero histogram (target=0).

### §6.4 What the verdict explicitly does NOT promise

- **PR-Y45 will close F0020.** The (α/γ) canary will *measure* whether the sub-class (a) entries trace to F.0 dedup or pre-F.0 re-tessellation; *fixing* the underlying mechanism is a PR-Y46 or later question. Per `feedback_no_last_bug`, the cycle does not declare F0020 closure imminent.
- **Cohort closure.** F0044 / F0045 show 100% sub-class (a) but at the triangle level have `common=0` (PR-Y42 §6.2 method-limit) — meaning cohort closure depends on a different fix-shape than F0020's, even if the within-Case-D mechanism is shared.
- **Cherchi non-det will resolve.** The Case D *total* varies 24 ↔ 26 across modes; the sub-class proportion is invariant at 100%, but the anchor-target-set differs by 2 triangles between modes. PR-Y45 canary should account for both modes.

### §6.5 What the verdict refutes

- **(b) vertex-production shift**: refuted (subclass_b = 0% across 4 reruns).
- **mixed (a/b) split**: refuted (subclass_b = 0%; not within the 30% mixed threshold).
- **diffuse-other**: refuted (subclass_other = 0%).
- **audit-y43 §6.2 cohort-Case-D-may-differ-from-F0020 hypothesis**: refuted (cohort 100% (a), same as F0020).

---

## §7 Rollback

PR-Y44 is INFRA-only with all changes confined to `crates/test-harness/tests/cherchi_differential_diff.rs`. Revert procedure if the δ probe ever regresses default-off behavior or breaks PR-Y43 baselines:

```bash
git checkout f335efc -- crates/test-harness/tests/cherchi_differential_diff.rs
# (f335efc = PR-Y43 SHIP-INFRA HEAD; cherchi_differential_diff.rs at that commit
#  is 1520 lines without the PR-Y44 δ extension)
cargo build -p test-harness --test cherchi_differential_diff
```

`app/tests/cases/assay/results.json` regenerates from `spotlight_f0020` invocations and is not load-bearing on PR-Y44. No kernel, wasm-bridge, or production-path changes to revert. WASM bundle unaffected (no rebuild required for PR-Y44; none required for rollback).

---

## §8 Cherchi non-determinism (PR-Y31 banked, PR-Y43 characterized, PR-Y44 confirmed mode-invariant)

Cherchi C++ has internal TBB non-determinism even at `TBB_NUM_THREADS=1` (banked PR-Y31; PR-Y43 audit §3.3 measured 50/50 across 8 reruns). PR-Y44's 4 reruns produced 50/50 (2 at 42-mode, 2 at 47-mode), bringing combined PR-Y43+PR-Y44 evidence to 8/17 at 42-mode (~47%). Both modes remain stable.

**Sub-class proportion is mode-invariant:**

| Quantity | 42-mode (runs 1,3) | 47-mode (runs 2,4) | Mode-invariant? |
|---|---|---|---|
| `target_tris` | 42 | 47 | NO (Cherchi non-det) |
| Case A | 4 | 7 | NO |
| Case B | **14** | **14** | **YES (BYTE-STABLE)** |
| Case C | **0** | **0** | **YES (BYTE-STABLE)** |
| Case D | 24 | 26 | NO |
| **subclass_a** | **24/24 = 100%** | **26/26 = 100%** | **YES (proportion mode-invariant)** |
| subclass_b | 0/24 = 0% | 0/26 = 0% | YES |
| subclass_other | 0/24 = 0% | 0/26 = 0% | YES |
| bucket-sum check | OK | OK | YES |

**Mitigation:** the load-bearing finding (subclass_a = 100%) is robust to Cherchi TBB non-determinism. PR-Y45 canary should use `missing-count` (the canonical-tri set diff, deterministic in our runs) as the load-bearing gate, NOT `extras` (mode-sensitive). The 47-mode 26-entry set is a strict superset of the 42-mode 24-entry set (canary §4.2 spot-checks `d[9]` and `d[12]` insertion shifts); PR-Y45 canary should account for both target sets.

---

## §9 Banked / open

### §9.1 Banked for PR-Y45 (first production-fix attempt in 13 cycles)

Carried forward from canary memo §8.1; updated to reflect (a)-dominant resolution:

1. **(α) F.0 `remove_winding_insensitive_duplicates` canary** — co-equal PR-Y45 candidate. Bisect F0020's 24 (or 26) sub-class (a) Case D entries against the 19-tri F.0 drop set. PR-Y40 scaffold preserved at `tessellation/mod.rs` instrumentation.
2. **(γ) Pre-F.0 Boolean LOD → Render LOD re-tessellation canary** — co-equal PR-Y45 candidate. Bisect sub-class (a) Case D entries against the 108-tri pre-F.0 drop layer at `yang_integration.rs:1024`. PR-Y41 §6.3 banked-unprobed; PR-Y45 closes the gap.
3. **(β) F.3 `remove_nonmanifold_duplicates_aggressive`** — tertiary; bank for PR-Y46 if (α)+(γ) doesn't bisect cleanly.
4. **Case B secondary anchor** — bank for PR-Y46. 14 entries with 10 distinct off-vertex positions (audit-y43 §3.1 corrected count from canary's "5"). Cohort F0044/F0045 also show 50% Case B; Case B fix-shape may generalize across cohort.

### §9.2 Open for PR-Y46+

Carried forward from canary memo §8.2 + audit-y43 §6.2; updated:

1. **The 6 / 1-2 entries with m2x ≠ 3** (42-mode `d[11], d[13], d[15-17], d[20]` show `m2x=2`; `d[21], d[22]` show `m2x=1`) are an f32-round-trip artifact at cell boundaries; not a fix target. Document if PR-Y45's bisection treats them differently.
2. **The 42 attributable tris vs the OTHER 152 missing tris.** PR-Y43+Y44 only classified the 42 (or 47) that border unpaired edges. The remaining 152 missing tris are **unclassified**. PR-Y46+ may need finer canary if (α)+(γ) closes only part of the 42 and the residual is in the unclassified 152. The δ probe is sub-class-extensible to the larger 194-or-201 missing-tri set if needed.
3. **Cohort triangle-survival ceiling** at `common=0` (PR-Y42 §6.2 method-limit). The cohort vertex-level finding (B/D 50/50, both 100% sub-class (a) at the D layer) is durable methodology; the cohort triangle-level closure ceiling is independent and unaffected by PR-Y44.

### §9.3 Methodological banked

1. **Sub-class disambiguation IS the right granularity for catch-all Case D buckets.** δ took +132 LOC and resolved the audit-y43 §3.2 inference into a measurement at 100% (a)-dominance. Future canaries that find a "catch-all" residual case should default to sub-class disambiguation as the Phase 1 measurement before fix selection. Per `feedback_phase1_diagnosis_ranking_is_inference`.
2. **(a) at 100% is the cleanest possible outcome.** No mixed-shape PR-Y45 needed; α/γ canary is fully orthogonal to vertex-production. The strong-refutation framing for (b)-dominant + mixed + diffuse-other is appropriate.
3. **Cherchi non-det is now well-characterized over 17 combined reruns.** Use `missing-count` (deterministic) as the load-bearing PR-Y45 gate, not `extras` (mode-sensitive). The sub-class proportion is mode-invariant; load-bearing invariants (sub-class proportion, Case B count, Case C count) all hold in both 42-mode and 47-mode.
4. **The bucket-sum check is a cheap audit invariant.** PR-Y45's bisection canary should adopt the same pattern: emit per-bucket counts + a check that they sum to the total.

### §9.4 Citations + feedback memories applied

**Paper citations:**

- **Yang 2025 §4.4.1** mesh updating (`refs/text/yang2025_hybrid_boolean.txt:548-590`): "the mesh boolean operations may produce a non-manifold mesh ... selectively retaining one of the duplicate triangles." Direct anchor for sub-class (a) framing + (γ) Boolean LOD → Render LOD re-tessellation candidate. Yang's "mesh updating" stage is the upstream cause for "verts present, triangle indexing/winding/edge-pair missing" — exactly the sub-class (a) signature.
- **Cherchi 2022 §5** manifold-flood (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`): canonical-form mesh-arrangement output assumes duplicate-removal happens at a specific pass; (α)'s `remove_winding_insensitive_duplicates` is at that layer. F.0 drops "tris (`v0, v1, v2`) where (`v0, v2, v1`) or any rotation also exists." Direct anchor for sub-class (a) + (α) PR-Y45 candidate.

**Feedback memories applied:**

- `feedback_external_coherence` (**load-bearing**): Cherchi C++ remains the reference oracle. PR-Y44 reuses PR-Y43's classification + per-tri 4-tuple — no new oracle, no new dump site.
- `feedback_anchor_before_fix`: δ IS the load-bearing measurement before any production fix attempt. PR-Y44 ships 0 production code; (α/γ) candidates are listed for empirical canary, NOT as fix prescriptions.
- `feedback_phase1_diagnosis_ranking_is_inference`: the verdict is measurement (subclass_a = 100% across 4 reruns; both Cherchi non-det modes), not inference. audit-y43 §3.2's plausible-but-inferred framing is resolved.
- `feedback_multi_stage_anchor_probe`: 4 grid levels emitted per Case D entry (1×/2×/5×/10×); sub-class predicate uses `m1x` + `m5x`. The 4-tuple is preserved as the load-bearing data; m2x noise (§6) does not affect the predicate.
- `feedback_validate_against_corpus`: cohort tested (Gate 6); cohort positive surprise (100% (a) in both F0044 and F0045) honestly reported. audit-y43 §6.2 hypothesis (cohort Case D may differ from F0020) refuted.
- `feedback_no_last_bug`: 13th cycle on F0020 Render LOD. Explicit non-closure language in §6.4. δ produces the sharpest anchor of the 13-cycle arc; does not promise PR-Y45 will fix F0020.
- `feedback_yang_only`: PR-Y44 ships measurement infrastructure; no production logic changed; no fallback paths.
- `feedback_no_regression_chasing`: INFRA-only; no production reverts.
- `feedback_adversary_no_destructive_git`: canary executed worktree-only.
- `feedback_implementer_anti_fabrication_diff`: canary memo §1.2-§1.5 includes verbatim diff/numstat/wc-l artifacts; impl-y44 must mirror.
- `feedback_per_plan_cycle_team`: team `pr-y44` exists for this cycle; TeamDelete at close-out.
- `feedback_always_push`: implementation phase pushes to origin/main (plain push only; never force-push).
- `feedback_oracle_credibility_via_role_separation`: canary-y44 built + ran δ; adversary-y44 will independently re-run from impl-y44 mirror without inheriting canary's reasoning chain.

---

## §10 Verification commands (verbatim, fresh-checkout)

```bash
# Gate 1: build
cargo build -p test-harness --test cherchi_differential_diff

# Gate 2: F0020 default-off byte parity (CRITICAL)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int
# expect: [stage-f] 138→119→119→113→113; unpaired 30→42→39→39→39

# Gate 3: PR-Y43 A/B/C/D + Case B baseline preserved
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect (42-mode, ~50% of runs):
#   Case A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), D=24 (57.1%)
# expect (47-mode, ~50% of runs):
#   Case A=7, B=14, C=0, D=26
# (PR-Y43 baseline; PR-Y44 δ output follows)

# Gate 4 + 5: F0020 Case D sub-class histogram + per-tri 4-tuple table (LOAD-BEARING)
# (same invocation as Gate 3; δ output appended)
# expect new section: "=== F0020 Case D sub-class distribution (24 entries) ==="
# expect (42-mode): subclass_a=24, subclass_b=0, subclass_other=0
# expect (47-mode): subclass_a=26, subclass_b=0, subclass_other=0
# expect per-tri table: 24 (or 26) entries with (m1x, m2x, m5x, m10x) tuple
# expect bucket-sum check: a + b + other == case_d  → OK

# Gate 6: cohort sanity (F0044/F0045/R0092)
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- cohort_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect: F0044 target=16, D=8, subclass_a=8/8=100%
# expect: F0045 target=4,  D=2, subclass_a=2/2=100%
# expect: R0092 target=0 (vacuous all-zero)

# Gate 7a: kernel lib regression
cargo test -p kernel --lib
# expect: 1262 passed; 24 failed; 42 ignored

# Gate 7b: yang_fast regression
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157 passed

# Gate 8: PR-Y31 hard gate
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture
# expect: PASS (F0044 Stage B missing=0, extras=0, common=136)
```
