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
- **PR8 outcome**: Anti-parallel polygon-winding gap RESOLVED.
  `split_brep_for_coplanar_pairs` and `inject_partial_overlap_mesh` now
  reverse face B's 2D polygon (`poly_b.reverse()`) when
  `!pair.same_direction` so both polygons walk CCW in A's basis frame
  before entering i_overlay. `inject_identical_footprint_mesh` was
  audited and required no change (it extracts only face A's boundary;
  the per-triangle B-winding flip at line 958 already handles the
  anti-parallel case at the triangle layer). Regression test
  `test_anti_parallel_polygon_winding_canonical` is the green
  deliverable — it asserts that `signed_area_2d(poly_a)` and
  `signed_area_2d(poly_b)` have matching signs after the reversal. The
  PR8 fix reduced the canary's residual unpaired half-edges from 4 → 2
  and surviving sub-tris from 23 → 22, but did NOT close the canary.
  Commit: `<TBD-by-team-lead-at-merge>`.
- **PR9 follow-up — STILL PENDING**: cosurface-annihilation
  incompleteness post PR5 inject. The canary's residual 2 unpaired + 2
  ambiguous half-edges trace to Cherchi STAGE2 not deduplicating PR5's
  canonical-injected triangles. See "PR9 follow-up" below.
- **Canary status**: `test_stacked_box_union_correct_topology` STAYS
  RED until PR9 closes cosurface annihilation. Do NOT silence the
  canary; it now tracks the next compliance gap (PR3 surfaced
  twin-pairing; PR5 improved to 2 ambiguous + 2 unpaired; PR6 added
  correct Hoffmann classification; PR8 closed the polygon-winding gap;
  PR9 will close cosurface annihilation).
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

## PR8 resolution — anti-parallel polygon-winding fix

The original "PR8 follow-up" framing in PR4-PR6 of this spec interpreted
the canary's overlap-polygon vertices `(0,-1), (1,-1)` as outside A's
`[0,1]²` XY footprint and called it an inject-geometry bug. **That
interpretation was wrong.** Direct instrumentation of `poly_a` and
`poly_b` during PR8 surfaced the actual mechanism.

### Audit correction

`compute_plane_basis([0,0,1])` returns `u_axis = [0,1,0]` (Y-axis) and
`v_axis = [-1,0,0]` (-X-axis). Under this basis, A's `[0,1]³` z=1 face
projects to `u∈[0,1], v∈[-1,0]`. The `(0,-1), (1,-1)` overlap vertices
are inside A's actual footprint **in this basis**, not outside. The
PR4-PR6 audit assumed `[0,1]²` was the canonical 2D footprint and
flagged any negative coordinate as a bug; the actual geometry is
basis-dependent.

### Real bug — polygon-winding mismatch

`pair.plane_normal` is always face A's outward normal (per
`detect_coplanar_face_pairs`). `compute_plane_basis(pair.plane_normal)`
derives the shared 2D basis from A's frame. `collect_face_loop_2d` /
`extract_face_boundary_2d` walk each face's loop in its STORED order:
B-Rep half-edge order for `collect_face_loop_2d`, and CCW-winding-
order for the mesh-triangle boundary chains in `extract_face_boundary_2d`.

For anti-parallel pairs (`same_direction = false`), B's outward normal
is opposite to A's. B's loop walks CCW-from-(-A's-normal) = CW in A's
basis. Without correction, A's polygon is CCW and B's polygon is CW
in the shared frame. i_overlay's `Intersect` / `Difference` with
`FillRule::EvenOdd` treats one CCW input and one CW input as
outer-vs-hole, not two outer contours, producing inconsistent boolean
output that propagates through the inject pipeline as residual
unpaired half-edges at the boundary corners.

### PR8 fix

Three sites in `crates/kernel/src/boolean/coplanar_preprocess.rs`:

1. **`split_brep_for_coplanar_pairs` (~line 219)**: after
   `collect_face_loop_2d` for face B, `if !pair.same_direction {
   poly_b.reverse(); }`.

2. **`inject_identical_footprint_mesh` (~line 920)**: AUDITED, NO
   CHANGE NEEDED. This helper extracts ONLY face A's boundary
   (`extract_face_boundary_2d` returns `poly_a`; no `poly_b`). It
   triangulates `poly_a` once and copies the canonical 3D vertex set
   to BOTH meshes via `replace_face_triangles`. The B-winding flip
   happens at the per-triangle level (line 958
   `[t[0], t[2], t[1]]`), not on a 2D polygon.

3. **`inject_partial_overlap_mesh` (~line 1077)**: same pattern as
   site 1 — `poly_b.reverse()` when `!pair.same_direction`.

Each call site cites Yang §4.5.5 + Fig. 16 ("The common part and the
other two parts share identical sampling points on their
boundaries.") inline.

### Regression test — `test_anti_parallel_polygon_winding_canonical`

FIP §8 red-before-green deliverable. Asserts that `poly_a` and
`poly_b` have matching signed-area signs in the shared 2D basis after
the reversal. Basis-coordinate-independent: avoids the trap the
PR4-PR6 audit fell into. Mathematically reliable — i_overlay needs
both inputs CCW (or both CW) regardless of the basis sign convention.

Red phase (with the reversal commented out):

```
assertion `left == right` failed: ...
Got area_a=1.000000, area_b=-1.000000
  left: 1.0
 right: -1.0
```

Green phase (with the reversal applied): both areas have signum `+1`.

### Canary impact

PR8 reduced the canary's residual symptom but did NOT close it.

| Metric                          | PR6 baseline | PR8 (this PR) |
|---------------------------------|--------------|---------------|
| label_cells A inside / outside  | 1 / 11       | 2 / 10        |
| label_cells B inside / outside  | 0 / 12       | 0 / 12        |
| Surviving sub-tris              | 23           | 22            |
| Patches                         | 12           | 11            |
| Paired / unpaired / ambiguous   | 18 / 4 / 2   | 18 / 2 / 2    |
| Validation failure              | `half_edge[2].twin = 0 but twin.twin = 0 (expected 2)` | `half_edge[18].twin = 0 but twin.twin = 16 (expected 18)` |

50% reduction in unpaired half-edges. One additional A z=1 sub-tri
correctly classified `Inside` under PR6's Hoffmann classifier (now
that the polygon-winding alignment lets `label_sub_tri_raycast` see
the canonical injected boundary on the right side). Per
`feedback_no_last_bug.md`: PR8 is NOT "the fix" for the canary. It
closes the polygon-winding gap, full stop. The canary's residual
failure tracks PR9.

## PR9 follow-up — cosurface annihilation incompleteness

After PR8, the canary still fails with 2 unpaired + 2 ambiguous
half-edges:

```
[topo-extract] ambiguous twin for (VertexIdx(4) -> VertexIdx(5)): 2 reverse candidates
[topo-extract] unpaired forward HE (VertexIdx(4) -> VertexIdx(6)): no reverse candidate
[topo-extract] ambiguous twin for (VertexIdx(5) -> VertexIdx(7)): 2 reverse candidates
[topo-extract] unpaired forward HE (VertexIdx(6) -> VertexIdx(7)): no reverse candidate
```

Both ambiguous events report 2 reverse candidates on A's z=1 corner
edges — the signature of two surviving boundary triangles sharing an
edge.

### Hypothesis

PR5's `inject_identical_footprint_mesh` produces "identical meshes"
in the bitwise-vertex-position sense (PR5's
`test_identical_footprint_inject_produces_consistent_meshes`
asserts this). But the mesh-level boolean (Cherchi STAGE2 dedup)
deduplicates triangles by some other recognition logic — possibly
canonical-triangle-key based, which is sensitive to vertex order
within the triangle, not just the position-multiset. PR5's per-
triangle winding flip (line 958: `[t[0], t[2], t[1]]`) writes the
B-side triangles with reversed indices but identical positions; if
STAGE2's recognition keys on `(idx0, idx1, idx2)` rather than the
unordered position-set, it sees two distinct triangles instead of one
canonical pair, and both survive into the patch graph.

Survival data confirms 11 patches (vs the expected ≤6 for a stacked
elongated box), one of which corresponds to the un-annihilated z=1
canonical surface. With both A's and B's z=1 canonical tris in the
patch graph, half-edge twin pairing sees 2 candidates per boundary
edge → ambiguous, and the boundary corners produce orphan unpaired
HEs.

### Diagnostic data (post-PR8 canary)

```
[cherchi-trace] STAGE1 merge: 12 verts, 24 tris
[cherchi-trace] STAGE2 degenerate: 22 tris       # only 2 dupes dropped (expected 4)
[yang-diag] after subdivide: tris_a=12, tris_b=12, verts=12
[yang-diag] after label_cells: A outside=10 inside=2 cosurface=0,
                                B outside=12 inside=0 cosurface=0
[yang-diag] after survival: 11 groups, 22 tris
[topo-extract] summary: paired=18, unpaired=2, ambiguous=2
A ∪ B should succeed: NotSupported {
  operation: "yang_boolean: result validation failed:
              half_edge[18].twin = 0 but twin.twin = 16 (expected 18)" }
```

`cosurface=0` for both A and B — STAGE2 is dedup'ing some pairs but
NOT classifying the remaining z=1 boundary tris as cosurface
(annihilable) duplicates.

### PR9 scope (proposed)

1. Investigate Cherchi STAGE2 dedup in
   `crates/kernel/src/boolean/cherchi/processing.rs` (or wherever
   STAGE2's degenerate-triangle classification lives). Determine
   whether it uses canonical-vertex-key or position-multiset matching.
2. If keying on vertex-index order, change PR5's inject so the per-
   triangle B winding flip preserves the canonical key; OR add a
   cosurface-annihilation pass that operates on position-multisets
   independent of triangle winding.
3. Validate that `test_stacked_box_union_correct_topology` flips
   RED→GREEN with PR9.

### Files PR9 will likely touch

- `crates/kernel/src/boolean/cherchi/processing.rs` (STAGE2 dedup
  logic)
- `crates/kernel/src/boolean/coplanar_preprocess.rs` (PR5 winding-
  flip mechanics — only if the fix needs to live there instead of
  Cherchi)
- Regression test in `coplanar_preprocess.rs` `mod tests` (or
  Cherchi tests)

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
- PR8 commit (coplanar-polygon-winding) — anti-parallel polygon-
  winding reversal in `split_brep_for_coplanar_pairs` and
  `inject_partial_overlap_mesh`.
