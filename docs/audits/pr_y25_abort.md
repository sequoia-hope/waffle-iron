# PR-Y25 ABORT — Option H1 antecedent empirically vacuous; bank for PR-Y26 (Yang §4.4.1)

**Date:** 2026-05-09
**Verdict:** **ABORT** at canary phase per spec acceptance gate ("simulated > 20 → ABORT")
**Anchor refuted:** Option (h) sub-option H1 — NMM-pair render-mesh vertex sharing keyed on `arena.constructed_directed_edge`
**Rightful PR-Y26 anchor:** Yang §4.4.1 mesh-updating (vertex unification between source-meshes A+B at the seam)

This abort is principled — the canary's discipline (per `feedback_anchor_before_fix.md`) caught a wrong fix shape BEFORE any production code was written.

---

## §1 Cycle artifacts (kept on main)

PR-Y25 produced one useful artifact:
- **`docs/audits/pr_y25_anchor_canary.md`** (commit `42674e2`) — empirical refutation of H1 with §D position-co-location probe template (reusable for future canaries to distinguish "defect AT named site" vs "elsewhere").

No spec, no test, no impl — Phase 0 acceptance gate triggered ABORT before those phases ran.

---

## §2 What canary-y25 found

### Headline
```
[yang-diag] flood_fill_patches: 39 unpaired HEs out of 169 total
[y25-probe-p1-diag] nmm_he_total=39 nmm_he_with_construct=39 nmm_directed_keys=39 keys_with_reverse_anywhere=0
[y25-probe-p1] total_nmm_pairs=0 ambiguous_pairs=0
[y25-probe-p2] actual_unpaired=36 simulated_watertight_unpaired=36 snap_targets=0 key_remap_size=0
[y25-probe-p2-d] nmm_he_at_unpaired_render_edge=0 nmm_he_at_paired_render_edge=39
```

### Key facts
1. **All 39 NMM HEs in F0020 b#2 are correctly plumbed** in `arena.constructed_directed_edge` (PR-Y24's plumbing is intact: `nmm_he_with_construct=39`).
2. **Zero of them have a reverse-direction counterpart** anywhere in the construction-time map (`keys_with_reverse_anywhere=0`). NMM HEs are directionally asymmetric per Yang §4.4.2 — single-direction-only.
3. **Zero NMM-pairs exist** in either bucket (`total_nmm_pairs=0`). H1's antecedent is vacuous.
4. **The 36 unpaired render-mesh edges sit at non-NMM-HE positions** entirely (`nmm_he_at_unpaired_render_edge=0`; all 39 NMM HEs sit at *paired* count=2 render edges).

### Cohort guard
F0044 batch invocation 3 (LOD=Render): 44 NMM HEs, `keys_with_reverse_anywhere=0` likewise. Confirms the directional-asymmetry pattern is structural, not F0020-specific.

---

## §3 Why Phase 1's "Diagnosis A dominant" was empirically wrong

The Phase 1 Explore agents posited:
- **Diagnosis A** (dominant per Phase 1): `tessellate_solid_bounded` doesn't share render-mesh vertex IDs across NMM-pair HEs → 36 boundary edges trace to ~30 NMM HEs.
- **Diagnosis B** (secondary): Yang §4.4.1 vertex unification missing → ~6 residual.

The canary's §D position-co-location probe inverts the ratio:
- 0/36 boundary edges trace to NMM HEs (Diagnosis A: 0 contribution)
- ~36/36 boundary edges trace elsewhere (Diagnosis B or third mechanism)

The Phase 1 numerical inference ("39 NMM HEs ≈ 30 of 36 boundary edges") was a structural assumption — IF NMM HEs flowed to render edges in a 1:1 way, they'd dominate. The empirical answer is they don't flow there at all; they sit at paired (manifold) render edges in F0020.

This is exactly the failure mode `feedback_validate_against_corpus.md` warns about: structural inference without empirical canary leads to wrong fix shapes. PR-Y25's canary caught it before code; PR-Y23's canary did not (PR-Y23 confirmed H1' mechanism but underestimated cohort blast radius).

---

## §4 Bank for PR-Y26

### §4.1 Re-anchor canary (REQUIRED before any PR-Y26 fix)

PR-Y26's first canary task is to enumerate WHERE the 36 unpaired render-mesh edges actually originate. Probe: walk all 36, label each by:
- Source face_idx (which B-Rep face emitted them)
- Singleton (count=1) vs supermanifold (count≥3)
- Position-co-location with: NMM HEs (canary §D template), source-mesh A vs B vertex set (Diagnosis B candidate), face-interior diagonal positions (earcut/Steiner-fan candidate)

This canary's §D probe is the template. Extend with face-id labeling and source-mesh A/B disambiguation.

### §4.2 PR-Y26 candidate anchors

Per canary §"banked findings" + plan §"Anti-scope" pre-bank:

| Anchor | Mechanism | Likelihood given canary §D | Scope |
|---|---|---|---|
| **(i) Yang §4.4.1 mesh-updating** | source-meshes A and B emit independent vertex positions at the seam after SSI refinement; not unified | HIGH (Diagnosis B inversion) | Stage 4a re-meshing in `ssi_refinement.rs:298-533`; high blast radius |
| **(j) Earcut/Steiner-fan diagonal mismatch** | per-face triangulation in `tessellate_solid_bounded` introduces interior diagonals that don't share with neighbors | MEDIUM | Per-face triangulation logic |
| **(k) Face-loop interior point insertion** | adding interior points (Yang §4.4.1 "we insert one point i into it") creates per-face vertices that aren't shared with adjacent face's CDT | MEDIUM | `update_mesh_along_refined_curves` interior-point logic |

PR-Y26 spec phase weighs these AFTER the re-anchor canary identifies which dominates.

### §4.3 PR-Y25's Plan-1 lesson (memory candidate)

The Phase 1 Explore agents' diagnosis ranking was structurally plausible but empirically wrong. A new feedback memory: "Phase 1 Explore agents' diagnosis ranking is inference, not measurement — canary the dominant diagnosis with a position-co-location probe before scoping the fix."

I will save this in §5 below as a memory addition.

---

## §5 Discipline notes

- Live tree at `/home/claude/workspace` clean throughout (verified via `git status` snapshots in canary memo §0).
- Probe worktree `/tmp/y25-probe-wt` retained at canary close per plan §"Phase 0"; will be removed at this close-out.
- No `git stash`/`reset --hard` on live tree.
- ZERO production code modified across the entire PR-Y25 cycle.

---

## §6 Recommendation to user

PR-Y25 is correctly aborted at canary phase. The canary memo + this abort memo are the only artifacts. Recommended next steps:

1. **Open PR-Y26** with a re-anchor canary that enumerates WHERE the 36 unpaired render-mesh edges actually originate (per §4.1). The §D probe template from PR-Y25 canary is the seed.
2. After re-anchor canary, the spec phase weighs Option (i) Yang §4.4.1 vs alternative anchors per the data.
3. Save the Phase-1-Explore-vs-canary lesson as a feedback memory (per §4.3).

Per `feedback_no_last_bug.md`: PR-Y26 addresses ONE layer at a time; do not bundle (i) with (j)/(k) without canary disambiguating which is dominant.

The team teardown (TeamDelete) closes this PR's cycle. PR-Y26 spawns a fresh team.
