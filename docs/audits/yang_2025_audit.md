# Yang 2025 compliance matrix (paper-ordered)

**As of 2026-10-16, HEAD `62c12b57`** (F-1 resolved the same day; row cites elsewhere are at `62c12b57`). This replaces the 2026-04-20 audit
(79 lines, written when the pipeline passed 1 of 157 cases; git history keeps
it). Where `specs/yang_faithful_cleanup.md` §2 starts from the code and asks
which paper mechanism each arm or pass stands in for, this document starts
from the paper. It asks of every procedural statement in §3–§4 (and the
procedural parts of §5.5, §5.6 and Appendices A–B) where it is implemented,
how faithfully, and what pins it. Read the two together: the cleanup table
finds code that should not exist, and this matrix finds paper mechanisms that
do not exist, exist partly, or run at the wrong point.

**Paper cites** are line ranges of `refs/text/yang2025_hybrid_boolean.txt`
(`./scripts/extract-papers.sh`). That text is two-column interleaved, so a
range can name lines whose other column belongs to a different section. Some
older ledger cites were made against an earlier extraction and are off by
tens of lines (see L-7).

**Code cites** are `path:line` at `62c12b57`. Each was opened when the row
was written. Line numbers drift; the function names are the stable part. Each slice
states its own path root (`crates/` or `crates/yang-rs/src/`).

**Method.** Six read-only passes, one per paper section group, each walking
its sections in order and emitting one row per procedural statement (a step,
decision, guarantee, threshold or parameter). Every pass also listed the code
in its files that no row accounts for, and compared the paper's step order
with the code's call order. The cross-cutting pipeline order below was read
from `boolean.rs` directly. Three findings were re-verified by hand before
this was written: the exhausted-loop policy (F-1), the d_ε scale (U-1) and
the Stage-0 relocation subsystem (U-2).

**Status vocabulary.**

| status | meaning |
|---|---|
| FAITHFUL | does what the paper says, where the paper does it |
| PARTIAL | the mechanism exists and covers a named subset |
| DEVIATES | does something different; the ledger N-id, or UNLEDGERED |
| ORDER | exists, but runs at a different point than the paper's order |
| MISSING | no implementation; the row names what compensates |
| SILENT | the paper leaves it open; the row names our choice |
| N/A | out of scope by a recorded decision (D14 analytic-only, N7 closed-form SSI) |

**Maintenance.** A change that touches a mechanism updates its row in the
same commit as its ledger entry (the P2 rule). A row moves to FAITHFUL only
with a pin named in its `pin` column.

## Summary

| slice | rows | FAITHFUL | PARTIAL | DEVIATES | MISSING | ORDER | SILENT | N/A |
|---|---|---|---|---|---|---|---|---|
| §3, §4.1, App. A — discretization | 43 | 10 | 11 | 8 | 2 | 2 | 1 | 9 |
| §4.2, App. B, §5.6 — mesh intersection | 31 | 9 | 6 | 7 | 2 | 0 | 1 | 6 |
| §4.3 — intersection optimization | 25 | 8 | 4 | 9 | 3 | 1 | 0 | 0 |
| §4.4 — mesh updating, Booleans | 28 | 10 | 10 | 4 | 1 | 2 | 1 | 0 |
| §4.5, §4.5.1, §4.5.2 — failure handling | 32 | 11 | 6 | 5 | 3 | 0 | 6 | 1 |
| §4.5.3–§4.5.5, §5.5 — reversal, illegal intersections, coplanarity | 35 | 18 | 9 | 4 | 0 | 1 | 3 | 0 |
| **total** | **194** | **66** | **46** | **37** | **11** | **6** | **12** | **16** |

Eighteen rows said UNLEDGERED outright when the matrix was written, and the
cross-cutting findings below added more that only show up across slices.
**All are ledgered since 2026-10-17** (N80–N94, and amendments to N2, N4,
N7, N62, N68, N79); each row's ledger column names its entry.

## Cross-cutting findings

Ranked within each group. A slice-local reference such as `s44 F2` means
finding 2 of the §4.4 slice below; a row id such as `4.5.1-05` names a row.

### F — a known failure leaves without a STOP or a refinable site (P10)

- **F-1. RESOLVED 2026-10-16 — the exhausted §4.5.2 loop shipped bodies
  whose failure set was not empty.** `refine_loop` returned the natural body
  with its failures only inventoried, or adopted "the first watertight
  refined body", which also still had failures. The paper's loop runs
  "until all the failures are resolved" (`:309-312`, `:667-668`) and names
  no ceiling. **User sign-off 2026-10-16: the ceiling is LOUD.** Only an
  empty failure set is adopted; surviving failures STOP as
  `YangError::RefinementExhausted`. Corpus 358C → 350C. All eight moves are
  recorded failures that used to ship: four T2, two T6, and two natural
  STOPs that had been masked. Three ERROR rows (P0045, P0056, P0061) now
  name their cause at the source instead of kernel-v2. Ledger N66
  amendment. (`5.6-03`, `4.5-10`.)
- **F-2. RESOLVED 2026-10-17 for the crossing class** (a CDT `TriangulationFailed` records a T3 site naming the faces C_p traverses; the other refusal classes are N2's named capability gaps; corpus 349C / 1T — R0044 slow, the M-1 cost; triage §"2026-10-17 (F-2, F-3)"). **A refused §4.4.1 patch update was silent.** `update_patches`
  (`stage4_construct.rs:3178-3207`) `continue`s on every refusal and maps an
  `apply_rebuild_batch` error to 0. The Phase-2b spec says a refusal records
  a §4.5.2 failure site. 2,312 patches currently fall back to
  relocate-in-place under `ChordDegradation`. (`s44 F3`, `s44 F6`.)
- **F-3. RESOLVED 2026-10-17** (all three exits name faces: T7 `ReversalUnresolved` for the gate's survivors and a no-progress removal, the last reversed point for `max_passes`; N29 amendment). **A §4.5.3 failure couldn't reach §4.5.2.** Three exits:
  - the 2·d_ε refusal (`reversal.rs:286`) leaves the reversal in place
    without a STOP or a site;
  - `Stage4ReversalUnresolved` (`:316`) is a loud STOP but names an edge
    and a vertex, not faces;
  - the `max_passes` STOP (`:40-45`) names vertex `u32::MAX`.

  None reaches the loop as a refinable region, though the paper's answer to
  an unresolved reversal is refinement. (`s453 F2`.)

### O — order of operations

- **O-1. Classification runs before optimization and mesh updating.** *Ledger N80 (2026-10-17); design `specs/yang_442_classify_after_update.md`.* The
  paper trims and updates both meshes, then classifies (`:572-576`). The
  code classifies the raw arrangement (`boolean.rs:2131` `keep_set`), then
  relocates and updates only the kept submesh. See the pipeline-order
  section. No ledger entry names it; N2's "trusts the sidecar-trimmed mesh"
  describes it without recording it. This is the structural reason a
  Case-IV phantom can't be ruled out by the update (cleanup §2a, C0117),
  and the reason §4.4.1 can't be "r_A = r_B = r" on two meshes.
  (`s44 F1`, `s43 F2`.)
- **O-2. Within Stage 4 the paper's order is not kept.** *Ledger N81.*
  - The §4.3 point removals and the Case-IV rule-out
    (`stage4_correct.rs:14274`, `:12975`, `stage5_topology.rs:4925`) run
    after the §4.5.3 sweep and the §4.4.1 update. The paper does them during
    optimization (`:527-536`).
  - §4.4.1 itself runs in three places: `update_patches`, the
    construct/fold-merge alternation (`stage5_topology.rs:4808`), and the
    Stage-5 seam merge/decimation.

  (`s43 F2`, `s44 F5`.)
- **O-3. Exact junctions are minted at Stage 1.** *Ledger N82; cleanup §2d.* The rim-junction insertion,
  P3a and the tangent boost (`boolean.rs:1125`, `:1210`, `:1297`) run before
  the arrangement; the paper identifies these vertices in §4.4.1. They appear
  only as an N2 "partial remediation" bullet and are absent from the cleanup
  table. (`s41 F4`.)
- **O-4. §4.5.4 detection runs on the emitted B-Rep, not the updated mesh.** *Ledger N83.*
  The mesh census (`failures.rs:589`) is test-only. T3b/T3c cover planes and
  unrolled cylinders; cone, sphere and torus faces and axis-wrapping loops
  are gaps. (`s453 F3`.)
- **O-5. Failure collection stops at the first fire.** *Ledger N84.* Only the OffCurve
  gates record and continue. About 50 `LocalRefinementRequired` returns end
  Stage 4 immediately, so each run names one vertex's faces. The paper
  collects all point pairs (`:656-659`). (`s451 F5`.)

### M — missing or half-built paper mechanisms

- **M-1. §4.5.2 locality.** *Ledger: N62 reopened OPEN (2026-10-17).* Three gaps:
  - The refined op is recomputed whole (`boolean.rs:769`), where the paper
    recomputes "only in the refined regions".
  - There is no p_f/p_b walk along C_p and no splice of the new curve.
  - The region is the faces at the STOP vertex plus one ring, not "the
    surfaces traversed by C_p".

  N62 is RESOLVED, and its text dropped the first gap. (`s451 F3`, `F4`.)
  *Cost, measured 2026-10-17 (R0044):* the first gap's cost was mostly a
  quadratic cherchi-rs Stage 2 (ledger N95, RESOLVED); what remains is the
  §4.5.3 sweep's whole-mesh Phase A per removal and a region that covers
  the whole operand (a planar cap's one-ring is every flank).
  *Region (third gap), L1 2026-10-17:* collected T1 failures and every T2
  fire now name the faces their C_p span traverses
  (`specs/yang_452_cp_region.md`); the first-fire T1 returns (N84) still
  name one vertex.
- **M-2. §4.5.1 for interior points is half built.** *Ledger N90.* Remove, midpoint and
  project are live for cone+plane and torus pairs only. Truncation at C_b,
  continuation on S1 and the q1/q2 solve are absent; `max_in_domain_step`
  is print-only. Meanwhile the two live transits (U-3) apply §4.5.1 to the
  boundary class the paper excludes. (`s451 F2`.)
- **M-3. Tangency.** *Ledger N91 (and the N7 amendment).* The paper uses Newton exactly for tangent points
  (`:489-492`). Our pair Newton refuses tangency (`stage4_relocate.rs:340`),
  and tangent points exist only as a cylinder×cylinder Stage-1 mint
  (`tangency.rs:78`). Also missing are small-loop tracing (`:598-601`), the
  Fig. 10(c) seeding, and detection below the model band
  (`shell_contact.rs:463-476`). Contacts that touch without crossing are
  never resolved. (`s43 F3`, `F4`; `s42 F3`.)
- **M-4. The §4.2.2 Gauss-map filter.** *Ledger N92.* D13 calls it moot for analytic
  surfaces. Only the Bézier half (Theorem 4.1) is out of scope; analytic
  surfaces have closed-form Gauss maps. (`s42 F2`.)
- **M-5. The Fig. 11(c) empty-loop insert.** *Ledger: N2 amendment (2026-10-17).* It was deleted in Phase 0 as
  "subsumed by d(T) seeding". The seeding fires only on curved
  `ChordDegradation`, so the step is covered only because the arrangement
  already triangulates loop interiors. (`s44 F2`.)
- **M-6. d(T) / d_ε(T) at Stage 1.** *Ledger N93.* Stage 1 never computes or certifies
  the boundary bound. The faithful primitive `stage4_dt.rs:101` `d_of_t`
  runs only at Stage 4. The sphere's half-budget rests on a corpus
  measurement, and cone charts are unbounded (N61). (`s41 F2`.)
- **M-7. Parametric boundary curves.** *Ledger N94 (PROPOSED, with D14).* The paper restores boundary curves
  "in the parametric domain" (`:581-594`). We emit 3D analytic curves and
  coalesce them in four passes; no entry names the absence of p-curves.
  (`s44 F7`.)
- **M-8. §4.3.4 refinement** *(ledger: N7 amendment)* covers periodic conics only, is capped at 4096
  inserts, and steps the curve parameter rather than optimizing from the
  surfaces' parameter midpoints. N7 says §4.3.4 is "not implemented".
  (`s43 F5`.)

### U — unledgered substitutes and deviations

- **U-1. d_ε is scaled per operand from its circle-rim bounding box** *(ledger N85)*
  (`normals_chord_bounds.rs:234` `curved_chord_bound`), with per-kind scales
  for other surfaces. The paper uses the B-Rep model's bounding box
  (`:391-395`). D11 describes the deleted legacy rule. (`s41 F1`.)
- **U-2. Stage 0 carries its own Fig-11 mesh-updating subsystem.** *Ledger N86; cleanup §2d.*
  `stage0/reloc.rs` has about 2.4k production lines (cavity relocation,
  Lawson flips, Fig-11(a) split/slide), driven at `stage0/mod.rs:1597-1958`.
  It has no ledger entry and is absent from the cleanup table, and cleanup
  §5 declares Stage 0 "not touched". It is the largest substitute the
  debt count does not see. (`s453 F1`.)
- **U-3. The crease and corner transits** *(ledger N87; cleanup §2d, removed from §5)* (`stage4_boundary_curve.rs:1560`
  `solve_crease_transit`, `stage4_correct.rs:4984` `corner_transit_apply`)
  apply §4.5.1 to boundary and corner points, which the paper sends to
  §4.5.2 (`:635-657`, Fig. 13). The catch-up row grades P0040's transit
  "faithful", and cleanup §5 lists it as untouched. (`4.5.1-05`, `-10`,
  `-13`; `s451 F1`.)
- **U-4. Stage-1 folds across patches.** *Ledger N88; cleanup §2d.* The intra-solid cylinder-pair
  phantom fold, the thin-band rim fold, the chart-crossing re-run and the
  P0008 planar-neighbour chord split each break "discretize each patch
  independently" (`:400-401`). (`s41 F3`.)
- **U-5. Smaller unledgered items** *(ledger N89, items 1–8 in this order)*:
  - `collapse_subtauwork_mesh_edges` (`stage4_correct.rs:3983`);
  - the T6 detection floor `2·max(TAU_MODEL, scale·TAU_WORK)`;
  - Parabola and Hyperbola curves never reversal-tested (`reversal.rs:1178`);
  - the ellipse-bounded planar face wall in Stage 0 (`stage0/frame.rs:1016`;
    the paper's own Fig. 24(b) can't run);
  - the coincident-cylinder Stage 0 (`stage0/cylinder.rs:416`);
  - the §4.5.3 overtake arms, ledgered only as N29 amendments, which bypass
    the endpoint guard and the paper-order removal;
  - the Fig. 11(b) merge threshold (N55 records identity-only merging but
    not that a quality threshold is the paper's);
  - the Stage-2 broad phase (D12, HISTORICAL, cites deleted code; still
    O(n²) in cherchi-rs).

### L — ledger and plan text that no longer matches the code

- **L-1. N68. FIXED 2026-10-17** (index row OPEN, body amended). The status index said RESOLVED, by a guard deleted on
  2026-10-14; the entry body says OPEN. The rule-out still acts only on
  closed components.
- **L-2. N62. FIXED 2026-10-17** (reopened OPEN with the M-1 gaps). It was RESOLVED as faithful, but see M-1. Separately, the loop has
  no direct yang-rs pin for adoption or ceiling behaviour (`s451 F8`).
- **L-3. N79. FIXED 2026-10-17** (index row added, State line corrected; N70 was missing from the index too and is added). N79 was absent from the status index, and its State line cites the
  deleted `YANG_PLANE_WELD` kill switch.
- **L-4. N4. FIXED 2026-10-17** (amendment: identity only; the (u,v) half folds into N7). N4's title says RESOLVED, but only face identity was built; the
  §4.2.3 barycentric (u,v) map was not (unnecessary under N7/D14, but
  unrecorded).
- **L-5. N2. FIXED 2026-10-17** (text corrected, amendment). N2 claimed "near-boundary interior removal"; the code removes only
  exactly collinear interiors.
- **L-6. Stale code locations:**
  - N5, N9, N10, N12 and N27 cite the pre-decomposition `lib.rs` layout;
  - N7, N26, N29 (deleted functions and the "parameter-nearer" rule),
    N31 (the 1e-13 floor), N32 and N66 cite moved or deleted code;
  - D13 and D14 cite deleted `crates/kernel`;
  - cleanup §2b's line numbers are stale.
- **L-7. Stale paper cites.** The catch-up rows' `:689-692` should be
  `:639-640`, and `:664-676` should be `:680-686`. `selector_clause2_walk`'s
  doc cites `:740-744`; the text is at `:651-657`.

## What this changes in the plan of record

These are recommendations; nothing is adopted until the cleanup spec says so.

1. **Ledger the U and O items first.** That is the P2 rule, and it is
   bookkeeping only. U-2, U-3 and O-1 change the debt count materially.
   **Done 2026-10-17:** N80–N94 and six amendments; the ledger's debt count
   went 47 → 64; cleanup §2d lists the four new substitutes.
2. **F-1 to F-3 come before further retirements.** (F-1 done 2026-10-16; F-2 and F-3 done 2026-10-17.) Each is a path where a
   known failure leaves without a STOP or a site. Under P10 a known failure
   must be loud or must reach the loop. F-2 and F-3 are small. F-1 needs a
   decision on what the loop does at its ceiling, and that is a sign-off
   question.
3. **O-1 is the deepest item.** Phase 2b is building §4.4.1 on the kept
   submesh. The paper's order (update both meshes, then classify) would move
   classification after Stage 4. That is a pipeline restructure, not an
   increment, and it needs its own design spec before Phase 2b goes much
   further. Several open items — C0117's phantom, r_A = r_B = r, the three
   §4.4.1 sites — trace to it.
4. **M-1 and M-2 reopen Phase 1.** §4.5.2's locality and §4.5.1's interior
   half are the paper's answer to cases the transits (U-3) and the whole-op
   re-run currently absorb.

## Pipeline order — the paper's sequence against `yang_rs::boolean`

The paper's operation is one sequence (§3 Fig. 2, §4.1–§4.4, with §4.5 as the
failure branch). The code's sequence is read from `boolean.rs`
(`boolean` `:845`, `boolean_once` `:973`, its numbered step comments) and
`stage5_topology.rs::reconstruct_topology_stage4` `:4504` →
`stage4_correct.rs::stage4_relocate_and_correct_inner` `:8918`.

| # | paper step | paper | code step | code | order |
|---|---|---|---|---|---|
| 1 | §4.5.5 coplanar preprocessing (before discretization) | `:718-750` right col. | (0) Stage 0 | `boolean.rs:1156` | same |
| 2 | §4.1 discretize each face to d_ε, shared boundaries | `:286-410` | Stage 1: each operand's mesh is built with the B-Rep (`as_mesh`) and re-tessellated in `boolean_once` by the arms and Stage 0 (Case-IV phantom guard `:1007`, rim overrides, Stage-0 conformal rebuild); the meshes handed to Stage 2 are chosen at `:1489` | `boolean.rs:1007-1489` | interleaved with step 1 — Stage-1 rebuilds run both before and after Stage 0 |
| 3 | §4.2 mesh intersection | `:399-520` | (1) Stage 2 exact labeled arrangement — which **also computes the inside/outside labels** | `boolean.rs:1829` | same position; **labels computed here, early** |
| 4 | §4.3 optimize intersection points onto the exact curves | `:480-600` | (6) relocation inside `reconstruct_topology_stage4` → `stage4_relocate_and_correct_inner` steps (1)–(2t) | `stage4_correct.rs:9002, 11193, 12360` | **after step 6** |
| 5 | §4.4.1 trim and update BOTH meshes (r_A = r_B = r, CDT, merge, insert, d(T)) | `:605-610`, `:555-571` right col. | (3c) `update_patches` + the (3c)/(3d) repairs, on the **kept submesh only** | `stage4_correct.rs:13076, 13338, 13400` | **after step 6, on one mesh** |
| 6 | §4.4.2 "After trimming the meshes … we directly apply a standard inside/outside classification [Cherchi 2022] to identify the triangles that need to be retained" | `:572-576` right col. | (3) `la.keep_set(op)` + (4) compact the kept submesh | `boolean.rs:2130, 2205` | **before steps 4–5** |
| 7 | §4.4.2 segment the result into patches along boundary curves; fit curves in the parametric domain | `:578-594` right col. | (5) Stage 6 face resolution (N4 provenance) + Phase A loops in `reconstruct_topology_stage4` | `boolean.rs:2599`, `stage5_topology.rs:4527` (`compute_phase_a`) | same |
| 8 | §4.5.3 reversed-point correction (part of the failure handling around §4.4.1) | `:679-751` | (3) sweep, between relocation and `update_patches` | `stage4_correct.rs:12772` | see §4.5.3 rows |
| 9 | §4.5.2 local refinement: refine the failed region and "compute the intersections between the meshes only in the refined regions" | `:659-690` | `refine_loop` re-runs the **whole** `boolean_once` at the refined density | `boolean.rs:621, 966` | see §4.5.2 rows |
| 10 | §4.4.3 watertightness inherited from the mesh Boolean | `:599-606` right col. | (4b) explicit Stage-4 watertightness gate | `stage4_correct.rs:14017` | same |

**The inversion (steps 4–6).** The paper optimizes the intersection
points, updates BOTH trimmed meshes so they share the exact curves, and only
then classifies inside/outside on the updated meshes. The code classifies on
the raw arrangement (Cherchi 2022's labels come out of Stage 2), keeps the
op's triangles, and then relocates and updates only the kept submesh. Two
consequences follow from that order, not from any one pass:

- The update cannot be "the same r on both meshes". After the keep, the two
  operands' triangles along an intersection curve are one mesh's two sides
  of a seam, and r_A = r_B = r becomes "relocate the shared seam vertex".
  That is faithful in effect only while the seam is shared.
- Every failure the relocation causes is downstream of the classification.
  A relocation that should have changed which triangles are kept (a phantom
  overlap the optimization rules out, §4.3.3 Case IV) cannot, because the
  keep already happened. This is the mechanism the cleanup spec §2a names for
  C0117 ("our exact arrangement cuts along the phantom before Stage 3 sees
  it").

**Ledger state.** No entry names this order. N2 says "`yang-rs` trusts the
sidecar-trimmed mesh", which describes it without recording it as a
deviation from `:572-576`. Recorded as finding O-1 above.

## Slice: §3 Overview, §4.1 Surface discretization, Appendix A


Paper: `refs/text/yang2025_hybrid_boolean.txt`. §3 is the right column at `:241-250` and continues in the left column at `:281-321`. The Fig. 2 caption is `:211-219`. §4.1 starts in the right column at `:286-321`. Page 5 has the left column at `:348-407`, and §4.1.2 continues at the top of the right column (`:350-352`). The right column at `:354-396` covers the d(T) material. Appendix A is at `:1163-1209`.
Code paths are relative to `crates/yang-rs/src/` unless noted. The scope decisions are D14 (analytic only) and N7 (closed-form SSI).

| id | paper | statement | code | status | ledger | pin |
|---|---|---|---|---|---|---|
| 3-01 | `:211-213` | Fig. 2: "establish a bijection between the B-Rep model and the triangle mesh" through error-bounded discretization (a) | `brep.rs:775` `from_topology_and_tess` (per-vertex `TessellationMap`, per-triangle `tri_face`); `stage1_tessellate.rs:506` `stage1_tessellate_once` | FAITHFUL | — | `src/tests_unit/construction_stage1.rs::brep_new_bijection_is_one_to_one` |
| 3-02 | `:213-216` | Fig. 2 (c): "increasing the resolution … iterating over erroneous regions" comes before optimization maps the curves back and before mesh updating (d) | `boolean.rs:621` `refine_loop`, called at `boolean.rs:966` after the whole `boolean_once` (Stages 0–6) | ORDER | N62 (RESOLVED) | `src/tests_unit/s452_face_refine.rs::refinement_composes_by_doubling_and_saturates_at_the_ceiling` |
| 3-03 | `:216-218` | (d) "intersection curves are used to update the mesh, maintaining the bijection" between each curve-bounded patch and its discretization | `stage4_construct.rs:3113` `update_patches` (called at `stage4_correct.rs:13338`). Refused patches keep their old triangles, and the Stage-4 repair passes remain | PARTIAL | N2 | — |
| 3-04 | `:218-219`, `:318-321` | (e) "inside/outside classification" on "each patch of the resulting mesh"; the B-Rep patches are retained to match | `boolean.rs:35` `flip_for_op` reads `LabeledArrangement` labels per arrangement triangle or cell, not per B-Rep patch | DEVIATES | N2 (ledger directive item 7: "its labels are per patch; ours are per arrangement cell, the N2 gap") | — |
| 3-05 | `:241-242` | Inputs are B-Rep models and an op: "intersection, union, and subtraction" | `boolean.rs:845` `boolean(a, b, op, backend)`. `BoolOp` also has Xor (`boolean.rs:44`) | FAITHFUL | — | — |
| 3-06 | `:244-246` | Faces are "planes, quadratic surfaces, and NURBS surfaces" | `geom.rs:37` `Surface` covers Plane / Sphere / Cylinder / Cone / Torus only | N/A | D14 | — |
| 3-07 | `:283-285` | The bijective mapping is kept "throughout the Boolean operation process" | Stage 1: `brep.rs:298` `TessellationMap`. Output vertices are tagged `TessellationSource::Intersection` (`brep.rs:287`), so their (u,v) are not kept; per-triangle `triangle_attribution` / `face_attribution` (`brep.rs:421,425`) are kept | PARTIAL | N4 amendment (2026-10-17: identity only; the (u,v) half is N7's); the per-vertex half belongs to §4.4.1 r_A = r_B = r | `src/tests_unit/boolean_functional.rs::brep_new_populates_tri_face_provenance` |
| 3-08 | `:290-292` | Results "are ensured to be watertight B-Rep models" | Checked, not ensured: Stage-4 half-edge gate `stage4_relocate.rs:2413` (`s4-halfedge-pairing`) plus the refine loop's watertight adoption (`boolean.rs:621`). A loud STOP stands (corpus 11E) | PARTIAL | — | — |
| 3-09 | `:296-302` | Step 1: build the bijective mapping, then intersect the meshes with [Cherchi 2022] | Stage 1 (`stage1_tessellate.rs:506`), then Stage 2 cherchi-rs (owned by the §4.2 slice) | FAITHFUL | — | — |
| 3-10 | `:299-301` | Accuracy is guaranteed under d_ε "using an iterative triangulation scheme" | Closed-form smallest N, with no iteration: `stage1_tessellate.rs:617-624`; sphere `stage1_tessellate/surface_face_tessellators.rs:92-107` | DEVIATES | N5 | `tests/yr7_cylinder.rs::oracle1_mesh_within_chord_error_of_surface` |
| 3-11 | `:303-309` | Step 2: map the loops back. "Bijectivity … ensures each intersection loop can be mapped" to either model; "numerical optimization" builds the correspondence | `stage3_ssi.rs:768` `build_intersection_curves` (closed-form SSI) | N/A | N7 | — |
| 3-12 | `:309-312` | "failed areas will undergo local intersection handling or local mesh refinement until all the failures are resolved" | `boolean.rs:621` `refine_loop`. Per-face ×2 is capped at `FACE_REFINE_MAX = 8` (`stage1_tessellate.rs:158`), and a natural STOP stands when no refined body is watertight | PARTIAL | N62 | `src/tests_unit/s452_face_refine.rs::refinement_composes_by_doubling_and_saturates_at_the_ceiling` |
| 4.1-01 | `:289-292` | "surface-to-mesh distance is guaranteed to be within an input error-bound d_ε" | Closed-form chord sagitta per surface kind: rim N `stage1_tessellate.rs:617-624`; sphere `d_ε/2` per segment, justified only empirically ("verified: worst centroid deviation ≈ 0.82·d_ε", `surface_face_tessellators.rs:85-92`); torus `√(8·d_ε/r)` meridian step `patch_tessellators.rs:1818`; cylinder chart interior-edge census `stage1_tessellate.rs:4088-4201`. Cone charts are boundary-only (`:3936-3939`, `ChartBudget::default()` at `surface_face_tessellators.rs:238`) | PARTIAL (sphere unproven; cone chart unbounded; chart boundary chords only counted) | N5, N61 (OPEN for cones) | `tests/yr12_sphere.rs::oracle1_mesh_within_chord_error_of_surface`; `tests/yr16_cone.rs::oracle1_mesh_within_chord_error_of_surface` |
| 4.1-02 | `:294-296` | Trimmed surfaces: "first generate a mesh on the untrimmed surface and then trim it using the boundary curves" | Each trimmed face is tessellated directly against its boundary chains (per-face dispatch `stage1_tessellate.rs:2112-2400`). Only the cylinder chart comes close: spade refines the convex hull and the exterior / hole triangles are dropped (`:4092-4110`) | DEVIATES | N5 | — |
| 4.1-03 | `:298-300` | "discretizes each closed B-Rep model composed of multiple surface patches into a triangle mesh" | `stage1_tessellate.rs:2112` per-face loop in `stage1_tessellate_once` | FAITHFUL | — | `tests/yr7_cylinder.rs::oracle4_euler_characteristic` |
| 4.1-04 | `:300-301` | "a bijective mapping between each surface patch and its discretization is constructed" (each triangle knows its patch) | `face_tri_ranges` → `tri_face` (`brep.rs:434`, built at `brep.rs:782-790`) | FAITHFUL | — | `src/tests_unit/boolean_functional.rs::brep_new_populates_tri_face_provenance` |
| 4.1-05 | `:301-303` | "The generated triangle mesh is a closed, watertight manifold under … d_ε" | Watertight by construction: shared per-edge chains, `stage1_tessellate.rs:726-734`. There is no Stage-1 closedness postcondition. Non-self-intersection comes from the self-contact guard (`stage1_tessellate.rs:386-479`, `self_contact.rs:68` `scan_self_contacts`), a substitute | PARTIAL | N5 / N61 (catch-up row P0037 "substitute") | `tests/yr7_cylinder.rs::oracle2_watertight_two_manifold`; `src/tests_unit/s1_self_contact.rs::the_driver_refines_the_cylinder_until_its_chords_clear_the_pocket` |
| 4.1-06 | `:304-305` | "convert the surface patches … into NURBS surfaces as inputs" | none (analytic `Surface` used directly) | N/A | D14 | — |
| 4.1-07 | `:306-313` | M is "r+1 planar convex polygons"; NURBS split into "rational Bézier sub-patches", each in "the convex hull defined by its control net" | none at Stage 1 | N/A | D14 | — |
| 4.1-08 | `:313-315` | Distance is "controlled by managing the distance from the control points … to P_i" (Thm A.1) | Stage 1 uses closed-form sagitta, not control points (4.1-01). The control-net certificate exists only in `stage4_dt.rs:101` `d_of_t` | DEVIATES | N5 (D14) | — |
| 4.1-09 | `:316-319` | Initial discretization is "two triangles from the four corner control points" | none (initial N computed in closed form) | N/A | D14 | — |
| 4.1-10 | `:319-321` | "If the distance … exceeds d_ε, it is subdivided into four sub-patches until the tolerance is fulfilled" | No subdivision loop. Smallest N in one pass (`stage1_tessellate.rs:617-624`); cylinder chart ≤ 3 halvings, then `Stage1ChartChordBound` STOP (`:4088`, `:4201`) | DEVIATES | N5, N61 | `src/tests_unit/s1_chart_chord_seed.rs::periodic_strip_with_dense_tongue_meets_the_chord_budget` |
| 4.1-11 | `:348-355` | Check: "project the four corners … onto the plane passing through the midpoints" to get a planar polygon (Fig. 3) | none | N/A | D14 | — |
| 4.1-12 | `:355-361` | "If the polygon is convex", control distance via control points → polygon (Thm A.2) | none | N/A | D14 | — |
| 4.1-13 | `:361-369` | "If the polygon is concave", use Thm A.1 with "one of the two generated triangles" (Fig. 4) | none at Stage 1 (`d_of_t` uses the A.1 triangle form directly, Stage 4 only) | N/A | D14 | — |
| 4.1-14 | `:371-375` | Vertices are "directly sampled from the refined … patch", so "u-v parameters for each vertex can be assigned" | `brep.rs:275` `TessellationSource`: interior Steiner `BRepFace{u,v}`; rim / chain samples `BRepEdge{edge,t}`; corners `BRepVertex`. Inverse is `brep.rs:1324` `eval_source`. Boundary samples carry edge parameters, not per-face (u,v) | FAITHFUL (representation choice SILENT) | — | `tests/yr7_cylinder.rs::oracle3_eval_source_round_trip`; `tests/yr16_cone.rs::eval_source_cone_face_arm_round_trip` |
| 4.1-15 | `:375-376` | "Each iteration … increases the mesh density by 2×" | `brep.rs:644` `refined_by` (per-face ×2, rims N·k via `edge_refine_factors` `stage1_tessellate.rs:168`), used only by §4.5.2. Stage 1's own loops are not 2×: chart-crossing jumps N to the demand (`:413-424`), self-contact asks ≈√2·N (`self_contact.rs:338`) | PARTIAL | N5 | `src/tests_unit/s452_face_refine.rs::refining_the_lateral_doubles_both_rims_and_both_caps_follow` |
| 4.1-16 | `:376-383` | Uniform (structured) refinement, not adaptive, to avoid "combinatorial problems caused by the unstructured discretization" | Rims, tube, sphere grid: structured. Cylinder chart and torus patch: unstructured Delaunay refinement (`stage1_tessellate.rs:4092` `cdt_polygon_with_holes_refined_seeded`; `patch_tessellators.rs:1818-1824` area cap) | DEVIATES | N5, N61 | — |
| 4.1-17 | `:385-389` | "guarantees to terminate since the control net converges … d_ε can be arbitrarily selected" | Ours terminates by hard caps with typed STOPs: `CHART_REFINE_ROUNDS = 4` (`stage1_tessellate.rs:355`), `SELF_CONTACT_ROUNDS = 4` (`self_contact.rs:33`), `MAX_HALVINGS = 3` (`:4088`), `FACE_REFINE_MAX = 8` (`:158`) | SILENT | — | `src/tests_unit/s1_chart_crossing.rs::the_driver_refines_the_spike_band_to_the_demand_and_tessellates` |
| 4.1-18 | `:391-395` | "select d_ε as … 10⁻²·d relative to the diagonal length d of the AABB of the B-Rep model" | Base `CHORD_BASE = 1e-2` (`normals_chord_bounds.rs:124`) is faithful. The scale is not the model AABB: `curved_chord_bound` uses the AABB of the CIRCLE RIMS only (`normals_chord_bounds.rs:234`), min-folded with each cone's own bound (`stage1_tessellate.rs:586-608`). Sphere uses 2r√3 (`:276`), torus R+r (`:293`), ellipse the major radius (`:163`). All-planar operands get `None` | DEVIATES | **N85** (ledgered 2026-10-17); D11 HISTORICAL describes the deleted legacy combined-AABB; N72 records the operand budget | `tests/yr7_cylinder.rs::oracle1_mesh_within_chord_error_of_surface` |
| 4.1-19 | `:397-401` | No integer optimization; "discretize each surface patch independently without considering its neighbors" | Not independent: one rim N per OPERAND from max radius (`stage1_tessellate.rs:617-624`); a rim takes the finer incident face's k (`:168`); intra-solid cylinder-pair phantom fold (`:636-664`); thin-band rim fold (`:665-700`); cross-operand boost (`boolean.rs:1023` `phantom_min_rim_segments`) | DEVIATES | N5 (shared rings); the two intra-solid folds are **N88** (ledgered 2026-10-17) | `src/tests_unit/m5_case_iv.rs::stage1_intra_solid_phantom_fold_densifies_rims`; `::thin_band_guard_derives_r0044_rim_density` |
| 4.1-20 | `:401-402` | "re-sample the boundary curves" | One shared sample chain per B-Rep edge, built before the faces and spliced by both incident faces (`stage1_tessellate.rs:726-744`; arc / full-ring builders follow). Line edges are endpoints only (exact) | FAITHFUL | N27 (override merge onto the chain) | `src/tests_unit/s1_azimuth_merge_unequal_rings.rs::unequal_arbitrary_rings_zip_to_a_closed_band` |
| 4.1-21 | `:404-405` | "For each surface, we first triangulate the rectangular u-v domain until reaching … d_ε" | Only the cylinder chart seeds a domain grid (`stage1_tessellate.rs:3936`, `:4092`). Torus patch: UV-CDT with area cap (`patch_tessellators.rs:1818-1824`). Tube, cone fan / frustum / strip, sphere grid, zipper (`:4777`, `:4818`): per-surface structured. Planar faces: no interior points (exact) | PARTIAL | N5, N61, N9 | `src/tests_unit/s1_chart_chord_seed.rs::periodic_strip_with_dense_tongue_meets_the_chord_budget` |
| 4.1-22 | `:405-407`, `:350` | "for each boundary curve, apply … CDT in CGAL to retriangulate the two adjacent surfaces around the boundary" | CDT (spade via `cherchi_rs::triangulation`) on non-convex / holed / curved-bounded planar faces (`stage1_tessellate.rs:2789` `tessellate_planar_cdt_face`, `:2447` `tessellate_planar_curved_cdt_face`) and holed / partial curved charts (`:3014`). Convex planar fan (`:2183`), cap fan (`:2895`), tube / zipper strips: no CDT | PARTIAL | N5, N9 (spade vs CGAL: D1 note) | `tests/nc1_nonconvex.rs::nonconvex_u_prism_exact_coverage_watertight_membership` |
| 4.1-23 | `:350-352` | "remove the trimmed area as in [Diazzi 2023] … generating a watertight mesh" | Flood-fill CDT (`stage1_tessellate.rs:2637`, `:2868`, `:3994`); orphans of the refined hull dropped (`:4104-4110`). Structured paths have nothing to trim | PARTIAL | N5 | — |
| 4.1-24 | `:354-358` | "extend our d_ε check to patches defined on triangular domains" for boundary triangle T, during the §4.1.2 retriangulation | Not run at Stage 1: chart boundary chords are only counted (`stage1_tessellate.rs:3916-3923` comment; `chart_sag_census`, `stage1_tessellate/chart_chord.rs:93`). The check (`d_of_t`) runs only in Stage 4 (`stage4_construct.rs:647,764`, `stage4_boundary_curve.rs:4537`) | ORDER (Stage 4 vs Stage 1) | N2 (N2-2 increment text) | `tests/n2_dt_adversary.rs::adv_i1_cylinder_full_turn_needle` |
| 4.1-25 | `:358-368`, Fig. 6 `:337-347` | d(T): "minimal rectangular region … that covers its three vertices", sub-patch, "maximal distance d(T) between the control points … and the triangle" | `stage4_dt.rs:101` `d_of_t`: covering rectangle, exact rational nets of the quadric (subdivided to ≤ π/2 spans) instead of NURBS subdivision, max control-point-to-triangle distance; planes → 0 | FAITHFUL (primitive) | — | `tests/n2_dt_adversary.rs::adv_i1_sphere_pole_to_pole` |
| 4.1-26 | `:370-382` | d_ε(T) = d_ε for an "inner triangle" (no boundary sampling points), d(T) otherwise | No per-triangle bound at Stage 1. Downstream readers use one operand-level budget (`normals_chord_bounds.rs:212` `owner_stage1_chord_budget`) | MISSING (operand budgets and derived bands compensate, e.g. N11) | **N93** (ledgered 2026-10-17) | — |
| 4.1-27 | `:384-390` | "The distance bounds are then used in our octree search … (Sec. 4.2.1) for a conservative intersection detection" | UNVERIFIED: the consumer belongs to the §4.2 slice. No d_ε(T) is produced to feed it (4.1-26) | MISSING | — | — |
| 4.1-28 | `:392-396` | Discretization is bijective "if the NURBS surfaces are regularly defined", so mesh results initialize the B-Rep intersection solve | `tri_face` (`brep.rs:434`) plus `TessellationSource` give Stage 3 its provenance (see N10's "durable target"); regularity is moot for analytic surfaces | FAITHFUL | N10 (provenance path) | `src/tests_unit/boolean_functional.rs::brep_new_populates_tri_face_provenance` |
| A-01 | `:1163-1166` | Thm A.1: within d_ε if every sub-patch's "control points" are within d_ε of a "planar convex polygon P_i ∈ M" | Implemented as the bound behind `stage4_dt.rs:101` `d_of_t` (triangle = convex P_i), consumed only by Stage 4. Stage 1 never certifies against it | PARTIAL | N5 (D14) | `tests/n2_dt_adversary.rs::adv_i1_cone_extreme_half_angles` |
| A-02 | `:1173-1181` | A.1 proof: the patch lies in "the convex hull of the control points", inside the d_ε envelope | `stage4_dt.rs:92-97` (doc: "convex-hull property of the positive-weight rational net … CERTIFIED upper bound") | FAITHFUL | — | `tests/n2_dt_adversary.rs::adv_i1_thin_torus_inner_equator_straddles_pi` |
| A-03 | `:1193-1209` | Thm A.2 (and proof): quadrangle w0..w3, "ω + d < d_ε", convexity condition | none | N/A | D14 | — |

**Row count:** 43 (FAITHFUL 10, PARTIAL 11, DEVIATES 8, ORDER 2, MISSING 2, SILENT 1, N/A 9 (D14 ×8, N7 ×1)). One of the eight DEVIATES rows (4.1-18) was UNLEDGERED and 4.1-19 partly (both ledgered 2026-10-17: N85, N88).

### Code with no paper row

| mechanism | where | what it does | compensates for |
|---|---|---|---|
| Chart-crossing refine loop | `stage1_tessellate.rs:355` `CHART_REFINE_ROUNDS`, `:413-434`; `stage1_tessellate/chart_crossing.rs:33` `chart_polygon_crossings` | When a face's unrolled boundary polygon self-crosses (chords of two chains interleave), the whole pass is re-run at a higher shared rim N (≤ 4 rounds) | §4.1 "mesh the untrimmed surface, then trim it with the boundary curves" (`:294-296`): the paper trims with exact curves, so chord crossings cannot arise. **N88** (ledgered 2026-10-17) |
| Self-contact guard | `stage1_tessellate.rs:386-479`; `self_contact.rs:68` | Exact tri–tri scan of the operand's own mesh. A contact halves the dominant face's bound and re-runs (≤ 4) | §4.1.1 "closed, watertight manifold under d_ε" (`:301-303`). In the plan (§2a, Phase 3); ledgered as a substitute (catch-up row P0037) |
| Intra-solid cylinder-pair phantom fold | `stage1_tessellate.rs:636-664` (`boolean/rim_junction.rs:37` `cyl_pair_phantom_n`) | Raises the operand's rim N so two of its own disjoint cylinders' chord bands cannot overlap | §4.1 d_ε certificate. **N88** (ledgered 2026-10-17) |
| Thin-band rim fold | `stage1_tessellate.rs:665-700` (`boolean/rim_junction.rs:122` `face_rim_pair_phantom_n`) | Raises N so two rims of one face don't interleave in its chart (R0044) | §4.1.2 domain triangulation + trim. **N88** (ledgered 2026-10-17) |
| Planar-neighbour chord split (P0008) | `stage1_tessellate/planar_neighbor_chords.rs:96` `split_planar_neighbor_chords` | Splits curved-chart interior chords whose two ends lie on the same planar neighbour | The missing §4.1.2 "triangulate the u-v domain first" (`:404-405`): boundary-only CDT is what creates such chords. **N88** (ledgered 2026-10-17) |
| Universal-cover chart and azimuth zipper | `stage1_tessellate.rs:3144-3210`; `:4777`, `:4818` | Chart choices for sheared bands and unequal rims | Ledgered as N5 / N61 amendments (P0038, P0044 rows); plan §2c (Phase 3) |
| Cross-operand Case-IV phantom boost | `boolean.rs:1023` `phantom_min_rim_segments` | Rebuilds both operands at a pair-derived rim N before Stage 0 | §4.3 Case IV rule-out. Plan §2a (retire with Phase 2) |
| Rim-junction insertion | `boolean.rs:1210-1270` (`rim_junction_overrides`) | Inserts exact Case-IV lobe-corner junction points as Stage-1 rim samples before the arrangement | §4.4.1 "set r_A = r_B = r" / §4.5.1 corners, done at Stage 1. Ledgered only as an N2 "partial remediation" bullet (ledger `:650-654`, whose line refs are stale); not in the plan §2 table |
| P3a conformal junction sampling + edge-in-plane conformity | `boolean.rs:1297` `junction_stage1_overrides`, `:1306-1330` | Mints each edge×face pierce point into both operands' Stage-1 meshes (edge chain + partner CDT interior) | Same §4.4.1 vertex identification, done at Stage 1. Not in the plan §2 table |
| Tangent-generator boost | `boolean.rs:1125` `tangent_generator_rim_overrides` | Mints §4.3.3 tangent rulings as standing rim samples before Stage 0 | The plan calls it faithful (r_A = r_B = r); it is at Stage 1, not §4.4.1 |
| Backtrack-spike normalization | `boolean.rs:1045` `normalized_without_backtrack_spikes` | Merges collinear backtracking LineSegment pairs in input loops | No paper basis (input repair before Stage 0/1) |
| M8-cyl forced min N | `stage1_tessellate.rs:630-635` | Forces coincident cylinders to the same rim N | §4.5.5 curved analog (Stage 0 slice) |

### Order of operations

The paper's §4.1 order (`:298-321`, `:348-407`, `:350-390`) is:
1. Triangulate each patch's u-v domain independently.
2. Iterate 2× subdivision until d_ε holds (A.1 / A.2 certificate).
3. Re-sample the boundary curves.
4. Re-triangulate around each boundary by CDT.
5. Trim (Diazzi).
6. Compute d(T) for the boundary triangles.
7. Hand the d_ε(T) bounds to the §4.2.1 octree.

The code's order in `stage1_tessellate_once` (`stage1_tessellate.rs:506`) is:
1. Choose ONE operand-wide rim N from the rim-AABB d_ε plus neighbour folds (`:575-724`). This is boundary sampling first, and it is not independent.
2. Build the shared boundary chains (`:726` ff).
3. Mint face-interior override points (`:1974` ff).
4. Triangulate each face against the fixed chains (`:2112-2400`). Only the cylinder chart seeds an interior domain grid, and the order there is boundary-constrained CDT then refinement inside it.
5. Whole-pass re-runs from outside: the chart-crossing rim-N raise and the self-contact rescan (`:395-490`).
6. d(T): never computed at Stage 1. It first appears in Stage 4's §4.4.1 rebuild (`stage4_construct.rs:647`).

Inversions:
- **Boundary before interior.** The paper resamples boundaries after the per-patch interior triangulation. We fix the boundary first and fit the interior to it. This is benign for watertightness, but it is why the neighbour folds exist.
- **d(T) moved from Stage 1 to Stage 4.** It runs after the arrangement it was meant to make conservative.
- **Junction mints at Stage 1.** In `boolean_once` (`boolean.rs:973`), §4.4.1-type vertex identification (rim-junction insertion `:1210`, P3a `:1297`, tangent boost `:1125`) runs at Stage 1, before Stage 2. The paper identifies these vertices after optimization (§4.4.1).
- **Refinement wraps the whole pipeline.** The §4.5.2 refinement (`refine_loop`, `boolean.rs:621`) re-runs Stages 0–6 and also triggers on sites found after assembly (T4 on the emitted body, T5 at Stage 5/6). Fig. 2 puts (c) before optimization and mesh updating (`:213-216`).

### Findings (ranked)

1. **d_ε's scale deviates and is unledgered.** It is `1e-2 ×` the circle-RIM AABB diagonal per operand (`normals_chord_bounds.rs:234`), with per-kind scales for sphere, torus, cone and ellipse and a cone min-fold (`stage1_tessellate.rs:586-608`). The paper uses the B-Rep model's AABB (`:391-395`), so a large box with a small hole is discretized at the hole's scale. D11 (HISTORICAL) describes the deleted legacy combined-AABB rule, and N72 names the operand budget without recording this departure. One line: ledger it, or move the base to the model AABB.
2. **d_ε(T) is missing at Stage 1, and the d_ε guarantee is not certified there** (`:354-390`, `:289-292`). Stage-1 boundary triangles carry no d(T), and chart boundary chords are only counted. The sphere's half-budget is justified only by a corpus measurement ("≈ 0.82·d_ε", `surface_face_tessellators.rs:85-92`), and cone charts have no bound at all (N61 OPEN). A faithful Theorem-A.1 primitive already exists (`stage4_dt.rs:101` `d_of_t`, exact quadric rational nets), but only Stage 4 calls it.
3. **"Discretize each patch independently" (`:400-401`) is violated by unledgered Stage-1 folds.** These are the intra-solid cylinder-pair phantom fold (`stage1_tessellate.rs:636-664`), the thin-band rim fold (`:665-700`), the chart-crossing re-run (`:355`, `:413-434`) and the P0008 planar-neighbour chord split (`planar_neighbor_chords.rs:96`). None has a ledger entry; the first two appear only in a plan §2a footnote, and the last two not at all.
4. **ORDER: exact junction vertices are minted at Stage 1** (rim-junction insertion `boolean.rs:1210`, P3a `boolean.rs:1297`, tangent boost `boolean.rs:1125`). The paper identifies vertices in §4.4.1 after optimization. These are ledgered only as an N2 "partial remediation" bullet with stale line refs, and the plan's §2 substitution table omits them. Phase 2 should list them for retirement or re-siting.
5. **Stale ledger text:**
   - N5's code location (`lib.rs:531-563` planar Newell fan) is now `stage1_tessellate.rs:2183`, and its "Remediation: closure bundled with NURBS support" contradicts plan Phase 3.
   - N9 cites `planar_outer_loop_is_nonconvex` in `lib.rs`. It is now `planar_outer_loop_fan_unsafe` (`stage1_tessellate/loop_geometry.rs:145`).
   - N27's refs `:81/:118/:358-359/:521-522` are now `:91/:330/:803-804/:966-967`.
   - D14's code location is the deleted `crates/kernel`.
   - N9 / N10 / N11 carry the boilerplate "N5 reopens if NURBS…" in their State lines.
6. **Fig. 2 / §3 ORDER.** `refine_loop` (`boolean.rs:621`) adopts failure sites read after assembly (T4 on the emitted body, T5 at Stage 5/6) and re-runs Stages 0–6. It is also capped (`FACE_REFINE_MAX = 8`), whereas the paper says "until all the failures are resolved" (`:309-312`). N62 is marked RESOLVED as faithful without recording either point.
7. **§3 per-patch classification (`:318-321`, Fig. 2e) is per arrangement cell in code** (`boolean.rs:35`). *(Corrected 2026-10-17: per Cherchi patch cut by the RAW mesh-intersection curves — `specs/yang_442_classify_after_update.md` §2; ledger N80.)* This is noted in the ledger directive (item 7) as "the N2 gap", but N2's own heading does not name it.

## Slice: §4.2 Mesh intersection, Appendix B, §5.6


Paper: `refs/text/yang2025_hybrid_boolean.txt`. §4.2 body is in the RIGHT column at :399-407. §4.2.1 is LEFT at :433-460 and continues RIGHT at :433-438. §4.2.2 is RIGHT at :440-482, continues LEFT at :498-503 after Fig. 10. §4.2.3 is LEFT at :510-518. §5.6 is LEFT at :1037-1041 and continues RIGHT at :1030-1041. App. B is LEFT at :1211-1229 and continues RIGHT at :1206-1230.

**On framing (exact vs approximate):** the paper does not compute an approximate mesh intersection. It names Cherchi 2022 / Livesu 2019 as *its* mesh-intersection method (:400-403). The approximation is in the meshes (each within d_ε of its surface), not in the intersection step. So using an EXACT arrangement (cherchi-rs) is faithful. The ledger frames it the same way: N1's "Paper section" says the labels are "products of the implementation's own exact mesh arrangement". No entry is owed for the framing. What does differ is what the arrangement's output feeds: the paper uses it as *hints/seeds* for a solve, and we use it as topology plus closed-form typing (N7).

| id | paper | statement | code | status | ledger | pin |
|---|---|---|---|---|---|---|
| 4.2-01 | `:399-403` | Detect intersections and compute intersection lines between the two triangle meshes with "[Cherchi et al. 2022; Livesu 2019]" | `crates/yang-rs/src/boolean.rs:1830` (`backend.labeled_arrangement`) → `crates/cherchi-rs/src/labeling/native.rs:109` `native_labeled_arrangement` → `crates/cherchi-rs/src/arrangements/soup.rs:691` `mesh_arrangement` | FAITHFUL (a port of the very reference the paper cites; port-level deviations ledgered) | N1 (RESOLVED), N13–N18, N24 (DELEGATED) | `crates/cherchi-rs/tests/parity_native_vs_sidecar.rs::parity_swap_corner_overlap_cubes` |
| 4.2-02 | `:403-405` | The mesh-intersection results "will be used as hints for the computation of intersections of surfaces" | `boolean.rs:2142` (`edge_provenance` from `la.intersection_edges`) → Stage-3 witness selection + closed-form `ssi_rs` (`stage3_ssi.rs:14` doc) | DEVIATES: the arrangement supplies TOPOLOGY (which edges are intersection edges) plus a closed-form curve choice. It is not an initial value for an iterative solve | N7, N10 | `crates/yang-rs/tests/yr18_attribution.rs::oracle3_provenance_admits_and_relocates_the_drifted_seam_vertex` |
| 4.2-03 | `:405-407`, `:449-451` | Because the discretization is "bounded by the error d_ε, the intersections of the surfaces can be detected conservatively" | `crates/yang-rs/src/shell_contact.rs:548` `surface_deviation` (exact per-triangle max distance to its own surface, every surface type), read at `:333` | FAITHFUL (stronger: uses each triangle's MEASURED deviation, not the global d_ε the paper assumes) | — | `shell_contact.rs` `tests::surface_deviation_is_exact_on_inscribed_triangles` (:1294) |
| 4.2-04 | `:433-445` (L) | Five-way classification (Fig. 8). "For Cases I and II, the detection of the mesh intersections provides valid results" | `boolean.rs:1830` (the arrangement's joined pairs are taken as-is); `boolean/failures.rs:376` `joined_triangle_pairs` records them | FAITHFUL | — | `tests_unit/s452_missed_intersection.rs::crossing_cylinders_mesh_intersections_are_not_near_misses_of_each_other` |
| 4.2-05 | `:443-447` (L) | "Case III [meshes miss intersections] … can be filtered by the distance threshold d_ε … Here we explain how we eliminate Case III" | `shell_contact.rs:295` `contact_pairs` + `boolean/failures.rs:492` `missed_intersection_sites`, recorded at `boolean.rs:1884` (`record_missed_intersections`, right after Stage 2) | PARTIAL: detection is at the paper's place. It admits only pairs whose exact leaf pieces CROSS by more than the model band on both sides (`shell_contact.rs:476`). Tangencies and sub-band loops are excluded by design (see 4.2-20, 5.6-01) | catch-up row P0049 ("faithful"); the band floor is **N89** item 2 (ledgered 2026-10-17) | `s452_missed_intersection.rs::a_sub_sagitta_graze_is_a_contact_pair_on_the_two_laterals` |
| 4.2-06 | `:451-460` (L) | Eq. (1): Dis(△t_AP, △t_BP) < 2d_ε; "if there are two triangles … Dis(△t_A, △t_B) < 2d_ε, this implies potential intersections" | `shell_contact.rs:333` (`reach = dev_a + dev_b + band`), `:347` (`triangle_distance`, `:151`), `:356` (`d_coarse > reach` rejected) | FAITHFUL (2d_ε generalized to dev_A + dev_B, plus the YR24 model band `:316`) | — | `shell_contact.rs` `tests::closest_pair_distance_matches_triangle_distance` (:1445) |
| 4.2-07 | `:434` (R) | "we use an octree to detect triangles that are closer than 2d_ε" | `shell_contact.rs:962` `struct Bvh` (median-split BVH), `for_each_near` called at `:327` | DEVIATES (BVH, not an octree; performance-only, same candidate set). live code ledgered as **N89** item 8 (ledgered 2026-10-17); D12 is HISTORICAL | N89 item 8; D12 (stale) | — |
| 4.2-08 | `:434-436` (R) | "For each triangle, we compute its AABB, offset the box by d_ε, and insert it into the tree" | `shell_contact.rs` `TriSet::from_parts` (`:919`, lo/hi per triangle `:942-955`); query pad `:326` = own dev + max partner dev + band; per-pair box test `:334-337` | FAITHFUL (each box is effectively offset by its own triangle's deviation, the d(T) form below applied to every triangle) | — | — |
| 4.2-09 | `:436-438` (R) | "For the triangles generated from the sampling points on the boundary curves, the offset distances become d(T)" (Fig. 6) | `shell_contact.rs:548` `surface_deviation` (per-triangle; no boundary/interior split) | FAITHFUL (generalized: every triangle uses its own exact deviation) | — | `shell_contact.rs` `tests::a_sphere_triangle_deviates_by_its_cap_sagitta` |
| 4.2-10 | `:425-431` (Fig. 7 caption) | Expand each triangle's AABB by "d_ε = 0.05d … where d is the diagonal length of the AABB" | none. The offset is per-triangle exact deviation plus `2·max(TAU_MODEL, scale·TAU_WORK)` (`shell_contact.rs:316`) | DEVIATES (low weight; the caption number is plausibly illustrative) | **N89** item 2 (ledgered 2026-10-17) | — |
| 4.2-11 | `:451-460` (L) | Paper does not say what Eq. (1) does with pairs on the SAME surface (coplanar / coaxial overlap) | `shell_contact.rs:330` skips pairs with identical `Surface` (Stage 0's §4.5.5 / coincident-cylinder population) | SILENT: our choice is to skip them, because they touch everywhere by construction | — | — |
| 4.2-12 | `:440-445` (R) | "We use the normal vectors of the areas that have potential intersections to improve the detection accuracy" | none (no Gauss-map / normal-cone code in yang-rs, kernel-v2, cherchi-rs or ssi-rs; grep "gauss", "normal cone", "half cone"). Compensated by `shell_contact.rs:431` `pieces_cross_point` (exact-surface leaf crossing) | MISSING | **N92** (ledgered 2026-10-17); D13 (HISTORICAL; cites deleted code) | — |
| 4.2-13 | `:444-446` (R) | "conservatively estimate the Gauss map of the corresponding region on each surface and check if two Gauss maps overlap" | none (see 4.2-12) | MISSING. Not covered by D14: analytic surfaces have closed-form Gauss maps (plane: a point; cylinder: a great-circle arc; cone: a small circle; sphere/torus: regions), so the filter is buildable in scope | **N92** (ledgered 2026-10-17); D13 (HISTORICAL; says "moot within analytic scope") | — |
| 4.2-14 | `:446-449` (R) | "use the control net of the rational Bézier patches … to compute the region that covers the Gauss map … as a circular half cone" (Fig. 9) | none | N/A | D14 | — |
| 4.2-15 | `:453-461` (R) | Theorem 4.1: normals lie in cone C3, axis d = a1×a2/‖a1×a2‖, angle θ = arccos(n·d), under the two-half-cone side condition | none | N/A | D14 (D13) | — |
| 4.2-16 | `:462-466` (R) | Theorem 4.1 needs non-overlapping complete cones; "checks if c1 and c2 overlap after discretizing the patches, and refines the discretization if overlap occurs" | none | N/A | D14 | — |
| 4.2-17 | `:471-473` (R) + `:498-499` (L) | Heuristic seed: "we select the barycenter of △t_A and its closest point on M_B as the potential intersection points" | `shell_contact.rs:477-484` (`ContactPair.point` = midpoint of the deciding LEAF pieces' closest points, `triangle_closest_pair` `:226`); consumed only as text in `failures.rs` site `detail` | DEVIATES: a point is computed but no solver ever starts from it | N7 | — |
| 4.2-18 | `:499-501` (L) | Seeds "are used … to find the accurate small intersection loop or a single tangential point if the intersection exists" (§4.3) | Small loop: MissedIntersection site → `boolean.rs:621` `refine_loop` (refine the named faces plus one ring, re-run the whole op). Tangent point: `boolean/tangency.rs:78` `cyl_cyl_tangent_points` / `:634` `tangent_point_face_overrides`, minted into Stage-1 meshes BEFORE tessellation (`boolean/junction.rs:1348`) | DEVIATES + ORDER: loops are resolved by §4.5.2-style refinement after a full run, not by a seeded solve in §4.3. Tangent points are solved only for cyl×cyl, and pre-emptively before Stage 1. The ledger row calls T6 "faithful … its Newton is N7", but no closed-form solve runs at a T6 site | N7; catch-up row P0049 (text does not match the code) | `crates/yang-rs/tests/tangency_pinch_split.rs::c0058_authored_geometry_union_mints_its_tangent_points`; `s452_missed_intersection.rs::a_contact_the_arrangement_joined_nowhere_is_a_missed_intersection` |
| 4.2-19 | `:501-503` (L) | "a point is regarded redundant if it falls in another accurately solved intersection curve. Such points will be directly removed" | `boolean/failures.rs:424` `RingCapture` (handed as `skip` → `shell_contact.rs:370`); coarse-intersecting pairs skipped `shell_contact.rs:348`; one site per face pair `failures.rs:511` | PARTIAL: redundancy is judged TOPOLOGICALLY (the arrangement joined a pair in the two one-rings) against mesh intersection edges, before any curve is accurately solved | — (spec `yang_452_refinement_loop.md` §3 "T6 as built") | `s452_missed_intersection.rs::a_contact_joined_within_its_one_rings_is_not_missed`, `::a_pair_the_caller_skips_is_never_refined_or_listed` |
| 4.2-20 | `:500-501` (L), `:1030-1041` (R) | The seeded population includes "a single tangential point" (tangencies, not only crossings) | `shell_contact.rs:463-476` (`straddles`: both leaves must cross the other's exact surface by > band, so a tangency never yields a site); `boolean/tangency.rs:78` (cyl×cyl only) | PARTIAL: general tangent-point detection is missing. Only cylinder×cylinder tangencies are found, and in closed form | **N91** (ledgered 2026-10-17) | `tests/tangency_pinch_split.rs::coplanar_30deg_symmetric_union_mints_its_tangent_points` |
| 4.2-21 | `:510-512` (L) | "After the mesh intersection step, each intersection curve is represented as a sequence of vertices" | `crates/cherchi-rs/src/labeled_arrangement.rs` field `intersection_edges` (`:74`), consumed `boolean.rs:2142`; chains assembled in Stage 3 (other slice) | FAITHFUL | N10 (durable target landed) | `crates/cherchi-rs/src/arrangements/soup.rs::intersection_edges_trace_the_box_box_polyline` (:1748) |
| 4.2-22 | `:513-514` (L) | "Owing to the implicit point representations used in [Cherchi et al. 2022]" the map back is direct | cherchi-rs keeps implicit LPI/TPI points internally, but `labeling/native.rs:494` `emit_vertex` rounds every vertex to f64 at the Stage-2 exit. Only `lpi_through_vertex` generator incidence survives | PARTIAL: the implicit representation does not cross the Stage-2 boundary. The I6 weld (`boolean.rs:1834`) and `weld_lpi_pencils` (`boolean.rs:524`) reconcile the rounding | — (N18 is about point identity inside the arrangement) | `tests_unit/p0014_lpi_pencil_weld.rs` (file) |
| 4.2-23 | `:514-516` (L) | "map each intersection point back to both … surfaces … by querying the triangles that intersect at that point" (face identity) | `la.source` → Stage-1 `tri_face` → `boolean.rs:2627` (N4 attribution) / `boolean/provenance.rs:103` `provenance_face_reason`; per-edge `boolean.rs:2142`. Stage 4 uses cycle-derived incidence, measured equivalent by `stage4_incidence.rs:122` `provenance_edge_incidence` (probe `stage4_correct.rs:1247`) | FAITHFUL (identity) | N4 (RESOLVED), N10 | `tests_unit/boolean_functional.rs::n4_provenance_miss_errors_loudly` (:426); `crates/cherchi-rs/src/labeling/native.rs::source_provenance_total_valid_and_consistent` (:837) |
| 4.2-24 | `:515-516` (L) | "… using barycentric coordinates" (an (u,v) on each surface per intersection point) | none: no parameter coordinates are formed. Stage 4 projects 3D points (`stage4_relocate.rs:285` `relocate_onto_implicit_pair`) | DEVIATES: N4's title says "not §4.2.3 barycentric implicit mapping", but its RESOLVED closed only face identity. The (u,v) half is unnecessary under N7 and D14, and no entry says so | N7, D14 (N4 title is misleading) | — |
| 4.2-25 | `:516-518` (L) | The two surface points from one intersection point "generally do not coincide. We further optimize" (§4.3) | `stage4_relocate.rs:285` `relocate_onto_implicit_pair` (closed form / Gauss-Newton onto the exact pair) | DEVIATES (closed-form relocation; §4.3 slice owns the detail) | N7 | — |
| 5.6-01 | `:1037-1041` (L), `:1030-1033` (R) | Extremely small loops where "the meshes … did not intersect directly", found by "the octree to determine potential mesh intersections" | T6: `shell_contact.rs:295` → `failures.rs:547` `record_missed_intersections` → `boolean.rs:621` `refine_loop` | PARTIAL: found only when deeper than the model band (`shell_contact.rs:476`), and resolved only if refinement makes the meshes meet within `REFINE_LOOP_ITER_CAP = 8` (`boolean.rs:582`) / `FACE_REFINE_MAX` | catch-up row P0049 | `s452_missed_intersection.rs::a_sub_sagitta_graze_is_a_contact_pair_on_the_two_laterals`; corpus P0049 |
| 5.6-02 | `:1033-1035` (R) | Regions are identified "based on the normal estimation and distance evaluation shown in Fig. 9" | distance: `shell_contact.rs:333-356`; normal estimation: none | PARTIAL (distance half only; exact crossing replaces the normal filter) | D13 (stale) | — |
| 5.6-03 | `:1035-1041` (R) | "our algorithm consistently produced all the intersection results" (no silent miss) | `boolean.rs` `refine_loop`: a natural body whose failure set (MissedIntersection included) survives the loop STOPs as `YangError::RefinementExhausted` (since 2026-10-16) | FAITHFUL in posture: a recorded miss never ships (the miss itself is still possible; M-3) | N66 (amended 2026-10-16) | `crates/test-harness/tests/assay_kv2.rs` smoke pin (F0064/F0067 class via P0005, R0070 `Category::Error`) |
| B-01 | `:1211-1218` (L) | At q, ∂p/∂u ∈ C2, ∂p/∂v ∈ C1. Half cones on one side of P̃ with C1∩C2 = o ⇒ v1×v2 ≠ 0, normal n = v1×v2/‖v1×v2‖ | none | N/A | D14 | — |
| B-02 | `:1218-1229` (L) | Steps (1)–(3): the normal is fixed by boundary vectors of C1, C2; the Gauss image is a great-circle segment with extremes at tangency; mirrored v1′ gives maps symmetric about d | none | N/A | D14 | — |
| B-03 | `:1206-1230` (R) | Step (4): l2² − l1² > 0 ⇒ maximal deviation at P tangent to C2 with v1 on a tangential line ⇒ cone angle θ; "Theorem 4.1 is proved" | none | N/A | D14 | — |

**Counts (31 rows):** FAITHFUL 8 · PARTIAL 6 · DEVIATES 8 · MISSING 2 · SILENT 1 · N/A 6 · ORDER 0 as a standalone status (4.2-18 carries an ORDER component).

Formerly UNLEDGERED or stale-ledgered: 4.2-07 (D12 stale), 4.2-10, 4.2-12/13 (D13 stale), 4.2-20, 5.6-03 — ledgered 2026-10-17 as N89, N92, N91; 5.6-03 resolved by F-1 (N66 amendment).

### Code with no paper row

- **`shell_contact_census`** (`crates/yang-rs/src/shell_contact.rs:1112`), called from kernel-v2 `clear_operands_set_algebra` (`crates/kernel-v2/src/boolean/mod.rs:652`, call `:665`). Runs before the mesh pipeline. It certifies that each operand shell is untouched by the other's exact surface (`pieces_may_touch` `:882`, proximity, not crossing) and orients it by generalized winding number (`winding_number` `:1089`). Kernel-v2 then answers with whole-shell set algebra. This is the N33 extension (out of paper scope; catch-up row P0030 cites §4.3.1's 2d_ε check).
- **I6 weld after Stage 2** (`crates/yang-rs/src/boolean.rs:1834-…`). It welds arrangement vertices near-aware at `TAU_WORK·(1+|c|)` for all-planar pairs and bit-exact otherwise. It compensates for f64 emission of exact implicit points (4.2-22). The paper has no step for this, because its implicit points are never rounded.
- **`weld_lpi_pencils`** (`boolean.rs:524`, called `:2127`; P0014). It fuses the pencil of LPIs minted around a sub-ULP off-plane input vertex, using `la.lpi_through_vertex`. Same compensation, no paper basis.
- **Stage-2 coplanar wall** (`crates/cherchi-rs/src/arrangements/soup.rs:741-753`, `CoplanarPairDeferred`). A loud refusal for a coplanar pair the arrangement cannot construct. This is the N17 boundary; the paper's basis is §4.5.5 (other slice).
- **Cherchi 2022 §5 in/out** (`crates/cherchi-rs/src/labeling/inside_out.rs:585` `find_ray_endpoints`, `:507` `ray_grazes_foreign_input`; N69, N19–N21). The paper's basis is §4.4.2 patch in/out (other slice). Note: N69's kill switch `CHERCHI_GRAZE_AWARE_RAY` is still live (`inside_out.rs:438-456`). The 2026-10-16 knob deletion covered yang-rs and kernel-v2 only.
- **Stage-2 broad phase** (`crates/cherchi-rs/src/arrangements/intersection_detection.rs:77` `detect_intersecting_pairs`). O(n²) pairs with AABB pruning, a documented deviation from Cherchi's upstream `cinolib::Octree` (module doc, "deviation #20"). It finds exactly-intersecting pairs, which is the arrangement's own job, not the paper's 2d_ε pass.
- **`stage4_incidence`** (`crates/yang-rs/src/stage4_incidence.rs:122`). A read-only §4.2.3 measurement instrument: cycle-derived incidence vs provenance, `cycle_unsupported` must stay 0. No production effect.

### Order of operations

Paper order (§4.2 → §4.3):
1. Mesh intersection (Cherchi).
2. 2d_ε octree near-pair detection (§4.2.1).
3. Gauss-map overlap filter (§4.2.2).
4. Barycenter + closest-point seeds; drop seeds lying on solved curves.
5. Barycentric map to both surfaces (§4.2.3).
6. §4.3 solve from seeds (small loops, tangent points), then the main curve optimization.
7. §4.5 refinement only for Case V / failures.

Code order:
1. Kernel-v2 `shell_contact_census` (whole-shell pre-check, before any yang run).
2. Stage-1 tessellation, with the cyl×cyl tangent points ALREADY minted into both meshes (`boolean/junction.rs:1348` → `tangency.rs:634`).
3. Stage 2 `labeled_arrangement` (`boolean.rs:1830`).
4. T6 near-miss census (`boolean.rs:1884`). This is the paper's §4.2.1 position, so the detection itself is in place.
5. I6 weld + LPI-pencil weld (`:1834-2127`).
6. Provenance (`:2142`, `:2627`).
7. Stages 3–6.
8. `failures::failures` (natural run) → `refine_loop` (`boolean.rs:621`): refine the named faces + ring, re-run the whole op.

Inversions:
- **Tangent-point solve runs BEFORE mesh intersection.** It is a pre-emptive closed-form mint, where the paper solves from post-intersection seeds.
- **Case-III resolution moved from §4.3 (an inline solve from seeds, before reassembly) to the post-run §4.5.2 loop.** The whole pipeline completes once on a mesh known to miss an intersection, then re-runs refined.
- **The redundancy test (4.2-19) runs before any curve is solved.** It reads the arrangement's mesh edges, where the paper reads "accurately solved intersection curve[s]".
- **The Gauss-map filter step does not exist.** The exact crossing test `pieces_cross_point` takes its place, at the same position.

### Findings (ranked)

1. **A T6 miss can ship silently, and the ledger calls T6 "faithful".**
   - The ledger row P0049 says T6 is faithful, "its Newton is N7's closed-form substitution". But no closed-form solve ever runs at a T6 site: the remedy is face refinement plus a whole-op re-run (`boolean.rs:621`). That is §4.5.2's mechanism standing in for §4.2.2/§4.3's seeded solve.
   - When the loop cannot clear the site on an emitted natural run, the natural body stands with no STOP (`boolean.rs:819-825`). That body is one T6 has certified as missing an intersection, and the paper (§5.6) claims no such miss. **UNLEDGERED** at the time; **RESOLVED 2026-10-16 by F-1** (the loud ceiling, N66 amendment). One-line remedy: a persisting MissedIntersection on the adopted body should be a loud STOP or a ledgered inventory.
2. **The §4.2.2 Gauss-map overlap filter is MISSING, and D13, its only entry, is stale.** D13 is HISTORICAL, cites deleted `crates/kernel`, and calls the filter "moot within analytic scope". That is wrong in substance: analytic surfaces have closed-form Gauss maps, so only the Bézier control-net/Theorem 4.1 machinery (4.2-14..16, App. B) is out of scope under D14. The live substitute, `pieces_cross_point` (`shell_contact.rs:431`), needs its own entry.
3. **The detection floor excludes tangencies and sub-band loops (ledgered 2026-10-17: N89 item 2, N91).** `pieces_cross_point` admits a site only when each leaf straddles the other surface by more than `2·max(TAU_MODEL, scale·TAU_WORK)` (`shell_contact.rs:463-476`). Tangent points and sub-band small loops are therefore never detected. The paper seeds both (:499-501). Tangent points exist only for cyl×cyl, minted before Stage 1 (`tangency.rs:78`). The A14.2 floor appears nowhere in `docs/yang_deviations.md`.
4. **The octree prescription has no live ledger entry.** T6 uses a BVH (`shell_contact.rs:962`). The Stage-2 broad phase is still O(n²) with AABB pruning in cherchi-rs (`intersection_detection.rs:77`). D12 is HISTORICAL and cites deleted code, but the O(n²) loop it describes survived into cherchi-rs. Performance-only.
5. **N4's title overstates what was resolved.** The §4.2.3 barycentric (u,v) map was never built. N4 "RESOLVED" covers face identity only (`boolean.rs:2627`). Separately, implicit points are rounded to f64 at Stage-2 exit (`labeling/native.rs:494`), and two welds compensate (`boolean.rs:1834`, `:524`). Under N7/D14 the (u,v) half is unnecessary, but no entry says so.
6. **Framing check, no action.** The exact Cherchi arrangement is the paper's own stated method (:400-403), and N1 frames it correctly. The port-level deviations N13–N18 and N24 remain DELEGATED debt per the 2026-10-14 directive (§5 calls them "fine as they stand").

## Slice: §4.3 Intersection optimization


Paper: `refs/text/yang2025_hybrid_boolean.txt`. Columns are de-interleaved. §4.3 opens at left :521-537. §4.3.1 starts at left :539-541 and continues at right :479-492. §4.3.2 is at right :493-516. §4.3.3 runs at right :518-537 and continues at left :597-601. §4.3.4 is at left :603-620. Appendix C is at right :1236-1252, d_p = 1e-7 at right :744-745, and the Fig. 8 cases are at left :436-447 with the caption at :471-473.
Code paths are under `crates/yang-rs/src/` unless noted otherwise.

| id | paper | statement | code | status | ledger | pin |
|---|---|---|---|---|---|---|
| 4.3-01 | `:436-447`, `:471-473` | Fig. 8 classification: Case IV, where "the meshes detect intersections that do not exist in surfaces", is left to §4.3's optimization. Case V goes to §4.5. | `boolean/rim_junction.rs:174` `phantom_min_rim_segments` (a-priori Stage-1 rim boost, called `boolean.rs:1023`); `stage4_phantom.rs:600` `certify_phantom_loops` | PARTIAL: Case IV is handled two ways, neither of them §4.3's per-point rule-out. One is a Stage-1 prediction that covers only cyl×cyl cross pairs. The other is a post-hoc certificate that covers only closed loops. | N68, N2 | `tests_unit/m5_case_iv.rs::phantom_guard_nested_disjoint_demands_finer_n` |
| 4.3-02 | `:521-530` | The surfaces may not intersect, and each mesh point "might be projected to two different points". A numeric optimization gets accurate curves and "distinguish[es] tangent points and small loops". | `stage3_ssi.rs:768` `build_intersection_curves` (closed-form `ssi_rs::intersect` `:1095`); `stage4_correct.rs:8918` `stage4_relocate_and_correct_inner` (projects each vertex onto the exact curve) | DEVIATES: closed-form SSI plus projection of a single 3D point. There is no pair of surface points and no optimization. The tangent-vs-small-loop distinction is absent (see 4.3-18..20). | N7 | `tests/yr9_stage3_ssi.rs::t1_cap_rings_carry_exact_ssi_circles` |
| 4.3-03 | `:531-535` | There are two optimization types: "Newton's root-finding method" (algebraic) and "a geometric optimization method". | `stage4_relocate.rs:285` `relocate_onto_implicit_pair`, `:367` `relocate_onto_implicit_triple` (implicit Gauss-Newton); `:23` `project_onto_circle`, `:1443` `project_onto_ellipse_nearest` (closed form) | DEVIATES: there is one implicit Gauss-Newton family, used only where no closed form exists (torus pairs, `SurfacePair`). Conic curves are projected in closed form. | N7 | `tests_unit/m5_case_iv.rs::m5_cone_pair_relocation_onto_both` |
| 4.3-04 | `:535-537`, `:744-745` | Both methods terminate "when the distance between the points is less than the given tolerance d_p" (1e-7). | `stage4_relocate.rs:315` per-surface residual floor `max(1e-13, 8·ε·max(\|x\|,anchor))`; `:96` `junction_certificate_band` | DEVIATES: termination is the residual of one point against each surface at evaluation precision. That is orders tighter than d_p, and no two-point distance is measured. | N31 (logged constants) | `tests_unit/m5_case_iv.rs::pair_newton_floor_reads_the_surface_anchor_r0032` |
| 4.3-05 | `:539-541`, `:479-480` | The approximate parameter coordinates (from §4.2.3) are "used as initial values for Newton's method". | `stage4_correct.rs:12512` (the pair Newton is seeded at the arrangement vertex's 3D position) | DEVIATES: the seed is a 3D point. There are no (u,v)/(s,t) coordinates, because all surfaces are implicit and analytic. | N7, D14 | — |
| 4.3-06 | `:480-483` | An intersection point on a boundary curve: "we restrict it to move along the curve". | `stage4_relocate.rs:367` `relocate_onto_implicit_triple` (the operand edge's two faces plus the partner surface); closed forms such as `:1562` `pp_line_circle_junction`; called `stage4_correct.rs:10345` | FAITHFUL: a three-surface junction solve is motion along the boundary curve. The root is domain-checked since N66. | N31, N66 | `tests_unit/s4_torus_conic_corner.rs::r0050_torus_conic_corner_relocates_onto_all_three_within_the_curve_corridor` |
| 4.3-07 | `:1236-1252` (App. C) | Newton on D=s1−s2 takes a min-norm step: "∇D∇Dᵀa_k = s2−s1, x_{k+1}=x_k+∇Dᵀa_k". | `stage4_relocate.rs:285` `relocate_onto_implicit_pair` (dx = −Jᵀ(JJᵀ)⁻¹f with unit-normal rows) | FAITHFUL in form. The unknowns are 3D x on implicit F0,F1 rather than (u,v,s,t). | N7 | `stage1_tessellate/torus_patch_tests.rs` (pair converges); `tests_unit/m5_case_iv.rs::m5_steep_cone_pair_relocation_converges` |
| 4.3-08 | `:485-492` | Newton is used "only for tangential points and points gliding on boundary curves". The geometric method handles generic points. | `stage4_relocate.rs:340` (pair Newton returns `None`, i.e. STOP, when `det ≤ MIN_FEATURE_SIZE²`) | DEVIATES: the method choice is inverted at tangency. Our Newton refuses tangent configurations and is used for generic torus/pair points. | N7 (records the tangent distinction as "not implemented") | `tests_unit/m5_case_iv.rs::m5_surface_pair_tangent_none_at_tangency` |
| 4.3-09 | `:493-500` | Geometric method: "linearize the two surfaces … as two tangent planes", then project the point "onto the intersection line L_k = P_A ∩ P_B". | `stage4_relocate.rs:285` `relocate_onto_implicit_pair` (the min-norm step on unit-normal distance residuals is the orthogonal projection onto the line of the two linearized planes) | FAITHFUL (implicit, single-point form). It covers only non-conic pairs; conics use closed form (N7). | N7 | `tests_unit/m5_case_iv.rs::m5_cone_pair_relocation_onto_both` |
| 4.3-10 | `:501-503` | The geometric method "is used only when the two tangent planes are not parallel". | `stage4_relocate.rs:340` (pair `det ≤ MIN_FEATURE_SIZE²`), `:408` (triple `\|det\| ≤ MIN_FEATURE_SIZE`) | FAITHFUL. The parallel threshold is our choice; the paper is silent on it. | — | `tests_unit/m5_case_iv.rs::m5_surface_pair_tangent_none_at_tangency` |
| 4.3-11 | `:521-526` | If the meshes do not intersect "but are within the distance tolerance d_ε", there is a tangent point, a small loop, or nothing. "We use Newton's method to solve such cases." | `boolean/failures.rs:547` `record_missed_intersections` (T6, called `boolean.rs:1884` after Stage 2); `shell_contact.rs:295` `contact_pairs`; response is `boolean.rs:621` `refine_loop` | DEVIATES: detection matches the paper (only for contacts whose exact surfaces CROSS). The response is §4.5.2 refinement until the arrangement captures the crossing, not a Newton solve. Tangent and no-crossing contacts are not resolved. | N7 (ledger directive item 7 calls T6 "faithful in mechanism") | `tests_unit/s452_missed_intersection.rs::a_contact_the_arrangement_joined_nowhere_is_a_missed_intersection` |
| 4.3-12 | `:492-494` (Fig. 10c), `:521-526` | Seeds come from "the triangle T … nearest to the right one … its centroid p, and … the nearest point q on the right surface". | none. T6 measures exact deviation (`shell_contact.rs:548` `surface_deviation`) and solves nothing from a seed. | MISSING. Refinement compensates (4.3-11). | N7 | — |
| 4.3-13 | `:526-527` | When the point lies on an intersection loop between the two meshes, "we use the geometric method". | `stage4_correct.rs:11340` (`project_onto_circle`), `:12512` (pair Newton for torus/pair edges), inside `stage4_relocate_and_correct_inner` | DEVIATES: closed-form projection onto the Stage-3 curve for conics. The geometric step (4.3-09) is used only for torus and `SurfacePair` edges. | N7 | `tests/yr10_stage4_relocate.rs::t1_relocate_on_curve_chord_decreases_watertight` |
| 4.3-14 | `:527-531` | For both methods, "if there is no solution in one of the two parametric domains, we regard it as a solving failure". | `stage3_ssi.rs:1367` `AmbiguousCurve` (no candidate curve) → `stage4_correct.rs:1299` `note_ssi_failure` (T1); `stage4_phantom.rs:359` `classify_claims` (the vertex's own root must lie inside the edge segment) | PARTIAL: the domain test exists only for a straight operand edge × surface. A claim on a CURVED edge is declined (`CurvedEdge`), and there is no test against the pierced face's trimmed domain. | N65, N68 | `stage4_phantom.rs::tests::own_root_decides_not_the_edge`; `tests_unit/s452_failure_sites.rs::a_stage3_solving_failure_names_the_faces_on_its_edge_only_when_the_run_ends_there` |
| 4.3-15 | `:529-531` | Failure "rule[s] out … Case IV where the meshes detect intersections that do not exist", i.e. the phantom points are removed. | `stage4_phantom.rs:600` `certify_phantom_loops` / `:674` `closed_refuted_components` → STOP `PhantomIntersectionLoop` `stage4_correct.rs:14274` → refine loop; a-priori `boolean/rim_junction.rs:174` `phantom_min_rim_segments` | DEVIATES: only a CLOSED, cyclic, all-refuted component is acted on, and the action is STOP-and-refine, not removal. Our labels are per arrangement cell, so a phantom bump on a real curve still rides through. | N68 (index says RESOLVED; entry body says OPEN), N2 | `stage4_phantom.rs::tests::refuted_chain_is_not_a_loop`; `stage4_phantom.rs::tests::isolated_refuted_triangle_is_certified` |
| 4.3-16 | `:531-534` | "If the distance … converges to zero, an accurate intersection point is found that lies on both S_A and S_B." | `stage4_relocate.rs:285` (returns only when both residuals are ≤ floor); `:96` `junction_certificate_band` | FAITHFUL: the relocated vertex is on every carried surface. Conics are exact by closed form. | N7 | `tests_unit/m5_case_iv.rs::m5_cone_pair_relocation_onto_both` |
| 4.3-17 | `:535-536` | "During optimization, we remove a point if it is too close to another point on the same loop." | `stage4_correct.rs:2025` `weld_coincident_relocated` (called `:12975`, `:13325`, after the §4.5.3 sweep); `:3818` `collapse_subresolution_intersection_segments` (called `stage5_topology.rs:4925`, after the §4.4.1 construct/fold-merge passes) | ORDER: these run after relocation, §4.5.3 and §4.4.1, not during optimization. The paper does not define "too close"; we use `TAU_MODEL·(1+scale)` (= d_p) with carried-surface containment. | N47, N56 | `tests_unit/n47_moved_weld.rs::coincident_pair_on_distinct_surfaces_is_not_welded`; `tests_unit/m5_case_iv.rs::kv15b_subresolution_intersection_segment_collapses` |
| 4.3-18 | `:536-539`, `:597` | One isolated point, or one point left after removal, is "either a tangent point or a point on a small loop". | none. There is no single-surviving-point census; `ssi_rs::intersect` (`crates/ssi-rs/src/lib.rs:404`) returns an empty Vec for tangent pairs. | MISSING | N7 | — |
| 4.3-19 | `:597-598` | "Compare the normal vectors … If the two normal vectors are collinear, it is a tangent point." | `boolean/tangency.rs:78` `cyl_cyl_tangent_points` (+ generator form `:161`), minted into both Stage-1 meshes via `:634` `tangent_point_face_overrides` (called `boolean/junction.rs:1348`) | PARTIAL + ORDER: closed-form collinear-normal identity for cylinder×cylinder only, at a TAU_WORK band. It runs at Stage 1, before the arrangement, not on an optimized point. Other tangent pairs STOP (4.3-08). | N2 (2026-09-13 update), N7 | `tests_unit/s433_tangent_relocation.rs::s433_equal_radius_crossing_axes_give_two_tangent_points` |
| 4.3-20 | `:598-601` | "Otherwise, it is a point that belongs to a loop … the tracing method in [Bajaj et al. 1988] … use this point as a seed to find the whole loop." | none. Closed-form SSI supplies whole conics. A `SurfacePair` loop is relocated pointwise. A sub-resolution loop appears only after T6 + §4.5.2 refinement make the meshes cross. | MISSING (compensated) | N7 | — |
| 4.3-21 | `:603-607` | The optimized polylines "may be uneven and sparse", so "a curvature-based refinement method … subdivide[s] the intersection lines". | `stage4_construct.rs:2777` `refine_conic_chain` (called `stage5_topology.rs:3285` in `run_construct_passes`) | PARTIAL: periodic conics (Circle/Ellipse) only. Hyperbola, Parabola and `SurfacePair` chains are never refined. Above `I5_INSERT_CAP`=4096 (`stage5_topology.rs:3088`) the pass declines to reorder-only. It also runs inside §4.4.1, not before it. | N58; N7 (text says §4.3.4 "not implemented", which is stale) | `tests_unit/i5_seam_density.rs::refine_densifies_a_short_arc_to_the_paper_criterion` |
| 4.3-22 | `:607-611` | "Use the midpoints of their parameter domains as the initial values for optimization"; the result is m between p and q. | `stage4_construct.rs:2843` `collect_inserts_rec` (`conic_eval` at the curve-parameter midpoint) | DEVIATES: m is the exact curve evaluation at the curve-parameter midpoint. It is not optimized from the midpoints of the surfaces' (u,v)/(s,t). | N7 (not ledgered for §4.3.4 specifically) | `tests_unit/i5_seam_density.rs::conic_eval_round_trips_conic_param_on_a_tilted_ellipse` |
| 4.3-23 | `:611-614` | Compute "the arc height h", "the angle α between … pm and mq", and "l = max(‖pm‖, ‖mq‖)". | `stage4_correct.rs:2461` `paper_chain_metrics` | FAITHFUL | N58 | `tests_unit/i5_seam_density.rs::metrics_agree_with_the_n58_predicate` |
| 4.3-24 | `:614-617`, `:744-745` | Stop when "h < d_p×10², l < d_p×10³, α < π/18", with d_p = 1e-7. | `stage4_correct.rs:2429` `paper_chain_sample_redundant` | FAITHFUL, with d_p scale-relativized to `TAU_MODEL·(1+scale)`. A degenerate leg counts as redundant. | N58 | `tests_unit/i5_seam_density.rs::census_accepts_a_paper_dense_chain` |
| 4.3-25 | `:617-620` | "Otherwise, we continue to insert midpoints between pm and mq." | `stage4_construct.rs:2843` `collect_inserts_rec` (recursion; `I5_SIM_DEPTH_MAX`=32 `:2680` plus the insert budget) | FAITHFUL. The depth cap and the budget cap are our choices; on exhaustion the shipped coarse chain is kept and the event is logged. | N58 | `tests_unit/i5_seam_density.rs::refine_declines_over_budget_and_skips_a_dense_chain` |

Row count: 25. FAITHFUL 8 (06, 07, 09, 10, 16, 23, 24, 25). PARTIAL 4 (01, 14, 19, 21). DEVIATES 9 (02, 03, 04, 05, 08, 11, 13, 15, 22). ORDER 1 (17). MISSING 3 (12, 18, 20). SILENT 0 as a standalone status (the paper-silent choices are noted inside rows 10, 17 and 25). N/A 0.

### Code with no paper row

- `stage4_correct.rs:21` `tangent_plane_corridor` and its `OffCurveBeyondChordBand` displacement gates (e.g. `:10405`, `:11384`) accept a relocation only if it moves less than `2·d_ε/sinθ`. The paper's §4.3 acceptance is convergence plus the domain test; it has no displacement gate. The gate compensates for the missing per-solution domain rule-out (4.3-14/15) and feeds the §4.5.2 loop. It is logged as a tolerance decision under N29 (ledger :2184).
- Stage-3 branch selection (`stage3_ssi.rs` ~`:880-:1000` on-both-surfaces gate; ~`:1210-:1320` tangent-alignment, disjoint-parallel-line and distance tie-breaks) picks which closed-form SSI component a mesh edge lies on. This stands in for the paper's seeded tracing (4.3-20), where the seed defines the branch. Ledgered N10, N38, N39, N45, N46.
- The Stage-3 provenance "witness" override (`stage3_ssi.rs` ~`:978`, `(0, None)` arm at `:1335`) admits producer-confirmed edges whose endpoints drifted off a surface. It is arrangement bookkeeping with no paper basis.
- `stage4_correct.rs:3983` `collapse_subtauwork_mesh_edges` (called `:14012` and `stage5_topology.rs:4963`) collapses ANY mesh edge shorter than `TAU_WORK·(1+scale)`. Closest paper statement: the §4.3 close-point removal or §4.4.1(b) merge (the plan maps it to §4.4.1(b)). Was UNLEDGERED; **N89** item 1 (ledgered 2026-10-17).
- `stage5_chain_decimate.rs:251` `decimate_intersection_runs` applies the §4.3.4 acceptance test in REVERSE, dropping output intersection-run vertices. Class (b) also drops straight operand-edge split points at TAU_WORK. Ledgered as the N58 2026-09-22 addendum. This is post-hoc removal; the paper only inserts.
- `stage5_seam_merge.rs:498` `merge_conic_seam_runs` (coalesces conic runs into arcs) and `stage5_output_refine.rs:569` `restore_carried_edge_curves` (re-types carried chords onto input curves) are both §4.4.2 output restoration, not §4.3 (N60). They live in this slice's files but belong to §4.4.2.
- `stage4_relocate.rs:2064` `remove_doubled_membranes` (N41), `:2256` `split_pinch_vertices` (N74/N78) and `:2385` `check_watertight_2manifold` live in this slice's file but belong to §4.4.3.
- `stage4_phantom.rs:531` `census_case_iv_phantom` is a print-only probe (`YANG_433_PHANTOM`).

### Order of operations

The paper's order is:
1. §4.2 mesh intersection.
2. §4.3 optimization. Each point is solved by Newton or the geometric method; a domain failure rules out Case IV; too-close points are removed *during* optimization; a single survivor goes to the normal test (tangent point or tracing seed).
3. §4.3.4 refinement of the polyline.
4. §4.4.1 mesh update.
5. §4.4.2 inside/outside classification.
6. §4.5 failure handling.

The code's order is:
1. Stage 1: the Case-IV phantom boost (`boolean.rs:1023`) and the cyl×cyl tangent-point mint (`boolean/junction.rs:1348` → `tangency.rs:634`).
2. Stage 2: the arrangement (`boolean.rs:1829`), then T6 (`:1884`), then **in/out classification `la.keep_set(op)` (`:2131`)**.
3. Topology reconstruction (`:3020` → `stage5_topology.rs:4504`):
   1. `compute_phase_a` runs Stage-3 SSI typing (`:4527`).
   2. `stage4_relocate_and_correct` (`:4730`) runs, in order:
      1. relocation (conic closed form `stage4_correct.rs:11193+`, torus/pair Newton `:12360+`, triple junctions `:10345`);
      2. the §4.5.3 sweep (`:12772`);
      3. N47 weld (`:12975`, `:13325`);
      4. §4.4.1(b) merge (`:13076`);
      5. §4.4.1 per-patch re-CDT (`:13338`);
      6. §4.4.1(a) split (`:13400`);
      7. subtauwork collapse (`:14012`);
      8. watertight gate (`:14017`);
      9. **Case-IV rule-out (`:14274`)**.
   3. **§4.3.4 refinement inside the construct passes (`stage5_topology.rs:4810` → `:3285`)**, then fold merge.
   4. **KV15b §4.3 collapse (`:4925`)**.
   5. `emit_topology`, then the Stage-5 restoration, seam merge and §4.3.4-criterion decimation (`:6740-6796`).

Inversions:
1. Classification (§4.4.2) precedes all §4.3 work. The paper classifies after trimming; we relocate the already-kept submesh. This is the "labels per arrangement cell" gap (N2).
2. The Case-IV rule-out runs last in Stage 4, after §4.5.3 and §4.4.1, instead of during optimization.
3. The §4.3 close-point removal runs after §4.5.3 (N47) and after §4.4.1 (N56), instead of during optimization.
4. §4.3.4 refinement runs inside the §4.4.1 construct pass, after the per-patch re-CDT, instead of before §4.4.1.
5. Tangent-point detection runs at Stage 1, before the arrangement, instead of after optimization.

### Findings (ranked)

1. **N68's index row contradicts both the code and its own entry.** The status index (`docs/yang_deviations.md:168`) says RESOLVED by "the LOCAL-form §4.3.3 density guard is always-on". That guard (`edge_graze_min_rim_segments` / `YANG_433_GUARD`) was deleted on 2026-10-14 and has no hits in `src`. The entry body (`:385-430`) still says "State: OPEN". Layers 1 and 2 of the gap are live: the rule-out acts only on closed components (`stage4_phantom.rs:674`), and curved-edge claims are declined. The remedy is STOP-and-refine, not the paper's per-solution removal (`:529-531`). Re-adjudicate N68.
2. **ORDER: all of §4.3 runs on the post-classification kept submesh.** `boolean.rs:2131` `keep_set` runs before `:3020` relocation. Inside Stage 4, the Case-IV rule-out (`stage4_correct.rs:14274`) and the §4.3 point removals (`:12975`, `stage5_topology.rs:4925`) run after the §4.5.3 and §4.4.1 passes. The paper does all three during optimization (`:527-536`). Only N2 implicitly covers the classification order; the removal/rule-out order is not recorded as an inversion.
3. **The tangency method choice is inverted and largely MISSING.** The paper uses Newton exactly for tangential points (`:489-492`). Our pair Newton refuses tangency (`stage4_relocate.rs:340` returns `None`), and `ssi_rs::intersect` returns nothing for tangent pairs. Tangent points exist only for cylinder×cylinder, via a Stage-1 closed-form mint (`tangency.rs:78`). The single-point census and the normal test (`:536-598`) are absent for every other pair. N7 waves this off as "open for general analytic pairs".
4. **Small-loop tracing (Bajaj, `:598-601`) and the Fig. 10(c) seeding are MISSING.** T6 plus §4.5.2 refinement replaces them. Ledger item 7 calls T6 "faithful in mechanism" under N7, but the paper's response is solve-and-trace, not refine-until-the-meshes-cross. Contacts that touch without crossing are never resolved.
5. **§4.3.4 refinement is partial and N7's text is stale.** It covers only periodic conics (`stage4_construct.rs:2777`), is capped at 4096 inserts, and evaluates the exact curve at the midpoint of the curve parameter rather than optimizing from the surfaces' parameter midpoints. N7 (`:1035-1055`) still says §4.3.4 is "not implemented" and points at `lib.rs:1316-1550`; that code now lives in `stage3_ssi.rs`. Hyperbola, Parabola and `SurfacePair` chains are never refined.
6. **UNLEDGERED: `collapse_subtauwork_mesh_edges`** (`stage4_correct.rs:3983`, the #194 TAU_WORK edge collapse over all mesh edges) had no ledger entry — now **N89** item 1 (ledgered 2026-10-17).
7. **Stale ledger and spec details.** The N31 tolerance note (`docs/yang_deviations.md:2338`) says `relocate_onto_implicit_pair` uses a flat 1e-13 floor; the code now uses `max(1e-13, 8ε·max(|x|, anchor))` (`stage4_relocate.rs:315`). The plan's §2b line cites for the three collapses (`stage4_correct.rs:2797/:4634/:4799`) are now `:2025/:3818/:3983`.

## Slice: §4.4 Booleans (§4.4.1–§4.4.3)


Paper cites are `refs/text/yang2025_hybrid_boolean.txt` lines. Page 8 layout: §4.4 intro and the head of §4.4.1 are in the LEFT column at `:595-611`. §4.4.1 continues at the top of the RIGHT column `:541-571`. The Fig. 11 caption is in the LEFT column at `:551-563`. §4.4.2 is right column `:574-598`; §4.4.3 is right column `:599-605`.
Code paths are relative to `crates/`. Every `path:line` below was opened during this audit.

| id | paper | statement | code | status | ledger | pin |
|---|---|---|---|---|---|---|
| 4.4-01 | `:595-600` | Do NOT classify B-Rep patches by casting a ray per patch. "numerical problems are easily encountered, leading to incorrect classifications" | No B-Rep-level classifier exists. Keep/discard comes from the mesh arrangement's labels: `yang-rs/src/boolean.rs:2131` `la.keep_set(op)` | FAITHFUL | — | `cherchi-rs/src/labeled_arrangement.rs::tests::keep_set_union_is_all_false_inside_tris` |
| 4.4-02 | `:600-604` | "leverage the inherent watertightness of the mesh Boolean"; results are "mapped back to the B-Rep model using the parametric and topology correspondence" | Per-triangle provenance goes through the input face map: `yang-rs/src/boolean.rs:2644` `provenance_face_reason`. A geometric fallback runs only for lineage-less input (`boolean.rs:~2950` tiered `narrow`) | FAITHFUL | N4 (RESOLVED), N12 | — |
| 4.4-03 | `:605-611`,`:541-543` | Relocation breaks bijectivity ("gaps or self-intersections", d_ε lost). "Therefore … we trim and update the meshes using the intersection curves" | `yang-rs/src/stage4_construct.rs:3113` `update_patches`, called at `stage4_correct.rs:13338`. It only runs when Stage 4 is entered (`stage5_topology.rs:4729` `if has_conic`). It skips patches that have no chart (`:3156`), more than one attribution (`:3166`), a refused plan (`:3178`) or a changed boundary (`:3197`) | PARTIAL: about 85 % of intersected patches; the rest keep relocate-in-place. Since 2026-10-17 a crossing-cycle refusal records a T3 site (F-2) | N2 (OPEN) | — |
| 4.4-04 | `:543-547` | Intersection curves on the surfaces are "mapped to the meshes M_A and M_B by mapping each intersection point r to r_A ∈ M_A and r_B ∈ M_B" | The points mapped are the arrangement's MESH-crossing vertices, each relocated onto the exact curve: `stage4_correct.rs:11193` "(2) Relocate / retag every endpoint", writes e.g. `:11343` `mesh.verts[v] = proj`. The curve's own refined (§4.3.4) samples are inserted later, in the construct pass (`stage5_topology.rs:3068` `run_construct_passes`, I5 insert) | PARTIAL: relocation of mesh points, not insertion of the curve's polyline | N2, N7 | — |
| 4.4-05 | `:547-549` | "Then we set r_A = r_B = r, so that the two polylines in the meshes coincide with the intersection curve" | Holds by construction: the Stage-2 arrangement gives both operands' kept triangles ONE shared vertex index per crossing, and relocation moves that vertex once (`stage4_correct.rs:11193`ff) | FAITHFUL | — | — |
| 4.4-06 | `:547-549` | r_A = r_B = r applied to an isolated TANGENT point | The exact tangent point is minted into BOTH operands' Stage-1 meshes: `yang-rs/src/boolean/tangency.rs:634` `tangent_point_face_overrides`, called from `boolean/junction.rs:1348` | ORDER: runs at Stage 1, before the arrangement, instead of at §4.4.1 after optimization | N2 (2026-09-13 update) | `yang-rs/tests/tangency_pinch_split.rs::c0058_authored_geometry_union_mints_its_tangent_points` |
| 4.4-07 | `:552-553`, Fig.11 `:560` | "through CDT we obtain valid discretizations of the trimmed meshes"; the polyline becomes "the boundary of the trimmed triangle meshes" | `stage4_construct.rs:484` `rebuild_patch_planar_bounded`, which makes the CDT call at `:675` `cdt_with_interior_constraints` with the patch cycles as constraints | PARTIAL: refusals (ChordDegradation 2,312; TriangulationFailed 226; DegenerateInput 84; torus/sphere 83; ThetaUnwrap 7 — spec §6/§9) leave relocated triangles in place | N2 | `stage4_construct.rs::tests::rebuild_patch_planar_keeps_boundary_edges_and_drops_interiors` |
| 4.4-08 | `:553-555` | "The generated trimmed meshes are still bijective to their corresponding surfaces" | Attribution is swapped in lockstep with the triangles: `stage4_construct.rs:2582` `apply_rebuild_batch`. Seed points are lifted exactly onto the surface (`chart.lift`, `:794`) | FAITHFUL | — | `stage4_construct.rs::tests::apply_rebuild_batch_swaps_tris_and_keeps_attribution_lockstep` |
| 4.4-09 | `:555-561` | "The triangulation can be totally operated in the parametric domain" | `yang-rs/src/stage4_project.rs:192` `SurfaceChart::supports` covers Plane, Cylinder and Cone only. Bands are cut open by `stage4_project.rs:555` `band_patch_from_cycles`. A cone apex is refused with `ApexInPatch` (`stage4_construct.rs:536`) | PARTIAL: no whole-patch sphere or torus chart; no chart through a cone apex | N2 | `stage4_project.rs::tests::a_cylinder_band_cuts_open_into_one_simple_polygon` |
| 4.4-10 | `:561` | The update "maps boundary curves to boundary curves" | Postcondition `stage4_construct.rs:3068` `plan_keeps_boundary`: the directed boundary is unchanged (with multiplicity). Checked in `update_patches` (`:3197`) | FAITHFUL (enforced as a postcondition) | N2 | `stage4_construct.rs::tests::a_closed_band_rebuilds_with_its_boundary_unchanged` |
| 4.4-11 | `:561-562` | The triangulation "contains no flipping triangles since the intersection curves are regular" | Orientation is set by area vector, or for a band by boundary sense (`stage4_construct.rs:705`). There is a no-repeated-vertex and no-doubled-directed-edge check (`:3068`). Cycles that cross in the chart make the CDT refuse | PARTIAL: flip-free only for accepted plans. Refused patches keep relocated triangles that can fold, which `repair_fan_cluster` and fold-merge then mend after the fact | N2, N58 | `stage4_construct.rs::tests::directed_boundary_of_a_fan_and_its_retriangulation` |
| 4.4-12 | Fig.11(a) `:556-557` | "We locate the constrained edge containing q (the red edge) and split it using q" | The Stage-2 arrangement splits at the MESH crossing, then relocation moves that vertex onto q. The block named "(3d) §4.4.1(a) edge-split" (`stage4_correct.rs:13400`) actually unzips collinear degenerate relocated triangles into their long-edge neighbour | DEVIATES: split at the mesh point plus post-hoc unzip, instead of a split at q | N2 (spec 2b inc-2 lists the unzip for retirement) | — |
| 4.4-13 | Fig.11(b) `:557-559` | "If an endpoint p of the split edge is too close to q, we merge p with q … to improve the mesh quality" | The N55 merge (`stage4_correct.rs:13076`) merges only numerically coincident pairs, `TAU_WORK·(1+scale)` (`:13215` `is_relocation_coincidence`). The fold-merge pass (`stage5_topology.rs:572` `run_fold_merge_passes`) merges boundary vertices that a relocation OVERRAN, using a local fan re-CDT | DEVIATES: no quality-proximity threshold. "Too close" is read as same-point, plus a fold repair | N55 | `stage4_construct.rs::tests::rebuild_merge_fan_replaces_only_the_victim_fan` |
| 4.4-14 | Fig.11(c) `:559-560`; `:564-567` | "If there are no other mesh vertices within an intersection loop, we insert one point i" "to ensure that the topology of the trimmed mesh patch aligns with the trimmed surface" | No live code. The N2-1 `insert` primitive in `stage4_update` was deleted in Phase 0; `stage4_update.rs` now holds only `Patch`. Spec 2b §3 says the step is "subsumed by d(T) seeding", but I2e seeding (`stage4_construct.rs:882` `i2e_seed_grid`) fires only on a curved `ChordDegradation` retry (`:807`) | MISSING. Compensated: the Stage-2 arrangement already triangulates every loop interior | N2 | — |
| 4.4-15 | `:563-564` | "we remove a mesh vertex if it is too close to the intersection curve on the mesh" | Planar patches: every interior vertex is dropped (no `theta_span`, `stage4_construct.rs:545`). Curved patches: an interior is dropped only when it lies EXACTLY on a boundary segment (`:568` `on_boundary_edge` → `:239` `point_on_segment_interior`, 1e-9 relative collinearity). No distance-to-curve test exists | PARTIAL / SILENT threshold. Our choice is exact collinearity | N2 (its text says "near-boundary interior removal", which overstates the code) | `stage4_construct.rs::tests::on_segment_interior_takes_inside_and_refuses_beyond_and_off` |
| 4.4-16 | `:569-571` | "For the newly generated boundary triangles around the intersection curve, we recalculate d(T) to maintain controllable error" | `yang-rs/src/stage4_dt.rs` `d_of_t` certifies every new triangle (`stage4_construct.rs:756`). The budget is `min(old max, d_ε)` (`:657`), where d_ε is the op-wide `stage4_chord_band` (`stage4_correct.rs:1587`, the max of both inputs' curved bounds). On failure there is one seeded retry (`:807`), then refusal | PARTIAL: a failed budget means the patch keeps its old triangles. It does not refine the patch, and the budget is op-wide rather than per patch | N2 | `yang-rs/tests/n2_dt_adversary.rs::adv_i1_cylinder_full_turn_needle`; `stage4_construct.rs::tests::rebuild_cylinder_chord_gate_seeds_wide_theta_rebuild` |
| 4.4-17 | `:541-543` + `:576-579` | Order: the meshes M_A and M_B are trimmed and updated FIRST; "After trimming the meshes … we directly apply" the in/out classification | Code order is the reverse. Classification and keep come first (`boolean.rs:2131` `keep_set`). Then Stage 4 and the update run only on the kept submesh (`boolean.rs:3020` → `stage5_topology.rs:4504`), so discarded triangles are never updated | ORDER: §4.4.2 classification runs before §4.4.1 | N2 implies it ("trusts the sidecar-trimmed mesh") but never names the order inversion | — |
| 4.4-18 | `:576-579` | "apply a standard inside/outside classification step [Cherchi et al. 2022] to identify the triangles that need to be retained" | `cherchi-rs/src/labeling/inside_out.rs:143` `compute_inside_out` (exact ray cast per arrangement patch); `labeled_arrangement.rs:113` `keep_set`; `yang-rs/src/boolean.rs:35` `flip_for_op` | FAITHFUL (the mechanism; its position is row 17) | N19, N20, N21, N23, N69 (port-level) | `cherchi-rs/src/labeled_arrangement.rs::tests::keep_set_subtraction_matches_hand_derived_indices` |
| 4.4-19 | `:581-583` | The B-Rep result is "a collection of parameter surfaces and their boundary curves in the parametric domain" | `yang-rs/src/stage5_topology.rs:5150` `emit_topology`: faces carry the inherited analytic `Surface`, and edges carry a 3D `Curve`. No parametric-domain (p-)curves exist | DEVIATES: 3D typed curves only | N7 / D14 cover the curve source; the absence of p-curves is unledgered | — |
| 4.4-20 | `:583-585` | "Instead of performing inside/outside classification on the B-Rep patches, we leverage the bijection to locate the surface patches retained using a mesh segmentation strategy" | Kept triangles carry their input face (`boolean.rs:2644`) and are grouped by flood fill (row 21) | FAITHFUL | N4 | — |
| 4.4-21 | `:586-587` | "segments the mesh Boolean results into patches along the boundary curves, which correspond to either the original boundary curves or the intersection curves" | `stage5_topology.rs:6936` `flood_fill_patches`: a patch is the connected set of triangles with equal `(input, face)` attribution, so a patch edge is exactly a face or intersection boundary. Then `stage4_correct.rs:1339` `merge_same_plane_patches` fuses adjacent same-plane patches (see Code with no paper row) | FAITHFUL (an equivalent criterion) | — | — |
| 4.4-22 | `:587-592` | "Starting from an inner triangle, i.e. not on the boundaries … expands the patch by including more neighboring inner triangles, until all the neighboring triangles of the patch are on the boundaries" | `stage5_topology.rs:6943-6975`: BFS seeded at the lowest unvisited index (any triangle), stopping at an attribution change | SILENT: our seed is the lowest index. The partition is identical | — | — |
| 4.4-23 | `:592-594` | Boundary curves "collected and mapped back to the parametric surfaces by fitting the curve in the parametric domain" | Collected by `stage5_topology.rs:6991` `patch_boundary_cycle` (N22 fold-sliver exclusion at `:7020`). Curves are attached by IDENTITY from the Stage-3 closed-form `intersection_curves` map, oriented by `:5090` `orient_directed_curve`. Then: §4.4.2 restoration `:6740` `restore_carried_edge_curves`, seam-run merge `:6768`, chain decimation `:6796`; and in kernel-v2, `kernel-v2/src/recover.rs:208` `recover_output_curves` (retag + fuse) | DEVIATES: no fitting. Exact analytic curves are assigned, and polyline runs are coalesced after emission | N7, N60, N58 (addendum), N32, N22 | `yang-rs/src/tests_unit/s434_output_restore.rs::carried_rim_chord_typed_on_both_copies` |
| 4.4-24 | `:594-595` | "All the patches are found if all the triangles in the mesh Boolean results are accessed" | `stage5_topology.rs:6943` loops over every triangle index. A `None` attribution is skipped, but the N4 retirement makes attribution total for lineage inputs (`boolean.rs:2644`ff loud miss) | FAITHFUL | N4 | — |
| 4.4-25 | `:595-598` | "finalized by restoring the corresponding parametric surfaces and boundary curves of them" | `emit_topology` (`stage5_topology.rs:5150`) gives each face its input face's analytic surface. Assembly: `kernel-v2/src/boolean/from_yang.rs:109` `from_yang_brep_indexed_with_operands` | FAITHFUL | — | — |
| 4.4-26 | `:599-601` | "The watertightness of our result is inherited from the mesh Boolean output, ensuring the mesh has no geometric gaps" | Inherited from the watertight Stage-2 arrangement; updates preserve it (`plan_keeps_boundary` `stage4_construct.rs:3068`; `collapse_vertex`). Yang-side gate: `yang-rs/src/stage4_relocate.rs:2385` `check_watertight_2manifold`, called at `stage4_correct.rs:14018`. Final pairing check: kernel-v2 twin validation (`from_yang.rs:~520-600`) | PARTIAL: the yang gate runs BEFORE rim relocation, the corner transit (`stage4_correct.rs:14339`), the post-Stage-4 construct/fold-merge alternation and KV15b. Only kernel-v2 re-checks after those | — | `yang-rs/tests/yr10_adversary.rs::adv4_geometric_closed_two_manifold` |
| 4.4-27 | `:601-603` | "After mesh updating, the topology of the mesh matches that of the intended B-Rep model exactly" | Not guaranteed: refused patches keep relocate-in-place. Mismatches are DETECTED afterwards by the §4.5.2 loop triggers (`boolean.rs:621` `refine_loop`: T3b/T3c/T4/T5) and by kernel-v2 | PARTIAL | N2 | — |
| 4.4-28 | `:603-605` | "The final B-Rep Boolean obtained via the bijection is therefore topologically correct" | Face and loop topology is read off mesh patches (faithful). After that, several rewrites change edge and loop granularity: the envelope pre-pass (`stage5_topology.rs:5248`), seam merge and decimation (`:6768`, `:6796`), the N76 backtrack merge (`boolean.rs:3057`) and kernel-v2 `recover_output_curves`. kernel-v2 then verifies the result (per-shell Euler–Poincaré genus at `from_yang.rs:1036`, then `validate_solid`) | PARTIAL: topology is verified, not only inherited | N76, N60 | — |

**Status counts (28 rows):** 10 FAITHFUL, 10 PARTIAL, 4 DEVIATES, 2 ORDER, 1 MISSING, 1 SILENT, 0 N/A.

### Code with no paper row

These are top-level passes in my files that neither my rows nor the plan's §2 table account for. `repair_fan_cluster`, `merge_same_plane_patches`, `collapse_subtauwork_mesh_edges`, seam merge and decimation ARE in plan §2b and are not repeated here.

- **`remove_doubled_membranes`** (`yang-rs/src/stage4_relocate.rs:2064`, called at `stage4_correct.rs:13929`). Drops opposite-winding coincident triangle fins before the shell gate. It compensates for §4.4.3 ("no geometric gaps" / 2-manifold), since the paper never produces the fin. Ledger N41. Spec 2b lists it for retirement, but plan §2b does not.
- **`split_pinch_vertices`** (`stage4_relocate.rs:2256`, called at `stage4_correct.rs:13948`, step "4a2"). Splits a tangency pinch vertex into copies. It compensates for §4.3.3 tangent points and §4.4.3 manifoldness. There is no paper basis for splitting.
- **`validate_relocated_triangles`** (`stage4_correct/validate.rs:160`, called at `stage4_correct.rs:13917`). A degenerate-triangle STOP on relocated triangles. The code cites "§4.5 step 4". It is the guard for what a §4.4.1 CDT would never emit.
- **`run_construct_passes`** (`stage5_topology.rs:3068`, alternating with `run_fold_merge_passes` under `ALTERNATION_CAP = 8`, `:4808`). A SECOND §4.4.1 site after Stage 4: it collapses seam runs, inserts §4.3.4 samples, and re-CDTs whole patches (`:3870` `rebuild_patch_planar_bounded`). Its paper basis is §4.4.1, so it duplicates `update_patches` at a later point (plan §1 names it only as "construct pass").
- **Run/group absorption and corner re-homing** inside `run_fold_merge_passes` (`stage5_topology.rs:1307` `run_absorption_attempt`, `:1721` `rehome_attempt` → `yang-rs/src/stage4_rehome.rs` `plan_corner_rehoming`, `:1400`). Re-homing kills a phantom junction pair and mints new corners. My guess: it compensates for the §4.3.3 Case-IV rule-out at a junction. As a mesh edit it has no paper basis.
- **`envelope_prepass`** (`yang-rs/src/stage5_envelope.rs`, called at `stage5_topology.rs:5248`). Rebuilds the loops of osculating boundary-curve pairs on a cylinder and can add standalone "notch seal" faces. No paper basis (#188). It touches §4.4.2 boundary-curve collection.
- **`subdivide_loops_at_shared_vertices`** (`stage5_topology.rs:7467`) and **`patch_fold_slivers`** (`:7387`). N22: Stage-6 loop T-subdivision and fold-sliver exclusion. They compensate for degenerate arrangement children, which the paper does not treat.
- **`normalize_output_curve_backtracks`** (`yang-rs/src/brep.rs:1119`, called at `boolean.rs:3057`). N76: merges output loops that traverse one `SurfacePair` curve twice. No paper basis; it serves §4.4.3.
- **kernel-v2 `recover_output_curves`** (`kernel-v2/src/recover.rs:208`). Retags chord runs as exact cylinder∩plane circles, fuses Steiner vertices, and canonicalizes laterals to the 4-edge form. It is closest to §4.4.2 "fitting the curve", done at the import boundary.
- `stage4_slit::bridge_census` (`stage5_topology.rs:4521`, gated by an environment variable) and the `stage5_loop_simplicity` / `stage4_truncate::max_simple_step` calls inside `emit_topology` (`:5571-5800`, under `YANG_S6_LOOP_SIMPLICITY`) are print-only probes. They are not mechanisms.

### Order of operations

**Paper:** §4.3 optimize → (§4.5 failure handling) → **§4.4.1** trim M_A and M_B with the curves (r_A = r_B = r; Fig. 11 split, merge, insert; CDT in the parametric domain; near-curve removal; recompute d(T)) → **§4.4.2** in/out classification of the trimmed meshes → patch segmentation → restore surfaces and curves → **§4.4.3** watertightness, inherited.

**Code** (`boolean.rs` `boolean_once`, then `stage5_topology.rs:4504` `reconstruct_topology_stage4`, then `stage4_correct.rs:8918` `stage4_relocate_and_correct_inner`):
1. Stage 2: arrangement plus Cherchi in/out, then `keep_set` (`boolean.rs:1831`, `:2131`). **§4.4.2 classification runs here, before any §4.4.1 step.**
2. Per-triangle attribution (`boolean.rs:2644`). Phase A flood-fill patches (`stage5_topology.rs:4527` → `stage4_correct.rs:1176`).
3. Stage 4 inner: relocation (2) `:11193`, which embeds the §4.5.1 transit (`:10428`) → KV9 twin dedup `:12777` → minted block: pre-sweep N47 weld `~:12975`, `trim_beyond_corner_phantoms` `:13035`, `retriangulate_collapsed_fan_regions` / `repair_fan_cluster` `:13046` → §4.5.3 sweep `:13059` → §4.4.1(b) N55 merge `:13076` → N47 weld `:13325` → **`update_patches` `:13338` (§4.4.1 CDT)** → (3d) "§4.4.1(a)" unzip `:13400` → degenerate STOP `:13917` → membrane removal `:13929` → pinch split `:13948` → sub-TAU_WORK collapse `~:14012` → **watertight gate (§4.4.3) `:14018`** → rim relocation → corner transit `:14339` → domain postconditions.
4. After Stage 4: construct pass ⇄ fold-merge alternation, up to 8 rounds (`stage5_topology.rs:4808-4851`). This is a second §4.4.1 re-CDT site. Then KV15b (`:4923`) and a second sub-TAU_WORK collapse.
5. `emit_topology` (`:5059` → `:5150`): T-subdivision, envelope pre-pass, emission, §4.4.2 restoration, seam merge, decimation. Then N76 (`boolean.rs:3057`). Then kernel-v2 `recover_output_curves` → `from_yang` validation.

**Inversions:**
- (a) §4.4.2 classification (step 1) comes before §4.4.1 (steps 3–4).
- (b) In the paper, Fig. 11's split, merge and insert happen INSIDE the CDT. In the code, the merge (N55, `:13076`) and a fold repair (`:13046`) run before the CDT (`:13338`), and the "(a) split" (`:13400`) runs after it as a repair.
- (c) §4.4.1 is spread over three sites (Stage-4 `update_patches`, the post-Stage-4 construct/fold-merge, and Stage-5 seam merge/decimation). The paper has one operation.
- (d) The §4.4.3 watertight gate runs before later mesh mutations: rim relocation and corner transit in Stage 4, then the construct/fold-merge passes and KV15b.
- (e) The tangent-point r_A = r_B = r runs at Stage 1 (row 06).

### Findings (ranked)

1. **ORDER inversion, not named in the ledger: classification before mesh updating.** The paper trims and updates M_A and M_B and then classifies ("After trimming the meshes … we directly apply a standard inside/outside classification", `:576-579`). The code classifies the untrimmed arrangement (`boolean.rs:2131`) and runs every §4.4.1 step on the kept submesh only (`boolean.rs:3020`). N2 says only that the code "trusts the sidecar-trimmed mesh" and never states the inversion.
2. **MISSING: the Fig. 11(c) empty-loop insert** (`:559-560`, `:564-567`). Its only implementation (the N2-1 `stage4_update` insert) was deleted in Phase 0. Spec 2b §3 calls it "subsumed by d(T) seeding", but I2e seeding (`stage4_construct.rs:807`, `:882`) fires only on curved `ChordDegradation`. In practice it is covered only because the Stage-2 arrangement already triangulates loop interiors.
3. **The ledger and spec describe more than the code does.**
   - N2's 2026-10-16 update claims "redundant / near-boundary interior removal". The code removes curved interiors only when they are exactly collinear with a boundary segment (`stage4_construct.rs:568`, 1e-9 relative). Spec 2b §3's planned "chart distance … below d_ε" removal was not built (row 15).
   - Spec 2b §3 also says "Failure is a §4.5.2 site, never a STOP … records a failure site". In fact `update_patches` silently `continue`s on every refusal (`:3178-3184`, `:3197`), drops an `apply_rebuild_batch` error to 0 (`:3207`), and writes a census only under `YANG_441_UPDATE_CENSUS`.
4. **Fig. 11(b) merge DEVIATES (N55).** The paper's "too close … merge p with q … to improve the mesh quality" (`:557-559`) is a quality threshold. The code merges only numerically identical points (`TAU_WORK·(1+scale)`, `stage4_correct.rs:13215`). Overruns are handled by a separate fold-merge pass after Stage 4 (`stage5_topology.rs:572`).
5. **§4.4.1 runs in three places** (row 3 and inversion c). `update_patches` (`stage4_correct.rs:13338`), the construct + fold-merge alternation (`stage5_topology.rs:4808`, which re-CDTs again via `:3870`), and Stage-5 seam merge/decimation (`:6768`, `:6796`). The paper describes one per-patch update. Plan Phase 2c covers only the Stage-5 part.
6. **A d(T) budget failure means refusal, not refinement** (row 16). 2,312 patches (spec §9) keep relocate-in-place under `ChordDegradation`. The budget is the op-wide `stage4_chord_band` (`stage4_correct.rs:1587`), not the patch's own d_ε.
7. **No p-curves (row 19/23).** The paper restores "boundary curves in the parametric domain" by "fitting the curve in the parametric domain" (`:581-594`). The code emits 3D analytic curves attached by identity, then coalesces runs in four passes (yang restoration, seam merge, decimation; kernel-v2 recover). The ledger covers the curve source (N7, N60) but never names the absence of parametric curves.
8. **Stale ledger and plan locations.**
   - N10 cites `lib.rs` `build_intersection_curves` / `compute_phase_a (lib.rs:3279)`; they are now at `stage3_ssi.rs:768` and `stage4_correct.rs:1176`, and `lib.rs` is 229 lines.
   - N12 cites `lib.rs`; the tier logic is now in `boolean.rs` (~`:2950`).
   - N32 cites `orient_directed_curve` at `:301` with pushes at `:425` / `:628`; it is now at `stage5_topology.rs:5090` with pushes at `:5319` / `:6071` / `:6642`.
   - Cleanup plan §2b line numbers are stale: `repair_fan_cluster` :3343 → :2609, `collapse_subtauwork_mesh_edges` :4799 → :3983, `merge_same_plane_patches` :1329 → :1339, `weld_coincident_relocated` :2797 → :2025.

## Slice: §4.5 Solving failures handling, §4.5.1, §4.5.2


Paper text: `refs/text/yang2025_hybrid_boolean.txt`. Where each section sits in the two-column layout: §4.5 intro starts in the right column at `:607-609` and continues in the left column at `:642-670`. §4.5.1 starts in the left column at `:672-689` and continues in the right column at `:632-657`. §4.5.2 is in the right column at `:659-677`.
All code paths are relative to `crates/yang-rs/src/` unless they say otherwise.

| id | paper | statement | code | status | ledger | pin |
|---|---|---|---|---|---|---|
| 4.5-01 | `:607-609,:642-652` | Failure cause: mesh intersections on "mesh patches that do not correspond to the two surfaces", or "insufficient discretization resolution" gives a bad initialization | `stage4_correct.rs:4831` `under_resolution_ratio` measures the cause as a quantity: d_ε(far) / \|d_far(q)\| at each crossed corner | SILENT: the paper only describes the cause. Our choice is a numeric under-resolution certificate carried on the STOP (`stage4_correct.rs:7835-7840`) | N67 | `tests_unit/s452_under_resolution_ladder.rs::r0085_s_rider_demand_names_density_eight` |
| 4.5-02 | `:654-656` | "first detects failure regions and then employs two strategies" | `boolean/failures.rs:1267` `failures` (the trigger set); `stage4_correct.rs:562` `s451_plan_repairs` (strategy 1); `boolean.rs:621` `refine_loop` (strategy 2) | FAITHFUL | N62 | `tests_unit/s452_failure_sites.rs::a_clean_body_has_an_empty_failure_set` |
| 4.5-03 | `:656-659` | After optimization, "collect the point pairs that cannot converge to a distance of 0 within their domains" | `stage4_correct.rs:174` `s451_stop` (OffCurve gates record and continue, `:183`); `stage4_correct.rs:8805` T1 `record_relocation_divergence`; `stage4_correct.rs:6991` `boundary_domain_postcondition` (T2, "within their domains") | PARTIAL. Only the ~13 `OffCurveBeyondChordBand` gates collect after the sweep. The ~50 other `LocalRefinementRequired` returns, plus the §4-I9 STOP (`:7838`), abort at the first fire. T1 records one vertex, not the set | N66 | `tests_unit/s45_boundary_domain.rs::p0003_rim_circle_crossing_left_the_lateral_face_past_its_top_edge` |
| 4.5-04 | `:656-659` | The failure set is the non-converged points only | `boolean/failures.rs:28` `FailureTrigger` also feeds T3b/T3c (§4.5.4), T4/T5 (non-manifold, §4.4.3), T6 (§4.3 near-miss) and Stage-3 `SsiRefinementFailed` into the same loop | SILENT: we extended the trigger set. The catch-up ledger rates T4/T5 "faithful" via `:659-668` | catch-up rows P0053/P0055, C0118 | `tests_unit/s452_failure_sites.rs::an_impossible_euler_shell_names_the_faces_on_its_over_covered_edges` |
| 4.5-05 | `:659-661` | An erroneous region is the run between two successfully optimized points v0, v1 on the same surface: "all the optimization between v0 and v1 has failed" | `stage4_correct.rs:562` `s451_plan_repairs` walks the failure's own-curve chain to the nearest `good` vertex in both directions (`good` = converged, not recorded, not an I9 crosser); `:4550` `selector_clause2_walk` (census) | FAITHFUL. Restricted to the vertex's own conic chain; LineSegment chains cannot key a region | — | — |
| 4.5-06 | `:661-664` | First strategy: remove the points between v0 and v1, replace them with a new point, "restart the optimization by allowing the points across the boundaries" | `stage4_correct.rs:12715-12747` applies the planned repairs (`collapse_vertex` victims → survivor, survivor ← proj) | PARTIAL. Remove-and-replace exists (always on since 2026-08-22). "Allowing the points across the boundaries" is not built for interior regions (DRIFT-only, see 4.5.1-04) | **N90** (ledgered 2026-10-17); D8 is HISTORICAL legacy | — |
| 4.5-07 | `:664-667` | "If such bound cannot be found or after crossing boundaries the optimization still fails", increase mesh resolution locally and re-optimize (second strategy) | `stage4_correct.rs:12748-12751`: any decline returns the first recorded error → T1 → `boolean.rs:966` `refine_loop` | FAITHFUL (strategy 1, then strategy 2) | N62 | kernel-v2 `tests/s452_domain_lens_corner.rs::a_quadruple_corner_inside_the_chord_sagitta_resolves_to_the_exact_triple_pair` |
| 4.5-08 | `:667-668` | "The above procedures are repeated if optimization failure persists" | `boolean.rs:712-817` loop: region → refine → `boolean_once` (`:769`) → `failures` again. A faceless refined STOP re-refines the previous region (`:723`) | FAITHFUL | N62, N67 | kernel-v2 `tests/s451_transit_neighbour_side_chain.rs::a_transit_onto_a_cap_plane_outside_its_disc_is_declined_and_the_ladder_converges` |
| 4.5-09 | `:668-670` | "guaranteed to terminate since the mesh intersections converge … under refinement" | `boolean.rs:582` `REFINE_LOOP_ITER_CAP = 8`; `stage1_tessellate.rs:158` `FACE_REFINE_MAX = 8`; convergence monitor (`boolean.rs:803`: a failure signature that returns twice ends the loop) | SILENT. The paper gives no budget. Our choice is ceiling density 8 per face, 8 iterations, and a signature-repeat monitor | N67 | `tests_unit/s452_face_refine.rs::refinement_composes_by_doubling_and_saturates_at_the_ceiling` |
| 4.5-10 | `:667-670` (+ overview `:309-312` "until all the failures are resolved") | The loop ends with failures resolved | `boolean.rs` `refine_loop`: only a run with an EMPTY failure set is adopted; at the ceiling a natural body with surviving failures STOPs (`RefinementExhausted`), a natural STOP stands | SILENT — the paper names no ceiling; our choice, a LOUD stop, user-signed 2026-10-16 | N66 (amended 2026-10-16) | `kernel-v2/tests/s4_ellipse_pair_corner_chain.rs::a_surviving_boundary_domain_fire_stops_at_the_ceiling`; `assay_kv2.rs` smoke pin (P0005, P0015, P0020, P0029, R0070 `Category::Error`) |
| 4.5.1-01 | `:672-678` | Strategy 1 applies when the same surface has two correct points v0, v1 on both sides of the erroneous region | `stage4_correct.rs:760` declines unless exactly 2 distinct bounds; shared cone+plane pair required (conic arm), or a ≥2-surface constraint set (torus arm, `:632-647`) | PARTIAL. The only pair kinds are cone+plane and torus-carried; everything else declines | — | — |
| 4.5.1-02 | `:678-679` | "points in the erroneous region are removed and replaced with the midpoint p0 of v0 and v1" | `stage4_correct.rs:803` (conic arm `mid`), `:632` (torus arm); collapse at `:12720-12727` with §4-I8 containment | FAITHFUL | N47 (containment) | — |
| 4.5.1-03 | `:679-680` | "p0 is optimized using the geometric method described in Sec. 4.3.2" | `stage4_correct.rs:808` `project_onto_cone_section` (closed form); `:647` `relocate_onto_implicit_pair` / `_triple` (Newton, torus arm) | N/A for the conic arm (closed form instead of §4.3.2); the torus arm uses Newton on implicit surfaces | N7 | — |
| 4.5.1-04 | `:680-683` | Truncate a step that would leave S2 so the point "moves to p on the boundary curve C_b" (interior-point regions) | not on the interior path. `stage4_truncate.rs:351` `max_in_domain_step` is called only as a print (`stage4_correct.rs:7344` "Report-only … §4.5.1's continuation … does not exist yet") | MISSING for the class the paper names. Spec `yang_451_optimize_across_boundaries.md` §3.4/§9 says it is "unbuilt until a customer appears"; §4.5.2 compensates | **N90** (ledgered 2026-10-17) | `stage4_truncate.rs` unit tests (primitive only) |
| 4.5.1-05 | `:680-686` | The same truncate → continue with S1's parameterization mechanism | `stage4_boundary_curve.rs:1560` `solve_crease_transit` (truncate at C_b, re-solve the triple with s_own → s_nbr seeded at p, certify neighbour domain); called from the Stage-4 triple block `stage4_correct.rs:10498` | DEVIATES. The mechanism is faithful in its steps but runs on a 3-surface (boundary/corner) vertex whose triple root left a crease. Paper `:635-657` excludes boundary points from strategy 1 and sends them to §4.5.2 | **N87** (ledgered 2026-10-17); the catch-up row P0040's "faithful" is withdrawn | `tests_unit/s451_crease_domain.rs::an_out_of_domain_step_transits_onto_the_neighbour` |
| 4.5.1-06 | `:686-688` | Smooth transit "without reparametrization" | implicit-surface Newton / closed form has no (u,v) parameterization (`stage4_relocate.rs:367` `relocate_onto_implicit_triple`) | SILENT. Moot under implicit solving | N7 | — |
| 4.5.1-07 | `:688-689,:632` | After p converges, "solve the intersection points q1 and q2 on C_b using Newton's method" | `stage4_boundary_curve.rs:1619-1630` step 4 of `solve_crease_transit`: `stage4_transit::circle_surface_roots` (all roots, nearest taken, margin recorded) | PARTIAL. Only a `Circle` C_b; any other crease returns `NoTruncation`. Closed form, not Newton (N7) | N7 | `tests_unit/s451_crease_domain.rs::a_transit_that_leaves_the_neighbour_in_turn_is_declined` |
| 4.5.1-08 | `:632-633` | "then refine the curve as mentioned in Sec. 4.3.4" | `stage4_boundary_curve.rs:4788` `transit_emission_refine` (splits crease chords at exact arc midpoints until the fill "lifts" consistently; its doc calls itself "Yang §4.5.2") | DEVIATES. The criterion is a lift-flip test, not §4.3.4's acceptance test | **N87** (ledgered 2026-10-17) | `tests_unit/s451_crease_domain.rs` (emission family) |
| 4.5.1-09 | `:635-638` | "the first strategy only applies to the interior points but not to the boundary points that glide along the boundary curves" | `stage4_correct.rs:590-592` clause 1: `carrier_counts` max ≠ 1 ⇒ DECLINE | FAITHFUL in the DRIFT arm. Violated by the two transits (4.5.1-05, 4.5.1-10) | — | — |
| 4.5.1-10 | `:639-651` (Fig. 13) | At corner s ("more than two surfaces meet"), gliding points crossing s "may lead to topology errors", so the case is excluded from strategy 1 | `stage4_correct.rs:4984` `corner_transit_apply` + `stage4_transit.rs:1654` `classify` / `:989` `walk_corridor` + `stage4_corridor.rs:372` `plan_invocation`: deterministic transit ACROSS the model corner q, including "CORNER CLIP" ("Yang Fig-13(c)'s declined case, made deterministic", `stage4_transit.rs:20-23`) | DEVIATES. This is the case the paper declines; spec `yang_451_corner_transit.md` says "the repair the paper does NOT describe" | **N87** (ledgered 2026-10-17) | `stage4_transit.rs:1835` `tests::crease_transit_classifies_one_real` |
| 4.5.1-11 | `:639-651` | Detecting that a gliding point crossed a corner / left its face | `stage4_correct.rs:4773` `corner_crossing_fires` / `:7127` `relocation_domain_postcondition` (§4-I9 STOP `RelocationCrossedCarrierVertex`); `stage4_boundary_curve.rs:1375` `boundary_crease_crossed` (T2) | FAITHFUL as detection. Feeds §4.5.2 | N66 | `tests_unit/s45_boundary_domain.rs::p0003_top_edge_crossing_left_the_torus_face_past_its_rim` |
| 4.5.1-12 | `:639-640` | "s is a corner point where more than two surfaces meet" | `stage4_correct.rs:10319-10345` triple block admits a vertex with exactly 3 distinct incident surfaces → `relocate_onto_implicit_triple` | PARTIAL. 3 surfaces solved; ≥4 surfaces fall to the loud audits (I2) | catch-up row "incidence gate" (faithful) | kernel-v2 `tests/s452_domain_lens_corner.rs` (triple pair) |
| 4.5.1-13 | `:653-657` | Use strategy 1 only when bounded by two successfully optimized points on the same surface; "for other cases, we apply the second strategy" | `stage4_correct.rs:562` (declines ⇒ STOP ⇒ loop) for interior points. The crease transit (`:10498`) and the corner transit (`:14339`) run BEFORE any bound test, on unbounded boundary sites | DEVIATES (the selector is bypassed by the two transits) | **N87** (ledgered 2026-10-17) | — |
| 4.5.2-01 | `:659-661` | "increase the mesh resolution of the parametric surfaces associated with the erroneous regions" | `boolean.rs:749-768` → `brep.rs:644` `BRep::refined_by` (density ×k per face; rims N·k, chart bounds d_ε/k²) | FAITHFUL | N62 | `tests_unit/s452_face_refine.rs::refining_the_lateral_doubles_both_rims_and_both_caps_follow` |
| 4.5.2-02 | `:661-663` | The region is bounded by two successfully optimized points p_f, p_b "on boundary curves belonging to different patches" | none. Sites name faces at a vertex (`boolean/failures.rs:172` `faces_at_vertex`) or a fire's `incident` (`:215`) | MISSING (p_f and p_b are never identified) | N62 (text claims RESOLVED) | — |
| 4.5.2-03 | `:663-666` | "surfaces requiring refinement include those traversed by C_p" (the curve segment between p_f and p_b) | `boolean/failures.rs:1315` `region`: the faces each site names (the STOP vertex's incident faces) | PARTIAL. Only the faces at the failure point, not every face C_p traverses; the one-ring compensates | N62 | `tests_unit/s452_failure_sites.rs::faces_at_a_vertex_are_the_distinct_incident_attributions` |
| 4.5.2-04 | `:666` | "and the neighbors of a ring of them" | `boolean/failures.rs:1331` `with_one_ring` (faces sharing a B-Rep VERTEX) | FAITHFUL. Vertex adjacency (not edge adjacency) is our SILENT choice, because LineSegment edges are per-loop copies | N62 | `tests_unit/s452_failure_sites.rs::the_region_is_the_named_faces_plus_one_ring_per_operand` |
| 4.5.2-05 | `:659-661` (step size unstated in §4.5.2) | Refinement step | `boolean.rs:749` k = 2 per iteration; first rung `failures::density_for_demand` (`boolean/failures.rs:634`, √certificate rounded up to a power of 2); regions accumulate | SILENT. ×2 is borrowed from §4.1's density step; the certificate-sized first rung is our economy | N67 | `tests_unit/s452_under_resolution_ladder.rs::p0015_s_demand_past_the_old_ceiling_starts_at_the_ceiling` |
| 4.5.2-06 | `:667-668` | "compute the intersections between the meshes only in the refined regions" | `boolean.rs:769` `boolean_once(&ra, &rb, …)` re-runs Stages 0–6 on the WHOLE refined operands | DEVIATES. The recomputation is global; only the discretization is local. N62's pre-2026-10-14 text named exactly this; the RESOLVED text no longer does | N62 | — |
| 4.5.2-07 | `:668-671` | From the new intersection points, find those on p_f's and p_b's boundary curves; "the curve bounded by them is used to replace the original" | none. The whole output body is replaced | MISSING (no local splice) | N62 | — |
| 4.5.2-08 | `:675-677` | "the new segments are further optimized to obtain accurate intersection points" | the re-run's Stage 4 (`stage5_topology.rs:4730` → `stage4_relocate_and_correct`) | FAITHFUL (globally; follows from 4.5.2-06) | — | — |
| 4.5.2-09 | `:667-670` (with `:309-312`) | Accept the refined result when the failures are gone | `boolean.rs:790` adopt iff `failures(out)` is empty | FAITHFUL; the adoption predicate is our explicit form | N62 | — (spec §6.4's planned "loop adopts first F-empty body" pin is absent from `tests_unit/s452_refinement_loop.rs`, which pins T3b only) |

### Code with no paper row

- `stage4_transit.rs` + `stage4_corridor.rs` + `stage4_correct.rs:4984` `corner_transit_apply` (with `:4893` `stitch_fan_polygon`): a corridor that crosses a model corner. It solves corrected triples on corner-incident edges, mints junctions, and re-CDTs the affected patches. It compensates for the paper's handling of Fig. 13 boundary gliders (paper: §4.5.2). The spec says the paper does not describe it. No paper basis; NOT ledgered.
- `stage4_boundary_curve.rs:2660-5077` transit emission chain (`transit_emission_plan` / `_edits` / `_region` / `_parts` / `_fill` / `_refine` / `_write`): mesh surgery that writes a crease transit into the Stage-4 mesh. It stands in for §4.4.1 mesh updating applied locally to a §4.5.1 result. `_refine` is a second Stage-4-internal "§4.5.2" (chord halving) that is separate from `refine_loop`.
- `stage4_correct.rs:4831` `under_resolution_ratio` + `boolean/failures.rs:634` `density_for_demand`: a predictive first-rung size. Compensates for nothing in the paper; it is an economy (N67).
- `boolean.rs:803` convergence monitor (failure-signature repeat ⇒ stop): no paper basis. The paper asserts termination.
- ~~`boolean.rs:819-838` exhausted-loop policy (natural body stands, or the first watertight refined body is adopted): no paper basis.~~ **Replaced 2026-10-16** by the loud ceiling (F-1).
- `stage4_correct.rs:199` `s451_post_sweep_census`, `:4685` `strategy_selection_census`, `:4377` `failure_population_census`: census-only prints (`YANG_451=census`, `YANG_S45_SELECT`). Instruments, not mechanisms.
- `stage4_truncate.rs:351` `max_in_domain_step`: the §4.5.1 truncation primitive, wired only as a print (`stage4_correct.rs:7344`) and in the Stage-5 `YANG_S451_*` probes. Built, never load-bearing.

### Order of operations

The paper's order: optimize all points (§4.3), then collect non-converged points (§4.5). For each interior region bounded on one surface, run §4.5.1 (midpoint, truncated/transit steps, q1/q2, §4.3.4 refine). Otherwise, or if that still fails, run §4.5.2: refine the C_p surfaces plus one ring, recompute intersections locally, splice, re-optimize. Repeat. The reversal correction (§4.5.3) and mesh updating (§4.4.1) operate on the converged points.

The code's order inside `stage4_relocate_and_correct_inner` (`stage4_correct.rs:8918`):
1. The relocation sweep. The crease transit (§4.5.1 mechanism on triple points, `:10498`) runs INLINE, per vertex, during the sweep. It does not run after failure collection.
2. Post-sweep §4.5.1 DRIFT repair (`:12715`). This is the paper's position.
3. §4.5.3 reversal sweep (`:13059`).
4. §4.4.1 `update_patches` (`:13338`).
5. `corner_transit_apply` (`:14339`), then `relocation_domain_postcondition` (`:14344`, §4-I9 failure detection), then `boundary_domain_postcondition` (`:14350`, T2 failure collection).

After that, Stages 5/6 emit, T3b/T3c/T4 are read on the emitted body (`boolean/failures.rs:1272-1283`), and only then does `refine_loop` run (`boolean.rs:966`).

Inversions:
- **(a)** A §4.5.1-labelled repair (the corner transit) and the §4.5 failure detection for boundary points (T2, I9) run AFTER §4.5.3 and §4.4.1, not before them.
- **(b)** The crease transit runs mid-sweep, before any failure set or v0/v1 bounds exist, so the §4.5 selector (`:653-657`) is never consulted for it.
- **(c)** §4.5.2 runs at the op level after B-Rep emission, wrapping the whole pipeline. The paper runs it inside the optimization phase.
- **(d)** Most T1 sources abort Stage 4 at the first fire. "Collect after optimization" holds only for the OffCurve gates.

### Findings (ranked)

1. **The two live "§4.5.1" transits apply the strategy to the exact class the paper excludes, and neither is ledgered.** `solve_crease_transit` (`stage4_boundary_curve.rs:1560`, called at `stage4_correct.rs:10498`) and `corner_transit_apply` / `stage4_transit` (`stage4_correct.rs:4984`) both run on boundary/corner points. Paper `:635-657` and Fig. 13 send that class to §4.5.2. `stage4_transit.rs:20-23` calls itself "Yang Fig-13(c)'s declined case, made deterministic". The ledger has no entry for either. The catch-up row for P0040 grades the transit "faithful — §4.5.1". Cleanup spec §5 lists the corner transit as "not touched", so it sits outside the cleanup.
2. **§4.5.1 for interior points is half built.** Remove + midpoint + project is live (`stage4_correct.rs:562`, `:12715`), but only for cone+plane and torus pairs. Truncation at C_b, continuation on S1, and q1/q2 for INTERIOR regions are absent: `max_in_domain_step` is print-only (`stage4_correct.rs:7344`). No live entry records this. D8 is HISTORICAL and legacy-only.
3. **N62 is marked RESOLVED, but two of §4.5.2's three locality clauses are not implemented.** First, the refined op is recomputed globally (`boolean.rs:769` `boolean_once` on the whole operands), not "only in the refined regions" (`:667-668`). Second, there is no p_f/p_b identification and no splice of the new curve into the old one (`:661-671`). N62's pre-RESOLVED text named the whole-op recompute; its RESOLVED text silently drops it.
4. **The region is "faces at the failure point plus one ring", not "surfaces traversed by C_p plus one ring".** `boolean/failures.rs:172` / `:1315`: C_p is never walked, so a long erroneous span crossing several faces refines only the faces at its STOP vertex. Each further span face is picked up one ring per iteration, if at all. Not ledgered as a deviation.
5. **Failure collection is mostly abort-first.** Only the OffCurve gates record and continue (`stage4_correct.rs:174-185`). About 50 other `LocalRefinementRequired` returns, and the §4-I9 STOP (`:7838`), end Stage 4 at the first fire. T1 therefore names one vertex's faces per run, against the paper's "collect the point pairs" (`:656-659`). The order inversions (corner transit and T2 detection after §4.5.3 and §4.4.1) compound this.
6. **RESOLVED 2026-10-16 (F-1).** **The exhausted-loop policy emits bodies that still have failures** (`boolean.rs:819-838`). This contradicts "until all the failures are resolved" (`:309-312`, `:667-668`). N66 records the inventory policy, but its text still names the deleted `refine_452_domain` ladder. So does the doc comment on `boundary_domain_postcondition` (`stage4_correct.rs:6975-6990`).
7. **Stale paper line cites in the ledger and code.** The catch-up rows cite `:689-692` for "s is a corner point" (actual `:639-640`) and `:664-676` for step rescaling (actual `:680-686`). `selector_clause2_walk`'s doc (`stage4_correct.rs:4527`) cites `:740-744` for the same-surface clause (actual `:651-657`). These look like citations made against an older text extraction.
8. **The loop has no direct pin in yang-rs.** `tests_unit/s452_refinement_loop.rs` pins only T3b. The adoption and ceiling behaviour of `refine_loop` is pinned only indirectly, through kernel-v2's `s452_domain_lens_corner.rs` and `s451_transit_neighbour_side_chain.rs`. The §4.5.1 DRIFT repair has no pin found.

## Slice: §4.5.3 Reversed intersections, §4.5.4 Illegal intersections, §4.5.5 Coplanarity, §5.5


Paper: `refs/text/yang2025_hybrid_boolean.txt`. §4.5.3 is in the right column at :679-751 (Fig. 15 caption :709-715). §4.5.4 is in the left column at :752-758. §4.5.5 is in the right column at :718-731 (Fig. 16 caption :752-760). §5 tolerances are at :743-745 (right). §5.5 is at :950-1033 (right), with the Fig. 20 caption at :973-977.
Paths are relative to `crates/yang-rs/src/` unless they start with `crates/`. I opened every cited line.
Note: the slice brief mentions "2·d_ε" under §4.5.3. **The paper's §4.5.3 has no 2·d_ε**; the paper uses 2d_ε only in §4.2.1 (:435-489). The 2·d_ε in this sweep is the code's own resolution gate (row 4.5.3-13).

| id | paper | statement | code | status | ledger | pin |
|---|---|---|---|---|---|---|
| 4.5.3-01 | `:679-687,718-721` | Reversal occurs "after convergence" when mesh resolution cannot keep a one-to-one map; it must be corrected before §4.4.1 mesh updating. | `stage4_correct.rs:13059` call to `sweep_reversed_intersections`. It runs after all Stage-4 relocations and before `update_patches` (`stage4_correct.rs:13338`). | FAITHFUL | — | — |
| 4.5.3-02 | `:718-721` | "first detects the reversed intersection point through tangential direction analysis, then removes the reversed point, and locally corrects the intersection curve". | `stage4_correct/reversal.rs:18` `sweep_reversed_intersections`. It edits the MESH through `collapse_vertex` (`:312`), not a separate curve polyline. | FAITHFUL | N29 | `tests_unit/m5_case_iv.rs::s453_collapse_removes_p_n_on_same_curve_run` |
| 4.5.3-03 | `:722-727` | Discrete tangent at p_r is "t̃ = p_b p_r/‖·‖ + p_r p_n/‖·‖" (Eq. 2). | `reversal.rs:1369` `angle_band_reversed` (`v1+v2`). | FAITHFUL | — | `m5_case_iv.rs::s453c_line_run_reversal_branches` |
| 4.5.3-04 | `:728-742` | The exact tangent is "t_pr = n_A,pr × n_B,pr" from the converged point's two surface normals. | `reversal.rs:1470` (straight runs use n0×n1 via `surface_normal_at`). Circle/Ellipse sites use the conic's own analytic tangent (`:1489` onward). That is the same direction computed another way. | FAITHFUL | — | `m5_case_iv.rs::s453c_surface_normal_at_canonical` |
| 4.5.3-05 | `:742-743` | "If the angle between them is greater than 45° and less than 135°, we determine that a reverse has occurred". | `reversal.rs:30-31` (lo/hi), `:1399` (angle folded by \|dot\|, which is equivalent because the band is symmetric about 90°). This applies ONLY to all-conic (Circle/Ellipse) cycles and straight LineSegment runs. | PARTIAL | N29 | `m5_case_iv.rs::s453c_line_run_reversal_branches` |
| 4.5.3-06 | `:743-745` | Collinear reversed points make t̃ "almost degenerate … we directly detect the reversal, avoiding the angle comparisons". | `reversal.rs:1383` (\|t̃\| < TAU_WORK ⇒ reversed). | FAITHFUL | N3 (RESOLVED) | `m5_case_iv.rs::s453c_line_run_reversal_branches` |
| 4.5.3-07 | `:742-745` (detector) | Angle test against t_pr (general surfaces). | Substituted on three site classes. (a) Shared Circle/Ellipse in MIXED cycles: exact parameter order `d1·d2<0`, `reversal.rs:228` and `:1116` `conic_param_deltas`. (b) Untyped pair chains: progression sign along n_A×n_B, `:218` and `:903` `pair_site_tangent`. (c) Typed `SurfacePair`: the same sign test, `:1116`. | DEVIATES | N29, N59 | `m5_case_iv.rs::s453d_shared_circle_backtrack_reversed`, `::s453e_surface_pair_tangent_backtrack` |
| 4.5.3-08 | `:742-745` (coverage) | Every intersection curve C is subject to the test. | **Parabola and Hyperbola edges are never tested.** `mixed_cycle_shared_conic` (`reversal.rs:1178`) returns `None` for them, so the site `continue`s (`:226-229`). Only Circle/Ellipse count as `all_conic` (`:135`). | PARTIAL | **N89** item 3 (ledgered 2026-10-17) | — |
| 4.5.3-09 | `:746-749` | "For a reversal point p_r, we remove its next point and reconnect … repetitively remove the next point p_n … until there is no reversal". | `reversal.rs:407` `paper_order_removal` returns `(p_n, p_r)` once a later point v_j resolves the reversal. It looks ahead before committing (reading (i)). | FAITHFUL | N29 (amend. 2026-10-16) | `m5_case_iv.rs::s453_collapse_removes_p_n_on_same_curve_run` |
| 4.5.3-10 | `:749-751` | "If no such next point can be found, we remove p_r and restart the correction from p_b". | `reversal.rs:407` removes p_r and collapses it onto the NEARER of p_b/p_n (reading (ii)). After every single removal, `break 'outer` (`:323`) recomputes Phase A (`:54`) and re-walks every loop from its start, which revisits p_b. | FAITHFUL | N29 | `m5_case_iv.rs::s453_paper_order_removes_p_r_when_no_next_point_resolves` |
| 4.5.3-11 | `:728-731` ("C between S_A and S_B") | The test runs on points of ONE curve C. | `reversal.rs:467` `curve_endpoint_ranked` defines where C ends: a curve change, a run end, a pair change, or rank ≥3 surfaces or a Stage-1 mint (`:170-184`). Endpoints are never p_r (`:253`) and never removed. | SILENT | N29 | `m5_case_iv.rs::s453_collapse_protects_junction_p_n`, `::s453e_minted_junction_outranks_an_edge_carried_tie` |
| 4.5.3-12 | `:746-751` | The removal is the only correction. | The two OVERTAKE arms skip `paper_order_removal`, name their own (victim=p_r, survivor=moved junction J), bypass the 2·d_ε gate, and skip the endpoint guard. `line_overtake_site` is at `reversal.rs:551` (call `:235`). `carried_curve_overtake_site` is at `:650` (call `:209`) and applies to a SINGLE operand's B-Rep edge, which is not an S_A∩S_B curve. | DEVIATES | N29 (amended for P0036/R0059) | `tests_unit/s453_line_overtake.rs::r0059_shaped_overtaken_points_collapse_onto_the_relocated_junction`, `tests_unit/s453_carried_curve_overtake.rs::p0036_shaped_rim_chord_vertex_collapses_onto_the_relocated_corner` |
| 4.5.3-13 | — (no paper basis) | A removal is refused if victim→survivor > 2·d_ε. The reversal is then LEFT IN PLACE; since 2026-10-17 it is recorded at the fixed point as a T7 site with faces (F-3). | `reversal.rs:286` (`continue`). d_ε is the operand-pair max chord bound, `stage4_correct.rs:8987` and `:1587` `stage4_chord_band`. | DEVIATES | N29 ("2·d_ε collapse gate"; its line cites are stale) | `crates/yang-rs/tests/m8_holed_disc_coplanar.rs::annular_cap_hole_crossing_stays_loud` |
| 4.5.3-14 | `:715` "ensuring no reversal occurs" | The correction terminates with no reversal. | If a collapse drops nothing: `Stage4ReversalUnresolved` (`reversal.rs:316`). If no fixed point within `max_passes`: `LocalRefinementRequired` at vertex `u32::MAX` (`:40-45`). Neither is recorded with faces: `record_relocation_divergence` fires only on `Stage4RegionInvalid` (`stage4_correct.rs:8804`), and a sentinel vertex names no face. The §4.5.2 loop therefore gets a faceless site and cannot refine. **Since 2026-10-17 (F-3):** the no-progress removal records T7 with faces, and `max_passes` names the last reversed point. | PARTIAL (the exits are refinable; the paper's own termination is the reversal-free fixed point) | N29 amendment | `tests_unit/s452_failure_sites.rs::an_unresolved_reversal_names_the_faces_at_both_points` |
| 4.5.3-15 | `:745` (§5 angular tol.) | "The angular tolerance is uniformly set to 10⁻⁶ rad." | `reversal.rs:29-31` `ANG_TOL = 1e-6` widens the 45°/135° band. `pair_site_tangent` uses `PAIR_RANK_FLOOR=1e-6` (`:903` body). | FAITHFUL | — | — |
| 4.5.4-01 | `:752-753` | Detect "self-intersections that may occur during surface discretization". | `stage1_tessellate/self_contact.rs:68` `scan_self_contacts` uses `cherchi_rs::detect_improper_contacts` (`crates/cherchi-rs/src/inputcheck.rs:281`) over the AABB octree (`crates/cherchi-rs/src/aabb_tree.rs:86`). It is driven from `stage1_tessellate.rs:440-448`. | FAITHFUL | substitute per plan (N5/N61 family, ledger table row P0037) | `tests_unit/s1_self_contact.rs::a_cylinder_skin_thinner_than_its_sagitta_reports_the_pocket_with_a_rim_demand` |
| 4.5.4-02 | `:753-754` | Detect "new intersections that may arise during mesh updating". | The mesh-level census `boolean/failures.rs:589` `illegal_intersection_sites` is TEST-ONLY (`#[cfg_attr(not(test), allow(dead_code))]`, `:588`). Production detection runs on the EMITTED B-Rep instead: T3b `:1141` `output_curve_crossings` and T3c `:828` `output_loop_crossings`, via `:1279`. kernel-v2 then runs a render-resolution STOP (`crates/kernel-v2/src/validate/selfx.rs:53`, called at `crates/kernel-v2/src/boolean/mod.rs:544`). | ORDER | N6, N2 | `tests_unit/s454_output_loop_crossings.rs::an_arc_that_bulges_through_the_opposite_edge_is_a_crossing`, `tests_unit/s452_refinement_loop.rs::two_full_ellipses_of_two_faces_on_one_cylinder_cross_twice` |
| 4.5.4-03 | `:752-754` (coverage) | Every illegal intersection is detected. | T3c skips cone, sphere and torus charts and cylinder loops that wrap the axis (named gaps, `failures.rs:818-823` doc). T3b skips planar faces, straight edges and `SurfacePair` (`:1132-1137` doc). | PARTIAL | — | `s452_refinement_loop.rs::parallel_sections_and_a_single_face_never_cross` |
| 4.5.4-04 | `:755-756` | "We detect these illegal intersections and perform local refinement." | Stage 1: the dominant face halves its chord bound and the whole pass re-runs (`self_contact.rs` header; `stage1_tessellate.rs:448`). Output: `boolean.rs:621` `refine_loop` refines named faces plus one ring (`failures.rs:1315` `region`). | FAITHFUL | — | `s452_failure_sites.rs::the_region_is_the_named_faces_plus_one_ring_per_operand` |
| 4.5.4-05 | `:756-758` | "Since the input B-Rep model has no self-intersections, these illegal intersections are eliminated." | Bounded instead. Stage 1 stops at `SELF_CONTACT_ROUNDS=4` (`self_contact.rs:33`) with a typed `Stage1SelfContact`. The loop has a density ceiling (§4.5.2 slice). The kernel-v2 selfx gate STOPs and does not remove. | PARTIAL | N2 (removal half) | `s1_self_contact.rs::the_driver_refines_the_cylinder_until_its_chords_clear_the_pocket` |
| 4.5.5-01 | `:718-723` | "it is necessary to check coplanar planes": detect coplanar surface pairs between the two models. | `boolean/coplanar_scan.rs:140` `scan_near_coplanar` checks offset agreement plus parallelism (`:233`). | FAITHFUL | N8 | `crates/yang-rs/tests/yr24_near_coplanar.rs::r0029_near_coplanar_union_succeeds_via_stage0_overlay` |
| 4.5.5-02 | `:743-745` (§5 tolerances) | Uniform tolerances: distance d_p=10⁻⁷, angle 10⁻⁶ rad. | `coplanar_scan.rs:241` band = `max(TAU_MODEL, scale·TAU_WORK)`. Parallelism is tested as `sin·extent ≤ band`, a distance and not 1e-6 rad. Pairs with gap > band/100 are a loud STOP (`stage0/mod.rs:350`). | DEVIATES | N57 | `tests_unit/n178_subres_coplanar.rs::n178_tau_model_gap_wall_stops_loudly` |
| 4.5.5-03 | `:723-724` | "perform 2D Boolean operations before mesh discretizations". | `boolean.rs:1166` `stage0_preprocess` runs before Stage 2. It re-tessellates both operands via `build_stage0_mesh` (`stage0/mod.rs:2336,2344`). | FAITHFUL | — | `crates/yang-rs/tests/yr26_coplanar_boolean.rs::stacked_union_is_merged_solid` |
| 4.5.5-04 | `:724-725`, Fig.16(c) `:754-757` | "the intersection points of the 2D curves are calculated, and a 2D Boolean operation is performed, segmenting two surfaces into three parts". | `coplanar_overlay.rs:368` `coplanar_overlay` / `:386` `_multi` produce an exact rational A-only/B-only/Overlap classification, called at `stage0/mod.rs:937`. Arcs enter as Stage-1 chord rings, and their crossings are minted onto the exact circle (`stage0/rim_chords.rs:300` `resolve_rim_chord_vertex`). | PARTIAL | N25, N2 (N2-3a on-circle mint) | `crates/yang-rs/tests/yr25_coplanar_overlay.rs::half_overlapping_squares_f0002_pattern` |
| 4.5.5-05 | `:723-725` (scope) | Any coplanar plane pair is handled. | `stage0/frame.rs:94` `overlay_face_supported` accepts only line polygons, circle discs, annuli and Line+Circle/SurfacePair mixed faces. **Ellipse-edged faces are a `face-unsupported` wall** (`frame.rs:1016` doc). Other refusals: intra-solid pairs (`mod.rs:305`), annular hole-rim crossings (`:515`), and overlay failures. | PARTIAL | N17 (loud wall), N25; the ellipse wall is **N89** item 4 (ledgered 2026-10-17) | `m8_holed_disc_coplanar.rs::annular_cap_hole_crossing_stays_loud` |
| 4.5.5-06 | `:726-727` | "The overlapping part is replaced by a trimmed common planar surface". | Implemented as a shared PLANE, not a trimmed surface entity. `stage0/plane_weld.rs:52` `weld_coplanar_planes` rewrites B's plane, curve anchors and vertices onto the canonical plane (`boolean.rs:1081`). Stage 0 snaps and cross-welds loop vertices (`mod.rs:393-435`, `frame.rs:29` `snap`). No B-Rep face for the overlap region is created. | PARTIAL | N79 | `tests_unit/plane_weld.rs::plane_weld_rewrites_residual_cap_onto_the_canonical_plane` |
| 4.5.5-07 | `:727-728` | "identical meshes are generated for both models in this part". | `stage0/mod.rs:2168-2172` `tris_for([AOnly,Overlap])` / `([BOnly,Overlap], opposite)` use the same f64 coords, wound per solid. The n-ary path is `stage0/nary.rs:166` `overlay_nary_group`. | FAITHFUL | N25, N26 | `yr26_coplanar_boolean.rs::near_coplanar_stacked_matches_exact_results` |
| 4.5.5-08 | Fig.16 caption `:759-760` | "The common part and the other two parts share identical sampling points on their boundaries." | `stage0/mod.rs:2189-2210` `collect_edge_splits` (def `stage0/mesh_build.rs:528`) propagates overlay vertices into every face sharing the edge. Rim overrides go into the laterals (`rim_overrides_*`). | FAITHFUL | N25, N27 | `yr26_coplanar_boolean.rs::partial_overlap_union_is_watertight_16` |
| 4.5.5-09 | `:728-730` | "The boundaries of the common surface are regarded as intersection curves between the two models". | Implicit. Identical overlap triangles dedup into multi-label cells in cherchi, so their borders become A×B intersection edges. `ClassifiedOverlay::interface_polylines` (`coplanar_overlay.rs:302`) has no pipeline caller. Overlay seams have no n_A×n_B tangent, so §4.5.3 skips them (`reversal.rs:1470-1480`). | FAITHFUL | — | `yr26_coplanar_boolean.rs::partial_overlap_subtract_is_a_exactly` |
| 4.5.5-10 | `:730-731` | "and thus the Boolean operations can be conducted." | The membrane rule keeps or drops a multi-label sheet by op and `opposite` (`boolean.rs:2242`; `PairPlane.opposite` set at `stage0/mod.rs:481`). | SILENT | — | `yr26_coplanar_boolean.rs::pocket_subtract_opens_the_pocket` |
| 4.5.5-11 | `:724-725` | 2D Boolean yields "three parts" (positive-area overlap). | A zero-area overlap (an in-plane touch) skips the overlay, and cherchi passes the touch as benign (`stage0/mod.rs:964-969`). | SILENT | N17 | `yr25_coplanar_overlay.rs::shared_edge_adjacency_no_overlap` |
| 5.5-01 | `:956-959` | "first perform a 2D Boolean … on the coplanar pair of trim surfaces … then we discretize both models and ensure … identical … Afterward, the intersection is computed for the remaining parts". | `boolean.rs:1166` then Stage 1/2 in `boolean_once`, the same as rows 4.5.5-03/07. | FAITHFUL | — | `yr26_coplanar_boolean.rs::stacked_subtract_is_a_exactly` |
| 5.5-02 | Fig.20 `:973-977` | "we detect overlapping edges and vertices. Coincident edges and points are merged (a), resulting in identical mesh vertices". | `coplanar_overlay.rs:1419` `split_all` dedups shared collinear sub-segments (module doc `:46-51`). Cross-weld of coincident loop vertices is at `stage0/mod.rs:435`. The n-ary path uses `stage0/nary.rs:140` `weld_shared_corners`. | FAITHFUL | N25 | `yr25_coplanar_overlay.rs::collinear_partial_edge_overlap` |
| 5.5-03 | `:962-966,1031-1033` | 24 elliptical cylinders with coplanar caps unioned in sequence; many coplanar fragments mapped to the mesh. | The n-ary plane groups (`stage0/nary.rs:53` `build_plane_groups`) handle many faces per plane. **Elliptical caps hit the `face-unsupported` wall** (`frame.rs:94`, `:1016`), so this benchmark is not reproducible. | PARTIAL | N25; ellipse **N89** item 4 (ledgered 2026-10-17) | `crates/yang-rs/tests/m8_bridge_nary_overlay.rs::narrow_bridge_union_is_genus1_frame` |
| 5.5-04 | Fig.24(a) `:956-958`, `:1027-1028` | "generating a boundary-consistent mesh in the coplanar region". | Same mechanism as row 4.5.5-08. | FAITHFUL | N25 | `yr26_coplanar_boolean.rs::partial_overlap_union_is_watertight_16` |

**Row count: 35.** FAITHFUL 18, PARTIAL 9, DEVIATES 4, ORDER 1, SILENT 3, MISSING 0, N/A 0 (4.5.3-14 SILENT → PARTIAL 2026-10-17, F-3).

### Code with no paper row

These are mechanisms in the owned files that no row above or the plan's §2 table covers.

- **Stage-0 cavity relocation, flip repair and Fig-11 split** (~5.3k lines). Sites: `stage0/reloc.rs:476` `relocate_minted_vertex`, `:775` `relocate_minted_region`, `:288` `carve_star_cavity`, `:1365` `fig11_split_cavity`, `:1935` `fig11_slide_splice`. They are driven from `stage0/mod.rs:1597` (amendment 4, constrained Lawson flip), `:1691/1728` (amendment 5, cavity relocation), `:1773` (region form) and `:1924/1958` (amendment 14, Fig-11(a) SPLIT). They re-triangulate the overlay mesh around minted on-circle vertices. This is §4.4.1 Fig 11 mesh updating applied inside Stage 0, which the paper never needs because its overlay is the trimmed surface itself. The only ledger trace is N2's 2026-07-02 note naming "the Stage-0 fold-gate revert population" as a consumer. The machinery has no ledger entry and does not appear in `specs/yang_faithful_cleanup.md` §2. Plan §5 lists "Stage-0 §4.5.5 … not touched".
- **Sub-floor shared-mint collapse.** `stage0/mod.rs:1285` (increment 4) and the n-ary twin at `stage0/nary.rs:619`. Femto-twin overlay mints are merged to one target under A14.2. It compensates for the f64 lift of the exact overlay, which has no paper basis because the paper's overlay is not rounded. The ledger home is closest to N26.
- **`absorbable_sub_band_lifts`** (`stage0/mod.rs:207`). Absorbs rounding-noise lift clusters (TAU_WORK band). No paper basis; N26/N54 family.
- **`coplanar_overlay.rs:737` `sub_resolution_contract` and `:923` `fused_emission_repair`.** Collapse at the f64 emission gate. Recorded as N26, but N26's `Where` line cites (`:696`) are stale.
- **`stage0/mesh_build.rs:289/349` `detect_rim_table_fusions` / `apply_rim_table_fusion`.** Rim override fusion (N27 family). It compensates for the f64 Stage-1 rim samples colliding with overlay mints.
- **`stage0/rim_chords.rs:995` `opposite_rim_image` and `:1087` `refine_rim_membership`.** They propagate crossings onto the opposite rim of a lateral (N28/N79). This extends the Fig.16 "shared sampling" requirement across the lateral, beyond the coplanar face.
- **`stage0/edge_in_plane.rs:395` `identify_vertices` and `:786` `conformity_overrides`** (`boolean.rs:1068`, `:1306`). §4.5.5 applied one dimension down. N64, PROPOSED (an extension with no paper basis). Arm 2 runs only when Stage 0 is idle (`boolean.rs` P3a scope gate).
- **`stage0/cylinder.rs:416` `coincident_cylinder_stage0` and `:23` `detect_coincident_cylinder_pairs`** (`boolean.rs:1179`). §4.5.5 analog for coincident CYLINDERS. The paper covers planes only. **No dedicated ledger entry**: it is mentioned only in passing under N57's follow-ups and N4.
- **Intra-solid near-coplanar STOP** (`stage0/mod.rs:305`) and the **#178 sub-resolution STOP** (`:350`, N57). Paper silent: it assumes clean coplanarity between the two models.
- **Disc-pair direct builders** (`stage0/disc_pair.rs:71` `build_disc_pair`, `:237`, `:376` `build_identical_discs`). They bypass the overlay for circle discs (N25 scope). This is our implementation of "identical meshes" for arcs.
- **`reversal.rs:1313` `sub_feature_merge_direction`.** The §4.4.1(b) 3c-merge survivor rank (junction > conic endpoint > plain). It lives in the §4.5.3 file but has no §4.5.3 basis (N55).
- **`reversal.rs:936` `dedup_single_pp_line`.** A pp-line × circle junction helper (N30, §4.4.1). It also has no §4.5.3 basis.
- **Leftover kill switches.** `reversal.rs:1080` `open_conic_param_enabled` reads `YANG_441_OPEN_CONIC_PARAM`. `docs/kernel_probe_registry.md:70` says "decide with Phase 2a-ii", which landed 2026-10-16, but the switch is still there. Also `YANG_S1_SELF_CONTACT`, `YANG_S1_SELF_CONTACT_CHART` (`stage1_tessellate.rs:384`, `self_contact.rs` `chart_channel_on`), slated for Phase 3.

### Order of operations

**Paper order:** §4.5.5 2D Boolean and identical meshing, before discretization → discretization (self-intersections detected here, §4.5.4) → mesh intersection → §4.3 optimization → §4.5.3 reversal correction ("after convergence", before it corrupts the §4.4.1 updated mesh) → §4.4.1 mesh update → §4.5.4 detection of new intersections from updating → local refinement.

**Code order** (`boolean_once`, then `stage4_relocate_and_correct_inner` from `stage4_correct.rs:8918`):
1. edge-in-plane identify (`boolean.rs:1068`)
2. plane weld (`:1081`)
3. §4.3.3 generator-tangency rim boost (`:1112`ff)
4. **Stage 0** `stage0_preprocess` (`:1166`), else coincident-cylinder Stage 0 (`:1179`)
5. P3a / edge-in-plane conformity, only when Stage 0 is idle
6. Stage 1 (self-contact guard)
7. Stage 2, Stage 3
8. Stage 4: relocation → moved×mint weld (`:12975`) → beyond-corner trim (`:13035`) → fan re-triangulation (`:13046`) → **§4.5.3 sweep (`:13059`)** → 3c §4.4.1(b) merge (`:13240` region) → §4.3 weld (`:13325`) → **§4.4.1 `update_patches` (`:13338`)** → validation (`:13917`) → `collapse_subtauwork_mesh_edges` (`:14012`)
9. Stage 5/6
10. **§4.5.4 T3b/T3c on the emitted B-Rep** (`failures.rs:1279`) → `refine_loop` (`boolean.rs:621`)
11. kernel-v2 selfx STOP

**Inversions:**
- **§4.5.4 "new intersections during mesh updating"** are detected on the assembled output (after Stage 5/6), not on the updated mesh. The mesh-level census is test-only.
- **§4.4.1-type triangulation updates run BEFORE §4.5.3**: the beyond-corner trim and `retriangulate_collapsed_fan_regions`. The paper corrects reversal before any mesh update.
- **A §4.3.3 tangency mint** (`tangent_generator_rim_overrides`) runs before §4.5.5 Stage 0. The paper's §4.3.3 is a post-intersection step.
- **§4.3 point dedup** (the "too close" weld and the 3c merge) runs both before and after §4.5.3. The paper's §4.3 dedup belongs to optimization, before §4.5.3. This is minor.

### Findings (ranked)

1. **The Stage-0 overlay carries a whole unledgered §4.4.1 Fig-11 mesh-updating subsystem.** It covers cavity relocation, Lawson flips and Fig-11(a) split/slide (`stage0/reloc.rs:476,775,1365,1935`, driven at `stage0/mod.rs:1597-1958`). No N-entry describes it. The cleanup plan's §2 table omits it, and plan §5 declares Stage 0 "not touched". It is the largest substitute in this slice and is invisible to the paper-compliance debt count.
2. **The §4.5.3 failure paths cannot reach §4.5.2.** Three paths are affected: the 2·d_ε refusal (`reversal.rs:286`) silently leaves the reversal in place; `Stage4ReversalUnresolved` (`:316`) is never recorded; and the `max_passes` STOP uses vertex `u32::MAX` (`:40-45`). All three become faceless sites, so `refine_loop` cannot refine where a reversal persists. The paper's answer to under-resolution is refinement.
3. **§4.5.4 detection is at the wrong stage and is incomplete.** The mesh-after-updating census (`failures.rs:589`) is dead in production. Detection happens on the emitted B-Rep (T3b/T3c), which has named chart gaps: cone, sphere and torus, axis-wrapping cylinder loops, and planar faces in T3b (`failures.rs:818-823`, `:1132-1137`). This is ORDER plus PARTIAL. N6 still describes the 2026-07-17 two-layer state and does not mention T3b/T3c.
4. **There is an unledgered coverage gap in §4.5.3.** Parabola and Hyperbola intersection curves are never reversal-tested (`reversal.rs:1178` returns `None`, and the site `continue`s at `:226-229`). The angle-band arm also handles only Circle/Ellipse (`:135`).
5. **There is an unledgered §4.5.5 scope wall.** Ellipse-bounded planar faces are `face-unsupported` (`stage0/frame.rs:1016`), so the paper's own §5.5 Fig. 24(b) benchmark (24 elliptical cylinders) cannot run. The wall is covered only generically by N17's "loud" posture.
6. **The overtake arms (`reversal.rs:551`, `:650`) stand outside the paper's §4.5.3.** They bypass the paper-order removal, the endpoint guard and the 2·d_ε gate. `carried_curve_overtake_site` applies "reversal" to a single operand's B-Rep edge, which is not an S_A∩S_B curve. They are ledgered only as N29 amendments under a "strengthening" posture.
7. **Several ledger entries are stale:**
   - N29's `Where` block cites deleted functions (`reversal_collapse_direction`, `shared_conic_reversed`) and `stage4_correct.rs:3402`. Its Mechanism paragraph still says "victim collapses onto the parameter-NEARER neighbor", and so does the `conic_param_deltas` doc at `reversal.rs:1110-1115`.
   - N79 is a `##` heading that is **absent from the status index** (`docs/yang_deviations.md:84-185`). Its State line still cites the deleted `YANG_PLANE_WELD=off` kill switch.
   - N26's line cites (`coplanar_overlay.rs:696`) now point to the wrong place; `fused_emission_repair` is at `:923`.
8. **The coincident-cylinder Stage 0 has no dedicated ledger entry.** It extends §4.5.5 to curved surfaces (`stage0/cylinder.rs:416`, `boolean.rs:1179`) and is mentioned only under N57 and N4.
