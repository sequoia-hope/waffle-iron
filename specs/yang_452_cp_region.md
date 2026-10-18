# §4.5.2 locality — the region is the faces C_p traverses (matrix M-1, ledger N62)

**Status: L1 LANDED 2026-10-17 (corpus-neutral, measured active); L2, L3 and §6 open.** Plan of record:
`specs/yang_faithful_cleanup.md` Phase 1 item 4(a)/(b); the loop itself is
`specs/yang_452_refinement_loop.md`.

## 1. The paper

§4.5.2 (`refs/text/yang2025_hybrid_boolean.txt:659-671`, right column):
"the erroneous region is bounded by two successfully optimized points p_f
and p_b on boundary curves belonging to different patches. The intersection
curve segment C_p between p_f and p_b is marked in green. The surfaces
requiring refinement include those traversed by C_p (red regions) and the
neighbors of a ring of them (orange regions). We then compute the
intersections between the meshes only in the refined regions … the curve
bounded by them is used to replace the original." "Successfully optimized"
is §4.5's "converge to a distance of 0 within their domains" (`:656-659`).

## 2. What was measured first (2026-10-17, R0044)

M-1 named three gaps: whole-op recompute, no p_f/p_b splice, region from
the STOP site. R0044 (the gap's cost customer) was timed before designing
(`YANG_STAGE_TIME_PROBE`, `CHERCHI_TIME_PROBE`):

- 75 % of every run was a quadratic cherchi-rs Stage 2 (ledger N95,
  RESOLVED): R0044 1194 s → 469 s, CORRECT in the corpus. The recompute's
  BREADTH was not the cost.
- The next cost is the §4.5.3 sweep: one whole-mesh `compute_phase_a` per
  removal (22 s of the natural run's 20 s-plus Stage 4 at 100k triangles).
  A cost item, not a compliance one; §6.
- R0044's region is every face of the gear — and that is the paper's rule
  read literally: a planar cap that C_p traverses has every flank among its
  neighbours. The region rule is not where R0044 deviates.

So the gap that changes WHAT is refined is the third: the region is the
faces at a failure SITE (one vertex) plus one ring, never the faces C_p
traverses. Two causes:

1. **The span is never walked.** T1's wrapper records the STOP vertex's
   faces; T2 records the fire vertex's incident faces. A failing run of
   several points across a patch boundary names one point's faces.
2. **Only the first failure is reported** (N84). The OffCurve gates already
   COLLECT (`s451_collect = true`, `s45_failures`) and a census walks each
   failure's own curve to converged bounds (`s451_post_sweep_census`), but
   the exit returns the first error and the loop sees one vertex.

## 3. The C_p walk (L1)

`cp_span(mesh, patches, v, good)`: breadth-first from the failing point v
along INTERSECTION edges — a mesh edge whose live triangles carry both
inputs (on the kept submesh an intersection edge has one triangle per
operand; Stage-0 overlap boundaries qualify, as the paper's "overlap
boundaries become intersection curves") — through vertices that are not
`good`, stopping at `good` ones. The visited set is the erroneous region,
the stopping vertices are its bounds (p_f, p_b, …: a span through a corner
has more than two), and the faces at every member and bound are "the
surfaces traversed by C_p". A walk is capped (512 vertices) so a run where
nothing converges stays bounded; the cap is reported in the site detail.

`good(w)` is the paper's "successfully optimized": `vertex_converged`
(within the junction-certificate band of a surface of EACH operand) and
not itself failed. Per vantage:

| trigger | vantage | failed set |
|---|---|---|
| T1 OffCurve (collected) | after relocation, before the sweep (every non-failed vertex relocated) | `s45_failures` plus `vertex_crossed_domain_endpoint` (the census's rule) |
| T2 boundary-domain fire | end of Stage 4 | the fire vertices (they lie ON their extended surfaces, so `vertex_converged` alone calls them good) |

The walk follows the mesh polyline, not one analytic curve: p_f and p_b sit
"on boundary curves belonging to different patches", so C_p crosses patch
boundaries by definition (the census's own-curve chain stops there).

L1 records, at the OffCurve exit, one T1 site per distinct collected
failure naming the faces of its span; and replaces each T2 fire's faces by
its span's. The STOP returned is unchanged (the first error), so error
texts do not drift; the failure SET grows, and with it the region and the
convergence monitor's signatures.

**Acceptance.** Corpus run; every move attributed with `YANG_452_PROBE`.
Expected customers: the loud-ceiling worklist's T2 rows (P0005, P0020,
R0049, R0070, P0056, P0061) and P0045 (OffCurve). A move CORRECT → ERROR is
a gap in the walk to fix, not a reason to narrow it.

**Measured 2026-10-17.** Corpus 350C / 0W / 19E / 5EE / 0T over 374
(release, 8 jobs, 900 s; wall 817.6 s): zero moves, zero detail drift. The
walk is ACTIVE — `[cp-span]` lines under `YANG_452_PROBE`, per worklist
case (spans / spans whose faces grew): R0070 215 / 149 (a run of 19 failing
points, 3 → 17 faces), P0005 63 / 9, R0049 3 / 3, P0056 6 / 6; P0020 9 / 0,
P0061 4 / 0, P0045 1 / 0. No worklist case converts: their failures survive
density 8 on the paper's region. P0020's four fires each name `(A:0, B:3)`
already. *(Retracted 2026-10-18, ledger N96: this paragraph read P0020's
fires as "a vertex gliding along A:0's boundary crease, the §4.5.1 boundary
class (M-2 / M-3)" with A:0 a planar face. A:0 is the cylinder lateral, and
every fire was FALSE — the domain certificate read each flank-ellipse crease
as its whole ellipse, and the steps crossed the flank planes 4 – 10 cm from
millimetre arcs. Bounded by the arc, P0005, P0020 and R0070 convert; the
region walk was never the question for them.)*

## 4. L2 — collect every T1 (N84)

The other ~50 `LocalRefinementRequired` / `RelocationCrossedCarrierVertex`
returns STOP at the first fire. Each becomes record-and-continue into the
same collection, with the same exit (no later pass runs on a mesh with an
unrelocated vertex), then L1's walk. Customers: P0015, P0029 (natural
`RelocationCrossedCarrierVertex` STOPs).

## 5. L3 — local recompute and splice

Deferred until a customer needs it. Outside the region the meshes are
byte-identical except along rims a refined face shares with an unrefined one
(per-edge N is the max of its faces' factors), so the whole-op recompute
gives the same curves there up to relocation of re-meshed rim points; the
splice's observable difference is confined to the ring's border. Its other
benefit was cost, and §2 measured where the cost really was.

## 6. The sweep's cost (not compliance)

`sweep_reversed_intersections` restarts with a fresh whole-mesh Phase A
after every removal. The paper walks C from an endpoint; a removal changes
the loop only around p_r. An incremental Phase A (dirty patches only) or
a single walk with local loop edits would make the sweep proportional to
|C|. Byte-identity is the acceptance; the ordering the walk imposes
(Phase 2a-i) must not change.

## 7. Ledger

N62 stays OPEN until L1–L2 land and L3 is either built or signed off as
the paper's efficiency measure with the measurement of §2/§5 (a prose
sign-off request, A15.7). N84 resolves with L2.
