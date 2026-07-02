# M8 — Cross-solid shared-boundary identity (femto-twin elimination)

**Status:** spec DRAFT (FIP Phase 1) — design recorded 2026-07-02; RED/GREEN in a
follow-up session. **Change class:** bug fix (modeling-related), M8 workstream —
the unified root behind mechanisms 2b + 4 of the wall decomposition.
**Crate:** `yang-rs` (`stage0.rs` snap phase; `coplanar_overlay` stays a pure
exact engine).

## 1. Goal

Yang §4.5.5 / Fig. 16: after the 2D Boolean, "the common part and the other two
parts **share identical sampling points on their boundaries**." Today Stage-0's
symbolic reconciliation covers (a) the plane itself (snap onto the canonical
pair plane) and (b) bit-exact-matching corner vertices (the cross-weld). What it
does NOT reconcile: **near-coincident boundary geometry between A's and B's
chains** — a B edge collinear with an A edge up to femto noise (chained-output
rounding, the same class the plane snap absorbs in the normal direction).

The exact overlay then arranges TWO near-coincident constraint segments where
the paper has ONE shared boundary, and every crossing against them is minted
TWICE, femto-apart ("femto-twins"). Measured consequences (probes 2026-07-02):

- Subdivided neighbor rings carry consecutive femto-twin vertices
  (~1e-18–1e-16 apart) → the exact ear-clip stalls loudly
  (`build-mesh-triangulate`): R0046, R0070, F0061 (F0063 same signature).
- Long-thin overlay needles (twin short edge + remote third vertex) →
  `RoundingCollapse` genuine-sliver rejects: R0076 (area2 ≈ 1.6e-17),
  R0081 (≈ 3.6e-21); both twin pairs share one exact coordinate and differ
  ~3e-17 in the other.
- Inside cherchi: near-coincident cross-input vertices → degenerate
  arrangement triangles → `LabelMismatch { seed: 0, .. }`: R0088 layer 1,
  R0070 second op (the `kernel_v2_m8_coplanar_landscape` 1-ulp finding, R0015).

One geometric crossing must be minted ONCE — this closes ~6 of the remaining
14 coplanar walls at the root, without any downstream tolerance (the
prohibited weld class — gear-flange banked lesson).

## 2. Design direction (to be finalized at RED time)

Extend the Stage-0 snap phase (after the plane snap + corner cross-weld,
before `face_polygon_2d_tessellated`):

1. **Near-collinear cross-solid edge nomination** (f64 nomination, exact
   verification — the KV8c discipline): for each pair (A-edge, B-edge) of the
   two faces' loops in the pair frame, nominate when both B endpoints lie
   within the pair `band` of A's line AND the runs overlap along A's
   direction.
2. **Exact snap**: project each nominated B endpoint exactly (rational) onto
   A's exact 2D line; replace B's vertex coordinate with the lift of the
   projection. After the snap the two runs are EXACTLY collinear, and the
   overlay's existing exact collinear-overlap machinery (edge splitting +
   shared-sub-segment dedup, YR25 step 1) produces ONE constraint per shared
   run — crossings mint once.
3. **Corner priority**: the existing corner cross-weld runs first; a B vertex
   welded to an A corner is not re-snapped. A B endpoint within band of TWO
   non-parallel A lines is a corner-region case: snap to the exact line×line
   intersection iff within band of it, else loud (`CoplanarFacesUnsupported`,
   typed residue — never a guess).
4. **Determinism**: A's geometry is canonical (mirrors the plane snap: face
   A's plane wins); nomination scan in loop order; exact outcomes only.

Non-goals: no downstream weld (ear-clip, cherchi, Stage-6 all unchanged); no
tolerance added anywhere below Stage-0 (the pair `band` is the ONE existing
scale-relative constant, reused).

## 3. Branch table (draft)

| # | Configuration | Behavior |
|---|---|---|
| B1 | B edge near-collinear with exactly one A edge, runs overlap | Snap both B endpoints onto A's exact line |
| B2 | B endpoint within band of two non-parallel A lines | Snap to exact line intersection iff within band; else loud typed wall |
| B3 | B edge near but runs disjoint (no overlap along direction) | No snap (parallel offset features are ≥ MIN_FEATURE_SIZE apart — unreachable; guard anyway) |
| B4 | Bit-exact already-collinear (post-plane-snap common case) | Snap is the identity — byte-identical path |
| B5 | A/B swapped roles | Same rule; A canonical by pair definition |

## 4. Invariants (draft)

- I1: after the snap, for every shared boundary run there is exactly ONE
  overlay constraint line; no two overlay vertices within the pair band of
  each other on that run (twin-free).
- I2: snap displacement ≤ band (verified exactly); vertex NEVER moves when
  already exactly on the line.
- I3: pairs with no near-collinear cross-solid runs are byte-identical.
- I4: full assay `SUPPORTED_WRONG == 0`; trackers R0046/R0070/R0076/R0081/
  F0061/F0063 (+R0088 layer 1) progress past their femto-twin walls.

## 5. Oracles / 6. Failure modes / 7. Research basis

To be completed at cycle start (unit fixtures: two femto-crooked collinear
runs → one constraint; corner-region B2 both arms; the R0076 needle as an
E2E regression). Research basis: Yang 2025 §4.5.5 Fig. 16 [#24] (identical
boundary sampling); the plane-snap precedent (PR-YR26 §1); exact snap =
rational projection (no published-algorithm gap — representation hygiene).
