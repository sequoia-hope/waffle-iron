# M8 slice g: disc / annular / mixed faces in n-ary plane groups

Status: IMPLEMENTING (task #132)
Driver: assay case R0046 — cylinder boss, rectangle cut through the cap
(splitting it into two MIXED Line+Arc pieces), then a revolve-circle cut
whose seam DISC is flush with both cap pieces. The subtract dies at the
slice-f scope wall `nary-face-unsupported` (stage0/nary.rs): the multi-pair
plane group contains faces that are not pure-`LineSegment` polygons
(`A0=mixed seg=1 circle=1`, `A1=mixed seg=1 circle=2`, `B0=disc`).

## 1. Goal

Extend the n-ary plane-group overlay (spec `m8_plane_group_nary_overlay`,
slice f) from pure-line faces to the tessellated planar classes the 1×1
path already supports: DISC (single full-circle rim), ANNULAR (outer rim +
hole rims), and MIXED Line+Arc faces. The group overlay gains, per face,
the same machinery the 1×1 general path wires pairwise:

- `face_polygon_2d_tessellated` polygons (exact Stage-1 rim rings /
  spliced chains; corner + rim key maps),
- rim-aware §2c coordinate clustering (rim sample domains excluded),
- rim/mixed chord mint contexts (`rim_chord_ctxs` / `mixed_chord_ctxs`,
  other-side segments = ALL other-side polygons of the group),
- overlay-vertex resolution: corners → rims → rim-ULP-snap → on-circle
  minting (exact circle∩line for transversal junctions, radial projection
  for pure x-events) → frame lift,
- sub-floor shared-mint collapse (slot space = every rim circle of the
  group),
- the fold-validity gate in reduced form (see B8),
- attribution-scoped per-face override triangulations + boundary edge
  splits (slice f, unchanged),
- per-face crossing propagation into laterals (`collect_rim_crossings` /
  `collect_mixed_crossings` → `rim_overrides`, newly threaded through
  `overlay_nary_group`).

User-visible: flush booleans whose coplanar plane carries several curved
cap pieces build correct solids (or fail LOUD at a deeper typed wall)
instead of the blanket Stage-0 residue.

## 2. Parameters

None user-facing. Same group inputs as slice f.

## 3. Branch table

| # | Branch | Behavior |
|---|---|---|
| B1 | group has 1 pair | existing 1×1 path, byte-identical (unchanged) |
| B2 | group ≥2 pairs, all faces pure-line | slice-f path, byte-identical (the tessellated collector's line arm delegates to `face_polygon_2d`; empty rim domains keep clustering identical) |
| B3 | group ≥2 pairs, faces ∈ {line, disc, annular, mixed} | tessellated n-ary overlay (NEW) |
| B4 | any face outside those classes (ellipse edge, non-planar, …) | loud `nary-face-unsupported` (unchanged wall text) |
| B5 | per-side mixed outward orientation | loud `nary-mixed-orientation` (unchanged) |
| B6 | a group pair where a DISC rim strictly crosses a hole rim of the other side's ANNULAR face | loud `annular-hole-rim-crossing` — the 1×1 wall (bore-lateral split propagation unproven), applied pairwise across the group's (face_a, face_b) combinations |
| B7 | crossing full-circle rims within a group (disc×disc lens etc.) | HANDLED — the 1×1 path already resolves crossing coplanar rims (`m8_disc_coplanar::disc_disc_crossing_union_succeeds`); the n-ary path inherits the same mint machinery, no new wall |
| B8 | fold gate | amendment-4 flips constrained to same (class, poly_a, poly_b) attribution + amendment-2 mint revert; the amendment-5/6 cavity relocation is NOT wired in this slice — an unflippable fold reverts (observable via kernel-v2's vertex-on-surface tripwire, P9-loud) |
| B9 | crossing collectors | disc/annular → `collect_rim_crossings`; mixed → `collect_mixed_crossings`; every typed collector failure (e.g. `rim-lateral-none` on a torus-profile rim, task #131) stays the loud pair error |
| B10 | zero total exact overlap | benign in-plane touch, unchanged (N17) |

## 4. Invariants

- I1–I6 of `m8_plane_group_nary_overlay` unchanged (coverage, attribution,
  identical overlap meshes, no 1-pair regression, conformal splits, group
  snap).
- I7 (rim conformality): a disc/annular/mixed face's override triangles
  reuse its exact Stage-1 rim ring / chain points bit-identically (shared
  with the adjacent lateral).
- I8 (exact mints): every chord-split vertex minted on a rim is ON the
  exact `Curve::Circle` (crossing branch: exact circle∩line; x-event:
  radial projection) — never left at its chord position unless the fold
  gate reverted it (then kernel-v2's tripwire keeps it observable).
- I9 (byte-identity): 1-pair groups (B1) and pure-line multi-pair groups
  (B2) produce byte-identical Stage-0 output to the pre-slice code.

## 5. Oracles

- yang-rs e2e (`tests/m8_nary_tessellated_overlay.rs`, RED first):
  - canonical pocketed cylinder (R0046's shape with a cylinder tool so no
    torus lateral is involved): cyl(r=2, h=2) − channel box(x∈[−0.5,0.5],
    z∈[1,2]) − flush coaxial cyl(r=1, z∈[1.5,2]) → the final subtract's
    Stage-0 group = {A mixed segment caps} × {B disc}; watertight, χ=2,
    exact analytic volume (circular-segment areas, rel 1e-9).
  - branch Union: same fixture, tool ∪ solid genuinely adds the tool's
    channel-void slice (delta = strip_area(r_tool, w)·depth).
  - 1×1 regression canary: the channel-free flush pocket (the supported
    disc∩disc containment class) keeps working (I9 at the e2e level).
  - disc×disc group: tool bottom disc flush over two boss tops — one pair
    crossing rims, one containment (B7) → watertight, χ=2, chord-band
    volume delta.
  - volume tolerances: the 6% chord band the m8_disc_coplanar suite uses
    (Stage-1 rim sag dominates; delta-form asserts cancel the untouched
    geometry's sag).
- stage0 unit (nary.rs): structural attribution oracle for the tessellated
  group — no duplicate triangles, disc face's override reuses its exact rim
  ring points (I7).
- assay corpus: R0046 re-censused (expected: deeper typed wall
  `rim-lateral-none` — its disc's lateral is a TORUS, task #131 — or
  CORRECT if its disc rim needs no propagation); full-corpus P9 gate
  zero-lost.

## 6. Failure modes

- B4/B5/B6/B7 walls: typed `YangError::CoplanarFacesUnsupported` naming the
  group's first pair, probe tags as listed.
- Collector failures (B9): the same typed pair error with the collector's
  tag (`rim-lateral-none`, `mixed-arc-lateral-*`, …).
- Downstream stages unchanged; anything they cannot resolve stays a loud
  typed error (P9-safe).

## 7. Research basis

- [#24] Yang, Jia & Yan 2025 §4.5.5 (Fig. 16;
  `refs/text/yang2025_hybrid_boolean.txt:716-760`): coplanar preprocessing
  segments the COPLANAR PLANE into A-only / B-only / overlap regions with
  identical overlap meshes; overlap boundaries become intersection curves
  carrying exact curve geometry. The disc/annular/mixed machinery is the
  1×1 implementation of that construction (specs `m8_disc/holed_disc/
  mixed_loop_coplanar_overlay`, `n2_stage4_junction_cluster_merge` §3);
  this slice applies it per face of the n-ary group — the set-level reading
  of §4.5.5 established by slice f.
- [#9]/[#38] Cherchi 2020/2022: downstream welding + keep-rules unchanged.

## 7a. Analytical vs. approximate method

Unchanged from the 1×1 path: rim geometry is exact (`Curve::Circle`
centers/radii, rational 2D predicates); chord mints are closed-form
circle∩line roots or radial projections (ULP-accurate), not tolerance
fits. The fold gate is a validity check, not a tolerance.

## 8. Ledger

- 2026-07-11: spec written; RED tests next (task #132).
- 2026-07-11: B6/B7 revised after fixture work — crossing full-circle rims
  are ALREADY handled by the 1×1 path (`disc_disc_crossing_union_succeeds`
  green), so the n-ary path admits them; only the annular-hole wall ports.
- 2026-07-11: IMPLEMENTED. All e2e oracles green ({mixed,mixed}×{disc}
  subtract/union partition, disc×disc crossing+containment group, 1×1
  canaries). Two PRE-EXISTING gaps discovered by fixture probing, out of
  slice scope:
  1. a PARTIAL-depth channel cut into a cylinder cap leaves an interior
     floor whose r=R arc chains tessellate non-conformally against the
     notched lateral (plain non-coplanar subtract of that operand already
     fails `NonManifoldOutput`; Stage-0 emission shows ~92 unbalanced
     edges at the floor plane) — slice-g fixtures use a full-height
     channel instead;
  2. a chained DISJOINT-lump union re-emits its rims as `LineSegment`
     polylines (no `Curve::Circle` vocabulary), so a later boolean dies at
     Stage-3 `chord_tol_for_curved_owner` producer fault — the fixture
     builds the two-lump operand directly.
