# §4.5.5 one dimension down — edge-in-plane identification + conformity (P0001)

**Task:** convert P0001 (prospector P0: octagon prism ∪ 4-point needle star,
`boolean_union TessellationFailed "planar triangle collapsed at render
precision"`). **Owner:** Stage 0 (`crates/yang-rs/src/stage0/edge_in_plane.rs`).
**Blocks:** nothing; **unblocks:** the P-series tail (P0002/P0003 are separate
signatures).

## 1. Root cause (anchored 2026-09-27, `YANG_MESH_DUMP` + `YANG_S5_FOLD_PROBE`)

The star's two tip vertices are authored at sketch `u = −4.047e-15` (the
generator's `cos 270°`), so the star's TIP EDGE lies **4e-15 below** the
octagon's bottom-cap plane (`y = 22.41755130980609`, the two sketches share
their origin) and runs INSIDE the cap region for `x ∈ [0, 3.044]`. Nothing in
the pipeline identifies an edge (or a vertex) into a partner PLANE — Stage 0's
§4.5.5 overlay identifies planar FACE pairs only — so the exact arrangement
faithfully keeps the 4e-15-wide wedge of the upper flank between the tilted
tip edge and the cap plane as "outside A": kept triangle `t66 = [22, 12, 14]`
has all three corners on the tip line (v12 exact on the plane after a P3a mint
collapse, v22 the authored femto-off tip, v14 a cap-diagonal crossing). Its
boundary cycle folds 180° (`YANG_S5_FOLD face=9 … turn=180.00`), the emitted
loop runs `7.85 → 0 → 3.04` along one line, and kernel-v2's G1 render gate
refuses the zero-area ear. The gate is right; the defect is upstream.

Two experiments pin the class:

| variant | verdict |
|---|---|
| P0001 with the tip `u` set to exactly `0` | SUPPORTED_CORRECT (χ 2, one body, V = 2747.7287) |
| the exact-tip variant under 9 random rigid motions | 8 CORRECT, seed 2 `reassembled output would be non-2-manifold` |
| the original under 6 rigid motions | 4 flip to CORRECT (rounding luck), scale ×1e-3 keeps 2 ERROR |

So the exact arrangement and Stage 5 already handle an edge that is
BIT-EXACTLY coplanar with the partner's cap triangles, and the class is
rounding-luck in both directions on oblique planes. The fix must not depend on
exact coplanarity at all.

## 2. Principle (Yang §4.5.5, `refs/text/yang2025_hybrid_boolean.txt:717-731`)

"Our discretization method does not maintain coplanarity in triangle meshes
because of floating-point error … it is necessary to check coplanar planes and
perform 2D Boolean operations before mesh discretizations … identical meshes
are generated for both models in this part … The common part and the other
two parts share identical sampling points on their boundaries."

An edge of X lying in a planar face F of Y is the same degeneracy one
dimension down: the "common part" is the sub-segment of the edge inside F, and
the paper's remedy is the same — make it an identically-sampled shared element
of BOTH Stage-1 meshes so the arrangement sees one edge shared by identity, not
two femto-separated ones. The #178 calibration line (`gap ≤ band/100`,
`band = max(TAU_MODEL, scale·TAU_WORK)`) already separates authored
coincidence from designed sub-resolution features for face pairs; the same line
applies here (P0001's gap 4e-15 is 5 orders below it).

## 3. Change (two arms, one pass)

**Arm 1 — identification (vertex onto partner plane).** Before the §4.3.3
generator tangency and Stage 0: for each vertex `v` of X and each planar
all-line face F of Y (unit `n̂`, `d̂`), `gap = |n̂·v + d̂|`. If
`0 < gap ≤ band/100` and `v` INTERACTS with F — its in-plane projection lies
in F's outer polygon inflated by `band` (and not inside a hole by more than
`band`), or an incident LineSegment edge whose other endpoint is also in-band
crosses F's polygon — then `v` is moved onto the plane. A vertex matched by
several planes of Y (a Y edge or corner) takes the least-norm displacement
satisfying all of them (Gram solve, near-parallel duplicates dropped). Vertices
of X incident to a face that forms a Stage-0 cross pair with any Y face are
NOT touched (Stage 0 owns the face-pair class; this keeps every pair case
byte-identical). Symmetric in (A, B).

**Arm 2 — conformity (the shared segment as a shared mesh edge).** In the P3a
scope gate (no Stage-0 interaction, no rim-junction boost), on the identified
operands: for each geometric LineSegment edge `g = (p0, p1)` of X with both
endpoints within `band/100` of F's plane, compute in F's own Stage-1 frame
(`ortho_basis(normal)`) the transversal crossings of `g` with F's loop edges
and the sub-segments of `g` inside F. Then:

- every crossing `P` is inserted, with identical bits, into every per-loop copy
  of `g` (X's `edge_overrides`) AND every copy of the crossed Y loop edge
  (Y's `edge_overrides`) — both incident faces of each edge split there by
  identity, exactly as the P3a mint does;
- an endpoint of `g` inside F becomes an interior Steiner point of F
  (Y's `face_overrides`);
- each inside sub-segment becomes an INTERIOR CONSTRAINT of F's CDT (new
  `face_constraints` channel → `cdt_with_interior_constraints`), so the
  segment is an EDGE of F's triangulation. On X's side the sub-segment is
  already a boundary chain of both faces incident to `g`.

Every crossing is registered as a junction mint (`minted_junction_keys`,
default provenance) so a later Stage-4 weld recognizes it.

Fail-closed scope (a skipped contact is status quo, counted by the probe):
crossings within `TAU_MODEL·(1+scale)` of a loop vertex or of `p0`/`p1`,
endpoints within that margin of F's boundary, collinear overlap of `g` with a
loop edge, curved-bounded F, non-planar F.

**Probe.** `YANG_EDGE_IN_PLANE_PROBE=1` prints every identified vertex (gap,
planes), every contact (crossings, sub-segments) and the census of gaps in the
STOP window `(band/100, band]` — the population a #178-style loud STOP would
need before it is added (none added here: unmeasured).
`YANG_EDGE_IN_PLANE=off|0` disables both arms (dev A/B).

## 4. Invariants

- **I1 (identity):** operands with no vertex in any partner plane's band are
  byte-identical (both arms return `None`/empty; no rebuild).
- **I2 (pair ownership):** a vertex on a Stage-0 cross-pair face is never
  moved; Stage 0's population is byte-identical.
- **I3 (one mint, two owners):** every crossing point has one f64 position
  used verbatim on both operands.
- **I4 (loud):** a constraint the CDT cannot honor (crossing another
  constraint or the boundary) is `MalformedTopology`, never a silent split.

## 5. Measurement gate

1. Unit red→green (`tests_unit/s455_edge_in_plane.rs`): the P0001 replica
   (femto-off tip) completes with the exact-tip volume; the oblique replica
   (rigid motion) completes with the same volume; a plain crossing-box union
   is byte-identical with the pass on and off; a face CDT with an interior
   constraint carries the segment as an edge.
2. `ASSAY_CASE=P0001 single_case` → SUPPORTED_CORRECT; the meta adjudicated
   (χ 2, volume from the exact oracle), `derived_meta` cleared, the smoke pin
   moved.
3. Full categorized release assay (8 jobs, ≥900 s): EXACTLY {P0001}
   ERROR→CORRECT; every other case category- and detail-identical.
