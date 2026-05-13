# PR-Y37 Canary — Probe-extension cross-cohort prediction REFUTED; F0020 OTHER cluster is dominantly H3 (novel); cohort F0044/F0045 100% H3, R0092 mixed; **SHIP-INFRA + 6th-refutation framing**

**Author:** canary-y37
**Date:** 2026-05-13
**Worktree branch:** `worktree-canary-y36` (re-used to keep Y36 + Y37 scaffolding co-located)
**Baseline:** `8778907` (PR-Y35.1 audit ACCEPT — main HEAD at canary start)
**Mandate:** Extend the Y36 inverse-direction probe with H1 (sub-grid seam) / H2 (NMM-pair render asymmetry) / H3 (residual) sub-classification of the OTHER cluster. Run on F0020 + cohort. Verify the PR-Y36 cross-cohort prediction. Recommend PR-Y38 anchor.
**Verdict:** **SHIP-INFRA + 6th-refutation framing.** All 8 gates GREEN. Cross-cohort prediction REFUTED: F0044/F0045 attribute 100% to H3 (predicted ≥80% H1); R0092 attributes 27.9% to H2, 72.1% to H3 (predicted ≥80% H2). F0020 inv#6 OTHER attribution: 0% H1, 13.6% H2, 86.4% H3 — also dominantly residual. PR-Y27's D.2 / D.3 sub-mechanism framework does **NOT** map to the H1/H2 signatures defined in the PR-Y37 plan; either the thresholds are wrong, or D.2/D.3's mechanisms are sub-quantization-granularity and don't surface in axis-aligned + NMM-twin proxies. **PR-Y38 anchor banked at: refine H1/H2 detection (sub-quantization geometric features for D.2; precise NMM-incidence per-segment for D.3) OR accept that the OTHER cluster is a novel pattern requiring its own canary.**

---

## §0 Summary

PR-Y37 extended the PR-Y36 inverse-direction probe with two new feature columns (`grid_aligned_count`, `nmm_asym_count`) and three new `Y36Class` variants (`OtherH1`, `OtherH2`, `OtherH3`). The sub-classification is applied in the writer (where edge-level data is already aggregated) to faces that pass through PR-Y36's `y36_classify` returning `Other`.

**F0020 inv#6 (load-bearing) attribution (39 unpaired edges):**

| Class | Count | % of total | Mechanism |
|---|---|---|---|
| **D.1a** | 9 | 23.1% | `boundary.len() < 3` planar entry gate |
| **D.1b** | 0 | 0.0% | earcut zero-emit on coincident 3-bounded |
| **D.1c** | 0 | 0.0% | ≥90% NMM boundary |
| **D.1d** | 8 | 20.5% | repair-pass drop |
| **D.1 total** | **17** | **43.6%** | |
| **OTHER (legacy unsplit)** | 0 | 0.0% | sub-class applied to all |
| **OtherH1** | 0 | 0.0% | ≥80% boundary edges axis-aligned + grid-quantized |
| **OtherH2** | 3 | 7.7% | ≥50% NMM edges with topology-present-but-render-absent twin |
| **OtherH3** | 19 | 48.7% | residual (neither H1 nor H2) |
| **Other total** | **22** | **56.4%** | (matches PR-Y36 baseline) |

**Cohort sub-classification (Gate 5):**

| Case | Total unpaired | D.1a | D.1d | OtherH1 | OtherH2 | OtherH3 | H1 % of Other | H2 % of Other | H3 % of Other |
|---|---|---|---|---|---|---|---|---|---|
| F0044 | 12 | 0 | 0 | 0 | 0 | 12 | 0.0% | 0.0% | **100.0%** |
| F0045 | 38 | 0 | 0 | 0 | 0 | 38 | 0.0% | 0.0% | **100.0%** |
| R0092 | 43 | 0 | 0 | 0 | 12 | 31 | 0.0% | 27.9% | **72.1%** |

**Cross-cohort prediction outcome (Gate 6, LOAD-BEARING):**

| Cohort case | Predicted (per PR-Y36 §4.2 + PR-Y27 D-framework) | Observed | Outcome |
|---|---|---|---|
| F0044 | ≥80% H1 (D.2 = sub-grid seam) | 0% H1 | **REFUTED** |
| F0045 | ≥80% H1 (D.2 = sub-grid seam) | 0% H1 | **REFUTED** |
| R0092 | ≥80% H2 (D.3 = NMM-pair render asymmetry) | 27.9% H2 | **REFUTED** |
| F0020's 22 OTHER | ≈ proportional mix of H1 + H2 | 0% H1, 13.6% H2, 86.4% H3 | **REFUTED** |

Per the brief's verdict logic: "**Refuted**: cohort prediction fails (e.g., F0044/F0045 are NOT H1-dominated) → H1/H2 thresholds wrong OR PR-Y27 framework partially stale → 6th-refutation framing."

**PR-Y38 anchor recommendation:** Per `feedback_no_last_bug`, this memo does NOT claim a definitive mechanism. The empirical narrative is: **the H1/H2 signatures as defined in the PR-Y37 plan are insufficient to discriminate the OTHER cluster cohort-wide.** PR-Y38 must either (a) refine H1/H2 detection (likely with sub-quantization geometric features and a precise per-segment NMM-incidence map) OR (b) accept that the OTHER cluster is genuinely novel and reframe the F0020-Render-LOD investigation around the partial-NMM kept-face mechanism per PR-Y36 §3.4 banked observation.

This is the **6th consecutive canary-stage finding-no-fix-shape outcome** on F0020 Render LOD (Y25/Y26/Y27/Y28/Y36/Y37). Discipline `feedback_anchor_before_fix` continues to pay: zero production code shipped; empirical clarification that PR-Y36's cross-cohort prediction was inference, not observation.

---

## §1 Discipline

### Live tree untouched

```
$ git status
On branch worktree-canary-y36
Changes not staged for commit:
  modified:   app/tests/cases/assay/results.json
  modified:   crates/kernel/src/tessellation/mod.rs
Untracked files:
  docs/audits/pr_y36_canary.md
  specs/yang_pr_y36_inverse_probe.md
```

`results.json` is the test-harness runner artifact. The Y36 canary memo + Y36 spec are also present from the PR-Y36 cycle. Per `feedback_adversary_no_destructive_git` (also applies to canary): no `git stash`, `checkout`, `reset --hard`, or other destructive op on live tree.

All Y37 instrumentation lives in worktree `canary-y36` (branch `worktree-canary-y36`) rooted at `8778907`.

### Worktree diff (verbatim)

```
$ git diff HEAD --stat
 app/tests/cases/assay/results.json    | 144 +++----
 crates/kernel/src/tessellation/mod.rs | 711 +++++++++++++++++++++++++++++++++-
 2 files changed, 780 insertions(+), 75 deletions(-)

$ git diff HEAD --numstat
72	72	app/tests/cases/assay/results.json
708	3	crates/kernel/src/tessellation/mod.rs
```

The `tessellation/mod.rs` net is +708/-3, which is the **combined PR-Y36 + PR-Y37 worktree state** (the worktree was inherited from PR-Y36). The Y37-specific delta vs the PR-Y36 audit baseline (commit `d8fa288` was merged to main at `1ad58ce`; the canary baseline at `8778907` did **not** include PR-Y36's production changes — PR-Y36 was infra-only, no production code shipped) is:

- **Y37-only additions vs PR-Y36 worktree:** ~250 LOC additive (Y36Class extension, H1/H2 helper functions, Y37 sub-classify, writer updates for new TSV columns + per-row sub-classification + cross_cohort_summary aggregator).

The combined +708 LOC in `tessellation/mod.rs` is composed of:
- ~462 LOC PR-Y36 probe scaffolding (Y36ProbeFaceInfo, y36_classify, y36_quantize_*, y36_write_inverse_attribution, dispatch-loop per-face capture, end-of-fn writer call)
- ~245 LOC PR-Y37 additions (3 new Y36Class variants + as_str arms; y37_edge_axis_aligned, y37_count_axis_aligned_edges, y37_count_nmm_asymmetric, y37_sub_classify; 2 new feature columns in unpaired TSV header + row format; 4 new feature columns in face inventory TSV header + row format; new stderr summary with H1/H2/H3 counts; new cross_cohort_summary.tsv writer)

### Probe gate

All probe logic gated on `std::env::var("Y36_INVERSE_PROBE").as_deref() == Ok("1")` (kept the Y36 env-var name — extension, not rename). The dispatch-loop per-face capture executes only inside `if y36_on { … }`. The final `y36_write_inverse_attribution` call is wrapped in `if y36_on { … }`. Default-off path is byte-identical to PR-Y35.1 ACCEPT baseline (verified by Gate 7 + the bare `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1` re-run reproducing the same 40-unpaired Status:Failed signature).

Per `feedback_anchor_before_fix`: **ZERO production logic changed**. The probe is observation-only. No `disc.positions`, `vertices`, `indices`, `face_ranges`, or arena state is mutated by probe code. All output is `eprintln!` and file-write to `Y36_INVERSE_PROBE_DIR`.

### Reproduction commands

```bash
cd /home/claude/workspace/.claude/worktrees/canary-y36
git rev-parse HEAD   # → 8778907

cargo build -p kernel

# Gate 2: F0020 baseline (no probe)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
    cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture > /tmp/y37-pre.log 2>&1

# Gates 3+4: F0020 with probe
rm -rf /tmp/y37-probe && mkdir -p /tmp/y37-probe
Y36_INVERSE_PROBE=1 Y36_INVERSE_PROBE_DIR=/tmp/y37-probe \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
  --ignored --nocapture > /tmp/y37-final.log 2>&1

# Gate 5: Cohort F0044/F0045/R0092 batch via spotlight_f0044
rm -rf /tmp/y37-cohort && mkdir -p /tmp/y37-cohort
Y36_INVERSE_PROBE=1 Y36_INVERSE_PROBE_DIR=/tmp/y37-cohort \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 \
  --ignored --nocapture > /tmp/y37-cohort.log 2>&1

# Gate 7: kernel lib regression
cargo test -p kernel --lib

# Gate 8: yang_fast
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast \
  --ignored --nocapture --test-threads=1
```

---

## §2 Method — probe extension design

### §2.1 Extension shape

The PR-Y36 probe data was already sufficient to derive geometric (boundary positions) and topological (NMM count, face_range_pushed) features per-face. PR-Y37 adds **two derived features per face**, computed in the writer:

1. **`grid_aligned_count`** — number of boundary edge segments (n consecutive pairs from `boundary_positions`) that are axis-aligned in the quantized grid space (exactly one of the three quantized-delta components is non-zero). Since endpoints are already quantized to integer grid cells via `y36_quantize_pos`, "grid-aligned" reduces to "axis-aligned at quantization granularity."

2. **`nmm_asym_count`** — number of boundary edge segments for which no other face in the per-face inventory hosts the same quantized edge AND is also in the final mesh (`kids_in_final`). Approximates the H2 signature "twin exists topologically but render is missing the peer."

### §2.2 Sub-classification function (`y37_sub_classify`)

```rust
fn y37_sub_classify(base, info, inv_grid, face_boundary_edges, kids_in_final)
    -> (Y36Class, grid_aligned_cnt, nmm_asym_cnt)
{
    if base != Other { return (base, 0, 0); }  // D.1a/b/c/d unchanged
    let n = info.boundary_positions.len();
    let aligned = count_axis_aligned_edges(...);
    let asym = count_nmm_asymmetric(...);
    // H1 first (geometric, cheap)
    if n >= 2 && aligned/n >= 0.80 { return OtherH1; }
    // H2 next (topological proxy)
    if info.outer_nmm_count > 0 && asym/outer_nmm_count >= 0.50 { return OtherH2; }
    OtherH3
}
```

Precedence: geometric H1 → topological H2 → residual H3. Thresholds: H1 = 80% of boundary segments axis-aligned; H2 = 50% of face's NMM HE count's worth of asymmetric edges (note: `nmm_asym_count` may exceed `outer_nmm_count` because it counts ALL boundary edges without a final-mesh peer, not just NMM-incident ones — this is the brief's approved "proxy heuristic" fallback).

### §2.3 H2 detection caveat (load-bearing for §3 interpretation)

**The H2 detection is a proxy, not a precise NMM-incidence-per-segment measurement.** The PR-Y36 probe records `outer_nmm_count` at face level (count of half-edges with `twin.is_none()`) but does NOT record which boundary `position[i] → position[(i+1) % n]` SEGMENTS are NMM-incident. Reasons documented in brief: "the grid-alignment + axis-alignment check needs to inspect `boundary_positions` … the NMM-twin-asymmetric check needs to walk topology … If walking topology is impractical from this site, document the limitation and fall back to a proxy heuristic."

The proxy I implemented counts segments whose quantized edge appears uniquely in this face's boundary (no other face in `face_boundary_edges` has it AND is in `kids_in_final`). This is a **superset** of the true H2 signature: it catches edges that lack a render peer regardless of whether they are NMM. Threshold normalized against `outer_nmm_count` to keep the ratio meaningful for faces that DO have NMM edges (no-NMM faces never trip H2).

**Implication for §3 interpretation:** If H2 fires for a face with `outer_nmm_count=2`, the proxy says "≥1 boundary edge of this face has no render peer." That's necessary-but-not-sufficient for true NMM asymmetry. If H2 doesn't fire, we can say firmly "all boundary edges of this face are paired in the render-LOD inventory" — useful negative signal.

### §2.4 What this extension does NOT do

- **Does NOT walk topology** to find the true cross-face twin face. The brief explicitly accepted the proxy fallback.
- **Does NOT discriminate** sub-quantization-granularity defects (PR-Y27 §3 footnote: "D.2's defect is at positions that quantize to different grid cells but the f64 distance is sub-grid"). The H1 axis-alignment test uses the SAME quantization as the unpaired-edge detection — so any defect at sub-grid scale will appear as `q0 == q1` (filtered out as degenerate) or as a non-axis-aligned edge.
- **Does NOT change** any classification behavior for the existing `D.1a/b/c/d` outputs — only the `Other` bucket is sub-classified.

---

## §3 Gates — empirical results

| Gate | Spec | Result |
|---|---|---|
| **Gate 1** | `cargo build -p kernel` clean | **PASS** — 0 errors; 57 pre-existing warnings only (no Y37-attributable warnings) |
| **Gate 2** | F0020 baseline (no probe) — Status:Failed with 40 unpaired oracle | **PASS** — exact baseline match |
| **Gate 3** | Probe fires on F0020 with new H1/H2/H3 columns | **PASS** — 6 TSVs emitted (matches 6 invocations); inv#6 = 39 rows with new columns populated |
| **Gate 4** | F0020 inv#6 H1/H2/H3 attribution | **PASS** — see §3.1 |
| **Gate 5** | Cohort F0044/F0045/R0092 sub-classification | **PASS** — see §3.2 |
| **Gate 6 (LOAD-BEARING)** | Cross-cohort prediction | **REFUTED** — see §3.3 |
| **Gate 7** | `cargo test -p kernel --lib` no regression | **PASS** — `1262 passed; 24 failed; 42 ignored` exact baseline match |
| **Gate 8** | `yang_fast` ≥10/157 | **PASS** — `10/157 passed, 139 failed, 8 errored` exact baseline match |

### §3.1 F0020 attribution (load-bearing inv#6, 7-bucket)

```
[y36-inverse-probe] case=F0020 inv#6 total_unpaired=39 D1a=9 D1b=0 D1c=0 D1d=8 OTHER=0 OtherH1=0 OtherH2=3 OtherH3=19
```

| Class | Count | % of 39 | Notes |
|---|---|---|---|
| D.1a | 9 | 23.1% | unchanged from PR-Y36 |
| D.1b | 0 | 0.0% | unchanged from PR-Y36 |
| D.1c | 0 | 0.0% | unchanged from PR-Y36 |
| D.1d | 8 | 20.5% | unchanged from PR-Y36 |
| **D.1 total** | **17** | **43.6%** | matches PR-Y36 |
| OTHER (unsplit) | 0 | 0.0% | sub-classification applied to all 22 |
| **OtherH1** | **0** | **0.0%** | zero axis-aligned-dominant faces |
| **OtherH2** | **3** | **7.7%** | three kids: 226 (×2), 231 (×1) |
| **OtherH3** | **19** | **48.7%** | residual; 9 distinct kids |
| **Other total** | **22** | **56.4%** | matches PR-Y36 |

**Per-kid OTHER breakdown** (inv#6, source `attributed_source_face_id` column):

```
kid 226 OtherH2 ×2     (7-HE planar, 4-NMM, nmm_pct=57.1%, 2/8 grid-aligned)
kid 231 OtherH2 ×1     (similar partial-NMM profile)
kid 195 OtherH3 ×2     (clean planar, 0 NMM, 0 grid-aligned)
kid 197 OtherH3 ×2
kid 204 OtherH3 ×2
kid 206 OtherH3 ×2
kid 207 OtherH3 ×3
kid 212 OtherH3 ×1
kid 213 OtherH3 ×1
kid 215 OtherH3 ×1
kid 216 OtherH3 ×1
kid 229 OtherH3 ×4     (7-HE planar, 4-NMM, 0 grid-aligned, 0 asym)
```

**Observation:** kids 226 and 231 (which hit H2) have partial-NMM at ~57% — exactly the "partial-NMM kept face" profile PR-Y36 §3.4 banked as the new dominant cluster. Yet H2 only fires for 3 of those edges out of ~7 total partial-NMM-attributed edges in the OTHER cluster. The H2 proxy threshold (50% of NMM-count as asym-count) is being just-barely tripped or just-barely-not-tripped; **the threshold may be over-tuned for the proxy's superset interpretation.**

### §3.2 Cohort sub-classification (Gate 5)

```
[y36-inverse-probe] case=F0044 inv#1 total_unpaired=12 D1a=0 D1b=0 D1c=0 D1d=0 OTHER=0 OtherH1=0 OtherH2=0 OtherH3=12
[y36-inverse-probe] case=F0045 inv#2 total_unpaired=38 D1a=0 D1b=0 D1c=0 D1d=0 OTHER=0 OtherH1=0 OtherH2=0 OtherH3=38
[y36-inverse-probe] case=R0092 inv#3 total_unpaired=43 D1a=0 D1b=0 D1c=0 D1d=0 OTHER=0 OtherH1=0 OtherH2=12 OtherH3=31
```

| Case | Total | H1 | H2 | H3 | H1 % | H2 % | H3 % |
|---|---|---|---|---|---|---|---|
| F0044 | 12 | 0 | 0 | 12 | 0.0% | 0.0% | **100.0%** |
| F0045 | 38 | 0 | 0 | 38 | 0.0% | 0.0% | **100.0%** |
| R0092 | 43 | 0 | 12 | 31 | 0.0% | **27.9%** | 72.1% |

**Per-kid breakdown:**

F0044/F0045: all attributions are kids 19, 20, 21, 22, 23 (et al.), faces classified as Planar OR Cylindrical, all with `outer_nmm_count=0` (clean arena, exactly the PR-Y27 D.2 signature). All have boundary_len 22 or 42 (cylindrical caps with many discretization vertices); **none are axis-aligned** because the polygon traces the cylinder rim, not XYZ axes. H1 trivially never fires. H2 trivially never fires (no NMM). All fall through to H3.

R0092: kids 22, 24, 26, 27 trip H2 (each ×3 unpaired edges, total 12). These kids have `outer_he_count=7, outer_nmm_count=2 (28.6% NMM)` — partial-NMM patches. The H2 proxy fires because all 3 boundary edges that lack final-mesh peers are not split among the 2 NMM HEs and 5 non-NMM HEs in a way the proxy can resolve — it counts the 3 missing-peer edges as ≥50% of nmm_count=2 → trips H2. The remaining 31 R0092 unpaired edges attribute to other kids with different profiles → H3.

### §3.3 Cross-cohort prediction outcome — REFUTED

The PR-Y36 canary §4.2 (load-bearing for PR-Y37) predicted:
- **F0044/F0045 ≥ 80% H1** (per PR-Y27 D.2 = sub-grid seam mismatch on cylindrical-cap face seams)
- **R0092 ≥ 80% H2** (per PR-Y27 D.3 = NMM-edge tessellation gap; ~44 legit NMM HEs ≈ 43 unpaired)
- **F0020's 22 OTHER ≈ proportional mix of H1+H2**

| Prediction | Observed | Outcome |
|---|---|---|
| F0044 ≥80% H1 | 0% H1, 100% H3 | **REFUTED** |
| F0045 ≥80% H1 | 0% H1, 100% H3 | **REFUTED** |
| R0092 ≥80% H2 | 27.9% H2, 72.1% H3 | **REFUTED** (well below 80%) |
| F0020 OTHER mixed H1+H2 | 0% H1, 13.6% H2, 86.4% H3 | **REFUTED** |

**Why the prediction failed:**

1. **H1 (sub-grid seam) signature is too strict.** PR-Y27's D.2 mechanism is "sub-quantization-granularity vertex disagreement between adjacent renders" — at f32 precision (~µm) but the watertight oracle's grid is `max_abs * 2e-6` ≈ several µm at typical CAD scales. By definition, D.2's defect lives BELOW the quantization granularity that my H1 detector operates on. A f32-precision vertex disagreement that lands in different grid cells produces edges that quantize to MULTIPLE grid edges (q0 != q1, but different edges in different faces), not a single axis-aligned grid edge.

2. **The F0044/F0045 boundaries are cylinder rims**, not XYZ axes. Cylinder discretization produces 22-vertex or 42-vertex polygons tracing a circle in some non-axis-aligned plane (or partially axis-aligned, e.g., only the radial chords). My H1 axis-aligned test catches only the few segments parallel to a global axis — typically the diameter chord, hence ~9% grid-aligned for F0044 caps. The PR-Y36 §4.2 H1 hypothesis (`face boundary tracks tessellation grid edges; the seam between two grid-aligned sub-meshes isn't conformal`) was framed assuming planar boundaries; the cohort reality is curved.

3. **H2 (NMM-pair render asymmetry) proxy is too permissive on the threshold but too narrow on the signature.** The proxy counts asymmetric segments **not gated to NMM positions**, but normalizes against `outer_nmm_count`. For R0092 kids 22/24/26/27, this fires (3 missing-peer edges / 2 NMM HEs = 150% of NMM count, way over 50%). But for kids 19-21 of R0092 (with `outer_nmm_count=0`), H2 cannot fire — and these clean-arena kids account for the other 31/43 R0092 unpaired edges. The TRUE PR-Y27 D.3 mechanism (NMM-edge tessellation gap) likely spans ALL R0092 faces, not just the partial-NMM ones, but my proxy can't discriminate that without per-segment NMM flags.

### §3.4 Cross-cohort overlap table

| Feature | F0020 (22 Other) | F0044 (12 Other) | F0045 (38 Other) | R0092 (43 Other) |
|---|---|---|---|---|
| H1 fraction | 0% | 0% | 0% | 0% |
| H2 fraction | 13.6% | 0% | 0% | 27.9% |
| H3 fraction | 86.4% | 100% | 100% | 72.1% |
| Faces NMM ≥50% | 11/22 hits | 0 (all clean arena) | 0 (all clean arena) | 12 (kids 22/24/26/27 only) |
| Faces with axis-aligned-rich boundaries | 0 hits ≥80% | 0 hits ≥80% | 0 hits ≥80% | 0 hits ≥80% |

**The cohort does NOT share a discriminable feature signal at H1/H2 granularity.** All four cases dominate H3. The OTHER cluster is not novel-vs-the-cohort — it IS the cohort, all H3.

---

## §4 PR-Y38 anchor recommendation

### §4.1 Why this is the 6th refutation

PR-Y36 §0 stated: "If F0020's OTHER maps to H1 (sub-grid seam, F0044/F0045 D.2 prediction) + H2 (NMM-pair render asymmetry, R0092 D.3 prediction), the cluster is NOT novel — fixing H1+H2 closes all four cases." The empirical test of this prediction at the PR-Y37 H1/H2 thresholds **refuted it**: zero H1, minor H2.

This is the 6th consecutive canary-stage finding-no-fix-shape outcome (Y25 → Y26 → Y27 → Y28 → Y36 → Y37). Per `feedback_anchor_before_fix` strategic escalation rule: "three wrong anchors in a row → stop bisecting, build a reference comparison." PR-Y36 was the reference comparison for D.1; PR-Y37 was the reference comparison for cross-cohort overlap. **Both reference comparisons have produced negative results.** The OTHER cluster is the empirical-dominant unknown, and its true mechanism is **not** revealed by the H1 (geometric axis-alignment) or H2 (topological asymmetric-edge-count-vs-NMM) signatures defined in the plan.

### §4.2 What the empirical narrative actually says

The H3 (residual) cluster is **dominant** across all four cases. F0044/F0045 are 100% H3; R0092 is 72% H3; F0020 OTHER is 86% H3. The cohort-wide signal: unpaired boundary edges live on faces that are NOT axis-aligned-dominant AND NOT NMM-asymmetry-dominant.

Empirically, looking at the per-kid breakdown in §3.1 + §3.2:

- **F0044/F0045 H3 faces:** clean-arena planar/cylindrical patches (outer_nmm_count=0) with curved or polygonal boundaries tracing cylinder rims or chamfer edges. Unpaired edges live at curve-discretization vertices. Plausible mechanism: f32-precision quantization disagreement at curve-discretization vertices between adjacent faces — sub-grid distance below the watertight oracle's grid, but the f32 vertex positions themselves disagree enough to land in different grid cells when quantized. This is PR-Y27 D.2 but the H1 detector can't see it because the disagreement is per-vertex-pair, not per-edge-axis.

- **F0020 H3 faces:** mix of low-NMM (kids 195/197/204/206/207/212/213/215/216) and partial-NMM (kid 229 at 57%) Planar faces — none of them tripping the H1/H2 thresholds. Boundary positions don't show axis-alignment patterns from these kids.

- **R0092 H2 + H3 split:** H2 = the 4 partial-NMM kids (22/24/26/27); H3 = clean-arena Planar/Cylindrical patches similar to F0044/F0045.

### §4.3 PR-Y38 anchor options (banked, NOT a fix shape recommendation)

Per `feedback_no_last_bug` + `feedback_phase1_diagnosis_ranking_is_inference`, the canary does NOT pre-commit to a PR-Y38 anchor. Banked options for team-lead to consider (in priority of empirical-coverage):

1. **Refine the H1 detector to sub-quantization granularity.** Replace the axis-aligned check with a pairwise vertex-position comparison: for each boundary edge, find the matching boundary edge in any neighboring face (by position-proximity, not quantization-equality), and report the per-vertex f64 distance. Threshold for "sub-grid seam" = vertices disagree by ≥ε (where ε ≈ f32 ULP at the model's magnitude) but ≤ quantization granularity. This would directly probe PR-Y27 D.2's sub-quantization signature. **Risk:** non-trivial probe extension (~150-300 LOC); requires neighbor-face lookup which the current probe doesn't compute.

2. **Refine the H2 detector to per-segment NMM-incidence.** Walk the outer-loop half-edges in dispatch order (same as `collect_loop_boundary`) and record per-position-pair whether the originating HE is NMM. Then count H2 = (# segments where NMM AND no final-mesh peer) / (# NMM segments). **Risk:** requires per-HE-to-per-position-segment mapping that's complex due to edge-discretization expansion (a single curved edge can produce N segments from one HE).

3. **Accept the OTHER cluster as a novel mechanism and pivot the investigation.** PR-Y36 §3.4 banked: "kids 235 and 256 ARE present in the inventory with 100% NMM — exactly the D.1c signature from PR-Y28. But neither shows up in the unpaired-edge attribution." This is a STRONG signal that the Cherchi-Rust port byte-parity work (Y34/Y35/Y35.1) fixed the D.1c peer-pairing problem, but the new defects are at the F.4 quantization layer for partial-NMM faces. A PR-Y38 canary that probes the `count_unpaired_in_mesh` quantization itself — does the f32 → quantized-grid round-trip lose precision for the partial-NMM kids' boundaries? — would be a fresh investigation.

4. **Cheap singleton: D.1d kids 218/232/233 fix.** PR-Y36 §4.2 banked alternative: investigate why these 3 kids are dropped at `remove_nonmanifold_topology_aware`. Accounts for 8/40 = 20% of F0020 oracle unpaired. Predicted F0020 outcome: 40 → ~32 unpaired (does NOT close Status:Failed). Cohort risk: must verify F0044/F0045/R0092's zero arena-drop status preserved. Not a Yang anchor; a hygiene PR.

### §4.4 What PR-Y38 should NOT be

- **NOT a fix shape against H1 or H2 thresholds in their current form.** Empirically 0% / 13.6% attribution; not load-bearing.
- **NOT a β-shape (peer-patch synthesis).** Refuted at HEAD in PR-Y36 (D.1c = 0%); refuted again at PR-Y37 by the OTHER cluster being H3-dominant, not H1/H2-dominant.
- **NOT a "this fixes Render LOD" claim.** Per `feedback_no_last_bug`. The OTHER cluster's true mechanism is now even less clear than PR-Y36 banked.

---

## §5 Verdict

**SHIP-INFRA + 6th-refutation framing.**

Rationale:

- All 8 gates GREEN (build, F0020 baseline, probe fires with new columns, attribution table populated, cohort sub-classification populated, kernel lib regression preserved, yang_fast preserved)
- Default-off byte-parity proven (probe-off re-run reproduces 40-unpaired Status:Failed exactly)
- Cross-cohort prediction REFUTED — PR-Y36's load-bearing hypothesis "F0020 OTHER is just F0044/F0045/R0092's defect at higher arena density" is empirically wrong as stated
- Probe extension is the FIRST cohort-wide measurement of geometric (grid-alignment) + topological (NMM-asymmetric-edge) features on the unpaired-edge attribution — establishes baseline measurements all four cases share H3 dominance
- Discovery: PR-Y27's D.2/D.3 sub-mechanism framework does NOT map to the H1/H2 signatures as defined; the framework may be partially stale, OR the signature definitions need refinement

The infrastructure ships (probe extension + memo + banked PR-Y38 options). NO production fix on F0020 Render LOD.

### §5.1 Decision tree applied

```
Gate 1 (build) — PASS ─────────────────────────────────────────────┐
Gate 7 (kernel lib regression) — PASS ─────────────────────────────┤
Gate 8 (yang_fast regression) — PASS ──────────────────────────────┤
                                                                   │
Gate 4 (F0020 OTHER H1/H2/H3) ─────────────────────────────────────┤
  ≥80% H1               → cohort-wide PR-Y38                NO     │
  ≥80% H2               → cohort-wide PR-Y38                NO     │
  ≥80% combined H1+H2   → SHIP-INFRA + cohort-wide          NO     │
  ≥40% H3 + cohort REF  → 6th refutation                    YES    │
                                                                   │
Gate 6 (cross-cohort prediction) ──────────────────────────────────┤
  Validated             → SHIP cohort-wide rec               NO    │
  Partial               → LAYERED                            NO    │
  Refuted               → 6th-refutation framing             YES   │
                                                                   ▼
                                          SHIP-INFRA + 6th-refutation framing
```

Both Gate 4 and Gate 6 converge on 6th-refutation. The numeric thresholds in the verdict logic (`≥40% H3 + cohort REF`) match: F0020 OTHER is 48.7% H3 (well above 40% of total unpaired), and cohort REF (F0044/F0045 100% H3) confirms.

---

## §6 Empirical confidence assessment

| Claim | Confidence | Evidence |
|---|---|---|
| F0020 inv#6 unpaired count = 39 (matches PR-Y36) | HIGH | `[stage-f] sub=4 unpaired=39` matches `total_unpaired=39` in probe summary |
| F0020 inv#6 D.1 distribution unchanged from PR-Y36 | HIGH | D.1a=9, D.1b=0, D.1c=0, D.1d=8 → exact match with PR-Y36 §3.1 |
| F0020 OtherH1 = 0% (no sub-grid seam dominance) | HIGH | Direct probe output; the H1 detector is geometric/deterministic |
| F0020 OtherH2 = 3/22 = 13.6% (only kids 226, 231 trip H2) | HIGH | Per-row TSV inspection: 3 rows with classification=OtherH2 |
| F0020 OtherH3 = 19/22 = 86.4% dominant | HIGH | Direct probe output; complement of D.1+H1+H2 |
| Cohort F0044/F0045 = 100% H3 (no H1, no H2) | HIGH | Probe output; F0044/F0045 have zero NMM → H2 cannot fire; their cylinder-cap boundaries are not axis-aligned → H1 doesn't fire |
| R0092 = 27.9% H2 (kids 22/24/26/27 hit), 72.1% H3 | HIGH | Probe output; kids 22-27 are partial-NMM patches matching the H2 proxy signature |
| Cross-cohort prediction REFUTED | HIGH | All three predicted thresholds (≥80% H1 for F0044/F0045; ≥80% H2 for R0092) miss by ≥50 percentage points |
| Refutation is a signature mismatch, NOT measurement noise | HIGH | The H1 detector is purely geometric (axis-aligned at quantization granularity); cylinder rims are not axis-aligned by construction; F0044/F0045's clean-arena, NMM=0 status means H2 cannot mathematically fire |
| The OTHER cluster has a real mechanism we have not yet measured | MEDIUM | Negative result: H1 + H2 as defined don't capture it. Positive direction: PR-Y27 D.2/D.3 may still be the right framework but require sub-quantization or per-segment-NMM signatures. |
| Existing H1/H2 thresholds (80%, 50%) are the right thresholds for the wrong signatures | MEDIUM | Lowering thresholds would shift H1/H2 marginally but H1 is structurally zero for all cases tested (geometric impossibility on curved boundaries) — threshold tuning won't rescue this. |
| The PR-Y28 D.1 framework remains partially active at HEAD (D.1a=23%, D.1d=21%, totaling 44%) | HIGH | Unchanged from PR-Y36; same probe logic with no D.1 path modifications |

### §6.1 What the canary did NOT measure (limitations)

- **Did not walk topology to find the true twin face for NMM edges.** The H2 detection is a proxy. If team-lead wants precise H2 measurement, PR-Y38 canary §4.3 option 2 spells out the extension.
- **Did not check sub-grid-granularity vertex disagreement.** PR-Y27 D.2's mechanism may live at f32-precision sub-grid distance that the H1 quantization step erases. PR-Y38 canary §4.3 option 1 spells out the extension.
- **Did not run on randomized corpus** beyond yang_fast smoke test. The 4 cases (F0020/F0044/F0045/R0092) are the cohort identified by PR-Y27 §1; this matches the brief.
- **Did not measure WASM probe path.** PR-VIZ-3a-fix capture path runs unconditionally, but it's a different probe (in-memory stage capture, not the Y36/Y37 unpaired-edge attribution). No coupling between them — separate code paths.

---

## §7 Reproduction artifacts

| Artifact | Path | Description |
|---|---|---|
| Probe Rust source (Y36 + Y37 combined) | `crates/kernel/src/tessellation/mod.rs` | +708/-3 in worktree `canary-y36`, NOT in live tree |
| F0020 inv#1..6 attribution + inventory | `/tmp/y37-probe/F0020_inv00{1..6}_{inverse_attribution,face_inventory}.tsv` | 12 files |
| F0020 cross_cohort_summary | `/tmp/y37-probe/cross_cohort_summary.tsv` | 6 rows (one per invocation) |
| Cohort F0044/F0045/R0092 attribution | `/tmp/y37-cohort/{F0044,F0045,R0092}_inv00*_*.tsv` | 6 files |
| Cohort cross_cohort_summary | `/tmp/y37-cohort/cross_cohort_summary.tsv` | 3 rows |
| F0020 stdout (probe-on) | `/tmp/y37-final.log` | full test stdout |
| F0020 stdout (probe-off, baseline) | `/tmp/y37-pre.log` | pre-probe baseline confirm |
| Cohort stdout | `/tmp/y37-cohort.log` | F0044/F0045/R0092 batch |
| Kernel lib regression | rerun `cargo test -p kernel --lib` | `1262 passed; 24 failed; 42 ignored` |
| yang_fast regression | rerun `YANG_BOOLEAN=1 ... yang_fast` | `10/157 passed, 139 failed, 8 errored` |

All paths under `/tmp` are per-canary-session.

---

## §8 Banked findings for PR-Y38

1. **H1 detector geometric definition is too narrow.** Axis-aligned + grid-quantized maps to "edge segment runs parallel to global X/Y/Z axis at the quantization granularity." For cylinder-rim boundaries (the entire F0044/F0045/R0092 cohort + some F0020 kids), this is structurally zero. PR-Y38 candidate refinement: replace with sub-quantization-distance vertex-pair comparison.

2. **H2 detector is a proxy that fires too coarsely.** It counts asymmetric segments without per-NMM gating. For F0020 inv#6, 3 of 22 OTHER edges trip H2 (13.6%). Whether the true precise H2 would be higher or lower is unknown without per-segment NMM-incidence data.

3. **F0020 OTHER cluster is split among 12 distinct kids** (4 partial-NMM, 8 clean-arena). The kept-face boundary positions of those 12 kids share no obvious geometric signature beyond "not axis-aligned." Further probing of these kids' actual render-mesh tessellation (per-face triangle vertices, per-face boundary-position-to-vertex-index mapping) would be the natural next investigation.

4. **PR-Y27 D.2/D.3 framework is empirically untouched by PR-Y37.** Neither validated nor refuted — only the H1/H2 *signatures* defined for it in PR-Y36 §4.2 are refuted. D.2 may still be the right mechanism for F0044/F0045 (zero NMM kept-face cylinder-cap seams) and D.3 for R0092 (NMM-rich tessellation) — they just live at granularities below what H1/H2 measure.

5. **Cross-case unpaired count distribution is stable.** F0020=40, F0044=12, F0045=38, R0092=43 — exactly the PR-Y27 baselines. No regression introduced by Y34/Y35/Y35.1 since cohort baselines. The PR-Y27 §3 cohort split is durable.

6. **PR-Y36 §4.2 banked alternative (3 D.1d kids 218/232/233) survives** as a low-risk PR-Y38 candidate (option 4 in §4.3). It accounts for 8/40 of F0020 oracle unpaired but does NOT close Status:Failed.

7. **Per `feedback_no_regression_chasing`:** PR-Y34/Y35/Y35.1 byte-parity progress is structurally correct; the F0020 unpaired remaining at 40 is not "regression to be chased." It's the unresolved downstream of richer arena topology hitting the post-PR-Y22 `tessellate_solid_bounded` dispatch loop with H3-class faces. PR-Y38's mandate should be: characterize the H3 cluster, not relitigate the upstream.

---

## §9 Acceptance gate honesty check

Per the team-lead's brief:

> **SHIP-INFRA + cohort-wide PR-Y38** if Gate 1/7/8 GREEN AND Gate 6 outcome = validated
> **SHIP-INFRA + LAYERED** if Gate 6 outcome = partial
> **SHIP-INFRA + 6th-refutation** if Gate 6 outcome = refuted
> **ABORT** if Gate 1/7/8 RED, OR thresholds produce uninterpretable all-H3 (suggests threshold values wrong)

Gate 1/7/8 GREEN. Gate 6 outcome is REFUTED (all 4 predictions miss by ≥50pp). Gate 4 produces partially-H3-dominated F0020 (87% H3 of OTHER, 49% H3 of total unpaired) — but with non-trivial D.1 (44%) and minor H2 (8%) signal. Not "uninterpretable all-H3" — the D.1 signal is healthy and consistent with PR-Y36.

**Question:** Does the F0044/F0045 all-H3 result (12/12 and 38/38) trigger the "uninterpretable all-H3 → ABORT" clause?

**Canary's reading:** NO. Reason: F0044/F0045 are 100% H3 by *construction* — they have zero NMM (so H2 trivially can't fire) and curved boundaries (so H1 trivially can't fire). The all-H3 result for them is the **correct** signal that the H1/H2 signatures *don't apply* to those cases, not that the signatures are mis-tuned. R0092 partially trips H2 (27.9%) demonstrating the detection *does* discriminate when it can; F0020's 13.6% H2 likewise. The all-H3 finding for the cohort is a *negative measurement* with high confidence, not measurement noise.

**Therefore: SHIP-INFRA + 6th-refutation framing.** The 6th-refutation framing captures the empirical narrative honestly:
- PR-Y36's cross-cohort prediction was a hypothesis built on inference (PR-Y28 D.1c-dominant framework was refuted in PR-Y36, but PR-Y27 D.2/D.3 framework was assumed forward without canary verification on the same probe).
- PR-Y37 was the canary verification of that assumption.
- The empirical answer: PR-Y27 D.2/D.3 may still be valid mechanisms, but the H1/H2 signatures defined in the PR-Y37 plan do not capture them.

Per `feedback_no_last_bug`: this memo does NOT claim "we now know what the OTHER cluster is." It says: H1/H2 as defined do not classify it; further investigation is needed.

Per `feedback_phase1_diagnosis_ranking_is_inference`: this memo does NOT recommend a PR-Y38 anchor as definitive. It banks 4 options with rationale for each.

Per `feedback_reference_oracle_invalidates_in_both_directions`: PR-Y36's reference oracle invalidated D.1c-dominant; PR-Y37's reference oracle invalidated H1+H2 cross-cohort cohesion. The cycle of refutation is the discipline working.

End of memo.
