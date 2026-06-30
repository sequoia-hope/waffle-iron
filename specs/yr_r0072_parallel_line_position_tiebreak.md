# PR — Stage-3 SSI: position tie-break for parallel-line candidates (R0072)

**Roadmap:** M8 general §4.5.5 / same-normal campaign, Mode 3
(`crates/test-harness/tests/m8_samenormal_campaign.rs::red_r0072_stage3_ambiguous_parallel_lines`).

**Paper:** §4.3 intersection-curve selection. yang-rs builds the exact analytic
`Curve` per output intersection edge by selecting, among the `ssi_rs::intersect`
candidates, the unique one passing both mesh endpoints within the Stage-1 chord
band. `matched != 1` is a loud P9 stop (`AmbiguousCurve`).

## Defect

R0072 (two coplanar same-normal bosses on one base plane) unions to a
`plane ∩ cylinder` **secant** edge that is **near-tangent**: the plane grazes the
cylinder, so the two generator lines `ssi_rs` returns are **near-coincident
parallels**. Both pass `curve_contains_point` (the `line_amp = r/√(r²−d²)`
near-tangency band amplification inflates the effective tolerance), so
`matched == 2`. The existing multi-match discriminator (PR-KV9) compares each
candidate's **tangent direction** to the edge direction — but two *parallel*
lines have identical tangents, so its margin test never fires and the edge dies
`AmbiguousCurve { candidates: 2, matched: 2 }`.

Instrumented numbers (edge (2,143), `YANG_R0072_PROBE=1`):

| cand | dir | d(p_s) | d(p_e) |
|------|-----|--------|--------|
| 0 | (0.539,−0.349,−0.766) | 2.049e-5 | 2.003e-5 |
| 1 | (0.539,−0.349,−0.766) | 3.310e-5 | 3.357e-5 |

tol = 7.97e-6 (both admitted via `line_amp`).

## Fix — disjoint-interval position tie-break (parallel lines only)

After the PR-KV9 tangent pass, if `matched > 1` AND every matched candidate is a
`Line` AND all matched lines are **mutually parallel** (`|dir_i × dir_j| <
TAU_MODEL`), break the tie by **position**: the mesh edge lies on exactly one
generator, which is nearer to *both* endpoints.

For each matched candidate `i`, form the endpoint-distance interval
`[lo_i, hi_i] = [min(d(p_s),d(p_e)), max(d(p_s),d(p_e))]` (perpendicular
distance to the line). Select candidate `w` iff its interval lies **strictly
below** every other matched candidate's: `hi_w < lo_j ∀ j≠w`. Then
`matched = 1, matched_idx = w`.

This is **margin-free and scale-free**: the winner's *worst* endpoint must still
be closer than every rival's *best* endpoint, so the endpoints unambiguously lie
on `w`. If the intervals overlap — the generators are merged below the mesh
resolution (true tangency territory) — no candidate qualifies and the loud
`AmbiguousCurve` stands (P9: a proximity tie-break on geometry the on-both gate
already verified, never a band widening; ambiguity is preserved, not papered).

R0072: `hi_0 = 2.049e-5 < lo_1 = 3.310e-5` → select cand 0.

### Scope / non-regression

- Gated on **all matched candidates being mutually-parallel Lines** — the exact
  structural case the tangent discriminator cannot resolve. Non-parallel or
  conic multi-matches keep their current behavior (tangent pass / loud).
- Runs only after `matched > 1` survives the tangent pass, so single-match edges
  and tangent-resolved crossings are byte-identical.
- The on-both-surfaces gate (PR-YR18) and `line_amp` band are untouched — this
  only chooses among candidates already admitted.

## Scope of THIS increment (and what it does NOT close)

This PR fixes **Mode 3 only** — the `AmbiguousCurve` parallel-line deadlock — at
its two occurrences (Stage-3 selection + Stage-4 line relocation), via one shared
`select_disjoint_parallel_line` helper. Verified: R0072's Stage-3/Stage-4
`AmbiguousCurve { matched: 2 }` is gone.

R0072 is **not** fully oracle-correct after this PR: with Mode 3 resolved it
advances PAST the ambiguity and surfaces a **Stage-4 `DegenerateTriangle`**
(vertex 7) — i.e. it becomes a **Mode-2** case (§4.5.3 region repair / N2
mesh-updating), the same blocker as R0021. That is a separate, larger increment.
So the campaign test `red_r0072_…` stays `#[ignore]`d, its reason repointed at
Mode 2.

## Regression guard

Because R0072 stays RED (Mode 2), the Mode-3 fix is guarded by a dedicated
yang-rs lib unit test `tests::r0072_parallel_line_position_tiebreak` (disjoint →
select nearer; overlapping → None; non-parallel → None; <2 cands → None).

## GREEN target (this PR)

- Mode-3 `AmbiguousCurve` no longer reachable for parallel-line candidates
  (unit test green).
- No regression: campaign always-on tests stay green, and the assay
  (`assay_randomized`) shows no `SUPPORTED_CORRECT` loss and **no new
  `SUPPORTED_WRONG`** (the silent-wrong guard — the tie-break only fires with a
  disjoint-interval winner, so it cannot mis-select).
