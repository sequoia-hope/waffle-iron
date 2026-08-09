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
