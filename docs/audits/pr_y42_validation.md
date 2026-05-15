# PR-Y42 — Final Audit Memo

| Field | Value |
|---|---|
| Auditor | audit-y42 |
| Date | 2026-05-15 |
| Live tree HEAD | `372bc8e` (PR-Y42 INFRA, NOT pushed) |
| Parent | `daafbbc` (PR-Y41 audit; 7th-refutation) |
| Class | INFRASTRUCTURE-CLASS (test-harness extension; 0 LOC production logic) |
| Phase artifacts | Spec ✓ · Canary ✓ · Impl ✓ · Adversary ✓ |
| Strategic-pivot ROI | **MIXED — paid off for F0020, method-limited for cohort** |
| Verdict | **ACCEPT — borderline-sharp F0020 anchor; B.1 strategic pivot honestly framed** |

---

## §0 Verdict (single paragraph)

PR-Y42 ships +413 LOC test-file harness extension at `crates/test-harness/tests/cherchi_differential_diff.rs` that extends the PR-Y29/Y30/Y31 Cherchi differential diff harness from the Stage B boundary downstream to the final Render LOD layer (`stage_E_lod=Render.obj`). It is the executed B.1 strategic pivot recommended by PR-Y41 spec §5 / canary §6 / audit §8.1, and the first cycle in the 11-PR F0020 Render LOD arc to introduce an external reference oracle (Cherchi C++) at the Render LOD layer. The canary's load-bearing measurement (F0020: `missing=194, extras=76, common=36`; oracle-attribution `20/40 = 50.0%`) is reproduced **byte-for-byte** by adversary §3 (9/9 gates GREEN) from an independent shell session against the same `372bc8e` HEAD with non-destructive git only (`git worktree add`/`remove --force`). All four FIP §5 phase artifacts exist with role separation across spec-y42 / canary-y42 / impl-y42 / adversary-y42 per the INFRA-CLASS test-author waiver chain (Y29/Y33/Y36/Y37/Y38/Y40/Y41). DoD §1.5 gates GREEN (probe-off byte parity at adversary Gate B/G; kernel lib 1262/24/42 stable; yang_fast 10/157 stable; F0044 `pr_y31_f0044_extras_zero` hard gate preserved). A15.6 compliance intact — the harness is paper-orthogonal measurement infrastructure (Cherchi paper scope ends at the arrangement output; the diff harness measures Waffle's downstream Render LOD vs Cherchi's final mesh). The 50.0% F0020 attribution **exactly meets** the canary's sharp-anchor threshold but is honestly framed as **BORDERLINE-sharp** (not "comfortably above") with explicit Option C failure-mode disclosure at spec §5.2 for PR-Y43→Y44; the cohort `common=0` method-limit and Cherchi `well_formed=false` caveat are documented at three places each across spec + canary. Per `feedback_no_last_bug`, no closure claim is made; F0020 Status:Failed remains at 40 unpaired across 11 cycles with 0 production LOC. Recommend **ACCEPT** + Phase 8 push authorized.

---

## §1 FIP §5 phase-artifact checklist

| Phase | Artifact | Path | Status |
|---|---|---|---|
| 1 — Spec | `yang_pr_y42_render_lod_diff.md` (351 lines) | `specs/yang_pr_y42_render_lod_diff.md` | **GREEN** — harness design (§3), F0020 + cohort empirical findings (§4), borderline-sharp PR-Y43 anchor (§5.1) with explicit Option C failure-mode disclosure (§5.2), MIXED strategic-pivot ROI (§6), banked-not-closed (§7), paper citations (§9: Yang §4.4.1/§4.4.2/§4.5.5, Cherchi 2022 §3) |
| 2 — Canary | `pr_y42_canary.md` (456 lines) | `docs/audits/pr_y42_canary.md` | **GREEN** — Gates 1-8 measured; §3 F0020 50.0% attribution byte-match to production oracle's 40-unpaired count; §4 cohort `common=0` method-limit discovery; §6 BORDERLINE-sharp PR-Y43 anchor; §7 honest MIXED ROI assessment; §9 empirical-confidence table |
| 3 — Tests | INFRA-CLASS waiver (no production logic change) | regression coverage = probe-off byte parity + new `#[ignore]`-gated baseline tests | **GREEN** — DoD §1.5 satisfied via canary Gates 2/7/8 + adversary Gates B/G; new `f0020_render_lod_diff_baseline` + `cohort_render_lod_diff_baseline` ARE the harness self-tests (skip-quietly when `CHERCHI2022_BIN` unset, per PR-Y29 convention) |
| 4 — Impl | Commit `372bc8e` (3 files: cherchi_differential_diff.rs +413 / pr_y42_canary.md +456 / yang_pr_y42_render_lod_diff.md +351; 0 kernel, 0 wasm-bridge, 0 app) | `git show 372bc8e` | **GREEN** — additive test-file-only; production tessellation paths verbatim preserved; no kernel dump-site added (reuses pre-existing `stage_E_lod=Render.obj` at `yang_integration.rs:1063-1074`); `results.json` correctly NOT staged |
| 5 — Adversary | `pr_y42_adversary.md` (185 lines, ACCEPT, 9/9 GREEN) | `docs/audits/pr_y42_adversary.md` | **GREEN** — Gates A-I all PASS on independent re-run against live HEAD `372bc8e`; non-destructive git via `git worktree add -f /tmp/y42-adv-baseline daafbbc` + `git worktree remove --force` cleanup |
| 6 — Audit | This memo | `docs/audits/pr_y42_validation.md` | **GREEN** (this audit) |

INFRA-CLASS test-author waiver is consistent with the precedent chain PR-Y29 / Y33 / Y36 / Y37 / Y38 / Y40 / Y41. Default-off byte parity is the regression coverage; canary Gate 2 + adversary Gate B verify it independently against the live HEAD.

---

## §2 Role separation — 4 distinct agents

| Role | Agent | Artifact ownership |
|---|---|---|
| Canary | `canary-y42` | `docs/audits/pr_y42_canary.md`; worktree harness build + Gates 1-8 measurement; F0020 50.0% attribution + cohort `common=0` discovery |
| Spec | `spec-y42` | `specs/yang_pr_y42_render_lod_diff.md`; harness design + MIXED-ROI framing + Option C failure-mode disclosure at §5.2 |
| Impl | `impl-y42` | Commit `372bc8e` (live tree, branch main); 3 files staged: harness +413 LOC + canary memo + spec; 0 kernel; no WASM rebuild required (per §3.6 of spec, the pre-existing kernel dump site was reused) |
| Adversary | `adversary-y42` | `docs/audits/pr_y42_adversary.md`; independent re-run of Gates A-I via `git worktree add` (non-destructive) + Cherchi C++ subprocess invocation from a fresh shell |

Per `feedback_oracle_credibility_via_role_separation`: canary built the harness and measured `missing=194, common=36, attribution=20/40`; adversary independently re-ran from a parent-commit worktree and reproduced the numbers byte-exactly (adversary §3, including the 5.422077e-6 oracle grid value and the rec[6]=2-edges top-10 outlier). The 50.0% attribution and the `common=0` cohort method-limit are reproducible across role-separated runs from independent shell sessions.

No test-author role per INFRA-CLASS waiver; this is consistent with the precedent chain through Y29/Y33/Y36/Y37/Y38/Y40/Y41.

---

## §3 DoD checklist — probe-off byte parity is load-bearing

| DoD §1.5 item | Status | Evidence |
|---|---|---|
| Pathological / near-tolerance inputs tested | **GREEN** | F0020 (all-planar 3-extrude, degenerate cluster, NMM) + F0044 (analytic-surface subtraction, well_formed=true at Stage B) + F0045 + R0092 cohort (canary §4, adversary §4) |
| Degenerate geometry behavior validated | **GREEN** | F0020 `well_formed=false, χ=5` on Cherchi output independently observed (adversary Gate C); canary §4.4 / spec §4.4 + §5.2 document this caveat with explicit closure-bounding language |
| No NaN values introduced | **GREEN** | Harness uses i64 quantization on f64 coords (1e-6 set-diff grid) and f32 round-trip via `oracle_quantize_waffle_obj` (replicates `oracle.rs::check_watertight_mesh` exactly); no new floating-point arithmetic on production paths |
| No invalid topology produced | **GREEN** | Zero production-code changes; test-harness-only extension. Production tessellation + assembly paths verbatim preserved at `daafbbc`-vs-`372bc8e` diff |
| No regression in existing test suite | **GREEN** | Adversary Gate G: kernel lib **1262 passed; 24 failed; 42 ignored** byte-identical to PR-Y41 baseline; yang_fast **10/157** identical. Adversary Gate D: `pr_y31_f0044_extras_zero` Stage B hard gate (PR-Y31's `assert!(extras == 0)`) still PASSES unchanged |
| Default-off byte parity (load-bearing) | **GREEN** | Adversary Gate B: F0020 spotlight at HEAD `372bc8e` without `YANG_STAGE_DUMP` / harness invocation produces `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degenerate; 10 self-int; stage-f 138→119→119→113→113 unpaired 30→42→39→39→39` — IDENTICAL to PR-Y41 baseline |

Probe-off byte parity is the load-bearing DoD anchor for INFRA-CLASS work. Independent verification by both canary (worktree) and adversary (live HEAD) confirms it. The new `f0020_render_lod_diff_baseline` + `cohort_render_lod_diff_baseline` tests are `#[ignore]`-gated and skip-quietly when `CHERCHI2022_BIN` is unset, per PR-Y29 convention.

---

## §4 Empirical evidence cross-check (canary §3 vs adversary §3)

| Quantity | Canary §3 | Adversary §3 | Cross-check |
|---|---|---|---|
| Cherchi C++ F0020 union triangles | 246 | 246 | **byte-match** |
| Cherchi C++ F0020 vertices | 120 | 120 | **byte-match** |
| Cherchi C++ F0020 `well_formed` | false | false | **byte-match** |
| Cherchi C++ F0020 χ | 5 | 5 | **byte-match** |
| Waffle Render LOD F0020 triangles | 113 | 113 | **byte-match** |
| Waffle Render LOD F0020 vertices | 219 | 219 | **byte-match** |
| Waffle Render LOD F0020 `well_formed` | false | false | **byte-match** |
| Waffle Render LOD F0020 χ | 2 | 2 | **byte-match** |
| Set-diff @ 1e-6 grid: missing | 194 | 194 | **byte-match** |
| Set-diff @ 1e-6 grid: extras | 76 | 76 | **byte-match** |
| Set-diff @ 1e-6 grid: common | 36 | 36 | **byte-match** |
| Oracle grid (max_abs × 1e-5) | 5.422077e-6 | 5.422077e-6 | **byte-match** |
| Waffle Render LOD unpaired edges (oracle grid) | 40 (39 boundary, 1 NMM) | 40 (39 boundary, 1 NMM) | **byte-match** + production-oracle byte-match |
| Cherchi-only missing tris explaining ≥1 unpaired | 42/194 | 42/194 | **byte-match** |
| **F0020 attribution** | **20/40 = 50.0%** | **20/40 = 50.0%** | **byte-match (at threshold)** |
| Top-10 records: rec[0..5] = 1 edge | 1,1,1,1,1,1 | 1,1,1,1,1,1 | **byte-match** |
| Top-10 records: rec[6] = 2 edges (outlier) | 2 | 2 | **byte-match** |
| Top-10 records: rec[7..9] = 1 edge | 1,1,1 | 1,1,1 | **byte-match** |
| F0044 subtraction: Cherchi tris (wf=T) | 136 (true) | 136 (true) | **byte-match** |
| F0044: Waffle tris / common / attr | 116 / **0** / 8/12=66.7% | 116 / **0** / 8/12=66.7% | **byte-match** |
| F0045 union: Cherchi tris (wf=T) | 236 (true) | 236 (true) | **byte-match** |
| F0045: Waffle tris / common / attr | 302 / **0** / 2/38=5.3% | 302 / **0** / 2/38=5.3% | **byte-match** |
| R0092 subtraction: Cherchi tris (wf=F) | 225 (false) | 225 (false) | **byte-match** |
| R0092: Waffle tris / common / attr | 173 / **0** / 0/43=0.0% | 173 / **0** / 0/43=0.0% | **byte-match** |

The load-bearing F0020 attribution (20/40 = 50.0%) and the cohort `common=0` universal finding (3/3 cases) are both **fully reproducible** across canary and adversary independent runs from separate shell sessions. The TBB non-determinism caveat (banked from PR-Y31) did not cause measurable drift on the missing-count (deterministic in our runs); per canary §9 + adversary §7, use missing-count as the load-bearing gate, not extras.

This is the **load-bearing positive value** of PR-Y42: the F0020 50% attribution is the sharpest empirical signal of the entire 11-cycle arc, and the cohort `common=0` is a structurally-reasoned method-limit (analytic surfaces re-tessellate at Render=64 segments while Cherchi keeps the post-Boolean-LOD 16-segment geometry), not a fluke.

---

## §5 A15 compliance

A15.6 (Hybrid B-Rep/Mesh Boolean Pipeline — Yang 2025) is the governing invariant. PR-Y42 ships infrastructure that diffs Waffle's Render LOD output against Cherchi C++'s `mesh_booleans` final output. Compliance assessment:

- **A15.6 paper-orthogonal positioning.** Per spec §9 + canary §2.4, Cherchi 2022 §3's well-formedness guarantee is structural (well-formed simplicial complex, no T-junctions, surface patches bounded by closed non-manifold loops) and conditional on inputs being non-tangentially-touching watertight manifolds. F0020 violates this precondition (3 rectangle extrudes with tangential planar contacts) → Cherchi's own well_formed=false. The Cherchi paper scope ends at the arrangement output; PR-Y42's diff harness measures Waffle's downstream Render LOD vs Cherchi's final mesh — a paper-orthogonal comparison the paper does not itself prescribe. This is the empirically-correct framing per `feedback_external_coherence`: external-reference parity at the Render LOD layer is a Waffle-specific quality probe, not a paper-conformance probe.
- **A15.5 analytical-surface preservation unaffected.** No production logic change; analytic Render-LOD re-tessellation behavior (Render=64 segments per `yang_integration.rs:1024`) verbatim preserved. The cohort `common=0` finding is empirical confirmation that this re-tessellation occurs (the spec §6.2 reasons about it correctly).
- **A15.4 (SSI solvers) sequencing.** SSI solver work independent and unaffected; PR-Y42 operates strictly at the post-arrangement/post-tessellation diff layer.
- **A15.6 Stage 2/Stage B byte parity gate preserved.** Adversary Gate D: `pr_y31_f0044_extras_zero` PASSES unchanged. The PR-Y31 byte-parity hard gate (Cherchi-Rust port byte-matches Cherchi C++ on F0044's first-op `Subtract`) is preserved.

The recommended PR-Y43 anchor (per-stage tri-survival probe for the 20-tri attribution subset) is at Waffle's Render LOD layer — downstream of A15.6's paper-prescribed Yang Stage 1-6 pipeline. Spec §5.1 enumerates the candidate stages (E_lod=Render / F.0 / F.1 / F.2 / F.3 / F.4) that already exist in the production pipeline.

---

## §6 INFRA-CLASS framing audit

| Criterion | Status | Evidence |
|---|---|---|
| Production logic LOC = 0 | **GREEN** | `git diff daafbbc..372bc8e -- 'crates/kernel/**' --stat` empty; `'crates/wasm-bridge/**' --stat` empty; `'app/**' --stat` empty. Only 3 files staged: cherchi_differential_diff.rs (+412/-1 = +411 net), pr_y42_canary.md (+456), yang_pr_y42_render_lod_diff.md (+351) |
| Test-file-only | **GREEN** | All +413 LOC of harness code live in `crates/test-harness/tests/cherchi_differential_diff.rs` (a `#[test] #[ignore]`-gated test file, not a production library) |
| Env-gated default-off | **GREEN** | `f0020_render_lod_diff_baseline` + `cohort_render_lod_diff_baseline` skip-quietly when `CHERCHI2022_BIN` unset; harness enables `YANG_STAGE_DUMP` only within its own scope. Probe-off byte parity verified Gate B + Gate G |
| Additive only | **GREEN** | +1219 insertions / 1 deletion (the 1 deletion is the cleanup-loop length change from 3 → 4 paths in `WaffleDumpPaths`) |
| No kernel dump-site added (reuses existing) | **GREEN** | Canary §1 + spec §3.6: the planned `stage_RENDER.obj` emission at `tessellate_solid_bounded` was found unnecessary because `tessellate_waffle_solid` at `yang_integration.rs:1063-1074` already emits `stage_E_lod=Render.obj` under `YANG_STAGE_DUMP`. md5 round-trip on F0020 confirms byte-identity to `stage_F.4.obj`. **0 kernel LOC required** |
| WASM rebuild NOT needed | **GREEN** | No kernel/wasm-bridge changes → no rebuild required. (Confirmed: app/static/pkg/wasm_bridge_bg.wasm is NOT in the commit's file list) |
| `results.json` correctly NOT staged | **GREEN** | `git status` shows it unstaged; same `spotlight_f0020`-invocation regeneration carry-over pattern as PR-Y38/Y40/Y41 (canary §1.1 + §10.4 disclose; adversary §1 acknowledges) |
| Cumulative probe complexity disclosed | **GREEN** | Spec §1 + §6 + commit body: 11th investigational PR; 10 prior cycles (5 ABORTs Y25-Y28/Y39 + 5 INFRA SHIPs Y36/Y37/Y38/Y40/Y41) produced 0 LOC production fix; 0 movement in F0020 unpaired count (40 → 40 across 11 cycles) |

### §6.1 Borderline-sharp framing audit (critical scrutiny axis)

The 50.0% exact-threshold result is the most scrutiny-worthy axis. Audit checks:

- **Canary §0 verdict line:** "SHIP-INFRA + **BORDERLINE-sharp** PR-Y43 anchor + cohort-method-limit disclosure" — uses BORDERLINE label explicitly, not "comfortably sharp" or "sharp anchor."
- **Canary §3.3:** "Hits the Gate 5 sharp-anchor threshold per plan verdict logic" + "Combined with the cohort `common=0` finding (METHODOLOGICAL DISCOVERY), the appropriate framing is **BORDERLINE-sharp**."
- **Canary §6 header:** "PR-Y43 anchor recommendation — **BORDERLINE-SHARP**" (all-caps, unambiguous).
- **Spec §5 title:** "PR-Y43 anchor recommendation — **BORDERLINE-sharp**" + §5.2 explicit Option C failure-mode trigger.
- **Spec §5.2:** "The 50% number is at the verdict threshold, not comfortably above; the attribution requantization has a ±1 record noise floor."
- **Adversary §6:** "The 50.0% number is framed as borderline. Spec §5.2: 'The 50% number is at the verdict threshold, not comfortably above; the attribution requantization has a ±1 record noise floor.' Canary §3.3 frames it the same way."

The borderline-sharp framing is consistent across spec, canary, adversary, and commit body. The ±1 record noise floor from the two-step requantization (1e-6 set-diff grid → f64 → f32 → oracle 5.4 µm grid; §3.4 of spec) is honestly disclosed in three places. **Per `feedback_phase1_diagnosis_ranking_is_inference` applied at the threshold boundary**: this is direct measurement at the cell boundary, not ranking; the borderline framing acknowledges the cell-boundary noise floor explicitly. Confirmed honest.

### §6.2 Strategic-checkpoint failure-mode disclosure

Spec §5.2 final paragraph (load-bearing): "If PR-Y43's canary CANNOT confirm a position-to-stage mapping for the 20 attributing triangles, OR if the 20 triangles trace exclusively to a class that Cherchi-also-misses (no Waffle-side production fix possible because Cherchi well_formed=false for F0020), **PR-Y44 should pivot to Option C (pause F0020 Render LOD) per PR-Y41 canary §6.4**." Canary §6.3 + §6.4 carry the same disclosure with the additional banked clause: if PR-Y43 closes ≤20 unpaired edges, F0020's final unpaired count would be ~20 (not 0) because the other 20 are not Cherchi-only-attributable.

This is the **explicit pre-baked failure-mode trigger** required by `feedback_no_last_bug` at this scrutiny axis. The 50% attribution is NOT presented as a closure path; it is presented as a sharp-but-bounded measurement with an upstream-stop condition for the next cycle.

INFRA-CLASS framing is intact; borderline-sharp framing is honest; failure-mode is documented.

---

## §7 Strategic context — 11-cycle reckoning; pivot ROI MIXED; PR-Y43 anchor with explicit failure-mode

| PR | Outcome | Cycle role |
|---|---|---|
| Y25 | ABORT (canary) | Yang §4.4.1 mesh-updating refuted as immediate anchor |
| Y26 | ABORT (canary) | Cohort-wide missing-triangle defect; not the 3 plan candidates |
| Y27 | ABORT (canary) | flood_fill_patches drops 0 SourceFaces; D.1 split into 3 sub-mechanisms |
| Y28 | ABORT (canary) | D.1d kids 218/232/233 identified; fix-shape refused commit |
| Y36 | INFRA SHIP | Inverse-probe source-face attribution (downstream) |
| Y37 | INFRA SHIP | H1/H2/H3 classification refined |
| Y38 | INFRA SHIP | Grid-sensitivity oracle gate; phantom-hypothesis refuted |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 with N=16 attribution |
| Y40 | INFRA SHIP — 6th-refutation | PR-Y39 §2.5's N=16 attribution refuted; measured N=4; banked "missing 12 upstream" |
| Y41 | INFRA SHIP — 7th-refutation | PR-Y40 §6's banked "missing 12 upstream" inference refuted; 18 indices EXACT; strategic-pivot trigger fired |
| **Y42** | **INFRA SHIP — B.1 STRATEGIC PIVOT executed** | **First external-oracle measurement at Render LOD layer; F0020 50.0% borderline-sharp; cohort `common=0` method-limit discovered** |

**Cumulative cycle accounting (11 cycles):**
- 5 canary-stage ABORTs (Y25/Y26/Y27/Y28/Y39); 6 INFRA SHIPs (Y36/Y37/Y38/Y40/Y41/Y42); **0 production fix on F0020 Render LOD in 11 cycles**.
- Cumulative probe LOC: ~1358 production-instrumentation (Y36/Y37/Y40/Y41) + ~413 test-harness (Y42) = **~1771 LOC cumulative diagnostic infrastructure**.
- F0020 unpaired count: **40 → 40 across all 11 cycles**.
- **First-ever external-reference oracle at Render LOD layer**: Y42 ships this.

**Pivot ROI verdict — MIXED, per spec §6.2 explicit framing:**

| Dimension | Pre-pivot expectation (PR-Y41 §6) | Post-pivot reality (PR-Y42) |
|---|---|---|
| External oracle gives an empirical answer | "Will reveal which Cherchi tris are missing" | **YES** — 194 missing tris with positions |
| Sharp anchor for PR-Y43 | "Sharp if ≥50% attribution" | **Borderline** (=50.0% EXACTLY at threshold) |
| Method generalizes to cohort | "Will localize cohort defects too" | **NO** — `common=0` for all 3 cohort cases |
| Strategic-pivot LOC cost | "~150-300 LOC" | ~413 LOC harness; 0 kernel |
| Avoided one more refutation cycle | "Either fix-anchor or confident stop" | F0020-only sharp; cohort method-limited |

**Net pivot ROI:** **PAID OFF for F0020 specifically (50% attribution is the sharpest signal of the 11-cycle arc); METHOD-LIMITED at the cohort level (`common=0` universal); CAVEATED by Cherchi well_formed=false on F0020 (matching Cherchi exactly ≠ fixing F0020).** Spec §6.2 frames this as MIXED with the exact words "What actually happened — ROI is MIXED" — no overclaim, no underclaim.

**Per `feedback_external_coherence`:** Cherchi C++ is the load-bearing reference oracle (now applied at Render LOD, not just Stage B). PR-Y42 executes exactly the prescription: "When the algorithm we're porting has a public reference implementation, build differential testing against that reference as the load-bearing oracle." The 50% measurement is the prescription's empirical product.

**Per `feedback_no_last_bug`:** 11th cycle on F0020 Render LOD. Spec §7 carries explicit "**No 'this closes Yang' language anywhere in this PR**" + adversary Gate H confirms by grep (only NEGATIONS appear). F0020 Status:Failed unchanged at 40 unpaired. The 50% attribution is NOT a closure claim; the borderline framing + Option C failure-mode disclosure prevent the threshold-boundary number from being read as one.

**Per `feedback_phase1_diagnosis_ranking_is_inference`:** PR-Y42's 50.0% is direct measurement at the oracle grid (not Phase-1 inference ranking), but the at-threshold framing acknowledges the ±1 cell-boundary noise floor as the appropriate epistemic posture for a measurement at the exact verdict cutoff. The Option C failure-mode for PR-Y44 enforces this discipline.

**Per `feedback_no_regression_chasing`:** Harness is test-file-only; kernel + yang_fast baselines unchanged.

---

## §8 Banked findings (from canary §6 + spec §7 + adversary §7)

1. **PR-Y43 PRIMARY anchor — per-stage tri-survival probe for the 20-tri F0020 attribution subset.** Positions captured in `/tmp/y42-f0020-render-lod.log`; regeneration via `f0020_render_lod_diff_baseline` invocation. PR-Y43's canary must verify position-to-stage mapping for each of E_lod / F.0 / F.1 / F.2 / F.3 / F.4 stages.
2. **PR-Y43 failure-mode → PR-Y44 Option C trigger.** If PR-Y43's canary cannot confirm position-to-stage mapping OR if the 20 triangles trace exclusively to Cherchi-also-misses, PR-Y44 should pivot to Option C (pause F0020 Render LOD) per PR-Y41 canary §6.4. Cohort F0044/F0045/R0092 (D.2/D.3 mechanisms), SSI solvers (A15.4), GUI test coverage, or cross-crate integration tests become the alternative scopes.
3. **F0020 closure ceiling at ~20 unpaired even if PR-Y43 succeeds.** Cherchi well_formed=false for F0020 union means Cherchi itself doesn't fully resolve the input; the other 20 unpaired edges (not Cherchi-only-attributable) may be Waffle-specific defects beyond what the external oracle captures. This is the upper bound on PR-Y42's measurement-victory translating to closure-progress.
4. **Cohort `common=0` is method-limited, not defect-attributable.** The PR-Y42 diff method does NOT generalize beyond F0020-class all-planar workloads. Future analytic-surface Render LOD work for F0044/F0045/R0092 (and the broader 139-failing yang_fast cohort) needs a different methodology (segment-count alignment, positional-tolerance match, or per-case fixtures).
5. **F0044 66.7% attribution is signal-of-proximity, not signal-of-defect.** Cohort common=0 means the 16 attributing Cherchi-only tris are positionally NEAR (within ~5 µm oracle grid) but not AT Waffle's triangle positions. This is structurally a proximity finding (analytic-surface re-tessellation), not a defect finding.
6. **Cherchi C++ TBB non-determinism (PR-Y31 banked, unchanged).** Persists under `TBB_NUM_THREADS=1` in some F0020 reruns; mitigation per canary §9 + adversary §7 is to use missing-count (deterministic in our runs) as the load-bearing gate, not extras.
7. **F0020-specific fully-degenerate-tri signal at dispatch (PR-Y41 banked: kids 235=7/256=4/198=1/231=1; 13 total; 3 survive to final 8-of-113 degen count).** Cohort F0044/F0045/R0045/R0092 has 0 fully-degen across ~21k cohort tris. Banked as F0020-Render-LOD investigation thread; connection to 40-unpaired-edge defect unproven.
8. **F0045 / R0092 retess-pass 13K-collision outliers (PR-Y40 §4.3, unchanged).** Different defect class (fully-degenerate Render-LOD quantization on huge planar faces). Banked.
9. **139 / 157 yang_fast failures.** Unchanged. Adversary Gate G confirms 10/157 baseline.
10. **Yang §4.4.1 mesh-updating (Diagnosis B from PR-Y25).** Long-term load-bearing layer; banked.
11. **Cumulative diagnostic scaffold ~1771 LOC across 6 INFRA SHIPs.** Maintenance debt acknowledged at spec §6.2 + commit body. PR-Y42 adds test-file scaffold (durable harness extension) rather than production-instrumentation; the harness is preserved as durable reference infrastructure regardless of PR-Y43's outcome.
12. **Cherchi well_formed varies by case.** F0044 and F0045 Cherchi outputs ARE well_formed=true; F0020 and R0092 are well_formed=false. The PR-Y31 byte-parity hard gate works on F0044 specifically because Cherchi has a clean answer at Stage B for that case. PR-Y42 confirms this varies.

All twelve items are correctly enumerated by canary + spec + adversary; this audit confirms they are appropriately scoped as banked-not-blocking.

---

## §9 Final recommendation

**ACCEPT — borderline-sharp F0020 anchor; B.1 strategic pivot honestly framed as MIXED ROI.**

Rationale:
- **FIP §5 GREEN** — 4-phase artifact chain complete with role separation across 4 distinct agents (spec-y42 / canary-y42 / impl-y42 / adversary-y42). INFRA-CLASS test-author waiver consistent with Y29/Y33/Y36/Y37/Y38/Y40/Y41 precedent.
- **DoD §1.5 GREEN** — probe-off byte parity load-bearing; verified independently by canary Gate 2 + adversary Gate B against live HEAD `372bc8e`. Stage B PR-Y31 hard gate (`pr_y31_f0044_extras_zero`) preserved (adversary Gate D).
- **INFRA-CLASS framing intact** — 0 LOC production logic; 0 kernel; 0 wasm-bridge; 0 app; only 3 files staged (harness +413 LOC + canary +456 LOC + spec +351 LOC). No WASM rebuild required (pre-existing kernel dump site reused).
- **A15.6 compliant** — paper-orthogonal Render LOD diff harness (Cherchi paper scope ends at arrangement output); A15.4/A15.5 unaffected; A15.6 Stage B byte-parity gate preserved.
- **Empirical evidence load-bearing** — F0020 50.0% attribution byte-matches across canary §3 + adversary §3 (24/24 measurements byte-identical, including the rec[6]=2 top-10 outlier and the 5.422077e-6 oracle grid value). Cohort `common=0` reproducible across 3/3 cases.
- **No-last-bug discipline GREEN** — adversary Gate H grep confirms only NEGATIONS appear in spec + canary (no "closes yang" / "last gap" / "fixes yang"); F0020 Status:Failed unchanged at 40 unpaired.
- **Borderline-sharp framing intact** — 50.0% at threshold is consistently labeled "BORDERLINE-sharp" / "at the threshold boundary, NOT comfortably above" across canary §0/§3.3/§6, spec §5/§6.2, adversary §6. ±1 cell-boundary noise floor disclosed in three places. **Per `feedback_phase1_diagnosis_ranking_is_inference` applied at the threshold boundary**: this is direct measurement at the cell boundary, not ranking; the borderline framing acknowledges the noise floor explicitly.
- **Strategic-pivot ROI honestly framed as MIXED** — spec §6 + canary §7 + commit body L66-77 explicitly use the words "ROI is MIXED" / "paid off for F0020 specifically … method-limited at the cohort/corpus level" / "BORDERLINE sharp + method-limited-to-F0020-class". No overclaim of "pivot succeeded." No underclaim of "pivot failed." The 11-cycle 0-production-LOC reality is named in spec §1, commit body L46-49, and §6.2.
- **PR-Y43 anchor with explicit failure-mode** — spec §5.2 + canary §6.4 enumerate the position-to-stage mapping investigation AND the Option C trigger for PR-Y44 if the mapping cannot be confirmed. This is the disciplined `feedback_no_last_bug` posture at a threshold-boundary measurement.

**Strategic-pivot ROI acknowledgment.** PR-Y42 executes the B.1 pivot recommended by PR-Y41 spec §5 / canary §6 / audit §8.1, and the outcome is honestly MIXED: the external oracle paid off for F0020 (50% sharp-but-borderline; 194 missing tris with positions), revealed a structural method-limit at the cohort level (`common=0` universal across 3/3 cohort cases with analytic surfaces), and bounded any potential closure-progress by Cherchi's own well_formed=false on F0020. This is the disciplined outcome the strategic-pivot was scoped to produce — either a sharp anchor or a confident stop signal — and PR-Y42 delivered the former with a clear pivot-to-Option-C trigger for PR-Y44 if PR-Y43 cannot translate measurement into production fix. The 11th cycle does NOT close Yang and is not framed as doing so.

**Phase 8 push authorized.** Recommend:
1. Commit this audit memo + adversary memo (`audit(yang-pr-y42): ACCEPT — borderline-sharp F0020 anchor; B.1 strategic pivot MIXED ROI honestly framed | INFRA-ONLY`).
2. Push origin main (plain push only per `feedback_always_push`; never force).
3. Memory update: `yang_pr_y42_shipped.md` + MEMORY.md one-liner noting INFRA-CLASS, B.1 strategic pivot executed, F0020 50.0% borderline-sharp attribution, cohort `common=0` method-limit, PR-Y43 PRIMARY anchor banked, PR-Y44 Option C trigger conditional on PR-Y43 outcome.
4. `TeamDelete pr-y42` per `feedback_per_plan_cycle_team`.

The cycle does NOT close Yang. PR-Y43 scoping should treat the 20-tri attribution subset as a banked PRIMARY anchor pending in-situ canary verification of the position-to-stage mapping; if that canary refutes (which is one possible outcome at a 50.0% threshold-boundary attribution), PR-Y44 should pivot to Option C (pause F0020 Render LOD) per spec §5.2 / canary §6.4. The harness scaffold is durable reference infrastructure preserved regardless.
