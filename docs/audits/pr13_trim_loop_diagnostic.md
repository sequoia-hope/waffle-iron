# PR13 trim-loop chaining diagnostic (T2)

**Author**: agent-diagnose, team `yang-trim-loop-chaining-pr13`
**Date**: 2026-05-01
**Branch**: `yang-trim-loop-chaining-pr13`
**Scope**: Empirical capture of the per-violation branch-point decision data for
the 9 non-bijective face pairs (2 R0020 + 7 R0021) PR12 deferred to PR13.
Output drives lead's spec §8 finalization (which fix approach: A/B/C/D).

---

## §1 Methodology

Probe: `crates/test-harness/tests/pr13_trim_loop_diagnostic.rs` (this commit).
Built around `with_yang_oracle_capture` + per-feature re-tessellation, mirrors
the pattern in `pr4_r0033_t_junction_diagnosis.rs` and
`pr12_stage1_diagnostic.rs`. Per case:

1. `LoadProject` the `.waffle` under `YANG_BOOLEAN=1` so the full feature
   tree replays through the kernel and Yang pipeline.
2. Walk `state.engine.tree.features[..active_index]` and, for each
   non-suppressed feature with an output, capture (mesh, face_map, arena)
   via `kernel.tessellate(...)` + `kernel.brep_diagnostic_view(...)`.
3. Run `check_face_pair_bijective` on each per-feature artifact. Identify
   the EARLIEST feature that produced a non-bijective B-Rep — that's the
   boolean op that introduced the defect.
4. Per non-bijective pair, dump:
   - `face_a`, `face_b` (B-Rep arena indices), shared `edge` (if known).
   - `unmatched_a_count`, `unmatched_b_count`, sample edges (first 4).
   - Reciprocity check: `sample_unmatched_a[i]` vs `sample_unmatched_b[i]`
     — byte-identical (same direction) or reciprocal (Yang §4.1.1 OK)?
   - Outer loop walks of face_a / face_b in the B-Rep arena.
   - Branch-point degree at unmatched-edge endpoints: how many outgoing
     mesh half-edges from face_a / face_b at each endpoint?

Per `feedback_anchor_before_fix.md`: probe contains `[PR13_PROBE]`
`eprintln!`s on the per-case driver loop, confirming each case is
exercised before the assertion of completeness. **Anchor verification was
critical here** — see §2 for the unanticipated result.

The probe is `#[ignore]`-gated audit-only and does not modify production
code. Run it via:
```
YANG_BOOLEAN=1 cargo test -p test-harness --test pr13_trim_loop_diagnostic \
    -- --ignored --nocapture
```

---

## §2 Critical anchor finding — wrong production code path

**The PR12 archaeological anchor names the wrong function.** The mission
brief and `specs/yang_stage1_bijective.md` §8 amendment both target
`extract_trim_boundaries` lines 1095-1200 (the CW-angular sort at branch
points). **Empirical instrumentation of that function shows it is NEVER
INVOKED by the production Yang pipeline.** Adding `eprintln!`s gated by
`PR13_DUMP_BRANCH=1` to `extract_trim_boundaries` and running R0020+R0021
produced **zero** firings — only the case banner emitted by the probe.

Investigation confirms: `extract_trim_boundaries` is called only from
`#[cfg(test)]` test functions inside `topology_extract.rs` (lines 2636+).
Production code uses **`flood_fill_patches`** (line 351) instead, which
performs its own boundary extraction and loop chaining at Step 6 (lines
607-684). The two functions have **fundamentally different chaining
strategies**:

| Aspect | `extract_trim_boundaries` (test-only) | `flood_fill_patches` (production) |
|---|---|---|
| Adjacency map | `BTreeMap<usize, Vec<...>>` (deterministic) | **`HashMap<usize, Vec<...>>`** (non-deterministic iteration) |
| Branch-point handling | CW-angular sort with face-local 2D frame | **Naive `outgoing.pop()`** (no geometric reasoning) |
| Face normal computation | Yes (lines 1054-1074) | **None** |
| Multi-candidate disambiguation | Smallest CW angle, tie-broken by target idx | **LIFO order of pushes** |

Locations confirmed:
- `flood_fill_patches`: `crates/kernel/src/boolean/topology_extract.rs:351`
- Step 6 boundary loop chaining: lines 607-684, especially lines 644 (HashMap)
  and 664 (`outgoing.pop()`).
- Production callers: `yang_boolean_pipeline` line 1561, `yang_pipeline_result_for_disjoint`
  line 1918.

**Implication**: PR13 spec §8 must redirect the fix to `flood_fill_patches`
Step 6, NOT `extract_trim_boundaries`. The branch-points in the production
path don't HAVE a CW-angular sort — they use naive pop. Lead should treat
the spec/mission's textual references to `extract_trim_boundaries` as
nominal (the feature being investigated) but the LOC anchor must be
updated.

---

## §3 Headline result — empirical confirmation of PR12 archaeological anchor

The **byte-identical-non-reciprocal pattern** PR12 documented holds **9/9
violations** across runs:

```
| case  | #   | fA    | fB    | deg_pA | deg_pB | deg_qA | byte_eq    | reciprocal |
|-------|-----|-------|-------|--------|--------|--------|------------|------------|
| R0020 | 0   | F(1)  | F(7)  | One    | ThreePlus | Two    | true       | false      |
| R0020 | 1   | F(7)  | F(9)  | ThreePlus | Two    | One    | true       | false      |
| R0021 | 0   | F(2)  | F(3)  | One    | ThreePlus | Two    | true       | false      |
| R0021 | 1   | F(2)  | F(5)  | One    | ThreePlus | Two    | true       | false      |
| R0021 | 2   | F(3)  | F(8)  | Two    | Two    | ThreePlus | true       | false      |
| R0021 | 3   | F(3)  | F(11) | ThreePlus | Two    | One    | true       | false      |
| R0021 | 4   | F(5)  | F(11) | ThreePlus | Two    | One    | true       | false      |
| R0021 | 5   | F(7)  | F(11) | ThreePlus | Two    | One    | true       | false      |
| R0021 | 6   | F(8)  | F(11) | One    | Two    | ThreePlus | true       | false      |
```

`byte_eq=true` on all 9 means every reported `(p, q)` directed edge
emitted from face A was matched byte-for-byte by an identical `(p, q)`
emission from face B — **not** the reciprocal `(q, p)`. Per Yang §4.1.1,
twin half-edges must produce reciprocal mesh boundary edges, so this is a
direction-consistency failure.

**Per-feature attribution**:
- **R0020**: feature[1] (`Revolve 1`) → 12/12 bijective. feature[3]
  (`Extrude 2`, the cut) → 17/19 bijective, **2 NB pairs**. The defect is
  introduced by op2.
- **R0021**: feature[1] (`Extrude 1`) → 12/12 bijective. feature[3]
  (`Extrude 2`, boss) → 16/23 bijective, **7 NB pairs**. Defect introduced
  by op2.
- In both cases, op3 (`Extrude 3`) consumes the broken op2 result and the
  Stage 6 oracle catches the twin-asymmetry that follows.

The B-Rep arena's `outer_loop` walks (face_a 6 verts; face_b 50 verts) on
R0021 NB pair #0 reveal the structure: **face A is a CCW polygon of 6
verts on a planar region; face B is a CCW polygon of 50 verts on the
adjacent (curved) face**. The 4 unmatched directed edges all lie on the
shared B-Rep edge between A and B. Both face A AND face B emit those
edges in the same orientation; one of the two faces' loop is walking the
shared edge in the WRONG direction.

---

## §4 Branch-point degree analysis

Branch-point degree at the unmatched-edge endpoint position p (mesh
out-edges from each face):

| Degree | At p in face_a | At p in face_b |
|---|---|---|
| Zero (missing) | 0/9 | 0/9 |
| One (linear)   | 4/9 | 1/9 |
| Two            | 1/9 | 6/9 |
| ThreePlus      | 4/9 | 3/9 |

So at the unmatched-edge endpoint:
- **Face A**: about half the time the endpoint is at a real branch (≥3
  outgoing mesh edges).
- **Face B**: typically has fewer outgoing edges at the endpoint (the
  B-side of the shared B-Rep edge is more linear in the mesh).

For pair `R0021 #3` (face F(3)→F(11)), face_a has ≥3 outgoing at p AND
face_b has 2 outgoing at p — **both** faces face a non-trivial choice at
the same vertex. This is the configuration most amenable to a geometric
reasoning fix (CW-angular sort with shared frame).

---

## §5 Direct production-code instrumentation: the naive pop produces same-direction picks

Adding env-gated `eprintln!`s to `flood_fill_patches::Step 6` on R0021 and
running the probe:

- 14 branch-point decisions across one run (3 with `n_cands=3`, 11 with
  `n_cands=2`). All 14 are in operand A (none in operand B — consistent
  with the operand-A-asymmetric pattern PR12 §5 documented).
- All three `n_cands=3` branches occur at `patch=4 source=FaceIdx(3)`,
  position `(8.11e-2, -2.15e-1, -2.53e-1)` — the same vertex three
  different times (different mesh-vertex indices because of pre-step
  dedup). Each time:
  - cand[0]: v=12 at `(9.47e-2, -2.88e-1, -2.53e-1)`, is_int=true (the
    geometrically-correct continuation along the shared edge).
  - cand[1]: v=18 at `(7.69e-2, -1.92e-1, -2.53e-1)`, is_int=true.
  - cand[2]: v=18 at `(7.69e-2, -1.92e-1, -2.53e-1)`, is_int=true (a
    DUPLICATE of cand[1] targeting the same canonical vertex).
  - **Naive pop chooses cand[2]** every time (LIFO).

This reveals two interacting defects in `flood_fill_patches` Step 6:
1. **Duplicate boundary edges**: the patch construction admits multiple
   edges with the same `(v0, v1)` pair targeting the same canonical
   vertex. (Likely a Cherchi arrangement post-dedup artifact — neighbour
   tris with parallel oriented edges across a degenerate sub-triangle.)
2. **No geometric disambiguation**: the chaining picks one of the
   duplicates by LIFO order. Without face-local frame reasoning, the
   correct continuation (cand[0]) is silently dropped. The result is a
   loop that walks back-and-forth between v=18 and v=111 instead of
   continuing onward.

For the R0020 case, **zero branch points** fire in Step 6 — yet R0020
still produces 2 NB pairs. So R0020's defect mechanism is **distinct**
from the duplicate-edge / LIFO-pick mechanism. Hypothesis: R0020's loop
chaining is single-path on every face, but two adjacent faces have
LOOPS that traverse the shared edge in the SAME orientation (one's
inner-CCW happens to match the other's inner-CCW rather than being
reciprocal). This points to a per-face START-vertex pick problem upstream
of branch-point chaining: `adj.iter().find(...)` on a HashMap picks a
non-deterministic start, and the resulting loop direction propagates.

---

## §6 Determinism

Across 5+ probe runs:

- **Stable**: Total NB pair count is `R0020=2, R0021=6-7` (R0021 flaps 6
  vs 7 across runs).
- **Stable**: `byte_eq=true / reciprocal=false` on **every** NB pair in
  every run.
- **Stable**: Per-feature attribution — feature[3] produces violations,
  feature[1] does not.
- **Stable**: Operand-A-asymmetric (operand B always has 0 NB pairs).
- **Flapping**: Specific face indices in the NB pairs vary (R0021 face
  F(11) some runs, F(10) others) because face-numbering depends on the
  HashMap iteration order at patch-construction time.
- **Stable**: The directed edge byte values for R0020 #0 (4 unmatched
  edges) are byte-identical across 3 consecutive runs:
  ```
  a-edge[0]: (-4.589e1, 2.588e1, -3.819e1) → (-4.323e1, 2.442e1, -3.689e1)
  a-edge[1]: (-4.867e1, 2.869e1, -4.171e1) → (-5.185e1, 2.980e1, -4.221e1)
  a-edge[2]: (-4.940e1, 2.640e1, -3.759e1) → (-5.027e1, 2.586e1, -3.633e1)
  a-edge[3]: (-4.953e1, 2.750e1, -3.937e1) → (-4.940e1, 2.640e1, -3.759e1)
  ```

Per `feedback_no_regression_chasing.md`: count flap is consistent with
the residual non-determinism in `flood_fill_patches::Step 6`'s `HashMap`
adjacency map. PR12's Step 1 widening converted four `boolean/` files,
but `flood_fill_patches` was not on that list — the HashMap on line 644
of `topology_extract.rs` is a confirmed PR12-residual non-determinism
source.

---

## §7 Cluster classification

Per-violation classification by **frame condition** + **branch-point
degree** + **same-mechanism cluster**:

### Cluster D1 — Duplicate-edge naive-pop (R0021 dominant pattern)

Pairs: R0021 #2, #3, #4, #5, #6 (5 pairs). All involve face F(11) /
F(10) — the cap face produced by the fresh extrude. The branch-point at
patch=4 / position `(8.11e-2, ...)` has duplicate boundary edges to v=18.
The naive `outgoing.pop()` picks one arbitrarily, producing a same-direction
emission rather than the reciprocal that the duplicate pair expects.

**Frame condition**: well-conditioned (face normals well-separated from
adjacent face normals).
**Branch-point degree**: 3 outgoing with 2 duplicates targeting same canonical vertex.
**Mechanism**: duplicate-edge admission + LIFO pick.

### Cluster D2 — Single-path same-direction (R0020 + R0021 pairs #0,#1)

Pairs: R0020 #0, R0020 #1, R0021 #0, R0021 #1 (4 pairs). Branch-point
degree at p in face_a is 1 (linear); face_b varies. No multi-candidate
branch decision in Step 6 — yet the directed edges still come out
non-reciprocal. R0020 has ZERO branches in flood_fill_patches Step 6 in
its run, confirming this pattern.

**Frame condition**: well-conditioned.
**Branch-point degree**: 1-2 outgoing per face at endpoint.
**Mechanism**: Step 6 does not coordinate START-vertex pick across
adjacent faces. `adj.iter().find(...)` (line 651-654) picks the first
HashMap entry with non-empty outgoing edges; this is non-deterministic
and uncorrelated between face A's loop and face B's loop. As a result,
two adjacent faces sharing a B-Rep edge can both walk that shared edge
"forward" rather than reciprocally.

### Same-mechanism vs split

The two clusters share a root cause at one level of abstraction: **Step 6
loop chaining has no concept of inter-face direction consistency**. D1
shows it inside a single face (duplicate edges), D2 shows it across
adjacent faces (independent start picks). Either way, the algorithm
makes locally-arbitrary choices that aggregate to globally-inconsistent
boundary directions.

A single fix that imposes inter-face direction consistency (approaches
A/B/C below) would address both clusters. A surgical fix to D1 alone
(deduplicating the boundary edges before chaining) would not fix D2.

---

## §8 Recommendation: Branch (A) Edge-canonical reference

### Why not (B) — global manifold post-hoc cross-check

Yang §4.1.1's "by-construction" framing requires Stage 1's output to BE
manifold without post-hoc repair. Per `feedback_yang_only.md`, an
ex-post-facto direction-flip is anti-paper.

### Why not (C) — twin-edge lookup at chaining time

`flood_fill_patches` Step 6 operates BEFORE `arena.half_edges` exist
(half-edges are created in Step 7, lines 686+). So at branch-point pick
time the twin-pair structure is not yet built. (C) requires either
two-pass construction or a parallel twin-tracking data structure built
during Step 6 — both are larger refactors than warranted.

### Why not (D) — surgical fix to one trigger

D1 (duplicate edges) and D2 (independent start picks) are both
surface-level effects of the same root cause: `flood_fill_patches` Step 6
has no inter-face coordination. A surgical fix (e.g., dedupe duplicate
edges before chaining, or sort `adj` by canonical vertex order) would
suppress one symptom without addressing the architecture defect.

### Why (A) — edge-canonical reference

**Approach**: along every patch boundary, when chaining arrives at a
vertex with multiple outgoing edges, pick the one whose
**B-Rep-canonical successor** is consistent with adjacent patches. The
B-Rep edge that face A and face B share has TWO endpoints; in
canonical-vertex-id order (lower → higher), there is ONE preferred
direction. Both face A and face B should emit boundary edges
consistently with that canonical direction — one face on the (lo→hi)
side, the other on the (hi→lo) side. This guarantees reciprocal
emissions by construction.

**Concrete instantiation**:
1. After patch construction (Step 5a), build a map
   `(v0_canon, v1_canon) → patch_id` for every directed boundary edge.
2. When chaining a patch, sort `adj[v]` entries so the first popped is
   the one whose `(v, target)` has the canonical orientation matching
   "this patch's CCW" — derivable from the patch's `source` (mesh A vs
   mesh B + face winding).
3. Equivalently for the start-vertex pick: use the canonical edge-pair
   ordering rather than HashMap iteration.

This lifts the chaining algorithm from "locally pick anything coherent"
to "locally pick the unique edge consistent with global manifold
structure." It also implicitly handles D1 (duplicates with same target
canonical vertex are equivalent — the choice between them is forced by
canonical orientation).

**Estimated LOC**: 50-200 LOC in `flood_fill_patches` Step 6 (lines
622-684). The work is:
- Compute the canonical edge-direction reference map after Step 5a.
- Replace `HashMap<usize, Vec<(usize, bool)>>` with a deterministic
  structure.
- Replace `outgoing.pop()` with a canonical-direction-aware picker.
- Replace `adj.iter().find(...)` with a deterministic start picker.

**Validation criteria** (for T3 red tests + T4 impl):
- R0020 / R0021 Stage 1 oracle returns Ok post-fix.
- Stage 6 oracle returns Ok post-fix (twin-symmetry restored).
- No regression on PR12's 84 AllPass cases.
- Determinism: 3+ consecutive probe runs report identical NB-pair counts
  and verdicts.

### Spec §8 ambiguity surfaced

The spec's `extract_trim_boundaries::1095-1200` LOC anchor is wrong (see
§2). The fix surface is `flood_fill_patches::Step 6`, lines 607-684.
Lead should amend §8 with the corrected anchor and update §8a's "fix
surface" subsection.

---

## §9 Open questions / followups

1. **What does PR12's HashMap→BTreeMap fix accomplish if `flood_fill_patches`
   still uses HashMap?** PR12 was scoped to `tessellation/bijective.rs`
   and four `boolean/` files; `flood_fill_patches` is `boolean/topology_extract.rs`
   but was not touched. The remaining HashMap on line 644 is a
   confirmed PR12-residual non-determinism source, contributing to the
   count flap (R0021 NB count varies 5/6/7 across runs).

2. **Does R0020's mechanism (D2) match Cluster Y cases (R0035, F0019,
   etc.)?** Cluster Y has S1 fire but S2 = Ok. The decoupled mechanism
   in Cluster Y might also be Step 6 START-vertex independence, but
   diluted because Y cases pass Stage 2 conservation. Worth empirical
   probe in PR14.

3. **Why 4-7 NB on operand A, never on operand B?** The asymmetry is
   stable across all PR12 + PR13 runs. Operand A is the result of the
   PREVIOUS boolean op; operand B is a freshly-tessellated extrude/revolve
   primitive. The freshly-tessellated mesh is bijective by construction
   (single tessellation pass, well-defined face boundaries). The
   boolean-op-result mesh accumulates `flood_fill_patches` defects.
   Multi-op chains compound the asymmetry.

---

## §10 Appendix — Probe artifact summary

- `crates/test-harness/tests/pr13_trim_loop_diagnostic.rs` — verbose probe.
  - Walks `state.engine.tree.features` after `LoadProject` and
    re-tessellates each feature's solid, runs `check_face_pair_bijective`
    on each.
  - Identifies the EARLIEST feature with NB violations and dumps per-pair
    detail: face_a, face_b, edge, sample unmatched edges with reciprocity
    check, B-Rep arena outer-loop walks, and branch-point degree at
    endpoints.
  - `[PR13_PROBE]` `[ANCHOR]` `eprintln!`s on the per-case driver loop
    confirm pipeline reach per `feedback_anchor_before_fix.md`.
  - `#[ignore]`-gated; production code is NOT modified by the probe
    itself. Branch-point dumps inside `flood_fill_patches::Step 6` were
    captured during instrumented runs and reverted before commit.

- Logs (referenced in body):
  - `/tmp/pr13_run4.log` — instrumented run with branch-point dumps
    showing the duplicate-edge / naive-pop pattern.
  - `/tmp/pr13_run6.log` — uninstrumented final probe run.

## §11 Verification — production-code clean

```
$ git diff --stat crates/kernel/
(no output)
```

All temporary instrumentation in `crates/kernel/src/boolean/topology_extract.rs`
and `crates/kernel/src/diagnostics.rs` was reverted before this commit.
The probe is fully self-contained and depends only on the existing
public API (`with_yang_oracle_capture`, `kernel.tessellate`,
`kernel.brep_diagnostic_view`, `check_face_pair_bijective`).
