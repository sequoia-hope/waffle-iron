# EPIC — Stage-4 mesh-updating + local-refinement loop (§4.4.1 + §4.5.2)

Status: **PLAN** (umbrella for #137, #168, and the refinement/reassembly tail).
Author date 2026-07-16. Supersedes the scattered framing in the two child specs;
they become consumers of the Phase-A foundation below.

## 0. Why this epic exists

The kernel corpus has been stuck near `241C / 0W / 49E` across many sessions. A
2026-07-15 classification of the ~54 op-level failures (memory
`feedback_stop_band_tuning_build_mesh_updating`) found **~45 are STRUCTURAL** and
only ~3-6 are tolerance work:

| bucket | n | this epic? |
|---|---|---|
| `LocalRefinementRequired` | 13 | PARTLY — the refinement sub-class (heterogeneous, see §4) |
| `non-2-manifold` reassembly | 12 | YES — broken bijectivity after relocation |
| `CDT backend / ring rejected` | 7 | YES — §4.4.1 CDT re-triangulation |
| `OffCurveBeyondChordBand` | 6 | PARTLY — C0065/R0074 grazing corners (#137) |
| others (InvalidBooleanOutput, NonPlanarFace, …) | ~16 | some |

The derived Stage-4 acceptance bands already did their job — they are why the
corpus is `0 WRONG` (they convert silent-wrong → loud STOP). But more bands move
~0 cases. The structural tail is blocked on ONE half-built capability: the paper's
**§4.4.1 mesh-updating + §4.5.2 local-refinement loop**, which the whole A15
architecture ("mesh gives exact topology → refine geometry analytically") is
premised on.

## 1. The paper's Stage-4 as ONE loop (the target)

Yang 2025 §4.3–§4.5 is a single relocate→update→recover loop, not a forward pass:

```
Stage 1 tessellate → Stage 2 mesh boolean → Stage 3 extract topology
loop:
  §4.3/4.4  relocate mesh intersection verts onto the analytic SSI curve
  §4.3.4    curvature-refine the curve polyline
  §4.4.1    MESH UPDATE: re-triangulate each trimmed patch (parametric-domain
            CDT, Fig 11 split/merge/insert) so the relocated curve IS the patch
            boundary — restores the broken bijectivity, both operands identically
  §4.5      detect failure regions (non-convergence / escape / reversal):
     §4.5.1   optimize across boundaries  (bounded, same surface)
     §4.5.2   LOCAL REFINEMENT (bounded, different patches): subdivide the mesh
              of the traversed patches + one-ring, re-intersect ONLY there,
              re-optimize
     §4.5.3   reversal correction
  repeat until converged, else loud STOP
Stage 5/6 assemble B-Rep via the bijection
```

§4.4.3: watertightness is *inherited from the mesh boolean* — the mesh update
keeps the B-Rep topology exactly aligned with the (watertight) mesh boolean.

## 2. What exists today (inventory)

| piece | file | state |
|---|---|---|
| relocate onto SSI curve (pair) | `stage4_relocate::relocate_onto_implicit_pair` | ✅ wired |
| relocate onto 3-surface junction | `…relocate_onto_implicit_triple` | ✅ wired |
| §4.5.1 optimize-across-boundaries | (#167) | ✅ shipped |
| §4.5.3 reversal correction | `stage4_correct/reversal.rs` | ✅ shipped |
| §4.4.1 Fig-11 parametric mesh-update primitive | `stage4_update::stage4_mesh_update` | ⚠️ built + unit-tested, **UNWIRED** |
| keep-interior CDT with holes | `cherchi_rs::cdt_polygon_with_holes_keep_interior` | ✅ built |
| per-patch degenerate re-CDT (#168) | `stage4_correct::replan_degenerate_cylinder_patches` | ⚠️ built, **gated off** `YANG_N2_RECDT_ENABLE`, blocked on two-sided conformality |
| torus∩plane∩plane corner junction (#137 N-137.1) | `stage4_relocate::torus_plane_clip_junction` | ⚠️ built + tested, **UNWIRED** |
| Phase-A two-sided conformal driver (common-frame) | `stage4_update::two_sided_conformal_update` | ⚠️ built + tested, **UNWIRED** |
| Phase-A→B frame-agnostic driver (two-surface, 3D-checked) | `stage4_update::two_sided_conformal_update_lifted` | ⚠️ built + tested (8 fixtures incl. dihedral two-surface), **UNWIRED** (Phase B wires behind `YANG_MESHUP_ENABLE`) |
| Phase-B per-operand parametric charts (Plane/Cylinder project+lift) | `stage4_project::SurfaceChart` | ⚠️ built + tested (round-trip + plane-tangent-cylinder integration w/ driver), **UNWIRED** |
| seam-vertex trace (`poly_vidx`) exposure | `stage4_update::stage4_mesh_update_traced` | ✅ built (byte-identical refactor of the primitive) |

**The current forward pass relocates vertices IN PLACE and skips §4.4.1** — it
never re-triangulates the patch to make the relocated curve the boundary. That is
the direct cause of the `non-2-manifold` reassembly bucket (relocation breaks the
bijective 1:1 map; the loop end dangles) and of the #137 grazing-corner defect.

## 3. The gap and the shared crux

Two things are missing, and both #137 (part b) and #168 (§5c.8) independently
stalled on the SAME wall:

**TWO-SIDED CONFORMALITY** — when a patch is re-triangulated/refined on operand A,
the adjacent patch on operand B (across the shared intersection curve) must be
re-triangulated with the IDENTICAL vertex set and chain along that curve, or the
reassembled mesh is non-manifold (unpaired half-edges). #168 solved the one-sided
seam reconstruction (§5c.7) but hit the two-sided wall (§5c.8: `(14,21)` fwd=1
rev=0). #137's corner stitch has the same requirement (spec §4 conformality note).

**Solve two-sided conformality ONCE as shared infra** → both children become
wireable. This is the linchpin of the whole epic and Phase A.

## 4. Target case inventory (honest, heterogeneous)

Per N52 (`session_2026_07_15_n52_lrr_rescope`), `LocalRefinementRequired` is a
catch-all STOP used by many sub-cases. Do NOT assume one machinery greens all 13.

- **Refinement / grazing (this epic, Phases C+D):** C0065, R0074 (torus∩plane
  corners, #137); C0067 (closed-torus); R0038 (plane-tangent-cylinder, #168→#137).
- **Reassembly / bijectivity (this epic, Phase B):** most of the `non-2-manifold`
  bucket — candidates C0044, C0058, F0058, F0060, F0082, F0085, R0049, R0095
  (triage each; F0064/R0051 may be the #146 Newell class, R0009 InvalidBooleanOutput).
- **CDT re-triangulation (this epic, Phase B/E):** F0045, F0067, R0011, R0016,
  R0028, R0085, R0100.
- **NOT this epic — route elsewhere:** R0044, R0096 = torus∩torus degree-4 SSI
  (M5 solver track); R0008/R0020/R0032/R0035/R0047/R0050/R0070 need per-case
  triage first (some are SSI-selection, some cone-apex/generator-parallel).

Deliverable of Phase 0 below is a per-case triage that assigns each STOP to a
phase or ejects it to another track — no case is assumed without a probe.

## 5. Phased plan (ordered; each gates the next)

### Phase 0 — Triage + failure-region detector
- Probe every `non-2-manifold` / `LocalRefinementRequired` / CDT case; assign to
  a phase or eject (M5 / #146 / SSI). Output: a case→phase table checked into this
  spec. (Cheap, high-value — prevents wasted work on mis-classified cases.)
- Build the §4.5 failure-region detector: after relocation, collect the
  point-pairs that did not converge / escaped / reversed, and the patches they
  traverse (the red/orange regions of Fig 14). Probe `YANG_MESHUP_REGION`.

### Phase A — Two-sided conformal patch re-triangulation (THE LINCHPIN)
- The shared primitive: given a patch pair on operands A and B sharing an
  intersection curve `C`, re-CDT BOTH in their parametric domains inserting the
  SAME ordered vertex set along `C`, producing watertight (paired) seam edges.
  Build on `cdt_polygon_with_holes_keep_interior` + `stage4_mesh_update` +
  the #168 seam-reconstruction (§5c.7) + the shared-vertex identity path
  (`yang_rim_junction_insertion`).
- De-risk with a MINIMAL two-patch unit fixture (two planes sharing one segment;
  then a curved pair) before touching any corpus case. Gate `YANG_MESHUP_ENABLE`,
  off by default; prove full-assay byte-identical off.
- Acceptance: the fixture reassembles with zero unpaired edges; #168's `(14,21)`
  seam pairs.
- **VALIDATED (2026-07-16, `stage4_update.rs` tests
  `two_patches_sharing_one_curve_get_conformal_seam` /
  `independent_seam_reconstruction_diverges`):** two-sided conformality is
  achieved by keeping the shared intersection-curve vertices as ONE identity set
  and re-triangulating each patch's INTERIOR only — driving both sides from the
  same curve keeps the seam identical even when the patch interiors differ;
  reconstructing the curve per-side diverges (the #168 §5c.8 mode). So the Phase-A
  driver's job is plumbing (feed both patches the one shared curve + interior-only
  keep-CDT), NOT a new geometric algorithm. This is the key de-risk: the primitive
  already supports it.
- **BUILT (2026-07-16, `stage4_update::two_sided_conformal_update`, UNWIRED):**
  the Phase-A driver. It runs `stage4_mesh_update_traced` (a byte-identical
  refactor of the primitive that also returns the per-point seam-vertex chain
  `poly_vidx`) on both operands' patches against ONE shared curve, pairs the two
  chains into a `seam: Vec<(va, vb)>`, and VERIFIES geometric conformality within
  a tight `conformal_tol` — a divergence is a LOUD `TwoSidedError::NonConformalSeam`
  (the #168 §5c.8 wall caught, never a silent unpaired half-edge). Fixtures pin:
  (a) two genuinely different patches sharing a chord → identical paired seam
  positions + seam edges present on both sides (manifold); (b) a merge that snaps
  one side's endpoint to an un-shared boundary vertex → loud NonConformalSeam;
  (c) per-side error propagation (SideA/SideB); (d) determinism. Both patches +
  curve are in ONE common frame (the de-risk contract); **Phase B supplies the
  per-operand parametric projection** that maps the 3D curve into each patch's
  own (u,v) domain, then calls this. Gated behind `YANG_MESHUP_ENABLE` at wire
  time; unwired now ⇒ production byte-identical.
- **BUILT (2026-07-16, frame-agnostic upgrade `two_sided_conformal_update_lifted`,
  UNWIRED):** the common-frame check only holds when both patches share one 2D
  frame — but the real forward pass puts the two adjacent patches on DIFFERENT
  surfaces (plane vs cylinder — cf. `replan_degenerate_cylinder_patches`'s (θ,z)
  projection), where the seam vertices coincide in **3D**, not in either 2D frame.
  So the driver now takes a per-side `lift: Point2 → Point3` and checks
  conformality in WORLD space; each side receives the ONE shared 3D curve
  projected into its OWN domain (`curve_a`/`curve_b`, same count+order, else a
  loud `SeamLengthMismatch`). `two_sided_conformal_update` is now the identity
  -lift (z=0) special case. De-risked on a dihedral two-perpendicular-planes
  fixture (A's chord vertical in (x,y), B's horizontal in (y,z) — genuinely
  different frames — all 3 paired seam verts lift to the SAME world point; a
  divergence where B samples at z=0.02 is caught as NonConformalSeam with the
  exact 0.02 world gap). **This resolves the Phase-A→B bridge's biggest unknown:
  the per-operand projection + that the conformal check lives in 3D.** Phase B's
  remaining work is now concrete: pull the per-operand surface projection/lift
  from the forward pass's `Surface`, extract the two adjacent patches + their
  shared mesh-vertex seam, call this, splice back.
- **BUILT (2026-07-16, `stage4_project::SurfaceChart`, UNWIRED):** the per
  -operand projection layer. `SurfaceChart::new(Surface)` returns a Plane or
  Cylinder chart (`None` for Sphere/Cone/Torus — the wiring skips those,
  byte-identical); `project` maps world→param (plane: in-plane ortho basis;
  cylinder: (θ,z)), `lift` maps param→world, mutual inverses on-surface.
  Tested: exact round-trip for both, plus an **integration test on the real
  #168 plane-tangent-cylinder pair** — the shared generator projected into each
  chart, re-CDT'd via `two_sided_conformal_update_lifted`, lifts back to a seam
  that coincides in 3D. This is the concrete "per-operand projection" the driver
  needed. **Phase B's ONLY remaining piece is the splice loop** in
  `stage4_relocate_and_correct`: for each patch pair that went non-manifold,
  build both charts, project the shared mesh-vertex curve into each, call the
  driver, rewrite the two patches' triangles in `Mesh` via the seam pairing —
  gated `YANG_MESHUP_ENABLE`, off → byte-identical.

### Phase B — Wire §4.4.1 mesh-update into the forward pass
- Replace relocation-only with: relocate curve → Phase-A conformal re-triangulate
  each affected patch → the relocated curve is now the exact boundary.
- Un-gate `replan_degenerate_cylinder_patches` (#168) on the Phase-A conformality.
- Target: the `non-2-manifold` reassembly + CDT buckets (per Phase-0 triage).
- Guardrail: every case that is byte-identical today MUST stay so with the gate
  off; with the gate on, 0 WRONG and no CORRECT regressions; sidecar parity.

### Phase C — §4.5.2 local-refinement loop
- For each failure region (Phase 0 detector): locally subdivide the traversed
  patches' tessellation + one-ring (NOT global n_seg — proven wrong/wasteful,
  `docs/yang_deviations.md` "#137 follow-up"), recompute the mesh intersection in
  the region only, re-optimize; repeat to a bounded depth, else loud STOP.
- Target: the grazing/near-tangency refinement cases (C0067, R0038, and set up
  #137 C0065/R0074).

### Phase D — #137 corner-junction assembly
- Using `torus_plane_clip_junction` (N-137.1, built) + Phase-A conformal stitch:
  pin the grazing corners, split both incident chains, trim the escaped arc.
- Green C0065/R0074 with the gate ON (see `specs/yang_137_torus_plane_grazing_corner.md`
  N-137.2…N-137.4, now reparented under this epic's Phase A).

### Phase E — Flip gates + un-quarantine
- Flip `YANG_MESHUP_ENABLE` / `YANG_N2_RECDT_ENABLE` to default-on once Phases
  B–D are green case-by-case; full assay + sidecar parity; un-quarantine the
  milestone-tagged `#[ignore]`/`test.skip` cases in the same PR.

## 6. Acceptance criteria (epic-level)

1. **`0 WRONG` invariant holds at every step** — the load-bearing STOPs never
   become silent-wrong (the sole non-negotiable; cf. #137 resolution sweep).
2. Phase B greens the triaged reassembly + CDT cases; Phase C+D green the
   refinement + corner cases. Each phase reports its actual case delta (no
   promised counts — triage governs).
3. Reference parity: refined/re-triangulated regions match the Cherchi C++
   sidecar (`reference_sidecar_available_here`).
4. No CORRECT regressions; gated byte-identical when off.

## 7. Guardrails (P9/P10)

- **Two-sided conformality is the gate on everything** — never wire a one-sided
  re-mesh (both #137 and #168 proved that path leaves the neighbour disagreeing →
  non-manifold). Phase A first, de-risked on fixtures.
- **No global refinement** — local only (§4.5.2); global n_seg is proven wrong
  without corner assembly and is wasteful.
- **No tolerance widening** to accept an escaped vertex — the mesh update makes
  the relocated curve the boundary EXACTLY; recovery is topological.
- **Triage before code** — every target case gets a probe verdict (Phase 0); M5
  torus∩torus and the #146 Newell class are ejected, not forced through here.
- Keep the child specs (`yang_137_torus_plane_grazing_corner`,
  `yang_n2_stage4_cdt_mesh_updating`) as the detailed per-part designs; this doc
  is the sequencing + shared-foundation contract.

## 8. Appendix — Phase 0 triage, first pass (2026-07-16)

From the release assay failure detail + probes (`YANG_TORUS_PROBE`) + memory
cross-refs. Confidence: ★★★ clear, ★★ likely, ★ needs a deeper probe before coding.

| case | failure detail | route | conf |
|---|---|---|---|
| C0044 | reassembled non-2-manifold (union) | Phase B | ★★ |
| C0058 | reassembled non-2-manifold (union) | Phase B | ★★ |
| F0058 | reassembled non-2-manifold (subtract) | Phase B | ★★ |
| F0060 | reassembled non-2-manifold (subtract) | Phase B | ★★ |
| F0082 | reassembled non-2-manifold (×2 union) | Phase B | ★★ |
| F0085 | reassembled non-2-manifold (union) | Phase B | ★★ |
| R0049 | reassembled non-2-manifold (subtract) | Phase B | ★★ |
| R0095 | reassembled non-2-manifold (subtract) | Phase B | ★★ |
| F0045 | render CDT: ring rejected (FaceId 9) | Phase B (confirm yang-output face vs kv2-render) | ★ |
| R0011 | render CDT: ring rejected (FaceId 407) | Phase B (boolean succeeds, OUTPUT face degenerate) | ★ |
| R0016 | render CDT: ring rejected (FaceId 1885) | Phase B | ★ |
| R0028 | render CDT: ring rejected (FaceId 32) | Phase B | ★ |
| F0067 | re-entry CDT fail (face 272) + azimuth-merge rims 572≠571 | Phase B + rim-sampling sub-bug | ★★ |
| R0085 | re-entry CDT fail (face 1) | Phase B (chained curved re-entry) | ★★ |
| R0100 | holed-lateral re-entry CDT fail (face 4) | Phase B (chained holed re-entry) | ★★ |
| C0065 | OffCurveBeyondChordBand (torus∩plane grazing) | Phase D (#137) | ★★★ |
| R0074 | OffCurveBeyondChordBand (torus∩plane grazing) | Phase D (#137) | ★★★ |
| R0038 | LRR u32::MAX (plane-tangent-cylinder) | Phase C/D (#168→#137) | ★★★ |
| C0067 | LRR v128 (closed-torus + on-axis sphere; torus block did NOT fire) | **PROBE** — M5 sphere∩torus (eject) vs refinement | ★ |
| R0077 | LRR v3 (torus∩plane; torus verts relocate to ~1e-13, STOP is a NON-torus-block site) | **PROBE** — Phase C or conic site | ★ |
| **EJECT** | | | |
| F0064 | non-2-manifold, but = #146 Newell off-plane class | → #146, not this epic | ★★ |
| R0051 | non-2-manifold, but = #146 Newell off-plane class | → #146 | ★★ |
| R0009 | InvalidBooleanOutput ellipse-endpoint | → §4.5.3 track (`yang_453_*`, R0009/R0091 `#[ignore]` RED) | ★★★ |
| R0044 | LRR — torus∩torus degree-4 | → M5 SSI solver track | ★★★ |
| R0096 | LRR — torus∩torus degree-4 | → M5 SSI solver track | ★★★ |

### 8b. Non-manifold bucket — detector triage (2026-07-16, `YANG_MESHUP_REGION`)

The `stage4_project::detect_nonmanifold_seams` probe (wired gated before the
Stage-4 `check_watertight_2manifold` gate) was run on the 8 reassembly cases. It
partitions the bucket sharply:

| case | Stage-4 detector fires? | region shape |
|---|---|---|
| C0044 | YES | ONE 3-patch junction: Plane(z=1) + 2 Cylinders — a triple corner |
| F0082 | YES | TWO regions, each a clean **two-plane** pair mismatch (charts ✓) |
| R0095 | YES | THREE regions: two-plane, **three-plane**, two-plane (charts ✓) |
| F0085 | (timeout — re-probe) | ? |
| C0058 | NO | non-manifold caught DOWNSTREAM (Stage 5/6), not the Stage-4 gate |
| F0058 | NO | downstream |
| F0060 | NO | downstream |
| R0049 | NO | downstream |

**Consequences (this refines §4/§8):**
1. **The bucket is heterogeneous.** Only ~half (C0044, F0082, R0095, F0085?)
   fail the Stage-4 half-edge gate — those are the genuine §4.4.1 targets. The
   other half (C0058, F0058, F0060, R0049) are manifold at the Stage-4 gate and
   fail LATER (Stage 5/6 cycle-walk `NonManifoldOutput`); a Stage-4 mesh-update
   will NOT fix them — they **re-triage out** of Phase B (need a separate probe:
   Stage-5/6 topology or a different defect).
2. **Two sub-shapes among the firing cases:** (a) clean **pairwise** two-patch
   seam mismatches (F0082, most of R0095) — the `two_sided_conformal_update_lifted`
   driver fits directly; (b) **3-patch junctions** (C0044 triple corner, one
   R0095 region) — the pairwise driver is insufficient; these need a triple
   -junction stitch (the #137 corner machinery / `torus_plane_clip_junction`
   pattern generalized). So the splice loop's FIRST target is the pairwise
   plane-plane subset (F0082), not C0044.
3. Every firing region so far has charts for all its patches (Plane/Cylinder), so
   the projection layer covers them.

### 8c. F0082 root cause → Phase-B RE-SCOPE (2026-07-16, local-topology dump)

Dumping the incident triangles of F0082's firing regions (enriched
`YANG_MESHUP_REGION` probe) shows the non-manifoldness is **NOT a two-sided
seam-subdivision mismatch** — it is a **spurious overlapping triangle inside ONE
planar patch**:

- Region 1 (face 306): `tri1217 [588,601,610]` and `tri1216 [588,609,610]` are
  BOTH incident to seam edge `{588,610}` in the SAME direction (`610→588`), so
  face 306 covers that seam edge TWICE (while face 302 covers `588→610` once).
  `tri1217` is a redundant sliver dangling out to junction vertex 601 (which is
  shared by faces 297/303/306, not by 302). One overlapping triangle within the
  patch → three imbalanced edges.
- Region 2 (face 29): identical shape — `tri156 [101,95,99]` overlaps within
  face 29, doubling edge `{95,99}`.

**Consequence — the Phase-B splice is a ONE-SIDED keep-boundary re-CDT, not the
two-sided curve-insertion driver.** The fix for a spurious-in-patch-triangle case
is to re-triangulate the offending patch's INTERIOR while keeping its boundary
(incl. shared seam verts) VERBATIM — which drops the overlap. This:
- is exactly what `replan_degenerate_cylinder_patches` +
  `cdt_polygon_with_holes_keep_interior` already do (generalize from
  degenerate-cylinder-caps to any charted planar/cyl patch, triggered by the
  detector instead of by degenerate caps);
- is **inherently two-sided-conformal** (the shared seam verts never move, so the
  neighbour still agrees — the #168 §5c.8 wall is avoided *because* we keep the
  boundary verbatim rather than reconstructing the seam chain);
- is **P10-safe**: keep-interior re-CDT moves NO geometry, so the worst case is a
  loud STOP, never a silent-wrong (the property `replan` already proved
  corpus-wide).

The **two-sided curve-insertion driver** (`two_sided_conformal_update_lifted`) +
`SurfaceChart` are therefore reserved for **Phase C/D** — where a relocated /
newly-refined curve is genuinely inserted into a curved patch pair (grazing /
near-tangency / #137 corner), which IS a two-sided insert. They are correct and
tested; they were just aimed at the wrong sub-problem for the Phase-B bucket.

**Revised Phase-B splice plan:** (1) detector supplies the region + patch keys;
(2) for each involved patch with a chart, extract its boundary loop from the flat
mesh (edges used once within the patch's own tri set, plus the cross-attribution
seam — the `replan` §5c.7 pattern), project into the chart, re-CDT the interior
keeping the boundary verbatim; (3) rewrite the patch's triangles in the mesh; (4)
gate `YANG_MESHUP_ENABLE` off → byte-identical; (5) target F0082 first (two clean
planar patches), then R0095's pairwise regions; 3-patch junctions (C0044) and the
downstream-failing cases stay out.

### 8d. F0082 splice attempt — mechanics work, residual is a near-degenerate junction (2026-07-16)

Built `remesh_nonmanifold_patches` (stage4_correct.rs): detector → for each 2-plane
region, re-CDT each patch's interior keeping the true (cross-attribution seam)
boundary verbatim — the boundary rule excludes spurious single-incidence edges, so
the overlap triangle is dropped. Wired gated behind `YANG_MESHUP_ENABLE` (off →
byte-identical; the fn is only reachable via the gate).

**Gate-ON result on F0082:** the remesh FIRES cleanly — all 4 patches (faces
302/306/29/32) re-CDT with valid degree-2 boundaries, dropping the overlap
(`tri1217`). BUT the case still errors: 2 of the 3 bad edges are fixed, leaving a
NEW unpaired edge `(588,601)`. v588=(0.1385,-0.0943,0.4742) and
v601=(0.1505,-0.0943,0.4731) are **only ~0.012 apart** — two distinct *junction*
vertices (588 ∈ faces 286/302/306; 601 ∈ faces 297/303/306) with v591 between them
on face 306's boundary. The single-patch keep-boundary re-CDT drops the spurious
triangle but the CDT bridges 588–601 across the near-degenerate junction, and the
neighbour faces (297/303) don't carry that edge → still non-manifold.

**Finding:** F0082's root defect is a **near-degenerate junction**, not merely a
spurious triangle. Single-patch re-CDT is necessary but not sufficient — greening
it needs junction-aware handling (coordinate the re-CDT across the faces meeting
at the 588/591/601 cluster, or resolve the near-duplicate junction verts). The
splice mechanics are correct and P10-safe; they are **banked gated-off**
(`YANG_MESHUP_ENABLE`) like #168's `replan`, pending the junction increment. Per
P9/P10 the residual is NOT hacked (no weld of the 0.012 pair — that is 4.6 % of
the model span, far above any coincidence tol).

### 8e. REFUTATION — the Stage-4 non-manifold bucket is near-duplicate / off-plane junction verts, NOT clean re-CDT targets (2026-07-16)

Probing the re-CDT boundary geometry (`YANG_MESHUP_RECDT` sharpest-triple dump)
REFUTES §8c/§8d's "keep-boundary re-CDT greens the bucket" hypothesis:

- **F0082 face 306:** boundary verts 591 and 601 are 0.012 apart in 3D but
  project ~4e-4 apart in face 306's plane — their separation is nearly ALONG the
  face normal, i.e. one vertex is slightly **off-plane** (the #146 Newell class).
  Faces 29/32 have boundary triples of area ~8e-20 / ~3.6e-21 — **near-duplicate
  vertices** (face 32: three verts within ~5e-8).
- **R0095:** EVERY re-CDT'd face (1,6,9,15,18,30,38,40,55) has a sharpest
  boundary triple of area ~1e-24…1e-27 — near-duplicate boundary verts
  throughout (the model is at ~1e-3 scale; the dup gaps are ~1e-6…1e-8).

**Conclusion:** the Stage-4-firing non-manifold cases are dominated by
**near-duplicate / off-plane junction vertices** produced upstream (arrangement /
Stage-0 / relocation), not by spurious triangles over a clean boundary. A
keep-boundary re-CDT projects those near-coincident boundary verts into
degenerate slivers → it CANNOT green them, and welding is barred (0.012 is 4.6 %
of F0082's span; the P10 line). So **Phase B (§4.4.1 re-CDT) does NOT green the
non-manifold bucket** — the real blocker is upstream degenerate junction geometry
(re-triage → the #146 off-plane-vertex track / a Stage-2/3 near-duplicate
resolution). The detector + splice are correct, P10-safe infrastructure (banked
gated-off `YANG_MESHUP_ENABLE`); they will apply once a genuine
spurious-triangle-over-clean-boundary case appears, or once the upstream
degeneracy is resolved. This is the "abort the fix, report what you learned"
guardrail: the Phase-B-greens-the-bucket diagnosis was wrong.

### 8f. Phase C gets its first MEASURED customer — and it is 16 folds, not 3 cases (2026-07-29)

The 2026-07-29 ring-fold triage routed R0074/R0011/F0045 here as "customers of the
mesh-updating epic". Verifying that anchor before building Phase C narrowed it
substantially. Full evidence in `docs/yang_tail_triage.md` §"SCOPED — the epic owns
16 of R0074's 78 folds"; the parts that bind THIS spec:

**What Phase C actually owns: R0074's 16 Stage-4-MINTED folds.** Re-evaluating each
fold's turn angle at the pre-Stage-4 vertex positions (the probe now records the
displacement VECTOR, so `pre = post − disp`) splits R0074's 78 folds into 16 that
Stage 4 minted (turn_pre 0.00° → 179.9x°) and **62 it merely inherited** from the
Stage-2/3 boundary cycle (already >120° beforehand; Stage 4 moved them a median
1.25°). §8's routing rested on "81 of 92 folds straddle a moved/still boundary",
which is a correlation — every one of the 78 has ≥1 moved vertex, so the straddle
test cannot separate minted from inherited. **Consequence: no phase of this epic
greens R0074**; its other 62 folds are an upstream (#146) defect. Report the fold
delta, not the case.

**The mechanism is Fig-11 `merge`, and it is a MERGE not a refinement.** For the
minted subset the relocation displacement exceeds the **pre-relocation spacing** of
adjacent chain vertices: median **3.85×**, max **81×** (spacing 9.101e-6 against a
7.404e-4 displacement — the known near-duplicate pair). The displacement is ~97%
NORMAL to the chain, so the order inversion is caused by its magnitude relative to
the spacing, not by sliding along the curve. Two vertices 9.1e-6 apart cannot be
independently projected 3e-4 onto the same curve and keep their order — no amount
of subdividing the incident edges fixes that, which **narrows §5's Phase-C plan**:
the first increment is Fig-11 `merge` (fuse a vertex within `merge_tol` of a curve
point instead of moving both), NOT the local-refinement loop. The primitive is
already built and unit-tested — `stage4_update::stage4_mesh_update` — and still
unwired, so this is a wiring increment, matching §7's "de-risk on fixtures first".

**Acceptance criterion for Phase C** (validated, replaces the 2026-07-29 triage's
first formulation): `max_displacement / min_pre_spacing < 1` at every relocated
boundary-chain vertex. Measured against the two known populations it is violated by
14/16 minted folds and respected by 56/62 inherited ones, so it discriminates
rather than merely correlating. `YANG_S5_FOLD_PROBE` reports it per fold, making
each increment checkable without a corpus run.

**Two of the three cases are NOT measurable yet — this blocks their triage.** F0045
and R0011 both take a §4.5.3 collapse (89→88 and 853→847 verts). The collapse
renumbers vertices, so the positional oracle reports UNAVAILABLE and neither
minting nor displacement can be measured for them. (The probe now prints
`turn_pre=NaN` there; previously the "pre" positions silently WERE the post
positions, so the number equalled `turn` exactly and read as "inherited fold".)
**Enabling increment: compose the `compact_unreferenced_verts` remap into
`S4_MOVED`** so the diff survives a collapse. Until then, do not assign either case
to a phase.

**F0045 re-routes OUT of this epic** to the boundary-curve-relocation spec: its 4
fold apexes are own-rim Fig-11 q triple points (`A:Cylinder+A:Plane+B:Cylinder`),
the F0083/v80 class. Measured so far is the INCIDENCE SIGNATURE (which is
definitional for q: on the operand's own rim AND on another operand's surface); that
its q is actually MIS-seated is not yet measured, because F0045's collapse blocks
the positional oracle. The cheap confirmation needs no pre/post state — dump the
apex's implicit residual against each of its three incident surfaces, the way the
F0083 probe did (`A:Plane` −5.55e-17, `B:Plane` 0.0, `A:Cylinder` −2.3046e-3 named
that defect outright). Do that before building the cylinder arm. `plan_triple_point_reseats` already handles q but skips these
because its closed form requires a `Plane` third surface
(`stage4_boundary_curve.rs:410`); the step is a rim-circle ∩ cylinder seat via the
existing `relocate_onto_implicit_triple` Newton under the existing
`satisfies_all_surfaces` certificate. R0011 is a third signature (0/10 apexes
own-rim; 6/10 `A:Cylinder+B:Plane` on an `Ellipse`).

### 8g. All three measured — R0011 is the lead case, and it is NOT a `merge` customer (2026-07-29)

§8f's blocking gap is closed. `S4_PRE_POS` stores each vertex's pre-Stage-4
POSITION and is re-keyed through all FOUR `compact_unreferenced_verts` sites
(§4.5.3, KV15b, #194, N50 f32 weld). Storing the position rather than the
displacement is load-bearing: `pre = post − disp` breaks if anything moves the
vertex again, and the last three sites run even when Stage 4 did not collapse.
**R0074's §8f numbers are confirmed index-aligned** (no remap site fires on it).

| case | folds | MINTED | INHERITED | minted ratio >1 | minted apex displacement |
|---|---|---|---|---|---|
| **R0011** | 10 | **10** | 0 | **10/10** (med 7.21) | 6/10 TANGENTIAL, up to 328 abs |
| R0074 | 78 | 16 | 62 | 14/16 (med 3.85) | 13/16 NORMAL (~97%) |
| F0045 | 4 | 1 | 3 | 1/1 | NORMAL |

The acceptance criterion generalizes: violated by **25 of 27** minted folds, respected
by **58 of 65** inherited ones.

**Sequencing change.** R0011 becomes the lead case — it is the only one whose folds
are 100% Stage-4-minted, so fixing the mint can actually convert it, whereas R0074
retains 62 inherited folds no matter what this epic does. **But R0011 is not a
Fig-11 `merge` customer.** Its minted displacements are tangential-dominant and
enormous (up to 328 on a ~5000-span model, ~7%); sliding 7% of the model ALONG a
curve is a wrong-point-on-curve selection, not off-curve error being corrected.
R0074's are ~97% NORMAL — genuine correction whose order breaks only because the
move dwarfs a near-duplicate spacing. So Fig-11 `merge` remains scoped to R0074's
near-dup class, and R0011 needs correct point selection along the curve instead.

**Do not read the printed `reloc(t=…)` as evidence of mis-relocation** — a vertex on
two curves carries one `t`, and `t` from different curve frames is incomparable
(R0011's adjacent verts 38/39 read `-0.428` and `+2.182`, which alone proves
nothing). Settled by §8h below.

### 8h. REFUTED — R0011's relocations are EARNED; it is a §4.5.2 case, and the ratio unifies both (2026-07-29)

§8g's point-selection reading is **refuted**. The probe now reports each fold
vertex's implicit residual against every incident surface at BOTH the final and the
pre-relocation position (`resid=` / `resid_pre=`). Every moved vertex on R0011 has a
LARGE pre-residual and a ~1e-13 post-residual — v34 84.68→1.8e-12, v38 107.5→9.1e-13,
v25 52.21→2.8e-14, v24 46.82→9.1e-13, v18 42.96→9.1e-13, v74 10.31→9.1e-13 — while
still vertices hold PRE ≡ POST at ~1e-13 (correctly left alone). **The destinations
are correct and the moves are earned.** A teleport to a different root of the same
constraints would show a SMALL pre-residual; none does.

The tangential dominance has a benign explanation: at a shallow-angle intersection
the mesh curve is offset from the true SSI curve largely ALONG the curve, so the
nearest true-curve point lies mostly in the chain direction. That is near-tangency —
**§4.5.2's own target**. R0011's minted folds are genuine local-refinement customers:
the mesh curve approximates the true curve so poorly that the correction (245)
exceeds the chain spacing (34), and a correction larger than the spacing can reorder
the polyline no matter how exact each individual destination is.

**Both sub-mechanisms belong to this epic, and the acceptance ratio unifies them:**

| case | sub-mechanism | spacing vs correction | Phase-C fix |
|---|---|---|---|
| R0011 | mesh curve poorly approximates the true curve (near-tangency) | 34 vs 245 | §4.5.2 LOCAL REFINEMENT + re-intersect |
| R0074 | near-DUPLICATE verts, small correction but huge relative to spacing | 9.1e-6 vs 3e-4 | Fig-11 `merge` |

Driving `max_disp / min_pre_spacing` below 1 is the shared criterion, and these are
the two ways to achieve it — shrink the correction by refining, or remove the
sub-spacing pair by merging. §5's Phase C therefore keeps BOTH arms; §8f's narrowing
to "merge first, not the refinement loop" applies to R0074 only.

**Ordering for the build.** Fig-11 `merge` is the smaller, better-de-risked arm (the
primitive exists and is unit-tested; it only removes a sub-spacing pair) and its
target R0074 cannot convert. §4.5.2 local refinement is the larger arm and owns the
only case that can convert (R0011, 10/10 minted). Do `merge` first as the wiring
de-risk — gated, byte-identical, fold-delta measured on R0074 — then refinement.
**⚠ The premise of this paragraph is WRONG — see §8i.**

### 8i. REFUTED — Fig-11 `merge` does not fuse two mesh vertices; R0074 has no merge arm (2026-07-29)

§8f and §8h both scoped R0074's fix as "Fig-11 `merge` — fuse a vertex within
`merge_tol` of a curve point instead of moving both independently". **Reading the
built primitive refutes that description of it.** `stage4_update.rs:176-234`
enumerates the four Fig-11 cases, and every one leaves the existing mesh vertices in
place:

- **boundary-VERTEX merge** — reuse the boundary vertex and **KEEP IT FIXED**; the
  CURVE POINT snaps onto it. (A regression test, `probe_merge_moves_boundary_vertex_
  off_edge_breaks_area` at `:901`, exists precisely because an earlier version DID
  drag the vertex and violated area conservation I4.)
- **boundary-EDGE split** — project the curve point onto the edge line; boundary
  unchanged.
- **interior merge** — move an INTERIOR patch vertex onto the curve point.
- **interior append** — a free interior curve vertex.

So `merge` governs how a CURVE POINT attaches to an existing triangulation. It never
fuses two mesh vertices to each other, and it never removes one. **R0074's fold needs
the opposite thing:** two MESH vertices 9.1e-6 apart are each relocated ~3e-4, and
the fix has to stop those two from being carried independently. `merge` leaves both.

**And no band-widening route is available.** The only pass that removes a near-dup
mesh edge is #194 `collapse_subtauwork_mesh_edges`, at TAU_WORK = 1e-12; R0074's pair
is 9.1e-6, seven orders above it. Raising that band is exactly the tolerance tuning
`feedback_stop_band_tuning_build_mesh_updating` bars, and §8e already recorded the
P10 line for welding near-duplicates.

**Consequence — the two arms collapse into one, and it is not `merge`.** What both
cases actually need is for the relocated intersection curve to be re-derived as a
PROPER POLYLINE along the analytic curve — the paper's §4.3.4 curve-polyline
refinement — with monotone parameterization, and the patch then re-triangulated to
match (§4.4.1). Relocating each mesh vertex independently and keeping the old
connectivity is the defect; it cannot be repaired by choosing better destinations
(§8h proved every destination is already exact) nor by attaching curve points to the
old vertices (`merge`, this section).

**This is a HYPOTHESIS, not a measured claim** — it is the only remaining structural
option after three eliminations, which is weaker evidence than a probe. Verify before
building: take one R0074 minted fold and check whether re-sampling that curve's
polyline monotonically, at the same vertex count, removes the fold; and confirm
`intersection_curves` is even populated for it (R0074 reports
`n_intersection_curves=0`, so its plane∩torus chains may have no analytic curve to
re-sample from — which would make Stage 3 curve construction the actual blocker and
would re-route this case again).

Nothing was built against the refuted framing.

### 8j. ANSWERED — R0074 has NO analytic curve to re-sample, by design; §8i is refuted for it (2026-07-29)

§8i flagged this as the specific way its hypothesis could still be wrong. It is wrong.

**1. The torus skip is deliberate and documented.** `stage3_ssi.rs:711-718` (KV6d
Tier B) `continue`s on ANY torus edge:

> a TORUS intersection edge is degree-4 — there is no analytic SSI curve
> (`surface_to_quadric` refuses a torus). Leave it as the `Curve::LineSegment`
> fallback; Stage 4 relocates its endpoints onto the exact torus∩surface curve via
> the implicit-pair Newton.

R0074 is entirely plane×torus, which is exactly why it reports
`n_intersection_curves=0` / `curve_kinds={}` and why all 78 folds read `NO-CURVE`.
This is a recorded capability boundary, not a defect.

**2. A second, redundant skip fires FIRST — worth recording for whoever lifts the
boundary.** The on-both-surfaces gate (`:615`) already rejects these edges before the
deliberate torus skip is reached, because the selection-tolerance ladder (`:559-593`)
has arms for Cylinder, Sphere and Cone but **none for Torus**, so `tol` falls through
to `TAU_WORK = 1e-12`. Measured with `YANG_V_PROBE=125,123,126` on R0074:

```
on-both gate SKIP edge (122,125) tol=1.000e-12
  surf0=Plane  surf1=Torus{major_radius: 0.17568, minor_radius: 0.11712}
  d_s=(0.000e0, 3.472e-4)   d_e=(0.000e0, 2.883e-4)
```

The endpoints are EXACTLY on the plane (0.000e0) and 2.806e-4–3.472e-4 off the torus
— its Stage-1 chord error, which the ladder is supposed to admit. Those numbers match
§8h's pre-residuals for the same vertices to every digit (v125 2.883e-4, v123
2.880e-4, v126 2.806e-4), cross-validating both probes. Harmless today because the
torus arm would skip anyway, but **anyone implementing a torus curve must add the
torus chord bound to the ladder first**, or the gate will silently eat every torus
edge. Note also that the `YANG_V_PROBE` "on-both gate SKIP" line misattributes the
cause for torus edges — the design decision at `:716` is the real reason.

**3. Promoting these edges to `Curve::SurfacePair` would NOT enable §8i either.** The
variant does accept a (Plane, Torus) pair — it holds yang `Surface`s (`geom.rs:144`) —
but its own contract rules out what §8i needs:

> There is no closed-form parameterization — endpoints come from the mesh edge,
> interior samples from downstream (kernel-v2) projection.

A monotone re-sampling needs a parameter to be monotone IN, and `SurfacePair` supplies
none. Nor can ssi-rs mint one here: every ssi-rs `SurfacePair` producer is
quadric-pair based and `surface_to_quadric` refuses a torus, so `ssi_rs::intersect` is
not even callable for this pair.

**⇒ R0074 re-routes OUT of Phase C.** Giving it a monotone polyline requires genuine
new capability — curve TRACING/marching on the implicit (plane, torus) pair (e.g.
poloidal/toroidal-angle marching with Newton projection per sample) — not the wiring
of any existing part. That is its own task, adjacent to the KV6d torus scope
(`docs/kv6d_torus_boolean_scope.md`) and M5, and it should not be smuggled into this
epic.

**⇒ R0011 is the only viable Phase-C target, and it is viable.** — **superseded by §8k: the
re-sample was RUN on R0011 and is a NO-OP there too.** It carries real
analytic `Ellipse` curves (28 on op1, 45 on op2; its fold apexes report
`curve=[… | Ellipse | …]`), which DO have a closed-form parameterization, so monotone
re-sampling is available there. Combined with §8g (R0011 is the only case whose folds
are 100 % Stage-4-minted, hence the only one that can convert), Phase C now has
exactly one grounded lead case and a testable first step: re-sample one R0011 minted
fold's ellipse chain monotonically at the same vertex count and check the fold clears.

### 8k. RUN — the monotone re-sample is a NO-OP on R0011; and §8f/§8g's own-rim counts were MY OWN probe artifact (2026-07-29)

§8j proposed the one remaining testable step: re-sample an R0011 minted fold's ellipse
chain monotonically at the same vertex count and see whether the fold clears. It was
run (`YANG_S5_CHAIN`, which walks each loop's maximal runs of consecutive edges sharing
a bit-identical ellipse and reports every vertex's exact `ellipse_param` in traversal
order, unwrapped across the atan2 seam before the monotonicity test).

**Result: all 31 ellipse chains on R0011 are MONOTONE** (`n_pos == 0 || n_neg == 0`,
31/31; run lengths 2–7 vertices). A monotone re-sample at the same vertex count
therefore reproduces the existing order exactly and **cannot clear any fold**.

**And it could not have, structurally: not one R0011 fold has both incident edges on an
ellipse.** The 10 folds split `LineSegment→Ellipse` ×4, `Ellipse→LineSegment` ×2,
`LineSegment→LineSegment` ×4. Every fold apex is a chain JUNCTION, never the interior
of a relocated chain — so an intra-chain re-parameterization is the wrong tool by
construction. **§8i/§8j's hypothesis is now refuted for BOTH cases, and Phase C has no
grounded lead case left in this bucket.**

**CORRECTION — §8f and §8g's own-rim statistics are wrong, and the error was in my own
probe.** Increment 1 stored the per-vertex incidence as `BTreeSet<String>` keyed on the
operand-qualified LABEL, which silently collapsed two DISTINCT surfaces sharing a label
(a vertex on two different `A:Plane`s) into one entry. Increment 2 changed the store to
`Vec<(String, Surface)>` deduped on `(label, surface)`. Same fold, same vertices, the
two binaries:

```
v3 (set):  inc=[B:Plane          | B:Plane                | A:Cylinder+B:Plane]
v5 (vec):  inc=[B:Plane+B:Plane+B:Plane | B:Plane+B:Plane+B:Plane | A:Cylinder+B:Plane+B:Plane]
```

Recomputed on the corrected output, the "own-rim = 0" claims invert completely:

| case | apex own-rim (≥2 distinct surfaces of ONE operand) | apex is Fig-11 q (own-rim AND cross) | published in §8f/§8g |
|---|---|---|---|
| R0074 | **67/78** | **66/78** (`A:Plane+A:Plane+B:Torus`) | "0/78 own-rim" ✗ |
| R0011 | **10/10** | **6/10** (`A:Cylinder+B:Plane+B:Plane`) | "0/10 own-rim" ✗ |
| F0045 | 4/4 | 3/4 | 4/4 ✓ (its labels differed, so the set did not collapse) |

**This UNIFIES the bucket instead of splitting it into three classes.** All three cases
are dominated by Fig-11 **q** points — a vertex on one operand's OWN RIM (two distinct
surfaces of that operand) *and* on the other operand's surface. That is the F0083/v80
class of `specs/yang_s4_boundary_curve_relocation.md`, and it is consistent with §8k's
other finding: the folds sit at the junction where a relocated cross-input chain meets
the operand's own rim chain, which is exactly what a q point is.

**But the obvious follow-up is already excluded.** §8h measured these q vertices as
satisfying ALL their incident surfaces to ~1e-13, so they are already seated at a valid
triple point and `plan_triple_point_reseats` would be a no-op on them even with the
cylinder/torus arm added. What §8h does NOT establish is whether each is at the
**nearest** valid triple point: a cylinder ∩ (plane∩plane line) has up to 2 roots, and
a vertex can be exactly on all three surfaces while sitting at the WRONG root. The
pre-residual only proves it had to move, not that it moved to the right one.

**THE next measurement, sharply defined:** for R0011's q apexes, solve the ≤2 roots of
`A:Cylinder ∩ (B:Plane₁ ∩ B:Plane₂)` in closed form and check whether the vertex landed
on the root NEAREST its pre-relocation position. This is exactly the test
`circle_plane_nearest_root` already encodes for inc-3's own geometry
(`stage4_boundary_curve.rs:290`), applied to a different surface triple. If it is at
the farther root, the defect is root selection at the junction and the fix is a
nearest-root constraint — cheap, local, and precedented. If it is at the nearest root,
the q vertices are correct and the fold must come from the chain that ARRIVES at them.

**METRIC CAVEAT, recorded because it inflates every fold count in §8f–§8k.** The
`turn > 120°` threshold is a proxy for "self-intersecting ring", not the thing itself,
and it conflates legitimate sharp corners with genuine retraces. R0074's turn
distribution is clearly bimodal — a 120–146° cluster of 63 folds, then a tail of 15 at
≥153° with 11 at ≥177° (a retrace). kernel-v2's own ring probe found **ONE** proper
self-crossing in R0011's 392-point ring, not 10. So "10/10 minted" and "16 minted"
count turn-angle outliers, and the confirmed-defect population is materially smaller.
Any future increment should be measured against the ring self-crossing count
(`KV2_RING_PROVENANCE`), with the turn angle used only to localize.

**Read:** ~15 cases route to Phase B (8 reassembly + 4 render-CDT + 3 re-entry-CDT),
~4 to Phase C/D (grazing), 5 eject (2 → #146, 1 → §4.5.3, 2 → M5). The Phase-B
reassembly bucket is the largest single lever and the ★★ hypothesis (post-relocation
non-manifoldness → §4.4.1 mesh-update fixes it) must be confirmed per-case as
Phase B lands — a case that survives §4.4.1 and stays non-manifold is a different
bug and re-triages out. Two ★ LRR cases (C0067, R0077) still need a routing probe
before they are assigned. The render-CDT ★ cases (F0045/R0011/R0016/R0028) need one
probe each to confirm the degenerate ring is a yang-emitted OUTPUT face (Phase B)
rather than a kernel-v2 render-tessellation bug (out of yang scope).
