# Spec: Yang §4.5.5 Identical-Footprint Coplanar Pairs

PR5 design note. PR4 (`coplanar-preprocess-555`) shipped instrumentation only.
This document captures the root cause Phase B uncovered and tracks the
two-PR fix split: PR5 implements §4.5.5 mesh-injection; PR6 implements the
downstream label_cells boundary-coincident classification.

## Status

- **PR4 outcome**: Phase A `[coplanar-tele]` counters in
  `crates/kernel/src/boolean/coplanar_preprocess.rs`. No fix attempt.
- **PR5 outcome (this PR)**: Yang §4.5.5 mesh-injection RESOLVED. New
  `inject_identical_footprint_mesh` helper produces bitwise-identical
  triangulations on identical-footprint coplanar pairs. Cherchi STAGE2
  dedup confirms (24 → 22 tris on the canary). Test
  `test_identical_footprint_inject_produces_consistent_meshes` is the
  green deliverable.
- **PR6 follow-up (still pending)**: Downstream `label_cells` classification
  of boundary-coincident triangles after Cherchi dedup. The canary
  `test_stacked_box_union_correct_topology` REMAINS RED until PR6 lands —
  the §4.5.5 piece improves it from 8 ambiguous twins → 2 ambiguous + 2
  unpaired, but does not fully resolve it. See "PR6 follow-up" below.
- **Research basis**: Yang et al. 2025 [#24] Section 4.5.5 (coplanarity
  handling) + §4.4.2 (binary inside/outside classification). The paper IS
  the spec — see `refs/yang2025_hybrid_boolean.pdf` lines 1281–1322 +
  Figure 16.

## Problem

`A ∪ B` for two axis-aligned unit boxes stacked along z (A = [0,1]³, B at
z ∈ [1,2]) fails topology validation with:

```
yang_boolean: result validation failed:
half_edge[1].twin = 0 but twin.twin = 0 (expected 1)
```

PR3's `[topo-extract]` instrumentation surfaces 8 ambiguous-twin events for
the 4 boundary edges of the z=1 cap (`(0→1), (0→3), (1→2), (2→3)` — each
flagged twice with 2 reverse candidates).

## Root cause

`crates/kernel/src/boolean/coplanar_preprocess.rs:216-224` — the
identical-footprint short-circuit:

```rust
if a_only_empty && b_only_empty {
    #[cfg(test)]
    eprintln!("[COPLANAR SPLIT]   -> Skipped: identical footprint (both A-only and B-only empty)");
    continue; // Identical footprint — tessellation already produces matching mesh
}
```

The trailing comment is a false assumption. Tessellation does NOT produce
matching meshes in the identical-footprint case:

- A's z=1 top face and B's z=1 bottom face have bitwise-identical corner
  positions (Cherchi STAGE1 confirms: 16 → 12 vertex collapse).
- BUT they triangulate independently — A's top picks one diagonal, B's
  bottom picks the other, because each tessellator runs on its own arena
  with its own canonical-form choice.
- Cherchi STAGE3 inserts 5 extra vertices (17 total) trying to make the
  conflicting interior edges conformal.

## Diagnostic evidence

Phase A telemetry on the failing canary:

```
[coplanar-tele] pairs=5 verts_existing=0 verts_split=0 verts_dropped=0
                mef_ok=0 mef_no_loop=0 overlay_groups=1 overlay_holes_ignored=0
```

All 5 detected pairs short-circuit before reaching `split_face_along_boundary`:

| Pair | Faces                | Plane     | same_dir | Skip reason            |
|------|----------------------|-----------|----------|------------------------|
| 0    | A face 0 / B face 1  | z=1       | false    | identical footprint    |
| 1    | A face 2 / B face 2  | y=0       | true     | no overlap             |
| 2    | A face 3 / B face 3  | x=1       | true     | no overlap             |
| 3    | A face 4 / B face 4  | y=1       | true     | no overlap             |
| 4    | A face 5 / B face 5  | x=0       | true     | no overlap             |

Pair 0 is the only pair where work was needed; it bailed via the bad skip.
Zero `mef` invocations and zero new vertices created — this rules out the
plan's hypothesis #1 (mef loop selection in `find_loop_containing_both_in_faces`).
The mef-loop bug may still exist for N≥4 overlap polygons but is not the
cause of this canary.

## Downstream symptom

After Cherchi STAGE3 mesh arrangement and flood-fill segmentation, two
patches survive at z=1:

- Patch 0: source = `SourceFace { mesh_id: A, face_idx: FaceIdx(0) }`, 2 tris
- Patch 11: source = `SourceFace { mesh_id: B, face_idx: FaceIdx(1) }`, 2 tris

These are two independent surfaces sharing the same boundary geometry.
`label_cells` reports:

```
[yang-diag] after label_cells: A outside=12 inside=0 cosurface=0,
                                B outside=12 inside=0 cosurface=0
```

`cosurface=0` for both meshes. The downstream cosurface-annihilation logic
(in `topology_extract.rs` survival selection) never fires because the meshes
were never made identical, so the survival selector emits BOTH A's top and
B's bottom as "outside" boundary faces.

The result: 4 forward boundary edges, each with 2 reverse-twin candidates
(one from A's surface, one from B's), → 8 ambiguous-twin events
→ `topology_extract` fails to pair half-edges
→ topology validation panic in `validate_solid`.

## Yang §4.5.5 prescription

> "The overlapping part is replaced by a trimmed common planar surface, and
> identical meshes are generated for both models in this part. The boundaries
> of the common surface are regarded as intersection curves between the two
> models, and thus the Boolean operations can be conducted."
> ...
> "The common part and the other two parts share identical sampling points
> on their boundaries." (Fig. 16 caption)

Identical-footprint coplanar pairs are the canonical case where this applies:
the common part IS the entire face. The mesh on both sides must be literally
identical (same vertex set, same triangulation, same winding orientation per
side). Skipping the work is wrong.

## Architectural fix candidates (PR5 scope)

These all require cross-solid coordination, which the plan classifies as
out-of-scope for PR4. Pick one in PR5 design phase:

### Option C — Cross-solid vertex-chain copy (engineer-a's lean)

For each identical-footprint pair, copy face A's existing boundary vertex
chain into face B (replacing B's independently-allocated boundary vertices).
After the copy, both faces share the same boundary VertexIdx values. When
tessellation runs, both faces triangulate from the same anchored basis and
produce identical interior diagonals.

- **Pros**: closest match to Yang's "identical sampling points" language.
- **Cons**: requires a shared vertex pool or a vertex-replacement Euler
  operator that doesn't currently exist. Two-arena architecture treats
  solids independently.

### Option B — Cosurface marker + downstream consumer

Mark coplanar pairs in solid metadata (e.g., a side table mapping
`(mesh_id, face_idx) → cosurface_partner`). Modify `label_cells` to assign
`cosurface` labels for tris belonging to marked faces. Modify
`select_boolean_result` to handle cosurface annihilation per the boolean
operation: Union keeps one side, Intersect keeps one side, Subtract drops
both.

- **Pros**: doesn't touch the Euler-operator topology layer. Works with
  the existing `cosurface` label vocabulary.
- **Cons**: doesn't solve the underlying mesh-identicality problem — relies
  on label_cells correctly seeing through Cherchi's STAGE3 vertex insertions.
  May need both this AND Option C.

### Shared canonical triangulation

Replace both face triangulations with a single canonical tessellation
anchored on the plane (deterministic vertex ordering, deterministic
diagonal choice), copy that triangle set into each solid's mesh
verbatim. Effectively does pre-tessellation work that bypasses the
per-solid tessellator for these faces.

- **Pros**: solves the problem at the right layer (tessellation).
- **Cons**: invasive. Tessellation pipeline currently runs per-solid;
  injecting a shared canonical triangulation requires either a
  pre-tessellation hook or a post-tessellation patch on both meshes
  (similar to the dead-code `inject_conformal_coplanar_mesh` in
  `coplanar_preprocess.rs:584` that PR4 leaves untouched).

## Sub-issues observed (low priority for PR5)

### Spurious side-face detection

Pairs 1–4 are flagged as coplanar (matching plane equations) but the side
faces of the two boxes occupy disjoint Z spans (A side at z ∈ [0,1], B side
at z ∈ [1,2]). They share no 2D footprint on their shared plane. The pairs
get correctly short-circuited later by the `no overlap` check, but
`detect_coplanar_face_pairs` could pre-filter via i_overlay to avoid the
wasted work and noise in `[coplanar-tele] pairs=`.

A more discerning detector would use the plane's 2D AABB (or full polygon
overlap) to confirm shape co-location, not just plane co-incidence. Telemetry
counter `COPLANAR_OVERLAY_GROUPS` already surfaces this — `pairs=5
overlay_groups=1` reveals 4 pairs that were detected but found no overlap.

### Identical-footprint counter

PR5 should add a dedicated counter `COPLANAR_IDENTICAL_FOOTPRINT` to the
existing `[coplanar-tele]` line, incremented in the
`a_only_empty && b_only_empty` block. Lets us measure the frequency of this
case across the assay corpus before designing the fix.

## Test that surfaces this

`crates/kernel/src/boolean/coplanar_preprocess.rs` —
`test_stacked_box_union_correct_topology`. Stays red until PR6 lands the
label_cells fix; PR5 reduces severity from 8 → 4 ambiguous half-edges but
does not turn it green. P9 enforces: do not silence, do not widen
tolerance, do not add a fallback path.

## PR5 resolution — §4.5.5 mesh-injection

**Branch**: `coplanar-identical-footprint`. **Commit**: TBD (filled in by
team-lead at merge).

### What landed

1. `CoplanarFacePair` gained a `is_identical_footprint: bool` field.
2. `split_brep_for_coplanar_pairs` takes `&mut [CoplanarFacePair]` and
   sets `is_identical_footprint = true` in the previous "skip identical
   footprint" short-circuit (where `a_only_empty && b_only_empty`). No
   B-Rep splitting happens for identical-footprint pairs (no edges to
   split, no `mef` calls — the existing face IS the overlap).
3. New `pub(crate) fn inject_identical_footprint_mesh(...)` reuses the
   pre-existing dead-code helpers (`find_plane_triangles`,
   `extract_face_boundary_2d`, `triangulate_polygon_with_holes`,
   `verts_2d_to_3d`, `replace_face_triangles`). For each marked pair:
   triangulates face A's 2D boundary ONCE, maps back to 3D ONCE,
   replaces both meshes' face triangles with the canonical set. B's
   per-triangle winding is reversed when `same_direction == false`
   (anti-parallel canonical case). T-junction repair is skipped —
   identical-footprint = no adjacent faces with mismatched edges.
4. Wired in `yang_boolean_inner` after tessellation/dedup/bijective-map
   build (the previous "Stage 0b: DISABLED" comment block).
5. Telemetry: `COPLANAR_IDENTICAL_FOOTPRINT` counter; new
   `identical_footprint={I}` field on the `[coplanar-tele]` summary
   line.

### Mechanical proof of §4.5.5 compliance

Phase D regression test:
`test_identical_footprint_inject_produces_consistent_meshes`. Asserts
that mesh A and mesh B have **bitwise-identical** vertex positions
(`f64::to_bits()` equal) and matching triangle vertex-position triples
(modulo winding) on the shared plane. Red phase (with the inject call
elided) prints divergent diagonals; green phase passes.

Cherchi STAGE2 dedup behaviour also confirms: on the canary, with
PR5's inject, STAGE2 reduces 24 → 22 tris (2 antiparallel duplicates
correctly dropped). Without PR5, STAGE2 keeps all 24 because the
diagonals diverge.

## PR6 follow-up — label_cells boundary-coincident classification

PR5 leaves the canary failing with a diagnostically narrower defect:

- **Before PR5** (baseline): 8 ambiguous-twin events on the 4 z=1
  boundary edges. Validation: `half_edge[1].twin = 0 but twin.twin = 0`.
- **After PR5**: 2 ambiguous + 2 unpaired half-edges. Validation:
  `half_edge[6].twin = 0 but twin.twin = 10 (expected 6)`.

### Root cause for PR6

After Cherchi STAGE2 correctly dedups the antiparallel duplicate
triangles, **only one mesh's surface representation survives** on the
shared plane. On the canary that surviving triangle belongs to mesh B's
face 1 (A's face 0 was the duplicate that got dropped). `label_cells`
ray-casts and labels the surviving triangle ambiguously: it's
boundary-coincident to A, so a strict ray-cast can flip either way.

For UNION of stacked boxes, the z=1 cap should NOT survive in the
result (it's interior). Yang §4.4.2's binary inside/outside
classification expects coplanar-shared surfaces to be REMOVED for
union. Our `select_boolean_result` no longer does this because PR1
(`cfc7b8`) deleted the CoSurface label vocabulary; today
`cosurface=0` after labeling and the surviving triangle is treated as
a regular outward-facing surface.

### Diagnostic snapshot (PR5 state)

```
[cherchi-trace] STAGE1 merge: 12 verts, 24 tris
[cherchi-trace] STAGE2 degenerate: 22 tris       # 2 antiparallel dupes dropped
[cherchi-trace] STAGE3 soup: 17 verts, 31 edges, 22 tris
[yang-diag] after label_cells: A outside=10 inside=2 cosurface=0,
                                B outside=12 inside=0 cosurface=0
[flood_fill DIAG Step5a] 11 patches:
  ... (A face 0 missing — correctly dropped) ...
  Patch 10: source=SourceFace { mesh_id: B, face_idx: FaceIdx(1) } tris=2
[topo-extract] ambiguous twin for (VertexIdx(4) -> VertexIdx(5)): 2 reverse candidates
[topo-extract] unpaired forward HE (VertexIdx(4) -> VertexIdx(6)): no reverse candidate
[topo-extract] ambiguous twin for (VertexIdx(5) -> VertexIdx(7)): 2 reverse candidates
[topo-extract] unpaired forward HE (VertexIdx(6) -> VertexIdx(7)): no reverse candidate
[topo-extract] summary: paired=18, unpaired=2, ambiguous=2
A ∪ B should succeed: NotSupported {
  operation: "yang_boolean: result validation failed:
              half_edge[6].twin = 0 but twin.twin = 10 (expected 6)" }
```

### PR6 scope (proposed; out of PR5's branch boundary)

PR6 needs to make `label_cells` (or the survival selection step in
`select_boolean_result`) recognize triangles that lie exactly on a
boundary shared with the OTHER mesh — those triangles need to be
classified by Yang §4.4.2's "infinitesimal inward perturbation" rule
rather than by raw ray-cast. Concretely: if a surviving triangle is
coplanar with a boundary that Cherchi STAGE2 deduplicated away, it's
either "shared boundary, drop" (Union) or "shared boundary, keep one"
(Intersect/Subtract). Implementation likely needs a side-channel from
Cherchi STAGE2 telling label_cells which triangles were "the
survivors" of an antiparallel-duplicate pair. PR1 deleted CoSurface
variants from the label vocabulary; PR6 must either reintroduce them
or thread the information through differently. Architectural
discussion deferred to PR6's design phase.

### Files PR6 will touch (out of PR5 branch)

- `crates/kernel/src/boolean/exact_mesh.rs` (label_cells, Cherchi STAGE2)
- `crates/kernel/src/boolean/topology_extract.rs` (survival selection)

## References

- Yang et al. 2025 [#24] §4.5.5 — the paper, the spec.
- Yang et al. 2025 [#24] §4.4.2 — binary inside/outside classification
  (PR6 deliverable).
- Cherchi et al. 2020 §5.4 — coplanar triangle handling reference.
- `specs/yang_coplanar_preprocessing.md` — the original Stage 0 design.
- `specs/yang_hybrid_migration.md` — overall pipeline migration plan.
- `/tmp/pr4_phaseB.log` — full diagnostic trace (361 lines, ephemeral).
- PR4 commit (coplanar-preprocess-555) — Phase A counters.
- PR5 commit (coplanar-identical-footprint) — §4.5.5 mesh-injection.
