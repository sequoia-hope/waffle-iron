# Disjoint-union passthrough (task #134)

Status: IMPLEMENTING
Driver: a union of two AABB-disjoint solids (the multi-body workflow; the
slice-g oracle-4 fixture) runs the full mesh pipeline and re-emits ALL
untouched geometry from mesh patches — every full rim degrades to a
LineSegment chord polyline (the output carries NO `Curve::Circle`
vocabulary at all when nothing intersects). A later boolean on that output
with a cylinder-owning intersection edge dies at the Stage-3 producer
fault `chord_tol_for_curved_owner` → `AmbiguousCurve{candidates:0,
matched:0}` (the owner has no Circle/Ellipse rim to derive a chord bound
from). Production via kernel-v2 is partially shielded (recover.rs retags
chord runs on ⊥-plane rims at ingestion), but a yang boolean output should
be a valid yang boolean input (the #133 contract).

## 1. Fix

A UNION whose operands' conservative AABBs are strictly disjoint is the
DISJOINT SUM: emit the concatenation of the two input B-Reps verbatim
(indices offset; every curve/surface tag preserved bit-for-bit). No
pipeline, no tessellation loss. Multi-lump outputs are legal since KV7-F2.

Conservative AABB (`conservative_aabb`): vertex hull, expanded by
- `Curve::Circle` / `Curve::Ellipse` edges → center ± radius (major) on
  every axis;
- `Surface::Sphere` → center ± r; `Surface::Torus` → center ± (R + r);
- Plane / Cylinder / Cone faces are inside the hull of their boundary
  bounds (planar faces by convexity of the hull; cylinder/cone laterals by
  the hull of their rim circles + apex vertex);
- `None` (no fast path) if any edge carries Hyperbola / Parabola /
  SurfacePair (open curves whose bulge is not cheaply bounded).
Disjointness = no overlap on some axis beyond band 1e-9·(1+scale).

## 2. Branch table

| # | Branch | Behavior |
|---|---|---|
| B1 | Union, conservative AABBs disjoint | concatenated B-Rep (NEW fast path) |
| B2 | Union, AABBs overlap (or unbounded curve vocabulary) | full pipeline, unchanged |
| B3 | Subtract / Intersect | unchanged (NO fast path this slice — a disjoint Subtract's pipeline output is byte-load-bearing for existing corpus verdicts; revisit if a case demands) |

## 3. Invariants

- I1: the fast-path output contains BOTH inputs' vertices/edges/faces with
  curve and surface tags bit-identical (indices offset).
- I2: `BRep::new` validation still runs (2-manifold per lump).
- I3: corpus zero-lost (P9 gate); any verdict movement must be
  UNSUPPORTED/ERROR → better.

## 4. Oracles

- `yang-rs/tests/disjoint_union_passthrough.rs` (RED first):
  - two disjoint cylinders → union output carries both exact
    `Curve::Circle` full rims (4 closed circle edges) + watertight, volume
    = sum;
  - CHAIN: that output ∪ a third overlapping cylinder succeeds (was the
    Stage-3 `AmbiguousCurve{0,0}` producer fault).
- full assay P9 zero-lost.

## 5. Research basis

- Boolean algebra: A ∪ B with A ∩ B = ∅ is the disjoint sum — no
  computation is exact-er than none. [#24 Yang 2025] applies to
  interacting solids; a non-interacting pair has no arrangement to build.

## 6. Ledger

- 2026-07-11: spec written (discovered during slice-g fixture probing,
  `m8_nary_tessellated_faces` §8 item 2).
- 2026-07-11: SHIPPED, two layers. (1) yang `boolean()` passthrough
  (concat_breps) — serves yang-direct chains; the disjointness margin must
  EXCEED the YR24 weld band (2·max(TAU_MODEL, scale·TAU_WORK)) or the
  near-partial r=1e-8 weld class (yr27) is stolen from Stage-0. (2)
  kernel-v2 `boolean_op` merges shells at the ARENA level using yang's
  exported predicate `union_operands_strictly_disjoint` — the yang
  passthrough output is INPUT-convention topology (seam-doubled loops)
  which `from_yang_brep` does not ingest; the native merge preserves every
  face bit-for-bit and journals identity lineage. TRAP: mock-backend
  adversary tests that park a dummy far-away B now bypass the pipeline —
  dummy operands must AABB-OVERLAP the real one (yr5c tjunction, yr9 t2
  fixed). Assay per-case byte-identical (237C/0W/49E/9U/0T).
