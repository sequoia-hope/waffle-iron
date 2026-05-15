# PR-Y42 — Cherchi C++ Render LOD vertex/triangle diff harness (INFRASTRUCTURE-CLASS; B.1 strategic pivot)

| Field | Value |
|---|---|
| **Verdict** | SHIP-INFRA + BORDERLINE-sharp PR-Y43 anchor + cohort-method-limit disclosure |
| **Class** | INFRASTRUCTURE-CLASS (test-harness extension; 0 production code) |
| **Parent commit** | `daafbbc` (audit-y41 ACCEPT; 7th-refutation; strategic-pivot recommended) |
| **Date** | 2026-05-15 |
| **Authors** | spec-y42 (this file); canary-y42 (`docs/audits/pr_y42_canary.md`) |
| **LOC** | +413 in `crates/test-harness/tests/cherchi_differential_diff.rs`; 0 kernel; 0 wasm-bridge |
| **Production-code delta on F0020** | **0** (unchanged after 11 cycles) |
| **F0020 Status:Failed** | unchanged — 40 unpaired edges (39 boundary, 1 NMM) |

---

## §1 Context

PR-Y42 is the **11th investigational PR on F0020 Render LOD**. The 10 prior
cycles (Y25/Y26/Y27/Y28/Y39 ABORTed at canary; Y36/Y37/Y38/Y40/Y41 INFRA-only
SHIPs) all ran **Waffle-internal probes** — each eliminated a candidate but no
fix anchor emerged and the F0020 40-unpaired-edge count did not move. PR-Y41's
canary §6 explicitly recommended a **strategic pivot per Option B.1**: extend
the existing PR-Y29/Y30/Y31 Cherchi differential harness — proven at the Stage B
layer — to the final Render LOD output layer, per `feedback_external_coherence`'s
load-bearing principle that the public reference C++ impl is the oracle.

PR-Y29-Y31 applied this at Stage B; PR-Y42 applies it one layer downstream:
diff Waffle's **final render mesh** (the OBJ shipped to the assay oracle and to
three.js) against Cherchi's `mesh_booleans` final output. The pivot is the
strategic response to the 10-cycle candidate-elimination pattern, not another
internal probe.

---

## §2 Why infrastructure-class

- **0 production LOC.** The Render LOD dump site that the plan anticipated
  building at `tessellate_solid_bounded` end was found to already exist as
  `stage_E_lod=Render.obj` at `crates/kernel/src/boolean/yang_integration.rs:1063-1074`,
  emitted whenever `YANG_STAGE_DUMP` is armed by the harness. PR-Y42 reuses
  this existing dump site → no kernel change, no wasm-bridge change, no WASM
  rebuild required.
- **Default-off byte parity verified.** Gate 2 (F0020 spotlight without
  `YANG_STAGE_DUMP` / `YANG_CONFORMAL_PROBE`) produces `Status:Failed; 40 unpaired
  (39 boundary, 1 NMM); 8 degen; 10 self-int` IDENTICAL to PR-Y41 baseline.
  The `[stage-f]` progression 138→119→119→113→113 (sub=0..4) and 30→42→39→39→39
  unpaired counts are byte-identical.
- **`#[test] #[ignore]` posture.** New tests `f0020_render_lod_diff_baseline` and
  `cohort_render_lod_diff_baseline` are env-gated; if `CHERCHI2022_BIN` is unset,
  they no-op per the existing PR-Y29 skip-quietly contract.

---

## §3 Probe design (harness extension)

All changes live in `crates/test-harness/tests/cherchi_differential_diff.rs`
(extends from 671 → 1082 lines; +411 LOC of new code, +2 lines in `WaffleDumpPaths`
struct extension).

### §3.1 `WaffleDumpPaths` extension

```rust
struct WaffleDumpPaths {
    workdir: PathBuf,
    path_a: PathBuf,
    path_b: PathBuf,
    path_stage_b: PathBuf,
    path_render_lod: PathBuf,   // NEW (PR-Y42)
}
```

`path_render_lod = workdir/stages/<CASE>/stage_E_lod=Render.obj` is populated by
the existing kernel-side probe at `yang_integration.rs:1063-1074` once
`YANG_STAGE_DUMP` is armed inside `run_waffle_and_collect_dumps`.

### §3.2 `RenderLodDiffCounts` struct

Fields: `waffle_tris`, `cherchi_tris`, `missing` (Cherchi-only), `extras`
(Waffle-only), `common` (positional match at 1e-6 grid), plus F0020 attribution
fields `waffle_unpaired_edges`, `missing_tris_explaining_unpaired`,
`unpaired_edges_explained`. All `usize`. Carries Debug + Copy + Eq for inspect-by-print
test bodies.

### §3.3 `run_render_lod_diff_for_case`

Mirrors PR-Y29's `run_diff_for_case`: resolve binary (skip-quietly if unset),
run Waffle via `run_waffle_and_collect_dumps`, resolve per-case op via PR-Y31's
`read_first_boolean_op`, invoke Cherchi, parse both OBJs, compute canonical-
quantized-triangle sets at the **1e-6 grid** (matches PR-Y29/Y30/Y31 Stage B
convention; counts comparable across stages), emit missing/extras/common +
top-N records. **Attribution step (F0020 load-bearing):** quantize Waffle's
Render LOD against the production oracle's scale-adaptive grid
(`max_abs * TAU_TESS_GRID_FACTOR` with f32 round-trip — replicates
`oracle.rs::check_watertight_mesh` exactly), enumerate unpaired edges, then
re-quantize each Cherchi-only missing triangle's 3 vertices against the SAME
oracle grid and bucket by edge-position overlap.

### §3.4 Two quantization grids — why both

| Grid | Where used | Purpose |
|---|---|---|
| `QUANTIZE_GRID = 1e-6 m` | `quantize_tri` | Set-difference triangle diff (matches PR-Y29/Y30/Y31 Stage B convention; sub-µm = well below kernel TAU_WORK 1e-12) |
| `max_abs * TAU_TESS_GRID_FACTOR` (~5.4 µm at F0020) | `oracle_quantize_waffle_obj` | Replicates production oracle exactly; required for F0020 40-unpaired attribution to byte-match the assay-reported count |

The two-step requantization (Cherchi vertex → 1e-6 quantize → f64 metres → f32
→ oracle grid) introduces a ~1 µm precision floor in the attribution step. At
F0020's scale (max_abs ≈ 0.36 m) the oracle grid is ~3.6 µm; the 1 µm floor is
~28% of one oracle cell. Acceptable for COUNT-level attribution; individual
edge-match decisions at the cell boundary can swing ±1 under noise.

### §3.5 Test entry points

`#[test] #[ignore] fn f0020_render_lod_diff_baseline()` and
`cohort_render_lod_diff_baseline()` (iterating F0044/F0045/R0092). Skip-quietly
when `CHERCHI2022_BIN` is unset.

### §3.6 Methodological note — planned kernel dump site was unnecessary

The plan anticipated adding `stage_RENDER.obj` emission in
`crates/kernel/src/tessellation/mod.rs:4805+`. Investigation found that
`tessellate_waffle_solid` at `crates/kernel/src/boolean/yang_integration.rs:1063-1074`
already emits `stage_E_lod=Render.obj` under `YANG_STAGE_DUMP` — the post-F.4
final render mesh, md5-verified byte-identical to per-stage `stage_F.4.obj`
on F0020 dry run. Reusing this site cut the kernel-dump-site work to 0 LOC.

---

## §4 Empirical findings (LOAD-BEARING)

### §4.1 F0020 Render LOD diff

| Quantity | Cherchi C++ (`mesh_booleans union`) | Waffle Render LOD | Delta |
|---|---|---|---|
| Triangles | 246 | 113 | +133 |
| Vertices | 120 | 219 | -99 |
| `well_formed` | **false** | false | – |
| Euler χ | 5 | 2 | +3 |

| Set-difference (1e-6 grid, winding-insensitive) | Count |
|---|---|
| **Missing** (in Cherchi, NOT in Waffle Render LOD) | **194** |
| **Extras** (in Waffle Render LOD, NOT in Cherchi) | **76** |
| **Common** (matching quantized positions) | **36** |

For reference: PR-Y31's existing Stage B diff baseline reports
`missing=7, extras=0, common=230`. The Render LOD `missing=194` is ≫ Stage B
`missing=7` — Render LOD destroys **187 Cherchi-matching triangles downstream of
byte-clean Stage B**. The pivot localizes the F0020 defect to the Stage B → Render
LOD transition (E_lod=Render re-tessellation + F.0–F.4 repair).

### §4.2 F0020 oracle attribution

```
Oracle grid: 5.4 µm (max_abs ≈ 0.36 m × TAU_TESS_GRID_FACTOR 1e-5)

Waffle Render LOD unpaired edges:         40  (39 boundary, 1 non-manifold)
  ← matches the production oracle's 40-unpaired count byte-for-byte ✓

Cherchi-only missing tris with ≥1 edge matching unpaired:  42 / 194  (22%)
Unpaired edges explained by ≥1 missing tri:                20 / 40   (50.0%)
```

**20 of 40 unpaired edges (50.0% exactly) are attributable to 42 specific
Cherchi-only missing triangles.** This is the SHARPEST signal of the entire
11-cycle arc — the first time an external reference oracle has produced a
direct edge-level attribution claim on F0020.

Top-10 attribution records: 9 of 10 records match exactly 1 unpaired edge per
missing triangle; 1 record (rec[6]) matches 2 — i.e., the 42 attributing tris
distribute roughly 2-per-edge across the 20 explained edges (each side of a
missing edge in Cherchi's mesh).

### §4.3 Cohort summary (Gate 6)

| Case | Op | Cherchi tris (wf) | Waffle tris | C-only missing | W-only extras | **common** | unpaired | attr % |
|---|---|---|---|---|---|---|---|---|
| **F0020** | union | 246 (false) | 113 | 194 | 76 | **36** | 40 | **20/40 = 50.0%** |
| F0044 | subtraction | 136 (**true**) | 116 | 136 | 116 | **0** | 12 | 8/12 = 66.7% |
| F0045 | union | 236 (**true**) | 302 | 236 | 275 | **0** | 38 | 2/38 = 5.3% |
| R0092 | subtraction | 225 (false) | 173 | 192 | 120 | **0** | 43 | 0/43 = 0.0% |

### §4.4 Cherchi well_formed=false for F0020

Cherchi C++'s `mesh_booleans union` output on F0020 is itself `well_formed=false,
χ=5`. Per Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-320`),
the well-formed guarantee is structural (well-formed simplicial complex, no
T-junctions, surface patches bounded by closed loops of non-manifold edges) and
holds only when the inputs are "watertight manifold meshes that do not touch
tangentially." F0020's inputs do touch tangentially (3 rectangle extrudes with
shared planar faces), so Cherchi's own guarantee fails on F0020. **Consequence:
matching Cherchi exactly would NOT fully close F0020 Status:Failed** — Cherchi
itself doesn't resolve F0020 cleanly. The other 20 unpaired edges in the 50%
non-attributed set may be Waffle-specific defects beyond what Cherchi captures.

This is honestly framed per `feedback_no_last_bug`: a borderline-sharp anchor is
NOT a closure claim.

---

## §5 PR-Y43 anchor recommendation — BORDERLINE-sharp

### §5.1 Primary recommendation (sharp interpretation)

Investigate **which Render LOD stage drops the 20-triangle subset bounding F0020's
20 attributable unpaired edges.** The stages downstream of Stage B (246 tris) and
upstream of the final Render LOD (113 tris):

| Stage | Site | Triangle delta observed (F0020) |
|---|---|---|
| E_lod=Render | `tessellate_waffle_solid` re-tessellate at Render LOD | 246 → 138 (Boolean LOD subdivision → Render LOD re-tessellation) |
| F.0 | `remove_winding_insensitive_duplicates` | 138 → 119 |
| F.1 | `remove_nonmanifold_topology_aware` | 119 → 119 (unpaired count moves 30 → 42) |
| F.2 | `remove_winding_insensitive_duplicates` (2nd pass) | 119 → 119 |
| F.3 | `remove_nonmanifold_duplicates_aggressive` | 119 → 113 |
| F.4 | `weld_smooth_vertices` | 113 → 113 (introduces +1 NMM edge) |

The 20 attribution records output by `f0020_render_lod_diff_baseline` provide
specific position-quantized triangle coordinates. The PR-Y43 canary should probe
per-stage triangle survival for that specific position list, identifying which
stage drops the 20 triangles attributable to the 20 unpaired edges.

### §5.2 Strategic checkpoint — failure mode at PR-Y44

If PR-Y43's canary CANNOT confirm a position-to-stage mapping for the 20
attributing triangles, OR if the 20 triangles trace exclusively to a class
that Cherchi-also-misses (no Waffle-side production fix possible because
Cherchi well_formed=false for F0020), **PR-Y44 should pivot to Option C
(pause F0020 Render LOD)** per PR-Y41 canary §6.4. The 50% number is at the
verdict threshold, not comfortably above; the attribution requantization has
a ±1 record noise floor (§3.4). This is the explicit failure-mode disclosure
required by `feedback_phase1_diagnosis_ranking_is_inference` applied at the
threshold boundary.

---

## §6 Strategic-pivot ROI assessment (HONEST)

### §6.1 What was promised in PR-Y41 §6

> If PR-Y42 produces a sharp PR-Y43 anchor (Verdict outcome 1), the pivot has
> paid off and production code becomes the next near-term goal. If PR-Y42
> produces an 8th-refutation (outcome 3), it's strong empirical signal to pause
> F0020 entirely per Option C from PR-Y41 (and the pivot itself has been
> ROI-positive by reaching a confident "stop" decision after 11 cycles rather
> than 20).

### §6.2 What actually happened — ROI is MIXED

**Paid off (F0020):** External oracle answered "what specific Cherchi triangles
are missing from Waffle's Render LOD" — 194 missing tris with positions. The
50.0% F0020 attribution is the sharpest empirical signal of the entire 11-cycle
arc. PR-Y43 now has a position-level target list rather than another round of
internal inference.

**Method limit (cohort, MIXED):** Cohort `common=0` is universal for
F0044/F0045/R0092. F0020 is all-planar (3 rectangle extrudes); cohort cases
contain analytic surfaces (cylindrical/spherical/conical) whose Waffle Render
LOD re-tessellates at 64 segments while Cherchi keeps the lower-segment (16)
post-arrangement geometry. The two pipelines never share vertex positions for
analytic faces — structurally expected per `yang_integration.rs:1024`. F0044's
headline 66.7% with `common=0` is **signal-of-proximity, not signal-of-defect**
(within ~5 µm but not AT Waffle positions). F0045 (5.3%) and R0092 (0.0%) are
noise. Future Render LOD work on analytic-surface cases needs a different
methodology (segment-count alignment, positional-tolerance match, or per-case
fixtures).

**Caveat constraining §5.1:** Cherchi well_formed=false for F0020. Matching
Cherchi exactly is NOT the same as fixing F0020. A PR-Y43 fix on the 20
attributable edges would land F0020 at ≈20 unpaired, not 0.

The pivot **paid off for F0020 specifically** but **revealed methodological
limits at the cohort level**. Per `feedback_no_last_bug`, this PR closes nothing.

---

## §7 Out of scope (banked, unchanged)

- **F0020 Status:Failed** remains at 40 unpaired (39 boundary, 1 NMM). PR-Y42
  is INFRA-only; no production code touches F0020 across 11 cycles.
- **F0045 Render LOD defect** (302 Waffle tris vs 236 Cherchi tris, 38 unpaired):
  unchanged. Attribution 5.3% under PR-Y42 method is noise; investigation needs
  a different methodology.
- **R0092 Render LOD defect** (173 vs 225, 43 unpaired): unchanged. Attribution
  0.0% — PR-Y42 method does not address this case.
- **F0044 Render LOD defect** (116 vs 136, 12 unpaired): unchanged. The 66.7%
  attribution is signal-of-proximity (analytic-surface re-tessellation
  divergence), not signal-of-defect.
- **139 yang_fast corpus failures**: unchanged. PR-Y42 method does not
  generalize to non-F0020-class workloads.
- **Cherchi C++ TBB non-determinism**: persists under `TBB_NUM_THREADS=1` in
  some F0020 reruns (per PR-Y31 banked finding). Mitigation: use missing-count
  (deterministic in our runs) as the load-bearing gate metric, not extras.
- **0 production code changes** on F0020 Render LOD across 11 cycles. PR-Y42
  does not change this. **No "this closes Yang" language anywhere in this PR.**

---

## §8 Risk / mitigation

| Risk | Mitigation | Status |
|---|---|---|
| Probe-off byte parity regression | Gate 2 (F0020 spotlight without `YANG_STAGE_DUMP` / `YANG_CONFORMAL_PROBE`) byte-matches PR-Y41 baseline | **VERIFIED** (canary §1) |
| `stage_E_lod=Render.obj` emission affecting production path | Already gated on `YANG_CONFORMAL_PROBE` env OR `is_yang_capture_armed()` (PR-VIZ-3a-fix). PR-Y42 does NOT touch this gate. | **NO CHANGE** |
| Cherchi binary unavailability in CI | Skip-quietly: `cherchi_bin()` returns `None`; harness emits `[render-lod-diff …] SKIP` and returns `None` | **PRESERVED** (mirrors PR-Y29) |
| Cohort `common=0` mistaken for general attribution | §4.3 + §6.2 document method-limit; canary §4/§6 give structural reason | **DOCUMENTED** |
| Cherchi well_formed=false on F0020 mistaken for closure | §4.4/§6.2/§5.2 flag this; Option C failure-mode at PR-Y44 explicit | **DOCUMENTED** |
| Strategic-pivot ROI overclaim | §6 frames as **MIXED**, not "paid off" | **DOCUMENTED** |
| Kernel lib / yang_fast / PR-Y31 Stage B hard gate regression | Gates 7/8 + `pr_y31_f0044_extras_zero` all pass unchanged | **GREEN** |

---

## §9 Citations and feedback memories applied

**Paper citations:**

- **Yang et al. 2025 §4.4.1** (`refs/text/yang2025_hybrid_boolean.txt:605-610`):
  mesh updating after intersection refinement breaks bijectivity; topology of
  mesh aligns with B-Rep Boolean output. Relevant context for the Render LOD
  layer being downstream of Stage B's exact mesh boolean.
- **Cherchi et al. 2022 §3** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-320`):
  arrangement step produces a well-formed simplicial complex; output triangle
  origin tracking; "a Boolean operation between two watertight manifold meshes
  that do not touch tangentially is guaranteed to be manifold watertight."
  Establishes that Cherchi's well_formed guarantee is conditional on the
  inputs being non-tangentially-touching — and that F0020's tangential planar
  contacts violate this precondition, explaining `well_formed=false` (§4.4).
- **Yang 2025 §4.4.2 and §4.5.5**: mesh and B-Rep booleans + coplanar
  preprocessing — relevant background for the Stage B → Render LOD pipeline
  ordering used in the harness.

**Feedback memories applied:**

- `feedback_external_coherence` (**load-bearing**): Cherchi C++ is the external
  reference oracle. The 50% F0020 attribution finding is empirical justification
  that the principle paid off for F0020-class cases.
- `feedback_yang_only`: PR-Y42 ships measurement infrastructure; no production
  logic changed; no fallback paths.
- `feedback_no_regression_chasing`: INFRA-only; no production reverts.
- `feedback_no_last_bug`: explicit non-closure language in §1/§4.4/§6.2/§7.
- `feedback_phase1_diagnosis_ranking_is_inference`: 50.0% is direct measurement;
  "borderline" framing in §5/§6; Option C failure-mode at §5.2.
- `feedback_validate_against_corpus`: cohort tested (Gate 6); method limit
  honestly framed.
- `feedback_anchor_before_fix`: canary IS the load-bearing measurement before
  any production fix.
- `feedback_adversary_no_destructive_git`: canary worktree-only.
- `feedback_implementer_anti_fabrication_diff`: canary memo §1.1-§1.4 includes
  verbatim diff artifacts.
- `feedback_per_plan_cycle_team`: team `pr-y42` exists for this cycle;
  TeamDelete at close-out.
- `feedback_always_push`: implementation phase pushes to origin/main.
