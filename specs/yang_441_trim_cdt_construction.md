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

   **I2b (2026-08-09, LANDED on the main gate and CORPUS-VERIFIED).**
   Eligibility becomes action-typed: a LINE seam COLLAPSES to its
   junction endpoints; a CONIC seam REORDERS its run to the curve's
   parameter order (`order_along_curve` + `reorder_cycles_to_curve`,
   both shipped; carrier pre-check added — the reorder primitive
   silently no-ops when no cycle carries the whole seam). Guards:
   straightness and near-curve removal are LINE-only (a conic's chord
   is not its curve); the corner-merge's seam-edge exclusion covers
   both the mesh order and the reordered chain. Measured: **F0059 runs
   764 conic reorders and stays SUPPORTED_CORRECT** (the machinery at
   scale, composition oracle green); the full gate-ON assay shows ONLY
   the two known deltas (F0085 conversion + R0095 latent) — zero new
   walls corpus-wide. On F0067 the reorders have no surviving
   customers yet: its orderable conic runs are 11 already-in-order +
   10 riding on the declining wall patches, and merge-only holders
   join the batch with their cycles VERBATIM, so their own conic
   chains never reorder — the CDT declines persist. **The named next
   increment is the PATCH-CLOSURE driver (§3 step 5 as written): the
   batch unit becomes the intersected PATCH with ALL its curves,
   closed transitively (a joining patch brings its seams; those seams'
   partners join too), which subsumes the merge-holder closure and
   gives every holder's conic chain its reorder.**

   **Full-stack diagnosis on F0067 (2026-08-09, measurement-only
   round): the wheel-corner residue needs a NEW capability — I2c,
   input-edge chain refinement — not a composition tweak.** With
   construct + merge + reorder all on: 176 merges and heavy near-curve
   removals fire, yet the TF-8 walls decline with their cycles
   UNCHANGED at the corner. Three interlocking measured facts: (1) the
   wall-corner merge (chord-anchored 975-class → junction 999-class)
   never fires because q is ALREADY a vertex of the partner circle
   chain — no edge CONTAINS q, and the containment selector has no arm
   for the q-already-split case (Fig-11(a) is topologically done
   there); (2) the ≤2-holder removal rule blocks the corner columns —
   CORRECTLY, since they are topological corners, and removal is the
   wrong operation for them; (3) the corner columns (975/2574-class)
   are the CHORD-ANCHORED Stage-1 input-edge chain of the rib's
   wall∩cap edge, sitting 1.34e-3 inside the exact ruling — their
   resolution is REFINEMENT of B's own input-edge chain onto the exact
   geometry at seam-adjacent corners (the §4.3.4 sampling discipline
   applied to input edges, which nothing in the tree builds yet).
   I2c's shape: for a batched patch whose boundary contains an
   input-edge chain adjacent to a seam junction, refine that chain's
   vertices onto the exact input edge (wall∩cap = a ruling of the cap
   cylinder in the wall plane), which lands the corner column at the
   junction radius and lets the existing merge/removal machinery
   close the corner. Until then the F0067 walls remain honest ERRORs;
   the corpus holds 259C/0W with the F0085 conversion.

   **I2c-1 (2026-08-10, LANDED sub-gated `YANG_441_INPUT_REFINE` — and
   its measurement REFUTES the wheel-corner reach hypothesis; the corner
   re-anchors to the JUNCTION layer.)** The machinery, all measured
   working: `input_edge_chains` (plain-run identification over scoped
   patches — same-input different-attribution neighbours, seam/curve
   edges excluded, maximal same-neighbour runs, seam-adjacency
   qualification within the derived chord band, canonical dedup, and a
   `RunSkip` coverage ledger printed under `YANG_441_VERBOSE`);
   `refine_chain_to_ruling` (exact plane∩cylinder ruling, parallel-axis
   identity at 1e-9, two-candidate selection with per-vertex ambiguity
   refusal, band-guarded displacement, idempotent); the Fig-13 authority
   partition (junction/relocated vertices at run ENDPOINTS are PINNED —
   the q-already-a-vertex case — one INSIDE the run refuses loud); and
   the driver phase feeding refined corner endpoints to the Fig-11(b)
   merge as refine-anchored pairs (the arm the containment selector
   cannot see).

   **Measured on F0067 (all gates): 0 chains refine.** 91 chains refuse
   `NonParallelAxis` (dot ±1.0 — the plane⊥axis plane-cap CIRCLE class,
   the real I2c tail), and 832 chains — including EVERY TF-8 wall's runs
   — land in the plane×plane-EXACT no-op arm. The wall corner columns
   (1523/2433-class) sit at r = 0.207507 exactly, wall AND column vertex
   alike, with the end-cap neighbour measured PLANE: they are the
   DESIGNED rib-end radius, exact same-solid input edges — nothing is
   chord-anchored there. The kept junction (1524-class) sits at
   r = 0.208846 (the rim-circle radius), 1.339e-3 BEYOND the wall's
   kept footprint. **The I1e "legitimate incidence" retraction is itself
   RETRACTED**: the `[A:Plane, B:Cyl]` incidence belongs to the
   rim-circle seam of the top-plane × drum-lateral pair (the 328-class
   top patches carry those curved-seam chains at the same z), not to a
   cylindrical rib end; the rib end is a PLANE and the 1.34e-3 gap is
   DESIGN — the original I1d reading. The wheel-corner defect is
   therefore the ORIGINAL I1d verdict, now measured from a second
   independent direction: the `vert_pp_circle_junction` relocation keeps
   the seam endpoint at the exact UNBOUNDED junction, outside the wall's
   kept footprint; the seam's true exit is the wall's own corner —
   Fig-11(a), q ON the boundary. **Named next increment: J1 —
   boundary-exit junction authority** (kept-content discipline at the
   junction/classification layer), carrying the I1e-rescope reroute
   hazard (refusing one arm reroutes the vertex to the plain circle
   loop; endpoint bookkeeping is coupled to edge classes) as a standing
   design constraint.

   Two shared-path fixes landed with the increment (live whenever the
   construct pass runs, sub-gates off included):
   - **Assembly-loop livelock fix**: the degenerate-cycle decline
     "dropped the patch's seams" — but a merge/removal HOLDER owns no
     seams, so the restart repeated the identical state forever
     (measured: 18,322 restarts to a CPU-budget TIMEOUT; reachable on
     main whenever `YANG_441_CORNER_MERGE` applies and a merge-only
     holder's cycle degenerates — every earlier applied-merge state
     escaped it only because the diagnosis rounds were census-only).
     The decline now lands on the merge pairs that pulled the holder in
     (`merge_blocked`), with a loud whole-batch refusal if neither a
     seam nor a merge is attributable.
   - **Attribution-hole loudness**: a scoped patch with mixed/absent
     attribution now prints its invisibility instead of silently
     skipping its runs.

   With the livelock fixed, the CORNER_MERGE-applied path on F0067 runs
   to a verdict instead of spinning: Extrude 10 fails as a Stage-6
   non-2-manifold (37/21 batch) — the applied-merge degradation family.
   The merge sub-gate stays OFF; no corpus configuration is affected.

   **Full-corpus gate-ON assay (2026-08-10, main gate only):
   258C/0W/50E/0T with the ERROR set BYTE-IDENTICAL to the canonical
   gate-OFF baseline — zero deltas attributable to this increment.**
   The comparison also surfaced a pre-existing loss: **the F0085
   conversion (I2a's first-and-only corpus conversion) did NOT survive
   the dashu-ratio 0.4.4 / lockfile pin** — single-case bisect: the
   identical `NonPlanarFace` ERROR at Extrude 20 (face id drift
   37928→37935) on committed MAIN with the working diff stashed, so it
   is not this increment's mint. The 2026-08-09j pin session
   re-baselined gate-OFF only; today's is the first post-pin gate-ON
   assay. Gate-ON and gate-OFF now measure IDENTICAL scores: the
   construct pass currently converts nothing corpus-wide under the
   pinned environment (a worklist fact, not a regression — the honest
   ERRORs are unchanged).

   **J1-0 + J1-1 (2026-08-11, LANDED sub-gated `YANG_441_BOUNDARY_EXIT`;
   the mechanism is VALIDATED on F0067 — 22 of 28 corners close in one
   pass-0 batch — and the residual blocker is NAMED: the A-top rim-weave
   CDT family.)** The J1-0 census (`YANG_441_J1_CENSUS`, read-only,
   pass-0, all open line-curve groups incl. minimal) measured the corner
   typology on F0067's failing boolean: ~24 seams carry EXACTLY ONE
   unrelocated ≥3-holder fold corner at the uniform t=0.0936 on the
   junction segment (the 1.339e-3 design gap; seam 17's local gap is
   3.098e-4), plus 2-holder walk-back folds (the I1f class); off-pattern
   members (two-corner span seam 29; junction-less fragmented line groups
   56/59/79/81) refuse loudly. Flush-coplanar structure confirmed: A-top
   `Af328` z=1.751898 ≡ B-bottom `Bf0` (opposite outward normals); the
   junction vertex is held by exactly four patches (A-top, A-wall,
   B-bottom, B-lateral-cylinder), occ=1 each — fused, no pinch.

   J1-1 = the Fig-11(b) merge with the BOUNDARY-EXIT DIRECTION: the
   relocated junction terminal J substitutes INTO the kept-boundary
   corner C (the reverse of the containment merge — here the JUNCTION is
   the vertex with zero kept content), through the same holder closure /
   `subs` machinery, with the collapse, near-curve removal (extended to
   exit-fixed minimal seams), and re-CDT in one batch. Selector guards,
   all loud: relocated+conic-terminal J only; straightness identity;
   exactly ONE unrelocated ≥3-holder corner strictly inside the
   (J, first-sample) span; no relocated vertex in span; direction- and
   chained-substitution conflicts with other merge arms.

   Three measured corrections en route (each its own F0067 run):
   - **Re-point-safe holders**: pulling every holder blocked ALL pairs —
     the encircling drum-lateral declines `ThetaUnwrap`. A holder is
     exempted from the re-CDT batch iff q appears NOWHERE in its cycles
     or triangles (pure relabel of p's slot); mere non-adjacency is NOT
     enough — rebuilt=[] on walk-back corners re-minted the inc-3
     bare-collapse pinch shape. With the contains-q rule the lateral
     re-points and the wall/A-top/owners rebuild.
   - **Pull attribution (`required_by`)**: blaming a declined holder's
     failure on every pair whose p sits on its cycles blocks the whole
     rim through the lateral (it holds EVERY junction). A declining
     holder blames exactly the pairs that REQUIRED its rebuild.
   - **Blame priority** at every decline/degenerate/audit site:
     required-pairs → own seams → incidental pairs → whole-batch refusal
     (the livelock-fix generalized; a baseline seam is never sacrificed
     for a merge's sin — the 39→37 seam loss of the first wiring).

   Measured end state (F0067, construct+boundary-exit): **42 seams over
   84 patches apply in pass 0** (baseline 37/57); walk-back removals fire
   (18–25/batch, previously all blocked); 22 corners close with junction
   vertices merged into their exact designed corners and rim chains shed
   their terminal chords. The 8 blocked pairs are all scoped to A-top
   patches (328/330/331/332/333/337/344) whose re-CDT declines on the
   PRE-EXISTING rim-weave: curved-seam × plain crossings at r≈0.2085–
   0.2098 (e.g. patch 332: (1029,1047)plain × (1050,1035)curved-seam) —
   the top patch's relocated rim-circle chain woven against its plain
   chord edges, present at baseline and independent of the corner family.
   The case verdict is UNCHANGED (the canonical Stage-6 non-2-manifold;
   note the pinned-environment canonical for F0067 is non-2-manifold, not
   the pre-pin ring-reject). **Named next wall: the A-top rim-weave —
   the relocated circle chain vs plain chord boundary on the top patches
   (owner of the remaining 6 corner blocks, and in the canonical failure
   family). Also surfaced loudly: `MalformedPatch` on the fragmented-
   line-seam owners (697/732), and the femto `DuplicateVertex` pair
   (1227/1162-class) now blocks two corners directly.**

   **RIM-TRIM (2026-08-11, same day, LANDED sub-gated
   `YANG_441_RIM_TRIM`; the mechanism converts the 2-holder debris and
   the residual is scoped to the FLUSH-INTERFACE SLIVER FRAGMENTS — the
   M8-residue boundary.)** The rim census (`YANG_441_RIM_CENSUS`,
   annotated per-vertex dump in the decline path: signed radial delta to
   the cycle's trim circle, chain membership, relocation identity)
   measured the A-top decline family exactly: the circle chain is IN
   parameter order (not an I2b defect); the weave is plain unmoved
   boundary vertices dipping INSIDE the circle (dr −3.1e-4…−1.3e-3, the
   chord-sliver ramps, symmetric on both flanks of each gap) poking
   through the chain's shallow chords — the 2026-08-02 CDT-ring anchor's
   "visits both sides of its own trim circle", re-measured post-J1.

   The increment: §4.4.1's near-curve removal generalized to CIRCLE
   chains, side-aware — a candidate is plain (no incident curve edge
   anywhere), unmoved, not part of any substitution, not a J1 exit
   corner, and strictly on the NON-KEPT side of the patch's circle
   within the derived band. Kept side is witnessed by chain-edge
   triangles' third vertices BEYOND the band (a within-band witness is
   potentially the sliver itself — measured: 1–4 inside witnesses on
   every declining top, all within band), with a boundary-majority
   fallback when the near-rim mesh is denser than the band; ambiguous ⇒
   loud skip (the encircling lateral lands here correctly). Candidates
   get their own holder closure (flush-interface debris is legitimately
   3–5-holder; chartable holders join the batch as trim participants;
   the ≤2-holder cap is exempted for trim candidates — design corners
   are excluded structurally, not by count), `trim_pull` attribution,
   and a `trim_blocked` arm in the blame chain at all four decline
   sites (required pairs → trim vertices → seams → incidental → STOP).

   Measured on F0067 (construct + boundary-exit + rim-trim): batch
   scales to 42 seams over 138 patches, removals to 68/batch; the
   2-holder ramp debris removes cleanly (e.g. patch 332's cycle 85→83,
   its crossing shifting to the surviving vertex) and TWO tops (340,
   346) convert outright. The rest of the A-top family still declines:
   the surviving ramp vertices (1048/1091-class) are held by
   TRIANGLE-SCALE SLIVER FRAGMENT patches whose cycles would degenerate
   below 3 vertices — the removal blocks correctly (deleting the
   fragment is the right exact-geometry outcome, but the sliver area is
   interface-INTERIOR at the flush plane and shell closure after
   deletion needs the Stage-0 overlap knowledge). **The rim-weave's
   residual is therefore attributed to the M8 flush-overlap residue
   (roadmap item), not to this epic's construction machinery.** F0067's
   verdict is unchanged (canonical non-2-manifold); the femto
   `DuplicateVertex` owners and the fragment wall carry the remaining
   corner blocks.

   **I2d (2026-08-15, LANDED with the I3 flip): the d(T) recompute gate —
   Yang §4.4.1's closing sentence wired** ("For the newly generated
   boundary triangles around the intersection curve, we recalculate d(T)
   to maintain controllable error",
   `refs/text/yang2025_hybrid_boolean.txt:568-571`) — and the FIRST
   production consumer of the N2-2 `stage4_dt::d_of_t` primitive.
   Surfaced by the first gate-ON run of the yang-rs pin suites (the
   2026-08-14c stale-pin lesson paying its bill):
   `kv6b_revolve_ingest::partial_revolve_union_and_subtract_box` measured
   the 90° partial-revolve ∪ box union at volume 6.199 against operand
   volume 6.888 — **union monotonicity broken by −10 % on a watertight,
   topology-clean output**. Pre-flip gate-ON reproduces identically, so
   it is a LATENT of the landed I2a machinery; the corpus never saw it
   because the composition oracle measures the render tessellation
   re-derived from the output B-Rep — yang's pipeline witness mesh has
   no corpus oracle; the unit sandwich is the only instrument at that
   layer. Anchored with the shipped bisect probes
   (`YANG_441_APPLY_SEAM_CAP` 0/1/2): seam 2 (planar pair) is clean;
   **seam 5 — the box-top × outer-cylinder ruling — is the whole loss.**
   Mesh diff (14 banded tris replaced by 10): the wall's 22.5°-column
   banding becomes fans from the two seam terminals spanning up to ~75°
   of θ — every vertex EXACTLY on the cylinder (r = 2.0 to the last
   digit), watertight, and the secant triangles shave the cylindrical
   bulge: 1.490 exactly. The mechanism is METRIC, not a backend bug: the
   chart CDT keeps only CYCLE constraints, the replaced banding's
   fidelity lives purely in rim-to-rim CONNECTIVITY (the wall has zero
   interior vertices, so I2a's carry has nothing to carry), and chart
   Delaunay under the θ-radians × world-units aspect distortion PREFERS
   wide-θ triangles wherever mid-span vertices are sparse — the notch
   vertices invade the tall band quads' circumcircles, so the fans ARE
   the Delaunay optimum in the chart.

   The gate: `rebuild_patch_planar` certifies max d(T) over the OLD and
   the NEW triangle sets (chart-frame uv is valid input for the
   certified bound — d(T) is invariant under the cylinder's isometries,
   and both frames are ortho_basis azimuth-radians × world-units axial)
   and refuses `ChordDegradation { old_max, new_max }` when the rebuild
   certifies COARSER than the triangles it replaces. **The budget is the
   patch's own pre-rebuild certified max — like for like, tolerance-free,
   no external constant.** (An absolute Stage-1 `d_ε` budget was
   considered and REJECTED: the certified control-net bound over-reports
   true sag ~2×, so a `d_ε` gate would refuse Stage-1's own banding.)
   `ChordCertify` refuses loudly when a triangle cannot be certified
   (θ-branch outside the unwrapped span / `DtError`). Planar patches are
   exempt by identity (d(T) ≡ 0 for any triangulation of a plane
   polygon — the planar family stays byte-stable). Measured: kv6b
   declines exactly the wall rebuild (certified 0.755 → 1.549) while the
   planar seam-2 collapse still applies; the union volume is restored;
   unit pin `rebuild_cylinder_chord_gate_declines_secant_coarsening`
   (the wall in miniature: notched θ∈[0,π/2] drum, dense rims, no
   interiors). **Named follow-on I2e (capability): seed the curved
   rebuild's CDT interior at the surface's own chord spacing** (the
   shipped `cdt_polygon_with_holes_refined_seeded`, Stage-1's own `d_ε`
   formula; the batch write-back learns appended vertices) so wide-θ
   curved rebuilds pass the gate instead of declining. Until I2e, they
   decline loudly and ship their prior state — capability withheld,
   never silent coarsening.

   **I2e (2026-08-15, same session, LANDED on the main path): seeded
   curved rebuilds.** A seedless curved rebuild that declines
   `ChordDegradation` is retried with a deterministic interior seed grid
   (`i2e_seed_grid`) at the patch's own pre-rebuild θ-arc sampling scale
   — `old_arc_span` = max old-triangle θ-span × radius, measured in the
   same certification sweep that prices the budget — halved once on a
   second failure; the I2d gate re-verifies every attempt, so a rescue
   is never taken on faith. Attempt 0 stays seedless and byte-identical
   wherever it already passes (the planar family and column-local
   cylinder rebuilds are untouched). Seeds are chart points strictly
   inside the polygon (even-odd test), outside every hole, with
   0.25·spacing arc-metric clearance from all constraint edges (a seed
   ON a constraint would make spade split it — the F0059 hazard;
   clearance is quality-only, it selects OPTIONAL seeds and cannot make
   an accepted rebuild wrong), capped by a 4096-seed runaway backstop
   (empty grid → the loud decline survives). `PatchRebuild` carries
   `new_verts` (chart-lifted, exactly on-surface — `lift` is
   2π-periodic so the unwrap shift is a world-space no-op), referenced
   as `plan_verts + k` and remapped onto the appended block by
   `apply_rebuild_batch`. The new-triangle certification moved into the
   CDT's own frame (pool coordinates — the very parametrization the
   triangles were built in), which also retires the mid-rounding
   re-projection's spurious `ChordCertify` on wide unwrapped spans; the
   OLD-triangle budget keeps the projection+containment path (filtered
   boundary-edge vertices are not in the pool). Measured: the kv6b wall
   seeds 9 points at arc spacing 7.854e-1 — exactly the original 22.5°
   banding scale — seam 5 APPLIES, the union volume holds the sandwich;
   the squashed-drum twin passes SEEDLESS (chart aspect inverted —
   column-local Delaunay, no degradation, no seeds: attempt-0
   byte-stability pinned). Unit pins:
   `rebuild_cylinder_chord_gate_seeds_wide_theta_rebuild`,
   `rebuild_cylinder_squashed_drum_passes_seedless`,
   `apply_rebuild_batch_appends_seed_vertices_and_remaps`,
   `i2e_seed_grid_*` (clearance/holes/degenerate-input arms).

3. **I3 — flip per wall class** (cdt-ring-rejected → relocation-region →
   reassembly-non-2-manifold), full assay after each; any conversion or new
   wall is censused before the next flip.

   **STATUS (2026-08-15): FLIPPED ALWAYS-ON — one step, all wall classes
   at once, because the flip census measured ZERO category deltas in
   every class.** Precondition (same morning, first gate-ON corpus after
   the rim-refine flip): `YANG_441_CONSTRUCT=1` full corpus
   **259C/0W/49E/0T with the ERROR set case-identical to canonical** —
   the per-wall-class ordering existed to bound conversion risk, and
   with zero deltas everywhere one measurement satisfies every per-class
   census (spec principle over literal). Mechanics:
   `run_construct_passes` is invoked unconditionally; the historical
   `YANG_441_CONSTRUCT` env var now only re-enables the pass's
   diagnostic chatter (`c441_verbose()` = CONSTRUCT ∨ VERBOSE — every
   recorded diagnostic workflow sets the main gate, so their output is
   byte-identical), the six anomaly STOPs (three whole-batch refusals,
   the patch/info correspondence break, degenerate-with-no-blame, the
   write-back refusal) stay unconditional `eprintln!`s, and the four
   sub-gates are untouched. The flip enables the §4.4.1 construction in
   the WASM app for the first time (`env::var_os` is always `None` on
   wasm32; kernel-v2's `from_yang` consumes only the output B-Rep, so
   the witness-mesh layer had never been app-visible either way).

   **The flip census paid the 2026-08-14c stale-pin bill on schedule:**
   the first-ever always-on run of the yang-rs pin suites caught the
   I2a latent (I2d above — the kv6b union-monotonicity silent-wrong at
   the witness-mesh layer), fixed in the same increment by the d(T)
   gate. Census results, all with I2d in: yang-rs 75 test targets green
   always-on; full corpus 259C/0W/49E/0T at the NEW ≥300s budget — the
   two 20-op stacks crossed the old 240s budget HONESTLY under the
   construct fixpoint's added CPU (F0065 ≈ 241s CORRECT, F0085 ≈ 242s
   ERROR at Extrude 20, both single-case adjudicated;
   `docs/TESTING.md` + root `CLAUDE.md` budget guidance raised
   240 → 300). Remaining per-case drifts vs the pre-flip canonical: the
   8 recorded within-ERROR face-id/vertex renumbers (R0020 R0025 R0053
   R0070 R0082 R0085 R0095 R0100) plus F0085's Extrude 19 → 20 advance
   (its planar collapses still apply; its cylinder-owner rebuilds now
   run under the d(T) gate).
4. **I4 — retire the relocate-in-place path** once I3 holds the score at
   ≥ parity with zero new WRONGs; the §4.5.3 reversal sweep stays (it acts on
   the curve polyline, which the paper orders the same way).

   **I4-0 CENSUS (2026-08-15, precondition met by I3): "the
   relocate-in-place path" no longer exists as a retireable THING — the
   naked substitutes were retired incrementally along the way, and every
   live Stage-4 pass implements quoted paper text or is a P10 gate.**
   Full inventory of `stage4_relocate_and_correct` by phase:
   - *Phases (1)/(2)/(2t)* — per-vertex exact-target assignment (conic /
     junction / M5-Newton closed forms, no-skip audits) + relocation:
     the paper's own "set r_A = r_B = r". KEEP.
   - *Phase (3)* — §4.5.3 reversal sweep: STAYS by this spec's own I4
     line. Its pre-steps: the P3b minted-junction welds and
     `retriangulate_collapsed_fan_regions` (inc-4c, the §4.4.1
     triangulation-update half of those welds, fail-closed) — paper
     roles, KEEP. `trim_beyond_corner_phantoms` — a LOCAL Fig-11(a)
     implementation for the curved-pierce minted-corner family (zero
     kept content past the boundary exit IS the paper's q-on-boundary);
     retirement condition = the J1 boundary-exit authority
     (`YANG_441_BOUNDARY_EXIT`) flipping always-on, which generalizes
     it. KEEP until then.
   - *Phases (3c)/(3d)* — §4.4.1(b) sub-feature merge (N55, TAU_WORK
     scale-relative) and §4.4.1(a) edge-split at q. Quoted paper text.
     KEEP.
   - *Phases (4)/(4a1)/(4a2)/(4b)* — relocated-triangle validation,
     doubled-membrane removal, tangency pinch split, the §4.4.3
     watertightness gate. P10 gates / topology sanitation with own
     specs. KEEP.
   - *Boundary-curve relocation* (rim snap, always-on since its inc-5)
     — Fig-11 "map boundary curves to boundary curves" for same-input
     rims. KEEP.
   - *N55/N56 merges, #194 sub-TAU_WORK collapse* — compliant
     always-on. KEEP.
   - *Gated-off banked development paths (NOT drivers — §3 item 5's
     "retire them as drivers" is the de-facto state)*:
     `detect_nonmanifold_seams` lives only behind `YANG_MESHUP_REGION`
     (probe) / `YANG_MESHUP_ENABLE` (the banked splice, measured
     necessary-not-sufficient for its bucket); `stage4_fold_risk`'s
     only consumer is the `YANG_S4_FOLD_RISK` experimental merge-plan
     arm (diagnostic, per §3 item 5); `YANG_N2_RECDT_ENABLE` (the #168
     replan) stays banked on its recorded §5c.6 generator-seam
     conformality wall. All keep their recorded re-entry conditions.
     The named follow-up (re-measure the replan gate post-I2e) was
     RESOLVED 2026-08-15: full gate-ON corpus BYTE-IDENTICAL — premise
     not dissolved (the Stage-4 STOP is upstream of the Stage-5
     construct pass); `specs/yang_n2_stage4_cdt_mesh_updating.md`
     §5c.12.

   **I4-1 (2026-08-15, LANDED): the last relocate-era hack arm DELETED.**
   `weld_enabled("f32")` — the N50 f32 render-twin weld, the weld
   audit's sole confirmed hack (non-geometric f32-render-precision
   identity, nowhere in the paper, regresses C0036, redundant since the
   N56 §4.3 dedup recovers its cases) — was callable behind
   `YANG_WELD_ENABLE` as a historical A/B artifact. The arm and the
   `weld_enabled` gate are removed; `weld_f32_render_twins` survives as
   a unit-tested banked primitive (`tests_unit/n50_f32_render_twin.rs`,
   4 oracles green). Production byte-identical BY CONSTRUCTION (the env
   was unset in every production and CI path); corpus re-verified.
   Ledger updated (§N50 + the weld-audit table).

   **I4 VERDICT: RESOLVED BY CENSUS + I4-1.** The epic's remaining tail
   is CAPABILITY, not retirement: §4.3.4 h/l/α density refinement of
   the seam polyline (conic chains are mesh-inherited density), the
   §4.5.2 local-refinement loop (§5 below), and the sub-gated
   increments' own flip conditions (J1 / CORNER_MERGE / INPUT_REFINE /
   RIM_TRIM, each with recorded blockers).

5. **I5 — §4.3.4 seam-polyline density refinement** (DESIGN 2026-08-15;
   census-first).

   **Paper requirement** (`refs/text/yang2025_hybrid_boolean.txt:575-593`):
   the optimized intersection polyline "may be uneven and sparse"; between
   consecutive points p, q insert an (optimized) midpoint m and terminate
   ONLY when ALL of

       h < d_p·10²,   l < d_p·10³,   α < π/18

   hold (h = arc height of m over chord pq in 3D, l = max(|pm|,|mq|),
   α = turning angle p→m→q); else recurse on p–m and m–q. d_p = 1e-7
   (`:744-745`) = this port's TAU_MODEL, scale-relative per the shipped N58
   convention (`paper_chain_sample_redundant`, stage4_correct.rs — the SAME
   predicate, used today only in the REMOVE direction). For closed-form
   conics the paper's "midpoint optimization" is exact evaluation at the
   wrapped parameter midpoint (spec-principle-over-literal: their numeric
   optimizer exists because their surfaces are general; our conic sections
   have closed forms — the same substitution Stage-3 already makes).

   **The gap**: I2b's conic action is REORDER-only — the seam chain keeps
   the relocated mesh crossing vertices as its sample set, so seam density
   is inherited from the Stage-1/2 tessellation. Note the scale mismatch
   this leaves: the paper's seam sagitta bound (h < ~1e-5·scale) is ~100×
   TIGHTER than the d_ε surface-mesh bound — §4.3.4 makes the seam
   polyline the highest-fidelity element in the pipeline, deliberately
   (it becomes the trim boundary both patches share). A mesh-inherited
   chain is at d_ε sagitta, two orders coarser than the paper's floor.
   Corollary: the l-term alone (chord < ~1e-4·scale) implies seam chains
   MUCH denser than Stage-1 chords (a r=0.05 rim circle → ~1.6k samples vs
   Stage-1's ~32–64) — whether §4.3.4-as-written is affordable, and where
   its density actually bites, is a MEASUREMENT, not an assumption. Hence:

   **I5-0 — the census (read-only, this increment).**
   `paper_chain_metrics(p, m, q)` factored out of
   `paper_chain_sample_redundant` (predicate byte-identical); a
   `conic_eval(curve, t)` primitive (Circle/Ellipse closed-form eval,
   unit-pinned round-trip against `conic_param` — this is also the future
   insert primitive); an env-gated probe (`YANG_434_CENSUS`) in the
   construct-pass eligibility loop that, for every ordered conic seam
   chain, reports per seam: pair count, per-criterion failure counts
   (h / l / α / any), and the implied §4.3.4 insertion count (bounded
   recursive simulation, loud cap). Runner: in-process campaign
   `s434_density_census.rs` (test-harness, `#[ignore]`, sets the env and
   replays corpus cases — the assay nulls child stderr, recorded trap).
   Targets: F0059 (764 conic reorders, CORRECT — the at-scale picture),
   curved CORRECT representatives, and whichever ERROR-family cases reach
   the construct pass. Byte-identical off AND on (read-only probe).

   **I5-0 MEASURED (2026-08-15, 9-case sweep via `s434_density_census`).**
   Per case (seams / pairs / fail-any% / implied inserts = ×density):

   | case | verdict | seams | pairs | fail% | implied | ×dens | worst h, l, α |
   |---|---|---|---|---|---|---|---|
   | F0059 | CORRECT | 8 | 44 | 100% | 16724 | 381× | 5.8e-3, 8.0e-2, 8.5° |
   | F0045 | ERROR (CDT) | 2 | 40 | 100% | 9177 | 230× | 5.2e-3, 5.5e-2, 11.2° |
   | R0011 | ERROR (CDT) | 25 | 38 | 100% | 6016 | 159× | 1.5e0, 2.5e2, 1.2° |
   | R0021 | CORRECT | 3 | 112 | 97.3% | 2233 | 21× | 6.0e-4, 6.9e-3, 9.9° |
   | R0072 | CORRECT | 4 | 32 | **0%** | 0 | 1× | 2.0e-6, 2.9e-5, 7.8° |
   | C0053 | CORRECT | 0 | — | — | — | — | SurfacePair (M5) unorderable |
   | C0067/R0028/R0085 | ERROR | 0 | — | — | — | — | wall upstream of the pass |

   Verdict, in decreasing force:

   1. **The gap is universal at ordinary model scale and 2–3 orders deep.**
      Every censused conic seam on models at scale ≳0.1 fails the paper's
      acceptance on ~every pair (97–100%); h and l are exceeded ~20–400×
      (R0011's large-scale model: 1000×). The α turning-angle term passes
      almost everywhere (worst 11.2° vs the 10° bound) — the chains are
      angularly adequate but chordally coarse. So §4.3.4-as-written here
      ≈ uniform arc-length resampling at the d_p·10³ chord floor, and the
      insertion count has a closed form ≈ arc_len/(d_p·10³) — the I5-1
      gate can PRICE a seam before inserting and decline loudly.
   2. **Implied densification is 21×–381×, bounded.** No case capped the
      simulation; worst measured total 16.7k inserts (F0059). Affordable
      in vertex count; the CDT/rebuild cost is the open question I5-1
      measures behind its gate.
   3. **Scale-dependence measured**: the scale-relative d_p floor makes
      sub-unit models trivially compliant — R0072 (model ~5e-4) passes
      every pair TODAY. Demand concentrates on ordinary-scale models.
   4. **Fidelity implication**: worst h ≈ 5.8e-3 on a CORRECT case means
      today's seam polylines deviate from the exact curve at ~d_ε scale —
      invisible to the corpus composition oracle (the I2e LAYER GAP) but
      inherited by every witness-mesh consumer. §4.3.4 is the paper's
      mechanism for making the seam ~100× tighter than the surface mesh.
   5. **Coverage boundaries for I5-1** (all must stay loud declines):
      procedural `SurfacePair` curves have no closed-form param/eval (the
      cyl×cyl M5 track — C0053); `SeamNotSimple` unorderable chains
      (C0053, R0021 ×1, R0011 ×2); cases walled upstream of Stage 5 are
      out of blast radius until their own walls retire.

   **I5-1 — the insert (gated `YANG_434_INSERT`; scope set by I5-0).** For
   a conic seam failing the criterion: insert `conic_eval` midpoint samples
   (wrap-aware parameter midpoints, recursion per the paper, priced up
   front by the closed-form estimate from I5-0 verdict 1; deterministic
   depth cap → loud decline on exhaustion) as NEW mesh vertices appended
   to the pool; both owner cycles receive the SAME vertex indices (seam
   identity by construction — the I1b batch rebuild already re-CDTs both
   sides against the modified cycles); the I2d d(T) gate certifies every
   rebuilt patch; I2e seeding rescues interior density where the denser
   rim demands it. Junction endpoints are never inserted past (the
   endpoints stay fixed, as in `order_along_curve`). First targets: F0059
   and R0021 (CORRECT today — regression canaries; proof gates = d(T),
   the composition oracle, corpus 0W). I5-0 measured the implied density
   affordable in vertex count (≤17k/case); if the CDT/rebuild COST proves
   prohibitive at gate-ON, bring the numbers to a deviations-ledger
   decision — never silently weaken the bound.

   **FINDING (2026-08-15, surfaced by the I5-1 probe; PRE-EXISTING): the
   construct pass-loop LIVELOCKS on parameter-descending conic chains.**
   Gate-off F0059 burns all 64 `MAX_PASSES` re-applying the same six
   6–7-vert reorders (64 "BATCH APPLIED" lines in every canonical run
   today). Mechanism, by code inspection: `order_along_curve` returns the
   ASCENDING chain; a cycle that traverses its seam run parameter-
   DESCENDING gets `reorder_cycles_to_curve`'s reversed splice — a NO-OP —
   but the driver's fixed-point test is `ordered == chain`, which a
   descending chain never satisfies, so the action re-fires every pass.
   Correctness-neutral (each extra pass rebuilds the same patches to the
   same state; the 64-cap bounds it) but wasteful — and 4× worse under
   I5-1 where the rebuilt cycles carry ~2k verts (F0059 gate-ON 42.8s).
   FIX (own increment, NOT conflated into I5-1): accept reversed equality
   as the fixed point (`ordered == chain || ordered == rev(chain)`).
   This changes tri ORDER on affected cases (the no-op rewrites still
   moved patch triangles to the tail), so it needs its own corpus
   detail-drift census per the amendment-17 flip precedent.

   **LIVELOCK FIX LANDED (2026-08-15, same day, always-on).** The
   reversed-equality fixed point converges F0059's construct loop 64 → 1
   pass (gate-off probe). Pin suites: all 75 yang-rs test binaries green.
   Full gate-off corpus census: **259C/0W/49E/0T, zero category deltas,
   exactly ONE justified detail drift** — R0070 (ERROR both ways, chained
   case) cites `face 156` instead of `face 202` on the IDENTICAL
   `holed lateral CDT failed: degenerate CDT input` wall: the previous
   no-op rewrites moved patch triangles to the tail of the intermediate
   body's mesh, so retiring them renumbers the faces of the converted
   B-Rep the later boolean rejects. The drifted results.json is the new
   canonical (amendment-17 precedent: zero category drift, renumber-only
   detail drift on an ERROR case). Gate-ON (insert) F0059 single-case:
   42.8s → 7.6s.

   **I5-1 probe results (2026-08-15, in-process gate-ON):**
   - **F0059**: 8 seams REFINE (chains 6–7 → 1697–2305 verts, all under
     the 4096 cap), batches APPLY, case completes with no engine errors;
     42.8s wall (~4× gate-off, the pre-existing livelock included).
   - **R0021**: both rim seams REFINE (22 → 569/537) but the batch
     DECLINES on the pre-existing `ThetaUnwrap { patch: 10 }` wall — the
     encircling cylinder lateral has no single θ branch (the recorded I2a
     tail; gate-off declines identically). The I5 orphan cleanup drops
     the 1062 unapplied inserts; R0021's §4.3.4 density is gated on the
     encircling-patch rebuild capability, not on I5 machinery.

   **I5-1 full-corpus gate-ON sweep (2026-08-15, 300s budget): SAFE but
   COST-BLOCKED by the livelock.** 256C/**0W**/49E/**3T** — zero ERROR
   deltas, ZERO detail drifts on every completing case (the insert never
   silently changes an outcome), but THREE CORRECT curved cases (F0047,
   F0048, F0059 — exactly the cases whose refines apply) exceed the 300s
   CPU budget: the pre-existing 64-pass livelock re-runs the dense
   (~2k-vert) patch CDTs 64×. Gate-off same day: BYTE-IDENTICAL to
   canonical. Disposition: **stays gated pending the livelock fix** — the
   cost is the livelock's, not the density's (single-case in-process
   F0059 completes in 42.8s), so the unblocking increment is the
   fixed-point fix above, then a gate-ON re-measure. Only if cost remains
   prohibitive after convergence does the deviations-ledger decision on
   the bound arise.

   **Gate-ON RE-MEASURE post-livelock-fix (2026-08-15): the cost moved
   DOWNSTREAM; still 256C/0W/49E/3T (same trio F0047/F0048/F0059).** The
   construct pass itself now converges (F0059 in-process 42.8s → 7.6s),
   yet the assay cases still exceed 300s CPU. The residual cost is the
   ~300×-denser seam's LIFECYCLE, not its construction: the refined
   chains survive into the output B-Rep, and the F-series cases CHAIN
   booleans — each subsequent op re-tessellates and re-arranges the
   densified input, compounding per op (the paper's context is a single
   boolean; a chained CAD kernel pays the density repeatedly). NEXT
   ANCHOR (before any bound decision): LOCALIZE where the density cost
   lands — (a) Stage-6 emitting one B-Rep edge per mesh seam segment
   (~2k collinear-on-curve edges per seam?) vs (b) chained Stage-1/2 on
   the densified input vs (c) oracle-side cost. If (a), the fix is
   Stage-6 chain-merging into single analytic edges (density stays in
   the MESH, where §4.4.1 wants it; the B-Rep edge is the exact curve) —
   likely also what the paper intends. Only after that localization does
   the deviations-ledger question on the l-floor arise. I5-1 STAYS GATED.

   **COST LOCALIZED (2026-08-16, task #88; runner
   `s434_cost_localize.rs` — in-process, assay-phase-for-phase, per-phase
   timings + per-feature B-Rep topology counts; `S434_COST_SKIP_SI=1`
   skips the one dominant oracle once measured).** Measured, gate-off →
   gate-ON:

   | case | final-body V/E (F const) | render tris | boolean (load) | SI oracle | leg minus SI |
   |---|---|---|---|---|---|
   | F0059 | 98/124 → 16 822/**16 848** (F=28) | 7.4k → 322k (44×) | 0.09 → 7.97s | 1.00 → **1227.13s** | ~27s |
   | F0047 | 71/79 → 14 956/**14 964** (F=10) | 3.5k → 386k (110×) | 0.07 → 8.17s | ~extrap ≥1200s | 27.2s |
   | F0048 | 82/90 → 7 409/**7 417** (F=10) | 4.7k → 258k (55×) | 0.07 → 10.78s | ~extrap ≥600s | 36.4s |

   Verdict, in decreasing force:

   1. **(a) CONFIRMED — Stage 6 emits one B-Rep edge per mesh seam
      segment.** F0059 ΔE ≈ ΔV ≈ 16 724 = exactly the I5-0 census's
      implied insert count; face count UNCHANGED. Emission point:
      `stage5_topology.rs` `push_loop` (and the `env_extra_faces` push
      site) pushes one `BRepEdge` per cycle segment `(s,e)`, each
      individually curve-tagged from `intersection_curves` — so the
      refined chain survives verbatim as ~2k conic micro-arc edges per
      seam, and this is the shipped architecture even gate-off (the 6–7
      mesh-vertex chains are per-segment B-Rep edges today; the insert
      multiplies it ~300×). The render tessellator must honor every
      B-Rep boundary vertex → the 44–110× mesh inflation at the SAME
      render tolerance.
   2. **The assay-budget killer is (c) driven by (a): the
      `no_self_intersection` oracle** — all-pairs Möller between
      AABB-overlapping face ranges with NO per-triangle broad phase
      (`oracle.rs check_no_self_intersection`), ~quadratic in the mesh
      inflation: 1.00s → 1227s on F0059 = 98% of the 1254.6s leg; the
      300s budget clips ~24% into it. Every OTHER phase combined —
      boolean ~8–11s, tessellation ~0.9s, composition oracle 17.4s
      (incl. its second full model build, 8.75s) — totals 27–36s,
      comfortably under budget, with composition verdicts Agree on both
      legs (the insert is volumetrically clean).
   3. **(b) chained Stage-1/2 REFUTED for the trio**: F0047/F0048/F0059
      are all 2-op single-boolean cases — there is no chained
      re-tessellation in them — and the 20-op chained stacks (F0065)
      completed gate-ON in the 08-15 sweep (their box seams don't
      refine). Density-through-chains remains a real concern for a
      future always-on world but is NOT what the timeouts measure.
   4. **The 08-15 "compounds through chained booleans" attribution is
      RETRACTED** — it was inference from case family names, not
      measurement (the trio was never per-phase-instrumented until now).

   **The named fix is the spec's own (a) branch — I5-1b, Stage-6 conic
   seam chain-merge**: coalesce maximal runs of consecutive loop edges
   sharing the same undirected conic curve into ONE analytic arc edge
   between junctions; interior chain vertices stay witness-mesh-only
   (the `TessellationSource::BRepEdge { edge, t }` mapping retags them
   onto the merged edge). Design walls recorded for the increment: the
   from_yang `near-half-ellipse arc` minor-side ambiguity means runs
   subtending ≳π must split at an interior chain vertex; closed seams
   (no junction) need ≥3 splits (closed-ellipse edges have no assembler
   vocabulary, by design); merging claims the chain lies ON the named
   conic, which the I5-1 refined chains satisfy by construction
   (`conic_eval`-exact) but today's gate-off relocated chains do NOT
   (~d_ε sagitta) — so the merge lands coupled to the insert gate (or
   with an explicit on-curve certification band). Payoff: E returns to
   O(seams), the render mesh returns to render-tolerance density for
   ALL consumers (SI oracle, viewport, file format, chained ops — a
   subsequent boolean re-tessellates an analytic arc at its own chord
   tolerance, retiring the chain-compounding concern structurally).
   Separately noted, NOT the unblock: the SI oracle's missing
   per-triangle broad phase is a harness-scalability wall of its own
   (1s/case gate-off makes it the priciest mesh oracle corpus-wide) —
   worth a grid/BVH pass on its own merits, but a fast oracle would
   still leave a 16.8k-edge output B-Rep in the app; the structural fix
   is the merge. Only if merged-edge density still breaks the budget
   does the l-floor deviations-ledger question arise.

   **I5-1b — Stage-6 conic seam chain-merge (gated `YANG_434_MERGE`;
   DESIGN 2026-08-16, task #89).** Paper basis (§4.4.2,
   `refs/text/yang2025_hybrid_boolean.txt:581-605`): the B-Rep Boolean
   output is "restored as a collection of parameter surfaces and their
   boundary curves" — the boundary curves are "collected and mapped back
   … by fitting the curve in the parametric domain". The paper's B-Rep
   edge is the CURVE; the dense polyline belongs to the mesh. Our port's
   curves are already known analytically (the seam edges carry their
   exact conic in `intersection_curves`), so "fitting" is restoring the
   known curve.

   **Mechanism** (post-pass at the tail of `emit_topology`, one copy for
   both reconstruct paths, strictly inside the gate — gate-off touches
   zero code): walk every emitted loop and coalesce maximal runs of
   consecutive edges carrying the SAME undirected conic
   (`conics_equal_up_to_normal_sign`) into single analytic arc edges.

   - **Elidable interior vertex** (the recover.rs Steiner/T rule, made
     global): across the whole output, the vertex has exactly 4
     loop-edge uses on exactly 2 faces, all 4 edges on the same
     undirected conic. Junction vertices (≥3 faces), curve changes,
     §4B T-subdivision vertices, and loop pinches all fail the count
     and stay — run boundaries by construction.
   - **Certification, not trust** (P10): a run merges ONLY if (i) every
     interior vertex lies ON the canonical conic within the classify
     band (`TAU_EVAL·(1+scale)` — the same band from_yang applies to
     endpoints), and (ii) the chain's `conic_param` sequence is
     strictly monotone (wrap-aware, consistent sign). I5-1 refined
     chains satisfy both by construction (`conic_eval`-exact,
     construct-pass ordered); anything else declines loudly
     (per-segment status quo, censused). This certifies the merge
     geometrically instead of coupling it to the insert gate's env var.
   - **Minor-side splitting**: pieces are capped at 2.0 rad sweep
     (comfortably under π − `ARC_MINOR_AMBIGUITY_BAND`), split at
     existing chain vertices nearest equal sweep fractions; closed
     runs (whole-loop circles/ellipses) split into 4 arcs (satisfies
     the ≥3-edge loop floor; avoids `Full` vocabulary in seam context
     and closed-ellipse edges, which the assembler rejects by design).
     Corollary: two merged arcs on the same curve can never share both
     endpoints (each < π ⇒ sum < 2π ⇒ not complementary), so the
     same-curve-bigon reject is unreachable.
   - **Twin conformance by construction**: candidacy (global counts),
     the canonical undirected curve (normal sign fixed
     lexicographically), params, and split selection all derive from
     undirected data, so both owners produce identical piece
     boundaries; each side orients its copy per traversal via
     `orient_directed_curve` (sweeps < π, the minor-side regime it
     assumes).
   - **Sources stay valid**: elided vertices retag to
     `BRepEdge { edge: <piece>, t: conic_param }` against the emitted
     piece (first-copy convention); surviving `BRepEdge` sources are
     index-remapped; `mesh`/`as_mesh` are untouched (density stays in
     the witness layer, where §4.4.1 wants it).
   - **Scope**: `Circle`/`Ellipse` runs only (the I5-1 insert's own
     scope — `conic_eval` closed forms). `Parabola`/`Hyperbola`/
     `SurfacePair`/`LineSegment` runs stay per-segment (recorded
     boundary; straight-run fusion is a separate concern).

   **Payoff**: output E returns to O(seams) regardless of witness
   density; kernel-v2's render tessellation samples analytic arcs at
   render tolerance (the 44–110× mesh inflation and the 1227s SI-oracle
   cost collapse); a CHAINED boolean re-tessellates the arc at its own
   chord tolerance, retiring the chain-compounding concern
   structurally. Proof gates: yang-rs pin suites, kernel-v2 suites,
   gate-off byte-identity (by construction: the pass is a single gated
   call), gate-ON trio re-measure via `s434_cost_localize`, full
   gate-ON corpus (with `YANG_434_INSERT` also on) expecting the 08-15
   category baseline restored (259C/0W/49E/0T) with the trio back
   under budget.

   **I5-1b LANDED GATED (2026-08-16, task #89).**
   `stage5_seam_merge.rs`: global elidability census → per-canonical-
   chain cached decisions (on-curve certification at the classify band,
   wrap-aware strict monotonicity, sweep-capped splits with a π-guard
   re-verify) → loop/edge rebuild with sources remap. 7 module unit
   tests (closed-ring 4-arc merge + twin conformance, junction split,
   off-curve decline, non-monotone decline, sweep-cap split at exactly
   π, segment loops untouched, sources retag round-trip); all 75
   yang-rs and 37 kernel-v2 test binaries green; rewrite tier green.

   **Trio measured (s434_cost_localize, insert+merge vs insert-only vs
   gate-off):**

   | case | insert-only | insert+merge | gate-off | E off→merged |
   |---|---|---|---|---|
   | F0059 | 1254.6s (SI 1227s) | **3.39s** (SI 0.79s) | 1.17s | 124 → **88** |
   | F0047 | ≳1200s est | **2.39s** | 0.39s | 79 → **43** |
   | F0048 | ≳650s est | **1.03s** | 0.55s | 90 → **46** |

   Merged bodies are SMALLER than gate-off (refined chains coalesce to
   junction-to-junction arcs with zero interior B-Rep vertices); render
   meshes return to render-tolerance density; composition verdicts
   Agree with near-identical bands on every leg. The insert-only 7.97s
   "load" is also explained: mostly engine-side tessellation of the
   16.8k-edge body (0.86s merged).

   **Full corpus, both gates ON (2026-08-16): 258C/0W/48E+1EE/0T —
   the TIMEOUTs are gone but the census is NOT category-clean.** Vs
   the insert-only baseline (which was delta-free apart from the trio),
   the merge moves five categories and drifts several chained ERROR
   details — ALL in CHAINED cases, where the merged output feeds the
   next boolean through to_yang re-tessellation and its
   sample-position-sensitive walls fire differently:

   - **C0105, R0028 ERROR → CORRECT** (chains that previously died on
     dense/degenerate per-segment intermediates now complete).
   - **C0117 CORRECT → ERROR**: the next subtract's to_yang Stage-1
     tessellation of merged kernel FaceId(6) hits "ring rejected by
     CDT (degenerate/self-intersecting)" — the arc-ring SAMPLING wall,
     not merge output validity (from_yang accepted the body).
   - **F0067, R0099 CORRECT → UNSUPPORTED(coplanar)**: the re-sampled
     merged intermediate now trips Stage-0's coplanar coincidence
     detection — the honest M8 wall, reached because facet coincidence
     is sampling-dependent.
   - **F0085** fails at Extrude 19 instead of 20 (`NonPlanarFace`),
     **R0015** Stage-4 `OffCurveBeyondChordBand`, **R0070** holed-
     lateral CDT wording — same class: chained re-entry perturbs
     sample-sensitive bands.

   **Merge-only discriminator**: C0117 fails IDENTICALLY with
   `YANG_434_MERGE` alone (insert off) — the gate-off relocated chains
   DO certify on-curve at the classify band and merge. Two
   consequences: (1) the merge is its OWN behavior change, not an
   insert rider — the certification-not-env-coupling design decision
   is validated but means the gate must stay OFF until adjudicated;
   (2) the C0117-class walls are downstream arc-ring sampling, present
   for coarse merged arcs too.

   **Disposition: LANDED GATED, gate stays OFF** (I5-1 precedent —
   the recorded deltas are the flip's work list, not a reason to hold
   the primitive out of tree). Adjudication list for I5-2, in order:
   (a) anchor C0117's to_yang arc-ring CDT rejection (the only
   CORRECT→ERROR); (b) F0085 `NonPlanarFace` at the merged
   intermediate; (c) R0015 chord-band sensitivity; (d) decide whether
   F0067/R0099's honest M8 boundary is acceptable capability loss at
   flip time or gated per-case; (e) re-run the merge-only corpus to
   census the coarse-chain merge on its own.

   **I5-2 (a) — C0117 ANCHORED and FIXED (2026-08-19).** The recorded
   attribution ("the next subtract's to_yang re-tessellation of merged
   FaceId(6)") was WRONG: C0117 has ONE boolean, and the failing call is
   kernel-v2's own post-assembly render gate
   (`validate_boolean_output_self_intersection` → `tessellate`) on the
   OUTPUT solid's z=0 annular cap. Mechanism, measured with
   `KV2_RING_REJECT_PROBE` + `KV2_RECOVER_PROBE`:
   - Gate-off, the bore rims reach `recover.rs` as 630 arc pieces (315
     lattice + 315 diagonal-crossing verts) that RETAIN the Stage-1
     lattice, so its canonical-lateral pairing finds an azimuth-aligned
     vertex pair → canonical `[rim, seam, rim, seam]` bore lateral +
     canonical annular caps, and every ring is sampled at N=72 from a
     seam at azimuth 0 — in phase with the boss.
   - Gate-ON, each rim is 4 merged arcs whose split verts are chosen
     PER RIM (z=0: −0.857°/89.14°/179.43°/−90.29°; z=2 the mirror set) —
     no aligned pair → `closed_fallback_pieces` (the recorded arc
     "re-entry wall") → the caps take the general planar path, which
     samples each arc from its own start; the hole ring runs 0.86° out
     of phase with the outer circle and, at a 1e-4 wall vs a 4.8e-4
     sagitta, the two 72-gons cross → "ring rejected by CDT". Threshold
     measured: the same geometry passes at ≥5e-4 gap and fails at 1e-4.
   - Gate-off's phase agreement is NOT design: with the tool sketch
     frame rotated 2.5°/30° the bore rims STILL align at 0° because the
     arrangement seeds the boss-lattice azimuths onto the bore rings
     (cap-triangulation edge crossings). Coincidence-by-arrangement, and
     exactly what the I5-1b elision removes.
   - A canonical bore lateral with an ARBITRARY seam azimuth is not
     enough either: the two coaxial laterals' fixed-N render rows then
     interpenetrate (`SelfIntersectingBooleanOutput`, 426 pairs) — a
     sub-sagitta wall renders self-consistently ONLY in phase.

   **Fix — kernel-v2 `recover.rs` typed-rim canonicalization (two-pass
   pairing).** Pass 1 = the former rule (existing aligned pair;
   deterministic; untouched ⇒ gate-off byte-identical). Pass 2, for a
   two-closed-rim cylinder/cone lateral with no aligned pair: the seam
   foot azimuth is kernel-v2's OWN representational choice, so choose it
   as `construct::extrude` does for a holed profile — COHERENT with an
   already-anchored coaxial lateral of the same output (else the face's
   own deterministic rim-a vertex) — and MINT the exact on-circle foot on
   any rim lacking a vertex there (same geometry, one representational
   vertex; never moves a vertex; a preset anchor from a shared chain is
   respected, two conflicting presets decline to the arc fallback).
   Corollary: the F0086-class "closed rims without a canonical pairing"
   fallback now fires only for non-circle rims or ≠2-rim faces.
   Pins: `kernel-v2/tests/s434_typed_rim_seam_mint.rs` (merged 1e-4
   tube canonical + in phase + 568-tri render identical to gate-off;
   gap sweep 1e-3…1e-5; 30°-rotated tool locks to the boss azimuth;
   minted foot on-circle at ε; gate-off control). C0117 gate-ON →
   SUPPORTED_CORRECT (assay single_case). Note for the F6 typing
   migration (`specs/yang_output_curve_typing_migration.md` I2): the
   seam-foot canonicalization is kernel-v2 vocabulary, not chord repair
   — it STAYS when the re-fuse half shrinks to an assertion.

   **I5-2 census after the recover.rs fix (2026-08-19), full corpus,
   300s budget — SUPERSEDED the same day by the identity-fix census
   below (these deltas were the zero-merge reorder artifact):**

   | config | score | category deltas vs canonical 259C/0W/49E/0T |
   |---|---|---|
   | gate-off (fix only) | 259C/0W/49E/1EE/0T | NONE — zero category, zero detail deltas |
   | `YANG_434_MERGE` only | 260C/0W/47E/1EE/0T (+3 UNSUPPORTED) | C0105, R0028 ERROR→CORRECT; F0067 CORRECT→UNSUPPORTED(M8) |
   | INSERT + MERGE | 260C/0W/47E/1EE/0T (+3 UNSUPPORTED) | same three |

   C0117 and R0099 no longer move; ERROR→ERROR detail drifts remain on
   R0015 (vertex 82→84), R0016, R0026, R0081 (a different Stage-3/4/
   backend wall fires first in the same chained op), F0085 (Extrude
   19 vs 20 `NonPlanarFace`), and R0070 (both-gates only, holed-lateral
   CDT wording) — chained sample-sensitive walls, no category effect.

   **(d) F0067 ANCHORED (mechanism right, source SUPERSEDED — see the
   identity-fix census below: F0067 does not move once the zero-merge
   reorder is gone; the ~1e-16 perturbation was loop ROTATION, i.e.
   `d` derived from each face's first loop vertex, not the merges)**:
   every one of its 10 stacked unions is an M8
   Stage-0 cross-pair handled by the overlay; at Extrude 10 the overlay
   of A's face 328 (the gear boss's UNTOUCHED top cap) with B's bottom
   cap fails `RoundingCollapse { tri: [218, 227, 219] }` under the merge
   and succeeds gate-off, and the only visible input difference is that
   cap's re-fitted plane normal (gate-off `(-1.67e-15, -9.7e-16, 1)` vs
   merge `(-2.23e-15, -2.3e-16, 1)` — kernel-v2 Newell + plane/vertex
   canonicalization propagate the merged intermediate's ~1e-16 vertex
   differences). A 1e-15 plane tilt flipping the overlay's f64
   re-tessellation is an M8 knife-edge (the r=0.2088 circle's chord
   vertices graze the r≈0.22 gear edges): the wall is MASKED gate-off by
   rounding luck, not minted by the merge. Recorded as M8 residue
   (`RoundingCollapse` robustness), decision at flip time per (d).
   (b)/(c) are the same class one layer down (ERROR→ERROR).

   **I5-2 gate-ON pin-suite census (2026-08-19, both gates ON):**
   kernel-v2 38/38 binaries and yang-rs 74/75 green; the two findings:
   - **LATENT FOUND + FIXED — the merge pass was not the identity on a
     zero-merge output.** `kv9_cyl_cyl_special::unequal_perpendicular_
     walls_on_selfx_gate` (the pinned N6 STOP on the degree-4 unequal
     cyl×cyl union) flipped to a completion with `[s6-merge] runs=0
     elided=0`: the rebuild re-indexed every edge in loop-traversal order
     and re-rotated every loop to the partition offset, and kernel-v2's
     loop-rotation-sensitive patch tessellation (KV7-F1) moved the
     render-resolution self-intersection verdict — a sampling artifact,
     not a fix. Now verbatim edges keep their original indices (clone +
     appended pieces + order-preserving compaction) and rebuilt loops
     rotate back to the entry covering original position 0; pin
     `zero_merge_pass_is_identity`. Corollary for the render gate: that
     M5 fixture's 79 penetrations are themselves rotation-sensitive at
     render resolution — the N6 verdict on degree-4 patches measures the
     patch tessellation as much as the B-Rep (recorded, not acted on).
   - **yr9 `t1_cap_rings_carry_exact_ssi_circles` §7.3** asserts each
     conic edge's chord midpoint lies within d_ε of the curve — the
     per-segment shape by construction; a merged ~1.8-rad arc's chord
     midpoint legitimately sits 0.073 away. Flip-time restatement: the
     midpoint bound is the arc sagitta `r(1−cos(sweep/2))` + d_ε, or
     verify the retagged `sources` against the curve. Not a defect.

   **I5-2 census AFTER the identity fix (2026-08-19), full corpus, 300s
   budget:**

   | config | score | deltas vs canonical 259C/0W/49E/0T |
   |---|---|---|
   | `YANG_434_MERGE` only | 259C/0W/48E/1EE/**1T** | F0085 ERROR→TIMEOUT only; ZERO other category or detail deltas |
   | INSERT + MERGE | 259C/0W/48E/1EE/**1T** | same + R0070 holed-lateral CDT wording (the recorded I5-1 renumber drift) |

   **F0085 is a GENUINE CONVERSION**: single-case merge-only at a 1500s
   budget → SUPPORTED_CORRECT in 296s (the honest `NonPlanarFace` STOP at
   Extrude 20 no longer fires; all 20 unions complete and the mesh +
   composition oracles pass) — it merely straddles the 300s CPU budget
   (gate-off it STOPPED at 149s, before the oracle phases). Every
   "chained re-entry" delta of the earlier censuses (C0105/R0028 gains,
   F0067 loss, R0015/R0016/R0026/R0081/F0085 drifts) was the zero-merge
   edge-reorder / loop-rotation artifact, not the merges. Adjudication
   list: (a) fixed, (b) F0085 = conversion, (c)/(d) dissolved, (e) done.
   The flip precondition (category-identical) is met with margin: one
   honest conversion, zero losses, zero drifts. **Budget note for the
   flip: F0085 CORRECT ≈ 296s CPU ⇒ the assay budget floor moves to
   ≥360s (docs/TESTING.md, CLAUDE.md quick form).**

   **I5-2 FLIPPED 2026-08-19 — BOTH gates ALWAYS-ON** (`YANG_434_INSERT` /
   `YANG_434_MERGE` = `0|off` remain as dev A/B knobs; the s434
   instruments' off legs drive them). Flip bar: gate-off byte-identical
   (recover.rs typed-rim canonicalization), gate-ON pin suites green
   (kernel-v2 38/38, yang-rs 75/75 with the yr9 §7.3 restatement,
   test-harness 59/59), gate-ON corpus category-identical except one
   honest conversion. **NEW CANONICAL (default gates, 360s budget):
   260C/0W/48E/1EE/0T (+2 UNSUPPORTED coplanar, +1 curved-profile) —
   F0085 ERROR→CORRECT (301.8s), R0070 face-index renumber only
   (156→134), the trio at 0.9/1.8/2.8s.** Assay budget floor moves to
   ≥360s (F0085 CORRECT ≈ 302s; F0065 ≈ 164s).

   The remaining density/shape tail after this flip is recorded, not
   open: `Parabola`/`Hyperbola`/`SurfacePair`/`LineSegment` runs stay
   per-segment (I5-1b scope); the render gate on degree-4 patches is
   loop-rotation-sensitive (KV7-F1, measured on the M5 fixture); and the
   yang closed-run split points are sweep-fraction vertices (kernel-v2's
   pass-2 canonicalization is geometry-intrinsic, so this is a shape
   choice, not a contract).

### I6 — Fig-11(b)→(c) fold merge: the boundary vertex the relocation OVERRAN

**Status: LANDED and FLIPPED ALWAYS-ON, 2026-08-19d**
(`YANG_441_FOLD_MERGE=0|off` is the dev A/B off-knob). Flip bar: gate-off
byte-identical by construction (every line of the pass is inside the predicate),
the rewrite tier green with the pass on (1173s), and a gate-ON corpus of
265C/0W/43E/1EE/0T — two honest conversions and zero other deltas.

**Anchor (census first).** `YANG_S6_LOOP_SIMPLICITY` + `YANG_S5_FOLD_PROBE` over
the nine `ring rejected by CDT` cases (F0045, R0011, R0025, R0044, R0053, R0074,
R0085, R0090, R0095): **every** non-simple output loop the scan can measure is
`class=MINTED_BY_S4` with `cross_pre=0`, and `cross_inherited=0` across the whole
family (per-case minted counts 1/6/3/6/–/2/40/1/5; R0053 and R0044's third op
fail on CURVED patches the planar scan does not cover). The family's loops were
simple before Stage 4 and are not after it.

**Mechanism, per vertex (F0045 = the clean witness).** F0045 is two parallel
cylinder bosses. On the bottom cap of cylinder B (z = 0.198702) the kept face is
`disk_B − disk_A`, bounded by an arc of circle A (the exact plane∩cylinder SSI,
emitted analytically) and a run of circle B's own rim (emitted as mesh chords).
The rim's Stage-1 grid is 360/13 = 27.69° and the exact junction falls at
−9.30°, between grid vertices −6.92° and −34.62°. Because the MESH of cylinder A
is an inscribed polygon, it is *smaller* than the true circle, so the arrangement
put the mesh crossing at ≈ −4.24° and the rim vertex at −6.92° classified OUT of
A — correctly, for the meshes. Stage 4 then relocated the junction onto the exact
−9.30°, a 2.382e-2 move across a 1.283e-2 spacing: it stepped **past** its own
rim neighbour. Turn at that neighbour: `27.69° → 167.34°` (`YANG_S5_FOLD` k=6,
`moved=(MOVED(2.382e-2),still,still)`, apex residual 0 on both its surfaces).
The loop folds; Stage 6 emits it; the render CDT rejects it.

**This is Fig 11 verbatim** (`refs/text/yang2025_hybrid_boolean.txt:558-563`):
q is "an intersection point on the boundary curve"; the paper locates the
constrained edge containing q, splits it, and — (b)→(c) — **merges the
too-close split endpoint p into q**. Our pipeline arrives at the same
configuration from the other side (the vertex exists first and is then moved),
so the operation to apply is the same one.

**Selector — `stage4_fold_risk::fold_merge_sites`, threshold-free.** A boundary
corner `(a, b, c)` is a site iff (1) `b` did not move across Stage 4, (2) `b`
sat inside `chord(a, c)` before and lies outside it after — `chord_order_
inversions`' certificate, i.e. exactly the `MINTED_BY_S4` verdict — and (3) the
END it overran (`a` if `t < 0`, else `c`) DID move. The sign of `t` PICKS the
survivor, so there is no distance tie-break; a victim two survivors claim is
dropped as ambiguous. Victims and survivors are disjoint by (1)∧(3), so a batch
can never chain substitutions.

The oracle for "moved" is `S4_PRE_POS`, not `relocations` — measured 2026-08-19:
on R0074/R0085/R0095/R0025 the `relocations` vector is EMPTY while 59–83 vertices
per loop moved (it carries conic `(v, t)` retags only; the implicit-pair and
junction arms do not push to it). Keying condition 3 on it rejected every
inversion in the family. `s4_pre_pos_enabled()` therefore gains a third, and
first non-diagnostic, consumer.

**Repair — `stage4_construct::rebuild_merge_fan`, a LOCAL re-triangulation.**
Not `collapse_vertex`: the 2026-08-05 trial measured that negative and named why
(a bare index rewrite of a real-length edge leaves the surrounding fan
inconsistent; F0067's wall merely moved to a non-2-manifold STOP). Not
`rebuild_patch_planar` either — measured 2026-08-19, whole-patch rebuild imposes
two requirements this merge cannot meet:

| holder | decline | why |
|---|---|---|
| F0045 patch 5, R0090 patch 5 | `ThetaUnwrap` | the merge is on the rim of a lateral that ENCIRCLES the axis, which `unwrap_theta` refuses by contract |
| R0074 patch 0, R0085 patches 0/264 | `Cdt TriangulationFailed` | the patch carries SEVERAL folds, so its full cycle still self-crosses; no single merge can make it simple |

The fan has neither problem. `rebuild_merge_fan` discards exactly the triangles
incident to the victim, chains their opposite directed edges into the victim's
link `l_0 … l_k`, and re-triangulates that polygon (closed by `l_k → l_0`, the
new boundary edge the merge creates) in the patch's chart, θ-unwrapped against
the victim's OWN branch — local, so no global span exists to fall outside of.
Orientation is matched by measurement against the fan it replaces, as the
whole-patch rebuild does. Every patch holding the victim in a TRIANGLE rebuilds
its fan (all-holders-or-none), so nothing is ever re-pointed without being
re-triangulated. One site per pass: two fans can share a triangle, and
`apply_rebuild_batch` refuses an overlapping batch.

Loud refusals, all persistent (a blocked victim is never re-proposed):
`FanNotSimple` (pinched vertex — the link chains into more than one run),
`FanSurvivorNotAdjacent`, `Cdt` (the fan polygon itself self-crosses), an
unchartable holder (cone/torus — the I2a scope), and the write-back's own
`StalePlan`/`OverlappingBatch`.

**Measured, gate ON, over the family:**

| case | sites | outcome |
|---|---|---|
| **F0045** | 1 (`v71→v68`, `chord_t = −0.0920`) | **ERROR → SUPPORTED_CORRECT** |
| **R0090** | 1 (`v41→v28`, `chord_t = +1.0289`) | **ERROR → SUPPORTED_CORRECT** |
| R0011 / R0074 / R0085 | 1 each | `FanNotSimple` (R0085 op2 `Cdt`) — pinched-vertex fans |
| R0044 | 13 | unchartable (cone/torus) holders |
| R0025 / R0053 / R0095 | 0 | every inversion is `apex_moved` |

Both `chord_t` values were derived independently by hand from the rejected
render ring before the selector existed, and match.

**Gate-ON full corpus (360s budget): 265C / 0W / 43E / 1EE / 0T — exactly the
two conversions, ZERO other category deltas and ZERO detail deltas** against the
263C canonical. No WRONG, no TIMEOUT, no regression.

**Recorded scope notes (measured, not open):**
* The pre-position snapshot is taken only when Stage 4 has a conic
  (`s4_probe && has_conic`, `stage5_topology.rs`), so a boolean with no analytic
  curve at all has no map and the pass is inert there by construction.
* One merge per pass: two fans can share a triangle and `apply_rebuild_batch`
  refuses an overlapping batch, so sites are sequenced rather than batched. Each
  applied pass re-runs `compute_phase_a`; `MAX_PASSES` is 32 as a runaway guard.
* The plan is applied with an EMPTY substitution map, after verifying no triangle
  outside the rebuilt fans still holds the victim. A merge is therefore carried
  entirely by re-triangulated fans — never by an index relabel, which is exactly
  what made the 2026-08-05 trial unsound.

**The residue is not one defect but TWO, and the census separates them.** Every
rejected inversion has an apex that genuinely MOVED (`apex_minted = 0` in every
case — a Stage-4-MINTED apex, e.g. an appended §4.3.4 sample, is a different
population and there are none). Splitting those by whether BOTH of the apex's
incident cycle edges are intersection-curve edges:

| case (per op) | inversions | apex_moved | of which ON-CURVE | Fig-11 sites |
|---|---|---|---|---|
| R0044 | 214 / 115 | 188 / 109 | **163 / 96** | 13 / 3 |
| R0053 | 83 / 12 | 83 / 12 | **62 / 12** | 0 / 0 |
| R0095 | 20 | 20 | **13** | 0 |
| R0011 | 7 / 3 | 4 / 2 | **0 / 0** | 0 / 1 |
| R0025 | 4 / 4 | 4 / 4 | **0 / 0** | 0 / 0 |
| R0074 | 37 | 36 | **0** | 1 |
| R0085 | 127 / 50 | 125 / 49 | **0 / 0** | 1 / 1 |

* **ON-CURVE (R0044, R0053, R0095):** two vertices of the SAME intersection
  chain crossed each other. That is chain ORDER, owned by §4.3.4's
  `ReorderConic` action (I2b) — a merge would discard an analytic certificate.
  Worth asking why the existing reorder does not already straighten them; the
  curves here are `Hyperbola`/`SurfacePair`, which the I5-1b record already
  lists as staying per-segment.
* **OFF-CURVE (R0011, R0025, R0074, R0085 — 100 % of their inversions):** a
  RELOCATED vertex crossed a neighbour on a PLAIN boundary. It is not Fig-11
  (the apex holds the analytic certificate, so it must not be merged away) and
  not `ReorderConic` (there is no chain to reorder). **This class has no owner
  yet** and is the honest next question.

Recorded, not folded into this increment.

### I7 — the OFF-CURVE class: anchored, split, and assigned (2026-08-19e)

**Status: MEASURED, not built.** I6 left "a relocated vertex crossed a neighbour
on a PLAIN boundary" as the class with no owner. It is now anchored, and it is
not one class.

**First, two false starts, both retracted by measurement** (recorded so neither
is re-derived):

1. *"The surface-pair arm is missing the displacement gate its siblings have."*
   The `(2s)` `relocate_onto_implicit_pair` call site indeed accepts Newton's
   result ungated — but a probe there fires **0 times** on all four cases. The
   arm that actually moves these vertices is the `(2t)` KV6d Tier-B torus block,
   and it *does* gate: `rho > tangent_plane_corridor(d_eps, sinθ)` → STOP.
2. *"Then the gate is ballooning at near-tangency."* Measured
   (`YANG_TORUS_PROBE`): sinθ is 0.90–1.00 (transversal) and every accepted move
   is well inside its gate — worst ratios **0.69 / 0.32 / 0.23** on
   R0074 / R0085 / R0025. The gate is doing its job.

**The real anchor.** The corridor bounds OFF-CURVE error against the global
Stage-1 chord budget `d_eps`; it says nothing about LOCAL order. At these
vertices `d_eps` is 27–1000× the local segment length, so a move that is a small
fraction of the off-curve budget is still many local edges long. The relocation
is CORRECT — it lands on the exact curve, within budget — and the mesh around it
is simply not updated to accommodate it. Same root as I6; different local
configuration.

**New instrument.** `[s6-simplicity]` now reports the crossing's own VERTICES,
each tagged with its Stage-4 status (`at=seg23:v673(still)->v675(still) X
seg27:v34(2.455e2)->v36(2.101e2)`). `first_cross=(i, j)` named where a loop
crosses but not what is at the crossing, and every repair acts on vertices.
`YANG_441_FOLD_CENSUS` adds one line per off-curve corner (gap, both adjacent
edge lengths, both displacements, per-edge curve incidence).

**The split — displacement measured against the corner's OWN shorter edge:**

| case | corners | median disp/local-edge | max | >2× | >10× |
|---|---|---|---|---|---|
| R0011 | 6 | **1.48** | 3.01 | 2 | 0 |
| R0025 | 8 | **3.13** | 8.73 | 4 | 0 |
| R0074 | 36 | **2.26** | 101.43 | 19 | 5 |
| R0085 | 174 | **6.07** | 1737.00 | 144 | 74 |

F0045 — the I6 witness that CONVERTED — sits at **1.86**: it stepped over exactly
one neighbour, which is what a local merge can absorb. So:

* **LOCAL (R0011 at 1.48 median, most of R0074):** the apex overran one or two
  neighbours. A merge can absorb this; the only thing I6 lacks is a survivor rule
  for the case where BOTH vertices moved.
* **GROSS (R0085 at 6.07 median, 42 % beyond 10×; R0074's tail):** the vertex
  travels tens to thousands of local edges. No mesh update absorbs that — the
  discretization was never fine enough for the optimization to be a local
  correction, which is **§4.5.2 local refinement**'s own trigger (roadmap item 4,
  `refs/text/yang2025_hybrid_boolean.txt:659+`). This half is NOT unowned; it
  belongs to an already-planned milestone.

**The next increment, precisely.** Extend the Fig-11 merge to a BOTH-MOVED
corner, choosing the survivor by **surface-incidence richness** — the KV15b I1b
rule already generalized on 2026-08-19c. The rule also supplies its own negative
case, tolerance-free:

* different richness ⇒ two authorities for ONE corner ⇒ merge into the richer
  (R0011's crossing witness: `v34` carries `{A:Cylinder, B:Plane, B:Plane}` and
  `v36` carries `{A:Cylinder, B:Plane}` — a junction and a curve point);
* equal richness ⇒ two distinct samples of ONE curve ⇒ merging would coarsen the
  curve; the defect is their ORDER, which is `ReorderConic`'s.

No displacement band is needed: the fan rebuild already refuses loudly
(`FanNotSimple`, `Cdt`) wherever the configuration is not locally repairable, so
the GROSS half declines itself rather than being excluded by a threshold.

### I8 — the fan of one, and what a merge may IDENTIFY (2026-08-20)

**Status: LANDED GATED** (`YANG_441_FAN_OF_ONE`, `YANG_441_MERGE_CARRIER`; both
default OFF). Two findings, the second of which RETRACTS I7's recorded next
increment.

#### (a) The `FanNotSimple` decline was never a pinch

I6 recorded "`FanNotSimple` (pinched victim — R0011/R0074/R0085)" as the
remaining refusal on the sites that ARE Fig-11's. That was an inference from the
variant's name; the variant carried no reason. It now does
(`ConstructError::FanNotSimple { reason: FanReason }` — `Degenerate` / `Pinch` /
`Split { runs, with_survivor }` / `Short { fan, link }`), and the measurement is
unambiguous: **every one of the three is `Short { fan: 1, link: 2 }`.** Not a
pinch, not a split — the victim carries a SINGLE triangle in the declining
patch, so its link is one edge and there is no polygon to re-triangulate.

That is not a refusal, it is the ANSWER. With `survivor ∈ {x, y}` the merge
rewrites `(victim, x, y)` as `(survivor, x, y)`, which is degenerate, so the
triangle is simply DROPPED — an empty [`rebuild_merge_fan`] result. The
patch's boundary `… y → victim → x …` becomes `… y → x …`, exactly the edge
every other holder's re-CDT produces, so the batch stays conformal. Measured:
the fan-of-one fires on R0085 (patch 1, dropping 1 of that patch's 40 triangles
— no patch is emptied).

#### (b) What a merge may IDENTIFY: carrier CONTAINMENT

With the fan of one repaired, R0011 and R0074 declined one layer deeper —
`FanSurvivorNotAdjacent`, because a tiny 2-triangle holder held the victim in a
triangle the survivor was not part of. Probing that holder (new
`YANG_441_MERGE_SITE_PROBE`: per-holder attribution, fan, and each endpoint's
distance to the holder's own surface) showed the refusal was RIGHT and its
stated reason IRRELEVANT. The endpoints' carried-surface sets:

| case | verdict | victim carries | survivor carries | victim off survivor's extra | survivor off victim's extra |
|---|---|---|---|---|---|
| F0045 | APPLIED | `{B:0, B:2}` | `{A:2, B:0, B:2}` | — (⊆) | — |
| R0090 | APPLIED | 2 surfaces | 3 surfaces | — (⊆) | — |
| R0011 | blocked | `{B:1, B:180, B:181}` | `{A:2, B:1, B:181}` | **3.425** off `A:2` | **0.424** off `B:180` |
| R0074 | blocked | `{A:0, A:162, A:163}` | `{A:0, A:162, B:2}` | 1.98e-4 off `B:2` | 2.59e-5 off `A:163` |

The two merges that CONVERTED have `carried(victim) ⊆ carried(survivor)`: the
victim is a plain sample on a model edge and the survivor is the exact junction
ON that same edge — Fig-11's p and q verbatim. The blocked ones have sets of
EQUAL SIZE that DIFFER: a model CORNER (three faces of one input) and a
curve∩edge junction, 5–7 local units apart. **A count-only richness test calls
those a tie; containment names them.** So the merge's precondition is

> a Fig-11 merge is legitimate iff `carried(victim) ⊆ carried(survivor)`,

certified on each side at `junction_certificate_band` — tolerance-free in the
sense that matters: it asks "does the survivor lie on this surface at junction
precision?", never "is it close enough?". Landed as `carrier_lost_by_merge`,
which returns the first lost surface so the refusal NAMES it. Measured: fires on
7 sites over R0011 (1), R0044 (3, cone carriers), R0074 (1), R0085 (2), and on
ZERO sites of F0045/R0090 — both still apply their merge and stay
SUPPORTED_CORRECT.

This closes a latent silent-wrong. Today those four sites are refused only
because a small holder happens not to contain the survivor; had every holder
contained it, the merge would have applied and evicted a model corner off one of
its own faces (R0011: 0.424 off `B:180`) while discarding a certified junction
(3.425 off `A:2`). The eviction KV15b I1b forbids for the sub-resolution
collapse is the same eviction, and the Fig-11 merge had no guard against it.

#### (c) The residue, REASSIGNED — and I7's next increment retracted

I7 named the next increment as "extend the Fig-11 merge to a BOTH-MOVED corner,
survivor by surface-incidence richness", targeting R0011 as LOCAL (displacement
1.48× the local edge). **Measured, R0011's site is not a both-moved corner at
all** — its overrun end never moved (`disp_over = 0`), and its one site is the
still/moved shape I6 already selects. The both-moved arm is real for R0025
(0 sites today, all four inversions both-moved) and adds sites on R0074/R0085,
but it is NOT what R0011 needs.

What the four blocked sites are, measured (`YANG_441_MERGE_SITE_PROBE`'s travel
arm): **the victim lies EXACTLY on the survivor's travel segment**, strictly
between its pre and post positions —

| case | travel | victim t | victim off travel |
|---|---|---|---|
| R0011 | 2.221e1 | 0.668 | 6.4e-13 |
| R0074 | 7.404e-4 | 0.296 | 1.4e-17 |
| R0085 (a) | 3.989e-2 | 0.376 | 0.0 |
| R0085 (b) | 5.777e-3 | 0.043 | 6.2e-17 |

— against 5.0 % / 6.6 % of travel for the two that converted (R0090 / F0045),
which are genuinely OFF the line. So the relocation slid the vertex along a
STRAIGHT CARRIER (the model edge shared by the two surfaces it stayed exactly
on) and overshot a vertex sitting on that carrier — the carrier's own ENDPOINT,
the model corner where a third face joins. R0011 makes the mechanism plain: the
vertex started 14.85 INSIDE the edge, travelled 22.21, and ended 7.37 BEYOND the
corner, 0.424 off the third face — the 3.3° angle between the two walls, over
7.37 of travel.

**The relocated position is outside its carrier's DOMAIN.** The exact
intersection of `A` with the LINE of `B`'s edge exists there; the edge does not.
No mesh update can absorb that, because the defect is not the mesh — the mesh
arrangement's local topology ("A crosses this edge") disagrees with the exact
geometry ("A crosses past its end"). That is §4.5.2 local refinement's own
trigger (roadmap item 4), the same owner I7 assigned the GROSS half to, reached
by a different and much sharper certificate than a displacement ratio. The
honest intermediate step, not yet built, is a relocation-domain STOP at the
`(2s)`/`(2t)` arms: a relocation whose travel segment CONTAINS another vertex of
its own carrier has left the domain, and should refuse there rather than be
discovered three stages later as a folded loop.

### I9 — the RELOCATION-DOMAIN postcondition (2026-08-20)

**Status: LANDED, ARMED** (`YANG_S4_CARRIER_DOMAIN`: unset = on, `0|off` = the
dev knob, `census` = report-only). I8 named this class but let it surface three
stages downstream; I9 names it where it happens.

**The defect.** A Stage-4 relocation slides a vertex onto the exact analytic
solution its arm converged to. That solution is computed against SURFACES, which
are unbounded; the vertex lives on a bounded FACE. When the exact solution lies
beyond the face, the arm still converges — and the vertex slides straight past
the carrier's own endpoint, the model corner where a third face joins. Stage 6
then emits a folded loop and the render CDT rejects it with a message about a
ring, naming neither the stage nor the defect.

**The certificate, two legs — and it needs both.**

1. *Crossed.* The still neighbour `q` lies ON the traveller's `pre → post`
   segment, strictly inside it, at the project's shared 1e-9 relative
   collinearity identity. `on_segment_interior` already had exactly this test
   for vertex triples; it now delegates to a position-based
   `point_on_segment_interior` so the two gates share one metric instead of a
   second collinearity band drifting away from the first. Measured separation:
   6.4e-13 → 0.0 on the four ring-reject sites, against 5.0 %–6.6 % of travel
   for the two Fig-11 merges that are LEGITIMATE — seven orders, so I9 does not
   preempt I6.
2. *A domain ENDPOINT, not a sample.* `q` carries a surface the relocated
   position is OFF (§4-I8's containment rule read in the other direction), so a
   third face joins at `q` and the carrier STOPS there. **Leg 1 alone
   over-fires**: a `q` lying only on surfaces the traveller also lies on is a
   plain sample of the SAME carrier, which Yang's own remedy owns ("we remove a
   mesh vertex if it is too close to the intersection curve", §4.4.1) — a STOP
   there would preempt the near-curve removal. Measured: leg 2 is what exempts
   F0064 (2 samples, an UNSUPPORTED coplanar case that would otherwise have been
   pushed to ERROR) and R0051 (1 sample; its `SelfIntersectingBooleanOutput` has
   a different cause, which leg 1 alone would have wrongly claimed).

**A POSTCONDITION, not a per-arm check.** Relocation happens at thirteen
`mesh.verts[v] = proj` sites in `stage4_relocate_and_correct` plus
`apply_boundary_relocations` further down, and every repair that might dissolve
the configuration (the P3b beyond-corner trim, the collapsed-fan
re-triangulation, the reversal sweep, the §4.4.1(b) sub-feature merge) runs
before the stage ends. One snapshot at entry and one check at the end covers
every arm — present and future — and fires only on what SURVIVES all the
repairs.

**Full-corpus census before arming** (`YANG_S4_CARRIER_DOMAIN=census`, all 312
cases; the corpus runner nulls child stderr, so this was run case-by-case):

| case | verdict today | leg-1 fires | leg-2 STOPs |
|---|---|---|---|
| R0011 | ERROR | 4 | **4** |
| R0085 | ERROR | 11 | **11** |
| R0044 | ERROR | 7 | **7** |
| R0074 | ERROR | 1 | **1** |
| R0004 | ERROR | 1 | **1** |
| R0051 | ERROR | 1 | 0 — sample |
| F0064 | UNSUPPORTED(coplanar) | 2 | 0 — sample |
| every other case | — | 0 | 0 |

**Not one firing case is SUPPORTED_CORRECT**, so arming cannot cost a correct
case. Armed full corpus: **265C/0W/43E/1EE/0T, unchanged**, with exactly FOUR
detail deltas — R0011, R0044, R0074, R0085 — each ERROR→ERROR, trading a
downstream `TessellationFailed { ring rejected by CDT }` for
`Stage-4 relocation region around vertex N is invalid:
RelocationCrossedCarrierVertex` at the stage that caused it.

R0004 is the fifth firing case and its detail did NOT change: `YANG_LRR_PROBE`
confirms the STOP fires there (once, v427), but its two reported engine errors
are byte-identical armed and unarmed, so the firing invocation is not the one the
case reports (a case's boolean runs more than once — the composition oracle
re-runs each op in isolation). No op flipped from success to failure, which is
the safety-relevant fact; which invocation swallows it is not yet pinned down and
is recorded as an open question rather than asserted.

**Related machinery, and why this is not it.** `trim_beyond_corner_phantoms`
(P3b inc-4b) already REPAIRS this class — "a Stage-4 relocation can land a
section-curve sample OUTSIDE the bounded owner face, past a Stage-1 minted
corner junction on the same curve" — by collapsing phantom→mint. Its
eligibility requires the corner to be a Stage-1 MINT, and (independently
derived) a patch-subset guard that is the face-level sibling of §4-I8's
surface-level containment. The I9 sites differ in exactly one respect: their
corner is an INHERITED input model corner, not a mint. Extending inc-4b's trim
to inherited corners is the natural next increment and is a REPAIR, not a STOP —
recorded here, not built.

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
