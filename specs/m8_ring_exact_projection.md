# M8 — Exact frame projection in Stage-0 ring triangulation

**Status:** implementing (2026-07-10)
**Crate:** `yang-rs` (`src/stage0.rs`, `triangulate_ring`)
**Corpus targets:** F0068, F0069, C0075 (`build-mesh-triangulate` stalls);
class = the `m8_nonstar_ring_earclip` spec's "Measured residue" (femto-twin
runs), re-diagnosed 2026-07-10.

## 1. Goal

`build_stage0_mesh` re-tessellates the neighbor faces of a coplanar pair,
splicing propagated boundary split points into each face ring, and
triangulates the ring via `triangulate_ring` (verified apex fan → centroid
fan → closed-containment exact ear-clip, spec `m8_nonstar_ring_earclip`).
Rings on chained-input models carry consecutive vertices separated by
1–2 ULP (distinct exact 3D points — the overlay minted one subdivision
point per femto-tied sweep event column). Today `triangulate_ring`
projects ring vertices into the face frame with **f64 dot products** and
lifts the *rounded* result to exact rationals. The f64 rounding aliases
the femto twins onto ONE bit-identical 2D point, so the exact 2D ring
carries a zero-length edge; every fan triangle through the pair has exact
area ≤ 0, the centroid fan hits an exactly-zero cross, and the ear-clip's
strict-positivity/closed-containment tests reject every candidate → `None`
→ the pair walls loud (`build-mesh-triangulate`), even though the true
ring is simple and trivially triangulable.

Measured (F0068, f=207, normal `[0.6687, 0.7435, 0.0]`): 3D twins
`(-0.09689736564471349, 0.22356710025888266, z)` vs
`(-0.0968973656447135, 0.22356710025888268, z)` both project to f64
`(u,v) = (1.0167700011240253, -0.22154331376205785)` — bit-identical.

The fix: compute the projection **exactly** — lift each f64 coordinate
and each (fixed, f64) basis component to rationals and evaluate
`u = p·e1`, `v = p·e2` in `RBig`. Distinct 3D vertices then project to
distinct exact 2D points (the frame map is an injective affine map of the
face plane), and the existing exact fan/ear-clip machinery triangulates
the femto zigzag like any other subdivided chain. No tolerance is
introduced or widened; no coordinate moves; the basis does not need to be
exactly orthonormal — any fixed nondegenerate frame gives a faithful
projection, and all orientation/coverage decisions are made consistently
inside that one frame.

## 2. Parameters

Unchanged: `ring: &[u32]`, `verts: &mut Vec<Point3>`, `normal: [f64; 3]`.
No new inputs, no new tolerances.

## 3. Branch table

| # | Ring configuration | Before | After |
|---|---|---|---|
| B1 | Well-separated vertices (every current GREEN ring) | triangulates | triangulates (same strategy ladder; triangulation may differ only where f64 rounding was already within one ULP of a decision boundary) |
| B2 | Consecutive femto twins, f64 projection ALIASES them (F0068 f=207) | `None` stall | triangulates: twins stay distinct exact 2D points |
| B3 | Consecutive femto twins, f64 projection keeps them distinct but scrambles collinearity | fan rejects / ear-clip stall possible | faithful exact geometry — triangulates iff the true projected ring is simple |
| B4 | Genuinely degenerate ring (exact zero area, < 3 distinct verts) | `None` | `None` (unchanged loud wall) |
| B5 | Non-finite coordinate | `None` (f64 lift fails) | `None` (rat() fails) |

## 4. Invariants

Unchanged from `m8_nonstar_ring_earclip`: I1 (no chord over a split
point — every ring sub-segment is an edge of exactly one emitted
triangle), I2 (strict exact positivity), I3 (exact coverage certificate),
I4 (no new vertex except B2's centroid), I6 (determinism).

New:

- I-EP1 (projection faithfulness): two ring vertices with distinct 3D
  coordinates never alias to one 2D point unless their exact projections
  coincide (possible only for displacement exactly along the plane
  normal, which cannot occur for two points of one planar face ring).
- I-EP2 (no tolerance): the projection is an exact rational evaluation of
  fixed f64 basis vectors; no epsilon appears.

## 5. Oracles

- Unit RED→GREEN: `f0068_lateral_ring_femto_twins` — the exact corpus
  ring (6 verts incl. the 1-ULP twin pair, corpus normal). RED: current
  code returns `None`. GREEN: `Some(tris)` with I1 boundary tiling over
  all 6 sub-segments, I2/I3 via the function's own certificates, plus an
  explicit exact-area > 0 re-check per triangle in the exact frame.
- Unit (B1 guard): existing `triangulate_ring` unit suites in
  `stage0.rs` stay green unchanged.
- E2E: F0068 / F0069 / C0075 lose the `build-mesh-triangulate` Stage-0
  wall (success or a DIFFERENT typed error both pass — the honest next
  wall may be downstream).
- Full assay: 0 WRONG, no SUPPORTED_CORRECT lost.

## 6. Failure modes

- A ring whose faithful exact projection is genuinely non-simple still
  stalls loud (`None` → `build-mesh-triangulate`) — correct residue.
- Exact-arithmetic cost: rings are small (≤ a few hundred verts); the
  rational dot products add 6 multiplications per vertex — negligible
  against the existing exact fan/ear-clip work.

## 7. Research basis

The strategy ladder and its certificates are unchanged
(`m8_nonstar_ring_earclip`: two-ears theorem, Meisters 1975; closed
containment per [#39] Livesu et al. 2021 family). This change only
replaces a rounded projection with an exact one — the standard exact
decision-procedure discipline used across Stage 0 ([#9] Cherchi 2020
indirect-predicates philosophy: never let a rounded intermediate decide
an exact predicate).

## 8. Method declaration (FIP §3.2.7a)

Exact (rational) evaluation; no mesh/polygon approximation introduced.
No SSI surface pairs involved (planar face rings only).
