# PR-Y37 — Extend inverse-direction probe with H1/H2/H3 OTHER-cluster sub-classification; cross-cohort prediction REFUTED; **SHIP-INFRA + 6th-refutation framing**

| Field | Value |
|---|---|
| Authors | spec-y37, canary-y37 |
| Parent | `1ad58ce` (post-PR-Y36 audit ACCEPT — main HEAD on 2026-05-13) |
| Date | 2026-05-13 |
| Class | **INFRASTRUCTURE-ONLY** — instrumentation + memo; **zero production logic changed** |
| Verdict | **SHIP-INFRA + 6th-refutation framing** — PR-Y36's cross-cohort prediction empirically REFUTED; H1 attribution 0% across all 4 cohort cases; H2 fires only on R0092 (27.9%); H3 dominant across all 4 (F0020 48.7%, F0044/F0045 100%, R0092 72.1%) |
| Load-bearing artifact | `docs/audits/pr_y37_canary.md` — H1/H2/H3 attribution table + cohort cross-prediction REFUTATION table + 8-gate verification |

---

## §1 Context

### §1.1 PR-Y36 banked the OTHER cluster

PR-Y36 (shipped `d8fa288`, 2026-05-13, INFRA-ONLY) built the inverse-direction probe at `tessellation/mod.rs::tessellate_solid_bounded`. F0020 inv#6 attribution at HEAD:

- D.1a = 9 (23.1%); D.1b = 0; D.1c = **0%** (empirically refutes PR-Y28's β-shape dominant hypothesis); D.1d = 8 (20.5%); D.1 total = 17 (43.6%)
- **OTHER = 22 (56.4%)** — the new dominant cluster: partial-NMM kept faces (50–69% NMM), `pushed=true, in_final=true`, NOT in PR-Y28's framework

Cohort sanity at PR-Y36: F0044 (12/12), F0045 (38/38), R0092 (43/43) — all 100% OTHER, 0% D.1.

### §1.2 PR-Y36's load-bearing cross-cohort hypothesis

PR-Y36 canary §4.2 banked the PR-Y37 anchor on a **load-bearing prediction**:

> "If F0020's OTHER maps to H1 (sub-grid seam, F0044/F0045 D.2 prediction) + H2 (NMM-pair render asymmetry, R0092 D.3 prediction), the cluster is NOT novel — fixing H1+H2 closes all four cases."

PR-Y37 extends the Y36 probe with H1/H2/H3 sub-classification of the OTHER cluster to **verify this prediction empirically**. The prediction is the load-bearing deliverable; the H1/H2/H3 detection thresholds (80% axis-aligned + grid-quantized; 50% NMM-pair-render-asymmetry) are first-cut heuristics derived from PR-Y27's D.2/D.3 cohort split.

### §1.3 Strategic context — 6th investigational PR on F0020 Render LOD

PR-Y25/Y26/Y27/Y28 = four consecutive canary-stage ABORTs on production-fix candidates; PR-Y36 = SHIP-INFRA, cross-cohort prediction banked. PR-Y37 = canary verification of that prediction. The durable artifact is the **probe itself**, accumulating cohort-wide diagnostic capability.

---

## §2 Why infrastructure-class

### §2.1 Strategic escalation rule (`feedback_anchor_before_fix`)

> "Three wrong anchors in a row → stop bisecting, build a reference comparison."

PR-Y23-Y28 = four canary-stage ABORTs; PR-Y36 was the first reference-comparison-class probe (D.1 attribution chain). PR-Y37 is the **second reference comparison** (cross-cohort overlap). Per the escalation rule, both reference comparisons run before any production-fix anchor is committed. PR-Y37 is the **6th consecutive canary-stage finding-no-fix-shape outcome** on F0020 Render LOD.

### §2.2 No fix shape without verified empirical chain (`feedback_phase1_diagnosis_ranking_is_inference`)

PR-Y36 ranked H1 (sub-grid seam) + H2 (NMM-pair asymmetry) as the predicted dominant signatures of the OTHER cluster — drawn from PR-Y27's D.2/D.3 cohort split. PR-Y37 measures whether that ranking holds at the H1/H2 detection thresholds defined in the PR-Y36 plan. **The prediction is structural inference, not measurement, until the canary fires.**

### §2.3 External coherence (`feedback_external_coherence`)

The probe IS the reference for Render LOD attribution. Yang 2025 §4.4 mesh updating (`refs/text/yang2025_hybrid_boolean.txt:595-605`) describes mesh updating along refined intersection curves but does NOT prescribe a Render-LOD-stage face-emit dispatch like `tessellate_solid_bounded`. Cherchi 2022 §3 paper scope (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:293-319`) ends at the arrangement → patch output — Render LOD is downstream and uncovered by either paper. The probe's H1/H2/H3 attribution is the only empirical record at this layer.

---

## §3 Probe extension design

### §3.1 Y36Class enum extension

Existing `D1a/b/c/d` + `Other` variants preserved. New variants carved out of `Other`:

```rust
enum Y36Class {
    D1a, D1b, D1c, D1d,   // unchanged
    Other,                 // legacy bucket (now empty post-classification)
    OtherH1,              // sub-grid seam dominant (axis-aligned + grid-quantized)
    OtherH2,              // NMM-pair render asymmetry dominant
    OtherH3,              // residual (neither H1 nor H2)
}
```

### §3.2 Detection signatures (canary §2 verbatim)

**H1 — sub-grid seam:** ≥80% of boundary edge segments are axis-aligned in the quantized grid space (exactly one of the three quantized-delta components is non-zero). Quantization granularity per existing `y36_quantize_pos` (PR-Y36).

**H2 — NMM-pair render asymmetry (proxy):** ≥50% of NMM-edge count's worth of boundary edges have no peer in the per-face inventory's `kids_in_final` set. Proxy caveat: the count is over ALL boundary edges lacking a final-mesh peer, not gated to NMM-incident positions. Normalized against `outer_nmm_count` so faces with zero NMM never trip H2.

**H3 — residual.** Default.

Precedence: geometric H1 first → topological H2 → residual H3.

### §3.3 Probe outputs

- 2 new columns in unpaired-edge attribution TSV: `grid_aligned_count`, `nmm_asym_count`
- 4 new columns in face-inventory TSV: per-kid feature counts
- Per-invocation stderr summary line: `OtherH1=X OtherH2=Y OtherH3=Z`
- New aggregator file: `cross_cohort_summary.tsv` — one row per probe invocation, columns for total unpaired + each bucket count + H1/H2/H3 fractions of OTHER

### §3.4 Default-off byte parity

Probe gated on `Y36_INVERSE_PROBE=1` (Y36 env-var name preserved — extension, not rename). Default-off path is byte-identical to PR-Y36 ACCEPT baseline. Probe is observation-only — no `disc.positions`, `vertices`, `indices`, `face_ranges`, or arena state mutated.

### §3.5 What this extension does NOT do (canary §2.4)

- Does NOT walk topology to find true cross-face twins (H2 is a proxy; brief approved proxy fallback)
- Does NOT discriminate sub-quantization-granularity defects (PR-Y27 D.2 lives below H1's quantization grid)
- Does NOT change any classification behavior for existing D.1a/b/c/d outputs

---

## §4 Empirical findings — REFUTATION TABLE

### §4.1 F0020 inv#6 (load-bearing) — 39 unpaired edges

| Class | Count | % of total | Mechanism |
|---|---|---|---|
| D.1a | 9 | 23.1% | `boundary.len() < 3` planar entry gate |
| D.1b | 0 | 0.0% | earcut zero-emit on coincident 3-bounded |
| D.1c | 0 | 0.0% | ≥90% NMM boundary |
| D.1d | 8 | 20.5% | repair-pass drop |
| **D.1 total** | **17** | **43.6%** | unchanged from PR-Y36 |
| OtherH1 | **0** | **0.0%** | axis-aligned + grid-quantized ≥80% |
| OtherH2 | **3** | **7.7%** | NMM-asym ≥50% (kids 226 ×2, 231 ×1) |
| OtherH3 | **19** | **48.7%** | residual |
| **Other total** | **22** | **56.4%** | matches PR-Y36 |

### §4.2 Cohort sub-classification

| Case | Total | D.1a | D.1d | OtherH1 | OtherH2 | OtherH3 | H1 % of Other | H2 % of Other | H3 % of Other |
|---|---|---|---|---|---|---|---|---|---|
| F0044 | 12 | 0 | 0 | 0 | 0 | 12 | 0.0% | 0.0% | **100.0%** |
| F0045 | 38 | 0 | 0 | 0 | 0 | 38 | 0.0% | 0.0% | **100.0%** |
| R0092 | 43 | 0 | 0 | 0 | 12 | 31 | 0.0% | 27.9% | **72.1%** |

### §4.3 Cross-cohort prediction outcome — REFUTED

PR-Y36 canary §4.2 predicted:
- F0044/F0045 ≥ 80% H1 (per PR-Y27 D.2 = sub-grid seam mismatch on cylindrical-cap face seams)
- R0092 ≥ 80% H2 (per PR-Y27 D.3 = NMM-edge tessellation gap; ~44 legit NMM HEs ≈ 43 unpaired)
- F0020's 22 OTHER ≈ proportional mix of H1 + H2

| Prediction | Observed | Outcome |
|---|---|---|
| F0044 ≥80% H1 | 0% H1, 100% H3 | **REFUTED** |
| F0045 ≥80% H1 | 0% H1, 100% H3 | **REFUTED** |
| R0092 ≥80% H2 | 27.9% H2, 72.1% H3 | **REFUTED** (well below 80%) |
| F0020 OTHER mixed H1+H2 | 0% H1, 13.6% H2, 86.4% H3 | **REFUTED** |

All four predictions miss by ≥50 percentage points. The numeric thresholds in the PR-Y37 plan verdict logic (`Gate 6 outcome = refuted`) converge on **SHIP-INFRA + 6th-refutation framing**.

---

## §5 Why the H1/H2 detection failed (canary §4.2)

Three structural reasons the H1/H2 signatures as defined in the PR-Y36 plan do not capture the cohort defects:

### §5.1 H1 detector misses cohort cylinder-rim boundaries

F0044/F0045/R0092 cohort + several F0020 OTHER kids have **curved discretized loops** tracing cylinder rims or chamfer edges. The H1 detector requires X/Y/Z alignment (one quantized-delta component non-zero, two zero); cylinder rims trace circular polygons in planes that are typically NOT global-axis-aligned. PR-Y36 §4.2 framed H1 assuming planar boundaries; the cohort reality is curved. **H1 is structurally zero for cylinder-rim cases**, not measurement noise.

### §5.2 H2 proxy cannot fire on clean-arena cases

F0044 and F0045 have `outer_nmm_count = 0` (clean arena) by construction. The H2 threshold is `nmm_asym / outer_nmm_count ≥ 0.50`. With `outer_nmm_count = 0`, the threshold expression is undefined — short-circuited to "never fires" in the probe. **H2 is mathematically impossible on the F0044/F0045 cohort.** R0092 partially trips H2 (12/43) only because kids 22/24/26/27 are partial-NMM (`outer_nmm_count = 2`).

### §5.3 PR-Y27 D.2 is sub-quantization-granularity

PR-Y27 §3 footnote (`docs/audits/pr_y27_abort.md:98`):
> "D.2's defect is at positions that quantize to different grid cells but the f64 distance is sub-grid (within TAU_TESS_GRID_FACTOR). The watertight oracle's grid is `max_abs * 2e-6` ≈ several µm at typical CAD scales. Vertex disagreement at f32 precision (~µm) can fall on different sides of the grid."

The H1 detector quantizes vertex positions BEFORE the axis-alignment check. Any defect that lives below the quantization granularity is structurally invisible to H1 — the quantization step collapses sub-grid disagreement to identical grid cells, erasing the signal that distinguishes D.2 from "no defect."

---

## §6 H3 cross-cohort dominance — a new positive finding

The PR-Y36 cross-cohort hypothesis ("H1+H2 captures cohort") is **refuted**. But the cross-cohort signal is NOT absent — it shifted bucket.

**H3 dominates across all 4 cases:**

- F0020 OTHER: 48.7% of total unpaired (86.4% of OTHER)
- F0044: 100% of total unpaired (100% of OTHER)
- F0045: 100% of total unpaired (100% of OTHER)
- R0092: 72.1% of total unpaired (72.1% of OTHER)

This is a distinct empirical claim from the refuted hypothesis. The OTHER cluster **may still be cohort-shared**, but the SHARED mechanism is something the current PR-Y37 probe does not characterize positively — H3 is the residual bucket (everything not grid-aligned-axis AND not NMM-twin-asymmetric).

**Plausible H3 mechanism per canary §4.2 (banked, not confirmed):** F0044/F0045 H3 faces are clean-arena planar/cylindrical patches with `outer_nmm_count=0`, curved boundaries (cylinder rims, chamfer edges), unpaired edges at curve-discretization vertices. Mechanism candidate: f32-precision quantization disagreement between adjacent faces at curve-discretization vertices — PR-Y27 D.2 may still be the right mechanism, but the *signature* defined for it in PR-Y36 §4.2 doesn't capture it.

**The H3 cohort dominance IS a positive finding.** PR-Y36's prediction "H1+H2 dominates" is wrong; **"unpaired edges cluster on curved-boundary OR clean-arena partial-NMM faces that are NOT axis-aligned AND NOT NMM-pair-asymmetric"** is the actual cohort-wide cluster signal — discoverable only via the PR-Y37 probe.

---

## §7 PR-Y38 banked options (NOT a recommended anchor)

Per `feedback_no_last_bug` and `feedback_phase1_diagnosis_ranking_is_inference`, this spec banks 4 candidate options for PR-Y38 with rationale. **None is promoted to "the fix."** Empirical chain "fix → unpaired_count to 0" is not yet verified for any candidate.

### Option 1 — Refine H1 to sub-quantization-distance vertex-pair comparison

Probe PR-Y27 D.2 signature below the f32 ULP / oracle grid granularity. Replace axis-aligned-at-quantization check with: for each boundary edge, find the matching boundary edge in any neighboring face by position-proximity (not quantization equality), and report per-vertex f64 distance. Threshold: vertices disagree by ≥ ε (~f32 ULP at model magnitude) but ≤ quantization granularity. Estimated +150-300 LOC; requires neighbor-face lookup not in current probe.

### Option 2 — Refine H2 to per-segment NMM-incidence

Walk outer-loop half-edges in dispatch order; record per-position-pair whether the originating HE is NMM. H2 = (# segments where NMM AND no final-mesh peer) / (# NMM segments). Estimated complex extension: per-HE-to-per-position-segment mapping must handle edge-discretization expansion (one curved edge → N segments from one HE).

### Option 3 — Pivot: probe `count_unpaired_in_mesh` f32 → quantization round-trip directly

Accept H3 as a novel cohort-wide cluster. Probe the oracle's quantization round-trip on partial-NMM faces — does f32 vertex precision disagree across faces in a way that quantization erases? Foundation for a fresh investigation of D.2 below the current grid granularity. Banked in PR-Y36 §3.4: "kids 235 and 256 are present in inventory with 100% NMM (D.1c signature) but do NOT appear in unpaired attribution — Cherchi-Rust port byte-parity (Y34/Y35/Y35.1) fixed D.1c peer-pairing, but new defects emerged at F.4 quantization layer for partial-NMM faces."

### Option 4 — Cheap singleton: D.1d kids 218/232/233 survival fix

`tessellation/repair.rs:585` (`remove_nonmanifold_topology_aware`). Accounts for 8/40 = 20% of F0020 oracle unpaired. **Predicted F0020 outcome: 40 → ~32 unpaired; does NOT close Status:Failed**. Cohort regression risk: must verify F0044/F0045/R0092 zero-arena-drop status preserved. Estimated ~20 LOC. Not a Yang anchor; a hygiene PR.

---

## §8 Out of scope (banked, unchanged from PR-Y34/Y35/Y35.1/Y36)

- **F0020 Status:Failed unchanged.** PR-Y37 ships zero production code; oracle unpaired remains at 40 (probe Stage E_lod 39 boundary + 1 NMM).
- **F0044/F0045/R0092 Status unchanged.** Cohort baselines preserved exactly (12/38/43 unpaired).
- **139 yang_fast failing cases unchanged.** Baseline 10/157 passed.
- **Cherchi C++ TBB non-determinism unresolved.** Per PR-Y29-Y33 banked finding: even at `TBB_NUM_THREADS=1` some F0020 reruns vary. Use missing-count as gate, not extras.

This memo does NOT claim "this closes Yang" or "this is the last gap on Render LOD." Per `feedback_no_last_bug`, the OTHER cluster's true mechanism remains uncharacterized below H1/H2 signature granularity. **The OTHER cluster is now better measured than at PR-Y36 — but less understood as a fix shape.**

---

## §9 Risk

### §9.1 H1/H2 detection thresholds are first-cut heuristics

The 80% (H1 axis-aligned) and 50% (H2 NMM-asym) thresholds were proposed in the PR-Y36 plan as empirical heuristics. **The PR-Y37 results show these thresholds are too coarse for the cohort defects** (canary §5):

- H1 is structurally zero (0% across all 4 cases) — the signature is geometrically wrong for curved-boundary cohort cases, not threshold-mis-tuned
- H2 cannot fire on clean-arena cases (F0044/F0045 trivially excluded by `outer_nmm_count=0` denominator)
- H3 sweeps up 72-100% of cohort unpaired — too broad a residual bucket to drive PR-Y38 anchor selection alone

Future probe extensions must look **BELOW the f32 quantization grid** AND **inspect per-segment topology** (per-HE-to-per-segment NMM map). Both extensions are non-trivial; canary §4.3 options 1 and 2 estimate the LOC budgets.

### §9.2 Probe complexity creep

Cumulative probe code is ~707 LOC across PR-Y36 (462) + PR-Y37 (245) in `tessellation/mod.rs`. Further extension (Option 1 or 2) would add 150-300 LOC. Per `feedback_external_coherence`, the probe IS the reference for Render LOD attribution — but at some point the probe's complexity becomes its own maintenance burden. PR-Y38+ should weigh whether a structurally different investigation (e.g., Option 3's quantization round-trip probe) gives better signal-per-LOC than further extending H1/H2 detection.

### §9.3 6th refutation pattern signal

Per the canary's §4.1 escalation rule observation: 6 consecutive canary-stage no-fix outcomes on F0020 Render LOD (Y25/Y26/Y27/Y28/Y36/Y37). The cycle of refutation is the discipline working — each ABORT/SHIP-INFRA shrinks the candidate space — but team-lead should weigh whether a fundamentally different investigation lens is warranted, e.g., (a) reproducing Yang §4.4.1 mesh updating at the Render LOD layer, (b) bringing in Cherchi C++ ground truth at Stage F (impossible per Cherchi 2022 §3 scope), or (c) pivot to a different priority area entirely.

Per `feedback_no_last_bug`: this spec does NOT predict that PR-Y38 closes Render LOD. The PR-Y38 anchor selection is team-lead's decision; the 4 banked options each have rationale + cost + risk, none has a verified empirical-chain to `unpaired_count = 0`.
