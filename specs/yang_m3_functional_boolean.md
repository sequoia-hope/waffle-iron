# Spec: M3 (+M4) — yang-rs first functional watertight boolean

**Status:** active (roadmap `docs/yang_functional_roadmap.md` M3 + M4)
**Feature cycle:** yang-m3
**Roles (P5):** Spec Writer = Manager; Test Author and Implementer are distinct agents.

## Goal

`yang-rs::boolean()` must produce a **watertight, 2-manifold B-Rep** result by
consuming the real `LabeledArrangement` (M2) — replacing the YR3/YR4 spatial-match
+ majority-vote substitute. This is the first end-to-end functional boolean of the
rewrite. Bundles **M4**: demote the now-unused substitute functions to a
`#[cfg(test)]` differential oracle (the lint gate denies dead code; roadmap rule
#9 says keep, don't delete).

## Scope

**In:** interpenetrating convex planar solids with **corner/edge-clipping**
overlap (no face fully interior-pierced). Canonical case: unit cubes A@[0,0,0],
B@[0.5,0.5,0.5] (diagonal offset ⇒ no coplanar faces, no interior pierce).
**Out:** coplanar overlap / multi-solid surface labels (M8 — error loudly);
faces with inner loops from interior-pierce (PR-YR5c); curved surfaces / SSI
refinement (M5); native arrangement backend (M6).

## Data flow (the rewrite)

`boolean(a, b, op, backend)` (signature unchanged):
1. `la = backend.labeled_arrangement(a.as_mesh(), b.as_mesh())?` — full arrangement
   mesh + per-tri `surface`/`inside`/`patch`. Assert **I6** (welded mesh).
2. `kept = la.keep_set(op)` — Stage 4 face survival.
3. Build a **compact kept sub-mesh** (re-index kept tris' verts). Output mesh.
4. **Geometric face resolution** per kept tri (Stage 6 attribution).
5. `reconstruct_topology(kept_submesh, full_attribution, a, b)` — unchanged;
   full attribution (no `None`) ⇒ closed boundary cycles ⇒ watertight 2-manifold.

## Branch table

Op selection is `LabeledArrangement::keep_set` (already shipped, cites
`booleans.cpp` rules): Union `inside.count()==0`; Intersect
`(surface ^ inside).count()==num`; Subtract per A-minus-B two-branch; Xor union∨inter.

Geometric face resolution per kept triangle `t`:

| # | Condition | Action |
|---|---|---|
| F1 | `surface[t] == [InputId(k)]`, exactly one face `F` of solid `k` has `|n_F·c+d_F| < TAU_WORK` | attribute `(k, F)` |
| F2 | `surface[t].len() >= 2` (coplanar multi-solid) | `Err(FaceResolutionFailed)` — out of scope (M8) |
| F3 | zero faces within `TAU_WORK`, or ≥2 tie within `TAU_WORK` | `Err(FaceResolutionFailed { tri, .. })` — loud, never `None` (P9) |

`c` = triangle centroid. `Surface::Plane{normal,d}` convention `n·x + d = 0`. Use
`TAU_WORK` (1e-12), not `TAU_MODEL`: M2 evidence shows bit-exact coplanarity for
this scope (multiplier=power-of-2 ⇒ no rescale drift); a looser tol risks a false
double-match. No `None` fallback — that reintroduces the non-manifold skeleton.

## Invariants

- **I6 (welded):** the arrangement mesh has no two distinct vertex indices with
  coincident coords (verified empirically for diagonal cubes: 22 unique verts).
  yang's index-based adjacency depends on this; assert it defensively.
- **I7 (unique face):** every kept triangle resolves to exactly one `(InputId,
  face)` (F1); else error.
- **I8 (watertight):** the output mesh is closed — every directed half-edge has
  exactly one opposite (0 unpaired).
- **I9 (signed volume):** signed volume `V = (1/6)Σ v0·(v1×v2)` over the output
  mesh equals the analytic value **with sign** (a wrong-winding subset yields
  neither +analytic nor −analytic, so this is a non-circular orientation test).
- **I10 (Euler):** `V − E + F = 2` over the reconstructed BRep (incident
  verts/edges/faces; genus 0).
- **I11 (surface tier, A15.5):** each output face retains its source input face's
  `Surface` (unmodified analytic faces stay analytic). Already what
  `reconstruct_topology` does.

## Oracles (numeric/structural — P1, DoD §1.2)

Canonical case A@[0,0,0], B@[0.5,0.5,0.5]; overlap = [0.5,1]³ = 0.125 m³:

| op | signed output volume (±TAU_MODEL) |
|---|---|
| Union | **1.875** (= 1 + 1 − 0.125) |
| Intersect | **0.125** |
| Subtract (A−B) | **0.875** (= 1 − 0.125) |

Plus I8 (watertight, 0 unpaired), I10 (Euler=2), and faces simply-connected
(no `NonManifoldOutput`). Oracle producer = the patched C++ sidecar (external,
not weakenable — P9); tests self-skip if the binary is absent.

## Failure modes

- Coplanar / multi-solid surface label → `YangError::FaceResolutionFailed` (M8).
- Centroid off all planes / tie → `FaceResolutionFailed { tri, .. }`.
- Backend without label support → default trait impl returns NotSupported →
  surfaces as `YangError::MeshBooleanFailed`.
- Inner-loop / interior-pierced face → existing `NonManifoldOutput` (PR-YR5c).

## M4 (bundled): substitute demotion + differential oracle

Move `match_with_input`/`match_against`/`face_candidates`/`majority_vote` +
`MATCH_TOLERANCE` into a `#[cfg(test)]` module (no longer production). Add a
differential test: real-label attribution and substitute attribution agree on a
fixture (disagreement localizes a label-path bug). Retain the existing YR3/4/5
unit tests against the now-test-only functions (rule #9).

## Research basis

- **Yang et al. 2025** §4.4.2 — Stage 4 face survival, Stage 5 flood-fill patch
  segmentation, Stage 6 B-Rep reassembly.
- **Governance A15.5** — surface tier preservation (I11); **A15.6** — hybrid
  pipeline stages (M3 implements 1/2/5/6; skips 0/3/4).
- **A14.3** — tolerances from cad-primitives (`TAU_WORK`, `TAU_MODEL`); no ad-hoc
  epsilon. **P9** — face resolution fails loud, no right-answer-wrong-reason.

## Definition of Done (DoD §1)

Spec (this file); RED→GREEN separate commits; every branch (ops + F1/F2/F3)
tested; numeric oracles (volumes) + structural (watertight/Euler/simply-connected),
not "no panic"; canonical (diagonal cubes) + degenerate (coplanar→error) cases;
determinism; M4 demotion leaves no dead-code warning (CI gate green); no test
weakened; clippy/fmt clean on new crates; siblings unchanged.
