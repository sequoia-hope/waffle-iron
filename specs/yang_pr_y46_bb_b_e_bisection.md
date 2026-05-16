# PR-Y46 — Stage Bb→B→E bisection probe (INFRA-CLASS; Layer-B-dominant at 24/24 = 100.0%)

| Field | Value |
|---|---|
| **Verdict** | SHIP-INFRA + **Layer-B-dominant at 24/24 = 100.0%** (γ Render-LOD re-tessellation ≥ 80% threshold ⇒ γ retess IS the load-bearing F0020 Case-D anchor; `face_survival_detect` is **NOT** the anchor) |
| **Class** | INFRASTRUCTURE-CLASS (test-harness probe extension; 0 production logic touched) |
| **Parent commit** | `c0c2019` (PR-Y45 audit ACCEPT; 2026-05-15) |
| **Date** | 2026-05-15 |
| **Authors** | spec-y46 (this file); canary-y46 (`docs/audits/pr_y46_canary.md`) |
| **LOC** | +289 in `crates/test-harness/tests/cherchi_differential_diff.rs` (1652 → 1943; all additive, `#[ignore]`-gated; one new test fn + two helper fns); 0 kernel; 0 wasm-bridge; 0 app |
| **Production-code delta on F0020** | **0** (unchanged after 15 cycles) |
| **15th investigational PR; 11th INFRA SHIP in F0020 Render-LOD arc** | **YES** |
| **First positive-measurement next-cycle anchor in 15 cycles** | **YES — PR-Y47 anchor at γ retess has 24/24 = 100% direct measurement, not inference-from-refutation** |
| **F0020 Status:Failed** | unchanged — 40 unpaired edges (39 boundary, 1 NMM); PR-Y46 changes none |
| **F0020 Layer A vs Layer B attribution** | Layer A (`face_survival_detect`, `Bb \ B`) = **0 / 24 = 0.0%**; Layer B (γ Render-LOD retess, `B \ E`) = **24 / 24 = 100.0%**; NEITHER = 0; PRESENT_AT_E = 0 — byte-stable across 3 reruns |

---

## §1 Motivation

PR-Y46 is the **15th investigational PR on F0020 Render LOD** and the **second consecutive measurement-first canary at an audit-recommended anchor that is empirically refuted**. Audit-y45 §4.1 (`docs/audits/pr_y45_validation.md:66`) prescribed PR-Y46's anchor as:

> **PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868`** — the Stage 3 selective-retention layer driving the post-arrangement → post-survival drop (`[yang-diag] after survival: 20 groups, 246 tris` followed downstream by `[stage-f] sub=0 tri_count=138`, a cumulative ~108-tri drop spanning face_survival_detect + Boolean LOD → Render LOD re-tessellation). Paper anchor: Cherchi 2022 §5 manifold-flood + inside/outside classification (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) + Yang 2025 §4.4.1 mesh-updating selective-retention (`refs/text/yang2025_hybrid_boolean.txt:548-590`). **Status: PLAUSIBLE-BUT-NOT-CONFIRMED.**

Audit-y45 §4.2 explicitly flagged the prescription as requiring canary-before-fix because the 108-tri drop is **cumulative across two layers** (face_survival_detect AND γ Render-LOD retess), not face_survival_detect alone. Adversary-y45 §8 reinforced "plausible-but-not-confirmed" as load-bearing.

Per `feedback_anchor_before_fix` + `feedback_phase1_diagnosis_ranking_is_inference`: PR-Y46 instruments BOTH layers via a three-stage bisection (Bb → B → E) before any fix-shape commit.

**The empirical question PR-Y46 measures (LOAD-BEARING):** Of the 24 F0020 Case-D (a)-sub-class triangle positions (preserved in PR-Y44's per-tri table at canary-y44 §4.1), how many drop at Layer A (`Bb \ B`, `face_survival_detect`) versus Layer B (`B \ E`, γ Render-LOD re-tessellation in `tessellate_waffle_solid` at `yang_integration.rs:1024`)?

**Empirical answer:**

- **Layer A (`face_survival_detect`, Bb\B drops): 0 / 24 = 0.0%** — REFUTED as anchor.
- **Layer B (γ Render-LOD retess, B\E drops): 24 / 24 = 100.0%** — confirmed as anchor.
- **NEITHER (defect upstream/elsewhere): 0 / 24 = 0.0%**.
- **PRESENT_AT_E (anomaly): 0** (no Case-D position survives to the final Render LOD mesh).

Byte-stable across 3 reruns: 2 independent stage-dump generations + 1 probe-replay against fixed dumps. Per-tri table uniform: every Case-D position d[0]–d[23] presents as `inBb=1, inB=1, inE=0 → Layer B`.

**The audit-y45 §4.1 prescription that `face_survival_detect` IS the PR-Y46 anchor is empirically REFUTED.** This is the SECOND consecutive audit-recommended anchor to fail the measurement-first gate (PR-Y44 audit-y44 §3.4 prescribed α at F.0; PR-Y45 measured 0/24 and refuted). The pattern reinforces `feedback_phase1_diagnosis_ranking_is_inference`: audit-recommended anchors are inference; canary measurement is truth. **For the first time in 15 cycles**, the next-cycle (PR-Y47) anchor recommendation has direct POSITIVE measurement evidence (24/24 = 100% at γ retess) rather than inference-from-refutation.

---

## §2 Methodology

### §2.1 Why infrastructure-class

- **0 production logic touched.** The +289 LOC in `crates/test-harness/tests/cherchi_differential_diff.rs` (1652 → 1943) are strictly additive: one new `#[ignore]` test fn `f0020_stage_bb_b_e_bisection` and two helper fns (`load_case_d_positions_file`, `load_obj_canonical_tri_set`). Existing test fns + helpers (PR-Y43/Y44/Y45) are unmodified.
- **Default-off byte parity preserved by construction.** The new test fn is `#[ignore]`-gated; cargo's default test runner skips ignored tests unless explicitly invoked. The probe consumes pre-existing `YANG_STAGE_DUMP=<dir> + YANG_CONFORMAL_PROBE=1` OBJ outputs (PR-Y14a Stage Bb + Stage B at `topology_extract.rs:2396,2568`; PR-VIZ-1 Stage E_lod=Render at `yang_integration.rs:1063-1074`) — those dump sites are unmodified. Gate 2 verifies F0020 spotlight byte-identical to PR-Y45 baseline.
- **Reuses PR-Y14a + PR-VIZ-1 + PR-Y44 infrastructure.** Probe consumes:
  - Three stage dumps (Bb, B, E_lod=Render) generated by pre-existing `YANG_STAGE_DUMP` instrumentation.
  - The 24-entry Case-D position set extracted from PR-Y44's `f0020_render_lod_nearest_attribution` per-tri 4-tuple table (re-used PR-Y45 §3.1 extraction; byte-match at d[16] cross-check).
  - The PR-Y30/Y43/Y44/Y45 `quantize_tri` canonical-key function at `cherchi_differential_diff.rs:175-183` (1e-6 oracle grid; sorted i64 3-tuple of i64 3-tuples; winding-insensitive).

### §2.2 Set-difference bisection methodology

Per `feedback_multi_stage_anchor_probe` + `feedback_anchor_before_fix`: instrument BOTH suspected layers before writing fix code. The Y46 probe is a **three-stage set-difference bisection** of the 24 Case-D positions:

| Step | Description |
|---|---|
| 1 | Generate 3 stage-dump OBJs (`stage_Bb.obj`, `stage_B.obj`, `stage_E_lod=Render.obj`) by running `spotlight_f0020` with `YANG_STAGE_DUMP=/tmp/y46-stages-f0020 YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1`. Counts (f-count): Bb=420, B=246, E=113. |
| 2 | Quantize all triangles to canonical-tri form at 1e-6 oracle grid (`quantize_tri`); winding-insensitive sort. Unique canonical-tri set sizes: Bb=401, B=230, E=112. |
| 3 | Load 24 Case-D positions from `/tmp/y46-f0020-case-d-positions.txt` (PR-Y45 §3.1 format: 9 i64 / line; sort each canonical key). |
| 4 | Compute set differences: `layer_a_losers = stage_bb_set \ stage_b_set` (face_survival_detect drops); `layer_b_losers = stage_b_set \ stage_e_set` (γ retess drops); `layer_e_survivors = stage_bb_set ∩ stage_e_set`. |
| 5 | For each of 24 Case-D positions, compute `(in_bb, in_b, in_e)` membership and assign layer label `{A | B | A+B | NEITHER | PRESENT_AT_E}`. Emit per-tri row + per-layer aggregate. |
| 6 | Sanity assertion (informational, non-panic): `\|Bb\| - \|union(layer_a_losers, layer_b_losers, layer_e_survivors)\| = 0` — partition must be monotone-decreasing per `feedback_validate_against_corpus`. |
| 7 | Decision gate (per plan §Phase 2c "Verdict logic"): Layer-A-dominant ≥ 80% → A confirmed; Layer-B-dominant ≥ 80% → B confirmed; Mixed (both ≥ 30%) → both load-bearing; Neither (both ≤ 20%) → upstream/elsewhere. |

The probe is intentionally extensible to any future drop-layer bisection: change the source OBJ paths (or the position file) and the same partition + decision-gate applies. PR-Y47+ sub-bisections at γ retess's F.0–F.4 sub-stages can reuse this scaffold.

### §2.3 Why bisection, not single-layer canary

Per audit-y45 §4.2 Q1 (`docs/audits/pr_y45_validation.md:75`): "the '108-tri drop' is the cumulative effect of BOTH layers, not face_survival_detect alone." A single-layer canary at face_survival_detect cannot distinguish "face_survival_detect drops the 24" from "face_survival_detect drops some other 108 triangles, γ retess drops the 24". The bisection at Bb→B→E partitions the cumulative drop into its two component layers and assigns each Case-D position to the layer that drops it. This applies `feedback_multi_stage_anchor_probe` cleanly: probe pre/mid/post the suspected source layer, not just one stage.

The 15-cycle accounting after PR-Y46:

| PR | Outcome | Cycle role |
|---|---|---|
| Y25-Y28 | ABORT (canary) ×4 | Wrong fix shapes caught at canary; D.1 split into 4 sub-mechanisms |
| Y36-Y38 | INFRA SHIP ×3 | Source-face attribution / H1-H3 / grid-sensitivity oracle |
| Y39 | ABORT (canary) | F.1→F.2 anchor refuted; banked F.0→F.1 N=16 |
| Y40 | INFRA SHIP — 6th-refutation | N=16 refuted; measured N=4 |
| Y41 | INFRA SHIP — 7th-refutation | "Missing 12 upstream" refuted; strategic-pivot trigger |
| Y42 | INFRA SHIP — B.1 STRATEGIC PIVOT | First external-oracle measurement at Render LOD |
| Y43 | INFRA SHIP — D-dominant + Case C=0 | F0020 90% accountable |
| Y44 | INFRA SHIP — (a)-DOMINANT at 100% | Anchor MEASURED at sub-class level |
| Y45 | INFRA SHIP — α-REFUTED at 0/24 | First production-fix ATTEMPT; ABORTed at canary |
| **Y46** | **INFRA SHIP — face_survival_detect REFUTED at 0/24; γ retess CONFIRMED at 24/24** | **15th investigational PR; 11th INFRA SHIP; first POSITIVE-measurement next-cycle anchor in 15 cycles** |

---

## §3 Probe extension surface

All changes live in `crates/test-harness/tests/cherchi_differential_diff.rs` (1652 → 1943 lines; **+289 LOC** strictly additive). The PR-Y43/Y44/Y45 test-fn surface is unmodified; PR-Y46 appends one new `#[ignore]` test fn + two helpers at end-of-file (lines 1655–1943).

### §3.1 Helper: load Case-D positions file (≈27 LOC)

```rust
fn load_case_d_positions_file(path: &Path) -> Vec<[(i64, i64, i64); 3]> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Y46: cannot read Case D positions file {:?}: {}", path, e));
    let mut out: Vec<[(i64, i64, i64); 3]> = Vec::new();
    for (line_no, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() != 9 {
            panic!("Y46: line {} in {:?}: expected 9 coords, got {}",
                   line_no + 1, path, tokens.len());
        }
        let mut parts = [0i64; 9];
        for (i, t) in tokens.iter().enumerate() {
            parts[i] = t.parse::<i64>().unwrap_or_else(|e|
                panic!("Y46: parse error at line {} token {}: {}", line_no + 1, i, e));
        }
        let mut tri = [
            (parts[0], parts[1], parts[2]),
            (parts[3], parts[4], parts[5]),
            (parts[6], parts[7], parts[8]),
        ];
        tri.sort();
        out.push(tri);
    }
    out
}
```

File format: 9 whitespace-separated i64 coords per line at the 1e-6 grid (`qa.x qa.y qa.z qb.x qb.y qb.z qc.x qc.y qc.z`); comment lines prefixed with `#`. Each canonical key is sorted into winding-insensitive form before insertion — matches `quantize_tri` at `cherchi_differential_diff.rs:175-183` (the PR-Y30 canonical-key function).

### §3.2 Helper: load OBJ as canonical-tri set (≈9 LOC)

```rust
fn load_obj_canonical_tri_set(path: &Path) -> HashSet<[(i64, i64, i64); 3]> {
    let (verts, tris) = parse_obj(path)
        .unwrap_or_else(|e| panic!("Y46: parse_obj({:?}) failed: {}", path, e));
    let mut out: HashSet<[(i64, i64, i64); 3]> = HashSet::new();
    for tri in &tris {
        out.insert(quantize_tri(&verts, *tri));
    }
    out
}
```

Reuses `parse_obj` + `quantize_tri` (pre-existing at `cherchi_differential_diff.rs:94-159, 175-183`). Position-quantized → sorted → set-inserted. The HashSet dedupes coincident canonical keys (winding-insensitive duplicates collapse).

### §3.3 Probe test fn — `f0020_stage_bb_b_e_bisection` (≈253 LOC)

```rust
#[test]
#[ignore]
fn f0020_stage_bb_b_e_bisection() {
    // 1. Env-var driven paths (mirror PR-Y45 pattern)
    let stage_dir = std::env::var("Y46_BISECTION_STAGE_DIR")
        .unwrap_or_else(|_| "/tmp/y46-stages-f0020/F0020".to_string());
    let case_d_path = std::env::var("Y46_CASE_D_POS")
        .unwrap_or_else(|_| "/tmp/y46-f0020-case-d-positions.txt".to_string());

    // 2. SKIP cleanly if dumps missing (with diagnostic instructions)
    // 3. Load 3 stage dumps as canonical-tri HashSet
    // 4. Load 24 Case D positions as Vec<canonical-tri> + dedupe to set
    // 5. layer_a_losers = stage_bb_set \ stage_b_set
    //    layer_b_losers = stage_b_set \ stage_e_set
    //    layer_e_survivors = stage_bb_set ∩ stage_e_set
    // 6. Sanity: |Bb| - |union(A_losers, B_losers, E_survivors)| = 0
    //    Informational: |E \ Bb| ADDED post-Bb (γ retess re-sample evidence)
    //    Informational: |B \ Bb| ADDED post-survival (expect 0 — face_survival_detect is selective-only)
    // 7. For each Case D position:
    //      - (in_a, in_b, in_e) → layer assignment {A | B | A+B | NEITHER | PRESENT_AT_E}
    //      - emit per-tri row
    // 8. Emit summary + decision-gate verdict per plan §Phase 2c
}
```

Default-off via `#[ignore]`. Probe SKIPs cleanly with a diagnostic if files missing — emits the exact `YANG_STAGE_DUMP` + `f0020_render_lod_nearest_attribution` commands to regenerate.

### §3.4 Determinism + parity preservation

- Probe is a NEW `#[ignore]` test fn appended at file end (line 1655+); no existing test fn modified. Cargo's default test runner skips ignored tests.
- Probe consumes pre-existing PR-Y14a Stage Bb dump (`topology_extract.rs:2396`) + Stage B dump (`topology_extract.rs:2568`) + PR-VIZ-1 Stage E_lod=Render dump (`yang_integration.rs:1063-1074`). None of those sites modified.
- Default-off env-gating (`YANG_STAGE_DUMP=<dir>` + `YANG_CONFORMAL_PROBE=1`) is unchanged — probe-off path byte-identical to PR-Y45 baseline.
- Canonical-key construction matches `quantize_tri` byte-exact: same 1e-6 grid (`(f64 * 1e6).round() as i64`), same sort, same HashSet semantics. No re-implementation of quantization.

---

## §4 Contracts

| Contract | Verification |
|---|---|
| Default-off byte parity (probe-off path byte-identical to PR-Y45 HEAD `c0c2019`) | Gate 2 — F0020 spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` byte-identical pre- and post-probe-add. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical. |
| PR-Y14a + PR-VIZ-1 stage-dump sites unchanged | The Y46 probe consumes pre-existing dump outputs; it does NOT add new dump sites or modify existing ones at `topology_extract.rs:2396` (Stage Bb), `topology_extract.rs:2568` (Stage B), or `yang_integration.rs:1063-1074` (Stage E_lod=Render). |
| PR-Y43+Y44+Y45 baselines preserved | `f0020_render_lod_nearest_attribution` produces 4/14/0/24 (42-mode) byte-identical to PR-Y44 canary §4.1 / PR-Y45 canary §3.1. Case D sub-class (a)=100% byte-identical. PR-Y45 α-attribution unchanged (Y45 probe lives in `crates/kernel/src/tessellation/repair.rs`; Y46 in `crates/test-harness/tests/cherchi_differential_diff.rs`; the two are non-overlapping). |
| Cohort safety | The probe is F0020-targeted by file path (`Y46_BISECTION_STAGE_DIR`, `Y46_CASE_D_POS`), not by case_id. Vacuously green since no production change shipped — cohort byte-identical. |
| Partition invariant (decision-gate methodology) | Per `feedback_validate_against_corpus`: `\|Bb\| - \|union(layer_a_losers, layer_b_losers, layer_e_survivors)\| = 0` asserted (informational; canary §5.3 verified 0 stragglers across all 3 reruns). |
| PR-Y31 hard gate preserved | `pr_y31_f0044_extras_zero` continues to pass byte-clean (F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4). |
| Cohort skip-quietly | If `Y46_BISECTION_STAGE_DIR` is unset/missing OR `Y46_CASE_D_POS` is unset/missing, probe emits diagnostic + skips cleanly; no panic, no side effect on default-off invocation. |

---

## §5 Gates

Eight gates, mirrors canary memo §6:

| Gate | Description | Pass criterion | Result |
|---|---|---|---|
| **1** | Build clean | `cargo build -p test-harness --test cherchi_differential_diff`; no new warnings beyond pre-existing baseline. (Pre-existing build error in `pr13_trim_loop_diagnostic.rs` is unrelated and does not affect the target test binary.) | **GREEN** |
| **2** | **F0020 probe-off byte parity (CRITICAL)** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` byte-identical to PR-Y45 baseline. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical. | **GREEN** |
| **3** | PR-Y43+Y44+Y45 baselines preserved (probe-off) | A/B/C/D = 4/14/0/24 (42-mode) byte-identical to PR-Y44 canary §4.1; Case D sub-class (a)=100%; PR-Y45 α-attribution 0/24 across 6 α invocations byte-identical (Y45 probe is independent of Y46). | **GREEN** |
| **4** | Stage dumps generated | `/tmp/y46-stages-f0020/F0020/{stage_Bb.obj=420f, stage_B.obj=246f, stage_E_lod=Render.obj=113f}` produced. Counts byte-stable across 2 independent pipeline reruns + 1 probe-replay. | **GREEN** |
| **5** | **Layer A vs Layer B attribution (LOAD-BEARING)** | Set-diff bisection of 24 Case-D positions across (Bb \ B, B \ E, Bb ∩ E); per-layer aggregate + decision-gate fires. | **GREEN — Layer A = 0 / 24 = 0.0%; Layer B = 24 / 24 = 100.0%; NEITHER = 0; PRESENT_AT_E = 0; 3 reruns byte-identical; decision-gate fires Layer-B-dominant.** |
| **6** | Per-tri Case-D layer assignment table | 24 rows emitted; each row reports `inBb, inB, inE, layer ∈ {A, B, A+B, NEITHER, PRESENT_AT_E}`. | **GREEN** — all 24 rows: `inBb=1 inB=1 inE=0 → Layer B`. Zero entries in Layer A; zero NEITHER; zero PRESENT_AT_E. |
| **7a / 7b** | kernel lib + yang_fast regression | `cargo test -p kernel --lib`: **1262 passed / 24 failed / 42 ignored** IDENTICAL to PR-Y45 baseline. `YANG_BOOLEAN=1 yang_fast`: **10/157 passed** IDENTICAL. | **GREEN** |
| **8** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4. | **GREEN** |

**8/8 gates GREEN.** Gate 2 is the critical INFRA-class contract; Gate 5 is the load-bearing measurement that fires **Layer-B-dominant at 100.0%**.

---

## §6 Outcome — **SHIP-INFRA + Layer-B-dominant at 24/24 = 100.0%**

### §6.1 Verdict (resolved measurement)

**γ Render-LOD re-tessellation IS the load-bearing F0020 Case-D anchor.** `face_survival_detect` is empirically refuted as the anchor.

Per-rerun summary (3 reruns, byte-identical; canary §5.2):

```
[pr-y46] |Bb \ B| Layer A losers (face_survival_detect)   = 171
[pr-y46] |B \ E|  Layer B losers (γ Render-LOD retess)     = 194
[pr-y46] |Bb ∩ E| Survivors all-the-way                     = 41
[pr-y46] SUMMARY: Layer A (face_survival_detect) = 0 / 24 = 0.0%
[pr-y46] SUMMARY: Layer B (γ Render-LOD retess)   = 24 / 24 = 100.0%
[pr-y46] SUMMARY: NEITHER (defect upstream/elsewhere) = 0 / 24 = 0.0%
[pr-y46] SUMMARY: PRESENT_AT_E (anomaly) = 0
[pr-y46] VERDICT: Layer-B-dominant (≥80%) → PR-Y47 anchor = γ Render-LOD retess
```

Per-tri table (canary §5.4): every Case-D position d[0]–d[23] reports `inBb=1, inB=1, inE=0 → Layer B`. Uniform 24/24 attribution; zero ambiguity.

### §6.2 Sanity findings (LOAD-BEARING for mechanism interpretation)

Per canary §4.4:

```
[pr-y46] SANITY: |Bb| - |union(A_losers, B_losers, E_survivors)| = 0 (monotone-decreasing partition; 0 stragglers)
[pr-y46] SANITY: |E \ Bb| = 71 (γ retess GENERATES 71 NEW canonical tris not present in Stage Bb)
[pr-y46] SANITY: |B \ Bb| = 0 (face_survival_detect adds 0 tris; selective-only as Yang §3.3 + Cherchi 2022 §5 specify)
```

**Interpretation:**

- **`|B \ Bb| = 0`** confirms `face_survival_detect` is monotone-selective: every Stage-B tri is in Stage Bb. Matches Yang 2025 §3.3 + Cherchi 2022 §5 `removeDuplicateAndDegenerateTriangles` + manifold-flood selective-retention semantics (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`).
- **`|E \ Bb| = 71`** is the load-bearing sanity finding: 71 of Stage E's 112 unique canonical tris (≈63%) are FRESH triangles NOT present at Stage Bb. γ retess is therefore a **REPLACE-AND-ADD** layer, not a DROP-only layer. The 16-seg Boolean LOD → 64-seg Render LOD re-sample on curved faces emits new vertices and produces a new triangulation per B-Rep face; the resulting per-tri canonical keys are different from the Stage-B post-survival tri keys. **γ retess REPLACES the 24 Case-D triangles' triangulation; it does not just drop them.**

This is consistent with the PR-Y44 (a)-sub-class (m1x=3, m5x=3) signature: the 24 Case-D triangles' VERTEX positions are present at Render LOD (m1x=3), but the TRIANGLE-LEVEL identity (the 3-vert triple) is different. γ retess's per-face independent re-tessellation reassembles the same vertex cloud into a different set of triangles.

### §6.3 What this canary explicitly refutes

- **`face_survival_detect` as the F0020 Case-D anchor.** 0/24 = 0.0% intersection with `Bb \ B`. The audit-y45 §4.1 PR-Y46 anchor prescription is empirically refuted at substance. `face_survival_detect` KEEPS all 24 Case-D positions; it does NOT drop them.
- **The "108-tri cumulative drop entirely at face_survival_detect" framing.** Per audit-y45 §4.2 Q1, the 108-tri drop is cumulative across BOTH layers. PR-Y46 measures the partition: Layer A (face_survival_detect) drops 171 unique canonical tris (Bb 401 → B 230 = 171); Layer B (γ retess) drops 194 (B 230 → E 112 in the survivors-set sense, with 71 new tris ADDED). The cumulative defect on Case-D is 100% Layer B.
- **`face_survival_detect` as a place to bank fix-shape work for PR-Y47.** It is not. Per `feedback_anchor_before_fix`: do not commit fix-shape on a refuted anchor. PR-Y47's anchor MUST be γ retess, not face_survival_detect.

### §6.4 What this canary explicitly accepts

- **The Case-D position set is byte-stable.** Re-extracted from PR-Y44's per-tri 4-tuple table; d[16] cross-check byte-match with PR-Y45 §3.4: `142179 -122161 -80083 156339 -119712 -121783 204678 -111355 -115049`.
- **The stage dumps are byte-stable.** 3 reruns produced identical Stage Bb (420f) / Stage B (246f) / Stage E_lod=Render (113f) OBJ outputs at 42-mode under `TBB_NUM_THREADS=1`. 47-mode not observed in this 3-run characterization (consistent with PR-Y45 §4.5).
- **The decision-gate discipline.** Per `feedback_anchor_before_fix` + `feedback_phase1_diagnosis_ranking_is_inference`: measurement-first, fix-shape-commit second. The bisection at Bb→B→E partitioned the cumulative drop and assigned 100% of the Case-D defect to Layer B. PR-Y47 has the FIRST positive-measurement anchor in 15 cycles.
- **The mechanism inference at PR-Y44 (m1x=3 ⇒ vertex-survival) IS empirically corroborated.** Per §6.2, the 24 verts survive into Stage E (`|E \ Bb| = 71` includes the Case-D verts in fresh-retess triangulations); the triangle-level identity is what diverges. The audit-y44 step-1 inference holds; audit-y44 step-2 (which-layer-drops) is now empirically resolved at γ retess.

### §6.5 What the verdict explicitly does NOT promise

- **PR-Y46 closes F0020.** It does not. F0020 unpaired count remains at 40 across all 15 cycles.
- **PR-Y47 will close F0020.** Per `feedback_no_last_bug`, even SHIP-FIX cycles do not close F0020 in one step (the closure ceiling is ~20 unpaired per PR-Y42 §6 Cherchi well_formed=false caveat).
- **γ retess is fully attributable for F0020's failure.** The 24 Case-D positions are 100% Layer B, but F0020 has 152 OTHER missing tris (PR-Y43/Y44 only classified the 42 bordering unpaired edges). The wider missing-set may have a different layer attribution; PR-Y47 sub-bisection should also probe the wider set.
- **The γ retess anchor will pass its own canary.** Per the recursive Y45/Y46 discipline pattern, PR-Y47 MUST sub-bisect γ retess's F.0/F.1/F.2/F.3/F.4 internal stages and probe per-B-Rep-face attribution BEFORE committing fix shape. If the sub-bisection shows the 24 Case-D drops are upstream of γ retess (e.g., at B-Rep assembly or `flood_fill_patches`), the anchor pivots again.

---

## §7 Rollback

PR-Y46 is INFRA-only with all changes confined to `crates/test-harness/tests/cherchi_differential_diff.rs`. Revert procedure if the Y46 probe ever regresses default-off behavior or breaks PR-Y45 baselines:

```bash
git checkout c0c2019 -- crates/test-harness/tests/cherchi_differential_diff.rs
# (c0c2019 = PR-Y45 audit ACCEPT HEAD; cherchi_differential_diff.rs at that
#  commit is 1652 lines without the PR-Y46 +289 LOC additive probe)
cargo build -p test-harness --test cherchi_differential_diff
```

Note: the +289 LOC are additive and live inside a TEST file (`crates/test-harness/tests/...`); the new test fn is `#[ignore]`-gated and cannot execute by default. The revert is logically equivalent to deleting dead test code; no behavioral state needs unwinding. `app/tests/cases/assay/results.json` is unaffected (probe does not write to it). No wasm-bridge or app changes to revert. WASM bundle unaffected; no rebuild required for PR-Y46 or its rollback.

---

## §8 Cherchi non-determinism characterization

Per the brief: ≥3 reruns required to characterize Cherchi non-det (42-mode vs 47-mode at `target_tris`). Canary §4.5 documented:

| Run | Stage Bb f-count | Stage B f-count | Stage E f-count | Mode (target_tris) |
|---|---:|---:|---:|---:|
| 1 (initial gen, default dump dir) | 420 | 246 | 113 | 42 |
| 2 (rerun, fresh dump dir `/tmp/y46-stages-f0020-rerun2/F0020`) | 420 | 246 | 113 | 42 |
| 3 (rerun, default dump dir; pipeline re-execution) | 420 | 246 | 113 | 42 |

**All three runs produced 42-mode (`target_tris=42 missing-attributable`).** Stage tri counts byte-stable; 47-mode was NOT observed in the 3-run characterization (consistent with PR-Y45 §4.5 also observing 42-mode dominance under `TBB_NUM_THREADS=1`).

**Mode-invariance of the verdict.** The bisection probe reads the static OBJ files post-pipeline-write, so the probe output is deterministic given the dumps. Any per-pipeline-run non-determinism would manifest as different OBJ contents (different stage counts), which would change the canonical-tri-set arithmetic. The 42-mode-dominance under `TBB_NUM_THREADS=1` is the relevant non-det observation. The decision-gate verdict (Layer-B ≥ 80%) is robust to mode: 47-mode (if observable) would add 2 entries per PR-Y44 §4.2, and since adding 2 entries cannot decrease the 24 already-attributed to Layer B, the minimum 47-mode percentage is `100.0% × 24 / 26 = 92.3%` — still Layer-B-dominant. **Decision-gate verdict is invariant under Cherchi mode.**

Per the canary discipline: probe runs must always set `TBB_NUM_THREADS=1` and `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 YANG_STAGE_DUMP=<dir>` for reproducibility. The brief's "missing-count is deterministic; extras is not" caveat (PR-Y31 banked) applies as usual.

---

## §9 PR-Y47 anchor pivot

### §9.1 The pivot

**PR-Y47 anchor = γ Render-LOD re-tessellation in `tessellate_waffle_solid` at `crates/kernel/src/boolean/yang_integration.rs:1024`** (and the underlying `tessellate_solid_ext_with_lod` in `crates/kernel/src/tessellation/mod.rs`). 24 / 24 = 100.0% of F0020 Case-D positions are dropped at this layer; 0 / 24 = 0.0% at the previously-prescribed `face_survival_detect` anchor (audit-y45 §4.1 refuted).

Paper anchor:

- **Yang 2025 §4.4.1** mesh updating + bijective re-mesh + constrained Delaunay triangulation (`refs/text/yang2025_hybrid_boolean.txt:548-590`): "We re-mesh the result along the refined intersection curves using a constrained Delaunay triangulation to restore bijectivity." The re-mesh step is the layer dropping the 24 Case-D triangles.
- **Yang 2025 §4.4.2** (`refs/text/yang2025_hybrid_boolean.txt:574-579`): "selectively retaining one of the duplicate triangles." This is the `face_survival_detect` selective-retention step — which the PR-Y46 bisection proves is NOT the dropping layer for the 24 Case-D triangles.
- **Livesu et al. 2021** simplified earcut CDT (cited in CLAUDE.md): the actual CDT implementation under γ retess.
- **Cherchi 2022 §5** manifold-flood inside/outside (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`): NOT load-bearing here — that's the Stage B layer the bisection just exonerated.

**Status: STRONG.** Position-co-location at 1e-6 oracle grid; sorted canonical-key; partition-invariant verified (0 stragglers); 3/3 reruns byte-identical at 24/24 Layer B; mode-invariance verified under `TBB_NUM_THREADS=1`.

### §9.2 Acknowledgment: audit-y45 §4.1 prescription was inference, not measurement

Audit-y45 §4.1 prescribed `face_survival_detect` as PRIMARY based on the m1x=3 ⇒ verts-survive ⇒ triangle-only-removal-layer reasoning. PR-Y46's measurement confirms the reasoning **chain** is correct (verts survive; the defect is at a triangle-only-removal layer) but **refutes** the specific anchor (`face_survival_detect`). The triangle-only-removal layer is downstream of `face_survival_detect`, at γ Render-LOD re-tessellation.

This is the SECOND consecutive cycle in which an audit-recommended anchor was empirically refuted (PR-Y44 audit-y44 §3.4 prescribed α; PR-Y45 refuted; PR-Y45 audit-y45 §4.1 prescribed face_survival_detect; PR-Y46 refutes). The pattern reinforces `feedback_phase1_diagnosis_ranking_is_inference`: audit/Phase-1 anchor prescriptions are STRUCTURAL INFERENCE based on prior-cycle measurement; canary at the next anchor is the load-bearing empirical verification step. **The discipline catches BOTH anchor prescriptions before fix-shape commit, saving two full implementation cycles.**

### §9.3 PR-Y47 canary discipline (LOAD-BEARING — applies the Y45/Y46 pattern recursively)

PR-Y47 MUST follow the recursive Y45/Y46 discipline:

1. **Sub-bisect Layer B.** Use existing per-F-stage dumps (`stage_F.0.obj` through `stage_F.4.obj` already captured at canary §4.2) to compute which F-stage(s) drop the 24 Case-D triangles. The pre-cleanup raw render mesh (Stage F.0, 138f) and the post-cleanup final (Stage F.4, 113f) are both captured. Probe `case_d ∩ (B \ F.0)` and `case_d ∩ (F.0 \ F.4)`.
2. **Per-B-Rep-face attribution.** Each Case-D triangle belongs to one or more B-Rep faces. Cross-reference Stage E_labels.csv (`tri_idx, face_id`) to find which face_id each Case-D triangle SHOULD have come from, then verify whether γ retess produced ANY tri on that face_id (or whether the face is entirely missing).
3. **Probe at multiple stages.** Per `feedback_multi_stage_anchor_probe`: the bisection at Bb→B→E was coarse. PR-Y47's sub-bisection must probe pre/mid/post the suspected re-tess sub-layer.
4. **Decision-gate before fix shape.** Per `feedback_anchor_before_fix`: ≥ 80% at the sub-layer → confirmed → propose fix-shape; ≤ 20% → refuted → SHIP-INFRA-ABORT-fix + pivot to next candidate (flood_fill_patches at PR-Y27 banked, B-Rep assembly at `assemble_brep_topology`, or `tessellate_solid_ext_with_lod` per-face independence).

### §9.4 Alternative candidates (if PR-Y47 sub-bisection refutes γ retess)

Per canary §8.4 + audit considerations:

1. **`flood_fill_patches` patch-segmentation** (PR-Y27 banked; audit-y45 §4.3 secondary). Probe if F.0→F.4 sub-bisection refutes γ retess's drop attribution.
2. **B-Rep assembly + `assemble_brep_topology`** — the conversion of Stage C (post-flood-fill) into the WaffleSolid B-Rep. Per Yang 2025 §4.4.2 + §4.5.
3. **`tessellate_solid_ext_with_lod` per-face independence** — each B-Rep face is re-tessellated independently, but boundary-vertex alignment between adjacent faces is fragile. Per PR-Y34 banked (Cherchi-Rust port stage-divergence pattern), per-face CDT may not produce edge-shared triangulations between adjacent faces.

Per `feedback_no_last_bug`: do NOT declare PR-Y47 will close F0020. The 15-cycle arc has produced anchor sharpness without closure; PR-Y47 may be the 12th INFRA SHIP (if γ retess sub-bisection refutes) or the first production-fix attempt (if γ retess sub-bisection confirms ≥ 80% at a specific sub-layer) — either is consistent with the discipline.

### §9.5 Strategic significance — first positive-measurement anchor in 15 cycles

The PR-Y(N+1) anchor recommendation has been driven by INFERENCE-FROM-REFUTATION across all prior cycles:

| Cycle | Recommended anchor | Anchor source | Measurement at canary |
|---|---|---|---|
| Y43 | (a) sub-class dominant inferred | Inference from PR-Y42 Case-A/D distribution | Measured 24 Case-D positions |
| Y44 | (a) measured 100% via δ probe | Inference from sub-class signature m1x=3 | Confirmed 100% (a) |
| Y45 | α (F.0 `remove_winding_insensitive_duplicates`) | Audit-y44 §3.4 inference from m1x=3 ⇒ triangle-only-removal-layer ⇒ α | **REFUTED at 0/24** |
| Y46 | `face_survival_detect` | Audit-y45 §4.1 inference from "the layer must be upstream of α" | **REFUTED at 0/24 Layer A; CONFIRMED at 24/24 Layer B** |
| **Y47 (this PR's pivot)** | **γ Render-LOD re-tessellation** | **MEASURED 24/24 = 100% via PR-Y46 bisection** | **(Pending PR-Y47 sub-bisection canary)** |

**For the first time in 15 cycles, the next-cycle anchor recommendation has direct positive measurement, not inference from prior refutations.** Layer B is empirically the dropping layer at 100% byte-stable confidence. PR-Y47's canary discipline still applies (sub-bisect the F.0/F.1/F.2/F.3/F.4 sub-stages of γ retess to localize within Layer B), but the macro-layer attribution is no longer an inference chain — it is a measurement.

---

## §10 Banked / open

### §10.1 Banked for PR-Y47

1. **γ Render-LOD re-tessellation at `tessellate_waffle_solid` (`yang_integration.rs:1024`) — PRIMARY PR-Y47 anchor (STRONG; 100% MEASURED).** Paper anchor Yang 2025 §4.4.1 + Livesu 2021 CDT. PR-Y47 canary must sub-bisect F.0/F.1/F.2/F.3/F.4 + per-B-Rep-face attribution before fix shape.
2. **`flood_fill_patches` patch-segmentation — SECONDARY** (audit-y45 §4.3 carry-over; PR-Y27 banked). Probe if F.0→F.4 sub-bisection refutes γ retess's drop attribution.
3. **B-Rep assembly + `assemble_brep_topology` — TERTIARY.** Probe if both γ retess and flood_fill refute.
4. **Per-face independent re-tess seam audit** — `tessellate_solid_ext_with_lod`'s face-by-face CDT may not produce edge-shared triangulations between adjacent faces. Probe if PR-Y47's per-face attribution shows the 24 Case-D split across face boundaries.

### §10.2 Banked carry-over from PR-Y45 §9.2

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44/Y45 (only the 42 bordering unpaired edges classified). The Y46 bisection scaffold is sub-class-extensible to the wider missing-set if γ retess only covers part of the 24 (which it doesn't — 100% — but a wider-set bisection would still be valuable).
2. **Cohort F0044/F0045/R0092 generalization at γ retess.** If PR-Y47 fires GREEN on F0020, run the same bisection against the cohort.
3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false (PR-Y42 §6) means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y47+ at best closes ~20.

### §10.3 Methodological banked

1. **Three-stage set-diff bisection IS the right pattern** for "which of N candidate layers drops the specific defect-attributable set?" +289 LOC additive, default-off `#[ignore]`-gated, env-var-driven, reusable for any N-layer bisection. PR-Y47+ canaries adopt this scaffold (for the F.0/F.1/F.2/F.3/F.4 sub-bisection at γ retess's internals).
2. **Decision-gate at canary phase, not at impl phase (recursive).** PR-Y46 is the second consecutive cycle to save the cost of a refuted-fix-shape impl + adversary + audit cycle by aborting at canary. This is `feedback_anchor_before_fix` discipline applied recursively across audit-recommended anchors.
3. **Audit-recommended anchors are inference, not measurement.** PR-Y44 audit-y44 §3.4 prescribed α; refuted. PR-Y45 audit-y45 §4.1 prescribed face_survival_detect; refuted. Both anchor recommendations were STRUCTURAL INFERENCE based on prior-cycle measurement, not direct measurement at the recommended anchor. The pattern reinforces `feedback_phase1_diagnosis_ranking_is_inference` LOAD-BEARINGLY: canary at the recommended anchor before scoping fix.
4. **The 15-cycle arc shows that anchor-narrowing-via-canary-refutation IS the discipline working as designed.** Each canary either confirms the anchor (rare; PR-Y46 is the first such positive-confirmation cycle in 4 cycles since PR-Y42's strategic pivot) or refutes it and narrows the candidate space. Both outcomes are valid measurement-first discipline.

### §10.4 Citations + feedback memories applied

**Paper citations:**

- **Yang 2025 §4.4.1 mesh updating** (`refs/text/yang2025_hybrid_boolean.txt:548-590`): re-mesh along refined intersection curves using CDT to restore bijectivity. **PR-Y47 PRIMARY paper anchor.**
- **Yang 2025 §4.4.2** (`refs/text/yang2025_hybrid_boolean.txt:574-579`): selective-retention of duplicate triangles — i.e., `face_survival_detect` Stage 3 selective-retention. **Empirically REFUTED as the F0020 Case-D anchor by PR-Y46 Layer A = 0/24.**
- **Cherchi 2022 §5 manifold-flood + `removeDuplicateAndDegenerateTriangles`** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`): post-arrangement triangulation cleanup. **Empirically exonerated as the F0020 Case-D anchor by PR-Y45 α at 0/24 + PR-Y46 Layer A at 0/24** — the dropping layer is downstream of both Cherchi §5 stages.
- **Livesu et al. 2021** simplified earcut CDT (cited in CLAUDE.md): the actual CDT under γ retess. PR-Y47 sub-bisection candidate.

**Feedback memories applied:**

- `feedback_anchor_before_fix` (**load-bearing for the verdict; SECOND consecutive cycle**): measurement-first; the bisection probed BOTH layers before any fix-shape commit. PR-Y46 ships 0 production logic; γ retess listed for PR-Y47 canary, NOT as fix prescription.
- `feedback_phase1_diagnosis_ranking_is_inference` (**load-bearing; SECOND consecutive cycle**): audit-y45 §4.1 "face_survival_detect PRIMARY" framing was structural inference. PR-Y46 refutes the inference empirically and POSITIVELY identifies the next anchor.
- `feedback_multi_stage_anchor_probe`: bisection probes BOTH Layer A (Bb \ B) and Layer B (B \ E); single-stage canary would have been insufficient to partition the cumulative 108-tri drop into its component layers.
- `feedback_validate_against_corpus`: partition-invariant `|Bb| - |union(A, B, E_survivors)| = 0` asserted across all 3 reruns; 0 stragglers.
- `feedback_external_coherence`: Cherchi C++ remains the reference oracle. PR-Y46 uses PR-Y44 δ output (Cherchi-Render-LOD-diff-attributed Case-D positions) as the cross-reference target; no new oracle.
- `feedback_no_last_bug`: 15th cycle on F0020 Render LOD. Explicit non-closure language in §6.5. PR-Y46 does NOT promise PR-Y47 will close F0020.
- `feedback_yang_only`: PR-Y46 ships measurement infrastructure; no production logic changed; no fallback paths.
- `feedback_no_regression_chasing`: INFRA-only; no production reverts.
- `feedback_adversary_no_destructive_git`: canary executed worktree-only; adversary-y46 brief MUST forbid destructive git (third reinforcement after PR-Y22 v1 + PR-Y45 slip).
- `feedback_implementer_anti_fabrication_diff`: canary memo §1.3-§1.4 includes verbatim diff/wc-l artifacts; impl-y46 must mirror.
- `feedback_per_plan_cycle_team`: team `pr-y46` exists for this cycle; TeamDelete at close-out per plan §Phase 8.
- `feedback_always_push`: implementation phase pushes to origin/main (plain push only; never force-push).
- `feedback_oracle_credibility_via_role_separation`: canary-y46 built + ran the bisection probe; adversary-y46 will independently re-run from impl-y46 mirror without inheriting canary's reasoning chain.
- `feedback_local_fix_for_global_invariant`: not applicable in PR-Y46 (no fix shape committed). Banked for PR-Y47 if γ retess canary green-lights and the fix-shape touches a global invariant (e.g., per-face CDT seam alignment across adjacent B-Rep faces).
- `feedback_reference_oracle_invalidates_in_both_directions`: PR-Y46 confirms that even reference-oracle-attributed findings (PR-Y44's Case-D set) require per-anchor canary verification. The Case-D set is a position-level oracle truth; which layer drops it is a separate empirical question PR-Y46 answers.

---

## §11 Verification commands (verbatim, fresh-checkout)

```bash
# Gate 1: build
cargo build -p kernel
cargo build -p test-harness --test cherchi_differential_diff

# Gate 2: F0020 default-off byte parity (CRITICAL)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int
# expect: [stage-f] 138→119→119→113→113; unpaired 30→42→39→39→39

# Gate 3: PR-Y43+Y44+Y45 baselines preserved (probe-off)
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect (42-mode): Case A=4, B=14, C=0, D=24; subclass_a=24/24=100%

# Step 0 (one-time per session): generate the 24 Case-D positions
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1 \
  2>&1 | tee /tmp/y46-attribution-source.log
# Parse per-tri 4-tuple table to /tmp/y46-f0020-case-d-positions.txt
# (24 lines + 2 comment lines = 26 total; format: qa_x qa_y qa_z qb_x qb_y qb_z qc_x qc_y qc_z)
# d[16] cross-check byte-match: 142179 -122161 -80083 156339 -119712 -121783 204678 -111355 -115049

# Step 1: generate the 3 stage dumps
mkdir -p /tmp/y46-stages-f0020
YANG_STAGE_DUMP=/tmp/y46-stages-f0020 \
  YANG_CONFORMAL_PROBE=1 \
  YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture 2>&1 | tee /tmp/y46-stage-dump-run.log
# expect:
#   /tmp/y46-stages-f0020/F0020/stage_Bb.obj (420f, 141v)
#   /tmp/y46-stages-f0020/F0020/stage_B.obj  (246f, 141v)
#   /tmp/y46-stages-f0020/F0020/stage_E_lod=Render.obj (113f, 219v)

# Gate 5 (LOAD-BEARING): bisection measurement
cargo test -p test-harness --test cherchi_differential_diff -- \
  f0020_stage_bb_b_e_bisection --ignored --nocapture
# expect:
#   [pr-y46] |Bb \ B| Layer A losers (face_survival_detect)   = 171
#   [pr-y46] |B \ E|  Layer B losers (γ Render-LOD retess)     = 194
#   [pr-y46] |Bb ∩ E| Survivors all-the-way                     = 41
#   [pr-y46] SUMMARY: Layer A (face_survival_detect) = 0 / 24 = 0.0%
#   [pr-y46] SUMMARY: Layer B (γ Render-LOD retess)   = 24 / 24 = 100.0%
#   [pr-y46] SUMMARY: NEITHER = 0 / 24 = 0.0%
#   [pr-y46] SUMMARY: PRESENT_AT_E = 0
#   [pr-y46] VERDICT: Layer-B-dominant (≥80%) → PR-Y47 anchor = γ Render-LOD retess
#   d[ 0]–d[23] each: inBb=1 inB=1 inE=0 -> B
# Decision-gate: 24/24 ≥ 19/24 (≥ 80% threshold) ⇒ Layer-B-dominant CONFIRMED;
#                PR-Y47 anchor = γ Render-LOD re-tessellation at yang_integration.rs:1024

# Gate 6: per-tri table (informational, emitted by Gate 5 run)
# Expect 24 rows: d[0]–d[23] all (inBb=1, inB=1, inE=0, layer=B). Zero NEITHER; zero PRESENT_AT_E.

# Gate 7a: kernel lib regression
cargo test -p kernel --lib
# expect: 1262 passed; 24 failed; 42 ignored

# Gate 7b: yang_fast regression
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157 passed (139 failed, 8 errored; 33 known timeouts skipped)

# Gate 8: PR-Y31 hard gate
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture
# expect: PASS (F0044 Stage B missing=0, extras=0, common=136; well_formed=true; χ=4)

# Optional re-run from fresh stage dumps (verify reproducibility):
mkdir -p /tmp/y46-stages-f0020-rerun2
YANG_STAGE_DUMP=/tmp/y46-stages-f0020-rerun2 \
  YANG_CONFORMAL_PROBE=1 YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture
Y46_BISECTION_STAGE_DIR=/tmp/y46-stages-f0020-rerun2/F0020 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_stage_bb_b_e_bisection --ignored --nocapture
# expect: byte-identical Layer A = 0 / 24 = 0.0%; Layer B = 24 / 24 = 100.0%
```
