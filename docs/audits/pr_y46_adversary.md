# PR-Y46 adversary — independent re-run + Layer A/B byte-verify

**Verdict:** **ACCEPT** — independent re-run BYTE-MATCHES canary at Layer A = 0/24 (0.0%), Layer B = 24/24 (100.0%). All 8 verification gates GREEN. The 100% Layer-B claim survives aggressive stress-testing across 6 scrutiny axes. PR-Y47 anchor at γ Render-LOD retess (`yang_integration.rs:1024`) is empirically load-bearing.

This memo is the **adversary-y46** independent re-verification of PR-Y46's canary findings, executed per `feedback_oracle_credibility_via_role_separation` (canary built the oracle; adversary independently re-runs and stress-tests it) and `feedback_adversary_no_destructive_git` REINFORCED (worktree isolation; zero destructive git ops; zero production-code or harness-code modification).

---

## §1 Mandate + worktree state

### §1.1 Mandate

Independently re-run the PR-Y46 Stage Bb→B→E bisection probe. Stress-test the 100% Layer-B claim across 7 scrutiny axes (OBJ parsing, |B \ Bb|=0 implication, |E \ Bb|=71 float-precision concern, Case D direction, Cherchi non-det, Bb=420 source, +289 LOC code review). Apply `feedback_reference_oracle_invalidates_in_both_directions` and `feedback_adversary_recommendations_need_canary`.

### §1.2 Worktree HEAD

```
$ git log -1 --oneline
b0009bd audit(yang-pr-y42): ACCEPT — Render LOD diff harness; 50% F0020 attribution; B.1 pivot honestly framed

$ git branch --show-current
worktree-canary-y36
```

**Note on HEAD:** the brief specified `2fa4058` (PR-Y46 impl) as expected HEAD; this worktree's branch HEAD is at `b0009bd` (PR-Y42 audit) with the PR-Y43/Y44/Y45/Y46 work present as **uncommitted + untracked** working-tree state plus committed reflog refs reachable via direct SHA. Verified byte-equivalence: `git diff 2fa4058 -- crates/test-harness/tests/cherchi_differential_diff.rs` returns 0 lines (working tree byte-matches the PR-Y46 commit content for the probe file). Per `feedback_adversary_no_destructive_git` REINFORCED, did NOT switch HEAD via `git checkout` — operated on the live working tree which IS PR-Y46 content.

### §1.3 Non-destructive-git compliance

ZERO destructive git operations performed:
- No `git stash` / `git stash pop`
- No `git checkout --` / `git checkout <ref>`
- No `git reset`
- Reads: `git log --all --oneline`, `git log <SHA> --oneline -5`, `git show 2fa4058 --stat`, `git diff 2fa4058 HEAD --stat`, `git diff HEAD <ref>:<path>`, `git diff 2fa4058 -- <path>` — all read-only.
- Working tree was UNTOUCHED throughout.

---

## §2 8-gate independent re-run results

| # | Gate | Expected | Observed | Status |
|---|---|---|---|---|
| **A** | Probe builds at HEAD | clean | clean (`Finished dev profile in 0.03s`) | **GREEN** |
| **B** | F0020 probe-off byte parity | Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int | Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int | **GREEN** |
| **C** | Independent stage-dump generation | Bb=420, B=246, E=113 | Bb=420 f / 141 v; B=246 f / 141 v; E=113 f / 219 v | **GREEN** |
| **D** | Independent Case D positions file | 24 entries; byte-match canary's | 24 entries; **BYTE-IDENTICAL** to canary's `/tmp/y46-f0020-case-d-positions.txt` (diff returns empty) | **GREEN** |
| **E** | Independent bisection ≥3 reruns | Layer A=0/24, Layer B=24/24 byte-stable | Run 1 (own dumps + own positions): 0/24, 24/24; Run 2 (r2 dumps): 0/24, 24/24; Run 3 (r3 dumps): 0/24, 24/24 | **GREEN** |
| **F** | PR-Y43+Y44+Y45 baselines preserved | A/B/C/D=4/14/0/24 (42-mode); sub-class (a)=100% (24/24); α attribution 0/24 | A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), D=24 (57.1%); sub-class (a)=24/24 (100.0%); bucket-sum 24+0+0=24 OK | **GREEN** |
| **G** | kernel lib + yang_fast | 1262/24/42 + 10/157 | kernel lib: 1262 passed / 24 failed / 42 ignored; yang_fast: 10/157 (139 failed, 8 errored, 33 timeouts skipped) | **GREEN** |
| **H** | PR-Y31 hard gate | F0044 missing=0, extras=0, common=136 | F0044 Cherchi=136 / Waffle=136 / common=136 / missing=0 / extras=0; test passed | **GREEN** |

**8/8 gates GREEN.** All baselines preserved; Layer A/B independently confirmed byte-identical to canary §5.2.

---

## §3 Stage-dump comparison (canary vs adversary)

### §3.1 Raw f-counts

| Stage | Canary (`/tmp/y46-stages-f0020/F0020/`) | Adversary (`/tmp/adversary-y46-stages-f0020/F0020/`) | Match |
|---|---:|---:|---|
| stage_A.obj | 420 f / 141 v | 420 f / 141 v | YES |
| stage_Bb.obj | 420 f / 141 v | 420 f / 141 v | YES |
| stage_B.obj | 246 f / 141 v | 246 f / 141 v | YES |
| stage_F.0.obj | 138 f / 217 v | 138 f / 217 v | YES |
| stage_F.4.obj | 113 f / 219 v | 113 f / 219 v | YES |
| stage_E_lod=Render.obj | 113 f / 219 v | 113 f / 219 v | YES |

### §3.2 Byte-diff across reruns

`diff /tmp/adversary-y46-stages-f0020/F0020/stage_Bb.obj /tmp/adversary-y46-stages-f0020-r2/F0020/stage_Bb.obj` → **BYTE-IDENTICAL**.

`diff /tmp/adversary-y46-stages-f0020/F0020/stage_B.obj /tmp/adversary-y46-stages-f0020-r2/F0020/stage_B.obj` → **BYTE-IDENTICAL**.

`diff /tmp/adversary-y46-stages-f0020/F0020/stage_E_lod=Render.obj /tmp/adversary-y46-stages-f0020-r2/F0020/stage_E_lod=Render.obj` → **NEAR-IDENTICAL** with one observed difference:

```
218d217
< v 5.08022755384445190430e-2 8.78301262855529785156e-3 -1.96913197636604309082e-1
219a219
> v 5.08022755384445190430e-2 8.78301262855529785156e-3 -1.96913197636604309082e-1
326,330d325
< f 218 160 2
[etc — 5 face lines]
332a328,332
> f 219 160 2
[etc — 5 face lines]
```

A single vertex position byte-identical at coordinates, with index swap 218 ↔ 219 between runs. Five face records referencing this vertex update accordingly. The vertex itself emits at identical float precision — this is **HashMap/BTreeMap iteration-order non-determinism** within the Render LOD emit path, NOT geometric non-determinism.

**Crucially:** this swap is canonical-key-invariant. Both runs produce the same set of `(quantized_pos_a, quantized_pos_b, quantized_pos_c)` triples after `quantize_tri`, because the underlying vertex positions are byte-identical. The bisection probe's set-arithmetic is therefore robust to this kind of harmless reordering. Verified by Gate E run 2's byte-match with run 1.

### §3.3 [yang-diag] log invariance

Across 5 spotlight_f0020 reruns:

| Run | STAGE6 triangulation | after subdivide | after survival | E_lod=Render unpaired/multi/χ/verts/tris/edges |
|---|---:|---|---|---|
| 1 | 420 tris | tris_a=290, tris_b=130, verts=141 | 20 groups, 246 tris | 56/18/2/77/113/188 |
| 2 | 420 | 290, 130, 141 | 20, 246 | 56/18/2/77/113/188 |
| 3 | 420 | 290, 130, 141 | 20, 246 | 56/18/2/77/113/188 |
| 4 | 420 | 290, 130, 141 | 20, 246 | 56/18/2/77/113/188 |
| 5 | 420 | 290, 130, 141 | 20, 246 | 56/18/2/77/113/188 |

The pipeline is **fully deterministic** at the count + tri-set level for F0020 across 5 independent reruns. The only observed non-determinism is the OBJ vertex-emit-index swap (§3.2), which is canonical-key-invariant.

---

## §4 Position-list extraction comparison (canary vs adversary)

### §4.1 Byte-diff

```
$ diff /tmp/adversary-y46-case-d-positions.txt /tmp/y46-f0020-case-d-positions.txt
[empty output → BYTE-IDENTICAL]
```

### §4.2 d[16] spot-check

Adversary line 17 (= 19th line with header): `142179 -122161 -80083 156339 -119712 -121783 204678 -111355 -115049`

Canary §3.4 d[16]: `qa=(+0.142, -0.122, -0.080) qb=(+0.156, -0.120, -0.122) qc=(+0.205, -0.111, -0.115)`

Decimal-to-i64 at 1e-6 grid: 142179 ↔ +1.42179e-1 = +0.142; -122161 ↔ -1.22161e-1 = -0.122. **MATCH.**

### §4.3 Y44 attribution log structure

Re-running `f0020_render_lod_nearest_attribution` produces 24 `d[]` entries all classified `(a)` sub-class (m1x=3, m5x=3, all-3 verts present at 1× grid). Per-cycle baselines (PR-Y43 §Y43 + PR-Y44 §Y44):
- Case A=4 (9.5%) — sub-grid drift
- Case B=14 (33.3%) — partial 2-of-3 match
- Case C=0 (0.0%) — no proximity (Option C refuted)
- Case D=24 (57.1%) — residual (sub-class (a) = 24/24 = 100%)
- target_tris=42 (42-mode)
- bucket-sum 4+14+0+24=42 OK

All match canary §6 Gate 3 + audit-y45 §3.1. PR-Y45 α attribution (0/24 byte-stable) is in `repair.rs::remove_winding_insensitive_duplicates` (separate kernel path), not invoked by Y46 probe — no Y46-side regression.

---

## §5 Independent attribution re-run results

### §5.1 Three reruns of bisection probe (canonical Layer-A vs Layer-B counts)

| Run | Stage dir | |Bb\B| | |B\E| | |Bb∩E| | Layer A (Case D) | Layer B (Case D) | NEITHER |
|---|---|---:|---:|---:|---:|---:|---:|
| 1 | `/tmp/adversary-y46-stages-f0020/F0020` | 171 | 194 | 41 | **0 / 24 (0.0%)** | **24 / 24 (100.0%)** | 0 |
| 2 | `/tmp/adversary-y46-stages-f0020-r2/F0020` | 171 | 194 | 41 | **0 / 24 (0.0%)** | **24 / 24 (100.0%)** | 0 |
| 3 | `/tmp/adversary-y46-stages-f0020-r3/F0020` | 171 | 194 | 41 | **0 / 24 (0.0%)** | **24 / 24 (100.0%)** | 0 |

**Aggregate: 0/24 Layer A; 24/24 Layer B; byte-identical across 3 reruns × 3 independent stage-dump dirs.** Probe-output deterministic given identical canonical-tri sets, which themselves are deterministic up to vertex-emit-index swap (§3.2 — irrelevant to canonical keys).

### §5.2 Per-tri layer assignment (24 entries)

All 24 entries (d[0]–d[23]) present identically: `inBb=1 inB=1 inE=0 → Layer B`. Every Case D position is:
- PRESENT at Stage Bb (post-`label_cells`, pre-survival)
- PRESENT at Stage B (post-`face_survival_detect`) — **face_survival_detect KEEPS all 24**
- ABSENT from Stage E_lod=Render (post-γ retess) — **γ retess DROPS all 24**

No entry assigned NEITHER (defect upstream/elsewhere). No entry assigned PRESENT_AT_E (would indicate PR-Y44 mis-classification).

### §5.3 Sanity-check partition invariant

```
[pr-y46] SANITY: |Bb| - |union(A_losers, B_losers, E_survivors)| = 0 (expect 0 if monotone-decreasing)
[pr-y46] SANITY: |E \ Bb| triangles ADDED post-Bb = 71 (γ retess re-samples — may be non-zero; informational)
[pr-y46] SANITY: |B \ Bb| triangles ADDED post-survival = 0 (expect 0 — face_survival_detect is selective only)
```

**3/3 reruns:** |Bb| - |union| = 0 (zero stragglers); |E\Bb| = 71 (γ retess replaces ~63% of E's 112 unique canonical tris with fresh-vertex re-samples); |B\Bb| = 0 (face_survival_detect monotone).

### §5.4 Pure-Python independent re-derivation

Independent re-derivation in Python from raw OBJ files (zero Rust harness involvement):

```
|Bb| raw f=420 unique canonical=401
|B|  raw f=246 unique canonical=230
|E|  raw f=113 unique canonical=112
|Bb ∩ B| = 230;  |Bb \ B| = 171;  |B \ Bb| = 0
|Bb ∩ E| = 41;   |E \ Bb| = 71;   |Bb \ E| = 360
|B ∩ E|  = 36;   |B \ E|  = 194;  |E \ B|  = 76
PARTITION CHECK: |Bb| = 401, |A_losers ∪ B_losers ∪ (Bb ∩ E)| = 401, diff = 0
Case D in Bb: 24/24; in B: 24/24; in E: 0/24
Layer A (Bb\B): 0/24; Layer B (B\E): 24/24; In both: 0/24
```

**Byte-match with Rust probe.** Two independent OBJ-parsing + canonical-key + set-arithmetic implementations converge on identical counts. Cross-implementation oracle agreement.

---

## §6 Stress-test findings — 7 scrutiny items addressed

### §6.1 Scrutiny #1 — OBJ-parsing correctness ✓ RESOLVED

**Concern:** triangle canonical-key construction, vertex de-duplication, float→i64 quantization consistency, Cherchi non-det float-precision drift.

**Investigation:**
- `quantize_tri` (line 175-183) sorts 3 `(i64, i64, i64)` quantized positions at 1e-6 grid → matches PR-Y45 / Y43 / Y44 / Y30 canonical-key form.
- `parse_obj` (line 94-159) handles 1-indexed OBJ vertex refs via `i.checked_sub(1).unwrap_or(0)` — minor smell (silent 0-index→0 mapping) but doesn't affect Yang stage-dump output (writer always emits 1-indexed).
- Float→i64 quantization at 1e-6 (`.round() as i64`) is consistent across all three OBJ files since they share the source vertex set at Stage Bb / B (`v=141`) and the γ retess emits fresh verts at the SAME float precision as the OBJ writer (the writer just dumps f64s; LOD doesn't affect emit precision).
- Pure-Python re-derivation byte-matches Rust probe (§5.4) → both implementations agree.

**Conclusion:** OBJ parsing is correct; canonical-key arithmetic is sound.

### §6.2 Scrutiny #2 — `|B \ Bb| = 0` reads as "B ⊆ Bb" ✓ RESOLVED

**Concern:** Bb=420 raw has MORE tris than B's 246; "face_survival_detect should retain a SUBSET" → expect `|B \ Bb| = 0`. Verify directly.

**Investigation:** Probe explicitly outputs `|B \ Bb| triangles ADDED post-survival = 0`. Pure-Python verifies: `|B \ Bb| = 0` (§5.4). The 401-230 = 171 canonical-tri reduction is consistent with face_survival_detect's Yang §3.3 + Cherchi 2022 §5 selective-retention semantics: it monotonically picks a subset of Bb's canonical-tri set, never adds new positions.

**Conclusion:** B ⊆ Bb confirmed. Face_survival_detect is monotone-selective. The "suspiciousness" was an artifact of the brief framing — the 100% Layer B claim is not a one-bucket artifact; it's a clean partition with `|B \ Bb| = 0` as expected.

### §6.3 Scrutiny #3 — `|E \ Bb| = 71` float-precision interpretation ✓ RESOLVED

**Concern:** γ retess generates NEW vertex set at 64-seg Render LOD vs Bb's 16-seg Boolean LOD. If E's positions are at different float precision than Bb, the 71 "new" tris could be artifact of precision mismatch, not real new geometry.

**Investigation:** Computed `|E ∩ Bb| = 41` directly. Then `|E ∩ B| = 36` — even better cross-check: B is the strictly-selected subset of Bb, so the 36 tris in `B ∩ E` are exact canonical matches between Stage B (post-survival, Boolean-LOD-sourced) and Stage E (post-γ retess, Render-LOD-sourced). **36 / 112 = 32% of E's unique canonical tris are preserved exactly through γ retess at canonical-key level**. If precision drift were the cause of the 71 "new" tris, this 36-overlap would be ~0.

The 71-tri E-only set therefore represents **genuinely new triangulation** from per-face independent CDT re-mesh at higher LOD — different vertex pairings on shared edges between adjacent B-Rep faces produce different interior triangulations of the same surface region.

**Conclusion:** No precision mismatch. The 71 ADDED tris are real geometric re-samples (γ retess replaces tris, doesn't just drop). The 36 PRESERVED tris confirm the canonical-key methodology measures real position equality.

### §6.4 Scrutiny #4 — Case D direction cross-reference ✓ RESOLVED

**Concern:** Case D positions are Cherchi-side (Cherchi-emitted-missing-from-Waffle). Stages Bb/B/E are Waffle-side. The probe asks: at which Waffle stage is each Cherchi-emitted Case D triangle present, and at which stage does Waffle drop it?

**Investigation:** Read PR-Y44 attribution code semantics — `d[]` rows are Cherchi-side positions emitted from the diff `cherchi_set \ waffle_set` after canonical-key quantization. The 24 are Cherchi triangles that ARE in Cherchi's output but ARE NOT in Waffle's final Render LOD output. PR-Y44 sub-classification confirms all 24 are sub-class (a) `m1x=3, m5x=3`: their three vertex positions are present in Waffle's vertex set at 1× grid, but Waffle never emits the triangle that uses those 3 verts.

The probe's check `case_d_position ∈ stage_bb_set?` therefore asks: "does Waffle's Stage Bb canonical-tri set contain this triple of quantized positions?" Answer for all 24: YES at Stage Bb (post-arrangement), YES at Stage B (post-survival), NO at Stage E (post-γ retess). Direction is correct.

**Conclusion:** Case D direction verified. The 24 are Cherchi-side ground truth; the probe correctly checks Waffle-side presence at each pipeline stage.

### §6.5 Scrutiny #5 — Cherchi non-det invariance ✓ RESOLVED

**Concern:** Spec §8 claims "mode-invariance proven (47-mode minimum = 92.3%)" needs independent verification with 47-mode appearances.

**Investigation:** 5+3 = 8 spotlight_f0020 reruns across this audit produced ALL 42-mode (`target_tris=42`) under default thread count. 47-mode NOT observed. Stage Bb/B/E f-counts byte-identical across all reruns; STAGE6 triangulation = 420 invariant; survival = 246 invariant; Render-LOD output = 113 invariant.

The decision-gate fires at percentage thresholds (Layer-B-dominant ≥ 80%), not absolute counts. Even if 47-mode appeared with 2 extra entries (per PR-Y44 §4.2), worst-case Layer B = 24 / 26 = 92.3% which is still above 80% threshold. **Decision-gate verdict is invariant under Cherchi mode.**

Additionally observed: the only non-determinism between reruns is vertex-emit-index swap (single vertex 218 ↔ 219) in Render-LOD output OBJ (§3.2). This is canonical-key-invariant.

**Conclusion:** 47-mode bound not exercised in this audit but bound is sound; 42-mode dominance confirmed in 8/8 reruns; canonical-key methodology robust to OBJ vertex-emit reordering.

### §6.6 Scrutiny #6 — Bb=420 vs 246 discrepancy ✓ RESOLVED

**Concern:** Brief said "Expect Bb=246-ish raw, E=113 raw." Canary memo §4.2 found Bb=420 actually. Where does 420 come from? Are stage Bb dumps from both meshes combined?

**Investigation:** Verified from `[yang-diag] after subdivide: tris_a=290, tris_b=130, verts=141` for the load-bearing third boolean invocation: 290 + 130 = **420 = STAGE6 triangulation output**. Stage Bb dump site at `topology_extract.rs:2396` writes the post-`label_cells` mesh which is the post-`tessellate_intersection_pieces` arrangement of BOTH A and B inputs combined into a single SoupMesh by Cherchi-Rust's arrangement output. This is NOT "external combination" — it's the canonical Cherchi 2020 §4 arrangement output for the boolean's two-solid input.

The brief's "246-ish" was the canary's anticipated count from prior Phase 1 exploration but actually corresponds to Stage B (post-`face_survival_detect`). Canary memo §4.2 explicitly documents this and updates the bisection arithmetic correctly: Layer A losers = `Bb \ B` = `face_survival_detect` drops; Layer B losers = `B \ E` = γ retess drops. The methodology is unaffected by the Bb=420-not-246 finding — the bisection layers are correctly attributed to the right operations.

**Conclusion:** Bb=420 is the load-bearing-third-boolean post-arrangement output (tris_a + tris_b combined by Cherchi-Rust arrangement). Methodology correct.

### §6.7 Scrutiny #7 — Code review of +289 LOC ✓ RESOLVED (one minor banked)

**Investigation of all three new functions:**

1. **`load_case_d_positions_file` (lines 1695-1728)** — parses 9 i64 tokens per line, builds 3-tuple, **sorts** before push. Sort matches `quantize_tri`'s canonical form. Comment-line skip + empty-line skip correct. Token-count check (`!= 9`) panics on malformed input (correct fail-loud per `feedback_anchor_before_fix`). ✓

2. **`load_obj_canonical_tri_set` (lines 1733-1741)** — calls `parse_obj` (reused from PR-Y29..Y45), iterates triangles, applies `quantize_tri`, inserts into HashSet (auto-dedupes winding-insensitive duplicates). Idiomatic + correct. ✓

3. **`f0020_stage_bb_b_e_bisection` (lines 1753-1943)** — env-var-driven paths (default `/tmp/y46-...`), skip-cleanly-if-missing pattern (correct), set-diff arithmetic via `HashSet::difference`, per-tri layer assignment with explicit `(in_a, in_b)` match arms, summary + sanity + verdict echo. ✓

**Smells / observations:**
- `parse_obj` line 136 `i.checked_sub(1).unwrap_or(0)` silently maps invalid `f 0 N M` to `f -1 N M → f 0 N M`. Not triggered by Yang stage-dump writer (always 1-indexed); banked as hygiene observation (not Y46-introduced; pre-existing).
- Layer "A+B" branch (line 1846-1850 `(true, true) => ...`) is unreachable when B ⊆ Bb (which the probe verifies via `|B \ Bb| = 0` sanity). The branch is defensive; counts as 1 each toward A and B (which is technically inconsistent with bucket-sum but unreachable so moot). Banked.
- Decision-gate "MIXED (both ≥30%)" and "NEITHER (both ≤20%)" branches between 20-30% are "AMBIGUOUS" — fall-through left for caller to interpret. Sound design.
- File-path resolution does not auto-detect which stage_E_lod=Adaptive_*.obj is load-bearing — the probe explicitly targets `stage_E_lod=Render.obj` (the final pipeline output). Correct for the bisection question.

**Conclusion:** Code is clean, idiomatic, correctly implements the bisection methodology. One pre-existing hygiene smell in `parse_obj` (banked, not Y46-introduced). Layer A+B unreachable branch is defensive-not-load-bearing.

---

## §7 Code review of +289 LOC — bugs / smells summary

**No load-bearing bugs found.** Summary of all findings:

| Concern | Location | Severity | Disposition |
|---|---|---|---|
| `parse_obj` silent 0-index→0 mapping | line 136 `i.checked_sub(1).unwrap_or(0)` | LOW (defensive; not triggered by Yang stage writer) | Banked hygiene; pre-existing, not Y46-introduced |
| Layer A+B branch unreachable when B ⊆ Bb | line 1846-1850 | LOW (defensive) | Banked; counts inconsistency only matters if invariant breaks (it doesn't per §5.3) |
| `stage_E_lod=Adaptive_*.obj` filename collision risk | shared with PR-VIZ-1 banked | LOW (not F0020) | Banked from PR-VIZ-1; not Y46-introduced |
| Decision-gate ambiguity in 20-30% / 30-80% bands | lines 1931-1941 | LOW (clearly labeled "AMBIGUOUS") | Sound design |

---

## §8 Verdict

### §8.1 Verdict: **ACCEPT 24/24 (100.0%)** at Layer B

The 100% Layer-B claim is empirically verified by:
1. Independent stage-dump generation (3 dirs) + bisection probe — all 3 runs byte-identical 0/24 Layer A, 24/24 Layer B.
2. Independent Case D positions file generation — byte-identical to canary's via independent Y44 attribution run.
3. Pure-Python re-implementation of OBJ-parse + canonical-key + set-arithmetic — byte-matches Rust probe across all reported counts (171/194/41 + |B\Bb|=0 + |E\Bb|=71 + |Bb∩E|=41 + |B∩E|=36 + Case D 24/24).
4. 8/8 verification gates GREEN.
5. 7/7 scrutiny axes resolved (6 fully verified + 1 banked-as-hygiene).

The claim survives aggressive stress-testing. The shape "clean 100%" was suspicious in pattern (per PR-Y45's clean 0/24 precedent) but is borne out by multiple independent oracles converging on identical attribution.

### §8.2 Per `feedback_reference_oracle_invalidates_in_both_directions`

The PR-Y46 measurement at canary §5.2 (Layer A=0/24, Layer B=24/24) is a single-oracle finding from one canary build. After the canary's clean refutation/confirmation, **adversary independently re-ran from scratch** with own stage dumps + own positions file. **Both directions confirmed:**
- Layer A refutation: NOT 1, NOT 2, NOT 4 — ZERO Case D positions drop at face_survival_detect.
- Layer B confirmation: NOT 18, NOT 22, NOT 23 — ALL 24 Case D positions drop at γ retess.

Both ends of the claim hold under independent re-run. The methodology partitions correctly: `|Bb| = |A_losers ∪ B_losers ∪ (Bb ∩ E)|` with zero stragglers. No methodological loophole survives the cross-implementation Python re-derivation.

### §8.3 Per `feedback_oracle_credibility_via_role_separation`

Role separation verified:
- **canary-y46** built the bisection probe (+289 LOC test-harness fn + 2 helpers).
- **adversary-y46 (this memo)** independently re-ran without trusting canary logs:
  - Independent stage-dump dir (`/tmp/adversary-y46-stages-f0020/`, NOT `/tmp/y46-stages-f0020/`)
  - Independent Case D positions file (`/tmp/adversary-y46-case-d-positions.txt`, NOT `/tmp/y46-f0020-case-d-positions.txt`)
  - Independent run logs (`/tmp/adversary-y46-*.log`)
  - Independent Python re-derivation (§5.4) confirms Rust probe independently

The interpretation of the 0/24 + 24/24 finding (and PR-Y47 anchor recommendation) belongs to **audit-y46** (next role). This adversary memo confines itself to verification.

---

## §9 PR-Y47 anchor stress-test — does γ retess at `yang_integration.rs:1024` hold up?

### §9.1 The anchor

Canary §7 + §8.2 recommends PR-Y47 anchor = `tessellate_waffle_solid` at `yang_integration.rs:1024` (Render-LOD re-tessellation of the WaffleSolid B-Rep). The empirical basis is Layer B = 24/24 = 100% direct measurement.

### §9.2 Stress-test of the anchor

**Question 1: Could `tessellate_waffle_solid` at line 1024 be a wrong anchor — could the drop be UPSTREAM of γ retess (e.g., at the WaffleSolid B-Rep assembly in `assemble_brep_topology`)?**

Verification: the 24 Case D triangles are PRESENT at Stage B (post-survival, raw arrangement triangles). The B-Rep assembly happens AFTER Stage B (in `topology_extract`). γ retess at line 1024 consumes the resulting WaffleSolid. If the B-Rep assembly drops information that γ retess then can't recover, the anchor would NOT be "γ retess" but "B-Rep assembly's loss of triangulation information." Canary §8.4 banks this scenario:
> If F.0→F.4 sub-bisection refutes γ retess, anchor moves to flood_fill_patches / assemble_brep_topology / per-face independence.

**My take:** the anchor "γ retess at `yang_integration.rs:1024`" is correct AT THE COARSE LEVEL — the drop is between Stage B (input to γ retess via B-Rep) and Stage E (output of γ retess). The SUB-LAYER that causes the drop could be:
- B-Rep assembly losing triangulation info (input to γ retess is already lossy)
- γ retess's per-face independent CDT not pairing edges between adjacent faces
- γ retess emitting different triangulation than Stage B's per-face triangles (the 71 ADDED tris in `|E\Bb|`)

Per `feedback_multi_stage_anchor_probe`: PR-Y47 should sub-bisect across F.0/F.1/F.2/F.3/F.4 + B-Rep-assembly stage to identify the exact sub-layer. Canary §8.3 already prescribes this discipline (PR-Y47 sub-bisection before fix).

**The PR-Y47 anchor recommendation is sound but COARSE.** It correctly localizes the drop to "between B-Rep assembly + γ retess" but does NOT yet identify the specific sub-step. PR-Y47 canary MUST sub-bisect before fix shape.

### §9.3 Banked PR-Y47 candidates

Adversary endorses canary §8.4 banked candidates:
1. **F.0→F.4 sub-bisection** — already-captured stage dumps make this near-free.
2. **B-Rep assembly (`assemble_brep_topology`)** — if Stage F.0 (138 tris) is missing the 24 Case D, the drop is at B-Rep assembly, not γ retess.
3. **Per-face independent CDT seam alignment** — Stage E's |E\Bb|=71 new tris show γ retess produces fresh triangulation; check whether adjacent B-Rep faces' CDTs share endpoints at intersection curves.

### §9.4 Per `feedback_adversary_recommendations_need_canary`

The "PR-Y47 anchor = γ retess" is canary-measured (not just inferred). However, the SUB-LAYER within γ retess is NOT YET measured. PR-Y47 should not commit fix shape at any sub-layer without canary measurement at that sub-layer specifically. The 24/24 = 100% finding establishes the coarse anchor; sub-anchor is inference until canary'd.

---

## §10 Open / banked

### §10.1 Banked from this adversary cycle

1. **`parse_obj` line 136 silent 0-index→0 mapping** — pre-existing hygiene smell; not Y46-introduced; not load-bearing under current OBJ writer. Banked.
2. **Layer A+B unreachable branch (line 1846)** — defensive; sound under invariant `B ⊆ Bb` which the probe verifies. Banked.
3. **47-mode bound not exercised** — 8/8 reruns produced 42-mode under default thread count. Bound is sound but not stress-tested in 47-mode. Banked for future cohort cases.
4. **Vertex-emit-index swap in Render-LOD OBJ** (§3.2) — HashMap iteration-order non-determinism; canonical-key-invariant. Banked as expected OBJ-writer non-det.

### §10.2 Forward-carry for audit-y46

1. **PR-Y47 anchor is COARSE-CORRECT but SUB-ANCHOR is INFERENCE.** Audit-y46 should preserve canary §8.3 discipline ("PR-Y47 canary discipline LOAD-BEARING — sub-bisect Layer B before fix").
2. **Banked candidates** (§9.3) — F.0→F.4 sub-bisection, B-Rep assembly, per-face CDT seam alignment.
3. **Cherchi non-det invariance** — decision-gate is percentage-based; sound even under 47-mode (worst-case 92.3% > 80% threshold).

### §10.3 Forward-carry for PR-Y47 adversary

Recommend:
- Re-emphasize `feedback_adversary_no_destructive_git` (FOURTH reinforcement). This adversary-y46 ran fully non-destructive; precedent maintained.
- For PR-Y47 sub-bisection: probe BOTH `stage_F.0` and `stage_F.4` + a NEW B-Rep-assembly-output dump (if available); sub-attribute the 24 Case D drops to the specific sub-layer.
- Independent stage-dump dir naming convention: `adversary-y47-stages-f0020-*` (do NOT reuse `/tmp/y46-*` or `/tmp/adversary-y46-*`).
- Pure-Python re-derivation as cross-implementation oracle (this adversary added it for the first time; recommend as standard for INFRA-PR adversary cycles going forward).

### §10.4 Notable observation not load-bearing for verdict

The probe's emit path is deterministic given identical canonical-tri sets, but identical canonical-tri sets are themselves deterministic only up to vertex-emit-order non-determinism. This is an unusual property worth noting: **the probe's output is robust to a non-determinism in its input that doesn't affect the question it answers.** PR-Y47 adversary should preserve this property when sub-bisecting (i.e., do NOT compare raw OBJ files or vertex indices; compare canonical-tri sets).

---

## §11 End-of-adversary status

- **Verdict:** **ACCEPT** — Layer A=0/24 (0.0%); Layer B=24/24 (100.0%); 8/8 gates GREEN; 7/7 scrutiny axes resolved.
- **Production-code modified:** 0 LOC (this is read-only verification).
- **Harness-code modified:** 0 LOC (zero touch).
- **Destructive git operations:** 0 (full compliance with `feedback_adversary_no_destructive_git` REINFORCED).
- **Cross-implementation oracle:** pure-Python re-derivation (§5.4) byte-matches Rust probe (Layer A=0/24, Layer B=24/24, |Bb\B|=171, |B\E|=194, |Bb∩E|=41, |E\Bb|=71, |B\Bb|=0, |B∩E|=36).
- **PR-Y47 anchor:** ENDORSED at coarse level (γ retess at `yang_integration.rs:1024`); SUB-ANCHOR REMAINS INFERENCE pending PR-Y47 canary sub-bisection.

Per `feedback_per_plan_cycle_team`: this adversary memo is the final independent-verification artifact for the PR-Y46 cycle. Audit-y46 (task #5) reads this + canary memo to produce the ACCEPT/REJECT memo with PR-Y47 anchor.
