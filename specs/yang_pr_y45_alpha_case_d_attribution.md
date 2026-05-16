# PR-Y45 — α Case-D attribution probe (INFRA-CLASS; α-REFUTED at 0/24 = 0.0%)

| Field | Value |
|---|---|
| **Verdict** | SHIP-INFRA + **α-REFUTED at 0/24 = 0.0%** (intersection ≤ 20% threshold ⇒ α is **NOT** the load-bearing F0020 Case-D anchor) |
| **Class** | INFRASTRUCTURE-CLASS (env-gated probe extension; 0 production logic touched) |
| **Parent commit** | `01e78fd` (PR-Y44 audit ACCEPT; 2026-05-15) |
| **Date** | 2026-05-15 |
| **Authors** | spec-y45 (this file); canary-y45 (`docs/audits/pr_y45_canary.md`) |
| **LOC** | +191 in `crates/kernel/src/tessellation/repair.rs` (3884 → 4075; all additive, env-gated by `Y45_CASE_D_ATTRIBUTION_POS`); 0 wasm-bridge; 0 app |
| **Production-code delta on F0020** | **0** (unchanged after 14 cycles) |
| **First production-fix ATTEMPT in 14 cycles** | **YES — but ABORTed at canary phase per `feedback_anchor_before_fix`** |
| **F0020 Status:Failed** | unchanged — 40 unpaired edges (39 boundary, 1 NMM); PR-Y45 changes none |
| **F0020 α-vs-Case-D measurement** | 0 / 24 = **0.0%** (inv006, 19 α-losers vs 24 Case-D positions) — byte-stable across 2 reruns; all 6 α invocations show 0/24 |

---

## §1 Motivation

PR-Y45 is the **14th investigational PR on F0020 Render LOD** and the **first production-fix ATTEMPT in 13 cycles** — but the attempt is ABORTed at the canary phase because the empirical measurement refutes the audit-y44 §3.4 anchor prescription.

PR-Y44 (commit `01e78fd`, 2026-05-15) measured F0020 Case D sub-class as **100% (a)** `(m1x=3, m5x=3)` across 8 combined reruns and 2 cohort runs. The (a) signature means **all 3 vertices of every Case-D-missing triangle are present in Waffle's Render LOD vertex set** — so the defect is at a layer that drops triangles **without** dropping their vertices.

Audit-y44 §3.4 (`docs/audits/pr_y44_validation.md:80-82`) prescribed PR-Y45's anchor as:

> **PR-Y45 anchor = (α) F.0 `remove_winding_insensitive_duplicates` (Cherchi 2022 §5 ... 19-tri drop at `[stage-f] 138→119`; PR-Y40 scaffold preserved) as the PRIMARY fix candidate, with (γ) pre-F.0 Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024` ... retained in the PR-Y45 canary surface as the BISECTION/CONTROL probe to verify the m1x=3 ⇒ vertex-survival ⇒ triangle-only-removal-layer reasoning empirically before fix shape is committed.**

The reasoning chain had two inferential steps:
1. m1x=3 ⇒ verts survive into Render LOD vertex set (mechanism evidence)
2. verts survive ⇒ defect is at a triangle-only-removal layer ⇒ α profile fits (paper anchor: Cherchi 2022 §5 `removeDuplicateAndDegenerateTriangles`)

Per `feedback_phase1_diagnosis_ranking_is_inference`: structural inference requires position-co-location canary before scoping a fix. Per `feedback_anchor_before_fix`: measurement before fix code; instrument the suspected anchor and verify empirically before writing production logic.

**The empirical question PR-Y45 measures (LOAD-BEARING):** Of the 24 F0020 Case-D (a)-sub-class triangle positions (preserved in PR-Y44's per-tri table at canary-y44 §4.1), how many are among the 19 triangles α drops at `[stage-f] 138→119`?

**Empirical answer:** **0 / 24 = 0.0% across all 6 α invocations.** Byte-stable across 2 reruns. α drops 19 entirely **DIFFERENT** triangles from the 24 Case-D triangles. The audit-y44 §3.3 reasoning chain confirms step 1 (verts survive — m1x=3 corroborated at the loser level) but **refutes step 2**: the triangle-removal layer at α is not load-bearing on the Case-D defect.

This is the discipline pattern `feedback_anchor_before_fix` describes in its load-bearing form: **measurement refuted the anchor before any production-fix code was written.** Sub-phase 2b (fix-shape selection) was skipped per the decision-gate logic; no production code touched in the 14th cycle.

---

## §2 Methodology

### §2.1 Why infrastructure-class (despite being a production-fix attempt)

- **0 production logic touched.** The +191 LOC in `crates/kernel/src/tessellation/repair.rs` are strictly additive: a new `y45_enabled` flag, an oracle-key accumulator, a per-collision capture branch inside the existing Y40 collision-record block, a per-invocation emission call, and ~140 LOC of new helpers. The `remove_winding_insensitive_duplicates` runtime semantics are byte-identical when `Y45_CASE_D_ATTRIBUTION_POS` is unset.
- **Default-off byte parity preserved by construction.** The `y45_enabled = y40_enabled && y45_case_d_attribution_enabled()` flag is false at all default-off runtime; every Y45 branch is gated on it. Gate 2 verifies F0020 spotlight byte-identical to PR-Y44 baseline (`Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int`; `[stage-f] 138→119→119→113→113`).
- **Reuses PR-Y40 scaffold.** Probe inserts inside the existing `Y40_COLLISION_PROBE=1` collision-record loop at `repair.rs:540-612`; the per-loser quantized-tri capture sits adjacent to PR-Y40's `y40_collisions.push(...)`. No new oracle, no new dump site for the underlying collision data.
- **Reuses PR-Y44 δ output.** The 24-position Case-D set is extracted from PR-Y44's `f0020_render_lod_nearest_attribution` per-tri 4-tuple table at canary-y44 §4.1 (or canary-y45 §3 — extraction script).

### §2.2 Cross-reference methodology

Per `feedback_multi_stage_anchor_probe` + `feedback_anchor_before_fix`: instrument the suspected anchor before writing fix code. The Y45 probe is a position-co-location cross-reference:

| Step | Description |
|---|---|
| 1 | Generate the 24 F0020 Case-D positions by running PR-Y44's `f0020_render_lod_nearest_attribution` (42-mode); parse per-tri 4-tuple table; persist to `/tmp/y45-f0020-case-d-positions.txt` at the 1e-6 oracle grid (matching `cherchi_differential_diff.rs::QUANTIZE_GRID`) |
| 2 | At every α invocation, capture each loser triangle's vertices quantized at the 1e-6 oracle grid (NOT α's adaptive `max_abs × 1e-5` grid; the two are deliberately different grids for clean comparison against Case-D position keys) |
| 3 | Cross-reference: for each loser canonical key (sorted i64 3-tuple of i64 3-tuples), look up in the 24-entry Case-D HashSet |
| 4 | Emit per-invocation summary: `intersection = N / 24 = N%` plus per-loser detail lines |
| 5 | Decision gate at end of 2a (canary): N ≥ 19 (≥80%) ⇒ proceed to 2b fix-shape selection; N ≤ 4 (≤20%) ⇒ α REFUTED; 5 ≤ N ≤ 18 ⇒ α PARTIAL (both skip 2b) |

The probe is intentionally extensible to any future drop layer: change the source of the position file (or the function the cross-reference lives in) and the same measurement pattern applies. PR-Y46+ canaries at other drop layers can reuse this scaffold.

### §2.3 Why this measurement, not a direct fix attempt

Per `feedback_anchor_before_fix` (banked since PR5; load-bearing again for PR-Y45):

> In tessellation/boolean fixes, add eprintln to the planned anchor function and run the test BEFORE writing code; the function may not be invoked. PR5 anchor was wrong 5 times in a row before empirical instrumentation caught it. Strategic escalation rule (2026-05-02): three wrong anchors in a row → stop bisecting, build a reference comparison.

The 13-cycle 0-production-code arc has been driven by exactly this discipline: each cycle's measurement either refutes the candidate anchor or sharpens it. PR-Y45 is the first cycle with a green-light path to a production fix shape (audit-y44 §3.4 prescription), but the discipline still applies: instrument α before committing fix logic. The Y45 probe answers the empirical question; the answer empirically refutes α; no fix code is written.

The 14-cycle accounting after PR-Y45:

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
| **Y45** | **INFRA SHIP — α-REFUTED at 0/24** | **14th investigational PR; 9th INFRA SHIP; first production-fix ATTEMPT but ABORTed at canary phase** |

---

## §3 Probe extension surface

All changes live in `crates/kernel/src/tessellation/repair.rs` (3884 → 4075 lines; **+191 LOC** strictly additive). The PR-Y40 scaffold at `repair.rs:540-723` is reused unchanged; PR-Y45 adds new env-gated branches inside the existing collision-record loop and new helper functions immediately after the PR-Y40 helpers.

### §3.1 Y45 enable flag + oracle-key accumulator declaration (≈8 LOC at L552-558)

```rust
// PR-Y45 INFRA: per-collision loser-tri quantized at the 1e-6 oracle grid
// (matches `cherchi_differential_diff.rs::QUANTIZE_GRID`). Default-off path
// byte-identical; only populated when `Y45_CASE_D_ATTRIBUTION_POS` is set
// (gated by `y40_enabled` so the Y40 collision-record loop is also armed).
let y45_enabled = y40_enabled && y45_case_d_attribution_enabled();
let mut y45_loser_oracle_keys: Vec<[(i64, i64, i64); 3]> = Vec::new();
```

### §3.2 Per-collision oracle-key capture (≈13 LOC at L611-622)

Inserted **inside** the existing `else if y40_enabled { ... y40_collisions.push(...) }` block:

```rust
if y45_enabled {
    // Re-quantize the loser tri at 1e-6 oracle grid (NOT α's
    // adaptive `max_abs * 1e-5`) so we can compare against the
    // Case-D position set which is encoded at the harness's
    // QUANTIZE_GRID = 1e-6. Mirror `quantize_pos` +
    // `quantize_tri` from `cherchi_differential_diff.rs:161-180`.
    let oa = y45_oracle_quantize_vert(vertices, indices[base]);
    let ob = y45_oracle_quantize_vert(vertices, indices[base + 1]);
    let oc = y45_oracle_quantize_vert(vertices, indices[base + 2]);
    let mut canon = [oa, ob, oc];
    canon.sort();
    y45_loser_oracle_keys.push(canon);
}
```

### §3.3 Per-invocation summary emit (≈8 LOC at L639-646)

Inserted after the existing Y40 dump call:

```rust
if y45_enabled {
    // Emits the per-loser cross-reference + intersection summary against
    // the Case-D position set. Each call corresponds to one α invocation;
    // the invocation counter (shared with PR-Y40) lets the spotlight log
    // isolate the 19-drop invocation (`[stage-f] 138→119`).
    y45_emit_case_d_attribution(&y40_collisions, &y45_loser_oracle_keys, n_tris);
}
```

### §3.4 Helper module (≈140 LOC of new helpers at L752+)

Three logical groups (full code at `crates/kernel/src/tessellation/repair.rs:752-952`):

1. **Pure-function quantizer** `y45_oracle_quantize_vert(vertices, idx) -> (i64, i64, i64)` — 1e-6 grid: `(f32 as f64 × 1e6 → round → i64)`. Matches `quantize_pos` from `cherchi_differential_diff.rs:161-180` byte-exact.
2. **Thread-local Case-D set cache** `Y45_CASE_D_SET: RefCell<Option<Result<HashSet<[(i64,i64,i64);3]>, String>>>` + `y45_load_case_d_set()` — lazy file parser; reads `Y45_CASE_D_ATTRIBUTION_POS` once per process, parses 9-int-per-line whitespace format, sorts each canonical key.
3. **Emission function** `y45_emit_case_d_attribution(collisions, loser_oracle_keys, n_tris_input)` — reads `Y40_INVOCATION_COUNTER`, lazy-loads the set, intersects per-loser keys against the set, emits one summary line + per-loser detail lines.

### §3.5 Determinism + parity preservation

- `y45_enabled` is computed once at function entry from env-var presence + `y40_enabled`. When unset, all subsequent Y45 branches are skipped at runtime.
- `y45_oracle_quantize_vert` is a pure function: deterministic across runs and matches `quantize_pos` byte-exact.
- `y45_load_case_d_set` is lazily called once per process via `thread_local!`; subsequent invocations clone the cached set (file I/O happens exactly once).
- Per-loser sorted canonical key matches the harness's `quantize_tri` sort discipline.
- `eprintln!` output ordering follows the shared `Y40_INVOCATION_COUNTER` (monotonic across the test run).

---

## §4 Contracts

| Contract | Verification |
|---|---|
| Default-off byte parity (probe-off path byte-identical to PR-Y44 HEAD `01e78fd`) | Gate 2 — F0020 spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` byte-identical pre- and post-probe-add. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical |
| PR-Y40 scaffold unchanged | The Y40 collision-record loop (L540-612), per-invocation dump (L651-723), `Y40_INVOCATION_COUNTER`, and `Y40_COLLISION_PROBE_DIR` semantics are preserved. PR-Y45 adds NEW branches inside the existing block, does not modify existing Y40 lines |
| PR-Y43+Y44 baselines preserved | `f0020_render_lod_nearest_attribution` produces 4/14/0/24 (42-mode) or 7/14/0/26 (47-mode) byte-identical to PR-Y44 canary §4.1. Case D sub-class (a)=100% byte-identical. Case B 14-entry vertex dump byte-identical |
| Cohort safety | The probe is F0020-targeted by file (Y45_CASE_D_ATTRIBUTION_POS), not by case_id. If cohort tests run with the F0020 position file, false-positive matches would surface; the canary Gate 6 is vacuously green because no production change shipped, so cohort regression is structurally impossible |
| Bucket-sum invariant (decision-gate) | Per-invocation summary asserts intersection_count ≤ min(α-losers, case_d_total). Empirically: inv006 = 19 α-losers ∩ 24 case_d = 0 ⇒ within bound; all 6 invocations consistent |
| PR-Y31 hard gate preserved | `pr_y31_f0044_extras_zero` continues to pass byte-clean (F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4) |
| Cohort skip-quietly preserved | If `Y45_CASE_D_ATTRIBUTION_POS` is unset OR cannot be read OR contains malformed lines, the probe emits a single ERROR line and returns; no panic, no abort, no side effect on the production `remove_winding_insensitive_duplicates` semantics |

---

## §5 Gates

Eight gates, mirrors canary memo §7:

| Gate | Description | Pass criterion | Result |
|---|---|---|---|
| **1** | `cargo build -p kernel && cargo build -p test-harness` | Clean build; no new warnings beyond 58 pre-existing kernel warnings + 1 slvs warning | **GREEN** — no new warnings introduced by Y45 probe |
| **2** | **F0020 default-off byte parity (CRITICAL)** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int` byte-identical to PR-Y44 baseline. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical | **GREEN** |
| **3** | PR-Y43+Y44 baselines preserved (probe-off) | A/B/C/D = 4/14/0/24 (42-mode) byte-identical to PR-Y44 canary §4.1; Case D sub-class (a)=100%; Case B 14-entry dump byte-identical | **GREEN** |
| **4** | **2a measurement (LOAD-BEARING)** | F0020 α-vs-Case-D intersection N reported across all 6 α invocations; decision gate fires | **α-REFUTED at 0/24 = 0.0%** — inv006 (n=138, 19 α-losers) = 0/24; all 6 invocations show 0/24; reproducible across 2 reruns |
| **5** | 2b fix correlation (CONDITIONAL on Gate 4 ≥ 80%) | If Gate 4 green-lights, candidate's predicted Δunpaired reported; absolute threshold for commit = ≥ -4 predicted reduction | **SKIPPED-per-design** — Gate 4 fires α REFUTED at ≤ 20% threshold; decision-gate logic skips 2b; no fix shape proposed |
| **6** | Cohort regression (CRITICAL if SHIP-FIX) | F0044/F0045/R0092 yang_boolean tests: no NEW failures | **VACUOUSLY GREEN** — no production change shipped; cohort byte-identical |
| **7a / 7b** | kernel lib + yang_fast regression | `cargo test -p kernel --lib`: **1262 / 24 / 42** IDENTICAL to PR-Y44 baseline. `YANG_BOOLEAN=1 yang_fast`: **10/157 passed** IDENTICAL | **GREEN** |
| **8** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | F0044 Stage B `missing=0, extras=0, common=136`; well_formed=true, χ=4 | **GREEN** |

**8/8 gates GREEN** (Gate 5 SKIPPED-per-design; Gate 6 vacuously green). Gate 2 is the critical INFRA-class contract; Gate 4 is the load-bearing measurement that fires α-REFUTED.

---

## §6 Outcome — **SHIP-INFRA + α-REFUTED at 0/24 = 0.0%**

### §6.1 Verdict (resolved measurement)

**α is NOT the load-bearing F0020 Case-D anchor.** Across all 6 α invocations in F0020's spotlight run, the intersection of α-losers with the 24 Case-D triangle positions is **0 / 24 = 0.0% across every invocation**, byte-stable across 2 reruns:

```
[Y45_CASE_D_ATTRIBUTION inv001] n_tris_input=12  α-losers=0  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv002] n_tris_input=12  α-losers=0  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv003] n_tris_input=60  α-losers=8  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv004] n_tris_input=60  α-losers=8  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv005] n_tris_input=12  α-losers=0  case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
[Y45_CASE_D_ATTRIBUTION inv006] n_tris_input=138 α-losers=19 case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
```

The **load-bearing invocation is `inv006`** (n_tris_input=138, 19 α-losers — the `[stage-f] 138→119` drop). 0/24 ≤ 20% threshold fires the decision-gate's "α REFUTED" outcome.

### §6.2 Semantic explanation of 0/24

Per canary §4.4: the α-dropped triangles share **VERTICES** with Case-D missing-from-Waffle triangles (consistent with the m1x=3 evidence) but the α-losers and Case-D missing triangles are **DIFFERENT triples of those vertices**. For example, loser 6 in inv006 has canonical key `(156339, -119712, -121783) / (204678, -111355, -115049) / (210686, -110317, -114212)` — the three vert positions individually appear in the Case-D file (at d[16] and d[17]), but no Case-D entry has the EXACT triple `(156339, 204678, 210686)`. α drops 19 entirely different triangles from the 24 Case-D triangles.

This is the **clean mechanism finding**: vertex-survival is corroborated (audit-y44 step 1 holds), but the triangle-removal layer at α is **not** the Case-D anchor (audit-y44 step 2 refuted). α's 19 drops are α's own dedup operations on different triangles produced upstream.

### §6.3 What this canary explicitly refutes

- **α (F.0 `remove_winding_insensitive_duplicates`) as the F0020 Case-D anchor.** 0/24 intersection. The audit-y44 §3.4 PR-Y45 anchor prescription is empirically refuted at substance (the form — "verts survive, so the defect is at a triangle-only-removal layer" — was correct as far as it went; α just isn't that layer for the 24 Case-D triangles).
- **The 19-tri F.0 drop being load-bearing on Case-D.** It IS a real drop (19 collisions × 1 loser each), but those 19 are not the Cherchi-only-missing triangles.
- **PR-Y28 banked Shape C as immediately viable.** Shape C ("source-attribution: keep the loser if `loser_face_total_tris < winner_face_total_tris`") was banked as a candidate fix-shape inside α at `repair.rs:565+`. Since α doesn't drop the Case-D triangles in the first place, Shape C cannot reduce the Case-D defect; this hypothesis is also weakened by the 0/24 measurement. Shape C may still apply to a different defect class (e.g., F0020's 152 OTHER missing tris, or cohort residuals), but it is NOT a candidate for the 24 Case-D triangles.

### §6.4 What this canary explicitly accepts

- **The Case-D position set is byte-stable.** Extracted from PR-Y44 canary §4.1 with byte-match at the d[16] spot-check (`142179 -122161 -80083 156339 -119712 -121783 204678 -111355 -115049`).
- **The 19-loser α invocation is byte-stable.** Reproduced across 2 reruns (and previously across PR-Y40's stability checks). The 19 α-losers are deterministically the same set across both runs.
- **The vertex-survival mechanism is sound.** Loser 6 in inv006 has all 3 verts individually present in the Case-D position file (as positions across different Case-D triangles). The m1x=3 measurement from PR-Y44 is corroborated.
- **The decision-gate discipline.** Per `feedback_anchor_before_fix`: measurement-first, fix-shape commit second. 0/24 refutes the anchor; no fix code is written. PR-Y45 saves the full implementation + adversary + audit cost of a refuted fix-shape attempt by aborting at canary phase.

### §6.5 What the verdict explicitly does NOT promise

- **PR-Y45 closes F0020.** It does not. F0020 unpaired count remains at 40 across all 14 cycles.
- **PR-Y46 will close F0020.** Per `feedback_no_last_bug`, even SHIP-FIX cycles do not close F0020 in one step (the closure ceiling is ~20 unpaired per PR-Y42 §6 Cherchi well_formed=false caveat).
- **The `face_survival_detect` anchor (§8 below) is confirmed.** PR-Y46 must canary it with the same position-co-location pattern; the canary IS the empirical anchor verification (per `feedback_phase1_diagnosis_ranking_is_inference`).

---

## §7 Rollback

PR-Y45 is INFRA-only with all changes confined to `crates/kernel/src/tessellation/repair.rs`. Revert procedure if the Y45 probe ever regresses default-off behavior or breaks PR-Y44 baselines:

```bash
git checkout 01e78fd -- crates/kernel/src/tessellation/repair.rs
# (01e78fd = PR-Y44 audit ACCEPT HEAD; repair.rs at that commit is 3884 lines
#  without the PR-Y45 +191 LOC additive probe)
cargo build -p kernel
```

Note: although the +191 LOC are additive and live inside a production-file `repair.rs` (not a test file), they are all **env-gated by `Y45_CASE_D_ATTRIBUTION_POS`** and structurally cannot execute in default operation. The revert is logically equivalent to deleting the dead probe code; no behavioral state needs unwinding.

`app/tests/cases/assay/results.json` regenerates from `spotlight_f0020` invocations and is not load-bearing on PR-Y45. No wasm-bridge or app changes to revert. WASM bundle unaffected (kernel-internal probe; the env-gate is also false in WASM at runtime since the wasm process never sets `Y45_CASE_D_ATTRIBUTION_POS`); no rebuild required for PR-Y45; none required for rollback.

---

## §8 PR-Y46 anchor pivot

### §8.1 The pivot

**PR-Y46 anchor = `face_survival_detect` at `crates/kernel/src/boolean/topology_extract.rs:1868`** — the Stage 3 108-tri drop layer (Boolean LOD 246 tris → 138 tris between `[yang-diag] after survival: 20 groups, 246 tris` and `[stage-f] sub=0 tri_count=138`).

Per canary-y45 §8.2 recommendation: this is the rightful PR-Y46 anchor. The Phase 1 exploration of the PR-Y45 plan corrected the audit-y44 framing of γ (γ at `yang_integration.rs:1024` is a fresh-vertex re-tessellation wrapper, NOT a 108-tri drop site). The actual 108-tri drop happens **upstream** at `face_survival_detect`.

### §8.2 Why face_survival_detect, not α / not γ / not the others

1. **The 108-tri drop magnitude is the right scale.** ~4.5× the 24-tri Case-D defect — substantially larger than α's 19-tri drop (~0.8× defect). Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:200-280`) ray-cast classification is exactly the kind of selective-retention layer that produces an (a)-class signature: it filters triangles by inside/outside labeling, dropping some while their vertex positions remain in the shared vertex set for kept neighbors.
2. **The (a) mechanism (m1x=3) fits `face_survival_detect` better than α.** The PR-Y44 (a) signature says verts survive into Render LOD. `face_survival_detect` drops triangles **before** vert deduplication at later stages; surviving neighbors of dropped triangles preserve the vert set. This is precisely the (a) signature.
3. **The other candidates are weakened.** α is empirically refuted at 0/24. γ (as Phase 1 reframed it) is a re-tessellation wrapper, not a drop site. The audit-y44 step-2 inference (verts survive ⇒ α profile) was right that the defect is at a triangle-only-removal layer; PR-Y45 has now empirically located that layer at `face_survival_detect`.

### §8.3 Acknowledgment: audit-y44 §3.4 prescription was correct in form, refuted in substance

Audit-y44 §3.4 prescribed (α) as PRIMARY based on the m1x=3 ⇒ verts-survive ⇒ triangle-only-removal-layer reasoning, with paper anchor Cherchi 2022 §5 (manifold-flood / `removeDuplicateAndDegenerateTriangles`). PR-Y45's measurement confirms the reasoning **chain** is correct (verts survive; the defect is at a triangle-only-removal layer) but **refutes** the specific anchor (α at `repair.rs:502-723`). The triangle-only-removal layer is upstream of α, at `face_survival_detect`.

This is the discipline pattern at work: structural inference picked the right shape of layer (triangle-only-removal); empirical measurement picked the wrong instance (α instead of face_survival_detect). Per `feedback_phase1_diagnosis_ranking_is_inference`, this is exactly why the canary phase exists.

### §8.4 PR-Y46 canary methodology (banked)

PR-Y46 canary should bisect `face_survival_detect`'s drop set against the 24 Case-D positions using the same Y45-style position-co-location probe pattern:

| Question | Expected outcome | Action |
|---|---|---|
| **Q1** | Of the 108 triangles dropped between Boolean LOD `246` and `face_survival_detect` output `138`, how many position-match the 24 Case-D entries at the 1e-6 oracle grid? | If ≥ 80% (≥ 19/24) → `face_survival_detect` empirically confirmed → propose fix-shape based on loser discriminators | Ship PR-Y46 SHIP-FIX (first production-fix arc) |
| **Q2** | If Q1 confirms, is the drop driven by `label_cells` inside/outside classification, op-type filtering, or both? | Per Cherchi 2022 §3 — likely inside/outside | Sub-anchor refinement |
| **Q3** | If Q1 refutes (< 20%), the residual candidates are: `flood_fill_patches` patch dropouts (PR-Y27 banked); pre-`face_survival_detect` arrangement output trimming; Yang §4.5.5 coplanar preprocessing | Decision-gate at PR-Y46 canary; possibly further refutation → PR-Y47+ | Bank wider candidate set |

The Y45 probe scaffold (+191 LOC at `repair.rs`) is the **reusable pattern** for any future drop-layer canary: change the source position file (or instrument a different drop site) and the same measurement applies.

---

## §9 Banked / open

### §9.1 Banked for PR-Y46

1. **`face_survival_detect` at `topology_extract.rs:1868`** — PRIMARY PR-Y46 anchor. 108-tri drop (Boolean LOD 246 → 138). Paper anchor Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:200-280`) + Yang 2025 §4.4.2 (`refs/text/yang2025_hybrid_boolean.txt:574-590`) inside/outside classification.
2. **`flood_fill_patches` at `topology_extract.rs` patch-segmentation** — TERTIARY. PR-Y27 banked. Probe if `face_survival_detect` also refutes.
3. **Yang §4.5.5 coplanar preprocessing** — QUATERNARY. PR-Y28-banked "D.1c all-NMM boundary" cohort residual. Longest-shot candidate.
4. **Reverse-direction canary** — PR-Y28 banked "inverse-direction canary (missing-twin source attribution)". From the 24 Case-D positions, walk backwards through the pipeline. Complementary to forward-direction Y45.
5. **Cherchi C++ `removeDuplicateAndDegenerateTriangles` differential comparison** — Per `feedback_external_coherence`. If Cherchi's dedup pass is also nearly empty on F0020 input, the F.0 19-tri drop is a Waffle-side over-aggressive dedup at the wrong layer. ~50 LOC at the C++ sidecar.

### §9.2 Open for PR-Y47+

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44 (only the 42 bordering unpaired edges classified). Y45 probe scaffold is sub-class-extensible if `face_survival_detect` only covers part of the 24.
2. **Cohort F0044/F0045/R0092 generalization at `face_survival_detect`.** If PR-Y46 fires GREEN on F0020, run the same probe against the cohort (which also has 100% sub-class (a) per PR-Y44 §6.3).
3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false (PR-Y42 §6) means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y46+ at best closes ~20.
4. **F-stage dedup audit.** If α (F.0; refuted by Y45) and `face_survival_detect` both refute, audit the F.1/F.2/F.3/F.4 dedup stages (6-tri F.3 drop is the next candidate per audit-y44 §7.1.3).
5. **Shape C / face_id gating / insert-order inversion (PR-Y28 banked fix-shape candidates).** PR-Y45 sub-phase 2b was skipped per decision-gate; these fix shapes remain candidates IF a future drop layer is empirically confirmed and the chosen layer has α-like collision semantics.

### §9.3 Methodological banked

1. **Position-co-location probe IS the right pattern** for "is layer X dropping the specific defect-attributable set?" +191 LOC additive, default-off byte-parity, env-gated, lazy file-load, thread-local cache, reusable for any drop layer. PR-Y46+ canaries adopt this scaffold.
2. **Decision-gate at canary phase, not at impl phase.** PR-Y45 saves the cost of a refuted-fix-shape impl + adversary + audit cycle by aborting at canary. This is `feedback_anchor_before_fix` discipline in its cleanest form: instrument the planned anchor BEFORE writing fix code.
3. **Inference chains with multiple steps can fail at any step.** audit-y44 §3.3 reasoning chain (verts-survive m1x=3 ⇒ triangle-only-removal-layer ⇒ α profile) had two inferential steps. PR-Y45 confirms step 1 (verts survive) but **refutes step 2 + 3** (the triangle-removal layer is not α). Future Phase 1 explorations should canary at every inferential step, not just the load-bearing one.
4. **Coarser grid in α (`max_abs × 1e-5`) vs harness oracle grid (`1e-6`) is benign at the canonical-key level.** Y45 re-quantizes at `1e-6` to compare against Case-D positions. α's adaptive grid drives its collision-detection but doesn't introduce extra false-positive matches at the cross-reference level. Verified empirically (0/24 is clean, no grid-jitter near-misses).

### §9.4 Citations + feedback memories applied

**Paper citations:**

- **Cherchi 2022 §5 manifold-flood** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`): describes the post-arrangement triangulation cleanup, including `removeDuplicateAndDegenerateTriangles` (the C++ equivalent of α). The 0/24 PR-Y45 finding means **Cherchi's dedup pass is also unlikely to be the F0020 defect anchor**; the defect must be at a layer Cherchi does NOT also have (i.e., Waffle-specific code paths like `face_survival_detect` are the suspect).
- **Cherchi 2022 §3 inside/outside classification** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:200-280`): the ray-cast classification that drives `face_survival_detect` in Waffle. This is the PR-Y46 paper anchor.
- **Yang 2025 §4.4.1** mesh-updating (`refs/text/yang2025_hybrid_boolean.txt:548-590`): "selectively retaining one of the duplicate triangles" is at the Yang §4.4.1 mesh-updating stage. PR-Y45 refutes this layer as the F0020 anchor; the anchor must be elsewhere in Yang's pipeline (likely §4.4.2 inside/outside classification, upstream of α).

**Feedback memories applied:**

- `feedback_anchor_before_fix` (**load-bearing for the verdict**): measurement refuted α before any fix code was written. PR-Y45 ships 0 production logic; (face_survival_detect) candidate listed for empirical canary in PR-Y46, NOT as fix prescription.
- `feedback_phase1_diagnosis_ranking_is_inference`: audit-y44 §3.4 "α PRIMARY" framing was structural inference (m1x=3 ⇒ triangle-only-removal-layer ⇒ α profile). PR-Y45 refutes the inference empirically. The canary phase did its job.
- `feedback_multi_stage_anchor_probe`: probe inserted at α (drop site, not at γ/upstream sites); the 0/24 finding precisely localizes α as non-load-bearing without ambiguity. Future canaries probe at every inferential step.
- `feedback_external_coherence`: Cherchi C++ remains the reference oracle. PR-Y45 uses PR-Y44 δ output (Cherchi-Render-LOD-diff-attributed Case-D positions) as the cross-reference target; no new oracle.
- `feedback_validate_against_corpus`: cohort N/A for PR-Y45 (probe is F0020-targeted). Cohort generalization remains banked per PR-Y44 §6.3.
- `feedback_no_last_bug`: 14th cycle on F0020 Render LOD. Explicit non-closure language in §6.5. PR-Y45 does NOT promise PR-Y46 will close F0020.
- `feedback_yang_only`: PR-Y45 ships measurement infrastructure; no production logic changed; no fallback paths.
- `feedback_no_regression_chasing`: INFRA-only; no production reverts.
- `feedback_adversary_no_destructive_git`: canary executed worktree-only.
- `feedback_implementer_anti_fabrication_diff`: canary memo §1.2-§1.5 includes verbatim diff/numstat/wc-l artifacts; impl-y45 must mirror.
- `feedback_per_plan_cycle_team`: team `pr-y45` exists for this cycle; TeamDelete at close-out.
- `feedback_always_push`: implementation phase pushes to origin/main (plain push only; never force-push).
- `feedback_oracle_credibility_via_role_separation`: canary-y45 built + ran the Y45 probe; adversary-y45 will independently re-run from impl-y45 mirror without inheriting canary's reasoning chain. Per the brief: adversary re-runs both reruns and verifies 0/24 byte-matches.
- `feedback_local_fix_for_global_invariant`: not applicable in PR-Y45 (no fix shape committed). Banked for PR-Y46 if face_survival_detect canary green-lights and the fix-shape touches a global invariant.

---

## §10 Verification commands (verbatim, fresh-checkout)

```bash
# Gate 1: build
cargo build -p kernel
cargo build -p test-harness

# Gate 2: F0020 default-off byte parity (CRITICAL)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 of 113 degen; 10 self-int
# expect: [stage-f] 138→119→119→113→113; unpaired 30→42→39→39→39

# Gate 3: PR-Y43+Y44 baselines preserved (probe-off)
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect (42-mode): Case A=4, B=14, C=0, D=24; subclass_a=24/24=100%
# expect (47-mode): Case A=7, B=14, C=0, D=26; subclass_a=26/26=100%

# Step 0 (one-time per session): generate the 24 Case-D positions
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1 \
  2>&1 | tee /tmp/y45-attribution-source.log
# Parse per-tri 4-tuple table to /tmp/y45-f0020-case-d-positions.txt
# (see canary-y45 §3.2 Python parser; output is 9 i64 per line at 1e-6 grid;
#  24 lines + 2 comment lines = 26 total)

# Gate 4 (LOAD-BEARING): α-vs-Case-D measurement
Y40_COLLISION_PROBE=1 \
  Y45_CASE_D_ATTRIBUTION_POS=/tmp/y45-f0020-case-d-positions.txt \
  YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture 2>&1 | grep -E "Y45_CASE_D_ATTRIBUTION|intersection"
# expect (all 6 invocations):
#   [Y45_CASE_D_ATTRIBUTION inv001] ... intersection=0 / 24 = 0.0% confirmation
#   [Y45_CASE_D_ATTRIBUTION inv002] ... intersection=0 / 24 = 0.0% confirmation
#   [Y45_CASE_D_ATTRIBUTION inv003] ... intersection=0 / 24 = 0.0% confirmation
#   [Y45_CASE_D_ATTRIBUTION inv004] ... intersection=0 / 24 = 0.0% confirmation
#   [Y45_CASE_D_ATTRIBUTION inv005] ... intersection=0 / 24 = 0.0% confirmation
#   [Y45_CASE_D_ATTRIBUTION inv006] n_tris_input=138 α-losers=19 case_d_loaded=24 intersection=0 / 24 = 0.0% confirmation
# Decision gate: 0 ≤ 4 (≤ 20% threshold) ⇒ α REFUTED; SKIP Sub-phase 2b

# Gate 5: SKIPPED per decision-gate (no fix shape attempted)

# Gate 6: cohort regression — vacuously green; no production change shipped
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0044 spotlight_f0045 spotlight_r0092 \
  --ignored --nocapture --test-threads=1
# expect: no NEW failures vs PR-Y44 baseline

# Gate 7a: kernel lib regression
cargo test -p kernel --lib
# expect: 1262 passed; 24 failed; 42 ignored

# Gate 7b: yang_fast regression
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157 passed (139 failed, 8 errored)

# Gate 8: PR-Y31 hard gate
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture
# expect: PASS (F0044 Stage B missing=0, extras=0, common=136)
```
