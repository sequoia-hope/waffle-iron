# Spec: Yang §4.5.5 Identical-Footprint Coplanar Pairs

PR5 design note. PR4 (`coplanar-preprocess-555`) shipped instrumentation only.
This document captures the root cause Phase B uncovered and tracks the
two-PR fix split: PR5 implements §4.5.5 mesh-injection; PR6 implements the
downstream label_cells boundary-coincident classification.

## Status

- **PR4 outcome**: Phase A `[coplanar-tele]` counters in
  `crates/kernel/src/boolean/coplanar_preprocess.rs`. No fix attempt.
- **PR5 outcome**: Yang §4.5.5 mesh-injection RESOLVED. New
  `inject_identical_footprint_mesh` helper produces bitwise-identical
  triangulations on identical-footprint coplanar pairs. Cherchi STAGE2
  dedup confirms (24 → 22 tris on the canary). Test
  `test_identical_footprint_inject_produces_consistent_meshes` is the
  green deliverable.
- **PR6 outcome (this PR)**: Hoffmann classifier (label_cells
  boundary-coincident) RESOLVED. `label_sub_tri_raycast` in
  `crates/kernel/src/boolean/exact_mesh.rs` now applies Hoffmann 1989 §5.3
  perturb-and-classify when the primary ray-cast returns degenerate
  (centroid on target surface): both `+eps * normal` and `-eps * normal`
  are sampled; differing classifications → boundary-coincident → `Inside`
  (closed-solid convention compatible with `select_boolean_result`'s
  Union/Intersect/Subtract keep tables). Regression test
  `test_label_cells_boundary_coincident_classifies_inside` is the green
  deliverable. Commit: `<TBD-by-team-lead-at-merge>`.
- **PR8 (or PR9) follow-up — STILL PENDING**: PR5 inject overlap polygon
  geometry bug. PR6 surfaced it via the canary-still-red diagnostic.
  See "PR8 follow-up" below.
- **Canary status**: `test_stacked_box_union_correct_topology` STAYS RED
  until the PR8 inject geometry bug is fixed. Do NOT silence the canary;
  it now tracks the next compliance gap (PR3 surfaced twin-pairing; PR5
  improved to 2 ambiguous + 2 unpaired; PR6 ships correct Hoffmann
  classification; PR8 will fix inject geometry).
- **Research basis**: Yang et al. 2025 [#24] Section 4.5.5 (coplanarity
  handling) + §4.4.2 (binary inside/outside classification) +
  Hoffmann 1989 §5.3 (perturb-and-classify, the canonical CSG technique
  Yang's binary classification implicitly relies on for boundary cases).
  The paper IS the spec — see `refs/yang2025_hybrid_boolean.pdf` lines
  1281–1322 + Figure 16.

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
`test_stacked_box_union_correct_topology`. Stays red until PR8 fixes
the inject geometry bug. Severity progression:

- Pre-PR5 baseline: 8 ambiguous-twin events.
- After PR5 (§4.5.5 mesh-injection): 2 ambiguous + 2 unpaired
  half-edges.
- After PR6 (Hoffmann boundary-coincident classification): 2
  ambiguous + 4 unpaired half-edges, exposing the PR8 inject geometry
  defect (overlap polygon at y∈[-1,0] instead of y∈[0,1]).
- After PR8 (inject geometry fix, planned): expected GREEN.

P9 enforces: do not silence, do not widen tolerance, do not add a
fallback path. The canary tracks the next compliance gap at each PR.

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

## PR6 resolution — Hoffmann boundary-coincident classification

PR5 left the canary failing with a diagnostically narrower defect:

- **Before PR5** (baseline): 8 ambiguous-twin events on the 4 z=1
  boundary edges. Validation: `half_edge[1].twin = 0 but twin.twin = 0`.
- **After PR5**: 2 ambiguous + 2 unpaired half-edges. Validation:
  `half_edge[6].twin = 0 but twin.twin = 10 (expected 6)`.

### Root cause addressed in PR6

After PR5's identical-mesh injection, Cherchi STAGE2 correctly dedups
the antiparallel duplicate triangles, leaving exactly ONE surviving
boundary-coincident triangle on the shared plane. `label_cells`'s
`label_sub_tri_raycast` previously offset along a single direction
(`-eps * normal`) when the primary ray-cast returned `None` (degenerate,
centroid on target surface). For anti-parallel boundary cases, that
single direction picked one side of the surface arbitrarily and could
return `Outside`, causing the boundary triangle to survive Union
selection (`keep = Outside`) and produce asymmetric half-edge twins.

### What landed in PR6

`crates/kernel/src/boolean/exact_mesh.rs:label_sub_tri_raycast`
replaces the single-direction offset with **Hoffmann 1989 §5.3
perturb-and-classify**:

1. Primary `ray_cast_inside(centroid)` — returns immediately for
   `Some(_)` cases (interior/exterior; the common path, zero impact on
   existing classifications).
2. If `None` (degenerate): sample BOTH `+eps * normal` and
   `-eps * normal` along the sub-triangle's own normal.
3. Differing classifications → boundary-coincident → return
   `CellLabel::Inside` per the closed-solid convention. This integrates
   correctly with `select_boolean_result`:
   - Union (keep=Outside): Inside boundary triangle dropped → boundary
     surface eliminated.
   - Intersect (keep=Inside): Inside boundary triangle kept →
     intersection surface preserved.
   - Subtract (keep_a=Outside, keep_b=Inside flipped): B's boundary
     triangle kept with flipped winding → correct subtraction surface.
4. Agreeing classifications → use the agreed result (primary was
   unlucky on a grazing edge, not boundary-coincident).
5. Both perturbations also degenerate → GWN fallback on centroid
   (defense-in-depth).

The doc comment cites Hoffmann 1989 §5.3 + Yang 2025 §4.4 explicitly
per P8.

### Mechanical proof of correctness

`test_label_cells_boundary_coincident_classifies_inside` in
`exact_mesh.rs` `mod tests`. Setup: target = unit cube A; sub-triangle
vertices `[(0,0,1), (0,1,1), (1,0,1)]` with normal `-z` (anti-parallel
to A's z=1 outward normal `+z`); centroid exactly on A's z=1 face.

- Red phase (single-direction-offset baseline): `Outside` (the buggy
  result for anti-parallel boundary cases). Captured as part of PR6's
  red-before-green compliance with FIP §8.
- Green phase (Hoffmann two-sided): `Inside` (boundary-coincident).

### PR6 did NOT close the canary — see PR8 follow-up

PR6 ships correct Hoffmann classification, full stop. It does NOT
make `test_stacked_box_union_correct_topology` go green. The canary
diagnostic data revealed a SEPARATE upstream bug in PR5's inject
geometry; see the PR8 follow-up section below for details.

Per `feedback_no_last_bug.md`: PR6 is not "the fix" for the canary.
It fixes Hoffmann classification, full stop.

## PR8 follow-up — PR5 inject overlap polygon geometry bug

PR6's diagnostic surfaced a SEPARATE bug in PR5's identical-footprint
inject pathway. The canary STAYS RED until PR8 fixes inject geometry.

### Smoking-gun diagnostic

PR5's `[COPLANAR SPLIT]` log on the canary:

```
[COPLANAR SPLIT] Processing pair 0: face_a=FaceIdx(0) face_b=FaceIdx(1)
                 normal=[0.0000,0.0000,1.0000] offset=1.000000 same_dir=false
[COPLANAR SPLIT]   Overlap polygon: 4 vertices
[COPLANAR SPLIT]     ov0: (0.000000, 0.000000)
[COPLANAR SPLIT]     ov1: (0.000000, -1.000000)
[COPLANAR SPLIT]     ov2: (1.000000, -1.000000)
[COPLANAR SPLIT]     ov3: (1.000000, 0.000000)
[COPLANAR SPLIT]   -> Marked identical_footprint=true (will inject post-tessellation)
```

A is the unit cube `[0,1]³`; its z=1 face footprint in 2D should be
`[0,1] × [0,1]` (or a sign-flipped variant of the same). The reported
overlap polygon has y-coordinates `0` and `-1`, NOT `0` and `+1`.
The vertices `(0,-1)` and `(1,-1)` are OUTSIDE A's actual XY footprint.

### Hypothesis

2D-basis sign flip in `extract_face_boundary_2d` or
`compute_plane_basis` for B's anti-parallel face. B's z=1 face has
outward normal `-z`, so the canonical 2D basis chosen for it likely has
its v-axis pointing in `-y` (instead of `+y`). When the inject code
extracts B's boundary and intersects it with A's boundary in the
"shared 2D frame", the frame happens to be B's frame (because B was
the anti-parallel face under §4.5.5 conventions), so A's `[0,1] × [0,1]`
gets re-expressed as `[0,1] × [-1,0]` in B's frame.

The 3D-projection roundtrip via `verts_2d_to_3d` may be canceling the
basis sign so the inject TRIANGLES land in the right 3D location
geometrically (which is why PR5's
`test_identical_footprint_inject_produces_consistent_meshes` passes —
that test only checks bitwise vertex equality and triangle equivalence,
both of which can hold even if the canonical 2D coordinates differ from
A's original footprint).

### Why this exposes a §4.5.5 compliance gap

Yang §4.5.5 requires "identical sampling points" — the canonical mesh
must REPLACE both A's and B's z=1 face triangulations with the same
canonical triangle set. PR5's inject produces canonical triangles whose
vertex POSITIONS are correct, but Cherchi STAGE2 dedupes against A's
ORIGINAL z=1 face triangulation (which was canonical-mapped from A's
own basis). If the bits are subtly different (vertex order, winding,
or one triangle's diagonal), STAGE2 dedups one duplicate but leaves
another non-shared triangle in place.

### Diagnostic data from PR6 canary run (with Hoffmann fix applied)

```
[cherchi-trace] STAGE1 merge: 12 verts, 24 tris
[cherchi-trace] STAGE2 degenerate: 22 tris       # 2 dupes dropped
[yang-diag] after subdivide: tris_a=12, tris_b=12, verts=12
[yang-diag] after label_cells: A outside=11 inside=1 cosurface=0,
                                B outside=12 inside=0 cosurface=0
[yang-diag] after survival: 12 groups, 23 tris
[flood_fill DIAG Step5a] 12 patches:
  Patch 0: source=SourceFace { mesh_id: A, face_idx: FaceIdx(0) } tris=1
  ... (A FaceIdx(0) is now PARTIALLY surviving — only 1 of 2 sub-tris) ...
  Patch 11: source=SourceFace { mesh_id: B, face_idx: FaceIdx(1) } tris=2
[topo-extract] summary: paired=18, unpaired=4, ambiguous=2
A ∪ B should succeed: NotSupported {
  operation: "yang_boolean: result validation failed:
              half_edge[2].twin = 0 but twin.twin = 0 (expected 2)" }
```

Comparison to the PR5-only baseline (without PR6's Hoffmann fix):

| Metric                | PR5 baseline | PR5 + PR6 (Hoffmann) |
|-----------------------|--------------|----------------------|
| A label_cells inside  | 2            | 1                    |
| A label_cells outside | 10           | 11                   |
| B label_cells inside  | 0            | 0                    |
| B label_cells outside | 12           | 12                   |
| Surviving sub-tris    | 22           | 23                   |
| Patches               | 11           | 12                   |
| Paired / unpaired / ambiguous | 18 / 2 / 2 | 18 / 4 / 2  |

PR6's Hoffmann fix correctly classifies the two A z=1 sub-triangles
ASYMMETRICALLY (one Inside, one Outside) — which is GEOMETRICALLY
consistent with the malformed overlap polygon: the overlap region in
B's frame is `[0,1] × [-1,0]`, which intersects A's actual footprint
`[0,1] × [0,1]` only along the line segment y=0. So one of A's z=1
sub-triangles overlaps the (broken) injected canonical region (→
boundary-coincident → Inside under Hoffmann), and the other doesn't (→
Outside → survives, contributing the 23rd triangle and the 4-unpaired
half-edge count).

### PR8 scope (proposed)

1. Audit `extract_face_boundary_2d` and `compute_plane_basis` in
   `crates/kernel/src/boolean/coplanar_preprocess.rs` for the
   anti-parallel (`same_dir = false`) case. Confirm whether the 2D
   basis chosen for B's face cancels out via `verts_2d_to_3d`'s reverse
   or whether the bug is in the basis itself.
2. Add a regression test asserting that for the stacked-box canary,
   the inject overlap polygon vertices ALL lie within A's true 2D
   footprint `[0,1]²` (or B's, whichever the canonical frame is).
3. Re-run the canary; with both PR6 (Hoffmann) and PR8 (correct inject
   geometry), it MUST flip RED→GREEN.

### Files PR8 will likely touch

- `crates/kernel/src/boolean/coplanar_preprocess.rs` (basis +
  boundary-extraction)
- Regression test in `coplanar_preprocess.rs` `mod tests`

## References

- Yang et al. 2025 [#24] §4.5.5 — the paper, the spec.
- Yang et al. 2025 [#24] §4.4.2 — binary inside/outside classification
  (PR6 deliverable, RESOLVED).
- Hoffmann 1989 §5.3 — perturb-and-classify, the canonical CSG
  technique for boundary-coincident classification (PR6 implementation
  basis).
- Cherchi et al. 2020 §5.4 — coplanar triangle handling reference.
- `specs/yang_coplanar_preprocessing.md` — the original Stage 0 design.
- `specs/yang_hybrid_migration.md` — overall pipeline migration plan.
- `/tmp/pr4_phaseB.log` — full diagnostic trace (361 lines, ephemeral).
- PR4 commit (coplanar-preprocess-555) — Phase A counters.
- PR5 commit (coplanar-identical-footprint) — §4.5.5 mesh-injection.
- PR6 commit (label-cells-hoffmann) — Hoffmann perturb-and-classify.
