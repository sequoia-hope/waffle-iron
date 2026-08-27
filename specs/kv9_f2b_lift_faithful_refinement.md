# Spec: the KV9-F2b fold family — refining until the chart→3D LIFT is faithful

**Status: FLIPPED ALWAYS-ON 2026-08-27 — NEW CANONICAL 272C/0W/35E/1EE/0T.**
`KV2_PATCH_LIFT_REFINE=0|off` is the dev off-knob. Anchored on R0017 (0.09 s
vehicle), which converts ERROR → SUPPORTED_CORRECT.

**Flip proofs (both full corpus runs, 312 cases).** Gate-off:
271C/0W/36E/1EE/0T, tracked `results.json` **byte-identical** to the committed
baseline (530.9 s). Gate-on: **272C/0W/35E/1EE/0T**, **exactly one category
move (R0017) and ZERO detail moves** — nothing else in the corpus is perturbed
at all — zero CORRECT regressions, marginally faster (521.6 s).

This is the **F2b** sub-family — explicitly recorded as separate and
UNANCHORED by `yang_434_output_chord_refinement.md` §1 ("R0017 (F2b) also
carries a 5.2× deep chord on its fold face; its fold mechanism
(all-on-surface 2D/3D inversion at r_unroll=4073) remains separate and
UNANCHORED — not this spec's customer"). F2a is that spec's customer and is
untouched here; §5 below gives the measurement that tells the two apart.

## 1. The anchor (R0017, and its same-development control)

`KV2_CHORD_DEPTH_CENSUS` + `KV2_PATCH_FOLD_PROBE`, both pre-existing.

Faces 14 and 17 of R0017 share the SAME development — identical
`w_facet = 3.604323e2`, `r_unroll = 4.072886e3`, `tan α = 5.767384e-1` (a 30°
half-angle cone). Face 14: `n_split=16`, `fold=0`. Face 17: `n_split=109`,
**`fold=inverted`** (`dot = -0.8051`). Same surface, denser refinement, folds.

The folded triangle is NOT a deviation defect: all three nodes sit exactly on
the ideal development (`dev` = 6.8e-13, 0, 0) and face 17's `max_split_dev`
(7.41) is SMALLER than clean face 14's (15.74). Two of the three nodes are
refinement `split` nodes; all three edges are `Interior`.

## 2. The refinement MINTS it — measured, not inferred (`KV2_PATCH_ASPECT_PROBE`)

New instrument: worst triangle aspect over the initial CDT and over the
refined mesh, measured in the **surface** metric (see §4). R0017, gate-off:

| face | splits | CDT worst | refined worst | fold |
|---|---|---|---|---|
| 14 (control) | 16 | 109.80 | **109.80** | 0 |
| 15 | 0 | 492.58 | 492.58 | 0 |
| 16 | 0 | 271.72 | 271.72 | 0 |
| 17 (anchor) | 109 | 204.33 | **3473.11** | inverted |

The control face proves the refinement CAN be quality-neutral on this very
development; the anchor face shows it degrading its input 17×. The sliver is
minted by refinement, not inherited from the CDT.

## 3. What the refinement is actually doing (`KV2_PATCH_MINT_PROBE`)

Second new instrument: for every split, the parent's and both children's
surface-metric aspect, printed when a child is materially worse. It names the
minting event directly instead of inferring it from the end state.

R0017 face 17, split **#38**, bisects a **needle** — base 17.03, sides 1078.5
and 1073.3 — and the child `[mid, nb, nc]` IS the folded triangle. Its
midpoint `(-742.0915, 5966.579)` is exactly node `b` of the fold probe's dump.

Crucially, **in the chart metric that bisection is correct**: parent aspect
66.8 → child 132.3, a factor of 2 — precisely Rivara's non-degeneracy bound
(finitely many similarity classes ⇒ min angle ≥ half the initial mesh's). The
LEPP walk is a faithful Rivara implementation and it is obeying its guarantee.

**The defect is not that the refinement is wrong. It is that its guarantee
does not control the fold.** Rivara bounds *chart* angles; the fold is a
property of the *lift* to 3D.

## 4. A REFUTED hypothesis, recorded (the chart is not isometric)

The first hypothesis was that the guarantee fails to transfer because the
working chart `(u, v) = (sense·θ·r_unroll, axial)` is isometric for a cylinder
and **not** for a cone — there `|∂P/∂u| = v·tanα/r_unroll` (which both differs
from 1 and VARIES with v) while `|∂P/∂v| = 1/cos α`. That non-isometry is
REAL, and it is why the anchor's triangle reads so differently by metric:

| metric | longest edge | height | aspect |
|---|---|---|---|
| working chart | 1073.26 | 8.114 | **132 : 1** |
| isometric development | 1225.62 | 1.598 | **767 : 1** |
| 3D chords (what is emitted) | 1225.61 | 1.986 | **617 : 1** |

But moving the refinement into the isometric development does NOT fix the
fold, in either form it can take:

- **Rank by the surface metric, keep bisecting at the chart midpoint.** This
  mixes two metrics, so the similarity-class invariant holds in neither.
  Measured: the previously-clean control face 14 degrades 109.80 → 590.99 and
  folds — a case that had never folded.
- **Rank AND bisect in the isometric development.** Converts R0017 and clears
  R0003's face-577 fold, but it is UNSOUND: the true developed midpoint lies
  OFF the chart-straight edge it splits, and the triangulation's combinatorics
  all live in the chart. Bending edges self-overlaps the patch — measured,
  R0032 regressed `UNSUPPORTED(curved-profile)` → `ERROR` with `mixed 2D
  orientation`, the chart-side fold tripwire. Constraining the split back onto
  the segment (bisecting it in arc length instead) restores R0032 and loses
  the R0017 fix, degrading face 14 to 590.79.

So the metric hypothesis is **refuted as a repair**. Its measurement survives:
`IsoDev::dist2` is retained as the metric the two probes above report in,
because measuring patch quality in the chart would have called the anchor's
767:1 sliver a benign 132:1. `IsoDev::mid` was deleted with the hypothesis.

## 5. The repair: refine until the lift is orientation-faithful

A chart triangle is lifted by taking its three corners onto the surface and
spanning them flat. The chords cut INSIDE the surface, so a triangle thin
enough relative to the sagitta of its own edges comes out facing inward. Until
now nothing upstream of the emit tripwire could prevent that: the refinement
was free to mint a folded triangle and the tripwire's only move was to fail
the whole patch.

`lift_inverts` adds that missing criterion to the refinement's work-queue test,
beside the existing Δu chord criterion. Bisection converges on it
quadratically — halving an edge quarters its sagitta while only halving the
triangle's height, so height/sagitta doubles per level.

**Bisection can only ever remove SAGITTA.** It cannot move a node that sits
off the ideal development, and a fold caused by such a node belongs to F2a.
The two are told apart by comparing the two quantities directly, with no tuned
constant between them:

```
dev = how far the nodes sit OFF the ideal development   (immovable)
sag = the ideal chart-lift sagitta of the edges         (removable)
refine only while dev < sag
```

`sag` is taken between IDEAL surface points so it stays independent of `dev`.
This also makes the arm **self-terminating**: `sag` falls quadratically under
bisection, so it crosses any fixed `dev` within a few levels and the arm
declines on its own.

Measured, this is exactly the F2a/F2b discriminator:

| case | face | dev | sag | dev/sag | verdict | splits off → on |
|---|---|---|---|---|---|---|
| R0017 | 17 | 6.8e-13 | 8.637 | 7.9e-14 | refine → **fold clears** | 109 → **114** |
| R0003 | 577 | 8.155e-2 | 1.597e-9 | **5.1e+07** | decline (F2a) | 5 → **5** |

The two cases sit 21 orders of magnitude apart on `dev/sag` — this is not a
close call the discriminator has to adjudicate, it is a clean separation.
R0003's face-577 node sits further off-surface (8.155e-2) than its triangle is
wide (8.1e-2); refinement cannot reach that fold, and the discriminator says
so for free. Without it, an earlier draft of this arm burned **28 104** splits
there and still failed. And the repair is cheap where it does apply: R0017's
fold clears on **five** extra splits (109 → 114), with `min_h2d` barely moved
(8.114 → 8.075) — it is not brute-force over-refinement.

**This is not a tolerance band.** The predicate is `the lift inverts`
(dot ≤ 0), not a tuned margin; it is strictly INSIDE the emit tripwire's own
−0.1 verdict; and it silences nothing — a triangle it declines, or fails to
fix, reaches the tripwire and fails loudly exactly as before. `du_floor =
w_limit/4096` is a convergence backstop against a chart-degenerate triangle,
not an accuracy band.

## 6. What is deliberately NOT changed

- **The Δu stop criterion** is untouched. It bounds Δθ, hence the chord
  sagitta, and is calibrated at `r_unroll = r_max`, so it is conservative at
  every smaller radius. It is a chord-accuracy criterion and was never the
  defect.
- **The LEPP metric and midpoint** are the chart's, exactly as before — see
  §4 for why.
- **The CDT** still runs in the working chart. It hands the refinement needles
  (the anchor's parent had a 17.03 base against 1075 sides); Rivara preserves
  needle-ness by design. Improving the CDT's own quality is a separate change
  with no measured customer yet.
- **F2a** (`yang_434_output_chord_refinement.md`) is a different mechanism and
  is not touched.

## 7. Tests

Six unit tests on `IsoDev` (`developable.rs` `mod tests`): cylinder identity;
same-generator distance = `Δv/cos α`; same-height short arc = `v·tanα·Δθ` (and
strictly shorter than the chart says — the varying u-scale); wrap-safety past
a half turn (both operations work from the RELATIVE Δφ, never an `atan2` of an
absolute developed angle); apex/other-nappe fallback; degenerate-cone
fallback.
