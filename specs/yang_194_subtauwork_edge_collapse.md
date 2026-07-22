# #194 — Sub-TAU_WORK mesh-edge collapse at emission (the F0082 Extrude-12 twin class)

**Task:** #194. **Driver:** F0082 Extrude 12 — the last remaining STOP layer
of the flagship case after #188 fixed the Extrude-11 union and #130's
close-out refuted the "M8 coplanar" attribution of this layer.

## 1. The class (measured 2026-07-22, `NONMANIFOLD_SITE_PROBE` + #188 spec §10.10)

A's own tessellation (the #188 seal-carrying body) self-grazes at the seal
corner. The exact arrangement then mints the SAME A-edge×A-plane junction
TWICE with swapped LPI roles (`line[A#925→A#986]×plane[A#1006,948,949]` vs
`line[A#949→A#948]×plane[A#925,986,967]`, both sin≈0.54): two verts
**5.5e-14 apart, connected by a mesh EDGE**, spawning a zero-area flap
`[972,971,977]` whose third use of the twin edge yields the χ=3 book edge /
double-cover reassembly STOP. Verified live today: twin
v971=(0.3094608389191706, 0.09202083071488087, 2.094303729583326) /
v972=(0.30946083891917336, 0.0920208307148765, 2.0943037295833804);
double-cover edges (930,931)/(930,934)/(931,971)/(932,994).

**Ownership gap (why nothing existing fires):**
- The I6/KV15 near-weld deliberately excludes curved-incident verts (the
  KV9 lens-tip record) — these twins are curved-incident.
- KV15b (`collapse_subresolution_intersection_segments`) is
  provenance-restricted to keys of `intersection_curves` — A×B junction
  pairs. An A-self-graze junction is not an intersection-curve key.
- Stage-4's KV9 collapse reconciles only THIS op's curve junctions.

## 2. Design

New emission-hygiene pass `collapse_subtauwork_mesh_edges` in
`stage4_correct.rs`, wired in `stage5_topology.rs` immediately AFTER the
KV15b block (own compact + Phase-A recompute when it fires):

- **Domain:** ALL undirected mesh edges (from `mesh.tris`), collected into
  a `BTreeSet` for deterministic order. No provenance restriction — the
  band does the scoping.
- **Criterion:** resolved edge length in the OPEN interval
  `(0, TAU_WORK·(1+scale))`, `scale` = max |coordinate| of the two
  endpoints (the KV15b idiom, five orders TIGHTER: TAU_WORK=1e-12 vs
  TAU_MODEL=1e-7). An edge below WORKING precision is not a representable
  segment — collapsing it is not proximity welding; it is refusing to carry
  two names for one point.
- **Survivor:** min resolved index, keeps its own bits (KV15b I1). No
  plane-incidence-richer position adoption (KV15b I1b) — at ≤6e-12 the
  positional difference is below working precision by definition.
- **No chain drift:** endpoints resolve through prior collapses and the
  RESOLVED length is re-measured against the band (KV15b I2/B5).
- **Exact-zero edges** stay untouched (the M-B emission-identification
  class, KV15b B3).
- Degenerate tris (the flap) drop via the shared `collapse_vertex`;
  `compact_unreferenced_verts` + `compute_phase_a` re-establish the
  emission invariants (the KV15b recovery pattern).

**KV9 safety:** the KV9 lens-tip record concerns UNCONNECTED coincident
ring duplicates — by construction this pass only sees pairs joined by a
mesh edge, so it cannot touch them. **KV15b non-interference:** KV15b runs
first at its wider band; any pair this pass sees is either not an
intersection-curve key or already collapsed.

## 3. Increments

- inc-0: this spec.
- inc-1: primitive + unit tests (twin-edge + zero-area-flap fixture
  collapses, min-index survivor bits kept; 1e-9 edge at unit scale stays —
  mutation canary against band-widening to TAU_MODEL; exact-zero edge
  stays; determinism). Wire always-on (KV15b precedent: tighter band than
  an already-always-on paper op; the corpus is the arbiter).
- inc-2: F0082 single-case (expect the χ=3/book-edge layer cleared — the
  case either greens or STOPs one layer deeper, LOUD either way), full
  release corpus (expect byte-identical outside F0082), tiers, ledger.

## 4. Ledger

- 2026-07-22: spec written; class re-verified live (twin @5.5e-14, flap
  `[972,971,977]` pattern present in today's probe).
- 2026-07-22 inc-1: primitive + 4 unit tests SHIPPED (twin+flap fixture;
  1e-9 supra-band canary — band-widening mutation verified kill;
  exact-zero B3; unconnected KV9 guard). First wiring at the
  stage5_topology emission block was measured DEAD for the driver — the
  s4-shell-euler gate fires UPSTREAM — so the production site is
  `stage4_relocate_and_correct` immediately before the (4b)
  `check_watertight_2manifold` gate (`collapsed_any` routes the caller's
  standard compact + Phase-A recompute); the stage5 site is kept for the
  all-planar path (which never enters stage-4 correction — the KV15b
  rationale).
- 2026-07-22 inc-2 (F0082 verdict): the pass fires EXACTLY on the specced
  twin (`victim=972 survivor=971 dist=5.487e-14`, measured) and the twin
  edge + flap leave the double-cover set ((971,972)/(972,994) →
  (971,994); Δχ=0 arithmetic checked: Δv=−1, Δe=−5, Δf=−4). **F0082
  stays LOUD at the same s4-shell-euler χ=3 gate — the residual layer is
  RE-CHARACTERIZED: six double-cover edges on REAL-length edges
  (930,931)@1.5e-3, (930,934)@8.2e-5, … — two DISTINCT triangles per
  directed edge in the seal neighborhood, i.e. operand self-OVERLAP at
  real scale** (the cap fan reaching the axis vert v935 vs the
  seal-region triangles), NOT a sub-resolution twin. No collapse may
  touch those (they are §10.10's "secondary near-dups" at 5e-5..1.5e-3 —
  the #146-family conformality scale); the loud STOP is correct until the
  producing op's seal-neighborhood emission is made overlap-free (new
  follow-up task). The #194-scoped candidate is COMPLETE: it does its
  job, the corpus adjudicates no-regression, and every remaining F0082
  layer is outside this spec's class.
