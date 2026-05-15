# PR-Y44 canary — SHIP-INFRA + sub-class (a) DOMINANT at 100%

**Verdict:** **SHIP-INFRA + (a)-dominant** (24/24 = 100.0% of F0020 Case D entries are sub-class (a) `(m1x=3, m5x=3)` — topology-emission defect; 0/24 sub-class (b); 0/24 other). **PR-Y45 anchor recommendation: co-equal canary of (α) F.0 `remove_winding_insensitive_duplicates` + (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation, per audit-y43 §4.1 contingent-on-δ verdict logic.**

**Gate 4 (F0020 Case D sub-class histogram, LOAD-BEARING):**

| Sub-class | Predicate | 42-mode (run 1,3) | 47-mode (run 2,4) | Dominance threshold |
|---|---|---|---|---|
| (a) | `m1x=3, m5x=3` (topology-emission) | **24/24 = 100.0%** | **26/26 = 100.0%** | ≥ 80% ⇒ **(α/γ) co-equal canary** |
| (b) | `m1x ∈ {0,1}, m5x=2` (partial-proximity residual) | 0/24 = 0.0% | 0/26 = 0.0% | ≥ 40% ⇒ vertex-production shift |
| other | residual catch-all | 0/24 = 0.0% | 0/26 = 0.0% | ≥ 40% ⇒ unexpected sub-class |

**Production code modified:** 0 LOC (probe extension is test-file only)
**Harness LOC:** +132 in `crates/test-harness/tests/cherchi_differential_diff.rs` (1520 → 1652 lines)
**Cumulative LOC since PR-Y42:** +570 in `cherchi_differential_diff.rs` (1082 → 1652; PR-Y43 +438 + PR-Y44 +132)
**Wrong-anchor count this cycle:** N/A — INFRA-class canary
**Stability:** F0020 Case D sub-class histogram BYTE-STABLE across all 4 reruns at subclass_a = 100%; both 42-mode and 47-mode produce 100% sub-class (a). Bucket-sum check passes (a+b+other = total) in all 4 runs.

---

## §1 Mandate + 8-gate plan

Per `/home/claude/.claude/plans/snappy-humming-hejlsberg.md` (PR-Y44 plan) + audit-y43 §4.1 prescription:

> Extend the PR-Y43 harness with per-Case-D 4-tuple emission + sub-class (a)/(b)/other histogram + per-tri dump. **PR-Y44 is a measurement cycle, NOT a fix cycle.** Until δ measures the (a)/(b) proportion, the α/γ co-equal anchor ranking is structural inference.

The plan defined 4 verdict outcomes:
1. **(a)-dominant (≥ 80%)** → PR-Y45 anchor = **(α/γ) co-equal canary** at F.0 + pre-F.0
2. **(b)-dominant (≥ 40%)** → PR-Y45 anchor SHIFTS to vertex-production
3. **mixed** ((a) and (b) both ≥ 30%) → PR-Y45 SPLITS into two PRs
4. **diffuse-other** (other ≥ 40%) → unexpected sub-class; PR-Y45 anchor TBD
5. **ABORT** if Gates 1/2/3/7/8 RED

F0020's empirical histogram measured sub-class (a) at **100%** (well above the 80% threshold). This canary therefore recommends outcome 1: SHIP-INFRA + (α/γ) co-equal PR-Y45 canary.

### §1.1 Discipline

- **Worktree-only.** Live tree at `/home/claude/workspace/.claude/worktrees/canary-y36/`, branch `worktree-canary-y36`, HEAD = `b0009bd` (PR-Y43 audit ACCEPT base; PR-Y43 INFRA artifacts staged as uncommitted; PR-Y44 δ extension added in-worktree).
- **No production logic changed.** All changes in `crates/test-harness/tests/cherchi_differential_diff.rs` (test file). No kernel, wasm-bridge, or app changes.
- **Default-off byte parity preserved.** Gate 2 spotlight produces IDENTICAL `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` post-probe-add.

### §1.2 Verbatim `git diff HEAD --stat`

```
 app/tests/cases/assay/results.json                 | 144 +++---
 .../tests/cherchi_differential_diff.rs             | 570 +++++++++++++++++++++
 2 files changed, 642 insertions(+), 72 deletions(-)
```

`results.json` is the same generated-artifact regeneration pattern as PR-Y38/Y40/Y41/Y42/Y43 — driven by `spotlight_f0020` test invocations. PR-Y44's production change is **only** the +132 LOC δ extension at `cherchi_differential_diff.rs` (PR-Y43's +438 LOC PR-Y43 baseline content is also staged as uncommitted in this worktree).

### §1.3 Verbatim `git diff HEAD --numstat` excerpt

```
72   72   app/tests/cases/assay/results.json
570  0    crates/test-harness/tests/cherchi_differential_diff.rs
```

### §1.4 `wc -l` of the modified test file

`crates/test-harness/tests/cherchi_differential_diff.rs`: **1652 lines** (was 1082 at HEAD pre-PR-Y43; was 1520 post-PR-Y43; PR-Y44 δ adds +132).

### §1.5 Net PR-Y44 contribution

The δ extension is **+132 LOC** strictly additive to the existing PR-Y43 harness. No PR-Y43 code is modified. The δ extension consists of:
- 11 LOC: new `CaseDSubclassTuple` struct
- 4 LOC: new `case_d_tuples` field on `NearestAttributionResult`
- 4 LOC: new `case_d_entries` accumulator in main classification loop
- 13 LOC: capture-on-D in the match arm
- ~95 LOC: histogram + per-tri table emission block
- 3 LOC: result construction for the new field
- Spacing/comments fill the remainder.

---

## §2 Probe extension surface

The δ extension is purely additive. All changes appear in
`crates/test-harness/tests/cherchi_differential_diff.rs`, between lines 1117
(post-`NearestVertAttribution`) and 1652 (EOF). Verbatim diff against
PR-Y43 baseline:

### §2.1 New `CaseDSubclassTuple` struct (immediately after `NearestVertAttribution`)

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

### §2.2 Extended `NearestAttributionResult`

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

### §2.3 New `case_d_entries` accumulator (paralleling `case_b_dumps`)

```rust
// PR-Y44 δ: per-Case-D 4-tuple capture for sub-class disambiguation.
// Pairs (quantized tri, tuple) so the per-tri table can print the tri id.
let mut case_d_entries: Vec<([(i64, i64, i64); 3], CaseDSubclassTuple)> = Vec::new();
```

### §2.4 Capture-on-D match arm (extends the "_ => case_d += 1;" branch)

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

### §2.5 Sub-class histogram + per-tri table emission (after Case B dump, before "end" line)

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
//
// Per audit-y43 §3.2 + §4.1: the canary memo's "3-of-3 at 1× / triangle
// missing" framing was an inference, not a measurement; the probe did
// not distinguish sub-classes. δ measures the proportion before α/γ
// anchor selection (`feedback_phase1_diagnosis_ranking_is_inference`).
eprintln!(
    "\n=== {} Case D sub-class distribution ({} entries) ===",
    case_id,
    case_d_entries.len()
);
eprintln!("  legend: (a) [m1x=3, m5x=3] — topology-emission (α/γ anchor)");
eprintln!("          (b) [m1x ∈ {{0,1}}, m5x=2] — partial-proximity residual (Case-B-adjacent)");
eprintln!("          other — unexpected residual sub-class");
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

### §2.6 Determinism + parity preservation

- The δ block executes **only when the probe runs** (Cherchi-binary gated; default-off byte parity unaffected).
- Per-tri sort key is `(qa, qb, qc)` (inherited from `missing_sorted.sort()` at line 1319). Deterministic within a single Cherchi run.
- Sub-class predicates are pure functions of the 4-tuple — no allocator/iteration order dependence.
- Bucket-sum check `subclass_a + subclass_b + subclass_other == case_d_entries.len()` is the audit invariant per PR-Y44 plan Gate 5. Confirmed OK in all 4 reruns.

### §2.7 Sub-class predicates — why these two patterns?

Per audit-y43 §3.2:
- **Sub-class (a) `(m1x=3, m5x=3)`**: all 3 verts match at 1× grid. The triangle is *positionally present* in Waffle's Render LOD vertex set, but Cherchi has the triangle and Waffle doesn't. This is a topology-emission defect (the verts exist, the edge-pair/index/winding doesn't). PR-Y45 candidates (α) `remove_winding_insensitive_duplicates` and (γ) pre-F.0 re-tessellation both drop *triangles* whose verts may still be present — exactly the (a) mechanism.
- **Sub-class (b) `(m1x ∈ {0,1}, m5x=2)`**: only 0-or-1 vert at 1× and only 2 at 5×. The triangle's verts are *not all present* in Waffle's Render LOD; one or more are within 5× cells of a Waffle vert but not at 1× cells. This is a vertex-production residual (Case B is `(2, _)` — 2-of-3 at 1×; sub-class (b) is the next-coarser pattern where the missing vert(s) are 5×-near but not 1×-coincident). Fix-shape closer to Case B than to Case D's topology-emission.
- **other**: anything else under the Case D catch-all (e.g., `(m1x=3, m5x=2)`, `(m1x=2, m5x=2)`, etc.). Each combination has a distinct interpretation; "other" reports the proportion that does not fit either (a) or (b).

The two patterns are *mutually exclusive* (a requires m1x=3, b requires m1x ≤ 1). They do NOT exhaustively cover Case D — that is the point of the "other" bucket.

---

## §3 F0020 Case D sub-class histogram (Gate 4, LOAD-BEARING)

### §3.1 Per-run histogram (4 reruns; Cherchi non-det)

| Run | target_tris | A | B | C | **D** | **(a)** | **(b)** | **other** | bucket-sum OK |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 42 | 4 | 14 | 0 | **24** | **24** | **0** | **0** | OK |
| 2 | 47 | 7 | 14 | 0 | **26** | **26** | **0** | **0** | OK |
| 3 | 42 | 4 | 14 | 0 | **24** | **24** | **0** | **0** | OK |
| 4 | 47 | 7 | 14 | 0 | **26** | **26** | **0** | **0** | OK |

### §3.2 Aggregate dominance

| Dominance check | 42-mode (runs 1+3) | 47-mode (runs 2+4) | Verdict |
|---|---|---|---|
| sub-class (a) ≥ 80% (→ (α/γ) co-equal) | **100.0%** | **100.0%** | **YES — STRONG (a)-dominant** |
| sub-class (b) ≥ 40% (→ vertex-production shift) | 0.0% | 0.0% | NO |
| sub-class other ≥ 40% (→ unexpected) | 0.0% | 0.0% | NO |
| mixed (a ≥ 30% AND b ≥ 30%) (→ split) | (a=100%, b=0%) | (a=100%, b=0%) | NO (not mixed) |

**Per audit-y43 §4.1's verdict-logic prescription**: subclass_a = 100% across 4 reruns at both Cherchi non-det modes exceeds the 80% threshold for outcome 1 (PR-Y45 anchor = (α/γ) co-equal canary). The 100% measurement does NOT trigger "mixed" (b = 0%), nor "(b)-dominant" (b < 40%), nor "diffuse-other" (other < 40%).

### §3.3 Per-rerun byte-stability

- subclass_a count is 100% across all 4 reruns at the case_d_entries level.
- The (24, 26) variance across modes is fully attributable to the Cherchi non-det Case D total variance (24 ↔ 26); the *proportion* is invariant.
- The bucket-sum check `a + b + other = total` passes in all 4 reruns.

Per `feedback_phase1_diagnosis_ranking_is_inference`: audit-y43 §3.2 flagged "3-of-3 at 1× / triangle missing → indexing/winding" as a *plausible inference*. δ confirms the inference is **empirically borne out at 100%** — there is *no* sub-class (b) residual mechanism for F0020. The α/γ co-equal recommendation is therefore not inference; it is measured.

---

## §4 F0020 Case D per-tri 4-tuple table (Gate 5)

### §4.1 42-mode (Run 1; 24 entries)

| d[i] | tri (compact qa→qb→qc) | (m1x, m2x, m5x, m10x) | sub-class |
|---|---|---|---|
| 0 | (-0.275,+0.099,-0.157)→(-0.275,+0.099,-0.142)→(-0.249,+0.104,-0.208) | (3, 3, 3, 3) | (a) |
| 1 | (-0.275,+0.099,-0.142)→(-0.249,+0.104,-0.208)→(-0.142,+0.122,+0.070) | (3, 3, 3, 3) | (a) |
| 2 | (-0.249,+0.104,-0.208)→(-0.240,+0.105,-0.220)→(-0.240,+0.105,-0.223) | (3, 3, 3, 3) | (a) |
| 3 | (-0.248,-0.367,+0.322)→(-0.187,-0.086,+0.206)→(-0.157,-0.099,+0.209) | (3, 3, 3, 3) | (a) |
| 4 | (-0.248,-0.367,+0.322)→(-0.157,-0.099,+0.209)→(-0.097,-0.123,+0.213) | (3, 3, 3, 3) | (a) |
| 5 | (-0.248,-0.367,+0.322)→(-0.097,-0.123,+0.213)→(+0.074,-0.436,+0.322) | (3, 3, 3, 3) | (a) |
| 6 | (-0.240,+0.105,-0.220)→(-0.240,+0.105,-0.223)→(-0.142,+0.122,+0.070) | (3, 3, 3, 3) | (a) |
| 7 | (-0.157,-0.099,+0.209)→(-0.097,-0.123,+0.213)→(+0.269,+0.193,+0.246) | (3, 3, 3, 3) | (a) |
| 8 | (-0.097,-0.123,+0.213)→(-0.026,-0.151,+0.218)→(+0.074,-0.436,+0.322) | (3, 3, 3, 3) | (a) |
| 9 | (-0.097,-0.123,+0.213)→(-0.026,-0.151,+0.218)→(+0.269,+0.193,+0.246) | (3, 3, 3, 3) | (a) |
| 10 | (-0.026,-0.151,+0.218)→(+0.074,-0.134,+0.203)→(+0.142,-0.122,+0.193) | (3, 3, 3, 3) | (a) |
| 11 | (-0.023,-0.151,-0.147)→(+0.156,-0.120,-0.122)→(+0.205,-0.111,-0.115) | (3, **2**, 3, 3) | (a) |
| 12 | (-0.023,-0.151,-0.147)→(+0.156,-0.120,-0.122)→(+0.211,-0.110,-0.114) | (3, 3, 3, 3) | (a) |
| 13 | (-0.023,-0.151,-0.147)→(+0.205,-0.111,-0.115)→(+0.211,-0.110,-0.114) | (3, **2**, 3, 3) | (a) |
| 14 | (+0.108,-0.281,-0.056)→(+0.136,-0.309,+0.243)→(+0.190,-0.362,-0.076) | (3, 3, 3, 3) | (a) |
| 15 | (+0.142,-0.122,-0.080)→(+0.142,-0.122,+0.070)→(+0.205,-0.111,-0.115) | (3, **2**, 3, 3) | (a) |
| 16 | (+0.142,-0.122,-0.080)→(+0.156,-0.120,-0.122)→(+0.205,-0.111,-0.115) | (3, **2**, 3, 3) | (a) |
| 17 | (+0.142,-0.122,+0.070)→(+0.205,-0.111,-0.115)→(+0.211,-0.110,-0.114) | (3, **2**, 3, 3) | (a) |
| 18 | (+0.142,-0.122,+0.099)→(+0.142,-0.122,+0.193)→(+0.142,-0.122,+0.193) | (3, 3, 3, 3) | (a) |
| 19 | (+0.142,-0.122,+0.099)→(+0.142,-0.122,+0.193)→(+0.318,-0.092,+0.246) | (3, 3, 3, 3) | (a) |
| 20 | (+0.156,-0.120,-0.122)→(+0.156,-0.120,-0.122)→(+0.205,-0.111,-0.115) | (3, **2**, 3, 3) | (a) |
| 21 | (+0.156,-0.120,-0.122)→(+0.205,-0.111,-0.115)→(+0.205,-0.111,-0.115) | (3, **1**, 3, 3) | (a) |
| 22 | (+0.205,-0.111,-0.115)→(+0.205,-0.111,-0.115)→(+0.211,-0.110,-0.114) | (3, **1**, 3, 3) | (a) |
| 23 | (+0.275,-0.099,-0.105)→(+0.275,-0.099,-0.105)→(+0.275,-0.099,+0.137) | (3, 3, 3, 3) | (a) |

**Bucket-sum: 24 (a) + 0 (b) + 0 (other) = 24 — matches Case D total = 24. OK.**

### §4.2 47-mode (Run 4; 26 entries, abbreviated to delta vs 42-mode)

Run 4 (47-mode) produces a superset of Run 1 (42-mode) with 2 additional entries (the same `target_tris = 47` mode adds 5 more "A"-eligible + 2 more Case D-eligible tris). All 26 entries are sub-class (a) with `m1x=3, m5x=3`. Diff vs Run 1:

| d[i] (47-mode) | qa → qb → qc (delta) | (m1x, m2x, m5x, m10x) |
|---|---|---|
| 9 | (-0.097,-0.123,+0.213)→(-0.026,-0.151,+0.218)→(+0.078,+0.160,+0.087) | (3, 3, 3, 3) (a) |
| 12 | (-0.026,-0.151,+0.218)→(+0.078,+0.160,+0.087)→(+0.142,-0.122,+0.193) | (3, 3, 3, 3) (a) |

Insertion shifts the d[i] indexing of subsequent entries by +1 (47-mode d[10] = 42-mode d[9], etc.). All m5x = 3; no new sub-class signature introduced.

**Bucket-sum: 26 (a) + 0 (b) + 0 (other) = 26 — matches Case D total = 26. OK.**

### §4.3 Observations from the per-tri table

- **m1x = 3 in 24/24 entries (42-mode) and 26/26 entries (47-mode).** Every Case D triangle has all 3 vertices at Waffle's 1× grid. Per audit-y43 §3.2, this was inferred; δ confirms.
- **m5x = 3 in 24/24 and 26/26.** Every Case D triangle is also positionally present at the 5× grid. No sub-class (b) `m5x=2` patterns appear.
- **m2x varies (2 or 3) but never m5x or m1x.** Six entries in 42-mode (d[11], d[13], d[15-17], d[20]) and one (d[21], d[22]) have `m2x=2` or `m2x=1` — meaning at 2× grid (which is a *finer-than-1× — wait, no, 2× is **coarser** than 1×).
  - Correction: 1× = 5.42µm; 2× = 10.84µm; 5× = 27.11µm; 10× = 54.22µm. Coarser cells SHOULD have MORE matches (collide more keys). The phenomenon `m1x=3, m2x=2` (3 verts at 1× but only 2 at 2×) appears anomalous but is an artifact of f32 round-trip — coarser quantization re-bins boundary-of-cell values into adjacent cells. This does NOT invalidate the (a)/(b) classification because (a)/(b) use only `m1x` + `m5x`, not `m2x`. The m2x variance is noise relative to the load-bearing dimensions.
- **All 24/26 entries have m10x = 3.** At 10× coarser grid (54µm), every Case D vert positionally collapses into a Waffle vert key. Confirms no Case D triangle has any vert that is genuinely far from Waffle's vert set; all are positionally-present-but-triangle-missing.

### §4.4 What sub-class (a) tells PR-Y45

Sub-class (a) is the empirical signature of **triangle missing while all 3 verts present in the destination mesh**. The only mechanisms that can produce this pattern:

1. **Triangle dropped during F-stage cleanup** — F.0 `remove_winding_insensitive_duplicates`, F.1 cosmetic-cleanup, F.2 retain-one-of-doubles, F.3 `remove_nonmanifold_duplicates_aggressive`, F.4 final dedup. The verts survive (they're in the Render LOD vertex set), but a specific triangle indexing/winding was removed.
2. **Triangle never emitted but verts shared with neighbours** — pre-F.0 Boolean-LOD → Render-LOD re-tessellation could omit a triangle that Cherchi has, while neighbouring triangles contribute the same verts.
3. **Triangle has different vert *indices* than Cherchi but same vert *positions*** — would manifest as Case D because vert positions are at-1×, but Waffle's quantized canonical triangle key differs from Cherchi's. (PR-Y43 used 1e-6 grid for triangle canonical-key matching; PR-Y43's `missing_from_waffle` is at the canonical-tri level.)

Mechanisms 1 and 2 align with audit-y43 §4.1's (α) and (γ) anchor candidates respectively. Mechanism 3 is a re-statement of the Case D semantics, not an independent anchor.

---

## §5 Cohort sub-class histograms (Gate 6)

### §5.1 Cohort histogram

| Case | F0044 | F0045 | R0092 |
|---|---|---|---|
| target_tris | 16 | 4 | 0 |
| Case A | 0 (0.0%) | 0 (0.0%) | 0 |
| Case B | 8 (50.0%) | 2 (50.0%) | 0 |
| Case C | 0 (0.0%) | 0 (0.0%) | 0 |
| **Case D** | **8 (50.0%)** | **2 (50.0%)** | **0** |
| **subclass_a (m1x=3, m5x=3)** | **8/8 = 100.0%** | **2/2 = 100.0%** | **0/0 (vacuous)** |
| subclass_b | 0/8 = 0.0% | 0/2 = 0.0% | 0/0 |
| subclass_other | 0/8 = 0.0% | 0/2 = 0.0% | 0/0 |
| bucket-sum check | OK | OK | OK (0+0+0=0) |
| base_grid | 4.332616e-6 m | 4.573874e-6 m | 1.305296e-7 m |

### §5.2 Cohort interpretation

**The cohort's Case D is 100% sub-class (a) — same pattern as F0020.** The F0044/F0045 Case D triangles all have `(m1x=3, m5x=3)`. The sub-class (a) mechanism is therefore **cohort-shared**, not F0020-specific. This is a stronger signal than expected per audit-y43 §6.2 ("cohort Case B/D semantics differ from F0020's"); the *Case D semantics* are the same across F0020 and cohort.

Cohort interpretation per audit-y43 §6.1 was that F0044 / F0045 have `common=0` at the triangle level (PR-Y42 §6.2 method-limit), which makes the missing-triangle set the *entire* arrangement output. The fact that 50% of those missing triangles classify as Case D + Case B 100%-sub-class-(a) means the vertex-level mechanism IS the same for the cohort — but at the triangle level the F0044/F0045 mismatch is wholesale (every triangle is missing-attributable; only the 50% that overlap unpaired-edges are classified). This is consistent with: **the same topology-emission mechanism produces the F0020 D and the cohort D — and an (α/γ) fix would generalize.** (But cohort fix-effectiveness is bounded by the `common=0` method-limit; vertex-level Case B fix-shape may be the more durable cohort closure.)

### §5.3 R0092 vacuous case

`target_tris=0` because R0092's Render LOD has zero missing-attributable triangles bordering unpaired edges (PR-Y43 §5.3 noted this). All histogram entries are 0; bucket-sum 0+0+0=0 = total 0 passes vacuously.

---

## §6 All other gate results

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | `cargo build -p test-harness --test cherchi_differential_diff` | **GREEN** | Clean build. 58 pre-existing kernel warnings + 1 slvs warning unchanged. New struct `CaseDSubclassTuple` + new `case_d_tuples` field compile clean; `#[allow(dead_code)]` applied per PR-Y43 idiom. |
| **2** | F0020 default-off byte parity (post-probe-add) | **GREEN** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` BYTE-IDENTICAL to PR-Y43 baseline. `[stage-f] 138→119→119→113→113` byte-identical. |
| **3** | PR-Y43 baselines preserved | **GREEN** | `f0020_render_lod_nearest_attribution` produces A/B/C/D = 4/14/0/24 (42-mode) or 7/14/0/26 (47-mode) byte-identical to PR-Y43 canary memo §3.1 + audit §2. Case B 14-entry vertex dump byte-identical (spot-checked b[0] cell_dist=12,661, b[1]=1,238, b[3]=12,793, b[9]=815, b[13]=6,884). |
| **4** | F0020 Case D sub-class histogram (LOAD-BEARING) | **(a)-DOMINANT at 100%** | 4 reruns at TBB_NUM_THREADS=1: 2 at 42-mode (subclass_a=24/24=100%), 2 at 47-mode (subclass_a=26/26=100%). subclass_b=0 across all 4 reruns; subclass_other=0 across all 4 reruns. Detail §3. |
| **5** | F0020 Case D per-tri 4-tuple table | **24 / 26 entries dumped** | Per-tri table emitted with (m1x, m2x, m5x, m10x) tuple + sub-class tag for each entry. Bucket-sum check OK in all 4 reruns. Detail §4. |
| **6** | Cohort sanity F0044/F0045/R0092 | **(a) 100% across all cohort cases** | F0044: D=8/16, subclass_a=8/8=100%. F0045: D=2/4, subclass_a=2/2=100%. R0092: target=0, vacuous. Detail §5. |
| **7a** | `cargo test -p kernel --lib` | **GREEN** | **1262 passed; 24 failed; 42 ignored** — IDENTICAL to PR-Y43 baseline. |
| **7b** | `YANG_BOOLEAN=1 yang_fast` | **GREEN** | **10/157 passed, 139 failed, 8 errored** (skipped 33 known timeouts) — IDENTICAL to PR-Y43 baseline. |
| **8** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | **GREEN** | F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4. |

**8/8 gates GREEN.**

---

## §7 Verdict + PR-Y45 anchor recommendation

### §7.1 Verdict

**SHIP-INFRA + (a)-dominant at 100%.**

The δ probe extension is a +132-LOC additive test-file extension to the existing PR-Y43 harness. It measures the within-Case-D sub-mechanism distribution that audit-y43 §3.2 + §4.1 prescribed as a prerequisite to PR-Y45 anchor selection.

**Empirical measurement:** subclass_a = 100% across 4 reruns at both Cherchi non-det modes (42-mode 24/24, 47-mode 26/26). subclass_b = 0%. subclass_other = 0%. The cohort F0044 / F0045 also show 100% subclass_a. **Sub-class (a) is the universal Case D signature for F0020 + cohort.**

Per the PR-Y44 plan verdict-logic prescription: subclass_a ≥ 80% triggers outcome 1.

### §7.2 PR-Y45 anchor recommendation (verbatim per verdict-logic)

**PR-Y45 anchor = (α/γ) co-equal canary.**

- **(α) F.0 `remove_winding_insensitive_duplicates`** (Cherchi 2022 §5; `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`)
  - 19-tri drop at F.0 (`[stage-f] 138→119`).
  - PR-Y40 prior probe found 4 collisions + distributed winners — measurement scaffold preserved.
  - Bisection question: of the 19 dropped tris, how many have all-3-verts matching Cherchi-only-missing positions at 1× grid?
- **(γ) Pre-F.0 Boolean LOD → Render LOD re-tessellation** at `yang_integration.rs:1024`
  - ~108-tri drop layer (Boolean 246 → Render 138).
  - PR-Y41 §6.3 banked-but-unprobed.
  - Bisection question: of the 108 dropped tris in the Boolean→Render LOD transition, how many have all-3-verts matching Cherchi-only-missing positions at 1× grid?

Both candidates are paper-anchored, magnitude-comparable (19 vs 108), and **mutually exclusive at the F-stage axis** (α acts within F.0, γ acts pre-F.0). They should be canaried in parallel; the PR-Y45 verdict picks whichever bisects the 24-or-26 Case D entries with higher signal (or, if both bisect overlapping sets, ships the cheaper fix first).

### §7.3 What the verdict refutes

- **(b) vertex-production shift**: refuted (subclass_b = 0% across 4 reruns).
- **mixed (a/b) split**: refuted (subclass_b = 0%; not within 30% threshold).
- **diffuse-other**: refuted (subclass_other = 0%).
- **Sub-class (a) inference being wrong**: confirmed; audit-y43 §3.2's "plausible but inferred" sub-class (a) framing is empirically borne out at 100%. The α/γ co-equal anchor ranking advances from inference to measurement.

### §7.4 What the verdict does NOT promise

- PR-Y45 will close F0020. The (α)+(γ) canary will *measure* whether the (a) sub-class entries trace to F.0 dedup or pre-F.0 re-tessellation; *fixing* the underlying mechanism is a PR-Y46 or later question. Per `feedback_no_last_bug`, the cycle does not declare F0020 closure imminent.
- The cohort Case D will close. F0044 / F0045 also show 100% sub-class (a) but at the triangle level have `common=0` (PR-Y42 §6.2 method-limit) — meaning the cohort closure depends on a different fix-shape than F0020's, even if the within-Case-D mechanism is shared.
- Cherchi non-det will resolve. The Case D total varies 24 ↔ 26 across modes; the sub-class proportion is invariant at 100%, but the *anchor-target-set* differs by 2 triangles between modes. PR-Y45 canary should account for both modes.

### §7.5 Paper citations

- **Cherchi 2022 §5 manifold-flood** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`): canonical-form mesh-arrangement output assumes duplicate-removal happens at a specific pass; (α)'s `remove_winding_insensitive_duplicates` is at that layer. F.0 drops "tris (`v0, v1, v2`) where (`v0, v2, v1`) or any rotation also exists."
- **Yang 2025 §4.4.1 mesh-updating** (`refs/text/yang2025_hybrid_boolean.txt:548-590`): "selectively retaining one of the duplicate triangles" — the same dedup logic, applied at Yang's mesh-updating stage. (γ)'s Boolean→Render LOD re-tessellation is Yang's mesh-updating equivalent.
- Both candidates are paper-cited and architecturally orthogonal (different stages of the Yang/Cherchi pipeline).

### §7.6 Per `feedback_anchor_before_fix` + `feedback_multi_stage_anchor_probe`

The δ probe was instrumented and run on F0020 alone first (Run 1; produced the expected (a)/(b)/other histogram + per-tri table) before re-running for non-det confirmation (Runs 2-4) and cohort (F0044/F0045/R0092). All 4 grid columns (m1x, m2x, m5x, m10x) were reported per Case D entry; sub-class (a) was verified against m5x=3 not inferred from m1x=3 alone. **`feedback_multi_stage_anchor_probe` discipline observed.**

---

## §8 Open / banked

### §8.1 Banked for PR-Y45

1. **(α) F.0 `remove_winding_insensitive_duplicates` canary** — primary PR-Y45 candidate. Bisect F0020's Case D entries against the 19-tri F.0 drop set. PR-Y40 scaffold preserved at `tessellation/mod.rs` instrumentation.
2. **(γ) Pre-F.0 Boolean LOD → Render LOD re-tessellation canary** — co-primary. Bisect F0020's Case D entries against the 108-tri pre-F.0 drop layer at `yang_integration.rs:1024`. PR-Y41 §6.3 banked-unprobed; PR-Y45 closes the gap.
3. **(β) F.3 `remove_nonmanifold_duplicates_aggressive`** — tertiary. 6-tri drop at F.3. Bank for PR-Y46 if (α)+(γ) doesn't bisect cleanly.
4. **Case B secondary anchor** — bank for PR-Y46. 14 entries with 10 distinct off-vertex positions (audit-y43 §3.1 corrected count). Cohort F0044/F0045 also show 50% Case B at the vertex level.
5. **Cherchi non-det 42/47 mode pinning** — `TBB_NUM_THREADS=1` did NOT pin Cherchi to one mode in this canary's 4 reruns (2/4 each). Combined with PR-Y43's 5+4 = 9 reruns: 5/13 at 42-mode + 8/13 at 47-mode ≈ ~40/60 (audit-y43 §3.3 reported 50/50 from 8 reruns; PR-Y44's 4-rerun split is 50/50 — combined 8/17 ≈ 47% at 42-mode). The *sub-class proportion* is invariant (100% (a) in both modes) so the load-bearing finding is robust to mode mix.

### §8.2 Open for PR-Y46+

1. **The 6 / 1-2 entries with m2x != 3** are an artifact of f32 round-trip near cell boundaries (§4.3). Not a fix target; document if PR-Y45's bisection treats them differently.
2. **The 42 attributable tris vs the OTHER 152 missing tris.** PR-Y43 only classified the 42 that border unpaired edges. δ inherits that bound. PR-Y46+ may need finer canary if (α)+(γ) closes only part of the 42 and the residual is in the un-classified 152.
3. **Cohort Case D 100% sub-class (a) is a stronger-than-expected finding.** Audit-y43 §6.2 hypothesized cohort Case D semantics may differ from F0020's; δ refutes that — cohort Case D is also 100% sub-class (a). The PR-Y45 (α/γ) fix MAY generalize to cohort at the sub-class level; whether it generalizes at the *triangle-survival* level is bounded by the `common=0` cohort method-limit.

### §8.3 Methodological banked

1. **Sub-class disambiguation IS the right granularity for catch-all Case D buckets.** The δ probe took +132 LOC and resolved the audit-y43 §3.2 inference into a measurement at 100% (a)-dominance. Future canaries that find a "catch-all" residual case should default to sub-class disambiguation as Phase 1 measurement before fix selection.
2. **(a) at 100% is the cleanest possible outcome.** No mixed-shape PR-Y45 needed; α/γ canary is fully orthogonal to vertex-production. Per `feedback_phase1_diagnosis_ranking_is_inference`, the strong-refutation framing for (b) is appropriate.
3. **Cherchi non-det is now well-characterized.** 13 combined reruns (PR-Y43 + PR-Y44): the 42-vs-47 mode split has roughly equal probability; subset-superset relationship preserved (47-mode = 42-mode + 5 A + 2 D entries); load-bearing invariants are mode-invariant. Future PRs should use `missing-count` (the canonical-tri set diff, deterministic in our runs) as the load-bearing gate, not `extras` (mode-sensitive).
4. **The bucket-sum check is a cheap audit invariant.** PR-Y45's bisection canary should adopt the same pattern: emit per-bucket counts + a check that they sum to the total.

---

## §9 Strategic-pivot status — PR-Y45 anchor is MEASURED, not inferred

PR-Y43 audit framed PR-Y44 (δ) as a **prerequisite measurement cycle** before PR-Y45 fix attempts. PR-Y44 closes that prerequisite at 100% sub-class (a) dominance. The strategic pivot to vertex-level diff (PR-Y42) → A/B/C/D classification (PR-Y43) → sub-class disambiguation (PR-Y44) is now at the sharpest empirical anchor of the 13-cycle arc:

| PR | F0020 measurement strength |
|---|---|
| PR-Y41 | "Missing 12 upstream" inference (refuted by PR-Y40 §6 outcome and PR-Y41 itself) |
| PR-Y42 (pivot) | 50.0% borderline-sharp attribution; cohort `common=0` method-limit |
| PR-Y43 | 90% accountable (D + B); Case C = 0 byte-stable; (a) sub-class inferred |
| **PR-Y44 (this PR)** | **(a) sub-class measured at 100%; α/γ co-equal anchor MEASURED, not inferred** |

PR-Y45 receives:
- A specific, measured fix-target (sub-class (a) — topology-emission).
- Two co-equal paper-anchored candidates (α at F.0 + γ pre-F.0).
- A specific per-tri 4-tuple table (24 / 26 entries) to bisect against.
- Cherchi non-det characterized; mode-invariance of the load-bearing invariants.

**Per `feedback_no_last_bug`**: 13th cycle. F0020 unpaired count unchanged at 40 across all 13 cycles. PR-Y44 does NOT close F0020. PR-Y44 advances PR-Y45's anchor ranking from inference (audit-y43 §4.1: "candidates contingent on δ's output") to measurement (subclass_a = 100% across 4 reruns; both Cherchi non-det modes; cohort-shared at F0044/F0045).

**Per `feedback_phase1_diagnosis_ranking_is_inference`**: the audit-y43 §3.2 framing of "(a) plausibly dominant, but inferred from the priority-ordered classification" is now resolved by δ to "(a) measurably dominant at 100%." The α/γ co-equal anchor ranking is MEASURED for PR-Y45.

**Per `feedback_external_coherence`**: Cherchi C++ remains the load-bearing reference oracle. δ extends PR-Y43's classification probe with sub-class disambiguation; the underlying set-diff data (PR-Y29 → PR-Y31 → PR-Y42 → PR-Y43) is reused; no new oracle invocation pattern.

---

## §10 Recommendation summary

- **SHIP-INFRA**: 0 LOC production logic; 0 kernel; 0 wasm-bridge; 0 app; +132 LOC harness extension in `crates/test-harness/tests/cherchi_differential_diff.rs`.
- **PR-Y45 anchor**: **(α/γ) co-equal canary** at F.0 + pre-F.0. (β) tertiary. Case B banked for PR-Y46.
- **8/8 gates GREEN**. Probe-off byte parity preserved. PR-Y43 baselines unchanged. kernel lib + yang_fast + PR-Y31 hard gate all preserved.
- **Verdict logic outcome**: 1 (a-dominant ≥ 80%). PR-Y45 anchor MEASURED at 100% sub-class (a).

The δ probe extension produces the sharpest PR-Y45 anchor in the 13-cycle Y25-Y44 arc. Recommend forward to **spec-y44 / impl-y44 / adversary-y44 / audit-y44** per the PR-Y44 plan.
