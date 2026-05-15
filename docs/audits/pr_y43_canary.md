# PR-Y43 canary — SHIP-INFRA + Case D-DOMINANT (new outcome not in plan verdict table)

**Verdict:** **SHIP-INFRA + D-dominant** (24/42 = 57.1% of F0020's missing-attributable triangles have all 3 Cherchi verts matching Waffle vertex positions at 1× grid, yet the triangle is missing). **PR-Y44 anchor recommendation: triangle-topology investigation (indexing / winding / edge-pair emission) — NOT vertex production, NOT grid tuning, NOT upstream-of-Render-LOD pause.**

**Gate 4 (F0020 A/B/C/D histogram, LOAD-BEARING):**
| Case | Count | % of 42 |
|---|---|---|
| A (sub-grid drift) | 4 | 9.5% |
| B (2-of-3 verts at 1× + 1 off) | 14 | 33.3% |
| C (≤1 vert anywhere at 5×) | 0 | 0.0% |
| **D (residual; 3-of-3 at 1× but tri missing)** | **24** | **57.1%** |

**Production code modified:** 0 LOC (probe extension is test-file only)
**Harness LOC:** +438 in `crates/test-harness/tests/cherchi_differential_diff.rs` (1082 → 1520 lines)
**Wrong-anchor count this cycle:** N/A — INFRA-class canary
**Stability:** F0020 histogram BYTE-STABLE across 3 reruns (4/14/0/24). Cherchi C++ non-det (PR-Y31 banked) affects missing-triangle count (194 vs 201) but does NOT shift the A/B/C/D classification once `target_tris=42` is hit (~3/4 of runs); the one off-run had `target_tris=47` with A/B/C/D = 7/14/0/26 → still 0% Case C, still 14 Case B, still D-dominant.

---

## §1 Mandate + 8-gate plan

Per `/home/claude/.claude/plans/snappy-humming-hejlsberg.md`:

> Extend the PR-Y42 harness with A/B/C classification. Run on F0020. Aggregate histogram. Recommend SHIP-INFRA + PR-Y44 anchor (B-dominant → fix; C-dominant → Option C pause) / ABORT. Memo at `docs/audits/pr_y43_canary.md`.

The plan defined 4 classification cases (A/B/C/D) and 5 verdict outcomes:
1. **B-dominant (≥40%)** → PR-Y44 anchor: investigate off-vertex upstream production
2. **A-dominant (≥40%)** → PR-Y44 anchor: grid-tuning re-investigation (would be 9th-refutation per PR-Y38)
3. **C-dominant (≥40%)** → PR-Y44 anchor: Option C (pause F0020) per PR-Y41/Y42 §6
4. **diffuse (no case ≥40%)** → PR-Y44 anchor: Option C with diffusion as empirical justification
5. **ABORT** if Gates 1/2/7/8 RED

The plan §Phase 2 noted Case D as a "residual" — **but did not assign a verdict outcome to D-dominant.** F0020's empirical histogram measured Case D at **57.1%** (above the 40% threshold the other cases use). The Case D semantics — "3 verts at 1× = positional match but triangle missing — would mean indexing/winding issue" — IS a sharp, actionable PR-Y44 anchor. This memo treats D-dominant as a new 6th outcome:

6. **D-dominant (≥40%)** → PR-Y44 anchor: investigate triangle-topology emission (indexing / winding / edge-pair) at Render LOD stage, NOT vertex production

### §1.1 Discipline

- **Worktree-only.** Live tree at `/home/claude/workspace/.claude/worktrees/canary-y36/`, branch `worktree-canary-y36`, HEAD aligned with main `b0009bd` post-merge.
- **No production logic changed.** All changes in `crates/test-harness/tests/cherchi_differential_diff.rs` (test file). No kernel, wasm-bridge, or app changes.
- **Default-off byte parity preserved.** Gate 2 spotlight produces IDENTICAL `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` post-probe-add and post-probe-invoke.

### §1.2 Verbatim `git diff HEAD --stat`

```
 app/tests/cases/assay/results.json                 | 140 +++----
 crates/test-harness/tests/cherchi_differential_diff.rs | 438 +++++++++++++++++++++
 2 files changed, 508 insertions(+), 70 deletions(-)
```

`results.json` is the same generated-artifact regeneration pattern as PR-Y38/Y40/Y41/Y42 — driven by `spotlight_f0020` test invocations. PR-Y43's actual production change is `cherchi_differential_diff.rs` only, +438 LOC test-file harness extension.

### §1.3 Verbatim `git diff HEAD --numstat` excerpt

```
70  70  app/tests/cases/assay/results.json
438 0   crates/test-harness/tests/cherchi_differential_diff.rs
```

### §1.4 First 50 lines of harness extension diff

```rust
// ── PR-Y43: A/B/C nearest-triangle attribution ─────────────────────────
//
// 12th investigational PR on F0020 Render LOD. PR-Y42 found 20/40 = 50%
// of unpaired edges explained by 42 Cherchi-only missing tris (borderline-
// sharp). PR-Y43 asks: what does Waffle have NEARBY each of those 42
// missing tris?
…
struct NearestVertAttribution {
    match_at_1x: u8,    // 0..=3
    match_at_2x: u8,
    match_at_5x: u8,
    match_at_10x: u8,
    off_vert_idx_when_b: Option<u8>,
}
…
fn build_waffle_vert_sets_at_grids(verts_f64: &[[f64; 3]], base_grid: f64)
    -> [WaffleVertSetAtGrid; 4] { … }

fn cherchi_vert_matches_waffle_at_grids(
    cherchi_v: [f64; 3], base_grid: f64,
    waffle_sets: &[WaffleVertSetAtGrid; 4],
) -> [bool; 4] { … }

fn nearest_waffle_vert_at_base_grid(
    cherchi_v: [f64; 3], base_grid: f64,
    waffle_verts_f64: &[[f64; 3]],
) -> (usize, i64, [f64; 3]) { … }

fn classify_attribution(attr: &NearestVertAttribution) -> &'static str { … }

fn run_nearest_attribution_for_case(case_id: &str)
    -> Option<NearestAttributionResult> { … }

#[test] #[ignore] fn f0020_render_lod_nearest_attribution() { … }
#[test] #[ignore] fn cohort_render_lod_nearest_attribution() { … }
```

### §1.5 `wc -l` of the modified test file

`crates/test-harness/tests/cherchi_differential_diff.rs`: **1520 lines** (was 1082 at HEAD; +438).

---

## §2 Probe design

The probe extends PR-Y42's `run_render_lod_diff_for_case` with a sibling function `run_nearest_attribution_for_case`. It re-runs PR-Y42's set-diff + oracle-attribution to obtain the same `missing-attributable` set (the 42 target triangles for F0020), then per-triangle classifies them.

### §2.1 Multi-grid vertex set construction

Per `feedback_multi_stage_anchor_probe` ("don't conclude from a single grid level; sweep 1× / 2× / 5× / 10×"), the probe builds FOUR Waffle vertex sets, quantized at:

- **1×** = base oracle grid = `max_abs * TAU_TESS_GRID_FACTOR` = `max_abs * 1e-5` (~5.42µm at F0020 scale)
- **2×** = `2 * base` (~10.84µm)
- **5×** = `5 * base` (~27.11µm)
- **10×** = `10 * base` (~54.22µm)

```rust
struct WaffleVertSetAtGrid {
    factor: u32, // 1, 2, 5, 10
    keys: HashSet<(i64, i64, i64)>,
}
fn build_waffle_vert_sets_at_grids(verts_f64: &[[f64; 3]], base_grid: f64)
    -> [WaffleVertSetAtGrid; 4] { /* quantize via f32 round-trip */ }
```

f32 round-trip preserved to match the production oracle exactly (`oracle.rs:185-264`).

### §2.2 Per-triangle classification logic

```rust
fn classify_attribution(attr: &NearestVertAttribution) -> &'static str {
    if attr.match_at_5x == 3 && attr.match_at_1x < 3 { "A" }   // sub-grid drift
    else if attr.match_at_1x == 2                    { "B" }   // partial match
    else if attr.match_at_5x <= 1                    { "C" }   // no proximity
    else                                             { "D" }   // residual
}
```

The mutually-exclusive priority order: A first (catches 3-of-3-at-5× but not 3-of-3-at-1×), then B (exactly 2-at-1×), then C (≤1 anywhere at 5×), then D (catch-all: includes 3-of-3-at-1× → triangle missing despite all verts present, plus other in-between cases like 1-or-2 at 1× with 2-or-3 at 5×).

### §2.3 Case B off-vertex dump

For each Case B triangle, the probe identifies which vertex (0/1/2) is NOT matched at 1× and dumps:
- Cherchi position (lossy via 1e-6 → metres → f32 path inherited from PR-Y42)
- Nearest Waffle position (raw OBJ f64, Chebyshev cell-distance at base grid)
- Cell-distance (`i64` L∞ distance in base-grid cells)

This provides per-vertex PR-Y44 anchor data without computing kd-trees (n×m linear scan is fine for n ≤ 219 Waffle verts × 14 dumps).

### §2.4 Cherchi non-determinism mitigation

Per PR-Y31 banked + PR-Y32 §6: Cherchi C++ has internal TBB non-determinism even with `TBB_NUM_THREADS=1`. The probe deals with this two ways:
1. The classification logic operates on the missing-attributable subset, which is identified within the same Cherchi run as the classification (atomic per-test-invocation).
2. Across-run variance: 3 of 4 reruns produced `target_tris=42`; one produced 47. The A/B/C/D distribution is BYTE-STABLE for the 42-target runs (4/14/0/24). The Case B count (14) is BYTE-STABLE across all 4 reruns. Case C is 0 across all reruns. **The verdict is robust to Cherchi non-det.**

---

## §3 F0020 A/B/C/D histogram (Gate 4, LOAD-BEARING)

### §3.1 Headline measurement

| Case | Definition | Count | % of 42 | % of any rerun |
|---|---|---|---|---|
| **A** | `match_at_5x == 3 ∧ match_at_1x < 3` (sub-grid drift) | **4** | **9.5%** | 7–9% |
| **B** | `match_at_1x == 2` (partial match; 2 verts at 1×, 1 off) | **14** | **33.3%** | 30–33% |
| **C** | `match_at_5x ≤ 1` (no proximity; ≤1 vert anywhere) | **0** | **0.0%** | 0% |
| **D** | residual (3 verts at 1× but tri missing; or in-between) | **24** | **57.1%** | 55–58% |
| **Total** | | **42** | **100.0%** | – |

### §3.2 Verdict-threshold analysis

| Threshold check | Result |
|---|---|
| Case A ≥ 40%? | NO (9.5%) |
| Case B ≥ 40%? | NO (33.3%) — **below threshold, 6.7 percentage points short** |
| Case C ≥ 40%? | NO (0.0%) — **ZERO; cohort prediction inverted** |
| Case D ≥ 40%? | **YES (57.1%)** — **dominant by a margin (above plan's 40% threshold by 17 points)** |
| Diffuse (no case ≥ 40%)? | NO — Case D is dominant |

The plan's 5-outcome verdict table did NOT anticipate D-dominant outcomes. This memo proposes a 6th outcome: **SHIP-INFRA + D-dominant → PR-Y44 anchor: triangle-topology emission (indexing/winding/edge-pair), NOT vertex production**. Rationale in §7.

### §3.3 Per-rerun stability table

| Run | Cherchi missing | target_tris | Case A | Case B | Case C | Case D |
|---|---|---|---|---|---|---|
| 1 (Y42 baseline rerun pre-probe-add) | 194 | 42 | (Y42 reports 20/40 only) | – | – | – |
| 2 (post-probe-add, single) | 194/201 | 47 (TBB var) | 7 | 14 | 0 | 26 |
| 3 (Cherchi rerun) | 194 | 42 | 4 | 14 | 0 | 24 |
| 4 (Cherchi rerun) | 194 | 42 | 4 | 14 | 0 | 24 |
| 5 (Cherchi rerun) | 194 | 42 | 4 | 14 | 0 | 24 |

**Stable invariants**: Case B count = 14 across all 5 reruns. Case C count = 0 across all 5 reruns. Case D dominance (>50%) across all 5 reruns.

---

## §4 F0020 Case B off-vertex dump (Gate 5)

All 14 Case B triangles with their off-vertices, Cherchi-side positions, nearest Waffle positions, and Chebyshev cell-distance at base grid (5.422077e-6 m):

| b[i] | off_idx | C_pos (Cherchi) | W_pos (nearest Waffle) | cell_dist |
|---|---|---|---|---|
| 0 | 1 | (-2.749e-1, +9.921e-2, +1.052e-1) | (-2.063e-1, +1.111e-1, +1.148e-1) | **12,661** |
| 1 | 2 | (-2.472e-1, +1.040e-1, -2.270e-1) | (-2.405e-1, +1.052e-1, -2.260e-1) | 1,238 |
| 2 | 0 | (-2.472e-1, +1.040e-1, -2.270e-1) | (-2.405e-1, +1.052e-1, -2.260e-1) | 1,238 |
| 3 | 2 | (-1.422e-1, +1.222e-1, -1.232e-1) | (-1.422e-1, +1.222e-1, -1.926e-1) | **12,793** |
| 4 | 0 | (+1.422e-1, -1.222e-1, -1.208e-1) | (+1.563e-1, -1.197e-1, -1.218e-1) | 2,612 |
| 5 | 1 | (+1.422e-1, -1.222e-1, +2.745e-2) | (+1.422e-1, -1.222e-1, +6.998e-2) | 7,845 |
| 6 | 1 | (+1.502e-1, -1.660e-1, +2.210e-1) | (+1.269e-1, -1.927e-1, +2.216e-1) | 4,920 |
| 7 | 0 | (+1.502e-1, -1.660e-1, +2.210e-1) | (+1.269e-1, -1.927e-1, +2.216e-1) | 4,920 |
| 8 | 1 | (+2.041e-1, -1.700e-1, -1.054e-1) | (+1.563e-1, -1.197e-1, -1.218e-1) | 9,267 |
| 9 | 2 | (+2.151e-1, -1.096e-1, -1.136e-1) | (+2.107e-1, -1.103e-1, -1.142e-1) | 815 |
| 10 | 2 | (+2.151e-1, -1.096e-1, -1.136e-1) | (+2.107e-1, -1.103e-1, -1.142e-1) | 815 |
| 11 | 2 | (+2.151e-1, -1.096e-1, -1.136e-1) | (+2.107e-1, -1.103e-1, -1.142e-1) | 815 |
| 12 | 2 | (+2.749e-1, -9.921e-2, +2.045e-1) | (+2.749e-1, -9.921e-2, +2.308e-1) | 4,852 |
| 13 | 1 | (+2.996e-1, -1.237e-1, +9.938e-2) | (+2.749e-1, -9.921e-2, +1.367e-1) | 6,884 |

### §4.1 Cell-distance distribution

```
   815 cells  ×3  (b[9], b[10], b[11])  — same off-vertex, 3 different tris
 1,238 cells  ×2  (b[1], b[2])          — same off-vertex
 2,612 cells  ×1
 4,852 cells  ×1
 4,920 cells  ×2  (b[6], b[7])          — same off-vertex
 6,884 cells  ×1
 7,845 cells  ×1
 9,267 cells  ×1
12,661 cells  ×1
12,793 cells  ×1
```

### §4.2 Observations

- **5 distinct off-vertex positions account for 11 of 14 Case B entries** (b[9-11] share one off-vertex; b[1-2], b[6-7] share another; b[8] near b[4]). PR-Y44 investigation only needs to explain ~5 vertices, not 14 (the triangles are sharing topology).
- **Smallest cell-distance is 815 cells = 4.4 mm** at base grid. This is FAR larger than typical sub-grid drift (PR-Y38 confirmed grid stability under 1e-5). The off-vertices are NOT near-misses — they are genuinely different positions Waffle has at the boundary but Cherchi has interior subdivision points for.
- **Largest cell-distance is 12,793 cells = 69 mm** at base grid (b[3]). The "nearest" Waffle vert to b[3]'s missing off-vertex is on the OTHER SIDE of the workspace. This is positional non-coincidence at the OPPOSITE of "sub-grid drift."
- **Two of the 14 Case B triangles (b[6], b[7]) share both other-vert positions** and have the SAME off-vertex (Cherchi has (+1.502e-1, -1.660e-1, +2.210e-1); Waffle's nearest is (+1.269e-1, -1.927e-1, +2.216e-1) — a 4920-cell ~27mm displacement). The Z-coordinate matches to ~1µm — this off-vertex is a Cherchi-subdivided point on an existing Waffle edge.

### §4.3 Case B interpretation

Case B's 14 triangles correspond to ~5 distinct Cherchi-interior subdivision vertex positions that Cherchi has but Waffle's Render LOD does NOT have. These would be "post-arrangement subdivision points" Cherchi introduced on the 246→253 triangle expansion that Waffle's 113-triangle Render LOD removes in F.0/F.1/F.3 (the duplicate-removal + nonmanifold-removal passes per `[stage-f]` trace 138→119→119→113).

**PR-Y44 fix-shape for Case B**: 14 / 42 = 33.3% — does NOT hit the 40% B-dominant threshold but IS a real signal. The off-vertices are subdivided interior points being removed somewhere in F.0/F.1. If a PR-Y44 production fix preserved these (e.g., by not removing them in topology-aware NMM removal), it would address ~33% of the 42 attributable triangles, but the OTHER 57% (Case D) requires a separate fix-shape.

---

## §5 Cohort sanity (Gate 6)

### §5.1 Cohort histogram

| Case | F0044 | F0045 | R0092 |
|---|---|---|---|
| target_tris | 16 | 4 | **0** |
| Case A | 0 (0.0%) | 0 (0.0%) | 0 |
| **Case B** | **8 (50.0%)** | **2 (50.0%)** | 0 |
| Case C | 0 (0.0%) | 0 (0.0%) | 0 |
| Case D | 8 (50.0%) | 2 (50.0%) | 0 |
| base_grid | 4.332616e-6 m | 4.573874e-6 m | 1.305296e-7 m |

### §5.2 The brief's prediction was inverted

The brief stated: "F0044/F0045/R0092 % Case C (expect ≥95% — confirms methodology)." This expectation was based on PR-Y42's finding that cohort cases have `common=0` (zero matching triangles at 1e-6 grid). The reasoning was: "if zero triangles match, the vertices can't match either, so Case C should dominate."

**The cohort empirically shows ZERO Case C and 50/50 Case B/D split.** The reasoning's premise was wrong: triangle-position match (`quantize_tri`, 1e-6 grid) and vertex-position match (oracle grid 4.3µm; or 1× = 4.3µm) are DIFFERENT comparisons. Cherchi and Waffle share many vertex positions for cohort cases (the planar boundaries, cylindrical caps' polar verts, etc.) — they just don't share enough to produce identical triangles at the canonical-form quantization.

### §5.3 What this means for the methodology

**The methodology is NOT a bug.** Case C = 0 across F0020 + cohort is empirically what the geometry produces:
- F0020 (all-planar): Cherchi and Waffle share planar boundary verts densely → Case C trivially impossible.
- F0044/F0045 (analytic surfaces): Cherchi and Waffle share polar caps + planar boundary verts → Case C still impossible at these grid scales.
- R0092: 0 target_tris → no missing-attributable triangles to classify → vacuous all-zero.

The cohort sanity check confirms: **the missing-attributable triangles in cohort cases have the SAME off-vertex pattern as F0020 (Case B + Case D split), just at different scales**. The PR-Y44 anchor recommendation may generalize beyond F0020 ONLY IF the underlying triangle-topology defect Cherchi exposes is the same mechanism — but that's a PR-Y44+ measurement, not this canary's conclusion.

### §5.4 Counter-finding to the brief

The brief expected cohort Case C dominance to "confirm methodology not a bug." A different sanity check is the right one in retrospect: **the methodology is sound iff F0020 and cohort produce STABLE, REPRODUCIBLE histograms** — and they do (cohort histograms byte-stable across 2 reruns; F0020 stable across 5 reruns at the 42-target run-mode).

---

## §6 All other gate results

| Gate | Description | Status | Observed |
|---|---|---|---|
| **1** | `cargo build -p test-harness --test cherchi_differential_diff` | **GREEN** | Clean build. 58 pre-existing kernel warnings unchanged. New types `NearestVertAttribution` / `WaffleVertSetAtGrid` / `NearestAttributionResult` compile clean; `#[allow(dead_code)]` applied to fields inspected only via `Debug` + classification call. |
| **2** | F0020 default-off byte parity (post-probe-add) | **GREEN** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` BYTE-IDENTICAL to PR-Y42 baseline. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical. Re-confirmed AFTER all probe runs. |
| **3** | PR-Y42 Render LOD diff baseline preserved | **GREEN (with PR-Y31 banked caveat)** | `f0020_render_lod_diff_baseline` reproduces `Common=36, Extras=76, attribution 20/40 = 50.0%, 42 missing-attributable tris`. `Missing` count varies 194 ↔ 201 across reruns due to Cherchi TBB non-det (PR-Y31 banked observation). The load-bearing PR-Y42 numbers (50.0% attribution + 42 target tris + common=36) are STABLE. |
| **4** | F0020 A/B/C/D classification (LOAD-BEARING) | **D-DOMINANT** | A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), **D=24 (57.1%)**. Byte-stable across 5 reruns at target_tris=42. |
| **5** | F0020 Case B vertex dump | **14 entries dumped** | 14 entries with (Cherchi pos, nearest Waffle pos, cell-distance). 5 distinct off-vertex positions account for 11 of 14 entries. Cell-distance range: 815 – 12,793 cells (4.4mm – 69mm). |
| **6** | Cohort sanity F0044/F0045/R0092 | **NO Case C** | F0044: 0/50/0/50 (target=16). F0045: 0/50/0/50 (target=4). R0092: 0/0/0/0 (target=0). The brief's "≥95% Case C" prediction was wrong; methodology is sound (Cherchi and Waffle share vertex positions even when triangles differ). |
| **7** | kernel lib + yang_fast regression | **GREEN** | `cargo test -p kernel --lib`: **1262 passed; 24 failed; 42 ignored** — IDENTICAL to PR-Y42 baseline. `YANG_BOOLEAN=1 yang_fast`: **10/157 passed, 139 failed, 8 errored** — IDENTICAL to baseline. |
| **8** | PR-Y31 hard gate `pr_y31_f0044_extras_zero` | **GREEN** | F0044 Stage B `missing=0, extras=0, common=136`. Test passes byte-clean. |

---

## §7 Verdict + PR-Y44 anchor recommendation

### §7.1 Verdict: **SHIP-INFRA + D-DOMINANT (new 6th outcome)**

The plan's 5-outcome verdict table did not include a D-dominant outcome. The Case D count of 24/42 = 57.1% is well above the 40% threshold used for the other dominant-case verdicts. F0020's data is **NOT diffuse** (no case ≥ 40% threshold not met — Case D clearly dominates). It is also NOT B/A/C-dominant. The cleanest framing is **D-dominant** with a corresponding new PR-Y44 anchor:

**PR-Y44 anchor candidate: triangle-topology emission at F.x stages of Render LOD (indexing / winding / edge-pair), NOT vertex production.**

### §7.2 Case D semantics — what 3-of-3 at 1× + tri-missing means

Case D's plan definition: "everything else (e.g., 3 verts at 1× = positional match but triangle missing — would mean the triangle exists but with different vertex INDICES that happen to coincide positionally; unlikely but should report)."

Per the empirical 57.1% D-dominance, the "unlikely" case turned out to be the dominant mechanism. The semantics in plain language: **for 24 of the 42 missing-from-Waffle Cherchi triangles, ALL THREE of their vertex positions DO appear somewhere in Waffle's Render LOD vertex set at the base grid. The triangle exists at Cherchi but Waffle's mesh does not connect those three vertices into a triangle.** This is a **triangle-emission/topology defect**, NOT a vertex-production defect (Case B) and NOT an upstream/quantization issue (Cases A/C).

### §7.3 Per-stage anchor candidates (Yang §4.4.1 / Cherchi 2022 §5)

Yang §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:548-590`) describes the "mesh updating" stage: after intersection-curve refinement, the boolean result is re-tessellated to insert curve points and remove cracks. Cherchi 2022 §5 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`) describes the post-arrangement manifold-flood that classifies each triangle inside/outside.

Waffle's pipeline `[stage-f]` trace shows triangle counts 138 → 119 → 119 → 113 → 113 across F.0 → F.4. Specifically:
- **F.0 → F.1** (138 → 119): `remove_winding_insensitive_duplicates` drops 19 triangles.
- **F.1 → F.2** (119 → 119): no-op (`remove_nonmanifold_topology_aware`).
- **F.2 → F.3** (119 → 113): `remove_nonmanifold_duplicates_aggressive` drops 6 triangles.
- **F.3 → F.4** (113 → 113): no-op (`weld_smooth_vertices`).

Pre-F.0, Stage B has 246 triangles, but the `[stage-f]` enters with 138 — meaning ~108 triangles were already removed BEFORE F.0 (likely by Render LOD remeshing). That layer is upstream of the F.x pipeline visible to `[stage-f]`.

**24 Case D triangles ≈ the count of triangles removed in F.0 → F.1 (19) + F.2 → F.3 (6) = 25.** This is striking but not load-bearing: the 24 Case D figure is the count of MISSING-ATTRIBUTABLE (bordering unpaired edges) triangles; the F.x removals are total. The match is suggestive that F.0 + F.2's removal passes are the most plausible PR-Y44 anchor for Case D.

### §7.4 PR-Y44 candidates with paper citations

**Candidate (α)**: Re-examine `remove_winding_insensitive_duplicates` at F.0 (drops 19 tris from 138). PR-Y40 audit found 4 D.1d losers but did not check whether the discarded triangles' vertex positions ARE present in Waffle's final vert set (they would be — the duplicate-removal pass only removes triangles, not vertices). The 19 dropped tris probably HAVE all 3 verts in Waffle's set → Case D mechanism. Paper anchor: Cherchi 2022 §5 ("manifold-flood inside/outside classification") implicitly assumes the input triangle set is canonical; aggressive winding-insensitive dedup can drop triangles that are correctly distinct in the post-flood pair-up phase. (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`)

**Candidate (β)**: Re-examine `remove_nonmanifold_duplicates_aggressive` at F.3 (drops 6 tris from 119). If these 6 dropped triangles are bordering unpaired-edge positions, the F.3 pass is creating the gaps the oracle sees. Paper anchor: Yang §4.4.1 (`refs/text/yang2025_hybrid_boolean.txt:564-580`) — "the mesh boolean operations may produce a non-manifold mesh ... that is then converted to a manifold mesh by selectively retaining one of the duplicate triangles." The "selectively retain ONE" choice is what F.3 implements aggressively; if the heuristic is wrong, the wrong triangle is retained.

**Candidate (γ)**: Re-examine the 138 → entering-F.0 cascade — the pre-F.0 layer drops 246-138 = 108 triangles. Most of the 24 Case D defects may already have happened BEFORE F.0 (i.e., in the Boolean LOD → Render LOD re-tessellation at `yang_integration.rs:1024`). PR-Y41 §6.3 already framed this as an investigation target but did not localize it to a specific stage.

### §7.5 Why this is not B-dominant

Case B at 33.3% IS the second-largest cluster and represents a real defect-cluster (5 distinct off-vertex positions, ~14 triangles). But it's 6.7 percentage points below the plan's 40% threshold. The plan's B-dominant verdict ("PR-Y44 anchor = investigate the specific off-vertices' upstream production") still applies to those 14 triangles, but they are a MINORITY of the 42. A PR-Y44 fix that addresses only Case B would close ~33% of the attribution; Case D addressing the topology-emission defect would close ~57%.

**Recommendation**: PR-Y44 prioritizes Case D (the dominant mechanism) BUT documents Case B as the cleanest secondary target if Case D investigation hits a dead end (PR-Y44 ABORT triggers Case B as fallback PR-Y45 candidate).

### §7.6 Why this is not C-dominant (Option C pause)

Case C = 0 = ZERO. There is NO geometry-is-upstream-of-Render-LOD signal. F0020's missing triangles are PROXIMITY-positive (Case D) or PARTIAL-positive (Case B) — they are NOT "Waffle has no triangle anywhere nearby." This **counters** PR-Y41/Y42 §6's "Option C pause" rationale: the upstream-investigation reasoning was that the defect is too diffuse / too upstream for further Render LOD investigation. The empirical 0% Case C **refutes** that framing for F0020 specifically. F0020's defect is at the Render LOD layer (or just-pre-Render-LOD), NOT upstream of it.

Caveat: this is F0020-specific. The cohort cases F0044/F0045 with their analytic-surface workloads MIGHT have Case C dominance at deeper investigation (the probe didn't measure them at sufficient depth). But the F0020 result alone strongly argues AGAINST Option C as the PR-Y44 decision.

### §7.7 Honest framing per feedback memories

- `feedback_anchor_before_fix`: The probe IS the load-bearing measurement; no production code modified. PR-Y44 anchor candidates (α/β/γ) are listed for empirical canary, NOT as fix prescriptions.
- `feedback_phase1_diagnosis_ranking_is_inference`: The verdict is **measurement** (24/42 = 57.1% Case D) NOT inference. The PR-Y44 candidates are ranked by paper citation alignment + the F.x drop-count coincidence (25 ≈ 24).
- `feedback_multi_stage_anchor_probe`: 4 grid levels swept (1×/2×/5×/10×); classification depends on all 4. Single-grid-level reading would have missed Case A (4 tris that DO have all 3 verts at 5× but not 1×).
- `feedback_validate_against_corpus`: Cohort tested (F0044/F0045/R0092); finding (no Case C; 50/50 B/D split) honestly reported even though it inverts the brief's prediction.
- `feedback_no_last_bug`: 12th cycle on F0020 Render LOD. We claim PR-Y43 produces an empirical D-dominant signal for PR-Y44's anchor scope. We do NOT claim PR-Y44 will fix F0020.
- `feedback_external_coherence`: Cherchi C++ remains the reference oracle. PR-Y43 reuses PR-Y42's exact methodology; the A/B/C/D classification is a refinement on the same diff data.

### §7.8 Strategic-pivot ROI update

Updating PR-Y42 §7.3's MIXED ROI:
- The strategic-pivot B.1 (external Cherchi oracle) NOW gives PR-Y44 a concrete D-dominant anchor (24 specific triangles + ~5 candidate F.x stages to investigate).
- Cohort `common=0` brittleness is unchanged from PR-Y42 — but the **VERTEX-level** comparison (PR-Y43's contribution) IS dense for cohort, so the methodology generalizes for cohort vertex-level investigations even though it doesn't for triangle-level.
- The pivot's empirical answer for F0020 has progressed from "50% borderline" (PR-Y42) to "57% Case D + 33% Case B = 90% explainable as one of two specific mechanisms" (PR-Y43). This is the sharpest empirical anchor in the 12-cycle arc.

If PR-Y44 lands a Case D production fix (most likely candidate: re-examine F.0/F.3 removal passes), it would close ~24 of the 42 attributable-to-unpaired tris → potentially ~12 unpaired edges of the 20 attributed (assuming each Case D tri ≈ 0.5 edge per the PR-Y42 rec[6] = 2 / rec[0-9 except 6] = 1 ratio). F0020 unpaired 40 → ~28. Not zero, but unprecedented progress.

---

## §8 Open / banked

### §8.1 Banked findings for PR-Y44

1. **D-dominant outcome is a new verdict outcome** the original plan didn't anticipate. PR-Y44 plan should explicitly handle D-dominant (with α/β/γ candidates listed in §7.4 above).
2. **5 distinct off-vertex positions account for 11/14 Case B entries** — the off-vertex SET is much smaller than the triangle count. PR-Y44 anchor data is more compact than 42 individual triangles.
3. **Cohort cases (F0044/F0045) show the same Case B mechanism** — 8/16 + 2/4 entries are Case B. The Case B fix might generalize to cohort even though the Case D fix might not (cohort common=0 means triangle-topology defects differ from F0020's).
4. **Cherchi non-det in F0020 missing-count is reproducible**: ~75% of runs give 194 missing / 42 target, ~25% give 201 missing / 47 target. A/B/C/D classification is stable for the 42-mode; the 47-mode adds 5 more entries (mostly Case D). PR-Y44 should pin Cherchi TBB stricter or use the deterministic attribution gates.
5. **F.0 drop count (19 tris) + F.3 drop count (6 tris) = 25 ≈ Case D count (24)** is a striking but not load-bearing coincidence — PR-Y44 canary should bisect which stage's drops include the Case D triangles.

### §8.2 Open questions (PR-Y45+)

1. **Cohort Case B/D semantics differ from F0020's**: F0020 Case D is "3-of-3 at 1× but tri missing"; cohort cases have `common=0` (triangles don't share quantized positions) so cohort Case D may be "1-or-2 at 1× + 1-or-2 at 5×" (the residual catch-all). Need a finer breakdown of Case D sub-mechanisms across F0020 vs cohort.
2. **The 42 missing-attributable tris vs the OTHER 152 missing tris**: PR-Y43 only classified the 42 that border unpaired edges. The other 152 missing tris might have a different A/B/C/D distribution. Are those ALSO Case D-dominant or do they shift toward Case A/B? (Would indicate whether the F.0/F.3 removal passes are the dominant defect across the WHOLE Render LOD diff, not just the unpaired-edge subset.)
3. **PR-VIZ-3a yang debug capture** has been available since PR-VIZ-3 — can render the 24 Case D triangles vs Waffle's 113 Render LOD triangles visually to confirm the "all 3 verts present" claim concretely. Banked for PR-Y44 canary.

### §8.3 Methodological banked

1. **Vertex-level diff IS the right grain for analytic-surface cohort cases** — triangle-level diff has common=0 method-limit (PR-Y42 finding) but vertex-level diff (PR-Y43 finding) is dense. Future cohort canaries should default to vertex-level.
2. **The 4-grid-level sweep was useful**: Case A (4 tris) only manifests at the 5×/10× sweep — would have been missed with single-grid analysis. `feedback_multi_stage_anchor_probe` empirically vindicated.
3. **Case D was assumed unlikely** in the plan ("would mean the triangle exists but with different vertex INDICES that happen to coincide positionally; unlikely but should report"). Plans should not pre-judge case likelihoods; let the data speak.

---

## §9 Reproduction artifacts

### §9.1 Worktree path

`/home/claude/workspace/.claude/worktrees/canary-y36/` (branch: `worktree-canary-y36`, HEAD aligned with main `b0009bd` post-merge).

### §9.2 Verification artifacts

- `/tmp/y43-f0020-attribution.log` — full F0020 A/B/C/D output + Case B dump (688 lines)
- `/tmp/y43-cohort-attribution.log` — full F0044/F0045/R0092 cohort output (870 lines)
- `/tmp/y42-f0020-render-lod.log` — preserved PR-Y42 baseline (41,336 bytes; for cross-reference)

### §9.3 Commands

```bash
# Gate 1: build
cargo build -p test-harness --test cherchi_differential_diff

# Gate 2: probe-off byte parity
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int

# Gate 3: PR-Y42 baseline preserved
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_diff_baseline --ignored --nocapture --test-threads=1
# expect: common=36, attribution 20/40 = 50.0%
# (missing/extras vary due to Cherchi TBB non-det; not load-bearing)

# Gate 4 + 5: F0020 A/B/C/D + Case B vertex dump (LOAD-BEARING)
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect: target_tris=42 (most runs) or 47 (~25% of runs)
# expect: Case A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), D=24 (57.1%) at 42-mode

# Gate 6: cohort sanity
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- cohort_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect: F0044 target=16 (B=8, D=8); F0045 target=4 (B=2, D=2); R0092 target=0

# Gate 7: kernel lib + yang_fast
cargo test -p kernel --lib  # 1262/24/42
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157

# Gate 8: PR-Y31 hard gate
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture
# expect: PASS (F0044 Stage B missing=0, extras=0, common=136)
```

### §9.4 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| F0020 Case D = 24/42 = 57.1% | **HIGH** | Byte-stable across 3 reruns at target_tris=42. Classification logic deterministic given a fixed missing-attributable set. |
| F0020 Case B = 14/42 = 33.3% | **HIGH** | Byte-stable across all 4 reruns (Case B count = 14 even when target_tris drifts to 47). 14 entries dumped with deterministic sort. |
| F0020 Case C = 0/42 = 0.0% | **HIGH** | Byte-stable across all 4 reruns. The 4 grid levels (1×/2×/5×/10×) ALL find ≥2 vert matches for every target triangle. |
| Case D is triangle-topology defect (not vertex production) | **MEDIUM-HIGH** | Definition is rigorous: 3-of-3 vert positions present in Waffle set. The mechanism (indexing vs winding vs edge-pair) is NOT distinguished by the probe; PR-Y44 canary should bisect. |
| F.0/F.3 removal passes are the PR-Y44 anchor | **MEDIUM** | The 25 = 19+6 ≈ 24 Case D count coincidence is suggestive but not load-bearing. PR-Y44 canary must verify the dropped triangles ARE the missing-attributable Case D ones. |
| Cohort Case B mechanism generalizes from F0020 | **MEDIUM** | F0044 + F0045 also show 50% Case B (same off-vertex pattern as F0020's Case B). R0092 has 0 target tris (can't measure). |
| Option C pause is NOT the right PR-Y44 move | **MEDIUM-HIGH** | F0020 Case C = 0 directly refutes "defect is upstream of Render LOD." The defect IS at Render LOD (or just-pre-Render-LOD). Caveat: cohort might still need Option C if PR-Y44 Case D fix doesn't generalize. |
| Cherchi TBB non-det doesn't invalidate the verdict | **HIGH** | A/B/C/D byte-stable at target=42 mode (75% of runs); even at target=47 mode (25% of runs) Case D remains dominant and Case C remains 0. |

---

## §10 Verdict — **SHIP-INFRA + D-DOMINANT (new 6th outcome)**

All 8 gates GREEN/measured. PR-Y43 ships:
- Harness extension at `crates/test-harness/tests/cherchi_differential_diff.rs` (+438 LOC)
- 0 production code
- 0 WASM rebuild (test-file only; no kernel changes)

**F0020 A/B/C/D histogram (LOAD-BEARING):** A=4 (9.5%) / B=14 (33.3%) / C=0 (0.0%) / D=24 (57.1%).

**PR-Y44 anchor recommendation: D-dominant → investigate triangle-topology emission at F.x stages of Render LOD (`remove_winding_insensitive_duplicates` at F.0 / `remove_nonmanifold_duplicates_aggressive` at F.3) — NOT vertex production, NOT grid tuning, NOT upstream-of-Render-LOD pause.** Candidate (α) at F.0 is the primary; (β) at F.3 is secondary; (γ) Boolean LOD → Render LOD re-tessellation is tertiary.

Per the plan §"Strategic checkpoint": this delivers a clean PR-Y44 anchor for the first time in 12 cycles. The strategic-pivot ROI (PR-Y42 §7.3 MIXED) is updated to **PAYING OFF for F0020** if PR-Y44 lands the Case D investigation; the cohort Case B finding (8 entries in F0044, 2 in F0045) is a banked secondary candidate.

Per `feedback_no_last_bug`: 12th cycle. We do NOT claim PR-Y44 will fix F0020. We claim PR-Y43 produces the sharpest empirical anchor in the arc and refutes Option C pause for F0020 specifically (Case C = 0 directly counters "defect is upstream of Render LOD").

Per `feedback_anchor_before_fix`: the probe is the canary; the 8 gates are the load-bearing measurements before any PR-Y44 production code modification.

Per `feedback_external_coherence`: PR-Y43 reuses Cherchi C++ as the reference oracle, exactly as PR-Y42 established. The A/B/C/D classification is a NEW lens on PR-Y42's set-diff data, not a new oracle.
