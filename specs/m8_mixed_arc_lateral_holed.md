# M8-mixed increment 2 — one-sided arc-chain insertion for chain-consuming (holed) laterals

**Milestone tag:** `M8-mixed` (same grep tag as the parent spec)
**Parent spec:** `m8_mixed_loop_coplanar_overlay.md` (this lifts its named sub-wall
`mixed-arc-lateral-holed`)
**Owner crate:** `crates/yang-rs` (Stage 0 `collect_mixed_crossings` /
`arc_lateral_opposite`)
**Assay targets:** R0021 R0026 R0051 (all three wall at probe
`mixed-arc-lateral-holed`, verified solo 2026-07-09)
**Status:** IMPLEMENTED

## 1. Goal

A mixed (Line+Arc) planar cap admitted to the Stage-0 §4.5.5 overlay walls
today whenever the overlap boundary subdivides one of its arcs AND that arc's
adjacent cylinder lateral carries inner loops (a window punched by a prior
boolean). The parent increment's propagation assumed the structured 2-arc
partial-strip lateral, whose tessellation pairs its two arc chains
index-for-index — so a split point had to be inserted into BOTH arcs (exact
axial projection). A HOLED lateral does not take the strip path: Stage 1
routes it through the KV14 unroll+CDT path (`tessellate_lateral_holed_cdt`),
which splices every boundary loop from the shared per-edge chains via
`loop_polyline`. There is no index-pairing constraint — an inserted chain
point is consumed automatically and conformally by the CDT boundary.

After this increment, the arc split points are inserted ONE-SIDED (into the
arc's own chain only) whenever the adjacent lateral is classified
**chain-consuming**, and the pair proceeds through the general overlay.

## 2. Parameters

No user-facing parameters. Internal inputs unchanged from the parent spec.
New internal classification, returned by `arc_lateral_opposite`:

- **Strip** — hole-free cylinder lateral whose outer loop is exactly 2 arcs +
  ruling segments (the structured partial strip). Insertion stays PAIRED
  (own chain + exact axial projection onto the opposite arc). Byte-identical
  to today.
- **ChainConsuming** — cylinder lateral with ≥1 inner loop (Stage-1 routes it
  to `tessellate_lateral_holed_cdt`), where every loop of the lateral is
  spliceable by `loop_polyline`: a single-edge full-circle/full-ellipse loop,
  or a multi-edge loop whose edges are all `LineSegment` / arc `Circle`
  (`start != end`) / `Ellipse` arcs. Insertion is ONE-SIDED.

## 3. Branch table

| # | Arc's adjacent lateral | Behavior |
|---|---|---|
| 1 | hole-free cylinder, structured 2-arc strip | PAIRED insertion (unchanged — parent spec branch) |
| 2 | cylinder with inner loops, all loops `loop_polyline`-spliceable | HANDLED — one-sided insertion, general overlay proceeds |
| 3 | cylinder with inner loops, some loop NOT spliceable (multi-edge loop containing a full-circle rim, or a `SurfacePair` edge) | LOUD wall `CoplanarFacesUnsupported`, probe `mixed-arc-lateral-holed` (unchanged tag — Stage 1 could not consume the insertion) |
| 4 | non-cylinder lateral (cone, torus, …) | LOUD wall `mixed-arc-lateral-not-cylinder` (unchanged) |
| 5 | hole-free cylinder, NOT the 2-arc strip (irregular Slice-D boundary) | LOUD wall `mixed-arc-lateral-unpaired` (unchanged — same one-sided mechanism plausibly applies via Slice D CDT, but no corpus case targets it; named follow-up) |
| 6 | no adjacent face carries the arc | LOUD wall `mixed-arc-no-lateral` (unchanged) |
| 7 | no arc split points found (crossing-free mixed pair) | `arc_lateral_opposite` never called (unchanged) |

## 4. Invariants

- **I1 (conformality, one-sided):** the inserted split point appears exactly
  once in the arc's shared chain; the cap's overlay triangles and the holed
  lateral's CDT boundary consume the SAME chain, so the pair's meshes stay
  vertex-conformal along the arc with no T-junction. (The strip's both-arcs
  constraint is a strip-tessellation artifact; the CDT path has no analogous
  count constraint.)
- **I2 (zero behavior change off-branch):** strip laterals (branch 1) take a
  byte-identical path; all-segment/disc/annular pairs untouched; walls 3–6
  keep their typed tags.
- **I3 (2-manifold or loud):** downstream stages/kernel-v2 validation
  unchanged — residual defects stop loud, never silently wrong (P9).
- **I4 (insertion validity):** the inserted point passes the Stage-1
  arc-chain override validation unchanged (radial sagitta band, sweep-range
  check, uniform-coincidence refusal, exact CCW ULP-twin tie-break).

## 5. Oracles

**Amendment 1 (2026-07-09, Test Phase measurement):** the KV14 holed-lateral
CDT is a boundary-only EARCUT with no triangle-quality bound — the unroll's
seam-ruling columns carry no intermediate samples, so it fans wall triangles
from the seam to the window corners (θ-span ~66° in the fixture; radial sag
1−cos(33°) ≈ 0.16 ≫ the one-chord sagitta 0.034). The fixture mesh
under-fills the analytic solid by ~15% while staying watertight and
topologically correct. This is PRE-EXISTING KV14 behavior (shipped Slice A/B),
not introduced here. Consequently: oracle 1's absolute-volume check is a
loose sanity band (0.75–1.02 × analytic, one-sided under-fill), and oracles
2–3 use a **delta volume**: V(union mesh) = V(fixture mesh) + V(box) within
3% — the fan sag is common to both sides and cancels, so the delta measures
exactly what this increment changes. Follow-up (named, out of scope):
`kv14-lateral-cdt-chord-bound` — sample seam/ruling chains or split wide
fan triangles so holed laterals honor the A14.3 one-chord sagitta bound.

yang-rs integration tests (`tests/m8_mixed_coplanar.rs`), same builders and
volume/watertight/orientation oracles as the parent spec:

1. **Fixture sanity (GREEN pre-change):** `windowed_half_cylinder` — a
   half-cylinder (r=1, z∈[0,2]) with a radial slot (x∈[−0.4,0.4],
   z∈[0.7,1.3]) cut through the flat wall: curved wall = 2-arc strip outer
   loop + window inner loop (2 arcs + 2 rulings) — the HOLED lateral; flat
   wall = rectangle with rectangular hole; 2 mixed notch faces. Meshes
   watertight/outward; volume = π/2·2 − 0.6·(0.4·√0.84 + asin 0.4) within
   the 5% chord band.
2. **Canonical (branch 2, RED→GREEN):** flush box partner on the windowed
   half-cylinder's mixed top cap CROSSING the semicircle arc away from the
   window's azimuth range (box x∈[0.5,1.5], y∈[0.1,0.6]; crossings at
   θ≈5.7°/36.9°, window spans θ∈[66°,114°]). Today
   `Err(CoplanarFacesUnsupported)`; after: union succeeds, watertight,
   outward, volume = V(windowed half-cyl) + V(box) within the chord band.
3. **Adversary (branch 2, crossing OVER the window):** box footprint
   x∈[−0.1,0.2], y∈[0.5,1.5] — both arc crossings (θ≈78.5°/95.7°) sit
   directly above the window, so the CDT boundary takes insertions whose
   unrolled u-columns pierce the hole span. Success + full oracle, or typed
   failure — asserted never silently wrong (volume checked on success).
4. **Regression (branch 1/7):** the parent suite (`m8_mixed_coplanar.rs`
   all tests) and `./scripts/test.sh rewrite` stay green — strip pairing and
   crossing-free admission byte-identical.
5. **E2E:** corpus R0021 R0026 R0051 flip UNSUPPORTED(coplanar)→CORRECT (or
   a typed downstream ERROR, never WRONG); full categorized assay, gate =
   zero lost CORRECT, 0 WRONG.

## 6. Failure modes

- **Non-spliceable holed lateral (branch 3):** admission would push the
  failure into Stage-1 `loop_polyline` as `MalformedTopology` (an ERROR-class
  miscategorization of a capability boundary). The classifier therefore
  verifies spliceability BEFORE admitting; failing loops keep the typed
  `CoplanarFacesUnsupported` wall + probe tag.
- **Insertion off the arc (bad overlay vertex):** Stage-1 override validation
  rejects loudly (`MalformedTopology`, I4) — unchanged from the parent.
- **Window straddling an insertion azimuth:** handled by the CDT's
  window-avoiding branch-cut selection (KV14 Slice B); oracle 3 pins it.

## 7. Research basis

- [#24] Yang et al. 2025 §4.5.5 (Fig. 16): one general 2D Boolean before
  discretization; overlap boundaries become intersection curves. The paper
  imposes no lateral-shape lattice; this increment removes one more
  restriction.
- [#24] §4.1/§4.4.1: shared exact per-edge boundary sampling is the
  watertightness mechanism. One-sided insertion preserves it precisely
  because the KV14 CDT lateral consumes the same chains
  (spec `yang_stage1_curved_holed_patch` Slices A/B).
- Livesu et al. 2021 / KV14: boundary-only CDT accepts arbitrary boundary
  vertices — no uniform-count constraint (the strip's `(N−k)` pairing is a
  structured-tessellation artifact, not a paper requirement).

### 7a. Analytical vs approximate

Method: exact 2D overlay on Stage-1 sample chains (mesh-as-exact-intermediate,
A15 hybrid corollary). Surface types survive unchanged (`Surface::Plane` cap,
`Surface::Cylinder` lateral). Surface pairs: plane×plane (the coplanar pair);
no new SSI. No mesh-as-final-representation.

## 8. Implementation sketch (Phase 3 contract)

1. `arc_lateral_opposite` returns `ArcLateralKind { Strip{…}, ChainConsuming }`
   instead of the bare strip tuple; holed-cylinder arm classifies loop
   spliceability (branch 2 vs 3) instead of returning the wall tag
   unconditionally.
2. `collect_mixed_crossings`: on `Strip` — existing paired insertion,
   byte-identical; on `ChainConsuming` — push split points into the arc's own
   `rim_overrides` entry only.
3. No Stage-1 changes (arc-chain override insertion + holed CDT already
   consume the chains).
