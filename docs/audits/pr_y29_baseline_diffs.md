# PR-Y29 — Cherchi 2022 differential diff baselines (F0020 + cohort)

**Date:** 2026-05-08
**Harness:** `crates/test-harness/tests/cherchi_differential_diff.rs`
**Cherchi binary:** `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans` (built sub-phase 0a)
**Quantization grid:** 1e-6 m (1 µm), winding-insensitive (vertex triple sorted)

## How to read this memo

Each baseline section contains the verbatim eprintln output of one
`run_diff_for_case` invocation. Both pipelines see the same preprocessed A
and B input meshes (Waffle's `YANG_DUMP_OBJ_BASE` write site at
`yang_integration.rs:802` — the input to `subdivide_mesh_pair`). The diff
compares:

- **Cherchi output** — `mesh_booleans union <A> <B> <out>` final OBJ.
- **Waffle output** — Stage-C `[conformal-probe]` dump
  (`topology_extract.rs:1005`); this is the unified
  post-`subdivide_mesh_pair` triangle soup — the same boundary Cherchi's
  output represents.

A **non-zero "In Cherchi, not in Waffle"** count identifies triangles
that Cherchi emits in a paper-faithful reference implementation but our
port does not. PR-Y30+ canaries can sample these triangles, work back to
the source mesh they should have come from, and trace which Waffle stage
dropped them.

A **non-zero "In Waffle, not in Cherchi"** count identifies triangles
our port produces that Cherchi does not — either extra subdivisions,
inverted survival decisions, or LPI/CDT divergence.

**Note on `well_formed` semantics:** `check_conformal::is_well_formed`
asserts edge-pairing manifoldness on the *combined* triangle soup
(Waffle Stage C is union of both solids' surviving triangles, not the
final boolean result). On healthy cases like F0044, both Cherchi and
Waffle report `well_formed=true` because the union mesh is closed and
manifold. On F0020 / F0045 / R0092, neither side is well-formed against
the Stage-C representation — these cases share an underlying
LPI/CDT-divergence signature that PR-Y30+ will localize.

**Reproducibility:** The harness side (Waffle Stage C output, OBJ
parsing, quantization, set diff, top-N sort) is byte-deterministic
across runs. Waffle's `stage_C.obj` MD5 stayed at `1df8535b87b7…`
across 3 consecutive runs of `f0020_cherchi_diff_baseline`. **The
Cherchi binary itself is NOT deterministic on F0020 / R0092**: across
the same 3 runs F0020 Cherchi-output triangle counts were 253 / 246 /
295; R0092 saw 153 / 405 (two consecutive runs). The cohort F0044 and
F0045 outputs ARE deterministic across runs (88 / 236 triangles
respectively in every run observed). This is a property of Cherchi
2022's parallel arrangement (Cherchi 2022 §6 — TBB parallel
tessellation) on inputs that have many independent intersection
clusters; the order in which the TBB worker pool resolves them is
schedule-dependent and feeds back into the canonicalization order.
The numbers in this memo are from the **first observed run**; PR-Y30+
canaries should run Cherchi 2-3 times and take the median, or set
`OMP_NUM_THREADS=1`/`TBB_NUM_THREADS=1` to force serial execution
(experiment to confirm).

---

## F0020 baseline diff

```
=== F0020 diff ===
Cherchi output: 253 triangles, 120 vertices, well_formed=false, χ=7
Waffle output:  288 triangles, 117 vertices, well_formed=false, χ=-3
Triangle count delta: N_c - N_w = -35

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 97 triangles
  In Waffle, not in Cherchi: 146 triangles
  Common (matching quantized positions): 140

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

**Observations (informational, not load-bearing):**
- F0020 has 140 common triangles (~55% overlap), confirming both
  pipelines agree on the bulk of the union — divergence is localized,
  not pervasive. Either pipeline is `well_formed=false` against the
  Stage-C union-soup oracle.
- 97 missing-from-Waffle triangles cluster around the vertex
  `qa=(-0.352714, +0.085762, +0.195664)` (8 of top 10 share this corner)
  — a single boundary feature is where Cherchi emits an additional fan
  Waffle is dropping. PR-Y30+ should trace this vertex back to its
  source face on operand A or B.

---

## F0044 baseline diff

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
  tri[3] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qc=(+5.379900e-2,-1.595630e-1,+3.548190e-1)
  tri[4] = qa=(+3.624600e-2,-2.250700e-1,+6.454500e-2) qb=(+5.379900e-2,-1.595630e-1,+6.454500e-2) qc=(+1.672600e-1,-2.250700e-1,+6.454500e-2)
  tri[5] = qa=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qb=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qc=(+5.379900e-2,-2.905770e-1,+3.548190e-1)
  tri[6] = qa=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qb=(+5.379900e-2,-2.905770e-1,+3.548190e-1) qc=(+1.672600e-1,-2.250700e-1,+3.548190e-1)
  tri[7] = qa=(+3.624600e-2,-2.250700e-1,+3.548190e-1) qb=(+5.379900e-2,-1.595630e-1,+3.548190e-1) qc=(+1.672600e-1,-2.250700e-1,+3.548190e-1)
  tri[8] = qa=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qb=(+5.379900e-2,-2.905770e-1,+3.548190e-1) qc=(+1.017530e-1,-3.385310e-1,+6.454500e-2)
  tri[9] = qa=(+5.379900e-2,-2.905770e-1,+6.454500e-2) qb=(+1.017530e-1,-3.385310e-1,+6.454500e-2) qc=(+1.672600e-1,-2.250700e-1,+6.454500e-2)
=== end F0044 diff ===
```

**Observations:**
- F0044 is the cleanest baseline in the cohort: Cherchi χ=2 (one closed
  shell, well_formed=true); Waffle's Stage C is also well_formed=true
  but reports χ=4 (two shells — likely the A+B *union-soup* counted
  separately, not the boolean result).
- ZERO triangles missing from Waffle — Cherchi's full output is a strict
  subset of Waffle's Stage C. Waffle's 48 "extra" triangles are
  candidates for survival-rule divergence (Waffle keeps tris Cherchi
  rejects) or extra subdivision.

---

## F0045 baseline diff

```
=== F0045 diff ===
Cherchi output: 236 triangles, 120 vertices, well_formed=true, χ=2
Waffle output:  460 triangles, 274 vertices, well_formed=false, χ=4
Triangle count delta: N_c - N_w = -224

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 236 triangles
  In Waffle, not in Cherchi: 458 triangles
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
```
(Top-10 extra-in-Waffle elided here; full text in test-run log.)

**Observations:**
- F0045 is the worst-overlap case: **0 common triangles**, 236+458 disjoint sets.
  This is consistent with a tessellation-resolution / intersection-point
  divergence — both pipelines tessellate the surface near the boolean
  boundary, but the produced vertex positions differ enough to never
  quantize-match at 1e-6 m. Cherchi is well_formed=true, χ=2 (correct);
  Waffle is well_formed=false, χ=4.

---

## R0092 baseline diff

```
=== R0092 diff ===
Cherchi output: 153 triangles, 139 vertices, well_formed=false, χ=32
Waffle output:  368 triangles, 303 vertices, well_formed=false, χ=7
Triangle count delta: N_c - N_w = -215

Position-quantized triangle set comparison (grid=1e-6 m, winding-insensitive):
  In Cherchi, not in Waffle: 140 triangles
  In Waffle, not in Cherchi: 368 triangles
  Common (matching quantized positions): 0

Top 10 missing-from-Waffle triangles (positions):
  tri[0] = qa=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qb=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qc=(+1.173000e-3,-6.760000e-4,-4.938000e-3)
  tri[1] = qa=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qb=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qc=(+1.190000e-3,-5.570000e-4,-5.087000e-3)
  tri[2] = qa=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qb=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qc=(+1.228000e-3,-7.780000e-4,-4.794000e-3)
  tri[3] = qa=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qb=(+1.190000e-3,-5.570000e-4,-5.087000e-3) qc=(+1.190000e-3,-5.570000e-4,-5.087000e-3)
  tri[4] = qa=(+1.173000e-3,-6.760000e-4,-4.938000e-3) qb=(+1.190000e-3,-5.570000e-4,-5.087000e-3) qc=(+1.228000e-3,-7.780000e-4,-4.794000e-3)
  tri[5] = qa=(+1.190000e-3,-5.570000e-4,-5.087000e-3) qb=(+1.228000e-3,-7.780000e-4,-4.794000e-3) qc=(+1.228000e-3,-7.780000e-4,-4.794000e-3)
  tri[6] = qa=(+1.824000e-3,+9.690000e-4,-6.907000e-3) qb=(+1.824000e-3,+9.690000e-4,-6.907000e-3) qc=(+2.038000e-3,+1.247000e-3,-7.215000e-3)
  tri[7] = qa=(+1.824000e-3,+9.690000e-4,-6.907000e-3) qb=(+2.038000e-3,+1.247000e-3,-7.215000e-3) qc=(+2.038000e-3,+1.247000e-3,-7.215000e-3)
  tri[8] = qa=(+1.824000e-3,+9.690000e-4,-6.907000e-3) qb=(+2.038000e-3,+1.247000e-3,-7.215000e-3) qc=(+2.658000e-3,+1.636000e-3,-7.575000e-3)
  tri[9] = qa=(+2.020000e-3,-5.452000e-3,-8.648000e-3) qb=(+2.020000e-3,-5.452000e-3,-8.648000e-3) qc=(+2.292000e-3,-6.982000e-3,-9.837000e-3)
```
(Top-10 extra-in-Waffle elided here; full text in test-run log.)

**Observations:**
- R0092 features sub-millimeter-scale geometry (positions ~1e-3 m) and
  Cherchi itself reports χ=32 (well_formed=false). The top-10
  missing-from-Waffle list shows multiple degenerate triangles (three
  identical vertices like tri[0]) which Cherchi emitted but Waffle's
  pipeline either rejected or never produced. The 0 common triangles
  reflect intersection-point-position divergence at the 1µm grid given
  the small feature size.

---

## Use of this baseline (forward to PR-Y30+)

These baselines are the input for PR-Y30+ canaries. **The fix shape
that closes F0020 watertight should ALSO close (or significantly
reduce) the diff against Cherchi.** A canary that proposes a fix should:

1. Run the harness on F0020 + cohort with the candidate fix applied.
2. Compare the new diff against this baseline — net reduction in
   `(missing + extra)` is a positive signal; an increase is a refutation
   regardless of whether the watertight oracle changes.
3. Use the top-N triangle positions as ground-truth waypoints — if a
   proposed-fix anchor doesn't change these positions' Waffle-side
   membership, the anchor is probably wrong.

The cases F0044 (cleanest, 88 common / 0 missing) and F0020 (97
missing) are the highest-signal cases; F0045 and R0092 have 0 common
and reflect deep tessellation-grid divergence rather than single-fix
recoverability.
