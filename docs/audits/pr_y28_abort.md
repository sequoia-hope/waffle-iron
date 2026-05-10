# PR-Y28 ABORT — D.1 mechanism precisely classified into 4 sub-mechanisms; no fix shape has empirical chain verified

**Date:** 2026-05-10
**Verdict:** **ABORT** at canary phase per canary-y28 recommendation
**Mechanism classified:** F0020 D.1 splits into 4 sub-mechanisms (D.1a/b/c/d) with quantitative weights
**Why no fix shape proposed:** the empirical chain `fix candidate → watertight unpaired_count 36→0` is unverified for every candidate; committing without that chain measured would be the 4th refuted structural inference

This is the **fourth consecutive canary-stage ABORT** (Y25/Y26/Y27/Y28). All four caught wrong fix shapes BEFORE production code. The canary is correctly refusing to commit prematurely.

---

## §1 The 4 sub-mechanism classification (canary's load-bearing finding)

F0020 final invocation: 33 arena faces → 25 render faces = 8 missing. Decomposed:

| Sub-mech | Faces | Mechanism | Code anchor |
|---|---|---|---|
| **D.1a** | 4 (face_idx 7, 25, 28, 31) | `boundary.len() < 3` planar entry gate (self-loops or 2-HE cycles) | `tessellation/mod.rs:3319` |
| **D.1b** | 1 (face_idx 29) | 3-vertex boundary with two coincident vertices → `is_convex=false` → earcut returns empty | `tessellation/mod.rs:3434+` |
| **D.1c** (DOMINANT — 48 of 51 emitted-then-dropped tris) | 2 (face_idx 8 with 12/12 NMM, face_idx 15 with 4/4 NMM) | All-NMM boundary; lost at F.0→F.1 in `remove_winding_insensitive_duplicates` | `tessellation/repair.rs:502-574` |
| **D.1d** | 1 (face_idx 26) | Single triangle removed by `remove_nonmanifold_topology_aware` at F.1→F.2 | `tessellation/repair.rs` |

Cohort cross-check: D.1 is F0020-specific. F0044/F0045/R0092 have **0** dispatch-loop dropouts (PR-Y27 cohort split D.1/D.2/D.3 reconfirmed).

## §2 Why ABORT instead of recommending β

The canary considered four fix-shape candidates:
- **α:** closed-loop filter at `topology_extract.rs:1119-1130` (~20 LOC)
- **β:** peer-patch synthesis (D.1c root) at `topology_extract.rs:~745-969` (~150-300 LOC)
- **γ:** pre-dedup conformal-merge in `tessellation/repair.rs:502-574`
- **δ:** canon-degen extension at `topology_extract.rs:~470`

For each, the empirical question was: does adopting this fix shape drive F0020 watertight `unpaired_count` from 36 to 0?

**α and δ:** by accounting argument, CANNOT reduce unpaired count. D.1a/b/d emit zero triangles already; removing them removes zero render geometry; the watertight oracle's count stays at 36.

**β:** unknown. Structural inference suggests it addresses D.1c (the dominant 48-tri cluster), but the chain "synthesize peer patches → unpaired 36→0" is not measured.

**γ:** unknown. Silent B-Rep face merge may or may not address the watertight oracle's count.

The canary's discipline: **adopting any of these without empirical measurement would be Y29 ABORT-in-spirit before it starts.** Better to ABORT cleanly and refine the canary scope.

## §3 Refined PR-Y29 scope (canary's recommendation)

**Inverse-direction probe.** Instead of asking "where do the 8 missing faces go missing?", ask "where do the missing-twin triangles come from?":

For each of PR-Y26's 36 count=1 unpaired edges in F0020:
1. Identify the missing twin triangle's spatial position (the would-be face)
2. Determine which SourceFace would have emitted it
3. Check whether that SourceFace maps to one of the 8 D.1 face_idx values {7, 8, 15, 25, 26, 28, 29, 31}

**Acceptance gate:**
- ≥80% map to D.1 set → **β (peer-patch synthesis) becomes the rightful anchor** with empirical backing; PR-Y29 specs against `topology_extract.rs:745-969`
- ≥25% but <80% → LAYERED; pick simpler one
- <25% → 5th refutation; the D.1/D.2/D.3 cohort split itself needs reconsideration

This inverse-direction probe is structurally different from prior canaries (which probed forward "where does the defect originate?"). It ties directly to the watertight oracle's predicate.

---

## §4 Banked findings worth carrying forward

1. **The α closed-loop filter is a hygiene PR.** It cleans arena-vs-pre-repair face parity (33→28) without affecting watertight count. Could ship standalone AFTER the load-bearing fix lands.
2. **D.1 sub-mechanism table** with verbatim probe data is in canary memo §1.1+§1.2. Reusable as PR-Y29 baseline.
3. **Arena-vs-render face parity diagnostic** (33 vs 25) — should harden into a cohort regression assertion (per PR-Y27 + PR-Y28 recommendation).
4. **Inverse-direction probe methodology** is a new tool for the canary toolkit; future PRs can reuse the inversion (defect-position → source-attribution rather than mechanism-search → defect-prediction).

---

## §5 Discipline notes

- Live tree clean throughout (verified via `git status` snapshots).
- Probe worktree `/tmp/y28-probe-wt` retained at canary close; will be removed.
- No `git stash`/`reset --hard` on live tree.
- ZERO production code modified across PR-Y28 cycle.
- Probes were 141 LOC additive to `tessellation/mod.rs`, all gated on `Y28_PROBE=1`. Default-off byte-identical.

---

## §6 Strategic context — four consecutive canary-stage ABORTs

| PR | Plan candidate(s) | Canary verdict | Production code shipped |
|---|---|---|---|
| Y25 | NMM-pair render sharing | REFUTED | 0 |
| Y26 | (i)/(j)/(k) — Yang §4.4.1, earcut, inner-loop | ALL REFUTED | 0 |
| Y27 | 7 drop sites (B.1-B.7, A.4, C.4) in flood_fill_patches | ALL REFUTED | 0 |
| Y28 | OPEN-ENDED — 4 sub-mechanism classification | NO FIX RECOMMENDATION (chain unverified) | 0 |

**The good:**
- 8+ candidates definitively refuted
- F0020 mechanism precisely understood: D.1 split into D.1a/b/c/d
- Cohort understood: D.1 (F0020-specific), D.2 (F0044/F0045), D.3 (R0092) are 3 distinct mechanisms
- Two new diagnostic tools earned (count=1 boundary classification, arena-vs-render face parity)
- Multiple banked refactors (α hygiene PR) ready to ship

**The hard:**
- Zero production code on F0020 in 4 PR cycles
- The inverse-direction probe (PR-Y29 recommendation) might also refute β/γ — there's no guarantee any of the existing candidates close watertight

**Strategic options for the user (recommended in §6.1):**

### §6.1 Three options

1. **PR-Y29 with the inverse-direction canary** (canary's recommendation). One more canary cycle; if β/γ get empirical backing, ship the fix; if not, escalate.
2. **Ship the α hygiene PR standalone** in parallel with PR-Y29. α is cleanly identified, won't fix watertight, but improves arena hygiene and reduces future canary noise. ~20 LOC, tight scope.
3. **Defer F0020 indefinitely; pivot Yang fast effort to other assay cases.** Yang fast is at 11/157. F0020 is one of those 11; locking it isn't a priority if the next 50 cases unlock easily. Could revisit F0020 after corpus-wide health is better understood.

The canary recommends option 1 (PR-Y29). Option 2 is parallel-safe. Option 3 is a strategic step-back worth user consideration given the 4-PR sunk cost.

Recommend bringing to user for scope decision.
