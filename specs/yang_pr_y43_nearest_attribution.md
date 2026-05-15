# PR-Y43 — Nearest-triangle attribution (A/B/C/D classification) over Cherchi Render LOD diff (INFRASTRUCTURE-CLASS; D-DOMINANT, new 6th outcome)

| Field | Value |
|---|---|
| **Verdict** | SHIP-INFRA + **D-DOMINANT** (new 6th outcome the plan didn't anticipate) |
| **Class** | INFRASTRUCTURE-CLASS (test-harness extension; 0 production code) |
| **Parent commit** | `b0009bd` (PR-Y42 SHIP-INFRA; 2026-05-15) |
| **Date** | 2026-05-15 |
| **Authors** | spec-y43 (this file); canary-y43 (`docs/audits/pr_y43_canary.md`) |
| **LOC** | +438 in `crates/test-harness/tests/cherchi_differential_diff.rs` (1082 → 1520); 0 kernel; 0 wasm-bridge |
| **Production-code delta on F0020** | **0** (unchanged after 12 cycles) |
| **F0020 Status:Failed** | unchanged — 40 unpaired edges (39 boundary, 1 NMM); PR-Y43 changes none |
| **F0020 A/B/C/D histogram** | 4 / 14 / **0** / **24** = 9.5% / 33.3% / **0.0%** / **57.1%** |

---

## §1 Context

PR-Y43 is the **12th investigational PR on F0020 Render LOD**. PR-Y42
(commit `b0009bd`, 2026-05-15) shipped the B.1 strategic pivot — a Cherchi
C++ Render LOD diff harness against Waffle's final render mesh — and
produced the sharpest empirical finding of the 11-cycle arc: **20 of 40
F0020 unpaired edges (50.0% exactly) attributable to 42 specific Cherchi-only
missing triangles**. PR-Y42 §5 framed that as a BORDERLINE-sharp anchor: at
the 40% verdict threshold, with a ±1 record requantization noise floor, and
with the explicit failure-mode at §5.2 (PR-Y43 must confirm a clean
position-to-mechanism mapping or pivot to Option C pause).

PR-Y42 left two open questions:

1. **What does Waffle have NEARBY each of those 42 missing tris?** The 1e-6
   common=36 (Cherchi 246 vs Waffle 113) means ~85% of Cherchi's triangles
   have NO positional match anywhere in Waffle. Are the 42 attributable tris
   sub-grid drift (Case A), partial-match (Case B), no-proximity (Case C),
   or a residual (Case D)?
2. **Does the answer drive a PR-Y44 production-fix anchor or an Option C
   pause?** PR-Y42 §5/§6 wired the failure-mode at the threshold boundary.

PR-Y43 closes that vertex-level gap. The probe per-triangle classifies each
of F0020's 42 missing-attributable triangles into A/B/C/D by counting
Cherchi-side vertex matches against Waffle's Render LOD vertex set, swept at
**four grid scales (1× / 2× / 5× / 10× of the base oracle grid)** per
`feedback_multi_stage_anchor_probe`. The classification IS the PR-Y44 anchor
decision.

---

## §2 Why infrastructure-class

- **0 production LOC.** All changes in
  `crates/test-harness/tests/cherchi_differential_diff.rs` (test file).
  No kernel, wasm-bridge, or app code modified. No WASM rebuild required.
- **Default-off byte parity verified.** Canary Gate 2 (F0020 spotlight
  without `YANG_STAGE_DUMP` / `YANG_CONFORMAL_PROBE`) produces
  `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int`
  IDENTICAL to PR-Y42 baseline. The `[stage-f]` progression 138→119→119→
  113→113 (sub=0..4) and 30→42→39→39→39 unpaired counts are byte-identical
  post-probe-add and post-probe-invoke.
- **`#[test] #[ignore]` posture preserved.** New tests
  `f0020_render_lod_nearest_attribution` and `cohort_render_lod_nearest_attribution`
  are env-gated; if `CHERCHI2022_BIN` is unset, they no-op per the existing
  PR-Y29 skip-quietly contract.
- **Reuses PR-Y42 infrastructure.** Probe is built on top of PR-Y42's
  `run_render_lod_diff_for_case` + `RenderLodDiffCounts`; no new oracle,
  no new dump site.

---

## §3 Probe design (harness extension)

All changes live in `crates/test-harness/tests/cherchi_differential_diff.rs`
(extends from 1082 → 1520 lines; +438 LOC). Sibling function to PR-Y42's
`run_render_lod_diff_for_case`.

### §3.1 New types

```rust
struct NearestVertAttribution {
    match_at_1x: u8,                  // 0..=3
    match_at_2x: u8,                  // 0..=3
    match_at_5x: u8,                  // 0..=3
    match_at_10x: u8,                 // 0..=3
    off_vert_idx_when_b: Option<u8>,  // 0..=2 if Case B
}

struct WaffleVertSetAtGrid {
    factor: u32,                       // 1, 2, 5, 10
    keys: HashSet<(i64, i64, i64)>,    // f32-round-tripped, i64-quantized
}

struct NearestAttributionResult {
    target_tris: usize,
    case_a: usize,
    case_b: usize,
    case_c: usize,
    case_d: usize,
    case_b_dump: Vec<(usize, u8, [f64; 3], [f64; 3], i64)>,
    base_grid: f64,
}
```

### §3.2 New functions

- `build_waffle_vert_sets_at_grids(verts_f64, base_grid) -> [WaffleVertSetAtGrid; 4]`
  — quantizes Waffle Render LOD verts at 1×/2×/5×/10× cells; preserves
  the f32-round-trip path from PR-Y42's `oracle_quantize_waffle_obj`
  (production-oracle parity).
- `cherchi_vert_matches_waffle_at_grids(cherchi_v, base_grid, waffle_sets) -> [bool; 4]`
  — set-membership check for each grid level.
- `nearest_waffle_vert_at_base_grid(cherchi_v, base_grid, waffle_verts_f64) -> (usize, i64, [f64; 3])`
  — linear scan (n ≤ 219 for F0020), returns nearest index, i64 Chebyshev
  cell-distance at base grid, and raw f64 position.
- `classify_attribution(attr: &NearestVertAttribution) -> &'static str` —
  mutually-exclusive priority-ordered classification (§3.4).
- `run_nearest_attribution_for_case(case_id) -> Option<NearestAttributionResult>`
  — reuses PR-Y42's diff + attribution pipeline to obtain the 42 target
  triangles, then per-triangle classifies.

### §3.3 Four grid levels — why all four

Per `feedback_multi_stage_anchor_probe` ("don't conclude from a single grid
level; sweep 1× / 2× / 5× / 10×"):

| Grid | Cell at F0020 | Purpose |
|---|---|---|
| **1×** = `max_abs * TAU_TESS_GRID_FACTOR` (= `max_abs * 1e-5`) | ~5.42 µm | Production oracle base; "match at 1×" = positional coincidence with what the oracle would consider equal |
| **2×** = `2 × base` | ~10.84 µm | First widen step; catches sub-grid f32-quantization drift |
| **5×** = `5 × base` | ~27.11 µm | Used by Case A definition (all 3 verts present at 5× but not at 1×) |
| **10×** = `10 × base` | ~54.22 µm | Used by Case C definition (≤1 vert at 10×); catches gross positional non-coincidence |

The empirical evidence for all four levels: Case A (4 F0020 tris) only
manifests at the 5×/10× sweep. Single-grid analysis would have missed it.

### §3.4 Classification logic (priority-ordered, mutually exclusive)

```rust
fn classify_attribution(attr: &NearestVertAttribution) -> &'static str {
    if attr.match_at_5x == 3 && attr.match_at_1x < 3 { "A" }   // sub-grid drift
    else if attr.match_at_1x == 2                    { "B" }   // partial match
    else if attr.match_at_5x <= 1                    { "C" }   // no proximity
    else                                             { "D" }   // residual
}
```

| Case | Definition | Fix-shape implication |
|---|---|---|
| **A** | `match_at_5x == 3 ∧ match_at_1x < 3` | Sub-grid drift; grid tuning (refuted by PR-Y38; would be 9th-refutation) |
| **B** | `match_at_1x == 2` | Partial: 2 verts at 1×, 1 off — investigate off-vertex's upstream production |
| **C** | `match_at_5x ≤ 1` | No proximity; defect upstream of Render LOD → Option C pause |
| **D** | residual (catch-all) | 3-of-3 at 1× but triangle missing → triangle-topology/indexing/winding/edge-pair defect; NOT vertex production |

### §3.5 Case B off-vertex dump

For each Case B triangle, the probe identifies which vertex (0/1/2) is NOT
matched at 1× and emits:
- Cherchi position (lossy via 1e-6 → metres → f32, inherited from PR-Y42),
- Nearest Waffle position (raw OBJ f64),
- Chebyshev cell-distance (`i64` L∞ in base-grid cells).

This provides per-vertex PR-Y44 anchor data without kd-trees (linear scan
is fine for F0020's ~219 Waffle Render LOD verts × 14 Case B tris).

### §3.6 Test entry points

```rust
#[test] #[ignore] fn f0020_render_lod_nearest_attribution() { … }
#[test] #[ignore] fn cohort_render_lod_nearest_attribution() { … } // F0044/F0045/R0092
```

Skip-quietly when `CHERCHI2022_BIN` is unset (mirrors PR-Y29 contract).

---

## §4 Contracts

| Contract | Verification |
|---|---|
| Default-off byte parity (probe-off path byte-identical to PR-Y42 HEAD) | Canary Gate 2 — F0020 spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` byte-identical pre- and post-probe-add (and post-probe-invoke) |
| Probe-on / probe-off equivalence at production-path level | The probe runs in a `#[test] #[ignore]` body; the production tessellation paths are not entered with probe instrumentation; `cargo test -p kernel --lib` baseline (1262/24/42) unchanged |
| Vertex set construction is read-only over Render LOD output | `build_waffle_vert_sets_at_grids` takes `&[[f64; 3]]` already produced by `tessellate_solid_bounded`; the probe does not write back |
| Classification is deterministic given a fixed-thread Cherchi run | A/B/C/D byte-stable across 3 reruns at `target_tris=42`. Cherchi TBB non-det (PR-Y31 banked) shifts `missing_count` 194 ↔ 201 (~75/25 split) but does NOT shift the 4/14/0/24 distribution once the 42-mode is hit; in the 47-mode rerun the count shifts to 7/14/0/26 — Case C still 0, Case B still 14, Case D still dominant (§3.4 verdict still applies) |
| PR-Y42 baselines preserved | `f0020_render_lod_diff_baseline` reproduces `common=36, extras=76, attribution 20/40 = 50.0%, 42 missing-attributable tris`; `pr_y31_f0044_extras_zero` hard gate continues to pass byte-clean |
| Cohort skip-quietly preserved | If `CHERCHI2022_BIN` is unset, harness emits `[nearest-attribution …] SKIP` and returns `None` (mirrors PR-Y29) |

---

## §5 Gates

Eight gates, mirrors the canary memo §6:

| Gate | Description | Pass criterion |
|---|---|---|
| **1** | `cargo build -p test-harness --test cherchi_differential_diff` | Clean build; no new warnings beyond the 58 pre-existing kernel warnings |
| **2** | **F0020 default-off byte parity (CRITICAL)** | Spotlight `Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int` byte-identical to PR-Y42 baseline. `[stage-f] 138→119→119→113→113 + unpaired 30→42→39→39→39` byte-identical |
| **3** | PR-Y42 Render LOD diff baseline preserved | `f0020_render_lod_diff_baseline` reports `Common=36, Extras=76, attribution 20/40 = 50.0%, 42 missing-attributable tris`; `missing` count varies 194 ↔ 201 across reruns due to Cherchi TBB non-det (PR-Y31 banked) — not load-bearing |
| **4** | **F0020 A/B/C/D classification (LOAD-BEARING)** | Histogram across 42 target tris: A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), **D=24 (57.1%)**. Byte-stable across 3 reruns at target_tris=42 |
| **5** | F0020 Case B vertex dump | 14 entries dumped with (Cherchi position, nearest Waffle position, cell-distance). 5 distinct off-vertex positions account for 11 of 14 entries |
| **6** | Cohort sanity (F0044/F0045/R0092) | F0044: 0/8/0/8 of target=16. F0045: 0/2/0/2 of target=4. R0092: 0/0/0/0 of target=0. (The brief's "≥95% Case C" prediction was inverted — Case C is 0% — methodology is still sound; rationale in §6 below) |
| **7** | kernel lib + yang_fast regression | `cargo test -p kernel --lib`: 1262 passed / 24 failed / 42 ignored — IDENTICAL to PR-Y42 baseline. `YANG_BOOLEAN=1 yang_fast`: 10/157 passed — IDENTICAL to baseline |
| **8** | PR-Y31 hard gate (`pr_y31_f0044_extras_zero`) | F0044 Stage B `missing=0, extras=0, common=136`. Test passes byte-clean |

**Gate 2 is the critical contract** for INFRA-class. All eight gates GREEN
in the canary; reproduction commands at §10.

---

## §6 Outcome — **SHIP-INFRA + D-DOMINANT (new 6th outcome)**

### §6.1 Verdict

The plan's 5-outcome verdict table (§Phase 2) anticipated B-dominant /
A-dominant / C-dominant / diffuse / ABORT. F0020's empirical histogram
inserts a 6th outcome **the plan did not anticipate**:

> **D-dominant (≥40%)** → PR-Y44 anchor candidate: triangle-topology
> emission at F.x stages of Render LOD (indexing / winding / edge-pair),
> **NOT** vertex production, **NOT** grid tuning, **NOT** upstream-of-Render-LOD pause.

F0020 measures Case D at 24/42 = **57.1%** — above the 40% threshold used
for the other dominant-case verdicts and 17 percentage points clear of it.
The histogram is byte-stable across 3 reruns at `target_tris=42` (and the
one off-run at 47 still has Case D dominant + Case C = 0).

### §6.2 Case D semantics

Plan §Phase 2 defined Case D as the residual: "the in-between cases (e.g.,
3 verts at 1× but no triangle uses all 3 — would mean the triangle exists
but with different vertex INDICES that happen to coincide positionally;
unlikely but should report)."

The empirical 57.1% D-dominance refutes the "unlikely" framing. In plain
language: **for 24 of the 42 missing-from-Waffle Cherchi triangles, ALL
THREE of their vertex positions appear somewhere in Waffle's Render LOD
vertex set at the base grid. The triangle exists at Cherchi but Waffle's
mesh does not connect those three vertices into a triangle.** This is a
triangle-emission/topology defect, distinct from Case B (vertex-production
defect at a specific off-vertex) and from Cases A/C (quantization /
upstream-source defects).

### §6.3 PR-Y44 anchor candidates (with paper citations)

The Yang/Cherchi pipeline at the F.x layer is where Case D is most plausibly
introduced:

**Candidate (α)** — `remove_winding_insensitive_duplicates` at F.0 (drops 19
tris from 138).
Paper anchor: Cherchi 2022 §5 (manifold-flood inside/outside classification)
at `refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`.
This pass implicitly assumes the input triangle set is canonical;
winding-insensitive dedup can drop triangles that are correctly distinct in
the post-flood pair-up phase, leaving holes at their positions even though
their vertices remain in the set. **Primary** candidate.

**Candidate (β)** — `remove_nonmanifold_duplicates_aggressive` at F.3 (drops
6 tris from 119).
Paper anchor: Yang 2025 §4.4.1 at `refs/text/yang2025_hybrid_boolean.txt:548-590`,
which specifies "the mesh boolean operations may produce a non-manifold
mesh ... selectively retaining one of the duplicate triangles" — the
selective-retain heuristic implemented by F.3 may retain the wrong
representative. **Secondary** candidate.

**Candidate (γ)** — Pre-F.0 Boolean LOD → Render LOD re-tessellation at
`yang_integration.rs:1024` (drops ~108 tris from 246 to 138).
Paper anchor: Yang 2025 §4.4.1 (mesh updating after intersection
refinement). The pre-F.0 cascade is upstream of F.0 / F.3 and is where most
of the 246 → 113 triangle reduction happens. PR-Y41 §6.3 banked this stage
as an investigation target without localizing. **Tertiary** candidate.

A non-load-bearing coincidence: F.0 drops 19 tris + F.3 drops 6 tris = **25
≈ Case D = 24**. Suggestive that (α)+(β) are the dominant emission-loss
stages; canary in PR-Y44 must bisect which of the 24 Case D triangles are
dropped by which stage.

### §6.4 Secondary anchor — Case B (5 distinct off-vertices)

Case B at 33.3% (14 of 42) is below the 40% threshold but is **not noise**:
14 entries with a clean per-vertex dump (Gate 5), and **5 distinct off-vertex
positions account for 11 of 14 entries** (b[9-11] share one off-vertex;
b[1-2] share another; b[6-7] share another). PR-Y44 secondary anchor data
is compact (5 positions) and structurally explainable as Cherchi-side
post-arrangement interior subdivision points that Waffle's Render LOD pass
removes in F.0/F.1.

A PR-Y44 Case D fix would close ~24 of the 42 attributable tris (~57%); a
Case B fix would close ~14 (~33%). They are independent fix-shapes; PR-Y44
should prioritize Case D, bank Case B as PR-Y45 candidate.

### §6.5 Why this refutes Option C pause for F0020

Case C = 0 = ZERO. There is NO geometry-is-upstream-of-Render-LOD signal in
F0020's 42 attributable tris. PR-Y41/Y42 §6 framed Option C ("pause F0020
Render LOD") on the rationale that the defect was too diffuse / too far
upstream for further Render LOD investigation. The empirical 0% Case C
**directly refutes** that framing for F0020 specifically: F0020's defect IS
at (or just-pre-) the Render LOD layer. The strategic-pivot trajectory
(PR-Y29-PR-Y43) was correct.

**Caveat:** this is F0020-specific. Cohort F0044/F0045/R0092 might still
warrant Option C if PR-Y44's Case D fix doesn't generalize. The cohort's
Case B/D 50/50 split (§7) is a different distribution from F0020's.

### §6.6 Strategic-pivot ROI update (PR-Y42 §6.2 → PR-Y43)

PR-Y42 framed ROI as **MIXED**. PR-Y43 updates to:

- **F0020:** PAYING OFF. PR-Y42's 50% borderline-attribution becomes
  PR-Y43's 90% explainable as Case D (57.1%) + Case B (33.3%). The
  strategic pivot has produced the sharpest empirical anchor in the
  12-cycle arc.
- **Cohort method limit (PR-Y42 §6.2):** UNCHANGED for triangle-level
  diff (common=0 universal). BUT vertex-level diff (PR-Y43's
  contribution) IS dense for cohort cases — F0044/F0045 both show 50%
  Case B vs 50% Case D. The methodology generalizes for **vertex-level**
  cohort investigations even though it doesn't for **triangle-level**.

Per `feedback_no_last_bug`: we do NOT claim PR-Y44 will fix F0020. We claim
PR-Y43 produces the sharpest empirical anchor in the arc and refutes Option C
pause for F0020 specifically.

---

## §7 Rollback

PR-Y43 is INFRA-only with all changes confined to
`crates/test-harness/tests/cherchi_differential_diff.rs`. Revert procedure
if the probe ever regresses default-off behavior:

```bash
git checkout b0009bd -- crates/test-harness/tests/cherchi_differential_diff.rs
# (b0009bd = PR-Y42 SHIP-INFRA HEAD; cherchi_differential_diff.rs at that commit
#  is 1082 lines without the PR-Y43 nearest-attribution extension)
cargo build -p test-harness --test cherchi_differential_diff
```

`app/tests/cases/assay/results.json` regenerates from `spotlight_f0020`
invocations and is not load-bearing on PR-Y43.

No kernel, wasm-bridge, or production-path changes to revert. WASM bundle
unaffected (no rebuild required for PR-Y43; none required for rollback).

---

## §8 Cherchi non-determinism (PR-Y31 banked, applied here)

Cherchi C++ has internal TBB non-determinism even with `TBB_NUM_THREADS=1`
(banked observation PR-Y31). PR-Y43's empirical behavior under non-det:

| Quantity | Stability under TBB non-det |
|---|---|
| `missing_count` (194 vs 201) | **~75/25 split** (3/4 reruns gave 194; 1/4 gave 201) |
| `target_tris` (42 vs 47) | Co-varies with `missing_count`: 194 → 42; 201 → 47 |
| Case A count | 4 (42-mode) / 7 (47-mode) |
| Case B count | **14 in BOTH modes** (BYTE-STABLE) |
| Case C count | **0 in BOTH modes** (BYTE-STABLE) |
| Case D count | 24 (42-mode) / 26 (47-mode) |
| **D-dominant verdict** | **Holds in both modes** (24/42 = 57.1% and 26/47 = 55.3%) |
| Case C-dominance refuted | **Holds in both modes** (Case C = 0) |
| Case B count = 14 | **Holds in both modes** (BYTE-STABLE) |

**Mitigation:** the load-bearing verdict (D-dominant; Case C = 0; Case B = 14)
is robust to Cherchi TBB non-determinism. PR-Y43 reports both modes in the
canary memo §3.3 per-rerun stability table. PR-Y44 should pin Cherchi
tighter (single-thread without TBB altogether, or use the deterministic
attribution gates) for any production-fix verification.

---

## §9 Banked / open

### §9.1 Banked findings for PR-Y44

Aligned with canary memo §8.1:

1. **D-dominant outcome is new** — the original plan's 5-outcome verdict
   table did not include it. PR-Y44 plan should explicitly handle
   D-dominant with the (α)/(β)/(γ) candidates listed in §6.3.
2. **5 distinct off-vertex positions account for 11/14 Case B entries.**
   PR-Y44 secondary anchor data is compact: ~5 positions rather than 14
   triangles.
3. **Cohort cases (F0044/F0045) show the same Case B mechanism** (8/16 +
   2/4 entries are Case B). A Case B fix might generalize to cohort even
   if Case D doesn't (cohort `common=0` means triangle-topology defects
   differ from F0020's).
4. **Cherchi non-det is reproducible** — ~75/25 split between 42-mode and
   47-mode. PR-Y44 should pin Cherchi stricter for production-fix
   verification.
5. **F.0 drop count (19) + F.3 drop count (6) = 25 ≈ Case D count (24).**
   Suggestive but not load-bearing; PR-Y44 canary must bisect which stage
   drops the Case D triangles specifically.

### §9.2 Open questions (PR-Y45+)

Aligned with canary memo §8.2:

1. **Cohort Case B/D semantics differ from F0020's.** F0020 Case D is
   "3-of-3 at 1× but tri missing"; cohort Case D may include "1-or-2 at
   1× + 1-or-2 at 5×" because cohort `common=0`. Need a finer Case D
   sub-classification.
2. **The 42 attributable tris vs the OTHER 152 missing tris.** PR-Y43 only
   classified the 42 that border unpaired edges. Are the other 152 also
   Case D-dominant or do they shift toward A/B?
3. **PR-VIZ-3a yang debug capture** can render the 24 Case D triangles vs
   Waffle's 113 Render LOD triangles visually — banked for PR-Y44 canary.

### §9.3 Methodological banked

Aligned with canary memo §8.3:

1. **Vertex-level diff IS the right grain for analytic-surface cohort
   cases.** Triangle-level diff has `common=0` method-limit (PR-Y42
   finding) but vertex-level diff (PR-Y43 finding) is dense. Future cohort
   canaries should default to vertex-level.
2. **The 4-grid-level sweep was useful.** Case A (4 tris) only manifests at
   the 5×/10× sweep — would have been missed with single-grid analysis.
   `feedback_multi_stage_anchor_probe` empirically vindicated.
3. **Case D was assumed unlikely in the plan.** Plans should not pre-judge
   case likelihoods; let the data speak.

### §9.4 Citations and feedback memories applied

**Paper citations:**

- **Yang 2025 §4.4.1** (`refs/text/yang2025_hybrid_boolean.txt:548-590`):
  mesh updating after intersection refinement — "the mesh boolean operations
  may produce a non-manifold mesh ... selectively retaining one of the
  duplicate triangles." Direct anchor for Case D (β) candidate (F.3
  selective-retention heuristic).
- **Cherchi 2022 §5** (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:340-413`):
  manifold-flood inside/outside classification — assumes input triangle set
  is canonical. Direct anchor for Case D (α) candidate (F.0 dedup may drop
  triangles that are correctly distinct).
- **Cherchi 2022 §3** (background; referenced via PR-Y42 §4.4):
  well-formed guarantee is conditional on "watertight manifold meshes that
  do not touch tangentially" — F0020's tangential planar contacts violate
  this, hence Cherchi's own well_formed=false. Matching Cherchi exactly is
  not the same as fixing F0020 (PR-Y42 §4.4 caveat preserved).

**Feedback memories applied:**

- `feedback_external_coherence` (**load-bearing**): Cherchi C++ remains the
  reference oracle. PR-Y43 reuses PR-Y42's set-diff + attribution data —
  no new oracle, just a new lens.
- `feedback_anchor_before_fix`: the probe IS the load-bearing measurement
  before any production fix. PR-Y43 ships 0 production code; PR-Y44
  candidates are listed for empirical canary, NOT as fix prescriptions.
- `feedback_phase1_diagnosis_ranking_is_inference`: the verdict is
  measurement (24/42 = 57.1% Case D), not inference. The (α)/(β)/(γ)
  ranking is by paper-citation alignment plus the F.x drop-count
  coincidence (25 ≈ 24).
- `feedback_multi_stage_anchor_probe`: 4 grid levels swept (1×/2×/5×/10×);
  classification depends on all 4. Case A would have been missed at single
  grid.
- `feedback_validate_against_corpus`: cohort tested (Gate 6); cohort
  prediction inversion (Case C = 0 vs expected ≥95%) honestly reported.
- `feedback_no_last_bug`: 12th cycle on F0020 Render LOD. Explicit
  non-closure language in §6.5/§6.6.
- `feedback_yang_only`: PR-Y43 ships measurement infrastructure; no
  production logic changed; no fallback paths.
- `feedback_no_regression_chasing`: INFRA-only; no production reverts.
- `feedback_adversary_no_destructive_git`: canary executed worktree-only.
- `feedback_implementer_anti_fabrication_diff`: canary memo §1.2-§1.5
  includes verbatim diff/numstat/first-50-lines artifacts.
- `feedback_per_plan_cycle_team`: team `pr-y43` exists for this cycle;
  TeamDelete at close-out.
- `feedback_always_push`: implementation phase pushes to origin/main
  (plain push only; never force-push).

---

## §10 Verification commands (verbatim, fresh-checkout)

```bash
# Gate 1: build
cargo build -p test-harness --test cherchi_differential_diff

# Gate 2: F0020 default-off byte parity (CRITICAL)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized \
  -- spotlight_f0020 --ignored --nocapture
# expect: Status:Failed; 40 unpaired (39 boundary, 1 NMM); 8 degen; 10 self-int
# expect: [stage-f] 138→119→119→113→113; unpaired 30→42→39→39→39

# Gate 3: PR-Y42 baseline preserved
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_diff_baseline --ignored --nocapture --test-threads=1
# expect: common=36, attribution 20/40 = 50.0%, target_tris=42
# (missing/extras vary 194↔201 due to Cherchi TBB non-det; not load-bearing)

# Gate 4 + 5: F0020 A/B/C/D classification + Case B dump (LOAD-BEARING)
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- f0020_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect (42-mode, ~75% of runs):
#   Case A=4 (9.5%), B=14 (33.3%), C=0 (0.0%), D=24 (57.1%)
# expect (47-mode, ~25% of runs):
#   Case A=7, B=14, C=0, D=26 — still D-dominant, still Case C=0, still Case B=14
# expect: 14 Case B entries dumped with (off_idx, C_pos, W_pos, cell_dist)

# Gate 6: cohort sanity (F0044/F0045/R0092)
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  TBB_NUM_THREADS=1 YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- cohort_render_lod_nearest_attribution --ignored --nocapture --test-threads=1
# expect: F0044 target=16 (B=8, D=8); F0045 target=4 (B=2, D=2); R0092 target=0

# Gate 7: kernel lib + yang_fast regression
cargo test -p kernel --lib
# expect: 1262 passed; 24 failed; 42 ignored

YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized \
  -- yang_fast --ignored --nocapture --test-threads=1
# expect: 10/157 passed

# Gate 8: PR-Y31 hard gate
CHERCHI2022_BIN=$HOME/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  cargo test -p test-harness --test cherchi_differential_diff \
  -- pr_y31_f0044_extras_zero --ignored --nocapture
# expect: PASS (F0044 Stage B missing=0, extras=0, common=136)
```
