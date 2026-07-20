# SPEC — #169 P3b inc-4c: post-merge fan re-triangulation (the R0061 fold)

Status: **DESIGN, measurement-grounded (2026-07-20)**. Parent:
`specs/yang_169_p3b_curved_partner_pierce.md` §5 inc-4c. This is the Yang
§4.4.1 "update the triangulation accordingly" half of the merge ops the
Stage-4 weld/trim passes already perform — the missing piece named by the
epic (`specs/yang_mesh_updating_epic.md`): the pipeline merges curve points
(§4.3 weld, §4.4.1(b), inc-4b trim) but never re-triangulates the merged
neighbourhood, so stacked collapses manufacture non-manifold fan folds.

## 1. The defect, measured (R0061 gate-ON, `YANG_P3B_PIERCE_ENABLE=1`)

Probe chain (`YANG_P3B_FOLD_PROBE=x,y,z,r` pre/post dumps at the pre-sweep
site, `YANG_MOVED_WELD_PROBE`, `YANG_P3B_TRIM_PROBE`, wedge-dump):

- **Pre-weld/trim the mesh is a clean closed 2-manifold** (every undirected
  edge has exactly 2 incident triangles; edge (186,211) = 1 A-tri + 1 B-tri).
- The passes then collapse **14 victims** (7 weld + 19-trim's subset in this
  neighbourhood) onto the Stage-1 mints. The victims form connected clusters
  spanning ADJACENT mints (v162→211, v173→186, … v166→186); each pre-mesh
  edge CROSSING the victim-partition cut (162,166), (162,174), (162,186),
  (211,186) maps onto the single mint-pair edge (211,186), stacking its
  surviving triangle there. `collapse_vertex` checks no link condition; its
  membrane cancellation removes only EXACT opposite-winding duplicate pairs —
  these survivors have DISTINCT near-dup tips (3e-5–5e-4 apart, above the
  weld band), so nothing fires.
- **Post-trim: exactly 6 edges violate the total-use-2 invariant** (552-tri
  op mesh, histogram {2: 815, 4: 5, 6: 1}), all with ≥1 minted endpoint,
  in 2 vertex-connected clusters:
  - cluster {180, 186, 193, 211, 220, 222} (the zigzag mint chain):
    (186,211) 4A+2B, (193,211) 2A+2B, (186,220)/(220,222)/(180,222) 3A+1B;
  - cluster {145, 146}: (145,146) 4A+0B.
  The walk dies at mint 211 (`s6-boundary-walk-deadend`, 2 in / 0 out) →
  legacy fallback → loud `NonManifoldOutput`. R0061 is CORRECT gate-OFF.
- **Ground truth (gate-OFF final mesh, same op, SUPPORTED_CORRECT):** every
  edge total-use 2; each pierce corner has degree 4 (2 section-curve edges +
  1 A-interior + 1 B-interior); the section curve threads INTERMEDIATE
  samples between corners (no direct corner-corner edge).
- **The pinned chain survives the collapses:** in the broken gate-ON mesh,
  the healthy (total-use-2) seam edges (edges with one A- and one B-tri)
  still form a complete section-curve chain through the cluster —
  182—211—216—186—190 (v216 = the intermediate sample, 4e-5 from 211).
  Every defective edge is a redundant CHORD of that chain ((186,211),
  (193,211), …) manufactured by the fan merge.

**Refuted en route:** the parent spec's "fold-dedup by arc-adjacency"
sketch — the extra triangles have DISTINCT tips, so dropping any copy
leaves an unpaired hole at its tip; no deletion-only rule can repair this.
The repair must REWIRE (re-triangulate), not delete.

## 2. Contract

At the pre-sweep site, immediately AFTER `trim_beyond_corner_phantoms`
(the weld and trim own the merges; this pass owns the §4.4.1 triangulation
update), `retriangulate_collapsed_fan_regions`:

1. **Detect** defective edges: undirected edges with total incident-triangle
   count ≠ 2 and at least one endpoint a Stage-1 minted junction vertex
   (the mint anchor keeps the pass away from any pre-existing legitimate
   4-sheet structure, e.g. the KV9-F1 Steinmetz tangency generator, which
   has no mints). No defective edges → return unchanged (the
   overwhelming-majority fast path, byte-identical).
2. **Cluster** defective edges by shared vertices (union-find). Per cluster,
   per attribution key `(input, face)` with ≥1 triangle incident to a
   cluster vertex: **region** = that key's triangles incident to any
   cluster vertex.
3. **Region boundary** = region edges with ≥1 incident triangle OUTSIDE the
   region (any key, any operand). Fail-closed guards, each aborting the
   CLUSTER (mesh untouched, downstream STOPs stay loud):
   - every boundary edge must have exactly 1 outside triangle (its region
     share is then exactly 1 — the total-2 target);
   - every defective edge must be region-INTERIOR (no outside triangle);
   - the boundary must walk into simple loops (every boundary vertex has
     exactly 2 boundary neighbours within the region's boundary-edge set);
   - the region's surface must have a chart (`SurfaceChart`: Plane or
     Cylinder; sphere/cone/torus regions → bail);
   - a cylinder-chart region must not straddle the θ branch cut after
     re-centring θ on the region mean (the inc-2 quarter-turn guard shape).
4. **Re-triangulate** the region in its chart:
   `cherchi_rs::cdt_with_interior_constraints(verts2d, outer, holes,
   interior, [])` — outer/holes from the boundary loops (largest-|area|
   loop = outer), interior = region vertices not on any loop (kept, e.g.
   near-dup arc tips that became fully interior). No new vertices, **no
   geometry moves** (the chart is used only to obtain 2D coordinates; the
   output triangles reference existing global vertices — the N54 lesson).
   CDT failure → bail the cluster.
5. **Winding**: orient each new triangle so its area vector agrees with the
   region's pre-repair net orientation (plane: net normal sign; cylinder:
   net radial sign at the triangle centroid — the `replan` precedent).
6. **Postcondition (per cluster, all-loud)**: after splicing all of the
   cluster's regions, every formerly-defective edge and every region
   boundary edge has total-use exactly 2; otherwise the pass restores the
   cluster's ORIGINAL triangles (bail) — it may never trade one
   non-manifold shape for another silently.

Conformality argument (why per-region CDT is two-sided-safe): no vertex is
created, moved, or removed; every seam edge on a region's boundary loop has
exactly one triangle on the other operand's side (guard 3a) and receives
exactly one region triangle from the CDT — so both operands meet along the
IDENTICAL 3D polyline by construction. This is the degenerate-but-sufficient
case of the banked Phase-A two-sided update (`stage4_update`), reachable
without projection-drift risk precisely because the repair is
connectivity-only.

## 3. Non-goals

- No tolerance, no band: eligibility is combinatorial (use counts, mint
  anchors); geometry is never compared, never moved.
- Not a general §4.5.2 refinement loop: no new samples are inserted; the
  region is re-triangulated over its EXISTING vertex set. If the existing
  set cannot triangulate (CDT failure), the case keeps its loud STOP.
- The B-missing-sheet asymmetry ((186,220) 3A+1B) needs no special arm: the
  A-side and B-side regions are repaired independently against the same
  pinned boundary; the postcondition verifies the totals.
- `collapse_vertex` itself is unchanged (its membrane cancellation remains
  correct for the exact-duplicate class).

## 4. Oracles & measurement

- Unit (`tests_unit/p3b_fan_retriangulation.rs`): a synthetic folded fan
  reproducing the measured R0061 shape (two mints, victim cluster collapsed
  to a 4-use edge with distinct tips) → repair yields all-edges-use-2 with
  boundary preserved verbatim; guards red/green (defective edge on region
  boundary → bail; chartless surface → bail; postcondition violation →
  original triangles restored); Steinmetz-shaped mint-free 4-sheet edge →
  detector NO-FIRE.
- Single case: R0061 gate-ON — the two clusters repair (probe
  `[p3b-fanfix]` lines), the case ends SUPPORTED_CORRECT or at a LOUD
  next-layer gate (measured either way; the P3a/P3b ledger records it).
- Full assay gate-OFF: byte-identical or category-identical (the pass is
  live gate-OFF — P3a mints are always-on — so measured, not assumed).
- Full assay gate-ON: 0-WRONG ratchet; R0061 regression cleared is the
  flip-blocking target; zero new regressions.
- Always-on from the start (the N55/inc-3a precedent: a paper op with a
  structural trigger ships always-on); `YANG_P3B_FANFIX_PROBE` is the
  observability knob, not a gate.

## 5. Increments

- **inc-4c-0 (DONE, this spec §1):** measurement + design. Probes banked:
  `YANG_P3B_FOLD_PROBE=x,y,z,r` (pre/post local-complex dump, fires in both
  gate states).
- **inc-4c-1 — BUILT + MEASURED (2026-07-20), SHIPPED always-on
  (fail-closed):** `retriangulate_collapsed_fan_regions` at the pre-sweep
  site, with three measured design corrections over §2's first draft:
  - **seam-pinned defective edges** ((186,220)/(220,222)/(180,222),
    3A+1B): a defective edge where some attribution key contributes
    EXACTLY 1 triangle is live seam (that side is unfolded; a fold
    contributes ≥2) — constrained as a boundary edge of every region
    touching it, one triangle per side. Only balanced fold chords
    ((186,211) 4A+2B, (193,211) 2A+2B) are left free to dissolve.
  - **pinch vertices** (the 4-strand crossing mints 193/145): regions
    split into edge-connected components, and the boundary walk allows
    even degree >2 with COMBINATORIAL fan-chain pairing (rotate through
    the component's triangles via shared at-vertex interior edges; each
    chain pairs its two boundary-edge ends). An angular-sector variant
    was tried first and REFUTED — fold triangles overlap the seam
    direction in the chart (182 lies on the 193→211 chord), so geometry
    cannot disambiguate; connectivity can.
  - **postcondition by expected multiplicity**: a region-boundary edge's
    target is (untouched outside triangles) + 1 per bounding region — 2
    in a closed mesh; interior CDT chords must be exactly 2; formerly-
    defective edges end at their expected count or vanish.

  Unit: 5 fixtures (`tests_unit/p3b_fan_retriangulation.rs`) — an ORGANIC
  fixture (manifold two-operand strip; the production `collapse_vertex`
  stack manufactures the fold; the key ingredient is DISTINCT kept tips
  on the cross-cut triangles, since symmetric sandwiches produce exact
  membrane pairs that the existing cancellation already handles) proves
  repair-to-closed-complex; mint-free no-fire; unattributed-triangle
  bail-without-mutation. 411 yang-rs lib green; clippy/fmt clean.

  **R0061 gate-ON measured outcome: both clusters BAIL LOUDLY at the CDT
  (`keep-interior CDT failed`), and the bails are CORRECT — the next
  layer is now characterized:** the region boundaries genuinely
  self-cross in the chart. The seam polylines carry RELOCATION-ORDER
  ZIGZAG NEEDLES: near-dup samples relocated onto the analytic curves
  OUT OF ORDER along the curve (measured: wall-section chain
  287→150→146→236 with true curve order 287,146,150,236 — segments
  (287,150)×(146,236) cross at ~1e-8 transverse, vertices 8e-9–4e-8 off
  the opposing chords; the big cluster's arc chain has SIX such
  crossings among its 3e-5-spaced near-dup samples). The exact
  arrangement cannot emit a self-crossing seam, so this disorder is
  CREATED by Stage-4 relocation — the #145 sample-misorder class
  resurfacing on the seam itself. No keep-boundary re-CDT can proceed
  over a self-crossing boundary; deletion-only and refinement approaches
  do not touch it either.
- **inc-4c-2 — seam-order needle resolution (spec-first, NEXT).** Restore
  along-curve order for relocated seam samples (the curves are
  analytically known — order by curve parameter; a needle vertex whose
  removal/reorder is sub-`TAU_EVAL` transverse is the §4.3 "point too
  close on the same loop" class), after which the inc-4c-1 boundaries
  become simple and the fan repair completes. Gate-ON flip stays blocked
  until then.
- **inc-4c-3:** full ledger both gate states; parent-spec §5 update; then
  (parent inc-5) the always-on flip decision for `YANG_P3B_PIERCE_ENABLE`
  once F0082's inc-4d also lands or is re-scoped.
