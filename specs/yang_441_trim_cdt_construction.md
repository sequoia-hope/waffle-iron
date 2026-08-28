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
| R0074 | ERROR | 1 | **1** | (armed detail corrected — §4-I10 (e)) |
| R0004 | ERROR | 1 | **1** |
| R0051 | ERROR | 1 | 0 — sample |
| F0064 | UNSUPPORTED(coplanar) | 2 | 0 — sample |
| every other case | — | 0 | 0 |

**Not one firing case is SUPPORTED_CORRECT**, so arming cannot cost a correct
case. Armed full corpus: **265C/0W/43E/1EE/0T, unchanged**, with exactly FOUR
detail deltas — R0011, R0044, R0074, R0085 — each ERROR→ERROR, trading a
downstream `TessellationFailed { ring rejected by CDT }` for
`Stage-4 relocation region around vertex N is invalid:
RelocationCrossedCarrierVertex` at the stage that caused it. **Three of the
four.** R0074's armed detail is a DIFFERENT pre-existing Stage-4 STOP —
corrected and explained in §4-I10 (e).

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

> **REFUTED 2026-08-20 (third session) — see §4-I10.** The extension does not
> fire. The blocker is not mint-ness; it is inc-4b's own patch-subset guard,
> which refuses **24 of 24** measured sites. Read §4-I10 before attempting it.

### I10 — the trim extension REFUTED, and the class anchored to §4.5's own trigger (2026-08-20)

**Status: MEASUREMENT.** No behaviour change; the census gained the face-level
and surface-level columns the decision needed, and they answered it.

#### (a) The recorded next increment does not fire — 24/24

`YANG_S4_CARRIER_DOMAIN=census` now also reports, per site, the attributed patch
set of the traveller `v` and of the crossed corner `q`, and whether inc-4b's
eligibility test `patches(v) ⊆ patches(q)` holds. Over every firing site of every
firing case (R0011 4, R0044 7, R0074 1, R0085 11, R0004 1 = 24):

| | traveller `v` | crossed corner `q` |
|---|---|---|
| R0074 v129 / v127 | `{A:0, A:162, B:2}` | `{A:0, A:162, A:163}` |
| R0011 v78 / v834 | `{A:2, B:1, B:181}` | `{B:1, B:180, B:181}` |
| R0044 v8 / v991 | `{A:2, B:1, B:154}` | `{B:1, B:154, B:155}` |
| R0085 v4165 / v388 | `{A:227, A:228, B:2}` | `{A:0, A:227, A:228}` |
| R0004 v427 / v571 | `{A:5, B:0, B:289}` | `{B:0, B:288, B:289}` |

**`subset = false` on all 24.** The shape is the same every time and it is
structural, not incidental:

- `v` is a **cross-operand junction** — two ADJACENT faces of the near operand
  (its carrier model edge) plus ONE face of the FAR operand (the surface it is
  being relocated onto);
- `q` is a **pure single-operand model corner** — three faces of the near
  operand, and none of the far one.

So `v` always carries the far-operand face and `q` never does, and the subset
test fails on exactly that element. It is *right* to fail: collapsing `v` onto
`q` would drag the far operand's intersection strip onto a point that is not on
the far operand's surface at all — measured `d_q` below is how far off.

**The lesson is the one §4-I8 already taught, one level up.** "Minted vs
inherited" was never the operative distinction; it was a PROXY for *does the
corner carry the traveller's far-operand face?* A Stage-1 mint is a
cross-operand junction by construction, so it carries both operands and the
subset holds. An inherited model corner is single-operand by construction, so it
cannot. Extending the trim by relaxing mint-ness relaxes the proxy and leaves the
real guard — correctly — refusing.

#### (b) What the sites actually are: §4.5's own stated trigger, measured

The census also reports, per incident face of the traveller, `|d|` at `pre`, at
`post`, and at the corner `q`:

| site | far face | `d_pre` | `d_q` | `pre→q` | `overrun` |
|---|---|---|---|---|---|
| R0074 v129 | B:2 | 2.806e-4 | 1.979e-4 | 2.192e-4 | 5.213e-4 |
| R0011 v78 | A:2 | 1.031e1 | 3.425e0 | 1.484e1 | 7.368e0 |
| R0011 v27 | A:2 | 5.221e1 | 4.116e1 | 7.053e1 | 2.576e2 |
| R0011 v37 | A:2 | 8.468e1 | 7.296e1 | 3.407e1 | 2.114e2 |
| R0011 v42 | A:2 | 1.075e2 | 7.421e1 | 7.775e1 | 1.675e2 |

Every traveller is EXACTLY on its two near-operand surfaces at both `pre` and
`post` (measured 0.0 / ~1e-13, against bands of ~1e-11): it lives on its carrier
model edge and never leaves it. It is the FAR surface it is chasing, and `d_pre`
is the distance to it at the mesh crossing.

**The relocation is arithmetically correct.** Extrapolate the approach linearly —
rate `= (d_pre − d_q)/|pre→q|`, remaining `= d_q/rate` — and it predicts the
measured overrun to **0.3 %–3.6 %** (0.56, 0.27, 1.96, 0.32, 3.55; the residual
is surface curvature). The arm is not diverging and is not on a wrong root. It is
solving the right equation and the answer is outside the domain.

**Which is the paper's stated trigger, verbatim.** §4.5
(`refs/text/yang2025_hybrid_boolean.txt:652-656`): *"we collect the point pairs
that cannot converge to a distance of 0 **within their domains**."* Along the
carrier edge from `pre` to its endpoint `q`, the distance to the far surface
falls monotonically from `d_pre` to `d_q` and **stops there**: the minimum
attainable distance inside the domain is `d_q > 0`. Distance 0 exists only past
the corner. That is non-convergence within the domain, measured, not inferred.

And §4.5.1 (`:672-690`) describes the defect in the same words the census
prints: *"Instead of taking a full step length that takes the point to a position
`p1` **outside** the surface `S2` where the point is initially located, we
truncate the step so that the point moves to `p` on the boundary curve `C_b`
between `S2` and the neighboring surface `S1`. In the next iteration, the
optimization step of `p` is computed using the parameterization of `S1`."*

#### (c) Consequence: `stage4_truncate`'s recorded scope gap is CLOSED

`crates/yang-rs/src/stage4_truncate.rs` implements §4.5.1's truncation MECHANISM
and says so honestly in its header: its trigger is loop-simplicity, while
*"§4.5.1's stated trigger is an erroneous region — points that 'cannot converge
to a distance of 0 within their domains' … **Our relocations converge exactly**
… this module implements §4.5.1's MECHANISM under a trigger the paper does not
state for it."*

That premise was true when written and is no longer. The I9 fire list is the
corpus's first measured population of §4.5.1's OWN stated trigger: relocations
that converge exactly *as equations* and provably do not converge *within their
domains*. The mechanism and its trigger can now be joined without borrowing.

#### (d) The next increment, and the measurement that selects it

The paper's ladder is explicit (`:740-744`): *"we only use the first strategy in
cases where the failure points are bounded by two successfully optimized points
**on the same surface**. For other cases, we apply the second strategy"*
(§4.5.2 local refinement). So the next step is neither "build §4.5.1" nor "build
§4.5.2" — it is to **measure the selection predicate** on the 24 sites: for each,
walk the intersection curve to the nearest successfully-optimized point on each
side and report whether both lie on a common surface. That answer, not a
preference, picks the strategy. Recorded here, not built.

Note the sub-shape this measurement must settle: the traveller does not step off
a SURFACE into its neighbour — it steps off a boundary CURVE (its carrier model
edge) past that curve's own endpoint, where a third face joins. §4.5.1's
"truncate to the boundary curve `C_b`, then re-parameterize on the neighbour
`S1`" has an obvious one-level-down analogue (truncate at the corner, continue on
the next edge), but the analogue is an inference and must not be built as though
the paper stated it — the §4.5.2 mislabel of 2026-08-04 and this section's own
(a) are both what borrowing across triggers costs.

**MEASURED, same session.** `strategy_selection_census` (`YANG_S45_SELECT`, in
the same census branch) implements the selector and takes the reading. For each
failing traveller it walks the intersection curve outward along every branch to
the nearest **successfully optimized** vertex — `converged(w) && !failed(w)`,
where `converged` means the position lies on a surface of EACH operand at the
shared certificate band, and `failed` is the §4-I9 fire list itself, which is
precisely "converged as an equation, but not within its domain". Where the curve
neighbourhood has degree > 2 the surface set is intersected over ALL bounds, so
the choice of which pair cannot decide the verdict.

The answer is unanimous over all 24 sites in all five cases:

| | measured |
|---|---|
| erroneous region size | **one point** — every bound is 1 hop away, on every branch |
| bounds converged | **all of them**, on every branch (2–6 per site) |
| surface common to ALL bounds | **exactly 1**, every site |
| traveller also on that surface | **yes**, every site |
| **verdict** | ~~FIRST STRATEGY (§4.5.1), 24/24~~ — **WRONG, corrected in (f)** |

And the common surface is the near-operand CARRIER face, which is the paper's
`S2` — "the surface where the point is initially located". R0011's four sites all
name the same `Cylinder{r = 6277.3}` = face `B:1`, the face the section curve runs
across; R0074's names the `Torus` its traveller sits on. The far surface the
traveller is chasing is NOT the common one.

> **The VERDICT above is wrong — see (f).** The bounding data in this table is
> sound and still stands; the verdict drawn from it is not. §4.5's selector has a
> second clause, stated one paragraph earlier than the one implemented here, and
> it EXCLUDES this class. This section is left as measured so the error and its
> cause stay legible.

#### (d2) The design this reading implies — RETRACTED, see (f)

> **Do not build this.** The reading it rests on is refuted in (f): the paper
> explicitly excludes this class from §4.5.1. Kept for the record because the
> primitive built under it is sound and still useful; its stated CUSTOMER is not.

§4.5.1, applied with `S2` = the measured common surface:

1. **Region.** The failing traveller alone (measured: the region is one point).
2. **Replace.** Remove it; insert the MIDPOINT of its two converged bounds — both
   on `S2`, so the midpoint is on/near `S2` by construction.
3. **Re-optimize with a truncated step.** Optimize the midpoint by §4.3.2, but
   *"instead of taking a full step length that takes the point to a position `p1`
   outside the surface `S2` … truncate the step so that the point moves to `p` on
   the boundary curve `C_b`"*. `C_b` here is the carrier model edge, and its own
   endpoint is the corner `q` that §4-I9 reports.
4. **Continue on the neighbour.** *"In the next iteration, the optimization step
   of `p` is computed using the parameterization of `S1`"* — the face across
   `C_b`.

**What must be built, and what already exists.** The missing primitive is a
DOMAIN truncation — "how far along this step before the point leaves `S2`'s
bounded domain?" — the exact analogue of `stage4_truncate::max_simple_step`,
which answers the same shape of question for loop simplicity ("how far before the
loop stops being simple?") and is already exact, tested, and unwired. The domain
version should be built beside it, not inside it: same module, same vocabulary,
different predicate.

> **BUILT, same session — `stage4_truncate::max_in_domain_step`.** Pure,
> deterministic, unwired for behaviour, and exercised on live data.
>
> - Answers `FullStepInDomain` / `TruncateAtVertex { t, at }` / `Unmeasurable`.
>   The caller must land on the STORED POSITION of `at` (exact input
>   coordinates), never on `lerp(pre, post, t)` — `t` is a rounded projection,
>   returned for ordering and diagnostics.
> - Candidate selection stays with the CALLER, because what makes a still
>   neighbour a domain END rather than a sample Yang's near-curve removal owns is
>   §4-I9's leg-2 certificate. The primitive only decides which certified
>   candidate the step reaches first.
> - Acceptance reuses the SHARED relative collinearity identity
>   (`point_on_segment_interior`), the same gate §4-I9 fires on — so the repair
>   cannot fire where the STOP would not, nor decline where it would, and no new
>   band enters the system.
> - Ten unit tests, RED-VERIFIED by two mutations: reversing the ordering rule
>   fails `picks_the_first_boundary` + `ties_resolve_by_lowest_vertex_index`;
>   removing the collinearity gate fails
>   `full_step_when_no_candidate_lies_on_the_segment`.
> - **Measured on every live site** from the census (`YANG_S45_TRUNCATE`,
>   report-only): all 24 return a strictly interior truncation, `t` from 0.0399
>   (R0085 v4359) to 0.9203 (R0085 v4216). R0074's live `t = 0.29599` agrees with
>   the unit test's `0.296`, which is derived independently from the measured
>   overrun/travel ratio.
>
> What it does NOT do, and must not be read as doing: §4.5.1's continuation —
> re-parameterizing the landed point on the neighbouring surface `S1` and solving
> `q1`/`q2` on `C_b`. Until that exists the §4-I9 STOP remains the answer, and
> the primitive stays report-only.

**Fail-closed, and gated.** A truncation that cannot certify its landing point on
`C_b` must leave the §4-I9 STOP standing rather than accept a position it cannot
justify — the STOP is the safety net this repair is trying to earn its way past,
and a repair that half-fires is worse than one that declines. Land it gated
(`YANG_451_DOMAIN_TRUNCATE`), census first, flip on a zero-category-delta corpus.

**Still an inference, still flagged.** Step 3's `C_b` is a model EDGE and the
traveller rides it lengthwise, whereas §4.5.1's figure has the point crossing
`C_b` transversally off a surface. The measured facts — region on `S2`, bounds on
`S2`, traveller on `S2` — put the configuration inside §4.5.1's stated scope, and
that is what the borrow now rests on. It is no longer a borrow across TRIGGERS
(that gap closed in (c)); it is a one-level-down reading of the mechanism, and it
stays labelled as such until a case proves it.

#### (e) Record correction — §4-I9's R0074 row, and a second instance of its open question

Re-measuring the four armed detail deltas found three reproduce and one does not:

| case | armed reason (measured 2026-08-20, third session) | I9 recorded |
|---|---|---|
| R0011 | `RelocationCrossedCarrierVertex` | ✅ matches |
| R0044 | `RelocationCrossedCarrierVertex` | ✅ matches |
| R0085 | `RelocationCrossedCarrierVertex` (+ `not 2-manifold`, op 3) | ✅ matches |
| **R0074** | **`OffCurveBeyondChordBand` v91** | ❌ recorded as `RelocationCrossedCarrierVertex` |

`YANG_LRR_PROBE` explains it, and the explanation is the same open question §4-I9
recorded for R0004 — R0074 is its second instance, this time visible in the
detail:

```
ARMED : YANG_LRR_SITE …stage4_correct.rs:3942 reason=RelocationCrossedCarrierVertex v=129
        YANG_LRR_SITE …stage4_correct.rs:6770 reason=OffCurveBeyondChordBand      v=91
OFF   : YANG_LRR_SITE …stage4_correct.rs:6770 reason=OffCurveBeyondChordBand      v=91
```

The `OffCurveBeyondChordBand` STOP at v91 fires in **both** modes — it is
pre-existing and independent of I9. Arming adds the postcondition STOP at v129 in
a different invocation of the same op ("Revolve 2: Auto-union"), and the detail
the case reports switches from the downstream ring-reject to that pre-existing
STOP. So arming did not create the reported reason; it removed the invocation
whose ring-reject used to be reported, and the other STOP surfaced.

The counts are unaffected: still FOUR detail deltas, still ERROR→ERROR, still
zero category deltas, corpus still 265C/0W/43E/1EE/0T. What is corrected is which
reason R0074 reports.

Gate hygiene re-verified while measuring: `YANG_S4_CARRIER_DOMAIN=census` is
byte-identical to `=0` on R0074 (both `ring rejected by CDT`), so the census mode
is behaviour-free as designed, and the enriched columns added here keep it so —
every new line is inside the `census` branch.

**Open, and now twice-observed:** which invocation of a multi-invocation op the
assay reports is not pinned down. It cost one wrong row in a ledger. Worth an
instrument (an invocation counter in the STOP's diagnostic) before the next
armed-flip measurement, so a detail delta can be attributed to an invocation
rather than inferred.

#### (f) CORRECTION — §4.5.1 does NOT apply: the paper EXCLUDES this class (Fig-13)

**Status: MEASUREMENT + record correction.** Asked to build §4.5.1's
continuation, I re-read the section end to end first. The page that follows
Fig-12 states an exclusion that (d) never tested, and it names this class exactly.

**The clause** (`refs/text/yang2025_hybrid_boolean.txt:637-651`, the right-hand
column of p. 114:9):

> *"We note that the first strategy only applies to the **interior points** but
> not to the **boundary points that glide along the boundary curves**. As shown
> in Fig. 13 (a), `s` is a corner point where more than two surfaces meet, and
> the white dots are the target positions of the boundary intersection points.
> If the initial positions indicated by the mesh intersections are given as in
> Fig. 13 (b), the points may **glide toward `s`** under optimization. However,
> after reaching `s`, it is difficult to predict in which direction each vertex
> goes. … However, this may lead to **topology errors**, as illustrated in (c).
> Thus, we only use the first strategy in cases where the failure points are
> bounded by two successfully optimized points on the same surface. For other
> cases, we apply the second strategy as described below."*

So §4.5's selector has TWO clauses. (d) implemented the second ("bounded by two
successfully optimized points on the same surface") and never tested the first
("interior points, not boundary points gliding along boundary curves"). The first
is the one that decides this class.

**Measured, not assumed** (`YANG_S45_SELECT`, the `carrier:` line). A traveller
on TWO distinct surfaces of one operand at BOTH ends of its step is riding that
operand's boundary curve; a crossed vertex on THREE is Fig-13's corner `s`
("where more than two surfaces meet"). Over all 24 sites in all five cases the
signature is uniform and unanimous:

| | R0011 / R0004 / R0044 | R0074 / R0085 |
|---|---|---|
| traveller at `pre` | `(A0, B2)` | `(A2, B0)` |
| traveller at `post` | `(A1, B2)` | `(A2, B1)` |
| crossed `q` | `(A0, B3)` | `(A3, B0)` |
| glides on a boundary curve | **yes** | **yes** |
| `q` is a corner where >2 surfaces meet | **yes** | **yes** |
| **verdict** | **SECOND STRATEGY (§4.5.2)** | **SECOND STRATEGY (§4.5.2)** |

**24/24 EXCLUDED.** Not one site is an interior point. The far-operand count
rising 0 → 1 across the step is the same fact (b) measured as `d_pre > 0`,
`d_post = 0`, seen through a second instrument.

**Fig-13(c) is our defect, drawn.** "Crossing the corner point causes a
topological error" — that is §4-I9's `RelocationCrossedCarrierVertex`, and the
folded loop the render CDT rejects three stages later. The paper does not merely
decline to fix this with strategy 1; it draws the failure that strategy 1 would
cause.

**Consequences.**

1. **§4.5.1's continuation must not be built for this class**, and the
   §4-I9 STOP stays the answer until §4.5.2 exists. Building it would be
   inventing mechanism the paper assigns elsewhere — the exact failure
   `feedback`/`yang_read_paper_before_scoping` names, and the one (c) of this
   very section congratulated itself for closing.
2. **§4-I8 was RIGHT.** It assigned this class to §4.5.2 ("outside its DOMAIN ⇒
   §4.5.2, not a merge, not `ReorderConic`"). (d) overrode that on an incomplete
   predicate; the paper restores it. When a fresh measurement contradicts a
   previous session's paper-grounded assignment, the measurement is the thing to
   re-check first.
3. **`max_in_domain_step` survives, its stated customer does not.** The primitive
   measures where a step leaves its carrier's domain; that fact is what §4.5.2
   needs to DETECT and bound its erroneous region too. What must be struck is the
   claim that it is one step of §4.5.1's continuation for these sites. Its doc
   comment is corrected accordingly.
4. **The selector is now the paper's, both clauses**, and the exclusion is
   evaluated FIRST — so no future reading can reach the bounding test on a
   boundary-gliding point.

**The methodological miss, named.** (d) was careful about the right thing and
still got it wrong: the note in (d) even flagged that "the traveller does not step
off a SURFACE into its neighbour — it steps off a boundary CURVE … the analogue is
an inference and must not be built as though the paper stated it". That instinct
was correct and the selector's verdict overrode it, because the selector
implemented one sentence of the paper rather than the section. **A measurement is
only as sound as the predicate it encodes; encoding half a stated rule produces a
confident number and a wrong answer.** Read the section to its end before
implementing its test.

### I11 — does §4.5.1 have ANY customer in the corpus? YES: 5 vertices, 3 cases (2026-08-20)

**Status: MEASUREMENT.** No behaviour change; every added line is census-gated
and `YANG_S4_CARRIER_DOMAIN=census` is verified outcome-identical to `=0` on
R0074, R0011 and C0065.

§4-I10 (f) answered the strategy question for ONE population — the §4-I9 fire
list, 24/24 excluded from §4.5.1. That is not the same as "§4.5.1 has no
customer", and the epic needs the second answer before spending sessions on
either strategy. So the census was widened from the I9 list to **the paper's own
failure population**, over all 312 cases.

#### (a) The population is the paper's, and it has two halves

§4.5 (`refs/text/yang2025_hybrid_boolean.txt:652-656`): *"we collect the point
pairs that cannot converge to a distance of 0 **within their domains**"*. Both
halves are enumerated:

- **in-domain non-convergence** — the optimization ran on the point and its final
  position does not lie on a surface of EACH operand;
- **out-of-domain convergence** — it lies on both, but past its carrier's own
  endpoint: the §4-I9 fire list.

Each member is classified by the Fig-13 discriminator at its INITIAL location
(§4.5.1's own wording is "the surface `S2` where the point is **initially
located**"): one surface per operand ⇒ INTERIOR ⇒ §4.5.1's stated scope; two or
more of one operand ⇒ a BOUNDARY intersection point ⇒ excluded.

#### (b) The census had to be taken from BOTH exits, and that is where the answer was

A first pass measured only from `relocation_domain_postcondition`, at the END of
Stage 4, and found **interior = 0** — which would have said "no customer". It was
wrong, for a structural reason: **a run that STOPs never reaches the end**, and
the hardest cases all STOP. So the census is now taken on both exits of
`stage4_relocate_and_correct`, tagged `at=postcondition` / `at=stopped`.

Even that was not enough. From a STOP the population still measured empty,
because **the STOP'd vertex is never written**: the refusal happens where the
answer is rejected, so "Stage 4 moved it" — the proxy for "the optimization ran
on it" — is false for precisely the vertex that failed. It is classified
directly instead (`YANG_S45_POP STOP-VERTEX`).

The same argument as §4-I9's: one vantage point covering every exit, rather than
an edit per site that the next site will forget.

#### (c) The measurement, all 312 cases

Coverage first, because a zero is only as good as its denominator:

| | cases |
|---|---|
| all-planar — Stage 4 never runs (`if has_conic`) | 125 |
| curved | 187 |
| … reporting from the postcondition | 113 |
| … reporting from a STOP | 16 |
| … no conic output edge, so no §4.5 optimization at all | 65 |
| planar cases reporting (expected 0) | **0** |

And the population, validated against ground truth — the I9 half reproduces the
known fire list exactly (R0004 1, R0011 4, R0044 7, R0074 1, R0085 11 = 24):

| population | members | INTERIOR | BOUNDARY | unlocated |
|---|---|---|---|---|
| completed Stage 4 — in-domain non-convergence | 12 | 0 | 12 | 0 |
| completed Stage 4 — out-of-domain (§4-I9) | 24 | **0** | 24 | 0 |
| **Stage-4 STOP vertices** | 12 | **6** | 5 | 1 |

over 30 287 curve vertices, 10 194 of which Stage 4 moved.

#### (d) The answer: YES — and everything in the completing half is §4.5.2's

**Among relocations that COMPLETE, 36 of 36 failure members are boundary points.**
§4.5.1 has no customer there, which extends §4-I10 (f) from the I9 list to the
whole completing population.

**Among the 12 STOP vertices, 6 are interior points**, in 4 cases:

| case | vertex | STOP reason | carrier |
|---|---|---|---|
| C0065 | v3, v8 | `OffCurveBeyondChordBand` | `(A0, B1)` |
| R0003 | v4233, v10583 | `OffCurveBeyondChordBand` | `(A0, B1)` |
| R0028 | v64 | `OffCurveBeyondChordBand` | `(A0, B1)` |
| R0050 | v125 | `LocalRefinementRequired` | `(A1, B1)` |

**R0050 is not a customer** and is excluded on inspection: `(A1,B1)` means it lies
on one surface of EACH operand — it CONVERGED — and its STOP is an explicit
capability refusal ("a torus-edge endpoint that is also a conic endpoint mixes
the implicit-pair and closed-form relocations — out of v1 scope"). A scope wall
is not a convergence failure.

**The other five are Fig-12(a) drawn.** `(A0,B1)` says the vertex lies on ONE
surface of one operand and NONE of the other — it has not converged onto the far
surface at all, while sitting interior to its own. The paper's caption:
*"C is the correct B-Rep intersection curve passing through surfaces `S1` and
`S2`. The intersection of the meshes is shifted onto `S2`, completely bypassing
`S1`."* And C0065's STOP site is the owner-face hull check — the relocated
position fell outside the bounding hull of its own input face — which is
Fig-12(c): *"A full step takes `p0` to an out-of-boundary location `p1`."*

So **§4.5.1 is not dead code for us**: C0065, R0003 and R0028 (5 vertices) are
candidates.

#### (e) What is still untested, and it is the deciding clause

§4.5's selector has two clauses and this section tested only the FIRST (interior
vs boundary) — the same one-clause mistake §4-I10 (d) made in the other
direction, avoided here only because (f) had just named it. **The second clause —
"bounded by two successfully optimized points on the same surface" — is NOT
tested on these five**, because `strategy_selection_census` walks the curve from
a site list the STOP path does not produce.

So the honest state is: **three cases are §4.5.1 CANDIDATES, not confirmed
customers.** The next measurement is to run the bounding walk from the STOP
vertex on C0065, R0003 and R0028. Only if it holds are they §4.5.1's; if it does
not, they are §4.5.2's too and §4.5.2 absorbs the entire §4.5 budget.

**Census cost, recorded:** under `census` the extra output pushes R0038 from
ERROR to TIMEOUT (verified ERROR with the census off and at default). Census-only
overhead, not a behaviour change — but it means a census run's category spread is
not a corpus score.

### I12 — clause 2 from the STOP vantage: the five candidates SPLIT — R0003's two vertices are §4.5.1's first CONFIRMED customers; C0065 and R0028 fall to §4.5.2 (2026-08-22)

**Status: MEASUREMENT.** Census-gated only; the corpus paths are untouched (the
only default-path edits are a mechanical extraction of the postcondition's
live-adjacency build into `build_live_adjacency` and a bounds guard on the
STOP-VERTEX classification for sentinel `u32::MAX` STOPs, which previously
indexed out of range).

I11 left one deciding measurement: §4.5's SECOND clause — *"the failure points
are bounded by two successfully optimized points on the same surface"*
(`refs/text/yang2025_hybrid_boolean.txt:740-744`) — had never run on the five
interior STOP vertices, because `strategy_selection_census` walks from a site
list the STOP path does not produce.

#### (a) The instrument

The clause-2 walk (branch walk → distinct converged bounds → common-surface
intersection → traveller-on-common) is EXTRACTED from
`strategy_selection_census` into `selector_clause2_walk` and now runs from both
vantages:

- the end-of-stage selector calls it unchanged — R0074 re-run: postcondition
  output identical to the recorded I10 behaviour (v129 Fig-13 exclusion, mirror
  carrier `(A2,B0)→(A2,B1)`, `q=(A3,B0)`);
- the STOP exit (`stage4_relocate_and_correct`'s census branch) calls it on the
  STOP'd vertex, after the existing clause-1 classification, printing under
  `YANG_S45_SELECT … vantage=stopped` plus a COMBINED clause1+clause2 verdict.

"Successfully optimized" cannot be computed identically at the two vantages.
The postcondition subtracts its §4-I9 fire list; a STOP'd run HAS no fire list
(the postcondition never ran), so the walk re-takes I9's two-leg reading per
candidate bound — `vertex_crossed_domain_endpoint`: travelled across a STILL
neighbour ON its pre→post segment (leg 1) that carries a surface the final
position is OFF (leg 2). Bounds skipped for that reason are counted and
printed (`i9_style_crossers_skipped`).

Two instrument-honesty items, both closed in the same commit:

- **Distinct-bounds fix.** The pre-I12 walk deduped `(vertex, hops)` PAIRS, so
  two branches reaching the same converged vertex (a loop around the erroneous
  region) would have counted as two bounds. The paper's clause names two
  POINTS `v0` and `v1`; now deduped by vertex. No recorded measurement is
  affected — every §4-I10 site reported distinct bound ids.
- **The predicate's zero had to be validated** (I11's lesson: a zero from an
  instrument is a claim about its vantage). `vertex_crossed_domain_endpoint`
  read 0 on every walked vertex, so it is cross-checked in census mode against
  the postcondition's own inline two-leg detection at each of its fire sites
  (`YANG_S45_XCHECK`): R0074's v129 fires `true`. The walk's zeros are genuine
  absences, not a dead predicate.

#### (b) The measurement

All three cases run under `YANG_S4_CARRIER_DOMAIN=census`, twice (pre- and
post-extraction of the shared predicate), byte-identical walk output:

| case | vertex | clause 1 | clause-2 walk | verdict |
|---|---|---|---|---|
| R0003 | v4233 | INTERIOR `(A0,B1)` | degree 4: bounds v4167/v4169/v4183 ALL at 1 hop (one branch refuses at a curve branch point); common surfaces = 2 (cone + plane); traveller on 1 | **§4.5.1 CONFIRMED** |
| R0003 | v10583 | INTERIOR `(A0,B1)` | degree 2: bounds v10564/v10585 both at 1 hop; common = 2 (cone + plane); traveller on 1 | **§4.5.1 CONFIRMED** |
| C0065 | v8 | INTERIOR `(A0,B1)` | bound v3 at 1 hop; the other branch refuses (curve branches) → 1 distinct bound | §4.5.2 |
| C0065 | v3 | INTERIOR `(A0,B1)` | bound v2 at 1 hop; the OTHER THREE branches (v8/v67/v68) all refuse (curve branches) → 1 distinct bound | §4.5.2 |
| R0028 | v64 | INTERIOR `(A0,B1)` | both branches walk 64 hops without a converged bound → 0 bounds | §4.5.2 |

`i9_style_crossers_skipped = 0` at every site.

Readings:

- **R0003 v10583 is Fig-12 drawn to the letter**: an interior failure point
  whose neighbours on BOTH branches converged one hop away, the bounds sharing
  exactly one surface per operand (`Cone` + `Plane` — both bounds sit on the
  same intersection-curve pair), the traveller on exactly one of the two (its
  carrier `B`-surface; it is OFF the other operand's surface — *"shifted onto
  `S2`, completely bypassing `S1`"*). This is the configuration §4.5.1's
  midpoint-and-truncated-step repair is FOR, and `max_in_domain_step` (I10) is
  its step primitive.
- **R0003 v4233 confirms too, with a wrinkle the build must own**: it sits at
  a degree-4 curve junction; three branches bound immediately, the fourth
  refuses at a further branch point. The paper's erroneous region is a simple
  segment between `v0` and `v1`; a multi-branch region needs the repair to
  define WHICH points "between the bounds" are removed (per-branch pairs, or
  the whole junction neighbourhood). Recorded as a design obligation, not
  resolved here.
- **C0065's two STOP vertices are curve-ADJACENT** — v8's walk reaches v3 as a
  converged bound in v8's own (earlier) invocation; v3's walk passes through v8
  in its later one. One erroneous REGION seen from two invocations. v3 is a
  degree-4 curve junction and every refusing branch dies at a branch point: the
  region reaches corner territory, where Fig-13 warns and the walk declines to
  guess. The paper's own selector sentence — *"If such bound cannot be found …
  the second strategy"* — lands it in §4.5.2.
- **R0028's region has no converged bound within 64 hops either way** — same
  sentence, same owner: §4.5.2.

#### (c) The vantage caveat, stated

The walk runs on the mesh FROZEN at the STOP — mid-sweep: where §4.5's repair
would run in our architecture today, but NOT where the paper runs its selector
(after a completed optimization sweep, failures collected). A vertex reading
non-converged here might have converged later in the sweep, so the two
NON-confirmations are vantage-sensitive — a post-sweep reading could find
bounds for C0065/R0028. The two CONFIRMATIONS are robust in that direction (a
converged, still bound stays a bound). Once the §4.5 loop exists (d), the
selector runs at the paper's own vantage and the C0065/R0028 verdicts are
re-taken there for free.

#### (d) What this decides, and the build order

**§4.5.1 must be built — it has confirmed customers** (I11's question is
answered). Everything else measured to date is §4.5.2's: the completing
population (36/36 boundary), the I9 fire list (24/24 Fig-13-excluded), and — at
this vantage — C0065 and R0028.

The faithful build, in increments:

1. **§4.5.1 wired AT the refusal site, gated** — when the acceptance gate would
   STOP on an interior vertex, run the selector; where both clauses hold,
   repair: remove the region's points, insert the midpoint of the bounds,
   re-optimize with `max_in_domain_step` truncation at the domain exit,
   continue on the neighbouring patch's parameterization (Fig-12 (c)–(d)),
   then solve `q1`/`q2` on `C_b` and refine per §4.3.4. STOP unchanged where
   any clause fails or the repair does not converge (P10). Pin case: R0003.
2. **The §4.5 loop conversion** — the paper collects failures after the sweep
   and repairs region-by-region, repeating until none persist (`:652-670`);
   ours STOPs at the first refusal. Converting refusal → record-and-continue
   (vertex left unmoved = the paper's "cannot converge" state) with the
   repair loop post-sweep keeps P10 (a run with unrepaired failures still
   cannot complete) and puts the selector at the paper's vantage. Natural
   companion of §4.5.2, which needs the post-sweep view anyway.
3. **§4.5.2 local refinement** (the majority owner) per Fig-14 — roadmap item
   3d/4's guard-shell posture stands.

**Scope honesty:** the confirmed population is 2 vertices in ONE case;
§4.5.1's direct corpus reach is at most R0003 (currently ERROR), and only if
its two regions are that run's whole story — each STOP is one invocation, and
a repaired run continues into whatever failure is next. The reason to build it
is not case count: increment 1 is the paper's stated first strategy, its step
primitive already exists and is red-verified (I10 (g)), and its first region
is measured to the letter of Fig-12.

### I13 — the rim×cut junction terminal overrun: Fig-11's on-curve arm + the Cone chart (2026-08-25)

**Status: FLIPPED ALWAYS-ON (2026-08-25, same day).** Flip proofs: default
corpus BIT-IDENTICAL pre-flip (tracked results.json unchanged, clean tree);
gated corpus CATEGORY-IDENTICAL — 271C/0W/36E/1EE/0T, zero CORRECT
regressions, exactly TWO explained detail rows (**R0003** advances face
437 → 467, the I13d out-of-band-run wall; **R0004's ring-CDT subtract wall
CLEARS** — only its pre-existing unrelated `RevolveAxisIntersectsProfile`
first error remains); per-case times equivalent (heaviest 322 s → 307 s).
`YANG_441_CONE_CHART=0|off`, `YANG_441_OPEN_CONIC_PARAM=0|off`,
`YANG_441_ONCURVE_MERGE=0|off` are the dev A/B off-knobs. R0100's face-15
wall did NOT move — it is not this family's shape; its own anchor is owed.

**Wall:** R0003 `TessellationFailed FaceId(437) "ring rejected by CDT"` — the
post-§4.5.1/§4.4.2 next wall (R0100 face 15 / R0004 face 514 recorded as the
same family; R0053 face 474 separately rooted `i6-input-overuse`).

#### (a) Anchor (measured, offline conic fit + `YANG_441_FOLD_ONCURVE` census)

Face 437 is a cone strip (r≈192, station band 0.0504 wide — a fine
revolve-profile step) cut by a plane; the cut curve is a HYPERBOLA, monotone
s(θ) through the strip (one generator crossing per azimuth). The emitted trim
loop's cut chain is `i8 → i9 → i10` (two typed `HyperbolaArc` edges), where
ALL THREE nodes lie exactly on cone∩plane, but their CURVE order is
`i8 (upper junction, s=0) → i10 (lower junction, s=0.0504) → i9 (s=0.0946)`:
the interior vertex sits BEYOND the terminal junction, 0.044 below the strip's
own rim, and the chain doubles back over the same conic segment — an
out-and-back spur whose chart image crosses the lower-rim edge 6.5e-4 from the
junction. Only the cone face's chart CDT sees the crossing (the wall plane's
2D image of the same spur is a simple V), which is why exactly one face fails.

Mesh-level story (fold census, the failing subtract): corner
`(v1818, v1817, v1788)`, both edges on the intersection curve. Pre-relocation
all three sit ON the tool plane but 0.24 OFF the conic (the recorded §4.5.1
DRIFT configuration), stations healthy `0 / 0.027 / 0.0504`, order clean.
Post: the ends land exactly on their rim junctions (§4.5.1 repairs, ~1.5
displacement); the interior vertex is repaired onto the conic AT ITS OWN
AZIMUTH (θ preserved to 9 decimals — the generator projection), which on this
steep hyperbola is station 0.2656 — 4.3 strip-widths past the terminal
junction. Chord inversion t=+5.31. The fold-merge SELECT line for the op:
`inversions=539 apex_moved=539 (on_curve=429) → sites=0` — the always-on
Fig-11 merge owns NONE of them, by its own condition 1 (still apex).

#### (b) Why the owner is Fig-11's merge, not ReorderConic

The census doc assigned on-curve moved-apex inversions to §4.3.4
`ReorderConic`. For the TERMINAL shape that assignment is structurally
impossible: `order_along_curve` refuses an ordering that changes an open
seam's endpoint set (`SeamEndpointsReordered` — reordering `(i8,i9,i10)` to
`(i8,i10,i9)` re-roots the seam junction other patches share). And a reorder
would keep the out-of-band vertex on the face whose domain excludes it. The
correct op is the paper's own Fig-11(b)→(c): q = the intersection point ON
the boundary curve (the junction, richer carrier set), p = the chain vertex
the relocation carried past it; merge p into q inside the §4.4.1
re-triangulation. Post-merge the cut chain is the direct in-band arc
`i8 → i10` on all three incident faces (upper strip, wall, lower strip
untouched); §4.3.4/I5 refinement re-densifies it if sag demands. The existing
merge machinery (substitution-in-cycles + holder re-CDT + I8 carrier
containment) is reused verbatim — I8 also blocks the backwards merge the
junction's own corner would propose (t=1.096 on `(v1782, v1788, v1817)`):
the junction victim carries surfaces the interior survivor is off.

Blockers, each its own sub-gate:

1. **I13a `YANG_441_CONE_CHART` — `SurfaceChart::Cone`.** The victim's cone
   holder must rebuild; today `SurfaceChart::new` is Plane|Cylinder and five
   chartability pre-filters repeat that `matches!`. Consolidate them onto one
   gate-aware `SurfaceChart::supports`; add the Cone variant (project =
   (θ, station); lift = apex + z·axis + z·tan(α)·radial — exact on-surface;
   refuse any patch vertex at/behind the apex station, loudly). This is the
   I2 tail's named "Cone chart" item: gate-on also lifts the construct pass's
   `curved` skip and the corner-merge/rim-trim unchartable-holder refusals
   for cone patches (R0044's 13 blocked sites are this class).
2. **I13b `YANG_441_OPEN_CONIC_PARAM` — `conic_param` += Hyperbola.** The
   selector's certificate needs the curve-parameter order; `conic_param` is
   the recorded single authority ("never a second notion of along-the-curve")
   but covers Circle/Ellipse only. Add the open-conic arm
   (t = asinh(v/semi_conjugate), monotone on the branch, no wrap) and make
   the two wrap-assuming consumers variant-aware (`order_along_curve`'s
   largest-circular-gap cut and `conic_param_deltas`' (−π,π] wrap apply to
   PERIODIC params only). Gated because the always-on §4.5.3 sweep, I5-1b
   seam merge, and construct refine all consume this authority — the flip
   measures them together. Parabola stays None (no measured customer).
3. **I13c `YANG_441_ONCURVE_MERGE` — the selector arm.** For an on-curve
   moved-apex inversion, propose victim=apex, survivor=the end it crossed,
   iff: both corner edges carry the SAME intersection curve; the survivor is
   the seam run's TERMINAL (its far-side cycle edge is not on that curve);
   the apex is run-INTERIOR; and the curve-parameter order certifies the
   overrun (t_apex strictly beyond t_survivor w.r.t. the t_other→t_survivor
   direction). No distance band (P10) — the certificate is the order
   inversion. Downstream I8 containment and holder-rebuild discipline apply
   unchanged; multi-vertex overruns resolve one merge per pass.

Pin case R0003; measure R0100/R0004 (family), R0044 (holder unblocks),
R0017/R0053 (expected untouched), then gated corpus vs default byte-identity.

#### (c) Measured en route (R0003 gated, 2026-08-25)

- **The population is ~190 sites per boolean, not 1–2** — one terminal
  overrun per strip×wall junction of the fine revolve profile, nearly all
  holding the SAME wall patch. Two consequences, both fixed:
  - the fold-merge pass cap of 32 (tuned to the still-apex family) BINDS and
    strands the family half-repaired; the cap is now derived from the true
    runaway bound (each applied pass strictly removes one boundary vertex ⇒
    `mesh.verts.len()`, floored at 32). Cost measured: ~200 single-site
    passes ≈ +5 s on the case.
  - `rebuild_merge_fan`'s victim-branch θ-unwrap keyed on
    `Surface::Cylinder` only — a cone fan straddling ±π would have charted
    wrong; now `Cylinder | Cone`, with the apex guard applied to the fan's
    vertex set too.
- **First full drain: 202 merges applied, wall moves 437 → 467 (same
  class).** At the fixed point 57 inversions remain: 14 terminal-overrun
  sites BLOCKED by the wall's fan CDT (`TriangulationFailed` — the fan
  polygon inherits neighbouring folds), 32 interior on-curve crossings, 11
  off-curve. The interior crossings are ReorderConic's; interleaved with the
  remaining terminal overruns they corrupt the fan polygons — so the
  open-conic ordering is NOT deferrable for this case:
  `order_along_curve` gained the open-conic arm (ascending order IS the
  chain order; no circular-gap cut; a closed chain on an open conic declines;
  endpoint guard unchanged).
- The construct pass's cone-patch rebuilds decline with `ChordDegradation`
  (old_max ≈ 2.5 → new_max ≈ 39–52, the geometry-blind chart CDT shaving the
  strip bulge) — loud, safe, and a recorded quality tail for the cone chart
  (I2e seeding may need the cone's station-dependent radius).
- **The alternation converges** (`SeamEndpointsReordered` declines 496 → 44;
  one extra merge in round 2; joint fixed point reached) and the residual is
  measured: 14 certified sites blocked (7 by the wall patch's fan-CDT
  refusal), 32 cert-refused on-curve inversions. Face 467's rejected ring
  (probe captured 2026-08-25) names the family: an **out-of-band terminal
  RUN** — ring nodes 14/15 sit BELOW the rim on (by the mixed-corner
  declines) the NEIGHBOR cone's conic, the junction node 13 sits between rim
  and that run in CHAIN order but between the run and the ascending cut in
  CURVE order. Corner-level certificates are structurally inadequate here:
  the chord-sign survivor pick chooses the far sample, and the
  junction-ward corner mixes two curves. Next increment: I13d, §(d) below.

#### (d) I13d — run-level junction absorption (`YANG_441_RUN_ABSORB`)

**Status: FLIPPED ALWAYS-ON (2026-08-25, same day as landing).** Flip
proofs: gate-off default corpus BIT-IDENTICAL (tracked results.json
unchanged after a full run, clean tree); gate-on corpus CATEGORY-IDENTICAL
— **271C/0W/36E/1EE/0T**, zero CORRECT regressions, exactly ONE explained
detail row (**R0003 advances face 467 → 517**, the I13e interlocked-pair
wall below); heaviest case 320 s (budget ≥360 s unchanged).
`YANG_441_RUN_ABSORB=0|off` is the dev A/B off-knob; `census` =
select-and-report at the fold-merge fixed points, never apply.

**Anchor revision by walk-back probe** (`YANG_441_RUN_PROBE_AT=x,y,z` — the
ring node's exact 3D position keyed back to its Stage-4/5 cycle): face 467's
run is NOT a spur of relocated chain vertices. The chain samples barely move
(v2331/v2330, 0.079/0.077); **the JUNCTION v2332 moved 0.67** — solved onto
the rim×cut junction — **hopping PAST its first two chain samples in curve
parameter** (junction t: pre 0.26522 → post 0.26871; samples still at
0.26645/0.26772; the next sample 0.26925 stays ahead). The §(c) reading
("samples relocated onto the neighbor conic") was the one-sided view from
the samples; both edges are typed on the strip's OWN conic (C0, hyperbola).

**Certificate (symmetric pair-order inversion, P10-clean).** For each
maximal same-curve run of typed cycle edges, each bounded end `J`: walking
outward, a vertex `w` is out-of-band iff Stage 4 INVERTED the order of the
pair `(w, J)` along the curve — strict opposite signs of
`t_pre(w) − t_pre(J)` and `t_post(w) − t_post(J)` (wrapped deltas for
periodic conics, raw for open; `conic_param` is the single authority).
Symmetric in which endpoint moved — covers the §(a) spur shape AND the
junction-hop shape. Site = the maximal inverted prefix, iff (1) nonempty;
(2) ≥1 prefix vertex is a MINTED chord inversion (pre in-chord, post out) —
**load-bearing**: the junction's own big relocation also inverts pair
orders against its OTHER chain's in-domain samples (projection artifacts of
the drifted pre position — measured refused on C1: order preserved there,
but the witness is the structural guard since an in-domain chain stays
post-monotone and cannot carry a minted fold); (3) `carried(w) ⊂
carried(J)` proper for every victim (strictly-richer junction; I8
re-checked per merge in the apply path via `carrier_lost_by_merge`).
Ambiguity (a victim claimed by two survivors) drops every touching site.

**Repair: `rebuild_run_fan`** — ONE region rebuild per holder covering ALL
run victims: region = triangles touching any victim; link = the region
boundary's non-victim edges chained into one open run; polygon = link
closed by (end→start), whose closure edge IS the absorbed boundary chain
`survivor → far neighbour` — hence the SURVIVOR must be a link ENDPOINT
(a mid-link survivor would be stranded off the new boundary; refused).
An ORPHAN guard refuses any region vertex that is neither victim nor link
(a boundary vertex sandwiched between non-consecutive victims, or an
enclosed interior vertex, would be silently disconnected — caught by unit
test before it could ship). The per-victim fan is structurally unable to
do this repair: each single link polygon still contains the still-folded
run sibling, so every per-victim CDT is refused (measured — the wall
declines all six single-site fans with `TriangulationFailed`).

**Driver**: consulted at BOTH fold-merge fixed-point exits (no corner site
found / all refused); one site per pass; shares the corner arm's `blocked`
set (a victim the wall already refused as a corner merge is not re-proposed
as a 1-victim run — the region would be identical).

**Measured on R0003 (gated ON, 2026-08-25):** first-op fixed point selects
**31 sites** (runs up to 8 victims: `[8014…8018] → v8015`), census
attribution clean (ambiguous 0; the spurious one-sided-flip population the
old certificate put in `no_inversion` (141) collapses to 14/3). **25
absorptions apply over both holders each (strip cone + wall plane), zero
declines — face 467's ring-CDT wall CLEARS; R0003 advances 467 → 517.**
Face 517 = the SIX remaining corner-certified single-overrun sites (v3221,
v3649, v8582, v8600, v9168, v9187 → their junctions, all on wall patch
475): `YANG_441_FAN_PROBE` (new; dumps the link polygon at every fan-CDT
decline) shows every declined polygon GENUINELY self-intersecting
(crossings=1) — and the link ids name the mechanism: the sites are
**mutually INTERLOCKED PAIRS** — adjacent strips' deep overruns cross each
other's wall territory, so each victim's fan polygon contains the
partner's fold (v3221's link holds {3647, 3649}; v3649's holds
{3219, 3221}; likewise the 8xxx/9xxx pairs and the v6xxx ladder). The CDT
is RIGHT to refuse each single fan.

#### (e) I13e — cross-site group absorption (`YANG_441_GROUP_ABSORB`)

**Status: FLIPPED ALWAYS-ON (2026-08-26, same day as landing).** Flip
proofs: gate-off default corpus BIT-IDENTICAL (tracked `results.json`
unchanged after a full 312-case run, clean tree, 532.9 s); gate-on corpus
CATEGORY-IDENTICAL — **271C/0W/36E/1EE/0T**, zero CORRECT regressions,
exactly ONE explained detail row (**R0003 advances face 517 → 577**), and
marginally FASTER overall (521.1 s); rewrite tier green pre- AND post-flip
(1205 s → 1180 s — the flip costs nothing, which is what settles the
debug-build slowness of `s434_typed_rim_seam_mint` / `m8_swiss_cheese_chain`
as pre-existing, not introduced). `YANG_441_GROUP_ABSORB=0|off` is the
dev A/B off-knob; `census` = select-and-report at the run arm's fixed
point, never apply.

**Mechanism.** Absorb an interlocked GROUP in one region rebuild per
holder (`rebuild_group_fan`, the cross-site analog of `rebuild_run_fan`):
region = union of the group's victims' triangles on the patch; its
boundary edges — victim-incident ones INCLUDED, unlike the run fan's open
link — chain into ONE closed cycle (several loops = not a disk, refused
loudly as `Split`). Each site's victims appear on the cycle as one maximal
ARC; deleting every arc joins each arc's two flanking vertices with a
closure edge = that site's absorbed boundary span `survivor → far
neighbour`, so each site's survivor must FLANK its own arc
(`FanSurvivorNotAdjacent` otherwise). A fused/split/partial arc has no
per-site closure edge and is refused (`ArcMismatch`); the orphan guard is
the run fan's. A holder where only SOME of the group's sites are present
contributes only their arcs — the strip cones each see one site (the k = 1
restriction IS the run-fan construction), the wall plane sees them all —
and its `dropped` lists only the present victims (the `PatchRebuild`
contract is per-patch).

**Grouping (`interlock_groups`).** Candidates = the run selector's sites
at the fixed point where every per-site proposal was refused (their
`blocked` membership is expected — the per-site refusal is what routes
them here), minus refused groups (`group_blocked`, the livelock guard),
minus I8 failures (a not-a-merge victim disqualifies its SITE, not its
whole component — the remainder may still form a repairable group).
Interlock relation: "some mesh triangle contains a victim of mine and a
victim of yours" — exactly "my fan polygon contains your victim", since a
shared triangle puts the other victim on my link. Groups = connected
components of size ≥ 2 (a singleton IS the already-refused per-site
repair; never re-proposed). One group applies per pass; certificates, I8
re-check in apply, the bare-collapse guard, and the all-holders-or-none
closure are the run arm's, unchanged.

**Measured on R0003 (gated ON, 2026-08-25).** Census (never applying): 5
fixed-point consultations across the op chain — 14, 14, 7, 6, 6
candidates pairing into 7 + 7 + 3 + 3 + 3 groups, ALL size 2 (the
"v6xxx ladder" of §(d) is four parallel pairs), `i8_dropped=0`
everywhere. The final-op pairing is v3221↔v3649, **v8582↔v9187,
v8600↔v9168** — the true shared-triangle components CROSS the 8xxx/9xxx
ranges, so §(d)'s conjecture ("v8582↔v8600, v9168↔v9187", read off the
declined links rather than measured) had two of its three pairs wrong.
A declined link polygon names WHICH victims interfere with the site; only
the shared triangle names WHICH sites form one repair unit.
Apply: **10 group absorptions, zero declines, zero I8 drops,
`group_blocked` never populated** — the first op drains its candidates
14→12→…→0 one pair per pass; a later op drains 7→…→1 (one partnerless
singleton correctly left measured, not guessed); two downstream fixed
points vanish entirely (upstream absorption healed them). Runtime flat
(46.6 s vs 46.2 s baseline). **Face 517's ring-CDT wall CLEARS; R0003
advances 517 → 577**, `TessellationFailed { face: 577, "patch
triangulation folded (inverted triangle) — KV9-F2" }` — the KV9-F2 fold
family (§4.3.4/§4.4.2 territory; the F2b anchor was already recorded as
owed).

**Is R0003's face-577 fold MINTED by I13e or merely UNMASKED by it?**
The wall is fail-fast, so the gate-off run never reaches 577 and a
per-face comparison is unavailable (face ids are Stage-6 assignments and
shift with the mesh anyway). Two independent measurements say UNMASKED.
(1) The corpus is the discriminator — a fold-minting repair regresses
cases that tessellate today, and gate-on regressed NONE of the 271. (2)
The KV9-F2 fold is already the standing wall of **R0017** (face 17) with
the gate off, so the family pre-exists this arm; its F2b anchor was
already recorded as owed. The census instrument when that anchor is taken
is `KV2_CHORD_DEPTH_CENSUS`, whose per-patch rows carry the
`fold=inverted` verdict
(`crates/kernel-v2/src/tessellate/developable.rs:1400`).

**KV9-F2 fold ANCHOR taken on R0017 (2026-08-26).** R0017 reproduces the
identical fold in **0.09 s** (vs R0003's 46 s) — it is the vehicle for
this family, ~500x faster. Two existing instruments answer it without
building anything: `KV2_CHORD_DEPTH_CENSUS` (per-face rows) and
`KV2_PATCH_FOLD_PROBE` (dumps the folded triangle).

*The control pair.* Faces 14 and 17 share the SAME development —
identical `w_facet=3.604323e2` and `r_unroll=4.072886e3`. Face 14:
`n_split=16`, `min_h2d=11.39`, `fold=0`. Face 17: `n_split=109`,
`min_h2d=8.11`, **`fold=inverted`**. Same surface, denser refinement,
folds.

*What it is NOT.* All three folded-triangle nodes sit EXACTLY on the ideal
development (`dev` = 6.8e-13, 0, 0). The fold is not a displacement or
deviation defect, and face 17's `max_split_dev` (7.41) is SMALLER than
non-folding face 14's (15.74). Two of the three nodes are refinement
`split` nodes and all three edges are `Interior` — the refinement's own
interior triangulation, not a boundary artifact.

*What it IS.* The triangle is an extreme SLIVER. In the unrolled chart its
longest edge is 1073.3 against a minimum height of 8.114 — **aspect
132:1** (and it is exactly the face's `min_h2d` triangle). Lifted to 3D it
degenerates further: the three points are collinear to one part in 1e5
(`|ab|+|bc| = 1225.62` vs `|ac| = 1225.61` over a 1225-unit span), so its
3D normal is meaningless and reads as strongly inverted (`dot = -0.8051`,
not a marginal miss). The 3D chord short-cuts the curved surface, so a thin
2D sliver spanning a large arc becomes an even thinner 3D one.

*RESOLVED 2026-08-27* — `specs/kv9_f2b_lift_faithful_refinement.md`. The
framing below was right that the fix belongs in the refinement, and the
sliver IS minted there (surface-metric worst aspect 204 → 3473, against a
same-development control face that holds at 109.80 exactly). But the
*mechanism* was not the metric: the LEPP walk is a faithful Rivara
implementation and its parent→child degradation on the minting split is
66.8 → 132.3, exactly the factor of 2 its theorem promises. The guarantee
simply does not control the FOLD, which is a property of the lift to 3D, not
of chart angles. Moving the refinement into the isometric development was
measured and REFUTED in both possible forms (see that spec §4). The repair is
a second refinement criterion — refine while the chart→3D lift inverts —
which converts R0017 on five extra splits. Canonical 271C →
**272C/0W/35E/1EE/0T**.

*Framing for the repair (as recorded when the anchor was taken).* This is the
I2d lesson on a different surface: **a chart-valid triangulation is NOT
geometry-valid on a curved development.** The structural fix belongs in how the developable
refinement triangulates (geometry-aware connectivity), NOT in an aspect-
ratio band — banding the sliver away would be exactly the tolerance
tuning P9/P10 forbids, and would silence a loud STOP without fixing what
mints the sliver. Note also that the census's `min_h2d` takes `.abs()` of
the signed 2D area, so it measures thinness but is sign-blind; the
tripwire's inversion test is 3D (triangle normal vs `outward_at`).

**Corpus-wide customer census (gate-off, 2026-08-26).** The ring-CDT wall
is the standing verdict of exactly THREE of the 36 ERROR cases — R0003
(face 517), **R0053** (face 474) and **R0100** (face 15). I13e converts
R0003's wall and leaves the other two untouched, so the interlocked-pair
shape is not what blocks them; their anchors stay separately owed
(R0100's face-15 anchor was already on the books). Reporting the
denominator: this arm's addressable population was 3, it moved 1, and the
other 2 are measured OUT of the family rather than assumed in.

Unit tests: 6 `rebuild_group_fan` (the measured interlocked-pair
shape with BOTH single fans proven to self-intersect, k = 1 degeneration
to the run fan, fused arcs, mid-cycle survivor, enclosed-interior orphan,
three-site ladder) + 4 `interlock_groups`.

#### (f) I13f — the INVERTED JUNCTION PAIR: a phantom junction whose exact solve lies outside its band, and the missed mirror crossing (ANCHORED 2026-08-28, not built)

R0003's wall after the inc-8 ellipse fix (`yang_434` — f577 skins) is
**FaceId(903), "ring rejected by CDT"**: the WALL-PLANE ring's single
proper 2D crossing, entirely between B-Rep VERTEX origins (unmasked, not
minted — the inc-8 change is render-side only). Anchor by
`YANG_441_RUN_PROBE_AT=-199.7376,-113.9101,-25.9313` (the spur vertex),
first absorption pass of the final subtract:

- The wall cycle (patch 485, S2 = the wall plane, cyc_len 305) walks the
  revolve profile's cone-band stack; each band's wall trace is its own
  hyperbola (S0→C0, S3→C1, S1→C6, …), and band-rim × wall vertices are
  triple junctions.
- The crossing pair: **v8413** carries {S3, S2, S4} (cut-plane corner:
  wall ∩ cone S3 ∩ second cut plane S4; relocated 0.048) and **v8398**
  carries {S0, S3, S2} (band-rim corner: wall ∩ S0/S3 rim; relocated
  0.566 onto its exact triple). Cycle order: … → v8413 → v8398 → … with
  the connecting edge typed C1.
- **Pair-order inversion along C1, in numbers:** pre (mesh) t(v8413) =
  −1.6045121 < t(v8398) = −1.6019084; post (exact triples) t(v8398) =
  −1.6047422 < t(v8413) = −1.6045353. The I13d selector FIRES its order
  and minted-chord-inversion certificates on this pair and refuses at
  `strictly_richer` (not_richer bucket): NEITHER triple's carrier set
  contains the other — absorption would delete a true corner. That
  refusal is CORRECT (the I8 lesson); the family needs a different
  repair, not a loosened gate.
- **What the true order means:** on the S3 cone face (patch 430) the
  band's whole wall trace is the C1 stretch [v8398 → v8413] — 0.04 long.
  Post-relocation that stretch has NEGATIVE length: the exact
  S3∩S2∩S4 point lies 0.0002 in parameter BEYOND the S0/S3 rim, i.e.
  **outside the S3 band's domain — v8413 is a junction-level PHANTOM
  (the §4.3.3 rule-out clause, R0100's Case-IV lesson at a junction
  instead of a loop)**. In exact geometry the S3-band's wall corner
  sliver does not exist: the cut line S4∩S2 truly crosses the S0 band's
  hyperbola C0 at a junction {S0, S2, S4} **that the mesh never minted**
  (the missed mirror crossing), the wall boundary runs S4-line → newJ →
  C0 → v8394 → …, and v8398 (the rim corner) leaves the wall cycle.
- **Repair shape (phase-3 junction layer, first small customer):**
  junction RE-HOMING across a rim — (1) certify the phantom by the
  domain clause (exact solve outside the band's rim-bounded parameter
  interval on its typed conic); (2) mint the mirror crossing on the
  ADJACENT band's conic (solve S4∩C0 exactly; mint ONCE, share by
  identity — the junction contract in
  `docs/yang_junction_research_findings.md` is BINDING); (3) update the
  affected cycles (wall + both cone bands) to the true topology; every
  decline is a loud typed STOP naming the pair. This is the same
  structural need R0100's corner-phantom session recorded ("ruling out
  the loop leaves pieces whose true boundary needs the MISSED mirror
  crossings created"), in a 1-junction/1-crossing shape — the right
  vehicle to build the junction-layer mesh-update primitive against
  before the R0100 loop-level case.
- SELECT accounting at the pass: runs=692 terminals=1384 no_param=40
  no_flip=1242 no_inversion=14 **not_richer=26** ambiguous=0 → 31 sites
  (absorbed elsewhere); the I13f pair is in the not_richer bucket, which
  a census mode should split into "true-corner pairs" (this family) vs
  other refusals before building.

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
