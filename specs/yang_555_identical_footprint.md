# Spec: Yang §4.5.5 Identical-Footprint Coplanar Pairs

PR5 design note. PR4 (`coplanar-preprocess-555`) shipped instrumentation only —
this document captures the root cause Phase B uncovered, so PR5 can implement
the fix.

## Status

- **PR4 outcome**: Phase A `[coplanar-tele]` counters in
  `crates/kernel/src/boolean/coplanar_preprocess.rs`. No fix attempt.
- **Tracking signal**: `boolean::coplanar_preprocess::tests::test_stacked_box_union_correct_topology`
  remains red. Per P9, this test is NOT silenced — it stays as the canary
  that turns green when PR5's architectural fix lands.
- **Research basis**: Yang et al. 2025 [#24] Section 4.5.5 (coplanarity
  handling). The paper IS the spec — see `refs/yang2025_hybrid_boolean.pdf`
  lines 1281–1322 + Figure 16.

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

`crates/kernel/src/boolean/coplanar_preprocess.rs:1407` —
`test_stacked_box_union_correct_topology`. Currently red; stays red until
PR5 lands the fix. P9 enforces: do not silence, do not widen tolerance,
do not add a fallback path.

## References

- Yang et al. 2025 [#24] §4.5.5 — the paper, the spec.
- `specs/yang_coplanar_preprocessing.md` — the original Stage 0 design.
- `specs/yang_hybrid_migration.md` — overall pipeline migration plan.
- `/tmp/pr4_phaseB.log` — full diagnostic trace (361 lines, ephemeral).
- PR4 commit (coplanar-preprocess-555) — Phase A counters.
