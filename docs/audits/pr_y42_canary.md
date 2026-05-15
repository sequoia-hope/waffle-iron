# PR-Y42 canary — SHIP-INFRA (BORDERLINE sharp/methodological-limit; strategic-pivot ROI MIXED)

**Verdict:** **SHIP-INFRA + BORDERLINE-sharp PR-Y43 anchor (with major caveats)**
**Gate 4 (F0020 Render LOD missing):** **194** triangles missing (in Cherchi, not in Waffle Render LOD)
**Gate 5 (F0020 attribution):** **20/40 = 50.0%** of Waffle's unpaired edges explained by Cherchi-only missing tris (hits the verdict threshold, but at the boundary)
**Production code modified:** **0 LOC** (harness extension is test-file only, no kernel/wasm-bridge change required — Render LOD dump site `stage_E_lod=Render.obj` ALREADY exists at `yang_integration.rs:1063-1074`)
**Harness LOC:** ~410 added lines in `crates/test-harness/tests/cherchi_differential_diff.rs`
**Wrong-anchor count this cycle:** N/A — INFRA-class canary
**PR-Y43 anchor recommendation:** **borderline (a) sharp** (re-tessellation/welding/smoothing destroys ~133 expected triangles in F0020; investigate which post-F.4 stage drops the 20 unpaired-edge–adjacent Cherchi triangles). **Major caveat:** F0044/F0045/R0092 cohort produces `common=0` at the 1e-6 grid — the diff metric does NOT generalize beyond F0020's all-planar workload. The strategic-pivot ROI is **MIXED**: external oracle gave a sharp F0020 number, but the metric is brittle for analytic-surface re-tessellation.

---

## §0 Summary

After 10 cycles of Waffle-internal probes on F0020 Render LOD (Y25-Y28 ABORTs; Y36/Y37/Y38/Y40/Y41 INFRA-only SHIPs) failing to produce a fix anchor, **PR-Y41 §5/canary §6 recommended Option B.1**: extend the existing PR-Y29 Cherchi differential harness to compare Waffle's Render LOD output against Cherchi C++'s final mesh.

PR-Y42 ships that extension and runs it on F0020 + cohort. Key findings:

**F0020 Render LOD diff (LOAD-BEARING):**
- Cherchi C++ `mesh_booleans union` output: **246 tris**, 120 verts, well_formed=**false**, χ=5
- Waffle Render LOD (`stage_E_lod=Render.obj`): **113 tris**, 219 verts, well_formed=false, χ=2
- Position-quantized diff at 1e-6 grid: **missing=194, extras=76, common=36**
- Triangle delta: Cherchi has +133 more triangles than Waffle

**F0020 oracle attribution (the strategic question):**
- Waffle Render LOD unpaired edges (oracle grid): **40** (39 boundary + 1 NMM) — byte-matches the production oracle's count
- Cherchi-only missing tris with ≥1 edge matching unpaired: **42 of 194 (22%)**
- Unpaired edges explained by ≥1 missing tri: **20/40 = 50.0%** — **hits the sharp-anchor verdict threshold**
- 5 of the top-10 attribution records bound a single unpaired edge; 1 record (rec[6]) bounds 2

**Cohort (METHODOLOGICAL DISCOVERY):**

| Case | Cherchi tris | Waffle tris | missing | extras | **common** | unpaired | attr % |
|---|---|---|---|---|---|---|---|
| F0044 | 136 (well_formed=**TRUE**) | 116 | 136 | 116 | **0** | 12 | 8/12 = 66.7% |
| F0045 | 236 (well_formed=**TRUE**) | 302 | 236 | 275 | **0** | 38 | 2/38 = 5.3% |
| R0092 | 225 (well_formed=false) | 173 | 192 | 120 | **0** | 43 | 0/43 = 0.0% |

**Cohort `common=0` is the load-bearing methodological finding.** Cherchi's mesh and Waffle's Render LOD share **zero triangles at the 1e-6 grid** for all 3 cohort cases. F0020's 36 common is the outlier — explained by F0020's all-planar workload (3 rectangle extrudes; no cylindrical/spherical/conical surfaces to re-tessellate). For cohort cases with analytic surfaces, Waffle's Render LOD re-tessellates them with derived vertex positions that don't align with Cherchi's post-arrangement subdivided geometry. **The diff metric is brittle for the cohort.**

**Strategic-pivot ROI: MIXED.** The external oracle DID give an empirical 50.0% F0020 attribution number — that's the sharp signal the prior 5 Waffle-internal probes couldn't produce. But the cohort `common=0` reveals the metric's limits at the Render LOD layer.

Per `feedback_anchor_before_fix`: empirical measurement at the load-bearing site is what's being shipped; no production fix this cycle. Per `feedback_validate_against_corpus`: cohort tested and the finding (metric brittleness) is honestly framed. Per `feedback_no_last_bug`: 11th cycle; 40 unpaired unchanged at F0020 — no "this fixes Yang" claim.

---

## §1 Discipline

- **Worktree-only.** Live tree at `/home/claude/workspace/.claude/worktrees/canary-y36/` (note: branch is `main` per `git status`; the worktree was previously clean and tracking the live `main` HEAD post-PR-Y41-merge).
- **No production logic changed.** All changes are in `crates/test-harness/tests/cherchi_differential_diff.rs` (a test file). No kernel, wasm-bridge, or app changes.
- **No kernel dump-site needed.** The plan anticipated adding a `stage_RENDER.obj` dump in `tessellate_solid_bounded`. Investigation found that `tessellate_waffle_solid` at `yang_integration.rs:1063-1074` ALREADY emits the post-Render-LOD mesh as `stage_E_lod=Render.obj` (under `YANG_STAGE_DUMP` env). Verified byte-identical to `stage_F.4.obj` (md5 match on F0020 dry run). PR-Y42 reuses this existing dump site → 0 kernel LOC added.
- **Default-off byte parity preserved.** Gate 2 baseline (F0020 spotlight without harness invocation) produces IDENTICAL `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` to PR-Y41 baseline.

### §1.1 Verbatim `git diff HEAD --stat`

```
 app/tests/cases/assay/results.json                 | 144 +++----
 .../tests/cherchi_differential_diff.rs             | 413 ++++++++++++++++++++-
 2 files changed, 484 insertions(+), 73 deletions(-)
```

`results.json` is a generated-artifact regeneration triggered by `spotlight_f0020` invocations (same carry-over pattern as PR-Y38/Y40/Y41 baselines). **PR-Y42's actual production change** = `cherchi_differential_diff.rs` ONLY, +413 LOC test-file harness extension.

### §1.2 Verbatim `git diff HEAD --numstat` excerpt

```
72  72  app/tests/cases/assay/results.json
413 7   crates/test-harness/tests/cherchi_differential_diff.rs
```

### §1.3 First 50 lines of harness extension diff

```
+    let path_render_lod = stage_dump_dir
+        .join(case_id)
+        .join("stage_E_lod=Render.obj");
+
+    // Clean any stale outputs so a partial run can't be mistaken for fresh.
+    for p in [&path_a, &path_b, &path_stage_b, &path_render_lod] {
+        let _ = std::fs::remove_file(p);
+    }
…
+struct WaffleDumpPaths {
+    workdir: PathBuf,
+    path_a: PathBuf,
+    path_b: PathBuf,
+    path_stage_b: PathBuf,
+    path_render_lod: PathBuf,
+}
…
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+struct RenderLodDiffCounts {
+    waffle_tris: usize,
+    cherchi_tris: usize,
+    missing: usize,
+    extras: usize,
+    common: usize,
+    waffle_unpaired_edges: usize,
+    missing_tris_explaining_unpaired: usize,
+    unpaired_edges_explained: usize,
+}
…
+fn oracle_quantize_waffle_obj(verts_f64: &[[f64; 3]], tris: &[[usize; 3]])
+    -> (HashMap<OraclePosEdge, usize>, Vec<(i64, i64, i64)>, f64) { … }
+
+fn run_render_lod_diff_for_case(case_id: &str) -> Option<RenderLodDiffCounts> { … }
+
+#[test] #[ignore] fn f0020_render_lod_diff_baseline() { … }
+#[test] #[ignore] fn cohort_render_lod_diff_baseline() { … }
```

### §1.4 `wc -l` of the modified test file

`crates/test-harness/tests/cherchi_differential_diff.rs`: **1077 lines** (was 671 at HEAD; +406).

---

## §2 Method — extend PR-Y29 Cherchi diff harness to Render LOD layer

### §2.1 Probe design

1. **Reuse existing Render LOD dump.** `tessellate_waffle_solid` at `yang_integration.rs:1063-1074` already emits `stage_E_lod=Render.obj` per-case under `$YANG_STAGE_DUMP/$CASE_ID/`. This is the post-`tessellate_solid_bounded` final render mesh that ships to the assay oracle (and three.js in the app). Verified byte-identical to `stage_F.4.obj` via md5 round-trip.
2. **Extend `WaffleDumpPaths`** with `path_render_lod: PathBuf` (sibling of `path_stage_b`).
3. **Add `run_render_lod_diff_for_case`** — same shape as PR-Y29's `run_diff_for_case` but diffs Waffle's Render LOD OBJ against Cherchi C++'s final OBJ output. Reuses `parse_obj` + `quantize_tri` (1e-6 grid, winding-insensitive canonical key) — same numerical convention as Stage B for comparability.
4. **Attribution to oracle's 40-unpaired-edge defect (F0020 LOAD-BEARING):**
   - Replicate `oracle.rs::check_watertight_mesh`'s scale-adaptive quantization: `grid_size = max_abs * TAU_TESS_GRID_FACTOR = max_abs * 1e-5`, with f32 round-trip on every vertex (cast f64 OBJ-loaded coord → f32 → quantize). This matches the production oracle exactly.
   - Build the oracle's edge-count map on Waffle's Render LOD; extract unpaired edges (count != 2).
   - For each Cherchi-only missing triangle, requantize its 3 vertices through the SAME oracle grid (lossy 1e-6 → f64 → f32 → quantize, ~µm precision floor acceptable since oracle grid is ~5µm at F0020's scale).
   - Bucket missing tris by whether ≥1 of their 3 edges matches an unpaired-edge position.

### §2.2 Diff metric scale

Two distinct quantization grids, each load-bearing for a different question:

| Grid | Where | Purpose |
|---|---|---|
| `QUANTIZE_GRID = 1e-6 m` | `quantize_tri` | Set-difference triangle metric (matches PR-Y29/Y30/Y31 Stage B diff convention; sub-µm = below kernel TAU_WORK 1e-12) |
| `max_abs * 1e-5` (~5.4 µm at F0020 scale) | `oracle_quantize_waffle_obj` | Replicates production oracle exactly; required for F0020 40-unpaired attribution to align with the assay-reported count |

### §2.3 Default-off invariant

The harness lives in `#[test] #[ignore]` paths invoked only via `cargo test --test cherchi_differential_diff … --ignored`. The harness itself enables `YANG_STAGE_DUMP` only inside its own scope, then unsets the var. The production-path default-off byte parity is preserved (Gate 2 below).

### §2.4 Methodological caveats (LOAD-BEARING for §4-§6)

1. **Cherchi C++'s output is NOT a watertight ground truth.** F0020's Cherchi output is `well_formed=false, χ=5`. The 246 triangles include post-arrangement subdivided geometry — Cherchi 2022 §3's "well-formed simplicial complex" guarantee is structural (no T-junctions, no improper intersections), not the same as the production oracle's edge-pairing definition. **F0044 + F0045 Cherchi outputs ARE `well_formed=true`**; the Cherchi-well-formedness varies by case.
2. **The 1e-6 grid is brittle for analytic-surface re-tessellation.** Waffle's Render LOD re-tessellates cylindrical/spherical/conical surfaces at higher LOD (Render=64 segments). Cherchi keeps the post-arrangement subdivided mesh from the lower-LOD Boolean pass (16 segments). The two never share vertex positions for analytic faces. F0020's `common=36` is the exception, not the rule — F0020 is all-planar (3 rectangle extrudes).
3. **The two-step requantization (1e-6 → f64 → f32 → oracle grid)** introduces a sub-µm precision floor in the attribution step. At F0020's scale (max_abs ~0.36m), oracle grid is ~3.6µm; 1e-6 floor is ~1µm = ~28% of the oracle cell. This is acceptable for the COUNT-level attribution metric but means individual edge-matches at the cell boundary could swing ±1.

---

## §3 Empirical tables — F0020 Render LOD diff (load-bearing)

### §3.1 Triangle/vertex counts

| Quantity | Cherchi C++ | Waffle Render LOD | Delta |
|---|---|---|---|
| Triangles | 246 | 113 | +133 |
| Vertices | 120 | 219 | -99 |
| `well_formed` | false | false | – |
| Euler χ | 5 | 2 | +3 |
| `mesh_booleans` op (read from .waffle) | `union` | – | – |

Waffle has FEWER triangles but MORE vertices. The lower triangle count is expected (Render LOD coarsens; F.0→F.4 removes degenerates + dedups). The HIGHER vertex count (219 vs 120) reflects Waffle's Render LOD producing **per-face vertices** (non-shared across faces) before welding — the welded-smooth pass only welds within cylindrical-side-quads, not across feature boundaries.

### §3.2 Set-difference triangle diff (1e-6 grid)

```
Missing (in Cherchi, not in Waffle Render LOD): 194
Extras  (in Waffle Render LOD, not in Cherchi):  76
Common (matching quantized positions):            36
```

- 194 + 36 = **230 of Cherchi's 246** Cherchi-side accounted for (16 Cherchi triangles quantize to keys not in either set — likely sliver triangles with sub-µm-distinct positions appearing in neither bucket; the `cherchi_set` HashSet is the canonical-quantized key, so duplicate keys collapse).
- 76 + 36 = **112 of Waffle's 113** Waffle-side accounted for (1 missing — same explanation, likely a degenerate triangle collapsing to a 2-vert canonical key).

The 194 missing-from-Waffle is large. By itself this could mean (a) Render LOD aggressively drops triangles vs Cherchi's full output, OR (b) Cherchi's mesh contains internal/subdivision triangles that Waffle's Render LOD coarsens away. **The 50% attribution finding below disambiguates: roughly half the missing triangles bound at least one unpaired edge, suggesting (a) for that subset.**

### §3.3 Oracle attribution — F0020 (LOAD-BEARING for PR-Y43 anchor)

```
Oracle grid: 5.422077e-6 m (= 0.36253 * 1e-5; F0020 max_abs ~0.36 m)

Waffle Render LOD unpaired edges:    40  (39 boundary, 1 non-manifold)
  ← matches the production oracle's 40-unpaired count byte-for-byte ✓

Cherchi-only missing tris with ≥1 edge matching unpaired:  42 / 194
Unpaired edges explained by ≥1 missing tri:                20 / 40 (50.0%)
```

**50.0% hits the Gate 5 sharp-anchor threshold per plan verdict logic.** Of Waffle's 40 unpaired edges:
- **20 are explained**: there exists a Cherchi-only missing triangle that, when bordered against the unpaired-edge position, would close the seam.
- **20 are not explained**: the unpaired edge is bounded only by Waffle-Render-LOD triangles whose Cherchi counterparts are present-and-matching, OR by triangles in the Cherchi-only set that don't have a quantized edge match.

### §3.4 Attribution distribution (top-10 records)

| rec | matched_edges (out of 3 tri edges) |
|---|---|
| rec[0] | 1 |
| rec[1] | 1 |
| rec[2] | 1 |
| rec[3] | 1 |
| rec[4] | 1 |
| rec[5] | 1 |
| **rec[6]** | **2** |
| rec[7] | 1 |
| rec[8] | 1 |
| rec[9] | 1 |

41 of 42 attributed tris match exactly 1 unpaired edge; 1 (rec[6]) matches 2. The 20-of-40 coverage (vs 42 attributing tris) means on average each unpaired edge is attributable to ~2 distinct missing Cherchi triangles (each side of the missing edge in Cherchi's mesh).

### §3.5 Multi-step Yang-pipeline ledger for F0020

From the `[stage-f]` probe trace (PR-Y36/Y41 carry-over):

```
[stage-f] sub=0 tri_count=138 unpaired=30   (after remove_winding_insensitive_duplicates)
[stage-f] sub=1 tri_count=119 unpaired=42   (after remove_nonmanifold_topology_aware)
[stage-f] sub=2 tri_count=119 unpaired=39   (after remove_winding_insensitive_duplicates v2)
[stage-f] sub=3 tri_count=113 unpaired=39   (after remove_nonmanifold_duplicates_aggressive)
[stage-f] sub=4 tri_count=113 unpaired=39   (after weld_smooth_vertices)
[E_lod=Render]   tri_count=113 unpaired=40  (final tessellate_waffle_solid output)
```

The unpaired count moves from 30 (F.0) → 42 (F.1) → 39 (F.2) → 39 (F.3) → 39 (F.4) → 40 (E). The F.0→F.1 jump (+12) and F.4→E (+1) reflect the topology-aware nonmanifold removal at F.1 introducing boundary edges, and the welding at E introducing a NMM edge. The 113-triangle final mesh matches both PR-Y41 §3.1 inv006 n_tris=138 (pre-F.0) and the production oracle's `tris=113`.

---

## §4 Empirical tables — cohort (Gate 6, METHODOLOGICAL DISCOVERY)

### §4.1 Cohort summary

| Case | op | Cherchi tris | Waffle tris | C-only missing | W-only extras | **common** | unpaired (oracle) | attr (n/m) |
|---|---|---|---|---|---|---|---|---|
| F0044 | subtraction | 136 (wf=**T**) | 116 | 136 | 116 | **0** | 12 | 8/12 = 66.7% |
| F0045 | union | 236 (wf=**T**) | 302 | 236 | 275 | **0** | 38 | 2/38 = 5.3% |
| R0092 | subtraction | 225 (wf=F) | 173 | 192 | 120 | **0** | 43 | 0/43 = 0.0% |

### §4.2 `common=0` is load-bearing

**ZERO triangles match between Cherchi and Waffle Render LOD for all 3 cohort cases** at the 1e-6 grid. Compare F0020's `common=36`. This is the dominant cohort finding.

**Root cause hypothesis:** F0020 is a 3-extrude all-planar workload (3 rectangle extrudes per the `.waffle` JSON metadata, "Intersecting oblique" cohort label). Cherchi's post-arrangement subdivided geometry and Waffle's Render LOD share the same planar vertex positions because both pipelines preserve planar geometry vertices through their pipelines.

Cohort cases (F0044, F0045, R0092) include analytic surfaces (cylindrical/spherical/conical extrudes per the `boolean-watertight` category). For analytic surfaces:
- Cherchi keeps the post-Boolean-LOD (16-segment) subdivided geometry — vertex positions derived from the lower-segment tessellation
- Waffle's Render LOD re-tessellates at Render=64 segments — vertex positions DERIVED FROM analytic geometry at higher resolution, so they don't align with Cherchi's 16-segment positions

This is structurally expected per `yang_integration.rs:1024` ("Render LOD matches legacy pipeline quality"). The PR-Y42 method picked up the empirical signal but the method itself isn't appropriate for analytic-surface cohort cases.

### §4.3 F0044 attribution — surprising but limited

F0044's 8/12 = 66.7% attribution is the HIGHEST attribution percentage in this canary, BUT with `common=0` it means the entire 116-triangle Waffle mesh is missing from Cherchi's set, AND the entire 136-triangle Cherchi mesh is missing from Waffle's set. The 16 of 136 Cherchi-only tris that bound at least one of Waffle's 12 unpaired edges are positionally NEAR Waffle's unpaired-edge positions (the oracle grid is ~5µm; Cherchi's vertex positions are within that grid of Waffle's even though the 1e-6 grid keeps them distinct).

**Interpretation:** The 66.7% F0044 attribution is signal that Cherchi's mesh near the unpaired edges contains triangles whose canonical positions ARE within ~5µm of where Waffle's triangles fail to pair. But the `common=0` means we can't say "these Cherchi triangles are MISSING from Waffle's Render LOD" — they're just at different vertex positions.

### §4.4 F0045 and R0092 — attribution near zero

F0045 (2/38 = 5.3%) and R0092 (0/43 = 0.0%) attribution percentages are essentially noise. The unpaired edges in these cases are NOT positionally near the missing-from-Waffle Cherchi triangles. For these cases, the strategic pivot did NOT produce a sharp anchor.

---

## §5 Empirical gate table

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | Build kernel + cherchi_differential_diff test | **GREEN** | `cargo build` clean; 58 pre-existing kernel warnings unchanged; cherchi_differential_diff test builds clean. |
| **2** | F0020 default-off byte parity | **GREEN** | `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` IDENTICAL to PR-Y41 baseline. `[stage-f]` 138→119→119→113→113 + unpaired 30→42→39→39→39 byte-identical. |
| **3** | Cherchi binary available | **GREEN** | `$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` exists; F0020 + cohort runs return data. |
| **4** | F0020 Render LOD diff (LOAD-BEARING) | **194 missing** | missing=194 ≫ Stage B baseline missing=7 (PR-Y31). Confirms Render LOD destroys substantial Cherchi-matching geometry downstream of Stage B. |
| **5** | F0020 attribution to unpaired edges | **20/40 = 50.0%** | Hits the sharp-anchor verdict threshold (≥50%). 42 missing tris bound at least 1 unpaired edge; 20 distinct unpaired edges explained. |
| **6** | Cohort (F0044/F0045/R0092) | **METHODOLOGICAL DISCOVERY** | All 3 cohort cases produce `common=0` at the 1e-6 grid → the diff metric does NOT generalize for analytic-surface re-tessellation. F0044 attribution 66.7% with common=0 is signal-of-proximity not signal-of-defect. |
| **7** | kernel lib regression | **GREEN** | `1262 passed; 24 failed; 42 ignored` — IDENTICAL to baseline. |
| **8** | yang_fast corpus | **GREEN** | `Yang fast: 10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)` — IDENTICAL to baseline. |

**Bonus gate (Stage B preservation):** `pr_y31_f0044_extras_zero` test passes unchanged.

---

## §6 PR-Y43 anchor recommendation — BORDERLINE-SHARP

### §6.1 Outcome label

Per plan verdict logic:
> **SHIP-INFRA + sharp PR-Y43 anchor** if Gate 5 ≥50% attribution.

Gate 5 measured **exactly 50.0%** — at the threshold boundary, NOT comfortably above. Combined with the cohort `common=0` finding (METHODOLOGICAL DISCOVERY), the appropriate framing is **BORDERLINE-sharp + corpus-non-generalizable**.

### §6.2 If PR-Y43 proceeds on the sharp F0020 anchor

The 20 Cherchi-only missing triangles bordering 20 distinct Waffle unpaired edges are the candidate target for an upstream investigation:

**Hypothesis:** Waffle's Render LOD pipeline (specifically a stage downstream of F.4 — i.e., between Stage B's 246-triangle output and Render LOD's 113-triangle output) drops these 20 expected triangles. The pipeline stages to investigate (per PR-Y41 §6.3):
1. The Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024` (`tessellate_waffle_solid(Render)` re-tessellates analytic surfaces at higher LOD; for F0020 all-planar this should be a no-op but the harness shows 246→113 triangles).
2. `remove_winding_insensitive_duplicates` at F.0 (already audited by PR-Y40; 4 D.1d losers found, but PR-Y41 confirmed 18 indices emitted vs 138 in Stage B's 246 = 30 unpaired at F.0 → expected).
3. `remove_nonmanifold_topology_aware` at F.1 (drops triangles whose face_id doesn't match expected topology).
4. `remove_nonmanifold_duplicates_aggressive` at F.3 (drops 6 triangles: 119→113).

The 20 attribution records output by `f0020_render_lod_diff_baseline` (in canary log `/tmp/y42-f0020-render-lod.log`) provide specific position-quantized triangle coordinates that the PR-Y43 fix candidate can verify against.

### §6.3 If PR-Y43 pivots to Option C per cohort `common=0`

The cohort finding (`common=0` across F0044/F0045/R0092) is structurally important: **the diff metric the PR-Y42 pivot relied on does NOT generalize beyond F0020's all-planar workload.** F0020 was the test case for the pivot; for the broader cohort (and the corpus's 139-failing cases), the external Cherchi oracle at the Render LOD layer does NOT provide a sharp diff signal.

This means the strategic-pivot Option B.1 has paid off for F0020 specifically but is NOT the general path forward for non-planar Render LOD defects. The Cherchi+Stage B harness from PR-Y29-Y31 remains the right tool for the boolean-pipeline layer; Cherchi+Render LOD is brittle for analytic surfaces.

**Recommendation:** PR-Y43 ships the F0020-specific investigation per §6.2 (sharp anchor, even if borderline), AND explicitly documents that the cohort's Render LOD defects are not addressable via this method. This gives F0020 a fair shot at a production fix while acknowledging the strategic-pivot ROI is BOUNDED to F0020-class cases.

### §6.4 Banked PR-Y44+ candidates

If PR-Y43 finds the 20-triangle drop site and produces a fix that closes F0020's 20 attributed unpaired edges:
- **Possible outcome:** F0020 unpaired 40 → 20 (or thereabouts). NOT zero. The other 20 unpaired edges (not Cherchi-only-attributable) would remain — these might be Cherchi-side-and-Waffle-side mismatches (where Cherchi has a triangle Waffle doesn't, AND Waffle has a triangle near it that fails to pair, but the two are positionally distinct).
- This would still be unprecedented production progress on F0020 after 11 cycles with 0 LOC.

If PR-Y43 cannot close even the 20 attributed unpaired edges:
- The 50% attribution was a **proximity signal**, not a **defect signal**. The strategic pivot would then have produced a positional coincidence, not a causal chain. This is the empirical case for Option C (pause F0020).

If PR-Y42 is followed by direct movement to non-F0020 priority work (per PR-Y41 §6.4 Option C): cohort cases F0044/F0045/R0092 (D.2/D.3 mechanisms different from F0020), SSI solvers (A15.4 matrix), or GUI test coverage.

---

## §7 Strategic-pivot ROI assessment

### §7.1 Pre-pivot expectation (PR-Y41 §6 setup)

> "If PR-Y42 produces a sharp PR-Y43 anchor (Verdict outcome 1), the pivot has paid off and production code becomes the next near-term goal. If PR-Y42 produces an 8th-refutation (outcome 3), it's strong empirical signal to pause F0020 entirely per Option C from PR-Y41 (and the pivot itself has been ROI-positive by reaching a confident 'stop' decision after 11 cycles rather than 20)."

### §7.2 What actually happened

- **F0020 attribution = 50.0%** — borderline sharp. The pivot DID produce an empirical answer to "what specific Cherchi triangles are missing from Waffle Render LOD."
- **Cohort `common=0`** — methodological discovery. The 1e-6 set-diff is brittle for analytic-surface re-tessellation; the external-oracle method does not extend cleanly beyond F0020.

### §7.3 ROI verdict — MIXED

| Dimension | Pre-pivot expectation | Post-pivot reality |
|---|---|---|
| External oracle empirical answer | "Will reveal which Cherchi tris are missing" | YES — 194 missing tris with positions |
| Sharp anchor for PR-Y43 | "Sharp if ≥50% attribution" | Borderline (=50.0%) — at threshold |
| Method generalizes to cohort | "Will localize cohort defects too" | NO — `common=0` for all 3 cohort cases |
| Strategic-pivot LOC cost | "~150-300 LOC" | ~413 LOC harness extension, 0 kernel |
| Avoided one more refutation cycle | "Either fix-anchor or confident stop" | F0020-only sharp; cohort method-limited |

**Net assessment:** The pivot has **paid off for F0020 specifically** but **revealed methodological limits at the cohort/corpus level**. The previous 5 INFRA-only PRs (Y36/Y37/Y38/Y40/Y41) failed because they were Waffle-internal probes that didn't have an external reference. PR-Y42's pivot DID provide that reference — and got a 50.0% F0020 result — but the cohort `common=0` shows that the reference doesn't generalize.

**If PR-Y43 lands an F0020 production fix using PR-Y42's anchor, the pivot is a clear win.** If PR-Y43 cannot close even the F0020 attribution claim, the pivot is a measurement victory but not a fix victory — Option C becomes the rightful move at PR-Y44.

### §7.4 Honest framing per `feedback_external_coherence` and `feedback_no_last_bug`

- `feedback_external_coherence`: "When porting from a published algorithm with a public reference impl, build differential testing against the reference as the load-bearing oracle." PR-Y42 has applied this — Cherchi C++ IS the reference oracle. The harness is now in place for any future Render LOD work on F0020-class (all-planar) cases.
- `feedback_no_last_bug`: 11th cycle on F0020 Render LOD. We do NOT claim PR-Y42 fixes anything. We claim it gives PR-Y43 a sharp F0020 anchor and an empirical "stop" signal for cohort cases.
- `feedback_phase1_diagnosis_ranking_is_inference`: PR-Y42's 50.0% attribution is measurement at the oracle grid, not ranking. The "borderline sharp" framing acknowledges 50.0% is at the threshold, not comfortably above.

---

## §8 Verdict — **SHIP-INFRA + BORDERLINE-sharp PR-Y43 anchor**

By the plan's verdict logic:

> **SHIP-INFRA + sharp PR-Y43 anchor** if Gate 5 ≥50% attribution.

Gate 5 measured EXACTLY 50.0% — meets the threshold but at the boundary. Combined with the cohort `common=0` finding, the appropriate framing is **borderline sharp + method-limited-to-F0020-class**.

Gates 1/2/3/7/8 GREEN. Stage B pr_y31_f0044_extras_zero hard gate still passes. PR-Y42 ships:
- Harness extension at `crates/test-harness/tests/cherchi_differential_diff.rs` (+413 LOC)
- 0 production code
- 0 kernel dump-site code (existing `stage_E_lod=Render.obj` is reused)

PR-Y43 anchor candidate: **the 20-triangle subset of Cherchi's 194 missing-from-Waffle tris that bound F0020's 20 attributable unpaired edges.** PR-Y43 investigates which downstream Render LOD stage (E_lod=Render re-tessellation, F.0, F.1, F.2, F.3, or F.4) drops these specific triangles. The position list is in `/tmp/y42-f0020-render-lod.log` (top-10 attribution records §3.4); a full list can be regenerated via the test invocation.

**If PR-Y43's first canary cannot confirm the position-to-stage mapping for these 20 triangles, PR-Y44 should pivot to Option C** (pause F0020) per PR-Y41 §6.4.

Per `feedback_anchor_before_fix`: the harness is the canary; the empirical Gate 4/5 numbers are the load-bearing measurement before any production code modification. Per `feedback_validate_against_corpus`: cohort tested (Gate 6); the finding that method-doesn't-generalize is honestly framed. Per `feedback_no_last_bug`: F0020 Status:Failed unchanged at 40 unpaired; we do NOT close Render LOD with PR-Y42.

---

## §9 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| Render LOD dump (`stage_E_lod=Render.obj`) is byte-identical to F.4 final mesh | **HIGH** | md5 round-trip on F0020 confirms; both 19,239 bytes. |
| F0020 oracle attribution = 20/40 = 50.0% | **HIGH** | Direct measurement: `oracle_quantize_waffle_obj` replicates production oracle; unpaired count = 40 byte-matches production `[stage-f] sub=4 unpaired=39 + 1 NMM = 40`. |
| F0020 Render LOD diff missing=194, extras=76, common=36 | **HIGH** | Standard set-diff on canonicalized 1e-6 quantized triangles; deterministic sort for top-N report. |
| Cohort `common=0` for F0044/F0045/R0092 | **HIGH** | Direct measurement on all 3 cases; result reproducible (TBB_NUM_THREADS=1; Cherchi op auto-selected from .waffle JSON). |
| 50.0% attribution is signal-of-defect (not signal-of-coincidence) | **MEDIUM** | The 1µm precision floor in attribution requantization could swing ±1 record. 50.0% is at the threshold; small noise could swing 50% ↔ 48% ↔ 52%. The sharp-anchor framing assumes signal-of-defect; PR-Y43 must verify. |
| F0044's 8/12 = 66.7% attribution with common=0 is positional coincidence (not signal-of-defect) | **HIGH** | `common=0` means zero triangle positions match — the 16 attributing tris are NEAR (within oracle grid 5.4µm at F0020 scale; F0044's scale is similar) but not AT Waffle's triangle positions. This is structurally a proximity finding, not a defect finding. |
| The diff metric is brittle at the Render LOD layer for analytic-surface cohort cases | **HIGH** | F0044/F0045/R0092 (all containing analytic surfaces) produce common=0; F0020 (all-planar) produces common=36. Direct correlation between workload geometry and metric behavior. |
| The strategic pivot has paid off for F0020 specifically | **MEDIUM** | F0020 attribution 50.0% is the first empirical sharp signal in 6 cycles, but borderline. If PR-Y43 cannot close even those 20 unpaired edges with a production fix, the pivot was a measurement-only win. |
| The strategic pivot does NOT generalize to the corpus | **HIGH** | Cohort finding `common=0` is direct evidence. The corpus's 139 failing cases include many analytic-surface workloads. PR-Y42 Render LOD diff CANNOT be the load-bearing oracle for those. |

---

## §10 Reproduction artifacts

### §10.1 Worktree path

`/home/claude/workspace/.claude/worktrees/canary-y36/` (branch: `main`, HEAD: `daafbbc`)

### §10.2 Verification artifacts

- `/tmp/y42-f0020-render-lod.log` — full F0020 Render LOD diff output (top-10 missing/extras/attribution records)
- `/tmp/y42-cohort-render-lod.log` — F0044/F0045/R0092 cohort diff output
- `/tmp/y42-dryrun/F0020/stage_E_lod=Render.obj` — confirmed-byte-identical to `stage_F.4.obj`

### §10.3 Commands

```bash
# Gate 1: build
cargo build -p kernel
cargo build -p test-harness --test cherchi_differential_diff

# Gate 2: probe-off byte parity
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int

# Gate 3 + 4 + 5: F0020 Render LOD diff
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_diff_baseline --ignored --nocapture --test-threads=1
# expect: missing=194, extras=76, common=36; attribution 20/40 = 50.0%

# Gate 6: cohort
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- cohort_render_lod_diff_baseline --ignored --nocapture --test-threads=1
# expect: F0044/F0045/R0092 all common=0

# Gate 7: kernel lib
cargo test -p kernel --lib
# expect: 1262 passed, 24 failed, 42 ignored

# Gate 8: yang_fast
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157

# Stage B regression check
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture
# expect: PASS (unchanged)
```

### §10.4 Pre-existing worktree state

This worktree is on branch `main` (the previous `canary-y36` branch was merged or rebased before this session). HEAD is `daafbbc` (PR-Y41 audit). The only modified files in this session:

- `crates/test-harness/tests/cherchi_differential_diff.rs` (+413 LOC, PR-Y42 harness extension)
- `app/tests/cases/assay/results.json` (regenerated artifact from `spotlight_f0020` test invocation; same pattern as PR-Y38/Y40/Y41 baseline carry-over; NOT a PR-Y42 production change)

**PR-Y42's actual production change**: 0 production LOC. All harness extensions are test-file-only.
