# PR-Y26 ABORT — all three plan candidates empirically refuted; defect is missing triangles, not seam misalignment

**Date:** 2026-05-10
**Verdict:** **ABORT** at canary phase per spec acceptance gate ("All candidates < 9/36 → UNCLASSIFIED → Investigate before specing" AND "New mechanism appears → NEW HYPOTHESIS → Document and bring to user for scope decision")
**Anchors refuted:** Option (i) Yang §4.4.1 vertex non-unification, Option (j) earcut diagonal mismatch, Option (k) inner-loop re-emission
**New mechanism surfaced:** missing triangles in render mesh (count=1 boundary-only-one-tri); cohort-wide

This is the second consecutive ABORT at canary phase (PR-Y25 was the first). The discipline `feedback_anchor_before_fix.md` + `feedback_phase1_diagnosis_ranking_is_inference.md` paid off — both PRs caught wrong fix shapes BEFORE writing any production code.

---

## §1 Cycle artifacts (kept on main)

PR-Y26 produced one useful artifact:
- **`docs/audits/pr_y26_anchor_canary.md`** (commit `0bbda11`) — empirical refutation of all three candidates with classification table + new mechanism.

No spec, no test, no impl — Phase 0 acceptance gate triggered ABORT before those phases ran.

---

## §2 What canary-y26 found

### F0020 b#2 (load-bearing case)
```
[y26-probe-p1-summary] total_unpaired=36 total_edges=130 tri_count=76
[y26-probe-p2-summary] total_unpaired=36 candidate_i_count=0 candidate_j_count=2 candidate_k_count=0 unclassified_count=34
[y26-probe-p3-summary] cand_i_count=0 p3_lines_emitted=0
```

The 2 candidate_j entries are zero-length quantized-degenerate triangle artifacts (`qa==qb`), not real (j) signature. Effective verdict: **0 of 36 fit any of (i)/(j)/(k)**.

### F0044/F0045/R0092 cohort (NEW finding)
```
F0044 b#1: total_unpaired=12 i=0 j=0 k=0 unclassified=12
F0045 b#2-4: total_unpaired=38 i=0 j=0 k=0 unclassified=38
R0092 b#5+: total_unpaired=43 i=0 j=0 k=0 unclassified=43
```

The cohort has been silently failing at the render-mesh watertight layer all along. PR-Y22 and PR-Y24 cohort guards measured topology-layer metrics (`[topo-extract]`, `[twin-oracle]`) — both green. But `check_watertight_mesh` (position-keyed pairing on the render mesh) was never asserted on the cohort. It would have been red since well before PR-Y22.

### Dominant mechanism

**34 of 36** unpaired edges in F0020 are `count=1 sig=boundary_only_one_tri` — exactly one triangle uses the edge; its twin is **absent**. 100% of cohort unpaired edges are the same shape.

The 33 non-degenerate count=1 edges in F0020 form **3 connected components** in the quant-endpoint graph:
- A 3-cycle (closed triangle hole)
- A 16-vertex / 20-edge bowtie
- A 9-vertex chain

This is a structural signature of **3 missing surface patches** in the final retessellated solid.

100% of unpaired edges have `prov_a=Some(...)` (their owning face exists in `face_provenance`) — meaning the drop happens BEFORE `result_topology_to_waffle_solid` populates the face map. The defect is at the topology-extraction layer or earlier, not at the tessellator.

---

## §3 Why all three plan candidates were wrong

The plan's (i)/(j)/(k) candidates all assume a `count=2` antecedent: "two triangles use the edge but their vertex positions disagree." The empirical data shows `count=1` dominance: **only one triangle exists**. Position-disagreement is structurally impossible when only one position is emitted.

PR-Y25 lesson: Phase 1 Explore agents' diagnosis ranking is inference, not measurement (`feedback_phase1_diagnosis_ranking_is_inference.md`). Phase 1 ranked (i) HIGH because it traced the SSI refinement code path and saw the per-face vertex addition. But Phase 1 didn't measure whether the unpaired edges were `count=2`-disagreement or `count=1`-missing. The canary measured.

PR-Y26 lesson: Phase 1's three candidates were all `count=2`-disagreement hypotheses. None considered `count=1`-missing. The canary's investigational mandate (per plan §"Phase 0 canary - INVESTIGATIONAL") let it surface the `count=1` finding via the unclassified bucket. **The plan's investigational design was correct;** the cost was just one canary cycle.

---

## §4 Bank for PR-Y27

### §4.1 Re-anchor canary candidates (per canary memo §"Banked next-investigation anchors")

| Anchor | Mechanism | Code site | Notes |
|---|---|---|---|
| **(m)** Surface-patch dropout in `flood_fill_patches` | Yang §4.4.2 NMM-aware patch labeling drops valid patches | `topology_extract.rs:404-1589` (flood_fill_patches body) | Most likely site; the 3 missing patches in F0020 should map to specific `flood_fill_patches` decisions |
| **(n)** Face deletion between Yang topology-extract and `result_topology_to_waffle_solid` | Some intermediate stage drops face entries | `yang_integration.rs:208-289` (result_topology_to_waffle_solid) + upstream | Less likely given `face_provenance` is 100% populated for surviving faces |
| **(o)** Triangle filter in retessellation | `tessellate_solid_bounded` drops triangles that should be emitted | `tessellation/mod.rs:4183-4427` | Refuted by canary §5: `prov_a=Some(...)` for all unpaired edges; if tessellator dropped triangles, `prov_a=None` would appear |

### §4.2 PR-Y27 first canary (REQUIRED before any fix)

For F0020 b#2, identify the 3 missing surface patches:
1. Walk all triangles in the rendered mesh; build a watertight closure via missing-twin enumeration
2. The 3 connected components of count=1 edges describe the BOUNDARY of the missing region
3. Trace those boundary positions back to the topology arena (which face should have emitted these triangles?)
4. Compare to `flood_fill_patches` output — was that face's patch correctly identified, or dropped?
5. Probe `label_cells` for the boolean classification of each patch — did the boolean op filter retain them?

For F0044/F0045/R0092 cohort: same enumeration, expecting analogous missing-patch structures.

### §4.3 Cohort scope re-evaluation

PR-Y27 must address the COHORT, not just F0020. The defect is:
- F0020: 36 unpaired (3 components)
- F0044: 12 unpaired
- F0045: 38 unpaired (largest)
- R0092: 43 unpaired (largest)

Any fix that drops F0020 to 0 but leaves F0045/R0092 unchanged is partial; re-evaluate whether the load-bearing target is "F0020 ships" or "all 4 cases ship watertight."

### §4.4 Cohort-guard hardening

The PR-Y22+PR-Y24 cohort guards (topology-layer) are NOT sufficient. PR-Y27 should add a render-mesh watertight cohort guard so we don't continue silently failing. Concretely: the test-y26 file `pr_y26_cohort_no_regression` (which was never written because of the ABORT) should be created in PR-Y27 with the actual cohort baselines (12/38/43 unpaired) as the regression bar.

---

## §5 Discipline notes

- Live tree at `/home/claude/workspace` clean throughout (verified via `git status` snapshots in canary memo).
- Probe worktree `/tmp/y26-probe-wt` removed at canary close.
- No `git stash`/`reset --hard` on live tree.
- ZERO production code modified across PR-Y26 cycle.

---

## §6 Recommendation to user

PR-Y26 is correctly aborted at canary phase. The discipline of empirical-first canary continues to pay off — two consecutive ABORTs (Y25, Y26) caught wrong fix shapes BEFORE production code. Recommended next steps:

1. **Open PR-Y27** with a re-anchor canary that identifies the 3 missing patches in F0020 (per §4.2) and the corresponding missing patches in F0044/F0045/R0092.
2. The fix shape is unknown until the canary identifies the dropout mechanism — likely candidate (m) `flood_fill_patches` patch dropout.
3. Reconsider scope: F0020-only fix vs F0020+cohort fix. The canary's cohort finding suggests the defect is structural and a single fix may resolve all 4 cases.
4. Add a render-mesh watertight cohort regression test (per §4.4) so future PRs can't silently regress this layer.

The team teardown (TeamDelete) closes this PR's cycle. PR-Y27 spawns a fresh team.
