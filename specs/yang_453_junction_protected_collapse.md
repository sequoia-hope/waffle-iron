# §4.5.3 junction-protected reversal collapse (yang-rs Stage 4)

Amendment to `specs/yang_pr_yr10_stage4_relocate.md` §3 step 3 (the §4.5.3
reversed-intersection sweep). Bug-fix cycle per FIP §8.

## 1. Goal

The §4.5.3 sweep must never remove an exact **curve-junction vertex** — a loop
vertex where the intersection curve CHANGES (the endpoint shared by two arcs of
two different conics, e.g. the corner where two plane∩cylinder ellipse sections
of adjacent prism faces meet). Junctions are the exact endpoints of the
intersection curves; Stage 4 relocates them in closed form onto BOTH curves
(`vert_ell_junction` / `vert_circle_junction` / `vert_junction`). Removing one
merges arcs of two different curves into a single output edge, whose single
representative `Curve` cannot contain the surviving endpoints — the kernel-v2
import then rejects loudly:

```
InvalidBooleanOutput("output ellipse-arc endpoint does not lie on its ellipse")
```

Measured on R0011 (`KV9_JUNCTION_PROBE` + `YANG_V_PROBE`, 2026-07-08): the
revolve-cylinder × gear-prism union relocates interior vertex 16 onto its
single incident ellipse E16; the projection lands past E16's junction endpoint
(vertex 28, exactly on both E16 and the adjacent section E_out), producing a
genuine §4.5.3 reversal at p_r=16 — and the sweep then collapses p_n=28 (the
junction) onto 16, cascading through all seven junctions of the loop. The
output edge (16,26) spans multiple sections; endpoint 16 sits on E16, off the
representative curve by the junction offset (out-of-plane 8.6e-2 at radius
6.5e3, band 6.5e-6).

## 2. Parameters

None (no new user-facing inputs; internal control flow of
`sweep_reversed_intersections`).

## 3. Branch table

Reversal detected at `p_r` (per `is_reversed`, which already returns healthy
when the two edges at `p_r` carry DIFFERENT curves — PR-KV11):

| # | `p_n` is a curve junction (curve(p_r,p_n) ≠ curve(p_n,p_after)) | Action |
|---|---|---|
| 1 | no  | collapse `p_n` onto `p_r` (paper default — UNCHANGED behavior) |
| 2 | yes | collapse `p_r` onto `p_n` (the junction survives; the reversed point is `p_r`, whose relocation overshot the exact end of its curve) |

No third branch: `is_reversed` returning true implies the two edges at `p_r`
carry the SAME curve, so `p_r` itself is not a junction.

## 4. Invariants

- I1: after the sweep reaches its fixed point, every vertex that was a curve
  junction of two different conics before the sweep is still present in some
  surviving triangle (junction positions are exact; they are never victims).
- I2: branch 1 inputs (no junction adjacency) produce byte-identical results to
  the pre-fix sweep.
- I3: the collapse victim in branch 2 lies on the SAME curve as the junction
  survivor (edge (p_r,p_n) carries that curve), so the merged edge chain stays
  on one conic — output edges keep endpoints on their stored `Curve`.

## 5. Oracles

- Unit (branch table): `reversal_collapse_direction` returns `(p_n, p_r)` for
  same-curve `p_after`, `(p_r, p_n)` for different-curve `p_after` (both
  branches exercised; mutation-inverting the comparison must fail the tests).
- Corpus trackers (RED → GREEN): R0009 / R0011 / R0091 replays must not carry
  `"does not lie on its ellipse"` nor `"does not lie on its circle"` in their
  boolean-failure sets (the failure-moved analog for circle junctions).
- Regression: full `yang-rs` suite; `m8_swiss_cheese_chain` chain suite
  (the sweep is shared by the M8 fold-gate paths); F0086/F0087 corpus replays
  unchanged.

## 3b. Second mechanism (R0091 + R0009): §4.4.1(b) merge survivor selection — DIAGNOSED, BANKED-UNWIRED

> **Status (2026-07-08): the ranked-survivor fix below is implemented as the
> banked primitive `sub_feature_merge_direction` (+ unit tests, mutation-
> killed) but is DELIBERATELY NOT WIRED at the (3c) merge call site.** Wiring
> it clears R0091's ellipse-endpoint wall but flips the case ERROR →
> SUPPORTED_WRONG: the completed subtract tessellates to χ = −4 against the
> meta's euler_target 2, with watertight/volume/monotonicity/bbox all
> passing. χ = −4 (three handles, one shell) could not be verified OR refuted
> in-session: the meta χ is the naive 3-op default (`compute_euler_target`
> returns 2 for ≠2-op cases), but op 1 is a PARTIAL 219° circle-revolve
> (genus 0 sausage, not a torus), so the honest handle count needs a real
> derivation. Precedent: the world-space canonicalization pass stayed
> unwired while it flipped any case to SUPPORTED_WRONG (roadmap §0.2 item 1
> bullet 3). UNBLOCK PATH: verify the R0091 output's true χ via the Cherchi
> sidecar reference parity (roadmap §6) or refute the meta χ from the
> authored numbers (the R0078/C0035-F1 authoring-error protocol); then wire
> the ranked survivor and un-ignore the R0091 tracker.

The same output signature has a second producer, measured on R0091
(`YANG_V_PROBE`, 2026-07-08): the Stage-4 §4.4.1(b) sub-feature merge picks its
collapse survivor by LOWER INDEX. At micro model scale the merge legitimately
fires (features below the A14.2 floor are unrepresentable), but when the pair
is (exactly-relocated conic endpoint, plain chord vertex) and the chord vertex
has the lower index, the EXACT vertex is destroyed: the conic edge's surviving
endpoint is the unrelocated chord vertex (R0091: v15 on the ellipse exactly,
merged into v8 — a plane∩plane triple point 8.1e-8 off the ellipse; the
post-merge recompute assigns the merged edge (8,14) the ellipse, and kernel-v2
rejects endpoint 8).

Yang Fig. 11(b) ("if an endpoint p of the split edge is too close to q, we
merge p with q", `refs/text/yang2025_hybrid_boolean.txt` §4.4.1) merges INTO
the existing exact intersection point — q survives. Survivor selection must
rank exactness:

| # | rank(u) vs rank(v) | Survivor |
|---|---|---|
| 1 | equal | lower index (UNCHANGED — byte-identical to pre-fix) |
| 2 | u higher | u |
| 3 | v higher | v |

with rank: 2 = closed-form junction vertex (`vert_ell_junction` /
`vert_circle_junction` / `vert_junction` — exact on TWO curves),
1 = single-curve conic endpoint (the `conic_endpoint` scan set),
0 = plain mesh vertex. A plain vertex merged into a conic endpoint moves by
less than the feature floor (definitionally the same point, A14.2); its
incident plane∩plane `LineSegment` edges have no positional membership check
(endpoints implicit), so no counterpart wall exists on the line side.

### Additional oracles

- Unit: `sub_feature_merge_survivor` branch table above (equal-rank keeps the
  index rule; higher rank survives regardless of index order — both argument
  orders exercised).
- Corpus: R0091 tracker (spec §5) is the RED for this mechanism.

## 6. Failure modes

- Branch 2 with `collapse_vertex` dropping zero triangles → existing loud
  `Stage4ReversalUnresolved` STOP (unchanged).
- A reversal whose BOTH neighbors are junctions of other curves cannot occur at
  `p_r` (PR-KV11 guard); if the loop degenerates below 3 vertices the existing
  `LoopTooSmall` STOP fires.

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.3, Fig. 15
  (`refs/text/yang2025_hybrid_boolean.txt:709-745`): "p_r is a point on the
  intersection curve C between the two surfaces S_A and S_B" — the reversal
  test and the p_n removal are defined for consecutive points progressing
  along ONE intersection curve. A vertex where the loop transitions between
  curves is an intersection-curve ENDPOINT, outside the correction's scope;
  removing it destroys exact topology rather than repairing point order.
- Deviation record: this was a paper-faithfulness bug in the PR-YR10 port
  (the sweep treated whole conic loops as one curve for victim selection,
  though `is_reversed` had already been junction-guarded for the TEST in
  PR-KV11).
