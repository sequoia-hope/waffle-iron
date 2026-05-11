# PR-Y31 Canary — F0044 48-extras layer attribution + fix-shape recommendation

**Agent:** `canary-y31`
**Date:** 2026-05-08
**Subject commit:** `27a09ed`
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md`
**Pre-canary baseline memo:** `docs/audits/pr_y30_stage_b_baselines.md`

## Verdict — RECOMMEND ABORT PRODUCTION FIX; PIVOT TO HARNESS FIX

The "48 extras" reported for F0044 by the PR-Y30 harness are **NOT a Waffle
production defect.** They are an artifact of the PR-Y29/PR-Y30 harness
invoking Cherchi 2022's `mesh_booleans` with a hard-coded `union` operator
(`crates/test-harness/tests/cherchi_differential_diff.rs:286`) regardless
of what F0044's actual `.waffle` model prescribes. **F0044's first
boolean operation is `Subtract` (cut=true).** When the canary re-runs
Cherchi with the matching `subtraction` operator, Cherchi's output is
**byte-identical to Waffle Stage B at the 1µm quantization grid** (136
triangles, 72 vertices, 0 missing, 0 extras, 136 common).

PR-Y31's production fix targets (arrangement / classification /
op-selection) are all **refuted** by direct probe data:

- Waffle's arrangement output for F0044's first op: 72 verts, 136 sub-tris
  (88 A-sub-tris + 48 B-sub-tris). Cherchi's arrangement output: 77 verts,
  136 sub-tris. The 5-vertex-count delta is downstream-irrelevant
  (arrangement vertex deduplication style differs but the simplicial
  complex it produces is functionally equivalent — see §1).
- Waffle's `label_cells` produces the correct labeling for F0044's first
  op (A all Outside, B all Inside — see §2).
- Waffle's `face_survival_detect` correctly filters via Subtract's
  (Outside, Inside, flip_b=true) selector — kept_a=88, kept_b=48, total=136
  (see §3). This matches Cherchi's `subtraction` output exactly.

**Per the canary acceptance gate's option (b):** no single production
layer accounts for the 48 extras because the 48 extras have a non-production
root cause. **Recommend ABORT production fix; ship the harness fix in PR-Y31.**

## Probes added (worktree only, default-off)

The probe code lived in `/tmp/y31-probe-wt` (worktree of `27a09ed`),
gated on `Y31_PROBE=1`, default-off byte-identical. Removed after the
canary run completed. Summary:

| Anchor | File:line | Output |
|---|---|---|
| **Step 1: arrangement** | `crates/kernel/src/boolean/exact_mesh.rs:2541` | exit of `subdivide_mesh_pair_full_cherchi`; reports `verts`, `sub_tris_a`, `sub_tris_b`, `total_sub_tris`, `upstream_tri_count`, `cherchi_raw_tris` |
| **Step 2: classification** | `crates/kernel/src/boolean/exact_mesh.rs:2123` | exit of `label_cells`; reports `labels_a outside/inside`, `labels_b outside/inside`, `n_a`, `n_b` |
| **Step 3: op-selection** | `crates/kernel/src/boolean/topology_extract.rs:1927` | exit of `face_survival_detect`; reports `op`, `keep_a`, `keep_b`, `flip_b`, `kept_a`, `kept_b`, `groups_total`, `n_a`, `n_b` |

An additional probe was added on the Cherchi side
(`/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/code/booleans.cpp`
inside `booleanPipeline`, gated on `Y31_CHERCHI_PROBE=1`) to dump
arrangement cardinality between `customArrangementPipeline` and
`customBooleanPipeline`. Reverted after use.

## §1 — Arrangement-layer probe: REFUTED as F0044 defect anchor

### Waffle arrangement output (F0044 first boolean, decisive call)

```
[y31-probe-arrangement] subdivide_output: verts=72 sub_tris_a=88 sub_tris_b=48 total_sub_tris=136 upstream_tri_count=136 cherchi_raw_tris=136
```

### Cherchi arrangement output (same A.obj / B.obj inputs)

```
[y31-cherchi-arr] arr_verts=77 arr_out_tris_flat=408 arr_out_tris=136 dupl_triangles=0 in_tris=136
```

### Interpretation

- Waffle arrangement: **72 verts, 136 tris** (all 88 A-tris + all 48 B-tris pass through unchanged — A and B don't intersect)
- Cherchi arrangement: **77 verts, 136 tris** (same triangle count; 5-vert delta is dedup-policy difference at coincident input vertices)

**Triangle count is identical (136).** Neither side produces extra
sub-triangles via intersection refinement, because F0044 mesh B is
strictly *contained inside* mesh A (no boundary intersection — see
bounding-box analysis below). The arrangement step is contributing
**zero** to the "48 extras" delta.

### Why F0044's A and B don't intersect

`f0044_a.obj` bounding box (preprocessed mesh A):
- x ∈ [−0.4289, 0.4289], y ∈ [−0.4333, 0.4333], z ∈ [0.0, 0.4194]

`f0044_b.obj` bounding box (preprocessed mesh B):
- x ∈ [0.0362, 0.2983], y ∈ [−0.3561, −0.0941], z ∈ [0.0645, 0.3548]

B is strictly contained in A's bounding box, and A is a solid (per the
assay corpus). Therefore: mesh B is inside mesh A. The expected boolean
results:
- `union(A, B) = A` → 88 triangles (Cherchi confirms: 88 tris, 46 verts)
- `subtraction(A, B) = A` (outer) ∪ `B` (flipped-inward) → 136 triangles
- `intersection(A, B) = B` → 48 triangles

## §2 — Classification-layer probe: REFUTED as F0044 defect anchor

### Waffle label_cells output (F0044 first boolean)

```
[y31-probe-classify] labels_a: outside=88 inside=0 | labels_b: outside=0 inside=48 | n_a=88 n_b=48
```

### Interpretation

`label_cells` correctly identifies:
- All 88 of mesh A's sub-tris as Outside (mesh A is the enclosing solid; none of A's surface is inside B)
- All 48 of mesh B's sub-tris as Inside (mesh B is fully enclosed by A; all of B's surface is inside A)

This is **the Cherchi-correct labeling** per Cherchi 2022 §5 Algorithm 1
("compute and sort intersections between r and M; if volume is negative
then set P as being inside M"). The labels match what Cherchi's
inside/outside ray-cast classification would produce on the same input.

For comparison with all 7 F0044 boolean calls (`Subtract` x4, `Union` x3):

```
[y31-probe-classify] labels_a: outside=88  inside=0   | labels_b: outside=0   inside=48   <- 1st: Subtract
[y31-probe-classify] labels_a: outside=138 inside=62  | labels_b: outside=99  inside=85   <- 2nd: Union
[y31-probe-classify] labels_a: outside=193 inside=89  | labels_b: outside=143 inside=115  <- 3rd: Union
[y31-probe-classify] labels_a: outside=269 inside=125 | labels_b: outside=199 inside=163  <- 4th: Union
[y31-probe-classify] labels_a: outside=153 inside=130 | labels_b: outside=252 inside=72   <- 5th: Subtract
[y31-probe-classify] labels_a: outside=181 inside=145 | labels_b: outside=262 inside=76   <- 6th: Subtract
[y31-probe-classify] labels_a: outside=249 inside=201 | labels_b: outside=347 inside=119  <- 7th: Subtract
```

## §3 — Op-selection-layer probe: REFUTED as F0044 defect anchor

### Waffle face_survival_detect output (F0044 first boolean, decisive)

```
[y31-probe-survival] op=Subtract keep_a=Outside keep_b=Inside flip_b=true | kept_a=88 kept_b=48 groups_total=136 n_a=88 n_b=48
```

### Interpretation

`face_survival_detect` is correctly applying the Subtract selector
(Yang 2025 §4.4.2 cell selection table — "Subtract keeps A-Outside +
B-Inside-flipped"):
- kept_a = 88 (all A-Outside; correct — A's outer surface is kept)
- kept_b = 48 (all B-Inside, with flip_b=true; correct — B's inner-facing
  surface becomes the cavity wall)
- groups_total = 136 = kept_a + kept_b

This **matches the well-formed B-Rep boolean specification for Subtract.**
The 136 triangles are precisely what F0044's actual `.waffle` model
prescribes. No production-side filtering bug.

### The 7-boolean op breakdown for F0044

```
[y31-probe-survival] op=Subtract keep_a=Outside keep_b=Inside  flip_b=true  | kept_a=88  kept_b=48   groups=136  <- 1st (dumped pair)
[y31-probe-survival] op=Union    keep_a=Outside keep_b=Outside flip_b=false | kept_a=138 kept_b=99   groups=237  <- 2nd
[y31-probe-survival] op=Union    keep_a=Outside keep_b=Outside flip_b=false | kept_a=193 kept_b=143  groups=336  <- 3rd
[y31-probe-survival] op=Union    keep_a=Outside keep_b=Outside flip_b=false | kept_a=269 kept_b=199  groups=468  <- 4th
[y31-probe-survival] op=Subtract keep_a=Outside keep_b=Inside  flip_b=true  | kept_a=153 kept_b=72   groups=225  <- 5th
[y31-probe-survival] op=Subtract keep_a=Outside keep_b=Inside  flip_b=true  | kept_a=181 kept_b=76   groups=257  <- 6th
[y31-probe-survival] op=Subtract keep_a=Outside keep_b=Inside  flip_b=true  | kept_a=249 kept_b=119  groups=368  <- 7th
```

F0044 = 4× Subtract + 3× Union. The PR-Y30 baseline's Stage B (136 tris)
reflects the FIRST boolean's result (Subtract). The harness reads
`stage_B.obj` after `run_single_case` returns; the conformal-probe-gated
dump fires on the first qualifying call and is overwritten only on
subsequent qualifying calls. For F0044 specifically, the dumped Stage B
= first Subtract's 136-tri output.

## §4 — The harness mis-config (load-bearing finding)

### Code anchor

`crates/test-harness/tests/cherchi_differential_diff.rs:286`:

```rust
let mut cmd = Command::new(bin);
cmd.arg("union").arg(path_a).arg(path_b).arg(path_out);   // <- hard-coded "union"
```

The harness's `invoke_cherchi_union` invokes Cherchi's `mesh_booleans`
binary with `union` regardless of what `.waffle` model prescribes for
the boolean being compared. PR-Y29 introduced this hard-coding; PR-Y30
preserved it.

### F0044's model prescribes Subtract

`/home/claude/workspace/app/tests/cases/assay/F0044.waffle` boolean ops
(via `grep "\"cut\":"`):

```
"cut": false,    <- 1st extrude (the dumped mesh A)
"cut": true,     <- 2nd extrude — Subtract, this is the operation Waffle runs on (A, B)
"cut": false,    <- 3rd extrude — Union
"cut": false,    <- 4th extrude — Union
```

The DUMPED A/B inputs correspond to the FIRST cut=true (Subtract) op.
The harness's hard-coded `union` invocation against this pair causes
Cherchi to emit `union(A, B) = A = 88 tris` instead of the correct
`subtraction(A, B) = 136 tris`. The "48 extras" reported by PR-Y30 are
exactly the 48 triangles of mesh B that would be flipped-and-kept by
Subtract but discarded by Union.

### Empirical verification — Cherchi-subtraction matches Waffle Stage B byte-identically

```
$ /home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
    subtraction /tmp/waffle_cherchi_diff_f0044/f0044_a.obj \
                /tmp/waffle_cherchi_diff_f0044/f0044_b.obj \
                /tmp/y31_cherchi_f0044_subtract.obj
$ grep -c "^v " /tmp/y31_cherchi_f0044_subtract.obj
72
$ grep -c "^f " /tmp/y31_cherchi_f0044_subtract.obj
136
```

Position-quantized triangle-set diff against Waffle Stage B
(same 1µm grid, winding-insensitive — matches the harness's `quantize_tri`
function exactly):

```
Cherchi subtraction: 72 verts, 136 tris, 136 unique quantized
Waffle Stage B    : 72 verts, 136 tris, 136 unique quantized
In Cherchi, not in Waffle: 0
In Waffle, not in Cherchi: 0
Common: 136
```

**Perfect byte-identical match.** The 48 extras vanish entirely under
correct Cherchi op selection.

### Compare to Cherchi-union (current harness behavior)

```
$ /home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
    union ... /tmp/y31_cherchi_f0044_union.obj
# verts=46 tris=88 — matches PR-Y30 baseline exactly (88/0/48 from earlier diff)
```

Cherchi-union emits `A = 88 tris` (correct for union since B is inside
A, but **wrong reference** for the diff because Waffle ran Subtract,
not Union).

## §5 — Layer attribution

| Layer | Evidence | Verdict |
|---|---|---|
| Arrangement (`subdivide_mesh_pair`) | 136 tris on both sides; 72 verts (Waffle) vs 77 verts (Cherchi); both well-formed | **REFUTED** as defect anchor (no extra tris emitted; vert-count delta is dedup style, not defect) |
| Classification (`label_cells`) | A all-Outside, B all-Inside — Cherchi-correct labels for B-inside-A topology | **REFUTED** as defect anchor |
| Op-selection (`face_survival_detect`) | Applies Subtract's (Outside, Inside, flip_b=true) correctly: kept_a=88, kept_b=48 | **REFUTED** as defect anchor |
| **Harness Cherchi invocation** | `cmd.arg("union")` hard-coded; F0044's first op is Subtract | **CONFIRMED** as defect anchor |

No production layer accounts for the 48 extras. The dominant cause is
harness mis-configuration; ABORT production fix is the correct call.

## §6 — Single fix-shape recommendation: align Cherchi op with Waffle op

### Fix shape

Plumb the actual boolean op used by Waffle for the boolean being
diffed into the harness's Cherchi invocation, so that `mesh_booleans`
runs the matching op (`union` / `subtraction` / `intersection`).

### Code anchor (test infrastructure, NOT production)

- `crates/test-harness/tests/cherchi_differential_diff.rs:277-318`
  (`invoke_cherchi_union` → rename to `invoke_cherchi` and take a
  `MeshBooleanOp` parameter) plus `crates/test-harness/tests/cherchi_differential_diff.rs:322-475`
  (`run_diff_for_case` — pass the op down).
- Op source: `run_single_case` returns an `AssayResult`; the canonical
  op for the first dumped boolean must be extracted from the model
  pipeline. Simplest approach: extend `run_waffle_and_collect_dumps`
  to capture the op of the **first** boolean operation that writes to
  `YANG_DUMP_OBJ_BASE` and propagate it as a return field. Alternative:
  read the `.waffle` JSON's first `cut` flag (cut=true → Subtract,
  cut=false → Union; Intersection isn't currently in the assay corpus).

### Paper citation (Cherchi 2022 §3, verbatim)

> "Our method takes as input a set of input meshes M1, M2, ..., Mn, and
> **a Boolean operator, namely union, intersection, subtraction**. ...
> The output is a mesh B that contains the result of applying the
> Boolean operator to the input meshes."
> — Cherchi 2022 §3, lines 232–236

The reference algorithm's output is parameterized by the boolean
operator. Comparing Waffle-Subtract against Cherchi-Union is not a
reference-parity check; it's a category error.

### LOC budget estimate

- Harness change: **~10–20 LOC** (rename function, plumb op enum, map
  op enum → CLI string)
- Op extraction: **~5–15 LOC** (read .waffle JSON or instrument
  `run_single_case` to surface the op).

Total: **~15–35 LOC** in test-harness only. **Zero production code
changes.**

### Predicted post-fix F0044 Stage B extras

**0.** Empirically verified above: Cherchi-subtraction vs Waffle Stage B
matches 136/136 at the 1µm quantization grid.

This is not inference — it is direct measurement using the exact
quantization function and grid the harness uses.

### Predicted F0020 cascade

**Unchanged. F0020 will still report 107 extras** at Stage B (or the
non-deterministic equivalent — Cherchi's union output for F0020 varies
between 246 and 295 tris across runs even at TBB_NUM_THREADS=1, per
PR-Y30 banked finding).

Why: F0020's `.waffle` ops are all `cut=false` = Union. The harness
already invokes Cherchi with `union` correctly for F0020 — there is no
op mis-alignment. F0020's defect is real (post-PR-Y31).

For full transparency, the F0020 spotlight probe data:

```
[y31-probe-survival] op=Union keep_a=Outside keep_b=Outside flip_b=false | kept_a=44  kept_b=32  groups=76  n_a=52  n_b=52    <- 1st (dumped pair)
[y31-probe-survival] op=Union keep_a=Outside keep_b=Outside flip_b=false | kept_a=164 kept_b=130 groups=294 n_a=185 n_b=130   <- 2nd
```

F0020's first survival has 76 tris (44+32) but downstream stages
likely re-mesh; the harness baseline reports 294-tri Waffle output
(PR-Y30: 294 tris, 117 verts, 137-185 common, 107 extras). F0020's
real defect remains to be localized in a future PR.

### Predicted F0045 / R0092 impact

- **F0045 unchanged.** F0045's ops are all `cut=false` = Union. Harness
  already correct. F0045 has 0 common at 1µm — its defect is
  tessellation-grid divergence (Yang §4.1.1 — discretization produces
  non-matching vertex positions). Independent of op alignment.
- **R0092 unchanged or marginally improved.** R0092 has one `cut=true`
  + one `cut=false`. The dumped pair may correspond to either; if it's
  the Subtract pair, post-fix R0092 extras may drop. If it's the Union
  pair, R0092 is unchanged. Either way, R0092's defect is dominated by
  Cherchi non-determinism (153/295/405 tris across runs), so the fix
  effect would be marginal at best.

### Confidence

**Very high.** Three independent verifications all converge:

1. Triangle-count arithmetic: `Subtract output = A-Outside + B-Inside-flipped = 88 + 48 = 136 = Waffle Stage B`.
2. Op-selector probe: Waffle's `face_survival_detect` enum confirms the op is `Subtract` with `keep_a=Outside, keep_b=Inside, flip_b=true`. The `.waffle` JSON's `cut=true` flag confirms the model prescribes Subtract.
3. Direct Cherchi re-invocation with `subtraction` yields 136 tris that match Waffle Stage B byte-identically at 1µm.

No assumptions, no inference chains — all three are direct
measurements on the load-bearing case.

## §7 — Strategic implications

### PR-Y31's scope must pivot

Per CLAUDE.md "P9-P10":
> "If you can't explain why a test fails, don't change code to make it
> pass. No tolerance widening, no special-case branches, no fallback
> paths that produce right answers for wrong reasons. Document in
> PLAN.md and move on."

The brief's planned production fix (arrangement / classification /
op-selection patch) would have produced a "right answer for wrong
reason" outcome: any production change that drops 48 tris from F0044
Stage B would BREAK F0044's correct Subtract behavior. The PR-Y30
banked finding ("the 48 extras at Stage B are the same 48 at Stage C,
and the geometric pattern is identical") was load-bearing but
**misinterpreted** — it ruled out flood-fill as the cause but did not
rule out the comparison-side mis-config.

The recommended ship is a **harness fix** producing:
- F0044 Stage B extras: 48 → 0
- F0020 Stage B extras: 107 → 107 (unchanged; real defect)
- F0045 Stage B extras: 466 → 466 (unchanged; tessellation-grid)
- R0092 Stage B extras: 368 → 368 (unchanged; non-determinism)

This is a real corpus improvement (one Status:Failed case retires from
the extras-watch) without touching production code. It also
**re-establishes the load-bearing oracle** as a reliable signal for
future PRs: the next time the diff shows extras > 0, it will be a real
defect, not an op mismatch.

### Banked findings for downstream PRs

1. **F0020 defect is still real.** F0020's 107 Stage B extras at Union
   reflect a genuine divergence between Waffle and Cherchi's pipelines.
   PR-Y32+ should target F0020 specifically. The classification probe
   shows F0020's labeling is NOT all-Outside-on-A — there are both
   Outside and Inside on both sides (e.g., labels_a outside=44 inside=8
   on first call), so the defect is more nuanced. Re-run the same
   3-step canary on F0020 with corrected harness as the new oracle.

2. **The harness diff oracle is salvageable but needs the op-plumbing
   fix to be load-bearing.** PR-Y31's harness fix is a prerequisite for
   any future "use the harness as fix-shape gate" PR. Without it, the
   harness conflates "Waffle is wrong" with "harness asked wrong question".

3. **The Cherchi side-probe technique (Y31_CHERCHI_PROBE) is reusable.**
   For future arrangement-vs-classification disambiguation, the
   `[y31-cherchi-arr]` probe (added to `customArrangementPipeline`
   inside `booleanPipeline`) lets us read Cherchi's intermediate
   arrangement cardinality. Reverted in this canary but the patch
   pattern is documented in §1 for future reuse.

## Verbatim probe data summary

### Step 1 — arrangement (Waffle worktree probe + Cherchi binary patch)

```
Waffle  [y31-probe-arrangement]: verts=72  sub_tris_a=88  sub_tris_b=48  total=136
Cherchi [y31-cherchi-arr]:       arr_verts=77  arr_out_tris=136
```

### Step 2 — classification (Waffle worktree probe)

```
[y31-probe-classify] labels_a: outside=88 inside=0 | labels_b: outside=0 inside=48
```

### Step 3 — op-selection (Waffle worktree probe)

```
[y31-probe-survival] op=Subtract keep_a=Outside keep_b=Inside flip_b=true | kept_a=88 kept_b=48 groups_total=136
```

### Cherchi-subtraction re-invocation (corrected reference)

```
Cherchi mesh_booleans subtraction: 72 verts, 136 tris
Diff against Waffle Stage B at 1µm grid: 0 missing, 0 extras, 136 common
```

## Recommendation summary

| Item | Value |
|---|---|
| Layer attribution | NONE-of-production; harness mis-config |
| Fix shape | Plumb Waffle's actual `MeshBooleanOp` into Cherchi harness invocation |
| Code anchor | `crates/test-harness/tests/cherchi_differential_diff.rs:277-318` (`invoke_cherchi_union` → `invoke_cherchi(op)`) + `:286` (`cmd.arg("union")` → `cmd.arg(op_to_cli_str(op))`) |
| Paper citation | Cherchi 2022 §3 lines 232–236: "*a Boolean operator, namely union, intersection, subtraction*" |
| LOC budget | ~15–35 LOC in test-harness only; **0 LOC production code** |
| Predicted F0044 post-fix extras | **0** (empirically verified) |
| Predicted F0020 cascade | unchanged (107 extras); real defect persists |
| Predicted F0045 / R0092 cascade | unchanged (independent of op alignment) |
| Confidence | Very high (3 independent verifications, no inference chains) |

## What the canary did not investigate (anti-scope)

- F0020's real 107-extras defect — out of scope for PR-Y31 per brief.
  PR-Y32+ canary should re-run the same 3-step probe on F0020 once
  the harness fix lands.
- The 5-vertex-count discrepancy (Cherchi 77 vs Waffle 72) in F0044's
  arrangement output. Both produce 136 well-formed sub-triangles; the
  vertex count delta does not affect the topology or the boolean
  result. Documented but not load-bearing.
- F0044's other 6 boolean ops (3 Union + 3 more Subtract). The harness
  only dumps the first op pair, so subsequent ops are not in the diff
  oracle's window. Out of scope for this PR.
- Cherchi non-determinism on F0020/R0092 — banked from PR-Y30,
  separate investigation.

## Worktree cleanup

```
$ git worktree remove /tmp/y31-probe-wt --force
$ git worktree list
/home/claude/workspace                27a09ed [main]
```

Probes were never committed. Cherchi-side patch reverted; cherchi
binary rebuilt clean.
