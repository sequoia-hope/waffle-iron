# PR-Y28 Anchor Canary — BROAD INVESTIGATION: F0020 D.1 splits into FOUR sub-mechanisms; recommend ABORT pending PR-Y29 inverse-direction probe

**Author:** canary-y28
**Date:** 2026-05-08
**Baseline:** `6901342` (post-PR-Y27 ABORT)
**Mandate:** Open-ended investigation per `/home/claude/.claude/plans/optimized-wandering-wind.md` (meta-process inversion: canary first, spec around findings)
**Verdict:** **ABORT recommended** at canary phase. Investigation produced rich empirical clustering but **does NOT empirically map a single fix shape to F0020 watertight `unpaired_count` 36 → 0**. Recommending a fix shape without that mapping would repeat PR-Y25/Y26/Y27's failure mode (structural inference, refuted at canary phase). The honest call is ABORT with refined PR-Y29 scope.

This memo names empirical evidence only. The would-have-been recommendation (a "Cherchi §3 closed-loop invariant filter") is presented at the end as **banked next-PR seed**, not as proposed fix shape, because the empirical chain "fix this anchor → watertight=0" is unverified.

---

## §0 Discipline — live tree untouched

### Live tree at session start and just before writing this memo

```
$ git -C /home/claude/workspace status
On branch main
Your branch is up to date with 'origin/main'.
nothing to commit, working tree clean

$ git -C /home/claude/workspace log --oneline -3 main
6901342 audit(yang-pr-y27): ABORT at canary phase — flood_fill_patches drops zero SourceFaces; cohort splits into 3 mechanisms in tessellate_solid_bounded
410da9a audit(yang-pr-y27-canary): NEW HYPOTHESIS — flood_fill_patches drops zero SourceFaces; defect is downstream in tessellate_solid_bounded
45e3b5c audit(yang-pr-y26): ABORT at canary phase — all three plan candidates refuted; defect is missing triangles, not seam misalignment
```

All probe instrumentation lives in a separate worktree at `/tmp/y28-probe-wt` rooted at `6901342`:

```
$ git worktree add /tmp/y28-probe-wt 6901342
Preparing worktree (detached HEAD 6901342)
HEAD is now at 6901342 audit(yang-pr-y27): ABORT at canary phase ...

$ cd /tmp/y28-probe-wt && git diff --stat
 app/tests/cases/assay/results.json    |   6 +-
 crates/kernel/src/tessellation/mod.rs | 141 +++++++++++++++++++++++++++++++++-
 2 files changed, 143 insertions(+), 4 deletions(-)

$ cd /tmp/y28-probe-wt && git diff crates/kernel/src/tessellation/mod.rs | wc -l
218
```

No `git stash`, `git checkout --`, `git reset --hard`, or other destructive op was used on the live working tree. Per `feedback_adversary_no_destructive_git.md`. (`results.json` mutation is the assay test runner's normal artifact; will be discarded with `git worktree remove`.)

### Probe gate

Every probe is gated on `std::env::var("Y28_PROBE").as_deref() == Ok("1")`. Default-off codepath is byte-identical to the `6901342` baseline. All output is `eprintln!`-only. No mutation of `disc.positions`, `vertices`, `indices`, `face_ranges`, `face_provenance`, or arena state.

### Reproduction commands

```
git worktree add /tmp/y28-probe-wt 6901342
cd /tmp/y28-probe-wt
# (probes injected per §3 below)

# Step 1 — visual stage dump (PR-VIZ-1) for cross-validation of per-stage drop counts
mkdir -p /tmp/y28-stage-dump-f0020
YANG_BOOLEAN=1 YANG_CONFORMAL_PROBE=1 YANG_STAGE_DUMP=/tmp/y28-stage-dump-f0020 Y28_PROBE=1 \
    cargo test -p test-harness --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y28-canary-f0020-stagedump.txt 2>&1

# Step 2 — per-face dispatch enumeration with positions
YANG_BOOLEAN=1 Y28_PROBE=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0020 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y28-canary-f0020-v2.txt 2>&1

# Step 3 — cohort cross-check
YANG_BOOLEAN=1 Y28_PROBE=1 cargo test -p test-harness \
    --test assay_randomized -- spotlight_f0044 \
    --ignored --nocapture --test-threads=1 \
    > /tmp/y28-canary-f0044.txt 2>&1
```

Both spotlight tests reported `test result: ok. 1 passed; 0 failed`. Inner spotlight verdicts unchanged from `6901342` baseline (F0020 + cohort `Status: Failed`).

---

## §1 F0020 — exhaustive per-face inventory (final/load-bearing invocation, 33 arena faces)

The F0020 spotlight produces six `tessellate_solid_bounded` invocations across the three sequential extrudes. The load-bearing one is the **sixth (final) invocation**: 33 arena faces; 65 edges; 80 arena vertices. Probe trace verbatim summary:

```
[y28-probe-summary] total_faces=6  face_ranges_pushed=6  missing_render_faces=0   (b#1 boolean LOD)
[y28-probe-summary] total_faces=6  face_ranges_pushed=6  missing_render_faces=0   (b#1 render LOD)
[y28-probe-summary] total_faces=26 face_ranges_pushed=15 missing_render_faces=11  (b#2 boolean LOD)
[y28-probe-summary] total_faces=26 face_ranges_pushed=15 missing_render_faces=11  (b#2 render LOD)
[y28-probe-summary] total_faces=6  face_ranges_pushed=6  missing_render_faces=0   (b#3 boolean LOD)
[y28-probe-summary] total_faces=33 face_ranges_pushed=28 missing_render_faces=5   (b#3 render LOD ← LOAD-BEARING)
```

**At dispatch-loop exit, F0020 final has 5 of 33 face_ranges missing.** Per the cross-validated PR-VIZ-1 stage dump, three more drop in repair passes:

| Pipeline stage | distinct face_ids | tri_count | Drop from prev |
|---|---|---|---|
| F.0 (dispatch exit, pre-repair) | 28 | 107 | (5 missing already, from dispatch) |
| F.1 (after `remove_winding_insensitive_duplicates`) | 26 | 77 | **-2 faces, -30 triangles** |
| F.2 (after `remove_nonmanifold_topology_aware`) | 25 | 79 | **-1 face** (note: +2 tri from `flip_nonmanifold_interior_diagonals` + `retessellate_nonmanifold_faces_with_steiner_fan`) |
| F.3 (after `remove_nonmanifold_duplicates_aggressive`) | 25 | 76 | -3 triangles |
| F.4 (after `weld_smooth_vertices`, FINAL) | 25 | 76 | unchanged |

**Total: 33 arena → 25 render = 8 missing.** The 8 split as **5 lost in dispatch loop + 2 lost at F.0→F.1 + 1 lost at F.1→F.2**. PR-Y27's "8 missing render faces" finding is **cross-validated and reproduced**, and now decomposed.

### §1.1 The 5 dispatch-loop dropouts (sub-cluster D.1a + D.1b)

Verbatim trace of the 5 faces with `face_range_pushed=false`:

```
[y28-probe-face] kid=199 face_idx=7  geom=Planar outer_he_count=2  outer_nmm_count=2  is_self_loop=false outer_boundary_len=2 inner_loop_count=0 inner_sizes=[] unique_edges=2
[y28-probe-dispatch] kid=199 arm=Planar outer_boundary_len=2 kept_inner_count=0 planar_lt3_gate=true
[y28-probe-exit] kid=199 face_idx=7 indices_emitted=0 face_range_pushed=false

[y28-probe-face] kid=217 face_idx=25 geom=Planar outer_he_count=1 outer_nmm_count=0 is_self_loop=true  outer_boundary_len=2 inner_loop_count=0 inner_sizes=[] unique_edges=1
[y28-probe-dispatch] kid=217 arm=Planar outer_boundary_len=2 kept_inner_count=0 planar_lt3_gate=true
[y28-probe-exit] kid=217 face_idx=25 indices_emitted=0 face_range_pushed=false

[y28-probe-face] kid=220 face_idx=28 geom=Planar outer_he_count=1 outer_nmm_count=0 is_self_loop=true  outer_boundary_len=2 inner_loop_count=0 inner_sizes=[] unique_edges=1
[y28-probe-dispatch] kid=220 arm=Planar outer_boundary_len=2 kept_inner_count=0 planar_lt3_gate=true
[y28-probe-exit] kid=220 face_idx=28 indices_emitted=0 face_range_pushed=false

[y28-probe-face] kid=223 face_idx=31 geom=Planar outer_he_count=1 outer_nmm_count=0 is_self_loop=true  outer_boundary_len=2 inner_loop_count=0 inner_sizes=[] unique_edges=1
[y28-probe-dispatch] kid=223 arm=Planar outer_boundary_len=2 kept_inner_count=0 planar_lt3_gate=true
[y28-probe-exit] kid=223 face_idx=31 indices_emitted=0 face_range_pushed=false

[y28-probe-face] kid=221 face_idx=29 geom=Planar outer_he_count=3 outer_nmm_count=0 is_self_loop=false outer_boundary_len=3 inner_loop_count=0 inner_sizes=[] unique_edges=3
[y28-probe-dispatch] kid=221 arm=Planar outer_boundary_len=3 kept_inner_count=0 planar_lt3_gate=false
[y28-probe-pos] kid=221 b_idx=0 v_idx=58  pos=(-1.421785e-1,1.221606e-1,1.208199e-1) plane_n=(7.866565e-2,3.641584e-1,9.280088e-1)
[y28-probe-pos] kid=221 b_idx=1 v_idx=74  pos=(-1.421785e-1,1.221606e-1,1.208199e-1) plane_n=(7.866565e-2,3.641584e-1,9.280088e-1)
[y28-probe-pos] kid=221 b_idx=2 v_idx=121 pos=(-1.871865e-1,-8.619017e-2,2.063937e-1) plane_n=(7.866565e-2,3.641584e-1,9.280088e-1)
[y28-probe-planar] ENTRY boundary_len=3 inner_count=0
[y28-probe-planar-no-holes] n=3 is_convex=false has_collinear=false reverse_outer=false
[y28-probe-planar-branch] EARCUT_NO_HOLES coords_2d_len=6 status=Ok(len=0)
[y28-probe-exit] kid=221 face_idx=29 indices_emitted=0 face_range_pushed=false
```

**D.1a (4 of 5) — `boundary.len() < 3` gate at `tessellation/mod.rs:3319`** (planar entry guard):
- kid=199 face_idx=7: `outer_he_count=2 outer_nmm_count=2` — a 2-HE closed cycle, **both NMM**. The two HEs traverse the same geometric edge in opposite directions (a "2-cycle" loop). `collect_loop_boundary` walks them and emits 2 positions.
- kid=217, kid=220, kid=223 (face_idx 25, 28, 31): `outer_he_count=1 outer_nmm_count=0 is_self_loop=true` — a single half-edge whose `next` points back to itself. The `collect_loop_boundary` self-loop branch (mod.rs:3260-3266) emits both endpoints of the underlying linear edge → boundary_len=2.

These 4 patches are **structurally invalid Yang patches** per:
- **Yang 2025 §4.4.2 line 588-590 verbatim**: *"Our algorithm segments the mesh Boolean results into patches along the boundary curves... Starting from an inner triangle, i.e. not on the boundaries of each mesh patch, using it as a seed triangle for the patch, our algorithm expands the patch by including more neighboring inner triangles, until all the neighboring triangles of the patch are on the boundaries."* — A valid Yang patch has at least one inner triangle; these patches have NO inner triangle (the boundary is a 1- or 2-HE chain, geometrically a single edge).
- **Cherchi 2022 §3 line 253-256 verbatim**: *"the arrangement is guaranteed to be a well formed simplicial complex and surface patches are bounded by closed loops of non-manifold edges, namely the intersection lines."* — A 1-edge or 2-HE-cycle is not a closed loop in the topological sense (no enclosed area).

**D.1b (1 of 5) — degenerate-triangle earcut zero-emit at `tessellation/mod.rs:3444`**:
- kid=221 face_idx=29: 3 HEs, 3 unique edges, all twin paired (0 NMM), `outer_boundary_len=3`. Passes the `boundary.len() < 3` gate. Dispatches to `tessellate_planar_face_bounded`.
- The 3 positions are: `b_idx=0 = b_idx=1` (vertex 58 = vertex 74 at the SAME 3D position `(-0.14217, 0.12216, 0.12082)` to f64 precision); `b_idx=2` at a different position.
- `is_convex=false` (because cross product is near-zero for a degenerate triangle, dot-with-normal can be < 0).
- `has_collinear` is gated on `n >= 4`, so it's false for n=3.
- Falls to earcut: `earcutr::earcut(coords_2d, &[], 2)` returns `Ok(vec![])` — empty triangulation for the degenerate triangle.

This patch is **invalid per Yang §4.4.2 + Cherchi §3** (well-formed simplicial complex requires non-zero-area patches), AND per the canon-degen filter intent at `topology_extract.rs:468-491` (the filter at Step 4 drops sub-triangles whose canonical vertices collide, but does NOT prune downstream patch emission when the SAME collision exists at the patch loop level).

### §1.2 The 3 repair-pass dropouts (sub-cluster D.1c + D.1d)

Faces lost between F-stages, identified by stage-dump label CSV diff:

```
$ comm -23 <(cut -d, -f2 stage_F.0_labels.csv) <(cut -d, -f2 stage_F.1_labels.csv) | sort -u
200
207
$ comm -23 <(cut -d, -f2 stage_F.1_labels.csv) <(cut -d, -f2 stage_F.2_labels.csv) | sort -u
218
```

Dispatch-loop trace for these:

```
[y28-probe-face] kid=200 face_idx=8  geom=Planar outer_he_count=12 outer_nmm_count=12 is_self_loop=false outer_boundary_len=12 inner_loop_count=0 inner_sizes=[] unique_edges=12
[y28-probe-dispatch] kid=200 arm=Planar outer_boundary_len=12 kept_inner_count=0 planar_lt3_gate=false
[y28-probe-exit] kid=200 face_idx=8 indices_emitted=36 face_range_pushed=true

[y28-probe-face] kid=207 face_idx=15 geom=Planar outer_he_count=4  outer_nmm_count=4  is_self_loop=false outer_boundary_len=4  inner_loop_count=0 inner_sizes=[] unique_edges=4
[y28-probe-dispatch] kid=207 arm=Planar outer_boundary_len=4 kept_inner_count=0 planar_lt3_gate=false
[y28-probe-exit] kid=207 face_idx=15 indices_emitted=12 face_range_pushed=true

[y28-probe-face] kid=218 face_idx=26 geom=Planar outer_he_count=3  outer_nmm_count=0  is_self_loop=false outer_boundary_len=3 inner_loop_count=0 inner_sizes=[] unique_edges=3
[y28-probe-dispatch] kid=218 arm=Planar outer_boundary_len=3 kept_inner_count=0 planar_lt3_gate=false
[y28-probe-exit] kid=218 face_idx=26 indices_emitted=3 face_range_pushed=true
```

**D.1c (2 of 3) — `remove_winding_insensitive_duplicates` at `tessellation/repair.rs:502-574`**:
- kid=200 face_idx=8: 12 HEs, **ALL 12 NMM (twin=None in the arena)**, emits 36 indices (12 triangles).
- kid=207 face_idx=15: 4 HEs, **ALL 4 NMM**, emits 12 indices (4 triangles).
- Both have 100% NMM boundary signatures. The repair pass at F.0→F.1 quantizes triangle vertex sets (sorted, winding-insensitive) and keeps one of each duplicate; these faces' triangles' canonical quantized keys match a different kept face's triangles → all their triangles are dropped → no FaceRange retained.

The 100% NMM signature is **Cherchi 2022 §3 line 253-256 contract violated**: an intersection-edge-bounded patch should be paired with a peer patch on the opposite side of the intersection curve (Cherchi §3: "an output triangle can belong to many input meshes, for example in the case of meshes that overlap at a coplanar region"). For kid=200, ALL 12 boundary HEs lack a twin in the arena — meaning the algorithm registered this patch but did NOT pair its peer patch on the other side of the intersection curves.

**D.1d (1 of 3) — `remove_nonmanifold_topology_aware` at `tessellation/repair.rs:585`**:
- kid=218 face_idx=26: 3 HEs, 0 NMM, emits 3 indices (1 triangle). Lost at F.1→F.2.
- The topology-aware repair "uses B-Rep edge→face relationships to determine which two faces should share each boundary edge. Removes triangles whose face_id doesn't match the expected topology." This single triangle's face_id was deemed incorrect.

### §1.3 Final cluster table

| Sub-cluster | Mechanism | face_idx count | Indices emitted at F.0 | Paper-cited invariant violated |
|---|---|---|---|---|
| **D.1a** | `boundary.len() < 3` gate at planar entry (n=1 self-loop OR n=2 cycle) | 4 (7, 25, 28, 31) | 0 | Yang §4.4.2 "patch has inner triangles"; Cherchi §3 "closed loops" |
| **D.1b** | degenerate-triangle earcut zero-emit (two of three positions coincident) | 1 (29) | 0 | Yang §4.4.2 well-formedness; canon-degen filter (`topology_extract.rs:468-491`) didn't trigger at patch level |
| **D.1c** | `remove_winding_insensitive_duplicates`: 100% NMM boundary patches whose triangles match another kept face's tris | 2 (8, 15) | 36+12 = 48 | Cherchi §3 "peer patch on opposite side" not paired; Yang §4.4.2 "each edge shared by two adjacent faces" |
| **D.1d** | `remove_nonmanifold_topology_aware`: face_id mismatch for single triangle | 1 (26) | 3 | (downstream consequence; not a paper-violation root) |

---

## §2 Cross-check with PR-Y26's 36 unpaired count=1 edges

PR-Y26 §1 measured F0020 b#2 final render-mesh watertight: 36 unpaired edges. 34 are `count=1` (only one incident triangle); 2 are `count=3` zero-length-quantized degenerate. PR-Y26 §1 line 117-127 identified 3 connected components of the count=1 edges:

```
comp 0:  3 vertices,  3 edges  — closed 3-cycle (a triangular hole)
comp 1: 16 vertices, 20 edges  — degree-{2:12, 4:4} (two cycles meeting at 4 nodes; figure-eight / merged-loop)
comp 2:  9 vertices, 10 edges  — degree-{1:1, 2:7, 5:1} (open chain with one branch / star)
```

### §2.1 Edge-count accounting (NOT a closed proof)

Sum of boundary HEs across the 8 missing F0020 faces:

| face_idx | boundary HEs (directed) |
|---|---|
| 7  | 2  |
| 8  | 12 |
| 15 | 4  |
| 25 | 1 (self-loop) |
| 26 | 3 |
| 28 | 1 (self-loop) |
| 29 | 3 |
| 31 | 1 (self-loop) |
| **Total** | **27** directed HEs |

PR-Y26 reports 33 non-degenerate count=1 edges (3+20+10), each of which is one *undirected* render-mesh edge with exactly one incident triangle. The accounting **does not match cleanly** — 27 ≠ 33 — but the **shape match is suggestive**:

- Comp 0 (3-cycle, 3 edges) ≈ kid=218 (single removed triangle, 3 edges) OR kid=221 (degenerate 3-edge boundary, 3 edges) — both are 3-edge-boundary missing patches.
- Comp 2 (9-vert chain, 10 edges) ≈ a mix of kid=200 (12 NMM) + small contributions from D.1a self-loops, with shared corner vertices reducing count.
- Comp 1 (16-vert bowtie, 20 edges) ≈ kid=200 (12) + kid=207 (4) plus shared corners with adjacent kept faces.

The unmatched 6 directed HEs (33 - 27) most likely come from the **adjacent kept faces' boundary edges that pair with the missing faces' boundaries** — each such kept-face edge appears as count=1 because its expected paired neighbor is missing.

### §2.2 CRITICAL UNKNOWN — fix-shape verification is not closed

I have NOT empirically verified that any specific fix would reduce `unpaired_count=36 → 0`. Specifically:

- **If only D.1a + D.1b are fixed (upstream rejection of malformed patches)**: D.1a/b dropouts emit ZERO triangles, so removing them removes ZERO render-mesh triangles → render-mesh edge graph is unchanged → `unpaired_count` is **unchanged at 36**. Arena vs render parity improves (33 → 28 vs 25 still has 3 missing), but watertight does not.
- **If D.1c is fixed (peer-patch pairing at flood_fill_patches Step 6)**: requires synthesizing a peer patch on the opposite side of each all-NMM boundary's intersection curves. This is **structural inference**, not measurement. I cannot confidently predict whether peer-patch synthesis emits the missing twin triangles that would close the unpaired edges.
- **If D.1d is fixed (preserve the single triangle from `remove_nonmanifold_topology_aware`)**: closes at most 3 count=1 edges.

**None of D.1a, D.1b, D.1c, or D.1d alone has a measured chain from "fix anchor X → watertight=0".**

---

## §3 Cohort cross-check — D.1 is F0020-specific

Verbatim cohort summaries (F0044 batch via `spotlight_f0044`):

```
[y28-probe-summary] total_faces=8  face_ranges_pushed=8  missing_render_faces=0   (F0044)
[y28-probe-summary] total_faces=8  face_ranges_pushed=8  missing_render_faces=0   (F0045)
[y28-probe-summary] total_faces=24 face_ranges_pushed=24 missing_render_faces=0   (R0092)
```

**Zero dispatch-loop dropouts on F0044, F0045, R0092**. Each cohort case still has its 12 / 38 / 43 unpaired count=1 edges (per PR-Y26 §2), but the **mechanism is different from F0020's D.1**. The PR-Y27 cohort split (D.1 F0020 / D.2 F0044+F0045 / D.3 R0092) is now reconfirmed at finer resolution:

- **D.1 (F0020-only)**: 8 missing render face_ranges from 4 sub-mechanisms (D.1a-d).
- **D.2 (F0044+F0045)**: 0 missing face_ranges; sub-quantization seam mismatch in per-face render-LOD tessellation.
- **D.3 (R0092)**: 0 missing face_ranges; NMM-edge tessellation gap (44 arena NMM ≈ 43 unpaired).

F0020 is the **only** cohort case with B.5/B.6 firings in `flood_fill_patches` (PR-Y27 §1 line 96: F0020 b#2 had `B5_R3_owner_wins_total=25 B6_open_chain_patches=16`; F0044/F0045/R0092 final invocations had zero). The D.1a/D.1b malformed-patch signatures **correlate with the R3-ownership-strip + open-chain residuals** introduced by PR-Y19's `flood_fill_patches::Step 6` boundary classification.

---

## §4 Why I am recommending ABORT, not a fix shape

The PR-Y28 plan asks for a SINGLE fix shape with paper citation, LOC budget, and predicted F0020 watertight outcome. I can produce candidates, but **none has an empirical chain from anchor to `unpaired_count=0`**:

| Candidate fix shape | Anchor (file:line) | Paper citation | LOC est. | F0020 watertight prediction | Empirical chain measured? |
|---|---|---|---|---|---|
| **(α)** Pre-emit closed-loop filter at flood_fill_patches::Step 7 | `topology_extract.rs:1119-1130` | Cherchi §3 line 253-256 "closed loops"; Yang §4.4.2 "inner triangle seed" | ~20 | **unchanged at 36** (D.1a/D.1b emit zero triangles; removing them removes zero render geometry) | NO — accounting argues it cannot reduce unpaired |
| **(β)** Peer-patch synthesis for all-NMM patches at flood_fill_patches::Step 6 | `topology_extract.rs:~745-969` (patch_boundaries construction + Step 6 R3 ownership) | Cherchi §3 "each edge shared by two adjacent faces" / Yang §4.4.2 "boundary curves correspond to intersection curves" | ~150-300 (large architectural change) | **unknown** — requires verifying that synthesized peer's triangles fall in the missing-twin-triangle positions PR-Y26 identified | NO — structural inference only |
| **(γ)** Pre-quantization-dedup conformal-merge at remove_winding_insensitive_duplicates | `tessellation/repair.rs:502-574` | (downstream patch; no paper citation; works against Yang's "B-Rep face per patch" invariant) | ~50 | **unknown** — kept-face vs dropped-face SourceFace identity is unmeasured | NO — would silently merge two distinct B-Rep faces |
| **(δ)** Canon-degen filter at patch-level (extension of `topology_extract.rs:468-491`) | `topology_extract.rs:~470` extended to whole-patch level | Yang §4.4.2 well-formedness | ~30 | **unchanged at 36** (same accounting as α — pruned patches emit zero triangles) | NO |

**Adopting any of α/β/γ/δ without measurement would repeat PR-Y25/Y26/Y27's failure mode** — three consecutive ABORTs where Phase 1 Explore agents ranked structurally-plausible fix shapes that the canary refuted. The whole point of inverting the plan→canary pattern in PR-Y28 was to NOT make this kind of inference-driven commitment.

**The honest empirical state**:
- The 8 missing faces are mechanically real and now precisely classified.
- The 36 PR-Y26 unpaired count=1 edges are mechanically real.
- The chain from "the 8 missing faces" to "the 36 unpaired edges" is **suggestive (D.1's 27 directed boundary HEs roughly match PR-Y26's 33 undirected count=1 edges in shape if not in count)** but **not empirically closed**.

To make the recommendation a fix shape, I would need ONE more probe pass: empirically map each PR-Y26 count=1 edge to its missing twin-triangle's would-be SourceFace. If the missing twin's SourceFace consistently lands in the 8-face D.1 set, the chain is closed and one of α/β/γ/δ has an empirical anchor. If it doesn't, the count=1 mechanism has a different upstream root that this canary did not surface.

That probe is feasible — instrument the render-mesh-watertight check at `tessellation/mod.rs:4420` after F.4, build the position-quantized edge→tri map, identify count=1 edges, and back-walk to face_provenance. It is a 30-60 minute additional investigation. The Y28 plan does not budget for an "extended canary phase" but the team-lead's brief allows ABORT recommendation with refined PR-Y29 scope.

---

## §5 ABORT recommendation + refined PR-Y29 scope

**ABORT PR-Y28.** Refined PR-Y29 scope:

### §5.1 PR-Y29 Phase 0 canary — inverse-direction probe

Build on PR-Y28's empirical foundation. The 8 missing F0020 face_idx values are now known (7, 8, 15, 25, 26, 28, 29, 31). The 36 PR-Y26 count=1 unpaired-edge positions are derivable from the final render mesh.

The canary mandate: **for each PR-Y26 count=1 edge, identify the missing-twin-triangle's would-be SourceFace**. Concretely:

1. Instrument `tessellate_solid_bounded` exit (after F.4 weld) to compute the position-quantized edge→tri-list map matching `oracle.rs::check_watertight_mesh`.
2. For each unpaired edge `e`: identify the LONE incident triangle `t_kept`, its `kid_kept`, and its `face_idx_kept`.
3. Reconstruct the missing twin's would-be vertex positions (i.e., the two endpoints of `e` plus a third vertex on the OPPOSITE side of `e` from `t_kept`). The third vertex's expected face SOMEWHERE in the arena is what we want to identify.
4. Check whether the position of the missing third vertex lies in any of the 8 missing faces' boundary loops (D.1a/b/c/d). If yes — the cross-mapping closes and a fix-shape candidate from α/β has empirical backing.
5. If NO — the count=1 mechanism is **NOT** caused by the 8 missing F0020 faces, and PR-Y29 must explore a different upstream layer.

**Acceptance gate for PR-Y29 canary**:
- If ≥80% of unpaired edges' missing twins land in the 8 D.1 faces → **fix shape β (peer-patch synthesis) is the rightful anchor**; PR-Y29 specs against `topology_extract.rs:745-969`.
- If ≥25% but <80% → LAYERED; PR-Y29 picks the dominant.
- If <25% → NEW HYPOTHESIS (5th refutation in a row). Reconsider the entire D.1/D.2/D.3 split.

### §5.2 Banked findings (carry forward)

- **D.1 sub-mechanism classification**: empirical, cross-validated, ready for PR-Y29 to consume verbatim.
- **D.1a + D.1b are Yang/Cherchi-invariant violations** at the patch-emission boundary. A simple `topology_extract.rs:1119-1130` pre-filter (~20 LOC) is mechanically sound and cleans up bookkeeping (33→28 arena faces) but does NOT close watertight. PR-Y30+ could ship this as a hygiene PR independent of D.1c/d fixes, gated on a regression test that asserts arena-vs-pre-repair face parity.
- **D.1c (100% NMM boundary patches)** is the dominant cluster by emitted-triangle-count (48 of 51 triangles emitted-then-dropped). Its mechanism — patches whose every boundary HE has no peer — is **structurally** Cherchi §3-incompatible. The fix is upstream of `tessellate_solid_bounded`; β is the rightful anchor in `flood_fill_patches` Step 6.
- **PR-Y26 connected-component shapes (3-cycle + 16-vert bowtie + 9-vert chain)** are now correlated structurally with D.1 face cardinality (8 missing) but the cross-mapping is **not bijective by my measurement**. The "missing patches → connected components of count=1 edges" hypothesis from PR-Y26 §1 NEW HYPOTHESIS section is **supported but not closed**.
- **F0020 is the unique cohort case with D.1 dropouts.** This re-confirms the PR-Y27 cohort split: D.2/D.3 fixes must target different mechanisms and should remain banked separately.

### §5.3 Cohort guard hardening (still applies)

Per PR-Y27 §4.2: add render-mesh watertight cohort regression test baselines (F0020=36, F0044=12, F0045=38, R0092=43). Whichever PR replaces PR-Y28 should still ship this regression test as cohort-protection layer.

The **arena-vs-render face parity** diagnostic (PR-Y27 surfaced; PR-Y28 reproduced 33 vs 25) is also worth hardening into a regression assertion — it cleanly distinguishes Y28-class defects from Y29-class defects. Baselines: F0020=33 vs 25; F0044=8 vs 8; F0045=8 vs 8; R0092=24 vs 23.

---

## §6 Probe instrumentation (worktree-only, NOT committed)

Located under `/tmp/y28-probe-wt`. Single file modified:

```
$ cd /tmp/y28-probe-wt && git diff --stat
 app/tests/cases/assay/results.json    |   6 +-
 crates/kernel/src/tessellation/mod.rs | 141 +++++++++++++++++++++++++++++++++-
 2 files changed, 143 insertions(+), 4 deletions(-)
```

(`results.json` mutation is the runner's per-test artifact; not probe logic.)

### Probe blocks

1. **ENTRY summary** — `[y28-probe-inv] tessellate_solid_bounded ENTRY: face_map_len=N edges=E verts=V` (one per invocation).
2. **Per-face inventory** — for each face in the dispatch loop, log `[y28-probe-face]` with `kid, face_idx, geom, outer_he_count, outer_nmm_count, is_self_loop, outer_boundary_len, inner_loop_count, inner_sizes, unique_edges`.
3. **Per-face dispatch decision** — log `[y28-probe-dispatch]` with arm (`Planar`/`Cylindrical`/`Fallback`), outer boundary length, inner kept count, and gate firing.
4. **Position dump (small-boundary diagnostic)** — for any face with `outer_boundary_len ∈ [3,6]`, log `[y28-probe-pos]` for each boundary vertex's 3D position + face plane normal. This is what surfaced the kid=221 coincident-vertex defect.
5. **`tessellate_planar_face_bounded` internal branch trace** — log `[y28-probe-planar-no-holes]` with `n, is_convex, has_collinear, reverse_outer` and `[y28-probe-planar-branch]` with `FAN`/`EARCUT_NO_HOLES`/etc and emitted-now count.
6. **Per-face exit** — log `[y28-probe-exit]` with kid, face_idx, indices_emitted, and `face_range_pushed` flag.
7. **End-of-loop summary** — log `[y28-probe-summary]` with totals and `missing_render_faces` count.
8. **Cylindrical short-circuit probe** — log `[y28-probe-cyl]` for the `boundary.len() < 3` gate in `tessellate_cylindrical_face_bounded` (no dispatched faces hit this on F0020, but instrumented for cohort robustness).

All probes are gated on `std::env::var("Y28_PROBE").as_deref() == Ok("1")`. The probe code does NOT mutate any state. It is `eprintln!`-only. Lives only in worktree; not committed to the live tree.

---

## §7 Reproduction artifacts

- Probe stdout F0020 (deep, with positions): `/tmp/y28-canary-f0020-v2.txt` (1410 lines)
- Probe stdout F0020 (with stage dump): `/tmp/y28-canary-f0020-stagedump.txt`
- Probe stdout cohort: `/tmp/y28-canary-f0044.txt` (F0044/F0045/R0092 batch)
- Stage dump F0020: `/tmp/y28-stage-dump-f0020/F0020/` (12 OBJ + 12 CSV per stage; F.0–F.4 + A/Bb/B/C/E)
- Probe instrumentation diff: `/tmp/y28-probe-wt` worktree, 141 lines additive in `tessellation/mod.rs`, 218 total diff lines
- Will be discarded by `git worktree remove /tmp/y28-probe-wt` at close-out

---

## §8 Acceptance gate honesty check

The PR-Y28 plan's acceptance gate for the canary memo (§"Phase 0 canary — Acceptance gate"):

> The canary memo MUST include:
> - Verbatim probe output for each of the 8 missing faces ✓ (§1.1 + §1.2)
> - Clustering analysis with quantitative weights ✓ (§1.3 table)
> - A SINGLE recommended fix shape with code anchor + paper citation + LOC budget estimate ✗ — **NOT FURNISHED**; instead, ABORT recommended with PR-Y29 scope
> - An ABORT recommendation if the clustering is too noisy to pick one fix shape ✓ (§5)

The clustering is NOT too noisy — D.1c is clearly dominant at 59% of dropped triangles. The **chain from anchor to outcome** is what's missing. Per `feedback_phase1_diagnosis_ranking_is_inference.md` and the PR-Y28 plan's own meta-thesis ("structural inference is unreliable; canary measurement is reliable; therefore start with measurement"), the principled call when the chain is unmeasured is to refuse to recommend a fix shape.

I am calling ABORT. The user's brief: *"OR honestly report ABORT if the data isn't conclusive"*. The data tells me where the malformed patches come from. It does not tell me which fix shape closes watertight=0.

End of memo.
