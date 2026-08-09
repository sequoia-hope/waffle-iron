# §4.4.1 As Written: Curve-Authoritative Trim + CDT, Replacing Relocate-In-Place

**Status:** DESIGN (2026-08-08). Sequenced AFTER the two silent-wrong anchors
(`docs/audits/volume_oracle_flags_anchored.md`) — confirmed wrongs outrank loud
ERRORs — but this remains the structural epic that closes deviation N2 and the
dominant ERROR family.

## 1. The claim

The production Stage 4 (`stage4_correct::stage4_relocate_and_correct`)
implements HALF of Yang §4.4.1: it maps the exact curve onto the meshes and
moves mesh crossing vertices onto it (`r_A = r_B = r`), but KEEPS the Stage-2
mesh connectivity. The paper's other half — trim + re-triangulate — is what
makes the construction valid:

> "we trim and update the meshes using the intersection curves … Then we set
> r_A = r_B = r, so that the two polylines in the meshes coincide with the
> intersection curve … Next, through CDT we obtain valid discretizations of
> the trimmed meshes … contains no flipping triangles since the intersection
> curves are regular."  (`refs/text/yang2025_hybrid_boolean.txt:546-563`)
>
> "To improve remeshing quality, we remove a mesh vertex if it is too close to
> the intersection curve on the mesh. If there is no point within an
> intersection loop, we insert a point … For the newly generated boundary
> triangles around the intersection curve, we recalculate d(T)."  (`:562-571`)

In the paper the constraint polyline is sampled ALONG the curve (monotone in
curve parameter — "regular", §4.3.4's h/l/α refinement), near-curve mesh
vertices are REMOVED, and the patch interior is re-triangulated by CDT in the
parametric domain. A self-crossing seam chain cannot arise: nothing drags
mesh-ordered vertices onto a curve whose geometry diverges from them.

Our relocate-in-place substitute is precisely what mints the ERROR family:

- The 2026-08-06 census: every self-crossing loop in the corpus is MINTED by
  Stage 4 (`cross_inherited = 0` over 312 cases; 8/47 ERROR vs 0/261 CORRECT).
- F0067 anchor: Stage 4 pulls a vertex 3.7e-3 against a 6.4e-4 segment; loop
  crossings occur between the relocated vertex and DISTANT outline vertices
  (loop-index pairs 5–7 apart) — vertices the paper's construction would have
  removed or re-triangulated around.
- The measured negatives (08-04..08-06: collapse_vertex trial, splice-as-repair
  with the non-manifold-seam selector, curve-authority reorder, §4.5.1 per-point
  truncation, joint truncation) were all POST-HOC repairs of the relocated
  chain. None was the paper's construction. The campaign's own closing
  inference — "points at the curve's SAMPLING" — is this spec.

ERROR mass in this family (2026-08-06 results.json): stage4-relocation-region
-invalid (10), reassembly-non-2-manifold (9), cdt-ring-rejected (8, of which
R0028 has its own anchor), patch-triangulation-folded (2) ≈ **27–29 of 47**.
Treat the reach as a hypothesis the wired increments must measure, per case
class — never claim conversions in advance.

## 2. What already exists (build on it, don't rebuild)

~7,000 LOC of tested, gated, currently customer-less §4.4.1 machinery:

- `stage4_update::stage4_mesh_update` — Fig-11 split/merge/insert + CDT via
  `cherchi_rs::cdt_with_interior_constraints` (N2-1, invariants I1–I6 tested).
- `stage4_dt::{eval_uv, d_of_t}` — the §4.1.2/Fig-6 per-triangle d(T)
  recompute, exact rational-Bézier bound for all analytic surfaces (N2-2).
- Charts + `patch_from_cycles(_shifted)` — parametric-domain patch extraction
  with θ-branch unwrap (N2-3b).
- `stage4_splice::merge_seam_chains` + `apply_splice`'s SEAM-VERTEX IDENTITY
  (one mesh vertex referenced by BOTH sides; `SeamVertexIdentityConflict` on
  coincident-but-distinct) — the piece that makes Stage-6 reassembly manifold.
  Carries over unchanged in role, but fed by the CURVE chain, not the
  relocated mesh chain.
- The shipped closed-form junction handlers (corner identities) remain the
  boundary-condition layer at patch corners.

## 3. The construction (per intersected patch)

1. **Constraint polyline = the curve's own sample chain**: Stage-3 exact curve
   points ordered by ITS parameter (`conic_param` where closed-form), refined
   per §4.3.4 (h/l/α) — never the relocated mesh chain. Junction endpoints
   keep their shipped corner identities.
2. **Near-curve vertex removal** (`:562-564`): drop patch vertices within the
   paper's proximity of the polyline (the Fig-11(b) merge criterion already
   exists in `stage4_mesh_update`); interior vertices that survive are carried
   into the CDT `interior` list.
3. **CDT re-triangulation** of the whole trimmed patch in the parametric
   domain against the constraint polyline (both sides, shared seam indices).
4. **d(T) recompute** for new boundary triangles (`stage4_dt`).
5. **Driver is UNCONDITIONAL**: every patch traversed by an intersection curve
   is updated — the paper does not condition §4.4.1 on a defect detector.
   (`detect_nonmanifold_seams` and `stage4_fold_risk` were selector attempts
   for an operation the paper applies always; retire them as drivers, keep
   fold-risk as a diagnostic.)

Curved-patch interior vertices are the known frontier: `patch_from_cycles`
refuses them today (`CurvedPatchInteriorVertices`) because dropping them
coarsens the surface. Carrying them through `interior` + d(T) is increment 2
below — most of the R-series is curved, so the epic's reach depends on it.

## 4. Increments (each lands gated + measured, byte-identical when off)

1. **I1 — planar-patch construction end-to-end** behind `YANG_441_CONSTRUCT`:
   curve-sampled constraint chain + removal + CDT + shared seam identity,
   planar patches only. Measure: the 8 census cases + the relocation-region-
   invalid subclass on planar seams; full assay gate-ON vs gate-OFF.

   **STATUS (2026-08-09): LANDED as the per-seam slice; mechanism sound, 0
   conversions, and the measurement names the I1b design.**
   `stage4_construct.rs` (seam enumeration `seam_groups` — unconditional, no
   defect detector — plus cycle-run collapse `replace_seam_run`) + the pass
   driver `stage5_topology::run_construct_passes` + a pair-own generalization
   of `splice_seam_pair`'s foreign-interior scan (vertices dropped by BOTH
   sides of the pair are the pair's own business). For a `LineSegment` seam
   the curve resample IS the two junction endpoints, so the relocated
   fold-back chain (collinear, order-scrambled — the census's crossing mint)
   leaves the boundary entirely and the pair re-triangulates around the
   clean seam.

   Measured on F0067's failing boolean (skip census built into the pass):
   101 seams → **39 APPLIED** (write-backs accepted, chains up to 16→2),
   69 already-minimal, 21 non-line (I2), **11 declined at the fixpoint —
   decline census: `SelfIntersectingPolyline` ×500 events,
   `CdtFailed(DuplicateVertex)` ×76**. The wall persists (same
   ring-rejected class). F0045: its 3 seams are plane×curved — entirely I2.
   Gate-OFF byte-identical (zero construct lines; yang-rs suite green).

   **I1b (the measured design correction): the §4.4.1 unit of work is the
   PATCH with ALL its curves, not the seam pair.** A collapsed straight seam
   still crosses the OTHER not-yet-collapsed relocated chains of the same
   cycle — mutually-blocked seams can never collapse one-at-a-time, which is
   exactly the paper's own text ("we trim and update the meshes using the
   intersection curveS"): rebuild each patch's cycles with ALL seam runs
   collapsed simultaneously, single-sided per patch, conformality from the
   shared curve-chain vertex identities (each collapsed run is the same
   vertex pair on both adjacent patches by construction — no two-sided
   driver needed once polyline points are existing shared vertices). The
   `DuplicateVertex` ×76 class is the known femto-pair junction family and
   needs the shipped junction identities applied to the modified cycles.

   **STATUS (2026-08-09, same day): I1b LANDED.** `collapse_patch_runs`
   (simultaneous per-patch collapse, degenerate-cycle guard) +
   `rebuild_patch_planar` (single-sided TOLERANCE-FREE plain CDT of the
   modified cycle polygon — after collapse the seams are ordinary boundary
   edges; no two-sided driver, no d_eps/merge_tol plumbing) +
   `apply_rebuild_batch` (one write-back for the whole batch). The driver's
   batch assembly removes a seam (or a whole patch's seams) on any refusal
   — mid-batch non-contiguity, degenerate cycle, cross-batch dropped-vertex
   conflict, foreign reference to a dropped vertex, CDT decline — and
   re-assembles, loudly; a seam collapses only if BOTH owners rebuild in
   the same batch. Decline census built in: per-cycle edge composition
   (collapsed-seam/line-seam/curved-seam/plain), CROSSING edge pairs on
   `TriangulationFailed`, coincident-pair identity on `DuplicateVertex`.

   Measured on F0067's failing boolean: **all 39 eligible seams collapse in
   ONE pass-0 batch over 59 patches** (I1 needed 39 passes and left 11
   mutually blocked with ×500 decline events); fixpoint census is now **9
   stable declines with exact signatures**:
   - `TriangulationFailed` ×8 — every one the SAME geometry, ANCHORED to
     the vertex (2026-08-09, follow-up session with full-cycle + position
     census): each declined patch is a rib SIDE WALL whose top boundary
     runs the collapsed seam from s=0 to s=1 along the top line, then
     walks BACKWARD along that same line (the fold-back chain, e.g. 991 →
     983 → 975 at s = 0.776, 0.795, 0.906) to the wall's TRUE corner at
     s≈0.906, where the vertical edge drops. **The seam's junction
     endpoint OVERSHOOTS the face corner by a uniform 1.339e-3 on every
     rib** (999 vs corner 975; 1934 vs 1971; …): the junction is minted
     PAST the point where the intersection line exits the bounded face,
     and the fold-back chain exists to walk back from the overshot
     junction. The collapse correctly keeps junction endpoints, so the
     overshoot survives and the direct seam edge crosses the corner's
     vertical edge — the CDT decline is naming the UPSTREAM mint. This is
     the F0082 minted-corner / beyond-corner-phantom family
     (`trim_beyond_corner_phantoms`, stage4_correct.rs — "the curve stops
     being an output boundary at the corner"); that machinery does NOT
     fire on these junctions. The paper's own answer is Fig-11(a): the
     boundary intersection q is computed ON the boundary curve; a sample
     beyond the face has zero kept content.

     **I1c census (2026-08-09, DONE): why the trim never fires, and the
     overshoot's identity.** (1) The entire moved×minted weld + trim block
     in `stage4_relocate_and_correct` is gated on
     `!minted_junction_keys.is_empty()`, and F0067's failing boolean mints
     ZERO junction keys — the beyond-corner machinery only reaches the
     curved-pierce mint family (F0082); the TF-8 plane×plane family is out
     of its reach by CALL-SITE GATE, not by predicate. (2) The relocation
     census (decline census now prints per-vertex relocation identity):
     the overshot seam endpoint is the UNIQUE relocated vertex in each
     declined cycle — projected onto the exact curve (recorded curve
     parameter; `relocations` stores `(v, t)`), landing past the face
     corner, while the true-corner column and the walk-back vertices are
     all UNMOVED. The I1b collapse keeps junction endpoints BY DESIGN, so
     this is a Stage-4 SEAM-ENDPOINT AUTHORITY defect — the 2026-08-01
     "TWO relocation authorities" anchor, now localized to endpoints.
     **I1d (2026-08-09, DONE): the endpoint authority is IDENTIFIED — and
     it is neither hypothesis.** The site-tagged relocation probe
     (`YANG_I1D_RELOC_PROBE`: every `relocations.push` self-tagged with
     `line!()`; junction sites also dump rho/sinθ/gate + curve params)
     puts every overshot endpoint at ONE site: the
     `vert_pp_circle_junction` relocation (task #146 branch 4 — exact
     pp-line∩circle). F0067 is a WHEEL: the seam lines are RADIAL spokes
     through the rim circle's center (sinθ = 1.0 exactly — no tangency);
     the rib end faces sit at r = 0.20751 and the rim circle at
     r = 0.208846, a 1.34e-3 design gap the bounded seams never cross.
     The 8 endpoints land at EXACTLY the circle radius (r matches to
     1e-9): each was classified as a pp-line×circle junction because its
     edge incidence carries both curves, and the EXACT unbounded-pair
     junction was computed 1.4e-4–5.96e-3 away (corridor gate 1.74e-2
     passes everything) — OUTSIDE the wall face's kept footprint (no wall
     triangles beyond r = 0.20751). The vertex was never a sample of that
     junction; it is q, the seam's face-boundary exit (Fig-11(a)). The
     corridor gate reasons only about displacement magnitude and cannot
     see kept content. The femto pair (`DuplicateVertex`, 1049+1050 @
     4.44e-16) is almost certainly the SAME mint — two adjacent spokes'
     endpoints relocated onto the same rim circle at near-identical
     angles (the 2026-08-01 quadruple-point region).

     **I1e incidence census (2026-08-09, SELF-RETRACTION recorded): the
     "mis-attribution" verdict below is WRONG — the circle incidence is
     LEGITIMATE.** Every chained edge carries `[A:Plane, B:Cyl]`, and B
     is the RIB: the rib's outer end is CYLINDRICAL with the same radius
     as the wheel rim (r = 0.208846). The chain vertices at
     r = 0.2029–0.2051 are the cap-facet CHORD crossings (sagitta-deep
     inside the circle, exactly as a coarse chord slab puts them); their
     outward relocation onto the exact circle is the paper's own
     resample, and v999's junction relocation onto line∩circle is the
     CORRECT r_A = r_B = r for a real wall-line × rim-circle junction.
     What breaks is downstream of a correct relocation: the walk-back
     vertices (991/983/975 — EXACTLY on the seam line at s = 0.776–0.906,
     unmoved because plane×plane geometry is exact) lie parametrically
     BETWEEN the seam's junction endpoints, but their edges are plain
     B-mesh boundary edges (not classified curve edges), so the I1b
     collapse's seam run ends at 999 and never swallows them — the
     boundary walks out to the junction and BACK over them: the
     fold-back. **The gap is §4.4.1's NEAR-CURVE VERTEX REMOVAL — §3
     step 2 of this spec, the one piece of the paper's construction I1b
     did not implement** ("we remove a mesh vertex if it is too close to
     the intersection curve"). **I1f (LANDED same day; measured an honest
     NO-OP on F0067 — and the block census names I1g).**
     `on_segment_interior` (1e-9 identity band, strict t∈(0,1)) + a
     conformal removal phase in the batch driver: a vertex is removed
     only if EVERY patch holding it on a cycle is rebuilt in this batch
     and holders ≤ 2 — all-holders-or-none (a one-sided removal would BE
     a T-junction). On F0067 every removal is BLOCKED, loudly: the
     walk-back vertices' second holder (the disc top patch) owns no
     eligible seam so it is not in the batch, and the corner vertex
     (975 class) has THREE holders — a topological corner, not a
     discretization vertex. The analysis goes further: even unblocked
     removal cannot convert, because the residual boundary
     `… 999 → 975 …` is a collinear fold between the EXACT junction
     (r = 0.2088) and the chord-anchored Stage-1 corner (r = 0.20751) —
     **975 and 999 are two authorities' versions of ONE B-Rep corner**
     (Stage-1 anchored the rib's wall∩cap edge on the cap's CHORD; the
     junction relocation placed the exact triple point). Removal is the
     wrong operation for a corner; the paper's own operation is
     Fig-11(a)–(c) SPLIT + MERGE: q (the junction — ON the cap patch's
     rim-circle chain by construction) splits the boundary chain
     containing it, and the too-close endpoint p (the chord-anchored
     corner, 1.34e-3 ≈ mesh scale) MERGES into q.

     **I1g: Fig-11(a)–(c) corner identification at collapsed-seam
     junctions — increment 1 LANDED SUB-GATED (`YANG_441_CORNER_MERGE`),
     measured OVER-FIRING; the missing predicate is named.** The
     machinery: candidate p = a ≥3-holder vertex (the I1f holder census's
     corner discriminator) near q; merge = shared-INDEX substitution
     (batched cycles rewritten; surviving non-batch triangles re-pointed
     in `apply_rebuild_batch` via a `subs` map — a curved neighbour
     adopts the merge without a re-CDT); consecutive-duplicate cycle
     cleanup + degeneration guards; a merge is refused if any surviving
     triangle holds both p and q (that substitution would mint a
     degenerate (q,q,x) — measured as a Stage-6 non-2-manifold before
     the guard). Measured on F0067: with the global d_eps band 652
     merges (geometry mangled, non-2-manifold); with the LOCAL band
     (min boundary-edge length at q, d_eps-capped) + the guard, still
     208 merges vs ≈16 expected, batch degraded to 34 applied /
     14 declined (vs 39/9 without). **The missing predicate is
     Fig-11(a)'s SPLIT-EDGE CONTAINMENT: q must lie ON a boundary edge
     adjacent to p (within that edge's owner band — for a rim chord,
     the cap owner's chord tolerance), not merely near p.** Proximity +
     corner-ness over-select in a wheel full of legitimate nearby
     corners.

     **Increment 2 (same day): the containment predicate LANDED and
     VALIDATED — and it exposes the mechanism's limit.** The scan
     anchors on a curve-classified boundary edge containing q (strict
     interior τ, perpendicular ≤ band, the seam's own edges excluded),
     over ALL patches' cycles — the containing chain (the rim chords)
     belongs to patches outside the seam's owner pair; p = the near
     endpoint, merged only when within the band. Measured on F0067:
     every accepted merge sits at the EXACT corner gap (dist = 1.344e-3
     uniformly, holders = 4, ZERO ambiguity, zero far-field over-fire —
     the predicate finds precisely the corner family). But 72 of 156
     candidate merges are REFUSED by the shared-triangle guard: at
     these quadruple-point corner clusters, members sharing a surviving
     triangle is the NORM, and index substitution cannot collapse
     across one (it would mint a degenerate (q,q,x)). Partial
     unification degrades the batch (32 applied / 11 declined vs
     baseline 39/9), so the merge stays sub-gated
     (`YANG_441_CORNER_MERGE`); the main gate remains the I1b+I1f
     baseline (39/9, honest ring-reject ERROR). **Inc-3 (same day, LANDED sub-gated; measured NEGATIVE — pairwise
     welding under-identifies the cluster).** The merge moved out of
     the batch into a weld PRE-PHASE per pass: validated pairs collapse
     via the watertight `collapse_vertex` (degenerate triangles
     dropped, membrane cancellation), then Phase A recomputes and the
     pass restarts. Measured on F0067: 5 pairs weld cleanly (zero
     ambiguity, 6 degenerate tris healed — e.g. v1935→v2943 and
     v1938→v2985 at the exact 1.344e-3 gap) — but the TF-8 patches
     still decline IDENTICALLY (the welded pairs were OTHER junction
     copies at the cluster, not the wall junctions — the wall q's were
     the split-edge ENDPOINTS in those hits, not the q's), the batch
     stays degraded (32/11 vs 39/9), and the verdict WORSENS to a
     Stage-6 non-2-manifold: at a quadruple-point corner the "corner"
     is a CLUSTER of ≥3 members (per-seam junction copies + outline
     corners), and welding some pairs leaves a half-identified pinch.
     **Inc-4 pre-step (same day, DONE — and it CLOSES I1g's planar arc):
     the welds ALONE mint the non-2-manifold.** Attribution from the
     logs: no welds fire on the early booleans (their blocks are
     clean); all 5 fire on the failing boolean. Bisect with
     `YANG_441_APPLY_SEAM_CAP=0` (welds fire, batch applies nothing):
     the non-2-manifold appears with ZERO batch applications — the
     bare `collapse_vertex` pinches the mesh by itself. This is the
     2026-08-05 measured-negative pattern verbatim (the splice module
     docs: "a bare collapse_vertex rewrote triangle indices without
     rebuilding the patch") — the paper's Fig-11 merge happens INSIDE
     the re-CDT, never as a mesh-space vertex collapse. (The weld
     phase now also respects `apply_enabled` — census-only booleans
     must not mutate.)

     **I1g STATUS: BLOCKED ON I2 for the wheel-corner family, with the
     selector VALIDATED and both mechanisms measured.** Synthesis of
     the three increments: inc-2's substitution-inside-rebuild is the
     correct SHAPE (the merge must be part of re-triangulation), but a
     corner cluster's holder set includes the CURVED cap patch, which
     cannot be rebuilt in I1's planar scope — substitution without
     rebuild on the cap is exactly the bare-collapse hazard localized
     to one patch, and refusing it (inc-2's guard) leaves the cluster
     half-identified. Inc-3's mesh-space weld handles shared triangles
     but pinches (measured alone). The dependency is real capability,
     not mechanism choice: **I2 — curved-patch single-sided rebuild
     (interior-vertex carry + d(T) recompute) — is the prerequisite**,
     after which the corner cluster unifies by rebuilding ALL holder
     patches against cycles where the cluster is one vertex (inc-2's
     substitution, batch extended to curved holders). Merge remains
     sub-gated; the main gate is the I1b+I1f baseline.

     (Superseded analysis, kept for the record:)
     The classification-time probe (`[i1d-classify]`, pre-relocation
     positions) measured: the junction-classified endpoints sit at
     r = 0.2029–0.2051 PRE-relocation — 3.9e-3 inside the rim circle,
     far beyond any chord's sagitta reach (chords live at r ≥ 0.2071) —
     so their circle incidence is a MIS-ATTRIBUTION; and the top-edge
     chain was PERFECTLY MONOTONE pre-relocation (999 at s = 0.732
     between its neighbours; 991/983/975 at 0.776/0.795/0.906): the
     junction relocation moved 999 from s = 0.732 to s = 1.0, leaping
     over three unmoved outline vertices AND the face corner in one move
     — the 2026-08-06 crossing mint, caught in the act. A
     kept-content gate at the junction arm alone CANNOT fix it: refusing
     classification reroutes the vertex to the plain circle loop, whose
     radial band (1.74e-2) also passes 3.85e-3 and projects onto the
     same false circle radially (same damage, different door), and the
     no-skip audit couples endpoint bookkeeping to EDGE classes. The
     deviation is the bug: some incident edge at these vertices is
     classified as a rim-circle edge despite its endpoint being
     millimetres off the circle. Next census: WHICH edge, its other
     endpoint, and which layer (Stage-3 `build_intersection_curves` /
     Phase-A curve classification) put it on the circle chain.
   - `DuplicateVertex` ×1 — the femto pair is ANCHORED: mesh verts
     1049+1050, 3D distance 4.441e-16, bit-identical chart projections.
     Upstream double-mint of one junction (the `SeamPointCoincident`
     posture: the pair is the defect; a downstream weld would be a band).
     Stays a loud decline; the fix is the recorded Root-C upstream-mint
     item.
   F0067 itself remains an honest ERROR at the downstream ring-reject — the
   9 declined patches carry their crossings into Stage 5/6. Zero write-back
   refusals; gate-OFF byte-identical by construction (all changes inside the
   gated driver + pure helpers).

   **Full-corpus gate-ON assay (2026-08-09): 258C/0W/49E/1T** vs the
   gate-OFF baseline 259C/0W/49E/0T. Zero WRONGs, zero conversions, two
   named deltas:
   - **F0085 ERROR→TIMEOUT** — a budget clip (construct-pass CPU pushed an
     already-ERROR case past 240s), the recorded budget-artifact class.
   - **R0095 CORRECT→ERROR — an UNMASKED LATENT, fully anchored.** Bisected
     with the new deterministic probes (`YANG_441_APPLY_BOOL_CAP`,
     `YANG_441_APPLY_SEAM_CAP`, `YANG_441_VERBOSE`, plus kernel-v2's
     `KV2_RING_REJECT_PROBE`) to a MINIMAL repro: ONE collapse (seam 0,
     patches 1+3, chain [21,23,79,83]) in the case's first boolean. That
     chain is CLEAN — collinear AND monotone (t = 0, 0.371, 0.595, 1);
     `direct-edge-pre-tris=0`; both rebuilds' CDTs succeed. The subtract
     then rejects an input face ring (99 verts) whose crossings sit at ring
     positions 62–74 — 25 positions from the seam (v83 at pos 93) — where
     the ring ZIPPERS two near-coincident chains (~1e-5 apart; interleaved
     monotone t-sequences). Patch 1's PRE-collapse cycle already weaves
     between the same two vertex families (`…78–58 | 25,24 | 57–54 | 27,26
     | 31 | 53–50…`), and that pass-0 state is identical gate-OFF (all
     prior passes run in both gates) — the woven double-chain PRE-EXISTS;
     the collapse of one clean seam merely perturbs how the downstream
     subtract processes that face. The latent's own root (the woven
     base-face boundary — coplanar-contact / M8-residue fingerprint) is a
     separate defect with its own worklist entry; per
     `feedback_regressions_can_be_unmasked_latents` it does NOT indict the
     construction.
   - Also landed with the campaign: the CHAIN-STRAIGHTNESS identity gate
     (`chain_straightness`, band 1e-9 relative, P10-sanctioned loud-skip):
     `Curve::LineSegment` is a unit variant, so one seam group can hold TWO
     different lines meeting at a real corner — collapsing that chain would
     cut the corner. Measured `nonstraight=0` corpus-wide on the cases run
     (every collapsed chain genuinely collinear), so it is a guard, not the
     R0095 fix (that hypothesis was tested and refuted same-day).
2. **I2 — curved patches**: interior-vertex carry into `interior` + d(T)
   recompute; retire `CurvedPatchInteriorVertices` by capability, not by
   band. Measure: R-series members of the family.

   **I2a (2026-08-09, LANDED on the main gate): CYLINDER-owner
   single-sided rebuild — and the epic's FIRST corpus conversion.**
   `rebuild_patch_planar` generalizes to Plane|Cylinder: θ-unwrap via
   `unwrap_theta` (encircling patches refuse loud), interior vertices
   CARRIED into the CDT keep-list after branch assignment (each interior
   vertex has exactly one branch inside the unwrapped boundary span —
   containment check, no tolerance), orientation matched as before;
   Sphere/Cone/Torus remain a loud skip. One measured correction en
   route: interior candidates lying ON a boundary edge are the DROPPED
   seam-chain vertices (collinear ruling verts) — carrying them makes
   spade split the boundary constraint one-sidedly (F0059's minted
   edge-use imbalance, `InvalidBooleanOutput`); §4.4.1's near-curve
   removal applies to cylinder patches too, so they are filtered by the
   same `on_segment_interior` identity predicate. Gate-ON full assay:
   **259C/0W/49E — the baseline count with ONE REAL CONVERSION (F0085
   ERROR→CORRECT, the `NonPlanarFace(37928)` case, fixed by
   cylinder-owner line-seam collapses; composition oracle green)**
   offset by the known R0095 woven-boundary latent; F0059 verified
   clean post-filter; F0067 unchanged (its walls are the corner cluster
   + femto pair, blocked on the fuller I2 story). Remaining I2 tail:
   Sphere/Cone/Torus charts, curved-SEAM (conic) collapse (I2b), d(T)
   recompute wiring when a consumer for persisted d(T) appears.

   **I1g in-batch merge RETRIED post-I2a (2026-08-09, sub-gated):
   mechanically SOUND — and the blocker moved upstream to I2b.** The
   substitution-inside-rebuild merge with holder closure (every holder
   patch of a merged corner joins the batch as a merge-only rebuild
   participant; unchartable holders refuse loudly; a declining holder
   blocks its pairs, not any seam) replaces the negative inc-3 weld. On
   F0067: 68 merges fire (the wall-corner family at the exact 1.344e-3
   gap included), the batch stays HEALTHY (39 applied / 9 declined —
   the inc-1/inc-3 degradation is gone), zero non-manifold mints. The
   10 blocked pairs all block the same way: the MERGE-ONLY HOLDER's
   rebuild declines `TriangulationFailed` — those holders' cycles carry
   their own CURVED-seam chains (rim-circle chord vertices relocated
   onto the circle with scrambled parameter order) that self-cross
   until resampled along the conic. The corner family is therefore
   gated on **I2b: conic-seam resampling — the curved analogue of the
   line collapse (`replace_seam_run` generalized: replace the run with
   the curve's parameter-ordered resample; `order_along_curve` is
   already shipped for exactly this ordering)** — which is also the
   nonline=21 worklist and the R0011/R0004/R0049 family. One gate, all
   remaining F0067 walls.
3. **I3 — flip per wall class** (cdt-ring-rejected → relocation-region →
   reassembly-non-2-manifold), full assay after each; any conversion or new
   wall is censused before the next flip.
4. **I4 — retire the relocate-in-place path** once I3 holds the score at
   ≥ parity with zero new WRONGs; the §4.5.3 reversal sweep stays (it acts on
   the curve polyline, which the paper orders the same way).

## 5. After this epic (recorded, not started)

- **§4.5.4 removal half / §4.5.2 guard shell** (roadmap item 3d/4): route the
  residual `YANG_SELFX_PROBE` fire-list and `LocalRefinementRequired` STOPs
  into bounded local refinement (refine → re-intersect locally → re-optimize →
  re-update; STOP, never accept, on budget exhaustion). The paper's Fig-2
  loop and termination argument: `refs/text/yang2025_hybrid_boolean.txt:659-670`,
  `:752-758`. Expected small customer count post-I4 (findings Q3) — it is the
  faithful closer for tangential/micro-feature residue, not a case farm.
- The AmbiguousCurve quartet, Root-C upstream-mint cleanup (R0019/R0025),
  R0028, R0053, and the P10 contract walls stay separately-anchored items.
