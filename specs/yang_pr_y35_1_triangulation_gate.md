# PR-Y35.1 — Widen `triangulation` gate to include triangles with non-empty `edge2pts`

**Authors:** spec-y35-1, canary-y35-1
**Date:** 2026-05-12
**Parent commit:** `248dae7` (PR-Y35 audit — ACCEPT — cinolib semantics re-port validated)
**Team:** pr-y35-1
**Sub-anchor:** PR-Y35 §5.3 banked → unwound here

---

## §1 Context

PR-Y35 (shipped `063304b`, 2026-05-12) re-ported `triangles_intersect_exact` at
`crates/kernel/src/boolean/cherchi/intersection_class.rs:1465-1551` to mirror
cinolib's `Triangle::intersects_triangle(_, ignore_if_valid_complex=true)`
semantics. F0020 STAGE4 inv1: 365 → 84 pairs (exact byte parity with Cherchi
C++ — strongest single-PR signal in 10+ PR cycles).

Side effect: `test_subdivision_shared_edge_split_propagation`
(`crates/kernel/src/boolean/exact_mesh.rs:5403-5469`) was annotated
`#[ignore = "PR-Y35.1 banked — subdivide_mesh_pair shared-edge propagation"]`.
The test exercises conformal subdivision through a same-mesh shared edge
crossed by mesh B; under the cinolib-correct predicate, the same-mesh pair
(T0_A, T1_A) correctly returns SIMPLICIAL_COMPLEX, and the sibling — whose
`edge2pts` entry on the shared edge is correctly populated by
`classify_intersections::add_vertex_in_edge` (`intersection_class.rs:1197-1213`,
898-907) — never reaches `triangulate_single_triangle`, breaking conformal
subdivision.

PR-Y35 §5.3 / §6 banked PR-Y35.1: widen the triangulation-stage gate so a
triangle is added to `tris_to_split` when ANY of its 3 edges has a non-empty
`edge_points_list` from the global `edge2pts` map. PR-Y35.1 unwinds the
`#[ignore]` and re-enables the test as load-bearing regression coverage
(FIP §4). Canary-y35-1 (`docs/audits/pr_y35_1_canary.md` §0) verified all 11
gates GREEN, with kernel lib full suite 1260/24/43 → **1261/24/42** exactly as
the plan predicted; failed-name set byte-identical to PR-Y35 24-name baseline.

---

## §2 Why — Cherchi 2022 §3 segment-insertion contract

Cherchi 2022 §3 (`refs/text/cherchi2022_interactive_robust_mesh_booleans.txt:315-319`)
states the segment-insertion responsibility:

> *"Inserting a segment amounts to eliminating, from the current tessellation,
> all triangles that conflict with it, and then re-triangulate the so-generated
> polygonal pocket, while making sure that the wanted segment is part of the
> new tessellation."*

The well-formed simplicial-complex output guarantee (lines 249-256) requires
that every triangle sharing an edge with a split incorporates that split. The
detection stage's job is to identify proper-interior intersections; the
**triangulation stage** owns segment-insertion across shared edges via the
global `edge2pts` map.

**Pre-PR-Y35.1 mechanism gap.** `classify_intersections` (correct) populates
`aux.edge2pts[edge_id]` via `add_vertex_in_edge` whenever a cross-mesh
intersection lands on any edge — including a same-mesh shared edge.
`triangulate_single_triangle` (correct) reads
`aux.edge_points_list(e0/e1/e2_id)` at `triangulation.rs:262-264` once a
triangle enters. The **GATE at `triangulation.rs:155-159` (defective)**
consults only `triangle_has_intersections` (set by `aux.intersection_list`
membership) and `triangle_has_coplanars`. It NEVER consults `edge2pts`. A
triangle whose flag is never set but whose edge has been split by a sibling
falls through to passthrough and emits un-split.

**Cherchi C++ comparison (canary §4.3, load-bearing per
`feedback_external_coherence`).** Canary-y35-1 read
`~/cherchi2022/InteractiveAndRobustMeshBooleans/arrangements/code/triangulation.cpp:145-150`:

```cpp
for(uint t_id = 0; t_id < ts.numTris(); t_id++)
{
    if(g.triangleHasIntersections(t_id) || g.triangleHasCoplanars(t_id))
        tris_to_split.push_back(t_id);
    else { /* passthrough */ }
}
```

**Cherchi C++ does NOT widen the gate by `edge2pts`** — same predicate
Waffle had pre-PR-Y35.1. Empirically Cherchi C++ produces correct shared-edge
conformal output on F0020 (STAGE4 84/84 byte parity) because in real corpus
geometry mesh B has many triangles, each intersecting multiple A-triangles
bulk-wise — every A-triangle adjacent to a split shared edge gets its flag
set **redundantly** through some other cross-mesh pair. The synthetic diamond
fixture in `test_subdivision_shared_edge_split_propagation` is the degenerate
case Cherchi C++'s regression suite does not exercise.

**PR-Y35.1's edge2pts-widening is a paper-grounded strict superset of Cherchi
C++'s observed behavior:** it adds triangles to `tris_to_split` whose
`edge2pts` data already exists (otherwise the split would be dropped), never
causes invalid triangulation work, and is paper-cited per Cherchi 2022 §3.
Empirically zero-impact on real corpus — canary Gate 6 STAGE4 byte parity
84/84 preserved; Gate 8 F0044 136/136 preserved — the widening fires zero
additional triangles on corpus cases.

---

## §3 Fix shape — verbatim diff

`crates/kernel/src/boolean/cherchi/triangulation.rs:155-184`, baseline
(parent `248dae7`) → post-fix:

```diff
     #[allow(clippy::needless_range_loop)]
     for t_id in 0..ts.num_tris() {
-        if (aux.triangle_has_intersections(t_id) && aux.triangle_has_actual_intersection_data(t_id))
-            || aux.triangle_has_coplanars(t_id)
-        {
+        let flagged = (aux.triangle_has_intersections(t_id)
+            && aux.triangle_has_actual_intersection_data(t_id))
+            || aux.triangle_has_coplanars(t_id);
+
+        // PR-Y35.1: widen gate to include triangles whose edges carry
+        // intersection points from a sibling (same- or cross-mesh) pair.
+        // Cherchi 2022 §3 segment-insertion contract: `edge2pts` is a global
+        // map, and conformal subdivision requires every triangle sharing an
+        // edge with a split to incorporate that split — even if the triangle
+        // itself has no cross-mesh pair (e.g. its sibling across the shared
+        // edge does, and the cinolib-correct predicate rejected the same-mesh
+        // pair as SIMPLICIAL_COMPLEX).
+        let has_edge_split = !flagged && {
+            let v0 = ts.tri_vert_id(t_id, 0);
+            let v1 = ts.tri_vert_id(t_id, 1);
+            let v2 = ts.tri_vert_id(t_id, 2);
+            let edge_has_pts = |a, b| {
+                ts.edge_id(a, b)
+                    .map(|e| !aux.edge_points_list(e).is_empty())
+                    .unwrap_or(false)
+            };
+            edge_has_pts(v0, v1) || edge_has_pts(v1, v2) || edge_has_pts(v2, v0)
+        };
+
+        if flagged || has_edge_split {
             tris_to_split.push(t_id);
         } else {
             // Triangle without intersections directly goes to the output list
```

Plus removal of `#[ignore = "PR-Y35.1 banked — subdivide_mesh_pair shared-edge
propagation"]` at `exact_mesh.rs:5418` and docstring update from "IGNORED post
PR-Y35" to "RE-ENABLED by PR-Y35.1". Canary §1 numstat (production code):
**+35 / -19 LOC** across `triangulation.rs` (+25/-3) and `exact_mesh.rs`
(+10/-16).

Hot-path-overhead guard: the `has_edge_split` closure only fires for
`!flagged` triangles (which would otherwise be passthrough), so it adds zero
work to already-flagged triangles. The 3 edge lookups are O(1) hash-map
fetches in the existing `edge2pts` structure.

---

## §4 Empirical evidence (canary §3)

| Gate | Quantity | PR-Y35 baseline | Post-PR-Y35.1 | Δ | Verdict |
|---|---|---|---|---|---|
| 1 | Build | clean | clean | — | GREEN |
| 2 | `test_subdivision_shared_edge_split_propagation` | IGNORED | **PASS** | +1 / -1 IGN | **GREEN (re-enabled)** |
| 3 | `test_triangles_intersect_exact_*` (6 tests) | 6/6 PASS | 6/6 PASS | 0 | GREEN |
| 4 | `test_detect_intersections_shared_vertex_cross_mesh_l_corner` | PASS | PASS | 0 | GREEN |
| 5 | Cherchi lib tests | 80 PASS / 2 IGN | 80 PASS / 2 IGN | 0 | GREEN |
| 6 | F0020 STAGE4 inv1 pair count | 84 | **84** | 0 | **GREEN (PR-Y35 parity)** |
| 6 | F0020 STAGE4 inv0 pair count | 20 | 20 | 0 | GREEN |
| 7 | F0020 Stage B missing / extras / common | 7 / 0 / 230 | 7 / 0 / 230 | 0 | GREEN |
| 8 | F0044 missing / extras / common | 0 / 0 / 136 | **0 / 0 / 136** | 0 | **GREEN (hard gate)** |
| 9 | F0045 missing / extras | 236 / 466 | 236 / 466 | 0 | GREEN |
| 9 | R0092 missing / extras | 192 / 368 | 192 / 368 | 0 | GREEN |
| 10 | yang_fast pass count | 10/157 | 10/157 | 0 | GREEN |
| 11 | Kernel lib passed / failed / ignored | 1260 / 24 / 43 | **1261 / 24 / 42** | +1 / 0 / −1 | **GREEN (plan-predicted)** |
| 11 | Failed-name set | 24-name baseline | identical 24-name set | ∅ | GREEN (no new RED) |

Highlights:
- **Gate 2** — load-bearing acceptance gate flips IGNORED → PASS as predicted.
- **Gate 6** — STAGE4 byte parity preserved at 84/84; the gate-widening adds
  zero triangles to corpus `tris_to_split` (F0020 has redundant cross-mesh
  flagging).
- **Gate 8** — F0044 byte parity hard gate preserved at 136/136.
- **Gate 11** — kernel lib lands at exactly 1261/24/42, the plan's predicted
  net-+1 state. Failed-name set byte-identical to PR-Y35 24-name baseline
  (canary §3 lists all 24); no new RED.

Wrong-anchor count for PR-Y35.1: **0/1** — planning anchor
(`triangulation.rs:155-159` is the bottleneck) matched fix shape (option (b)
widen by edge2pts) without canary-stage refutation. First clean
single-cycle PR in 10+ cycles.

---

## §5 Regression coverage

`test_subdivision_shared_edge_split_propagation`
(`crates/kernel/src/boolean/exact_mesh.rs:5403-5469`) is **RE-ENABLED** by
PR-Y35.1 as the load-bearing FIP §4 regression coverage:

- **RED on baseline** (parent `248dae7` post-PR-Y35): un-ignoring on baseline
  produces FAIL with `t0_count > 1 && t1_count > 1` violated (sibling T1_A
  passthrough'd un-split because gate consults only flag, not `edge2pts`).
- **GREEN with fix:** widened gate adds T1_A to `tris_to_split` because edge
  `(v1, v2)` has non-empty `edge_points_list` from T0_A's classification
  call. T1_A enters `triangulate_single_triangle` and produces the conformal
  split. Test asserts both `t0_count > 1` AND `t1_count > 1` — PASS.

End-to-end exercise of exactly the gate-widening path
(detection → edge2pts population → triangulation gate consultation →
subdivision propagation). test-y35-1 may optionally add a focused unit test
on `triangulation.rs` gate behavior; defer decision to their report — the
re-enabled end-to-end test provides sufficient coverage of the user-visible
behavior.

The 6 PR-Y35 `test_triangles_intersect_exact_*` unit tests preserve PASS
post-PR-Y35.1 (Gate 3). PR-Y35.1 widens the **gate** in the triangulation
stage; the detection predicate is untouched.

---

## §6 Out of scope (banked, unchanged from PR-Y35 §6)

PR-Y35.1 ships only the triangulation-stage gate widening. Open architectural
anchors:

1. **F0020 Render-LOD downstream Status:Failed** — ~40 unpaired edges at the
   render layer; same defect class as F0044 Status:Failed and the F0020
   missing=7 residual. Rightful PR-Y36+ anchor.
2. **F0045 tessellation-grid divergence (Yang §4.1.1)** — missing=236 /
   extras=466; Stage 1 tessellation grid (PR-Y30 banked); independent.
3. **R0092 NMM-edge tessellation gap (PR-Y27 §D.3)** — missing=192; Stage 1
   NMM-edge tessellation defect; independent.
4. **139 still-failing yang_fast cases** — corpus aggregate 10/157
   preserved; remaining 139 fail at downstream stages unaffected by the
   triangulation-stage gate widening.
5. **24 kernel lib failures** — all pre-existing post-PR-Y35; all at
   downstream B-Rep assembly stages, not the cherchi triangulation pass
   (canary §3 lists all 24).

PR-Y35.1 closes one sub-anchor within one stage (Stage 6, triangulation) of
one pipeline (Yang hybrid boolean) of one feature class (mesh booleans). Many
architectural anchors remain.

---

## §7 Risk / mitigation

**Risk.** Gate widening could add triangles to `tris_to_split` on real
corpus, slowing triangulation. In the worst case the closure would fire for
every triangle whose adjacent triangle is flagged.

**Mitigation — empirical (canary, load-bearing).** ZERO additional triangles
added to `tris_to_split` on F0020 / F0044 / F0045 / R0092:
- Gate 6 F0020 STAGE4 byte parity 84/84 preserved (PR-Y35 win intact).
- Gate 8 F0044 byte parity 0/0/136 preserved.
- Gate 9 cohort missing-counts preserved at 236 / 192.
- Gate 10 yang_fast 10/157 preserved.

The widening fires only in degenerate configurations like the diamond fixture
(single B-triangle, single cross-mesh pair, sibling A-triangle never flagged
otherwise). Residual risk is bounded by cases not exercised by F0020 + cohort
+ yang_fast — Gate 10/11 corpus measurement is the bound.

**Mitigation — paper.** Widening is a strict superset of Cherchi C++'s
observed behavior (canary §4.3), paper-cited per Cherchi 2022 §3
segment-insertion contract. Adding a triangle with non-empty `edge2pts` to
`tris_to_split` is paper-correct (every triangle sharing an edge with a split
must incorporate the split); it cannot produce invalid triangulation output.
Worst case is extra harmless work, not incorrect output.

---

*End of spec.*
