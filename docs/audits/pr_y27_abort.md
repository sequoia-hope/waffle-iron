# PR-Y27 ABORT — flood_fill_patches drops zero SourceFaces; cohort splits into 3 mechanisms

**Date:** 2026-05-10
**Verdict:** **ABORT** at canary phase per spec acceptance gate ("Drop site is something not in the B.1-B.7 / A.4 / C.4 list → NEW HYPOTHESIS → Document and bring to user for scope decision")
**Anchors refuted:** All 7 plan candidates (B.1, B.4, B.5, B.6, B.7, A.4, C.4)
**New finding:** F0020 + cohort defect splits into 3 distinct mechanisms in `tessellate_solid_bounded`, NOT in `flood_fill_patches`

This is the **third consecutive canary-stage ABORT** (PR-Y25, PR-Y26, PR-Y27). All three caught wrong fix shapes BEFORE production code. The discipline `feedback_anchor_before_fix.md` + `feedback_phase1_diagnosis_ranking_is_inference.md` continues to pay off — three for three.

---

## §1 Cycle artifacts (kept on main)

PR-Y27 produced one useful artifact:
- **`docs/audits/pr_y27_anchor_canary.md`** (commit `410da9a`) — empirical refutation with cohort-split classification table and recommended PR-Y27a/b/c structure.

No spec, no test, no impl — Phase 0 acceptance gate triggered ABORT.

---

## §2 What canary-y27 found

### §2.1 P1 — `flood_fill_patches` does NOT drop SourceFaces

```
F0020 inv 1: survival_groups=12 arena_faces=26 missing_count=0
F0020 inv 2: survival_groups=20 arena_faces=33 missing_count=0   ← LOAD-BEARING
F0044 final: survival_groups=6  arena_faces=8  missing_count=0
F0045 inv 4: survival_groups=6  arena_faces=8  missing_count=0
R0092 inv 3: survival_groups=5  arena_faces=24 missing_count=0   ← LOAD-BEARING
(plus 3 earlier cohort invocations: all missing_count=0)
```

PR-Y26 §4.1 candidate (m) — "surface-patch dropout in flood_fill_patches" — is **REFUTED at the SourceFace granularity**. Every SourceFace in `survival.groups` appears in `face_provenance`.

### §2.2 P2 — drop-site attribution

```
                    F0020 inv2  F0044 fin  F0045 inv4  R0092 inv3
B.1 canon_degen        6           0          8           0
B.5 R3_owner_strip    25 (9 src)   0          0           0
B.6 open_chain        16 (9 src)   0          0           0
B.7 zero_loop          0           0          2           0
```

B.5+B.6 fire ONLY on F0020. F0044/F0045/R0092 final invocations: **ZERO drop sites fire** — yet render produces 12, 38, 43 unpaired edges.

**None of the plan's 7 candidate fix shapes (B.1-B.7, A.4, C.4) can explain the cohort defect.**

### §2.3 The actual defect surfaces — `tessellate_solid_bounded`

Render face_id count vs arena face count:
- **F0020:** 33 arena → 25 in render (**8 missing render faces**)
- **F0044:** 8 → 8 (0 missing)
- **F0045:** 8 → 8 (0 missing)
- **R0092:** 24 → 23 (1 missing)

The 8 missing F0020 face_ids correspond to `tessellate_solid_bounded` L4283-4290: faces emit zero indices and `FaceRange` is not pushed. F0044/F0045 lose zero faces but still produce count=1 unpaired — defect is per-face seam mismatch on CLOSED arena topology.

---

## §3 NEW finding — three distinct cohort mechanisms

| Case | Mechanism | Symptom | Likely fix layer |
|---|---|---|---|
| **D.1** F0020 | 8 missing render faces + 39 NMM HEs (mixed clean + malformed arena) | 36 unpaired (3 components: 3-cycle + bowtie + chain) | `tessellate_solid_bounded` face emission OR upstream B.5/B.6 |
| **D.2** F0044/F0045 | Clean arena (no drops, no NMM); per-face seam mismatch in render-LOD tessellation | 12+38 unpaired | `tessellate_solid_bounded` per-face vertex unification at face seams |
| **D.3** R0092 | 44 legit NMM HEs ≈ 43 unpaired | 43 unpaired | `tessellate_solid_bounded` doesn't handle Yang §4.4.2 NMM edges in render |

PR-Y26's (i)/(j) hypotheses (vertex unification, earcut diagonal) **may have been partially right for D.2** but were measured incorrectly because the canary §D probe co-located vs NMM-HE positions only — D.2's defect is at non-NMM positions but the position-disagreement is sub-quantization-granularity (the watertight oracle's grid is too coarse to expose the edges' would-be-pairing).

PR-Y25's "Diagnosis B Yang §4.4.1" was structurally right for D.2 but couldn't be measured at quantization granularity.

PR-Y26's "missing patches" was right for D.1 (8 missing render face entries) but wrong about WHERE they go missing — not in flood_fill_patches but in tessellate_solid_bounded.

---

## §4 Bank for next PR(s)

### §4.1 Cohort-split (canary's recommendation)

The canary recommends splitting into three separate PRs:

- **PR-Y27a** — F0020 missing render face entries (8 of 33). Anchor: `tessellate_solid_bounded` L4283-4290 face emission gate. Likely fix: detect zero-index emission and either repair upstream or emit a degenerate marker.
- **PR-Y27b** — F0044/F0045 per-face seam mismatch on clean arena. Anchor: per-face vertex emission in `tessellate_planar_face_bounded` L3349-3356. Likely fix: shared edge-vertex pool already exists in `disc.edge_verts` but f64→f32 + accumulation order may diverge across faces; need empirical canary on positions.
- **PR-Y27c** — R0092 NMM-edge handling in retessellation. Anchor: `tessellate_solid_bounded` doesn't share render-mesh vertices across NMM-pair HEs. PR-Y25's option (h) was the correct shape but rejected because F0020 had no NMM pairs (canary §D); R0092 DOES have NMM pairs (44 ≈ 43 unpaired correlation suggests).

**Wait — D.3's "NMM HEs cause unpaired edges" contradicts PR-Y25's canary §D finding** (which said 0/36 F0020 unpaired edges sit at NMM-HE positions). The contradiction:
- F0020: NMM HEs are at PAIRED render edges (PR-Y25 finding)
- R0092: NMM HEs may be at UNPAIRED render edges (PR-Y27 inference from count correlation)

The R0092 inference is structural inference, not measurement. PR-Y27c canary must include a §D-style position-co-location probe to confirm before scoping.

### §4.2 Banked findings worth carrying

1. **Render face count drift signal:** The canary's `arena_faces.len() vs render face_ranges.len()` comparison is a useful new diagnostic. PR-Y27a's regression test should assert `arena.faces.len() == render_mesh.distinct_face_ids().count()`.
2. **Cohort-guard hardening still needed:** PR-Y22+PR-Y24 guards measure topology-layer only. Whichever PR addresses D.1/D.2/D.3 must add render-mesh watertight cohort regression test (baselines: F0020=36, F0044=12, F0045=38, R0092=43).
3. **Quantization granularity matters:** D.2's defect is at positions that quantize to different grid cells but the f64 distance is sub-grid (within TAU_TESS_GRID_FACTOR). The watertight oracle's grid is `max_abs * 2e-6` ≈ several µm at typical CAD scales. Vertex disagreement at f32 precision (~µm) can fall on different sides of the grid.

### §4.3 Possibly-deferred path: investigate Phase-1-Explore-as-canary anti-pattern

Three consecutive ABORTs (Y25, Y26, Y27) suggest Phase 1 Explore agents systematically over-confident. They produce plausible-but-wrong rankings; the canary refutes them. The cycle cost is small (~30-60 min canary) but the planning effort (Phase 1 + Phase 2 + plan write) is wasted.

Possible PR-Y28+ pattern: **skip Phase 1 explore agents; spawn the canary FIRST with a broad investigational mandate** ("identify which layer hosts the defect; report what you find"). The plan is then written AROUND the canary's findings, not before them.

This would invert the current pattern (plan → canary → spec) to (canary → plan/spec). Worth user consideration before PR-Y27a/b/c.

---

## §5 Discipline notes

- Live tree at `/home/claude/workspace` clean throughout.
- Probe worktree `/tmp/y27-probe-wt` retained at canary close; will be removed.
- No `git stash`/`reset --hard` on live tree.
- ZERO production code modified.

---

## §6 Recommendation to user

PR-Y27 is correctly aborted at canary phase. The three-PR cohort split (Y27a/b/c) is the canary's structural recommendation, but the user should consider before proceeding:

1. **Cohort-split path:** Open three separate PRs — Y27a (F0020 missing render faces), Y27b (F0044/F0045 seam mismatch), Y27c (R0092 NMM render). Each gets its own canary. Smaller per-PR scope; lower risk per PR. **3× cycle cost.**

2. **Investigate-first path:** Open PR-Y27a with a broader canary mandate that probes WHICH of the 3 mechanisms is actually causing F0020's user-visible failure (the user's brief was "make F0020 ship"). The other two mechanisms might or might not need separate fixes; canary measures.

3. **Pause-and-replan path:** Three consecutive canary-stage ABORTs suggest the Phase 1 Explore agent pattern is systematically over-confident. Consider inverting the workflow for PR-Y27+: canary FIRST with broad investigational mandate, then plan around findings. This would be a meta-process change, not a code change.

The team teardown (TeamDelete) closes this PR's cycle. Recommend bringing to user for scope decision before opening PR-Y27a/b/c or PR-Y28.
