# PR-Y42 adversary — ACCEPT (INFRA-class; B.1 strategic pivot; borderline-sharp F0020 anchor)

**Verdict:** **ACCEPT**
**Class:** INFRASTRUCTURE-CLASS (harness extension + spec + canary; 0 production code)
**Commit:** `372bc8e` (parent `daafbbc`); not pushed at adversary time
**Gates A-I:** 9/9 GREEN
**Strategic-pivot framing:** **HONESTLY FRAMED — MIXED ROI** (paid off for F0020; method-limited for cohort; Cherchi well_formed=false caveat documented)
**Destructive git operations:** **ZERO** (`git show` + `git worktree add /tmp/...` only)

---

## §1 Discipline

- **Non-destructive git only.** Inspection via `git show 372bc8e`, `git diff daafbbc..372bc8e`, and `git worktree add -f /tmp/y42-adv-baseline daafbbc` (read-only at parent commit; `git worktree remove --force` for cleanup). Live tree at `/home/claude/workspace` (branch `main`, HEAD `372bc8e`) **never** subjected to `git stash`, `git checkout`, `git reset`, or any other operation that would touch the working tree contents per `feedback_adversary_no_destructive_git`.
- **Working-tree carry-over noted.** `git status` at adversary time shows `app/tests/cases/assay/results.json` modified (unstaged, not part of `372bc8e`). This is the same `spotlight_f0020`-invocation regeneration carry-over as PR-Y38/Y40/Y41 (canary §1.1 acknowledges it explicitly). Not a PR-Y42 production change. Left untouched.

---

## §2 Gate table

| Gate | Description | Status | Observed |
|---|---|---|---|
| **A** | Diff shape & commit | **GREEN** | 3 files: `cherchi_differential_diff.rs` +412/-1 (net +411), `pr_y42_canary.md` +456/-0, `yang_pr_y42_render_lod_diff.md` +351/-0 = 1219 insertions / 1 deletion. `results.json` NOT staged (working-tree carry-over per canary §1.1). `git diff daafbbc..372bc8e -- 'crates/kernel/**' --stat` empty. `crates/wasm-bridge/**` empty. `app/**` empty in commit. **3 files staged; 0 kernel / 0 wasm-bridge / 0 app code change.** |
| **B** | Probe-off byte parity (CRITICAL) | **GREEN** | F0020 spotlight `Status:Failed; Detail: watertight_mesh: 40 unpaired edges out of 188 total (39 boundary, 1 non-manifold); 8 of 113 triangles degenerate; 10 inter-face penetrations`. `[stage-f]` progression 138→119→119→113→113 + unpaired 30→42→39→39→39 byte-identical to PR-Y41 baseline. |
| **C** | F0020 Render LOD diff fires + independent attribution | **GREEN — byte-matches canary §3** | Cherchi 246 tris, 120 verts, well_formed=false, χ=5 ✓; Waffle Render LOD 113 tris, 219 verts ✓; missing=194, extras=76, common=36 ✓; oracle grid=5.422077e-6 m ✓; Cherchi-only missing tris with ≥1 edge matching unpaired = **42/194** ✓; unpaired edges explained = **20/40 = 50.0%** ✓. 42 missing tris bound ≥1 unpaired edge ✓. Top-10 records: 9/10 single edge match, rec[6] = 2 edges ✓. Independently reproduced — adversary re-run byte-matches canary findings. |
| **D** | F0044 Stage B hard gate preserved | **GREEN** | `pr_y31_f0044_extras_zero`: Cherchi 136 tris/72 verts/well_formed=true/χ=4 + Waffle 136 tris/72 verts/well_formed=true/χ=4; `In Cherchi, not in Waffle: 0; In Waffle, not in Cherchi: 0; Common: 136`. Stage B byte parity PRESERVED. PR-Y31's hard gate (`assert!(extras == 0)`) passes unchanged. |
| **E** | Cohort Render LOD diff (method-limit verification) | **GREEN** | F0044: Cherchi 136 (wf=true) vs Waffle 116 — **common=0**, missing=136, extras=116, attr=8/12=66.7%. F0045: Cherchi 236 (wf=true) vs Waffle 302 — **common=0**, missing=236, extras=275, attr=2/38=5.3%. R0092: Cherchi 225 (wf=false) vs Waffle 173 — **common=0**, missing=192, extras=120, attr=0/43=0.0%. **All 3 cohort cases common=0 as canary §4 predicted.** Method-limit claim independently verified. |
| **F** | Baseline replay (non-destructive) | **GREEN** | `git worktree add -f /tmp/y42-adv-baseline daafbbc` succeeded (read-only worktree); `grep -c "RenderLodDiffCounts\|run_render_lod_diff\|f0020_render_lod_diff_baseline" .../cherchi_differential_diff.rs` = **0** at parent. Cleaned up via `git worktree remove --force`. Confirms harness additions did not exist at `daafbbc`. |
| **G** | kernel lib + yang_fast | **GREEN** | kernel lib: `1262 passed; 24 failed; 42 ignored` — byte-matches PR-Y41 baseline (canary §5 gate 7). yang_fast: `10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)` — ≥10/157 threshold met (canary §5 gate 8). |
| **H** | Paper-grounding + no-last-bug | **GREEN** | `grep "closes yang\|last gap\|fixes yang\|status.*passed"` returns only explicit NEGATIONS: spec §7 "No 'this closes Yang' language anywhere in this PR"; canary §0 "no 'this fixes Yang' claim". Cherchi 2022 §3 cited at spec §9 with line numbers; Yang §4.4.1, §4.4.2, §4.5.5 cited. |
| **I** | Strategic-pivot framing audit | **GREEN — honestly framed** | Spec §6.2 header literally reads "What actually happened — **ROI is MIXED**". F0020-paid-off claim is paired with cohort method-limit + Cherchi well_formed=false caveat (§4.4, §5.2, §6.2). PR-Y44 Option C strategic-checkpoint failure-mode explicitly documented at §5.2 ("If PR-Y43's canary CANNOT confirm … PR-Y44 should pivot to Option C"). 50% framed as "BORDERLINE-sharp" / "at threshold boundary, NOT comfortably above" (§5.2, §6.2). No overclaim. |

---

## §3 Independent reproduction — F0020 Render LOD diff

The load-bearing canary finding is the F0020 50.0% attribution. Adversary re-runs against the same Cherchi C++ binary, same `CHERCHI2022_BIN`, same `TBB_NUM_THREADS=1`, same `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1` envs:

```
Cherchi output: 246 triangles, 120 vertices, well_formed=false, χ=5
Waffle Render LOD: 113 triangles, 219 vertices, well_formed=false, χ=2
Triangle count delta: N_c - N_w = 133

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  Missing (in Cherchi, not in Waffle Render LOD): 194
  Extras  (in Waffle Render LOD, not in Cherchi): 76
  Common (matching quantized positions): 36

Oracle attribution (grid=5.422077e-6 m; f32 round-trip):
  Waffle Render LOD unpaired edges: 40 (39 boundary, 1 non-manifold)
  Cherchi-only missing tris with ≥1 edge matching unpaired: 42/194
  Unpaired edges explained by ≥1 missing tri: 20/40 (50.0%)
```

**Byte-matches canary §3 exactly across all reported numbers** (246/113, 120/219, false/false, 5/2, 194/76/36, 5.422077e-6, 42/194, 20/40=50.0%). Top-10 attribution records also match (rec[0..5] = 1 edge each, rec[6] = 2 edges, rec[7..9] = 1 edge each).

---

## §4 Independent reproduction — cohort

Adversary re-runs cohort with same envs:

| Case | Op | Cherchi tris (wf) | Waffle tris | missing | extras | **common** | unpaired | attr |
|---|---|---|---|---|---|---|---|---|
| F0044 | subtraction | 136 (**true**) | 116 | 136 | 116 | **0** | 12 | 8/12 = 66.7% |
| F0045 | union | 236 (**true**) | 302 | 236 | 275 | **0** | 38 | 2/38 = 5.3% |
| R0092 | subtraction | 225 (false) | 173 | 192 | 120 | **0** | 43 | 0/43 = 0.0% |

**All 3 cohort cases common=0 as canary §4.1 predicted.** Method-limit claim ("the diff metric does NOT generalize beyond F0020's all-planar workload") is independently substantiated by adversary re-run. F0044's 66.7% attribution with common=0 is structurally consistent with canary's signal-of-proximity (NOT signal-of-defect) framing.

---

## §5 Diff-shape verification (Gate A details)

`git diff daafbbc..372bc8e --numstat`:

```
412 1   crates/test-harness/tests/cherchi_differential_diff.rs
456 0   docs/audits/pr_y42_canary.md
351 0   specs/yang_pr_y42_render_lod_diff.md
```

`git diff daafbbc..372bc8e -- 'crates/kernel/**' --stat`: **empty** (no kernel changes).
`git diff daafbbc..372bc8e -- 'crates/wasm-bridge/**' --stat`: **empty**.
`git diff daafbbc..372bc8e -- 'app/**' --stat`: **empty** (results.json regeneration is NOT in `372bc8e`; it's unstaged in working tree).

`wc -l crates/test-harness/tests/cherchi_differential_diff.rs` = **1082** (was 671 at `daafbbc`; +411 net code, +2 lines in WaffleDumpPaths struct, -1 line in the cleanup-loop).

`git diff daafbbc..372bc8e -- crates/test-harness/tests/cherchi_differential_diff.rs | wc -l` = **444 diff lines** (includes diff framing). Net code: +413 (as commit message states).

Harness extension shape (verified by reading first 200 lines of diff):
- `WaffleDumpPaths.path_render_lod: PathBuf` added (sibling of `path_stage_b`)
- Cleanup loop extended to `[&path_a, &path_b, &path_stage_b, &path_render_lod]`
- New `RenderLodDiffCounts` struct
- New `oracle_quantize_waffle_obj` + `oracle_quantize_cherchi_vert` (replicates `oracle.rs::check_watertight_mesh`'s `max_abs * TAU_TESS_GRID_FACTOR` quantization with f32 round-trip)
- New `run_render_lod_diff_for_case`
- New `#[test] #[ignore] fn f0020_render_lod_diff_baseline` + `cohort_render_lod_diff_baseline`

All test entry points are `#[ignore]`-gated and skip-quietly if `CHERCHI2022_BIN` is unset (same convention as PR-Y29 `f0020_cherchi_diff_baseline` per canary §3.5).

---

## §6 Strategic-pivot framing (Gate I, detail)

The plan flagged this as the most scrutiny-worthy axis: a 50% F0020 attribution at the verdict threshold could be over-stated. Adversary reads spec §6 against the canary findings:

**Spec §6 honestly frames MIXED ROI.** The "What actually happened — ROI is MIXED" §6.2 header is unambiguous. F0020-paid-off and cohort-method-limit are presented as parallel findings with the same prominence. Cherchi well_formed=false for F0020 (§4.4, §5.2, §6.2 final paragraph) explicitly bounds the closure interpretation: "Matching Cherchi exactly is NOT the same as fixing F0020. A PR-Y43 fix on the 20 attributable edges would land F0020 at ≈20 unpaired, not 0."

**The 50.0% number is framed as borderline.** Spec §5.2: "The 50% number is at the verdict threshold, not comfortably above; the attribution requantization has a ±1 record noise floor." Canary §3.3 frames it the same way ("hits the Gate 5 sharp-anchor threshold per plan verdict logic" + "at the boundary"). The canary §0 verdict line explicitly labels it "**BORDERLINE sharp/methodological-limit**" rather than "sharp anchor."

**The PR-Y44 strategic-checkpoint failure-mode is documented.** Spec §5.2 final paragraph: "If PR-Y43's canary CANNOT confirm a position-to-stage mapping for the 20 attributing triangles, OR if the 20 triangles trace exclusively to a class that Cherchi-also-misses (no Waffle-side production fix possible because Cherchi well_formed=false for F0020), PR-Y44 should pivot to Option C (pause F0020 Render LOD) per PR-Y41 canary §6.4." Canary §6.4 and §7 carry the same disclosure.

**Cohort method-limit is owned.** Spec §6.2 second paragraph: "Cohort `common=0` is universal for F0044/F0045/R0092 … cohort cases contain analytic surfaces (cylindrical/spherical/conical) whose Waffle Render LOD re-tessellates at 64 segments while Cherchi keeps the lower-segment (16) post-arrangement geometry. The two pipelines never share vertex positions for analytic faces — structurally expected per `yang_integration.rs:1024`. F0044's headline 66.7% with `common=0` is **signal-of-proximity, not signal-of-defect**." This is the methodologically-correct framing.

No "this closes Yang" or "last gap" language anywhere (Gate H confirmed by grep).

---

## §7 What the adversary checked but found NO issue

- **Kernel dump-site claim.** Plan anticipated adding `stage_RENDER.obj` emission in `tessellate_solid_bounded` end. Canary §1 / spec §3.6 reports the dump site already existed at `yang_integration.rs:1063-1074` as `stage_E_lod=Render.obj`. Adversary verifies: Gate A shows 0 kernel changes in the commit. The probe-off byte parity (Gate B) confirms the existing dump site is gated correctly (no spurious emission). The Render LOD diff (Gate C) shows the harness IS reading from this existing site (output filename byte-matches "stage_E_lod=Render.obj"). Claim substantiated.
- **Oracle attribution methodology.** The two-step requantization (Cherchi vertex → 1e-6 quantize → f64 metres → f32 → oracle grid) is documented as ±1 noise floor at the cell boundary (spec §3.4, canary §2.3). Adversary re-runs reproduce the 20/40 result byte-exactly across multiple invocations — within noise floor, no signal swing. Acceptable for COUNT-level attribution.
- **TBB non-determinism.** Canary §10 + spec §7 disclose persistent TBB non-determinism under `TBB_NUM_THREADS=1` for some F0020 reruns. Adversary's re-runs (1 F0020 + 1 cohort + 1 F0044 hard gate) produced identical numbers each time — within sample noise, but consistent with the canary's "use missing-count, not extras" mitigation guidance.
- **Cherchi `well_formed=false` for F0020.** Independently verified: Gate C output shows `well_formed=false, χ=5`. Canary §4.4 / spec §4.4 + §5.2 disclose this caveat with appropriate scope-bounding language ("matching Cherchi exactly is NOT the same as fixing F0020"). No closure overclaim.
- **`well_formed=true` for cohort.** F0044 and F0045 Cherchi outputs ARE `well_formed=true` (Gate D + Gate E). This is what allows the F0044 Stage B hard gate (`pr_y31_f0044_extras_zero`) to work at all — Cherchi has a clean answer at Stage B. But at the Render LOD layer, `common=0` reveals the analytic-surface re-tessellation mismatch (canary §4.2). Honestly framed.

---

## §8 Verdict — **ACCEPT**

All 9 gates GREEN. INFRA-class framing is faithful (3 files: harness + spec + canary; 0 production code; 0 kernel/wasm-bridge/app code change). Strategic-pivot framing is honestly MIXED. The 50.0% F0020 attribution is at-threshold, framed as borderline-sharp with explicit failure-mode disclosure for PR-Y43→Y44. Cohort method-limit is owned, not buried. Cherchi well_formed=false caveat is documented at three layers (spec §4.4, §5.2, §6.2 + canary §0, §4.4, §6.4).

**This is the 11th investigational PR on F0020 Render LOD with 0 production code.** PR-Y42 ships measurement infrastructure that produced the sharpest empirical signal in 11 cycles. Per `feedback_no_last_bug`, no closure claim is made; F0020 Status:Failed remains at 40 unpaired. Per `feedback_external_coherence`, Cherchi C++ is the load-bearing reference oracle (now applied at the Render LOD layer, not just Stage B). Per `feedback_phase1_diagnosis_ranking_is_inference`, the 50% at-threshold framing acknowledges the metric is direct measurement at the boundary, with ±1 noise floor.

PR-Y43 has a sharp F0020 anchor (the 20-triangle subset bounding F0020's 20 attributable unpaired edges) and a clear pivot-to-Option-C failure mode if the canary refutes the position-to-stage mapping. Cohort cases (F0045/R0092) are explicitly out-of-scope for PR-Y42's method.

Adversary recommends **ACCEPT — INFRA-class strategic pivot; B.1 paid off for F0020; cohort method-limited; honest framing throughout.**

---

## §9 Reproduction artifacts

```
Live tree HEAD:       /home/claude/workspace @ 372bc8e (parent daafbbc)
Cherchi binary:        $HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans (827136 bytes, exec)
Adversary worktree:   /tmp/y42-adv-baseline @ daafbbc (read-only; removed after Gate F)
Destructive ops:       NONE — git show + git worktree add only
```

Gate-replay commands (all run in `/home/claude/workspace`):
```bash
# A
git show 372bc8e --stat
git diff daafbbc..372bc8e -- 'crates/kernel/**' --stat
git diff daafbbc..372bc8e -- 'crates/wasm-bridge/**' --stat
git diff daafbbc..372bc8e -- 'app/**' --stat
# B
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
# C
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_diff_baseline --ignored --nocapture --test-threads=1
# D
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture --test-threads=1
# E
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- cohort_render_lod_diff_baseline --ignored --nocapture --test-threads=1
# F
git worktree add -f /tmp/y42-adv-baseline daafbbc
grep -c "RenderLodDiffCounts\|run_render_lod_diff\|f0020_render_lod_diff_baseline" /tmp/y42-adv-baseline/crates/test-harness/tests/cherchi_differential_diff.rs   # 0
git worktree remove /tmp/y42-adv-baseline --force
# G
cargo test -p kernel --lib
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1
# H
grep -i "closes yang\|last gap\|fixes yang\|status.*passed" specs/yang_pr_y42_render_lod_diff.md docs/audits/pr_y42_canary.md
# I — read spec §6 and §5.2; verify MIXED + borderline + Option-C-failure-mode framing
```
