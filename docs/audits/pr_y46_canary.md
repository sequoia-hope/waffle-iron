# PR-Y46 canary — SHIP-INFRA + Layer-B-dominant at 100.0% (24/24)

**Verdict:** **SHIP-INFRA + Layer-B-dominant** — **24 / 24 = 100.0% of F0020 Case D positions drop at Layer B (γ Render-LOD re-tessellation in `tessellate_waffle_solid` at `yang_integration.rs:1024`); 0 / 24 = 0.0% drop at Layer A (`face_survival_detect` at `topology_extract.rs:1868`). PR-Y47 anchor = γ Render-LOD retess.** Fires decision-gate outcome 2 (Layer B ≥ 80%) per plan §Phase 2c "Verdict logic" + audit-y45 §4.

**Strategic outcome:** The audit-y45 §4.1 prescription that PR-Y46 anchor = `face_survival_detect` is **empirically REFUTED**. The 14th-cycle anchor of "PLAUSIBLE-BUT-NOT-CONFIRMED" was correctly held to canary discipline (per `feedback_anchor_before_fix` + audit-y45 §8 caveat) and is now disqualified. PR-Y47 anchor narrowed to a more precise upstream layer: the **Boolean LOD → Render LOD re-tessellation step**, NOT the `face_survival_detect` Stage 3 selective-retention layer.

The PR-Y45 disciplinary pattern recurs: a measurement at the recommended anchor refutes it; the refutation NARROWS the anchor candidate space (Yang §4.4.1 mesh-updating + Render LOD re-sample) and PRESERVES the discipline against committing fix shape on inference (`feedback_anchor_before_fix` + `feedback_phase1_diagnosis_ranking_is_inference`). The 15th cycle is the 11th INFRA SHIP.

---

## §1 Mandate + 8-gate plan + worktree state

### §1.1 Discipline

PR-Y46 is **pure test-harness extension**. Zero production code modified. Default-off byte-identical by construction (the probe is a new `#[ignore]` test fn at end of `crates/test-harness/tests/cherchi_differential_diff.rs`). The probe consumes three pre-existing `YANG_STAGE_DUMP=<dir> + YANG_CONFORMAL_PROBE=1` OBJ outputs and a PR-Y44-derived Case-D positions file; both are environmental, not in-tree, byte-identical to PR-Y45 measurement methodology.

Phase 1 exploration via `feedback_phase1_diagnosis_ranking_is_inference`: dominant ranking from audit-y45 §4.1 was `face_survival_detect`; canary tests dominant-vs-secondary by position-co-location and applies decision-gate.

Per `feedback_multi_stage_anchor_probe`: the bisection probes BOTH layers (Bb→B at Layer A; B→E at Layer B) rather than one stage alone. Per `feedback_validate_against_corpus`: the partition sanity-check `|Bb| - |union(A_losers, B_losers, E_survivors)| = 0` is asserted (Gate 5 §5.3).

### §1.2 Worktree + branch state

- Worktree path: `/home/claude/workspace/.claude/worktrees/canary-y36`
- Worktree HEAD: `b0009bd` (PR-Y42 ACCEPT) plus untracked Y43/Y44/Y45 work
- Live tree HEAD: `c0c2019` (PR-Y45 ACCEPT) on `main`
- The worktree contains PR-Y43+Y44+Y45 as un-merged (untracked + modified) state; PR-Y46 is layered on top via additive insertion at end of `cherchi_differential_diff.rs`.

### §1.3 Verbatim `git diff HEAD --stat` (PR-Y46 net contribution)

```
 crates/kernel/src/tessellation/repair.rs            | 191 ++++  (PR-Y45 pre-existing)
 crates/test-harness/tests/cherchi_differential_diff.rs | 861 +++  (PR-Y43 + Y44 + Y45 + Y46 cumulative)
```

PR-Y46-specific additive contribution to `cherchi_differential_diff.rs`: **289 LOC** (line range 1655–1943 in the new file). Pure additive after the last PR-Y44 test fn. Zero production code modified by PR-Y46.

### §1.4 `wc -l` of modified test file

```
$ wc -l crates/test-harness/tests/cherchi_differential_diff.rs
1943 crates/test-harness/tests/cherchi_differential_diff.rs
```

(PR-Y45 baseline: 1652 LOC. PR-Y46 delta = +289 LOC additive.)

### §1.5 8-gate plan

1. Build clean.
2. F0020 probe-off byte parity preserved.
3. PR-Y43+Y44+Y45 baselines preserved.
4. Stage dumps generated (Bb / B / E_lod=Render).
5. Layer A vs Layer B attribution measured (LOAD-BEARING).
6. Per-tri layer assignment table.
7. kernel lib + yang_fast preserved.
8. PR-Y31 hard gate.

---

## §2 Probe extension surface (verbatim Rust)

### §2.1 Helper: load Case-D positions file

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
            parts[i] = t.parse::<i64>().unwrap_or_else(|e| { panic!(...) });
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

Each line is 9 whitespace-separated i64 coords at the 1e-6 grid (qa.x qa.y qa.z qb.x qb.y qb.z qc.x qc.y qc.z). The 3-vertex triple is sorted into canonical winding-insensitive form before insertion. This MATCHES `quantize_tri` (lines 175-183 of the same file), which is the canonical-key function PR-Y30 / Y43 / Y44 / Y45 use for set-difference comparisons.

### §2.2 Helper: load OBJ as canonical-tri set

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

Reuses `parse_obj` + `quantize_tri` (both already exist in the test file; lines 94-159, 175-183). Position-quantized → sorted → set-inserted. The HashSet dedupes coincident canonical keys (winding-insensitive duplicates collapse).

### §2.3 Probe test fn — control flow

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
    // 6. For each Case D position:
    //      - (in_a, in_b) → layer assignment {A | B | A+B | NEITHER | PRESENT_AT_E}
    //      - emit per-tri row
    // 7. Emit summary + decision-gate verdict
}
```

Default-off via `#[ignore]`. Probe SKIPs cleanly if files missing — emits a diagnostic with the exact `YANG_STAGE_DUMP` command to run.

### §2.4 Determinism + parity preservation

- Probe is a NEW `#[ignore]` test fn appended at file end (line 1655+); no existing test fn modified.
- Probe consumes pre-existing PR-Y14a Stage Bb dump (`topology_extract.rs:2396`) + Stage B dump (`topology_extract.rs:2568`) + PR-VIZ-1 Stage E_lod=Render dump (`yang_integration.rs:1063-1074`). None of those sites modified.
- Default-off env-gating (`YANG_STAGE_DUMP=<dir>` + `YANG_CONFORMAL_PROBE=1`) is unchanged — probe-off path byte-identical to baseline.

---

## §3 Case-D position list extraction (Gate 4 prerequisite)

### §3.1 Source

PR-Y44 `f0020_render_lod_nearest_attribution` (in same test file) emits a 24-entry per-tri table at 42-mode. Same emit path PR-Y45 §3.1 used. Re-runnable via:

```bash
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1 \
  2>&1 | tee /tmp/y46-attribution-source.log
```

42-mode produced; `A/B/C/D = 4/14/0/24`; Case D sub-class (a) = 100%; bucket-sum OK. Byte-identical to PR-Y44 §4.1 + PR-Y45 §3.1 baselines.

### §3.2 Parsing

Same Python parser as PR-Y45 §3.2 (regex `d\[(\d+)\] tri=qa=\(([^)]+)\) qb=\(([^)]+)\) qc=\(([^)]+)\)` + decimal-to-i64 via `round(float(n) * 1e6)`).

### §3.3 Output file format

`/tmp/y46-f0020-case-d-positions.txt`:

```
# F0020 Case D positions at 1e-6 grid (i64); 42-mode
# Format: qa_x qa_y qa_z qb_x qb_y qb_z qc_x qc_y qc_z (un-sorted, source order)
-274919 99212 -157073 -274919 99212 -141683 -248797 103728 -207691
-274919 99212 -141683 -248797 103728 -207691 -142179 122161 70103
... (22 more lines)
274919 -99212 -105263 274919 -99212 -105263 274919 -99212 136703
```

24 data lines + 2 comment lines = 26 total.

### §3.4 Counter-check: d[16] byte-match vs PR-Y45 §3.4

PR-Y45 canary §3.4 lists d[16] = `qa=(+0.142, -0.122, -0.080) qb=(+0.156, -0.120, -0.122) qc=(+0.205, -0.111, -0.115)`. Extracted line 17 of file:

```
142179 -122161 -80083 156339 -119712 -121783 204678 -111355 -115049
```

Byte-match with PR-Y45 §3.4 confirmed. Cross-PR position-file integrity verified.

---

## §4 Sub-phase 2a measurement — Stage dumps (Gate 4)

### §4.1 Run command

```bash
mkdir -p /tmp/y46-stages-f0020
YANG_STAGE_DUMP=/tmp/y46-stages-f0020 \
  YANG_CONFORMAL_PROBE=1 \
  YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture 2>&1 | tee /tmp/y46-stage-dump-run.log
```

### §4.2 Stage tri counts (F0020 spotlight, the load-bearing third invocation)

| Stage | OBJ filename | f-count (raw OBJ) | v-count |
|---|---|---:|---:|
| A (post-subdivide) | `stage_A.obj` | 420 | 141 |
| Bb (post-label_cells, pre-survival) | `stage_Bb.obj` | **420** | 141 |
| B (post-`face_survival_detect`) | `stage_B.obj` | **246** | 141 |
| C (post-flood_fill_patches) | `stage_C.obj` | 243 | 141 |
| F.0 (Render-LOD pre-cleanup) | `stage_F.0.obj` | 138 | 217 |
| F.4 (Render-LOD final dedup) | `stage_F.4.obj` | 113 | 219 |
| **E_lod=Render** (final pipeline output) | `stage_E_lod=Render.obj` | **113** | 219 |

(Plus two `stage_E_lod=Adaptive___d_epsilon__N__.obj` entries from earlier sub-extrude invocations that are unrelated to the load-bearing third boolean.)

Per `[yang-diag]` log lines:

```
[yang-diag] after subdivide: tris_a=290, tris_b=130, verts=141    (Stage A: 420 tris)
[yang-diag] after label_cells: A outside=190 inside=100 cosurface=0, B outside=56 inside=74 cosurface=0
[yang-diag] after survival: 20 groups, 246 tris                    (Stage B: 246 tris)
[yang-diag] after flood_fill: 65 faces                             (Stage C: 243 tris)
[stage-f] sub=0 tri_count=138 unpaired=30                          (Stage F.0)
[stage-f] sub=4 tri_count=113 unpaired=39                          (Stage F.4 = E_lod=Render)
```

**Key observation:** the brief's expected counts (Bb=246, E=138) were actually:
- Bb = 420 (the FULL post-`label_cells` mesh = tris_a + tris_b; the brief's "246" matches Stage B post-`face_survival_detect`, not Stage Bb)
- E_lod=Render = 113 (the final pipeline output post-F-stages; the brief's "138" matches Stage F.0 = Render-LOD pre-cleanup)

The plan's narrative "246→138 cumulative drop spans face_survival_detect + γ retess" maps cleanly to **Stage B (246) → Stage E_lod=Render (113)** via the Render-LOD re-sample path; the actual "108-tri drop" magnitude per audit-y45 §4.1 is `246 - 138 = 108` for the F.0 sub-window, or `246 - 113 = 133` for the full Stage-B-to-E_lod=Render delta.

For the bisection, the plan's exact formulation is preserved:
- Layer A losers = `Bb \ B` = `face_survival_detect` drops
- Layer B losers = `B \ E` = γ Render-LOD re-tessellation drops

The Layer A magnitude is `|Bb \ B| = |420 - common| = 171` (the canonical-tri set delta, which is smaller than the OBJ raw delta `420 - 246 = 174` due to winding-insensitive dedup of degenerate duplicates in Bb).

### §4.3 Canonical-tri set sizes (winding-insensitive 1e-6 grid)

| Stage | Raw OBJ f-count | Unique canonical tris |
|---|---:|---:|
| Bb | 420 | **401** |
| B | 246 | **230** |
| E_lod=Render | 113 | **112** |
| Case D positions (PR-Y44) | 24 | **24** (sorted-canonical) |

The 420 → 401 collapse in Bb shows 19 of the 420 raw Bb triangles are winding-insensitive duplicates (likely from the subdivided mesh's tris_a + tris_b overlap on coplanar source faces); the 246 → 230 collapse in B shows 16 winding-insensitive duplicates; the 113 → 112 in E shows 1.

### §4.4 Sanity: monotone-decreasing partition

```
[pr-y46] SANITY: |Bb| - |union(A_losers, B_losers, E_survivors)| = 0
```

The partition `union(A_losers, B_losers, E_survivors) = stage_Bb_set` is verified — every Bb canonical tri is either dropped at Layer A, dropped at Layer B, or survives to E. **0 stragglers**.

```
[pr-y46] SANITY: |E \ Bb| triangles ADDED post-Bb = 71 (γ retess re-samples — may be non-zero; informational)
[pr-y46] SANITY: |B \ Bb| triangles ADDED post-survival = 0 (expect 0 — face_survival_detect is selective only)
```

- 71 triangles in Stage E are NOT in Stage Bb. This is the γ Render-LOD re-tessellation's fresh-vertex re-sample at 64 seg/circle generating NEW triangles that don't share canonical-tri keys with the 16-seg Boolean LOD's output. This is a load-bearing observation: **γ retess REPLACES triangles, doesn't just select**.
- 0 triangles in Stage B are NOT in Stage Bb — confirms `face_survival_detect` is selective-only (Yang §3.3 + Cherchi 2022 §5 inside/outside classification: it picks a subset of the input mesh, doesn't add new tris).

### §4.5 Cherchi non-determinism characterization

The brief required ≥3 reruns to characterize Cherchi non-det (42-mode vs 47-mode at `target_tris`). Three reruns produced:

| Run | Stage Bb f-count | Stage B f-count | Stage E f-count | Mode (target_tris) |
|---|---:|---:|---:|---:|
| 1 (initial gen) | 420 | 246 | 113 | 42 |
| 2 (rerun, fresh dump dir) | 420 | 246 | 113 | 42 |
| 3 (rerun, default dir) | 420 | 246 | 113 | 42 |

All three runs produced 42-mode (`target_tris=42 (missing-attributable)`). Stage tri counts byte-stable. 47-mode was not observed in this 3-run characterization — consistent with PR-Y45 §4.5 also observing 42-mode dominance under `TBB_NUM_THREADS=1`.

The bisection probe reads the static OBJ files post-pipeline-write, so the probe output is deterministic given the dump (any per-pipeline-run non-determinism would manifest as different OBJ contents, which would change the canonical-tri-set arithmetic). 42-mode-dominance is the relevant non-det observation, and **the Layer A / Layer B attribution result is 24/24=100% in 42-mode**; 47-mode (if observable) would add 2 entries (per PR-Y44 §4.2) but the audit-y45 §4 decision-gate semantics are based on percentage, not absolute count, and the percentage would be `100.0% × 24/26 = 92.3%` minimum (since adding 2 entries cannot decrease the 24 already-attributed to Layer B). **The decision-gate verdict is invariant under Cherchi mode.**

---

## §5 Sub-phase 2c measurement — Bisection probe (LOAD-BEARING; Gates 5+6)

### §5.1 Run command

```bash
cargo test -p test-harness --test cherchi_differential_diff -- \
  f0020_stage_bb_b_e_bisection --ignored --nocapture
```

Re-runnable against fresh dumps via:

```bash
Y46_BISECTION_STAGE_DIR=/tmp/y46-stages-f0020-rerun2/F0020 \
  cargo test -p test-harness --test cherchi_differential_diff -- \
  f0020_stage_bb_b_e_bisection --ignored --nocapture
```

### §5.2 Per-rerun summary (3 reruns, BYTE-IDENTICAL)

```
[pr-y46] |Bb \ B| Layer A losers (face_survival_detect)   = 171
[pr-y46] |B \ E|  Layer B losers (γ Render-LOD retess)     = 194
[pr-y46] |Bb ∩ E| Survivors all-the-way                     = 41
[pr-y46] SUMMARY: Layer A (face_survival_detect) = 0 / 24 = 0.0%
[pr-y46] SUMMARY: Layer B (γ Render-LOD retess)   = 24 / 24 = 100.0%
[pr-y46] SUMMARY: NEITHER (defect upstream/elsewhere) = 0 / 24 = 0.0%
[pr-y46] SUMMARY: PRESENT_AT_E (anomaly) = 0
```

**Layer A = 0 / 24 = 0.0%** (face_survival_detect drops ZERO of the 24 Case D positions).
**Layer B = 24 / 24 = 100.0%** (γ Render-LOD re-tessellation drops ALL 24 Case D positions).
**NEITHER = 0 / 24 = 0.0%** (no Case D position escapes the Bb→B→E cumulative drop).
**PRESENT_AT_E = 0** (no Case D position remains in the final mesh; consistent with PR-Y44's Case D definition = Cherchi-only-missing-from-Waffle).

Run 1 (default dump dir): 0/24, 24/24, byte-identical.
Run 2 (re-execute probe, same dumps): 0/24, 24/24, byte-identical (deterministic OBJ-set arithmetic).
Run 3 (`Y46_BISECTION_STAGE_DIR=...rerun2/F0020`, fresh stage dumps): 0/24, 24/24, byte-identical.

**Stability: 3/3 reruns at 0/24 Layer A; 24/24 Layer B; byte-identical.**

### §5.3 Sanity check — partition invariant

```
[pr-y46] SANITY: |Bb| - |union(A_losers, B_losers, E_survivors)| = 0 (expect 0 if monotone-decreasing)
```

The probe asserts (informationally — does not panic) that `layer_a_losers ∪ layer_b_losers ∪ stage_e_survivors = stage_bb_set`. **0 stragglers across all 3 reruns**. The methodology partition is sound (per `feedback_validate_against_corpus`).

### §5.4 Per-tri Case D layer assignment table (Gate 6)

```
[pr-y46] d[ 0] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 1] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 2] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 3] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 4] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 5] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 6] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 7] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 8] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[ 9] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[10] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[11] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[12] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[13] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[14] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[15] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[16] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[17] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[18] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[19] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[20] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[21] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[22] inBb=1 inB=1 inE=0 -> B
[pr-y46] d[23] inBb=1 inB=1 inE=0 -> B
```

**Every Case D position presents identically:** `inBb=1, inB=1, inE=0`. Each of the 24 positions is:
- PRESENT in Stage Bb (post-`label_cells`, pre-survival)
- PRESENT in Stage B (post-`face_survival_detect`)
- ABSENT from Stage E_lod=Render (post-γ retess)

**face_survival_detect KEEPS all 24 Case D positions** (it does NOT drop them). **γ Render-LOD re-tessellation DROPS all 24**.

### §5.5 Decision gate verdict

```
[pr-y46] VERDICT: Layer-B-dominant (≥80%) → PR-Y47 anchor = γ Render-LOD retess
```

Per plan §Phase 2c "Verdict logic":

| Threshold | Condition | Status |
|---|---|---|
| Layer-A-dominant ≥ 80% | `case_d_in_layer_a ≥ 19/24` | NO (0/24) |
| **Layer-B-dominant ≥ 80%** | `case_d_in_layer_b ≥ 19/24` | **YES (24/24 = 100%)** |
| Mixed (both ≥ 30%) | `pct_a ≥ 30 AND pct_b ≥ 30` | NO |
| Neither (both ≤ 20%) | `pct_a ≤ 20 AND pct_b ≤ 20` | NO |

Fires **outcome 2 → SHIP-INFRA + Layer-B-dominant**.

---

## §6 8-gate result summary

| # | Gate | Result | Detail |
|---|---|---|---|
| **1** | Build clean | **GREEN** | `cargo build -p test-harness --test cherchi_differential_diff` finished. Pre-existing build error in `pr13_trim_loop_diagnostic.rs` (unrelated) does not affect our target test binary. |
| **2** | F0020 probe-off byte parity | **GREEN** | `Status: Failed`; `Detail: watertight_mesh: 40 unpaired edges out of 188 total (39 boundary, 1 non-manifold); no_degenerate_triangles: 8 of 113 triangles are degenerate; no_self_intersection: 10 inter-face triangle penetrations` — matches brief baseline. |
| **3** | PR-Y43+Y44+Y45 baselines preserved | **GREEN** | `A/B/C/D = 4/14/0/24` (42-mode); Case D sub-class (a) = 100%; bucket-sum 24+0+0=24 OK. PR-Y45 α-attribution unchanged (probe lives in separate kernel `repair.rs`; not invoked by Y46 test). |
| **4** | Stage dumps generated | **GREEN** | `/tmp/y46-stages-f0020/F0020/{stage_Bb.obj=420f, stage_B.obj=246f, stage_E_lod=Render.obj=113f}` produced. Counts byte-stable across 2 pipeline reruns. |
| **5** | Layer A vs Layer B attribution (LOAD-BEARING) | **GREEN** | **Layer A = 0/24 = 0.0%; Layer B = 24/24 = 100.0%. 3 reruns byte-identical. Decision-gate fires Layer-B-dominant.** |
| **6** | Per-tri layer assignment table | **GREEN** | All 24 entries: `inBb=1 inB=1 inE=0 → B`. No entries in Layer A; no entries NEITHER. |
| **7** | kernel lib + yang_fast preserved | **GREEN** | `kernel --lib`: 1262 pass, 24 fail, 42 ignored (baseline). `yang_fast`: 10/157 passed, 139 failed, 8 errored (skipped 33 timeouts) — matches baseline. |
| **8** | PR-Y31 hard gate | **GREEN** | `pr_y31_f0044_extras_zero`: F0044 Cherchi 136 / Waffle 136 / common 136 / missing 0 / extras 0. PASS. |

**All 8 Gates GREEN. SHIP-INFRA + Layer-B-dominant.**

---

## §7 What 100% Layer B attribution actually tells us

### §7.1 The mechanism

The 24 Case D triangles emerge from Cherchi's reference output but not from Waffle's final Render LOD mesh. PR-Y44 measured that all 24 have sub-class (a) = `m1x=3, m5x=3`: their three vertices are positionally present at Waffle's 1× and 5× quantization grids (i.e., the underlying point cloud has them), but the triangles that consume those vertices are missing.

PR-Y45 refuted α (F.0 `remove_winding_insensitive_duplicates`) at 0/24 — the 24 triangles never reach F.0 to be dropped there.

PR-Y46 now refutes face_survival_detect at 0/24 — the 24 triangles ARE present in Stage B (post-`face_survival_detect`). They survive the Yang §3.3 / Cherchi 2022 §5 inside/outside classification.

PR-Y46 confirms γ Render-LOD re-tessellation at 24/24 — the 24 triangles are absent in Stage E_lod=Render after the final retessellation step at `yang_integration.rs:1024` (`tessellate_waffle_solid(&waffle, tessellation::TessellationLod::Render)`).

### §7.2 Why γ retess can drop triangles that survive face_survival_detect

The γ retess step does NOT consume Stage B's triangle list directly. Per `yang_integration.rs:1024`:

```rust
let cached_mesh = tessellate_waffle_solid(&waffle, tessellation::TessellationLod::Render)?;
```

The input is `&waffle` — a `WaffleSolid` B-Rep that was assembled from Stage B's surviving triangles via `topology_extract` → `flood_fill_patches` → `assemble_brep_topology` (Yang 2025 §4.4.2 and §4.5). The B-Rep is then re-tessellated at 64 seg/circle (Render LOD) instead of 16 seg/circle (Boolean LOD), with fresh vertex generation per curved face.

The 24 Case D positions are present at the Stage B triangle level, which means their `face_survival_detect` retention is correct. But the B-Rep assembly + re-tessellation step does NOT preserve the per-triangle topology of Stage B — it preserves the FACE topology and re-samples each face's surface independently. **Triangles at Stage B that span boundary regions, NMM half-edges, or intersection curves can be re-sampled into a different set of triangles** at Render LOD.

The 71 triangles ADDED post-Bb at Stage E (per §4.4 sanity check) are exactly this re-sample behavior: γ retess emits fresh triangles drawn from the B-Rep's analytical face surfaces, not from the Stage B triangle list. The 24 Case D triangles are **dropped because the re-tess re-samples produce DIFFERENT triangulations of the same face geometry**.

Paper citations:
- **Yang 2025 §4.4.1 mesh updating** (`refs/text/yang2025_hybrid_boolean.txt:548-590`): "We re-mesh the result along the refined intersection curves using a constrained Delaunay triangulation to restore bijectivity." The re-mesh step is the layer dropping the 24 Case D triangles.
- **Yang 2025 §4.4.2** (`refs/text/yang2025_hybrid_boolean.txt:574-579`): "selectively retaining one of the duplicate triangles." This is the `face_survival_detect` selective-retention step — which the bisection proves is NOT the dropping layer.

The mechanism is: face_survival_detect produces a watertight selection of arrangement triangles; the B-Rep is then reassembled and re-tessellated; the new tessellation has a different per-face triangulation than the surviving arrangement output.

### §7.3 What PR-Y47 must investigate

**PR-Y47 anchor (verbatim for memory file):** `tessellate_waffle_solid` at `crates/kernel/src/boolean/yang_integration.rs:1024` (the Render LOD re-tessellation call) AND/OR the upstream `tessellate_solid_ext_with_lod` (in `crates/kernel/src/tessellation/mod.rs`) at the LOD-dependent vertex re-sample boundary. Status: **STRONG (Layer B = 24/24 = 100.0%; 3/3 reruns byte-stable).**

Paper anchor: Yang 2025 §4.4.1 mesh updating + bijective re-mesh; constrained Delaunay triangulation; Livesu et al. 2021 simplified earcut for the actual CDT.

The investigation must answer: WHY does Render LOD re-tessellation produce a triangulation that diverges from Cherchi's 24 specific triangles when the underlying B-Rep faces are correctly assembled (Stage B confirms it)? Candidates:

1. **LOD-dependent re-sample boundary divergence.** 16-seg → 64-seg re-sample on curved faces generates 4× more vertices; the new triangulation's edges connect to different vertex pairs than Cherchi's matched-vertex triangles. Sub-class (a) is the signature: the verts are there, the triangle indexing isn't.
2. **`tessellate_solid_ext_with_lod` face-by-face independence.** Each B-Rep face is re-tessellated independently. Shared edges across faces may receive different vertex projections / boundary samples, producing crack-prone or hole-prone seams. This would explain why the 24 Case D triangles cluster near unpaired-edge boundaries (the 24 are bordering the 40 unpaired edges per audit-y45 §5.2).
3. **CDT seam alignment.** Constrained Delaunay along intersection curves may not match Cherchi's CDT for the same input curves. PR-Y34 banked the Yang Gauss-map filter delete — that touched STAGE4 of Cherchi-Rust port; a similar mismatch may exist at the per-face CDT step.

### §7.4 PR-Y47 canary discipline (recursive Y45 / Y46 pattern)

Per `feedback_anchor_before_fix` + `feedback_phase1_diagnosis_ranking_is_inference`: **PR-Y47 must canary at the Render-LOD re-tess drop site BEFORE committing fix shape.** The bisection at Layer B is at the COARSE level (Stage B → Stage E). PR-Y47's canary must sub-bisect: which sub-step of `tessellate_solid_ext_with_lod` introduces the drop? Candidates per `feedback_multi_stage_anchor_probe`:

- Stage E_lod=Render's sub-stages F.0 / F.1 / F.2 / F.3 / F.4 (raw render mesh → final dedup; see `stage_F.0.obj` through `stage_F.4.obj` already dumped). The drop is between Stage B (246 tris) and Stage F.0 (138 tris) = -108; then F.0 → F.4 = 138 → 113 = -25 (the F-stage cleanup).
- The B-Rep face-by-face independent re-mesh: each `face_ranges` entry corresponds to one B-Rep face; PR-Y47 can probe per-face which Case-D triangles drop where.
- The 16-seg → 64-seg vertex projection boundary: do shared edges' vertex counts match across adjacent faces?

The Y45 + Y46 probe scaffold pattern (env-gated, in-process accumulator, position-co-location at 1e-6 grid + sorted canonical-key + decision-gate) generalizes to PR-Y47.

---

## §8 Verdict + PR-Y47 anchor recommendation

### §8.1 Verdict

**SHIP-INFRA — Layer-B-dominant at 100.0% (24/24 byte-stable across 3 reruns).** All 8 Gates GREEN. Zero production code modified. PR-Y46 is the 11th INFRA SHIP in the F0020 Render-LOD arc.

### §8.2 PR-Y47 anchor (verbatim for memory file)

**PR-Y47 anchor = γ Render-LOD re-tessellation in `tessellate_waffle_solid` at `crates/kernel/src/boolean/yang_integration.rs:1024`** (and the underlying `tessellate_solid_ext_with_lod` in `crates/kernel/src/tessellation/mod.rs`). 24 / 24 = 100.0% of F0020 Case D positions are dropped at this layer; 0 / 24 = 0.0% at the previously-prescribed `face_survival_detect` anchor (audit-y45 §4.1 refuted).

Paper anchor:
- **Yang 2025 §4.4.1** (mesh updating; bijective re-mesh; CDT) at `refs/text/yang2025_hybrid_boolean.txt:548-590`.
- **Livesu et al. 2021** (simplified earcut CDT; cited in CLAUDE.md).
- Cherchi 2022 §5 (manifold-flood inside/outside) is NOT load-bearing here — that's the Stage B layer the bisection just exonerated.

**Status: STRONG.** Position-co-location at 1e-6 oracle grid; sorted canonical-key; partition-invariant verified (0 stragglers); 3/3 reruns byte-identical at 24/24 Layer B.

### §8.3 PR-Y47 canary discipline (LOAD-BEARING)

PR-Y47 MUST follow the recursive Y45 / Y46 discipline:

1. **Sub-bisect Layer B.** Use existing per-F-stage dumps (`stage_F.0.obj` through `stage_F.4.obj`) to compute which F-stage(s) drop the 24 Case D triangles. The pre-cleanup raw render mesh (Stage F.0) and the post-cleanup final (Stage F.4) are both already captured. Probe `case_d ∩ (F.0 \ F.4)` and `case_d ∩ (B \ F.0)`.
2. **Per-B-Rep-face attribution.** Each Case D triangle belongs to one or more B-Rep faces. Cross-reference Stage E_labels.csv (`tri_idx,face_id`) to find which face_id each Case D triangle SHOULD have come from, then verify whether γ retess produced ANY tri on that face_id (or whether the face is entirely missing).
3. **Probe at multiple stages.** Per `feedback_multi_stage_anchor_probe`: the bisection at Bb→B→E was coarse. PR-Y47's sub-bisection must probe pre / mid / post the suspected re-tess sub-layer. Don't commit a fix shape on one-probe inference.

### §8.4 Alternative candidates (if PR-Y47 sub-bisection refutes γ retess)

If the F.0 → F.4 sub-bisection shows Case D drops at the B-Rep assembly stage UPSTREAM of γ retess (Stage C → B-Rep → Stage F.0), the anchor moves to:

1. **`flood_fill_patches` patch-segmentation** (PR-Y27 banked, audit-y45 §4.3 secondary).
2. **B-Rep assembly + `assemble_brep_topology` in `topology_extract.rs`** — the conversion of Stage C (post-flood-fill) into the WaffleSolid B-Rep.
3. **`tessellate_solid_ext_with_lod` per-face independence** — each face is re-tessellated independently, but boundary-vertex alignment between adjacent faces is fragile.

Per `feedback_no_last_bug`: do NOT declare PR-Y47 will close F0020. The 15-cycle arc has produced anchor sharpness without closure; PR-Y47 may be the 12th INFRA SHIP or the first production-fix attempt — either is consistent with the discipline.

---

## §9 Open / banked

### §9.1 Banked for PR-Y47

1. **γ Render-LOD re-tessellation at `tessellate_waffle_solid` (`yang_integration.rs:1024`) — PRIMARY PR-Y47 anchor (STRONG; 100% measured).** Paper anchor Yang 2025 §4.4.1 + Livesu 2021 CDT. PR-Y47 canary must sub-bisect F.0 / F.1 / F.2 / F.3 / F.4 + per-B-Rep-face attribution before fix shape.
2. **`flood_fill_patches` patch-segmentation — SECONDARY** (audit-y45 §4.3 carry-over; PR-Y27 banked). Probe if F.0→F.4 sub-bisection refutes γ retess's drop attribution.
3. **B-Rep assembly + `assemble_brep_topology` — TERTIARY.** Probe if both γ retess and flood_fill refute.
4. **Per-face independent re-tess seam audit** — `tessellate_solid_ext_with_lod`'s face-by-face CDT may not produce edge-shared triangulations between adjacent faces. Probe if PR-Y47's per-face attribution shows the 24 Case D split across face boundaries.

### §9.2 Banked carry-over from PR-Y45 §8

1. **The 152 OTHER F0020 missing tris.** Unclassified by PR-Y43/Y44/Y45 (only the 42 bordering unpaired edges classified). δ + Y45 probe + Y46 bisection are sub-class-extensible to the wider 194-tri set if γ retess only covers part of the 24 (which it doesn't — 100% — but a wider-set bisection would still be valuable).
2. **Cohort F0044/F0045/R0092 generalization at γ retess.** If PR-Y47 fires GREEN on F0020, run the same bisection against the cohort.
3. **F0020 closure ceiling at ~20 unpaired.** Cherchi well_formed=false means ~20 of 40 unpaired edges are not Cherchi-only-attributable; PR-Y47+ at best closes ~20.

### §9.3 Forward-carry for PR-Y47 adversary

PR-Y47 adversary brief should:
- Re-emphasize `feedback_adversary_no_destructive_git` (third reinforcement after PR-Y22 v1 + PR-Y45 slip).
- Prefer `git show <ref>:<file>` / `git worktree add` for any cross-PR comparison.
- Independent stage-dump re-generation (do NOT reuse `/tmp/y46-stages-f0020/`).
- Independent Case-D positions file generation (do NOT reuse `/tmp/y46-f0020-case-d-positions.txt`).
- At least 3 reruns of the probe.
- Spot-check 3-5 Case D positions: trace which layer dropped them (expect: all 24 inBb=1, inB=1, inE=0).
- Per `feedback_oracle_credibility_via_role_separation`: oracle-build and oracle-interpret roles separated; PR-Y46 canary built the oracle; PR-Y46 audit (Phase 7) is responsible for interpreting it.

### §9.4 Notable observations not load-bearing for verdict

1. **71 triangles ADDED post-Bb at Stage E** (`|E \ Bb| = 71`). γ retess emits ~63% of Stage E's 112 unique canonical tris as FRESH triangles not present at Stage Bb. This is the LOD-up re-sample mechanism (16-seg → 64-seg). γ retess is therefore a REPLACE-and-ADD layer, not just a DROP layer. Sub-class (a)'s "verts present, triangle absent" signature emerges from this replace-add semantics.
2. **0 triangles ADDED post-survival at Stage B** (`|B \ Bb| = 0`). `face_survival_detect` is monotone-selective: subset of Bb. Confirms Yang §3.3 + Cherchi 2022 §5 selective-retention semantics.
3. **`stage_E_lod=Adaptive___d_epsilon__X__.obj` collision risk.** The PR-VIZ-2 banked candidate (filename collision when multiple operands share `d_epsilon`) does NOT affect F0020 (the spotlight runs 3 sub-extrudes with distinct `d_epsilon` values; no overwrite observed). Still a banked hygiene item for cohort sub-extrude cases.

---

## §10 Comparison vs PR-Y45 — the recursive discipline pattern

| Cycle | Recommended anchor | Canary verdict | Anchor outcome |
|---|---|---|---|
| **Y43** | (a) sub-class dominant inferred | Built A/B/C/D classifier; D=24 measured | (a)-inferred sub-class confirmed |
| **Y44** | (a) measured 100% via δ probe | 24/24 sub-class (a) ⇒ "topology-emission defect" | (α) PRIMARY + (γ) BISECTION CANARY |
| **Y45** | α (F.0 `remove_winding_insensitive_duplicates`) | 0/24 byte-stable across 30/30 invocations | **α REFUTED** ⇒ PR-Y46 anchor = face_survival_detect (audit-y45 §4.1) |
| **Y46 (this PR)** | face_survival_detect (audit-y45 §4.1) | 0/24 Layer A; 24/24 Layer B | **face_survival_detect REFUTED** ⇒ PR-Y47 anchor = γ Render-LOD retess |

The pattern: anchor inference based on prior-cycle measurement; canary at the recommended anchor; canary refutes the inference at a paper-anchored, mechanism-grounded candidate; the refutation narrows the candidate space upstream / downstream. **The 15th cycle and 11th INFRA SHIP.**

This is the textbook execution of `feedback_anchor_before_fix` + `feedback_phase1_diagnosis_ranking_is_inference`:
- Audit-y45 §4.1 prescribed face_survival_detect as PLAUSIBLE-BUT-NOT-CONFIRMED.
- Adversary-y45 §8 stress-tested the prescription and recommended PR-Y46 canary at face_survival_detect's drop set BEFORE fix shape.
- PR-Y46 did the canary, refuted the prescription, and identified the next-downstream candidate.
- A negative measurement at a paper-anchored mechanism-grounded candidate is just as valuable as a positive measurement.

**Strategic-pivot ROI remains POSITIVE.** PR-Y43 elevated MIXED → POSITIVE. PR-Y44 advanced from "(a) plausibly dominant" to "(a) measured 100%". PR-Y45 refuted α. **PR-Y46 refutes face_survival_detect AND positively identifies γ retess.** The cycle now has a STRONG anchor (24/24 = 100% byte-stable) rather than a PLAUSIBLE-BUT-NOT-CONFIRMED one — the first time in 15 cycles that the PR-Y(N+1) anchor recommendation has direct positive measurement, not just inference from prior refutations.

---

## §11 End-of-canary status

- **8 / 8 gates GREEN.**
- **Verdict: SHIP-INFRA + Layer-B-dominant at 100.0% (24/24 byte-stable across 3 reruns).**
- **PR-Y47 anchor: γ Render-LOD re-tessellation at `crates/kernel/src/boolean/yang_integration.rs:1024` (`tessellate_waffle_solid` + `tessellate_solid_ext_with_lod`).**
- **Production code modified: 0 LOC.**
- **Test code added: +289 LOC (one new test fn + two helper fns).**
- **Default-off byte-parity preserved.**

Per `feedback_per_plan_cycle_team`: team is `pr-y46`; teardown in close-out per plan §Phase 8.

Per `feedback_no_last_bug`: PR-Y46 does NOT close F0020. F0020 Status:Failed remains at 40 unpaired across all 15 cycles. PR-Y46 sharpens the PR-Y47 anchor from "face_survival_detect PLAUSIBLE-BUT-NOT-CONFIRMED" to "γ Render-LOD retess STRONG at 100.0%". PR-Y47 may itself be the 12th INFRA SHIP if the F.0→F.4 sub-bisection refutes γ retess as the load-bearing layer; that outcome is consistent with the discipline.
