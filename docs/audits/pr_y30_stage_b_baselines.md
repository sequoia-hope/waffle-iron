# PR-Y30 — Cherchi 2022 differential diff baselines, Stage B (post-survival)

**Date:** 2026-05-08
**Harness:** `crates/test-harness/tests/cherchi_differential_diff.rs` (after Stage C → Stage B switch)
**Cherchi binary:** `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans`
**Quantization grid:** 1e-6 m (1 µm), winding-insensitive (vertex triple sorted)
**TBB threads:** `TBB_NUM_THREADS=1` (Cherchi serialized for determinism on this run)
**Cherchi output non-determinism:** F0020 (295 tris this run) and R0092 (405 tris this run) remain non-deterministic across runs even at `TBB_NUM_THREADS=1`; F0044 (88 tris) and F0045 (236 tris) are deterministic. Numbers below are from THIS run.

## Why Stage B instead of Stage C (the PR-Y30 calibration)

PR-Y29 used Waffle's Stage C dump (post-flood-fill patch-id from
`flood_fill_patches`) as the Waffle side of the diff. impl-y29 flagged
this as apples-to-oranges:

> "Waffle Stage C is the unified union of A∪B's surviving triangles, not
> the final boolean result. Cherchi's output IS the boolean result. The
> 146 'extra' in F0020 conflates (i) Waffle keeping triangles Cherchi
> rejects via survival and (ii) Waffle keeping triangles Cherchi doesn't
> have at all."

Cherchi 2022 §3 + §5 describe the reference's two-step pipeline:
*mesh arrangement* (producing a well-formed simplicial complex) followed
by *in/out classification* (selecting which patches survive per the
boolean operator). `mesh_booleans union` emits the **post-classification**
result.

Yang 2025 §4.4.2 ("Mesh and B-Rep Booleans") describes the equivalent
step on the Waffle side: after applying inside/outside classification per
Cherchi 2022, "directly apply a standard inside/outside classification
step [Cherchi et al. 2022] to identify the triangles that need to be
retained, thus completing the mesh Boolean operation." On the Waffle
side this is exactly `face_survival_detect`, whose output is dumped at
`topology_extract.rs:2569` under the tag **Stage B**.

Stage C is post-flood-fill patch labeling (downstream of Stage B by ~430
lines in `topology_extract.rs`); it filters Stage B triangles via patch
identification but is one structural step further than Cherchi's output.

Stage B is therefore the correct comparison anchor for differential
diff against the Cherchi reference. PR-Y30 is the 12-LOC calibration
fix; PR-Y31+ will use these recalibrated baselines as the load-bearing
oracle for actual mechanism fixes.

## Stage C → Stage B comparison (PR-Y29 vs PR-Y30)

| Case  | Stage C common | Stage C missing | Stage C extras | Stage B common | Stage B missing | Stage B extras |
|-------|---------------:|----------------:|---------------:|---------------:|----------------:|---------------:|
| F0020 | 140            | 97              | 146            | **185**        | **93**          | **107**        |
| F0044 | 88             | 0               | 48             | 88             | 0               | **48**         |
| F0045 | 0              | 236             | 458            | 0              | 236             | 466            |
| R0092 | 0              | 140             | 368            | 0              | **340**         | 368            |

Bold cells highlight changes that diverge from the Stage C baseline.

## Hypothesis verification

Plan agent's predictions (from `/home/claude/.claude/plans/optimized-wandering-wind.md`):

- **F0044 hypothesis (PRIMARY):** extras drop from 48 → ~0-10.
  **Result:** extras stay at **48**. **REFUTED.**
- **F0020 hypothesis:** ~80-100 extras (was 146 at Stage C).
  **Result:** extras = 107. **PARTIALLY CONFIRMED** (right-ballpark; just over the predicted ceiling).
- **F0045/R0092 hypothesis:** "still 0 common (tessellation-grid divergence is pre-survival, unchanged)."
  **Result:** F0045 still 0 common; R0092 still 0 common. **CONFIRMED.**

### F0044 hypothesis refutation — what it means

The Plan agent's prediction was: the 48 "extras" reported at Stage C are
patches Waffle's flood-fill emits that Cherchi's survival rejects;
switching to Stage B should drop them to near-zero because both pipelines
emit the same post-survival result.

The data refutes this: F0044's Stage B Waffle output has **the same 48
extras**, and the same 88 common, as Stage C. This means:

1. F0044's 48 extra-in-Waffle triangles are NOT introduced between
   `face_survival_detect` (Stage B) and `flood_fill_patches` (Stage C) —
   they exist already at Stage B.
2. F0044's `face_survival_detect` is producing 48 extra triangles that
   Cherchi's `select_boolean_result` does NOT produce. The divergence
   is **inside or upstream of** `face_survival_detect`, not in flood-fill
   downstream.

This is a load-bearing banked finding for PR-Y31+. The next-anchor
should be either (a) the survival classification logic itself (`label_cells`
inside/outside decision plus the op-specific keep-mask) or (b) the
mesh arrangement subdivision step that feeds it — NOT the patch-ID
flood-fill which sits one step downstream.

The Plan agent's prediction encoded an assumption that Cherchi and
Waffle agree at the post-survival boundary and disagree only at
flood-fill. The corpus has just shown that assumption is wrong on F0044
— the disagreement starts at or above survival.

### F0020 ~28% extras reduction at Stage B

F0020's Stage B extras dropped from 146 → 107 (–39, –27%), and common
jumped from 140 → 185 (+45, +32%). This is meaningful progress and
confirms Stage B IS strictly closer to Cherchi's output than Stage C
for F0020. But the headline number — 107 extra-in-Waffle triangles
that Cherchi does not emit — is still large. The dominant geometric
signature (top-10 extras cluster at corner `(-0.352714, +0.085762,
+0.195664)` and `(-0.317799, +0.091798, -0.246218)`) is unchanged
from the Stage C baseline — same boundary feature, same fan-divergence.
This signals a survival-classification or upstream-mesh-arrangement
divergence at a single B-Rep face boundary.

### F0045 unchanged — tessellation-grid divergence is structural

F0045's 0 common, 236 missing, 466 extras (Stage B) matches the Stage C
profile exactly (0 / 236 / 458). The Cherchi side is unchanged
(`mesh_booleans union` is the same); the Waffle Stage B output gained 8
extras (+2%) over Stage C. Both pipelines produce mutually-disjoint
triangle sets at the 1µm grid — neither's intersection points round to
the other's. This is consistent with Yang 2025 §4.1.1's error-bounded
discretization producing a different surface tessellation than Cherchi's
input-as-given. F0045 will not be fixable by survival-rule adjustments
alone; it needs the Yang 2025 §4.3 intersection-optimization step to
produce surface-exact intersection vertices matching Cherchi's.

### R0092 — Cherchi non-determinism dominates

R0092's missing grew from 140 (PR-Y29 run, Cherchi emitted 153 tris) to
340 (PR-Y30 run, Cherchi emitted 405 tris). Both are extremes the
PR-Y29 memo flagged ("R0092 saw 153 / 405 (two consecutive runs)"). The
Cherchi side is unstable on this case even at `TBB_NUM_THREADS=1`;
the diff numbers swing with which Cherchi output happens to land. The
Waffle side stayed at 368 / 0 / 368, byte-identical to PR-Y29's
Stage C numbers for tri/vert/common. So R0092 also shows no Waffle-side
behavior change from Stage C to Stage B (which makes sense; flood-fill
adds patch-IDs, not triangles).

Confirms what PR-Y29 already established: R0092 is best treated as a
case where the *Cherchi reference itself* is unstable; mean-of-N
sampling or another reference (mesh arrangement only, no survival) may
be the only useful comparison.

## Verbatim diff blocks

### F0020 baseline diff (Stage B)

```
=== F0020 diff ===
Cherchi output: 295 triangles, 120 vertices, well_formed=false, χ=5
Waffle output:  294 triangles, 117 vertices, well_formed=false, χ=1
Triangle count delta: N_c - N_w = 1

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 93 triangles
  In Waffle, not in Cherchi: 107 triangles
  Common (matching quantized positions): 185

Top 10 missing-from-Waffle triangles (positions):
  tri[0] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qc=(-2.749190e-1,+9.921200e-2,-1.416830e-1)
  tri[1] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-2.749190e-1,+9.921200e-2,-1.416830e-1) qc=(-1.421790e-1,+1.221610e-1,+7.010300e-2)
  tri[2] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-2.062750e-1,+1.110790e-1,+1.148260e-1) qc=(-1.421790e-1,+1.221610e-1,+7.010300e-2)
  tri[3] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-2.062750e-1,+1.110790e-1,+1.148260e-1) qc=(+2.685320e-1,+1.931670e-1,+2.462180e-1)
  tri[4] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-2.749190e-1,+9.921200e-2,-1.570730e-1) qc=(-2.749190e-1,+9.921200e-2,-1.416830e-1)
  tri[5] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-2.749190e-1,+9.921200e-2,-1.570730e-1) qc=(-2.487970e-1,+1.037280e-1,-2.076910e-1)
  tri[6] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-2.487970e-1,+1.037280e-1,-2.076910e-1) qc=(-2.402670e-1,+1.052020e-1,-2.229470e-1)
  tri[7] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-2.404770e-1,+1.051660e-1,-2.260490e-1) qc=(-2.402670e-1,+1.052020e-1,-2.229470e-1)
  tri[8] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-2.404770e-1,+1.051660e-1,-2.260490e-1) qc=(-3.629900e-2,+1.404660e-1,-2.087470e-1)
  tri[9] = qa=(-2.749190e-1,+9.921200e-2,-2.102050e-1) qb=(-2.749190e-1,+9.921200e-2,-1.570730e-1) qc=(-2.749190e-1,+9.921200e-2,-1.416830e-1)

Top 10 extra-in-Waffle triangles (positions):
  tri[0] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qc=(-1.421790e-1,+1.221610e-1,+7.010300e-2)
  tri[1] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-1.421790e-1,+1.221610e-1,+7.010300e-2) qc=(-1.421790e-1,+1.221610e-1,+1.208200e-1)
  tri[2] = qa=(-3.527140e-1,+8.576200e-2,+1.956640e-1) qb=(-1.421790e-1,+1.221610e-1,+1.208200e-1) qc=(+2.685320e-1,+1.931670e-1,+2.462180e-1)
  tri[3] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-1.421790e-1,+1.221610e-1,-1.925890e-1) qc=(-1.421790e-1,+1.221610e-1,-8.902000e-3)
  tri[4] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-1.421790e-1,+1.221610e-1,-1.925890e-1) qc=(-3.629900e-2,+1.404660e-1,-2.087470e-1)
  tri[5] = qa=(-3.177990e-1,+9.179800e-2,-2.462180e-1) qb=(-1.421790e-1,+1.221610e-1,-8.902000e-3) qc=(-1.421790e-1,+1.221610e-1,+7.010300e-2)
  tri[6] = qa=(-2.749190e-1,+9.921200e-2,-2.102050e-1) qb=(-2.749190e-1,+9.921200e-2,+5.152000e-2) qc=(-2.749190e-1,+9.921200e-2,+1.052630e-1)
  tri[7] = qa=(-2.749190e-1,+9.921200e-2,-2.102050e-1) qb=(-2.749190e-1,+9.921200e-2,+1.052630e-1) qc=(-2.471870e-1,+1.040060e-1,-2.269840e-1)
  tri[8] = qa=(-2.749190e-1,+9.921200e-2,+1.052630e-1) qb=(-2.471870e-1,+1.040060e-1,-2.269840e-1) qc=(-1.421790e-1,+1.221610e-1,+7.622600e-2)
  tri[9] = qa=(-2.749190e-1,+9.921200e-2,+1.052630e-1) qb=(-7.855700e-2,+1.331600e-1,+1.326190e-1) qc=(+2.308200e-2,+1.507320e-1,+1.467790e-1)
=== end F0020 diff ===
```

### F0044 baseline diff (Stage B)

```
=== F0044 diff ===
Cherchi output: 88 triangles, 46 vertices, well_formed=true, χ=2
Waffle output:  136 triangles, 72 vertices, well_formed=true, χ=4
Triangle count delta: N_c - N_w = -48

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 0 triangles
  In Waffle, not in Cherchi: 48 triangles
  Common (matching quantized positions): 88

Top 0 missing-from-Waffle triangles (positions):

Top 10 extra-in-Waffle triangles (positions):
  tri[0] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qc=(+5.379900e-2,-2.905770e-1,+6.454500e-2)
  tri[1] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qc=(+5.379900e-2,-1.595630e-1,+3.548190e-1)
  tri[2] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qc=(+1.672600e-1,-2.250700e-1,+6.454500e-2)
  tri[3] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+5.379900e-2,-1.595630e-1,+6.454500e-2) qc=(+5.379900e-2,-1.595630e-1,+3.548190e-1)
  tri[4] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+5.379900e-2,-1.595630e-1,+6.454500e-2) qc=(+1.672600e-1,-2.250700e-1,+6.454500e-2)
  tri[5] = qa=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qb=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qc=(+5.379900e-2,-2.905770e-1,+3.548190e-1)
  tri[6] = qa=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qb=(+5.379900e-2,-2.905770e-1,+3.548190e-1) qc=(+1.672600e-1,-2.250700e-1,+3.548190e-1)
  tri[7] = qa=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qb=(+5.379900e-2,-1.595630e-1,+3.548190e-1) qc=(+1.672600e-1,-2.250700e-1,+3.548190e-1)
  tri[8] = qa=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qb=(+5.379900e-2,-2.905770e-1,+3.548190e-1) qc=(+1.017530e-1,-3.385310e-1,+6.454500e-2)
  tri[9] = qa=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qb=(+1.017530e-1,-3.385310e-1,+6.454500e-2) qc=(+1.672600e-1,-2.250700e-1,+6.454500e-2)
=== end F0044 diff ===
```

### F0045 baseline diff (Stage B)

```
=== F0045 diff ===
Cherchi output: 236 triangles, 120 vertices, well_formed=true, χ=2
Waffle output:  468 triangles, 274 vertices, well_formed=false, χ=9
Triangle count delta: N_c - N_w = -232

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 236 triangles
  In Waffle, not in Cherchi: 466 triangles
  Common (matching quantized positions): 0

Top 10 missing-from-Waffle triangles (positions):
  tri[0] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qc=(-4.054730e-1,+9.254700e-2,+2.483780e-1)
  tri[1] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qc=(-3.871510e-1,-1.519460e-1,+0.000000e0)
  tri[2] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.054730e-1,+9.254700e-2,+0.000000e0) qc=(-4.054730e-1,+9.254700e-2,+2.483780e-1)
  tri[3] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.054730e-1,+9.254700e-2,+0.000000e0) qc=(+0.000000e0,+0.000000e0,+0.000000e0)
  tri[4] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-3.871510e-1,-1.519460e-1,+0.000000e0) qc=(+0.000000e0,+0.000000e0,+0.000000e0)
  tri[5] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-4.054730e-1,+9.254700e-2,+2.483780e-1) qc=(-1.032200e-1,+2.355900e-2,+2.483780e-1)
  tri[6] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-3.871510e-1,-1.519460e-1,+0.000000e0) qc=(-3.871510e-1,-1.519460e-1,+2.483780e-1)
  tri[7] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-3.871510e-1,-1.519460e-1,+2.483780e-1) qc=(-1.237730e-1,-9.275000e-3,+2.483780e-1)
  tri[8] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-1.237730e-1,-9.275000e-3,+2.483780e-1) qc=(-1.109160e-1,+1.654400e-2,+2.483780e-1)
  tri[9] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-1.109160e-1,+1.654400e-2,+2.483780e-1) qc=(-1.032200e-1,+2.355900e-2,+2.483780e-1)

Top 10 extra-in-Waffle triangles (positions):
  tri[0] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qc=(-4.147380e-1,+3.108000e-2,+2.483780e-1)
  tri[1] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qc=(-4.054730e-1,-9.254700e-2,+0.000000e0)
  tri[2] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.147380e-1,+3.108000e-2,+0.000000e0) qc=(-4.147380e-1,+3.108000e-2,+2.483780e-1)
  tri[3] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.147380e-1,+3.108000e-2,+0.000000e0) qc=(+0.000000e0,+0.000000e0,+0.000000e0)
  tri[4] = qa=(-4.147380e-1,-3.108000e-2,+0.000000e0) qb=(-4.054730e-1,-9.254700e-2,+0.000000e0) qc=(+0.000000e0,+0.000000e0,+0.000000e0)
  tri[5] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-4.147380e-1,+3.108000e-2,+2.483780e-1) qc=(-2.102610e-1,+6.618000e-3,+2.802450e-1)
  tri[6] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-4.054730e-1,-9.254700e-2,+0.000000e0) qc=(-4.054730e-1,-9.254700e-2,+2.483780e-1)
  tri[7] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-4.054730e-1,-9.254700e-2,+2.483780e-1) qc=(-2.084090e-1,-1.810800e-2,+2.802450e-1)
  tri[8] = qa=(-4.147380e-1,-3.108000e-2,+2.483780e-1) qb=(-2.102610e-1,+6.618000e-3,+2.802450e-1) qc=(-2.084090e-1,-1.810800e-2,+2.802450e-1)
  tri[9] = qa=(-4.147380e-1,+3.108000e-2,+0.000000e0) qb=(-4.147380e-1,+3.108000e-2,+2.483780e-1) qc=(-4.054730e-1,+9.254700e-2,+2.483780e-1)
=== end F0045 diff ===
```

### R0092 baseline diff (Stage B)

```
=== R0092 diff ===
Cherchi output: 405 triangles, 187 vertices, well_formed=false, χ=112
Waffle output:  368 triangles, 303 vertices, well_formed=false, χ=7
Triangle count delta: N_c - N_w = 37

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 340 triangles
  In Waffle, not in Cherchi: 368 triangles
  Common (matching quantized positions): 0

Top 10 missing-from-Waffle triangles (positions):
  tri[0] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+1.730000e-4,+2.430000e-4,-6.349000e-3) qc=(+8.950000e-4,+1.334000e-3,-7.589000e-3)
  tri[1] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+1.730000e-4,+2.430000e-4,-6.349000e-3) qc=(+9.000000e-4,-5.651000e-3,-8.648000e-3)
  tri[2] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+5.500000e-4,-1.882000e-3,-3.528000e-3) qc=(+1.397000e-3,-6.658000e-3,-7.238000e-3)
  tri[3] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+5.500000e-4,-1.882000e-3,-3.528000e-3) qc=(+1.603000e-3,-2.659000e-3,-2.287000e-3)
  tri[4] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+8.950000e-4,+1.334000e-3,-7.589000e-3) qc=(+1.173000e-3,-6.760000e-4,-4.938000e-3)
  tri[5] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+9.000000e-4,-5.651000e-3,-8.648000e-3) qc=(+1.397000e-3,-6.658000e-3,-7.238000e-3)
  tri[6] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qc=(+1.173000e-3,-6.760000e-4,-4.938000e-3)
  tri[7] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qc=(+1.228000e-3,-7.780000e-4,-4.794000e-3)
  tri[8] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+1.228000e-3,-7.780000e-4,-4.794000e-3) qc=(+1.642000e-3,-1.550000e-3,-3.706000e-3)
  tri[9] = qa=(+5.300000e-5,-8.740000e-4,-4.938000e-3) qb=(+1.603000e-3,-2.659000e-3,-2.287000e-3) qc=(+2.647000e-3,-2.183000e-3,-2.661000e-3)

Top 10 extra-in-Waffle triangles (positions):
  tri[0] = qa=(+7.900000e-4,-5.660000e-4,-5.167000e-3) qb=(+7.970000e-4,-3.330000e-4,-5.464000e-3) qc=(+1.150000e-3,-4.270000e-4,-5.264000e-3)
  tri[1] = qa=(+7.900000e-4,-5.660000e-4,-5.167000e-3) qb=(+7.970000e-4,-3.330000e-4,-5.464000e-3) qc=(+1.364000e-3,+1.000000e-6,-5.766000e-3)
  tri[2] = qa=(+7.900000e-4,-5.660000e-4,-5.167000e-3) qb=(+1.150000e-3,-4.270000e-4,-5.264000e-3) qc=(+1.195000e-3,-6.790000e-4,-4.929000e-3)
  tri[3] = qa=(+7.900000e-4,-5.660000e-4,-5.167000e-3) qb=(+1.195000e-3,-6.790000e-4,-4.929000e-3) qc=(+1.627000e-3,-5.970000e-4,-4.936000e-3)
  tri[4] = qa=(+7.900000e-4,-5.660000e-4,-5.167000e-3) qb=(+1.364000e-3,+1.000000e-6,-5.766000e-3) qc=(+1.627000e-3,-5.970000e-4,-4.936000e-3)
  tri[5] = qa=(+7.970000e-4,-3.330000e-4,-5.464000e-3) qb=(+1.150000e-3,-4.270000e-4,-5.264000e-3) qc=(+1.218000e-3,+1.800000e-5,-5.821000e-3)
  tri[6] = qa=(+7.970000e-4,-3.330000e-4,-5.464000e-3) qb=(+1.218000e-3,+1.800000e-5,-5.821000e-3) qc=(+1.364000e-3,+1.000000e-6,-5.766000e-3)
  tri[7] = qa=(+1.150000e-3,-4.270000e-4,-5.264000e-3) qb=(+1.195000e-3,-6.790000e-4,-4.929000e-3) qc=(+5.165000e-3,+3.200000e-5,-4.938000e-3)
  tri[8] = qa=(+1.150000e-3,-4.270000e-4,-5.264000e-3) qb=(+1.218000e-3,+1.800000e-5,-5.821000e-3) qc=(+5.165000e-3,+3.200000e-5,-4.938000e-3)
  tri[9] = qa=(+1.195000e-3,-6.790000e-4,-4.929000e-3) qb=(+1.627000e-3,-5.970000e-4,-4.936000e-3) qc=(+5.165000e-3,+3.200000e-5,-4.938000e-3)
=== end R0092 diff ===
```

## Banked findings for PR-Y31+

1. **F0044 hypothesis refuted — the divergence is at or above
   `face_survival_detect`, not downstream in `flood_fill_patches`.**
   The 48 extras at Stage B are the same 48 at Stage C, and the
   geometric pattern (top-10) is identical. PR-Y31's mechanism
   investigation should target the survival/classification logic and
   the mesh-arrangement step that feeds it. Specifically:
   - `face_survival_detect` (`topology_extract.rs` near 2554) and its
     in/out keep-mask per `MeshBooleanOp::Union`.
   - `label_cells` (Yang 2025 §4.4 inside/outside step adapting
     Cherchi 2022 §5).
   - The mesh-arrangement subdivision in `subdivide_mesh_pair` if
     Cherchi's tessellation has fewer subdivisions and emits 88 tris
     where Waffle emits 136 — the Waffle side has 26 more vertices
     (72 vs 46) and 48 more tris, suggesting Waffle subdivides 24
     more triangles than Cherchi for the same boolean.

2. **F0020 fan-divergence pattern is unchanged** (top-10 missing/extras
   identical between Stage C and Stage B baselines). Even with the
   28% improvement in extras count, the same boundary feature
   (`qa=(-0.352714, +0.085762, +0.195664)` and adjacent corner) drives
   the missing triangles. This points at the same single surviving-face
   anchor, not a globally-distributed bug.

3. **F0045 confirmed unfixable by survival-rule alone** — 0 common at
   1µm grid means Cherchi and Waffle produce mutually-disjoint vertex
   positions in the boolean region. The fix shape needs Yang §4.3
   intersection optimization (surface-exact mid-points) or to feed
   Cherchi-canonicalized intersection vertices to the Waffle survival
   step.

4. **R0092 is a Cherchi-reference-instability case.** Reference
   non-determinism (153 / 295 / 405 tri counts across runs) makes a
   single-run diff unreliable. Recommend mean-of-N over 5+ runs OR
   treat R0092 as a separate sub-cohort where the load-bearing oracle
   is internal (e.g., conformal closure) rather than reference-diff.

5. **Cherchi non-determinism persists at `TBB_NUM_THREADS=1`** (this run)
   for F0020 (295 tris) and R0092 (405 tris), refuting the PR-Y29
   memo's hypothesis that single-thread mode would stabilize output.
   F0044 (88 tris) and F0045 (236 tris) remain deterministic. The
   cause is likely Cherchi's parallel arrangement step that still
   uses multiple threads at lower levels even when TBB is constrained
   — investigation banked for a downstream PR if reference parity for
   R0092 becomes load-bearing.

## Use forward to PR-Y31+

The Stage B baselines replace the Stage C baselines as the load-bearing
diff oracle. PR-Y31 spec should anchor on **F0044 as the primary case**
— it is the cleanest signal (88 common, 0 missing, 48 extras, both sides
well-formed and χ-correct) and the hypothesis refutation gives a
specific direction (survival/arrangement, not flood-fill). F0020 stays
as cohort sibling with 185 common providing finer-grained reference.
F0045 and R0092 should be deprioritized until the F0044/F0020 mechanism
is closed; both have known structural reasons (tessellation-grid
divergence + Cherchi non-determinism) for being out of scope of a
single fix shape.
