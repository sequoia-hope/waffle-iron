# Junction-Layer Research Charter (endgame Phase 3 de-risk)

**Created 2026-07-17.** The compliance endgame plan (`docs/yang_functional_roadmap.md`
§0.0) rests on closing the junction layer, and the paper under-specifies it.
This charter turns each under-specified aspect into a precise research question
with the paper citation showing the gap, the corpus demand, the decision the
answer feeds, and the sources to consult. The research session's output is
`docs/yang_junction_research_findings.md`; each finding feeds a Phase-3 spec.

**Method:** Track 1 — close-read the locally held literature (`refs/text/*.txt`;
license-restricted, local-only). Track 2 — web sweep of industrial-kernel
practice (CGAL corefinement, OpenCascade BOPAlgo, meshing literature).
Findings must cite file:line (local) or URL (web). A finding that contradicts
a shipped mechanism is flagged, not silently adopted (P10).

---

## Q1 — Corner / triple-point junction construction and stitching

**Paper gap:** §4.3–§4.5 relocate intersection points onto curves and (§4.5.1,
`refs/text/yang2025_hybrid_boolean.txt:672-688`) solve points q1/q2 on a
boundary curve C_b — but nowhere specifies how a corner where an intersection
curve exits through a face-boundary (a 3-surface junction, e.g.
torus∩plane_x∩plane_y) is constructed, inserted into BOTH meshes, and stitched
watertight. Our triple-Newton primitive (N-137.1) is an invention.

**Corpus demand:** C0065, R0074, R0038 (#137); R0003 (ellipse×hyperbola chain).
**Feeds:** the P3b insert+stitch spec.
**Sources:** yang2023_topology_guaranteed_ssi, cheng2023_topology_driven_ssi,
li2026_ssi_survey (singular/branch-point handling); urick2019_watertight_booleans
(watertight rebuild at curve endpoints); web: CGAL corefinement triple-point
insertion, OCC section-edge vertices.
**Exit criterion:** a named algorithm (or the confirmed absence of one) for
junction-point construction + two-sided insertion, compared against N-137.1.

## Q2 — Two-sided conformality at a shared (curved) seam

**Paper gap:** §4.4.1 (`:548-590`) prescribes CDT of each trimmed face with
r_A = r_B identification, each face in its own u-v domain — but never says how
two independently-CDT'd sides of a curved seam stay edge-identical (the exact
failure that stalled #168 §5c.8 and #169 Phase A/B; N54 showed even a 1-ulp
disagreement tears the Stage-0 seam).

**Corpus demand:** the banked two-sided driver's wiring; every P3b stitch.
**Feeds:** P3b wiring design; the `SurfaceChart` contract.
**Sources:** yang2025_hybrid_boolean §4.4.1 close-read; urick2019 (seam
reconstruction); Marinov & Kobbelt 2004 (cited by paper §4.5.1); web:
conforming interface meshing, CGAL corefinement shared-edge guarantees.
**Exit criterion:** a stated invariant ("the seam polyline is the shared
input; neither side re-derives it") or equivalent, adoptable as a contract.

## Q3 — The §4.5.2 local-refinement loop, operationalized for analytic surfaces

**Paper gap (partially concrete):** §4.5.2 (`:658-671, 690-700`) defines the
erroneous region (bounded by converged points p_f/p_b, refine faces traversed
by C_p + one-ring, re-intersect only there, repeat; termination via mesh→spline
convergence). Under-specified for US: our surfaces are analytic with closed-form
curves (no spline grid to subdivide), and #137 proved naive global refinement
flips loud STOPs into silent-wrongs — the paper's loop assumes refinement is
always safe.

**Corpus demand:** the 18-case LRR bucket.
**Feeds:** P3 local-refinement loop spec (the §4.5.2 half of N2).
**Sources:** paper §4.5.1/4.5.2 close-read + Fig 12/14; li2026_ssi_survey
(marching/refinement near singularities); our #137 resolution-sweep data.
**Exit criterion:** a loop design with (a) region = the paper's bounded region,
(b) an analytic-surface refinement operator, (c) a termination/abort criterion
that PRESERVES loud STOPs when refinement alone cannot converge topology.

## Q4 — Near-duplicate junction vertices: tolerance-safe dedup vs mint-avoidance

**Paper gap:** §4.3 (`:535-540`) — "we remove a point if it is too close to
another point on the same loop" — one sentence, no tolerance model, no
topology guard. Our #146 bucket (F0082 v588≈v601 0.012 apart; R0095 1e-24-area
triples) shows the real problem is UPSTREAM MINTING of near-dups across
loops/patches, which the sentence doesn't cover.

**Corpus demand:** 8-case non-2-manifold bucket + suspected chained-CDT cases.
**Feeds:** the P3a mint fix spec (Stage 2/3).
**Sources:** cherchi2022 + mesh_arrangement (how exact arrangements avoid
near-dup output verts), attene-predicates; web: OCC fuzzy booleans tolerance
model, snap rounding with topology preservation.
**Exit criterion:** identify where robust pipelines prevent (not repair)
near-dup junction verts, mapped onto our Stage-2/3 mint sites.

## Q5 — §4.5.4 illegal self-intersection detection/removal

**Paper gap:** §4.5.4 (`:752-758`) is ~7 lines: input has no self-intersection,
so post-trim illegal intersections are detected and removed by local
refinement — no detector algorithm, no removal procedure.

**Corpus demand:** none loud today (the watertight gate covers); N6 is an OPEN
deviation and task #173.
**Feeds:** #173 detector-first spec.
**Sources:** li2025_nurbs_self_intersection; mesh_arrangement (self-intersection
detection machinery); cherchi2022.
**Exit criterion:** a detector algorithm choice with complexity + a removal
strategy consistent with detector-first/loud-STOP.

## Q6 — Stage-0 "identical meshes" on coplanar overlap regions

**Paper gap:** §4.5.5 says generate identical meshes for both models on the
shared trimmed overlap — without saying how identity is guaranteed under
floating point. Our N54 refutation established the live constraint (bit-exact
`f64::to_bits` seam; no post-hoc coordinate motion is safe).

**Corpus demand:** M8 residue (R0007/R0071, C0048/F0067); prevention of the
twin-mint class (N48–N56 arc).
**Feeds:** M8 residue design; any future Stage-0 canonicalization re-spec.
**Sources:** **yang2025_overlap_region_extraction (the companion paper — likely
the §4.5.5 authors' own elaboration; read FIRST)**; our stage0/ implementation
notes; web only if the companion paper is silent.
**Exit criterion:** the companion paper's actual identical-mesh mechanism,
compared against our overlay + bit-exact-stitch design.

---

## Non-goals

- No mechanism changes from research alone — findings feed specs, specs feed
  gated increments (FIP unchanged).
- No NURBS-scope reopening (D14 PERMANENT); read spline-based sources for the
  *shape* of algorithms, port to analytic surfaces.
- Web findings about closed-source kernels are hints, not evidence — only
  adopt what we can state as a testable invariant.
