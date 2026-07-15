# Spec: f32-render-twin weld before topology emission (Stage 5/6, deviation N50)

## Goal

Guarantee the Yang boolean output B-Rep never contains two **distinct** vertices
that are **bitwise-identical after rounding to f32** — the exact point at which
kernel-v2's G1 render-collapse gate (`f32_render_degenerate`, B2 clause) fails
loudly with `planar triangle collapsed at render precision`.

This is the 3D, output-magnitude completion of N47 (which only reached
`moved`×`moved` relocated pairs). The R0012/R0098 render-collapse twins are
**non-relocated** arrangement vertices (N47/N48/N49): near-coincident Stage-0
coplanar-overlay sweep-event columns mint a twin lift-point pair on a crossing
edge; after the *final* Stage-4 relocation onto the exact analytic curves the
pair converges to within f32 render precision at the output (world) magnitude,
so it survives every earlier merge and trips G1 downstream.

## Research basis (P8)

Yang et al. 2025 §4.4.1 / Fig. 11(b) (`refs/text/yang2025_hybrid_boolean.txt`
lines 535–538, 562, 975): *"we remove a point if it is too close to another …";
"Coincident edges and points are merged."* The paper's "too close" is the model
resolution; here the operative resolution is the **render** resolution (f32),
because the defect is a render-buffer collapse, and it is measured — as G1
measures it — at the vertex's own magnitude (f32 ulp ≈ `|coord|·2⁻²³`). Two
vertices that round to the same f32 bits are the same rendered point; they cannot
be represented distinctly in the output mesh, so merging them is render-invariant.

## Why the criterion is f32 bit-equality, not a model band (N49 corrections)

N49 refuted a `TAU_MODEL·(1+scale)` band applied in the 2D overlay for two
reasons; the f32-bit-key criterion in 3D fixes both:

- **Fault 1 (over-merge):** a global-`scale` band over-merges legitimate
  near-origin rim samples in far-flung models. The f32 bit-key is **local by
  construction** — its resolution at a near-origin vertex (`|coord|·2⁻²³`, tiny)
  is far finer than at a vertex at magnitude 1686, so it never merges a
  near-origin pair that a global band would. A pair that rounds to distinct f32
  cells is genuinely distinct in the render buffer and is left alone.
- **Fault 2 (frame):** the 2D overlay cannot predict the 3D collapse because the
  render f32 floor is set by the 3D world magnitude, not the 2D overlay
  coordinates. This weld runs where the final 3D output coordinates exist
  (Stage 5/6, after all relocation), so it measures the same magnitude G1 does.

## Parameters / inputs

- The final Stage-5 `mesh` and its parallel `attribution`, at the point in
  `reconstruct_topology_stage4` **after** Stage-4 relocation and the KV15b
  sub-resolution collapse and immediately **before** `emit_topology`. Output
  vertices are 1:1 with `mesh.verts` (`emit_topology` step 1), so welding
  `mesh.verts` here is equivalent to welding output vertices.

## Branch table

| Condition | Action |
|---|---|
| Two live (triangle-referenced) verts share an f32 bit-key `[(x as f32).to_bits(), (y…), (z…)]` | `collapse_vertex(victim→survivor)`, survivor = min index; drop degenerate slivers / cancel membranes; re-scan |
| A group of ≥3 verts shares one f32 cell | collapse all onto the group's min-index survivor (one f32 cell = one render point; chaining within a cell is correct) |
| No two live verts share an f32 bit-key | strict no-op (byte-identical output) |

Grouping is by exact f32 bit-key (an equivalence relation on the f32 grid), so
the weld is **non-chaining across cells**: it never single-linkages two verts
that occupy distinct render cells (the N49 fault-1 / F0090 rim-drop hazard).

## Invariants / oracles

- **I1 — collapse soundness:** reuse `collapse_vertex` (proven watertight-
  preserving; membrane cancellation). After any collapse, `compact_unreferenced_
  verts` + recompute Phase A (the KV15b pattern), so emission re-validates the
  corrected mesh.
- **I2 — render-invariance:** every welded pair is bitwise-f32-equal, i.e. the
  same rendered point. The weld can only ever collapse an output edge that is
  already sub-render-precision; it never merges two verts that render distinctly.
- **I3 — no model-band widening (P9):** the criterion is exact f32 equality, not
  a tuned tolerance. For any vertex at magnitude ≥ 1, f32 equality is ⊆ the
  `TAU_MODEL` model-coincidence relation (f32 ulp ≥ `TAU_MODEL`), so welded pairs
  are also model-coincident; for magnitude < 1 it is strictly tighter.
- **I4 — zero-regression:** the full categorized release assay must show **0
  SUPPORTED_WRONG and no CORRECT lost**; R0012 and R0098 flip ERROR → CORRECT.
  Cases with no f32-coincident output pair are byte-identical (the fast path).

## Failure modes

- Does **not** address a pure B3 collapse (three f32-distinct verts that are
  f32-collinear with no coincident pair). R0012/R0098 are the B2 coincident-pair
  class (verified: the collapsing pair shares an f32 bit-key). A pure-B3 case, if
  found, is a separate class and out of scope here.
- Does not change the Stage-0 producer (N48/N49): the twin is still minted
  upstream. This is a self-localizing emission-hygiene weld — the same posture as
  KV15b (`collapse_subresolution_intersection_segments`) and N47.

## P9/P10

The criterion is the render buffer's own definition of "same point" measured at
the output magnitude, not a widened acceptance band. The pass only collapses an
already-render-degenerate output edge. If the assay shows any CORRECT lost or any
SUPPORTED_WRONG, abort and report (P10) — do not re-tune to a band.
