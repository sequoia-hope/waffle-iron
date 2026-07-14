# Spec: R0008 — cone-apex crossing-generator SSI selection

## Goal

Resolve the `AmbiguousCurve { candidates: 2, matched: 2 }` Stage-3 wall on
R0008: a cutting plane passing through a cone's **apex** sections it into the
degenerate conic that is a **pair of generator lines crossing at the apex**.
`ssi_rs::intersect` correctly returns both lines; the mesh intersection edge
lies on exactly ONE of them, but both pass the on-curve membership test
(`curve_contains_point`) because the cone chord band `tol` is large (a very
flat cone: R0008 half-angle ≈ 88.95°, `tol ≈ 2.81`). Neither existing
multi-match discriminator fires:

- **Tangent discriminator** (`|cos| margin > 0.1`): both generators are nearly
  aligned with the edge near the apex-plane (their in-plane projections differ
  only by a small out-of-plane tilt), so the cosine margin is ≪ 0.1.
- **Parallel-line position tiebreak** (`select_disjoint_parallel_line`, R0072):
  bails because the two generators are **not parallel** — they cross at the
  apex (cross-product magnitude ≈ 0.037 ≫ `TAU_MODEL`).

## Mechanism

The edge endpoints lie on the true generator to within the mesh chord accuracy
(perpendicular distance ~0), while the false generator is a full band away
(R0008: ~2.6 vs ~0.009). The **disjoint perpendicular-distance interval** test
already used for parallel lines (R0072) is a pure *position* discriminator — it
makes no parallelism assumption and is sound for crossing lines as well.

## Parameters / branch table

Applies in `build_intersection_curves` (`stage3_ssi.rs`) after both existing
`matched > 1` discriminators, only when:

- `matched > 1` still holds, AND
- every matched candidate is an `SsiCurve::Line`.

Selection = the candidate whose endpoint perp-distance interval `[lo, hi]` lies
**strictly below** every rival's (`hi_winner < lo_j ∀ j ≠ winner`). No margin,
no scale constant.

## Invariants / oracles

- **Correctness:** on R0008's probed geometry the horizontal generator (edge
  z = apex z, direction z-component 0) is selected; R0008 advances past the
  Stage-3 wall. Unit oracle `r0008_cone_apex_crossing_generators_position_tiebreak`
  pins the two probed candidate lines + edge endpoints and asserts the general
  `select_disjoint_line_by_distance` returns the horizontal generator's index
  while `select_disjoint_parallel_line` returns `None` (its parallel gate still
  bails — contract preserved).
- **Byte-stability (P9):** the new block runs only when `matched > 1` survives
  both existing discriminators, which today raises `AmbiguousCurve` (an ERROR) —
  so it can only convert current ERRORs to resolved; no CORRECT case can
  regress. The R0072 parallel path is byte-identical (its wrapper still gates on
  parallelism, then delegates to the shared interval core).

## Failure modes

- Overlapping intervals (edge sitting at the apex crossing, both generators
  within chord accuracy) → no winner → the loud `AmbiguousCurve` stands (P9 —
  a proximity tie-break on geometry the on-both gate already verified, never a
  band widening).

## Research basis

Degenerate conic sections of a quadric cone [#1 Patrikalakis Ch.5]: a plane
through the apex yields a line pair. Analytical-primacy corollary (P8): the
exact section is these two lines; selecting the edge's generator by position is
principled, not a tolerance heuristic.
