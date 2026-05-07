# PR-Y17-TWIN sub-phase 0a — Algorithm A canary on F0030

**Author:** canary-runner-2
**Date:** 2026-05-06
**Scope:** READ-ONLY empirical probe of Algorithm A (most-antiparallel patch-pair-normal pairing) viability on F0030's 11 collision-arm cases at `crates/kernel/src/boolean/topology_extract.rs:1138-1162`.

---

## §1 — Collision case enumeration on F0030

Probe: `TWIN_DEBUG=1 YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0030 --ignored --nocapture --test-threads=1`. Pre-pairing baseline observation (extracted from existing `[twin-debug] edge ...` lines emitted at `topology_extract.rs:1080-1090`):

| idx | edge (lo→hi) | fwd_count | rev_count | candidates per fwd HE |
|-----|--------------|-----------|-----------|------------------------|
|  1  | (4, 5)       | 1         | 2         | HE[4]  → [48, 72]      |
|  2  | (4, 16)      | 1         | 2         | HE[49] → [38, 56]      |
|  3  | (5, 6)       | 1         | 2         | HE[5]  → [47, 71]      |
|  4  | (6, 7)       | 1         | 2         | HE[6]  → [46, 70]      |
|  5  | (7, 8)       | 1         | 3         | HE[7]  → [33, 51, 69]  |
|  6  | (8, 9)       | 1         | 2         | HE[8]  → [43, 68]      |
|  7  | (8, 16)      | 1         | 2         | HE[44] → [32, 50]      |
|  8  | (9, 10)      | 1         | 2         | HE[9]  → [42, 67]      |
|  9  | (10, 11)     | 1         | 3         | HE[10] → [36, 54, 66]  |
| 10  | (11, 16)     | 2         | 2         | HE[40] → [35, 53]      |
| 11  | (11, 16)     | 2         | 2         | HE[58] → [35, 53]      |

Total: 11 collision-arm cases. Matches Phase 1 probe report. `[topo-extract] summary: paired=21, unpaired=4, ambiguous=11`. Independent `[twin-oracle]` block reports `unpaired_count=37, collision_count=3` post-pairing — consistent (37 = 4 no-cand + 11 collisions × ~3 affected HEs each, since the collision arm leaves the fwd + ALL unmatched candidates with default `twin = 0`).

The collision cluster is concentrated on a single coplanar plane at z ≈ 0.2735878 m (verified via `[twin-oracle] offender` lines reporting `dest=(...,2.735878e-1)` for he=4..8). Vertices 4-11, 16 form the boundary of a coplanar A-cap face overlapping with a coplanar B-cap face — exactly the Yang 2025 §4.5.5 coplanar-preprocessing case.

---

## §2 — Patch-pair-normal data per collision case

Method: I extended `flood_fill_patches` with a temporary probe (now reverted; `git diff` clean as of memo write):
1. Computed per-patch outward normals as the **area-weighted normalized average of triangle normals** for all `all_tris[ti]` members, using `subdivided.verts` for vertex positions and the post-flip Step 3 vertex ordering. `n_p = normalize(Σ_t (e1_t × e2_t))` where `e1_t = p1_t - p0_t`, `e2_t = p2_t - p0_t`.
2. Built `he_to_patch_idx: HashMap<HalfEdgeIdx, usize>` parallel to `he_to_face` during the patch_boundaries loop (HE belongs to the patch whose `pb` it was emitted from).
3. In the collision arm, computed `angle_deg = acos(clamp(dot(n_fwd, n_cand), -1, 1)) · 180/π` for the forward HE's patch normal vs. each candidate reverse HE's patch normal. Sorted descending (most-antiparallel first). Reported tie status with ε = 0.5°.

**No accessor existed in `CellLabeling` for per-patch outward normal** — `CellLabeling` only stores `Inside`/`Outside` enum labels per sub-triangle (`exact_mesh.rs:1189-1194`). I had to compute normals on the fly from the triangle geometry. This is a real implementation cost for sub-phase 0d: either extend `CellLabeling` / `ManifoldPatchGraph` with a `patch_outward_normals: Vec<[f64;3]>` field and populate it where `label_cells` already iterates per-patch, OR add the on-the-fly computation inline to `flood_fill_patches` (~25 LOC, what I did for the probe).

Per-case probe data (raw `[algA-canary]` output, condensed):

| idx | edge | fwd HE / patch / normal | candidates (he, patch, normal, angle°) | top angle | 2nd angle | Δ | tie status |
|-----|------|--------------------------|-----------------------------------------|-----------|-----------|---|------------|
| 1 | (4,5)   | HE[4]/p0/(0,0,1)   | (72,p15,(0,-0.6153,0.7883),37.972), (48,p10,(0,0,1),0)         | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS |
| 2 | (4,16)  | HE[49]/p10/(0,0,1) | (38,p8,(0,0,1),0), (56,p13,(0,0,1),0)                          | 0.000°  | 0.000° | 0.000°  | TIE_WITHIN_ε |
| 3 | (5,6)   | HE[5]/p0/(0,0,1)   | (71,p15,(0,-0.6153,0.7883),37.972), (47,p10,(0,0,1),0)         | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS |
| 4 | (6,7)   | HE[6]/p0/(0,0,1)   | (70,p15,(0,-0.6153,0.7883),37.972), (46,p10,(0,0,1),0)         | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS |
| 5 | (7,8)   | HE[7]/p0/(0,0,1)   | (69,p15,(0,-0.6153,0.7883),37.972), (33,p6,(0,0,1),0), (51,p11,(0,0,1),0) | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS (3 cand) |
| 6 | (8,9)   | HE[8]/p0/(0,0,1)   | (68,p15,(0,-0.6153,0.7883),37.972), (43,p9,(0,0,1),0)          | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS |
| 7 | (8,16)  | HE[44]/p9/(0,0,1)  | (32,p6,(0,0,1),0), (50,p11,(0,0,1),0)                          | 0.000°  | 0.000° | 0.000°  | TIE_WITHIN_ε |
| 8 | (9,10)  | HE[9]/p0/(0,0,1)   | (67,p15,(0,-0.6153,0.7883),37.972), (42,p9,(0,0,1),0)          | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS |
| 9 | (10,11) | HE[10]/p0/(0,0,1)  | (66,p15,(0,-0.6153,0.7883),37.972), (36,p7,(0,0,1),0), (54,p12,(0,0,1),0) | 37.972° | 0.000° | 37.972° | UNAMBIGUOUS (3 cand) |
| 10 | (11,16)| HE[40]/p8/(0,0,1)  | (35,p7,(0,0,1),0), (53,p12,(0,0,1),0)                          | 0.000°  | 0.000° | 0.000°  | TIE_WITHIN_ε |
| 11 | (11,16)| HE[58]/p13/(0,0,1) | (35,p7,(0,0,1),0), (53,p12,(0,0,1),0)                          | 0.000°  | 0.000° | 0.000°  | TIE_WITHIN_ε |

**Tie summary:** 7/11 cases "UNAMBIGUOUS" by ε=0.5° rule, 4/11 hard 0° ties.

**Critical observation that invalidates Algorithm A's premise:** The most-antiparallel pair angle in all 11 cases is either **0°** (parallel/identical) or **37.972°** (still acute; not antiparallel). **NOT A SINGLE CASE has a candidate near 180°.** The patches incident to these collision edges are all approximately co-oriented — 7/11 have all candidates with normal ≈ (0,0,+1), and 4/11 have one candidate with (0, -0.6153, +0.7883) which is still on the same +Z half-space.

**Patch decomposition** (from `[flood_fill DIAG Step5a]`, 16 per-face patches): 12/16 patches share `source = SourceFace { mesh_id, face_idx: FaceIdx(0) }` (one of the two cap faces of either operand). Step 5a's source-face split correctly separates A from B by source — but BOTH A's cap and B's cap project to the same z-plane, and the per-face triangle tessellation produces patches whose geometric outward normals are all ≈ (0,0,+1). The "collision" in twin-pairing is a **coplanar-face stacking** failure mode, not a non-manifold-meeting failure mode.

---

## §3 — Algorithm A viability verdict

**FAIL.**

Algorithm A's geometric premise — "the most-antiparallel pair forms the manifold seam" — assumes the colliding patches lie on opposite sides of a shared edge so that one normal points outward from material on one side and the other points outward from material on the opposite side (≈180° apart). On F0030's 11 collision-arm cases, this premise is violated by construction: the patches are coplanar (or near-coplanar) cap faces from BOTH operands, all stacked on the +Z side of the shared coplanar plane at z ≈ 0.27358. Their geometric outward normals are co-oriented, not antiparallel.

The 4 hard-tie cases (idx 2, 7, 10, 11) cannot be resolved by ANY deterministic geometric tie-breaker that reads only patch-normal angle, because the input data carries zero angular discrimination. The 7 "UNAMBIGUOUS-by-ε" cases are misleadingly named: the "winner" at 37.972° is geometrically meaningless because it's still on the same hemisphere as the loser at 0°. Picking the more-deviated normal as "the seam" is an arbitrary rule with no physical justification, and the patch it's pointing at (p15, B.face_idx 1 — the bottom cylinder face of B) is *not* the geometric twin of the cap face HE[4..10] live on (p0, A.face_idx 0 — A's cap).

The actual root cause living upstream is **Stage 5 manifold-flood not separating coincident coplanar A+B patches**: when A's cap and B's cap occupy the same plane, post-tessellation they share canonicalized vertices, the manifold-incidence test sees ≥4 incident sub-tris per shared edge (2 from A + 2 from B with reverse orientation post-flip), the manifold-edge barrier correctly fires (incidence ≠ 2 ⇒ barrier), but Step 5a source-FACE split then produces multiple per-face sub-patches per coplanar overlap, and Step 6 boundary extraction emits forward HEs from each of them. The collision is a **provenance-multiplication** symptom of upstream coplanar-overlap handling, not a non-manifold-meeting symptom.

Per `feedback_yang_only.md`, this is exactly the "if you can't explain why a test fails, don't change code to make it pass" boundary. Algorithm A would silently pick wrong twins for 7/11 cases and panic-or-leak for the 4 hard-tie cases. That would mask the real defect.

---

## §4 — Spec scope decision

**Recommendation for spec-writer-o: HALT PR-Y17-TWIN. ABORT-and-rescope per plan §Risks #2.**

Algorithm A is not viable on F0030. Per the plan ABORT condition: "if §3 = FAIL, halt PR-Y17-TWIN and re-plan algorithm choice."

**Refined PR scope candidates** (for team-lead to evaluate; NOT a self-recommendation):

1. **PR-Y17-COPLANAR (rename + rescope):** Yang 2025 §4.5.5 coplanar preprocessing is the actual fix anchor for F0030. The current `coplanar_preprocess.rs` (Stage 0 infrastructure) needs to detect A-cap × B-cap coplanar overlap PRE-tessellation and segment into A-only / B-only / shared-trimmed-surface regions per the paper's Section 4.5.5. This eliminates the provenance-multiplication at source, before Step 5/5a/6/7 ever sees stacked coplanar patches. CLAUDE.md already names this as Stage 0 infrastructure that exists but is incomplete for cap-face overlap. This matches the user's "follow yang completely" directive — the paper IS the spec for this case.

2. **PR-Y17-COPLANAR-ALT:** If the coplanar-preprocessing pre-tessellation route is too large, an alternative is post-Stage-5a coplanar-patch merge: detect that two same-canonical-plane patches from different operands share a closed boundary, merge them into a single shared trimmed surface (per the paper's prescription) before Step 6. This is downstream of where my probe ran but upstream of the collision arm.

3. **Algorithm B/C/D/E from the original Phase 1 algorithm survey:** These were geometric pairings using volume orientation, ray-cast direction, etc. Algorithm A's failure on F0030 is **specifically because the pairings are coplanar**, which is a degeneracy any purely-geometric tie-breaker (B/C/D/E) will also struggle with. None of the Phase 1 alternatives address coplanar-stacking; they all assume non-coplanar manifold-meeting. Recommend NOT picking any of B/C/D/E without re-running this canary against each of them — Algorithm A's failure mode is structural, not algorithmic.

4. **Bank F0030 + cohort under PR-Y17-COPLANAR; bank F0020 under PR-Y18-DOWNSTREAM and F0050 under PR-Y19-NORMALS-EULER as already planned.** F0030's defect class is now identified as "coplanar-cap stacking", distinct from the original "non-manifold twin-pairing" framing.

**Spec scope decision:** abort current PR-Y17-TWIN scope. Do NOT write a spec for Algorithm A. Team-lead should re-plan with one of (1)/(2)/(3) above, with (1) the strongest because it aligns with the paper.

---

## §5 — Self-canaried recommendation for sub-phase 0d implementer

**There will be no sub-phase 0d** under the current PR-Y17-TWIN scope, because §3 = FAIL. This section is therefore a recommendation for the *replacement PR's* implementer, conditioned on which of §4's options team-lead picks.

Per `feedback_adversary_recommendations_need_canary.md`: my recommendation here MUST cite empirical observation, not inference.

**Empirical anchor I verified (the only one cited in this memo):** the collision-arm at `topology_extract.rs:1138-1162` fires 11 times on F0030 spotlight, and the per-patch outward normals at those fire sites are co-oriented (not antiparallel), with all incident patches sharing approximate plane z=0.27358. This was observed via direct probe (now reverted). The collision arm IS the right diagnostic anchor — what's wrong is what would be done IN that arm, not where it lives.

**Recommendation, with self-canary backing each clause:**

- **For PR-Y17-COPLANAR (option 1):** the implementer's first canary should verify that Yang §4.5.5 preprocessing reduces F0030's `[topo-extract] summary` collision count from 11 to 0 (or at least visibly reduces). Probe site: `coplanar_preprocess.rs` post-state for F0030, and confirm Step 5a per-face patch count drops from 16 toward fewer (the redundant per-cap patches should be merged into a single shared trimmed surface). I have NOT empirically verified what `coplanar_preprocess.rs` currently does for F0030 — that's the implementer's pre-implementation canary, per the feedback rule.

- **For any other option:** the implementer's pre-implementation canary should be the same shape as mine: enable `TWIN_DEBUG=1` on F0030 spotlight, run, and confirm the proposed algorithm's discriminant variable (whatever replaces "patch-pair-normal angle") actually has discrimination > ε on the 11 collision cases. Mine took ~2h to instrument and run; theirs should take similar.

- **What I do NOT recommend:** I do NOT recommend sub-phase 0d as written, because it would burn an implementation cycle on an algorithm I just empirically falsified. I do NOT recommend a "fall back to Algorithm B" reflex without first re-running this canary against B's discriminant. Algorithm A failed for a structural reason (coplanar inputs) that may also defeat B/C/D/E.

---

## Verification

- `git diff` shows only `docs/audits/pr_y17_twin_canary.md` (this file). Probe was added then fully reverted; `cargo build -p kernel` clean post-revert.
- Probe was run with `TWIN_DEBUG=1 YANG_BOOLEAN=1 cargo test -p test-harness --test assay_randomized -- spotlight_f0030 --ignored --nocapture --test-threads=1`, completed in <30s.
- 11 collision cases enumerated; 11 corresponding `[algA-canary]` lines captured before revert.
- §3 verdict: **FAIL** (committed).
- §4 picks ONE: **HALT PR-Y17-TWIN, ABORT-and-rescope** (option 1 strongly preferred — Yang §4.5.5 coplanar preprocessing).
- §5 references empirical observation explicitly + self-canaries the recommended next-step probe.
