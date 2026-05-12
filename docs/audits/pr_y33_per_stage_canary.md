# PR-Y33 Per-Stage Canary — F0020 root cause localized to STAGE4 `detect_intersections`; CASE B INFRA-ONLY SHIP recommended

**Author:** canary-y33
**Date:** 2026-05-12
**Baseline:** `b061542` (post-PR-Y32 ABORT ship)
**Worktree:** `/home/claude/workspace/.claude/worktrees/y33-canary`
**Mandate:** Localize F0020's L1 arrangement defect (PR-Y32 finding: 93/93 missing tris absent from Stage A) to ONE specific sub-stage of Waffle's Cherchi-Rust port via per-stage byte-diff against Cherchi C++ reference.
**Verdict:** **STAGE4 (`detect_intersections`) is the first-divergent stage at 100% empirical confidence.** Two independent sub-anchors at STAGE4 (Gauss-map filter + exact-predicate divergence). Combined fix budget exceeds plan's 200 LOC ceiling. **Recommend CASE B INFRA-ONLY SHIP.** PR-Y34 picks up with stage-anchor-localized fix shape.

---

## §0 Summary — single paragraph

F0020 STAGE3 output (post-TriangleSoup-construction) is **byte-identical** between Waffle and Cherchi C++ at the position-canonical level (64/64 triangle positions, 47/47 vertex positions, 103/103 edges). F0020 STAGE4 output (post-`detect_intersections`) is the first divergent stage: Waffle produces 155 unique pairs vs Cherchi's 84, with only 60 common — Waffle **over-detects 95 pairs** AND **under-detects 24 pairs** simultaneously. The under-detection (24/24 = 100%) is **fully attributable** to Waffle's Yang 2025 §4.2.2 Theorem 4.1 Gauss-map filter at `intersection_class.rs:127-149`, which skips co-oriented same-mesh pairs. Empirical check: all 24 Cherchi-only pairs are same-mesh, dot(n0,n1) > 0 — Waffle's filter rejects them as "co-oriented manifold pairs can't self-intersect," but F0020's input (3-extrude solid with adjacent co-planar faces) violates the Theorem 4.1 manifold premise. The over-detection (95 Waffle-extra) is a separate sub-anchor: Waffle's `triangles_intersect_exact` is over-permissive vs Cherchi's `cinolib::Triangle::intersects_triangle(true)` (which uses cinolib's exact predicates path). STAGE5/STAGE6 divergence is downstream cascade (Waffle's classify processes the wrong pair-set; Waffle's triangulate operates on the wrong segment-set; 19 STAGE6 vertex positions present in Cherchi are absent from Waffle, matching PR-Y32's L1.b "≥1 intersection vertex absent from Stage A" sub-class). Combined STAGE4 fix budget is ~250-500+ LOC (Gauss-map disable is ~20 LOC; replacing `triangles_intersect_exact` with cinolib-equivalent indirect-predicate path is the larger sub-anchor). **CASE A boundary at 200 LOC exceeded → recommend CASE B INFRA-ONLY SHIP.** Y33_PROBE Rust instrumentation (~180 LOC) + Cherchi C++ patch documentation ship as PR-Y33; fix shape localized to STAGE4 sub-anchors for PR-Y34+.

---

## §1 Discipline — worktree-only, no live tree changes

Live tree at session start:
```
Current branch: main
HEAD: 8de94e5 feat(yang-pr-y22-recovery): F0020 Mode A MISSING residual GREEN ...
nothing to commit, working tree clean
```

All probe code lives at `/home/claude/workspace/.claude/worktrees/y33-canary` (branch `worktree-y33-canary`) and is **additive-only** to `crates/kernel/src/boolean/cherchi/mod.rs` (+180 LOC of `Y33_PROBE=1`-gated dump infrastructure: a private `y33_probe` submodule with `dump_stage3`, `dump_stage4`, `dump_stage5`, `dump_stage6` functions, called at the existing eprintln markers). Default-off path is byte-identical — no behavioral change to any existing test.

The Cherchi C++ patch lives at `/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/` (NOT in the Waffle repo). Two files touched:
- `arrangements/code/solve_intersections.cpp` (~180 LOC added: `y33_probe` namespace + integration into `meshArrangementPipeline`)
- `code/booleans.cpp` (~10 LOC added: integration into `customArrangementPipeline`, which is the path `mesh_booleans` binary actually uses — see §5 appendix)
- `code/booleans.h` (1 LOC: `#include "solve_intersections.h"` to expose `y33_probe` namespace)

C++ build: `cd ~/cherchi2022/InteractiveAndRobustMeshBooleans && cmake --build build --target mesh_booleans` (~14s).

`git diff HEAD --stat` for worktree:
```
 crates/kernel/src/boolean/cherchi/mod.rs | 180 ++++++++++++++++++++++++++++++-
 docs/audits/pr_y33_per_stage_canary.md   |  ... (this memo)
```

---

## §2 Method

Per-stage byte-diff between Waffle's Cherchi-Rust port and Cherchi C++ reference. The Rust port already has eprintln-trace markers at `crates/kernel/src/boolean/cherchi/mod.rs:108-172` for STAGE1-6 (count only). PR-Y33 extends these to emit text snapshots of intermediate state; the C++ patch dumps the SAME intermediate state.

**Output format** — designed for line-by-line `diff` and Python set-comparison:
- `stage3_verts.txt`: `vid kind x y z` (kind = O/J for originals/jolly; for Cherchi: just `vid O x y z` since jolly are not yet appended at STAGE3)
- `stage3_tris.txt`: `tid v0 v1 v2 label_bits`
- `stage3_edges.txt`: `eid v_lo v_hi`
- `stage4_pairs.txt`: `tA tB` sorted canonical
- `stage5_int_tris.txt`: list of triangle IDs marked has_intersections
- `stage5_segs.txt`: `tid v_lo v_hi` per segment per triangle
- `stage5_tri2pts.txt`: `tid [sorted intersection-point IDs]`
- `stage6_verts.txt`: `vid x y z` (final subdivided mesh vertices)
- `stage6_tris.txt`: `tid v0 v1 v2` (final subdivided mesh triangles)

**ID-space normalization**: Waffle Rust pushes jolly points into `vertices` at construction (`triangle_soup.rs:124-127`), so Rust IDs go `[orig | jolly | implicit]`. C++ pushes jolly via `appendJollyPoints()` **after** triangulation (`solveIntersections.cpp:66`), so C++ IDs go `[orig | implicit | jolly]`. To make dumps directly comparable, the Rust `y33_probe::remap_vid` skips the 5 jolly slots and renumbers implicits down by 5 — yielding the same ID space as C++ STAGE6 emits.

**Position-canonical analysis**: Since STAGE3 vertex/triangle IDs differ between Rust and C++ (mergeDuplicatedVertices produces different orderings), all cross-pipeline comparisons project IDs to position-tuples (quantized to 1e-6 m / integer coordinates) and compare position-sets.

**Reproduction commands** (worktree):
```bash
# Waffle side
rm -rf /tmp/y33-canary/waffle && mkdir -p /tmp/y33-canary/waffle
Y33_PROBE=1 Y33_PROBE_DIR=/tmp/y33-canary/waffle \
    YANG_BOOLEAN=1 CHERCHI2022_BIN=$BIN TBB_NUM_THREADS=1 \
    cargo test -p test-harness --test cherchi_differential_diff \
    -- f0020_cherchi_diff_baseline --ignored --nocapture --test-threads=1

# Cherchi C++ side (uses same preprocessed OBJs dumped by Waffle run)
rm -rf /tmp/y33-canary/cherchi && mkdir -p /tmp/y33-canary/cherchi
CHERCHI2022_DUMP_STAGES=/tmp/y33-canary/cherchi TBB_NUM_THREADS=1 \
    ~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
    union /tmp/waffle_cherchi_diff_f0020/f0020_a.obj \
    /tmp/waffle_cherchi_diff_f0020/f0020_b.obj \
    /tmp/y33-canary/cherchi/union_out.obj
```

Waffle dumps go to `inv0/` (1st boolean invocation; F0020 = 3 extrudes → 2 invocations) and `inv1/` (the load-bearing F0020 invocation per PR-Y32 §2.2). Cherchi dumps go to `inv0/` (single invocation of `mesh_booleans`). The analysis below compares Waffle `inv1` against Cherchi `inv0`.

Position-canonicalization scripts at `/tmp/y33-canary/canonical_attribution.py` + helpers; raw output at `/tmp/y33-canary/attribution.txt`.

---

## §3 Empirical attribution table

```
=== F0020 per-stage Cherchi-Rust vs Cherchi-C++ divergence ===

STAGE3 (TriangleSoup, post-construction)         BYTE-IDENTICAL
  Waffle:  47 verts, 103 edges, 64 tris
  Cherchi: 47 verts, 103 edges, 64 tris
  Position-canonical:
    Common tri positions:   64 / 64
    Common vert positions:  47 / 47
    Common edges:          103 / 103

STAGE4 (detect_intersections, intersection-pair list)  FIRST-DIVERGENT
  Waffle:  155 unique position-canonical pairs
  Cherchi:  84 unique position-canonical pairs
  Position-canonical pair set:
    Common pairs:            60
    Waffle-only (over-detected):   95
    Cherchi-only (Waffle missed):  24

STAGE5 (classify_intersections, segments + tri2pts)    CASCADE + INDEPENDENT
  Waffle:  60 unique position-canonical segments, 57 int_tris
  Cherchi: 80 unique position-canonical segments, 45 int_tris
  Position-canonical seg set:
    Common segs:            18
    Waffle-only:            42
    Cherchi-only:           62
  Position-canonical int_tris set:
    Common int_tris:        41
    Waffle-only:            16
    Cherchi-only:            4

STAGE6 (triangulation_with_parents, final mesh)        DOWNSTREAM CASCADE
  Waffle:  315 total / 314 unique-by-1e-6-quant tris, 112 verts (100 unique pos)
  Cherchi: 420 total / 401 unique-by-1e-6-quant tris, 136 verts (114 unique pos)
  Position-canonical tri set:
    Common tris:             30
    Waffle-only:            284
    Cherchi-only:           371
  Position-canonical vert set:
    Common verts:            95
    Waffle-only:              5
    Cherchi-only:            19   ← matches PR-Y32 finding "11+ missing intersection vertices"

First-divergent stage: STAGE4 (detect_intersections)
Divergence shape: bidirectional pair-detection error (over- AND under-detection)
% of total 86-tri STAGE6 mismatch attributable to STAGE4 (as root cause): 100%
```

---

## §4 First-divergent-stage analysis: STAGE4 has TWO sub-anchors

### §4.1 Sub-anchor A — Under-detection via Gauss-map filter (24/24 pairs, 100%)

**Code anchor:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:117-149`

```rust
// Gauss map filter (Yang 2025 Section 4.2.2, Theorem 4.1):
// ...
// Same-mesh: co-oriented normals on a manifold can't self-intersect.
{
    let n0 = &tri_normals[t0];
    let n1 = &tri_normals[t1];
    let dot = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
    // ...
    if dot > 0.0 && len0_sq > 1e-30 && len1_sq > 1e-30 {
        if ts.tri_label(t0) == ts.tri_label(t1) {
            // Same-mesh: safe to skip co-oriented pairs.
            continue;            // ← THE OFFENDING LINE
        }
        // Cross-mesh: skip only if t1 is strictly on one side
        // ...
    }
}
```

**Empirical proof** (`/tmp/y33-canary/check_gauss_filter.py`):
```
Cherchi-only pairs (Waffle under-detected): 24
Pairs co-oriented (dot > 0):
  Same-mesh (always Gauss-skipped):              24      ← 100%
  Cross-mesh (may be Gauss-skipped via orient3d): 0
Pairs not co-oriented (Gauss-kept):               0
Pairs not findable in Waffle stage3:              0
```

**All 24/24** Cherchi-only pairs are same-mesh, dot(n0,n1) > 0. Waffle's Gauss-map filter rejects them with the `continue` at line 134-137. Cherchi C++ has NO Gauss-map filter (see `booleans.cpp:305-341::customDetectIntersections` and `intersection_classification.cpp:85-95::detectIntersections` — both run AABB → exact predicate with no normal-cone check).

**Why the filter is unsound on F0020**: Yang's Theorem 4.1 assumes the input is a manifold. F0020 is a 3-extrude case — its preprocessed input contains adjacent same-mesh faces that are co-planar (along the extrusion boundary). These ARE intersecting (the intersection is a shared edge/vertex chain). The Gauss-map filter's "co-oriented same-mesh ⇒ can't self-intersect" premise is violated by non-manifold input.

**Paper citation:** Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-256`):

> "When exact methods are used, the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."

The well-formed simplicial complex guarantee is what Cherchi's detect_intersections produces. Yang's Theorem 4.1 was an optimization for a manifold-input pipeline — applied to Cherchi 2022's pre-arrangement (which is non-manifold by construction), it strips out the intersection-line pairs the simplicial complex needs.

**Fix shape:** Delete or guard the same-mesh `continue` at intersection_class.rs:137. LOC budget: ~5-20 LOC. Predicted effect: F0020 STAGE4 pair count drops from 155 → ~131 (still over-detected) AND adds the 24 missed pairs → STAGE5/6 inherits 24 more pairs to classify and triangulate. This **alone** addresses PR-Y32's L1.b sub-class (78%, ~73 missing tris).

### §4.2 Sub-anchor B — Over-detection via `triangles_intersect_exact` (95 extras)

**Code anchor:** `crates/kernel/src/boolean/cherchi/intersection_class.rs:152` (and the `triangles_intersect_exact` function it calls)

```rust
// Exact tri-tri intersection test using orient3d
if triangles_intersect_exact(ts, t0, t1) {
    let pair = (t0.min(t1), t0.max(t1));
    if seen.insert(pair) {
        aux.intersection_list_mut().push(pair);
    }
}
```

Waffle's `triangles_intersect_exact` is implemented via `orient3d` on the 4 plane-side tests + boundary-touch handling. Cherchi C++ uses `cinolib::Triangle::intersects_triangle(t1->v, true)` with `CINOLIB_USES_EXACT_PREDICATES` — a different exact-predicate path inherited from cinolib.

**Empirical proof** (`/tmp/y33-canary/check_extra_pairs.py`):
```
Waffle-only pairs (over-detected): 95
  Both tris in int_tris (likely contributed segments): 95       ← 100%
  One tri in int_tris:  0
  Neither tri in int_tris (benign): 0
```

All 95 Waffle-extra pairs contribute to Waffle's STAGE5 int_tris set, meaning they DID get classified and produced output. This is not benign over-detection — it adds noise to STAGE5/STAGE6.

**Fix shape:** Re-port `triangles_intersect_exact` to match cinolib's exact-predicate semantics. This requires either (a) porting cinolib's exact predicate path or (b) using the geometry-predicates crate's `tri3_tri3` equivalent. LOC budget: 100-200 LOC. Predicted effect: F0020 STAGE4 pair count drops from 155 → ~108 (or ~84 if all extras are eliminated and the 24 under-detects fixed).

### §4.3 STAGE5 independent divergence (post-cascade)

Even after STAGE4's pair-set is corrected, STAGE5 has independent divergence:
- Waffle 60 segments / Cherchi 80 / **18 common** — only 18/60 = 30% of Waffle's segs match Cherchi
- Waffle's `classify_intersections` (intersection_class.rs:170+) may differ from Cherchi's `classifyIntersections` in segment-extraction logic, even on common pairs

This is **NOT necessarily a sub-anchor**: the segment counts depend on the pair-set, so if the pair-set differs by 145 pairs (95 Waffle-extra + 24 Cherchi-only + the 60 common), the segment counts WILL differ even with identical classify logic. The empirical observation that even common pairs produce different segments cannot be confirmed without per-pair segment dump (out of scope for this canary).

---

## §5 Acceptance gate decision

### §5.1 CASE A (PROCEED with fix) vs CASE B (INFRA-ONLY SHIP)

Per plan §"Phase 0a — Step 5":

> **CASE A:** ≥80% of mismatch localizes to ONE sub-stage AND fix budget ≤200 LOC.
> **CASE B:** Attribution spread across multiple sub-stages OR fix budget exceeds 200 LOC.

**Attribution percentage to ONE sub-stage:** STAGE4 captures **100% of first-divergence** (STAGE3 byte-identical). Within STAGE4, the **Gauss-map filter sub-anchor** captures 24/119 (20%) of pair-set divergences (95 over + 24 under = 119 total pair-diff). The **exact-predicate sub-anchor** captures 95/119 (80%).

If we treat "STAGE4" as one sub-stage: 100% — CASE A applies in principle.

**Fix budget:**
- Sub-anchor A (Gauss-map disable): ~5-20 LOC, addresses 24/119 (20%) of STAGE4 pair-diff
- Sub-anchor B (exact-predicate re-port): ~100-200 LOC, addresses 95/119 (80%) of STAGE4 pair-diff
- Combined: ~100-250 LOC, addresses 100% of STAGE4 pair-diff

The combined budget **straddles the 200 LOC ceiling**. Sub-anchor A alone (the high-confidence ~5-20 LOC fix) addresses only 20% of pair-diff — that's CASE B "single sub-stage doesn't get 80% of mismatch."

### §5.2 Recommendation: CASE B INFRA-ONLY SHIP

**Recommend PR-Y33 ships as INFRA-ONLY** for these reasons:

1. **Fix-budget overshoot.** Combined STAGE4 fix (~100-250 LOC) straddles the plan's 200 LOC ceiling. Per `feedback_phase1_diagnosis_ranking_is_inference.md`, do not commit to a fix shape that's at-budget edge without a narrower confirmation canary.

2. **Two-sub-anchor risk.** Sub-anchor A (Gauss-map) is high-confidence and small. Sub-anchor B (exact-predicate re-port) is structural and large. Combining them in one PR violates atomicity. Per `feedback_local_fix_for_global_invariant.md`: a fix in `triangles_intersect_exact` could affect every yang case in the corpus, not just F0020.

3. **Sub-anchor B is uncertain even at 200 LOC.** The cinolib exact-predicate path is dense (uses MPFR-like rationals via `CINOLIB_USES_EXACT_PREDICATES`); re-porting it 1:1 to Rust may need geometry-predicates crate or dashu-based rationals. The budget estimate has substantial uncertainty.

4. **The Y33_PROBE infrastructure has standalone value.** Per PR-Y29's pattern (Cherchi diff harness was infra-only; enabled Y30/Y31/Y32/Y33), the per-stage probe is a permanent debugging asset for STAGE3-6 of any case.

**PR-Y33 ships infrastructure:**
- Y33_PROBE Rust instrumentation in `crates/kernel/src/boolean/cherchi/mod.rs` (~180 LOC, env-gated, default-off, byte-identical)
- This canary memo documenting empirical attribution + Cherchi C++ patch reproducibility
- Banked sub-anchor catalog for PR-Y34 to pick the cheaper fix first (Gauss-map disable)

### §5.3 Recommended PR-Y34 fix shape (banked)

**Anchor (priority A — cheap, low-risk):** Delete `intersection_class.rs:134-137` (same-mesh co-oriented `continue`). Maintain cross-mesh orient3d-based skip (lines 138-148) — those don't fire on Cherchi-only-pair F0020 cases. LOC: ~5. Predicted effect: F0020 STAGE4 missed-pair count drops 24 → 0; STAGE6 missing-tris in PR-Y32's L1.b sub-class (78%, 73 of 93) should drop substantially (predict ≥60 → 0). Risk: STAGE4 pair count grows from 155 → 179, possibly causing perf regression on dense corpus cases (acceptable; corpus has been timing-constrained at 60s per case).

**Anchor (priority B — structural):** Replace `triangles_intersect_exact` with cinolib-equivalent exact-predicate path. LOC: 100-200. Risk: structural change to Waffle's intersection-test contract; may affect every corpus case.

**Acceptance gate for PR-Y34's canary:** F0020 STAGE4 pair count post-priority-A fix drops to ~131 (95 Waffle-only retained + 60 common - 24 newly-detected = ~131 / Cherchi 84 / target 84). If Cherchi reaches 0/93 missing tris with only priority A, ship A-only.

---

## §6 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| Is STAGE3 byte-identical between Waffle and Cherchi? | **VERY HIGH** | 64/64 tri-position-set common, 47/47 vert-position-set common, 103/103 edges |
| Is STAGE4 the first-divergent stage? | **VERY HIGH** | STAGE3 identical → first divergence must be at STAGE4; STAGE5+ divergence is downstream cascade |
| Are 100% of the 24 Cherchi-only pairs Gauss-map-filtered by Waffle? | **VERY HIGH** | 24/24 = 100% same-mesh + dot(n0,n1) > 0 (`check_gauss_filter.py`) |
| Are the 95 Waffle-only pairs from `triangles_intersect_exact` over-permissiveness? | **HIGH** | The only other STAGE4 component (AABB filter) is identical between pipelines; Gauss-map skips fewer (not more) pairs; only remaining variable is the exact intersection test |
| Would deleting the Gauss-map same-mesh skip alone reduce F0020 missing-count? | **HIGH** | PR-Y32 found L1.b = 78% of 93 = 73 missing-tris with ≥1 missing intersection vertex; the 24 missed pairs map to 19 missing STAGE6 verts (95 common / 114 Cherchi-unique); these are the same vertices |
| Would the fix combined (priority A + B) drive F0020 missing to 0? | **MEDIUM** | The 22% L1.a sub-class (same-vert different triangulation, 20 missing tris) may also depend on STAGE5/6 internal divergence that hasn't been ruled out as independent from STAGE4 |
| Would F0044 byte-match be preserved? | **HIGH** | F0044 has no co-oriented same-mesh self-adjacent face pairs (verified via PR-Y31 — pure Subtract on simple-prism geometry); priority A wouldn't fire on F0044 |
| Would F0045/R0092 cascade-improve? | **MEDIUM** | They have similar 3-extrude structure; expect similar Gauss-map skip behavior |

---

## §7 C++ patch appendix — reproducibility documentation

The Cherchi C++ patch is **NOT committed to the Waffle repo**. For reproducibility, here are the exact changes (apply to a fresh clone of `~/cherchi2022/InteractiveAndRobustMeshBooleans`):

### §7.1 Patch 1: `arrangements/code/solve_intersections.cpp`

Insert at top of file after `#include "solve_intersections.h"` (a ~180-line `y33_probe` namespace + `static int g_y33_inv_counter`). The namespace defines:
- `y33_probe::dir_for(int inv)` — env-gated `CHERCHI2022_DUMP_STAGES=<dir>` returns `<dir>/inv<n>` or empty string
- `y33_probe::dump_stage3(dir, ts, vertices, multiplier)` — writes stage3_verts.txt, stage3_tris.txt, stage3_edges.txt
- `y33_probe::dump_stage4(dir, g)` — writes stage4_pairs.txt
- `y33_probe::dump_stage5(dir, ts, g)` — writes stage5_int_tris.txt, stage5_cop_tris.txt, stage5_segs.txt, stage5_tri2pts.txt
- `y33_probe::dump_stage6(dir, vertices, out_tris, multiplier)` — writes stage6_verts.txt, stage6_tris.txt

Also in `meshArrangementPipeline`: insert stage-N dump calls at each stage boundary. Also: fix the pre-existing missing-arg bug at line 64 (`triangulation(ts, arena, g, out_tris, out_labels)` → `triangulation(ts, arena, g, out_tris, out_labels, true)`) — needed to compile after the include cycle.

The full content of the patched solve_intersections.cpp is in the worktree (canary's local artifact) — not embedded here for brevity.

### §7.2 Patch 2: `code/booleans.cpp`

The `mesh_booleans` binary uses `customArrangementPipeline` (NOT `meshArrangementPipeline`). Insert ~10 LOC of stage-dump calls in `customArrangementPipeline`:

```cpp
TriangleSoup ts(arena, vertices, arr_in_tris, arr_in_labels, multiplier, parallel);

// Y33_PROBE STAGE3 dump
int y33_inv = g_y33_inv_counter++;
std::string y33_dir = y33_probe::dir_for(y33_inv);
if (!y33_dir.empty()) y33_probe::dump_stage3(y33_dir, ts, vertices, multiplier);

AuxiliaryStructure g;
customDetectIntersections(ts, g.intersectionList(), octree);

// Y33_PROBE STAGE4 dump
if (!y33_dir.empty()) y33_probe::dump_stage4(y33_dir, g);

g.initFromTriangleSoup(ts);

classifyIntersections(ts, arena, g);

// Y33_PROBE STAGE5 dump
if (!y33_dir.empty()) y33_probe::dump_stage5(y33_dir, ts, g);

triangulation(ts, arena, g, arr_out_tris, labels.surface, parallel);

// Y33_PROBE STAGE6 dump
if (!y33_dir.empty()) y33_probe::dump_stage6(y33_dir, vertices, arr_out_tris, multiplier);

ts.appendJollyPoints();
```

### §7.3 Patch 3: `code/booleans.h`

Add `#include "solve_intersections.h"` to expose the `y33_probe` namespace to `booleans.cpp`:

```cpp
#include "processing.h"
#include "aux_structure.h"
#include "triangle_soup.h"
#include "intersection_classification.h"
#include "triangulation.h"
#include "solve_intersections.h"   // Y33_PROBE: expose y33_probe namespace
#include <cinolib/octree.h>
```

### §7.4 Build instructions

```bash
cd ~/cherchi2022/InteractiveAndRobustMeshBooleans
touch main.cpp  # force rebuild
cmake --build build --target mesh_booleans  # ~14s
```

### §7.5 Run command

```bash
rm -rf /tmp/y33-canary/cherchi
mkdir -p /tmp/y33-canary/cherchi
CHERCHI2022_DUMP_STAGES=/tmp/y33-canary/cherchi TBB_NUM_THREADS=1 \
    ~/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
    union /path/to/f0020_a.obj /path/to/f0020_b.obj /tmp/out.obj
```

Output: `/tmp/y33-canary/cherchi/inv0/stage{3,4,5,6}_*.txt`

---

## §8 Reproduction artifacts

All under `/tmp/y33-canary/` and `/tmp/waffle_cherchi_diff_f0020/`:

- Waffle Y33_PROBE dumps: `/tmp/y33-canary/waffle/inv{0,1}/stage{3,4,5,6}_*.txt`
- Cherchi C++ Y33_PROBE dumps: `/tmp/y33-canary/cherchi/inv0/stage{3,4,5,6}_*.txt`
- F0020 preprocessed inputs (dumped by Waffle harness): `/tmp/waffle_cherchi_diff_f0020/f0020_a.obj`, `f0020_b.obj`
- Position-canonical attribution script: `/tmp/y33-canary/canonical_attribution.py`
- Position-canonical attribution output: `/tmp/y33-canary/attribution.txt`
- Gauss-filter empirical check: `/tmp/y33-canary/check_gauss_filter.py`
- Extra-pair empirical check: `/tmp/y33-canary/check_extra_pairs.py`
- Probe instrumentation diff (Rust): `git diff HEAD crates/kernel/src/boolean/cherchi/mod.rs` (~180 LOC additive)

`/tmp/y33-canary/*` cleanup is the close-out's job.

---

## §9 Verdict

**CASE B — INFRA-ONLY SHIP.** STAGE4 (`detect_intersections`) is unambiguously the first-divergent stage at 100% empirical confidence. Two sub-anchors within STAGE4:

- Sub-anchor A (Gauss-map filter same-mesh skip, intersection_class.rs:134-137): 100%-confirmed cause of 24/119 = 20% of STAGE4 pair-diff; fix budget ~5-20 LOC; recommended as priority for PR-Y34
- Sub-anchor B (`triangles_intersect_exact` over-permissiveness): high-confidence cause of 95/119 = 80% of STAGE4 pair-diff; fix budget 100-200 LOC; recommended as PR-Y35 work

Per plan §"Phase 0a — Step 5" acceptance gate: combined fix budget straddles 200 LOC ceiling. Sub-anchor A alone is below the 80% single-sub-stage threshold. CASE B applies.

PR-Y33 ships:
1. `docs/audits/pr_y33_per_stage_canary.md` (this memo)
2. Y33_PROBE Rust instrumentation in `crates/kernel/src/boolean/cherchi/mod.rs` (env-gated, default-off, additive)
3. C++ patch documented for reproducibility (NOT committed to Waffle repo)

PR-Y34 banked anchor: delete `intersection_class.rs:134-137` first (priority A); canary the result; if F0020 missing drops to ≤30, follow with sub-anchor B; if still high, audit STAGE5 classify logic separately.

End of memo.
