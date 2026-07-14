# Spec: Coincident relocated-vertex weld (Stage-4, deviation N47)

## Goal

Guarantee the Yang boolean output B-Rep never contains two **relocated** vertices
that the model regards as the same geometric point. Two vertices that Stage-4
pushed onto an analytic curve (`moved`) can converge to within the model
coincidence tolerance; emitting them as distinct output vertices produces a
sub-render-precision output edge that trips kernel-v2's G1 render-collapse gate
far downstream (`planar triangle collapsed at render precision`).

## Research basis (P8)

Yang et al. 2025 §4.4.1 / Fig. 11(b) (`refs/text/yang2025_hybrid_boolean.txt`
lines 535–538, 562, 975): *"we remove a point if it is too close to another …
after removing all the points too close to each other";* *"if an endpoint p of
the split edge is too close to q, we merge p with q";* *"Coincident edges and
points are merged."* This increment is the **relocation-convergence analog** of
that merge: the two coincident points are produced by two arrangement vertices
that both Newton-project onto one intersection point.

## Parameters / inputs

- The Stage-4 intermediate `mesh`, its parallel `attribution`, and the `moved`
  set (vertices relocated onto an analytic circle / ellipse / line / torus /
  surface-pair earlier in `stage4_relocate_and_correct`).

## Branch table

| Condition | Action |
|---|---|
| A pair `(u, w)`, both in `moved` and still triangle-referenced, with `‖p_u − p_w‖ < TAU_MODEL·(1+scale)` | `collapse_vertex(w→u)` (victim = higher index); set `collapsed_any`; re-scan |
| No such live pair remains | stop (fixed point) |
| Pair not both `moved` | **untouched** — never collapse un-relocated arrangement geometry (the §4.4.1(b) micro-scale R0091 revert) |

`scale = max |coord|` over the pair. The band `TAU_MODEL·(1+scale)` is the model
coincidence tolerance used by the stage-5 planarity wall and every other
coincidence test — **10× tighter** than the `MIN_FEATURE_SIZE·(1+scale)` feature
floor, so it admits only sub-(feature/10) coincidences.

## Invariants / oracles

- **I1 — collapse soundness:** `collapse_vertex` is the proven watertight-
  preserving edge-collapse (drops the degenerate slivers, cancels opposite-
  winding membrane duplicates). Unit test: a relocated needle twin collapses to
  one vertex; a genuinely-separated (≥ feature-floor) relocated pair is left
  intact.
- **I2 — restriction:** only `moved`×`moved` pairs are eligible; un-relocated
  arrangement vertices are never welded (P9/P10 — the R0091 landmine).
- **I3 — zero-regression:** the full categorized assay stays byte-stable on all
  currently-CORRECT cases (they carry no relocated coincident twins); the pass
  is a no-op unless a relocation genuinely converged.

## Failure modes

- Not a fix for the **non-relocated** arrangement-twin render-collapse
  (R0012/R0098): those coincident vertices are Cherchi arrangement points, not
  `moved`, so welding them would collapse legitimately-distinct arrangement
  geometry — blocked on sidecar reference parity (the R0091 class). Documented as
  out of scope here.

## P9/P10

The band is the model's own definition of "same point," not a tuned tolerance,
and the pass only ever collapses an already-degenerate output edge between two
relocated vertices — never widens a downstream acceptance band.
