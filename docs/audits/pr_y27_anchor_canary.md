# PR-Y27 Anchor Canary — NEW HYPOTHESIS: dropout site is in `tessellate_solid_bounded`, not `flood_fill_patches`. The plan's B.1–B.7/A.4/C.4 map is REFUTED for the cohort.

**Author:** canary-y27
**Date:** 2026-05-08
**Plan:** `/home/claude/.claude/plans/optimized-wandering-wind.md` Phase 0 canary
**Verdict:** **NEW HYPOTHESIS** (per plan acceptance gate row 4) — empirical P1 finding refutes the entire dropout-site map for the cohort. **`flood_fill_patches` does NOT drop SourceFaces**: across 8 invocations spanning F0020 + F0044/F0045/R0092, `missing_count = 0`. Survival groups → arena faces is bijective. The 36 + 12 + 38 + 43 unpaired render-mesh edges (PR-Y26 `count=1` boundary signature) originate **DOWNSTREAM** of `flood_fill_patches`, in `tessellate_solid_bounded`'s per-face dispatch loop.
**Recommended next step:** **bring to user for scope decision** before any fix-shape spec is written. Spec-y27 cannot proceed with any of the 7 candidate sites because none of them is dominant; in fact none of them is the source of the cohort defect. The canary's measurement also surfaces a previously-unknown data point: **F0020 loses 8 of 33 arena faces in its render mesh** (face_ranges entries dropped); F0044, F0045 lose 0; R0092 loses 1. The defect is heterogeneous across the cohort.

This memo names empirical evidence only. It does NOT propose fix shape. Per `feedback_anchor_before_fix.md` and `feedback_phase1_diagnosis_ranking_is_inference.md`, scope decision is upstream.

---

## §0 Discipline — live tree untouched

### Live tree at session start and just before writing this memo

```
$ git -C /home/claude/workspace status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean

$ git -C /home/claude/workspace log --oneline -3 main
45e3b5c audit(yang-pr-y26): ABORT at canary phase ...
0bbda11 audit(yang-pr-y26-canary): UNCLASSIFIED + NEW HYPOTHESIS ...
d1a301d audit(yang-pr-y25): ABORT at canary phase ...
```

All probe instrumentation lives in a separate worktree at `/tmp/y27-probe-wt` rooted at `45e3b5c`:

```
$ git worktree add /tmp/y27-probe-wt 45e3b5c
Preparing worktree (detached HEAD 45e3b5c)
HEAD is now at 45e3b5c audit(yang-pr-y26): ABORT at canary phase ...

$ cd /tmp/y27-probe-wt && git diff --stat
 app/tests/cases/assay/results.json            |  10 +-
 crates/kernel/src/boolean/topology_extract.rs | 191 +++++++++++++++++++++++++-
 2 files changed, 195 insertions(+), 6 deletions(-)

$ cd /tmp/y27-probe-wt && git diff | wc -l
297
```

No `git stash`, `git checkout --`, `git reset --hard`, or other destructive op was used on the live working tree. Per `feedback_adversary_no_destructive_git.md`. (`results.json` mutation is the assay test runner's normal artifact; will be discarded with `git worktree remove`.)

### Probe gate

Every probe is gated on `std::env::var("Y27_PROBE").as_deref() == Ok("1")`. Default-off codepath is byte-identical to the `45e3b5c` baseline. All output is `eprintln!`-only. No mutation of `arena`, `face_provenance`, `survival.groups`, or any other state.

### Reproduction commands

```
git worktree add /tmp/y27-probe-wt 45e3b5c
cd /tmp/y27-probe-wt
# (probes injected per §3 below)

# Step 1 — visual stage dump (PR-VIZ-1) for empirical baseline
mkdir -p /tmp/y27-stage-dump-f0020
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 YANG_STAGE_DUMP=/tmp/y27-stage-dump-f0020 \
    cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1

# F0020 P1+P2+P4 (load-bearing)
YANG_BOOLEAN=1 Y27_PROBE=1 cargo test -p test-harness --test assay_randomized -- \
    spotlight_f0020 --ignored --nocapture --test-threads=1 \
    > /tmp/y27-canary-f0020-p14.txt 2>&1

# Cohort batch (F0044+F0045+R0092)
mkdir -p /tmp/y27-stage-dump-f0044
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 YANG_STAGE_DUMP=/tmp/y27-stage-dump-f0044 \
    cargo test -p test-harness --test assay_randomized -- spotlight_f0044 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y27-canary-f0044-stagedump.txt 2>&1

YANG_BOOLEAN=1 Y27_PROBE=1 cargo test -p test-harness --test assay_randomized -- \
    spotlight_f0044 --ignored --nocapture --test-threads=1 \
    > /tmp/y27-canary-f0044-full.txt 2>&1
```

Both spotlight tests reported `test result: ok. 1 passed; 0 failed` (the harness reports test pass/fail; inner spotlight verdicts are `Status: Failed` for all 4 cases — same as `45e3b5c` baseline).

---

## §1 F0020 P1+P2+P4 — load-bearing case

`spotlight_f0020` invokes `flood_fill_patches` twice (one per Boolean operand pair across the three sequential extrudes). The load-bearing one is the **second invocation** (`survival_groups=20 → arena_faces=33`); the render-LOD invocation downstream of it is the one whose 76 triangles the watertight oracle reports `unpaired_count=36` on.

### Verbatim P1 + P2 + P4 summary lines (final invocation)

```
[y27-probe-p4-summary] arena_faces=33 face_open_loops=0 arena_he_with_twin_none=39
[y27-probe-p1-summary] survival_groups=20 arena_faces=33 unique_arena_sources=20 missing_count=0
[y27-probe-p2-summary] B1_canon_degen_drops=6 B1_unique_sources=4
[y27-probe-p2-summary] B5_R3_owner_wins_total=25 B5_loser_drops_total=25 loser_unique_sources=9
[y27-probe-p2-summary] B6_open_chain_patches=16 B6_dead_end_breaks=16
[y27-probe-p2-summary] B7_zero_loop_patches=0 B7_unique_sources_zero=0
```

(Invocation 1 — pre-final — was clean: `face_open_loops=0`, `arena_he_with_twin_none=0`, no B.5/B.6 firings.)

### Drop-site attribution table (final invocation)

| Site | Fired? | Magnitude | SourceFace coverage of missing-from-arena |
|---|---|---|---|
| **B.1** Step 4 canon-degen filter | ✓ | 6 sub-tris dropped | (irrelevant: missing_count=0) |
| **B.4** Step 5a per-source split → 0 components | ✗ | structurally cannot fire (BFS seeds with at least itself) | — |
| **B.5** Step 6 R3 ownership strip | ✓ | 25 edges stripped from 9 loser sources | (irrelevant: missing_count=0) |
| **B.6** Step 6 loop-chaining open-chain emit | ✓ | 16 open chains across same 9 sources | (irrelevant: missing_count=0) |
| **B.7** Step 7 zero-loop patch skip | ✗ | 0 patches skipped | — |
| **A.4** `select_boolean_result` op filter | n/a (dead-code) | live path is `face_survival_detect` (different filter shape; per-tri label keeps; doesn't drop SourceFace) | — |
| **C.4** `label_cells` ray-casting failure | not-instrumented | (would manifest upstream as `survival.groups` shape change; canary did not fire because survival shape matches expected) | — |

### **Critical empirical finding — P1 missing_count=0**

Across **both** invocations of `flood_fill_patches`:
- Invocation 1: `survival_groups=12 arena_faces=26 unique_arena_sources=12 missing_count=0`
- Invocation 2: `survival_groups=20 arena_faces=33 unique_arena_sources=20 missing_count=0`

**Every SourceFace surviving the boolean op makes it into `face_provenance`.** The PR-Y26 §4.1 candidate (m) — *"surface-patch dropout in `flood_fill_patches`"* — is **REFUTED** at the SourceFace granularity. There are no missing surface patches at this layer.

### What B.5 + B.6 actually do

The 9 source faces with B.5/B.6 firings (A:{4,9,10,11,12} + B:{2,3,4,5}) are **NOT dropped from arena** — they each appear in `face_provenance` for at least one face (often multiple, since per-source-face split can produce multiple patches per SourceFace). What B.5/B.6 produce is **open-chain loops** in the patch_boundaries: a chain like `[a→b, b→c, c→d]` with no closing `d→a` edge.

The Step 7 arena-build loop (L1131-1146 in `topology_extract.rs`) wraps `next` cyclically across loop_edges so `outer_loop`'s next-chain IS topologically closed (the last HE points back to first). But the LAST HE in the chain has `(origin=d, dest=a)` directly, despite no `directed_edge_to_tris` entry for `(d, a)` (because the underlying triangulation never produced that edge).

**This results in 39 arena half-edges with `twin=None`** (P4: `arena_he_with_twin_none=39`):
- Some are legitimate non-manifold edges (Cherchi 2022 §3 / Yang §4.4.2: "surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines"; the cohort `[yang-diag] NMM half-edges` line emits this metric).
- The remainder are B.5/B.6-induced "phantom" edges — closed-chain residuals that the validator cannot distinguish from legit NMM (the PR-Y22 M1 + PR-Y22-RECOVERY M2 NMM predicate at L1287-1311 reports them as legitimate via `undirected_count != 2`).

**Per the plan's likely-mapping table:** B.5 was predicted to produce the "9-vert chain (open chain residual)" component. Empirically, B.5 + B.6 fire for 9 unique source faces (matching), but they do NOT cause SourceFace dropout — the symptom propagates downstream as malformed per-face boundary geometry, not as missing arena entries.

### P4 secondary measurement: face_open_loops

```
[y27-probe-p4-summary] arena_faces=33 face_open_loops=0 arena_he_with_twin_none=39
```

`face_open_loops=0` despite 16 B.6 open chains. **The arena's `outer_loop.next`-chain IS cyclically closed by Step 7's wraparound** (L1131-1146). The "openness" of the chain manifests not as topology-cycle violation but as `twin=None` on the wraparound edge — the phantom edge has no real underlying mesh edge, so no twin to pair with.

This is important for downstream: the per-face tessellator walks `outer_loop` via the next chain (which is closed), but the boundary positions it collects via `collect_loop_boundary` may include the phantom edge endpoints. Whether that produces zero triangles, malformed triangles, or seam-mismatches is the **next-investigation question** that PR-Y27 cannot answer without further probing.

---

## §2 Cohort enumeration F0044 / F0045 / R0092

`spotlight_f0044` runs the F0044+F0045+R0092 batch (3 cases). Each case invokes `flood_fill_patches` multiple times (1 + retry rounds). Final-invocation P1/P4 summaries:

| Case | Final flood_fill | P1 missing | P4 face_open_loops | P4 he_twin_none | B.1 drops | B.5 firings | B.6 firings | B.7 firings | Render unpaired (oracle) |
|---|---|---|---|---|---|---|---|---|---|
| **F0044** | 6→8 | 0 | 0 | **0** | 0 | 0 | 0 | 0 | **12** |
| **F0045** | 6→8 | 0 | 0 | **0** | 8 | 0 | 0 | 2 | **38** |
| **R0092** | 5→24 | 0 | 0 | **44** | 0 | 0 | 0 | 0 | **43** |
| **F0020** | 20→33 | 0 | 0 | **39** | 6 | 25 | 16 | 0 | **36** |

**Observations:**

1. **F0044 has a perfectly clean arena exit** — 8 faces, 0 open loops, 0 twin=None, no drop-site fires — yet its render mesh has 12 unpaired edges. This is dispositive: **the defect is downstream of `flood_fill_patches`** for F0044.
2. **F0045 has only B.1 + B.7 firings; arena exit is also clean** (0 twin=None) — yet 38 unpaired in render.
3. **R0092 has 44 legitimate NMM in arena; no drop-site fires** in its final invocation — yet 43 unpaired in render. The 44≈43 correspondence suggests **one-to-one mapping between arena NMM half-edges and render-mesh count=1 unpaired edges** in this case.
4. **F0020 is the OUTLIER** — only F0020 has B.5+B.6 firings (and 39 twin=None). Its arena topology is partially malformed (open-chain residuals), yet `missing_count=0` because the source faces still appear in `face_provenance`.

### Render-mesh face-count differential (NEW signal — not in plan)

Comparing arena face counts to distinct face_ids in the render OBJ (`stage_E_lod=Render_labels.csv`):

| Case | arena_faces (final flood_fill) | render unique_face_ids | **Faces with zero render triangles** |
|---|---|---|---|
| F0020 | 33 | 25 | **8** |
| F0044 | 8 | 8 | **0** |
| F0045 | 8 | 8 | **0** |
| R0092 | 24 | 23 | **1** |

**F0020 loses 8 of 33 arena faces in its render mesh.** These are kernel face_ids that are ABSENT from `face_ranges` because `tessellate_solid_bounded` emits zero indices for them (per L4283-4290: `if end_index > start_index { face_ranges.push(...) }`). This is a **completely separate dropout site** that the plan's map does not cover, and it occurs **inside** `tessellate_solid_bounded`'s per-face dispatch loop — likely at:
- `collect_loop_boundary` (`tessellation/mod.rs`): if it returns < 3 vertices, the fallback branch at L4247 skips emission, OR
- `tessellate_planar_face_bounded` / earcut: returns zero indices for degenerate boundaries.

But F0044 and F0045 lose ZERO faces. **The "missing face" mechanism in F0020 cannot explain F0044/F0045's count=1 unpaired edges.** Two distinct mechanisms are required for the cohort:
- **F0020:** ~8 missing face entries + 39 NMM half-edges (mixed clean/malformed arena exit).
- **F0044/F0045:** clean arena exit; defect is a per-face-tessellation seam mismatch even on closed loops.
- **R0092:** clean arena exit with 44 legit NMM; defect is the per-face tessellator producing position-mismatched boundaries at NMM edges.

---

## §3 Probe instrumentation (worktree-only, NOT committed)

Located under `/tmp/y27-probe-wt`. Single change to `topology_extract.rs`:

```
$ cd /tmp/y27-probe-wt && git diff --stat
 app/tests/cases/assay/results.json            |  10 +-
 crates/kernel/src/boolean/topology_extract.rs | 191 +++++++++++++++++++++++++-
 2 files changed, 195 insertions(+), 6 deletions(-)
```

(`results.json` mutation is the runner's per-test artifact; not probe logic.)

### Block 1 — counters declared at top of `flood_fill_patches`

```rust
let y27_probe = std::env::var("Y27_PROBE").as_deref() == Ok("1");
let mut y27_b1_drops: usize = 0;
let mut y27_b1_sources: BTreeSet<SourceFace> = BTreeSet::new();
let mut y27_b5_loser_drops: BTreeMap<SourceFace, usize> = BTreeMap::new();
let mut y27_b5_owner_wins: BTreeMap<SourceFace, usize> = BTreeMap::new();
let mut y27_b6_open_chain_patches: Vec<(usize, SourceFace)> = Vec::new();
let mut y27_b6_dead_end_breaks: Vec<(usize, SourceFace)> = Vec::new();
let mut y27_b7_zero_loop_patches: Vec<SourceFace> = Vec::new();
let mut y27_patches_with_some_loops: BTreeSet<SourceFace> = BTreeSet::new();
let mut y27_patches_with_zero_loops: BTreeSet<SourceFace> = BTreeSet::new();
```

### Block 2 — B.1 instrumentation at L482

```rust
if cv[0] == cv[1] || cv[1] == cv[2] || cv[0] == cv[2] {
    if y27_probe {
        y27_b1_drops += 1;
        y27_b1_sources.insert(*sf);
    }
    continue;
}
```

### Block 3 — B.5 instrumentation at L862-863 (post `edge_owner.insert`)

```rust
let owner = owner_candidates[0];
edge_owner.insert((v0, v1), owner);
if y27_probe {
    *y27_b5_owner_wins.entry(patches[owner].source).or_insert(0) += 1;
    for &loser_pi in owner_candidates.iter().skip(1) {
        *y27_b5_loser_drops
            .entry(patches[loser_pi].source)
            .or_insert(0) += 1;
    }
}
```

### Block 4 — B.6 instrumentation in loop-chaining inner loop (L949-983)

```rust
let mut y27_closed_normally = false;
loop {
    let (next, is_int) = match outgoing {
        Some(v) if !v.is_empty() => v.remove(0),
        _ => {
            if y27_probe {
                y27_b6_dead_end_breaks.push((pi, patch.source));
            }
            break;
        }
    };
    chain.push((current, next, is_int));
    if next == start {
        y27_closed_normally = true;
        break;
    }
    current = next;
}
if y27_probe && !chain.is_empty() && !y27_closed_normally {
    y27_b6_open_chain_patches.push((pi, patch.source));
}
```

### Block 5 — B.7 + zero-loop bookkeeping just before Step 7 emit loop

```rust
if y27_probe {
    for pb in &patch_boundaries {
        let total_loops = pb.loops.len();
        let nonzero = pb.loops.iter().filter(|l| !l.is_empty()).count();
        if total_loops == 0 || nonzero == 0 {
            y27_patches_with_zero_loops.insert(pb.source);
            // ... accumulate y27_b7_zero_loop_patches ...
        } else {
            y27_patches_with_some_loops.insert(pb.source);
        }
    }
}
```

### Block 6 — P1 + P2 + P4 emission at function exit (just before `ResultTopology { ... }` return)

```rust
if y27_probe {
    // P4: count arena-level faces with malformed outer_loop (open chain)
    let mut face_open_loop_count = 0usize;
    let mut face_total_he_unpaired_in_arena: usize = 0;
    for (fi, _f) in arena.faces.iter().enumerate() {
        let face_obj = &arena.faces[fi];
        let lp = &arena.loops[face_obj.outer_loop.0];
        let start_he = lp.half_edge;
        let mut current = start_he;
        let mut closed = false;
        loop {
            if arena.half_edges[current.0].twin.is_none() {
                face_total_he_unpaired_in_arena += 1;
            }
            let nxt = arena.half_edges[current.0].next;
            if nxt == start_he { closed = true; break; }
            current = nxt;
            // (runaway protection)
        }
        if !closed { face_open_loop_count += 1; }
    }
    eprintln!("[y27-probe-p4-summary] arena_faces={} face_open_loops={} arena_he_with_twin_none={}",
        arena.faces.len(), face_open_loop_count, face_total_he_unpaired_in_arena);

    // P1: enumerate survival.groups vs face_provenance
    let arena_sources: BTreeSet<SourceFace> = face_provenance.values().copied().collect();
    let mut missing_sources: Vec<SourceFace> = Vec::new();
    for (sf, tris) in survival.groups.iter() {
        if !arena_sources.contains(sf) {
            missing_sources.push(*sf);
            eprintln!("[y27-probe-p1] MISSING source_face={:?} tris_in_survival={}", sf, tris.len());
        }
    }
    eprintln!("[y27-probe-p1-summary] survival_groups={} arena_faces={} unique_arena_sources={} missing_count={}",
        survival.groups.len(), face_provenance.len(), arena_sources.len(), missing_sources.len());

    // P2: drop-site attribution summaries (B.1, B.5, B.6, B.7) and per-source detail lines
    // [omitted for brevity — full text in /tmp/y27-probe-wt diff]
}
```

The probe does NOT mutate any state. It is `eprintln!`-only. Lives only in worktree; not committed to the live tree.

---

## §4 Verdict against plan acceptance gate

Plan §"Phase 0 canary" → "Acceptance gate":

| Outcome | Plan-defined verdict | Observation | Result |
|---|---|---|---|
| One drop site fires for ≥75% of missing SourceFaces | DOMINANT — spec proceeds | **`missing_count=0` across all 8 invocations** — the metric "missing SourceFaces" is empty by construction; no drop site can hit any percentage | not satisfied |
| Two sites each ≥25% | LAYERED — spec picks simpler, banks other | same | not satisfied |
| All sites <25% | UNCLASSIFIED — investigate before specing | same | (vacuously) not satisfied |
| Drop site is something not in B.1-B.7/A.4/C.4 | **NEW HYPOTHESIS — document and bring to user** | **The dropout is downstream of `flood_fill_patches`, in `tessellate_solid_bounded`'s per-face dispatch loop** (F0020 loses 8 of 33 arena faces; F0044/F0045 lose 0 yet have 12/38 unpaired; R0092 has 44 legit NMM ≈ 43 render unpaired); the "missing patches at flood_fill" hypothesis from PR-Y26 §4.1 (m) is REFUTED at the SourceFace granularity; the cohort defect is at the **per-face render-LOD tessellation** layer | **MET** |

### Critically — what the canary refutes vs. what it surfaces

**REFUTED:**
- PR-Y26 §4.1 candidate (m) — "surface-patch dropout in `flood_fill_patches`": MEASURED, `missing_count=0` across 8 invocations of 4 cases.
- B.1 as cohort-dominant cause: it fires across all cases but doesn't cause SourceFace dropout (`missing_count=0`).
- B.5/B.6 as cohort-dominant cause: they fire only on F0020 (and on R0092 invocation 1 of 3, but R0092's load-bearing final invocation is clean).
- B.7 as cohort-dominant cause: fires sporadically (F0045 invocation 4, 2 zero-loop patches) but doesn't cause SourceFace dropout (other patches for the same SourceFace cover it).
- The hypothesis that "the 36 unpaired in F0020 = 3 missing surface patches at flood_fill exit": empirically, the flood_fill arena has 33 faces with all 20 surviving SourceFaces represented; 8 of those 33 are then DROPPED by `tessellate_solid_bounded`.

**SURFACED (not in plan, requires user scope decision):**
- **D.1** `tessellate_solid_bounded` per-face dispatch L4283-4290: emits no `FaceRange` for faces whose tessellator produces zero indices. F0020 loses 8 of 33; R0092 loses 1 of 24; F0044/F0045 lose 0. (NOT a cohort-uniform mechanism.)
- **D.2** `tessellate_solid_bounded` per-face seam mismatch: F0044 + F0045 have CLEAN arena exits AND no missing render face_ids, yet 12 + 38 unpaired in render. This means the per-face tessellator is producing edges whose POSITIONS don't match across face boundaries even when the arena topology is correct. PR-Y26 (i)/(j) attempted to capture this and was REFUTED via count=2 absence — but this could be because the position deltas ARE LARGER than the watertight oracle's quantization grid (so each side keys to its own count=1 edge).
- **D.3** R0092 NMM-edge tessellation: arena has 44 legit non-manifold half-edges; render has 43 unpaired. The 44≈43 correspondence suggests the per-face tessellator is producing one count=1 edge per arena NMM HE — i.e., **per-face tessellation does not handle NMM edges per Yang §4.4.2's requirement** ("watertightness inherited from the mesh Boolean output").

The PR-Y26 finding "3 connected components of count=1 edges in F0020" matched the count of "8 missing faces in render" for F0020 imperfectly (PR-Y26 found 3 components: 3-cycle, 16-vert bowtie, 9-vert chain). 8 missing faces likely corresponds to a more complex topology than 3 component shapes, but the missing-face mechanism IS consistent with "boundaries of missing face-shaped patches" forming connected components.

---

## §5 Recommendation to user (scope decision)

PR-Y27 cannot proceed as scoped. The plan's 7-candidate fix-shape menu (B.1-fix, B.4-fix, B.5-fix, B.6-fix, B.7-fix, A.4-fix, C.4-fix) is built on the assumption that `flood_fill_patches` is dropping surface patches. **It is not.**

Two structurally-different recommendations:

**(R1) — Re-anchor PR-Y27 on `tessellate_solid_bounded` (the empirically dominant site).** A new canary phase would probe:
- Why does F0020 lose 8 of 33 arena faces in render? Likely candidate: `collect_loop_boundary` returns < 3 vertices for those faces because B.5/B.6 open-chain wraparound produces phantom edges whose endpoints quantize away.
- Why do F0044/F0045 produce count=1 unpaired across face boundaries even with clean arena topology? Likely candidate: the per-face tessellator (planar earcut, cylindrical strip, etc.) emits boundary vertices at positions that quantize differently from what the adjacent face emits at the SAME geometric edge. This is what the PR-Y26 plan's (i) hypothesis WAS — but the watertight oracle's quantization granularity is too coarse to detect "two count=1s that should be one count=2".
- Why does R0092 have 44 NMM ≈ 43 unpaired? Likely candidate: per-face tessellator emits one triangle per face along an NMM edge, producing duplicate boundary edges that don't pair as twins (since they belong to DIFFERENT faces in the render mesh's per-face-id partitioning).

**(R2) — Cohort-split PR-Y27.** The data clearly shows different mechanisms for F0020 vs F0044/F0045 vs R0092. A single fix shape is unlikely to address all 4. Splitting into:
- **PR-Y27a:** F0020 missing-render-face mechanism (8 faces lost; mixed with B.5/B.6 open-chain residuals);
- **PR-Y27b:** F0044/F0045 per-face seam-mismatch on closed arena topology;
- **PR-Y27c:** R0092 NMM-edge per-face tessellation per Yang §4.4.2.

The user should choose between R1 (single re-anchored canary) and R2 (cohort-split) before any spec-y27 is drafted. **Drafting a spec without this scope decision would repeat PR-Y25/Y26's failure mode** — taking inferred-but-not-measured fix shapes into production-code phases.

### Cohort-guard hardening (still applies regardless of scope decision)

Per `pr_y26_abort.md` §4.4: PR-Y22+PR-Y24 cohort guards measure topology-layer metrics only (`[topo-extract]`, `[twin-oracle]`). The render-mesh `check_watertight_mesh` was never asserted on the cohort. Whichever PR replaces PR-Y27 should add a render-mesh watertight cohort regression test using the actual baselines (F0044=12, F0045=38, R0092=43, F0020=36) so future changes can't silently regress this layer.

This canary's render-mesh face-count differential (33→25 for F0020, etc.) is a **second cohort-guard signal** that should also be hardened: assert that `arena.faces.len() == render_mesh.distinct_face_ids().count()` for the cohort cases, so if the per-face tessellator drops faces the regression test fails immediately.

---

## §6 Banked findings

- **F0020 R3-ownership-strip + soft-break open-chain residuals (B.5+B.6):** these DO produce 39 NMM half-edges in the arena (vs F0044's 0), but they are NOT the cause of `missing_count=0` — they are correctly recovered by the validator. The downstream effect (per-face tessellation of phantom-edge boundaries) is a separate concern. Worth investigating if R1 is chosen.
- **B.1 canon-degen filter:** consistently fires across all cohort cases (6, 5, 10, 8, 20, 8 sub-tris dropped per invocation). Per the comment at L470-481 and `docs/audits/pr_y22_mode_a_missing_canary.md`, these are quantization-induced collapses introduced by `canon_v` in `flood_fill_patches`. They are NOT a defect (the comment cites Cherchi §4 well-formed-simplicial-complex preservation). Confirmed empirically irrelevant to the SourceFace dropout question.
- **F0045 invocation 4 B.7=2:** two zero-loop patches (sources A:1 and B:2) dropped. But missing_count=0 because OTHER patches for the same SourceFace exist with non-zero loops. This is the per-source-face split working as intended at the SourceFace level; the per-PATCH B.7 skip is hidden. Not a defect at the SourceFace level (though potentially still significant at the per-patch level — out of scope for this canary).
- **R0092 invocation 1 B.5+B.6 firings (5 patches all source B:0):** does NOT propagate to invocation 3 (final, used for render). This case's load-bearing flood_fill output has clean arena AND legit-NMM-only twin=None. R0092 is the strongest evidence that "B.5/B.6 in flood_fill" is NOT the cohort-uniform mechanism — final invocations are clean.
- **PR-Y26's `count=1` connected-component analysis** (3-cycle, 16-vert bowtie, 9-vert chain) for F0020 should be re-correlated against the 8 missing render faces, not against the 9 B.5+B.6 source faces. The 3-cycle is a 1-triangle missing patch (consistent with one missing face); the bowtie + 9-vert chain may correspond to 7 more missing faces collectively (two adjacent missing faces each share a boundary, producing a more complex graph). Worth a follow-up measurement at `tessellate_solid_bounded` exit.
- **The watertight oracle's quantization granularity** (`max_abs * TAU_TESS_GRID_FACTOR`, floor `TAU_TESS_GRID_MIN`) sets a lower bound on detectable seam-position deltas. F0044/F0045 producing count=1 (NOT count=2) when their arenas are clean MAY mean position deltas exceed this granularity. PR-Y26's REFUTATION of (i) was based on "0 of 36 are count=2" — but that was a side-of-the-quant check, not "0 pairs of count=1s have similar-but-not-equal positions." A re-investigation should use a finer-grained position match (e.g., per-edge centroid distance in 3D) before declaring the per-face seam-mismatch hypothesis dead.

---

## §7 Reproduction artifacts

- Probe stdout F0020 (P1+P2+P4): `/tmp/y27-canary-f0020-p14.txt`
- Probe stdout F0044 cohort: `/tmp/y27-canary-f0044-full.txt` (≈37KB, full trace)
- Stage dump F0020: `/tmp/y27-stage-dump-f0020/F0020/` (12 OBJ + 12 CSV per stage)
- Stage dump cohort: `/tmp/y27-stage-dump-f0044/{F0044,F0045,R0092}/`
- Probe instrumentation diff: `/tmp/y27-probe-wt` worktree, 191 lines additive in `topology_extract.rs`, 297 total diff lines
- Will be discarded by `git worktree remove /tmp/y27-probe-wt` at close-out

End of memo.
