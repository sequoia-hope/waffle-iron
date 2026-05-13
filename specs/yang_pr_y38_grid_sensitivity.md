# PR-Y38 — Grid-sensitivity probe at the watertight oracle (INFRA-CLASS)

**Authors:** spec-y38, canary-y38
**Parent commit:** `d632d5f` (plan baseline; canary observed live tree at `8778907` — see canary memo §1)
**Date:** 2026-05-13
**Verdict (from canary memo §5):** **SHIP-INFRA + CORROBORATION**

---

## §1 Context

PR-Y38 is the **7th investigational PR** on F0020 Render LOD (Y25/Y26/Y27/Y28/Y36/Y37/Y38). PR-Y36 + PR-Y37 mapped F0020's 39–40 unpaired Render LOD edges to source faces and sub-classified them into D.1a/b/c/d + H1/H2/H3 buckets. PR-Y37's load-bearing finding (canary memo §5, banked options 1/2 in §4.3) was that the **H3 cluster** dominates cohort-wide:

| Case | Unpaired | H3 fraction |
|---|---|---|
| F0020 OTHER | 22/39 | 48.7% of total unpaired (86.4% of OTHER) |
| F0044 | 12/12 | **100%** |
| F0045 | 38/38 | **100%** |
| R0092 | 31/43 | 72.1% |

PR-Y37 could not characterize H3 with the existing H1 (axis-aligned at grid granularity) / H2 (NMM-asymmetric proxy) signatures, and attributed the residual to "sub-quantization-granularity defects" — vertex disagreements below the watertight oracle's grid (PR-Y27 §3 footnote: "D.2's defect is at positions that quantize to different grid cells but the f64 distance is sub-grid").

The first six investigations (Y25/Y26/Y27/Y28/Y36/Y37) all took the **watertight oracle's count as ground truth** without questioning it. PR-Y38 pivots: rather than refining the same source-face probe again, **interrogate the measurement framework itself** — does the oracle's `f32 → i64` quantization through `TAU_TESS_GRID_FACTOR=1e-5` inflate the 40-unpaired count with phantom edges from f32 round-trip noise? (Cite PR-Y37 canary §5; PR-Y27 abort memo §3 footnote.)

This spec documents the env-gated grid-sensitivity probe that the canary built and ran, what it found, and what it corroborates for PR-Y39 anchor selection.

## §2 Why infra-class

Per `feedback_anchor_before_fix` strategic escalation: three wrong anchors in a row → stop bisecting, build a reference comparison. PR-Y36 was the reference comparison for D.1; PR-Y37 was the reference comparison for cross-cohort H1/H2 overlap. Both negative. PR-Y38 is the **first** of the seven investigational cycles to question the oracle baseline itself — and therefore the **first** to definitively eliminate "measurement artifact" as a possible mechanism.

This is the 7th no-fix cycle. Zero production logic is changed. The +179 LOC additive probe is env-gated behind a single `if std::env::var("Y38_GRID_PROBE").as_deref() == Ok("1")` check; default-off path is byte-identical (Gate 2 verified). Per `feedback_no_last_bug` and `feedback_phase1_diagnosis_ranking_is_inference`, this spec does NOT claim that grid-sensitivity is a final question, only that it has now been asked and answered for the current oracle definition and ±1-cell neighborhood.

## §3 Probe design

**Insertion site:** `crates/test-harness/src/oracle.rs::check_watertight_mesh`, at L244-246 (canary worktree). The probe runs **after** the production `non_paired` is computed (so it observes the same data the oracle reports on) and **before** the verdict return.

The production quantization (cited verbatim from `crates/test-harness/src/oracle.rs:191-200`):

```rust
let max_abs = mesh.vertices.iter().map(|v| v.abs()).fold(0.0_f32, f32::max);
let grid_size = (max_abs as f64 * TAU_TESS_GRID_FACTOR).max(TAU_TESS_GRID_MIN);
let inv_grid = 1.0 / grid_size;
let quantize = |v: f32| -> i64 { (v as f64 * inv_grid).round() as i64 };
```

Constants (`crates/kernel/src/units.rs:60-63`):
- `TAU_TESS_GRID_FACTOR: f64 = 1e-5`
- `TAU_TESS_GRID_MIN: f64 = 1e-10`

At magnitude 1, f32 ULP ≈ 1.2e-7; the production grid is ~83× wider than f32 noise. The probe replicates the quantization at six grid multipliers and inspects neighborhoods.

### §3.1 Per-grid sweep — `y38_count_non_paired_at_multiplier`

For each `m ∈ {0.5, 1.0, 2.0, 4.0, 10.0, 100.0}`:

```rust
let grid_size_m = (max_abs as f64 * TAU_TESS_GRID_FACTOR * m).max(TAU_TESS_GRID_MIN * m);
```

Both the relative factor AND the absolute floor scale with `m`, so a tighter `m=0.5` permits a smaller floor and a looser `m=100.0` permits a larger floor. Re-walk `mesh.indices`/`mesh.vertices`, build a fresh `HashMap<((i64,i64,i64),(i64,i64,i64)), usize>` of edge counts at the alternate grid, count edges with `count != 2`, and return `(non_paired, total_edges)`. (Canary memo §2, L294-321.)

### §3.2 Near-pair scan — `y38_near_pair_scan`

For each `(va, vb)` in the production `non_paired` slice (at 1× grid), enumerate the 27 i64-offsets `(dx, dy, dz) ∈ {-1, 0, +1}³` for **both** endpoints. Total candidate pairs: 27 × 27 − 1 (self) − degenerate = at most 728. For each candidate edge `make_edge(va + Δa, vb + Δb)`, look up in the production `edge_counts`. If found with `count >= 1`, record Chebyshev distance `max(|Δa|, |Δb|)` across the 6 axes. Bucket the **minimum** Chebyshev distance per unpaired edge: `dist1`, `dist2+` (vacuous under ±1 scan; preserved in TSV header for schema compatibility), or `isolated` (no near-pair found).

Rationale for ±1 (not ±2): f32 ULP at meter scale is ~1.2e-7, far below 1 i64-cell at 1e-5 relative grid. Realistic round-trip drift sits inside ±1. ±2 (125² candidates) banked for PR-Y39 if needed. (Canary memo §2.4.)

### §3.3 Output schema

One TSV per `check_watertight_mesh` invocation at `$Y38_GRID_PROBE_DIR/Y38_inv{NNNN}_grid_sensitivity.tsv`, with filename disambiguated by a process-local `Y38_INVOCATION_COUNTER: AtomicUsize`. Header:

```
case  total_edges  unpaired_at_05x  unpaired_at_1x  unpaired_at_2x  unpaired_at_4x  unpaired_at_10x  unpaired_at_100x  near_pair_dist1  near_pair_dist2  isolated  non_paired_at_1x_oracle
```

`non_paired_at_1x_oracle` is a sanity-check duplicate of `unpaired_at_1x` (production count vs probe-recomputed count); a mismatch would indicate probe/oracle disagreement. Across all 8 canary invocations they match perfectly.

### §3.4 Env gates

- `Y38_GRID_PROBE=1` — required to fire the probe
- `Y38_GRID_PROBE_DIR=<path>` — required output directory (probe `mkdir -p`'s)
- `Y38_PROBE_CASE_NAME=<label>` — optional label prefix for the TSV `case` column

## §4 Empirical findings — CORROBORATION TABLE (LOAD-BEARING)

All findings reproduced from canary memo §3, evidenced by TSV files in `/tmp/y38-*` and stdout logs `/tmp/y37-{pre,final,cohort}.log`. All gates GREEN; determinism verified across 3 reruns.

### §4.1 F0020 grid table

| Multiplier | grid_size | unpaired |
|---|---|---|
| 0.5× | `max_abs * 0.5e-5` | **40** |
| 1.0× | `max_abs * 1e-5` (oracle baseline) | **40** |
| 2.0× | `max_abs * 2e-5` | **40** |
| 4.0× | `max_abs * 4e-5` | **40** |
| 10.0× | `max_abs * 10e-5` | **40** |
| 100.0× | `max_abs * 100e-5` | **40** |

### §4.2 F0020 near-pair attribution at 1× (±1 scan)

| Bucket | Count | % of 40 |
|---|---|---|
| `dist1` | 0 | 0.0% |
| `dist2` | 0 | 0.0% (vacuous; ±1 scan can't produce dist 2) |
| `isolated` | **40** | **100.0%** |

### §4.3 Cohort grid sensitivity

| Case | Total edges | 0.5× | 1× | 2× | 4× | 10× | 100× | dist1 | dist2 | isolated |
|---|---|---|---|---|---|---|---|---|---|---|
| F0020 | 188 | 40 | **40** | 40 | 40 | 40 | 40 | 0 | 0 | 40 |
| F0044 | 180 | 12 | **12** | 12 | 12 | 12 | 12 | 0 | 0 | 12 |
| F0045 | 472 | 38 | **38** | 38 | 38 | 38 | 38 | 0 | 0 | 38 |
| R0092 | 280 | 43 | **43** | 43 | 43 | 43 | **45** | 0 | 0 | 43 |
| R0045 | 950 | 88 | **88** | 88 | 88 | 88 | 88 | 0 | 0 | 88 |

### §4.4 Determinism gate

3 F0020 reruns produce byte-identical TSV row each time (`40 40 40 40 40 40 0 0 40 40`). The probe is deterministic.

### §4.5 R0092 100× anomaly

R0092's count goes UP from 43 to 45 at 100× — the only deviation from flatness. This is **over-merging**, not phantom recovery: at `grid = max_abs * 1e-3`, originally-distinct vertices collapse to the same i64 cell, breaking previously-paired edges. The direction (UP, not DOWN) is corroborating evidence that 1× sits on the correct side of the precision/coverage tradeoff: looser grids over-merge before they recover any phantoms.

## §5 Why the phantom hypothesis is refuted

Three independent lines of evidence (canary memo §4):

1. **Stable count across 200× grid range.** F0020's 40-unpaired holds across `{0.5×, 1×, 2×, 4×, 10×, 100×}` — two orders of magnitude in quantization scale. If even a fraction of the 40 were phantom edges from f32 round-trip noise (which lives at ~1.2e-7 absolute), widening the grid 4× or 10× would have pulled near-neighbor candidates into the same i64 cell and dropped the count. It does not.

2. **100% isolated at ±1 cells.** Every one of F0020's 40 unpaired edges sits in an i64-cell whose ±1 neighborhood (27³ candidates per endpoint pair) contains **no** edge that hashed anywhere into the production edge map. There is no near-neighbor that would pair if the grid were a hair looser — the unpaired edges are not "almost pairs that the grid missed," they are genuinely isolated boundaries.

3. **R0092 over-merging at 100×.** The only non-flat data point (43 → 45) goes in the direction of pair breakage from over-merging, not pair recovery from phantom collapse. This corroborates that the 1× grid is on the correct side of the precision/coverage tradeoff.

The PR-Y27 §3 footnote framing of "sub-quantization granularity" as a *mechanism class* remains valid — defects at f64 distance below the grid spacing certainly do exist. But that class does NOT manifest as phantom unpaired edges in the watertight oracle at the current grid: any edge that lives at sub-grid f64 distance from a partner would either (a) collide in the same i64 cell at production granularity and be paired, or (b) collide at one of the wider multipliers tested here. Neither happens. The 40 are real.

## §6 What this corroborates for prior PRs

All 6 prior investigational PRs (Y25/Y26/Y27/Y28/Y36/Y37) implicitly assumed the watertight oracle's unpaired count was geometric ground truth. PR-Y38 was the first cycle to test that assumption. **The assumption was sound.** This is a positive, separate finding from the phantom hypothesis being refuted:

- **Phantom hypothesis refuted** (negative finding, §5): no measurement artifact at the current oracle's 1e-5 grid; the 40 are real geometric defects.
- **Prior 6 PRs corroborated** (positive finding, this section): every attribution number those cycles produced (D.1a=9, D.1b=0, D.1c=0, D.1d=8, OtherH1=0, OtherH2=3, OtherH3=19, cohort H3 100%/100%/72.1%) was computed against a sound ground-truth baseline. PR-Y36's D.1c=0% refutation stands as real geometry. PR-Y37's H3 = 56.4% of F0020 / 100% of F0044/F0045 / 72.1% of R0092 stands as real geometry. PR-Y37's banked Options 1/2 (refine source-face probe to characterize H3) remain the right scaffolding for PR-Y39 because 40 is the correct target count.

This positive corroboration is what justifies PR-Y38's "INFRA-CLASS-with-purpose" framing: 7th no-fix cycle, but the first to *eliminate* a hypothesis (measurement artifact) rather than merely refining the same probe. Per `feedback_phase1_diagnosis_ranking_is_inference`, corroboration is itself an empirical finding and is presented here as such — not as inference, but as a direct measurement against the alternative hypothesis (phantom inflation), which has now failed to reproduce across 6 grid widths and 4 cohort cases.

## §7 PR-Y39 anchor recommendation

Per canary memo §4, the recommended PR-Y39 anchor is:

**PRIMARY: refine the source-face probe to characterize the H3 cluster** (PR-Y37 banked Options 1 and 2).

- **Option 1** (PR-Y37 §4.3 / §8 #1) — replace H1's axis-aligned-edge check with a sub-quantization-distance vertex-pair comparison: for each boundary edge, find the matching boundary edge in any neighboring face by **position-proximity** (not quantization equality), and report per-vertex f64 distance. Threshold for "sub-grid seam" = vertices disagree by ≥ε (where ε ≈ f32 ULP at the model's magnitude) but ≤ quantization granularity. Directly probes the PR-Y27 D.2 mechanism CLASS that PR-Y38 confirmed is below the oracle's grid (but is NOT inflating the unpaired count).
- **Option 2** (PR-Y37 §4.3 #2 / §8 #2) — refine H2 to a precise per-segment NMM-incidence map: walk outer-loop half-edges in dispatch order and record per-position-pair whether the originating HE is NMM. Then count true H2 = (# NMM segments with no final-mesh peer) / (# NMM segments).

40 is now an **empirically validated target count** for either Option.

**NOT recommended (canary §4 + §5):**

- **Do not tune `TAU_TESS_GRID_FACTOR` upward.** The grid is fine. Loosening it 100× starts to over-merge (R0092: 43→45). Tighter (0.5×) doesn't add edges. Status quo at 1e-5 sits on the correct side of the curve.
- **Do not adopt position-tolerance edge-pairing.** The brief offered this as the "reframe" PR-Y39 candidate under PHANTOM verdict. Since verdict is CORROBORATION, this is unnecessary; it would also weaken the watertight oracle's discrimination of real defects with no empirical justification.

## §8 Out of scope

Banked for separate PRs (unchanged from prior cycles):

- **F0020 Status:Failed.** PR-Y38 does not change F0020's verdict; the 40 unpaired are still real and the case is still Failed. This is the 7th investigational PR without a fix shape on F0020 Render LOD. Per `feedback_no_last_bug`, no claim is made here about how close the investigation is to closure.
- **F0044 / F0045 / R0092 unpaired counts.** Stable cohort baselines (12 / 38 / 43); deferred to PR-Y39+.
- **139 yang_fast cases.** Out of scope; corpus baseline preserved at 10/157.
- **Cherchi C++ TBB non-determinism.** Banked since PR-Y31; not in PR-Y38 scope.
- **D.1d narrow fix.** PR-Y36 §4.2 banked alternative (3 kids 218/232/233 dropped at `remove_nonmanifold_topology_aware`, 8/40 = 20% of F0020 oracle unpaired). Survives as a separate hygiene candidate; would NOT close Status:Failed. Out of scope.
- **±2 near-pair scan + sub-quantization vertex-distance probe.** Banked for PR-Y39 if Options 1/2 don't localize H3.

This is NOT a "closes Yang" PR (`feedback_no_last_bug`).

## §9 Risk / mitigation

None material.

- The probe is env-gated and default-off byte-identical (canary Gate 2 GREEN: baseline F0020 with `Y38_GRID_PROBE` unset reproduces exact 40-unpaired Status:Failed signature; re-verified after Gate 8).
- The new finding eliminates a hypothesis rather than introducing new behavior — there is no production code change that could regress.
- `cargo test -p kernel --lib` and `yang_fast` baselines preserved (Gates 6/7 GREEN: 1262/24/42 and 10/157).
- Probe determinism verified across 3 F0020 reruns (Gate 8 GREEN).

**Live-tree hygiene caveat.** The canary documented a near-miss (memo §1.1) where an Edit call passed an absolute path to the live tree at `/home/claude/workspace/...` instead of the worktree at `/home/claude/workspace/.claude/worktrees/canary-y36/...`. The live tree briefly carried the probe diff; canary reverted with `git checkout --` after verifying the file was clean prior to the unintended edit. Adversary-y38 should re-verify the live tree is clean before staging.

**Paper-citation note.** Per `feedback_external_coherence`: no paper (Yang 2025, Cherchi 2022, Cherchi 2020) covers Render LOD watertight oracle calibration. The probe IS the empirical reference. Yang §4.4.1 ("Mesh updating", refs/text/yang2025_hybrid_boolean.txt:605-610) and Cherchi 2022 §3 (refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:240-260; mesh arrangement as well-formed simplicial complex with implicit point representation) describe upstream watertightness guarantees, NOT downstream Render LOD f32 quantization. PR-Y38 is paper-orthogonal (oracle measurement, not boolean pipeline). PR-Y27 abort memo §3 footnote describes the "sub-quantization granularity" mechanism class that motivated this canary; the canary confirms the class exists in principle but does NOT manifest as phantom unpaired edges at the current oracle definition.

---

**End of spec.**
