# PR-Y38 Canary — Grid-sensitivity probe at the watertight oracle; F0020's 40-unpaired baseline is REAL (not a phantom from f32 round-trip); 100% isolated, stable across all grids cohort-wide; **SHIP-INFRA + corroboration**

**Author:** canary-y38
**Date:** 2026-05-13
**Worktree branch:** `worktree-canary-y36` (re-used; PR-Y36 + PR-Y37 scaffolding already co-located on this worktree)
**Baseline:** `8778907` (PR-Y35.1 audit ACCEPT — main HEAD at canary start; the plan brief named `d632d5f` for PR-Y37, but PR-Y36/Y37 work was never committed to main — only the worktree carries it. See §1 below for the reconciliation.)
**Mandate:** Build an env-gated grid-sensitivity probe at `oracle.rs::check_watertight_mesh`. Sweep `TAU_TESS_GRID_FACTOR` at multipliers {0.5, 1, 2, 4, 10, 100} and perform a ±1-cell near-neighbor scan on F0020's 40 unpaired edges and the F0044/F0045/R0092/R0045 cohort. Determine whether the unpaired count is PHANTOM (drops with wider grid), REAL (stable), or LAYERED (partial). Recommend PR-Y39 anchor.
**Verdict:** **SHIP-INFRA + corroboration.** All 8 gates GREEN. F0020 `unpaired_at_1x = 40` is stable across all six grid multipliers (40 / 40 / 40 / 40 / 40 / 40) and 100% near-pair isolated (`dist1=0, dist2=0, isolated=40`). Cohort same: F0044=12, F0045=38, R0092=43 (43→45 at 100× — over-merging at extreme widening, not phantom recovery), R0045=88 — all stable and 100% isolated at ±1 scan. The 40-unpaired baseline is REAL geometric defect, NOT f32 quantization noise. PR-Y39 anchor confirmed at: refine the source-face probe (PR-Y37 banked Options 1/2) with confidence that 40 is the right target.

---

## §0 Summary

PR-Y38 added an env-gated probe to `crates/test-harness/src/oracle.rs::check_watertight_mesh` that, when `Y38_GRID_PROBE=1` is set:

1. **Per-grid sweep.** Re-quantizes every triangle vertex with `TAU_TESS_GRID_FACTOR * m` at `m ∈ {0.5, 1.0, 2.0, 4.0, 10.0, 100.0}` and re-counts non-paired edges at each multiplier.
2. **±1-cell near-neighbor scan.** For each unpaired edge at the default 1× grid, walks the 27 i64-neighbors of each endpoint and checks whether any candidate edge `(va', vb')` (perturbed in {−1, 0, +1}³ × {−1, 0, +1}³ minus self) exists in the edge map with count ≥ 1. Classifies by minimum Chebyshev distance: `dist1`, `dist2` (vacuous under ±1 scan), or `isolated` (no near-pair found).

Probe code is **entirely env-gated** behind one `if std::env::var("Y38_GRID_PROBE").as_deref() == Ok("1") { ... }` check. Default-off path is byte-identical (Gate 2).

**F0020 grid-sensitivity table (load-bearing):**

| Multiplier | grid_size (m) | unpaired |
|---|---|---|
| 0.5× | `max_abs * 0.5e-5` | **40** |
| 1.0× | `max_abs * 1e-5` (oracle baseline) | **40** |
| 2.0× | `max_abs * 2e-5` | **40** |
| 4.0× | `max_abs * 4e-5` | **40** |
| 10.0× | `max_abs * 10e-5` | **40** |
| 100.0× | `max_abs * 100e-5` | **40** |

**F0020 near-pair attribution at 1× (±1 scan, 27² candidates per edge minus self):**

| Bucket | Count | % of 40 |
|---|---|---|
| `dist1` | 0 | 0.0% |
| `dist2` | 0 | 0.0% (vacuous; ±1 scan can't produce dist 2) |
| `isolated` | **40** | **100.0%** |

**Cohort grid sensitivity (Gate 5):**

| Case | Total edges | 0.5× | 1× | 2× | 4× | 10× | 100× | dist1 | dist2 | isolated |
|---|---|---|---|---|---|---|---|---|---|---|
| F0020 | 188 | 40 | **40** | 40 | 40 | 40 | 40 | 0 | 0 | 40 |
| F0044 | 180 | 12 | **12** | 12 | 12 | 12 | 12 | 0 | 0 | 12 |
| F0045 | 472 | 38 | **38** | 38 | 38 | 38 | 38 | 0 | 0 | 38 |
| R0092 | 280 | 43 | **43** | 43 | 43 | 43 | **45** | 0 | 0 | 43 |
| R0045 | 950 | 88 | **88** | 88 | 88 | 88 | 88 | 0 | 0 | 88 |

R0092's 43→45 jump at 100× is the only deviation. It is **not** phantom recovery — the count went UP, not down. The mechanism is over-merging: at `1e-3 * max_abs` grid spacing, originally-distinct vertices collapse to the same i64 cell, breaking previously-paired edges. This is corroborating evidence that 1× is already on the correct side of the phantom/over-merge tradeoff.

**Verdict logic (from the brief):**

> - SHIP-INFRA + reframe if F0020 unpaired drops dramatically at 2-4× → tune `TAU_TESS_GRID_FACTOR` upward.
> - **SHIP-INFRA + corroboration** if F0020 stays stable across all grids → defect is real; PR-Y39 should refine the source-face probe with confidence.
> - SHIP-INFRA + LAYERED if F0020 drops partway → tune grid AND fix residual.
> - ABORT if Gate 1/6/7 RED, Gate 8 non-deterministic, or Gate 2 default-off byte parity broken.

F0020 stays flat at 40 across {0.5×, 1×, 2×, 4×, 10×, 100×} and 100% isolated — verdict is **SHIP-INFRA + corroboration**.

**PR-Y39 anchor recommendation:** PR-Y37's banked Options 1/2 (refine the source-face probe to discriminate H3 mechanism) are the right next step. PR-Y38 has eliminated the "40 is partly phantom" hypothesis — every one of those 40 edges is a real geometric boundary at the 1e-5 relative grid, with no near-neighbor pair within ±1 cell. This is the **7th consecutive canary-stage finding-no-fix-shape outcome** on F0020 Render LOD (Y25/Y26/Y27/Y28/Y36/Y37/Y38), but is the first one to definitively rule out a measurement-artifact mechanism — the kind of empirical clarification that justifies infra-class.

Per `feedback_no_last_bug`, this memo does **not** claim "40 is the final count" or "phantom hypothesis is permanently closed." It claims that **at the current oracle quantization (1e-5 relative) and within a ±1 i64-cell neighborhood, F0020's 40 unpaired edges are isolated and stable**. There may be larger-radius near-neighbors (≥2 cells), or completely different oracle definitions (e.g., position-tolerance edge matching rather than quantization), under which the count is different. Out of scope for PR-Y38.

---

## §1 Discipline

### Live tree untouched

The PR-Y38 brief mandates worktree-only. Mid-canary I caught a near-miss: an Edit call passed an absolute path to the **live tree** at `/home/claude/workspace/...` rather than the worktree at `/home/claude/workspace/.claude/worktrees/canary-y36/...`. The live tree briefly had the probe diff. I:

1. Copied the patched file to the worktree (`cp live worktree`).
2. Reverted the live tree (`git checkout -- crates/test-harness/src/oracle.rs`).
3. Confirmed live tree is now clean.

Both before and after, the live tree is at the same content as origin/main `8778907`. Per `feedback_adversary_no_destructive_git`, `git checkout --` is acceptable when the file was clean prior to the unintended edit (verified by inspecting the diff before reverting); no upstream work was destroyed. Documenting it here for adversary-y38's audit trail.

### Worktree state (verbatim)

```
$ git status
On branch worktree-canary-y36
Changes not staged for commit:
	modified:   app/tests/cases/assay/results.json
	modified:   crates/kernel/src/tessellation/mod.rs
	modified:   crates/test-harness/src/oracle.rs
Untracked files:
	docs/audits/pr_y36_canary.md
	docs/audits/pr_y37_canary.md
	specs/yang_pr_y36_inverse_probe.md
	specs/yang_pr_y37_other_classification.md
```

`results.json` is the test-harness runner artifact (auto-mutated by yang_fast). `tessellation/mod.rs` is the PR-Y36/Y37 inverse-direction probe scaffolding (411 LOC + 297 LOC respectively, totaling +708 — verified by `git diff HEAD --numstat`). PR-Y38 instrumentation is **only** in `crates/test-harness/src/oracle.rs` (+179 LOC). See §1.3 for the verbatim diff.

### Worktree diff (verbatim)

```
$ git diff HEAD --stat
 app/tests/cases/assay/results.json    | 138 +++----
 crates/kernel/src/tessellation/mod.rs | 711 +++++++++++++++++++++++++++++++++-
 crates/test-harness/src/oracle.rs     | 179 +++++++++
 3 files changed, 956 insertions(+), 72 deletions(-)

$ git diff HEAD --numstat
69	69	app/tests/cases/assay/results.json
708	3	crates/kernel/src/tessellation/mod.rs
179	0	crates/test-harness/src/oracle.rs
```

**PR-Y38 isolated diff** (`git diff HEAD -- crates/test-harness/src/oracle.rs`): +179 LOC, 0 deletions. All additions inside a single `if std::env::var("Y38_GRID_PROBE")...` gate plus three helper functions defined after the public function. Carryover from PR-Y36/Y37 (`tessellation/mod.rs` 708 LOC and `results.json` 138 LOC churn) is explicitly **not** PR-Y38's territory; per `feedback_implementer_anti_fabrication_diff`, the implementer agent in Phase 5 must stage only `crates/test-harness/src/oracle.rs` (plus this memo + the spec) when committing to live main.

### Infra-class framing

PR-Y38 is the 3rd canary in the inverse-direction probe arc (Y36/Y37/Y38) and the 7th canary on F0020 Render LOD (Y25/Y26/Y27/Y28/Y36/Y37/Y38). Per `feedback_phase1_diagnosis_ranking_is_inference`, the brief mandates measurement, not fix-shape commitment. Per `feedback_no_last_bug`, the memo explicitly does NOT claim "this closes Yang." Zero production logic changed. The probe is env-gated and additive.

---

## §2 Method

### Probe insertion site

`crates/test-harness/src/oracle.rs::check_watertight_mesh`. The function as previously written computes `edge_counts` (a `HashMap<PosEdge, usize>` of quantized-position edges → multiplicity) and a `non_paired` slice of edges with multiplicity ≠ 2. The probe is inserted **after** these are computed and **before** the verdict return, so it observes exactly the same data the oracle reports on.

Probe entry point at L243-L246:

```rust
if std::env::var("Y38_GRID_PROBE").as_deref() == Ok("1") {
    y38_grid_sensitivity_probe(mesh, max_abs, &edge_counts, &non_paired);
}
```

### Per-grid sweep (`y38_count_non_paired_at_multiplier`)

For each multiplier `m`:

```rust
let grid_size_m = (max_abs as f64 * TAU_TESS_GRID_FACTOR * m).max(TAU_TESS_GRID_MIN * m);
let inv_grid_m = 1.0 / grid_size_m;
```

Note: `TAU_TESS_GRID_MIN * m` so that the absolute floor also scales — a tighter `m=0.5` permits a smaller absolute floor; a looser `m=100` permits a larger absolute floor. This is the same scaling pattern used in the brief's `grid_size_m` formula. Both `TAU_TESS_GRID_FACTOR` and `TAU_TESS_GRID_MIN` are pulled from `crates/kernel/src/units.rs:60-63`.

Then re-walk `mesh.indices` and `mesh.vertices`, build a fresh `HashMap<Y38PosEdge, usize>` of edge counts at the alternate grid, count `non_paired = c != 2`, and return `(non_paired, total_edges)`.

### Near-pair scan (`y38_near_pair_scan`)

For each `(va, vb)` in `non_paired` (at 1× grid):

- Enumerate all 27 i64-offsets `(dx, dy, dz) ∈ {-1,0,+1}³` for **both** endpoints. Total candidate pairs: 27 × 27 − 1 (excluding self) − degenerate (where `va' == vb'`) = at most 728.
- For each candidate edge `make_edge(va + Δa, vb + Δb)`, look up in `edge_counts`. If `count >= 1` (anything that hashed under the same key at 1×), record the Chebyshev distance `max(|Δa|, |Δb|)` across all 6 axes.
- Bucket the **minimum** Chebyshev distance observed for that unpaired edge: `dist1`, `dist2+`, or `isolated`.

**Scan radius choice (±1).** The brief recommends ±1 and notes ±2 (125² = 15,625 candidates per edge) is feasible. I chose ±1 (max possible Chebyshev distance = 1) for two reasons: (a) the load-bearing question is *whether nearby f32 round-trip drift creates phantom edges*, and f32 ULP at meter scale is ~1.2e-7 — far below 1 i64-cell at 1e-5 relative grid. If two edges are within 1 ULP they collide in the same i64 cell; if they're a few ULPs apart they end up in adjacent cells (≤1 distance). Larger drifts are unphysical for f32 round-trip noise. (b) ±1 already produces a definitive `isolated = total` result for all 5 cases, so widening to ±2 cannot change the qualitative answer.

The `dist2` column in the TSV header is preserved for compatibility with the brief's schema; under ±1 scan it's always 0. ±2 would require a 125²-candidate inner loop per edge but would also need extending the bucket to `dist3+`. Banked for PR-Y39 if needed.

### Output (`y38_grid_sensitivity_probe`)

One TSV per invocation, written to `$Y38_GRID_PROBE_DIR/Y38_inv{NNNN}_grid_sensitivity.tsv`. Filename uses a process-local atomic counter (`Y38_INVOCATION_COUNTER`) for monotonic numbering — this disambiguates multiple calls during batch runs (e.g., `spotlight_f0044` invokes `check_watertight_mesh` three times for F0044/F0045/R0092). The canary documents the inv-to-case mapping manually based on spotlight run ordering (§3 below).

The TSV header:

```
case	total_edges	unpaired_at_05x	unpaired_at_1x	unpaired_at_2x	unpaired_at_4x	unpaired_at_10x	unpaired_at_100x	near_pair_dist1	near_pair_dist2	isolated	non_paired_at_1x_oracle
```

The `non_paired_at_1x_oracle` column is a sanity-check duplicate: it should equal `unpaired_at_1x` (which is recomputed independently in the probe via `y38_count_non_paired_at_multiplier(m=1.0)`). If those two columns drift, the probe's edge-counting is inconsistent with the oracle's — a red flag. Across all 8 invocations in the gates below, they match perfectly.

### 8 Gates (commands and verdicts)

| # | Gate | Command (abbreviated) | Verdict | Evidence |
|---|---|---|---|---|
| 1 | Build | `cargo build -p test-harness --lib` | **GREEN** | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.50s`. One format-string typo fixed before final compile (`{0,1}` → `0 or 1`); see §1 git history if needed. |
| 2 | Default-off byte parity | `YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 cargo test ... spotlight_f0020 ...` | **GREEN** | `Status: Failed, watertight_mesh: 40 unpaired edges out of 188 total (39 boundary, 1 non-manifold); 8 of 113 triangles are degenerate; 10 inter-face triangle penetrations` — byte-identical to PR-Y37 baseline. |
| 3 | Probe fires | `Y38_GRID_PROBE=1 Y38_GRID_PROBE_DIR=/tmp/y38-probe ...` | **GREEN** | One TSV `Y38_inv0000_grid_sensitivity.tsv` produced; header + row populated. |
| 4 | F0020 grid table | (Gate 3 TSV) | **GREEN** | `40 40 40 40 40 40`; near-pair `0 0 40`. F0020 is REAL geometric defect, NOT phantom. |
| 5 | Cohort grid tables | `spotlight_f0044`, `spotlight_r0045` | **GREEN** | F0044=12, F0045=38, R0092=43→45 at 100×, R0045=88; all 100% isolated. See §3. |
| 6 | kernel lib regression | `cargo test -p kernel --lib` | **GREEN** | `1262 passed; 24 failed; 42 ignored` — matches required baseline. |
| 7 | yang_fast corpus | `YANG_BOOLEAN=1 cargo test ... yang_fast --test-threads=1` | **GREEN** | `10/157 passed, 139 failed, 8 errored (skipped 33 known timeouts)` — matches required baseline. |
| 8 | Probe determinism (3 reruns) | F0020 spotlight × 3 with probe | **GREEN** | All 3 reruns produce identical row: `40 40 40 40 40 40 0 0 40 40`. |

### Plain-language commands

```bash
# Gate 1
cd /home/claude/workspace/.claude/worktrees/canary-y36
cargo build -p test-harness --lib

# Gate 2 (default-off baseline)
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture

# Gate 3 (probe fires F0020)
rm -rf /tmp/y38-probe && mkdir -p /tmp/y38-probe
Y38_GRID_PROBE=1 Y38_GRID_PROBE_DIR=/tmp/y38-probe Y38_PROBE_CASE_NAME=F0020 \
  YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture

# Gate 5a (F0044 batch: F0044/F0045/R0092)
rm -rf /tmp/y38-cohort && mkdir -p /tmp/y38-cohort
Y38_GRID_PROBE=1 Y38_GRID_PROBE_DIR=/tmp/y38-cohort Y38_PROBE_CASE_NAME=F0044_batch \
  YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_f0044 --ignored --nocapture

# Gate 5b (R0045)
rm -rf /tmp/y38-r0045 && mkdir -p /tmp/y38-r0045
Y38_GRID_PROBE=1 Y38_GRID_PROBE_DIR=/tmp/y38-r0045 Y38_PROBE_CASE_NAME=R0045 \
  YANG_BOOLEAN=1 \
  cargo test -p test-harness --test assay_randomized -- spotlight_r0045 --ignored --nocapture

# Gate 6 (kernel lib regression)
cargo test -p kernel --lib

# Gate 7 (yang_fast corpus)
YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture --test-threads=1

# Gate 8 (determinism × 3)
for i in 1 2 3; do
  rm -rf /tmp/y38-det$i && mkdir -p /tmp/y38-det$i
  Y38_GRID_PROBE=1 Y38_GRID_PROBE_DIR=/tmp/y38-det$i Y38_PROBE_CASE_NAME=F0020_det$i \
    YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 \
    cargo test -p test-harness --test assay_randomized -- spotlight_f0020 --ignored --nocapture
  cat /tmp/y38-det$i/*.tsv | tail -1
done
```

---

## §3 Empirical tables

### F0020 (load-bearing)

Spotlight: `spotlight_f0020` (`assay_randomized.rs:286`). One mesh invocation per spotlight run.

| Column | Value |
|---|---|
| `case` | F0020_inv0 |
| `total_edges` | 188 |
| `unpaired_at_05x` | 40 |
| `unpaired_at_1x` | **40** ← oracle baseline; matches `non_paired_at_1x_oracle` |
| `unpaired_at_2x` | 40 |
| `unpaired_at_4x` | 40 |
| `unpaired_at_10x` | 40 |
| `unpaired_at_100x` | 40 |
| `near_pair_dist1` | 0 |
| `near_pair_dist2` | 0 (vacuous; ±1 scan) |
| `isolated` | 40 |
| `non_paired_at_1x_oracle` | 40 |

**Reading.** Every one of F0020's 40 unpaired edges sits in an i64-cell whose ±1 neighborhood is empty of any candidate edge that hashes anywhere in the edge map. There is no near-neighbor that would pair if the grid were a hair looser. Widening the grid 200× (100× / 0.5×) does not change the count — the geometry holds 40 unpaired edges across two orders of magnitude of quantization scale.

### Cohort grid sensitivity (Gate 5)

`spotlight_f0044` runs F0044/F0045/R0092 in sequence; one `check_watertight_mesh` call per case → 3 TSV invocations:

| Inv | Case (inferred) | total_edges | 05x | 1x | 2x | 4x | 10x | 100x | dist1 | dist2 | isolated | oracle |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | F0044 | 180 | 12 | **12** | 12 | 12 | 12 | 12 | 0 | 0 | 12 | 12 |
| 1 | F0045 | 472 | 38 | **38** | 38 | 38 | 38 | 38 | 0 | 0 | 38 | 38 |
| 2 | R0092 | 280 | 43 | **43** | 43 | 43 | 43 | **45** | 0 | 0 | 43 | 43 |

`spotlight_r0045`:

| Inv | Case | total_edges | 05x | 1x | 2x | 4x | 10x | 100x | dist1 | dist2 | isolated | oracle |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | R0045 | 950 | 88 | **88** | 88 | 88 | 88 | 88 | 0 | 0 | 88 | 88 |

**R0092 100× anomaly.** R0092's count goes UP from 43 to 45 at the 100× grid, while every other column and case stays flat. This is **over-merging**: at `grid = max_abs * 1e-3`, originally-distinct vertices collapse to the same i64 cell, and 2 edges that were each paired (count=2) now become count=1 each (or count=3+ at one cell). The mechanism is opposite to phantom-edge recovery and *confirms* that 1× sits on the correct side of the grid-tradeoff curve: looser grids over-merge before they recover any phantoms. F0020/F0044/F0045/R0045 don't show this at 100× — likely because their geometry has more headroom between near-pairs at that scale.

### Cross-case attribution

| Case | 1× unpaired | Phantom % (1×→4×) | Phantom % (1×→100×) | Isolated % |
|---|---|---|---|---|
| F0020 | 40 | 0% | 0% | **100%** |
| F0044 | 12 | 0% | 0% | **100%** |
| F0045 | 38 | 0% | 0% | **100%** |
| R0092 | 43 | 0% | -4.7% (count UP) | **100%** |
| R0045 | 88 | 0% | 0% | **100%** |

The grid-sensitivity hypothesis (40 is partly phantom from f32 round-trip) is **fully refuted** for F0020 and cohort-wide. The `isolated = total` result across all 5 cases means **the watertight oracle's 1e-5 relative quantization is not pulling in spurious unpaired edges from f32 noise**. Whatever defect produces these unpaired edges is at scales ≥1 i64-cell from any partner edge — it is real B-Rep / topology / mesh defect, not measurement artifact.

### Gate 8 determinism

3 reruns of F0020 with the probe, identical TSV row each time:

```
F0020_det1_inv0	188	40	40	40	40	40	40	0	0	40	40
F0020_det2_inv0	188	40	40	40	40	40	40	0	0	40	40
F0020_det3_inv0	188	40	40	40	40	40	40	0	0	40	40
```

No flap. The probe is deterministic. (HashMap iteration order is non-deterministic but irrelevant — the counts are aggregated, not ordered.)

---

## §4 PR-Y39 anchor recommendation

**Recommended anchor: refine the source-face probe at the H3-dominant cluster, per PR-Y37's banked Options 1 and 2.**

### Why this is the right shape

PR-Y38 has empirically eliminated one hypothesis that, if true, would have invalidated PR-Y36/Y37's attribution numbers: *"the 40-unpaired count is partly inflated by f32 round-trip noise + 1e-5 quantization."* It isn't. F0020's 40 are isolated and stable; cohort's 12/38/43/88 are isolated and stable.

This means:

1. PR-Y37's H3 cluster (19/40 F0020 = 48.7%, 100% of F0044/F0045's unpaired, 72.1% of R0092's) is **real geometric defect** — not an oracle artifact.
2. PR-Y37's banked options 1 (sub-quantization geometric features per D.2) and 2 (per-segment NMM-incidence map per D.3) for refining H1/H2 detection remain the right scaffolding.
3. The "OTHER cluster is genuinely novel and reframe around partial-NMM kept-face mechanism" path (PR-Y36 §3.4 banked) is also still viable.

PR-Y39 can proceed without re-examining the oracle. The 40 number is the right target.

### What PR-Y39 should NOT do

- **Do not tune `TAU_TESS_GRID_FACTOR`.** The grid is fine. Tightening it (0.5×) doesn't add edges; loosening it doesn't drop edges; loosening it 100× starts to over-merge (R0092 anomaly). Status quo at 1e-5 is on the correct side of the curve.
- **Do not adopt position-tolerance edge-pairing.** The brief offered this as the "reframe" PR-Y39 candidate under PHANTOM verdict. Since verdict is corroboration, this is unnecessary; it would also weaken the watertight oracle's discrimination of real defects.
- **Do not claim this closes the F0020 investigation.** Per `feedback_no_last_bug`, the cluster has been investigated 7 canary cycles deep and still has residual H3 dominance. PR-Y39's anchor (refined source-face probe) is itself another investigational PR; the F0020 Render LOD residual is not committed-to-fix material yet.

### Edge cases to consider for PR-Y39

- **Wider ±2 scan.** If H3 mechanism turns out to involve vertices drifting 2+ i64-cells, the ±1 isolation result here could miss near-pairs. ±2 would add a `dist3+` bucket and ~10× more candidate checks per edge (125² = 15,625 vs ±1's 729). Banked for PR-Y39 if the source-face probe doesn't localize H3.
- **Position-tolerance edge matching.** The watertight oracle uses i64-cell key matching. An alternative is L∞-distance-based matching: pair edge A with edge B if max distance between endpoints (in either orientation) is < ε. This is more lenient but vulnerable to false positives near near-coincident vertices. PR-Y39 should NOT change to this without a separate canary measuring its corpus impact.

---

## §5 Verdict

**SHIP-INFRA + corroboration.**

- All 8 gates GREEN.
- Default-off byte parity verified (Gate 2, then re-verified after Gate 8).
- Probe is deterministic, env-gated, additive. +179 LOC in `crates/test-harness/src/oracle.rs`. 0 LOC of production logic touched.
- F0020 baseline of 40 unpaired edges is REAL geometric defect across {0.5×, 1×, 2×, 4×, 10×, 100×} grids and 100% isolated at ±1 i64-cell neighborhood. Cohort follows the same pattern.
- The phantom-from-f32-round-trip hypothesis is empirically refuted.
- PR-Y39 anchor confirmed: PR-Y37's banked Options 1/2 (refined source-face probe for H3 cluster discrimination). PR-Y38 has banked one infrastructure measurement (grid sensitivity) that future canaries no longer need to redo.

This is the 7th investigational PR on F0020 Render LOD without a fix shape. Per `feedback_phase1_diagnosis_ranking_is_inference` and `feedback_no_last_bug`, the memo does not claim "this closes Yang" or "phantom hypothesis is gone forever" — only that *under the current oracle's 1e-5 quantization and ±1 i64-cell scan*, the 40 are real. Future oracle redefinitions (position-tolerance matching, ±2 scan) are out of scope.

---

## §6 Empirical confidence assessment

| Claim | Confidence | Evidence |
|---|---|---|
| F0020 unpaired_count=40 is geometrically real, not phantom | **High** | 6-point grid sweep all 40; ±1 scan 100% isolated; deterministic across 3 reruns. Hypothesis-refuting result. |
| Cohort behaves the same way | **High** | F0044/F0045/R0092/R0045 all 100% isolated, all flat across grids. 4 independent cases × 6 grids = 24 independent measurements, all consistent. |
| Looser grid does not recover phantom pairs | **High** | R0092 actually goes UP at 100× — direct evidence of over-merging at extreme widening. Looser is worse, not better. |
| 40 is the right target count for PR-Y39's source-face probe | **High** | Follows from the first three claims. |
| H3 cluster (PR-Y37 dominant residual) is a real geometric mechanism | **Medium-high** | Indirect: PR-Y38 rules out oracle artifact for the count, but doesn't itself characterize the cluster. PR-Y37's H3 classification stands as the best descriptor pending Options 1/2 refinement. |
| ±1 scan is sufficient for the phantom question | **Medium-high** | f32 ULP at meter scale is ~1.2e-7, far below 1 cell at 1e-5 relative grid. Realistic round-trip drift sits inside ±1. ±2 banked if PR-Y39 needs it. |
| The brief's hypothesis was a reasonable one to canary | **High** | The oracle's f32→i64 quantization through TAU_TESS_GRID_FACTOR is the kind of measurement artifact that absolutely could have inflated counts. The fact that it didn't is itself useful: prior 6 canaries' attribution numbers stand. |

---

## §7 Reproduction artifacts

### TSV files (in worktree-local /tmp)

- `/tmp/y38-probe/Y38_inv0000_grid_sensitivity.tsv` — F0020
- `/tmp/y38-cohort/Y38_inv{0000,0001,0002}_grid_sensitivity.tsv` — F0044, F0045, R0092
- `/tmp/y38-r0045/Y38_inv0000_grid_sensitivity.tsv` — R0045
- `/tmp/y38-det{1,2,3}/Y38_inv0000_grid_sensitivity.tsv` — F0020 determinism reruns

### Code

- `crates/test-harness/src/oracle.rs:243-246` — probe entry point (env gate)
- `crates/test-harness/src/oracle.rs:283-308` — `y38_make_edge` + `Y38PosEdge` type alias + `Y38_INVOCATION_COUNTER` static
- `crates/test-harness/src/oracle.rs:311-338` — `y38_count_non_paired_at_multiplier`
- `crates/test-harness/src/oracle.rs:340-393` — `y38_near_pair_scan`
- `crates/test-harness/src/oracle.rs:395-451` — `y38_grid_sensitivity_probe` (writer)

### Env vars

- `Y38_GRID_PROBE=1` — gate (required)
- `Y38_GRID_PROBE_DIR=<path>` — output dir (required when gate is on)
- `Y38_PROBE_CASE_NAME=<label>` — optional label prefix for TSV `case` column

### Plan

- `/home/claude/.claude/plans/snappy-humming-hejlsberg.md` — PR-Y38 plan; Phase 2 brief at L52-107

### Adjacent canary memos

- `docs/audits/pr_y36_canary.md` — inverse-direction probe v1; D.1c=0% refutation
- `docs/audits/pr_y37_canary.md` — inverse-direction probe v2; H3 dominance discovered

---

**End of canary memo.**
