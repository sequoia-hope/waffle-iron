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
   - `TriangulationFailed` ×8 — every one the SAME geometry: the collapsed
     straight seam `(minted-junction, outline-junction)` properly crosses
     one of the patch's own PLAIN outline edges whose near endpoint is
     numerically adjacent to the seam's outline junction (1090↔1096,
     1523↔1524, 1616↔1617, 1720↔1721 …) and whose far endpoint is a minted
     vertex in a regular per-rib series (2574, 2546, 2518 … step ≈28).
     This is the Fig-11(a) boundary-junction configuration (intersection
     point q ON the boundary curve): the outline chain near the junction
     disagrees with the exact seam — an upstream junction/outline placement
     defect, NOT a collapse defect. Anchoring which mint (relocated outline
     vertex vs missing/misplaced boundary split) is the next probe.
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
