# #146 inc-3b — keep-interior CDT flood-fill migration (task #180)

**Status: SHIPPED (2026-07-19).**

The documented remaining increment-3 blocker of
`yang_146_conformal_junction_sampling.md`: gate-ON, the junction-insertion
rebuild (`rebuilt_with_junction_overrides`) mints NON-CONFORMAL operand
meshes — near-dup T-junction pairs with fwd=1/rev=2 + open directed edges
at junction-inserted regions.

## 1. Root cause (measured, F0084 gate-ON)

Probe chain: `NONMANIFOLD_SITE_PROBE` `i6-input-overuse` + topo dump +
`YANG_JUNCTION_MINT_PROBE=v` + `YANG_CDT_PROBE=8`, joined at unit level by
the bit-exact fixture `tests_unit/p3a_insertion_conformality.rs`.

At every offender site the directed-edge imbalance is exactly ONE EXTRA
SLIVER TRIANGLE over three consecutive points of a split edge polyline
(fixture: face 8's boundary `7→J19→11` where J19 is the edge-pierce point
0.0034 from B-Rep vert 11; the CDT emits the flap `[7, 11, J19]` between
the un-split chord `7→11` and the split polyline, so the boundary
constraint edges `(7,J19)`/`(J19,11)` are each used by TWO triangles —
fwd=1/rev=2 — and the chord shows up as an open directed edge).

A face carrying interior junction points routes through
`cherchi_rs::cdt_polygon_with_holes_keep_interior`, which classified kept
faces by **f64 centroid `point_in_polygon`** — the same parity/centroid
classifier class the F0047 and #179 migrations eliminated from the
flood-fill and all-segment CDT paths. A pierce point is within
`1e-9·(1+scale)` of the edge chord, so the flap's centroid sits ~1e-10
off the boundary and the f64 parity test misclassifies it as inside.
Faces WITHOUT interior points route through
`cdt_polygon_with_holes_floodfill` and stay conformal (fixture face 7).

## 2. Fix

Finish the migration in `cdt_polygon_with_holes_keep_interior` (its step
5): replace the centroid classification with the SAME two-part
classification `cdt_polygon_with_holes_floodfill` uses —

- **outer region topologically**: flood the CDT dual graph from the
  convex hull inward across non-constraint edges only; any face reached
  is outside the outer constraint loop (decision-exact — combinatorial,
  no f64 point-geometry), then
- **holes by exact parity**: `centroid_in_polygon_exact` (rational tier
  decides inside the uncertainty band).

Interior Steiner vertices do not disturb the flood-fill: they only add
inner faces; boundary loops remain the constraint walls.

## 3. Oracles

- cherchi-rs red→green: `keep_interior_near_collinear_boundary_chain_is_conformal`
  (bit-exact face-8 local CDT input via `f64::from_bits`) — every
  boundary constraint edge used exactly once, no directed-edge imbalance;
  existing keep-interior suite unchanged.
- yang-rs red→green: `junction_inserted_octagon_prism_stage1_mesh_is_2_manifold`
  (bit-exact F0084 live operand-B topology + the op's actual junction
  payload) — the rebuilt Stage-1 mesh is a closed conformal 2-manifold.
- Gate-OFF full assay: byte-identical (all callers of the keep-interior
  variant are env-gated: P3a junction sampling, `YANG_N2_RECDT_ENABLE`,
  `YANG_MESHUP_ENABLE`).
- Gate-ON full assay: category-identical or better vs the post-#179
  gate-ON baseline (251C/0W/55E/2T), 0 WRONG.

## 4. Non-goals

- The rebuilt-operand 2-manifold postcondition (loud STOP) — considered,
  deferred: after this fix the insertion rebuild is conformal by
  construction again; a blanket postcondition on every rebuild is a
  P10-style safety net to revisit if a NEW silent class appears.
- Near-dup pierce-point/vertex geometry itself (J19 0.0034 from vert 11
  is a REAL junction distinct from the vertex — nothing to weld; the
  insertion contract's TAU_MODEL endpoint guard already rejects sub-weld
  grazes).
