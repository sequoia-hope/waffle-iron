# Junction-Layer Research Findings

**Date 2026-07-17. Charter: `docs/yang_junction_research_charter.md` (Q1–Q6).**
Six parallel research tracks (five close-reading the local `refs/text/` corpus,
one web sweep of industrial practice). All six delivered; citations are
file:line into `refs/text/*.txt` (or URLs for web sources). Each finding feeds
a named Phase-3 spec. Corrections to the plan of record are listed at the end
and applied to `docs/yang_functional_roadmap.md` §0.0 in the same commit.

---

## The convergent through-line (meta-finding)

Every track independently arrived at the same principle:

> **Junction and seam correctness is achieved by SHARED-OBJECT IDENTITY —
> mint the junction/seam once, insert it into both operands as the same
> vertex/polyline object — never by numeric coincidence within a tolerance.
> N near-coincident endpoints where one shared vertex should be is an
> upstream defect and a loud STOP, never a fuzzy-merge.**

- Yang §4.4.1's `r_A = r_B = r` IS this move (`yang2025_hybrid_boolean.txt:551-554`) — stated as a precondition, never spelled out as a protocol (the gap that stalled #168/#169).
- Urick 2019 is the only source with the full two-operand protocol: exchange characteristic points, build ONE shared curve C̃(ζ) as the superset of both sides' features, conformally bind BOTH faces to it (`urick2019_watertight_booleans.txt:311-363, 620-630`).
- Cherchi 2022 achieves it structurally: one exact arrangement, shared regions are physically one set of triangles (`cherchi2022:249-256`).
- CGAL corefinement: one intersection polyline inserted as constrained edges into BOTH meshes; symbolic (indirect-predicate) points; non-manifold output REJECTED loudly, never repaired.
- OpenCascade names our #146 bucket: every section-curve endpoint resolves to one shared "technological vertex" referenced by ALL incident curves; the "fuzzy value" tolerance knob is the cautionary contrast we already reject.
- The companion Stage-0 paper (Q6): identical overlap meshes BY CONSTRUCTION — compute the overlap once, mesh one shared trimmed surface once, reuse for both operands, never move a coordinate afterward (`yang2025_overlap_region_extraction.txt:145-152`).

Corollary from Q4: exact arithmetic prevents FALSE degeneracy (rounding
collapsing distinct points) but correctly PRESERVES true near-degeneracy —
genuinely-close-but-distinct junction verts are real geometry the arrangement
must keep. Prevention therefore lives at the SAMPLING/MINT layer, not at a
downstream merge.

---

## Q1 — Corner / triple-point junctions

**Finding:** the SSI-topology literature (yang2023, cheng2023, li2026) does NOT
construct 3-distinct-surface junctions at all — it computes "boundary points"
(curve ∩ ONE patch's parametric-domain edge; `yang2023:427-429`) and curve
singularities (tangency/self-intersection; `cheng2023:254-320`), both
single-surface-pair. The only two-operand stitch source is **Urick 2019**:
corners are promoted to shared knots present in BOTH operands, minted once,
never re-derived per side; >2-patch corners become explicitly-managed
extraordinary points (`urick2019:787-796`).

**Assessment of our primitive:** `torus_plane_clip_junction` /
`relocate_onto_implicit_triple` (N-137.1) is geometrically STRONGER than
anything found (exact convergence onto all three surfaces vs Urick's
approximate projection). It fills a genuine literature gap. What we lack is
exactly what Urick names: the mint-once, insert-into-both-by-identity stitch.

**Feeds P3b spec:** trigger = the boundary-point taxonomy (curve exits a face
boundary); construct = our triple-Newton (keep); stitch = mint the corner ONCE,
insert into both operands as ONE shared arrangement vertex, split both incident
curve chains at it; never recompute per side.

## Q2 — Two-sided seam conformality

**Finding:** zero of four sources let the two sides independently re-derive the
seam. Conformality is a PRECONDITION established by giving both sides one
shared seam as identical constrained input, with CDT freedom only in each
face's interior u-v domain (Yang `:551-561`; Urick `:353-363, 620-630`;
Cherchi `:252-255`; CGAL constrained shared polyline).

**Contract invariant (adopt verbatim in the driver/SurfaceChart contract):**
*"The seam polyline is computed exactly once as a single canonical vertex
sequence and supplied verbatim as the constrained boundary input to the CDT of
BOTH incident faces; neither side re-projects, re-optimizes, or re-derives
seam vertices; each face's CDT may vary only in its own interior."*

**Flagged design tension (resolved below, correction #4):** our Stage-0 stitch
matches coordinates via `f64::to_bits` hashing — it works ONLY because the
overlap mesh is generated once from shared exact geometry (Q6 confirms this is
the canonical pattern). For NEW P3b seam/junction work, prefer pass-by-handle
(shared vertex indices) over re-emitting coordinates and reconciling by bits:
identity by construction, nothing to bit-match.

## Q3 — The §4.5.2 local-refinement loop

**Finding (strategic correction):** the loop's termination guarantee
(`yang2025:668-670`) holds ONLY for transversal intersections, and **no
confirmed case in our LRR/OffCurve buckets is a transversal resolution
deficit** — they are tangential junctions (#137: C0065/R0074/R0038), missing
degree-4 solvers (M5: R0044/R0096), or genuine micro-features/degenerate
junctions (R0072, R0003). Yang's own Table 3 shows the loop firing 4/400
operations at production d_ε — a rare tail mechanism. yang2023 §5.4
(`:625-643`) independently certifies refinement does NOT converge near
tangency. Our #137 sweep (χ wanders 1→1→−1 under refinement) is the in-house
proof.

**Consequence:** spec §4.5.2 as the **disciplined abort SHELL around the
junction work, not a case-recovery lever**:
1. Transversality pre-check at entry (|n_A × n_B| gate) — tangential/corner
   regions route to the #137 junction path or STOP;
2. Per-pass strict-decrease monitor on a topology-error functional
   (unpaired-edge count / |χ−2|) — abort on the first non-improving pass;
3. Refinement budget (max passes / min-d_ε floor), the shape our
   `merge_budget`/`split_max_passes` STOPs already use;
4. Output gated on the watertight/χ oracle — a non-converged refinement can
   only STOP, never silently accept (the permanent form of the #137 lesson).

**Analytic refinement operator:** locally lower d_ε / raise N on the traversed
faces + one-ring via the existing chord-sagitta knob
(`stage1_tessellate.rs:99-228`; the #137 spec Part (a) θ-band insertion is the
template). Hard dependency: the partner surface must be re-sampled identically
on the shared region — the Q2 invariant again.

## Q4 — Near-duplicate junction vertices

**Finding:** exact-arithmetic pipelines fully prevent the ROUNDING near-dup
class (implicit points, exact coincidence merge, one output rounding site at a
power-of-two multiplier) — all already correct in our port and N48
sidecar-certified bit-identical. They cannot and should not prevent
genuinely-close-but-distinct verts (real geometry). Our F0082 (0.012-apart
junction verts) and R0095 (1e-24-area boundary triples) signatures are
therefore **minted UPSTREAM at Stage-1 tessellation: the two operands sample
their analytic surfaces independently and non-conformally near a shared
junction** — the arrangement then correctly preserves both samples as distinct.

**Consequence (P3a re-scope):** P3a is a **prevent-at-mint conformal junction
sampling spec at Stage 1**, folding into the Q1/Q2 junction work: faces
incident to a shared curve must use the SAME boundary sample points (share the
seam polyline; don't re-sample), with genuine 3-surface corners inserted once
into both meshes. Explicit non-goal: **no new tolerance merge** — merging a
real 0.012 feature is the R0091 silent-wrong hazard; the existing loud STOPs
(`s6-planar-loop-nonplanar`, the Stage-4 shell gate) remain the P10 safety
net.

## Q5 — §4.5.4 illegal self-intersection detector

**Finding:** Yang's "illegal intersections" are mesh-discretization artifacts
(the input B-Rep is certified clean, `yang2025:756-760`), so the detector is a
MESH property test: **exact triangle–triangle intersection on non-adjacent
pairs of the same output shell** — octree broad-phase + the `cherchi-rs`
indirect predicates already backing Stage 2 (`cherchi2022:171-173, 296-305`).
Zero new numerics; conservative by the d_ε error bound. The Li 2025 algebraic
surface signature is the wrong layer (certifies the analytic surface, which is
already clean). Removal (increment 2) routes detected regions into the #169
mesh-update loop — no new remover.

## Q6 — Stage-0 identical meshes

**Finding:** the companion paper (Jieyin Yang & Jia, TOG 44(6) Art. 228,
`yang2025_overlap_region_extraction.txt`) guarantees identical overlap meshes
**by construction**: extract the overlap region once (tolerance-ε Hausdorff for
general NURBS), replace it with ONE shared trimmed surface, mesh it ONCE,
assign that single mesh to both operands; non-overlap parts share the boundary
sampling points. No post-hoc snapping exists anywhere.

**Assessment:** our exact planar overlay + bit-exact stitch is the exact
special case of this pattern (and stronger than tolerance-ε where coplanar
planes admit exact overlap). **No contradiction with N54 — independent
confirmation of it**: overlap identity is a build-time invariant; coordinates
are never movable after the seam exists.

---

## Corrections applied to the plan of record (roadmap §0.0)

1. **P3a re-scoped:** the junction-vert mint is at **Stage-1 tessellation**
   (independent non-conformal sampling), not Stage 2/3. P3a = conformal
   junction sampling, folded into the P3b junction protocol; no new tolerance
   merge, ever.
2. **§4.5.2 re-valued:** guard shell, not case mover. It recovers ~zero
   current cases; it exists to keep STOPs loud while the junction layer and M5
   recover the cases.
3. **#173 design settled:** exact non-adjacent tri–tri detector on the output
   shell via existing `cherchi-rs` predicates; removal routes into #169
   machinery.
4. **Seam identity doctrine:** new P3 seam/junction machinery passes shared
   vertices BY HANDLE (identity by construction). The Stage-0 bit-exact stitch
   stays as-is — it correctly realizes mesh-once/reuse (Q6) — but is not the
   pattern to replicate for new seams.
5. **Validated as-is:** the N-137.1 triple-Newton primitive (stronger than the
   literature), the non-2-manifold loud STOP posture (CGAL does the same), the
   weld-retirement conclusion (shared-index yes, tolerance welds no), N54.

## The junction contract (design principles for all P3 specs)

1. **Mint once, exactly** — junction points constructed by the exact triple
   solver; seam polylines computed once as canonical vertex sequences.
2. **Share by identity** — inserted into BOTH operands as the same vertex
   handles / constrained polyline; all incident curves reference the same
   vertex; interiors free, boundaries shared.
3. **Trigger by taxonomy** — boundary points (curve exits a face boundary)
   invoke the junction path; the transversality pre-check routes tangential
   regions there instead of the refinement loop.
4. **Multiplicity is a STOP** — N near-coincident endpoints where one shared
   vertex should be names an upstream defect loudly; never fuzzy-merged.
5. **Refinement is a guarded shell** — entry gate, strict-decrease monitor,
   budget, watertight-gated output.

## Spec mapping

| Finding | Feeds | Task |
|---|---|---|
| Q1 + Q2 + Q4 (junction contract, conformal sampling, corner stitch) | P3 junction-layer spec(s): Stage-1 conformal junction sampling + #137 insert/stitch | #169, #146, #137 |
| Q3 (guard shell + analytic operator) | §4.5.2 loop spec as abort wrapper | #169 |
| Q5 (detector) | #173 detector-first spec | #173 |
| Q6 (validation) | no change; Stage-0/M8 residue proceeds as designed | #130/#144 |
