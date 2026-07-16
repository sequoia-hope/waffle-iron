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
