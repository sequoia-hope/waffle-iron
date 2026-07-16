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

**Read:** ~15 cases route to Phase B (8 reassembly + 4 render-CDT + 3 re-entry-CDT),
~4 to Phase C/D (grazing), 5 eject (2 → #146, 1 → §4.5.3, 2 → M5). The Phase-B
reassembly bucket is the largest single lever and the ★★ hypothesis (post-relocation
non-manifoldness → §4.4.1 mesh-update fixes it) must be confirmed per-case as
Phase B lands — a case that survives §4.4.1 and stays non-manifold is a different
bug and re-triages out. Two ★ LRR cases (C0067, R0077) still need a routing probe
before they are assigned. The render-CDT ★ cases (F0045/R0011/R0016/R0028) need one
probe each to confirm the degenerate ring is a yang-emitted OUTPUT face (Phase B)
rather than a kernel-v2 render-tessellation bug (out of yang scope).
