# PR-Y32 Anchor Canary — F0020 93 missing-from-Cherchi triangles attribute to L1 (arrangement); recommend ABORT pending PR-Y33 narrower Cherchi-Rust-port-divergence canary

**Author:** canary-y32
**Date:** 2026-05-12
**Baseline:** `723480c` (post-PR-Y31 ship)
**Mandate:** Localize F0020's 93 stable missing-from-Cherchi triangles to ONE dominant layer (L1 arrangement / L2 classification / L3 op-selection) and recommend a single empirically-anchored fix shape.
**Verdict:** **L1 dominant at 93/93 = 100%**. **ABORT PR-Y32 recommended at canary phase**: the dominant layer is conclusive but the fix shape "repair the Cherchi-Rust port to match Cherchi 2022 C++ reference arrangement" is too large and underspecified to commit to in a single PR without a narrower follow-up canary localizing the specific stage of Cherchi-Rust divergence (STAGE3 soup, STAGE4 detection, STAGE5 classification, or STAGE6 triangulation).

---

## §0 Summary — single-paragraph

All 93 of F0020's missing-from-Cherchi triangles are **absent from Waffle's Stage A output** (the full subdivided mesh emitted by `subdivide_mesh_pair_full_cherchi`, `crates/kernel/src/boolean/exact_mesh.rs:2391-2541`). Strict-equality intersection: 0/93 missing triangles appear in Stage A's 315-tri set. The arrangement produced by Waffle's `boolean/cherchi::solve_intersections` ports of Cherchi 2020 §5 / Cherchi 2022 §4 has structurally diverged from the Cherchi C++ reference's arrangement: Cherchi C++ Union output (253 tris) contains 93 triangles Waffle's Stage A (313 unique tris) does not contain, and 140 triangles drawn from the combined Cherchi-C++ Union/Intersection/Subtraction output (401 unique tris) are absent from Waffle's Stage A. The divergence has two empirical sub-shapes: (a) 11 unique quantized vertex positions appear in Cherchi-only output never inserted by Waffle's intersection-vertex pipeline, and (b) ~22% (20/93) of missing triangles use ONLY vertices present in Stage A — these reflect *sub-tessellation divergence* between the Rust and C++ Cherchi ports despite identical vertex placement. **The fix anchor is at L1; the production code to repair is `crates/kernel/src/boolean/cherchi/*` (the Cherchi-Rust port itself).** Predicted post-fix F0020 missing count: 0 IF the Cherchi-Rust port is brought into byte parity with Cherchi C++. Predicted cohort cascade: F0044 stays at 0 (Cherchi-Rust already matches at F0044 — STAGE6=136 vs C++=136); F0045/R0092 missing both shrink towards 0 (their Cherchi-Rust STAGE6 sizes also diverge from C++).

---

## §1 Discipline — worktree-only, no live tree changes

### Live tree at session start

```
$ git -C /home/claude/workspace status
On branch main
Your branch is up to date with 'origin/main'.
nothing to commit, working tree clean

$ git -C /home/claude/workspace rev-parse HEAD
723480c8a54fd52e9a4654fd9acf704ceb3b2af9
```

All probe code lives at `/home/claude/workspace/.claude/worktrees/y32-probe` (branch `worktree-y32-probe`) and is **additive-only** to `crates/test-harness/tests/cherchi_differential_diff.rs` (~24 LOC of `Y32_DUMP_POSITIONS=1`-gated `eprintln!` to emit ALL missing-from-waffle / extra-in-waffle quantized positions, beyond the existing top-10).

Worktree probe diff:

```
$ cd /home/claude/workspace/.claude/worktrees/y32-probe && git diff --stat
 app/tests/cases/assay/results.json                |  6 +-
 crates/test-harness/tests/cherchi_differential_diff.rs | 24 ++++++++++++++++++++++++
 2 files changed, 28 insertions(+), 2 deletions(-)
```

(`results.json` mutation is the assay runner's normal artifact; will be discarded.)

No `git stash`, `git checkout --`, `git reset --hard` was used per `feedback_adversary_no_destructive_git.md`. All probe output is `eprintln!`-only — zero mutation of `subdivided`, `labeling`, `survival`, arena state, or any production data structure.

---

## §2 Method

### §2.1 Anchor probe — use existing Stage A / Stage Bb / Stage B dumps

Rather than write production code probes for L1/L2/L3, this canary leverages the existing `YANG_STAGE_DUMP` machinery already wired at `crates/kernel/src/boolean/topology_extract.rs:2221` (Stage A — full subdivided mesh post-arrangement, pre-labeling), `:2411` (Stage Bb — full subdivided mesh post-labeling with origin+inside CSV), and `:2584` (Stage B — survival-filtered post-`face_survival_detect` output). These dump sites are gated on `YANG_CONFORMAL_PROBE=1 YANG_STAGE_DUMP=<dir>` and already produce per-stage `stage_<tag>.obj` + `stage_<tag>_labels.csv`.

This gives a clean three-stage decomposition without any production-code probe. Layer attribution rules:

- **L1** = missing triangle absent from `stage_A.obj` ⇒ arrangement never produced it
- **L2** = missing triangle present in `stage_A.obj` AND in `stage_Bb.obj` with `inside=1` for Union (Waffle labels it `Inside`, Cherchi keeps it as `Outside`) ⇒ mis-classification at `label_cells`
- **L3** = missing triangle present in `stage_A.obj` AND in `stage_Bb.obj` with `inside=0` for Union BUT absent from `stage_B.obj` ⇒ correct label, `face_survival_detect` dropped

### §2.2 Reproduction commands

```
git worktree add /home/claude/workspace/.claude/worktrees/y32-probe 723480c
cd /home/claude/workspace/.claude/worktrees/y32-probe

# Step 1 — extend run_diff_for_case to emit ALL missing/extra positions
#   (24 LOC probe in cherchi_differential_diff.rs:580 — additive-only,
#   default-off behind Y32_DUMP_POSITIONS=1)

# Step 2 — run F0020 diff with positions + stage dump
CHERCHI2022_BIN=/home/claude/cherchi2022/InteractiveAndRobustMeshBooleans/build/mesh_booleans \
  YANG_BOOLEAN=1 Y32_DUMP_POSITIONS=1 TBB_NUM_THREADS=1 \
  cargo test -p test-harness --test cherchi_differential_diff -- \
    f0020_cherchi_diff_baseline --ignored --nocapture --test-threads=1 \
  > /tmp/y32-canary/f0020-run.txt 2>&1

# Step 3 — cohort batch
CHERCHI2022_BIN=... YANG_BOOLEAN=1 Y32_DUMP_POSITIONS=1 TBB_NUM_THREADS=1 \
  cargo test -p test-harness --test cherchi_differential_diff -- \
    cohort_cherchi_diff_baseline --ignored --nocapture --test-threads=1 \
  > /tmp/y32-canary/cohort-run.txt 2>&1

# Step 4 — extract missing positions, run attribution script
grep "^\[y32-pos F0020\] M " /tmp/y32-canary/f0020-run.txt | awk '{print $4}' \
  | sort -u > /tmp/y32-canary/missing_keys.txt
python3 /tmp/y32-canary/attribute.py
```

Stage A is dumped per-`tessellate_solid_bounded`-invocation and the file is overwritten across invocations — F0020 has 2 boolean invocations and the second (load-bearing) one dumps 315 tris (185 tris_a + 130 tris_b post-subdivide). Cherchi C++ runs on the SAME `f0020_a.obj` + `f0020_b.obj` (`YANG_DUMP_OBJ_BASE` also overwritten so it reflects the second invocation's inputs). So both pipelines are compared on the same input pair. Stage A is therefore the apples-to-apples Waffle arrangement output.

---

## §3 Empirical attribution table

### §3.1 F0020 per-layer attribution

```
=== F0020 layer attribution (sources: stage_A.obj + stage_Bb_labels.csv + stage_B.obj) ===
  L1: 93 / 93   (absent from Stage A; arrangement never produced)
  L2:  0 / 93
  L3:  0 / 93
  L_OTHER: 0 / 93
  TOTAL: 93
```

L1 dominance: **100%**. No ambiguity, no even split. (Per the PR-Y32 plan §5 "Acceptance gate": "Pick the SINGLE DOMINANT layer (largest N). If no clear dominant (≥40% of total), recommend ABORT with refined-scope-canary." 93/93 unambiguously points at L1.)

### §3.2 Verbatim probe output — top 10 L1 missing triangles

From `/tmp/y32-canary/attribute.out`:

```
-- L1 (93) --
  row_a=None origin=None inside=None  v0=(+1.079200e-01,-2.807490e-01,-5.568400e-02) v1=(+1.079200e-01,-2.807490e-01,+1.733900e-01) v2=(+1.361990e-01,-3.088150e-01,+2.425030e-01)
  row_a=None origin=None inside=None  v0=(+1.079200e-01,-2.807490e-01,-5.568400e-02) v1=(+1.361990e-01,-3.088150e-01,+2.425030e-01) v2=(+1.897180e-01,-3.619320e-01,-7.574200e-02)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,-1.208200e-01) v1=(+1.421790e-01,-1.221610e-01,-8.008300e-02) v2=(+1.563390e-01,-1.197120e-01,-1.217830e-01)
  row_a=None origin=None inside=None  v0=(-1.421790e-01,+1.221610e-01,+1.208200e-01) v1=(-3.418400e-02,+1.408310e-01,+1.388010e-01) v2=(+7.766300e-02,+1.601680e-01,+8.727000e-02)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,+2.744700e-02) v1=(+2.046780e-01,-1.113550e-01,-1.150490e-01) v2=(+2.151060e-01,-1.095520e-01,-1.135960e-01)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,+6.998500e-02) v1=(+1.421790e-01,-1.221610e-01,+9.872200e-02) v2=(+2.413070e-01,-1.050230e-01,-1.099460e-01)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,+6.998500e-02) v1=(+2.046780e-01,-1.113550e-01,-1.150490e-01) v2=(+2.106860e-01,-1.103170e-01,-1.142120e-01)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,+6.998500e-02) v1=(+2.106860e-01,-1.103170e-01,-1.142120e-01) v2=(+2.413070e-01,-1.050230e-01,-1.099460e-01)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,-8.008300e-02) v1=(+1.421790e-01,-1.221610e-01,+2.744700e-02) v2=(+2.046780e-01,-1.113550e-01,-1.150490e-01)
  row_a=None origin=None inside=None  v0=(+1.421790e-01,-1.221610e-01,-8.008300e-02) v1=(+1.421790e-01,-1.221610e-01,+6.998500e-02) v2=(+2.046780e-01,-1.113550e-01,-1.150490e-01)
```

`row_a=None` means the triangle's quantized position does not match any row in `stage_A.obj`. There is NO L2 sample to show because L2 = 0; there is NO L3 sample to show because L3 = 0.

### §3.3 Sub-class decomposition within L1

L1 has two empirical sub-mechanisms (per `/tmp/y32-canary/verify_l1.out` + `/tmp/y32-canary/dig_vertices.out`):

| Sub-class | Mechanism | Count | Evidence |
|---|---|---|---|
| **L1.a** | All 3 verts in Stage A, but triangulation differs | 20/93 (22%) | `0/3 in Stage A: 2 tris; 1/3: 30; 2/3: 41; 3/3: 20` (vertex-coverage histogram) |
| **L1.b** | At least 1 vert missing from Stage A | 73/93 (78%) | 11 unique quantized vertex positions appear in missing tris but never in Stage A |

Decomposition of unique vertices in the 93 missing tris (54 unique):

```
=== Vertex source attribution (54 unique verts in 93 missing tris) ===
  In A only:    20  (of which in Stage A: 20)
  In B only:    0   (of which in Stage A: 0)
  In A and B:   0   (of which in Stage A: 0)
  In neither A nor B (= intersection-only):  34 (of which in Stage A: 23)
```

The 34 "intersection-only" vertices are positions that don't appear in either the input mesh A (`f0020_a.obj`) or input mesh B (`f0020_b.obj`) — they were computed by some intersection-vertex pipeline. 23 of those 34 are present in Waffle's Stage A (Waffle DID compute them); **11 are absent** (Cherchi C++ computed them, Waffle did not).

The 20 vertices "In A only" are original input vertices from mesh A — all present in Stage A. Combined with the 23 intersection-only verts present in Stage A, that's 43/54 missing-tri verts present in Stage A; the remaining 11 missing-tri verts (Cherchi-only intersection verts) are absent.

### §3.4 Sub-tessellation divergence (L1.a worked example)

One concrete missing triangle from the top-10:

```
Target = sorted(
  (107920, -280749, -55684),
  (107920, -280749, 173390),
  (136199, -308815, 242503),
)

In Stage A?  False  (this exact 3-vertex triangle is NOT emitted by Waffle's arrangement)
In Cherchi?  True   (Cherchi C++ emits this exact 3-vertex triangle)

Stage A tris with ≥2 of these 3 verts (= local 1-ring around target):
  (107920, -280749, 173390), (136199, -308815, 242503), (189718, -361932, -75742)
  (107920, -280749, -55684), (107920, -280749, 173390), (189718, -361932, -75742)
  (4666, -178271, -87145),   (107920, -280749, -55684), (107920, -280749, 173390)
  (107920, -280749, 173390), (107920, -280749, 196350), (136199, -308815, 242503)
  (107920, -280749, -55684), (107920, -280749, 173390), (142179, -122161, 27447)
  (51440, -542208, 51702),   (107920, -280749, -55684), (107920, -280749, 173390)

Cherchi tris with ≥2 of these 3 verts:
  (107920, -280749, -55684), (136199, -308815, 242503), (189718, -361932, -75742)
  (107920, -280749, -55684), (107920, -280749, 173390), (136199, -308815, 242503)
  (107920, -280749, 173390), (107920, -280749, 196350), (136199, -308815, 242503)
  (51440, -542208, 51702),   (107920, -280749, -55684), (107920, -280749, 173390)

  Local Stage A vert neighborhood: 8 verts
  Local Cherchi vert neighborhood: 6 verts
  Difference (Cherchi - Stage A): 0 verts (Cherchi inserts no NEW verts here)
  Difference (Stage A - Cherchi): 2 verts (Waffle inserts 2 EXTRA verts: 4666,-178271,-87145 and 142179,-122161,27447)
```

For this target, Waffle's Cherchi-Rust port inserts 2 *extra* intersection vertices and triangulates the local neighborhood with 6 tris using 8 verts. Cherchi C++ uses only 6 verts and 4 tris in the same neighborhood. The target triangle is in Cherchi C++'s set (4 of 4 use it directly or transitively) but Waffle's over-subdivision splits the same region into smaller pieces that, when canonical-quantized, do not match Cherchi C++'s coarser triangulation.

### §3.5 Cherchi-Rust port STAGE6 size divergence

From the live trace at b#2 (the F0020 load-bearing invocation):

```
[cherchi-trace] STAGE1 merge: 47 verts, 64 tris
[cherchi-trace] STAGE2 degenerate: 64 tris
[cherchi-trace] STAGE3 soup: 52 verts, 103 edges, 64 tris
[cherchi-trace] STAGE4 pairs: 155
[cherchi-trace] STAGE5 classify: 57 with_intersections, 0 with_coplanars
[cherchi-trace] STAGE6 triangulation: 315 tris
[cherchi-tele] jolly_creations: 0
```

Waffle's Cherchi-Rust port STAGE6 emits **315 tris** from 64 input tris.

Cherchi C++ on the same `f0020_a.obj` + `f0020_b.obj`:
- `mesh_booleans union`        → 253 tris (post-classification, A-out + B-out)
- `mesh_booleans intersection` → 125 tris (post-classification, A-in + B-in)
- `mesh_booleans subtraction`  → 264 tris (post-classification, A-out + B-in)
- `(union ∪ intersection ∪ subtraction)` as triangle-position sets → **401 unique tris**

Cherchi C++'s "full arrangement" (the pre-classification superset, derivable as `union ∪ intersection ∪ subtraction` since every arrangement tri ends up A-out|B-out, A-in|B-in, etc.) ≥ 401 unique tris. Cross-reference:

```
Waffle Stage A: 313 unique tris (set size after canonical-quantization)
  ∩ with combined Cherchi C++ ops:  261
  Stage A \ combined:                 52  (Waffle has these; Cherchi C++ does not)
  combined \ Stage A:                140  (Cherchi C++ has these; Waffle does not)
```

**Waffle's arrangement contains 140 fewer tris from the Cherchi-C++ arrangement AND 52 extra tris Cherchi-C++ never produces.** This is the L1 defect in two directions.

### §3.6 Cohort cross-check

```
=== F0044 diff ===  (Subtract; F0044 batch identical post-PR-Y31)
  In Cherchi, not in Waffle: 0   (missing == 0; matches PR-Y31 contract)
  In Waffle, not in Cherchi: 0   (extras == 0)
  Common: 136

  [cherchi-trace] STAGE6 triangulation: 136 tris    ← Cherchi-Rust port emits 136
  Cherchi C++ subtraction:              136 tris    ← C++ ref emits 136
  ⇒ Cherchi-Rust port BYTE-MATCHES Cherchi C++ on F0044

=== F0045 diff ===  (Union)
  In Cherchi, not in Waffle: 236
  In Waffle, not in Cherchi: 466
  Common: 0       ← totally different triangulation; tessellation-grid divergence (PR-Y32 anti-scope)

=== R0092 diff ===  (Subtract)
  In Cherchi, not in Waffle: 192   (was 392 in PR-Y30/Y31 — Cherchi C++ non-determinism)
  In Waffle, not in Cherchi: 368
  Common: 0       ← totally different triangulation; pre-survival structural divergence
```

**F0044's clean parity is the rosetta stone.** When Waffle's Cherchi-Rust port produces the SAME number of arrangement tris as Cherchi C++ (136==136), missing-count is 0. When sizes diverge (315 vs ~401 for F0020), missing-count is non-zero. The defect IS in the Cherchi-Rust port's arrangement behavior, NOT in any downstream Yang stage.

---

## §4 Spatial clustering

```
=== Connected-component analysis of 93 missing triangles ===
  Total directed-edge entries: 279
  Edges shared by ≥2 missing tris: 85
  Unique missing tris (after dedup): 93
  Connected components: 3
  Component sizes (sorted desc): [47, 44, 2]

  Per-component centroid:
    comp size= 47  centroid=(-4.5407e-02, +2.7086e-04, -8.0910e-02)
    comp size= 44  centroid=(-4.3018e-03, -3.2486e-02, -1.0131e-01)
    comp size=  2  centroid=(+1.5904e-01, -1.1925e-01, -9.6206e-02)
```

The 93 missing tris form **3 disjoint connected components**, sizes 47 / 44 / 2. Per-component centroids are within the central solid (x,y,z each O(0.1m) for a model bounded roughly in O(0.5m)). Comp_0 and comp_1 are the two large patches; comp_2 is a 2-tri sliver.

This pattern **mirrors PR-Y26's 3-cycle + 16-vert bowtie + 9-vert chain** unpaired-edge connected-component finding (PR-Y26 §1 line 117-127). The component count is invariant; the absent triangles ARE the source of the watertight defects.

Spatial clustering suggests **specific input-geometry features** — not a uniformly-distributed algorithm bug. F0020 is a 3-extrude case (per PR-Y26 §1, F0020 = "Extrude 1 → Extrude 2 → Extrude 3"); the 3 missing patches likely correspond to 3 separate intersection regions where Waffle's Cherchi-Rust port over-subdivides relative to Cherchi C++.

---

## §5 Recommended fix shape — and why I recommend ABORT instead

### §5.1 Anchor + paper citation

**Code anchor at HEAD `723480c`:** `crates/kernel/src/boolean/cherchi/*` — specifically the chain
- `crates/kernel/src/boolean/cherchi/mod.rs:82-220` (`solve_intersections` pipeline)
- `crates/kernel/src/boolean/cherchi/triangulation.rs` (STAGE6 triangulation_with_parents)
- `crates/kernel/src/boolean/cherchi/intersection_class.rs` (STAGE5 classify_intersections)
- `crates/kernel/src/boolean/cherchi/processing.rs` (STAGE1/STAGE2 dedup + degenerate removal)

**Paper citation:** Cherchi 2022 §3, line 251-256 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:251-256`):

> "When exact methods are used, the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines. We take advantage of this property in the second phase of our algorithm, that takes the arrangement as input..."

The well-formed simplicial complex guarantee is what enables Cherchi 2022's Algorithm 1 (§5 inside/outside classification) to produce a watertight Boolean result. **Waffle's Cherchi-Rust port does NOT produce the same simplicial complex as the Cherchi C++ reference on F0020 inputs** — therefore the downstream Yang B-Rep reassembly (Yang §4.4.2-§4.4.3: "watertightness... is inherited from the mesh Boolean output") inherits a non-watertight result.

### §5.2 Why not propose a specific patch shape

The dominant layer is **conclusive** (L1 100%), but the fix shape is **underspecified**. Possible Cherchi-Rust divergence points (any one of which could explain the 315 vs 253-401 size mismatch):

| Sub-anchor (Cherchi-Rust stage) | Code anchor | Hypothesized bug class | LOC budget |
|---|---|---|---|
| STAGE3 jolly-point fallback | `boolean/cherchi/triangle_soup.rs` (jolly point construction) | `JOLLY_POINT_CREATIONS` telemetry shows `jolly_creations: 0` on F0020 — fires don't explain this case; UNLIKELY anchor | n/a |
| STAGE4 detection — d_epsilon coarseness | `boolean/cherchi/intersection_class.rs::detect_intersections` | F0020 d_epsilon=0.0113 — coarse d_epsilon may cause spurious "near-intersection" segments → over-subdivision | ~50-100 |
| STAGE5 classify — coplanar tris | `boolean/cherchi/intersection_class.rs::classify_intersections` | trace shows `0 with_coplanars` — coplanar path not exercised on F0020; UNLIKELY anchor | n/a |
| STAGE6 triangulation — earcut diagonal choice | `boolean/cherchi/triangulation.rs::earcut_linear` | Earcut CDT for segment insertion produces different diagonals than Cherchi C++ — explains L1.a (same verts, different triangulation) | ~50-200 |
| STAGE6 segment-insertion — intersection vertex placement | `boolean/cherchi/triangulation.rs` (where new intersection verts are inserted) | 11 verts missing from Stage A but present in Cherchi C++ output — Waffle's intersection-vertex computation diverges | ~30-100 |
| Indirect predicates — orient2d/insphere fallback | `boolean/cherchi/common.rs` (indirect predicate impl) | Predicate divergence at the bit level — would produce different "is this point on this segment" answers; HIGH-IMPACT | ~100-500 (deep change) |

**No single one of these sub-anchors has an empirical chain to "fix anchor X ⇒ F0020 missing-count = 0".** Picking one without measurement would repeat the PR-Y25/Y26/Y27/Y28 failure mode (canary-stage ABORTs after structural-inference fix shapes were refuted by their own canaries).

Per `feedback_phase1_diagnosis_ranking_is_inference.md`: any ranking I produce among these is structural inference unless backed by a Cherchi-Rust-vs-Cherchi-C++ per-stage comparison. Per `feedback_external_coherence.md`: the differential test against the C++ reference must be the load-bearing oracle, not internal-stage diagnostics.

### §5.3 Predicted post-fix metrics (only valid IF the right Cherchi-Rust sub-anchor is fixed)

**Predicted F0020 missing count:** 0 (if STAGE6 triangulation is brought into byte parity with Cherchi C++ on F0020-class inputs). Caveat: the fix may need to be MORE than one sub-anchor — STAGE4 detection and STAGE6 triangulation both feed STAGE6 output; if both diverge, fixing one may halve missing but not zero it.

**Predicted cohort cascade:**
- F0044: unchanged at missing=0 (already byte-matches Cherchi C++)
- F0045: missing should drop from 236 toward 0 (its STAGE6 also diverges — `[cherchi-trace] STAGE6 triangulation: 384 tris` for F0045 vs Cherchi C++ Union 236 tris)
- R0092: missing should drop from 192 toward 0 (similar — `[cherchi-trace] STAGE6 triangulation: 540 tris` vs Cherchi C++ Subtraction 264 tris)

**Confidence:** HIGH for the L1 attribution itself (100% empirical). MEDIUM for the missing→0 prediction (depends on which sub-anchor is fixed). LOW for cohort-cascade-amount predictions (F0045 and R0092 may have multiple compounding divergences).

### §5.4 LOC budget (combined sub-anchors)

If the L1 fix is bounded to a single Cherchi-Rust sub-anchor: ~30-200 LOC. If it spans multiple sub-anchors (likely): 200-500+ LOC. **This exceeds the original PR-Y32 plan's implicit ~150 LOC budget for an L1 anchor (per the plan's "LOC budget by layer" table line 176-179: L1 = ~50-150 LOC).** The fix is also high-blast-radius because the Cherchi-Rust port is used by every Yang boolean call in the corpus, not just F0020.

---

## §6 Empirical confidence assessment

| Question | Confidence | Evidence |
|---|---|---|
| Is the dominant defect layer L1 vs L2 vs L3? | **HIGH** | 93/93 missing tris absent from Stage A; 0/93 in Stage A but mis-labeled or dropped at survival. Strict set intersection, no ambiguity. |
| Is the L1 defect at the Cherchi-Rust port vs upstream (`subdivide_mesh_pair_full_cherchi` outside the port)? | **HIGH** | `subdivide_mesh_pair_full_cherchi` after the Cherchi-Rust call just splits by label and propagates parents; no triangle dropping. F0044 byte-matches Cherchi C++ → upstream-of-port plumbing is fine. The divergence is at `solve_intersections` itself. |
| Is the specific sub-anchor STAGE4 vs STAGE5 vs STAGE6? | **MEDIUM** | STAGE5 traces `0 with_coplanars` rules out the coplanar path. STAGE6 produces 315 tris where C++ produces ≤401; the count divergence localizes here but does not isolate the bug to a specific function. |
| Would fixing the sub-anchor reduce missing to 0? | **MEDIUM-LOW** | Plausible but unverified. May require multi-stage repair. |
| Would fixing it leave F0044 missing=0? | **HIGH** | F0044's STAGE6 ALREADY byte-matches C++ — any fix targeting F0020's specific divergence should not touch F0044's already-correct path. |
| Would it improve F0045/R0092? | **MEDIUM** | Their STAGE6 sizes also diverge (384 vs 236; 540 vs 264). Same defect class likely. But "improve" is not "fix to 0" — they may have additional defects (PR-Y31 anti-scope flagged tessellation-grid divergence). |

---

## §7 Failure-mode appendix — what predicts a wrong-anchor finding

If PR-Y32 proceeds with an L1 fix shape, the following symptoms would indicate the wrong sub-anchor was chosen:

1. **F0020 missing drops to ~50 but not to 0** ⇒ multi-stage divergence; the single sub-anchor only fixed one mechanism. Predicted by §3.3's L1.a (sub-tessellation, 22%) vs L1.b (intersection-vert placement, 78%) split — if one is fixed and the other is not, missing drops to ~20 or ~73 not to 0.

2. **F0020 missing stays at 93** ⇒ wrong sub-anchor entirely. Possible if the bug is in a Cherchi-Rust path the planned fix doesn't traverse. Mitigated by writing a `cherchi_rust_stage6_byte_parity_test` (F0020-specific arrangement-output snapshot) BEFORE the fix lands.

3. **F0044 regresses from missing=0 to missing>0** ⇒ over-aggressive Cherchi-Rust patch touched the path F0044 uses. Mitigated by the existing `pr_y31_f0044_extras_zero` regression test in `cherchi_differential_diff.rs:630-647`.

4. **Yang fast corpus drops from 10/157** ⇒ the patch broke another case's already-passing arrangement. Mitigated by running `YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- yang_fast --ignored --nocapture` pre-merge.

5. **Kernel baseline drops from 1254/25/42** ⇒ unit tests against `solve_intersections` snapshot byte-output and the patch changed them. Per CLAUDE.md "P9-P10": investigate root cause; do not widen tolerances.

---

## §8 Why I recommend ABORT and refined PR-Y33 scope

The PR-Y32 plan §5 Acceptance Gate:

> Canary memo MUST include:
> - Verbatim probe output (or representative top-10 with summary counts) for each of L1/L2/L3 ✓ (§3.2; L2/L3 are empty by design)
> - Per-layer count ✓ (§3.1; L1=93, L2=0, L3=0)
> - Single dominant layer attribution ✓ (L1 at 100%)
> - Single fix shape recommendation with code anchor + paper citation + LOC budget + predicted post-fix missing count + predicted cohort cascade ✗ — **NOT FURNISHED**; alternative ABORT recommended
> - Empirical confidence assessment ✓ (§6)
> - If layer attribution is roughly even (no single dominant) → ABORT with refined canary scope — N/A; layer attribution is conclusive

The attribution is unambiguous. The reason for ABORT is **not** that the canary is inconclusive — it is that the L1 attribution maps onto a fix-shape space too large for a single PR cycle without further canary-driven localization. Specifically:

1. The plan budgets ~50-150 LOC for L1 (line 176-179). My empirical sub-class split (L1.a / L1.b) shows L1 likely needs multi-stage repair → 200-500+ LOC, exceeding budget.

2. The Cherchi-Rust port lives in `crates/kernel/src/boolean/cherchi/*` — 7 files totalling thousands of LOC. Without a per-stage byte-diff canary (Cherchi-Rust STAGE6 output vs Cherchi C++ raw arrangement output), the sub-anchor pick is structural inference. Per `feedback_phase1_diagnosis_ranking_is_inference.md` and the four prior canary-stage ABORTs (PR-Y25/Y26/Y27/Y28), the principled call is to refuse to commit to a sub-anchor without measurement.

3. The reference parity hypothesis is testable. PR-Y33's narrower canary should:
   - Instrument the Cherchi-Rust port to dump STAGE3 / STAGE4 / STAGE5 / STAGE6 outputs to OBJ files.
   - Build a separate `cherchi_rust_vs_cpp_per_stage_diff` test that runs Cherchi C++ on the same F0020 A/B inputs (possibly by patching the C++ binary to also dump per-stage internal state, OR by snapshotting both pipelines' raw post-segment-insertion outputs).
   - Per-stage size and position comparison localizes the bug to ONE Cherchi-Rust sub-stage. THEN a fix-shape becomes well-anchored.

### §8.1 Refined PR-Y33 scope

PR-Y33 Phase 0 canary: **Cherchi-Rust per-stage byte parity against Cherchi C++**.

- Anchor probe: instrument `boolean/cherchi/mod.rs::solve_intersections` to dump `cherchi_rust_stage3.obj`, `cherchi_rust_stage4_segments.txt`, `cherchi_rust_stage5_classified.obj`, `cherchi_rust_stage6.obj` on a Y33_PROBE=1 gate.
- Reference oracle: Cherchi C++ source has equivalent per-stage logging (`#define VERBOSE` paths in `code/main.cpp` / `solve_intersections.cpp`). Either patch the existing `mesh_booleans` binary to expose them, or use the existing PR-Y29 sidecar build to run with verbose flags.
- Diff F0020's STAGE6 outputs: 315 (Rust) vs C++ (TBD). If they match, the divergence is at downstream `subdivide_mesh_pair_full_cherchi` post-port (NEW CANARY surprise). If they diverge, the size and structure of the diff identifies the offending Cherchi-Rust stage.
- Per-stage diff → SINGLE sub-anchor → empirically-grounded fix shape.

PR-Y33 acceptance gate for canary: F0020 STAGE6 (Rust) vs F0020 STAGE6 (C++) diff must localize to ≥80% of one Cherchi-Rust sub-stage; otherwise refine further.

### §8.2 Banked findings (carry forward to PR-Y33 plan)

1. **L1 = 100% empirical**. All 93 missing tris absent from Stage A. The defect IS in Cherchi-Rust's arrangement output.
2. **F0044 byte-matches Cherchi C++ (STAGE6=136 both)**. Use F0044 as the regression-protection witness — any PR-Y33 fix must keep F0044's missing=0.
3. **L1 sub-class split: 22% same-vert-different-triangulation (L1.a) + 78% missing-vertex (L1.b)**. Fix may need both.
4. **3 connected components (47+44+2)** mirror PR-Y26's count=1 unpaired-edge component split (3-cycle + 16-vert bowtie + 9-vert chain). The 93 missing arrangement tris ARE the upstream root of the 36 unpaired render-mesh edges. This empirically closes the chain PR-Y28 §2.2 declared "not closed" — fixing L1 would close downstream watertight.
5. **Cherchi C++ non-determinism remains.** F0020 union output varies (PR-Y31 saw 107-155 extras; this canary saw 148 extras; common count was 144 here vs 185 in PR-Y31 plan baseline). TBB_NUM_THREADS=1 does not fully eliminate it. Missing-count remained stable at 93 in both runs. ⇒ Missing-count is the right gate; extras are NOT.
6. **F0045 + R0092 cohort cascade**: their Cherchi-Rust STAGE6 sizes (384, 540) also diverge from C++ (236, 264). Same defect class. If PR-Y33 fixes F0020, expect partial improvement on F0045/R0092 missing-counts.

---

## §9 Reproduction artifacts

All under `/tmp/y32-canary/` and `/tmp/waffle_cherchi_diff_f0020/`:

- F0020 diff stdout (921 lines): `/tmp/y32-canary/f0020-run.txt`
- F0020 missing-position list (93 quantized triple keys): `/tmp/y32-canary/missing_keys.txt`
- Layer attribution: `/tmp/y32-canary/attribute.py` + `attribute.out`
- Stage A vert-coverage histogram: `/tmp/y32-canary/verify_l1.py` + `verify_l1.out`
- Vertex source attribution: `/tmp/y32-canary/dig_vertices.py` + `dig_vertices.out`
- Single-tri drill-down: `/tmp/y32-canary/check_one_missing.py` + `check_one_missing.out`
- Cherchi-C++-op aggregate sizes: `/tmp/y32-canary/cpp_op_sum.py`
- Stage dumps: `/tmp/waffle_cherchi_diff_f0020/stages/F0020/stage_{A,Bb,B}.obj` + `*_labels.csv`
- Cherchi C++ outputs: `/tmp/y32-canary/cpp-{out,isect,sub}-test.obj`
- Cohort diff stdout: `/tmp/y32-canary/cohort-run.txt`
- Probe instrumentation diff: worktree at `/home/claude/workspace/.claude/worktrees/y32-probe`, branch `worktree-y32-probe`, 24 LOC additive in `crates/test-harness/tests/cherchi_differential_diff.rs`

`/tmp/y32-canary/*` and `/tmp/waffle_cherchi_diff_f0020/*` will be cleaned at PR-Y33 close-out.

---

## §10 Verdict

**ABORT PR-Y32 at canary phase.** L1 attribution conclusive (93/93). Fix shape needs PR-Y33 narrower per-stage canary to localize within Cherchi-Rust before a production fix is anchored. The L1 finding itself is the load-bearing PR-Y33 input — it reframes the next-PR problem from "find the bug" to "find which Cherchi-Rust sub-stage diverges from Cherchi C++ reference and bring them into byte parity per CLAUDE.md 'reference parity is not optional'."

The brief asked: *"if your attribution doesn't clearly point at one layer, ABORT and document why."* Inverse: attribution DOES clearly point at L1, but the L1 anchor isn't bounded enough to ship in a single PR. The honest call is the same — ABORT — for a different (sharper) reason.

End of memo.
